use std::{
    fs,
    path::{Path, PathBuf},
};

use ropey::Rope;

/// Editor core buffer tối thiểu cho single-file workflow.
#[derive(Debug, Clone)]
pub struct EditorBuffer {
    pub text: Rope,
    pub cursor_char_idx: usize,
    pub target_col: usize,
    pub active_file: Option<PathBuf>,
    pub file_dirty: bool,
}

impl EditorBuffer {
    pub fn new() -> Self {
        Self {
            text: Rope::new(),
            cursor_char_idx: 0,
            target_col: 0,
            active_file: None,
            file_dirty: false,
        }
    }

    pub fn from_str(content: &str) -> Self {
        Self {
            text: Rope::from(content),
            cursor_char_idx: 0,
            target_col: 0,
            active_file: None,
            file_dirty: false,
        }
    }

    pub fn filetype_label(&self) -> &'static str {
        self.active_file
            .as_deref()
            .map(filetype_label_for_path)
            .unwrap_or("Plain Text")
    }

    pub fn current_position(&self) -> (usize, usize) {
        let line_idx = self.text.char_to_line(self.cursor_char_idx);
        let line_start = self.text.line_to_char(line_idx);
        let col_idx = self.cursor_char_idx - line_start;
        (line_idx, col_idx)
    }

    pub fn insert_char(&mut self, ch: char) {
        self.text.insert_char(self.cursor_char_idx, ch);
        self.cursor_char_idx += 1;
        let (_, col) = self.current_position();
        self.target_col = col;
        self.file_dirty = true;
    }

    pub fn insert_newline(&mut self) {
        self.insert_char('\n');
    }

    pub fn delete_backward(&mut self) {
        if self.cursor_char_idx == 0 {
            return;
        }

        let start = self.cursor_char_idx - 1;
        self.text.remove(start..self.cursor_char_idx);
        self.cursor_char_idx -= 1;
        let (_, col) = self.current_position();
        self.target_col = col;
        self.file_dirty = true;
    }

    pub fn move_left(&mut self) {
        let (line_idx, col) = self.current_position();
        if col > 0 {
            self.cursor_char_idx -= 1;
            self.target_col = col - 1;
            return;
        }

        if line_idx > 0 {
            // Đầu dòng -> cuối dòng trước.
            self.cursor_char_idx -= 1;
            self.target_col = self.max_col_for_line(line_idx - 1);
        }
    }

    pub fn move_right(&mut self) {
        let (line_idx, col) = self.current_position();
        let max_col = self.max_col_for_line(line_idx);

        // Lưu ý quan trọng: cho phép tới vị trí sau ký tự cuối dòng
        // với dòng không có '\n' (max_col = chars).
        if col < max_col {
            self.cursor_char_idx += 1;
            self.target_col = col + 1;
            return;
        }

        // Cuối dòng -> đầu dòng kế tiếp (nếu có).
        let total_lines = self.text.len_lines();
        if line_idx + 1 < total_lines {
            self.cursor_char_idx = self.text.line_to_char(line_idx + 1);
            self.target_col = 0;
        }
    }

    pub fn move_up(&mut self) {
        let (line_idx, _) = self.current_position();
        if line_idx == 0 {
            return;
        }

        let prev_line = line_idx - 1;
        let prev_line_start = self.text.line_to_char(prev_line);
        let new_col = self.target_col.min(self.max_col_for_line(prev_line));
        self.cursor_char_idx = prev_line_start + new_col;
    }

    pub fn move_down(&mut self) {
        let (line_idx, _) = self.current_position();
        let total_lines = self.text.len_lines();
        if line_idx + 1 >= total_lines {
            return;
        }

        let next_line = line_idx + 1;
        let next_line_start = self.text.line_to_char(next_line);
        let new_col = self.target_col.min(self.max_col_for_line(next_line));
        self.cursor_char_idx = next_line_start + new_col;
    }

    pub fn move_word_forward(&mut self) -> bool {
        let next_idx = next_word_start(&self.text, self.cursor_char_idx);
        if next_idx == self.cursor_char_idx {
            return false;
        }

        self.cursor_char_idx = next_idx.min(self.text.len_chars());
        let (_, col) = self.current_position();
        self.target_col = col;
        true
    }

    pub fn move_word_backward(&mut self) -> bool {
        let next_idx = previous_word_start(&self.text, self.cursor_char_idx);
        if next_idx == self.cursor_char_idx {
            return false;
        }

        self.cursor_char_idx = next_idx;
        let (_, col) = self.current_position();
        self.target_col = col;
        true
    }

    pub fn move_word_end(&mut self) -> bool {
        let Some(next_idx) = word_end_at_or_after(&self.text, self.cursor_char_idx) else {
            return false;
        };
        if next_idx == self.cursor_char_idx {
            return false;
        }

        self.cursor_char_idx = next_idx;
        let (_, col) = self.current_position();
        self.target_col = col;
        true
    }

    pub fn append_after_cursor(&mut self) -> bool {
        let line_idx = self.text.char_to_line(self.cursor_char_idx);
        let line_end = self.line_content_end_char_idx(line_idx);
        let target_idx = if self.cursor_char_idx < line_end {
            self.cursor_char_idx + 1
        } else {
            line_end
        };

        if target_idx == self.cursor_char_idx {
            return false;
        }

        self.cursor_char_idx = target_idx.min(self.text.len_chars());
        let (_, col) = self.current_position();
        self.target_col = col;
        true
    }

    pub fn move_to_line_start(&mut self) -> bool {
        let line_idx = self.text.char_to_line(self.cursor_char_idx);
        let target_idx = self.text.line_to_char(line_idx);
        if target_idx == self.cursor_char_idx {
            return false;
        }

        self.cursor_char_idx = target_idx;
        self.target_col = 0;
        true
    }

    pub fn move_to_line_end(&mut self) -> bool {
        let line_idx = self.text.char_to_line(self.cursor_char_idx);
        let target_idx = self.line_content_end_char_idx(line_idx);
        if target_idx == self.cursor_char_idx {
            return false;
        }

        self.cursor_char_idx = target_idx;
        let (_, col) = self.current_position();
        self.target_col = col;
        true
    }

    pub fn move_to_first_non_whitespace(&mut self) -> bool {
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

        if target_idx == self.cursor_char_idx {
            return false;
        }

        self.cursor_char_idx = target_idx;
        let (_, col) = self.current_position();
        self.target_col = col;
        true
    }

    pub fn move_to_first_line(&mut self) {
        self.cursor_char_idx = 0;
        self.target_col = 0;
    }

    pub fn move_to_last_line(&mut self) {
        let total = self.text.len_lines();
        let last = if total > 0 { total - 1 } else { 0 };
        self.cursor_char_idx = self.text.line_to_char(last);
        self.target_col = 0;
    }

    pub fn insert_line_below(&mut self) -> bool {
        let line_idx = self.text.char_to_line(self.cursor_char_idx);
        let insert_at = self.line_content_end_char_idx(line_idx);

        self.text.insert_char(insert_at, '\n');
        let target_line = (line_idx + 1).min(self.text.len_lines().saturating_sub(1));
        self.cursor_char_idx = self.text.line_to_char(target_line);
        self.target_col = 0;
        self.file_dirty = true;
        true
    }

    pub fn insert_line_above(&mut self) -> bool {
        let line_idx = self.text.char_to_line(self.cursor_char_idx);
        let line_start = self.text.line_to_char(line_idx);

        self.text.insert_char(line_start, '\n');
        self.cursor_char_idx = line_start;
        self.target_col = 0;
        self.file_dirty = true;
        true
    }

    pub fn insert_at_line_start(&mut self) -> bool {
        self.move_to_first_non_whitespace()
    }

    pub fn append_at_line_end(&mut self) -> bool {
        self.move_to_line_end()
    }

    pub fn substitute_line(&mut self) -> bool {
        let line_idx = self.text.char_to_line(self.cursor_char_idx);
        let line_start = self.text.line_to_char(line_idx);
        let line_end = self.line_content_end_char_idx(line_idx);
        let old_cursor = self.cursor_char_idx;

        let text_changed = line_end > line_start;
        if text_changed {
            self.text.remove(line_start..line_end);
            self.file_dirty = true;
        }

        self.cursor_char_idx = line_start.min(self.text.len_chars());
        let (_, col) = self.current_position();
        self.target_col = col;

        let cursor_changed = self.cursor_char_idx != old_cursor;
        text_changed || cursor_changed
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

        self.text.remove(delete_idx..(delete_idx + 1));
        self.cursor_char_idx = delete_idx.min(self.text.len_chars());
        let (_, col) = self.current_position();
        self.target_col = col;
        self.file_dirty = true;
        true
    }

    pub fn delete_word_forward(&mut self) -> bool {
        let n = self.text.len_chars();
        if self.cursor_char_idx >= n {
            return false;
        }

        let end = next_word_start(&self.text, self.cursor_char_idx);
        if end <= self.cursor_char_idx {
            return false;
        }

        self.text.remove(self.cursor_char_idx..end);
        self.cursor_char_idx = self.cursor_char_idx.min(self.text.len_chars());
        let (_, col) = self.current_position();
        self.target_col = col;
        self.file_dirty = true;
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

        self.text.remove(start..self.cursor_char_idx);
        self.cursor_char_idx = start;
        let (_, col) = self.current_position();
        self.target_col = col;
        self.file_dirty = true;
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

        self.text.remove(self.cursor_char_idx..end);
        self.cursor_char_idx = self.cursor_char_idx.min(self.text.len_chars());
        let (_, col) = self.current_position();
        self.target_col = col;
        self.file_dirty = true;
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

        self.text.remove(start..self.cursor_char_idx);
        self.cursor_char_idx = start;
        let (_, col) = self.current_position();
        self.target_col = col;
        self.file_dirty = true;
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

        self.text.remove(replace_idx..(replace_idx + 1));
        self.text.insert_char(replace_idx, ch);
        self.cursor_char_idx = replace_idx.min(self.text.len_chars().saturating_sub(1));
        let (_, col) = self.current_position();
        self.target_col = col;
        self.file_dirty = true;
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

        if delete_start == line_end && line_idx > 0 {
            delete_start = delete_start.saturating_sub(1);
            line_end = line_end.max(delete_start);
        }

        if delete_start == line_end {
            return false;
        }

        self.text.remove(delete_start..line_end);
        let remaining_lines = self.text.len_lines();
        let target_line = line_idx.min(remaining_lines.saturating_sub(1));
        self.cursor_char_idx = self.text.line_to_char(target_line);
        let (_, col) = self.current_position();
        self.target_col = col;
        self.file_dirty = true;
        true
    }

    pub fn open_file(&mut self, path: impl AsRef<Path>) -> Result<(), String> {
        let path = path.as_ref();
        let content = fs::read_to_string(path)
            .map_err(|err| format!("open file {:?} failed: {err}", path))?;

        self.text = Rope::from(content.as_str());
        self.cursor_char_idx = 0;
        self.target_col = 0;
        self.active_file = Some(path.to_path_buf());
        self.file_dirty = false;
        Ok(())
    }

    pub fn save_file(&mut self) -> Result<PathBuf, String> {
        let path = self
            .active_file
            .clone()
            .ok_or_else(|| "save failed: active file is not set".to_string())?;

        fs::write(&path, self.text.to_string())
            .map_err(|err| format!("save file {:?} failed: {err}", path))?;
        self.file_dirty = false;
        Ok(path)
    }

    pub fn save_file_as(&mut self, path: impl AsRef<Path>) -> Result<PathBuf, String> {
        let path = path.as_ref().to_path_buf();
        fs::write(&path, self.text.to_string())
            .map_err(|err| format!("save file {:?} failed: {err}", path))?;
        self.active_file = Some(path.clone());
        self.file_dirty = false;
        Ok(path)
    }

    pub fn to_string(&self) -> String {
        self.text.to_string()
    }

    fn line_content_end_char_idx(&self, line_idx: usize) -> usize {
        let clamped_line = line_idx.min(self.text.len_lines().saturating_sub(1));
        let line_start = self.text.line_to_char(clamped_line);
        line_start + self.max_col_for_line(clamped_line)
    }

    fn max_col_for_line(&self, line_idx: usize) -> usize {
        let line = self.text.line(line_idx);
        let len_chars = line.len_chars();
        if len_chars == 0 {
            return 0;
        }

        // Nếu có '\n' ở cuối dòng -> không cho cursor đứng sau '\n'.
        if line.char(len_chars - 1) == '\n' {
            len_chars - 1
        } else {
            // Dòng cuối không có newline -> cho phép caret tới EOF.
            len_chars
        }
    }
}

pub fn filetype_label_for_path(path: &Path) -> &'static str {
    match path
        .extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| ext.to_ascii_lowercase())
        .as_deref()
    {
        Some("rs") => "Rust",
        Some("js") | Some("mjs") | Some("cjs") => "JavaScript",
        Some("jsx") => "JavaScript React",
        Some("ts") => "TypeScript",
        Some("tsx") => "TypeScript React",
        Some("py") => "Python",
        Some("go") => "Go",
        Some("json") => "JSON",
        Some("toml") => "TOML",
        Some("md") => "Markdown",
        Some("txt") => "Text",
        Some("yml") | Some("yaml") => "YAML",
        Some("sh") | Some("bash") | Some("zsh") => "Shell",
        Some("html") => "HTML",
        Some("css") => "CSS",
        _ => "Plain Text",
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum WordClass {
    Word,
    Punct,
    Space,
    Newline,
}

fn classify_char(ch: char) -> WordClass {
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

fn next_word_start(text: &Rope, cursor: usize) -> usize {
    let n = text.len_chars();
    if cursor >= n {
        return cursor;
    }

    let start_class = classify_char(text.char(cursor));
    if start_class == WordClass::Newline {
        return cursor + 1;
    }

    let mut i = cursor;
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

    while i < n && classify_char(text.char(i)) == start_class {
        i += 1;
    }
    while i < n && classify_char(text.char(i)) == WordClass::Space {
        i += 1;
    }
    i
}

fn previous_word_start(text: &Rope, cursor: usize) -> usize {
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

fn word_end_at_or_after(text: &Rope, cursor: usize) -> Option<usize> {
    let n = text.len_chars();
    if n == 0 || cursor >= n {
        return None;
    }

    let mut i = cursor;
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

#[cfg(test)]
mod tests {
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::*;

    fn unique_temp_path(suffix: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock drift")
            .as_nanos();
        std::env::temp_dir().join(format!("netherize_editor_core_{suffix}_{nanos}.txt"))
    }

    fn setup_buffer() -> EditorBuffer {
        EditorBuffer::from_str("12345\n12\n1234567")
    }

    #[test]
    fn test_horizontal_movement() {
        let mut buf = setup_buffer();
        assert_eq!(buf.current_position(), (0, 0));

        buf.move_right();
        buf.move_right();
        buf.move_right();
        assert_eq!(buf.current_position(), (0, 3));
        assert_eq!(buf.target_col, 3);

        buf.move_left();
        assert_eq!(buf.current_position(), (0, 2));
        assert_eq!(buf.target_col, 2);
    }

    #[test]
    fn test_vertical_movement_with_target_col_memory() {
        let mut buf = setup_buffer();

        for _ in 0..5 {
            buf.move_right();
        }
        assert_eq!(buf.current_position(), (0, 5));
        assert_eq!(buf.target_col, 5);

        buf.move_down();
        assert_eq!(buf.current_position(), (1, 2));
        assert_eq!(buf.target_col, 5);

        buf.move_down();
        assert_eq!(buf.current_position(), (2, 5));
        assert_eq!(buf.target_col, 5);

        buf.move_up();
        assert_eq!(buf.current_position(), (1, 2));

        buf.move_left();
        assert_eq!(buf.current_position(), (1, 1));
        assert_eq!(buf.target_col, 1);

        buf.move_down();
        assert_eq!(buf.current_position(), (2, 1));
    }

    #[test]
    fn test_insert_updates_target_col() {
        let mut buf = setup_buffer();
        buf.move_down();
        buf.move_right();
        assert_eq!(buf.current_position(), (1, 1));

        buf.insert_char('A');
        assert_eq!(buf.current_position(), (1, 2));
        assert_eq!(buf.target_col, 2);
        assert!(buf.file_dirty);
    }

    #[test]
    fn move_right_allows_eof_on_last_line_without_newline() {
        let mut buf = EditorBuffer::from_str("abc");
        buf.move_right();
        buf.move_right();
        buf.move_right();
        assert_eq!(buf.current_position(), (0, 3));

        buf.move_right();
        assert_eq!(buf.current_position(), (0, 3));
    }

    #[test]
    fn move_right_crosses_to_next_line() {
        let mut buf = EditorBuffer::from_str("abc\ndef");
        buf.move_right();
        buf.move_right();
        buf.move_right();
        assert_eq!(buf.current_position(), (0, 3));

        buf.move_right();
        assert_eq!(buf.current_position(), (1, 0));
    }

    #[test]
    fn open_and_save_roundtrip() {
        let src = unique_temp_path("src");
        let dst = unique_temp_path("dst");
        fs::write(&src, "hello").expect("write src");

        let mut buf = EditorBuffer::new();
        buf.open_file(&src).expect("open");
        assert_eq!(buf.current_position(), (0, 0));
        assert!(!buf.file_dirty);

        buf.insert_newline();
        buf.insert_char('x');
        assert!(buf.file_dirty);

        let saved = buf.save_file_as(&dst).expect("save as");
        assert_eq!(saved, dst);
        assert!(!buf.file_dirty);
        assert_eq!(fs::read_to_string(&dst).expect("read dst"), "\nxhello");

        let _ = fs::remove_file(src);
        let _ = fs::remove_file(dst);
    }

    #[test]
    fn vim_word_motions_and_line_motions_work() {
        let mut buf = EditorBuffer::from_str("   foo bar");
        assert!(buf.move_to_first_non_whitespace());
        assert_eq!(buf.current_position(), (0, 3));

        assert!(buf.move_word_end());
        assert_eq!(buf.current_position(), (0, 5));

        assert!(buf.move_word_forward());
        assert_eq!(buf.current_position(), (0, 7));

        assert!(buf.move_word_backward());
        assert_eq!(buf.current_position(), (0, 3));

        assert!(buf.move_to_line_start());
        assert_eq!(buf.current_position(), (0, 0));

        assert!(buf.move_to_line_end());
        assert_eq!(buf.current_position(), (0, 10));
    }

    #[test]
    fn vim_line_edits_work() {
        let mut buf = EditorBuffer::from_str("foo\nbar");
        assert!(buf.insert_line_below());
        assert_eq!(buf.to_string(), "foo\n\nbar");
        assert_eq!(buf.current_position(), (1, 0));

        assert!(buf.insert_line_above());
        assert_eq!(buf.to_string(), "foo\n\n\nbar");
        assert_eq!(buf.current_position(), (1, 0));

        buf.move_to_last_line();
        assert_eq!(buf.current_position(), (3, 0));
        assert!(buf.substitute_line());
        assert_eq!(buf.to_string(), "foo\n\n\n");
        assert_eq!(buf.current_position(), (3, 0));
    }

    #[test]
    fn vim_append_change_and_replace_ops_work() {
        let mut buf = EditorBuffer::from_str("foo   bar");
        assert!(buf.append_after_cursor());
        assert_eq!(buf.current_position(), (0, 1));

        buf.cursor_char_idx = 0;
        buf.target_col = 0;
        assert!(buf.change_word_forward());
        assert_eq!(buf.to_string(), "bar");
        assert_eq!(buf.current_position(), (0, 0));
        assert!(buf.file_dirty);

        let mut buf = EditorBuffer::from_str("foo   bar");
        for _ in 0..6 {
            buf.move_right();
        }
        assert!(buf.change_word_backward());
        assert_eq!(buf.to_string(), "bar");
        assert_eq!(buf.current_position(), (0, 0));

        let mut buf = EditorBuffer::from_str("abc");
        assert!(buf.replace_char_at_cursor('X'));
        assert_eq!(buf.to_string(), "Xbc");
        assert_eq!(buf.current_position(), (0, 0));
    }

    #[test]
    fn vim_delete_operators_work() {
        let mut buf = EditorBuffer::from_str("foo   bar\nline2");
        assert!(buf.delete_word_forward());
        assert_eq!(buf.to_string(), "bar\nline2");
        assert_eq!(buf.current_position(), (0, 0));

        for _ in 0..3 {
            buf.move_right();
        }
        assert!(buf.delete_word_backward());
        assert_eq!(buf.to_string(), "\nline2");
        assert_eq!(buf.current_position(), (0, 0));

        assert!(buf.delete_current_line());
        assert_eq!(buf.to_string(), "line2");
    }

    #[test]
    fn filetype_label_uses_active_file_extension() {
        let mut buf = EditorBuffer::new();
        assert_eq!(buf.filetype_label(), "Plain Text");

        buf.active_file = Some(PathBuf::from("src/main.rs"));
        assert_eq!(buf.filetype_label(), "Rust");

        buf.active_file = Some(PathBuf::from("web/app.js"));
        assert_eq!(buf.filetype_label(), "JavaScript");
    }
}
