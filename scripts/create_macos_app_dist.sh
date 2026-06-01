#!/usr/bin/env bash
# Create a macOS .app for Velotype
# Usage: ./scripts/create_app_dist.sh
set -euo pipefail

BINARY_NAME="velotype"
APP_NAME="Velotype"

PROJECT_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
DIST_DIR="$PROJECT_ROOT/dist"
ICON_SOURCE="$PROJECT_ROOT/resources/AppIcon.png"

echo "==> Clean old dists."
rm -rf "$DIST_DIR"
mkdir -p "$DIST_DIR"

echo "==> Build Release binary."
cargo build --release

echo "==> Create App Bundle struct."
APP_DIR="$DIST_DIR/$APP_NAME.app"
mkdir -p "$APP_DIR/Contents/MacOS"
mkdir -p "$APP_DIR/Contents/Resources"
cp "$PROJECT_ROOT/target/release/$BINARY_NAME" "$APP_DIR/Contents/MacOS/"
cp resources/macos/Info.plist "$APP_DIR/Contents/"
cp resources/macos/$BINARY_NAME.icns "$APP_DIR/Contents/Resources/$BINARY_NAME.icns"

echo "==> Copy resources files"
[ -f "$PROJECT_ROOT/README.md" ] && cp "$PROJECT_ROOT/README.md" "$APP_DIR/Contents/Resources/"

echo "==> ✅ Done"
echo "    Output: $APP_DIR"
echo "       Use: open '$APP_DIR'"
