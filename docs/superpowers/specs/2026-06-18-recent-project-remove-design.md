# Recent Project Remove Design

## Goal

Recent Projects currently stores full workspace paths and keeps showing entries after a repository has moved. Selecting a stale path can fail and the welcome/recent-project UI becomes noisy. Add a keyboard-first way to remove a selected recent project from the persisted list.

## User Flow

1. User opens Recent Projects with the existing flow, e.g. `Space p j`.
2. The palette starts in its normal query/editing behavior. Typing `x` still filters/inserts text and must not remove anything.
3. User presses `Esc` to leave palette insert/query mode and enter the existing palette normal mode. The mode indicator should be visible, consistent with other command-palette modes.
4. In Recent Projects normal mode only, `x` removes the currently selected recent project.
5. The row disappears immediately. If entries remain, selection clamps to a valid row and the palette stays open. If no entries remain, the palette may show the existing empty state or close after showing a toast.

## Behavior

- Removal only deletes the entry from `AppPersistentState.recent_projects` and matching `recent_project_meta`.
- Removal never deletes or modifies the project directory on disk.
- Removal saves persistent state immediately.
- Removal refreshes both the open Recent Projects palette and the welcome recent-project list.
- A transient toast reports the action, e.g. `Removed recent project: repo-name`.
- `Enter` keeps its existing behavior: open the selected project.
- `j/k` keep navigating in Recent Projects normal mode.

## Architecture

Follow the existing input-to-command flow:

`application.rs` -> `app/input/handler.rs` -> `app/input_map/mod.rs` -> `app/resolved_keymap.rs` -> `app/event_loop/commands.rs` -> `core/command_dispatch.rs` -> `app/app_state.rs`

Minimal implementation shape:

- Add a command variant such as `Command::RemoveRecentProject` and a command id if needed by the keymap path.
- Route `x` only when `CommandPaletteMode::RecentProjects` is active and the palette Vim/editor mode is normal, not insert/query mode.
- Add an `AppPersistentState` helper to remove one recent project path and prune metadata.
- Add an event-loop handler that reads the selected `CommandPaletteAction::OpenFile(path)`, removes that path from persistent state, saves, refreshes `open_recent_projects_palette_with_meta` or hidden welcome items, and marks UI layout dirty as needed.
- Update Recent Projects footer/hints to include `x remove` only for the Recent Projects palette.

## Error Handling

- If no selected recent-project action exists, the remove command is a no-op.
- If saving persistent state fails, preserve existing persistence behavior. Current `save()` does not surface errors, so the command should not panic.
- If the removed path is currently active as the open workspace, only remove it from the recent list; do not close or switch the active workspace.

## Tests

- Add focused input-map coverage proving `x` does not remove while the Recent Projects palette is in insert/query mode.
- Add focused input-map coverage proving `x` resolves to remove in Recent Projects normal mode.
- Add persistence/helper coverage proving removing a path removes both the recent entry and associated metadata.
- Add event-loop or command-level coverage if existing shell tests can exercise Recent Projects removal without heavy setup.

## Out Of Scope

- Automatic pruning of all missing paths.
- Bulk recent-project management.
- Deleting directories from disk.
- Changing how recent projects are added or ordered.
