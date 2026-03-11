use std::process::Command;

use crate::model::package::PackageInfo;
use crate::model::package_source::PackageSource;

use super::{PackageScanner, ScanError};

pub struct FlatpakScanner;

impl PackageScanner for FlatpakScanner {
    fn source(&self) -> PackageSource {
        PackageSource::Flatpak
    }

    fn label(&self) -> &str {
        "Flatpak"
    }

    fn is_available(&self) -> bool {
        which::which("flatpak").is_ok()
    }

    fn scan_blocking(&self) -> Result<Vec<PackageInfo>, ScanError> {
        let output = Command::new("flatpak")
            .args(["list", "--app", "--columns=application,name,version,size"])
            .output()
            .map_err(|e| ScanError {
                source: "flatpak",
                message: format!("Failed to run flatpak list: {e}"),
            })?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        let mut packages = Vec::new();

        for line in stdout.lines() {
            let parts: Vec<&str> = line.split('\t').collect();
            if parts.len() < 2 {
                continue;
            }

            let app_id = parts[0].trim();
            let name = parts[1].trim();
            let version = parts.get(2).map(|s| s.trim()).unwrap_or("");
            let size_str = parts.get(3).map(|s| s.trim()).unwrap_or("0");

            packages.push(PackageInfo {
                id: format!("flatpak:{app_id}"),
                name: app_id.to_string(),
                display_name: name.to_string(),
                version: version.to_string(),
                description: String::new(),
                categories: Vec::new(),
                icon_name: None,
                source: PackageSource::Flatpak,
                installed_size: parse_flatpak_size(size_str),
                depends: Vec::new(),
                required_by: Vec::new(),
                provides: Vec::new(),
                is_explicit: true,
                is_protected: false,
                install_path: None,
                install_date: None,
            });
        }

        Ok(packages)
    }
}

fn parse_flatpak_size(s: &str) -> u64 {
    // Flatpak outputs sizes like "842.3 MB", "1.2 GB"
    let parts: Vec<&str> = s.split_whitespace().collect();
    if parts.len() != 2 {
        return 0;
    }

    let num: f64 = parts[0].parse().unwrap_or(0.0);
    let multiplier: f64 = match parts[1] {
        "kB" | "KB" => 1000.0,
        "MB" => 1_000_000.0,
        "GB" => 1_000_000_000.0,
        _ => 0.0,
    };

    (num * multiplier) as u64
}
