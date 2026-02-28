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
                    let names: Vec<&str> = pkgs.iter().map(|p| p.name.as_str()).collect();
                    cmds.push(RemovalCommand {
                        source: *source,
                        command: format!("pkexec pacman -Rns --noconfirm {}", names.join(" ")),
                        packages: names.iter().map(|s| s.to_string()).collect(),
                    });
                }
                PackageSource::Flatpak => {
                    for pkg in pkgs {
                        cmds.push(RemovalCommand {
                            source: *source,
                            command: format!("flatpak uninstall -y {}", pkg.name),
                            packages: vec![pkg.name.clone()],
                        });
                    }
                }
                PackageSource::AppImage | PackageSource::Wine | PackageSource::Desktop => {
                    for pkg in pkgs {
                        if let Some(path) = &pkg.install_path {
                            let path_str = path.display();
                            // Use pkexec if in /opt or other system dirs
                            let cmd = if path.starts_with("/opt") {
                                format!("pkexec rm -rf {path_str}")
                            } else {
                                format!("rm -rf {path_str}")
                            };
                            cmds.push(RemovalCommand {
                                source: *source,
                                command: cmd,
                                packages: vec![pkg.name.clone()],
                            });
                        }
                    }
                }
                PackageSource::Snap => {
                    for pkg in pkgs {
                        cmds.push(RemovalCommand {
                            source: *source,
                            command: format!("snap remove {}", pkg.name),
                            packages: vec![pkg.name.clone()],
                        });
                    }
                }
                PackageSource::Pip => {
                    let names: Vec<&str> = pkgs.iter().map(|p| p.name.as_str()).collect();
                    cmds.push(RemovalCommand {
                        source: *source,
                        command: format!("pip uninstall -y {}", names.join(" ")),
                        packages: names.iter().map(|s| s.to_string()).collect(),
                    });
                }
                PackageSource::Npm => {
                    let names: Vec<&str> = pkgs.iter().map(|p| p.name.as_str()).collect();
                    cmds.push(RemovalCommand {
                        source: *source,
                        command: format!("npm uninstall -g {}", names.join(" ")),
                        packages: names.iter().map(|s| s.to_string()).collect(),
                    });
                }
                PackageSource::Cargo => {
                    for pkg in pkgs {
                        cmds.push(RemovalCommand {
                            source: *source,
                            command: format!("cargo uninstall {}", pkg.name),
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
    pub command: String,
    pub packages: Vec<String>,
}
