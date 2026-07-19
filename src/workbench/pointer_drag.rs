//! Pure pointer-drag types and panel/splitter geometry. No GPU/winit.

use crate::canvas::interaction::CardZone;
use crate::canvas::model::BlockId;

/// Screen-pixel thickness of a dock's draggable inner-edge band.
pub const SPLITTER_BAND_PX: f32 = 6.0;
/// Minimum dock size in pixels.
pub const MIN_PANEL_PX: f32 = 120.0;
/// A dock may not exceed this fraction of the relevant viewport dimension.
pub const MAX_PANEL_FRACTION: f32 = 0.6;
/// Pixels the cursor must travel after press before a press becomes a drag.
pub const DRAG_DEADZONE_PX: f32 = 3.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PanelSide {
    Left,
    Right,
    Bottom,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DragTarget {
    PanelEdge(PanelSide),
    CardMove(BlockId),
    CardResize(BlockId, CardZone),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HoverTarget {
    PanelEdge(PanelSide),
    CardBody,
    CardResize(CardZone),
}

/// What was being dragged, captured at press time.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DragAnchor {
    Panel {
        start_size: f32,
    },
    CardMove {
        start_world: (f32, f32),
    },
    /// Card resize keeps the top-left fixed, so it recomputes from the live
    /// camera each move and needs no captured anchor.
    CardResize,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ActiveDrag {
    pub target: DragTarget,
    pub start_cursor: (f32, f32),
    pub anchor: DragAnchor,
    /// Card-move drags stay inert until the cursor leaves a small dead-zone
    /// around the press point, so a click-to-focus never nudges the card.
    /// `true` once the threshold is crossed (latched for the rest of the drag).
    /// Always treated as armed for panel/resize drags.
    pub armed: bool,
}

/// True once the cursor has travelled more than `threshold` pixels (Euclidean)
/// from the press point — the dead-zone gate for card-move drags.
pub fn past_deadzone(start: (f32, f32), cur: (f32, f32), threshold: f32) -> bool {
    let dx = cur.0 - start.0;
    let dy = cur.1 - start.1;
    dx * dx + dy * dy > threshold * threshold
}

/// Which dock's inner edge (if any) the cursor is within `band` pixels of.
/// Bounds are `[x, y, w, h]`. Priority: Left, Right, Bottom.
pub fn splitter_hit_test(
    left: Option<[f32; 4]>,
    right: Option<[f32; 4]>,
    bottom: Option<[f32; 4]>,
    band: f32,
    cursor: (f32, f32),
) -> Option<PanelSide> {
    let (cx, cy) = cursor;
    if let Some([x, y, w, h]) = left {
        let edge = x + w; // inner (right) edge
        if (cx - edge).abs() <= band && cy >= y && cy <= y + h {
            return Some(PanelSide::Left);
        }
    }
    if let Some([x, y, _w, h]) = right {
        let edge = x; // inner (left) edge
        if (cx - edge).abs() <= band && cy >= y && cy <= y + h {
            return Some(PanelSide::Right);
        }
    }
    if let Some([x, y, w, _h]) = bottom {
        let edge = y; // inner (top) edge
        if (cy - edge).abs() <= band && cx >= x && cx <= x + w {
            return Some(PanelSide::Bottom);
        }
    }
    None
}

/// Clamp a candidate dock size to `[MIN_PANEL_PX, MAX_PANEL_FRACTION * viewport]`.
pub fn clamp_panel_size(side: PanelSide, raw: f32, viewport: (f32, f32)) -> f32 {
    let extent = match side {
        PanelSide::Left | PanelSide::Right => viewport.0,
        PanelSide::Bottom => viewport.1,
    };
    let max = (extent * MAX_PANEL_FRACTION).max(MIN_PANEL_PX);
    raw.clamp(MIN_PANEL_PX, max)
}

/// New dock size for a pointer delta `(dx, dy)` from the press point.
pub fn apply_panel_drag(
    side: PanelSide,
    start_size: f32,
    dx: f32,
    dy: f32,
    viewport: (f32, f32),
) -> f32 {
    let raw = match side {
        PanelSide::Left => start_size + dx,
        PanelSide::Right => start_size - dx,
        PanelSide::Bottom => start_size - dy,
    };
    clamp_panel_size(side, raw, viewport)
}

/// Press hit-test priority: a canvas card (only while cards are interactive —
/// navigating OR floating in the background) beats a splitter; otherwise the
/// splitter; otherwise nothing.
pub fn resolve_press_target(
    cards_interactive: bool,
    card_hit: Option<(BlockId, CardZone)>,
    splitter_hit: Option<PanelSide>,
) -> Option<DragTarget> {
    if cards_interactive {
        if let Some((id, zone)) = card_hit {
            return Some(match zone {
                CardZone::Body => DragTarget::CardMove(id),
                other => DragTarget::CardResize(id, other),
            });
        }
    }
    splitter_hit.map(DragTarget::PanelEdge)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::canvas::interaction::CardZone;

    #[test]
    fn splitter_left_edge_hit() {
        // Left dock occupies x∈[0,240], full height.
        let left = Some([0.0, 0.0, 240.0, 1000.0]);
        // Inner edge at x=240; cursor near it within band.
        assert_eq!(
            splitter_hit_test(left, None, None, 6.0, (242.0, 500.0)),
            Some(PanelSide::Left)
        );
        // Outside the band → miss.
        assert_eq!(
            splitter_hit_test(left, None, None, 6.0, (200.0, 500.0)),
            None
        );
        // Outside the dock's vertical span → miss.
        assert_eq!(
            splitter_hit_test(left, None, None, 6.0, (240.0, 1500.0)),
            None
        );
    }

    #[test]
    fn splitter_right_and_bottom_edges() {
        // Right dock x∈[1280,1920].
        let right = Some([1280.0, 0.0, 640.0, 1000.0]);
        assert_eq!(
            splitter_hit_test(None, right, None, 6.0, (1281.0, 400.0)),
            Some(PanelSide::Right)
        );
        // Bottom dock y∈[700,1000], x∈[0,1920].
        let bottom = Some([0.0, 700.0, 1920.0, 300.0]);
        assert_eq!(
            splitter_hit_test(None, None, bottom, 6.0, (900.0, 702.0)),
            Some(PanelSide::Bottom)
        );
    }

    #[test]
    fn clamp_panel_size_bounds() {
        let vp = (1000.0, 800.0);
        // Below MIN → MIN.
        assert_eq!(clamp_panel_size(PanelSide::Left, 10.0, vp), MIN_PANEL_PX);
        // Above 60% width → capped.
        assert!((clamp_panel_size(PanelSide::Left, 5000.0, vp) - 600.0).abs() < 1e-3);
        // Bottom uses viewport height.
        assert!((clamp_panel_size(PanelSide::Bottom, 5000.0, vp) - 480.0).abs() < 1e-3);
    }

    #[test]
    fn apply_panel_drag_directions() {
        let vp = (2000.0, 2000.0);
        // Left grows with +dx.
        assert_eq!(
            apply_panel_drag(PanelSide::Left, 240.0, 30.0, 0.0, vp),
            270.0
        );
        // Right grows with -dx.
        assert_eq!(
            apply_panel_drag(PanelSide::Right, 240.0, 30.0, 0.0, vp),
            210.0
        );
        // Bottom grows with -dy.
        assert_eq!(
            apply_panel_drag(PanelSide::Bottom, 300.0, 0.0, -50.0, vp),
            350.0
        );
    }

    #[test]
    fn deadzone_gate() {
        // Within the radius → not past.
        assert!(!past_deadzone((100.0, 100.0), (102.0, 101.0), 3.0));
        // Just outside on one axis → past.
        assert!(past_deadzone((100.0, 100.0), (104.0, 100.0), 3.0));
        // Diagonal crossing the radius → past.
        assert!(past_deadzone((0.0, 0.0), (3.0, 3.0), 3.0));
        // Exactly on the radius → not past (strict).
        assert!(!past_deadzone((0.0, 0.0), (3.0, 0.0), 3.0));
    }

    #[test]
    fn resolve_prefers_card_then_splitter() {
        // Card body while navigating → CardMove.
        assert_eq!(
            resolve_press_target(true, Some((7, CardZone::Body)), Some(PanelSide::Left)),
            Some(DragTarget::CardMove(7))
        );
        // Card resize zone → CardResize.
        assert_eq!(
            resolve_press_target(true, Some((7, CardZone::ResizeRight)), None),
            Some(DragTarget::CardResize(7, CardZone::ResizeRight))
        );
        // Not navigating → ignore card, fall to splitter.
        assert_eq!(
            resolve_press_target(false, Some((7, CardZone::Body)), Some(PanelSide::Left)),
            Some(DragTarget::PanelEdge(PanelSide::Left))
        );
        // Nothing → None.
        assert_eq!(resolve_press_target(false, None, None), None);
    }
}
