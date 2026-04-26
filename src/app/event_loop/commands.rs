use super::*;
use crate::{
    app::clipboard::ClipboardProvider,
    app::command_palette::CommandPaletteMode,
    app::input::{LeapState, LeapTarget, generate_leap_labels},
    core::command_dispatch::{
        DispatchReport, dispatch_command_with_clipboard_count,
        dispatch_command_with_clipboard_count_with_terminal,
    },
};

fn dispatch_palette_overlay_command(
    app_state: &mut AppState,
    clipboard: &mut dyn ClipboardProvider,
    command: Command,
) -> crate::core::command_dispatch::DispatchReport {
    match command {
        Command::EditorPaste | Command::PasteSystemClipboard => {
            dispatch_command_with_clipboard(app_state, command, Some(clipboard))
        }
        _ => dispatch_command(app_state, command),
    }
}

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

    fn switch_workspace_to(&mut self, root_path: PathBuf) -> bool {
        self.prepare_for_workspace_switch();

        if let Err(err) = self.app_state.attach_workspace(root_path.clone()) {
            eprintln!("[AppShell] attach_workspace failed: {err}");
            return false;
        }

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

    fn explorer_rename_base_selection(name: &str) -> (usize, usize) {
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

    pub(super) fn handle_command(&mut self, command: Command) -> bool {
        self.handle_command_with_count(command, 1)
    }

    fn reconcile_highlight_spans_with_pending_edits(&mut self) {
        let edits = self.app_state.take_highlight_edits();
        if edits.is_empty() {
            return;
        }

        crate::syntax::highlight::apply_highlight_edits(&mut self.highlight_spans, &edits);
        crate::syntax::highlight::apply_highlight_edits(&mut self.semantic_highlight_spans, &edits);

        // Store an incremental-parse hint when the transaction was a single edit.
        // Multiple edits (undo/redo, replace-all, paste of many chars) clear the hint
        // so the worker falls back to a safe full reparse.
        self.last_syntax_edit_hint = if edits.len() == 1 {
            Some(SyntaxEditHint {
                start_byte: edits[0].start,
                old_end_byte: edits[0].old_end,
                new_end_byte: edits[0].new_end,
            })
        } else {
            None
        };
    }

    fn dispatch_command_with_focused_terminal(
        &mut self,
        command: Command,
        repeat_count: usize,
    ) -> DispatchReport {
        let focus_target = self.focus_manager.current();
        let active_terminal_session = self.app_state.active_terminal_session_id();
        let active_buffer_is_terminal = self.app_state.active_buffer_is_terminal();

        if active_buffer_is_terminal && focus_target == FocusTarget::CenterEditor {
            let terminal = active_terminal_session
                .and_then(|session_id| self.terminal_buffer_grids.get_mut(&session_id));
            let (app_state, clipboard) = (&mut self.app_state, &mut self.clipboard);
            return dispatch_command_with_clipboard_count_with_terminal(
                app_state,
                command,
                repeat_count,
                Some(clipboard),
                terminal,
            );
        }

        if focus_target == FocusTarget::BottomPanel {
            let terminal = Some(&mut self.terminal_grid);
            let (app_state, clipboard) = (&mut self.app_state, &mut self.clipboard);
            return dispatch_command_with_clipboard_count_with_terminal(
                app_state,
                command,
                repeat_count,
                Some(clipboard),
                terminal,
            );
        }

        let (app_state, clipboard) = (&mut self.app_state, &mut self.clipboard);
        dispatch_command_with_clipboard_count_with_terminal(
            app_state,
            command,
            repeat_count,
            Some(clipboard),
            None,
        )
    }

    fn finalize_settings_change(&mut self) -> bool {
        self.apply_scaled_runtime_config();
        let _ = self.ui_config.save_user_override();
        self.editor_needs_layout = true;
        self.editor_caret_needs_layout = false;
        true
    }

    fn update_active_settings_edit_draft(&mut self, text: String) -> bool {
        let Some(state) = self.app_state.active_settings_buffer_mut() else {
            return false;
        };
        let Some(editing) = &mut state.editing else {
            return false;
        };
        editing.draft = text;
        self.editor_needs_layout = true;
        self.editor_caret_needs_layout = false;
        true
    }

    fn adjust_selected_setting(&mut self, delta: i32) -> bool {
        let Some(selected) = self
            .app_state
            .active_settings_buffer()
            .and_then(|state| state.selected_item())
            .cloned()
        else {
            return false;
        };

        if self.app_state.settings_is_editing() {
            return match selected {
                crate::app::app_state::SettingItem::FontSize { current } => {
                    let next = (current + delta as f32 * 0.5).clamp(8.0, 40.0);
                    self.update_active_settings_edit_draft(format!("{next:.1}"))
                }
                crate::app::app_state::SettingItem::LineHeight { current } => {
                    let next = (current + delta as f32 * 0.5).clamp(10.0, 64.0);
                    self.update_active_settings_edit_draft(format!("{next:.1}"))
                }
                crate::app::app_state::SettingItem::IndentTabWidth { current } => {
                    let next = (current as i32 + delta).clamp(1, 8) as u8;
                    self.update_active_settings_edit_draft(next.to_string())
                }
                crate::app::app_state::SettingItem::SidebarWidth { current } => {
                    let next = (current + delta * 20).clamp(160, 640);
                    self.update_active_settings_edit_draft(next.to_string())
                }
                crate::app::app_state::SettingItem::RightSidebarWidth { current } => {
                    let next = (current + delta * 20).clamp(180, 720);
                    self.update_active_settings_edit_draft(next.to_string())
                }
                crate::app::app_state::SettingItem::BottomPanelHeight { current } => {
                    let next = (current + delta * 20).clamp(120, 520);
                    self.update_active_settings_edit_draft(next.to_string())
                }
                _ => false,
            };
        }

        match selected {
            crate::app::app_state::SettingItem::FontSize { current } => {
                let next = (current + delta as f32 * 0.5).clamp(8.0, 40.0);
                self.base_theme.editor.font_size = next;
                self.ui_config.editor.font_size = next;
                if let Some(state) = self.app_state.active_settings_buffer_mut()
                    && let Some(crate::app::app_state::SettingItem::FontSize { current }) =
                        state.selected_item_mut()
                {
                    *current = next;
                }
                self.finalize_settings_change()
            }
            crate::app::app_state::SettingItem::LineHeight { current } => {
                let next = (current + delta as f32 * 0.5).clamp(10.0, 64.0);
                self.base_theme.editor.line_height = next;
                self.ui_config.editor.line_height = next;
                if let Some(state) = self.app_state.active_settings_buffer_mut()
                    && let Some(crate::app::app_state::SettingItem::LineHeight { current }) =
                        state.selected_item_mut()
                {
                    *current = next;
                }
                self.finalize_settings_change()
            }
            crate::app::app_state::SettingItem::SidebarWidth { current } => {
                let next = (current + delta * 20).clamp(160, 640);
                self.ui_config.docks.left.size_px = next as f32;
                if let Some(state) = self.app_state.active_settings_buffer_mut()
                    && let Some(crate::app::app_state::SettingItem::SidebarWidth { current }) =
                        state.selected_item_mut()
                {
                    *current = next;
                }
                self.finalize_settings_change()
            }
            crate::app::app_state::SettingItem::RightSidebarWidth { current } => {
                let next = (current + delta * 20).clamp(180, 720);
                self.ui_config.docks.right.size_px = next as f32;
                if let Some(state) = self.app_state.active_settings_buffer_mut()
                    && let Some(crate::app::app_state::SettingItem::RightSidebarWidth { current }) =
                        state.selected_item_mut()
                {
                    *current = next;
                }
                self.finalize_settings_change()
            }
            crate::app::app_state::SettingItem::BottomPanelHeight { current } => {
                let next = (current + delta * 20).clamp(120, 520);
                self.ui_config.docks.bottom.size_px = next as f32;
                if let Some(state) = self.app_state.active_settings_buffer_mut()
                    && let Some(crate::app::app_state::SettingItem::BottomPanelHeight { current }) =
                        state.selected_item_mut()
                {
                    *current = next;
                }
                self.finalize_settings_change()
            }
            crate::app::app_state::SettingItem::IndentTabWidth { current } => {
                let next = (current as i32 + delta).clamp(1, 8) as u8;
                self.ui_config.indent.tab_width = next;
                self.app_state.set_indent_config(self.ui_config.indent);
                if let Some(state) = self.app_state.active_settings_buffer_mut()
                    && let Some(crate::app::app_state::SettingItem::IndentTabWidth { current }) =
                        state.selected_item_mut()
                {
                    *current = next;
                }
                self.finalize_settings_change()
            }
            _ => false,
        }
    }

    fn activate_selected_setting(&mut self) -> bool {
        if self.app_state.settings_is_editing() {
            return self.commit_settings_editing();
        }

        let Some(selected) = self
            .app_state
            .active_settings_buffer()
            .and_then(|state| state.selected_item())
            .cloned()
        else {
            return false;
        };

        match selected {
            crate::app::app_state::SettingItem::ThemeSelector { .. } => {
                self.handle_command(Command::OpenThemeSelector)
            }
            crate::app::app_state::SettingItem::IndentInsertSpaces { enabled } => {
                let next = !enabled;
                self.ui_config.indent.insert_spaces = next;
                self.app_state.set_indent_config(self.ui_config.indent);
                if let Some(state) = self.app_state.active_settings_buffer_mut()
                    && let Some(crate::app::app_state::SettingItem::IndentInsertSpaces { enabled }) =
                        state.selected_item_mut()
                {
                    *enabled = next;
                }
                let _ = self.ui_config.save_user_override();
                self.editor_needs_layout = true;
                self.editor_caret_needs_layout = false;
                true
            }
            crate::app::app_state::SettingItem::FontFamily { .. }
            | crate::app::app_state::SettingItem::FontSize { .. }
            | crate::app::app_state::SettingItem::LineHeight { .. }
            | crate::app::app_state::SettingItem::IndentTabWidth { .. }
            | crate::app::app_state::SettingItem::SidebarWidth { .. }
            | crate::app::app_state::SettingItem::RightSidebarWidth { .. }
            | crate::app::app_state::SettingItem::BottomPanelHeight { .. } => {
                let changed = self.app_state.settings_begin_editing();
                if changed {
                    if let Ok(result) = self
                        .app_state
                        .apply_mode_event(crate::core::mode::ModeEvent::EnterInsert)
                    {
                        let _ = result.changed;
                    }
                    self.editor_needs_layout = true;
                    self.editor_caret_needs_layout = false;
                }
                changed
            }
            crate::app::app_state::SettingItem::UiRounding { enabled, radius_px } => {
                let next_radius = if !enabled || radius_px <= 0.0 {
                    8.0
                } else if radius_px < 12.0 {
                    16.0
                } else {
                    0.0
                };
                let next_enabled = next_radius > 0.0;
                if let Some(state) = self.app_state.active_settings_buffer_mut()
                    && let Some(crate::app::app_state::SettingItem::UiRounding {
                        enabled,
                        radius_px,
                    }) = state.selected_item_mut()
                {
                    *enabled = next_enabled;
                    *radius_px = next_radius;
                }
                self.ui_config.border_radius_px = next_radius;
                self.apply_scaled_runtime_config();
                let _ = self.ui_config.save_user_override();
                true
            }
        }
    }

    fn commit_settings_editing(&mut self) -> bool {
        let Some((kind, draft)) = self
            .app_state
            .active_settings_buffer()
            .and_then(|state| state.editing.as_ref())
            .map(|editing| (editing.kind.clone(), editing.draft.clone()))
        else {
            return false;
        };

        let trimmed = draft.trim();
        let mut changed = false;

        match kind {
            crate::app::app_state::SettingsEditingKind::FontFamily => {
                self.base_theme.editor.font_family =
                    (!trimmed.is_empty()).then(|| trimmed.to_string());
                self.ui_config.editor.font_family =
                    (!trimmed.is_empty()).then(|| trimmed.to_string());
                if let Some(state) = self.app_state.active_settings_buffer_mut()
                    && let Some(crate::app::app_state::SettingItem::FontFamily { current }) =
                        state.selected_item_mut()
                {
                    *current = trimmed.to_string();
                }
                changed = true;
            }
            crate::app::app_state::SettingsEditingKind::FontSize => {
                if let Ok(value) = trimmed.parse::<f32>() {
                    let value = value.clamp(8.0, 40.0);
                    self.base_theme.editor.font_size = value;
                    self.ui_config.editor.font_size = value;
                    if let Some(state) = self.app_state.active_settings_buffer_mut()
                        && let Some(crate::app::app_state::SettingItem::FontSize { current }) =
                            state.selected_item_mut()
                    {
                        *current = value;
                    }
                    changed = true;
                }
            }
            crate::app::app_state::SettingsEditingKind::LineHeight => {
                if let Ok(value) = trimmed.parse::<f32>() {
                    let value = value.clamp(10.0, 64.0);
                    self.base_theme.editor.line_height = value;
                    self.ui_config.editor.line_height = value;
                    if let Some(state) = self.app_state.active_settings_buffer_mut()
                        && let Some(crate::app::app_state::SettingItem::LineHeight { current }) =
                            state.selected_item_mut()
                    {
                        *current = value;
                    }
                    changed = true;
                }
            }
            crate::app::app_state::SettingsEditingKind::SidebarWidth => {
                if let Ok(value) = trimmed.parse::<i32>() {
                    let value = value.clamp(160, 640);
                    self.ui_config.docks.left.size_px = value as f32;
                    if let Some(state) = self.app_state.active_settings_buffer_mut()
                        && let Some(crate::app::app_state::SettingItem::SidebarWidth { current }) =
                            state.selected_item_mut()
                    {
                        *current = value;
                    }
                    changed = true;
                }
            }
            crate::app::app_state::SettingsEditingKind::RightSidebarWidth => {
                if let Ok(value) = trimmed.parse::<i32>() {
                    let value = value.clamp(180, 720);
                    self.ui_config.docks.right.size_px = value as f32;
                    if let Some(state) = self.app_state.active_settings_buffer_mut()
                        && let Some(crate::app::app_state::SettingItem::RightSidebarWidth {
                            current,
                        }) = state.selected_item_mut()
                    {
                        *current = value;
                    }
                    changed = true;
                }
            }
            crate::app::app_state::SettingsEditingKind::BottomPanelHeight => {
                if let Ok(value) = trimmed.parse::<i32>() {
                    let value = value.clamp(120, 520);
                    self.ui_config.docks.bottom.size_px = value as f32;
                    if let Some(state) = self.app_state.active_settings_buffer_mut()
                        && let Some(crate::app::app_state::SettingItem::BottomPanelHeight {
                            current,
                        }) = state.selected_item_mut()
                    {
                        *current = value;
                    }
                    changed = true;
                }
            }
            crate::app::app_state::SettingsEditingKind::IndentTabWidth => {
                if let Ok(value) = trimmed.parse::<u8>() {
                    let value = value.clamp(1, 8);
                    self.ui_config.indent.tab_width = value;
                    self.app_state.set_indent_config(self.ui_config.indent);
                    if let Some(state) = self.app_state.active_settings_buffer_mut()
                        && let Some(crate::app::app_state::SettingItem::IndentTabWidth { current }) =
                            state.selected_item_mut()
                    {
                        *current = value;
                    }
                    changed = true;
                }
            }
        }

        if changed {
            self.apply_scaled_runtime_config();
            let _ = self.ui_config.save_user_override();
        }
        let cancelled = self.app_state.settings_cancel_editing();
        if changed || cancelled {
            if self.app_state.current_mode() == crate::core::mode::EditorMode::Insert {
                if let Ok(result) = self
                    .app_state
                    .apply_mode_event(crate::core::mode::ModeEvent::Escape)
                {
                    let _ = result.changed;
                }
            }
            self.editor_needs_layout = true;
            self.editor_caret_needs_layout = false;
        }
        changed || cancelled
    }

    fn mark_focused_terminal_layout_dirty(&mut self) {
        if self.app_state.active_buffer_is_terminal()
            && self.focus_manager.current() == FocusTarget::CenterEditor
        {
            self.buffer_terminal_needs_layout = true;
        } else {
            self.terminal_needs_layout = true;
        }
    }

    fn handle_terminal_normal_command(
        &mut self,
        command: &Command,
        repeat_count: usize,
    ) -> Option<bool> {
        let terminal_copy_routing = self.app_state.current_mode() == EditorMode::TerminalNormal
            || matches!(command, Command::SwitchMode(ModeEvent::EnterTerminalNormal));
        if !terminal_copy_routing {
            return None;
        }

        let supported = matches!(
            command,
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
                | Command::MoveToFirstLine
                | Command::MoveToLastLine
                | Command::ScrollHalfPageUp
                | Command::ScrollHalfPageDown
                | Command::CenterCursorLine
                | Command::YankSelection
                | Command::SwitchMode(ModeEvent::EnterTerminalNormal)
                | Command::SwitchMode(ModeEvent::EnterVisual | ModeEvent::FocusTerminal)
        );
        if !supported {
            return None;
        }

        let report = self.dispatch_command_with_focused_terminal(command.clone(), repeat_count);
        if report.state_changed {
            self.mark_focused_terminal_layout_dirty();
        }
        Some(report.request_redraw || report.state_changed)
    }

    pub(super) fn handle_command_with_count(
        &mut self,
        command: Command,
        repeat_count: usize,
    ) -> bool {
        if matches!(command, Command::TerminalPaste) {
            return self.handle_terminal_paste();
        }

        if let Some(changed) = self.handle_terminal_normal_command(&command, repeat_count) {
            return changed;
        }

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
            | Command::SearchInFiles
            | Command::OpenThemeSelector => {
                let opens_fuzzy_buffer =
                    matches!(command, Command::OpenFileFinder | Command::SearchInFiles);
                let report = dispatch_command(&mut self.app_state, command);
                if report.success {
                    if opens_fuzzy_buffer {
                        self.editor_needs_layout = true;
                        self.editor_caret_needs_layout = false;
                    }
                    let focus_changed = if opens_fuzzy_buffer {
                        self.clear_palette_ime_commit_suppression();
                        self.focus_manager.set(FocusTarget::CenterEditor)
                    } else {
                        self.arm_palette_ime_commit_suppression();
                        self.focus_manager.set(FocusTarget::OverlayLayer)
                    };
                    if focus_changed {
                        self.input_handler.clear_pending_prefix();
                    }
                }
                report.request_redraw
            }
            Command::OpenSettings => {
                let theme_profile = self
                    .persistent_state
                    .configured_theme_profile()
                    .unwrap_or(self.base_theme.name.as_str())
                    .to_string();
                let font_family = self
                    .base_theme
                    .editor
                    .font_family
                    .clone()
                    .unwrap_or_default();
                self.app_state.open_settings_buffer(
                    theme_profile,
                    font_family,
                    self.base_theme.editor.font_size,
                    self.base_theme.editor.line_height,
                    self.ui_config.indent.tab_width,
                    self.ui_config.indent.insert_spaces,
                    self.ui_config.docks.left.size_px.round() as i32,
                    self.ui_config.docks.right.size_px.round() as i32,
                    self.ui_config.docks.bottom.size_px.round() as i32,
                    self.ui_config.border_radius_px > 0.0,
                    self.ui_config.border_radius_px,
                );
                let _ = self.sync_focus_mode_for_active_buffer();
                self.editor_needs_layout = true;
                self.editor_caret_needs_layout = false;
                true
            }
            Command::SettingsSelectNext => {
                let changed = self.app_state.settings_select_next();
                if changed {
                    self.editor_needs_layout = true;
                    self.editor_caret_needs_layout = false;
                }
                changed
            }
            Command::SettingsSelectPrev => {
                let changed = self.app_state.settings_select_prev();
                if changed {
                    self.editor_needs_layout = true;
                    self.editor_caret_needs_layout = false;
                }
                changed
            }
            Command::SettingsAdjustDecrease => self.adjust_selected_setting(-1),
            Command::SettingsAdjustIncrease => self.adjust_selected_setting(1),
            Command::SettingsActivate => self.activate_selected_setting(),
            Command::CloseFilePicker if self.app_state.active_buffer_is_settings() => {
                if self.app_state.settings_is_editing() {
                    let changed = self.app_state.settings_cancel_editing();
                    if changed {
                        if self.app_state.current_mode() == crate::core::mode::EditorMode::Insert {
                            if let Ok(result) = self
                                .app_state
                                .apply_mode_event(crate::core::mode::ModeEvent::Escape)
                            {
                                let _ = result.changed;
                            }
                        }
                        self.editor_needs_layout = true;
                        self.editor_caret_needs_layout = false;
                    }
                    changed
                } else {
                    self.close_current_buffer_now()
                }
            }
            Command::GitOpenLazygit => self.open_lazygit_buffer(),
            Command::GitBlameLine => self.submit_git_blame_line(),
            Command::LspHover => self.submit_lsp_hover(),
            Command::LspGoToDefinition => self.submit_lsp_definition(true),
            Command::LspPreviewDefinition => self.submit_lsp_definition(false),
            Command::LspReferences => self.submit_lsp_references(),
            Command::TriggerCompletion => self.submit_lsp_completion(),
            Command::CompletionNext => self.select_next_completion_item(),
            Command::CompletionPrev => self.select_prev_completion_item(),
            Command::CompletionAccept => self.accept_completion_item(),
            Command::CompletionClose => self.close_completion_popup(),
            Command::DiagnosticsOpenPicker => self.open_diagnostics_picker(),
            Command::ReferencesSelectNext => self.select_next_reference_item(),
            Command::ReferencesSelectPrev => self.select_prev_reference_item(),
            Command::ReferencesOpenSelection => self.open_selected_reference_item(),
            Command::DiagnosticsSelectNext => self.select_next_diagnostic_item(),
            Command::DiagnosticsSelectPrev => self.select_prev_diagnostic_item(),
            Command::DiagnosticsOpenSelection => self.open_selected_diagnostic_item(),
            Command::JumpBack => return self.execute_jump_back(),
            Command::JumpForward => return self.execute_jump_forward(),
            Command::FilePickerAppendQuery(_)
            | Command::FilePickerBackspaceQuery
            | Command::EditorPaste
            | Command::PasteSystemClipboard
                if self.app_state.active_buffer_is_settings()
                    && self.app_state.settings_is_editing() =>
            {
                let changed = match command {
                    Command::FilePickerAppendQuery(text) => {
                        self.app_state.settings_append_editing_text(&text)
                    }
                    Command::FilePickerBackspaceQuery => {
                        self.app_state.settings_backspace_editing()
                    }
                    Command::EditorPaste | Command::PasteSystemClipboard => {
                        if let Ok(text) = self.clipboard.get_text() {
                            self.app_state.settings_append_editing_text(&text)
                        } else {
                            false
                        }
                    }
                    _ => false,
                };
                if changed {
                    self.editor_needs_layout = true;
                    self.editor_caret_needs_layout = false;
                }
                changed
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
                self.app_state
                    .sync_welcome_recent_projects(&self.persistent_state.recent_projects);
                let report = dispatch_command(&mut self.app_state, command);
                report.request_redraw || report.state_changed
            }
            Command::FilePickerConfirmSelection
                if self.app_state.buffers().is_empty()
                    && (!self.app_state.is_command_palette_visible()
                        || self.app_state.command_palette_mode()
                            == Some(CommandPaletteMode::RecentProjects)) =>
            {
                self.app_state
                    .sync_welcome_recent_projects(&self.persistent_state.recent_projects);
                let selected = self.app_state.command_palette_selected_index().min(
                    self.persistent_state
                        .recent_projects
                        .len()
                        .saturating_sub(1),
                );
                let Some(root) = self.persistent_state.recent_projects.get(selected).cloned()
                else {
                    return false;
                };
                match self.app_state.attach_workspace(root.clone()) {
                    Ok(()) => {
                        self.persistent_state.push_recent(root);
                        self.persistent_state.save();
                        self.workspace_git_branch = self
                            .app_state
                            .workspace_root_path()
                            .and_then(detect_git_branch);
                        true
                    }
                    Err(err) => {
                        eprintln!("[AppShell] recent project open failed: {err}");
                        false
                    }
                }
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
                        command,
                        repeat_count,
                        Some(clipboard),
                    )
                };
                if !report.success {
                    return report.request_redraw;
                }
                if report.state_changed {
                    self.editor_needs_layout = true;
                    self.editor_caret_needs_layout = false;
                }
                self.submit_active_palette_fzf_search();
                self.submit_fuzzy_picker_preview_load();
                report.request_redraw || report.state_changed
            }
            Command::FilePickerAppendQuery(_)
            | Command::FilePickerBackspaceQuery
            | Command::EditorPaste
            | Command::PasteSystemClipboard
                if self.app_state.active_buffer_is_fuzzy_picker() =>
            {
                let report = {
                    let (app_state, clipboard) = (&mut self.app_state, &mut self.clipboard);
                    dispatch_palette_overlay_command(app_state, clipboard, command)
                };
                if !report.success {
                    return report.request_redraw;
                }
                if report.state_changed {
                    self.editor_needs_layout = true;
                    self.editor_caret_needs_layout = false;
                }
                self.submit_active_palette_fzf_search();
                self.submit_fuzzy_picker_preview_load();
                self.request_redraw();
                true
            }
            Command::FilePickerAppendQuery(_)
            | Command::FilePickerBackspaceQuery
            | Command::EditorPaste
            | Command::PasteSystemClipboard
                if self.app_state.current_mode() == EditorMode::PaletteFocus
                    && self.app_state.is_command_palette_visible() =>
            {
                let is_typing_edit = matches!(
                    &command,
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
                    dispatch_palette_overlay_command(app_state, clipboard, command)
                };
                if !report.success {
                    return report.request_redraw;
                }

                match self.app_state.command_palette_mode() {
                    Some(
                        CommandPaletteMode::ExplorerCreateFile
                        | CommandPaletteMode::ExplorerCreateFolder
                        | CommandPaletteMode::ExplorerRenameFull
                        | CommandPaletteMode::ExplorerRenameBase,
                    ) => {}
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
                            | CommandPaletteMode::ExplorerRenameFull
                            | CommandPaletteMode::ExplorerRenameBase
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
            Command::InsertChar(_)
            | Command::InsertText(_)
            | Command::Backspace
            | Command::Newline => {
                let report = {
                    let (app_state, clipboard) = (&mut self.app_state, &mut self.clipboard);
                    dispatch_command_with_clipboard_count(
                        app_state,
                        command,
                        repeat_count,
                        Some(clipboard),
                    )
                };
                if !report.success {
                    return report.request_redraw;
                }
                if report.state_changed {
                    self.reconcile_highlight_spans_with_pending_edits();
                    self.editor_needs_layout = true;
                    self.editor_caret_needs_layout = true;
                    let viewport_lines = self.editor_viewport_lines();
                    self.app_state.auto_scroll_to_cursor(viewport_lines);
                    self.queue_lsp_did_change_for_active_file();
                }
                let completion_changed = if report.state_changed {
                    self.refresh_open_completion_after_text_edit()
                } else {
                    false
                };
                if report.request_redraw || report.state_changed || completion_changed {
                    self.request_redraw();
                }
                report.request_redraw || report.state_changed || completion_changed
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
            Command::ExplorerStartFilter => {
                let changed = self.app_state.workspace_start_filter_input();
                if changed {
                    self.input_handler.clear_pending_prefix();
                    self.mark_explorer_dirty();
                }
                changed
            }
            Command::ExplorerClearFilter => {
                let changed = self.app_state.workspace_clear_filter();
                if changed {
                    self.input_handler.clear_pending_prefix();
                    self.mark_explorer_dirty();
                }
                changed
            }
            Command::ExplorerToggleHidden => {
                let changed = self.app_state.workspace_toggle_show_hidden();
                if !changed {
                    return false;
                }
                let Ok(rescanned) = self.app_state.rescan_workspace() else {
                    return false;
                };
                if rescanned {
                    self.mark_explorer_dirty();
                    true
                } else {
                    false
                }
            }
            Command::ExplorerToggleIgnored => {
                let changed = self.app_state.workspace_toggle_show_ignored();
                if !changed {
                    return false;
                }
                let Ok(rescanned) = self.app_state.rescan_workspace() else {
                    return false;
                };
                if rescanned {
                    self.mark_explorer_dirty();
                    true
                } else {
                    false
                }
            }
            Command::ExplorerMoveToTop => {
                self.ensure_explorer_snapshot();
                if self.explorer_snapshot.entries.is_empty() {
                    self.explorer_cursor = 0;
                    return false;
                }
                self.explorer_cursor = 0;
                let _ = self
                    .app_state
                    .workspace_select_path(&self.explorer_snapshot.entries[0].path);
                self.sidebar_needs_layout = true;
                true
            }
            Command::ExplorerMoveToBottom => {
                self.ensure_explorer_snapshot();
                if self.explorer_snapshot.entries.is_empty() {
                    self.explorer_cursor = 0;
                    return false;
                }
                self.explorer_cursor = self.explorer_snapshot.entries.len().saturating_sub(1);
                let _ = self.app_state.workspace_select_path(
                    &self.explorer_snapshot.entries[self.explorer_cursor].path,
                );
                self.sidebar_needs_layout = true;
                true
            }
            Command::ExplorerRenameFull => self.open_explorer_rename_prompt(false),
            Command::ExplorerRenameBase => self.open_explorer_rename_prompt(true),
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
            Command::TerminalPaste => self.handle_terminal_paste(),
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
                        let target_line = if cursor_line + margin + 1 >= viewport_lines {
                            cursor_line + margin + 1 - viewport_lines
                        } else {
                            0
                        };
                        self.app_state.set_target_scroll_line(target_line);
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
                let leap_state = self.generate_editor_leap_state(*target_char);
                if leap_state.targets.is_empty() {
                    self.leap_state = None;
                    false
                } else {
                    self.input_handler.set_pending_leap_label();
                    self.leap_state = Some(leap_state);
                    self.editor_needs_layout = true;
                    self.editor_caret_needs_layout = false;
                    true
                }
            }
            Command::LeapJump(label_char) => {
                let Some(mut leap_state) = self.leap_state.take() else {
                    return false;
                };

                leap_state
                    .typed_prefix
                    .push(Self::normalize_leap_target(*label_char));

                let matching_targets: Vec<LeapTarget> = leap_state
                    .targets
                    .iter()
                    .filter(|target| target.label.starts_with(&leap_state.typed_prefix))
                    .cloned()
                    .collect();

                if matching_targets.is_empty() {
                    self.editor_needs_layout = true;
                    self.editor_caret_needs_layout = false;
                    return true;
                }

                let resolved_target = matching_targets
                    .iter()
                    .find(|target| target.label == leap_state.typed_prefix)
                    .or_else(|| (matching_targets.len() == 1).then_some(&matching_targets[0]))
                    .cloned();

                if let Some(target) = resolved_target {
                    let changed = self.app_state.leap_jump_to_char(target.char_idx);
                    let viewport_lines = self.editor_viewport_lines();
                    let prev_scroll = self.app_state.target_scroll_y;
                    self.app_state.auto_scroll_to_cursor(viewport_lines);
                    if (self.app_state.target_scroll_y - prev_scroll).abs() > f32::EPSILON {
                        self.submit_parse_for_active_buffer(true);
                    }
                    self.editor_needs_layout = true;
                    self.editor_caret_needs_layout = false;
                    changed || (self.app_state.target_scroll_y - prev_scroll).abs() > f32::EPSILON
                } else {
                    leap_state.targets = matching_targets;
                    self.input_handler.set_pending_leap_label();
                    self.leap_state = Some(leap_state);
                    self.editor_needs_layout = true;
                    self.editor_caret_needs_layout = false;
                    true
                }
            }
            Command::LeapCancel => {
                let had_leap_state = self.leap_state.take().is_some();
                if had_leap_state {
                    self.editor_needs_layout = true;
                    self.editor_caret_needs_layout = false;
                }
                had_leap_state
            }
            Command::FilePickerConfirmSelection | Command::OpenFile(_) => {
                if matches!(&command, Command::FilePickerConfirmSelection)
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
                        Some(CommandPaletteMode::ThemeSelector)
                    )
                {
                    return self.confirm_theme_selection();
                }

                if matches!(&command, Command::FilePickerConfirmSelection)
                    && matches!(
                        self.app_state.command_palette_mode(),
                        Some(CommandPaletteMode::InFileSearch)
                    )
                {
                    let report = dispatch_command(&mut self.app_state, command);
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
                    self.clear_highlight_layers();

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

                    // Trigger async LSP install check khi mở file mới.
                    // Chỉ check khi file thực sự thay đổi (tránh spam check).
                    if let Some(ref path) = file_after
                        && file_after != file_before
                    {
                        self.submit_lsp_check_for_path(path.clone());
                    }
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
                        | Command::EditorPaste
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
                let auto_trigger_char = match &command {
                    Command::InsertChar(ch) => Some(*ch),
                    _ => None,
                };
                if is_typing_edit {
                    let _ = self.app_state.clear_completion();
                }
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
                    self.clear_highlight_layers();
                    self.mark_explorer_dirty();
                    let _ = self.sync_focus_mode_for_active_buffer();
                }

                if report.state_changed && is_cursor_move {
                    let prev_scroll = self.app_state.target_scroll_y;
                    let viewport_lines = self.editor_viewport_lines();
                    self.app_state.auto_scroll_to_cursor(viewport_lines);
                    if (self.app_state.target_scroll_y - prev_scroll).abs() > f32::EPSILON {
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
                    if is_typing_edit {
                        self.submit_lsp_did_change_for_active_file();
                    } else {
                        self.force_flush_lsp_did_change_for_active_file();
                    }
                }
                if report.success
                    && let Some(ch) = auto_trigger_char
                    && self.should_auto_trigger_lsp_completion_for_char(ch)
                {
                    self.submit_lsp_completion();
                }
                if report.success && should_notify_did_open {
                    self.submit_lsp_did_open_for_active_file();
                }

                report.request_redraw
            }
        }
    }

    fn handle_terminal_paste(&mut self) -> bool {
        let clipboard_text = match self.clipboard.get_text() {
            Ok(text) => text,
            Err(err) => {
                eprintln!("[terminal] paste failed: {err}");
                return false;
            }
        };
        if clipboard_text.is_empty() {
            return false;
        }

        let payload = normalize_terminal_paste_text(&clipboard_text);
        if payload.is_empty() {
            return false;
        }

        let mut changed = false;
        if self.app_state.current_mode() == EditorMode::TerminalNormal {
            if let Some(grid) = self.focused_terminal_grid_mut() {
                changed |= grid.exit_normal_mode();
            }
            if let Ok(result) = self.app_state.apply_mode_event(ModeEvent::FocusTerminal) {
                changed |= result.changed;
            }
            self.mark_focused_terminal_layout_dirty();
        }

        let Some(session_id) = self.focused_terminal_session_id() else {
            eprintln!("[terminal] paste ignored: no focused PTY session");
            return changed;
        };

        self.forward_to_terminal_session(session_id, &payload);
        changed
    }

    fn forward_to_pty(&self, text: &str) {
        if let Some(session_id) = self.focused_terminal_session_id() {
            self.forward_to_terminal_session(session_id, text);
        }
    }

    fn forward_to_terminal_session(&self, session_id: u64, text: &str) {
        self.submit(RequestSpec {
            revision_id: 0,
            topic: RequestTopic::TerminalPty,
            payload: WorkerRequestPayload::WritePtyInput {
                session_id,
                input: text.to_string(),
            },
        });
    }

    pub(super) fn dismiss_lsp_guide(&mut self) {
        self.active_lsp_guide = None;
        if let Some(renderer) = self.renderer.as_mut() {
            renderer.clear_lsp_guide_popup();
        }
    }

    pub(super) fn show_transient_toast(&mut self, message: impl Into<String>) {
        self.transient_toast = Some(TransientToast {
            message: message.into(),
            expires_at: Instant::now() + Duration::from_secs(4),
        });
    }

    pub(super) fn clear_expired_transient_toast(&mut self) -> bool {
        let expired = self
            .transient_toast
            .as_ref()
            .is_some_and(|toast| Instant::now() >= toast.expires_at);
        if !expired {
            return false;
        }

        self.transient_toast = None;
        if let Some(renderer) = self.renderer.as_mut() {
            renderer.clear_toast_popup();
        }
        true
    }

    fn lsp_install_working_dir(&self) -> Option<PathBuf> {
        self.app_state
            .workspace_root_path()
            .map(PathBuf::from)
            .or_else(|| {
                self.app_state
                    .active_file()
                    .and_then(|path| path.parent())
                    .map(PathBuf::from)
            })
            .or_else(|| std::env::current_dir().ok())
    }

    pub(super) fn accept_lsp_install_guide(&mut self) -> bool {
        let Some(guide) = self.active_lsp_guide.take() else {
            return false;
        };
        let LspInstallGuide {
            binary,
            install_cmd,
        } = guide;

        if let Some(renderer) = self.renderer.as_mut() {
            renderer.clear_lsp_guide_popup();
        }

        let mut changed = true;
        if let Some(session_id) = self.pty_session_id {
            changed |= self.handle_command(Command::FocusTerminal);
            self.forward_to_terminal_session(session_id, &format!("{install_cmd}\r"));
        } else {
            self.submit(RequestSpec {
                revision_id: 0,
                topic: RequestTopic::TerminalPty,
                payload: WorkerRequestPayload::SpawnDetachedShellCommand {
                    command: install_cmd,
                    working_dir: self.lsp_install_working_dir(),
                },
            });
            self.show_transient_toast(format!("Installing {binary} in background..."));
        }

        changed
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
        self.clear_highlight_layers();
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

    /// Helper: trả về (language_id, uri, lsp_line, lsp_character) nếu điều kiện hợp lệ.
    fn lsp_cursor_context(&self) -> Option<(String, String, u32, u32)> {
        if self.app_state.active_buffer_is_terminal() {
            return None;
        }
        let buffer = self.app_state.active_text_buffer()?;
        let language_id = buffer.language_id.clone()?;
        let uri = crate::lsp::client::path_to_lsp_uri(&buffer.path);
        let (line, col) = self.app_state.cursor_line_col();
        Some((language_id, uri, line as u32, col as u32))
    }

    fn submit_lsp_hover(&mut self) -> bool {
        self.force_flush_lsp_did_change_for_active_file();
        let Some((language_id, uri, line, character)) = self.lsp_cursor_context() else {
            return false;
        };
        let changed = self.app_state.clear_current_overlays();
        self.submit(RequestSpec {
            revision_id: 0,
            topic: RequestTopic::LspRequest,
            payload: WorkerRequestPayload::LspHoverRequest {
                language_id,
                uri,
                line,
                character,
            },
        });
        changed
    }

    /// `jump = true` => gd (go to definition). `jump = false` => gD (peek preview).
    fn submit_lsp_definition(&mut self, jump: bool) -> bool {
        self.force_flush_lsp_did_change_for_active_file();
        let Some((_language_id, uri, line, character)) = self.lsp_cursor_context() else {
            return false;
        };
        let changed = self.app_state.clear_current_overlays();
        self.submit(RequestSpec {
            revision_id: 0,
            topic: RequestTopic::LspRequest,
            payload: WorkerRequestPayload::LspDefinitionRequest {
                uri,
                line,
                character,
                jump,
            },
        });
        changed
    }

    fn submit_lsp_references(&mut self) -> bool {
        self.force_flush_lsp_did_change_for_active_file();
        let Some((_language_id, uri, line, character)) = self.lsp_cursor_context() else {
            return false;
        };
        let mut changed = self.app_state.clear_current_overlays();
        let origin_path = self.app_state.active_file().map(PathBuf::from);
        let origin_line = self.app_state.cursor_line_col().0;
        let Some(request) = self.submit(RequestSpec {
            revision_id: 0,
            topic: RequestTopic::LspRequest,
            payload: WorkerRequestPayload::LspReferencesRequest {
                uri,
                line,
                character,
            },
        }) else {
            return changed;
        };

        self.app_state.open_pending_references_buffer(
            "References",
            origin_path,
            origin_line,
            request.request_id,
        );
        self.editor_needs_layout = true;
        self.editor_caret_needs_layout = false;
        let focus_changed = self.focus_manager.set(FocusTarget::CenterEditor);
        changed = true;
        if focus_changed {
            self.input_handler.clear_pending_prefix();
        }
        self.request_redraw();
        changed || focus_changed
    }

    fn select_next_reference_item(&mut self) -> bool {
        let changed = self.app_state.references_select_next();
        if changed {
            self.submit_references_preview_load();
            self.editor_needs_layout = true;
            self.editor_caret_needs_layout = false;
        }
        changed
    }

    fn select_prev_reference_item(&mut self) -> bool {
        let changed = self.app_state.references_select_prev();
        if changed {
            self.submit_references_preview_load();
            self.editor_needs_layout = true;
            self.editor_caret_needs_layout = false;
        }
        changed
    }

    fn open_selected_reference_item(&mut self) -> bool {
        let Some(item) = self.app_state.selected_reference_item_cloned() else {
            return false;
        };

        let closed = self.close_current_buffer_now();

        if let Some((origin_path, origin_line)) = self.app_state.active_references_origin() {
            self.app_state.push_jump_entry(origin_path, origin_line);
        }

        if let Err(err) = self.app_state.open_file(item.path.clone()) {
            eprintln!("[AppShell] references open_file failed: {err}");
            return false;
        }

        self.app_state
            .jump_to_line_and_column(item.line, item.column);
        let vp = self.editor_viewport_lines();
        self.app_state.auto_scroll_to_cursor(vp);
        self.submit_lsp_check_for_path(item.path.clone());
        self.submit_lsp_did_open_for_active_file();
        self.editor_needs_layout = true;
        self.editor_caret_needs_layout = false;
        let focus_changed = self.focus_manager.set(FocusTarget::CenterEditor);
        if focus_changed {
            self.input_handler.clear_pending_prefix();
        }
        let _ = closed;
        true
    }

    fn open_diagnostics_picker(&mut self) -> bool {
        let mut items = self
            .app_state
            .diagnostics()
            .iter()
            .flat_map(|(path, diagnostics)| {
                diagnostics
                    .iter()
                    .map(|diagnostic| crate::app::app_state::DiagnosticItem {
                        file_path: path.clone(),
                        line: diagnostic.range.start.line as usize,
                        col: diagnostic.range.start.character as usize,
                        message: diagnostic.message.clone(),
                        severity: diagnostic.severity,
                    })
            })
            .collect::<Vec<_>>();

        if items.is_empty() {
            return false;
        }

        items.sort_by(|a, b| {
            a.severity
                .unwrap_or(u32::MAX)
                .cmp(&b.severity.unwrap_or(u32::MAX))
                .then_with(|| a.file_path.cmp(&b.file_path))
                .then_with(|| a.line.cmp(&b.line))
                .then_with(|| a.col.cmp(&b.col))
        });

        if let Err(err) = self.app_state.open_diagnostics_buffer(items) {
            eprintln!("[AppShell] diagnostics open buffer failed: {err}");
            return false;
        }

        self.submit_diagnostics_preview_load();
        self.editor_needs_layout = true;
        self.editor_caret_needs_layout = false;
        let focus_changed = self.focus_manager.set(FocusTarget::CenterEditor);
        if focus_changed {
            self.input_handler.clear_pending_prefix();
        }
        self.request_redraw();
        true
    }
    fn select_next_diagnostic_item(&mut self) -> bool {
        let changed = self.app_state.diagnostics_select_next();
        if changed {
            self.submit_diagnostics_preview_load();
            self.editor_needs_layout = true;
            self.editor_caret_needs_layout = false;
        }
        changed
    }

    fn select_prev_diagnostic_item(&mut self) -> bool {
        let changed = self.app_state.diagnostics_select_prev();
        if changed {
            self.submit_diagnostics_preview_load();
            self.editor_needs_layout = true;
            self.editor_caret_needs_layout = false;
        }
        changed
    }

    fn open_selected_diagnostic_item(&mut self) -> bool {
        let Some(item) = self.app_state.selected_diagnostic_item_cloned() else {
            return false;
        };

        let origin = self
            .app_state
            .active_file()
            .map(PathBuf::from)
            .map(|path| (path, self.app_state.cursor_line_col().0));

        let _ = self.app_state.close_current_buffer();

        if let Some((active_path, active_line)) = origin {
            self.app_state.push_jump_entry(active_path, active_line);
        }

        if let Err(err) = self.app_state.open_file(item.file_path.clone()) {
            eprintln!("[AppShell] diagnostics open_file failed: {err}");
            return false;
        }

        self.app_state.jump_to_line_and_column(item.line, item.col);
        let vp = self.editor_viewport_lines();
        self.app_state.auto_scroll_to_cursor(vp);
        self.submit_lsp_check_for_path(item.file_path.clone());
        self.submit_lsp_did_open_for_active_file();
        self.editor_needs_layout = true;
        self.editor_caret_needs_layout = false;
        let focus_changed = self.focus_manager.set(FocusTarget::CenterEditor);
        if focus_changed {
            self.input_handler.clear_pending_prefix();
        }
        true
    }

    fn execute_jump_back(&mut self) -> bool {
        let Some((path, line)) = self.app_state.pop_jump_back() else {
            return false;
        };
        if let Err(err) = self.app_state.open_file(path.clone()) {
            eprintln!("[AppShell] jump_back open_file failed: {err}");
            return false;
        }
        self.app_state.jump_to_line(line);
        let vp = self.editor_viewport_lines();
        self.app_state.auto_scroll_to_cursor(vp);
        self.submit_lsp_check_for_path(path);
        self.submit_lsp_did_open_for_active_file();
        self.editor_needs_layout = true;
        true
    }

    fn execute_jump_forward(&mut self) -> bool {
        let Some((path, line)) = self.app_state.pop_jump_forward() else {
            return false;
        };
        if let Err(err) = self.app_state.open_file(path.clone()) {
            eprintln!("[AppShell] jump_forward open_file failed: {err}");
            return false;
        }
        self.app_state.jump_to_line(line);
        let vp = self.editor_viewport_lines();
        self.app_state.auto_scroll_to_cursor(vp);
        self.submit_lsp_check_for_path(path);
        self.submit_lsp_did_open_for_active_file();
        self.editor_needs_layout = true;
        true
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

    fn should_auto_trigger_lsp_completion_for_char(&self, ch: char) -> bool {
        self.app_state.current_mode() == EditorMode::Insert
            && self.active_lsp_server.is_some()
            && self.lsp_completion_trigger_chars.contains(&ch)
    }

    fn submit_lsp_completion(&mut self) -> bool {
        self.force_flush_lsp_did_change_for_active_file();
        let Some((language_id, uri, line, character)) = self.lsp_cursor_context() else {
            return false;
        };
        let (cursor_line, cursor_col) = self.app_state.cursor_line_col();
        let prefix_info = self
            .app_state
            .completion_prefix_info_at(cursor_line, cursor_col);
        let mut changed = self.app_state.clear_current_overlays();
        changed |= self.app_state.clear_completion();
        self.submit(RequestSpec {
            revision_id: 0,
            topic: RequestTopic::LspRequest,
            payload: WorkerRequestPayload::LspCompletionRequest {
                language_id,
                uri,
                line,
                character,
                cursor_line,
                cursor_col,
                prefix_start_col: prefix_info.start_col,
                prefix: prefix_info.prefix,
            },
        });
        changed
    }

    fn select_next_completion_item(&mut self) -> bool {
        let changed = self.app_state.completion_select_next();
        if changed {
            self.editor_caret_needs_layout = true;
            self.request_redraw();
        }
        changed
    }

    fn select_prev_completion_item(&mut self) -> bool {
        let changed = self.app_state.completion_select_prev();
        if changed {
            self.editor_caret_needs_layout = true;
            self.request_redraw();
        }
        changed
    }

    fn close_completion_popup(&mut self) -> bool {
        let changed = self.app_state.clear_completion();
        if changed {
            self.editor_caret_needs_layout = true;
            self.request_redraw();
        }
        changed
    }

    fn accept_completion_item(&mut self) -> bool {
        let Some(completion) = self.app_state.completion().cloned() else {
            return false;
        };
        let Some(item) = completion
            .filtered_items
            .get(completion.selected_index)
            .map(|entry| entry.item.clone())
        else {
            return false;
        };
        let mut insert_text = item
            .insert_text
            .clone()
            .or(item.text_edit_text.clone())
            .unwrap_or(item.label.clone());
        if insert_text.is_empty() {
            return self.close_completion_popup();
        }

        // Deduplicate trigger char: some LSP servers include the trigger character
        // (e.g. `.getInstance`) in insertText. If the char immediately left of the
        // cursor is that same trigger char, strip it from the front of insertText
        // to avoid doubling (e.g. `message..getInstance()`).
        if let Some(char_left) = self.app_state.char_before_cursor() {
            if self.lsp_completion_trigger_chars.contains(&char_left) {
                if insert_text.starts_with(char_left) {
                    insert_text = insert_text.chars().skip(1).collect();
                }
            }
        }

        if insert_text.is_empty() {
            return self.close_completion_popup();
        }

        let prefix_len = completion.typed_prefix.chars().count();
        let popup_closed = self.app_state.clear_completion();
        let changed = self
            .app_state
            .replace_completion_prefix_at_cursor(prefix_len, &insert_text);
        if changed {
            self.reconcile_highlight_spans_with_pending_edits();
            self.editor_needs_layout = true;
            self.editor_caret_needs_layout = true;
            let viewport_lines = self.editor_viewport_lines();
            self.app_state.auto_scroll_to_cursor(viewport_lines);
            self.submit_lsp_did_open_for_active_file();
        } else if popup_closed {
            self.editor_caret_needs_layout = true;
        }
        if popup_closed || changed {
            self.request_redraw();
        }
        popup_closed || changed
    }

    fn refresh_open_completion_after_text_edit(&mut self) -> bool {
        let Some(completion) = self.app_state.completion().cloned() else {
            return false;
        };

        let (cursor_line, cursor_col) = self.app_state.cursor_line_col();
        if cursor_line != completion.trigger_pos.line || cursor_col < completion.trigger_pos.col {
            return self.close_completion_popup();
        }

        let prefix_info = self
            .app_state
            .completion_prefix_info_at(cursor_line, cursor_col);
        if prefix_info.start_col != completion.trigger_pos.col {
            return self.close_completion_popup();
        }

        let changed = self
            .app_state
            .refresh_completion_with_prefix(&prefix_info.prefix);
        if self
            .app_state
            .completion()
            .is_some_and(|state| state.filtered_items.is_empty())
        {
            return self.close_completion_popup() || changed;
        }

        if changed {
            self.editor_caret_needs_layout = true;
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
            self.clear_highlight_layers();
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
        let target_path = match mode {
            crate::app::command_palette::CommandPaletteMode::ExplorerCreateFile
            | crate::app::command_palette::CommandPaletteMode::ExplorerCreateFolder => {
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
                    _ => unreachable!(),
                };

                if let Err(err) = create_result {
                    eprintln!(
                        "[AppShell] explorer create failed for {}: {err}",
                        target_path.display()
                    );
                    return false;
                }
                target_path
            }
            crate::app::command_palette::CommandPaletteMode::ExplorerRenameFull
            | crate::app::command_palette::CommandPaletteMode::ExplorerRenameBase => {
                let Some(old_path) = self
                    .app_state
                    .pending_explorer_rename_path()
                    .map(PathBuf::from)
                else {
                    return false;
                };
                let Some(parent) = old_path.parent().map(PathBuf::from) else {
                    return false;
                };
                let new_name = self.app_state.command_palette_query_text().trim();
                if new_name.is_empty()
                    || new_name.contains(std::path::MAIN_SEPARATOR)
                    || new_name.contains('/')
                    || new_name.contains('\\')
                {
                    return false;
                }
                let new_path = parent.join(new_name);
                if new_path == old_path || new_path.exists() {
                    return false;
                }
                if let Err(err) = std::fs::rename(&old_path, &new_path) {
                    eprintln!(
                        "[AppShell] explorer rename failed from {} to {}: {err}",
                        old_path.display(),
                        new_path.display()
                    );
                    return false;
                }
                let _ = self.app_state.set_pending_explorer_rename_path(None);
                new_path
            }
            _ => return false,
        };

        if let Err(err) = self.app_state.rescan_workspace() {
            eprintln!(
                "[AppShell] workspace rescan failed after explorer prompt confirm for {}: {err}",
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

        self.switch_workspace_to(folder)
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

        changed | self.switch_workspace_to(path)
    }

    fn confirm_theme_selection(&mut self) -> bool {
        let Some(crate::app::command_palette::CommandPaletteAction::SelectTheme(theme_profile)) =
            self.app_state.command_palette_selected_action()
        else {
            return false;
        };

        let loaded_theme = match ThemeConfig::load(&theme_profile) {
            Ok(theme) => theme,
            Err(err) => {
                eprintln!(
                    "[AppShell] theme load failed for profile '{}': {err}",
                    theme_profile
                );
                self.show_transient_toast(format!("Failed to load theme: {theme_profile}"));
                return true;
            }
        };

        self.base_theme = loaded_theme;
        self.apply_scaled_runtime_config();
        self.leap_state = None;
        if let Some(renderer) = self.renderer.as_mut() {
            renderer.clear_leap_labels();
        }
        self.persistent_state
            .set_theme_profile(Some(theme_profile.clone()));
        self.persistent_state.save();

        self.clear_palette_ime_commit_suppression();
        let mut changed = self.app_state.close_command_palette();
        if let Ok(result) = self.app_state.apply_mode_event(ModeEvent::ExitFocus) {
            changed |= result.changed;
        }
        let focus_changed = self.focus_manager.set(FocusTarget::CenterEditor);
        changed |= focus_changed;
        if focus_changed {
            self.input_handler.clear_pending_prefix();
        }

        self.show_transient_toast(format!("Theme loaded: {theme_profile}"));
        changed || self.transient_toast.is_some()
    }

    fn normalize_leap_target(target: char) -> char {
        if target.is_ascii_alphabetic() {
            target.to_ascii_lowercase()
        } else {
            target
        }
    }

    /// Quét viewport editor hiện tại, tìm tất cả ký tự `target` và sinh labels động.
    fn generate_editor_leap_state(&self, target: char) -> LeapState {
        let viewport_lines = self.editor_viewport_lines().max(1);
        let scroll_line = self.app_state.scroll_line();
        let total_chars = self.app_state.text_len_chars();

        // char_idx range [viewport_start_char, viewport_end_char)
        let viewport_start_char = self.app_state.char_idx_for_line(scroll_line);
        let viewport_end_char = self
            .app_state
            .char_idx_for_line(scroll_line + viewport_lines)
            .min(total_chars);

        if viewport_start_char >= viewport_end_char {
            return LeapState::default();
        }

        let text = self.app_state.text_string();
        let target_lower = Self::normalize_leap_target(target);
        let mut matches = Vec::new();

        let mut char_idx: usize = 0;
        for ch in text.chars() {
            if char_idx >= viewport_start_char && char_idx < viewport_end_char {
                let ch_lower = if ch.is_ascii_alphabetic() {
                    ch.to_ascii_lowercase()
                } else {
                    ch
                };
                if ch_lower == target_lower {
                    matches.push(char_idx);
                }
            }
            char_idx += 1;
            if char_idx >= viewport_end_char {
                break;
            }
        }

        let targets = generate_leap_labels(matches.len())
            .into_iter()
            .zip(matches)
            .map(|(label, char_idx)| LeapTarget { label, char_idx })
            .collect();
        LeapState::new(targets)
    }
}

fn normalize_terminal_paste_text(text: &str) -> String {
    let mut normalized = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();

    while let Some(ch) = chars.next() {
        match ch {
            '\r' => {
                if chars.peek() == Some(&'\n') {
                    let _ = chars.next();
                }
                normalized.push('\r');
            }
            '\n' => normalized.push('\r'),
            _ => normalized.push(ch),
        }
    }

    normalized
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

        let report =
            dispatch_palette_overlay_command(&mut app_state, &mut clipboard, Command::EditorPaste);

        assert!(report.success);
        assert!(report.state_changed);
        assert_eq!(app_state.command_palette_query_text(), "foo bar");
    }

    #[test]
    fn terminal_paste_normalizes_newlines_to_carriage_returns() {
        assert_eq!(
            normalize_terminal_paste_text("echo one\necho two\r\npwd\r"),
            "echo one\recho two\rpwd\r"
        );
    }

    #[test]
    fn move_to_first_line_uses_viewport_layout_path() {
        let mut shell = AppShell::new_for_tests().expect("create app shell");
        let text = (0..80)
            .map(|idx| format!("line {idx}"))
            .collect::<Vec<_>>()
            .join("\n");
        shell.app_state = AppState::from_text(PathBuf::from("gg-layout.txt"), &text);
        let _ = shell.app_state.apply_mode_event(ModeEvent::EnterNormal);
        assert!(shell.app_state.move_to_last_line());
        shell.app_state.set_target_scroll_line(24);
        shell.editor_needs_layout = false;
        shell.editor_caret_needs_layout = true;

        let changed = shell.handle_command(Command::MoveToFirstLine);

        assert!(changed);
        assert_eq!(shell.app_state.cursor_line_col(), (0, 0));
        assert_eq!(shell.app_state.scroll_line(), 0);
        assert!(shell.editor_needs_layout);
        assert!(!shell.editor_caret_needs_layout);
    }

    #[test]
    fn move_to_last_line_uses_viewport_layout_path() {
        let mut shell = AppShell::new_for_tests().expect("create app shell");
        let text = (0..120)
            .map(|idx| format!("line {idx}"))
            .collect::<Vec<_>>()
            .join("\n");
        shell.app_state = AppState::from_text(PathBuf::from("g-layout.txt"), &text);
        let _ = shell.app_state.apply_mode_event(ModeEvent::EnterNormal);
        shell.app_state.move_to_first_line();
        shell.app_state.set_target_scroll_line(0);
        shell.editor_needs_layout = false;
        shell.editor_caret_needs_layout = true;

        let changed = shell.handle_command(Command::MoveToLastLine);

        assert!(changed);
        let (cursor_line, _) = shell.app_state.cursor_line_col();
        assert_eq!(cursor_line, shell.app_state.total_lines().saturating_sub(1));
        assert!(shell.app_state.scroll_line() > 0);
        assert!(shell.editor_needs_layout);
        assert!(!shell.editor_caret_needs_layout);
    }

    #[test]
    fn center_cursor_line_uses_viewport_layout_path() {
        let mut shell = AppShell::new_for_tests().expect("create app shell");
        let text = (0..80)
            .map(|idx| format!("line {idx}"))
            .collect::<Vec<_>>()
            .join("\n");
        shell.app_state = AppState::from_text(PathBuf::from("zz-layout.txt"), &text);
        let _ = shell.app_state.apply_mode_event(ModeEvent::EnterNormal);
        for _ in 0..30 {
            shell.app_state.move_down();
        }
        shell.app_state.set_target_scroll_line(0);
        shell.editor_needs_layout = false;
        shell.editor_caret_needs_layout = true;
        let viewport_lines = shell.editor_viewport_lines();

        let changed = shell.handle_command(Command::CenterCursorLine);

        assert!(changed);
        let (cursor_line, _) = shell.app_state.cursor_line_col();
        assert_eq!(
            shell.app_state.scroll_line(),
            cursor_line.saturating_sub(viewport_lines / 2)
        );
        assert!(shell.editor_needs_layout);
        assert!(!shell.editor_caret_needs_layout);
    }

    #[test]
    fn scroll_half_page_down_uses_viewport_layout_path() {
        let mut shell = AppShell::new_for_tests().expect("create app shell");
        let text = (0..100)
            .map(|idx| format!("line {idx}"))
            .collect::<Vec<_>>()
            .join("\n");
        shell.app_state = AppState::from_text(PathBuf::from("ctrl-d-layout.txt"), &text);
        let _ = shell.app_state.apply_mode_event(ModeEvent::EnterNormal);
        shell.app_state.set_target_scroll_line(0);
        shell.editor_needs_layout = false;
        shell.editor_caret_needs_layout = true;
        let half = (shell.editor_viewport_lines() / 2).max(1);

        let changed = shell.handle_command(Command::ScrollHalfPageDown);

        assert!(changed);
        let (cursor_line, _) = shell.app_state.cursor_line_col();
        assert_eq!(cursor_line, half);
        assert_eq!(shell.app_state.scroll_line(), half);
        assert!(shell.editor_needs_layout);
        assert!(!shell.editor_caret_needs_layout);
    }

    #[test]
    fn explorer_rename_base_selection_keeps_extension() {
        assert_eq!(AppShell::explorer_rename_base_selection("main.rs"), (0, 4));
        assert_eq!(
            AppShell::explorer_rename_base_selection("archive.tar.gz"),
            (0, 11)
        );
        assert_eq!(AppShell::explorer_rename_base_selection("README"), (0, 6));
        assert_eq!(
            AppShell::explorer_rename_base_selection(".gitignore"),
            (0, 10)
        );
    }

    #[test]
    fn toggle_terminal_command_closes_bottom_panel_after_second_press() {
        let mut shell = AppShell::new_for_tests().expect("create app shell");
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
        let mut shell = AppShell::new_for_tests().expect("create app shell");
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
    fn explorer_filter_commands_update_workspace_state() {
        let mut shell = AppShell::new_for_tests().expect("create app shell");
        let root = std::env::temp_dir().join(format!(
            "netherize_explorer_filter_cmd_{}",
            std::process::id()
        ));
        std::fs::create_dir_all(root.join("src")).expect("create dirs");
        std::fs::write(root.join("src/main.rs"), "fn main() {}\n").expect("write file");

        shell
            .app_state
            .attach_workspace(root.clone())
            .expect("attach workspace");
        shell.focus_manager.set(FocusTarget::LeftSidebar);

        assert!(shell.handle_command(Command::ExplorerStartFilter));
        assert!(shell.app_state.workspace_is_inputting_filter());
        assert!(shell.app_state.workspace_append_filter_text("main"));
        assert!(shell.handle_command(Command::ExplorerClearFilter));
        assert!(!shell.app_state.workspace_is_inputting_filter());
        assert!(!shell.app_state.workspace_has_active_filter());

        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn leap_uses_editor_targets_even_when_explorer_is_focused() {
        let mut shell = AppShell::new_for_tests().expect("create app shell");
        shell.app_state = AppState::from_text(PathBuf::from("editor-leap.txt"), "beta\nomega");
        shell.focus_manager.set(FocusTarget::LeftSidebar);
        shell.last_editor_bounds = Some([0.0, 0.0, 640.0, 240.0]);

        assert!(shell.handle_command(Command::LeapActivate('b')));

        let leap_state = shell.leap_state.as_ref().expect("editor leap state");
        assert_eq!(leap_state.typed_prefix, "");
        assert_eq!(leap_state.targets.len(), 1);
        assert_eq!(leap_state.targets[0].label, "a");
        assert_eq!(leap_state.targets[0].char_idx, 0);
    }

    #[test]
    fn leap_generates_multi_char_labels_after_twenty_six_matches() {
        let mut shell = AppShell::new_for_tests().expect("create app shell");
        let text = (0..27).map(|_| "a").collect::<Vec<_>>().join(" ");
        shell.app_state = AppState::from_text(PathBuf::from("editor-leap.txt"), &text);
        shell.last_editor_bounds = Some([0.0, 0.0, 960.0, 240.0]);

        assert!(shell.handle_command(Command::LeapActivate('a')));

        let leap_state = shell.leap_state.as_ref().expect("editor leap state");
        assert_eq!(leap_state.targets.len(), 27);
        assert_eq!(leap_state.targets[0].label, "a");
        assert_eq!(leap_state.targets[12].label, "m");
        assert_eq!(leap_state.targets[13].label, "na");
        assert_eq!(leap_state.targets[25].label, "nm");
        assert_eq!(leap_state.targets[26].label, "nn");
    }

    #[test]
    fn leap_fast_jump_label_resolves_immediately() {
        let mut shell = AppShell::new_for_tests().expect("create app shell");
        let text = (0..40).map(|_| "a").collect::<Vec<_>>().join(" ");
        shell.app_state = AppState::from_text(PathBuf::from("editor-leap.txt"), &text);
        shell.last_editor_bounds = Some([0.0, 0.0, 960.0, 240.0]);

        assert!(shell.handle_command(Command::LeapActivate('a')));
        assert!(shell.handle_command(Command::LeapJump('b')));

        assert!(shell.leap_state.is_none());
        assert_eq!(shell.app_state.cursor_line_col(), (0, 2));
    }

    #[test]
    fn leap_prefix_label_filters_and_waits_for_second_key() {
        let mut shell = AppShell::new_for_tests().expect("create app shell");
        let text = (0..40).map(|_| "a").collect::<Vec<_>>().join(" ");
        shell.app_state = AppState::from_text(PathBuf::from("editor-leap.txt"), &text);
        shell.last_editor_bounds = Some([0.0, 0.0, 960.0, 240.0]);

        assert!(shell.handle_command(Command::LeapActivate('a')));
        assert!(shell.handle_command(Command::LeapJump('n')));

        let leap_state = shell.leap_state.as_ref().expect("filtered leap state");
        assert_eq!(leap_state.typed_prefix, "n");
        assert_eq!(leap_state.targets.len(), 26);
        assert!(
            leap_state
                .targets
                .iter()
                .all(|target| target.label.starts_with("n"))
        );

        assert!(shell.handle_command(Command::LeapJump('b')));
        assert!(shell.leap_state.is_none());
        assert_eq!(shell.app_state.cursor_line_col(), (0, 28));
    }

    #[test]
    fn delete_confirmation_removes_selected_file_after_y() {
        let mut shell = AppShell::new_for_tests().expect("create app shell");
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
        let mut shell = AppShell::new_for_tests().expect("create app shell");
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
        let mut shell = AppShell::new_for_tests().expect("create app shell");
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
        let mut shell = AppShell::new_for_tests().expect("create app shell");
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
        let mut shell = AppShell::new_for_tests().expect("create app shell");
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
        let mut shell = AppShell::new_for_tests().expect("create app shell");

        assert!(shell.handle_command(Command::OpenCommandPalette));
        assert!(shell.app_state.is_command_palette_visible());
        assert!(shell.suppress_next_palette_ime_commit);
        assert!(shell.should_swallow_palette_ime_commit());
        assert!(!shell.suppress_next_palette_ime_commit);
    }

    #[test]
    fn first_real_keypress_after_palette_open_clears_ime_suppression() {
        let mut shell = AppShell::new_for_tests().expect("create app shell");

        assert!(shell.handle_command(Command::OpenCommandPalette));
        assert!(shell.suppress_next_palette_ime_commit);

        shell.note_post_open_keyboard_press();

        assert!(!shell.suppress_next_palette_ime_commit);
        assert!(!shell.should_swallow_palette_ime_commit());
    }

    #[test]
    fn open_file_finder_keeps_center_focus_for_fuzzy_buffer() {
        let mut shell = AppShell::new_for_tests().expect("create app shell");

        assert!(shell.handle_command(Command::OpenFileFinder));

        assert_eq!(shell.focus_manager.current(), FocusTarget::CenterEditor);
        assert!(shell.app_state.active_buffer_is_fuzzy_picker());
        assert_eq!(shell.app_state.current_mode(), EditorMode::Insert);
        assert!(!shell.app_state.is_command_palette_visible());
    }

    #[test]
    fn search_in_files_keeps_center_focus_for_fuzzy_buffer() {
        let mut shell = AppShell::new_for_tests().expect("create app shell");

        assert!(shell.handle_command(Command::SearchInFiles));

        assert_eq!(shell.focus_manager.current(), FocusTarget::CenterEditor);
        assert!(shell.app_state.active_buffer_is_fuzzy_picker());
        assert_eq!(shell.app_state.current_mode(), EditorMode::Insert);
        assert_eq!(
            shell.app_state.command_palette_mode(),
            Some(CommandPaletteMode::LiveGrep)
        );
        assert!(!shell.app_state.is_command_palette_visible());
    }

    #[test]
    fn welcome_recent_projects_can_navigate_without_opening_palette() {
        let mut shell = AppShell::new_for_tests().expect("create app shell");
        let root = std::env::temp_dir().join(format!(
            "netherize_welcome_recent_nav_{}",
            std::process::id()
        ));
        let project_a = root.join("project_a");
        let project_b = root.join("project_b");
        std::fs::create_dir_all(&project_a).expect("create project a");
        std::fs::create_dir_all(&project_b).expect("create project b");
        shell.persistent_state.recent_projects = vec![project_a.clone(), project_b.clone()];

        assert!(shell.app_state.buffers().is_empty());
        assert!(!shell.app_state.is_command_palette_visible());

        assert!(shell.handle_command(Command::OverlaySelectNext));

        assert!(!shell.app_state.is_command_palette_visible());
        assert_eq!(shell.app_state.command_palette_selected_index(), 1);

        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn colon_help_vim_command_opens_help_buffer() {
        let mut shell = AppShell::new_for_tests().expect("create app shell");

        assert!(shell.handle_command(Command::OpenVimCommand));
        assert!(shell.handle_command(Command::FilePickerAppendQuery(":help".to_string())));
        assert!(shell.handle_command(Command::FilePickerConfirmSelection));

        let help = shell
            .app_state
            .active_help_buffer()
            .expect(":help should open the cheat sheet help buffer");
        assert_eq!(help.title, "[Help]");
        assert!(help.lines.iter().any(|line| line == "Netherize Help"));
        assert!(!shell.app_state.is_command_palette_visible());
    }

    #[test]
    fn file_picker_confirm_scrolls_explorer_to_opened_file() {
        let mut shell = AppShell::new_for_tests().expect("create app shell");
        let root =
            std::env::temp_dir().join(format!("netherize_picker_scroll_{}", std::process::id()));
        std::fs::create_dir_all(&root).expect("create workspace");
        for idx in 0..40 {
            std::fs::write(root.join(format!("file_{idx:02}.rs")), "fn main() {}\n")
                .expect("write file");
        }
        let target = root.join("file_35.rs");
        let canonical_target = target.canonicalize().expect("canonical target");

        shell
            .app_state
            .attach_workspace(root.clone())
            .expect("attach workspace");
        shell.last_sidebar_bounds = Some([0.0, 0.0, 240.0, 90.0]);
        shell.sidebar_needs_layout = false;

        assert!(shell.handle_command(Command::OpenFileFinder));
        assert!(shell.handle_command(Command::FilePickerAppendQuery("file_35".to_string())));
        assert!(shell.app_state.set_command_palette_results(
            CommandPaletteMode::FilePicker,
            "file_35",
            vec![crate::app::command_palette::CommandPaletteItem::file_match(
                "file_35.rs".to_string(),
                target.clone(),
            )],
        ));
        assert!(shell.handle_command(Command::FilePickerConfirmSelection));

        assert_eq!(
            shell.app_state.workspace_selected_path(),
            Some(canonical_target.as_path())
        );
        assert!(
            shell
                .app_state
                .workspace_scroll_offset_rows(shell.theme.ui.sidebar_line_height)
                > 0
        );
        assert!(shell.sidebar_needs_layout);

        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn open_theme_selector_opens_overlay_with_theme_profiles() {
        let mut shell = AppShell::new_for_tests().expect("create app shell");

        assert!(shell.handle_command(Command::OpenThemeSelector));

        assert_eq!(shell.focus_manager.current(), FocusTarget::OverlayLayer);
        assert_eq!(
            shell.app_state.command_palette_mode(),
            Some(CommandPaletteMode::ThemeSelector)
        );
        assert!(
            shell
                .app_state
                .command_palette_result_labels()
                .contains(&"default-dark".to_string())
        );
    }

    #[test]
    fn confirming_theme_selector_reloads_theme_and_closes_overlay() {
        let mut shell = AppShell::new_for_tests().expect("create app shell");
        shell.editor_needs_layout = false;
        shell.sidebar_needs_layout = false;
        shell.terminal_needs_layout = false;

        assert!(shell.handle_command(Command::OpenThemeSelector));
        shell
            .app_state
            .set_command_palette_query("default-dark")
            .expect("set theme query");
        assert!(shell.handle_command(Command::FilePickerConfirmSelection));

        assert!(!shell.app_state.is_command_palette_visible());
        assert_eq!(shell.focus_manager.current(), FocusTarget::CenterEditor);
        assert_eq!(shell.base_theme.name, "bearded-arc-zed");
        assert_eq!(shell.theme.name, "bearded-arc-zed");
        assert_eq!(
            shell.persistent_state.configured_theme_profile(),
            Some("default-dark")
        );
        assert!(shell.editor_needs_layout);
        assert!(shell.sidebar_needs_layout);
        assert!(shell.terminal_needs_layout);
    }

    #[test]
    fn lsp_references_open_loading_buffer_immediately() {
        let mut shell = AppShell::new_for_tests().expect("create app shell");
        let root = std::env::temp_dir().join(format!(
            "netherize_references_loading_{}",
            std::process::id()
        ));
        std::fs::create_dir_all(root.join("src")).expect("create workspace");
        let file_path = root.join("src/main.rs");
        std::fs::write(&file_path, "fn demo() {\n    demo();\n}\n").expect("write file");
        shell
            .app_state
            .attach_workspace(root.clone())
            .expect("attach workspace");
        shell
            .app_state
            .open_file(file_path.clone())
            .expect("open file");
        shell.active_lsp_server = Some(ActiveLspServer {
            server_name: "rust-analyzer".to_string(),
            root_path: root.clone(),
        });

        assert!(shell.handle_command(Command::LspReferences));

        let references = shell
            .app_state
            .active_references_buffer()
            .expect("references buffer should open immediately");
        assert!(references.loading);
        assert!(references.items.is_empty());
        assert_eq!(
            references.status_message.as_deref(),
            Some("Loading references...")
        );
        assert!(references.pending_request_id.is_some());
        assert_eq!(shell.focus_manager.current(), FocusTarget::CenterEditor);
        assert!(shell.editor_needs_layout);

        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn opening_fuzzy_buffer_marks_editor_layout_dirty() {
        let mut shell = AppShell::new_for_tests().expect("create app shell");
        shell.editor_needs_layout = false;
        shell.editor_caret_needs_layout = true;

        assert!(shell.handle_command(Command::SearchInFiles));

        assert!(shell.editor_needs_layout);
        assert!(!shell.editor_caret_needs_layout);
    }

    #[test]
    fn fuzzy_picker_query_updates_mark_editor_layout_dirty() {
        let mut shell = AppShell::new_for_tests().expect("create app shell");
        assert!(shell.handle_command(Command::SearchInFiles));
        shell.editor_needs_layout = false;
        shell.editor_caret_needs_layout = true;

        assert!(shell.handle_command(Command::FilePickerAppendQuery("foo".to_string())));

        assert!(shell.editor_needs_layout);
        assert!(!shell.editor_caret_needs_layout);
    }

    #[test]
    fn fuzzy_picker_selection_clears_stale_preview_lines() {
        let mut shell = AppShell::new_for_tests().expect("create app shell");
        shell
            .app_state
            .open_fuzzy_picker_buffer(CommandPaletteMode::FilePicker);
        assert!(shell.app_state.set_command_palette_results(
            CommandPaletteMode::FilePicker,
            "",
            vec![
                crate::app::command_palette::CommandPaletteItem::file_match(
                    "a.rs".to_string(),
                    PathBuf::from("a.rs"),
                ),
                crate::app::command_palette::CommandPaletteItem::file_match(
                    "b.rs".to_string(),
                    PathBuf::from("b.rs"),
                ),
            ],
        ));
        assert!(shell.app_state.set_fuzzy_picker_preview(
            vec![crate::async_runtime::message::FilePreviewLine {
                line_number: 1,
                text: "hello".to_string(),
                is_target: false,
            }],
            String::new(),
            Vec::new(),
        ));

        assert!(shell.handle_command(Command::OverlaySelectNext));

        assert!(
            shell
                .app_state
                .active_fuzzy_picker_buffer()
                .expect("fuzzy buffer")
                .preview_lines
                .is_empty()
        );
    }

    #[test]
    fn fuzzy_picker_open_search_match_confirm_closes_results_buffer() {
        let mut shell = AppShell::new_for_tests().expect("create app shell");
        let root = std::env::temp_dir().join(format!(
            "netherize_fuzzy_confirm_close_{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&root).expect("create workspace");
        let target = root.join("match.rs");
        std::fs::write(&target, "alpha\nbeta\ngamma\n").expect("write target");
        let canonical_target = target.canonicalize().expect("canonical target");

        shell
            .app_state
            .attach_workspace(root.clone())
            .expect("attach workspace");
        shell
            .app_state
            .open_fuzzy_picker_buffer(CommandPaletteMode::LiveGrep);
        assert!(shell.handle_command(Command::FilePickerAppendQuery("beta".to_string())));
        assert!(shell.app_state.set_command_palette_results(
            CommandPaletteMode::LiveGrep,
            "beta",
            vec![
                crate::app::command_palette::CommandPaletteItem::search_match(
                    "match.rs:2".to_string(),
                    Some("beta".to_string()),
                    target.clone(),
                    2,
                    1,
                )
            ],
        ));

        assert!(shell.handle_command(Command::FilePickerConfirmSelection));

        assert!(!shell.app_state.active_buffer_is_fuzzy_picker());
        assert_eq!(
            shell.app_state.active_file(),
            Some(canonical_target.as_path())
        );
        assert_eq!(shell.app_state.cursor_line_col(), (1, 0));

        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn references_selection_clears_stale_preview_lines() {
        let mut shell = AppShell::new_for_tests().expect("create app shell");
        shell
            .app_state
            .open_references_buffer(
                "References (2)",
                Some(PathBuf::from("origin.rs")),
                4,
                vec![
                    crate::app::app_state::ReferencesBufferItem {
                        path: PathBuf::from("a.rs"),
                        relative_path: "a.rs".to_string(),
                        line: 10,
                        column: 2,
                        summary: "Ln 11, Col 3".to_string(),
                    },
                    crate::app::app_state::ReferencesBufferItem {
                        path: PathBuf::from("b.rs"),
                        relative_path: "b.rs".to_string(),
                        line: 20,
                        column: 5,
                        summary: "Ln 21, Col 6".to_string(),
                    },
                ],
            )
            .expect("open references buffer");
        assert!(shell.app_state.set_active_references_preview(
            vec![crate::async_runtime::message::FilePreviewLine {
                line_number: 11,
                text: "hello".to_string(),
                is_target: true,
            }],
            String::new(),
            Vec::new(),
        ));

        assert!(shell.handle_command(Command::ReferencesSelectNext));

        assert!(
            shell
                .app_state
                .active_references_buffer()
                .expect("references buffer")
                .preview_lines
                .is_empty()
        );
    }

    #[test]
    fn references_open_selection_closes_results_buffer() {
        let mut shell = AppShell::new_for_tests().expect("create app shell");
        let root = std::env::temp_dir().join(format!(
            "netherize_refs_confirm_close_{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&root).expect("create workspace");
        let origin = root.join("origin.rs");
        let target = root.join("target.rs");
        std::fs::write(&origin, "origin\n").expect("write origin");
        std::fs::write(&target, "one\ntwo\nthree\n").expect("write target");
        let canonical_target = target.canonicalize().expect("canonical target");

        shell
            .app_state
            .open_references_buffer(
                "References (1)",
                Some(origin.clone()),
                0,
                vec![crate::app::app_state::ReferencesBufferItem {
                    path: target.clone(),
                    relative_path: "target.rs".to_string(),
                    line: 1,
                    column: 0,
                    summary: "Ln 2, Col 1".to_string(),
                }],
            )
            .expect("open references buffer");

        assert!(shell.handle_command(Command::ReferencesOpenSelection));

        assert!(!shell.app_state.active_buffer_is_references());
        assert_eq!(
            shell.app_state.active_file(),
            Some(canonical_target.as_path())
        );
        assert_eq!(shell.app_state.cursor_line_col(), (1, 0));

        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn startup_keeps_a_workspace_attached_for_global_search() {
        let shell = AppShell::new_for_tests().expect("create app shell");

        assert!(shell.app_state.workspace_root_path().is_some());
    }

    #[test]
    fn completion_accept_replaces_typed_prefix_instead_of_inserting_after_it() {
        let mut shell = AppShell::new_for_tests().expect("create app shell");
        shell.app_state =
            AppState::from_text(PathBuf::from("completion_accept.ts"), "MessageManager.ge");
        let completion = crate::app::app_state::CompletionState::from_lsp_items(
            vec![crate::async_runtime::message::LspCompletionItem {
                label: "getInstance".to_string(),
                detail: Some("() -> MessageManager".to_string()),
                insert_text: Some("getInstance()".to_string()),
                text_edit_text: None,
                kind: Some(2),
            }],
            0,
            "MessageManager.ge".chars().count(),
            "MessageManager.".chars().count(),
            "ge".to_string(),
        );
        assert!(
            shell
                .app_state
                .jump_to_line_and_column(0, "MessageManager.ge".chars().count())
        );
        assert!(shell.app_state.set_completion(completion));

        assert!(shell.handle_command(Command::CompletionAccept));
        assert_eq!(
            shell.app_state.text_string(),
            "MessageManager.getInstance()"
        );
        assert!(shell.app_state.completion().is_none());
    }

    #[test]
    fn completion_accept_deduplicates_trigger_char_in_insert_text() {
        // Scenario: user typed "message." and LSP returns insertText = ".getInstance()"
        // (trigger char included). Without dedup the result would be "message..getInstance()".
        let mut shell = AppShell::new_for_tests().expect("create app shell");
        shell.app_state = AppState::from_text(PathBuf::from("dedup_trigger.ts"), "message.");
        shell.lsp_completion_trigger_chars = vec!['.'];
        let cursor_col = "message.".chars().count();
        let completion = crate::app::app_state::CompletionState::from_lsp_items(
            vec![crate::async_runtime::message::LspCompletionItem {
                label: "getInstance".to_string(),
                detail: None,
                insert_text: Some(".getInstance()".to_string()),
                text_edit_text: None,
                kind: Some(2),
            }],
            0,
            cursor_col,
            cursor_col,
            String::new(),
        );
        assert!(shell.app_state.jump_to_line_and_column(0, cursor_col));
        assert!(shell.app_state.set_completion(completion));

        assert!(shell.handle_command(Command::CompletionAccept));
        assert_eq!(shell.app_state.text_string(), "message.getInstance()");
        assert!(shell.app_state.completion().is_none());
    }

    #[test]
    fn welcome_hides_while_command_palette_is_visible() {
        let mut shell = AppShell::new_for_tests().expect("create app shell");

        assert!(shell.should_show_welcome());
        assert!(shell.handle_command(Command::OpenCommandPalette));
        assert!(!shell.should_show_welcome());
    }
}
