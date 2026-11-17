#!/bin/bash
# Builds Tingzi.app and a ZIP of it. Run on a Mac: bash macos/build.sh [output folder]
# Needs Rust (cargo), Python 3 and the Xcode command line tools.
set -euo pipefail
cd "$(dirname "$0")/.."
ROOT="$PWD"
OUT="${1:-$ROOT/.build/macos}"
mkdir -p "$OUT"
OUT="$(cd "$OUT" && pwd)"
APP="$OUT/Tingzi.app"
VERSION="$(sed -n 's/^version = "\(.*\)"/\1/p' app/src-tauri/Cargo.toml | head -1)"
HOST="$(rustc -vV | sed -n 's/^host: //p')"

# Source paths are remapped so the program does not carry this computer's folder names.
export RUSTFLAGS="--remap-path-prefix=$HOME=/_ --remap-path-prefix=$ROOT=/_/src"
cargo build --release --locked --manifest-path app/src-tauri/Cargo.toml --target-dir "$OUT/app-target"

rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
cp "$OUT/app-target/release/Tingzi" "$APP/Contents/MacOS/Tingzi"

# The readings of each character, which the app uses to match the user's own words by sound.
unzip -o -q windows/standalone-data/public-data.zip phrases.txt McBopomofo-LICENSE.txt -d "$APP/Contents/Resources"
cp LICENSE "$APP/Contents/Resources/LICENSE"
sips -s format icns app/src-tauri/icons/icon.png --out "$APP/Contents/Resources/Tingzi.icns" >/dev/null

# The licences of every Rust package built into the program for this kind of Mac.
cargo metadata --format-version 1 --locked --filter-platform "$HOST" --manifest-path app/src-tauri/Cargo.toml > "$OUT/cargo-metadata.json"
python3 macos/notices.py "$OUT/cargo-metadata.json" "$APP/Contents/Resources/THIRD-PARTY-NOTICES.txt"

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

# Signed for this Mac only; there is no Developer ID yet, so Gatekeeper warns on other Macs.
codesign --force --sign - "$APP"

ZIP="$OUT/Tingzi-$VERSION-macos-$(uname -m).zip"
rm -f "$ZIP"
ditto -c -k --keepParent "$APP" "$ZIP"
shasum -a 256 "$ZIP"
