//! Per-window lifecycle hooks the multi-window driver calls: identity,
//! which repos this window hosts, new-window requests and teardown.
//! Spec: docs/superpowers/specs/2026-09-04-multi-window-design.md

use super::*;

/// After the teardown requests are submitted, keep the shell (and its worker
/// runtime) alive this long so the dispatch loop can deliver them.
pub(super) const TEARDOWN_GRACE: Duration = Duration::from_millis(500);
/// Upper bound for the runtime to join its blocking threads on shutdown.
const RUNTIME_SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(2);

impl AppShell {
    pub(super) fn window_id(&self) -> Option<WindowId> {
        self.window.as_ref().map(|window| window.id())
    }

    /// How this window shows up in the other windows' switcher.
    pub(super) fn window_summary(&self) -> Option<WindowSummary> {
        Some(WindowSummary {
            id: self.window_id()?,
            root: self.app_state.workspace_root_path().map(PathBuf::from),
            dirty: self.app_state.dirty_buffer_count(),
            branch: self.workspace_git_branch.clone(),
        })
    }

    /// Active root first, then parked ones.
    pub(super) fn live_roots(&self) -> Vec<PathBuf> {
        self.app_state
            .workspace_root_path()
            .map(PathBuf::from)
            .into_iter()
            .chain(self.background_sessions.iter().map(|s| s.root.clone()))
            .collect()
    }

    /// `app.new_window` (Cmd+Shift+N): an empty window with the Welcome
    /// screen, like VS Code's New Window. Layouts are persisted first so a
    /// repo picked from its recents comes back with the same tabs.
    pub(super) fn request_new_window(&mut self) -> bool {
        self.persist_session_layouts(true);
        self.pending_new_windows.push(NewWindowRequest::Welcome);
        true
    }

    pub(super) fn take_pending_new_windows(&mut self) -> Vec<NewWindowRequest> {
        std::mem::take(&mut self.pending_new_windows)
    }

    /// Start closing this window: hide it, tell the worker to close every
    /// PTY, LSP server and watcher it owns. Layouts stay persisted so the
    /// roots come back with their tabs when opened again.
    pub(super) fn begin_teardown(&mut self) {
        if self.closing_since.is_some() {
            return;
        }
        self.closing_since = Some(Instant::now());
        if let Some(window) = self.window.as_ref() {
            window.set_visible(false);
        }
        let mut pty_ids: Vec<u64> = self
            .terminal_tabs
            .iter()
            .filter_map(|t| t.session_id)
            .chain(self.right_pty_session_id)
            .chain(self.terminal_buffer_grids.keys().copied())
            .collect();
        for session in &self.background_sessions {
            pty_ids.extend(session.pty_session_ids());
        }
        for session_id in pty_ids {
            self.submit(RequestSpec {
                revision_id: 0,
                topic: RequestTopic::TerminalPty,
                payload: WorkerRequestPayload::ClosePtySession { session_id },
            });
        }
        for root in self.live_roots() {
            self.submit(RequestSpec {
                revision_id: 0,
                topic: RequestTopic::LspClient,
                payload: WorkerRequestPayload::ShutdownLspServersForRoot {
                    root_path: root.clone(),
                },
            });
            self.submit(RequestSpec {
                revision_id: 0,
                topic: RequestTopic::WorkspaceWatch,
                payload: WorkerRequestPayload::StopFileWatch {
                    root_path: root.clone(),
                },
            });
        }
    }

    pub(super) fn teardown_due(&self) -> bool {
        self.closing_since
            .is_some_and(|since| since.elapsed() >= TEARDOWN_GRACE)
    }

    /// Wake deadline while closing, so the driver reaps on time.
    pub(super) fn teardown_deadline(&self) -> Option<Instant> {
        self.closing_since.map(|since| since + TEARDOWN_GRACE)
    }

    /// Drop the window and renderer, then stop the worker runtime.
    pub(super) fn finish_teardown(self) {
        let AppShell {
            scheduler,
            window,
            renderer,
            ..
        } = self;
        drop(renderer);
        drop(window);
        scheduler.shutdown(RUNTIME_SHUTDOWN_TIMEOUT);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_window_command_queues_a_welcome_window_and_persists_layout() {
        let mut shell = AppShell::new_for_tests().expect("shell");
        let root = shell
            .app_state
            .workspace_root_path()
            .expect("root")
            .to_path_buf();
        shell.persistent_state.push_recent(root.clone());

        assert!(shell.handle_command(Command::NewWindow));

        assert_eq!(
            shell.take_pending_new_windows(),
            vec![NewWindowRequest::Welcome]
        );
        assert!(shell.take_pending_new_windows().is_empty(), "taken once");
        assert!(
            shell.persistent_state.session_layouts.contains_key(&root),
            "layout persisted so a repo picked from recents restores its tabs"
        );
    }

    #[test]
    fn welcome_window_attaches_no_workspace() {
        let (scheduler, rx) = crate::async_runtime::scheduler::AsyncScheduler::new_for_tests()
            .expect("scheduler");
        let shell = AppShell::new_with_scheduler(
            scheduler,
            rx,
            NewWindowRequest::Welcome,
            AppPersistentState::default(),
        )
        .expect("shell");
        assert!(shell.app_state.workspace_root_path().is_none());
        assert!(shell.app_state.is_initial_launch_welcome());
        assert!(shell.app_state.buffers().is_empty());
    }

    #[test]
    fn live_roots_lists_active_then_parked_sessions() {
        let mut shell = AppShell::new_for_tests().expect("shell");
        let origin = shell
            .app_state
            .workspace_root_path()
            .expect("root")
            .to_path_buf();
        let target =
            std::env::temp_dir().join(format!("netherize_hosts_{}", std::process::id()));
        std::fs::create_dir_all(&target).expect("dir");
        let target = target.canonicalize().expect("canonicalize");
        let stranger =
            std::env::temp_dir().join(format!("netherize_hosts_x_{}", std::process::id()));

        assert!(shell.switch_workspace_to(target.clone()));

        let roots = shell.live_roots();
        assert_eq!(roots, vec![target.clone(), origin], "active first, then parked");
        assert!(!roots.contains(&stranger));

        let _ = std::fs::remove_dir_all(target);
    }

    #[test]
    fn teardown_marks_closing_and_becomes_due_after_grace() {
        let mut shell = AppShell::new_for_tests().expect("shell");
        assert!(shell.teardown_deadline().is_none());

        shell.begin_teardown();

        assert!(shell.closing_since.is_some());
        assert!(!shell.teardown_due(), "grace period not over yet");
        shell.closing_since = Some(Instant::now() - TEARDOWN_GRACE);
        assert!(shell.teardown_due());
        shell.finish_teardown();
    }
}
