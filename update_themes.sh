#!/bin/bash
# Script to update all theme files with vibrant syntax colors

THEMES_DIR="/Users/qc-bright/Project/netherize_editor/config/themes"

# Function to update a theme file
update_theme() {
    local file="$1"
    local theme_name=$(basename "$file" .toml)

    echo "Updating $theme_name..."

    # Backup original
    cp "$file" "$file.bak"

    # Read current colors to preserve theme identity
    local keyword=$(grep '^keyword = ' "$file" | head -1 | cut -d'"' -f2)
    local string=$(grep '^string = ' "$file" | head -1 | cut -d'"' -f2)
    local function=$(grep '^function = ' "$file" | head -1 | cut -d'"' -f2)
    local comment=$(grep '^comment = ' "$file" | head -1 | cut -d'"' -f2)
    local type=$(grep '^type = ' "$file" | head -1 | cut -d'"' -f2)
    local number=$(grep '^number = ' "$file" | head -1 | cut -d'"' -f2)

    # Calculate vibrant variants (brighten by ~15%)
    # For now, we'll use the strategy:
    # - variable/parameter: use a salmon/pink tone or brighten identifier
    # - property/field: use red/coral or brighten field color
    # - operator: use orange/gold or brighten operator

    echo "  keyword: $keyword"
    echo "  string: $string"
    echo "  function: $function"
}

# Update all themes except default-dark (already done)
for theme in "$THEMES_DIR"/*.toml; do
    if [[ "$(basename "$theme")" != "default-dark.toml" ]]; then
        update_theme "$theme"
    fi
done

echo "Done!"
