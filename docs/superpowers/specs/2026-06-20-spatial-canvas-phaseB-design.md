# NetherCanvas — Spatial Canvas Phase B: Edit-in-Card

> Status: design approved-in-principle by the user (async); to be reviewed after
> implementation. Builds on Phase A
> (`2026-06-19-spatial-canvas-phaseA-design.md`).

## 1. Goal

Phase A gives a **read-only** navigable canvas: from a focal symbol you spawn
def/caller cards (mini-editor snapshots) joined by connectors, and move/pin/close
them. Phase B makes the cards **live**: you can *enter* a card and edit its file
in place — "as if a miniature main editor lived inside the card" — with the edit
written back to disk on save.

The defining behaviour the user asked for:

> "khi đã 'vào' 1 card (đang sửa/focus-in), hjkl = di chuyển con trỏ / scroll
> source NGAY trong card như là 1 main editor thu nhỏ."

So the meaning of `hjkl` (and of every editing key) becomes **state-dependent**:
moving cards when navigating, driving a real cursor inside a card when editing.

## 2. Edit model — buffer-switch (chosen)

The user chose **buffer-switch** (over a card-local buffer) in brainstorming.
Entering a card re-uses the editor's existing multi-buffer machinery:

- `AppState::open_file(path)` activates (or opens) the card's file as the live
  buffer — exactly the `gd`-jump path the editor already uses. The active
  buffer's `Rope` becomes `self.text`; all existing cursor / vim / insert / undo
  / save logic then operates on it **for free**.
- The jump origin is recorded (`push_jump_entry`) so `Ctrl+O` returns to where
  the canvas was opened from after editing — the user is never stranded.
- ⌘S (`save_file`) writes the active buffer back to disk. No new write path.

Trade-off accepted: editing a card changes the global active buffer, so after
closing the canvas you remain on the last-edited file (standard "I jumped here"
editor behaviour; `Ctrl+O` walks back). The focal block's provenance is
unaffected — it is pure data, not the active buffer.

Why not a card-local buffer? It would require re-implementing cursor motion,
vim, insert, undo, and selection against a second `Rope` — a second editor. The
buffer-switch model reuses 100% of the editor and is the smaller, safer change.

## 3. Four-state focus machine

One enum on `CanvasState` drives everything: input routing, rendering, and the
status pill.

```
enum CanvasInteraction {
    Navigate,                                   // S1
    EditCard { block, cursor_line, cursor_col },// S2
    Background,                                 // S3
}
```

`S0` is "no canvas" (`AppState::canvas == None`).

| State | `canvas` | `interaction` | Input focus | hjkl | Status pill | Canvas drawn |
|-------|----------|---------------|-------------|------|-------------|--------------|
| S0 Editor        | None     | —          | Editor | editor cursor | NORMAL/… | no |
| S1 Navigate      | Some     | Navigate   | **Canvas** | move focused card | **CANVAS** | full (scrim + hints + focus ring) |
| S2 Edit-Card     | Some     | EditCard   | **Editor** | **cursor in card** | **CANVAS·EDIT** | edit card renders live + caret |
| S3 Background    | Some     | Background | Editor | editor cursor (focal file) | NORMAL/… | dimmed, **no scrim, no hints** |

### Transitions

```
S0 --gc--------------------> S1   (open_canvas: source def+refs, focus first card)
S1 --Enter (on a card)-----> S2   (begin_edit: open_file + position cursor)
S2 --Esc (Normal mode)-----> S1   (end_edit; refresh the card snapshot from buffer)
S1 --Esc-------------------> S3   (push to background; editor regains focus)
S3 --Esc-------------------> S0   (close canvas)
S3 --gc--------------------> S1   (re-grab the canvas — no re-source)
```

Two-stage Esc (the user's explicit requirement): from cards, the **first** Esc
returns you to the editor with cards still floating (S3); the **second** Esc
closes the canvas. While editing (S2), Esc first leaves *insert* (vim), and only
an Esc in Normal mode leaves the card back to Navigate.

### Why focus = Editor in S2 and S3

Editing must use the **whole** editor input pipeline — vim sequences (`dd`,
`gg`, `ciw`), insert mode, completion, undo. That pipeline only runs when
`InputFocusContext == Editor`. So S2/S3 set focus = Editor and the canvas state
machine intercepts **only `Esc`** (see §4). `hjkl` then naturally means
"editor cursor", which — because the edit card mirrors the live buffer — *is*
"move the cursor inside the card".

## 4. Input routing changes

`build_context` (`event_loop/setup.rs`) currently sets
`focus = Canvas` whenever `is_canvas_active()`. New rule:

```
focus = match canvas.interaction {
    None | Some(Background)            => (normal focus resolution; Editor)
    Some(Navigate) | Some(EditCard)    => Canvas? NO — see below
}
```

Refined: focus = `Canvas` only for `Navigate`; for `EditCard` and `Background`
focus falls through to the **Editor** so editing works fully. The canvas owns
`Esc` via a guarded interceptor in `InputMap::resolve`, placed after the palette
check and before the keymap lookup:

```
if Esc && !cmd-modifier && mode == Normal {
    match context.canvas_interaction {
        Some(EditCard)   => return CanvasExitEdit,   // S2 -> S1
        Some(Background) => return CanvasClose,       // S3 -> S0
        _ => {}                                        // S1 handled in resolve_canvas_focus
    }
}
```

The `mode == Normal` guard is essential: in S2 you may be in Insert mode, where
Esc must drop to Normal first (vim), not jump out of the card.

`KeybindingContext` gains `canvas_interaction: Option<CanvasInteraction>`,
populated in `build_context`.

`resolve_canvas_focus` (the S1 handler) changes only:
- `Enter` → `CanvasEnterEdit` (was `CanvasSpawnRelations`; def+refs stay on
  `gd`/`gr` and on the initial `gc`).
- `Esc` → `CanvasEnterBackground` (was `CanvasClose`).

`open_canvas_mode` (`gc`): if a canvas is already active, just set
`interaction = Navigate` (re-grab from Background) instead of re-opening.

New `Command`s: `CanvasEnterEdit`, `CanvasExitEdit`, `CanvasEnterBackground`.
Each is added to the two exhaustive routing match lists
(`command_dispatch/{session,mod}.rs`) and handled in `handle_canvas_command`.

## 5. Live in-card render (S2)

The edit card must show the **live buffer** and a cursor, not a frozen snapshot.

- **Sync.** While `interaction == EditCard`, before each canvas redraw the app
  layer re-derives the edit card's `BlockSnapshot` from the active buffer
  (`AppState::text`) centred on the cursor line, windowed by `context_lines`,
  re-highlighted via `highlight_snippet`. `start_line = cursor_line −
  context_lines`. This makes the card scroll with the cursor — a mini viewport.
  Implemented as `AppShell::canvas_sync_edit_card()`, called on the render path
  whenever the canvas is active.
- **Cursor.** `interaction` carries `cursor_line/col` (kept in sync). The
  renderer draws (a) the active-line band on `cursor_line − (start_line−1)`
  (already exists, just retargeted) and (b) a thin caret rect at
  `code_x + col·char_w`, in the theme caret colour, clipped to the card.
- **Reuse.** Everything else (gutter, syntax spans, horizontal clip, "+N more")
  is the existing Phase A card body renderer — no second pipeline.

This is the highest-risk part (novel, not GUI-verifiable here). It is isolated
behind `canvas_sync_edit_card` + a caret block in the renderer; if it regresses
it can be disabled without affecting S1/S3.

## 6. Background render (S3)

`update_canvas_content` reads `canvas.interaction`:
- `Background` → **no scrim**, **no hint bar**, cards drawn dimmed (reduced
  border/halo) so the editor underneath is fully usable with cards as floating
  reference.
- `Navigate` → full Phase A chrome (scrim, focus ring, hint bar).
- `EditCard` → scrim + the edit card rendered live with caret; hint bar shows
  edit-relevant keys (`Esc Exit`, `gd Def`, `gr Refs`, `⌘S Save`).

## 7. Status pill

`update_statusbar_content`'s `canvas_active: bool` becomes
`canvas_label: Option<&str>`:
- `Navigate` → `Some("CANVAS")`
- `EditCard` → `Some("CANVAS·EDIT")`
- `Background` / `None` → `None` → the normal editor-mode pill (NORMAL/INSERT/…).

`StatusbarLayoutKey.canvas_active: bool` → `canvas_label: Option<String>`.

## 8. gd / gr chaining from a card

In S2 the active buffer **is** the card's file and the cursor is real, so the
existing `gd`/`gr` machinery already resolves at the card cursor. The canvas
relation-spawn path (`canvas_submit_definition/references`) keys off the
*focal* origin today; for Phase B chaining it should key off the **edit card's**
current cursor location when editing. Spawned cards append to the column as in
Phase A. (Lands last; if time-boxed it degrades gracefully to "gd jumps the
buffer", which still works.)

## 9. Read-only invariant (revisited)

Phase A's invariant — "opening/closing the canvas never mutates the document" —
still holds for S1/S3. S2 **intentionally** edits (that is the feature). The
existing test `open_canvas_builds_focused_focal_block_readonly` asserts only that
*opening* is read-only and stays valid. No Phase-A test asserts edit-time
read-only-ness.

## 10. Staging (each lands green: build + `cargo test --lib` + clippy on new files)

- **B1 — Model + commands (TDD).** `CanvasInteraction` enum + `CanvasState`
  fields/methods (`begin_edit`, `end_edit`, `enter_background`, `focus_navigate`,
  `set_edit_cursor`, queries) + `AppState` thin wrappers. New `Command`s + both
  dispatch lists. Pure unit tests for every transition.
- **B2 — Routing.** `KeybindingContext.canvas_interaction`; `build_context`
  focus rule; `resolve` Esc interceptor; `resolve_canvas_focus` Enter/Esc remap;
  `open_canvas_mode` re-grab; `handle_canvas_command` arms.
- **B3 — Edit buffer-switch + live render.** `begin_edit` → `open_file` +
  cursor position; `canvas_sync_edit_card`; caret + retargeted band in renderer;
  Background render branch.
- **B4 — Polish.** Status pill 3-state; hint bars per state; exit-edit snapshot
  refresh; gd/gr chaining from the card cursor.

## 11. Risks / mitigations

- **Input-router regression** (high blast radius, no existing tests): the Esc
  interceptor is tightly guarded (`canvas active && sub-state && Normal mode`);
  the state machine underneath is fully unit-tested so the *logic* is proven
  even though the wiring can't be click-tested here.
- **Live-render perf** (re-highlight each frame while editing): windowed to
  `2·context_lines+1` lines — tiny; only runs in S2.
- **Cannot GUI-verify**: everything testable is covered by unit tests; an
  adversarial multi-agent review pass runs over the final diff.

## 12. Out of scope (Phase B)

Multi-cursor across cards; simultaneous editing of two cards; live connector
re-routing while typing; collaborative/remote; persisting canvas layout.

## 13. Revision (2026-06-20) — in-card edit feel, per-card context, key-repeat

After a GUI review the user flagged three corrections; all are implemented.

### 13.1 "Edit IN the card, not open a different file"
The single-buffer editor reality is unchanged — entering a card still switches
the active buffer to the card's file **under the hood** (so the full vim /
insert / undo / LSP machinery drives it as a real mini main-editor). The fix is
to make that switch invisible and reversible:

- **Restore on exit.** `canvas_end_edit` (and `close_canvas` while editing) now
  call `canvas_restore_origin_buffer`, which re-activates the canvas's **focal
  anchor** file + cursor. Navigate therefore always sits on the file the canvas
  was opened from; the user is never stranded in the card's file. Restore prefers
  the focal buffer's **in-memory** copy (`activate_open_text_buffer_for_path`),
  falling back to disk — so it succeeds even if the focal file vanished from disk
  mid-edit. Background (S3) is user-controlled and is **not** force-restored.
- **Visual containment.** The scrim deepens to `0.66` while editing (hides the
  switched buffer behind the canvas); the edit-target card's halo grows to `5.0`
  so it reads as the active pane.
- **Status bar.** During Navigate/EditCard the filename shows the focal document,
  never the switched card file (`canvas_anchor_origin`). The dirty dot and
  diagnostics intentionally still track the live edit target (a save cue).

### 13.2 `+`/`-` adjusts ONLY the focused card
Context size is now **per card** (`CanvasBlock.context_lines`, seeded from the
session default at spawn). `canvas_change_focused_context` adjusts only the
focused relation card (clamped 3–24, no-op on the focal anchor / at bounds);
`canvas_apply_focused_context` re-sources just that card and re-grows it **in
place** (top-left fixed, downward only) — no re-stack, so a hand-arranged layout
survives a `+`/`-`. The old session-wide re-source-all path is removed. The edit
window uses the editing card's own `context_lines`, so a card grown with `+`
keeps its span when promoted to a live editor.

### 13.3 Key-repeat in canvas mode
`Command::supports_press_and_hold_repeat` now includes the canvas Navigate
commands (`CanvasMove*`, `CanvasPan*`, `CanvasFocus*`, `CanvasContext{Expand,
Shrink}`), so holding `hjkl` / `⇧hjkl` / arrows / `+`/`-` repeats. These are
structurally unreachable outside S1 (focus == Canvas only in Navigate), so they
never auto-repeat in EditCard/Background. In-card `hjkl` already repeats via the
editor's own `Move*` entries.

### 13.4 Verification
Unit tests cover per-card context (only-focused / in-place / clamp / focal-skip),
the edit round-trip restore (incl. focal-file-vanished), and canvas key-repeat
routing. Full lib suite green (946 passed); two-agent adversarial review run over
the diff (one finding — focal-file-vanished stranding — fixed above).
