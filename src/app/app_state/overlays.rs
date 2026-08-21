use super::*;

const INTERACTIVE_TEXT_FILE_LIMIT_BYTES: u64 = 10 * 1024 * 1024;

pub(super) fn ensure_interactive_text_file_size(path: &Path) -> Result<(), String> {
    let bytes = std::fs::metadata(path)
        .map_err(|err| format!("inspect file {:?} failed: {err}", path))?
        .len();
    if bytes > INTERACTIVE_TEXT_FILE_LIMIT_BYTES {
        return Err(format!(
            "open file {:?} refused: {bytes} bytes exceeds the 10 MiB interactive editor limit",
            path
        ));
    }
    Ok(())
}

impl AppState {
    pub fn set_current_overlays(&mut self, overlays: Vec<EditorOverlay>) -> bool {
        if self.current_overlays == overlays {
            return false;
        }
        self.current_overlays = overlays;
        true
    }

    /// Update an item's `detail` (signature) by label. Used to apply data filled in
    /// by `completionItem/resolve` (some LSPs only populate detail via resolve).
    /// Updates both the raw item and the matching display item so the renderer sees it.
    pub fn update_completion_item_detail(&mut self, label: &str, detail: Option<String>) {
        let Some(state) = self.completion.as_mut() else {
            return;
        };
        let mut changed = false;
        for raw in state.raw_items.iter_mut() {
            if raw.label == label && raw.detail != detail {
                raw.detail = detail.clone();
                changed = true;
            }
        }
        for entry in state.filtered_items.iter_mut() {
            if entry.item.label == label && entry.item.detail != detail {
                entry.item.detail = detail.clone();
                changed = true;
            }
        }
        for entry in state.cached_full_items.iter_mut() {
            if entry.item.label == label && entry.item.detail != detail {
                entry.item.detail = detail.clone();
                changed = true;
            }
        }
        if changed {
            self.revision += 1;
        }
    }

    pub fn update_completion_item_from_resolve(
        &mut self,
        label: &str,
        resolved: crate::async_runtime::message::LspCompletionItem,
    ) {
        let Some(state) = self.completion.as_mut() else {
            return;
        };
        let mut changed = false;
        for raw in state.raw_items.iter_mut() {
            if raw.label == label {
                *raw = merge_completion_item(raw, &resolved);
                changed = true;
            }
        }
        for entry in state.filtered_items.iter_mut() {
            if entry.item.label == label {
                entry.item = merge_completion_item(&entry.item, &resolved);
                changed = true;
            }
        }
        for entry in state.cached_full_items.iter_mut() {
            if entry.item.label == label {
                entry.item = merge_completion_item(&entry.item, &resolved);
                changed = true;
            }
        }
        if changed {
            self.revision += 1;
        }
    }

    pub fn set_completion_hover_doc(&mut self, doc: Option<String>) {
        if let Some(state) = self.completion.as_mut() {
            state.hover_doc = doc;
            // Calling with `None` means "back to loading" (e.g. on selection change).
            // Calling with `Some(_)` means we have a definitive answer.
            state.hover_doc_resolved = state.hover_doc.is_some();
            self.revision += 1;
        }
    }

    /// Mark the resolve as finished without populating any docs (e.g. server returned
    /// no documentation, or the request failed, or no resolve was needed). Lets the
    /// UI swap "Loading…" for "No docs available".
    pub fn mark_completion_hover_doc_resolved(&mut self) {
        if let Some(state) = self.completion.as_mut() {
            if !state.hover_doc_resolved {
                state.hover_doc_resolved = true;
                self.revision += 1;
            }
        }
    }

    pub fn set_completion_loading(&mut self, loading: bool) {
        self.completion_loading = loading;
    }

    pub fn is_completion_loading(&self) -> bool {
        self.completion_loading
    }

    pub fn clear_current_overlays(&mut self) -> bool {
        if self.current_overlays.is_empty() {
            return false;
        }
        self.current_overlays.clear();
        true
    }

    pub fn has_scrollable_floating_overlay(&self) -> bool {
        self.current_overlays.iter().any(|overlay| {
            matches!(
                overlay,
                EditorOverlay::FloatingBox {
                    style: FloatingBoxStyle::DocHover,
                    ..
                }
            )
        })
    }

    pub fn scroll_floating_overlay_lines(&mut self, delta_lines: isize) -> bool {
        let mut changed = false;
        for overlay in &mut self.current_overlays {
            let EditorOverlay::FloatingBox {
                style: FloatingBoxStyle::DocHover,
                scroll,
                ..
            } = overlay
            else {
                continue;
            };

            let next = if delta_lines.is_negative() {
                scroll
                    .offset_lines
                    .saturating_sub(delta_lines.unsigned_abs())
            } else {
                scroll.offset_lines.saturating_add(delta_lines as usize)
            };
            if next != scroll.offset_lines {
                scroll.offset_lines = next;
                changed = true;
            }
        }
        if changed {
            self.revision += 1;
        }
        changed
    }

    pub fn scroll_floating_overlay_half_page(&mut self, down: bool) -> bool {
        let delta = if down { 8 } else { -8 };
        self.scroll_floating_overlay_lines(delta)
    }

    pub fn clamp_floating_overlay_scroll(&mut self, max_offset_lines: usize) -> bool {
        let mut changed = false;
        for overlay in &mut self.current_overlays {
            let EditorOverlay::FloatingBox {
                style: FloatingBoxStyle::DocHover,
                scroll,
                ..
            } = overlay
            else {
                continue;
            };
            if scroll.offset_lines > max_offset_lines {
                scroll.offset_lines = max_offset_lines;
                changed = true;
            }
        }
        if changed {
            self.revision += 1;
        }
        changed
    }

    pub fn active_filetype_label(&self) -> &'static str {
        if self.active_buffer_is_terminal() {
            return "Terminal";
        }
        if self.active_buffer_is_diagnostics() {
            return "Diagnostics";
        }
        if self.active_buffer_is_markdown_preview() {
            return "Markdown Preview";
        }
        if self.active_buffer_is_references() {
            return "References";
        }
        self.active_file
            .as_deref()
            .map(filetype_label_for_path)
            .unwrap_or("Plain Text")
    }

    pub fn default_save_path(&self) -> &Path {
        &self.default_save_path
    }

    pub fn len_chars(&self) -> usize {
        self.text.len_chars()
    }

    pub fn len_lines(&self) -> usize {
        self.text.len_lines()
    }

    pub fn len_bytes(&self) -> usize {
        self.text.len_bytes()
    }

    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    pub fn revision(&self) -> u64 {
        self.revision
    }

    pub fn current_mode(&self) -> EditorMode {
        self.mode_state.current()
    }

    pub fn can_apply_mode_event(&self, event: ModeEvent) -> bool {
        self.mode_state.can_apply(event)
    }

    pub fn apply_mode_event(
        &mut self,
        event: ModeEvent,
    ) -> Result<ModeTransitionResult, ModeTransitionError> {
        let result = self.mode_state.apply(event)?;
        if result.from == EditorMode::Insert && result.to != EditorMode::Insert {
            let _ = self.commit_transaction();
            self.inline_suggestion = None;
        }
        if result.from == EditorMode::MultiInsert && result.to != EditorMode::MultiInsert {
            let _ = self.commit_transaction();
        }
        if matches!(
            result.to,
            EditorMode::Normal | EditorMode::Insert | EditorMode::Visual
        ) && matches!(
            result.from,
            EditorMode::MultiCursor | EditorMode::MultiInsert
        ) {
            self.clear_virtual_cursors();
        }
        Ok(result)
    }

    pub fn preview(&self, max_chars: usize) -> String {
        let mut preview = String::new();
        for ch in self.text.chars().take(max_chars) {
            for escaped in ch.escape_default() {
                preview.push(escaped);
            }
        }

        if preview.is_empty() {
            return "<empty>".to_string();
        }

        if self.text.len_chars() > max_chars {
            preview.push_str("...");
        }
        preview
    }

    pub fn debug_state_line(&self) -> String {
        let (line, col) = self.cursor_line_col();
        let file_text = self
            .active_file()
            .map(|path| path.display().to_string())
            .unwrap_or_else(|| "<none>".to_string());
        let palette_query = if self.is_command_palette_visible() {
            self.command_palette_query_text()
        } else {
            ""
        };
        let palette_mode = self
            .command_palette_mode()
            .map(|mode| format!("{mode:?}"))
            .unwrap_or_else(|| "None".to_string());
        let selection = self
            .visual_selection_range()
            .map(|range| format!("{}..{}", range.start_char, range.end_char))
            .unwrap_or_else(|| "-".to_string());

        format!(
            "mode={} cursor=({},{}) chars={} lines={} bytes={} dirty={} rev={} palette_visible={} palette_mode={} terminal_open={} open_buffers={} active_buffer_index={:?} visual_selection={} palette_query={:?} palette_results={} conflict={:?} notice={:?} file={} preview=\"{}\"",
            self.current_mode().as_str(),
            line,
            col,
            self.len_chars(),
            self.len_lines(),
            self.len_bytes(),
            self.is_dirty(),
            self.revision(),
            self.is_command_palette_visible(),
            palette_mode,
            self.is_terminal_panel_open(),
            self.buffers.len(),
            self.active_buffer_index,
            selection,
            palette_query,
            self.command_palette.results.len(),
            self.external_conflict_message(),
            self.last_external_notice(),
            file_text,
            self.preview(48)
        )
    }

    pub(super) fn cursor_state(&self) -> CursorState {
        CursorState {
            char_idx: self.cursor_char_idx,
            target_col: self.target_col,
        }
    }

    pub(super) fn text_buffer_view_state(&self) -> TextBufferViewState {
        TextBufferViewState {
            cursor: self.cursor_state(),
            selection_anchor_char_idx: self.selection_anchor_char_idx,
            visual_line_mode: self.visual_line_mode,
            target_scroll_y: self.target_scroll_y,
            current_scroll_y: self.current_scroll_y,
            scroll_column: self.scroll_column,
        }
    }

    pub(super) fn restore_cursor_state(&mut self, state: CursorState) {
        self.cursor_char_idx = state.char_idx.min(self.text.len_chars());
        let line_idx = self
            .text
            .char_to_line(self.cursor_char_idx.min(self.text.len_chars()));
        self.target_col = state.target_col.min(self.max_col_for_line(line_idx));
    }

    pub(super) fn restore_text_buffer_view_state(&mut self, state: TextBufferViewState) {
        self.restore_cursor_state(state.cursor);
        let max_char_idx = self.text.len_chars();
        self.selection_anchor_char_idx = state
            .selection_anchor_char_idx
            .map(|anchor| anchor.min(max_char_idx));
        if self.selection_anchor_char_idx == Some(self.cursor_char_idx) {
            self.selection_anchor_char_idx = None;
        }

        let max_scroll = self.text.len_lines().saturating_sub(1) as f32;
        self.target_scroll_y = state.target_scroll_y.min(max_scroll);
        self.current_scroll_y = state.current_scroll_y.min(max_scroll);
        self.scroll_column = state.scroll_column;
        self.visual_line_mode = state.visual_line_mode && self.selection_anchor_char_idx.is_some();
    }

    pub(super) fn ensure_current_transaction(&mut self) {
        if self.current_transaction.is_none() {
            self.current_transaction = Some(PendingTransaction {
                before_text: self.text.clone(),
                before_cursor: self.cursor_state(),
            });
        }
    }

    pub(super) fn apply_insert(&mut self, index: usize, text: String) -> bool {
        if text.is_empty() {
            return false;
        }

        self.ensure_current_transaction();
        let insert_at = index.min(self.text.len_chars());
        self.record_insert_highlight_edit(insert_at, &text);
        self.apply_insert_raw(insert_at, &text);
        true
    }

    pub(super) fn apply_delete(&mut self, index: usize, len_chars: usize) -> bool {
        if len_chars == 0 || index >= self.text.len_chars() {
            return false;
        }

        let end = (index + len_chars).min(self.text.len_chars());
        self.ensure_current_transaction();
        self.record_delete_highlight_edit(index, end - index);
        if self.apply_delete_raw(index, end - index).is_none() {
            return false;
        }
        true
    }

    pub(super) fn apply_insert_raw(&mut self, index: usize, text: &str) {
        if text.is_empty() {
            return;
        }

        let insert_at = index.min(self.text.len_chars());
        self.text.insert(insert_at, text);
        let _ = self.refresh_active_search_highlights();

        // Clear folded ranges when text is modified to prevent corruption
        if !self.folded_ranges.is_empty() {
            self.folded_ranges.clear();
        }
        if !self.auto_folded_long_lines.is_empty() {
            self.auto_folded_long_lines.clear();
        }
    }

    pub(super) fn apply_delete_raw(&mut self, index: usize, len_chars: usize) -> Option<String> {
        if len_chars == 0 || index >= self.text.len_chars() {
            return None;
        }

        let end = (index + len_chars).min(self.text.len_chars());
        if end <= index {
            return None;
        }

        let deleted = self.text.slice(index..end).to_string();
        self.text.remove(index..end);
        let _ = self.refresh_active_search_highlights();

        // Clear folded ranges when text is modified to prevent corruption
        if !self.folded_ranges.is_empty() {
            self.folded_ranges.clear();
        }
        if !self.auto_folded_long_lines.is_empty() {
            self.auto_folded_long_lines.clear();
        }

        Some(deleted)
    }

    pub(super) fn char_range_text(&self, start: usize, end: usize) -> Option<String> {
        if start >= end || start >= self.text.len_chars() {
            return None;
        }

        let end = end.min(self.text.len_chars());
        if end <= start {
            return None;
        }

        Some(self.text.slice(start..end).to_string())
    }

    pub(super) fn linewise_text_for_range(&self, start: usize, end: usize) -> Option<String> {
        let mut text = self.char_range_text(start, end)?;
        if !text.ends_with('\n') {
            text.push('\n');
        }
        Some(text)
    }

    pub(super) fn delete_char_range_at_cursor(&self) -> Option<(usize, usize)> {
        if self.text.len_chars() == 0 {
            return None;
        }

        let line_idx = self
            .text
            .char_to_line(self.cursor_char_idx.min(self.text.len_chars()));
        let line_start = self.text.line_to_char(line_idx);
        let line_end = self.line_content_end_char_idx(line_idx);
        if line_start == line_end {
            return None;
        }

        let mut delete_idx = if self.cursor_char_idx < line_end {
            self.cursor_char_idx
        } else {
            line_end.saturating_sub(1)
        };
        if delete_idx < line_start {
            delete_idx = line_start;
        }
        if delete_idx >= self.text.len_chars() {
            return None;
        }

        Some((delete_idx, delete_idx + 1))
    }

    pub(super) fn current_line_delete_range(&self) -> Option<(usize, usize)> {
        if self.text.len_lines() == 0 {
            return None;
        }

        let line_idx = self
            .text
            .char_to_line(self.cursor_char_idx.min(self.text.len_chars()));
        let line_start = self.text.line_to_char(line_idx);
        let mut line_end = if line_idx + 1 < self.text.len_lines() {
            self.text.line_to_char(line_idx + 1)
        } else {
            self.text.len_chars()
        };
        let mut delete_start = line_start;

        if delete_start == line_end && line_idx > 0 {
            delete_start = delete_start.saturating_sub(1);
            line_end = line_end.max(delete_start);
        }

        (delete_start < line_end).then_some((delete_start, line_end))
    }

    pub(super) fn delete_word_forward_range(&self) -> Option<(usize, usize)> {
        let n = self.text.len_chars();
        if self.cursor_char_idx >= n {
            return None;
        }

        let end = next_word_start(&self.text, self.cursor_char_idx);
        (end > self.cursor_char_idx).then_some((self.cursor_char_idx, end))
    }

    pub(super) fn delete_word_backward_range(&self) -> Option<(usize, usize)> {
        if self.cursor_char_idx == 0 {
            return None;
        }

        let start = previous_word_start(&self.text, self.cursor_char_idx);
        (start < self.cursor_char_idx).then_some((start, self.cursor_char_idx))
    }

    pub(super) fn yank_word_end_range(&self) -> Option<(usize, usize)> {
        let n = self.text.len_chars();
        if self.cursor_char_idx >= n {
            return None;
        }

        let end = word_end_from_cursor(&self.text, self.cursor_char_idx)?;
        (end >= self.cursor_char_idx).then_some((self.cursor_char_idx, end + 1))
    }

    pub(super) fn paste_linewise(&mut self, text: &str, before: bool) -> bool {
        let mut insert_text = text.to_string();
        if insert_text.is_empty() {
            return false;
        }

        let total_chars = self.text.len_chars();
        let line_idx = if total_chars == 0 {
            0
        } else {
            self.text
                .char_to_line(self.cursor_char_idx.min(total_chars))
        };
        let line_start = self.text.line_to_char(line_idx);
        let has_following_line = line_idx + 1 < self.text.len_lines();
        let insert_at = if before {
            line_start
        } else if has_following_line {
            self.text.line_to_char(line_idx + 1)
        } else {
            total_chars
        };

        let buffer_has_trailing_newline =
            total_chars > 0 && self.text.char(total_chars.saturating_sub(1)) == '\n';
        let inserted_line_start = if before {
            insert_at
        } else if total_chars == 0 || has_following_line || buffer_has_trailing_newline {
            insert_at
        } else {
            insert_text = format!("\n{insert_text}");
            insert_at + 1
        };

        if !self.apply_insert(insert_at, insert_text) {
            return false;
        }

        self.cursor_char_idx = inserted_line_start.min(self.text.len_chars());
        let (_, col) = self.cursor_line_col();
        self.target_col = col;
        self.dirty = true;
        self.bump_revision();
        true
    }

    pub(super) fn clear_history(&mut self) {
        self.history.clear();
        self.current_transaction = None;
        self.pending_highlight_edits.clear();
    }

    pub(super) fn record_insert_highlight_edit(&mut self, index: usize, text: &str) {
        if text.is_empty() {
            return;
        }

        let insert_at = index.min(self.text.len_chars());
        let start_byte = self.text.char_to_byte(insert_at);
        self.pending_highlight_edits
            .push(HighlightEdit::insert(start_byte, text.len()));
    }

    pub(super) fn record_delete_highlight_edit(&mut self, index: usize, len_chars: usize) {
        if len_chars == 0 || index >= self.text.len_chars() {
            return;
        }

        let start_char = index.min(self.text.len_chars());
        let end_char = (start_char + len_chars).min(self.text.len_chars());
        if start_char >= end_char {
            return;
        }

        let start_byte = self.text.char_to_byte(start_char);
        let end_byte = self.text.char_to_byte(end_char);
        self.pending_highlight_edits
            .push(HighlightEdit::delete(start_byte, end_byte));
    }

    pub(super) fn sync_file_picker_cache(&mut self) {
        if !self.is_file_picker_open() {
            self.file_picker_results_cache.clear();
            return;
        }

        self.file_picker_results_cache = self
            .command_palette
            .results
            .iter()
            .filter_map(|item| match &item.action {
                CommandPaletteAction::OpenFile(path) => Some(FilePickerEntry {
                    absolute_path: path.clone(),
                    relative_path: item.label.clone(),
                    score: 0,
                }),
                _ => None,
            })
            .collect();
    }

    pub(super) fn set_search_query_internal(&mut self, query: &str, whole_word: bool) -> bool {
        let query_changed = self.last_search_query != query;
        let whole_word_changed = self.search_whole_word != whole_word;
        self.last_search_query = query.to_string();
        self.search_whole_word = whole_word;
        let highlights_changed = self.refresh_active_search_highlights();
        query_changed || whole_word_changed || highlights_changed
    }

    pub fn search_case_sensitive(&self) -> bool {
        self.search_case_sensitive
    }

    pub fn toggle_search_case_sensitive(&mut self) -> bool {
        self.search_case_sensitive = !self.search_case_sensitive;
        let _ = self.refresh_active_search_highlights();
        true
    }

    pub(super) fn refresh_active_search_highlights(&mut self) -> bool {
        let next = if self.last_search_query.is_empty() {
            Vec::new()
        } else {
            let text = self.text.to_string();
            collect_search_highlights(
                &text,
                &self.last_search_query,
                self.search_whole_word,
                self.search_case_sensitive,
            )
        };

        if self.search_highlights == next {
            return false;
        }

        self.search_highlights = next;
        true
    }

    pub(super) fn jump_to_search_match(&mut self, forward: bool) -> bool {
        if self.search_highlights.is_empty() {
            return false;
        }

        let cursor_byte = self.cursor_byte_idx();
        let target = if forward {
            self.search_highlights
                .iter()
                .copied()
                .find(|(start, _)| *start > cursor_byte)
                .or_else(|| self.search_highlights.first().copied())
        } else {
            self.search_highlights
                .iter()
                .copied()
                .rev()
                .find(|(_, end)| *end <= cursor_byte)
                .or_else(|| self.search_highlights.last().copied())
        };

        let Some((start_byte, _)) = target else {
            return false;
        };
        self.move_cursor_to_char_idx(self.byte_to_char_idx(start_byte))
    }

    pub(super) fn move_cursor_to_char_idx(&mut self, char_idx: usize) -> bool {
        let changed = self.update_cursor_position(char_idx);
        let (_, col) = self.cursor_line_col();
        let target_changed = self.target_col != col;
        self.target_col = col;
        changed || target_changed
    }

    pub(super) fn char_at_cursor(&self) -> Option<char> {
        (self.cursor_char_idx < self.text.len_chars()).then(|| self.text.char(self.cursor_char_idx))
    }

    pub fn char_before_cursor(&self) -> Option<char> {
        (self.cursor_char_idx > 0).then(|| self.text.char(self.cursor_char_idx - 1))
    }

    /// Returns the char at (line, col) in the rope, or None if out of bounds.
    pub fn char_at_line_col(&self, line: usize, col: usize) -> Option<char> {
        if line >= self.text.len_lines() {
            return None;
        }
        let line_start = self.text.line_to_char(line);
        let line_len = self.text.line(line).len_chars();
        let newline_adj = if line_len > 0 && self.text.char(line_start + line_len - 1) == '\n' {
            1
        } else {
            0
        };
        if col >= line_len.saturating_sub(newline_adj) {
            return None;
        }
        Some(self.text.char(line_start + col))
    }

    pub(super) fn line_indent_string(&self, line_idx: usize) -> String {
        if self.text.len_lines() == 0 {
            return String::new();
        }

        let clamped_line = line_idx.min(self.text.len_lines().saturating_sub(1));
        let line_text = self.text.line(clamped_line).to_string();
        let line_content = line_text.strip_suffix('\n').unwrap_or(&line_text);
        line_content
            .chars()
            .take_while(|ch| matches!(ch, ' ' | '\t'))
            .collect()
    }

    /// True if the given line's last non-blank character opens a block
    /// (`{`, `(`, `[`). Used by `o`/`O` to add one extra indent level, mirroring
    /// Vim's smartindent.
    pub(super) fn line_opens_block(&self, line_idx: usize) -> bool {
        if self.text.len_lines() == 0 {
            return false;
        }
        let clamped_line = line_idx.min(self.text.len_lines().saturating_sub(1));
        let line_text = self.text.line(clamped_line).to_string();
        matches!(
            line_text.trim_end().chars().next_back(),
            Some('{' | '(' | '[')
        )
    }

    /// Indent string of the first non-blank line that follows `line_idx` and is
    /// nested deeper than it — i.e. the actual indentation of the block this line
    /// opens. Lets `o` follow the file's real indent step (e.g. 2 spaces) instead
    /// of guessing from config. Returns None when there is no deeper body line to
    /// sample (e.g. an empty block).
    pub(super) fn block_body_indent(&self, line_idx: usize) -> Option<String> {
        let total = self.text.len_lines();
        let opener_indent = self.line_indent_string(line_idx);
        for next in (line_idx + 1)..total {
            let line_text = self.text.line(next).to_string();
            if line_text.trim().is_empty() {
                continue; // skip blank lines inside the block
            }
            let body_indent = self.line_indent_string(next);
            if body_indent.len() > opener_indent.len() && body_indent.starts_with(&opener_indent) {
                return Some(body_indent);
            }
            return None; // first real line isn't nested deeper → nothing to sample
        }
        None
    }

    pub(super) fn indent_unit_for_line(&self, current_indent: &str) -> String {
        if current_indent.contains('\t') || !self.indent_config.insert_spaces {
            "\t".to_string()
        } else {
            " ".repeat(self.indent_config.tab_width as usize)
        }
    }

    pub(super) fn word_under_cursor(&self) -> Option<String> {
        let len_chars = self.text.len_chars();
        if len_chars == 0 {
            return None;
        }

        let focus = self.cursor_char_idx.min(len_chars.saturating_sub(1));
        if classify_char(self.text.char(focus)) != WordClass::Word {
            return None;
        }

        let mut start = focus;
        while start > 0 && classify_char(self.text.char(start - 1)) == WordClass::Word {
            start -= 1;
        }

        let mut end = focus + 1;
        while end < len_chars && classify_char(self.text.char(end)) == WordClass::Word {
            end += 1;
        }

        self.char_range_text(start, end)
    }

    pub(super) fn active_comment_syntax(&self) -> Option<CommentSyntax> {
        self.active_file
            .as_deref()
            .or(Some(self.default_save_path.as_path()))
            .and_then(active_comment_syntax_for_path)
    }

    pub(super) fn toggle_comments_on_lines(&mut self, start_line: usize, end_line: usize) -> bool {
        let Some(syntax) = self.active_comment_syntax() else {
            return false;
        };
        let Some(line_prefix) = syntax.line_prefix else {
            return false;
        };
        if self.text.len_lines() == 0 {
            return false;
        }

        let last_line = self.text.len_lines().saturating_sub(1);
        let start_line = start_line.min(last_line);
        let end_line = end_line.min(last_line);
        let plans: Vec<LineCommentPlan> = (start_line..=end_line)
            .map(|line_idx| line_comment_plan(&self.text, line_idx, line_prefix))
            .collect();
        let should_uncomment =
            !plans.is_empty() && plans.iter().all(|plan| plan.removal_len_chars.is_some());

        let edits: Vec<CommentEdit> = if should_uncomment {
            plans
                .into_iter()
                .filter_map(|plan| {
                    plan.removal_len_chars.map(|len_chars| CommentEdit::Delete {
                        at: plan.edit_char_idx,
                        len_chars,
                    })
                })
                .collect()
        } else {
            let insert_text = format!("{} ", line_prefix);
            plans
                .into_iter()
                .map(|plan| CommentEdit::Insert {
                    at: plan.edit_char_idx,
                    text: insert_text.clone(),
                })
                .collect()
        };

        if edits.is_empty() {
            return false;
        }

        let mut cursor = self.cursor_char_idx.min(self.text.len_chars());
        let mut offset: isize = 0;
        let mut changed = false;

        for edit in edits {
            match edit {
                CommentEdit::Insert { at, text } => {
                    let current_at = shift_char_position(at, offset).min(self.text.len_chars());
                    let inserted_chars = text.chars().count();
                    if self.apply_insert(current_at, text) {
                        cursor = adjust_cursor_after_insert(cursor, current_at, inserted_chars);
                        offset += inserted_chars as isize;
                        changed = true;
                    }
                }
                CommentEdit::Delete { at, len_chars } => {
                    let current_at = shift_char_position(at, offset).min(self.text.len_chars());
                    if self.apply_delete(current_at, len_chars) {
                        cursor = adjust_cursor_after_delete(cursor, current_at, len_chars);
                        offset -= len_chars as isize;
                        changed = true;
                    }
                }
            }
        }

        if !changed {
            return false;
        }

        self.dirty = true;
        let moved = self.move_cursor_to_char_idx(cursor.min(self.text.len_chars()));
        if !moved {
            self.bump_revision();
        }
        true
    }

    pub(super) fn toggle_block_comment_on_selection(&mut self) -> bool {
        let Some(syntax) = self.active_comment_syntax() else {
            return false;
        };
        let (Some(block_open), Some(block_close)) = (syntax.block_open, syntax.block_close) else {
            return false;
        };
        let Some(selection) = self.visual_selection_range() else {
            return false;
        };
        if self.text.len_lines() == 0 {
            return false;
        }

        let last_line_idx = self.text.len_lines().saturating_sub(1);
        let start_line = selection.start_line.min(last_line_idx);
        let end_line = selection.end_line.min(last_line_idx);
        if start_line > end_line {
            return false;
        }

        let first_line_text = self.text.line(start_line).to_string();
        let last_line_text = self.text.line(end_line).to_string();
        let first_trimmed = first_line_text
            .strip_suffix('\n')
            .unwrap_or(&first_line_text);
        let last_trimmed = last_line_text.strip_suffix('\n').unwrap_or(&last_line_text);

        let already_wrapped = first_trimmed.trim_start().starts_with(block_open)
            && last_trimmed.trim_end().ends_with(block_close);

        let mut cursor = self.cursor_char_idx.min(self.text.len_chars());

        if already_wrapped {
            let first_line_start = self.text.line_to_char(start_line);
            let open_pos = first_trimmed.find(block_open).unwrap_or(0);
            let open_at = first_line_start + open_pos;

            let mut open_len = block_open.len();
            let suffix = &first_trimmed[open_pos + block_open.len()..];
            if suffix.starts_with(' ') {
                open_len += 1;
            }

            let last_line_start = self.text.line_to_char(end_line);
            let close_pos = last_trimmed
                .rfind(block_close)
                .unwrap_or(last_trimmed.len().saturating_sub(block_close.len()));

            let mut close_at = last_line_start + close_pos;
            let mut close_len = block_close.len();
            if close_pos > 0
                && last_trimmed
                    .chars()
                    .nth(close_pos - 1)
                    .is_some_and(|c| c == ' ')
            {
                close_at -= 1;
                close_len += 1;
            }

            if close_at > open_at {
                if self.apply_delete(close_at, close_len) {
                    cursor = adjust_cursor_after_delete(cursor, close_at, close_len);
                }
                if self.apply_delete(open_at, open_len) {
                    cursor = adjust_cursor_after_delete(cursor, open_at, open_len);
                }
            } else {
                if self.apply_delete(open_at, open_len) {
                    cursor = adjust_cursor_after_delete(cursor, open_at, open_len);
                    close_at = shift_char_position(close_at, -(open_len as isize));
                }
                if self.apply_delete(close_at.min(self.text.len_chars()), close_len) {
                    cursor = adjust_cursor_after_delete(
                        cursor,
                        close_at.min(self.text.len_chars()),
                        close_len,
                    );
                }
            }
        } else {
            let first_line_start = self.text.line_to_char(start_line);
            let last_content_end = self.text.line_to_char(end_line) + last_trimmed.len();

            let open_text = format!("{} ", block_open);
            let open_len = open_text.chars().count();
            if self.apply_insert(first_line_start, open_text) {
                cursor = adjust_cursor_after_insert(cursor, first_line_start, open_len);
            }

            let close_at = shift_char_position(last_content_end, open_len as isize);
            let close_text = format!(" {}", block_close);
            let close_len = close_text.chars().count();
            if self.apply_insert(close_at.min(self.text.len_chars()), close_text) {
                cursor = adjust_cursor_after_insert(
                    cursor,
                    close_at.min(self.text.len_chars()),
                    close_len,
                );
            }
        }

        self.dirty = true;
        let moved = self.move_cursor_to_char_idx(cursor.min(self.text.len_chars()));
        if !moved {
            self.bump_revision();
        }
        true
    }

    pub(super) fn max_col_for_line(&self, line_idx: usize) -> usize {
        let line = self.text.line(line_idx);
        let len_chars = line.len_chars();
        if len_chars == 0 {
            return 0;
        }

        // Rope line thường chứa '\n' ở cuối (trừ dòng cuối của file).
        // Cursor nên dừng ở "cuối nội dung dòng", không đứng sau '\n'.
        if line.char(len_chars - 1) == '\n' {
            len_chars - 1
        } else {
            // Dòng không có '\n' => cho phép caret đi tới vị trí sau ký tự cuối.
            len_chars
        }
    }

    pub(super) fn line_content_end_char_idx(&self, line_idx: usize) -> usize {
        let clamped_line = line_idx.min(self.text.len_lines().saturating_sub(1));
        let line_start = self.text.line_to_char(clamped_line);
        line_start + self.max_col_for_line(clamped_line)
    }

    /// Unified cursor update path used by all motion commands.
    ///
    /// - In `Visual` mode: keep `selection_anchor_char_idx` untouched and move
    ///   cursor/focus only.
    /// - In non-visual modes: clear stale selection anchor while moving.
    pub(super) fn update_cursor_position(&mut self, new_index: usize) -> bool {
        let clamped = new_index.min(self.text.len_chars());
        let mut changed = false;

        if clamped != self.cursor_char_idx {
            self.cursor_char_idx = clamped;
            changed = true;
        }

        if self.current_mode() != EditorMode::Visual
            && self.selection_anchor_char_idx.take().is_some()
        {
            changed = true;
        }

        changed |= self.refresh_matched_bracket_without_revision();

        if changed {
            self.bump_revision();
        }
        changed
    }

    pub(super) fn bump_revision(&mut self) {
        self.revision += 1;
    }

    pub(super) fn load_buffer_from_file(&mut self, canonical_path: &Path) -> Result<(), String> {
        ensure_interactive_text_file_size(canonical_path)?;
        let bytes = fs::read(canonical_path)
            .map_err(|err| format!("read text file {:?} failed: {err}", canonical_path))?;
        let content = String::from_utf8(bytes).map_err(|err| {
            format!(
                "text file {:?} is not valid UTF-8 (invalid byte at offset {})",
                canonical_path,
                err.utf8_error().valid_up_to()
            )
        })?;
        let modified_time = std::fs::metadata(canonical_path)
            .and_then(|m| m.modified())
            .ok();
        self.replace_text_buffer_preserving_view(content.as_str());
        let _ = self.refresh_active_search_highlights();
        if let Some(active_idx) = self.active_buffer_index
            && let Some(slot) = self.buffers.get_mut(active_idx)
            && let BufferContent::Text(ref mut buffer) = slot.content
        {
            buffer.last_known_modified_time = modified_time;
        }
        Ok(())
    }

    pub(super) fn load_buffer_from_file_resetting_view(
        &mut self,
        canonical_path: &Path,
    ) -> Result<(), String> {
        ensure_interactive_text_file_size(canonical_path)?;
        let content = fs::read_to_string(canonical_path)
            .map_err(|err| format!("open UTF-8 text file {:?} failed: {err}", canonical_path))?;
        let modified_time = std::fs::metadata(canonical_path)
            .and_then(|m| m.modified())
            .ok();
        self.text = Rope::from(content.as_str());
        self.cached_line_starts = None;
        self.cursor_char_idx = 0;
        self.target_col = 0;
        self.target_scroll_y = 0.0;
        self.current_scroll_y = 0.0;
        self.scroll_column = 0;
        self.selection_anchor_char_idx = None;
        self.visual_line_mode = false;
        self.clear_history();
        let _ = self.refresh_active_search_highlights();
        if let Some(active_idx) = self.active_buffer_index
            && let Some(slot) = self.buffers.get_mut(active_idx)
            && let BufferContent::Text(ref mut buffer) = slot.content
        {
            buffer.last_known_modified_time = modified_time;
        }
        Ok(())
    }

    pub(super) fn replace_text_buffer_preserving_view(&mut self, content: &str) {
        let old_cursor = self.cursor_char_idx;
        let old_selection_anchor = self.selection_anchor_char_idx;
        let old_target_scroll_y = self.target_scroll_y;
        let old_current_scroll_y = self.current_scroll_y;
        let old_scroll_column = self.scroll_column;
        let old_visual_line_mode = self.visual_line_mode;

        self.text = Rope::from(content);
        self.cached_line_starts = None;
        self.revision += 1;

        let max_char_idx = self.text.len_chars();
        self.cursor_char_idx = old_cursor.min(max_char_idx);
        self.selection_anchor_char_idx =
            old_selection_anchor.map(|anchor| anchor.min(max_char_idx));

        if self.selection_anchor_char_idx == Some(self.cursor_char_idx) {
            self.selection_anchor_char_idx = None;
        }

        let (_, clamped_col) = self.cursor_line_col();
        self.target_col = clamped_col;
        let max_scroll = self.text.len_lines().saturating_sub(1) as f32;
        self.target_scroll_y = old_target_scroll_y.min(max_scroll);
        self.current_scroll_y = old_current_scroll_y.min(max_scroll);
        self.scroll_column = old_scroll_column;
        self.visual_line_mode = old_visual_line_mode && self.selection_anchor_char_idx.is_some();
        self.clear_history();
    }

    pub(super) fn register_open_text_buffer(&mut self, active_path: PathBuf) {
        if let Some(active_idx) = self.active_buffer_index
            && let Some(slot) = self.buffers.get(active_idx)
            && matches!(&slot.content, BufferContent::Text(buffer) if buffer.path == active_path)
        {
            return;
        }

        // Save current text buffer before potentially switching to a different buffer
        self.save_current_text_buffer_history();

        let language_id = crate::lsp::registry::language_profile_for_path(&active_path)
            .map(|profile| profile.language_id.to_string());
        if let Some(existing_idx) = self
            .buffers
            .iter()
            .position(|buffer| matches!(&buffer.content, BufferContent::Text(buffer) if buffer.path == active_path))
        {
            self.active_buffer_index = Some(existing_idx);
            return;
        }

        if let Some(cached) = self.closed_text_buffers.remove(&active_path) {
            self.buffers.push(BufferEntry {
                content: BufferContent::Text(cached),
            });
            self.active_buffer_index = Some(self.buffers.len().saturating_sub(1));
            return;
        }

        self.buffers.push(BufferEntry {
            content: BufferContent::Text(EditorBuffer::new(active_path, language_id)),
        });
        self.active_buffer_index = Some(self.buffers.len().saturating_sub(1));
    }

    pub(super) fn cycle_buffer(&mut self, forward: bool) -> Result<bool, String> {
        if self.buffers.is_empty() {
            return Ok(false);
        }

        let current_idx = self
            .active_buffer_index
            .filter(|idx| *idx < self.buffers.len());

        let next_idx = match current_idx {
            Some(idx) if forward => (idx + 1) % self.buffers.len(),
            Some(idx) => {
                if idx == 0 {
                    self.buffers.len() - 1
                } else {
                    idx - 1
                }
            }
            None if forward => 0,
            None => self.buffers.len() - 1,
        };

        if current_idx == Some(next_idx) {
            return Ok(false);
        }

        let mut candidate_idx = next_idx;
        let mut attempts = self.buffers.len();
        while attempts > 0 && !self.buffers.is_empty() {
            attempts -= 1;
            match self.activate_buffer_index(candidate_idx) {
                Ok(()) => return Ok(true),
                Err(_) => {
                    self.buffers.remove(candidate_idx);
                    if self.buffers.is_empty() {
                        return Ok(self.new_empty_buffer());
                    }
                    if candidate_idx >= self.buffers.len() {
                        candidate_idx = 0;
                    }
                }
            }
        }

        Ok(false)
    }

    pub fn activate_buffer_index(&mut self, index: usize) -> Result<(), String> {
        let Some(buffer) = self.buffers.get(index).cloned() else {
            return Err(format!("buffer index {index} out of range"));
        };
        let active_buffer_changed = self.active_buffer_index != Some(index);

        match buffer.content {
            BufferContent::Text(buffer) => {
                if self.active_buffer_index == Some(index) {
                    self.active_file = Some(buffer.path.clone());
                    self.external_conflict = None;
                    let _ = self.workspace_expand_to_path(&buffer.path);
                    self.bump_revision();
                    return Ok(());
                }
                if self.active_buffer_index != Some(index) {
                    self.save_current_text_buffer_history();
                }
                // CRITICAL: the old buffer's state is saved; clear the index so
                // nothing below (e.g. the mtime stamp inside
                // `load_buffer_from_file_resetting_view`) writes into the OLD slot
                // while `self.text` is mid-switch.
                self.active_buffer_index = None;

                let mut restored_view_state = TextBufferViewState::default();
                let mut restored_history = EditHistory::new();
                let mut restored_in_memory_text: Option<Rope> = None;
                let mut restored_dirty = false;
                let mut restored_mtime: Option<std::time::SystemTime> = None;
                if let Some(slot) = self.buffers.get(index) {
                    if let BufferContent::Text(ref new_buf) = slot.content {
                        restored_history = new_buf.history.clone();
                        restored_in_memory_text = new_buf.in_memory_text.clone();
                        restored_dirty = new_buf.dirty;
                        restored_view_state = new_buf.view_state;
                        restored_mtime = new_buf.last_known_modified_time;
                    }
                }
                // A clean snapshot is only trustworthy while the file on disk is
                // unchanged: the fs watcher never covers closed tabs, so a file
                // rewritten externally (git checkout, AI agent) while its tab was
                // closed would otherwise show the stale cache forever. Dirty
                // snapshots always win — losing unsaved edits is worse than stale.
                let disk_mtime = std::fs::metadata(&buffer.path)
                    .and_then(|m| m.modified())
                    .ok();
                // mtime is a cheap FIRST check. When it differs, only treat the
                // cached snapshot as stale if the on-disk CONTENT actually changed —
                // otherwise a benign mtime bump (format-on-save, `touch`, git status)
                // would needlessly throw away the user's undo history on reopen. A
                // real external rewrite still drops it (old edit positions no longer
                // apply to the new content).
                let mtime_changed = disk_mtime.is_some() && disk_mtime != restored_mtime;
                let snapshot_stale = !restored_dirty
                    && mtime_changed
                    && restored_in_memory_text
                        .as_ref()
                        .map(|snap| !rope_matches_disk(&buffer.path, snap))
                        .unwrap_or(true);
                let mut loaded_from_disk = false;
                if let Some(snapshot) = restored_in_memory_text.filter(|_| !snapshot_stale) {
                    self.text = snapshot;
                    self.cached_line_starts = None;
                    let _ = self.refresh_active_search_highlights();
                } else {
                    // First open in this session, or stale snapshot: disk baseline.
                    self.load_buffer_from_file_resetting_view(&buffer.path)?;
                    loaded_from_disk = true;
                    if snapshot_stale {
                        // The old text's undo history and cursor can't apply to
                        // the reloaded content — drop them with the snapshot.
                        restored_history = EditHistory::new();
                        restored_view_state = TextBufferViewState::default();
                    }
                }
                self.history = restored_history;
                self.restore_text_buffer_view_state(restored_view_state);
                self.active_file = Some(buffer.path.clone());
                self.active_buffer_index = Some(index);
                self.dirty = restored_dirty;
                if let Some(slot) = self.buffers.get_mut(index)
                    && let BufferContent::Text(ref mut live_buf) = slot.content
                {
                    if loaded_from_disk {
                        live_buf.in_memory_text = Some(self.text.clone());
                        live_buf.history = self.history.clone();
                        live_buf.last_known_modified_time = disk_mtime;
                    } else {
                        if live_buf.in_memory_text.is_none() {
                            live_buf.in_memory_text = Some(self.text.clone());
                        }
                        // Kept the cache across a benign mtime change (content still
                        // matched): adopt the new mtime so we don't re-read the file
                        // on every future activation.
                        if mtime_changed {
                            live_buf.last_known_modified_time = disk_mtime;
                        }
                    }
                    live_buf.dirty = self.dirty;
                }
                self.external_conflict = None;
                let _ = self.workspace_expand_to_path(&buffer.path);
            }
            BufferContent::Image(buffer) => {
                self.save_current_text_buffer_history();
                self.reset_text_editor_state();
                let refreshed = load_image_buffer(&buffer.path);
                if let Some(slot) = self.buffers.get_mut(index) {
                    slot.content = BufferContent::Image(refreshed);
                }
                self.active_buffer_index = Some(index);
                let _ = self.workspace_expand_to_path(&buffer.path);
            }
            BufferContent::Terminal(_) => {
                self.save_current_text_buffer_history();
                self.active_file = None;
                self.active_buffer_index = Some(index);
                self.selection_anchor_char_idx = None;
                self.visual_line_mode = false;
                self.external_conflict = None;
            }
            BufferContent::References(_)
            | BufferContent::Diagnostics(_)
            | BufferContent::MarkdownPreview(_)
            | BufferContent::FuzzyPicker(_)
            | BufferContent::SettingsTab(_)
            | BufferContent::Help(_)
            | BufferContent::ExtensionsManager(_) => {
                self.save_current_text_buffer_history();
                self.reset_text_editor_state();
                self.active_buffer_index = Some(index);
                let _ = self.clear_current_overlays();
            }
        }

        if active_buffer_changed {
            let _ = self.clear_semantic_symbol_highlights();
        }
        self.bump_revision();
        Ok(())
    }

    /// Save the live history and view state into the active EditorBuffer.
    /// Must be called before switching away from any text buffer.
    pub(super) fn save_current_text_buffer_history(&mut self) {
        let _ = self.commit_transaction();
        if let Some(old_idx) = self.active_buffer_index {
            // CRITICAL: Validate that old_idx is still in bounds before saving.
            // This prevents race condition where buffer was removed but active_buffer_index
            // hasn't been updated yet, which would cause content corruption.
            if old_idx >= self.buffers.len() {
                return;
            }

            let saved = std::mem::take(&mut self.history);
            let saved_view_state = self.text_buffer_view_state();
            if let Some(slot) = self.buffers.get_mut(old_idx) {
                if let BufferContent::Text(ref mut buf) = slot.content {
                    // Only save if we have actual content or history to preserve.
                    // This prevents overwriting buffer state with empty data on double-save.
                    buf.history = saved;
                    buf.view_state = saved_view_state;
                    buf.in_memory_text = Some(self.text.clone());
                    buf.dirty = self.dirty;
                }
            }
        }
    }

    pub(super) fn reset_text_editor_state(&mut self) {
        self.text = Rope::new();
        self.cached_line_starts = None;
        self.cursor_char_idx = 0;
        self.target_col = 0;
        self.target_scroll_y = 0.0;
        self.current_scroll_y = 0.0;
        self.scroll_column = 0;
        self.active_file = None;
        self.selection_anchor_char_idx = None;
        self.dirty = false;
        self.external_conflict = None;
        self.visual_line_mode = false;
        let _ = self.clear_semantic_symbol_highlights();
        self.clear_history();
        let _ = self.refresh_active_search_highlights();
    }
}

/// Whether the file at `path` currently holds exactly the bytes in `rope`. Used
/// to decide whether a cached buffer snapshot is still valid when only the mtime
/// moved. A read error counts as "changed" so the caller reloads from disk (the
/// safe default).
fn rope_matches_disk(path: &Path, rope: &Rope) -> bool {
    match std::fs::read(path) {
        Ok(bytes) => rope.to_string().as_bytes() == bytes.as_slice(),
        Err(_) => false,
    }
}

fn merge_completion_item(
    existing: &crate::async_runtime::message::LspCompletionItem,
    resolved: &crate::async_runtime::message::LspCompletionItem,
) -> crate::async_runtime::message::LspCompletionItem {
    let mut merged = existing.clone();
    if resolved.detail.is_some() {
        merged.detail = resolved.detail.clone();
    }
    if resolved.insert_text.is_some() {
        merged.insert_text = resolved.insert_text.clone();
    }
    if resolved.text_edit.is_some() {
        merged.text_edit = resolved.text_edit.clone();
    }
    if resolved.text_edit_text.is_some() {
        merged.text_edit_text = resolved.text_edit_text.clone();
    }
    if !resolved.additional_text_edits.is_empty() {
        merged.additional_text_edits = resolved.additional_text_edits.clone();
    }
    if resolved.kind.is_some() {
        merged.kind = resolved.kind;
    }
    if resolved.callable.is_some() {
        merged.callable = resolved.callable;
    }
    if resolved.has_parameters.is_some() {
        merged.has_parameters = resolved.has_parameters;
    }
    if resolved.documentation.is_some() {
        merged.documentation = resolved.documentation.clone();
    }
    if resolved.data.is_some() {
        merged.data = resolved.data.clone();
    }
    if resolved.raw_json.is_some() {
        merged.raw_json = resolved.raw_json.clone();
    }
    merged
}

#[derive(Debug, Clone, Copy)]
pub(super) struct CommentSyntax {
    line_prefix: Option<&'static str>,
    block_open: Option<&'static str>,
    block_close: Option<&'static str>,
}

#[derive(Debug, Clone, Copy)]
pub(super) struct LineCommentPlan {
    edit_char_idx: usize,
    removal_len_chars: Option<usize>,
}

#[derive(Debug, Clone)]
enum CommentEdit {
    Insert { at: usize, text: String },
    Delete { at: usize, len_chars: usize },
}

pub(super) fn active_comment_syntax_for_path(path: &Path) -> Option<CommentSyntax> {
    let file_name_lower = path
        .file_name()
        .and_then(|name| name.to_str())
        .map(|s| s.to_ascii_lowercase());

    let is_makefile = file_name_lower.as_deref().is_some_and(|n| n == "makefile");
    let is_envfile = file_name_lower.as_deref().map_or(false, |n| {
        n.starts_with('.')
            || n == "env"
            || n.starts_with("env.")
            || n == "dockerfile"
            || n == "vagrantfile"
            || n == "gemfile"
            || n == "rakefile"
            || n.ends_with("rc")
    });

    if is_makefile || is_envfile {
        return Some(CommentSyntax {
            line_prefix: Some("#"),
            block_open: None,
            block_close: None,
        });
    }

    let extension = path
        .extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| ext.to_ascii_lowercase());

    match extension.as_deref() {
        Some(
            "rs" | "go" | "js" | "jsx" | "ts" | "tsx" | "c" | "cc" | "cpp" | "cxx" | "h" | "hpp"
            | "hxx" | "java" | "kt" | "kts" | "swift" | "cs" | "dart" | "scala" | "scss" | "less"
            | "proto" | "php" | "m" | "mm" | "groovy" | "zig",
        ) => Some(CommentSyntax {
            line_prefix: Some("//"),
            block_open: Some("/*"),
            block_close: Some("*/"),
        }),
        Some("rsx" | "vue" | "svelte" | "astro") => Some(CommentSyntax {
            line_prefix: Some("//"),
            block_open: Some("/*"),
            block_close: Some("*/"),
        }),
        Some(
            "py" | "sh" | "bash" | "zsh" | "fish" | "rb" | "pl" | "pm" | "r" | "R" | "yml" | "yaml"
            | "toml" | "ini" | "cfg" | "conf" | "properties" | "gitignore" | "dockerignore" | "txt"
            | "text" | "env" | "dist",
        ) => Some(CommentSyntax {
            line_prefix: Some("#"),
            block_open: None,
            block_close: None,
        }),
        Some("lua") => Some(CommentSyntax {
            line_prefix: Some("--"),
            block_open: Some("--[["),
            block_close: Some("]]"),
        }),
        Some("hs") => Some(CommentSyntax {
            line_prefix: Some("--"),
            block_open: Some("{-"),
            block_close: Some("-}"),
        }),
        Some("sql") => Some(CommentSyntax {
            line_prefix: Some("--"),
            block_open: Some("/*"),
            block_close: Some("*/"),
        }),
        Some("html" | "htm" | "xml" | "svg" | "mdx") => Some(CommentSyntax {
            line_prefix: Some("<!--"),
            block_open: Some("<!--"),
            block_close: Some("-->"),
        }),
        Some("css") => Some(CommentSyntax {
            line_prefix: None,
            block_open: Some("/*"),
            block_close: Some("*/"),
        }),
        Some(
            "md" | "markdown" | "rst" | "tex" | "bib" | "json" | "jsonc" | "csv" | "tsv" | "log",
        ) => Some(CommentSyntax {
            line_prefix: None,
            block_open: None,
            block_close: None,
        }),
        Some("tf" | "tfvars" | "hcl") => Some(CommentSyntax {
            line_prefix: Some("#"),
            block_open: Some("/*"),
            block_close: Some("*/"),
        }),
        Some("elm") => Some(CommentSyntax {
            line_prefix: Some("--"),
            block_open: Some("{-"),
            block_close: Some("-}"),
        }),
        Some("erl" | "hrl") => Some(CommentSyntax {
            line_prefix: Some("%"),
            block_open: None,
            block_close: None,
        }),
        Some("ex" | "exs" | "heex") => Some(CommentSyntax {
            line_prefix: Some("#"),
            block_open: None,
            block_close: None,
        }),
        Some("nim") => Some(CommentSyntax {
            line_prefix: Some("#"),
            block_open: None,
            block_close: None,
        }),
        Some("wat") => Some(CommentSyntax {
            line_prefix: Some(";;"),
            block_open: None,
            block_close: None,
        }),
        Some("clj" | "cljs" | "cljc" | "edn") => Some(CommentSyntax {
            line_prefix: Some(";;"),
            block_open: None,
            block_close: None,
        }),
        Some("coffee" | "litcoffee") => Some(CommentSyntax {
            line_prefix: Some("#"),
            block_open: None,
            block_close: None,
        }),
        _ => Some(CommentSyntax {
            line_prefix: Some("#"),
            block_open: None,
            block_close: None,
        }),
    }
}

pub(super) fn line_comment_plan(
    text: &Rope,
    line_idx: usize,
    line_prefix: &str,
) -> LineCommentPlan {
    let clamped_line = line_idx.min(text.len_lines().saturating_sub(1));
    let line_start = text.line_to_char(clamped_line);
    let line_text = text.line(clamped_line).to_string();
    let line_content = line_text.strip_suffix('\n').unwrap_or(&line_text);
    let indent_byte_idx = line_content
        .char_indices()
        .find(|(_, ch)| !ch.is_whitespace())
        .map(|(idx, _)| idx)
        .unwrap_or(line_content.len());
    let indent_chars = line_content[..indent_byte_idx].chars().count();
    let rest = &line_content[indent_byte_idx..];

    LineCommentPlan {
        edit_char_idx: line_start + indent_chars,
        removal_len_chars: line_comment_removal_len(rest, line_prefix),
    }
}

pub(super) fn line_comment_removal_len(rest: &str, line_prefix: &str) -> Option<usize> {
    if !rest.starts_with(line_prefix) {
        return None;
    }

    let after_prefix = &rest[line_prefix.len()..];
    if line_prefix == "//" && (after_prefix.starts_with('/') || after_prefix.starts_with('!')) {
        return None;
    }
    if line_prefix == "#" && after_prefix.starts_with('!') {
        return None;
    }

    let mut len_chars = line_prefix.chars().count();
    if after_prefix
        .chars()
        .next()
        .is_some_and(|ch| ch.is_whitespace())
    {
        len_chars += 1;
    }
    Some(len_chars)
}

pub(super) fn matching_close_char(open: char) -> Option<char> {
    match open {
        '(' => Some(')'),
        '[' => Some(']'),
        '{' => Some('}'),
        '"' => Some('"'),
        '\'' => Some('\''),
        '`' => Some('`'),
        _ => None,
    }
}

pub(super) fn matches_matching_bracket_pair(left: Option<char>, right: Option<char>) -> bool {
    matches!(
        (left, right),
        (Some('('), Some(')'))
            | (Some('['), Some(']'))
            | (Some('{'), Some('}'))
            | (Some('"'), Some('"'))
            | (Some('\''), Some('\''))
            | (Some('`'), Some('`'))
    )
}

pub(super) fn shift_char_position(position: usize, delta: isize) -> usize {
    if delta.is_negative() {
        position.saturating_sub(delta.unsigned_abs())
    } else {
        position.saturating_add(delta as usize)
    }
}

pub(super) fn adjust_cursor_after_insert(
    cursor: usize,
    insert_at: usize,
    len_chars: usize,
) -> usize {
    if insert_at <= cursor {
        cursor.saturating_add(len_chars)
    } else {
        cursor
    }
}

pub(super) fn adjust_cursor_after_delete(
    cursor: usize,
    delete_at: usize,
    len_chars: usize,
) -> usize {
    if delete_at >= cursor {
        return cursor;
    }

    cursor.saturating_sub(len_chars.min(cursor.saturating_sub(delete_at)))
}

/// Vim word-class for `dw` boundary detection.
///   Word  = alphanumeric + `_`
///   Punct = non-whitespace, non-word
///   Space = space/tab (newline handled separately)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum WordClass {
    Word,
    Punct,
    Space,
    Newline,
}

pub(super) fn classify_char(ch: char) -> WordClass {
    if ch == '\n' {
        WordClass::Newline
    } else if ch.is_whitespace() {
        WordClass::Space
    } else if ch.is_alphanumeric() || ch == '_' {
        WordClass::Word
    } else {
        WordClass::Punct
    }
}

pub(super) fn is_completion_identifier_char(ch: char) -> bool {
    ch.is_alphanumeric() || matches!(ch, '_' | '$')
}

/// Returns the char index where vim's `dw` motion should stop, counting from
/// `cursor`. Crosses same-line whitespace after the current token so `dw` eats
/// one "word-like run" plus trailing spaces. Stops at newline.
pub(super) fn next_word_start(text: &Rope, cursor: usize) -> usize {
    let n = text.len_chars();
    if cursor >= n {
        return cursor;
    }

    let start_class = classify_char(text.char(cursor));

    // Cursor sitting on a newline: delete just that one newline (line join).
    if start_class == WordClass::Newline {
        return cursor + 1;
    }

    let mut i = cursor;

    // If cursor starts on whitespace, skip same-line whitespace only.
    if start_class == WordClass::Space {
        while i < n {
            let cls = classify_char(text.char(i));
            if cls != WordClass::Space {
                break;
            }
            i += 1;
        }
        return i;
    }

    // On Word or Punct: skip the current run of the same class, then skip
    // same-line trailing whitespace to land at start of next token.
    while i < n && classify_char(text.char(i)) == start_class {
        i += 1;
    }
    while i < n && classify_char(text.char(i)) == WordClass::Space {
        i += 1;
    }
    i
}

pub(super) fn previous_word_start(text: &Rope, cursor: usize) -> usize {
    if cursor == 0 {
        return 0;
    }

    let mut i = cursor.saturating_sub(1);
    while i > 0 {
        let cls = classify_char(text.char(i));
        if cls != WordClass::Space && cls != WordClass::Newline {
            break;
        }
        i -= 1;
    }

    if classify_char(text.char(i)) == WordClass::Space
        || classify_char(text.char(i)) == WordClass::Newline
    {
        return i;
    }

    let cls = classify_char(text.char(i));
    while i > 0 && classify_char(text.char(i - 1)) == cls {
        i -= 1;
    }
    i
}

pub(super) fn word_end_at_or_after(text: &Rope, cursor: usize) -> Option<usize> {
    let n = text.len_chars();
    if n == 0 || cursor >= n {
        return None;
    }

    let mut i = cursor;

    // If already at a word-end (non-space char whose next char is a different class),
    // step forward one so we land on the NEXT word (Vim `e` behavior).
    if i + 1 < n {
        let cur_cls = classify_char(text.char(i));
        let next_cls = classify_char(text.char(i + 1));
        if cur_cls != WordClass::Space && cur_cls != WordClass::Newline && next_cls != cur_cls {
            i += 1;
        }
    }

    while i < n {
        let cls = classify_char(text.char(i));
        if cls != WordClass::Space && cls != WordClass::Newline {
            break;
        }
        i += 1;
    }
    if i >= n {
        return None;
    }

    let cls = classify_char(text.char(i));
    while i + 1 < n && classify_char(text.char(i + 1)) == cls {
        i += 1;
    }
    Some(i)
}

pub(super) fn word_end_from_cursor(text: &Rope, cursor: usize) -> Option<usize> {
    let n = text.len_chars();
    if n == 0 || cursor >= n {
        return None;
    }

    let cls = classify_char(text.char(cursor));
    if cls == WordClass::Space || cls == WordClass::Newline {
        return None;
    }

    let mut i = cursor;
    while i + 1 < n && classify_char(text.char(i + 1)) == cls {
        i += 1;
    }
    Some(i)
}

pub(super) fn collect_search_highlights(
    text: &str,
    query: &str,
    whole_word: bool,
    case_sensitive: bool,
) -> Vec<(usize, usize)> {
    if query.is_empty() {
        return Vec::new();
    }

    if case_sensitive {
        return text
            .match_indices(query)
            .filter_map(|(start, matched)| {
                let end = start + matched.len();
                if whole_word && !is_whole_word_match(text, start, end) {
                    return None;
                }
                Some((start, end))
            })
            .collect();
    }

    let haystack = text.to_lowercase();
    let needle = query.to_lowercase();
    haystack
        .match_indices(&needle)
        .filter_map(|(start, matched)| {
            let end = start + matched.len();
            if whole_word && !is_whole_word_match(text, start, end) {
                return None;
            }
            Some((start, end))
        })
        .collect()
}

pub(super) fn build_completion_display_items(
    items: &[LspCompletionItem],
    prefix: &str,
) -> Vec<CompletionDisplayItem> {
    if prefix.is_empty() {
        return items
            .iter()
            .cloned()
            .map(|item| CompletionDisplayItem {
                item,
                match_ranges: Vec::new(),
                score: 0,
                source: CompletionItemSource::Lsp,
            })
            .collect();
    }

    let mut scored = items
        .iter()
        .enumerate()
        .filter_map(|(original_idx, item)| {
            score_completion_match(&item.label, prefix)
                .map(|(score, match_ranges)| (original_idx, item.clone(), score, match_ranges))
        })
        .collect::<Vec<_>>();

    scored.sort_by(|a, b| b.2.cmp(&a.2).then_with(|| a.0.cmp(&b.0)));

    scored
        .into_iter()
        .map(|(_, item, score, match_ranges)| CompletionDisplayItem {
            item,
            match_ranges,
            score,
            source: CompletionItemSource::Lsp,
        })
        .collect()
}

/// Upper bound on standalone workspace-symbol items blended into one completion
/// list, so a short prefix in a large workspace can't flood the popup.
const MAX_STANDALONE_WORKSPACE_COMPLETIONS: usize = 12;

/// Build completion display items with workspace symbol fallback.
/// If LSP returns fewer than `min_items` results, query the workspace symbol cache.
pub(super) fn build_completion_display_items_with_cache(
    items: &[LspCompletionItem],
    prefix: &str,
    cache: &crate::lsp::WorkspaceSymbolCache,
    language_id: Option<&str>,
    _min_items: usize,
) -> Vec<CompletionDisplayItem> {
    let mut lsp_items = build_completion_display_items(items, prefix);

    if prefix.is_empty() {
        return lsp_items;
    }

    // Always blend importable workspace/package symbols into the LSP list. Some
    // servers return many broad built-ins before auto-import candidates, so a
    // count-based fallback can hide the entries that are actually useful.
    let workspace_symbols = cache.query_symbols(prefix, language_id);

    // Convert workspace symbols to completion items
    let mut workspace_items: Vec<CompletionDisplayItem> = workspace_symbols
        .into_iter()
        .filter_map(|symbol| {
            if let Some(existing) = lsp_items
                .iter_mut()
                .find(|lsp_item| lsp_item.item.label == symbol.name)
            {
                if symbol.export_kind.is_some()
                    && existing.item.additional_text_edits.is_empty()
                    && existing.item.source_path.is_none()
                    && existing.item.export_kind.is_none()
                    // A resolvable LSP item (carries `data`/`raw_json`) computes its
                    // own correct auto-import via `completionItem/resolve`. Enriching
                    // it here sets `export_kind`/`source_path`, which suppresses
                    // `should_resolve_lsp_completion_before_accept` and forces a
                    // *guessed* import. Leave it untouched so the server wins.
                    && existing.item.raw_json.is_none()
                {
                    existing.item.source_path = symbol
                        .source_path
                        .clone()
                        .or_else(|| Some(symbol.file_path.clone()));
                    existing.item.import_path = symbol.import_path.clone();
                    existing.item.export_kind = symbol.export_kind.clone();
                    if existing.item.detail.is_none() {
                        existing.item.detail = workspace_symbol_completion_detail(&symbol);
                    }
                    if existing.item.callable.is_none() {
                        existing.item.callable = symbol.callable;
                    }
                    if existing.item.has_parameters.is_none() {
                        existing.item.has_parameters = symbol.has_parameters;
                    }
                    existing.source = CompletionItemSource::WorkspaceSymbol;
                }
                return None;
            }

            // Only surface a standalone cache item when accepting it produces
            // valid code. Two cases qualify:
            //   • TS/JS WITH export metadata → the editor synthesizes the import.
            //   • Go → a bare insert is resolved by goimports / same-package.
            // Everything else (TS/JS without export metadata; Java/Rust/Python,
            // whose cache symbols come from LSP `workspace/symbol` with no export
            // info and which the editor can't auto-import) would insert a bare,
            // unqualified name → an undefined reference. Drop those and let the
            // language server provide the proper auto-import completion instead.
            let is_ts_js_family = matches!(
                language_id,
                Some("typescript" | "tsx" | "javascript" | "jsx")
            );
            let bare_insert_safe = matches!(language_id, Some("go"));
            let import_synthesizable = is_ts_js_family && symbol.export_kind.is_some();
            if !bare_insert_safe && !import_synthesizable {
                return None;
            }

            // Score the symbol name against the prefix
            let (score, match_ranges) = score_completion_match(&symbol.name, prefix)?;

            // Convert to LspCompletionItem
            let detail = workspace_symbol_completion_detail(&symbol);

            Some(CompletionDisplayItem {
                item: LspCompletionItem {
                    label: symbol.name.clone(),
                    detail,
                    insert_text: Some(symbol.name),
                    text_edit: None,
                    text_edit_text: None,
                    additional_text_edits: Vec::new(),
                    kind: Some(symbol_kind_to_lsp_kind(&symbol.kind)),
                    callable: symbol.callable,
                    has_parameters: symbol.has_parameters,
                    documentation: None,
                    data: None,
                    source_path: symbol
                        .source_path
                        .clone()
                        .or_else(|| Some(symbol.file_path.clone())),
                    import_path: symbol.import_path.clone(),
                    export_kind: symbol.export_kind.clone(),
                    raw_json: None,
                },
                match_ranges,
                score,
                source: CompletionItemSource::WorkspaceSymbol,
            })
        })
        .collect();

    // Cap standalone workspace-symbol injections to the best few. A short prefix
    // in a large workspace can match dozens of exports; injecting all of them
    // buries the LSP's context-aware items under workspace-wide noise. Enriched
    // LSP items are unaffected — this only bounds the *extra* entries we add.
    if workspace_items.len() > MAX_STANDALONE_WORKSPACE_COMPLETIONS {
        workspace_items.sort_by(|a, b| {
            b.score
                .cmp(&a.score)
                .then_with(|| a.item.label.cmp(&b.item.label))
        });
        workspace_items.truncate(MAX_STANDALONE_WORKSPACE_COMPLETIONS);
    }

    lsp_items.append(&mut workspace_items);
    lsp_items.sort_by(|a, b| {
        b.score
            .cmp(&a.score)
            .then_with(|| {
                b.item
                    .export_kind
                    .is_some()
                    .cmp(&a.item.export_kind.is_some())
            })
            .then_with(|| a.item.label.cmp(&b.item.label))
    });

    lsp_items
}

fn workspace_symbol_completion_detail(symbol: &crate::lsp::CachedSymbol) -> Option<String> {
    if let Some(container_name) = symbol.container_name.as_ref() {
        return Some(container_name.clone());
    }
    let file_name = symbol
        .source_path
        .as_deref()
        .unwrap_or(symbol.file_path.as_path())
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_else(|| {
            symbol
                .import_path
                .as_deref()
                .and_then(|path| path.rsplit('/').next())
                .unwrap_or("")
        });
    if file_name.is_empty() {
        symbol.import_path.clone()
    } else {
        Some(format!("{file_name}:{}", symbol.line + 1))
    }
}

/// Convert symbol kind string to LSP completion kind number
fn symbol_kind_to_lsp_kind(kind: &str) -> u32 {
    match kind {
        "Function" => 3,
        "Method" => 2,
        "Class" => 7,
        "Interface" => 8,
        "Module" => 9,
        "Variable" => 6,
        "Constant" => 21,
        "Struct" => 22,
        "Enum" => 13,
        "EnumMember" => 20,
        "Constructor" => 4,
        "Field" => 5,
        "Property" => 10,
        _ => 1, // Text
    }
}

/// Filter cached completion items by a new prefix (client-side incremental filtering).
/// This is much faster than re-requesting from LSP server.
pub(super) fn filter_cached_completion_items(
    cached_items: &[CompletionDisplayItem],
    prefix: &str,
) -> Vec<CompletionDisplayItem> {
    if prefix.is_empty() {
        return cached_items
            .iter()
            .map(|item| CompletionDisplayItem {
                item: item.item.clone(),
                match_ranges: Vec::new(),
                score: 0,
                source: item.source.clone(),
            })
            .collect();
    }

    let mut scored: Vec<CompletionDisplayItem> = cached_items
        .iter()
        .filter_map(|cached_item| {
            let (score, match_ranges) = score_completion_match(&cached_item.item.label, prefix)?;
            Some(CompletionDisplayItem {
                item: cached_item.item.clone(),
                match_ranges,
                score,
                source: cached_item.source.clone(),
            })
        })
        .collect();

    scored.sort_by(|a, b| b.score.cmp(&a.score));
    scored
}

pub(super) fn score_completion_match(
    label: &str,
    query: &str,
) -> Option<(i64, Vec<(usize, usize)>)> {
    score_label_match(label, query)
}

pub(super) fn is_whole_word_match(text: &str, start: usize, end: usize) -> bool {
    let left_ok = if start == 0 {
        true
    } else {
        text[..start]
            .chars()
            .next_back()
            .is_none_or(|ch| classify_char(ch) != WordClass::Word)
    };
    let right_ok = if end >= text.len() {
        true
    } else {
        text[end..]
            .chars()
            .next()
            .is_none_or(|ch| classify_char(ch) != WordClass::Word)
    };
    left_ok && right_ok
}

pub(crate) fn path_matches(left: &Path, right: &Path) -> bool {
    if left == right {
        return true;
    }

    match (left.canonicalize(), right.canonicalize()) {
        (Ok(left), Ok(right)) => left == right,
        _ => {
            // A deleted file can no longer canonicalize, which would make
            // delete events through path aliases (/var vs /private/var,
            // symlinked roots) miss their buffer. Fall back to comparing the
            // canonical parent + file name.
            let canonical_parent_and_name = |path: &Path| {
                Some((
                    path.parent()?.canonicalize().ok()?,
                    path.file_name()?.to_owned(),
                ))
            };
            match (
                canonical_parent_and_name(left),
                canonical_parent_and_name(right),
            ) {
                (Some(left), Some(right)) => left == right,
                _ => false,
            }
        }
    }
}

pub(super) fn load_image_buffer(path: &Path) -> ImageBuffer {
    match image::open(path) {
        Ok(dynamic) => {
            let rgba = dynamic.to_rgba8();
            ImageBuffer {
                path: path.to_path_buf(),
                width: rgba.width(),
                height: rgba.height(),
                rgba: Some(rgba.into_raw()),
                error: None,
            }
        }
        Err(err) => ImageBuffer {
            path: path.to_path_buf(),
            width: 0,
            height: 0,
            rgba: None,
            error: Some(format!("image decode failed: {err}")),
        },
    }
}
