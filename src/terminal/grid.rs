//! Terminal Grid — Phase 9b.
//!
//! `TerminalGrid` là model lưu trữ toàn bộ nội dung terminal ở dạng 2D cells.
//! Mỗi cell giữ một `char` và một `CellStyle` (fg/bg color + bold flag).
//!
//! # Cách dùng
//!
//! ```rust
//! use netherize_editor::terminal::grid::TerminalGrid;
//!
//! let mut grid = TerminalGrid::new(80, 24);
//! grid.feed_chunk("\x1b[32mHello\x1b[0m\r\nworld");
//! let cell = grid.cell_at(0, 0);
//! // cell.ch == 'H', cell.style.fg == Index(2) (green)
//! ```

use crate::terminal::ansi_parser::{AnsiColor, AnsiEvent, AnsiParser};

// ─── Kiểu dữ liệu ────────────────────────────────────────────────────────────

/// Style của một terminal cell.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CellStyle {
    pub fg: AnsiColor,
    pub bg: AnsiColor,
    pub bold: bool,
}

impl Default for CellStyle {
    fn default() -> Self {
        Self {
            fg: AnsiColor::Default,
            bg: AnsiColor::Default,
            bold: false,
        }
    }
}

/// Một cell trong terminal grid.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TerminalCell {
    /// Ký tự hiển thị. `' '` = cell trống.
    pub ch: char,
    pub style: CellStyle,
}

impl Default for TerminalCell {
    fn default() -> Self {
        Self {
            ch: ' ',
            style: CellStyle::default(),
        }
    }
}

impl TerminalCell {
    fn blank() -> Self {
        Self::default()
    }
}

// ─── TerminalGrid ─────────────────────────────────────────────────────────────

/// Terminal display grid với kích thước cố định `cols × rows`.
///
/// Cells được lưu row-major: `cells[row * cols + col]`.
/// Khi cursor xuống quá dòng cuối, grid **scroll up** (xóa dòng đầu, thêm dòng trống cuối).
pub struct TerminalGrid {
    pub cols: usize,
    pub rows: usize,
    cells: Vec<TerminalCell>,

    /// Vị trí cursor hiện tại (0-based).
    pub cursor_row: usize,
    pub cursor_col: usize,

    /// Style hiện tại sẽ áp cho ký tự tiếp theo được in.
    pub current_style: CellStyle,

    /// Parser ANSI nội bộ — giữ state giữa các chunk.
    parser: AnsiParser,
}

impl TerminalGrid {
    /// Tạo grid trống với kích thước `cols × rows`.
    pub fn new(cols: usize, rows: usize) -> Self {
        let cols = cols.max(1);
        let rows = rows.max(1);
        Self {
            cols,
            rows,
            cells: vec![TerminalCell::blank(); cols * rows],
            cursor_row: 0,
            cursor_col: 0,
            current_style: CellStyle::default(),
            parser: AnsiParser::new(),
        }
    }

    /// Nạp một string chunk (raw PTY output) vào grid.
    /// Parser giữ state nên có thể gọi nhiều lần liên tiếp.
    pub fn feed_chunk(&mut self, chunk: &str) {
        // Collect events trước để tránh borrow conflict với self.
        let mut events: Vec<AnsiEvent> = Vec::with_capacity(chunk.len());
        self.parser.feed_str(chunk, &mut |ev| events.push(ev));

        for event in events {
            self.apply_event(event);
        }
    }

    /// Áp một AnsiEvent lên grid state.
    fn apply_event(&mut self, event: AnsiEvent) {
        match event {
            AnsiEvent::PrintChar(ch) => {
                // TAB → advance đến cột chia hết 8 tiếp theo.
                if ch == '\t' {
                    let next_tab = ((self.cursor_col / 8) + 1) * 8;
                    let spaces = next_tab.min(self.cols).saturating_sub(self.cursor_col);
                    for _ in 0..spaces {
                        self.print_char_at_cursor(' ');
                    }
                    return;
                }

                // Các control char khác (non-printable) → bỏ qua.
                if ch.is_control() {
                    return;
                }

                self.print_char_at_cursor(ch);
            }

            AnsiEvent::Newline => {
                self.cursor_row += 1;
                if self.cursor_row >= self.rows {
                    self.scroll_up();
                    self.cursor_row = self.rows - 1;
                }
            }

            AnsiEvent::CarriageReturn => {
                self.cursor_col = 0;
            }

            AnsiEvent::Backspace => {
                if self.cursor_col > 0 {
                    self.cursor_col -= 1;
                }
            }

            AnsiEvent::ResetStyle => {
                self.current_style = CellStyle::default();
            }

            AnsiEvent::SetFg(color) => {
                self.current_style.fg = color;
            }

            AnsiEvent::SetBg(color) => {
                self.current_style.bg = color;
            }

            AnsiEvent::SetBold(bold) => {
                self.current_style.bold = bold;
            }

            AnsiEvent::CursorMove {
                row_delta,
                col_delta,
            } => {
                self.cursor_row = clamp_add(self.cursor_row, row_delta, 0, self.rows - 1);
                self.cursor_col = clamp_add(self.cursor_col, col_delta, 0, self.cols - 1);
            }

            AnsiEvent::CursorGoto { row, col } => {
                self.cursor_row = row.min(self.rows - 1);
                self.cursor_col = col.min(self.cols - 1);
            }

            AnsiEvent::EraseDisplay(mode) => {
                match mode {
                    // 0: from cursor to end of screen
                    0 => {
                        let start = self.cursor_row * self.cols + self.cursor_col;
                        for cell in &mut self.cells[start..] {
                            *cell = TerminalCell::blank();
                        }
                    }
                    // 1: from beginning to cursor
                    1 => {
                        let end = self.cursor_row * self.cols + self.cursor_col + 1;
                        for cell in &mut self.cells[..end] {
                            *cell = TerminalCell::blank();
                        }
                    }
                    // 2 / 3: clear entire screen
                    _ => {
                        for cell in &mut self.cells {
                            *cell = TerminalCell::blank();
                        }
                    }
                }
            }

            AnsiEvent::EraseLine(mode) => {
                let row_start = self.cursor_row * self.cols;
                match mode {
                    0 => {
                        // From cursor to end of line
                        let start = row_start + self.cursor_col;
                        let end = row_start + self.cols;
                        for cell in &mut self.cells[start..end] {
                            *cell = TerminalCell::blank();
                        }
                    }
                    1 => {
                        // From beginning to cursor
                        let end = row_start + self.cursor_col + 1;
                        for cell in &mut self.cells[row_start..end] {
                            *cell = TerminalCell::blank();
                        }
                    }
                    _ => {
                        // Entire line
                        for cell in &mut self.cells[row_start..row_start + self.cols] {
                            *cell = TerminalCell::blank();
                        }
                    }
                }
            }

            AnsiEvent::Unknown => {
                // Bỏ qua sequences không nhận ra.
            }
        }
    }

    /// In một ký tự tại vị trí cursor, advance cursor.
    fn print_char_at_cursor(&mut self, ch: char) {
        if self.cursor_col >= self.cols {
            // Wrap: xuống dòng mới.
            self.cursor_col = 0;
            self.cursor_row += 1;
            if self.cursor_row >= self.rows {
                self.scroll_up();
                self.cursor_row = self.rows - 1;
            }
        }

        let idx = self.cursor_row * self.cols + self.cursor_col;
        self.cells[idx] = TerminalCell {
            ch,
            style: self.current_style,
        };
        self.cursor_col += 1;
    }

    /// Scroll lên 1 dòng: xóa dòng đầu, thêm dòng trống dưới cùng.
    fn scroll_up(&mut self) {
        self.cells.drain(0..self.cols);
        self.cells
            .extend(std::iter::repeat_n(TerminalCell::blank(), self.cols));
    }

    // ─── Query API ────────────────────────────────────────────────────────────

    /// Lấy cell tại vị trí `(row, col)`. Trả cell blank nếu out-of-bounds.
    pub fn cell_at(&self, row: usize, col: usize) -> &TerminalCell {
        if row >= self.rows || col >= self.cols {
            return &BLANK_CELL;
        }
        &self.cells[row * self.cols + col]
    }

    /// Iterate qua tất cả visible cells: `(row, col, &TerminalCell)`.
    pub fn iter_visible_cells(&self) -> impl Iterator<Item = (usize, usize, &TerminalCell)> {
        self.cells
            .iter()
            .enumerate()
            .map(|(idx, cell)| (idx / self.cols, idx % self.cols, cell))
    }

    /// Số dòng thực sự có nội dung (không tính dòng trống trailing).
    pub fn used_rows(&self) -> usize {
        for row in (0..self.rows).rev() {
            let row_start = row * self.cols;
            let row_cells = &self.cells[row_start..row_start + self.cols];
            if row_cells.iter().any(|c| c.ch != ' ') {
                return row + 1;
            }
        }
        0
    }

    /// Debug: dump grid thành string nhiều dòng (only printable chars).
    pub fn debug_dump(&self) -> String {
        let used = self.used_rows().max(self.cursor_row + 1);
        let mut out = String::new();
        for row in 0..used {
            let row_start = row * self.cols;
            let row_end = row_start + self.cols;
            let row_str: String = self.cells[row_start..row_end]
                .iter()
                .map(|c| c.ch)
                .collect::<String>()
                .trim_end()
                .to_string();
            out.push_str(&row_str);
            out.push('\n');
        }
        out
    }

    /// Debug: dump cell styles của một dòng cụ thể.
    pub fn debug_row_styles(&self, row: usize) -> Vec<(char, CellStyle)> {
        if row >= self.rows {
            return Vec::new();
        }
        let row_start = row * self.cols;
        self.cells[row_start..row_start + self.cols]
            .iter()
            .map(|c| (c.ch, c.style))
            .collect()
    }

    /// Reset toàn bộ grid về trạng thái ban đầu.
    pub fn clear(&mut self) {
        for cell in &mut self.cells {
            *cell = TerminalCell::blank();
        }
        self.cursor_row = 0;
        self.cursor_col = 0;
        self.current_style = CellStyle::default();
    }
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

static BLANK_CELL: TerminalCell = TerminalCell {
    ch: ' ',
    style: CellStyle {
        fg: AnsiColor::Default,
        bg: AnsiColor::Default,
        bold: false,
    },
};

/// i32 addition với clamp về [min, max].
fn clamp_add(base: usize, delta: i32, min: usize, max: usize) -> usize {
    let result = base as i32 + delta;
    result.max(min as i32).min(max as i32) as usize
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::terminal::ansi_parser::AnsiColor;

    #[test]
    fn plain_text_fills_grid() {
        let mut grid = TerminalGrid::new(10, 5);
        grid.feed_chunk("hello");
        assert_eq!(grid.cell_at(0, 0).ch, 'h');
        assert_eq!(grid.cell_at(0, 4).ch, 'o');
        assert_eq!(grid.cursor_col, 5);
        assert_eq!(grid.cursor_row, 0);
    }

    #[test]
    fn newline_advances_row() {
        let mut grid = TerminalGrid::new(10, 5);
        grid.feed_chunk("abc\n");
        assert_eq!(grid.cursor_row, 1);
    }

    #[test]
    fn cr_resets_col() {
        let mut grid = TerminalGrid::new(10, 5);
        grid.feed_chunk("abc\rXY");
        // \r → col 0, in "XY" → overwrite "ab"
        assert_eq!(grid.cell_at(0, 0).ch, 'X');
        assert_eq!(grid.cell_at(0, 1).ch, 'Y');
        assert_eq!(grid.cell_at(0, 2).ch, 'c'); // 'c' không bị ghi đè
    }

    #[test]
    fn ansi_color_applied_to_cells() {
        let mut grid = TerminalGrid::new(20, 5);
        // ESC[32m = green fg, ESC[0m = reset
        grid.feed_chunk("\x1b[32mGREEN\x1b[0mNORMAL");
        assert_eq!(grid.cell_at(0, 0).style.fg, AnsiColor::Index(2)); // green
        assert_eq!(grid.cell_at(0, 0).ch, 'G');
        assert_eq!(grid.cell_at(0, 4).ch, 'N');
        // Sau reset, style về default
        assert_eq!(grid.cell_at(0, 5).style.fg, AnsiColor::Default);
        assert_eq!(grid.cell_at(0, 5).ch, 'N');
    }

    #[test]
    fn line_wrap_at_col_boundary() {
        let mut grid = TerminalGrid::new(5, 4);
        grid.feed_chunk("ABCDEFGH"); // 8 chars, cols=5 → wrap ở col 5
        assert_eq!(grid.cell_at(0, 4).ch, 'E');
        assert_eq!(grid.cell_at(1, 0).ch, 'F');
        assert_eq!(grid.cell_at(1, 2).ch, 'H');
    }

    #[test]
    fn scroll_when_cursor_below_rows() {
        let mut grid = TerminalGrid::new(5, 3);
        grid.feed_chunk("LINE1\nLINE2\nLINE3\nLINE4");
        // Sau 3 newlines (4 dòng với 3 rows) → phải scroll 1 lần.
        // Dòng 0 của grid bây giờ là LINE2.
        assert_eq!(grid.cell_at(0, 0).ch, 'L'); // LINE2
        assert_eq!(grid.cell_at(2, 0).ch, 'L'); // LINE4
        assert_eq!(grid.cursor_row, 2);
    }

    #[test]
    fn erase_line_clears_to_end() {
        let mut grid = TerminalGrid::new(10, 3);
        grid.feed_chunk("HELLO");
        // Di chuyển cursor về col 2, rồi erase to end.
        grid.feed_chunk("\x1b[1;3H\x1b[K"); // row=0,col=2, erase to end of line
        assert_eq!(grid.cell_at(0, 0).ch, 'H');
        assert_eq!(grid.cell_at(0, 1).ch, 'E');
        // Từ col 2 trở đi phải là blank.
        assert_eq!(grid.cell_at(0, 2).ch, ' ');
        assert_eq!(grid.cell_at(0, 4).ch, ' ');
    }

    #[test]
    fn debug_dump_shows_content() {
        let mut grid = TerminalGrid::new(10, 3);
        grid.feed_chunk("hello\nworld");
        let dump = grid.debug_dump();
        assert!(dump.contains("hello"));
        assert!(dump.contains("world"));
    }

    #[test]
    fn clear_resets_all_state() {
        let mut grid = TerminalGrid::new(10, 3);
        grid.feed_chunk("\x1b[32mhello");
        grid.clear();
        assert_eq!(grid.cursor_row, 0);
        assert_eq!(grid.cursor_col, 0);
        assert_eq!(grid.current_style, CellStyle::default());
        assert_eq!(grid.cell_at(0, 0).ch, ' ');
    }

    #[test]
    fn multi_chunk_feed_maintains_state() {
        let mut grid = TerminalGrid::new(20, 5);
        // ESC sequence split across two chunks
        grid.feed_chunk("\x1b[3");
        grid.feed_chunk("2mHI");
        // Parser phải nhớ state của ESC[3 từ chunk trước.
        assert_eq!(grid.cell_at(0, 0).style.fg, AnsiColor::Index(2));
        assert_eq!(grid.cell_at(0, 0).ch, 'H');
    }
}
