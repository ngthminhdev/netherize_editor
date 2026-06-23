# Coupled Cursor + Viewport Smooth Scroll (v2) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to
> implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make editor vertical navigation feel calm and continuous — the caret never
teleports relative to the moving text, the viewport glides, folded regions don't
jitter, and input stays zero-latency.

**Architecture:** One animation clock drives two eased values in visual-line space:
the existing scroll tween (`current_scroll_y` → `target_scroll_y`) and a new caret
lag. Both share `started_at`, duration, curve and the same eased fraction, so the
caret's screen row is stable when caret and scroll move by the same delta. Three
bundled bug fixes: visual-space `auto_scroll_to_cursor`, styled-span caching across
scroll-only frames, and rendering the caret at its animated (lagged) position.

**Tech Stack:** Rust, wgpu/cosmic-text rendering, winit event loop. Tests via
`cargo test` (`#[cfg(test)]` modules colocated with code).

## Global Constraints

- Logical state (cursor, `target_scroll_y`) updates immediately; animation is visual only.
- All scroll/caret easing happens in **visual-line space** (fold-safe), reusing
  `AppState::logical_scroll_to_visual` / `visual_scroll_to_logical`.
- Cross-file / go-to-def / search-LSP jumps **snap** (unchanged). NetherCanvas untouched.
- Legacy `editor.smooth_scroll_*` TOML keys stay parseable.
- `editor_smooth_scroll_animation_ms = 0` or `enabled = false` → everything snaps.
- Do NOT run git commit/push (human-only per global rule). Plan shows no commit steps.
- rtk shell proxy prints `setValueForKeyFakeAssocArray ... _encode` noise — cosmetic; ignore.

## File Structure

- `src/workbench/motion.rs` — pure: add `ease_fraction` (shared progress) + `scroll_duration_for_distance`.
- `src/config/ui_config.rs` — `MotionConfig` per-distance duration fields + parse + back-compat.
- `src/app/app_state/editor.rs` — rewrite `auto_scroll_to_cursor` in visual space.
- `src/app/app_state/mod.rs` — add `caret_scroll_lag: f32` field + `cursor_visual_line()` helper.
- `src/app/event_loop/mod.rs` + `setup.rs` — add `caret_anim_start: f32` shell field.
- `src/app/event_loop/application.rs` — `advance_scroll_anim` eases scroll + caret lag together.
- `src/render/renderer/editor/viewport.rs` — offset caret Y by `caret_scroll_lag`.
- Perf: `src/app/event_loop/application.rs` — cache `text`/`styled_spans` across scroll frames.

---

### Task 1: Shared eased fraction + distance→duration (motion.rs)

**Files:**
- Modify: `src/workbench/motion.rs` (after `ease_scroll`, ~line 78)
- Test: same file `#[cfg(test)] mod tests`

**Interfaces:**
- Consumes: `EaseCurve` (exists).
- Produces:
  - `pub fn ease_fraction(started_at: Instant, now: Instant, duration: Duration, curve: EaseCurve) -> (f32, bool)` — returns `(eased_fraction in 0..=1, done)`. `done` true when elapsed ≥ duration; zero duration → `(1.0, true)`.
  - `pub fn scroll_duration_for_distance(animated_lines: f32, step_ms: u32, halfpage_ms: u32, center_ms: u32) -> Duration` — picks step (≤3 lines), halfpage (≤ ~viewport/2 ⇒ treat as |lines|≤24), else center, by `animated_lines.abs()`.

- [ ] **Step 1: Write failing tests**

```rust
#[test]
fn ease_fraction_zero_duration_is_done_at_one() {
    let t0 = Instant::now();
    let (f, done) = ease_fraction(t0, t0, Duration::ZERO, EaseCurve::EaseOutCubic);
    assert_eq!(f, 1.0);
    assert!(done);
}

#[test]
fn ease_fraction_matches_ease_scroll_value() {
    let t0 = Instant::now();
    let now = t0 + Duration::from_millis(40);
    let dur = Duration::from_millis(120);
    let (frac, _) = ease_fraction(t0, now, dur, EaseCurve::EaseOutCubic);
    let (val, _) = ease_scroll(10.0, 30.0, t0, now, dur, EaseCurve::EaseOutCubic);
    // value reconstructed from the shared fraction must equal ease_scroll's value.
    assert!((10.0 + (30.0 - 10.0) * frac - val).abs() < 1e-4);
}

#[test]
fn duration_scales_with_distance() {
    let s = scroll_duration_for_distance(2.0, 80, 120, 130);
    let h = scroll_duration_for_distance(20.0, 80, 120, 130);
    let c = scroll_duration_for_distance(200.0, 80, 120, 130);
    assert_eq!(s, Duration::from_millis(80));
    assert_eq!(h, Duration::from_millis(120));
    assert_eq!(c, Duration::from_millis(130));
}
```

- [ ] **Step 2: Run, verify fail** — `cargo test -p <crate> ease_fraction 2>/dev/null` → FAIL (undefined).
- [ ] **Step 3: Implement**

```rust
/// Shared eased progress for phase-locking two tweens (scroll + caret) on one clock.
/// Returns `(eased_fraction, done)`. Zero duration → `(1.0, true)`.
pub fn ease_fraction(
    started_at: Instant,
    now: Instant,
    duration: Duration,
    curve: EaseCurve,
) -> (f32, bool) {
    if duration.is_zero() {
        return (1.0, true);
    }
    let elapsed = now.saturating_duration_since(started_at).as_secs_f32();
    let t = (elapsed / duration.as_secs_f32()).clamp(0.0, 1.0);
    (curve.apply(t), t >= 1.0)
}

/// Pick a short, distance-scaled duration. `animated_lines` is the post-clamp
/// visual distance, so far jumps (clamped) use the short bucket, not a long one.
pub fn scroll_duration_for_distance(
    animated_lines: f32,
    step_ms: u32,
    halfpage_ms: u32,
    center_ms: u32,
) -> Duration {
    let d = animated_lines.abs();
    let ms = if d <= 3.0 {
        step_ms
    } else if d <= 24.0 {
        halfpage_ms
    } else {
        center_ms
    };
    Duration::from_millis(ms as u64)
}
```

- [ ] **Step 4: Run, verify pass** — `cargo test -p <crate> ease_fraction duration_scales 2>/dev/null` → PASS.

---

### Task 2: Per-distance duration config (ui_config.rs)

**Files:**
- Modify: `src/config/ui_config.rs` — `MotionConfig` (struct ~147, Default ~160, impl ~169), `from_raw` (~698), `RawMotion` (~918).
- Test: same file `#[cfg(test)] mod tests`.

**Interfaces:**
- Produces on `MotionConfig`: `pub editor_scroll_step_ms: u32`, `pub editor_scroll_halfpage_ms: u32`, `pub editor_scroll_center_ms: u32` (defaults 80/120/130). Back-compat: when these keys are absent but legacy `editor_smooth_scroll_animation_ms` is set, all three fall back to it. Existing `editor_scroll_duration()` stays (now unused by editor scroll but kept for compatibility).

- [ ] **Step 1: Write failing tests**

```rust
#[test]
fn motion_distance_durations_default() {
    let m = MotionConfig::default();
    assert_eq!(m.editor_scroll_step_ms, 80);
    assert_eq!(m.editor_scroll_halfpage_ms, 120);
    assert_eq!(m.editor_scroll_center_ms, 130);
}

#[test]
fn motion_distance_durations_parse_override() {
    let toml = r#"
[motion]
editor_scroll_step_ms = 60
editor_scroll_halfpage_ms = 100
editor_scroll_center_ms = 110
"#;
    let cfg = UiConfig::from_toml_str_for_test(toml);
    assert_eq!(cfg.motion.editor_scroll_step_ms, 60);
    assert_eq!(cfg.motion.editor_scroll_halfpage_ms, 100);
    assert_eq!(cfg.motion.editor_scroll_center_ms, 110);
}

#[test]
fn motion_distance_durations_fall_back_to_legacy_animation_ms() {
    let toml = r#"
[motion]
editor_smooth_scroll_animation_ms = 90
"#;
    let cfg = UiConfig::from_toml_str_for_test(toml);
    assert_eq!(cfg.motion.editor_scroll_step_ms, 90);
    assert_eq!(cfg.motion.editor_scroll_halfpage_ms, 90);
    assert_eq!(cfg.motion.editor_scroll_center_ms, 90);
}
```

(Use whatever the existing test harness is for parsing a `[motion]` block — mirror
the existing `motion_parses_overrides_and_bad_ease_falls_back` test's construction.)

- [ ] **Step 2: Run, verify fail** — fields undefined.
- [ ] **Step 3: Implement** — add the three `u32` fields to `MotionConfig`; defaults 80/120/130 in `Default`; add to `builtin()`/struct literal sites; add `Option<u32>` triplet to `RawMotion`; in `from_raw` resolve each as `raw.motion.editor_scroll_step_ms.or(raw.motion.editor_smooth_scroll_animation_ms).unwrap_or(fb.editor_scroll_step_ms)` (and analogously for halfpage/center). Add a helper:

```rust
impl MotionConfig {
    pub fn scroll_duration_for(&self, animated_lines: f32) -> std::time::Duration {
        crate::workbench::motion::scroll_duration_for_distance(
            animated_lines,
            self.editor_scroll_step_ms,
            self.editor_scroll_halfpage_ms,
            self.editor_scroll_center_ms,
        )
    }
}
```

- [ ] **Step 4: Run, verify pass.** Also update `config/ui/default.toml` `[motion]` block with the three keys (commented defaults).

---

### Task 3: Visual-space `auto_scroll_to_cursor` (app_state)

**Files:**
- Modify: `src/app/app_state/editor.rs:1116-1133` (`auto_scroll_to_cursor`).
- Test: `src/app/app_state/tests.rs`.

**Interfaces:**
- Consumes: `self.cursor_line_col()`, `self.fold_marker_line_for_hidden_line`, `self.logical_scroll_to_visual`, `self.visual_scroll_to_logical`, `self.folded_ranges`.
- Produces: same signature `pub fn auto_scroll_to_cursor(&mut self, viewport_lines: usize)` but computes the follow in visual-line space so the edge step is exactly the visual overflow and a fold-cross never oscillates.

- [ ] **Step 1: Write failing tests** (in `tests.rs`)

```rust
#[test]
fn auto_scroll_edge_follow_moves_one_visual_line() {
    // 200 plain lines, no folds. Put viewport so cursor sits exactly at the
    // bottom margin, then move one line down → target advances by exactly 1.
    let mut st = AppState::from_text(&"x\n".repeat(200));
    let viewport = 40usize;
    st.set_target_scroll_line(10);
    st.snap_current_scroll_to_target();
    // cursor to the last visible margin line:
    st.jump_to_line_and_column(10 + viewport - 3 - 1, 0); // just inside margin
    st.auto_scroll_to_cursor(viewport);
    let before = st.target_scroll_y;
    st.jump_to_line_and_column(10 + viewport - 3, 0); // crosses the bottom margin by 1
    st.auto_scroll_to_cursor(viewport);
    assert!((st.target_scroll_y - (before + 1.0)).abs() < 1e-3,
        "edge follow must advance exactly one visual line, got {} -> {}", before, st.target_scroll_y);
}

#[test]
fn auto_scroll_across_fold_is_monotonic_in_visual_space() {
    // Build text with a real fold so visual height < logical height; scanning the
    // cursor downward across the fold must produce non-decreasing visual targets.
    let mut st = AppState::from_text(&"x\n".repeat(200));
    st.add_fold_for_test(20, 60); // hide lines 21..=60 (helper; see Step 3 note)
    let viewport = 40usize;
    let mut last_visual = -1.0f32;
    for line in 0..120 {
        st.jump_to_line_and_column(line, 0);
        st.auto_scroll_to_cursor(viewport);
        let v = st.logical_scroll_to_visual(st.target_scroll_y);
        assert!(v + 1e-3 >= last_visual, "visual target went backwards at line {line}: {v} < {last_visual}");
        last_visual = v;
    }
}
```

> Note: if `add_fold_for_test` / `jump_to_line_and_column` helpers don't exist with
> these exact names, use the existing fold-insertion + cursor-jump test helpers in
> `tests.rs` (the v1 fold tests already insert `folded_ranges`); match their names.

- [ ] **Step 2: Run, verify fail.**
- [ ] **Step 3: Implement** — rewrite the body to operate on visual lines:

```rust
pub fn auto_scroll_to_cursor(&mut self, viewport_lines: usize) {
    let (raw_cursor_line, _) = self.cursor_line_col();
    let cursor_logical = self
        .fold_marker_line_for_hidden_line(raw_cursor_line)
        .unwrap_or(raw_cursor_line);
    // Work entirely in visual-line space so folds (zero-height hidden spans)
    // never desync the follow math.
    let cursor_visual = self.logical_scroll_to_visual(cursor_logical as f32);
    let top_visual = self.logical_scroll_to_visual(self.target_scroll_y);
    let margin = 3.0f32;
    let vp = viewport_lines as f32;
    let new_top_visual = if cursor_visual < top_visual + margin {
        (cursor_visual - margin).max(0.0)
    } else if vp > margin && cursor_visual + margin >= top_visual + vp {
        cursor_visual + margin + 1.0 - vp
    } else {
        top_visual
    };
    if (new_top_visual - top_visual).abs() > f32::EPSILON {
        self.target_scroll_y = self.visual_scroll_to_logical(new_top_visual.max(0.0));
    }
}
```

- [ ] **Step 4: Run, verify pass.** Re-run the full `app_state` test module to confirm no regression in existing scroll/fold tests.

---

### Task 4: Caret lag state + helper (app_state)

**Files:**
- Modify: `src/app/app_state/mod.rs` — add field `caret_scroll_lag: f32` (near `target_scroll_y` ~line 2281, default 0.0 at every constructor site ~2385/2513) + a public reader/writer and a `cursor_visual_line()` helper.
- Test: `src/app/app_state/tests.rs`.

**Interfaces:**
- Produces:
  - `pub caret_scroll_lag: f32` — visual-line offset added to the caret's drawn Y (0 = no lag). Read by the renderer.
  - `pub fn cursor_visual_line(&self) -> f32` — the cursor's current visual line (fold-aware), via `fold_marker_line_for_hidden_line` + `logical_scroll_to_visual`.

- [ ] **Step 1: Write failing test**

```rust
#[test]
fn cursor_visual_line_without_folds_equals_logical() {
    let mut st = AppState::from_text(&"x\n".repeat(50));
    st.jump_to_line_and_column(17, 0);
    assert!((st.cursor_visual_line() - 17.0).abs() < 1e-3);
}

#[test]
fn caret_scroll_lag_defaults_zero() {
    let st = AppState::from_text("a\nb\n");
    assert_eq!(st.caret_scroll_lag, 0.0);
}
```

- [ ] **Step 2: Run, verify fail.**
- [ ] **Step 3: Implement** field + helper:

```rust
pub fn cursor_visual_line(&self) -> f32 {
    let (raw, _) = self.cursor_line_col();
    let logical = self.fold_marker_line_for_hidden_line(raw).unwrap_or(raw);
    self.logical_scroll_to_visual(logical as f32)
}
```

Add `caret_scroll_lag: 0.0` to every `AppState { .. }` constructor (mod.rs + canvas_edit.rs swap if it mirrors scroll fields — check `canvas_edit.rs:128` swaps and mirror `caret_scroll_lag` there too).

- [ ] **Step 4: Run, verify pass.**

---

### Task 5: Couple caret to scroll in the animation tick (shell)

**Files:**
- Modify: `src/app/event_loop/mod.rs` (~327) + `setup.rs` — add `caret_anim_start: f32` (default 0.0).
- Modify: `src/app/event_loop/application.rs:1725-1789` (`advance_scroll_anim`).
- Test: `src/app/event_loop/commands_tests.rs`.

**Interfaces:**
- Consumes: `motion::ease_fraction`, `motion::plan_scroll_retarget`, `MotionConfig::scroll_duration_for`, `AppState::cursor_visual_line`, `AppState::{logical_scroll_to_visual, visual_scroll_to_logical}`.
- Produces: each tick sets `self.app_state.current_scroll_y` (eased) AND `self.app_state.caret_scroll_lag = caret_anim_visual - cursor_visual_now`. When snapping, `caret_scroll_lag = 0`.

- [ ] **Step 1: Write failing shell tests**

```rust
#[test]
fn halfpage_down_eases_caret_with_scroll() {
    let mut shell = AppShell::new_for_tests();
    shell.load_text_for_tests(&"x\n".repeat(400));
    let now = std::time::Instant::now();
    shell.dispatch_command_for_tests(Command::ScrollHalfPageDown);
    // first tick retargets, second samples mid-tween
    let _ = shell.advance_scroll_anim(now + std::time::Duration::from_millis(1));
    let _ = shell.advance_scroll_anim(now + std::time::Duration::from_millis(40));
    // Mid-tween the caret lags (nonzero) AND scroll is between old and target.
    assert!(shell.app_state.caret_scroll_lag.abs() > 1e-3,
        "caret should lag mid-tween, got {}", shell.app_state.caret_scroll_lag);
    assert!(shell.app_state.current_scroll_y > 0.0
        && shell.app_state.current_scroll_y < shell.app_state.target_scroll_y);
}

#[test]
fn caret_screen_row_constant_for_equal_delta_follow() {
    // Ctrl-D moves cursor and scroll by the same delta → caret screen row
    // (cursor_visual + lag) - scroll_visual is ~constant across the tween.
    let mut shell = AppShell::new_for_tests();
    shell.load_text_for_tests(&"x\n".repeat(400));
    let now = std::time::Instant::now();
    shell.dispatch_command_for_tests(Command::ScrollHalfPageDown);
    let row = |s: &AppShell| {
        s.app_state.cursor_visual_line() + s.app_state.caret_scroll_lag
            - s.app_state.logical_scroll_to_visual(s.app_state.current_scroll_y)
    };
    let _ = shell.advance_scroll_anim(now + std::time::Duration::from_millis(1));
    let r1 = row(&shell);
    let _ = shell.advance_scroll_anim(now + std::time::Duration::from_millis(50));
    let r2 = row(&shell);
    assert!((r1 - r2).abs() < 0.6, "caret screen row drifted: {r1} vs {r2}");
}

#[test]
fn single_line_move_without_scroll_keeps_caret_unlagged() {
    let mut shell = AppShell::new_for_tests();
    shell.load_text_for_tests(&"x\n".repeat(400));
    let now = std::time::Instant::now();
    shell.dispatch_command_for_tests(Command::MoveDown); // no viewport move
    let _ = shell.advance_scroll_anim(now + std::time::Duration::from_millis(1));
    assert_eq!(shell.app_state.caret_scroll_lag, 0.0);
}
```

> Use the actual test entry points present in `commands_tests.rs` (`AppShell::new_for_tests`,
> the existing `dispatch`/`load` helpers — mirror `forced_editor_scroll_animates_not_snaps`).

- [ ] **Step 2: Run, verify fail.**
- [ ] **Step 3: Implement `advance_scroll_anim`** — compute scroll + caret on one clock:

```rust
pub(super) fn advance_scroll_anim(&mut self, now: Instant) -> bool {
    use crate::workbench::motion::{ease_fraction, plan_scroll_retarget, ScrollRetarget};
    self.last_scroll_animation_tick = now;

    let target = self.app_state.target_scroll_y;
    let current = self.app_state.current_scroll_y;
    let motion = self.ui_config.motion;
    let smooth = motion.editor_smooth_scroll_active();
    let curve = motion.ease;
    let far_lines = motion.editor_smooth_scroll_far_lines;
    let force = std::mem::take(&mut self.scroll_anim_force);

    if (target - self.scroll_anim_last_target).abs() > f32::EPSILON {
        self.scroll_anim_last_target = target;
        let viewport_lines = self.editor_viewport_lines();
        match plan_scroll_retarget(current, target, smooth, force,
            SCROLL_SNAP_THRESHOLD_LINES, viewport_lines, far_lines)
        {
            ScrollRetarget::Snap => {
                self.scroll_anim_started_at = None;
                self.app_state.caret_scroll_lag = 0.0;
                if (current - target).abs() > f32::EPSILON {
                    self.app_state.current_scroll_y = target;
                    return true;
                }
                return false;
            }
            ScrollRetarget::Animate { start } => {
                self.scroll_anim_start = start;
                // Caret begins where it's currently drawn (cursor_visual + existing lag)
                // so a retarget mid-tween doesn't snap the caret.
                self.caret_anim_start =
                    self.app_state.cursor_visual_line() + self.app_state.caret_scroll_lag;
                // Duration scaled by the *visual* animated distance (post-clamp).
                let v_start = self.app_state.logical_scroll_to_visual(start);
                let v_target = self.app_state.logical_scroll_to_visual(target);
                self.scroll_anim_duration = motion.scroll_duration_for(v_target - v_start);
                self.scroll_anim_started_at = Some(now);
            }
        }
    }

    let Some(started_at) = self.scroll_anim_started_at else {
        return false;
    };
    let (frac, done) = ease_fraction(started_at, now, self.scroll_anim_duration, curve);

    // Scroll eased in visual space, converted back to logical for the renderer.
    let v_start = self.app_state.logical_scroll_to_visual(self.scroll_anim_start);
    let v_target = self.app_state.logical_scroll_to_visual(target);
    let v_value = v_start + (v_target - v_start) * frac;
    let value = self.app_state.visual_scroll_to_logical(v_value);

    // Caret eased on the SAME fraction toward the live cursor visual line; the lag
    // is what the renderer adds to the caret Y. Ends at 0 when frac == 1.
    let caret_target = self.app_state.cursor_visual_line();
    let caret_value = self.caret_anim_start + (caret_target - self.caret_anim_start) * frac;
    self.app_state.caret_scroll_lag = caret_value - caret_target;

    if done {
        self.scroll_anim_started_at = None;
        self.app_state.caret_scroll_lag = 0.0;
    }
    let changed = (self.app_state.current_scroll_y - value).abs() > f32::EPSILON
        || self.app_state.caret_scroll_lag.abs() > f32::EPSILON;
    self.app_state.current_scroll_y = value;
    changed
}
```

Add shell fields `caret_anim_start: f32` and `scroll_anim_duration: Duration` (mod.rs + setup.rs init to `0.0` / `Duration::ZERO`). Keep the existing `about_to_wait` redraw-while-animating gate as-is.

- [ ] **Step 4: Run, verify pass.** Re-run existing `advance_scroll_anim` shell tests; update any that assumed the old fixed-duration field (`editor_scroll_duration`) — they should now pass through `scroll_duration_for`.

---

### Task 6: Render caret at the lagged position (renderer)

**Files:**
- Modify: `src/render/renderer/editor/viewport.rs:382-449` (`update_editor_content`, caret origin).

**Interfaces:**
- Consumes: `app_state.caret_scroll_lag` (visual lines), `geometry.line_height`.
- Produces: caret rects shifted by `caret_scroll_lag * line_height` so the caret is drawn at its animated visual line, not the snapped cursor line. Text/glyphs are NOT shifted.

- [ ] **Step 1:** No GPU unit test (rendering). Add the offset to the caret origin only:

```rust
let caret_origin_y =
    corrected_origin_y + app_state.caret_scroll_lag * geometry.line_height;
```

Use `caret_origin_y` in place of `corrected_origin_y` for BOTH `build_caret_rects(..., [geometry.origin_x, caret_origin_y])` and the cursor-overlay/block path (`caret_rect_for_mode` input via `projection.caret_layout` is from the text layout at `corrected_origin_y`; add the same `caret_scroll_lag * line_height` to the resulting caret rect `.y` and the overlay instance Y). Keep `editor_focal_screen` / connector anchor on `corrected_origin_y` (it tracks the text line, not the caret).

- [ ] **Step 2: Build** — `cargo build 2>/dev/null` → no errors.
- [ ] **Step 3: Manual smoke** — run app, Ctrl-D/U, j/k at edge, zz, G/gg, over folds; confirm caret stays calm (verified by the user). Document in the design doc's follow-up section.

---

### Task 7: Cache styled spans across scroll-only frames (perf)

**Files:**
- Modify: `src/app/event_loop/application.rs:2335-2356` (the `else` editor-content branch) + shell fields in `mod.rs`/`setup.rs`.

**Interfaces:**
- Consumes: `app_state.revision()`, `self.semantic_highlight_request_revision` (or the span source revisions), theme generation.
- Produces: `text`/`styled_spans` recomputed only when the content/spans actually change; scroll-only frames reuse cached values. The renderer's own reshape cache (`viewport.rs:358`) already skips re-shaping; this removes the upstream per-frame `text_string()` + `syntax_spans_to_styled`.

- [ ] **Step 1: Write a failing unit test** for the cache-key predicate (pure helper):

```rust
// in application.rs #[cfg(test)]
#[test]
fn styled_span_cache_key_changes_with_revision_and_spans() {
    let a = EditorContentKey { revision: 1, highlight_rev: 5, semantic_rev: 2, theme_gen: 0 };
    let b = EditorContentKey { revision: 1, highlight_rev: 5, semantic_rev: 2, theme_gen: 0 };
    let c = EditorContentKey { revision: 2, highlight_rev: 5, semantic_rev: 2, theme_gen: 0 };
    assert_eq!(a, b);
    assert_ne!(a, c);
}
```

- [ ] **Step 2: Run, verify fail.**
- [ ] **Step 3: Implement** — add a `#[derive(PartialEq, Clone, Copy)] struct EditorContentKey {revision, highlight_rev, semantic_rev, theme_gen}` plus shell fields `cached_editor_content_key: Option<EditorContentKey>`, `cached_editor_text: String`, `cached_editor_styled_spans: Vec<StyledTextSpan>`. In the editor-content branch, build the current key; if it equals the cached key, reuse the cached `text`/`styled_spans` (skip `text_string()`/`overlay_highlight_layers`/`syntax_spans_to_styled`/`diagnostic_spans_to_styled`); else recompute and store. Pass the (cached or fresh) `&text`, `&styled_spans` into `update_editor_content`.

> If the existing code lacks `highlight_rev`/`theme_gen` counters, use the cheapest
> available change signal already tracked (e.g. `spans_fingerprint(&styled)` for the
> spans and a bool `theme_dirty`). The invariant: the key MUST change whenever
> `text`, syntax/semantic spans, diagnostics, or theme change. When unsure, fall
> back to recompute (correctness over caching) — but never key only on revision if
> diagnostics/semantic can change without bumping revision.

- [ ] **Step 4: Run, verify pass + full build.**

---

### Task 8: Full suite + verification

- [ ] **Step 1:** `cargo test 2>/dev/null` → all green (note pre-existing ignored tests).
- [ ] **Step 2:** `cargo build 2>/dev/null` → clean (no new warnings).
- [ ] **Step 3:** Update the design doc's acceptance section: check each criterion against a test or the manual smoke. Invoke `superpowers:verification-before-completion`.
- [ ] **Step 4:** Report to the user (no commit — human-only). Summarize behavior, tests, files.

---

## Self-Review

**Spec coverage:** one-clock model → Task 5; caret-from-anim → Tasks 4–6; visual-space
`auto_scroll_to_cursor` (fold jitter + jolt) → Task 3; styled-span caching → Task 7;
distance-scaled durations + far-clamp + retarget → Tasks 1,2,5; config + back-compat →
Task 2; snap for cross-file/zz-no-move → falls out of `plan_scroll_retarget` (unchanged)
+ caret delta 0; testing → each task. All §9 acceptance criteria map to a task.

**Placeholder scan:** the two "use the existing helper names if these differ" notes
(Tasks 3,5,7) are explicit fallbacks, not blanks — the engineer mirrors the named
existing tests. No TBD/TODO in code steps.

**Type consistency:** `caret_scroll_lag: f32` (app_state, read by renderer Task 6);
`caret_anim_start`/`scroll_anim_duration` (shell, Task 5); `cursor_visual_line() -> f32`
(Task 4, used in Task 5); `ease_fraction`/`scroll_duration_for_distance` (Task 1, used
in Tasks 2,5); `scroll_duration_for` (Task 2, used in Task 5). Names consistent across tasks.
