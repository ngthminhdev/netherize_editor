# NetherCanvas v2 — Overlay Redesign (target-driven) — Design

**Date:** 2026-06-19
**Status:** Approved (auto-approved; user out)
**Supersedes the render/interaction model of:** `2026-06-19-spatial-canvas-phaseA-design.md`

## Why a redesign

The first render attempts ("full-screen takeover", then a hand-rolled card
renderer) fought the renderer's runtime-scale system → glyphs dwarfed the cards
("tùm lum"). The target mockup is an **overlay on the live editor** with cards
that look like **mini editors** (tab header + gutter + syntax highlight).

**Root-cause fix:** render cards with the **same technique as the working
PeekWindow/FloatingBox** code panel — i.e. the editor's own geometry
(`theme.editor.font_size`/`line_height`), per-line layout with byte-range syntax
spans, per-line vertical clip, a line-number gutter. Cards are **sized from the
editor font** so code fits at zoom 1; zoom scales the whole plane uniformly.

## What we keep (already green)

- `src/canvas/` pure core: `Camera` (world↔screen, zoom-about, pan),
  `CanvasState` (blocks/focus/nav), `layout`, `navigation`.
- `AppState.canvas` + open/close/nav/pan/zoom/`canvas_add_relations`.
- LSP sourcing: `gd`→definition, `gr`→references, redirected into the canvas via
  `canvas_def_request_id`/`canvas_refs_request_id` → `attach_canvas_relations`.
- Commands + dispatch + `InputFocusContext::Canvas` + `resolve_canvas_focus`.

## What changes

### 1. Card = mini-editor (reuse the peek technique)
Each relation card renders via a focused per-line code panel (modelled on the
FloatingBox loop):
- **Tab header**: `path` · `symbol` · `line-range` (+ `● Live` for def later),
  with a relation-coloured accent + active underline on the focused card.
- **Gutter**: 1-based line numbers (`start_line`).
- **Body**: per-line layout of the snippet with **syntax spans** (highlighted
  app-side at sourcing time), clipped per line to the card height, each line
  truncated to the card width.
- Card world size is computed from the editor font (≈46 cols × ~14 lines) so the
  code fits at zoom 1.

### 2. Focal = the editor (drop the focal card)
No focal card is drawn. The focal block stays in the model only as the **layout
anchor** (relations stack to its right) and the **connector origin**. The
editor's cursor line is the visual anchor.

### 3. Snapshot model carries highlight
`BlockSnapshot { title, symbol, start_line, text, spans }` where `spans:
Vec<CanvasSpan>` (`{start,end,color,bold,italic}`, byte offsets into `text`).
`CanvasSpan` keeps the canvas core decoupled from `text_system`. Spans are
computed in `attach_canvas_relations` via the existing `highlight_snippet` +
`syntax_spans_to_styled` (which have the theme), converted to `CanvasSpan`.

### 4. Connectors (Stage 2)
Curved (cubic/elbow, accent) from the editor's focal line (right edge, screen Y
from editor geometry) to each card's left-mid.

### 5. Chrome
Top strip `‹ Spatial Canvas`; bottom hint bar
(`gc · gd def · gr refs · P pin · Tab next · ↑↓←→ navigate · hjkl pan · Esc`).

### 6. Keymap (synced with the main editor)
`gc` open · **`gd`** spawn definition · **`gr`** spawn references(callers) ·
`P` pin (stub) · `Tab` cycle · arrows navigate · `hjkl` pan · `+/-` zoom · `Esc`
close. `gd`/`gr` are `g`-prefixed sequences handled in `resolve_canvas_focus`
(a `Cell<bool>` pending-`g` flag). (True callee via callHierarchy is later.)

## Staging (each build-green, GUI-verified by the user)
1. **Card mini-editor**: `CanvasSpan` + rich snapshot + per-line render
   (tab header + gutter + syntax) + drop focal card + `gd`/`gr` keymap.
2. **Connectors** from the editor focal line → cards.
3. **Polish**: pin / ✕ / `● Live`, `⌘K`, real callee (callHierarchy).

## Components / seams
- `src/canvas/model.rs` — `CanvasSpan`; `BlockSnapshot` fields.
- `src/app/app_state/canvas.rs` — focal snapshot (minimal; not rendered);
  block-size already font-derived.
- `src/app/event_loop/async_results/lsp.rs` — `attach_canvas_relations` builds
  rich snapshots (raw lines + highlight + start_line + symbol/path).
- `src/app/event_loop/async_results/preview.rs` — raw line reader (no gutter
  baked) returning `(text, start_line)`.
- `src/render/renderer/canvas.rs` — per-line code-panel render; skip focal.
- `src/app/input_map/focus.rs` + `mod.rs` — `g`-prefix `gd`/`gr`.
- `src/core/commands.rs` + dispatch + `commands_canvas.rs` — keep; map `gd`/`gr`
  to the existing def/refs submits.

## Testing
Keep pure unit tests (camera/layout/nav/state/add_relations). Per-line span
slicing helper gets a unit test. Render verified in the GUI per stage.
