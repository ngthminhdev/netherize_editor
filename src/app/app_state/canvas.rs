//! `AppState` integration for NetherCanvas (Phase A). Builds/clears the
//! `CanvasState`, applies navigation/pan/zoom, and appends sourced relation
//! blocks. **Read-only**: nothing here mutates the live document (`self.text`)
//! or the dirty flag — opening/closing the canvas leaves the editor untouched.

use crate::canvas::{
    BlockId, BlockOrigin, BlockRelation, BlockSnapshot, CARD_MAX_LINES, CARD_MIN_LINES, CanvasBlock,
    CanvasInteraction, CanvasSpan, CanvasState, Dir, WorldRect,
};

use super::{AppState, CanvasEditSession};

/// Gap between the focal block and the relation column, and between stacked
/// relation blocks (world units == screen px at zoom 1).
const CANVAS_GAP_X: f32 = 90.0;
const CANVAS_GAP_Y: f32 = 48.0;
/// Default lines of context shown above and below the focal line in a block
/// snapshot — the instant ±N preview a card spawns with, before its enclosing
/// scope resolves and replaces it.
const CANVAS_CONTEXT_LINES: usize = 8;

impl AppState {
    pub fn is_canvas_active(&self) -> bool {
        self.canvas.is_some()
    }

    pub fn canvas(&self) -> Option<&CanvasState> {
        self.canvas.as_ref()
    }

    /// Open the canvas on the symbol under the cursor in the active file. The
    /// focal block is a read-only snapshot of the lines around the cursor.
    /// `block_w` is the uniform card width and `block_h` the maximum card height
    /// (both computed by the caller from the editor font so text fits at zoom 1);
    /// `line_h` is the code line height in world units, which drives per-card
    /// height via [`CanvasState::card_height`]. Returns false when there is no
    /// active file to source from.
    pub fn open_canvas(&mut self, block_w: f32, block_h: f32, line_h: f32) -> bool {
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
        let snippet = raw_lines.join("\n");

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
        state.line_h = line_h;
        state.context_lines = CANVAS_CONTEXT_LINES;
        let id = state.alloc_id();
        state.push(CanvasBlock {
            id,
            relation: BlockRelation::Focal,
            origin: BlockOrigin {
                path,
                start_byte,
                end_byte,
                symbol_name: symbol.clone(),
                lsp_line: focal_line as u32,
                lsp_character: col as u32,
            },
            snapshot: BlockSnapshot {
                title,
                symbol,
                start_line: start_line as u32 + 1,
                text: snippet,
                spans: Vec::new(),
            },
            pinned: false,
            context_lines: CANVAS_CONTEXT_LINES,
            parent: None,
            scope_lines: None,
            height_rows: None,
            world: WorldRect::new(0.0, 0.0, block_w, block_h),
        });
        self.canvas = Some(state);
        true
    }

    /// Close the canvas. Returns whether a canvas was active. Any in-card edit
    /// session is dropped (the main editor was never switched, so there is
    /// nothing to restore).
    pub fn close_canvas(&mut self) -> bool {
        self.discard_canvas_edit_session();
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

    /// Move the focused card by `(dx, dy)` world units (hjkl). No-op if pinned.
    pub fn canvas_move_focused(&mut self, dx: f32, dy: f32) -> bool {
        self.canvas
            .as_mut()
            .map(|c| c.move_focused(dx, dy))
            .unwrap_or(false)
    }

    /// Toggle pin on the focused card (P).
    pub fn canvas_toggle_pin(&mut self) -> bool {
        self.canvas.as_mut().map(|c| c.toggle_pin()).unwrap_or(false)
    }

    /// Close the focused relation card (space x).
    pub fn canvas_close_focused(&mut self) -> bool {
        self.canvas
            .as_mut()
            .map(|c| c.close_focused())
            .unwrap_or(false)
    }

    // ── Phase B: edit-in-card (S2) / background (S3) ─────────────────────────

    /// The current canvas interaction sub-state, if a canvas is open.
    pub fn canvas_interaction(&self) -> Option<CanvasInteraction> {
        self.canvas.as_ref().map(|c| c.interaction)
    }

    /// S1→S2: begin editing the focused relation card. Creates a
    /// [`CanvasEditSession`] holding the card file's text + cursor; the editor
    /// engine edits it via a scoped swap during dispatch, so `self.text` (the
    /// main editor) NEVER leaves the gc-origin file. Resumes a stashed (dirty)
    /// session for the same card. Returns whether editing began.
    pub fn canvas_begin_edit(&mut self) -> bool {
        let Some((block, origin)) = self.canvas.as_ref().and_then(|c| {
            let b = c.focused_block()?;
            if b.relation == BlockRelation::Focal {
                return None;
            }
            Some((b.id, b.origin.clone()))
        }) else {
            return false;
        };
        // Reuse a stashed (dirty) session for this card; otherwise build one.
        if !self.canvas_edit_session_is_for(block) {
            // If the card is the SAME file the main editor has open, seed from the
            // LIVE buffer (`self.text`) so the card includes unsaved edits and the
            // two stay consistent; otherwise read the file from disk.
            let same_as_main = self
                .active_file()
                .is_some_and(|a| crate::app::app_state::overlays::path_matches(a, &origin.path));
            let text = if same_as_main {
                self.text.clone()
            } else {
                match std::fs::read_to_string(&origin.path) {
                    Ok(content) => ropey::Rope::from_str(&content),
                    Err(_) => return false, // file vanished — stay in Navigate
                }
            };
            let session = CanvasEditSession::new(
                block,
                origin.path.clone(),
                text,
                origin.lsp_line as usize,
                origin.lsp_character as usize,
            );
            self.put_canvas_edit_session(session);
        }
        // Promote the interaction state machine (rejects the Focal anchor again).
        if self.canvas.as_mut().and_then(|c| c.begin_edit()).is_none() {
            self.discard_canvas_edit_session();
            return false;
        }
        // Mirror the session cursor into the interaction for the renderer.
        let ctx = self.canvas_block_context_lines(block);
        if let Some((_lines, _start, cl, cc)) = self.canvas_edit_session_window(ctx)
            && let Some(c) = self.canvas.as_mut()
        {
            c.set_edit_cursor(cl as u32, cc as u32);
        }
        true
    }

    /// S2→S1: leave edit mode back to card navigation. `self.text` (the main
    /// editor) was never switched, so there is nothing to restore. The session is
    /// kept stashed so re-entering the SAME card resumes both cursor position and
    /// unsaved edits. Entering a *different* card replaces it — one session at a
    /// time this iteration; ⌘S before leaving to persist.
    pub fn canvas_end_edit(&mut self) -> bool {
        let ended = self.canvas.as_mut().map(|c| c.end_edit()).unwrap_or(false);
        if ended {
            // Leaving the card dismisses any in-card hover popup.
            if let Some(c) = self.canvas.as_mut() {
                c.card_hover = None;
            }
        }
        ended
    }

    /// In-card `gd`/`gr` spawned a child card and focus already jumped to it; if
    /// we're still editing the card that spawned it, leave EditCard (→ Navigate)
    /// so the focus visibly lands on the child without needing an Esc. Returns
    /// whether it ended an edit (so the caller can `didClose` the card's LSP doc).
    /// No-op (false) if we've moved on to a different card / left edit mode.
    pub fn canvas_end_edit_for_spawn(&mut self, parent: BlockId) -> bool {
        if self.canvas.as_ref().and_then(|c| c.editing_block()) != Some(parent) {
            return false;
        }
        self.canvas_end_edit()
    }

    /// Drop any in-card edit session (on close / abort / clean exit).
    pub(crate) fn discard_canvas_edit_session(&mut self) {
        self.canvas_edit_session = None;
    }

    /// Set/replace the in-card `K` hover doc box (anchored at the edit caret).
    /// No-op if the canvas isn't open.
    pub(crate) fn canvas_set_card_hover(&mut self, lines: Vec<String>) -> bool {
        match self.canvas.as_mut() {
            Some(c) => {
                c.card_hover = Some(lines);
                true
            }
            None => false,
        }
    }

    /// Clear the in-card hover popup. Returns whether one was showing.
    pub(crate) fn canvas_clear_card_overlay(&mut self) -> bool {
        match self.canvas.as_mut() {
            Some(c) => c.card_hover.take().is_some(),
            None => false,
        }
    }

    /// S1→S3: push the canvas to the background (editor regains focus).
    pub fn canvas_enter_background(&mut self) -> bool {
        self.canvas
            .as_mut()
            .map(|c| c.enter_background())
            .unwrap_or(false)
    }

    /// S3→S1: re-grab the canvas from the background.
    pub fn canvas_focus_navigate(&mut self) -> bool {
        self.canvas
            .as_mut()
            .map(|c| c.focus_navigate())
            .unwrap_or(false)
    }

    /// Replace the edit card's snapshot with a freshly-windowed view of the live
    /// buffer (re-highlighted by the caller) and mirror the cursor. The card
    /// re-grows to fit so the cursor row stays visible. No-op unless `block_id`
    /// exists. Drives the live in-card render (S2).
    #[allow(clippy::too_many_arguments)]
    pub fn canvas_update_edit_card(
        &mut self,
        block_id: BlockId,
        start_line: u32,
        text: String,
        spans: Vec<CanvasSpan>,
        cursor_line: u32,
        cursor_col: u32,
    ) {
        // Mirror the session's visual selection onto the canvas for the renderer
        // (computed before the mutable borrow below). `None` clears it outside
        // Visual mode so a stale band never lingers.
        let selection = self.canvas_edit_session_selection();
        let Some(state) = self.canvas.as_mut() else {
            return;
        };
        state.set_edit_selection(selection);
        let lines = text.split('\n').count();
        let new_h = (state.line_h > 0.0).then(|| state.card_height(lines));
        if let Some(b) = state.blocks.iter_mut().find(|b| b.id == block_id) {
            b.snapshot.text = text;
            b.snapshot.spans = spans;
            b.snapshot.start_line = start_line;
            if let Some(h) = new_h {
                b.world.h = h;
            }
            // NOTE: `b.origin` is left UNTOUCHED — it must stay pinned to the
            // card's real source site, since dedup ([`canvas_add_relations`]) and
            // `+`/`-` re-source ([`canvas_change_context`]) key off it. The live
            // caret line lives only in the transient interaction state below; the
            // renderer derives the active-line band from it for the edit card.
        }
        state.set_edit_cursor(cursor_line, cursor_col);
    }

    /// The live per-card context-line count (defaults to [`CANVAS_CONTEXT_LINES`]
    /// when no canvas is open or the value is unset).
    pub fn canvas_context_lines(&self) -> usize {
        self.canvas
            .as_ref()
            .map(|c| {
                if c.context_lines == 0 {
                    CANVAS_CONTEXT_LINES
                } else {
                    c.context_lines
                }
            })
            .unwrap_or(CANVAS_CONTEXT_LINES)
    }

    /// Per-card context-line count for `block` (defaults when unset / absent).
    /// Drives the edit-card window size so a card grown with `+` keeps showing
    /// the same span once promoted to a live mini-editor.
    pub fn canvas_block_context_lines(&self, block: BlockId) -> usize {
        self.canvas
            .as_ref()
            .and_then(|c| c.block(block))
            .map(|b| {
                if b.context_lines == 0 {
                    CANVAS_CONTEXT_LINES
                } else {
                    b.context_lines
                }
            })
            .unwrap_or(CANVAS_CONTEXT_LINES)
    }

    /// `=`/`-`: resize ONLY the focused relation card's HEIGHT by `delta` visible
    /// rows, in place (top-left fixed, growing/shrinking downward). With
    /// scope-loaded cards this is a PURE visual resize — it never re-sources extra
    /// file context (the card already shows only its scope). The per-card row count
    /// is stored in `height_rows` (clamped to `[CARD_MIN_LINES, CARD_MAX_LINES]`)
    /// and also caps the in-card edit viewport so editing scrolls within those
    /// rows. No-op (`false`) on the focal anchor, at the bounds, or with no focused
    /// card. Marks the layout user-arranged so a later spawn won't reflow it.
    pub fn canvas_change_focused_height(&mut self, delta: i32) -> bool {
        let Some(state) = self.canvas.as_mut() else {
            return false;
        };
        let Some(id) = state.focused else {
            return false;
        };
        let Some(block) = state.blocks.iter().find(|b| b.id == id) else {
            return false;
        };
        if block.relation == BlockRelation::Focal {
            return false;
        }
        // Effective current rows: explicit override, else the snapshot's own size.
        let cur = block
            .height_rows
            .unwrap_or_else(|| block.snapshot.text.split('\n').count().max(1));
        let new = (cur as i32 + delta)
            .clamp(CARD_MIN_LINES as i32, CARD_MAX_LINES as i32) as usize;
        if new == cur {
            return false;
        }
        let new_h = (state.line_h > 0.0).then(|| state.card_height(new));
        let Some(block) = state.blocks.iter_mut().find(|b| b.id == id) else {
            return false;
        };
        block.height_rows = Some(new);
        if let Some(h) = new_h {
            block.world.h = h; // top-left fixed; grow/shrink downward only
        }
        state.user_arranged = true;
        true
    }

    /// `Shift`+`+`/`-`: change ONLY the focused relation card's WIDTH by `delta`
    /// world units, in place (top-left fixed), clamped to a sane range relative
    /// to the uniform card width. Widening shows more horizontal code (the body
    /// clips to the card width). No-op on the focal anchor / at the bounds / with
    /// no focused card. Marks the layout user-arranged so auto-arrange won't reset
    /// the width. Returns whether the width changed.
    pub fn canvas_change_focused_width(&mut self, delta: f32) -> bool {
        let Some(state) = self.canvas.as_mut() else {
            return false;
        };
        let Some(id) = state.focused else {
            return false;
        };
        let base_w = if state.block_w > 0.0 { state.block_w } else { 400.0 };
        let min_w = base_w * 0.5;
        let max_w = base_w * 2.5;
        let Some(b) = state.blocks.iter_mut().find(|b| b.id == id) else {
            return false;
        };
        if b.relation == BlockRelation::Focal {
            return false;
        }
        let new_w = (b.world.w + delta).clamp(min_w, max_w);
        if (new_w - b.world.w).abs() < 0.5 {
            return false;
        }
        b.world.w = new_w;
        state.user_arranged = true;
        true
    }

    /// Replace a card's snapshot with a scope-resolved one (progressive refine).
    /// The card regrows to fit the new snapshot height (top-left fixed, mirroring
    /// [`canvas_apply_focused_context`]); the focal anchor is never targeted
    /// (it's the editor itself, not drawn as a card). Returns whether a relation
    /// card matched `card_id` (false if it was closed/opened-as-tab before resolve
    /// or if `card_id` points at the focal anchor).
    pub fn canvas_apply_card_scope(
        &mut self,
        card_id: BlockId,
        snapshot: BlockSnapshot,
        scope_lines: Option<(u32, u32)>,
    ) -> bool {
        let Some(state) = self.canvas.as_mut() else {
            return false;
        };
        let lines = snapshot.text.split('\n').count();
        // Respect a user HEIGHT override (`=`/`-`) if one is set; else auto-fit.
        let rows = state
            .block(card_id)
            .and_then(|b| b.height_rows)
            .unwrap_or(lines);
        let new_h = (state.line_h > 0.0).then(|| state.card_height(rows));
        let Some(b) = state.blocks.iter_mut().find(|b| b.id == card_id) else {
            return false;
        };
        if b.relation == BlockRelation::Focal {
            return false;
        }
        b.snapshot = snapshot;
        // The full enclosing-definition range (NOT the clamped snapshot) bounds the
        // in-card edit viewport so editing scrolls within the whole function.
        b.scope_lines = scope_lines;
        if let Some(h) = new_h {
            b.world.h = h; // top-left fixed; grow/shrink downward only
        }
        true
    }

    /// Everything `CanvasCardScopeResult` needs to re-source a card to its
    /// enclosing-definition scope: `(lsp_line, lsp_character, symbol_name)`.
    /// `None` when the card is gone / focal / canvas closed (stale result dropped).
    pub fn canvas_card_scope_target(
        &self,
        card_id: BlockId,
    ) -> Option<(u32, u32, String)> {
        let state = self.canvas.as_ref()?;
        let b = state.blocks.iter().find(|b| b.id == card_id)?;
        if b.relation == BlockRelation::Focal {
            return None;
        }
        Some((b.origin.lsp_line, b.origin.lsp_character, b.origin.symbol_name.clone()))
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

    /// The gc-origin file the canvas is anchored to (the FOCAL block's path).
    /// Restoring a stashed canvas (`gc`) switches the editor back to it so the
    /// connectors line up with the focal symbol.
    pub fn canvas_focal_file(&self) -> Option<std::path::PathBuf> {
        self.canvas.as_ref().and_then(|c| {
            c.blocks
                .iter()
                .find(|b| b.relation == BlockRelation::Focal)
                .map(|b| b.origin.path.clone())
        })
    }

    /// The focal symbol's `(line, col)` — but ONLY when the editor is currently
    /// showing the focal (gc-origin) file, so the connector origin projects onto
    /// the right buffer. `None` otherwise (the renderer falls back to the caret).
    pub fn canvas_focal_position(&self) -> Option<(u32, u32)> {
        let c = self.canvas.as_ref()?;
        let focal = c
            .blocks
            .iter()
            .find(|b| b.relation == BlockRelation::Focal)?;
        if self.active_file() != Some(focal.origin.path.as_path()) {
            return None;
        }
        Some((focal.origin.lsp_line, focal.origin.lsp_character))
    }

    /// The origin of the focused RELATION card (`None` on the focal anchor / no
    /// focused card) — used to open it as a buffer tab (`o`).
    pub fn canvas_focused_card_origin(&self) -> Option<BlockOrigin> {
        let c = self.canvas.as_ref()?;
        let b = c.focused_block()?;
        if b.relation == BlockRelation::Focal {
            return None;
        }
        Some(b.origin.clone())
    }

    /// Stash (hide, keep state) / un-stash the canvas — `o` opens a card as a tab
    /// and stashes; `gc` un-stashes. Returns whether a canvas was present.
    pub fn canvas_set_stashed(&mut self, stashed: bool) -> bool {
        match self.canvas.as_mut() {
            Some(c) => {
                c.stashed = stashed;
                true
            }
            None => false,
        }
    }

    /// Whether the canvas is stashed (hidden but kept).
    pub fn canvas_is_stashed(&self) -> bool {
        self.canvas.as_ref().is_some_and(|c| c.stashed)
    }

    /// Whether the canvas overlay should render THIS frame. It is **bound to the
    /// file it was opened over** (the `gc` focal file): switching to another
    /// buffer (file picker, Ctrl-h/Ctrl-l) hides it — the canvas stays in state
    /// and shows again on return to the focal file. A stashed canvas (a card
    /// opened as a tab via `o`) never renders.
    pub fn canvas_should_render(&self) -> bool {
        let Some(c) = self.canvas.as_ref() else {
            return false;
        };
        if c.stashed {
            return false;
        }
        // Bound to the focal file: render only while it is the active buffer. When
        // the focal path is unknown (no focal block — shouldn't happen for a real
        // canvas), fall back to rendering rather than hiding silently.
        match self.canvas_focal_file() {
            Some(focal) => self.active_file() == Some(focal.as_path()),
            None => true,
        }
    }

    /// Append sourced relation blocks in a single column to the **right** of the
    /// focal block. Cards stack below any relations already present (so repeated
    /// def/refs spawns don't overlap) and each card's height **tracks its own
    /// snapshot** so short snippets don't leave a tall empty gap.
    ///
    /// Locations are **deduplicated by `(path, line)`**: gopls `references`
    /// returns the declaration too, so a `Caller` can land on the same site as
    /// the `Definition` (or a prior caller). One card per site is kept; a
    /// `Definition` supersedes a previously-added `Caller` at the same site
    /// (handles either async arrival order of the def/refs results).
    ///
    /// Each entry is `(relation, origin, snapshot)`.
    pub fn canvas_add_relations(
        &mut self,
        relations: Vec<(BlockRelation, BlockOrigin, BlockSnapshot)>,
    ) -> bool {
        self.canvas_add_relations_with_parent(relations, None)
    }

    /// As [`canvas_add_relations`], but each new card records `parent` (the card
    /// it was spawned from via in-card `gd`/`gr`). When `parent` is `Some`, focus
    /// jumps to the first new card so the user lands on the spawned result and the
    /// connector is drawn from the parent card rather than the editor caret.
    pub fn canvas_add_relations_with_parent(
        &mut self,
        relations: Vec<(BlockRelation, BlockOrigin, BlockSnapshot)>,
        parent: Option<BlockId>,
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

        // Seed every new card's per-card context from the session default so
        // `+`/`-` later adjust each card independently from a known baseline.
        let default_ctx = if state.context_lines == 0 {
            CANVAS_CONTEXT_LINES
        } else {
            state.context_lines
        };

        let right_x = focal.x + focal.w + CANVAS_GAP_X;
        // Stack below the lowest existing relation card; with the first batch
        // (none yet) top-align with the focal block. Variable card heights mean
        // we track a running cursor instead of `index * fixed_step`.
        let lowest = state
            .blocks
            .iter()
            .filter(|b| b.relation != BlockRelation::Focal)
            .map(|b| b.world.y + b.world.h)
            .fold(f32::NEG_INFINITY, f32::max);
        let mut y_cursor = if lowest.is_finite() {
            lowest + CANVAS_GAP_Y
        } else {
            focal.y
        };

        // The focal location is the cursor in the live editor — a relation that
        // resolves to it (e.g. the reference at the call site under the cursor)
        // is already on screen, so skip it rather than draw a redundant card of
        // the current file.
        let focal_loc = state
            .blocks
            .iter()
            .find(|b| b.relation == BlockRelation::Focal)
            .map(|b| (b.origin.path.clone(), b.origin.lsp_line));

        let mut added = false;
        let mut first_new_id: Option<BlockId> = None;
        for (relation, origin, snapshot) in relations {
            if relation == BlockRelation::Focal {
                continue;
            }
            if focal_loc
                .as_ref()
                .is_some_and(|(p, l)| *p == origin.path && *l == origin.lsp_line)
            {
                continue;
            }
            // Dedup by (path, line) against existing relation cards.
            if let Some(pos) = state.blocks.iter().position(|b| {
                b.relation != BlockRelation::Focal
                    && b.origin.path == origin.path
                    && b.origin.lsp_line == origin.lsp_line
            }) {
                // A Definition upgrades a Caller occupying the same site; any
                // other duplicate (incl. Caller-over-Definition) is dropped.
                if relation == BlockRelation::Definition
                    && state.blocks[pos].relation != BlockRelation::Definition
                {
                    state.blocks[pos].relation = BlockRelation::Definition;
                    state.blocks[pos].origin = origin;
                    state.blocks[pos].snapshot = snapshot;
                    added = true;
                }
                continue;
            }

            let lines = snapshot.text.split('\n').count();
            let block_h = if state.line_h > 0.0 {
                state.card_height(lines)
            } else {
                state.block_h.max(1.0)
            };
            let world = WorldRect::new(right_x, y_cursor, block_w, block_h);
            y_cursor += block_h + CANVAS_GAP_Y;
            let id = state.alloc_id();
            state.push(CanvasBlock {
                id,
                relation,
                origin,
                snapshot,
                world,
                pinned: false,
                context_lines: default_ctx,
                parent,
                scope_lines: None,
                height_rows: None,
            });
            if first_new_id.is_none() {
                first_new_id = Some(id);
            }
            added = true;
        }

        if added {
            if let (Some(_), Some(fid)) = (parent, first_new_id) {
                // Spawned from a card via in-card gd/gr — land focus on the new
                // card so the user immediately sees/works the spawned result.
                state.focused = Some(fid);
            } else {
                // Auto-focus the first relation card so arrows/Enter work
                // immediately (no Tab needed). Only steal focus while it still
                // rests on the invisible focal block — never override a card the
                // user already chose.
                let focal_id = state
                    .blocks
                    .iter()
                    .find(|b| b.relation == BlockRelation::Focal)
                    .map(|b| b.id);
                if state.focused.is_none() || state.focused == focal_id {
                    let first = state
                        .blocks
                        .iter()
                        .find(|b| b.relation != BlockRelation::Focal)
                        .map(|b| b.id);
                    if let Some(fid) = first {
                        state.focused = Some(fid);
                    }
                }
            }
        }
        added
    }

    /// Anchor the relation-card column to the **right** of the viewport with a
    /// ~5% right margin (and a small top margin), so cards sit out near the
    /// right edge over the editor rather than floating mid-screen.
    pub fn canvas_anchor_cards_right(&mut self, viewport_w: f32, viewport_h: f32) -> bool {
        let Some(state) = self.canvas.as_mut() else {
            return false;
        };
        let zoom = state.camera.zoom.max(0.01);
        let mut right = f32::NEG_INFINITY;
        let mut top = f32::INFINITY;
        for b in &state.blocks {
            if b.relation == BlockRelation::Focal {
                continue;
            }
            right = right.max(b.world.x + b.world.w);
            top = top.min(b.world.y);
        }
        if !right.is_finite() || !top.is_finite() {
            return false;
        }
        // screen = (world - offset) * zoom  →  solve offset for the target edge.
        state.camera.offset_x = right - (viewport_w * 0.95) / zoom;
        state.camera.offset_y = top - (viewport_h * 0.12) / zoom;
        true
    }

    /// Re-lay ALL relation cards into uniform-width columns to the RIGHT of the
    /// focal anchor, each column holding as many as fit within `viewport_h`
    /// (world units ≈ px at zoom 1), overflowing into further columns — so cards
    /// are tidy and visible on open instead of piling into one tall column.
    /// **Skipped once the user hand-arranges** (moves/pins a card). Returns
    /// whether it ran.
    pub fn canvas_auto_arrange(&mut self, viewport_h: f32) -> bool {
        let Some(state) = self.canvas.as_mut() else {
            return false;
        };
        if state.user_arranged {
            return false;
        }
        let Some(focal) = canvas_focal_rect(state) else {
            return false;
        };
        let block_w = if state.block_w > 0.0 {
            state.block_w
        } else {
            focal.w
        };
        // Leave a margin so a column doesn't hug the viewport's bottom edge.
        let col_budget = (viewport_h * 0.92).max(focal.h);
        // Heights first (immutable borrow), then place (mutable).
        let cards: Vec<(BlockId, f32)> = state
            .blocks
            .iter()
            .filter(|b| b.relation != BlockRelation::Focal)
            .map(|b| {
                let lines = b.snapshot.text.split('\n').count();
                let h = if state.line_h > 0.0 {
                    state.card_height(lines)
                } else {
                    b.world.h
                };
                (b.id, h)
            })
            .collect();
        let base_x = focal.x + focal.w + CANVAS_GAP_X;
        let mut col = 0usize;
        let mut y = focal.y;
        for (id, h) in cards {
            // Wrap to a new column when this card would overflow the budget
            // (always keep at least one card per column).
            if y > focal.y && (y - focal.y) + h > col_budget {
                col += 1;
                y = focal.y;
            }
            let x = base_x + col as f32 * (block_w + CANVAS_GAP_X);
            if let Some(b) = state.blocks.iter_mut().find(|b| b.id == id) {
                b.world.x = x;
                b.world.y = y;
                b.world.w = block_w;
                b.world.h = h;
            }
            y += h + CANVAS_GAP_Y;
        }
        true
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
    /// Code line height in world units used by the canvas tests.
    const LH: f32 = 20.0;

    fn app_with_text(text: &str) -> AppState {
        let mut app = AppState::new(PathBuf::from("/tmp/scratch.rs"));
        app.text = Rope::from_str(text);
        app.active_file = Some(PathBuf::from("/proj/src/foo.rs"));
        app
    }

    fn origin_at(path: &str, line: u32) -> BlockOrigin {
        BlockOrigin {
            path: PathBuf::from(path),
            start_byte: 0,
            end_byte: 1,
            symbol_name: "s".into(),
            lsp_line: line,
            lsp_character: 0,
        }
    }

    fn snap(title: &str) -> BlockSnapshot {
        BlockSnapshot {
            title: title.into(),
            symbol: title.into(),
            start_line: 1,
            text: "code".into(),
            spans: Vec::new(),
        }
    }

    /// Snapshot whose `text` has exactly `n` lines.
    fn snap_lines(n: usize) -> BlockSnapshot {
        let text = (0..n).map(|i| format!("line {i}")).collect::<Vec<_>>().join("\n");
        BlockSnapshot {
            title: "f.rs".into(),
            symbol: "s".into(),
            start_line: 1,
            text,
            spans: Vec::new(),
        }
    }

    #[test]
    fn open_canvas_requires_active_file() {
        let mut app = AppState::new(PathBuf::from("/tmp/x.rs"));
        app.text = Rope::from_str("fn main() {}\n");
        app.active_file = None;
        assert!(!app.open_canvas(VW, VH, LH));
        assert!(!app.is_canvas_active());
    }

    #[test]
    fn open_canvas_builds_focused_focal_block_readonly() {
        let mut app = app_with_text("fn foo() {\n    bar();\n}\n");
        let text_before = app.text.to_string();
        assert!(app.open_canvas(VW, VH, LH));
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
        app.open_canvas(VW, VH, LH);
        assert!(app.close_canvas());
        assert!(!app.is_canvas_active());
        assert!(!app.close_canvas());
    }

    #[test]
    fn add_relations_stacks_all_to_the_right_and_navigates() {
        let mut app = app_with_text("fn focal() {}\n");
        app.open_canvas(VW, VH, LH);
        let added = app.canvas_add_relations(vec![
            (BlockRelation::Definition, origin_at("/proj/src/def.rs", 10), snap("def")),
            (BlockRelation::Caller, origin_at("/proj/src/caller.rs", 20), snap("caller")),
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
        // Auto-focus: the first relation card is focused immediately (no Tab).
        let first = app.canvas().unwrap().focused;
        assert_ne!(
            app.canvas().unwrap().focused_block().unwrap().relation,
            BlockRelation::Focal
        );
        // `↓` moves to the next stacked relation card.
        assert!(app.canvas_focus_dir(Dir::Down));
        assert_ne!(app.canvas().unwrap().focused, first);
    }

    #[test]
    fn add_relations_appends_below_existing() {
        let mut app = app_with_text("fn focal() {}\n");
        app.open_canvas(VW, VH, LH);
        app.canvas_add_relations(vec![(
            BlockRelation::Definition,
            origin_at("/proj/src/d.rs", 10),
            snap("d"),
        )]);
        let first_y = app.canvas().unwrap().blocks[1].world.y;
        app.canvas_add_relations(vec![(
            BlockRelation::Caller,
            origin_at("/proj/src/c.rs", 20),
            snap("c"),
        )]);
        let second_y = app.canvas().unwrap().blocks[2].world.y;
        assert!(second_y > first_y, "second batch should stack below the first");
    }

    #[test]
    fn center_on_focus_anchors_focal() {
        let mut app = app_with_text("fn x() {}\n");
        app.open_canvas(VW, VH, LH);
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
        app.open_canvas(VW, VH, LH);
        assert!(app.canvas_pan(40.0, 0.0));
        assert!(app.canvas_zoom(2.0, 100.0, 100.0));
        assert!((app.canvas().unwrap().camera.zoom - 2.0).abs() < 1e-6);
        app.close_canvas();
        assert!(!app.canvas_pan(1.0, 1.0));
        assert!(!app.canvas_zoom(2.0, 0.0, 0.0));
    }

    #[test]
    fn relation_card_height_tracks_snapshot_line_count() {
        let mut app = app_with_text("fn focal() {}\n");
        app.open_canvas(VW, VH, LH);
        app.canvas_add_relations(vec![
            (BlockRelation::Caller, origin_at("/p/a.rs", 10), snap_lines(3)),
            (BlockRelation::Caller, origin_at("/p/b.rs", 20), snap_lines(13)),
        ]);
        let canvas = app.canvas().unwrap();
        let short = &canvas.blocks[1];
        let tall = &canvas.blocks[2];
        // A 13-line card must be taller than a 3-line card (height tracks content).
        assert!(
            short.world.h < tall.world.h,
            "short card ({}) should be shorter than tall card ({})",
            short.world.h,
            tall.world.h
        );
        // The tall card stacks fully below the short card — no overlap.
        assert!(
            tall.world.y >= short.world.y + short.world.h,
            "cards overlap: short ends at {}, tall starts at {}",
            short.world.y + short.world.h,
            tall.world.y
        );
    }

    #[test]
    fn references_declaration_does_not_duplicate_definition() {
        let mut app = app_with_text("fn focal() {}\n");
        app.open_canvas(VW, VH, LH);
        // Definition resolves to a.rs:10.
        app.canvas_add_relations(vec![(
            BlockRelation::Definition,
            origin_at("/p/a.rs", 10),
            snap_lines(4),
        )]);
        // gopls `references` returns the declaration (a.rs:10) plus a real call site.
        app.canvas_add_relations(vec![
            (BlockRelation::Caller, origin_at("/p/a.rs", 10), snap_lines(4)),
            (BlockRelation::Caller, origin_at("/p/b.rs", 20), snap_lines(4)),
        ]);
        let canvas = app.canvas().unwrap();
        let non_focal: Vec<_> = canvas
            .blocks
            .iter()
            .filter(|b| b.relation != BlockRelation::Focal)
            .collect();
        // def(a.rs:10) + caller(b.rs:20): the duplicate declaration is dropped.
        assert_eq!(non_focal.len(), 2, "declaration must not create a duplicate card");
        let at_decl = non_focal
            .iter()
            .find(|b| b.origin.lsp_line == 10)
            .expect("a.rs:10 block present");
        assert_eq!(at_decl.relation, BlockRelation::Definition);
    }

    #[test]
    fn definition_upgrades_a_previously_added_caller_at_the_same_site() {
        let mut app = app_with_text("fn focal() {}\n");
        app.open_canvas(VW, VH, LH);
        // References arrive first: declaration (a.rs:10) + a real caller (b.rs:20).
        app.canvas_add_relations(vec![
            (BlockRelation::Caller, origin_at("/p/a.rs", 10), snap_lines(4)),
            (BlockRelation::Caller, origin_at("/p/b.rs", 20), snap_lines(4)),
        ]);
        // Then the definition resolves to the same site — it supersedes the caller.
        app.canvas_add_relations(vec![(
            BlockRelation::Definition,
            origin_at("/p/a.rs", 10),
            snap_lines(4),
        )]);
        let canvas = app.canvas().unwrap();
        let non_focal: Vec<_> = canvas
            .blocks
            .iter()
            .filter(|b| b.relation != BlockRelation::Focal)
            .collect();
        assert_eq!(non_focal.len(), 2, "definition must not duplicate an existing caller site");
        let at_decl = non_focal
            .iter()
            .find(|b| b.origin.lsp_line == 10)
            .expect("a.rs:10 block present");
        assert_eq!(
            at_decl.relation,
            BlockRelation::Definition,
            "definition should supersede the caller at the same site"
        );
    }

    #[test]
    fn add_relations_skips_the_focal_location() {
        let mut app = app_with_text("fn focal() {}\n");
        app.open_canvas(VW, VH, LH);
        // Provenance of the focal block (the cursor in the live editor).
        let (fp, fl) = {
            let c = app.canvas().unwrap();
            let f = &c.blocks[0];
            (f.origin.path.to_string_lossy().into_owned(), f.origin.lsp_line)
        };
        app.canvas_add_relations(vec![
            // A reference AT the focal location — already visible in the editor.
            (BlockRelation::Caller, origin_at(&fp, fl), snap("self")),
            // A real caller elsewhere — must still appear.
            (BlockRelation::Caller, origin_at("/p/other.rs", 99), snap("other")),
        ]);
        let c = app.canvas().unwrap();
        let non_focal: Vec<_> = c
            .blocks
            .iter()
            .filter(|b| b.relation != BlockRelation::Focal)
            .collect();
        assert_eq!(non_focal.len(), 1, "the focal-location reference should be skipped");
        assert_eq!(non_focal[0].origin.lsp_line, 99);
    }

    #[test]
    fn change_focused_height_only_affects_focused_card() {
        let mut app = app_with_text("fn focal() {}\n");
        app.open_canvas(VW, VH, LH);
        app.canvas_add_relations(vec![
            (BlockRelation::Caller, origin_at("/p/a.rs", 10), snap_lines(5)),
            (BlockRelation::Caller, origin_at("/p/b.rs", 20), snap_lines(5)),
        ]);
        // The first relation card is auto-focused.
        let focused = app.canvas().unwrap().focused.unwrap();
        // Capture the OTHER card's full rect and the focused card's size/width.
        let other_before = {
            let c = app.canvas().unwrap();
            c.blocks
                .iter()
                .find(|b| b.id != focused && b.relation != BlockRelation::Focal)
                .unwrap()
                .world
        };
        let (fw0, fh0) = {
            let b = app.canvas().unwrap().block(focused).unwrap();
            (b.world.w, b.world.h)
        };
        // `=` grows ONLY the focused card's HEIGHT in place — no re-source.
        assert!(app.canvas_change_focused_height(4));
        let c = app.canvas().unwrap();
        let fb = c.block(focused).unwrap();
        assert_eq!(fb.height_rows, Some(5 + 4), "rows = snapshot size + delta");
        // Focused card grew taller; WIDTH stays fixed (no box scaling).
        assert!(fb.world.h > fh0, "focused card should grow taller");
        assert!((fb.world.w - fw0).abs() < 1e-3, "width must stay fixed");
        // The OTHER card is untouched — position AND size (no re-stack).
        let other_after = c
            .blocks
            .iter()
            .find(|b| b.id != focused && b.relation != BlockRelation::Focal)
            .unwrap()
            .world;
        assert_eq!(
            (other_after.x, other_after.y, other_after.w, other_after.h),
            (other_before.x, other_before.y, other_before.w, other_before.h),
            "a non-focused card must not move or resize on a =/-"
        );
    }

    #[test]
    fn change_focused_height_resizes_in_place_keeping_top_left() {
        let mut app = app_with_text("fn focal() {}\n");
        app.open_canvas(VW, VH, LH);
        app.canvas_add_relations(vec![(
            BlockRelation::Caller,
            origin_at("/p/a.rs", 10),
            snap_lines(5),
        )]);
        let focused = app.canvas().unwrap().focused.unwrap();
        // Move the focused card somewhere arbitrary first (hand-arrangement).
        assert!(app.canvas_move_focused(150.0, 90.0));
        let (x0, y0, h0) = {
            let b = app.canvas().unwrap().block(focused).unwrap();
            (b.world.x, b.world.y, b.world.h)
        };
        assert!(app.canvas_change_focused_height(6));
        let b = app.canvas().unwrap().block(focused).unwrap();
        // Top-left pinned; only height changed (grew downward).
        assert_eq!((b.world.x, b.world.y), (x0, y0), "top-left must stay fixed");
        assert!(b.world.h > h0, "card grows downward with more rows");
    }

    #[test]
    fn canvas_apply_card_scope_replaces_snapshot() {
        let mut app = app_with_text("fn focal() {\n    bar();\n}\n");
        app.open_canvas(VW, VH, LH);
        app.canvas_add_relations(vec![(
            BlockRelation::Caller,
            origin_at("/p/a.rs", 10),
            snap("a"),
        )]);
        let id = app.canvas().unwrap().focused.unwrap();
        let new_snap = BlockSnapshot {
            title: "a.rs".into(),
            symbol: "s".into(),
            start_line: 8,
            text: "fn s() {\n  x\n}".into(),
            spans: Vec::new(),
        };
        assert!(app.canvas_apply_card_scope(id, new_snap.clone(), Some((7, 9))));
        let b = app.canvas().unwrap().blocks.iter().find(|b| b.id == id).unwrap();
        assert_eq!(b.snapshot.start_line, 8);
        assert_eq!(b.snapshot.text, "fn s() {\n  x\n}");
        assert_eq!(b.scope_lines, Some((7, 9)), "scope range bounds the edit viewport");
    }

    #[test]
    fn canvas_apply_card_scope_refuses_focal_anchor() {
        let mut app = app_with_text("fn focal() {\n    bar();\n}\n");
        app.open_canvas(VW, VH, LH);
        let focal_id = app.canvas().unwrap().focused.unwrap();
        let new_snap = BlockSnapshot {
            title: "f.rs".into(),
            symbol: "s".into(),
            start_line: 8,
            text: "fn s() {\n  x\n}".into(),
            spans: Vec::new(),
        };
        // The focal anchor is the editor itself (never drawn as a card); applying
        // a scope snapshot to it must be refused.
        assert!(!app.canvas_apply_card_scope(focal_id, new_snap, None));
    }

    #[test]
    fn canvas_apply_card_scope_drops_stale_card_id() {
        let mut app = app_with_text("fn focal() {}\n");
        app.open_canvas(VW, VH, LH);
        // No relation card exists with this id → false (card was closed/opened-as-tab).
        let new_snap = BlockSnapshot {
            title: "f.rs".into(),
            symbol: "s".into(),
            start_line: 8,
            text: "fn s() {}".into(),
            spans: Vec::new(),
        };
        assert!(!app.canvas_apply_card_scope(9999, new_snap, None));
    }

    #[test]
    fn edit_session_window_preserves_scope_snapshot_range() {
        use std::time::{SystemTime, UNIX_EPOCH};
        let nanos = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
        let dir = std::env::temp_dir().join(format!("netherize_canvas_scope_edit_{nanos}"));
        std::fs::create_dir_all(&dir).unwrap();
        let foo = dir.join("foo.rs");
        let bar = dir.join("bar.rs");
        std::fs::write(&foo, "fn main() {\n    helper();\n}\n").unwrap();
        let body = (0..20).map(|i| format!("line {i}")).collect::<Vec<_>>().join("\n");
        std::fs::write(&bar, body).unwrap();

        let mut app = AppState::new(foo.clone());
        app.open_file(foo).expect("open foo");
        assert!(app.open_canvas(VW, VH, LH));
        let scope_snap = BlockSnapshot {
            title: "bar.rs".into(),
            symbol: "helper".into(),
            start_line: 6,
            text: "line 5\nline 6\nline 7".into(),
            spans: Vec::new(),
        };
        app.canvas_add_relations(vec![
            (BlockRelation::Definition, origin_at(bar.to_string_lossy().as_ref(), 5), scope_snap),
        ]);

        assert!(app.canvas_begin_edit());
        let block = app.canvas_edit_session_block().unwrap();
        let (lines, start0, _cursor_line, _cursor_col) = app
            .canvas_edit_session_snapshot_window(block)
            .expect("scope window");
        assert_eq!(start0, 5);
        assert_eq!(lines, vec!["line 5", "line 6", "line 7"]);

        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn change_focused_height_clamps_and_reports_false_at_bounds() {
        let mut app = app_with_text("fn focal() {}\n");
        app.open_canvas(VW, VH, LH);
        app.canvas_add_relations(vec![(
            BlockRelation::Caller,
            origin_at("/p/a.rs", 10),
            snap_lines(5),
        )]);
        let focused = app.canvas().unwrap().focused.unwrap();
        // Effective rows start at the snapshot size (5). Shrinking floors at
        // CARD_MIN_LINES (3), then no-ops (false).
        assert!(app.canvas_change_focused_height(-2)); // 5 -> 3 (MIN)
        assert_eq!(app.canvas().unwrap().block(focused).unwrap().height_rows, Some(3));
        assert!(!app.canvas_change_focused_height(-2)); // already at MIN -> false
        // Climbing hits the ceiling (CARD_MAX_LINES) and then reports false.
        for _ in 0..40 {
            app.canvas_change_focused_height(2);
        }
        assert_eq!(
            app.canvas().unwrap().block(focused).unwrap().height_rows,
            Some(CARD_MAX_LINES)
        );
        assert!(!app.canvas_change_focused_height(2)); // at MAX -> false
    }

    #[test]
    fn change_focused_width_resizes_in_place_and_clamps() {
        let mut app = app_with_text("fn focal() {}\n");
        app.open_canvas(VW, VH, LH);
        app.canvas_add_relations(vec![(
            BlockRelation::Caller,
            origin_at("/p/a.rs", 10),
            snap_lines(5),
        )]);
        let focused = app.canvas().unwrap().focused.unwrap();
        let (x0, y0, w0) = {
            let b = app.canvas().unwrap().block(focused).unwrap();
            (b.world.x, b.world.y, b.world.w)
        };
        // Shift+`+` widens the focused card in place.
        assert!(app.canvas_change_focused_width(80.0));
        let b = app.canvas().unwrap().block(focused).unwrap();
        assert!((b.world.w - (w0 + 80.0)).abs() < 1e-3, "width grows by the step");
        assert_eq!((b.world.x, b.world.y), (x0, y0), "top-left stays fixed");
        assert!(
            app.canvas().unwrap().user_arranged,
            "a width change freezes auto-arrange"
        );
        // Clamps at the minimum, then reports no change.
        for _ in 0..50 {
            app.canvas_change_focused_width(-80.0);
        }
        assert!(!app.canvas_change_focused_width(-80.0), "no-op at min width");
    }

    #[test]
    fn in_card_spawn_focuses_new_card_and_records_parent() {
        let mut app = app_with_text("fn focal() {}\n");
        app.open_canvas(VW, VH, LH);
        // An initial card spawned from the focal symbol.
        app.canvas_add_relations(vec![(
            BlockRelation::Caller,
            origin_at("/p/a.rs", 10),
            snap("a"),
        )]);
        let parent = app.canvas().unwrap().focused.unwrap();
        assert_eq!(app.canvas().unwrap().block(parent).unwrap().parent, None);

        // A card spawned FROM that card (in-card gd/gr) records its parent and
        // steals focus to the new card.
        app.canvas_add_relations_with_parent(
            vec![(BlockRelation::Definition, origin_at("/p/b.rs", 20), snap("b"))],
            Some(parent),
        );
        let c = app.canvas().unwrap();
        let focused = c.focused.unwrap();
        assert_ne!(focused, parent, "focus jumps to the spawned card");
        let new_block = c.block(focused).unwrap();
        assert_eq!(new_block.parent, Some(parent), "new card records its parent");
        assert_eq!(new_block.origin.lsp_line, 20);
    }

    #[test]
    fn canvas_open_card_buffer_helpers_and_stash() {
        let mut app = app_with_text("fn focal() {\n    bar();\n}\n");
        let focal_file = app.active_file().unwrap().to_path_buf();
        assert!(app.open_canvas(VW, VH, LH));

        // Stash round-trips and starts off.
        assert!(!app.canvas_is_stashed());
        assert!(app.canvas_set_stashed(true));
        assert!(app.canvas_is_stashed());
        assert!(app.canvas_set_stashed(false));
        assert!(!app.canvas_is_stashed());

        // Focused on the focal anchor → nothing to open as a tab.
        assert_eq!(app.canvas_focused_card_origin(), None);
        // Focal file = the gc-origin file; editor shows it → focal position available.
        assert_eq!(app.canvas_focal_file().as_deref(), Some(focal_file.as_path()));
        assert!(app.canvas_focal_position().is_some());

        // A relation card is focused → it is openable as a tab.
        app.canvas_add_relations(vec![(
            BlockRelation::Caller,
            origin_at("/p/a.rs", 10),
            snap("a"),
        )]);
        let card = app.canvas_focused_card_origin().expect("focused card origin");
        assert_eq!(card.path, std::path::PathBuf::from("/p/a.rs"));
        assert_eq!(card.lsp_line, 10);

        // After opening that card as a tab the editor shows a DIFFERENT file, so the
        // focal position vanishes (connector falls back to the caret) — it must not
        // project the focal line onto the wrong buffer.
        app.active_file = Some(std::path::PathBuf::from("/p/a.rs"));
        assert_eq!(app.canvas_focal_position(), None);
    }

    #[test]
    fn canvas_should_render_tracks_active_file_and_stash() {
        let mut app = app_with_text("fn focal() {\n    bar();\n}\n");
        let focal_file = app.active_file().unwrap().to_path_buf();

        // No canvas yet → nothing to render.
        assert!(!app.canvas_should_render());

        assert!(app.open_canvas(VW, VH, LH));
        // Editor is on the focal (gc-origin) file → render.
        assert_eq!(app.canvas_focal_file().as_deref(), Some(focal_file.as_path()));
        assert!(app.canvas_should_render());

        // Stashed (a card opened as a tab via `o`) → never render; un-stash restores.
        app.canvas_set_stashed(true);
        assert!(!app.canvas_should_render());
        app.canvas_set_stashed(false);
        assert!(app.canvas_should_render());

        // Switch to another file (file picker / Ctrl-h / Ctrl-l) → hide, but KEEP it.
        app.active_file = Some(PathBuf::from("/proj/src/other.rs"));
        assert!(!app.canvas_should_render());
        assert!(app.canvas().is_some(), "canvas kept in state when hidden");

        // Switch back to the focal file → shows again.
        app.active_file = Some(focal_file);
        assert!(app.canvas_should_render());
    }

    #[test]
    fn end_edit_for_spawn_leaves_editcard_so_focus_lands_on_child() {
        use std::time::{SystemTime, UNIX_EPOCH};
        let nanos = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
        let dir = std::env::temp_dir().join(format!("netherize_spawn_focus_{nanos}"));
        std::fs::create_dir_all(&dir).unwrap();
        let foo = dir.join("foo.rs");
        let bar = dir.join("bar.rs");
        std::fs::write(&foo, "fn main() {\n    helper();\n}\n").unwrap();
        std::fs::write(&bar, "fn helper() {\n    let x = 1;\n}\n").unwrap();

        let mut app = AppState::new(foo.clone());
        app.open_file(foo.clone()).expect("open foo");
        assert!(app.open_canvas(VW, VH, LH));
        let bar_canon = bar.canonicalize().unwrap();
        app.canvas_add_relations(vec![(
            BlockRelation::Definition,
            BlockOrigin {
                path: bar_canon.clone(),
                start_byte: 0,
                end_byte: 1,
                symbol_name: "helper".into(),
                lsp_line: 0,
                lsp_character: 0,
            },
            snap("helper"),
        )]);
        let parent = app.canvas().unwrap().focused.unwrap();

        // Edit the parent card (EditCard).
        assert!(app.canvas_begin_edit());
        assert!(matches!(
            app.canvas_interaction(),
            Some(CanvasInteraction::EditCard { block, .. }) if block == parent
        ));

        // In-card gd/gr spawns a child card (focus jumps to it in canvas state).
        app.canvas_add_relations_with_parent(
            vec![(BlockRelation::Definition, origin_at("/p/child.rs", 20), snap("child"))],
            Some(parent),
        );
        let child = app.canvas().unwrap().focused.unwrap();
        assert_ne!(child, parent, "focus jumps to the spawned child in canvas state");

        // Guard: a non-matching parent (user moved on) must NOT end the edit.
        assert!(!app.canvas_end_edit_for_spawn(child));
        assert!(matches!(
            app.canvas_interaction(),
            Some(CanvasInteraction::EditCard { .. })
        ));

        // The real spawning parent → leave EditCard so the focus visibly lands on
        // the child (no Esc needed). Focus stays on the child.
        assert!(app.canvas_end_edit_for_spawn(parent));
        assert_eq!(app.canvas_interaction(), Some(CanvasInteraction::Navigate));
        assert_eq!(app.canvas().unwrap().focused, Some(child));

        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn change_focused_height_is_false_for_focal_anchor() {
        let mut app = app_with_text("fn focal() {}\n");
        app.open_canvas(VW, VH, LH);
        // Only the focal anchor exists and is focused — it isn't drawn, so `=`/`-`
        // must be a no-op rather than resizing the invisible anchor.
        assert!(!app.canvas_change_focused_height(2));
    }

    #[test]
    fn auto_arrange_wraps_into_columns_and_freezes_after_manual_layout() {
        let mut app = app_with_text("fn focal() {}\n");
        app.open_canvas(VW, VH, LH);
        let rels: Vec<_> = (0..8)
            .map(|i| {
                (
                    BlockRelation::Caller,
                    origin_at(&format!("/p/f{i}.rs"), (i * 10) as u32),
                    snap_lines(3),
                )
            })
            .collect();
        app.canvas_add_relations(rels);
        assert!(app.canvas_auto_arrange(VH));
        let c = app.canvas().unwrap();
        let mut xs: Vec<i64> = c
            .blocks
            .iter()
            .filter(|b| b.relation != BlockRelation::Focal)
            .map(|b| b.world.x.round() as i64)
            .collect();
        xs.sort_unstable();
        xs.dedup();
        assert!(xs.len() >= 2, "cards should wrap into >= 2 columns, got {} columns", xs.len());
        // No vertical overlap within the first column.
        let col0 = xs[0];
        let mut col0_cards: Vec<(f32, f32)> = c
            .blocks
            .iter()
            .filter(|b| b.relation != BlockRelation::Focal && b.world.x.round() as i64 == col0)
            .map(|b| (b.world.y, b.world.h))
            .collect();
        col0_cards.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
        for w in col0_cards.windows(2) {
            assert!(w[1].0 >= w[0].0 + w[0].1, "cards overlap within a column");
        }
        // After a manual move, auto-arrange freezes (manual placement wins).
        assert!(app.canvas_move_focused(10.0, 10.0));
        assert!(
            !app.canvas_auto_arrange(VH),
            "auto-arrange must freeze after a manual move"
        );
    }

    #[test]
    fn edit_card_round_trip_keeps_main_editor_on_origin() {
        use std::time::{SystemTime, UNIX_EPOCH};
        let nanos = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
        let dir = std::env::temp_dir().join(format!("netherize_canvas_edit_{nanos}"));
        std::fs::create_dir_all(&dir).unwrap();
        let foo = dir.join("foo.rs");
        let bar = dir.join("bar.rs");
        std::fs::write(&foo, "fn main() {\n    helper();\n}\n").unwrap();
        std::fs::write(&bar, "fn helper() {\n    let x = 1;\n}\n").unwrap();

        let mut app = AppState::new(foo.clone());
        app.open_file(foo.clone()).expect("open foo");
        let foo_active = app.active_file().unwrap().to_path_buf();
        let main_text_before = app.text_string();

        assert!(app.open_canvas(VW, VH, LH));
        let bar_canon = bar.canonicalize().unwrap();
        app.canvas_add_relations(vec![(
            BlockRelation::Definition,
            BlockOrigin {
                path: bar_canon.clone(),
                start_byte: 0,
                end_byte: 1,
                symbol_name: "helper".into(),
                lsp_line: 0,
                lsp_character: 0,
            },
            snap("helper"),
        )]);
        let card = app.canvas().unwrap().focused.unwrap();

        // Enter the card → a session is created; the MAIN editor never switches
        // (this is the v2 fix: the main editor must keep showing the origin file).
        assert!(app.canvas_begin_edit());
        assert_eq!(app.canvas_edit_session_block(), Some(card));
        assert_eq!(
            app.active_file().unwrap(),
            foo_active.as_path(),
            "main editor must stay on the gc-origin file"
        );
        assert_eq!(app.text_string(), main_text_before, "main buffer untouched");
        assert!(matches!(
            app.canvas_interaction(),
            Some(CanvasInteraction::EditCard { .. })
        ));

        // Esc out → Navigate (card navigation; a 2nd Esc backgrounds to the
        // editor). The clean session is KEPT (per the cursor-restore fix:
        // re-entering the same card resumes the cursor rather than resetting to
        // the origin lsp_line/lsp_character).
        assert!(app.canvas_end_edit());
        assert_eq!(app.active_file().unwrap(), foo_active.as_path());
        assert_eq!(app.text_string(), main_text_before);
        assert_eq!(app.canvas_interaction(), Some(CanvasInteraction::Navigate));
        assert_eq!(app.canvas_edit_session_block(), Some(card));

        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn reentering_clean_card_edit_restores_last_cursor_position() {
        use std::time::{SystemTime, UNIX_EPOCH};
        let nanos = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
        let dir = std::env::temp_dir().join(format!("netherize_canvas_cursor_{nanos}"));
        std::fs::create_dir_all(&dir).unwrap();
        let foo = dir.join("foo.rs");
        let bar = dir.join("bar.rs");
        std::fs::write(&foo, "fn main() {\n    helper();\n}\n").unwrap();
        std::fs::write(&bar, "fn helper() {\n    let value = 1;\n}\n").unwrap();

        let mut app = AppState::new(foo.clone());
        app.open_file(foo.clone()).expect("open foo");
        assert!(app.open_canvas(VW, VH, LH));
        let bar_canon = bar.canonicalize().unwrap();
        app.canvas_add_relations(vec![(
            BlockRelation::Definition,
            BlockOrigin {
                path: bar_canon,
                start_byte: 0,
                end_byte: 1,
                symbol_name: "helper".into(),
                lsp_line: 0,
                lsp_character: 0,
            },
            snap("helper"),
        )]);

        assert!(app.canvas_begin_edit());
        let mut session = app.take_canvas_edit_session().expect("card session");
        app.swap_canvas_edit_session(&mut session);
        assert!(app.jump_to_line_and_column(1, 4));
        app.swap_canvas_edit_session(&mut session);
        app.put_canvas_edit_session(session);
        app.canvas_update_edit_card(1, 1, String::new(), Vec::new(), 1, 4);

        assert!(app.canvas_end_edit());
        assert!(app.canvas_begin_edit());

        assert_eq!(
            app.canvas_interaction(),
            Some(CanvasInteraction::EditCard {
                block: 1,
                cursor_line: 1,
                cursor_col: 4,
            })
        );

        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn card_lsp_target_only_for_a_card_we_must_register() {
        use std::time::{SystemTime, UNIX_EPOCH};
        let nanos = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
        let dir = std::env::temp_dir().join(format!("netherize_card_lsp_{nanos}"));
        std::fs::create_dir_all(&dir).unwrap();
        let foo = dir.join("foo.rs");
        let bar = dir.join("bar.rs");
        std::fs::write(&foo, "fn main() {\n    helper();\n}\n").unwrap();
        std::fs::write(&bar, "fn helper() {\n    let x = 1;\n}\n").unwrap();

        // No session → nothing to register.
        let mut app = AppState::new(foo.clone());
        app.open_file(foo.clone()).expect("open foo");
        assert_eq!(app.canvas_card_lsp_target(), None, "no session → None");

        assert!(app.open_canvas(VW, VH, LH));
        let bar_canon = bar.canonicalize().unwrap();
        app.canvas_add_relations(vec![(
            BlockRelation::Definition,
            BlockOrigin {
                path: bar_canon.clone(),
                start_byte: 0,
                end_byte: 1,
                symbol_name: "helper".into(),
                lsp_line: 0,
                lsp_character: 0,
            },
            snap("helper"),
        )]);
        assert!(app.canvas_begin_edit());

        // bar.rs is NOT the active file and NOT an open buffer → we own its LSP
        // lifecycle, so gd/gr can resolve against it.
        assert_eq!(
            app.canvas_card_lsp_target(),
            Some(bar_canon.clone()),
            "card file (≠ active, not open) must be registered by us"
        );

        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn card_lsp_target_is_none_when_file_already_open() {
        use std::time::{SystemTime, UNIX_EPOCH};
        let nanos = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
        let dir = std::env::temp_dir().join(format!("netherize_card_lsp_open_{nanos}"));
        std::fs::create_dir_all(&dir).unwrap();
        let foo = dir.join("foo.rs");
        let bar = dir.join("bar.rs");
        std::fs::write(&foo, "fn main() {\n    helper();\n}\n").unwrap();
        std::fs::write(&bar, "fn helper() {\n    let x = 1;\n}\n").unwrap();

        let mut app = AppState::new(foo.clone());
        // Open BOTH files: bar becomes a background text buffer, foo is active.
        app.open_file(bar.clone()).expect("open bar");
        app.open_file(foo.clone()).expect("open foo");
        let bar_canon = bar.canonicalize().unwrap();

        assert!(app.open_canvas(VW, VH, LH));
        app.canvas_add_relations(vec![(
            BlockRelation::Definition,
            BlockOrigin {
                path: bar_canon.clone(),
                start_byte: 0,
                end_byte: 1,
                symbol_name: "helper".into(),
                lsp_line: 0,
                lsp_character: 0,
            },
            snap("helper"),
        )]);
        assert!(app.canvas_begin_edit());

        // bar.rs is already an open buffer (the main editor's LSP owns it) → we
        // must NOT re-open or later didClose it.
        assert_eq!(
            app.canvas_card_lsp_target(),
            None,
            "card file already open as a buffer → not ours to manage"
        );

        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn card_completion_context_extracts_identifier_prefix() {
        use std::time::{SystemTime, UNIX_EPOCH};
        let nanos = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
        let dir = std::env::temp_dir().join(format!("netherize_card_ctx_{nanos}"));
        std::fs::create_dir_all(&dir).unwrap();
        let foo = dir.join("foo.rs");
        let bar = dir.join("bar.rs");
        std::fs::write(&foo, "fn main() {}\n").unwrap();
        std::fs::write(&bar, "fn helper() {\n}\n").unwrap();

        let mut app = AppState::new(foo.clone());
        app.open_file(foo.clone()).expect("open foo");
        assert!(app.open_canvas(VW, VH, LH));
        let bar_canon = bar.canonicalize().unwrap();
        app.canvas_add_relations(vec![(
            BlockRelation::Definition,
            BlockOrigin {
                path: bar_canon.clone(),
                start_byte: 0,
                end_byte: 1,
                symbol_name: "helper".into(),
                lsp_line: 0,
                lsp_character: 5, // after "fn he"
            },
            snap("helper"),
        )]);
        assert!(app.canvas_begin_edit());

        // Line 0 = "fn helper() {"; cursor at col 5 → identifier prefix "he",
        // starting at col 3.
        let (line, col, start, prefix) =
            app.canvas_edit_session_completion_context().expect("ctx");
        assert_eq!(line, 0);
        assert_eq!(col, 5);
        assert_eq!(start, 3);
        assert_eq!(prefix, "he");

        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn card_edit_swap_isolates_main_buffer_and_saves_card_file() {
        use std::time::{SystemTime, UNIX_EPOCH};
        let nanos = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
        let dir = std::env::temp_dir().join(format!("netherize_canvas_swap_{nanos}"));
        std::fs::create_dir_all(&dir).unwrap();
        let foo = dir.join("foo.rs");
        let bar = dir.join("bar.rs");
        std::fs::write(&foo, "fn main() {\n    helper();\n}\n").unwrap();
        std::fs::write(&bar, "fn helper() {\n    let x = 1;\n}\n").unwrap();

        let mut app = AppState::new(foo.clone());
        app.open_file(foo.clone()).expect("open foo");
        let main_before = app.text_string();
        let main_cursor_before = app.cursor_char_idx();

        assert!(app.open_canvas(VW, VH, LH));
        let bar_canon = bar.canonicalize().unwrap();
        app.canvas_add_relations(vec![(
            BlockRelation::Definition,
            BlockOrigin {
                path: bar_canon.clone(),
                start_byte: 0,
                end_byte: 1,
                symbol_name: "helper".into(),
                lsp_line: 0,
                lsp_character: 0,
            },
            snap("helper"),
        )]);
        assert!(app.canvas_begin_edit());

        // Pre-seed MAIN-editor transient state that a card edit must NOT corrupt
        // (the edit path clears folds, pushes highlight edits, and motions push
        // jumps / overwrite the search query on `self`).
        app.folded_ranges = vec![(0, 1)];
        app.jump_back_stack = vec![(foo.clone(), 5, 2)];
        app.last_search_query = "needle".to_string();

        // Drive a real editor edit against the card via the scoped swap, exactly
        // as the dispatch hook does: take → swap in → edit → swap out → put back.
        let mut session = app.take_canvas_edit_session().expect("session exists");
        app.swap_canvas_edit_session(&mut session); // card swapped IN
        app.insert_char('Z'); // edits the CARD's text, not the main buffer
        app.swap_canvas_edit_session(&mut session); // swap OUT
        app.put_canvas_edit_session(session);

        // The MAIN buffer + cursor are byte-for-byte unchanged.
        assert_eq!(app.text_string(), main_before, "card edit must not touch the main buffer");
        assert_eq!(app.cursor_char_idx(), main_cursor_before, "main cursor unmoved");
        // The MAIN editor's folds, jump list, and search query survive the card
        // edit (would be cleared/overwritten if these weren't in the scoped swap).
        assert_eq!(app.folded_ranges, vec![(0, 1)], "card edit must not clear the main editor's folds");
        assert_eq!(app.jump_back_stack, vec![(foo.clone(), 5, 2)], "main jump list preserved");
        assert_eq!(app.last_search_query, "needle", "main search query preserved");

        // ⌘S in the card writes the edit to the CARD file (not the main file).
        assert!(app.canvas_save_edit_session());
        let bar_after = std::fs::read_to_string(&bar).unwrap();
        assert!(bar_after.starts_with('Z'), "card save writes the edit to the card file: {bar_after:?}");
        let foo_after = std::fs::read_to_string(&foo).unwrap();
        assert_eq!(foo_after, main_before, "the origin file on disk is untouched");

        let _ = std::fs::remove_dir_all(dir);
    }
}
