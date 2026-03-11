use std::collections::HashMap;

use super::package::PackageInfo;
use super::package_source::PackageSource;

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
                    let mut args = vec![
                        "pacman".to_string(),
                        "-Rns".to_string(),
                        "--noconfirm".to_string(),
                    ];
                    args.extend(names.clone());
                    cmds.push(RemovalCommand {
                        source: *source,
                        display: format!("pkexec pacman -Rns --noconfirm {}", names.join(" ")),
                        program: "pkexec".to_string(),
                        args,
                        packages: names,
                    });
                }
                PackageSource::Flatpak => {
                    for pkg in pkgs {
                        cmds.push(RemovalCommand {
                            source: *source,
                            display: format!("flatpak uninstall -y {}", pkg.name),
                            program: "flatpak".to_string(),
                            args: vec!["uninstall".to_string(), "-y".to_string(), pkg.name.clone()],
                            packages: vec![pkg.name.clone()],
                        });
                    }
                }
                PackageSource::AppImage | PackageSource::Wine | PackageSource::Desktop => {
                    let mut root_paths: Vec<String> = Vec::new();
                    let mut root_names: Vec<String> = Vec::new();
                    let mut user_paths: Vec<String> = Vec::new();
                    let mut user_names: Vec<String> = Vec::new();

                    for pkg in pkgs {
                        if let Some(path) = &pkg.install_path {
                            let path_str = path.display().to_string();
                            let needs_root = path.starts_with("/opt")
                                || path.starts_with("/usr")
                                || path.starts_with("/bin")
                                || path.starts_with("/sbin");
                            if needs_root {
                                root_paths.push(path_str);
                                root_names.push(pkg.name.clone());
                            } else {
                                user_paths.push(path_str);
                                user_names.push(pkg.name.clone());
                            }
                        }
                    }

                    if !root_paths.is_empty() {
                        let mut args = vec!["rm".to_string(), "-rf".to_string()];
                        args.extend(root_paths.clone());
                        cmds.push(RemovalCommand {
                            source: *source,
                            display: format!("pkexec rm -rf {}", root_paths.join(" ")),
                            program: "pkexec".to_string(),
                            args,
                            packages: root_names,
                        });
                    }
                    if !user_paths.is_empty() {
                        let mut args = vec!["-rf".to_string()];
                        args.extend(user_paths.clone());
                        cmds.push(RemovalCommand {
                            source: *source,
                            display: format!("rm -rf {}", user_paths.join(" ")),
                            program: "rm".to_string(),
                            args,
                            packages: user_names,
                        });
                    }
                }
                PackageSource::Snap => {
                    for pkg in pkgs {
                        cmds.push(RemovalCommand {
                            source: *source,
                            display: format!("snap remove {}", pkg.name),
                            program: "snap".to_string(),
                            args: vec!["remove".to_string(), pkg.name.clone()],
                            packages: vec![pkg.name.clone()],
                        });
                    }
                }
                PackageSource::Pip => {
                    let names: Vec<String> = pkgs.iter().map(|p| p.name.clone()).collect();
                    let mut args = vec!["uninstall".to_string(), "-y".to_string()];
                    args.extend(names.clone());
                    cmds.push(RemovalCommand {
                        source: *source,
                        display: format!("pip uninstall -y {}", names.join(" ")),
                        program: "pip".to_string(),
                        args,
                        packages: names,
                    });
                }
                PackageSource::Npm => {
                    let names: Vec<String> = pkgs.iter().map(|p| p.name.clone()).collect();
                    let mut args = vec!["uninstall".to_string(), "-g".to_string()];
                    args.extend(names.clone());
                    cmds.push(RemovalCommand {
                        source: *source,
                        display: format!("npm uninstall -g {}", names.join(" ")),
                        program: "npm".to_string(),
                        args,
                        packages: names,
                    });
                }
                PackageSource::Cargo => {
                    for pkg in pkgs {
                        cmds.push(RemovalCommand {
                            source: *source,
                            display: format!("cargo uninstall {}", pkg.name),
                            program: "cargo".to_string(),
                            args: vec!["uninstall".to_string(), pkg.name.clone()],
                            packages: vec![pkg.name.clone()],
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
}
