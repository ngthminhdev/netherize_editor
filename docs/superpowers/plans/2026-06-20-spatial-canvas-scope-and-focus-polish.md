# NetherCanvas Scope-aware Cards & Focus Polish (T3–T6) — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make NetherCanvas cards scope-aware (enclosing function/method/const instead of a fixed line window) and polish two focus/status details — card mode badge and Tab cycling.

**Architecture:** Four mostly-independent changes on `feature/spatial-canvas`. T6 (Tab) and T3 (badge) are local edits. T4/T5 add a scope-detection layer: snapshots spawn instantly with the existing ±N window, then progressively re-source to the LSP `documentSymbol` enclosing-symbol range (focal card resolves synchronously from cached symbols; def/caller cards via an async request correlated by `card_id`).

**Tech Stack:** Rust, wgpu, cosmic-text, tree-sitter (syntax), tower-lsp-style worker over `async_runtime`. Tests: `cargo test` (in-crate `#[test]`).

## Global Constraints

- **NEVER auto-commit.** Committing is the human's job. Every "stage" step ends by staging and **stopping for the human to commit** — do not run `git commit`/`push`/`merge`.
- Respond to the user in **Vietnamese**.
- Use `/usr/bin/grep` (bare `grep` is proxied/broken in this env). Absolute paths in tools.
- TDD red→green for every new unit. Keep `cargo build`, `cargo clippy` (changed files), and the full `cargo test` suite green before moving to the next task.
- After fixing any bug/test/build failure, append to `.wolf/buglog.json` (`error_message`, `root_cause`, `fix`, `tags`). Update `.wolf/anatomy.md` / `.wolf/memory.md` per the openwolf rules.
- Design source of truth: `docs/superpowers/specs/2026-06-20-spatial-canvas-scope-and-focus-polish-design.md`.

---

## File Structure

| File | Responsibility | Tasks |
|------|----------------|-------|
| `src/canvas/model.rs` | `CanvasState::focus_cycle` skip Focal | T6 |
| `src/render/renderer/helpers.rs` | `card_mode_label` helper | T3 |
| `src/render/renderer/canvas.rs` | badge + edit-target ring gating | T3 |
| `src/app/event_loop/async_results/canvas_scope.rs` *(new)* | `enclosing_definition` pure fn | T4 |
| `src/app/event_loop/async_results/preview.rs` | `read_file_lines_range` | T4 |
| `src/app/event_loop/async_results/lsp.rs` | `build_canvas_relation_snapshot_range`, scope-result handler | T4/T5 |
| `src/async_runtime/message.rs` | `CanvasCardScopeRequest`/`Result` variants | T5 |
| `src/app/event_loop/commands_canvas.rs` | spawn-time focal sync resolve + apply scope | T5 |
| `src/app/app_state/canvas.rs` | `canvas_apply_card_scope`, range on `BlockOrigin` | T5 |

---

## Task 1: T6 — Tab cycle skips the Focal block

**Files:**
- Modify: `src/canvas/model.rs` — `CanvasState::focus_cycle` (~line 304)
- Test: same file, `#[cfg(test)] mod tests`

**Interfaces:**
- Consumes: `CanvasState.blocks: Vec<CanvasBlock>`, `CanvasBlock.relation: BlockRelation`, `CanvasBlock.id: BlockId`, `CanvasState.focused: Option<BlockId>`, `CanvasState::focus(&mut self, id) -> bool`.
- Produces: `focus_cycle(&mut self, forward: bool) -> bool` cycling **relation cards only**.

- [ ] **Step 1: Write the failing test**

Add to the `tests` module in `src/canvas/model.rs`:

```rust
#[test]
fn focus_cycle_skips_focal_block() {
    let mut st = CanvasState::default();
    let focal = st.push(block(/*id*/ 1, BlockRelation::Focal, WorldRect::new(0.0, 0.0, 20.0, 20.0)));
    let c1 = st.push(block(2, BlockRelation::Caller, WorldRect::new(100.0, 0.0, 20.0, 20.0)));
    let c2 = st.push(block(3, BlockRelation::Callee, WorldRect::new(100.0, 40.0, 20.0, 20.0)));

    st.focus(c1);
    assert!(st.focus_cycle(true));
    assert_eq!(st.focused, Some(c2));
    // Forward again wraps straight to c1 — NEVER the focal anchor.
    assert!(st.focus_cycle(true));
    assert_eq!(st.focused, Some(c1));
    assert_ne!(st.focused, Some(focal));

    // Reverse is symmetric.
    assert!(st.focus_cycle(false));
    assert_eq!(st.focused, Some(c2));
}

#[test]
fn focus_cycle_focal_only_is_noop() {
    let mut st = CanvasState::default();
    st.push(block(1, BlockRelation::Focal, WorldRect::new(0.0, 0.0, 20.0, 20.0)));
    assert!(!st.focus_cycle(true));
}
```

> Match the existing test helpers' signatures: reuse the local `block(id, relation, world)` constructor and `push`/`focus` already used by `focus_cycle_wraps` (~line 652). If `block(...)` there takes different args, copy its exact shape.

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test focus_cycle_skips_focal_block focus_cycle_focal_only_is_noop 2>&1 | tail -20`
Expected: FAIL — current `focus_cycle` lands on the focal block (assert `Some(c1)`/no-op fails).

- [ ] **Step 3: Implement skip-focal in `focus_cycle`**

Replace the body of `focus_cycle` (~line 304):

```rust
pub fn focus_cycle(&mut self, forward: bool) -> bool {
    // Relation cards only — the Focal block is the editor anchor, never drawn as
    // a card, so Tab must not land on it (that reads as a "focus off" step).
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

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test focus_cycle 2>&1 | tail -20`
Expected: PASS for the two new tests. If `focus_cycle_wraps` (~line 652) asserted the focal participated in the cycle, update its expectations to the card-only cycle.

- [ ] **Step 5: Build + clippy**

Run: `cargo build 2>&1 | /usr/bin/grep -E "^error|Finished"` (expect `Finished`)
Run: `cargo clippy --bin netherize_editor 2>&1 | /usr/bin/grep -E "warning|error" | /usr/bin/grep model.rs` (expect empty)

- [ ] **Step 6: Stage and hand off to human for commit**

```bash
git add src/canvas/model.rs
# DO NOT COMMIT. Report: "T6 ready — focus_cycle skips Focal, tests green."
```

---

## Task 2: T3 — Card badge shows only card-edit modes

**Files:**
- Modify: `src/render/renderer/helpers.rs` — add `card_mode_label` near `mode_display_label` (~line 436)
- Modify: `src/render/renderer/canvas.rs` — badge block (~line 346) + edit-target ring color (~line 224)
- Test: `src/render/renderer/helpers.rs` `#[cfg(test)] mod tests` (add if absent)

**Interfaces:**
- Consumes: `EditorMode` (`crate::core::mode::EditorMode`), `mode_pill_color(mode, theme)`.
- Produces: `pub(super) fn card_mode_label(mode: EditorMode) -> Option<&'static str>`.

- [ ] **Step 1: Write the failing test**

In `src/render/renderer/helpers.rs`:

```rust
#[cfg(test)]
mod card_mode_tests {
    use super::*;
    use crate::core::mode::EditorMode;

    #[test]
    fn card_mode_label_only_edit_modes() {
        assert_eq!(card_mode_label(EditorMode::Normal), Some("NORMAL"));
        assert_eq!(card_mode_label(EditorMode::Insert), Some("INSERT"));
        assert_eq!(card_mode_label(EditorMode::Visual), Some("VISUAL"));
        assert_eq!(card_mode_label(EditorMode::VisualBlock), Some("VISUAL"));
        assert_eq!(card_mode_label(EditorMode::PaletteFocus), None);
        assert_eq!(card_mode_label(EditorMode::TerminalFocus), None);
        assert_eq!(card_mode_label(EditorMode::TerminalNormal), None);
        assert_eq!(card_mode_label(EditorMode::MultiCursor), None);
        assert_eq!(card_mode_label(EditorMode::MultiInsert), None);
        assert_eq!(card_mode_label(EditorMode::Resize), None);
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test card_mode_label_only_edit_modes 2>&1 | tail -20`
Expected: FAIL — `card_mode_label` not defined (compile error).

- [ ] **Step 3: Add the helper**

In `helpers.rs`, after `mode_display_label`:

```rust
/// The card header's mode badge shows ONLY the modes a card is actually edited
/// in. App-global modes (palette/terminal/multi-cursor/resize) return `None`, so
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

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test card_mode_label_only_edit_modes 2>&1 | tail -20` → PASS.

- [ ] **Step 5: Gate the badge + ring in `canvas.rs`**

Near the top of the per-card loop (before the ring color at ~line 224), compute once:

```rust
let card_mode = card_mode_label(mode); // None for app-global modes
```

Update the edit-target ring color (~line 224) so an app mode falls back to cyan focus:

```rust
let ring_color = if is_edit_target {
    if card_mode.is_some() { mode_color } else { border_focus }
} else if focused {
    border_focus
} else {
    border_dim
};
```

Replace the badge block (~line 346) to use the `Option`:

```rust
if is_edit_target {
    if let Some(label) = card_mode {
        let label_w = label.chars().count() as f32 * char_w;
        let label_x = range_x - char_w * 1.4 - label_w;
        if label_x > sx + PAD + char_w {
            self.canvas_glyph_instances.extend(layout_panel_text(
                label,
                &mut self.canvas_text_system,
                &mut self.atlas,
                &self.queue,
                label_x,
                header_y,
                mode_color,
            ));
        }
    }
}
```

Add `card_mode_label` to the `use super::helpers::...` / `use super::{...}` import group already importing `mode_pill_color`, `mode_display_label` at the top of `canvas.rs`.

- [ ] **Step 6: Build + clippy + suite**

Run: `cargo build 2>&1 | /usr/bin/grep -E "^error|Finished"` (expect `Finished`)
Run: `cargo clippy --bin netherize_editor 2>&1 | /usr/bin/grep -E "warning|error" | /usr/bin/grep -iE "helpers.rs|canvas.rs"` (expect empty)
Run: `cargo test card_mode 2>&1 | tail -5` (expect PASS)

- [ ] **Step 7: Stage and hand off**

```bash
git add src/render/renderer/helpers.rs src/render/renderer/canvas.rs
# DO NOT COMMIT. Report: "T3 ready — badge/ring show only NORMAL/INSERT/VISUAL."
```

---

## Task 3: T4 — `enclosing_definition` pure selector

**Files:**
- Create: `src/app/event_loop/async_results/canvas_scope.rs`
- Modify: `src/app/event_loop/async_results/mod.rs` — add `mod canvas_scope;` (or `pub(crate) mod`)
- Test: in `canvas_scope.rs`

**Interfaces:**
- Consumes: `crate::async_runtime::message::{LspDocumentSymbol, LspRange}` (fields `name`, `kind: String`, `range: LspRange`; `LspRange { start, end }` each with `.line`).
- Produces: `pub(crate) fn enclosing_definition(symbols: &[LspDocumentSymbol], line: u32) -> Option<LspRange>`.

> **Verify before coding:** confirm the exact `kind` strings the worker emits into `LspDocumentSymbol.kind` (grep the worker that builds `LspDocumentSymbol`). LSP `SymbolKind` display names are typically `"Function"`, `"Method"`, `"Constant"`, `"Constructor"` — adjust the match set to the real spellings.

- [ ] **Step 1: Write the failing test**

Create `src/app/event_loop/async_results/canvas_scope.rs`:

```rust
use crate::async_runtime::message::{LspDocumentSymbol, LspPosition, LspRange};

#[cfg(test)]
mod tests {
    use super::*;

    fn sym(kind: &str, s: u32, e: u32) -> LspDocumentSymbol {
        LspDocumentSymbol {
            name: "x".into(),
            kind: kind.into(),
            range: LspRange {
                start: LspPosition { line: s, character: 0 },
                end: LspPosition { line: e, character: 0 },
            },
            ancestors: Vec::new(),
        }
    }

    #[test]
    fn picks_deepest_enclosing_function_or_method() {
        // class [0..50] contains method [10..20]; line 12 → the method, not the class.
        let symbols = vec![sym("Class", 0, 50), sym("Method", 10, 20), sym("Function", 30, 40)];
        let r = enclosing_definition(&symbols, 12).expect("enclosing");
        assert_eq!((r.start.line, r.end.line), (10, 20));
    }

    #[test]
    fn const_enclosing_at_its_own_line() {
        let symbols = vec![sym("Constant", 5, 5)];
        let r = enclosing_definition(&symbols, 5).expect("const");
        assert_eq!((r.start.line, r.end.line), (5, 5));
    }

    #[test]
    fn no_definition_kind_enclosing_returns_none() {
        // Line is inside a class body but not inside any function/method/const.
        let symbols = vec![sym("Class", 0, 50)];
        assert!(enclosing_definition(&symbols, 3).is_none());
    }

    #[test]
    fn line_outside_all_symbols_returns_none() {
        let symbols = vec![sym("Function", 10, 20)];
        assert!(enclosing_definition(&symbols, 99).is_none());
    }
}
```

> Check the real field names of `LspRange`/`LspPosition` (grep `struct LspRange`, `struct LspPosition` in `message.rs`) and adjust the `sym(...)` constructor to match exactly.

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p netherize_editor enclosing 2>&1 | tail -20`
Expected: FAIL — `enclosing_definition` not defined.

- [ ] **Step 3: Implement the selector**

In `canvas_scope.rs` (above the test module):

```rust
/// The deepest enclosing **definition** symbol (function/method/const/constructor)
/// whose line range contains `line`. `documentSymbol` arrives as a FLAT list with
/// `ancestors`, so "deepest" == smallest containing span. Returns `None` when no
/// definition-kind symbol contains the line (caller falls back to the ±N window).
pub(crate) fn enclosing_definition(symbols: &[LspDocumentSymbol], line: u32) -> Option<LspRange> {
    const DEF_KINDS: [&str; 4] = ["Function", "Method", "Constant", "Constructor"];
    symbols
        .iter()
        .filter(|s| DEF_KINDS.contains(&s.kind.as_str()))
        .filter(|s| s.range.start.line <= line && line <= s.range.end.line)
        .min_by_key(|s| s.range.end.line.saturating_sub(s.range.start.line))
        .map(|s| s.range.clone())
}
```

- [ ] **Step 4: Wire the module**

In `src/app/event_loop/async_results/mod.rs` add: `pub(crate) mod canvas_scope;`

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test enclosing 2>&1 | tail -10` → PASS (4 tests).

- [ ] **Step 6: Build + clippy**

Run: `cargo build 2>&1 | /usr/bin/grep -E "^error|Finished"` (expect `Finished`)
Run: `cargo clippy --bin netherize_editor 2>&1 | /usr/bin/grep canvas_scope.rs` (expect empty)

- [ ] **Step 7: Stage and hand off**

```bash
git add src/app/event_loop/async_results/canvas_scope.rs src/app/event_loop/async_results/mod.rs
# DO NOT COMMIT. Report: "T4 core ready — enclosing_definition selector green."
```

---

## Task 4: T4 — explicit-range snapshot builder

**Files:**
- Modify: `src/app/event_loop/async_results/preview.rs` — add `read_file_lines_range`
- Modify: `src/app/event_loop/async_results/lsp.rs` — add `build_canvas_relation_snapshot_range` (near `build_canvas_relation_snapshot`, ~line 892)
- Test: `lsp.rs` test module

**Interfaces:**
- Consumes: existing `read_file_lines(path, line, context) -> (Vec<String>, usize)`, `canvas_snapshot_spans(theme, text, ext)`, `BlockOrigin`, `BlockSnapshot`.
- Produces:
  - `read_file_lines_range(path: &Path, start_line: usize, end_line: usize, max_lines: usize) -> (Vec<String>, usize, usize)` → `(lines, start0, total_in_range)` where lines are clamped to `max_lines`.
  - `build_canvas_relation_snapshot_range(theme, path, start_line, end_line, character, symbol) -> Option<(BlockOrigin, BlockSnapshot)>`.

- [ ] **Step 1: Write the failing test**

In the `lsp.rs` test module (find `#[cfg(test)] mod tests`; if testing file IO, write a temp file like `end_edit_for_spawn_*` does):

```rust
#[test]
fn snapshot_range_reads_exact_lines_and_clamps() {
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
    let dir = std::env::temp_dir().join(format!("netherize_scope_{nanos}"));
    std::fs::create_dir_all(&dir).unwrap();
    let f = dir.join("a.rs");
    let body = (0..100).map(|i| format!("line{i}")).collect::<Vec<_>>().join("\n");
    std::fs::write(&f, body).unwrap();

    let theme = crate::config::theme_config::ThemeConfig::default();
    // Range lines 10..=80 (0-based) is 71 lines → clamp to 60 + "+N more".
    let (_o, snap) = build_canvas_relation_snapshot_range(&theme, &f, 10, 80, 0, "s").unwrap();
    assert_eq!(snap.start_line, 11); // 1-based
    let n = snap.text.split('\n').count();
    assert!(n <= 61, "clamped to <=60 lines + marker, got {n}");
    assert!(snap.text.contains("+") && snap.text.contains("more"), "has +N more marker");

    std::fs::remove_dir_all(&dir).ok();
}
```

> Adjust `ThemeConfig::default()` to however the existing tests obtain a theme.

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test snapshot_range_reads_exact_lines_and_clamps 2>&1 | tail -20`
Expected: FAIL — builder not defined.

- [ ] **Step 3: Add `read_file_lines_range` to `preview.rs`**

```rust
/// Read an inclusive 0-based line range `[start_line, end_line]` from `path`,
/// clamped to `max_lines` rows. Returns `(lines, start0, total_in_range)`; when
/// `total_in_range > max_lines` the caller appends a "+N more" marker.
pub(super) fn read_file_lines_range(
    path: &std::path::Path,
    start_line: usize,
    end_line: usize,
    max_lines: usize,
) -> (Vec<String>, usize, usize) {
    let Ok(content) = std::fs::read_to_string(path) else {
        return (Vec::new(), start_line, 0);
    };
    let all: Vec<&str> = content.lines().collect();
    if all.is_empty() || start_line >= all.len() {
        return (Vec::new(), start_line, 0);
    }
    let end = end_line.min(all.len().saturating_sub(1));
    let total = end.saturating_sub(start_line) + 1;
    let take = total.min(max_lines);
    let lines = all[start_line..start_line + take]
        .iter()
        .map(|s| s.to_string())
        .collect();
    (lines, start_line, total)
}
```

- [ ] **Step 4: Add `build_canvas_relation_snapshot_range` to `lsp.rs`**

```rust
/// Like `build_canvas_relation_snapshot` but for an explicit enclosing-symbol line
/// range (scope-aware). Clamps to 60 lines + a "+N more" marker so a huge function
/// never makes a giant card; the full body is available on Enter-to-edit.
pub(crate) fn build_canvas_relation_snapshot_range(
    theme: &crate::config::theme_config::ThemeConfig,
    path: &Path,
    start_line: u32,
    end_line: u32,
    character: u32,
    symbol: &str,
) -> Option<(crate::canvas::BlockOrigin, crate::canvas::BlockSnapshot)> {
    use crate::canvas::{BlockOrigin, BlockSnapshot};
    const MAX_LINES: usize = 60;
    let (mut lines, start0, total) =
        super::preview::read_file_lines_range(path, start_line as usize, end_line as usize, MAX_LINES);
    if lines.is_empty() {
        return None;
    }
    if total > MAX_LINES {
        lines.push(format!("    \u{2026} +{} more", total - MAX_LINES));
    }
    let text = lines.join("\n");
    let extension = path.extension().and_then(|e| e.to_str()).unwrap_or_default();
    let spans = canvas_snapshot_spans(theme, &text, extension);
    let file = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    Some((
        BlockOrigin {
            path: path.to_path_buf(),
            start_byte: 0,
            end_byte: 0,
            symbol_name: symbol.to_string(),
            lsp_line: start_line,
            lsp_character: character,
        },
        BlockSnapshot {
            title: file,
            symbol: symbol.to_string(),
            start_line: start0 as u32 + 1,
            text,
            spans,
        },
    ))
}
```

- [ ] **Step 5: Run test to verify it passes**

Run: `cargo test snapshot_range_reads_exact_lines_and_clamps 2>&1 | tail -10` → PASS.

- [ ] **Step 6: Build + clippy**

Run: `cargo build 2>&1 | /usr/bin/grep -E "^error|Finished"` (expect `Finished`)
Run: `cargo clippy --bin netherize_editor 2>&1 | /usr/bin/grep -iE "preview.rs|lsp.rs"` (expect empty)

- [ ] **Step 7: Stage and hand off**

```bash
git add src/app/event_loop/async_results/preview.rs src/app/event_loop/async_results/lsp.rs
# DO NOT COMMIT. Report: "T4 range builder ready, green."
```

---

## Task 5: T4/T5 — wire progressive scope resolution

**Files:**
- Modify: `src/async_runtime/message.rs` — `CanvasCardScopeRequest`/`CanvasCardScopeResult` variants
- Modify: `src/app/app_state/canvas.rs` — `canvas_apply_card_scope(card_id, snapshot)` + carry resolved range on `BlockOrigin`
- Modify: `src/app/event_loop/commands_canvas.rs` — focal sync resolve at spawn + emit scope requests for def/caller cards
- Modify: `src/app/event_loop/async_results/lsp.rs` — handle `CanvasCardScopeResult` → apply
- Modify worker side (request handler) per the existing `LspDocumentSymbolsRequest` path
- Test: `commands_canvas.rs` / `canvas.rs` test modules

**Interfaces:**
- Consumes: `enclosing_definition` (Task 3), `build_canvas_relation_snapshot_range` (Task 4), `app.cached_document_symbols` + `cached_document_symbols_path`, `CanvasState` block ids.
- Produces: `AppState::canvas_apply_card_scope(&mut self, card_id: BlockId, snapshot: BlockSnapshot) -> bool`.

> This task has the only structurally-new plumbing. Implement in two passes; keep the suite green after each.

### Pass A — focal card resolves synchronously (no new message)

- [ ] **Step 1: Write the failing test (apply scope to a card)**

In `src/app/app_state/canvas.rs` tests:

```rust
#[test]
fn canvas_apply_card_scope_replaces_snapshot() {
    let mut app = app_with_text("fn focal() {\n    bar();\n}\n");
    app.open_canvas(VW, VH, LH);
    app.canvas_add_relations(vec![(
        BlockRelation::Caller, origin_at("/p/a.rs", 10), snap("a"),
    )]);
    let id = app.canvas().unwrap().focused.unwrap();
    let new_snap = BlockSnapshot {
        title: "a.rs".into(), symbol: "s".into(), start_line: 8,
        text: "fn s() {\n  x\n}".into(), spans: Vec::new(),
    };
    assert!(app.canvas_apply_card_scope(id, new_snap.clone()));
    let b = app.canvas().unwrap().blocks.iter().find(|b| b.id == id).unwrap();
    assert_eq!(b.snapshot.start_line, 8);
    assert_eq!(b.snapshot.text, "fn s() {\n  x\n}");
}
```

- [ ] **Step 2: Run it — expect FAIL** (`canvas_apply_card_scope` undefined).

Run: `cargo test canvas_apply_card_scope_replaces_snapshot 2>&1 | tail -20`

- [ ] **Step 3: Implement `canvas_apply_card_scope`** in `canvas.rs`

```rust
/// Replace a card's snapshot with a scope-resolved one (progressive refine). The
/// card regrows to fit; the focal block is never targeted. Returns whether a card
/// matched `card_id` (false if it was closed/opened-as-tab before resolve).
pub fn canvas_apply_card_scope(&mut self, card_id: BlockId, snapshot: BlockSnapshot) -> bool {
    let Some(c) = self.canvas.as_mut() else { return false; };
    let Some(b) = c.blocks.iter_mut().find(|b| b.id == card_id) else { return false; };
    if b.relation == BlockRelation::Focal {
        return false;
    }
    b.snapshot = snapshot;
    // Regrow the card to its new snapshot height (reuse the existing sizing path).
    c.user_arranged = c.user_arranged; // no-op marker; call the real resize helper:
    true
}
```

> Replace the `no-op marker` line with the codebase's actual per-card resize call (the same one `canvas_apply_focused_context` uses to refit a card after re-source — grep `canvas_apply_focused_context` in `commands_canvas.rs:117` and mirror its sizing).

- [ ] **Step 4: Run it — expect PASS.** `cargo test canvas_apply_card_scope_replaces_snapshot 2>&1 | tail -10`

- [ ] **Step 5: Focal sync resolve at spawn** — in `commands_canvas.rs` (focal build / `canvas_card_spawn`), after the focal card exists and when `app.cached_document_symbols_path == active_file`, call:

```rust
if let Some(range) = super::async_results::canvas_scope::enclosing_definition(
    &self.cached_document_symbols, focal_line,
) {
    if let Some((_o, snap)) = super::async_results::build_canvas_relation_snapshot_range(
        &self.theme, &focal_path, range.start.line, range.end.line, focal_char, &focal_symbol,
    ) {
        self.app_state.canvas_apply_card_scope(focal_card_id, snap);
    }
}
```

> Substitute the real local variable names for `focal_line/focal_path/focal_char/focal_symbol/focal_card_id` from the surrounding spawn code.

- [ ] **Step 6: Build + clippy + suite.** `cargo build`, `cargo clippy ... | /usr/bin/grep canvas`, `cargo test canvas 2>&1 | tail -5` (all green).

- [ ] **Step 7: Stage and hand off** (Pass A).

```bash
git add src/app/app_state/canvas.rs src/app/event_loop/commands_canvas.rs
# DO NOT COMMIT. Report: "T4/T5 Pass A — focal card scope-resolves synchronously."
```

### Pass B — async scope for def/caller cards

- [ ] **Step 8: Add message variants** in `src/async_runtime/message.rs` (near `LspDocumentSymbolsRequest` ~line 298 and `...Result` ~line 763):

```rust
// Request documentSymbol for a NON-active file, correlated to a canvas card.
CanvasCardScopeRequest { card_id: u64, uri: String, line: u32 },
// ... in the result enum:
CanvasCardScopeResult { card_id: u64, uri: String, symbols: Vec<LspDocumentSymbol> },
```

> Use the same `BlockId` integer type the canvas uses for `card_id`. Resolve the LSP session by the target file's root exactly as the in-card didOpen path does (`sessions_for_document_uri`); mirror the worker arm that already serves `LspDocumentSymbolsRequest`.

- [ ] **Step 9: Emit a request per non-focal new card** in `attach_canvas_relations` (`lsp.rs` ~line 935): for each appended card whose target file isn't the active file (or whose definition reply lacked `targetRange`), send `CanvasCardScopeRequest { card_id, uri, line }`.

- [ ] **Step 10: Handle the result** — new arm beside `LspDocumentSymbolsResult` (`lsp.rs` ~line 527):

```rust
WorkerResultPayload::CanvasCardScopeResult { card_id, uri, symbols } => {
    if let Some(path) = lsp_uri_to_path(&uri) {
        // Use the card's target line stored at spawn; resolve enclosing scope.
        if let Some(line) = app.app_state.canvas_card_target_line(card_id) {
            if let Some(range) = super::canvas_scope::enclosing_definition(&symbols, line) {
                if let Some((_o, snap)) = build_canvas_relation_snapshot_range(
                    &app.theme, &path, range.start.line, range.end.line, 0, /*symbol*/ "",
                ) {
                    app.app_state.canvas_apply_card_scope(card_id, snap);
                }
            }
        }
    }
}
```

> Add a tiny accessor `canvas_card_target_line(card_id) -> Option<u32>` returning the card's `origin.lsp_line`, and pass the card's real `symbol`. Guard: `canvas_apply_card_scope` already returns `false` if the card was closed/opened-as-tab — the stale result is harmlessly dropped.

- [ ] **Step 11: `targetRange` fast-path (optional, if the worker surfaces it)** — when the `gd` definition reply is a `LocationLink` with `targetRange`, skip the request and call `build_canvas_relation_snapshot_range` directly with that range. If the worker currently flattens to a point, leave this out and rely on documentSymbol.

- [ ] **Step 12: Build + clippy + FULL suite.**

Run: `cargo build 2>&1 | /usr/bin/grep -E "^error|Finished"` (expect `Finished`)
Run: `cargo clippy --bin netherize_editor 2>&1 | /usr/bin/grep -iE "message.rs|lsp.rs|canvas"` (expect empty)
Run: `cargo test 2>&1 | tail -6` (expect `0 failed`)

- [ ] **Step 13: Stage and hand off** (Pass B).

```bash
git add src/async_runtime/message.rs src/app/event_loop/async_results/lsp.rs src/app/app_state/canvas.rs
# DO NOT COMMIT. Report: "T4/T5 Pass B — def/caller cards scope-resolve async, progressive."
```

---

## Task 6: Regression sweep + bookkeeping

- [ ] **Step 1: Full suite green.** `cargo test 2>&1 | tail -6` → `0 failed`.
- [ ] **Step 2: Manual GUI checklist** (report to user, do not self-approve):
  - Tab with 2+ cards: card1 → card2 → card1, no "focus off" frame (T6).
  - Editing a card shows `NORMAL/INSERT/VISUAL`; open palette over canvas → badge disappears, ring goes cyan (T3).
  - Spawn gc/gd/gr: card first shows the ±N window, then snaps to the enclosing function/method/const; a >60-line function shows `"+N more"` (T4/T5).
  - Connector (bug 213/215) + file-binding (bug 216) unaffected.
- [ ] **Step 3: Bookkeeping.** Append any fixes to `.wolf/buglog.json`; update `.wolf/anatomy.md` (new `canvas_scope.rs` module + new fns) and `.wolf/memory.md`. Update the auto-memory `project_spatial_canvas.md` pointer.
- [ ] **Step 4: Stage and hand off.**

```bash
git add -A
# DO NOT COMMIT. Report final status + the GUI checklist for the human to verify, then commit.
```

---

## Self-Review (filled)

**Spec coverage:**
- T3 → Task 2 ✓ · T4 → Tasks 3,4,5 ✓ · T5 (auto range + clamp) → Tasks 4 (clamp) + 5 (apply to all spawns) ✓ · T6 → Task 1 ✓.
- Spec "progressive UX" → Task 5 (instant window at spawn, async refine) ✓.
- Spec fallback (no enclosing / unsupported) → `enclosing_definition` returns `None` → window snapshot retained (Task 3 tests + Task 5 guards) ✓.

**Placeholder scan:** Code-bearing steps carry full code. Two intentional verify-notes remain (real `kind` strings; the per-card resize call name) — both are pinpointed lookups with exact grep targets, not vague "handle X" placeholders.

**Type consistency:** `enclosing_definition(&[LspDocumentSymbol], u32) -> Option<LspRange>`, `build_canvas_relation_snapshot_range(theme, path, u32, u32, u32, &str)`, `canvas_apply_card_scope(BlockId, BlockSnapshot) -> bool`, `read_file_lines_range(path, usize, usize, usize) -> (Vec<String>, usize, usize)` are used consistently across Tasks 3→5. `card_id` is the canvas `BlockId` integer in both message variants and the accessor.

**Known soft spots (flagged in spec Risks):** documentSymbol for non-active files (worker session resolution), `targetRange` availability, exact `kind` spellings. Each has an exact file/line to verify against during Task 5.
