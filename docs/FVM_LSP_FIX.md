# Dart/Flutter LSP FVM Support - Fix Summary

## Problem
LSP server cho Dart/Flutter không hoạt động với error:
```
[AppShell] worker LspClient failed: lsp didOpen rejected: server is not running
```

**Root Cause:** Editor đang sử dụng system `dart` binary thay vì FVM-managed dart binary trong workspace. Khi workspace sử dụng FVM (Flutter Version Manager) để quản lý Flutter/Dart version, LSP server cần sử dụng đúng version từ FVM, không phải system dart.

## Solution

### 1. Added FVM Detection Logic (`src/lsp/client.rs`)

Thêm function `detect_fvm_dart_binary()` với priority:
1. **Local FVM** (highest priority): `.fvm/flutter_sdk/bin/cache/dart-sdk/bin/dart`
   - Workspace-specific version được config trong `.fvm/fvm_config.json`
2. **Global FVM cache**: `~/.fvm/versions/*/bin/cache/dart-sdk/bin/dart`
   - Tìm version mới nhất trong global cache
3. **System dart** (fallback): Resolve từ PATH

### 2. Modified `resolve_lsp_server_command()`

Trước khi fallback sang `detect_lsp_server_for_workspace()`, check FVM first:
```rust
pub fn resolve_lsp_server_command(
    requested_command: Option<&str>,
    root_path: &Path,
) -> Option<String> {
    // ... existing logic ...
    
    // Special handling for Dart: prioritize FVM
    if let Some(dart_binary) = detect_fvm_dart_binary(root_path) {
        return Some(dart_binary);
    }

    detect_lsp_server_for_workspace(root_path)
}
```

### 3. Detection Flow

```
User opens .dart file
    ↓
LSP scheduler calls spawn_lsp_server()
    ↓
resolve_lsp_server_command() checks:
    1. Check if workspace has pubspec.yaml (Dart/Flutter project)
    2. If yes, look for .fvm/flutter_sdk/bin/cache/dart-sdk/bin/dart
    3. If found, return full path to FVM dart
    4. If not, check ~/.fvm/versions/* (global FVM)
    5. If not, fallback to "dart" (system)
    ↓
spawn_lsp_server() uses resolved dart binary
    ↓
LSP server starts with correct Dart version
```

## Testing

### Test Workspace
- Path: `/Users/qc-bright/Project/mine_wallet`
- FVM Config: Flutter 3.35.3
- Local FVM dart: `/Users/qc-bright/Project/mine_wallet/.fvm/flutter_sdk/bin/cache/dart-sdk/bin/dart`
- Dart version: 3.9.2 (stable)

### Verification
1. ✓ Code compiles without errors
2. ✓ Test script confirms FVM dart binary exists
3. ✓ Unit tests pass (2/2)
4. ✓ Cerebrum.md updated with learning

## Files Modified

1. `src/lsp/client.rs`
   - Added `detect_fvm_dart_binary()` function
   - Modified `resolve_lsp_server_command()` to check FVM first

2. `.wolf/cerebrum.md`
   - Added Key Learning about Dart/Flutter LSP with FVM

3. `tests/lsp_fvm_detection.rs` (new)
   - Unit tests for FVM detection

4. `test_fvm_detection.sh` (new)
   - Integration test script

## Expected Behavior After Fix

When opening a `.dart` file in a workspace with FVM:
- ✓ LSP server uses FVM-managed dart binary
- ✓ LSP features work correctly (completion, hover, diagnostics)
- ✓ No more "server is not running" errors
- ✓ Dart version matches project's FVM configuration

## Notes

- FVM detection only activates for workspaces with `pubspec.yaml`
- Non-Dart workspaces are unaffected
- System dart is still used as fallback if FVM is not configured
- Global FVM cache is checked if local FVM is not present
