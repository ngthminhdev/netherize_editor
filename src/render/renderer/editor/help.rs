#![allow(unused_imports)]

use crate::{
    app::app_state::{
        AppState, CompletionDisplayItem, DiagnosticsState, EditorOverlay, FloatingBoxBlock,
        FloatingBoxStyle, HelpState, OverlayColorToken, ReferencesBufferState, SettingItem,
        SettingsState,
    },
    async_runtime::message::LspDiagnostic,
    config::theme_config::ThemeConfig,
    core::mode::EditorMode,
    render::{
        glyph_instance::GlyphInstance, region_pipeline::RegionDrawInstance, renderer::Renderer,
    },
    text::layout_sync::{compute_caret_layout, compute_cursor_overlay, rebuild_layout_projection},
};
use cosmic_text::Metrics;

use super::super::helpers::{
    caret_rect_for_mode, clamp_monospace_text, estimate_monospace_width, gutter_width_for_editor,
    layout_panel_rich_text, layout_panel_text, layout_panel_text_italic, rect_to_scissor,
    should_draw_block_cursor,
};
use super::{cursor_diagnostic, editor_viewport_geometry, run_x_for_byte, wrap_text_lines};
use crate::text::text_system::StyledTextSpan;

impl Renderer {
    pub fn update_help_buffer_content(&mut self, help: &HelpState, center_bounds: [f32; 4]) {
        if center_bounds[2] < 1.0 || center_bounds[3] < 1.0 {
            self.clear_editor_overlays();
            return;
        }

        let font_size = self.theme.editor.font_size;
        let line_height = self.theme.editor.line_height.max(font_size + 4.0);
        self.editor_overlay_text_system
            .set_metrics(Metrics::new(font_size, line_height));
        self.editor_overlay_scissor = rect_to_scissor(center_bounds);

        let pad_x = self.editor_padding_x.max(18.0);
        let pad_y = self.editor_padding_y.max(18.0);
        let panel_x = center_bounds[0] + pad_x;
        let panel_y = center_bounds[1] + pad_y;
        let panel_w = (center_bounds[2] - pad_x * 2.0).max(1.0);
        let panel_h = (center_bounds[3] - pad_y * 2.0).max(1.0);
        let fg = self.theme.ui.fg.as_f32();
        let fg_dim = self.theme.ui.fg_dim.as_f32();
        let accent = self.theme.ui.accent.as_f32();
        let panel_bg = self.theme.ui.panel_bg.as_f32();
        let mut divider = self.theme.ui.fg_ghost.as_f32();
        divider[3] = divider[3].clamp(0.28, 0.42);

        let mut glyphs = Vec::new();
        let chrome = vec![
            RegionDrawInstance::new([panel_x, panel_y, panel_w, panel_h], panel_bg),
            RegionDrawInstance::new(
                [panel_x, panel_y + line_height + 15.0, panel_w, 1.0],
                divider,
            ),
        ];

        self.editor_overlay_text_system
            .set_size(Some((panel_w - 28.0).max(1.0)), Some(line_height));
        glyphs.extend(layout_panel_text(
            &help.title,
            &mut self.editor_overlay_text_system,
            &mut self.atlas,
            &self.queue,
            panel_x + 14.0,
            panel_y + 8.0,
            accent,
        ));

        let rows = ((panel_h - line_height - 28.0) / line_height).floor() as usize;
        let text_w = (panel_w - 28.0).max(1.0);
        for (row, line) in help.lines.iter().take(rows.max(1)).enumerate() {
            let color = if line.ends_with("Help")
                || matches!(
                    line.as_str(),
                    "Command palette" | "Buffers" | "Navigation" | "Editing" | "Tools"
                ) {
                fg
            } else {
                fg_dim
            };
            self.editor_overlay_text_system
                .set_size(Some(text_w), Some(line_height));
            glyphs.extend(layout_panel_text(
                &clamp_monospace_text(line, text_w, font_size),
                &mut self.editor_overlay_text_system,
                &mut self.atlas,
                &self.queue,
                panel_x + 14.0,
                panel_y + line_height + 24.0 + row as f32 * line_height,
                color,
            ));
        }

        self.editor_overlay_chrome_instances = chrome;
        self.editor_overlay_glyph_instances = glyphs;
        self.editor_overlay_text_pipeline.upload_instances(
            &self.device,
            &self.queue,
            &self.editor_overlay_glyph_instances,
        );
    }
}
