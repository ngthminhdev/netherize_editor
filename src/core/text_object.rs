use crate::core::commands::{TextObjectKind, TextObjectModifier};
use ropey::Rope;

pub fn find_text_object_range(
    text: &Rope,
    cursor_char_idx: usize,
    modifier: TextObjectModifier,
    kind: TextObjectKind,
) -> Option<(usize, usize)> {
    if text.len_chars() == 0 {
        return None;
    }

    match kind {
        TextObjectKind::Quote(q) => find_quote_text_object(text, cursor_char_idx, modifier, q),
        TextObjectKind::Bracket(open_char, close_char) => {
            find_bracket_text_object(text, cursor_char_idx, modifier, open_char, close_char)
        }
    }
}

fn find_quote_text_object(
    text: &Rope,
    cursor: usize,
    modifier: TextObjectModifier,
    quote: char,
) -> Option<(usize, usize)> {
    let cursor = cursor.min(text.len_chars().saturating_sub(1));
    let line_idx = text.char_to_line(cursor);
    let line_start = text.line_to_char(line_idx);
    let line_end = if line_idx + 1 < text.len_lines() {
        text.line_to_char(line_idx + 1)
    } else {
        text.len_chars()
    };

    let mut quotes = Vec::new();
    for i in line_start..line_end {
        if text.char(i) == quote {
            let mut is_escaped = false;
            let mut backslash_count = 0;
            let mut j = i.saturating_sub(1);
            while j >= line_start && text.char(j) == '\\' {
                backslash_count += 1;
                if j == 0 {
                    break;
                }
                j -= 1;
            }
            if backslash_count % 2 == 1 {
                is_escaped = true;
            }
            if !is_escaped {
                quotes.push(i);
            }
        }
    }

    if quotes.len() < 2 {
        return None;
    }

    let mut open_pos = None;
    let mut close_pos = None;

    for chunk in quotes.chunks_exact(2) {
        let (start, end) = (chunk[0], chunk[1]);
        if cursor >= start && cursor <= end {
            open_pos = Some(start);
            close_pos = Some(end);
            break;
        }
    }

    if open_pos.is_none() {
        for chunk in quotes.chunks_exact(2) {
            let (start, end) = (chunk[0], chunk[1]);
            if start > cursor {
                open_pos = Some(start);
                close_pos = Some(end);
                break;
            }
        }
    }

    let (open, close) = (open_pos?, close_pos?);

    match modifier {
        TextObjectModifier::Inner => {
            if open + 1 > close {
                Some((open + 1, open + 1))
            } else {
                Some((open + 1, close))
            }
        }
        TextObjectModifier::Around => Some((open, close + 1)),
    }
}

fn find_bracket_text_object(
    text: &Rope,
    cursor: usize,
    modifier: TextObjectModifier,
    open_char: char,
    close_char: char,
) -> Option<(usize, usize)> {
    let len = text.len_chars();
    let cursor = cursor.min(len.saturating_sub(1));

    let mut depth = 0;
    let mut open_pos = None;

    for i in (0..=cursor).rev() {
        let ch = text.char(i);
        if ch == close_char {
            depth += 1;
        } else if ch == open_char {
            if depth == 0 {
                open_pos = Some(i);
                break;
            }
            depth -= 1;
        }
    }

    if open_pos.is_none() {
        let line_idx = text.char_to_line(cursor);
        let line_end = if line_idx + 1 < text.len_lines() {
            text.line_to_char(line_idx + 1)
        } else {
            len
        };

        for i in cursor..line_end {
            if text.char(i) == open_char {
                open_pos = Some(i);
                break;
            }
        }
    }

    let open_pos = open_pos?;

    depth = 0;
    let mut close_pos = None;
    for i in (open_pos + 1)..len {
        let ch = text.char(i);
        if ch == open_char {
            depth += 1;
        } else if ch == close_char {
            if depth == 0 {
                close_pos = Some(i);
                break;
            }
            depth -= 1;
        }
    }

    let close_pos = close_pos?;

    match modifier {
        TextObjectModifier::Inner => {
            if open_pos + 1 > close_pos {
                Some((open_pos + 1, open_pos + 1))
            } else {
                Some((open_pos + 1, close_pos))
            }
        }
        TextObjectModifier::Around => Some((open_pos, close_pos + 1)),
    }
}
