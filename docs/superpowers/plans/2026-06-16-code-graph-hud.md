# Code Graph HUD & Blast Radius Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Press `gp` on a function to open a 2D node-graph overlay showing its callers/callees with a blast-radius risk coloring, navigable with vim `hjkl`, jumping to code with `Enter`.

**Architecture:** A pure-logic `codegraph` module (JSON parsing, graph model, risk classification, hjkl navigation, column layout) is unit-tested in isolation. The editor invokes the external `codegraph` CLI through the async worker (Module 05), mirroring `scheduler/fzf.rs`. Results open an overlay (`OverlayKind::CodeGraphHud`) rendered with existing quad/glyph primitives in `render/renderer/editor/overlays.rs`. A new input-focus context routes `hjkl/Enter/Esc` while open.

**Tech Stack:** Rust 2024, tokio (`Command`), serde/serde_json, wgpu (existing `RegionDrawInstance`/`layout_panel_text`), winit, tree-sitter (focal-symbol resolution).

**Spec:** `docs/superpowers/specs/2026-06-16-code-graph-hud-design.md`

---

## File Structure

**New — pure domain (crate root `src/codegraph/`, unit-tested, zero UI deps):**
- `src/codegraph/mod.rs` — module re-exports + `mod` declarations.
- `src/codegraph/cli_json.rs` — serde structs matching `codegraph --json` + raw parse fns.
- `src/codegraph/model.rs` — `NodeRole`, `RiskLevel`, `GraphNode`, `CodeGraphModel`, `build_model`.
- `src/codegraph/navigation.rs` — `FocusCursor` hjkl state machine.
- `src/codegraph/layout.rs` — column coordinate layout + overflow cap.

**Modified — integration:**
- `src/main.rs` (or crate root module list) — add `mod codegraph;`.
- `src/async_runtime/message.rs` — `WorkerRequestPayload::CodeGraphQuery`, `WorkerResultPayload::{CodeGraphReady, CodeGraphFailed}`, `RequestTopic` variant if needed.
- `src/async_runtime/scheduler/codegraph.rs` — NEW: spawn CLI, parse, emit (mirror `fzf.rs`).
- `src/async_runtime/scheduler/mod.rs` — `mod codegraph;` + re-export.
- `src/async_runtime/scheduler/dispatch.rs` — route `CodeGraphQuery` to the runner.
- `src/core/command_ids.rs` — `CODEGRAPH_OPEN_GRAPH_HUD` id.
- `src/core/commands.rs` — register `codegraph.open_graph_hud`.
- `config/keymaps/default.toml` — `g p` binding.
- `src/app/app_state/mod.rs` — `CodeGraphHudState` field + `InputFocusContext::CodeGraph` + `codegraph` extension item.
- `src/app/app_state/code_graph_hud.rs` — NEW: HUD state struct + methods.
- `src/app/input/handler.rs` (+ `input_map/focus.rs`) — route keys when CodeGraph focused.
- `src/workbench/overlay_manager.rs` — `OverlayKind::CodeGraphHud`.
- `src/render/renderer/editor/overlays.rs` — draw the HUD (pills, edges, focus ring, panels).
- `src/app/event_loop/` — submit the query on command; handle worker result → open overlay.

---

## Phase 1 — Pure domain core (no UI, fully unit-tested)

### Task 1: serde structs for `codegraph --json`

**Files:**
- Create: `src/codegraph/cli_json.rs`
- Create: `src/codegraph/mod.rs`
- Modify: `src/main.rs` (add `mod codegraph;` next to the other top-level `mod` lines)

Reference JSON (captured from `codegraph` v1.0.1):
- `callers <sym> --json` → `{ "symbol": "...", "callers": [ {name,kind,filePath,startLine} ] }`
- `callees <sym> --json` → `{ "symbol": "...", "callees": [ {...} ] }`
- `impact <sym> --json --depth 2` → `{ "symbol","depth","nodeCount","edgeCount","affected":[{...}] }`

- [ ] **Step 1: Write the failing test**

In `src/codegraph/cli_json.rs`:
```rust
use serde::Deserialize;

/// One symbol entry as emitted by codegraph's `--json` array elements.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct CgSymbol {
    pub name: String,
    pub kind: String,
    #[serde(rename = "filePath")]
    pub file_path: String,
    #[serde(rename = "startLine")]
    pub start_line: u32,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CallersJson {
    pub symbol: String,
    #[serde(default)]
    pub callers: Vec<CgSymbol>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CalleesJson {
    pub symbol: String,
    #[serde(default)]
    pub callees: Vec<CgSymbol>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ImpactJson {
    pub symbol: String,
    #[serde(default)]
    pub affected: Vec<CgSymbol>,
}

pub fn parse_callers(json: &str) -> Result<CallersJson, String> {
    serde_json::from_str(json).map_err(|e| format!("callers json: {e}"))
}
pub fn parse_callees(json: &str) -> Result<CalleesJson, String> {
    serde_json::from_str(json).map_err(|e| format!("callees json: {e}"))
}
pub fn parse_impact(json: &str) -> Result<ImpactJson, String> {
    serde_json::from_str(json).map_err(|e| format!("impact json: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_callers_with_camelcase_fields() {
        let json = r#"{"symbol":"validate","callers":[
            {"name":"login","kind":"function","filePath":"src/auth.rs","startLine":58}]}"#;
        let parsed = parse_callers(json).unwrap();
        assert_eq!(parsed.symbol, "validate");
        assert_eq!(parsed.callers.len(), 1);
        assert_eq!(parsed.callers[0].name, "login");
        assert_eq!(parsed.callers[0].file_path, "src/auth.rs");
        assert_eq!(parsed.callers[0].start_line, 58);
    }

    #[test]
    fn parses_impact_affected_array() {
        let json = r#"{"symbol":"validate","depth":2,"nodeCount":2,"edgeCount":1,
            "affected":[{"name":"login","kind":"function","filePath":"src/auth.rs","startLine":58}]}"#;
        let parsed = parse_impact(json).unwrap();
        assert_eq!(parsed.affected.len(), 1);
        assert_eq!(parsed.affected[0].name, "login");
    }

    #[test]
    fn missing_array_defaults_empty() {
        let parsed = parse_callees(r#"{"symbol":"x"}"#).unwrap();
        assert!(parsed.callees.is_empty());
    }
}
```

In `src/codegraph/mod.rs`:
```rust
pub mod cli_json;
```

In `src/main.rs`, add `mod codegraph;` alongside the existing top-level `mod` declarations.

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p netherize_editor codegraph::cli_json 2>&1 | tail -20`
Expected: compile error (module new) then PASS once it builds — if it fails to compile because `mod codegraph;` is missing, add it. First real run should pass; if you wrote the test before the impl, it fails with "cannot find function".

- [ ] **Step 3: Implement** — already inlined above (structs + parse fns). No extra code.

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test codegraph::cli_json 2>&1 | tail -20`
Expected: `test result: ok. 3 passed`

- [ ] **Step 5: Commit**

```bash
git add src/codegraph/cli_json.rs src/codegraph/mod.rs src/main.rs
git commit -m "feat(codegraph): parse codegraph CLI json output"
```

---

### Task 2: Graph model + risk classification (with dedup)

**Files:**
- Create: `src/codegraph/model.rs`
- Modify: `src/codegraph/mod.rs`

Rules:
- Nodes = focal (center) + callers + callees.
- **Dedup** callers/callees by `(name, file_path, start_line)` — `callees` output repeats entries.
- Risk: Center→`Focal`; every callee→`Safe`; caller→`High` if it appears in `impact.affected` (matched by name+file+line), else `Medium`.

- [ ] **Step 1: Write the failing test**

In `src/codegraph/model.rs`:
```rust
use crate::codegraph::cli_json::{CalleesJson, CallersJson, CgSymbol, ImpactJson};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NodeRole { Center, Caller, Callee }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RiskLevel { Focal, Safe, Medium, High }

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GraphNode {
    pub name: String,
    pub kind: String,
    pub file_path: String,
    pub line: u32,
    pub role: NodeRole,
    pub risk: RiskLevel,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodeGraphModel {
    pub focal: GraphNode,
    pub callers: Vec<GraphNode>,
    pub callees: Vec<GraphNode>,
}

impl CodeGraphModel {
    pub fn is_empty(&self) -> bool {
        self.callers.is_empty() && self.callees.is_empty()
    }
}

fn ident(s: &CgSymbol) -> (String, String, u32) {
    (s.name.clone(), s.file_path.clone(), s.start_line)
}

/// Build the renderable model from the three CLI json payloads.
/// `focal_name`/`focal_file`/`focal_line` describe the symbol under the caret.
pub fn build_model(
    focal_name: &str,
    focal_file: &str,
    focal_line: u32,
    callers: &CallersJson,
    callees: &CalleesJson,
    impact: &ImpactJson,
) -> CodeGraphModel {
    use std::collections::HashSet;

    let affected: HashSet<(String, String, u32)> =
        impact.affected.iter().map(ident).collect();

    let mut seen: HashSet<(String, String, u32)> = HashSet::new();
    let dedup = |src: &[CgSymbol], seen: &mut HashSet<(String, String, u32)>| -> Vec<CgSymbol> {
        let mut out = Vec::new();
        for s in src {
            if seen.insert(ident(s)) {
                out.push(s.clone());
            }
        }
        out
    };

    let focal = GraphNode {
        name: focal_name.to_string(),
        kind: "focal".to_string(),
        file_path: focal_file.to_string(),
        line: focal_line,
        role: NodeRole::Center,
        risk: RiskLevel::Focal,
    };
    // Focal must never duplicate into the side columns.
    seen.insert((focal_name.to_string(), focal_file.to_string(), focal_line));

    let callers = dedup(&callers.callers, &mut seen)
        .into_iter()
        .map(|s| {
            let risk = if affected.contains(&ident(&s)) {
                RiskLevel::High
            } else {
                RiskLevel::Medium
            };
            GraphNode { name: s.name, kind: s.kind, file_path: s.file_path,
                line: s.start_line, role: NodeRole::Caller, risk }
        })
        .collect();

    let callees = dedup(&callees.callees, &mut seen)
        .into_iter()
        .map(|s| GraphNode { name: s.name, kind: s.kind, file_path: s.file_path,
            line: s.start_line, role: NodeRole::Callee, risk: RiskLevel::Safe })
        .collect();

    CodeGraphModel { focal, callers, callees }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegraph::cli_json::{parse_callees, parse_callers, parse_impact};

    fn fixtures() -> (CallersJson, CalleesJson, ImpactJson) {
        let callers = parse_callers(r#"{"symbol":"validate","callers":[
            {"name":"login","kind":"function","filePath":"src/auth.rs","startLine":58},
            {"name":"check","kind":"method","filePath":"src/sess.rs","startLine":23}]}"#).unwrap();
        let callees = parse_callees(r#"{"symbol":"validate","callees":[
            {"name":"find","kind":"function","filePath":"src/db.rs","startLine":3},
            {"name":"find","kind":"function","filePath":"src/db.rs","startLine":3}]}"#).unwrap();
        let impact = parse_impact(r#"{"symbol":"validate","affected":[
            {"name":"login","kind":"function","filePath":"src/auth.rs","startLine":58}]}"#).unwrap();
        (callers, callees, impact)
    }

    #[test]
    fn caller_in_impact_is_high_else_medium() {
        let (cr, ce, im) = fixtures();
        let m = build_model("validate", "src/user.rs", 142, &cr, &ce, &im);
        assert_eq!(m.focal.risk, RiskLevel::Focal);
        assert_eq!(m.callers[0].name, "login");
        assert_eq!(m.callers[0].risk, RiskLevel::High);   // in impact
        assert_eq!(m.callers[1].risk, RiskLevel::Medium); // not in impact
    }

    #[test]
    fn callees_are_safe_and_deduped() {
        let (cr, ce, im) = fixtures();
        let m = build_model("validate", "src/user.rs", 142, &cr, &ce, &im);
        assert_eq!(m.callees.len(), 1); // duplicate "find" collapsed
        assert_eq!(m.callees[0].risk, RiskLevel::Safe);
    }
}
```

Add to `src/codegraph/mod.rs`:
```rust
pub mod model;
```

- [ ] **Step 2: Run test to verify it fails** — Run: `cargo test codegraph::model 2>&1 | tail -20` — Expected: FAIL (before adding `pub mod model;` it won't compile; after, tests pass since impl is inlined).

- [ ] **Step 3: Implement** — inlined above.

- [ ] **Step 4: Run test to verify it passes** — Run: `cargo test codegraph::model 2>&1 | tail -20` — Expected: `2 passed`.

- [ ] **Step 5: Commit**
```bash
git add src/codegraph/model.rs src/codegraph/mod.rs
git commit -m "feat(codegraph): build graph model with risk + dedup"
```

---

### Task 3: `hjkl` navigation state machine

**Files:**
- Create: `src/codegraph/navigation.rs`
- Modify: `src/codegraph/mod.rs`

Focus model: `Center`, `Caller(idx)`, `Callee(idx)`. Rules:
- `h`: Callee→Center; Center→first Caller (if any); Caller→stays.
- `l`: Caller→Center; Center→first Callee (if any); Callee→stays.
- `j`: within column, idx+1 clamped; Center→no-op.
- `k`: within column, idx-1 clamped; Center→no-op.

- [ ] **Step 1: Write the failing test**

In `src/codegraph/navigation.rs`:
```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Focus { Center, Caller(usize), Callee(usize) }

#[derive(Debug, Clone, Copy)]
pub enum NavKey { Left, Right, Up, Down }

/// Pure transition. `n_callers`/`n_callees` are the visible counts.
pub fn navigate(focus: Focus, key: NavKey, n_callers: usize, n_callees: usize) -> Focus {
    match (focus, key) {
        (Focus::Callee(_), NavKey::Left) => Focus::Center,
        (Focus::Center, NavKey::Left) if n_callers > 0 => Focus::Caller(0),
        (Focus::Caller(_), NavKey::Right) => Focus::Center,
        (Focus::Center, NavKey::Right) if n_callees > 0 => Focus::Callee(0),

        (Focus::Caller(i), NavKey::Down) => Focus::Caller((i + 1).min(n_callers.saturating_sub(1))),
        (Focus::Caller(i), NavKey::Up)   => Focus::Caller(i.saturating_sub(1)),
        (Focus::Callee(i), NavKey::Down) => Focus::Callee((i + 1).min(n_callees.saturating_sub(1))),
        (Focus::Callee(i), NavKey::Up)   => Focus::Callee(i.saturating_sub(1)),

        (other, _) => other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn center_moves_into_columns() {
        assert_eq!(navigate(Focus::Center, NavKey::Left, 3, 3), Focus::Caller(0));
        assert_eq!(navigate(Focus::Center, NavKey::Right, 3, 3), Focus::Callee(0));
    }

    #[test]
    fn columns_return_to_center() {
        assert_eq!(navigate(Focus::Caller(2), NavKey::Right, 3, 3), Focus::Center);
        assert_eq!(navigate(Focus::Callee(1), NavKey::Left, 3, 3), Focus::Center);
    }

    #[test]
    fn vertical_clamps_within_column() {
        assert_eq!(navigate(Focus::Caller(0), NavKey::Up, 3, 3), Focus::Caller(0));
        assert_eq!(navigate(Focus::Caller(2), NavKey::Down, 3, 3), Focus::Caller(2));
        assert_eq!(navigate(Focus::Caller(0), NavKey::Down, 3, 3), Focus::Caller(1));
    }

    #[test]
    fn empty_column_blocks_entry() {
        assert_eq!(navigate(Focus::Center, NavKey::Left, 0, 3), Focus::Center);
    }
}
```

Add to `src/codegraph/mod.rs`: `pub mod navigation;`

- [ ] **Step 2: Run test to verify it fails** — Run: `cargo test codegraph::navigation 2>&1 | tail -20` — Expected: builds then PASS (impl inlined). If you stage the test first, FAIL "cannot find function navigate".

- [ ] **Step 3: Implement** — inlined above.

- [ ] **Step 4: Run test to verify it passes** — Run: `cargo test codegraph::navigation 2>&1 | tail -20` — Expected: `4 passed`.

- [ ] **Step 5: Commit**
```bash
git add src/codegraph/navigation.rs src/codegraph/mod.rs
git commit -m "feat(codegraph): hjkl navigation state machine"
```

---

### Task 4: Column layout with overflow cap

**Files:**
- Create: `src/codegraph/layout.rs`
- Modify: `src/codegraph/mod.rs`

Given an HUD content rect and node counts, compute pill rects for center + each visible column slot. Cap **8 per column**; when capped, the focused index scrolls the visible window.

- [ ] **Step 1: Write the failing test**

In `src/codegraph/layout.rs`:
```rust
pub const MAX_PER_COLUMN: usize = 8;

/// A laid-out pill rect: [x, y, w, h].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PillRect { pub x: f32, pub y: f32, pub w: f32, pub h: f32 }

#[derive(Debug, Clone, PartialEq)]
pub struct GraphLayout {
    pub center: PillRect,
    pub callers: Vec<PillRect>,  // one per VISIBLE caller slot
    pub callees: Vec<PillRect>,
    pub caller_window_start: usize,
    pub callee_window_start: usize,
    pub caller_overflow: usize,  // hidden count below the window
    pub callee_overflow: usize,
}

/// Compute the window [start, start+len) that keeps `focused` visible.
pub fn visible_window(total: usize, focused: usize, cap: usize) -> (usize, usize) {
    if total <= cap { return (0, total); }
    let start = focused.saturating_sub(cap - 1).min(total - cap);
    (start.min(focused), (start.min(focused) + cap).min(total))
}

/// `content`: [x,y,w,h] of the HUD graph area (below top bar, above footer).
pub fn layout(
    content: [f32; 4],
    n_callers: usize,
    n_callees: usize,
    caller_focus: Option<usize>,
    callee_focus: Option<usize>,
) -> GraphLayout {
    let [cx, cy, cw, ch] = content;
    let pill_w = (cw * 0.24).clamp(120.0, 200.0);
    let pill_h = 44.0;
    let center_w = (cw * 0.28).clamp(160.0, 220.0);
    let center_h = 62.0;

    let center = PillRect {
        x: cx + (cw - center_w) * 0.5,
        y: cy + (ch - center_h) * 0.5,
        w: center_w, h: center_h,
    };

    let column = |n: usize, focus: usize, left_x: f32| -> (Vec<PillRect>, usize, usize) {
        let (start, end) = visible_window(n, focus, MAX_PER_COLUMN);
        let visible = end - start;
        let gap = 14.0;
        let total_h = visible as f32 * pill_h + (visible.saturating_sub(1)) as f32 * gap;
        let top = cy + (ch - total_h) * 0.5;
        let rects = (0..visible).map(|i| PillRect {
            x: left_x, y: top + i as f32 * (pill_h + gap), w: pill_w, h: pill_h,
        }).collect();
        (rects, start, n.saturating_sub(end))
    };

    let (callers, caller_window_start, caller_overflow) =
        column(n_callers, caller_focus.unwrap_or(0), cx + 12.0);
    let (callees, callee_window_start, callee_overflow) =
        column(n_callees, callee_focus.unwrap_or(0), cx + cw - 12.0 - pill_w);

    GraphLayout { center, callers, callees,
        caller_window_start, callee_window_start, caller_overflow, callee_overflow }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn window_shows_all_when_under_cap() {
        assert_eq!(visible_window(5, 0, 8), (0, 5));
    }

    #[test]
    fn window_scrolls_to_keep_focus_visible() {
        // 20 items, focus at 19, cap 8 -> window [12, 20)
        assert_eq!(visible_window(20, 19, 8), (12, 20));
    }

    #[test]
    fn layout_caps_visible_and_reports_overflow() {
        let l = layout([0.0, 0.0, 800.0, 400.0], 20, 0, Some(0), None);
        assert_eq!(l.callers.len(), MAX_PER_COLUMN);
        assert_eq!(l.caller_overflow, 12);
    }

    #[test]
    fn center_is_horizontally_centered() {
        let l = layout([0.0, 0.0, 800.0, 400.0], 0, 0, None, None);
        let mid = l.center.x + l.center.w * 0.5;
        assert!((mid - 400.0).abs() < 0.5);
    }
}
```

Add to `src/codegraph/mod.rs`: `pub mod layout;`

- [ ] **Step 2: Run test to verify it fails** — Run: `cargo test codegraph::layout 2>&1 | tail -20` — Expected: builds then PASS.

- [ ] **Step 3: Implement** — inlined above.

- [ ] **Step 4: Run test to verify it passes** — Run: `cargo test codegraph::layout 2>&1 | tail -20` — Expected: `4 passed`.

- [ ] **Step 5: Commit**
```bash
git add src/codegraph/layout.rs src/codegraph/mod.rs
git commit -m "feat(codegraph): column layout with overflow cap"
```

---

## Phase 2 — Async worker integration

### Task 5: Worker request/result payloads

**Files:**
- Modify: `src/async_runtime/message.rs` (enum `WorkerRequestPayload` ~line 138; enum `WorkerResultPayload` ~line 582)

- [ ] **Step 1: Add the request variant**

In `WorkerRequestPayload`, add:
```rust
    /// Run codegraph callers+callees+impact for the symbol under the caret.
    CodeGraphQuery {
        symbol: String,
        focal_file: String,
        focal_line: u32,
        workspace_root: std::path::PathBuf,
    },
```

- [ ] **Step 2: Add the result variants**

In `WorkerResultPayload`, add:
```rust
    CodeGraphReady {
        model: crate::codegraph::model::CodeGraphModel,
    },
    CodeGraphFailed {
        /// `not_installed` distinguishes "codegraph binary missing" from a query error.
        not_installed: bool,
        message: String,
    },
```

- [ ] **Step 3: Verify it compiles**

Run: `cargo check 2>&1 | tail -20`
Expected: compiles (CodeGraphModel already derives Clone/Debug/PartialEq). Fix any missing `Eq` derive mismatch by ensuring `WorkerResultPayload` does not require `Eq` (it derives only `Debug, Clone`).

- [ ] **Step 4: Commit**
```bash
git add src/async_runtime/message.rs
git commit -m "feat(codegraph): worker request/result payloads"
```

---

### Task 6: codegraph scheduler runner

**Files:**
- Create: `src/async_runtime/scheduler/codegraph.rs`
- Modify: `src/async_runtime/scheduler/mod.rs` (add `pub(super) mod codegraph;` near the other `mod fzf;` lines)

Mirror `fzf.rs` structure: emit `Started`, run async, emit `Result` + `Completed` or `Failed`. The runner runs `codegraph sync` then the three queries.

- [ ] **Step 1: Write the runner**

In `src/async_runtime/scheduler/codegraph.rs`:
```rust
use std::{path::Path, process::Output, sync::mpsc as std_mpsc};

use winit::event_loop::EventLoopProxy;

use crate::{
    app::event_loop::AppEvent,
    async_runtime::message::{
        WorkerEvent, WorkerEventKind, WorkerFailure, WorkerFailureKind, WorkerMessage,
        WorkerRequest, WorkerRequestPayload, WorkerResult, WorkerResultPayload,
    },
    codegraph::{
        cli_json::{parse_callees, parse_callers, parse_impact},
        model::build_model,
    },
};

use super::emit::{emit_message, emit_message_and_wake};

const MAX_PER_SIDE: &str = "20";
const IMPACT_DEPTH: &str = "2";

pub(super) async fn run_codegraph_request(
    request: WorkerRequest,
    worker_tx: std_mpsc::Sender<WorkerMessage>,
    event_proxy: EventLoopProxy<AppEvent>,
) {
    emit_message(
        &worker_tx,
        WorkerMessage::Event(WorkerEvent {
            request_id: request.request_id,
            revision_id: request.revision_id,
            topic: request.topic,
            kind: WorkerEventKind::Started,
        }),
    );

    let payload = match execute(&request).await {
        Ok(payload) => payload,
        Err((not_installed, message)) => {
            WorkerResultPayload::CodeGraphFailed { not_installed, message }
        }
    };

    emit_message_and_wake(
        &worker_tx,
        &event_proxy,
        WorkerMessage::Result(WorkerResult {
            request_id: request.request_id,
            revision_id: request.revision_id,
            topic: request.topic,
            payload,
        }),
    );
    emit_message_and_wake(
        &worker_tx,
        &event_proxy,
        WorkerMessage::Event(WorkerEvent {
            request_id: request.request_id,
            revision_id: request.revision_id,
            topic: request.topic,
            kind: WorkerEventKind::Completed,
        }),
    );
}

async fn execute(request: &WorkerRequest) -> Result<WorkerResultPayload, (bool, String)> {
    let WorkerRequestPayload::CodeGraphQuery { symbol, focal_file, focal_line, workspace_root } =
        &request.payload
    else {
        return Err((false, "codegraph runner received wrong payload".to_string()));
    };

    // Incremental refresh; ignore failures (stale index still usable).
    let _ = run_cg(&["sync"], workspace_root).await;

    let callers_out = run_cg(&["callers", symbol, "--json", "--limit", MAX_PER_SIDE], workspace_root).await?;
    let callees_out = run_cg(&["callees", symbol, "--json", "--limit", MAX_PER_SIDE], workspace_root).await?;
    let impact_out  = run_cg(&["impact",  symbol, "--json", "--depth", IMPACT_DEPTH], workspace_root).await?;

    let callers = parse_callers(&callers_out).map_err(|e| (false, e))?;
    let callees = parse_callees(&callees_out).map_err(|e| (false, e))?;
    let impact  = parse_impact(&impact_out).map_err(|e| (false, e))?;

    let model = build_model(symbol, focal_file, *focal_line, &callers, &callees, &impact);
    Ok(WorkerResultPayload::CodeGraphReady { model })
}

/// Run `codegraph <args>` in the workspace, returning stdout.
/// Err.0 == true means the binary is not installed.
async fn run_cg(args: &[&str], cwd: &Path) -> Result<String, (bool, String)> {
    use tokio::process::Command;
    let mut command = Command::new("codegraph");
    command.kill_on_drop(true);
    let output: Output = command
        .args(args)
        .current_dir(cwd)
        .output()
        .await
        .map_err(|err| {
            let not_installed = err.kind() == std::io::ErrorKind::NotFound;
            (not_installed, format!("codegraph {}: {err}", args.first().copied().unwrap_or("")))
        })?;

    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        Err((false, format!("codegraph {} failed: {stderr}", args.first().copied().unwrap_or(""))))
    }
}
```

In `src/async_runtime/scheduler/mod.rs` add `pub(super) mod codegraph;` next to `pub(super) mod fzf;` (match the existing visibility used for sibling modules).

- [ ] **Step 2: Verify it compiles**

Run: `cargo check 2>&1 | tail -20`
Expected: compiles. If `emit` module path differs, copy the exact `use super::emit::...` line from `fzf.rs`.

- [ ] **Step 3: Commit**
```bash
git add src/async_runtime/scheduler/codegraph.rs src/async_runtime/scheduler/mod.rs
git commit -m "feat(codegraph): scheduler runner spawns codegraph CLI"
```

---

### Task 7: Route the request in dispatch

**Files:**
- Modify: `src/async_runtime/scheduler/dispatch.rs` (after the `FzfSearch` block ~line 206-217)
- Modify imports at top of `dispatch.rs` to bring `run_codegraph_request` into scope (mirror how `run_fzf_request` is imported).

- [ ] **Step 1: Add the routing block**

After the `FzfSearch` `if matches!` block, add:
```rust
        if matches!(request.payload, WorkerRequestPayload::CodeGraphQuery { .. }) {
            let worker_tx = result_tx.clone();
            let event_proxy = event_proxy.clone();
            tokio::spawn(async move {
                run_codegraph_request(request, worker_tx, event_proxy).await;
            });
            continue;
        }
```

Add `run_codegraph_request` to the `use super::{... run_fzf_request ...}` import (see `dispatch.rs:27` style import group).

- [ ] **Step 2: Verify it compiles** — Run: `cargo check 2>&1 | tail -20` — Expected: compiles.

- [ ] **Step 3: Commit**
```bash
git add src/async_runtime/scheduler/dispatch.rs
git commit -m "feat(codegraph): route CodeGraphQuery to runner"
```

---

## Phase 3 — Command + keybinding + focus context

### Task 8: Command id + registration + keybinding

**Files:**
- Modify: `src/core/command_ids.rs` (add const id, mirror an existing `LSP_REFERENCES`-style entry)
- Modify: `src/core/commands.rs` (register `codegraph.open_graph_hud`)
- Modify: `config/keymaps/default.toml` (after the `g r` block, ~line 958)

- [ ] **Step 1: Add command id**

In `src/core/command_ids.rs`, add next to the LSP ids:
```rust
pub const CODEGRAPH_OPEN_GRAPH_HUD: &str = "codegraph.open_graph_hud";
```

- [ ] **Step 2: Register the command**

In `src/core/commands.rs`, register it in the same table/registry the other commands use (copy the shape of the `lsp.references` registration: id = `command_ids::CODEGRAPH_OPEN_GRAPH_HUD`, title `"Code Graph: Open Graph HUD"`).

- [ ] **Step 3: Add the keybinding**

In `config/keymaps/default.toml`, after the `g r` binding block (line 955-958):
```toml
[[bindings]]
mode = "normal"
key = "g p"
command = "codegraph.open_graph_hud"
```

- [ ] **Step 4: Verify**

Run: `cargo test commands 2>&1 | tail -20` (runs any command-registry tests) and `cargo check`.
Expected: compiles; if there is a keymap-resolution test, it passes. Manually confirm: `grep -n "g p" config/keymaps/default.toml`.

- [ ] **Step 5: Commit**
```bash
git add src/core/command_ids.rs src/core/commands.rs config/keymaps/default.toml
git commit -m "feat(codegraph): gp command + keybinding"
```

---

### Task 9: HUD state + focus context

**Files:**
- Create: `src/app/app_state/code_graph_hud.rs`
- Modify: `src/app/app_state/mod.rs` (declare `pub mod code_graph_hud;`; add field to `AppState`; add `InputFocusContext::CodeGraph`)

- [ ] **Step 1: Write the state + test**

In `src/app/app_state/code_graph_hud.rs`:
```rust
use crate::codegraph::model::CodeGraphModel;
use crate::codegraph::navigation::{navigate, Focus, NavKey};

#[derive(Debug, Clone, PartialEq)]
pub enum CodeGraphHudStatus {
    Loading,
    Ready(CodeGraphModel),
    Empty,
    NotInstalled,
    Error(String),
}

#[derive(Debug, Clone, PartialEq)]
pub struct CodeGraphHudState {
    pub open: bool,
    pub status: CodeGraphHudStatus,
    pub focus: Focus,
}

impl Default for CodeGraphHudState {
    fn default() -> Self {
        Self { open: false, status: CodeGraphHudStatus::Loading, focus: Focus::Center }
    }
}

impl CodeGraphHudState {
    pub fn open_loading(&mut self) {
        self.open = true;
        self.status = CodeGraphHudStatus::Loading;
        self.focus = Focus::Center;
    }

    pub fn set_model(&mut self, model: CodeGraphModel) {
        self.focus = Focus::Center;
        self.status = if model.is_empty() {
            CodeGraphHudStatus::Empty
        } else {
            CodeGraphHudStatus::Ready(model)
        };
    }

    pub fn close(&mut self) { self.open = false; }

    /// Apply a navigation key; returns true if focus changed.
    pub fn nav(&mut self, key: NavKey) -> bool {
        let CodeGraphHudStatus::Ready(model) = &self.status else { return false; };
        let next = navigate(self.focus, key, model.callers.len(), model.callees.len());
        let changed = next != self.focus;
        self.focus = next;
        changed
    }

    /// The node currently focused, if the graph is ready.
    pub fn focused_node(&self) -> Option<&crate::codegraph::model::GraphNode> {
        let CodeGraphHudStatus::Ready(model) = &self.status else { return None; };
        match self.focus {
            Focus::Center => Some(&model.focal),
            Focus::Caller(i) => model.callers.get(i),
            Focus::Callee(i) => model.callees.get(i),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegraph::model::{CodeGraphModel, GraphNode, NodeRole, RiskLevel};

    fn node(name: &str, role: NodeRole) -> GraphNode {
        GraphNode { name: name.into(), kind: "fn".into(), file_path: "f.rs".into(),
            line: 1, role, risk: RiskLevel::Safe }
    }

    fn ready_model() -> CodeGraphModel {
        CodeGraphModel {
            focal: node("validate", NodeRole::Center),
            callers: vec![node("login", NodeRole::Caller)],
            callees: vec![node("find", NodeRole::Callee)],
        }
    }

    #[test]
    fn empty_model_yields_empty_status() {
        let mut s = CodeGraphHudState::default();
        s.set_model(CodeGraphModel { focal: node("x", NodeRole::Center),
            callers: vec![], callees: vec![] });
        assert_eq!(s.status, CodeGraphHudStatus::Empty);
    }

    #[test]
    fn nav_left_from_center_focuses_first_caller() {
        let mut s = CodeGraphHudState::default();
        s.set_model(ready_model());
        assert!(s.nav(NavKey::Left));
        assert_eq!(s.focus, Focus::Caller(0));
        assert_eq!(s.focused_node().unwrap().name, "login");
    }
}
```

In `src/app/app_state/mod.rs`:
- add `pub mod code_graph_hud;` with the other `pub mod` lines,
- add to the `AppState` struct: `pub code_graph_hud: code_graph_hud::CodeGraphHudState,` and initialize it `..Default::default()` style in the constructor,
- add `CodeGraph` to the `InputFocusContext` enum (find `enum InputFocusContext` — it already has `AiChat`, `TestRunner`, `Outline`, etc.).

- [ ] **Step 2: Run test to verify it fails** — Run: `cargo test code_graph_hud 2>&1 | tail -20` — Expected: builds then PASS.

- [ ] **Step 3: Implement** — inlined above + the `mod.rs` wiring.

- [ ] **Step 4: Run test to verify it passes** — Run: `cargo test code_graph_hud 2>&1 | tail -20` — Expected: `2 passed`.

- [ ] **Step 5: Commit**
```bash
git add src/app/app_state/code_graph_hud.rs src/app/app_state/mod.rs
git commit -m "feat(codegraph): HUD state + CodeGraph focus context"
```

---

### Task 10: Input routing for the overlay

**Files:**
- Modify: `src/app/input/handler.rs` (focus gating, mirror the `Outline`/`TestRunner` blocks ~line 421-426)
- Modify: `src/app/input_map/focus.rs` (per-key actions when CodeGraph is focused, mirror the `extensions:` block ~line 17-58)

When `app_state.input_focus() == InputFocusContext::CodeGraph`:
- `h`→`nav(Left)`, `l`→`nav(Right)`, `k`→`nav(Up)`, `j`→`nav(Down)`
- `Enter`→jump to `focused_node()` (`file_path:line`) then `close()`
- `Esc`→`close()`

- [ ] **Step 1: Add focus mapping**

In `src/app/input_map/focus.rs`, add a branch that, when the focus context is CodeGraph, maps keys to intents. Follow the exact return-shape used by the surrounding branches (they return resolved actions with a `reason`). Map:
```rust
// pseudocode shape — match the real ResolvedAction type in this file
"h" / Left  -> CodeGraphNav(NavKey::Left)
"l" / Right -> CodeGraphNav(NavKey::Right)
"k" / Up    -> CodeGraphNav(NavKey::Up)
"j" / Down  -> CodeGraphNav(NavKey::Down)
"Enter"     -> CodeGraphJump
"Esc"       -> CodeGraphClose
```
Add the corresponding action variants to whatever action enum `focus.rs` returns (search the file for `enum` it constructs).

- [ ] **Step 2: Gate other handlers**

In `src/app/input/handler.rs`, where `InputFocusContext::Outline` / `TestRunner` short-circuit (~line 421-426), add a parallel guard for `CodeGraph` so normal-mode motions don't leak through while the HUD owns input. Keep leader chords available only if needed (the HUD does not need them — exclude).

- [ ] **Step 3: Handle the actions**

In the event-loop command handler that consumes resolved actions (the same place `extensions_select_next` etc. are dispatched), call:
```rust
CodeGraphNav(key) => { app_state.code_graph_hud.nav(key); redraw(); }
CodeGraphClose    => { app_state.code_graph_hud.close(); app_state.restore_focus(); redraw(); }
CodeGraphJump     => {
    if let Some(n) = app_state.code_graph_hud.focused_node() {
        let (file, line) = (n.file_path.clone(), n.line);
        app_state.code_graph_hud.close();
        app_state.restore_focus();
        // reuse the existing "open file at line" path used by fzf/gr jumps
        open_file_at_line(&file, line);
    }
}
```
Use the same open-at-location helper the references/fzf jump uses (search for where `FzfResultItem.line` is consumed to navigate).

- [ ] **Step 4: Verify** — Run: `cargo check 2>&1 | tail -20` and `cargo test input 2>&1 | tail -20` — Expected: compiles; existing input tests pass.

- [ ] **Step 5: Commit**
```bash
git add src/app/input/handler.rs src/app/input_map/focus.rs src/app/event_loop/
git commit -m "feat(codegraph): hjkl/Enter/Esc input routing for HUD"
```

---

## Phase 4 — Submit query + receive result

### Task 11: Resolve focal symbol + submit on command

**Files:**
- Modify: `src/app/event_loop/` (the command handler that maps command ids to actions — where `lsp.references` is handled)

- [ ] **Step 1: Implement the command handler**

When `CODEGRAPH_OPEN_GRAPH_HUD` fires:
```rust
// 1. Resolve the symbol under the caret via tree-sitter (enclosing fn/method name).
//    Reuse the outline/symbol extraction the editor already has; if none, toast and return.
let Some((symbol, line)) = resolve_symbol_at_caret(app_state) else {
    app_state.set_toast(Some("Code Graph: no symbol under cursor".into()));
    return;
};
let Some(file) = app_state.active_file().map(|p| p.display().to_string()) else { return; };
let workspace_root = app_state.workspace_root_path(); // existing accessor

// 2. Open HUD in loading state, take focus.
app_state.code_graph_hud.open_loading();
app_state.set_input_focus(InputFocusContext::CodeGraph);

// 3. Submit the worker request (mirror how fzf search is submitted).
scheduler.submit(WorkerRequestPayload::CodeGraphQuery {
    symbol, focal_file: file, focal_line: line, workspace_root,
});
redraw();
```

For `resolve_symbol_at_caret`: reuse the existing tree-sitter outline. If a ready helper isn't obvious, add a small function in the editor's tree-sitter module that walks up from the caret byte to the nearest `function_item`/`method`/`function_definition` node and returns its `name` child text + start line. Add a unit test for it against a small Rust snippet.

- [ ] **Step 2: Verify** — Run: `cargo check 2>&1 | tail -20` — Expected: compiles.

- [ ] **Step 3: Commit**
```bash
git add src/app/event_loop/
git commit -m "feat(codegraph): resolve focal symbol and submit query on gp"
```

---

### Task 12: Handle worker result → update HUD

**Files:**
- Modify: `src/app/event_loop/` `on_worker_result` (where `WorkerResultPayload` variants are matched — e.g. `FzfResults`, `LspReferencesResult`)

- [ ] **Step 1: Match the new payloads**

```rust
WorkerResultPayload::CodeGraphReady { model } => {
    if app_state.code_graph_hud.open {
        app_state.code_graph_hud.set_model(model);
        self.request_redraw();
    }
}
WorkerResultPayload::CodeGraphFailed { not_installed, message } => {
    if app_state.code_graph_hud.open {
        app_state.code_graph_hud.status = if not_installed {
            CodeGraphHudStatus::NotInstalled
        } else {
            CodeGraphHudStatus::Error(message)
        };
        self.request_redraw();
    }
}
```

- [ ] **Step 2: Verify** — Run: `cargo check 2>&1 | tail -20` — Expected: compiles.

- [ ] **Step 3: Commit**
```bash
git add src/app/event_loop/
git commit -m "feat(codegraph): apply worker result to HUD state"
```

---

## Phase 5 — Rendering

### Task 13: OverlayKind + render entry

**Files:**
- Modify: `src/workbench/overlay_manager.rs` (add `OverlayKind::CodeGraphHud` ~line 13-20)
- Modify: `src/render/renderer/editor/overlays.rs` (new draw fn called from the overlay render path)

- [ ] **Step 1: Add the OverlayKind variant**

In `src/workbench/overlay_manager.rs`, add `CodeGraphHud` to `enum OverlayKind`.

- [ ] **Step 2: Draw the HUD**

In `src/render/renderer/editor/overlays.rs`, add `fn draw_code_graph_hud(&mut self, app_state, center_bounds)` invoked from the overlay render path when `app_state.code_graph_hud.open`. Use existing primitives:
- Backdrop: `RegionDrawInstance::new(center_bounds, overlay_dim_rgba)` (use `self.theme.ui.overlay_bg`).
- HUD panel rect: centered, `.with_radius(self.panel_corner_radius)`, `self.theme.ui.panel_bg`.
- Top bar / footer: rects + `layout_panel_text(...)` (copy the breadcrumb text pattern lines 118-127).
- Pills: for each `GraphLayout` rect, `RegionDrawInstance::new([x,y,w,h], fill).with_radius(...)`; fill/stroke color from `risk_color()`:
  - Focal→`self.theme.ui.cyan`, Safe→`self.theme.git.added_sidebar` (green), Medium→`self.theme.ui.amber`, High→`self.theme.ui.error`.
- Focus ring: a slightly larger outline quad behind the focused pill (static).
- Risk dot: tiny `.with_radius(big)` quad at pill left.
- Edges: for each visible side node, a thin rotated quad from center edge to pill edge (see Task 14), plus `▸` glyph as arrowhead via `layout_panel_text`.
- Node label + `file:line`: `layout_panel_text` inside each pill.
- Status overlays: when status is `Loading`/`Empty`/`NotInstalled`/`Error`, draw a centered message instead of the graph. `NotInstalled` text: `"codegraph not installed — press <leader>m e to install"`.

Call this fn from the same place `update_editor_overlays` / overlay rendering composes editor overlays.

- [ ] **Step 3: Verify** — Run: `cargo check 2>&1 | tail -20` — Expected: compiles.

- [ ] **Step 4: Commit**
```bash
git add src/workbench/overlay_manager.rs src/render/renderer/editor/overlays.rs
git commit -m "feat(codegraph): render HUD panel, pills, labels, states"
```

---

### Task 14: Edge segments + arrowheads

**Files:**
- Create: `src/codegraph/edges.rs` (pure geometry, unit-tested)
- Modify: `src/codegraph/mod.rs`; `src/render/renderer/editor/overlays.rs` (consume)

Edges are drawn as a single thin rotated quad approximating the straight segment between the center pill edge and each side pill edge. The renderer's quad instances are axis-aligned, so we approximate each edge with a short horizontal connector + a vertical connector (an "elbow"), avoiding rotation entirely.

- [ ] **Step 1: Write the failing test**

In `src/codegraph/edges.rs`:
```rust
use crate::codegraph::layout::PillRect;

/// An axis-aligned segment [x, y, w, h] for the quad renderer.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EdgeQuad { pub x: f32, pub y: f32, pub w: f32, pub h: f32 }

const THICK: f32 = 1.5;

/// Elbow connector from `center` right/left edge to a side `pill`.
/// `to_right=true` connects center→callee (rightward); false connects caller→center.
pub fn elbow(center: PillRect, pill: PillRect, to_right: bool) -> Vec<EdgeQuad> {
    let cy = center.y + center.h * 0.5;
    let py = pill.y + pill.h * 0.5;
    let (cx, px) = if to_right {
        (center.x + center.w, pill.x)             // center right edge -> callee left edge
    } else {
        (pill.x + pill.w, center.x)               // caller right edge -> center left edge
    };
    let mid_x = (cx + px) * 0.5;
    let x0 = cx.min(mid_x);
    let x1 = cx.max(mid_x);
    let y0 = cy.min(py);
    let y1 = cy.max(py);
    vec![
        // horizontal from center to mid
        EdgeQuad { x: x0, y: cy - THICK * 0.5, w: (x1 - x0).max(THICK), h: THICK },
        // vertical at mid spanning cy..py
        EdgeQuad { x: mid_x - THICK * 0.5, y: y0, w: THICK, h: (y1 - y0).max(THICK) },
        // horizontal from mid to pill
        EdgeQuad { x: mid_x.min(px), y: py - THICK * 0.5, w: (mid_x.max(px) - mid_x.min(px)).max(THICK), h: THICK },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegraph::layout::PillRect;

    #[test]
    fn elbow_returns_three_segments() {
        let c = PillRect { x: 300.0, y: 180.0, w: 200.0, h: 60.0 };
        let p = PillRect { x: 600.0, y: 60.0, w: 160.0, h: 44.0 };
        let segs = elbow(c, p, true);
        assert_eq!(segs.len(), 3);
        // first segment starts at center right edge
        assert!((segs[0].x - 500.0).abs() < 0.5);
    }
}
```

Add to `src/codegraph/mod.rs`: `pub mod edges;`

- [ ] **Step 2: Run test to verify it fails** — Run: `cargo test codegraph::edges 2>&1 | tail -20` — Expected: builds then PASS.

- [ ] **Step 3: Implement** — inlined above.

- [ ] **Step 4: Consume in renderer** — In `draw_code_graph_hud`, for each visible caller/callee call `elbow(center, pill, to_right)` and push each `EdgeQuad` as a `RegionDrawInstance` tinted by the node's risk color (dimmed unless the node is focused). Draw a `▸` glyph at the arrival edge.

- [ ] **Step 5: Run test + check** — Run: `cargo test codegraph::edges 2>&1 | tail -20 && cargo check 2>&1 | tail -5` — Expected: PASS + compiles.

- [ ] **Step 6: Commit**
```bash
git add src/codegraph/edges.rs src/codegraph/mod.rs src/render/renderer/editor/overlays.rs
git commit -m "feat(codegraph): elbow edge connectors with arrowheads"
```

---

## Phase 6 — Extension entry + manual verification

### Task 15: Register codegraph in the Extensions manager

**Files:**
- Modify: `src/app/app_state/mod.rs` (`default_extension_items()` ~line 783-805)

- [ ] **Step 1: Add the extension item**

In `default_extension_items()`, add (match the existing `ExtensionItem` builder/struct-literal style used by neighbors):
```rust
ExtensionItem {
    name: "CodeGraph".to_string(),
    subtitle: "Code intelligence — callers/callees/impact graph".to_string(),
    binary: "codegraph".to_string(),
    category: ExtensionCategory::/* the existing "tools"/"intelligence" variant */,
    tag: "graph".to_string(),
    macos_install: "npm install -g codegraph".to_string(),
    linux_install: "npm install -g codegraph".to_string(),
    macos_uninstall: "npm uninstall -g codegraph".to_string(),
    linux_uninstall: "npm uninstall -g codegraph".to_string(),
    extensions: Vec::new(),
    installed: false,
},
```
Pick the closest existing `ExtensionCategory` variant (inspect the enum; if there's no intelligence category, use the same one other CLI tools use).

- [ ] **Step 2: Verify** — Run: `cargo test extensions 2>&1 | tail -20 && cargo check` — Expected: compiles; the `which codegraph` sweep will mark it installed at runtime.

- [ ] **Step 3: Commit**
```bash
git add src/app/app_state/mod.rs
git commit -m "feat(codegraph): register codegraph in extensions manager"
```

---

### Task 16: Full build, test sweep, manual smoke test

- [ ] **Step 1: Full test suite**

Run: `cargo test 2>&1 | tail -30`
Expected: all green, including the new `codegraph::*` and `code_graph_hud` tests.

- [ ] **Step 2: Build the app**

Run: `cargo build 2>&1 | tail -15`
Expected: builds clean.

- [ ] **Step 3: Manual smoke test (use the `run` skill or launch the editor)**

- Open a Rust file, put the caret inside a function that has known callers (e.g. `build_overlays` in `src/workbench/overlay_manager.rs`).
- Press `gp`. Expect: HUD opens "indexing…" briefly, then center pill + caller/callee columns with risk colors.
- Press `h`/`l`/`j`/`k`: focus ring moves per the spec. `Enter` jumps to the node's file:line and closes. `Esc` closes.
- Temporarily rename the `codegraph` binary on PATH and press `gp`: expect the "not installed → `<leader>m e`" state.
- Open Extensions manager (`<leader>m e`): CodeGraph appears, marked installed.

- [ ] **Step 4: Commit any fixes from smoke testing**
```bash
git add -A
git commit -m "fix(codegraph): smoke-test adjustments"
```

---

## Notes for the implementer

- **Mirror, don't invent.** For every "modify" task, open the referenced sibling (`fzf.rs`, the `g r` binding, the `Outline` focus guard, the breadcrumb text drawing) and copy its exact shape — types and helper names in this codebase may differ slightly from the snippets here.
- **Blast radius is an estimate.** codegraph under-reports trait/dynamic-dispatch and macro calls. The HUD top bar already states this; do not present risk as authoritative.
- **Unsaved buffers.** codegraph indexes on-disk files; the focal symbol comes from the live buffer but edges reflect last save. Acceptable for v1.
- **No auto-commit by the human's rule:** these commit steps are for the implementing engineer/agent executing the plan; if you (assistant) are running this interactively, do not commit unless the user explicitly says so.
