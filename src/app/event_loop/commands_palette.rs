use super::*;

impl AppShell {
    pub(super) fn handle_palette_and_open_command(
        &mut self,
        command: &Command,
        repeat_count: usize,
        command_for_post_hooks: &Command,
    ) -> Option<bool> {
        match command {
            Command::OpenFilePicker
            | Command::OpenFileFinder
            | Command::OpenCommandPalette
            | Command::OpenVimCommand
            | Command::OpenWorkspaceSymbols
            | Command::OpenDocumentSymbols
            | Command::LspRename
            | Command::OpenInFileSearch
            | Command::SearchInFiles
            | Command::OpenFileHistory
            | Command::OpenThemeSelector
            | Command::OpenHelp => {
                let opens_center_buffer = matches!(
                    command,
                    Command::OpenFileFinder
                        | Command::SearchInFiles
                        | Command::OpenFileHistory
                        | Command::OpenHelp
                );
                let report = dispatch_command(&mut self.app_state, command.clone());
                let mut request_redraw = report.request_redraw;
                if report.success {
                    request_redraw |= self.dismiss_initial_launch_welcome_if_active();
                    if opens_center_buffer {
                        self.editor_needs_layout = true;
                        self.editor_caret_needs_layout = false;
                    }
                    let focus_changed = if opens_center_buffer {
                        self.clear_palette_ime_commit_suppression();
                        self.focus_manager.set(FocusTarget::CenterEditor)
                    } else {
                        self.arm_palette_ime_commit_suppression();
                        self.focus_manager.set(FocusTarget::OverlayLayer)
                    };
                    if focus_changed {
                        self.input_handler.clear_pending_prefix();
                    }
                    if matches!(command_for_post_hooks, Command::OpenFileHistory)
                        && !self.app_state.command_palette_result_labels().is_empty()
                    {
                        let _ = self.app_state.preview_file_history_index(0);
                        if let Some((lines, preview_text)) =
                            self.app_state.build_file_history_diff_preview()
                        {
                            // FileHistory diff preview uses + / - markers — tree-sitter
                            // syntax highlighting would produce misaligned spans on the
                            // diff-formatted text. The renderer already applies green/red
                            // backgrounds based on the line prefix, so plain text is
                            // visually sufficient.
                            let _ = self.app_state.set_fuzzy_picker_preview(
                                lines,
                                preview_text,
                                Vec::new(),
                            );
                        }
                    }
                    if matches!(command_for_post_hooks, Command::OpenDocumentSymbols) {
                        self.submit_lsp_document_symbols();
                    }
                    if matches!(command_for_post_hooks, Command::OpenThemeSelector) {
                        self.begin_theme_picker_preview_session();
                        request_redraw |= self.preview_selected_theme_from_picker();
                    }
                    if self.app_state.command_palette_mode() == Some(CommandPaletteMode::ThemeSelector) {
                        self.begin_theme_picker_preview_session();
                        request_redraw |= self.preview_selected_theme_from_picker();
                    }
                }
                Some(request_redraw)
            }
            Command::OverlaySelectNext
            | Command::OverlaySelectPrev
            | Command::FilePickerSelectNext
            | Command::FilePickerSelectPrev
                if self.app_state.buffers().is_empty()
                    && (!self.app_state.is_command_palette_visible()
                        || self.app_state.command_palette_mode()
                            == Some(CommandPaletteMode::RecentProjects)) =>
            {
                if !self.app_state.is_command_palette_visible() {
                    self.app_state
                        .sync_welcome_recent_projects(&self.persistent_state.recent_projects);
                }
                let report = dispatch_command(&mut self.app_state, command.clone());
                Some(report.request_redraw || report.state_changed)
            }
            Command::FilePickerConfirmSelection
                if self.app_state.buffers().is_empty()
                    && (!self.app_state.is_command_palette_visible()
                        || self.app_state.command_palette_mode()
                            == Some(CommandPaletteMode::RecentProjects)) =>
            {
                if !self.app_state.is_command_palette_visible() {
                    // Palette not yet open — populate it first so selected_action works.
                    self.app_state
                        .sync_welcome_recent_projects(&self.persistent_state.recent_projects);
                }
                Some(self.confirm_recent_project_selection())
            }
            Command::OverlaySelectNext
            | Command::OverlaySelectPrev
            | Command::FilePickerSelectNext
            | Command::FilePickerSelectPrev
                if self.app_state.active_buffer_is_fuzzy_picker() =>
            {
                let _ = self.app_state.clear_completion();
                let report = {
                    let (app_state, clipboard) = (&mut self.app_state, &mut self.clipboard);
                    dispatch_command_with_clipboard_count(
                        app_state,
                        command.clone(),
                        repeat_count,
                        Some(clipboard),
                    )
                };
                if !report.success {
                    return Some(report.request_redraw);
                }
                if report.state_changed {
                    self.editor_needs_layout = true;
                    self.editor_caret_needs_layout = false;
                }
                if self.app_state.command_palette_mode() == Some(CommandPaletteMode::FileHistory) {
                    if let Some(
                        crate::app::command_palette::CommandPaletteAction::SelectFileHistoryEntry(
                            index,
                        ),
                    ) = self.app_state.command_palette_selected_action()
                    {
                        let _ = self.app_state.preview_file_history_index(index);
                        if let Some((lines, preview_text)) =
                            self.app_state.build_file_history_diff_preview()
                        {
                            // FileHistory diff preview uses + / - markers — tree-sitter
                            // syntax highlighting would produce misaligned spans. Plain
                            // text with green/red backgrounds from the renderer is sufficient.
                            let _ = self.app_state.set_fuzzy_picker_preview(
                                lines,
                                preview_text,
                                Vec::new(),
                            );
                        }
                    }
                } else if self.app_state.command_palette_mode() == Some(CommandPaletteMode::ThemeSelector) {
                    self.preview_selected_theme_from_picker();
                } else {
                    self.submit_active_palette_fzf_search();
                    self.submit_fuzzy_picker_preview_load();
                }
                Some(report.request_redraw || report.state_changed)
            }
            Command::FilePickerAppendQuery(_)
            | Command::FilePickerBackspaceQuery
            | Command::ToggleLiveGrepCaseSensitive
            | Command::EditorPaste
            | Command::PasteSystemClipboard
                if self.app_state.active_buffer_is_fuzzy_picker() =>
            {
                let report = {
                    let (app_state, clipboard) = (&mut self.app_state, &mut self.clipboard);
                    dispatch_palette_overlay_command(app_state, clipboard, command.clone())
                };
                if !report.success {
                    return Some(report.request_redraw);
                }
                if report.state_changed {
                    self.editor_needs_layout = true;
                    self.editor_caret_needs_layout = false;
                }
                if self.app_state.command_palette_mode() == Some(CommandPaletteMode::ThemeSelector) {
                    self.preview_selected_theme_from_picker();
                }
                self.submit_active_palette_fzf_search();
                self.submit_fuzzy_picker_preview_load();
                self.request_redraw();
                Some(true)
            }
            Command::OverlaySelectNext
            | Command::OverlaySelectPrev
            | Command::FilePickerSelectNext
            | Command::FilePickerSelectPrev
                if self.app_state.current_mode() == EditorMode::PaletteFocus
                    && self.app_state.is_command_palette_visible() =>
            {
                let report = dispatch_command(&mut self.app_state, command.clone());
                if report.success
                    && self.app_state.command_palette_mode() == Some(CommandPaletteMode::ThemeSelector)
                {
                    self.preview_selected_theme_from_picker();
                }
                Some(report.request_redraw || report.state_changed)
            }
            Command::FilePickerAppendQuery(_)
            | Command::FilePickerBackspaceQuery
            | Command::ToggleLiveGrepCaseSensitive
            | Command::ToggleInFileSearchCaseSensitive
            | Command::EditorPaste
            | Command::PasteSystemClipboard
                if self.app_state.current_mode() == EditorMode::PaletteFocus
                    && self.app_state.is_command_palette_visible() =>
            {
                let is_typing_edit = matches!(
                    command,
                    Command::InsertChar(_)
                        | Command::InsertText(_)
                        | Command::Backspace
                        | Command::Newline
                );
                if is_typing_edit {
                    let _ = self.app_state.clear_completion();
                }
                let report = {
                    let (app_state, clipboard) = (&mut self.app_state, &mut self.clipboard);
                    dispatch_palette_overlay_command(app_state, clipboard, command.clone())
                };
                if !report.success {
                    return Some(report.request_redraw);
                }

                match self.app_state.command_palette_mode() {
                    Some(
                        CommandPaletteMode::ExplorerCreateFile
                        | CommandPaletteMode::ExplorerCreateFolder
                        | CommandPaletteMode::ExplorerRenameFull
                        | CommandPaletteMode::ExplorerRenameBase,
                    ) => {}
                    Some(CommandPaletteMode::FilePicker)
                        if !matches!(command, Command::ToggleLiveGrepCaseSensitive) =>
                    {
                        self.submit_active_palette_fzf_search();
                    }
                    Some(CommandPaletteMode::LiveGrep) => {
                        self.submit_active_palette_fzf_search();
                    }
                    Some(CommandPaletteMode::InFileSearch) => {
                        if matches!(command, Command::ToggleInFileSearchCaseSensitive) {
                            self.request_redraw();
                        } else {
                            let _ = self.sync_in_file_search_with_palette_query();
                        }
                    }
                    Some(CommandPaletteMode::ThemeSelector) => {
                        self.preview_selected_theme_from_picker();
                    }
                    _ => {}
                }

                Some(report.request_redraw || report.state_changed)
            }
            Command::BufferCloseCurrent => {
                if self.app_state.is_dirty() && self.app_state.active_file().is_some() {
                    Some(self.begin_dirty_buffer_close_confirmation())
                } else {
                    Some(self.close_current_buffer_now())
                }
            }
            Command::CloseFilePicker => {
                let returns_to_explorer = matches!(
                    self.app_state.command_palette_mode(),
                    Some(
                        CommandPaletteMode::ExplorerCreateFile
                            | CommandPaletteMode::ExplorerCreateFolder
                            | CommandPaletteMode::ExplorerRenameFull
                            | CommandPaletteMode::ExplorerRenameBase
                            | CommandPaletteMode::ExplorerDeleteConfirm
                    )
                );
                let was_terminal_search = self.terminal_search_palette_active;
                self.terminal_search_palette_active = false;
                let was_theme_selector = self.app_state.command_palette_mode()
                    == Some(CommandPaletteMode::ThemeSelector);
                let restored_theme_preview = if was_theme_selector {
                    self.restore_theme_picker_preview()
                } else {
                    false
                };
                let report = dispatch_command(&mut self.app_state, command.clone());
                self.clear_palette_ime_commit_suppression();
                let focus_changed = if returns_to_explorer {
                    let _ = self.app_state.apply_mode_event(ModeEvent::ExitFocus);
                    self.focus_manager.set(FocusTarget::LeftSidebar)
                } else if was_terminal_search {
                    // Return focus to the terminal panel after closing search.
                    // `release_focus_mode_to_editor` already applied ExitFocus,
                    // transitioning PaletteFocus → TerminalNormal (via return_mode).
                    // We now re-enter TerminalNormal explicitly for safety and
                    // point focus at the bottom panel.
                    let _ = self.release_focus_mode_to_editor();
                    let _ = self.app_state.apply_mode_event(ModeEvent::FocusTerminal);
                    let _ = self
                        .app_state
                        .apply_mode_event(ModeEvent::EnterTerminalNormal);
                    // Clear terminal search highlights (user cancelled).
                    if let Some(grid) = self.focused_terminal_grid_mut() {
                        grid.search_matches.clear();
                        grid.search_cursor = 0;
                    }
                    self.mark_focused_terminal_layout_dirty();
                    self.focus_manager.set(FocusTarget::BottomPanel)
                } else {
                    self.focus_manager.set(FocusTarget::CenterEditor)
                };
                if focus_changed {
                    self.input_handler.clear_pending_prefix();
                }
                Some(report.request_redraw || restored_theme_preview)
            }
            Command::FilePickerConfirmSelection | Command::OpenFile(_) => {
                if matches!(command, Command::FilePickerConfirmSelection)
                    && matches!(
                        self.app_state.command_palette_mode(),
                        Some(
                            CommandPaletteMode::ExplorerCreateFile
                                | CommandPaletteMode::ExplorerCreateFolder
                                | CommandPaletteMode::ExplorerRenameFull
                                | CommandPaletteMode::ExplorerRenameBase
                        )
                    )
                {
                    return Some(self.confirm_explorer_prompt());
                }

                if matches!(command, Command::FilePickerConfirmSelection)
                    && matches!(
                        self.app_state.command_palette_mode(),
                        Some(CommandPaletteMode::RecentProjects)
                    )
                {
                    return Some(self.confirm_recent_project_selection());
                }

                if matches!(command, Command::FilePickerConfirmSelection)
                    && matches!(
                        self.app_state.command_palette_mode(),
                        Some(CommandPaletteMode::ThemeSelector)
                    )
                {
                    return Some(self.confirm_theme_selection());
                }

                if matches!(command, Command::FilePickerConfirmSelection)
                    && matches!(
                        self.app_state.command_palette_mode(),
                        Some(CommandPaletteMode::CodeAction)
                    )
                {
                    return Some(self.confirm_code_action_selection());
                }

                if matches!(command, Command::FilePickerConfirmSelection)
                    && matches!(
                        self.app_state.command_palette_mode(),
                        Some(CommandPaletteMode::PythonEnvSelector)
                    )
                {
                    return Some(self.confirm_python_env_selection());
                }

                if matches!(command, Command::FilePickerConfirmSelection)
                    && matches!(
                        self.app_state.command_palette_mode(),
                        Some(CommandPaletteMode::LspRename)
                    )
                {
                    return Some(self.confirm_lsp_rename_prompt());
                }

                if matches!(command, Command::FilePickerConfirmSelection)
                    && matches!(
                        self.app_state.command_palette_mode(),
                        Some(CommandPaletteMode::InFileSearch)
                    )
                {
                    let was_terminal_search = self.terminal_search_palette_active;
                    self.terminal_search_palette_active = false;

                    let report = dispatch_command(&mut self.app_state, command.clone());
                    if was_terminal_search {
                        // Terminal search: close palette and return focus to the
                        // terminal panel.  The grid search was already performed by
                        // `sync_in_file_search_with_palette_query` while the user
                        // typed; we only need to close the overlay and restore the
                        // terminal mode.
                        let _ = self.release_focus_mode_to_editor();
                        // Re-enter terminal normal so n/N continue to work.
                        let _ = self.app_state.apply_mode_event(ModeEvent::FocusTerminal);
                        let _ = self
                            .app_state
                            .apply_mode_event(ModeEvent::EnterTerminalNormal);
                        let focus_changed = self.focus_manager.set(FocusTarget::BottomPanel);
                        if focus_changed {
                            self.input_handler.clear_pending_prefix();
                        }
                        self.clear_palette_ime_commit_suppression();
                        self.mark_focused_terminal_layout_dirty();
                        return Some(true);
                    }

                    if report.state_changed {
                        let prev_scroll = self.app_state.target_scroll_y;
                        let viewport_lines = self.editor_viewport_lines();
                        self.app_state.auto_scroll_to_cursor(viewport_lines);
                        if (self.app_state.target_scroll_y - prev_scroll).abs() > f32::EPSILON {
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
                    return Some(report.request_redraw || report.success);
                }

                let file_before = self.app_state.active_file().map(PathBuf::from);
                let palette_mode_before = if matches!(command, Command::FilePickerConfirmSelection)
                {
                    self.app_state.command_palette_mode()
                } else {
                    None
                };
                let confirmed_from_fuzzy_picker =
                    matches!(command, Command::FilePickerConfirmSelection)
                        && self.app_state.active_buffer_is_fuzzy_picker();

                let is_open_file = matches!(command, Command::OpenFile(_));
                let report = {
                    let (app_state, clipboard) = (&mut self.app_state, &mut self.clipboard);
                    dispatch_command_with_clipboard(app_state, command.clone(), Some(clipboard))
                };
                self.reconcile_highlight_spans_with_pending_edits();

                // Command palette can open PythonEnvSelector without closing the overlay.
                // Keep palette focus and kick off the async environment scan.
                if self.app_state.command_palette_mode()
                    == Some(CommandPaletteMode::PythonEnvSelector)
                {
                    if let Some(workspace_root) = self
                        .app_state
                        .workspace_root_path()
                        .map(|p| p.to_path_buf())
                    {
                        self.submit(RequestSpec {
                            revision_id: 0,
                            topic: RequestTopic::SystemTask,
                            payload: WorkerRequestPayload::ScanPythonEnvironments {
                                workspace_root,
                            },
                        });
                    }
                    self.arm_palette_ime_commit_suppression();
                    self.focus_manager.set(FocusTarget::OverlayLayer);
                    return Some(true);
                }

                let file_after = self.app_state.active_file().map(PathBuf::from);
                let file_changed = report.success && file_after != file_before;
                let mut parsed_after_file_change = false;

                if file_changed {
                    self.invalidate_highlights_and_parse_active_buffer();
                    parsed_after_file_change = true;

                    if let Some(path) = file_after.as_ref() {
                        self.explorer_reveal_file(path);
                    }

                    self.submit_lsp_did_open_for_active_file();
                    if !is_open_file {
                        self.submit_active_buffer_git_baseline_refresh();
                    }
                    let _ = self.sync_focus_mode_for_active_buffer();

                    if let Some(path) = file_after.as_ref() {
                        self.submit_lsp_check_for_path(path.clone());
                    }
                }

                if report.success {
                    let viewport_lines = self.editor_viewport_lines();
                    if palette_mode_before == Some(CommandPaletteMode::DocumentSymbols) {
                        self.app_state.center_cursor_line(viewport_lines);
                    } else {
                        self.app_state.auto_scroll_to_cursor(viewport_lines);
                    }
                }

                if report.state_changed {
                    self.editor_needs_layout = true;
                    self.editor_caret_needs_layout = false;
                }
                if (report.state_changed || file_changed) && !parsed_after_file_change {
                    self.submit_parse_for_active_buffer(true);
                }

                if !is_open_file {
                    if self.focus_manager.set(FocusTarget::CenterEditor) {
                        self.input_handler.clear_pending_prefix();
                    }
                    let _ = self.release_focus_mode_to_editor();
                    if confirmed_from_fuzzy_picker
                        && !self.app_state.active_buffer_is_fuzzy_picker()
                        && self.app_state.current_mode() != EditorMode::Normal
                    {
                        let _ = self.app_state.apply_mode_event(ModeEvent::EnterNormal);
                    }
                }

                Some(report.request_redraw || report.success)
            }
            _ => None,
        }
    }
}
