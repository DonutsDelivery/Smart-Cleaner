use std::fs;
use std::path::PathBuf;

use crate::model::package::PackageInfo;
use crate::model::package_source::PackageSource;

use super::{PackageScanner, ScanError};

pub struct AppImageScanner;

impl PackageScanner for AppImageScanner {
    fn source(&self) -> PackageSource {
        PackageSource::AppImage
    }

    fn label(&self) -> &str {
        "AppImage"
    }

    fn is_available(&self) -> bool {
        // Always available — we just scan filesystem
        true
    }

    fn scan_blocking(&self) -> Result<Vec<PackageInfo>, ScanError> {
        let mut packages = Vec::new();

        for dir in search_dirs() {
            if !dir.exists() {
                continue;
            }

            let entries = fs::read_dir(&dir).map_err(|e| ScanError {
                source: "appimage",
                message: format!("Failed to read {}: {e}", dir.display()),
            })?;

            for entry in entries.flatten() {
                let path = entry.path();
                if is_appimage(&path) {
                    if let Some(pkg) = appimage_to_package(&path) {
                        packages.push(pkg);
                    }
                }
            }
        }

        Ok(packages)
    }
}

fn search_dirs() -> Vec<PathBuf> {
    let mut dirs = vec![PathBuf::from("/opt")];

    if let Ok(home) = std::env::var("HOME") {
        let home = PathBuf::from(home);
        dirs.push(home.join("Applications"));
        dirs.push(home.join("bin"));
        dirs.push(home.join(".local/bin"));
    }

    dirs
}

fn is_appimage(path: &PathBuf) -> bool {
    if !path.is_file() {
        return false;
    }

    // Check extension
    if let Some(ext) = path.extension() {
        let ext_lower = ext.to_string_lossy().to_lowercase();
        if ext_lower == "appimage" {
            return true;
        }
    }

    // Check for AppImage magic bytes (ELF + AI marker)
    if let Ok(data) = fs::read(path) {
        if data.len() > 11 {
            // AppImage Type 2: ELF header + "AI\x02" at offset 8
            if &data[0..4] == b"\x7fELF" && data.len() > 10 && &data[8..10] == b"AI" {
                return true;
            }
        }
    }

    false
}

fn appimage_to_package(path: &PathBuf) -> Option<PackageInfo> {
    let filename = path.file_name()?.to_string_lossy();

    // Extract name from filename: "Obsidian-1.5.3.AppImage" -> "Obsidian"
    let name = filename
        .trim_end_matches(".AppImage")
        .trim_end_matches(".appimage")
        .split('-')
        .next()
        .unwrap_or(&filename)
        .to_string();

    let size = fs::metadata(path).map(|m| m.len()).unwrap_or(0);

    Some(PackageInfo {
        id: format!("appimage:{name}"),
        name: name.clone(),
        display_name: name,
        version: String::new(),
        description: format!("AppImage at {}", path.display()),
        categories: Vec::new(),
        icon_name: None,
        source: PackageSource::AppImage,
        installed_size: size,
        depends: Vec::new(),
        required_by: Vec::new(),
        is_explicit: true,
        install_path: Some(path.clone()),
    })
}
