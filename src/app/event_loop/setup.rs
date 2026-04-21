use super::*;

impl AppShell {
    pub fn new() -> Result<Self, String> {
        let cwd = std::env::current_dir().unwrap_or_default();
        let save_path = PathBuf::new();

        let (scheduler, rx) = AsyncScheduler::new()?;
        let bridge = AppAsyncBridge::new(rx);

        let mut app_state = AppState::new(save_path.clone());
        let _ = app_state.apply_mode_event(ModeEvent::EnterNormal);
        if let Err(err) = app_state.attach_workspace(cwd.clone()) {
            eprintln!("[AppShell] workspace attach skipped: {err}");
        }

        let mut explorer_expanded = HashSet::new();
        if let Some(root) = app_state.workspace_root_path() {
            explorer_expanded.insert(root.to_path_buf());
        }
        let workspace_git_branch = app_state.workspace_root_path().and_then(detect_git_branch);

        let base_theme = ThemeConfig::load_active();
        let theme = base_theme.clone();
        let ui_config = UiConfig::load_active();
        let layout_engine = WorkbenchLayoutEngine::new(ui_config.layout);
        let mut panel_state = WorkbenchPanelState::default();
        panel_state.left.visible = ui_config.docks.left.visible;
        panel_state.left.size_px = ui_config.docks.left.size_px;
        panel_state.right.visible = if DEBUG_UI_ENABLED {
            ui_config.docks.right.visible
        } else {
            false
        };
        panel_state.right.size_px = ui_config.docks.right.size_px;
        panel_state.bottom.visible = ui_config.docks.bottom.visible;
        panel_state.bottom.size_px = ui_config.docks.bottom.size_px;
        panel_state.overlay_visible = ui_config.docks.overlay_visible;
        let _ = app_state.set_terminal_panel_open(panel_state.bottom.visible);
        let window_width = ui_config.window.width;
        let window_height = ui_config.window.height;

        Ok(Self {
            app_state,
            input_handler: InputHandler::new(),
            input_map: InputMap::new(save_path),
            scheduler,
            bridge: Some(bridge),
            pty_session_id: None,
            highlight_spans: Vec::new(),
            terminal_grid: TerminalGrid::new(120, 40),
            explorer_cursor: 0,
            explorer_expanded,
            explorer_snapshot: ExplorerSnapshot::default(),
            explorer_snapshot_dirty: true,
            workspace_git_branch,
            base_theme,
            theme,
            ui_config,
            runtime_scale: 0.0,
            layout_engine,
            panel_state,
            focus_manager: FocusManager::default(),
            overlay_manager: OverlayManager::default(),
            window: None,
            renderer: None,
            window_size: PhysicalSize::new(window_width, window_height),
            editor_needs_layout: true,
            editor_caret_needs_layout: false,
            sidebar_needs_layout: true,
            terminal_needs_layout: true,
            last_parse_submit_at: None,
            active_highlight_request_revision: 0,
            pending_parse_after_debounce: false,
            last_editor_bounds: None,
            last_sidebar_bounds: None,
            last_sidebar_focused: None,
            last_terminal_bounds: None,
            sidebar_selection_quads: Vec::new(),
        })
    }

    pub(super) fn startup_subsystems(&mut self) {
        let cwd = std::env::current_dir().unwrap_or_default();

        self.submit(RequestSpec {
            revision_id: 0,
            topic: RequestTopic::WorkspaceWatch,
            payload: WorkerRequestPayload::StartFileWatch {
                root_path: cwd.clone(),
            },
        });

        self.submit(RequestSpec {
            revision_id: 0,
            topic: RequestTopic::TerminalPty,
            payload: WorkerRequestPayload::SpawnPtyShell {
                shell: None,
                working_dir: Some(cwd.clone()),
            },
        });

        self.submit(RequestSpec {
            revision_id: 0,
            topic: RequestTopic::LspClient,
            payload: WorkerRequestPayload::StartLspServer {
                root_path: cwd,
                server_command: None,
            },
        });

        eprintln!(
            "[AppShell] subsystems started - profile={}",
            KeymapLoader::active_profile()
        );
    }

    pub(super) fn submit(&self, spec: RequestSpec) {
        if let Err(err) = self.scheduler.submit(spec) {
            eprintln!("[AppShell] scheduler submit failed: {err}");
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

    pub(super) fn update_runtime_scaling_for_window(&mut self, scale_factor: f64) {
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
            content_scale = content_scale.clamp(
                self.ui_config.window.min_content_scale,
                self.ui_config.window.max_content_scale,
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

        self.layout_engine.config = scaled_ui.layout;
        self.panel_state.left.size_px = scaled_ui.docks.left.size_px;
        self.panel_state.right.size_px = scaled_ui.docks.right.size_px;
        self.panel_state.bottom.size_px = scaled_ui.docks.bottom.size_px;

        self.theme = scaled_theme.clone();
        if let Some(renderer) = self.renderer.as_mut() {
            renderer.apply_theme(scaled_theme);
            renderer.apply_ui_config(&scaled_ui);
        }

        self.editor_needs_layout = true;
        self.editor_caret_needs_layout = false;
        self.sidebar_needs_layout = true;
        self.terminal_needs_layout = true;
        self.last_editor_bounds = None;
        self.last_sidebar_bounds = None;
        self.last_terminal_bounds = None;
    }

    pub(super) fn build_context(&self) -> KeybindingContext {
        let mode = self.app_state.current_mode();
        let focus = match self.focus_manager.current() {
            FocusTarget::LeftSidebar => InputFocusContext::Explorer,
            FocusTarget::RightSidebar => InputFocusContext::Inspector,
            FocusTarget::BottomPanel => {
                if mode == EditorMode::TerminalFocus {
                    InputFocusContext::Terminal
                } else {
                    InputFocusContext::BottomPanel
                }
            }
            _ => InputFocusContext::Editor,
        };
        KeybindingContext {
            mode,
            focus,
            command_palette_visible: self.app_state.is_command_palette_visible(),
        }
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

    pub(super) fn release_focus_mode_to_editor(&mut self) -> bool {
        let mut changed = false;

        if self.app_state.current_mode() == EditorMode::PaletteFocus {
            changed |= self.app_state.close_command_palette();
        }

        if matches!(
            self.app_state.current_mode(),
            EditorMode::PaletteFocus | EditorMode::TerminalFocus
        ) && let Ok(result) = self.app_state.apply_mode_event(ModeEvent::ExitFocus)
        {
            changed |= result.changed;
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

        if let Some(file_path) = self.app_state.active_file().map(PathBuf::from) {
            let viewport_line_count = self.editor_viewport_lines().max(1);
            self.active_highlight_request_revision =
                self.active_highlight_request_revision.saturating_add(1);
            self.pending_parse_after_debounce = false;
            self.submit(RequestSpec {
                revision_id: self.active_highlight_request_revision,
                topic: RequestTopic::ActiveBufferLayout,
                payload: WorkerRequestPayload::ParseAndHighlight {
                    file_path: Some(file_path),
                    text_snapshot: self.app_state.text_string(),
                    language_id: LanguageId::Rust,
                    buffer_revision: self.app_state.revision(),
                    viewport_line_start: self.app_state.scroll_line,
                    viewport_line_count,
                },
            });
            self.last_parse_submit_at = Some(std::time::Instant::now());
        } else {
            self.pending_parse_after_debounce = false;
        }
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

        self.submit_parse_for_active_buffer(true);
    }

    pub(super) fn submit_lsp_did_open_for_active_file(&self) {
        let Some(path) = self.app_state.active_file() else {
            return;
        };
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

    pub(super) fn mark_explorer_dirty(&mut self) {
        self.explorer_snapshot_dirty = true;
        self.sidebar_needs_layout = true;
    }

    pub(super) fn ensure_explorer_snapshot(&mut self) {
        if !self.explorer_snapshot_dirty {
            return;
        }

        let entries = collect_explorer_entries(&self.app_state, &self.explorer_expanded);
        self.explorer_cursor = if entries.is_empty() {
            0
        } else {
            self.explorer_cursor.min(entries.len() - 1)
        };
        self.explorer_snapshot = ExplorerSnapshot { entries };
        self.explorer_snapshot_dirty = false;
    }

    pub(super) fn sync_explorer_expanded_with_workspace(&mut self) {
        let Some(root) = self.app_state.workspace_root_path().map(PathBuf::from) else {
            self.explorer_expanded.clear();
            self.explorer_cursor = 0;
            self.mark_explorer_dirty();
            return;
        };

        let mut valid_folders: HashSet<PathBuf> = HashSet::new();
        valid_folders.insert(root.clone());
        if let Some(nodes) = self.app_state.workspace_nodes() {
            for node in nodes {
                if node.file_type == WorkspaceNodeType::Folder {
                    valid_folders.insert(node.path.clone());
                }
            }
        }

        self.explorer_expanded
            .retain(|path| valid_folders.contains(path));
        self.explorer_expanded.insert(root);
        self.mark_explorer_dirty();
    }
}
