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
rm -f /tmp/AppDir/.DirIcon
ln -s $APP_ID.png /tmp/AppDir/.DirIcon

export LINUXDEPLOY_OUTPUT_VERSION=$VERSION
export LDAI_UPDATE_INFORMATION="gh-releases-zsync|DonutsDelivery|Smart-Cleaner|latest|DonutCleaner-*x86_64.AppImage.zsync"
cd /tmp && linuxdeploy --appdir /tmp/AppDir --output appimage

cp /tmp/DonutCleaner-*x86_64.AppImage /tmp/DonutCleaner-*x86_64.AppImage.zsync /out/
ls -la /out
sha256sum /out/*
