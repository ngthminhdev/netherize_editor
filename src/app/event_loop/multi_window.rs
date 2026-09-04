//! The winit `ApplicationHandler`: one `AppShell` per window. Each shell owns
//! its window, renderer, worker runtime and sessions; this driver only routes
//! events, spawns and reaps windows, and keeps the single copy of the
//! persisted state. Spec: docs/superpowers/specs/2026-09-04-multi-window-design.md

use super::*;

/// Where a remote `netherize <paths>` request goes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum RemoteRoute {
    /// A window already hosts the directory: forward (focus + activate).
    Existing(usize),
    /// Nobody hosts it: open a new window with these CLI paths.
    NewWindow(Vec<PathBuf>),
    /// Files only (no directory): the focused window, else the last one.
    Focused(usize),
    /// No window at all (should not happen while the loop runs).
    None,
}

/// Pure routing so it is testable without windows. `hosted[i]` = roots
/// window `i` hosts; `focused` = index of the focused window.
pub(super) fn route_remote_open(
    hosted: &[Vec<PathBuf>],
    focused: Option<usize>,
    paths: &[PathBuf],
) -> RemoteRoute {
    use crate::app::app_state::path_matches;
    if hosted.is_empty() {
        return RemoteRoute::None;
    }
    let dir = paths.iter().find(|p| p.is_dir());
    match dir {
        Some(dir) => {
            if let Some(idx) = hosted
                .iter()
                .position(|roots| roots.iter().any(|r| path_matches(r, dir)))
            {
                RemoteRoute::Existing(idx)
            } else {
                RemoteRoute::NewWindow(paths.to_vec())
            }
        }
        None => RemoteRoute::Focused(
            focused
                .filter(|i| *i < hosted.len())
                .unwrap_or(hosted.len() - 1),
        ),
    }
}

/// Every window except `me` (pure, for the directory each shell sees).
pub(super) fn others_for(all: &[WindowSummary], me: Option<WindowId>) -> Vec<WindowSummary> {
    all.iter()
        .filter(|w| Some(w.id) != me)
        .cloned()
        .collect()
}

pub(super) fn min_deadline(a: Option<Instant>, b: Option<Instant>) -> Option<Instant> {
    match (a, b) {
        (Some(a), Some(b)) => Some(a.min(b)),
        (a, b) => a.or(b),
    }
}

pub struct MultiWindowApp {
    shells: Vec<AppShell>,
    proxy: EventLoopProxy<AppEvent>,
    /// Single source of truth for state.toml, swapped into the shell being
    /// dispatched and back out afterwards (one thread, one shell at a time).
    persistent: AppPersistentState,
    focused: Option<WindowId>,
    cascade: u32,
    /// What the first window opens (consumed in `resumed`).
    initial: Option<NewWindowRequest>,
    stale_instance_running: bool,
}

impl MultiWindowApp {
    pub fn new(
        proxy: EventLoopProxy<AppEvent>,
        cli_args: Vec<PathBuf>,
        persistent: AppPersistentState,
        stale_instance_running: bool,
    ) -> Self {
        Self {
            shells: Vec::new(),
            proxy,
            persistent,
            focused: None,
            cascade: 0,
            initial: Some(NewWindowRequest::Paths(cli_args)),
            stale_instance_running,
        }
    }

    /// Run `f` on shell `idx` with the persisted state swapped in.
    fn with_shell<R>(&mut self, idx: usize, f: impl FnOnce(&mut AppShell) -> R) -> R {
        std::mem::swap(&mut self.persistent, &mut self.shells[idx].persistent_state);
        let result = f(&mut self.shells[idx]);
        std::mem::swap(&mut self.persistent, &mut self.shells[idx].persistent_state);
        result
    }

    fn shell_index_for_window(&self, id: WindowId) -> Option<usize> {
        self.shells.iter().position(|s| s.window_id() == Some(id))
    }

    fn focused_index(&self) -> Option<usize> {
        self.focused.and_then(|id| self.shell_index_for_window(id))
    }

    /// Build a shell for `request` and open its window. Returns false when
    /// the window could not be created (the shell is discarded).
    fn spawn_window(&mut self, event_loop: &ActiveEventLoop, request: NewWindowRequest) -> bool {
        let persistent = std::mem::take(&mut self.persistent);
        let mut shell = match AppShell::new(self.proxy.clone(), request, persistent) {
            Ok(shell) => shell,
            Err(err) => {
                eprintln!("[MultiWindowApp] AppShell::new failed: {err}");
                // The state moved into the constructor is gone; reload it.
                self.persistent = AppPersistentState::load();
                return false;
            }
        };
        self.persistent = std::mem::take(&mut shell.persistent_state);
        shell.window_cascade = self.cascade;
        self.cascade += 1;
        self.shells.push(shell);
        let idx = self.shells.len() - 1;
        let opened = self.with_shell(idx, |s| s.on_resumed(event_loop));
        match opened {
            Ok(()) => {
                if let Some(window) = self.shells[idx].window.as_ref() {
                    window.focus_window();
                    self.focused = Some(window.id());
                }
                true
            }
            Err(err) => {
                eprintln!("[MultiWindowApp] window {idx} failed: {err}");
                let shell = self.shells.remove(idx);
                shell.finish_teardown();
                false
            }
        }
    }

    /// Give every shell the list of the OTHER open windows (switcher rows).
    fn refresh_window_directory(&mut self) {
        let all: Vec<WindowSummary> = self
            .shells
            .iter()
            .filter(|s| s.closing_since.is_none())
            .filter_map(|s| s.window_summary())
            .collect();
        for shell in &mut self.shells {
            let others = others_for(&all, shell.window_id());
            if shell.other_windows != others {
                shell.other_windows = others;
            }
        }
    }

    /// `<leader>p p` picked a window: bring it to the front.
    fn honor_focus_requests(&mut self) {
        let requests: Vec<WindowId> = self
            .shells
            .iter_mut()
            .filter_map(|s| s.pending_focus_window.take())
            .collect();
        for id in requests {
            if let Some(idx) = self.shell_index_for_window(id)
                && let Some(window) = self.shells[idx].window.as_ref()
            {
                window.focus_window();
                self.focused = Some(id);
            }
        }
    }

    /// Start teardown for shells that asked to close, drop the ones whose
    /// grace period passed, open the windows shells asked for, exit when
    /// none remain.
    fn reap_and_spawn(&mut self, event_loop: &ActiveEventLoop) {
        self.honor_focus_requests();
        for idx in 0..self.shells.len() {
            if self.shells[idx].exit_requested && self.shells[idx].closing_since.is_none() {
                self.with_shell(idx, |s| s.begin_teardown());
            }
        }
        let mut idx = 0;
        while idx < self.shells.len() {
            if self.shells[idx].teardown_due() {
                let shell = self.shells.remove(idx);
                if Some(shell.window_id()) == Some(self.focused) {
                    self.focused = None;
                }
                shell.finish_teardown();
            } else {
                idx += 1;
            }
        }
        let requests: Vec<NewWindowRequest> = self
            .shells
            .iter_mut()
            .flat_map(|s| s.take_pending_new_windows())
            .collect();
        for request in requests {
            self.spawn_window(event_loop, request);
        }
        if self.shells.is_empty() {
            event_loop.exit();
        }
    }
}

impl ApplicationHandler<AppEvent> for MultiWindowApp {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if let Some(request) = self.initial.take() {
            if !self.spawn_window(event_loop, request) {
                eprintln!("[fatal] could not open the first window");
                event_loop.exit();
                return;
            }
            if self.stale_instance_running {
                self.stale_instance_running = false;
                self.with_shell(0, |s| {
                    s.show_transient_toast_kind(
                        "Older Netherize build still running\nThis window is the new build. Quit the old one (Cmd+Q) when you are done there.",
                        ToastKind::Warning,
                    )
                });
            }
        }
        // macOS may call `resumed` again after a suspend; shells whose window
        // already exists return early.
        for idx in 0..self.shells.len() {
            let _ = self.with_shell(idx, |s| s.on_resumed(event_loop));
        }
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        window_id: WindowId,
        event: WindowEvent,
    ) {
        if matches!(event, WindowEvent::Focused(true)) {
            self.focused = Some(window_id);
        }
        if matches!(event, WindowEvent::KeyboardInput { .. }) {
            // A key can open the switcher: make sure it sees the current set.
            self.refresh_window_directory();
        }
        if let Some(idx) = self.shell_index_for_window(window_id) {
            self.with_shell(idx, |s| s.on_window_event(window_id, event));
        }
        self.reap_and_spawn(event_loop);
    }

    fn user_event(&mut self, event_loop: &ActiveEventLoop, event: AppEvent) {
        match event {
            AppEvent::RemoteOpen(paths) => {
                let hosted: Vec<Vec<PathBuf>> = self.shells.iter().map(|s| s.live_roots()).collect();
                match route_remote_open(&hosted, self.focused_index(), &paths) {
                    RemoteRoute::Existing(idx) | RemoteRoute::Focused(idx) => {
                        self.with_shell(idx, |s| s.on_user_event(AppEvent::RemoteOpen(paths)));
                    }
                    RemoteRoute::NewWindow(paths) => {
                        self.spawn_window(event_loop, NewWindowRequest::Paths(paths));
                    }
                    RemoteRoute::None => {}
                }
            }
            other => {
                // Worker wake-ups carry no window: every shell pumps its own
                // bridge (an empty pump is free).
                for idx in 0..self.shells.len() {
                    let event = match other {
                        AppEvent::TerminalOutputReady => AppEvent::TerminalOutputReady,
                        AppEvent::AiInlineReady => AppEvent::AiInlineReady,
                        AppEvent::WorkerMessageReady => AppEvent::WorkerMessageReady,
                        AppEvent::RemoteOpen(_) => unreachable!("handled above"),
                    };
                    self.with_shell(idx, |s| s.on_user_event(event));
                }
            }
        }
        self.reap_and_spawn(event_loop);
    }

    /// Cmd+Q terminates without a `CloseRequested` per window: persist every
    /// window's layout and the geometry of the focused one before we go.
    fn exiting(&mut self, _event_loop: &ActiveEventLoop) {
        let focused = self.focused_index();
        for idx in 0..self.shells.len() {
            self.with_shell(idx, |s| {
                s.persist_session_layouts(true);
                if Some(idx) == focused {
                    s.capture_window_geometry();
                    s.persist_window_geometry_if_due(true);
                }
            });
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        self.refresh_window_directory();
        let mut deadline: Option<Instant> = None;
        for idx in 0..self.shells.len() {
            let shell_deadline = self.with_shell(idx, |s| s.on_about_to_wait());
            deadline = min_deadline(deadline, shell_deadline);
            deadline = min_deadline(deadline, self.shells[idx].teardown_deadline());
        }
        self.reap_and_spawn(event_loop);
        match deadline {
            Some(when) => event_loop.set_control_flow(ControlFlow::WaitUntil(when)),
            None => event_loop.set_control_flow(ControlFlow::Wait),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("netherize_route_{tag}_{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("dir");
        dir.canonicalize().expect("canonicalize")
    }

    #[test]
    fn route_remote_open_prefers_hosting_window_then_new_window_then_focused() {
        let a = temp_dir("a");
        let b = temp_dir("b");
        let c = temp_dir("c");
        let hosted = vec![vec![a.clone()], vec![b.clone()]];
        let file = a.join("x.txt");

        assert_eq!(
            route_remote_open(&hosted, Some(0), &[b.clone(), file.clone()]),
            RemoteRoute::Existing(1),
            "dir hosted by window 1 (parked or active) wins even when 0 is focused"
        );
        assert_eq!(
            route_remote_open(&hosted, Some(0), std::slice::from_ref(&c)),
            RemoteRoute::NewWindow(vec![c.clone()])
        );
        assert_eq!(
            route_remote_open(&hosted, Some(1), std::slice::from_ref(&file)),
            RemoteRoute::Focused(1),
            "files only go to the focused window"
        );
        assert_eq!(
            route_remote_open(&hosted, None, std::slice::from_ref(&file)),
            RemoteRoute::Focused(1),
            "no focus → last window"
        );
        assert_eq!(
            route_remote_open(&[], None, std::slice::from_ref(&c)),
            RemoteRoute::None
        );

        for d in [a, b, c] {
            let _ = std::fs::remove_dir_all(d);
        }
    }

    #[test]
    fn others_for_excludes_the_asking_window() {
        let w = |n: u64| WindowSummary {
            id: WindowId::from(n),
            root: None,
            dirty: 0,
            branch: None,
        };
        let all = vec![w(1), w(2), w(3)];
        let others = others_for(&all, Some(WindowId::from(2)));
        assert_eq!(others.iter().map(|s| s.id).collect::<Vec<_>>(), vec![WindowId::from(1), WindowId::from(3)]);
        assert_eq!(others_for(&all, None).len(), 3);
    }

    #[test]
    fn min_deadline_takes_earliest_and_tolerates_none() {
        let now = Instant::now();
        let later = now + Duration::from_secs(1);
        assert_eq!(min_deadline(None, None), None);
        assert_eq!(min_deadline(Some(later), None), Some(later));
        assert_eq!(min_deadline(None, Some(now)), Some(now));
        assert_eq!(min_deadline(Some(later), Some(now)), Some(now));
    }
}
