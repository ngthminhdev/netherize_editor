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

/// Minimum comfortable width for one dock tab, in logical px. Below this a
/// tab label plus icon gets cramped and the click target too small.
pub(crate) const DOCK_TAB_MIN_WIDTH: f32 = 56.0;

/// Alpha of the hover wash drawn behind a non-active hovered tab in the left
/// / right dock strips. Matches `TOPBAR_TAB_HOVER_ALPHA` so every tab strip
/// reacts to the pointer with the same intensity.
pub(crate) const DOCK_TAB_HOVER_ALPHA: f32 = 0.08;

/// Effective width of one tab in a dock strip `strip_w` logical px wide split
/// `tab_count` ways.
///
/// This is the single source of truth for dock tab geometry: BOTH the strip
/// renderer (`build_left_tab_strip` / `build_right_tab_strip`) and the
/// hit-test (`left_dock_tab_index_at` / `right_dock_tab_index_at`) must go
/// through it, or rendered tabs and clickable tabs drift apart.
///
/// Tabs divide the strip equally, clamped up to [`DOCK_TAB_MIN_WIDTH`] when
/// that minimum still fits. Tradeoff: when there are more tabs than fit at
/// the minimum (`strip_w / n < 56`), equal division wins — tabs shrink below
/// 56px instead of overflowing the strip's right edge. Under today's strict
/// equal division the clamp therefore only engages if a future layout passes
/// a divisor other than `tab_count`; keeping it here means such layouts
/// inherit the minimum automatically.
pub(crate) fn dock_tab_width(strip_w: f32, tab_count: usize) -> Option<f32> {
    if tab_count == 0 || strip_w <= 0.0 {
        return None;
    }
    let equal = strip_w / tab_count as f32;
    let desired = equal.max(DOCK_TAB_MIN_WIDTH);
    if desired * tab_count as f32 <= strip_w + f32::EPSILON {
        Some(desired)
    } else {
        // The minimum would push the last tab past the strip's right edge;
        // narrower-than-56px tabs beat overflow.
        Some(equal)
    }
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

#[cfg(test)]
mod tests {
    use super::{DOCK_TAB_MIN_WIDTH, dock_tab_width};

    #[test]
    fn dock_tab_width_rejects_degenerate_inputs() {
        assert_eq!(dock_tab_width(200.0, 0), None, "no tabs → no geometry");
        assert_eq!(dock_tab_width(0.0, 3), None, "empty strip → no geometry");
        assert_eq!(dock_tab_width(-10.0, 2), None, "negative strip → no geometry");
    }

    #[test]
    fn dock_tab_width_uses_equal_division_when_above_minimum() {
        // Comfortably above the minimum: plain equal division.
        assert_eq!(dock_tab_width(300.0, 3), Some(100.0));
        assert_eq!(dock_tab_width(240.0, 2), Some(120.0));
    }

    #[test]
    fn dock_tab_width_holds_exact_minimum_boundary() {
        // 112 / 2 = 56 exactly: sits on the minimum and must stay there.
        let w = dock_tab_width(DOCK_TAB_MIN_WIDTH * 2.0, 2).expect("two tabs always fit");
        assert!((w - DOCK_TAB_MIN_WIDTH).abs() < f32::EPSILON);
    }

    #[test]
    fn dock_tab_width_falls_back_to_equal_division_below_minimum() {
        // More tabs than fit at 56px: equal division wins so the last tab
        // never overflows the strip's right edge (documented tradeoff).
        let w = dock_tab_width(100.0, 3).expect("nonzero strip has geometry");
        assert!(w < DOCK_TAB_MIN_WIDTH);
        assert!((w - 100.0 / 3.0).abs() < 1e-4);
        // Rendered width × count never exceeds the strip.
        assert!(w * 3.0 <= 100.0 + f32::EPSILON);
    }
}
