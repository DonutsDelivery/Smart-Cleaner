#!/bin/bash
# Runs inside the container from packaging/appimage/Dockerfile.
#   /src  the source tree (read-only)
#   /out  where the AppImage and its .zsync land
set -euo pipefail
export CARGO_HOME=/tmp/cargo-home CARGO_TARGET_DIR=/tmp/target HOME=/tmp
APP_ID=online.donutsdelivery.DonutCleaner
cp -r /src /tmp/build && cd /tmp/build
VERSION=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)

cargo build --release --locked
T=/tmp/target/release

rm -rf /tmp/AppDir
export DEPLOY_GTK_VERSION=4 NO_STRIP=1
args=(--appdir /tmp/AppDir --executable $T/donutcleaner
      --desktop-file data/$APP_ID.desktop --plugin gtk)
for icon in icons/hicolor/*/apps/$APP_ID.png icons/$APP_ID.svg; do
  args+=(--icon-file "$icon")
done
linuxdeploy "${args[@]}"

# The helper and the policy cannot be installed from inside an AppImage. They ride along
# so the first-run notice can tell the user how to extract and install them.
install -Dm755 $T/donutcleaner-helper /tmp/AppDir/usr/lib/donutcleaner/donutcleaner-helper
install -Dm644 data/$APP_ID.helper.policy /tmp/AppDir/usr/share/polkit-1/actions/$APP_ID.helper.policy
install -Dm644 data/$APP_ID.metainfo.xml /tmp/AppDir/usr/share/metainfo/$APP_ID.appdata.xml
# .DirIcon is what file managers show for the AppImage itself: a real 256px PNG.
install -Dm644 icons/hicolor/256x256/apps/$APP_ID.png /tmp/AppDir/$APP_ID.png
rm -f /tmp/AppDir/.DirIcon /tmp/AppDir/$APP_ID.svg
ln -s $APP_ID.png /tmp/AppDir/.DirIcon

# Ship glibc and its loader. GTK 4.14 and libadwaita 1.5 are built on Ubuntu 24.04, but
# appimage.github.io tests on the oldest supported LTS, whose glibc is older. With its own
# glibc the AppImage runs on any distribution. AppRun starts the program through the
# bundled loader, and --library-path (unlike LD_LIBRARY_PATH) is not passed to the
# programs the app starts, such as pkexec and pacman.
GLIBC_DIR=/tmp/AppDir/glibc
mkdir -p $GLIBC_DIR
for lib in ld-linux-x86-64.so.2 libc.so.6 libm.so.6 libdl.so.2 libpthread.so.0 librt.so.1 \
           libresolv.so.2 libutil.so.1 libnss_files.so.2 libnss_dns.so.2 libgcc_s.so.1; do
  cp -L /lib/x86_64-linux-gnu/$lib $GLIBC_DIR/
done
# linuxdeploy leaves out libraries such as harfbuzz, fribidi, freetype and fontconfig on
# the assumption that the host has them. An older host has older versions, and the bundled
# Pango needs a newer harfbuzz. Bundle every dependency except the ones tied to the
# host's graphics and display stack.
(
  export LD_LIBRARY_PATH=/tmp/AppDir/usr/lib
  find /tmp/AppDir -type f \( -name '*.so*' -o -path '*/usr/bin/donutcleaner' \) -print0 \
    | xargs -0 -n1 ldd 2>/dev/null | awk '/=> \//{print $3}' | sort -u > /tmp/deps.txt
)
while read -r lib; do
  case $lib in /tmp/AppDir/*) continue ;; esac
  case $(basename "$lib") in
    libGL*|libEGL*|libOpenGL*|libglapi*|libdrm*|libgbm*|libX*|libxcb*|libudev*|libdbus-1*) continue ;;
    ld-linux*|libc.so*|libm.so*|libdl.so*|libpthread.so*|librt.so*|libresolv.so*|libutil.so*) continue ;;
    libnss_*|libgcc_s.so*) continue ;;
  esac
  cp -nL "$lib" /tmp/AppDir/usr/lib/
done < /tmp/deps.txt
cat > /tmp/AppDir/AppRun <<'RUN'
#! /usr/bin/env bash
set -e
this_dir="$(readlink -f "$(dirname "$0")")"
source "$this_dir"/apprun-hooks/linuxdeploy-plugin-gtk.sh
exec "$this_dir"/glibc/ld-linux-x86-64.so.2 \
  --library-path "$this_dir/glibc:$this_dir/usr/lib:$this_dir/usr/lib/x86_64-linux-gnu" \
  "$this_dir"/usr/bin/donutcleaner "$@"
RUN
chmod +x /tmp/AppDir/AppRun
rm -f /tmp/AppDir/AppRun.wrapped

export LINUXDEPLOY_OUTPUT_VERSION=$VERSION
export LDAI_UPDATE_INFORMATION="gh-releases-zsync|DonutsDelivery|Smart-Cleaner|latest|DonutCleaner-*x86_64.AppImage.zsync"
cd /tmp && linuxdeploy --appdir /tmp/AppDir --output appimage

cp /tmp/DonutCleaner-*x86_64.AppImage /tmp/DonutCleaner-*x86_64.AppImage.zsync /out/
ls -la /out
sha256sum /out/*
