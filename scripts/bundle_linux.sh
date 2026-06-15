#!/usr/bin/env bash
set -euo pipefail

# Netherize Editor - Linux Bundle Script
# Creates a portable Linux distribution package
# Supports cross-compilation from macOS

APP_NAME="netherize-editor"
BINARY="netherize_editor"
VERSION="${1:-v1.0.7-alpha}"
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
TARGET="x86_64-unknown-linux-gnu"
BUILD_DIR="$PROJECT_ROOT/target/$TARGET/release"
BUNDLE_DIR="$PROJECT_ROOT/dist/linux"
TARBALL="$PROJECT_ROOT/dist/${APP_NAME}-${VERSION}-linux-x86_64.tar.gz"

echo "🐧 Building Netherize Editor for Linux..."

# ── 1. Setup cross-compilation (if on macOS) ──────────────────────────────────
if [[ "$OSTYPE" == "darwin"* ]]; then
    echo "📦 Detected macOS - setting up cross-compilation..."

    # Check if cross is installed
    if ! command -v cross &> /dev/null; then
        echo "⚠️  'cross' not found. Installing..."
        cargo install cross --git https://github.com/cross-rs/cross
    fi

    # Add Linux target
    if ! rustup target list | grep -q "$TARGET (installed)"; then
        echo "📦 Adding Linux target..."
        rustup target add "$TARGET"
    fi

    # Build with cross
    echo "🔨 Cross-compiling for Linux..."
    cd "$PROJECT_ROOT"
    cross build --release --target "$TARGET"
else
    # Native Linux build
    echo "🔨 Compiling release build (native)..."
    cd "$PROJECT_ROOT"

    # Add target if needed
    if ! rustup target list | grep -q "$TARGET (installed)"; then
        rustup target add "$TARGET"
    fi

    cargo build --release --target "$TARGET"
fi

# ── 2. Create bundle directory structure ──────────────────────────────────────
echo "📁 Creating Linux bundle..."
rm -rf "$BUNDLE_DIR"
mkdir -p "$BUNDLE_DIR/bin"
mkdir -p "$BUNDLE_DIR/share/applications"
mkdir -p "$BUNDLE_DIR/share/icons/hicolor/256x256/apps"
mkdir -p "$BUNDLE_DIR/share/doc/$APP_NAME"

# ── 3. Copy binary ────────────────────────────────────────────────────────────
echo "📦 Copying binary..."
cp "$BUILD_DIR/$BINARY" "$BUNDLE_DIR/bin/"
chmod +x "$BUNDLE_DIR/bin/$BINARY"

# Strip debug symbols to reduce size
if [[ "$OSTYPE" == "darwin"* ]]; then
    # Use cross-platform strip or skip
    echo "⚠️  Skipping strip on macOS (cross-compiled binary)"
else
    strip "$BUNDLE_DIR/bin/$BINARY" 2>/dev/null || echo "⚠️  strip not available, skipping"
fi

# ── 4. Copy config files ──────────────────────────────────────────────────────
echo "📦 Copying config files..."
cp -r "$PROJECT_ROOT/config" "$BUNDLE_DIR/"

# ── 5. Create desktop entry ───────────────────────────────────────────────────
echo "🖥️  Creating desktop entry..."
cat > "$BUNDLE_DIR/share/applications/$APP_NAME.desktop" << EOF
[Desktop Entry]
Version=1.0
Type=Application
Name=Netherize Editor
Comment=High-performance keyboard-first text editor
Exec=$BINARY %F
Icon=$APP_NAME
Terminal=false
Categories=Development;TextEditor;Utility;
MimeType=text/plain;text/x-rust;text/x-python;text/x-javascript;text/x-typescript;
Keywords=editor;text;code;vim;
StartupNotify=true
EOF

# ── 6. Copy icon (if exists) ──────────────────────────────────────────────────
if [ -f "$PROJECT_ROOT/assets/app_logo_black.png" ]; then
    echo "🎨 Copying application icon..."
    cp "$PROJECT_ROOT/assets/app_logo_black.png" \
       "$BUNDLE_DIR/share/icons/hicolor/256x256/apps/$APP_NAME.png"
fi

# ── 7. Create installation script ─────────────────────────────────────────────
echo "📝 Creating install script..."
cat > "$BUNDLE_DIR/install.sh" << 'EOF'
#!/usr/bin/env bash
set -euo pipefail

INSTALL_DIR="${HOME}/.local"
CONFIG_DIR="${HOME}/.config/netherize"

echo "Installing Netherize Editor..."

# Create directories
mkdir -p "$INSTALL_DIR/bin"
mkdir -p "$INSTALL_DIR/share/applications"
mkdir -p "$INSTALL_DIR/share/icons/hicolor/256x256/apps"
mkdir -p "$CONFIG_DIR"

# Copy binary
cp bin/netherize_editor "$INSTALL_DIR/bin/"
chmod +x "$INSTALL_DIR/bin/netherize_editor"

# Copy config
cp -r config "$CONFIG_DIR/"

# Copy desktop entry
cp share/applications/netherize-editor.desktop "$INSTALL_DIR/share/applications/"

# Copy icon
if [ -f share/icons/hicolor/256x256/apps/netherize-editor.png ]; then
    cp share/icons/hicolor/256x256/apps/netherize-editor.png \
       "$INSTALL_DIR/share/icons/hicolor/256x256/apps/"
fi

# Update desktop database
if command -v update-desktop-database &> /dev/null; then
    update-desktop-database "$INSTALL_DIR/share/applications" 2>/dev/null || true
fi

echo ""
echo "✅ Installation complete!"
echo ""
echo "Binary installed to: $INSTALL_DIR/bin/netherize_editor"
echo "Config installed to: $CONFIG_DIR"
echo ""
echo "Make sure $INSTALL_DIR/bin is in your PATH:"
echo "  export PATH=\"\$HOME/.local/bin:\$PATH\""
echo ""
echo "Run with: netherize_editor"
EOF

chmod +x "$BUNDLE_DIR/install.sh"

# ── 8. Create uninstall script ────────────────────────────────────────────────
cat > "$BUNDLE_DIR/uninstall.sh" << 'EOF'
#!/usr/bin/env bash
set -euo pipefail

INSTALL_DIR="${HOME}/.local"
CONFIG_DIR="${HOME}/.config/netherize"

echo "Uninstalling Netherize Editor..."

rm -f "$INSTALL_DIR/bin/netherize_editor"
rm -f "$INSTALL_DIR/share/applications/netherize-editor.desktop"
rm -f "$INSTALL_DIR/share/icons/hicolor/256x256/apps/netherize-editor.png"

echo ""
echo "✅ Uninstalled!"
echo ""
echo "Config files remain at: $CONFIG_DIR"
echo "To remove config: rm -rf $CONFIG_DIR"
EOF

chmod +x "$BUNDLE_DIR/uninstall.sh"

# ── 9. Create README ──────────────────────────────────────────────────────────
echo "📄 Creating README..."
cat > "$BUNDLE_DIR/README.md" << 'EOF'
# Netherize Editor - Linux Distribution

High-performance, keyboard-first text editor written in Rust.

## Installation

### Quick Install (Recommended)

```bash
./install.sh
```

This will install to `~/.local/bin/netherize_editor` and copy config files to `~/.config/netherize/`.

Make sure `~/.local/bin` is in your PATH:

```bash
export PATH="$HOME/.local/bin:$PATH"
```

Add this line to your `~/.bashrc` or `~/.zshrc` to make it permanent.

### Manual Installation

```bash
# Copy binary
cp bin/netherize_editor ~/.local/bin/
chmod +x ~/.local/bin/netherize_editor

# Copy config
mkdir -p ~/.config/netherize
cp -r config ~/.config/netherize/
```

## Running

```bash
netherize_editor [file]
```

## Uninstallation

```bash
./uninstall.sh
```

## System Requirements

- Linux (x86_64)
- GPU with Vulkan support (or Mesa drivers)
- glibc 2.31+ (Ubuntu 20.04+, Fedora 32+, Arch, etc.)

### Required Libraries

**Ubuntu/Debian:**
```bash
sudo apt install libfontconfig1 libfreetype6
```

**Fedora/RHEL:**
```bash
sudo dnf install fontconfig freetype
```

**Arch:**
```bash
sudo pacman -S fontconfig freetype2
```

## Configuration

Config files are located at `~/.config/netherize/`:
- `config/themes/` - Color themes
- `config/keymaps/` - Keyboard mappings
- `config/ui/` - UI settings

## More Information

- GitHub: https://github.com/yourusername/netherize_editor
- Documentation: See docs/ in source repository
EOF

# ── 10. Create tarball ────────────────────────────────────────────────────────
echo "📦 Creating tarball..."
mkdir -p "$PROJECT_ROOT/dist"
cd "$PROJECT_ROOT/dist"
tar -czf "$(basename "$TARBALL")" -C linux .

# ── 11. Summary ───────────────────────────────────────────────────────────────
BINARY_SIZE=$(du -h "$BUNDLE_DIR/bin/$BINARY" | cut -f1)
TARBALL_SIZE=$(du -h "$TARBALL" | cut -f1)

echo ""
echo "✅ Linux bundle created successfully!"
echo ""
echo "Bundle directory: $BUNDLE_DIR"
echo "Tarball: $TARBALL"
echo ""
echo "Binary size: $BINARY_SIZE"
echo "Tarball size: $TARBALL_SIZE"
echo ""
echo "📦 Bundle contents:"
ls -lh "$BUNDLE_DIR"
echo ""
echo "🚀 To test locally:"
echo "   cd $BUNDLE_DIR && ./install.sh"
echo ""
echo "📤 To distribute:"
echo "   Upload $TARBALL to GitHub Releases"
