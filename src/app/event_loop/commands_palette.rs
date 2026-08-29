use super::*;
use crate::{app::command_palette::PaletteVimAction, core::commands::PaletteVimKey};

impl AppShell {
    pub(super) fn handle_palette_vim_input(&mut self, key: PaletteVimKey) -> Option<bool> {
        let mode = self.app_state.command_palette_mode();
        let has_result_list = !matches!(
            mode,
            Some(
                CommandPaletteMode::ExplorerCreateFile
                    | CommandPaletteMode::ExplorerCreateFolder
                    | CommandPaletteMode::ExplorerRenameFull
                    | CommandPaletteMode::ExplorerRenameBase
                    | CommandPaletteMode::ExplorerPasteFile
                    | CommandPaletteMode::LspRename
            )
        );

        // Fuzzy-picker buffers store their query (and Vim state) on the buffer,
        // not the overlay palette, and refresh results through the async search
        // plumbing the event loop owns — so route them separately.
        if self.app_state.active_buffer_is_fuzzy_picker() {
            let outcome = self.app_state.fuzzy_picker_vim_input(key, has_result_list);
            if outcome.text_changed {
                self.submit_active_palette_fzf_search();
                self.submit_fuzzy_picker_preview_load();
            }
            return match outcome.action {
                PaletteVimAction::Consumed | PaletteVimAction::Ignore => Some(true),
                PaletteVimAction::ListNext => self.handle_palette_and_open_command(
                    &Command::OverlaySelectNext,
                    1,
                    &Command::OverlaySelectNext,
                ),
                PaletteVimAction::ListPrev => self.handle_palette_and_open_command(
                    &Command::OverlaySelectPrev,
                    1,
                    &Command::OverlaySelectPrev,
                ),
                PaletteVimAction::Confirm => self.handle_palette_and_open_command(
                    &Command::FilePickerConfirmSelection,
                    1,
                    &Command::FilePickerConfirmSelection,
                ),
                PaletteVimAction::Close => self.handle_palette_and_open_command(
                    &Command::CloseFilePicker,
                    1,
                    &Command::CloseFilePicker,
                ),
            };
        }

        match self
            .app_state
            .command_palette_vim_input(key, has_result_list)
        {
            PaletteVimAction::Consumed | PaletteVimAction::Ignore => Some(true),
            PaletteVimAction::ListNext => self.handle_palette_and_open_command(
                &Command::OverlaySelectNext,
                1,
                &Command::OverlaySelectNext,
            ),
            PaletteVimAction::ListPrev => self.handle_palette_and_open_command(
                &Command::OverlaySelectPrev,
                1,
                &Command::OverlaySelectPrev,
            ),
            PaletteVimAction::Confirm => self.handle_palette_and_open_command(
                &Command::FilePickerConfirmSelection,
                1,
                &Command::FilePickerConfirmSelection,
            ),
            PaletteVimAction::Close => self.handle_palette_and_open_command(
                &Command::CloseFilePicker,
                1,
                &Command::CloseFilePicker,
            ),
        }
    }

    pub(super) fn handle_palette_and_open_command(
        &mut self,
        command: &Command,
        repeat_count: usize,
        command_for_post_hooks: &Command,
    ) -> Option<bool> {
        match command {
            Command::PaletteVimInput(key) => self.handle_palette_vim_input(*key),
            Command::OpenFilePicker
            | Command::OpenFileFinder
            | Command::OpenCommandPalette
            | Command::OpenVimCommand
            | Command::OpenWorkspaceSymbols
            | Command::OpenDocumentSymbols
            | Command::FetchLeetCodeProblem
            | Command::LspRename
            | Command::OpenInFileSearch
            | Command::SearchInFiles
            | Command::OpenFileHistory
            | Command::OpenThemeSelector
            | Command::OpenHelp
            | Command::OpenExtensionsManager => {
                let opens_center_buffer = matches!(
                    command,
                    Command::OpenFileFinder
                        | Command::SearchInFiles
                        | Command::OpenFileHistory
                        | Command::OpenHelp
                        | Command::OpenExtensionsManager
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
                        // Row 0 is the NEWEST entry (list is reversed), so preview the
                        // index that row actually maps to — not undo_stack[0] (oldest).
                        let first_index = match self.app_state.command_palette_selected_action() {
                            Some(
                                crate::app::command_palette::CommandPaletteAction::SelectFileHistoryEntry(
                                    index,
                                ),
                            ) => index,
                            _ => 0,
                        };
                        let _ = self.app_state.preview_file_history_index(first_index);
                        self.refresh_file_history_preview();
                    }
                    if matches!(command_for_post_hooks, Command::OpenDocumentSymbols) {
                        self.submit_lsp_document_symbols();
                    }
                    if self.app_state.command_palette_mode()
                        == Some(CommandPaletteMode::CommandPalette)
                    {
                        self.app_state.set_palette_recent_commands(
                            self.persistent_state.recent_commands.clone(),
                        );
                    }
                    if self.app_state.command_palette_mode()
                        == Some(CommandPaletteMode::ThemeSelector)
                    {
                        // Active theme first + selected, THEN preview: row 0 is
                        // the current theme, so opening the picker previews the
                        // theme already applied instead of jumping to whatever
                        // sorts first alphabetically.
                        let current_profile = ThemeConfig::resolved_profile(
                            self.persistent_state.configured_theme_profile(),
                        );
                        request_redraw |= self
                            .app_state
                            .promote_theme_selector_current(&current_profile);
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
            Command::RemoveRecentProject
                if self.app_state.command_palette_mode()
                    == Some(CommandPaletteMode::RecentProjects) =>
            {
                Some(self.remove_recent_project_selection())
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
                        self.refresh_file_history_preview();
                    }
                } else if self.app_state.command_palette_mode()
                    == Some(CommandPaletteMode::ThemeSelector)
                {
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
                if self.app_state.command_palette_mode() == Some(CommandPaletteMode::ThemeSelector)
                {
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
                    && self.app_state.command_palette_mode()
                        == Some(CommandPaletteMode::ThemeSelector)
                {
                    self.preview_selected_theme_from_picker();
                }
                Some(report.request_redraw || report.state_changed)
            }

            Command::FilePickerAppendQuery(_)
            | Command::FilePickerBackspaceQuery
            | Command::PaletteMoveCursorLeft
            | Command::PaletteMoveCursorRight
            | Command::PaletteMoveCursorToStart
            | Command::PaletteMoveCursorToEnd
            | Command::PaletteDeleteCharForward
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
                            | CommandPaletteMode::ExplorerPasteFile
                    )
                );
                let was_terminal_search = self.terminal_search_palette_active;
                self.terminal_search_palette_active = false;
                let was_theme_selector = self.app_state.command_palette_mode()
                    == Some(CommandPaletteMode::ThemeSelector);
                let is_paste_popup = matches!(
                    self.app_state.command_palette_mode(),
                    Some(CommandPaletteMode::ExplorerPasteFile)
                );
                if is_paste_popup {
                    self.pending_paste_source_path = None;
                    self.pending_paste_target_dir = None;
                }

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
                                | CommandPaletteMode::ExplorerPasteFile
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

                // `:w` / `:wq` / `:x` / `:q` typed while EDITING A CANVAS CARD act on
                // the card (write its file / leave the card edit), not on the focal
                // buffer — the core palette dispatch would send `SaveFile` straight
                // to core, bypassing the shell's card gate, and write the wrong file.
                if matches!(command, Command::FilePickerConfirmSelection)
                    && matches!(
                        self.app_state.command_palette_mode(),
                        Some(CommandPaletteMode::VimCommand)
                    )
                    && let Some(handled) = self.confirm_vim_command_for_card()
                {
                    return Some(handled);
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
                        Some(CommandPaletteMode::DartEnvSelector)
                    )
                {
                    return Some(self.confirm_dart_env_selection());
                }

                if matches!(command, Command::FilePickerConfirmSelection)
                    && matches!(
                        self.app_state.command_palette_mode(),
                        Some(CommandPaletteMode::LeetCodeProblemInput)
                    )
                {
                    return Some(self.confirm_leetcode_problem_input());
                }

                if matches!(command, Command::FilePickerConfirmSelection)
                    && matches!(
                        self.app_state.command_palette_mode(),
                        Some(CommandPaletteMode::LeetCodeLanguageSelector)
                    )
                {
                    return Some(match self.app_state.command_palette_selected_action() {
                        Some(CommandPaletteAction::FetchLeetCodeWithLanguage { .. }) => {
                            self.confirm_fetch_leetcode_language_selection()
                        }
                        _ => self.confirm_leetcode_language_selection(),
                    });
                }

                if matches!(command, Command::FilePickerConfirmSelection)
                    && matches!(
                        self.app_state.command_palette_mode(),
                        Some(CommandPaletteMode::LspRename)
                    )
                {
                    return Some(self.confirm_lsp_rename_prompt());
                }

                // Record whichever palette command is about to run as the most
                // recent one (RECENT group on the next open). Runs before the
                // intercepts below so fire-and-execute commands count too.
                if matches!(command, Command::FilePickerConfirmSelection)
                    && self.app_state.command_palette_mode()
                        == Some(CommandPaletteMode::CommandPalette)
                    && let Some(crate::app::command_palette::CommandPaletteAction::ExecuteCommand(
                        id,
                    )) = self.app_state.command_palette_selected_action()
                {
                    self.persistent_state.push_recent_command(&id);
                    self.persistent_state.save();
                }

                // Enter on a command-palette row runs the command through the
                // FULL event-loop path (`handle_command_with_count`), never the
                // core-only dispatch inside `confirm_selection`. Core-only had
                // two failure modes for event-loop commands: sub-dispatchers
                // without an arm hit their `unreachable!` and aborted the app
                // (real SIGABRT 2026-08-21), and passthrough commands returned
                // "handled by event loop" with nobody actually handling them.
                if matches!(command, Command::FilePickerConfirmSelection)
                    && self.app_state.command_palette_mode()
                        == Some(CommandPaletteMode::CommandPalette)
                    && let Some(crate::app::command_palette::CommandPaletteAction::ExecuteCommand(
                        id,
                    )) = self.app_state.command_palette_selected_action()
                {
                    let parsed = crate::core::command_ids::parse(&id, self.app_state.active_file());
                    let _ = self.app_state.close_command_palette();
                    if let Ok(result) = self.app_state.apply_mode_event(ModeEvent::ExitFocus) {
                        if result.changed {
                            self.editor_needs_layout = true;
                        }
                    }
                    self.focus_manager.set(FocusTarget::CenterEditor);
                    self.input_handler.clear_pending_prefix();
                    self.clear_palette_ime_commit_suppression();
                    return match parsed {
                        Some(next) => Some(self.handle_command_with_count(next, 1)),
                        None => {
                            self.show_transient_toast(format!("Unknown command: {id}"));
                            Some(true)
                        }
                    };
                }

                if matches!(command, Command::FilePickerConfirmSelection)
                    && matches!(
                        self.app_state.command_palette_mode(),
                        Some(CommandPaletteMode::FileHistory)
                    )
                {
                    return Some(self.confirm_file_history_selection());
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

                // Close the search/file-picker tab after opening the file,
                // like ReferencesOpenSelection. The open switched the active
                // buffer to the file, so find-and-close the picker wherever
                // it sits instead of requiring it to still be active.
                if report.success
                    && confirmed_from_fuzzy_picker
                    && matches!(
                        palette_mode_before,
                        Some(CommandPaletteMode::LiveGrep | CommandPaletteMode::FilePicker)
                    )
                    && self.app_state.close_fuzzy_picker_buffer()
                {
                    self.editor_needs_layout = true;
                }

                // Command palette can open the LeetCode language picker without
                // closing the overlay. Repopulate it with MRU-sorted languages
                // (the dispatch arm opened it empty) and keep palette focus.
                if self.app_state.command_palette_mode()
                    == Some(CommandPaletteMode::LeetCodeLanguageSelector)
                {
                    self.refresh_leetcode_language_items();
                    self.arm_palette_ime_commit_suppression();
                    self.focus_manager.set(FocusTarget::OverlayLayer);
                    return Some(true);
                }

                // Command palette can open the LeetCode problem-input prompt
                // without closing the overlay. Keep palette focus so the user
                // can type a problem ID/slug/URL (the input has no list items).
                if self.app_state.command_palette_mode()
                    == Some(CommandPaletteMode::LeetCodeProblemInput)
                {
                    self.arm_palette_ime_commit_suppression();
                    self.focus_manager.set(FocusTarget::OverlayLayer);
                    return Some(true);
                }

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

                // Command palette can open DartEnvSelector without closing the overlay.
                // Keep palette focus and kick off the async environment scan.
                if self.app_state.command_palette_mode()
                    == Some(CommandPaletteMode::DartEnvSelector)
                {
                    if let Some(workspace_root) = self
                        .app_state
                        .workspace_root_path()
                        .map(|p| p.to_path_buf())
                    {
                        self.submit(RequestSpec {
                            revision_id: 0,
                            topic: RequestTopic::SystemTask,
                            payload: WorkerRequestPayload::ScanDartEnvironments { workspace_root },
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

                    // Restore cached LeetCode test cases for the opened file
                    // (no-op for files without a netherize-leetcode header).
                    self.app_state.load_leetcode_cases_for_active_file();

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
