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
    pub fn clear_editor_content(&mut self) {
        self.glyph_instances.clear();
        self.text_pipeline
            .upload_instances(&self.device, &self.queue, &[]);
        self.caret_pipeline.upload_caret(&self.queue, None);
        self.editor_cursor_overlay_pipeline
            .upload_instances(&self.device, &self.queue, &[]);
        self.gutter_glyph_instances.clear();
        self.gutter_text_pipeline
            .upload_instances(&self.device, &self.queue, &[]);
        self.clear_editor_overlays();
        self.clear_diagnostic_hover_popup();
        self.editor_scissor = None;
    }

    /// Rebuild glyph instances and caret for the center editor region.
    pub fn update_editor_content(
        &mut self,
        text: &str,
        app_state: &AppState,
        center_bounds: [f32; 4],
        spans: &[StyledTextSpan],
    ) {
        let geometry = editor_viewport_geometry(self, app_state, center_bounds);
        let width = geometry.viewport_text_width;

        self.editor_scissor = rect_to_scissor(center_bounds);
        // Allow cosmic-text to shape full height; scissor clips the visible region.
        self.text_system.set_size(Some(width), None);

        let result = rebuild_layout_projection(
            text,
            app_state,
            &mut self.text_system,
            &mut self.atlas,
            &self.queue,
            [geometry.origin_x, geometry.origin_y],
            self.theme.editor.fg.as_f32(),
            self.theme.editor.bg.as_f32(),
            spans,
        );

        match result {
            Ok(projection) => {
                self.glyph_instances = projection.glyph_instances;
                let caret = caret_rect_for_mode(
                    projection.caret_layout,
                    app_state.current_mode(),
                    self.theme.editor.cursor.as_f32(),
                    self.theme.editor.font_size,
                    self.cursor_shape,
                    self.cursor_beam_width,
                    self.cursor_block_width,
                    self.cursor_underline_height,
                );
                self.caret_pipeline.upload_caret(&self.queue, Some(caret));
                let overlay_instances: Vec<GlyphInstance> = projection
                    .cursor_overlay
                    .filter(|_| {
                        should_draw_block_cursor(app_state.current_mode(), self.cursor_shape)
                    })
                    .into_iter()
                    .collect();
                self.editor_cursor_overlay_pipeline.upload_instances(
                    &self.device,
                    &self.queue,
                    &overlay_instances,
                );
            }
            Err(e) => {
                eprintln!("[Renderer] text layout: {e}");
                self.glyph_instances.clear();
                self.caret_pipeline.upload_caret(&self.queue, None);
                self.editor_cursor_overlay_pipeline.upload_instances(
                    &self.device,
                    &self.queue,
                    &[],
                );
            }
        }

        self.text_pipeline
            .upload_instances(&self.device, &self.queue, &self.glyph_instances);
        self.update_editor_gutter(
            app_state,
            center_bounds,
            geometry.line_height,
            geometry.font_size,
            app_state.total_lines().max(1).to_string().len().max(3),
            geometry.gutter_width,
        );
    }

    /// Fast path for cursor movement: reuse existing layout and update caret only.
    ///
    /// Must honor the same mode → shape mapping as `update_editor_content`, otherwise
    /// h/j/k/l in Normal mode would collapse the block caret back to a thin bar.
    pub fn update_editor_caret(&mut self, app_state: &AppState, center_bounds: [f32; 4]) {
        let geometry = editor_viewport_geometry(self, app_state, center_bounds);

        let caret_layout = compute_caret_layout(
            &self.text_system,
            app_state,
            [geometry.origin_x, geometry.origin_y],
        );
        let caret = caret_rect_for_mode(
            caret_layout,
            app_state.current_mode(),
            self.theme.editor.cursor.as_f32(),
            self.theme.editor.font_size,
            self.cursor_shape,
            self.cursor_beam_width,
            self.cursor_block_width,
            self.cursor_underline_height,
        );
        self.caret_pipeline.upload_caret(&self.queue, Some(caret));

        let overlay = if should_draw_block_cursor(app_state.current_mode(), self.cursor_shape) {
            compute_cursor_overlay(
                &mut self.text_system,
                app_state,
                &mut self.atlas,
                &self.queue,
                [geometry.origin_x, geometry.origin_y],
                self.theme.editor.bg.as_f32(),
            )
            .unwrap_or(None)
        } else {
            None
        };
        let overlay_instances: Vec<GlyphInstance> = overlay.into_iter().collect();
        self.editor_cursor_overlay_pipeline.upload_instances(
            &self.device,
            &self.queue,
            &overlay_instances,
        );

        self.update_editor_gutter(
            app_state,
            center_bounds,
            geometry.line_height,
            geometry.font_size,
            app_state.total_lines().max(1).to_string().len().max(3),
            geometry.gutter_width,
        );
    }
}
