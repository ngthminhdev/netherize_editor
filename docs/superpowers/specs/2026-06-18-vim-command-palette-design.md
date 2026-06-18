# Vim-mode Command Palette — Design

**Date:** 2026-06-18
**Scope:** Add a self-contained, single-line modal (Vim) editor to the shared `CommandPalette`, so every palette/prompt overlay gains Normal/Insert/Visual editing on its query field. Built as a reusable base, not wired per-mode.

## Overview

The editor is already fully modal (neovim-style: `EditorMode::Normal` is the buffer default, keymap profile is `nvim-ultimate`). There is **no vim on/off toggle** — the app is always modal. The command palette, however, is currently a flat always-typing input that only recently gained arrow/Home/End/Delete cursor movement (see `2026-06-18-explorer-paste-rename-popup-design.md`).

This design layers a single-line Vim state machine onto the **shared** `CommandPalette` struct. Because all palette modes (FilePicker, command list, paste/rename/create prompts, theme selector, etc.) use the same `CommandPalette` with `query: String`, `cursor_byte`, and `selection_range`, adding the Vim layer there makes it available to **every** palette by construction.

The palette opens in **Insert** mode (preserving today's "type immediately" UX as a pure superset). `Esc` enters Normal. From Normal the user gets core motions and operators; in list-pickers, `j`/`k` navigate the result list.

## Goals

- Single-line Vim editing (Normal / Insert / Visual) on the palette query, shared across all palette modes.
- Open in Insert mode; `Esc` → Normal; never disrupt the current type-and-go behavior.
- Core motions: `h l`, `w b e`, `0 ^ $`.
- Enter Insert: `i a I A`.
- Edits: `x`, `d`/`c`/`y` + motion (`dw cw d$ yw …`), `p`/`P` from an internal register.
- Visual mode: `v`, motion-extends selection, `d`/`c`/`y` on the selection.
- In list-pickers (FilePicker, command list, …): `j`/`k` in Normal/Visual move the result selection (alongside the existing `Ctrl+N/P` + arrows); existing list keys unchanged.
- Mode-aware caret (block in Normal/Visual, bar in Insert) and a `-- NORMAL/INSERT/VISUAL --` status indicator.

## Non-Goals (YAGNI v1)

- Numeric counts (`3w`, `2dd`).
- `f/F/t/T`, text objects (`ciw/diw`), `r`, `.` (repeat).
- System-clipboard integration for `p`/`P` (internal unnamed register only).
- Word-wise selection via `Ctrl+Shift+Left/Right`.
- Multi-line editing (the palette query is single-line; no `j/k` cursor movement, no `dd`).

## Design

### 1. Data Model

#### Palette Vim mode

Add to `src/app/command_palette.rs`:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PaletteVimMode {
    #[default]
    Insert,
    Normal,
    Visual,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PaletteVimOperator {
    Delete,
    Change,
    Yank,
}
```

New fields on `CommandPalette`:

```rust
pub vim_mode: PaletteVimMode,            // reset to Insert on open()
pending_operator: Option<PaletteVimOperator>,
vim_register: String,                    // y/d/c write here; p/P read
```

Existing `cursor_byte: usize` and `selection_range: Option<(usize, usize)>` are reused as-is (Visual selection lives in `selection_range`).

`open()` (and the prompt-overlay open path) resets `vim_mode = Insert`, `pending_operator = None`. The register persists across opens within a session (matches Vim register persistence) but is cleared on workspace switch alongside other palette state.

#### Render model

Extend `CommandPaletteRenderModel`:

```rust
pub vim_mode_label: Option<&'static str>,   // "NORMAL" / "INSERT" / "VISUAL"
pub vim_caret_block: bool,                  // true in Normal/Visual
```

Populated in `CommandPalette::render()` from `vim_mode`. `prompt_cursor_byte` / `prompt_selection_range` (already added) continue to drive caret/selection geometry.

### 2. Input Routing

#### Single command variant

Add exactly one `Command` variant in `src/core/commands.rs`:

```rust
/// A keystroke routed into the palette's single-line Vim state machine.
PaletteVimInput(PaletteVimKey),
```

`PaletteVimKey` is a small enum capturing the keys the state machine cares about (printable char + the named keys `Esc`/`Enter`). It lives in `src/core/commands.rs` next to `Command`:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PaletteVimKey {
    Char(char),
    Esc,
    Enter,
}
```

> Rationale: one carrier variant keeps the `Command` enum, `command_ids`, keymap, and dispatch from exploding into dozens of per-key commands. The Vim semantics live entirely in `CommandPalette`. `PaletteVimInput` is **not** added to `command_ids::ALL_IDS`/`parse()` because it is never bound from TOML — it is synthesized by `focus.rs` from raw key events. (Confirm this does not break `default_keymap_has_no_unknown_commands`, which only validates IDs present in the keymap.)

#### `focus.rs::resolve_palette_focus`

The palette-focus resolver becomes Vim-aware:

- **Insert mode:** behavior unchanged (printable → append query, Backspace → `FilePickerBackspaceQuery`, arrows/Home/End/Delete → existing cursor commands), **except** `Esc` → `Command::PaletteVimInput(PaletteVimKey::Esc)` (enter Normal) instead of closing the palette.
- **Normal / Visual mode:** printable chars and `Esc`/`Enter` → `Command::PaletteVimInput(...)`. Existing list-nav keys (`Ctrl+N/P`, `ArrowUp/Down`) keep their current mappings so muscle memory still works.

> Note: today `Esc` in palette focus maps to `SwitchMode(Escape)` which closes the palette. With Vim, `Esc` in Insert goes to Normal; `Esc` in Normal closes. This indirection is handled inside the state machine (returns `Close`), so `focus.rs` always forwards `Esc` as `PaletteVimInput` while in a Vim-enabled palette.

#### Dispatch / event loop

`Command::PaletteVimInput(key)` is handled in the palette focus branch of `src/app/event_loop/commands_palette.rs`:

```rust
AppShell::handle_palette_vim_input(&mut self, key: PaletteVimKey) -> bool
```

It calls `self.app_state.command_palette.vim_input(key, workspace)` which returns:

```rust
pub enum PaletteVimAction {
    Consumed,   // text/cursor/mode/register changed → redraw
    ListPrev,   // j/k in a list-picker
    ListNext,
    Confirm,    // Enter
    Close,      // Esc in Normal
    Ignore,
}
```

The shell interprets the action by **reusing existing plumbing**:

- `Consumed` → request redraw.
- `ListPrev` / `ListNext` → dispatch `Command::OverlaySelectPrev` / `OverlaySelectNext` (only meaningful when the active palette has a result list; for single-line prompts the state machine returns `Ignore` for `j/k` instead).
- `Confirm` → the same routing the Enter key uses today (`FilePickerConfirmSelection` → `confirm_explorer_prompt` etc.), so paste/rename/create/open all keep working — **including the bug-178 fix** that routes `ExplorerPasteFile` to `confirm_explorer_prompt`.
- `Close` → the `CloseFilePicker` path (which already clears pending paste state, returns focus, etc.).

Whether the active palette is a list-picker (so `j/k` should navigate) vs a single-line prompt is determined from `command_palette_mode()` (the prompt modes — Create/Rename/Paste/LspRename — are single-line; everything else has a list). The state machine is told this via a parameter so it can map `j/k` accordingly.

### 3. State Machine — `CommandPalette::vim_input`

Signature:

```rust
pub fn vim_input(
    &mut self,
    key: PaletteVimKey,
    has_result_list: bool,
    workspace: Option<&WorkspaceModel>,
) -> PaletteVimAction
```

Behavior by mode:

**Insert**
- `Esc` → `vim_mode = Normal`; clamp cursor one char left (Vim semantics); clear `selection_range`. Returns `Consumed`.
- `Enter` → `Confirm`. (Other keys never reach here — `focus.rs` only forwards `Esc`/`Enter` as `PaletteVimInput` in Insert.)

**Normal**
- Motions: `h`,`l`; `w`,`b`,`e`; `0`,`^`,`$`. Move `cursor_byte` (char-boundary safe). If an operator is pending, apply it over the moved range instead (see operators).
- Enter Insert: `i` (at cursor), `a` (after), `I` (first non-blank/start), `A` (end). Sets `vim_mode = Insert`.
- `x` → delete char under cursor into `vim_register`. `refresh_results`.
- Operators `d`,`c`,`y`: set `pending_operator`; the next motion key defines the range. `dw cw d$ yw 0d …`. `dd`/`cc`/`yy` are **out of scope** (single-line). On apply: write range to `vim_register`; `d`/`c` remove it; `c` then enters Insert. `refresh_results` if text changed.
- `p` / `P` → insert `vim_register` after / before cursor. `refresh_results`.
- `v` → `vim_mode = Visual`, start selection anchored at `cursor_byte`.
- `j` / `k` → if `has_result_list`: return `ListNext` / `ListPrev`; else `Ignore`.
- `Esc` → clear `pending_operator`; if none was pending, return `Close`; otherwise `Consumed`.
- `Enter` → `Confirm`.

**Visual**
- Motions extend `selection_range` from the anchor.
- `d` / `c` / `y` → operate on the selection into `vim_register`; `d`/`c` delete it (`c` → Insert), `y` → Normal. `refresh_results` if changed.
- `Esc` → `vim_mode = Normal`, clear selection. `Consumed`.
- `Enter` → `Confirm`.

Word-boundary logic (`w`/`b`/`e` and `dw`/`cw`) reuses the algorithm already in `src/editor_core.rs` (`move_word_forward/backward/word_end`); it is extracted into a small `&str`-based helper so it can run on the palette `query` without a Rope. Any text mutation goes through the existing `refresh_results(workspace)` so filtered lists stay in sync, mirroring `delete_char_forward`.

### 4. Rendering

`src/render/renderer/palette/minimal.rs`:

- Caret: when `model.vim_caret_block`, draw a full-cell block (width ≈ one monospace char) at `prompt_cursor_byte`; otherwise the existing 2px bar. The editable-prompt gating already added for paste is extended to apply to all Vim-enabled palettes.
- Selection highlight: reuse the existing `prompt_selection_range` quad for Visual mode.
- Status indicator: render `model.vim_mode_label` (e.g. right-aligned `-- NORMAL --`) in the palette prompt row. Hidden when `None`.

### 5. Scope Across Palettes

- All palette modes get text-Vim on the query (the state machine is mode-agnostic).
- List-pickers (anything with a result list) additionally map `j/k` → list navigation.
- Single-line prompts (Create/Rename/Paste/LspRename) map `j/k` → `Ignore`.
- `Confirm`/`Close` always defer to the existing per-mode Enter/Esc routing, so no palette's confirm/cancel semantics change.

### 6. Edge Cases

| Situation | Behavior |
|-----------|----------|
| Empty query in Normal | Motions/operators are no-ops; `cursor_byte` stays at 0. |
| `Esc` in Insert with empty query | → Normal, cursor stays at 0. |
| `Esc` in Normal | Closes the palette (existing `CloseFilePicker` path). |
| Operator with no following motion, then `Esc` | Pending operator cleared, palette stays open. |
| `p` with empty register | No-op (`Consumed`, nothing inserted). |
| `j/k` in a single-line prompt | `Ignore` (no list to move). |
| UTF-8 / CJK query | All motions/operators clamp to char boundaries (reuse existing boundary helpers). |
| Workspace switch while palette open | Palette state (incl. `vim_mode`, register) reset with the rest. |

## Files Affected

- `src/app/command_palette.rs` — `PaletteVimMode`, fields, `vim_input` state machine, render-model fields, `open()` reset, unit tests.
- `src/core/commands.rs` — `PaletteVimKey`, `Command::PaletteVimInput`.
- `src/app/input_map/focus.rs` — Vim-aware palette-focus routing (Esc/Normal/Visual keys → `PaletteVimInput`).
- `src/app/event_loop/commands_palette.rs` — `handle_palette_vim_input`, map `PaletteVimAction` onto existing list-nav/confirm/close paths.
- `src/render/renderer/palette/minimal.rs` — mode-aware caret + status indicator.
- (Possibly) `src/editor_core.rs` — extract `&str` word-boundary helper for reuse.

## Testing Considerations

Lesson from `bug-178` (the paste feature shipped with the Enter→confirm route unwired and untested): **test the integration glue, not just the pure logic.**

- Unit tests on `vim_input` (pure over query/cursor/mode/register):
  - motions `h l w b e 0 ^ $` with ASCII and CJK;
  - `i a I A` mode transitions and cursor placement;
  - `x`, `dw`, `cw`, `d$`, `yw` + `p`/`P` round-trips through the register;
  - Visual `v` + `d`/`c`/`y`;
  - `Esc` transitions Insert→Normal→Close.
- Glue tests:
  - `focus.rs` forwards `Esc` as `PaletteVimInput` (not palette-close) when in a Vim palette, and forwards Normal-mode chars correctly;
  - `j/k` returns `ListNext/ListPrev` when `has_result_list`, `Ignore` otherwise;
  - `Confirm` reaches `confirm_explorer_prompt` for the prompt modes (regression guard for bug-178).
- Manual: open command palette → `Esc` shows `-- NORMAL --` with block caret → `dw`, `cw`, `x`, `v`+`d`, `p` behave; `j/k` move the result list; `i` returns to Insert; type-and-go still works on open.

## Future Work

- v2 Vim subset: counts, `f/F/t/T`, `ciw/diw`, `r`, `.` repeat.
- System-clipboard register integration for `p`/`P`.
- Word-wise visual selection.
