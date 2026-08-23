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

use std::collections::VecDeque;

use unicode_normalization::UnicodeNormalization;

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
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TerminalCell {
    /// Ký tự hiển thị. `' '` = cell trống.
    pub ch: char,
    pub style: CellStyle,
    /// Per-cell foreground color override (linear RGBA).
    /// When `Some`, takes precedence over the ANSI-based `style.fg`.
    pub style_fg: Option<[f32; 4]>,
    /// Set on the first cell (col 0) of a row that is a soft-wrap continuation
    /// of the previous physical row (the line filled to the edge with no
    /// newline). Lets highlighting treat wrapped rows as one logical line.
    pub wrap_continued: bool,
}

impl Default for TerminalCell {
    fn default() -> Self {
        Self {
            ch: ' ',
            style: CellStyle::default(),
            style_fg: None,
            wrap_continued: false,
        }
    }
}

impl TerminalCell {
    fn blank() -> Self {
        Self::default()
    }

    fn is_visually_empty(&self) -> bool {
        self.ch == ' '
            && self.style.fg == AnsiColor::Default
            && self.style.bg == AnsiColor::Default
            && !self.style.bold
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TerminalPoint {
    pub row: usize,
    pub col: usize,
}

/// A single search match within the terminal grid.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TerminalSearchMatch {
    /// Absolute row index (0 = top of scrollback).
    pub row: usize,
    /// Starting column of the match.
    pub col: usize,
    /// Character length of the match.
    pub len: usize,
}

/// Colors used for regex-based terminal output highlighting.
///
/// Each field is an RGBA color in sRGB space (matching `AnsiColor::to_rgba_f32` output).
/// The default values are derived from the built-in dark theme.
#[derive(Debug, Clone, Copy)]
pub struct HighlightColors {
    /// diagnostic.warn — yellow/orange tint for `W/` log lines.
    pub warn: [f32; 4],
    /// diagnostic.error — red tint for `E/` log lines.
    pub error: [f32; 4],
    /// Dimmed foreground for `D/` (debug) log lines.
    pub fg_dim: [f32; 4],
    /// syntax.string — green for quoted string literals.
    pub syntax_string: [f32; 4],
    /// syntax.number — red/orange for numeric literals and time formats.
    pub syntax_number: [f32; 4],
    /// syntax.keyword — yellow for boolean and null literals.
    pub syntax_keyword: [f32; 4],
}

impl Default for HighlightColors {
    fn default() -> Self {
        Self {
            warn: [0.949, 0.722, 0.294, 1.0],
            error: [1.0, 0.482, 0.447, 1.0],
            fg_dim: [0.427, 0.455, 0.514, 1.0],
            syntax_string: [0.404, 0.839, 0.486, 1.0],
            syntax_number: [1.0, 0.482, 0.447, 1.0],
            syntax_keyword: [0.918, 0.804, 0.380, 1.0],
        }
    }
}

impl HighlightColors {
    /// Build highlight colors from the active theme config.
    ///
    /// Maps each semantic token to the corresponding theme field so that
    /// regex-based terminal highlighting stays consistent with the user's
    /// chosen color scheme.
    pub fn from_theme(theme: &crate::config::theme_config::ThemeConfig) -> Self {
        Self {
            warn: theme.ui.warning.as_f32(),
            error: theme.ui.error.as_f32(),
            fg_dim: theme.ui.fg_dim.as_f32(),
            syntax_string: theme.syntax.string.as_f32(),
            syntax_number: theme.syntax.number.as_f32(),
            syntax_keyword: theme.syntax.keyword.as_f32(),
        }
    }
}

// ─── TerminalGrid ─────────────────────────────────────────────────────────────

/// Terminal display grid với kích thước cố định `cols × rows`.
///
/// Cells được lưu row-major: `cells[row * cols + col]`.
/// Khi cursor xuống quá dòng cuối, grid **scroll up** (xóa dòng đầu, thêm dòng trống cuối).
const SCROLLBACK_LIMIT: usize = 10_000;

#[derive(Clone)]
pub struct TerminalGrid {
    pub cols: usize,
    pub rows: usize,
    cells: Vec<TerminalCell>,

    /// Scrollback buffer — rows pushed off the top of the live grid.
    scrollback: VecDeque<Vec<TerminalCell>>,

    /// How many rows above the live grid bottom are shown (0 = live view).
    pub scroll_offset: usize,

    /// Vị trí cursor hiện tại (0-based).
    pub cursor_row: usize,
    pub cursor_col: usize,

    /// Virtual cursor dùng cho Terminal Normal Mode / copy mode.
    pub virtual_cursor: TerminalPoint,

    /// Điểm bắt đầu selection khi user nhấn `v` trong Terminal Normal Mode.
    pub selection_anchor: Option<TerminalPoint>,

    /// Style hiện tại sẽ áp cho ký tự tiếp theo được in.
    pub current_style: CellStyle,

    /// Parser ANSI nội bộ — giữ state giữa các chunk.
    parser: AnsiParser,

    /// Colors used by `apply_regex_highlights`.
    pub highlight_colors: HighlightColors,

    /// All current search matches (absolute row coordinates).
    pub search_matches: Vec<TerminalSearchMatch>,
    /// Index into `search_matches` for the currently active match.
    pub search_cursor: usize,
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
            scrollback: VecDeque::new(),
            scroll_offset: 0,
            cursor_row: 0,
            cursor_col: 0,
            virtual_cursor: TerminalPoint { row: 0, col: 0 },
            selection_anchor: None,
            current_style: CellStyle::default(),
            parser: AnsiParser::new(),
            highlight_colors: HighlightColors::default(),
            search_matches: Vec::new(),
            search_cursor: 0,
        }
    }

    /// Nạp một string chunk (raw PTY output) vào grid.
    /// Parser giữ state nên có thể gọi nhiều lần liên tiếp.
    pub fn feed_chunk(&mut self, chunk: &str) -> usize {
        self.feed_bytes(chunk.as_bytes())
    }

    /// Nạp một byte chunk trực tiếp từ PTY vào grid.
    /// Toàn bộ chunk được parse trước, rồi apply state trong một batch.
    pub fn feed_bytes(&mut self, chunk: &[u8]) -> usize {
        // Collect events trước để tránh borrow conflict với self.
        let mut events: Vec<AnsiEvent> = Vec::with_capacity(chunk.len());
        self.parser.feed_bytes(chunk, &mut |ev| events.push(ev));

        let mut scrolled_rows = 0usize;
        for event in events {
            scrolled_rows += self.apply_event(event);
        }
        self.clamp_virtual_points();
        scrolled_rows
    }

    /// Áp một AnsiEvent lên grid state.
    fn apply_event(&mut self, event: AnsiEvent) -> usize {
        match event {
            AnsiEvent::PrintChar(ch) => {
                // TAB → advance đến cột chia hết 8 tiếp theo.
                if ch == '\t' {
                    let next_tab = ((self.cursor_col / 8) + 1) * 8;
                    let spaces = next_tab.min(self.cols).saturating_sub(self.cursor_col);
                    let mut scrolled = 0usize;
                    for _ in 0..spaces {
                        scrolled += self.print_char_at_cursor(' ');
                    }
                    return scrolled;
                }

                // Các control char khác (non-printable) → bỏ qua.
                if ch.is_control() {
                    return 0;
                }

                self.print_char_at_cursor(ch)
            }

            AnsiEvent::Newline => {
                self.cursor_row += 1;
                if self.cursor_row >= self.rows {
                    self.scroll_up();
                    self.cursor_row = self.rows - 1;
                    return 1;
                }
                0
            }

            AnsiEvent::CarriageReturn => {
                self.cursor_col = 0;
                0
            }

            AnsiEvent::Backspace => {
                if self.cursor_col > 0 {
                    self.cursor_col -= 1;
                }
                0
            }

            AnsiEvent::ResetStyle => {
                self.current_style = CellStyle::default();
                0
            }

            AnsiEvent::SetFg(color) => {
                self.current_style.fg = color;
                0
            }

            AnsiEvent::SetBg(color) => {
                self.current_style.bg = color;
                0
            }

            AnsiEvent::SetBold(bold) => {
                self.current_style.bold = bold;
                0
            }

            AnsiEvent::CursorMove {
                row_delta,
                col_delta,
            } => {
                self.cursor_row = clamp_add(self.cursor_row, row_delta, 0, self.rows - 1);
                self.cursor_col = clamp_add(self.cursor_col, col_delta, 0, self.cols - 1);
                0
            }

            AnsiEvent::CursorGoto { row, col } => {
                self.cursor_row = row.min(self.rows - 1);
                self.cursor_col = col.min(self.cols - 1);
                0
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
                        // Clamp to cols-1: cursor_col can equal cols in pending-wrap state
                        // (after printing into the last column before the next char wraps).
                        let safe_col = self.cursor_col.min(self.cols.saturating_sub(1));
                        let end = self.cursor_row * self.cols + safe_col + 1;
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
                0
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
                        // From beginning to cursor — clamp for pending-wrap state.
                        let safe_col = self.cursor_col.min(self.cols.saturating_sub(1));
                        let end = row_start + safe_col + 1;
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
                0
            }

            AnsiEvent::DeleteChars(n) => {
                // DCH: xoá n cell tại cursor, dồn phần còn lại của dòng sang trái,
                // đệm blank vào cuối dòng. Clamp cho pending-wrap (cursor_col == cols).
                let row_start = self.cursor_row * self.cols;
                let col = self.cursor_col.min(self.cols.saturating_sub(1));
                let n = n.min(self.cols - col);
                if n > 0 {
                    let start = row_start + col;
                    let end = row_start + self.cols;
                    self.cells.copy_within(start + n..end, start);
                    for cell in &mut self.cells[end - n..end] {
                        *cell = TerminalCell::blank();
                    }
                }
                0
            }

            AnsiEvent::InsertChars(n) => {
                // ICH: chèn n ô trống tại cursor, dồn phần còn lại sang phải
                // (cell bị đẩy quá cuối dòng bị drop — đúng semantics xterm).
                let row_start = self.cursor_row * self.cols;
                let col = self.cursor_col.min(self.cols.saturating_sub(1));
                let n = n.min(self.cols - col);
                if n > 0 {
                    let start = row_start + col;
                    let end = row_start + self.cols;
                    self.cells.copy_within(start..end - n, start + n);
                    for cell in &mut self.cells[start..start + n] {
                        *cell = TerminalCell::blank();
                    }
                }
                0
            }

            AnsiEvent::EraseChars(n) => {
                // ECH: blank n ô tại cursor, KHÔNG dồn dòng.
                let row_start = self.cursor_row * self.cols;
                let col = self.cursor_col.min(self.cols.saturating_sub(1));
                let n = n.min(self.cols - col);
                let start = row_start + col;
                for cell in &mut self.cells[start..start + n] {
                    *cell = TerminalCell::blank();
                }
                0
            }

            AnsiEvent::Unknown => {
                // Bỏ qua sequences không nhận ra.
                0
            }
        }
    }

    /// In một ký tự tại vị trí cursor, advance cursor.
    fn print_char_at_cursor(&mut self, ch: char) -> usize {
        // Combining marks (e.g. Vietnamese tone marks U+0300–U+036F) are zero-width:
        // they must attach to the preceding cell's base character instead of consuming
        // a new cell, otherwise the base letter renders without its mark ("cụ" → "cu").
        if unicode_normalization::char::is_combining_mark(ch) {
            self.compose_combining_into_prev(ch);
            return 0;
        }

        let mut scrolled = 0usize;
        // A soft wrap happens when the cursor has run past the last column with
        // no intervening newline. The first cell of the new row records this so
        // highlighting can stitch wrapped rows back into one logical line.
        let wrapped = self.cursor_col >= self.cols;
        if wrapped {
            // Wrap: xuống dòng mới.
            self.cursor_col = 0;
            self.cursor_row += 1;
            if self.cursor_row >= self.rows {
                self.scroll_up();
                self.cursor_row = self.rows - 1;
                scrolled = 1;
            }
        }

        let idx = self.cursor_row * self.cols + self.cursor_col;
        self.cells[idx] = TerminalCell {
            ch,
            style: self.current_style,
            style_fg: None,
            wrap_continued: wrapped,
        };
        self.cursor_col += 1;
        scrolled
    }

    /// Fold a combining mark into the base character of the previous cell using
    /// NFC composition (e.g. 'ô' + U+0323 → 'ộ'). If no precomposed form exists,
    /// or there is no base cell, the mark is dropped rather than misaligning the
    /// grid. The cursor is not advanced — combining marks are zero-width.
    fn compose_combining_into_prev(&mut self, mark: char) {
        let (row, col) = if self.cursor_col > 0 {
            (self.cursor_row, self.cursor_col - 1)
        } else if self.cursor_row > 0 {
            (self.cursor_row - 1, self.cols.saturating_sub(1))
        } else {
            return;
        };
        let idx = row * self.cols + col;
        if let Some(cell) = self.cells.get_mut(idx) {
            // NFC-normalize the base + mark together. This reorders the marks
            // canonically first, so it composes even when the stream sends the
            // marks out of canonical order (e.g. precomposed 'ô' followed by a
            // combining dot-below — pairwise composition alone would miss this).
            let mut grapheme = String::with_capacity(8);
            grapheme.push(cell.ch);
            grapheme.push(mark);
            let mut nfc = grapheme.nfc();
            if let Some(first) = nfc.next()
                && nfc.next().is_none()
            {
                // Only adopt the result when it collapses to a single precomposed
                // scalar; otherwise keep the base and drop the mark rather than
                // misaligning the fixed-width grid.
                cell.ch = first;
            }
        }
    }

    /// Scroll lên 1 dòng: push dòng đầu vào scrollback, thêm dòng trống dưới.
    fn scroll_up(&mut self) {
        let pushed_row: Vec<TerminalCell> = self.cells[0..self.cols].to_vec();
        self.scrollback.push_back(pushed_row);
        let overflowed = if self.scrollback.len() > SCROLLBACK_LIMIT {
            self.scrollback.pop_front();
            true
        } else {
            false
        };
        if overflowed {
            self.shift_points_up_after_front_trim(1);
        }
        self.cells.drain(0..self.cols);
        self.cells
            .extend(std::iter::repeat_n(TerminalCell::blank(), self.cols));
    }

    /// Scroll view lên N dòng (không thay đổi live grid, chỉ thay offset).
    pub fn view_scroll_up(&mut self, lines: usize) {
        let max = self.scrollback.len();
        self.scroll_offset = (self.scroll_offset + lines).min(max);
    }

    /// Scroll view xuống N dòng (về phía live content).
    pub fn view_scroll_down(&mut self, lines: usize) {
        self.scroll_offset = self.scroll_offset.saturating_sub(lines);
    }

    /// Reset về live view (offset = 0).
    pub fn view_scroll_to_bottom(&mut self) {
        self.scroll_offset = 0;
    }

    /// Resize grid while preserving recent terminal output as much as possible.
    ///
    /// - Growing keeps the existing live rows pinned to the top.
    /// - Shrinking keeps the newest visible rows and pushes trimmed rows into
    ///   scrollback so recent output stays on screen.
    pub fn resize(&mut self, new_cols: usize, new_rows: usize) -> bool {
        let new_cols = new_cols.max(1);
        let new_rows = new_rows.max(1);
        if self.cols == new_cols && self.rows == new_rows {
            return false;
        }

        let old_cols = self.cols;
        let old_rows = self.rows;
        let old_cells = std::mem::replace(
            &mut self.cells,
            vec![TerminalCell::blank(); new_cols * new_rows],
        );

        for row in &mut self.scrollback {
            resize_terminal_row(row, new_cols);
        }

        let copy_cols = old_cols.min(new_cols);
        let copy_rows = old_rows.min(new_rows);
        let trimmed_top_rows = old_rows.saturating_sub(new_rows);

        if trimmed_top_rows > 0 {
            for trimmed_row in 0..trimmed_top_rows {
                let start = trimmed_row * old_cols;
                let end = start + old_cols;
                let mut row = old_cells[start..end].to_vec();
                resize_terminal_row(&mut row, new_cols);
                self.scrollback.push_back(row);
            }
            if self.scrollback.len() > SCROLLBACK_LIMIT {
                let overflow = self.scrollback.len() - SCROLLBACK_LIMIT;
                self.scrollback.drain(0..overflow);
                self.shift_points_up_after_front_trim(overflow);
            }
        }

        let src_row_offset = trimmed_top_rows;
        for row in 0..copy_rows {
            let src_row = row + src_row_offset;
            let src_start = src_row * old_cols;
            let dst_start = row * new_cols;
            self.cells[dst_start..dst_start + copy_cols]
                .copy_from_slice(&old_cells[src_start..src_start + copy_cols]);
        }

        self.cols = new_cols;
        self.rows = new_rows;
        self.cursor_col = self.cursor_col.min(new_cols.saturating_sub(1));
        self.cursor_row = if trimmed_top_rows == 0 {
            self.cursor_row.min(new_rows.saturating_sub(1))
        } else {
            self.cursor_row
                .saturating_sub(trimmed_top_rows)
                .min(new_rows.saturating_sub(1))
        };
        self.scroll_offset = self.scroll_offset.min(self.scrollback.len());
        self.virtual_cursor.col = self.virtual_cursor.col.min(new_cols.saturating_sub(1));
        if let Some(anchor) = self.selection_anchor.as_mut() {
            anchor.col = anchor.col.min(new_cols.saturating_sub(1));
        }
        self.clamp_virtual_points();
        true
    }

    pub fn total_rows(&self) -> usize {
        self.scrollback.len() + self.rows
    }

    pub fn live_cursor_absolute_position(&self) -> TerminalPoint {
        TerminalPoint {
            row: self.scrollback.len() + self.cursor_row,
            col: self.cursor_col.min(self.cols.saturating_sub(1)),
        }
    }

    pub fn enter_normal_mode(&mut self) -> bool {
        let next = self.live_cursor_absolute_position();
        let changed = self.virtual_cursor != next || self.selection_anchor.is_some();
        self.virtual_cursor = next;
        self.selection_anchor = None;
        self.ensure_virtual_cursor_visible();
        changed
    }

    /// "Shell-line territory": viewport ở live prompt, không có visual
    /// selection, và virtual cursor tại-hoặc-dưới dòng shell cursor (mọi row
    /// dưới prompt đều trống nên tính là "ở prompt" — nhờ đó G/ctrl-q luôn rơi
    /// vào territory này). Chỉ khi đó motion/edit mới điều khiển shell qua
    /// readline; phía trên prompt là copy-mode territory với virtual cursor.
    pub fn shell_line_editing_active(&self) -> bool {
        self.scroll_offset == 0
            && self.selection_anchor.is_none()
            && self.virtual_cursor.row >= self.live_cursor_absolute_position().row
    }

    /// Đồng bộ virtual cursor (T-COPY) theo shell cursor thật sau khi PTY echo
    /// làm cursor di chuyển — cần cho vim-style line editing qua readline,
    /// nơi mỗi motion/edit được shell thực hiện rồi echo lại. Chỉ snap khi đang
    /// ở shell-line territory: user đã đưa cursor lên scrollback/output thì
    /// output mới đến không được kéo cursor về prompt.
    pub fn sync_virtual_cursor_to_shell(&mut self) {
        if self.shell_line_editing_active() {
            self.virtual_cursor = self.live_cursor_absolute_position();
        }
    }

    pub fn exit_normal_mode(&mut self) -> bool {
        let changed = self.selection_anchor.take().is_some();
        self.ensure_virtual_cursor_visible();
        changed
    }

    pub fn begin_selection(&mut self) -> bool {
        let anchor = self.virtual_cursor;
        if self.selection_anchor == Some(anchor) {
            return false;
        }
        self.selection_anchor = Some(anchor);
        true
    }

    pub fn begin_line_selection(&mut self) -> bool {
        let line_start = TerminalPoint {
            row: self.virtual_cursor.row,
            col: 0,
        };
        if self.selection_anchor == Some(line_start) {
            return false;
        }
        self.selection_anchor = Some(line_start);
        // Move cursor to end of line to select the entire line
        let line_end_col = self.line_end_col_for_absolute_row(self.virtual_cursor.row);
        self.set_virtual_cursor(TerminalPoint {
            row: self.virtual_cursor.row,
            col: line_end_col,
        });
        true
    }

    pub fn clear_selection(&mut self) -> bool {
        self.selection_anchor.take().is_some()
    }

    pub fn move_virtual_left(&mut self) -> bool {
        if self.virtual_cursor.col == 0 {
            return false;
        }
        self.set_virtual_cursor(TerminalPoint {
            row: self.virtual_cursor.row,
            col: self.virtual_cursor.col - 1,
        })
    }

    pub fn move_virtual_right(&mut self) -> bool {
        if self.virtual_cursor.col + 1 >= self.cols {
            return false;
        }
        self.set_virtual_cursor(TerminalPoint {
            row: self.virtual_cursor.row,
            col: self.virtual_cursor.col + 1,
        })
    }

    pub fn move_virtual_up(&mut self) -> bool {
        if self.virtual_cursor.row == 0 {
            return false;
        }
        self.set_virtual_cursor(TerminalPoint {
            row: self.virtual_cursor.row - 1,
            col: self.virtual_cursor.col,
        })
    }

    pub fn move_virtual_down(&mut self) -> bool {
        let last_row = self.total_rows().saturating_sub(1);
        if self.virtual_cursor.row >= last_row {
            return false;
        }
        self.set_virtual_cursor(TerminalPoint {
            row: self.virtual_cursor.row + 1,
            col: self.virtual_cursor.col,
        })
    }

    pub fn move_virtual_word_forward(&mut self) -> bool {
        let Some(next) = self.word_motion_target(WordMotion::Forward) else {
            return false;
        };
        self.set_virtual_cursor(next)
    }

    pub fn move_virtual_word_backward(&mut self) -> bool {
        let Some(next) = self.word_motion_target(WordMotion::Backward) else {
            return false;
        };
        self.set_virtual_cursor(next)
    }

    pub fn move_virtual_word_end(&mut self) -> bool {
        let Some(next) = self.word_motion_target(WordMotion::End) else {
            return false;
        };
        self.set_virtual_cursor(next)
    }

    pub fn move_virtual_to_line_start(&mut self) -> bool {
        self.set_virtual_cursor(TerminalPoint {
            row: self.virtual_cursor.row,
            col: 0,
        })
    }

    pub fn move_virtual_to_line_end(&mut self) -> bool {
        self.set_virtual_cursor(TerminalPoint {
            row: self.virtual_cursor.row,
            col: self.line_end_col_for_absolute_row(self.virtual_cursor.row),
        })
    }

    pub fn move_virtual_to_first_non_whitespace(&mut self) -> bool {
        self.set_virtual_cursor(TerminalPoint {
            row: self.virtual_cursor.row,
            col: self.first_non_whitespace_col_for_absolute_row(self.virtual_cursor.row),
        })
    }

    pub fn move_virtual_to_first_line(&mut self) -> bool {
        self.set_virtual_cursor(TerminalPoint {
            row: 0,
            col: self.virtual_cursor.col.min(self.cols.saturating_sub(1)),
        })
    }

    pub fn move_virtual_to_last_line(&mut self) -> bool {
        self.set_virtual_cursor(TerminalPoint {
            row: self.total_rows().saturating_sub(1),
            col: self.virtual_cursor.col.min(self.cols.saturating_sub(1)),
        })
    }

    pub fn move_virtual_half_page_up(&mut self, lines: usize) -> bool {
        let lines = lines.max(1);
        let target_row = self.virtual_cursor.row.saturating_sub(lines);
        self.set_virtual_cursor(TerminalPoint {
            row: target_row,
            col: self.virtual_cursor.col,
        })
    }

    pub fn move_virtual_half_page_down(&mut self, lines: usize) -> bool {
        let lines = lines.max(1);
        let last_row = self.total_rows().saturating_sub(1);
        let target_row = (self.virtual_cursor.row + lines).min(last_row);
        self.set_virtual_cursor(TerminalPoint {
            row: target_row,
            col: self.virtual_cursor.col,
        })
    }

    pub fn center_virtual_cursor_line(&mut self) -> bool {
        let current_top = self.viewport_start_absolute_row();
        let centered_top = self
            .virtual_cursor
            .row
            .saturating_sub(self.rows.saturating_sub(1) / 2);
        self.set_viewport_top_absolute_row(centered_top);
        current_top != self.viewport_start_absolute_row()
    }

    pub fn virtual_cursor_display_position(&self) -> Option<(usize, usize)> {
        Some((
            self.absolute_row_to_display_row(self.virtual_cursor.row)?,
            self.virtual_cursor.col.min(self.cols.saturating_sub(1)),
        ))
    }

    pub fn visible_selection_spans(&self) -> Vec<(usize, usize, usize)> {
        let Some((start, end)) = self.normalized_selection_bounds() else {
            return Vec::new();
        };

        let viewport_top = self.viewport_start_absolute_row();
        let viewport_bottom = viewport_top + self.rows.saturating_sub(1);
        if end.row < viewport_top || start.row > viewport_bottom {
            return Vec::new();
        }

        let mut spans = Vec::new();
        for row in start.row.max(viewport_top)..=end.row.min(viewport_bottom) {
            let Some(display_row) = self.absolute_row_to_display_row(row) else {
                continue;
            };
            let start_col = if row == start.row { start.col } else { 0 };
            let end_col = if row == end.row {
                end.col
            } else {
                self.cols.saturating_sub(1)
            };
            spans.push((display_row, start_col, end_col.saturating_add(1)));
        }
        spans
    }

    /// Returns visible search match spans as `(display_row, start_col, end_col_exclusive)`.
    ///
    /// Only includes matches that are currently in the viewport, converting
    /// absolute row coordinates to display row coordinates.
    pub fn visible_search_match_spans(&self) -> Vec<(usize, usize, usize)> {
        if self.search_matches.is_empty() {
            return Vec::new();
        }

        let viewport_top = self.viewport_start_absolute_row();
        let viewport_bottom = viewport_top + self.rows.saturating_sub(1);

        let mut spans = Vec::new();
        for m in &self.search_matches {
            if m.row < viewport_top || m.row > viewport_bottom {
                continue;
            }
            let Some(display_row) = self.absolute_row_to_display_row(m.row) else {
                continue;
            };
            let start_col = m.col;
            let end_col = m.col + m.len;
            if start_col >= self.cols {
                continue;
            }
            let end_col = end_col.min(self.cols);
            spans.push((display_row, start_col, end_col));
        }
        spans
    }

    pub fn yank_selection_text(&self) -> Option<String> {
        let (start, end) = self.normalized_selection_bounds()?;
        let mut lines = Vec::new();

        for row in start.row..=end.row {
            let row_cells = self.row_cells_absolute(row)?;
            let start_col = if row == start.row { start.col } else { 0 };
            let end_col = if row == end.row {
                end.col
            } else {
                self.cols.saturating_sub(1)
            };
            let segment: String = row_cells[start_col..=end_col]
                .iter()
                .map(|cell| cell.ch)
                .collect();
            lines.push(segment.trim_end_matches(' ').to_string());
        }

        Some(lines.join("\n"))
    }

    // ─── Query API ────────────────────────────────────────────────────────────

    /// Lấy cell tại vị trí `(row, col)`. Trả cell blank nếu out-of-bounds.
    pub fn cell_at(&self, row: usize, col: usize) -> &TerminalCell {
        if row >= self.rows || col >= self.cols {
            return &BLANK_CELL;
        }
        &self.cells[row * self.cols + col]
    }

    /// Iterate qua visible cells theo scroll_offset hiện tại: `(row, col, &TerminalCell)`.
    ///
    /// Khi `scroll_offset > 0`, rows được lấy từ scrollback + live grid.
    /// Row 0 trong iterator luôn là dòng trên cùng của viewport.
    pub fn iter_visible_cells(&self) -> impl Iterator<Item = (usize, usize, TerminalCell)> + '_ {
        let rows = self.rows;
        let cols = self.cols;
        let offset = self.scroll_offset;
        let sb_len = self.scrollback.len();

        (0..rows).flat_map(move |display_row| {
            // display_row 0 = top of viewport.
            // When offset > 0, top of viewport is in scrollback.
            let source_row_from_bottom = (rows - 1 - display_row) + offset;
            let cells: Vec<TerminalCell> = if source_row_from_bottom < rows {
                // Row is in the live grid.
                let live_row = rows - 1 - source_row_from_bottom;
                let start = live_row * cols;
                self.cells[start..start + cols].to_vec()
            } else {
                // Row is in the scrollback buffer.
                let sb_idx = sb_len.saturating_sub(source_row_from_bottom - rows + 1);
                if sb_idx < sb_len {
                    self.scrollback[sb_idx].clone()
                } else {
                    vec![TerminalCell::blank(); cols]
                }
            };
            cells
                .into_iter()
                .enumerate()
                .map(move |(col, cell)| (display_row, col, cell))
        })
    }

    /// Số dòng thực sự có nội dung (không tính dòng trống trailing).
    pub fn used_rows(&self) -> usize {
        for row in (0..self.rows).rev() {
            let row_start = row * self.cols;
            let row_cells = &self.cells[row_start..row_start + self.cols];
            if row_cells.iter().any(|c| !c.is_visually_empty()) {
                return row + 1;
            }
        }
        0
    }

    /// Kiểm tra grid có bất kỳ nội dung nào không (bao gồm cả scrollback).
    ///
    /// Khác với `used_rows()` chỉ kiểm tra live grid, method này cũng kiểm tra
    /// scrollback buffer để tránh hiển thị EMPTY_TERMINAL_HINT khi nội dung
    /// đã bị đẩy vào scrollback sau resize.
    pub fn is_empty(&self) -> bool {
        if self.used_rows() > 0 {
            return false;
        }
        for row in &self.scrollback {
            if row.iter().any(|c| !c.is_visually_empty()) {
                return false;
            }
        }
        true
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
        self.scrollback.clear();
        self.scroll_offset = 0;
        self.virtual_cursor = TerminalPoint { row: 0, col: 0 };
        self.selection_anchor = None;
        self.current_style = CellStyle::default();
        self.parser = AnsiParser::new();
    }

    /// Apply regex-based syntax highlighting to visible rows.
    ///
    /// **Step 1 — Base line color:** checks if a row matches a log-level prefix
    /// and tints the entire line accordingly (`W/` → warn, `E/` → error,
    /// `D/` → dimmed).
    ///
    /// **Step 2 — Data-type overrides:** scans each row for string, number,
    /// boolean/null and time patterns, overriding individual cell colors.
    pub fn apply_regex_highlights(&mut self) {
        self.apply_regex_highlights_from(0);
    }

    /// Incremental variant for PTY feeds: only re-run the regexes over rows
    /// this feed could have touched — the live grid plus the `rows_scrolled`
    /// rows the feed just pushed into scrollback. Rows deeper in scrollback can
    /// never change again and keep the colors they got while still live.
    ///
    /// Without this, every output chunk re-highlighted the ENTIRE scrollback
    /// (8 regexes × up to SCROLLBACK_LIMIT rows) on the UI thread, making a
    /// busy terminal quadratic over the course of a session.
    pub fn apply_regex_highlights_incremental(&mut self, rows_scrolled: usize) {
        let mut from_row = self.scrollback.len().saturating_sub(rows_scrolled);
        // Lùi về đầu logical line (soft-wrap) để token vắt ngang ranh giới vẫn
        // được match trên text đầy đủ.
        while from_row > 0 && self.row_is_wrap_continued(from_row) {
            from_row -= 1;
        }
        self.apply_regex_highlights_from(from_row);
    }

    fn apply_regex_highlights_from(&mut self, from_row: usize) {
        use crate::terminal::highlighter::{
            RE_BOOL, RE_LOG_DEBUG, RE_LOG_ERROR, RE_LOG_WARN, RE_NULL, RE_NUMBER, RE_STRING,
            RE_TIME,
        };

        let colors = self.highlight_colors;
        let cols = self.cols;
        let total = self.total_rows();

        // Phase 1 (immutable borrow): walk the buffer one *logical* line at a
        // time and collect cell color overrides as `(absolute_row, col, color)`.
        //
        // A logical line is a maximal run of physical rows where each row after
        // the first is a soft-wrap continuation (`wrap_continued`). Running the
        // regexes over the joined text — instead of per physical row — keeps log
        // levels, strings, numbers and timestamps highlighted across the wrap
        // instead of resetting at every visual line break. Char index `i` in the
        // joined text maps back to row `start + i / cols`, col `i % cols`, since
        // every physical row contributes exactly `cols` chars.
        let mut updates: Vec<(usize, usize, [f32; 4])> = Vec::new();
        let mut start = from_row;
        while start < total {
            let mut end = start + 1;
            while end < total && self.row_is_wrap_continued(end) {
                end += 1;
            }

            let mut logical = String::with_capacity((end - start) * cols);
            for row in start..end {
                match self.row_cells_absolute(row) {
                    Some(cells) => logical.extend(cells.iter().map(|c| c.ch)),
                    None => logical.extend(std::iter::repeat(' ').take(cols)),
                }
            }

            let push_range = |updates: &mut Vec<(usize, usize, [f32; 4])>,
                              s: usize,
                              e: usize,
                              color: [f32; 4]| {
                for i in s..e {
                    updates.push((start + i / cols, i % cols, color));
                }
            };

            // Step 1: base line color from the log-level prefix, every cell.
            let line_color = if RE_LOG_ERROR.is_match(&logical) {
                Some(colors.error)
            } else if RE_LOG_WARN.is_match(&logical) {
                Some(colors.warn)
            } else if RE_LOG_DEBUG.is_match(&logical) {
                Some(colors.fg_dim)
            } else {
                None
            };
            if let Some(color) = line_color {
                push_range(&mut updates, 0, (end - start) * cols, color);
            }

            // Step 2: data-type overrides. Pushed lowest precedence first so
            // that — since later writes win in phase 2 — strings end up on top,
            // matching the original `find`-first ordering (string > number >
            // bool > null > time, all above the base line color).
            for (re, color) in [
                (&RE_TIME, colors.syntax_number),
                (&RE_NULL, colors.syntax_keyword),
                (&RE_BOOL, colors.syntax_keyword),
                (&RE_NUMBER, colors.syntax_number),
                (&RE_STRING, colors.syntax_string),
            ] {
                for mat in re.find_iter(&logical) {
                    let (s, e) = byte_to_char_range(&logical, mat.start(), mat.end());
                    push_range(&mut updates, s, e, color);
                }
            }

            start = end;
        }

        // Phase 2 (mutable borrow): apply the collected overrides in push order
        // so higher-precedence matches (pushed later) win on overlapping cells.
        for (row, col, color) in updates {
            self.set_style_fg_absolute(row, col, color);
        }
    }

    /// Whether the physical row at `absolute_row` is a soft-wrap continuation of
    /// the row above it (flag stored on its first cell at write time).
    fn row_is_wrap_continued(&self, absolute_row: usize) -> bool {
        self.row_cells_absolute(absolute_row)
            .and_then(|cells| cells.first())
            .is_some_and(|cell| cell.wrap_continued)
    }

    /// Set `style_fg` on the cell at an absolute `(row, col)`, resolving whether
    /// the row lives in scrollback or the live grid. Out-of-bounds is ignored.
    fn set_style_fg_absolute(&mut self, row: usize, col: usize, color: [f32; 4]) {
        if col >= self.cols {
            return;
        }
        if row < self.scrollback.len() {
            if let Some(cell) = self.scrollback[row].get_mut(col) {
                cell.style_fg = Some(color);
            }
        } else if let Some(live_row) = row.checked_sub(self.scrollback.len())
            && live_row < self.rows
        {
            if let Some(cell) = self.cells.get_mut(live_row * self.cols + col) {
                cell.style_fg = Some(color);
            }
        }
    }

    fn normalized_selection_bounds(&self) -> Option<(TerminalPoint, TerminalPoint)> {
        let anchor = self.selection_anchor?;
        let cursor = self.virtual_cursor;
        if point_leq(anchor, cursor) {
            Some((anchor, cursor))
        } else {
            Some((cursor, anchor))
        }
    }

    fn viewport_start_absolute_row(&self) -> usize {
        self.total_rows()
            .saturating_sub(self.rows)
            .saturating_sub(self.scroll_offset)
    }

    fn absolute_row_to_display_row(&self, absolute_row: usize) -> Option<usize> {
        let top = self.viewport_start_absolute_row();
        let bottom = top + self.rows.saturating_sub(1);
        if absolute_row < top || absolute_row > bottom {
            return None;
        }
        Some(absolute_row - top)
    }

    fn set_viewport_top_absolute_row(&mut self, top_row: usize) {
        let max_top = self.total_rows().saturating_sub(self.rows);
        let clamped_top = top_row.min(max_top);
        self.scroll_offset = max_top
            .saturating_sub(clamped_top)
            .min(self.scrollback.len());
    }

    fn ensure_virtual_cursor_visible(&mut self) {
        let current_top = self.viewport_start_absolute_row();
        let current_bottom = current_top + self.rows.saturating_sub(1);
        if self.virtual_cursor.row < current_top {
            self.set_viewport_top_absolute_row(self.virtual_cursor.row);
        } else if self.virtual_cursor.row > current_bottom {
            let next_top = self
                .virtual_cursor
                .row
                .saturating_sub(self.rows.saturating_sub(1));
            self.set_viewport_top_absolute_row(next_top);
        }
    }

    fn set_virtual_cursor(&mut self, next: TerminalPoint) -> bool {
        let clamped = self.clamp_point(next);
        let old_cursor = self.virtual_cursor;
        let old_scroll = self.scroll_offset;
        self.virtual_cursor = clamped;
        self.ensure_virtual_cursor_visible();
        old_cursor != self.virtual_cursor || old_scroll != self.scroll_offset
    }

    fn clamp_point(&self, point: TerminalPoint) -> TerminalPoint {
        TerminalPoint {
            row: point.row.min(self.total_rows().saturating_sub(1)),
            col: point.col.min(self.cols.saturating_sub(1)),
        }
    }

    fn clamp_virtual_points(&mut self) {
        let max_row = self.total_rows().saturating_sub(1);
        let max_col = self.cols.saturating_sub(1);
        self.virtual_cursor = self.clamp_point(self.virtual_cursor);
        if let Some(anchor) = self.selection_anchor.as_mut() {
            *anchor = TerminalPoint {
                row: anchor.row.min(max_row),
                col: anchor.col.min(max_col),
            };
        }
    }

    fn shift_points_up_after_front_trim(&mut self, removed_rows: usize) {
        if removed_rows == 0 {
            return;
        }

        self.virtual_cursor.row = self.virtual_cursor.row.saturating_sub(removed_rows);
        if let Some(anchor) = self.selection_anchor.as_mut() {
            anchor.row = anchor.row.saturating_sub(removed_rows);
        }
    }

    fn row_cells_absolute(&self, row: usize) -> Option<&[TerminalCell]> {
        if row < self.scrollback.len() {
            return Some(&self.scrollback[row]);
        }
        let live_row = row.checked_sub(self.scrollback.len())?;
        if live_row >= self.rows {
            return None;
        }
        let start = live_row * self.cols;
        Some(&self.cells[start..start + self.cols])
    }

    fn line_end_col_for_absolute_row(&self, row: usize) -> usize {
        let Some(row_cells) = self.row_cells_absolute(row) else {
            return 0;
        };
        row_cells
            .iter()
            .rposition(|cell| cell.ch != ' ')
            .unwrap_or(0)
    }

    fn first_non_whitespace_col_for_absolute_row(&self, row: usize) -> usize {
        let Some(row_cells) = self.row_cells_absolute(row) else {
            return 0;
        };
        row_cells
            .iter()
            .position(|cell| !cell.ch.is_whitespace())
            .unwrap_or(0)
    }

    fn flattened_text(&self) -> Vec<char> {
        let total_rows = self.total_rows();
        let mut out = Vec::with_capacity(total_rows * self.cols + total_rows.saturating_sub(1));
        for row in 0..total_rows {
            if let Some(row_cells) = self.row_cells_absolute(row) {
                out.extend(row_cells.iter().map(|cell| cell.ch));
            }
            if row + 1 < total_rows {
                out.push('\n');
            }
        }
        out
    }

    fn flat_index_for_point(&self, point: TerminalPoint) -> usize {
        point.row.saturating_mul(self.cols + 1) + point.col.min(self.cols.saturating_sub(1))
    }

    fn point_for_flat_index(&self, index: usize) -> TerminalPoint {
        let stride = self.cols + 1;
        let total_rows = self.total_rows().max(1);
        let row = (index / stride).min(total_rows.saturating_sub(1));
        let slot = index % stride;
        if slot >= self.cols {
            TerminalPoint {
                row: (row + 1).min(total_rows.saturating_sub(1)),
                col: 0,
            }
        } else {
            TerminalPoint {
                row,
                col: slot.min(self.cols.saturating_sub(1)),
            }
        }
    }

    fn word_motion_target(&self, motion: WordMotion) -> Option<TerminalPoint> {
        let text = self.flattened_text();
        if text.is_empty() {
            return None;
        }
        let cursor = self
            .flat_index_for_point(self.virtual_cursor)
            .min(text.len() - 1);
        let next = match motion {
            WordMotion::Forward => next_word_start_chars(&text, cursor),
            WordMotion::Backward => previous_word_start_chars(&text, cursor),
            WordMotion::End => word_end_at_or_after_chars(&text, cursor)?,
        };
        Some(self.point_for_flat_index(next))
    }

    // ─── Terminal search ────────────────────────────────────────────────────

    /// Extract plain text from all rows (scrollback + visible grid).
    ///
    /// Returns one `String` per absolute row, with each cell's character
    /// concatenated.  Trailing whitespace is preserved.
    pub fn get_scrollback_text(&self) -> Vec<String> {
        let total = self.total_rows();
        let mut lines = Vec::with_capacity(total);
        for row in 0..total {
            if let Some(cells) = self.row_cells_absolute(row) {
                lines.push(cells.iter().map(|c| c.ch).collect());
            }
        }
        lines
    }

    /// Search all terminal text for `query` and populate `search_matches`.
    ///
    /// When `whole_word` is `true`, matches are only kept when the character
    /// before and after the match is a word boundary (start/end of line,
    /// whitespace, or punctuation).
    pub fn search_in_terminal(&mut self, query: &str, whole_word: bool) {
        self.search_matches.clear();
        self.search_cursor = 0;

        if query.is_empty() {
            return;
        }

        let lines = self.get_scrollback_text();
        let q_len = query.chars().count();

        for (row, line) in lines.iter().enumerate() {
            let mut search_start = 0;
            while let Some(byte_pos) = line[search_start..].find(query) {
                // `str::find` returns a byte offset; convert to char index.
                let col = line[..search_start + byte_pos].chars().count();

                if whole_word {
                    let before_ok = if col == 0 {
                        true
                    } else {
                        line.chars()
                            .nth(col.wrapping_sub(1))
                            .map_or(true, |ch| !ch.is_alphanumeric() && ch != '_')
                    };
                    let after_col = col + q_len;
                    let after_ok = line
                        .chars()
                        .nth(after_col)
                        .map_or(true, |ch| !ch.is_alphanumeric() && ch != '_');

                    if !before_ok || !after_ok {
                        search_start += byte_pos + 1;
                        continue;
                    }
                }

                self.search_matches.push(TerminalSearchMatch {
                    row,
                    col,
                    len: q_len,
                });
                search_start += byte_pos + 1;
            }
        }
    }

    /// Advance to the next search match, wrapping around if necessary.
    ///
    /// Returns the `TerminalPoint` of the new current match, or `None` when
    /// there are no matches at all.  The viewport is automatically scrolled so
    /// that the match is visible.
    pub fn search_next(&mut self) -> Option<TerminalPoint> {
        if self.search_matches.is_empty() {
            return None;
        }
        self.search_cursor = (self.search_cursor + 1) % self.search_matches.len();
        let m = self.search_matches[self.search_cursor];
        let point = TerminalPoint {
            row: m.row,
            col: m.col,
        };
        self.virtual_cursor = self.clamp_point(point);
        self.ensure_virtual_cursor_visible();
        Some(self.virtual_cursor)
    }

    /// Move to the previous search match, wrapping around if necessary.
    ///
    /// Returns the `TerminalPoint` of the new current match, or `None` when
    /// there are no matches.  The viewport is automatically scrolled.
    pub fn search_prev(&mut self) -> Option<TerminalPoint> {
        if self.search_matches.is_empty() {
            return None;
        }
        self.search_cursor = if self.search_cursor == 0 {
            self.search_matches.len() - 1
        } else {
            self.search_cursor - 1
        };
        let m = self.search_matches[self.search_cursor];
        let point = TerminalPoint {
            row: m.row,
            col: m.col,
        };
        self.virtual_cursor = self.clamp_point(point);
        self.ensure_virtual_cursor_visible();
        Some(self.virtual_cursor)
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
    style_fg: None,
    wrap_continued: false,
};

/// Convert a byte-offset range from a regex match into a char-index range.
fn byte_to_char_range(text: &str, byte_start: usize, byte_end: usize) -> (usize, usize) {
    let char_start = text[..byte_start].chars().count();
    let char_end = char_start + text[byte_start..byte_end].chars().count();
    (char_start, char_end)
}

/// i32 addition với clamp về [min, max].
fn clamp_add(base: usize, delta: i32, min: usize, max: usize) -> usize {
    let result = base as i32 + delta;
    result.max(min as i32).min(max as i32) as usize
}

fn resize_terminal_row(row: &mut Vec<TerminalCell>, new_cols: usize) {
    if row.len() > new_cols {
        row.truncate(new_cols);
    } else if row.len() < new_cols {
        row.resize(new_cols, TerminalCell::blank());
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum WordClass {
    Space,
    Word,
    Punct,
    Newline,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum WordMotion {
    Forward,
    Backward,
    End,
}

fn point_leq(left: TerminalPoint, right: TerminalPoint) -> bool {
    left.row < right.row || (left.row == right.row && left.col <= right.col)
}

fn classify_terminal_char(ch: char) -> WordClass {
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

fn next_word_start_chars(text: &[char], cursor: usize) -> usize {
    let n = text.len();
    if cursor >= n {
        return cursor;
    }

    let start_class = classify_terminal_char(text[cursor]);
    if start_class == WordClass::Newline {
        return (cursor + 1).min(n.saturating_sub(1));
    }

    let mut i = cursor;
    if start_class == WordClass::Space {
        while i < n {
            let cls = classify_terminal_char(text[i]);
            if cls != WordClass::Space {
                break;
            }
            i += 1;
        }
        return i.min(n.saturating_sub(1));
    }

    while i < n && classify_terminal_char(text[i]) == start_class {
        i += 1;
    }
    while i < n && classify_terminal_char(text[i]) == WordClass::Space {
        i += 1;
    }
    i.min(n.saturating_sub(1))
}

fn previous_word_start_chars(text: &[char], cursor: usize) -> usize {
    if cursor == 0 {
        return 0;
    }

    let mut i = cursor.saturating_sub(1);
    while i > 0 {
        let cls = classify_terminal_char(text[i]);
        if cls != WordClass::Space && cls != WordClass::Newline {
            break;
        }
        i -= 1;
    }

    if classify_terminal_char(text[i]) == WordClass::Space
        || classify_terminal_char(text[i]) == WordClass::Newline
    {
        return i;
    }

    let cls = classify_terminal_char(text[i]);
    while i > 0 && classify_terminal_char(text[i - 1]) == cls {
        i -= 1;
    }
    i
}

fn word_end_at_or_after_chars(text: &[char], cursor: usize) -> Option<usize> {
    let n = text.len();
    if n == 0 || cursor >= n {
        return None;
    }

    let mut i = cursor;
    let start_class = classify_terminal_char(text[i]);

    if start_class != WordClass::Space && start_class != WordClass::Newline {
        let mut end = i;
        while end + 1 < n && classify_terminal_char(text[end + 1]) == start_class {
            end += 1;
        }
        if end > i {
            return Some(end);
        }
        i = end.saturating_add(1);
    }

    while i < n {
        let cls = classify_terminal_char(text[i]);
        if cls != WordClass::Space && cls != WordClass::Newline {
            break;
        }
        i += 1;
    }
    if i >= n {
        return None;
    }

    let cls = classify_terminal_char(text[i]);
    while i + 1 < n && classify_terminal_char(text[i + 1]) == cls {
        i += 1;
    }
    Some(i)
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn terminal_keeps_ten_thousand_scrollback_rows() {
        let mut grid = TerminalGrid::new(8, 2);
        let output = (0..10_500)
            .map(|idx| format!("{idx}\r\n"))
            .collect::<String>();

        grid.feed_chunk(&output);

        assert_eq!(grid.scrollback.len(), 10_000);
    }
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
    fn combining_marks_fold_into_base_cell() {
        let mut grid = TerminalGrid::new(10, 2);
        // "cụ" written decomposed: 'c', 'u', then combining dot-below (U+0323).
        grid.feed_chunk("cu\u{0323}");
        assert_eq!(grid.cell_at(0, 0).ch, 'c');
        // 'u' + U+0323 composes to 'ụ' (U+1EE5) in the same cell.
        assert_eq!(grid.cell_at(0, 1).ch, 'ụ');
        // The mark is zero-width: cursor only advanced for 'c' and 'u'.
        assert_eq!(grid.cursor_col, 2);
    }

    #[test]
    fn stacked_combining_marks_compose_incrementally() {
        let mut grid = TerminalGrid::new(10, 2);
        // "ộ" decomposed: 'o' + circumflex (U+0302) + dot-below (U+0323).
        grid.feed_chunk("o\u{0302}\u{0323}");
        assert_eq!(grid.cell_at(0, 0).ch, 'ộ');
        assert_eq!(grid.cursor_col, 1);
    }

    #[test]
    fn precomposed_vietnamese_passes_through() {
        let mut grid = TerminalGrid::new(10, 2);
        grid.feed_chunk("ộ");
        assert_eq!(grid.cell_at(0, 0).ch, 'ộ');
        assert_eq!(grid.cursor_col, 1);
    }

    #[test]
    fn vietnamese_phrase_keeps_precomposed_letters() {
        let mut grid = TerminalGrid::new(40, 2);
        grid.feed_chunk("Tôi không đổi tiếng Việt");
        let rendered: String = (0..24).map(|col| grid.cell_at(0, col).ch).collect();
        assert!(rendered.starts_with("Tôi không đổi tiếng Việt"));
    }

    #[test]
    fn vietnamese_utf8_split_across_chunks_is_preserved() {
        let mut grid = TerminalGrid::new(10, 2);
        let bytes = "đổi".as_bytes();
        grid.feed_bytes(&bytes[..1]);
        grid.feed_bytes(&bytes[1..3]);
        grid.feed_bytes(&bytes[3..]);
        assert_eq!(grid.cell_at(0, 0).ch, 'đ');
        assert_eq!(grid.cell_at(0, 1).ch, 'ổ');
        assert_eq!(grid.cell_at(0, 2).ch, 'i');
        assert_eq!(grid.cursor_col, 3);
    }

    #[test]
    fn leading_combining_mark_is_dropped_not_misaligned() {
        let mut grid = TerminalGrid::new(10, 2);
        // A combining mark with no preceding base must not consume a cell.
        grid.feed_chunk("\u{0323}a");
        assert_eq!(grid.cell_at(0, 0).ch, 'a');
        assert_eq!(grid.cursor_col, 1);
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
    fn delete_chars_shifts_line_left() {
        // DCH (`ESC[P`) — cách shell xoá ký tự giữa dòng với TERM=xterm-256color.
        let mut grid = TerminalGrid::new(10, 3);
        grid.feed_chunk("HELLO");
        // Cursor về col 1 (giữa "ELLO"), xoá 1 ký tự → "HLLO".
        grid.feed_chunk("\x1b[1;2H\x1b[P");
        assert_eq!(grid.cell_at(0, 0).ch, 'H');
        assert_eq!(grid.cell_at(0, 1).ch, 'L');
        assert_eq!(grid.cell_at(0, 2).ch, 'L');
        assert_eq!(grid.cell_at(0, 3).ch, 'O');
        assert_eq!(grid.cell_at(0, 4).ch, ' ');
    }

    #[test]
    fn delete_chars_multi_and_clamped_at_line_end() {
        let mut grid = TerminalGrid::new(6, 2);
        grid.feed_chunk("ABCDEF");
        grid.feed_chunk("\x1b[1;2H\x1b[3P"); // xoá 3 từ col 1 → "AEF"
        assert_eq!(grid.cell_at(0, 0).ch, 'A');
        assert_eq!(grid.cell_at(0, 1).ch, 'E');
        assert_eq!(grid.cell_at(0, 2).ch, 'F');
        assert_eq!(grid.cell_at(0, 3).ch, ' ');
        // Xoá nhiều hơn số cell còn lại → clamp, không panic.
        grid.feed_chunk("\x1b[1;2H\x1b[99P");
        assert_eq!(grid.cell_at(0, 0).ch, 'A');
        assert_eq!(grid.cell_at(0, 1).ch, ' ');
    }

    #[test]
    fn insert_chars_shifts_line_right() {
        // ICH (`ESC[@`) — chèn ô trống tại cursor.
        let mut grid = TerminalGrid::new(8, 2);
        grid.feed_chunk("ABCD");
        grid.feed_chunk("\x1b[1;2H\x1b[2@"); // chèn 2 ô tại col 1 → "A  BCD"
        assert_eq!(grid.cell_at(0, 0).ch, 'A');
        assert_eq!(grid.cell_at(0, 1).ch, ' ');
        assert_eq!(grid.cell_at(0, 2).ch, ' ');
        assert_eq!(grid.cell_at(0, 3).ch, 'B');
        assert_eq!(grid.cell_at(0, 4).ch, 'C');
        assert_eq!(grid.cell_at(0, 5).ch, 'D');
    }

    #[test]
    fn erase_chars_blanks_without_shifting() {
        // ECH (`ESC[X`) — blank n ô, phần sau giữ nguyên.
        let mut grid = TerminalGrid::new(10, 2);
        grid.feed_chunk("HELLO");
        grid.feed_chunk("\x1b[1;2H\x1b[2X"); // blank 2 ô từ col 1 → "H  LO"
        assert_eq!(grid.cell_at(0, 0).ch, 'H');
        assert_eq!(grid.cell_at(0, 1).ch, ' ');
        assert_eq!(grid.cell_at(0, 2).ch, ' ');
        assert_eq!(grid.cell_at(0, 3).ch, 'L');
        assert_eq!(grid.cell_at(0, 4).ch, 'O');
    }

    #[test]
    fn sync_virtual_cursor_follows_shell_cursor() {
        // Vim-style line editing: shell echo làm cursor di chuyển (qua readline
        // sequences), virtual cursor của T-COPY phải bám theo.
        let mut grid = TerminalGrid::new(10, 3);
        grid.feed_chunk("HELLO");
        grid.enter_normal_mode();
        assert_eq!(grid.virtual_cursor.col, 5);
        // Shell cursor lùi 2 (như khi user bấm h h → ESC[D ESC[D được echo).
        grid.feed_chunk("\x1b[2D");
        grid.sync_virtual_cursor_to_shell();
        assert_eq!(grid.virtual_cursor.col, 3);
        assert_eq!(grid.virtual_cursor.row, 0);
    }

    #[test]
    fn sync_virtual_cursor_keeps_selection_anchor() {
        let mut grid = TerminalGrid::new(10, 3);
        grid.feed_chunk("HELLO");
        grid.enter_normal_mode();
        grid.begin_selection();
        grid.feed_chunk("\x1b[2D");
        grid.sync_virtual_cursor_to_shell();
        // Đang visual select → không nhảy virtual cursor theo shell.
        assert_eq!(grid.virtual_cursor.col, 5);
    }

    #[test]
    fn sync_virtual_cursor_leaves_copy_mode_navigation_alone() {
        // User đã đưa cursor lên output phía trên prompt → output/echo mới đến
        // không được kéo cursor về prompt.
        let mut grid = TerminalGrid::new(10, 3);
        grid.feed_chunk("out\r\n$ cmd");
        grid.enter_normal_mode();
        grid.move_virtual_up();
        let parked = grid.virtual_cursor;
        grid.feed_chunk("\x1b[2D");
        grid.sync_virtual_cursor_to_shell();
        assert_eq!(grid.virtual_cursor, parked);
    }

    #[test]
    fn shell_line_editing_active_only_at_or_below_prompt_row() {
        let mut grid = TerminalGrid::new(10, 4);
        grid.feed_chunk("out\r\n$ cmd");
        grid.enter_normal_mode();
        assert!(grid.shell_line_editing_active());

        // Xuống dưới prompt (row trống) vẫn là shell-line territory → G hoạt động.
        grid.move_virtual_down();
        assert!(grid.shell_line_editing_active());

        // Lên output phía trên prompt → copy-mode territory.
        grid.move_virtual_up();
        grid.move_virtual_up();
        assert!(!grid.shell_line_editing_active());

        // Visual selection tắt shell-line editing kể cả trên prompt row.
        grid.enter_normal_mode();
        grid.begin_selection();
        assert!(!grid.shell_line_editing_active());
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
    fn used_rows_counts_background_colored_spaces_as_visible_content() {
        let mut grid = TerminalGrid::new(10, 3);
        grid.feed_chunk("\x1b[48;5;196m \x1b[0m");
        assert_eq!(grid.used_rows(), 1);
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

    #[test]
    fn resize_keeps_recent_rows_when_shrinking_height() {
        let mut grid = TerminalGrid::new(4, 3);
        grid.feed_chunk("1111\r\n2222\r\n3333");

        assert!(grid.resize(4, 2));
        assert_eq!(grid.debug_dump(), "2222\n3333\n");
    }

    #[test]
    fn resize_expands_width_without_losing_existing_text() {
        let mut grid = TerminalGrid::new(4, 2);
        grid.feed_chunk("ab");

        assert!(grid.resize(8, 2));
        assert_eq!(grid.debug_dump(), "ab\n");
        assert_eq!(grid.cols, 8);
    }

    #[test]
    fn terminal_normal_selection_extracts_trimmed_multiline_text() {
        let mut grid = TerminalGrid::new(8, 4);
        let _ = grid.feed_chunk("alpha\r\nbeta\r\ngamma");
        assert!(grid.enter_normal_mode());
        assert!(grid.move_virtual_up());
        assert!(grid.move_virtual_to_line_start());
        assert!(grid.begin_selection());
        assert!(grid.move_virtual_word_end());

        assert_eq!(grid.yank_selection_text().as_deref(), Some("beta"));
    }

    #[test]
    fn virtual_cursor_visibility_tracks_scrollback_viewport() {
        let mut grid = TerminalGrid::new(5, 3);
        let _ = grid.feed_chunk("11111\r\n22222\r\n33333\r\n44444\r\n55555");
        assert!(grid.enter_normal_mode());
        assert_eq!(
            grid.virtual_cursor_display_position(),
            Some((2, 5.min(grid.cols - 1)))
        );

        assert!(grid.move_virtual_up());
        assert!(grid.move_virtual_up());
        assert!(grid.move_virtual_up());
        let (display_row, _) = grid
            .virtual_cursor_display_position()
            .expect("cursor should stay in viewport");
        assert_eq!(display_row, 0);
        assert!(grid.scroll_offset > 0);
    }

    #[test]
    fn apply_regex_highlights_warn_line_gets_warn_color() {
        let mut grid = TerminalGrid::new(40, 5);
        grid.feed_chunk("W/Network timeout occurred\nnormal line");
        grid.apply_regex_highlights();

        // First row (W/ line) should have warn style_fg on every cell.
        let warn = grid.highlight_colors.warn;
        for col in 0..grid.cols {
            let cell = grid.cell_at(0, col);
            if cell.ch != ' ' {
                assert_eq!(cell.style_fg, Some(warn), "col {col}");
            }
        }
        // Second row should have no style_fg.
        let cell = grid.cell_at(1, 0);
        assert_eq!(cell.style_fg, None);
    }

    #[test]
    fn apply_regex_highlights_error_line_gets_error_color() {
        let mut grid = TerminalGrid::new(40, 5);
        grid.feed_chunk("E/Fatal crash in module\nother text");
        grid.apply_regex_highlights();

        let error = grid.highlight_colors.error;
        let cell = grid.cell_at(0, 0);
        assert_eq!(cell.style_fg, Some(error));
    }

    #[test]
    fn apply_regex_highlights_debug_line_gets_fg_dim() {
        let mut grid = TerminalGrid::new(40, 5);
        grid.feed_chunk("D/verbose debug output\nother text");
        grid.apply_regex_highlights();

        let fg_dim = grid.highlight_colors.fg_dim;
        let cell = grid.cell_at(0, 0);
        assert_eq!(cell.style_fg, Some(fg_dim));
    }

    #[test]
    fn apply_regex_highlights_string_overrides_line_color() {
        let mut grid = TerminalGrid::new(60, 5);
        grid.feed_chunk("W/Loaded \"config.json\" successfully");
        grid.apply_regex_highlights();

        let warn = grid.highlight_colors.warn;
        let syn_str = grid.highlight_colors.syntax_string;

        // "W/Loaded "config.json" successfully"
        //  0123456789...
        // 'W' at col 0 is part of W/ prefix — still warn color.
        assert_eq!(grid.cell_at(0, 0).style_fg, Some(warn));
        // '"' at col 9 starts the string — should be syntax_string.
        assert_eq!(grid.cell_at(0, 9).style_fg, Some(syn_str));
        // Space at col 8 is not in any data-type pattern — keeps warn.
        assert_eq!(grid.cell_at(0, 8).style_fg, Some(warn));
    }

    #[test]
    fn apply_regex_highlights_number_and_bool_get_keyword_colors() {
        let mut grid = TerminalGrid::new(60, 5);
        grid.feed_chunk("count=42 enabled=true nothing=null");
        grid.apply_regex_highlights();

        let syn_num = grid.highlight_colors.syntax_number;
        let syn_kw = grid.highlight_colors.syntax_keyword;

        // "count=42 enabled=true nothing=null"
        //  01234567890123456789012345678901234
        // '4' at col 6 is part of the number 42.
        assert_eq!(grid.cell_at(0, 6).style_fg, Some(syn_num));
        // 't' at col 17 starts "true".
        assert_eq!(grid.cell_at(0, 17).style_fg, Some(syn_kw));
        // 'n' at col 30 starts "null".
        assert_eq!(grid.cell_at(0, 30).style_fg, Some(syn_kw));
    }

    #[test]
    fn apply_regex_highlights_string_spanning_soft_wrap_keeps_color_on_continuation() {
        // A quoted string longer than the grid width soft-wraps onto a second
        // physical row. The closing quote is on row 1, so neither row matches
        // RE_STRING on its own — only the joined logical line does.
        let mut grid = TerminalGrid::new(8, 4);
        grid.feed_chunk("\"abcdefghij\"");
        grid.apply_regex_highlights();

        let syn_str = grid.highlight_colors.syntax_string;
        // Row 1 is flagged as a wrap continuation of row 0.
        assert!(grid.cell_at(1, 0).wrap_continued);
        // Opening quote (row 0) and the wrapped tail (row 1) share string color.
        assert_eq!(grid.cell_at(0, 0).style_fg, Some(syn_str));
        assert_eq!(grid.cell_at(1, 0).style_fg, Some(syn_str)); // 'h'
        assert_eq!(grid.cell_at(1, 3).style_fg, Some(syn_str)); // closing quote
    }

    #[test]
    fn apply_regex_highlights_log_level_spans_soft_wrap() {
        // An error line wraps; the continuation row must keep the error tint
        // even though the "E/" prefix only appears on the first physical row.
        let mut grid = TerminalGrid::new(8, 4);
        grid.feed_chunk("E/fatal boom");
        grid.apply_regex_highlights();

        let error = grid.highlight_colors.error;
        assert!(grid.cell_at(1, 0).wrap_continued);
        assert_eq!(grid.cell_at(0, 0).style_fg, Some(error));
        assert_eq!(grid.cell_at(1, 0).style_fg, Some(error));
    }

    #[test]
    fn apply_regex_highlights_incremental_covers_rows_scrolled_into_scrollback() {
        // 3 visible rows: feeding 5 log lines pushes 2 into scrollback within a
        // single feed. The incremental pass must still color those 2 rows.
        let mut grid = TerminalGrid::new(20, 3);
        let scrolled = grid
            .feed_chunk("E/boom one\r\nE/boom two\r\nE/boom three\r\nE/boom four\r\nE/boom five");
        grid.apply_regex_highlights_incremental(scrolled);

        let error = grid.highlight_colors.error;
        let in_scrollback = grid.scrollback.len();
        assert!(in_scrollback >= 2, "expected rows pushed into scrollback");
        assert!(
            scrolled >= in_scrollback,
            "feed must report at least the rows it scrolled"
        );
        for row in 0..in_scrollback {
            assert_eq!(
                grid.scrollback[row][0].style_fg,
                Some(error),
                "scrollback row {row} must be highlighted"
            );
        }
        // Live rows.
        for row in 0..3 {
            assert_eq!(grid.cell_at(row, 0).style_fg, Some(error));
        }
    }

    #[test]
    fn apply_regex_highlights_incremental_does_not_touch_old_scrollback() {
        let mut grid = TerminalGrid::new(20, 2);
        // First feed scrolls a warn line into scrollback and highlights it.
        let scrolled = grid.feed_chunk("W/old warning\r\nplain\r\nplain2");
        grid.apply_regex_highlights_incremental(scrolled);
        let warn = grid.highlight_colors.warn;
        assert_eq!(grid.scrollback[0][0].style_fg, Some(warn));

        // Wipe the recorded color to detect re-processing, then feed more
        // output WITHOUT scrolling past that old row.
        grid.scrollback[0][0].style_fg = None;
        let scrolled = grid.feed_chunk("plain3\r\nplain4");
        grid.apply_regex_highlights_incremental(scrolled);

        // Old scrollback row was outside the incremental window — untouched.
        assert_eq!(grid.scrollback[0][0].style_fg, None);
    }
}
