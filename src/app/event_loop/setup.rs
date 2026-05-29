use super::*;

fn is_ai_inline_word_char(ch: char) -> bool {
    ch == '_' || ch.is_alphanumeric()
}

impl AppShell {
    pub fn new(event_proxy: EventLoopProxy<AppEvent>) -> Result<Self, String> {
        let (scheduler, rx) = AsyncScheduler::new(event_proxy)?;
        Self::new_with_scheduler(scheduler, rx)
    }

    #[cfg(test)]
    pub fn new_for_tests() -> Result<Self, String> {
        let (scheduler, rx) = AsyncScheduler::new_for_tests()?;
        Self::new_with_scheduler(scheduler, rx)
    }

    fn new_with_scheduler(
        scheduler: AsyncScheduler,
        rx: std::sync::mpsc::Receiver<crate::async_runtime::message::WorkerMessage>,
    ) -> Result<Self, String> {
        let save_path = PathBuf::new();
        let cwd = std::env::current_dir().unwrap_or_default();
        let now = Instant::now();
        let bridge = AppAsyncBridge::new(rx);

        // ── Parse CLI args ────────────────────────────────────────────────
        let cli_args: Vec<PathBuf> = std::env::args_os().skip(1).map(PathBuf::from).collect();

        // First directory arg becomes workspace root (like `zed .` / `code .`).
        // If only files are passed, use the first file's parent as the workspace
        // so a new process opens an isolated project instead of restoring the
        // globally most-recent project.
        let cli_workspace_dir = cli_args
            .iter()
            .filter_map(|p| p.canonicalize().ok())
            .find_map(|cp| {
                if cp.is_dir() {
                    Some(cp)
                } else if cp.is_file() {
                    cp.parent().map(Path::to_path_buf)
                } else {
                    None
                }
            });

        let cli_files: Vec<PathBuf> = cli_args
            .iter()
            .filter_map(|p| p.canonicalize().ok().filter(|cp| cp.is_file()))
            .collect();

        // Load persisted state and restore most recent project if it still exists.
        let mut persistent_state = AppPersistentState::load();

        let mut app_state = AppState::new(save_path.clone());
        let _ = app_state.apply_mode_event(ModeEvent::EnterNormal);

        let mut restored_workspace = false;
        // Priority: CLI directory > recent project > cwd
        if let Some(ref ws_dir) = cli_workspace_dir {
            match app_state.attach_workspace(ws_dir.clone()) {
                Ok(()) => restored_workspace = true,
                Err(err) => eprintln!(
                    "[AppShell] CLI workspace attach skipped ({}): {err}",
                    ws_dir.display()
                ),
            }
        }
        if !restored_workspace {
            if let Some(recent_dir) = persistent_state.most_recent_existing() {
                match app_state.attach_workspace(recent_dir.clone()) {
                    Ok(()) => {
                        restored_workspace = true;
                    }
                    Err(err) => {
                        eprintln!("[AppShell] workspace attach skipped: {err}");
                        persistent_state
                            .recent_projects
                            .retain(|p| p != &recent_dir);
                    }
                }
            }
        }
        if !restored_workspace && let Err(err) = app_state.attach_workspace(cwd) {
            eprintln!("[AppShell] cwd workspace attach skipped: {err}");
        }

        for cli_path in &cli_files {
            if let Err(err) = app_state.open_file(cli_path.clone()) {
                eprintln!(
                    "[AppShell] CLI file open skipped ({}): {err}",
                    cli_path.display()
                );
            }
        }
        // Welcome visibility is controlled by AppState's one-shot initial launch
        // flag, not by workspace attachment or later buffer-list emptiness.

        let workspace_git_branch = app_state.workspace_root_path().and_then(detect_git_branch);

        let mut base_theme =
            ThemeConfig::load_preferred(persistent_state.configured_theme_profile());
        let ai_config = AiConfig::load();
        let ui_config = UiConfig::load_active();
        // Sync explicitly user-set editor metrics from ui.toml → base_theme so that
        // apply_scaled_runtime_config() renders with the persisted values, not the
        // theme-file defaults.  Fields absent from ui.toml leave base_theme unchanged.
        let (user_font_size, user_line_height, user_font_family) =
            UiConfig::load_user_editor_overrides();
        if let Some(fs) = user_font_size {
            base_theme.editor.font_size = fs;
        }
        if let Some(lh) = user_line_height {
            base_theme.editor.line_height = lh;
        }
        if let Some(family) = user_font_family {
            base_theme.editor.font_family = Some(family);
        }
        let theme = base_theme.clone();
        app_state.set_indent_config(ui_config.indent);
        let layout_engine = WorkbenchLayoutEngine::new(
            crate::workbench::layout_engine::WorkbenchLayoutConfig::from_ui_theme(&theme.ui),
        );
        let mut panel_state = WorkbenchPanelState::from_ui_theme(&theme.ui);
        panel_state.left.size_px = ui_config.docks.left.size_px;
        panel_state.right.size_px = ui_config.docks.right.size_px;
        panel_state.bottom.size_px = ui_config.docks.bottom.size_px;
        panel_state.left.visible = ui_config.docks.left.visible;
        panel_state.right.visible = if DEBUG_UI_ENABLED {
            ui_config.docks.right.visible
        } else {
            false
        };
        panel_state.bottom.visible = ui_config.docks.bottom.visible;
        panel_state.overlay_visible = ui_config.docks.overlay_visible;
        let _ = app_state.set_terminal_panel_open(panel_state.bottom.visible);
        let window_width = ui_config.window.width;
        let window_height = ui_config.window.height;

        Ok(Self {
            app_state,
            persistent_state,
            input_handler: InputHandler::new(),
            input_map: InputMap::new(save_path),
            scheduler,
            bridge: Some(bridge),
            right_pty_session_id: None,
            right_terminal_grid: {
                let mut g = TerminalGrid::new(120, 40);
                g.highlight_colors = HighlightColors::from_theme(&theme);
                g
            },
            right_terminal_needs_layout: true,
            last_right_terminal_bounds: None,
            pending_right_pty_spawn: false,
            right_pty_startup_command: None,
            terminal_buffer_grids: HashMap::new(),
            pending_lazygit_buffer_index: None,
            pending_lazydocker_buffer_index: None,
            highlight_spans: Vec::new(),
            semantic_highlight_spans: Vec::new(),
            cached_document_symbols_path: None,
            cached_document_symbols: Vec::new(),
            syntax_engine: None,
            syntax_engine_file: None,
            terminal_tabs: {
                let mut g = TerminalGrid::new(120, 40);
                g.highlight_colors = HighlightColors::from_theme(&theme);
                vec![TerminalTab::new(g, "bash".to_string())]
            },
            active_terminal_tab: 0,
            pending_terminal_tab_spawns: HashMap::new(),
            ignored_terminal_tab_spawns: HashSet::new(),
            explorer_cursor: 0,
            explorer_snapshot: ExplorerSnapshot::default(),
            explorer_snapshot_dirty: true,
            explorer_clipboard_path: None,
            pending_confirmation: None,
            workspace_git_branch,
            active_lsp_server: None,
            pending_lsp_server: None,
            lsp_completion_trigger_chars: Vec::new(),
            active_lsp_guide: None,
            dismissed_lsp_binaries: HashSet::new(),
            active_system_dep_guide: None,
            dismissed_system_deps: false,
            transient_toast: None,
            theme_picker_original_theme: None,
            theme_picker_preview_profile: None,
            base_theme,
            theme,
            ui_config,
            ai_config,
            runtime_scale: 0.0,
            layout_engine,
            panel_state,
            focus_manager: FocusManager::default(),
            overlay_manager: OverlayManager::default(),
            clipboard: SystemClipboard::new(),
            window: None,
            renderer: None,
            window_size: PhysicalSize::new(window_width, window_height),
            editor_needs_layout: true,
            editor_caret_needs_layout: false,
            sidebar_needs_layout: true,
            terminal_needs_layout: true,
            buffer_terminal_needs_layout: true,
            terminal_search_palette_active: false,
            last_frame_time: now,
            last_fps_metrics_update_at: now,
            accumulated_frame_time: Duration::ZERO,
            accumulated_frame_count: 0,
            current_fps_metrics: "--.-ms | -- FPS".to_string(),
            last_parse_submit_at: None,
            last_git_diff_recalc_at: None,
            last_syntax_edit_hint: None,
            active_highlight_request_revision: 0,
            semantic_highlight_request_revision: 0,
            references_request_revision: 0,
            completion_resolve_request_id: None,
            active_lsp_completion_request_id: None,
            hover_loading_request_id: None,
            latest_hover_request_id: None,
            latest_definition_request_id: None,
            document_symbols_request_revision: 0,
            lsp_rename_request_revision: 0,
            latest_rename_request_id: None,
            fzf_search_revision: 0,
            pending_parse_after_debounce: false,
            pending_git_diff_after_debounce: false,
            pending_completion_resolve_after_debounce: false,
            last_completion_resolve_select_at: None,
            pending_completion_resolve_revision: 0,
            pending_completion_accept_after_resolve: None,
            pending_lsp_completion_after_debounce: false,
            last_lsp_completion_type_at: None,
            ai_inline_revision: 0,
            pending_ai_inline_request: None,
            ai_inline_cancel_token: None,
            last_ai_inline_queue_at: None,
            last_ai_inline_submit_at: None,
            pending_lsp_document_sync: None,
            last_editor_bounds: None,
            last_show_welcome: None,
            last_sidebar_bounds: None,
            last_sidebar_focused: None,
            last_terminal_bounds: None,
            last_buffer_terminal_bounds: None,
            sidebar_selection_quads: Vec::new(),
            suppress_next_palette_ime_commit: false,
            leap_state: None,
            git_overlay_revision: 0,
            git_status_revision: 0,
            git_baseline_revision: 0,
            last_scroll_animation_tick: now,
            last_git_branch_refresh_at: now,
            last_workspace_git_status_refresh_at: now,
            last_thinking_animation_tick: now,
            last_lsp_loading_animation_tick: now,
            lsp_loading_frame: 0,
            caret_blink_visible: true,
            caret_blink_dirty: false,
            last_caret_blink_tick: now,
            last_external_file_check: now,
            last_external_file_check_times: std::collections::HashMap::new(),
            pre_markdown_preview_right_width: None,
            pending_code_actions: Vec::new(),
            selected_python_env: None,
            runtime_versions: RuntimeVersionInfo::default(),
            lsp_retry_at: None,
        })
    }

    pub(super) fn startup_subsystems(&mut self) {
        let cwd = std::env::current_dir().unwrap_or_default();
        let workspace_root = self
            .app_state
            .workspace_root_path()
            .map(PathBuf::from)
            .unwrap_or_else(|| cwd.clone());

        self.submit(RequestSpec {
            revision_id: 0,
            topic: RequestTopic::WorkspaceWatch,
            payload: WorkerRequestPayload::StartFileWatch {
                root_path: workspace_root.clone(),
            },
        });

        self.submit(RequestSpec {
            revision_id: 0,
            topic: RequestTopic::TerminalPty,
            payload: WorkerRequestPayload::SpawnPtyShell {
                shell: None,
                working_dir: Some(workspace_root.clone()),
            },
        });

        if let Some(path) = self.app_state.active_file().map(PathBuf::from) {
            self.submit_lsp_check_for_path(path);
            self.submit_lsp_did_open_for_active_file();
        }

        // ── System Dependency Check ──────────────────────────────────────
        self.submit(RequestSpec {
            revision_id: 0,
            topic: RequestTopic::SystemDepCheck,
            payload: WorkerRequestPayload::CheckSystemDeps,
        });

        // ── Git Status & Baseline ────────────────────────────────────────
        self.submit_workspace_git_status_refresh();
        self.submit_active_buffer_git_baseline_refresh();

        eprintln!(
            "[AppShell] subsystems started - profile={}",
            KeymapLoader::active_profile()
        );
    }

    pub(super) fn submit(
        &self,
        spec: RequestSpec,
    ) -> Option<crate::async_runtime::message::WorkerRequest> {
        match self.scheduler.submit(spec) {
            Ok(request) => Some(request),
            Err(err) => {
                eprintln!("[AppShell] scheduler submit failed: {err}");
                None
            }
        }
    }

    pub(super) fn pump_bridge(&mut self) -> bool {
        let Some(mut bridge) = self.bridge.take() else {
            return false;
        };
        let stats = bridge.pump(self);
        self.bridge = Some(bridge);
        stats.accepted > 0 || stats.completed > 0
    }

    pub(super) fn request_redraw(&self) {
        if let Some(window) = &self.window {
            window.request_redraw();
        }
    }

    /// Update the window title bar to show the project name.
    ///
    /// - No workspace attached: title stays as the configured base title
    ///   (e.g. "Netherize Editor").
    /// - Workspace attached: title becomes "<project-name> - <base title>"
    ///   (e.g. "my-project - Netherize Editor").
    pub(super) fn update_window_title(&self) {
        let Some(window) = self.window.as_ref() else {
            return;
        };
        let base_title = &self.ui_config.window.title;

        if let Some(root) = self.app_state.workspace_root_path() {
            let project_name = root
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("Netherize Editor");
            window.set_title(&format!("{project_name} - {base_title}"));
        } else {
            window.set_title(base_title);
        }
    }

    pub(super) fn refresh_workspace_git_branch(&mut self) -> bool {
        let next_branch = self
            .app_state
            .workspace_root_path()
            .and_then(detect_git_branch);
        if self.workspace_git_branch == next_branch {
            return false;
        }
        self.workspace_git_branch = next_branch;
        true
    }

    pub(super) fn maybe_refresh_workspace_git_branch(&mut self, force: bool) -> bool {
        let now = Instant::now();
        if !force
            && now.saturating_duration_since(self.last_git_branch_refresh_at)
                < GIT_BRANCH_REFRESH_INTERVAL
        {
            return false;
        }
        self.last_git_branch_refresh_at = now;
        self.refresh_workspace_git_branch()
    }

    pub(super) fn submit_workspace_git_status_refresh(&mut self) {
        let Some(workspace_root) = self.app_state.workspace_root_path().map(PathBuf::from) else {
            return;
        };
        self.last_workspace_git_status_refresh_at = Instant::now();
        self.git_status_revision = self.git_status_revision.saturating_add(1);
        self.submit(RequestSpec {
            revision_id: self.git_status_revision,
            topic: RequestTopic::GitStatus,
            payload: WorkerRequestPayload::RefreshWorkspaceGitStatus { workspace_root },
        });
    }

    pub(super) fn maybe_refresh_workspace_git_status(&mut self) -> bool {
        if !self.app_state.active_buffer_is_terminal() && self.terminal_buffer_grids.is_empty() {
            return false;
        }
        if self.last_workspace_git_status_refresh_at.elapsed() < GIT_STATUS_REFRESH_INTERVAL {
            return false;
        }
        self.submit_workspace_git_status_refresh();
        true
    }

    pub(super) fn next_workspace_git_status_refresh_deadline(&self) -> Option<Instant> {
        (self.app_state.active_buffer_is_terminal() || !self.terminal_buffer_grids.is_empty())
            .then_some(self.last_workspace_git_status_refresh_at + GIT_STATUS_REFRESH_INTERVAL)
    }

    pub(super) fn submit_active_buffer_git_baseline_refresh(&mut self) {
        let Some(workspace_root) = self.app_state.workspace_root_path().map(PathBuf::from) else {
            return;
        };
        let Some(file_path) = self.app_state.active_file().map(PathBuf::from) else {
            return;
        };
        self.pending_git_diff_after_debounce = false;
        self.last_git_diff_recalc_at = None;
        self.git_baseline_revision = self.git_baseline_revision.saturating_add(1);
        self.submit(RequestSpec {
            revision_id: self.git_baseline_revision,
            topic: RequestTopic::GitBaseline,
            payload: WorkerRequestPayload::FetchGitBaseline {
                workspace_root,
                file_path,
            },
        });
    }

    pub(super) fn refresh_active_buffer_git_diff_state(&mut self) {
        if self.app_state.recalculate_active_buffer_git_diff() {
            self.editor_needs_layout = true;
            self.editor_caret_needs_layout = false;
            self.request_redraw();
        }
        self.last_git_diff_recalc_at = Some(Instant::now());
        self.pending_git_diff_after_debounce = false;
    }

    pub(super) fn schedule_active_buffer_git_diff_recalculation(&mut self, force: bool) {
        if !force
            && let Some(last) = self.last_git_diff_recalc_at
            && last.elapsed() < GIT_DIFF_DEBOUNCE_INTERVAL
        {
            self.pending_git_diff_after_debounce = true;
            return;
        }

        self.refresh_active_buffer_git_diff_state();
    }

    pub(super) fn flush_pending_git_diff_after_debounce(&mut self) {
        if !self.pending_git_diff_after_debounce {
            return;
        }

        if let Some(last) = self.last_git_diff_recalc_at
            && last.elapsed() < GIT_DIFF_DEBOUNCE_INTERVAL
        {
            return;
        }

        self.refresh_active_buffer_git_diff_state();
    }

    pub(super) fn next_git_diff_recalc_deadline(&self) -> Option<Instant> {
        self.pending_git_diff_after_debounce
            .then(|| self.last_git_diff_recalc_at.unwrap_or_else(Instant::now))
            .map(|last| last + GIT_DIFF_DEBOUNCE_INTERVAL)
    }

    pub(super) fn next_lsp_retry_deadline(&self) -> Option<Instant> {
        self.lsp_retry_at
    }

    pub(super) fn flush_lsp_retry_if_due(&mut self) -> bool {
        let Some(retry_at) = self.lsp_retry_at else {
            return false;
        };
        if Instant::now() < retry_at {
            return false;
        }
        self.lsp_retry_at = None;
        self.sync_lsp_server_for_workspace()
    }

    pub(super) fn next_git_branch_refresh_deadline(&self) -> Instant {
        self.last_git_branch_refresh_at + GIT_BRANCH_REFRESH_INTERVAL
    }

    pub(super) fn update_runtime_scaling_for_window(&mut self, scale_factor: f64) {
        // let raw_scale = if let Some(over) = self.ui_config.window.scale_factor_override {
        //     over as f64
        // } else {
        //     scale_factor
        // };
        // let dpi_scale = (raw_scale as f32).max(0.25);
        let dpi_scale = (scale_factor as f32).max(0.25);
        let logical_width = self.window_size.width as f32 / dpi_scale;
        let logical_height = self.window_size.height as f32 / dpi_scale;

        let mut content_scale = 1.0_f32;
        if self.ui_config.window.auto_scale {
            let base_width = self.ui_config.window.width as f32;
            let base_height = self.ui_config.window.height as f32;
            if base_width > 0.0 && base_height > 0.0 {
                content_scale = (logical_width / base_width).min(logical_height / base_height);
            }
            let min_scale = self.ui_config.window.min_content_scale;
            let max_scale = self.ui_config.window.max_content_scale;
            let lower = match (min_scale.is_nan(), max_scale.is_nan()) {
                (true, true) => 1.0,
                (true, false) => max_scale,
                (false, true) => min_scale,
                (false, false) => min_scale.min(max_scale),
            };
            let upper = match (min_scale.is_nan(), max_scale.is_nan()) {
                (true, true) => 1.0,
                (true, false) => max_scale,
                (false, true) => min_scale,
                (false, false) => min_scale.max(max_scale),
            };
            content_scale = if content_scale.is_nan() {
                lower
            } else {
                content_scale.clamp(lower, upper)
            };
            debug_assert!(
                content_scale.is_finite(),
                "content_scale must be finite after normalization: min_scale={min_scale}, max_scale={max_scale}, logical_width={logical_width}, logical_height={logical_height}"
            );
        }

        let runtime_scale = (dpi_scale * content_scale).max(0.5);
        if (runtime_scale - self.runtime_scale).abs() < 0.001 {
            return;
        }

        self.runtime_scale = runtime_scale;
        self.apply_scaled_runtime_config();
    }

    pub(super) fn apply_scaled_runtime_config(&mut self) {
        let scaled_theme = scale_theme(&self.base_theme, self.runtime_scale);
        let scaled_ui = scale_ui_config(&self.ui_config, self.runtime_scale);

        self.theme = scaled_theme.clone();
        if let Some(font_family) = scaled_ui.editor.font_family.clone() {
            self.theme.editor.font_family = Some(font_family);
        }

        // Update terminal highlight colors when theme changes
        self.right_terminal_grid.highlight_colors = HighlightColors::from_theme(&self.theme);
        for tab in &mut self.terminal_tabs {
            tab.grid.highlight_colors = HighlightColors::from_theme(&self.theme);
        }
        for grid in self.terminal_buffer_grids.values_mut() {
            grid.highlight_colors = HighlightColors::from_theme(&self.theme);
        }

        self.layout_engine.config =
            crate::workbench::layout_engine::WorkbenchLayoutConfig::from_ui_theme(&scaled_theme.ui);
        self.layout_engine.config.outer_gap = scaled_ui.layout.outer_gap;
        self.layout_engine.config.panel_gap = scaled_ui.layout.panel_gap;
        self.layout_engine.config.inner_padding = scaled_ui.layout.inner_padding;
        self.layout_engine.config.round_ui = scaled_ui.layout.round_ui;
        self.layout_engine.config.center_min_width = scaled_ui.layout.center_min_width;
        self.layout_engine.config.center_min_height = scaled_ui.layout.center_min_height;
        self.layout_engine.config.sidebar_min_width = scaled_ui.layout.sidebar_min_width;
        self.layout_engine.config.bottom_min_height = scaled_ui.layout.bottom_min_height;
        self.layout_engine.config.chat_input_height = scaled_ui.layout.chat_input_height;
        self.layout_engine.config.panel_border_width = scaled_ui.layout.panel_border_width;

        self.panel_state.left.size_px = scaled_ui.docks.left.size_px;
        self.panel_state.right.size_px = scaled_ui.docks.right.size_px;
        self.panel_state.bottom.size_px = scaled_ui.docks.bottom.size_px;
        if let Some(renderer) = self.renderer.as_mut() {
            renderer.apply_theme(scaled_theme);
            renderer.apply_ui_config(&scaled_ui);
        }

        self.editor_needs_layout = true;
        self.editor_caret_needs_layout = false;
        self.sidebar_needs_layout = true;
        self.terminal_needs_layout = true;
        self.right_terminal_needs_layout = true;
        self.buffer_terminal_needs_layout = true;
        self.last_editor_bounds = None;
        self.last_show_welcome = None;
        self.last_sidebar_bounds = None;
        self.last_sidebar_focused = None;
        self.last_terminal_bounds = None;
        self.last_right_terminal_bounds = None;
        self.last_buffer_terminal_bounds = None;
        self.sidebar_selection_quads.clear();
    }

    pub(super) fn sync_right_terminal_layout(&mut self, bounds: [f32; 4]) -> bool {
        let scaled_ui = scale_ui_config(&self.ui_config, self.runtime_scale);
        let panel_padding = scaled_ui.layout.inner_padding;
        let line_height = self.theme.ui.panel_line_height.max(1.0);
        let cell_width = (self.theme.ui.panel_font_size * 0.6).max(1.0);

        let content_width = (bounds[2] - panel_padding * 2.0).max(1.0);
        let content_height = (bounds[3] - panel_padding * 2.0 - line_height).max(1.0);
        let cols = (content_width / cell_width).floor().max(1.0) as usize;
        let rows = (content_height / line_height).floor().max(1.0) as usize;

        let grid_changed = self.right_terminal_grid.resize(cols, rows);
        if grid_changed {
            self.right_terminal_needs_layout = true;
        }

        if grid_changed && let Some(session_id) = self.right_pty_session_id {
            self.submit(RequestSpec {
                revision_id: 0,
                topic: RequestTopic::TerminalPty,
                payload: WorkerRequestPayload::ResizePtySession {
                    session_id,
                    cols: cols.min(u16::MAX as usize) as u16,
                    rows: rows.min(u16::MAX as usize) as u16,
                },
            });
        }

        grid_changed
    }

    pub(super) fn sync_terminal_layout(&mut self, bounds: [f32; 4]) -> bool {
        let scaled_ui = scale_ui_config(&self.ui_config, self.runtime_scale);
        let panel_padding = scaled_ui.layout.inner_padding;
        let line_height = self.theme.ui.panel_line_height.max(1.0);
        let cell_width = (self.theme.ui.panel_font_size * 0.6).max(1.0);

        let content_width = (bounds[2] - panel_padding * 2.0).max(1.0);
        let content_height = (bounds[3] - panel_padding * 2.0 - line_height).max(1.0);
        let cols = (content_width / cell_width).floor().max(1.0) as usize;
        let rows = (content_height / line_height).floor().max(1.0) as usize;

        let mut changed_sessions = Vec::new();
        let mut grid_changed = false;
        for tab in &mut self.terminal_tabs {
            if tab.grid.resize(cols, rows) {
                grid_changed = true;
                if let Some(session_id) = tab.session_id {
                    changed_sessions.push(session_id);
                }
            }
        }
        if grid_changed {
            self.terminal_needs_layout = true;
        }

        for session_id in changed_sessions {
            self.submit(RequestSpec {
                revision_id: 0,
                topic: RequestTopic::TerminalPty,
                payload: WorkerRequestPayload::ResizePtySession {
                    session_id,
                    cols: cols.min(u16::MAX as usize) as u16,
                    rows: rows.min(u16::MAX as usize) as u16,
                },
            });
        }

        grid_changed
    }

    pub(super) fn sync_terminal_buffer_layout(
        &mut self,
        session_id: u64,
        bounds: [f32; 4],
    ) -> bool {
        let Some(grid) = self.terminal_buffer_grids.get_mut(&session_id) else {
            return false;
        };

        let scaled_ui = scale_ui_config(&self.ui_config, self.runtime_scale);
        let panel_padding = scaled_ui.layout.inner_padding;
        let line_height = self.theme.ui.panel_line_height.max(1.0);
        let cell_width = (self.theme.ui.panel_font_size * 0.6).max(1.0);

        let content_width = (bounds[2] - panel_padding * 2.0).max(1.0);
        let content_height = (bounds[3] - panel_padding * 2.0 - line_height).max(1.0);
        let cols = (content_width / cell_width).floor().max(1.0) as usize;
        let rows = (content_height / line_height).floor().max(1.0) as usize;

        let grid_changed = grid.resize(cols, rows);
        if grid_changed {
            self.buffer_terminal_needs_layout = true;
            self.submit(RequestSpec {
                revision_id: 0,
                topic: RequestTopic::TerminalPty,
                payload: WorkerRequestPayload::ResizePtySession {
                    session_id,
                    cols: cols.min(u16::MAX as usize) as u16,
                    rows: rows.min(u16::MAX as usize) as u16,
                },
            });
        }

        grid_changed
    }

    pub(super) fn build_context(&self) -> KeybindingContext {
        let mode = self.app_state.current_mode();
        let welcome_visible = self.app_state.is_initial_launch_welcome()
            && self.app_state.buffers().is_empty()
            && (!self.app_state.is_command_palette_visible()
                || self.app_state.command_palette_mode()
                    == Some(CommandPaletteMode::RecentProjects));
        let focus = if welcome_visible
            && !matches!(self.focus_manager.current(), FocusTarget::LeftSidebar)
        {
            InputFocusContext::Welcome
        } else {
            match self.focus_manager.current() {
                FocusTarget::LeftSidebar => InputFocusContext::Explorer,
                FocusTarget::RightSidebar => match self.panel_state.right.active_tab_id() {
                    Some(PanelTabId::AiChat) => InputFocusContext::AiChat,
                    Some(PanelTabId::MarkdownPreview) => InputFocusContext::MarkdownPreview,
                    Some(PanelTabId::Terminal)
                        if matches!(
                            mode,
                            EditorMode::TerminalFocus | EditorMode::TerminalNormal
                        ) =>
                    {
                        InputFocusContext::Terminal
                    }
                    _ => InputFocusContext::Inspector,
                },
                FocusTarget::BottomPanel => {
                    if matches!(mode, EditorMode::TerminalFocus | EditorMode::TerminalNormal) {
                        InputFocusContext::Terminal
                    } else {
                        InputFocusContext::BottomPanel
                    }
                }
                FocusTarget::CenterEditor if self.app_state.active_buffer_is_terminal() => {
                    if mode == EditorMode::TerminalNormal {
                        InputFocusContext::Terminal
                    } else {
                        InputFocusContext::BufferTerminal
                    }
                }
                FocusTarget::CenterEditor if self.app_state.active_buffer_is_fuzzy_picker() => {
                    InputFocusContext::FuzzyPicker
                }
                FocusTarget::CenterEditor if self.app_state.active_buffer_is_settings() => {
                    InputFocusContext::SettingsTab
                }
                FocusTarget::CenterEditor if self.app_state.active_buffer_is_diagnostics() => {
                    InputFocusContext::Diagnostics
                }
                FocusTarget::CenterEditor if self.app_state.active_buffer_is_markdown_preview() => {
                    InputFocusContext::MarkdownPreview
                }
                FocusTarget::CenterEditor if self.app_state.active_buffer_is_references() => {
                    InputFocusContext::References
                }
                FocusTarget::CenterEditor if self.app_state.active_buffer_is_help() => {
                    InputFocusContext::Help
                }
                FocusTarget::CenterEditor if self.app_state.active_buffer_is_extensions_manager() => {
                    InputFocusContext::ExtensionsManager
                }
                _ => InputFocusContext::Editor,
            }
        };
        KeybindingContext {
            mode,
            focus,
            command_palette_visible: self.app_state.is_command_palette_visible(),
            command_palette_mode: self.app_state.command_palette_mode(),
            welcome_visible,
            completion_visible: self.app_state.has_completion(),
            hover_overlay_visible: self.app_state.has_scrollable_floating_overlay(),
            zen_mode_active: self.panel_state.maximized_region.is_some(),
        }
    }

    pub(super) fn sidebar_filter_state(&self) -> Option<SidebarFilterState> {
        let query = self
            .app_state
            .workspace_filter_query()
            .unwrap_or_default()
            .to_string();
        let is_inputting = self.app_state.workspace_is_inputting_filter();
        if !is_inputting && query.is_empty() {
            return None;
        }
        Some(SidebarFilterState {
            query,
            is_inputting,
            show_cursor: is_inputting && self.filter_cursor_visible(),
        })
    }

    pub(super) fn filter_cursor_visible(&self) -> bool {
        let elapsed_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_millis())
            .unwrap_or(0);
        (elapsed_ms / 500) % 2 == 0
    }

    pub(super) fn editor_viewport_lines(&self) -> usize {
        if let Some(bounds) = self.last_editor_bounds {
            let line_height = self.theme.editor.line_height;
            let padding = 14.0 * 2.0 + line_height;
            ((bounds[3] - padding) / line_height).floor() as usize
        } else {
            20
        }
    }

    pub(super) fn sidebar_tree_viewport_height(&self, bounds: [f32; 4]) -> f32 {
        let line_height = self.theme.ui.sidebar_line_height.max(1.0);
        let scaled_ui = scale_ui_config(&self.ui_config, self.runtime_scale);
        let filter_height = if self.sidebar_filter_state().is_some() {
            31.0
        } else {
            0.0
        };
        (bounds[3] - filter_height - scaled_ui.spacing.panel_padding - line_height - 2.0)
            .max(line_height)
    }

    pub(super) fn sync_explorer_scroll_to_selected(&mut self, bounds: [f32; 4]) -> bool {
        let viewport_height = self.sidebar_tree_viewport_height(bounds);
        self.app_state.workspace_scroll_to_selected_node(
            viewport_height,
            self.theme.ui.sidebar_line_height.max(1.0),
        )
    }

    pub(super) fn update_frame_metrics_snapshot(&mut self, now: Instant) {
        let delta = now.saturating_duration_since(self.last_frame_time);
        if delta.is_zero() {
            return;
        }

        self.accumulated_frame_time += delta;
        self.accumulated_frame_count = self.accumulated_frame_count.saturating_add(1);

        if now.duration_since(self.last_fps_metrics_update_at) < FPS_METRICS_UPDATE_INTERVAL {
            return;
        }

        let sample_count = self.accumulated_frame_count.max(1) as f64;
        let avg_secs = self.accumulated_frame_time.as_secs_f64() / sample_count;
        let avg_ms = avg_secs * 1000.0;
        let fps = if avg_secs > f64::EPSILON {
            1.0 / avg_secs
        } else {
            0.0
        };

        self.current_fps_metrics = format!("{avg_ms:.1}ms | {:.0} FPS", fps);
        self.accumulated_frame_time = Duration::ZERO;
        self.accumulated_frame_count = 0;
        self.last_fps_metrics_update_at = now;
    }

    pub(super) fn release_focus_mode_to_editor(&mut self) -> bool {
        let mut changed = false;
        let was_terminal_focus = matches!(
            self.app_state.current_mode(),
            EditorMode::TerminalFocus | EditorMode::TerminalNormal
        );

        if self.app_state.current_mode() == EditorMode::PaletteFocus {
            changed |= self.app_state.close_command_palette();
        }

        if matches!(
            self.app_state.current_mode(),
            EditorMode::PaletteFocus | EditorMode::TerminalFocus | EditorMode::TerminalNormal
        ) && let Ok(result) = self.app_state.apply_mode_event(ModeEvent::ExitFocus)
        {
            changed |= result.changed;
        }

        if was_terminal_focus {
            if self.app_state.active_buffer_is_terminal() {
                self.buffer_terminal_needs_layout = true;
            } else {
                self.terminal_needs_layout = true;
            }
        }

        changed
    }

    pub(super) fn submit_parse_for_active_buffer(&mut self, force: bool) {
        if !force
            && let Some(last) = self.last_parse_submit_at
            && last.elapsed() < PARSE_DEBOUNCE_INTERVAL
        {
            self.pending_parse_after_debounce = true;
            return;
        }

        if self.refresh_inline_syntax_highlighting() {
            return;
        }

        if let Some(file_path) = self.app_state.active_file().map(PathBuf::from) {
            let Some(language_id) = crate::syntax::parser::language_id_for_path(&file_path) else {
                self.clear_highlight_layers();
                self.syntax_engine = None;
                self.syntax_engine_file = None;
                self.pending_parse_after_debounce = false;
                self.editor_needs_layout = true;
                self.editor_caret_needs_layout = false;
                return;
            };

            let viewport_line_count = self.editor_viewport_lines().max(1);
            self.active_highlight_request_revision =
                self.active_highlight_request_revision.saturating_add(1);

            // Plaintext: regex highlight inline cho mọi kích thước file.
            if language_id == crate::syntax::syntax_engine::LanguageId::Plaintext {
                let text_snapshot = self.app_state.text_string();
                self.highlight_spans =
                    crate::syntax::highlight::generate_plaintext_highlight_spans(&text_snapshot);
                self.semantic_highlight_spans.clear();
                self.syntax_engine = None;
                self.syntax_engine_file = None;
                self.pending_parse_after_debounce = false;
                self.last_parse_submit_at = Some(std::time::Instant::now());
                self.editor_needs_layout = true;
                self.editor_caret_needs_layout = false;
                return;
            }

            self.pending_parse_after_debounce = false;
            let edit_hint = self.last_syntax_edit_hint.take();
            let line_starts = self.app_state.line_start_byte_indices().to_vec();
            self.submit(RequestSpec {
                revision_id: self.active_highlight_request_revision,
                topic: RequestTopic::ActiveBufferLayout,
                payload: WorkerRequestPayload::ParseAndHighlight {
                    buffer_id: file_path.clone(),
                    file_path: Some(file_path),
                    text_snapshot: self.app_state.text_string(),
                    language_id,
                    buffer_revision: self.app_state.revision(),
                    viewport_line_start: self.app_state.scroll_line(),
                    viewport_line_count,
                    line_starts,
                    edit_hint,
                },
            });
            self.last_parse_submit_at = Some(std::time::Instant::now());
        } else {
            self.pending_parse_after_debounce = false;
        }
    }

    pub(super) fn invalidate_highlights_and_parse_active_buffer(&mut self) {
        self.clear_highlight_layers();
        self.syntax_engine = None;
        self.syntax_engine_file = None;
        self.last_syntax_edit_hint = None;
        self.pending_parse_after_debounce = false;
        self.editor_needs_layout = true;
        self.editor_caret_needs_layout = false;
        self.submit_parse_for_active_buffer(true);
        self.request_redraw();
    }

    fn refresh_inline_syntax_highlighting(&mut self) -> bool {
        let Some(file_path) = self.app_state.active_file().map(PathBuf::from) else {
            let had_highlighting =
                !self.highlight_spans.is_empty() || !self.semantic_highlight_spans.is_empty();
            if had_highlighting {
                self.clear_highlight_layers();
                self.syntax_engine = None;
                self.syntax_engine_file = None;
                self.editor_needs_layout = true;
                self.editor_caret_needs_layout = false;
            }
            return had_highlighting;
        };

        let Some(language_id) = crate::syntax::parser::language_id_for_path(&file_path) else {
            let had_highlighting =
                !self.highlight_spans.is_empty() || !self.semantic_highlight_spans.is_empty();
            if had_highlighting {
                self.clear_highlight_layers();
                self.editor_needs_layout = true;
                self.editor_caret_needs_layout = false;
            }
            self.syntax_engine = None;
            self.syntax_engine_file = None;
            return had_highlighting;
        };

        if self.app_state.text_len_bytes()
            > crate::syntax::highlight::INLINE_TREE_SITTER_BYTE_THRESHOLD
            || self.app_state.total_lines()
                > crate::syntax::highlight::INLINE_TREE_SITTER_LINE_THRESHOLD
        {
            return false;
        }

        let text_snapshot = self.app_state.text_string();

        // Plaintext dùng regex highlight thay vì tree-sitter.
        if language_id == crate::syntax::syntax_engine::LanguageId::Plaintext {
            self.highlight_spans =
                crate::syntax::highlight::generate_plaintext_highlight_spans(&text_snapshot);
            self.semantic_highlight_spans.clear();
            self.syntax_engine = None;
            self.syntax_engine_file = None;
            self.pending_parse_after_debounce = false;
            self.last_parse_submit_at = Some(std::time::Instant::now());
            self.editor_needs_layout = true;
            self.editor_caret_needs_layout = false;
            return true;
        }

        let buffer_revision = self.app_state.revision();
        let needs_reset = self
            .syntax_engine_file
            .as_ref()
            .is_none_or(|current| current != &file_path)
            || self
                .syntax_engine
                .as_ref()
                .is_none_or(|engine| engine.language_id() != language_id);
        if needs_reset {
            self.syntax_engine = match SyntaxEngine::new(language_id) {
                Ok(engine) => Some(engine),
                Err(err) => {
                    eprintln!("[AppShell] syntax engine init failed: {err}");
                    self.syntax_engine_file = None;
                    return false;
                }
            };
            self.syntax_engine_file = Some(file_path);
        }

        let Some(engine) = self.syntax_engine.as_mut() else {
            return false;
        };

        // Use incremental parse when a fresh single-edit hint is available.
        let hint = self.last_syntax_edit_hint.take();
        let parse_result = match hint {
            Some(h) => engine.parse_incremental(
                &text_snapshot,
                h.start_byte,
                h.old_end_byte,
                h.new_end_byte,
                buffer_revision,
            ),
            None => engine.parse_source(&text_snapshot, buffer_revision),
        };
        let parse_result = match parse_result {
            Ok(tree) if tree.root_node().end_byte() <= text_snapshot.len() => Ok(tree),
            Ok(_) => engine.parse_source(&text_snapshot, buffer_revision),
            Err(err) => {
                eprintln!("[AppShell] incremental tree-sitter parse failed: {err}; retrying full parse");
                engine.parse_source(&text_snapshot, buffer_revision)
            }
        };
        match parse_result {
            Ok(tree) => {
                self.highlight_spans =
                    crate::syntax::highlight::generate_highlight_spans(tree, &text_snapshot);
                let foldable = crate::syntax::fold::compute_foldable_ranges(
                    tree.root_node(),
                    tree.language_id(),
                );
                self.app_state.set_foldable_ranges_cache(foldable);
                self.pending_parse_after_debounce = false;
                self.last_parse_submit_at = Some(std::time::Instant::now());
                self.editor_needs_layout = true;
                self.editor_caret_needs_layout = false;
                true
            }
            Err(err) => {
                eprintln!("[AppShell] inline tree-sitter parse failed: {err}");
                false
            }
        }
    }

    pub(super) fn clear_highlight_layers(&mut self) {
        self.highlight_spans.clear();
        self.semantic_highlight_spans.clear();
    }

    pub(super) fn flush_pending_parse_after_debounce(&mut self) {
        if !self.pending_parse_after_debounce {
            return;
        }

        if let Some(last) = self.last_parse_submit_at
            && last.elapsed() < PARSE_DEBOUNCE_INTERVAL
        {
            return;
        }

        // Multiple keystrokes accumulated during the debounce window; a single-edit
        // hint would describe only one of them — discard it and do a full reparse.
        self.last_syntax_edit_hint = None;
        self.submit_parse_for_active_buffer(true);
    }

    pub(super) fn cancel_ai_inline_completion(&mut self) {
        self.pending_ai_inline_request = None;
        if let Some(token) = self.ai_inline_cancel_token.take() {
            token.cancel();
        }
    }

    pub(super) fn queue_ai_inline_completion(&mut self) {
        if self.app_state.current_mode() != EditorMode::Insert {
            self.cancel_ai_inline_completion();
            return;
        }
        if self.app_state.active_buffer_is_terminal() || self.app_state.active_file().is_none() {
            self.cancel_ai_inline_completion();
            return;
        }
        let Some(cfg) = self.ai_config.inline_completion() else {
            self.cancel_ai_inline_completion();
            return;
        };
        let now = Instant::now();
        if !self.should_queue_ai_inline_completion(cfg, now) {
            self.pending_ai_inline_request = None;
            self.last_ai_inline_queue_at = Some(now);
            return;
        }
        self.last_ai_inline_queue_at = Some(now);
        self.ai_inline_revision = self.ai_inline_revision.saturating_add(1);
        self.pending_ai_inline_request = Some(PendingAiInlineRequest {
            revision: self.ai_inline_revision,
            queued_at: now,
        });
    }

    fn should_queue_ai_inline_completion(
        &self,
        cfg: &crate::config::ai_config::InlineCompletionConfig,
        now: Instant,
    ) -> bool {
        if let Some(last_submit) = self.last_ai_inline_submit_at
            && last_submit.elapsed() < Duration::from_millis(cfg.min_interval_ms())
        {
            return false;
        }

        let cursor = self.app_state.cursor_char_idx();
        if cursor < cfg.min_prefix_chars() {
            return false;
        }

        let text = self.app_state.text_string();
        let mut chars = text.chars();
        let before = cursor.checked_sub(1).and_then(|idx| chars.nth(idx));
        let after = text.chars().nth(cursor);

        if cfg.suppress_in_middle_of_word()
            && before.is_some_and(is_ai_inline_word_char)
            && after.is_some_and(is_ai_inline_word_char)
        {
            return false;
        }

        if before.is_some_and(|ch| cfg.trigger_chars().contains(&ch)) {
            return true;
        }

        self.last_ai_inline_queue_at
            .map(|last| now.duration_since(last) >= Duration::from_millis(cfg.idle_trigger_ms()))
            .unwrap_or(true)
    }

    pub(super) fn flush_pending_ai_inline_completion(&mut self) {
        let Some(pending) = self.pending_ai_inline_request.as_ref() else {
            return;
        };
        let Some(cfg) = self.ai_config.inline_completion().cloned() else {
            self.cancel_ai_inline_completion();
            return;
        };
        if pending.queued_at.elapsed() < Duration::from_millis(cfg.debounce_ms()) {
            return;
        }
        let revision = pending.revision;
        self.pending_ai_inline_request = None;
        if let Some(token) = self.ai_inline_cancel_token.take() {
            token.cancel();
        }
        let cancel_token = CancellationToken::new();
        self.ai_inline_cancel_token = Some(cancel_token.clone());
        let api_url = cfg.provider.api_url.clone();
        let api_key = cfg.provider.api_key.clone();
        let model = cfg.provider.model.clone();
        let endpoint_kind = cfg.provider.endpoint_kind.clone();
        let max_tokens = cfg.max_tokens();

        let text = self.app_state.text_string();
        let cursor = self.app_state.cursor_char_idx();
        let prefix_take = cfg.prefix_chars();
        let suffix_take = cfg.suffix_chars();
        let prefix: String = text.chars().take(cursor).collect();
        let suffix: String = text.chars().skip(cursor).take(suffix_take).collect();
        let prefix_chars: Vec<char> = prefix.chars().collect();
        let prefix = prefix_chars
            .iter()
            .skip(prefix_chars.len().saturating_sub(prefix_take))
            .collect::<String>();
        if prefix.trim().is_empty() && suffix.trim().is_empty() {
            self.cancel_ai_inline_completion();
            return;
        }
        let language_id = self.app_state.active_file().map(language_id_for_path);
        self.last_ai_inline_submit_at = Some(Instant::now());
        self.submit(RequestSpec {
            revision_id: revision,
            topic: RequestTopic::AiInlineCompletion,
            payload: WorkerRequestPayload::AiInlineCompletionRequest {
                api_url,
                api_key,
                model,
                endpoint_kind,
                prefix,
                suffix,
                language_id,
                file_path: self.app_state.active_file().map(PathBuf::from),
                max_tokens,
                cancel_token,
            },
        });
    }

    pub(super) fn next_ai_inline_flush_deadline(&self) -> Option<Instant> {
        let pending = self.pending_ai_inline_request.as_ref()?;
        let cfg = self.ai_config.inline_completion()?;
        Some(pending.queued_at + Duration::from_millis(cfg.debounce_ms()))
    }

    pub(super) fn submit_active_palette_fzf_search(&mut self) {
        let Some(mode) = self.app_state.command_palette_mode() else {
            return;
        };
        let Some(search_mode) = (match mode {
            CommandPaletteMode::FilePicker => Some(FzfSearchMode::FindFile),
            CommandPaletteMode::LiveGrep => Some(FzfSearchMode::LiveGrep),
            _ => None,
        }) else {
            return;
        };
        let Some(workspace_root) = self.app_state.workspace_root_path().map(PathBuf::from) else {
            return;
        };

        self.fzf_search_revision = self.fzf_search_revision.saturating_add(1);
        let query = self.app_state.command_palette_query_text().to_string();
        if query.trim().is_empty() {
            let _ = self
                .app_state
                .set_command_palette_results(mode, &query, Vec::new());
            return;
        }

        self.submit(RequestSpec {
            revision_id: self.fzf_search_revision,
            topic: RequestTopic::FzfSearch,
            payload: WorkerRequestPayload::FzfSearch {
                query,
                mode: search_mode,
                workspace_root,
                case_sensitive: search_mode == FzfSearchMode::LiveGrep
                    && self.app_state.live_grep_case_sensitive(),
            },
        });
    }

    pub(super) fn submit_fuzzy_picker_preview_load(&mut self) {
        if !self.app_state.active_buffer_is_fuzzy_picker() {
            return;
        }

        let action = self.app_state.command_palette_selected_action();
        let Some((path, target_line)) = (match action {
            Some(crate::app::command_palette::CommandPaletteAction::OpenFile(path)) => {
                Some((path, None))
            }
            Some(crate::app::command_palette::CommandPaletteAction::OpenSearchMatch {
                path,
                line,
                ..
            }) => Some((path, Some(line as usize))),
            _ => None,
        }) else {
            return;
        };

        self.submit(RequestSpec {
            revision_id: 0,
            topic: RequestTopic::FilePreview,
            payload: WorkerRequestPayload::LoadFilePreview {
                file_path: path,
                max_lines: 100,
                target_line,
            },
        });
    }

    pub(super) fn submit_references_preview_load(&mut self) {
        if !self.app_state.active_buffer_is_references() {
            return;
        }

        let Some(item) = self.app_state.selected_reference_item_cloned() else {
            return;
        };

        self.submit(RequestSpec {
            revision_id: 0,
            topic: RequestTopic::FilePreview,
            payload: WorkerRequestPayload::LoadFilePreview {
                file_path: item.path,
                max_lines: 100,
                target_line: Some(item.line + 1),
            },
        });
    }

    pub(super) fn submit_diagnostics_preview_load(&mut self) {
        if !self.app_state.active_buffer_is_diagnostics() {
            return;
        }

        let Some(item) = self.app_state.selected_diagnostic_item_cloned() else {
            return;
        };

        self.submit(RequestSpec {
            revision_id: 0,
            topic: RequestTopic::FilePreview,
            payload: WorkerRequestPayload::LoadFilePreview {
                file_path: item.file_path,
                max_lines: 100,
                target_line: Some(item.line + 1),
            },
        });
    }

    pub(super) fn sync_in_file_search_with_palette_query(&mut self) -> bool {
        if !matches!(
            self.app_state.command_palette_mode(),
            Some(CommandPaletteMode::InFileSearch)
        ) {
            return false;
        }

        let query = self.app_state.command_palette_query_text().to_string();

        // When terminal search palette is active, search the terminal grid
        // directly. We check `terminal_search_palette_active` FIRST because
        // the bottom panel terminal isn't the active buffer, so
        // `active_buffer_is_terminal()` would return false.
        if self.terminal_search_palette_active {
            // Access active tab's grid directly — not through
            // focused_terminal_grid_mut(), because focus is on OverlayLayer.
            let Some(tab) = self.active_terminal_tab_mut() else {
                return false;
            };
            let grid = &mut tab.grid;
            grid.search_in_terminal(&query, false);
            let _ = grid.search_next();
            self.mark_focused_terminal_layout_dirty();
            return true;
        }

        self.app_state.set_in_file_search_query(&query)
    }

    fn queue_lsp_server_start(&mut self, desired: ActiveLspServer) -> bool {
        if self.pending_lsp_server.as_ref() == Some(&desired) {
            return false;
        }
        if self.active_lsp_server.as_ref() == Some(&desired) && self.pending_lsp_server.is_none() {
            return false;
        }

        self.pending_lsp_server = Some(desired.clone());
        self.submit(RequestSpec {
            revision_id: 0,
            topic: RequestTopic::LspClient,
            payload: WorkerRequestPayload::StartLspServer {
                root_path: desired.root_path,
                server_command: Some(desired.server_name),
            },
        });
        true
    }

    pub(super) fn sync_lsp_server_for_workspace(&mut self) -> bool {
        match self.desired_lsp_server_for_active_file() {
            Some(desired) => self.queue_lsp_server_start(desired),
            None => {
                let had_lsp = self.active_lsp_server.take().is_some()
                    || self.pending_lsp_server.take().is_some();
                if had_lsp {
                    self.submit(RequestSpec {
                        revision_id: 0,
                        topic: RequestTopic::LspClient,
                        payload: WorkerRequestPayload::StopLspServer,
                    });
                }
                had_lsp
            }
        }
    }

    fn desired_lsp_server_for_active_file(&self) -> Option<ActiveLspServer> {
        let Some(path) = self.app_state.active_file() else {
            return None;
        };
        let profile = crate::lsp::registry::language_profile_for_path(path)?;
        if profile.lsp_binary.is_empty() {
            return None;
        }
        let server_name = profile.lsp_binary.to_string();
        let root_path = crate::lsp::registry::find_project_root(path, profile.root_markers);

        Some(ActiveLspServer {
            server_name,
            root_path,
        })
    }

    pub(super) fn submit_lsp_did_open_for_active_file(&mut self) {
        self.pending_lsp_document_sync = None;
        let Some(path) = self.app_state.active_file() else {
            return;
        };
        let Some(desired) = self.desired_lsp_server_for_active_file() else {
            return;
        };
        if self.active_lsp_server.as_ref() != Some(&desired) {
            let _ = self.queue_lsp_server_start(desired);
            return;
        }

        let version = self.app_state.revision().min(i32::MAX as u64) as i32;
        self.submit(RequestSpec {
            revision_id: self.app_state.revision(),
            topic: RequestTopic::LspClient,
            payload: WorkerRequestPayload::LspDidOpen {
                uri: path_to_lsp_uri(path),
                language_id: language_id_for_path(path),
                version,
                text: self.app_state.text_string(),
            },
        });
    }

    pub(super) fn submit_lsp_did_change_for_active_file(&mut self) {
        self.pending_lsp_document_sync = None;
        let Some(path) = self.app_state.active_file() else {
            return;
        };
        let Some(desired) = self.desired_lsp_server_for_active_file() else {
            return;
        };
        if self.active_lsp_server.as_ref() != Some(&desired) {
            let _ = self.queue_lsp_server_start(desired);
            return;
        }

        let version = self.app_state.revision().min(i32::MAX as u64) as i32;
        self.submit(RequestSpec {
            revision_id: self.app_state.revision(),
            topic: RequestTopic::LspClient,
            payload: WorkerRequestPayload::LspDidChange {
                uri: path_to_lsp_uri(path),
                version,
                text: self.app_state.text_string(),
            },
        });
    }

    pub(super) fn force_flush_lsp_did_change_for_active_file(&mut self) -> bool {
        self.pending_lsp_document_sync = None;
        let Some(path) = self.app_state.active_file() else {
            return false;
        };
        let Some(desired) = self.desired_lsp_server_for_active_file() else {
            return false;
        };
        if self.active_lsp_server.as_ref() != Some(&desired) {
            let _ = self.queue_lsp_server_start(desired);
            return false;
        }

        let version = self.app_state.revision().min(i32::MAX as u64) as i32;
        // Use LspDidOpen instead of LspDidChange: the worker already handles both
        // "register + open" and "already open → change" in a single payload, so
        // the first call after server start doesn't silently drop the sync because
        // the document hasn't been registered yet.
        self.submit(RequestSpec {
            revision_id: self.app_state.revision(),
            topic: RequestTopic::LspClient,
            payload: WorkerRequestPayload::LspDidOpen {
                uri: path_to_lsp_uri(path),
                language_id: language_id_for_path(path),
                version,
                text: self.app_state.text_string(),
            },
        });
        true
    }

    pub(super) fn queue_lsp_did_change_for_active_file(&mut self) {
        let Some(path) = self.app_state.active_file().map(PathBuf::from) else {
            self.pending_lsp_document_sync = None;
            return;
        };
        if self.desired_lsp_server_for_active_file().is_none() {
            self.pending_lsp_document_sync = None;
            return;
        }

        self.pending_lsp_document_sync = Some(PendingLspDocumentSync {
            path,
            revision: self.app_state.revision(),
            queued_at: Instant::now(),
        });
    }

    pub(super) fn flush_pending_lsp_did_change_after_debounce(&mut self) {
        let Some(pending) = self.pending_lsp_document_sync.as_ref() else {
            return;
        };

        if pending.queued_at.elapsed() < LSP_DIAGNOSTIC_DEBOUNCE_INTERVAL {
            return;
        }

        let active_path = self.app_state.active_file().map(PathBuf::from);
        if active_path.as_ref() != Some(&pending.path)
            || self.app_state.revision() != pending.revision
        {
            self.pending_lsp_document_sync = None;
            return;
        }

        let _ = self.force_flush_lsp_did_change_for_active_file();
    }

    pub(super) fn next_lsp_did_change_flush_deadline(&self) -> Option<Instant> {
        self.pending_lsp_document_sync
            .as_ref()
            .map(|pending| pending.queued_at + LSP_DIAGNOSTIC_DEBOUNCE_INTERVAL)
    }

    /// Submit async task kiểm tra xem LSP binary cho `path` có được cài chưa.
    ///
    /// Nếu extension không có trong registry, request bị skip (worker sẽ fail
    /// silently — không hiển thị lỗi ra UI).
    pub(super) fn submit_lsp_check_for_path(&self, path: PathBuf) {
        let Some(profile) = crate::lsp::registry::language_profile_for_path(&path) else {
            return;
        };
        self.submit_runtime_detection_for_profile(profile, &path);
        if profile.lsp_binary.is_empty() {
            return;
        }

        self.submit(RequestSpec {
            revision_id: 0,
            topic: RequestTopic::LspCheck,
            payload: WorkerRequestPayload::CheckLspForPath { path },
        });
    }

    /// Submit workspace/symbol request to pre-index symbols for fast completion.
    pub(super) fn submit_workspace_symbol_indexing(&self, language_id: String) {
        if matches!(
            language_id.as_str(),
            "typescript" | "tsx" | "javascript" | "jsx"
        ) {
            if let Some(workspace_root) = self.app_state.workspace_root_path().map(PathBuf::from) {
                self.app_state
                    .workspace_symbol_cache()
                    .set_indexing_progress(&language_id, 0, 0);
                eprintln!(
                    "[AppShell] starting TS/JS workspace symbol index for {} in {}",
                    language_id,
                    workspace_root.display()
                );
                self.submit(RequestSpec {
                    revision_id: 0,
                    topic: RequestTopic::SystemTask,
                    payload: WorkerRequestPayload::WorkspaceExportIndexRequest {
                        language_id,
                        workspace_root,
                    },
                });
            }
            return;
        }
        self.submit(RequestSpec {
            revision_id: 0,
            topic: RequestTopic::LspRequest,
            payload: WorkerRequestPayload::WorkspaceSymbolRequest {
                language_id,
                query: String::new(), // Empty query returns all symbols
            },
        });
    }

    fn submit_runtime_detection_for_profile(
        &self,
        profile: &crate::lsp::registry::LanguageProfile,
        path: &Path,
    ) {
        let should_detect_runtime = matches!(
            profile.key,
            "javascript"
                | "jsx"
                | "typescript"
                | "tsx"
                | "go"
                | "python"
                | "sql"
                | "yaml"
                | "dockerfile"
                | "json"
                | "bash"
        );
        if !should_detect_runtime {
            return;
        }

        let workspace_root = self
            .app_state
            .workspace_root_path()
            .map(PathBuf::from)
            .or_else(|| path.parent().map(PathBuf::from))
            .unwrap_or_default();
        self.submit(RequestSpec {
            revision_id: 0,
            topic: RequestTopic::SystemTask,
            payload: WorkerRequestPayload::DetectRuntimeVersions {
                python_binary: self.selected_python_env.clone(),
                workspace_root,
            },
        });
    }

    pub(super) fn mark_explorer_dirty(&mut self) {
        self.explorer_snapshot_dirty = true;
        self.sidebar_needs_layout = true;
    }

    pub(super) fn ensure_explorer_snapshot(&mut self) {
        if !self.explorer_snapshot_dirty {
            return;
        }

        let entries = collect_explorer_entries(&self.app_state);
        self.explorer_cursor = if entries.is_empty() {
            0
        } else if let Some(selected_path) = self.app_state.workspace_selected_path() {
            entries
                .iter()
                .position(|entry| entry.path == selected_path)
                .unwrap_or(self.explorer_cursor.min(entries.len() - 1))
        } else {
            self.explorer_cursor.min(entries.len() - 1)
        };
        self.explorer_snapshot = ExplorerSnapshot { entries };
        self.explorer_snapshot_dirty = false;
    }

    pub(super) fn sync_explorer_expanded_with_workspace(&mut self) {
        let Some(_root) = self.app_state.workspace_root_path().map(PathBuf::from) else {
            self.explorer_cursor = 0;
            self.mark_explorer_dirty();
            return;
        };
        self.mark_explorer_dirty();
    }

    /// Reveal `file_path` trong Explorer tree:
    /// 1. Expand tất cả folder cha từ workspace root đến thư mục chứa file.
    /// 2. Rebuild snapshot.
    /// 3. Đặt `explorer_cursor` trỏ đến entry của file đó.
    /// 4. Đánh dấu sidebar cần re-render.
    ///
    /// Gọi sau khi mở file từ File Picker để tree tự động sync với file đang mở.
    pub(super) fn explorer_reveal_file(&mut self, file_path: &Path) {
        if self.app_state.workspace_root_path().is_none() {
            return;
        }
        let expanded = self.app_state.workspace_expand_to_path(file_path);
        let selected = self
            .app_state
            .workspace_selected_path()
            .is_some_and(|selected| selected == file_path);
        if expanded || selected {
            self.mark_explorer_dirty();
        }
        if let Some(bounds) = self.last_sidebar_bounds
            && self.sync_explorer_scroll_to_selected(bounds)
        {
            self.sidebar_needs_layout = true;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::command_palette::CommandPaletteMode;

    #[test]
    fn build_context_marks_center_fuzzy_picker_buffer() {
        let mut shell = AppShell::new_for_tests().expect("create app shell");
        let _ = shell.app_state.apply_mode_event(ModeEvent::EnterInsert);
        shell
            .app_state
            .open_fuzzy_picker_buffer(CommandPaletteMode::LiveGrep);

        let context = shell.build_context();

        assert_eq!(context.mode, EditorMode::Insert);
        assert_eq!(context.focus, InputFocusContext::FuzzyPicker);
        assert!(!context.command_palette_visible);
    }

    #[test]
    fn build_context_uses_welcome_focus_even_when_sidebar_is_focused() {
        let mut shell = AppShell::new_for_tests().expect("create app shell");
        let _ = shell.focus_manager.set(FocusTarget::RightSidebar);

        let context = shell.build_context();

        assert_eq!(context.focus, InputFocusContext::Welcome);
        assert!(context.welcome_visible);
    }

    #[test]
    fn submit_parse_for_small_rust_buffer_runs_inline_tree_sitter() {
        let file_path = std::env::temp_dir().join(format!(
            "netherize_inline_highlight_{}.rs",
            std::process::id()
        ));
        std::fs::write(
            &file_path,
            "const MAX_SIZE: usize = 32;\nfn main() { println!(\"{}\", MAX_SIZE); }\n",
        )
        .expect("write rust fixture");

        let mut shell = AppShell::new_for_tests().expect("create app shell");
        shell
            .app_state
            .open_file(file_path.clone())
            .expect("open rust fixture");

        shell.submit_parse_for_active_buffer(true);

        assert!(
            !shell.highlight_spans.is_empty(),
            "expected inline tree-sitter spans"
        );
        assert_eq!(
            shell.active_highlight_request_revision, 0,
            "small rust buffers should not wait for async worker"
        );
        assert!(shell.syntax_engine.is_some());

        let _ = std::fs::remove_file(file_path);
    }
}
