# DonutCleaner

Clean caches and remove unused packages on Linux. Built with GTK4, libadwaita and Rust. DonutCleaner was called SysClean before.

## Features

- **Package dependency graph.** Installed packages and their dependencies as an interactive graph, across Pacman, AUR, Flatpak, Snap, AppImage, pip, npm, Cargo and more.
- **Safe removal.** Dependency-aware checks and warnings for protected system packages before anything is removed.
- **Maintenance cleanup.** Scan for pacman and AUR caches, orphaned packages, dev tool caches, coredumps, the journal and other reclaimable space, with the size of each.

## Supported systems

System package cleanup (pacman cache, orphaned packages, package removal) works on Arch-based systems for now. On other distributions DonutCleaner shows a notice in the window, and Flatpak, AppImage, Cargo, npm and pip cleanup still work. apt, dnf and zypper are not supported yet.

## Administrator rights

DonutCleaner never runs `rm`, `pacman` or a shell as root. Root work goes to a small separate program, `donutcleaner-helper`, installed at `/usr/lib/donutcleaner/donutcleaner-helper` and started with `pkexec`. It has its own polkit action, `online.donutsdelivery.DonutCleaner.helper`, so the password prompt names DonutCleaner and the authorization is kept for a few minutes (`auth_admin_keep`).

The helper accepts seven operations and nothing else:

- `paccache-all` and `paccache-uninstalled` clear the pacman cache.
- `journal-vacuum` trims the systemd journal to one week.
- `clear-dir debtap-cache|coredumps` empties `/var/cache/debtap` or `/var/lib/systemd/coredump`.
- `remove-orphans` removes the packages `pacman -Qdtq` lists.
- `remove-packages <name>...` runs `pacman -Rns` on installed packages. Protected system packages are refused.
- `remove-paths <path>...` removes direct children of `/opt`, `/usr/local/bin` and `/usr/local/sbin` that no pacman package owns.

Package names are checked against pacman's character set, paths must be absolute and already normalized, and the helper starts its tools by absolute path with an empty environment. Symlinks are removed as links and never followed.

## Download

**Arch Linux (AUR):** `donutcleaner` builds from source, `donutcleaner-bin` installs the release binary. Both install the helper and the polkit policy.

**AppImage:** `DonutCleaner-x86_64.AppImage` from the release page bundles GTK4 and libadwaita and needs glibc 2.39 or newer. It cannot install the administrator helper, so on first run it shows the commands that extract the helper and the polkit policy from the AppImage and install them as root.

```bash
chmod +x DonutCleaner-*-x86_64.AppImage
./DonutCleaner-*-x86_64.AppImage
```

**Tarball:**
```bash
curl -LO https://github.com/DonutsDelivery/Smart-Cleaner/releases/latest/download/donutcleaner-x86_64.tar.gz
tar xzf donutcleaner-x86_64.tar.gz
cd donutcleaner
./donutcleaner
```

Root actions need the helper and the policy in system paths. Install them once:

```bash
sudo install -Dm755 donutcleaner-helper /usr/lib/donutcleaner/donutcleaner-helper
sudo install -Dm644 online.donutsdelivery.DonutCleaner.helper.policy /usr/share/polkit-1/actions/online.donutsdelivery.DonutCleaner.helper.policy
```

**Standalone binary:** `donutcleaner-x86_64` has no helper. Take `donutcleaner-helper` and the policy from the tarball and install them as above, or user-level cleanup (caches in your home directory, Flatpak, Cargo, npm, pip) still works without them.

## Dependencies

- GTK4
- libadwaita
- polkit (for root actions)

## Building the AppImage

`packaging/appimage/Dockerfile` builds an Ubuntu 24.04 image with pinned linuxdeploy tools. Inside it, `packaging/appimage/build.sh` expects the source at `/src` and writes the AppImage and its `.zsync` file to `/out`.

## Building from source

```bash
# Install build dependencies (Arch Linux)
sudo pacman -S gtk4 libadwaita rust

# Build and install (asks for sudo to place the helper and policy under /usr)
make install
```

If you used SysClean, `make install` removes its old binary, desktop file and icon from the install prefix.

## License

MIT, copyright Zero Action (DonutsDelivery). See [LICENSE](LICENSE).
