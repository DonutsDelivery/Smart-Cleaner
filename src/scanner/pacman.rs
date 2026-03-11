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
    let mut provides = Vec::new();
    let mut install_date: Option<i64> = None;

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
                &mut provides,
                &mut install_date,
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
        &mut provides,
        &mut install_date,
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
        provides,
        is_explicit,
        is_protected: false,
        install_path: None,
        install_date,
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
    provides: &mut Vec<String>,
    install_date: &mut Option<i64>,
) {
    match field {
        "Name" => *name = value.to_string(),
        "Version" => *version = value.to_string(),
        "Description" => *description = value.to_string(),
        "Installed Size" => *installed_size = parse_size(value),
        "Install Date" => *install_date = parse_install_date(value),
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
        "Provides" => {
            if value != "None" {
                *provides = value
                    .split_whitespace()
                    .map(|s| {
                        // Strip version constraints: "python3=3.12.3" -> "python3"
                        s.split(|c: char| c == '>' || c == '<' || c == '=')
                            .next()
                            .unwrap_or(s)
                            .to_string()
                    })
                    .collect();
            }
        }
        _ => {}
    }
}

/// Parse pacman install date: "Thu 27 Feb 2025 03:45:12 PM UTC" → Unix timestamp
fn parse_install_date(s: &str) -> Option<i64> {
    let parts: Vec<&str> = s.split_whitespace().collect();
    if parts.len() < 6 {
        return None;
    }

    let day: i64 = parts[1].parse().ok()?;
    let month: i64 = match parts[2] {
        "Jan" => 1,
        "Feb" => 2,
        "Mar" => 3,
        "Apr" => 4,
        "May" => 5,
        "Jun" => 6,
        "Jul" => 7,
        "Aug" => 8,
        "Sep" => 9,
        "Oct" => 10,
        "Nov" => 11,
        "Dec" => 12,
        _ => return None,
    };
    let year: i64 = parts[3].parse().ok()?;

    let time_parts: Vec<&str> = parts[4].split(':').collect();
    if time_parts.len() != 3 {
        return None;
    }
    let mut hour: i64 = time_parts[0].parse().ok()?;
    let min: i64 = time_parts[1].parse().ok()?;
    let sec: i64 = time_parts[2].parse().ok()?;

    if parts.len() > 5 {
        match parts[5] {
            "PM" if hour != 12 => hour += 12,
            "AM" if hour == 12 => hour = 0,
            _ => {}
        }
    }

    let is_leap = |y: i64| (y % 4 == 0 && y % 100 != 0) || y % 400 == 0;
    let mut total_days: i64 = 0;
    for y in 1970..year {
        total_days += if is_leap(y) { 366 } else { 365 };
    }
    let days_in_month = [
        0, 31,
        28 + if is_leap(year) { 1 } else { 0 },
        31, 30, 31, 30, 31, 31, 30, 31, 30, 31,
    ];
    for m in 1..month {
        total_days += days_in_month[m as usize];
    }
    total_days += day - 1;

    Some(total_days * 86400 + hour * 3600 + min * 60 + sec)
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

/// Get package names that should be protected from removal.
/// Includes: `base` and its dependencies, the kernel, and the package manager.
pub fn get_protected_package_names() -> HashSet<String> {
    let mut protected = HashSet::new();

    // Always protect base itself
    protected.insert("base".to_string());

    // Get direct deps of base
    if let Ok(output) = Command::new("pacman").args(["-Qi", "base"]).output() {
        let text = String::from_utf8_lossy(&output.stdout);
        for line in text.lines() {
            if line.starts_with("Depends On") {
                if let Some(deps) = line.split(':').nth(1) {
                    for dep in deps.split_whitespace() {
                        // Strip version constraints like >=2.0
                        let name = dep.split(|c| c == '>' || c == '<' || c == '=')
                            .next()
                            .unwrap_or(dep);
                        if !name.is_empty() && name != "None" {
                            protected.insert(name.to_string());
                        }
                    }
                }
            }
        }
    }

    // Protect the kernel
    protected.insert("linux".to_string());
    protected.insert("linux-headers".to_string());
    protected.insert("linux-firmware".to_string());

    // Protect the package manager itself
    protected.insert("pacman".to_string());
    protected.insert("yay".to_string());
    protected.insert("paru".to_string());

    protected
}
