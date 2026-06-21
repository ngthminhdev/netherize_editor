use super::*;

use crate::canvas::{CanvasInteraction, Dir};

/// World-units the camera pans per keypress (Shift+hjkl).
const CANVAS_PAN_STEP: f32 = 120.0;
/// World-units the focused card moves per keypress (hjkl).
const CANVAS_MOVE_STEP: f32 = 40.0;
/// Visible rows added/removed from the focused card's HEIGHT per `=`/`-` press
/// (a pure in-place resize — the card already shows only its scope).
const CANVAS_HEIGHT_STEP: i32 = 2;
/// World-units the focused card's WIDTH changes per `Shift`+`+`/`-` press.
const CANVAS_WIDTH_STEP: f32 = 80.0;

impl AppShell {
    /// Handle NetherCanvas commands. Returns `Some(changed)` when the command is
    /// a canvas command, `None` otherwise (so the dispatcher tries other groups).
    pub(super) fn handle_canvas_command(&mut self, command: &Command) -> Option<bool> {
        let changed = match command {
            Command::CanvasOpen => self.open_canvas_mode(),
            Command::CanvasClose => {
                self.dismiss_canvas_card_completion();
                self.canvas_hover_request_id = None;
                self.submit_canvas_card_did_close();
                self.app_state.close_canvas()
            }
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
            Command::CanvasTogglePin => self.app_state.canvas_toggle_pin(),
            Command::CanvasCloseFocused => self.app_state.canvas_close_focused(),
            // Phase B: Enter promotes the focused card to a live mini-editor;
            // Esc-staged exits are routed here from the input layer.
            Command::CanvasEnterEdit => {
                let started = self.app_state.canvas_begin_edit();
                if started {
                    self.canvas_sync_edit_card();
                    // Register the card as an open LSP document so in-card gd/gr
                    // resolve against it (Phase 1 in-card LSP).
                    self.submit_canvas_card_did_open();
                }
                started
            }
            Command::CanvasExitEdit => {
                let ended = self.app_state.canvas_end_edit();
                if ended {
                    self.dismiss_canvas_card_completion();
                    self.canvas_hover_request_id = None;
                    self.submit_canvas_card_did_close();
                }
                ended
            }
            // `o`: open the focused card's file as a real buffer tab + stash canvas.
            Command::CanvasOpenCardBuffer => self.canvas_open_card_as_buffer(),
            Command::CanvasEnterBackground => self.app_state.canvas_enter_background(),
            // `gca`: re-flow the cards into tidy columns (unfreezes user-arranged).
            Command::CanvasAutoArrange => {
                let h = self.canvas_arrange_viewport_height();
                self.app_state.canvas_force_auto_arrange(h)
            }
            Command::CanvasNoop => false,
            // `=`/`-`: resize the focused card's HEIGHT (visible rows) in place.
            // Cards already show only their scope, so this is a pure visual resize
            // — it no longer re-sources extra file context.
            Command::CanvasContextExpand => {
                self.app_state.canvas_change_focused_height(CANVAS_HEIGHT_STEP)
            }
            Command::CanvasContextShrink => {
                self.app_state.canvas_change_focused_height(-CANVAS_HEIGHT_STEP)
            }
            // Shift+`+`/`-`: widen / narrow the focused card (horizontal clip).
            Command::CanvasWidthExpand => self.app_state.canvas_change_focused_width(CANVAS_WIDTH_STEP),
            Command::CanvasWidthShrink => {
                self.app_state.canvas_change_focused_width(-CANVAS_WIDTH_STEP)
            }
            // hjkl move the focused card; Shift+hjkl pan the camera.
            Command::CanvasMoveLeft => self.app_state.canvas_move_focused(-CANVAS_MOVE_STEP, 0.0),
            Command::CanvasMoveRight => self.app_state.canvas_move_focused(CANVAS_MOVE_STEP, 0.0),
            Command::CanvasMoveUp => self.app_state.canvas_move_focused(0.0, -CANVAS_MOVE_STEP),
            Command::CanvasMoveDown => self.app_state.canvas_move_focused(0.0, CANVAS_MOVE_STEP),
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


    /// While editing a card (S2), refresh that card's snapshot + caret from the
    /// **`CanvasEditSession`** (the card's own text/cursor — NOT the active
    /// buffer, which stays the gc-origin file). Windows `2·context+1` lines
    /// around the session cursor so the card scrolls with the caret. No-op unless
    /// we are actively editing a card (`EditCard`) — the session is kept stashed
    /// across Navigate/Background/`o` for resume, so the bare session check is not
    /// enough; syncing then would repaint a stale edit caret on a floating card.
    /// Called after each card-editing command and on the render path; cheap
    /// (re-highlights only the window).
    pub(crate) fn canvas_sync_edit_card(&mut self) {
        if !matches!(
            self.app_state.canvas_interaction(),
            Some(CanvasInteraction::EditCard { .. })
        ) {
            return;
        }
        let Some(block_id) = self.app_state.canvas_edit_session_block() else {
            return;
        };
        let Some((lines, start0, cursor_line, cursor_col)) =
            self.app_state.canvas_edit_session_snapshot_window(block_id)
        else {
            return;
        };
        let text = lines.join("\n");
        let extension = self
            .app_state
            .canvas_edit_session_path()
            .as_deref()
            .and_then(|p| p.extension())
            .and_then(|e| e.to_str())
            .unwrap_or_default()
            .to_string();
        let spans = super::super::async_results::canvas_snapshot_spans(&self.theme, &text, &extension);
        self.app_state.canvas_update_edit_card(
            block_id,
            start0 as u32 + 1,
            text,
            spans,
            cursor_line as u32,
            cursor_col as u32,
        );
    }

    /// `⌘S` inside a card: persist the edit session's file (handled specially —
    /// the card is not a `self.buffers` slot, so the generic save would corrupt
    /// the main editor's buffer). Returns whether a save happened.
    pub(crate) fn canvas_save_edit_card(&mut self) -> bool {
        let saved = self.app_state.canvas_save_edit_session();
        if saved {
            self.canvas_sync_edit_card();
            self.request_redraw();
        }
        saved
    }

    /// Run an editor command against the **card's** edit session via a scoped
    /// swap: take the session out, swap it into the engine (so `self.text` etc.
    /// transiently hold the card), run the **core** dispatch, swap back out
    /// (capturing the edit), and refresh the card. `self.text` (the main editor)
    /// is unchanged on return.
    ///
    /// The card runs the **core** `dispatch_command_with_clipboard_count` — NOT
    /// the shell pipeline `handle_command_with_count_impl` — on purpose: the shell
    /// layer fires LSP `didChange`, syntax parse, highlight-span reconcile, AI
    /// inline, completion and semantic-highlight side effects keyed off the
    /// *active buffer*; running those with the card swapped in would poison the
    /// main editor's highlights and the LSP server's view of the card file. The
    /// card re-highlights itself fully via `canvas_sync_edit_card`, so it needs
    /// none of that. Core dispatch auto-commits transactions, so undo works.
    pub(crate) fn dispatch_card_editing_command(
        &mut self,
        command: Command,
        repeat_count: usize,
    ) -> bool {
        let Some(mut session) = self.app_state.take_canvas_edit_session() else {
            return self.handle_command_with_count_impl(command, repeat_count);
        };
        // A hover popup is one-shot — any edit/motion dismisses it (Phase 2).
        if self
            .app_state
            .canvas()
            .is_some_and(|c| c.card_hover.is_some())
        {
            self.app_state.canvas_clear_card_overlay();
        }
        // Classify the command for completion (Phase 3) before it is moved.
        let completion_trigger_char = match &command {
            Command::InsertChar(ch) => {
                ch.is_alphanumeric() || *ch == '_' || self.lsp_completion_trigger_chars.contains(ch)
            }
            _ => false,
        };
        let is_backspace = matches!(&command, Command::Backspace);
        // A pure motion (h/j/k/l, w/b, gg/G, f/F, %, Ctrl-d/u→Move…) must pan the
        // scope view without parking the caret OUTSIDE the function: clamp it back
        // into the card's scope right after the command (while still swapped IN).
        let is_motion = command.is_card_motion_command();
        let block_id = session.block;

        self.app_state.swap_canvas_edit_session(&mut session); // swap IN
        let report = {
            let (app_state, clipboard) = (&mut self.app_state, &mut self.clipboard);
            crate::core::command_dispatch::dispatch_command_with_clipboard_count(
                app_state,
                command,
                repeat_count,
                Some(clipboard),
            )
        };
        if is_motion {
            self.app_state.canvas_clamp_active_cursor_to_scope(block_id);
        }
        self.app_state.swap_canvas_edit_session(&mut session); // swap OUT
        self.app_state.put_canvas_edit_session(session);
        if report.state_changed {
            self.canvas_sync_edit_card();
            // Keep the LSP server's view of the card in sync so gd/gr/completion
            // resolve against the edited text (Phase 1 in-card LSP).
            self.submit_canvas_card_did_change();
            // In-card completion (Phase 3): (re)request on identifier typing or on
            // delete while a menu is open; any other command dismisses the menu.
            let in_insert = self.app_state.current_mode() == crate::core::mode::EditorMode::Insert;
            if in_insert && completion_trigger_char {
                self.submit_canvas_card_completion();
            } else if in_insert && is_backspace && self.canvas_completion.is_some() {
                self.submit_canvas_card_completion();
            } else if self.canvas_completion.is_some() {
                self.dismiss_canvas_card_completion();
            }
            self.request_redraw();
        }
        report.state_changed
    }

    fn open_canvas_mode(&mut self) -> bool {
        // A STASHED canvas (a card was opened as a tab via `o`) → restore it:
        // un-stash, switch the editor back to the focal (gc-origin) file so the
        // connectors line up, and grab navigation focus.
        if self.app_state.canvas_is_stashed() {
            self.app_state.canvas_set_stashed(false);
            if let Some(focal) = self.app_state.canvas_focal_file()
                && self.app_state.active_file() != Some(focal.as_path())
                && self.app_state.open_file(focal.clone()).is_ok()
            {
                self.invalidate_highlights_and_parse_active_buffer();
                self.submit_lsp_check_for_path(focal.clone());
                self.submit_lsp_did_open_for_active_file();
            }
            let _ = self.app_state.canvas_focus_navigate();
            self.editor_needs_layout = true;
            self.request_redraw();
            return true;
        }
        // Already open: if it owns focus, toggle off; otherwise re-grab navigation
        // focus instead of re-sourcing a fresh canvas.
        if self.app_state.is_canvas_active() {
            if self.app_state.canvas_interaction() == Some(CanvasInteraction::Navigate) {
                self.dismiss_canvas_card_completion();
                self.canvas_hover_request_id = None;
                self.submit_canvas_card_did_close();
                return self.app_state.close_canvas();
            }
            return self.app_state.canvas_focus_navigate();
        }
        let (bw, bh, lh) = self.canvas_block_size();
        if !self.app_state.open_canvas(bw, bh, lh) {
            return false;
        }
        let w = self.window_size.width as f32;
        let h = self.window_size.height as f32;
        self.app_state.canvas_center_on_focus(w, h);
        // F8 shows ONLY the source function (the definition of the symbol under the
        // cursor) — no auto-spawned reference/caller cards. The user spawns those
        // on demand with `gr` (refs) / `gd` (def) once inside the canvas. Results
        // arrive async and are routed in via the request-id redirect.
        self.canvas_submit_definition();
        true
    }

    /// `o`: open the focused relation card's file as a real buffer tab at the
    /// symbol (full editor side effects: LSP + parse + scroll), then STASH the
    /// canvas (hidden, kept) so `gc` restores it. No-op on the focal anchor / with
    /// no focused card. Enter (edit-in-card) is the lighter level; this is "go
    /// deeper / code longer in a real tab".
    fn canvas_open_card_as_buffer(&mut self) -> bool {
        let Some(origin) = self.app_state.canvas_focused_card_origin() else {
            return false;
        };
        if let Err(err) = self.app_state.open_file(origin.path.clone()) {
            eprintln!("[AppShell] canvas open card buffer failed: {err}");
            return false;
        }
        let _ = self
            .app_state
            .jump_to_line_and_column(origin.lsp_line as usize, origin.lsp_character as usize);
        let vp = self.editor_viewport_lines();
        self.app_state.auto_scroll_to_cursor(vp);
        self.invalidate_highlights_and_parse_active_buffer();
        self.submit_lsp_check_for_path(origin.path.clone());
        self.submit_lsp_did_open_for_active_file();
        self.app_state.canvas_set_stashed(true);
        self.focus_manager.set(FocusTarget::CenterEditor);
        self.input_handler.clear_pending_prefix();
        self.editor_needs_layout = true;
        self.editor_caret_needs_layout = false;
        self.request_redraw();
        true
    }

    /// Card geometry derived from the editor font so code fits at zoom 1 (cards
    /// render at the editor's own font size — like a mini editor). Returns
    /// `(width, max_height, line_height)`: `width` is uniform; `max_height` is a
    /// full `CARD_MAX_LINES` card (the focal anchor's size) — relation cards size
    /// to their own content via [`CanvasState::card_height`] using `line_height`.
    /// Visible canvas height in WORLD units for auto-arrange column wrapping: the
    /// editor pane's physical height (NOT the whole window — that includes the
    /// tab/status bars and any bottom panel, which would let a column overflow the
    /// visible area before wrapping) divided by the camera zoom. Falls back to the
    /// window height before the first editor render.
    pub(crate) fn canvas_arrange_viewport_height(&self) -> f32 {
        let pane = self
            .renderer
            .as_ref()
            .and_then(|r| r.canvas_pane_height())
            .unwrap_or(self.window_size.height as f32);
        let zoom = self
            .app_state
            .canvas()
            .map(|c| c.camera.zoom)
            .unwrap_or(1.0)
            .max(0.01);
        pane / zoom
    }

    fn canvas_block_size(&self) -> (f32, f32, f32) {
        let efs = self.theme.editor.font_size.max(8.0);
        let char_w = efs * 0.6;
        // Use the editor's real line height so cards are spaced exactly like the
        // main editor (the renderer scales this same value by zoom).
        let line_h = self.theme.editor.line_height.max(efs);
        let gutter = 7.0 * char_w; // "▶ 1234  "
        let pad = efs * 1.2;
        let cols = 58.0;
        let w = gutter + cols * char_w + pad * 2.0;
        let h = line_h
            * (crate::canvas::CARD_HEADER_LINES
                + crate::canvas::CARD_MAX_LINES as f32
                + crate::canvas::model::CARD_BOTTOM_LINES);
        (w, h, line_h)
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
        self.canvas_def_parent = None; // spawned from the focal symbol
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
        self.canvas_refs_parent = None; // spawned from the focal symbol
        self.canvas_refs_request_id.is_some()
    }

    /// `gd`/`gr` while editing a card: submit definition/references for the
    /// **card's** file at the **session cursor**, so results spawn NEW cards on
    /// the canvas (routed through the same `canvas_def/refs_request_id` redirect)
    /// rather than jumping the main editor. Best-effort: the card file is
    /// resolved by the workspace LSP even when it isn't the active buffer; its
    /// in-session unsaved edits are not synced, so positions reflect the
    /// on-disk/indexed file. Returns whether a request was submitted.
    pub(crate) fn canvas_card_spawn(&mut self, definition: bool) -> bool {
        let Some(parent) = self.app_state.canvas_edit_session_block() else {
            return false;
        };
        let Some(path) = self.app_state.canvas_edit_session_path() else {
            return false;
        };
        let Some((line, character)) = self.app_state.canvas_edit_session_cursor() else {
            return false;
        };
        let uri = crate::lsp::client::path_to_lsp_uri(&path);
        let payload = if definition {
            WorkerRequestPayload::LspDefinitionRequest {
                uri,
                line: line as u32,
                character: character as u32,
                jump: false,
            }
        } else {
            WorkerRequestPayload::LspReferencesRequest {
                uri,
                line: line as u32,
                character: character as u32,
            }
        };
        let request = self.submit(RequestSpec {
            revision_id: 0,
            topic: RequestTopic::LspRequest,
            payload,
        });
        if definition {
            self.canvas_def_request_id = request.map(|r| r.request_id);
            self.canvas_def_parent = Some(parent);
            self.canvas_def_request_id.is_some()
        } else {
            self.canvas_refs_request_id = request.map(|r| r.request_id);
            self.canvas_refs_parent = Some(parent);
            self.canvas_refs_request_id.is_some()
        }
    }

    // ── In-card LSP document lifecycle (Phase 1) ─────────────────────────────
    // Register the card file as an open LSP document while it is being edited, so
    // `gd`/`gr` (and later completion/hover) resolve against it. Only files we
    // *own* (`canvas_card_lsp_target`: not the active buffer, not already open)
    // get this lifecycle, so the main editor's documents are never disturbed.

    /// `Enter` into a card → `didOpen` the card file with the session text. If a
    /// *different* card doc is still owned (e.g. spawning a child card chain),
    /// close it first — one owned card doc at a time, matching one-session-at-a-time.
    pub(crate) fn submit_canvas_card_did_open(&mut self) {
        if self.active_lsp_server.is_none() {
            return;
        }
        let Some(path) = self.app_state.canvas_card_lsp_target() else {
            return;
        };
        let Some(text) = self.app_state.canvas_edit_session_text() else {
            return;
        };
        // A different card doc is still owned (e.g. a gd/gr child-card chain) —
        // close it first; we own exactly one card doc at a time. `path` and the
        // owned path are both the block's origin path, so a direct compare is exact.
        if self
            .canvas_card_lsp_open
            .as_ref()
            .is_some_and(|owned| owned.as_path() != path.as_path())
        {
            self.submit_canvas_card_did_close();
        }
        self.canvas_card_lsp_version = 1;
        self.submit(RequestSpec {
            revision_id: 0,
            topic: RequestTopic::LspClient,
            payload: WorkerRequestPayload::LspDidOpen {
                uri: crate::lsp::client::path_to_lsp_uri(&path),
                language_id: super::helpers::language_id_for_path(&path),
                version: self.canvas_card_lsp_version,
                text,
            },
        });
        self.canvas_card_lsp_open = Some(path);
    }

    /// After a card edit → `didChange` so the server's view tracks the in-card
    /// edits (positions stay correct for `gd`/`gr`). No-op unless we own the doc.
    pub(crate) fn submit_canvas_card_did_change(&mut self) {
        let Some(path) = self.canvas_card_lsp_open.clone() else {
            return;
        };
        let Some(text) = self.app_state.canvas_edit_session_text() else {
            return;
        };
        self.canvas_card_lsp_version = self.canvas_card_lsp_version.saturating_add(1);
        self.submit(RequestSpec {
            revision_id: 0,
            topic: RequestTopic::LspClient,
            payload: WorkerRequestPayload::LspDidChange {
                uri: crate::lsp::client::path_to_lsp_uri(&path),
                version: self.canvas_card_lsp_version,
                text,
            },
        });
    }

    /// Leaving the card / closing the canvas → `didClose` the owned card doc.
    pub(crate) fn submit_canvas_card_did_close(&mut self) {
        let Some(path) = self.canvas_card_lsp_open.take() else {
            return;
        };
        self.submit(RequestSpec {
            revision_id: 0,
            topic: RequestTopic::LspClient,
            payload: WorkerRequestPayload::LspDidClose {
                uri: crate::lsp::client::path_to_lsp_uri(&path),
            },
        });
    }

    // ── In-card completion (Phase 3) ─────────────────────────────────────────

    /// Request `textDocument/completion` for the **card** file at the **session
    /// cursor**; the result fills the card completion menu (not the main editor's).
    pub(crate) fn submit_canvas_card_completion(&mut self) -> bool {
        if self.active_lsp_server.is_none() {
            return false;
        }
        let Some(path) = self.app_state.canvas_edit_session_path() else {
            return false;
        };
        let Some((line, col, prefix_start_col, prefix)) =
            self.app_state.canvas_edit_session_completion_context()
        else {
            return false;
        };
        let request = self.submit(RequestSpec {
            revision_id: 0,
            topic: RequestTopic::LspRequest,
            payload: WorkerRequestPayload::LspCompletionRequest {
                language_id: super::helpers::language_id_for_path(&path),
                uri: crate::lsp::client::path_to_lsp_uri(&path),
                line: line as u32,
                character: col as u32,
                cursor_line: line,
                cursor_col: col,
                prefix_start_col,
                prefix,
            },
        });
        self.canvas_completion_request_id = request.map(|r| r.request_id);
        self.canvas_completion_request_id.is_some()
    }

    /// Store the card completion state. The renderer reads `canvas_completion`
    /// directly (passed into `update_canvas_content`) and draws it with the SAME
    /// `draw_completion_menu` the main editor uses — no separate mirror needed.
    pub(crate) fn canvas_set_card_completion(
        &mut self,
        completion: crate::app::app_state::CompletionState,
    ) {
        self.canvas_completion = Some(completion);
    }

    /// Dismiss the in-card completion menu. Returns whether one was showing.
    pub(crate) fn dismiss_canvas_card_completion(&mut self) -> bool {
        self.canvas_completion_request_id = None;
        self.canvas_completion.take().is_some()
    }

    /// Navigate/accept/close the in-card completion menu. `Some(changed)` when the
    /// command was consumed by the menu, `None` to let it fall through (e.g. typing).
    pub(crate) fn handle_canvas_completion_command(&mut self, command: &Command) -> Option<bool> {
        self.canvas_completion.as_ref()?;
        match command {
            Command::CompletionNext => {
                if let Some(c) = self.canvas_completion.as_mut() {
                    let len = c.filtered_items.len().max(1);
                    c.selected_index = (c.selected_index + 1) % len;
                }
                self.request_redraw();
                Some(true)
            }
            Command::CompletionPrev => {
                if let Some(c) = self.canvas_completion.as_mut() {
                    let len = c.filtered_items.len().max(1);
                    c.selected_index = (c.selected_index + len - 1) % len;
                }
                self.request_redraw();
                Some(true)
            }
            Command::CompletionAccept => Some(self.accept_canvas_completion()),
            Command::CompletionClose => {
                self.dismiss_canvas_card_completion();
                self.request_redraw();
                Some(true)
            }
            _ => None,
        }
    }

    /// Apply the selected completion item to the session: delete the typed prefix
    /// and insert the item's text (a simple prefix-replace — snippet/import edge
    /// cases are not expanded in this iteration), routed through the card editing
    /// dispatch so history / `didChange` / re-sync all happen normally.
    fn accept_canvas_completion(&mut self) -> bool {
        let Some(completion) = self.canvas_completion.take() else {
            return false;
        };
        self.canvas_completion_request_id = None;
        self.app_state.canvas_clear_card_overlay();
        let Some(entry) = completion.filtered_items.get(completion.selected_index) else {
            return false;
        };
        let item = &entry.item;
        let insert_text = item
            .text_edit_text
            .clone()
            .or(item.insert_text.clone())
            .unwrap_or(item.label.clone());
        if insert_text.is_empty() {
            return true;
        }
        // Delete the identifier prefix actually before the cursor NOW (more robust
        // than the request-time `typed_prefix` if the user kept typing).
        let prefix_len = self
            .app_state
            .canvas_edit_session_completion_context()
            .map(|(_, _, _, p)| p.chars().count())
            .unwrap_or_else(|| completion.typed_prefix.chars().count());
        for _ in 0..prefix_len {
            self.dispatch_card_editing_command(Command::Backspace, 1);
        }
        self.dispatch_card_editing_command(Command::InsertText(insert_text), 1);
        true
    }

    // ── In-card hover (Phase 2) ──────────────────────────────────────────────

    /// `K` while editing a card: request `textDocument/hover` for the **card's**
    /// file at the **session cursor**; the result fills the card overlay (a small
    /// doc box at the caret) rather than the main editor's FloatingBox.
    pub(crate) fn canvas_card_hover(&mut self) -> bool {
        if self.active_lsp_server.is_none() {
            return false;
        }
        let Some(path) = self.app_state.canvas_edit_session_path() else {
            return false;
        };
        let Some((line, character)) = self.app_state.canvas_edit_session_cursor() else {
            return false;
        };
        let request = self.submit(RequestSpec {
            revision_id: 0,
            topic: RequestTopic::LspRequest,
            payload: WorkerRequestPayload::LspHoverRequest {
                language_id: super::helpers::language_id_for_path(&path),
                uri: crate::lsp::client::path_to_lsp_uri(&path),
                line: line as u32,
                character: character as u32,
                for_completion: false,
                completion_revision: None,
            },
        });
        self.canvas_hover_request_id = request.map(|r| r.request_id);
        self.canvas_hover_request_id.is_some()
    }
}
