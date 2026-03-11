use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use crate::model::package::PackageInfo;
use crate::model::package_source::PackageSource;
use crate::scanner::{PackageScanner, ScanError};

/// Scanner that finds .desktop entries not claimed by any other package scanner.
/// These are standalone programs installed manually (e.g., Discord, REAPER, Steam games).
pub struct DesktopEntryScanner {
    /// Names/IDs of packages already found by other scanners, used for deduplication.
    known_packages: HashSet<String>,
    /// Exec binaries already claimed by other scanners.
    known_binaries: HashSet<String>,
}

impl DesktopEntryScanner {
    pub fn new(existing_packages: &[PackageInfo]) -> Self {
        let mut known_packages = HashSet::new();
        let mut known_binaries = HashSet::new();

        for pkg in existing_packages {
            known_packages.insert(pkg.name.to_lowercase());
            // Also index by desktop file ID patterns
            known_packages.insert(pkg.qualified_id());
            if let Some(ref path) = pkg.install_path {
                if let Some(bin) = path.file_name().and_then(|f| f.to_str()) {
                    known_binaries.insert(bin.to_lowercase());
                }
            }
        }

        Self {
            known_packages,
            known_binaries,
        }
    }

    fn is_claimed(&self, desktop_id: &str, exec_binary: &str, _name: &str) -> bool {
        let id_lower = desktop_id.to_lowercase();
        let exec_lower = exec_binary.to_lowercase();

        // Only filter true duplicates:
        // 1. Exact desktop_id match (e.g. flatpak "com.spotify.Client" == desktop file stem)
        // 2. Binary already provided by another scanner (appimage, flatpak)
        // Note: pacman-owned .desktop files are already filtered by pacman_owned_desktop_files()
        self.known_packages.contains(&id_lower)
            || self.known_binaries.contains(&exec_lower)
    }
}

impl PackageScanner for DesktopEntryScanner {
    fn source(&self) -> PackageSource {
        PackageSource::Desktop
    }

    fn label(&self) -> &str {
        "Desktop entries"
    }

    fn is_available(&self) -> bool {
        true
    }

    fn scan_blocking(&self) -> Result<Vec<PackageInfo>, ScanError> {
        let dirs = desktop_dirs();
        let pacman_owned = pacman_owned_desktop_files();
        let mut packages = Vec::new();
        let mut seen_ids = HashSet::new();

        for dir in &dirs {
            if !dir.exists() {
                continue;
            }
            let entries = fs::read_dir(dir).map_err(|e| ScanError {
                source: "Desktop",
                message: format!("Failed to read {}: {}", dir.display(), e),
            })?;

            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().is_some_and(|ext| ext == "desktop") {
                    // Skip .desktop files owned by pacman packages
                    if pacman_owned.contains(&path) {
                        continue;
                    }
                    if let Some(pkg) = self.parse_desktop_entry(&path, &mut seen_ids) {
                        packages.push(pkg);
                    }
                }
            }
        }

        Ok(packages)
    }
}

impl DesktopEntryScanner {
    fn parse_desktop_entry(
        &self,
        path: &Path,
        seen_ids: &mut HashSet<String>,
    ) -> Option<PackageInfo> {
        let content = fs::read_to_string(path).ok()?;
        let mut in_desktop_entry = false;
        let mut name = String::new();
        let mut comment = String::new();
        let mut icon = None;
        let mut exec = String::new();
        let mut categories = Vec::new();
        let mut no_display = false;
        let mut entry_type = String::new();

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
                    "Type" => entry_type = val.trim().to_string(),
                    _ => {}
                }
            }
        }

        // Skip non-application entries and hidden ones
        if no_display || name.is_empty() || (entry_type != "Application" && !entry_type.is_empty()) {
            return None;
        }

        let desktop_id = path.file_stem()?.to_str()?.to_string();

        // Extract the binary name from Exec line
        let exec_binary = exec
            .split_whitespace()
            .next()
            .unwrap_or("")
            .rsplit('/')
            .next()
            .unwrap_or("")
            .to_string();

        // Skip if already claimed by another scanner
        if self.is_claimed(&desktop_id, &exec_binary, &name) {
            return None;
        }

        // Deduplicate within this scanner
        if !seen_ids.insert(desktop_id.clone()) {
            return None;
        }

        // Determine install path from Exec line
        let install_path = if !exec.is_empty() {
            let exec_path = exec.split_whitespace().next().unwrap_or("");
            let p = Path::new(exec_path);
            if p.is_absolute() {
                if p.exists() {
                    Some(p.to_path_buf())
                } else {
                    // Binary doesn't exist — stale .desktop file
                    return None;
                }
            } else {
                // Try to resolve via which
                match which::which(exec_path) {
                    Ok(resolved) => Some(resolved),
                    Err(_) => return None, // Binary not found — skip
                }
            }
        } else {
            None
        };

        // Try to get file size of the binary
        let installed_size = install_path
            .as_ref()
            .and_then(|p| fs::metadata(p).ok())
            .map(|m| m.len())
            .unwrap_or(0);

        Some(PackageInfo {
            id: String::new(), // Will be set by qualified_id()
            name: desktop_id,
            display_name: name,
            version: String::new(),
            description: comment,
            categories,
            icon_name: icon,
            source: PackageSource::Desktop,
            installed_size,
            depends: Vec::new(),
            required_by: Vec::new(),
            provides: Vec::new(),
            is_explicit: true, // Standalone — always a leaf node
            is_protected: false,
            install_path,
            install_date: None,
        })
    }
}

/// Get all .desktop files owned by any pacman package.
/// This prevents the desktop scanner from claiming packages managed by pacman.
fn pacman_owned_desktop_files() -> HashSet<PathBuf> {
    let output = std::process::Command::new("bash")
        .args(["-c", "pacman -Ql 2>/dev/null | grep '/applications/.*\\.desktop$'"])
        .output()
        .ok();

    let Some(output) = output else {
        return HashSet::new();
    };

    let stdout = String::from_utf8_lossy(&output.stdout);
    stdout
        .lines()
        .filter_map(|line| {
            // Format: "package_name /path/to/file.desktop"
            let (_, path) = line.split_once(' ')?;
            Some(PathBuf::from(path))
        })
        .collect()
}

fn desktop_dirs() -> Vec<PathBuf> {
    let mut dirs = vec![PathBuf::from("/usr/share/applications")];

    if let Ok(home) = std::env::var("HOME") {
        dirs.push(PathBuf::from(&home).join(".local/share/applications"));
    }

    dirs
}
