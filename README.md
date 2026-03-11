# SysClean

A system maintenance and package dependency manager for Linux, built with GTK4 and Rust.

## Features

- **Package Dependency Graph** — Visual interactive graph of all installed packages and their dependencies across Pacman, AUR, Flatpak, Snap, AppImage, pip, npm, Cargo, and more
- **Safe Removal** — Select packages for removal with dependency-aware safety checks and protected system package warnings
- **Maintenance Cleanup** — Scan and clean pacman cache, AUR build cache, orphaned packages, and other reclaimable space
- **Disk Space Analysis** — See exactly how much space each cleanup action will free

## Download

**Tarball:**
```bash
curl -LO https://github.com/DonutsDelivery/Smart-Cleaner/releases/latest/download/sysclean-x86_64.tar.gz
tar xzf sysclean-x86_64.tar.gz
cd sysclean
./sysclean
```

**Standalone binary:**
```bash
curl -LO https://github.com/DonutsDelivery/Smart-Cleaner/releases/latest/download/sysclean-x86_64
chmod +x sysclean-x86_64
./sysclean-x86_64
```

## Dependencies

- GTK4
- libadwaita

## Building from source

```bash
# Install build dependencies (Arch Linux)
sudo pacman -S gtk4 libadwaita rust

# Build and install
make install
```

## License

MIT
