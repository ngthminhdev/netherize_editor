use super::*;

impl AppShell {
    pub(super) fn default_terminal_working_dir(&self) -> Option<PathBuf> {
        self.app_state
            .workspace_root_path()
            .map(PathBuf::from)
            .or_else(|| {
                std::env::current_dir()
                    .ok()
                    .filter(|path| path != &PathBuf::from("/"))
            })
    }

    fn spawn_shell_for_terminal_tab(&mut self, tab_idx: usize) {
        let Some(tab) = self.terminal_tabs.get_mut(tab_idx) else {
            return;
        };
        if tab.session_id.is_some() {
            return;
        }
        tab.status = TerminalTabStatus::Running;
        let working_dir = self.default_terminal_working_dir();
        if let Some(request) = self.submit(RequestSpec {
            revision_id: 0,
            topic: RequestTopic::TerminalPty,
            payload: WorkerRequestPayload::SpawnPtyShell {
                shell: None,
                working_dir,
            },
        }) {
            self.pending_terminal_tab_spawns
                .insert(request.request_id, tab_idx);
        }
    }

    fn ensure_active_terminal_tab_spawned(&mut self) {
        let idx = self.active_terminal_tab;
        self.spawn_shell_for_terminal_tab(idx);
    }

    fn map_directional_focus_command(&self, command: &Command) -> Command {
        match command {
            Command::FocusLeft => match self.focus_manager.current() {
                FocusTarget::RightSidebar => Command::FocusEditor,
                FocusTarget::LeftSidebar => Command::FocusExplorer,
                _ => Command::FocusExplorer,
            },
            Command::FocusRight => match self.focus_manager.current() {
                FocusTarget::LeftSidebar => Command::FocusEditor,
                FocusTarget::RightSidebar => Command::FocusInspector,
                _ => Command::FocusInspector,
            },
            Command::FocusUp => Command::FocusEditor,
            Command::FocusDown => Command::FocusTerminal,
            _ => Command::FocusEditor,
        }
    }

    /// Scroll the terminal grid that currently owns focus (bottom panel, right
    /// sidebar opencode chat, or center buffer terminal). `half_page` scrolls by
    /// half the visible rows (Ctrl+U/Ctrl+D); otherwise it scrolls 3 lines.
    fn scroll_focused_terminal(&mut self, up: bool, half_page: bool) -> bool {
        let scrolled = if let Some(grid) = self.focused_terminal_grid_mut() {
            let lines = if half_page { (grid.rows / 2).max(1) } else { 3 };
            if up {
                grid.view_scroll_up(lines);
            } else {
                grid.view_scroll_down(lines);
            }
            true
        } else {
            false
        };
        if scrolled {
            self.mark_focused_terminal_layout_dirty();
        }
        scrolled
    }

    pub(super) fn handle_terminal_and_focus_command(&mut self, command: &Command) -> Option<bool> {
        match command {
            Command::ToggleTerminal => {
                let report = dispatch_command(&mut self.app_state, command.clone());
                let is_open = self.app_state.is_terminal_panel_open();
                let mut changed = report.request_redraw;
                if self.panel_state.bottom.visible != is_open {
                    self.panel_state.bottom.visible = is_open;
                    self.terminal_needs_layout = true;
                    changed = true;
                }
                if is_open {
                    self.terminal_needs_layout = true;
                    changed |= self.dismiss_initial_launch_welcome_if_active();
                }

                let focus_changed = if is_open {
                    let changed = self.focus_manager.set(FocusTarget::BottomPanel);
                    self.ensure_active_terminal_tab_spawned();
                    // Enter terminal focus mode to enable text input
                    if let Ok(result) = self.app_state.apply_mode_event(ModeEvent::FocusTerminal) {
                        changed || result.changed
                    } else {
                        changed
                    }
                } else if self.focus_manager.current() == FocusTarget::BottomPanel {
                    self.focus_manager.set(FocusTarget::CenterEditor)
                } else {
                    false
                };

                if focus_changed {
                    self.input_handler.clear_pending_prefix();
                }
                Some(changed || focus_changed)
            }
            Command::ToggleBottomDock => {
                let next_visible = !self.panel_state.bottom.visible;
                let mut changed = false;

                if self.panel_state.bottom.visible != next_visible {
                    self.panel_state.bottom.visible = next_visible;
                    changed = true;
                }
                changed |= self.app_state.set_terminal_panel_open(next_visible);
                self.terminal_needs_layout = true;

                if next_visible {
                    changed |= self.dismiss_initial_launch_welcome_if_active();
                    self.ensure_active_terminal_tab_spawned();
                }

                if !next_visible
                    && matches!(
                        self.app_state.current_mode(),
                        EditorMode::TerminalFocus | EditorMode::TerminalNormal
                    )
                {
                    if let Ok(result) = self.app_state.apply_mode_event(ModeEvent::ExitFocus) {
                        changed |= result.changed;
                    }
                }

                let focus_changed =
                    if !next_visible && self.focus_manager.current() == FocusTarget::BottomPanel {
                        self.focus_manager.set(FocusTarget::CenterEditor)
                    } else {
                        false
                    };

                if focus_changed {
                    self.input_handler.clear_pending_prefix();
                }

                Some(changed || focus_changed)
            }
            Command::FocusEditor => {
                let mut changed = self.release_focus_mode_to_editor();
                let focus_changed = self.focus_manager.set(FocusTarget::CenterEditor);
                changed |= focus_changed;
                if focus_changed {
                    self.input_handler.clear_pending_prefix();
                }
                Some(changed)
            }
            Command::FocusBack => {
                let mut changed = self.release_focus_mode_to_editor();

                // In Zen Mode, FocusBack is a mode escape only: return the status
                // to NORMAL while preserving the currently maximized surface
                // (terminal, markdown preview, etc.) instead of forcing focus back
                // to the main editor.
                if self.panel_state.maximized_region.is_some() {
                    if matches!(
                        self.app_state.current_mode(),
                        EditorMode::Insert
                            | EditorMode::Visual
                            | EditorMode::MultiCursor
                            | EditorMode::MultiInsert
                    ) && let Ok(result) = self.app_state.apply_mode_event(ModeEvent::Escape)
                    {
                        changed |= result.changed;
                    }
                    return Some(changed);
                }

                let focus_changed = self.focus_manager.set(FocusTarget::CenterEditor);
                changed |= focus_changed;
                if focus_changed {
                    self.input_handler.clear_pending_prefix();
                }
                Some(changed)
            }
            Command::FocusExplorer => {
                let mut changed = self.release_focus_mode_to_editor();
                if self
                    .panel_state
                    .left
                    .switch_to_tab(crate::workbench::panel_state::PanelTabId::Explorer)
                {
                    changed = true;
                    self.sidebar_needs_layout = true;
                }
                if !self.panel_state.left.visible {
                    self.panel_state.left.visible = true;
                    changed = true;
                    self.sidebar_needs_layout = true;
                }
                changed |= self.dismiss_initial_launch_welcome_if_active();
                let focus_changed = self.focus_manager.set(FocusTarget::LeftSidebar);
                changed |= focus_changed;
                if focus_changed {
                    self.input_handler.clear_pending_prefix();
                }
                Some(changed)
            }
            Command::FocusOutline => {
                let mut changed = self.release_focus_mode_to_editor();
                if self
                    .panel_state
                    .left
                    .switch_to_tab(crate::workbench::panel_state::PanelTabId::Outline)
                {
                    changed = true;
                    self.sidebar_needs_layout = true;
                }
                if !self.panel_state.left.visible {
                    self.panel_state.left.visible = true;
                    changed = true;
                    self.sidebar_needs_layout = true;
                }
                changed |= self.dismiss_initial_launch_welcome_if_active();
                let focus_changed = self.focus_manager.set(FocusTarget::LeftSidebar);
                changed |= focus_changed;
                if focus_changed {
                    self.input_handler.clear_pending_prefix();
                }
                Some(changed)
            }
            Command::FocusInspector => {
                let right_has_focus = self.focus_manager.current() == FocusTarget::RightSidebar;
                let mut changed = false;

                if right_has_focus {
                    if self.panel_state.right.visible {
                        self.panel_state.right.visible = false;
                        changed = true;
                        self.sidebar_needs_layout = true;
                    }
                    if self.focus_manager.current() == FocusTarget::RightSidebar {
                        changed |= self.focus_manager.set(FocusTarget::CenterEditor);
                    }
                } else {
                    changed |= self.release_focus_mode_to_editor();
                    if !self.panel_state.right.visible {
                        self.panel_state.right.visible = true;
                        changed = true;
                        self.sidebar_needs_layout = true;
                    }
                    let focus_changed = self.focus_manager.set(FocusTarget::RightSidebar);
                    changed |= focus_changed;
                    if focus_changed {
                        self.input_handler.clear_pending_prefix();
                    }
                    // AI Chat tab: drop into the running agent terminal, or open
                    // the agent picker when none is running.
                    if self.panel_state.right.active_tab_id() == Some(PanelTabId::AiChat) {
                        if self.right_pty_session_id.is_some() || self.pending_right_pty_spawn {
                            if let Ok(result) =
                                self.app_state.apply_mode_event(ModeEvent::FocusTerminal)
                            {
                                changed |= result.changed;
                            }
                        } else {
                            self.open_ai_agent_chooser();
                            changed = true;
                        }
                    }
                }
                Some(changed)
            }
            Command::FocusTerminal => {
                let terminal_has_focus = matches!(
                    self.app_state.current_mode(),
                    EditorMode::TerminalFocus | EditorMode::TerminalNormal
                ) || self.focus_manager.current()
                    == FocusTarget::BottomPanel;
                let mut changed = false;
                let mut focus_changed = false;

                if terminal_has_focus {
                    // When F12 is pressed from the terminal itself, hide the panel
                    // and return to the editor/mode that owned focus before terminal focus.
                    if self.panel_state.bottom.visible {
                        self.panel_state.bottom.visible = false;
                        changed = true;
                    }
                    changed |= self.app_state.set_terminal_panel_open(false);
                    self.terminal_needs_layout = true;

                    if self.focus_manager.current() == FocusTarget::BottomPanel {
                        focus_changed = self.focus_manager.set(FocusTarget::CenterEditor);
                        changed |= focus_changed;
                    }

                    if matches!(
                        self.app_state.current_mode(),
                        EditorMode::TerminalFocus | EditorMode::TerminalNormal
                    ) && let Ok(result) = self.app_state.apply_mode_event(ModeEvent::ExitFocus)
                    {
                        changed |= result.changed;
                    }
                } else {
                    // From editor/sidebar/etc: ensure the terminal is visible and
                    // focus it. If it is already visible, keep it open.
                    if !self.panel_state.bottom.visible {
                        self.panel_state.bottom.visible = true;
                        changed = true;
                    }
                    changed |= self.app_state.set_terminal_panel_open(true);
                    self.terminal_needs_layout = true;
                    changed |= self.dismiss_initial_launch_welcome_if_active();
                    self.ensure_active_terminal_tab_spawned();

                    focus_changed = self.focus_manager.set(FocusTarget::BottomPanel);
                    changed |= focus_changed;

                    if let Ok(result) = self.app_state.apply_mode_event(ModeEvent::FocusTerminal) {
                        changed |= result.changed;
                    }
                }

                if focus_changed {
                    self.input_handler.clear_pending_prefix();
                }

                Some(changed)
            }
            Command::FocusLeft | Command::FocusRight | Command::FocusUp | Command::FocusDown => {
                let mapped = self.map_directional_focus_command(command);
                Some(self.handle_command(mapped))
            }
            Command::TerminalWriteInput(input) => {
                self.track_terminal_tab_input(input);
                self.forward_to_pty(input);
                self.caret_blink_visible = true;
                self.caret_blink_dirty = true;
                self.last_caret_blink_tick = std::time::Instant::now();
                Some(false)
            }
            Command::TerminalPaste => Some(self.handle_terminal_paste()),
            Command::TerminalScrollUp => Some(self.scroll_focused_terminal(true, false)),
            Command::TerminalScrollDown => Some(self.scroll_focused_terminal(false, false)),
            Command::TerminalScrollHalfPageUp => Some(self.scroll_focused_terminal(true, true)),
            Command::TerminalScrollHalfPageDown => Some(self.scroll_focused_terminal(false, true)),
            Command::TerminalSearchOpen => {
                let report = dispatch_command(&mut self.app_state, Command::OpenInFileSearch);
                if report.success {
                    let _ = self.dismiss_initial_launch_welcome_if_active();
                    self.terminal_search_palette_active = true;
                    self.arm_palette_ime_commit_suppression();
                    let focus_changed = self.focus_manager.set(FocusTarget::OverlayLayer);
                    if focus_changed {
                        self.input_handler.clear_pending_prefix();
                    }
                }
                Some(report.request_redraw)
            }
            Command::ToggleMaximizeFocus => {
                let current_focus = self.focus_manager.current();
                match self.panel_state.maximized_region {
                    None => {
                        // Maximize current region
                        self.panel_state.maximized_region = Some(current_focus);
                    }
                    Some(_) => {
                        // Restore normal layout
                        self.panel_state.maximized_region = None;
                    }
                }
                self.editor_needs_layout = true;
                self.editor_caret_needs_layout = false;
                self.sidebar_needs_layout = true;
                self.terminal_needs_layout = true;
                self.right_terminal_needs_layout = true;
                self.buffer_terminal_needs_layout = true;
                Some(true)
            }
            Command::MoveFocusCycle => {
                let changed = self.focus_manager.cycle_next(&self.panel_state);
                if changed {
                    self.input_handler.clear_pending_prefix();
                }
                Some(changed)
            }
            Command::NextPanelTab => Some(match self.focus_manager.current() {
                FocusTarget::BottomPanel => self.panel_state.switch_bottom_next_tab(),
                FocusTarget::LeftSidebar => self.panel_state.switch_left_next_tab(),
                FocusTarget::RightSidebar => self.panel_state.switch_right_next_tab(),
                _ => false,
            }),
            Command::PrevPanelTab => Some(match self.focus_manager.current() {
                FocusTarget::BottomPanel => self.panel_state.switch_bottom_prev_tab(),
                FocusTarget::LeftSidebar => self.panel_state.switch_left_prev_tab(),
                FocusTarget::RightSidebar => self.panel_state.switch_right_prev_tab(),
                _ => false,
            }),
            Command::TerminalTabNew => Some(self.handle_terminal_tab_new()),
            Command::TerminalTabClose => Some(self.handle_terminal_tab_close()),
            Command::SwitchTerminalTab(idx) => Some(self.handle_switch_terminal_tab(*idx)),
            Command::SwitchBottomTab(idx) => Some(self.panel_state.bottom.switch_to_index(*idx)),
            Command::SwitchRightTab(idx) => Some({
                let idx = *idx;
                if idx < self.panel_state.right.tabs.len() {
                    let _ = self.panel_state.right.switch_to_index(idx);

                    if !self.panel_state.right.visible {
                        self.panel_state.right.visible = true;
                    }

                    let focus_changed = self.focus_manager.set(FocusTarget::RightSidebar);
                    if focus_changed {
                        self.input_handler.clear_pending_prefix();
                    }

                    if self.panel_state.right.active_tab_id() == Some(PanelTabId::AiChat) {
                        if self.right_pty_session_id.is_some() || self.pending_right_pty_spawn {
                            if let Ok(result) =
                                self.app_state.apply_mode_event(ModeEvent::FocusTerminal)
                            {
                                let _ = result;
                            }
                        }
                    }

                    self.sidebar_needs_layout = true;
                    true
                } else {
                    false
                }
            }),
            Command::SwitchLeftTab(idx) => Some({
                let idx = *idx;
                if idx < self.panel_state.left.tabs.len() {
                    let mut changed = false;
                    let old_tab = self.panel_state.left.active_tab_id();
                    if self.panel_state.left.switch_to_index(idx) {
                        changed = true;
                    }

                    if !self.panel_state.left.visible {
                        self.panel_state.left.visible = true;
                        changed = true;
                    }

                    let focus_changed = self.focus_manager.set(FocusTarget::LeftSidebar);
                    changed |= focus_changed;
                    if focus_changed {
                        self.input_handler.clear_pending_prefix();
                    }

                    let new_tab = self.panel_state.left.active_tab_id();
                    if old_tab == Some(PanelTabId::Outline) && new_tab != Some(PanelTabId::Outline)
                    {
                        self.outline_selected = None;
                    }

                    self.sidebar_needs_layout = true;
                    let _ = changed;
                    true
                } else {
                    false
                }
            }),
            Command::RunTestCases => Some(self.handle_run_test_cases()),
            Command::AiAgentPickerNext => Some(self.ai_agent_picker_move(true)),
            Command::AiAgentPickerPrev => Some(self.ai_agent_picker_move(false)),
            Command::AiAgentPickerLaunch => Some(self.ai_agent_picker_launch()),
            Command::TestRunnerFocus => Some(self.handle_test_runner_focus()),
            Command::TestRunnerUnfocus => Some(self.handle_test_runner_unfocus()),
            Command::TestRunnerAddCase => Some({
                self.app_state.test_runner.add_case("{}", "null");
                if let Some(idx) = self.app_state.test_runner.selected {
                    self.handle_test_runner_edit_field(idx, crate::runner::TestField::Input)
                } else {
                    true
                }
            }),
            Command::TestRunnerDeleteCase => Some(self.handle_test_runner_delete()),
            Command::TestRunnerGenerateCases => Some(self.handle_test_runner_generate()),
            Command::TestRunnerNextCase => Some(self.app_state.test_runner.select_next()),
            Command::TestRunnerPrevCase => Some(self.app_state.test_runner.select_prev()),
            Command::TestRunnerToggleField => Some(self.app_state.test_runner.toggle_field()),
            Command::TestRunnerEditField => {
                let selected = self.app_state.test_runner.selected;
                let field = self.app_state.test_runner.focused_field;
                Some(if let Some(idx) = selected {
                    self.handle_test_runner_edit_field(idx, field)
                } else {
                    false
                })
            }
            Command::TestRunnerSelectCase(index) => {
                if *index >= self.app_state.test_runner.cases.len() {
                    Some(false)
                } else {
                    self.app_state.test_runner.selected = Some(*index);
                    Some(true)
                }
            }
            Command::TestRunnerOpenField {
                case_index,
                expected,
            } => {
                let field = if *expected {
                    crate::runner::TestField::Expected
                } else {
                    crate::runner::TestField::Input
                };
                Some(self.handle_test_runner_edit_field(*case_index, field))
            }
            Command::TestRunnerScroll(delta) => {
                Some(self.app_state.test_runner.scroll_cases(*delta))
            }
            _ => None,
        }
    }

    /// Open the right dock, switch to the Test Runner tab, and move focus to it.
    pub(in crate::app::event_loop) fn handle_test_runner_focus(&mut self) -> bool {
        self.panel_state.right.visible = true;
        self.panel_state
            .right
            .switch_to_tab(crate::workbench::panel_state::PanelTabId::TestRunner);
        self.focus_manager.set(FocusTarget::RightSidebar);
        true
    }

    /// Return focus from the Test Runner panel to the editor.
    fn handle_test_runner_unfocus(&mut self) -> bool {
        self.focus_manager.set(FocusTarget::CenterEditor);
        true
    }

    /// Delete the selected case (no-op if none selected).
    fn handle_test_runner_delete(&mut self) -> bool {
        let Some(idx) = self.app_state.test_runner.selected else {
            return false;
        };
        let removed = self.app_state.test_runner.remove_case(idx);
        if removed {
            self.app_state.persist_leetcode_cases_for_active_file();
        }
        removed
    }

    /// Kick off AI generation of 5 fresh test cases for the active problem.
    /// Reads the problem id from the file header, loads cached context, and
    /// submits a non-blocking worker request; results replace all cases.
    /// Vim-style edit của shell input line từ T-COPY mode (Warp-style).
    /// Dịch op sang readline sequence và ghi vào PTY — shell tự edit và echo
    /// lại nên không bao giờ desync. `then_insert` (i/a/c/s...) quay về
    /// TerminalFocus sau khi gửi bytes.
    pub(super) fn terminal_line_edit(
        &mut self,
        kind: crate::core::commands::TerminalLineEditKind,
        then_insert: bool,
    ) -> bool {
        use crate::core::commands::TerminalLineEditKind;
        let shell_line = self
            .focused_terminal_grid_mut()
            .is_some_and(|grid| grid.shell_line_editing_active());
        if !shell_line || self.focused_terminal_session_id().is_none() {
            self.show_transient_toast_kind(
                "Terminal Edit\nReturn to the command line (G) to edit — exit visual/scrollback first."
                    .to_string(),
                ToastKind::Info,
            );
            return true;
        }
        // `p` paste system clipboard (vim flow: v → y → p). Shell kill-ring
        // (Ctrl-Y) không chứa text đã yank bằng y nên không dùng ở đây;
        // ponytail: dd/dw + p không di chuyển được text qua kill-ring nữa,
        // dùng clipboard làm nguồn duy nhất.
        if kind == TerminalLineEditKind::YankFromKillRing {
            return self.handle_terminal_paste();
        }
        let bytes = kind.readline_bytes();
        if !bytes.is_empty() {
            self.forward_to_pty(bytes);
        }
        if then_insert {
            // Mirror đường thoát bằng Esc: clear search highlight + FocusTerminal.
            if let Some(grid) = self.focused_terminal_grid_mut() {
                grid.search_matches.clear();
                grid.search_cursor = 0;
            }
            let report = self.dispatch_command_with_focused_terminal(
                Command::SwitchMode(ModeEvent::FocusTerminal),
                1,
            );
            if report.state_changed {
                self.mark_focused_terminal_layout_dirty();
            }
        }
        true
    }

    /// Readline bytes cho motion khi T-COPY đang ở live prompt — shell cursor
    /// di chuyển thật thay vì virtual cursor. `None` → không phải shell motion.
    pub(super) fn shell_motion_bytes(command: &Command) -> Option<&'static str> {
        match command {
            Command::MoveLeft => Some("\u{1b}[D"),
            Command::MoveRight => Some("\u{1b}[C"),
            Command::MoveWordForward => Some("\u{1b}f"),
            Command::MoveWordBackward => Some("\u{1b}b"),
            // readline không có "end of word" — Alt-f là xấp xỉ gần nhất.
            Command::MoveWordEnd => Some("\u{1b}f"),
            Command::MoveToLineStart => Some("\u{1}"),
            Command::MoveToLineEnd => Some("\u{5}"),
            _ => None,
        }
    }

    fn handle_test_runner_generate(&mut self) -> bool {
        use crate::runner::leetcode_adapter::language_key_for_extension;
        use crate::runner::leetcode_cache::{cache_dir, load_cache_in, parse_header};

        let Some(header) = parse_header(&self.app_state.text_string()) else {
            self.show_transient_toast_kind(
                "Generate LeetCode Tests\nActive file has no LeetCode header — fetch a problem first."
                    .to_string(),
                ToastKind::Error,
            );
            return true;
        };
        let Some(language_key) = self
            .app_state
            .active_file()
            .and_then(|path| path.extension())
            .and_then(|ext| ext.to_str())
            .and_then(language_key_for_extension)
        else {
            self.show_transient_toast_kind(
                "Generate LeetCode Tests\nUnsupported language for this file.".to_string(),
                ToastKind::Error,
            );
            return true;
        };
        let Some(cache) = load_cache_in(&cache_dir(), &header.id) else {
            self.show_transient_toast_kind(
                format!(
                    "Generate LeetCode Tests\nNo cached context for problem {} — re-fetch it first.",
                    header.id
                ),
                ToastKind::Error,
            );
            return true;
        };
        if !self.ai_config.leetcode_ai_enabled() {
            self.show_transient_toast_kind(
                "Generate LeetCode Tests\nEnable LeetCode AI in Settings first.".to_string(),
                ToastKind::Error,
            );
            return true;
        }
        let Some(provider) = self
            .ai_config
            .leetcode_ai_provider()
            .cloned()
            .filter(|p| !p.api_url.trim().is_empty() && !p.model.trim().is_empty())
        else {
            self.show_transient_toast_kind(
                "Generate LeetCode Tests\nConfigure [leetcode.provider] in config/ai.toml first."
                    .to_string(),
                ToastKind::Error,
            );
            return true;
        };

        self.app_state.test_runner.is_generating = true;
        self.submit(RequestSpec {
            revision_id: 0,
            topic: RequestTopic::LeetCode,
            payload: WorkerRequestPayload::GenerateLeetCodeTests {
                cache,
                language_key: language_key.to_string(),
                provider,
                verify: self.ai_config.leetcode_verify_enabled(),
            },
        });
        self.show_transient_toast_kind(
            "Generate LeetCode Tests\nGenerating 5 test cases with AI…".to_string(),
            ToastKind::Info,
        );
        true
    }

    fn handle_test_runner_edit_field(
        &mut self,
        case_index: usize,
        field: crate::runner::TestField,
    ) -> bool {
        if case_index >= self.app_state.test_runner.cases.len() {
            return false;
        }
        self.app_state.test_runner.selected = Some(case_index);
        self.app_state.test_runner.focused_field = field;

        let return_buffer_index = self.app_state.active_buffer_index();
        let suffix = match field {
            crate::runner::TestField::Input => "input",
            crate::runner::TestField::Expected => "expected",
        };
        let scratch_path =
            std::env::temp_dir().join(format!("test-case-{}-{}.json", case_index + 1, suffix));

        let initial_text = match field {
            crate::runner::TestField::Input => &self.app_state.test_runner.cases[case_index].input,
            crate::runner::TestField::Expected => {
                &self.app_state.test_runner.cases[case_index].expected
            }
        };
        if let Err(err) = std::fs::write(&scratch_path, initial_text) {
            self.show_transient_toast(format!("Failed to write temp JSON file: {err}"));
            return false;
        }

        let canonical_scratch_path = scratch_path.canonicalize().unwrap_or(scratch_path.clone());
        self.app_state.test_field_edit = Some(crate::app::app_state::TestFieldEditSession {
            case_index,
            field,
            return_buffer_index,
            scratch_path: canonical_scratch_path,
        });

        let report = dispatch_command(&mut self.app_state, Command::OpenFile(scratch_path));
        if !report.success {
            self.show_transient_toast("Failed to open scratch file".to_string());
            self.app_state.test_field_edit = None;
            return false;
        }

        self.clear_highlight_layers();
        self.submit_active_buffer_git_baseline_refresh();
        self.submit_parse_for_active_buffer(true);
        self.submit_lsp_did_open_for_active_file();
        self.editor_needs_layout = true;
        self.editor_caret_needs_layout = false;

        self.focus_manager.set(FocusTarget::CenterEditor);
        self.request_redraw();
        true
    }

    /// Resolve the active file's interpreter, mark all cases running, and submit
    /// the async test-execution worker. Returns `true` (always redraw, to show
    /// the running/error state). Validation failures surface a toast + set
    /// `test_runner.launch_error` instead of submitting.
    fn handle_run_test_cases(&mut self) -> bool {
        // Always reveal + focus the panel so the user lands inside it.
        self.handle_test_runner_focus();

        // First use: seed an empty case and drop into edit mode to author it,
        // instead of erroring on "no cases".
        if self.app_state.test_runner.is_empty() {
            self.app_state.test_runner.add_case("{}", "null");
            if let Some(idx) = self.app_state.test_runner.selected {
                self.handle_test_runner_edit_field(idx, crate::runner::TestField::Input);
            }
            self.show_transient_toast_kind(
                "Test Runner\nType input, Tab to Expected, then F5 to run.".to_string(),
                ToastKind::Info,
            );
            return true;
        }

        if let Err(error) = self.app_state.test_runner.validate_cases_json() {
            self.show_transient_toast_kind(format!("Test Runner\n{error}"), ToastKind::Error);
            return true;
        }
        let Some(path) = self.app_state.active_file().map(PathBuf::from) else {
            self.app_state.test_runner.launch_error =
                Some("Save the file before running tests.".to_string());
            self.show_transient_toast_kind(
                "Test Runner\nOpen and save a source file before running.".to_string(),
                ToastKind::Error,
            );
            return true;
        };
        // Scratch binary path for compiled languages (Rust). Derived from the
        // file stem in the temp dir; overwritten each run (runs are sequential).
        let stem = path
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| "solution".to_string());
        let output_binary = std::env::temp_dir().join(format!("netherize_lc_{stem}"));

        let Some(plan) = crate::runner::resolve_run_plan(&path, &output_binary) else {
            let ext = path
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("?")
                .to_string();
            self.app_state.test_runner.launch_error = Some(format!("Unsupported language: .{ext}"));
            self.show_transient_toast_kind(
                format!("Test Runner\n.{ext} files aren't runnable yet."),
                ToastKind::Error,
            );
            return true;
        };

        let preview = plan.preview(&path);
        let working_dir = path.parent().map(|p| p.to_path_buf());
        let inputs: Vec<String> = self
            .app_state
            .test_runner
            .cases
            .iter()
            .map(|case| case.input.clone())
            .collect();

        self.app_state.test_runner.mark_all_running();
        self.app_state.test_runner.last_command_preview = Some(preview.clone());

        self.submit(RequestSpec {
            revision_id: 0,
            topic: RequestTopic::TestRunner,
            payload: WorkerRequestPayload::RunTestCases {
                compile: plan.compile,
                program: plan.program,
                args: plan.args,
                working_dir,
                inputs,
                timeout_ms: 10_000,
                command_preview: preview,
            },
        });
        true
    }

    /// Populate the LeetCode language picker with the available scaffolds,
    /// MRU-sorted (languages used most recently float to the top, the rest in
    /// their default order). The picker itself was opened by the dispatch arm.
    pub(super) fn refresh_leetcode_language_items(&mut self) {
        use crate::app::command_palette::CommandPaletteItem;

        let recent = &self.persistent_state.recent_leetcode_languages;
        let mut templates: Vec<&crate::runner::leetcode::LeetCodeTemplate> =
            crate::runner::leetcode::leetcode_templates()
                .iter()
                .collect();
        // Stable sort by MRU rank: recent keys keep their recency order, the
        // rest stay in the original default order (rank = usize::MAX).
        templates.sort_by_key(|t| {
            recent
                .iter()
                .position(|key| key == t.key)
                .unwrap_or(usize::MAX)
        });

        let items = templates
            .into_iter()
            .map(|t| CommandPaletteItem::leetcode_language(t.key, t.label, t.hint))
            .collect();
        self.app_state
            .open_leetcode_language_selector_with_items(items);
    }

    fn refresh_fetch_leetcode_language_items(&mut self, problem_input: &str) {
        use crate::app::command_palette::CommandPaletteItem;

        let recent = &self.persistent_state.recent_leetcode_languages;
        let mut templates: Vec<&crate::runner::leetcode::LeetCodeTemplate> =
            crate::runner::leetcode::leetcode_templates()
                .iter()
                .collect();
        templates.sort_by_key(|template| {
            recent
                .iter()
                .position(|key| key == template.key)
                .unwrap_or(usize::MAX)
        });
        let items = templates
            .into_iter()
            .map(|template| {
                CommandPaletteItem::leetcode_fetch_language(
                    problem_input,
                    template.key,
                    template.label,
                    template.hint,
                )
            })
            .collect();
        self.app_state
            .open_leetcode_language_selector_with_items(items);
    }

    pub(super) fn confirm_leetcode_problem_input(&mut self) -> bool {
        let input = self
            .app_state
            .command_palette_query_text()
            .trim()
            .to_string();
        if let Err(message) = crate::runner::leetcode_api::normalize_problem_input(&input) {
            self.show_transient_toast_kind(
                format!("Fetch LeetCode Problem\n{message}"),
                ToastKind::Error,
            );
            return true;
        }
        let language_key = self
            .app_state
            .active_file()
            .and_then(|path| path.extension())
            .and_then(|extension| extension.to_str())
            .and_then(crate::runner::leetcode_adapter::language_key_for_extension);
        if let Some(language_key) = language_key {
            let _ = self.app_state.close_command_palette();
            self.submit_leetcode_fetch(input, language_key.to_string());
        } else {
            self.refresh_fetch_leetcode_language_items(&input);
            self.arm_palette_ime_commit_suppression();
            self.focus_manager.set(FocusTarget::OverlayLayer);
        }
        true
    }

    pub(super) fn confirm_fetch_leetcode_language_selection(&mut self) -> bool {
        let (problem_input, language_key) = match self.app_state.command_palette_selected_action() {
            Some(CommandPaletteAction::FetchLeetCodeWithLanguage {
                problem_input,
                language_key,
            }) => (problem_input, language_key),
            _ => return false,
        };
        let _ = self.app_state.close_command_palette();
        self.persistent_state
            .push_recent_leetcode_language(&language_key);
        self.persistent_state.save();
        self.submit_leetcode_fetch(problem_input, language_key);
        true
    }

    pub(in crate::app::event_loop) fn submit_leetcode_fetch(
        &mut self,
        input: String,
        language_key: String,
    ) {
        let destination_dir = self
            .app_state
            .workspace_root_path()
            .map(PathBuf::from)
            .or_else(|| {
                self.app_state
                    .active_file()
                    .and_then(|path| path.parent().map(PathBuf::from))
            })
            .unwrap_or_else(|| std::env::temp_dir().join("netherize-leetcode"));
        let provider = self.ai_config.leetcode_ai_provider().cloned();
        self.submit(RequestSpec {
            revision_id: 0,
            topic: RequestTopic::LeetCode,
            payload: WorkerRequestPayload::FetchLeetCodeProblem {
                input,
                language_key,
                destination_dir,
                use_ai: self.ai_config.leetcode_ai_enabled(),
                provider,
            },
        });
        let _ = self.release_focus_mode_to_editor();
        self.focus_manager.set(FocusTarget::CenterEditor);
        self.show_transient_toast_kind(
            "Fetch LeetCode Problem\nFetching problem and examples...".to_string(),
            ToastKind::Info,
        );
    }

    /// Confirm the selected language: scaffold a runnable starter file, open it,
    /// and record the language as most-recently-used.
    pub(super) fn confirm_leetcode_language_selection(&mut self) -> bool {
        let language_key = match self.app_state.command_palette_selected_action() {
            Some(CommandPaletteAction::CreateLeetCodeFile(key)) => key,
            _ => return false,
        };
        let Some(template) = crate::runner::leetcode::leetcode_template(&language_key) else {
            return false;
        };

        // Close the picker and return focus to the editor regardless of outcome.
        let _ = self.app_state.close_command_palette();
        if let Ok(result) = self.app_state.apply_mode_event(ModeEvent::ExitFocus) {
            if result.changed {
                self.editor_needs_layout = true;
            }
        }
        self.focus_manager.set(FocusTarget::CenterEditor);
        self.input_handler.clear_pending_prefix();

        // Record the choice for the MRU cache before any early return.
        self.persistent_state
            .push_recent_leetcode_language(template.key);
        self.persistent_state.save();

        // Pick a directory: workspace root, else the active file's folder, else
        // a temp scratch dir. Create it if needed.
        let dir = self
            .app_state
            .workspace_root_path()
            .map(PathBuf::from)
            .or_else(|| {
                self.app_state
                    .active_file()
                    .and_then(|p| p.parent().map(PathBuf::from))
            })
            .unwrap_or_else(|| std::env::temp_dir().join("netherize-leetcode"));
        if let Err(err) = std::fs::create_dir_all(&dir) {
            self.show_transient_toast_kind(
                format!(
                    "New LeetCode File\nCould not create {}: {err}",
                    dir.display()
                ),
                ToastKind::Error,
            );
            return true;
        }

        let target = Self::unique_scaffold_path(&dir, template.extension);
        if let Err(err) = std::fs::write(&target, template.body) {
            self.show_transient_toast_kind(
                format!(
                    "New LeetCode File\nCould not write {}: {err}",
                    target.display()
                ),
                ToastKind::Error,
            );
            return true;
        }

        // Refresh the tree so the new file shows up, then open it with the same
        // setup the explorer uses (parse, git baseline, LSP didOpen).
        if let Err(err) = self.app_state.rescan_workspace() {
            eprintln!("[AppShell] workspace rescan after leetcode scaffold failed: {err}");
        }
        let report = dispatch_command(&mut self.app_state, Command::OpenFile(target.clone()));
        if report.success {
            self.clear_highlight_layers();
            self.submit_active_buffer_git_baseline_refresh();
            self.submit_parse_for_active_buffer(true);
            self.submit_lsp_did_open_for_active_file();
            if let Some(path) = self.app_state.active_file().map(PathBuf::from) {
                self.explorer_reveal_file(&path);
                self.submit_lsp_check_for_path(path);
            }
            let _ = self.release_focus_mode_to_editor();
            self.editor_needs_layout = true;
            self.editor_caret_needs_layout = false;
        }

        let file_name = target
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
        self.show_transient_toast_kind(
            format!(
                "New LeetCode File\n{} · {file_name} — press F5 to run.",
                template.label
            ),
            ToastKind::Info,
        );
        true
    }

    /// First non-existing `solution[.-N].<ext>` path inside `dir`.
    fn unique_scaffold_path(dir: &Path, ext: &str) -> PathBuf {
        let mut candidate = dir.join(format!("solution.{ext}"));
        let mut n = 2;
        while candidate.exists() {
            candidate = dir.join(format!("solution-{n}.{ext}"));
            n += 1;
        }
        candidate
    }

    fn track_terminal_tab_input(&mut self, input: &str) {
        if self.focus_manager.current() != FocusTarget::BottomPanel {
            return;
        }
        let Some(tab) = self.active_terminal_tab_mut() else {
            return;
        };

        match input {
            "\r" | "\n" | "\r\n" => {
                let command = tab.pending_input.trim();
                if !command.is_empty() {
                    tab.label = terminal_command_title(command, &tab.shell_label);
                    tab.pending_input.clear();
                    self.terminal_needs_layout = true;
                }
            }
            "\u{7f}" => {
                tab.pending_input.pop();
            }
            "\u{15}" => {
                tab.pending_input.clear();
            }
            _ => {
                if input.starts_with('\u{1b}') || input.chars().any(char::is_control) {
                    return;
                }
                tab.pending_input.push_str(input);
            }
        }
    }

    fn handle_terminal_tab_new(&mut self) -> bool {
        let mut g = TerminalGrid::new(120, 40);
        g.highlight_colors = HighlightColors::from_theme(&self.theme);
        let label = format!("terminal {}", self.terminal_tabs.len() + 1);
        self.terminal_tabs.push(TerminalTab::new(g, label));
        self.active_terminal_tab = self.terminal_tabs.len() - 1;
        self.terminal_needs_layout = true;
        self.spawn_shell_for_terminal_tab(self.active_terminal_tab);
        true
    }

    fn handle_terminal_tab_close(&mut self) -> bool {
        if self.terminal_tabs.len() <= 1 {
            return false;
        }
        let idx = self.active_terminal_tab;
        let session_id = self.terminal_tabs[idx].session_id;
        self.pending_terminal_tab_spawns
            .retain(|_, pending_idx| *pending_idx != idx);
        self.terminal_tabs.remove(idx);
        for pending_idx in self.pending_terminal_tab_spawns.values_mut() {
            if *pending_idx > idx {
                *pending_idx -= 1;
            }
        }
        if self.active_terminal_tab >= self.terminal_tabs.len() {
            self.active_terminal_tab = self.terminal_tabs.len().saturating_sub(1);
        }
        self.terminal_needs_layout = true;

        if let Some(sid) = session_id {
            self.submit(RequestSpec {
                revision_id: 0,
                topic: RequestTopic::TerminalPty,
                payload: WorkerRequestPayload::ClosePtySession { session_id: sid },
            });
        }
        true
    }

    fn handle_switch_terminal_tab(&mut self, idx: usize) -> bool {
        if idx >= self.terminal_tabs.len() {
            return false;
        }
        if self.active_terminal_tab == idx {
            return false;
        }
        self.active_terminal_tab = idx;
        self.terminal_needs_layout = true;
        self.ensure_active_terminal_tab_spawned();
        true
    }

    /// Handle search commands when in Terminal Normal Mode.
    ///
    /// Intercepts `SearchNext`, `SearchPrev`, and `SearchWordUnderCursor` so
    /// they operate on the terminal grid's scrollback text instead of the
    /// editor buffer.  Returns `None` when the current mode is not
    /// `TerminalNormal`, allowing the normal editor dispatch to proceed.
    pub(super) fn handle_terminal_search_command(&mut self, command: &Command) -> Option<bool> {
        if self.app_state.current_mode() != EditorMode::TerminalNormal {
            return None;
        }

        match command {
            Command::ClearSearchHighlights => {
                if let Some(grid) = self.focused_terminal_grid_mut() {
                    let had_matches = !grid.search_matches.is_empty();
                    grid.search_matches.clear();
                    grid.search_cursor = 0;
                    if had_matches {
                        self.mark_focused_terminal_layout_dirty();
                        return Some(true);
                    }
                }
                Some(false)
            }
            Command::SearchNext => {
                if let Some(grid) = self.focused_terminal_grid_mut() {
                    if grid.search_next().is_some() {
                        self.mark_focused_terminal_layout_dirty();
                        return Some(true);
                    }
                }
                Some(false)
            }
            Command::SearchPrev => {
                if let Some(grid) = self.focused_terminal_grid_mut() {
                    if grid.search_prev().is_some() {
                        self.mark_focused_terminal_layout_dirty();
                        return Some(true);
                    }
                }
                Some(false)
            }
            Command::SearchWordUnderCursor => {
                if let Some(grid) = self.focused_terminal_grid_mut() {
                    let word = word_at_virtual_cursor(grid);
                    if let Some(word) = word {
                        grid.search_in_terminal(&word, true);
                        let found = grid.search_next().is_some();
                        self.mark_focused_terminal_layout_dirty();
                        return Some(found);
                    }
                }
                Some(false)
            }
            _ => None,
        }
    }
}

/// Extract the word under the virtual cursor from a terminal grid.
///
/// Uses the grid's scrollback text and `virtual_cursor` position to find a
/// contiguous span of alphanumeric / underscore characters.  Returns `None`
/// when the cursor is not on a word character or the grid is empty.
fn terminal_command_title(command: &str, shell_label: &str) -> String {
    let trimmed = command.trim();
    if trimmed.is_empty() {
        return shell_label.to_string();
    }

    // Avoid turning common shell-only navigation/cleanup commands into a
    // permanent-looking task title.
    let first = trimmed.split_whitespace().next().unwrap_or(trimmed);
    if matches!(first, "cd" | "clear" | "exit" | "pwd") {
        return shell_label.to_string();
    }

    const MAX_TITLE_CHARS: usize = 32;
    let mut title = String::new();
    for (idx, ch) in trimmed.chars().enumerate() {
        if idx >= MAX_TITLE_CHARS {
            title.push('…');
            break;
        }
        title.push(ch);
    }
    title
}

fn word_at_virtual_cursor(grid: &crate::terminal::grid::TerminalGrid) -> Option<String> {
    let lines = grid.get_scrollback_text();
    let cursor = grid.virtual_cursor;
    if cursor.row >= lines.len() {
        return None;
    }
    let line = &lines[cursor.row];
    if line.is_empty() {
        return None;
    }

    let chars: Vec<char> = line.chars().collect();
    let col = cursor.col.min(chars.len().saturating_sub(1));
    if col >= chars.len() {
        return None;
    }
    if !chars[col].is_alphanumeric() && chars[col] != '_' {
        return None;
    }

    let mut start = col;
    while start > 0 && (chars[start - 1].is_alphanumeric() || chars[start - 1] == '_') {
        start -= 1;
    }

    let mut end = col + 1;
    while end < chars.len() && (chars[end].is_alphanumeric() || chars[end] == '_') {
        end += 1;
    }

    Some(chars[start..end].iter().collect())
}
