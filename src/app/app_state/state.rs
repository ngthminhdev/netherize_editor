use super::overlays::{collect_search_highlights, is_completion_identifier_char};
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
    pub fn minimap_visible(&self) -> bool {
        self.minimap_visible
    }

    pub fn toggle_minimap(&mut self) -> bool {
        self.minimap_visible = !self.minimap_visible;
        self.minimap_visible
    }

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
        let undo_len = history.undo_stack.len();
        history
            .undo_stack
            .iter()
            .enumerate()
            .rev()
            .map(|(index, transaction)| {
                let (label, tone) = file_history_transaction_label(transaction);
                // Display ordinal: newest = 1 (list is newest-first). The action
                // still carries the raw stack `index` used to restore.
                let ordinal = undo_len - index;
                crate::app::command_palette::CommandPaletteItem::file_history_entry(
                    label,
                    Some(file_history_transaction_secondary(ordinal, transaction)),
                    index,
                    tone,
                )
            })
            .collect()
    }

    /// Preview pane for the file-history picker: the WHOLE file as it was at the
    /// selected step (reconstructed into `preview_text` by
    /// `preview_file_history_index`), with the line(s) that step touched marked
    /// `is_target` so the renderer scrolls to and highlights the change.
    pub fn build_file_history_diff_preview(&self) -> Option<(Vec<FilePreviewLine>, String)> {
        let session = self.file_history_preview.as_ref()?;
        let preview_index = session.preview_index?;
        let text = session.preview_text.as_ref()?;

        // The edit's start maps to a line in the reconstructed text; the inserted
        // text's line span tells us how many lines it produced there.
        let (target_start, target_end) =
            if let Some(transaction) = session.baseline_history.undo_stack.get(preview_index) {
                let edit = &transaction.edit;
                let char_idx = edit.start_char_idx.min(text.len_chars());
                let start_line = text.char_to_line(char_idx);
                let inserted_lines = edit.inserted_text.lines().count().max(1);
                (start_line, start_line + inserted_lines - 1)
            } else {
                (0, 0)
            };

        // Window the preview to a bounded region around the change. Keeps it under
        // the inline tree-sitter cap (300 lines) so the pane actually gets syntax
        // colors on long files, and bounds the per-keystroke highlight cost. Real
        // line numbers are preserved so the gutter and scroll stay correct.
        const PREVIEW_CONTEXT: usize = 120;
        let total = text.len_lines();
        let win_start = target_start.saturating_sub(PREVIEW_CONTEXT);
        let win_end = (target_end + PREVIEW_CONTEXT + 1).min(total);

        let lines: Vec<FilePreviewLine> = text
            .lines()
            .enumerate()
            .skip(win_start)
            .take(win_end - win_start)
            .map(|(i, line)| FilePreviewLine {
                line_number: i + 1,
                text: line.to_string().trim_end_matches('\n').to_string(),
                is_target: i >= target_start && i <= target_end,
            })
            .collect();
        let preview_text = lines
            .iter()
            .map(|line| line.text.clone())
            .collect::<Vec<_>>()
            .join("\n");
        Some((lines, preview_text))
    }

    /// Non-destructive time-travel: move the buffer back to the state AFTER
    /// undo-stack entry `index`, keeping every later step on the redo stack so it
    /// can be redone. Assumes the source text buffer is the active buffer (the
    /// file-history picker closes and reactivates it before this runs). Just
    /// replays `undo()` — reusing the tested edit-inversion path.
    pub fn restore_to_history_index(&mut self, index: usize) -> bool {
        let len = self.history.undo_stack.len();
        if index >= len {
            return false;
        }
        let steps = len - (index + 1);
        let mut moved = false;
        for _ in 0..steps {
            if !self.undo() {
                break;
            }
            moved = true;
        }
        moved
    }

    pub fn begin_file_history_preview_session(&mut self) -> bool {
        let Some(_file_path) = self.active_file.clone() else {
            return false;
        };
        // Always start fresh, overwriting any session a prior picker leaked —
        // the session is an inert read cache, so replacing it is safe and stops
        // a stale one from blocking re-open.
        self.file_history_preview = Some(FileHistoryPreviewSession {
            baseline_text: self.text.clone(),
            baseline_history: self.history.clone(),
            preview_index: None,
            preview_text: None,
        });
        true
    }

    pub fn preview_file_history_index(&mut self, transaction_index: usize) -> bool {
        let Some((baseline_text, baseline_history)) =
            self.file_history_preview.as_ref().map(|session| {
                (
                    session.baseline_text.clone(),
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

        // Reconstruct the file text at that step by undoing the later transactions
        // on a throwaway rope. Cache it for the preview PANE only — do NOT push it
        // into the live editor. The picker is a separate center-pane buffer, so the
        // live view isn't visible here anyway, and mutating it is exactly what let
        // a lingering session clobber real edits on the next save.
        let mut text = baseline_text;
        for transaction in baseline_history
            .undo_stack
            .iter()
            .skip(transaction_index + 1)
            .rev()
        {
            if !undo_edit_on_rope(&mut text, &transaction.edit) {
                return false;
            }
        }

        if let Some(session) = self.file_history_preview.as_mut() {
            session.preview_index = Some(transaction_index);
            session.preview_text = Some(text);
            return true;
        }
        false
    }

    /// Source file the active file-history picker is scrubbing, if that picker is
    /// the active buffer. Lets the event loop return to it after the picker closes.
    pub fn file_history_source_path(&self) -> Option<PathBuf> {
        let idx = self.active_buffer_index?;
        let slot = self.buffers.get(idx)?;
        if let BufferContent::FuzzyPicker(state) = &slot.content {
            if state.mode == CommandPaletteMode::FileHistory {
                return state.source_file_path.clone();
            }
        }
        None
    }

    /// End the file-history preview session. The session is a read-only cache for
    /// the preview pane and never owns live editor state, so there is nothing to
    /// restore — the old restore-on-cancel here is what clobbered real edits on
    /// the next `:w`. Just drop it.
    pub fn cancel_file_history_preview(&mut self) -> bool {
        self.file_history_preview.take().is_some()
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

    /// Vim f/F/t/T motion + highlight: di chuyển cursor tới ký tự trên dòng hiện
    /// tại VÀ set search query thành ký tự đó (như `*`) để mọi occurrence được
    /// highlight và n/N nhảy tới/lui giữa các match toàn file.
    pub fn find_char_motion_and_highlight(
        &mut self,
        kind: crate::core::commands::FindMotionKind,
        target: char,
    ) -> bool {
        let moved = self.move_find_char(kind, target);
        let highlighted = self.set_search_query_internal(&target.to_string(), false);
        moved || highlighted
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

    pub fn active_buffer_is_markdown_preview(&self) -> bool {
        self.active_buffer()
            .is_some_and(|buffer| matches!(buffer.content, BufferContent::MarkdownPreview(_)))
    }

    pub fn active_references_buffer(&self) -> Option<&ReferencesBufferState> {
        match self.active_buffer().map(|buffer| &buffer.content) {
            Some(BufferContent::References(state)) => Some(state),
            _ => None,
        }
    }

    pub fn active_references_buffer_mut(&mut self) -> Option<&mut ReferencesBufferState> {
        let idx = self.active_buffer_index?;
        match &mut self.buffers.get_mut(idx)?.content {
            BufferContent::References(state) => Some(state),
            _ => None,
        }
    }

    pub fn active_diagnostics_buffer(&self) -> Option<&DiagnosticsState> {
        match self.active_buffer().map(|buffer| &buffer.content) {
            Some(BufferContent::Diagnostics(state)) => Some(state),
            _ => None,
        }
    }

    pub fn active_markdown_preview_buffer(&self) -> Option<&MarkdownPreviewState> {
        match self.active_buffer().map(|buffer| &buffer.content) {
            Some(BufferContent::MarkdownPreview(state)) => Some(state),
            _ => None,
        }
    }

    pub fn active_markdown_preview_buffer_mut(&mut self) -> Option<&mut MarkdownPreviewState> {
        let idx = self.active_buffer_index?;
        match &mut self.buffers.get_mut(idx)?.content {
            BufferContent::MarkdownPreview(state) => Some(state),
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
                | BufferContent::MarkdownPreview(_)
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

        let old_index = state.selected_index;
        let len = state.items.len();
        if len == 0 {
            return false;
        }

        let mut new_index = old_index;
        loop {
            new_index = (new_index + 1) % len;
            if new_index == old_index {
                return false;
            }
            let item = &state.items[new_index];
            if !state.collapsed_paths.contains(&item.relative_path) {
                break;
            }
        }

        state.selected_index = new_index;
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

        let old_index = state.selected_index;
        let len = state.items.len();
        if len == 0 {
            return false;
        }

        let mut new_index = old_index;
        loop {
            new_index = if new_index == 0 {
                len - 1
            } else {
                new_index - 1
            };
            if new_index == old_index {
                return false;
            }
            let item = &state.items[new_index];
            if !state.collapsed_paths.contains(&item.relative_path) {
                break;
            }
        }

        state.selected_index = new_index;
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

    pub fn completion_mut(&mut self) -> Option<&mut CompletionState> {
        self.completion.as_mut()
    }

    pub fn has_completion(&self) -> bool {
        self.completion.is_some()
    }

    pub fn workspace_symbol_cache(&self) -> &Arc<crate::lsp::WorkspaceSymbolCache> {
        &self.workspace_symbol_cache
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

    /// Prefix-matching retention: when the user types exactly the head of the
    /// visible ghost text, consume that head and keep showing the rest instead
    /// of clearing and re-requesting. Returns true only when a non-empty
    /// remainder stays visible.
    pub fn retain_inline_suggestion_for_typed_text(&mut self, typed: &str) -> bool {
        let Some(suggestion) = self.inline_suggestion.as_ref() else {
            return false;
        };
        if typed.is_empty() || typed.len() >= suggestion.len() || !suggestion.starts_with(typed) {
            return false;
        }
        let remaining = suggestion[typed.len()..].to_string();
        self.inline_suggestion = Some(remaining);
        self.bump_revision();
        true
    }

    /// Context around the caret for sanitizing model output: the current line
    /// up to the caret, and up to `suffix_take` chars after the caret.
    pub fn inline_suggestion_context(&self, suffix_take: usize) -> (String, String) {
        let total = self.text.len_chars();
        let cursor = self.cursor_char_idx.min(total);
        let (line, _) = self.cursor_line_col();
        let line_start = self
            .text
            .line_to_char(line.min(self.text.len_lines().saturating_sub(1)));
        let line_prefix = if line_start <= cursor {
            self.text.slice(line_start..cursor).to_string()
        } else {
            String::new()
        };
        let suffix_end = cursor.saturating_add(suffix_take).min(total);
        let suffix = self.text.slice(cursor..suffix_end).to_string();
        (line_prefix, suffix)
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

        // Use cached items for incremental filtering (much faster than re-requesting from LSP)
        let mut filtered_items =
            super::overlays::filter_cached_completion_items(&state.cached_full_items, prefix);

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
    let inserted = diff_preview_text(&edit.inserted_text, 40);
    let deleted = diff_preview_text(&edit.deleted_text, 40);

    match (
        !edit.inserted_text.is_empty(),
        !edit.deleted_text.is_empty(),
    ) {
        (true, false) => (
            format!("Insert  \"{inserted}\""),
            crate::app::command_palette::CommandPaletteItemTone::Added,
        ),
        (false, true) => (
            format!("Delete  \"{deleted}\""),
            crate::app::command_palette::CommandPaletteItemTone::Removed,
        ),
        (true, true) => (
            format!("Replace  \"{deleted}\" -> \"{inserted}\""),
            crate::app::command_palette::CommandPaletteItemTone::Modified,
        ),
        (false, false) => (
            "No change".to_string(),
            crate::app::command_palette::CommandPaletteItemTone::Default,
        ),
    }
}

/// `ordinal` is the human-facing step number (1 = newest), not the raw stack index.
fn file_history_transaction_secondary(ordinal: usize, transaction: &Transaction) -> String {
    let edit = &transaction.edit;
    let ins = edit.inserted_len_chars();
    let del = edit.deleted_len_chars();
    let delta = match (ins > 0, del > 0) {
        (true, false) => format!("+{ins}"),
        (false, true) => format!("-{del}"),
        (true, true) => format!("+{ins} -{del}"),
        (false, false) => "0".to_string(),
    };
    let step = if ordinal == 1 {
        "latest".to_string()
    } else {
        format!("{ordinal} steps back")
    };
    format!("#{ordinal} · {step} · {delta} chars")
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

#[cfg(test)]
mod inline_suggestion_tests {
    use super::AppState;

    fn state_with_text(text: &str) -> AppState {
        AppState::from_text(std::path::PathBuf::from("inline_suggestion_test.rs"), text)
    }

    #[test]
    fn retain_consumes_matching_typed_char() {
        let mut state = state_with_text("");
        assert!(state.set_inline_suggestion(Some("println".to_string())));
        assert!(state.retain_inline_suggestion_for_typed_text("p"));
        assert_eq!(state.inline_suggestion(), Some("rintln"));
    }

    #[test]
    fn retain_rejects_mismatch_and_keeps_suggestion_for_caller_to_clear() {
        let mut state = state_with_text("");
        assert!(state.set_inline_suggestion(Some("println".to_string())));
        assert!(!state.retain_inline_suggestion_for_typed_text("x"));
        assert_eq!(state.inline_suggestion(), Some("println"));
    }

    #[test]
    fn retain_rejects_when_typed_text_exhausts_suggestion() {
        let mut state = state_with_text("");
        assert!(state.set_inline_suggestion(Some("ok".to_string())));
        assert!(!state.retain_inline_suggestion_for_typed_text("ok"));
    }

    #[test]
    fn retain_handles_multichar_typed_text() {
        let mut state = state_with_text("");
        assert!(state.set_inline_suggestion(Some("hello world".to_string())));
        assert!(state.retain_inline_suggestion_for_typed_text("hello"));
        assert_eq!(state.inline_suggestion(), Some(" world"));
    }

    #[test]
    fn accept_full_inserts_at_cursor_and_clears() {
        let mut state = state_with_text("");
        assert!(state.set_inline_suggestion(Some("fn main() {}".to_string())));
        assert!(state.accept_inline_suggestion());
        assert_eq!(state.text_string(), "fn main() {}");
        assert_eq!(state.inline_suggestion(), None);
    }

    #[test]
    fn accept_multiline_preserves_text_and_cursor_at_end() {
        // Caret sits after one tab of indentation on an empty body line.
        let initial = "func Sum() int {\n\ttotal := 0\n\t";
        let mut state = state_with_text(initial);
        for _ in 0..initial.chars().count() {
            state.move_right();
        }
        // Model continuation with absolute indentation for lines 2+.
        let suggestion = "for _, num := range nums {\n\t\ttotal += num\n\t}\n\treturn total";
        assert!(state.set_inline_suggestion(Some(suggestion.to_string())));
        assert!(state.accept_inline_suggestion());

        let expected = format!("{initial}{suggestion}");
        assert_eq!(state.text_string(), expected);
        // Cursor must land at the very end of the inserted text.
        assert_eq!(state.cursor_char_idx(), expected.chars().count());
        let (line, col) = state.cursor_line_col();
        assert_eq!((line, col), (5, "\treturn total".chars().count()));
        assert_eq!(state.inline_suggestion(), None);
    }

    #[test]
    fn accept_word_inserts_first_token_and_keeps_rest() {
        let mut state = state_with_text("");
        assert!(state.set_inline_suggestion(Some("hello world".to_string())));
        assert!(state.accept_inline_suggestion_word());
        assert_eq!(state.text_string(), "hello");
        assert_eq!(state.inline_suggestion(), Some(" world"));
    }

    #[test]
    fn context_returns_line_prefix_and_suffix() {
        let mut state = state_with_text("foo(\nbar");
        // Place caret after "foo(" on the first line.
        for _ in 0..4 {
            state.move_right();
        }
        let (line_prefix, suffix) = state.inline_suggestion_context(200);
        assert_eq!(line_prefix, "foo(");
        assert_eq!(suffix, "\nbar");
    }
}
