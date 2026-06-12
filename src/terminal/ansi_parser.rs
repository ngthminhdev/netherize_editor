//! ANSI escape sequence parser — Phase 9b.
//!
//! Phạm vi hỗ trợ (in-scope):
//!   - Ký tự in được          → `AnsiEvent::PrintChar`
//!   - `\n`                   → `AnsiEvent::Newline`
//!   - `\r`                   → `AnsiEvent::CarriageReturn`
//!   - `\x08` (Backspace)     → `AnsiEvent::Backspace`
//!   - `ESC[...m` (SGR)       → SetFg / SetBg / ResetStyle / Bold / BoldOff
//!   - `ESC[...A/B/C/D` (CUD) → CursorMove (row_delta, col_delta) — phục vụ prompt zsh
//!   - `ESC[H`, `ESC[f`       → CursorGoto(row, col)
//!
//! Out-of-scope (parse nhưng bỏ qua ở grid layer):
//!   - `ESC[J`, `ESC[K`  (erase display / line)
//!   - `ESC[?...h/l`     (private mode: alt-screen, mouse, etc.)
//!   - `ESC]...` (OSC)   (title, color set, etc.)

use crate::config::theme_config::srgb_rgba_to_linear_f32;

// ─── Màu sắc ────────────────────────────────────────────────────────────────

/// Đại diện màu trong ANSI terminal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnsiColor {
    /// Màu mặc định của terminal (fg = foreground default, bg = background default).
    Default,
    /// 4-bit ANSI standard color (index 0-7 / bright 8-15).
    Index(u8),
    /// 24-bit truecolor `ESC[38;2;r;g;bm`.
    Rgb(u8, u8, u8),
}

impl AnsiColor {
    /// Chuyển sang `[f32; 4]` RGBA để upload lên GPU.
    /// Dùng bảng màu xterm-256 chuẩn cho `Index`.
    pub fn to_rgba_f32(self, is_foreground: bool) -> [f32; 4] {
        self.to_rgba_f32_with_defaults(
            [0.92, 0.92, 0.92, 1.0],
            [0.08, 0.10, 0.14, 1.0],
            is_foreground,
        )
    }

    pub fn to_rgba_f32_with_defaults(
        self,
        default_fg: [f32; 4],
        default_bg: [f32; 4],
        is_foreground: bool,
    ) -> [f32; 4] {
        match self {
            AnsiColor::Default => {
                if is_foreground {
                    default_fg
                } else {
                    default_bg
                }
            }
            AnsiColor::Index(idx) => {
                let (r, g, b) = xterm256_to_rgb(idx);
                srgb_rgba_to_linear_f32([r, g, b, 255])
            }
            AnsiColor::Rgb(r, g, b) => srgb_rgba_to_linear_f32([r, g, b, 255]),
        }
    }
}

// ─── Events ─────────────────────────────────────────────────────────────────

/// Kết quả parse của một đơn vị ANSI input.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AnsiEvent {
    /// Ký tự bình thường cần in ra cell.
    PrintChar(char),
    /// LF (`\n`) — xuống dòng.
    Newline,
    /// CR (`\r`) — về đầu dòng.
    CarriageReturn,
    /// Backspace (`\x08`) — lùi một cột.
    Backspace,
    /// SGR: reset tất cả style.
    ResetStyle,
    /// SGR: đặt màu foreground.
    SetFg(AnsiColor),
    /// SGR: đặt màu background.
    SetBg(AnsiColor),
    /// SGR: bold on/off.
    SetBold(bool),
    /// Di chuyển cursor tương đối (row_delta, col_delta).
    CursorMove { row_delta: i32, col_delta: i32 },
    /// Di chuyển cursor tuyệt đối (1-based từ terminal, đã convert 0-based).
    CursorGoto { row: usize, col: usize },
    /// Xoá màn hình / dòng — grid layer sẽ quyết định có xử lý không.
    EraseDisplay(u8),
    /// Xoá tới cuối / đầu dòng.
    EraseLine(u8),
    /// CSI sequence không nhận ra — bỏ qua.
    Unknown,
}

// ─── Parser state machine ────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TerminalCharset {
    Ascii,
    DecSpecialGraphics,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CharsetTarget {
    G0,
    G1,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum ParseState {
    /// Đang đọc ký tự bình thường.
    Normal,
    /// Vừa thấy ESC (0x1b).
    EscapeStart,
    /// Đang đọc trong CSI (`ESC[`), tích lũy param bytes.
    Csi { buf: Vec<u8> },
    /// Đang đọc OSC (`ESC]`), bỏ qua cho đến ST (ESC\ hoặc BEL).
    Osc,
    /// Vừa thấy ESC trong OSC — consume `\` nếu đây là ST terminator.
    OscEsc,
    /// Sau ESC, thấy `(` hoặc `)` — charset indicator cho G0/G1.
    CharsetSelect(CharsetTarget),
}

/// Bộ parser streaming — dùng bằng cách gọi `parse_chunk` lặp lại.
///
/// Parser không lưu kết quả; mọi event được emit qua closure `emit` ngay lập tức,
/// giúp tránh allocation trung gian khi chunk lớn.
#[derive(Clone)]
pub struct AnsiParser {
    state: ParseState,
    g0_charset: TerminalCharset,
    g1_charset: TerminalCharset,
    invoked_charset: CharsetTarget,
    /// UTF-8 partial decode buffer (tối đa 4 bytes).
    utf8_buf: [u8; 4],
    utf8_len: usize,
}

impl AnsiParser {
    pub fn new() -> Self {
        Self {
            state: ParseState::Normal,
            g0_charset: TerminalCharset::Ascii,
            g1_charset: TerminalCharset::Ascii,
            invoked_charset: CharsetTarget::G0,
            utf8_buf: [0u8; 4],
            utf8_len: 0,
        }
    }

    /// Nạp một chunk bytes (raw PTY output) và emit events qua closure.
    pub fn feed_bytes(&mut self, data: &[u8], emit: &mut impl FnMut(AnsiEvent)) {
        for &byte in data {
            self.feed_byte(byte, emit);
        }
    }

    /// Nạp string UTF-8 đã giải mã (chunk từ PTY nếu đã là String).
    pub fn feed_str(&mut self, s: &str, emit: &mut impl FnMut(AnsiEvent)) {
        self.feed_bytes(s.as_bytes(), emit);
    }

    fn feed_byte(&mut self, byte: u8, emit: &mut impl FnMut(AnsiEvent)) {
        match self.state {
            ParseState::Normal => self.handle_normal(byte, emit),
            ParseState::EscapeStart => self.handle_escape_start(byte, emit),
            ParseState::Csi { .. } => self.handle_csi(byte, emit),
            ParseState::Osc => self.handle_osc(byte),
            ParseState::OscEsc => self.handle_osc_esc(byte),
            ParseState::CharsetSelect(target) => self.handle_charset_select(byte, target),
        }
    }

    fn handle_normal(&mut self, byte: u8, emit: &mut impl FnMut(AnsiEvent)) {
        match byte {
            0x1b => {
                // Flush UTF-8 buffer (nếu có partial) rồi bắt đầu ESC.
                self.flush_utf8(emit);
                self.state = ParseState::EscapeStart;
            }
            b'\n' => {
                self.flush_utf8(emit);
                emit(AnsiEvent::Newline);
            }
            b'\r' => {
                self.flush_utf8(emit);
                emit(AnsiEvent::CarriageReturn);
            }
            0x08 => {
                self.flush_utf8(emit);
                emit(AnsiEvent::Backspace);
            }
            // SO/SI invoke G1/G0 charset (DEC VT behavior used by ncurses TUIs).
            0x0e => {
                self.flush_utf8(emit);
                self.invoked_charset = CharsetTarget::G1;
            }
            0x0f => {
                self.flush_utf8(emit);
                self.invoked_charset = CharsetTarget::G0;
            }
            // Control chars khác (BEL, TAB, NUL, …) — bỏ qua nhưng flush trước.
            0x00..=0x1f | 0x7f => {
                self.flush_utf8(emit);
                // TAB → phát ra spaces (đơn giản hóa: 1 tab = 8 spaces sẽ xử lý ở grid)
                if byte == b'\t' {
                    emit(AnsiEvent::PrintChar('\t'));
                }
            }
            // Byte UTF-8 bình thường: tích lũy vào buffer.
            _ => {
                if self.utf8_len < 4 {
                    self.utf8_buf[self.utf8_len] = byte;
                    self.utf8_len += 1;
                    // Nếu sequence hoàn chỉnh → emit.
                    if let Some(ch) = try_decode_utf8(&self.utf8_buf[..self.utf8_len]) {
                        emit(AnsiEvent::PrintChar(self.map_print_char(ch)));
                        self.utf8_len = 0;
                    }
                } else {
                    // Overflow: reset buffer, treat as replacement char.
                    self.utf8_len = 0;
                    emit(AnsiEvent::PrintChar('\u{FFFD}'));
                }
            }
        }
    }

    fn flush_utf8(&mut self, emit: &mut impl FnMut(AnsiEvent)) {
        if self.utf8_len > 0 {
            // Partial/incomplete UTF-8 — emit replacement char.
            emit(AnsiEvent::PrintChar('\u{FFFD}'));
            self.utf8_len = 0;
        }
    }

    fn handle_escape_start(&mut self, byte: u8, emit: &mut impl FnMut(AnsiEvent)) {
        match byte {
            b'[' => {
                self.state = ParseState::Csi {
                    buf: Vec::with_capacity(16),
                };
            }
            b']' => {
                self.state = ParseState::Osc;
            }
            b'(' => {
                self.state = ParseState::CharsetSelect(CharsetTarget::G0);
            }
            b')' => {
                self.state = ParseState::CharsetSelect(CharsetTarget::G1);
            }
            // ESC M = reverse index (scroll up), ESC= / ESC>, etc. — bỏ qua.
            _ => {
                self.state = ParseState::Normal;
                emit(AnsiEvent::Unknown);
            }
        }
    }

    fn handle_csi(&mut self, byte: u8, emit: &mut impl FnMut(AnsiEvent)) {
        // Param bytes: 0x30-0x3f, intermediate: 0x20-0x2f, final: 0x40-0x7e.
        match byte {
            0x30..=0x3f | 0x20..=0x2f => {
                // Accumulate param/intermediate bytes.
                if let ParseState::Csi { buf } = &mut self.state {
                    buf.push(byte);
                }
            }
            0x40..=0x7e => {
                // Final byte — parse và emit.
                let buf = if let ParseState::Csi { buf } = &self.state {
                    buf.clone()
                } else {
                    Vec::new()
                };
                let final_char = byte as char;
                self.state = ParseState::Normal;
                for event in parse_csi(&buf, final_char) {
                    emit(event);
                }
            }
            _ => {
                // Byte không hợp lệ — abandon CSI.
                self.state = ParseState::Normal;
                emit(AnsiEvent::Unknown);
            }
        }
    }

    fn handle_osc(&mut self, byte: u8) {
        // Bỏ qua cho đến khi gặp ST (ESC \) hoặc BEL (0x07).
        match byte {
            0x07 => {
                self.state = ParseState::Normal;
            }
            0x1b => {
                self.state = ParseState::OscEsc;
            }
            _ => { /* bỏ qua */ }
        }
    }

    fn handle_osc_esc(&mut self, byte: u8) {
        // OSC ST = ESC \. Consume both bytes so the trailing backslash is not rendered.
        if byte == b'\\' {
            self.state = ParseState::Normal;
        } else {
            // Not an ST terminator. Stay inside OSC and continue discarding the payload.
            self.state = ParseState::Osc;
        }
    }

    fn handle_charset_select(&mut self, byte: u8, target: CharsetTarget) {
        self.state = ParseState::Normal;

        let charset = match byte {
            b'0' => TerminalCharset::DecSpecialGraphics,
            b'B' => TerminalCharset::Ascii,
            _ => return,
        };

        match target {
            CharsetTarget::G0 => {
                self.g0_charset = charset;
            }
            CharsetTarget::G1 => {
                self.g1_charset = charset;
            }
        }
    }

    fn map_print_char(&self, ch: char) -> char {
        let charset = match self.invoked_charset {
            CharsetTarget::G0 => self.g0_charset,
            CharsetTarget::G1 => self.g1_charset,
        };

        match charset {
            TerminalCharset::Ascii => ch,
            TerminalCharset::DecSpecialGraphics => map_dec_special_graphics(ch),
        }
    }
}

impl Default for AnsiParser {
    fn default() -> Self {
        Self::new()
    }
}

// ─── CSI parser ─────────────────────────────────────────────────────────────

/// Parse một CSI sequence đã tích lũy đủ (buf = param bytes, final_char = lệnh).
fn parse_csi(buf: &[u8], final_char: char) -> Vec<AnsiEvent> {
    let param_str = std::str::from_utf8(buf)
        .unwrap_or("")
        .trim_start_matches('[');

    match final_char {
        // SGR: Select Graphic Rendition
        'm' => parse_sgr(param_str),

        // Cursor Up / Down / Forward / Back
        'A' => {
            let n = parse_first_param(param_str, 1) as i32;
            vec![AnsiEvent::CursorMove {
                row_delta: -n,
                col_delta: 0,
            }]
        }
        'B' => {
            let n = parse_first_param(param_str, 1) as i32;
            vec![AnsiEvent::CursorMove {
                row_delta: n,
                col_delta: 0,
            }]
        }
        'C' => {
            let n = parse_first_param(param_str, 1) as i32;
            vec![AnsiEvent::CursorMove {
                row_delta: 0,
                col_delta: n,
            }]
        }
        'D' => {
            let n = parse_first_param(param_str, 1) as i32;
            vec![AnsiEvent::CursorMove {
                row_delta: 0,
                col_delta: -n,
            }]
        }

        // Cursor Position (1-based → 0-based)
        'H' | 'f' => {
            let mut parts = param_str.split(';');
            let row = parts
                .next()
                .and_then(|s| s.parse::<usize>().ok())
                .unwrap_or(1)
                .saturating_sub(1);
            let col = parts
                .next()
                .and_then(|s| s.parse::<usize>().ok())
                .unwrap_or(1)
                .saturating_sub(1);
            vec![AnsiEvent::CursorGoto { row, col }]
        }

        // Erase Display
        'J' => vec![AnsiEvent::EraseDisplay(
            parse_first_param(param_str, 0) as u8
        )],

        // Erase Line
        'K' => vec![AnsiEvent::EraseLine(parse_first_param(param_str, 0) as u8)],

        // Private mode (ESC[?...h/l) — bỏ qua
        'h' | 'l' => vec![AnsiEvent::Unknown],

        _ => vec![AnsiEvent::Unknown],
    }
}

/// Parse SGR (ESC[...m) — có thể nhiều params phân cách bởi `;`.
fn parse_sgr(param_str: &str) -> Vec<AnsiEvent> {
    // Nếu rỗng hoặc "0" → ResetStyle
    if param_str.is_empty() || param_str == "0" {
        return vec![AnsiEvent::ResetStyle];
    }

    let params: Vec<u32> = param_str
        .split(';')
        .filter_map(|s| s.parse::<u32>().ok())
        .collect();

    let mut events = Vec::new();
    let mut i = 0;
    while i < params.len() {
        match params[i] {
            0 => events.push(AnsiEvent::ResetStyle),
            1 => events.push(AnsiEvent::SetBold(true)),
            22 => events.push(AnsiEvent::SetBold(false)),
            // Foreground standard (30-37) & bright (90-97)
            n @ 30..=37 => events.push(AnsiEvent::SetFg(AnsiColor::Index((n - 30) as u8))),
            38 => {
                // 256-color: 38;5;n hoặc truecolor: 38;2;r;g;b
                if i + 2 < params.len() && params[i + 1] == 5 {
                    events.push(AnsiEvent::SetFg(AnsiColor::Index(params[i + 2] as u8)));
                    i += 2;
                } else if i + 4 < params.len() && params[i + 1] == 2 {
                    events.push(AnsiEvent::SetFg(AnsiColor::Rgb(
                        params[i + 2] as u8,
                        params[i + 3] as u8,
                        params[i + 4] as u8,
                    )));
                    i += 4;
                }
            }
            39 => events.push(AnsiEvent::SetFg(AnsiColor::Default)),
            // Background standard (40-47) & bright (100-107)
            n @ 40..=47 => events.push(AnsiEvent::SetBg(AnsiColor::Index((n - 40) as u8))),
            48 => {
                if i + 2 < params.len() && params[i + 1] == 5 {
                    events.push(AnsiEvent::SetBg(AnsiColor::Index(params[i + 2] as u8)));
                    i += 2;
                } else if i + 4 < params.len() && params[i + 1] == 2 {
                    events.push(AnsiEvent::SetBg(AnsiColor::Rgb(
                        params[i + 2] as u8,
                        params[i + 3] as u8,
                        params[i + 4] as u8,
                    )));
                    i += 4;
                }
            }
            49 => events.push(AnsiEvent::SetBg(AnsiColor::Default)),
            // Bright foreground (90-97)
            n @ 90..=97 => events.push(AnsiEvent::SetFg(AnsiColor::Index((n - 90 + 8) as u8))),
            // Bright background (100-107)
            n @ 100..=107 => events.push(AnsiEvent::SetBg(AnsiColor::Index((n - 100 + 8) as u8))),
            _ => {}
        }
        i += 1;
    }

    if events.is_empty() {
        vec![AnsiEvent::Unknown]
    } else {
        events
    }
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

fn parse_first_param(param_str: &str, default: usize) -> usize {
    param_str
        .split(';')
        .next()
        .and_then(|s| s.parse::<usize>().ok())
        .unwrap_or(default)
}

fn map_dec_special_graphics(ch: char) -> char {
    match ch {
        '_' => ' ',
        '`' => '◆',
        'a' => '▒',
        'b' => '\t',
        'c' => '␌',
        'd' => '␍',
        'e' => '␊',
        'f' => '°',
        'g' => '±',
        'h' => '␤',
        'i' => '␋',
        'j' => '┘',
        'k' => '┐',
        'l' => '┌',
        'm' => '└',
        'n' => '┼',
        'o' => '⎺',
        'p' => '⎻',
        'q' => '─',
        'r' => '⎼',
        's' => '⎽',
        't' => '├',
        'u' => '┤',
        'v' => '┴',
        'w' => '┬',
        'x' => '│',
        'y' => '≤',
        'z' => '≥',
        '{' => 'π',
        '|' => '≠',
        '}' => '£',
        '~' => '·',
        _ => ch,
    }
}

/// Thử decode một UTF-8 sequence không hoàn chỉnh.
/// Trả `Some(char)` nếu đủ bytes, `None` nếu cần thêm.
fn try_decode_utf8(bytes: &[u8]) -> Option<char> {
    match std::str::from_utf8(bytes) {
        Ok(s) => s.chars().next(),
        Err(_) => {
            // Có thể là partial — kiểm tra bằng cách thử decode từng prefix.
            // Nếu byte đầu tiên cho biết sequence dài hơn số bytes hiện có → None.
            let first = bytes[0];
            let expected_len = if first & 0x80 == 0 {
                1
            } else if first & 0xe0 == 0xc0 {
                2
            } else if first & 0xf0 == 0xe0 {
                3
            } else if first & 0xf8 == 0xf0 {
                4
            } else {
                return Some('\u{FFFD}'); // invalid
            };
            if bytes.len() >= expected_len {
                // Đủ bytes nhưng vẫn lỗi → replacement char.
                Some('\u{FFFD}')
            } else {
                None // chờ thêm bytes
            }
        }
    }
}

// ─── xterm-256 color table ───────────────────────────────────────────────────

/// Chuyển xterm-256 index → (r, g, b).
/// Dựa trên spec: 0-7 standard, 8-15 bright, 16-231 color cube, 232-255 grayscale.
pub fn xterm256_to_rgb(idx: u8) -> (u8, u8, u8) {
    match idx {
        // Standard colors (approximate terminal defaults)
        0 => (0, 0, 0),
        1 => (128, 0, 0),
        2 => (0, 128, 0),
        3 => (128, 128, 0),
        4 => (0, 0, 128),
        5 => (128, 0, 128),
        6 => (0, 128, 128),
        7 => (192, 192, 192),
        // Bright colors
        8 => (128, 128, 128),
        9 => (255, 0, 0),
        10 => (0, 255, 0),
        11 => (255, 255, 0),
        12 => (0, 0, 255),
        13 => (255, 0, 255),
        14 => (0, 255, 255),
        15 => (255, 255, 255),
        // 6×6×6 color cube (indices 16-231)
        16..=231 => {
            let n = idx - 16;
            let b = n % 6;
            let g = (n / 6) % 6;
            let r = n / 36;
            let to_byte = |v: u8| if v == 0 { 0 } else { 55 + v * 40 };
            (to_byte(r), to_byte(g), to_byte(b))
        }
        // Grayscale ramp (indices 232-255)
        232..=255 => {
            let level = 8 + (idx - 232) * 10;
            (level, level, level)
        }
    }
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn collect_events(input: &str) -> Vec<AnsiEvent> {
        let mut parser = AnsiParser::new();
        let mut events = Vec::new();
        parser.feed_str(input, &mut |ev| events.push(ev));
        events
    }

    #[test]
    fn plain_text_emits_print_chars() {
        let events = collect_events("hi");
        assert_eq!(
            events,
            vec![AnsiEvent::PrintChar('h'), AnsiEvent::PrintChar('i')]
        );
    }

    #[test]
    fn newline_and_cr() {
        let events = collect_events("a\r\nb");
        assert_eq!(
            events,
            vec![
                AnsiEvent::PrintChar('a'),
                AnsiEvent::CarriageReturn,
                AnsiEvent::Newline,
                AnsiEvent::PrintChar('b'),
            ]
        );
    }

    #[test]
    fn sgr_reset() {
        let events = collect_events("\x1b[0m");
        assert_eq!(events, vec![AnsiEvent::ResetStyle]);
    }

    #[test]
    fn sgr_empty_is_reset() {
        let events = collect_events("\x1b[m");
        assert_eq!(events, vec![AnsiEvent::ResetStyle]);
    }

    #[test]
    fn sgr_fg_standard_green() {
        let events = collect_events("\x1b[32m");
        assert_eq!(events, vec![AnsiEvent::SetFg(AnsiColor::Index(2))]);
    }

    #[test]
    fn sgr_fg_bright_red() {
        let events = collect_events("\x1b[91m");
        assert_eq!(events, vec![AnsiEvent::SetFg(AnsiColor::Index(9))]);
    }

    #[test]
    fn sgr_bg_blue() {
        let events = collect_events("\x1b[44m");
        assert_eq!(events, vec![AnsiEvent::SetBg(AnsiColor::Index(4))]);
    }

    #[test]
    fn sgr_256_color_fg() {
        let events = collect_events("\x1b[38;5;200m");
        assert_eq!(events, vec![AnsiEvent::SetFg(AnsiColor::Index(200))]);
    }

    #[test]
    fn sgr_truecolor_fg() {
        let events = collect_events("\x1b[38;2;255;128;0m");
        assert_eq!(events, vec![AnsiEvent::SetFg(AnsiColor::Rgb(255, 128, 0))]);
    }

    #[test]
    fn sgr_multiple_params_emit_all_styles() {
        let events = collect_events("\x1b[0;1;38;5;196;48;2;1;2;3m");
        assert_eq!(
            events,
            vec![
                AnsiEvent::ResetStyle,
                AnsiEvent::SetBold(true),
                AnsiEvent::SetFg(AnsiColor::Index(196)),
                AnsiEvent::SetBg(AnsiColor::Rgb(1, 2, 3)),
            ]
        );
    }

    #[test]
    fn cursor_move_up() {
        let events = collect_events("\x1b[3A");
        assert_eq!(
            events,
            vec![AnsiEvent::CursorMove {
                row_delta: -3,
                col_delta: 0
            }]
        );
    }

    #[test]
    fn cursor_goto() {
        let events = collect_events("\x1b[5;10H");
        assert_eq!(events, vec![AnsiEvent::CursorGoto { row: 4, col: 9 }]);
    }

    #[test]
    fn colored_text_sequence() {
        // printf '\033[32mGREEN\033[0m'
        let s = "\x1b[32mGREEN\x1b[0m";
        let events = collect_events(s);
        assert_eq!(events[0], AnsiEvent::SetFg(AnsiColor::Index(2)));
        assert_eq!(events[1], AnsiEvent::PrintChar('G'));
        assert_eq!(events[5], AnsiEvent::PrintChar('N'));
        assert_eq!(events[6], AnsiEvent::ResetStyle);
    }

    #[test]
    fn osc_sequence_ignored() {
        // ESC]0;window titleBEL
        let events = collect_events("\x1b]0;window title\x07hello");
        // OSC bị bỏ qua, chỉ emit "hello"
        assert!(events.iter().any(|e| *e == AnsiEvent::PrintChar('h')));
        assert!(!events.iter().any(|e| *e == AnsiEvent::PrintChar('0')));
    }

    #[test]
    fn osc_st_terminator_consumes_backslash() {
        let events = collect_events("\x1b]8;;https://example.com\x1b\\link");
        assert_eq!(
            events,
            vec![
                AnsiEvent::PrintChar('l'),
                AnsiEvent::PrintChar('i'),
                AnsiEvent::PrintChar('n'),
                AnsiEvent::PrintChar('k'),
            ]
        );
    }

    #[test]
    fn dec_special_graphics_maps_line_drawing_chars() {
        let events = collect_events("\x1b(0lqxkmjtnuuvw\x1b(B");
        assert_eq!(
            events,
            vec![
                AnsiEvent::PrintChar('┌'),
                AnsiEvent::PrintChar('─'),
                AnsiEvent::PrintChar('│'),
                AnsiEvent::PrintChar('┐'),
                AnsiEvent::PrintChar('└'),
                AnsiEvent::PrintChar('┘'),
                AnsiEvent::PrintChar('├'),
                AnsiEvent::PrintChar('┼'),
                AnsiEvent::PrintChar('┤'),
                AnsiEvent::PrintChar('┤'),
                AnsiEvent::PrintChar('┴'),
                AnsiEvent::PrintChar('┬'),
            ]
        );
    }

    #[test]
    fn dec_special_graphics_resets_back_to_ascii() {
        let events = collect_events("\x1b(0q\x1b(Bq");
        assert_eq!(
            events,
            vec![AnsiEvent::PrintChar('─'), AnsiEvent::PrintChar('q')]
        );
    }

    #[test]
    fn dec_special_graphics_supports_g1_invocation_with_so_si() {
        let events = collect_events("\x1b)0q\x0eqx\x0fq");
        assert_eq!(
            events,
            vec![
                AnsiEvent::PrintChar('q'),
                AnsiEvent::PrintChar('─'),
                AnsiEvent::PrintChar('│'),
                AnsiEvent::PrintChar('q'),
            ]
        );
    }

    #[test]
    fn xterm256_basic_colors() {
        assert_eq!(xterm256_to_rgb(0), (0, 0, 0));
        assert_eq!(xterm256_to_rgb(15), (255, 255, 255));
        // Grayscale: index 232 → level 8
        assert_eq!(xterm256_to_rgb(232), (8, 8, 8));
    }

    #[test]
    fn ansi_colors_are_linear_for_srgb_render_target() {
        let color = AnsiColor::Rgb(128, 64, 32).to_rgba_f32_with_defaults(
            [1.0, 1.0, 1.0, 1.0],
            [0.0, 0.0, 0.0, 1.0],
            true,
        );

        assert!((color[0] - 0.215_860_53).abs() < 0.000_01);
        assert!((color[1] - 0.051_269_46).abs() < 0.000_01);
        assert!((color[2] - 0.014_443_84).abs() < 0.000_01);
        assert_eq!(color[3], 1.0);
    }
}
