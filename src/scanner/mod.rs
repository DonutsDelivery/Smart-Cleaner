pub mod appimage;
pub mod desktop_entries;
pub mod desktop_scanner;
pub mod flatpak;
pub mod maintenance;
pub mod pacman;
pub mod registry;

use crate::model::package::PackageInfo;
use crate::model::package_source::PackageSource;

pub trait PackageScanner: Send + Sync {
    fn source(&self) -> PackageSource;
    fn label(&self) -> &str;
    fn is_available(&self) -> bool;
    fn scan_blocking(&self) -> Result<Vec<PackageInfo>, ScanError>;
}

#[derive(Debug)]
pub struct ScanError {
    pub source: &'static str,
    pub message: String,
}

impl std::fmt::Display for ScanError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "[{}] {}", self.source, self.message)
    }
}
