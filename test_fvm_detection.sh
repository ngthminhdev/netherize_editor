#!/bin/bash
# Test FVM detection logic

echo "=== Testing FVM Detection ==="
echo ""

# Test workspace with local FVM
TEST_WORKSPACE="/Users/qc-bright/Project/mine_wallet"
echo "Test workspace: $TEST_WORKSPACE"
echo ""

# Check if .fvm exists
if [ -d "$TEST_WORKSPACE/.fvm" ]; then
    echo "✓ .fvm directory exists"
else
    echo "✗ .fvm directory not found"
    exit 1
fi

# Check if pubspec.yaml exists
if [ -f "$TEST_WORKSPACE/pubspec.yaml" ]; then
    echo "✓ pubspec.yaml exists"
else
    echo "✗ pubspec.yaml not found"
    exit 1
fi

# Check if local FVM dart binary exists
LOCAL_DART="$TEST_WORKSPACE/.fvm/flutter_sdk/bin/cache/dart-sdk/bin/dart"
if [ -f "$LOCAL_DART" ]; then
    echo "✓ Local FVM dart binary exists: $LOCAL_DART"
    echo "  Version: $($LOCAL_DART --version 2>&1 | head -1)"
else
    echo "✗ Local FVM dart binary not found"
    exit 1
fi

echo ""
echo "=== FVM Detection Test Passed ==="
echo ""
echo "Expected behavior:"
echo "  When opening a .dart file in $TEST_WORKSPACE,"
echo "  the LSP server should use: $LOCAL_DART"
echo "  instead of system dart"
