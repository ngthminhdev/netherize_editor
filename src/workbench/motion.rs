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
        assert_eq!(tr.sample(t0).model.find(RegionId::Center).unwrap().width, 100.0);
        assert!(!tr.is_done(t0));
        let end = t0 + Duration::from_millis(100);
        assert_eq!(tr.sample(end).model.find(RegionId::Center).unwrap().width, 60.0);
        assert!(tr.is_done(end));
    }

    #[test]
    fn lerp_layout_uses_target_shape_when_id_missing() {
        let mut from = layout(100.0);
        from.model.root.children.retain(|c| c.id == RegionId::Center);
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
    fn reveal_scrim_leads_content() {
        // The scrim dims well before the content appears (backdrop first).
        let t0 = Instant::now();
        let m = OverlayMotion::enter(t0, Duration::from_millis(100), EaseCurve::Linear);
        let s = m.reveal_sample(t0 + Duration::from_millis(55));
        assert!((s.scrim_alpha - 1.0).abs() < 1e-6); // scrim fully dimmed by 55%
        assert_eq!(s.content_alpha, 0.0); // content still hidden
    }
}
