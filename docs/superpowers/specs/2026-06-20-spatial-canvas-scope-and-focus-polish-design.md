# NetherCanvas — Scope-aware Cards & Focus Polish (T3–T6)

**Date:** 2026-06-20
**Branch:** `feature/spatial-canvas`
**Status:** Design approved, ready for implementation plan
**Predecessor:** `2026-06-20-spatial-canvas-incard-edit-v2-design.md` (§11–§15)

## Motivation

After in-card edit v2 (4-state focus, in-card LSP, shared completion menu, card→tab
promote, scope-correct connectors), four polish items remain. They are mostly
independent; only T4/T5 share a foundation (scope detection).

| # | Item | Problem today |
|---|------|---------------|
| T3 | Card title status | Header badge reads the **global** `EditorMode` and renders every label (`PALETTE`, `TERMINAL`, `MC-SELECT`, …). App-global modes leak into the card's "status line". |
| T4 | Context scope | Card snapshot is a fixed **±N line window** (`read_file_lines(path, line, context)`) around the location — not the enclosing function/method/const. |
| T5 | Spawn auto range | Spawned cards (gc/gd/gr) inherit the ±N window; range should track the symbol's scope. |
| T6 | Tab focus cycle | `focus_cycle` iterates **all** blocks including the `Focal` anchor (not drawn as a card) → Tab hits a "no card focused" step (card1 → card2 → *focal* → card1). |

## Decisions (locked during brainstorming)

- **T4 scope engine:** LSP `documentSymbol` (semantic), **not** tree-sitter — with a
  `LocationLink.targetRange` fast-path and a `cached_document_symbols` sync path for the
  focal card.
- **Async UX:** **Progressive** — spawn instantly with the ±N window snapshot, then
  re-source the card to the scope range when it resolves (one reflow).
- **T5 clamp:** scope > ~60 lines → keep first 60 + `"+N more"` marker; full body on
  Enter-to-edit (loads the real file).
- **T3 mode set:** show only `NORMAL / INSERT / VISUAL`; every app-global mode → **hide**
  the badge (and fall the edit-target ring back to cyan focus).
- **T6:** Tab cycles **relation cards only**, skipping `Focal`.

---

## T6 — Tab cycle skips Focal *(do first; smallest, unblocks focus-state tests)*

**File:** `src/canvas/model.rs` — `CanvasState::focus_cycle` (~line 304).

Build the candidate list as the relation cards only, then index into that:

```rust
pub fn focus_cycle(&mut self, forward: bool) -> bool {
    // Relation cards only — the Focal block is the editor anchor, never a card,
    // so Tab must not land on it (that reads as a "focus off" step).
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
    let next = if forward { (cur + 1) % len } else { (cur + len - 1) % len };
    self.focus(cards[next])
}
```

- Currently focused is `Focal` or `None` → `position` returns `None` → `unwrap_or(0)` →
  Tab moves to the first card. Correct defensive behavior.
- Single card → wraps to itself (no-op visible change, returns `true`/`false` per `focus`).
- Shift-Tab (`forward = false`) symmetric.

**Tests** (`src/canvas/model.rs` tests):
- `focus_cycle_skips_focal`: focal + 2 cards → Tab gives card1 → card2 → card1 (never focal).
- reverse direction symmetric.
- focal-only (0 cards) → `false`, focus unchanged.
- single card → stays on that card.
- Adjust existing `focus_cycle_wraps` (~line 652) if it asserted focal participation.

---

## T3 — Card badge: card-edit modes only

**Files:** `src/render/renderer/helpers.rs`, `src/render/renderer/canvas.rs`.

New helper next to `mode_display_label`:

```rust
/// The card header's mode badge shows ONLY the modes a card is actually edited
/// in. App-global modes (palette/terminal/multi-cursor/resize) return `None` so
/// the badge is hidden — the card is not being edited in those.
pub(super) fn card_mode_label(mode: EditorMode) -> Option<&'static str> {
    match mode {
        EditorMode::Normal => Some("NORMAL"),
        EditorMode::Insert => Some("INSERT"),
        EditorMode::Visual | EditorMode::VisualBlock => Some("VISUAL"),
        EditorMode::PaletteFocus
        | EditorMode::TerminalFocus
        | EditorMode::TerminalNormal
        | EditorMode::MultiCursor
        | EditorMode::MultiInsert
        | EditorMode::Resize => None,
    }
}
```

In `canvas.rs`:
1. Badge block (~line 346): `if is_edit_target { if let Some(label) = card_mode_label(mode) { … draw … } }`.
2. **Ring consistency:** the edit-target ring uses `mode_color` (`mode_pill_color(mode)`).
   When `card_mode_label(mode)` is `None`, the card is not really being edited — use the
   cyan focus border instead of the palette/terminal pill color. Compute once near the
   top: `let card_mode = card_mode_label(mode);` and gate both badge and ring color on it.

**Tests** (`helpers.rs` unit): `card_mode_label` returns `Some("NORMAL"/"INSERT"/"VISUAL")`
for Normal/Insert/Visual/VisualBlock and `None` for palette/terminal/MC/resize.

---

## T4 + T5 — Scope-aware snapshot via LSP documentSymbol (progressive)

### Overview

Snapshot range becomes the **enclosing definition symbol's** line range instead of a fixed
window. Spawn stays instant (±N window); a scope-resolution pass refines each card.

### Enclosing-symbol selection (pure logic, the testable core)

`LspDocumentSymbol { name, kind: String, range: LspRange, ancestors }` arrives as a **flat
list** (message.rs:540). Selection:

```
fn enclosing_definition(symbols: &[LspDocumentSymbol], line: u32) -> Option<LspRange>
  candidates = symbols where:
    kind ∈ {"Function", "Method", "Constant", "Constructor"}   // LSP SymbolKind names
    range.start.line <= line <= range.end.line
  pick the SMALLEST span (deepest enclosing); None if no candidate
```

- Put this in a scope module (e.g. `src/app/event_loop/async_results/canvas_scope.rs` or a
  fn in `lsp.rs`). Pure `(symbols, line) -> Option<range>` → unit-testable with hand-built
  symbol lists.
- Confirm the actual `kind` strings the worker emits (it stores `kind: String`); match those
  exact spellings.

### Data flow (progressive)

1. **Spawn (unchanged, instant):** `attach_canvas_relations` / focal build still call the
   ±N `build_canvas_relation_snapshot` → card visible immediately.
2. **Resolve scope per new card:**
   - **Focal card (gc):** active file's symbols are already in `app.cached_document_symbols`
     (`cached_document_symbols_path` matches) → resolve **synchronously**, no request.
   - **Def card (gd):** if the `textDocument/definition` reply was a `LocationLink` with
     `targetRange`, use it directly (no documentSymbol request). *(Check whether the worker
     currently surfaces `targetRange`; if it flattens to a point, fall through to documentSymbol.)*
   - **Caller cards (gr) / file not open / no targetRange:** request `documentSymbol` for the
     **target file**, correlated to the card.
3. **Apply:** on a resolved range, re-source the card via a new range builder, re-highlight,
   regrow the card height; clamp to ~60 lines + `"+N more"`.
4. **Fallback:** documentSymbol unsupported / no enclosing symbol / read error → keep the ±N
   window snapshot. **No regression** vs today.

### New / changed pieces

- **Snapshot builder (range):** `build_canvas_relation_snapshot_range(theme, path, start_line,
  end_line, character, symbol)` in `lsp.rs` — reads an explicit inclusive line range (new
  `read_file_lines_range` in `preview.rs` alongside `read_file_lines`), clamps to 60 +
  `"+N more"`, highlights via `canvas_snapshot_spans`. The ±N builder stays for instant spawn
  + fallback.
- **Async request for arbitrary file:** `LspDocumentSymbolsRequest` (message.rs:298) is
  documented "for the active file". Extend so canvas can request symbols for a **specific
  file** and correlate the reply to a `card_id`. Options:
  - Add an optional `card_id` / `purpose` tag carried request→result, **or**
  - A dedicated `CanvasCardScopeRequest { card_id, uri, line }` →
    `CanvasCardScopeResult { card_id, range }` pair (keeps the canvas path off the breadcrumb
    `document_symbols_request_revision` gating at lsp.rs:528).
  - **Recommended:** the dedicated pair — it avoids entangling canvas with the
    active-file breadcrumb/picker revision logic.
- **Apply path:** handler reuses `canvas_apply_focused_context` (commands_canvas.rs:117) or a
  sibling `canvas_apply_card_scope(card_id, snapshot)`.
- **Range storage:** stash the resolved `[start_byte/end_byte]` or `[start_line,end_line]` on
  `BlockOrigin` (currently `start_byte/end_byte = 0`) so `+`/`-` context re-source and edit
  open use the scope, not the window.

### Symbol kinds

Deepest enclosing of `{Function, Method, Constant, Constructor}`. A reference inside a method
→ the method; a top-level const → the const's own range. A line in a class body but not in any
method → no candidate → fallback window (we intentionally do NOT widen to the class).

### Clamp (T5)

Resolved range > 60 lines → snapshot = first 60 lines + a `"+N more"` marker row (mirror the
existing viewport body cap in `canvas.rs`). Full body is available on Enter-to-edit, which
loads the real file/buffer.

**Tests:**
- `enclosing_definition`: deepest function/method/const selected; nested method beats its class;
  no-candidate → `None`; tie/containment edges.
- `read_file_lines_range` / `build_canvas_relation_snapshot_range`: exact `[start,end]`,
  `start_line` correct, clamp to 60 + `"+N more"`.
- Progressive: card carries the ±N window snapshot pre-resolve; the scope snapshot post-apply
  (assert `start_line`/line-count change).
- Fallback: empty symbols / no enclosing → window snapshot retained.

---

## Implementation order

1. **T6** — `focus_cycle` skip-focal + model tests. (Isolated; unblocks Tab-cycle regression.)
2. **T3** — `card_mode_label` + badge/ring gating + helper tests.
3. **T4 core** — `enclosing_definition` pure fn + `read_file_lines_range` +
   `build_canvas_relation_snapshot_range` + unit tests (no wiring yet).
4. **T4/T5 wiring** — focal sync resolve (cached symbols) → then async
   `CanvasCardScopeRequest/Result` for def/caller cards → progressive apply + clamp.
5. **Regression sweep** — Tab cycle, connector (bug 213/215), file-binding (bug 216) stay green.

Each step builds + clippy-clean (changed files) + full suite green before the next. TDD
(red→green) for every new unit (model focus, `card_mode_label`, `enclosing_definition`,
range builder). Log fixes to `.wolf/buglog.json`.

## Risks / open points

- **documentSymbol for non-active files** is the only structurally new plumbing. Worker must
  resolve the right LSP session by the target file's root (the in-card LSP work already does
  this for didOpen via `sessions_for_document_uri`); reuse that resolution.
- **`targetRange` availability** depends on the server returning `LocationLink` vs `Location`.
  Treat it as a best-effort fast-path; documentSymbol is the reliable route.
- **Kind-string spelling** must match what the worker emits into `LspDocumentSymbol.kind`
  (verify before relying on `"Function"`/`"Method"`/`"Constant"`/`"Constructor"`).
- Index/timing: progressive reflow is one-shot per card; ensure a card opened-as-tab or closed
  before resolve drops the stale `CanvasCardScopeResult` (guard by `card_id` still present).
