use super::overlays::load_image_buffer;
use super::*;

impl AppState {
    pub fn open_file(&mut self, path: PathBuf) -> Result<(), String> {
        let canonical_path = path
            .canonicalize()
            .map_err(|err| format!("canonicalize file {:?} failed: {err}", path))?;
        self.is_initial_launch_welcome = false;
        if is_supported_image_path(&canonical_path) {
            let active_idx = match self.buffers.iter().position(|buffer| {
                matches!(&buffer.content, BufferContent::Image(buffer) if buffer.path == canonical_path)
            }) {
                Some(idx) => idx,
                None => {
                    let image = load_image_buffer(&canonical_path);
                    self.buffers.push(BufferEntry {
                        content: BufferContent::Image(image),
                    });
                    self.buffers.len().saturating_sub(1)
                }
            };
            self.activate_buffer_index(active_idx)?;
            return Ok(());
        }
        let language_id = crate::lsp::registry::language_profile_for_path(&canonical_path)
            .map(|profile| profile.language_id.to_string());
        let active_idx = match self
            .buffers
            .iter()
            .position(|buffer| matches!(&buffer.content, BufferContent::Text(buffer) if buffer.path == canonical_path))
        {
            Some(idx) => idx,
            None => {
                let cached = self.closed_text_buffers.remove(&canonical_path);
                self.buffers.push(BufferEntry {
                    content: BufferContent::Text(cached.unwrap_or_else(|| {
                        EditorBuffer::new(canonical_path.clone(), language_id)
                    })),
                });
                self.buffers.len().saturating_sub(1)
            }
        };
        self.activate_buffer_index(active_idx)?;
        Ok(())
    }

    pub fn save_file(&mut self) -> Result<PathBuf, String> {
        if self.active_buffer_is_terminal() {
            return Err("cannot save terminal buffer".to_string());
        }
        if self.active_buffer_is_references() {
            return Err("cannot save references buffer".to_string());
        }
        if self.active_buffer_is_diagnostics() {
            return Err("cannot save diagnostics buffer".to_string());
        }

        let _ = self.cancel_file_history_preview();

        let _ = self.commit_transaction();

        let path = self
            .active_file
            .clone()
            .unwrap_or_else(|| self.default_save_path.clone());

        fs::write(&path, self.text.to_string())
            .map_err(|err| format!("save file {:?} failed: {err}", path))?;
        self.last_saved_at = Some(Instant::now());

        let canonical_path = path
            .canonicalize()
            .map_err(|err| format!("canonicalize saved file {:?} failed: {err}", path))?;

        self.active_file = Some(canonical_path.clone());
        self.register_open_text_buffer(canonical_path.clone());
        if let Some(active_idx) = self.active_buffer_index
            && let Some(slot) = self.buffers.get_mut(active_idx)
            && let BufferContent::Text(ref mut buffer) = slot.content
        {
            buffer.in_memory_text = Some(self.text.clone());
            buffer.dirty = false;
            buffer.history = self.history.clone();
        }
        let _ = self.workspace_expand_to_path(&canonical_path);
        self.dirty = false;
        self.external_conflict = None;
        Ok(canonical_path)
    }

    pub fn reload_active_file_from_disk_discarding_local(&mut self) -> Result<PathBuf, String> {
        let active_path = self
            .active_file
            .clone()
            .ok_or_else(|| "no active file to reload".to_string())?;
        self.load_buffer_from_file(&active_path)?;
        self.dirty = false;
        self.external_conflict = None;
        self.external_notice = Some(format!(
            "reloaded active file from disk (discarded unsaved changes): {}",
            active_path.display()
        ));
        if let Some(active_idx) = self.active_buffer_index
            && let Some(slot) = self.buffers.get_mut(active_idx)
            && let BufferContent::Text(ref mut buffer) = slot.content
        {
            buffer.in_memory_text = Some(self.text.clone());
            buffer.dirty = false;
            buffer.history = self.history.clone();
        }
        Ok(active_path)
    }

    pub fn new_empty_buffer(&mut self) -> bool {
        let changed = self.active_file.is_some()
            || self.active_buffer_index.is_some()
            || self.dirty
            || self.text.len_chars() > 0
            || self.is_initial_launch_welcome;
        if !changed {
            return false;
        }

        self.is_initial_launch_welcome = false;
        self.reset_text_editor_state();
        self.active_buffer_index = None;
        let _ = self.clear_current_overlays();
        self.bump_revision();
        true
    }

    pub fn buffer_next(&mut self) -> Result<bool, String> {
        self.cycle_buffer(true)
    }

    pub fn buffer_prev(&mut self) -> Result<bool, String> {
        self.cycle_buffer(false)
    }

    pub fn goto_buffer_index(&mut self, index: usize) -> bool {
        if index >= self.buffers.len() {
            return false;
        }
        if self.active_buffer_index == Some(index) {
            return false;
        }
        match self.activate_buffer_index(index) {
            Ok(()) => true,
            Err(_) => false,
        }
    }

    pub fn close_current_buffer(&mut self) -> Result<bool, String> {
        let Some(current_idx) = self.active_buffer_index else {
            return Ok(false);
        };
        self.close_buffer_index(current_idx)
    }

    pub fn close_buffer_for_path(&mut self, path: &Path) -> Result<bool, String> {
        let Some(index) = self.buffers.iter().position(|entry| {
            matches!(&entry.content, BufferContent::Text(buffer) if buffer.path == path)
        }) else {
            return Ok(false);
        };
        self.close_buffer_index(index)
    }

    fn close_buffer_index(&mut self, current_idx: usize) -> Result<bool, String> {
        self.save_current_text_buffer_history();
        let removed = self.buffers.remove(current_idx);
        if let BufferContent::Text(buffer) = removed.content {
            self.closed_text_buffers.insert(buffer.path.clone(), buffer);
        }

        // CRITICAL: Clear active_buffer_index immediately after removal to prevent
        // stale state from being accessed during buffer switch.
        // This fixes git decoration and treesitter highlight corruption when closing buffers.
        self.active_buffer_index = None;

        // CRITICAL: Reset text state to prevent closed buffer content from being
        // saved into the next activated buffer during the activate_buffer_index() call.
        // This fixes race condition where rapid close+switch operations would corrupt
        // buffer content (closed buffer content overwrites newly opened buffer).
        self.text = Rope::new();
        self.cached_line_starts = None;

        if self.buffers.is_empty() {
            self.reset_text_editor_state();
            let _ = self.clear_current_overlays();
            self.bump_revision();
            return Ok(true);
        }

        let mut next_idx = current_idx.min(self.buffers.len().saturating_sub(1));
        while !self.buffers.is_empty() {
            match self.activate_buffer_index(next_idx) {
                Ok(()) => return Ok(true),
                Err(_) => {
                    self.buffers.remove(next_idx);
                    if self.buffers.is_empty() {
                        return Ok(self.new_empty_buffer());
                    }
                    if next_idx >= self.buffers.len() {
                        next_idx = 0;
                    }
                }
            }
        }

        Ok(self.new_empty_buffer())
    }

    pub fn begin_visual_selection(&mut self) -> bool {
        self.visual_line_mode = false;
        let anchor = if self.text.len_chars() == 0 {
            0
        } else {
            self.cursor_char_idx
                .min(self.text.len_chars().saturating_sub(1))
        };
        if self.selection_anchor_char_idx == Some(anchor) {
            return false;
        }
        self.selection_anchor_char_idx = Some(anchor);
        true
    }

    pub fn begin_visual_line_selection(&mut self) -> bool {
        self.visual_line_mode = true;
        let line_idx = self.text.char_to_line(
            self.cursor_char_idx
                .min(self.text.len_chars().saturating_sub(1).max(0)),
        );
        let anchor = self.text.line_to_char(line_idx);
        self.selection_anchor_char_idx = Some(anchor);
        // Move cursor to the last char of the line (before newline)
        let line_end = self.line_content_end_char_idx(line_idx);
        if self.cursor_char_idx != line_end {
            self.cursor_char_idx = line_end;
            let (_, col) = self.cursor_line_col();
            self.target_col = col;
        }
        true
    }

    pub fn clear_visual_selection(&mut self) -> bool {
        if self.selection_anchor_char_idx.is_none() && !self.visual_line_mode {
            return false;
        }
        self.selection_anchor_char_idx = None;
        self.visual_line_mode = false;
        true
    }

    pub fn visual_selection_range(&self) -> Option<VisualSelectionRange> {
        if self.current_mode() != EditorMode::Visual {
            return None;
        }
        let anchor = self.selection_anchor_char_idx?;
        let len_chars = self.text.len_chars();
        if len_chars == 0 {
            return None;
        }

        let anchor_idx = anchor.min(len_chars.saturating_sub(1));
        let focus_idx = self.cursor_char_idx.min(len_chars.saturating_sub(1));
        let (start_char, end_char) = if self.visual_line_mode {
            // Expand to full lines: from start of first line to start of line after last
            let first_line = self.text.char_to_line(anchor_idx.min(focus_idx));
            let last_line = self.text.char_to_line(anchor_idx.max(focus_idx));
            let sc = self.text.line_to_char(first_line);
            let ec = if last_line + 1 < self.text.len_lines() {
                self.text.line_to_char(last_line + 1)
            } else {
                len_chars
            };
            (sc, ec)
        } else {
            let sc = anchor_idx.min(focus_idx);
            let ec = anchor_idx.max(focus_idx).saturating_add(1).min(len_chars);
            (sc, ec)
        };

        if start_char >= end_char {
            return None;
        }

        let start_line = self.text.char_to_line(start_char);
        let end_line = self.text.char_to_line(end_char.saturating_sub(1));
        let start_byte = self.text.char_to_byte(start_char);
        let end_byte = self.text.char_to_byte(end_char);
        let start_byte_in_line = start_byte.saturating_sub(self.text.line_to_byte(start_line));
        let end_byte_in_line = end_byte.saturating_sub(self.text.line_to_byte(end_line));

        Some(VisualSelectionRange {
            start_char,
            end_char,
            start_line,
            end_line,
            start_byte_in_line,
            end_byte_in_line,
        })
    }

    pub fn visual_selection_text(&self) -> Option<String> {
        let selection = self.visual_selection_range()?;
        self.char_range_text(selection.start_char, selection.end_char)
    }

    pub fn delete_char_text_at_cursor(&self) -> Option<String> {
        let (start, end) = self.delete_char_range_at_cursor()?;
        self.char_range_text(start, end)
    }

    pub fn delete_current_line_text(&self) -> Option<String> {
        let (start, end) = self.current_line_delete_range()?;
        self.linewise_text_for_range(start, end)
    }

    pub fn yank_current_line_text(&self) -> Option<String> {
        let (start, end) = self.current_line_delete_range()?;
        self.linewise_text_for_range(start, end)
    }

    pub fn delete_word_forward_text(&self) -> Option<String> {
        let (start, end) = self.delete_word_forward_range()?;
        self.char_range_text(start, end)
    }

    pub fn yank_to_word_end_text(&self) -> Option<String> {
        let (start, end) = self.yank_word_end_range()?;
        self.char_range_text(start, end)
    }

    pub fn delete_word_backward_text(&self) -> Option<String> {
        let (start, end) = self.delete_word_backward_range()?;
        self.char_range_text(start, end)
    }

    pub fn substitute_current_line_text(&self) -> Option<String> {
        let line_idx = self
            .text
            .char_to_line(self.cursor_char_idx.min(self.text.len_chars()));
        let start = self.first_non_blank_or_line_start(line_idx);
        let end = self.line_content_end_char_idx(line_idx);
        if start >= end {
            return None;
        }
        self.char_range_text(start, end)
    }

    /// Replace the current visual selection with the given text.
    /// Deletes the selected range, inserts `text` at the selection start,
    /// and positions the cursor at the last character of the inserted text.
    /// Returns `false` if there is no selection or the buffer is empty.
    pub fn replace_selection_with_text(&mut self, text: &str) -> bool {
        let Some(selection) = self.visual_selection_range() else {
            return false;
        };

        let start = selection.start_char;
        let len = selection.end_char - selection.start_char;

        // Delete the selected range
        self.apply_delete(start, len);
        self.selection_anchor_char_idx = None;

        // Insert the replacement text at the start position
        if !text.is_empty() {
            self.apply_insert(start, text.to_string());
        }

        // Position cursor: last char of inserted text (or at start if empty)
        let inserted_chars = text.chars().count();
        self.cursor_char_idx = if inserted_chars > 0 {
            (start + inserted_chars - 1).min(self.text.len_chars())
        } else {
            start.min(self.text.len_chars())
        };
        let (_, col) = self.cursor_line_col();
        self.target_col = col;
        self.dirty = true;
        self.bump_revision();
        true
    }

    pub fn delete_visual_selection(&mut self) -> bool {
        let Some(selection) = self.visual_selection_range() else {
            return false;
        };

        self.apply_delete(
            selection.start_char,
            selection.end_char - selection.start_char,
        );
        self.cursor_char_idx = selection.start_char.min(self.text.len_chars());
        let (_, col) = self.cursor_line_col();
        self.target_col = col;
        self.selection_anchor_char_idx = None;
        self.dirty = true;
        self.bump_revision();
        true
    }

    pub fn select_text_object(
        &mut self,
        modifier: TextObjectModifier,
        kind: TextObjectKind,
    ) -> bool {
        let Some((start, end)) =
            find_text_object_range(&self.text, self.cursor_char_idx, modifier, kind)
        else {
            return false;
        };
        let len = self.text.len_chars();
        if len == 0 {
            return false;
        }

        // Clamp để tránh out-of-bounds.
        let anchor = start.min(len.saturating_sub(1));
        let focus = end.saturating_sub(1).min(len.saturating_sub(1));

        // Chuyển sang Visual mode nếu cần.
        if self.current_mode() != EditorMode::Visual {
            if self.can_apply_mode_event(ModeEvent::EnterVisual) {
                let _ = self.apply_mode_event(ModeEvent::EnterVisual);
            } else {
                return false;
            }
        }

        self.visual_line_mode = false;
        self.selection_anchor_char_idx = Some(anchor);
        self.cursor_char_idx = focus;
        let (_, col) = self.cursor_line_col();
        self.target_col = col;
        self.bump_revision();
        true
    }

    /// Lấy char range text cho một text object (dùng trước khi xóa/yank).
    pub fn text_object_text(
        &self,
        modifier: TextObjectModifier,
        kind: TextObjectKind,
    ) -> Option<String> {
        let (start, end) =
            find_text_object_range(&self.text, self.cursor_char_idx, modifier, kind)?;
        self.char_range_text(start, end)
    }

    /// Xóa text object tại vị trí con trỏ và trả về true nếu thành công.
    pub fn delete_text_object(
        &mut self,
        modifier: TextObjectModifier,
        kind: TextObjectKind,
    ) -> bool {
        let Some((start, end)) =
            find_text_object_range(&self.text, self.cursor_char_idx, modifier, kind)
        else {
            return false;
        };
        if start >= end {
            return false;
        }
        self.apply_delete(start, end - start);
        self.cursor_char_idx = start.min(self.text.len_chars());
        let (_, col) = self.cursor_line_col();
        self.target_col = col;
        self.selection_anchor_char_idx = None;
        self.visual_line_mode = false;
        self.dirty = true;
        self.bump_revision();
        true
    }

    pub fn paste_after(&mut self, text: &str) -> bool {
        let insert_text = text.to_string();
        if insert_text.is_empty() {
            return false;
        }

        let line_idx = self
            .text
            .char_to_line(self.cursor_char_idx.min(self.text.len_chars()));
        let line_end = self.line_content_end_char_idx(line_idx);
        let insert_at = if self.cursor_char_idx < line_end {
            self.cursor_char_idx + 1
        } else {
            line_end
        };

        if !self.apply_insert(insert_at, insert_text.clone()) {
            return false;
        }

        let inserted_chars = insert_text.chars().count();
        self.cursor_char_idx =
            (insert_at + inserted_chars.saturating_sub(1)).min(self.text.len_chars());
        let (_, col) = self.cursor_line_col();
        self.target_col = col;
        self.dirty = true;
        self.bump_revision();
        true
    }

    pub fn paste_before(&mut self, text: &str) -> bool {
        let insert_text = text.to_string();
        if insert_text.is_empty() {
            return false;
        }

        let insert_at = self.cursor_char_idx.min(self.text.len_chars());
        if !self.apply_insert(insert_at, insert_text.clone()) {
            return false;
        }

        let inserted_chars = insert_text.chars().count();
        self.cursor_char_idx =
            (insert_at + inserted_chars.saturating_sub(1)).min(self.text.len_chars());
        let (_, col) = self.cursor_line_col();
        self.target_col = col;
        self.dirty = true;
        self.bump_revision();
        true
    }

    pub fn insert_text_at_cursor(&mut self, text: &str) -> bool {
        let insert_text = text.to_string();
        if insert_text.is_empty() {
            return false;
        }

        let insert_at = self.cursor_char_idx.min(self.text.len_chars());
        if !self.apply_insert(insert_at, insert_text.clone()) {
            return false;
        }

        let inserted_chars = insert_text.chars().count();
        self.cursor_char_idx = (insert_at + inserted_chars).min(self.text.len_chars());
        let (_, col) = self.cursor_line_col();
        self.target_col = col;
        self.dirty = true;
        self.bump_revision();
        true
    }

    pub fn replace_completion_prefix_at_cursor(
        &mut self,
        prefix_len_chars: usize,
        text: &str,
    ) -> bool {
        let insert_text = text.to_string();
        if insert_text.is_empty() {
            return false;
        }

        let cursor = self.cursor_char_idx.min(self.text.len_chars());
        let line_idx = self.text.char_to_line(cursor);
        let line_start = self.text.line_to_char(line_idx);
        let delete_start = cursor.saturating_sub(prefix_len_chars).max(line_start);
        let delete_len = cursor.saturating_sub(delete_start);

        let mut changed = false;
        if delete_len > 0 {
            changed |= self.apply_delete(delete_start, delete_len);
        }
        changed |= self.apply_insert(delete_start, insert_text.clone());
        if !changed {
            return false;
        }

        self.cursor_char_idx = delete_start + insert_text.chars().count();
        let (_, col) = self.cursor_line_col();
        self.target_col = col;
        self.dirty = true;
        let _ = self.commit_transaction();
        self.bump_revision();
        true
    }

    pub fn paste_linewise_after(&mut self, text: &str) -> bool {
        self.paste_linewise(text, false)
    }

    pub fn paste_linewise_before(&mut self, text: &str) -> bool {
        self.paste_linewise(text, true)
    }

    pub fn toggle_line_comment(&mut self) -> bool {
        let (line_idx, _) = self.cursor_line_col();
        self.toggle_comments_on_lines(line_idx, line_idx)
    }

    pub fn toggle_selection_comment(&mut self) -> bool {
        let Some(selection) = self.visual_selection_range() else {
            return false;
        };
        self.toggle_comments_on_lines(selection.start_line, selection.end_line)
    }

    pub fn wrap_selection_with_star(&mut self) -> bool {
        let Some(selection) = self.visual_selection_range() else {
            return false;
        };
        let Some(selected_text) = self.visual_selection_text() else {
            return false;
        };

        let start = selection.start_char;
        let len = selection.end_char - selection.start_char;

        self.apply_delete(start, len);
        let wrapped = format!("*{}*", selected_text);
        self.apply_insert(start, wrapped);

        self.cursor_char_idx = start + 1 + selected_text.len();
        let (_, col) = self.cursor_line_col();
        self.target_col = col;
        self.selection_anchor_char_idx = None;
        self.dirty = true;
        self.bump_revision();
        true
    }

    pub fn commit_transaction(&mut self) -> bool {
        let Some(pending) = self.current_transaction.take() else {
            return false;
        };
        let edit = compact_edit_delta(&pending.before_text, &self.text);

        let transaction = Transaction::new(edit, pending.before_cursor, self.cursor_state());
        self.history.undo_stack.push(transaction);
        self.history.redo_stack.clear();
        true
    }
}

fn compact_edit_delta(before: &Rope, after: &Rope) -> EditTransaction {
    let before_len = before.len_chars();
    let after_len = after.len_chars();
    let shared_len = before_len.min(after_len);

    let mut start_char_idx = 0usize;
    while start_char_idx < shared_len && before.char(start_char_idx) == after.char(start_char_idx) {
        start_char_idx += 1;
    }

    let mut before_end = before_len;
    let mut after_end = after_len;
    while before_end > start_char_idx
        && after_end > start_char_idx
        && before.char(before_end - 1) == after.char(after_end - 1)
    {
        before_end -= 1;
        after_end -= 1;
    }

    let deleted_text = if start_char_idx < before_end {
        before.slice(start_char_idx..before_end).to_string()
    } else {
        String::new()
    };
    let inserted_text = if start_char_idx < after_end {
        after.slice(start_char_idx..after_end).to_string()
    } else {
        String::new()
    };

    EditTransaction::new(start_char_idx, deleted_text, inserted_text)
}
