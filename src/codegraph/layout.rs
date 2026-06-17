//! Column layout for the Code Graph HUD: center pill + two side columns,
//! capped at [`MAX_PER_COLUMN`] visible pills, with a scrolling window that
//! keeps the focused index visible.

pub const MAX_PER_COLUMN: usize = 8;

/// A laid-out pill rect: `[x, y, w, h]` in HUD-content coordinates.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PillRect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct GraphLayout {
    pub center: PillRect,
    pub callers: Vec<PillRect>, // one per VISIBLE caller slot
    pub callees: Vec<PillRect>,
    pub caller_window_start: usize,
    pub callee_window_start: usize,
    pub caller_overflow: usize, // hidden count below the window
    pub callee_overflow: usize,
}

/// Compute the window `[start, end)` that keeps `focused` visible.
pub fn visible_window(total: usize, focused: usize, cap: usize) -> (usize, usize) {
    if total <= cap {
        return (0, total);
    }
    let start = focused.saturating_sub(cap - 1).min(total - cap);
    let start = start.min(focused);
    (start, (start + cap).min(total))
}

/// `content`: `[x, y, w, h]` of the HUD graph area (below top bar, above footer).
/// `scale` is the HiDPI / ui scale factor applied to absolute pixel sizes so the
/// HUD tracks the rest of the chrome on retina displays.
pub fn layout(
    content: [f32; 4],
    n_callers: usize,
    n_callees: usize,
    caller_focus: Option<usize>,
    callee_focus: Option<usize>,
    scale: f32,
) -> GraphLayout {
    let [cx, cy, cw, ch] = content;
    let s = scale.max(0.1);
    let pill_w = (cw * 0.26).clamp(150.0 * s, 260.0 * s);
    let pill_h = 60.0 * s;
    let center_w = (cw * 0.30).clamp(190.0 * s, 300.0 * s);
    let center_h = 78.0 * s;

    let center = PillRect {
        x: cx + (cw - center_w) * 0.5,
        y: cy + (ch - center_h) * 0.5,
        w: center_w,
        h: center_h,
    };

    let column = |n: usize, focus: usize, left_x: f32| -> (Vec<PillRect>, usize, usize) {
        let (start, end) = visible_window(n, focus, MAX_PER_COLUMN);
        let visible = end - start;
        let gap = 14.0 * s;
        let total_h = visible as f32 * pill_h + (visible.saturating_sub(1)) as f32 * gap;
        let top = cy + (ch - total_h) * 0.5;
        let rects = (0..visible)
            .map(|i| PillRect {
                x: left_x,
                y: top + i as f32 * (pill_h + gap),
                w: pill_w,
                h: pill_h,
            })
            .collect();
        (rects, start, n.saturating_sub(end))
    };

    let (callers, caller_window_start, caller_overflow) =
        column(n_callers, caller_focus.unwrap_or(0), cx + 12.0 * s);
    let (callees, callee_window_start, callee_overflow) =
        column(n_callees, callee_focus.unwrap_or(0), cx + cw - 12.0 * s - pill_w);

    GraphLayout {
        center,
        callers,
        callees,
        caller_window_start,
        callee_window_start,
        caller_overflow,
        callee_overflow,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn window_shows_all_when_under_cap() {
        assert_eq!(visible_window(5, 0, 8), (0, 5));
    }

    #[test]
    fn window_scrolls_to_keep_focus_visible() {
        // 20 items, focus at 19, cap 8 -> window [12, 20)
        assert_eq!(visible_window(20, 19, 8), (12, 20));
    }

    #[test]
    fn layout_caps_visible_and_reports_overflow() {
        let l = layout([0.0, 0.0, 800.0, 400.0], 20, 0, Some(0), None, 1.0);
        assert_eq!(l.callers.len(), MAX_PER_COLUMN);
        assert_eq!(l.caller_overflow, 12);
    }

    #[test]
    fn center_is_horizontally_centered() {
        let l = layout([0.0, 0.0, 800.0, 400.0], 0, 0, None, None, 1.0);
        let mid = l.center.x + l.center.w * 0.5;
        assert!((mid - 400.0).abs() < 0.5);
    }
}
