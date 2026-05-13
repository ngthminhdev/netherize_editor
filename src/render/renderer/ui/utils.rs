use crate::render::region_pipeline::RegionDrawInstance;
use crate::text::text_system::StyledTextSpan;
use std::ops::Range;

pub(crate) fn with_alpha(mut color: [f32; 4], alpha: f32) -> [f32; 4] {
    color[3] = alpha.clamp(0.0, 1.0);
    color
}

pub(crate) fn blend_rgb(base: [f32; 4], tint: [f32; 4], amount: f32, alpha: f32) -> [f32; 4] {
    let t = amount.clamp(0.0, 1.0);
    [
        base[0] * (1.0 - t) + tint[0] * t,
        base[1] * (1.0 - t) + tint[1] * t,
        base[2] * (1.0 - t) + tint[2] * t,
        alpha.clamp(0.0, 1.0),
    ]
}

pub(crate) fn slash_suggestion_rect(
    input_bounds: [f32; 4],
    history_clip: [f32; 4],
    line_h: f32,
    suggestion_count: usize,
) -> [f32; 4] {
    let desired_h = suggestion_count as f32 * line_h + 12.0;
    let scissor_bottom = history_clip[1] + history_clip[3];
    let bottom_gap = (input_bounds[1] - scissor_bottom).max(8.0);
    let available_above_input = (scissor_bottom - history_clip[1] - 8.0).max(line_h);
    let h = desired_h.min(available_above_input);
    [
        input_bounds[0] + 2.0,
        (input_bounds[1] - h - bottom_gap).max(history_clip[1]),
        (input_bounds[2] - 4.0).max(1.0),
        h,
    ]
}

/// Word-wrap a string into lines of at most `max_chars` characters,
/// breaking at whitespace where possible and at glyph boundaries for long tokens.
pub(crate) fn word_wrap(text: &str, max_chars: usize) -> Vec<String> {
    word_wrap_with_ranges(text, max_chars)
        .into_iter()
        .map(|(line, _)| line)
        .collect()
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

#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn right_sidebar_background_quads(
    bounds: [f32; 4],
    input_bounds: Option<[f32; 4]>,
    panel_border_width: f32,
    border_color: [f32; 4],
    panel_bg: [f32; 4],
    input_bg: [f32; 4],
    border_radius: f32,
) -> Vec<RegionDrawInstance> {
    let [x, y, w, h] = bounds;
    if w <= 0.0 || h <= 0.0 {
        return Vec::new();
    }

    let t = panel_border_width.max(0.0).min(w * 0.5).min(h * 0.5);
    let mut quads = Vec::with_capacity(4);

    // Step 1: Outline border — full bounds.
    quads.push(RegionDrawInstance::new(bounds, border_color).with_radius(border_radius));

    // Step 2: Inner background — inset by panel_border_width.
    let inner_w = (w - t * 2.0).max(0.0);
    let inner_h = (h - t * 2.0).max(0.0);
    if inner_w > 0.0 && inner_h > 0.0 {
        quads.push(
            RegionDrawInstance::new([x + t, y + t, inner_w, inner_h], panel_bg)
                .with_radius((border_radius - t).max(0.0)),
        );
    }

    // Step 3: Input box background — slightly different shade.
    if let Some([ix, iy, iw, ih]) = input_bounds {
        if iw > 0.0 && ih > 0.0 {
            quads.push(
                RegionDrawInstance::new([ix, iy, iw, ih], input_bg)
                    .with_radius((border_radius - t).max(0.0)),
            );
        }
    }

    quads
}

#[cfg(test)]
mod tests {
    use super::*;

    const BORDER: [f32; 4] = [0.3, 0.3, 0.3, 1.0];
    const PANEL: [f32; 4] = [0.1, 0.1, 0.1, 1.0];
    const INPUT: [f32; 4] = [0.08, 0.08, 0.08, 1.0];

    #[test]
    fn empty_bounds_returns_no_quads() {
        let quads = right_sidebar_background_quads(
            [0.0, 0.0, 0.0, 0.0],
            None,
            1.0,
            BORDER,
            PANEL,
            INPUT,
            8.0,
        );
        assert!(quads.is_empty());
    }

    #[test]
    fn negative_dimensions_return_no_quads() {
        let quads = right_sidebar_background_quads(
            [10.0, 10.0, -5.0, 100.0],
            None,
            1.0,
            BORDER,
            PANEL,
            INPUT,
            8.0,
        );
        assert!(quads.is_empty());
    }

    #[test]
    fn produces_border_and_fill_without_input() {
        let quads = right_sidebar_background_quads(
            [10.0, 20.0, 300.0, 400.0],
            None,
            1.0,
            BORDER,
            PANEL,
            INPUT,
            8.0,
        );
        assert_eq!(quads.len(), 2);
        // First quad is the border (full bounds).
        assert_eq!(quads[0].rect, [10.0, 20.0, 300.0, 400.0]);
        assert_eq!(quads[0].color, BORDER);
        // Second quad is the inner fill (inset by 1px).
        assert_eq!(quads[1].rect, [11.0, 21.0, 298.0, 398.0]);
        assert_eq!(quads[1].color, PANEL);
    }

    #[test]
    fn produces_three_quads_with_input_bounds() {
        let quads = right_sidebar_background_quads(
            [10.0, 20.0, 300.0, 400.0],
            Some([14.0, 320.0, 292.0, 80.0]),
            1.0,
            BORDER,
            PANEL,
            INPUT,
            8.0,
        );
        assert_eq!(quads.len(), 3);
        // Third quad is the input box.
        assert_eq!(quads[2].rect, [14.0, 320.0, 292.0, 80.0]);
        assert_eq!(quads[2].color, INPUT);
    }

    #[test]
    fn zero_size_input_bounds_are_skipped() {
        let quads = right_sidebar_background_quads(
            [10.0, 20.0, 300.0, 400.0],
            Some([14.0, 320.0, 0.0, 0.0]),
            1.0,
            BORDER,
            PANEL,
            INPUT,
            8.0,
        );
        // Only border + fill; input is skipped.
        assert_eq!(quads.len(), 2);
    }

    #[test]
    fn border_radius_is_reduced_for_inner_quads() {
        let quads = right_sidebar_background_quads(
            [0.0, 0.0, 200.0, 200.0],
            Some([12.0, 150.0, 176.0, 40.0]),
            2.0,
            BORDER,
            PANEL,
            INPUT,
            12.0,
        );
        assert_eq!(quads.len(), 3);
        assert_eq!(quads[0].border_radius, 12.0);
        assert_eq!(quads[1].border_radius, 10.0);
        assert_eq!(quads[2].border_radius, 10.0);
    }
}
