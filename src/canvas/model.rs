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

/// A syntax-highlight span over `BlockSnapshot.text` (byte offsets). Mirrors the
/// renderer's `StyledTextSpan` but keeps the canvas core decoupled from the text
/// layer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CanvasSpan {
    pub start: usize,
    pub end: usize,
    pub color: [u8; 4],
    pub bold: bool,
    pub italic: bool,
}

/// Read-only code shown inside a card. `text` is the raw snippet (newline-
/// separated, no gutter); `start_line` (1-based) drives the line-number gutter;
/// `spans` carry syntax colors (computed at sourcing time).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct BlockSnapshot {
    pub title: String,
    pub symbol: String,
    pub start_line: u32,
    pub text: String,
    pub spans: Vec<CanvasSpan>,
}

/// A single code block positioned on the infinite plane (world coordinates).
#[derive(Debug, Clone, PartialEq)]
pub struct CanvasBlock {
    pub id: BlockId,
    pub relation: BlockRelation,
    pub origin: BlockOrigin,
    pub snapshot: BlockSnapshot,
    pub world: WorldRect,
    /// Pinned cards are immovable by `hjkl` (move-focused-card) — they stay put
    /// while other cards are rearranged.
    pub pinned: bool,
    /// Lines of context this card reads above/below its origin line. `+`/`-`
    /// adjust ONLY the focused card (re-sourced in place, top-left fixed).
    /// Seeded from the session default at spawn; clamped by the app layer.
    pub context_lines: usize,
    /// The card this one was spawned from via in-card `gd`/`gr`, if any. `None`
    /// when spawned from the focal symbol (the connector then starts at the
    /// editor caret); `Some` makes the connector run from the parent card.
    pub parent: Option<BlockId>,
    /// The enclosing-definition line range (0-based, inclusive) this card is
    /// scoped to, once resolved. The in-card edit viewport scrolls ONLY within
    /// this range (it never pulls in lines outside the function/scope). `None`
    /// before scope resolution → the edit window falls back to whole-file scroll.
    pub scope_lines: Option<(u32, u32)>,
    /// User-chosen visible-row count for this card (the `=`/`-` HEIGHT control).
    /// `None` = auto-size to the snapshot/scope (capped at `CARD_MAX_LINES`);
    /// `Some(n)` pins the box to `n` rows and caps the in-card edit viewport so
    /// editing scrolls within those rows. Pure visual resize — never re-sources
    /// extra file context (the card already shows only its scope).
    pub height_rows: Option<usize>,
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

/// Card title-bar height as a multiple of the code line height. The renderer
/// MUST use the same factor for the card's tab header so card sizing (here) and
/// drawing (renderer) agree — otherwise capacity math drifts. Kept compact (~1.5)
/// so the header hugs its single row of text without a tall empty band.
pub const CARD_HEADER_LINES: f32 = 1.5;
/// Breathing room below the last code line, in line-heights. Kept ≥ the
/// renderer's body margin so a card sized to N lines never clips its Nth line.
pub const CARD_BOTTOM_LINES: f32 = 1.0;
/// A relation card shows between `CARD_MIN_LINES` and `CARD_MAX_LINES` lines of
/// code; its height tracks the snapshot's actual line count within these bounds
/// so short snippets don't leave a tall empty gap and `+` (more context) grows
/// the card to fit the extra code. `CARD_MAX_LINES` is the plateau beyond which
/// the renderer shows a "+N more" marker instead of an ever-taller card.
pub const CARD_MIN_LINES: usize = 3;
pub const CARD_MAX_LINES: usize = 30;

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

/// Phase B interaction sub-state of an open canvas. Drives input routing,
/// rendering, and the status pill. `Navigate` is the Phase A behaviour.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CanvasInteraction {
    /// S1: hjkl move the focused card; Enter edits it; Esc pushes to background.
    #[default]
    Navigate,
    /// S2: a card is promoted to a live mini-editor. The active editor buffer is
    /// the card's file; `cursor_line`/`cursor_col` mirror the live cursor so the
    /// renderer can draw the caret and scroll the card to follow it.
    EditCard {
        block: BlockId,
        cursor_line: u32,
        cursor_col: u32,
    },
    /// S3: canvas pushed to the background — the editor regains focus and cards
    /// float dimmed (no scrim, no hints); a second Esc closes the canvas.
    Background,
}

/// The live visual selection inside the edit-target card (S2), mirrored from the
/// `CanvasEditSession` so the renderer can paint selection bands like the main
/// editor. Lines are 0-based **absolute** file lines; `end_col` is exclusive on
/// `end_line`. `line_mode` paints the full width of every covered row.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CardSelection {
    pub start_line: u32,
    pub start_col: u32,
    pub end_line: u32,
    pub end_col: u32,
    pub line_mode: bool,
}

/// The full state of one canvas session.
#[derive(Debug, Clone, Default)]
pub struct CanvasState {
    pub blocks: Vec<CanvasBlock>,
    pub focused: Option<BlockId>,
    pub camera: Camera,
    /// World width used for every card this session, and the *maximum* card
    /// height (a full `CARD_MAX_LINES` card, also the focal anchor's height).
    /// Set at open from the editor font so cards fit code at zoom 1.
    pub block_w: f32,
    pub block_h: f32,
    /// Code line height in world units (editor `font_size * 1.4` at zoom 1).
    /// Drives per-card height via [`CanvasState::card_height`].
    pub line_h: f32,
    /// Lines of context read above/below the focal line in each relation card
    /// (`+`/`-` adjust it). A larger value re-reads MORE code from the file and
    /// the card grows to fit — it never scales the card box or the font. Set at
    /// open from the canvas default; clamped by the app layer.
    pub context_lines: usize,
    /// Phase B interaction sub-state (Navigate / EditCard / Background).
    pub interaction: CanvasInteraction,
    /// Set once the user hand-arranges the layout (moves or pins a card). While
    /// `false`, newly-spawned relation cards are auto-arranged to fit the
    /// viewport; once `true`, auto-arrange is frozen (manual placement wins).
    pub user_arranged: bool,
    /// `K` hover doc lines for the card being edited, anchored at its caret
    /// (Phase 2 in-card LSP). Completion is rendered separately from the app
    /// layer's `CompletionState`. `None` clears.
    pub card_hover: Option<Vec<String>>,
    /// Stashed (hidden) while the user opened a card as a buffer tab (`o`): the
    /// state is kept but NOT rendered; `gc` un-stashes it. Distinct from
    /// `Background` (which floats dimmed cards over the editor).
    pub stashed: bool,
    /// Live visual selection in the edit-target card (S2), refreshed each frame
    /// from the edit session; `None` outside Visual mode / edit. Renderer-only.
    pub edit_selection: Option<CardSelection>,
    next_id: BlockId,
}

impl CanvasState {
    pub fn new() -> Self {
        Self::default()
    }

    /// World height of a card showing `line_count` lines of code, clamped to
    /// `[CARD_MIN_LINES, CARD_MAX_LINES]`. Derived from `line_h` so the card
    /// hugs its content at zoom 1 (no tall empty gap below short snippets). This
    /// is the AUTO size (spawn / scope-resolve); the `CARD_MAX_LINES` plateau
    /// stops a freshly-sourced card from being enormous.
    pub fn card_height(&self, line_count: usize) -> f32 {
        self.card_height_exact(line_count.min(CARD_MAX_LINES))
    }

    /// World height for an EXACT visible-row count, floored at `CARD_MIN_LINES`
    /// but NOT capped at `CARD_MAX_LINES`. Used when the user grows a card with
    /// `=`/`-` (height_rows) to reveal its WHOLE scope (e.g. a 50-line function),
    /// past the auto plateau. The caller bounds `rows` to the available content.
    pub fn card_height_exact(&self, rows: usize) -> f32 {
        let n = rows.max(CARD_MIN_LINES) as f32;
        self.line_h * (CARD_HEADER_LINES + n + CARD_BOTTOM_LINES)
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
        // Relation cards only — the Focal block is the editor anchor, never drawn as
        // a card, so Tab must not land on it (that reads as a "focus off" step).
        let cards: Vec<BlockId> = self
            .blocks
            .iter()
            .filter(|b| b.relation != BlockRelation::Focal)
            .map(|b| b.id)
            .collect();
        if cards.is_empty() {
            return false;
        }
        let cur = self
            .focused
            .and_then(|id| cards.iter().position(|&c| c == id))
            .unwrap_or(0);
        let len = cards.len();
        let next = if forward {
            (cur + 1) % len
        } else {
            (cur + len - 1) % len
        };
        self.focus(cards[next])
    }

    /// Toggle the focused block's pinned state. Returns whether a block toggled.
    pub fn toggle_pin(&mut self) -> bool {
        let Some(id) = self.focused else {
            return false;
        };
        match self.blocks.iter_mut().find(|b| b.id == id) {
            Some(b) => {
                b.pinned = !b.pinned;
                // Pinning is a manual layout choice — freeze auto-arrange.
                self.user_arranged = true;
                true
            }
            None => false,
        }
    }

    /// Move the focused block by `(dx, dy)` world units. No-op (false) when the
    /// focused block is pinned or absent.
    pub fn move_focused(&mut self, dx: f32, dy: f32) -> bool {
        let Some(id) = self.focused else {
            return false;
        };
        match self.blocks.iter_mut().find(|b| b.id == id) {
            Some(b) if !b.pinned => {
                b.world.x += dx;
                b.world.y += dy;
                // The user took manual control of the layout — stop auto-arranging.
                self.user_arranged = true;
                true
            }
            _ => false,
        }
    }

    /// Close (remove) the focused block unless it's the `Focal` anchor. Focus
    /// moves to a remaining relation block (or None). Returns whether removed.
    pub fn close_focused(&mut self) -> bool {
        let Some(id) = self.focused else {
            return false;
        };
        let Some(idx) = self.blocks.iter().position(|b| b.id == id) else {
            return false;
        };
        if self.blocks[idx].relation == BlockRelation::Focal {
            return false;
        }
        self.blocks.remove(idx);
        self.focused = self
            .blocks
            .iter()
            .find(|b| b.relation != BlockRelation::Focal)
            .map(|b| b.id);
        true
    }

    // ── Phase B: interaction state machine ───────────────────────────────────

    /// S1→S2: begin editing the focused relation card. Returns its [`BlockOrigin`]
    /// so the caller can switch the active buffer to that file and position the
    /// cursor. Returns `None` (and stays in Navigate) when no block is focused or
    /// the focused block is the `Focal` anchor — the live editor *is* the focal,
    /// so there is nothing to promote into an editable card.
    pub fn begin_edit(&mut self) -> Option<BlockOrigin> {
        let id = self.focused?;
        let block = self.blocks.iter().find(|b| b.id == id)?;
        if block.relation == BlockRelation::Focal {
            return None;
        }
        let origin = block.origin.clone();
        self.interaction = CanvasInteraction::EditCard {
            block: id,
            cursor_line: origin.lsp_line,
            cursor_col: origin.lsp_character,
        };
        Some(origin)
    }

    /// S2→S1: leave edit mode back to Navigate. Returns whether it was editing.
    pub fn end_edit(&mut self) -> bool {
        if matches!(self.interaction, CanvasInteraction::EditCard { .. }) {
            self.interaction = CanvasInteraction::Navigate;
            self.edit_selection = None;
            true
        } else {
            false
        }
    }

    /// S1→S3: push the canvas to the background (editor regains focus). No-op
    /// (false) unless currently navigating.
    pub fn enter_background(&mut self) -> bool {
        if self.interaction == CanvasInteraction::Navigate {
            self.interaction = CanvasInteraction::Background;
            true
        } else {
            false
        }
    }

    /// S3→S1: re-grab the canvas from the background. No-op (false) unless
    /// currently in the background (never force-exits an edit).
    pub fn focus_navigate(&mut self) -> bool {
        if self.interaction == CanvasInteraction::Background {
            self.interaction = CanvasInteraction::Navigate;
            true
        } else {
            false
        }
    }

    /// The block currently being edited (S2), if any.
    pub fn editing_block(&self) -> Option<BlockId> {
        match self.interaction {
            CanvasInteraction::EditCard { block, .. } => Some(block),
            _ => None,
        }
    }

    /// True when the canvas is in the background (S3).
    pub fn is_background(&self) -> bool {
        self.interaction == CanvasInteraction::Background
    }

    /// Update the live cursor shown in the edit card (S2). No-op otherwise.
    pub fn set_edit_cursor(&mut self, line: u32, col: u32) {
        if let CanvasInteraction::EditCard {
            cursor_line,
            cursor_col,
            ..
        } = &mut self.interaction
        {
            *cursor_line = line;
            *cursor_col = col;
        }
    }

    /// Set (or clear) the live visual selection painted on the edit card (S2).
    pub fn set_edit_selection(&mut self, sel: Option<CardSelection>) {
        self.edit_selection = sel;
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
            pinned: false,
            context_lines: 8,
            parent: None,
            scope_lines: None,
            height_rows: None,
            // (test helper)
        }
    }

    #[test]
    fn toggle_pin_blocks_move_focused() {
        let mut st = CanvasState::new();
        let id = st.alloc_id();
        st.push(block(id, BlockRelation::Caller, WorldRect::new(10.0, 10.0, 5.0, 5.0)));
        // Unpinned: move works.
        assert!(st.move_focused(4.0, -2.0));
        assert_eq!(st.block(id).unwrap().world.x, 14.0);
        assert_eq!(st.block(id).unwrap().world.y, 8.0);
        // Pinned: move is blocked.
        assert!(st.toggle_pin());
        assert!(st.block(id).unwrap().pinned);
        assert!(!st.move_focused(4.0, 0.0));
        assert_eq!(st.block(id).unwrap().world.x, 14.0);
        // Unpinned again: move works.
        assert!(st.toggle_pin());
        assert!(st.move_focused(1.0, 0.0));
        assert_eq!(st.block(id).unwrap().world.x, 15.0);
    }

    #[test]
    fn close_focused_removes_relation_and_refocuses_but_not_focal() {
        let mut st = CanvasState::new();
        let focal = st.alloc_id();
        st.push(block(focal, BlockRelation::Focal, WorldRect::new(0.0, 0.0, 5.0, 5.0)));
        let a = st.alloc_id();
        st.push(block(a, BlockRelation::Caller, WorldRect::new(20.0, 0.0, 5.0, 5.0)));
        let b = st.alloc_id();
        st.push(block(b, BlockRelation::Caller, WorldRect::new(20.0, 20.0, 5.0, 5.0)));
        st.focus(a);
        assert!(st.close_focused());
        assert!(st.block(a).is_none());
        assert_eq!(st.focused, Some(b)); // refocused to the remaining relation
        // The focal anchor is not closeable.
        st.focus(focal);
        assert!(!st.close_focused());
        assert_eq!(st.blocks.len(), 2);
    }

    #[test]
    fn interaction_state_machine_transitions() {
        let mut st = CanvasState::new();
        let focal = st.alloc_id();
        st.push(block(focal, BlockRelation::Focal, WorldRect::new(0.0, 0.0, 5.0, 5.0)));
        let a = st.alloc_id();
        st.push(block(a, BlockRelation::Caller, WorldRect::new(20.0, 0.0, 5.0, 5.0)));
        // Default is Navigate.
        assert_eq!(st.interaction, CanvasInteraction::Navigate);
        // S1 → S3 background and back; idempotent at each end.
        assert!(st.enter_background());
        assert!(st.is_background());
        assert!(!st.enter_background());
        assert!(st.focus_navigate());
        assert!(!st.is_background());
        assert!(!st.focus_navigate());
        // S1 → S2: edit the focused relation card.
        st.focus(a);
        let origin = st.begin_edit().expect("a relation card is editable");
        assert_eq!(st.editing_block(), Some(a));
        assert_eq!(origin.lsp_line, 0);
        // The live cursor can be updated while editing.
        st.set_edit_cursor(12, 4);
        match st.interaction {
            CanvasInteraction::EditCard {
                cursor_line,
                cursor_col,
                ..
            } => assert_eq!((cursor_line, cursor_col), (12, 4)),
            other => panic!("expected EditCard, got {other:?}"),
        }
        // S2 → S1: exit; idempotent.
        assert!(st.end_edit());
        assert_eq!(st.interaction, CanvasInteraction::Navigate);
        assert!(!st.end_edit());
    }

    #[test]
    fn cannot_edit_the_focal_anchor() {
        let mut st = CanvasState::new();
        let focal = st.alloc_id();
        st.push(block(focal, BlockRelation::Focal, WorldRect::new(0.0, 0.0, 5.0, 5.0)));
        st.focus(focal);
        assert!(st.begin_edit().is_none());
        assert_eq!(st.interaction, CanvasInteraction::Navigate);
    }

    #[test]
    fn focus_navigate_does_not_force_exit_an_edit() {
        let mut st = CanvasState::new();
        let focal = st.alloc_id();
        st.push(block(focal, BlockRelation::Focal, WorldRect::new(0.0, 0.0, 5.0, 5.0)));
        let a = st.alloc_id();
        st.push(block(a, BlockRelation::Caller, WorldRect::new(20.0, 0.0, 5.0, 5.0)));
        st.focus(a);
        st.begin_edit().unwrap();
        // gc (focus_navigate) must NOT yank you out of an active edit.
        assert!(!st.focus_navigate());
        assert_eq!(st.editing_block(), Some(a));
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
        let focal = st.alloc_id();
        st.push(block(focal, BlockRelation::Focal, WorldRect::new(0.0, 0.0, 10.0, 10.0)));
        let a = st.alloc_id();
        st.push(block(a, BlockRelation::Caller, WorldRect::new(40.0, 0.0, 10.0, 10.0)));
        let b = st.alloc_id();
        st.push(block(b, BlockRelation::Callee, WorldRect::new(80.0, 0.0, 10.0, 10.0)));
        st.focus(a);
        assert!(st.focus_cycle(true));
        assert_eq!(st.focused, Some(b));
        assert!(st.focus_cycle(true)); // wraps back to a, NEVER focal
        assert_eq!(st.focused, Some(a));
        assert_ne!(st.focused, Some(focal));
    }

    #[test]
    fn focus_cycle_skips_focal_block() {
        let mut st = CanvasState::default();
        let focal = st.push(block(1, BlockRelation::Focal, WorldRect::new(0.0, 0.0, 20.0, 20.0)));
        let c1 = st.push(block(2, BlockRelation::Caller, WorldRect::new(100.0, 0.0, 20.0, 20.0)));
        let c2 = st.push(block(3, BlockRelation::Callee, WorldRect::new(100.0, 40.0, 20.0, 20.0)));

        st.focus(c1);
        assert!(st.focus_cycle(true));
        assert_eq!(st.focused, Some(c2));
        // Forward again wraps straight to c1 — NEVER the focal anchor.
        assert!(st.focus_cycle(true));
        assert_eq!(st.focused, Some(c1));
        assert_ne!(st.focused, Some(focal));

        // Reverse is symmetric.
        assert!(st.focus_cycle(false));
        assert_eq!(st.focused, Some(c2));
    }

    #[test]
    fn focus_cycle_focal_only_is_noop() {
        let mut st = CanvasState::default();
        st.push(block(1, BlockRelation::Focal, WorldRect::new(0.0, 0.0, 20.0, 20.0)));
        assert!(!st.focus_cycle(true));
    }
}
