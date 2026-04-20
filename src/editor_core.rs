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
}
