use winit::dpi::PhysicalSize;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PanelRect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl PanelRect {
    pub fn origin(self) -> [f32; 2] {
        [self.x, self.y]
    }
}

#[derive(Debug, Clone, Copy)]
pub struct PanelLayoutConfig {
    pub outer_margin_x: f32,
    pub content_top: f32,
    pub content_bottom_margin: f32,
    pub hud_origin_x: f32,
    pub hud_origin_y: f32,
    pub hud_height: f32,
    pub split_ratio: f32,
    pub split_gap: f32,
}

impl Default for PanelLayoutConfig {
    fn default() -> Self {
        Self {
            outer_margin_x: 40.0,
            content_top: 320.0,
            content_bottom_margin: 16.0,
            hud_origin_x: 16.0,
            hud_origin_y: 12.0,
            hud_height: 300.0,
            split_ratio: 0.70,
            split_gap: 8.0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EditorPanelLayout {
    pub hud: PanelRect,
    pub editor: PanelRect,
    pub terminal: PanelRect,
    pub terminal_visible: bool,
    pub split_ratio: f32,
}

impl EditorPanelLayout {
    pub fn from_window(
        size: PhysicalSize<u32>,
        terminal_visible: bool,
        config: PanelLayoutConfig,
    ) -> Self {
        let window_w = size.width as f32;
        let window_h = size.height as f32;

        let hud = PanelRect {
            x: config.hud_origin_x,
            y: config.hud_origin_y,
            width: (window_w - config.hud_origin_x * 2.0).max(1.0),
            height: config.hud_height.max(1.0),
        };

        let content_x = config.outer_margin_x;
        let content_y = config.content_top;
        let content_w = (window_w - config.outer_margin_x * 2.0).max(1.0);
        let content_h = (window_h - config.content_top - config.content_bottom_margin).max(1.0);

        if !terminal_visible || content_h <= (config.split_gap + 2.0) {
            let editor = PanelRect {
                x: content_x,
                y: content_y,
                width: content_w,
                height: content_h,
            };
            let terminal = PanelRect {
                x: content_x,
                y: content_y + content_h,
                width: content_w,
                height: 0.0,
            };
            return Self {
                hud,
                editor,
                terminal,
                terminal_visible: false,
                split_ratio: config.split_ratio,
            };
        }

        let max_editor = (content_h - config.split_gap - 1.0).max(1.0);
        let editor_h = (content_h * config.split_ratio).clamp(1.0, max_editor);
        let terminal_h = (content_h - editor_h - config.split_gap).max(1.0);

        let editor = PanelRect {
            x: content_x,
            y: content_y,
            width: content_w,
            height: editor_h,
        };
        let terminal = PanelRect {
            x: content_x,
            y: content_y + editor_h + config.split_gap,
            width: content_w,
            height: terminal_h,
        };

        Self {
            hud,
            editor,
            terminal,
            terminal_visible: true,
            split_ratio: config.split_ratio,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{EditorPanelLayout, PanelLayoutConfig};

    #[test]
    fn split_layout_is_roughly_70_30_when_terminal_is_visible() {
        let size = winit::dpi::PhysicalSize::new(1200, 900);
        let config = PanelLayoutConfig {
            content_top: 200.0,
            hud_height: 150.0,
            ..Default::default()
        };

        let layout = EditorPanelLayout::from_window(size, true, config);
        let content_h = layout.editor.height + layout.terminal.height + config.split_gap;
        let ratio = layout.editor.height / content_h;

        assert!((ratio - 0.70).abs() < 0.06, "ratio={ratio}");
        assert!(layout.terminal.height > 0.0);
    }

    #[test]
    fn terminal_panel_collapses_when_hidden() {
        let size = winit::dpi::PhysicalSize::new(1000, 700);
        let layout = EditorPanelLayout::from_window(size, false, PanelLayoutConfig::default());

        assert!(!layout.terminal_visible);
        assert_eq!(layout.terminal.height, 0.0);
        assert!(layout.editor.height > 0.0);
    }
}
