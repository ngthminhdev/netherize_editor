#!/usr/bin/env bash
set -euo pipefail

BINARY="netherize_editor"
CLI_ALIAS="netherize"
INSTALL_DIR="${HOME}/.local/bin"
CONFIG_DIR="${HOME}/.config/netherize"
PROJECT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

echo "Building $BINARY (release)..."
cd "$PROJECT_DIR"
cargo build --release

echo "Installing binary → $INSTALL_DIR/$BINARY"
mkdir -p "$INSTALL_DIR"
cp "target/release/$BINARY" "$INSTALL_DIR/$BINARY"
chmod +x "$INSTALL_DIR/$BINARY"
ln -sf "$INSTALL_DIR/$BINARY" "$INSTALL_DIR/$CLI_ALIAS"

echo "Syncing config/themes → $CONFIG_DIR/themes"
mkdir -p "$CONFIG_DIR/themes"
cp -r "$PROJECT_DIR/config/themes/"* "$CONFIG_DIR/themes/"

# Ensure ~/.local/bin is in PATH
if [[ ":$PATH:" != *":$INSTALL_DIR:"* ]]; then
  echo ""
  echo "⚠ Add to your shell profile (~/.zshrc or ~/.bashrc):"
  echo '  export PATH="$HOME/.local/bin:$PATH"'
fi

echo ""
echo "Done. Run: $CLI_ALIAS <file>"
