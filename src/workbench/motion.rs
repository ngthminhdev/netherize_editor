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

/// Clamp the *start* of a scroll tween so a very large jump never crawls: the
/// animation begins at most `max` units away from `target`, teleporting the far
/// part instantly and easing only the last `max` units (Neovide "far-lines").
/// `max <= 0` disables the clamp (the full distance animates).
#[inline]
pub fn clamp_scroll_start(current: f32, target: f32, max: f32) -> f32 {
    if max <= 0.0 {
        return current;
    }
    target + (current - target).clamp(-max, max)
}

/// Sample a fixed-duration ease-out scroll tween. Returns `(value, done)` where
/// `value` is the eased position between `start` and `target` and `done` is true
/// once the duration has fully elapsed (then `value == target`). Time-based, so
/// the motion is identical regardless of frame cadence. A zero `duration` snaps
/// to `target` immediately.
pub fn ease_scroll(
    start: f32,
    target: f32,
    started_at: Instant,
    now: Instant,
    duration: Duration,
    curve: EaseCurve,
) -> (f32, bool) {
    if duration.is_zero() {
        return (target, true);
    }
    let elapsed = now.saturating_duration_since(started_at).as_secs_f32();
    let t = (elapsed / duration.as_secs_f32()).clamp(0.0, 1.0);
    if t >= 1.0 {
        return (target, true);
    }
    let eased = curve.apply(t);
    (start + (target - start) * eased, false)
}

/// Shared eased progress for phase-locking two tweens (scroll + caret) on one
/// clock. Returns `(eased_fraction, done)` where `eased_fraction` is in `0..=1`
/// after the curve, and `done` is true once the duration has elapsed. A zero
/// duration yields `(1.0, true)` so callers snap instantly.
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

/// Pick a short, distance-scaled tween duration. `animated_lines` is the
/// post-clamp visual distance, so a far jump that clamped to a screenful uses the
/// short bucket, not a long cinematic one. Buckets: ≤3 lines → `step_ms`
/// (j/k edge follow), ≤24 lines → `halfpage_ms` (Ctrl-D/U), else `center_ms`.
#[inline]
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

/// Far-jump clamp width in lines: one screenful plus `far_lines` extra. Adapts
/// Neovide's `scroll_animation_far_lines` so a large jump still shows a visible
/// settle of ~one screenful (rather than near-snapping at the default). Never
/// zero, so `clamp_scroll_start` always leaves a finite span to animate.
#[inline]
pub fn scroll_far_clamp_lines(viewport_lines: usize, far_lines: u32) -> f32 {
    (viewport_lines as f32 + far_lines as f32).max(1.0)
}

/// How a scroll-target change should be applied to the tween.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ScrollRetarget {
    /// Jump instantly to target (cursor follow below threshold, or smooth off).
    Snap,
    /// Begin a tween from `start` (already far-clamped) toward target.
    Animate { start: f32 },
}

/// Decide snap vs animate for a scroll-target change. Pure so it is unit-testable
/// without an `AppShell`. `smooth_enabled` already folds in the global motion
/// gate, the editor toggle, and `animation_ms > 0`. `force` lets an explicit
/// command (zz/gg/G/Ctrl-D/Ctrl-U) animate even when `|delta| < snap_threshold`,
/// while ordinary cursor follow (`j`/`k`, `force = false`) snaps. `current` is the
/// live animated position, so a retarget mid-tween recomputes from where the
/// buffer is *now* — no one-frame jump.
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
    out.children = node.children.iter().map(|c| map_node(c, from, t)).collect();
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
        Self {
            from,
            to,
            started_at,
            duration,
            curve,
        }
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

/// Sample of the "Dot → Line → Panel Reveal" overlay motion. Factors are
/// normalized [0,1] against the panel's full size; the renderer maps them to
/// pixels (and clamps width/height to a minimum line thickness so the early
/// frames read as a dot/line rather than nothing).
///
/// Timeline (eased): a dot grows **horizontally** into a line (`width_factor`
/// 0→1) first, then the line unfolds **vertically** into the full panel
/// (`height_factor` 0→1); `content_alpha` fades the text/rows in only once the
/// panel is nearly open, and `scrim_alpha` dims the backdrop early.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RevealSample {
    pub width_factor: f32,
    pub height_factor: f32,
    pub content_alpha: f32,
    pub scrim_alpha: f32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct OverlayMotion {
    pub phase: OverlayPhase,
    pub started_at: Instant,
    pub duration: Duration,
    pub curve: EaseCurve,
}

impl OverlayMotion {
    /// Construct an Enter motion (Dot → Line → Panel Reveal) starting now.
    pub fn enter(started_at: Instant, duration: Duration, curve: EaseCurve) -> Self {
        Self {
            phase: OverlayPhase::Enter,
            started_at,
            duration,
            curve,
        }
    }

    fn progress(&self, now: Instant) -> f32 {
        if self.duration.is_zero() {
            return 1.0;
        }
        let elapsed = now.saturating_duration_since(self.started_at).as_secs_f32();
        (elapsed / self.duration.as_secs_f32()).clamp(0.0, 1.0)
    }

    pub fn is_done(&self, now: Instant) -> bool {
        self.progress(now) >= 1.0
    }

    /// Sample the Dot → Line → Panel Reveal motion. Over normalized progress `p`:
    /// the scrim dims **first and fast** (`[0, SCRIM_END]`) so the backdrop is set
    /// before anything is drawn; the dot then sweeps **horizontally** into a line
    /// over `[WIDTH_START, WIDTH_END]` using a **linear** ramp (no easing) so the
    /// stroke reads as a steady draw instead of an ease-out "pop" that finishes
    /// almost instantly; the line unfolds **vertically** into the panel over
    /// `[WIDTH_END, HEIGHT_END]`, and the content (text/rows/icons) fades in last
    /// over `[CONTENT_START, 1]`, once the panel is essentially open.
    pub fn reveal_sample(&self, now: Instant) -> RevealSample {
        const SCRIM_END: f32 = 0.2;
        const WIDTH_START: f32 = 0.12;
        const WIDTH_END: f32 = 0.66;
        const HEIGHT_END: f32 = 0.9;
        const CONTENT_START: f32 = 0.85;
        let raw = self.progress(now);
        let p = match self.phase {
            OverlayPhase::Enter => raw,
            OverlayPhase::Leave => 1.0 - raw,
        };
        let ramp = |from: f32, to: f32| ((p - from) / (to - from)).clamp(0.0, 1.0);
        RevealSample {
            // Linear (no curve) so the horizontal draw is perceptibly progressive
            // across its whole window rather than front-loaded by the ease-out.
            width_factor: ramp(WIDTH_START, WIDTH_END),
            height_factor: self.curve.apply(ramp(WIDTH_END, HEIGHT_END)),
            content_alpha: self.curve.apply(ramp(CONTENT_START, 1.0)),
            scrim_alpha: self.curve.apply(ramp(0.0, SCRIM_END)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workbench::layout_engine::WorkbenchLayout;
    use crate::workbench::region_model::{RegionBounds, RegionId, RegionModel, RegionNode};
    use std::time::{Duration, Instant};

    fn layout(center_w: f32) -> WorkbenchLayout {
        let children = vec![
            RegionNode::new(
                RegionId::LeftSidebar,
                RegionBounds::new(0.0, 0.0, 100.0 - center_w, 100.0),
                center_w < 100.0,
            ),
            RegionNode::new(
                RegionId::Center,
                RegionBounds::new(100.0 - center_w, 0.0, center_w, 100.0),
                true,
            ),
        ];
        WorkbenchLayout {
            model: RegionModel::new(RegionBounds::new(0.0, 0.0, 100.0, 100.0))
                .with_children(children),
            handles: vec![],
        }
    }

    #[test]
    fn ease_out_cubic_clamps_and_endpoints() {
        assert_eq!(EaseCurve::EaseOutCubic.apply(-1.0), 0.0);
        assert_eq!(EaseCurve::EaseOutCubic.apply(2.0), 1.0);
        assert!((EaseCurve::EaseOutCubic.apply(0.0) - 0.0).abs() < 1e-6);
        assert!((EaseCurve::EaseOutCubic.apply(1.0) - 1.0).abs() < 1e-6);
        assert!(EaseCurve::EaseOutCubic.apply(0.5) > 0.5);
    }

    #[test]
    fn ease_fraction_zero_duration_is_done_at_one() {
        let t0 = Instant::now();
        let (f, done) = ease_fraction(t0, t0, Duration::ZERO, EaseCurve::EaseOutCubic);
        assert_eq!(f, 1.0);
        assert!(done);
    }

    #[test]
    fn ease_fraction_matches_ease_scroll_value() {
        // The shared fraction, lerped between the same endpoints, must reproduce
        // ease_scroll's value — so scroll and caret can phase-lock on one clock.
        let t0 = Instant::now();
        let now = t0 + Duration::from_millis(40);
        let dur = Duration::from_millis(120);
        let (frac, done) = ease_fraction(t0, now, dur, EaseCurve::EaseOutCubic);
        assert!(!done);
        let (val, _) = ease_scroll(10.0, 30.0, t0, now, dur, EaseCurve::EaseOutCubic);
        assert!((10.0 + (30.0 - 10.0) * frac - val).abs() < 1e-4);
    }

    #[test]
    fn duration_scales_with_distance() {
        assert_eq!(
            scroll_duration_for_distance(2.0, 80, 120, 130),
            Duration::from_millis(80)
        );
        assert_eq!(
            scroll_duration_for_distance(20.0, 80, 120, 130),
            Duration::from_millis(120)
        );
        assert_eq!(
            scroll_duration_for_distance(200.0, 80, 120, 130),
            Duration::from_millis(130)
        );
        // Sign-independent: a large upward jump uses the same bucket as downward.
        assert_eq!(
            scroll_duration_for_distance(-200.0, 80, 120, 130),
            Duration::from_millis(130)
        );
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
        let from = layout(100.0);
        let to = layout(60.0);
        let t0 = Instant::now();
        let tr = LayoutTransition::new(
            from.clone(),
            to.clone(),
            t0,
            Duration::from_millis(100),
            EaseCurve::Linear,
        );
        assert_eq!(
            tr.sample(t0).model.find(RegionId::Center).unwrap().width,
            100.0
        );
        assert!(!tr.is_done(t0));
        let end = t0 + Duration::from_millis(100);
        assert_eq!(
            tr.sample(end).model.find(RegionId::Center).unwrap().width,
            60.0
        );
        assert!(tr.is_done(end));
    }

    #[test]
    fn lerp_layout_uses_target_shape_when_id_missing() {
        let mut from = layout(100.0);
        from.model
            .root
            .children
            .retain(|c| c.id == RegionId::Center);
        let to = layout(60.0);
        let mid = lerp_layout(&from, &to, 0.5);
        assert_eq!(mid.model.find(RegionId::LeftSidebar).unwrap().width, 40.0);
        assert_eq!(mid.model.find(RegionId::Center).unwrap().width, 80.0);
    }

    #[test]
    fn reveal_dot_line_panel_phasing() {
        let t0 = Instant::now();
        let m = OverlayMotion::enter(t0, Duration::from_millis(100), EaseCurve::Linear);
        // t=0: a dot — nothing has grown or appeared yet.
        let s0 = m.reveal_sample(t0);
        assert!(s0.width_factor < 0.01);
        assert!(s0.height_factor < 0.01);
        assert!(s0.content_alpha < 0.01);
        // Mid-width phase (p=0.5, before WIDTH_END=0.66): width is growing, height
        // is still a flat line, content still hidden.
        let mid = m.reveal_sample(t0 + Duration::from_millis(50));
        assert!(mid.width_factor > 0.5);
        assert_eq!(mid.height_factor, 0.0);
        assert_eq!(mid.content_alpha, 0.0);
        // End: fully revealed.
        let end = m.reveal_sample(t0 + Duration::from_millis(100));
        assert!((end.width_factor - 1.0).abs() < 1e-6);
        assert!((end.height_factor - 1.0).abs() < 1e-6);
        assert!((end.content_alpha - 1.0).abs() < 1e-6);
        assert!((end.scrim_alpha - 1.0).abs() < 1e-6);
    }

    #[test]
    fn ease_scroll_endpoints_and_done() {
        let t0 = Instant::now();
        let dur = Duration::from_millis(140);
        // t=0 → exactly start, not done.
        let (v0, d0) = ease_scroll(10.0, 50.0, t0, t0, dur, EaseCurve::EaseOutCubic);
        assert!((v0 - 10.0).abs() < 1e-4);
        assert!(!d0);
        // t≥duration → snapped to target, done.
        let (v1, d1) = ease_scroll(10.0, 50.0, t0, t0 + dur, dur, EaseCurve::EaseOutCubic);
        assert!((v1 - 50.0).abs() < 1e-4);
        assert!(d1);
        // Past the end stays done at target.
        let (v2, d2) = ease_scroll(10.0, 50.0, t0, t0 + dur * 2, dur, EaseCurve::EaseOutCubic);
        assert!((v2 - 50.0).abs() < 1e-4);
        assert!(d2);
    }

    #[test]
    fn ease_scroll_is_monotonic_ease_out() {
        let t0 = Instant::now();
        let dur = Duration::from_millis(100);
        let sample = |ms: u64| {
            ease_scroll(
                0.0,
                100.0,
                t0,
                t0 + Duration::from_millis(ms),
                dur,
                EaseCurve::EaseOutCubic,
            )
            .0
        };
        let a = sample(10);
        let b = sample(50);
        let c = sample(90);
        // Strictly increasing toward target…
        assert!(a < b && b < c && c < 100.0);
        // …and front-loaded (ease-out): more than half the distance covered by the
        // midpoint.
        assert!(b > 50.0);
    }

    #[test]
    fn ease_scroll_zero_duration_snaps() {
        let t0 = Instant::now();
        let (v, done) = ease_scroll(0.0, 99.0, t0, t0, Duration::ZERO, EaseCurve::EaseOutCubic);
        assert_eq!(v, 99.0);
        assert!(done);
    }

    #[test]
    fn scroll_far_clamp_lines_is_screenful_plus_far() {
        assert_eq!(scroll_far_clamp_lines(40, 1), 41.0);
        assert_eq!(scroll_far_clamp_lines(40, 9), 49.0);
        // Never zero, even with a zero viewport / far.
        assert_eq!(scroll_far_clamp_lines(0, 0), 1.0);
    }

    #[test]
    fn plan_retarget_snaps_when_smooth_disabled() {
        // Even an explicit (forced) far command snaps when smooth scroll is off.
        assert_eq!(
            plan_scroll_retarget(0.0, 50.0, false, true, 1.5, 40, 1),
            ScrollRetarget::Snap
        );
    }

    #[test]
    fn plan_retarget_snaps_sub_line_jitter_without_force() {
        // Sub-line move below the 0.5 floor, no force → instant snap.
        assert_eq!(
            plan_scroll_retarget(10.0, 10.3, true, false, 0.5, 40, 1),
            ScrollRetarget::Snap
        );
    }

    #[test]
    fn plan_retarget_animates_single_line_follow_without_force() {
        // A whole-line j/k follow at the viewport edge glides, not snaps.
        match plan_scroll_retarget(10.0, 11.0, true, false, 0.5, 40, 1) {
            ScrollRetarget::Animate { start } => assert!((start - 10.0).abs() < f32::EPSILON),
            other => panic!("expected animate, got {other:?}"),
        }
    }

    #[test]
    fn plan_retarget_animates_sub_line_when_forced() {
        // Explicit command with a tiny delta still animates (force bypasses floor).
        match plan_scroll_retarget(10.0, 10.3, true, true, 0.5, 40, 1) {
            ScrollRetarget::Animate { start } => assert!((start - 10.0).abs() < f32::EPSILON),
            other => panic!("expected animate, got {other:?}"),
        }
    }

    #[test]
    fn plan_retarget_clamps_far_jump_to_screenful() {
        // gg from line 5000 to 0, viewport 40, far 1 → start within 41 lines of target.
        match plan_scroll_retarget(5000.0, 0.0, true, true, 1.5, 40, 1) {
            ScrollRetarget::Animate { start } => assert_eq!(start, 41.0),
            other => panic!("expected animate, got {other:?}"),
        }
    }

    #[test]
    fn plan_retarget_recomputes_from_current_position() {
        // Mid-tween at line 60, new target 0 → start clamped from 60, not the old start.
        match plan_scroll_retarget(60.0, 0.0, true, true, 1.5, 40, 1) {
            ScrollRetarget::Animate { start } => assert_eq!(start, 41.0),
            other => panic!("expected animate, got {other:?}"),
        }
    }

    #[test]
    fn clamp_scroll_start_far_jump_and_passthrough() {
        // Huge downward jump: start is pulled to within `max` above the target.
        assert_eq!(clamp_scroll_start(0.0, 1000.0, 50.0), 950.0);
        // Huge upward jump: within `max` below the target.
        assert_eq!(clamp_scroll_start(1000.0, 0.0, 50.0), 50.0);
        // Small jump within `max` is untouched.
        assert_eq!(clamp_scroll_start(40.0, 50.0, 50.0), 40.0);
        // max <= 0 disables the clamp.
        assert_eq!(clamp_scroll_start(0.0, 1000.0, 0.0), 0.0);
    }

    #[test]
    fn reveal_scrim_leads_content() {
        // The scrim dims well before the content appears (backdrop first).
        let t0 = Instant::now();
        let m = OverlayMotion::enter(t0, Duration::from_millis(100), EaseCurve::Linear);
        let s = m.reveal_sample(t0 + Duration::from_millis(55));
        assert!((s.scrim_alpha - 1.0).abs() < 1e-6); // scrim fully dimmed by 55%
        assert_eq!(s.content_alpha, 0.0); // content still hidden
    }
}
