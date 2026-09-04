# Multi-window — design

Date: 2026-09-04. Status: approved (user delegated design decisions; asked for
"the next phase" after workspace sessions).

## Goal

VS Code / Zed parity: one process, one dock icon, N windows. `netherize repoB`
from a terminal opens a **new window** unless a window already hosts that repo
(then that window is focused and the repo activated). Sessions (previous
spec) keep working *inside* each window.

## Approach: one `AppShell` per window

`AppShell` already owns everything a window needs — the winit `Window`, its
`Renderer` (own wgpu device), its `AsyncScheduler` (own tokio runtime, own PTY /
LSP / watcher registries), its bridge, input map, layout, sessions — and its
`window_event` already ignores foreign `WindowId`s. So the shell is kept whole
and a thin `MultiWindowApp` becomes the `ApplicationHandler`:

```rust
pub struct MultiWindowApp {
    shells: Vec<AppShell>,
    proxy: EventLoopProxy<AppEvent>,
    /// Single source of truth for state.toml; swapped INTO the shell being
    /// dispatched and back OUT afterwards (one thread, one shell at a time).
    persistent: AppPersistentState,
    focused: Option<WindowId>,
    cascade: u32,
}
```

The trait impl on `AppShell` turns into inherent methods with the same bodies:

| was (trait) | now (inherent) | change |
|---|---|---|
| `resumed(event_loop)` | `on_resumed(event_loop) -> Result<(), String>` | failure returns `Err` instead of `event_loop.exit()` |
| `window_event(_, id, ev)` | `on_window_event(id, ev)` | none |
| `about_to_wait(event_loop)` | `on_about_to_wait() -> Option<Instant>` | returns the wake deadline instead of `set_control_flow`; the exit branch just persists and returns |
| `user_event(_, ev)` | `on_user_event(ev)` | none |

`MultiWindowApp`:
- `resumed`: creates the first shell from the CLI args (`AppShell::new(proxy, cli_args, persistent)`), calls `on_resumed`; a failure exits the loop.
- `window_event(id, ev)`: `Focused(true)` updates `focused`; forwards to the shell whose `window_id() == Some(id)` (with the persistent swap); then `reap_and_spawn(event_loop)`.
- `user_event`: `RemoteOpen(paths)` is routed (below); the three "ready" events are forwarded to every shell (each pumps its own bridge; empty pumps are free).
- `about_to_wait`: forwards to every shell, sets `ControlFlow::WaitUntil(min deadline)` or `Wait`; shells in teardown add a 500 ms deadline; then `reap_and_spawn`.
- `reap_and_spawn`: shells with `exit_requested` start teardown (`begin_teardown`: submit `ClosePtySession` for every PTY, `ShutdownLspServersForRoot` and `StopFileWatch` for every live root, hide the window); shells whose teardown is ≥500 ms old are removed and `finish_teardown` shuts their runtime down with a 2 s timeout (`AsyncScheduler::shutdown`). New windows requested by shells (`take_pending_new_windows()`) are created with a +40 px cascade. When no shells remain the loop exits.

### Remote open routing (pure, tested)

```
dir = first directory in paths
dir hosted by some shell (active or parked session)  → forward to that shell (it focuses + activates)
dir not hosted                                        → new window with `paths` as its CLI args
no dir (files only)                                   → focused shell, else last shell
```

### New window command

`app.new_window` (`Command::NewWindow`, `mod+shift+n`, palette "New Window"):
an EMPTY window showing the Welcome screen (`NewWindowRequest::Welcome`:
no CLI dir, no recent, no cwd is attached), exactly like VS Code's New
Window — the user picks a recent there, and the lazy session restore brings
that repo's tabs back (layouts are persisted before the request).
`app.new_instance` stays as the separate-process escape hatch.

### Window switcher (`<leader>p p`) — user feedback round

The user's mental model is VS Code's: windows are the unit. So `projects.switch`
now lists the **other open windows** (one row per window with a root: `● branch
N unsaved`; Welcome windows are skipped) and Enter **focuses that window**. No
recents, no cold "restore" rows. The driver refreshes every shell's
`other_windows: Vec<WindowSummary>` each idle tick and before keyboard input;
the shell's confirm path recognises the `WINDOWS` title override and sets
`pending_focus_window`, which the driver honors in `reap_and_spawn`.

Tab memory is per root (`session_layouts`, pruned to recents) and restores
whenever that root is opened anywhere — startup, CLI, recents, Welcome —
including after the window that had it was closed. `open_sessions` and the
cold rows were removed. In-window parked sessions still exist (`space p n/b`
cycle them, `space p x` closes one) but are not surfaced in `space p p`.

### Persistent state

One `AppPersistentState` lives in `MultiWindowApp`. Every delegation does
`mem::swap` into `shell.persistent_state` before and out after, so all 59
existing `self.persistent_state.…` sites keep working and there is never a
stale copy to clobber another window's writes. `AppShell::new` takes the state
as a parameter instead of loading it; the top level loads it once in `run()`.

### Worker teardown

`FileWatchRegistry` gets a `Drop` that raises every stop flag (watcher
threads poll it at 1 s), so a dropped dispatch loop cannot leak watcher
threads. Runtime shutdown uses `shutdown_timeout(2 s)`: blocking PTY readers
exit when the PTY is closed by the requests sent 500 ms earlier.

`exiting()` on the driver persists every window's session layout and the
focused window's geometry, because Cmd+Q on macOS terminates without a
`CloseRequested` per window.

## Out of scope

Moving a session between windows (its PTY ids belong to one runtime),
restoring several windows after a restart (one window opens; other roots come
back as cold "○ restore" rows), per-window theme.

## Testing

- `route_remote_open` table test (hosted / unhosted / files-only / no focus).
- `min_deadline` over `None`/`Some` mixes.
- `Command::NewWindow` pushes the active root and persists layouts first.
- `live_roots` lists the active root first, then parked ones (what routing consults).
- `begin_teardown` hides nothing in tests (no window) but marks
  `closing_since`; `teardown_due` after 500 ms.
- Existing 1360 tests keep passing (shells built by `new_for_tests` are
  unchanged apart from the constructor signature).
