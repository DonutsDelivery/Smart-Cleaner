use std::collections::HashSet;
use std::process::Command;

use crate::model::package::PackageInfo;
use crate::model::package_source::PackageSource;

use super::{PackageScanner, ScanError};

pub struct PacmanScanner;

impl PackageScanner for PacmanScanner {
    fn source(&self) -> PackageSource {
        PackageSource::Pacman
    }

    fn label(&self) -> &str {
        "Pacman"
    }

    fn is_available(&self) -> bool {
        which::which("pacman").is_ok()
    }

    fn scan_blocking(&self) -> Result<Vec<PackageInfo>, ScanError> {
        let explicit = get_explicit_packages()?;
        let foreign = get_foreign_packages()?;
        let packages = parse_pacman_qi(&explicit, &foreign)?;
        Ok(packages)
    }
}

fn get_explicit_packages() -> Result<HashSet<String>, ScanError> {
    let output = Command::new("pacman")
        .args(["-Qqe"])
        .output()
        .map_err(|e| ScanError {
            source: "pacman",
            message: format!("Failed to run pacman -Qqe: {e}"),
        })?;

    Ok(String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(|s| s.to_string())
        .collect())
}

fn get_foreign_packages() -> Result<HashSet<String>, ScanError> {
    let output = Command::new("pacman")
        .args(["-Qqm"])
        .output()
        .map_err(|e| ScanError {
            source: "pacman",
            message: format!("Failed to run pacman -Qqm: {e}"),
        })?;

    Ok(String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(|s| s.to_string())
        .collect())
}

fn parse_pacman_qi(
    explicit: &HashSet<String>,
    foreign: &HashSet<String>,
) -> Result<Vec<PackageInfo>, ScanError> {
    let output = Command::new("pacman")
        .args(["-Qi"])
        .output()
        .map_err(|e| ScanError {
            source: "pacman",
            message: format!("Failed to run pacman -Qi: {e}"),
        })?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut packages = Vec::new();

    // pacman -Qi outputs blocks separated by empty lines
    for block in stdout.split("\n\n") {
        if block.trim().is_empty() {
            continue;
        }

        if let Some(pkg) = parse_package_block(block, explicit, foreign) {
            packages.push(pkg);
        }
    }

    Ok(packages)
}

fn parse_package_block(
    block: &str,
    explicit: &HashSet<String>,
    foreign: &HashSet<String>,
) -> Option<PackageInfo> {
    let mut name = String::new();
    let mut version = String::new();
    let mut description = String::new();
    let mut installed_size: u64 = 0;
    let mut depends = Vec::new();
    let mut required_by = Vec::new();

    // Track the current field for multi-line values
    let mut current_field = String::new();
    let mut current_value = String::new();

    for line in block.lines() {
        if let Some((key, val)) = line.split_once(':') {
            let key = key.trim();
            let val = val.trim();

            // Process previous field if it was multi-line
            process_field(
                &current_field,
                &current_value,
                &mut name,
                &mut version,
                &mut description,
                &mut installed_size,
                &mut depends,
                &mut required_by,
            );

            current_field = key.to_string();
            current_value = val.to_string();
        } else {
            // Continuation line
            if !current_value.is_empty() {
                current_value.push(' ');
            }
            current_value.push_str(line.trim());
        }
    }

    // Process the last field
    process_field(
        &current_field,
        &current_value,
        &mut name,
        &mut version,
        &mut description,
        &mut installed_size,
        &mut depends,
        &mut required_by,
    );

    if name.is_empty() {
        return None;
    }

    let is_explicit = explicit.contains(&name);
    let source = if foreign.contains(&name) {
        PackageSource::Aur
    } else {
        PackageSource::Pacman
    };

    Some(PackageInfo {
        id: format!("{}:{}", source.prefix(), name),
        name,
        display_name: String::new(), // Filled by desktop entry enrichment
        version,
        description,
        categories: Vec::new(),
        icon_name: None,
        source,
        installed_size,
        depends,
        required_by,
        is_explicit,
        install_path: None,
    })
}

fn process_field(
    field: &str,
    value: &str,
    name: &mut String,
    version: &mut String,
    description: &mut String,
    installed_size: &mut u64,
    depends: &mut Vec<String>,
    required_by: &mut Vec<String>,
) {
    match field {
        "Name" => *name = value.to_string(),
        "Version" => *version = value.to_string(),
        "Description" => *description = value.to_string(),
        "Installed Size" => *installed_size = parse_size(value),
        "Depends On" => {
            if value != "None" {
                *depends = value
                    .split_whitespace()
                    .map(|s| {
                        // Strip version constraints: "glibc>=2.38" -> "glibc"
                        s.split(|c: char| c == '>' || c == '<' || c == '=')
                            .next()
                            .unwrap_or(s)
                            .to_string()
                    })
                    .collect();
            }
        }
        "Required By" => {
            if value != "None" {
                *required_by = value.split_whitespace().map(|s| s.to_string()).collect();
            }
        }
        _ => {}
    }
}

fn parse_size(s: &str) -> u64 {
    // Parse strings like "245.00 MiB", "1.20 GiB", "52.00 KiB"
    let parts: Vec<&str> = s.split_whitespace().collect();
    if parts.len() != 2 {
        return 0;
    }

    let num: f64 = parts[0].parse().unwrap_or(0.0);
    let multiplier: f64 = match parts[1] {
        "B" => 1.0,
        "KiB" => 1024.0,
        "MiB" => 1024.0 * 1024.0,
        "GiB" => 1024.0 * 1024.0 * 1024.0,
        "TiB" => 1024.0 * 1024.0 * 1024.0 * 1024.0,
        _ => 0.0,
    };

    (num * multiplier) as u64
}
