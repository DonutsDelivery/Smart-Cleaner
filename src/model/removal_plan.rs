use std::collections::HashMap;
use std::path::PathBuf;

use super::package::PackageInfo;
use super::package_source::PackageSource;
use crate::remover::safe;

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
                    let names: Vec<String> = pkgs.iter().map(|p| p.name.clone()).collect();
                    // "--" keeps a name that starts with "-" from being read as an option.
                    let mut args = vec![
                        "-Rns".to_string(),
                        "--noconfirm".to_string(),
                        "--".to_string(),
                    ];
                    args.extend(names.clone());
                    cmds.push(RemovalCommand::new(
                        *source,
                        true,
                        safe::PACMAN,
                        args,
                        names,
                    ));
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
                        });
                    }
                }
                PackageSource::AppImage | PackageSource::Wine | PackageSource::Desktop => {
                    let mut root_paths: Vec<PathBuf> = Vec::new();
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
                        let needs_root = path.starts_with("/opt")
                            || path.starts_with("/usr")
                            || path.starts_with("/bin")
                            || path.starts_with("/sbin");
                        if needs_root {
                            root_paths.push(path);
                            root_names.push(pkg.name.clone());
                        } else {
                            user_paths.push(path);
                            user_names.push(pkg.name.clone());
                        }
                    }

                    if !root_paths.is_empty() {
                        let (program, args) = safe::rm_command(true, &root_paths);
                        cmds.push(RemovalCommand::new(
                            *source, true, &program, args, root_names,
                        ));
                    }
                    if !user_paths.is_empty() {
                        let (program, args) = safe::rm_command(false, &user_paths);
                        cmds.push(RemovalCommand::new(
                            *source, false, &program, args, user_names,
                        ));
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
    /// Program to execute
    pub program: String,
    /// Arguments (each element is one argument, handles spaces correctly)
    pub args: Vec<String>,
    pub packages: Vec<String>,
    /// Set when the target failed a safety check. The executor reports it and runs nothing.
    pub refused: Option<String>,
}

impl RemovalCommand {
    /// Build a command from a program and its arguments. A privileged command runs as
    /// `pkexec <program> <args>`; `display` shows exactly that argv.
    pub fn new(
        source: PackageSource,
        privileged: bool,
        program: &str,
        args: Vec<String>,
        packages: Vec<String>,
    ) -> Self {
        let (program, args) = safe::argv(privileged, program, &args);
        let display = std::iter::once(program.as_str())
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
            program,
            args,
            packages,
            refused: None,
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
        }
    }
}
