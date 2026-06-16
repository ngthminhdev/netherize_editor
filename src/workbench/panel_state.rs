#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PanelTabId {
    Explorer,
    Search,
    Inspector,
    Outline,
    Terminal,
    TestRunner,
    DebugConsole,
    Problems,
    AiChat,
    MarkdownPreview,
}

impl PanelTabId {
    pub fn label(self) -> &'static str {
        match self {
            Self::Explorer => "Explorer",
            Self::Search => "Search",
            Self::Inspector => "Inspector",
            Self::Outline => "Outline",
            Self::Terminal => "Terminal",
            Self::TestRunner => "Test Runner",
            Self::DebugConsole => "Debug Console",
            Self::Problems => "Problems",
            Self::AiChat => "AI Chat",
            Self::MarkdownPreview => "Preview",
        }
    }

    /// Icon displayed in the right-dock tab strip (SVG asset or nerd-font glyph).
    pub fn icon_glyph(self) -> Option<&'static str> {
        match self {
            Self::Explorer => Some("built_in:folder"),
            Self::Search => Some(""),          // nf-fa-search
            Self::Inspector => Some(""),       // nf-fa-info_circle
            Self::Outline => Some("built_in:outline"),
            Self::Terminal => Some(""),        // nf-fa-terminal
            Self::TestRunner => Some("built_in:flask"),
            Self::DebugConsole => Some(""),    // nf-fa-terminal
            Self::Problems => Some(""),        // nf-fa-exclamation_triangle
            Self::AiChat => Some("built_in:sparkles"),
            Self::MarkdownPreview => Some(""), // nf-fa-file_text
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

    /// Switch to the tab at `idx`. Returns `true` if the active tab changed,
    /// `false` if `idx` is out of range or already active.
    pub fn switch_to_index(&mut self, idx: usize) -> bool {
        if idx >= self.tabs.len() || self.active_tab == idx {
            return false;
        }
        self.active_tab = idx;
        true
    }

    /// Switch to the tab matching `tab_id`. Returns `true` if the active tab
    /// changed (or the tab was found but was already active — returns `false`).
    pub fn switch_to_tab(&mut self, tab_id: PanelTabId) -> bool {
        if let Some(idx) = self.tabs.iter().position(|t| *t == tab_id) {
            if self.active_tab != idx {
                self.active_tab = idx;
                return true;
            }
        }
        false
    }
}

use super::focus_manager::FocusTarget;

#[derive(Debug, Clone, PartialEq)]
pub struct WorkbenchPanelState {
    pub left: PanelState,
    pub right: PanelState,
    pub bottom: PanelState,
    pub overlay_visible: bool,
    /// When `Some`, the specified region/editor is maximized (Zen Mode).
    pub maximized_region: Option<FocusTarget>,
}

impl Default for WorkbenchPanelState {
    fn default() -> Self {
        Self {
            left: PanelState::new(
                true,
                240.0,
                vec![
                    PanelTabId::Explorer,
                    PanelTabId::Outline,
                ],
            ),
            right: PanelState::new(
                false,
                650.0,
                vec![
                    PanelTabId::AiChat,
                    PanelTabId::TestRunner,
                ],
            ),
            // Bottom dock only exposes the Terminal for now; Debug Console and
            // Problems have no content yet, so they're hidden to avoid dead tabs.
            bottom: PanelState::new(false, 420.0, vec![PanelTabId::Terminal]),
            overlay_visible: false,
            maximized_region: None,
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
