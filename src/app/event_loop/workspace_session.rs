//! One open repo = one `WorkspaceSession`. The active session's fields live
//! directly on `AppShell` (no rename churn); parked sessions are stashed here
//! so switching is a field swap, not a teardown — terminals keep running, LSP
//! servers stay up, buffers and jump stacks survive.
//! Spec: docs/superpowers/specs/2026-09-04-workspace-sessions-design.md

use super::*;
use crate::app::app_state::path_matches;
use crate::async_runtime::message::FileSystemEvent;

/// The AppShell fields that belong to one workspace. `swap_shell_workspace_state`
/// is the single place that lists them; the round-trip test guards it.
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
    pub pending_right_pty_spawn: bool,
    pub right_agent_label: Option<String>,
    pub right_pty_startup_command: Option<String>,
    pub right_terminal_wheel_accum: f64,
    // terminal buffers (lazygit / lazydocker tabs)
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
    pub active_lsp_server: Option<ActiveLspServer>,
    pub pending_lsp_server: Option<ActiveLspServer>,
    pub pending_lsp_document_sync: Option<PendingLspDocumentSync>,
    pub lsp_completion_trigger_chars: Vec<char>,
    pub active_lsp_guide: Option<LspInstallGuide>,
    // highlight / symbols
    pub highlight_spans: Vec<HighlightSpan>,
    pub semantic_highlight_spans: Vec<HighlightSpan>,
    pub cached_document_symbols: Vec<crate::async_runtime::message::LspDocumentSymbol>,
    pub cached_document_symbols_path: Option<PathBuf>,
    pub outline_fetch_path: Option<PathBuf>,
    pub outline_selected: Option<usize>,
    pub syntax_engine: Option<SyntaxEngine>,
    pub syntax_engine_file: Option<PathBuf>,
    pub last_syntax_edit_hint: Option<SyntaxEditHint>,
}

fn fresh_grid(theme: &ThemeConfig) -> TerminalGrid {
    let mut g = TerminalGrid::new(120, 40);
    g.highlight_colors = HighlightColors::from_theme(theme);
    g
}

impl ShellWorkspaceState {
    /// Defaults for a brand-new workspace — mirrors `AppShell::new`.
    pub(super) fn fresh(theme: &ThemeConfig) -> Self {
        Self {
            terminal_tabs: vec![TerminalTab::new(fresh_grid(theme), "bash".to_string())],
            active_terminal_tab: 0,
            pending_terminal_tab_spawns: HashMap::new(),
            ignored_terminal_tab_spawns: HashSet::new(),
            bottom_terminal_wheel_accum: 0.0,
            right_pty_session_id: None,
            right_terminal_grid: fresh_grid(theme),
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

/// A parked workspace: everything needed to bring it back exactly as left.
pub(super) struct WorkspaceSession {
    pub root: PathBuf,
    pub app_state: AppState,
    pub shell: ShellWorkspaceState,
    pub panel_state: WorkbenchPanelState,
    /// File-system events that arrived while parked; replayed on restore.
    pub pending_fs_events: Vec<FileSystemEvent>,
    pub last_active: Instant,
}

impl WorkspaceSession {
    pub(super) fn pty_session_ids(&self) -> Vec<u64> {
        self.shell
            .terminal_tabs
            .iter()
            .filter_map(|t| t.session_id)
            .chain(self.shell.right_pty_session_id)
            .chain(self.shell.terminal_buffer_grids.keys().copied())
            .collect()
    }

    pub(super) fn owns_pty_session(&self, id: u64) -> bool {
        self.pty_session_ids().contains(&id)
    }
}

pub(super) fn workspace_display_name(root: &Path) -> &str {
    root.file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("workspace")
}

impl AppShell {
    /// Move the active workspace out of the shell, leaving fresh defaults in
    /// place. `None` when no workspace is attached (welcome screen).
    pub(super) fn stash_active_session(&mut self) -> Option<WorkspaceSession> {
        let root = self.app_state.workspace_root_path()?.to_path_buf();
        let fresh_state = self.fresh_app_state();
        let app_state = std::mem::replace(&mut self.app_state, fresh_state);
        let shell = self.swap_shell_workspace_state(ShellWorkspaceState::fresh(&self.theme));
        let mut parked_panels = WorkbenchPanelState::from_ui_theme(&self.theme.ui);
        parked_panels.left.visible = false;
        parked_panels.right.visible = false;
        parked_panels.bottom.visible = false;
        let panel_state = std::mem::replace(&mut self.panel_state, parked_panels);
        self.reset_in_flight_requests();
        self.mark_all_layout_dirty();
        Some(WorkspaceSession {
            root,
            app_state,
            shell,
            panel_state,
            pending_fs_events: Vec::new(),
            last_active: Instant::now(),
        })
    }

    /// Put a parked session back into the shell. Caller handles the worker
    /// side (watcher, LSP sync, git refresh) via `after_session_activated`.
    pub(super) fn restore_session(&mut self, session: WorkspaceSession) {
        let WorkspaceSession {
            app_state,
            shell,
            panel_state,
            pending_fs_events,
            ..
        } = session;
        self.app_state = app_state;
        let _ = self.swap_shell_workspace_state(shell);
        self.panel_state = panel_state;
        let _ = self
            .app_state
            .set_terminal_panel_open(self.panel_state.bottom.visible);
        self.pending_fs_events_to_drain = pending_fs_events;
        self.reset_in_flight_requests();
        self.mark_all_layout_dirty();
    }

    pub(super) fn session_index_for_root(&self, root: &Path) -> Option<usize> {
        self.background_sessions
            .iter()
            .position(|s| path_matches(&s.root, root))
    }

    /// True when the active shell owns PTY `id` (bottom tab, right dock or a
    /// terminal buffer).
    pub(super) fn owns_pty_session(&self, id: u64) -> bool {
        self.terminal_tabs.iter().any(|t| t.session_id == Some(id))
            || self.right_pty_session_id == Some(id)
            || self.terminal_buffer_grids.contains_key(&id)
    }

    pub(super) fn parked_session_owning_pty(&mut self, id: u64) -> Option<&mut WorkspaceSession> {
        self.background_sessions
            .iter_mut()
            .find(|s| s.owns_pty_session(id))
    }

    pub(super) fn parked_session_for_root(&mut self, root: &Path) -> Option<&mut WorkspaceSession> {
        self.background_sessions
            .iter_mut()
            .find(|s| path_matches(&s.root, root))
    }

    /// Parked session waiting on PTY spawn `request_id` (bottom tab, right
    /// dock, or an abandoned spawn it wants closed).
    pub(super) fn parked_session_expecting_spawn(
        &mut self,
        request_id: u64,
    ) -> Option<&mut WorkspaceSession> {
        self.background_sessions.iter_mut().find(|s| {
            s.shell.pending_terminal_tab_spawns.contains_key(&request_id)
                || s.shell.ignored_terminal_tab_spawns.contains(&request_id)
                || s.shell.pending_right_pty_spawn
        })
    }

    pub(super) fn fresh_app_state(&self) -> AppState {
        let mut state = AppState::new(self.app_state.default_save_path().to_path_buf());
        let _ = state.apply_mode_event(ModeEvent::EnterNormal);
        state.set_indent_config(self.ui_config.indent);
        state
    }

    /// Swap every per-workspace field with `incoming`, returning the previous
    /// values. ONE place lists the fields.
    fn swap_shell_workspace_state(&mut self, incoming: ShellWorkspaceState) -> ShellWorkspaceState {
        use std::mem::replace;
        let ShellWorkspaceState {
            terminal_tabs,
            active_terminal_tab,
            pending_terminal_tab_spawns,
            ignored_terminal_tab_spawns,
            bottom_terminal_wheel_accum,
            right_pty_session_id,
            right_terminal_grid,
            pending_right_pty_spawn,
            right_agent_label,
            right_pty_startup_command,
            right_terminal_wheel_accum,
            terminal_buffer_grids,
            pending_lazygit_buffer_index,
            pending_lazydocker_buffer_index,
            explorer_cursor,
            explorer_snapshot,
            explorer_snapshot_dirty,
            explorer_clipboard_path,
            pending_paste_source_path,
            pending_paste_target_dir,
            workspace_git_branch,
            active_lsp_server,
            pending_lsp_server,
            pending_lsp_document_sync,
            lsp_completion_trigger_chars,
            active_lsp_guide,
            highlight_spans,
            semantic_highlight_spans,
            cached_document_symbols,
            cached_document_symbols_path,
            outline_fetch_path,
            outline_selected,
            syntax_engine,
            syntax_engine_file,
            last_syntax_edit_hint,
        } = incoming;
        ShellWorkspaceState {
            terminal_tabs: replace(&mut self.terminal_tabs, terminal_tabs),
            active_terminal_tab: replace(&mut self.active_terminal_tab, active_terminal_tab),
            pending_terminal_tab_spawns: replace(
                &mut self.pending_terminal_tab_spawns,
                pending_terminal_tab_spawns,
            ),
            ignored_terminal_tab_spawns: replace(
                &mut self.ignored_terminal_tab_spawns,
                ignored_terminal_tab_spawns,
            ),
            bottom_terminal_wheel_accum: replace(
                &mut self.bottom_terminal_wheel_accum,
                bottom_terminal_wheel_accum,
            ),
            right_pty_session_id: replace(&mut self.right_pty_session_id, right_pty_session_id),
            right_terminal_grid: replace(&mut self.right_terminal_grid, right_terminal_grid),
            pending_right_pty_spawn: replace(
                &mut self.pending_right_pty_spawn,
                pending_right_pty_spawn,
            ),
            right_agent_label: replace(&mut self.right_agent_label, right_agent_label),
            right_pty_startup_command: replace(
                &mut self.right_pty_startup_command,
                right_pty_startup_command,
            ),
            right_terminal_wheel_accum: replace(
                &mut self.right_terminal_wheel_accum,
                right_terminal_wheel_accum,
            ),
            terminal_buffer_grids: replace(&mut self.terminal_buffer_grids, terminal_buffer_grids),
            pending_lazygit_buffer_index: replace(
                &mut self.pending_lazygit_buffer_index,
                pending_lazygit_buffer_index,
            ),
            pending_lazydocker_buffer_index: replace(
                &mut self.pending_lazydocker_buffer_index,
                pending_lazydocker_buffer_index,
            ),
            explorer_cursor: replace(&mut self.explorer_cursor, explorer_cursor),
            explorer_snapshot: replace(&mut self.explorer_snapshot, explorer_snapshot),
            explorer_snapshot_dirty: replace(
                &mut self.explorer_snapshot_dirty,
                explorer_snapshot_dirty,
            ),
            explorer_clipboard_path: replace(
                &mut self.explorer_clipboard_path,
                explorer_clipboard_path,
            ),
            pending_paste_source_path: replace(
                &mut self.pending_paste_source_path,
                pending_paste_source_path,
            ),
            pending_paste_target_dir: replace(
                &mut self.pending_paste_target_dir,
                pending_paste_target_dir,
            ),
            workspace_git_branch: replace(&mut self.workspace_git_branch, workspace_git_branch),
            active_lsp_server: replace(&mut self.active_lsp_server, active_lsp_server),
            pending_lsp_server: replace(&mut self.pending_lsp_server, pending_lsp_server),
            pending_lsp_document_sync: replace(
                &mut self.pending_lsp_document_sync,
                pending_lsp_document_sync,
            ),
            lsp_completion_trigger_chars: replace(
                &mut self.lsp_completion_trigger_chars,
                lsp_completion_trigger_chars,
            ),
            active_lsp_guide: replace(&mut self.active_lsp_guide, active_lsp_guide),
            highlight_spans: replace(&mut self.highlight_spans, highlight_spans),
            semantic_highlight_spans: replace(
                &mut self.semantic_highlight_spans,
                semantic_highlight_spans,
            ),
            cached_document_symbols: replace(
                &mut self.cached_document_symbols,
                cached_document_symbols,
            ),
            cached_document_symbols_path: replace(
                &mut self.cached_document_symbols_path,
                cached_document_symbols_path,
            ),
            outline_fetch_path: replace(&mut self.outline_fetch_path, outline_fetch_path),
            outline_selected: replace(&mut self.outline_selected, outline_selected),
            syntax_engine: replace(&mut self.syntax_engine, syntax_engine),
            syntax_engine_file: replace(&mut self.syntax_engine_file, syntax_engine_file),
            last_syntax_edit_hint: replace(&mut self.last_syntax_edit_hint, last_syntax_edit_hint),
        }
    }

    /// In-flight request ids belong to the workspace that issued them; a late
    /// result is dropped by mismatch once these are cleared.
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

    /// The renderer's uploaded terminal glyphs belong to the old grids; drop
    /// them and force every region to lay out again from the new session.
    fn mark_all_layout_dirty(&mut self) {
        self.editor_needs_layout = true;
        self.editor_caret_needs_layout = true;
        self.sidebar_needs_layout = true;
        self.terminal_needs_layout = true;
        self.buffer_terminal_needs_layout = true;
        self.right_terminal_needs_layout = true;
        self.last_terminal_bounds = None;
        self.last_buffer_terminal_bounds = None;
        self.last_right_terminal_bounds = None;
        self.last_window_title = None;
        if let Some(renderer) = self.renderer.as_mut() {
            renderer.clear_terminal();
            renderer.clear_buffer_terminal();
            renderer.clear_right_terminal();
        }
    }

    /// Live sessions: parked ones plus the active one when it has a root.
    pub(super) fn session_count(&self) -> usize {
        self.background_sessions.len()
            + usize::from(self.app_state.workspace_root_path().is_some())
    }

    pub(super) fn root_is_active(&self, root: &Path) -> bool {
        self.app_state
            .workspace_root_path()
            .is_some_and(|r| path_matches(r, root))
    }

    /// Bring `root` to the front: reuse a parked session or build a new one.
    /// Never tears anything down.
    pub(super) fn activate_session(&mut self, root: PathBuf, follow_files: Vec<PathBuf>) -> bool {
        if !self.root_is_active(&root) {
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
                eprintln!(
                    "[AppShell] follow-file open skipped ({}): {err}",
                    file.display()
                );
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
            self.show_transient_toast_kind(
                format!("Cannot open {}: {err}", root.display()),
                ToastKind::Error,
            );
            return None;
        }
        let mut panel_state = WorkbenchPanelState::from_ui_theme(&self.theme.ui);
        panel_state.left.visible = false;
        panel_state.right.visible = false;
        panel_state.bottom.visible = false;
        let icon = crate::app::persistence::AppPersistentState::infer_project_icon_source(&root);
        self.persistent_state
            .push_recent_with_icon(root.clone(), Some(icon));
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
        // Idempotent: the dispatch loop keeps one watcher per root.
        self.submit(RequestSpec {
            revision_id: 0,
            topic: RequestTopic::WorkspaceWatch,
            payload: WorkerRequestPayload::StartFileWatch {
                root_path: root.to_path_buf(),
                recursive: true,
            },
        });
        self.mark_explorer_dirty();
        self.workspace_git_branch = self
            .app_state
            .workspace_root_path()
            .and_then(detect_git_branch);
        self.submit_workspace_git_status_refresh();
        self.submit_active_buffer_git_baseline_refresh();
        // No-op when the restored `active_lsp_server` already matches.
        self.sync_lsp_server_for_workspace();
        self.drain_pending_fs_events_for_active();
        let (idx, total) = self.session_position(root);
        if total > 1 {
            self.show_transient_toast(format!(
                "{} ({idx}/{total})",
                workspace_display_name(root)
            ));
        }
        self.dojo_after_workspace_switch(root);
    }

    /// Close `root`'s session: dirty guard first, then kill its PTYs, its
    /// LSP servers and its watcher. A parked dirty session is activated
    /// first so the prompt (and save-all) acts on the right buffers.
    pub(super) fn close_session(&mut self, root: PathBuf) -> bool {
        if !self.root_is_active(&root) {
            if self.session_index_for_root(&root).is_none() {
                return false;
            }
            let dirty = self
                .session_index_for_root(&root)
                .map(|i| self.background_sessions[i].app_state.dirty_buffer_count())
                .unwrap_or(0);
            if dirty > 0 {
                self.activate_session(root.clone(), Vec::new());
            }
        }
        let dirty = if self.root_is_active(&root) {
            self.app_state.dirty_buffer_count()
        } else {
            0
        };
        if dirty > 0 && self.pending_confirmation.is_none() {
            return self.begin_workspace_close_confirmation(root, dirty);
        }
        self.perform_session_close(root)
    }

    pub(super) fn perform_session_close(&mut self, root: PathBuf) -> bool {
        let was_active = self.root_is_active(&root);
        let session = if was_active {
            let Some(s) = self.stash_active_session() else {
                return false;
            };
            s
        } else {
            let Some(i) = self.session_index_for_root(&root) else {
                return false;
            };
            self.background_sessions.remove(i)
        };
        for session_id in session.pty_session_ids() {
            self.submit(RequestSpec {
                revision_id: 0,
                topic: RequestTopic::TerminalPty,
                payload: WorkerRequestPayload::ClosePtySession { session_id },
            });
        }
        self.submit(RequestSpec {
            revision_id: 0,
            topic: RequestTopic::LspClient,
            payload: WorkerRequestPayload::ShutdownLspServersForRoot {
                root_path: session.root.clone(),
            },
        });
        self.submit(RequestSpec {
            revision_id: 0,
            topic: RequestTopic::WorkspaceWatch,
            payload: WorkerRequestPayload::StopFileWatch {
                root_path: session.root.clone(),
            },
        });
        drop(session);
        if was_active {
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
        let idx = self
            .background_sessions
            .iter()
            .enumerate()
            .max_by_key(|(_, s)| s.last_active)
            .map(|(i, _)| i)?;
        Some(self.background_sessions.remove(idx))
    }

    /// 1-based position of `root` in MRU order (active first) and the total.
    pub(super) fn session_position(&self, root: &Path) -> (usize, usize) {
        let total = self.session_count();
        if self.root_is_active(root) {
            return (1, total);
        }
        let mut parked: Vec<&WorkspaceSession> = self.background_sessions.iter().collect();
        parked.sort_by_key(|s| std::cmp::Reverse(s.last_active));
        let pos = parked
            .iter()
            .position(|s| path_matches(&s.root, root))
            .map(|p| p + 2)
            .unwrap_or(total);
        (pos, total)
    }

    /// `<leader>p n` / `<leader>p b`: MRU forward / backward through parked
    /// sessions.
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
        let Some(root) = pick.map(|s| s.root.clone()) else {
            return false;
        };
        self.activate_session(root, Vec::new())
    }

    /// Switcher rows: parked sessions in MRU order, then recents that are not
    /// already live (the active root is hidden from both).
    pub(super) fn switcher_items(
        &self,
    ) -> (
        Vec<(PathBuf, crate::app::command_palette::LiveSessionMeta)>,
        Vec<PathBuf>,
    ) {
        let mut parked: Vec<&WorkspaceSession> = self.background_sessions.iter().collect();
        parked.sort_by_key(|s| std::cmp::Reverse(s.last_active));
        let live: Vec<(PathBuf, crate::app::command_palette::LiveSessionMeta)> = parked
            .into_iter()
            .map(|s| {
                (
                    s.root.clone(),
                    crate::app::command_palette::LiveSessionMeta {
                        dirty: s.app_state.dirty_buffer_count(),
                        branch: s.shell.workspace_git_branch.clone(),
                    },
                )
            })
            .collect();
        let mut hidden: Vec<&Path> = live.iter().map(|(p, _)| p.as_path()).collect();
        if let Some(active) = self.app_state.workspace_root_path() {
            hidden.push(active);
        }
        let recent = self
            .persistent_state
            .recent_projects
            .iter()
            .filter(|p| !hidden.iter().any(|h| path_matches(h, p)))
            .cloned()
            .collect();
        (live, recent)
    }

    /// Replay file-system events that arrived while the session was parked.
    fn drain_pending_fs_events_for_active(&mut self) {
        let events = std::mem::take(&mut self.pending_fs_events_to_drain);
        if events.is_empty() {
            return;
        }
        let root_path = self
            .app_state
            .workspace_root_path()
            .map(PathBuf::from)
            .unwrap_or_default();
        super::async_results::filesystem::handle_filesystem_result(
            self,
            crate::async_runtime::message::WorkerResultPayload::FileSystemEvents {
                root_path,
                events,
            },
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stash_restore_round_trip_moves_every_workspace_field() {
        let mut shell = AppShell::new_for_tests().expect("shell");
        let root = shell
            .app_state
            .workspace_root_path()
            .expect("root")
            .to_path_buf();
        shell.right_pty_session_id = Some(41);
        shell.terminal_tabs[0].session_id = Some(42);
        shell
            .terminal_buffer_grids
            .insert(43, TerminalGrid::new(3, 3));
        shell.explorer_cursor = 7;
        shell.explorer_clipboard_path = Some(root.join("x"));
        shell.workspace_git_branch = Some("feature".into());
        shell.lsp_completion_trigger_chars = vec!['.'];
        shell.cached_document_symbols_path = Some(root.join("y.rs"));
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
        assert_eq!(shell.cached_document_symbols_path, None);
        assert_eq!(shell.outline_selected, None);
        assert!(!shell.panel_state.bottom.visible);
        assert!(session.owns_pty_session(41));
        assert!(session.owns_pty_session(42));
        assert!(session.owns_pty_session(43));
        assert_eq!(session.root, root);

        shell.restore_session(session);

        assert_eq!(shell.app_state.workspace_root_path(), Some(root.as_path()));
        assert_eq!(shell.right_pty_session_id, Some(41));
        assert_eq!(shell.terminal_tabs[0].session_id, Some(42));
        assert!(shell.terminal_buffer_grids.contains_key(&43));
        assert_eq!(shell.explorer_cursor, 7);
        assert_eq!(shell.workspace_git_branch.as_deref(), Some("feature"));
        assert_eq!(shell.lsp_completion_trigger_chars, vec!['.']);
        assert_eq!(
            shell.cached_document_symbols_path.as_deref(),
            Some(root.join("y.rs").as_path())
        );
        assert_eq!(shell.outline_selected, Some(3));
        assert!(shell.panel_state.bottom.visible);
        assert!(shell.editor_needs_layout);
        assert!(shell.sidebar_needs_layout);
        assert!(shell.terminal_needs_layout);
        assert!(shell.background_sessions.is_empty());
    }
}
