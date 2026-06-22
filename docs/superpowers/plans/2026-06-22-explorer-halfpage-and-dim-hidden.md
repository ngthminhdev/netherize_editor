# Explorer half-page scroll + dimmed hidden filenames — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add `Ctrl-d`/`Ctrl-u` half-page cursor navigation to the file explorer, and render hidden/ignored filenames in the dim `fg_ghost` color.

**Architecture:** Explorer scrolling is cursor-driven (view auto-follows the selected node), so half-page = move the cursor by `page_rows/2`, derived from the stored sidebar bounds. The dim is a new `is_dim` flag on `SidebarRow`, set from the already-computed `is_hidden_or_ignored`, consumed by the sidebar renderer.

**Tech Stack:** Rust, winit keymap, wgpu sidebar renderer.

## Global Constraints

- Never commit on the agent's own initiative — committing is the human's job. (Steps below stage nothing and run no `git commit`.)
- Reuse existing theme color `fg_ghost`; no new theme key.
- Follow the exact 5-spot explorer-command wiring pattern used by `ExplorerMoveToBottom`.

---

### Task 1: Dim hidden/ignored filenames

**Files:**
- Modify: `src/render/renderer.rs` (struct `SidebarRow`, ~line 45)
- Modify: `src/app/event_loop/helpers.rs` (`build_sidebar_rows`, ~lines 1727 and 1769)
- Modify: `src/render/renderer/ui/sidebar.rs` (`update_sidebar_content`, ~line 154)

**Interfaces:**
- Produces: `SidebarRow.is_dim: bool` — true for hidden/ignored rows.

- [ ] **Step 1: Add `is_dim` to `SidebarRow`.** In `src/render/renderer.rs`, add to the struct after `is_selected`:

```rust
    pub is_selected: bool,
    /// Hidden/ignored entry — label rendered in the dim `fg_ghost` color.
    pub is_dim: bool,
```

- [ ] **Step 2: Set `is_dim` in `build_sidebar_rows`.** In `src/app/event_loop/helpers.rs`, the real-row `SidebarRow { ... }` (ends with `is_selected: idx == selected,`): add `is_dim: is_hidden_or_ignored,`. The empty-state placeholder row (`is_selected: false,`): add `is_dim: false,`.

- [ ] **Step 3: Consume `is_dim` in the renderer.** In `src/render/renderer/ui/sidebar.rs`, replace:

```rust
            let label_base_color = row.git_color.unwrap_or(fg_dim);
```

with:

```rust
            let base_label_color = if row.is_dim { fg_ghost } else { fg_dim };
            let label_base_color = row.git_color.unwrap_or(base_label_color);
```

(`fg_ghost` is already bound at the top of this function. The selected+focused `accent` path is unchanged.)

- [ ] **Step 4: Build.**

Run: `cargo build`
Expected: compiles clean (no missing-field errors for `SidebarRow`).

- [ ] **Step 5 (no commit).** Leave changes staged-free for the human to review/commit.

---

### Task 2: `Ctrl-d` / `Ctrl-u` half-page navigation

**Files:**
- Modify: `src/core/commands.rs` (Command enum, ~line 341)
- Modify: `src/core/command_ids.rs` (consts ~line 257; id→Command arm ~line 737)
- Modify: `src/core/command_dispatch/mod.rs` (passthrough arm, ~line 415)
- Modify: `src/app/resolved_keymap.rs` (explorer mode bindings, ~line 1104)
- Modify: `src/app/event_loop/commands_explorer.rs` (`handle_explorer_and_workspace_command`, + helper)

**Interfaces:**
- Consumes: `self.last_sidebar_bounds: Option<[f32;4]>`, `self.sidebar_tree_viewport_height(bounds) -> f32`, `self.theme.ui.sidebar_line_height`, `self.explorer_cursor`, `self.explorer_snapshot.entries`.
- Produces: `Command::ExplorerHalfPageDown`, `Command::ExplorerHalfPageUp`; command ids `explorer.half_page_down` / `explorer.half_page_up`.

- [ ] **Step 1: Command enum variants.** In `src/core/commands.rs` after `ExplorerMoveToBottom,`:

```rust
    ExplorerMoveToBottom,
    ExplorerHalfPageDown,
    ExplorerHalfPageUp,
```

- [ ] **Step 2: Command id consts.** In `src/core/command_ids.rs` after `EXPLORER_MOVE_TO_BOTTOM`:

```rust
pub const EXPLORER_HALF_PAGE_DOWN: &str = "explorer.half_page_down";
pub const EXPLORER_HALF_PAGE_UP: &str = "explorer.half_page_up";
```

- [ ] **Step 3: id→Command arm.** In `src/core/command_ids.rs` after the `EXPLORER_MOVE_TO_BOTTOM => ...` arm:

```rust
        EXPLORER_HALF_PAGE_DOWN => Some(Command::ExplorerHalfPageDown),
        EXPLORER_HALF_PAGE_UP => Some(Command::ExplorerHalfPageUp),
```

- [ ] **Step 4: Dispatch passthrough.** In `src/core/command_dispatch/mod.rs`, add to the explorer block of the big match arm (after `| Command::ExplorerMoveToBottom`):

```rust
        | Command::ExplorerMoveToBottom
        | Command::ExplorerHalfPageDown
        | Command::ExplorerHalfPageUp
```

- [ ] **Step 5: Keymap.** In `src/app/resolved_keymap.rs`, in the `"explorer"` mode block (near the `EXPLORER_MOVE_TO_BOTTOM` binding), add:

```rust
    km.insert(
        Some("explorer"),
        KeySpec::CtrlPlus(KeyCode::KeyD),
        EXPLORER_HALF_PAGE_DOWN,
    );
    km.insert(
        Some("explorer"),
        KeySpec::CtrlPlus(KeyCode::KeyU),
        EXPLORER_HALF_PAGE_UP,
    );
```

Ensure `EXPLORER_HALF_PAGE_DOWN`/`UP` are imported in the command_ids `use` group at the top of the file (add them alongside the other `EXPLORER_*` imports).

- [ ] **Step 6: Handler + helper.** In `src/app/event_loop/commands_explorer.rs`, add a private helper on the same `impl` and two match arms in `handle_explorer_and_workspace_command`.

Helper (place near `explorer_selected_entry`):

```rust
    /// Number of fully-visible explorer rows in the current sidebar viewport.
    /// Falls back to a constant when the sidebar has not been laid out yet.
    fn explorer_page_rows(&self) -> usize {
        const FALLBACK_PAGE_ROWS: usize = 20;
        let Some(bounds) = self.last_sidebar_bounds else {
            return FALLBACK_PAGE_ROWS;
        };
        let line_height = self.theme.ui.sidebar_line_height.max(1.0);
        let viewport = self.sidebar_tree_viewport_height(bounds);
        ((viewport / line_height).floor() as usize).max(1)
    }

    /// Move the explorer cursor by `delta` rows (down if `down`, else up),
    /// clamping to the entry list. Returns whether the cursor moved.
    fn move_explorer_cursor_by(&mut self, delta: usize, down: bool) -> bool {
        self.ensure_explorer_snapshot();
        let entries_len = self.explorer_snapshot.entries.len();
        if entries_len == 0 {
            self.explorer_cursor = 0;
            return false;
        }
        let last = entries_len - 1;
        let current = self.explorer_cursor.min(last);
        let target = if down {
            (current + delta).min(last)
        } else {
            current.saturating_sub(delta)
        };
        if target == current {
            return false;
        }
        self.explorer_cursor = target;
        let _ = self
            .app_state
            .workspace_select_path(&self.explorer_snapshot.entries[target].path);
        self.sidebar_needs_layout = true;
        true
    }
```

Match arms (add alongside `ExplorerMoveToBottom` inside `handle_explorer_and_workspace_command`):

```rust
            Command::ExplorerHalfPageDown => {
                let step = (self.explorer_page_rows() / 2).max(1);
                Some(self.move_explorer_cursor_by(step, true))
            }
            Command::ExplorerHalfPageUp => {
                let step = (self.explorer_page_rows() / 2).max(1);
                Some(self.move_explorer_cursor_by(step, false))
            }
```

- [ ] **Step 7: Build.**

Run: `cargo build`
Expected: compiles clean.

- [ ] **Step 8: Test suite.**

Run: `cargo test`
Expected: all tests pass (942 green baseline).

- [ ] **Step 9 (no commit).** Leave changes for the human to review/commit. Report what was done and suggest GUI verification: focus the explorer, `Ctrl-d`/`Ctrl-u` jump ~half a screen; toggle `H`/`I` and confirm hidden/ignored labels are faded.

---

## Self-Review

- **Spec coverage:** Part 1 (Ctrl-d/u) → Task 2; Part 2 (dim) → Task 1. Both decisions (hidden+ignored, half-page) encoded.
- **Placeholders:** none — all code shown.
- **Type consistency:** `is_dim: bool` defined in Task 1 Step 1 and used Steps 2–3; `explorer_page_rows`/`move_explorer_cursor_by` defined and used within Task 2 Step 6. Command/id names consistent across steps.
