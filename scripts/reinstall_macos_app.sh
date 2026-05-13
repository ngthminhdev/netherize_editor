#!/usr/bin/env bash
set -euo pipefail

APP_NAME="Netherize"
BUNDLE_ID="com.netherize.editor"
BINARY="netherize_editor"
RESET_LS_CACHE="${RESET_LS_CACHE:-0}"
PROJECT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TARGET_APP="$PROJECT_DIR/target/$APP_NAME.app"
APPLICATIONS_APP="/Applications/$APP_NAME.app"
LSREGISTER="/System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister"
TEST_FILE="${1:-$PROJECT_DIR/src/lib.rs}"

run() {
  echo "+ $*"
  "$@"
}

rtk_run() {
  if command -v rtk >/dev/null 2>&1; then
    echo "+ rtk $*"
    rtk "$@"
  else
    echo "+ $*"
    "$@"
  fi
}

echo "== Netherize macOS full reinstall =="
echo "Project      : $PROJECT_DIR"
echo "Target app   : $TARGET_APP"
echo "Install app  : $APPLICATIONS_APP"
echo "Test file    : $TEST_FILE"
echo ""

cd "$PROJECT_DIR"

# Stop the currently running app so the bundle can be replaced cleanly.
run /usr/bin/pkill -x "$APP_NAME" 2>/dev/null || true
run /usr/bin/pkill -x "$BINARY" 2>/dev/null || true

# Remove stale build/install outputs first to avoid LaunchServices seeing old bundle metadata.
run /bin/rm -rf "$TARGET_APP"
run /bin/rm -rf "$APPLICATIONS_APP"

# Build and create target/Netherize.app using the project bundle script.
rtk_run sh scripts/os_integration/bundle_macos.sh

# Validate generated Info.plist before copying it into /Applications.
run /usr/bin/plutil -lint "$TARGET_APP/Contents/Info.plist"
echo ""
echo "Generated document declarations:"
rtk_run /usr/libexec/PlistBuddy -c "Print :CFBundleDocumentTypes" "$TARGET_APP/Contents/Info.plist"
echo ""

# Install fresh app bundle.
run /bin/cp -R "$TARGET_APP" "$APPLICATIONS_APP"

# Ensure ad-hoc signature is fresh after copy.
run /usr/bin/codesign --force --deep --sign - "$APPLICATIONS_APP"

# Refresh LaunchServices. First unregister old records for both target and installed paths,
# then register the installed bundle again.
run "$LSREGISTER" -u "$TARGET_APP" 2>/dev/null || true
run "$LSREGISTER" -u "$APPLICATIONS_APP" 2>/dev/null || true

if [ "$RESET_LS_CACHE" = "1" ]; then
  echo "Rebuilding LaunchServices cache for local/system/user domains..."
  run "$LSREGISTER" -r -domain local -domain system -domain user || true
fi

run "$LSREGISTER" -f "$APPLICATIONS_APP"
run "$LSREGISTER" -R -f "$APPLICATIONS_APP" 2>/dev/null || true

# Touch Finder/LaunchServices related processes so Finder/Open With reloads metadata sooner.
run /usr/bin/killall Finder 2>/dev/null || true
run /usr/bin/killall sharedfilelistd 2>/dev/null || true

# Show what macOS thinks about the test file and the installed app.
echo ""
echo "Installed document declarations:"
rtk_run /usr/libexec/PlistBuddy -c "Print :CFBundleDocumentTypes" "$APPLICATIONS_APP/Contents/Info.plist"

echo ""
echo "Test file UTI:"
rtk_run mdls -name kMDItemContentType -name kMDItemContentTypeTree "$TEST_FILE"

echo ""
echo "LaunchServices records for $BUNDLE_ID:"
rtk_run sh -c "$LSREGISTER -dump | grep -A80 -B8 '$BUNDLE_ID' | head -n 220"

echo ""
echo "LaunchServices document claims mentioning Netherize:"
rtk_run sh -c "$LSREGISTER -dump | grep -A16 -B4 'bundle:                     Netherize' | head -n 220"

echo ""
echo "Direct binary smoke test command:"
echo "  $APPLICATIONS_APP/Contents/MacOS/$BINARY '$TEST_FILE'"
echo ""
echo "Open With smoke test command:"
echo "  open -a '$APP_NAME' '$TEST_FILE'"
echo ""
echo "Done. If 'open -a' still shows 'cannot open files', rerun this script with a full LaunchServices reset:"
echo "  RESET_LS_CACHE=1 ./scripts/reinstall_macos_app.sh '$TEST_FILE'"
