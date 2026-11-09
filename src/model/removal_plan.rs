use std::collections::HashMap;
use std::path::PathBuf;

use super::package::PackageInfo;
use super::package_source::PackageSource;
use donutcleaner_ops::{MAX_PACKAGES, MAX_PATHS, Op, check_package_name, is_protected_package};

use crate::remover::{helper, safe};

/// Groups packages by source for removal
pub struct RemovalPlan {
    pub groups: HashMap<PackageSource, Vec<PackageInfo>>,
    pub total_size: u64,
}

impl RemovalPlan {
    pub fn from_packages(packages: Vec<PackageInfo>) -> Self {
        let total_size = packages.iter().map(|p| p.installed_size).sum();
        let mut groups: HashMap<PackageSource, Vec<PackageInfo>> = HashMap::new();

        for pkg in packages {
            groups.entry(pkg.source).or_default().push(pkg);
        }

        Self { groups, total_size }
    }

    pub fn total_packages(&self) -> usize {
        self.groups.values().map(|v| v.len()).sum()
    }

    /// Generate human-readable summary of what will be removed
    pub fn summary_lines(&self) -> Vec<String> {
        let mut lines = Vec::new();
        for (source, pkgs) in &self.groups {
            lines.push(format!("[{}] {} package(s):", source.label(), pkgs.len()));
            for pkg in pkgs {
                lines.push(format!(
                    "  - {} {}",
                    pkg.name,
                    bytesize::ByteSize(pkg.installed_size)
                ));
            }
        }
        lines.push(format!(
            "\nTotal: {} packages, {} freed",
            self.total_packages(),
            bytesize::ByteSize(self.total_size)
        ));
        lines
    }

    /// Generate the removal commands
    pub fn commands(&self) -> Vec<RemovalCommand> {
        let mut cmds = Vec::new();

        for (source, pkgs) in &self.groups {
            match source {
                PackageSource::Pacman | PackageSource::Aur => {
                    let mut names: Vec<String> = Vec::new();
                    for pkg in pkgs {
                        // The root helper checks these again. Refusing here keeps one bad
                        // name from sinking the whole batch.
                        if let Err(reason) = check_package_name(&pkg.name) {
                            cmds.push(RemovalCommand::refused(*source, pkg.name.clone(), reason));
                        } else if is_protected_package(&pkg.name) {
                            cmds.push(RemovalCommand::refused(
                                *source,
                                pkg.name.clone(),
                                format!("{} is a protected system package", pkg.name),
                            ));
                        } else {
                            names.push(pkg.name.clone());
                        }
                    }
                    for chunk in names.chunks(MAX_PACKAGES) {
                        cmds.push(RemovalCommand::helper(
                            *source,
                            Op::RemovePackages(chunk.to_vec()),
                            chunk.to_vec(),
                        ));
                    }
                }
                PackageSource::Flatpak => {
                    for pkg in pkgs {
                        cmds.push(RemovalCommand {
                            source: *source,
                            display: format!("flatpak uninstall -y {}", pkg.name),
                            program: "flatpak".to_string(),
                            args: vec![
                                "uninstall".to_string(),
                                "-y".to_string(),
                                "--".to_string(),
                                pkg.name.clone(),
                            ],
                            packages: vec![pkg.name.clone()],
                            refused: None,
                            helper: None,
                        });
                    }
                }
                PackageSource::AppImage | PackageSource::Wine | PackageSource::Desktop => {
                    let mut root_paths: Vec<String> = Vec::new();
                    let mut root_names: Vec<String> = Vec::new();
                    let mut user_paths: Vec<PathBuf> = Vec::new();
                    let mut user_names: Vec<String> = Vec::new();

                    for pkg in pkgs {
                        let Some(path) = &pkg.install_path else {
                            continue;
                        };
                        // Resolve and sanity-check the path before it reaches `rm`.
                        let path = match safe::check_app_target(path) {
                            Ok(p) => p,
                            Err(reason) => {
                                cmds.push(RemovalCommand::refused(
                                    *source,
                                    pkg.name.clone(),
                                    reason,
                                ));
                                continue;
                            }
                        };
                        if path.starts_with("/opt") || path.starts_with("/usr/local") {
                            // Root work goes to the helper, which only accepts direct children
                            // of /opt, /usr/local/bin and /usr/local/sbin.
                            match path.to_str().map(|p| Op::parse(&["remove-paths".into(), p.into()])) {
                                Some(Ok(_)) => {
                                    let text = path.to_string_lossy().into_owned();
                                    if !root_paths.contains(&text) {
                                        root_paths.push(text);
                                        root_names.push(pkg.name.clone());
                                    }
                                }
                                _ => cmds.push(RemovalCommand::refused(
                                    *source,
                                    pkg.name.clone(),
                                    format!(
                                        "{} is not directly inside /opt, /usr/local/bin or /usr/local/sbin, \
                                         so the administrator helper will not remove it",
                                        path.display()
                                    ),
                                )),
                            }
                        } else if path.starts_with("/usr")
                            || path.starts_with("/bin")
                            || path.starts_with("/sbin")
                        {
                            cmds.push(RemovalCommand::refused(
                                *source,
                                pkg.name.clone(),
                                format!(
                                    "{} is a system location, remove it with the package manager",
                                    path.display()
                                ),
                            ));
                        } else {
                            user_paths.push(path);
                            user_names.push(pkg.name.clone());
                        }
                    }

                    for (chunk, names) in root_paths
                        .chunks(MAX_PATHS)
                        .zip(root_names.chunks(MAX_PATHS))
                    {
                        cmds.push(RemovalCommand::helper(
                            *source,
                            Op::RemovePaths(chunk.to_vec()),
                            names.to_vec(),
                        ));
                    }
                    if !user_paths.is_empty() {
                        let (program, args) = safe::rm_command(&user_paths);
                        cmds.push(RemovalCommand::new(*source, &program, args, user_names));
                    }
                }
                PackageSource::Snap => {
                    for pkg in pkgs {
                        cmds.push(RemovalCommand {
                            source: *source,
                            display: format!("snap remove {}", pkg.name),
                            program: "snap".to_string(),
                            args: vec!["remove".to_string(), "--".to_string(), pkg.name.clone()],
                            packages: vec![pkg.name.clone()],
                            refused: None,
                            helper: None,
                        });
                    }
                }
                PackageSource::Pip => {
                    let names: Vec<String> = pkgs.iter().map(|p| p.name.clone()).collect();
                    let mut args =
                        vec!["uninstall".to_string(), "-y".to_string(), "--".to_string()];
                    args.extend(names.clone());
                    cmds.push(RemovalCommand {
                        source: *source,
                        display: format!("pip uninstall -y {}", names.join(" ")),
                        program: "pip".to_string(),
                        args,
                        packages: names,
                        refused: None,
                        helper: None,
                    });
                }
                PackageSource::Npm => {
                    let names: Vec<String> = pkgs.iter().map(|p| p.name.clone()).collect();
                    let mut args =
                        vec!["uninstall".to_string(), "-g".to_string(), "--".to_string()];
                    args.extend(names.clone());
                    cmds.push(RemovalCommand {
                        source: *source,
                        display: format!("npm uninstall -g {}", names.join(" ")),
                        program: "npm".to_string(),
                        args,
                        packages: names,
                        refused: None,
                        helper: None,
                    });
                }
                PackageSource::Cargo => {
                    for pkg in pkgs {
                        cmds.push(RemovalCommand {
                            source: *source,
                            display: format!("cargo uninstall {}", pkg.name),
                            program: "cargo".to_string(),
                            args: vec!["uninstall".to_string(), "--".to_string(), pkg.name.clone()],
                            packages: vec![pkg.name.clone()],
                            refused: None,
                            helper: None,
                        });
                    }
                }
            }
        }

        cmds
    }
}

pub struct RemovalCommand {
    pub source: PackageSource,
    /// Human-readable display string
    pub display: String,
    /// Program to execute (ignored when `helper` is set)
    pub program: String,
    /// Arguments (each element is one argument, handles spaces correctly)
    pub args: Vec<String>,
    pub packages: Vec<String>,
    /// Set when the target failed a safety check. The executor reports it and runs nothing.
    pub refused: Option<String>,
    /// Set when the command needs root. The executor sends it to `donutcleaner-helper`.
    pub helper: Option<Op>,
}

impl RemovalCommand {
    /// A command that runs as the current user.
    pub fn new(source: PackageSource, program: &str, args: Vec<String>, packages: Vec<String>) -> Self {
        let display = std::iter::once(program)
            .chain(args.iter().map(String::as_str))
            .map(|a| {
                if a.contains(char::is_whitespace) {
                    format!("{a:?}")
                } else {
                    a.to_string()
                }
            })
            .collect::<Vec<_>>()
            .join(" ");
        Self {
            source,
            display,
            program: program.to_string(),
            args,
            packages,
            refused: None,
            helper: None,
        }
    }

    /// A command that runs as root through the helper.
    pub fn helper(source: PackageSource, op: Op, packages: Vec<String>) -> Self {
        Self {
            source,
            display: helper::display(&op),
            program: String::new(),
            args: Vec::new(),
            packages,
            refused: None,
            helper: Some(op),
        }
    }

    /// A removal that was blocked before running.
    pub fn refused(source: PackageSource, package: String, reason: String) -> Self {
        Self {
            source,
            display: format!("(not run) {package}"),
            program: String::new(),
            args: Vec::new(),
            packages: vec![package],
            refused: Some(reason),
            helper: None,
        }
    }
}
