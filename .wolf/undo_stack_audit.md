# Undo Stack Integration Audit

**Date:** 2026-05-24  
**Issue:** Code actions (space ca) and potentially other LSP edits don't push to undo stack

## Summary

Reviewed all editor text modification paths. Found **1 critical bug** where code actions cannot be undone.

## How Undo Works

1. **Transaction Creation:** `ensure_current_transaction()` captures before-state
2. **Text Modification:** `apply_insert()` / `apply_delete()` modify text
3. **Transaction Commit:** `commit_transaction()` pushes to undo stack
4. **Auto-commit:** `DispatchCtx::commit_text_transaction()` auto-commits after commands

## Text Modification Methods

### ✅ SAFE (Creates Transactions)

All these methods call `ensure_current_transaction()` before modifying text:

- `apply_insert()` → used by all normal editing commands
- `apply_delete()` → used by all deletion commands
- `accept_inline_suggestion()` → AI inline suggestions
- `accept_inline_suggestion_word()` → partial AI suggestions
- All methods in `editor.rs`: `insert_char`, `insert_tab`, `backspace`, `delete_word`, etc.
- All multi-cursor operations in `multi_cursor.rs`

### ✅ SAFE (Uses _with_undo variant)

- **LSP Rename** (`async_results/lsp.rs:447`): Uses `replace_active_document_text_preserve_cursor_with_undo()`
- **LSP Format Document** (`async_results/lsp.rs:524`): Uses `replace_active_document_text_preserve_cursor_with_undo()`

### ❌ BUG: Code Actions (NO UNDO)

**Location:** `src/app/event_loop/commands_lsp.rs:703`

```rust
pub(crate) fn do_apply_code_action_edits(&mut self, edits: &[...], title: &str) {
    let text = self.app_state.text_string();
    match super::async_results::apply_lsp_text_edits(&text, edits) {
        Ok(next) => {
            if self.app_state.replace_active_document_text_preserve_cursor(&next) {
                // ❌ Uses the NO-UNDO variant!
                // Should use: replace_active_document_text_preserve_cursor_with_undo()
```

**Impact:** When user accepts a code action (space ca → select action → Enter), the changes are applied but cannot be undone with `u`.

### ⚠️ INTENTIONALLY NO UNDO (File Operations)

These operations reset history intentionally:

- `load_buffer_from_file_resetting_view()` - Opening a file (calls `clear_history()`)
- `replace_text_buffer_preserving_view()` - External file changes (no undo needed)
- `open_file()` - New buffer (starts fresh)

## The Fix

Change line 703 in `src/app/event_loop/commands_lsp.rs`:

```diff
- .replace_active_document_text_preserve_cursor(&next)
+ .replace_active_document_text_preserve_cursor_with_undo(&next)
```

## Verification

After fix, test:
1. Open a Rust/Python/JS file with LSP running
2. Trigger code action: `space` `c` `a`
3. Select an action and press Enter
4. Press `u` to undo
5. ✅ Should restore original text

## Related Code

- Transaction system: `src/core/transaction.rs`
- Undo/redo: `src/app/app_state/state.rs:232` (undo), `state.rs:250` (redo)
- Auto-commit: `src/core/command_dispatch/common.rs:105`
- Text replacement methods: `src/app/app_state/state.rs:950` (no undo), `state.rs:977` (with undo)
