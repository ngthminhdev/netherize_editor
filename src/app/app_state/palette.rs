use super::overlays::path_matches;
use super::*;

impl AppState {
    pub fn open_command_palette_mode(&mut self, mode: CommandPaletteMode) -> Result<usize, String> {
        let workspace = self.workspace_model.as_ref();
        if matches!(
            mode,
            CommandPaletteMode::FilePicker
                | CommandPaletteMode::LiveGrep
                | CommandPaletteMode::ExplorerCreateFile
                | CommandPaletteMode::ExplorerCreateFolder
                | CommandPaletteMode::ExplorerRenameFull
                | CommandPaletteMode::ExplorerRenameBase
        ) && workspace.is_none()
        {
            return Err("workspace is not attached".to_string());
        }

        let count = self.command_palette.open(mode, workspace);
        self.sync_file_picker_cache();
        Ok(count)
    }

    /// Push current file+line onto the jump back stack before a jump (e.g. gd).
    /// Clears the forward stack since jumping starts a new branch.
    pub fn push_jump(&mut self) {
        let Some(path) = self.active_file.clone() else {
            return;
        };
        let line = self.cursor_line_col().0;
        self.jump_back_stack.push((path, line));
        self.jump_forward_stack.clear();
    }

    /// Push an explicit file+line onto the jump back stack.
    /// Useful when the current active surface is a non-file buffer.
    pub fn push_jump_entry(&mut self, path: PathBuf, line: usize) {
        self.jump_back_stack.push((path, line));
        self.jump_forward_stack.clear();
    }

    /// Pop from the back stack and return (path, line). Pushes current pos onto forward stack.
    pub fn pop_jump_back(&mut self) -> Option<(PathBuf, usize)> {
        let entry = self.jump_back_stack.pop()?;
        let current_path = self.active_file.clone().unwrap_or_default();
        let current_line = self.cursor_line_col().0;
        self.jump_forward_stack.push((current_path, current_line));
        Some(entry)
    }

    /// Pop from the forward stack and return (path, line). Pushes current pos onto back stack.
    pub fn pop_jump_forward(&mut self) -> Option<(PathBuf, usize)> {
        let entry = self.jump_forward_stack.pop()?;
        let current_path = self.active_file.clone().unwrap_or_default();
        let current_line = self.cursor_line_col().0;
        self.jump_back_stack.push((current_path, current_line));
        Some(entry)
    }

    /// Mở Command Palette ở LspReferences mode với danh sách references tĩnh từ LSP.
    pub fn open_lsp_references_palette(
        &mut self,
        items: Vec<crate::app::command_palette::CommandPaletteItem>,
    ) -> Result<(), String> {
        self.command_palette
            .open_with_items(CommandPaletteMode::LspReferences, items);
        Ok(())
    }

    pub fn open_recent_projects_palette(
        &mut self,
        recent: &[std::path::PathBuf],
    ) -> Result<(), String> {
        use crate::app::command_palette::CommandPaletteItem;
        let items = recent
            .iter()
            .map(|path| CommandPaletteItem::recent_project(path))
            .collect();
        self.command_palette
            .open_with_items(CommandPaletteMode::RecentProjects, items);
        Ok(())
    }

    pub fn sync_welcome_recent_projects(&mut self, recent: &[std::path::PathBuf]) -> bool {
        use crate::app::command_palette::CommandPaletteItem;
        let items: Vec<_> = recent
            .iter()
            .map(|path| CommandPaletteItem::recent_project(path))
            .collect();
        self.command_palette
            .set_hidden_items(CommandPaletteMode::RecentProjects, items)
    }

    pub fn open_theme_selector_palette(
        &mut self,
        themes: &[crate::config::theme_config::ThemeProfileEntry],
    ) -> Result<usize, String> {
        use crate::app::command_palette::CommandPaletteItem;
        let items = themes
            .iter()
            .map(|theme| CommandPaletteItem::theme(&theme.profile, &theme.path))
            .collect();
        Ok(self
            .command_palette
            .open_with_items(CommandPaletteMode::ThemeSelector, items))
    }

    pub fn open_file_history_palette(
        &mut self,
        items: Vec<crate::app::command_palette::CommandPaletteItem>,
    ) -> Result<usize, String> {
        Ok(self
            .command_palette
            .open_with_items(CommandPaletteMode::FileHistory, items))
    }

    pub fn close_command_palette(&mut self) -> bool {
        let _ = self.cancel_file_history_preview();
        let changed = self.command_palette.close();
        self.sync_file_picker_cache();
        changed
    }

    pub fn is_command_palette_visible(&self) -> bool {
        self.command_palette.is_visible
    }

    pub fn command_palette_mode(&self) -> Option<CommandPaletteMode> {
        if let Some(index) = self.active_buffer_index {
            if let Some(BufferEntry {
                content: BufferContent::FuzzyPicker(state),
                ..
            }) = self.buffers.get(index)
            {
                return Some(state.mode);
            }
        }
        if self.command_palette.is_visible {
            Some(self.command_palette.mode)
        } else {
            None
        }
    }

    pub fn command_palette_query_text(&self) -> &str {
        if let Some(index) = self.active_buffer_index {
            if let Some(BufferEntry {
                content: BufferContent::FuzzyPicker(state),
                ..
            }) = self.buffers.get(index)
            {
                return &state.query;
            }
        }
        &self.command_palette.query
    }

    pub fn command_palette_selected_index(&self) -> usize {
        if let Some(index) = self.active_buffer_index {
            if let Some(BufferEntry {
                content: BufferContent::FuzzyPicker(state),
                ..
            }) = self.buffers.get(index)
            {
                return state.selected_index;
            }
        }
        self.command_palette.selected_index
    }

    pub fn command_palette_result_labels(&self) -> Vec<String> {
        if let Some(index) = self.active_buffer_index {
            if let Some(BufferEntry {
                content: BufferContent::FuzzyPicker(state),
                ..
            }) = self.buffers.get(index)
            {
                return state
                    .results
                    .iter()
                    .map(|entry| entry.label.clone())
                    .collect();
            }
        }
        self.command_palette
            .results
            .iter()
            .map(|entry| entry.label.clone())
            .collect()
    }

    pub fn command_palette_append_query(&mut self, text: &str) -> Result<bool, String> {
        if let Some(index) = self.active_buffer_index {
            if let Some(BufferEntry {
                content: BufferContent::FuzzyPicker(state),
                ..
            }) = self.buffers.get_mut(index)
            {
                if state.mode == CommandPaletteMode::FileHistory {
                    return Ok(false);
                }
                let changed = state.append_query(text);
                self.bump_revision();
                return Ok(changed);
            }
        }

        let workspace = self.workspace_model.as_ref();
        if matches!(
            self.command_palette.mode,
            CommandPaletteMode::FilePicker
                | CommandPaletteMode::LiveGrep
                | CommandPaletteMode::ExplorerCreateFile
                | CommandPaletteMode::ExplorerCreateFolder
                | CommandPaletteMode::ExplorerRenameFull
                | CommandPaletteMode::ExplorerRenameBase
        ) && workspace.is_none()
        {
            return Err("workspace is not attached".to_string());
        }

        let changed = self.command_palette.append_query(text, workspace);
        if changed {
            self.sync_file_picker_cache();
        }
        Ok(changed)
    }

    pub fn set_command_palette_query(&mut self, text: &str) -> Result<bool, String> {
        let workspace = self.workspace_model.as_ref();
        if matches!(
            self.command_palette.mode,
            CommandPaletteMode::FilePicker
                | CommandPaletteMode::LiveGrep
                | CommandPaletteMode::ExplorerCreateFile
                | CommandPaletteMode::ExplorerCreateFolder
                | CommandPaletteMode::ExplorerRenameFull
                | CommandPaletteMode::ExplorerRenameBase
        ) && workspace.is_none()
        {
            return Err("workspace is not attached".to_string());
        }

        let changed = self.command_palette.set_query(text, workspace);
        if changed {
            self.sync_file_picker_cache();
        }
        Ok(changed)
    }

    pub fn command_palette_backspace_query(&mut self) -> Result<bool, String> {
        if let Some(index) = self.active_buffer_index {
            if let Some(BufferEntry {
                content: BufferContent::FuzzyPicker(state),
                ..
            }) = self.buffers.get_mut(index)
            {
                if state.mode == CommandPaletteMode::FileHistory {
                    return Ok(false);
                }
                let changed = state.backspace_query();
                self.bump_revision();
                return Ok(changed);
            }
        }

        let workspace = self.workspace_model.as_ref();
        if matches!(
            self.command_palette.mode,
            CommandPaletteMode::FilePicker
                | CommandPaletteMode::LiveGrep
                | CommandPaletteMode::ExplorerCreateFile
                | CommandPaletteMode::ExplorerCreateFolder
                | CommandPaletteMode::ExplorerRenameFull
                | CommandPaletteMode::ExplorerRenameBase
        ) && workspace.is_none()
        {
            return Err("workspace is not attached".to_string());
        }

        let changed = self.command_palette.backspace_query(workspace);
        if changed {
            self.sync_file_picker_cache();
        }
        Ok(changed)
    }

    pub fn command_palette_select_next(&mut self) -> bool {
        if let Some(index) = self.active_buffer_index {
            if let Some(BufferEntry {
                content: BufferContent::FuzzyPicker(state),
                ..
            }) = self.buffers.get_mut(index)
            {
                let changed = state.select_next();
                if changed {
                    self.bump_revision();
                }
                return changed;
            }
        }
        self.command_palette.select_next()
    }

    pub fn command_palette_select_prev(&mut self) -> bool {
        if let Some(index) = self.active_buffer_index {
            if let Some(BufferEntry {
                content: BufferContent::FuzzyPicker(state),
                ..
            }) = self.buffers.get_mut(index)
            {
                let changed = state.select_prev();
                if changed {
                    self.bump_revision();
                }
                return changed;
            }
        }
        self.command_palette.select_prev()
    }

    pub fn command_palette_selected_action(&self) -> Option<CommandPaletteAction> {
        if let Some(index) = self.active_buffer_index {
            if let Some(BufferEntry {
                content: BufferContent::FuzzyPicker(state),
                ..
            }) = self.buffers.get(index)
            {
                return state
                    .results
                    .get(state.selected_index)
                    .map(|item| item.action.clone());
            }
        }
        self.command_palette.selected_action()
    }

    pub fn set_command_palette_results(
        &mut self,
        mode: CommandPaletteMode,
        query: &str,
        items: Vec<CommandPaletteItem>,
    ) -> bool {
        if let Some(index) = self.active_buffer_index {
            if let Some(BufferEntry {
                content: BufferContent::FuzzyPicker(state),
                ..
            }) = self.buffers.get_mut(index)
            {
                if state.mode == mode && state.query == query {
                    state.results = items;
                    state.selected_index = state
                        .selected_index
                        .min(state.results.len().saturating_sub(1));
                    state.preview_lines.clear();
                    state.preview_text.clear();
                    state.preview_spans.clear();
                    self.bump_revision();
                    return true;
                }
            }
        }

        if !self.command_palette.is_visible
            || self.command_palette.mode != mode
            || self.command_palette.query != query
        {
            return false;
        }

        let changed = self.command_palette.replace_results(items);
        if changed {
            self.sync_file_picker_cache();
        }
        changed
    }

    pub fn command_palette_render_model(
        &self,
        theme: &crate::config::theme_config::ThemeConfig,
        overlay_bounds: [f32; 4],
    ) -> Option<CommandPaletteRenderModel> {
        self.command_palette.render(theme, overlay_bounds)
    }

    pub fn set_command_palette_selection_range(&mut self, range: Option<(usize, usize)>) -> bool {
        self.command_palette.set_selection_range(range)
    }

    pub fn pending_explorer_rename_path(&self) -> Option<&Path> {
        self.pending_explorer_rename_path.as_deref()
    }

    pub fn set_pending_explorer_rename_path(&mut self, path: Option<PathBuf>) -> bool {
        if self.pending_explorer_rename_path == path {
            return false;
        }
        self.pending_explorer_rename_path = path;
        true
    }

    pub fn open_file_picker(&mut self) -> Result<usize, String> {
        self.open_command_palette_mode(CommandPaletteMode::FilePicker)
    }

    pub fn close_file_picker(&mut self) -> bool {
        if !self.is_file_picker_open() {
            return false;
        }
        self.close_command_palette()
    }

    pub fn set_fuzzy_picker_preview(
        &mut self,
        lines: Vec<FilePreviewLine>,
        preview_text: String,
        preview_spans: Vec<StyledTextSpan>,
    ) -> bool {
        if let Some(index) = self.active_buffer_index {
            if let Some(BufferEntry {
                content: BufferContent::FuzzyPicker(state),
                ..
            }) = self.buffers.get_mut(index)
            {
                state.preview_lines = lines;
                state.preview_text = preview_text;
                state.preview_spans = preview_spans;
                self.bump_revision();
                return true;
            }
        }
        false
    }

    pub fn set_active_references_preview(
        &mut self,
        lines: Vec<FilePreviewLine>,
        preview_text: String,
        preview_spans: Vec<StyledTextSpan>,
    ) -> bool {
        if let Some(index) = self.active_buffer_index {
            if let Some(BufferEntry {
                content: BufferContent::References(state),
                ..
            }) = self.buffers.get_mut(index)
            {
                state.preview_lines = lines;
                state.preview_text = preview_text;
                state.preview_spans = preview_spans;
                self.bump_revision();
                return true;
            }
        }
        false
    }

    pub fn set_active_diagnostics_preview(
        &mut self,
        lines: Vec<FilePreviewLine>,
        preview_text: String,
        preview_spans: Vec<StyledTextSpan>,
    ) -> bool {
        if let Some(index) = self.active_buffer_index {
            if let Some(BufferEntry {
                content: BufferContent::Diagnostics(state),
                ..
            }) = self.buffers.get_mut(index)
            {
                if state.preview_lines == lines
                    && state.preview_text == preview_text
                    && state.preview_spans == preview_spans
                {
                    return false;
                }
                state.preview_lines = lines;
                state.preview_text = preview_text;
                state.preview_spans = preview_spans;
                self.bump_revision();
                return true;
            }
        }
        false
    }

    pub fn active_fuzzy_picker_buffer(&self) -> Option<&FuzzyState> {
        if let Some(index) = self.active_buffer_index {
            if let Some(buffer) = self.buffers.get(index) {
                if let BufferContent::FuzzyPicker(state) = &buffer.content {
                    return Some(state);
                }
            }
        }
        None
    }

    pub fn diagnostics(&self) -> &HashMap<PathBuf, Vec<LspDiagnostic>> {
        &self.diagnostics
    }

    pub fn diagnostics_for_path(&self, path: &Path) -> Option<&[LspDiagnostic]> {
        let normalized = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
        self.diagnostics.get(&normalized).map(Vec::as_slice)
    }

    pub fn set_file_diagnostics(&mut self, path: PathBuf, diagnostics: Vec<LspDiagnostic>) -> bool {
        let normalized = path.canonicalize().unwrap_or(path);
        if diagnostics.is_empty() {
            return self.diagnostics.remove(&normalized).is_some();
        }

        let changed = self.diagnostics.get(&normalized) != Some(&diagnostics);
        if changed {
            self.diagnostics.insert(normalized, diagnostics);
        }
        changed
    }

    pub fn open_diagnostics_buffer(&mut self, items: Vec<DiagnosticItem>) -> Result<usize, String> {
        if items.is_empty() {
            return Err("cannot open diagnostics buffer without items".to_string());
        }

        self.is_initial_launch_welcome = false;
        self.buffers.push(BufferEntry {
            content: BufferContent::Diagnostics(DiagnosticsState {
                results: items,
                selected_index: 0,
                preview_lines: Vec::new(),
                preview_text: String::new(),
                preview_spans: Vec::new(),
            }),
        });

        let index = self.buffers.len().saturating_sub(1);
        self.reset_text_editor_state();
        self.active_buffer_index = Some(index);
        let _ = self.clear_current_overlays();
        self.bump_revision();
        Ok(index)
    }

    pub fn active_buffer_is_fuzzy_picker(&self) -> bool {
        if let Some(index) = self.active_buffer_index {
            if let Some(buffer) = self.buffers.get(index) {
                return matches!(buffer.content, BufferContent::FuzzyPicker(_));
            }
        }
        false
    }

    pub fn active_buffer_is_settings(&self) -> bool {
        self.active_buffer()
            .is_some_and(|buffer| matches!(buffer.content, BufferContent::SettingsTab(_)))
    }

    pub fn active_settings_buffer(&self) -> Option<&SettingsState> {
        match self.active_buffer().map(|buffer| &buffer.content) {
            Some(BufferContent::SettingsTab(state)) => Some(state),
            _ => None,
        }
    }

    pub fn active_help_buffer(&self) -> Option<&HelpState> {
        match self.active_buffer().map(|buffer| &buffer.content) {
            Some(BufferContent::Help(state)) => Some(state),
            _ => None,
        }
    }

    pub fn active_settings_buffer_mut(&mut self) -> Option<&mut SettingsState> {
        self.active_buffer_index
            .and_then(|idx| self.buffers.get_mut(idx))
            .and_then(|buffer| match &mut buffer.content {
                BufferContent::SettingsTab(state) => Some(state),
                _ => None,
            })
    }

    pub fn settings_is_editing(&self) -> bool {
        self.active_settings_buffer()
            .and_then(|state| state.editing.as_ref())
            .is_some()
    }

    pub fn settings_begin_editing(&mut self) -> bool {
        let Some(state) = self.active_settings_buffer_mut() else {
            return false;
        };
        let changed = state.begin_editing();
        if changed {
            self.bump_revision();
        }
        changed
    }

    pub fn settings_cancel_editing(&mut self) -> bool {
        let Some(state) = self.active_settings_buffer_mut() else {
            return false;
        };
        let changed = state.cancel_editing();
        if changed {
            self.bump_revision();
        }
        changed
    }

    pub fn settings_append_editing_text(&mut self, text: &str) -> bool {
        let Some(state) = self.active_settings_buffer_mut() else {
            return false;
        };
        let changed = state.append_editing_text(text);
        if changed {
            self.bump_revision();
        }
        changed
    }

    pub fn settings_backspace_editing(&mut self) -> bool {
        let Some(state) = self.active_settings_buffer_mut() else {
            return false;
        };
        let changed = state.backspace_editing();
        if changed {
            self.bump_revision();
        }
        changed
    }

    pub fn is_file_picker_open(&self) -> bool {
        self.command_palette.is_visible
            && self.command_palette.mode == CommandPaletteMode::FilePicker
    }

    pub fn file_picker_query_text(&self) -> &str {
        self.command_palette_query_text()
    }

    pub fn file_picker_selected_index(&self) -> usize {
        self.command_palette_selected_index()
    }

    pub fn file_picker_results(&self) -> &[FilePickerEntry] {
        &self.file_picker_results_cache
    }

    pub fn file_picker_append_query(&mut self, text: &str) -> Result<bool, String> {
        self.command_palette_append_query(text)
    }

    pub fn file_picker_backspace_query(&mut self) -> Result<bool, String> {
        self.command_palette_backspace_query()
    }

    pub fn file_picker_select_next(&mut self) -> bool {
        self.command_palette_select_next()
    }

    pub fn file_picker_select_prev(&mut self) -> bool {
        self.command_palette_select_prev()
    }

    pub fn file_picker_selected_path(&self) -> Option<PathBuf> {
        match self.command_palette_selected_action() {
            Some(CommandPaletteAction::OpenFile(path)) => Some(path),
            _ => None,
        }
    }

    pub fn is_terminal_panel_open(&self) -> bool {
        self.terminal_panel_open
    }

    pub fn set_terminal_panel_open(&mut self, open: bool) -> bool {
        if self.terminal_panel_open == open {
            return false;
        }
        self.terminal_panel_open = open;
        true
    }

    pub fn open_terminal_buffer(
        &mut self,
        title: impl Into<String>,
        working_dir: Option<PathBuf>,
    ) -> usize {
        self.is_initial_launch_welcome = false;
        self.buffers.push(BufferEntry {
            content: BufferContent::Terminal(PtyState {
                session_id: None,
                title: title.into(),
                working_dir,
            }),
        });
        let index = self.buffers.len().saturating_sub(1);
        self.active_buffer_index = Some(index);
        self.active_file = None;
        self.selection_anchor_char_idx = None;
        self.visual_line_mode = false;
        self.external_conflict = None;
        self.bump_revision();
        index
    }

    pub fn open_help_buffer(&mut self) -> usize {
        self.is_initial_launch_welcome = false;
        self.buffers.push(BufferEntry {
            content: BufferContent::Help(HelpState::new()),
        });
        let index = self.buffers.len().saturating_sub(1);
        self.reset_text_editor_state();
        self.active_buffer_index = Some(index);
        let _ = self.clear_current_overlays();
        self.bump_revision();
        index
    }

    pub fn open_references_buffer(
        &mut self,
        title: impl Into<String>,
        origin_path: Option<PathBuf>,
        origin_line: usize,
        items: Vec<ReferencesBufferItem>,
    ) -> Result<usize, String> {
        if items.is_empty() {
            return Err("cannot open references buffer without items".to_string());
        }

        self.is_initial_launch_welcome = false;
        self.buffers.push(BufferEntry {
            content: BufferContent::References(ReferencesBufferState {
                title: title.into(),
                origin_path,
                origin_line,
                items,
                selected_index: 0,
                preview_lines: Vec::new(),
                preview_text: String::new(),
                preview_spans: Vec::new(),
                loading: false,
                status_message: None,
                pending_request_id: None,
            }),
        });

        let index = self.buffers.len().saturating_sub(1);
        self.reset_text_editor_state();
        self.active_buffer_index = Some(index);
        let _ = self.clear_current_overlays();
        self.bump_revision();
        Ok(index)
    }

    pub fn open_pending_references_buffer(
        &mut self,
        title: impl Into<String>,
        origin_path: Option<PathBuf>,
        origin_line: usize,
        pending_request_id: u64,
    ) -> usize {
        self.is_initial_launch_welcome = false;
        self.buffers.push(BufferEntry {
            content: BufferContent::References(ReferencesBufferState {
                title: title.into(),
                origin_path,
                origin_line,
                items: Vec::new(),
                selected_index: 0,
                preview_lines: Vec::new(),
                preview_text: String::new(),
                preview_spans: Vec::new(),
                loading: true,
                status_message: Some("Loading references...".to_string()),
                pending_request_id: Some(pending_request_id),
            }),
        });

        let index = self.buffers.len().saturating_sub(1);
        self.reset_text_editor_state();
        self.active_buffer_index = Some(index);
        let _ = self.clear_current_overlays();
        self.bump_revision();
        index
    }

    pub fn finish_pending_references_buffer(
        &mut self,
        pending_request_id: u64,
        title: impl Into<String>,
        items: Vec<ReferencesBufferItem>,
    ) -> bool {
        let Some(buffer) = self.buffers.iter_mut().find(|buffer| {
            matches!(
                &buffer.content,
                BufferContent::References(state)
                    if state.pending_request_id == Some(pending_request_id)
            )
        }) else {
            return false;
        };

        let BufferContent::References(state) = &mut buffer.content else {
            return false;
        };

        state.title = title.into();
        state.items = items;
        state.selected_index = 0;
        state.preview_lines.clear();
        state.preview_text.clear();
        state.preview_spans.clear();
        state.loading = false;
        state.pending_request_id = None;
        state.status_message = if state.items.is_empty() {
            Some("No references found".to_string())
        } else {
            None
        };
        self.bump_revision();
        true
    }

    pub fn fail_pending_references_buffer(
        &mut self,
        pending_request_id: u64,
        message: impl Into<String>,
    ) -> bool {
        let Some(buffer) = self.buffers.iter_mut().find(|buffer| {
            matches!(
                &buffer.content,
                BufferContent::References(state)
                    if state.pending_request_id == Some(pending_request_id)
            )
        }) else {
            return false;
        };

        let BufferContent::References(state) = &mut buffer.content else {
            return false;
        };

        state.title = "References (0)".to_string();
        state.items.clear();
        state.selected_index = 0;
        state.preview_lines.clear();
        state.preview_text.clear();
        state.preview_spans.clear();
        state.loading = false;
        state.pending_request_id = None;
        state.status_message = Some(message.into());
        self.bump_revision();
        true
    }

    pub fn open_fuzzy_picker_buffer(&mut self, mode: CommandPaletteMode) -> usize {
        let mut state = FuzzyState::new(mode);
        if mode == CommandPaletteMode::FileHistory {
            state.source_file_path = self.active_file.clone();
        }
        self.is_initial_launch_welcome = false;
        self.buffers.push(BufferEntry {
            content: BufferContent::FuzzyPicker(state),
        });

        let index = self.buffers.len().saturating_sub(1);
        self.reset_text_editor_state();
        self.active_buffer_index = Some(index);
        let _ = self.clear_current_overlays();
        self.bump_revision();
        index
    }

    pub fn open_settings_buffer(
        &mut self,
        theme_profile: impl Into<String>,
        font_family: impl Into<String>,
        font_size: f32,
        line_height: f32,
        tab_width: u8,
        insert_spaces: bool,
        left_width: i32,
        right_width: i32,
        bottom_height: i32,
        ui_rounding_enabled: bool,
        border_radius_px: f32,
    ) -> usize {
        if let Some(existing_idx) = self
            .buffers
            .iter()
            .position(|buffer| matches!(buffer.content, BufferContent::SettingsTab(_)))
        {
            self.is_initial_launch_welcome = false;
            self.reset_text_editor_state();
            self.active_buffer_index = Some(existing_idx);
            let _ = self.clear_current_overlays();
            self.bump_revision();
            return existing_idx;
        }

        let state = SettingsState::new(
            theme_profile,
            font_family,
            font_size,
            line_height,
            tab_width,
            insert_spaces,
            left_width,
            right_width,
            bottom_height,
            ui_rounding_enabled,
            border_radius_px,
        );
        self.is_initial_launch_welcome = false;
        self.buffers.push(BufferEntry {
            content: BufferContent::SettingsTab(state),
        });

        let index = self.buffers.len().saturating_sub(1);
        self.reset_text_editor_state();
        self.active_buffer_index = Some(index);
        let _ = self.clear_current_overlays();
        self.bump_revision();
        index
    }

    pub fn settings_select_next(&mut self) -> bool {
        let Some(state) = self.active_settings_buffer_mut() else {
            return false;
        };
        let changed = state.select_next();
        if changed {
            self.bump_revision();
        }
        changed
    }

    pub fn settings_select_prev(&mut self) -> bool {
        let Some(state) = self.active_settings_buffer_mut() else {
            return false;
        };
        let changed = state.select_prev();
        if changed {
            self.bump_revision();
        }
        changed
    }

    pub fn bind_terminal_buffer_session(
        &mut self,
        buffer_index: usize,
        session_id: u64,
        working_dir: PathBuf,
    ) -> bool {
        let Some(buffer) = self.buffers.get_mut(buffer_index) else {
            return false;
        };
        let BufferContent::Terminal(state) = &mut buffer.content else {
            return false;
        };

        let changed = state.session_id != Some(session_id)
            || state.working_dir.as_deref() != Some(working_dir.as_path());
        state.session_id = Some(session_id);
        state.working_dir = Some(working_dir);
        changed
    }

    pub fn mark_terminal_buffer_closed(&mut self, session_id: u64) -> bool {
        let Some(index) = self.terminal_buffer_index_for_session(session_id) else {
            return false;
        };
        let Some(buffer) = self.buffers.get_mut(index) else {
            return false;
        };
        let BufferContent::Terminal(state) = &mut buffer.content else {
            return false;
        };
        if state.session_id.is_none() {
            return false;
        }
        state.session_id = None;
        true
    }

    pub fn refresh_file_picker_results_if_open(&mut self) -> Result<bool, String> {
        if !self.is_file_picker_open() {
            return Ok(false);
        }
        if matches!(
            self.command_palette.mode,
            CommandPaletteMode::FilePicker
                | CommandPaletteMode::LiveGrep
                | CommandPaletteMode::LspReferences
        ) {
            return Ok(false);
        }
        let workspace = self
            .workspace_model
            .as_ref()
            .ok_or_else(|| "workspace is not attached".to_string())?;
        let changed = self.command_palette.refresh_if_open(Some(workspace));
        if changed {
            self.sync_file_picker_cache();
        }
        Ok(changed)
    }

    pub fn external_conflict_message(&self) -> Option<&str> {
        self.external_conflict.as_deref()
    }

    pub fn last_external_notice(&self) -> Option<&str> {
        self.external_notice.as_deref()
    }

    pub fn apply_external_file_events(
        &mut self,
        events: &[FileSystemEvent],
    ) -> Result<ExternalChangeReport, String> {
        let mut report = ExternalChangeReport::default();
        if events.is_empty() {
            return Ok(report);
        }

        let requires_workspace_rescan = events.iter().any(|event| {
            matches!(
                event.kind,
                FileSystemChangeKind::Create
                    | FileSystemChangeKind::Delete
                    | FileSystemChangeKind::Rename
            ) || (matches!(event.kind, FileSystemChangeKind::Modify) && !event.path.exists())
        });

        // Chỉ rescan khi tree shape có thể đổi (create/delete/rename).
        // Modify-only thường không đổi cấu trúc workspace, tránh quét cả cây quá nhiều.
        if requires_workspace_rescan && let Some(workspace) = self.workspace_model.as_mut() {
            workspace.rescan()?;
            report.workspace_reloaded = true;
        }

        if report.workspace_reloaded && self.is_file_picker_open() {
            if self.refresh_file_picker_results_if_open()? {
                let note = format!(
                    "file picker refreshed ({} results)",
                    self.file_picker_results().len()
                );
                self.external_notice = Some(note.clone());
                report.notices.push(note);
            }
        }

        let Some(active_path) = self.active_file.clone() else {
            return Ok(report);
        };

        for event in events {
            let touches_active = path_matches(&event.path, &active_path)
                || event
                    .new_path
                    .as_ref()
                    .is_some_and(|new_path| path_matches(new_path, &active_path));
            if !touches_active {
                continue;
            }

            if self.is_dirty() {
                let warning = format!(
                    "external {:?} detected on active file while dirty: {}",
                    event.kind,
                    active_path.display()
                );
                self.external_conflict = Some(warning.clone());
                self.external_notice = Some(warning.clone());
                report.conflict_detected = true;
                report.notices.push(warning);
                continue;
            }

            match event.kind {
                FileSystemChangeKind::Modify | FileSystemChangeKind::Create => {
                    if matches!(event.kind, FileSystemChangeKind::Modify)
                        && self.should_ignore_self_save_event()
                    {
                        continue;
                    }

                    match self.load_buffer_from_file(&active_path) {
                        Ok(()) => {
                            self.active_file = Some(active_path.clone());
                            self.register_open_text_buffer(active_path.clone());
                            self.dirty = false;
                            let note = format!(
                                "auto reloaded active file from disk: {}",
                                active_path.display()
                            );
                            self.external_notice = Some(note.clone());
                            self.external_conflict = None;
                            report.active_file_reloaded = true;
                            report.notices.push(note);
                        }
                        Err(err) => {
                            let note = format!(
                                "auto reload skipped for active file {}: {}",
                                active_path.display(),
                                err
                            );
                            self.external_notice = Some(note.clone());
                            report.notices.push(note);
                        }
                    }
                }
                FileSystemChangeKind::Rename => {
                    if let Some(new_path) = &event.new_path {
                        match self.open_file(new_path.clone()) {
                            Ok(()) => {
                                let note = format!(
                                    "active file renamed externally, reloaded: {} -> {}",
                                    active_path.display(),
                                    new_path.display()
                                );
                                self.external_notice = Some(note.clone());
                                self.external_conflict = None;
                                report.active_file_reloaded = true;
                                report.notices.push(note);
                            }
                            Err(err) => {
                                let note = format!(
                                    "active file rename detected but reload failed {} -> {}: {}",
                                    active_path.display(),
                                    new_path.display(),
                                    err
                                );
                                self.external_notice = Some(note.clone());
                                report.notices.push(note);
                            }
                        }
                    }
                }
                FileSystemChangeKind::Delete => {
                    let note = format!(
                        "active file deleted externally: {} (buffer kept in memory)",
                        active_path.display()
                    );
                    self.external_notice = Some(note.clone());
                    report.notices.push(note);
                }
            }
        }

        Ok(report)
    }
}
