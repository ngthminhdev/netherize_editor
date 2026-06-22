# Explorer half-page scroll + dimmed hidden filenames — design

**Date:** 2026-06-22
**Status:** Approved design, pending implementation plan

## Goal

Two small, independent improvements to the file explorer (left sidebar tree):

1. **`Ctrl-d` / `Ctrl-u` half-page navigation** — vim-style half-page jump of the
   explorer cursor (down / up), with the view auto-following as it already does.
2. **Dim hidden/ignored filenames** — when hidden or git-ignored files are shown
   (via `H` / `I` toggles), render their *filename label* in the dimmest UI
   foreground (`fg_ghost`), giving the "faded / unused-variable" look. Their icon
   already renders in the `warning` color today; this extends the same visual cue
   to the label text.

These are two separate changes that share one file (`build_sidebar_rows`) but are
otherwise orthogonal.

## Background — how the explorer works today

- The explorer tree is a flat list of `ExplorerEntry` rows in
  `event_loop::mod` (`explorer_snapshot.entries`), with `explorer_cursor: usize`
  as the selected index.
- **Scrolling is cursor-driven.** Each frame, `sync_explorer_scroll_to_selected`
  calls `workspace_scroll_to_selected_node(viewport_height, line_height)`, which
  nudges the pixel scroll offset so the selected node stays in view. Moving the
  cursor is therefore sufficient — the view follows automatically.
- `build_sidebar_rows` (`event_loop/helpers.rs`) maps `ExplorerEntry` →
  `SidebarRow` (the renderer's DTO in `render/renderer.rs`). It already computes
  `is_hidden_or_ignored = entry.is_hidden || entry.is_ignored` and uses it to pick
  the **icon** color (`theme.ui.warning` vs the per-filetype color).
- The renderer `update_sidebar_content` (`render/renderer/ui/sidebar.rs`) decides
  the **label** color:
  `label_base_color = row.git_color.unwrap_or(fg_dim)`, with the selected+focused
  row overridden to `git_color.unwrap_or(accent)`.
- Explorer keys live in a dedicated `"explorer"` keymap mode in
  `resolved_keymap.rs`. `Ctrl+d` is currently **unbound** there (plain `d` =
  delete-node). `Ctrl+u` is also unbound.
- Command plumbing for an explorer action touches four spots, mirrored from the
  existing `ExplorerMoveToBottom`:
  1. `core/command_ids.rs` — the `&str` const + the id→`Command` match arm.
  2. `core/commands.rs` — the `Command` enum variant.
  3. `core/command_dispatch/mod.rs` — add the variant to the passthrough match arm
     (alongside `ExplorerMoveToTop`/`ExplorerMoveToBottom`).
  4. `app/event_loop/commands_explorer.rs` — the real handler inside
     `handle_explorer_and_workspace_command`.
  5. `app/resolved_keymap.rs` — the default key binding in `"explorer"` mode.

## Part 1 — `Ctrl-d` / `Ctrl-u` half-page scroll

### Behavior

- `Ctrl-d`: move the explorer cursor **down** by `page_rows / 2`.
- `Ctrl-u`: move the explorer cursor **up** by `page_rows / 2`.
- Cursor is clamped to `[0, entries.len()-1]`. At the ends, jumping clamps to the
  first/last row (no wrap). The view follows via the existing auto-scroll.
- Each jump calls `workspace_select_path` on the new row and marks the sidebar
  dirty, exactly like `ExplorerMoveUp/Down`.

### Page size

The visible row count is derived at command time from the last known sidebar
bounds — the event loop already stores `last_sidebar_bounds: Option<[f32;4]>`:

```
page_rows = floor(sidebar_tree_viewport_height(bounds) / sidebar_line_height)
step      = max(1, page_rows / 2)
```

If `last_sidebar_bounds` is `None` (sidebar never laid out), fall back to a
sensible constant step (e.g. `step = 10`) so the keys still do something.

`sidebar_tree_viewport_height` and `sidebar_line_height` are both already
available on the event-loop shell.

### Wiring

- `command_ids.rs`: add `EXPLORER_HALF_PAGE_DOWN = "explorer.half_page_down"` and
  `EXPLORER_HALF_PAGE_UP = "explorer.half_page_up"`, plus their id→`Command` arms.
- `commands.rs`: add `Command::ExplorerHalfPageDown` and
  `Command::ExplorerHalfPageUp`.
- `command_dispatch/mod.rs`: add both variants to the existing explorer passthrough
  match arm.
- `commands_explorer.rs`: handle both in `handle_explorer_and_workspace_command`,
  using the page-size logic above. Factor the shared move-by-N logic into a small
  helper (e.g. `move_explorer_cursor_by(delta_rows, down)`).
- `resolved_keymap.rs`: in `"explorer"` mode,
  `KeySpec::CtrlPlus(KeyCode::KeyD) -> EXPLORER_HALF_PAGE_DOWN` and
  `KeySpec::CtrlPlus(KeyCode::KeyU) -> EXPLORER_HALF_PAGE_UP`.

## Part 2 — dim hidden/ignored filenames

### Behavior

Hidden **or** ignored files (matching the existing icon treatment) render their
filename label in `theme.ui.fg_ghost` (the dimmest UI foreground) instead of the
normal `fg_dim`.

**Color precedence (unchanged except for the new dim tier):**
1. Selected **and** sidebar-focused → `git_color.unwrap_or(accent)` (readability
   on the selection highlight wins — dim does not apply here).
2. `git_color` if present (a modified hidden file keeps its git color).
3. Hidden/ignored → `fg_ghost`.
4. Otherwise → `fg_dim`.

So the dim only changes the *base* (non-git, non-active-selection) label color.

### Wiring

- `render/renderer.rs`: add `pub is_dim: bool` to `SidebarRow`.
- `event_loop/helpers.rs::build_sidebar_rows`: set `is_dim: is_hidden_or_ignored`
  on the real-row branch and `is_dim: false` on the empty-state placeholder row.
  (`SidebarRow` is only constructed in these two spots.)
- `render/renderer/ui/sidebar.rs::update_sidebar_content`: compute the base label
  color as `let base = if row.is_dim { fg_ghost } else { fg_dim };` and replace the
  current `fg_dim` fallback in `label_base_color` with `base`. The selected+focused
  `accent` path is unchanged.

## Testing

- **Part 1 (unit):** the existing `workspace::model` tests cover scroll-follows-
  selection. Add a focused test for the cursor-step math if a pure helper is
  extracted (clamping at both ends, half-page rounding, empty list → no-op).
  GUI verification: open a deep tree, `Ctrl-d`/`Ctrl-u` jump ~half a screen and the
  selection stays visible.
- **Part 2 (manual/GUI):** toggle `H` (hidden) / `I` (ignored); dotfiles and
  ignored files show faded labels; a git-modified hidden file still shows its git
  color; selecting a hidden file while focused shows accent.
- Run `cargo test` (full suite is green at 942 tests today) and `cargo build`.

## Out of scope (YAGNI)

- No new theme color key — reuse the existing `fg_ghost`.
- No configurable half-page fraction.
- No change to mouse-wheel scrolling or to the outline panel.
- No change to which files count as hidden/ignored.
