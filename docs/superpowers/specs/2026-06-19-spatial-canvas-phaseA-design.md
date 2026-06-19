# Spatial Canvas (NetherCanvas) — Phase A: Navigable Read-Only Canvas — Design

**Date:** 2026-06-19
**Status:** Approved (auto-approved by user)
**Branch:** `feature/spatial-canvas`

## 0. Context & scope

NetherCanvas turns the editor into a 2D plane where, standing on a function
call, you bring its **definition / callers / callees** onto the plane as
**Code Blocks**, arrange/zoom/pan, and (later) edit them with write-back to the
source file.

This is delivered as **two sequential sub-projects**:

- **Phase A (this spec):** a navigable **read-only** spatial canvas. Enter a
  dedicated full-screen Canvas mode, spawn def/caller/callee as read-only code
  blocks, navigate between them (`hjkl`/`Tab`), spawn related blocks (`Enter`),
  pan/zoom. No editing. This is independently shippable and testable.
- **Phase B (later spec):** edit + write-back — the focused block becomes the
  single live document (reusing the existing buffer-switch machinery), full Vim
  applies, `save` writes back.

Locked decisions (from brainstorming):

1. **Surface:** a dedicated full-screen **Canvas mode** (not an HUD evolution,
   not an editor overlay).
2. **Edit model (Phase B):** the *focused* block is the one live document,
   loaded into the existing `AppState.text` slot exactly like a buffer switch;
   other blocks are read-only snapshots. Phase A keeps **all** blocks read-only,
   so the live-document slot is untouched and the existing editor state is
   preserved while in Canvas mode.
3. **Relationship source:** LSP `textDocument/definition` +
   `callHierarchy/incomingCalls` (callers) / `outgoingCalls` (callees); fall
   back to `textDocument/references` when call hierarchy is unavailable.
   `src/codegraph/` is an offline augment, not required for v1.

## 1. Goal & non-goals

**Goal (Phase A):** From an open file with the cursor on a symbol, press a key
to enter Canvas mode with that symbol as the focal block. Pull its definition,
callers, and callees onto a pannable/zoomable plane as read-only, syntax-
highlighted code blocks. Navigate focus with `hjkl`/`Tab`, spawn a focused
symbol's relations with `Enter`, pan/zoom, and `Esc` back to the editor — all
keyboard-first, at the existing 90+ FPS bar.

**Non-goals (Phase A):** editing blocks / write-back (Phase B); persisting the
canvas across sessions; minimap; multiple simultaneous live documents; mouse
drag-arrange (keyboard-first first); animated auto-layout beyond simple
placement.

## 2. Existing infrastructure reused (verified in code)

- **Modal editor:** `EditorMode` + command dispatch (`src/core/commands.rs`,
  `src/core/command_dispatch/`) and event-loop key routing. Canvas adds a new
  top-level mode with its own key map; it does not invent a new event loop.
- **Multi-buffer model:** `AppState` holds one live document (`text: Rope`,
  cursor, `active_file`) plus stashed `buffers: Vec<BufferEntry>`
  (`BufferContent::Text { path, .. }`). Phase A reads snapshots from open
  buffers or disk; Phase B reuses `activate_buffer_index`-style switching.
- **Self-contained render layer pattern:** the Command Palette renders via its
  own `palette_chrome_instances` / `palette_glyph_instances` /
  `palette_icon_instances` + dedicated pipelines + scissor, built in
  `update_palette_content` and drawn in the frame loop. Canvas mirrors this with
  `canvas_*` instance vectors + a single pan/zoom transform applied before
  upload.
- **Syntax highlight:** tree-sitter highlight already used for editor text +
  `set_text_with_spans`; reused to color block snapshots.
- **LSP plumbing:** `submit_lsp_*` requests (`commands_lsp.rs`) → async
  scheduler → `async_results/` handlers. Canvas adds definition / call-hierarchy
  submissions and result handlers that populate canvas blocks.
- **Motion:** `src/workbench/motion.rs` (`OverlayMotion` reveal, easing,
  `tick_*` cadence) reused for the mode enter transition and for block-spawn
  reveal.

## 3. Core model

A new pure-ish module **`src/canvas/`** (UI-free model + logic, unit-tested):

```rust
// src/canvas/model.rs
pub struct CanvasBlock {
    pub id: BlockId,
    pub origin: BlockOrigin,         // where it came from
    pub world_rect: WorldRect,       // position+size on the infinite plane (world coords)
    pub snapshot: BlockSnapshot,     // read-only text + highlight spans + title
    pub relation: BlockRelation,     // Focal | Definition | Caller | Callee
}

pub struct BlockOrigin {            // enough to (Phase B) open as a live document
    pub path: PathBuf,
    pub range: ByteRange,            // byte range in the file the snapshot covers
    pub symbol_name: String,
}

pub struct CanvasState {
    pub blocks: Vec<CanvasBlock>,
    pub focused: Option<BlockId>,
    pub camera: Camera,              // pan (world offset) + zoom (scale)
    pub pending: Vec<PendingSource>, // in-flight LSP requests awaiting results
}

pub enum BlockRelation { Focal, Definition, Caller, Callee }
```

- **World vs screen:** blocks live in **world coordinates**; `Camera { offset:
  Vec2, zoom: f32 }` maps world→screen each frame. Pan = move `offset`, zoom =
  scale about the focused block (or screen center).
- **Spawn placement (deterministic, simple):** definition placed to the
  **right** of the focal block, callers stacked to the **left**, callees stacked
  to the **right**, vertically distributed; collision-avoided by a simple
  column/row packing in `src/canvas/layout.rs` (pure, unit-tested). No physics.

## 4. Data flow (Golden Data Flow)

1. **Enter:** command `CanvasOpen` (e.g. `gc` / `<leader>c`) reads the cursor's
   symbol from `AppState`, creates a `CanvasState` with one **Focal** block =
   the current file region around the symbol, sets `app_state.canvas =
   Some(state)`, and switches the top-level mode to `Canvas`. The editor's
   document state is left untouched (Phase A is read-only).
2. **Render:** when `canvas` is active, the frame loop calls
   `renderer.update_canvas_content(&canvas_render_model, camera)` which builds
   `canvas_chrome/glyph/icon` instances in **screen space** (world→screen via
   camera) and draws them on a dedicated layer; the editor layer is skipped.
3. **Navigate (Nav layer):** keys map to canvas commands that mutate
   `CanvasState` (focus move, pan, zoom) — pure state changes → `request_redraw`.
4. **Spawn (`Enter`):** on the focused block, submit LSP definition + call
   hierarchy (incoming/outgoing) for the focal symbol; push `PendingSource`
   entries. Async results land in `async_results/canvas.rs`, are converted to
   `CanvasBlock`s (snapshot text read from the open buffer or disk, highlighted),
   placed via `layout.rs`, and appended; a spawn reveal motion animates them in.
5. **Exit:** `Esc` clears `app_state.canvas`, restores the previous mode and the
   editor view. No document mutation occurred.

## 5. Navigation (keyboard-first, confirmed)

Phase A is a single **Nav** layer (Edit layer arrives in Phase B):

| Key | Action |
|---|---|
| `h`/`j`/`k`/`l` | Move focus to the nearest block left/down/up/right (spatial) |
| `Tab` / `Shift-Tab` | Cycle focus through blocks in spawn order |
| `Enter` | Spawn def/callers/callees of the focused block's symbol |
| `+` / `-` (or `<leader>z`/`<leader>Z`) | Zoom in/out about the focused block |
| `H`/`J`/`K`/`L` (or arrows) | Pan the camera |
| `gg` | Re-center / fit-all |
| `Esc` | Exit Canvas mode back to the editor |

Spatial focus move picks the block whose center is nearest in the requested
direction within an angular cone (pure function in `src/canvas/navigation.rs`,
unit-tested).

## 6. Rendering & motion

- New self-contained layer mirroring the palette: `canvas_chrome_instances:
  Vec<RegionDrawInstance>`, `canvas_glyph_instances: Vec<GlyphInstance>`
  (`canvas_text_pipeline`), `canvas_icon_instances` (`canvas_icon_pipeline`),
  `canvas_scissor`. Built in `update_canvas_content`, drawn in the frame loop
  when `canvas` is active (editor layer skipped).
- **Camera transform** applied in `update_canvas_content`: each block's
  `world_rect` → screen rect via `offset`/`zoom`; glyph metrics scale with
  `zoom` (snapshot re-laid at the zoomed font size, viewport-culled so only
  on-screen blocks build glyphs → bounded per-frame cost).
- Block chrome: rounded panel bg + 1px border (focused = accent border, others =
  dim), title bar (icon + symbol name + relation tag), code body.
- **Motion:** reuse `OverlayMotion` reveal for (a) mode enter (whole plane fades
  in) and (b) each spawned block (Dot→Line→Panel reveal at the block's rect).
  Driven by the existing `tick_*` + `WaitUntil` cadence.

## 7. Components / seams touched

- `src/canvas/` — **new** pure module: `model.rs`, `layout.rs`, `navigation.rs`,
  `mod.rs`. Fully unit-tested (placement, focus-move, camera math, snapshot
  range extraction).
- `src/app/app_state/` — add `canvas: Option<CanvasState>` + accessors; a
  `canvas_render_model()` builder (mirrors `command_palette_render_model`);
  helper to read a snapshot range from an open buffer or disk.
- `src/core/commands.rs` + `command_dispatch/` — new `Canvas*` commands and a
  `Canvas` mode branch in dispatch.
- `src/app/event_loop/` — route keys to canvas commands while in Canvas mode;
  `tick`/`WaitUntil` for canvas motions; build the canvas render model in
  `redraw()`.
- `src/app/event_loop/commands_lsp.rs` + `async_runtime/scheduler/lsp_parse.rs`
  + `async_results/` — definition + call-hierarchy submit/parse/handle feeding
  canvas blocks (verify callHierarchy is wired; add if missing, with references
  fallback).
- `src/render/renderer/` — `canvas_*` instance vectors + pipelines (lifecycle),
  `update_canvas_content`, `clear_canvas`, draw hook in the frame loop.
- `src/render/renderer/canvas.rs` — **new** render module for the layer.

## 8. Testing

- **Unit (pure, `src/canvas/`):** block placement (caller-left/callee-right,
  no overlap); spatial focus-move direction/cone; camera world↔screen round-trip
  and zoom-about-point; snapshot range extraction from a rope.
- **State:** `CanvasOpen` builds a focal block from the cursor symbol; `Esc`
  clears canvas without mutating the document; spawn appends mapped blocks from a
  fake LSP result.
- **Perf:** frame `< 8 ms` with N blocks on screen (viewport-culled); validate
  with the existing bench harness; manual GPU FPS check in the GUI.
- Follow the existing `command_palette` / `overlay_manager` test style.

## 9. Phase boundary

Phase A ends with a working, navigable, read-only canvas. Phase B (separate
spec) adds: focus → live document (buffer-switch reuse), Vim editing inside the
focused block, and `save` write-back, plus same-file multi-block refresh.
