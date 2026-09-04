# Workspace Sessions Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** N repos live in one window; switching between them is an instant swap that keeps terminals, LSP servers, buffers and panels alive.

**Architecture:** The active workspace keeps living in `AppShell`'s existing fields. A `WorkspaceSession` value type carries a parked workspace (`AppState` + the per-workspace shell fields grouped in `ShellWorkspaceState` + `panel_state`). `stash_active_session` / `restore_session` move fields in and out with `mem::take`. Worker results for parked sessions are routed by PTY session id / root path. Two worker fixes ride along: file watchers get a stop flag (today they leak on every switch) and LSP shutdown becomes per-root.

**Tech Stack:** Rust 2024, winit/wgpu shell, tokio worker, `cargo test --lib` (1343 tests, ~1 min).

**Spec:** `docs/superpowers/specs/2026-09-04-workspace-sessions-design.md`

## Global Constraints

- Never `git commit` — the human commits. Steps that say "commit" mean "stop and report; leave the tree ready".
- `gitnexus_impact` was run (LOW risk) for: `perform_workspace_switch`, `prepare_for_workspace_switch`, `switch_workspace_with_files`, `handle_remote_open`, `sync_lsp_server_for_workspace`, `handle_terminal_result`, `handle_filesystem_result`, `execute_file_watch_loop`. Re-run for any symbol outside that list before editing it.
- Keep `cargo test --lib` green after every task. Run the focused filter first, the whole suite at the end of each task.
- Session identity = canonical root path compared with `crate::app::app_state::path_matches`.
- No new crates.

---

### Task 1: File-watcher stop flags (worker)

**Files:**
- Modify: `src/async_runtime/message.rs:173-180` (add `StopFileWatch`)
- Modify: `src/async_runtime/scheduler/file_watch.rs` (registry, flag plumbing, `recv_timeout`)
- Modify: `src/async_runtime/scheduler/dispatch.rs:61-152`
- Modify: `src/async_runtime/scheduler/syntax_jobs.rs:348` (add `StopFileWatch` to the "handled elsewhere" arm)
- Test: `src/async_runtime/scheduler/file_watch.rs` (new `#[cfg(test)] mod tests`)

**Interfaces:**
- Produces: `WorkerRequestPayload::StopFileWatch { root_path: PathBuf }`;
  `pub(super) struct FileWatchRegistry` with `fn start(&mut self, root: &Path) -> Option<Arc<AtomicBool>>` (None = already watched) and `fn stop(&mut self, root: &Path) -> bool`;
  `run_file_watch_request(request, worker_tx, event_proxy, stop: Arc<AtomicBool>)`.

- [ ] **Step 1: Write the failing registry test** (append to `file_watch.rs`)

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::Ordering;

    #[test]
    fn start_twice_creates_one_watcher_and_stop_sets_flag() {
        let mut reg = FileWatchRegistry::default();
        let root = std::path::PathBuf::from("/tmp/ws-a");
        let flag = reg.start(&root).expect("first start registers");
        assert!(reg.start(&root).is_none(), "second start is a no-op");
        assert!(!flag.load(Ordering::Relaxed));
        assert!(reg.stop(&root));
        assert!(flag.load(Ordering::Relaxed), "stop raises the flag");
        assert!(!reg.stop(&root), "stopping twice is harmless");
        assert!(reg.start(&root).is_some(), "root can be watched again");
    }
}
```

- [ ] **Step 2: Run** `cargo test --lib file_watch::tests` → FAIL (no `FileWatchRegistry`).

- [ ] **Step 3: Implement**

In `message.rs`, after `StartFileWatch { .. }`:
```rust
    /// Drop the watcher for `root_path` (workspace session closed).
    StopFileWatch {
        root_path: PathBuf,
    },
```

In `file_watch.rs`:
```rust
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

/// Live watchers keyed by root. One watcher per root, ever — before this the
/// dispatch loop spawned a fresh watcher on every workspace switch and never
/// stopped the old one (four `notify-rs fsevents loop` threads after four
/// switches).
#[derive(Default)]
pub(super) struct FileWatchRegistry {
    flags: std::collections::HashMap<PathBuf, Arc<AtomicBool>>,
}

impl FileWatchRegistry {
    /// `Some(flag)` when a new watcher must be spawned; `None` when this root
    /// is already watched.
    pub(super) fn start(&mut self, root: &Path) -> Option<Arc<AtomicBool>> {
        if self.flags.contains_key(root) {
            return None;
        }
        let flag = Arc::new(AtomicBool::new(false));
        self.flags.insert(root.to_path_buf(), flag.clone());
        Some(flag)
    }

    pub(super) fn stop(&mut self, root: &Path) -> bool {
        match self.flags.remove(root) {
            Some(flag) => {
                flag.store(true, Ordering::Relaxed);
                true
            }
            None => false,
        }
    }
}
```

`run_file_watch_request` gains `stop: Arc<AtomicBool>`; at the top of its `loop {` add `if stop.load(Ordering::Relaxed) { return; }`, clone `stop` into the `spawn_blocking` closure and pass it to `execute_file_watch_loop(&watcher_request, &watcher_tx, &watcher_proxy, &stop_for_loop)`.

`execute_file_watch_loop` gains `stop: &AtomicBool`. Replace the outer `match notify_rx.recv()` with:
```rust
        let first = match notify_rx.recv_timeout(FILE_WATCH_STOP_POLL) {
            Ok(event) => event,
            Err(std_mpsc::RecvTimeoutError::Timeout) => {
                if stop.load(Ordering::Relaxed) {
                    return Ok(());
                }
                continue;
            }
            Err(err) => return Err(format!("file watcher channel disconnected: {err}")),
        };
        match first {
```
with `const FILE_WATCH_STOP_POLL: Duration = Duration::from_secs(1);` and the previous `Ok(Ok(event))`/`Ok(Err(err))` arms kept as `Ok(event)`/`Err(err)`. Inside the batch loop, after `channel_disconnected` handling, keep everything else.

In `dispatch.rs`: `let mut file_watches = FileWatchRegistry::default();` next to `pty_sessions`. Replace the `StartFileWatch` arm:
```rust
        if let WorkerRequestPayload::StartFileWatch { root_path, .. } = &request.payload {
            let Some(stop) = file_watches.start(root_path) else {
                async_trace!("[Scheduler] watcher already live for {}", root_path.display());
                continue;
            };
            let worker_tx = result_tx.clone();
            let event_proxy = event_proxy.clone();
            tokio::spawn(async move {
                run_file_watch_request(request, worker_tx, event_proxy, stop).await;
            });
            continue;
        }
        if let WorkerRequestPayload::StopFileWatch { root_path } = &request.payload {
            file_watches.stop(root_path);
            continue;
        }
```
Import `FileWatchRegistry` from `file_watch`. Add `| WorkerRequestPayload::StopFileWatch { .. }` to the `syntax_jobs.rs:348` arm's pattern.

- [ ] **Step 4: Run** `cargo test --lib file_watch` → PASS; `cargo build` clean.
- [ ] **Step 5: Report** (no commit).

---

### Task 2: Per-root LSP shutdown (worker)

**Files:**
- Modify: `src/async_runtime/message.rs:408` (add `ShutdownLspServersForRoot`, change `StopLspServer`)
- Modify: `src/async_runtime/scheduler.rs:279-286` (add `drain_for_root`, `remove_by_binary_and_root`)
- Modify: `src/async_runtime/scheduler/lsp.rs:311-337`
- Modify: `src/app/event_loop/setup.rs:1838-1854` (`StopLspServer` now carries the server)
- Test: `src/async_runtime/scheduler.rs` tests module (find with `grep -n "mod tests" src/async_runtime/scheduler.rs`; create one if absent)

**Interfaces:**
- Produces: `WorkerRequestPayload::ShutdownLspServersForRoot { root_path: PathBuf }`;
  `WorkerRequestPayload::StopLspServer { server_name: String, root_path: PathBuf }`;
  `LspSessionRegistry::drain_for_root(&self, root: &Path) -> Result<Vec<LspSessionHandle>, String>`;
  `LspSessionRegistry::remove_by_binary_and_root(&self, binary: &str, root: &Path) -> Result<Option<LspSessionHandle>, String>`.

- [ ] **Step 1: Failing test** — `LspSessionHandle` needs a process; construct via the existing test helper if one exists (`grep -n "fn test_lsp_handle\|LspClientProcess::new" src/async_runtime/scheduler*.rs`). If none, test only the pure key logic by adding `pub(super) fn session_roots(&self) -> Vec<PathBuf>` and asserting `drain_for_root` leaves the other root:

```rust
#[test]
fn drain_for_root_leaves_other_roots() {
    let registry = LspSessionRegistry::default();
    registry.replace("rust-analyzer@/a".into(), test_handle("rust-analyzer", "/a")).unwrap();
    registry.replace("rust-analyzer@/b".into(), test_handle("rust-analyzer", "/b")).unwrap();
    let drained = registry.drain_for_root(Path::new("/a")).unwrap();
    assert_eq!(drained.len(), 1);
    assert_eq!(registry.session_roots(), vec![PathBuf::from("/b")]);
}
```
(`test_handle` = whatever helper the file already uses to build a handle; if the process type cannot be built in tests, keep the registry methods and skip this test — say so in the report.)

- [ ] **Step 2: Run** the filter → FAIL.
- [ ] **Step 3: Implement**

```rust
    pub(super) fn drain_for_root(&self, root: &Path) -> Result<Vec<LspSessionHandle>, String> {
        let mut guard = self.sessions.lock().map_err(|_| "lsp session lock poisoned".to_string())?;
        let keys: Vec<String> = guard
            .iter()
            .filter(|(_, s)| s.root_path == root)
            .map(|(k, _)| k.clone())
            .collect();
        Ok(keys.into_iter().filter_map(|k| guard.remove(&k)).collect())
    }

    pub(super) fn remove_by_binary_and_root(&self, binary: &str, root: &Path) -> Result<Option<LspSessionHandle>, String> {
        let mut guard = self.sessions.lock().map_err(|_| "lsp session lock poisoned".to_string())?;
        let key = guard
            .iter()
            .find(|(_, s)| s.root_path == root && session_name_matches_binary(&s.server_name, binary))
            .map(|(k, _)| k.clone());
        Ok(key.and_then(|k| guard.remove(&k)))
    }
```

`lsp.rs`: `StopLspServer { server_name, root_path }` → `remove_by_binary_and_root`; if `None` return `Err("stop lsp rejected: no such server")`. New arm:
```rust
        WorkerRequestPayload::ShutdownLspServersForRoot { root_path } => {
            let sessions = lsp_sessions.drain_for_root(root_path)?;
            let mut last_exit_status = None;
            for session in sessions {
                session.process.update_request_meta(request.request_id, request.revision_id);
                last_exit_status = session.process.shutdown_and_exit()?;
            }
            Ok(WorkerResultPayload::LspServerStopped {
                exit_status: last_exit_status,
                reason: format!("lsp servers shutdown for closed workspace {}", root_path.display()),
            })
        }
```
Keep `ShutdownAllLspServers` (still used at quit — verify with grep; if only the switch used it, delete it).

`setup.rs:1842` — build the payload from the server being dropped:
```rust
            None => {
                let dropped = self.active_lsp_server.take().or_else(|| self.pending_lsp_server.take());
                if let Some(server) = dropped {
                    self.pending_lsp_server = None;
                    self.submit(RequestSpec {
                        revision_id: 0,
                        topic: RequestTopic::LspClient,
                        payload: WorkerRequestPayload::StopLspServer {
                            server_name: server.server_name,
                            root_path: server.root_path,
                        },
                    });
                    true
                } else { false }
            }
```
Fix every other `StopLspServer` construction (`grep -rn "StopLspServer" src`).

- [ ] **Step 4: Run** `cargo test --lib lsp` → PASS. `cargo build`.
- [ ] **Step 5: Report.**

---

### Task 3: `WorkspaceSession` + stash/restore round-trip

**Files:**
- Create: `src/app/event_loop/workspace_session.rs`
- Modify: `src/app/event_loop/mod.rs` (declare module, add `background_sessions: Vec<WorkspaceSession>` field, init in `setup.rs` with `Vec::new()`)
- Test: `src/app/event_loop/workspace_session.rs` tests module

**Interfaces:**
- Produces:
```rust
pub(super) struct ShellWorkspaceState { /* table in spec §1 */ }
impl ShellWorkspaceState { pub(super) fn fresh(theme: &ThemeConfig) -> Self }
pub(super) struct WorkspaceSession {
    pub root: PathBuf,
    pub app_state: AppState,
    pub shell: ShellWorkspaceState,
    pub panel_state: WorkbenchPanelState,
    pub pending_fs_events: Vec<FileSystemEvent>,
    pub last_active: Instant,
}
impl AppShell {
    pub(super) fn stash_active_session(&mut self) -> Option<WorkspaceSession>; // None when no workspace root
    pub(super) fn restore_session(&mut self, session: WorkspaceSession);
    pub(super) fn session_index_for_root(&self, root: &Path) -> Option<usize>;
    pub(super) fn owns_pty_session(&self, id: u64) -> bool;  // active shell
}
impl WorkspaceSession {
    pub(super) fn owns_pty_session(&self, id: u64) -> bool;
    pub(super) fn pty_session_ids(&self) -> Vec<u64>;
}
```

- [ ] **Step 1: Failing round-trip test**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::event_loop::AppShell;

    #[test]
    fn stash_restore_round_trip_moves_every_workspace_field() {
        let mut shell = AppShell::new_for_tests().expect("shell");
        let root = shell.app_state.workspace_root_path().expect("root").to_path_buf();
        // Sentinels in every group of ShellWorkspaceState.
        shell.right_pty_session_id = Some(41);
        shell.terminal_tabs[0].session_id = Some(42);
        shell.active_terminal_tab = 0;
        shell.terminal_buffer_grids.insert(43, crate::terminal::grid::TerminalGrid::new(3, 3));
        shell.explorer_cursor = 7;
        shell.explorer_clipboard_path = Some(root.join("x"));
        shell.workspace_git_branch = Some("feature".into());
        shell.lsp_completion_trigger_chars = vec!['.'];
        shell.highlight_spans.push(Default::default());
        shell.outline_selected = Some(3);
        shell.panel_state.bottom.visible = true;

        let session = shell.stash_active_session().expect("has workspace");

        assert!(shell.app_state.workspace_root_path().is_none());
        assert_eq!(shell.right_pty_session_id, None);
        assert_eq!(shell.terminal_tabs.len(), 1);
        assert_eq!(shell.terminal_tabs[0].session_id, None);
        assert!(shell.terminal_buffer_grids.is_empty());
        assert_eq!(shell.explorer_cursor, 0);
        assert_eq!(shell.explorer_clipboard_path, None);
        assert_eq!(shell.workspace_git_branch, None);
        assert!(shell.lsp_completion_trigger_chars.is_empty());
        assert!(shell.highlight_spans.is_empty());
        assert_eq!(shell.outline_selected, None);
        assert!(!shell.panel_state.bottom.visible);
        assert!(session.owns_pty_session(41) && session.owns_pty_session(42) && session.owns_pty_session(43));
        assert_eq!(session.root, root);

        shell.restore_session(session);

        assert_eq!(shell.app_state.workspace_root_path(), Some(root.as_path()));
        assert_eq!(shell.right_pty_session_id, Some(41));
        assert_eq!(shell.terminal_tabs[0].session_id, Some(42));
        assert!(shell.terminal_buffer_grids.contains_key(&43));
        assert_eq!(shell.explorer_cursor, 7);
        assert_eq!(shell.workspace_git_branch.as_deref(), Some("feature"));
        assert_eq!(shell.lsp_completion_trigger_chars, vec!['.']);
        assert_eq!(shell.highlight_spans.len(), 1);
        assert_eq!(shell.outline_selected, Some(3));
        assert!(shell.panel_state.bottom.visible);
        assert!(shell.editor_needs_layout && shell.sidebar_needs_layout && shell.terminal_needs_layout);
    }
}
```
(Adjust sentinel field names to the real ones if a name differs — the spec table is authoritative; `HighlightSpan` may need a real constructor instead of `Default`.)

- [ ] **Step 2: Run** `cargo test --lib workspace_session` → FAIL.
- [ ] **Step 3: Implement** `workspace_session.rs`

```rust
//! One open repo = one `WorkspaceSession`. The active session's fields live
//! directly on `AppShell`; parked sessions are stashed here so switching is a
//! field swap, not a teardown. See docs/superpowers/specs/2026-09-04-workspace-sessions-design.md.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::Instant;

use super::{AppShell, ExplorerSnapshot, TerminalTab};
use crate::app::app_state::{path_matches, AppState};
use crate::async_runtime::message::FileSystemEvent;
use crate::config::theme_config::ThemeConfig;
use crate::terminal::grid::{HighlightColors, TerminalGrid};
use crate::workbench::panel_state::WorkbenchPanelState;

pub(super) struct ShellWorkspaceState {
    // bottom terminal
    pub terminal_tabs: Vec<TerminalTab>,
    pub active_terminal_tab: usize,
    pub pending_terminal_tab_spawns: HashMap<u64, usize>,
    pub ignored_terminal_tab_spawns: HashSet<u64>,
    pub bottom_terminal_wheel_accum: f64,
    // right dock terminal
    pub right_pty_session_id: Option<u64>,
    pub right_terminal_grid: TerminalGrid,
    pub right_terminal_needs_layout: bool,
    pub last_right_terminal_bounds: Option<[f32; 4]>,
    pub pending_right_pty_spawn: bool,
    pub right_agent_label: Option<String>,
    pub right_pty_startup_command: Option<String>,
    pub right_terminal_wheel_accum: f64,
    // terminal buffers
    pub terminal_buffer_grids: HashMap<u64, TerminalGrid>,
    pub pending_lazygit_buffer_index: Option<usize>,
    pub pending_lazydocker_buffer_index: Option<usize>,
    // explorer
    pub explorer_cursor: usize,
    pub explorer_snapshot: ExplorerSnapshot,
    pub explorer_snapshot_dirty: bool,
    pub explorer_clipboard_path: Option<PathBuf>,
    pub pending_paste_source_path: Option<PathBuf>,
    pub pending_paste_target_dir: Option<PathBuf>,
    // git / lsp
    pub workspace_git_branch: Option<String>,
    pub active_lsp_server: Option<super::ActiveLspServer>,
    pub pending_lsp_server: Option<super::ActiveLspServer>,
    pub pending_lsp_document_sync: Option<super::PendingLspDocumentSync>, // real type name from mod.rs
    pub lsp_completion_trigger_chars: Vec<char>,
    pub active_lsp_guide: Option<super::LspInstallGuide>,
    // highlight / symbols
    pub highlight_spans: Vec<crate::syntax::HighlightSpan>,
    pub semantic_highlight_spans: Vec<crate::syntax::HighlightSpan>,
    pub cached_document_symbols: Vec<crate::async_runtime::message::LspDocumentSymbol>,
    pub cached_document_symbols_path: Option<PathBuf>,
    pub outline_fetch_path: Option<PathBuf>,
    pub outline_selected: Option<usize>,
    pub syntax_engine: Option<crate::syntax::SyntaxEngine>,
    pub syntax_engine_file: Option<PathBuf>,
    pub last_syntax_edit_hint: Option<super::SyntaxEditHint>,
}

fn fresh_grid(theme: &ThemeConfig) -> TerminalGrid {
    let mut g = TerminalGrid::new(120, 40);
    g.highlight_colors = HighlightColors::from_theme(theme);
    g
}

impl ShellWorkspaceState {
    pub(super) fn fresh(theme: &ThemeConfig) -> Self {
        Self {
            terminal_tabs: vec![TerminalTab::new(fresh_grid(theme), "bash".to_string())],
            active_terminal_tab: 0,
            pending_terminal_tab_spawns: HashMap::new(),
            ignored_terminal_tab_spawns: HashSet::new(),
            bottom_terminal_wheel_accum: 0.0,
            right_pty_session_id: None,
            right_terminal_grid: fresh_grid(theme),
            right_terminal_needs_layout: true,
            last_right_terminal_bounds: None,
            pending_right_pty_spawn: false,
            right_agent_label: None,
            right_pty_startup_command: None,
            right_terminal_wheel_accum: 0.0,
            terminal_buffer_grids: HashMap::new(),
            pending_lazygit_buffer_index: None,
            pending_lazydocker_buffer_index: None,
            explorer_cursor: 0,
            explorer_snapshot: ExplorerSnapshot::default(),
            explorer_snapshot_dirty: true,
            explorer_clipboard_path: None,
            pending_paste_source_path: None,
            pending_paste_target_dir: None,
            workspace_git_branch: None,
            active_lsp_server: None,
            pending_lsp_server: None,
            pending_lsp_document_sync: None,
            lsp_completion_trigger_chars: Vec::new(),
            active_lsp_guide: None,
            highlight_spans: Vec::new(),
            semantic_highlight_spans: Vec::new(),
            cached_document_symbols: Vec::new(),
            cached_document_symbols_path: None,
            outline_fetch_path: None,
            outline_selected: None,
            syntax_engine: None,
            syntax_engine_file: None,
            last_syntax_edit_hint: None,
        }
    }
}

pub(super) struct WorkspaceSession {
    pub root: PathBuf,
    pub app_state: AppState,
    pub shell: ShellWorkspaceState,
    pub panel_state: WorkbenchPanelState,
    pub pending_fs_events: Vec<FileSystemEvent>,
    pub last_active: Instant,
}

impl WorkspaceSession {
    pub(super) fn pty_session_ids(&self) -> Vec<u64> {
        self.shell.terminal_tabs.iter().filter_map(|t| t.session_id)
            .chain(self.shell.right_pty_session_id)
            .chain(self.shell.terminal_buffer_grids.keys().copied())
            .collect()
    }
    pub(super) fn owns_pty_session(&self, id: u64) -> bool {
        self.pty_session_ids().contains(&id)
    }
    pub(super) fn name(&self) -> String {
        self.root.file_name().and_then(|n| n.to_str()).unwrap_or("workspace").to_string()
    }
}

impl AppShell {
    /// Move the active workspace out of the shell, leaving fresh defaults.
    /// `None` when no workspace is attached (welcome screen).
    pub(super) fn stash_active_session(&mut self) -> Option<WorkspaceSession> {
        let root = self.app_state.workspace_root_path()?.to_path_buf();
        let fresh_state = self.fresh_app_state();
        let app_state = std::mem::replace(&mut self.app_state, fresh_state);
        let shell = self.take_shell_workspace_state();
        let mut parked_panels = WorkbenchPanelState::from_ui_theme(&self.theme.ui);
        parked_panels.left.visible = false;
        parked_panels.right.visible = false;
        parked_panels.bottom.visible = false;
        let panel_state = std::mem::replace(&mut self.panel_state, parked_panels);
        self.reset_in_flight_requests();
        self.mark_all_layout_dirty();
        Some(WorkspaceSession { root, app_state, shell, panel_state, pending_fs_events: Vec::new(), last_active: Instant::now() })
    }

    pub(super) fn restore_session(&mut self, session: WorkspaceSession) {
        let WorkspaceSession { app_state, shell, panel_state, .. } = session;
        self.app_state = app_state;
        self.put_shell_workspace_state(shell);
        self.panel_state = panel_state;
        let _ = self.app_state.set_terminal_panel_open(self.panel_state.bottom.visible);
        self.reset_in_flight_requests();
        self.mark_all_layout_dirty();
    }

    pub(super) fn session_index_for_root(&self, root: &Path) -> Option<usize> {
        self.background_sessions.iter().position(|s| path_matches(&s.root, root))
    }

    pub(super) fn owns_pty_session(&self, id: u64) -> bool {
        self.terminal_tabs.iter().any(|t| t.session_id == Some(id))
            || self.right_pty_session_id == Some(id)
            || self.terminal_buffer_grids.contains_key(&id)
    }

    fn fresh_app_state(&self) -> AppState {
        let mut state = AppState::new(self.app_state.default_save_path().to_path_buf(), None);
        let _ = state.apply_mode_event(crate::core::mode::ModeEvent::EnterNormal);
        state.set_indent_config(self.ui_config.indent);
        state
    }

    fn take_shell_workspace_state(&mut self) -> ShellWorkspaceState {
        let fresh = ShellWorkspaceState::fresh(&self.theme);
        self.put_shell_workspace_state_returning(fresh)
    }

    fn put_shell_workspace_state(&mut self, s: ShellWorkspaceState) {
        let _ = self.put_shell_workspace_state_returning(s);
    }

    /// Swap every per-workspace field with `incoming`, returning the previous
    /// values. ONE place lists the fields; the round-trip test guards it.
    fn put_shell_workspace_state_returning(&mut self, incoming: ShellWorkspaceState) -> ShellWorkspaceState {
        use std::mem::replace;
        ShellWorkspaceState {
            terminal_tabs: replace(&mut self.terminal_tabs, incoming.terminal_tabs),
            active_terminal_tab: replace(&mut self.active_terminal_tab, incoming.active_terminal_tab),
            // … one `replace` line per field, in the struct's order …
            last_syntax_edit_hint: replace(&mut self.last_syntax_edit_hint, incoming.last_syntax_edit_hint),
        }
    }

    fn reset_in_flight_requests(&mut self) {
        self.completion_resolve_request_id = None;
        self.active_lsp_completion_request_id = None;
        self.hover_loading_request_id = None;
        self.latest_hover_request_id = None;
        self.latest_definition_request_id = None;
        self.latest_rename_request_id = None;
        self.canvas_def_request_id = None;
        self.canvas_refs_request_id = None;
        self.canvas_hover_request_id = None;
        self.canvas_completion_request_id = None;
        self.pending_parse_after_debounce = false;
        self.pending_git_diff_after_debounce = false;
    }

    fn mark_all_layout_dirty(&mut self) {
        self.editor_needs_layout = true;
        self.editor_caret_needs_layout = true;
        self.sidebar_needs_layout = true;
        self.terminal_needs_layout = true;
        self.buffer_terminal_needs_layout = true;
        self.right_terminal_needs_layout = true;
        self.last_window_title = None;
    }
}
```
Check real type names for `pending_lsp_document_sync`, `active_lsp_guide`, `last_syntax_edit_hint`, `HighlightSpan` with `grep -n "pending_lsp_document_sync:\|active_lsp_guide:\|last_syntax_edit_hint:\|highlight_spans:" src/app/event_loop/mod.rs`.

- [ ] **Step 4: Run** `cargo test --lib workspace_session` → PASS.
- [ ] **Step 5: Report.**

---

### Task 4: `activate_session` replaces the destructive switch

**Files:**
- Modify: `src/app/event_loop/commands_explorer.rs:32-107,147-300`
- Modify: `src/app/event_loop/workspace_session.rs` (add `activate_session`, `close_session`)
- Modify: `src/app/event_loop/mod.rs:490-496` (`WorkspaceSwitch` → `WorkspaceClose { root, dirty_count }`)
- Modify: `src/app/event_loop/commands_prompts.rs:93-100,195-215,895-915`
- Test: `src/app/event_loop/commands_tests.rs:5425-5580` (rewrite the dirty-guard block) + new tests

**Interfaces:**
- Produces: `AppShell::activate_session(&mut self, root: PathBuf, follow_files: Vec<PathBuf>) -> bool`,
  `AppShell::close_session(&mut self, root: PathBuf) -> bool` (dirty guard inside),
  `AppShell::session_count(&self) -> usize` (background + 1 if active has a root),
  `AppShell::begin_workspace_close_confirmation(root, dirty_count) -> bool`.
- Consumes: Task 3 stash/restore, Task 1 `StopFileWatch`, Task 2 `ShutdownLspServersForRoot`.

- [ ] **Step 1: Failing tests** (replace the three `switch_workspace_*confirmation*` tests and keep the fixture)

```rust
#[test]
fn switch_workspace_with_dirty_buffer_parks_it_instead_of_prompting() {
    let (mut shell, file_path, target) = switch_guard_fixture("park");

    assert!(shell.switch_workspace_to(target.clone()));

    assert!(shell.pending_confirmation.is_none(), "switching never destroys, so it never asks");
    assert_eq!(shell.app_state.workspace_root_path(), Some(target.as_path()));
    assert_eq!(shell.background_sessions.len(), 1);
    assert!(shell.background_sessions[0].app_state.is_dirty(), "edit survives in the parked session");

    let _ = std::fs::remove_file(file_path);
    let _ = std::fs::remove_dir_all(target);
}

#[test]
fn switching_back_restores_the_parked_session() {
    let (mut shell, file_path, target) = switch_guard_fixture("back");
    let origin = shell.app_state.workspace_root_path().unwrap().to_path_buf();

    assert!(shell.switch_workspace_to(target.clone()));
    assert!(shell.switch_workspace_to(origin.clone()));

    assert_eq!(shell.app_state.workspace_root_path(), Some(origin.as_path()));
    assert_eq!(shell.app_state.active_file(), Some(file_path.as_path()));
    assert!(shell.app_state.is_dirty());
    assert_eq!(shell.background_sessions.len(), 1);
    assert_eq!(shell.background_sessions[0].root, target);

    let _ = std::fs::remove_file(file_path);
    let _ = std::fs::remove_dir_all(target);
}

#[test]
fn close_session_with_dirty_buffer_prompts_and_yes_saves() {
    let (mut shell, file_path, target) = switch_guard_fixture("close_yes");
    let origin = shell.app_state.workspace_root_path().unwrap().to_path_buf();
    assert!(shell.switch_workspace_to(target.clone()));
    assert!(shell.switch_workspace_to(origin.clone()));

    assert!(shell.close_session(origin.clone()));
    assert!(shell.pending_confirmation.is_some());
    assert!(shell.respond_to_pending_confirmation(true));

    assert_eq!(std::fs::read_to_string(&file_path).unwrap(), "!hello\n");
    assert_eq!(shell.app_state.workspace_root_path(), Some(target.as_path()), "falls back to the MRU session");
    assert!(shell.background_sessions.is_empty());

    let _ = std::fs::remove_file(file_path);
    let _ = std::fs::remove_dir_all(target);
}

#[test]
fn close_last_session_shows_welcome() {
    let mut shell = AppShell::new_for_tests().expect("shell");
    let root = shell.app_state.workspace_root_path().unwrap().to_path_buf();
    assert!(shell.close_session(root));
    assert!(shell.app_state.workspace_root_path().is_none());
    assert!(shell.app_state.initial_launch_welcome_active());
}
```
(`initial_launch_welcome_active` = whatever getter pairs with `set_initial_launch_welcome`; `grep -n "initial_launch_welcome" src/app/app_state/mod.rs`.)

- [ ] **Step 2: Run** `cargo test --lib switch_workspace close_session switching_back` → FAIL.
- [ ] **Step 3: Implement**

`workspace_session.rs`:
```rust
impl AppShell {
    pub(super) fn session_count(&self) -> usize {
        self.background_sessions.len() + usize::from(self.app_state.workspace_root_path().is_some())
    }

    /// Bring `root` to the front: reuse a parked session, or build a new one.
    /// Never tears anything down.
    pub(super) fn activate_session(&mut self, root: PathBuf, follow_files: Vec<PathBuf>) -> bool {
        let is_active = self.app_state.workspace_root_path().is_some_and(|r| path_matches(r, &root));
        if !is_active {
            let incoming = match self.session_index_for_root(&root) {
                Some(i) => self.background_sessions.remove(i),
                None => match self.new_session(root.clone()) {
                    Some(s) => s,
                    None => return false,
                },
            };
            if let Some(mut parked) = self.stash_active_session() {
                parked.last_active = Instant::now();
                self.background_sessions.push(parked);
            }
            self.restore_session(incoming);
            let _ = self.app_state.set_initial_launch_welcome(false);
            self.after_session_activated(&root);
        }
        for file in follow_files {
            if let Err(err) = self.app_state.open_file(file.clone()) {
                eprintln!("[AppShell] follow-file open skipped ({}): {err}", file.display());
            }
        }
        if self.app_state.active_file().is_some() {
            self.invalidate_highlights_and_parse_active_buffer();
            self.submit_lsp_did_open_for_active_file();
        }
        self.update_window_title();
        self.request_redraw();
        true
    }

    fn new_session(&mut self, root: PathBuf) -> Option<WorkspaceSession> {
        let mut app_state = self.fresh_app_state();
        if let Err(err) = app_state.attach_workspace(root.clone()) {
            eprintln!("[AppShell] attach_workspace failed: {err}");
            self.show_transient_toast_kind(format!("Cannot open {}: {err}", root.display()), ToastKind::Error);
            return None;
        }
        let mut panel_state = WorkbenchPanelState::from_ui_theme(&self.theme.ui);
        panel_state.left.visible = false;
        panel_state.right.visible = false;
        panel_state.bottom.visible = false;
        let icon = crate::app::persistence::AppPersistentState::infer_project_icon_source(&root);
        self.persistent_state.push_recent_with_icon(root.clone(), Some(icon));
        self.persistent_state.save();
        Some(WorkspaceSession {
            root,
            app_state,
            shell: ShellWorkspaceState::fresh(&self.theme),
            panel_state,
            pending_fs_events: Vec::new(),
            last_active: Instant::now(),
        })
    }

    /// Everything a freshly foregrounded session needs from the worker side.
    fn after_session_activated(&mut self, root: &Path) {
        self.submit(RequestSpec { revision_id: 0, topic: RequestTopic::WorkspaceWatch,
            payload: WorkerRequestPayload::StartFileWatch { root_path: root.to_path_buf(), recursive: true } });
        self.mark_explorer_dirty();
        self.workspace_git_branch = self.app_state.workspace_root_path().and_then(detect_git_branch);
        self.submit_workspace_git_status_refresh();
        self.submit_active_buffer_git_baseline_refresh();
        self.sync_lsp_server_for_workspace();
        self.drain_pending_fs_events_for_active();
        let (idx, total) = self.session_position(root);
        if total > 1 {
            self.show_transient_toast(format!("{} ({idx}/{total})", root.file_name().and_then(|n| n.to_str()).unwrap_or("workspace")));
        }
        self.dojo_after_workspace_switch(root);
    }

    /// Close `root`'s session: dirty guard, then kill its PTYs/LSP/watcher.
    pub(super) fn close_session(&mut self, root: PathBuf) -> bool {
        let is_active = self.app_state.workspace_root_path().is_some_and(|r| path_matches(r, &root));
        let dirty = if is_active { self.app_state.dirty_buffer_count() } else {
            self.session_index_for_root(&root).map(|i| self.background_sessions[i].app_state.dirty_buffer_count()).unwrap_or(0)
        };
        if dirty > 0 && self.pending_confirmation.is_none() {
            return self.begin_workspace_close_confirmation(root, dirty);
        }
        self.perform_session_close(root)
    }

    pub(super) fn perform_session_close(&mut self, root: PathBuf) -> bool {
        let is_active = self.app_state.workspace_root_path().is_some_and(|r| path_matches(r, &root));
        let session = if is_active {
            let Some(s) = self.stash_active_session() else { return false };
            s
        } else {
            let Some(i) = self.session_index_for_root(&root) else { return false };
            self.background_sessions.remove(i)
        };
        for session_id in session.pty_session_ids() {
            self.submit(RequestSpec { revision_id: 0, topic: RequestTopic::TerminalPty,
                payload: WorkerRequestPayload::ClosePtySession { session_id } });
        }
        self.submit(RequestSpec { revision_id: 0, topic: RequestTopic::LspClient,
            payload: WorkerRequestPayload::ShutdownLspServersForRoot { root_path: session.root.clone() } });
        self.submit(RequestSpec { revision_id: 0, topic: RequestTopic::WorkspaceWatch,
            payload: WorkerRequestPayload::StopFileWatch { root_path: session.root.clone() } });
        drop(session);
        if is_active {
            match self.take_mru_background_session() {
                Some(next) => {
                    let next_root = next.root.clone();
                    self.restore_session(next);
                    self.after_session_activated(&next_root);
                    if self.app_state.active_file().is_some() {
                        self.invalidate_highlights_and_parse_active_buffer();
                        self.submit_lsp_did_open_for_active_file();
                    }
                }
                None => {
                    let _ = self.app_state.set_initial_launch_welcome(true);
                }
            }
        }
        self.update_window_title();
        self.request_redraw();
        true
    }

    fn take_mru_background_session(&mut self) -> Option<WorkspaceSession> {
        let idx = self.background_sessions.iter().enumerate()
            .max_by_key(|(_, s)| s.last_active).map(|(i, _)| i)?;
        Some(self.background_sessions.remove(idx))
    }

    /// 1-based position of `root` in MRU order (active first) and the total.
    pub(super) fn session_position(&self, root: &Path) -> (usize, usize) {
        let total = self.session_count();
        if self.app_state.workspace_root_path().is_some_and(|r| path_matches(r, root)) {
            return (1, total);
        }
        let mut parked: Vec<&WorkspaceSession> = self.background_sessions.iter().collect();
        parked.sort_by_key(|s| std::cmp::Reverse(s.last_active));
        let pos = parked.iter().position(|s| path_matches(&s.root, root)).map(|p| p + 2).unwrap_or(total);
        (pos, total)
    }

    fn drain_pending_fs_events_for_active(&mut self) {
        // Filled by Task 5; keep as a no-op stub here so Task 4 compiles.
    }
}
```

`commands_explorer.rs`:
- Delete `reset_terminals_for_workspace_switch` and `prepare_for_workspace_switch` (and their `ShutdownAllLspServers` submit).
- `switch_workspace_with_files` body becomes `self.perform_workspace_switch(root_path, follow_files)`.
- `perform_workspace_switch` body becomes `self.activate_session(root_path, follow_files)`.
- `handle_remote_open` unchanged.

`mod.rs`: rename the variant:
```rust
    /// Session close requested while it has unsaved edits: y = save all
    /// first, n = discard and close, Esc = keep it open.
    WorkspaceClose { root: PathBuf, dirty_count: usize },
```
`commands_prompts.rs`: `begin_workspace_switch_confirmation` → `begin_workspace_close_confirmation(root, dirty_count)`; prompt text `"{name} has {n} unsaved file(s). y = save & close, n = discard & close (Esc = keep open)"`; the respond arm calls `save_all_dirty_buffers_for_quit()` on yes then `self.perform_session_close(root)`. Note: for a *parked* dirty session, `save_all_dirty_buffers_for_quit` acts on the active state — activate the session first inside `close_session` when it is parked (`self.activate_session(root.clone(), Vec::new())` before the dirty check). Add that line.

- [ ] **Step 4: Run** `cargo test --lib switch_workspace close_ remote_open dojo` → PASS; then `cargo test --lib` full.
- [ ] **Step 5: Report.**

---

### Task 5: Route worker results to parked sessions

**Files:**
- Modify: `src/app/event_loop/async_results/terminal.rs` (`PtyOutput`, `PtySpawned`, `PtySessionClosed`)
- Modify: `src/app/event_loop/async_results/filesystem.rs:57` (`FileSystemEvents` root check)
- Modify: `src/app/event_loop/async_results/lsp.rs:12-50` (`LspServerStarted` root check)
- Modify: `src/app/event_loop/workspace_session.rs` (`drain_pending_fs_events_for_active`, lookup helpers)
- Test: `src/app/event_loop/commands_tests.rs`

**Interfaces:**
- Produces: `AppShell::parked_session_owning_pty(&mut self, id: u64) -> Option<&mut WorkspaceSession>`,
  `AppShell::parked_session_for_root(&mut self, root: &Path) -> Option<&mut WorkspaceSession>`.

- [ ] **Step 1: Failing tests**

```rust
#[test]
fn pty_output_for_parked_session_feeds_its_grid_without_redraw() {
    use crate::app::async_bridge::AsyncResultRouter;
    use crate::async_runtime::message::{RequestTopic, WorkerResult, WorkerResultPayload};
    let mut shell = AppShell::new_for_tests().expect("shell");
    let origin = shell.app_state.workspace_root_path().unwrap().to_path_buf();
    shell.right_pty_session_id = Some(9);
    let target = std::env::temp_dir().join(format!("netherize_parked_pty_{}", std::process::id()));
    std::fs::create_dir_all(&target).unwrap();
    let target = target.canonicalize().unwrap();
    assert!(shell.switch_workspace_to(target.clone()));
    shell.terminal_needs_layout = false;
    shell.right_terminal_needs_layout = false;

    shell.on_worker_result(WorkerResult { request_id: 1, revision_id: 0, topic: RequestTopic::TerminalPty,
        payload: WorkerResultPayload::PtyOutput { session_id: 9, chunk: b"parked\r\n".to_vec() } });

    assert!(!shell.right_terminal_needs_layout, "parked output must not dirty the active layout");
    let parked = &shell.background_sessions[0];
    assert_eq!(parked.root, origin);
    assert!(parked.shell.right_terminal_grid.plain_text().contains("parked"));
    let _ = std::fs::remove_dir_all(target);
}

#[test]
fn fs_events_for_parked_session_are_queued_and_drained_on_restore() {
    use crate::app::async_bridge::AsyncResultRouter;
    use crate::async_runtime::message::{FileSystemChangeKind, FileSystemEvent, RequestTopic, WorkerResult, WorkerResultPayload};
    let mut shell = AppShell::new_for_tests().expect("shell");
    let origin = shell.app_state.workspace_root_path().unwrap().to_path_buf();
    let target = std::env::temp_dir().join(format!("netherize_parked_fs_{}", std::process::id()));
    std::fs::create_dir_all(&target).unwrap();
    let target = target.canonicalize().unwrap();
    assert!(shell.switch_workspace_to(target.clone()));

    shell.on_worker_result(WorkerResult { request_id: 1, revision_id: 0, topic: RequestTopic::WorkspaceWatch,
        payload: WorkerResultPayload::FileSystemEvents { root_path: origin.clone(),
            events: vec![FileSystemEvent { kind: FileSystemChangeKind::Created, path: origin.join("new.txt"), new_path: None }] } });

    assert_eq!(shell.background_sessions[0].pending_fs_events.len(), 1);
    assert!(shell.switch_workspace_to(origin));
    assert!(shell.background_sessions.iter().all(|s| s.pending_fs_events.is_empty()));
    assert!(shell.explorer_snapshot_dirty);
    let _ = std::fs::remove_dir_all(target);
}
```
(`plain_text()` = whatever `TerminalGrid` exposes for row text in the existing terminal tests — `grep -n "pub fn .*text\|fn row_text" src/terminal/grid.rs`; `FileSystemChangeKind` variant names from `message.rs:95-103`.)

- [ ] **Step 2: Run** → FAIL.
- [ ] **Step 3: Implement**

`workspace_session.rs`:
```rust
impl AppShell {
    pub(super) fn parked_session_owning_pty(&mut self, id: u64) -> Option<&mut WorkspaceSession> {
        self.background_sessions.iter_mut().find(|s| s.owns_pty_session(id))
    }
    pub(super) fn parked_session_for_root(&mut self, root: &Path) -> Option<&mut WorkspaceSession> {
        self.background_sessions.iter_mut().find(|s| path_matches(&s.root, root))
    }
    fn drain_pending_fs_events_for_active(&mut self) {
        // Events were queued on the session while parked; it is active now, so
        // its `pending_fs_events` were restored onto... nothing — they travel
        // with the session struct, which `restore_session` consumed. Hence
        // restore_session stashes them into `self.pending_fs_events_to_drain`.
        let events = std::mem::take(&mut self.pending_fs_events_to_drain);
        if events.is_empty() { return; }
        let root = self.app_state.workspace_root_path().map(PathBuf::from).unwrap_or_default();
        super::async_results::filesystem::handle_filesystem_result(self,
            WorkerResultPayload::FileSystemEvents { root_path: root, events });
    }
}
```
Add `pending_fs_events_to_drain: Vec<FileSystemEvent>` to `AppShell` (init `Vec::new()`); in `restore_session` set `self.pending_fs_events_to_drain = session.pending_fs_events`.

`terminal.rs` — at the top of the `PtyOutput` arm, before the active routing:
```rust
            if !app.owns_pty_session(session_id) {
                if let Some(parked) = app.parked_session_owning_pty(session_id) {
                    if let Some(tab) = parked.shell.terminal_tabs.iter_mut().find(|t| t.session_id == Some(session_id)) {
                        let rows = tab.grid.feed_bytes(&chunk);
                        tab.grid.apply_regex_highlights_incremental(rows);
                        tab.grid.view_scroll_to_bottom();
                    } else if parked.shell.right_pty_session_id == Some(session_id) {
                        let rows = parked.shell.right_terminal_grid.feed_bytes(&chunk);
                        parked.shell.right_terminal_grid.apply_regex_highlights_incremental(rows);
                        parked.shell.right_terminal_grid.view_scroll_to_bottom();
                    } else if let Some(grid) = parked.shell.terminal_buffer_grids.get_mut(&session_id) {
                        let rows = grid.feed_bytes(&chunk);
                        grid.apply_regex_highlights_incremental(rows);
                    }
                }
                return; // parked: no layout flags, no redraw
            }
```
`PtySessionClosed`: same guard; for a parked tab set `session_id = None`, `status = Exited(0)`, label `(dead)`; for a parked right PTY set `right_pty_session_id = None`. `PtySpawned`: if `!app.pending_right_pty_spawn && app.pending_lazygit_buffer_index.is_none() && app.pending_lazydocker_buffer_index.is_none() && !app.pending_terminal_tab_spawns.contains_key(&request_id)`, look for a parked session whose `shell.pending_terminal_tab_spawns` contains `request_id` (or `pending_right_pty_spawn`) and bind there (`tab.session_id = Some(session_id)`, submit `ResizePtySession` with that grid's cols/rows); otherwise fall through to the existing code.

`filesystem.rs:57`:
```rust
    if let WorkerResultPayload::FileSystemEvents { root_path, events } = payload {
        let for_active = app.app_state.workspace_root_path().is_some_and(|r| crate::app::app_state::path_matches(r, &root_path));
        if !for_active {
            if let Some(parked) = app.parked_session_for_root(&root_path) {
                parked.pending_fs_events.extend(events);
                parked.shell.explorer_snapshot_dirty = true;
            }
            return;
        }
        // …existing body…
```
Make `handle_filesystem_result` `pub(in crate::app::event_loop)`.

`lsp.rs:12`: after the companion check, if `root_path` does not match the active root:
```rust
            if !app.app_state.workspace_root_path().is_some_and(|r| crate::app::app_state::path_matches(r, &root_path)) {
                if let Some(parked) = app.parked_session_for_root(&root_path) {
                    let started = ActiveLspServer { server_name: server_name.clone(), root_path: root_path.clone() };
                    parked.shell.active_lsp_server = Some(started.clone());
                    parked.shell.lsp_completion_trigger_chars = completion_trigger_chars.clone();
                    if parked.shell.pending_lsp_server.as_ref() == Some(&started) { parked.shell.pending_lsp_server = None; }
                }
                return;
            }
```

- [ ] **Step 4: Run** `cargo test --lib parked_session pty_output fs_events` → PASS; full suite.
- [ ] **Step 5: Report.**

---

### Task 6: Commands, keys, switcher palette

**Files:**
- Modify: `src/core/commands.rs:197` (4 variants), `src/core/command_ids.rs:82,338,684` (ids + ALL_IDS + mapping), `src/core/command_dispatch/mod.rs:373`, `src/core/command_dispatch/session.rs:204` (add to the shell-handled lists)
- Modify: `src/app/command_palette.rs:1508-1512` (palette actions), `src/app/command_palette.rs:317-341` (`recent_project_with_meta` gains `live: Option<LiveSessionMeta>`)
- Modify: `config/keymaps/default.toml:1173-1181`
- Modify: `src/app/event_loop/commands_explorer.rs:385-400` (handlers), `src/app/event_loop/commands_prompts.rs:527-563,605-623` (switcher palette + confirm)
- Modify: `src/app/app_state/palette.rs:185-214` (`open_workspace_switcher_palette`)
- Modify: `src/render/renderer/palette/recent_projects.rs:459-480` (parse `live=`, `dirty=`, `branch=`; render `●`, dirty count, branch after the name)
- Test: `src/app/input_map/tests.rs` (keys), `src/app/event_loop/commands_tests.rs` (switcher order, next/prev), `src/render/renderer/palette/recent_projects.rs` tests (parser)

**Interfaces:**
- Produces: `Command::SwitchWorkspaceSession | NextWorkspaceSession | PrevWorkspaceSession | CloseWorkspaceSession`; ids `projects.switch`, `projects.next`, `projects.prev`, `projects.close`; keys `<leader>p p`, `<leader>p n`, `<leader>p b`, `<leader>p x`;
  `pub struct LiveSessionMeta { pub dirty: usize, pub branch: Option<String> }`;
  `AppState::open_workspace_switcher_palette(&mut self, live: &[(PathBuf, LiveSessionMeta)], recent: &[PathBuf], meta: &HashMap<PathBuf, RecentProjectMeta>)`;
  `AppShell::switcher_items(&self) -> (Vec<(PathBuf, LiveSessionMeta)>, Vec<PathBuf>)` (live MRU without current, recents minus live).

- [ ] **Step 1: Failing tests**

```rust
// commands_tests.rs
#[test]
fn switcher_lists_live_sessions_before_recents_and_hides_current() {
    let mut shell = AppShell::new_for_tests().expect("shell");
    let origin = shell.app_state.workspace_root_path().unwrap().to_path_buf();
    let a = std::env::temp_dir().join(format!("netherize_sw_a_{}", std::process::id()));
    let b = std::env::temp_dir().join(format!("netherize_sw_b_{}", std::process::id()));
    std::fs::create_dir_all(&a).unwrap(); std::fs::create_dir_all(&b).unwrap();
    let (a, b) = (a.canonicalize().unwrap(), b.canonicalize().unwrap());
    assert!(shell.switch_workspace_to(a.clone()));
    assert!(shell.switch_workspace_to(b.clone()));
    shell.persistent_state.push_recent(std::env::temp_dir().canonicalize().unwrap());

    let (live, recent) = shell.switcher_items();
    assert_eq!(live.iter().map(|(p, _)| p.clone()).collect::<Vec<_>>(), vec![a.clone(), origin.clone()], "MRU, current (b) excluded");
    assert!(!recent.contains(&a) && !recent.contains(&b) && !recent.contains(&origin), "live roots are not repeated under recents");

    assert!(shell.handle_command(Command::NextWorkspaceSession));
    assert_eq!(shell.app_state.workspace_root_path(), Some(a.as_path()));
    assert!(shell.handle_command(Command::PrevWorkspaceSession));
    assert_eq!(shell.app_state.workspace_root_path(), Some(b.as_path()));
    let _ = std::fs::remove_dir_all(a); let _ = std::fs::remove_dir_all(b);
}

// recent_projects.rs tests
#[test]
fn parse_recent_secondary_reads_live_session_markers() {
    let meta = parse_recent_secondary("/x\u{1f}icon=rust;live=1;dirty=2;branch=main");
    assert!(meta.live);
    assert_eq!(meta.dirty, 2);
    assert_eq!(meta.branch.as_deref(), Some("main"));
}
```
Plus an input-map test mirroring `tests.rs:1288-1300` for `<leader>p p` → `Command::SwitchWorkspaceSession`.

- [ ] **Step 2: Run** → FAIL (variants missing).
- [ ] **Step 3: Implement**

`commands.rs` (after `OpenWorktreePalette`):
```rust
    /// Palette of live workspace sessions (MRU) followed by recent projects.
    SwitchWorkspaceSession,
    /// Activate the most recently used parked session.
    NextWorkspaceSession,
    /// Activate the least recently used parked session (cycle backwards).
    PrevWorkspaceSession,
    /// Close the active session (dirty guard), fall back to the MRU one.
    CloseWorkspaceSession,
```
`command_ids.rs`: `SWITCH_WORKSPACE_SESSION = "projects.switch"`, `NEXT_WORKSPACE_SESSION = "projects.next"`, `PREV_WORKSPACE_SESSION = "projects.prev"`, `CLOSE_WORKSPACE_SESSION = "projects.close"` + `ALL_IDS` + `id → Command` mapping (and the reverse mapping if the file has one — `grep -n "Command::OpenWorktreePalette" src/core/command_ids.rs`).

`default.toml` after the `projects.worktrees` binding:
```toml
[[bindings]]
mode = "normal"
key = "<leader>p p"
command = "projects.switch"

[[bindings]]
mode = "normal"
key = "<leader>p n"
command = "projects.next"

[[bindings]]
mode = "normal"
key = "<leader>p b"
command = "projects.prev"

[[bindings]]
mode = "normal"
key = "<leader>p x"
command = "projects.close"
```
`COMMAND_PALETTE_ACTIONS`: `("projects.switch", "Switch Workspace Session")`, `("projects.next", "Next Workspace Session")`, `("projects.prev", "Previous Workspace Session")`, `("projects.close", "Close Workspace Session")`.

`commands_explorer.rs` handler arms:
```rust
            Command::SwitchWorkspaceSession => Some(self.open_workspace_switcher_palette()),
            Command::NextWorkspaceSession => Some(self.cycle_session(true)),
            Command::PrevWorkspaceSession => Some(self.cycle_session(false)),
            Command::CloseWorkspaceSession => {
                let Some(root) = self.app_state.workspace_root_path().map(PathBuf::from) else {
                    self.show_transient_toast("No workspace session to close".to_string());
                    return Some(false);
                };
                Some(self.close_session(root))
            }
```
`workspace_session.rs`:
```rust
    pub(super) fn cycle_session(&mut self, forward: bool) -> bool {
        if self.background_sessions.is_empty() {
            self.show_transient_toast("Only one workspace session".to_string());
            return false;
        }
        let pick = if forward {
            self.background_sessions.iter().max_by_key(|s| s.last_active)
        } else {
            self.background_sessions.iter().min_by_key(|s| s.last_active)
        };
        let root = pick.map(|s| s.root.clone()).expect("non-empty");
        self.activate_session(root, Vec::new())
    }

    pub(super) fn switcher_items(&self) -> (Vec<(PathBuf, LiveSessionMeta)>, Vec<PathBuf>) {
        let mut live: Vec<&WorkspaceSession> = self.background_sessions.iter().collect();
        live.sort_by_key(|s| std::cmp::Reverse(s.last_active));
        let live: Vec<(PathBuf, LiveSessionMeta)> = live.into_iter().map(|s| (s.root.clone(), LiveSessionMeta {
            dirty: s.app_state.dirty_buffer_count(),
            branch: s.shell.workspace_git_branch.clone(),
        })).collect();
        let mut hidden: Vec<&Path> = live.iter().map(|(p, _)| p.as_path()).collect();
        if let Some(active) = self.app_state.workspace_root_path() { hidden.push(active); }
        let recent = self.persistent_state.recent_projects.iter()
            .filter(|p| !hidden.iter().any(|h| path_matches(h, p))).cloned().collect();
        (live, recent)
    }
```
`commands_prompts.rs::open_workspace_switcher_palette` = copy of `open_recent_projects_palette` that calls `open_workspace_switcher_palette(&live, &recent, &meta)` and then `self.app_state.command_palette_mut().set_title_override(Some("WORKSPACES".into()))` (use whatever accessor `open_worktree_palette` uses). Empty live + empty recent → toast "No other workspaces. Use Ctrl+O to open a folder." `confirm_recent_project_selection` stays (it calls `switch_workspace_to`, which now activates sessions).

`palette.rs` (`app_state`): `open_workspace_switcher_palette` builds live items with `CommandPaletteItem::recent_project_with_meta(path, Some(icon), None, Some(&meta))` then recent items as today, and `open_with_items(CommandPaletteMode::RecentProjects, items)`. Extend `recent_project_with_meta` with a 4th param `live: Option<&LiveSessionMeta>` appending `;live=1;dirty={n}` and `;branch={b}` when present; update the existing callers to pass `None`.

`recent_projects.rs`: `RecentProjectRenderMeta` gains `live: bool, dirty: usize, branch: Option<String>`; parse `live=`, `dirty=`, `branch=`; where the row label is drawn, prefix `"● "` when live and append `"  {branch}"` (dim) and `"  ●{dirty}"` (modified tone) when dirty > 0. Follow the existing glyph/color calls in that function.

- [ ] **Step 4: Run** `cargo test --lib switcher recent_secondary leader_p command_ids palette` → PASS; full suite.
- [ ] **Step 5: Report.**

---

### Task 7: Verification, docs, GUI checklist

**Files:**
- Modify: `docs/project-knowledge/lessons.md` (one entry: watcher leak + sessions)
- Modify: memory `project_single_instance_workspace.md` (update)

- [ ] **Step 1:** `cargo test --lib` → all green (expect 1343 + new). `cargo clippy --lib -- -D warnings` clean (or no new warnings vs. baseline).
- [ ] **Step 2:** `gitnexus_detect_changes()` → confirm only `Event_loop`, `Scheduler`, `core::commands*`, `render::palette` symbols changed.
- [ ] **Step 3:** `scripts/bundle_macos.sh` so `/Applications/Netherize.app` carries the build.
- [ ] **Step 4:** Report with the GUI checklist from spec §6.
