//! Edge geometry for the Code Graph HUD.
//!
//! The quad renderer only draws axis-aligned rects, so each edge is an "elbow":
//! a horizontal connector from the center pill to a mid-x, a vertical connector
//! at mid-x, then a horizontal connector into the side pill. No rotation needed.

use crate::codegraph::layout::PillRect;

/// An axis-aligned segment `[x, y, w, h]` for the quad renderer.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EdgeQuad {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

/// Elbow connector between `center` and a side `pill`.
/// `to_right == true` connects center→callee (rightward);
/// `false` connects caller→center. `scale` thickens the line on HiDPI.
pub fn elbow(center: PillRect, pill: PillRect, to_right: bool, scale: f32) -> Vec<EdgeQuad> {
    let thick = 1.5 * scale.max(0.1);
    let cy = center.y + center.h * 0.5; // center anchor is ALWAYS at center's mid-y
    let py = pill.y + pill.h * 0.5; // pill anchor is ALWAYS at pill's mid-y
    // Horizontal edges each node connects on: callee on the right uses center's
    // right edge + callee's left edge; caller on the left uses center's left edge
    // + caller's right edge.
    let center_edge = if to_right {
        center.x + center.w
    } else {
        center.x
    };
    let pill_edge = if to_right { pill.x } else { pill.x + pill.w };
    let mid_x = (center_edge + pill_edge) * 0.5;
    let y0 = cy.min(py);
    let y1 = cy.max(py);
    vec![
        // horizontal from center (at cy) to mid_x
        EdgeQuad {
            x: center_edge.min(mid_x),
            y: cy - thick * 0.5,
            w: (center_edge - mid_x).abs().max(thick),
            h: thick,
        },
        // vertical at mid_x spanning cy..py
        EdgeQuad {
            x: mid_x - thick * 0.5,
            y: y0,
            w: thick,
            h: (y1 - y0).max(thick),
        },
        // horizontal from mid_x to pill (at py)
        EdgeQuad {
            x: mid_x.min(pill_edge),
            y: py - thick * 0.5,
            w: (pill_edge - mid_x).abs().max(thick),
            h: thick,
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegraph::layout::PillRect;

    #[test]
    fn elbow_returns_three_segments() {
        let c = PillRect {
            x: 300.0,
            y: 180.0,
            w: 200.0,
            h: 60.0,
        };
        let p = PillRect {
            x: 600.0,
            y: 60.0,
            w: 160.0,
            h: 44.0,
        };
        let segs = elbow(c, p, true, 1.0);
        assert_eq!(segs.len(), 3);
        // first segment starts at center right edge (300 + 200 = 500)
        assert!((segs[0].x - 500.0).abs() < 0.5);
    }
}
