use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use crate::model::package::PackageInfo;

/// Enriches packages with metadata from .desktop files
pub fn enrich_packages(packages: &mut Vec<PackageInfo>) {
    let entries = scan_desktop_entries();
    let exec_to_entry = build_exec_index(&entries);

    for pkg in packages.iter_mut() {
        // Try to find a matching .desktop entry by package name
        if let Some(entry) = entries.get(&pkg.name) {
            apply_entry(pkg, entry);
            continue;
        }

        // Try matching by common binary name patterns
        // Some packages have .desktop files named differently
        for (_exec, entry) in &exec_to_entry {
            if entry.exec_matches_package(&pkg.name) {
                apply_entry(pkg, entry);
                break;
            }
        }
    }
}

fn apply_entry(pkg: &mut PackageInfo, entry: &DesktopEntry) {
    if !entry.name.is_empty() {
        pkg.display_name = entry.name.clone();
    }
    if !entry.comment.is_empty() && pkg.description.is_empty() {
        pkg.description = entry.comment.clone();
    }
    if !entry.categories.is_empty() {
        pkg.categories = entry.categories.clone();
    }
    if entry.icon.is_some() {
        pkg.icon_name = entry.icon.clone();
    }
}

#[derive(Debug, Clone)]
struct DesktopEntry {
    name: String,
    comment: String,
    icon: Option<String>,
    exec: String,
    categories: Vec<String>,
}

impl DesktopEntry {
    fn exec_matches_package(&self, pkg_name: &str) -> bool {
        if self.exec.is_empty() {
            return false;
        }
        // Get the binary name from the Exec line
        let binary = self
            .exec
            .split_whitespace()
            .next()
            .unwrap_or("")
            .rsplit('/')
            .next()
            .unwrap_or("");

        binary == pkg_name
    }
}

fn scan_desktop_entries() -> HashMap<String, DesktopEntry> {
    let dirs = desktop_dirs();
    let mut entries = HashMap::new();

    for dir in dirs {
        if !dir.exists() {
            continue;
        }

        if let Ok(read_dir) = fs::read_dir(&dir) {
            for entry in read_dir.flatten() {
                let path = entry.path();
                if path.extension().is_some_and(|ext| ext == "desktop") {
                    if let Some((id, desktop_entry)) = parse_desktop_file(&path) {
                        entries.insert(id, desktop_entry);
                    }
                }
            }
        }
    }

    entries
}

fn desktop_dirs() -> Vec<PathBuf> {
    let mut dirs = vec![
        PathBuf::from("/usr/share/applications"),
        PathBuf::from("/var/lib/flatpak/exports/share/applications"),
    ];

    if let Some(home) = dirs::home_dir() {
        dirs.push(home.join(".local/share/applications"));
    }

    // Snap .desktop files
    if let Some(home) = dirs::home_dir() {
        let snap_dir = home.join("snap");
        if snap_dir.exists() {
            if let Ok(read_dir) = fs::read_dir(&snap_dir) {
                for entry in read_dir.flatten() {
                    let gui_dir = entry.path().join("current/meta/gui");
                    if gui_dir.exists() {
                        dirs.push(gui_dir);
                    }
                }
            }
        }
    }

    dirs
}

fn parse_desktop_file(path: &Path) -> Option<(String, DesktopEntry)> {
    let content = fs::read_to_string(path).ok()?;
    let mut in_desktop_entry = false;
    let mut name = String::new();
    let mut comment = String::new();
    let mut icon = None;
    let mut exec = String::new();
    let mut categories = Vec::new();
    let mut no_display = false;

    for line in content.lines() {
        let line = line.trim();

        if line == "[Desktop Entry]" {
            in_desktop_entry = true;
            continue;
        }

        if line.starts_with('[') {
            in_desktop_entry = false;
            continue;
        }

        if !in_desktop_entry {
            continue;
        }

        if let Some((key, val)) = line.split_once('=') {
            match key.trim() {
                "Name" => name = val.trim().to_string(),
                "Comment" => comment = val.trim().to_string(),
                "Icon" => icon = Some(val.trim().to_string()),
                "Exec" => exec = val.trim().to_string(),
                "Categories" => {
                    categories = val
                        .split(';')
                        .map(|s| s.trim().to_string())
                        .filter(|s| !s.is_empty())
                        .collect();
                }
                "NoDisplay" => no_display = val.trim() == "true",
                _ => {}
            }
        }
    }

    if no_display || name.is_empty() {
        return None;
    }

    // Use the filename (without .desktop) as the package name key
    let id = path
        .file_stem()?
        .to_str()?
        .to_string();

    Some((
        id,
        DesktopEntry {
            name,
            comment,
            icon,
            exec,
            categories,
        },
    ))
}

fn build_exec_index(entries: &HashMap<String, DesktopEntry>) -> HashMap<String, &DesktopEntry> {
    let mut index = HashMap::new();
    for entry in entries.values() {
        if !entry.exec.is_empty() {
            if let Some(binary) = entry.exec.split_whitespace().next() {
                let binary_name = binary.rsplit('/').next().unwrap_or(binary);
                index.insert(binary_name.to_string(), entry);
            }
        }
    }
    index
}

// Minimal dirs module since we don't want to pull in the `dirs` crate just for home_dir
mod dirs {
    use std::path::PathBuf;

    pub fn home_dir() -> Option<PathBuf> {
        std::env::var_os("HOME").map(PathBuf::from)
    }
}
