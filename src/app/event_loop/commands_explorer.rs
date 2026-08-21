use super::*;

fn next_available_paste_path(target_dir: &Path, file_name: &str) -> PathBuf {
    let candidate = target_dir.join(file_name);
    if !candidate.exists() {
        return candidate;
    }

    let path = Path::new(file_name);
    let stem = path
        .file_stem()
        .and_then(|value| value.to_str())
        .filter(|value| !value.is_empty())
        .unwrap_or(file_name);
    let extension = path.extension().and_then(|value| value.to_str());

    for index in 1.. {
        let next_name = match extension {
            Some(ext) if !ext.is_empty() => format!("{stem} ({index}).{ext}"),
            _ => format!("{stem} ({index})"),
        };
        let next = target_dir.join(next_name);
        if !next.exists() {
            return next;
        }
    }

    candidate
}

impl AppShell {
    fn reset_terminals_for_workspace_switch(&mut self) {
        let bottom_sessions: Vec<u64> = self
            .terminal_tabs
            .iter()
            .filter_map(|tab| tab.session_id)
            .collect();
        let right_session = self.right_pty_session_id;
        let buffer_sessions: Vec<u64> = self.terminal_buffer_grids.keys().copied().collect();

        for session_id in bottom_sessions
            .into_iter()
            .chain(right_session)
            .chain(buffer_sessions)
        {
            self.submit(RequestSpec {
                revision_id: 0,
                topic: RequestTopic::TerminalPty,
                payload: WorkerRequestPayload::ClosePtySession { session_id },
            });
        }

        self.ignored_terminal_tab_spawns
            .extend(self.pending_terminal_tab_spawns.keys().copied());
        self.pending_terminal_tab_spawns.clear();
        self.terminal_buffer_grids.clear();
        self.pending_lazygit_buffer_index = None;
        self.pending_lazydocker_buffer_index = None;
        self.right_pty_session_id = None;
        self.pending_right_pty_spawn = false;
        self.right_pty_startup_command = None;
        self.right_terminal_grid = TerminalGrid::new(120, 40);
        self.right_terminal_grid.highlight_colors = HighlightColors::from_theme(&self.theme);
        self.right_terminal_needs_layout = true;
        self.last_right_terminal_bounds = None;

        let mut grid = TerminalGrid::new(120, 40);
        grid.highlight_colors = HighlightColors::from_theme(&self.theme);
        self.terminal_tabs = vec![TerminalTab::new(grid, "bash".to_string())];
        self.active_terminal_tab = 0;
        self.terminal_needs_layout = true;
        self.buffer_terminal_needs_layout = true;
        self.last_terminal_bounds = None;
        self.last_buffer_terminal_bounds = None;

        if let Some(renderer) = self.renderer.as_mut() {
            renderer.clear_terminal();
            renderer.clear_buffer_terminal();
            renderer.clear_right_terminal();
        }
    }

    fn prepare_for_workspace_switch(&mut self) {
        self.reset_terminals_for_workspace_switch();

        self.submit(RequestSpec {
            revision_id: 0,
            topic: RequestTopic::LspClient,
            payload: WorkerRequestPayload::ShutdownAllLspServers,
        });

        self.app_state.clear_workspace_session_state();
        // Drop the previous workspace's indexed symbols so completion can't
        // suggest names — and synthesize imports — pointing at the old project
        // during the window before the new workspace finishes indexing.
        self.app_state.workspace_symbol_cache().clear_all();
        self.active_lsp_server = None;
        self.pending_lsp_server = None;
        self.pending_lsp_document_sync = None;
        self.lsp_completion_trigger_chars.clear();
        self.active_lsp_guide = None;
        self.highlight_spans.clear();
        self.semantic_highlight_spans.clear();
        self.cached_document_symbols.clear();
        self.cached_document_symbols_path = None;
    }

    pub(super) fn reload_workspace(&mut self) -> bool {
        let Some(root_path) = self.app_state.workspace_root_path().map(PathBuf::from) else {
            self.show_transient_toast("Reload Workspace: no workspace open".to_string());
            return false;
        };

        let _ = crate::lsp::client::refresh_patched_env_path();

        if let Err(err) = self.app_state.attach_workspace(root_path.clone()) {
            self.show_transient_toast(format!("Reload Workspace failed: {err}"));
            return false;
        }

        self.explorer_snapshot = ExplorerSnapshot::default();
        self.explorer_snapshot_dirty = true;
        self.mark_explorer_dirty();
        self.workspace_git_branch = self
            .app_state
            .workspace_root_path()
            .and_then(detect_git_branch);

        self.submit(RequestSpec {
            revision_id: 0,
            topic: RequestTopic::WorkspaceWatch,
            payload: WorkerRequestPayload::StartFileWatch {
                root_path: root_path.clone(),
                recursive: true,
            },
        });
        self.submit_workspace_git_status_refresh();
        self.submit_active_buffer_git_baseline_refresh();

        self.show_transient_toast("Workspace reloaded".to_string());
        self.update_window_title();
        self.editor_needs_layout = true;
        self.editor_caret_needs_layout = true;
        true
    }

    pub(in crate::app::event_loop) fn switch_workspace_to(&mut self, root_path: PathBuf) -> bool {
        self.switch_workspace_with_files(root_path, Vec::new())
    }

    /// Switch workspace, then open `follow_files` in the new workspace. When
    /// unsaved edits exist this asks first (save all / discard / stay) instead
    /// of silently dropping them with the buffer list.
    pub(in crate::app::event_loop) fn switch_workspace_with_files(
        &mut self,
        root_path: PathBuf,
        follow_files: Vec<PathBuf>,
    ) -> bool {
        let dirty_count = self.app_state.dirty_buffer_count();
        if dirty_count > 0 && self.pending_confirmation.is_none() {
            return self.begin_workspace_switch_confirmation(root_path, follow_files, dirty_count);
        }
        self.perform_workspace_switch(root_path, follow_files)
    }

    /// A second launch handed us its CLI paths: focus this window, switch to
    /// the requested project dir (if different), open the requested files.
    pub(in crate::app::event_loop) fn handle_remote_open(&mut self, paths: Vec<PathBuf>) {
        if let Some(window) = self.window.as_ref() {
            window.focus_window();
        }
        let dir = paths.iter().find(|p| p.is_dir()).cloned();
        let files: Vec<PathBuf> = paths.into_iter().filter(|p| p.is_file()).collect();

        let same_workspace = |shell: &Self, dir: &Path| {
            shell
                .app_state
                .workspace_root_path()
                .is_some_and(|root| crate::app::app_state::path_matches(root, dir))
        };
        match dir {
            Some(dir) if !same_workspace(self, &dir) => {
                let name = dir
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("workspace");
                self.show_transient_toast(format!("Opening {name}…"));
                self.switch_workspace_with_files(dir, files);
            }
            _ => {
                let mut opened_any = false;
                for file in files {
                    if let Err(err) = self.app_state.open_file(file.clone()) {
                        eprintln!("[AppShell] remote open skipped ({}): {err}", file.display());
                    } else {
                        opened_any = true;
                    }
                }
                if opened_any {
                    self.invalidate_highlights_and_parse_active_buffer();
                    self.submit_lsp_did_open_for_active_file();
                    self.update_window_title();
                }
            }
        }
        self.editor_needs_layout = true;
        self.request_redraw();
    }

    pub(in crate::app::event_loop) fn perform_workspace_switch(
        &mut self,
        root_path: PathBuf,
        follow_files: Vec<PathBuf>,
    ) -> bool {
        self.prepare_for_workspace_switch();

        if let Err(err) = self.app_state.attach_workspace(root_path.clone()) {
            eprintln!("[AppShell] attach_workspace failed: {err}");
            return false;
        }

        // Do NOT re-enter welcome mode here: the explorer Enter key maps to
        // FilePickerConfirmSelection when welcome_visible=true, which silently
        // fails when no recent-projects palette is open (shows "[no file]").
        let _ = self.app_state.set_initial_launch_welcome(false);

        // Hide ALL panels so the new workspace starts clean (like a fresh open).
        self.panel_state.left.visible = false;
        self.panel_state.right.visible = false;
        self.panel_state.bottom.visible = false;
        self.sidebar_needs_layout = true;

        // Force explorer snapshot refresh (clear stale cached entries from previous workspace).
        self.explorer_snapshot = ExplorerSnapshot::default();
        self.explorer_snapshot_dirty = true;
        self.explorer_cursor = 0;

        let icon_source =
            crate::app::persistence::AppPersistentState::infer_project_icon_source(&root_path);
        self.persistent_state
            .push_recent_with_icon(root_path.clone(), Some(icon_source));
        self.persistent_state.save();

        self.mark_explorer_dirty();
        self.workspace_git_branch = self
            .app_state
            .workspace_root_path()
            .and_then(detect_git_branch);

        self.submit(RequestSpec {
            revision_id: 0,
            topic: RequestTopic::WorkspaceWatch,
            payload: WorkerRequestPayload::StartFileWatch {
                root_path: root_path.clone(),
                recursive: true,
            },
        });

        self.submit_workspace_git_status_refresh();
        self.submit_active_buffer_git_baseline_refresh();

        self.sync_lsp_server_for_workspace();

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

    /// Number of fully-visible explorer rows in the current sidebar viewport.
    /// Falls back to a constant when the sidebar has not been laid out yet.
    fn explorer_page_rows(&self) -> usize {
        const FALLBACK_PAGE_ROWS: usize = 20;
        let Some(bounds) = self.last_sidebar_bounds else {
            return FALLBACK_PAGE_ROWS;
        };
        let line_height = self.theme.ui.sidebar_line_height.max(1.0);
        let viewport = self.sidebar_tree_viewport_height(bounds);
        ((viewport / line_height).floor() as usize).max(1)
    }

    /// Move the explorer cursor by `delta` rows (down if `down`, else up),
    /// clamping to the entry list. Returns whether the cursor moved.
    fn move_explorer_cursor_by(&mut self, delta: usize, down: bool) -> bool {
        self.ensure_explorer_snapshot();
        let entries_len = self.explorer_snapshot.entries.len();
        if entries_len == 0 {
            self.explorer_cursor = 0;
            return false;
        }
        let last = entries_len - 1;
        let current = self.explorer_cursor.min(last);
        let target = if down {
            (current + delta).min(last)
        } else {
            current.saturating_sub(delta)
        };
        if target == current {
            return false;
        }
        self.explorer_cursor = target;
        let _ = self
            .app_state
            .workspace_select_path(&self.explorer_snapshot.entries[target].path);
        self.sidebar_needs_layout = true;
        true
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
            Command::ReloadWorkspace => Some(self.reload_workspace()),
            Command::OpenFolder => Some(self.open_folder_with_dialog()),
            Command::OpenRecentProjects => {
                let mut changed = self.open_recent_projects_palette();
                if changed {
                    changed |= self.dismiss_initial_launch_welcome_if_active();
                }
                Some(changed)
            }
            Command::OpenWorktreePalette => Some(self.open_worktree_palette()),
            Command::ToggleLeftDock => {
                let mut changed = self.panel_state.toggle_left();
                if changed {
                    self.sidebar_needs_layout = true;
                }
                let mut focus_changed = false;
                if self.panel_state.left.visible {
                    changed |= self.dismiss_initial_launch_welcome_if_active();
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
            Command::ExplorerToggleGitChangesOnly => {
                let changed = self.app_state.workspace_toggle_show_git_changes_only();
                if changed {
                    self.submit_workspace_git_status_refresh();
                    self.mark_explorer_dirty();
                }
                Some(changed)
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
            Command::ExplorerHalfPageDown => {
                let step = (self.explorer_page_rows() / 2).max(1);
                Some(self.move_explorer_cursor_by(step, true))
            }
            Command::ExplorerHalfPageUp => {
                let step = (self.explorer_page_rows() / 2).max(1);
                Some(self.move_explorer_cursor_by(step, false))
            }
            Command::ExplorerRenameFull => Some(self.open_explorer_rename_prompt(false)),
            Command::ExplorerRenameBase => Some(self.open_explorer_rename_prompt(true)),
            Command::ExplorerDeleteNode => Some(self.begin_explorer_delete_confirmation()),
            Command::ExplorerCopyFile => {
                self.ensure_explorer_snapshot();
                if self.explorer_snapshot.entries.is_empty() {
                    return Some(false);
                }
                let selected_path = &self.explorer_snapshot.entries[self.explorer_cursor].path;
                self.explorer_clipboard_path = Some(selected_path.clone());
                self.show_transient_toast(format!("Copied: {}", selected_path.display()));
                Some(true)
            }
            Command::ExplorerPasteFile => {
                let Some(source_path) = self.explorer_clipboard_path.clone() else {
                    self.show_transient_toast("No file copied".to_string());
                    return Some(false);
                };

                self.ensure_explorer_snapshot();
                if self.explorer_snapshot.entries.is_empty() {
                    return Some(false);
                }

                let selected_entry = &self.explorer_snapshot.entries[self.explorer_cursor];
                let target_dir = if selected_entry.file_type == WorkspaceNodeType::Folder {
                    selected_entry.path.clone()
                } else {
                    selected_entry
                        .path
                        .parent()
                        .map(PathBuf::from)
                        .unwrap_or_else(|| {
                            self.app_state
                                .workspace_root_path()
                                .map(PathBuf::from)
                                .unwrap_or_default()
                        })
                };

                let file_name = source_path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("file")
                    .to_string();
                let suggested_path = next_available_paste_path(&target_dir, &file_name);
                let suggested_name = suggested_path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or(&file_name)
                    .to_string();

                if !self.open_prompt_overlay(CommandPaletteMode::ExplorerPasteFile) {
                    return Some(false);
                }

                self.pending_paste_source_path = Some(source_path);
                self.pending_paste_target_dir = Some(target_dir);
                let _ = self.app_state.set_command_palette_query(&suggested_name);
                let _ = self
                    .app_state
                    .set_command_palette_selection_range(Some((0, suggested_name.len())));

                Some(true)
            }
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
                self.app_state.load_leetcode_cases_for_active_file();
                self.submit_active_buffer_git_baseline_refresh();
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
