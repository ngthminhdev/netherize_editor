# Explorer Paste Rename Popup Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the silent auto-rename paste behavior in the Explorer with an editable popup that pre-fills a unique suggested name, supports cursor movement, and blocks duplicate names.

**Architecture:** Add a new `CommandPaletteMode::ExplorerPasteFile` prompt overlay, reuse the existing `CommandPalette` cursor/selection state, add palette cursor commands with keybindings, render a caret and selection highlight in the minimalist palette renderer, and route confirm/cancel through a dedicated paste handler in `commands_prompts.rs`.

**Tech Stack:** Rust, winit input events, custom GPU renderer, tokio async file worker.

## Global Constraints

- Never run `git commit`, `git push`, `git merge`, or `git tag` without explicit user instruction.
- All blocking file I/O must go through async `WorkerRequest`/`WorkerResult`; never call `std::fs::copy` on the main thread.
- Follow existing code style and patterns in `commands_explorer.rs`, `commands_prompts.rs`, and `command_palette.rs`.
- Keep the change focused: do not refactor unrelated palette code.
- New command IDs must be added to `command_ids::ALL_IDS` and wired through `parse()`.
- Keybindings must be added to both `src/app/resolved_keymap.rs` (built-in fallback) and `config/keymaps/default.toml` (TOML profile).

---

## Task 1: Add New `Command` Variants and Command IDs

**Files:**
- Modify: `src/core/commands.rs`
- Modify: `src/core/command_ids.rs`
- Test: `src/core/command_ids.rs` (existing tests)

**Interfaces:**
- Consumes: nothing (scaffolding).
- Produces:
  - `Command::PaletteMoveCursorLeft`
  - `Command::PaletteMoveCursorRight`
  - `Command::PaletteMoveCursorToStart`
  - `Command::PaletteMoveCursorToEnd`
  - `Command::PaletteDeleteCharForward`
  - `command_ids::PALETTE_MOVE_CURSOR_LEFT`, `PALETTE_MOVE_CURSOR_RIGHT`, `PALETTE_MOVE_CURSOR_TO_START`, `PALETTE_MOVE_CURSOR_TO_END`, `PALETTE_DELETE_CHAR_FORWARD`

- [ ] **Step 1: Add command variants**

In `src/core/commands.rs`, add the new variants in the `// ── File & palette ─────────────────────────────────────────────────────────` section after `FilePickerBackspaceQuery`:

```rust
    FilePickerBackspaceQuery,
    /// Move the command-palette cursor one character left.
    PaletteMoveCursorLeft,
    /// Move the command-palette cursor one character right.
    PaletteMoveCursorRight,
    /// Move the command-palette cursor to the start of the query.
    PaletteMoveCursorToStart,
    /// Move the command-palette cursor to the end of the query.
    PaletteMoveCursorToEnd,
    /// Delete the character after the command-palette cursor.
    PaletteDeleteCharForward,
```

- [ ] **Step 2: Add command ID constants**

In `src/core/command_ids.rs`, add after the `FILE_PICKER_BACKSPACE` constant:

```rust
pub const PALETTE_MOVE_CURSOR_LEFT: &str = "palette.move_cursor_left";
pub const PALETTE_MOVE_CURSOR_RIGHT: &str = "palette.move_cursor_right";
pub const PALETTE_MOVE_CURSOR_TO_START: &str = "palette.move_cursor_to_start";
pub const PALETTE_MOVE_CURSOR_TO_END: &str = "palette.move_cursor_to_end";
pub const PALETTE_DELETE_CHAR_FORWARD: &str = "palette.delete_char_forward";
```

- [ ] **Step 3: Register IDs in `ALL_IDS`**

Add the five new constants to the `ALL_IDS` slice in `src/core/command_ids.rs`.

- [ ] **Step 4: Wire `parse()` arms**

In `src/core/command_ids.rs::parse()`, add arms before the `_ => None` fallback:

```rust
        PALETTE_MOVE_CURSOR_LEFT => Some(Command::PaletteMoveCursorLeft),
        PALETTE_MOVE_CURSOR_RIGHT => Some(Command::PaletteMoveCursorRight),
        PALETTE_MOVE_CURSOR_TO_START => Some(Command::PaletteMoveCursorToStart),
        PALETTE_MOVE_CURSOR_TO_END => Some(Command::PaletteMoveCursorToEnd),
        PALETTE_DELETE_CHAR_FORWARD => Some(Command::PaletteDeleteCharForward),
```

- [ ] **Step 5: Run existing tests**

Run:

```bash
cargo test --lib command_ids
```

Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add src/core/commands.rs src/core/command_ids.rs
git commit -m "feat: add palette cursor movement command variants and ids"
```

---

## Task 2: Add `CommandPaletteMode::ExplorerPasteFile`

**Files:**
- Modify: `src/app/command_palette.rs`
- Test: `cargo test --lib command_palette` (smoke build)

**Interfaces:**
- Consumes: nothing.
- Produces:
  - `CommandPaletteMode::ExplorerPasteFile`
  - Updated `prompt_prefix()`, `empty_hint()`, `title()`, `is_complex_picker()`, and `refresh_results()`.

- [ ] **Step 1: Add the enum variant**

In `src/app/command_palette.rs`, add `ExplorerPasteFile` after `ExplorerRenameBase`:

```rust
    /// Explorer prompt to choose the destination name for a pasted file.
    ExplorerPasteFile,
```

- [ ] **Step 2: Update mode metadata**

In `prompt_prefix()`:

```rust
            Self::ExplorerRenameFull | Self::ExplorerRenameBase => "rename> ",
            Self::ExplorerPasteFile => "paste> ",
```

In `empty_hint()`:

```rust
            Self::ExplorerRenameFull | Self::ExplorerRenameBase => "enter a new file name...",
            Self::ExplorerPasteFile => "enter destination file name...",
```

In `title()`:

```rust
            Self::ExplorerRenameFull | Self::ExplorerRenameBase => "RENAME",
            Self::ExplorerPasteFile => "PASTE",
```

- [ ] **Step 3: Exclude from result list**

In `is_complex_picker()`, no change needed — `ExplorerPasteFile` is not added.

In `refresh_results()`, add `ExplorerPasteFile` to the arm that returns `Vec::new()` for single-line prompts:

```rust
            CommandPaletteMode::ExplorerCreateFile
            | CommandPaletteMode::ExplorerCreateFolder
            | CommandPaletteMode::ExplorerDeleteConfirm
            | CommandPaletteMode::ExplorerRenameFull
            | CommandPaletteMode::ExplorerRenameBase
            | CommandPaletteMode::ExplorerPasteFile
            | CommandPaletteMode::LspRename
            | CommandPaletteMode::BufferCloseConfirm => Vec::new(),
```

- [ ] **Step 4: Build check**

Run:

```bash
cargo check --lib
```

Expected: no errors.

- [ ] **Step 5: Commit**

```bash
git add src/app/command_palette.rs
git commit -m "feat: add ExplorerPasteFile palette mode"
```

---

## Task 3: Add Cursor Helpers to `CommandPalette` and `AppState`

**Files:**
- Modify: `src/app/command_palette.rs`
- Modify: `src/app/app_state/palette.rs`
- Test: add unit tests in `src/app/command_palette.rs`

**Interfaces:**
- Consumes: existing `cursor_byte` and `selection_range` fields.
- Produces:
  - `CommandPalette::move_cursor_left(&mut self) -> bool`
  - `CommandPalette::move_cursor_right(&mut self) -> bool`
  - `CommandPalette::move_cursor_to_start(&mut self) -> bool`
  - `CommandPalette::move_cursor_to_end(&mut self) -> bool`
  - `CommandPalette::delete_char_forward(&mut self, workspace) -> bool`
  - `AppState::command_palette_move_cursor_left(&mut self) -> Result<bool, String>`
  - `AppState::command_palette_move_cursor_right(&mut self) -> Result<bool, String>`
  - `AppState::command_palette_move_cursor_to_start(&mut self) -> Result<bool, String>`
  - `AppState::command_palette_move_cursor_to_end(&mut self) -> Result<bool, String>`
  - `AppState::command_palette_delete_char_forward(&mut self) -> Result<bool, String>`

- [ ] **Step 1: Implement cursor helpers in `CommandPalette`**

Add these methods after `backspace_query` in `src/app/command_palette.rs`:

```rust
    pub fn move_cursor_left(&mut self) -> bool {
        if !self.is_visible {
            return false;
        }
        if let Some((start, _end)) = self.normalized_selection_range() {
            self.cursor_byte = start;
            self.selection_range = None;
            return true;
        }
        if self.cursor_byte == 0 {
            return false;
        }
        let new_byte = self.prev_char_boundary(self.cursor_byte);
        let changed = new_byte != self.cursor_byte;
        self.cursor_byte = new_byte;
        changed
    }

    pub fn move_cursor_right(&mut self) -> bool {
        if !self.is_visible {
            return false;
        }
        if let Some((_start, end)) = self.normalized_selection_range() {
            self.cursor_byte = end;
            self.selection_range = None;
            return true;
        }
        if self.cursor_byte >= self.query.len() {
            return false;
        }
        let new_byte = self.next_char_boundary(self.cursor_byte);
        let changed = new_byte != self.cursor_byte;
        self.cursor_byte = new_byte;
        changed
    }

    pub fn move_cursor_to_start(&mut self) -> bool {
        if !self.is_visible {
            return false;
        }
        if let Some((start, _end)) = self.normalized_selection_range() {
            self.cursor_byte = start;
            self.selection_range = None;
            return true;
        }
        if self.cursor_byte == 0 {
            return false;
        }
        self.cursor_byte = 0;
        true
    }

    pub fn move_cursor_to_end(&mut self) -> bool {
        if !self.is_visible {
            return false;
        }
        if let Some((_start, end)) = self.normalized_selection_range() {
            self.cursor_byte = end;
            self.selection_range = None;
            return true;
        }
        if self.cursor_byte >= self.query.len() {
            return false;
        }
        self.cursor_byte = self.query.len();
        true
    }

    pub fn delete_char_forward(&mut self, workspace: Option<&WorkspaceModel>) -> bool {
        if !self.is_visible || self.query.is_empty() {
            return false;
        }
        if let Some((start, end)) = self.normalized_selection_range() {
            self.query.replace_range(start..end, "");
            self.cursor_byte = start;
            self.selection_range = None;
        } else {
            if self.cursor_byte >= self.query.len() {
                return false;
            }
            let end = self.next_char_boundary(self.cursor_byte);
            self.query.replace_range(self.cursor_byte..end, "");
        }
        self.selected_index = 0;
        self.refresh_results(workspace);
        true
    }

    fn prev_char_boundary(&self, byte: usize) -> usize {
        if byte == 0 {
            return 0;
        }
        let mut i = byte - 1;
        while i > 0 && !self.query.is_char_boundary(i) {
            i -= 1;
        }
        i
    }

    fn next_char_boundary(&self, byte: usize) -> usize {
        if byte >= self.query.len() {
            return self.query.len();
        }
        let mut i = byte + 1;
        while i < self.query.len() && !self.query.is_char_boundary(i) {
            i += 1;
        }
        i
    }
```

- [ ] **Step 2: Expose helpers on `AppState`**

In `src/app/app_state/palette.rs`, add after `command_palette_backspace_query`:

```rust
    pub fn command_palette_move_cursor_left(&mut self) -> Result<bool, String> {
        let changed = self.command_palette.move_cursor_left();
        if changed {
            self.sync_file_picker_cache();
        }
        Ok(changed)
    }

    pub fn command_palette_move_cursor_right(&mut self) -> Result<bool, String> {
        let changed = self.command_palette.move_cursor_right();
        if changed {
            self.sync_file_picker_cache();
        }
        Ok(changed)
    }

    pub fn command_palette_move_cursor_to_start(&mut self) -> Result<bool, String> {
        let changed = self.command_palette.move_cursor_to_start();
        if changed {
            self.sync_file_picker_cache();
        }
        Ok(changed)
    }

    pub fn command_palette_move_cursor_to_end(&mut self) -> Result<bool, String> {
        let changed = self.command_palette.move_cursor_to_end();
        if changed {
            self.sync_file_picker_cache();
        }
        Ok(changed)
    }

    pub fn command_palette_delete_char_forward(&mut self) -> Result<bool, String> {
        let workspace = self.workspace_model.as_ref();
        let changed = self.command_palette.delete_char_forward(workspace);
        if changed {
            self.sync_file_picker_cache();
        }
        Ok(changed)
    }
```

- [ ] **Step 3: Add unit tests**

Append to the `#[cfg(test)] mod tests` block at the bottom of `src/app/command_palette.rs`:

```rust
    #[test]
    fn cursor_moves_left_and_right() {
        let mut p = CommandPalette::default();
        p.open(CommandPaletteMode::ExplorerPasteFile, None);
        p.append_query("file (1).txt", None);
        assert_eq!(p.cursor_byte, "file (1).txt".len());

        p.move_cursor_left();
        assert_eq!(p.cursor_byte, "file (1).tx".len());

        p.move_cursor_right();
        assert_eq!(p.cursor_byte, "file (1).txt".len());
    }

    #[test]
    fn arrow_clears_selection_and_moves_to_edge() {
        let mut p = CommandPalette::default();
        p.open(CommandPaletteMode::ExplorerPasteFile, None);
        p.set_query("file (1).txt", None);
        p.set_selection_range(Some((0, "file (1).txt".len())));

        p.move_cursor_left();
        assert_eq!(p.cursor_byte, 0);
        assert!(p.selection_range.is_none());

        p.set_selection_range(Some((0, "file (1).txt".len())));
        p.move_cursor_right();
        assert_eq!(p.cursor_byte, "file (1).txt".len());
        assert!(p.selection_range.is_none());
    }

    #[test]
    fn delete_forward_removes_selection_or_next_char() {
        let mut p = CommandPalette::default();
        p.open(CommandPaletteMode::ExplorerPasteFile, None);
        p.set_query("file (1).txt", None);
        p.set_selection_range(Some((0, 4)));
        p.delete_char_forward(None);
        assert_eq!(p.query, " (1).txt");
        assert_eq!(p.cursor_byte, 0);

        p.set_query("ab.txt", None);
        p.cursor_byte = 0;
        p.delete_char_forward(None);
        assert_eq!(p.query, "b.txt");
    }
```

- [ ] **Step 4: Run tests**

Run:

```bash
cargo test --lib command_palette
```

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src/app/command_palette.rs src/app/app_state/palette.rs
git commit -m "feat: add palette cursor movement helpers and tests"
```

---

## Task 4: Dispatch New Palette Cursor Commands

**Files:**
- Modify: `src/core/command_dispatch/palette.rs`
- Test: `cargo test --lib` (build check)

**Interfaces:**
- Consumes: `AppState` cursor helpers from Task 3.
- Produces: dispatch arms for the five new `Command` variants.

- [ ] **Step 1: Route cursor commands in `palette::dispatch`**

In `src/core/command_dispatch/palette.rs`, add arms after the `FilePickerBackspaceQuery` arm:

```rust
        Command::PaletteMoveCursorLeft => match ctx.app_state.command_palette_move_cursor_left() {
            Ok(changed) => DispatchReport::success(
                if changed {
                    "Dispatch: palette cursor left".to_string()
                } else {
                    "Dispatch: palette cursor left ignored".to_string()
                },
                changed,
            ),
            Err(err) => DispatchReport::failure(format!(
                "Dispatch: palette cursor left failed -> {err}"
            )),
        },
        Command::PaletteMoveCursorRight => {
            match ctx.app_state.command_palette_move_cursor_right() {
                Ok(changed) => DispatchReport::success(
                    if changed {
                        "Dispatch: palette cursor right".to_string()
                    } else {
                        "Dispatch: palette cursor right ignored".to_string()
                    },
                    changed,
                ),
                Err(err) => DispatchReport::failure(format!(
                    "Dispatch: palette cursor right failed -> {err}"
                )),
            }
        }
        Command::PaletteMoveCursorToStart => {
            match ctx.app_state.command_palette_move_cursor_to_start() {
                Ok(changed) => DispatchReport::success(
                    if changed {
                        "Dispatch: palette cursor to start".to_string()
                    } else {
                        "Dispatch: palette cursor to start ignored".to_string()
                    },
                    changed,
                ),
                Err(err) => DispatchReport::failure(format!(
                    "Dispatch: palette cursor to start failed -> {err}"
                )),
            }
        }
        Command::PaletteMoveCursorToEnd => {
            match ctx.app_state.command_palette_move_cursor_to_end() {
                Ok(changed) => DispatchReport::success(
                    if changed {
                        "Dispatch: palette cursor to end".to_string()
                    } else {
                        "Dispatch: palette cursor to end ignored".to_string()
                    },
                    changed,
                ),
                Err(err) => DispatchReport::failure(format!(
                    "Dispatch: palette cursor to end failed -> {err}"
                )),
            }
        }
        Command::PaletteDeleteCharForward => {
            match ctx.app_state.command_palette_delete_char_forward() {
                Ok(changed) => DispatchReport::success(
                    if changed {
                        "Dispatch: palette delete char forward".to_string()
                    } else {
                        "Dispatch: palette delete char forward ignored".to_string()
                    },
                    changed,
                ),
                Err(err) => DispatchReport::failure(format!(
                    "Dispatch: palette delete char forward failed -> {err}"
                )),
            }
        }
```

- [ ] **Step 2: Build check**

Run:

```bash
cargo check --lib
```

Expected: no errors.

- [ ] **Step 3: Commit**

```bash
git add src/core/command_dispatch/palette.rs
git commit -m "feat: dispatch palette cursor movement commands"
```

---

## Task 5: Add Keybindings

**Files:**
- Modify: `src/app/resolved_keymap.rs`
- Modify: `config/keymaps/default.toml`
- Test: `cargo test --lib` (especially `default_keymap_has_no_unknown_commands`)

**Interfaces:**
- Consumes: command IDs from Task 1.
- Produces: palette-mode bindings for ArrowLeft, ArrowRight, Home, End, Ctrl+A, Ctrl+E, Delete.

- [ ] **Step 1: Add built-in palette bindings**

In `src/app/resolved_keymap.rs::builtin_defaults()`, add after the existing palette bindings:

```rust
    // ── Palette cursor movement ───────────────────────────────────────────────
    km.insert(
        Some("palette"),
        nk(NamedKey::ArrowLeft),
        PALETTE_MOVE_CURSOR_LEFT,
    );
    km.insert(
        Some("palette"),
        nk(NamedKey::ArrowRight),
        PALETTE_MOVE_CURSOR_RIGHT,
    );
    km.insert(
        Some("palette"),
        nk(NamedKey::Home),
        PALETTE_MOVE_CURSOR_TO_START,
    );
    km.insert(
        Some("palette"),
        nk(NamedKey::End),
        PALETTE_MOVE_CURSOR_TO_END,
    );
    km.insert(
        Some("palette"),
        KeySpec::CtrlPlus(KeyCode::KeyA),
        PALETTE_MOVE_CURSOR_TO_START,
    );
    km.insert(
        Some("palette"),
        KeySpec::CtrlPlus(KeyCode::KeyE),
        PALETTE_MOVE_CURSOR_TO_END,
    );
    km.insert(
        Some("palette"),
        nk(NamedKey::Delete),
        PALETTE_DELETE_CHAR_FORWARD,
    );
```

Ensure `command_ids::*` is in scope (`builtin_defaults` already does `use command_ids::*;`).

- [ ] **Step 2: Add TOML bindings**

In `config/keymaps/default.toml`, add in the `# ───────────────── PALETTE ─────────────────` section:

```toml
[[bindings]]
mode = "palette"
key = "ArrowLeft"
command = "palette.move_cursor_left"

[[bindings]]
mode = "palette"
key = "ArrowRight"
command = "palette.move_cursor_right"

[[bindings]]
mode = "palette"
key = "Home"
command = "palette.move_cursor_to_start"

[[bindings]]
mode = "palette"
key = "End"
command = "palette.move_cursor_to_end"

[[bindings]]
mode = "palette"
key = "Ctrl+a"
command = "palette.move_cursor_to_start"

[[bindings]]
mode = "palette"
key = "Ctrl+e"
command = "palette.move_cursor_to_end"

[[bindings]]
mode = "palette"
key = "Delete"
command = "palette.delete_char_forward"
```

- [ ] **Step 3: Verify keymap tests**

Run:

```bash
cargo test --lib default_keymap_has_no_unknown_commands
```

Expected: PASS.

- [ ] **Step 4: Commit**

```bash
git add src/app/resolved_keymap.rs config/keymaps/default.toml
git commit -m "feat: add palette cursor movement keybindings"
```

---

## Task 6: Handle Cursor Commands in the Event Loop

**Files:**
- Modify: `src/app/event_loop/commands_palette.rs`
- Test: `cargo check --lib`

**Interfaces:**
- Consumes: `Command::PaletteMoveCursorLeft/Right/ToStart/ToEnd` and `Command::PaletteDeleteCharForward`.
- Produces: event-loop routing for these commands while in `PaletteFocus`.

- [ ] **Step 1: Extend the palette typing-command matcher**

In `src/app/event_loop/commands_palette.rs`, locate the branch:

```rust
            Command::FilePickerAppendQuery(_)
            | Command::FilePickerBackspaceQuery
            | Command::ToggleLiveGrepCaseSensitive
            | Command::ToggleInFileSearchCaseSensitive
            | Command::EditorPaste
            | Command::PasteSystemClipboard
                if self.app_state.current_mode() == EditorMode::PaletteFocus
                    && self.app_state.is_command_palette_visible() =>
```

Add the new cursor commands to the alternation:

```rust
            Command::FilePickerAppendQuery(_)
            | Command::FilePickerBackspaceQuery
            | Command::PaletteMoveCursorLeft
            | Command::PaletteMoveCursorRight
            | Command::PaletteMoveCursorToStart
            | Command::PaletteMoveCursorToEnd
            | Command::PaletteDeleteCharForward
            | Command::ToggleLiveGrepCaseSensitive
            | Command::ToggleInFileSearchCaseSensitive
            | Command::EditorPaste
            | Command::PasteSystemClipboard
                if self.app_state.current_mode() == EditorMode::PaletteFocus
                    && self.app_state.is_command_palette_visible() =>
```

The existing body dispatches through `dispatch_palette_overlay_command` and requests redraw on success, which is correct for cursor moves.

- [ ] **Step 2: Ensure input focus passes the keys through**

In `src/app/input_map/focus.rs::resolve_palette_focus`, the existing named-key arms for `ArrowLeft`/`ArrowRight`/`Home`/`End`/`Delete` should be removed or guarded so they do not resolve to `OverlaySelectPrev`/`OverlaySelectNext`. Currently there is no such mapping, but verify that no new global intercept swallows these keys.

Specifically, after the existing `NamedKey::Backspace` and `NamedKey::Space` arms, add explicit named-key arms before the fallback to `palette_query_from_text`:

```rust
                NamedKey::ArrowLeft => Some(KeybindingMatch {
                    command: Command::PaletteMoveCursorLeft,
                    reason: "palette focus: ArrowLeft -> move cursor left",
                }),
                NamedKey::ArrowRight => Some(KeybindingMatch {
                    command: Command::PaletteMoveCursorRight,
                    reason: "palette focus: ArrowRight -> move cursor right",
                }),
                NamedKey::Home => Some(KeybindingMatch {
                    command: Command::PaletteMoveCursorToStart,
                    reason: "palette focus: Home -> move cursor to start",
                }),
                NamedKey::End => Some(KeybindingMatch {
                    command: Command::PaletteMoveCursorToEnd,
                    reason: "palette focus: End -> move cursor to end",
                }),
                NamedKey::Delete => Some(KeybindingMatch {
                    command: Command::PaletteDeleteCharForward,
                    reason: "palette focus: Delete -> delete char forward",
                }),
```

This ensures the named keys are routed even if the keymap has no binding.

- [ ] **Step 3: Build check**

Run:

```bash
cargo check --lib
```

Expected: no errors.

- [ ] **Step 4: Commit**

```bash
git add src/app/event_loop/commands_palette.rs src/app/input_map/focus.rs
git commit -m "feat: route palette cursor commands through event loop"
```

---

## Task 7: Add Pending Paste State to `AppShell`

**Files:**
- Modify: `src/app/event_loop/mod.rs`
- Test: `cargo check --lib`

**Interfaces:**
- Consumes: nothing.
- Produces:
  - `AppShell.pending_paste_source_path: Option<PathBuf>`
  - `AppShell.pending_paste_target_dir: Option<PathBuf>`

- [ ] **Step 1: Add fields**

In `src/app/event_loop/mod.rs`, locate the `AppShell` struct definition near `explorer_clipboard_path` and add:

```rust
    pending_paste_source_path: Option<PathBuf>,
    pending_paste_target_dir: Option<PathBuf>,
```

- [ ] **Step 2: Initialize fields**

Find `AppShell::new` / `new_for_tests` in `src/app/event_loop/setup.rs` and initialize:

```rust
            pending_paste_source_path: None,
            pending_paste_target_dir: None,
```

If the struct is constructed with `..Default::default()` or a macro, add the fields to the initialization list instead.

- [ ] **Step 3: Build check**

Run:

```bash
cargo check --lib
```

Expected: no errors.

- [ ] **Step 4: Commit**

```bash
git add src/app/event_loop/mod.rs src/app/event_loop/setup.rs
git commit -m "feat: add pending paste state to AppShell"
```

---

## Task 8: Open the Paste Popup from `ExplorerPasteFile`

**Files:**
- Modify: `src/app/event_loop/commands_explorer.rs`
- Test: `cargo test --lib` (build check)

**Interfaces:**
- Consumes: `next_available_paste_path`, `open_prompt_overlay`, `set_command_palette_query`, `set_command_palette_selection_range`.
- Produces: updated `Command::ExplorerPasteFile` handler that opens the popup.

- [ ] **Step 1: Refactor `ExplorerPasteFile`**

Replace the body of `Command::ExplorerPasteFile` in `src/app/event_loop/commands_explorer.rs` with:

```rust
            Command::ExplorerPasteFile => {
                let Some(source_path) = self.explorer_clipboard_path.clone() else {
                    self.show_transient_toast("No file copied".to_string());
                    return Some(false);
                };

                self.ensure_explorer_snapshot();
                if self.explorer_snapshot.entries.is_empty() {
                    return Some(false);
                }

                let selected_entry = &self.explorer_snapshot.entries[self.explorer_cursor];
                let target_dir = if selected_entry.file_type == WorkspaceNodeType::Folder {
                    selected_entry.path.clone()
                } else {
                    selected_entry
                        .path
                        .parent()
                        .map(PathBuf::from)
                        .unwrap_or_else(|| {
                            self.app_state
                                .workspace_root_path()
                                .map(PathBuf::from)
                                .unwrap_or_default()
                        })
                };

                let file_name = source_path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("file")
                    .to_string();
                let suggested_path = next_available_paste_path(&target_dir, &file_name);
                let suggested_name = suggested_path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or(&file_name)
                    .to_string();

                if !self.open_prompt_overlay(CommandPaletteMode::ExplorerPasteFile) {
                    return Some(false);
                }

                self.pending_paste_source_path = Some(source_path);
                self.pending_paste_target_dir = Some(target_dir);
                let _ = self.app_state.set_command_palette_query(&suggested_name);
                let _ = self
                    .app_state
                    .set_command_palette_selection_range(Some((0, suggested_name.len())));

                Some(true)
            }
```

`next_available_paste_path` remains the helper at the top of the file.

- [ ] **Step 2: Build check**

Run:

```bash
cargo check --lib
```

Expected: no errors.

- [ ] **Step 3: Commit**

```bash
git add src/app/event_loop/commands_explorer.rs
git commit -m "feat: open paste rename popup from ExplorerPasteFile"
```

---

## Task 9: Confirm and Cancel the Paste Popup

**Files:**
- Modify: `src/app/event_loop/commands_prompts.rs`
- Test: `cargo test --lib` (build check)

**Interfaces:**
- Consumes: `pending_paste_source_path`, `pending_paste_target_dir`, `command_palette_query_text`.
- Produces: paste confirmation branch in `confirm_explorer_prompt()` and cleanup on cancel.

- [ ] **Step 1: Add paste confirmation branch**

In `src/app/event_loop/commands_prompts.rs::confirm_explorer_prompt`, add a new arm before the `_ => return false` fallback:

```rust
            CommandPaletteMode::ExplorerPasteFile => {
                let Some(source_path) = self.pending_paste_source_path.take() else {
                    return false;
                };
                let Some(target_dir) = self.pending_paste_target_dir.take() else {
                    return false;
                };

                let new_name = self.app_state.command_palette_query_text().trim();
                if new_name.is_empty() {
                    self.show_transient_toast("Paste name cannot be empty".to_string());
                    // Restore state so the user can retry.
                    self.pending_paste_source_path = Some(source_path);
                    self.pending_paste_target_dir = Some(target_dir);
                    return false;
                }
                if new_name.contains(std::path::MAIN_SEPARATOR)
                    || new_name.contains('/')
                    || new_name.contains('\\')
                {
                    self.show_transient_toast("Invalid file name".to_string());
                    self.pending_paste_source_path = Some(source_path);
                    self.pending_paste_target_dir = Some(target_dir);
                    return false;
                }

                let target_path = target_dir.join(new_name);
                if target_path.exists() {
                    self.show_transient_toast("File already exists".to_string());
                    self.pending_paste_source_path = Some(source_path);
                    self.pending_paste_target_dir = Some(target_dir);
                    return false;
                }

                self.submit(RequestSpec {
                    revision_id: 0,
                    topic: RequestTopic::FileOperation,
                    payload: WorkerRequestPayload::CopyFile {
                        source_path,
                        target_path: target_path.clone(),
                    },
                });
                target_path
            }
```

- [ ] **Step 2: Clear pending state on cancel**

In the `Command::CloseFilePicker` handler in `src/app/event_loop/commands_palette.rs`, after checking `returns_to_explorer`, clear the pending paste state when the mode is `ExplorerPasteFile`:

```rust
                let is_paste_popup = matches!(
                    self.app_state.command_palette_mode(),
                    Some(CommandPaletteMode::ExplorerPasteFile)
                );
                if is_paste_popup {
                    self.pending_paste_source_path = None;
                    self.pending_paste_target_dir = None;
                }
```

Place this before the call to `dispatch_command(&mut self.app_state, command.clone())`.

- [ ] **Step 3: Build check**

Run:

```bash
cargo check --lib
```

Expected: no errors.

- [ ] **Step 4: Commit**

```bash
git add src/app/event_loop/commands_prompts.rs src/app/event_loop/commands_palette.rs
git commit -m "feat: confirm/cancel explorer paste popup"
```

---

## Task 10: Render Selection Highlight and Caret

**Files:**
- Modify: `src/app/command_palette.rs`
- Modify: `src/render/renderer/palette/minimal.rs`
- Test: visual/manual; `cargo check --lib`

**Interfaces:**
- Consumes: `CommandPalette.cursor_byte` and `selection_range`.
- Produces: `CommandPaletteRenderModel.prompt_cursor_byte` and `prompt_selection_range`, plus rendering code in `render_command_palette_minimalist`.

- [ ] **Step 1: Extend the render model**

In `src/app/command_palette.rs::CommandPaletteRenderModel`, add:

```rust
    pub prompt_cursor_byte: usize,
    pub prompt_selection_range: Option<(usize, usize)>,
```

- [ ] **Step 2: Populate the new fields**

In `CommandPalette::render()`, inside the `Some(CommandPaletteRenderModel { ... })` literal, add:

```rust
            prompt_cursor_byte: self.cursor_byte,
            prompt_selection_range: self.normalized_selection_range(),
```

- [ ] **Step 3: Render selection and caret in minimalist palette**

In `src/render/renderer/palette/minimal.rs::render_command_palette_minimalist`, after drawing the query text (around line 331-343), add:

```rust
        // Selection highlight + caret for editable palette modes
        let is_editable_prompt = matches!(
            model.mode,
            crate::app::command_palette::CommandPaletteMode::ExplorerPasteFile
                | crate::app::command_palette::CommandPaletteMode::ExplorerRenameFull
                | crate::app::command_palette::CommandPaletteMode::ExplorerRenameBase
                | crate::app::command_palette::CommandPaletteMode::ExplorerCreateFile
                | crate::app::command_palette::CommandPaletteMode::ExplorerCreateFolder
                | crate::app::command_palette::CommandPaletteMode::LspRename
        );
        if is_editable_prompt {
            let query_display = &model.prompt_query;
            let prefix_x = text_x + prefix_w;

            if let Some((sel_start, sel_end)) = model.prompt_selection_range {
                let before_w = estimate_monospace_width(&query_display[..sel_start], font_size);
                let sel_w = estimate_monospace_width(&query_display[sel_start..sel_end], font_size);
                let mut sel_bg = model.selection_bg;
                sel_bg[3] = sel_bg[3].max(0.55);
                quads.push(RegionDrawInstance::new(
                    [prefix_x + before_w, prompt_y - 2.0, sel_w, line_h + 4.0],
                    sel_bg,
                ));
            } else {
                let before_w =
                    estimate_monospace_width(&query_display[..model.prompt_cursor_byte], font_size);
                let caret_w = 2.0_f32;
                let mut caret_color = model.text_color;
                caret_color[3] = 0.9;
                quads.push(RegionDrawInstance::new(
                    [prefix_x + before_w, prompt_y - 2.0, caret_w, line_h + 4.0],
                    caret_color,
                ));
            }
        }
```

Make sure `estimate_monospace_width` is in scope (it is used elsewhere in the same file via `crate::render::renderer::helpers::estimate_monospace_width`).

- [ ] **Step 4: Build check**

Run:

```bash
cargo check --lib
```

Expected: no errors.

- [ ] **Step 5: Commit**

```bash
git add src/app/command_palette.rs src/render/renderer/palette/minimal.rs
git commit -m "feat: render palette prompt selection and caret"
```

---

## Task 11: Integration Testing and Manual Verification

**Files:**
- Test: existing test suites.
- Manual: run the app.

**Interfaces:**
- Consumes: all previous tasks.
- Produces: passing tests and verified UI behavior.

- [ ] **Step 1: Run unit tests**

```bash
cargo test --lib
```

Expected: PASS.

- [ ] **Step 2: Run keymap validation test**

```bash
cargo test --lib default_keymap_has_no_unknown_commands
```

Expected: PASS.

- [ ] **Step 3: Run clippy**

```bash
cargo clippy --lib -- -D warnings
```

Expected: no warnings (or only pre-existing ones).

- [ ] **Step 4: Manual smoke test**

Run the editor:

```bash
cargo run --release
```

In the Explorer:
1. Copy a file with `d` (or the bound copy key).
2. Paste with `p` (or the bound paste key).
3. Observe a popup with `paste> file (N).ext` and the whole name selected.
4. Press ArrowRight — selection disappears and caret moves to end.
5. Press ArrowLeft — caret moves left.
6. Press Home / Ctrl+A — caret jumps to start.
7. Press End / Ctrl+E — caret jumps to end.
8. Type a new name, press Enter.
9. Verify the file is copied with the chosen name.
10. Try pasting again with the same target name — a toast should say "File already exists" and the popup stays open.
11. Press Esc — popup closes and focus returns to Explorer.

- [ ] **Step 5: Commit final state**

```bash
git add -A
git commit -m "feat: explorer paste rename popup with cursor movement"
```

---

## Spec Coverage Checklist

| Spec Requirement | Implementing Task |
|------------------|-------------------|
| Popup on paste | Task 8 |
| Pre-fill unique suggested name | Task 8 |
| Select all on open | Task 8 |
| Arrow Left/Right cursor movement | Tasks 3, 4, 5, 6 |
| Home/End / Ctrl+A/E jump to edges | Tasks 3, 4, 5, 6 |
| Arrow clears selection and jumps to edge | Task 3 |
| Block duplicate names + toast | Task 9 |
| Esc cancels and returns focus | Task 9 |
| Render selection + caret | Task 10 |

## Placeholder Scan

No TBD, TODO, or vague steps remain. Every step includes exact file paths, code, and verification commands.

## Type Consistency Notes

- `Command` variants added in Task 1 match the `parse()` arms added in Task 1.
- `AppState` helper names match the `CommandPalette` method names and the dispatch arms in Task 4.
- `CommandPaletteRenderModel` field names match the render usage in Task 10.
- `pending_paste_source_path` / `pending_paste_target_dir` are both `Option<PathBuf>` and initialized together.

## Execution Handoff

**Plan complete and saved to `docs/superpowers/plans/2026-06-18-explorer-paste-rename-popup.md`.**

Two execution options:

1. **Subagent-Driven (recommended)** — dispatch a fresh subagent per task, review between tasks, fast iteration.
2. **Inline Execution** — execute tasks in this session using `executing-plans`, batch execution with checkpoints.

Which approach would you like?
