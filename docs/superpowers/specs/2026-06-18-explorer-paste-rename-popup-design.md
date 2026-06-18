# Explorer Paste — Rename Popup Design

**Date:** 2026-06-18  
**Scope:** Replace the automatic `file (N).ext` paste behavior in the Explorer tree with an editable popup that lets the user review and change the destination file name before copying.

## Overview

Currently, when the user pastes a file in the Explorer (`ExplorerPasteFile`), the editor silently picks the next available unique name (`file (1).ext`, `file (2).ext`, …) and submits an async copy. This is fast but often not what the user wants: they usually intend to rename the pasted copy immediately, and they typically only want to edit the base name while keeping the extension.

This design introduces a dedicated `ExplorerPasteFile` prompt overlay. It pre-fills the suggested unique name, selects the whole name on open, and supports cursor movement so the user can quickly adjust only the part they care about.

## Goals

- Show a popup when pasting a file so the user can edit the destination name.
- Pre-fill the popup with the next available unique name (`file (N).ext`).
- Select the entire pre-filled name when the popup opens.
- Allow cursor movement with Arrow Left/Right and Home/End (or Ctrl+A/E).
- If a selection exists, pressing an arrow key clears the selection and places the cursor at the corresponding edge of the selection.
- Block paste and keep the popup open if the chosen name already exists in the target folder.
- Cancel paste and return focus to the Explorer when the user presses Esc.

## Non-Goals

- Do not support full Vim normal-mode navigation in the popup; only basic cursor movement.
- Do not add overwrite confirmation; duplicate names are blocked with a toast.
- Do not change the behavior of `ExplorerCopyFile` (copy-to-clipboard).
- Do not change `ExplorerRenameFull` / `ExplorerRenameBase`; the new cursor/selection features are added to the shared palette model but are only wired for `ExplorerPasteFile` initially.

## Design

### 1. Data Model

#### New palette mode

Add a new variant to `CommandPaletteMode` in `src/app/command_palette.rs`:

```rust
/// Explorer prompt to choose the destination name for a pasted file.
ExplorerPasteFile,
```

Update `prompt_prefix()`, `empty_hint()`, and `title()` to return sensible values, e.g.:

- prefix: `"paste> "`
- hint: `"enter destination file name..."`
- title: `"PASTE"`

`refresh_results()` should return an empty list for `ExplorerPasteFile`, like the existing rename/create modes.

#### Pending paste state

Add two fields to `AppShell` in `src/app/event_loop/mod.rs`:

```rust
pending_paste_source_path: Option<PathBuf>,
pending_paste_target_dir: Option<PathBuf>,
```

These are populated when the popup opens and cleared when it is confirmed or cancelled.

#### Reuse existing cursor/selection state

`CommandPalette` already tracks:

```rust
pub cursor_byte: usize,
pub selection_range: Option<(usize, usize)>,
```

These fields are reused for the paste popup. No new state is needed for the text cursor itself.

#### Render model

Extend `CommandPaletteRenderModel` in `src/app/command_palette.rs` with:

```rust
pub prompt_cursor_byte: usize,
pub prompt_selection_range: Option<(usize, usize)>,
```

These are populated from the palette state in `CommandPalette::render()`.

### 2. Input Handling

#### New commands

Add new `Command` variants in `src/core/commands.rs`:

```rust
PaletteMoveCursorLeft,
PaletteMoveCursorRight,
PaletteMoveCursorToStart,
PaletteMoveCursorToEnd,
PaletteDeleteCharForward,
```

Add corresponding command IDs in `src/core/command_ids.rs`:

```rust
pub const PALETTE_MOVE_CURSOR_LEFT: &str = "palette.move_cursor_left";
pub const PALETTE_MOVE_CURSOR_RIGHT: &str = "palette.move_cursor_right";
pub const PALETTE_MOVE_CURSOR_TO_START: &str = "palette.move_cursor_to_start";
pub const PALETTE_MOVE_CURSOR_TO_END: &str = "palette.move_cursor_to_end";
pub const PALETTE_DELETE_CHAR_FORWARD: &str = "palette.delete_char_forward";
```

Wire `parse()` and `ALL_IDS` so the IDs are valid.

#### Keybindings

Bind the new commands in the `"palette"` mode of `src/app/resolved_keymap.rs` and `config/keymaps/default.toml`. Only activate them for `ExplorerPasteFile` (and potentially rename/create modes later). Initial bindings:

| Key | Command |
|-----|---------|
| `ArrowLeft` | `PaletteMoveCursorLeft` |
| `ArrowRight` | `PaletteMoveCursorRight` |
| `Home` / `Ctrl+A` | `PaletteMoveCursorToStart` |
| `End` / `Ctrl+E` | `PaletteMoveCursorToEnd` |
| `Delete` | `PaletteDeleteCharForward` |

`Backspace` continues to be handled by the existing `FilePickerBackspaceQuery` command.

> Note: `Ctrl+A` is currently used by In-file search for toggling case sensitivity. That binding only applies when `palette_mode == Some(CommandPaletteMode::InFileSearch)`, so it does not conflict with `ExplorerPasteFile`.

#### Command palette cursor helpers

Add methods to `CommandPalette`:

```rust
pub fn move_cursor_left(&mut self) -> bool;
pub fn move_cursor_right(&mut self) -> bool;
pub fn move_cursor_to_start(&mut self) -> bool;
pub fn move_cursor_to_end(&mut self) -> bool;
pub fn delete_char_forward(&mut self, workspace: Option<&WorkspaceModel>) -> bool;
```

Rules:

- If a selection exists, any move command clears the selection and places `cursor_byte` at the selection edge corresponding to the direction (left → start, right → end, start → start, end → end).
- `move_cursor_left/right` steps by one UTF-8 char boundary, clamped to `[0, query.len()]`.
- `delete_char_forward` deletes the selection if any; otherwise deletes the character after `cursor_byte`.

Expose corresponding methods on `AppState` in `src/app/app_state/palette.rs`.

#### Dispatch routing

In `src/core/command_dispatch/palette.rs`, route the new cursor commands to the `AppState` palette helpers. In `src/app/event_loop/commands_palette.rs`, handle the new commands when the palette is visible in `PaletteFocus` mode, similar to `FilePickerAppendQuery` / `FilePickerBackspaceQuery`.

### 3. Rendering

#### Minimalist palette renderer

In `src/render/renderer/palette/minimal.rs`, update `render_command_palette_minimalist` so that when the mode is `ExplorerPasteFile` (and later rename/create), it renders:

1. **Selection highlight** — a semi-transparent quad behind the selected text range, using `selection_bg`.
2. **Caret** — a vertical bar (or block) at `prompt_cursor_byte` when no selection is active.

The prompt text must be shaped once and measured so that byte offsets can be mapped to x-positions. Use the same monospace width estimation used elsewhere in the minimalist renderer.

When the popup first opens with the whole name selected, the selection quad covers the entire query and no caret is drawn. Once the user presses an arrow key, the selection clears and the caret appears at the corresponding edge.

### 4. Paste Flow

#### Opening the popup

Modify `Command::ExplorerPasteFile` in `src/app/event_loop/commands_explorer.rs`:

1. If `explorer_clipboard_path` is `None`, toast `"No file copied"` and return.
2. Compute `target_dir` from the selected explorer entry (same logic as today).
3. Compute `suggested_name = next_available_paste_path(&target_dir, &file_name)` and extract just the file name.
4. Set `pending_paste_source_path` and `pending_paste_target_dir`.
5. Call `open_prompt_overlay(CommandPaletteMode::ExplorerPasteFile)`.
6. Set the query to `suggested_name`.
7. Set selection range to `(0, suggested_name.len())`.

`next_available_paste_path` is kept as a helper, but it is now only used to compute the suggestion.

#### Confirming the popup

Add a new branch in `confirm_explorer_prompt()` in `src/app/event_loop/commands_prompts.rs` for `CommandPaletteMode::ExplorerPasteFile`:

1. Read `pending_paste_source_path` and `pending_paste_target_dir`; if either is `None`, close popup and return false.
2. Read the query, trim whitespace, and validate:
   - Empty → toast `"Paste name cannot be empty"`, keep popup.
   - Contains `/` or `\\` → toast `"Invalid file name"`, keep popup.
3. Build `target_path = target_dir.join(name)`.
4. If `target_path.exists()` → toast `"File already exists"`, keep popup.
5. Submit async `WorkerRequestPayload::CopyFile { source_path, target_path }`.
6. Clear pending paste state.
7. Close popup and return focus to Explorer.

#### Cancelling

`Esc` / `CloseFilePicker` in `PaletteFocus` already close the palette and return focus to the Explorer. Ensure that when the mode is `ExplorerPasteFile`, `pending_paste_source_path` and `pending_paste_target_dir` are cleared so a stale paste cannot be confirmed later.

### 5. Edge Cases

| Situation | Behavior |
|-----------|----------|
| No file copied | Toast `"No file copied"`, no popup. |
| Empty target dir (no workspace) | Same fallback as today: use workspace root, or default empty path. |
| Empty query on confirm | Toast `"Paste name cannot be empty"`, keep popup. |
| Query contains path separator | Toast `"Invalid file name"`, keep popup. |
| Target file already exists | Toast `"File already exists"`, keep popup. |
| Worker copy fails | Existing async result handler toasts the error; popup is already closed. |
| Esc pressed | Close popup, clear pending paste state, return focus to Explorer. |

## Files Affected

- `src/app/command_palette.rs` — new mode, render model fields, cursor helpers.
- `src/app/app_state/palette.rs` — expose cursor helpers on `AppState`.
- `src/core/commands.rs` — new `Command` variants.
- `src/core/command_ids.rs` — new command IDs.
- `src/core/command_dispatch/palette.rs` — dispatch new commands.
- `src/app/resolved_keymap.rs` — palette-mode keybindings.
- `config/keymaps/default.toml` — matching TOML keybindings.
- `src/app/input_map/focus.rs` — ensure palette focus passes the new keys through.
- `src/app/event_loop/commands_palette.rs` — handle new commands in palette focus.
- `src/app/event_loop/commands_explorer.rs` — open popup instead of submitting copy.
- `src/app/event_loop/commands_prompts.rs` — confirm/cancel paste popup.
- `src/app/event_loop/mod.rs` — add pending paste state fields.
- `src/render/renderer/palette/minimal.rs` — render selection + caret.

## Testing Considerations

- Unit test `CommandPalette` cursor helpers:
  - move left/right with and without selection;
  - move to start/end with and without selection;
  - delete forward with and without selection.
- Unit test `next_available_paste_path` still produces expected unique names.
- Integration test (if feasible): dispatch `ExplorerPasteFile` with a clipboard path, confirm popup, assert async `CopyFile` request is submitted with the edited name.
- Manual verification:
  - Paste in same folder shows `file (1).ext` selected.
  - Arrow right clears selection and places caret after `(1)`.
  - Arrow left clears selection and places caret at start.
  - Confirming an existing name shows toast and keeps popup.
  - Esc cancels and returns focus to Explorer.

## Future Work

- Apply the same cursor/selection rendering to `ExplorerRenameFull`, `ExplorerRenameBase`, `ExplorerCreateFile`, and `ExplorerCreateFolder` for consistency.
- Add `Ctrl+Shift+Left/Right` to select by word inside the popup.
- Add overwrite confirmation as an optional behavior.
