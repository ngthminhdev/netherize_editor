use super::overlays::load_image_buffer;
use super::*;

impl AppState {
    pub fn dirty_buffer_count(&self) -> usize {
        let active_index = self.active_buffer_index;
        let tab_buffers = self
            .buffers
            .iter()
            .enumerate()
            .filter(|(index, buffer)| buffer.is_dirty(Some(*index) == active_index, self.dirty))
            .count();
        let untabbed_editor = usize::from(active_index.is_none() && self.dirty);
        let canvas_session = usize::from(
            self.canvas_edit_session
                .as_ref()
                .is_some_and(CanvasEditSession::is_dirty),
        );
        tab_buffers + untabbed_editor + canvas_session
    }

    pub(crate) fn dirty_recovery_buffers(&self) -> Vec<crate::app::persistence::RecoveryBuffer> {
        let active_index = self.active_buffer_index;
        let mut recovery = self
            .buffers
            .iter()
            .enumerate()
            .filter_map(|(index, entry)| {
                let BufferContent::Text(buffer) = &entry.content else {
                    return None;
                };
                let is_active = active_index == Some(index);
                if !(buffer.dirty || (is_active && self.dirty)) {
                    return None;
                }
                let text = if is_active {
                    self.text.clone()
                } else {
                    buffer.in_memory_text.clone()?
                };
                Some(crate::app::persistence::RecoveryBuffer {
                    path: buffer.path.clone(),
                    text,
                })
            })
            .collect::<Vec<_>>();
        if active_index.is_none() && self.dirty {
            let path = self
                .active_file
                .clone()
                .unwrap_or_else(|| self.default_save_path.clone());
            recovery.push(crate::app::persistence::RecoveryBuffer {
                path,
                text: self.text.clone(),
            });
        }
        if let Some(session) = self
            .canvas_edit_session
            .as_ref()
            .filter(|session| session.is_dirty())
        {
            recovery.push(session.recovery_buffer());
        }
        recovery
    }

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
        super::overlays::ensure_interactive_text_file_size(&canonical_path)?;
        let language_id = crate::lsp::registry::language_profile_for_path(&canonical_path)
            .map(|profile| profile.language_id.to_string());
        let mut inserted = false;
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
                inserted = true;
                self.buffers.len().saturating_sub(1)
            }
        };
        if let Err(err) = self.activate_buffer_index(active_idx) {
            if inserted {
                self.buffers.remove(active_idx);
            }
            return Err(err);
        }
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
        if self.active_buffer_is_markdown_preview() {
            return Err("cannot save markdown preview buffer".to_string());
        }

        let _ = self.cancel_file_history_preview();

        let _ = self.commit_transaction();

        let path = self
            .active_file
            .clone()
            .unwrap_or_else(|| self.default_save_path.clone());

        crate::app::persistence::atomic_write(&path, self.text.to_string())
            .map_err(|err| format!("save file {:?} failed: {err}", path))?;

        let canonical_path = path
            .canonicalize()
            .map_err(|err| format!("canonicalize saved file {:?} failed: {err}", path))?;

        let modified_time = std::fs::metadata(&canonical_path)
            .and_then(|m| m.modified())
            .ok();

        self.active_file = Some(canonical_path.clone());
        self.register_open_text_buffer(canonical_path.clone());
        if let Some(active_idx) = self.active_buffer_index
            && let Some(slot) = self.buffers.get_mut(active_idx)
            && let BufferContent::Text(ref mut buffer) = slot.content
        {
            buffer.in_memory_text = Some(self.text.clone());
            buffer.dirty = false;
            buffer.history = self.history.clone();
            buffer.last_known_modified_time = modified_time;
        }
        let _ = self.workspace_expand_to_path(&canonical_path);
        self.reindex_ts_js_exports_for_saved_file(&canonical_path);
        self.dirty = false;
        self.external_conflict = None;
        Ok(canonical_path)
    }

    /// Re-extract a just-saved TS/JS file's exports into the workspace symbol
    /// cache. The whole-workspace export index only runs once at LSP start, so
    /// without this an export added after startup stays invisible to auto-import
    /// until a restart. No-op for non-TS/JS files or when no workspace is open.
    fn reindex_ts_js_exports_for_saved_file(&self, saved_path: &Path) {
        let Some(profile) = crate::lsp::registry::language_profile_for_path(saved_path) else {
            return;
        };
        if !matches!(profile.key, "typescript" | "tsx" | "javascript" | "jsx") {
            return;
        }
        let Some(workspace_root) = self.workspace_root_path().map(PathBuf::from) else {
            return;
        };
        let text = self.text.to_string();
        let symbols =
            crate::lsp::extract_ts_js_exports_from_text(saved_path, &workspace_root, &text);
        self.workspace_symbol_cache()
            .upsert_file_symbols(profile.key, saved_path, symbols);
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

    /// Activate the already-open text buffer for `path`, if present. Used to
    /// return to the source file after a center-pane picker (e.g. file history)
    /// closes to an arbitrary adjacent buffer. No-op if the buffer isn't open.
    pub fn activate_text_buffer_for_path(&mut self, path: &Path) -> bool {
        let Some(idx) = self.buffers.iter().position(
            |entry| matches!(&entry.content, BufferContent::Text(buffer) if buffer.path == path),
        ) else {
            return false;
        };
        self.activate_buffer_index(idx).is_ok()
    }

    pub fn close_buffer_for_path(&mut self, path: &Path) -> Result<bool, String> {
        let Some(index) = self.buffers.iter().position(
            |entry| matches!(&entry.content, BufferContent::Text(buffer) if buffer.path == path),
        ) else {
            return Ok(false);
        };
        self.close_buffer_index(index)
    }

    fn close_buffer_index(&mut self, current_idx: usize) -> Result<bool, String> {
        self.save_current_text_buffer_history();
        let removed = self.buffers.remove(current_idx);
        if let BufferContent::Text(buffer) = removed.content {
            if let Some(ref session) = self.test_field_edit {
                if buffer.path == session.scratch_path {
                    let path_to_remove = session.scratch_path.clone();
                    self.test_field_edit = None;
                    let _ = std::fs::remove_file(path_to_remove);
                }
            }
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

    pub fn begin_visual_block_selection(&mut self) -> bool {
        let (line_idx, col) = self.cursor_line_col();
        if self.visual_block_anchor_line == Some(line_idx)
            && self.visual_block_anchor_col == Some(col)
        {
            return false;
        }
        self.visual_block_anchor_line = Some(line_idx);
        self.visual_block_anchor_col = Some(col);
        self.selection_anchor_char_idx = None;
        self.visual_line_mode = false;
        true
    }

    pub fn clear_visual_block_selection(&mut self) -> bool {
        if self.visual_block_anchor_line.is_none() {
            return false;
        }
        self.visual_block_anchor_line = None;
        self.visual_block_anchor_col = None;
        true
    }

    pub fn visual_block_range(&self) -> Option<VisualBlockRange> {
        if self.current_mode() != EditorMode::VisualBlock {
            return None;
        }
        let anchor_line = self.visual_block_anchor_line?;
        let anchor_col = self.visual_block_anchor_col?;
        let (cursor_line, cursor_col) = self.cursor_line_col();

        let start_line = anchor_line.min(cursor_line);
        let end_line = anchor_line.max(cursor_line);
        let start_col = anchor_col.min(cursor_col);
        let end_col = anchor_col.max(cursor_col);

        Some(VisualBlockRange {
            start_line,
            end_line,
            start_col,
            end_col,
        })
    }

    pub fn clear_visual_selection(&mut self) -> bool {
        let had_selection = self.selection_anchor_char_idx.is_some()
            || self.visual_line_mode
            || self.visual_block_anchor_line.is_some();
        self.selection_anchor_char_idx = None;
        self.visual_line_mode = false;
        self.visual_block_anchor_line = None;
        self.visual_block_anchor_col = None;
        had_selection
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
        // Anchor is cleared below, before the caller's mode event — capture for gv now.
        self.capture_last_visual_selection();

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
        // Anchor is cleared below, before the caller's mode event — capture for gv now.
        self.capture_last_visual_selection();

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

    /// Mouse click — collapse any selection and move the caret to `char_idx`.
    /// Exits Visual/MultiCursor modes (virtual cursors are auto-cleared by the
    /// mode transition).
    pub fn place_caret_at(&mut self, char_idx: usize) -> bool {
        let len_chars = self.text.len_chars();
        match self.current_mode() {
            EditorMode::Visual | EditorMode::VisualBlock | EditorMode::MultiCursor => {
                let _ = self.apply_mode_event(ModeEvent::EnterNormal);
                let _ = self.clear_visual_selection();
            }
            _ => {}
        }
        self.cursor_char_idx = char_idx.min(len_chars);
        let (_, col) = self.cursor_line_col();
        self.target_col = col;
        self.selection_anchor_char_idx = None;
        self.bump_revision();
        true
    }

    /// Double-click — select the word at `char_idx` (Visual charwise).
    pub fn select_word_at(&mut self, char_idx: usize) -> bool {
        let len_chars = self.text.len_chars();
        if len_chars == 0 {
            return false;
        }
        let pos = char_idx.min(len_chars - 1);
        if self.text.char(pos) == '\n' {
            return false;
        }
        let is_word_char = |c: char| c.is_alphanumeric() || c == '_';
        let mut start = pos;
        while start > 0 && is_word_char(self.text.char(start - 1)) {
            start -= 1;
        }
        let mut end = pos + 1;
        while end < len_chars && is_word_char(self.text.char(end)) {
            end += 1;
        }
        if !is_word_char(self.text.char(pos)) {
            // On whitespace/punctuation — select that single char instead.
            (start, end) = (pos, pos + 1);
        }

        if self.current_mode() != EditorMode::Visual {
            if !self.can_apply_mode_event(ModeEvent::EnterVisual) {
                return false;
            }
            let _ = self.apply_mode_event(ModeEvent::EnterVisual);
        }
        self.visual_line_mode = false;
        self.selection_anchor_char_idx = Some(start);
        self.cursor_char_idx = end - 1;
        let (_, col) = self.cursor_line_col();
        self.target_col = col;
        self.bump_revision();
        true
    }

    /// Mouse drag — anchor stays fixed, head follows the pointer. Enters Visual
    /// charwise on the first movement.
    pub fn drag_select_to(&mut self, anchor_char: usize, head_char: usize) -> bool {
        let len_chars = self.text.len_chars();
        if len_chars == 0 {
            return false;
        }
        if self.current_mode() != EditorMode::Visual {
            if !self.can_apply_mode_event(ModeEvent::EnterVisual) {
                return false;
            }
            let _ = self.apply_mode_event(ModeEvent::EnterVisual);
            self.visual_line_mode = false;
        }
        self.selection_anchor_char_idx = Some(anchor_char.min(len_chars));
        self.cursor_char_idx = head_char.min(len_chars - 1);
        let (_, col) = self.cursor_line_col();
        self.target_col = col;
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
        if self.is_multi_cursor_mode() {
            return self.multi_insert_text(text);
        }
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
        if self.is_multi_cursor_mode() {
            return self.multi_insert_text(text);
        }
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
        if self.is_multi_cursor_mode() {
            return self.multi_insert_text(text);
        }
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
        if self.toggle_block_comment_on_selection() {
            return true;
        }
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

    /// In-memory text of the open buffer at `path` (active or inactive).
    /// Used to push externally reloaded content to the LSP server.
    pub fn buffer_text_for_path(&self, path: &Path) -> Option<String> {
        if self
            .active_file()
            .is_some_and(|active| crate::app::app_state::overlays::path_matches(active, path))
        {
            return Some(self.text_string());
        }
        self.buffers.iter().find_map(|entry| match &entry.content {
            BufferContent::Text(buffer)
                if crate::app::app_state::overlays::path_matches(&buffer.path, path) =>
            {
                buffer.in_memory_text.as_ref().map(|rope| rope.to_string())
            }
            _ => None,
        })
    }

    /// 3s safety-net poll: stat every clean open buffer and return the paths
    /// whose mtime changed. NO file contents are read here (#4) — the caller
    /// submits a `ReadExternalFiles` worker request and the contents come back
    /// through `apply_external_file_contents`. The self-save echo check also
    /// lives in the apply phase (content compare beats re-reading the disk).
    /// `buffer.last_known_modified_time` is only advanced when the content is
    /// applied, so an in-flight change keeps being re-detected (idempotent)
    /// rather than silently dropped if a read fails.
    pub fn collect_externally_modified_open_buffers(
        &mut self,
        last_checked_times: &mut HashMap<PathBuf, std::time::SystemTime>,
    ) -> Vec<PathBuf> {
        let mut changed_paths = Vec::new();

        let candidates: Vec<(usize, PathBuf)> = self
            .buffers
            .iter()
            .enumerate()
            .filter_map(|(idx, entry)| {
                if let BufferContent::Text(ref buffer) = entry.content {
                    let is_active = self.active_buffer_index == Some(idx);
                    let is_dirty = buffer.dirty || (is_active && self.dirty);
                    if !is_dirty {
                        Some((idx, buffer.path.clone()))
                    } else {
                        None
                    }
                } else {
                    None
                }
            })
            .collect();

        for (idx, path) in candidates {
            if let Ok(metadata) = std::fs::metadata(&path) {
                if let Ok(modified_time) = metadata.modified() {
                    let needs_reload = if let Some(slot) = self.buffers.get(idx)
                        && let BufferContent::Text(ref buffer) = slot.content
                    {
                        // #2: dùng `!=` thay vì `>` — git checkout/stash có thể khôi
                        // phục mtime CŨ HƠN, mà `>` sẽ bỏ sót hoàn toàn.
                        match buffer.last_known_modified_time {
                            Some(last_checked) => modified_time != last_checked,
                            None => true,
                        }
                    } else {
                        false
                    };

                    if needs_reload {
                        changed_paths.push(path.clone());
                    }
                    last_checked_times.insert(path, modified_time);
                }
            }
        }

        changed_paths
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

// ── Session layout (persisted tabs + cursors) ────────────────────────────────

impl AppState {
    /// Text tabs in order with their cursors, plus which one is active. Non-text
    /// tabs (terminals, previews, pickers) are transient and skipped.
    pub fn session_layout_snapshot(&self) -> crate::app::persistence::SessionLayout {
        use crate::app::persistence::{SessionFile, SessionLayout};
        let mut files = Vec::new();
        let mut active = None;
        for (idx, entry) in self.buffers.iter().enumerate() {
            let BufferContent::Text(buf) = &entry.content else {
                continue;
            };
            let is_active = self.active_buffer_index == Some(idx);
            let (line, col) = if is_active {
                self.cursor_line_col()
            } else if let Some(rope) = buf.in_memory_text.as_ref() {
                let char_idx = buf.view_state.cursor.char_idx.min(rope.len_chars());
                let line = rope.char_to_line(char_idx);
                (line, char_idx - rope.line_to_char(line))
            } else {
                (0, 0)
            };
            if is_active {
                active = Some(buf.path.clone());
            }
            files.push(SessionFile {
                path: buf.path.clone(),
                line,
                col,
            });
        }
        SessionLayout {
            files,
            active,
            bottom_terminal: false,
        }
    }

    /// Reopen the tabs of `layout` (files that still exist), restore each
    /// cursor and make the recorded tab active. Returns how many opened.
    pub fn apply_session_layout(
        &mut self,
        layout: &crate::app::persistence::SessionLayout,
    ) -> usize {
        let mut opened = 0;
        for file in &layout.files {
            if !file.path.is_file() {
                continue;
            }
            if let Err(err) = self.open_file(file.path.clone()) {
                eprintln!(
                    "[AppState] session restore skipped ({}): {err}",
                    file.path.display()
                );
                continue;
            }
            opened += 1;
            let _ = self.jump_to_line_col(file.line, file.col);
        }
        if let Some(active) = layout.active.as_ref()
            && let Some(idx) = self.buffers.iter().position(|entry| {
                matches!(&entry.content, BufferContent::Text(buf) if &buf.path == active)
            })
        {
            let _ = self.activate_buffer_index(idx);
        }
        opened
    }
}
