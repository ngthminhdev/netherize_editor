# Mouse Spatial Interactions Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add mouse drag support for resizing docks, and moving + resizing canvas cards, in the keyboard-first Netherize editor.

**Architecture:** A single centralized pointer-drag state machine. All geometry/hit-test math lives in two new **pure** modules (`canvas/interaction.rs`, `workbench/pointer_drag.rs`) that are unit-tested without GPU/winit. `application.rs` owns one `Option<ActiveDrag>` + `Option<HoverTarget>` and wires winit's existing `MouseInput`/`CursorMoved` arms into those pure helpers and into thin `AppState` mutators. No model fields are added — card width/height already live in `CanvasBlock.world` (`WorldRect`), which the renderer already uses as the source of truth.

**Tech Stack:** Rust, winit 0.30.13, wgpu. Inline `#[cfg(test)]` unit tests. `cargo test` / `cargo build`.

## Global Constraints

- **No new model fields.** Card width is `block.world.w`, height is `block.world.h` (driven by `height_rows`). The renderer draws every card via `cam.world_to_screen(block.world)`.
- **Reuse existing clamps:** card width `[block_w*0.5, block_w*2.5]`; card height rows `[CARD_MIN_LINES, CANVAS_CARD_HARD_MAX]` (these match the keyboard methods `canvas_change_focused_width` / `canvas_change_focused_height`).
- **winit 0.30.13 cursor API:** `window.set_cursor(winit::window::CursorIcon::EwResize)` etc. Valid icons used: `EwResize`, `NsResize`, `NwseResize`, `Move`, `Default`.
- **Canvas mouse only in `CanvasInteraction::Navigate`** with a canvas open. Never in `EditCard` / `Background`. No text-editing by mouse.
- **Mouse drag overrides `pinned`** (direct manipulation is explicit intent).
- **Coordinate space:** cursor positions, `current_*_bounds()`, and canvas screen coords are all physical pixels over the full window (the canvas is a full-window overlay using `surface_state.config.width/height`). No panel offset to subtract.
- **TDD, frequent commits.** Commit after each task's tests pass.
- Run the **whole** suite at least once at the end: `cargo test`.

---

## File Structure

| File | Responsibility | New/Modify |
|------|---------------|-----------|
| `src/canvas/interaction.rs` | Pure: `CardZone`, card pointer hit-test, pixel→world / pixel→rows conversions, resize constants | Create |
| `src/canvas/mod.rs` | Register `pub mod interaction;` | Modify |
| `src/workbench/pointer_drag.rs` | Pure: `PanelSide`, `DragTarget`, `HoverTarget`, `DragAnchor`, `ActiveDrag`, splitter hit-test, panel-size clamp/apply, press-priority resolver, drag constants | Create |
| `src/workbench/mod.rs` | Register `pub mod pointer_drag;` | Modify |
| `src/app/app_state/canvas.rs` | `canvas_is_navigating`, `canvas_pointer_move_block`, `canvas_pointer_resize_block` | Modify |
| `src/app/event_loop/mod.rs` | Add `active_drag` + `hover_target` fields to `AppShell` | Modify |
| `src/app/event_loop/setup.rs` | Initialize the two new fields to `None` | Modify |
| `src/app/event_loop/application.rs` | Wire press/move/release drag lifecycle + hover cursor; new helper methods | Modify |
| `src/render/renderer/canvas.rs` (or overlay pass) | Draw the hover highlight quad | Modify (Task 6) |

---

## Task 1: Pure canvas pointer hit-test + resize math

**Files:**
- Create: `src/canvas/interaction.rs`
- Modify: `src/canvas/mod.rs`
- Test: inline `#[cfg(test)]` in `src/canvas/interaction.rs`

**Interfaces:**
- Consumes: `crate::canvas::model::{CanvasBlock, Camera, BlockId, BlockRelation, WorldRect, CARD_MIN_LINES, CARD_HEADER_LINES, CARD_BOTTOM_LINES}`.
- Produces:
  - `pub enum CardZone { Body, ResizeRight, ResizeBottom, ResizeCorner }` (derive `Debug, Clone, Copy, PartialEq, Eq`).
  - `pub fn card_pointer_hit_test(blocks: &[CanvasBlock], camera: &Camera, cursor: (f32, f32)) -> Option<(BlockId, CardZone)>`
  - `pub fn resize_width_world(card_screen_x: f32, cursor_x: f32, zoom: f32) -> f32`
  - `pub fn resize_height_rows(card_screen_y: f32, cursor_y: f32, zoom: f32, line_h: f32) -> usize`
  - `pub const CARD_RESIZE_BAND_PX: f32` , `pub const CARD_RESIZE_CORNER_PX: f32`

- [ ] **Step 1: Register the module**

In `src/canvas/mod.rs`, add alongside the existing `pub mod` lines:

```rust
pub mod interaction;
```

- [ ] **Step 2: Write the failing tests**

Create `src/canvas/interaction.rs` with ONLY the tests first (the code comes in Step 4):

```rust
//! Pure pointer hit-testing and resize math for canvas cards. No GPU/winit.

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
        let cam = Camera { offset_x: 0.0, offset_y: 0.0, zoom: 2.0 };
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
```

- [ ] **Step 3: Run the tests to verify they fail to compile**

Run: `cargo test --lib canvas::interaction`
Expected: FAIL — `cannot find function card_pointer_hit_test` / `CardZone not found`.

- [ ] **Step 4: Write the implementation**

Add ABOVE the `#[cfg(test)]` block in `src/canvas/interaction.rs`:

```rust
use crate::canvas::model::{
    BlockRelation, BlockId, CanvasBlock, Camera, CARD_BOTTOM_LINES, CARD_HEADER_LINES,
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
        let in_corner = cx >= sx + sw - CARD_RESIZE_CORNER_PX
            && cy >= sy + sh - CARD_RESIZE_CORNER_PX;
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
```

- [ ] **Step 5: Run the tests to verify they pass**

Run: `cargo test --lib canvas::interaction`
Expected: PASS (8 tests).

- [ ] **Step 6: Commit**

```bash
git add src/canvas/interaction.rs src/canvas/mod.rs
git commit -m "feat(canvas): pure pointer hit-test + resize math for cards"
```

---

## Task 2: Pure pointer-drag types, splitter hit-test, panel-size math

**Files:**
- Create: `src/workbench/pointer_drag.rs`
- Modify: `src/workbench/mod.rs`
- Test: inline `#[cfg(test)]` in `src/workbench/pointer_drag.rs`

**Interfaces:**
- Consumes: `crate::canvas::model::BlockId`, `crate::canvas::interaction::CardZone`.
- Produces:
  - `pub enum PanelSide { Left, Right, Bottom }`
  - `pub enum DragTarget { PanelEdge(PanelSide), CardMove(BlockId), CardResize(BlockId, CardZone) }`
  - `pub enum HoverTarget { PanelEdge(PanelSide), CardBody, CardResize(CardZone) }`
  - `pub enum DragAnchor { Panel { start_size: f32 }, CardMove { start_world: (f32, f32) }, CardResize }`
  - `pub struct ActiveDrag { pub target: DragTarget, pub start_cursor: (f32, f32), pub anchor: DragAnchor }`
  - `pub fn splitter_hit_test(left: Option<[f32;4]>, right: Option<[f32;4]>, bottom: Option<[f32;4]>, band: f32, cursor: (f32,f32)) -> Option<PanelSide>`
  - `pub fn clamp_panel_size(side: PanelSide, raw: f32, viewport: (f32, f32)) -> f32`
  - `pub fn apply_panel_drag(side: PanelSide, start_size: f32, dx: f32, dy: f32, viewport: (f32, f32)) -> f32`
  - `pub fn resolve_press_target(canvas_navigating: bool, card_hit: Option<(BlockId, CardZone)>, splitter_hit: Option<PanelSide>) -> Option<DragTarget>`
  - `pub const SPLITTER_BAND_PX: f32`, `pub const MIN_PANEL_PX: f32`, `pub const MAX_PANEL_FRACTION: f32`, `pub const DRAG_DEADZONE_PX: f32`

- [ ] **Step 1: Register the module**

In `src/workbench/mod.rs`, add alongside the existing `pub mod` lines:

```rust
pub mod pointer_drag;
```

- [ ] **Step 2: Write the failing tests**

Create `src/workbench/pointer_drag.rs` with ONLY the tests first:

```rust
//! Pure pointer-drag types and panel/splitter geometry. No GPU/winit.

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
        assert_eq!(splitter_hit_test(left, None, None, 6.0, (200.0, 500.0)), None);
        // Outside the dock's vertical span → miss.
        assert_eq!(splitter_hit_test(left, None, None, 6.0, (240.0, 1500.0)), None);
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
        assert_eq!(clamp_panel_size(PanelSide::Left, 5000.0, vp), 600.0);
        // Bottom uses viewport height.
        assert_eq!(clamp_panel_size(PanelSide::Bottom, 5000.0, vp), 480.0);
    }

    #[test]
    fn apply_panel_drag_directions() {
        let vp = (2000.0, 2000.0);
        // Left grows with +dx.
        assert_eq!(apply_panel_drag(PanelSide::Left, 240.0, 30.0, 0.0, vp), 270.0);
        // Right grows with -dx.
        assert_eq!(apply_panel_drag(PanelSide::Right, 240.0, 30.0, 0.0, vp), 210.0);
        // Bottom grows with -dy.
        assert_eq!(apply_panel_drag(PanelSide::Bottom, 300.0, 0.0, -50.0, vp), 350.0);
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
```

- [ ] **Step 3: Run the tests to verify they fail**

Run: `cargo test --lib workbench::pointer_drag`
Expected: FAIL — types/functions not found.

- [ ] **Step 4: Write the implementation**

Add ABOVE the `#[cfg(test)]` block:

```rust
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
    Panel { start_size: f32 },
    CardMove { start_world: (f32, f32) },
    /// Card resize keeps the top-left fixed, so it recomputes from the live
    /// camera each move and needs no captured anchor.
    CardResize,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ActiveDrag {
    pub target: DragTarget,
    pub start_cursor: (f32, f32),
    pub anchor: DragAnchor,
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

/// Press hit-test priority: a canvas card (only while navigating) beats a
/// splitter; otherwise the splitter; otherwise nothing.
pub fn resolve_press_target(
    canvas_navigating: bool,
    card_hit: Option<(BlockId, CardZone)>,
    splitter_hit: Option<PanelSide>,
) -> Option<DragTarget> {
    if canvas_navigating {
        if let Some((id, zone)) = card_hit {
            return Some(match zone {
                CardZone::Body => DragTarget::CardMove(id),
                other => DragTarget::CardResize(id, other),
            });
        }
    }
    splitter_hit.map(DragTarget::PanelEdge)
}
```

- [ ] **Step 5: Run the tests to verify they pass**

Run: `cargo test --lib workbench::pointer_drag`
Expected: PASS (5 tests).

- [ ] **Step 6: Commit**

```bash
git add src/workbench/pointer_drag.rs src/workbench/mod.rs
git commit -m "feat(workbench): pure pointer-drag types + splitter/panel math"
```

---

## Task 3: AppState canvas mutators for pointer move + resize

**Files:**
- Modify: `src/app/app_state/canvas.rs`
- Test: inline `#[cfg(test)]` (the existing `mod tests` block at the bottom of `src/app/app_state/canvas.rs`)

**Interfaces:**
- Consumes: existing `CanvasState`, `CanvasInteraction`, `BlockRelation`, `CARD_MIN_LINES`, the file-local `CANVAS_CARD_HARD_MAX`, `state.card_height_exact`.
- Produces (all on `impl AppState`):
  - `pub fn canvas_is_navigating(&self) -> bool`
  - `pub fn canvas_pointer_move_block(&mut self, id: BlockId, world_x: f32, world_y: f32) -> bool`
  - `pub fn canvas_pointer_resize_block(&mut self, id: BlockId, new_w: Option<f32>, new_rows: Option<usize>) -> bool`

- [ ] **Step 1: Write the failing tests**

Add to the existing `#[cfg(test)] mod tests` block in `src/app/app_state/canvas.rs`. (The existing test `assert!(canvas.block_w > 0.0 ...)` shows how a canvas is opened in tests — reuse that opener helper if present; otherwise call `open_canvas`.)

```rust
#[test]
fn pointer_move_block_sets_absolute_world_and_user_arranged() {
    let mut app = AppState::new_blank_for_test(); // use the crate's test ctor
    app.open_canvas(400.0, 600.0, 20.0);
    // Spawn at least one relation card via the existing test path, then grab its id.
    let id = app
        .canvas()
        .unwrap()
        .blocks
        .iter()
        .find(|b| b.relation != BlockRelation::Focal)
        .map(|b| b.id)
        .expect("a relation card exists");
    assert!(app.canvas_pointer_move_block(id, 123.0, 456.0));
    let b = app.canvas().unwrap().block(id).unwrap();
    assert_eq!((b.world.x, b.world.y), (123.0, 456.0));
    assert!(app.canvas().unwrap().user_arranged);
}

#[test]
fn pointer_move_overrides_pin_but_not_focal() {
    let mut app = AppState::new_blank_for_test();
    app.open_canvas(400.0, 600.0, 20.0);
    let canvas = app.canvas().unwrap();
    let focal = canvas
        .blocks
        .iter()
        .find(|b| b.relation == BlockRelation::Focal)
        .map(|b| b.id);
    let card = canvas
        .blocks
        .iter()
        .find(|b| b.relation != BlockRelation::Focal)
        .map(|b| b.id)
        .unwrap();
    // Pin the card, then confirm a mouse move still relocates it.
    app.canvas_mut_for_test().blocks.iter_mut().find(|b| b.id == card).unwrap().pinned = true;
    assert!(app.canvas_pointer_move_block(card, 10.0, 20.0));
    // The focal anchor is never movable.
    if let Some(f) = focal {
        assert!(!app.canvas_pointer_move_block(f, 10.0, 20.0));
    }
}

#[test]
fn pointer_resize_clamps_width_and_rows() {
    let mut app = AppState::new_blank_for_test();
    app.open_canvas(400.0, 600.0, 20.0);
    let id = app
        .canvas()
        .unwrap()
        .blocks
        .iter()
        .find(|b| b.relation != BlockRelation::Focal)
        .map(|b| b.id)
        .unwrap();
    // Width far above max (block_w*2.5 = 1000) clamps to 1000.
    assert!(app.canvas_pointer_resize_block(id, Some(99999.0), None));
    assert!((app.canvas().unwrap().block(id).unwrap().world.w - 1000.0).abs() < 1.0);
    // Rows below CARD_MIN_LINES clamp up.
    assert!(app.canvas_pointer_resize_block(id, None, Some(0)));
    assert_eq!(
        app.canvas().unwrap().block(id).unwrap().height_rows,
        Some(CARD_MIN_LINES)
    );
}
```

> **Implementer note:** match the exact test constructor / canvas-opener and any `canvas_mut_for_test` accessor the existing tests in this file already use. If no mutable test accessor exists, add a small `#[cfg(test)] pub(crate) fn canvas_mut_for_test(&mut self) -> &mut CanvasState`. Read the top of the existing `mod tests` block first and follow its setup pattern verbatim.

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test --lib app_state::canvas::tests::pointer_`
Expected: FAIL — methods not found.

- [ ] **Step 3: Write the implementation**

Add to the `impl AppState` block in `src/app/app_state/canvas.rs` (next to `canvas_change_focused_width`):

```rust
/// True when a canvas is open and in the `Navigate` interaction sub-state —
/// the only state where mouse drag/resize of cards is allowed.
pub fn canvas_is_navigating(&self) -> bool {
    matches!(
        self.canvas.as_ref().map(|c| c.interaction),
        Some(crate::canvas::CanvasInteraction::Navigate)
    )
}

/// Move a card to an absolute world position (mouse drag). Overrides `pinned`
/// (direct manipulation is explicit intent); never moves the Focal anchor.
/// Marks the layout user-arranged. Returns whether a card moved.
pub fn canvas_pointer_move_block(&mut self, id: BlockId, world_x: f32, world_y: f32) -> bool {
    let Some(state) = self.canvas.as_mut() else {
        return false;
    };
    let Some(b) = state.blocks.iter_mut().find(|b| b.id == id) else {
        return false;
    };
    if b.relation == BlockRelation::Focal {
        return false;
    }
    b.world.x = world_x;
    b.world.y = world_y;
    state.user_arranged = true;
    true
}

/// Resize a card (mouse drag on an edge/corner). `new_w` sets `world.w` clamped
/// to `[block_w*0.5, block_w*2.5]`; `new_rows` sets `height_rows` clamped to
/// `[CARD_MIN_LINES, min(snapshot_lines, CANVAS_CARD_HARD_MAX)]` and recomputes
/// `world.h`. Never the Focal anchor. Returns whether anything changed.
pub fn canvas_pointer_resize_block(
    &mut self,
    id: BlockId,
    new_w: Option<f32>,
    new_rows: Option<usize>,
) -> bool {
    let Some(state) = self.canvas.as_mut() else {
        return false;
    };
    let base_w = if state.block_w > 0.0 { state.block_w } else { 400.0 };
    let min_w = base_w * 0.5;
    let max_w = base_w * 2.5;

    // Compute the clamped row count + its world height BEFORE the mutable borrow.
    let (snapshot_lines, line_h) = match state.blocks.iter().find(|b| b.id == id) {
        Some(b) => (b.snapshot.text.split('\n').count().max(1), state.line_h),
        None => return false,
    };
    let max_rows = snapshot_lines.clamp(CARD_MIN_LINES, CANVAS_CARD_HARD_MAX);
    let clamped_rows = new_rows.map(|r| r.clamp(CARD_MIN_LINES, max_rows));
    let new_h = clamped_rows
        .and_then(|r| (line_h > 0.0).then(|| state.card_height_exact(r)));

    let Some(b) = state.blocks.iter_mut().find(|b| b.id == id) else {
        return false;
    };
    if b.relation == BlockRelation::Focal {
        return false;
    }
    let mut changed = false;
    if let Some(w) = new_w {
        let cw = w.clamp(min_w, max_w);
        if (cw - b.world.w).abs() >= 0.5 {
            b.world.w = cw;
            changed = true;
        }
    }
    if let Some(r) = clamped_rows {
        if b.height_rows != Some(r) {
            b.height_rows = Some(r);
            changed = true;
        }
        if let Some(h) = new_h {
            b.world.h = h; // top-left fixed; grow/shrink downward
        }
    }
    if changed {
        state.user_arranged = true;
    }
    changed
}
```

> **Implementer note:** confirm `BlockId` and `BlockRelation` are already imported at the top of `canvas.rs` (the existing methods use `BlockRelation`). Add `use crate::canvas::model::BlockId;` if not already in scope.

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test --lib app_state::canvas::tests::pointer_`
Expected: PASS (3 tests).

- [ ] **Step 5: Commit**

```bash
git add src/app/app_state/canvas.rs
git commit -m "feat(canvas): absolute pointer move + resize mutators on AppState"
```

---

## Task 4: Wire panel/dock resize into the event loop

**Files:**
- Modify: `src/app/event_loop/mod.rs` (add `AppShell` fields)
- Modify: `src/app/event_loop/setup.rs` (init fields)
- Modify: `src/app/event_loop/application.rs` (drag lifecycle + helpers)
- Test: manual GUI verification (winit/GPU wiring is not unit-testable; the math it calls is already covered by Tasks 1–2).

**Interfaces:**
- Consumes: `workbench::pointer_drag::{ActiveDrag, DragTarget, DragAnchor, PanelSide, HoverTarget, splitter_hit_test, resolve_press_target, apply_panel_drag, SPLITTER_BAND_PX, DRAG_DEADZONE_PX}`; existing `current_left_sidebar_bounds` / `current_right_sidebar_bounds` / `current_bottom_panel_bounds`; existing `self.last_cursor_position`, `self.panel_state`, `self.window`.
- Produces: `AppShell.active_drag: Option<ActiveDrag>`, `AppShell.hover_target: Option<HoverTarget>`; methods `begin_pointer_drag`, `update_pointer_drag`, `end_pointer_drag`, `viewport_size`.

- [ ] **Step 1: Add the fields to `AppShell`**

In `src/app/event_loop/mod.rs`, near `last_cursor_position: Option<(f32, f32)>,` (line ~100) add:

```rust
    /// Active pointer drag (panel resize / card move / card resize), if any.
    active_drag: Option<crate::workbench::pointer_drag::ActiveDrag>,
    /// What draggable zone the cursor is currently hovering (for cursor shape +
    /// highlight). Recomputed on every `CursorMoved` while not dragging.
    hover_target: Option<crate::workbench::pointer_drag::HoverTarget>,
```

- [ ] **Step 2: Initialize the fields**

In `src/app/event_loop/setup.rs`, near `last_cursor_position: None,` (line ~156) add:

```rust
            active_drag: None,
            hover_target: None,
```

- [ ] **Step 3: Add a viewport helper + the drag lifecycle methods**

In `src/app/event_loop/application.rs`, near the other `current_*_bounds` / `handle_*_mouse_*` helpers (around line 456), add:

```rust
    /// Physical-pixel window size, used to cap dock resize.
    fn viewport_size(&self) -> (f32, f32) {
        self.window
            .as_ref()
            .map(|w| {
                let s = w.inner_size();
                (s.width as f32, s.height as f32)
            })
            .unwrap_or((1920.0, 1080.0))
    }

    /// Hit-test the press point and, if it lands on a draggable zone, capture an
    /// `ActiveDrag`. Returns whether a drag started. (Task 5 extends this with
    /// canvas card targets; for now it covers panel splitters only.)
    fn begin_pointer_drag(&mut self) -> bool {
        use crate::workbench::pointer_drag::{
            resolve_press_target, splitter_hit_test, ActiveDrag, DragAnchor, DragTarget,
            PanelSide, SPLITTER_BAND_PX,
        };
        let Some(cursor) = self.last_cursor_position else {
            return false;
        };

        let left = self
            .panel_state
            .left
            .visible
            .then(|| self.current_left_sidebar_bounds())
            .flatten();
        let right = self
            .panel_state
            .right
            .visible
            .then(|| self.current_right_sidebar_bounds())
            .flatten();
        let bottom = self
            .panel_state
            .bottom
            .visible
            .then(|| self.current_bottom_panel_bounds())
            .flatten();
        let splitter_hit = splitter_hit_test(left, right, bottom, SPLITTER_BAND_PX, cursor);

        // Task 5 fills in `card_hit`; panels-only for now.
        let card_hit = None;
        let Some(target) = resolve_press_target(self.canvas_is_navigating_shell(), card_hit, splitter_hit)
        else {
            return false;
        };

        let anchor = match target {
            DragTarget::PanelEdge(side) => DragAnchor::Panel {
                start_size: match side {
                    PanelSide::Left => self.panel_state.left.size_px,
                    PanelSide::Right => self.panel_state.right.size_px,
                    PanelSide::Bottom => self.panel_state.bottom.size_px,
                },
            },
            // Card anchors are added in Task 5.
            _ => return false,
        };
        self.active_drag = Some(ActiveDrag {
            target,
            start_cursor: cursor,
            anchor,
        });
        true
    }

    /// Apply an in-progress drag for the current cursor position. Returns whether
    /// state changed (and thus a redraw is needed).
    fn update_pointer_drag(&mut self, cursor: (f32, f32)) -> bool {
        use crate::workbench::pointer_drag::{apply_panel_drag, DragAnchor, DragTarget, PanelSide};
        let Some(drag) = self.active_drag else {
            return false;
        };
        let dx = cursor.0 - drag.start_cursor.0;
        let dy = cursor.1 - drag.start_cursor.1;
        match (drag.target, drag.anchor) {
            (DragTarget::PanelEdge(side), DragAnchor::Panel { start_size }) => {
                let vp = self.viewport_size();
                let new = apply_panel_drag(side, start_size, dx, dy, vp);
                match side {
                    PanelSide::Left => self.panel_state.left.size_px = new,
                    PanelSide::Right => self.panel_state.right.size_px = new,
                    PanelSide::Bottom => self.panel_state.bottom.size_px = new,
                }
                true
            }
            // Card cases are added in Task 5.
            _ => false,
        }
    }

    /// Finish any active drag.
    fn end_pointer_drag(&mut self) -> bool {
        if self.active_drag.take().is_some() {
            return true;
        }
        false
    }

    /// Shell-level wrapper so the press handler reads cleanly.
    fn canvas_is_navigating_shell(&self) -> bool {
        self.app_state.canvas_is_navigating()
    }
```

- [ ] **Step 4: Hook the winit event arms**

In `src/app/event_loop/application.rs`, the `WindowEvent` match (around lines 979–1020):

1. In `WindowEvent::CursorMoved { position, .. }` — after the existing
   `self.last_cursor_position = Some(...)` line, drive an in-progress drag:

```rust
            WindowEvent::CursorMoved { position, .. } => {
                let cursor = (position.x as f32, position.y as f32);
                self.last_cursor_position = Some(cursor);
                if self.active_drag.is_some() {
                    if self.update_pointer_drag(cursor) {
                        self.request_redraw();
                    }
                }
            }
```

2. In `WindowEvent::MouseInput { state, button, .. }` — make a Left **press**
   try to begin a drag FIRST (so it pre-empts the tab/outline click handlers),
   and handle Left **release** to end a drag. Replace the arm body's opening
   so it reads:

```rust
            WindowEvent::MouseInput { state, button, .. } => {
                if button == MouseButton::Left && state == ElementState::Released {
                    if self.end_pointer_drag() {
                        self.request_redraw();
                    }
                    return; // a release never triggers click handlers
                }
                if self.handle_right_terminal_mouse_input(button, state) {
                    self.request_redraw();
                } else if button == MouseButton::Left
                    && state == ElementState::Pressed
                    && self.begin_pointer_drag()
                {
                    self.request_redraw();
                } else if button == MouseButton::Left
                    && state == ElementState::Pressed
                    && self.handle_left_dock_tab_mouse_click()
                {
                    self.request_redraw();
                } else if button == MouseButton::Left
```

   …leaving the remaining `else if` click handlers (right dock tab, outline,
   test runner, bottom tab) unchanged after it.

> **Implementer note:** confirm the enclosing function returns `()` so the early
> `return;` on release is valid (the winit `window_event` handler returns unit).
> If the match is inside a larger expression, replace `return;` with a guard that
> skips the rest of the arm instead.

- [ ] **Step 5: Build and manually verify**

Run: `cargo build`
Expected: compiles clean.

Run: `cargo run` and verify by hand:
- Open the left dock; drag its right edge → it resizes, clamped at ~120px min and 60% max.
- Open the right dock; drag its left edge → resizes.
- Open the bottom dock; drag its top edge → editor/terminal split resizes.
- Dragging anywhere else still does nothing unexpected; clicking dock tabs still switches tabs.

- [ ] **Step 6: Commit**

```bash
git add src/app/event_loop/mod.rs src/app/event_loop/setup.rs src/app/event_loop/application.rs
git commit -m "feat(workbench): drag dock inner edges to resize panels"
```

---

## Task 5: Wire canvas card move + resize into the event loop

**Files:**
- Modify: `src/app/event_loop/application.rs` (extend `begin_pointer_drag` + `update_pointer_drag`)
- Test: manual GUI verification (the math is covered by Tasks 1 & 3).

**Interfaces:**
- Consumes: `crate::canvas::interaction::{card_pointer_hit_test, resize_width_world, resize_height_rows, CardZone}`; `workbench::pointer_drag::{DragTarget, DragAnchor}`; `self.app_state.canvas()`, `self.app_state.canvas_pointer_move_block`, `self.app_state.canvas_pointer_resize_block`.
- Produces: extended drag handling (no new public surface).

- [ ] **Step 1: Compute `card_hit` in `begin_pointer_drag`**

In `begin_pointer_drag`, replace `let card_hit = None;` with a real hit-test
(only while navigating, so cards are ignored in Edit/Background):

```rust
        let card_hit = if self.app_state.canvas_is_navigating() {
            self.app_state.canvas().and_then(|c| {
                crate::canvas::interaction::card_pointer_hit_test(&c.blocks, &c.camera, cursor)
            })
        } else {
            None
        };
```

- [ ] **Step 2: Add card anchors to the `match target` block**

In `begin_pointer_drag`, extend the `anchor` match (replace the `_ => return false,` arm) with card cases:

```rust
            DragTarget::CardMove(id) => {
                let start_world = self
                    .app_state
                    .canvas()
                    .and_then(|c| c.block(id))
                    .map(|b| (b.world.x, b.world.y))
                    .unwrap_or((0.0, 0.0));
                DragAnchor::CardMove { start_world }
            }
            DragTarget::CardResize(_, _) => DragAnchor::CardResize,
```

(Keep the existing `DragTarget::PanelEdge(side) => DragAnchor::Panel { .. }` arm.)

> **Implementer note:** `DragAnchor`/`DragTarget` are already imported via the
> `use` at the top of `begin_pointer_drag`. Remove the now-unreachable
> `_ => return false,` line if the match becomes exhaustive.

- [ ] **Step 3: Add card cases to `update_pointer_drag`**

In `update_pointer_drag`, extend the match (replace its `_ => false,` arm) with:

```rust
            (DragTarget::CardMove(id), DragAnchor::CardMove { start_world }) => {
                let zoom = self
                    .app_state
                    .canvas()
                    .map(|c| c.camera.zoom)
                    .unwrap_or(1.0);
                let wx = start_world.0 + dx / zoom;
                let wy = start_world.1 + dy / zoom;
                self.app_state.canvas_pointer_move_block(id, wx, wy)
            }
            (DragTarget::CardResize(id, zone), DragAnchor::CardResize) => {
                use crate::canvas::interaction::{resize_height_rows, resize_width_world, CardZone};
                // Copy out the card's fixed screen top-left + zoom + line height
                // BEFORE the mutable resize call (ends the immutable borrow).
                let geom = self.app_state.canvas().and_then(|c| {
                    c.block(id).map(|b| {
                        let [sx, sy, _, _] = c.camera.world_to_screen(b.world);
                        (sx, sy, c.camera.zoom, c.line_h)
                    })
                });
                let Some((sx, sy, zoom, line_h)) = geom else {
                    return false;
                };
                let (new_w, new_rows) = match zone {
                    CardZone::ResizeRight => {
                        (Some(resize_width_world(sx, cursor.0, zoom)), None)
                    }
                    CardZone::ResizeBottom => {
                        (None, Some(resize_height_rows(sy, cursor.1, zoom, line_h)))
                    }
                    CardZone::ResizeCorner => (
                        Some(resize_width_world(sx, cursor.0, zoom)),
                        Some(resize_height_rows(sy, cursor.1, zoom, line_h)),
                    ),
                    CardZone::Body => (None, None),
                };
                self.app_state.canvas_pointer_resize_block(id, new_w, new_rows)
            }
            _ => false,
```

(Keep the existing `PanelEdge` arm above it.)

- [ ] **Step 4: Build and manually verify**

Run: `cargo build`
Expected: compiles clean.

Run: `cargo run`, open a file, press `F8` to open the canvas, then verify:
- Press-and-drag a card body → the card follows the cursor (1:1 at zoom 1; correct at other zooms).
- Drag a card's right edge → width changes (clamped to half..2.5× the base width).
- Drag a card's bottom edge → height changes by whole rows (min 3 rows).
- Drag the bottom-right corner → both change together.
- A quick click on a card (no movement) → it does NOT jump (the absolute-move math keeps it put within a couple px; see Task 6 dead-zone note).
- Drag a pinned card → it still moves.
- Enter card edit mode (`Enter`), then try dragging → cards do NOT move (only `Navigate` allows it).

- [ ] **Step 5: Commit**

```bash
git add src/app/event_loop/application.rs
git commit -m "feat(canvas): drag to move cards and drag edges/corner to resize"
```

---

## Task 6: Hover affordance — cursor shape + handle highlight

**Files:**
- Modify: `src/app/event_loop/application.rs` (hover hit-test + `set_cursor` + store highlight rect)
- Modify: `src/render/renderer/canvas.rs` (draw the highlight quad) and/or the overlay pass
- Test: manual GUI verification.

**Interfaces:**
- Consumes: the same hit-test helpers from Tasks 1–2; `self.window`; `winit::window::CursorIcon`; the renderer's existing colored-quad path (`region_pipeline`).
- Produces: `AppShell` method `update_hover_affordance(cursor)`; a renderer setter `set_pointer_hover_highlight(rect: Option<[f32; 4]>)` (or equivalent) consumed in the overlay pass.

- [ ] **Step 1: Hover hit-test + cursor shape (no highlight yet)**

In `src/app/event_loop/application.rs`, add a method near the drag helpers:

```rust
    /// While not dragging, hit-test the cursor and update the OS cursor shape +
    /// the stored hover target (used for the highlight in the renderer). Returns
    /// whether the hover target changed (so the caller can redraw the highlight).
    fn update_hover_affordance(&mut self, cursor: (f32, f32)) -> bool {
        use crate::workbench::pointer_drag::{
            resolve_press_target, splitter_hit_test, DragTarget, HoverTarget, PanelSide,
            SPLITTER_BAND_PX,
        };
        use winit::window::CursorIcon;

        let card_hit = if self.app_state.canvas_is_navigating() {
            self.app_state.canvas().and_then(|c| {
                crate::canvas::interaction::card_pointer_hit_test(&c.blocks, &c.camera, cursor)
            })
        } else {
            None
        };
        let left = self.panel_state.left.visible.then(|| self.current_left_sidebar_bounds()).flatten();
        let right = self.panel_state.right.visible.then(|| self.current_right_sidebar_bounds()).flatten();
        let bottom = self.panel_state.bottom.visible.then(|| self.current_bottom_panel_bounds()).flatten();
        let splitter_hit = splitter_hit_test(left, right, bottom, SPLITTER_BAND_PX, cursor);

        let target = resolve_press_target(self.app_state.canvas_is_navigating(), card_hit, splitter_hit);
        let hover = target.map(|t| match t {
            DragTarget::PanelEdge(side) => HoverTarget::PanelEdge(side),
            DragTarget::CardMove(_) => HoverTarget::CardBody,
            DragTarget::CardResize(_, zone) => HoverTarget::CardResize(zone),
        });

        let icon = match hover {
            Some(HoverTarget::PanelEdge(PanelSide::Left | PanelSide::Right)) => CursorIcon::EwResize,
            Some(HoverTarget::PanelEdge(PanelSide::Bottom)) => CursorIcon::NsResize,
            Some(HoverTarget::CardBody) => CursorIcon::Move,
            Some(HoverTarget::CardResize(zone)) => match zone {
                crate::canvas::interaction::CardZone::ResizeRight => CursorIcon::EwResize,
                crate::canvas::interaction::CardZone::ResizeBottom => CursorIcon::NsResize,
                crate::canvas::interaction::CardZone::ResizeCorner => CursorIcon::NwseResize,
                crate::canvas::interaction::CardZone::Body => CursorIcon::Default,
            },
            None => CursorIcon::Default,
        };
        if let Some(w) = self.window.as_ref() {
            w.set_cursor(icon);
        }
        let changed = self.hover_target != hover;
        self.hover_target = hover;
        changed
    }
```

- [ ] **Step 2: Call it from `CursorMoved` when not dragging**

Extend the `CursorMoved` arm so hover updates when no drag is active:

```rust
            WindowEvent::CursorMoved { position, .. } => {
                let cursor = (position.x as f32, position.y as f32);
                self.last_cursor_position = Some(cursor);
                if self.active_drag.is_some() {
                    if self.update_pointer_drag(cursor) {
                        self.request_redraw();
                    }
                } else if self.update_hover_affordance(cursor) {
                    self.request_redraw();
                }
            }
```

- [ ] **Step 3: Build and verify cursor shapes**

Run: `cargo build` then `cargo run`. Hover over each dock inner edge → the
cursor becomes the horizontal/vertical resize arrow. On the canvas, hover a card
body → move cursor; hover its right/bottom edge/corner → the matching resize
arrow. Move away → cursor returns to default.

- [ ] **Step 4: Add the highlight rectangle (renderer)**

Compute the highlight rect in screen pixels in `update_hover_affordance` and
store it on the renderer for the overlay pass. After computing `hover`, derive a
thin rect:

```rust
        // A thin accent rect over the hovered handle/edge, for the renderer.
        let highlight: Option<[f32; 4]> = match hover {
            Some(HoverTarget::PanelEdge(side)) => {
                let band = SPLITTER_BAND_PX;
                match side {
                    PanelSide::Left => left.map(|[x, y, w, h]| [x + w - band, y, band * 2.0, h]),
                    PanelSide::Right => right.map(|[x, y, _w, h]| [x - band, y, band * 2.0, h]),
                    PanelSide::Bottom => bottom.map(|[x, y, w, _h]| [x, y - band, w, band * 2.0]),
                }
            }
            // Card highlights are drawn by the canvas renderer from `hover_target`
            // + the focused card rect; keep panel-only here for simplicity.
            _ => None,
        };
        if let Some(r) = self.renderer.as_mut() {
            r.set_pointer_hover_highlight(highlight);
        }
```

Add the setter + a drawn quad in the renderer. In `src/render/renderer.rs` (the
`Renderer` struct) add a field `pointer_hover_highlight: Option<[f32; 4]>` (init
`None` in the constructor), and a setter:

```rust
    pub fn set_pointer_hover_highlight(&mut self, rect: Option<[f32; 4]>) {
        self.pointer_hover_highlight = rect;
    }
```

Then, in the overlay portion of the frame (follow how an existing overlay pushes
a colored quad through `region_pipeline` — e.g. the leap dim or selection band),
push one quad for `self.pointer_hover_highlight` using the theme accent color at
low alpha. Keep it last so it sits on top.

> **Implementer note:** match the repo's existing region-quad helper signature
> (color type, NDC vs pixel rect). Read one existing quad push in
> `src/render/renderer/lifecycle/frame.rs` or `selections.rs` and mirror it. Do
> not invent a new pipeline — reuse `region_pipeline`.

- [ ] **Step 5: Build and verify the highlight**

Run: `cargo build` then `cargo run`. Hover a dock inner edge → a thin accent
band appears under the resize cursor and tracks the edge. Move away → it clears.

- [ ] **Step 6: Run the full test suite**

Run: `cargo test`
Expected: PASS (all existing tests + the new pure tests from Tasks 1–3).

- [ ] **Step 7: Commit**

```bash
git add src/app/event_loop/application.rs src/render/renderer.rs src/render/renderer/canvas.rs
git commit -m "feat(workbench): hover cursor shapes + handle highlight for drag zones"
```

---

## Manual GUI Verification Checklist (final)

Run `cargo run` and confirm every item:

- [ ] Resize the left dock by dragging its right edge; min/max clamps hold.
- [ ] Resize the right dock by dragging its left edge.
- [ ] Resize the editor/terminal split by dragging the bottom dock's top edge.
- [ ] Dock tab clicks still switch tabs (no regression).
- [ ] `F8` canvas: drag a card body to reposition it (correct at zoom in/out).
- [ ] Resize a card by its right edge (width), bottom edge (height), and corner (both).
- [ ] Click-to-focus a card does NOT noticeably move it.
- [ ] A pinned card still moves under a mouse drag.
- [ ] In card-edit mode, canvas drags do nothing.
- [ ] Cursor shape changes on every hover zone (EW / NS / NWSE / Move) and resets to default off-zone.
- [ ] The hover highlight band appears on dock edges and clears when leaving.

---

## Self-Review Notes (author)

- **Spec coverage:** §3 architecture → Tasks 1–2 (pure modules) + Task 4 (wiring). §4 panel resize → Task 4. §5 card move → Tasks 3+5. §6 card resize → Tasks 3+5. §7 hover affordance → Task 6. §9 testing → unit tests in Tasks 1–3 + manual checklists in 4–6. §10 out-of-scope items are intentionally absent.
- **No new model field** (corrected from an earlier spec draft): width/height already live in `block.world`; Task 3 only adds absolute-value setters reusing the existing clamps.
- **Type consistency:** `CardZone` defined in Task 1 is used unchanged in Tasks 2/5/6; `DragTarget`/`DragAnchor`/`HoverTarget`/`ActiveDrag` defined in Task 2 are used unchanged in Tasks 4–6; `canvas_pointer_move_block` / `canvas_pointer_resize_block` / `canvas_is_navigating` signatures defined in Task 3 match their call sites in Tasks 4–6.
