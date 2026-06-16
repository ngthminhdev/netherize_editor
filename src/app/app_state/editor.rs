use super::overlays::{
    matches_matching_bracket_pair, matching_close_char, next_word_start, previous_word_start,
    word_end_at_or_after,
};
use super::*;

impl AppState {
    pub fn insert_tab(&mut self) -> bool {
        let text = if self.indent_config.insert_spaces {
            " ".repeat(self.indent_config.tab_width as usize)
        } else {
            "\t".to_string()
        };
        let char_count = text.chars().count();
        if !self.apply_insert(self.cursor_char_idx, text) {
            return false;
        }
        self.cursor_char_idx += char_count;
        let (_, col) = self.cursor_line_col();
        self.target_col = col;
        self.dirty = true;
        self.bump_revision();
        true
    }

    pub fn insert_char(&mut self, ch: char) {
        self.apply_insert(self.cursor_char_idx, ch.to_string());
        self.cursor_char_idx += 1;
        let (_, col) = self.cursor_line_col();
        self.target_col = col;
        self.dirty = true;
        self.bump_revision();
    }

    pub fn step_over_closing_char(&mut self, ch: char) -> bool {
        if self.char_at_cursor() != Some(ch) {
            return false;
        }

        let before_cursor = self.cursor_char_idx;
        self.move_right();
        if self.cursor_char_idx == before_cursor {
            return false;
        }

        self.dirty = true;
        self.bump_revision();
        true
    }

    /// Sau khi `>` được insert, kiểm tra xem vừa đóng một opening tag chưa.
    /// Nếu có → insert closing tag và giữ cursor giữa 2 tag.
    /// Chỉ kích hoạt cho HTML / JSX / TSX.
    pub fn insert_html_auto_close_tag(&mut self) -> bool {
        let ext = self
            .active_file()
            .and_then(|p| p.extension())
            .and_then(|e| e.to_str())
            .map(|e| e.to_ascii_lowercase())
            .unwrap_or_default();
        if !matches!(ext.as_str(), "html" | "htm" | "jsx" | "tsx") {
            return false;
        }

        if self.cursor_char_idx == 0 {
            return false;
        }

        // Scan at most 512 chars back để tìm `<`
        let scan_end = self.cursor_char_idx; // đã qua `>`
        let scan_start = scan_end.saturating_sub(512);
        let chars: Vec<char> = self.text.slice(scan_start..scan_end).chars().collect();

        let lt_pos = match chars.iter().rposition(|&c| c == '<') {
            Some(p) => p,
            None => return false,
        };

        let after_lt = &chars[lt_pos + 1..];
        if after_lt.is_empty() {
            return false;
        }

        // Bỏ qua closing tag, comment, doctype, processing instruction
        match after_lt[0] {
            '/' | '!' | '?' => return false,
            c if !c.is_alphabetic() && c != '_' => return false,
            _ => {}
        }

        // Tên tag: chữ/số, `-`, `_`, `.`, `:`
        let tag_name: String = after_lt
            .iter()
            .take_while(|&&c| c.is_alphanumeric() || matches!(c, '-' | '_' | '.' | ':'))
            .collect();

        if tag_name.is_empty() {
            return false;
        }

        // Self-closing `<br />` — char trước `>` là `/`
        if chars.len() >= 2 && chars[chars.len() - 2] == '/' {
            return false;
        }

        // Void elements (HTML / HTM / JSX / TSX đều bỏ qua)
        const VOID: &[&str] = &[
            "area", "base", "br", "col", "embed", "hr", "img", "input", "link", "meta", "param",
            "source", "track", "wbr",
        ];
        if VOID.contains(&tag_name.to_ascii_lowercase().as_str()) {
            return false;
        }

        // Insert closing tag; cursor không di chuyển → ở giữa 2 tag
        let close_tag = format!("</{tag_name}>");
        self.apply_insert(self.cursor_char_idx, close_tag);
        self.dirty = true;
        self.bump_revision();
        true
    }

    pub fn insert_auto_pair(&mut self, open: char) -> bool {
        let Some(close) = matching_close_char(open) else {
            return false;
        };

        let insert_at = self.cursor_char_idx;
        if !self.apply_insert(insert_at, format!("{open}{close}")) {
            return false;
        }

        self.cursor_char_idx = insert_at + 1;
        let (_, col) = self.cursor_line_col();
        self.target_col = col;
        self.dirty = true;
        self.bump_revision();
        true
    }

    pub fn smart_insert_newline(&mut self) -> bool {
        let left = self.char_before_cursor();
        let right = self.char_at_cursor();

        if !matches_matching_bracket_pair(left, right) {
            let (line_idx, _) = self.cursor_line_col();
            let indent = self.line_indent_string(line_idx);
            let indent_char_count = indent.chars().count();
            let insert_at = self.cursor_char_idx;
            if !self.apply_insert(insert_at, format!("\n{}", indent)) {
                return false;
            }
            self.cursor_char_idx = insert_at + 1 + indent_char_count;
            let (_, col) = self.cursor_line_col();
            self.target_col = col;
            self.dirty = true;
            self.bump_revision();
            return true;
        }

        let (line_idx, _) = self.cursor_line_col();
        let current_indent = self.line_indent_string(line_idx);
        let indent_unit = self.indent_unit_for_line(&current_indent);
        let insert_at = self.cursor_char_idx;
        let inserted = format!("\n{}{}\n{}", current_indent, indent_unit, current_indent);

        if !self.apply_insert(insert_at, inserted) {
            return false;
        }

        let cursor_after_first_line_break =
            insert_at + 1 + current_indent.chars().count() + indent_unit.chars().count();
        self.cursor_char_idx = cursor_after_first_line_break;
        let (_, col) = self.cursor_line_col();
        self.target_col = col;
        self.dirty = true;
        self.bump_revision();
        true
    }

    pub fn backspace(&mut self) -> bool {
        if self.visual_selection_range().is_some() {
            return self.delete_visual_selection();
        }

        if self.cursor_char_idx == 0 {
            return false;
        }

        let start = self.cursor_char_idx - 1;
        let delete_len = match (self.char_before_cursor(), self.char_at_cursor()) {
            (Some(left), Some(right)) if matches_matching_bracket_pair(Some(left), Some(right)) => {
                2
            }
            _ => 1,
        };

        if !self.apply_delete(start, delete_len) {
            return false;
        }

        self.cursor_char_idx = start;

        let (_, col) = self.cursor_line_col();
        self.target_col = col;
        self.dirty = true;
        self.bump_revision();
        true
    }

    pub fn insert_line_below(&mut self) -> bool {
        let line_idx = self.text.char_to_line(self.cursor_char_idx);
        let insert_at = self.line_content_end_char_idx(line_idx);
        let indent = self.line_indent_string(line_idx);
        let indent_char_count = indent.chars().count();

        self.apply_insert(insert_at, format!("\n{}", indent));
        let target_line = (line_idx + 1).min(self.text.len_lines().saturating_sub(1));
        self.cursor_char_idx = self.text.line_to_char(target_line) + indent_char_count;
        let (_, col) = self.cursor_line_col();
        self.target_col = col;
        self.dirty = true;
        self.bump_revision();
        true
    }

    pub fn insert_line_above(&mut self) -> bool {
        let line_idx = self.text.char_to_line(self.cursor_char_idx);
        let line_start = self.text.line_to_char(line_idx);

        self.apply_insert(line_start, "\n".to_string());
        self.cursor_char_idx = line_start;
        self.target_col = 0;
        self.dirty = true;
        self.bump_revision();
        true
    }

    pub fn move_to_line_first_non_blank(&mut self) -> bool {
        let line_idx = self.text.char_to_line(self.cursor_char_idx);
        let line_start = self.text.line_to_char(line_idx);
        let line_end = self.line_content_end_char_idx(line_idx);

        let mut target_idx = line_start;
        for char_idx in line_start..line_end {
            let ch = self.text.char(char_idx);
            if ch != ' ' && ch != '\t' {
                target_idx = char_idx;
                break;
            }
        }

        let previous_cursor = self.cursor_char_idx;
        let changed = self.update_cursor_position(target_idx);
        if self.cursor_char_idx != previous_cursor {
            let (_, col) = self.cursor_line_col();
            self.target_col = col;
        }
        changed
    }

    pub fn move_to_line_start(&mut self) -> bool {
        let line_idx = self.text.char_to_line(self.cursor_char_idx);
        let target_idx = self.text.line_to_char(line_idx);
        let changed = self.update_cursor_position(target_idx);
        self.target_col = 0;
        changed
    }

    // ── Leap navigation helpers ────────────────────────────────────────────────

    /// Nhảy cursor trực tiếp đến char_idx (Leap jump).
    /// Sử dụng `update_cursor_position` để đảm bảo clamp và revision bump.
    pub fn leap_jump_to_char(&mut self, char_idx: usize) -> bool {
        let clamped = char_idx.min(self.text.len_chars().saturating_sub(1));
        let changed = self.update_cursor_position(clamped);
        if changed {
            let (_, col) = self.cursor_line_col();
            self.target_col = col;
            self.bump_revision();
        }
        changed
    }

    /// Trả về char_idx đầu tiên của một line (dùng để tính viewport scan range).
    pub fn char_idx_for_line(&self, line: usize) -> usize {
        let clamped = line.min(self.text.len_lines());
        self.text.line_to_char(clamped)
    }

    /// Tổng số chars trong text (để Leap biết khi nào dừng scan).
    pub fn text_len_chars(&self) -> usize {
        self.text.len_chars()
    }

    /// Convert byte offset trong một line sang char offset trong cùng line đó.
    /// Dùng bởi renderer để map cosmic-text glyph.start → char_idx trong Rope.
    pub fn byte_to_char_in_line(&self, line_idx: usize, byte_in_line: usize) -> usize {
        let line = self
            .text
            .line(line_idx.min(self.text.len_lines().saturating_sub(1)));
        // Rope::line() trả về RopeSlice — đếm chars tới byte_in_line
        let mut char_count = 0usize;
        let mut byte_count = 0usize;
        for ch in line.chars() {
            if byte_count >= byte_in_line {
                break;
            }
            byte_count += ch.len_utf8();
            char_count += 1;
        }
        char_count
    }

    pub fn move_to_first_non_whitespace(&mut self) -> bool {
        self.move_to_line_first_non_blank()
    }

    pub fn move_to_line_end(&mut self) -> bool {
        let line_idx = self.text.char_to_line(self.cursor_char_idx);
        let target_idx = self.line_content_end_char_idx(line_idx);
        let changed = self.update_cursor_position(target_idx);
        let (_, col) = self.cursor_line_col();
        self.target_col = col;
        changed
    }

    pub fn move_word_forward(&mut self) -> bool {
        let next_idx = next_word_start(&self.text, self.cursor_char_idx);
        let changed = self.update_cursor_position(next_idx);
        let (_, col) = self.cursor_line_col();
        self.target_col = col;
        changed
    }

    pub fn move_word_backward(&mut self) -> bool {
        let next_idx = previous_word_start(&self.text, self.cursor_char_idx);
        let changed = self.update_cursor_position(next_idx);
        let (_, col) = self.cursor_line_col();
        self.target_col = col;
        changed
    }

    pub fn move_word_end(&mut self) -> bool {
        let next_idx =
            word_end_at_or_after(&self.text, self.cursor_char_idx).unwrap_or(self.cursor_char_idx);
        let changed = self.update_cursor_position(next_idx);
        let (_, col) = self.cursor_line_col();
        self.target_col = col;
        changed
    }

    pub fn append_after_cursor(&mut self) -> bool {
        let line_idx = self.text.char_to_line(self.cursor_char_idx);
        let line_end = self.line_content_end_char_idx(line_idx);
        let target_idx = if self.cursor_char_idx < line_end {
            self.cursor_char_idx + 1
        } else {
            line_end
        };

        let changed = self.update_cursor_position(target_idx);
        let (_, col) = self.cursor_line_col();
        self.target_col = col;
        changed
    }

    pub fn substitute_current_line(&mut self) -> bool {
        let line_idx = self.text.char_to_line(self.cursor_char_idx);
        let line_end = self.line_content_end_char_idx(line_idx);
        let old_cursor = self.cursor_char_idx;
        let indent_target = self.first_non_blank_or_line_start(line_idx);

        let text_changed = line_end > indent_target;
        if text_changed {
            self.apply_delete(indent_target, line_end - indent_target);
            self.dirty = true;
        }

        self.cursor_char_idx = indent_target.min(self.text.len_chars());
        let (_, col) = self.cursor_line_col();
        self.target_col = col;

        let cursor_changed = self.cursor_char_idx != old_cursor;
        if text_changed || cursor_changed {
            self.bump_revision();
            return true;
        }
        false
    }

    pub fn join_line_below(&mut self) -> bool {
        let total_lines = self.text.len_lines();
        if total_lines <= 1 {
            return false;
        }
        let line_idx = self
            .text
            .char_to_line(self.cursor_char_idx.min(self.text.len_chars()));
        if line_idx + 1 >= total_lines {
            return false;
        }

        let line_end_char = if line_idx + 1 < total_lines {
            self.text.line_to_char(line_idx + 1)
        } else {
            return false;
        };
        if line_end_char == 0 || self.text.char(line_end_char.saturating_sub(1)) != '\n' {
            return false;
        }

        let next_line = line_idx + 1;
        let next_start = self.text.line_to_char(next_line);
        let next_end = self.line_content_end_char_idx(next_line);
        let has_next_text = next_start < next_end;
        let mut remove_len = 1usize;
        let mut insert_space = false;
        if has_next_text {
            let mut trim = 0usize;
            while next_start + trim < next_end {
                let ch = self.text.char(next_start + trim);
                if ch == ' ' || ch == '\t' {
                    trim += 1;
                } else {
                    break;
                }
            }
            remove_len += trim;
            let prev_end = self.line_content_end_char_idx(line_idx);
            let prev_non_blank = prev_end > 0 && self.text.char(prev_end.saturating_sub(1)) != ' ';
            insert_space = prev_non_blank;
        }

        let delete_index = line_end_char.saturating_sub(1);
        let changed = self.apply_delete(delete_index, remove_len);
        if !changed {
            return false;
        }
        if insert_space {
            let _ = self.apply_insert(delete_index, " ".to_string());
        }
        let line_end = self.line_content_end_char_idx(line_idx);
        let target = self.cursor_char_idx.min(line_end);
        self.cursor_char_idx = target;
        let (_, col) = self.cursor_line_col();
        self.target_col = col;
        self.dirty = true;
        self.bump_revision();
        true
    }

    pub(super) fn first_non_blank_or_line_start(&self, line_idx: usize) -> usize {
        let line_start = self.text.line_to_char(line_idx);
        let line_end = self.line_content_end_char_idx(line_idx);
        for idx in line_start..line_end {
            let ch = self.text.char(idx);
            if ch != ' ' && ch != '\t' {
                return idx;
            }
        }
        line_start
    }

    pub fn delete_char_at_cursor(&mut self) -> bool {
        if self.text.len_chars() == 0 {
            return false;
        }

        let line_idx = self
            .text
            .char_to_line(self.cursor_char_idx.min(self.text.len_chars()));
        let line_start = self.text.line_to_char(line_idx);
        let line_end = self.line_content_end_char_idx(line_idx);
        if line_start == line_end {
            return false;
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
            return false;
        }

        self.apply_delete(delete_idx, 1);
        self.cursor_char_idx = delete_idx.min(self.text.len_chars());
        let (_, col) = self.cursor_line_col();
        self.target_col = col;
        self.dirty = true;
        self.bump_revision();
        true
    }

    /// Vim `dw`: delete from cursor to the start of the next word on the same line,
    /// including trailing spaces/tabs. If cursor sits on a newline, delete just that
    /// newline (join with next line). No-op at EOF.
    pub fn delete_word_forward(&mut self) -> bool {
        let n = self.text.len_chars();
        if self.cursor_char_idx >= n {
            return false;
        }

        let end = next_word_start(&self.text, self.cursor_char_idx);
        if end <= self.cursor_char_idx {
            return false;
        }

        self.apply_delete(self.cursor_char_idx, end - self.cursor_char_idx);
        // Cursor stays at the same char index — the tail shifted in. Clamp in case
        // we just deleted everything after it (including a trailing newline).
        self.cursor_char_idx = self.cursor_char_idx.min(self.text.len_chars());
        let (_, col) = self.cursor_line_col();
        self.target_col = col;
        self.dirty = true;
        self.bump_revision();
        true
    }

    pub fn delete_word_backward(&mut self) -> bool {
        if self.cursor_char_idx == 0 {
            return false;
        }

        let start = previous_word_start(&self.text, self.cursor_char_idx);
        if start >= self.cursor_char_idx {
            return false;
        }

        self.apply_delete(start, self.cursor_char_idx - start);
        self.cursor_char_idx = start;
        let (_, col) = self.cursor_line_col();
        self.target_col = col;
        self.dirty = true;
        self.bump_revision();
        true
    }

    pub fn change_word_forward(&mut self) -> bool {
        let n = self.text.len_chars();
        if self.cursor_char_idx >= n {
            return false;
        }

        let end = next_word_start(&self.text, self.cursor_char_idx);
        if end <= self.cursor_char_idx {
            return false;
        }

        self.apply_delete(self.cursor_char_idx, end - self.cursor_char_idx);
        self.cursor_char_idx = self.cursor_char_idx.min(self.text.len_chars());
        let (_, col) = self.cursor_line_col();
        self.target_col = col;
        self.dirty = true;
        self.bump_revision();
        true
    }

    pub fn change_word_backward(&mut self) -> bool {
        if self.cursor_char_idx == 0 {
            return false;
        }

        let start = previous_word_start(&self.text, self.cursor_char_idx);
        if start >= self.cursor_char_idx {
            return false;
        }

        self.apply_delete(start, self.cursor_char_idx - start);
        self.cursor_char_idx = start;
        let (_, col) = self.cursor_line_col();
        self.target_col = col;
        self.dirty = true;
        self.bump_revision();
        true
    }

    pub fn replace_char_at_cursor(&mut self, ch: char) -> bool {
        if self.text.len_chars() == 0 {
            return false;
        }

        let line_idx = self
            .text
            .char_to_line(self.cursor_char_idx.min(self.text.len_chars()));
        let line_start = self.text.line_to_char(line_idx);
        let line_end = self.line_content_end_char_idx(line_idx);
        if line_start == line_end {
            return false;
        }

        let mut replace_idx = if self.cursor_char_idx < line_end {
            self.cursor_char_idx
        } else {
            line_end.saturating_sub(1)
        };
        if replace_idx < line_start {
            replace_idx = line_start;
        }
        if replace_idx >= self.text.len_chars() {
            return false;
        }

        self.apply_delete(replace_idx, 1);
        self.apply_insert(replace_idx, ch.to_string());
        self.cursor_char_idx = replace_idx.min(self.text.len_chars().saturating_sub(1));
        let (_, col) = self.cursor_line_col();
        self.target_col = col;
        self.dirty = true;
        self.bump_revision();
        true
    }

    pub fn delete_current_line(&mut self) -> bool {
        if self.text.len_lines() == 0 {
            return false;
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

        // Trailing empty line (from a final '\n') has an empty range.
        // Delete the previous '\n' so one logical line is removed.
        if delete_start == line_end && line_idx > 0 {
            delete_start = delete_start.saturating_sub(1);
            line_end = line_end.max(delete_start);
        }

        if delete_start == line_end {
            return false;
        }

        self.apply_delete(delete_start, line_end - delete_start);
        let remaining_lines = self.text.len_lines();
        let target_line = line_idx.min(remaining_lines.saturating_sub(1));
        self.cursor_char_idx = self.text.line_to_char(target_line);
        let (_, col) = self.cursor_line_col();
        self.target_col = col;
        self.dirty = true;
        self.bump_revision();
        true
    }

    pub fn operation_text(
        &self,
        target: OperationTarget,
        op: Operator,
    ) -> Option<(String, ClipboardRecordKind)> {
        let (start, end, linewise) = self.operation_range(target, op)?;
        let text = if linewise {
            self.linewise_text_for_range(start, end)?
        } else {
            self.char_range_text(start, end)?
        };
        Some((
            text,
            if linewise {
                ClipboardRecordKind::Linewise
            } else {
                ClipboardRecordKind::Charwise
            },
        ))
    }

    pub fn apply_operation(&mut self, target: OperationTarget, op: Operator) -> bool {
        let Some((start, end, linewise)) = self.operation_range(target, op) else {
            return false;
        };
        if op == Operator::Yank {
            return false;
        }

        if linewise && op == Operator::Change {
            return self.substitute_current_line();
        }

        if end <= start {
            return false;
        }

        self.apply_delete(start, end - start);
        self.cursor_char_idx = start.min(self.text.len_chars());
        let (_, col) = self.cursor_line_col();
        self.target_col = col;
        self.dirty = true;
        self.bump_revision();
        true
    }

    pub fn operation_range(
        &self,
        target: OperationTarget,
        op: Operator,
    ) -> Option<(usize, usize, bool)> {
        match target {
            OperationTarget::CurrentLine => {
                let (start, end) = self.current_line_delete_range()?;
                Some((start, end, true))
            }
            OperationTarget::TextObject { modifier, kind } => {
                let (start, end) =
                    find_text_object_range(&self.text, self.cursor_char_idx, modifier, kind)?;
                Some((start, end, false))
            }
            OperationTarget::Motion(motion) => self.motion_range(motion, op),
        }
    }

    pub(super) fn motion_range(
        &self,
        motion: Motion,
        op: Operator,
    ) -> Option<(usize, usize, bool)> {
        let cursor = self.cursor_char_idx.min(self.text.len_chars());
        match motion {
            Motion::WordForward => {
                let end = if op == Operator::Change {
                    word_end_at_or_after(&self.text, cursor).map(|idx| idx.saturating_add(1))?
                } else {
                    next_word_start(&self.text, cursor)
                };
                (end > cursor).then_some((cursor, end, false))
            }
            Motion::WordBackward => {
                let start = previous_word_start(&self.text, cursor);
                (start < cursor).then_some((start, cursor, false))
            }
            Motion::WordEnd => {
                let end = word_end_at_or_after(&self.text, cursor)?.saturating_add(1);
                (end > cursor).then_some((cursor, end, false))
            }
            Motion::LineStart => {
                let line = self
                    .text
                    .char_to_line(cursor.min(self.text.len_chars().saturating_sub(1).max(0)));
                let start = self.text.line_to_char(line);
                (start < cursor).then_some((start, cursor, false))
            }
            Motion::LineEnd => {
                let line = self
                    .text
                    .char_to_line(cursor.min(self.text.len_chars().saturating_sub(1).max(0)));
                let end = self.line_content_end_char_idx(line);
                (end > cursor).then_some((cursor, end, false))
            }
            Motion::FirstNonWhitespace => {
                let line = self
                    .text
                    .char_to_line(cursor.min(self.text.len_chars().saturating_sub(1).max(0)));
                let line_start = self.text.line_to_char(line);
                let line_end = self.line_content_end_char_idx(line);
                let mut target = line_start;
                for idx in line_start..line_end {
                    let ch = self.text.char(idx);
                    if ch != ' ' && ch != '\t' {
                        target = idx;
                        break;
                    }
                }
                if target < cursor {
                    Some((target, cursor, false))
                } else if target > cursor {
                    Some((cursor, target, false))
                } else {
                    None
                }
            }
            Motion::FirstLine => Some((0, cursor, false)).filter(|(s, e, _)| s < e),
            Motion::LastLine => {
                Some((cursor, self.text.len_chars(), false)).filter(|(s, e, _)| s < e)
            }
            Motion::FindChar(kind, target) => self.find_char_motion_range(kind, target),
        }
    }

    pub(super) fn find_char_motion_range(
        &self,
        kind: FindMotionKind,
        target: char,
    ) -> Option<(usize, usize, bool)> {
        if self.text.len_chars() == 0 {
            return None;
        }
        let cursor = self
            .cursor_char_idx
            .min(self.text.len_chars().saturating_sub(1));
        let line = self.text.char_to_line(cursor);
        let line_start = self.text.line_to_char(line);
        let line_end = self.line_content_end_char_idx(line);
        match kind {
            FindMotionKind::ForwardTo => {
                let hit = ((cursor + 1).min(line_end)..line_end)
                    .find(|&i| self.text.char(i) == target)?;
                Some((cursor, hit + 1, false))
            }
            FindMotionKind::ForwardTill => {
                let hit = ((cursor + 1).min(line_end)..line_end)
                    .find(|&i| self.text.char(i) == target)?;
                (hit > cursor).then_some((cursor, hit, false))
            }
            FindMotionKind::BackwardTo => {
                let hit = (line_start..cursor)
                    .rev()
                    .find(|&i| self.text.char(i) == target)?;
                Some((hit, cursor + 1, false))
            }
            FindMotionKind::BackwardTill => {
                let hit = (line_start..cursor)
                    .rev()
                    .find(|&i| self.text.char(i) == target)?;
                (hit + 1 <= cursor).then_some((hit + 1, cursor + 1, false))
            }
        }
    }

    /// Vim f/F/t/T: tìm `target` trên dòng hiện tại và di chuyển cursor tới đó
    /// (f/F đứng trên ký tự, t/T đứng cạnh ký tự). Trả về true nếu cursor di chuyển.
    pub fn move_find_char(&mut self, kind: FindMotionKind, target: char) -> bool {
        if self.text.len_chars() == 0 {
            return false;
        }
        let cursor = self
            .cursor_char_idx
            .min(self.text.len_chars().saturating_sub(1));
        let line = self.text.char_to_line(cursor);
        let line_start = self.text.line_to_char(line);
        let line_end = self.line_content_end_char_idx(line);
        let dest = match kind {
            FindMotionKind::ForwardTo => {
                ((cursor + 1).min(line_end)..line_end).find(|&i| self.text.char(i) == target)
            }
            FindMotionKind::ForwardTill => ((cursor + 1).min(line_end)..line_end)
                .find(|&i| self.text.char(i) == target)
                .map(|hit| hit.saturating_sub(1))
                .filter(|&pos| pos > cursor),
            FindMotionKind::BackwardTo => (line_start..cursor)
                .rev()
                .find(|&i| self.text.char(i) == target),
            FindMotionKind::BackwardTill => (line_start..cursor)
                .rev()
                .find(|&i| self.text.char(i) == target)
                .map(|hit| hit + 1)
                .filter(|&pos| pos < cursor),
        };
        let Some(dest) = dest else {
            return false;
        };
        self.move_cursor_to_char_idx(dest)
    }

    pub fn move_left(&mut self) {
        let (line_idx, col) = self.cursor_line_col();
        let previous_cursor = self.cursor_char_idx;
        let mut target_idx = self.cursor_char_idx;
        let mut next_target_col = self.target_col;

        if col > 0 {
            target_idx = self.cursor_char_idx.saturating_sub(1);
            next_target_col = col - 1;
        } else if line_idx > 0 {
            // Ở đầu dòng và đi trái -> sang cuối dòng trước.
            let target_line = self.previous_visible_line_before(line_idx);
            target_idx = self.text.line_to_char(target_line) + self.max_col_for_line(target_line);
            next_target_col = self.max_col_for_line(target_line);
        }

        let _ = self.update_cursor_position(target_idx);
        if self.cursor_char_idx != previous_cursor {
            self.target_col = next_target_col;
        }
    }

    pub fn move_right(&mut self) {
        let (line_idx, col) = self.cursor_line_col();
        let max_col = self.max_col_for_line(line_idx);
        let previous_cursor = self.cursor_char_idx;
        let mut target_idx = self.cursor_char_idx;
        let mut next_target_col = self.target_col;
        if col < max_col {
            target_idx = self.cursor_char_idx + 1;
            next_target_col = col + 1;
        } else {
            // Ở cuối dòng, ArrowRight sẽ nhảy sang đầu dòng kế tiếp (nếu có).
            let target_line = self.next_visible_line_after(line_idx);
            if target_line != line_idx {
                target_idx = self.text.line_to_char(target_line);
                next_target_col = 0;
            }
        }

        let _ = self.update_cursor_position(target_idx);
        if self.cursor_char_idx != previous_cursor {
            self.target_col = next_target_col;
        }
    }

    pub fn move_up(&mut self) {
        let (line_idx, _) = self.cursor_line_col();
        let next_idx = if line_idx == 0 {
            self.cursor_char_idx
        } else {
            let target_line = self.previous_visible_line_before(line_idx);
            let line_start = self.text.line_to_char(target_line);
            let new_col = self.target_col.min(self.max_col_for_line(target_line));
            line_start + new_col
        };
        let _ = self.update_cursor_position(next_idx);
    }

    pub fn move_down(&mut self) {
        let (line_idx, _) = self.cursor_line_col();
        let target_line = self.next_visible_line_after(line_idx);
        let next_idx = if target_line == line_idx {
            self.cursor_char_idx
        } else {
            let line_start = self.text.line_to_char(target_line);
            let new_col = self.target_col.min(self.max_col_for_line(target_line));
            line_start + new_col
        };
        let _ = self.update_cursor_position(next_idx);
    }

    pub fn total_lines(&self) -> usize {
        self.text.len_lines()
    }

    pub fn move_to_first_line(&mut self) -> bool {
        let cursor_changed = self.update_cursor_position(0);
        let scroll_changed = if self.target_scroll_y != 0.0 {
            self.target_scroll_y = 0.0;
            true
        } else {
            false
        };
        self.target_col = 0;
        if scroll_changed && !cursor_changed {
            self.bump_revision();
        }
        cursor_changed || scroll_changed
    }

    pub fn move_to_last_line(&mut self) -> bool {
        let total = self.text.len_lines();
        let last = if total > 0 { total - 1 } else { 0 };
        let changed = self.update_cursor_position(self.text.line_to_char(last));
        self.target_col = 0;
        changed
    }

    pub fn move_paragraph_up(&mut self) -> bool {
        let total = self.text.len_lines();
        if total == 0 {
            return false;
        }
        let current_line = self
            .text
            .char_to_line(self.cursor_char_idx.min(self.text.len_chars()));
        let mut line = current_line.saturating_sub(1);

        if self.line_is_blank(current_line.min(total.saturating_sub(1))) {
            while line > 0 && self.line_is_blank(line) {
                line = line.saturating_sub(1);
            }
        }
        while line > 0 && !self.line_is_blank(line) {
            line = line.saturating_sub(1);
        }

        let changed = self.update_cursor_position(self.text.line_to_char(line));
        self.target_col = 0;
        changed
    }

    pub fn move_paragraph_down(&mut self) -> bool {
        let total = self.text.len_lines();
        if total == 0 {
            return false;
        }
        let current_line = self
            .text
            .char_to_line(self.cursor_char_idx.min(self.text.len_chars()));
        let mut line = current_line;

        if self.line_is_blank(line) {
            while line + 1 < total && self.line_is_blank(line) {
                line += 1;
            }
        }
        while line + 1 < total && !self.line_is_blank(line) {
            line += 1;
        }

        let changed =
            self.update_cursor_position(self.text.line_to_char(line.min(total.saturating_sub(1))));
        self.target_col = 0;
        changed
    }

    pub(super) fn line_is_blank(&self, line_idx: usize) -> bool {
        let start = self
            .text
            .line_to_char(line_idx.min(self.text.len_lines().saturating_sub(1)));
        let end =
            self.line_content_end_char_idx(line_idx.min(self.text.len_lines().saturating_sub(1)));
        self.text
            .slice(start..end)
            .chars()
            .all(|ch| ch == ' ' || ch == '\t')
    }

    /// Nhảy đến `line_idx` (0-indexed). Dùng bởi `:N` vim command.
    /// Trả về true nếu cursor thực sự thay đổi.
    pub fn jump_to_line(&mut self, line_idx: usize) -> bool {
        self.jump_to_line_col(line_idx, 0)
    }

    /// Jump to a 0-indexed line and character column, preserving the target column
    /// for subsequent vertical motions and jump-list restoration.
    pub fn jump_to_line_col(&mut self, line_idx: usize, col_idx: usize) -> bool {
        let total = self.text.len_lines();
        let target_line = line_idx.min(total.saturating_sub(1));
        let line_start = self.text.line_to_char(target_line);
        let max_col = self.max_col_for_line(target_line);
        let target_col = col_idx.min(max_col);
        let char_idx = line_start + target_col;
        let changed = self.update_cursor_position(char_idx);
        let target_changed = self.target_col != target_col;
        self.target_col = target_col;
        // Scroll: đặt target_line vào giữa màn hình nếu scroll_line cần update
        if changed || target_changed {
            // Dùng auto_scroll_to_cursor sẽ được gọi bởi renderer
            // Ở đây chỉ reset scroll_line về target_line để viewport thấy dòng đó
            self.target_scroll_y = target_line.saturating_sub(10) as f32;
            self.bump_revision();
        }
        changed || target_changed
    }

    pub fn center_cursor_line(&mut self, viewport_lines: usize) {
        let (cursor_line, _) = self.cursor_line_col();
        self.target_scroll_y = cursor_line.saturating_sub(viewport_lines / 2) as f32;
        self.current_scroll_y = self.target_scroll_y;
    }

    pub fn scroll_half_page_up(&mut self, half: usize) {
        self.target_scroll_y = (self.target_scroll_y - half as f32).max(0.0);
        self.current_scroll_y = self.target_scroll_y;
        let new_line = self.cursor_line_col().0.saturating_sub(half);
        self.cursor_char_idx = self.text.line_to_char(new_line);
        self.target_col = 0;
        self.bump_revision();
    }

    pub fn scroll_half_page_down(&mut self, half: usize) {
        let total = self.text.len_lines();
        self.target_scroll_y =
            (self.target_scroll_y + half as f32).min(total.saturating_sub(1) as f32);
        self.current_scroll_y = self.target_scroll_y;
        let new_line = (self.cursor_line_col().0 + half).min(total.saturating_sub(1));
        self.cursor_char_idx = self.text.line_to_char(new_line);
        self.target_col = 0;
        self.bump_revision();
    }

    /// Adjust target_scroll_y so the cursor is within the viewport.
    pub fn auto_scroll_to_cursor(&mut self, viewport_lines: usize) {
        let (raw_cursor_line, _) = self.cursor_line_col();

        // If the cursor is on a hidden line, use the visible marker line instead.
        // This prevents viewport jitter when navigating through folded regions.
        let cursor_line = self
            .fold_marker_line_for_hidden_line(raw_cursor_line)
            .unwrap_or(raw_cursor_line);

        let margin = 3usize;
        let target_line = self.target_scroll_y.floor().max(0.0) as usize;
        if cursor_line < target_line + margin {
            self.target_scroll_y = cursor_line.saturating_sub(margin) as f32;
        } else if viewport_lines > margin && cursor_line + margin >= target_line + viewport_lines {
            self.target_scroll_y = (cursor_line + margin + 1 - viewport_lines) as f32;
        }
    }

    pub fn scroll_line(&self) -> usize {
        self.target_scroll_y.floor().max(0.0) as usize
    }

    pub fn set_target_scroll_line(&mut self, line: usize) {
        self.target_scroll_y = line.min(self.text.len_lines().saturating_sub(1)) as f32;
    }

    pub fn snap_current_scroll_to_target(&mut self) {
        self.current_scroll_y = self.target_scroll_y;
    }
}
