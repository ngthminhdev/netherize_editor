# Neovide-style pixel-smooth editor viewport scrolling — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development or superpowers:executing-plans to implement task-by-task. Steps use checkbox (`- [ ]`) syntax.

**Goal:** Make same-buffer page/jump/center editor scrolling slide pixel-by-pixel (Neovide-style, fixed ~300 ms ease-out) while keeping `j`/`k`, cross-file, and LSP navigation instant, and leaving NetherCanvas untouched.

**Architecture:** Reuse the existing `current_scroll_y` fractional renderer + `ease_scroll`/`clamp_scroll_start` motion primitives. Add a pure `plan_scroll_retarget` decision helper to `workbench::motion`, a `[motion]` config section, an animated centering variant, and a one-shot `scroll_anim_force` flag on `AppShell`. No `AppState`/snapshot changes.

**Tech Stack:** Rust, wgpu, winit event loop, `serde`/`toml` config.

## Global Constraints

- Feature affects only the **normal editor buffer viewport**; NetherCanvas camera/card/zoom must be **untouched**.
- Timing model is **fixed duration** (default 300 ms), curve `ease_out_cubic`. Do **not** use a `lines_per_sec` model.
- Snap threshold `SCROLL_SNAP_THRESHOLD_LINES = 1.5` stays; explicit commands bypass it via `force`.
- No new `AppState` fields, **no snapshot changes**; new motion state lives on `AppShell`.
- Legacy `[editor].smooth_scroll_*` keys stay parseable (deprecated/mapped); invalid config must not panic.
- Do not commit (human commits). Run `cargo test` / `cargo build` to verify.

---

### Task 1: Pure motion helpers (`scroll_far_clamp_lines`, `plan_scroll_retarget`)

**Files:**
- Modify: `src/workbench/motion.rs` (add after `ease_scroll`, ~line 78)
- Test: `src/workbench/motion.rs` (`#[cfg(test)]` module, alongside `ease_scroll_*` tests)

**Interfaces:**
- Consumes: existing `clamp_scroll_start(current, target, max)`.
- Produces:
  - `pub fn scroll_far_clamp_lines(viewport_lines: usize, far_lines: u32) -> f32`
  - `pub enum ScrollRetarget { Snap, Animate { start: f32 } }`
  - `pub fn plan_scroll_retarget(current: f32, target: f32, smooth_enabled: bool, force: bool, snap_threshold: f32, viewport_lines: usize, far_lines: u32) -> ScrollRetarget`

- [ ] **Step 1: Write failing tests**

```rust
#[test]
fn scroll_far_clamp_lines_is_screenful_plus_far() {
    assert_eq!(scroll_far_clamp_lines(40, 1), 41.0);
    assert_eq!(scroll_far_clamp_lines(0, 0), 1.0); // never zero
}

#[test]
fn plan_retarget_snaps_when_smooth_disabled() {
    assert_eq!(plan_scroll_retarget(0.0, 50.0, false, true, 1.5, 40, 1), ScrollRetarget::Snap);
}

#[test]
fn plan_retarget_snaps_small_delta_without_force() {
    // j/k cursor follow
    assert_eq!(plan_scroll_retarget(10.0, 11.0, true, false, 1.5, 40, 1), ScrollRetarget::Snap);
}

#[test]
fn plan_retarget_animates_small_delta_when_forced() {
    // explicit zz with tiny delta
    match plan_scroll_retarget(10.0, 11.0, true, true, 1.5, 40, 1) {
        ScrollRetarget::Animate { start } => assert!((start - 10.0).abs() < f32::EPSILON),
        other => panic!("expected animate, got {other:?}"),
    }
}

#[test]
fn plan_retarget_clamps_far_jump_to_screenful() {
    // gg from line 5000 to 0, viewport 40, far 1 -> start within 41 lines of target
    match plan_scroll_retarget(5000.0, 0.0, true, true, 1.5, 40, 1) {
        ScrollRetarget::Animate { start } => assert_eq!(start, 41.0),
        other => panic!("expected animate, got {other:?}"),
    }
}

#[test]
fn plan_retarget_recomputes_from_current_position() {
    // mid-tween at line 60, new target 0 -> start clamped from 60, not from old start
    match plan_scroll_retarget(60.0, 0.0, true, true, 1.5, 40, 1) {
        ScrollRetarget::Animate { start } => assert_eq!(start, 41.0),
        other => panic!("expected animate, got {other:?}"),
    }
}
```

- [ ] **Step 2: Run, expect FAIL** — `cargo test -p <crate> scroll_far_clamp_lines plan_retarget` → unresolved name.

- [ ] **Step 3: Implement**

```rust
/// Far-jump clamp width in lines: one screenful plus `far_lines` extra (Neovide
/// `scroll_animation_far_lines` adapted so large jumps still show a visible
/// settle). Never zero.
#[inline]
pub fn scroll_far_clamp_lines(viewport_lines: usize, far_lines: u32) -> f32 {
    (viewport_lines as f32 + far_lines as f32).max(1.0)
}

/// How a scroll-target change should be applied.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ScrollRetarget {
    /// Jump instantly to target (cursor follow below threshold, or smooth off).
    Snap,
    /// Start a tween from `start` (already far-clamped) toward target.
    Animate { start: f32 },
}

/// Decide snap vs animate for a scroll retarget. Pure so it is unit-testable
/// without an `AppShell`. `smooth_enabled` already folds in the global motion
/// gate, the editor toggle, and `animation_ms > 0`. `force` lets an explicit
/// command (zz/gg/G/Ctrl-D/U) animate even when `|delta| < snap_threshold`.
pub fn plan_scroll_retarget(
    current: f32,
    target: f32,
    smooth_enabled: bool,
    force: bool,
    snap_threshold: f32,
    viewport_lines: usize,
    far_lines: u32,
) -> ScrollRetarget {
    let delta = target - current;
    if !smooth_enabled || (!force && delta.abs() < snap_threshold) {
        return ScrollRetarget::Snap;
    }
    let max = scroll_far_clamp_lines(viewport_lines, far_lines);
    ScrollRetarget::Animate {
        start: clamp_scroll_start(current, target, max),
    }
}
```

- [ ] **Step 4: Run, expect PASS** — `cargo test -p <crate> motion::`

- [ ] **Step 5: Commit** (human) — staged: `src/workbench/motion.rs`.

---

### Task 2: `MotionConfig` + `[motion]` parsing + back-compat mapping

**Files:**
- Modify: `src/config/ui_config.rs` — add `MotionConfig` struct (~after `AnimationConfig`, line 141), `motion: MotionConfig` field on `UiConfig` (~line 179), `builtin()` init (~line 295), `from_raw()` parse block (~after the `animation` block, line 643), `RawMotion` struct + `RawUiFile.motion` field (~line 832/843).
- Modify: `config/ui/default.toml` — add `[motion]` section; deprecate `[editor]` smooth keys.
- Test: `src/config/ui_config.rs` `#[cfg(test)]` module (alongside `animation_config_*` tests).

**Interfaces:**
- Consumes: `EaseCurve`, `EaseCurve::from_str_or_default`.
- Produces:
  - `pub struct MotionConfig { pub enabled: bool, pub duration_ms: u32, pub ease: EaseCurve, pub editor_smooth_scroll_enabled: bool, pub editor_smooth_scroll_animation_ms: u32, pub editor_smooth_scroll_far_lines: u32 }`
  - `MotionConfig::editor_smooth_scroll_active(&self) -> bool`
  - `MotionConfig::editor_scroll_duration(&self) -> std::time::Duration`
  - `UiConfig.motion: MotionConfig`

- [ ] **Step 1: Write failing tests**

```rust
#[test]
fn motion_config_defaults() {
    let cfg = UiConfig::builtin();
    assert!(cfg.motion.enabled);
    assert_eq!(cfg.motion.editor_smooth_scroll_animation_ms, 300);
    assert_eq!(cfg.motion.editor_smooth_scroll_far_lines, 1);
    assert!(cfg.motion.editor_smooth_scroll_active());
}

#[test]
fn motion_disable_paths() {
    let mut m = UiConfig::builtin().motion;
    m.enabled = false;                       assert!(!m.editor_smooth_scroll_active());
    let mut m = UiConfig::builtin().motion;
    m.editor_smooth_scroll_enabled = false;  assert!(!m.editor_smooth_scroll_active());
    let mut m = UiConfig::builtin().motion;
    m.editor_smooth_scroll_animation_ms = 0; assert!(!m.editor_smooth_scroll_active());
}

#[test]
fn motion_parses_overrides_and_bad_ease_falls_back() {
    let toml_src = r#"
        [motion]
        enabled = true
        ease = "garbage"
        editor_smooth_scroll_animation_ms = 120
        editor_smooth_scroll_far_lines = 4
    "#;
    let raw: RawUiFile = toml::from_str(toml_src).unwrap();
    let cfg = UiConfig::from_raw(raw).unwrap();
    assert_eq!(cfg.motion.ease, crate::workbench::motion::EaseCurve::EaseOutCubic);
    assert_eq!(cfg.motion.editor_smooth_scroll_animation_ms, 120);
    assert_eq!(cfg.motion.editor_smooth_scroll_far_lines, 4);
}

#[test]
fn motion_back_compat_maps_legacy_editor_keys() {
    let toml_src = r#"
        [editor]
        smooth_scroll_enabled = false
        smooth_scroll_duration_ms = 90
    "#;
    let raw: RawUiFile = toml::from_str(toml_src).unwrap();
    let cfg = UiConfig::from_raw(raw).unwrap();
    assert!(!cfg.motion.editor_smooth_scroll_enabled);          // mapped from legacy
    assert_eq!(cfg.motion.editor_smooth_scroll_animation_ms, 90); // mapped from legacy
}

#[test]
fn motion_missing_block_uses_defaults() {
    let raw: RawUiFile = toml::from_str("").unwrap();
    let cfg = UiConfig::from_raw(raw).unwrap();
    assert_eq!(cfg.motion.editor_smooth_scroll_animation_ms, 300);
}
```

- [ ] **Step 2: Run, expect FAIL** — `cargo test -p <crate> motion_` → no field `motion`.

- [ ] **Step 3: Implement** — add the struct + Default + impl:

```rust
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MotionConfig {
    pub enabled: bool,
    pub duration_ms: u32,
    pub ease: crate::workbench::motion::EaseCurve,
    pub editor_smooth_scroll_enabled: bool,
    pub editor_smooth_scroll_animation_ms: u32,
    pub editor_smooth_scroll_far_lines: u32,
}

impl Default for MotionConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            duration_ms: 250,
            ease: crate::workbench::motion::EaseCurve::EaseOutCubic,
            editor_smooth_scroll_enabled: true,
            editor_smooth_scroll_animation_ms: 300,
            editor_smooth_scroll_far_lines: 1,
        }
    }
}

impl MotionConfig {
    pub fn editor_smooth_scroll_active(&self) -> bool {
        self.enabled
            && self.editor_smooth_scroll_enabled
            && self.editor_smooth_scroll_animation_ms > 0
    }
    pub fn editor_scroll_duration(&self) -> std::time::Duration {
        std::time::Duration::from_millis(self.editor_smooth_scroll_animation_ms as u64)
    }
}
```

Add `pub motion: MotionConfig,` to `UiConfig`; `motion: MotionConfig::default(),`
to `builtin()`. Add `RawMotion` + `#[serde(default)] motion: RawMotion,` to
`RawUiFile`:

```rust
#[derive(Debug, Default, Deserialize)]
struct RawMotion {
    enabled: Option<bool>,
    duration_ms: Option<u32>,
    ease: Option<String>,
    editor_smooth_scroll_enabled: Option<bool>,
    editor_smooth_scroll_animation_ms: Option<u32>,
    editor_smooth_scroll_far_lines: Option<u32>,
}
```

Add the `from_raw` parse block (with legacy mapping) after the `animation` block:

```rust
motion: {
    let fb = MotionConfig::default();
    MotionConfig {
        enabled: raw.motion.enabled.unwrap_or(fb.enabled),
        duration_ms: raw.motion.duration_ms.unwrap_or(fb.duration_ms),
        ease: raw.motion.ease
            .map(|s| crate::workbench::motion::EaseCurve::from_str_or_default(&s))
            .unwrap_or(fb.ease),
        editor_smooth_scroll_enabled: raw.motion.editor_smooth_scroll_enabled
            .or(raw.editor.smooth_scroll_enabled)          // legacy fallback
            .unwrap_or(fb.editor_smooth_scroll_enabled),
        editor_smooth_scroll_animation_ms: raw.motion.editor_smooth_scroll_animation_ms
            .or(raw.editor.smooth_scroll_duration_ms)       // legacy fallback
            .unwrap_or(fb.editor_smooth_scroll_animation_ms),
        editor_smooth_scroll_far_lines: raw.motion.editor_smooth_scroll_far_lines
            .unwrap_or(fb.editor_smooth_scroll_far_lines),
    }
},
```

Update `config/ui/default.toml`:

```toml
[editor]
relative_numbers = true
# Smooth-scroll moved to [motion] (Neovide-style). The keys below are deprecated
# but still parsed for backward compatibility.

[motion]
enabled = true
duration_ms = 250
ease = "ease_out_cubic"
editor_smooth_scroll_enabled = true
editor_smooth_scroll_animation_ms = 300
editor_smooth_scroll_far_lines = 1
```

- [ ] **Step 4: Run, expect PASS** — `cargo test -p <crate> motion_`

- [ ] **Step 5: Commit** (human).

---

### Task 3: Animated centering variant (`center_cursor_line_animated`)

**Files:**
- Modify: `src/app/app_state/editor.rs` (add next to `center_cursor_line`, ~line 1078)
- Test: `src/app/app_state/editor.rs` `#[cfg(test)]` (or the editor test module)

**Interfaces:**
- Produces: `AppState::center_cursor_line_animated(&mut self, viewport_lines: usize)`.
- Leaves `AppState::center_cursor_line` unchanged (snaps both).

- [ ] **Step 1: Write failing tests** (build an `AppState` with multi-line text via the existing editor-test constructor; mirror an existing editor test's setup)

```rust
#[test]
fn center_cursor_line_animated_sets_target_only() {
    let mut st = /* existing test AppState with >100 lines, cursor at line 50 */;
    st.current_scroll_y = 7.0;
    let before = st.current_scroll_y;
    st.center_cursor_line_animated(20);
    assert_ne!(st.target_scroll_y, before);     // target moved to center
    assert_eq!(st.current_scroll_y, before);    // current NOT snapped
}

#[test]
fn center_cursor_line_still_snaps() {
    let mut st = /* same setup */;
    st.current_scroll_y = 7.0;
    st.center_cursor_line(20);
    assert_eq!(st.current_scroll_y, st.target_scroll_y); // snapped
}
```

- [ ] **Step 2: Run, expect FAIL** — method missing.

- [ ] **Step 3: Implement**

```rust
/// Like `center_cursor_line` but sets only `target_scroll_y`; the smooth-scroll
/// tick eases `current_scroll_y` toward it (Neovide-style `zz`). Used by the
/// editor `zz`/`gg` commands; LSP/go-to-def keep the snapping `center_cursor_line`.
pub fn center_cursor_line_animated(&mut self, viewport_lines: usize) {
    let (cursor_line, _) = self.cursor_line_col();
    self.target_scroll_y = cursor_line.saturating_sub(viewport_lines / 2) as f32;
}
```

- [ ] **Step 4: Run, expect PASS.**

- [ ] **Step 5: Commit** (human).

---

### Task 4: `scroll_anim_force` flag on `AppShell` + command wiring

**Files:**
- Modify: `src/app/event_loop/mod.rs` — add `scroll_anim_force: bool,` field next to `scroll_anim_last_target` (~line 336).
- Modify: `src/app/event_loop/setup.rs` — init `scroll_anim_force: false,` (~line 289).
- Modify: `src/app/event_loop/commands_editor.rs` — switch `CenterCursorLine`/`MoveToFirstLine` to `center_cursor_line_animated` and set `self.scroll_anim_force = true;` for the whole arm (lines 125-155).

**Interfaces:**
- Consumes: `AppState::center_cursor_line_animated` (Task 3).
- Produces: `AppShell.scroll_anim_force` (read one-shot in Task 5).

- [ ] **Step 1:** Add field declaration (`mod.rs`) with a doc comment:

```rust
/// One-shot: set by an explicit scroll command (zz/gg/G/Ctrl-D/U) so the next
/// `advance_scroll_anim` retarget animates even below the snap threshold. Read
/// via `std::mem::take` so it cannot leak into a later j/k cursor-follow.
scroll_anim_force: bool,
```

- [ ] **Step 2:** Init in `setup.rs`: `scroll_anim_force: false,`.

- [ ] **Step 3:** In `commands_editor.rs`, change the two centering sites and arm:

```rust
Command::CenterCursorLine => {
    self.app_state.center_cursor_line_animated(viewport_lines);
}
// ...
Command::MoveToFirstLine => {
    self.app_state.move_to_first_line();
    self.app_state.center_cursor_line_animated(viewport_lines);
}
```

After the inner `match command { ... }` (still inside the outer arm, before its
closing `}`), add:

```rust
// Explicit scroll command: animate even if the delta is below the snap
// threshold (consumed one-shot in advance_scroll_anim).
self.scroll_anim_force = true;
```

- [ ] **Step 4: Build** — `cargo build` (no behavior test yet; verified in Task 5).

- [ ] **Step 5: Commit** (human).

---

### Task 5: Wire `advance_scroll_anim` to `[motion]` + `plan_scroll_retarget` + force

**Files:**
- Modify: `src/app/event_loop/application.rs` — `advance_scroll_anim` (lines 1724-1773).
- Test: shell-level test module that already builds an `AppShell` (e.g. `src/app/event_loop/async_results/mod.rs` or `commands_tests.rs`).

**Interfaces:**
- Consumes: `MotionConfig` accessors (Task 2), `plan_scroll_retarget`/`ScrollRetarget`/`scroll_far_clamp_lines` (Task 1), `scroll_anim_force` (Task 4).

- [ ] **Step 1:** Rewrite the retarget section of `advance_scroll_anim`:

```rust
fn advance_scroll_anim(&mut self, now: Instant) -> bool {
    use crate::workbench::motion::{ease_scroll, plan_scroll_retarget, ScrollRetarget};

    self.last_scroll_animation_tick = now;

    let target = self.app_state.target_scroll_y;
    let current = self.app_state.current_scroll_y;
    let motion = &self.ui_config.motion;
    let smooth = motion.editor_smooth_scroll_active();
    let duration = motion.editor_scroll_duration();
    let curve = motion.ease;
    let far_lines = motion.editor_smooth_scroll_far_lines;

    // One-shot: consumed by exactly the tick that observes the target change.
    let force = std::mem::take(&mut self.scroll_anim_force);

    if (target - self.scroll_anim_last_target).abs() > f32::EPSILON {
        self.scroll_anim_last_target = target;
        let viewport_lines = self.editor_viewport_lines();
        match plan_scroll_retarget(
            current, target, smooth, force,
            SCROLL_SNAP_THRESHOLD_LINES, viewport_lines, far_lines,
        ) {
            ScrollRetarget::Snap => {
                self.scroll_anim_started_at = None;
                if (current - target).abs() > f32::EPSILON {
                    self.app_state.current_scroll_y = target;
                    return true;
                }
                return false;
            }
            ScrollRetarget::Animate { start } => {
                self.scroll_anim_start = start;
                self.scroll_anim_started_at = Some(now);
                // fall through to sample at t=0 this frame
            }
        }
    }

    let Some(started_at) = self.scroll_anim_started_at else { return false; };
    let (value, done) =
        ease_scroll(self.scroll_anim_start, target, started_at, now, duration, curve);
    if done { self.scroll_anim_started_at = None; }
    let changed = (self.app_state.current_scroll_y - value).abs() > f32::EPSILON;
    self.app_state.current_scroll_y = value;
    changed
}
```

(Remove the old `clamp_scroll_start`/`EaseCurve` import line and the inline
`smooth`/`duration` reads of `ui_config.editor.smooth_scroll_*`.)

- [ ] **Step 2: Write failing/■ shell tests** (reuse the existing AppShell test builder in the chosen module):

```rust
#[test]
fn ctrl_d_animates_not_snaps() {
    let mut shell = /* existing test shell with a long buffer */;
    let now = std::time::Instant::now();
    shell.advance_scroll_anim(now);                 // settle baseline
    let half = (shell.editor_viewport_lines() / 2).max(1);
    shell.app_state.scroll_half_page_down(half);
    shell.scroll_anim_force = true;                 // as the command arm would
    let started = now + std::time::Duration::from_millis(1);
    let moved = shell.advance_scroll_anim(started);
    assert!(moved);
    assert!(shell.scroll_anim_started_at.is_some(), "should be animating");
    assert!(shell.app_state.current_scroll_y < shell.app_state.target_scroll_y);
}

#[test]
fn editor_scroll_does_not_touch_canvas() {
    let mut shell = /* test shell */;
    let canvas_before = format!("{:?}", shell.app_state.canvas /* camera/cards */);
    shell.app_state.scroll_half_page_down(10);
    shell.scroll_anim_force = true;
    shell.advance_scroll_anim(std::time::Instant::now());
    let canvas_after = format!("{:?}", shell.app_state.canvas);
    assert_eq!(canvas_before, canvas_after);
}
```

(Adapt field access — `shell.app_state.canvas` — to the real canvas accessor;
if no `Debug` snapshot is available, assert the specific camera offset / card
count fields are unchanged.)

- [ ] **Step 3: Run, expect FAIL then implement until PASS** — `cargo test -p <crate> ctrl_d_animates editor_scroll_does_not_touch_canvas`.

- [ ] **Step 4: Full regression** — `cargo test` (all green, incl. updated `animation_config_*`).

- [ ] **Step 5: Commit** (human).

---

## Self-Review

- **Spec coverage:** timing (T5), far-clamp (T1/T5), `[motion]` config + 3 disables + back-compat + invalid-fallback (T2), animated `zz`/`gg` (T3/T4), force bypass (T4/T5), `j`/`k` snap (T1), retarget-from-current (T1/T5), canvas isolation (T5), no-snapshot-change (T4 puts state on AppShell). ✓ All spec sections map to a task.
- **Placeholders:** test bodies that say "existing test shell" reference a real harness the implementer must locate in the named module — acceptable (codebase-specific construction), all logic shown.
- **Type consistency:** `plan_scroll_retarget` / `ScrollRetarget::Animate { start }` / `scroll_far_clamp_lines` / `MotionConfig::editor_smooth_scroll_active` / `editor_scroll_duration` / `center_cursor_line_animated` / `scroll_anim_force` names identical across tasks. ✓
