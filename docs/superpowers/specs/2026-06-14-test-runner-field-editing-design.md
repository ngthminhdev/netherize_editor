# Test Runner — Edit JSON fields via the real vim editor

**Date:** 2026-06-14
**Status:** Approved (design), pending implementation plan
**Branch:** feature/leetcode

## Problem

The Test Runner panel (right dock) gained a JSON-input UI for authoring test
cases (`input` / `expected`). Two defects appeared:

1. **Overlap not clearing** — after pressing `i`/Enter to edit a field, the
   edit overlay renders on top of the still-visible list/results UI.
2. **No real vim** — editing uses a bespoke mini-editor (`TestRunnerInputChar`,
   `TestRunnerCursor*`, custom undo/redo in `runner/mod.rs`), not the editor's
   real vim engine. The user wants real vim editing.

### Root cause

The bespoke in-panel editor is a parallel editing system that does not
integrate with the editor's buffer / focus / render model. It is the source of
both the overlap and the missing vim behaviour. The fix is architectural:
remove it and route field editing through the real editor (Approach A, chosen).

## Approach (A): field opens in a real scratch buffer

Pressing `i`/Enter on a focused field opens that field's JSON text in a
transient **scratch Text buffer** (`BufferContent::Text`, synthetic path e.g.
`test-case-1-input.json`, `language_id = "json"`). The user edits with the full
real vim engine. `:w` validates the JSON and writes it back into the
`TestCase`; `:q` cancels. Then focus returns to the Test Runner panel.

The app already has a multi-buffer system (`AppState.buffers: Vec<BufferEntry>`,
`active_buffer_index`, scratch buffers via `:enew`, switching in
`app_state/buffers.rs`). We reuse it — no new render surface, no second editor.

## State changes

### `AppState` (`src/app/app_state/mod.rs`)
Add:
```rust
test_field_edit: Option<TestFieldEditSession>,
// struct TestFieldEditSession {
//     case_index: usize,
//     field: TestField,
//     return_buffer_index: Option<usize>, // buffer to restore on commit/cancel
//     scratch_path: PathBuf,              // synthetic path identifying the scratch buffer
// }
```

The `:w` / `:q` interception fires only when `test_field_edit.is_some()` **and**
the active buffer's path equals `scratch_path` — so saving/closing any other
buffer behaves normally even while a session is technically open.

### `TestRunnerState` (`src/runner/mod.rs`)
Remove (no longer needed): `editing`, `cursor`, `undo_stack`, `redo_stack`, and
every insert / cursor-motion / backspace / newline / undo / redo method and the
`TestRunnerEditSnapshot` machinery.
Keep: `cases`, results, `selected`, `focused_field`, `scroll_offset`,
`validate_cases_json`, `validate_json_text`, `open_field`, `add_case`,
`remove_case`, selection movement, `toggle_field`.

## Flow (Golden Data Flow honored)

1. **Input** (`src/app/input/handler.rs::route_test_runner_input`):
   - `i` / `Enter` / pointer `OpenField` → `Command::TestRunnerEditField`.
   - **Delete** routing for `TestRunnerInputChar`, `TestRunnerInputText`,
     `TestRunnerCursor{Left,Right,Up,Down,Home,End}`, `TestRunnerBackspace`,
     `TestRunnerDeleteForward`, `TestRunnerNewline`, `TestRunnerPaste`,
     `TestRunnerUndo`, `TestRunnerRedo`, `TestRunnerBeginEdit`,
     `TestRunnerEndEdit`, and the IME `test_runner_editing` branch
     (`handler.rs:1582`).
2. **Command** (`src/core/command_ids.rs`, `src/core/commands.rs`): replace the
   removed edit commands with `TestRunnerEditField`, `TestFieldEditCommit`,
   `TestFieldEditCancel`.
3. **Dispatch** (`src/core/command_dispatch/` + `commands_terminal.rs`):
   - `TestRunnerEditField`: if a case is selected and not running — create a
     scratch Text buffer seeded with `cases[idx].{input|expected}`, record
     `test_field_edit = Some(session)` (capturing the current
     `active_buffer_index` as `return_buffer_index`), switch the active buffer
     to the scratch, move focus to the editor.
4. **Edit:** real vim. No custom code.
5. **Commit** — `:w` (`Command::SaveFile`) is intercepted in the SaveFile
   handler (`core/command_dispatch/session.rs`): when `test_field_edit.is_some()`
   and the active buffer is the scratch field buffer, read the scratch text →
   `validate_json_text`. On `Ok`: write into `cases[idx].{input|expected}`,
   reset that case's result, close the scratch buffer, restore
   `return_buffer_index`, focus the Test Runner panel, clear `test_field_edit`.
   On `Err`: keep the buffer open, surface the message in the status bar (no
   write).
6. **Cancel** — `:q` / close-buffer is intercepted similarly: discard the
   scratch, restore the previous buffer, clear `test_field_edit`, focus the
   panel.

## Rendering (`src/render/renderer/ui/test_runner.rs`)

- **Delete** the entire `if state.editing { ... }` branch in
  `build_test_runner_content` (lines ~242–314). This removes the overlap at the
  source — the panel always renders the list/results view.
- Remove the `EDIT`/`NAV` mode chip's `EDIT` state (always `NAV`), or relabel.
- Optional: while `test_field_edit` targets a case, tint that card and show an
  inline `editing in buffer…` hint. (Nice-to-have; not required for the fix.)

## Error handling

- Invalid JSON on `:w`: status-bar error, buffer stays open, no write.
- `TestRunnerEditField` when no case selected or `is_running`: no-op.
- Closing the scratch via any path (`:q`, tab close, buffer switch) must clear
  `test_field_edit` so a stale session can't leak.

## Testing (TDD)

Unit (`runner` + `app_state` + dispatch):
- `TestRunnerEditField` seeds a scratch buffer with the exact field text and
  sets the session.
- `:w` with valid JSON writes back to `cases[idx]`, resets the result, closes
  the scratch, restores the prior buffer, clears the session.
- `:w` with invalid JSON keeps the session + buffer open and reports an error;
  `cases[idx]` is unchanged.
- `:q` discards edits and clears the session.

Routing (`src/app/input/tests.rs`):
- `i` / `Enter` in `InputFocusContext::TestRunner` (nav, not editing) →
  `TestRunnerEditField`.
- The removed edit commands no longer have routes.

## Out of scope

- Editing all cases as one document (Approach C).
- An embedded editor view inside the panel (Approach B).
- The `Invalid actual output JSON: EOF…` runtime error in the screenshot is a
  separate runner/program-output concern, not part of this UI work.
