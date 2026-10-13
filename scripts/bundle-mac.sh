#!/usr/bin/env bash
# Builds "Screen Snap.app" for macOS: the release binary, packaging/macos/Info.plist
# with the version from Cargo.toml (LSUIElement: menu bar only, no Dock icon; bundle id
# com.larsbrubaker.screensnap), and an .icns drawn by the app itself
# (`screensnap --write-icon`, the mark from screensnap-core/src/icon.rs). The bundle is
# ad-hoc signed only (no Developer ID), so Gatekeeper asks before the first launch, and
# macOS forgets the Screen Recording permission whenever the bundle is rebuilt.
#
#   scripts/bundle-mac.sh [OUT_DIR]          (default: dist/)
#   UNIVERSAL=1 scripts/bundle-mac.sh        Apple Silicon + Intel via lipo; needs
#       rustup target add aarch64-apple-darwin x86_64-apple-darwin
set -euo pipefail

cd "$(dirname "$0")/.."
out="${1:-dist}"
version="$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -n 1)"
if [ -z "$version" ]; then
    echo "no version in Cargo.toml" >&2
    exit 1
fi

if [ "${UNIVERSAL:-0}" = "1" ]; then
    for target in aarch64-apple-darwin x86_64-apple-darwin; do
        cargo build --release -p screensnap-native --target "$target"
    done
else
    cargo build --release -p screensnap-native
fi

mkdir -p "$out"
app="$out/Screen Snap.app"
rm -rf "$app"
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"
if [ "${UNIVERSAL:-0}" = "1" ]; then
    lipo -create \
        target/aarch64-apple-darwin/release/screensnap \
        target/x86_64-apple-darwin/release/screensnap \
        -output "$app/Contents/MacOS/screensnap"
else
    cp target/release/screensnap "$app/Contents/MacOS/screensnap"
fi
sed "s/@VERSION@/$version/g" packaging/macos/Info.plist > "$app/Contents/Info.plist"
plutil -lint "$app/Contents/Info.plist"

# The icon: every size iconutil expects, rendered by the app.
iconset="$(mktemp -d)/screensnap.iconset"
mkdir -p "$iconset"
for size in 16 32 128 256 512; do
    "$app/Contents/MacOS/screensnap" --write-icon "$iconset/icon_${size}x${size}.png" "$size"
    "$app/Contents/MacOS/screensnap" --write-icon "$iconset/icon_${size}x${size}@2x.png" "$((size * 2))"
done
iconutil -c icns "$iconset" -o "$app/Contents/Resources/screensnap.icns"
rm -rf "$(dirname "$iconset")"
[ -f LICENSE ] && cp LICENSE "$app/Contents/Resources/"

codesign --force --deep --sign - "$app"
codesign --verify --verbose=1 "$app"
echo "built $app ($version)"
