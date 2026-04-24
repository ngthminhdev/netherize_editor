#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PanelTabId {
    Explorer,
    Search,
    Inspector,
    Outline,
    Terminal,
    DebugConsole,
    Problems,
}

impl PanelTabId {
    pub fn label(self) -> &'static str {
        match self {
            Self::Explorer => "Explorer",
            Self::Search => "Search",
            Self::Inspector => "Inspector",
            Self::Outline => "Outline",
            Self::Terminal => "Terminal",
            Self::DebugConsole => "Debug Console",
            Self::Problems => "Problems",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct PanelState {
    pub visible: bool,
    pub size_px: f32,
    pub tabs: Vec<PanelTabId>,
    pub active_tab: usize,
}

impl PanelState {
    pub fn new(visible: bool, size_px: f32, tabs: Vec<PanelTabId>) -> Self {
        Self {
            visible,
            size_px: size_px.max(0.0),
            tabs,
            active_tab: 0,
        }
    }

    pub fn active_tab_id(&self) -> Option<PanelTabId> {
        self.tabs.get(self.active_tab).copied()
    }

    pub fn active_tab_label(&self) -> &'static str {
        self.active_tab_id()
            .map(PanelTabId::label)
            .unwrap_or("None")
    }

    pub fn switch_to_next_tab(&mut self) -> bool {
        if self.tabs.len() <= 1 {
            return false;
        }
        self.active_tab = (self.active_tab + 1) % self.tabs.len();
        true
    }

    pub fn switch_to_prev_tab(&mut self) -> bool {
        if self.tabs.len() <= 1 {
            return false;
        }
        self.active_tab = if self.active_tab == 0 {
            self.tabs.len() - 1
        } else {
            self.active_tab - 1
        };
        true
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct WorkbenchPanelState {
    pub left: PanelState,
    pub right: PanelState,
    pub bottom: PanelState,
    pub overlay_visible: bool,
}

impl Default for WorkbenchPanelState {
    fn default() -> Self {
        Self {
            left: PanelState::new(true, 240.0, vec![PanelTabId::Explorer, PanelTabId::Search]),
            right: PanelState::new(
                false,
                260.0,
                vec![PanelTabId::Inspector, PanelTabId::Outline],
            ),
            bottom: PanelState::new(
                false,
                220.0,
                vec![
                    PanelTabId::Terminal,
                    PanelTabId::DebugConsole,
                    PanelTabId::Problems,
                ],
            ),
            overlay_visible: false,
        }
    }
}

impl WorkbenchPanelState {
    /// Default panel tabs, but with sizes driven by the `[ui]` theme block
    /// instead of the hardcoded defaults. Visibility defaults are preserved
    /// (left on, right off, bottom off).
    pub fn from_ui_theme(ui: &crate::config::theme_config::UiThemeTokens) -> Self {
        let mut state = Self::default();
        state.left.size_px = ui.sidebar_width.max(0.0);
        state.right.size_px = ui.right_sidebar_width.max(0.0);
        state.bottom.size_px = ui.bottom_panel_height.max(0.0);
        state
    }

    pub fn toggle_left(&mut self) -> bool {
        self.left.visible = !self.left.visible;
        true
    }

    pub fn toggle_right(&mut self) -> bool {
        self.right.visible = !self.right.visible;
        true
    }

    pub fn toggle_bottom(&mut self) -> bool {
        self.bottom.visible = !self.bottom.visible;
        true
    }

    pub fn toggle_overlay(&mut self) -> bool {
        self.overlay_visible = !self.overlay_visible;
        true
    }

    pub fn switch_bottom_next_tab(&mut self) -> bool {
        self.bottom.switch_to_next_tab()
    }

    pub fn switch_bottom_prev_tab(&mut self) -> bool {
        self.bottom.switch_to_prev_tab()
    }

    pub fn switch_left_next_tab(&mut self) -> bool {
        self.left.switch_to_next_tab()
    }

    pub fn switch_left_prev_tab(&mut self) -> bool {
        self.left.switch_to_prev_tab()
    }

    pub fn switch_right_next_tab(&mut self) -> bool {
        self.right.switch_to_next_tab()
    }

    pub fn switch_right_prev_tab(&mut self) -> bool {
        self.right.switch_to_prev_tab()
    }
}

#[cfg(test)]
mod tests {
    use super::{PanelState, PanelTabId, WorkbenchPanelState};

    #[test]
    fn panel_tabs_cycle_both_directions() {
        let mut panel = PanelState::new(
            true,
            220.0,
            vec![
                PanelTabId::Terminal,
                PanelTabId::DebugConsole,
                PanelTabId::Problems,
            ],
        );

        assert_eq!(panel.active_tab_label(), "Terminal");
        panel.switch_to_next_tab();
        assert_eq!(panel.active_tab_label(), "Debug Console");
        panel.switch_to_prev_tab();
        assert_eq!(panel.active_tab_label(), "Terminal");
    }

    #[test]
    fn toggles_preserve_previous_size() {
        let mut state = WorkbenchPanelState::default();
        state.left.size_px = 321.0;
        state.toggle_left();
        state.toggle_left();
        assert_eq!(state.left.size_px, 321.0);
    }
}
