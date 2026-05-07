use super::*;

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

    pub fn active_filetype_label(&self) -> &'static str {
        if self.active_buffer_is_terminal() {
            return "Terminal";
        }
        if self.active_buffer_is_diagnostics() {
            return "Diagnostics";
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

    pub(super) fn restore_cursor_state(&mut self, state: CursorState) {
        self.cursor_char_idx = state.char_idx.min(self.text.len_chars());
        let line_idx = self
            .text
            .char_to_line(self.cursor_char_idx.min(self.text.len_chars()));
        self.target_col = state.target_col.min(self.max_col_for_line(line_idx));
    }

    pub(super) fn snapshot_editor_view(&self) -> EditorViewSnapshot {
        EditorViewSnapshot {
            text: self.text.clone(),
            cursor: self.cursor_state(),
            selection_anchor_char_idx: self.selection_anchor_char_idx,
            visual_line_mode: self.visual_line_mode,
            target_scroll_y: self.target_scroll_y,
            current_scroll_y: self.current_scroll_y,
            scroll_column: self.scroll_column,
            dirty: self.dirty,
        }
    }

    pub(super) fn restore_editor_view(&mut self, snapshot: &EditorViewSnapshot) {
        self.text = snapshot.text.clone();
        self.restore_cursor_state(snapshot.cursor);
        self.selection_anchor_char_idx = snapshot.selection_anchor_char_idx;
        self.visual_line_mode = snapshot.visual_line_mode;
        self.target_scroll_y = snapshot.target_scroll_y;
        self.current_scroll_y = snapshot.current_scroll_y;
        self.scroll_column = snapshot.scroll_column;
        self.dirty = snapshot.dirty;
        let _ = self.refresh_active_search_highlights();
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

    pub(super) fn refresh_active_search_highlights(&mut self) -> bool {
        let next = if self.last_search_query.is_empty() {
            Vec::new()
        } else {
            let text = self.text.to_string();
            collect_search_highlights(&text, &self.last_search_query, self.search_whole_word)
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
        if self.text.len_lines() == 0 {
            return false;
        }

        let last_line = self.text.len_lines().saturating_sub(1);
        let start_line = start_line.min(last_line);
        let end_line = end_line.min(last_line);
        let plans: Vec<LineCommentPlan> = (start_line..=end_line)
            .map(|line_idx| line_comment_plan(&self.text, line_idx, syntax.line_prefix))
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
            let insert_text = format!("{} ", syntax.line_prefix);
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

        if changed {
            self.bump_revision();
        }
        changed
    }

    pub(super) fn bump_revision(&mut self) {
        self.revision += 1;
    }

    pub(super) fn should_ignore_self_save_event(&self) -> bool {
        self.last_saved_at.is_some_and(|saved_at| {
            Instant::now().saturating_duration_since(saved_at) < Self::SELF_SAVE_IGNORE_WINDOW
        })
    }

    pub(super) fn load_buffer_from_file(&mut self, canonical_path: &Path) -> Result<(), String> {
        let content = fs::read_to_string(canonical_path)
            .map_err(|err| format!("open file {:?} failed: {err}", canonical_path))?;
        self.replace_text_buffer_preserving_view(content.as_str());
        let _ = self.refresh_active_search_highlights();
        Ok(())
    }

    pub(super) fn load_buffer_from_file_resetting_view(
        &mut self,
        canonical_path: &Path,
    ) -> Result<(), String> {
        let content = fs::read_to_string(canonical_path)
            .map_err(|err| format!("open file {:?} failed: {err}", canonical_path))?;
        self.text = Rope::from(content.as_str());
        self.cursor_char_idx = 0;
        self.target_col = 0;
        self.target_scroll_y = 0.0;
        self.current_scroll_y = 0.0;
        self.scroll_column = 0;
        self.selection_anchor_char_idx = None;
        self.visual_line_mode = false;
        self.clear_history();
        let _ = self.refresh_active_search_highlights();
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

    pub(super) fn activate_buffer_index(&mut self, index: usize) -> Result<(), String> {
        let Some(buffer) = self.buffers.get(index).cloned() else {
            return Err(format!("buffer index {index} out of range"));
        };

        match buffer.content {
            BufferContent::Text(buffer) => {
                self.load_buffer_from_file_resetting_view(&buffer.path)?;
                self.active_file = Some(buffer.path.clone());
                self.active_buffer_index = Some(index);
                self.selection_anchor_char_idx = None;
                self.dirty = false;
                self.external_conflict = None;
                self.visual_line_mode = false;
                let _ = self.workspace_expand_to_path(&buffer.path);
            }
            BufferContent::Image(buffer) => {
                self.reset_text_editor_state();
                let refreshed = load_image_buffer(&buffer.path);
                if let Some(slot) = self.buffers.get_mut(index) {
                    slot.content = BufferContent::Image(refreshed);
                }
                self.active_buffer_index = Some(index);
                let _ = self.workspace_expand_to_path(&buffer.path);
            }
            BufferContent::Terminal(_) => {
                self.active_file = None;
                self.active_buffer_index = Some(index);
                self.selection_anchor_char_idx = None;
                self.visual_line_mode = false;
                self.external_conflict = None;
            }
            BufferContent::References(_)
            | BufferContent::Diagnostics(_)
            | BufferContent::FuzzyPicker(_)
            | BufferContent::SettingsTab(_)
            | BufferContent::Help(_) => {
                self.reset_text_editor_state();
                self.active_buffer_index = Some(index);
                let _ = self.clear_current_overlays();
            }
        }

        self.bump_revision();
        Ok(())
    }

    pub(super) fn reset_text_editor_state(&mut self) {
        self.text = Rope::new();
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
        self.clear_history();
        let _ = self.refresh_active_search_highlights();
    }
}

#[derive(Debug, Clone, Copy)]
pub(super) struct CommentSyntax {
    line_prefix: &'static str,
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
    if let Some(file_name) = path.file_name().and_then(|name| name.to_str())
        && file_name.eq_ignore_ascii_case("makefile")
    {
        return Some(CommentSyntax { line_prefix: "#" });
    }

    let extension = path
        .extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| ext.to_ascii_lowercase());
    let line_prefix = match extension.as_deref() {
        Some(
            "rs" | "go" | "js" | "jsx" | "ts" | "tsx" | "c" | "cc" | "cpp" | "h" | "hpp" | "java"
            | "kt" | "kts" | "swift" | "cs" | "dart" | "scala" | "scss" | "proto" | "php",
        ) => "//",
        Some(
            "py" | "sh" | "bash" | "zsh" | "fish" | "rb" | "yml" | "yaml" | "toml" | "ini" | "cfg"
            | "conf" | "properties",
        ) => "#",
        Some("sql" | "lua") => "--",
        _ => "//",
    };

    Some(CommentSyntax { line_prefix })
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
) -> Vec<(usize, usize)> {
    if query.is_empty() {
        return Vec::new();
    }

    text.match_indices(query)
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
        })
        .collect()
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

pub(super) fn path_matches(left: &Path, right: &Path) -> bool {
    if left == right {
        return true;
    }

    match (left.canonicalize(), right.canonicalize()) {
        (Ok(left), Ok(right)) => left == right,
        _ => false,
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
