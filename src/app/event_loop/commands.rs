use super::*;

impl AppShell {
    fn reconcile_highlight_spans_with_pending_edits(&mut self) {
        let edits = self.app_state.take_highlight_edits();
        if edits.is_empty() {
            return;
        }

        crate::syntax::highlight::apply_highlight_edits(&mut self.highlight_spans, &edits);
    }

    pub(super) fn handle_command(&mut self, command: Command) -> bool {
        match &command {
            Command::ToggleTerminal => {
                let report = dispatch_command(&mut self.app_state, command);
                let is_open = self.app_state.is_terminal_panel_open();
                if is_open != self.panel_state.bottom.visible {
                    self.panel_state.toggle_bottom();
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
                } else {
                    self.focus_manager.set(FocusTarget::CenterEditor)
                };

                if focus_changed {
                    self.input_handler.clear_pending_prefix();
                }
                report.request_redraw
            }
            Command::OpenFilePicker
            | Command::OpenFileFinder
            | Command::OpenCommandPalette
            | Command::OpenVimCommand
            | Command::OpenWorkspaceSymbols
            | Command::SearchInFiles => {
                let report = dispatch_command(&mut self.app_state, command);
                if report.success && self.focus_manager.set(FocusTarget::OverlayLayer) {
                    self.input_handler.clear_pending_prefix();
                }
                report.request_redraw
            }
            Command::CloseFilePicker => {
                let report = dispatch_command(&mut self.app_state, command);
                if self.focus_manager.set(FocusTarget::CenterEditor) {
                    self.input_handler.clear_pending_prefix();
                }
                report.request_redraw
            }
            Command::ToggleExplorer => {
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
                self.terminal_grid.view_scroll_up(3);
                self.terminal_needs_layout = true;
                true
            }
            Command::TerminalScrollDown => {
                self.terminal_grid.view_scroll_down(3);
                self.terminal_needs_layout = true;
                true
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
                    let changed = self.explorer_expanded.remove(&selected.path);
                    if changed {
                        self.mark_explorer_dirty();
                    }
                    return changed;
                }

                let Some(parent_path) = selected.parent_path.as_ref() else {
                    return false;
                };
                let Some(parent_idx) = self
                    .explorer_snapshot
                    .entries
                    .iter()
                    .position(|entry| &entry.path == parent_path)
                else {
                    return false;
                };
                if parent_idx == self.explorer_cursor {
                    return false;
                }
                self.explorer_cursor = parent_idx;
                self.sidebar_needs_layout = true;
                true
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
                    let changed = self.explorer_expanded.insert(selected.path.clone());
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
                self.sidebar_needs_layout = true;
                true
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
                        let changed = self.explorer_expanded.remove(&selected.path);
                        if changed {
                            self.mark_explorer_dirty();
                        }
                        return changed;
                    }
                    let changed = self.explorer_expanded.insert(selected.path.clone());
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
            _ => {
                let should_notify_did_open = matches!(
                    &command,
                    Command::OpenFile(_)
                        | Command::FilePickerConfirmSelection
                        | Command::BufferNext
                        | Command::BufferPrev
                        | Command::BufferCloseCurrent
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
                        | Command::DeleteWordForward
                        | Command::DeleteWordBackward
                        | Command::ChangeSelection
                        | Command::ChangeWordForward
                        | Command::ChangeWordBackward
                        | Command::Undo
                        | Command::Redo
                        | Command::ReplaceChar(_)
                        | Command::BufferNew
                        | Command::BufferNext
                        | Command::BufferPrev
                        | Command::BufferCloseCurrent
                        | Command::OpenFile(_)
                        | Command::FilePickerConfirmSelection
                );
                let is_typing_edit = matches!(
                    &command,
                    Command::InsertChar(_) | Command::InsertText(_) | Command::Backspace
                );
                let report = dispatch_command(&mut self.app_state, command);
                self.reconcile_highlight_spans_with_pending_edits();
                if report.success && should_notify_did_open {
                    self.highlight_spans.clear();
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
        if let Some(session_id) = self.pty_session_id {
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
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
