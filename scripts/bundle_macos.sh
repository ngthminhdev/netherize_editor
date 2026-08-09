#!/usr/bin/env bash
set -euo pipefail

APP_NAME="Netherize"
BINARY="netherize_editor"
VERSION="${1:-v1.1.0-alpha}"
PROJECT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
BUNDLE="$PROJECT_DIR/target/$APP_NAME.app"
ZIP="$PROJECT_DIR/target/${APP_NAME}-${VERSION}-macos.zip"
LOGO_SRC="$PROJECT_DIR/assets/app_logo_black.png"

# ── 1. Build ───────────────────────────────────────────────────────────────────
echo "Building $BINARY (release)..."
cd "$PROJECT_DIR"
cargo build --release

# ── 2. Bundle skeleton ────────────────────────────────────────────────────────
echo "Creating .app bundle at $BUNDLE"
rm -rf "$BUNDLE"
mkdir -p "$BUNDLE/Contents/MacOS"
mkdir -p "$BUNDLE/Contents/Resources"

# ── 3. Binary ─────────────────────────────────────────────────────────────────
cp "target/release/$BINARY" "$BUNDLE/Contents/MacOS/$BINARY"
chmod +x "$BUNDLE/Contents/MacOS/$BINARY"

# ── 4. Config (must sit next to binary so exe.parent()/config/ resolves) ──────
rm -rf "$BUNDLE/Contents/MacOS/config"
cp -R "$PROJECT_DIR/config" "$BUNDLE/Contents/MacOS/config"

# ── 5. Icon: PNG → ICNS ───────────────────────────────────────────────────────
ICONSET="$PROJECT_DIR/target/AppIcon.iconset"
rm -rf "$ICONSET"
mkdir -p "$ICONSET"

sips -z 16   16   "$LOGO_SRC" --out "$ICONSET/icon_16x16.png"      >/dev/null
sips -z 32   32   "$LOGO_SRC" --out "$ICONSET/icon_16x16@2x.png"   >/dev/null
sips -z 32   32   "$LOGO_SRC" --out "$ICONSET/icon_32x32.png"      >/dev/null
sips -z 64   64   "$LOGO_SRC" --out "$ICONSET/icon_32x32@2x.png"   >/dev/null
sips -z 128  128  "$LOGO_SRC" --out "$ICONSET/icon_128x128.png"    >/dev/null
sips -z 256  256  "$LOGO_SRC" --out "$ICONSET/icon_128x128@2x.png" >/dev/null
sips -z 256  256  "$LOGO_SRC" --out "$ICONSET/icon_256x256.png"    >/dev/null
sips -z 512  512  "$LOGO_SRC" --out "$ICONSET/icon_256x256@2x.png" >/dev/null
sips -z 512  512  "$LOGO_SRC" --out "$ICONSET/icon_512x512.png"    >/dev/null
sips -z 1024 1024 "$LOGO_SRC" --out "$ICONSET/icon_512x512@2x.png" >/dev/null

iconutil -c icns "$ICONSET" -o "$BUNDLE/Contents/Resources/AppIcon.icns"
rm -rf "$ICONSET"

# ── 6. Info.plist ─────────────────────────────────────────────────────────────
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
  <key>CFBundleDisplayName</key>
  <string>$APP_NAME</string>
  <key>CFBundleVersion</key>
  <string>0.1.0</string>
  <key>CFBundleShortVersionString</key>
  <string>0.1.0</string>
  <key>CFBundlePackageType</key>
  <string>APPL</string>
  <key>CFBundleIconFile</key>
  <string>AppIcon</string>
  <key>NSHighResolutionCapable</key>
  <true/>
  <key>LSMinimumSystemVersion</key>
  <string>12.0</string>
</dict>
</plist>
PLIST

# Refuse to package a stale binary. This catches copy/path regressions before a
# developer replaces the daily-use app with a bundle that is not the build above.
# Compare before codesigning because signing intentionally mutates the Mach-O.
SOURCE_HASH="$(shasum -a 256 "$PROJECT_DIR/target/release/$BINARY" | awk '{print $1}')"
BUNDLE_HASH="$(shasum -a 256 "$BUNDLE/Contents/MacOS/$BINARY" | awk '{print $1}')"
if [[ "$SOURCE_HASH" != "$BUNDLE_HASH" ]]; then
  echo "Bundle verification failed: binary hash mismatch" >&2
  exit 1
fi
echo "Verified bundled binary: $BUNDLE_HASH"

# ── 7. Codesign (ad-hoc, no developer account needed) ─────────────────────────
if codesign --force --deep --sign - "$BUNDLE" 2>/dev/null; then
  codesign --verify --deep --strict "$BUNDLE"
  echo "Ad-hoc signature verified"
else
  echo "Codesign skipped (install Xcode CLT if needed)"
fi

# ── 8. Zip for GitHub Release ─────────────────────────────────────────────────
echo "Creating release zip → $ZIP"
rm -f "$ZIP"
cd "$PROJECT_DIR/target"
zip -r --symlinks "$(basename "$ZIP")" "$APP_NAME.app" >/dev/null
cd "$PROJECT_DIR"

echo ""
echo "Bundle : $BUNDLE"
echo "Release : $ZIP ($(du -sh "$ZIP" | cut -f1))"
echo ""
echo "Upload $ZIP to GitHub Releases as attachment."
echo ""
echo "Install locally:"
echo "  cp -r '$BUNDLE' /Applications/"
