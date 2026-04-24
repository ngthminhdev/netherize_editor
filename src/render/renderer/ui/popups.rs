use cosmic_text::Metrics;

use crate::{
    config::theme_config::linear_rgba_to_srgb_u8,
    render::{
        glyph_instance::GlyphInstance, region_pipeline::RegionDrawInstance, renderer::Renderer,
    },
    text::text_system::TextSystem,
};

use super::super::helpers::{
    layout_panel_text, layout_panel_text_bold, layout_panel_text_italic, rect_to_scissor,
};

impl Renderer {
    pub fn update_lsp_guide_popup(
        &mut self,
        binary: &str,
        install_cmd: &str,
        window_w: f32,
        window_h: f32,
    ) {
        const MODAL_W: f32 = 500.0;
        const MARGIN_X: f32 = 24.0;
        const BORDER: f32 = 1.5;
        const INNER_PAD_X: f32 = 22.0;
        const INNER_PAD_Y: f32 = 18.0;
        const BLOCK_GAP: f32 = 8.0;

        let popup_available_w = (window_w - MARGIN_X * 2.0).max(160.0);
        let popup_w = popup_available_w.min(MODAL_W);
        let content_w = (popup_w - INNER_PAD_X * 2.0).max(1.0);

        let bg_color = self.theme.ui.panel_bg.as_f32();
        let warning_color = self.theme.ui.warning.as_f32();
        let fg = self.theme.ui.fg.as_f32();
        let accent = self.theme.ui.accent.as_f32();
        let fg_ghost = self.theme.ui.fg_ghost.as_f32();
        let scrim = [0.0, 0.0, 0.0, 0.36];

        let title = "[ LSP Missing ]";
        let subtitle = format!("File type requires Language Server: {binary}");
        let command = format!("Command: {install_cmd}");
        let hint = "Press [Enter] to auto-install in Terminal  |  [Esc] to cancel";

        let title_font = (self.theme.ui.panel_font_size + 2.0).max(12.0);
        let title_line_h = (self.theme.ui.panel_line_height + 4.0).max(1.0);
        let body_font = self.theme.ui.panel_font_size.max(11.0);
        let body_line_h = self.theme.ui.panel_line_height.max(1.0);

        let title_h = measure_wrapped_block_height(
            &mut self.lsp_guide_text_system,
            title,
            content_w,
            title_font,
            title_line_h,
            warning_color,
            PopupTextStyle::Bold,
        );
        let subtitle_h = measure_wrapped_block_height(
            &mut self.lsp_guide_text_system,
            &subtitle,
            content_w,
            body_font,
            body_line_h,
            fg,
            PopupTextStyle::Normal,
        );
        let command_h = measure_wrapped_block_height(
            &mut self.lsp_guide_text_system,
            &command,
            content_w,
            body_font,
            body_line_h,
            accent,
            PopupTextStyle::Italic,
        );
        let hint_h = measure_wrapped_block_height(
            &mut self.lsp_guide_text_system,
            hint,
            content_w,
            body_font,
            body_line_h,
            fg_ghost,
            PopupTextStyle::Normal,
        );

        let popup_h =
            INNER_PAD_Y * 2.0 + title_h + subtitle_h + command_h + hint_h + BLOCK_GAP * 3.0;
        let x = ((window_w - popup_w) * 0.5).max(0.0);
        let y = ((window_h - popup_h) * 0.5).max(0.0);

        let text_x = x + INNER_PAD_X;
        let mut text_y = y + INNER_PAD_Y;

        let mut glyphs: Vec<GlyphInstance> = Vec::new();
        glyphs.extend(layout_wrapped_block(
            title,
            &mut self.lsp_guide_text_system,
            &mut self.atlas,
            &self.queue,
            text_x,
            text_y,
            warning_color,
            content_w,
            title_font,
            title_line_h,
            PopupTextStyle::Bold,
        ));
        text_y += title_h + BLOCK_GAP;

        glyphs.extend(layout_wrapped_block(
            &subtitle,
            &mut self.lsp_guide_text_system,
            &mut self.atlas,
            &self.queue,
            text_x,
            text_y,
            fg,
            content_w,
            body_font,
            body_line_h,
            PopupTextStyle::Normal,
        ));
        text_y += subtitle_h + BLOCK_GAP;

        glyphs.extend(layout_wrapped_block(
            &command,
            &mut self.lsp_guide_text_system,
            &mut self.atlas,
            &self.queue,
            text_x,
            text_y,
            accent,
            content_w,
            body_font,
            body_line_h,
            PopupTextStyle::Italic,
        ));
        text_y += command_h + BLOCK_GAP;

        glyphs.extend(layout_wrapped_block(
            hint,
            &mut self.lsp_guide_text_system,
            &mut self.atlas,
            &self.queue,
            text_x,
            text_y,
            fg_ghost,
            content_w,
            body_font,
            body_line_h,
            PopupTextStyle::Normal,
        ));

        self.lsp_guide_scissor = rect_to_scissor([0.0, 0.0, window_w, window_h]);
        self.lsp_guide_glyph_instances = glyphs;
        self.lsp_guide_text_pipeline.upload_instances(
            &self.device,
            &self.queue,
            &self.lsp_guide_glyph_instances,
        );

        self.lsp_guide_chrome_instances = vec![
            RegionDrawInstance::new([0.0, 0.0, window_w, window_h], scrim),
            RegionDrawInstance::new(
                [
                    x - BORDER,
                    y - BORDER,
                    popup_w + BORDER * 2.0,
                    popup_h + BORDER * 2.0,
                ],
                warning_color,
            ),
            RegionDrawInstance::new([x, y, popup_w, popup_h], bg_color),
        ];
    }

    /// Clear LSP guide popup state (sau khi user dismiss hoặc install).
    pub fn clear_lsp_guide_popup(&mut self) {
        self.lsp_guide_scissor = None;
        self.lsp_guide_chrome_instances.clear();
        self.lsp_guide_glyph_instances.clear();
        self.lsp_guide_text_pipeline
            .upload_instances(&self.device, &self.queue, &[]);
    }

    pub fn update_toast_popup(&mut self, message: &str, window_w: f32, window_h: f32) {
        const TOAST_W: f32 = 360.0;
        const MARGIN_X: f32 = 18.0;
        const MARGIN_Y: f32 = 18.0;
        const BORDER: f32 = 1.0;
        const INNER_PAD_X: f32 = 16.0;
        const INNER_PAD_Y: f32 = 12.0;

        let toast_available_w = (window_w - MARGIN_X * 2.0).max(140.0);
        let toast_w = toast_available_w.min(TOAST_W);
        let content_w = (toast_w - INNER_PAD_X * 2.0).max(1.0);
        let font_size = self.theme.ui.sidebar_font_size.max(11.0);
        let line_h = self.theme.ui.sidebar_line_height.max(1.0);
        let bg_color = self.theme.ui.panel_bg.as_f32();
        let accent = self.theme.ui.accent.as_f32();
        let fg = self.theme.ui.fg.as_f32();

        let text_h = measure_wrapped_block_height(
            &mut self.toast_text_system,
            message,
            content_w,
            font_size,
            line_h,
            fg,
            PopupTextStyle::Normal,
        );
        let toast_h = INNER_PAD_Y * 2.0 + text_h;
        let x = (window_w - toast_w - MARGIN_X).max(0.0);
        let y = MARGIN_Y.min((window_h - toast_h).max(0.0));

        self.toast_scissor = rect_to_scissor([0.0, 0.0, window_w, window_h]);
        self.toast_glyph_instances = layout_wrapped_block(
            message,
            &mut self.toast_text_system,
            &mut self.atlas,
            &self.queue,
            x + INNER_PAD_X,
            y + INNER_PAD_Y,
            fg,
            content_w,
            font_size,
            line_h,
            PopupTextStyle::Normal,
        );
        self.toast_text_pipeline.upload_instances(
            &self.device,
            &self.queue,
            &self.toast_glyph_instances,
        );

        self.toast_chrome_instances = vec![
            RegionDrawInstance::new(
                [
                    x - BORDER,
                    y - BORDER,
                    toast_w + BORDER * 2.0,
                    toast_h + BORDER * 2.0,
                ],
                accent,
            ),
            RegionDrawInstance::new([x, y, toast_w, toast_h], bg_color),
            RegionDrawInstance::new([x, y, 4.0, toast_h], accent),
        ];
    }

    pub fn clear_toast_popup(&mut self) {
        self.toast_scissor = None;
        self.toast_chrome_instances.clear();
        self.toast_glyph_instances.clear();
        self.toast_text_pipeline
            .upload_instances(&self.device, &self.queue, &[]);
    }
}

#[derive(Debug, Clone, Copy)]
enum PopupTextStyle {
    Normal,
    Bold,
    Italic,
}

fn measure_wrapped_block_height(
    text_system: &mut TextSystem,
    text: &str,
    width: f32,
    font_size: f32,
    line_height: f32,
    color: [f32; 4],
    style: PopupTextStyle,
) -> f32 {
    text_system.set_metrics(Metrics::new(font_size, line_height));
    text_system.set_size(Some(width), None);
    let color_u8 = linear_rgba_to_srgb_u8(color);
    match style {
        PopupTextStyle::Normal => text_system.set_text_with_color(text, color_u8),
        PopupTextStyle::Bold => text_system.set_text_bold_color(text, color_u8),
        PopupTextStyle::Italic => text_system.set_text_italic_color(text, color_u8),
    }
    let wrapped_lines = text_system.buffer().layout_runs().count().max(1);
    wrapped_lines as f32 * line_height
}

fn layout_wrapped_block(
    text: &str,
    text_system: &mut TextSystem,
    atlas: &mut crate::text::atlas::GlyphAtlas,
    queue: &wgpu::Queue,
    origin_x: f32,
    origin_y: f32,
    color: [f32; 4],
    width: f32,
    font_size: f32,
    line_height: f32,
    style: PopupTextStyle,
) -> Vec<GlyphInstance> {
    text_system.set_metrics(Metrics::new(font_size, line_height));
    text_system.set_size(Some(width), None);
    match style {
        PopupTextStyle::Normal => {
            layout_panel_text(text, text_system, atlas, queue, origin_x, origin_y, color)
        }
        PopupTextStyle::Bold => {
            layout_panel_text_bold(text, text_system, atlas, queue, origin_x, origin_y, color)
        }
        PopupTextStyle::Italic => {
            layout_panel_text_italic(text, text_system, atlas, queue, origin_x, origin_y, color)
        }
    }
}
