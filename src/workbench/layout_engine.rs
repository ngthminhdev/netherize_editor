use winit::dpi::PhysicalSize;

use crate::{
    config::theme_config::UiThemeTokens,
    workbench::{
        panel_state::WorkbenchPanelState,
        region_model::{RegionBounds, RegionId, RegionModel, RegionNode},
    },
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SplitHandleId {
    LeftCenter,
    CenterRight,
    CenterBottom,
}

impl SplitHandleId {
    pub fn label(self) -> &'static str {
        match self {
            Self::LeftCenter => "left-center",
            Self::CenterRight => "center-right",
            Self::CenterBottom => "center-bottom",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SplitHandle {
    pub id: SplitHandleId,
    pub bounds: RegionBounds,
}

impl SplitHandle {
    pub fn contains(self, x: f32, y: f32) -> bool {
        x >= self.bounds.x
            && x <= self.bounds.x + self.bounds.width
            && y >= self.bounds.y
            && y <= self.bounds.y + self.bounds.height
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct WorkbenchLayout {
    pub model: RegionModel,
    pub handles: Vec<SplitHandle>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WorkbenchLayoutConfig {
    pub region_gap: f32,
    pub top_bar_height: f32,
    pub status_bar_height: f32,
    pub center_min_width: f32,
    pub center_min_height: f32,
    pub sidebar_min_width: f32,
    pub bottom_min_height: f32,
}

impl Default for WorkbenchLayoutConfig {
    fn default() -> Self {
        Self {
            region_gap: 4.0,
            top_bar_height: 32.0,
            status_bar_height: 20.0,
            center_min_width: 260.0,
            center_min_height: 140.0,
            sidebar_min_width: 140.0,
            bottom_min_height: 100.0,
        }
    }
}

impl WorkbenchLayoutConfig {
    /// Build a layout config from the themed `[ui]` block so chrome sizes are
    /// data-driven instead of baked into Rust constants.
    pub fn from_ui_theme(ui: &UiThemeTokens) -> Self {
        let defaults = Self::default();
        Self {
            region_gap: defaults.region_gap,
            top_bar_height: ui.top_bar_height,
            status_bar_height: ui.status_bar_height,
            center_min_width: defaults.center_min_width,
            center_min_height: defaults.center_min_height,
            // Allow shrinking below the configured sidebar_width, but not
            // below a usability floor.
            sidebar_min_width: defaults.sidebar_min_width,
            bottom_min_height: defaults.bottom_min_height,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct WorkbenchLayoutEngine {
    pub config: WorkbenchLayoutConfig,
}

impl WorkbenchLayoutEngine {
    pub fn new(config: WorkbenchLayoutConfig) -> Self {
        Self { config }
    }

    pub fn compute(
        &self,
        size: PhysicalSize<u32>,
        panels: &WorkbenchPanelState,
    ) -> WorkbenchLayout {
        let width = size.width as f32;
        let height = size.height as f32;
        let root_bounds = RegionBounds::new(0.0, 0.0, width, height);

        let top_h = self.config.top_bar_height.min(height.max(0.0));
        let remain_after_top = (height - top_h).max(0.0);
        let status_h = self.config.status_bar_height.min(remain_after_top.max(0.0));
        let body_y = top_h;
        let body_h = (height - top_h - status_h).max(0.0);

        let (center_h, bottom_h, vertical_gap) =
            self.compute_vertical_split(body_h, panels.bottom.visible, panels.bottom.size_px);

        let (left_w, right_w, center_w, left_gap, right_gap) = self.compute_horizontal_split(
            width,
            panels.left.visible,
            panels.left.size_px,
            panels.right.visible,
            panels.right.size_px,
        );

        let center_x = left_w;
        let center_y = body_y;
        // Keep the right sidebar hard-aligned to the viewport edge so we
        // don't leave a 1px seam/sliver due to floating-point rounding.
        let right_x = (width - right_w).max(0.0);
        let bottom_y = center_y + center_h + vertical_gap;

        let top_bar = RegionNode::new(
            RegionId::TopBar,
            RegionBounds::new(0.0, 0.0, width, top_h),
            true,
        );
        let left_sidebar = RegionNode::new(
            RegionId::LeftSidebar,
            RegionBounds::new(0.0, center_y, left_w, body_h),
            panels.left.visible && left_w > 0.0 && body_h > 0.0,
        );
        let center = RegionNode::new(
            RegionId::Center,
            RegionBounds::new(center_x, center_y, center_w, center_h),
            center_w > 0.0 && center_h > 0.0,
        );
        let right_sidebar = RegionNode::new(
            RegionId::RightSidebar,
            RegionBounds::new(right_x, center_y, right_w, body_h),
            panels.right.visible && right_w > 0.0 && body_h > 0.0,
        );
        let bottom_panel = RegionNode::new(
            RegionId::BottomPanel,
            RegionBounds::new(center_x, bottom_y, center_w, bottom_h),
            panels.bottom.visible && center_w > 0.0 && bottom_h > 0.0,
        );
        let status_bar = RegionNode::new(
            RegionId::StatusBar,
            RegionBounds::new(0.0, body_y + body_h, width, status_h),
            true,
        );
        let overlay_layer = RegionNode::new(
            RegionId::OverlayLayer,
            RegionBounds::new(0.0, 0.0, width, height),
            panels.overlay_visible,
        );
        let model = RegionModel::new(root_bounds).with_children(vec![
            top_bar,
            left_sidebar,
            center,
            right_sidebar,
            bottom_panel,
            status_bar,
            overlay_layer,
        ]);

        let mut handles = Vec::new();
        if panels.left.visible && left_w > 0.0 && left_gap > 0.0 && center_h > 0.0 {
            let handle_x = (left_w - left_gap * 0.5).clamp(0.0, (width - left_gap).max(0.0));
            handles.push(SplitHandle {
                id: SplitHandleId::LeftCenter,
                bounds: RegionBounds::new(handle_x, center_y, left_gap, center_h),
            });
        }
        if panels.right.visible && right_w > 0.0 && right_gap > 0.0 && center_h > 0.0 {
            let handle_x = (right_x - right_gap * 0.5).clamp(0.0, (width - right_gap).max(0.0));
            handles.push(SplitHandle {
                id: SplitHandleId::CenterRight,
                bounds: RegionBounds::new(handle_x, center_y, right_gap, center_h),
            });
        }
        if panels.bottom.visible && bottom_h > 0.0 && vertical_gap > 0.0 {
            handles.push(SplitHandle {
                id: SplitHandleId::CenterBottom,
                bounds: RegionBounds::new(center_x, center_y + center_h, center_w, vertical_gap),
            });
        }

        WorkbenchLayout { model, handles }
    }

    pub fn apply_handle_drag(
        &self,
        size: PhysicalSize<u32>,
        panels: &mut WorkbenchPanelState,
        handle: SplitHandleId,
        delta_x: f32,
        delta_y: f32,
    ) -> bool {
        let width = size.width as f32;
        let height = size.height as f32;
        let body_h = (height - self.config.top_bar_height - self.config.status_bar_height).max(0.0);
        let gap = self.config.region_gap.max(0.0);

        match handle {
            SplitHandleId::LeftCenter => {
                if !panels.left.visible {
                    return false;
                }
                let right_w = if panels.right.visible {
                    panels.right.size_px.max(self.config.sidebar_min_width)
                } else {
                    0.0
                };
                let max_left = (width - right_w - self.config.center_min_width)
                    .max(self.config.sidebar_min_width);
                let min_left = self.config.sidebar_min_width.min(max_left);
                let next = (panels.left.size_px + delta_x).clamp(min_left, max_left);
                if (next - panels.left.size_px).abs() < f32::EPSILON {
                    return false;
                }
                panels.left.size_px = next;
                true
            }
            SplitHandleId::CenterRight => {
                if !panels.right.visible {
                    return false;
                }
                let left_w = if panels.left.visible {
                    panels.left.size_px.max(self.config.sidebar_min_width)
                } else {
                    0.0
                };
                let max_right = (width - left_w - self.config.center_min_width)
                    .max(self.config.sidebar_min_width);
                let min_right = self.config.sidebar_min_width.min(max_right);
                let next = (panels.right.size_px - delta_x).clamp(min_right, max_right);
                if (next - panels.right.size_px).abs() < f32::EPSILON {
                    return false;
                }
                panels.right.size_px = next;
                true
            }
            SplitHandleId::CenterBottom => {
                if !panels.bottom.visible {
                    return false;
                }
                let max_bottom = (body_h - gap - self.config.center_min_height)
                    .max(self.config.bottom_min_height);
                let min_bottom = self.config.bottom_min_height.min(max_bottom);
                let next = (panels.bottom.size_px - delta_y).clamp(min_bottom, max_bottom);
                if (next - panels.bottom.size_px).abs() < f32::EPSILON {
                    return false;
                }
                panels.bottom.size_px = next;
                true
            }
        }
    }

    fn compute_vertical_split(
        &self,
        body_h: f32,
        bottom_visible: bool,
        bottom_size_px: f32,
    ) -> (f32, f32, f32) {
        if !bottom_visible || body_h <= 0.0 {
            return (body_h.max(0.0), 0.0, 0.0);
        }
        let gap = self.config.region_gap.max(0.0);
        let max_bottom = (body_h - self.config.center_min_height - gap).max(0.0);
        if max_bottom <= 0.0 {
            return (body_h.max(0.0), 0.0, 0.0);
        }

        let min_bottom = self.config.bottom_min_height.min(max_bottom);
        let bottom_h = bottom_size_px.clamp(min_bottom, max_bottom);
        let center_h = (body_h - bottom_h - gap).max(0.0);
        (center_h, bottom_h, gap)
    }

    fn compute_horizontal_split(
        &self,
        width: f32,
        left_visible: bool,
        left_size_px: f32,
        right_visible: bool,
        right_size_px: f32,
    ) -> (f32, f32, f32, f32, f32) {
        if width <= 0.0 {
            return (0.0, 0.0, 0.0, 0.0, 0.0);
        }

        let gap = self.config.region_gap.max(0.0);
        let left_gap = if left_visible { gap } else { 0.0 };
        let right_gap = if right_visible { gap } else { 0.0 };

        let left_target = if left_visible {
            left_size_px.max(self.config.sidebar_min_width)
        } else {
            0.0
        };
        let right_target = if right_visible {
            right_size_px.max(self.config.sidebar_min_width)
        } else {
            0.0
        };

        let side_total = left_target + right_target;
        let max_side_total = (width - self.config.center_min_width).max(0.0);

        let (left_w, right_w) = if side_total <= max_side_total || side_total <= 0.0 {
            (left_target, right_target)
        } else {
            let scale = max_side_total / side_total;
            (left_target * scale, right_target * scale)
        };

        // Snap split coordinates to pixel boundaries to avoid sub-pixel seams
        // (visible as a 1px "khuyet" strip on the right edge on some scales).
        let left_w = left_w.round();
        let right_w = right_w.round();

        // Horizontal split intentionally has no visual gap between sidebars
        // and center so center.width stays exact:
        // center.width = window.width - left.width - right.width.
        let center_w = (width - left_w - right_w).max(0.0);
        (left_w, right_w, center_w, left_gap, right_gap)
    }
}

#[cfg(test)]
mod tests {
    use super::{SplitHandleId, WorkbenchLayoutConfig, WorkbenchLayoutEngine};
    use crate::workbench::{panel_state::WorkbenchPanelState, region_model::RegionId};
    use winit::dpi::PhysicalSize;

    #[test]
    fn computes_core_regions_and_handles() {
        let engine = WorkbenchLayoutEngine::new(WorkbenchLayoutConfig::default());
        let state = WorkbenchPanelState::default();
        let layout = engine.compute(PhysicalSize::new(1280, 800), &state);

        assert!(layout.model.find(RegionId::TopBar).is_some());
        assert!(layout.model.find(RegionId::Center).is_some());
        assert!(!layout.handles.is_empty());
    }

    #[test]
    fn dragging_left_handle_changes_left_sidebar_width() {
        let engine = WorkbenchLayoutEngine::new(WorkbenchLayoutConfig::default());
        let size = PhysicalSize::new(1280, 800);
        let mut state = WorkbenchPanelState::default();
        let before = state.left.size_px;

        let changed =
            engine.apply_handle_drag(size, &mut state, SplitHandleId::LeftCenter, 30.0, 0.0);
        assert!(changed);
        assert!(state.left.size_px > before);
    }

    #[test]
    fn resize_keeps_center_and_bottom_non_overlapping() {
        let engine = WorkbenchLayoutEngine::new(WorkbenchLayoutConfig::default());
        let state = WorkbenchPanelState::default();

        let small = engine.compute(PhysicalSize::new(900, 600), &state);
        let large = engine.compute(PhysicalSize::new(1600, 1000), &state);

        let small_center = small.model.find(RegionId::Center).expect("center region");
        let small_bottom = small
            .model
            .find(RegionId::BottomPanel)
            .expect("bottom region");
        let large_center = large.model.find(RegionId::Center).expect("center region");
        let large_bottom = large
            .model
            .find(RegionId::BottomPanel)
            .expect("bottom region");

        assert!(small_center.y + small_center.height <= small_bottom.y + 0.001);
        assert!(large_center.y + large_center.height <= large_bottom.y + 0.001);
        assert!(large_center.width > small_center.width);
        assert!(large_center.height > small_center.height);
    }

    #[test]
    fn right_sidebar_stays_flush_with_viewport_edge() {
        let engine = WorkbenchLayoutEngine::new(WorkbenchLayoutConfig::default());
        let mut state = WorkbenchPanelState::default();
        state.right.visible = true;
        state.right.size_px = 333.3;
        state.left.size_px = 241.7;

        let layout = engine.compute(PhysicalSize::new(1280, 800), &state);
        let right = layout
            .model
            .find(RegionId::RightSidebar)
            .expect("right sidebar region");

        let viewport_w = 1280.0;
        let right_edge = right.x + right.width;
        assert!(
            (right_edge - viewport_w).abs() <= 0.001,
            "right sidebar should be flush: right_edge={right_edge}, viewport_w={viewport_w}"
        );
    }

    #[test]
    fn center_width_equals_window_minus_sidebars_when_right_visible() {
        let engine = WorkbenchLayoutEngine::new(WorkbenchLayoutConfig::default());
        let mut state = WorkbenchPanelState::default();
        state.left.visible = true;
        state.right.visible = true;
        state.left.size_px = 280.0;
        state.right.size_px = 320.0;

        let viewport_w = 1600.0;
        let layout = engine.compute(PhysicalSize::new(viewport_w as u32, 900), &state);

        let left = layout
            .model
            .find(RegionId::LeftSidebar)
            .expect("left sidebar region");
        let center = layout.model.find(RegionId::Center).expect("center region");
        let right = layout
            .model
            .find(RegionId::RightSidebar)
            .expect("right sidebar region");

        let expected = viewport_w - left.width - right.width;
        assert!(
            (center.width - expected).abs() <= 0.001,
            "center.width mismatch: center={} expected={expected}",
            center.width
        );
    }

    #[test]
    fn center_reaches_viewport_right_edge_when_right_sidebar_hidden() {
        let engine = WorkbenchLayoutEngine::new(WorkbenchLayoutConfig::default());
        let mut state = WorkbenchPanelState::default();
        state.left.visible = true;
        state.right.visible = false;
        state.left.size_px = 280.0;
        state.right.size_px = 320.0; // must be ignored while hidden

        let viewport_w = 1600.0;
        let layout = engine.compute(PhysicalSize::new(viewport_w as u32, 900), &state);

        let center = layout.model.find(RegionId::Center).expect("center region");
        let right_edge = center.x + center.width;
        assert!(
            (right_edge - viewport_w).abs() <= 0.001,
            "center should reach viewport edge: right_edge={right_edge}, viewport_w={viewport_w}"
        );
    }

    #[test]
    fn left_sidebar_spans_full_body_height_when_bottom_panel_is_visible() {
        let engine = WorkbenchLayoutEngine::new(WorkbenchLayoutConfig::default());
        let mut state = WorkbenchPanelState::default();
        state.bottom.visible = true;
        state.bottom.size_px = 220.0;

        let viewport = PhysicalSize::new(1280, 800);
        let layout = engine.compute(viewport, &state);

        let left = layout
            .model
            .find(RegionId::LeftSidebar)
            .expect("left sidebar region");
        let top = layout.model.find(RegionId::TopBar).expect("top bar region");
        let status = layout
            .model
            .find(RegionId::StatusBar)
            .expect("status bar region");
        let expected_body_height = viewport.height as f32 - top.height - status.height;

        assert!(
            (left.height - expected_body_height).abs() <= 0.001,
            "left sidebar should fill body height: left.height={} expected={expected_body_height}",
            left.height
        );
    }

    #[test]
    fn bottom_panel_starts_after_left_sidebar() {
        let engine = WorkbenchLayoutEngine::new(WorkbenchLayoutConfig::default());
        let mut state = WorkbenchPanelState::default();
        state.left.visible = true;
        state.left.size_px = 280.0;
        state.bottom.visible = true;
        state.bottom.size_px = 220.0;
        state.right.visible = false;

        let layout = engine.compute(PhysicalSize::new(1400, 900), &state);
        let left = layout
            .model
            .find(RegionId::LeftSidebar)
            .expect("left sidebar region");
        let center = layout.model.find(RegionId::Center).expect("center region");
        let bottom = layout
            .model
            .find(RegionId::BottomPanel)
            .expect("bottom panel region");

        assert!(
            (bottom.x - left.width).abs() <= 0.001,
            "bottom panel should start after left sidebar: bottom.x={} left.width={}",
            bottom.x,
            left.width
        );
        assert!(
            (bottom.width - center.width).abs() <= 0.001,
            "bottom panel width should match center pane: bottom.width={} center.width={}",
            bottom.width,
            center.width
        );
    }
}
