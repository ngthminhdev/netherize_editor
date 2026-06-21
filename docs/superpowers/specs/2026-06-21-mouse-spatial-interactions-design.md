# Mouse Spatial Interactions — Design

**Date:** 2026-06-21
**Status:** Approved design, ready for implementation plan
**Author:** Brainstorm (Min + Claude)

## 1. Goal & Philosophy

Netherize is a keyboard-first (vim) editor. This work adds **selective mouse
augmentation**: the pointer handles only **spatial / geometric** operations that
the keyboard does clumsily. The keyboard stays primary. **No text-editing by
mouse** (no click-to-place-cursor, no drag-select) in this scope.

In-scope interactions (confirmed):

1. **Resize panel/dock** — drag a dock's inner edge to resize it.
2. **Drag-reposition canvas card** — press-and-drag a card across the canvas plane.
3. **Resize canvas card** — drag a card's right edge / bottom edge / bottom-right
   corner to change its width **and** height.

Full hover affordance (confirmed): change the OS cursor shape on hover over a
draggable zone **and** draw a light highlight on the hovered handle/border.

## 2. Current State (what exists today)

- Mouse events are already received in `src/app/event_loop/application.rs`:
  `CursorMoved` (stores `last_cursor_position`), `MouseWheel`, `MouseInput`.
- Existing mouse handling is **click-only**, following one pattern:
  `last_cursor_position` + a `current_*_bounds()` region helper + a renderer
  hit-test helper (e.g. `handle_left_dock_tab_mouse_click`,
  `handle_outline_mouse_click`, `handle_bottom_tab_mouse_click`).
- **There is no drag lifecycle** anywhere: `CursorMoved` only stores the
  position; nothing tracks press → move → release with pointer capture. This is
  the core new primitive this design introduces.
- Panel sizes: `WorkbenchPanelState { left, right, bottom }`, each a
  `PanelState { visible, size_px, .. }` (`src/workbench/panel_state.rs`). Seeded
  from UI theme tokens. **Not persisted across restart** today (only
  `recent_projects` + `theme_profile` are persisted in `state.toml`).
- Canvas model (`src/canvas/model.rs`):
  - `CanvasBlock { world: WorldRect{x,y,w,h}, height_rows: Option<usize>, pinned, .. }`.
  - **Per-card width AND height already live in `block.world.w` / `block.world.h`** —
    the renderer draws every card via `cam.world_to_screen(block.world)`, so the
    `WorldRect` is the rendering source of truth (no `block_w` read in the
    renderer). `CanvasState.block_w` is only the *spawn default* width.
  - Width/height resize already exists for the **keyboard**:
    `AppState::canvas_change_focused_width(delta: f32)` (clamps to
    `[block_w*0.5, block_w*2.5]`, mutates `world.w`) and
    `AppState::canvas_change_focused_height(delta: i32)` (clamps rows to
    `[CARD_MIN_LINES, CANVAS_CARD_HARD_MAX]`, sets `height_rows` + recomputes
    `world.h = card_height_exact(rows)`). **No new model field is needed** — mouse
    resize reuses these same clamps via new absolute-value methods.
  - `Camera` provides `world_to_screen_point`, `screen_to_world_point`,
    `world_to_screen`, `pan`, `zoom_about`.
  - `CanvasState::move_focused(dx, dy)` already exists (keyboard move); pinned
    blocks are immovable by it; it sets `user_arranged = true`.
  - `CanvasInteraction` sub-state: `Navigate` / `EditCard` / `Background`.
  - The canvas renders as a **full-window overlay** (renderer uses
    `surface_state.config.width/height`), so window cursor coords map directly to
    canvas screen coords — no panel offset to subtract.

## 3. Architecture (Approach A — centralized pointer-drag)

A single drag state-machine. Geometry/hit-test is **pure and unit-testable**
(no GPU/winit); `application.rs` only wires winit events into it.

### New modules

- **`src/canvas/interaction.rs`** (pure)
  - `CardZone` enum: `Body`, `ResizeRight`, `ResizeBottom`, `ResizeCorner`.
  - `pub fn card_pointer_hit_test(blocks: &[CanvasBlock], camera: &Camera, effective_w: impl Fn(&CanvasBlock)->f32, cursor: (f32,f32)) -> Option<(BlockId, CardZone)>`
    — projects each card's world rect (using its effective width) to screen,
    tests handle bands first, then body; returns the topmost hit.
  - Resize math helpers: `world_height_to_rows(line_h, world_h) -> usize`,
    `pixel_delta_to_world(zoom, dpx) -> f32`. Clamp helpers.
  - Unit tests cover every zone, misses, and multiple zoom levels.

- **`src/workbench/pointer_drag.rs`** (pure)
  - `PanelSide` enum: `Left`, `Right`, `Bottom`.
  - `DragTarget` enum: `PanelEdge(PanelSide)`, `CardMove(BlockId)`,
    `CardResize(BlockId, CardZone)`.
  - `HoverTarget` (same variants) for cursor-shape + highlight.
  - `ActiveDrag { target: DragTarget, start_cursor: (f32,f32), start_value: DragAnchor }`
    where `DragAnchor` snapshots what's being dragged at press time
    (`PanelSize(f32)`, `CardPos{x,y}`, `CardSize{w,h_rows}`).
  - `pub fn splitter_hit_test(left, right, bottom: Option<[f32;4]>, band_px, cursor) -> Option<PanelSide>`
    — band is `SPLITTER_BAND_PX` (~6px) on each visible dock's inner edge.
  - `clamp_panel_size(side, raw, viewport) -> f32` → `[MIN_PANEL_PX=120, 0.6*viewport_dim]`.
  - Unit tests for each side hit, misses, and clamps.

### Event wiring (`application.rs`)

Add two fields to `AppShell`: `active_drag: Option<ActiveDrag>` and
`hover_target: Option<HoverTarget>`.

- **`MouseInput { Left, Pressed }`**: run the **press hit-test priority chain**
  (below). On hit, set `active_drag` with the start anchor and `return` (do not
  fall through to the existing click handlers). On miss, fall through to the
  current click handlers (tabs/outline/etc.) unchanged.
- **`CursorMoved`**: always update `last_cursor_position`. Then:
  - If `active_drag` is `Some`: compute `delta = cursor - start_cursor`, apply to
    the right model (panel size / card world pos / card size), `request_redraw()`.
  - If `active_drag` is `None`: recompute `hover_target`; update the OS cursor via
    `window.set_cursor(CursorIcon)`; if the hover band/handle changed,
    `request_redraw()` so the highlight tracks it.
- **`MouseInput { Left, Released }`**: clear `active_drag`, recompute hover,
  `request_redraw()`.

### Press hit-test priority

`CardResize handle` → `Card body (move)` → `Panel splitter`.

Canvas hit-tests only run when the canvas is open **and**
`interaction == Navigate` (never in `EditCard` / `Background`). Panel splitter
hit-tests run whenever the relevant dock is visible.

## 4. Resize panel/dock

- **Splitter zones**: a `SPLITTER_BAND_PX` (~6px) band on each visible dock's
  **inner** edge — left dock's right edge, right dock's left edge, bottom dock's
  top edge — derived from `current_left_sidebar_bounds()`,
  `current_right_sidebar_bounds()`, `current_bottom_panel_bounds()`.
- **Apply**: `Left` → `size_px = start + dx`; `Right` → `size_px = start - dx`;
  `Bottom` → `size_px = start - dy`. Then `clamp_panel_size(...)`. Relayout is
  live each move (set `size_px`, `request_redraw`).
- The **bottom dock's top edge is the editor/terminal split** — covered here, no
  separate splitter needed.
- **Cursor**: `ResizeHorizontal` (left/right), `ResizeVertical` (bottom).
- **Persistence**: session-only (matches today's behavior). Cross-restart
  persistence of panel sizes is **deferred** (would require extending
  `state.toml`).

## 5. Drag-reposition canvas card

- **Press on card body** (inside the projected screen rect, not on a resize
  handle), canvas open & `Navigate`: focus that card; record `CardPos` anchor.
- **Dead-zone**: the press only becomes a drag after the cursor moves
  `DRAG_DEADZONE_PX` (~3px), so a click-to-focus doesn't nudge the card.
- **Apply**: `world.x = anchor.x + (cursor.x - start.x)/zoom`, same for `y`. Set
  `user_arranged = true`.
- **New model method**: `AppState::canvas_pointer_move_block(id, world_x, world_y)`
  (sets a block's absolute `world.x/y`; generalizes `move_focused` to an arbitrary
  block at an absolute position). Mouse drag **overrides pin** (direct
  manipulation is explicit intent) — it moves pinned cards too. Sets
  `user_arranged = true`.
- **Cursor**: `Move` while dragging / hovering a card body.

## 6. Resize canvas card (width + height)

- **Handles**: bottom-right corner (~12px screen, `ResizeCorner` = both),
  right-edge band (`ResizeRight` = width), bottom-edge band (`ResizeBottom` =
  height). Band thickness `CARD_RESIZE_BAND_PX` (~8px screen).
- **Height**: convert dragged world-height → row count via `line_h`, set
  `height_rows = Some(n)` and recompute `world.h = card_height_exact(n)` (reuses
  existing `=`/`-` semantics), clamp rows to `[CARD_MIN_LINES, CANVAS_CARD_HARD_MAX]`.
- **Width**: set `block.world.w` directly (the renderer already reads `world.w`
  per card), clamp to `[block_w*0.5, block_w*2.5]` — the **same bounds** as the
  existing `canvas_change_focused_width`.
- **No new model field.** Add one absolute-value method:
  `AppState::canvas_pointer_resize_block(id, new_w: Option<f32>, new_rows: Option<usize>) -> bool`
  that applies the clamps above to an arbitrary block id (mirrors the keyboard
  `canvas_change_focused_width/height` clamp logic). The pure pixel→world and
  pixel→rows conversions live in `canvas/interaction.rs`.
- **No renderer change for sizing** — `world.w`/`world.h` are already the source
  of truth.
- **Cursor**: `ResizeHorizontal` (right), `ResizeVertical` (bottom),
  `ResizeNorthWest`/`ResizeSouthEast` (corner).

## 7. Hover affordance (full)

- On `CursorMoved` without an active drag, run the same hit-tests to compute
  `HoverTarget`; map to a `winit::window::CursorIcon` and call
  `window.set_cursor`. When nothing is hovered → `CursorIcon::Default`.
- **Highlight**: when hovering a splitter, draw a thin accent band along the
  edge; when hovering a card resize handle, draw a small accent corner/edge.
  Reuse `region_pipeline` quads in the renderer. Computed only when a hover
  target is present (cheap; no per-frame cost otherwise).
- Cursor-shape mapping table:
  | HoverTarget | CursorIcon |
  |---|---|
  | `PanelEdge(Left|Right)` | `ResizeHorizontal` |
  | `PanelEdge(Bottom)` | `ResizeVertical` |
  | `CardMove` | `Move` |
  | `CardResize(ResizeRight)` | `ResizeHorizontal` |
  | `CardResize(ResizeBottom)` | `ResizeVertical` |
  | `CardResize(ResizeCorner)` | `ResizeSouthEast` |
  | none | `Default` |

## 8. Constants (single source of truth)

Add to the relevant modules (suggested):
- `SPLITTER_BAND_PX: f32 = 6.0`
- `MIN_PANEL_PX: f32 = 120.0`, `MAX_PANEL_FRACTION: f32 = 0.6`
- `DRAG_DEADZONE_PX: f32 = 3.0`
- `CARD_RESIZE_BAND_PX: f32 = 8.0`, `CARD_RESIZE_CORNER_PX: f32 = 12.0`
- `MIN_CARD_WIDTH_COLS: f32 = 8.0` (converted to world units via the card font)

## 9. Testing strategy (TDD)

All geometry is pure → tested without GPU/winit:

- **`workbench/pointer_drag.rs`**: splitter hit-test per side + miss; band
  edge boundaries; `clamp_panel_size` at both bounds.
- **`canvas/interaction.rs`**: `card_pointer_hit_test` returns the correct zone
  for body / right / bottom / corner / miss, across zoom = {0.5, 1.0, 2.0};
  topmost-card-wins on overlap; `world_height_to_rows` and
  `pixel_delta_to_world` round-trips.
- **`app/app_state/canvas.rs`**: `canvas_pointer_move_block` moves an arbitrary
  (incl. pinned) block to an absolute position and sets `user_arranged`;
  `canvas_pointer_resize_block` sets `world.w` / `height_rows` + `world.h` with the
  same clamps as the keyboard width/height methods.
- The `application.rs` wiring itself is thin; covered by the pure tests above plus
  manual GUI verification (it needs a real window + pointer).

## 10. Out of scope (deferred)

- Zoom + pan the canvas with the mouse (wheel zoom / drag-empty-space pan).
- Editor click-to-place-cursor and drag-select text.
- Click a file in the explorer tree to open it.
- Draggable scrollbars.
- Cross-restart persistence of panel sizes and card positions/sizes.

## 11. Handoff notes

- Follow the existing `handle_*_mouse_*` pattern in `application.rs`; the drag
  arms slot next to them.
- Keep all geometry in the two new pure modules so it's testable; `application.rs`
  should contain no math beyond `delta = cursor - start`.
- No model field changes — card width/height already live in `block.world`. The
  new methods only add absolute-value setters reusing existing clamps.
- Manual GUI checklist: resize each dock; resize editor/terminal split (bottom
  edge); drag a card; drag a pinned card; resize a card by right edge, bottom
  edge, and corner; confirm cursor shape + highlight on every hover; confirm
  click-to-focus a card does NOT move it (dead-zone); confirm canvas drags do
  nothing while editing a card.
