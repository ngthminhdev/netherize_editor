# Spatial Canvas — Phase A Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: superpowers:executing-plans. Steps use `- [ ]`.

**Goal:** A navigable, read-only spatial canvas: enter a dedicated full-screen Canvas mode on the cursor symbol, render code blocks (focal + LSP definition/callers/callees) on a pan/zoom plane, navigate with `hjkl`/`Tab`, spawn relations with `Enter`, exit with `Esc`.

**Architecture:** Pure core in `src/canvas/` (done). A `CanvasState` lives on `AppState`. A new `EditorMode::Canvas` gates input; `Canvas*` commands mutate `CanvasState`. A self-contained `canvas_*` render layer (mirrors the palette) draws blocks via camera world→screen. LSP definition/references source relation blocks (callHierarchy is a later add).

**Tech stack:** Rust, wgpu, cosmic-text, ropey, winit; existing LSP async runtime + tree-sitter highlight.

## Global Constraints
- 90+ FPS bar; per-frame canvas work viewport-culled (only on-screen blocks build glyphs).
- Phase A is **read-only** — never mutate the live document or open buffers.
- No `git commit` (human only).
- Spec: `docs/superpowers/specs/2026-06-19-spatial-canvas-phaseA-design.md`.

---

## Task 1 — Pure canvas core ✅ DONE
`src/canvas/{mod,model,layout,navigation}.rs`. Camera world↔screen + zoom-about + pan; `CanvasState` (push/focus/focus_direction/focus_cycle); `place_relations`; `nearest_in_direction`. 12 unit tests green.

## Task 2 — AppState canvas state + logic
**Files:** Modify `src/app/app_state/mod.rs` (struct field + init), create `src/app/app_state/canvas.rs` (logic), declare it in the app_state module list.
**Interfaces produced:** `AppState.canvas: Option<CanvasState>`; `app_state.open_canvas(block_w,block_h)`, `close_canvas()`, `is_canvas_active()`, `canvas_focus_dir(Dir)`, `canvas_cycle(bool)`, `canvas_pan(dx,dy)`, `canvas_zoom(factor, anchor)`, `canvas_focal_origin()` (for sourcing), `canvas_add_relations(Vec<(BlockRelation, BlockOrigin, BlockSnapshot)>)`.
- [ ] Add `pub(crate) canvas: Option<CanvasState>` field (defaults `None`); ensure constructor/Default sets it.
- [ ] `open_canvas`: build focal `CanvasBlock` from the cursor's current file + a region around the cursor (symbol name via existing symbol-at-cursor helper or word-under-cursor; snapshot text = lines around cursor; origin byte range + lsp line/col from `cursor_line_col`). Place focal at world origin. `Some(CanvasState)`.
- [ ] `close_canvas` → `canvas = None`.
- [ ] nav/pan/zoom delegate to `CanvasState`.
- [ ] `canvas_add_relations`: alloc ids, place via `place_relations` using focal rect + counts, push blocks.
- [ ] Unit tests: open builds focal+focus; close clears; focus_dir after adding relations; add_relations places left/right; **open/close never touch `self.text`/dirty**.

## Task 3 — EditorMode::Canvas + commands + dispatch
**Files:** `src/core/mode.rs` (enum + `as_str` + `ModeEvent::EnterCanvas` + TRANSITION_RULES, bump array len), `src/core/commands.rs` (`CanvasOpen`, `CanvasClose`, `CanvasFocusLeft/Right/Up/Down`, `CanvasCycleNext/Prev`, `CanvasSpawnRelations`, `CanvasZoomIn/Out`, `CanvasPanLeft/Right/Up/Down`, `CanvasFitAll`), `src/core/command_dispatch/mod.rs` (match arms → new `canvas::dispatch`), create `src/core/command_dispatch/canvas.rs`.
- [ ] `EditorMode::Canvas` + `as_str` "canvas".
- [ ] `ModeEvent::EnterCanvas`; rules `Normal --EnterCanvas--> Canvas`, `Canvas --Escape--> Normal`, `Canvas --OpenPalette--> PaletteFocus` (return to Canvas). Update `TRANSITION_RULES` length.
- [ ] Commands enum variants (+ `supports_numeric_count` where sensible, e.g. pan/zoom).
- [ ] `canvas::dispatch(ctx, command)` mutates `app_state.canvas`, returns `DispatchReport{redraw}`. `CanvasOpen` also fires `SwitchMode(EnterCanvas)`; `CanvasClose`/Escape fires `SwitchMode(Escape)`.
- [ ] Wire into the big dispatch match.
- [ ] Tests: dispatch CanvasOpen → mode Canvas + canvas Some; Escape → cleared + Normal.

## Task 4 — Input routing for Canvas mode
**Files:** `src/app/input/handler.rs` (`route_canvas_input`), possibly `src/app/event_loop/commands.rs` (`handle_canvas_command`).
- [ ] In `route_normalized_input`, before the PaletteFocus block: `if context.mode == EditorMode::Canvas { return self.route_canvas_input(...) }`.
- [ ] Map: `h/j/k/l`→FocusLeft/Down/Up/Right; `Tab`/`Shift-Tab`→CycleNext/Prev; `Enter`→SpawnRelations; `+`/`-`→ZoomIn/Out; `H/J/K/L` or arrows→Pan; `gg`→FitAll; `Esc`→CanvasClose. Unmapped keys swallowed (no-op) so the editor underneath is inert.
- [ ] A keybinding to ENTER: add `CanvasOpen` (e.g. `gc` in Normal) via the input map / Normal handler.
- [ ] Tests for the key→command mapping where the handler is unit-testable.

## Task 5 — Render layer
**Files:** `src/render/renderer.rs` (canvas_* fields), `src/render/renderer/lifecycle.rs` (init + theme/font propagation), create `src/render/renderer/canvas.rs` (`update_canvas_content`, `clear_canvas`), `src/render/renderer/lifecycle/frame.rs` (range calc + merge + draw; skip editor when canvas active), `src/app/app_state/...` (`canvas_render_model()` builder) + `src/app/event_loop/application.rs` redraw hook.
- [ ] Add `canvas_text_system/pipeline`, `canvas_glyph_instances`, `canvas_chrome_instances`, `canvas_icon_pipeline`, `canvas_icon_instances`, `canvas_scissor`; init in lifecycle (mirror palette lines).
- [ ] `update_canvas_content(model, camera)`: for each block, `camera.world_to_screen(block.world)`; viewport-cull off-screen; push panel bg + border (focused=accent) + title (icon+name+relation tag) + body glyphs (snapshot text, syntax spans via `layout_panel_rich_text`, font scaled by zoom). Upload text+icon instances.
- [ ] `clear_canvas` (mirror `clear_palette`).
- [ ] frame.rs: compute `canvas_start/count`, merge chrome into region buffer, draw region range + icon + text with `canvas_scissor`; when canvas active, **skip editor layer**.
- [ ] redraw(): when `app_state.is_canvas_active()`, build model + `renderer.update_canvas_content`; else `renderer.clear_canvas()`.
- [ ] Reuse `motion::OverlayMotion` reveal for mode-enter + per-block spawn (optional polish); drive via existing `tick_*`/`WaitUntil`.
- [ ] Manual GPU FPS check (no automated GPU test).

## Task 6 — LSP sourcing (definition + references; callHierarchy deferred)
**Files:** `src/app/event_loop/commands_lsp.rs` (submit on `CanvasSpawnRelations`), `src/app/event_loop/async_results/` (new `canvas.rs` handler), reuse `parse_locations`, `lsp_uri_to_path`, `lsp_position_to_byte_idx`, `read_file_preview`/syntax span helper.
- [ ] On `CanvasSpawnRelations`: submit `textDocument/definition` + `textDocument/references` for the focal block's `lsp_line/character` + path; tag results with a canvas request id + focal block id.
- [ ] Result handler: map each location → `BlockOrigin` + `BlockSnapshot` (read preview around the location, highlight), classify definition vs caller (references = callers), call `app_state.canvas_add_relations(...)`, request redraw.
- [ ] Stale-guard by request id (mirror definition handler).
- [ ] **Follow-on (still Phase A):** implement `callHierarchy/{prepare,incomingCalls,outgoingCalls}` (capabilities + message types + submit + worker dispatch + parse + handler) for precise callers/callees; references stays as fallback. Tracked separately — large.
- [ ] Tests: fake LSP result → `canvas_add_relations` produces correctly-classified, placed blocks.

## Task 7 — Polish & verification
- [ ] `cargo build` + `cargo clippy` clean; `cargo test --lib` green.
- [ ] Manual GUI: `gc` opens canvas on a symbol; `Enter` spawns def/refs; `hjkl`/`Tab` navigate; `+`/`-` zoom; `Esc` returns to editor unchanged. Confirm 90+ FPS.
- [ ] Update `.wolf/` + memory; note Phase B (edit + write-back) is the next spec.

## Status
Task 1 ✅ complete (green). Tasks 2–7 pending. Tasks 5 (render) and 6 (LSP) require GUI/LSP-server verification and cannot be fully validated headless.
