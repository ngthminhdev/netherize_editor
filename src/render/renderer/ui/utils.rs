use crate::text::text_system::StyledTextSpan;
use std::ops::Range;

pub(crate) fn blend_rgb(base: [f32; 4], tint: [f32; 4], amount: f32, alpha: f32) -> [f32; 4] {
    let t = amount.clamp(0.0, 1.0);
    [
        base[0] * (1.0 - t) + tint[0] * t,
        base[1] * (1.0 - t) + tint[1] * t,
        base[2] * (1.0 - t) + tint[2] * t,
        alpha.clamp(0.0, 1.0),
    ]
}

pub(crate) fn word_wrap_with_ranges(text: &str, max_chars: usize) -> Vec<(String, Range<usize>)> {
    if max_chars == 0 || text.is_empty() {
        return vec![(text.to_string(), 0..text.len())];
    }

    let mut lines = Vec::new();
    let mut line_start = 0usize;

    while line_start < text.len() {
        debug_assert!(text.is_char_boundary(line_start));

        let remaining = &text[line_start..];
        let mut char_count = 0usize;
        let mut hard_end = text.len();
        let mut last_break: Option<usize> = None;

        for (offset, ch) in remaining.char_indices() {
            let idx = line_start + offset;
            if ch.is_whitespace() {
                last_break = Some(idx + ch.len_utf8());
            }

            char_count += 1;
            if char_count > max_chars {
                hard_end = idx;
                break;
            }
        }

        if char_count <= max_chars {
            lines.push((remaining.to_string(), line_start..text.len()));
            break;
        }

        let break_end = last_break
            .filter(|break_idx| {
                *break_idx > line_start
                    && *break_idx <= hard_end
                    && text.is_char_boundary(*break_idx)
            })
            .unwrap_or(hard_end);
        let trimmed_end = text[line_start..break_end]
            .trim_end()
            .len()
            .saturating_add(line_start);

        if trimmed_end > line_start && text.is_char_boundary(trimmed_end) {
            lines.push((
                text[line_start..trimmed_end].to_string(),
                line_start..trimmed_end,
            ));
        } else if break_end > line_start && text.is_char_boundary(break_end) {
            lines.push((
                text[line_start..break_end].to_string(),
                line_start..break_end,
            ));
        }

        line_start = break_end;
        while line_start < text.len() {
            let Some(next) = text[line_start..].chars().next() else {
                break;
            };
            if !next.is_whitespace() {
                break;
            }
            line_start += next.len_utf8();
        }
    }

    if lines.is_empty() {
        lines.push((String::new(), 0..0));
    }
    lines
}

pub(crate) fn clip_styled_span_to_range(
    span: StyledTextSpan,
    range: &Range<usize>,
) -> Option<StyledTextSpan> {
    let start = span.start.max(range.start);
    let end = span.end.min(range.end);
    if start >= end {
        return None;
    }
    Some(StyledTextSpan::with_style(
        start.saturating_sub(range.start),
        end.saturating_sub(range.start),
        span.color_rgba,
        span.bold,
        span.italic,
    ))
}
