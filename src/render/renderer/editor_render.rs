//! Editor viewport rendering: content text, caret, gutter, visual selection,
//! current-line highlight.

use cosmic_text::Metrics;

use crate::{
    app::app_state::{AppState, EditorOverlay, OverlayColorToken},
    core::mode::EditorMode,
    render::{
        glyph_instance::GlyphInstance, region_pipeline::RegionDrawInstance, renderer::Renderer,
    },
    text::layout_sync::{compute_caret_layout, compute_cursor_overlay, rebuild_layout_projection},
};

use super::helpers::{
    caret_rect_for_mode, gutter_width_for_editor, layout_panel_text, layout_panel_text_italic,
    rect_to_scissor, should_draw_block_cursor,
};
use crate::text::text_system::StyledTextSpan;

fn run_x_for_byte(text_area_x: f32, run: &cosmic_text::LayoutRun, byte_in_line: usize) -> f32 {
    if run.glyphs.is_empty() {
        return text_area_x;
    }

    let mut x = text_area_x + run.line_w;
    for glyph in run.glyphs {
        let left = text_area_x + glyph.x;
        let right = left + glyph.w;
        if byte_in_line <= glyph.start {
            return left;
        }
        if byte_in_line < glyph.end {
            return left;
        }
        x = right;
    }
    x
}

struct EditorViewportGeometry {
    line_height: f32,
    font_size: f32,
    gutter_width: f32,
    viewport_text_left: f32,
    viewport_text_width: f32,
    origin_x: f32,
    origin_y: f32,
}

fn editor_viewport_geometry(
    renderer: &Renderer,
    app_state: &AppState,
    center_bounds: [f32; 4],
) -> EditorViewportGeometry {
    let line_height = renderer.theme.editor.line_height;
    let font_size = renderer.theme.editor.font_size;
    let total_lines = app_state.total_lines().max(1);
    let gutter_digits = total_lines.to_string().len().max(3);
    let gutter_width = gutter_width_for_editor(gutter_digits, font_size, line_height);
    let scroll_y = app_state.scroll_line as f32 * line_height;
    let scroll_x = app_state.scroll_column as f32 * (font_size * 0.6).max(1.0);
    let viewport_text_left = center_bounds[0] + renderer.editor_padding_x + gutter_width;
    let origin_x = viewport_text_left - scroll_x;
    let origin_y = center_bounds[1] + renderer.editor_padding_y + line_height - scroll_y;
    let viewport_text_width =
        (center_bounds[2] - renderer.editor_padding_x - gutter_width).max(1.0);

    EditorViewportGeometry {
        line_height,
        font_size,
        gutter_width,
        viewport_text_left,
        viewport_text_width,
        origin_x,
        origin_y,
    }
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
        self.clear_editor_overlays();
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

    pub fn clear_editor_overlays(&mut self) {
        self.editor_overlay_scissor = None;
        self.editor_overlay_glyph_instances.clear();
        self.editor_overlay_text_pipeline
            .upload_instances(&self.device, &self.queue, &[]);
    }

    pub fn update_editor_overlays(&mut self, app_state: &AppState, center_bounds: [f32; 4]) {
        if app_state.current_overlays().is_empty() {
            self.clear_editor_overlays();
            return;
        }

        let geometry = editor_viewport_geometry(self, app_state, center_bounds);
        let viewport_top = center_bounds[1] + self.editor_padding_y;
        let viewport_bottom =
            viewport_top + (center_bounds[3] - self.editor_padding_y * 2.0).max(1.0);
        let viewport_right = center_bounds[0] + center_bounds[2] - self.editor_padding_x;

        self.editor_overlay_scissor = rect_to_scissor(center_bounds);
        let mut glyphs = Vec::new();

        for overlay in app_state.current_overlays() {
            let EditorOverlay::VirtualText {
                line,
                column: _,
                text,
                color_token,
            } = overlay;
            let line_end_byte = app_state.line_content_end_byte_idx(*line);
            let line_start_byte = app_state.line_start_byte_idx(*line);
            let byte_in_line = line_end_byte.saturating_sub(line_start_byte);

            let mut line_top: Option<f32> = None;
            let mut line_height_px = geometry.line_height.max(1.0);
            let mut tail_x = geometry.origin_x;

            for run in self.text_system.buffer().layout_runs() {
                if run.line_i != *line {
                    continue;
                }
                let candidate_top = geometry.origin_y + run.line_top;
                let candidate_bottom = candidate_top + run.line_height.max(1.0);
                if candidate_bottom <= viewport_top || candidate_top >= viewport_bottom {
                    continue;
                }

                line_top = Some(candidate_top);
                line_height_px = run.line_height.max(1.0);
                tail_x = tail_x.max(run_x_for_byte(geometry.origin_x, &run, byte_in_line));
            }

            let Some(line_top) = line_top else {
                continue;
            };

            let origin_x = (tail_x + 10.0).max(geometry.viewport_text_left + 4.0);
            if origin_x >= viewport_right {
                continue;
            }

            let width = (viewport_right - origin_x).max(1.0);
            self.editor_overlay_text_system
                .set_size(Some(width), Some(line_height_px));
            glyphs.extend(layout_panel_text_italic(
                text,
                &mut self.editor_overlay_text_system,
                &mut self.atlas,
                &self.queue,
                origin_x,
                line_top,
                match color_token {
                    OverlayColorToken::UiFgGhost => self.theme.ui.fg_ghost.as_f32(),
                },
            ));
        }

        self.editor_overlay_glyph_instances = glyphs;
        self.editor_overlay_text_pipeline.upload_instances(
            &self.device,
            &self.queue,
            &self.editor_overlay_glyph_instances,
        );
    }

    /// Returns a quad for the current-line highlight (drawn behind text).
    pub fn current_line_highlight_quad(
        &self,
        app_state: &AppState,
        center_bounds: [f32; 4],
    ) -> Option<RegionDrawInstance> {
        let line_height = self.theme.editor.line_height;
        let font_size = self.theme.editor.font_size;
        let total_lines = app_state.total_lines().max(1);
        let gutter_digits = total_lines.to_string().len().max(3);
        let gutter_width = gutter_width_for_editor(gutter_digits, font_size, line_height);
        let text_area_x = center_bounds[0] + self.editor_padding_x + gutter_width;
        let text_area_w = (center_bounds[2] - self.editor_padding_x - gutter_width).max(1.0);
        let scroll_y = app_state.scroll_line as f32 * line_height;
        let origin_y = center_bounds[1] + self.editor_padding_y + line_height - scroll_y;
        let caret_layout =
            compute_caret_layout(&self.text_system, app_state, [text_area_x, origin_y]);

        let viewport_top = center_bounds[1] + self.editor_padding_y;
        let viewport_bottom =
            viewport_top + (center_bounds[3] - self.editor_padding_y * 2.0).max(1.0);
        let line_top = caret_layout.top;
        let line_bottom = line_top + caret_layout.height.max(1.0);
        if line_bottom <= viewport_top || line_top >= viewport_bottom {
            return None;
        }

        let mut color = self.theme.editor.selection.as_f32();
        color[3] = (color[3] * 0.22).clamp(0.10, 0.30);
        Some(RegionDrawInstance::new(
            [
                text_area_x,
                line_top,
                text_area_w,
                caret_layout.height.max(1.0),
            ],
            color,
        ))
    }

    /// Returns per-line selection quads for Visual mode (empty outside Visual).
    pub fn visual_selection_quads(
        &self,
        app_state: &AppState,
        center_bounds: [f32; 4],
    ) -> Vec<RegionDrawInstance> {
        if app_state.current_mode() != EditorMode::Visual {
            return Vec::new();
        }
        let Some(selection) = app_state.visual_selection_range() else {
            return Vec::new();
        };

        let line_height = self.theme.editor.line_height;
        let font_size = self.theme.editor.font_size;
        let total_lines = app_state.total_lines().max(1);
        let gutter_digits = total_lines.to_string().len().max(3);
        let gutter_width = gutter_width_for_editor(gutter_digits, font_size, line_height);
        let text_area_x = center_bounds[0] + self.editor_padding_x + gutter_width;
        let text_area_w = (center_bounds[2] - self.editor_padding_x - gutter_width).max(1.0);
        let scroll_y = app_state.scroll_line as f32 * line_height;
        let origin_y = center_bounds[1] + self.editor_padding_y + line_height - scroll_y;

        let viewport_top = center_bounds[1] + self.editor_padding_y;
        let viewport_bottom =
            viewport_top + (center_bounds[3] - self.editor_padding_y * 2.0).max(1.0);

        let mut color = self.theme.editor.selection.as_f32();
        color[3] = (color[3] * 0.45).clamp(0.18, 0.42);

        let mut quads = Vec::new();
        for run in self.text_system.buffer().layout_runs() {
            if run.line_i < selection.start_line || run.line_i > selection.end_line {
                continue;
            }
            let line_top = origin_y + run.line_top;
            let line_height_px = run.line_height.max(1.0);
            let line_bottom = line_top + line_height_px;
            if line_bottom <= viewport_top || line_top >= viewport_bottom {
                continue;
            }

            let line_start_x = text_area_x;
            let line_end_x = (text_area_x + run.line_w).max(line_start_x + 1.0);
            let start_x = if run.line_i == selection.start_line {
                run_x_for_byte(text_area_x, &run, selection.start_byte_in_line)
            } else {
                line_start_x
            };
            let end_x = if run.line_i == selection.end_line {
                run_x_for_byte(text_area_x, &run, selection.end_byte_in_line)
            } else {
                line_end_x
            };

            let left = start_x.min(end_x).max(text_area_x);
            let right = start_x.max(end_x).min(text_area_x + text_area_w);
            let width = (right - left).max(1.0);
            quads.push(RegionDrawInstance::new(
                [left, line_top, width, line_height_px],
                color,
            ));
        }

        quads
    }

    /// Returns per-match quads for `/` and `*` search highlights in the active buffer.
    pub fn search_highlight_quads(
        &self,
        app_state: &AppState,
        center_bounds: [f32; 4],
    ) -> Vec<RegionDrawInstance> {
        if app_state.search_highlights().is_empty() {
            return Vec::new();
        }

        let line_height = self.theme.editor.line_height;
        let font_size = self.theme.editor.font_size;
        let total_lines = app_state.total_lines().max(1);
        let gutter_digits = total_lines.to_string().len().max(3);
        let gutter_width = gutter_width_for_editor(gutter_digits, font_size, line_height);
        let text_area_x = center_bounds[0] + self.editor_padding_x + gutter_width;
        let text_area_w = (center_bounds[2] - self.editor_padding_x - gutter_width).max(1.0);
        let scroll_y = app_state.scroll_line as f32 * line_height;
        let origin_y = center_bounds[1] + self.editor_padding_y + line_height - scroll_y;

        let viewport_top = center_bounds[1] + self.editor_padding_y;
        let viewport_bottom =
            viewport_top + (center_bounds[3] - self.editor_padding_y * 2.0).max(1.0);

        let mut color = self.theme.ui.warning.as_f32();
        color[3] = color[3].clamp(0.26, 0.38);

        let mut quads = Vec::new();
        for &(start_byte, end_byte) in app_state.search_highlights() {
            if start_byte >= end_byte {
                continue;
            }

            let start_line = app_state.byte_to_line_idx(start_byte);
            let end_line = app_state.byte_to_line_idx(end_byte.saturating_sub(1));

            for run in self.text_system.buffer().layout_runs() {
                if run.line_i < start_line || run.line_i > end_line {
                    continue;
                }

                let line_top = origin_y + run.line_top;
                let line_height_px = run.line_height.max(1.0);
                let line_bottom = line_top + line_height_px;
                if line_bottom <= viewport_top || line_top >= viewport_bottom {
                    continue;
                }

                let line_start_byte = app_state.line_start_byte_idx(run.line_i);
                let line_end_byte = app_state.line_end_byte_idx(run.line_i);
                let local_start = if run.line_i == start_line {
                    start_byte.saturating_sub(line_start_byte)
                } else {
                    0
                };
                let local_end = if run.line_i == end_line {
                    end_byte.saturating_sub(line_start_byte)
                } else {
                    line_end_byte.saturating_sub(line_start_byte)
                };
                if local_start >= local_end {
                    continue;
                }

                let line_start_x = text_area_x;
                let line_end_x = (text_area_x + run.line_w).max(line_start_x + 1.0);
                let start_x = if run.line_i == start_line {
                    run_x_for_byte(text_area_x, &run, local_start)
                } else {
                    line_start_x
                };
                let end_x = if run.line_i == end_line {
                    run_x_for_byte(text_area_x, &run, local_end)
                } else {
                    line_end_x
                };

                let left = start_x.min(end_x).max(text_area_x);
                let right = start_x.max(end_x).min(text_area_x + text_area_w);
                let width = (right - left).max(2.0);
                quads.push(RegionDrawInstance::new(
                    [left, line_top, width, line_height_px],
                    color,
                ));
            }
        }

        quads
    }

    /// Render line numbers in the gutter. Called by both `update_editor_content`
    /// and `update_editor_caret`.
    pub(super) fn update_editor_gutter(
        &mut self,
        app_state: &AppState,
        center_bounds: [f32; 4],
        line_height: f32,
        font_size: f32,
        gutter_digits: usize,
        gutter_width: f32,
    ) {
        let total_lines = app_state.total_lines().max(1);
        let scroll_line = app_state.scroll_line;
        let scroll_y = scroll_line as f32 * line_height;
        let (cursor_line, _) = app_state.cursor_line_col();
        let gutter_bg_color = self.theme.editor.gutter.as_f32();
        let gutter_text_color = self.theme.ui.fg_dim.as_f32();
        let gutter_active_color = self.theme.editor.gutter_active.as_f32();
        let gutter_x = center_bounds[0] + self.editor_padding_x;

        let gutter_font_size = (font_size + 3.0).min(line_height - 2.0).max(8.0);
        self.gutter_text_system
            .set_metrics(Metrics::new(gutter_font_size, line_height));
        self.gutter_text_system
            .set_size(Some(gutter_width), Some(line_height));

        let origin_y = center_bounds[1] + self.editor_padding_y + line_height - scroll_y;
        let viewport_top = center_bounds[1];
        let viewport_bottom = center_bounds[1] + center_bounds[3];

        let mut gutter_glyphs: Vec<GlyphInstance> = Vec::new();
        let mut quads: Vec<RegionDrawInstance> = Vec::new();

        // Clear gutter background to avoid stale pixels from previous frame.
        quads.push(RegionDrawInstance::new(
            [gutter_x, center_bounds[1], gutter_width, center_bounds[3]],
            gutter_bg_color,
        ));

        let active_line_color = {
            let mut c = self.theme.editor.selection.as_f32();
            c[3] = 0.22;
            c
        };

        // Only draw the first run of each logical line (skip soft-wrap continuations).
        let mut last_drawn_line: Option<usize> = None;
        for run in self.text_system.buffer().layout_runs() {
            let abs_line = run.line_i;
            if abs_line >= total_lines {
                break;
            }
            if last_drawn_line == Some(abs_line) {
                continue;
            }

            let line_top_y = origin_y + run.line_top;
            if line_top_y + line_height < viewport_top {
                continue;
            }
            if line_top_y > viewport_bottom {
                break;
            }

            last_drawn_line = Some(abs_line);

            if abs_line == cursor_line {
                quads.push(RegionDrawInstance::new(
                    [gutter_x, line_top_y, gutter_width, run.line_height.max(1.0)],
                    active_line_color,
                ));
            }

            let num_str = if self.relative_numbers {
                let dist = abs_line.abs_diff(cursor_line);
                if dist == 0 {
                    format!("{}", abs_line + 1)
                } else {
                    format!("{dist}")
                }
            } else {
                format!("{}", abs_line + 1)
            };
            let label = format!("{:>width$} ", num_str, width = gutter_digits);
            let color = if abs_line == cursor_line {
                gutter_active_color
            } else {
                gutter_text_color
            };

            gutter_glyphs.extend(layout_panel_text(
                &label,
                &mut self.gutter_text_system,
                &mut self.atlas,
                &self.queue,
                gutter_x,
                line_top_y,
                color,
            ));
        }

        self.gutter_glyph_instances = gutter_glyphs;
        self.gutter_text_pipeline.upload_instances(
            &self.device,
            &self.queue,
            &self.gutter_glyph_instances,
        );
        // Gutter background quads uploaded to region_pipeline (drawn before text in render()).
        self.region_pipeline
            .upload_instances(&self.device, &self.queue, &quads);
    }
}
