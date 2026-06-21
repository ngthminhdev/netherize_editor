# NetherCanvas — In-Card Editing v2 (no buffer switch)

> Status: model approved by the user (Option A: reuse the editor engine via a
> scoped swap). Supersedes the Phase B §13 "edit-in-card" approach, which
> switched the active buffer and was rejected after a GUI review.

## 1. Problem (from the GUI review)

The Phase B edit-in-card switched the **active buffer** to the card's file
(`begin_edit → open_file(card)`). Consequences the user rejected:

1. **The main editor showed the card's file** — it renders the active buffer.
   The user requires the main editor to keep showing the **gc-origin file**;
   editing happens *only* inside the card.
2. The card's caret rendered at the wrong position.
3. `gd`/`gr` inside a card jumped the main editor instead of spawning new cards.
4. Cards pile into one tall column on open (no auto-arrange).

## 2. Core decision

The editor is single-buffer (one live `self.text: Rope`). To edit a card
**without** disturbing the main editor, the card gets its **own** edit state
(`CardEditSession`) and the full editor engine operates on it via a **scoped
swap** around command dispatch. `self.text` always holds the original file, so
the main editor render path is **unchanged**.

This reverts the Phase B §13 buffer-switch machinery: `canvas_begin_edit`'s
`open_file`, `canvas_restore_origin_buffer`, `activate_open_text_buffer_for_path`,
the statusbar filename override, and the heavy edit scrim are all removed (the
main editor is now genuinely the original file — nothing to hide or restore).
Per-card context (Phase B §13.2) and key-repeat (§13.3) are **kept**.

## 3. Components

### 3.1 `CardEditSession` (new, `src/app/app_state/`)
Holds the full text-edit state for the one card being edited:
```
block: BlockId, path: PathBuf,
text: Rope,
view: TextBufferViewState,   // cursor (char_idx + target_col), selection, visual-line, scroll
history: EditHistory,
transaction: Option<PendingTransaction>,
dirty: bool,
mode: ModeState,
visual_block_anchor_line/col: Option<usize>,
```
Stored as `AppState.canvas_edit_session: Option<CardEditSession>`. One session at
a time (you edit one card, save/leave, move on); multi-card unsaved sessions are
a future refinement.

### 3.2 Scoped swap (`AppState`)
`swap_card_session(&mut self, s: &mut CardEditSession)` `mem::swap`s the session's
fields with the matching `AppState` fields (text, view via
`text_buffer_view_state`/`restore_*`, history, current_transaction, dirty,
mode_state, visual_block anchors, active_file ↔ card path) and clears
`cached_line_starts`. Calling it twice (in, then out) is symmetric.

### 3.3 Dispatch hook (`AppShell`)
`handle_command_with_count_impl` gains a top-of-function route:
```
if self.app_state.canvas_editing_block().is_some()
   && command.is_card_editing_command() {
    return self.dispatch_card_editing_command(command, repeat_count);
}
```
`dispatch_card_editing_command`: take the session out of `AppState`, `swap` it in,
call the normal dispatch body (`..._impl_inner`, the current body minus the card
route — extracted to avoid re-entry), `swap` out, put the session back, then
`canvas_sync_edit_card` so the render reflects the edit. **No buffer switch, no
tab, no extra revision churn on the main buffer.**

`Command::is_card_editing_command()` (new, like `supports_press_and_hold_repeat`):
the editor text/cursor/mode set — `InsertChar/InsertText/Backspace/Newline/
InsertTab`, all `Move*`, `Delete*`/`Change*`/`Operate`/`Substitute*`,
`Paste*`, visual-selection commands, `SwitchMode`, `Undo`/`Redo`,
`MatchBracket`, `Save` (so ⌘S targets the card file via the swapped `active_file`),
search-in-buffer. Canvas/panel/app/picker commands are **excluded** — they run
normally on the real state. `gd`/`gr` are excluded and redirected (§3.5).

### 3.4 begin/sync/end (`app_state/canvas.rs`, `commands_canvas.rs`)
- `canvas_begin_edit`: focused relation card → read its file into a Rope →
  build `CardEditSession` with the cursor at the symbol `(line, col)` → set
  interaction `EditCard` → store the session. **Does not** touch `self.text`,
  `active_file`, or `self.buffers`.
- `canvas_sync_edit_card`: window the **session** text around the **session**
  cursor at the card's `context_lines`; re-highlight; update the card snapshot +
  caret. (No longer reads the active buffer.)
- `canvas_end_edit`: clear interaction → Navigate. Keep the session stashed iff
  `dirty` (so re-entering the card resumes unsaved edits); else drop it. No buffer
  restore (nothing was switched).
- `close_canvas`: drop the session. No restore.

### 3.5 Caret (fix #b)
The caret derives from the **session cursor**. Window row = `cursor_line -
window_start`; x = `code_x + cursor_col * char_w` (same `code_x`/gutter the body
uses). The active-line band uses the session cursor line. The Phase B bug came
from reading `origin.lsp_line` / the active-buffer cursor; the session is the
single source of truth now.

### 3.6 `gd`/`gr` in a card (fix #c)
New commands `CanvasCardDef` / `CanvasCardRefs`. While `EditCard`, the editor's
`gd`/`gr` are redirected to these (intercept at command level when
`canvas_editing_block().is_some()`). They submit `textDocument/definition` /
`references` for the **card file URI** at the **session cursor** position; the
results flow through the existing canvas redirect
(`canvas_def_request_id`/`refs_request_id` → `attach_canvas_relations`) and spawn
**new cards** on the canvas — never a main-editor jump. Best-effort: the card
file is resolved by the workspace LSP even if it is not the active buffer.

### 3.7 Auto-arrange on open (fix #d)
`CanvasState.user_arranged: bool` (set `true` on the first `move_focused` /
`toggle_pin`). While `!user_arranged`, after each relation spawn run
`canvas_auto_arrange(vw, vh)`: pack the relation cards into uniform-width columns
to the right of the focal, each column holding as many as fit in the viewport
height, overflowing into additional columns — so all cards are visible and tidy
on open. Once the user moves/pins, auto-arrange is frozen (manual placement wins).

## 4. Data flow (editing a card)
1. Navigate, focus card (app.go:26). `Enter` → `begin_edit`: session = app.go
   rope, cursor at 26. `self.text` stays cmd/main.go. Main editor unchanged.
2. Keystroke (e.g. `ciw`) → dispatch hook → swap session in → full vim engine
   edits the session rope → swap out → `self.text` is cmd/main.go again →
   `canvas_sync_edit_card` refreshes the card from the session.
3. Render: main editor = cmd/main.go (untouched); card = live session view + caret.
4. `⌘S` (card-editing command) → swapped-in → saves app.go. Session `dirty=false`.
5. `gd` on a symbol in the card → spawns a new def card on the canvas.
6. `Esc`(normal) → `end_edit` → Navigate. Main editor never moved.

## 5. What is reverted vs kept
- **Reverted** (Phase B §13.1): `open_file` switch in `begin_edit`,
  `canvas_restore_origin_buffer`, `activate_open_text_buffer_for_path`, statusbar
  filename override, edit scrim 0.66 (back to the light Phase-A scrim so the
  original file stays visible behind the floating cards).
- **Kept**: per-card `context_lines` + `+`/`-` (§13.2), canvas key-repeat
  (§13.3), the edit-card halo emphasis, the 4-state machine + two-stage Esc.

## 6. Testing
- Swap isolation: edit a session, assert the **main buffer is byte-for-byte
  unchanged**, the session holds the edit, and the main cursor is unmoved.
- Round-trip: begin_edit (main buffer unchanged) → simulate an insert via the
  swap → end_edit → main buffer still original; session dirty.
- Undo in card: edit + undo affects only the session.
- Caret mapping: cursor (line,col) → (row,x) within the window.
- Auto-arrange: N cards fit columns; freezes after a move/pin.
- `is_card_editing_command` classification (editor cmds yes; canvas/app no).
- Full lib suite green; adversarial review over the diff (can't GUI-test).

## 7. Risks
- **Swap completeness**: a text-edit field not in the bundle would leak between
  card and main editor. Mitigation: the bundle mirrors the existing per-buffer
  swap (`save_current_text_buffer_history`); unit test asserts main-buffer
  invariance after a card edit.
- **Re-entry**: the dispatch hook must call the non-card inner path. Mitigation:
  extract `..._impl_inner`; the hook is the only caller of the card path.
- **LSP/search/completion in-card** are best-effort (not swapped): acceptable;
  documented. The card is for editing, not full IDE services, in this iteration.

## 8. Out of scope (this iteration)
Multi-card simultaneous unsaved sessions; full LSP completion/diagnostics inside
the card; live connector re-routing while typing.

## 9. Adversarial-review fixes (two-agent review over the diff)

The first cut routed card commands through the SHELL pipeline
(`handle_command_with_count_impl`) and swapped only the obvious text fields. The
review found that pulls in shell-side side effects the swap can't isolate, plus
several un-swapped data fields. Fixes:

- **Core dispatch, not shell pipeline.** `dispatch_card_editing_command` now runs
  the card command through the **core** `dispatch_command_with_clipboard_count`
  (AppState + clipboard) instead of `handle_command_with_count_impl`. This stops
  card edits from firing the shell's LSP `didChange`, syntax parse,
  highlight-span reconcile, AI-inline, completion-clear and semantic-clear side
  effects against the main editor / the LSP server. The card re-highlights itself
  via `canvas_sync_edit_card`; core dispatch auto-commits transactions so undo
  works. Verified every `is_card_editing_command` is handled by the core dispatch.
- **Expanded swap bundle.** Added `pending_highlight_edits`, `search_highlights`,
  `folded_ranges`, `foldable_ranges_cache`, `auto_folded_long_lines`,
  `matched_bracket_pos`, `bracket_ripple_*`, `yank_flash_*`, **`jump_back_stack`,
  `jump_forward_stack`, `last_search_query`** — so a card edit can't clear the
  main editor's folds, corrupt its incremental highlights, pollute its jump list
  (Ctrl-O/I), or overwrite its search query.
- **Caret tab fix (#b).** The card text system now sets a fixed tab width
  (`CARD_TAB_WIDTH = 4`) and the edit caret expands tabs with the same width
  (visual column, not raw char column) — fixes the caret drifting left of the
  text on tab-indented lines (the screenshot symptom).
- **Same-file save guard.** If the card is the file the main editor has open,
  `begin_edit` seeds the session from the live `self.text` (not disk), and
  `canvas_save_edit_session` writes through `self.text` + clears `dirty` — so
  editing/saving such a card never silently clobbers the main editor's content.
- **target_col** is clamped to the cursor line's content length.

### Known limitations (documented, not blocking)
- One edit session at a time: entering a *different* card while a dirty session is
  stashed replaces it (⌘S before leaving to persist).
- `gd`/`gr` from a card with **unsaved** edits resolves against the on-disk file,
  so positions can be stale until ⌘S.
- Auto-arrange re-runs (and the camera re-anchors) on each async def/refs batch,
  so cards visibly reflow as results trickle in (until the user hand-arranges).
- Full LSP completion/diagnostics are not wired inside the card.

## 10. GUI-review refinements (round 3)

- **Block caret + exact position.** The in-card caret is a block (Normal/Visual)
  vs a thin beam (Insert), like the main editor (`caret_block` plumbed to the
  canvas renderer). Its X is measured by shaping the cursor prefix with the card
  text system + `CARD_TAB_WIDTH`, so it lands on the glyph (tabs handled).
- **In-card `gd`/`gr` spawns a CHILD card.** New `CanvasBlock.parent`: cards
  spawned from a card record it as parent; focus jumps to the new card; the
  connector is drawn **parent-card → child-card** (not from the main editor).
  Parent is threaded `canvas_card_spawn` → `canvas_def/refs_parent` →
  `attach_canvas_relations` → `canvas_add_relations_with_parent`; a closed parent
  falls back to the focal connector. While editing, only the edit-target rings
  (the spawned child rings once you return to Navigate).
- **`Shift`+`+`/`-` adjusts the focused card's WIDTH** (`canvas_change_focused_width`,
  in place, clamped to [0.5×, 2.5×] the uniform width); plain `=`/`-` still adjust
  context height. Both freeze auto-arrange (`user_arranged`).
- Perf: `read_file_lines` now `.take(end)` (stops scanning past the window).

## 11. In-card LSP — Phase 1: document lifecycle (fixes gd/gr regression)

### Bug (GUI report)
`gd`/`gr` inside a card stopped spawning child cards. Log:
`LspRequest failed (revision=0): definition rejected: LSP server not running`.

### Root cause (traced)
`canvas_card_spawn` submits `LspDefinitionRequest`/`LspReferencesRequest` for the
**card file URI**. The worker resolves the server via
`lsp_sessions.get_handle_by_uri(uri)` (`scheduler.rs`), which **only** matches a
session where `session.process.is_document_open(uri)` is true. But
`canvas_begin_edit` reads the card file into a `CanvasEditSession` and **never
sends `didOpen`** — so the card file is not a registered/open LSP document, the
handle lookup returns `None`, and the request is rejected. The focal `gc` path
works only because it queries the **active file** URI, which *is* `didOpen`'d at
startup; the card is typically a *different* file.

So the design's earlier "§3.6 best-effort, resolved by the workspace LSP even if
not the active buffer" claim was never actually delivered — `get_handle_by_uri`
requires the document to be **open**, not merely workspace-contained.

### Decision (Approach A — card-scoped, render at canvas; chosen by user)
Make the card a **real LSP-tracked document** for the duration of an edit
session. The worker's `LspDidOpen` handler resolves the server via
`sessions_for_document_uri` (matches a session whose **root contains** the URI),
registers the doc, and marks it open — after which `get_handle_by_uri` finds it
and `gd`/`gr` resolve. didChange keeps the server's view live so positions track
the in-card edits; didClose releases it on leave.

Phase 1 is *only* the document lifecycle (no completion/hover UI yet — those are
deferred Phases 2–3, render via canvas-scoped holders so the main editor's LSP UI
is never touched).

### Components (Phase 1)
- `AppState::canvas_card_lsp_target() -> Option<PathBuf>` (**pure, unit-tested**):
  `Some(card_path)` iff an edit session is active AND its path is neither the
  `active_file` nor an already-open text buffer — i.e. a file we must register
  ourselves. `None` when the doc is already open elsewhere (don't fight an
  existing open doc / don't risk closing the main editor's document).
- `AppState::canvas_edit_session_text() -> Option<String>`: the session's full
  rope as a string (the didOpen/didChange payload).
- AppShell fields: `canvas_card_lsp_open: Option<PathBuf>` (the doc we currently
  own as open; `None` if not ours) and `canvas_card_lsp_version: i32` (monotonic
  LSP document version for the card doc).
- AppShell helpers (mirror the active-file ones, parameterised by path+text):
  - `submit_canvas_card_did_open()`: target = `canvas_card_lsp_target()`; if
    `Some(p)` and `active_lsp_server.is_some()` → if a *different* doc is owned,
    didClose it first; reset version to 1; submit `LspDidOpen{uri, language_id,
    version, text}`; set `canvas_card_lsp_open = Some(p)`.
  - `submit_canvas_card_did_change()`: if `canvas_card_lsp_open == Some(p)` →
    bump version; submit `LspDidChange{uri, version, text=session text}`.
  - `submit_canvas_card_did_close()`: if `canvas_card_lsp_open.take()` is
    `Some(p)` → submit `LspDidClose{uri}`.

### Lifecycle hooks (`handle_canvas_command`, `dispatch_card_editing_command`)
- `CanvasEnterEdit` (after `canvas_begin_edit()` succeeds) → `did_open`.
- `dispatch_card_editing_command` (after `report.state_changed`) → `did_change`.
- `CanvasExitEdit` and `CanvasClose` → `did_close` (computed before the session is
  dropped; owns exactly one card doc at a time, matching "one session at a time").

### Edge cases / limitations (documented)
- **same-as-main / already-open buffer**: `canvas_card_lsp_target` returns `None`,
  so we never re-open or close a doc the main editor owns → no LSP desync, no
  accidental didClose of the active document. gd/gr still resolve (the doc is
  already open). The trade-off: such a card's gd/gr see the main editor's
  document content, not the card's unsaved edits.
- **child-card chaining**: entering a child card while owning the parent's doc
  didCloses the parent first (one owned doc at a time).
- **version**: a dedicated per-card counter (not the main buffer revision).
- didChange fires per state-changing card edit (small files — fine); debounce is a
  later optimisation.

### Testing
- Unit (pure): `canvas_card_lsp_target` → `Some` when card path ≠ active and not an
  open buffer; `None` when card path == active; `None` when card path matches an
  open text buffer; `None` with no session.
- The submit side effects are verified by GUI test (the test scheduler discards
  requests, so submits aren't observable in unit tests — consistent with the
  existing `submit_lsp_*` helpers, none of which are unit-tested).
- GUI: enter a card whose file ≠ the active file → `gd`/`gr` spawn child cards
  again (no "LSP server not running" rejection).

### Follow-up (GUI report): focus must auto-jump to the spawned child
After Phase 1 the child card spawned and the connector drew correctly, but the
focus did not visibly move to it — the user had to press `Esc` (back to Navigate)
first. Cause: `canvas_add_relations_with_parent` sets `state.focused = child`, but
the interaction stayed `EditCard{parent}`, and the renderer's focus ring is gated
on `edit_block.is_none()` (the edit-target rings while editing, not
`canvas.focused`). Fix: `AppState::canvas_end_edit_for_spawn(parent)` (pure,
unit-tested) — if still editing `parent`, leave EditCard (→ Navigate) and return
`true`; `attach_canvas_relations` calls it after a parent-spawned add and, on
`true`, `submit_canvas_card_did_close()`s the parent's LSP doc. The focus then
lands on the child (already focused) with no Esc. Guarded to only auto-exit when
still editing the card that spawned (don't yank focus if the user moved on); same
session keep/drop as the manual Esc path (no data loss).

## 12. Card-as-mini-editor polish + in-card LSP Phases 2 & 3

Delivered together (user: "làm cho xong", autonomous — no further approval).

### 12.1 Current-line highlight → main-editor style (renderer)
The card's active-line band used `blend(card_bg, rel_color, 0.18)` (a green/relation
tint). Replaced with the **exact** main-editor current-line look: the
`theme.editor.selection` color at low alpha (`* 0.22`, clamped `[0.10, 0.30]`), so
the card reads as a plain mini editor.

### 12.2 Mode badge + mode-colored ring (renderer)
`update_canvas_content(canvas, mode: EditorMode)` (was `caret_block: bool` — derived
internally now). For the **edit-target** card: the header shows the active mode
label (`NORMAL`/`INSERT`/`VISUAL`/`V-BLOCK`, via `mode_display_label`) in the mode
color (`mode_pill_color`) just left of the line-range, and the card's ring + header
underline use the mode color (`ring_color`). A navigation-focused card still rings
cyan; the rest are dim.

### 12.3 Phase 2 — in-card hover (`K`)
- AppShell `canvas_hover_request_id`. The card gate routes `Command::LspHover` →
  `canvas_card_hover()` (request `textDocument/hover` for the card path + session
  cursor). The result handler, when `canvas_hover_request_id` matches, flattens the
  doc blocks to lines (`flatten_hover_blocks_to_lines`) and fills the card overlay —
  never the main editor's FloatingBox.
- Rendered as a bordered doc box at the edit caret; dismissed by any edit/motion.

### 12.4 Phase 3 — in-card completion
- **State**: `CanvasState.card_overlay: Option<CardOverlay>` (`Hover(Vec<String>)`
  or `Completion(CardCompletionView{items,selected})`) — simple presentational data
  in the canvas model, filled by the app layer, drawn by the renderer (no
  cross-crate type coupling, renderer reads it from `CanvasState`). The source of
  truth for navigate/accept is AppShell `canvas_completion: Option<CompletionState>`
  (reuses `CompletionState::from_lsp_items`), mirrored into `card_overlay`.
- **Request**: typing an identifier char in a card (or `(`/trigger chars) →
  `submit_canvas_card_completion()` (card path + session cursor + session prefix via
  `canvas_edit_session_completion_context`). The result handler, on
  `canvas_completion_request_id` match (Insert mode + active session), builds the
  card `CompletionState` and mirrors the menu.
- **Render**: a menu at the caret (rows = label + dim detail; selected row
  highlighted) — shares `draw_card_overlay` with hover. (Fix: re-set the card text
  metrics inside `draw_card_overlay` since the bottom hint bar changed them.)
- **Navigate/accept**: `completion_visible` in the keymap context now ORs
  `canvas_completion.is_some()`, so Tab/Enter/Ctrl-n-p/Esc emit
  `CompletionAccept/Next/Prev/Close`; the card gate routes them to
  `handle_canvas_completion_command`. Accept = prefix-replace into the session
  (delete the current identifier prefix, insert the item's text) via the card
  editing dispatch (history/`didChange`/sync reused). Dismissed on non-identifier
  commands, leaving Insert, or leaving the card.

### 12.5 Limitations (documented)
- Completion accept is a **simple prefix-replace** — snippet placeholders (`$0`),
  multi-range `textEdit`s, and auto-import `additionalTextEdits` are not expanded.
- Completion requests fire per identifier keystroke (no debounce); stale results are
  dropped by request-id + Insert-mode check.
- Hover/completion render as flat monospace boxes (not the rich main-editor
  FloatingBox); `completionItem/resolve` docs are not fetched in-card.
- Rendering positions/sizes were implemented without a live GUI — needs a visual
  pass.

## 13. Shared completion menu (main editor + card use ONE component)

User asked why the card didn't reuse the main editor's completion UI. Reason: the
editor popup was a ~650-line block inlined in `update_editor_overlays`, hard-wired
to the editor buffers/geometry/caret, and was a **two-pane** layout (list + a
right-hand doc/hover panel) — not reusable. Decision (user): make ONE compact
single-column component, drop the doc panel, keep the kind badges, use it in both.

- **`editor/completion.rs::draw_completion_menu`** (new, `pub(crate)`): a
  surface-agnostic single-column menu — kind badge (icon) + match-highlighted
  label + dim inline detail + selection + scrollbar; **no doc panel, no footer**;
  compact widths (`min ≈ 220·ui_scale`, was `700`). Takes `CompletionMenuGeom`
  (anchor + bounds + font metrics + ui_scale) and `MenuRenderTargets` (disjoint
  `&mut` borrows of a text system + atlas + queue + chrome/glyph/icon vecs), so it
  writes into either the editor-overlay buffers or the canvas buffers.
- **Main editor**: the 650-line popup block in `update_editor_overlays` is replaced
  by a call to `draw_completion_menu` (anchor from `compute_caret_layout_with_folds`
  + the typed-prefix offset). The doc panel + footer are gone → smaller popup.
  `strip_markdown_inline` (only the doc panel used it) kept `#[allow(dead_code)]`
  for its tests / future reuse.
- **Card**: `update_canvas_content(canvas, mode, card_completion: Option<&CompletionState>)`
  now receives the card's `CompletionState` (AppShell `canvas_completion`, passed
  from `application.rs`) and renders it via the SAME `draw_completion_menu` at the
  card caret (`ui_scale = cam.zoom`). The earlier per-card mirror
  (`CanvasState.card_overlay::Completion` / `CardCompletionView`) is removed; the
  renderer reads `canvas_completion` directly. `CanvasState.card_hover:
  Option<Vec<String>>` remains for the `K` hover box (`draw_card_hover`).
- Result: editor + card completion are pixel-identical (one code path). Functional
  path (request/result/navigate/accept) unchanged — only the render was swapped, so
  low functional risk; the new look still needs a GUI pass.

## 14. GUI-review fixes — popup occlusion + "focus here ⇒ act here"

Two bugs from the GUI review.

### 14.1 In-card popups had no background coverage (text bleed-through)
The canvas renders in three sub-passes — **all chrome → all icons → all text** — and
the card code glyphs AND the popup glyphs share ONE text buffer. So a popup's
opaque background (a chrome quad) can't occlude code glyphs that render after it in
the single text pass; the code text bled through the hover/completion popup. (The
main editor avoids this because its popup renders in a separate later overlay pass.)
Fix: `draw_completion_menu`/`draw_card_hover` return their on-screen rect and use an
opaque bg; after drawing a popup the canvas **drops the card-code glyphs whose rect
overlaps the popup rect** (split off the popup's own glyphs, `retain` the
non-overlapping code glyphs, re-append the popup glyphs) so only the popup shows.

### 14.2 Commands leaked to the main editor while editing a card
Focus is in the card (EditCard = Editor focus), but the in-card gate only routed
SaveFile/gd/gr/hover + `is_card_editing_command`. So `Ctrl-Space`
(`TriggerCompletion`) opened the **main editor's** completion dropdown, and
`LspPreviewDefinition`/`LspRename`/`LspFormatDocument`/`CodeAction` + search/fold
commands acted on the gc-origin buffer behind the card. Fix (principle: *focus in
the card ⇒ action stays in the card*):
- `TriggerCompletion` → `submit_canvas_card_completion` (the card menu).
- `LspPreviewDefinition` → `canvas_card_spawn(true)` (like `gd`).
- Suppressed as a no-op while editing a card (not card-scoped yet):
  `LspRename`, `LspFormatDocument`, `CodeAction`, `OpenInFileSearch`, `SearchNext`,
  `SearchPrev`, `SearchWordUnderCursor`, `ToggleFold`, `ToggleFoldAll`.
- (Already routed earlier: hover `K`, `gd`/`gr`, completion nav/accept, ⌘S.)

## 15. Promote card → buffer tab, connector anchoring, return flow

User-approved design (forks chosen via Q&A). Three pieces.

### 15.1 Connector anchors at the gc symbol, not the live caret (fix)
Today the canvas connector starts at `editor_caret_screen` (the live caret), so
coding in the main editor drags the connector around (screenshot: it ran from the
`panic` line, not the `NewApp` symbol where `gc` was triggered). Fix: the connector
origin is the **focal symbol's editor line** projected to screen.
- Add `Renderer.editor_focal_screen: Option<[f32; 4]>`. During the editor render
  (viewport.rs), when the canvas is active, project the canvas **focal block's
  origin line/col** (`canvas.focal origin.lsp_line/character`) to a screen rect
  using the same viewport geometry + fold/scroll path the caret uses; clamp Y to
  the viewport edges when the focal line is scrolled off-screen.
- `canvas.rs` `focal_source` uses `editor_focal_screen` first, then falls back to
  `editor_caret_screen` / the focal world rect. So the connector stays put while the
  caret moves.

### 15.2 `o` = open the focused card as a real buffer tab
- New `Command::CanvasOpenCardBuffer`, bound to **`o`** in canvas Navigate focus
  (`input_map/focus.rs`). No-op on the focal anchor / with no focused relation card.
- Handler: take the focused card's `origin` → `open_file(origin.path)` → jump the
  cursor to `(origin.lsp_line, origin.lsp_character)` → **stash the canvas** (hide,
  state kept). Active buffer becomes the card file; the user codes normally.
- Enter (edit-in-card) is unchanged — `o` is the "go deeper / code longer" level.

### 15.3 Stash + restore (return flow)
- **Stash**: add `CanvasState.stashed: bool` (or a `Stashed` interaction). When
  stashed, `application.rs` calls `clear_canvas()` instead of `update_canvas_content`
  (fully hidden, not the dimmed S3 background) but `app_state.canvas` stays `Some`.
  A statusbar hint reads e.g. `canvas stashed · gc to return`.
- **Restore**: `gc` (`open_canvas_mode`) when a canvas exists + is stashed →
  un-stash + Navigate, restoring the same card layout. Because the canvas is paired
  with its **gc-origin file** (the connectors anchor to it), restore also switches
  the active buffer back to the focal file (`open_file(focal.origin.path)`), whose
  per-buffer cursor is preserved at the gc symbol. (The card file opened by `o`
  stays in the buffer list.)

### 15.4 Navigate-mode key map (after this change)
`hjkl` focus/move · `Enter` edit-in-card · **`o` open as tab (stash canvas)** ·
`gc` restore canvas · `Esc` background→close · `+`/`-` context · `Shift`+`+`/`-`
width · pin / close card.

### 15.5 Roadmap (out of scope here, noted)
Save-all + dirty indicator across multiple edited cards; in-card search/fold
(currently suppressed); canvas minimap/overview + layout persistence across
sessions; optional connector fade while actively coding.

### 15.6 Testing
- Pure: `CanvasState.stashed` round-trip (set on `o`, cleared on restore); restore
  re-targets the focal file. Connector-origin selection (focal over caret) is a
  renderer concern — GUI-verified.
- GUI: `o` opens the card file as a tab + hides the canvas; `gc` brings the canvas
  back over the focal file; the connector stays on the gc symbol while the caret
  moves.
