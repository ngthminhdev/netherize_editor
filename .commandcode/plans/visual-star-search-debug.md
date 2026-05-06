# Visual Star Search — Debug & Fix Plan

## Vấn đề
Bấm `*` trong Visual Mode bọc text bằng dấu `*` (vd: `*abc*`) thay vì search. Codebase đã có logic đúng nhưng vẫn gặp bug.

## Phân tích
- `config/keymaps/default.toml:315`: `*` visual → `editor.search_word_under_cursor` ✅
- `src/app/app_state/state.rs:420-428`: `search_word_under_cursor()` dùng `visual_selection_text()` ✅
- `src/core/command_dispatch/navigation.rs:186-197`: dispatch + exit visual mode ✅
- `WrapSelectionWithStar` không có key binding nào, không có fallback ✅
- **Root cause nghi ngờ**: TOML keymap không load được → `*` trong visual mode không resolve → có thể `WrapSelectionWithStar` bị trigger từ code path khác (3 files đã modified trong git status)

## Các bước thực hiện

### Bước 1: Chạy `git diff` để xác định 3 files modified
```bash
rtk git diff --name-only
rtk git diff
```
Kiểm tra xem file nào bị thay đổi — có thể một trong số đó gây ra bug.

### Bước 2: Thêm logging vào keymap loading
**File:** `src/config/keymap_loader.rs`

Trong hàm `load()`, sau khi bindings được thu thập:
```rust
log::info!(
    "[keymap] profile='{}' path={:?} bindings_count={}",
    profile, path.as_ref(), bindings.len()
);
for b in &bindings {
    if b.key == "*" {
        log::info!("[keymap] STAR binding: mode={:?} command={}", b.mode, b.command);
    }
}
```

Trong `find_profile_path()`, log các path đã thử:
```rust
log::debug!("[keymap] find_profile: cwd={:?} exe_dir={:?}", cwd_path, exe_path);
```

### Bước 3: Thêm logging vào keymap resolution
**File:** `src/app/resolved_keymap.rs`

Trong hàm `lookup()`, thêm block này trước khi return:
```rust
if mode_str == "visual" && input.text.as_deref() == Some("*") {
    log::info!(
        "[keymap] lookup '*' in visual: specs={:?} result={:?}",
        input_to_specs(input), result
    );
}
```

### Bước 4: Thêm logging vào input routing
**File:** `src/app/input/handler.rs`

Trong `route_normalized_input()`, tại vị trí gọi `input_map.resolve()` cuối cùng (khoảng line 959):
```rust
if normalized.text.as_deref() == Some("*") {
    if let Some(ref m) = resolved {
        log::info!(
            "[input] STAR → {:?} (mode={}, reason={})",
            m.command, context.mode.as_str(), m.reason
        );
    } else {
        log::warn!(
            "[input] STAR UNRESOLVED (mode={}, focus={:?})",
            context.mode.as_str(), context.focus
        );
    }
}
```

### Bước 5: Thêm safeguard — hardcode `*` trong `builtin_defaults()`
**File:** `src/app/resolved_keymap.rs`

Trong hàm `builtin_defaults()`, phần visual mode (tìm `// ── Visual ──`), thêm dòng:
```rust
km.insert(Some("visual"), ch('*'), SEARCH_WORD_UNDER_CURSOR);
```

Điều này đảm bảo `*` trong visual mode luôn trigger search kể cả khi TOML không load được. Pattern này đã được dùng cho các phím `$`, `^`, `G`, `{`, `}` trong visual mode.

### Bước 6: Build & test
```bash
rtk cargo build 2>&1 | grep -E "error|warning.*keymap|warning.*star"
rtk cargo test --lib resolved_keymap
```

Chạy editor, bật log level `info`:
```bash
RUST_LOG=netherize=info cargo run
```
Bấm `*` trong visual mode, kiểm tra log output để xác nhận flow.

### Bước 7: Cleanup (sau khi xác nhận fix)
- Giữ lại safeguard trong `builtin_defaults()` (permanent fix)
- Hạ logging từ `info!` xuống `trace!` hoặc xóa nếu không cần

## Verification
1. Mở file bất kỳ trong editor
2. Bôi đen một từ (vào Visual Mode bằng `v` + chọn text)
3. Bấm `*`
4. **Expected:** Editor highlight tất cả occurrences của từ được chọn, cursor nhảy tới match tiếp theo, thoát về Normal Mode
5. **Not expected:** Text bị bọc bởi dấu `*`
