# Workbench Motion — Panel Slide Animation — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Hyprland-style smooth (90+ FPS) slide animation when docks toggle, Zen/maximize changes, and modal overlays open/close.

**Architecture:** A pure `workbench/motion.rs` module interpolates whole `WorkbenchLayout`s (region bounds, keyed by `RegionId`) over an eased duration. `redraw()` consumes an *effective layout* — the sampled transition while animating, else the plain `compute()` result — so all existing viewport-bounded content layout follows the animated bounds with no new content code. A `tick_*` in `about_to_wait` drives frames at the existing ~8 ms cadence. Overlays animate separately via opacity+scale.

**Tech Stack:** Rust, winit event loop, wgpu renderer (quad/glyph batching), existing `tick_*`/`ControlFlow::WaitUntil` animation loop and `*_needs_layout` dirty-flag system.

## Global Constraints

- **90+ FPS / frame < 8 ms** (`FRAME_TIME_WARN_THRESHOLD`). Janky = drop it.
- **No commits by the agent** — per project rule, committing is the human's job. Each task ends with a **Checkpoint** (tests green, working tree left for the human). Do NOT run `git commit`.
- **No `.unwrap()`/`.expect()`** in render/async/tree-sitter paths; use graceful handling.
- **No blocking the UI thread.** Animation math is pure and cheap; no IO.
- Follow the Golden Data Flow for any input→action wiring.
- Reduce-motion kill-switch (`[animation] enabled = false`) must snap instantly.
- Pure module = no GPU/IO; `sample`/`is_done` take `now: Instant` as a parameter so tests are deterministic.

---

### Task 1: Pure motion module (easing + layout interpolation + overlay motion)

**Files:**
- Create: `src/workbench/motion.rs`
- Modify: `src/workbench/mod.rs` (add `pub mod motion;`)
- Test: inline `#[cfg(test)]` in `src/workbench/motion.rs` (matches repo style)

**Interfaces:**
- Consumes: `crate::workbench::layout_engine::WorkbenchLayout`, `crate::workbench::region_model::{RegionBounds, RegionId, RegionModel, RegionNode}`.
- Produces:
  - `pub enum EaseCurve { Linear, EaseOutCubic }` with `pub fn apply(self, t: f32) -> f32` (input/output clamped to `[0,1]`).
  - `pub fn lerp_bounds(a: RegionBounds, b: RegionBounds, t: f32) -> RegionBounds`
  - `pub fn lerp_layout(from: &WorkbenchLayout, to: &WorkbenchLayout, t: f32) -> WorkbenchLayout` — result has `to`'s tree shape & `visible` flags & handles; each node's bounds = `lerp_bounds(from_bounds_for_same_id, to_bounds, t)`, falling back to `to`'s bounds when an id is absent in `from`.
  - `pub struct LayoutTransition { from: WorkbenchLayout, to: WorkbenchLayout, started_at: Instant, duration: Duration, curve: EaseCurve }` with:
    - `pub fn new(from, to, started_at, duration, curve) -> Self`
    - `pub fn progress(&self, now: Instant) -> f32` (clamped `[0,1]`)
    - `pub fn sample(&self, now: Instant) -> WorkbenchLayout`
    - `pub fn is_done(&self, now: Instant) -> bool`
    - `pub fn target(&self) -> &WorkbenchLayout`
  - `pub struct OverlayMotion { phase: OverlayPhase, started_at: Instant, duration: Duration, curve: EaseCurve }`, `pub enum OverlayPhase { Enter, Leave }`, `pub struct OverlaySample { pub alpha: f32, pub scale: f32 }` with `pub fn sample(&self, now) -> OverlaySample` and `pub fn is_done(&self, now) -> bool`.

- [ ] **Step 1: Write failing tests**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::workbench::layout_engine::{WorkbenchLayout};
    use crate::workbench::region_model::{RegionBounds, RegionId, RegionModel, RegionNode};
    use std::time::{Duration, Instant};

    fn layout(center_w: f32) -> WorkbenchLayout {
        let root = RegionNode::new(RegionId::Root, RegionBounds::new(0.0, 0.0, 100.0, 100.0), true)
            .with_children(vec![
                RegionNode::new(RegionId::LeftSidebar, RegionBounds::new(0.0, 0.0, 100.0 - center_w, 100.0), center_w < 100.0),
                RegionNode::new(RegionId::Center, RegionBounds::new(100.0 - center_w, 0.0, center_w, 100.0), true),
            ]);
        WorkbenchLayout { model: RegionModel::new(RegionBounds::new(0.0,0.0,100.0,100.0)).with_children(root.children), handles: vec![] }
    }

    #[test]
    fn ease_out_cubic_clamps_and_endpoints() {
        assert_eq!(EaseCurve::EaseOutCubic.apply(-1.0), 0.0);
        assert_eq!(EaseCurve::EaseOutCubic.apply(2.0), 1.0);
        assert!((EaseCurve::EaseOutCubic.apply(0.0) - 0.0).abs() < 1e-6);
        assert!((EaseCurve::EaseOutCubic.apply(1.0) - 1.0).abs() < 1e-6);
        // ease-out is ahead of linear in the middle
        assert!(EaseCurve::EaseOutCubic.apply(0.5) > 0.5);
    }

    #[test]
    fn lerp_bounds_midpoint() {
        let a = RegionBounds::new(0.0, 0.0, 0.0, 10.0);
        let b = RegionBounds::new(10.0, 0.0, 20.0, 10.0);
        let m = lerp_bounds(a, b, 0.5);
        assert_eq!(m.x, 5.0);
        assert_eq!(m.width, 10.0);
    }

    #[test]
    fn transition_sample_endpoints_and_done() {
        let from = layout(100.0); // no sidebar
        let to = layout(60.0);    // sidebar 40 wide
        let t0 = Instant::now();
        let tr = LayoutTransition::new(from.clone(), to.clone(), t0, Duration::from_millis(100), EaseCurve::Linear);
        assert_eq!(tr.sample(t0).model.find(RegionId::Center).unwrap().width, 100.0);
        assert!(!tr.is_done(t0));
        let end = t0 + Duration::from_millis(100);
        assert_eq!(tr.sample(end).model.find(RegionId::Center).unwrap().width, 60.0);
        assert!(tr.is_done(end));
    }

    #[test]
    fn lerp_layout_uses_target_shape_when_id_missing() {
        // from has only Center; to has Center + LeftSidebar -> sidebar snaps to target.
        let mut from = layout(100.0);
        from.model.root.children.retain(|c| c.id == RegionId::Center);
        let to = layout(60.0);
        let mid = lerp_layout(&from, &to, 0.5);
        // sidebar absent in `from` -> takes target bounds
        assert_eq!(mid.model.find(RegionId::LeftSidebar).unwrap().width, 40.0);
        // center present in both -> interpolated
        assert_eq!(mid.model.find(RegionId::Center).unwrap().width, 80.0);
    }

    #[test]
    fn overlay_enter_ramps_alpha() {
        let t0 = Instant::now();
        let m = OverlayMotion { phase: OverlayPhase::Enter, started_at: t0, duration: Duration::from_millis(100), curve: EaseCurve::Linear };
        assert!(m.sample(t0).alpha < 0.01);
        assert!((m.sample(t0 + Duration::from_millis(100)).alpha - 1.0).abs() < 1e-6);
        assert!(m.sample(t0).scale > 0.9 && m.sample(t0).scale <= 1.0);
    }
}
```

- [ ] **Step 2: Run tests, verify they fail to compile (module missing)**

Run: `cargo test --lib workbench::motion`
Expected: FAIL (unresolved module / items).

- [ ] **Step 3: Implement `src/workbench/motion.rs`**

```rust
//! Pure animation primitives for workbench transitions (panel slide, zen,
//! overlay enter/leave). UI-free and fully unit-tested. `sample`/`is_done` take
//! `now: Instant` so tests are deterministic.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use crate::workbench::layout_engine::WorkbenchLayout;
use crate::workbench::region_model::{RegionBounds, RegionId, RegionModel, RegionNode};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EaseCurve {
    Linear,
    EaseOutCubic,
}

impl EaseCurve {
    pub fn apply(self, t: f32) -> f32 {
        let t = t.clamp(0.0, 1.0);
        match self {
            EaseCurve::Linear => t,
            // 1 - (1 - t)^3
            EaseCurve::EaseOutCubic => {
                let inv = 1.0 - t;
                1.0 - inv * inv * inv
            }
        }
    }

    pub fn from_str_or_default(s: &str) -> Self {
        match s.trim().to_ascii_lowercase().as_str() {
            "linear" => EaseCurve::Linear,
            _ => EaseCurve::EaseOutCubic,
        }
    }
}

#[inline]
fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

pub fn lerp_bounds(a: RegionBounds, b: RegionBounds, t: f32) -> RegionBounds {
    RegionBounds::new(
        lerp(a.x, b.x, t),
        lerp(a.y, b.y, t),
        lerp(a.width, b.width, t),
        lerp(a.height, b.height, t),
    )
}

fn collect_bounds(model: &RegionModel, out: &mut HashMap<RegionId, RegionBounds>) {
    fn walk(node: &RegionNode, out: &mut HashMap<RegionId, RegionBounds>) {
        out.insert(node.id, node.bounds);
        for child in &node.children {
            walk(child, out);
        }
    }
    walk(&model.root, out);
}

fn map_node(node: &RegionNode, from: &HashMap<RegionId, RegionBounds>, t: f32) -> RegionNode {
    let target = node.bounds;
    let bounds = match from.get(&node.id) {
        Some(&start) => lerp_bounds(start, target, t),
        None => target,
    };
    let mut out = RegionNode::new(node.id, bounds, node.visible);
    out.children = node
        .children
        .iter()
        .map(|c| map_node(c, from, t))
        .collect();
    out
}

/// Interpolate `from` toward `to` at progress `t` (already eased). Result keeps
/// `to`'s tree shape, `visible` flags, and handles; bounds are lerped per
/// `RegionId` (ids absent in `from` snap to their target bounds).
pub fn lerp_layout(from: &WorkbenchLayout, to: &WorkbenchLayout, t: f32) -> WorkbenchLayout {
    let mut from_bounds = HashMap::new();
    collect_bounds(&from.model, &mut from_bounds);
    let root = map_node(&to.model.root, &from_bounds, t);
    WorkbenchLayout {
        model: RegionModel { root },
        handles: to.handles.clone(),
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct LayoutTransition {
    from: WorkbenchLayout,
    to: WorkbenchLayout,
    started_at: Instant,
    duration: Duration,
    curve: EaseCurve,
}

impl LayoutTransition {
    pub fn new(
        from: WorkbenchLayout,
        to: WorkbenchLayout,
        started_at: Instant,
        duration: Duration,
        curve: EaseCurve,
    ) -> Self {
        Self { from, to, started_at, duration, curve }
    }

    pub fn progress(&self, now: Instant) -> f32 {
        if self.duration.is_zero() {
            return 1.0;
        }
        let elapsed = now.saturating_duration_since(self.started_at).as_secs_f32();
        (elapsed / self.duration.as_secs_f32()).clamp(0.0, 1.0)
    }

    pub fn sample(&self, now: Instant) -> WorkbenchLayout {
        let t = self.curve.apply(self.progress(now));
        lerp_layout(&self.from, &self.to, t)
    }

    pub fn is_done(&self, now: Instant) -> bool {
        self.progress(now) >= 1.0
    }

    pub fn target(&self) -> &WorkbenchLayout {
        &self.to
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OverlayPhase {
    Enter,
    Leave,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct OverlaySample {
    pub alpha: f32,
    pub scale: f32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct OverlayMotion {
    pub phase: OverlayPhase,
    pub started_at: Instant,
    pub duration: Duration,
    pub curve: EaseCurve,
}

impl OverlayMotion {
    fn progress(&self, now: Instant) -> f32 {
        if self.duration.is_zero() {
            return 1.0;
        }
        let elapsed = now.saturating_duration_since(self.started_at).as_secs_f32();
        (elapsed / self.duration.as_secs_f32()).clamp(0.0, 1.0)
    }

    pub fn sample(&self, now: Instant) -> OverlaySample {
        let p = self.curve.apply(self.progress(now));
        let f = match self.phase {
            OverlayPhase::Enter => p,
            OverlayPhase::Leave => 1.0 - p,
        };
        // subtle pop: scale 0.96 -> 1.0
        OverlaySample { alpha: f, scale: 0.96 + 0.04 * f }
    }

    pub fn is_done(&self, now: Instant) -> bool {
        self.progress(now) >= 1.0
    }
}
```

(then the `#[cfg(test)] mod tests` from Step 1)

- [ ] **Step 4: Add module to `src/workbench/mod.rs`**

Add the line (alongside the other `pub mod` entries):

```rust
pub mod motion;
```

- [ ] **Step 5: Run tests, verify pass**

Run: `cargo test --lib workbench::motion`
Expected: PASS (6 tests).

- [ ] **Step 6: Checkpoint** — `cargo build` succeeds, tests green. Leave staged for the human to commit. Do NOT commit.

---

### Task 2: `[animation]` config block

**Files:**
- Modify: `config/ui/default.toml` (add `[animation]` section)
- Modify: `src/config/ui_config.rs` (raw struct, parsed struct, defaults, parse)
- Test: inline `#[cfg(test)]` in `src/config/ui_config.rs`

**Interfaces:**
- Produces on the parsed UI config a field `pub animation: AnimationConfig` where:
  ```rust
  #[derive(Debug, Clone, Copy, PartialEq)]
  pub struct AnimationConfig {
      pub enabled: bool,
      pub dock_duration_ms: u32,
      pub overlay_duration_ms: u32,
      pub curve: crate::workbench::motion::EaseCurve,
  }
  ```
  with `dock_duration()` / `overlay_duration()` helpers returning `Duration`.

- [ ] **Step 1: Read the smooth_scroll parsing pattern** to mirror exactly.

Run: `cargo run -q --example noop 2>/dev/null; true` (no-op) — instead, open `src/config/ui_config.rs` and locate the `[editor]` raw/parse blocks (`smooth_scroll_enabled`, `parse_positive_f32`) and the top-level raw config struct + `EditorConfig`. Mirror that structure for `AnimationConfig`.

- [ ] **Step 2: Write failing tests**

```rust
#[test]
fn animation_config_defaults() {
    let cfg = UiConfig::default(); // or the project's default constructor
    assert!(cfg.animation.enabled);
    assert_eq!(cfg.animation.dock_duration_ms, 150);
    assert_eq!(cfg.animation.overlay_duration_ms, 110);
    assert_eq!(cfg.animation.curve, crate::workbench::motion::EaseCurve::EaseOutCubic);
}

#[test]
fn animation_config_parses_overrides() {
    let toml = r#"
        [animation]
        enabled = false
        dock_duration_ms = 200
        overlay_duration_ms = 90
        curve = "linear"
    "#;
    let cfg = UiConfig::from_toml_str(toml).unwrap(); // use the project's parse entry point
    assert!(!cfg.animation.enabled);
    assert_eq!(cfg.animation.dock_duration_ms, 200);
    assert_eq!(cfg.animation.curve, crate::workbench::motion::EaseCurve::Linear);
}
```

(Adjust `UiConfig::default()` / `from_toml_str` to the actual constructors found in Step 1.)

- [ ] **Step 3: Run, verify fail**

Run: `cargo test --lib config::ui_config::tests`
Expected: FAIL (no `animation` field).

- [ ] **Step 4: Implement**

In `config/ui/default.toml` add:
```toml
[animation]
enabled = true
dock_duration_ms = 150
overlay_duration_ms = 110
curve = "ease_out_cubic"
```

In `src/config/ui_config.rs`:
- Add a `RawAnimationConfig` (all `Option<...>`, `#[serde(default)]`) mirroring the raw editor block, with field `curve: Option<String>`.
- Add `animation: Option<RawAnimationConfig>` to the top-level raw struct (with `#[serde(default)]`).
- Add the `AnimationConfig` struct (above) + a `Default` impl with `enabled:true, dock:150, overlay:110, curve: EaseOutCubic`, plus:
  ```rust
  impl AnimationConfig {
      pub fn dock_duration(&self) -> std::time::Duration {
          std::time::Duration::from_millis(self.dock_duration_ms as u64)
      }
      pub fn overlay_duration(&self) -> std::time::Duration {
          std::time::Duration::from_millis(self.overlay_duration_ms as u64)
      }
  }
  ```
- Add `pub animation: AnimationConfig` to the parsed config struct.
- In the parse/build fn, populate it from the raw block with fallbacks:
  ```rust
  let raw_anim = raw.animation.unwrap_or_default();
  let fb = AnimationConfig::default();
  let animation = AnimationConfig {
      enabled: raw_anim.enabled.unwrap_or(fb.enabled),
      dock_duration_ms: raw_anim.dock_duration_ms.unwrap_or(fb.dock_duration_ms),
      overlay_duration_ms: raw_anim.overlay_duration_ms.unwrap_or(fb.overlay_duration_ms),
      curve: raw_anim
          .curve
          .map(|s| crate::workbench::motion::EaseCurve::from_str_or_default(&s))
          .unwrap_or(fb.curve),
  };
  ```

- [ ] **Step 5: Run, verify pass**

Run: `cargo test --lib config::ui_config`
Expected: PASS.

- [ ] **Step 6: Checkpoint** — `cargo build` green. Leave for human commit.

---

### Task 3: AppShell state + effective layout in `redraw()` (dock push core)

**Files:**
- Modify: `src/app/event_loop/mod.rs` (`AppShell` fields)
- Modify: `src/app/event_loop/application.rs` (`redraw()` ~line 1351; add `current_render_layout()` + `last_committed_layout` upkeep)

**Interfaces:**
- Consumes: `motion::LayoutTransition`, `AnimationConfig` from `self.ui_config.animation`.
- Produces:
  - `AppShell.panel_transition: Option<motion::LayoutTransition>`
  - `AppShell.last_committed_layout: Option<WorkbenchLayout>`
  - `fn current_render_layout(&self, now: Instant) -> WorkbenchLayout` — returns `panel_transition.sample(now)` if a transition exists, else `compute(window_size, panel_state)`.
  - `fn begin_layout_transition(&mut self, now: Instant)` — captures `from` (current sampled/committed layout), computes `to`, installs a `LayoutTransition` (or snaps if `!animation.enabled`).

- [ ] **Step 1: Add fields to `AppShell`**

In `src/app/event_loop/mod.rs`, add to the `AppShell` struct:
```rust
pub(super) panel_transition: Option<crate::workbench::motion::LayoutTransition>,
pub(super) last_committed_layout: Option<crate::workbench::layout_engine::WorkbenchLayout>,
```
Initialize both to `None` wherever `AppShell` is constructed (`setup.rs`).

- [ ] **Step 2: Add `current_render_layout` + `begin_layout_transition`** in `application.rs` (near the other layout helpers):

```rust
pub(super) fn current_render_layout(&self, now: std::time::Instant) -> crate::workbench::layout_engine::WorkbenchLayout {
    if let Some(tr) = &self.panel_transition {
        tr.sample(now)
    } else {
        self.layout_engine.compute(self.window_size, &self.panel_state)
    }
}

/// Capture the current on-screen layout as `from`, compute the new target as
/// `to`, and install a transition (or snap instantly if animations are off).
pub(super) fn begin_layout_transition(&mut self, now: std::time::Instant) {
    let to = self.layout_engine.compute(self.window_size, &self.panel_state);
    let anim = self.ui_config.animation;
    if !anim.enabled {
        self.panel_transition = None;
        self.last_committed_layout = Some(to);
        return;
    }
    let from = self
        .panel_transition
        .as_ref()
        .map(|tr| tr.sample(now))
        .or_else(|| self.last_committed_layout.clone())
        .unwrap_or_else(|| to.clone());
    self.panel_transition = Some(crate::workbench::motion::LayoutTransition::new(
        from,
        to,
        now,
        anim.dock_duration(),
        anim.curve,
    ));
    self.mark_all_panels_dirty();
}
```

If `mark_all_panels_dirty()` does not exist, add a small helper that sets `editor_needs_layout`, `sidebar_needs_layout`, `terminal_needs_layout`, `buffer_terminal_needs_layout`, and the right/bottom dock dirty flags to `true` (these flag names appear in `application.rs` around lines 754-806).

- [ ] **Step 3: Use the effective layout in `redraw()`**

In `src/app/event_loop/application.rs`, replace the `redraw()` layout computation at lines ~1351-1353:
```rust
let layout = self
    .layout_engine
    .compute(self.window_size, &self.panel_state);
```
with:
```rust
let now = Instant::now();
let layout = self.current_render_layout(now);
if self.panel_transition.is_none() {
    self.last_committed_layout = Some(layout.clone());
}
```

- [ ] **Step 4: Build & smoke test**

Run: `cargo build`
Expected: compiles. (No behavior change yet — transitions are installed in Task 4.)

- [ ] **Step 5: Checkpoint** — build green, existing tests pass (`cargo test --lib`). Leave for human commit.

---

### Task 4: Install transition on dock toggles + tick + deadline

**Files:**
- Modify: handler(s) for `ToggleLeftDock` / `ToggleRightDock` / `ToggleBottomDock` (find via `command_ids.rs` → dispatch; likely `command_dispatch` or `commands_settings*` / event-loop command handling)
- Modify: `src/app/event_loop/application.rs` (`tick_panel_animation`, `about_to_wait`)

**Interfaces:**
- Consumes: `begin_layout_transition` (Task 3), `panel_transition`.
- Produces: `fn tick_panel_animation(&mut self) -> bool`.

- [ ] **Step 1: Install the transition where dock visibility flips.**

Find the place that calls `panel_state.toggle_left()/toggle_right()/toggle_bottom()` (search `toggle_left`/`toggle_right`/`toggle_bottom`). Immediately AFTER the toggle mutates `panel_state`, call:
```rust
self.begin_layout_transition(std::time::Instant::now());
self.request_redraw();
```
Do this for all three docks. (If toggles are handled centrally after dispatch, a single call there keyed on "panel visibility changed" is preferable.)

- [ ] **Step 2: Add `tick_panel_animation`** in `application.rs` (mirror `tick_smooth_scroll_animation`):

```rust
fn tick_panel_animation(&mut self) -> bool {
    let now = Instant::now();
    let Some(tr) = &self.panel_transition else {
        return false;
    };
    if tr.is_done(now) {
        // Finalize: commit the authoritative target layout, drop the transition.
        self.last_committed_layout = Some(tr.target().clone());
        self.panel_transition = None;
        self.mark_all_panels_dirty();
        return true;
    }
    self.mark_all_panels_dirty();
    true
}
```

- [ ] **Step 3: Drive it from `about_to_wait`** — add alongside the other ticks (after `tick_smooth_scroll_animation`):

```rust
if self.tick_panel_animation() {
    self.request_redraw();
}
```

- [ ] **Step 4: Add the ~8 ms deadline** so frames keep coming while animating. In `about_to_wait`, where `next_deadline` is assembled (near the yank/ripple deadline blocks ~lines 1138-1155), add:

```rust
if self.panel_transition.is_some() {
    let next_frame = Instant::now() + Duration::from_millis(8);
    next_deadline = Some(match next_deadline {
        Some(existing) => existing.min(next_frame),
        None => next_frame,
    });
}
```

- [ ] **Step 5: Manual verify** — run the editor, toggle left dock (`app.toggle_left_dock` keybinding). The editor should smoothly push, not snap.

Run: `cargo run`
Expected: smooth slide on dock toggle; with `[animation] enabled=false` it snaps.

- [ ] **Step 6: Checkpoint** — build + `cargo test --lib` green. Leave for human commit.

---

### Task 5: Zen / maximize transition

**Files:**
- Modify: handler for `ToggleMaximizeFocus` (`command_ids.rs` → `Command::ToggleMaximizeFocus`)

**Interfaces:**
- Consumes: `begin_layout_transition` (Task 3) — already handles differing tree shapes (matched ids lerp, others snap).

- [ ] **Step 1: Install transition on maximize toggle.** Find where `panels.maximized_region` is set/cleared (search `maximized_region`). Immediately after the mutation:
```rust
self.begin_layout_transition(std::time::Instant::now());
self.request_redraw();
```

- [ ] **Step 2: Manual verify** — toggle Zen (`TOGGLE_MAXIMIZE_FOCUS` binding). The focused region grows/others recede smoothly; matched regions interpolate, new-only regions snap (acceptable for v1).

Run: `cargo run`
Expected: smooth zen transition; no panic; reduce-motion snaps.

- [ ] **Step 3: Checkpoint** — build + tests green. Leave for human commit.

---

### Task 6: Overlay enter/leave (Command Palette, Code Graph HUD, popups)

**Files:**
- Modify: `src/app/event_loop/mod.rs` (`AppShell.overlay_motion: Option<motion::OverlayMotion>`)
- Modify: `src/app/event_loop/application.rs` (`tick_overlay_motion`, install on open/close, deadline)
- Modify: `src/render/renderer/editor/overlays.rs` and/or `src/render/renderer/palette.rs` / `lifecycle/frame.rs` — multiply overlay backdrop/content color alpha by the sample and apply scale about the overlay center.

**Interfaces:**
- Produces: `AppShell.overlay_motion`, `fn tick_overlay_motion(&mut self) -> bool`, and an accessor the renderer reads (e.g. `fn overlay_alpha_scale(&self, now) -> (f32, f32)` returning `(1.0, 1.0)` when no motion).

- [ ] **Step 1: Add field + helpers.** In `mod.rs`: `pub(super) overlay_motion: Option<crate::workbench::motion::OverlayMotion>,` (init `None`). In `application.rs`:
```rust
pub(super) fn begin_overlay_motion(&mut self, phase: crate::workbench::motion::OverlayPhase) {
    let anim = self.ui_config.animation;
    if !anim.enabled {
        self.overlay_motion = None;
        return;
    }
    self.overlay_motion = Some(crate::workbench::motion::OverlayMotion {
        phase,
        started_at: std::time::Instant::now(),
        duration: anim.overlay_duration(),
        curve: anim.curve,
    });
    self.request_redraw();
}

pub(super) fn overlay_alpha_scale(&self, now: std::time::Instant) -> (f32, f32) {
    match &self.overlay_motion {
        Some(m) => { let s = m.sample(now); (s.alpha, s.scale) }
        None => (1.0, 1.0),
    }
}

fn tick_overlay_motion(&mut self) -> bool {
    let now = std::time::Instant::now();
    let Some(m) = &self.overlay_motion else { return false; };
    if m.is_done(now) {
        // Enter -> steady (clear motion). Leave -> motion done; overlay already
        // closed in state, so just clear.
        self.overlay_motion = None;
        return true;
    }
    true
}
```

- [ ] **Step 2: Install on open/close.** Where the Command Palette / Code Graph HUD / popups open, call `self.begin_overlay_motion(OverlayPhase::Enter)`. Where they close, call `OverlayPhase::Leave` BEFORE the overlay's `open=false` is committed if you want a leave animation; for v1 a simple Enter-only animation is acceptable if leave is hard to sequence — keep Enter at minimum.

- [ ] **Step 3: Drive from `about_to_wait`** (after panel tick):
```rust
if self.tick_overlay_motion() {
    self.request_redraw();
}
```
and add an 8 ms deadline when `self.overlay_motion.is_some()` (same pattern as Task 4 Step 4).

- [ ] **Step 4: Apply alpha/scale in the renderer.** In the overlay draw path (`overlays.rs` / `palette.rs`), fetch `(alpha, scale)` (thread it from `redraw()` into the overlay-build call) and:
  - multiply the overlay backdrop + panel + text colors' alpha channel by `alpha`;
  - scale overlay quad/text positions about the overlay's center by `scale`.

- [ ] **Step 5: Manual verify** — open the command palette; it fades+pops in. `enabled=false` → instant.

Run: `cargo run`
Expected: palette/HUD fade+scale on open; no perf regression.

- [ ] **Step 6: Checkpoint** — build + tests green. Leave for human commit.

---

### Task 7: Perf validation + reduce-motion verification

**Files:** none (validation only); optionally tune `[animation]` durations.

- [ ] **Step 1: Run the perf bench.**

Run: `cargo bench --bench e2e_perf_runner` (or the documented perf script `scripts/run_perf_baseline.sh`).
Expected: frame-prep time within budget; record the number.

- [ ] **Step 2: Manual 90+ check.** Open `benchmarks/inputs/rust_10k_lines.rs`, toggle left/right/bottom docks repeatedly, watch for dropped frames (the renderer warns when frame > 8 ms via `FRAME_TIME_WARN_THRESHOLD`). Confirm no sustained warnings during the slide.

- [ ] **Step 3: Reduce-motion check.** Set `[animation] enabled = false` in `~/.config/netherize/ui.toml` (or the active ui config), restart, confirm toggles snap instantly with zero animation frames.

- [ ] **Step 4: If the gate fails for a content type** (sustained >8 ms): apply the deferred fallback from the spec §6 — lay that content out once at its target bounds and clip+translate during the slide rather than relaying out each frame. Re-run Step 1-2.

- [ ] **Step 5: Checkpoint** — record bench numbers in the task notes. Leave for human commit.

---

## Self-Review

- **Spec coverage:** §3 primitives → Task 1; §8 config → Task 2; §5 data flow + §6 effective-layout perf → Tasks 3-4; zen (§9) → Task 5; overlay enter/leave (§3B, §9) → Task 6; perf gate (§6, §11) + reduce-motion (§8) → Task 7. Mid-flight reversal (§7) → Task 3 `begin_layout_transition` (`from = current sample`). All covered.
- **Placeholders:** Task 2 references the project's actual `UiConfig` constructors — Step 1 resolves exact names before coding (config file too large to inline verbatim; pattern is the existing `smooth_scroll_*` block). Task 4/5/6 "find the handler" steps are searches, not placeholders — exact command IDs given (`app.toggle_left_dock`, `TOGGLE_MAXIMIZE_FOCUS`).
- **Type consistency:** `EaseCurve`, `LayoutTransition::{new,sample,is_done,target}`, `OverlayMotion`, `current_render_layout`, `begin_layout_transition`, `last_committed_layout`, `panel_transition`, `mark_all_panels_dirty` are used consistently across tasks.
