#!/usr/bin/env bash
set -euo pipefail

APP_NAME="Netherize"
BINARY="netherize_editor"
VERSION="${1:-v1.0.2-alpha}"
PROJECT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
BUNDLE="$PROJECT_DIR/target/$APP_NAME.app"
ZIP="$PROJECT_DIR/target/${APP_NAME}-${VERSION}-macos.zip"
LOGO_SRC="$PROJECT_DIR/assets/app_logo_black.png"

echo "Building $BINARY (release)..."
cd "$PROJECT_DIR"
cargo build --release

echo "Creating .app bundle at $BUNDLE"
rm -rf "$BUNDLE"
mkdir -p "$BUNDLE/Contents/MacOS"
mkdir -p "$BUNDLE/Contents/Resources"

cp "target/release/$BINARY" "$BUNDLE/Contents/MacOS/$BINARY"
chmod +x "$BUNDLE/Contents/MacOS/$BINARY"

rm -rf "$BUNDLE/Contents/MacOS/config"
cp -R "$PROJECT_DIR/config" "$BUNDLE/Contents/MacOS/config"

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
  <key>CFBundleDocumentTypes</key>
  <array>
    <dict>
      <key>CFBundleTypeName</key>
      <string>Rust Source File</string>
      <key>CFBundleTypeRole</key>
      <string>Editor</string>
      <key>LSHandlerRank</key>
      <string>Alternate</string>
      <key>CFBundleTypeExtensions</key>
      <array>
        <string>rs</string>
      </array>
      <key>LSItemContentTypes</key>
      <array>
        <string>org.rust-lang.rust-script</string>
        <string>com.apple.dt.document.rust-source</string>
        <string>public.rust-source</string>
      </array>
    </dict>
    <dict>
      <key>CFBundleTypeName</key>
      <string>Markdown Document</string>
      <key>CFBundleTypeRole</key>
      <string>Editor</string>
      <key>LSHandlerRank</key>
      <string>Alternate</string>
      <key>CFBundleTypeExtensions</key>
      <array>
        <string>md</string>
        <string>markdown</string>
      </array>
      <key>LSItemContentTypes</key>
      <array>
        <string>net.daringfireball.markdown</string>
        <string>public.markdown</string>
      </array>
    </dict>
    <dict>
      <key>CFBundleTypeName</key>
      <string>Text and Source Files</string>
      <key>CFBundleTypeRole</key>
      <string>Editor</string>
      <key>LSHandlerRank</key>
      <string>Alternate</string>
      <key>CFBundleTypeExtensions</key>
      <array>
        <string>txt</string>
        <string>text</string>
        <string>toml</string>
        <string>json</string>
        <string>jsonc</string>
        <string>yaml</string>
        <string>yml</string>
        <string>xml</string>
        <string>html</string>
        <string>css</string>
        <string>js</string>
        <string>jsx</string>
        <string>ts</string>
        <string>tsx</string>
        <string>py</string>
        <string>go</string>
        <string>java</string>
        <string>c</string>
        <string>h</string>
        <string>cpp</string>
        <string>hpp</string>
        <string>sh</string>
        <string>bash</string>
        <string>zsh</string>
        <string>fish</string>
        <string>sql</string>
        <string>proto</string>
        <string>dockerfile</string>
        <string>conf</string>
        <string>ini</string>
        <string>log</string>
      </array>
      <key>CFBundleTypeOSTypes</key>
      <array>
        <string>TEXT</string>
      </array>
      <key>LSItemContentTypes</key>
      <array>
        <string>public.plain-text</string>
        <string>public.text</string>
        <string>public.source-code</string>
        <string>public.script</string>
        <string>public.shell-script</string>
        <string>public.json</string>
        <string>public.xml</string>
        <string>public.yaml</string>
        <string>public.python-script</string>
        <string>public.typescript-source</string>
        <string>com.netscape.javascript-source</string>
        <string>com.google.go-source</string>
        <string>com.apple.property-list</string>
      </array>
    </dict>
  </array>
  <key>NSHighResolutionCapable</key>
  <true/>
  <key>LSMinimumSystemVersion</key>
  <string>12.0</string>
</dict>
</plist>
PLIST

codesign --force --deep --sign - "$BUNDLE" 2>/dev/null && echo "Ad-hoc signed" || echo "Codesign skipped (install Xcode CLT if needed)"

echo "Creating release zip -> $ZIP"
rm -f "$ZIP"
cd "$PROJECT_DIR/target"
zip -r --symlinks "$(basename "$ZIP")" "$APP_NAME.app" >/dev/null
cd "$PROJECT_DIR"

echo ""
echo "Bundle  : $BUNDLE"
echo "Release : $ZIP ($(du -sh "$ZIP" | cut -f1))"
echo ""
echo "Install locally:"
echo "  cp -r '$BUNDLE' /Applications/"
echo "  /System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister -f /Applications/$APP_NAME.app"
