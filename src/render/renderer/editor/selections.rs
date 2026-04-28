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

const DIAGNOSTIC_SEVERITY_ERROR: u32 = 1;
const DIAGNOSTIC_SEVERITY_WARNING: u32 = 2;
const EDITOR_FRAME_INSET: f32 = 4.0;

fn leading_indent_columns(app_state: &AppState, line_idx: usize, tab_width: usize) -> usize {
    let text = app_state.line_string(line_idx);
    let mut cols = 0usize;
    for ch in text.chars() {
        match ch {
            ' ' => cols += 1,
            '\t' => cols += tab_width.max(1),
            _ => break,
        }
    }
    cols
}

impl Renderer {
    pub fn indent_guide_quads(
        &self,
        app_state: &AppState,
        center_bounds: [f32; 4],
    ) -> Vec<RegionDrawInstance> {
        let gutter_inset_left = self.editor_padding_x + 6.0 + EDITOR_FRAME_INSET;
        let line_height = self.theme.editor.line_height;
        let font_size = self.theme.editor.font_size;
        let total_lines = app_state.total_lines().max(1);
        let gutter_digits = total_lines.to_string().len().max(3);
        let gutter_width = gutter_width_for_editor(gutter_digits, font_size, line_height);
        let text_area_x = center_bounds[0] + gutter_inset_left + gutter_width;
        let scroll_y = app_state.current_scroll_y * line_height;
        let scroll_x = app_state.scroll_column as f32 * (font_size * 0.6).max(1.0);
        let origin_y = center_bounds[1] + self.editor_padding_y + line_height - scroll_y;
        let viewport_top = center_bounds[1] + self.editor_padding_y;
        let viewport_bottom =
            viewport_top + (center_bounds[3] - self.editor_padding_y * 2.0).max(1.0);
        let tab_width = app_state.indent_config().tab_width as usize;
        let char_width = (font_size * 0.6).max(1.0);
        let guide_step = char_width * tab_width.max(1) as f32;
        if guide_step <= 0.0 {
            return Vec::new();
        }

        let mut color = self.theme.editor.indent_guide.as_f32();
        color[3] = color[3].clamp(0.08, 0.22);
        let mut quads = Vec::new();
        let mut last_drawn_line: Option<usize> = None;

        for run in self.text_system.buffer().layout_runs() {
            let line_idx = run.line_i;
            if last_drawn_line == Some(line_idx) {
                continue;
            }
            last_drawn_line = Some(line_idx);

            let line_top = origin_y + run.line_top;
            let line_height_px = run.line_height.max(1.0);
            let line_bottom = line_top + line_height_px;
            if line_bottom <= viewport_top || line_top >= viewport_bottom {
                continue;
            }

            let indent_columns = leading_indent_columns(app_state, line_idx, tab_width);
            let full_levels = indent_columns / tab_width.max(1);
            if full_levels == 0 {
                continue;
            }

            for level in 0..full_levels {
                let x = text_area_x - scroll_x + guide_step * level as f32 + guide_step * 0.5;
                quads.push(RegionDrawInstance::new(
                    [x, line_top + 1.0, 1.0, (line_height_px - 2.0).max(1.0)],
                    color,
                ));
            }
        }

        quads
    }

    pub fn current_line_highlight_quad(
        &self,
        app_state: &AppState,
        center_bounds: [f32; 4],
    ) -> Option<RegionDrawInstance> {
        let gutter_inset_left = self.editor_padding_x + 6.0 + EDITOR_FRAME_INSET;
        let line_height = self.theme.editor.line_height;
        let font_size = self.theme.editor.font_size;
        let total_lines = app_state.total_lines().max(1);
        let gutter_digits = total_lines.to_string().len().max(3);
        let gutter_width = gutter_width_for_editor(gutter_digits, font_size, line_height);
        let text_area_x = center_bounds[0] + gutter_inset_left + gutter_width;
        let text_area_w =
            (center_bounds[2] - gutter_inset_left - self.editor_padding_x - gutter_width).max(1.0);
        let scroll_y = app_state.current_scroll_y * line_height;
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
        let gutter_inset_left = self.editor_padding_x + 6.0 + EDITOR_FRAME_INSET;
        let text_area_x = center_bounds[0] + gutter_inset_left + gutter_width;
        let text_area_w =
            (center_bounds[2] - gutter_inset_left - self.editor_padding_x - gutter_width).max(1.0);
        let scroll_y = app_state.current_scroll_y * line_height;
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
        let scroll_y = app_state.current_scroll_y * line_height;
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

    pub fn diagnostic_underline_quads(
        &self,
        app_state: &AppState,
        center_bounds: [f32; 4],
    ) -> Vec<RegionDrawInstance> {
        let Some(path) = app_state.active_file() else {
            return Vec::new();
        };
        let Some(diagnostics) = app_state.diagnostics_for_path(path) else {
            return Vec::new();
        };

        let line_height = self.theme.editor.line_height;
        let font_size = self.theme.editor.font_size;
        let total_lines = app_state.total_lines().max(1);
        let gutter_digits = total_lines.to_string().len().max(3);
        let gutter_width = gutter_width_for_editor(gutter_digits, font_size, line_height);
        let text_area_x = center_bounds[0] + self.editor_padding_x + gutter_width;
        let text_area_w = (center_bounds[2] - self.editor_padding_x - gutter_width).max(1.0);
        let scroll_y = app_state.current_scroll_y * line_height;
        let origin_y = center_bounds[1] + self.editor_padding_y + line_height - scroll_y;
        let viewport_top = center_bounds[1] + self.editor_padding_y;
        let viewport_bottom =
            viewport_top + (center_bounds[3] - self.editor_padding_y * 2.0).max(1.0);

        let mut quads = Vec::new();
        for diagnostic in diagnostics {
            let severity = diagnostic.severity.unwrap_or(DIAGNOSTIC_SEVERITY_WARNING);
            if severity != DIAGNOSTIC_SEVERITY_ERROR && severity != DIAGNOSTIC_SEVERITY_WARNING {
                continue;
            }
            let color = if severity == DIAGNOSTIC_SEVERITY_ERROR {
                self.theme.ui.error.as_f32()
            } else {
                self.theme.ui.warning.as_f32()
            };

            let start_line = diagnostic.range.start.line as usize;
            let end_line = diagnostic.range.end.line as usize;

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
                let line_end_byte = app_state.line_content_end_byte_idx(run.line_i);
                let local_start = if run.line_i == start_line {
                    diagnostic.range.start.character as usize
                } else {
                    0
                };
                let mut local_end = if run.line_i == end_line {
                    diagnostic.range.end.character as usize
                } else {
                    line_end_byte.saturating_sub(line_start_byte)
                };
                if run.line_i == start_line && run.line_i == end_line && local_start >= local_end {
                    local_end = local_start.saturating_add(1);
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
                let width = (right - left).max(6.0);
                if severity == DIAGNOSTIC_SEVERITY_ERROR {
                    let underline_h = 2.0;
                    let underline_y = line_top + (line_height_px - underline_h).max(0.0);
                    quads.push(RegionDrawInstance::new(
                        [left, underline_y, width, underline_h],
                        color,
                    ));
                } else {
                    let mut warning_color = color;
                    warning_color[3] = warning_color[3].clamp(0.9, 1.0);
                    let top_h = 2.0;
                    let bottom_h = 1.0;
                    let gap = 1.0;
                    let bottom_y = line_top + (line_height_px - bottom_h).max(0.0);
                    let top_y = (bottom_y - gap - top_h).max(line_top);
                    quads.push(RegionDrawInstance::new(
                        [left, top_y, width, top_h],
                        warning_color,
                    ));
                    quads.push(RegionDrawInstance::new(
                        [left, bottom_y, width, bottom_h],
                        warning_color,
                    ));
                }
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
        let gutter_inset_left = self.editor_padding_x + 6.0;
        let total_lines = app_state.total_lines().max(1);
        let scroll_line = app_state.current_scroll_y.floor().max(0.0) as usize;
        let scroll_y = scroll_line as f32 * line_height;
        let (cursor_line, _) = app_state.cursor_line_col();
        let gutter_bg_color = self.theme.editor.gutter.as_f32();
        let gutter_text_color = self.theme.ui.fg_dim.as_f32();
        let gutter_active_color = self.theme.editor.gutter_active.as_f32();
        let gutter_x = center_bounds[0] + gutter_inset_left;

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
            [
                gutter_x,
                center_bounds[1] + EDITOR_FRAME_INSET,
                gutter_width,
                (center_bounds[3] - EDITOR_FRAME_INSET * 2.0).max(0.0),
            ],
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

            if let Some(diff_kind) = app_state
                .active_buffer_git_diff()
                .and_then(|diff| {
                    diff.ranges
                        .iter()
                        .find(|range| abs_line >= range.start_line && abs_line <= range.end_line)
                })
                .map(|range| range.kind)
            {
                let diff_color = match diff_kind {
                    crate::app::app_state::GitDiffKind::Added => {
                        self.theme.git.added_gutter.as_f32()
                    }
                    crate::app::app_state::GitDiffKind::Modified => {
                        self.theme.git.modified_gutter.as_f32()
                    }
                };
                quads.push(RegionDrawInstance::new(
                    [gutter_x + 2.0, line_top_y, 3.0, run.line_height.max(1.0)],
                    diff_color,
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
                gutter_text_color
            } else {
                gutter_active_color
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
        self.last_editor_chrome_instances = quads;
    }
}
