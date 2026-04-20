use crate::workbench::{
    panel_state::WorkbenchPanelState,
    region_model::{RegionId, RegionModel},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FocusTarget {
    CenterEditor,
    LeftSidebar,
    RightSidebar,
    BottomPanel,
    TopBar,
    StatusBar,
    OverlayLayer,
}

impl FocusTarget {
    pub fn label(self) -> &'static str {
        match self {
            Self::CenterEditor => "center_editor",
            Self::LeftSidebar => "left_sidebar",
            Self::RightSidebar => "right_sidebar",
            Self::BottomPanel => "bottom_panel",
            Self::TopBar => "top_bar",
            Self::StatusBar => "status_bar",
            Self::OverlayLayer => "overlay_layer",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FocusManager {
    current: FocusTarget,
}

impl Default for FocusManager {
    fn default() -> Self {
        Self {
            current: FocusTarget::CenterEditor,
        }
    }
}

impl FocusManager {
    pub fn current(&self) -> FocusTarget {
        self.current
    }

    pub fn set(&mut self, target: FocusTarget) -> bool {
        if self.current == target {
            return false;
        }
        self.current = target;
        true
    }

    pub fn ensure_valid(&mut self, panel_state: &WorkbenchPanelState) -> bool {
        let available = visible_focus_targets(panel_state);
        if available.contains(&self.current) {
            return false;
        }
        let fallback = available
            .first()
            .copied()
            .unwrap_or(FocusTarget::CenterEditor);
        self.set(fallback)
    }

    pub fn cycle_next(&mut self, panel_state: &WorkbenchPanelState) -> bool {
        self.cycle(panel_state, false)
    }

    pub fn cycle_prev(&mut self, panel_state: &WorkbenchPanelState) -> bool {
        self.cycle(panel_state, true)
    }

    fn cycle(&mut self, panel_state: &WorkbenchPanelState, reverse: bool) -> bool {
        let available = visible_focus_targets(panel_state);
        if available.is_empty() {
            return false;
        }

        let current_idx = available
            .iter()
            .position(|target| *target == self.current)
            .unwrap_or(0);
        let next_idx = if reverse {
            if current_idx == 0 {
                available.len() - 1
            } else {
                current_idx - 1
            }
        } else {
            (current_idx + 1) % available.len()
        };
        self.set(available[next_idx])
    }

    pub fn set_from_click(
        &mut self,
        x: f32,
        y: f32,
        model: &RegionModel,
        panel_state: &WorkbenchPanelState,
    ) -> bool {
        // Overlay được ưu tiên bắt focus nếu đang visible và hit trong bounds của nó.
        if panel_state.overlay_visible
            && is_point_inside_region(model, RegionId::OverlayLayer, x, y)
        {
            return self.set(FocusTarget::OverlayLayer);
        }

        let mapping = [
            (RegionId::LeftSidebar, FocusTarget::LeftSidebar),
            (RegionId::Center, FocusTarget::CenterEditor),
            (RegionId::RightSidebar, FocusTarget::RightSidebar),
            (RegionId::BottomPanel, FocusTarget::BottomPanel),
            (RegionId::TopBar, FocusTarget::TopBar),
            (RegionId::StatusBar, FocusTarget::StatusBar),
        ];

        for (region, target) in mapping {
            if is_point_inside_region(model, region, x, y) {
                return self.set(target);
            }
        }

        false
    }
}

fn visible_focus_targets(panel_state: &WorkbenchPanelState) -> Vec<FocusTarget> {
    let mut targets = vec![FocusTarget::CenterEditor];
    if panel_state.left.visible {
        targets.push(FocusTarget::LeftSidebar);
    }
    if panel_state.right.visible {
        targets.push(FocusTarget::RightSidebar);
    }
    if panel_state.bottom.visible {
        targets.push(FocusTarget::BottomPanel);
    }
    targets.push(FocusTarget::TopBar);
    targets.push(FocusTarget::StatusBar);
    if panel_state.overlay_visible {
        targets.push(FocusTarget::OverlayLayer);
    }
    targets
}

fn is_point_inside_region(model: &RegionModel, id: RegionId, x: f32, y: f32) -> bool {
    let Some(bounds) = model.find(id) else {
        return false;
    };
    x >= bounds.x && x <= bounds.x + bounds.width && y >= bounds.y && y <= bounds.y + bounds.height
}

#[cfg(test)]
mod tests {
    use super::{FocusManager, FocusTarget};
    use crate::workbench::panel_state::WorkbenchPanelState;

    #[test]
    fn cycle_skips_hidden_panels() {
        let mut focus = FocusManager::default();
        let mut state = WorkbenchPanelState::default();
        state.left.visible = false;
        state.right.visible = false;
        state.bottom.visible = true;

        focus.cycle_next(&state);
        assert_eq!(focus.current(), FocusTarget::BottomPanel);
    }
}
