pub const APP_ID: &str = "online.donutsdelivery.DonutCleaner";
pub const APP_NAME: &str = "DonutCleaner";
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Shown when pacman is missing. Package-cache cleanup only knows pacman for now.
pub const NO_PACMAN_NOTICE: &str = "System package cleanup currently supports Arch-based systems; Flatpak, AppImage, Cargo, npm and pip cleanup still work.";
