#!/bin/sh
# Builds the release binary and wraps it in an ad-hoc signed target/Wisp.app.
set -eu
cd "$(dirname "$0")/.."

cargo build --release

APP=target/Wisp.app
rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS"
cp target/release/wisp "$APP/Contents/MacOS/wisp"
cp bundle/Info.plist "$APP/Contents/Info.plist"
# Cargo.toml is the single source of the version number.
VERSION=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -n 1)
plutil -replace CFBundleShortVersionString -string "$VERSION" "$APP/Contents/Info.plist"
codesign --force --sign - "$APP"
echo "$APP"
