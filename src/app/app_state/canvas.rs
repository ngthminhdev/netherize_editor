//! `AppState` integration for NetherCanvas (Phase A). Builds/clears the
//! `CanvasState`, applies navigation/pan/zoom, and appends sourced relation
//! blocks. **Read-only**: nothing here mutates the live document (`self.text`)
//! or the dirty flag — opening/closing the canvas leaves the editor untouched.

use crate::canvas::{
    BlockOrigin, BlockRelation, BlockSnapshot, CanvasBlock, CanvasState, Dir, WorldRect,
};

use super::AppState;

/// Gap between the focal block and the relation column, and between stacked
/// relation blocks (world units == screen px at zoom 1).
const CANVAS_GAP_X: f32 = 90.0;
const CANVAS_GAP_Y: f32 = 48.0;
/// Lines of context shown above and below the focal line in a block snapshot.
const CANVAS_CONTEXT_LINES: usize = 6;

/// Build a line-numbered snippet (matches the relation-block preview format) so
/// every card reads like a mini editor with a gutter.
fn numbered_snippet(lines: &[String], start_line_0: usize, focal_line_0: usize) -> String {
    let mut out = String::new();
    for (offset, line) in lines.iter().enumerate() {
        let n = start_line_0 + offset + 1;
        let marker = if start_line_0 + offset == focal_line_0 {
            "▶"
        } else {
            " "
        };
        out.push_str(&format!("{marker}{n:>4}  {line}\n"));
    }
    out
}

impl AppState {
    pub fn is_canvas_active(&self) -> bool {
        self.canvas.is_some()
    }

    pub fn canvas(&self) -> Option<&CanvasState> {
        self.canvas.as_ref()
    }

    /// Open the canvas on the symbol under the cursor in the active file. The
    /// focal block is a read-only snapshot of the lines around the cursor.
    /// `block_w/h` are the world size of every card (computed by the caller from
    /// the editor font so text fits at zoom 1). Returns false when there is no
    /// active file to source from.
    pub fn open_canvas(&mut self, block_w: f32, block_h: f32) -> bool {
        let Some(path) = self.active_file.clone() else {
            return false;
        };
        let (line, col) = self.cursor_line_col();
        let symbol = self.word_under_cursor().unwrap_or_default();

        let total_lines = self.text.len_lines().max(1);
        let focal_line = line.min(total_lines.saturating_sub(1));
        let start_line = focal_line.saturating_sub(CANVAS_CONTEXT_LINES);
        let end_line = (focal_line + CANVAS_CONTEXT_LINES + 1).min(total_lines);

        let start_char = self.text.line_to_char(start_line);
        let end_char = self.text.line_to_char(end_line);
        let start_byte = self.text.char_to_byte(start_char);
        let end_byte = self.text.char_to_byte(end_char);
        let raw_lines: Vec<String> = (start_line..end_line)
            .map(|l| {
                let s = self.text.line_to_char(l);
                let e = self.text.line_to_char((l + 1).min(total_lines));
                self.text.slice(s..e).to_string().trim_end_matches('\n').to_string()
            })
            .collect();
        let snippet = numbered_snippet(&raw_lines, start_line, focal_line);

        let file_label = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| path.to_string_lossy().into_owned());
        let title = if symbol.is_empty() {
            file_label
        } else {
            format!("{symbol} — {file_label}")
        };

        let mut state = CanvasState::new();
        state.block_w = block_w;
        state.block_h = block_h;
        let id = state.alloc_id();
        state.push(CanvasBlock {
            id,
            relation: BlockRelation::Focal,
            origin: BlockOrigin {
                path,
                start_byte,
                end_byte,
                symbol_name: symbol,
                lsp_line: focal_line as u32,
                lsp_character: col as u32,
            },
            snapshot: BlockSnapshot {
                title,
                text: snippet,
            },
            world: WorldRect::new(0.0, 0.0, block_w, block_h),
        });
        self.canvas = Some(state);
        true
    }

    /// Close the canvas. Returns whether a canvas was active.
    pub fn close_canvas(&mut self) -> bool {
        self.canvas.take().is_some()
    }

    pub fn canvas_focus_dir(&mut self, dir: Dir) -> bool {
        self.canvas
            .as_mut()
            .map(|c| c.focus_direction(dir))
            .unwrap_or(false)
    }

    pub fn canvas_cycle(&mut self, forward: bool) -> bool {
        self.canvas
            .as_mut()
            .map(|c| c.focus_cycle(forward))
            .unwrap_or(false)
    }

    pub fn canvas_pan(&mut self, dx: f32, dy: f32) -> bool {
        match self.canvas.as_mut() {
            Some(c) => {
                c.camera.pan(dx, dy);
                true
            }
            None => false,
        }
    }

    pub fn canvas_zoom(&mut self, factor: f32, anchor_x: f32, anchor_y: f32) -> bool {
        match self.canvas.as_mut() {
            Some(c) => {
                c.camera.zoom_about(anchor_x, anchor_y, factor);
                true
            }
            None => false,
        }
    }

    /// Center the camera so the focused block sits at 50%/45% of the viewport —
    /// used on open while only the focal card exists.
    pub fn canvas_center_on_focus(&mut self, viewport_w: f32, viewport_h: f32) -> bool {
        self.anchor_focal(viewport_w, viewport_h, 0.5, 0.45)
    }

    /// Shift the focal card to the left (26%/40%) so the relation column to its
    /// right is visible — used once relations are spawned.
    pub fn canvas_anchor_for_relations(&mut self, viewport_w: f32, viewport_h: f32) -> bool {
        self.anchor_focal(viewport_w, viewport_h, 0.26, 0.40)
    }

    fn anchor_focal(&mut self, viewport_w: f32, viewport_h: f32, fx: f32, fy: f32) -> bool {
        let Some(state) = self.canvas.as_mut() else {
            return false;
        };
        let Some(focal) = canvas_focal_rect(state) else {
            return false;
        };
        let (cx, cy) = focal.center();
        let zoom = state.camera.zoom;
        state.camera.offset_x = cx - (viewport_w * fx) / zoom;
        state.camera.offset_y = cy - (viewport_h * fy) / zoom;
        true
    }

    /// Provenance of the focused block — used to submit LSP relation requests.
    pub fn canvas_focal_origin(&self) -> Option<BlockOrigin> {
        self.canvas
            .as_ref()
            .and_then(|c| c.focused_block())
            .map(|b| b.origin.clone())
    }

    /// Append sourced relation blocks in a single column to the **right** of the
    /// focal block (stacked below any relations already present, so repeated
    /// def/refs spawns don't overlap). Each entry is `(relation, origin,
    /// snapshot)`.
    pub fn canvas_add_relations(
        &mut self,
        relations: Vec<(BlockRelation, BlockOrigin, BlockSnapshot)>,
    ) -> bool {
        if relations.is_empty() {
            return false;
        }
        let Some(state) = self.canvas.as_mut() else {
            return false;
        };
        let Some(focal) = canvas_focal_rect(state) else {
            return false;
        };
        let block_w = if state.block_w > 0.0 {
            state.block_w
        } else {
            focal.w
        };
        let block_h = if state.block_h > 0.0 {
            state.block_h
        } else {
            focal.h
        };

        let right_x = focal.x + focal.w + CANVAS_GAP_X;
        let step = block_h + CANVAS_GAP_Y;
        let existing = state
            .blocks
            .iter()
            .filter(|b| b.relation != BlockRelation::Focal)
            .count();

        let mut added = false;
        for (i, (relation, origin, snapshot)) in relations.into_iter().enumerate() {
            if relation == BlockRelation::Focal {
                continue;
            }
            let idx = existing + i;
            let world = WorldRect::new(
                right_x,
                focal.y + idx as f32 * step,
                block_w,
                block_h,
            );
            let id = state.alloc_id();
            state.push(CanvasBlock {
                id,
                relation,
                origin,
                snapshot,
                world,
            });
            added = true;
        }
        added
    }
}

/// World rect of the focal (origin) block, falling back to the first block.
fn canvas_focal_rect(state: &CanvasState) -> Option<WorldRect> {
    state
        .blocks
        .iter()
        .find(|b| b.relation == BlockRelation::Focal)
        .or_else(|| state.blocks.first())
        .map(|b| b.world)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ropey::Rope;
    use std::path::PathBuf;

    const VW: f32 = 1000.0;
    const VH: f32 = 600.0;

    fn app_with_text(text: &str) -> AppState {
        let mut app = AppState::new(PathBuf::from("/tmp/scratch.rs"));
        app.text = Rope::from_str(text);
        app.active_file = Some(PathBuf::from("/proj/src/foo.rs"));
        app
    }

    fn origin(name: &str) -> BlockOrigin {
        BlockOrigin {
            path: PathBuf::from("/proj/src/bar.rs"),
            start_byte: 0,
            end_byte: 1,
            symbol_name: name.into(),
            lsp_line: 0,
            lsp_character: 0,
        }
    }

    fn snap(title: &str) -> BlockSnapshot {
        BlockSnapshot {
            title: title.into(),
            text: "code".into(),
        }
    }

    #[test]
    fn open_canvas_requires_active_file() {
        let mut app = AppState::new(PathBuf::from("/tmp/x.rs"));
        app.text = Rope::from_str("fn main() {}\n");
        app.active_file = None;
        assert!(!app.open_canvas(VW, VH));
        assert!(!app.is_canvas_active());
    }

    #[test]
    fn open_canvas_builds_focused_focal_block_readonly() {
        let mut app = app_with_text("fn foo() {\n    bar();\n}\n");
        let text_before = app.text.to_string();
        assert!(app.open_canvas(VW, VH));
        assert!(app.is_canvas_active());
        let canvas = app.canvas().unwrap();
        assert_eq!(canvas.blocks.len(), 1);
        assert!(canvas.block_w > 0.0 && canvas.block_h > 0.0);
        let focal = canvas.focused_block().unwrap();
        assert_eq!(focal.relation, BlockRelation::Focal);
        assert!(!focal.snapshot.text.is_empty());
        // Read-only invariant: the live document is untouched.
        assert_eq!(app.text.to_string(), text_before);
        assert!(!app.dirty);
    }

    #[test]
    fn close_canvas_clears() {
        let mut app = app_with_text("fn x() {}\n");
        app.open_canvas(VW, VH);
        assert!(app.close_canvas());
        assert!(!app.is_canvas_active());
        assert!(!app.close_canvas());
    }

    #[test]
    fn add_relations_stacks_all_to_the_right_and_navigates() {
        let mut app = app_with_text("fn focal() {}\n");
        app.open_canvas(VW, VH);
        let added = app.canvas_add_relations(vec![
            (BlockRelation::Definition, origin("def"), snap("def")),
            (BlockRelation::Caller, origin("caller"), snap("caller")),
        ]);
        assert!(added);
        let canvas = app.canvas().unwrap();
        assert_eq!(canvas.blocks.len(), 3);
        let focal = canvas.blocks[0].world;
        for b in &canvas.blocks[1..] {
            // Every relation sits to the right of the focal block.
            assert!(b.world.x >= focal.x + focal.w, "relation not on the right");
        }
        // Relations are stacked (distinct Y), not overlapping.
        assert!((canvas.blocks[1].world.y - canvas.blocks[2].world.y).abs() > 1.0);
        // From the focal block, `→` reaches a right-column block.
        assert!(app.canvas_focus_dir(Dir::Right));
        assert_ne!(
            app.canvas().unwrap().focused_block().unwrap().relation,
            BlockRelation::Focal
        );
    }

    #[test]
    fn add_relations_appends_below_existing() {
        let mut app = app_with_text("fn focal() {}\n");
        app.open_canvas(VW, VH);
        app.canvas_add_relations(vec![(BlockRelation::Definition, origin("d"), snap("d"))]);
        let first_y = app.canvas().unwrap().blocks[1].world.y;
        app.canvas_add_relations(vec![(BlockRelation::Caller, origin("c"), snap("c"))]);
        let second_y = app.canvas().unwrap().blocks[2].world.y;
        assert!(second_y > first_y, "second batch should stack below the first");
    }

    #[test]
    fn center_on_focus_anchors_focal() {
        let mut app = app_with_text("fn x() {}\n");
        app.open_canvas(VW, VH);
        assert!(app.canvas_center_on_focus(1000.0, 600.0));
        let cam = app.canvas().unwrap().camera;
        let focal = app.canvas().unwrap().blocks[0].world;
        let (cx, cy) = focal.center();
        let (sx, sy) = cam.world_to_screen_point(cx, cy);
        assert!((sx - 500.0).abs() < 1e-2);
        assert!((sy - 270.0).abs() < 1e-2);
    }

    #[test]
    fn pan_zoom_only_affect_camera() {
        let mut app = app_with_text("fn x() {}\n");
        app.open_canvas(VW, VH);
        assert!(app.canvas_pan(40.0, 0.0));
        assert!(app.canvas_zoom(2.0, 100.0, 100.0));
        assert!((app.canvas().unwrap().camera.zoom - 2.0).abs() < 1e-6);
        app.close_canvas();
        assert!(!app.canvas_pan(1.0, 1.0));
        assert!(!app.canvas_zoom(2.0, 0.0, 0.0));
    }
}
