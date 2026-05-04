use super::*;

impl AppShell {
    fn prepare_for_workspace_switch(&mut self) {
        self.submit(RequestSpec {
            revision_id: 0,
            topic: RequestTopic::LspClient,
            payload: WorkerRequestPayload::ShutdownAllLspServers,
        });

        self.app_state.clear_workspace_session_state();
        self.active_lsp_server = None;
        self.pending_lsp_server = None;
        self.pending_lsp_document_sync = None;
        self.lsp_completion_trigger_chars.clear();
        self.active_lsp_guide = None;
        self.highlight_spans.clear();
        self.semantic_highlight_spans.clear();
    }

    pub(in crate::app::event_loop) fn switch_workspace_to(&mut self, root_path: PathBuf) -> bool {
        self.prepare_for_workspace_switch();

        if let Err(err) = self.app_state.attach_workspace(root_path.clone()) {
            eprintln!("[AppShell] attach_workspace failed: {err}");
            return false;
        }
        let _ = self.app_state.dismiss_initial_launch_welcome();

        self.persistent_state.push_recent(root_path.clone());
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
                root_path: root_path.clone(),
            },
        });

        self.submit_workspace_git_status_refresh();
        self.submit_active_buffer_git_baseline_refresh();

        if let Some(session_id) = self.pty_session_id {
            let quoted = shell_quote_path(&root_path);
            self.forward_to_terminal_session(session_id, &format!("\x15cd {quoted}\r"));
        }
        if let Some(session_id) = self.right_pty_session_id {
            let quoted = shell_quote_path(&root_path);
            self.forward_to_terminal_session(session_id, &format!("\x15cd {quoted}\r"));
        }

        self.sync_lsp_server_for_workspace();

        self.editor_needs_layout = true;
        self.editor_caret_needs_layout = false;
        true
    }

    fn explorer_selected_entry(&mut self) -> Option<ExplorerEntry> {
        self.ensure_explorer_snapshot();
        if self.explorer_snapshot.entries.is_empty() {
            self.explorer_cursor = 0;
            return None;
        }
        self.explorer_cursor = self
            .explorer_cursor
            .min(self.explorer_snapshot.entries.len().saturating_sub(1));
        self.explorer_snapshot
            .entries
            .get(self.explorer_cursor)
            .cloned()
    }

    pub(super) fn explorer_rename_base_selection(name: &str) -> (usize, usize) {
        match name.rfind('.') {
            Some(0) | None => (0, name.len()),
            Some(dot_index) => (0, dot_index),
        }
    }

    fn open_explorer_rename_prompt(&mut self, rename_base_only: bool) -> bool {
        let Some(selected) = self.explorer_selected_entry() else {
            return false;
        };
        if selected.file_type != WorkspaceNodeType::File {
            return false;
        }
        let Some(file_name) = selected
            .path
            .file_name()
            .and_then(|name| name.to_str())
            .map(str::to_string)
        else {
            return false;
        };

        let mode = if rename_base_only {
            CommandPaletteMode::ExplorerRenameBase
        } else {
            CommandPaletteMode::ExplorerRenameFull
        };
        if !self.open_prompt_overlay(mode) {
            return false;
        }
        let _ = self
            .app_state
            .set_pending_explorer_rename_path(Some(selected.path.clone()));
        let _ = self.app_state.set_command_palette_query(&file_name);
        let _ = self
            .app_state
            .set_command_palette_selection_range(if rename_base_only {
                Some(Self::explorer_rename_base_selection(&file_name))
            } else {
                None
            });
        true
    }

    pub(super) fn handle_explorer_and_workspace_command(
        &mut self,
        command: &Command,
    ) -> Option<bool> {
        match command {
            Command::OpenFolder => Some(self.open_folder_with_dialog()),
            Command::OpenRecentProjects => Some(self.open_recent_projects_palette()),
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
                Some(changed)
            }
            Command::ExplorerCreateFile => Some(self.open_prompt_overlay(
                crate::app::command_palette::CommandPaletteMode::ExplorerCreateFile,
            )),
            Command::ExplorerCreateFolder => Some(self.open_prompt_overlay(
                crate::app::command_palette::CommandPaletteMode::ExplorerCreateFolder,
            )),
            Command::ExplorerStartFilter => {
                let changed = self.app_state.workspace_start_filter_input();
                if changed {
                    self.input_handler.clear_pending_prefix();
                    self.mark_explorer_dirty();
                }
                Some(changed)
            }
            Command::ExplorerClearFilter => {
                let changed = self.app_state.workspace_clear_filter();
                if changed {
                    self.input_handler.clear_pending_prefix();
                    self.mark_explorer_dirty();
                }
                Some(changed)
            }
            Command::ExplorerToggleHidden => {
                let changed = self.app_state.workspace_toggle_show_hidden();
                if !changed {
                    return Some(false);
                }
                let Ok(rescanned) = self.app_state.rescan_workspace() else {
                    return Some(false);
                };
                if rescanned {
                    self.mark_explorer_dirty();
                    Some(true)
                } else {
                    Some(false)
                }
            }
            Command::ExplorerToggleIgnored => {
                let changed = self.app_state.workspace_toggle_show_ignored();
                if !changed {
                    return Some(false);
                }
                let Ok(rescanned) = self.app_state.rescan_workspace() else {
                    return Some(false);
                };
                if rescanned {
                    self.mark_explorer_dirty();
                    Some(true)
                } else {
                    Some(false)
                }
            }
            Command::ExplorerMoveToTop => {
                self.ensure_explorer_snapshot();
                if self.explorer_snapshot.entries.is_empty() {
                    self.explorer_cursor = 0;
                    return Some(false);
                }
                self.explorer_cursor = 0;
                let _ = self
                    .app_state
                    .workspace_select_path(&self.explorer_snapshot.entries[0].path);
                self.sidebar_needs_layout = true;
                Some(true)
            }
            Command::ExplorerMoveToBottom => {
                self.ensure_explorer_snapshot();
                if self.explorer_snapshot.entries.is_empty() {
                    self.explorer_cursor = 0;
                    return Some(false);
                }
                self.explorer_cursor = self.explorer_snapshot.entries.len().saturating_sub(1);
                let _ = self.app_state.workspace_select_path(
                    &self.explorer_snapshot.entries[self.explorer_cursor].path,
                );
                self.sidebar_needs_layout = true;
                Some(true)
            }
            Command::ExplorerRenameFull => Some(self.open_explorer_rename_prompt(false)),
            Command::ExplorerRenameBase => Some(self.open_explorer_rename_prompt(true)),
            Command::ExplorerDeleteNode => Some(self.begin_explorer_delete_confirmation()),
            Command::ExplorerMoveUp => {
                self.ensure_explorer_snapshot();
                let entries_len = self.explorer_snapshot.entries.len();
                if entries_len == 0 {
                    self.explorer_cursor = 0;
                    return Some(false);
                }
                self.explorer_cursor = self.explorer_cursor.min(entries_len.saturating_sub(1));
                if self.explorer_cursor == 0 {
                    Some(false)
                } else {
                    self.explorer_cursor -= 1;
                    let _ = self.app_state.workspace_select_path(
                        &self.explorer_snapshot.entries[self.explorer_cursor].path,
                    );
                    self.sidebar_needs_layout = true;
                    Some(true)
                }
            }
            Command::ExplorerMoveDown => {
                self.ensure_explorer_snapshot();
                let entries_len = self.explorer_snapshot.entries.len();
                if entries_len == 0 {
                    self.explorer_cursor = 0;
                    return Some(false);
                }
                self.explorer_cursor = self.explorer_cursor.min(entries_len.saturating_sub(1));
                if self.explorer_cursor + 1 >= entries_len {
                    Some(false)
                } else {
                    self.explorer_cursor += 1;
                    let _ = self.app_state.workspace_select_path(
                        &self.explorer_snapshot.entries[self.explorer_cursor].path,
                    );
                    self.sidebar_needs_layout = true;
                    Some(true)
                }
            }
            Command::ExplorerCollapseOrParent => {
                self.ensure_explorer_snapshot();
                if self.explorer_snapshot.entries.is_empty() {
                    self.explorer_cursor = 0;
                    return Some(false);
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
                    return Some(changed);
                }

                let Some(parent_path) = selected.parent_path.as_ref() else {
                    return Some(false);
                };
                if self.app_state.workspace_root_path() == Some(parent_path.as_path()) {
                    return Some(false);
                }
                let mut changed = self.app_state.workspace_select_path(parent_path);
                changed |= self.app_state.workspace_collapse_path(parent_path);
                if changed {
                    self.mark_explorer_dirty();
                }
                Some(changed)
            }
            Command::ExplorerCollapseAllUnderNode => {
                self.ensure_explorer_snapshot();
                if self.explorer_snapshot.entries.is_empty() {
                    self.explorer_cursor = 0;
                    return Some(false);
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
                        return Some(false);
                    }
                    parent_path.clone()
                } else {
                    return Some(false);
                };

                let mut changed = self.app_state.workspace_select_path(&target_path);
                changed |= self
                    .app_state
                    .workspace_collapse_path_and_descendants(&target_path);
                if changed {
                    self.mark_explorer_dirty();
                }
                Some(changed)
            }
            Command::ExplorerExpandNode => {
                self.ensure_explorer_snapshot();
                if self.explorer_snapshot.entries.is_empty() {
                    self.explorer_cursor = 0;
                    return Some(false);
                }
                self.explorer_cursor = self
                    .explorer_cursor
                    .min(self.explorer_snapshot.entries.len().saturating_sub(1));
                let selected = self.explorer_snapshot.entries[self.explorer_cursor].clone();
                if selected.file_type != WorkspaceNodeType::Folder {
                    return Some(false);
                }

                let changed = self.app_state.workspace_expand_path(&selected.path);
                if changed {
                    self.mark_explorer_dirty();
                }
                Some(changed)
            }
            Command::ExplorerExpandOrChild => {
                self.ensure_explorer_snapshot();
                if self.explorer_snapshot.entries.is_empty() {
                    self.explorer_cursor = 0;
                    return Some(false);
                }
                self.explorer_cursor = self
                    .explorer_cursor
                    .min(self.explorer_snapshot.entries.len().saturating_sub(1));
                let selected = self.explorer_snapshot.entries[self.explorer_cursor].clone();
                if selected.file_type != WorkspaceNodeType::Folder {
                    return Some(false);
                }

                if !selected.is_expanded {
                    let changed = self.app_state.workspace_expand_path(&selected.path);
                    if changed {
                        self.mark_explorer_dirty();
                    }
                    return Some(changed);
                }

                let Some(first_child_idx) = self
                    .explorer_snapshot
                    .entries
                    .iter()
                    .position(|entry| entry.parent_path.as_ref() == Some(&selected.path))
                else {
                    return Some(false);
                };

                if first_child_idx == self.explorer_cursor {
                    return Some(false);
                }
                self.explorer_cursor = first_child_idx;
                let _ = self
                    .app_state
                    .workspace_select_path(&self.explorer_snapshot.entries[first_child_idx].path);
                self.sidebar_needs_layout = true;
                Some(true)
            }
            Command::ExplorerExpandAllUnderNode => {
                self.ensure_explorer_snapshot();
                if self.explorer_snapshot.entries.is_empty() {
                    self.explorer_cursor = 0;
                    return Some(false);
                }
                self.explorer_cursor = self
                    .explorer_cursor
                    .min(self.explorer_snapshot.entries.len().saturating_sub(1));
                let selected = self.explorer_snapshot.entries[self.explorer_cursor].clone();
                if selected.file_type != WorkspaceNodeType::Folder {
                    return Some(false);
                }

                let mut changed = self.app_state.workspace_select_path(&selected.path);
                changed |= self
                    .app_state
                    .workspace_expand_path_and_descendants(&selected.path);
                if changed {
                    self.mark_explorer_dirty();
                }
                Some(changed)
            }
            Command::ExplorerToggleOrOpen
            | Command::ExplorerExpandCollapse
            | Command::ExplorerOpenFile => {
                self.ensure_explorer_snapshot();
                if self.explorer_snapshot.entries.is_empty() {
                    self.explorer_cursor = 0;
                    return Some(false);
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
                        return Some(changed);
                    }
                    let changed = self.app_state.workspace_expand_path(&selected.path);
                    if changed {
                        self.mark_explorer_dirty();
                    }
                    return Some(changed);
                }

                let report = dispatch_command(
                    &mut self.app_state,
                    Command::OpenFile(selected.path.clone()),
                );
                if !report.success {
                    return Some(false);
                }
                self.clear_highlight_layers();
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
                Some(changed)
            }
            _ => None,
        }
    }
}
