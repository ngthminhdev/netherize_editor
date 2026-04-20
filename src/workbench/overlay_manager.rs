use crate::workbench::{
    debug_state::DebugSharedState, region_model::RegionBounds,
    text_coordinate_map::TextCoordinateMapper,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum OverlaySpace {
    WindowRelative,
    EditorRelative,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum OverlayKind {
    CommandPalette,
    Toast,
    FloatingToolbar,
    ExecutionHud,
    InlineValue,
    BreakpointTooltip,
}

#[derive(Debug, Clone, PartialEq)]
pub struct OverlaySurface {
    pub kind: OverlayKind,
    pub space: OverlaySpace,
    pub rect: RegionBounds,
    pub color: [f32; 4],
    pub label: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct OverlayManager {
    pub command_palette_open: bool,
    pub command_palette_query: String,
    pub toast_message: Option<String>,
}

impl Default for OverlayManager {
    fn default() -> Self {
        Self {
            command_palette_open: false,
            command_palette_query: String::new(),
            toast_message: Some("debug session attached".to_string()),
        }
    }
}

impl OverlayManager {
    pub fn toggle_command_palette(&mut self) -> bool {
        if !self.command_palette_open {
            self.command_palette_query.clear();
        }
        self.command_palette_open = !self.command_palette_open;
        true
    }

    pub fn is_command_palette_open(&self) -> bool {
        self.command_palette_open
    }

    pub fn command_palette_query(&self) -> &str {
        &self.command_palette_query
    }

    pub fn push_command_palette_text(&mut self, text: &str) -> bool {
        if !self.command_palette_open || text.is_empty() {
            return false;
        }
        self.command_palette_query.push_str(text);
        true
    }

    pub fn pop_command_palette_char(&mut self) -> bool {
        if !self.command_palette_open {
            return false;
        }
        self.command_palette_query.pop().is_some()
    }

    pub fn set_toast(&mut self, message: Option<String>) {
        self.toast_message = message;
    }

    pub fn build_overlays(
        &self,
        window_bounds: RegionBounds,
        editor_bounds: RegionBounds,
        mapper: &TextCoordinateMapper,
        debug_state: &DebugSharedState,
    ) -> Vec<OverlaySurface> {
        let mut overlays = Vec::new();

        if self.command_palette_open {
            let width = (window_bounds.width * 0.42).clamp(320.0, 680.0);
            let x = window_bounds.x + (window_bounds.width - width) * 0.5;
            let y = window_bounds.y + 20.0;
            overlays.push(OverlaySurface {
                kind: OverlayKind::CommandPalette,
                space: OverlaySpace::WindowRelative,
                rect: RegionBounds::new(x, y, width, 54.0),
                color: [0.10, 0.12, 0.19, 0.94],
                label: format!("Command Palette: {}", self.command_palette_query),
            });
        }

        if let Some(message) = &self.toast_message {
            let width = (window_bounds.width * 0.26).clamp(220.0, 360.0);
            overlays.push(OverlaySurface {
                kind: OverlayKind::Toast,
                space: OverlaySpace::WindowRelative,
                rect: RegionBounds::new(
                    window_bounds.x + window_bounds.width - width - 16.0,
                    window_bounds.y + 16.0,
                    width,
                    34.0,
                ),
                color: [0.20, 0.34, 0.24, 0.92],
                label: message.clone(),
            });
        }

        if let Some(location) = debug_state.execution_location
            && let Some(anchor) = mapper.map_source_location_rect(location)
        {
            let hud_rect = clamp_to_bounds(
                RegionBounds::new(anchor.x + 18.0, anchor.y - 6.0, 170.0, 28.0),
                editor_bounds,
            );
            overlays.push(OverlaySurface {
                kind: OverlayKind::ExecutionHud,
                space: OverlaySpace::EditorRelative,
                rect: hud_rect,
                color: [0.88, 0.58, 0.18, 0.92],
                label: format!("paused @ {}:{}", location.line + 1, location.column + 1),
            });

            let floating_tools_rect = clamp_to_bounds(
                RegionBounds::new(anchor.x + 18.0, anchor.y + 24.0, 120.0, 24.0),
                editor_bounds,
            );
            overlays.push(OverlaySurface {
                kind: OverlayKind::FloatingToolbar,
                space: OverlaySpace::EditorRelative,
                rect: floating_tools_rect,
                color: [0.14, 0.20, 0.33, 0.92],
                label: "Step | Continue".to_string(),
            });
        }

        for inline in debug_state.inline_values.iter().take(3) {
            if let Some(anchor) = mapper.map_source_location_rect(inline.location) {
                let rect = clamp_to_bounds(
                    RegionBounds::new(anchor.x + 10.0, anchor.y - 2.0, 118.0, 22.0),
                    editor_bounds,
                );
                overlays.push(OverlaySurface {
                    kind: OverlayKind::InlineValue,
                    space: OverlaySpace::EditorRelative,
                    rect,
                    color: [0.29, 0.23, 0.14, 0.90],
                    label: inline.text.clone(),
                });
            }
        }

        if let Some(first_breakpoint) = debug_state.breakpoints.first()
            && let Some(anchor) = mapper.gutter_marker_rect(first_breakpoint.location.line)
        {
            let rect = clamp_to_bounds(
                RegionBounds::new(anchor.x + 16.0, anchor.y - 6.0, 150.0, 24.0),
                editor_bounds,
            );
            overlays.push(OverlaySurface {
                kind: OverlayKind::BreakpointTooltip,
                space: OverlaySpace::EditorRelative,
                rect,
                color: [0.42, 0.13, 0.15, 0.94],
                label: format!("breakpoint line {}", first_breakpoint.location.line + 1),
            });
        }

        overlays
    }
}

fn clamp_to_bounds(rect: RegionBounds, bounds: RegionBounds) -> RegionBounds {
    let max_x = (bounds.x + bounds.width - rect.width).max(bounds.x);
    let max_y = (bounds.y + bounds.height - rect.height).max(bounds.y);
    RegionBounds::new(
        rect.x.clamp(bounds.x, max_x),
        rect.y.clamp(bounds.y, max_y),
        rect.width,
        rect.height,
    )
}

#[cfg(test)]
mod tests {
    use crate::workbench::{
        debug_state::DebugSharedState, region_model::RegionBounds,
        text_coordinate_map::TextCoordinateMapper,
    };

    use super::OverlayManager;

    #[test]
    fn builds_both_window_and_editor_relative_overlays() {
        let mut manager = OverlayManager::default();
        manager.command_palette_open = true;
        manager.command_palette_query = "src/main".to_string();
        let debug_state = DebugSharedState::default();
        let mapper = TextCoordinateMapper::from_text(
            "a\nb\nc\nd\ne\nf\ng\nh",
            RegionBounds::new(40.0, 120.0, 700.0, 420.0),
            16.0,
            12.0,
            20.0,
            9.0,
        );
        let overlays = manager.build_overlays(
            RegionBounds::new(0.0, 0.0, 1280.0, 800.0),
            RegionBounds::new(40.0, 120.0, 700.0, 420.0),
            &mapper,
            &debug_state,
        );

        assert!(
            overlays
                .iter()
                .any(|overlay| overlay.space == super::OverlaySpace::WindowRelative)
        );
        assert!(
            overlays
                .iter()
                .any(|overlay| overlay.space == super::OverlaySpace::EditorRelative)
        );
        assert!(overlays.iter().any(|overlay| {
            overlay.kind == super::OverlayKind::CommandPalette && overlay.label.contains("src/main")
        }));
    }
}
