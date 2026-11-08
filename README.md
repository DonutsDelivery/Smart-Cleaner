# DonutCleaner

Clean caches and remove unused packages on Linux. Built with GTK4, libadwaita and Rust. DonutCleaner was called SysClean before.

## Features

- **Package dependency graph.** Installed packages and their dependencies as an interactive graph, across Pacman, AUR, Flatpak, Snap, AppImage, pip, npm, Cargo and more.
- **Safe removal.** Dependency-aware checks and warnings for protected system packages before anything is removed.
- **Maintenance cleanup.** Scan for pacman and AUR caches, orphaned packages, dev tool caches, coredumps, the journal and other reclaimable space, with the size of each.

## Supported systems

System package cleanup (pacman cache, orphaned packages, package removal) works on Arch-based systems for now. On other distributions DonutCleaner shows a notice in the window, and Flatpak, AppImage, Cargo, npm and pip cleanup still work. apt, dnf and zypper are not supported yet.

## Administrator rights

Anything that needs root runs through polkit as a plain program call (`pkexec /usr/bin/rm`, `/usr/bin/pacman`, `/usr/bin/paccache`, `/usr/bin/journalctl`), never through a shell. `make install` installs `online.donutsdelivery.DonutCleaner.policy` into `/usr/share/polkit-1/actions` so the password prompt says what DonutCleaner is asking for. Paths are resolved and checked before they are passed to `rm`.

## Download

**Tarball:**
```bash
curl -LO https://github.com/DonutsDelivery/Smart-Cleaner/releases/latest/download/donutcleaner-x86_64.tar.gz
tar xzf donutcleaner-x86_64.tar.gz
cd donutcleaner
./donutcleaner
```

**Standalone binary:**
```bash
curl -LO https://github.com/DonutsDelivery/Smart-Cleaner/releases/latest/download/donutcleaner-x86_64
chmod +x donutcleaner-x86_64
./donutcleaner-x86_64
```

The standalone binary has no polkit policy, so the password prompt will name the program (`rm`, `pacman`) instead of DonutCleaner. Copy `online.donutsdelivery.DonutCleaner.policy` from the tarball to `/usr/share/polkit-1/actions` to fix that.

## Dependencies

- GTK4
- libadwaita
- polkit (for root actions)

## Building from source

```bash
# Install build dependencies (Arch Linux)
sudo pacman -S gtk4 libadwaita rust

# Build and install (asks for sudo once, to install the polkit policy)
make install
```

If you used SysClean, `make install` removes its old binary, desktop file and icon from the install prefix.

## License

MIT, copyright Zero Action (DonutsDelivery). See [LICENSE](LICENSE).
