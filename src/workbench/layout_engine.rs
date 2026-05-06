use winit::dpi::PhysicalSize;

use crate::{
    config::theme_config::UiThemeTokens,
    workbench::{
        focus_manager::FocusTarget,
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
    pub outer_gap: f32,
    pub panel_gap: f32,
    pub inner_padding: f32,
    pub round_ui: bool,
    pub top_bar_height: f32,
    pub status_bar_height: f32,
    pub center_min_width: f32,
    pub center_min_height: f32,
    pub sidebar_min_width: f32,
    pub bottom_min_height: f32,
    pub panel_border_width: f32,
    pub chat_input_height: f32,
}

impl Default for WorkbenchLayoutConfig {
    fn default() -> Self {
        Self {
            outer_gap: 10.0,
            panel_gap: 8.0,
            inner_padding: 12.0,
            round_ui: true,
            top_bar_height: 32.0,
            status_bar_height: 20.0,
            center_min_width: 260.0,
            center_min_height: 140.0,
            sidebar_min_width: 140.0,
            bottom_min_height: 100.0,
            panel_border_width: 1.0,
            chat_input_height: 120.0,
        }
    }
}

impl WorkbenchLayoutConfig {
    /// Build a layout config from the themed `[ui]` block so chrome sizes are
    /// data-driven instead of baked into Rust constants.
    pub fn from_ui_theme(ui: &UiThemeTokens) -> Self {
        let defaults = Self::default();
        Self {
            outer_gap: defaults.outer_gap,
            panel_gap: defaults.panel_gap,
            inner_padding: defaults.inner_padding,
            round_ui: defaults.round_ui,
            top_bar_height: ui.top_bar_height,
            status_bar_height: ui.status_bar_height,
            center_min_width: defaults.center_min_width,
            center_min_height: defaults.center_min_height,
            // Allow shrinking below the configured sidebar_width, but not
            // below a usability floor.
            sidebar_min_width: defaults.sidebar_min_width,
            bottom_min_height: defaults.bottom_min_height,
            panel_border_width: defaults.panel_border_width,
            chat_input_height: defaults.chat_input_height,
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

    fn status_bar_top_gap(&self, status_h: f32) -> f32 {
        if status_h <= 0.0 {
            return 0.0;
        }
        self.config.panel_gap.max(0.0).min(status_h * 0.5)
    }

    pub fn compute(
        &self,
        size: PhysicalSize<u32>,
        panels: &WorkbenchPanelState,
    ) -> WorkbenchLayout {
        let width = size.width as f32;
        let height = size.height as f32;
        let root_bounds = RegionBounds::new(0.0, 0.0, width, height);

        // Zen Mode: maximize one region
        if let Some(maximized) = panels.maximized_region {
            return self.compute_maximized(size, panels, maximized);
        }

        let outer_gap = self.config.outer_gap.max(0.0);
        let gap = self.config.panel_gap.max(0.0);
        let viewport_bounds = RegionBounds::new(
            outer_gap.min(width * 0.5),
            outer_gap.min(height * 0.5),
            (width - outer_gap * 2.0).max(0.0),
            (height - outer_gap * 2.0).max(0.0),
        );

        let top_h = self
            .config
            .top_bar_height
            .min((height - gap).max(0.0));
        let remain_after_top = (height - gap - top_h).max(0.0);
        let status_h = self.config.status_bar_height.min(remain_after_top);
        let status_top_gap = self
            .status_bar_top_gap(status_h)
            .min(remain_after_top - status_h);
        let body_y = top_h + gap;
        let body_h = (height - gap - top_h - status_h - status_top_gap).max(0.0);

        let (center_h, bottom_h, vertical_gap) =
            self.compute_vertical_split(body_h, panels.bottom.visible, panels.bottom.size_px);

        let (left_w, right_w, center_w, left_gap, right_gap) = self.compute_horizontal_split(
            viewport_bounds.width,
            panels.left.visible,
            panels.left.size_px,
            panels.right.visible,
            panels.right.size_px,
        );

        let left_x = viewport_bounds.x;
        let center_inset_left = if panels.left.visible {
            left_gap
        } else {
            self.config.panel_gap.max(0.0)
        };
        let center_x = viewport_bounds.x + left_w + center_inset_left;
        let center_y = body_y;
        let right_x = viewport_bounds.x + viewport_bounds.width - right_w;
        let bottom_y = center_y + center_h + vertical_gap;

        let top_bar = RegionNode::new(
            RegionId::TopBar,
            RegionBounds::new(0.0, 0.0, width, top_h),
            true,
        );
        let left_sidebar = RegionNode::new(
            RegionId::LeftSidebar,
            RegionBounds::new(left_x, center_y, left_w, body_h),
            panels.left.visible && left_w > 0.0 && body_h > 0.0,
        );
        let center = RegionNode::new(
            RegionId::Center,
            RegionBounds::new(center_x, center_y, center_w, center_h),
            center_w > 0.0 && center_h > 0.0,
        );
        let right_sidebar = if panels.right.visible && right_w > 0.0 && body_h > 0.0 {
            let rs_rect = RegionBounds::new(right_x, center_y, right_w, body_h);

            // Split into AI Chat sub-regions using inner_padding only
            let pad = self.config.inner_padding;
            let available = (rs_rect.height - pad * 2.0).max(0.0);
            let chat_input_h = self.config.chat_input_height.min(available * 0.5);
            let history_h = (available - chat_input_h - pad).max(0.0);

            let history_rect = RegionBounds::new(
                rs_rect.x + pad,
                rs_rect.y + pad,
                (rs_rect.width - pad * 2.0).max(0.0),
                history_h,
            );
            let input_rect = RegionBounds::new(
                rs_rect.x + pad,
                rs_rect.y + pad + history_h + pad,
                (rs_rect.width - pad * 2.0).max(0.0),
                chat_input_h,
            );

            let history_node = RegionNode::new(RegionId::AiChatHistory, history_rect, true);
            let input_node = RegionNode::new(RegionId::AiChatInput, input_rect, true);

            RegionNode::new(RegionId::RightSidebar, rs_rect, true)
                .with_children(vec![history_node, input_node])
        } else {
            RegionNode::new(
                RegionId::RightSidebar,
                RegionBounds::new(right_x, center_y, right_w, body_h),
                false,
            )
        };
        let bottom_panel = RegionNode::new(
            RegionId::BottomPanel,
            RegionBounds::new(center_x, bottom_y, center_w, bottom_h),
            panels.bottom.visible && center_w > 0.0 && bottom_h > 0.0,
        );
        let status_inset_x = self.config.panel_gap.max(0.0);
        let status_bar = RegionNode::new(
            RegionId::StatusBar,
            RegionBounds::new(
                viewport_bounds.x + status_inset_x,
                (height - status_h).max(0.0),
                (viewport_bounds.width - status_inset_x * 2.0).max(0.0),
                status_h,
            ),
            true,
        );
        let overlay_layer = RegionNode::new(
            RegionId::OverlayLayer,
            RegionBounds::new(
                viewport_bounds.x,
                viewport_bounds.y,
                viewport_bounds.width,
                viewport_bounds.height,
            ),
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
            let handle_x = (left_x + left_w).clamp(0.0, (width - left_gap).max(0.0));
            handles.push(SplitHandle {
                id: SplitHandleId::LeftCenter,
                bounds: RegionBounds::new(handle_x, center_y, left_gap, center_h),
            });
        }
        if panels.right.visible && right_w > 0.0 && right_gap > 0.0 && center_h > 0.0 {
            let handle_x = (right_x - right_gap).clamp(0.0, (width - right_gap).max(0.0));
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

    fn compute_maximized(
        &self,
        size: PhysicalSize<u32>,
        _panels: &WorkbenchPanelState,
        target: FocusTarget,
    ) -> WorkbenchLayout {
        let width = size.width as f32;
        let height = size.height as f32;
        let outer_gap = self.config.outer_gap.max(0.0);
        let gap = self.config.panel_gap.max(0.0);

        // Keep TopBar and StatusBar, give everything else to target
        let top_h = self
            .config
            .top_bar_height
            .min((height - gap).max(0.0));
        let remain_after_top = (height - gap - top_h).max(0.0);
        let status_h = self.config.status_bar_height.min(remain_after_top);
        let status_top_gap = self
            .status_bar_top_gap(status_h)
            .min(remain_after_top - status_h);

        let available_y = top_h + gap;
        let available_h = (height - gap - top_h - status_h - status_top_gap).max(0.0);
        let available_x = outer_gap;
        let available_w = (width - outer_gap * 2.0).max(0.0);

        let target_bounds = RegionBounds::new(available_x, available_y, available_w, available_h);
        let zero = RegionBounds::new(0.0, 0.0, 0.0, 0.0);

        let top_bar = RegionNode::new(
            RegionId::TopBar,
            RegionBounds::new(0.0, 0.0, width, top_h),
            true,
        );

        let left = RegionNode::new(
            RegionId::LeftSidebar,
            if target == FocusTarget::LeftSidebar {
                target_bounds
            } else {
                zero
            },
            target == FocusTarget::LeftSidebar,
        );
        let center = RegionNode::new(
            RegionId::Center,
            if target == FocusTarget::CenterEditor {
                target_bounds
            } else {
                zero
            },
            target == FocusTarget::CenterEditor,
        );
        let right = RegionNode::new(
            RegionId::RightSidebar,
            if target == FocusTarget::RightSidebar {
                target_bounds
            } else {
                zero
            },
            target == FocusTarget::RightSidebar,
        );
        let bottom = RegionNode::new(
            RegionId::BottomPanel,
            if target == FocusTarget::BottomPanel {
                target_bounds
            } else {
                zero
            },
            target == FocusTarget::BottomPanel,
        );

        let status_inset_x = self.config.panel_gap.max(0.0);
        let status_bar = RegionNode::new(
            RegionId::StatusBar,
            RegionBounds::new(
                outer_gap + status_inset_x,
                (height - status_h).max(0.0),
                ((width - outer_gap * 2.0) - status_inset_x * 2.0).max(0.0),
                status_h,
            ),
            true,
        );

        let root_bounds = RegionBounds::new(0.0, 0.0, width, height);
        WorkbenchLayout {
            model: RegionModel::new(root_bounds).with_children(vec![
                top_bar,
                left,
                center,
                right,
                bottom,
                status_bar,
            ]),
            handles: vec![],
        }
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
        let outer_gap = self.config.outer_gap.max(0.0);
        let usable_width = (width - outer_gap * 2.0).max(0.0);
        let gap = self.config.panel_gap.max(0.0);
        let body_h = (height
            - gap
            - self.config.top_bar_height
            - self.config.status_bar_height
            - self.status_bar_top_gap(self.config.status_bar_height))
        .max(0.0);
        let gap = self.config.panel_gap.max(0.0);

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
                let right_gap = if panels.right.visible { gap } else { 0.0 };
                let max_left =
                    (usable_width - right_w - right_gap - gap - self.config.center_min_width)
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
                let left_gap = if panels.left.visible { gap } else { 0.0 };
                let max_right =
                    (usable_width - left_w - left_gap - gap - self.config.center_min_width)
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
        let gap = self.config.panel_gap.max(0.0);
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

        let gap = self.config.panel_gap.max(0.0);
        let left_gap = gap;
        let right_gap = gap;

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
        let gap_total = left_gap + right_gap;
        let max_side_total = (width - self.config.center_min_width - gap_total).max(0.0);

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

        let center_w = (width - left_w - right_w - gap_total).max(0.0);
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

        // Right sidebar should use full allocated bounds (no floating panel_gap inset),
        // matching how left sidebar works.
        let viewport_w = 1280.0;
        let right_edge = right.x + right.width;
        let expected_edge = viewport_w - engine.config.outer_gap;
        assert!(
            (right_edge - expected_edge).abs() <= 0.001,
            "right sidebar should be flush with viewport edge: right_edge={right_edge}, expected_edge={expected_edge}"
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

        let expected = viewport_w
            - engine.config.outer_gap * 2.0
            - left.width
            - right.width
            - engine.config.panel_gap * 2.0;
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

        let left = layout
            .model
            .find(RegionId::LeftSidebar)
            .expect("left sidebar region");
        let center = layout.model.find(RegionId::Center).expect("center region");
        let right_edge = center.x + center.width;
        let expected_edge = viewport_w - engine.config.outer_gap - engine.config.panel_gap;
        assert!(
            (right_edge - expected_edge).abs() <= 0.001,
            "center should reach inset viewport edge: right_edge={right_edge}, expected_edge={expected_edge}"
        );
        assert!(
            (center.x - (left.x + left.width + engine.config.panel_gap)).abs() <= 0.001,
            "center should start after left sidebar gap"
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
        let expected_body_height = viewport.height as f32
            - top.height
            - status.height
            - engine.status_bar_top_gap(status.height)
            - engine.config.panel_gap;

        assert!(
            (left.height - expected_body_height).abs() <= 0.001,
            "left sidebar should fill body height: left.height={} expected={expected_body_height}",
            left.height
        );
    }

    #[test]
    fn status_bar_keeps_gap_from_body_regions() {
        let engine = WorkbenchLayoutEngine::new(WorkbenchLayoutConfig::default());
        let mut state = WorkbenchPanelState::default();
        state.left.visible = true;
        state.bottom.visible = true;

        let layout = engine.compute(PhysicalSize::new(1280, 800), &state);
        let center = layout.model.find(RegionId::Center).expect("center region");
        let bottom = layout
            .model
            .find(RegionId::BottomPanel)
            .expect("bottom region");
        let status = layout
            .model
            .find(RegionId::StatusBar)
            .expect("status bar region");
        let expected_gap = engine.status_bar_top_gap(status.height);
        let body_bottom = (bottom.y + bottom.height).max(center.y + center.height);

        assert!(
            status.y >= body_bottom + expected_gap - 0.001,
            "status bar should be padded below body regions: status.y={} body_bottom={} expected_gap={}",
            status.y,
            body_bottom,
            expected_gap
        );
    }

    #[test]
    fn chrome_regions_pin_to_window_edges_vertically() {
        let engine = WorkbenchLayoutEngine::new(WorkbenchLayoutConfig::default());
        let layout = engine.compute(
            PhysicalSize::new(1280, 800),
            &WorkbenchPanelState::default(),
        );
        let top = layout.model.find(RegionId::TopBar).expect("top bar region");
        let status = layout
            .model
            .find(RegionId::StatusBar)
            .expect("status bar region");

        assert!(
            (top.y - 0.0).abs() <= 0.001,
            "top bar should touch window top"
        );
        assert!(
            ((status.y + status.height) - 800.0).abs() <= 0.001,
            "status bar should touch window bottom"
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

        assert!(bottom.x >= left.x + left.width - 0.001);
        assert!((bottom.x - center.x).abs() <= 0.001);
        assert!(
            (bottom.x - (left.x + left.width + engine.config.panel_gap)).abs() <= 0.001,
            "bottom panel should start after left sidebar gap: bottom.x={} left.max_x={} gap={}",
            bottom.x,
            left.x + left.width,
            engine.config.panel_gap
        );
        let bottom_right = bottom.x + bottom.width;
        let expected_bottom_right = 1400.0 - engine.config.outer_gap - engine.config.panel_gap;
        assert!(
            (bottom_right - expected_bottom_right).abs() <= 0.001,
            "bottom panel should keep right margin when no right dock: bottom_right={} expected={expected_bottom_right}",
            bottom_right
        );
        assert!(
            (bottom.width - center.width).abs() <= 0.001,
            "bottom panel width should match center pane: bottom.width={} center.width={}",
            bottom.width,
            center.width
        );
    }

    #[test]
    fn tiny_window_preserves_non_negative_bounds() {
        let engine = WorkbenchLayoutEngine::new(WorkbenchLayoutConfig::default());
        let state = WorkbenchPanelState::default();
        let layout = engine.compute(PhysicalSize::new(1, 1), &state);

        for region in layout.model.flatten() {
            assert!(
                region.bounds.width >= 0.0,
                "{} width < 0",
                region.id.label()
            );
            assert!(
                region.bounds.height >= 0.0,
                "{} height < 0",
                region.id.label()
            );
            assert!(region.bounds.x >= 0.0, "{} x < 0", region.id.label());
            assert!(region.bounds.y >= 0.0, "{} y < 0", region.id.label());
        }
    }

    #[test]
    fn sidebars_shrink_before_center_drops_below_minimum() {
        let engine = WorkbenchLayoutEngine::new(WorkbenchLayoutConfig::default());
        let mut state = WorkbenchPanelState::default();
        state.left.visible = true;
        state.right.visible = true;
        state.left.size_px = 500.0;
        state.right.size_px = 500.0;

        let layout = engine.compute(PhysicalSize::new(700, 500), &state);
        let center = layout.model.find(RegionId::Center).expect("center region");

        assert!(
            center.width >= engine.config.center_min_width - 0.001,
            "center width dropped below min: {} < {}",
            center.width,
            engine.config.center_min_width
        );
    }

    #[test]
    fn right_sidebar_uses_full_allocated_bounds() {
        let engine = WorkbenchLayoutEngine::new(WorkbenchLayoutConfig::default());
        let mut state = WorkbenchPanelState::default();
        state.right.visible = true;
        state.right.size_px = 320.0;

        let layout = engine.compute(PhysicalSize::new(1280, 800), &state);
        let right = layout
            .model
            .find(RegionId::RightSidebar)
            .expect("right sidebar");

        // Right sidebar now uses full allocated bounds (no floating panel_gap),
        // matching left sidebar behavior. The visual border/styling is handled
        // by the renderer, not the layout engine.
        assert!(right.width <= 320.0);
        assert!(right.height <= 800.0);
    }

    #[test]
    fn right_sidebar_contains_chat_sub_regions() {
        let engine = WorkbenchLayoutEngine::new(WorkbenchLayoutConfig::default());
        let mut state = WorkbenchPanelState::default();
        state.right.visible = true;
        state.right.size_px = 320.0;

        let layout = engine.compute(PhysicalSize::new(1280, 800), &state);
        assert!(layout.model.find(RegionId::AiChatHistory).is_some());
        assert!(layout.model.find(RegionId::AiChatInput).is_some());

        let history = layout.model.find(RegionId::AiChatHistory).unwrap();
        let input = layout.model.find(RegionId::AiChatInput).unwrap();
        // Input should be below history
        assert!(input.y >= history.y + history.height - 0.001);
    }

    #[test]
    fn maximize_center_gives_full_space_to_center() {
        use crate::workbench::focus_manager::FocusTarget;

        let engine = WorkbenchLayoutEngine::new(WorkbenchLayoutConfig::default());
        let mut state = WorkbenchPanelState::default();
        state.maximized_region = Some(FocusTarget::CenterEditor);

        let layout = engine.compute(PhysicalSize::new(1280, 800), &state);
        let flat = layout.model.flatten();
        let find = |id: RegionId| flat.iter().find(|r| r.id == id);

        // TopBar and StatusBar remain visible
        let top = find(RegionId::TopBar).expect("top bar");
        assert!(top.bounds.height > 0.0);
        assert!(top.visible);
        let status = find(RegionId::StatusBar).expect("status bar");
        assert!(status.bounds.height > 0.0);
        assert!(status.visible);

        // Center is visible with full space
        let center = find(RegionId::Center).expect("center");
        assert!(center.visible);
        assert!(center.bounds.width > 0.0);
        assert!(center.bounds.height > 0.0);

        // Other regions are hidden with zero bounds
        let left = find(RegionId::LeftSidebar).expect("left");
        assert!(!left.visible);
        assert!(left.bounds.width == 0.0);
        assert!(left.bounds.height == 0.0);

        let right = find(RegionId::RightSidebar).expect("right");
        assert!(!right.visible);
        assert!(right.bounds.width == 0.0);

        let bottom = find(RegionId::BottomPanel).expect("bottom");
        assert!(!bottom.visible);
        assert!(bottom.bounds.width == 0.0);

        // No split handles
        assert!(layout.handles.is_empty());
    }

    #[test]
    fn maximize_left_sidebar_gives_full_space() {
        use crate::workbench::focus_manager::FocusTarget;

        let engine = WorkbenchLayoutEngine::new(WorkbenchLayoutConfig::default());
        let mut state = WorkbenchPanelState::default();
        state.maximized_region = Some(FocusTarget::LeftSidebar);

        let layout = engine.compute(PhysicalSize::new(1280, 800), &state);
        let flat = layout.model.flatten();
        let find = |id: RegionId| flat.iter().find(|r| r.id == id);

        let left = find(RegionId::LeftSidebar).expect("left");
        assert!(left.visible);
        assert!(left.bounds.width > 0.0);
        assert!(left.bounds.height > 0.0);

        let center = find(RegionId::Center).expect("center");
        assert!(!center.visible);
        assert!(center.bounds.width == 0.0);

        assert!(layout.handles.is_empty());
    }

    #[test]
    fn maximize_bottom_panel_gives_full_space() {
        use crate::workbench::focus_manager::FocusTarget;

        let engine = WorkbenchLayoutEngine::new(WorkbenchLayoutConfig::default());
        let mut state = WorkbenchPanelState::default();
        state.maximized_region = Some(FocusTarget::BottomPanel);

        let layout = engine.compute(PhysicalSize::new(1280, 800), &state);
        let flat = layout.model.flatten();
        let find = |id: RegionId| flat.iter().find(|r| r.id == id);

        let bottom = find(RegionId::BottomPanel).expect("bottom");
        assert!(bottom.visible);
        assert!(bottom.bounds.width > 0.0);
        assert!(bottom.bounds.height > 0.0);

        let center = find(RegionId::Center).expect("center");
        assert!(!center.visible);

        assert!(layout.handles.is_empty());
    }
}
