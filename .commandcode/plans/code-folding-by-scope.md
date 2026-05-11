# Code Folding by Scope — Implementation Plan

## Tổng quan

Thêm code folding theo scope dùng AST tree-sitter, với 2 lệnh chính:
- `za` → `editor.toggle_fold`: Toggle fold/unfold scope tại cursor line
- `zA` → `editor.toggle_fold_all`: Toggle fold/unfold tất cả scope trong file

## Data Model

### Fold Range
```
folded_ranges: Vec<(usize, usize)>  // (start_line, end_line) logical line indices
                                    // start inclusive, end exclusive, sorted
```
Mỗi folded range đóng góp 1 visible line (marker line ▶).

### Visible Line Map
Map visible_line_index → logical_line_index, tính toán lazily khi fold thay đổi.

Ví dụ: file 10 dòng, fold line 2-4:
```
visible 0 → logical 0
visible 1 → logical 1
visible 2 → logical 2  (fold marker: "▶ 3 lines folded")
visible 3 → logical 5
visible 4 → logical 6
...
```

## Implementation Steps

### Step 1: AppState — Fold State
**File**: `src/app/app_state/mod.rs`

Thêm field vào `AppState` struct:
```rust
folded_ranges: Vec<(usize, usize)>,
foldable_ranges_cache: Option<Vec<(usize, usize)>>,
```

Thêm methods:
```rust
pub fn folded_ranges(&self) -> &[(usize, usize)]
pub fn is_line_folded(&self, line_idx: usize) -> bool
pub fn compute_visible_line_map(&self) -> Vec<usize>  // visible→logical
pub fn visible_line_count(&self) -> usize
pub fn logical_to_visible_line(&self, logical: usize) -> Option<usize>
pub fn visible_to_logical_line(&self, visible: usize) -> usize
pub fn toggle_fold_at_line(&mut self, logical_line: usize) -> bool
pub fn toggle_fold_all(&mut self) -> bool
pub fn unfold_all(&mut self) -> bool
pub fn set_foldable_ranges_cache(&mut self, ranges: Vec<(usize, usize)>)
```

Logic `toggle_fold_at_line`:
1. Kiểm tra line có đang nằm trong folded range không
2. Nếu có → unfold range đó (remove khỏi `folded_ranges`)
3. Nếu không → tìm foldable range chứa line trong cache, fold nó (insert sorted)

Logic `toggle_fold_all`:
1. Nếu `folded_ranges` đang rỗng → fold tất cả foldable ranges trong cache
2. Nếu có fold → unfold_all

### Step 2: Syntax — Fold Range Provider
**File mới**: `src/syntax/fold.rs`

```rust
use tree_sitter::Node;
use super::syntax_engine::LanguageId;

pub fn compute_foldable_ranges(root_node: Node, language_id: LanguageId) -> Vec<(usize, usize)>
```

Walk AST depth-first, với mỗi node có loại là "foldable scope", lấy `start_position.row` → `end_position.row`. 
Chỉ fold nếu span ≥ 2 dòng. Sắp xếp theo start line, loại bỏ overlap (giữ range ngoài cùng).

Foldable node types per language — dùng HashSet<&str> lookup:
- **Rust**: `block`, `impl_item`, `trait_item`, `struct_item`, `enum_item`, `enum_variant`, `match_arm`, `if_expression`, `while_expression`, `for_expression`, `loop_expression`, `mod_item`, `macro_definition`, `closure_expression`, `function_item`, `use_declaration`
- **JavaScript/TypeScript**: `statement_block`, `class_body`, `object`, `arrow_function`, `function_declaration`, `method_definition`, `switch_body`, `if_statement`, `for_statement`, `while_statement`, `try_statement`
- **Go**: `block`, `if_statement`, `for_statement`, `func_literal`, `struct_type`, `interface_type`
- **Python**: `block`, `function_definition`, `class_definition`, `if_statement`, `for_statement`, `while_statement`, `with_statement`, `try_statement`
- **Java**: `block`, `class_body`, `interface_body`, `enum_body`, `constructor_body`, `if_statement`, `for_statement`, `while_statement`, `switch_block`
- **HTML/XML**: element nodes
- **JSON**: `object`, `array`
- **Markdown**: `section`, `fenced_code_block`
- **CSS**: `block`
- **Bash**: `compound_statement`, `if_statement`, `for_statement`, `while_statement`, `case_statement`, `function_definition`
- **YAML**: `block_mapping`, `block_sequence`
- **SQL**: compound statements
- **Fallback (plaintext)**: không có foldable ranges

Đăng ký module: `src/syntax/mod.rs` → thêm `pub mod fold;`

### Step 3: Commands — New Command Variants
**File**: `src/core/commands.rs`

Thêm vào `Command` enum:
```rust
/// Toggle fold/unfold the scope at the cursor line.
ToggleFold,
/// Toggle fold all / unfold all scopes in the current file.
ToggleFoldAll,
```

Không cần thêm vào `supports_numeric_count()`, `groups_repeated_edits_into_single_transaction()`, `supports_press_and_hold_repeat()`.

### Step 4: Command IDs
**File**: `src/core/command_ids.rs`

Thêm:
```rust
pub const TOGGLE_FOLD: &str = "editor.toggle_fold";
pub const TOGGLE_FOLD_ALL: &str = "editor.toggle_fold_all";
```

Thêm vào `ALL_IDS` array và `parse()` function:
```rust
TOGGLE_FOLD => Some(Command::ToggleFold),
TOGGLE_FOLD_ALL => Some(Command::ToggleFoldAll),
```

### Step 5: Command Dispatch
**File**: `src/core/command_dispatch/mod.rs`

Thêm `ToggleFold` và `ToggleFoldAll` vào mệnh đề session::dispatch:
```rust
Command::ToggleFold | Command::ToggleFoldAll => session::dispatch(&mut ctx, command),
```

**File**: `src/core/command_dispatch/session.rs`

Thêm handling trong `dispatch()`:
```rust
Command::ToggleFold => {
    let (cursor_line, _) = ctx.app_state.cursor_line_col();
    let changed = ctx.app_state.toggle_fold_at_line(cursor_line);
    DispatchReport::success(
        if changed { "Dispatch: toggled fold at cursor" } else { "Dispatch: no foldable scope at cursor" },
        changed,
    )
}
Command::ToggleFoldAll => {
    let changed = ctx.app_state.toggle_fold_all();
    DispatchReport::success(
        if changed { "Dispatch: toggled fold all" } else { "Dispatch: no foldable ranges" },
        changed,
    )
}
```

### Step 6: Populate Foldable Cache
Tìm trong event loop nơi gọi `parse_source()` → `generate_highlight_spans()`. 
Sau khi syntax tree được parse/cập nhật, gọi:
```rust
let foldable = compute_foldable_ranges(tree_state.root_node(), tree_state.language_id());
app_state.set_foldable_ranges_cache(foldable);
```

Vị trí chính xác sẽ được xác định trong quá trình implementation bằng cách trace update_highlights flow.

Trong `AppState::new()`, khởi tạo:
```rust
folded_ranges: Vec::new(),
foldable_ranges_cache: None,
```

### Step 7: Keymaps
**File**: `config/keymaps/default.toml`

Thêm vào NORMAL section:
```toml
[[bindings]]
mode = "normal"
key = "z a"
command = "editor.toggle_fold"

[[bindings]]
mode = "normal"
key = "z A"
command = "editor.toggle_fold_all"
```

### Step 8: Render — Viewport
**File**: `src/render/renderer/editor/viewport.rs`

Trong `update_editor_content()`:
1. Dùng `app_state.visible_line_count()` thay cho `app_state.total_lines()` khi tính gutter digits

**File**: `src/text/layout_sync.rs`

Trong `visual_y_for_logical_scroll()`:
- Nhận thêm `folded_ranges: &[(usize, usize)]` parameter
- Compute visible Y position accounting for folded hidden lines

### Step 9: Gutter
**File**: `src/render/renderer/editor/selections.rs`

Trong `update_editor_gutter()`:
1. Compute `visible_line_map` từ `app_state.compute_visible_line_map()`
2. Khi iterate `layout_runs()`:
   - Skip run nếu `abs_line` nằm trong folded range (không phải line đầu tiên)
   - Với line đầu tiên của mỗi folded range: render fold marker "▶" (dùng ký tự unicode)
   - Với các line khác: render line number tương ứng từ `visible_line_map`

### Step 10: Scroll Adjustment
Khi fold/unfold thay đổi → `bump_revision()` → trigger re-render.
Scroll position cần được điều chỉnh để cursor vẫn visible sau khi fold/unfold.
Trong `toggle_fold_at_line` và `toggle_fold_all`, sau khi thay đổi `folded_ranges`:
- Recompute visible scroll position cho cursor line
- Set `current_scroll_y` để cursor line nằm trong viewport

## Verification

1. **Unit test `compute_visible_line_map()`**: Test với các folded ranges khác nhau
2. **Unit test `compute_foldable_ranges()`**: Test với sample Rust/Python/JS code
3. **Manual test za**: Mở file code, bấm `za` trên function → gọn thành 1 dòng marker, bấm lại → mở ra
4. **Manual test zA**: Bấm `zA` → tất cả scopes fold, bấm lại → unfold hết
5. **Gutter**: Folded region hiện "▶" trong gutter, line numbers điều chỉnh đúng
6. **Scroll**: Scroll qua folded region mượt, cursor không bị lạc
7. **Keymap**: `za`, `zA` hoạt động trong normal mode, không conflict với các binding khác

## Files to Modify/Create

| File | Action |
|------|--------|
| `src/app/app_state/mod.rs` | Thêm fold state + methods (fields, getters, toggle logic) |
| `src/syntax/fold.rs` | **NEW** — `compute_foldable_ranges()` |
| `src/syntax/mod.rs` | Đăng ký `pub mod fold` |
| `src/core/commands.rs` | Thêm `ToggleFold`, `ToggleFoldAll` |
| `src/core/command_ids.rs` | Thêm string IDs + parse + ALL_IDS |
| `src/core/command_dispatch/mod.rs` | Route fold commands → session |
| `src/core/command_dispatch/session.rs` | Handle `ToggleFold`, `ToggleFoldAll` |
| `src/render/renderer/editor/viewport.rs` | Dùng visible_line_count cho gutter digits |
| `src/render/renderer/editor/selections.rs` | Fold markers trong gutter, skip folded LayoutRuns |
| `src/text/layout_sync.rs` | `visual_y_for_logical_scroll` account for folds |
| `config/keymaps/default.toml` | Thêm `za`, `zA` bindings |
| Event loop file (xác định sau) | Populate `foldable_ranges_cache` sau parse |
