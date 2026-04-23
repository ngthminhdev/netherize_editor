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

// ─── TerminalGrid ─────────────────────────────────────────────────────────────

/// Terminal display grid với kích thước cố định `cols × rows`.
///
/// Cells được lưu row-major: `cells[row * cols + col]`.
/// Khi cursor xuống quá dòng cuối, grid **scroll up** (xóa dòng đầu, thêm dòng trống cuối).
const SCROLLBACK_LIMIT: usize = 500;

pub struct TerminalGrid {
    pub cols: usize,
    pub rows: usize,
    cells: Vec<TerminalCell>,

    /// Scrollback buffer — rows pushed off the top of the live grid.
    scrollback: Vec<Vec<TerminalCell>>,

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
            scrollback: Vec::new(),
            scroll_offset: 0,
            cursor_row: 0,
            cursor_col: 0,
            virtual_cursor: TerminalPoint { row: 0, col: 0 },
            selection_anchor: None,
            current_style: CellStyle::default(),
            parser: AnsiParser::new(),
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
        let mut scrolled = 0usize;
        if self.cursor_col >= self.cols {
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
        };
        self.cursor_col += 1;
        scrolled
    }

    /// Scroll lên 1 dòng: push dòng đầu vào scrollback, thêm dòng trống dưới.
    fn scroll_up(&mut self) {
        let pushed_row: Vec<TerminalCell> = self.cells[0..self.cols].to_vec();
        self.scrollback.push(pushed_row);
        let overflowed = if self.scrollback.len() > SCROLLBACK_LIMIT {
            self.scrollback.remove(0);
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
                self.scrollback.push(row);
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
}
