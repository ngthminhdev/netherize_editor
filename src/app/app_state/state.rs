use super::overlays::{
    build_completion_display_items, collect_search_highlights, is_completion_identifier_char,
};
use super::*;

fn inline_suggestion_accept_prefix_byte_len(suggestion: &str) -> usize {
    let mut saw_leading_whitespace = false;
    let mut token_kind: Option<InlineSuggestionTokenKind> = None;
    let mut last_end = 0;

    for (idx, ch) in suggestion.char_indices() {
        if token_kind.is_none() && ch.is_whitespace() {
            saw_leading_whitespace = true;
            last_end = idx + ch.len_utf8();
            if ch == '\n' {
                return last_end;
            }
            continue;
        }

        let kind = InlineSuggestionTokenKind::for_char(ch);
        match token_kind {
            None => {
                token_kind = Some(kind);
                last_end = idx + ch.len_utf8();
                if kind == InlineSuggestionTokenKind::Punctuation {
                    return last_end;
                }
            }
            Some(current) if current == kind && kind != InlineSuggestionTokenKind::Punctuation => {
                last_end = idx + ch.len_utf8();
            }
            Some(_) => {
                return if saw_leading_whitespace {
                    idx
                } else {
                    last_end
                };
            }
        }
    }

    last_end
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum InlineSuggestionTokenKind {
    Word,
    Number,
    Punctuation,
}

impl InlineSuggestionTokenKind {
    fn for_char(ch: char) -> Self {
        if ch == '_' || ch.is_alphabetic() {
            Self::Word
        } else if ch.is_ascii_digit() {
            Self::Number
        } else {
            Self::Punctuation
        }
    }
}

impl AppState {
    pub fn file_history_picker_items(
        &self,
    ) -> Vec<crate::app::command_palette::CommandPaletteItem> {
        // When a FileHistory FuzzyPicker is the active buffer, its source_file_path
        // tells us which EditorBuffer's history to display.
        let history: &EditHistory = if let Some(active_idx) = self.active_buffer_index {
            if let Some(slot) = self.buffers.get(active_idx) {
                if let BufferContent::FuzzyPicker(ref state) = slot.content {
                    if state.mode == CommandPaletteMode::FileHistory {
                        if let Some(ref src_path) = state.source_file_path {
                            // The text buffer's history is stored inside its EditorBuffer.
                            let found = self.buffers.iter().find_map(|b| match &b.content {
                                BufferContent::Text(eb) if &eb.path == src_path => {
                                    Some(&eb.history)
                                }
                                _ => None,
                            });
                            found.unwrap_or(&self.history)
                        } else {
                            &self.history
                        }
                    } else {
                        &self.history
                    }
                } else {
                    &self.history
                }
            } else {
                &self.history
            }
        } else {
            &self.history
        };

        if history.undo_stack.is_empty() {
            return Vec::new();
        }
        history
            .undo_stack
            .iter()
            .enumerate()
            .rev()
            .map(|(index, transaction)| {
                let (label, tone) = file_history_transaction_label(transaction);
                crate::app::command_palette::CommandPaletteItem::file_history_entry(
                    label,
                    Some(file_history_transaction_secondary(index, transaction)),
                    index,
                    tone,
                )
            })
            .collect()
    }

    pub fn build_file_history_diff_preview(&self) -> Option<(Vec<FilePreviewLine>, String)> {
        let Some(session) = self.file_history_preview.as_ref() else {
            return None;
        };
        let Some(preview_index) = session.preview_index else {
            return None;
        };
        let transaction = session.baseline_history.undo_stack.get(preview_index)?;
        let lines = build_transaction_diff_preview_lines(transaction);
        let preview_text = lines
            .iter()
            .map(|line| line.text.clone())
            .collect::<Vec<_>>()
            .join("\n");
        Some((lines, preview_text))
    }

    pub fn begin_file_history_preview_session(&mut self) -> bool {
        let Some(_file_path) = self.active_file.clone() else {
            return false;
        };
        if self.file_history_preview.is_some() {
            return false;
        }
        self.file_history_preview = Some(FileHistoryPreviewSession {
            baseline_view: self.snapshot_editor_view(),
            baseline_history: self.history.clone(),
            preview_index: None,
            preview_view: None,
        });
        true
    }

    pub fn preview_file_history_index(&mut self, transaction_index: usize) -> bool {
        let Some((baseline_view, baseline_history)) =
            self.file_history_preview.as_ref().map(|session| {
                (
                    session.baseline_view.clone(),
                    session.baseline_history.clone(),
                )
            })
        else {
            return false;
        };
        let undo_len = baseline_history.undo_stack.len();
        if transaction_index >= undo_len {
            return false;
        }

        let mut preview_view = baseline_view.clone();
        for transaction in baseline_history
            .undo_stack
            .iter()
            .skip(transaction_index + 1)
            .rev()
        {
            if !undo_edit_on_rope(&mut preview_view.text, &transaction.edit) {
                return false;
            }
        }
        if let Some(transaction) = baseline_history.undo_stack.get(transaction_index) {
            preview_view.cursor = transaction.after_cursor;
            preview_view.selection_anchor_char_idx = None;
            preview_view.visual_line_mode = false;
        }

        self.restore_editor_view(&preview_view);
        self.current_transaction = None;
        if let Some(session) = self.file_history_preview.as_mut() {
            session.preview_index = Some(transaction_index);
            session.preview_view = Some(preview_view);
        }
        self.bump_revision();
        true
    }

    pub fn accept_file_history_preview(&mut self) -> bool {
        let Some(session) = self.file_history_preview.take() else {
            return false;
        };
        let Some(preview_index) = session.preview_index else {
            self.history = session.baseline_history;
            return false;
        };
        let Some(preview_view) = session.preview_view else {
            self.history = session.baseline_history;
            return false;
        };

        self.restore_editor_view(&preview_view);
        self.dirty = session.baseline_view.dirty
            || preview_index + 1 < session.baseline_history.undo_stack.len();
        let keep_len = preview_index + 1;
        self.history = session.baseline_history;
        self.history.undo_stack.truncate(keep_len);
        self.history.redo_stack.clear();
        self.current_transaction = None;
        self.bump_revision();
        true
    }

    pub fn cancel_file_history_preview(&mut self) -> bool {
        let Some(session) = self.file_history_preview.take() else {
            return false;
        };
        self.restore_editor_view(&session.baseline_view);
        self.history = session.baseline_history;
        self.current_transaction = None;
        self.bump_revision();
        true
    }

    pub fn undo(&mut self) -> bool {
        let Some(transaction) = self.history.undo_stack.pop() else {
            return false;
        };

        self.current_transaction = None;
        if !transaction.edit.is_empty() && !self.undo_edit_transaction(&transaction.edit) {
            self.history.undo_stack.push(transaction);
            return false;
        }

        self.restore_cursor_state(transaction.before_cursor);
        self.selection_anchor_char_idx = None;
        self.visual_line_mode = false;
        self.dirty = true;
        self.history.redo_stack.push(transaction);
        self.bump_revision();
        true
    }

    pub fn redo(&mut self) -> bool {
        let Some(transaction) = self.history.redo_stack.pop() else {
            return false;
        };

        self.current_transaction = None;
        if !transaction.edit.is_empty() && !self.redo_edit_transaction(&transaction.edit) {
            self.history.redo_stack.push(transaction);
            return false;
        }

        self.restore_cursor_state(transaction.after_cursor);
        self.selection_anchor_char_idx = None;
        self.visual_line_mode = false;
        self.dirty = true;
        self.history.undo_stack.push(transaction);
        self.bump_revision();
        true
    }

    fn undo_edit_transaction(&mut self, edit: &EditTransaction) -> bool {
        if edit.is_empty() {
            return false;
        }
        if !range_matches_rope(
            &self.text,
            edit.start_char_idx,
            edit.inserted_len_chars(),
            &edit.inserted_text,
        ) {
            return false;
        }

        if !edit.inserted_text.is_empty() {
            self.record_delete_highlight_edit(edit.start_char_idx, edit.inserted_len_chars());
            if self
                .apply_delete_raw(edit.start_char_idx, edit.inserted_len_chars())
                .is_none()
            {
                return false;
            }
        }
        if !edit.deleted_text.is_empty() {
            self.record_insert_highlight_edit(edit.start_char_idx, &edit.deleted_text);
            self.apply_insert_raw(edit.start_char_idx, &edit.deleted_text);
        }
        true
    }

    fn redo_edit_transaction(&mut self, edit: &EditTransaction) -> bool {
        if edit.is_empty() {
            return false;
        }
        if !range_matches_rope(
            &self.text,
            edit.start_char_idx,
            edit.deleted_len_chars(),
            &edit.deleted_text,
        ) {
            return false;
        }

        if !edit.deleted_text.is_empty() {
            self.record_delete_highlight_edit(edit.start_char_idx, edit.deleted_len_chars());
            if self
                .apply_delete_raw(edit.start_char_idx, edit.deleted_len_chars())
                .is_none()
            {
                return false;
            }
        }
        if !edit.inserted_text.is_empty() {
            self.record_insert_highlight_edit(edit.start_char_idx, &edit.inserted_text);
            self.apply_insert_raw(edit.start_char_idx, &edit.inserted_text);
        }
        true
    }

    pub fn cursor_line_col(&self) -> (usize, usize) {
        let line_idx = self.text.char_to_line(self.cursor_char_idx);
        let line_start = self.text.line_to_char(line_idx);
        let col_idx = self.cursor_char_idx - line_start;
        (line_idx, col_idx)
    }

    pub fn cursor_char_idx(&self) -> usize {
        self.cursor_char_idx
    }

    pub fn cursor_byte_idx(&self) -> usize {
        self.text.char_to_byte(self.cursor_char_idx)
    }

    /// Byte offset tương đối trong dòng hiện tại (không phải toàn buffer).
    /// Hữu ích khi map với glyph.start/end của cosmic-text theo từng line run.
    pub fn cursor_byte_in_line(&self) -> usize {
        let line_idx = self.text.char_to_line(self.cursor_char_idx);
        let line_start_byte = self.text.line_to_byte(line_idx);
        self.cursor_byte_idx().saturating_sub(line_start_byte)
    }

    /// (line_idx, byte_in_line) for an arbitrary char position — used by the
    /// renderer to compute virtual-cursor caret positions.
    pub fn char_idx_to_line_and_byte_in_line(&self, char_idx: usize) -> (usize, usize) {
        let char_idx = char_idx.min(self.text.len_chars());
        let line_idx = self.text.char_to_line(char_idx);
        let line_start_byte = self.text.line_to_byte(line_idx);
        let byte_idx = self.text.char_to_byte(char_idx);
        (line_idx, byte_idx.saturating_sub(line_start_byte))
    }

    pub fn completion_prefix_info_at(
        &self,
        line_idx: usize,
        cursor_col: usize,
    ) -> CompletionPrefixInfo {
        if self.text.len_lines() == 0 {
            return CompletionPrefixInfo {
                start_col: 0,
                prefix: String::new(),
            };
        }

        let clamped_line = line_idx.min(self.text.len_lines().saturating_sub(1));
        let line_text = self.text.line(clamped_line).to_string();
        let line_content = line_text.strip_suffix('\n').unwrap_or(&line_text);
        let chars: Vec<char> = line_content.chars().collect();
        let cursor_col = cursor_col.min(chars.len());
        let mut start_col = cursor_col;

        while start_col > 0 && is_completion_identifier_char(chars[start_col - 1]) {
            start_col -= 1;
        }

        CompletionPrefixInfo {
            start_col,
            prefix: chars[start_col..cursor_col].iter().collect(),
        }
    }

    pub fn last_search_query(&self) -> &str {
        &self.last_search_query
    }

    pub fn search_highlights(&self) -> &[(usize, usize)] {
        &self.search_highlights
    }

    pub fn semantic_symbol_highlights(&self) -> &[(usize, usize)] {
        &self.semantic_symbol_highlights
    }

    pub fn set_semantic_symbol_highlights(&mut self, highlights: Vec<(usize, usize)>) -> bool {
        if self.semantic_symbol_highlights == highlights {
            return false;
        }
        self.semantic_symbol_highlights = highlights;
        true
    }

    pub fn clear_semantic_symbol_highlights(&mut self) -> bool {
        if self.semantic_symbol_highlights.is_empty() {
            return false;
        }
        self.semantic_symbol_highlights.clear();
        true
    }

    pub fn fallback_symbol_highlights_under_cursor(&self) -> Vec<(usize, usize)> {
        let Some(word) = self.word_under_cursor() else {
            return Vec::new();
        };
        let text = self.text.to_string();
        collect_search_highlights(&text, &word, true, true)
    }

    pub fn active_search_match_position(&self) -> Option<(usize, usize)> {
        let total = self.search_highlights.len();
        if self.last_search_query.is_empty() || total == 0 {
            return None;
        }

        let cursor_byte = self.cursor_byte_idx();
        let current_idx = self
            .search_highlights
            .iter()
            .position(|(start, end)| *start <= cursor_byte && cursor_byte < *end)
            .or_else(|| {
                self.search_highlights
                    .iter()
                    .position(|(start, _)| *start > cursor_byte)
            })
            .unwrap_or(0);

        Some((current_idx + 1, total))
    }

    pub fn remember_clipboard_text(&mut self, text: String, kind: ClipboardRecordKind) {
        if text.is_empty() {
            return;
        }
        self.clipboard_record = Some(ClipboardRecord { text, kind });
    }

    pub fn clipboard_record_kind_for_text(&self, text: &str) -> Option<ClipboardRecordKind> {
        self.clipboard_record
            .as_ref()
            .filter(|record| record.text == text)
            .map(|record| record.kind)
    }

    pub fn set_in_file_search_query(&mut self, query: &str) -> bool {
        self.set_search_query_internal(query, false)
    }

    pub fn search_next(&mut self) -> bool {
        self.jump_to_search_match(true)
    }

    pub fn search_prev(&mut self) -> bool {
        self.jump_to_search_match(false)
    }

    pub fn search_word_under_cursor(&mut self) -> bool {
        // In visual mode, search for the selected text instead of the word under cursor
        let query = if let Some(selected_text) = self.visual_selection_text() {
            selected_text
        } else {
            let Some(word) = self.word_under_cursor() else {
                return false;
            };
            word
        };

        let changed = self.set_search_query_internal(&query, true);
        let moved = self.search_next();
        changed || moved
    }

    pub fn clear_search_highlights(&mut self) -> bool {
        self.set_search_query_internal("", false)
    }

    pub fn jump_to_line_and_column(&mut self, line_idx: usize, col_idx: usize) -> bool {
        if self.text.len_lines() == 0 {
            return false;
        }

        self.jump_to_line_col(line_idx, col_idx)
    }

    pub fn byte_to_char_idx(&self, byte_idx: usize) -> usize {
        self.text.byte_to_char(byte_idx.min(self.text.len_bytes()))
    }

    pub fn byte_to_line_idx(&self, byte_idx: usize) -> usize {
        if self.text.len_bytes() == 0 {
            return 0;
        }
        self.text
            .byte_to_line(byte_idx.min(self.text.len_bytes().saturating_sub(1)))
    }

    pub fn line_start_byte_idx(&self, line_idx: usize) -> usize {
        if self.text.len_lines() == 0 {
            return 0;
        }
        self.text
            .line_to_byte(line_idx.min(self.text.len_lines().saturating_sub(1)))
    }

    pub fn line_end_byte_idx(&self, line_idx: usize) -> usize {
        if self.text.len_lines() == 0 {
            return 0;
        }
        let clamped = line_idx.min(self.text.len_lines().saturating_sub(1));
        if clamped + 1 < self.text.len_lines() {
            self.text.line_to_byte(clamped + 1)
        } else {
            self.text.len_bytes()
        }
    }

    pub fn line_content_end_byte_idx(&self, line_idx: usize) -> usize {
        let line_end_char = self.line_content_end_char_idx(line_idx);
        self.text.char_to_byte(line_end_char)
    }

    pub fn line_char_to_byte_idx(&self, line_idx: usize, char_in_line: usize) -> usize {
        if self.text.len_lines() == 0 {
            return 0;
        }

        let clamped_line = line_idx.min(self.text.len_lines().saturating_sub(1));
        let line_start_char = self.text.line_to_char(clamped_line);
        let target_char = line_start_char + char_in_line.min(self.max_col_for_line(clamped_line));
        self.text.char_to_byte(target_char)
    }

    pub fn text_len_bytes(&self) -> usize {
        self.text.len_bytes()
    }

    pub fn text_string(&self) -> String {
        self.text.to_string()
    }

    pub fn line_start_byte_indices(&mut self) -> &[usize] {
        // If cache exists and is valid, return it
        if let Some(ref cached) = self.cached_line_starts {
            return cached;
        }

        // Otherwise, compute and cache
        let line_count = self.text.len_lines();
        let line_starts = if line_count == 0 {
            Vec::new()
        } else {
            (0..line_count)
                .map(|line_idx| self.text.line_to_byte(line_idx))
                .collect()
        };

        self.cached_line_starts = Some(line_starts);
        self.cached_line_starts.as_ref().unwrap()
    }

    pub fn invalidate_line_starts_cache(&mut self) {
        self.cached_line_starts = None;
    }

    pub fn line_string(&self, line_idx: usize) -> String {
        if self.text.len_lines() == 0 {
            return String::new();
        }
        self.text
            .line(line_idx.min(self.text.len_lines().saturating_sub(1)))
            .to_string()
    }

    pub fn take_highlight_edits(&mut self) -> Vec<HighlightEdit> {
        std::mem::take(&mut self.pending_highlight_edits)
    }

    /// Lấy prefix text để render mode file lớn mà không cần clone toàn bộ buffer.
    pub fn prefix_text(&self, max_chars: usize) -> String {
        self.text.chars().take(max_chars).collect()
    }

    pub fn active_file(&self) -> Option<&Path> {
        self.active_file.as_deref()
    }

    pub fn buffers(&self) -> &[BufferEntry] {
        &self.buffers
    }

    pub fn active_buffer_index(&self) -> Option<usize> {
        self.active_buffer_index
    }

    pub fn active_buffer(&self) -> Option<&BufferEntry> {
        self.active_buffer_index
            .and_then(|idx| self.buffers.get(idx))
    }

    pub fn active_text_buffer(&self) -> Option<&EditorBuffer> {
        let buffer = self.active_buffer()?;
        match &buffer.content {
            BufferContent::Text(text) => Some(text),
            _ => None,
        }
    }

    pub fn active_text_buffer_mut(&mut self) -> Option<&mut EditorBuffer> {
        let idx = self.active_buffer_index?;
        match &mut self.buffers.get_mut(idx)?.content {
            BufferContent::Text(text) => Some(text),
            _ => None,
        }
    }

    pub fn active_buffer_git_line_statuses(&self) -> Option<&HashMap<usize, GitLineStatus>> {
        self.active_text_buffer()
            .map(|buffer| &buffer.git_line_statuses)
    }

    pub fn active_buffer_is_terminal(&self) -> bool {
        self.active_buffer()
            .is_some_and(|buffer| matches!(buffer.content, BufferContent::Terminal(_)))
    }

    pub fn active_image_buffer(&self) -> Option<&ImageBuffer> {
        match self.active_buffer().map(|buffer| &buffer.content) {
            Some(BufferContent::Image(state)) => Some(state),
            _ => None,
        }
    }

    pub fn active_buffer_is_references(&self) -> bool {
        self.active_buffer()
            .is_some_and(|buffer| matches!(buffer.content, BufferContent::References(_)))
    }

    pub fn active_buffer_is_diagnostics(&self) -> bool {
        self.active_buffer()
            .is_some_and(|buffer| matches!(buffer.content, BufferContent::Diagnostics(_)))
    }

    pub fn active_references_buffer(&self) -> Option<&ReferencesBufferState> {
        match self.active_buffer().map(|buffer| &buffer.content) {
            Some(BufferContent::References(state)) => Some(state),
            _ => None,
        }
    }

    pub fn active_diagnostics_buffer(&self) -> Option<&DiagnosticsState> {
        match self.active_buffer().map(|buffer| &buffer.content) {
            Some(BufferContent::Diagnostics(state)) => Some(state),
            _ => None,
        }
    }

    pub fn active_references_origin(&self) -> Option<(PathBuf, usize)> {
        let state = self.active_references_buffer()?;
        Some((state.origin_path.clone()?, state.origin_line))
    }

    pub fn selected_reference_item(&self) -> Option<&ReferencesBufferItem> {
        let state = self.active_references_buffer()?;
        state.items.get(state.selected_index)
    }

    pub fn selected_reference_item_cloned(&self) -> Option<ReferencesBufferItem> {
        self.selected_reference_item().cloned()
    }

    pub fn selected_diagnostic_item(&self) -> Option<&DiagnosticItem> {
        let state = self.active_diagnostics_buffer()?;
        state.results.get(state.selected_index)
    }

    pub fn selected_diagnostic_item_cloned(&self) -> Option<DiagnosticItem> {
        self.selected_diagnostic_item().cloned()
    }

    pub fn active_terminal_session_id(&self) -> Option<u64> {
        match self.active_buffer().map(|buffer| &buffer.content) {
            Some(BufferContent::Terminal(state)) => state.session_id,
            _ => None,
        }
    }

    pub fn terminal_buffer_index_for_session(&self, session_id: u64) -> Option<usize> {
        self.buffers
            .iter()
            .position(|buffer| match &buffer.content {
                BufferContent::Terminal(state) => state.session_id == Some(session_id),
                BufferContent::Text(_)
                | BufferContent::Image(_)
                | BufferContent::References(_)
                | BufferContent::Diagnostics(_)
                | BufferContent::FuzzyPicker(_)
                | BufferContent::SettingsTab(_)
                | BufferContent::Help(_)
                | BufferContent::ExtensionsManager(_) => false,
            })
    }

    pub fn references_select_next(&mut self) -> bool {
        let Some(BufferContent::References(state)) = self
            .active_buffer_index
            .and_then(|idx| self.buffers.get_mut(idx))
            .map(|buffer| &mut buffer.content)
        else {
            return false;
        };

        if state.items.is_empty() {
            return false;
        }
        let next = (state.selected_index + 1) % state.items.len();
        if next == state.selected_index {
            return false;
        }
        state.selected_index = next;
        state.preview_lines.clear();
        state.preview_text.clear();
        state.preview_spans.clear();
        self.bump_revision();
        true
    }

    pub fn references_select_prev(&mut self) -> bool {
        let Some(BufferContent::References(state)) = self
            .active_buffer_index
            .and_then(|idx| self.buffers.get_mut(idx))
            .map(|buffer| &mut buffer.content)
        else {
            return false;
        };

        if state.items.is_empty() {
            return false;
        }
        let next = if state.selected_index == 0 {
            state.items.len().saturating_sub(1)
        } else {
            state.selected_index - 1
        };
        if next == state.selected_index {
            return false;
        }
        state.selected_index = next;
        state.preview_lines.clear();
        state.preview_text.clear();
        state.preview_spans.clear();
        self.bump_revision();
        true
    }

    pub fn diagnostics_select_next(&mut self) -> bool {
        let Some(BufferContent::Diagnostics(state)) = self
            .active_buffer_index
            .and_then(|idx| self.buffers.get_mut(idx))
            .map(|buffer| &mut buffer.content)
        else {
            return false;
        };

        if state.results.is_empty() {
            return false;
        }
        let next = (state.selected_index + 1) % state.results.len();
        if next == state.selected_index {
            return false;
        }
        state.selected_index = next;
        state.preview_lines.clear();
        state.preview_text.clear();
        state.preview_spans.clear();
        self.bump_revision();
        true
    }

    pub fn diagnostics_select_prev(&mut self) -> bool {
        let Some(BufferContent::Diagnostics(state)) = self
            .active_buffer_index
            .and_then(|idx| self.buffers.get_mut(idx))
            .map(|buffer| &mut buffer.content)
        else {
            return false;
        };

        if state.results.is_empty() {
            return false;
        }
        let next = if state.selected_index == 0 {
            state.results.len().saturating_sub(1)
        } else {
            state.selected_index - 1
        };
        if next == state.selected_index {
            return false;
        }
        state.selected_index = next;
        state.preview_lines.clear();
        state.preview_text.clear();
        state.preview_spans.clear();
        self.bump_revision();
        true
    }

    pub fn current_overlays(&self) -> &[EditorOverlay] {
        &self.current_overlays
    }

    pub fn completion(&self) -> Option<&CompletionState> {
        self.completion.as_ref()
    }

    pub fn has_completion(&self) -> bool {
        self.completion.is_some()
    }

    pub fn set_completion(&mut self, completion: CompletionState) -> bool {
        if self.completion.as_ref() == Some(&completion) {
            return false;
        }
        self.completion = Some(completion);
        self.bump_revision();
        true
    }

    pub fn clear_completion(&mut self) -> bool {
        if self.completion.is_none() {
            return false;
        }
        self.completion = None;
        self.bump_revision();
        true
    }

    pub fn inline_suggestion(&self) -> Option<&str> {
        self.inline_suggestion.as_deref()
    }

    pub fn set_inline_suggestion(&mut self, suggestion: Option<String>) -> bool {
        let normalized = suggestion.and_then(|text| {
            let trimmed = text.replace("\r\n", "\n").replace('\r', "\n");
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed)
            }
        });
        if self.inline_suggestion == normalized {
            return false;
        }
        self.inline_suggestion = normalized;
        self.bump_revision();
        true
    }

    pub fn clear_inline_suggestion(&mut self) -> bool {
        self.set_inline_suggestion(None)
    }

    pub fn append_inline_suggestion_chunk(&mut self, chunk: &str) -> bool {
        let normalized = chunk.replace("\r\n", "\n").replace('\r', "\n");
        if normalized.is_empty() {
            return false;
        }
        match self.inline_suggestion.as_mut() {
            Some(suggestion) => suggestion.push_str(&normalized),
            None => self.inline_suggestion = Some(normalized),
        }
        self.bump_revision();
        true
    }

    pub fn accept_inline_suggestion(&mut self) -> bool {
        let Some(suggestion) = self.inline_suggestion.clone() else {
            return false;
        };
        let char_count = suggestion.chars().count();
        if !self.apply_insert(self.cursor_char_idx, suggestion) {
            return false;
        }
        self.cursor_char_idx += char_count;
        let (_, col) = self.cursor_line_col();
        self.target_col = col;
        self.inline_suggestion = None;
        self.dirty = true;
        self.bump_revision();
        true
    }

    pub fn accept_inline_suggestion_word(&mut self) -> bool {
        let Some(suggestion) = self.inline_suggestion.clone() else {
            return false;
        };
        let split_byte = inline_suggestion_accept_prefix_byte_len(&suggestion);
        if split_byte == 0 {
            return false;
        }
        let accepted = suggestion[..split_byte].to_string();
        let remaining = suggestion[split_byte..].to_string();
        let accepted_chars = accepted.chars().count();
        if !self.apply_insert(self.cursor_char_idx, accepted) {
            return false;
        }
        self.cursor_char_idx += accepted_chars;
        let (_, col) = self.cursor_line_col();
        self.target_col = col;
        self.inline_suggestion = (!remaining.is_empty()).then_some(remaining);
        self.dirty = true;
        self.bump_revision();
        true
    }

    pub fn indent_config(&self) -> IndentConfig {
        self.indent_config
    }

    pub fn replace_active_document_text_preserve_cursor(&mut self, text: &str) -> bool {
        if self.active_text_buffer().is_none() {
            return false;
        }

        if self.text.to_string() == text {
            return false;
        }

        let (cursor_line, cursor_col) = self.cursor_line_col();
        self.text = Rope::from(text);
        self.cached_line_starts = None;

        let max_line = self.text.len_lines().saturating_sub(1);
        let clamped_line = cursor_line.min(max_line);
        let line_start = self.text.line_to_char(clamped_line);
        let max_col = self.max_col_for_line(clamped_line);
        self.cursor_char_idx = line_start + cursor_col.min(max_col);
        self.target_col = self.cursor_line_col().1;
        self.dirty = true;
        self.search_highlights.clear();
        self.clear_completion();
        self.clear_inline_suggestion();
        self.bump_revision();
        true
    }

    pub fn replace_active_document_text_preserve_cursor_with_undo(&mut self, text: &str) -> bool {
        if self.active_text_buffer().is_none() || self.text.to_string() == text {
            return false;
        }

        self.ensure_current_transaction();
        let changed = self.replace_active_document_text_preserve_cursor(text);
        if changed {
            let _ = self.commit_transaction();
        } else {
            self.current_transaction = None;
        }
        changed
    }

    pub fn completion_select_next(&mut self) -> bool {
        let Some(state) = self.completion.as_mut() else {
            return false;
        };
        if state.filtered_items.is_empty() {
            return false;
        }
        let next = (state.selected_index + 1) % state.filtered_items.len();
        if next == state.selected_index {
            return false;
        }
        state.selected_index = next;
        state.current_revision = state.current_revision.wrapping_add(1);
        self.bump_revision();
        true
    }

    pub fn completion_select_prev(&mut self) -> bool {
        let Some(state) = self.completion.as_mut() else {
            return false;
        };
        if state.filtered_items.is_empty() {
            return false;
        }
        let len = state.filtered_items.len();
        let next = (state.selected_index + len - 1) % len;
        if next == state.selected_index {
            return false;
        }
        state.selected_index = next;
        state.current_revision = state.current_revision.wrapping_add(1);
        self.bump_revision();
        true
    }

    pub fn selected_completion_item(&self) -> Option<&LspCompletionItem> {
        let state = self.completion.as_ref()?;
        state
            .filtered_items
            .get(state.selected_index)
            .map(|entry| &entry.item)
    }

    pub fn refresh_completion_with_prefix(&mut self, prefix: &str) -> bool {
        let Some(state) = self.completion.as_mut() else {
            return false;
        };

        let mut filtered_items = build_completion_display_items(&state.raw_items, prefix);

        let next_selected = if filtered_items.is_empty() { 0 } else { 0 };
        let changed = state.filtered_items != filtered_items
            || state.selected_index != next_selected
            || state.typed_prefix != prefix;
        if !changed {
            return false;
        }

        state.filtered_items.clear();
        state.filtered_items.append(&mut filtered_items);
        state.selected_index = next_selected;
        state.typed_prefix = prefix.to_string();
        self.bump_revision();
        true
    }
}

fn file_history_transaction_label(
    transaction: &Transaction,
) -> (String, crate::app::command_palette::CommandPaletteItemTone) {
    let edit = &transaction.edit;
    let inserted = diff_preview_text(&edit.inserted_text, 30);
    let deleted = diff_preview_text(&edit.deleted_text, 30);

    match (
        !edit.inserted_text.is_empty(),
        !edit.deleted_text.is_empty(),
    ) {
        (true, false) => (
            format!("[+ {inserted}]"),
            crate::app::command_palette::CommandPaletteItemTone::Added,
        ),
        (false, true) => (
            format!("[- {deleted}]"),
            crate::app::command_palette::CommandPaletteItemTone::Removed,
        ),
        (true, true) => (
            format!("[- {deleted}] [+ {inserted}]"),
            crate::app::command_palette::CommandPaletteItemTone::Modified,
        ),
        (false, false) => (
            "[no-op]".to_string(),
            crate::app::command_palette::CommandPaletteItemTone::Default,
        ),
    }
}

fn file_history_transaction_secondary(index: usize, transaction: &Transaction) -> String {
    let edit = &transaction.edit;
    match (
        !edit.inserted_text.is_empty(),
        !edit.deleted_text.is_empty(),
    ) {
        (true, false) => format!(
            "#{index} · inserted {} chars @ {}",
            edit.inserted_len_chars(),
            edit.start_char_idx
        ),
        (false, true) => format!(
            "#{index} · deleted {} chars @ {}",
            edit.deleted_len_chars(),
            edit.start_char_idx
        ),
        (true, true) => format!(
            "#{index} · replaced {} -> {} chars @ {}",
            edit.deleted_len_chars(),
            edit.inserted_len_chars(),
            edit.start_char_idx
        ),
        (false, false) => format!("#{index} · no text delta"),
    }
}

fn build_transaction_diff_preview_lines(transaction: &Transaction) -> Vec<FilePreviewLine> {
    let edit = &transaction.edit;
    let mut out = vec![
        FilePreviewLine {
            line_number: edit.start_char_idx,
            text: format!("@@ char {} @@", edit.start_char_idx),
            is_target: false,
        },
        FilePreviewLine {
            line_number: edit.start_char_idx,
            text: "--- deleted".to_string(),
            is_target: false,
        },
    ];

    push_delta_preview_lines(&mut out, "-", &edit.deleted_text, edit.start_char_idx);
    out.push(FilePreviewLine {
        line_number: edit.start_char_idx,
        text: "+++ inserted".to_string(),
        is_target: false,
    });
    push_delta_preview_lines(&mut out, "+", &edit.inserted_text, edit.start_char_idx);

    if edit.is_empty() {
        out.push(FilePreviewLine {
            line_number: edit.start_char_idx,
            text: "  (selected history entry has no text delta)".to_string(),
            is_target: false,
        });
    }

    out
}

fn push_delta_preview_lines(
    out: &mut Vec<FilePreviewLine>,
    prefix: &str,
    text: &str,
    start_char_idx: usize,
) {
    if text.is_empty() {
        out.push(FilePreviewLine {
            line_number: start_char_idx,
            text: format!("{prefix} <empty>"),
            is_target: false,
        });
        return;
    }

    for (offset, line) in text.lines().enumerate() {
        out.push(FilePreviewLine {
            line_number: start_char_idx + offset,
            text: format!("{prefix} {line}"),
            is_target: true,
        });
    }
    if text.ends_with('\n') {
        out.push(FilePreviewLine {
            line_number: start_char_idx + text.lines().count(),
            text: format!("{prefix} <newline>"),
            is_target: true,
        });
    }
}

fn diff_preview_text(text: &str, max_chars: usize) -> String {
    if text.is_empty() {
        return "<empty>".to_string();
    }

    let mut preview = String::new();
    for ch in text.chars().take(max_chars) {
        match ch {
            '\n' => preview.push_str("\\n"),
            '\t' => preview.push_str("\\t"),
            '\r' => preview.push_str("\\r"),
            _ => preview.push(ch),
        }
    }
    if text.chars().count() > max_chars {
        preview.push_str("...");
    }
    preview
}

fn undo_edit_on_rope(text: &mut Rope, edit: &EditTransaction) -> bool {
    if edit.is_empty() {
        return true;
    }
    if !range_matches_rope(
        text,
        edit.start_char_idx,
        edit.inserted_len_chars(),
        &edit.inserted_text,
    ) {
        return false;
    }

    if !edit.inserted_text.is_empty() {
        text.remove(edit.start_char_idx..edit.start_char_idx + edit.inserted_len_chars());
    }
    if !edit.deleted_text.is_empty() {
        text.insert(edit.start_char_idx, &edit.deleted_text);
    }
    true
}

fn range_matches_rope(
    text: &Rope,
    start_char_idx: usize,
    len_chars: usize,
    expected: &str,
) -> bool {
    if expected.is_empty() && len_chars == 0 {
        return start_char_idx <= text.len_chars();
    }

    let end_char_idx = start_char_idx.saturating_add(len_chars);
    if start_char_idx > text.len_chars() || end_char_idx > text.len_chars() {
        return false;
    }
    text.slice(start_char_idx..end_char_idx).to_string() == expected
}
