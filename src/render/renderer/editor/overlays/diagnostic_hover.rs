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

use super::super::super::helpers::{
    caret_rect_for_mode, clamp_monospace_text, estimate_monospace_width, gutter_width_for_editor,
    layout_panel_rich_text, layout_panel_text, layout_panel_text_italic, rect_to_scissor,
    should_draw_block_cursor,
};
use super::super::completion::{completion_kind_badge, completion_label_spans};
use super::super::{cursor_diagnostic, editor_viewport_geometry, run_x_for_byte, wrap_text_lines};
use crate::text::text_system::StyledTextSpan;
impl Renderer {
    pub fn clear_diagnostic_hover_popup(&mut self) {
        self.diagnostic_hover_scissor = None;
        self.diagnostic_hover_chrome_instances.clear();
        self.diagnostic_hover_glyph_instances.clear();
        self.diagnostic_hover_text_pipeline
            .upload_instances(&self.device, &self.queue, &[]);
    }

    pub fn update_diagnostic_hover_popup(&mut self, app_state: &AppState, center_bounds: [f32; 4]) {
        if center_bounds[2] < 1.0 || center_bounds[3] < 1.0 {
            self.clear_diagnostic_hover_popup();
            return;
        }

        let Some(diagnostic) = cursor_diagnostic(app_state) else {
            self.clear_diagnostic_hover_popup();
            return;
        };

        const PAD_X: f32 = 10.0;
        const PAD_Y: f32 = 6.0;
        const BORDER: f32 = 1.0;
        const WINDOW_PAD: f32 = 12.0;

        let geometry = editor_viewport_geometry(self, app_state, center_bounds);
        let window_w = self.surface_state.config.width.max(1) as f32;
        let window_h = self.surface_state.config.height.max(1) as f32;
        let severity = diagnostic.severity.unwrap_or(2);
        let border_color = if severity == 1 {
            self.theme.ui.error.as_f32()
        } else {
            self.theme.ui.warning.as_f32()
        };
        let bg_color = self.theme.ui.panel_bg.as_f32();
        let text_color = self.theme.ui.fg.as_f32();
        let char_w = (geometry.font_size * 0.6).max(1.0);
        let available_popup_w = (window_w - WINDOW_PAD * 2.0)
            .min(geometry.viewport_text_width)
            .max(1.0);
        let max_popup_w = available_popup_w.min(420.0);
        let wrap_cols = ((max_popup_w - PAD_X * 2.0) / char_w).floor() as usize;
        let wrapped = wrap_text_lines(&diagnostic.message, wrap_cols.max(16));
        let longest = wrapped
            .iter()
            .map(|line| line.chars().count())
            .max()
            .unwrap_or(0);
        let desired_popup_w = longest as f32 * char_w + PAD_X * 2.0;
        let preferred_min_popup_w = 160.0_f32.min(max_popup_w.max(1.0));
        let popup_w = desired_popup_w.clamp(preferred_min_popup_w, max_popup_w.max(1.0));
        debug_assert!(
            popup_w.is_finite() && popup_w >= 1.0,
            "diagnostic popup width must stay finite: desired={desired_popup_w}, available_popup_w={available_popup_w}, max_popup_w={max_popup_w}"
        );
        let popup_h = (wrapped.len().max(1) as f32 * geometry.line_height + PAD_Y * 2.0)
            .max(geometry.line_height);

        let anchor_line = diagnostic.range.start.line as usize;
        let anchor_col = diagnostic.range.start.character as usize;
        let mut anchor_x = geometry.viewport_text_left + 18.0;
        let mut anchor_y = geometry.origin_y + anchor_line as f32 * geometry.line_height;
        for run in self.text_system.buffer().layout_runs() {
            if run.line_i != anchor_line {
                continue;
            }
            anchor_x = run_x_for_byte(geometry.origin_x, &run, anchor_col) + 18.0;
            anchor_y = geometry.origin_y + run.line_top;
            break;
        }

        let max_popup_x = (window_w - popup_w - WINDOW_PAD).max(WINDOW_PAD);
        let min_popup_x = geometry.viewport_text_left.max(WINDOW_PAD).min(max_popup_x);
        let popup_x = anchor_x.clamp(min_popup_x, max_popup_x);
        let mut popup_y = anchor_y + geometry.line_height;
        if popup_y + popup_h > window_h - WINDOW_PAD {
            popup_y = (anchor_y - popup_h).max(WINDOW_PAD);
        }

        self.diagnostic_hover_scissor = Some([
            0,
            0,
            self.surface_state.config.width.max(1),
            self.surface_state.config.height.max(1),
        ]);
        self.diagnostic_hover_chrome_instances = vec![
            RegionDrawInstance::new(
                [
                    popup_x - BORDER,
                    popup_y - BORDER,
                    popup_w + BORDER * 2.0,
                    popup_h + BORDER * 2.0,
                ],
                border_color,
            ),
            RegionDrawInstance::new([popup_x, popup_y, popup_w, popup_h], bg_color),
        ];

        self.diagnostic_hover_text_system
            .set_metrics(Metrics::new(geometry.font_size, geometry.line_height));
        let mut glyphs = Vec::new();
        for (idx, line_text) in wrapped.iter().enumerate() {
            let text_y = popup_y + PAD_Y + idx as f32 * geometry.line_height;
            self.diagnostic_hover_text_system.set_size(
                Some((popup_w - PAD_X * 2.0).max(1.0)),
                Some(geometry.line_height),
            );
            glyphs.extend(layout_panel_text(
                line_text,
                &mut self.diagnostic_hover_text_system,
                &mut self.atlas,
                &self.queue,
                popup_x + PAD_X,
                text_y,
                text_color,
            ));
        }

        self.diagnostic_hover_glyph_instances = glyphs;
        self.diagnostic_hover_text_pipeline.upload_instances(
            &self.device,
            &self.queue,
            &self.diagnostic_hover_glyph_instances,
        );
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn diagnostic_popup_width_handles_narrow_viewport_without_panicking() {
        const PAD_X: f32 = 10.0;
        const WINDOW_PAD: f32 = 12.0;

        let window_w = 104.7749;
        let viewport_text_width = 110.525;
        let char_w = 9.5;
        let longest = 32usize;

        let available_popup_w = (window_w - WINDOW_PAD * 2.0)
            .min(viewport_text_width)
            .max(1.0);
        let max_popup_w = available_popup_w.min(420.0);
        let preferred_min_popup_w = 160.0_f32.min(max_popup_w.max(1.0));
        let popup_w = (longest as f32 * char_w + PAD_X * 2.0)
            .clamp(preferred_min_popup_w, max_popup_w.max(1.0));

        assert!((max_popup_w - 80.7749).abs() < 0.001);
        assert!((popup_w - 80.7749).abs() < 0.001);
    }
}
