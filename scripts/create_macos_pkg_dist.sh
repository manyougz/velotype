#!/usr/bin/env bash
# Create a macOS PKG installer for Velotype
# Usage: ./scripts/create_pkg_dist.sh <version>

set -euo pipefail

if [ $# -eq 0 ]; then
    echo "Usage: $0 <version>"
    echo "Example: $0 0.1.0"
    exit 1
fi

VERSION="$1"

PROJECT_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
DIST_DIR="$PROJECT_ROOT/dist"
RESOURCES_DIR="$PROJECT_ROOT/resources/macos"

APP_NAME="Velotype"
APP_BUNDLE="${APP_NAME}.app"
BINARY_NAME="velotype"
BUNDLE_ID="com.manyougz.Velotype"

PKG_DIR="$DIST_DIR/pkg"
PKG_NAME="${APP_NAME}-${VERSION}.pkg"
COMPONENT_PKG="${APP_NAME}-component.pkg"

INSTALL_LOCATION="/Applications"
CLI_LINK="/usr/local/bin/${BINARY_NAME}"

if [ ! -d "$DIST_DIR/$APP_BUNDLE" ]; then
    echo "Error: $APP_BUNDLE not found at $DIST_DIR"
    echo "Please run create_app_dist.sh first"
    exit 1
fi

if [ ! -f "$RESOURCES_DIR/pkg/Distribution.xml" ]; then
    echo "Error: Distribution.xml not found at $RESOURCES_DIR/pkg/"
    exit 1
fi

if [ ! -f "$RESOURCES_DIR/pkg/postinstall" ]; then
    echo "Error: postinstall script not found at $RESOURCES_DIR/pkg/"
    exit 1
fi

echo "==> Creating PKG installer for $APP_NAME $VERSION"
echo "    Bundle ID: $BUNDLE_ID"
echo "    Install location: $INSTALL_LOCATION"
echo "    CLI tool: $CLI_LINK"

rm -rf "$PKG_DIR"
mkdir -p "$PKG_DIR/root/Applications"
mkdir -p "$PKG_DIR/scripts"

echo "==> Preparing installation payload..."
cp -R "$DIST_DIR/$APP_BUNDLE" "$PKG_DIR/root/Applications/"

echo "==> Copying installation scripts..."
cp "$RESOURCES_DIR/pkg/postinstall" "$PKG_DIR/scripts/"
cp "$RESOURCES_DIR/pkg/preuninstall" "$PKG_DIR/scripts/"
chmod +x "$PKG_DIR/scripts/"*

# Sign the app bundle (ad-hoc signature for development)
# This is required for the PKG installer to work properly
echo "==> Signing app bundle..."
# Remove old signature first
xattr -cr "$PKG_DIR/root/Applications/$APP_BUNDLE" 2>/dev/null || true
# Apply ad-hoc signature
codesign --force --deep --sign - "$PKG_DIR/root/Applications/$APP_BUNDLE" 2>&1 || {
    echo "Warning: Code signing failed. This may prevent proper installation."
    echo "For production: sign with a developer certificate"
    echo "For development: users may need to manually allow the app in System Preferences"
}

echo "==> Creating component package..."
pkgbuild --identifier "$BUNDLE_ID" \
    --version "$VERSION" \
    --scripts "$PKG_DIR/scripts" \
    --root "$PKG_DIR/root" \
    --install-location "/" \
    "$PKG_DIR/$COMPONENT_PKG"

echo "==> Creating distribution package..."
cp "$RESOURCES_DIR/pkg/Distribution.xml" "$PKG_DIR/"

productbuild --distribution "$PKG_DIR/Distribution.xml" \
    --package-path "$PKG_DIR" \
    "$DIST_DIR/$PKG_NAME"

echo "==> Fixing package metadata..."
pkgutil --expand "$DIST_DIR/$PKG_NAME" "$PKG_DIR/expanded" || true
if [ -f "$PKG_DIR/expanded/$COMPONENT_PKG/PackageInfo" ]; then
    sed -i '' '/<relocate>/,/<\/relocate>/d' "$PKG_DIR/expanded/$COMPONENT_PKG/PackageInfo"
    pkgutil --flatten "$PKG_DIR/expanded" "$DIST_DIR/$PKG_NAME"
    rm -rf "$PKG_DIR/expanded"
fi

echo "==> ✅ PKG installer created successfully!"
echo "    Output: $DIST_DIR/$PKG_NAME"
echo "      Size: $(du -h "$DIST_DIR/$PKG_NAME" | cut -f1)"
