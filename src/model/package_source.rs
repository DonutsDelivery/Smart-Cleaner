use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PackageSource {
    Pacman,
    Aur,
    Flatpak,
    Snap,
    AppImage,
    Wine,
    Pip,
    Npm,
    Cargo,
    Desktop,
}

impl PackageSource {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Pacman => "Pacman",
            Self::Aur => "AUR",
            Self::Flatpak => "Flatpak",
            Self::Snap => "Snap",
            Self::AppImage => "AppImage",
            Self::Wine => "Wine",
            Self::Pip => "pip",
            Self::Npm => "npm",
            Self::Cargo => "Cargo",
            Self::Desktop => "Desktop",
        }
    }

    pub fn icon_name(&self) -> &'static str {
        match self {
            Self::Pacman | Self::Aur => "system-software-install-symbolic",
            Self::Flatpak => "system-software-install-symbolic",
            Self::Snap => "system-software-install-symbolic",
            Self::AppImage => "application-x-executable-symbolic",
            Self::Wine => "preferences-desktop-apps-symbolic",
            Self::Pip | Self::Npm | Self::Cargo => "utilities-terminal-symbolic",
            Self::Desktop => "application-x-desktop-symbolic",
        }
    }

    pub fn prefix(&self) -> &'static str {
        match self {
            Self::Pacman => "pacman",
            Self::Aur => "aur",
            Self::Flatpak => "flatpak",
            Self::Snap => "snap",
            Self::AppImage => "appimage",
            Self::Wine => "wine",
            Self::Pip => "pip",
            Self::Npm => "npm",
            Self::Cargo => "cargo",
            Self::Desktop => "desktop",
        }
    }
}

impl fmt::Display for PackageSource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.label())
    }
}
