#!/bin/bash
# Checks a disk image made by build.sh: it opens, holds a signed Tingzi.app for both kinds of Mac beside a
# shortcut to Applications, matches its .sha256, and the app copied out of it still passes its signature check.
# Run on a Mac: bash macos/check_dmg.sh Tingzi-<version>-macos.dmg
set -euo pipefail
DMG="$(cd "$(dirname "$1")" && pwd)/$(basename "$1")"
(cd "$(dirname "$DMG")" && shasum -a 256 -c "$(basename "$DMG").sha256")
MOUNT="$(mktemp -d)"
COPY="$(mktemp -d)"
trap 'hdiutil detach "$MOUNT" -quiet >/dev/null 2>&1 || true; rm -rf "$COPY"' EXIT
hdiutil attach "$DMG" -nobrowse -readonly -mountpoint "$MOUNT" >/dev/null
test "$(readlink "$MOUNT/Applications")" = /Applications
APP="$MOUNT/Tingzi.app"
lipo "$APP/Contents/MacOS/Tingzi" -verify_arch arm64 x86_64
codesign --verify --strict "$APP"
test "$(plutil -extract CFBundleShortVersionString raw "$APP/Contents/Info.plist")" = "$(basename "$DMG" | sed 's/^Tingzi-\(.*\)-macos\.dmg$/\1/')"
# What a user does: drag the app to a folder of applications.
ditto "$APP" "$COPY/Tingzi.app"
codesign --verify --strict "$COPY/Tingzi.app"
echo "$(basename "$DMG") is ready"
