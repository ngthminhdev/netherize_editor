use super::*;

use crate::canvas::Dir;

/// World-units the camera pans per keypress.
const CANVAS_PAN_STEP: f32 = 120.0;
/// Multiplicative zoom step per `+`/`-` press.
const CANVAS_ZOOM_STEP: f32 = 1.15;

impl AppShell {
    /// Handle NetherCanvas commands. Returns `Some(changed)` when the command is
    /// a canvas command, `None` otherwise (so the dispatcher tries other groups).
    pub(super) fn handle_canvas_command(&mut self, command: &Command) -> Option<bool> {
        let changed = match command {
            Command::CanvasOpen => self.open_canvas_mode(),
            Command::CanvasClose => self.app_state.close_canvas(),
            Command::CanvasFocusLeft => self.app_state.canvas_focus_dir(Dir::Left),
            Command::CanvasFocusRight => self.app_state.canvas_focus_dir(Dir::Right),
            Command::CanvasFocusUp => self.app_state.canvas_focus_dir(Dir::Up),
            Command::CanvasFocusDown => self.app_state.canvas_focus_dir(Dir::Down),
            Command::CanvasCycleNext => self.app_state.canvas_cycle(true),
            Command::CanvasCyclePrev => self.app_state.canvas_cycle(false),
            // Enter brings both the definition and the callers.
            Command::CanvasSpawnRelations => {
                let d = self.canvas_submit_definition();
                let r = self.canvas_submit_references();
                d || r
            }
            // E → definition (what the symbol resolves to), R → callers (refs).
            // (callHierarchy callee/caller is a later refinement.)
            Command::CanvasExpandCallee => self.canvas_submit_definition(),
            Command::CanvasExpandCaller => self.canvas_submit_references(),
            Command::CanvasTogglePin => false,
            Command::CanvasZoomIn => self.canvas_zoom(CANVAS_ZOOM_STEP),
            Command::CanvasZoomOut => self.canvas_zoom(1.0 / CANVAS_ZOOM_STEP),
            Command::CanvasPanLeft => self.app_state.canvas_pan(-CANVAS_PAN_STEP, 0.0),
            Command::CanvasPanRight => self.app_state.canvas_pan(CANVAS_PAN_STEP, 0.0),
            Command::CanvasPanUp => self.app_state.canvas_pan(0.0, -CANVAS_PAN_STEP),
            Command::CanvasPanDown => self.app_state.canvas_pan(0.0, CANVAS_PAN_STEP),
            _ => return None,
        };
        if changed {
            self.request_redraw();
        }
        Some(changed)
    }

    fn open_canvas_mode(&mut self) -> bool {
        let (bw, bh) = self.canvas_block_size();
        if !self.app_state.open_canvas(bw, bh) {
            return false;
        }
        let w = self.window_size.width as f32;
        let h = self.window_size.height as f32;
        self.app_state.canvas_center_on_focus(w, h);
        true
    }

    /// World size of every card, derived from the editor font so code fits at
    /// zoom 1 (cards render at the editor's own font size — like a mini editor).
    fn canvas_block_size(&self) -> (f32, f32) {
        let efs = self.theme.editor.font_size.max(8.0);
        let char_w = efs * 0.6;
        let line_h = efs * 1.4;
        let gutter = 7.0 * char_w; // "▶ 1234  "
        let pad = efs * 1.2;
        let cols = 46.0;
        let lines = 14.0;
        let tab_h = line_h * 1.7;
        let w = gutter + cols * char_w + pad * 2.0;
        let h = tab_h + lines * line_h + pad * 1.5;
        (w, h)
    }

    fn canvas_zoom(&mut self, factor: f32) -> bool {
        let cx = self.window_size.width as f32 * 0.5;
        let cy = self.window_size.height as f32 * 0.5;
        self.app_state.canvas_zoom(factor, cx, cy)
    }

    /// Submit `textDocument/definition` for the cursor symbol (still on the focal
    /// symbol while the canvas is open); the result is routed into the canvas.
    fn canvas_submit_definition(&mut self) -> bool {
        if !self.app_state.is_canvas_active() {
            return false;
        }
        self.force_flush_lsp_did_change_for_active_file();
        let Some((_lang, uri, line, character)) = self.lsp_cursor_context() else {
            return false;
        };
        let request = self.submit(RequestSpec {
            revision_id: 0,
            topic: RequestTopic::LspRequest,
            payload: WorkerRequestPayload::LspDefinitionRequest {
                uri,
                line,
                character,
                jump: false,
            },
        });
        self.canvas_def_request_id = request.map(|r| r.request_id);
        self.canvas_def_request_id.is_some()
    }

    /// Submit `textDocument/references`; results become Caller blocks.
    fn canvas_submit_references(&mut self) -> bool {
        if !self.app_state.is_canvas_active() {
            return false;
        }
        self.force_flush_lsp_did_change_for_active_file();
        let Some((_lang, uri, line, character)) = self.lsp_cursor_context() else {
            return false;
        };
        let request = self.submit(RequestSpec {
            revision_id: 0,
            topic: RequestTopic::LspRequest,
            payload: WorkerRequestPayload::LspReferencesRequest {
                uri,
                line,
                character,
            },
        });
        self.canvas_refs_request_id = request.map(|r| r.request_id);
        self.canvas_refs_request_id.is_some()
    }
}
