use super::overlays::{build_completion_display_items, is_completion_identifier_char};
use super::*;

impl AppState {
    pub fn undo(&mut self) -> bool {
        let Some(transaction) = self.history.undo_stack.pop() else {
            return false;
        };

        self.current_transaction = None;
        for action in transaction.actions.iter().rev() {
            match action {
                EditAction::Insert { index, text } => {
                    self.record_delete_highlight_edit(*index, text.chars().count());
                    let _ = self.apply_delete_raw(*index, text.chars().count());
                }
                EditAction::Delete { index, text } => {
                    self.record_insert_highlight_edit(*index, text);
                    self.apply_insert_raw(*index, text);
                }
            }
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
        for action in &transaction.actions {
            match action {
                EditAction::Insert { index, text } => {
                    self.record_insert_highlight_edit(*index, text);
                    self.apply_insert_raw(*index, text);
                }
                EditAction::Delete { index, text } => {
                    self.record_delete_highlight_edit(*index, text.chars().count());
                    let _ = self.apply_delete_raw(*index, text.chars().count());
                }
            }
        }

        self.restore_cursor_state(transaction.after_cursor);
        self.selection_anchor_char_idx = None;
        self.visual_line_mode = false;
        self.dirty = true;
        self.history.undo_stack.push(transaction);
        self.bump_revision();
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
        let Some(query) = self.word_under_cursor() else {
            return false;
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

        let target_line = line_idx.min(self.text.len_lines().saturating_sub(1));
        let line_start = self.text.line_to_char(target_line);
        let target_char = line_start + col_idx.min(self.max_col_for_line(target_line));
        self.move_cursor_to_char_idx(target_char)
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

    pub fn active_buffer_git_diff(&self) -> Option<&BufferGitDiff> {
        self.active_buffer()
            .and_then(|buffer| buffer.git_diff.as_ref())
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
                | BufferContent::Help(_) => false,
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
