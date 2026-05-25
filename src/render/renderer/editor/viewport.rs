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
        compute_caret_layout_at_with_folds, compute_caret_layout_with_folds,
        compute_cursor_overlay, rebuild_layout_projection, visual_y_for_logical_scroll_with_folds,
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
fn inline_suggestion_virtual_gap(app_state: &AppState, line_height: f32) -> Option<(usize, f32)> {
    const MAX_INLINE_SUGGESTION_LINES: usize = 6;

    let suggestion = app_state.inline_suggestion()?;
    let rendered_lines = suggestion
        .split('\n')
        .take(MAX_INLINE_SUGGESTION_LINES)
        .count();
    let extra_lines = rendered_lines.saturating_sub(1);
    if extra_lines == 0 {
        return None;
    }
    let (cursor_line, _) = app_state.cursor_line_col();
    Some((cursor_line, extra_lines as f32 * line_height.max(1.0)))
}

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

/// Truncate auto-folded long lines to 100 chars + "..." before shaping.
/// This prevents wrapped display of folded lines.
fn truncate_folded_lines(
    text: &str,
    spans: &[StyledTextSpan],
    app_state: &AppState,
) -> (String, Vec<StyledTextSpan>) {
    const FOLD_TRUNCATE_LIMIT: usize = 100;

    let folded_ranges = app_state.folded_ranges();
    if folded_ranges.is_empty() {
        return (text.to_string(), spans.to_vec());
    }

    // Find auto-folded long lines (where start == end)
    let auto_folded_lines: Vec<usize> = folded_ranges
        .iter()
        .filter(|&&(s, e)| s == e)
        .map(|&(s, _)| s)
        .collect();

    if auto_folded_lines.is_empty() {
        return (text.to_string(), spans.to_vec());
    }

    // Build line-to-byte mapping for original text
    let mut line_byte_starts = vec![0];
    for line in text.lines() {
        let last = *line_byte_starts.last().unwrap();
        line_byte_starts.push(last + line.len() + 1); // +1 for newline
    }

    let mut result = String::with_capacity(text.len());
    let mut byte_offset_map: Vec<(usize, usize)> = Vec::new(); // (old_byte, new_byte)

    for (line_idx, line) in text.lines().enumerate() {
        let old_line_start = line_byte_starts[line_idx];
        let new_line_start = result.len();

        if auto_folded_lines.contains(&line_idx) {
            // Truncate this line to 100 chars + "..."
            let chars: Vec<char> = line.chars().collect();
            if chars.len() > FOLD_TRUNCATE_LIMIT {
                let truncated: String = chars.iter().take(FOLD_TRUNCATE_LIMIT).collect();
                let truncated_byte_len = truncated.len();

                // Map each byte position in the original line to the new position
                for old_byte in 0..=truncated_byte_len {
                    byte_offset_map.push((old_line_start + old_byte, new_line_start + old_byte));
                }
                // The truncation point: everything after maps to the end of "..."
                let ellipsis_end = new_line_start + truncated_byte_len + 3;
                for old_byte in (truncated_byte_len + 1)..=line.len() {
                    byte_offset_map.push((old_line_start + old_byte, ellipsis_end));
                }

                result.push_str(&truncated);
                result.push_str("...");
            } else {
                // Line is short enough, no truncation needed
                for old_byte in 0..=line.len() {
                    byte_offset_map.push((old_line_start + old_byte, new_line_start + old_byte));
                }
                result.push_str(line);
            }
        } else {
            // Not a folded line, copy as-is
            for old_byte in 0..=line.len() {
                byte_offset_map.push((old_line_start + old_byte, new_line_start + old_byte));
            }
            result.push_str(line);
        }

        // Add newline if not the last line
        if line_idx + 1 < text.lines().count() {
            result.push('\n');
            // Map the newline byte
            byte_offset_map.push((old_line_start + line.len(), result.len() - 1));
        }
    }

    // Sort the map for binary search
    byte_offset_map.sort_unstable_by_key(|&(old, _)| old);

    // Adjust spans using the byte offset map
    let mut adjusted_spans = Vec::with_capacity(spans.len());
    for span in spans {
        // Find new positions for start and end
        let new_start = byte_offset_map
            .binary_search_by_key(&span.start, |&(old, _)| old)
            .map(|idx| byte_offset_map[idx].1)
            .unwrap_or_else(|idx| {
                if idx > 0 {
                    byte_offset_map[idx - 1].1
                } else {
                    0
                }
            });

        let new_end = byte_offset_map
            .binary_search_by_key(&span.end, |&(old, _)| old)
            .map(|idx| byte_offset_map[idx].1)
            .unwrap_or_else(|idx| {
                if idx > 0 {
                    byte_offset_map[idx - 1].1
                } else {
                    0
                }
            });

        // Only keep spans that have valid ranges in the new text
        if new_start < new_end && new_end <= result.len() {
            adjusted_spans.push(StyledTextSpan {
                start: new_start,
                end: new_end,
                ..*span
            });
        }
    }

    (result, adjusted_spans)
}

impl Renderer {
    pub(crate) fn set_editor_breadcrumb_segments(
        &mut self,
        _segments: Vec<crate::render::renderer::EditorBreadcrumbSegment>,
    ) -> bool {
        // Temporary kill-switch: breadcrumb UI is disabled, so keep the render list empty.
        let had_segments = !self.editor_breadcrumb_segments.is_empty();
        self.editor_breadcrumb_segments.clear();
        had_segments
    }

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
        self.editor_scissor = rect_to_scissor([
            center_bounds[0],
            geometry.viewport_text_top,
            center_bounds[2],
            geometry.viewport_text_height,
        ]);
        // Allow cosmic-text to shape full height; scissor clips the visible region.
        self.text_system.set_size(Some(width), None);
        let tab_width = u16::from(app_state.indent_config().tab_width.max(1));
        let tab_width_changed = self.text_system.tab_width() != tab_width;
        if tab_width_changed {
            self.text_system.set_tab_width(tab_width);
        }

        // ── Tối ưu 2: Text Caching ─────────────────────────────────────────────
        // Trong các frame chỉ cuộn (smooth scroll), text revision không đổi →
        // TextSystem buffer đã được shaped từ frame trước → bỏ qua set_text_with_spans.
        // Reshape khi: (a) text thay đổi, (b) syntax/LSP spans thay đổi, hoặc
        // (c) viewport width/tab width thay đổi (word-wrap/tab-stop boundary shift).
        let text_fg = self.theme.editor.fg.as_f32();
        let default_color_rgba = linear_rgba_to_srgb_u8(text_fg);
        let current_revision = app_state.revision();
        let spans_fp = spans_fingerprint(spans);
        let needs_reshape = self.last_shaped_revision != current_revision
            || self.last_shaped_spans_fingerprint != spans_fp
            || (self.last_shaped_viewport_width - width).abs() > 0.5
            || tab_width_changed;
        if needs_reshape {
            // Truncate auto-folded long lines to 100 chars + "..." before shaping
            let (display_text, adjusted_spans) = truncate_folded_lines(text, spans, app_state);
            self.text_system
                .set_text_with_spans(&display_text, default_color_rgba, &adjusted_spans);
            self.last_shaped_revision = current_revision;
            self.last_shaped_spans_fingerprint = spans_fp;
            self.last_shaped_viewport_width = width;
        } else {
            self.text_system.set_size(Some(width), None);
        }

        let visual_scroll_y = visual_y_for_logical_scroll_with_folds(
            &self.text_system,
            app_state.current_scroll_y.max(0.0),
            app_state.folded_ranges(),
        );
        let corrected_origin_y =
            geometry.viewport_text_top + geometry.line_height - visual_scroll_y;

        // Cull glyphs ngoài viewport (overscan 1 dòng mỗi đầu) — file 10k dòng
        // chỉ build instance cho ~100 dòng visible thay vì toàn buffer.
        let virtual_gap_after_line = inline_suggestion_virtual_gap(app_state, geometry.line_height);
        let virtual_gap_y = virtual_gap_after_line.map(|(_, gap)| gap).unwrap_or(0.0);
        let clip_top = geometry.viewport_text_top;
        let clip_bottom = geometry.viewport_text_top
            + geometry.viewport_text_height
            + geometry.line_height
            + virtual_gap_y;
        let viewport_clip = Some((clip_top, clip_bottom));

        let result = rebuild_layout_projection(
            app_state,
            &mut self.text_system,
            &mut self.atlas,
            &self.queue,
            [geometry.origin_x, corrected_origin_y],
            viewport_clip,
            virtual_gap_after_line,
            self.theme.editor.fg.as_f32(),
            self.theme.editor.bg.as_f32(),
        );

        match result {
            Ok(projection) => {
                self.glyph_instances = projection.glyph_instances;
                let primary_caret = caret_rect_for_mode(
                    projection.caret_layout,
                    app_state.current_mode(),
                    self.theme.editor.cursor.as_f32(),
                    self.theme.editor.font_size,
                    self.cursor_shape,
                    self.cursor_beam_width,
                    self.cursor_block_width,
                    self.cursor_underline_height,
                );
                let caret_rects = build_caret_rects(
                    app_state,
                    &self.text_system,
                    primary_caret,
                    self.theme.editor.cursor.as_f32(),
                    self.theme.editor.font_size,
                    self.cursor_shape,
                    self.cursor_beam_width,
                    self.cursor_block_width,
                    self.cursor_underline_height,
                    [geometry.origin_x, corrected_origin_y],
                );
                self.caret_pipeline.upload_carets(&self.queue, &caret_rects);
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
            app_state
                .visible_line_count()
                .max(1)
                .to_string()
                .len()
                .max(3),
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
        let visual_scroll_y = visual_y_for_logical_scroll_with_folds(
            &self.text_system,
            app_state.current_scroll_y.max(0.0),
            app_state.folded_ranges(),
        );
        let corrected_origin_y =
            geometry.viewport_text_top + geometry.line_height - visual_scroll_y;

        let caret_layout = compute_caret_layout_with_folds(
            &self.text_system,
            app_state,
            [geometry.origin_x, corrected_origin_y],
            app_state.folded_ranges(),
        );
        let primary_caret = caret_rect_for_mode(
            caret_layout,
            app_state.current_mode(),
            self.theme.editor.cursor.as_f32(),
            self.theme.editor.font_size,
            self.cursor_shape,
            self.cursor_beam_width,
            self.cursor_block_width,
            self.cursor_underline_height,
        );
        let caret_rects = build_caret_rects(
            app_state,
            &self.text_system,
            primary_caret,
            self.theme.editor.cursor.as_f32(),
            self.theme.editor.font_size,
            self.cursor_shape,
            self.cursor_beam_width,
            self.cursor_block_width,
            self.cursor_underline_height,
            [geometry.origin_x, corrected_origin_y],
        );
        self.caret_pipeline.upload_carets(&self.queue, &caret_rects);

        let overlay = if should_draw_block_cursor(app_state.current_mode(), self.cursor_shape) {
            compute_cursor_overlay(
                &mut self.text_system,
                app_state,
                &mut self.atlas,
                &self.queue,
                [geometry.origin_x, corrected_origin_y],
                self.theme.editor.bg.as_f32(),
                app_state.folded_ranges(),
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
            app_state
                .visible_line_count()
                .max(1)
                .to_string()
                .len()
                .max(3),
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
        const MAX_INLINE_SUGGESTION_LINES: usize = 6;

        let caret = compute_caret_layout_with_folds(
            &self.text_system,
            app_state,
            [origin_x, origin_y],
            app_state.folded_ranges(),
        );
        let line_height = self.theme.editor.line_height.max(1.0);
        let color =
            crate::config::theme_config::linear_rgba_to_srgb_u8(self.theme.ui.fg_ghost.as_f32());
        let ghost_color = self.theme.ui.fg_ghost.as_f32();
        let mut instances = Vec::new();

        self.editor_overlay_text_system.set_metrics(Metrics::new(
            self.theme.editor.font_size,
            self.theme.editor.line_height,
        ));

        for (line_idx, line) in suggestion
            .split('\n')
            .take(MAX_INLINE_SUGGESTION_LINES)
            .enumerate()
        {
            if line.is_empty() {
                continue;
            }

            let (line_x, line_width) = if line_idx == 0 {
                let remaining_width = (origin_x + width - caret.x).max(1.0);
                (caret.x, remaining_width)
            } else {
                (origin_x, width.max(1.0))
            };
            let line_y = caret.top + line_idx as f32 * line_height;

            self.editor_overlay_text_system
                .set_size(Some(line_width), Some(line_height));
            self.editor_overlay_text_system
                .set_text_with_color(line, color);

            let raw_glyphs = self.editor_overlay_text_system.collect_visible_glyphs(
                line_x,
                line_y,
                ghost_color,
                None,
            );
            instances.reserve(raw_glyphs.len());
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
        }

        self.atlas.flush_pending(&self.queue);
        instances
    }
}

// ── Multi-cursor caret batching ───────────────────────────────────────────────

use crate::{
    config::ui_config::CursorShape, render::caret::CaretScreenRect, text::text_system::TextSystem,
};

/// Build the full list of caret rects: primary at index 0, then one per virtual
/// cursor.  All carets are uploaded in a single `upload_carets` call.
#[allow(clippy::too_many_arguments)]
fn build_caret_rects(
    app_state: &AppState,
    text_system: &TextSystem,
    primary_caret: CaretScreenRect,
    cursor_color: [f32; 4],
    font_size: f32,
    cursor_shape: CursorShape,
    beam_width: f32,
    block_width: f32,
    underline_height: f32,
    viewport_origin: [f32; 2],
) -> Vec<CaretScreenRect> {
    let mut rects = vec![primary_caret];

    // Only add virtual cursors when in MultiCursor / MultiInsert mode.
    let mode = app_state.current_mode();
    if !matches!(
        mode,
        crate::core::mode::EditorMode::MultiCursor | crate::core::mode::EditorMode::MultiInsert
    ) {
        return rects;
    }

    // Use a slightly dimmed color for virtual cursors so the primary stands out.
    let virtual_color = [
        cursor_color[0],
        cursor_color[1],
        cursor_color[2],
        cursor_color[3] * 0.7,
    ];

    for vc in app_state.virtual_cursors() {
        let (line_idx, byte_in_line) = app_state.char_idx_to_line_and_byte_in_line(vc.char_idx);
        let line_hidden = app_state.is_line_folded(line_idx);
        if line_hidden {
            continue;
        }
        let layout = compute_caret_layout_at_with_folds(
            text_system,
            line_idx,
            byte_in_line,
            viewport_origin,
            app_state.folded_ranges(),
        );
        let rect = caret_rect_for_mode(
            layout,
            mode,
            virtual_color,
            font_size,
            cursor_shape,
            beam_width,
            block_width,
            underline_height,
        );
        rects.push(rect);
    }

    rects
}
