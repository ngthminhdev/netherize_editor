use super::*;

impl AppShell {
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
                }

                let focus_changed = if is_open {
                    let changed = self.focus_manager.set(FocusTarget::BottomPanel);
                    if self.pty_session_id.is_none() {
                        let working_dir = self
                            .app_state
                            .active_file()
                            .and_then(|path| path.parent())
                            .map(PathBuf::from)
                            .or_else(|| self.app_state.workspace_root_path().map(PathBuf::from))
                            .or_else(|| {
                                std::env::current_dir()
                                    .ok()
                                    .filter(|path| path != &PathBuf::from("/"))
                            });
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

                if next_visible && self.pty_session_id.is_none() {
                    let working_dir = self
                        .app_state
                        .active_file()
                        .and_then(|path| path.parent())
                        .map(PathBuf::from)
                        .or_else(|| self.app_state.workspace_root_path().map(PathBuf::from))
                        .or_else(|| {
                            std::env::current_dir()
                                .ok()
                                .filter(|path| path != &PathBuf::from("/"))
                        });
                    self.submit(RequestSpec {
                        revision_id: 0,
                        topic: RequestTopic::TerminalPty,
                        payload: WorkerRequestPayload::SpawnPtyShell {
                            shell: None,
                            working_dir,
                        },
                    });
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
                Some(changed)
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
                Some(changed)
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
                        .or_else(|| self.app_state.workspace_root_path().map(PathBuf::from))
                        .or_else(|| {
                            std::env::current_dir()
                                .ok()
                                .filter(|path| path != &PathBuf::from("/"))
                        });
                    self.submit(RequestSpec {
                        revision_id: 0,
                        topic: RequestTopic::TerminalPty,
                        payload: WorkerRequestPayload::SpawnPtyShell {
                            shell: None,
                            working_dir,
                        },
                    });
                }

                Some(changed)
            }
            Command::FocusLeft | Command::FocusRight | Command::FocusUp | Command::FocusDown => {
                let mapped = self.map_directional_focus_command(command);
                Some(self.handle_command(mapped))
            }
            Command::TerminalWriteInput(input) => {
                self.forward_to_pty(input);
                Some(false)
            }
            Command::TerminalPaste => Some(self.handle_terminal_paste()),
            Command::TerminalScrollUp => {
                if let Some(grid) = self.focused_terminal_grid_mut() {
                    grid.view_scroll_up(3);
                    if self.app_state.active_buffer_is_terminal() {
                        self.buffer_terminal_needs_layout = true;
                    } else {
                        self.terminal_needs_layout = true;
                    }
                    Some(true)
                } else {
                    Some(false)
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
                    Some(true)
                } else {
                    Some(false)
                }
            }
            Command::TerminalSearchOpen => {
                let report = dispatch_command(&mut self.app_state, Command::OpenInFileSearch);
                if report.success {
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
                self.sidebar_needs_layout = true;
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
            _ => None,
        }
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
