# Workspace Sessions — design

Date: 2026-09-04. Status: approved (user delegated design decisions).

## Problem

Netherize is one process, one window, one workspace. Opening a second repo
(`netherize repoB`, Open Folder, recent project, worktree, Dojo) *switches the
running window in place* and tears everything down: every PTY is closed, every
LSP server is shut down, every buffer and jump stack is dropped
(`prepare_for_workspace_switch` → `clear_workspace_session_state`). Going back
to repo A is a cold start. `--new-instance` avoids this but produces a second
process, a second dock icon and two writers of `state.toml`.

Two existing defects compound this:

- **File-watcher leak.** Every switch submits `StartFileWatch` and nothing ever
  stops the previous watcher; a crash log after four switches shows four
  `notify-rs fsevents loop` threads.
- **`StopLspServer` is `take_any`.** It stops an arbitrary server, which is
  harmless with one workspace but would kill another workspace's server once
  several roots are live.

## Goal

One window, one process, N repos live at once, instant switch that loses
nothing. Side-by-side windows are explicitly out of scope (user did not need
them); the session type is shaped so a later multi-window step can host one
session per window.

## 1. Model

```rust
/// Everything that belongs to one open repo. Exactly one is "active" (its
/// fields live directly on AppShell as today); the rest are parked in
/// `AppShell::background_sessions`.
pub(super) struct WorkspaceSession {
    pub root: PathBuf,                 // canonical; identity via path_matches
    pub app_state: AppState,           // buffers, workspace_model, diagnostics, jumps, symbol cache…
    pub shell: ShellWorkspaceState,    // per-workspace AppShell fields, see below
    pub panel_state: WorkbenchPanelState,
    pub pending_fs_events: Vec<FileSystemEvent>, // arrived while parked
    pub last_active: Instant,          // MRU ordering
}
```

`ShellWorkspaceState` groups the AppShell fields that are workspace-scoped.
The active session keeps them *in place* on `AppShell` (no rename churn);
`stash_active_session()` moves them out with `mem::take`/`mem::replace`, and
`restore_session(session)` moves them back. The field list:

| Group | Fields |
|---|---|
| Bottom terminal | `terminal_tabs`, `active_terminal_tab`, `pending_terminal_tab_spawns`, `ignored_terminal_tab_spawns`, `bottom_terminal_wheel_accum` |
| Right dock terminal | `right_pty_session_id`, `right_terminal_grid`, `pending_right_pty_spawn`, `right_agent_label`, `right_pty_startup_command`, `right_terminal_wheel_accum` (layout flags and `last_*_bounds` are window state: reset on every swap, not carried) |
| Terminal buffers | `terminal_buffer_grids`, `pending_lazygit_buffer_index`, `pending_lazydocker_buffer_index` |
| Explorer | `explorer_cursor`, `explorer_snapshot`, `explorer_snapshot_dirty`, `explorer_clipboard_path`, `pending_paste_source_path`, `pending_paste_target_dir` |
| Git | `workspace_git_branch` |
| LSP | `active_lsp_server`, `pending_lsp_server`, `pending_lsp_document_sync`, `lsp_completion_trigger_chars`, `active_lsp_guide` |
| Highlight / symbols | `highlight_spans`, `semantic_highlight_spans`, `cached_document_symbols`, `cached_document_symbols_path`, `outline_fetch_path`, `outline_selected`, `syntax_engine`, `syntax_engine_file`, `last_syntax_edit_hint` |

Everything else stays global: theme/config/keymap, layout engine, focus,
overlays, window/renderer, scheduler/bridge, dojo runtime, persistent state,
perf/fps counters, request-revision counters (monotonic, never reset) and
in-flight request ids (`Option<u64>`; set to `None` on switch so a late
result for the previous workspace is dropped by mismatch).

`panel_state` is per session so the terminal you had open in repo A is still
open when you come back. Layout dirty flags are all set on restore.

## 2. Switching

`activate_session(root, follow_files)`:

1. If `root` matches the active session → open `follow_files`, done.
2. If a background session matches → `stash_active_session()` into
   `background_sessions`, remove the match, `restore_session(it)`.
3. Else build a fresh session: `AppState::new(save_path)` + `EnterNormal` +
   `set_indent_config` + `set_terminal_panel_open` (same as startup),
   `attach_workspace(root)`, `panel_state` from theme with all docks hidden,
   then stash + restore as above. Register with recents (`push_recent_with_icon`).
4. Post-restore, in order: `StartFileWatch{root}` (idempotent, see §4),
   `sync_lsp_server_for_workspace()` (no-op when the restored
   `active_lsp_server` already matches), open `follow_files`, if an active
   file exists `invalidate_highlights_and_parse_active_buffer()` +
   `submit_lsp_did_open_for_active_file()`, drain `pending_fs_events`
   through the normal filesystem handler, git status/baseline refresh,
   `mark_explorer_dirty()`, `update_window_title()`, all layout dirty flags,
   `dojo_after_workspace_switch(root)`, toast `"repoB (2/3)"`.

Nothing is shut down or cleared. `prepare_for_workspace_switch` and the
`ShutdownAllLspServers` submit on switch are deleted.

`perform_workspace_switch` becomes a thin wrapper over `activate_session`;
`switch_workspace_with_files` drops its dirty-buffer confirmation (switching no
longer destroys anything). `handle_remote_open`, Open Folder, recent project,
worktree palette and Dojo all keep calling `switch_workspace_with_files`, so
they get sessions for free.

`close_session(root)`:

1. Dirty guard reuses `PendingConfirmationAction::WorkspaceSwitch` renamed to
   `WorkspaceClose { root, dirty_count }`: y = save all, n = discard, Esc = stay.
2. `ClosePtySession` for every session id owned by that session (bottom tabs,
   right PTY, buffer grids). `ShutdownLspServersForRoot{root}`.
   `StopFileWatch{root}`.
3. If it was the active session, restore the MRU background session; if none
   remain, restore a fresh empty session with `initial_launch_welcome = true`.

## 3. Commands and keys

| Command id | Enum | Key | Behaviour |
|---|---|---|---|
| `projects.switch` | `SwitchWorkspaceSession` | `<leader>p p` | Palette "WORKSPACES": live sessions first (MRU, current excluded), then recents not already live. Enter → `activate_session`. |
| `projects.next` | `NextWorkspaceSession` | `<leader>p n` | Activate the most recently used background session (round-robin by MRU). Toast `name (i/n)`. |
| `projects.prev` | `PrevWorkspaceSession` | `<leader>p b` | Same, opposite direction (least recently used). |
| `projects.close` | `CloseWorkspaceSession` | `<leader>p x` | `close_session(active)` with dirty guard. |

The switcher reuses `CommandPaletteMode::RecentProjects` plus
`set_title_override("WORKSPACES")`, so Enter/`x`/rendering already work. Live
rows are `CommandPaletteItem::recent_project_with_meta` with the
`secondary_label` extended by `;live=1;dirty=<n>;branch=<b>` — the recent
projects renderer reads the existing `icon=`/`last=` keys and now also renders
a `●` for `live`, a dirty count and the branch. `x` on a live row is ignored
(close is `<leader>p x`); on a recent row it removes the recent as before.

`--new-instance` and `app.new_instance` remain as the escape hatch.

## 4. Worker routing for parked sessions

- **PTY.** `PtyOutput`, `PtySpawned`, `PtySessionClosed` look up the session id
  across the active shell *and* every background session
  (`terminal_grid_for_session_mut(id) -> Option<(&mut TerminalGrid, bool /*active*/)>`).
  Output for a parked session feeds its grid but neither sets layout flags nor
  requests a redraw. A `PtySpawned` for a parked session's pending spawn is
  bound into that session's tab.
- **File system.** `FileSystemEvents{root_path}` whose root matches a parked
  session is appended to `pending_fs_events` (with `explorer_snapshot_dirty`)
  and drained on restore through `handle_filesystem_result`. Every other root
  (the active workspace, or the parent-dir watcher of a file opened outside
  it) is handled as today.
- **LSP.** Results carry no root; anything not matching the active shell's
  request ids / active server is dropped as today. `LspDiagnostics` for a parked
  workspace are dropped; on restore `did_open` makes the server republish.
- **Watcher lifetime (bug fix).** The dispatch loop keeps
  `HashMap<PathBuf, Arc<AtomicBool>>` of stop flags keyed by root.
  `StartFileWatch` for a root already in the map is a no-op; `StopFileWatch{root}`
  sets the flag and removes the entry. `execute_file_watch_loop` polls
  `notify_rx.recv_timeout(1s)` instead of `recv()` and returns `Ok(())` when the
  flag is set; the restart loop in `run_file_watch_request` also exits on the
  flag.
- **LSP shutdown by root (new).** `ShutdownLspServersForRoot{root_path}`
  drains only sessions whose `root_path` matches. `StopLspServer` gains
  `{server_name, root_path}` and stops that server only
  (`get_handle_by_binary_and_root`), so a `.md` buffer in repo B cannot stop
  repo A's rust-analyzer.

## 5. Persistence (phase 2, shipped same day)

`state.toml` gains two keys (both `#[serde(default)]`, old files still parse):

```toml
open_sessions = ["/repo/a", "/repo/b"]          # active first, then MRU; then cold roots
[session_layouts."/repo/a"]
active = "/repo/a/src/main.rs"
bottom_terminal = true
[[session_layouts."/repo/a".files]]              # text tabs in order, cursor per tab
path = "/repo/a/src/main.rs"
line = 12
col = 4
```

- **Snapshot** = `AppState::session_layout_snapshot()` (text tabs only; the
  active tab's cursor from the live state, parked tabs' cursors from their
  saved view state) + `panel_state.bottom.visible`.
- **When written**: `persist_session_layouts(force)` on a 5 s tick in
  `about_to_wait` (compare, write only on change), on `exit_requested`
  (force), and `forget_session` + save when a session is closed or its recent
  entry is removed. Layouts are pruned to roots still in recents/open list.
- **Restore**: eagerly at startup for the launch workspace when the CLI named
  no files (`apply_session_layout`: reopen existing files, `jump_to_line_col`,
  activate the recorded tab, terminal dock visibility); lazily in
  `new_session` for any root that has a layout — so `netherize repoB` and the
  switcher both bring a repo back as it was left.
- ~~Cold roots~~ (removed 2026-09-05 on user feedback): no `open_sessions`
  list and no `○ restore` rows; a root simply restores its tabs whenever it is
  opened again, and `<leader>p p` became the window switcher (see the
  multi-window spec).
- `AppPersistentState::save()` is a no-op under `cfg!(test)`: shells built in
  tests load the real state file, and switching tests used to push temp dirs
  into the user's recents.

## 6. Testing

Unit (`cargo test --lib`):

- `stash_restore_round_trip`: populate every `ShellWorkspaceState` field with
  a sentinel, stash, assert the shell is back at defaults, restore, assert the
  sentinels are back. Guards against a forgotten field.
- `activate_session_reuses_existing`: two temp roots; activate A, open a file,
  activate B, activate A again → same buffers, `background_sessions.len()==1`.
- `remote_open_same_root_only_opens_files`.
- `pty_output_for_parked_session_feeds_grid_without_redraw`.
- `fs_events_for_parked_session_are_queued_and_drained_on_restore`.
- `switcher_lists_live_sessions_before_recents` (item order + labels).
- `next_prev_cycle_mru`.
- `close_session_falls_back_to_mru_then_welcome`.
- Dispatch: `start_file_watch_twice_creates_one_watcher`,
  `stop_file_watch_removes_entry`, `shutdown_lsp_for_root_leaves_other_roots`.

GUI checklist (user): two terminals `netherize .` in two repos → one dock
icon, second repo appears as a session; `<leader>p p` shows both, Enter swaps
instantly with the bottom terminal still running; LSP hover works immediately
after swapping back; `<leader>p x` with a dirty buffer prompts; after ten
swaps Activity Monitor shows one `notify-rs fsevents loop` thread per open
session.

## Out of scope

Multi-window, per-session persistence (phase 2), Dojo runtime per session
(stays global), shared `state.toml` merging across `--new-instance` processes.
