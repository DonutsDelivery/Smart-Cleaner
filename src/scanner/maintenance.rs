use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::remover::safe;

/// What to do for one cleanup item. Nothing here is a shell string: programs and
/// arguments stay separate, and paths are checked again right before they are removed.
#[derive(Debug, Clone)]
pub enum CleanAction {
    /// Run a fixed program with fixed arguments.
    Run {
        program: &'static str,
        args: Vec<String>,
        privileged: bool,
    },
    /// Remove these paths. Each must resolve to somewhere strictly inside `root`.
    RemovePaths {
        root: PathBuf,
        paths: Vec<PathBuf>,
        privileged: bool,
    },
    /// Remove everything inside `dir` but keep `dir` itself.
    RemoveContents { dir: PathBuf, privileged: bool },
    /// Remove the packages `pacman -Qdtq` lists at the time of cleaning.
    RemoveOrphans,
}

/// A command ready to run, produced from a [`CleanAction`] just before execution.
#[derive(Debug)]
pub enum Prepared {
    /// `rm -rf -- <paths>`, already validated.
    Remove {
        privileged: bool,
        paths: Vec<PathBuf>,
    },
    Exec {
        privileged: bool,
        program: String,
        args: Vec<String>,
    },
}

impl CleanAction {
    /// Validate and expand the action. `Ok(None)` means there is nothing left to do.
    pub fn prepare(&self) -> Result<Option<Prepared>, String> {
        match self {
            CleanAction::Run {
                program,
                args,
                privileged,
            } => Ok(Some(Prepared::Exec {
                privileged: *privileged,
                program: (*program).to_string(),
                args: args.clone(),
            })),
            CleanAction::RemovePaths {
                root,
                paths,
                privileged,
            } => {
                let mut checked = Vec::new();
                for path in paths {
                    if let Some(p) = safe::check_inside(root, path)? {
                        checked.push(p);
                    }
                }
                Ok(removal(*privileged, checked))
            }
            CleanAction::RemoveContents { dir, privileged } => {
                Ok(removal(*privileged, safe::contents_of(dir)?))
            }
            CleanAction::RemoveOrphans => {
                let output = Command::new("pacman")
                    .arg("-Qdtq")
                    .output()
                    .map_err(|e| format!("cannot run pacman: {e}"))?;
                let names: Vec<String> = String::from_utf8_lossy(&output.stdout)
                    .lines()
                    .map(str::trim)
                    .filter(|l| !l.is_empty() && !l.starts_with('-'))
                    .map(str::to_string)
                    .collect();
                if names.is_empty() {
                    return Ok(None);
                }
                let mut args = vec![
                    "-Rns".to_string(),
                    "--noconfirm".to_string(),
                    "--".to_string(),
                ];
                args.extend(names);
                Ok(Some(Prepared::Exec {
                    privileged: true,
                    program: safe::PACMAN.to_string(),
                    args,
                }))
            }
        }
    }
}

fn removal(privileged: bool, paths: Vec<PathBuf>) -> Option<Prepared> {
    if paths.is_empty() {
        None
    } else {
        Some(Prepared::Remove { privileged, paths })
    }
}

/// A single cleanup task with its current disk usage and cleanup action.
#[derive(Debug, Clone)]
pub struct MaintenanceItem {
    pub id: String,
    pub category: &'static str,
    pub label: String,
    pub size: u64,
    pub action: CleanAction,
    pub checked: bool,
}

/// Scan for all maintenance items with nonzero sizes. Blocking — call from a background thread.
pub fn scan_maintenance_items() -> Vec<MaintenanceItem> {
    let mut items = Vec::new();
    let home = std::env::var("HOME").unwrap_or_default();

    // --- Package manager caches ---

    // Pacman cache (needs paccache from pacman-contrib)
    let has_paccache = Path::new(safe::PACCACHE).exists();
    if let Some(size) = dir_size(Path::new("/var/cache/pacman/pkg")).filter(|_| has_paccache) {
        if size > 0 {
            items.push(MaintenanceItem {
                id: "pacman-cache".into(),
                category: "Package manager caches",
                label: "Pacman cache (remove all versions)".into(),
                size,
                action: CleanAction::Run {
                    program: safe::PACCACHE,
                    args: vec!["-rk0".into()],
                    privileged: true,
                },
                checked: false,
            });
        }
    }

    // Uninstalled package cache
    let uninstalled_size = get_paccache_uninstalled_size();
    if uninstalled_size > 0 && has_paccache {
        items.push(MaintenanceItem {
            id: "pacman-uninstalled".into(),
            category: "Package manager caches",
            label: "Uninstalled package cache".into(),
            size: uninstalled_size,
            action: CleanAction::Run {
                program: safe::PACCACHE,
                args: vec!["-ruk0".into()],
                privileged: true,
            },
            checked: false,
        });
    }

    // AUR build cache (~/.cache/yay/)
    let yay_cache = PathBuf::from(&home).join(".cache/yay");
    if let Some(size) = dir_size(&yay_cache) {
        if size > 0 {
            items.push(MaintenanceItem {
                id: "aur-cache".into(),
                category: "Package manager caches",
                label: "AUR build cache (~/.cache/yay/)".into(),
                size,
                action: CleanAction::RemoveContents {
                    dir: yay_cache.clone(),
                    privileged: false,
                },
                checked: false,
            });
        }
    }

    // Debtap cache
    if let Some(size) = dir_size(Path::new("/var/cache/debtap")) {
        if size > 0 {
            items.push(MaintenanceItem {
                id: "debtap-cache".into(),
                category: "Package manager caches",
                label: "Debtap cache (/var/cache/debtap/)".into(),
                size,
                action: CleanAction::RemoveContents {
                    dir: PathBuf::from("/var/cache/debtap"),
                    privileged: true,
                },
                checked: false,
            });
        }
    }

    // Orphaned packages
    let orphan_size = if Path::new(safe::PACMAN).exists() {
        get_orphan_size()
    } else {
        0
    };
    if orphan_size > 0 {
        items.push(MaintenanceItem {
            id: "orphans".into(),
            category: "Package manager caches",
            label: "Orphaned packages".into(),
            size: orphan_size,
            action: CleanAction::RemoveOrphans,
            checked: false,
        });
    }

    // Flatpak unused runtimes
    let flatpak_size = get_flatpak_unused_size();
    if flatpak_size > 0 {
        items.push(MaintenanceItem {
            id: "flatpak-unused".into(),
            category: "Package manager caches",
            label: "Flatpak unused runtimes".into(),
            size: flatpak_size,
            action: CleanAction::Run {
                program: "flatpak",
                args: vec!["uninstall".into(), "--unused".into(), "-y".into()],
                privileged: false,
            },
            checked: false,
        });
    }

    // --- Dev tool caches ---

    // Cargo registry + git
    let cargo_registry = PathBuf::from(&home).join(".cargo/registry");
    let cargo_git = PathBuf::from(&home).join(".cargo/git");
    let cargo_size = dir_size(&cargo_registry).unwrap_or(0) + dir_size(&cargo_git).unwrap_or(0);
    if cargo_size > 0 {
        items.push(MaintenanceItem {
            id: "cargo-cache".into(),
            category: "Dev tool caches",
            label: "Cargo registry + git cache".into(),
            size: cargo_size,
            action: CleanAction::RemovePaths {
                root: PathBuf::from(&home).join(".cargo"),
                paths: vec![cargo_registry.clone(), cargo_git.clone()],
                privileged: false,
            },
            checked: false,
        });
    }

    // Rust target/ directories
    let rust_targets = find_rust_target_dirs(&home);
    let rust_target_size: u64 = rust_targets.iter().map(|(_, s)| s).sum();
    if rust_target_size > 0 {
        let target_paths: Vec<PathBuf> = rust_targets.iter().map(|(p, _)| p.clone()).collect();
        items.push(MaintenanceItem {
            id: "rust-targets".into(),
            category: "Dev tool caches",
            label: format!("Rust target/ dirs ({} projects)", rust_targets.len()),
            size: rust_target_size,
            action: CleanAction::RemovePaths {
                root: PathBuf::from(&home).join("Programs"),
                paths: target_paths,
                privileged: false,
            },
            checked: false,
        });
    }

    // npm cache
    let npm_cache = PathBuf::from(&home).join(".npm/_cacache");
    if let Some(size) = dir_size(&npm_cache) {
        if size > 0 {
            items.push(MaintenanceItem {
                id: "npm-cache".into(),
                category: "Dev tool caches",
                label: "npm cache".into(),
                size,
                action: CleanAction::Run {
                    program: "npm",
                    args: vec!["cache".into(), "clean".into(), "--force".into()],
                    privileged: false,
                },
                checked: false,
            });
        }
    }

    // pip cache
    let pip_cache = PathBuf::from(&home).join(".cache/pip");
    if let Some(size) = dir_size(&pip_cache) {
        if size > 0 {
            items.push(MaintenanceItem {
                id: "pip-cache".into(),
                category: "Dev tool caches",
                label: "pip cache".into(),
                size,
                action: CleanAction::Run {
                    program: "pip",
                    args: vec!["cache".into(), "purge".into()],
                    privileged: false,
                },
                checked: false,
            });
        }
    }

    // Go build cache
    let go_cache = PathBuf::from(&home).join(".cache/go-build");
    if which::which("go").is_ok() {
        if let Some(size) = dir_size(&go_cache) {
            if size > 0 {
                items.push(MaintenanceItem {
                    id: "go-cache".into(),
                    category: "Dev tool caches",
                    label: "Go build cache".into(),
                    size,
                    action: CleanAction::Run {
                        program: "go",
                        args: vec!["clean".into(), "-cache".into()],
                        privileged: false,
                    },
                    checked: false,
                });
            }
        }
    } else if let Some(size) = dir_size(&go_cache) {
        // go not installed but cache dir exists — clean with rm
        if size > 0 {
            items.push(MaintenanceItem {
                id: "go-cache".into(),
                category: "Dev tool caches",
                label: "Go build cache".into(),
                size,
                action: CleanAction::RemoveContents {
                    dir: go_cache.clone(),
                    privileged: false,
                },
                checked: false,
            });
        }
    }

    // --- App/Electron caches ---

    let (electron_size, electron_dirs) = scan_electron_caches(&home);
    if electron_size > 0 {
        items.push(MaintenanceItem {
            id: "electron-cache".into(),
            category: "App/Electron caches",
            label: "Electron app caches (Discord, Slack, etc.)".into(),
            size: electron_size,
            action: CleanAction::RemovePaths {
                root: PathBuf::from(&home).join(".config"),
                paths: electron_dirs,
                privileged: false,
            },
            checked: false,
        });
    }

    // Tauri cache
    let tauri_cache = PathBuf::from(&home).join(".cache/tauri");
    let temp_tauri = PathBuf::from(&home).join(".cache/temp-tauri");
    let tauri_size = dir_size(&tauri_cache).unwrap_or(0) + dir_size(&temp_tauri).unwrap_or(0);
    if tauri_size > 0 {
        items.push(MaintenanceItem {
            id: "tauri-cache".into(),
            category: "App/Electron caches",
            label: "Tauri cache".into(),
            size: tauri_size,
            action: CleanAction::RemovePaths {
                root: PathBuf::from(&home).join(".cache"),
                paths: vec![tauri_cache.clone(), temp_tauri.clone()],
                privileged: false,
            },
            checked: false,
        });
    }

    // --- System cruft ---

    // Coredumps
    if let Some(size) = dir_size(Path::new("/var/lib/systemd/coredump")) {
        if size > 0 {
            items.push(MaintenanceItem {
                id: "coredumps".into(),
                category: "System cruft",
                label: "Coredumps".into(),
                size,
                action: CleanAction::RemoveContents {
                    dir: PathBuf::from("/var/lib/systemd/coredump"),
                    privileged: true,
                },
                checked: false,
            });
        }
    }

    // Systemd journal
    let journal_size = get_journal_size();
    if journal_size > 0 {
        items.push(MaintenanceItem {
            id: "journal".into(),
            category: "System cruft",
            label: "Systemd journal (keep 1 week)".into(),
            size: journal_size,
            action: CleanAction::Run {
                program: safe::JOURNALCTL,
                args: vec!["--vacuum-time=1week".into()],
                privileged: true,
            },
            checked: false,
        });
    }

    // Thumbnail cache
    let thumbs = PathBuf::from(&home).join(".cache/thumbnails");
    if let Some(size) = dir_size(&thumbs) {
        if size > 0 {
            items.push(MaintenanceItem {
                id: "thumbnails".into(),
                category: "System cruft",
                label: "Thumbnail cache".into(),
                size,
                action: CleanAction::RemoveContents {
                    dir: thumbs.clone(),
                    privileged: false,
                },
                checked: false,
            });
        }
    }

    // Trash
    let trash = PathBuf::from(&home).join(".local/share/Trash");
    if let Some(size) = dir_size(&trash) {
        if size > 0 {
            items.push(MaintenanceItem {
                id: "trash".into(),
                category: "System cruft",
                label: "Trash".into(),
                size,
                // The trash is in the user's home, so no root is needed.
                action: CleanAction::RemoveContents {
                    dir: trash.clone(),
                    privileged: false,
                },
                checked: false,
            });
        }
    }

    // Docker unused images
    if which::which("docker").is_ok() {
        let docker_size = get_docker_reclaimable_size();
        if docker_size > 0 {
            items.push(MaintenanceItem {
                id: "docker-images".into(),
                category: "System cruft",
                label: "Docker unused images".into(),
                size: docker_size,
                action: CleanAction::Run {
                    program: "docker",
                    args: vec!["image".into(), "prune".into(), "-a".into(), "-f".into()],
                    privileged: false,
                },
                checked: false,
            });
        }
    }

    items
}

/// Recursively compute directory size in bytes. Returns None if path doesn't exist.
fn dir_size(path: &Path) -> Option<u64> {
    if !path.exists() {
        return None;
    }
    let mut total = 0u64;
    if path.is_file() {
        return fs::metadata(path).ok().map(|m| m.len());
    }
    for entry in walkdir(path) {
        if let Ok(meta) = fs::metadata(&entry) {
            if meta.is_file() {
                total += meta.len();
            }
        }
    }
    Some(total)
}

/// Simple recursive directory walk (avoids pulling in the `walkdir` crate).
fn walkdir(path: &Path) -> Vec<PathBuf> {
    let mut result = Vec::new();
    if let Ok(entries) = fs::read_dir(path) {
        for entry in entries.flatten() {
            let p = entry.path();
            if p.is_dir() {
                result.extend(walkdir(&p));
            } else {
                result.push(p);
            }
        }
    }
    result
}

fn get_paccache_uninstalled_size() -> u64 {
    // Estimate: run paccache -duk0 and count sizes. For simplicity, use pacman cache
    // minus installed packages as a rough estimate. Actual computation is expensive.
    // Return 0 to avoid false reporting — the main pacman-cache item covers this.
    0
}

fn get_orphan_size() -> u64 {
    let output = Command::new("pacman").args(["-Qdtq"]).output().ok();
    let Some(output) = output else { return 0 };
    if !output.status.success() {
        return 0;
    }

    let orphans: Vec<&str> = std::str::from_utf8(&output.stdout)
        .unwrap_or("")
        .lines()
        .filter(|l| !l.is_empty())
        .collect();

    if orphans.is_empty() {
        return 0;
    }

    // Get sizes of orphan packages
    let mut total = 0u64;
    for name in &orphans {
        if let Ok(qi) = Command::new("pacman").args(["-Qi", name]).output() {
            let text = String::from_utf8_lossy(&qi.stdout);
            for line in text.lines() {
                if line.starts_with("Installed Size") {
                    if let Some(size) = parse_pacman_size(line) {
                        total += size;
                    }
                }
            }
        }
    }
    total
}

fn parse_pacman_size(line: &str) -> Option<u64> {
    // "Installed Size  : 12.50 MiB"
    let val_str = line.split(':').nth(1)?.trim();
    let parts: Vec<&str> = val_str.split_whitespace().collect();
    if parts.len() < 2 {
        return None;
    }
    let num: f64 = parts[0].parse().ok()?;
    let unit = parts[1];
    let bytes = match unit {
        "B" => num as u64,
        "KiB" => (num * 1024.0) as u64,
        "MiB" => (num * 1024.0 * 1024.0) as u64,
        "GiB" => (num * 1024.0 * 1024.0 * 1024.0) as u64,
        _ => return None,
    };
    Some(bytes)
}

fn get_flatpak_unused_size() -> u64 {
    // flatpak list --unused --columns=size
    if which::which("flatpak").is_err() {
        return 0;
    }
    let output = Command::new("flatpak")
        .args(["list", "--unused", "--columns=size"])
        .output()
        .ok();
    let Some(output) = output else { return 0 };
    if !output.status.success() {
        return 0;
    }
    let text = String::from_utf8_lossy(&output.stdout);
    let mut total = 0u64;
    for line in text.lines().skip(1) {
        let size_str = line.trim();
        if size_str.is_empty() {
            continue;
        }
        // Flatpak reports sizes like "234.5 MB"
        let parts: Vec<&str> = size_str.split_whitespace().collect();
        if parts.len() >= 2 {
            if let Ok(num) = parts[0].parse::<f64>() {
                let mult = match parts[1] {
                    "kB" => 1000.0,
                    "MB" => 1_000_000.0,
                    "GB" => 1_000_000_000.0,
                    _ => 1.0,
                };
                total += (num * mult) as u64;
            }
        }
    }
    total
}

fn get_journal_size() -> u64 {
    let output = Command::new("journalctl")
        .args(["--disk-usage"])
        .output()
        .ok();
    let Some(output) = output else { return 0 };
    let text = String::from_utf8_lossy(&output.stdout);
    // "Archived and active journals take up 371.2M in the file system."
    for word in text.split_whitespace() {
        if word.ends_with('M') || word.ends_with('G') || word.ends_with('K') {
            let unit = &word[word.len() - 1..];
            if let Ok(num) = word[..word.len() - 1].parse::<f64>() {
                let mult = match unit {
                    "K" => 1024.0,
                    "M" => 1024.0 * 1024.0,
                    "G" => 1024.0 * 1024.0 * 1024.0,
                    _ => 1.0,
                };
                return (num * mult) as u64;
            }
        }
    }
    0
}

fn get_docker_reclaimable_size() -> u64 {
    let output = Command::new("docker")
        .args(["system", "df", "--format", "{{.Reclaimable}}"])
        .output()
        .ok();
    let Some(output) = output else { return 0 };
    if !output.status.success() {
        return 0;
    }
    let text = String::from_utf8_lossy(&output.stdout);
    let mut total = 0u64;
    for line in text.lines() {
        // "381.2MB (100%)" or "0B"
        let size_part = line.split('(').next().unwrap_or("").trim();
        if let Some(bytes) = parse_docker_size(size_part) {
            total += bytes;
        }
    }
    total
}

fn parse_docker_size(s: &str) -> Option<u64> {
    let s = s.trim();
    if s.ends_with("GB") {
        let n: f64 = s.trim_end_matches("GB").parse().ok()?;
        Some((n * 1_000_000_000.0) as u64)
    } else if s.ends_with("MB") {
        let n: f64 = s.trim_end_matches("MB").parse().ok()?;
        Some((n * 1_000_000.0) as u64)
    } else if s.ends_with("kB") {
        let n: f64 = s.trim_end_matches("kB").parse().ok()?;
        Some((n * 1000.0) as u64)
    } else if s.ends_with('B') {
        let n: f64 = s.trim_end_matches('B').parse().ok()?;
        Some(n as u64)
    } else {
        None
    }
}

/// Total size and paths of `~/.config/*/Cache` directories.
fn scan_electron_caches(home: &str) -> (u64, Vec<PathBuf>) {
    let config_dir = PathBuf::from(home).join(".config");
    if !config_dir.exists() {
        return (0, Vec::new());
    }
    let mut total = 0u64;
    let mut dirs = Vec::new();
    if let Ok(entries) = fs::read_dir(&config_dir) {
        for entry in entries.flatten() {
            let cache_dir = entry.path().join("Cache");
            if cache_dir.is_dir() {
                total += dir_size(&cache_dir).unwrap_or(0);
                dirs.push(cache_dir);
            }
        }
    }
    (total, dirs)
}

fn find_rust_target_dirs(home: &str) -> Vec<(PathBuf, u64)> {
    let mut targets = Vec::new();
    let programs_dir = PathBuf::from(home).join("Programs");
    if programs_dir.exists() {
        scan_for_targets(&programs_dir, &mut targets, 0);
    }
    targets
}

fn scan_for_targets(dir: &Path, targets: &mut Vec<(PathBuf, u64)>, depth: usize) {
    if depth > 4 {
        return;
    }
    let entries = match fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let name = entry.file_name();
        let name_str = name.to_string_lossy();
        if name_str == "target" {
            // Verify it's a Rust target dir: has CACHEDIR.TAG or debug/release subdirs
            let has_cachedir = path.join("CACHEDIR.TAG").exists();
            let has_debug = path.join("debug").exists();
            let has_release = path.join("release").exists();
            if has_cachedir || has_debug || has_release {
                let size = dir_size(&path).unwrap_or(0);
                if size > 0 {
                    targets.push((path, size));
                }
            }
        } else if name_str != ".git" && name_str != "node_modules" {
            scan_for_targets(&path, targets, depth + 1);
        }
    }
}
