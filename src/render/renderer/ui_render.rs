//! Right-sidebar (AI Chat) background rendering.
//!
//! Generates region quads for the three-layer panel background used when the
//! RightSidebar is visible: outline border, inner fill, and input-box accent.

use crate::render::region_pipeline::RegionDrawInstance;

/// Build the three-layer background quads for a visible RightSidebar.
///
/// * **Step 1 — Outline border**: full `bounds`, `border_color` from
///   `theme.ui.border_color`.
/// * **Step 2 — Inner background**: inset by `panel_border_width`, filled
///   with `panel_bg`.
/// * **Step 3 — Input box**: `input_bounds` filled with `input_bg` (a
///   slightly different shade such as `editor.bg`).
pub fn right_sidebar_background_quads(
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

    let t = panel_border_width
        .max(0.0)
        .min(w * 0.5)
        .min(h * 0.5);
    let mut quads = Vec::with_capacity(4);

    // Step 1: Outline border — full bounds.
    quads.push(
        RegionDrawInstance::new(bounds, border_color).with_radius(border_radius),
    );

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
