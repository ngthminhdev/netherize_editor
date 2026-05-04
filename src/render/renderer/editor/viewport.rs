#![allow(unused_imports)]

use crate::{
    app::app_state::{
        AppState, CompletionDisplayItem, DiagnosticsState, EditorOverlay, FloatingBoxBlock,
        FloatingBoxStyle, HelpState, ImageBuffer, OverlayColorToken, ReferencesBufferState,
        SettingItem, SettingsState,
    },
    async_runtime::message::LspDiagnostic,
    config::theme_config::{ThemeConfig, linear_rgba_to_srgb_u8},
    core::mode::EditorMode,
    render::{
        glyph_instance::GlyphInstance, region_pipeline::RegionDrawInstance, renderer::Renderer,
    },
    text::layout_sync::{
        compute_caret_layout, compute_cursor_overlay, rebuild_layout_projection,
        visual_y_for_logical_scroll,
    },
};
use cosmic_text::Metrics;

use super::super::helpers::{
    caret_rect_for_mode, clamp_monospace_text, estimate_monospace_width, gutter_width_for_editor,
    layout_panel_rich_text, layout_panel_text, layout_panel_text_italic, rect_to_scissor,
    should_draw_block_cursor,
};
use super::{cursor_diagnostic, editor_viewport_geometry, run_x_for_byte, wrap_text_lines};
use crate::text::text_system::StyledTextSpan;

/// Quick fingerprint để phát hiện thay đổi trong syntax / diagnostic spans.
/// Không cần hoàn hảo — chỉ cần bắt được phần lớn thay đổi thực tế.
fn spans_fingerprint(spans: &[StyledTextSpan]) -> u64 {
    let mut h: u64 = spans.len() as u64 ^ 0xcbf29ce484222325;
    for (i, s) in spans.iter().take(32).enumerate() {
        h ^= (s.start as u64)
            .wrapping_mul(0x9e3779b97f4a7c15)
            .wrapping_add(i as u64);
        h = h.rotate_left(17).wrapping_add(s.end as u64);
    }
    h
}

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
        self.last_editor_chrome_instances.clear();
        self.clear_editor_overlays();
        self.clear_diagnostic_hover_popup();
        self.editor_scissor = None;
        self.image_pipeline.clear();
        self.image_scissor = None;
        // Invalidate text cache so the next update_editor_content always reshapes.
        self.last_shaped_revision = u64::MAX;
        self.last_shaped_spans_fingerprint = u64::MAX;
        self.last_shaped_viewport_width = 0.0;
    }

    pub fn update_image_content(&mut self, image: &ImageBuffer, center_bounds: [f32; 4]) {
        self.clear_buffer_terminal();
        self.clear_welcome_logo();
        self.clear_editor_content();
        self.editor_scissor = rect_to_scissor(center_bounds);
        self.image_scissor = rect_to_scissor(center_bounds);

        if let (Some(rgba), 1..) = (image.rgba.as_deref(), image.width) {
            let padding = 24.0;
            let avail_w = (center_bounds[2] - padding * 2.0).max(1.0);
            let avail_h = (center_bounds[3] - padding * 2.0).max(1.0);
            let scale = (avail_w / image.width as f32)
                .min(avail_h / image.height.max(1) as f32)
                .min(1.0);
            let draw_w = image.width as f32 * scale;
            let draw_h = image.height as f32 * scale;
            let rect = [
                center_bounds[0] + (center_bounds[2] - draw_w) * 0.5,
                center_bounds[1] + (center_bounds[3] - draw_h) * 0.5,
                draw_w,
                draw_h,
            ];
            self.image_pipeline.upload_rgba(
                &self.device,
                &self.queue,
                rgba,
                image.width,
                image.height,
                rect,
                [
                    self.surface_state.config.width,
                    self.surface_state.config.height,
                ],
            );
        }

        let status = image
            .error
            .clone()
            .unwrap_or_else(|| format!("{} × {} px · fit to viewport", image.width, image.height));
        self.editor_overlay_scissor = rect_to_scissor(center_bounds);
        self.editor_overlay_glyph_instances = layout_panel_text(
            &status,
            &mut self.editor_overlay_text_system,
            &mut self.atlas,
            &self.queue,
            center_bounds[0] + self.editor_padding_x,
            center_bounds[1] + self.editor_padding_y,
            if image.error.is_some() {
                self.theme.ui.error.as_f32()
            } else {
                self.theme.ui.fg_ghost.as_f32()
            },
        );
        self.editor_overlay_text_pipeline.upload_instances(
            &self.device,
            &self.queue,
            &self.editor_overlay_glyph_instances,
        );
    }

    /// Rebuild glyph instances and caret for the center editor region.
    pub fn update_editor_content(
        &mut self,
        text: &str,
        app_state: &AppState,
        center_bounds: [f32; 4],
        spans: &[StyledTextSpan],
    ) {
        // Text/scratch/no-tab surfaces share the center viewport with image buffers.
        // Always drop any previously uploaded image texture/quad before rebuilding
        // text content; otherwise closing the last image tab can leave the final
        // image visible behind the now-empty editor surface.
        self.image_pipeline.clear();
        self.image_scissor = None;

        let geometry = editor_viewport_geometry(self, app_state, center_bounds);
        let width = geometry.viewport_text_width;

        self.editor_scissor = rect_to_scissor(center_bounds);
        // Allow cosmic-text to shape full height; scissor clips the visible region.
        self.text_system.set_size(Some(width), None);

        // Pre-shape the current text so that visual_y_for_logical_scroll can walk the
        // real LayoutRun positions.  Without this, origin_y would be computed with
        // current_scroll_y * line_height which is wrong whenever any logical line above
        // the scroll position has been soft-wrapped into multiple visual rows (e.g. a
        // JWT token).  rebuild_layout_projection reshapes the same text again — this is
        // intentional: the second call collects glyphs with the now-correct origin.
        let text_fg = self.theme.editor.fg.as_f32();
        let default_color_rgba = linear_rgba_to_srgb_u8(text_fg);

        // ── Tối ưu 2: Text Caching ─────────────────────────────────────────────
        // Trong các frame chỉ cuộn (smooth scroll), text revision không đổi →
        // TextSystem buffer đã được shaped từ frame trước → bỏ qua set_text_with_spans.
        // Reshape khi: (a) text thay đổi, (b) syntax/LSP spans thay đổi, hoặc
        // (c) viewport width thay đổi (word-wrap boundary shift).
        let current_revision = app_state.revision();
        let spans_fp = spans_fingerprint(spans);
        let needs_reshape = self.last_shaped_revision != current_revision
            || self.last_shaped_spans_fingerprint != spans_fp
            || (self.last_shaped_viewport_width - width).abs() > 0.5;
        if needs_reshape {
            self.text_system
                .set_text_with_spans(text, default_color_rgba, spans);
            self.last_shaped_revision = current_revision;
            self.last_shaped_spans_fingerprint = spans_fp;
            self.last_shaped_viewport_width = width;
        } else {
            self.text_system.set_size(Some(width), None);
        }

        let visual_scroll_y =
            visual_y_for_logical_scroll(&self.text_system, app_state.current_scroll_y.max(0.0));
        let corrected_origin_y =
            center_bounds[1] + self.editor_padding_y + geometry.line_height - visual_scroll_y;

        // Cull glyphs ngoài viewport (overscan 1 dòng mỗi đầu) — file 10k dòng
        // chỉ build instance cho ~100 dòng visible thay vì toàn buffer.
        let clip_top = geometry.viewport_text_top - geometry.line_height;
        let clip_bottom =
            geometry.viewport_text_top + geometry.viewport_text_height + geometry.line_height;
        let viewport_clip = Some((clip_top, clip_bottom));

        let result = rebuild_layout_projection(
            text,
            app_state,
            &mut self.text_system,
            &mut self.atlas,
            &self.queue,
            [geometry.origin_x, corrected_origin_y],
            viewport_clip,
            text_fg,
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
        self.image_pipeline.clear();
        self.image_scissor = None;

        let geometry = editor_viewport_geometry(self, app_state, center_bounds);

        // The text_system buffer is already shaped from the last update_editor_content call.
        // Use it to compute the correct visual scroll Y (accounts for wrapped long lines).
        let visual_scroll_y =
            visual_y_for_logical_scroll(&self.text_system, app_state.current_scroll_y.max(0.0));
        let corrected_origin_y =
            center_bounds[1] + self.editor_padding_y + geometry.line_height - visual_scroll_y;

        let caret_layout = compute_caret_layout(
            &self.text_system,
            app_state,
            [geometry.origin_x, corrected_origin_y],
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
                [geometry.origin_x, corrected_origin_y],
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

    /// Collect ghost text glyph instances for the inline AI suggestion.
    /// Returns an empty Vec if there is no active suggestion or it is empty.
    /// The caller is responsible for uploading the returned glyphs together with
    /// any other overlay content so they share a single pipeline upload.
    pub(super) fn collect_inline_suggestion_glyphs(
        &mut self,
        app_state: &AppState,
        origin_x: f32,
        origin_y: f32,
        width: f32,
    ) -> Vec<GlyphInstance> {
        let Some(suggestion) = app_state.inline_suggestion() else {
            return Vec::new();
        };
        let first_line = suggestion.lines().next().unwrap_or_default();
        if first_line.is_empty() {
            return Vec::new();
        }

        self.editor_overlay_text_system.set_metrics(Metrics::new(
            self.theme.editor.font_size,
            self.theme.editor.line_height,
        ));
        self.editor_overlay_text_system.set_size(Some(width), None);
        let color =
            crate::config::theme_config::linear_rgba_to_srgb_u8(self.theme.ui.fg_ghost.as_f32());
        self.editor_overlay_text_system
            .set_text_with_color(first_line, color);

        let caret = compute_caret_layout(&self.text_system, app_state, [origin_x, origin_y]);
        let raw_glyphs = self.editor_overlay_text_system.collect_visible_glyphs(
            caret.x,
            caret.top,
            self.theme.ui.fg_ghost.as_f32(),
            None,
        );
        let mut instances = Vec::with_capacity(raw_glyphs.len());
        for glyph in raw_glyphs {
            let entry = if let Some(entry) = self.atlas.get(glyph.cache_key) {
                entry
            } else {
                let Some(rasterized) = crate::text::raster::rasterize_glyph_alpha(
                    &mut self.editor_overlay_text_system,
                    glyph.cache_key,
                ) else {
                    continue;
                };
                let Ok(entry) = self.atlas.get_or_reserve(glyph.cache_key, &rasterized) else {
                    continue;
                };
                entry
            };
            if entry.region.width == 0 || entry.region.height == 0 {
                continue;
            }
            let (uv_min, uv_max) = self.atlas.uv_min_max(entry.region);
            let top_left_x = glyph.physical_x + entry.placement_left;
            let top_left_y = glyph.physical_y - entry.placement_top;
            instances.push(GlyphInstance::new(
                [top_left_x as f32, top_left_y as f32],
                [entry.region.width as f32, entry.region.height as f32],
                uv_min,
                uv_max,
                glyph.color,
            ));
        }
        self.atlas.flush_pending(&self.queue);
        instances
    }
}
