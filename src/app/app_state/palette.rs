use super::overlays::path_matches;
use super::*;

const WELCOME_RECENT_PROJECT_LIMIT: usize = 5;

fn reference_path_counts(
    items: &[ReferencesBufferItem],
) -> std::collections::HashMap<String, usize> {
    let mut path_counts = std::collections::HashMap::new();
    for item in items {
        *path_counts.entry(item.relative_path.clone()).or_insert(0) += 1;
    }
    path_counts
}

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

    /// Mở Python Environment selector palette — envs sẽ được đổ vào qua async result.
    pub fn open_python_env_selector(&mut self) -> bool {
        let workspace = self.workspace_model.as_ref();
        self.command_palette
            .open(CommandPaletteMode::PythonEnvSelector, workspace)
            > 0
    }

    /// Push current file+line+column onto the jump back stack before a jump (e.g. gd).
    /// Clears the forward stack since jumping starts a new branch.
    pub fn push_jump(&mut self) {
        let Some(path) = self.active_file.clone() else {
            return;
        };
        let (line, col) = self.cursor_line_col();
        self.jump_back_stack.push((path, line, col));
        self.jump_forward_stack.clear();
    }

    /// Push an explicit file+line+column onto the jump back stack.
    /// Useful when the current active surface is a non-file buffer.
    pub fn push_jump_entry(&mut self, path: PathBuf, line: usize, col: usize) {
        self.jump_back_stack.push((path, line, col));
        self.jump_forward_stack.clear();
    }

    /// Pop from the back stack and return (path, line, col). Pushes current pos onto forward stack.
    pub fn pop_jump_back(&mut self) -> Option<(PathBuf, usize, usize)> {
        let entry = self.jump_back_stack.pop()?;
        let current_path = self.active_file.clone().unwrap_or_default();
        let (current_line, current_col) = self.cursor_line_col();
        self.jump_forward_stack
            .push((current_path, current_line, current_col));
        Some(entry)
    }

    /// Pop from the forward stack and return (path, line, col). Pushes current pos onto back stack.
    pub fn pop_jump_forward(&mut self) -> Option<(PathBuf, usize, usize)> {
        let entry = self.jump_forward_stack.pop()?;
        let current_path = self.active_file.clone().unwrap_or_default();
        let (current_line, current_col) = self.cursor_line_col();
        self.jump_back_stack
            .push((current_path, current_line, current_col));
        Some(entry)
    }

    /// Mở Command Palette ở CodeAction mode với danh sách actions do LSP trả về.
    pub fn open_code_action_picker(
        &mut self,
        items: Vec<crate::app::command_palette::CommandPaletteItem>,
    ) {
        self.command_palette
            .open_with_items(CommandPaletteMode::CodeAction, items);
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

    pub fn open_recent_projects_palette_with_meta(
        &mut self,
        recent: &[std::path::PathBuf],
        meta: &std::collections::HashMap<std::path::PathBuf, crate::app::persistence::RecentProjectMeta>,
    ) -> Result<(), String> {
        use crate::app::command_palette::CommandPaletteItem;
        let items = recent
            .iter()
            .map(|path| {
                let item_meta = meta.get(path);
                // Always refresh the icon source from the current project markers.
                // Older persisted metadata may contain a stale/generic icon source,
                // which makes the recent-project picker render a file icon while
                // the welcome page correctly infers the project language/framework.
                let inferred_icon_source = crate::app::persistence::AppPersistentState::infer_project_icon_source(path);
                CommandPaletteItem::recent_project_with_meta(
                    path,
                    Some(inferred_icon_source.as_str()),
                    item_meta.and_then(|meta| meta.last_opened_unix_secs),
                )
            })
            .collect();
        self.command_palette
            .open_with_items(CommandPaletteMode::RecentProjects, items);
        Ok(())
    }

    pub fn sync_welcome_recent_projects(&mut self, recent: &[std::path::PathBuf]) -> bool {
        use crate::app::command_palette::CommandPaletteItem;
        let items: Vec<_> = recent
            .iter()
            .take(WELCOME_RECENT_PROJECT_LIMIT)
            .map(|path| {
                let icon_source = crate::app::persistence::AppPersistentState::infer_project_icon_source(path);
                CommandPaletteItem::recent_project_with_meta(path, Some(icon_source.as_str()), None)
            })
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

    pub fn open_document_symbols_palette_loading(&mut self) -> Result<usize, String> {
        let count = self
            .command_palette
            .open_with_items(CommandPaletteMode::DocumentSymbols, Vec::new());
        let _ = self.command_palette.set_loading(true);
        Ok(count)
    }

    pub fn set_document_symbol_picker_results(
        &mut self,
        symbols: Vec<crate::async_runtime::message::LspDocumentSymbol>,
    ) -> bool {
        let items = symbols
            .iter()
            .map(crate::app::command_palette::CommandPaletteItem::document_symbol)
            .collect();
        self.command_palette.replace_static_results(items)
    }

    pub fn finish_document_symbol_picker_loading(&mut self) -> bool {
        self.command_palette.set_loading(false)
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
        let mut model = self.command_palette.render(theme, overlay_bounds)?;
        model.search_case_sensitive = self.search_case_sensitive;
        Some(model)
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

    pub fn live_grep_case_sensitive(&self) -> bool {
        self.live_grep_case_sensitive
    }

    pub fn toggle_live_grep_case_sensitive(&mut self) -> bool {
        self.live_grep_case_sensitive = !self.live_grep_case_sensitive;
        if let Some(index) = self.active_buffer_index
            && let Some(BufferEntry {
                content: BufferContent::FuzzyPicker(state),
                ..
            }) = self.buffers.get_mut(index)
            && state.mode == CommandPaletteMode::LiveGrep
        {
            state.live_grep_case_sensitive = self.live_grep_case_sensitive;
        }
        self.bump_revision();
        true
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

    /// Currently displayed LSP progress entry, if any. The status bar uses this
    /// to show the user that the language server is busy (e.g. rust-analyzer
    /// indexing) so they avoid spamming `gd`/`K` and getting stuck requests.
    pub fn lsp_progress(&self) -> Option<&LspProgressEntry> {
        let key = self.lsp_progress_active_key.as_ref()?;
        self.lsp_progress.get(key)
    }

    /// Update or remove the progress entry for `(server, token)`. Returns
    /// `true` if anything changed (so the caller can mark the frame dirty).
    pub fn update_lsp_progress(
        &mut self,
        server: &str,
        token: &str,
        kind: LspProgressKind,
        title: Option<String>,
        message: Option<String>,
        percentage: Option<u32>,
    ) -> bool {
        let key = (server.to_string(), token.to_string());
        match kind {
            LspProgressKind::End => {
                let removed = self.lsp_progress.remove(&key).is_some();
                if self.lsp_progress_active_key.as_ref() == Some(&key) {
                    self.lsp_progress_active_key = self.lsp_progress.keys().next().cloned();
                }
                removed
            }
            LspProgressKind::Begin | LspProgressKind::Report => {
                let entry =
                    self.lsp_progress
                        .entry(key.clone())
                        .or_insert_with(|| LspProgressEntry {
                            server: server.to_string(),
                            token: token.to_string(),
                            ..LspProgressEntry::default()
                        });
                let prev = entry.clone();
                if matches!(kind, LspProgressKind::Begin) {
                    entry.title = title.clone();
                }
                if title.is_some() && entry.title.is_none() {
                    entry.title = title;
                }
                entry.message = message;
                entry.percentage = percentage;
                let changed = *entry != prev;
                self.lsp_progress_active_key = Some(key);
                changed
            }
        }
    }

    /// Drop every progress entry for the given server (e.g. when the LSP
    /// session is shut down). Returns `true` if anything was removed.
    pub fn clear_lsp_progress_for_server(&mut self, server: &str) -> bool {
        let before = self.lsp_progress.len();
        self.lsp_progress.retain(|(s, _), _| s != server);
        let changed = self.lsp_progress.len() != before;
        if let Some(key) = self.lsp_progress_active_key.as_ref() {
            if key.0 == server {
                self.lsp_progress_active_key = self.lsp_progress.keys().next().cloned();
            }
        }
        changed
    }

    pub fn open_diagnostics_buffer(&mut self, items: Vec<DiagnosticItem>) -> Result<usize, String> {
        if items.is_empty() {
            return Err("cannot open diagnostics buffer without items".to_string());
        }

        // Save current text buffer before switching to diagnostics buffer
        self.save_current_text_buffer_history();

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

    pub fn active_buffer_is_help(&self) -> bool {
        self.active_buffer()
            .is_some_and(|buffer| matches!(buffer.content, BufferContent::Help(_)))
    }

    pub fn active_buffer_is_extensions_manager(&self) -> bool {
        self.active_buffer()
            .is_some_and(|buffer| matches!(buffer.content, BufferContent::ExtensionsManager(_)))
    }

    pub fn active_extensions_manager_buffer(&self) -> Option<&ExtensionsManagerState> {
        match self.active_buffer().map(|buffer| &buffer.content) {
            Some(BufferContent::ExtensionsManager(state)) => Some(state),
            _ => None,
        }
    }

    pub fn active_extensions_manager_buffer_mut(&mut self) -> Option<&mut ExtensionsManagerState> {
        self.active_buffer_index
            .and_then(|idx| self.buffers.get_mut(idx))
            .and_then(|buffer| match &mut buffer.content {
                BufferContent::ExtensionsManager(state) => Some(state),
                _ => None,
            })
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
        // Save current text buffer before switching to terminal buffer
        self.save_current_text_buffer_history();

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

    pub fn help_scroll_down(&mut self, amount: f32) {
        if let Some(buf) = self.active_help_buffer_mut() {
            buf.scroll_y += amount;
        }
    }

    pub fn help_scroll_up(&mut self, amount: f32) {
        if let Some(buf) = self.active_help_buffer_mut() {
            buf.scroll_y = (buf.scroll_y - amount).max(0.0);
        }
    }

    fn active_help_buffer_mut(&mut self) -> Option<&mut HelpState> {
        self.active_buffer_index
            .and_then(|idx| self.buffers.get_mut(idx))
            .and_then(|buffer| match &mut buffer.content {
                BufferContent::Help(state) => Some(state),
                _ => None,
            })
    }

    pub fn open_help_buffer(&mut self) -> usize {
        // Save current text buffer before switching to help buffer
        self.save_current_text_buffer_history();

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

    pub fn open_extensions_manager_buffer(&mut self) -> usize {
        self.save_current_text_buffer_history();

        if let Some(existing_idx) = self
            .buffers
            .iter()
            .position(|buffer| matches!(buffer.content, BufferContent::ExtensionsManager(_)))
        {
            self.is_initial_launch_welcome = false;
            self.reset_text_editor_state();
            self.active_buffer_index = Some(existing_idx);
            let _ = self.clear_current_overlays();
            self.bump_revision();
            return existing_idx;
        }

        self.is_initial_launch_welcome = false;
        self.buffers.push(BufferEntry {
            content: BufferContent::ExtensionsManager(ExtensionsManagerState::new()),
        });
        let index = self.buffers.len().saturating_sub(1);
        self.reset_text_editor_state();
        self.active_buffer_index = Some(index);
        let _ = self.clear_current_overlays();
        self.bump_revision();
        index
    }

    pub fn sync_markdown_preview_buffer(&mut self, preview: MarkdownPreviewState) -> bool {
        let Some(existing_idx) = self
            .buffers
            .iter()
            .position(|buffer| matches!(buffer.content, BufferContent::MarkdownPreview(_)))
        else {
            return false;
        };

        if let Some(BufferEntry {
            content: BufferContent::MarkdownPreview(state),
            ..
        }) = self.buffers.get_mut(existing_idx)
        {
            *state = preview;
            return true;
        }

        false
    }

    pub fn open_markdown_preview_buffer(&mut self, preview: MarkdownPreviewState) -> usize {
        if let Some(existing_idx) = self
            .buffers
            .iter()
            .position(|buffer| matches!(buffer.content, BufferContent::MarkdownPreview(_)))
        {
            let _ = self.sync_markdown_preview_buffer(preview);
            self.reset_text_editor_state();
            self.active_buffer_index = Some(existing_idx);
            let _ = self.clear_current_overlays();
            self.bump_revision();
            return existing_idx;
        }

        self.save_current_text_buffer_history();
        self.is_initial_launch_welcome = false;
        self.buffers.push(BufferEntry {
            content: BufferContent::MarkdownPreview(preview),
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

        // Save current text buffer before switching to references buffer
        self.save_current_text_buffer_history();

        let path_counts = reference_path_counts(&items);

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
                path_counts,
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
        // Save current text buffer before switching to pending references buffer
        self.save_current_text_buffer_history();

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
                path_counts: std::collections::HashMap::new(),
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
        state.path_counts = reference_path_counts(&items);
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
        state.path_counts.clear();
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
        self.save_current_text_buffer_history();
        let mut state = FuzzyState::new(mode);
        if mode == CommandPaletteMode::FileHistory {
            state.source_file_path = self.active_file.clone();
        }
        if mode == CommandPaletteMode::LiveGrep {
            state.live_grep_case_sensitive = self.live_grep_case_sensitive;
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
        enable_outline: bool,
        inline_suggestion_enabled: bool,
    ) -> usize {
        // Save current text buffer before switching to settings buffer
        self.save_current_text_buffer_history();

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
            enable_outline,
            inline_suggestion_enabled,
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

    pub fn extensions_select_next(&mut self) -> bool {
        let Some(state) = self.active_extensions_manager_buffer_mut() else {
            return false;
        };
        let changed = state.select_next();
        if changed {
            self.bump_revision();
        }
        changed
    }

    pub fn extensions_select_prev(&mut self) -> bool {
        let Some(state) = self.active_extensions_manager_buffer_mut() else {
            return false;
        };
        let changed = state.select_prev();
        if changed {
            self.bump_revision();
        }
        changed
    }

    pub fn extensions_set_filter_focused(&mut self, focused: bool) -> bool {
        let Some(state) = self.active_extensions_manager_buffer_mut() else {
            return false;
        };
        if state.filter_focused == focused {
            return false;
        }
        state.filter_focused = focused;
        self.bump_revision();
        true
    }

    pub fn extensions_append_filter(&mut self, text: &str) -> bool {
        let Some(state) = self.active_extensions_manager_buffer_mut() else {
            return false;
        };
        let changed = state.append_filter(text);
        if changed {
            self.bump_revision();
        }
        changed
    }

    pub fn extensions_backspace_filter(&mut self) -> bool {
        let Some(state) = self.active_extensions_manager_buffer_mut() else {
            return false;
        };
        let changed = state.backspace_filter();
        if changed {
            self.bump_revision();
        }
        changed
    }

    pub fn extensions_toggle_expanded_selected(&mut self) -> bool {
        let Some(state) = self.active_extensions_manager_buffer_mut() else {
            return false;
        };
        let changed = state.toggle_expanded_selected();
        if changed {
            self.bump_revision();
        }
        changed
    }

    pub fn extensions_switch_tab_next(&mut self) -> bool {
        let Some(state) = self.active_extensions_manager_buffer_mut() else {
            return false;
        };
        let changed = state.switch_tab_next();
        if changed {
            self.bump_revision();
        }
        changed
    }

    pub fn extensions_switch_tab_prev(&mut self) -> bool {
        let Some(state) = self.active_extensions_manager_buffer_mut() else {
            return false;
        };
        let changed = state.switch_tab_prev();
        if changed {
            self.bump_revision();
        }
        changed
    }

    pub fn extensions_toggle_install_selected(&mut self, installed: bool) -> bool {
        let Some(state) = self.active_extensions_manager_buffer_mut() else {
            return false;
        };
        let Some(idx) = state.selected_item_index() else {
            return false;
        };
        let Some(item) = state.items.get_mut(idx) else {
            return false;
        };
        if item.installed == installed {
            return false;
        }
        item.installed = installed;
        self.bump_revision();
        true
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

        if self.refresh_text_buffer_missing_on_disk_states() {
            report.workspace_reloaded = true;
        }

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
            eprintln!("[FileWatch] No active file, skipping event processing");
            return Ok(report);
        };

        eprintln!("[FileWatch] Active file: {:?}", active_path);

        for event in events {
            let touches_active = path_matches(&event.path, &active_path)
                || event
                    .new_path
                    .as_ref()
                    .is_some_and(|new_path| path_matches(new_path, &active_path));

            eprintln!("[FileWatch] Event {:?} on {:?}, touches_active: {}",
                event.kind, event.path, touches_active);

            if !touches_active {
                continue;
            }

            if self.is_dirty() {
                eprintln!("[FileWatch] File is dirty, showing conflict warning");
                let warning = format!(
                    "external {:?} detected on active file while dirty: {}",
                    event.kind,
                    active_path.display()
                );
                self.external_conflict = Some(warning.clone());
                self.external_notice = Some(warning.clone());
                report.conflict_detected = true;
                report.conflict_path = Some(active_path.clone());
                report.notices.push(warning);
                continue;
            }

            match event.kind {
                FileSystemChangeKind::Modify | FileSystemChangeKind::Create => {
                    if matches!(event.kind, FileSystemChangeKind::Modify)
                        && self.should_ignore_self_save_event()
                    {
                        eprintln!("[FileWatch] Ignoring self-save event (within 2s window)");
                        continue;
                    }

                    eprintln!("[FileWatch] Reloading file from disk: {:?}", active_path);
                    match self.load_buffer_from_file(&active_path) {
                        Ok(()) => {
                            self.active_file = Some(active_path.clone());
                            // Update the buffer entry's in_memory_text to match the reloaded content.
                            // Don't call register_open_text_buffer here - it would save the old
                            // working copy over the newly loaded text.
                            let reloaded_text = self.text.clone();
                            if let Some(buffer) = self.active_text_buffer_mut() {
                                buffer.in_memory_text = Some(reloaded_text);
                                buffer.missing_on_disk = false;
                                buffer.dirty = false;
                            }
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
                            if let Some(buffer) = self.active_text_buffer_mut() {
                                buffer.missing_on_disk = !active_path.exists();
                            }
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
                    if let Some(buffer) = self.active_text_buffer_mut() {
                        buffer.missing_on_disk = true;
                    }
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
