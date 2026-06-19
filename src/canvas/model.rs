//! Core data model for the spatial canvas: blocks, the camera (world↔screen
//! mapping + zoom), and the aggregate `CanvasState`. All pure — no GPU/IO.

use std::path::PathBuf;

use super::navigation::{self, Dir};

/// Stable identifier for a block within one canvas session.
pub type BlockId = u64;

/// How a block relates to the focal symbol it was spawned from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlockRelation {
    /// The symbol the user opened the canvas on (or the last `Enter` target).
    Focal,
    /// `textDocument/definition` of the focal symbol.
    Definition,
    /// A site that calls the focal symbol (incoming / reference).
    Caller,
    /// A symbol the focal symbol calls (outgoing).
    Callee,
}

/// Enough provenance to (Phase B) re-open the block as a live document and to
/// re-read its snapshot text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlockOrigin {
    pub path: PathBuf,
    /// Byte range in `path` that the snapshot covers.
    pub start_byte: usize,
    pub end_byte: usize,
    pub symbol_name: String,
    /// LSP position (0-indexed) the block was sourced at — used to spawn
    /// further relations without re-resolving the symbol.
    pub lsp_line: u32,
    pub lsp_character: u32,
}

/// Read-only text shown inside a block (Phase A). Highlight spans are computed
/// at render time from the file's syntax, so the core model stays pure.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct BlockSnapshot {
    pub title: String,
    pub text: String,
}

/// A single code block positioned on the infinite plane (world coordinates).
#[derive(Debug, Clone, PartialEq)]
pub struct CanvasBlock {
    pub id: BlockId,
    pub relation: BlockRelation,
    pub origin: BlockOrigin,
    pub snapshot: BlockSnapshot,
    pub world: WorldRect,
}

/// An axis-aligned rectangle in world space.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WorldRect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl WorldRect {
    pub fn new(x: f32, y: f32, w: f32, h: f32) -> Self {
        Self { x, y, w, h }
    }

    pub fn center(&self) -> (f32, f32) {
        (self.x + self.w * 0.5, self.y + self.h * 0.5)
    }

    /// True when the two rectangles overlap (touching edges do not count).
    pub fn intersects(&self, other: &WorldRect) -> bool {
        self.x < other.x + other.w
            && other.x < self.x + self.w
            && self.y < other.y + other.h
            && other.y < self.y + self.h
    }
}

pub const MIN_ZOOM: f32 = 0.2;
pub const MAX_ZOOM: f32 = 5.0;

/// Maps the infinite world plane onto the screen. `screen = (world - offset) *
/// zoom`. `offset` is in world units; `zoom` is a uniform scale.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Camera {
    pub offset_x: f32,
    pub offset_y: f32,
    pub zoom: f32,
}

impl Default for Camera {
    fn default() -> Self {
        Self {
            offset_x: 0.0,
            offset_y: 0.0,
            zoom: 1.0,
        }
    }
}

impl Camera {
    pub fn world_to_screen_point(&self, wx: f32, wy: f32) -> (f32, f32) {
        ((wx - self.offset_x) * self.zoom, (wy - self.offset_y) * self.zoom)
    }

    pub fn screen_to_world_point(&self, sx: f32, sy: f32) -> (f32, f32) {
        (sx / self.zoom + self.offset_x, sy / self.zoom + self.offset_y)
    }

    /// Project a world rect to a screen rect `[x, y, w, h]` (pixels).
    pub fn world_to_screen(&self, r: WorldRect) -> [f32; 4] {
        let (sx, sy) = self.world_to_screen_point(r.x, r.y);
        [sx, sy, r.w * self.zoom, r.h * self.zoom]
    }

    /// Pan by a screen-pixel delta (content appears to move by `(dx, dy)`).
    pub fn pan(&mut self, dx: f32, dy: f32) {
        self.offset_x -= dx / self.zoom;
        self.offset_y -= dy / self.zoom;
    }

    /// Multiply the zoom by `factor`, keeping the world point currently under
    /// the screen anchor `(sx, sy)` fixed on screen. Clamped to `[MIN, MAX]`.
    pub fn zoom_about(&mut self, sx: f32, sy: f32, factor: f32) {
        let (wx, wy) = self.screen_to_world_point(sx, sy);
        let new_zoom = (self.zoom * factor).clamp(MIN_ZOOM, MAX_ZOOM);
        // Solve offset so (w - offset)*new_zoom == screen anchor.
        self.offset_x = wx - sx / new_zoom;
        self.offset_y = wy - sy / new_zoom;
        self.zoom = new_zoom;
    }
}

/// The full state of one canvas session.
#[derive(Debug, Clone, Default)]
pub struct CanvasState {
    pub blocks: Vec<CanvasBlock>,
    pub focused: Option<BlockId>,
    pub camera: Camera,
    /// World size used for every block this session (set at open from the
    /// viewport so cards are a sensible fraction of the screen at any DPI).
    pub block_w: f32,
    pub block_h: f32,
    next_id: BlockId,
}

impl CanvasState {
    pub fn new() -> Self {
        Self::default()
    }

    /// Allocate the next unique block id.
    pub fn alloc_id(&mut self) -> BlockId {
        let id = self.next_id;
        self.next_id += 1;
        id
    }

    /// Insert a block whose `id` was obtained from [`alloc_id`]. The first block
    /// inserted becomes the focused block.
    pub fn push(&mut self, block: CanvasBlock) -> BlockId {
        let id = block.id;
        if self.focused.is_none() {
            self.focused = Some(id);
        }
        self.blocks.push(block);
        id
    }

    pub fn focused_block(&self) -> Option<&CanvasBlock> {
        let id = self.focused?;
        self.blocks.iter().find(|b| b.id == id)
    }

    pub fn block(&self, id: BlockId) -> Option<&CanvasBlock> {
        self.blocks.iter().find(|b| b.id == id)
    }

    /// Set focus to `id` if it exists. Returns whether focus changed.
    pub fn focus(&mut self, id: BlockId) -> bool {
        if self.focused == Some(id) || !self.blocks.iter().any(|b| b.id == id) {
            return false;
        }
        self.focused = Some(id);
        true
    }

    /// Move focus to the nearest block in `dir` from the focused block.
    pub fn focus_direction(&mut self, dir: Dir) -> bool {
        let Some(from_id) = self.focused else {
            return false;
        };
        let Some(from) = self.block(from_id).map(|b| b.world.center()) else {
            return false;
        };
        let centers: Vec<(BlockId, (f32, f32))> =
            self.blocks.iter().map(|b| (b.id, b.world.center())).collect();
        match navigation::nearest_in_direction(&centers, from, from_id, dir) {
            Some(target) => self.focus(target),
            None => false,
        }
    }

    /// Cycle focus through blocks in insertion order.
    pub fn focus_cycle(&mut self, forward: bool) -> bool {
        if self.blocks.is_empty() {
            return false;
        }
        let cur = self
            .focused
            .and_then(|id| self.blocks.iter().position(|b| b.id == id))
            .unwrap_or(0);
        let len = self.blocks.len();
        let next = if forward {
            (cur + 1) % len
        } else {
            (cur + len - 1) % len
        };
        let target = self.blocks[next].id;
        self.focus(target)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn block(id: BlockId, rel: BlockRelation, rect: WorldRect) -> CanvasBlock {
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
        }
    }

    #[test]
    fn camera_world_screen_round_trip() {
        let cam = Camera {
            offset_x: 12.0,
            offset_y: -7.0,
            zoom: 1.8,
        };
        let (sx, sy) = cam.world_to_screen_point(40.0, 25.0);
        let (wx, wy) = cam.screen_to_world_point(sx, sy);
        assert!((wx - 40.0).abs() < 1e-3);
        assert!((wy - 25.0).abs() < 1e-3);
    }

    #[test]
    fn camera_zoom_about_keeps_anchor_fixed() {
        let mut cam = Camera::default();
        // World point under the anchor before zoom.
        let before = cam.screen_to_world_point(300.0, 200.0);
        cam.zoom_about(300.0, 200.0, 2.0);
        let after = cam.screen_to_world_point(300.0, 200.0);
        assert!((cam.zoom - 2.0).abs() < 1e-6);
        assert!((before.0 - after.0).abs() < 1e-3);
        assert!((before.1 - after.1).abs() < 1e-3);
    }

    #[test]
    fn camera_zoom_clamps() {
        let mut cam = Camera::default();
        cam.zoom_about(0.0, 0.0, 100.0);
        assert_eq!(cam.zoom, MAX_ZOOM);
        cam.zoom_about(0.0, 0.0, 0.0001);
        assert_eq!(cam.zoom, MIN_ZOOM);
    }

    #[test]
    fn camera_pan_shifts_world_point_by_screen_delta() {
        let mut cam = Camera::default();
        let (sx0, _) = cam.world_to_screen_point(100.0, 0.0);
        cam.pan(30.0, 0.0);
        let (sx1, _) = cam.world_to_screen_point(100.0, 0.0);
        assert!((sx1 - (sx0 + 30.0)).abs() < 1e-3);
    }

    #[test]
    fn first_pushed_block_takes_focus() {
        let mut st = CanvasState::new();
        let id0 = st.alloc_id();
        st.push(block(id0, BlockRelation::Focal, WorldRect::new(0.0, 0.0, 10.0, 10.0)));
        let id1 = st.alloc_id();
        st.push(block(id1, BlockRelation::Caller, WorldRect::new(50.0, 0.0, 10.0, 10.0)));
        assert_eq!(st.focused, Some(id0));
    }

    #[test]
    fn focus_direction_moves_to_nearest_block() {
        let mut st = CanvasState::new();
        let focal = st.alloc_id();
        st.push(block(focal, BlockRelation::Focal, WorldRect::new(0.0, 0.0, 20.0, 20.0)));
        let right = st.alloc_id();
        st.push(block(right, BlockRelation::Callee, WorldRect::new(100.0, 0.0, 20.0, 20.0)));
        let left = st.alloc_id();
        st.push(block(left, BlockRelation::Caller, WorldRect::new(-100.0, 0.0, 20.0, 20.0)));
        assert!(st.focus_direction(Dir::Right));
        assert_eq!(st.focused, Some(right));
        assert!(st.focus_direction(Dir::Left));
        assert_eq!(st.focused, Some(focal));
    }

    #[test]
    fn focus_cycle_wraps() {
        let mut st = CanvasState::new();
        let a = st.alloc_id();
        st.push(block(a, BlockRelation::Focal, WorldRect::new(0.0, 0.0, 10.0, 10.0)));
        let b = st.alloc_id();
        st.push(block(b, BlockRelation::Caller, WorldRect::new(40.0, 0.0, 10.0, 10.0)));
        assert!(st.focus_cycle(true));
        assert_eq!(st.focused, Some(b));
        assert!(st.focus_cycle(true)); // wraps back to a
        assert_eq!(st.focused, Some(a));
    }
}
