#!/bin/bash
# Builds Tingzi.app for both Apple silicon and Intel Macs, and a disk image (.dmg) to install it from.
# Run on a Mac: bash macos/build.sh [output folder]
# Needs Rust (cargo and rustup), Python 3 and the Xcode command line tools.
set -euo pipefail
cd "$(dirname "$0")/.."
ROOT="$PWD"
OUT="${1:-$ROOT/.build/macos}"
mkdir -p "$OUT"
OUT="$(cd "$OUT" && pwd)"
APP="$OUT/Tingzi.app"
VERSION="$(sed -n 's/^version = "\(.*\)"/\1/p' app/src-tauri/Cargo.toml | head -1)"
TARGETS=(aarch64-apple-darwin x86_64-apple-darwin)
rustup target add "${TARGETS[@]}"

# Source paths are remapped so the program does not carry this computer's folder names.
export RUSTFLAGS="--remap-path-prefix=$HOME=/_ --remap-path-prefix=$ROOT=/_/src"
for target in "${TARGETS[@]}"; do
  cargo build --release --locked --manifest-path app/src-tauri/Cargo.toml --target "$target" --target-dir "$OUT/app-target"
done

rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
# One program that runs natively on both kinds of Mac.
lipo -create -output "$APP/Contents/MacOS/Tingzi" "$OUT/app-target/aarch64-apple-darwin/release/Tingzi" "$OUT/app-target/x86_64-apple-darwin/release/Tingzi"
lipo "$APP/Contents/MacOS/Tingzi" -verify_arch arm64 x86_64

# The readings of each character, which the app uses to match the user's own words by sound.
unzip -o -q windows/standalone-data/public-data.zip phrases.txt McBopomofo-LICENSE.txt -d "$APP/Contents/Resources"
cp LICENSE "$APP/Contents/Resources/LICENSE"
sips -s format icns app/src-tauri/icons/icon.png --out "$APP/Contents/Resources/Tingzi.icns" >/dev/null

# The licences of every Rust package built into the program, for either kind of Mac.
METADATA=()
for target in "${TARGETS[@]}"; do
  cargo metadata --format-version 1 --locked --filter-platform "$target" --manifest-path app/src-tauri/Cargo.toml > "$OUT/cargo-metadata-$target.json"
  METADATA+=("$OUT/cargo-metadata-$target.json")
done
python3 macos/notices.py "${METADATA[@]}" "$APP/Contents/Resources/THIRD-PARTY-NOTICES.txt"

# The app's own keys, on top of the ones in app/src-tauri/Info.plist.
PLIST="$APP/Contents/Info.plist"
cp app/src-tauri/Info.plist "$PLIST"
add() { plutil -insert "$1" -string "$2" "$PLIST"; }
add CFBundleExecutable Tingzi
add CFBundleIdentifier app.tingzi.voice
add CFBundleName Tingzi
add CFBundleDisplayName 聽字
add CFBundleIconFile Tingzi
add CFBundlePackageType APPL
add CFBundleShortVersionString "$VERSION"
add CFBundleVersion "$VERSION"
add CFBundleDevelopmentRegion zh_TW
add LSMinimumSystemVersion 11.0
plutil -insert NSHighResolutionCapable -bool true "$PLIST"
plutil -lint "$PLIST" >/dev/null

# Signed ad hoc only: there is no Developer ID, so on other Macs Gatekeeper blocks the first open until the
# user chooses Open Anyway in System Settings > Privacy & Security.
codesign --force --sign - "$APP"

# The disk image opens to Tingzi.app beside a shortcut to Applications, so installing is one drag.
DMG="Tingzi-$VERSION-macos.dmg"
STAGE="$OUT/dmg"
rm -rf "$STAGE" "$OUT/$DMG" "$OUT/$DMG.sha256"
mkdir -p "$STAGE"
ditto "$APP" "$STAGE/Tingzi.app"
ln -s /Applications "$STAGE/Applications"
hdiutil create -volname 聽字 -srcfolder "$STAGE" -fs HFS+ -format UDZO -ov "$OUT/$DMG" >/dev/null
rm -rf "$STAGE"
hdiutil verify "$OUT/$DMG" >/dev/null
(cd "$OUT" && shasum -a 256 "$DMG" | tee "$DMG.sha256")
