# Build image for the DonutCleaner AppImage. Ubuntu 24.04 keeps the glibc requirement low.
FROM ubuntu:24.04
ENV DEBIAN_FRONTEND=noninteractive \
    RUSTUP_HOME=/usr/local/rustup \
    CARGO_HOME=/usr/local/cargo \
    PATH=/usr/local/cargo/bin:$PATH
RUN apt-get update && apt-get install -y --no-install-recommends \
      build-essential pkg-config curl ca-certificates git file patchelf zsync squashfs-tools \
      desktop-file-utils appstream \
      libgtk-4-dev libadwaita-1-dev librsvg2-common adwaita-icon-theme shared-mime-info \
    && curl -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal --default-toolchain stable \
    && chmod -R a+rX /usr/local/rustup /usr/local/cargo \
    && rm -rf /var/lib/apt/lists/*
# Pinned AppImage tooling. The build stops if a checksum differs.
RUN set -e; cd /usr/local/bin; \
    fetch() { curl -fsSL -o "$1" "$2"; echo "$3  $1" | sha256sum -c -; chmod +x "$1"; }; \
    fetch linuxdeploy https://github.com/linuxdeploy/linuxdeploy/releases/download/1-alpha-20251107-1/linuxdeploy-x86_64.AppImage c20cd71e3a4e3b80c3483cef793cda3f4e990aca14014d23c544ca3ce1270b4d; \
    fetch linuxdeploy-plugin-appimage https://github.com/linuxdeploy/linuxdeploy-plugin-appimage/releases/download/continuous/linuxdeploy-plugin-appimage-x86_64.AppImage 49d6a17160675a6bd1781699aae6bdf7692d98552e02a3671d2183d10547842e; \
    fetch linuxdeploy-plugin-gtk.sh https://raw.githubusercontent.com/linuxdeploy/linuxdeploy-plugin-gtk/7a3fbc31a9e5075073ff8790f26effbac5f84453/linuxdeploy-plugin-gtk.sh b0f4cbc684a0103a9651f0955b635eaea0096b3a66c0f5a2c2aa337960375171
ENV APPIMAGE_EXTRACT_AND_RUN=1
