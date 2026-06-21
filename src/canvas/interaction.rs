//! Pure pointer hit-testing and resize math for canvas cards. No GPU/winit.

use crate::canvas::model::{
    BlockId, BlockRelation, CanvasBlock, Camera, CARD_BOTTOM_LINES, CARD_HEADER_LINES,
    CARD_MIN_LINES,
};

/// Screen-pixel thickness of a card's right/bottom resize bands.
pub const CARD_RESIZE_BAND_PX: f32 = 8.0;
/// Screen-pixel size of the bottom-right "both axes" corner handle.
pub const CARD_RESIZE_CORNER_PX: f32 = 12.0;

/// Which part of a card the cursor is over.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CardZone {
    Body,
    ResizeRight,
    ResizeBottom,
    ResizeCorner,
}

/// Topmost card (and which zone) under `cursor` in screen pixels, skipping the
/// Focal anchor (never drawn as a draggable card). Iterates back-to-front so the
/// last-drawn (visually top) card wins on overlap.
pub fn card_pointer_hit_test(
    blocks: &[CanvasBlock],
    camera: &Camera,
    cursor: (f32, f32),
) -> Option<(BlockId, CardZone)> {
    let (cx, cy) = cursor;
    for block in blocks.iter().rev() {
        if block.relation == BlockRelation::Focal {
            continue;
        }
        let [sx, sy, sw, sh] = camera.world_to_screen(block.world);
        if sw <= 0.0 || sh <= 0.0 {
            continue;
        }
        // Outside the rect entirely → not this card.
        if cx < sx || cx > sx + sw || cy < sy || cy > sy + sh {
            continue;
        }
        let near_right = cx >= sx + sw - CARD_RESIZE_BAND_PX;
        let near_bottom = cy >= sy + sh - CARD_RESIZE_BAND_PX;
        let in_corner =
            cx >= sx + sw - CARD_RESIZE_CORNER_PX && cy >= sy + sh - CARD_RESIZE_CORNER_PX;
        let zone = if in_corner {
            CardZone::ResizeCorner
        } else if near_right {
            CardZone::ResizeRight
        } else if near_bottom {
            CardZone::ResizeBottom
        } else {
            CardZone::Body
        };
        return Some((block.id, zone));
    }
    None
}

/// New card world width from the cursor x and the card's fixed screen-left edge.
/// Top-left is fixed during resize, so width = (cursor - left) / zoom.
pub fn resize_width_world(card_screen_x: f32, cursor_x: f32, zoom: f32) -> f32 {
    let z = if zoom > 0.0 { zoom } else { 1.0 };
    (cursor_x - card_screen_x) / z
}

/// New visible-row count from the cursor y and the card's fixed screen-top edge.
/// Inverts `CanvasState::card_height_exact`: world_h = line_h*(HEADER + rows + BOTTOM).
pub fn resize_height_rows(card_screen_y: f32, cursor_y: f32, zoom: f32, line_h: f32) -> usize {
    let z = if zoom > 0.0 { zoom } else { 1.0 };
    let world_h = (cursor_y - card_screen_y) / z;
    if line_h <= 0.0 {
        return CARD_MIN_LINES;
    }
    let rows_f = world_h / line_h - CARD_HEADER_LINES - CARD_BOTTOM_LINES;
    let rows = rows_f.round();
    if !rows.is_finite() || rows < CARD_MIN_LINES as f32 {
        CARD_MIN_LINES
    } else {
        rows as usize
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::canvas::model::{
        BlockId, BlockOrigin, BlockRelation, BlockSnapshot, CanvasBlock, Camera, WorldRect,
    };
    use std::path::PathBuf;

    fn card(id: BlockId, rel: BlockRelation, rect: WorldRect) -> CanvasBlock {
        CanvasBlock {
            id,
            relation: rel,
            origin: BlockOrigin {
                path: PathBuf::from("/x.rs"),
                start_byte: 0,
                end_byte: 0,
                symbol_name: "s".into(),
                lsp_line: 0,
                lsp_character: 0,
            },
            snapshot: BlockSnapshot::default(),
            world: rect,
            pinned: false,
            context_lines: 8,
            parent: None,
            scope_lines: None,
            height_rows: None,
            spawned_at: None,
        }
    }

    #[test]
    fn body_hit_is_inside_rect() {
        let cam = Camera::default(); // zoom 1, offset 0 → world == screen
        let blocks = vec![card(1, BlockRelation::Caller, WorldRect::new(0.0, 0.0, 100.0, 100.0))];
        assert_eq!(
            card_pointer_hit_test(&blocks, &cam, (50.0, 50.0)),
            Some((1, CardZone::Body))
        );
    }

    #[test]
    fn corner_beats_edges() {
        let cam = Camera::default();
        let blocks = vec![card(1, BlockRelation::Caller, WorldRect::new(0.0, 0.0, 100.0, 100.0))];
        // bottom-right corner zone
        assert_eq!(
            card_pointer_hit_test(&blocks, &cam, (98.0, 98.0)),
            Some((1, CardZone::ResizeCorner))
        );
    }

    #[test]
    fn right_and_bottom_edges() {
        let cam = Camera::default();
        let blocks = vec![card(1, BlockRelation::Caller, WorldRect::new(0.0, 0.0, 100.0, 100.0))];
        assert_eq!(
            card_pointer_hit_test(&blocks, &cam, (98.0, 50.0)),
            Some((1, CardZone::ResizeRight))
        );
        assert_eq!(
            card_pointer_hit_test(&blocks, &cam, (50.0, 98.0)),
            Some((1, CardZone::ResizeBottom))
        );
    }

    #[test]
    fn miss_returns_none_and_focal_is_skipped() {
        let cam = Camera::default();
        let blocks = vec![
            card(1, BlockRelation::Focal, WorldRect::new(0.0, 0.0, 100.0, 100.0)),
            card(2, BlockRelation::Caller, WorldRect::new(200.0, 200.0, 50.0, 50.0)),
        ];
        // Inside the focal rect → skipped (focal is never a draggable card).
        assert_eq!(card_pointer_hit_test(&blocks, &cam, (50.0, 50.0)), None);
        // Far away → none.
        assert_eq!(card_pointer_hit_test(&blocks, &cam, (10.0, 400.0)), None);
        // Inside the relation card → body.
        assert_eq!(
            card_pointer_hit_test(&blocks, &cam, (210.0, 210.0)),
            Some((2, CardZone::Body))
        );
    }

    #[test]
    fn topmost_card_wins_on_overlap() {
        let cam = Camera::default();
        let blocks = vec![
            card(1, BlockRelation::Caller, WorldRect::new(0.0, 0.0, 100.0, 100.0)),
            card(2, BlockRelation::Callee, WorldRect::new(50.0, 50.0, 100.0, 100.0)),
        ];
        // Overlap region (60,60): the later block (id 2, drawn on top) wins.
        assert_eq!(
            card_pointer_hit_test(&blocks, &cam, (60.0, 60.0)),
            Some((2, CardZone::Body))
        );
    }

    #[test]
    fn hit_test_respects_zoom() {
        let cam = Camera {
            offset_x: 0.0,
            offset_y: 0.0,
            zoom: 2.0,
        };
        // World rect 0..50 → screen 0..100 at zoom 2.
        let blocks = vec![card(1, BlockRelation::Caller, WorldRect::new(0.0, 0.0, 50.0, 50.0))];
        assert_eq!(
            card_pointer_hit_test(&blocks, &cam, (40.0, 40.0)),
            Some((1, CardZone::Body))
        );
        // Past the screen extent (>100) → miss.
        assert_eq!(card_pointer_hit_test(&blocks, &cam, (140.0, 40.0)), None);
    }

    #[test]
    fn width_world_inverts_zoom() {
        // cursor at screen x=200, card left at screen x=20, zoom 2 → world width 90.
        assert!((resize_width_world(20.0, 200.0, 2.0) - 90.0).abs() < 1e-3);
    }

    #[test]
    fn height_rows_inverts_card_height_formula() {
        use crate::canvas::model::{CARD_BOTTOM_LINES, CARD_HEADER_LINES};
        let line_h = 20.0;
        // Make a screen height that corresponds to exactly 10 rows at zoom 1:
        // world_h = line_h*(HEADER + 10 + BOTTOM); card top at screen y=0.
        let world_h = line_h * (CARD_HEADER_LINES + 10.0 + CARD_BOTTOM_LINES);
        let rows = resize_height_rows(0.0, world_h, 1.0, line_h);
        assert_eq!(rows, 10);
    }

    #[test]
    fn height_rows_floors_at_card_min() {
        use crate::canvas::model::CARD_MIN_LINES;
        // A tiny drag → never below CARD_MIN_LINES.
        assert_eq!(resize_height_rows(0.0, 1.0, 1.0, 20.0), CARD_MIN_LINES);
    }
}
