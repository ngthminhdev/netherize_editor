use super::*;
use crate::{
    app::clipboard::ClipboardProvider, app::command_palette::CommandPaletteMode,
    core::command_dispatch::dispatch_command_with_clipboard_count,
};

fn dispatch_palette_overlay_command(
    app_state: &mut AppState,
    clipboard: &mut dyn ClipboardProvider,
    command: Command,
) -> crate::core::command_dispatch::DispatchReport {
    match command {
        Command::PasteSystemClipboard => {
            dispatch_command_with_clipboard(app_state, command, Some(clipboard))
        }
        _ => dispatch_command(app_state, command),
    }
}

impl AppShell {
    pub(super) fn handle_command(&mut self, command: Command) -> bool {
        self.handle_command_with_count(command, 1)
    }

    fn reconcile_highlight_spans_with_pending_edits(&mut self) {
        let edits = self.app_state.take_highlight_edits();
        if edits.is_empty() {
            return;
        }

        crate::syntax::highlight::apply_highlight_edits(&mut self.highlight_spans, &edits);
    }

    pub(super) fn handle_command_with_count(
        &mut self,
        command: Command,
        repeat_count: usize,
    ) -> bool {
        match &command {
            Command::ToggleTerminal => {
                let report = dispatch_command(&mut self.app_state, command);
                let is_open = self.app_state.is_terminal_panel_open();
                let mut changed = report.request_redraw;
                if self.panel_state.bottom.visible != is_open {
                    self.panel_state.bottom.visible = is_open;
                    self.terminal_needs_layout = true;
                    changed = true;
                }
                if is_open {
                    self.terminal_needs_layout = true;
                }

                let focus_changed = if is_open {
                    let changed = self.focus_manager.set(FocusTarget::BottomPanel);
                    if self.pty_session_id.is_none() {
                        let working_dir = self
                            .app_state
                            .active_file()
                            .and_then(|path| path.parent())
                            .map(PathBuf::from)
                            .or_else(|| std::env::current_dir().ok());
                        self.submit(RequestSpec {
                            revision_id: 0,
                            topic: RequestTopic::TerminalPty,
                            payload: WorkerRequestPayload::SpawnPtyShell {
                                shell: None,
                                working_dir,
                            },
                        });
                    }
                    changed
                } else if self.focus_manager.current() == FocusTarget::BottomPanel {
                    self.focus_manager.set(FocusTarget::CenterEditor)
                } else {
                    false
                };

                if focus_changed {
                    self.input_handler.clear_pending_prefix();
                }
                changed || focus_changed
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

                if next_visible && self.pty_session_id.is_none() {
                    let working_dir = self
                        .app_state
                        .active_file()
                        .and_then(|path| path.parent())
                        .map(PathBuf::from)
                        .or_else(|| std::env::current_dir().ok());
                    self.submit(RequestSpec {
                        revision_id: 0,
                        topic: RequestTopic::TerminalPty,
                        payload: WorkerRequestPayload::SpawnPtyShell {
                            shell: None,
                            working_dir,
                        },
                    });
                }

                if !next_visible && self.app_state.current_mode() == EditorMode::TerminalFocus {
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

                changed || focus_changed
            }
            Command::OpenFolder => self.open_folder_with_dialog(),
            Command::OpenRecentProjects => self.open_recent_projects_palette(),
            Command::OpenFilePicker
            | Command::OpenFileFinder
            | Command::OpenCommandPalette
            | Command::OpenVimCommand
            | Command::OpenWorkspaceSymbols
            | Command::OpenInFileSearch
            | Command::SearchInFiles => {
                let report = dispatch_command(&mut self.app_state, command);
                if report.success {
                    self.arm_palette_ime_commit_suppression();
                    if self.focus_manager.set(FocusTarget::OverlayLayer) {
                        self.input_handler.clear_pending_prefix();
                    }
                }
                report.request_redraw
            }
            Command::GitOpenLazygit => self.open_lazygit_buffer(),
            Command::GitBlameLine => self.submit_git_blame_line(),
            Command::FilePickerAppendQuery(_)
            | Command::FilePickerBackspaceQuery
            | Command::PasteSystemClipboard
                if self.app_state.current_mode() == EditorMode::PaletteFocus
                    && self.app_state.is_command_palette_visible() =>
            {
                let report = {
                    let (app_state, clipboard) = (&mut self.app_state, &mut self.clipboard);
                    dispatch_palette_overlay_command(app_state, clipboard, command)
                };
                if !report.success {
                    return report.request_redraw;
                }

                match self.app_state.command_palette_mode() {
                    Some(CommandPaletteMode::FilePicker | CommandPaletteMode::LiveGrep) => {
                        self.submit_active_palette_fzf_search();
                    }
                    Some(CommandPaletteMode::InFileSearch) => {
                        let _ = self.sync_in_file_search_with_palette_query();
                    }
                    _ => {}
                }

                report.request_redraw || report.state_changed
            }
            Command::BufferCloseCurrent => {
                if self.app_state.is_dirty() && self.app_state.active_file().is_some() {
                    return self.begin_dirty_buffer_close_confirmation();
                }
                self.close_current_buffer_now()
            }
            Command::CloseFilePicker => {
                let returns_to_explorer = matches!(
                    self.app_state.command_palette_mode(),
                    Some(
                        CommandPaletteMode::ExplorerCreateFile
                            | CommandPaletteMode::ExplorerCreateFolder
                            | CommandPaletteMode::ExplorerDeleteConfirm
                    )
                );
                let report = dispatch_command(&mut self.app_state, command);
                self.clear_palette_ime_commit_suppression();
                let focus_changed = if returns_to_explorer {
                    let _ = self.app_state.apply_mode_event(ModeEvent::ExitFocus);
                    self.focus_manager.set(FocusTarget::LeftSidebar)
                } else {
                    self.focus_manager.set(FocusTarget::CenterEditor)
                };
                if focus_changed {
                    self.input_handler.clear_pending_prefix();
                }
                report.request_redraw
            }
            Command::ToggleLeftDock => {
                let mut changed = self.panel_state.toggle_left();
                if changed {
                    self.sidebar_needs_layout = true;
                }
                let mut focus_changed = false;
                if self.panel_state.left.visible {
                    changed |= self.release_focus_mode_to_editor();
                    focus_changed = self.focus_manager.set(FocusTarget::LeftSidebar);
                    changed |= focus_changed;
                } else if self.focus_manager.current() == FocusTarget::LeftSidebar {
                    focus_changed = self.focus_manager.set(FocusTarget::CenterEditor);
                    changed |= focus_changed;
                }
                if focus_changed {
                    self.input_handler.clear_pending_prefix();
                }
                changed
            }
            Command::ExplorerCreateFile => self.open_prompt_overlay(
                crate::app::command_palette::CommandPaletteMode::ExplorerCreateFile,
            ),
            Command::ExplorerCreateFolder => self.open_prompt_overlay(
                crate::app::command_palette::CommandPaletteMode::ExplorerCreateFolder,
            ),
            Command::ExplorerDeleteNode => self.begin_explorer_delete_confirmation(),
            Command::FocusEditor | Command::FocusBack => {
                let mut changed = self.release_focus_mode_to_editor();
                let focus_changed = self.focus_manager.set(FocusTarget::CenterEditor);
                changed |= focus_changed;
                if focus_changed {
                    self.input_handler.clear_pending_prefix();
                }
                changed
            }
            Command::FocusExplorer => {
                let mut changed = self.release_focus_mode_to_editor();
                if !self.panel_state.left.visible {
                    self.panel_state.left.visible = true;
                    changed = true;
                    self.sidebar_needs_layout = true;
                }
                let focus_changed = self.focus_manager.set(FocusTarget::LeftSidebar);
                changed |= focus_changed;
                if focus_changed {
                    self.input_handler.clear_pending_prefix();
                }
                changed
            }
            Command::FocusInspector => {
                let mut changed = self.release_focus_mode_to_editor();
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
                changed
            }
            Command::FocusTerminal => {
                let mut changed = false;

                if self.app_state.current_mode() == EditorMode::PaletteFocus {
                    changed |= self.app_state.close_command_palette();
                    if let Ok(result) = self.app_state.apply_mode_event(ModeEvent::ExitFocus) {
                        changed |= result.changed;
                    }
                }

                if self.app_state.set_terminal_panel_open(true) {
                    changed = true;
                }
                if !self.panel_state.bottom.visible {
                    self.panel_state.bottom.visible = true;
                    changed = true;
                }
                self.terminal_needs_layout = true;

                if self.app_state.current_mode() != EditorMode::TerminalFocus
                    && let Ok(result) = self.app_state.apply_mode_event(ModeEvent::FocusTerminal)
                {
                    changed |= result.changed;
                }

                let focus_changed = self.focus_manager.set(FocusTarget::BottomPanel);
                changed |= focus_changed;
                if focus_changed {
                    self.input_handler.clear_pending_prefix();
                }

                if self.pty_session_id.is_none() {
                    let working_dir = self
                        .app_state
                        .active_file()
                        .and_then(|path| path.parent())
                        .map(PathBuf::from)
                        .or_else(|| std::env::current_dir().ok());
                    self.submit(RequestSpec {
                        revision_id: 0,
                        topic: RequestTopic::TerminalPty,
                        payload: WorkerRequestPayload::SpawnPtyShell {
                            shell: None,
                            working_dir,
                        },
                    });
                }

                changed
            }
            Command::FocusLeft | Command::FocusRight | Command::FocusUp | Command::FocusDown => {
                let mapped = self.map_directional_focus_command(&command);
                self.handle_command(mapped)
            }
            Command::TerminalWriteInput(input) => {
                self.forward_to_pty(input);
                false
            }
            Command::TerminalScrollUp => {
                if let Some(grid) = self.focused_terminal_grid_mut() {
                    grid.view_scroll_up(3);
                    if self.app_state.active_buffer_is_terminal() {
                        self.buffer_terminal_needs_layout = true;
                    } else {
                        self.terminal_needs_layout = true;
                    }
                    true
                } else {
                    false
                }
            }
            Command::TerminalScrollDown => {
                if let Some(grid) = self.focused_terminal_grid_mut() {
                    grid.view_scroll_down(3);
                    if self.app_state.active_buffer_is_terminal() {
                        self.buffer_terminal_needs_layout = true;
                    } else {
                        self.terminal_needs_layout = true;
                    }
                    true
                } else {
                    false
                }
            }
            Command::MoveFocusCycle => {
                let changed = self.focus_manager.cycle_next(&self.panel_state);
                if changed {
                    self.input_handler.clear_pending_prefix();
                }
                changed
            }
            Command::ExplorerMoveUp => {
                self.ensure_explorer_snapshot();
                let entries_len = self.explorer_snapshot.entries.len();
                if entries_len == 0 {
                    self.explorer_cursor = 0;
                    return false;
                }
                self.explorer_cursor = self.explorer_cursor.min(entries_len.saturating_sub(1));
                if self.explorer_cursor == 0 {
                    false
                } else {
                    self.explorer_cursor -= 1;
                    let _ = self.app_state.workspace_select_path(
                        &self.explorer_snapshot.entries[self.explorer_cursor].path,
                    );
                    self.sidebar_needs_layout = true;
                    true
                }
            }
            Command::ExplorerMoveDown => {
                self.ensure_explorer_snapshot();
                let entries_len = self.explorer_snapshot.entries.len();
                if entries_len == 0 {
                    self.explorer_cursor = 0;
                    return false;
                }
                self.explorer_cursor = self.explorer_cursor.min(entries_len.saturating_sub(1));
                if self.explorer_cursor + 1 >= entries_len {
                    false
                } else {
                    self.explorer_cursor += 1;
                    let _ = self.app_state.workspace_select_path(
                        &self.explorer_snapshot.entries[self.explorer_cursor].path,
                    );
                    self.sidebar_needs_layout = true;
                    true
                }
            }
            Command::ExplorerCollapseOrParent => {
                self.ensure_explorer_snapshot();
                if self.explorer_snapshot.entries.is_empty() {
                    self.explorer_cursor = 0;
                    return false;
                }
                self.explorer_cursor = self
                    .explorer_cursor
                    .min(self.explorer_snapshot.entries.len().saturating_sub(1));
                let selected = self.explorer_snapshot.entries[self.explorer_cursor].clone();

                if selected.file_type == WorkspaceNodeType::Folder && selected.is_expanded {
                    let changed = self.app_state.workspace_collapse_path(&selected.path);
                    if changed {
                        self.mark_explorer_dirty();
                    }
                    return changed;
                }

                let Some(parent_path) = selected.parent_path.as_ref() else {
                    return false;
                };
                if self.app_state.workspace_root_path() == Some(parent_path.as_path()) {
                    return false;
                }
                let mut changed = self.app_state.workspace_select_path(parent_path);
                changed |= self.app_state.workspace_collapse_path(parent_path);
                if changed {
                    self.mark_explorer_dirty();
                }
                changed
            }
            Command::ExplorerCollapseAllUnderNode => {
                self.ensure_explorer_snapshot();
                if self.explorer_snapshot.entries.is_empty() {
                    self.explorer_cursor = 0;
                    return false;
                }
                self.explorer_cursor = self
                    .explorer_cursor
                    .min(self.explorer_snapshot.entries.len().saturating_sub(1));
                let selected = self.explorer_snapshot.entries[self.explorer_cursor].clone();
                let root_path = self.app_state.workspace_root_path().map(PathBuf::from);
                let target_path = if selected.file_type == WorkspaceNodeType::Folder {
                    selected.path.clone()
                } else if let Some(parent_path) = selected.parent_path.as_ref() {
                    if root_path.as_deref() == Some(parent_path.as_path()) {
                        return false;
                    }
                    parent_path.clone()
                } else {
                    return false;
                };

                let mut changed = self.app_state.workspace_select_path(&target_path);
                changed |= self
                    .app_state
                    .workspace_collapse_path_and_descendants(&target_path);
                if changed {
                    self.mark_explorer_dirty();
                }
                changed
            }
            Command::ExplorerExpandNode => {
                self.ensure_explorer_snapshot();
                if self.explorer_snapshot.entries.is_empty() {
                    self.explorer_cursor = 0;
                    return false;
                }
                self.explorer_cursor = self
                    .explorer_cursor
                    .min(self.explorer_snapshot.entries.len().saturating_sub(1));
                let selected = self.explorer_snapshot.entries[self.explorer_cursor].clone();
                if selected.file_type != WorkspaceNodeType::Folder {
                    return false;
                }

                let changed = self.app_state.workspace_expand_path(&selected.path);
                if changed {
                    self.mark_explorer_dirty();
                }
                changed
            }
            Command::ExplorerExpandOrChild => {
                self.ensure_explorer_snapshot();
                if self.explorer_snapshot.entries.is_empty() {
                    self.explorer_cursor = 0;
                    return false;
                }
                self.explorer_cursor = self
                    .explorer_cursor
                    .min(self.explorer_snapshot.entries.len().saturating_sub(1));
                let selected = self.explorer_snapshot.entries[self.explorer_cursor].clone();
                if selected.file_type != WorkspaceNodeType::Folder {
                    return false;
                }

                if !selected.is_expanded {
                    let changed = self.app_state.workspace_expand_path(&selected.path);
                    if changed {
                        self.mark_explorer_dirty();
                    }
                    return changed;
                }

                let Some(first_child_idx) = self
                    .explorer_snapshot
                    .entries
                    .iter()
                    .position(|entry| entry.parent_path.as_ref() == Some(&selected.path))
                else {
                    return false;
                };

                if first_child_idx == self.explorer_cursor {
                    return false;
                }
                self.explorer_cursor = first_child_idx;
                let _ = self
                    .app_state
                    .workspace_select_path(&self.explorer_snapshot.entries[first_child_idx].path);
                self.sidebar_needs_layout = true;
                true
            }
            Command::ExplorerExpandAllUnderNode => {
                self.ensure_explorer_snapshot();
                if self.explorer_snapshot.entries.is_empty() {
                    self.explorer_cursor = 0;
                    return false;
                }
                self.explorer_cursor = self
                    .explorer_cursor
                    .min(self.explorer_snapshot.entries.len().saturating_sub(1));
                let selected = self.explorer_snapshot.entries[self.explorer_cursor].clone();
                if selected.file_type != WorkspaceNodeType::Folder {
                    return false;
                }

                let mut changed = self.app_state.workspace_select_path(&selected.path);
                changed |= self
                    .app_state
                    .workspace_expand_path_and_descendants(&selected.path);
                if changed {
                    self.mark_explorer_dirty();
                }
                changed
            }
            Command::ExplorerToggleOrOpen
            | Command::ExplorerExpandCollapse
            | Command::ExplorerOpenFile => {
                self.ensure_explorer_snapshot();
                if self.explorer_snapshot.entries.is_empty() {
                    self.explorer_cursor = 0;
                    return false;
                }
                self.explorer_cursor = self
                    .explorer_cursor
                    .min(self.explorer_snapshot.entries.len().saturating_sub(1));
                let selected = self.explorer_snapshot.entries[self.explorer_cursor].clone();
                if selected.file_type == WorkspaceNodeType::Folder {
                    if selected.is_expanded {
                        let changed = self.app_state.workspace_collapse_path(&selected.path);
                        if changed {
                            self.mark_explorer_dirty();
                        }
                        return changed;
                    }
                    let changed = self.app_state.workspace_expand_path(&selected.path);
                    if changed {
                        self.mark_explorer_dirty();
                    }
                    return changed;
                }

                let report = dispatch_command(
                    &mut self.app_state,
                    Command::OpenFile(selected.path.clone()),
                );
                if !report.success {
                    return false;
                }
                self.highlight_spans.clear();
                self.submit_parse_for_active_buffer(true);
                self.submit_lsp_did_open_for_active_file();
                let mut changed = report.request_redraw || report.state_changed;
                let focus_changed = self.focus_manager.set(FocusTarget::CenterEditor);
                changed |= focus_changed;
                changed |= self.release_focus_mode_to_editor();
                self.editor_needs_layout = true;
                self.editor_caret_needs_layout = false;
                if focus_changed {
                    self.input_handler.clear_pending_prefix();
                }
                changed
            }
            Command::NextPanelTab => match self.focus_manager.current() {
                FocusTarget::BottomPanel => self.panel_state.switch_bottom_next_tab(),
                FocusTarget::LeftSidebar => self.panel_state.switch_left_next_tab(),
                FocusTarget::RightSidebar => self.panel_state.switch_right_next_tab(),
                _ => false,
            },
            Command::PrevPanelTab => match self.focus_manager.current() {
                FocusTarget::BottomPanel => self.panel_state.switch_bottom_prev_tab(),
                FocusTarget::LeftSidebar => self.panel_state.switch_left_prev_tab(),
                FocusTarget::RightSidebar => self.panel_state.switch_right_prev_tab(),
                _ => false,
            },
            Command::ScrollHalfPageUp
            | Command::ScrollHalfPageDown
            | Command::CenterCursorLine
            | Command::MoveToFirstLine
            | Command::MoveToLastLine => {
                let viewport_lines = self.editor_viewport_lines();
                match command {
                    Command::ScrollHalfPageUp => {
                        self.app_state
                            .scroll_half_page_up((viewport_lines / 2).max(1));
                    }
                    Command::ScrollHalfPageDown => {
                        self.app_state
                            .scroll_half_page_down((viewport_lines / 2).max(1));
                    }
                    Command::CenterCursorLine => {
                        self.app_state.center_cursor_line(viewport_lines);
                    }
                    Command::MoveToFirstLine => {
                        // Treat `gg` like a viewport motion so it relayouts the
                        // editor the same way as an immediate follow-up `zz`.
                        self.app_state.move_to_first_line();
                        self.app_state.center_cursor_line(viewport_lines);
                    }
                    Command::MoveToLastLine => {
                        self.app_state.move_to_last_line();
                        let (cursor_line, _) = self.app_state.cursor_line_col();
                        // Scroll viewport so cursor sits near the bottom (scrolloff = 3).
                        let margin = 3usize;
                        self.app_state.scroll_line = if cursor_line + margin + 1 >= viewport_lines {
                            cursor_line + margin + 1 - viewport_lines
                        } else {
                            0
                        };
                    }
                    _ => {}
                }
                self.editor_needs_layout = true;
                self.editor_caret_needs_layout = false;
                self.submit_parse_for_active_buffer(true);
                true
            }
            // ── Leap / EasyMotion navigation ──────────────────────────────────
            Command::LeapStart => {
                self.input_handler.set_pending_leap_char();
                self.editor_caret_needs_layout = true; // làm status bar hiển thị pending state
                true
            }
            Command::LeapActivate(target_char) => {
                let labels = self.generate_leap_labels(*target_char);
                if labels.is_empty() {
                    self.leap_labels = None;
                    false
                } else {
                    self.input_handler.set_pending_leap_label();
                    self.leap_labels = Some(labels);
                    self.editor_needs_layout = true;
                    self.editor_caret_needs_layout = false;
                    true
                }
            }
            Command::LeapJump(label_char) => {
                let jumped = if let Some(labels) = self.leap_labels.take() {
                    if let Some((_, char_idx)) = labels.iter().find(|(lc, _)| *lc == *label_char) {
                        let changed = self.app_state.leap_jump_to_char(*char_idx);
                        if changed {
                            let viewport_lines = self.editor_viewport_lines();
                            let prev_scroll = self.app_state.scroll_line;
                            self.app_state.auto_scroll_to_cursor(viewport_lines);
                            if self.app_state.scroll_line != prev_scroll {
                                self.editor_needs_layout = true;
                                self.editor_caret_needs_layout = false;
                                self.submit_parse_for_active_buffer(true);
                            } else {
                                self.editor_caret_needs_layout = true;
                            }
                        }
                        changed
                    } else {
                        false
                    }
                } else {
                    false
                };
                jumped
            }
            Command::LeapCancel => {
                let had_labels = self.leap_labels.take().is_some();
                if had_labels {
                    self.editor_needs_layout = true;
                    self.editor_caret_needs_layout = false;
                }
                had_labels
            }
            Command::FilePickerConfirmSelection | Command::OpenFile(_) => {
                if matches!(&command, Command::FilePickerConfirmSelection)
                    && matches!(
                        self.app_state.command_palette_mode(),
                        Some(
                            CommandPaletteMode::ExplorerCreateFile
                                | CommandPaletteMode::ExplorerCreateFolder
                        )
                    )
                {
                    return self.confirm_explorer_prompt();
                }

                if matches!(&command, Command::FilePickerConfirmSelection)
                    && matches!(
                        self.app_state.command_palette_mode(),
                        Some(CommandPaletteMode::RecentProjects)
                    )
                {
                    return self.confirm_recent_project_selection();
                }

                if matches!(&command, Command::FilePickerConfirmSelection)
                    && matches!(
                        self.app_state.command_palette_mode(),
                        Some(CommandPaletteMode::InFileSearch)
                    )
                {
                    let report = dispatch_command(&mut self.app_state, command);
                    if report.state_changed {
                        let prev_scroll = self.app_state.scroll_line;
                        let viewport_lines = self.editor_viewport_lines();
                        self.app_state.auto_scroll_to_cursor(viewport_lines);
                        if self.app_state.scroll_line != prev_scroll {
                            self.editor_needs_layout = true;
                            self.editor_caret_needs_layout = false;
                        } else {
                            self.editor_caret_needs_layout = true;
                        }
                    }
                    if self.focus_manager.set(FocusTarget::CenterEditor) {
                        self.input_handler.clear_pending_prefix();
                    }
                    let _ = self.release_focus_mode_to_editor();
                    return report.request_redraw || report.success;
                }

                // Lưu lại active file TRƯỚC khi dispatch (để so sánh sau)
                let file_before = self.app_state.active_file().map(PathBuf::from);

                let is_open_file = matches!(&command, Command::OpenFile(_));
                let report = {
                    let (app_state, clipboard) = (&mut self.app_state, &mut self.clipboard);
                    dispatch_command_with_clipboard(app_state, command, Some(clipboard))
                };
                self.reconcile_highlight_spans_with_pending_edits();

                if report.success {
                    self.highlight_spans.clear();

                    // Lấy file vừa được mở
                    let file_after = self.app_state.active_file().map(PathBuf::from);

                    // Chỉ reveal khi file thực sự thay đổi
                    if let Some(ref path) = file_after
                        && file_after != file_before
                    {
                        self.explorer_reveal_file(path);
                    }

                    let viewport_lines = self.editor_viewport_lines();
                    self.app_state.auto_scroll_to_cursor(viewport_lines);

                    self.submit_lsp_did_open_for_active_file();
                    let _ = self.sync_focus_mode_for_active_buffer();
                }

                if report.state_changed {
                    self.editor_needs_layout = true;
                    self.editor_caret_needs_layout = false;
                }
                if report.state_changed || report.success {
                    self.submit_parse_for_active_buffer(true);
                }

                // Sau FilePickerConfirmSelection: đóng palette và về editor
                if !is_open_file {
                    if self.focus_manager.set(FocusTarget::CenterEditor) {
                        self.input_handler.clear_pending_prefix();
                    }
                    let _ = self.release_focus_mode_to_editor();
                }

                report.request_redraw || report.success
            }
            _ => {
                let should_notify_did_open = matches!(
                    &command,
                    Command::BufferNext | Command::BufferPrev | Command::BufferCloseCurrent
                );

                let is_cursor_move = matches!(
                    &command,
                    Command::MoveLeft
                        | Command::MoveRight
                        | Command::MoveUp
                        | Command::MoveDown
                        | Command::MoveWordForward
                        | Command::MoveWordBackward
                        | Command::MoveWordEnd
                        | Command::MoveToLineStart
                        | Command::MoveToLineEnd
                        | Command::MoveToFirstNonWhitespace
                        | Command::InsertAtLineStart
                        | Command::AppendAtLineEnd
                        | Command::AppendAfterCursor
                        | Command::EnterVisualLine
                        | Command::SearchNext
                        | Command::SearchPrev
                        | Command::SearchWordUnderCursor
                );
                let should_reparse = matches!(
                    &command,
                    Command::InsertChar(_)
                        | Command::InsertText(_)
                        | Command::Newline
                        | Command::Backspace
                        | Command::InsertLineBelow
                        | Command::InsertLineAbove
                        | Command::SubstituteLine
                        | Command::DeleteChar
                        | Command::DeleteSelection
                        | Command::DeleteCurrentLine
                        | Command::ToggleLineComment
                        | Command::ToggleSelectionComment
                        | Command::DeleteWordForward
                        | Command::DeleteWordBackward
                        | Command::ChangeSelection
                        | Command::ChangeWordForward
                        | Command::ChangeWordBackward
                        | Command::PasteAfter
                        | Command::PasteBefore
                        | Command::PasteSystemClipboard
                        | Command::Undo
                        | Command::Redo
                        | Command::ReplaceChar(_)
                        | Command::BufferNew
                        | Command::BufferNext
                        | Command::BufferPrev
                        | Command::BufferCloseCurrent
                );
                let is_typing_edit = matches!(
                    &command,
                    Command::InsertChar(_) | Command::InsertText(_) | Command::Backspace
                );
                let report = {
                    let (app_state, clipboard) = (&mut self.app_state, &mut self.clipboard);
                    dispatch_command_with_clipboard_count(
                        app_state,
                        command,
                        repeat_count,
                        Some(clipboard),
                    )
                };
                self.reconcile_highlight_spans_with_pending_edits();
                if report.success && should_notify_did_open {
                    self.highlight_spans.clear();
                    self.mark_explorer_dirty();
                    let _ = self.sync_focus_mode_for_active_buffer();
                }

                if report.state_changed && is_cursor_move {
                    let prev_scroll = self.app_state.scroll_line;
                    let viewport_lines = self.editor_viewport_lines();
                    self.app_state.auto_scroll_to_cursor(viewport_lines);
                    if self.app_state.scroll_line != prev_scroll {
                        self.editor_needs_layout = true;
                        self.editor_caret_needs_layout = false;
                        self.submit_parse_for_active_buffer(true);
                    } else {
                        self.editor_caret_needs_layout = true;
                    }
                } else if report.state_changed {
                    self.editor_needs_layout = true;
                    self.editor_caret_needs_layout = false;
                }
                if report.state_changed && should_reparse {
                    self.submit_parse_for_active_buffer(!is_typing_edit);
                }
                if report.success && should_notify_did_open {
                    self.submit_lsp_did_open_for_active_file();
                }

                report.request_redraw
            }
        }
    }

    fn forward_to_pty(&self, text: &str) {
        if let Some(session_id) = self.focused_terminal_session_id() {
            self.submit(RequestSpec {
                revision_id: 0,
                topic: RequestTopic::TerminalPty,
                payload: WorkerRequestPayload::WritePtyInput {
                    session_id,
                    input: text.to_string(),
                },
            });
        }
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

    fn open_lazygit_buffer(&mut self) -> bool {
        let Some(workspace_root) = self.app_state.workspace_root_path().map(PathBuf::from) else {
            eprintln!("[AppShell] lazygit open skipped: workspace is not attached");
            return false;
        };

        let buffer_index = self
            .app_state
            .open_terminal_buffer("[Lazygit]", Some(workspace_root.clone()));
        self.pending_lazygit_buffer_index = Some(buffer_index);
        self.buffer_terminal_needs_layout = true;
        self.editor_needs_layout = true;
        self.editor_caret_needs_layout = false;
        self.highlight_spans.clear();
        let _ = self.sync_focus_mode_for_active_buffer();

        self.submit(RequestSpec {
            revision_id: 0,
            topic: RequestTopic::TerminalPty,
            payload: WorkerRequestPayload::SpawnPtyCommand {
                program: "lazygit".to_string(),
                args: Vec::new(),
                working_dir: Some(workspace_root),
            },
        });

        true
    }

    fn submit_git_blame_line(&mut self) -> bool {
        if self.app_state.active_buffer_is_terminal() {
            return false;
        }
        let Some(workspace_root) = self.app_state.workspace_root_path().map(PathBuf::from) else {
            return false;
        };
        let Some(file_path) = self.app_state.active_file().map(PathBuf::from) else {
            return false;
        };

        self.git_overlay_revision = self.git_overlay_revision.saturating_add(1);
        let line_number = self.app_state.cursor_line_col().0 + 1;
        self.submit(RequestSpec {
            revision_id: self.git_overlay_revision,
            topic: RequestTopic::Git,
            payload: WorkerRequestPayload::GitBlameLine {
                workspace_root,
                file_path,
                line_number,
            },
        });
        false
    }

    fn pending_confirmation_prompt(&self) -> Option<String> {
        match &self.pending_confirmation.as_ref()?.action {
            PendingConfirmationAction::Delete { path, .. } => {
                let label = path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .map(str::to_string)
                    .unwrap_or_else(|| path.to_string_lossy().into_owned());
                Some(format!("Delete {label}? (y/n)"))
            }
            PendingConfirmationAction::CloseDirtyBuffer { path } => {
                let label = path
                    .as_ref()
                    .and_then(|path| path.file_name())
                    .and_then(|name| name.to_str())
                    .map(str::to_string)
                    .unwrap_or_else(|| "current buffer".to_string());
                Some(format!("Save changes to {label} before closing? (y/n)"))
            }
        }
    }

    fn begin_explorer_delete_confirmation(&mut self) -> bool {
        self.ensure_explorer_snapshot();
        if self.explorer_snapshot.entries.is_empty() {
            self.explorer_cursor = 0;
            return false;
        }
        self.explorer_cursor = self
            .explorer_cursor
            .min(self.explorer_snapshot.entries.len().saturating_sub(1));
        let selected = self.explorer_snapshot.entries[self.explorer_cursor].clone();
        self.pending_confirmation = Some(PendingConfirmation {
            action: PendingConfirmationAction::Delete {
                path: selected.path,
                file_type: selected.file_type,
            },
            return_focus: FocusTarget::LeftSidebar,
        });
        let prompt = self.pending_confirmation_prompt().unwrap_or_default();
        if !self.open_prompt_overlay(
            crate::app::command_palette::CommandPaletteMode::ExplorerDeleteConfirm,
        ) {
            self.pending_confirmation = None;
            return false;
        }
        if let Err(err) = self.app_state.set_command_palette_query(&prompt) {
            eprintln!("[AppShell] delete confirmation prompt failed: {err}");
            self.pending_confirmation = None;
            let _ = self.app_state.close_command_palette();
            let _ = self.app_state.apply_mode_event(ModeEvent::ExitFocus);
            let _ = self.focus_manager.set(FocusTarget::LeftSidebar);
            return false;
        }
        true
    }

    fn begin_dirty_buffer_close_confirmation(&mut self) -> bool {
        self.pending_confirmation = Some(PendingConfirmation {
            action: PendingConfirmationAction::CloseDirtyBuffer {
                path: self.app_state.active_file().map(PathBuf::from),
            },
            return_focus: FocusTarget::CenterEditor,
        });
        let prompt = self.pending_confirmation_prompt().unwrap_or_default();
        if !self.open_prompt_overlay(
            crate::app::command_palette::CommandPaletteMode::BufferCloseConfirm,
        ) {
            self.pending_confirmation = None;
            return false;
        }
        if let Err(err) = self.app_state.set_command_palette_query(&prompt) {
            eprintln!("[AppShell] close confirmation prompt failed: {err}");
            self.pending_confirmation = None;
            let _ = self.app_state.close_command_palette();
            let _ = self.app_state.apply_mode_event(ModeEvent::ExitFocus);
            let _ = self.focus_manager.set(FocusTarget::CenterEditor);
            return false;
        }
        true
    }

    pub(super) fn respond_to_pending_confirmation(&mut self, confirmed: bool) -> bool {
        let Some(pending) = self.pending_confirmation.take() else {
            return false;
        };
        let mut changed = self.close_pending_confirmation_overlay(pending.return_focus);

        match pending.action {
            PendingConfirmationAction::Delete { path, file_type } => {
                if !confirmed {
                    return changed;
                }
                let fallback_selection = self.app_state.workspace_root_path().and_then(|root| {
                    path.parent().and_then(|parent| {
                        (parent.starts_with(root) && parent != root).then(|| parent.to_path_buf())
                    })
                });

                let delete_result = match file_type {
                    WorkspaceNodeType::File => std::fs::remove_file(&path),
                    WorkspaceNodeType::Folder => std::fs::remove_dir_all(&path),
                };
                if let Err(err) = delete_result {
                    eprintln!(
                        "[AppShell] explorer delete failed for {}: {err}",
                        path.display()
                    );
                    return changed;
                }

                if let Err(err) = self.app_state.rescan_workspace() {
                    eprintln!(
                        "[AppShell] workspace rescan failed after explorer delete for {}: {err}",
                        path.display()
                    );
                }
                if let Some(parent_path) = fallback_selection {
                    let _ = self.app_state.workspace_select_path(&parent_path);
                }
                self.mark_explorer_dirty();
                true
            }
            PendingConfirmationAction::CloseDirtyBuffer { .. } => {
                if confirmed {
                    let saved = self.handle_command(Command::SaveFile);
                    changed |= saved;
                    if self.app_state.is_dirty() {
                        return changed;
                    }
                }
                changed | self.close_current_buffer_now()
            }
        }
    }

    fn close_pending_confirmation_overlay(&mut self, focus_target: FocusTarget) -> bool {
        let mut changed = self.app_state.close_command_palette();
        if self.app_state.current_mode() == EditorMode::PaletteFocus
            && let Ok(result) = self.app_state.apply_mode_event(ModeEvent::ExitFocus)
        {
            changed |= result.changed;
        }
        let focus_changed = self.focus_manager.set(focus_target);
        changed |= focus_changed;
        if focus_changed {
            self.input_handler.clear_pending_prefix();
        }
        changed
    }

    fn open_prompt_overlay(
        &mut self,
        mode: crate::app::command_palette::CommandPaletteMode,
    ) -> bool {
        let current_mode = self.app_state.current_mode();
        if current_mode != EditorMode::PaletteFocus
            && !self.app_state.can_apply_mode_event(ModeEvent::OpenPalette)
        {
            return false;
        }

        if let Err(err) = self.app_state.open_command_palette_mode(mode) {
            eprintln!("[AppShell] prompt overlay open failed: {err}");
            return false;
        }

        if current_mode != EditorMode::PaletteFocus
            && let Err(err) = self.app_state.apply_mode_event(ModeEvent::OpenPalette)
        {
            let _ = self.app_state.close_command_palette();
            eprintln!("[AppShell] explorer prompt mode change failed: {err:?}");
            return false;
        }

        let focus_changed = self.focus_manager.set(FocusTarget::OverlayLayer);
        if focus_changed {
            self.input_handler.clear_pending_prefix();
        }
        true
    }

    pub(super) fn close_current_buffer_now(&mut self) -> bool {
        let closed_terminal_session = self.app_state.active_terminal_session_id();
        let report = dispatch_command(&mut self.app_state, Command::BufferCloseCurrent);
        self.reconcile_highlight_spans_with_pending_edits();

        if report.state_changed {
            if let Some(session_id) = closed_terminal_session {
                self.terminal_buffer_grids.remove(&session_id);
                self.submit(RequestSpec {
                    revision_id: 0,
                    topic: RequestTopic::TerminalPty,
                    payload: WorkerRequestPayload::ClosePtySession { session_id },
                });
            }
            self.highlight_spans.clear();
            self.mark_explorer_dirty();
            let viewport_lines = self.editor_viewport_lines();
            self.app_state.auto_scroll_to_cursor(viewport_lines);
            self.editor_needs_layout = true;
            self.editor_caret_needs_layout = false;
            self.buffer_terminal_needs_layout = true;
            self.submit_parse_for_active_buffer(true);
            self.submit_lsp_did_open_for_active_file();
            let _ = self.sync_focus_mode_for_active_buffer();
        }

        report.request_redraw || report.state_changed
    }

    fn confirm_explorer_prompt(&mut self) -> bool {
        let Some(mode) = self.app_state.command_palette_mode() else {
            return false;
        };
        let Some(target_path) = self.resolve_explorer_creation_target() else {
            return false;
        };

        let create_result = match mode {
            crate::app::command_palette::CommandPaletteMode::ExplorerCreateFile => {
                if let Some(parent) = target_path.parent()
                    && let Err(err) = std::fs::create_dir_all(parent)
                {
                    eprintln!(
                        "[AppShell] explorer create parent directories failed for {}: {err}",
                        target_path.display()
                    );
                    return false;
                }
                std::fs::OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(&target_path)
                    .map(|_| ())
            }
            crate::app::command_palette::CommandPaletteMode::ExplorerCreateFolder => {
                std::fs::create_dir_all(&target_path)
            }
            _ => return false,
        };

        if let Err(err) = create_result {
            eprintln!(
                "[AppShell] explorer create failed for {}: {err}",
                target_path.display()
            );
            return false;
        }

        if let Err(err) = self.app_state.rescan_workspace() {
            eprintln!(
                "[AppShell] workspace rescan failed after explorer create for {}: {err}",
                target_path.display()
            );
        }
        self.explorer_reveal_file(&target_path);

        let mut changed = self.app_state.close_command_palette();
        if let Ok(result) = self.app_state.apply_mode_event(ModeEvent::ExitFocus) {
            changed |= result.changed;
        }
        let focus_changed = self.focus_manager.set(FocusTarget::LeftSidebar);
        changed |= focus_changed;
        if focus_changed {
            self.input_handler.clear_pending_prefix();
        }
        self.sidebar_needs_layout = true;
        changed
    }

    fn resolve_explorer_creation_target(&mut self) -> Option<PathBuf> {
        let raw_name = self
            .app_state
            .command_palette_query_text()
            .trim()
            .to_string();
        if raw_name.is_empty() {
            return None;
        }

        self.ensure_explorer_snapshot();
        let root = self.app_state.workspace_root_path()?.to_path_buf();
        let base_dir = self
            .explorer_snapshot
            .entries
            .get(self.explorer_cursor)
            .map(|entry| {
                if entry.file_type == WorkspaceNodeType::Folder {
                    entry.path.clone()
                } else {
                    entry
                        .path
                        .parent()
                        .map(PathBuf::from)
                        .unwrap_or_else(|| root.clone())
                }
            })
            .unwrap_or_else(|| root.clone());

        let target_path = if Path::new(&raw_name).is_absolute() {
            PathBuf::from(&raw_name)
        } else {
            base_dir.join(&raw_name)
        };

        if !target_path.starts_with(&root) {
            eprintln!(
                "[AppShell] explorer create target must stay inside workspace root: {}",
                target_path.display()
            );
            return None;
        }

        Some(target_path)
    }

    // ── OpenFolder / OpenRecentProjects ───────────────────────────────────────

    fn open_folder_with_dialog(&mut self) -> bool {
        let Some(folder) = rfd::FileDialog::new().pick_folder() else {
            return false;
        };

        if let Err(err) = self.app_state.attach_workspace(folder.clone()) {
            eprintln!("[AppShell] attach_workspace failed: {err}");
            return false;
        }

        self.persistent_state.push_recent(folder.clone());
        self.persistent_state.save();

        self.mark_explorer_dirty();
        if !self.panel_state.left.visible {
            self.panel_state.left.visible = true;
            self.sidebar_needs_layout = true;
        }
        self.workspace_git_branch = self
            .app_state
            .workspace_root_path()
            .and_then(detect_git_branch);

        self.submit(RequestSpec {
            revision_id: 0,
            topic: RequestTopic::WorkspaceWatch,
            payload: WorkerRequestPayload::StartFileWatch {
                root_path: folder.clone(),
            },
        });
        self.sync_lsp_server_for_workspace();

        self.editor_needs_layout = true;
        self.editor_caret_needs_layout = false;
        true
    }

    fn open_recent_projects_palette(&mut self) -> bool {
        let recent = self.persistent_state.recent_projects.clone();
        if recent.is_empty() {
            return false;
        }

        let current_mode = self.app_state.current_mode();
        if current_mode != EditorMode::PaletteFocus
            && !self.app_state.can_apply_mode_event(ModeEvent::OpenPalette)
        {
            return false;
        }

        if let Err(err) = self.app_state.open_recent_projects_palette(&recent) {
            eprintln!("[AppShell] open recent projects palette failed: {err}");
            return false;
        }

        if current_mode != EditorMode::PaletteFocus
            && let Err(err) = self.app_state.apply_mode_event(ModeEvent::OpenPalette)
        {
            let _ = self.app_state.close_command_palette();
            eprintln!("[AppShell] recent projects mode change failed: {err:?}");
            return false;
        }

        if self.focus_manager.set(FocusTarget::OverlayLayer) {
            self.input_handler.clear_pending_prefix();
        }
        true
    }

    fn confirm_recent_project_selection(&mut self) -> bool {
        let Some(crate::app::command_palette::CommandPaletteAction::OpenFile(path)) =
            self.app_state.command_palette_selected_action()
        else {
            return false;
        };

        let mut changed = self.app_state.close_command_palette();
        if let Ok(result) = self.app_state.apply_mode_event(ModeEvent::ExitFocus) {
            changed |= result.changed;
        }
        let focus_changed = self.focus_manager.set(FocusTarget::CenterEditor);
        changed |= focus_changed;
        if focus_changed {
            self.input_handler.clear_pending_prefix();
        }

        if let Err(err) = self.app_state.attach_workspace(path.clone()) {
            eprintln!("[AppShell] attach_workspace from recent failed: {err}");
            return changed;
        }

        self.persistent_state.push_recent(path.clone());
        self.persistent_state.save();

        self.mark_explorer_dirty();
        if !self.panel_state.left.visible {
            self.panel_state.left.visible = true;
            self.sidebar_needs_layout = true;
        }
        self.workspace_git_branch = self
            .app_state
            .workspace_root_path()
            .and_then(detect_git_branch);

        self.submit(RequestSpec {
            revision_id: 0,
            topic: RequestTopic::WorkspaceWatch,
            payload: WorkerRequestPayload::StartFileWatch {
                root_path: path.clone(),
            },
        });
        self.sync_lsp_server_for_workspace();

        self.editor_needs_layout = true;
        self.editor_caret_needs_layout = false;
        true
    }

    /// Quét viewport hiện tại, tìm tất cả ký tự `target` và gán labels 'a'-'z'.
    ///
    /// Trả về `Vec<(label_char, char_idx)>` để caller dùng khi render và khi jump.
    /// Chỉ scan trong khoảng [scroll_line, scroll_line + viewport_lines).
    fn generate_leap_labels(&self, target: char) -> Vec<(char, usize)> {
        const LABEL_CHARS: &[char] = &[
            'a', 'b', 'c', 'd', 'e', 'f', 'g', 'h', 'i', 'j', 'k', 'l', 'm', 'n', 'o', 'p', 'q',
            'r', 's', 't', 'u', 'v', 'w', 'x', 'y', 'z',
        ];

        let viewport_lines = self.editor_viewport_lines().max(1);
        let scroll_line = self.app_state.scroll_line;
        let total_chars = self.app_state.text_len_chars();

        // char_idx range [viewport_start_char, viewport_end_char)
        let viewport_start_char = self.app_state.char_idx_for_line(scroll_line);
        let viewport_end_char = self
            .app_state
            .char_idx_for_line(scroll_line + viewport_lines)
            .min(total_chars);

        if viewport_start_char >= viewport_end_char {
            return Vec::new();
        }

        // Lấy text snapshot và scan
        let text = self.app_state.text_string();
        let target_lower = if target.is_ascii_alphabetic() {
            target.to_ascii_lowercase()
        } else {
            target
        };

        let mut labels: Vec<(char, usize)> = Vec::new();
        let mut label_idx = 0usize;

        // char_indices() trả về (byte_offset, char) — ta cần char_idx để map Rope
        // Chuyển đổi: duyệt qua chars() với counter riêng
        let mut char_idx: usize = 0;
        for ch in text.chars() {
            if char_idx >= viewport_start_char && char_idx < viewport_end_char {
                let ch_lower = if ch.is_ascii_alphabetic() {
                    ch.to_ascii_lowercase()
                } else {
                    ch
                };
                if ch_lower == target_lower {
                    if label_idx < LABEL_CHARS.len() {
                        labels.push((LABEL_CHARS[label_idx], char_idx));
                        label_idx += 1;
                    } else {
                        break; // Đủ 26 labels rồi
                    }
                }
            }
            char_idx += 1;
            if char_idx >= viewport_end_char {
                break;
            }
        }

        labels
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::clipboard::ClipboardProvider;

    #[derive(Default)]
    struct MockClipboard {
        text: String,
    }

    impl ClipboardProvider for MockClipboard {
        fn get_text(&mut self) -> Result<String, String> {
            Ok(self.text.clone())
        }

        fn set_text(&mut self, text: &str) -> Result<(), String> {
            self.text = text.to_string();
            Ok(())
        }
    }

    #[test]
    fn palette_paste_uses_clipboard_provider() {
        let mut app_state = AppState::from_text(PathBuf::from("palette-paste.txt"), "alpha beta");
        let mut clipboard = MockClipboard {
            text: "foo\nbar".to_string(),
        };

        let open = dispatch_command(&mut app_state, Command::OpenInFileSearch);
        assert!(open.success);
        assert_eq!(app_state.current_mode(), EditorMode::PaletteFocus);
        assert!(app_state.is_command_palette_visible());

        let report = dispatch_palette_overlay_command(
            &mut app_state,
            &mut clipboard,
            Command::PasteSystemClipboard,
        );

        assert!(report.success);
        assert!(report.state_changed);
        assert_eq!(app_state.command_palette_query_text(), "foo bar");
    }

    #[test]
    fn move_to_first_line_uses_viewport_layout_path() {
        let mut shell = AppShell::new().expect("create app shell");
        let text = (0..80)
            .map(|idx| format!("line {idx}"))
            .collect::<Vec<_>>()
            .join("\n");
        shell.app_state = AppState::from_text(PathBuf::from("gg-layout.txt"), &text);
        let _ = shell.app_state.apply_mode_event(ModeEvent::EnterNormal);
        assert!(shell.app_state.move_to_last_line());
        shell.app_state.scroll_line = 24;
        shell.editor_needs_layout = false;
        shell.editor_caret_needs_layout = true;

        let changed = shell.handle_command(Command::MoveToFirstLine);

        assert!(changed);
        assert_eq!(shell.app_state.cursor_line_col(), (0, 0));
        assert_eq!(shell.app_state.scroll_line, 0);
        assert!(shell.editor_needs_layout);
        assert!(!shell.editor_caret_needs_layout);
    }

    #[test]
    fn toggle_terminal_command_closes_bottom_panel_after_second_press() {
        let mut shell = AppShell::new().expect("create app shell");
        assert!(!shell.panel_state.bottom.visible);

        assert!(shell.handle_command(Command::ToggleTerminal));
        assert!(shell.panel_state.bottom.visible);
        assert_eq!(shell.focus_manager.current(), FocusTarget::BottomPanel);

        assert!(shell.handle_command(Command::ToggleTerminal));
        assert!(!shell.panel_state.bottom.visible);
        assert_eq!(shell.focus_manager.current(), FocusTarget::CenterEditor);
    }

    #[test]
    fn toggle_bottom_dock_keeps_editor_focus_when_opening() {
        let mut shell = AppShell::new().expect("create app shell");
        assert_eq!(shell.focus_manager.current(), FocusTarget::CenterEditor);
        assert!(!shell.panel_state.bottom.visible);

        assert!(shell.handle_command(Command::ToggleBottomDock));
        assert!(shell.panel_state.bottom.visible);
        assert_eq!(shell.focus_manager.current(), FocusTarget::CenterEditor);

        assert!(shell.handle_command(Command::ToggleBottomDock));
        assert!(!shell.panel_state.bottom.visible);
        assert_eq!(shell.focus_manager.current(), FocusTarget::CenterEditor);
    }

    #[test]
    fn delete_confirmation_removes_selected_file_after_y() {
        let mut shell = AppShell::new().expect("create app shell");
        let root =
            std::env::temp_dir().join(format!("netherize_delete_confirm_{}", std::process::id()));
        std::fs::create_dir_all(&root).expect("create root");
        let file_path = root.join("delete-me.txt");
        std::fs::write(&file_path, "bye\n").expect("write file");

        shell
            .app_state
            .attach_workspace(root.clone())
            .expect("attach workspace");
        let _ = shell.app_state.workspace_select_path(&file_path);
        shell.mark_explorer_dirty();
        shell.ensure_explorer_snapshot();

        assert!(shell.handle_command(Command::ExplorerDeleteNode));
        assert_eq!(
            shell.app_state.command_palette_mode(),
            Some(crate::app::command_palette::CommandPaletteMode::ExplorerDeleteConfirm)
        );
        assert_eq!(
            shell.pending_confirmation_prompt().as_deref(),
            Some("Delete delete-me.txt? (y/n)")
        );
        assert_eq!(
            shell.app_state.command_palette_query_text(),
            "Delete delete-me.txt? (y/n)"
        );
        assert!(shell.respond_to_pending_confirmation(true));
        assert!(!file_path.exists());
        assert!(shell.pending_confirmation.is_none());
        assert!(!shell.app_state.is_command_palette_visible());

        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn delete_confirmation_cancels_on_escape() {
        let mut shell = AppShell::new().expect("create app shell");
        shell.pending_confirmation = Some(PendingConfirmation {
            action: PendingConfirmationAction::Delete {
                path: PathBuf::from("demo.txt"),
                file_type: WorkspaceNodeType::File,
            },
            return_focus: FocusTarget::LeftSidebar,
        });

        assert!(shell.respond_to_pending_confirmation(false));
        assert!(shell.pending_confirmation.is_none());
        assert!(!shell.app_state.is_command_palette_visible());
    }

    #[test]
    fn dirty_buffer_close_opens_save_confirmation_prompt() {
        let mut shell = AppShell::new().expect("create app shell");
        let file_name = format!("netherize_dirty_close_prompt_{}.txt", std::process::id());
        let file_path = std::env::temp_dir().join(&file_name);
        let expected_prompt = format!("Save changes to {file_name} before closing? (y/n)");
        std::fs::write(&file_path, "hello\n").expect("write file");
        shell
            .app_state
            .open_file(file_path.clone())
            .expect("open file");
        shell.app_state.insert_char('!');

        assert!(shell.handle_command(Command::BufferCloseCurrent));
        assert_eq!(
            shell.app_state.command_palette_mode(),
            Some(crate::app::command_palette::CommandPaletteMode::BufferCloseConfirm)
        );
        assert_eq!(
            shell.pending_confirmation_prompt().as_deref(),
            Some(expected_prompt.as_str())
        );

        let _ = std::fs::remove_file(file_path);
    }

    #[test]
    fn dirty_buffer_close_confirmation_yes_saves_then_closes() {
        let mut shell = AppShell::new().expect("create app shell");
        let file_path = std::env::temp_dir().join(format!(
            "netherize_dirty_close_yes_{}.txt",
            std::process::id()
        ));
        std::fs::write(&file_path, "hello\n").expect("write file");
        shell
            .app_state
            .open_file(file_path.clone())
            .expect("open file");
        shell.app_state.insert_char('!');

        assert!(shell.handle_command(Command::BufferCloseCurrent));
        assert!(shell.respond_to_pending_confirmation(true));
        assert_eq!(
            std::fs::read_to_string(&file_path).expect("read file"),
            "!hello\n"
        );
        assert!(shell.app_state.active_file().is_none());
        assert!(!shell.app_state.is_command_palette_visible());

        let _ = std::fs::remove_file(file_path);
    }

    #[test]
    fn dirty_buffer_close_confirmation_no_discards_changes_and_closes() {
        let mut shell = AppShell::new().expect("create app shell");
        let file_path = std::env::temp_dir().join(format!(
            "netherize_dirty_close_no_{}.txt",
            std::process::id()
        ));
        std::fs::write(&file_path, "hello\n").expect("write file");
        shell
            .app_state
            .open_file(file_path.clone())
            .expect("open file");
        shell.app_state.insert_char('!');

        assert!(shell.handle_command(Command::BufferCloseCurrent));
        assert!(shell.respond_to_pending_confirmation(false));
        assert_eq!(
            std::fs::read_to_string(&file_path).expect("read file"),
            "hello\n"
        );
        assert!(shell.app_state.active_file().is_none());
        assert!(!shell.app_state.is_command_palette_visible());

        let _ = std::fs::remove_file(file_path);
    }

    #[test]
    fn opening_palette_arms_one_shot_ime_suppression() {
        let mut shell = AppShell::new().expect("create app shell");

        assert!(shell.handle_command(Command::OpenCommandPalette));
        assert!(shell.app_state.is_command_palette_visible());
        assert!(shell.suppress_next_palette_ime_commit);
        assert!(shell.should_swallow_palette_ime_commit());
        assert!(!shell.suppress_next_palette_ime_commit);
    }

    #[test]
    fn first_real_keypress_after_palette_open_clears_ime_suppression() {
        let mut shell = AppShell::new().expect("create app shell");

        assert!(shell.handle_command(Command::OpenCommandPalette));
        assert!(shell.suppress_next_palette_ime_commit);

        shell.note_post_open_keyboard_press();

        assert!(!shell.suppress_next_palette_ime_commit);
        assert!(!shell.should_swallow_palette_ime_commit());
    }

    #[test]
    fn startup_keeps_a_workspace_attached_for_global_search() {
        let shell = AppShell::new().expect("create app shell");

        assert!(shell.app_state.workspace_root_path().is_some());
    }

    #[test]
    fn welcome_hides_while_command_palette_is_visible() {
        let mut shell = AppShell::new().expect("create app shell");

        assert!(shell.should_show_welcome());
        assert!(shell.handle_command(Command::OpenCommandPalette));
        assert!(!shell.should_show_welcome());
    }
}
