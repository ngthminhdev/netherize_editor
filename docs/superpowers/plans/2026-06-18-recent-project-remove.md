# Recent Project Remove Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add `x` removal for the selected Recent Projects entry, gated to palette normal mode after `Esc` so insert/query typing is unaffected.

**Architecture:** Keep the existing Recent Projects palette and persistence model. Add one command, route it only when `CommandPaletteMode::RecentProjects` and `PaletteVimMode::Normal`, then remove the selected `OpenFile(path)` from `AppPersistentState`, save, and refresh palette/welcome state.

**Tech Stack:** Rust, existing `Command` dispatch pipeline, `CommandPalette`, `AppPersistentState`, `cargo test`.

---

## Files

- Modify `src/core/commands.rs`: add `Command::RemoveRecentProject` near `OpenRecentProjects`.
- Modify `src/core/command_ids.rs`: add a stable id only if using keymap/config routing; direct context routing can avoid this.
- Modify `src/app/input_map/mod.rs`: resolve `x` to remove only in Recent Projects normal palette mode.
- Modify `src/app/input_map/tests.rs`: lock insert-mode `x` as text and normal-mode `x` as removal.
- Modify `src/app/persistence.rs`: add `remove_recent(&mut self, path: &Path) -> bool` and prune metadata.
- Modify `src/app/event_loop/commands_prompts.rs`: add `remove_recent_project_selection` handler.
- Modify `src/app/event_loop/commands_palette.rs` or the relevant dispatcher file: route `Command::RemoveRecentProject` to the new handler if palette commands are handled there.
- Modify `src/core/command_dispatch/mod.rs` and/or `src/core/command_dispatch/session.rs`: classify the new command consistently with other shell-handled palette commands.
- Modify `src/render/renderer/palette/recent_projects.rs`: add `x remove` footer hint for Recent Projects.
- Update `.wolf/cerebrum.md`, `.wolf/memory.md`, and `.wolf/buglog.json` if implementation discovers project learnings or fixes reported broken behavior.

## Task 1: Input Mapping Tests

**Files:**
- Modify: `src/core/commands.rs`
- Modify: `src/app/input_map/tests.rs`

- [ ] **Step 1: Add the command variant needed by tests**

In `src/core/commands.rs`, add this enum variant near `OpenRecentProjects`:

```rust
/// Remove the selected entry from the recent projects list.
RemoveRecentProject,
```

- [ ] **Step 2: Write failing input-map tests**

In `src/app/input_map/tests.rs`, add tests near the existing Recent Projects tests:

```rust
#[test]
fn recent_projects_x_filters_in_palette_insert_mode() {
    let map = make_default_profile_map();
    let input_x = NormalizedInput {
        physical_key: Some(KeyCode::KeyX),
        named_key: None,
        text: Some("x".to_string()),
        modifiers: ModifiersState::empty(),
    };

    let mut context = KeybindingContext::for_mode_with_picker(EditorMode::PaletteFocus, true);
    context.command_palette_mode = Some(CommandPaletteMode::RecentProjects);
    context.palette_vim_mode = Some(crate::app::command_palette::PaletteVimMode::Insert);

    assert_eq!(
        map.translate(&input_x, context),
        Some(Command::FilePickerAppendQuery("x".to_string()))
    );
}

#[test]
fn recent_projects_x_removes_only_in_palette_normal_mode() {
    let map = make_default_profile_map();
    let input_x = NormalizedInput {
        physical_key: Some(KeyCode::KeyX),
        named_key: None,
        text: Some("x".to_string()),
        modifiers: ModifiersState::empty(),
    };

    let mut context = KeybindingContext::for_mode_with_picker(EditorMode::PaletteFocus, true);
    context.command_palette_mode = Some(CommandPaletteMode::RecentProjects);
    context.palette_vim_mode = Some(crate::app::command_palette::PaletteVimMode::Normal);

    assert_eq!(map.translate(&input_x, context), Some(Command::RemoveRecentProject));
}
```

- [ ] **Step 3: Run tests and verify they fail**

Run:

```bash
cargo test recent_projects_x --lib
```

Expected: the normal-mode test fails because `x` is not mapped to `RemoveRecentProject` yet.

## Task 2: Input Mapping Implementation

**Files:**
- Modify: `src/app/input_map/mod.rs`
- Modify: `src/app/input_map/tests.rs`

- [ ] **Step 1: Add direct Recent Projects normal-mode routing**

In `InputMap::translate` or the nearby Recent Projects direct routing block in `src/app/input_map/mod.rs`, add a branch before generic palette text insertion:

```rust
if context.command_palette_mode == Some(CommandPaletteMode::RecentProjects)
    && context.palette_vim_mode == Some(crate::app::command_palette::PaletteVimMode::Normal)
    && input.modifiers.is_empty()
    && input.physical_key == Some(KeyCode::KeyX)
{
    return Some(Command::RemoveRecentProject);
}
```

If the function returns `KeybindingMatch` rather than `Command` at the insertion point, use the existing local return shape:

```rust
return Some(KeybindingMatch {
    command: Command::RemoveRecentProject,
    reason: "recent projects palette normal mode: x -> remove recent project",
});
```

- [ ] **Step 2: Run focused input-map tests**

Run:

```bash
cargo test recent_projects_x --lib
```

Expected: both tests pass.

## Task 3: Persistence Helper

**Files:**
- Modify: `src/app/persistence.rs`

- [ ] **Step 1: Add failing persistence test**

In the existing `#[cfg(test)]` module for `src/app/persistence.rs`, or create one at the bottom if absent, add:

```rust
#[test]
fn remove_recent_drops_path_and_metadata() {
    let project_a = std::path::PathBuf::from("/tmp/netherize-project-a");
    let project_b = std::path::PathBuf::from("/tmp/netherize-project-b");
    let mut state = AppPersistentState::default();

    state.push_recent_with_icon(project_a.clone(), Some("a-icon".to_string()));
    state.push_recent_with_icon(project_b.clone(), Some("b-icon".to_string()));

    assert!(state.remove_recent(&project_a));
    assert_eq!(state.recent_projects, vec![project_b.clone()]);
    assert!(!state.recent_project_meta.contains_key(&project_a));
    assert!(state.recent_project_meta.contains_key(&project_b));
    assert!(!state.remove_recent(&project_a));
}
```

- [ ] **Step 2: Run test and verify it fails**

Run:

```bash
cargo test remove_recent_drops_path_and_metadata --lib
```

Expected: fail because `remove_recent` does not exist.

- [ ] **Step 3: Implement helper**

Add this method to `impl AppPersistentState` in `src/app/persistence.rs`:

```rust
pub fn remove_recent(&mut self, path: &Path) -> bool {
    let before_len = self.recent_projects.len();
    self.recent_projects.retain(|recent| recent != path);
    self.recent_project_meta.remove(path);
    before_len != self.recent_projects.len()
}
```

Use the already imported `Path` type if present; otherwise add `use std::path::Path;` at the top without disturbing existing imports.

- [ ] **Step 4: Run persistence test**

Run:

```bash
cargo test remove_recent_drops_path_and_metadata --lib
```

Expected: pass.

## Task 4: Event-Loop Remove Handler

**Files:**
- Modify: `src/app/event_loop/commands_prompts.rs`
- Modify: `src/app/event_loop/commands_palette.rs`
- Modify: `src/core/command_dispatch/mod.rs`
- Modify: `src/core/command_dispatch/session.rs`

- [ ] **Step 1: Run impact analysis before editing symbols**

Run GitNexus impact before editing each symbol you touch:

```text
gitnexus_impact target=confirm_recent_project_selection direction=upstream repo=netherize_editor
gitnexus_impact target=dispatch_command direction=upstream repo=netherize_editor
```

If either returns HIGH or CRITICAL risk, stop and warn the user before editing.

- [ ] **Step 2: Implement selected-entry removal handler**

Add near `confirm_recent_project_selection` in `src/app/event_loop/commands_prompts.rs`:

```rust
pub(super) fn remove_recent_project_selection(&mut self) -> bool {
    let Some(crate::app::command_palette::CommandPaletteAction::OpenFile(path)) =
        self.app_state.command_palette_selected_action()
    else {
        return false;
    };

    let label = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_else(|| path.to_string_lossy().as_ref())
        .to_string();

    if !self.persistent_state.remove_recent(&path) {
        return false;
    }

    self.persistent_state.save();
    let mut changed = self
        .app_state
        .open_recent_projects_palette_with_meta(
            &self.persistent_state.recent_projects,
            &self.persistent_state.recent_project_meta,
        )
        .is_ok();
    changed |= self
        .app_state
        .sync_welcome_recent_projects(&self.persistent_state.recent_projects);
    self.show_transient_toast(format!("Removed recent project: {label}"));
    changed || self.transient_toast.is_some()
}
```

If the `unwrap_or_else` borrow does not compile due to temporary lifetime, use this equivalent:

```rust
let label = path
    .file_name()
    .and_then(|name| name.to_str())
    .map(str::to_string)
    .unwrap_or_else(|| path.to_string_lossy().into_owned());
```

- [ ] **Step 3: Route the command from palette handling**

In the same command handling area that maps `Command::FilePickerConfirmSelection` to `confirm_recent_project_selection`, add:

```rust
Command::RemoveRecentProject => Some(self.remove_recent_project_selection()),
```

If the file uses a boolean return rather than `Option<bool>`, match the surrounding style exactly:

```rust
Command::RemoveRecentProject => self.remove_recent_project_selection(),
```

- [ ] **Step 4: Mark command as shell-handled if required**

In `src/core/command_dispatch/mod.rs` and `src/core/command_dispatch/session.rs`, add `Command::RemoveRecentProject` beside `Command::OpenRecentProjects` or `Command::FilePickerConfirmSelection` in any pass-through/session classification match arms.

Use this exact pattern where commands are listed with pipes:

```rust
| Command::RemoveRecentProject
```

- [ ] **Step 5: Run compile-focused tests**

Run:

```bash
cargo test recent_projects_x remove_recent_drops_path_and_metadata --lib
```

Expected: pass or compile errors limited to routing signatures. Fix compile errors by matching nearby command patterns.

## Task 5: Footer Hint

**Files:**
- Modify: `src/render/renderer/palette/recent_projects.rs`

- [ ] **Step 1: Update footer actions**

In `render_recent_projects`, change the footer action list for non-theme Recent Projects to include `x remove`. Replace the static action slice with a local branch:

```rust
let recent_project_footer_actions = [
    PaletteFooterAction {
        keys: &["↑↓"],
        label: "navigate",
    },
    PaletteFooterAction {
        keys: &["↵"],
        label: enter_label,
    },
    PaletteFooterAction {
        keys: &["x"],
        label: "remove",
    },
    PaletteFooterAction {
        keys: &["󱊷"],
        label: "close",
    },
];
let theme_footer_actions = [
    PaletteFooterAction {
        keys: &["↑↓"],
        label: "navigate",
    },
    PaletteFooterAction {
        keys: &["↵"],
        label: enter_label,
    },
    PaletteFooterAction {
        keys: &["󱊷"],
        label: "close",
    },
];
let footer_actions: &[PaletteFooterAction<'_>] = if is_theme_selector {
    &theme_footer_actions
} else {
    &recent_project_footer_actions
};
```

Then pass `footer_actions` to `render_palette_footer`.

- [ ] **Step 2: Run formatting check**

Run:

```bash
cargo fmt --check
```

Expected: pass. If it fails, run `cargo fmt` and re-run `cargo fmt --check`.

## Task 6: Verification And Project Memory

**Files:**
- Modify: `.wolf/memory.md`
- Modify: `.wolf/cerebrum.md` if a reusable project learning was discovered
- Modify: `.wolf/buglog.json` because the user reported a broken/noisy stale recent-project behavior

- [ ] **Step 1: Run focused tests**

Run:

```bash
cargo test recent_projects_x remove_recent_drops_path_and_metadata --lib
```

Expected: pass.

- [ ] **Step 2: Run broader relevant tests**

Run:

```bash
cargo test recent_projects --lib
```

Expected: pass.

- [ ] **Step 3: Run GitNexus change detection**

Run:

```text
```

Expected: changed symbols match recent-project input, persistence, command routing, and renderer footer.

- [ ] **Step 4: Update OpenWolf memory**

Append one line to `.wolf/memory.md` using this format:

```markdown
| HH:MM | Added Recent Projects normal-mode x removal | recent-project palette/input/persistence | removes stale entries without disk deletion | ~tokens |
```

- [ ] **Step 5: Update bug log**

Append a bug record to `.wolf/buglog.json` describing stale recent-project paths causing failed opens and UI noise. Use the next available bug id and this shape:

```json
{
  "id": "bug-NNN",
  "timestamp": "2026-06-18T00:00:00Z",
  "error_message": "Recent Projects keeps stale moved repository paths; selecting them opens nothing and clutters the UI.",
  "file": "src/app/event_loop/commands_prompts.rs",
  "root_cause": "Recent project persistence had add/open behavior but no selected-entry removal path.",
  "fix": "Added Recent Projects normal-mode x removal that deletes the selected path from persisted recent projects and metadata, saves state, and refreshes UI lists.",
  "tags": ["recent-projects", "persistence", "palette", "ui-noise"],
  "related_bugs": [],
  "occurrences": 1,
  "last_seen": "2026-06-18T00:00:00Z"
}
```

- [ ] **Step 6: Final verification**

Run:

```bash
cargo test --lib
```

Expected: pass. If too slow or unrelated failures appear, report exact command output and the narrower passing commands.

---

## Self-Review

- Spec coverage: selected-entry removal, normal-mode `x` gate, no disk deletion, persistence save, palette/welcome refresh, footer hint, and tests are covered.
- Placeholder scan: no placeholder task remains; exact file paths and code snippets are included.
- Type consistency: command name is consistently `RemoveRecentProject`; persistence helper is consistently `remove_recent(&Path) -> bool`.
