#!/usr/bin/env bash
set -euo pipefail

# Netherize Editor - Windows Bundle Script
# Cross-compile from macOS to Windows x86_64

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
TARGET="x86_64-pc-windows-msvc"
BUILD_DIR="$PROJECT_ROOT/target/$TARGET/release"
BUNDLE_DIR="$PROJECT_ROOT/dist/windows"

echo "🪟 Building Netherize Editor for Windows..."

# Check if cargo-xwin is installed
if ! command -v cargo-xwin &> /dev/null; then
    echo "⚠️  cargo-xwin not found. Installing..."
    cargo install cargo-xwin
fi

# Add Windows target if not already added
if ! rustup target list | grep -q "$TARGET (installed)"; then
    echo "📦 Adding Windows target..."
    rustup target add "$TARGET"
fi

# Build release binary
echo "🔨 Compiling for Windows..."
cd "$PROJECT_ROOT"
cargo xwin build --release --target "$TARGET"

# Create bundle directory
echo "📁 Creating Windows bundle..."
rm -rf "$BUNDLE_DIR"
mkdir -p "$BUNDLE_DIR"

# Copy executable
cp "$BUILD_DIR/netherize_editor.exe" "$BUNDLE_DIR/"

# Copy config files
cp -r "$PROJECT_ROOT/config" "$BUNDLE_DIR/"

# Create README
cat > "$BUNDLE_DIR/README.txt" << 'EOF'
Netherize Editor - Windows Build

Installation:
1. Extract all files to a folder
2. Double-click netherize_editor.exe to run
3. Config files are in the ./config/ directory

System Requirements:
- Windows 10/11 (64-bit)
- GPU with Vulkan or DirectX 12 support

For more info: https://github.com/yourusername/netherize_editor
EOF

# Create launcher script (optional)
cat > "$BUNDLE_DIR/launch.bat" << 'EOF'
@echo off
cd /d "%~dp0"
netherize_editor.exe
pause
EOF

echo "✅ Windows bundle created at: $BUNDLE_DIR"
echo ""
echo "📦 Bundle contents:"
ls -lh "$BUNDLE_DIR"
echo ""
echo "💡 To create a ZIP archive:"
echo "   cd dist && zip -r netherize-editor-windows.zip windows/"
