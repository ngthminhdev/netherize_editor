#!/usr/bin/env bash
set -euo pipefail

APP_NAME="Netherize"
BINARY="netherize_editor"
PROJECT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
BUNDLE="$PROJECT_DIR/target/$APP_NAME.app"

echo "Building release binary..."
cd "$PROJECT_DIR"
cargo build --release

echo "Creating .app bundle at $BUNDLE"
rm -rf "$BUNDLE"
mkdir -p "$BUNDLE/Contents/MacOS"
mkdir -p "$BUNDLE/Contents/Resources/themes"

cp "target/release/$BINARY" "$BUNDLE/Contents/MacOS/$BINARY"
cp -r "config/themes/"* "$BUNDLE/Contents/Resources/themes/"

cat > "$BUNDLE/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleExecutable</key>
  <string>$BINARY</string>
  <key>CFBundleIdentifier</key>
  <string>com.netherize.editor</string>
  <key>CFBundleName</key>
  <string>$APP_NAME</string>
  <key>CFBundleVersion</key>
  <string>0.1.0</string>
  <key>CFBundlePackageType</key>
  <string>APPL</string>
  <key>NSHighResolutionCapable</key>
  <true/>
  <key>LSMinimumSystemVersion</key>
  <string>12.0</string>
</dict>
</plist>
PLIST

echo ""
echo "Bundle ready: $BUNDLE"
echo "To install: cp -r '$BUNDLE' /Applications/"
