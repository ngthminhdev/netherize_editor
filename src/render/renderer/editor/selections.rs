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
    text::layout_sync::{
        compute_caret_layout_with_folds, compute_cursor_overlay, rebuild_layout_projection,
        visual_y_for_logical_scroll_with_folds,
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

const DIAGNOSTIC_SEVERITY_ERROR: u32 = 1;
const DIAGNOSTIC_SEVERITY_WARNING: u32 = 2;
const DIAGNOSTIC_SEVERITY_INFO: u32 = 3;
const DIAGNOSTIC_SEVERITY_HINT: u32 = 4;
const GUTTER_BG_RIGHT_TRIM: f32 = 10.0;
const GIT_GUTTER_MARKER_LEFT_INSET: f32 = -10.0;
const GIT_GUTTER_MARKER_WIDTH: f32 = 6.0;
const GIT_GUTTER_DELETED_MARKER_HEIGHT: f32 = 2.0;
const FOLD_ICON_LEFT_INSET: f32 = -18.0;

fn leading_indent_info(app_state: &AppState, line_idx: usize, tab_width: usize) -> (usize, bool) {
    let text = app_state.line_string(line_idx);
    let mut cols = 0usize;
    for ch in text.chars() {
        match ch {
            ' ' => cols += 1,
            '\t' => cols += tab_width.max(1),
            '\r' | '\n' => return (cols, false),
            _ => return (cols, true),
        }
    }
    (cols, false)
}

impl Renderer {
    pub fn indent_guide_quads(
        &self,
        app_state: &AppState,
        center_bounds: [f32; 4],
    ) -> Vec<RegionDrawInstance> {
        let geometry = editor_viewport_geometry(self, app_state, center_bounds);
        let line_height = geometry.line_height;
        let font_size = geometry.font_size;
        let text_area_x = geometry.viewport_text_left;
        let scroll_y = visual_y_for_logical_scroll_with_folds(
            &self.text_system,
            app_state.current_scroll_y,
            app_state.folded_ranges(),
        );
        let scroll_x = app_state.scroll_column as f32 * (font_size * 0.6).max(1.0);
        let origin_x = text_area_x - scroll_x;
        let origin_y = geometry.viewport_text_top + line_height - scroll_y;
        let viewport_top = geometry.viewport_text_top;
        let viewport_bottom = viewport_top + geometry.viewport_text_height.max(1.0);
        let tab_width = app_state.indent_config().tab_width as usize;
        let char_width = (font_size * 0.6).max(1.0);
        let guide_step = char_width * tab_width.max(1) as f32;

        let mut color = self.theme.editor.indent_guide.as_f32();
        color[3] = color[3].clamp(0.08, 0.22);
        let mut quads = Vec::new();
        let mut last_drawn_line: Option<usize> = None;
        let folded_ranges = app_state.folded_ranges();

        for run in self.text_system.buffer().layout_runs() {
            let line_idx = run.line_i;
            if last_drawn_line == Some(line_idx) {
                continue;
            }
            last_drawn_line = Some(line_idx);

            if !folded_ranges.is_empty() && app_state.is_line_folded(line_idx) {
                continue;
            }

            let line_top = origin_y + run.line_top
                - app_state.folded_visual_y_offset_before(line_idx, run.line_height);
            let line_height_px = run.line_height.max(1.0);
            let line_bottom = line_top + line_height_px;
            if line_bottom <= viewport_top || line_top >= viewport_bottom {
                continue;
            }

            let (indent_columns, has_text_after_indent) =
                leading_indent_info(app_state, line_idx, tab_width);
            let full_levels = indent_columns / tab_width.max(1);

            // Skip lines with no indentation
            if full_levels == 0 {
                continue;
            }

            for level in 1..=full_levels {
                let guide_column = level * tab_width.max(1);
                if has_text_after_indent && guide_column == indent_columns {
                    continue;
                }

                let x = origin_x + guide_step * level as f32;
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
        let geometry = editor_viewport_geometry(self, app_state, center_bounds);
        let text_area_x = geometry.viewport_text_left;
        let text_area_w = geometry.viewport_text_width;
        let scroll_y = visual_y_for_logical_scroll_with_folds(
            &self.text_system,
            app_state.current_scroll_y,
            app_state.folded_ranges(),
        );
        let origin_y = geometry.viewport_text_top + geometry.line_height - scroll_y;
        let caret_layout = compute_caret_layout_with_folds(
            &self.text_system,
            app_state,
            [text_area_x, origin_y],
            app_state.folded_ranges(),
        );

        let viewport_top = geometry.viewport_text_top;
        let viewport_bottom = viewport_top + geometry.viewport_text_height.max(1.0);
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

        let geometry = editor_viewport_geometry(self, app_state, center_bounds);
        let line_height = geometry.line_height;
        let text_area_x = geometry.viewport_text_left;
        let text_area_w = geometry.viewport_text_width;
        let viewport_top = geometry.viewport_text_top;
        let viewport_bottom = viewport_top + geometry.viewport_text_height.max(1.0);

        let mut color = self.theme.editor.selection.as_f32();
        color[3] = (color[3] * 0.45).clamp(0.18, 0.42);

        let scroll_y_px = visual_y_for_logical_scroll_with_folds(
            &self.text_system,
            app_state.current_scroll_y,
            app_state.folded_ranges(),
        );
        let origin_y = geometry.viewport_text_top + line_height - scroll_y_px;

        let mut quads = Vec::new();
        for run in self.text_system.buffer().layout_runs() {
            if run.line_i < selection.start_line || run.line_i > selection.end_line {
                continue;
            }
            if app_state.is_line_folded(run.line_i) {
                continue;
            }
            let line_top = origin_y + run.line_top
                - app_state.folded_visual_y_offset_before(run.line_i, run.line_height);
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

    /// Returns rectangular per-line selection quads for Visual Block mode.
    pub fn visual_block_selection_quads(
        &self,
        app_state: &AppState,
        center_bounds: [f32; 4],
    ) -> Vec<RegionDrawInstance> {
        if app_state.current_mode() != EditorMode::VisualBlock {
            return Vec::new();
        }
        let Some(block) = app_state.visual_block_range() else {
            return Vec::new();
        };

        let geometry = editor_viewport_geometry(self, app_state, center_bounds);
        let line_height = geometry.line_height;
        let text_area_x = geometry.viewport_text_left;
        let text_area_w = geometry.viewport_text_width;
        let viewport_top = geometry.viewport_text_top;
        let viewport_bottom = viewport_top + geometry.viewport_text_height.max(1.0);
        let char_width = (geometry.font_size * 0.6).max(1.0);

        let mut color = self.theme.editor.selection.as_f32();
        color[3] = (color[3] * 0.45).clamp(0.18, 0.42);

        let scroll_y_px = visual_y_for_logical_scroll_with_folds(
            &self.text_system,
            app_state.current_scroll_y,
            app_state.folded_ranges(),
        );
        let origin_y = geometry.viewport_text_top + line_height - scroll_y_px;

        let mut quads = Vec::new();
        for run in self.text_system.buffer().layout_runs() {
            if run.line_i < block.start_line || run.line_i > block.end_line {
                continue;
            }
            if app_state.is_line_folded(run.line_i) {
                continue;
            }

            let line_top = origin_y + run.line_top
                - app_state.folded_visual_y_offset_before(run.line_i, run.line_height);
            let line_height_px = run.line_height.max(1.0);
            let line_bottom = line_top + line_height_px;
            if line_bottom <= viewport_top || line_top >= viewport_bottom {
                continue;
            }

            let line_start_byte = app_state.line_start_byte_idx(run.line_i);
            let line_end_byte = app_state.line_content_end_byte_idx(run.line_i);
            let local_start = app_state
                .line_char_to_byte_idx(run.line_i, block.start_col)
                .saturating_sub(line_start_byte);
            let local_end = app_state
                .line_char_to_byte_idx(run.line_i, block.end_col.saturating_add(1))
                .saturating_sub(line_start_byte);

            let run_start = run.glyphs.first().map(|glyph| glyph.start).unwrap_or(0);
            let run_end = run
                .glyphs
                .last()
                .map(|glyph| glyph.end)
                .unwrap_or_else(|| line_end_byte.saturating_sub(line_start_byte));

            if local_end <= run_start || local_start >= run_end {
                continue;
            }

            let clipped_start = local_start.max(run_start);
            let clipped_end = local_end.min(run_end);
            let start_x = run_x_for_byte(text_area_x, &run, clipped_start);
            let mut end_x = run_x_for_byte(text_area_x, &run, clipped_end);
            if clipped_start == clipped_end {
                end_x = start_x + char_width;
            }

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

    /// Returns per-match highlight quads for all multi-cursor selections (primary + virtual).
    pub fn multi_cursor_selection_quads(
        &self,
        app_state: &AppState,
        center_bounds: [f32; 4],
    ) -> Vec<RegionDrawInstance> {
        let selections = app_state.multi_cursor_selection_ranges();
        if selections.is_empty() {
            return Vec::new();
        }

        let geometry = editor_viewport_geometry(self, app_state, center_bounds);
        let line_height = geometry.line_height;
        let text_area_x = geometry.viewport_text_left;
        let text_area_w = geometry.viewport_text_width;
        // Use visual_y_for_logical_scroll to match the actual rendered text Y (handles soft-wrapped lines).
        let scroll_y_px = visual_y_for_logical_scroll_with_folds(
            &self.text_system,
            app_state.current_scroll_y,
            app_state.folded_ranges(),
        );
        let origin_y = geometry.viewport_text_top + line_height - scroll_y_px;
        let viewport_top = geometry.viewport_text_top;
        let viewport_bottom = viewport_top + geometry.viewport_text_height.max(1.0);

        let base = self.theme.editor.selection.as_f32();
        let color = [base[0], base[1], base[2], 0.30];

        let mut quads = Vec::new();
        for selection in &selections {
            for run in self.text_system.buffer().layout_runs() {
                if run.line_i < selection.start_line || run.line_i > selection.end_line {
                    continue;
                }
                if app_state.is_line_folded(run.line_i) {
                    continue;
                }
                let line_top = origin_y + run.line_top
                    - app_state.folded_visual_y_offset_before(run.line_i, run.line_height);
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
        }

        quads
    }

    fn byte_range_highlight_quads(
        &self,
        app_state: &AppState,
        center_bounds: [f32; 4],
        ranges: &[(usize, usize)],
        color: [f32; 4],
    ) -> Vec<RegionDrawInstance> {
        if ranges.is_empty() {
            return Vec::new();
        }

        let geometry = editor_viewport_geometry(self, app_state, center_bounds);
        let line_height = geometry.line_height;
        let text_area_x = geometry.viewport_text_left;
        let text_area_w = geometry.viewport_text_width;
        let scroll_y = visual_y_for_logical_scroll_with_folds(
            &self.text_system,
            app_state.current_scroll_y,
            app_state.folded_ranges(),
        );
        let origin_y = geometry.viewport_text_top + line_height - scroll_y;

        let viewport_top = geometry.viewport_text_top;
        let viewport_bottom = viewport_top + geometry.viewport_text_height.max(1.0);

        let mut quads = Vec::new();
        for &(start_byte, end_byte) in ranges {
            if start_byte >= end_byte {
                continue;
            }

            let start_line = app_state.byte_to_line_idx(start_byte);
            let end_line = app_state.byte_to_line_idx(end_byte.saturating_sub(1));

            for run in self.text_system.buffer().layout_runs() {
                if run.line_i < start_line || run.line_i > end_line {
                    continue;
                }
                if app_state.is_line_folded(run.line_i) {
                    continue;
                }

                let line_top = origin_y + run.line_top
                    - app_state.folded_visual_y_offset_before(run.line_i, run.line_height);
                let line_height_px = run.line_height.max(1.0);
                let line_bottom = line_top + line_height_px;
                if line_bottom <= viewport_top || line_top >= viewport_bottom {
                    continue;
                }

                let line_start_byte = app_state.line_start_byte_idx(run.line_i);
                let line_end_byte = app_state.line_end_byte_idx(run.line_i);
                let mut local_start = if run.line_i == start_line {
                    start_byte.saturating_sub(line_start_byte)
                } else {
                    0
                };
                let mut local_end = if run.line_i == end_line {
                    end_byte.saturating_sub(line_start_byte)
                } else {
                    line_end_byte.saturating_sub(line_start_byte)
                };

                // A single logical line can be split into multiple cosmic-text layout runs
                // by soft-wrap. Only draw the part of the byte range that intersects this
                // visual run; otherwise a symbol on a wrapped continuation row can be painted
                // on the first visual row of the same logical line.
                if let (Some(first), Some(last)) = (run.glyphs.first(), run.glyphs.last()) {
                    let run_start = first.start;
                    let run_end = last.end;
                    local_start = local_start.max(run_start);
                    local_end = local_end.min(run_end);
                }

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

    pub fn semantic_symbol_highlight_quads(
        &self,
        app_state: &AppState,
        center_bounds: [f32; 4],
    ) -> Vec<RegionDrawInstance> {
        // Use fg (white-ish) for the background tint so it stays visible on dark themes.
        let mut bg_color = self.theme.ui.fg.as_f32();
        bg_color[3] = 0.05;
        let mut quads = self.byte_range_highlight_quads(
            app_state,
            center_bounds,
            app_state.semantic_symbol_highlights(),
            bg_color,
        );
        // Accent-colored underline for clear visual identification.
        let mut underline_color = self.theme.ui.accent.as_f32();
        underline_color[3] = 0.85;
        let underline_h = 2.0f32;
        let underlines: Vec<RegionDrawInstance> = quads
            .iter()
            .map(|q| {
                let [x, y, w, h] = q.rect;
                RegionDrawInstance::new([x, y + h - underline_h, w, underline_h], underline_color)
            })
            .collect();
        quads.extend(underlines);
        quads
    }

    /// Returns per-match quads for `/` and `*` search highlights in the active buffer.
    pub fn search_highlight_quads(
        &self,
        app_state: &AppState,
        center_bounds: [f32; 4],
    ) -> Vec<RegionDrawInstance> {
        let mut color = self.theme.ui.warning.as_f32();
        color[3] = color[3].clamp(0.26, 0.38);
        self.byte_range_highlight_quads(
            app_state,
            center_bounds,
            app_state.search_highlights(),
            color,
        )
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

        let geometry = editor_viewport_geometry(self, app_state, center_bounds);
        let line_height = geometry.line_height;
        let text_area_x = geometry.viewport_text_left;
        let text_area_w = geometry.viewport_text_width;
        let scroll_y = visual_y_for_logical_scroll_with_folds(
            &self.text_system,
            app_state.current_scroll_y,
            app_state.folded_ranges(),
        );
        let origin_y = geometry.viewport_text_top + line_height - scroll_y;
        let viewport_top = geometry.viewport_text_top;
        let viewport_bottom = viewport_top + geometry.viewport_text_height.max(1.0);

        let mut quads = Vec::new();
        for diagnostic in diagnostics {
            let severity = diagnostic.severity.unwrap_or(DIAGNOSTIC_SEVERITY_WARNING);
            if severity < 1 || severity > 4 {
                continue;
            }
            let color = match severity {
                DIAGNOSTIC_SEVERITY_ERROR => self.theme.ui.error.as_f32(),
                DIAGNOSTIC_SEVERITY_WARNING => self.theme.ui.warning.as_f32(),
                DIAGNOSTIC_SEVERITY_INFO => self.theme.ui.info.as_f32(),
                DIAGNOSTIC_SEVERITY_HINT => {
                    let mut hint_c = self.theme.ui.fg_ghost.as_f32();
                    hint_c[3] = 0.48; // Very subtle for hints/unused
                    hint_c
                }
                _ => self.theme.ui.fg_ghost.as_f32(),
            };

            let start_line = diagnostic.range.start.line as usize;
            let end_line = diagnostic.range.end.line as usize;

            for run in self.text_system.buffer().layout_runs() {
                if run.line_i < start_line || run.line_i > end_line {
                    continue;
                }
                if app_state.is_line_folded(run.line_i) {
                    continue;
                }

                let line_top = origin_y + run.line_top
                    - app_state.folded_visual_y_offset_before(run.line_i, run.line_height);
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
                } else if severity == DIAGNOSTIC_SEVERITY_WARNING {
                    let mut warning_color = color;
                    warning_color[3] = warning_color[3].clamp(0.9, 1.0);
                    let underline_h = 3.0;
                    let underline_y = line_top + (line_height_px - underline_h).max(0.0);
                    quads.push(RegionDrawInstance::new(
                        [left, underline_y, width, underline_h],
                        warning_color,
                    ));
                } else {
                    let mut info_color = color;
                    info_color[3] = info_color[3].clamp(0.35, 0.65);
                    let underline_h = 1.5;
                    let underline_y = line_top + (line_height_px - underline_h).max(0.0);
                    quads.push(RegionDrawInstance::new(
                        [left, underline_y, width, underline_h],
                        info_color,
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
        breakpoint_lines: &[usize],
    ) {
        let geometry = editor_viewport_geometry(self, app_state, center_bounds);
        let gutter_inset_left = self.editor_padding_x + 6.0;
        let (cursor_line, _) = app_state.cursor_line_col();
        let folded = app_state.folded_ranges();
        let has_folds = !folded.is_empty();

        let total_lines = app_state.total_lines().max(1);
        let gutter_bg_color = self.theme.editor.gutter.as_f32();
        let gutter_active_color = self.theme.editor.gutter_active.as_f32();
        let mut gutter_text_color = gutter_active_color;
        gutter_text_color[0] = gutter_text_color[0] * 0.72 + gutter_bg_color[0] * 0.28;
        gutter_text_color[1] = gutter_text_color[1] * 0.72 + gutter_bg_color[1] * 0.28;
        gutter_text_color[2] = gutter_text_color[2] * 0.72 + gutter_bg_color[2] * 0.28;
        gutter_text_color[3] = gutter_text_color[3].min(0.72);
        let gutter_x = center_bounds[0] + gutter_inset_left;
        let gutter_bg_width = (gutter_width - GUTTER_BG_RIGHT_TRIM).max(0.0);

        let gutter_font_size = (font_size - 1.0).min(line_height - 2.0).max(8.0);
        self.gutter_text_system
            .set_metrics(Metrics::new(gutter_font_size, line_height));
        self.gutter_text_system
            .set_size(Some(gutter_width), Some(line_height));

        // Use visual Y scroll derived from LayoutRun positions so that soft-wrapped
        // logical lines (e.g. a JWT token) do not shift all subsequent gutter numbers.
        let visual_scroll_y = visual_y_for_logical_scroll_with_folds(
            &self.text_system,
            app_state.current_scroll_y.max(0.0),
            app_state.folded_ranges(),
        );
        let origin_y = geometry.viewport_text_top + line_height - visual_scroll_y;
        let viewport_top = geometry.viewport_text_top;
        let viewport_bottom = viewport_top + geometry.viewport_text_height.max(1.0);
        let virtual_gap_y = app_state
            .inline_suggestion()
            .map(|suggestion| suggestion.split('\n').take(6).count().saturating_sub(1))
            .filter(|extra_lines| *extra_lines > 0)
            .map(|extra_lines| extra_lines as f32 * line_height.max(1.0))
            .unwrap_or(0.0);

        let mut gutter_glyphs: Vec<GlyphInstance> = Vec::new();
        let mut quads: Vec<RegionDrawInstance> = Vec::new();
        let git_line_statuses = app_state.active_buffer_git_line_statuses();

        // Clear gutter background to avoid stale pixels from previous frame.
        quads.push(RegionDrawInstance::new(
            [
                gutter_x,
                geometry.viewport_text_top,
                gutter_bg_width,
                geometry.viewport_text_height.max(0.0),
            ],
            gutter_bg_color,
        ));

        let active_line_color = {
            let mut c = self.theme.editor.selection.as_f32();
            c[3] = 0.22;
            c
        };

        // `last_seen_line` tracks the logical line of the PREVIOUS LayoutRun regardless
        // of viewport visibility, so continuation visual rows (soft-wrap) are never
        // mistakenly given a line number even when the first visual row of that logical
        // line was above the visible area.
        let mut last_seen_line: Option<usize> = None;

        for run in self.text_system.buffer().layout_runs() {
            let abs_line = run.line_i;
            if abs_line >= total_lines {
                break;
            }
            let is_fold_marker = has_folds && app_state.is_fold_marker_line(abs_line);
            if has_folds && app_state.is_line_folded(abs_line) {
                continue;
            }

            let y_offset = if has_folds {
                app_state.folded_visual_y_offset_before(abs_line, run.line_height)
            } else {
                0.0
            };

            let is_continuation = last_seen_line == Some(abs_line);
            last_seen_line = Some(abs_line);

            let line_top_y = origin_y + run.line_top - y_offset
                + if abs_line > cursor_line {
                    virtual_gap_y
                } else {
                    0.0
                };

            if is_continuation {
                // Continuation visual row: extend the cursor-line active highlight so
                // the entire wrapped block is highlighted, but draw no line number.
                if abs_line == cursor_line
                    && line_top_y + run.line_height >= viewport_top
                    && line_top_y <= viewport_bottom
                {
                    quads.push(RegionDrawInstance::new(
                        [gutter_x, line_top_y, gutter_width, run.line_height.max(1.0)],
                        active_line_color,
                    ));
                }
                continue;
            }

            // First visual row of this logical line.
            if line_top_y + run.line_height < viewport_top {
                continue;
            }
            if line_top_y > viewport_bottom {
                break;
            }

            // ── Fold handling ─────────────────────────────────────────────────
            if abs_line == cursor_line {
                quads.push(RegionDrawInstance::new(
                    [gutter_x, line_top_y, gutter_width, run.line_height.max(1.0)],
                    active_line_color,
                ));
            }

            if let Some(statuses) = git_line_statuses
                && let Some(status) = statuses.get(&abs_line)
            {
                let end_trim = ((run.line_height - gutter_font_size.min(run.line_height)).max(0.0)
                    * 0.5)
                    .max(0.0);
                let marker_x = gutter_x + GIT_GUTTER_MARKER_LEFT_INSET;

                let is_vertical_marker = |status: &crate::app::app_state::GitLineStatus| {
                    matches!(
                        status,
                        crate::app::app_state::GitLineStatus::Added
                            | crate::app::app_state::GitLineStatus::Modified
                    )
                };

                let (marker_top, marker_bottom) = if is_vertical_marker(status) {
                    let connects_above = abs_line
                        .checked_sub(1)
                        .and_then(|line| statuses.get(&line))
                        .is_some_and(is_vertical_marker);
                    let connects_below = statuses
                        .get(&(abs_line + 1))
                        .is_some_and(is_vertical_marker);

                    (
                        line_top_y + if connects_above { 0.0 } else { end_trim },
                        line_top_y + run.line_height - if connects_below { 0.0 } else { end_trim },
                    )
                } else {
                    (
                        line_top_y + end_trim,
                        line_top_y + run.line_height - end_trim,
                    )
                };

                let clipped_top = marker_top.max(viewport_top);
                let clipped_bottom = marker_bottom.min(viewport_bottom);
                let clipped_height = (clipped_bottom - clipped_top).max(0.0);

                if clipped_height > 0.0 {
                    let (rect, color) = match status {
                        crate::app::app_state::GitLineStatus::Added => (
                            [
                                marker_x,
                                clipped_top,
                                GIT_GUTTER_MARKER_WIDTH,
                                clipped_height,
                            ],
                            self.theme.git.added_gutter.as_f32(),
                        ),
                        crate::app::app_state::GitLineStatus::Modified => (
                            [
                                marker_x,
                                clipped_top,
                                GIT_GUTTER_MARKER_WIDTH,
                                clipped_height,
                            ],
                            self.theme.git.modified_gutter.as_f32(),
                        ),
                        crate::app::app_state::GitLineStatus::DeletedAbove => (
                            [
                                marker_x,
                                clipped_top,
                                GIT_GUTTER_MARKER_WIDTH,
                                GIT_GUTTER_DELETED_MARKER_HEIGHT.min(clipped_height),
                            ],
                            self.theme.git.deleted_gutter.as_f32(),
                        ),
                        crate::app::app_state::GitLineStatus::DeletedBelow => (
                            [
                                marker_x,
                                (clipped_top
                                    + (clipped_height - GIT_GUTTER_DELETED_MARKER_HEIGHT).max(0.0))
                                .max(clipped_top),
                                GIT_GUTTER_MARKER_WIDTH,
                                GIT_GUTTER_DELETED_MARKER_HEIGHT.min(clipped_height),
                            ],
                            self.theme.git.deleted_gutter.as_f32(),
                        ),
                    };
                    quads.push(RegionDrawInstance::new(rect, color));
                }
            }

            // ── Fold icon 󰡍 ───────────────────────────────────────────────────
            if has_folds && is_fold_marker {
                let folded_line_count =
                    app_state.folded_line_count_at_marker(abs_line).unwrap_or(0);
                if folded_line_count > 0 {
                    let icon_x = gutter_x + FOLD_ICON_LEFT_INSET;
                    let mut icon_color = gutter_text_color;
                    icon_color[3] = icon_color[3] * 0.6;
                    gutter_glyphs.extend(layout_panel_text(
                        "󰡍",
                        &mut self.gutter_text_system,
                        &mut self.atlas,
                        &self.queue,
                        icon_x,
                        line_top_y,
                        icon_color,
                    ));
                }
            }

            let num_str = if has_folds && is_fold_marker {
                let folded_line_count =
                    app_state.folded_line_count_at_marker(abs_line).unwrap_or(0);
                if folded_line_count > 0 {
                    if app_state.is_auto_folded_long_line(abs_line) {
                        format!("…")
                    } else {
                        format!("...{}", folded_line_count)
                    }
                } else {
                    format!("")
                }
            } else if self.relative_numbers {
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
            } else if has_folds && is_fold_marker {
                let mut fold_color = gutter_text_color;
                fold_color[3] = fold_color[3] * 0.7;
                fold_color
            } else {
                gutter_text_color
            };

            if breakpoint_lines.contains(&abs_line) {
                let dot_size = (run.line_height * 0.52).clamp(8.0, 12.0);
                let dot_x = gutter_x + 3.0;
                let dot_y = line_top_y + (run.line_height - dot_size) * 0.5;
                let dot_color = self.theme.ui.error.as_f32();
                quads.push(
                    RegionDrawInstance::new([dot_x, dot_y, dot_size, dot_size], dot_color)
                        .with_radius(dot_size * 0.5),
                );
            }

            gutter_glyphs.extend(layout_panel_text(
                &label,
                &mut self.gutter_text_system,
                &mut self.atlas,
                &self.queue,
                gutter_x + 14.0,
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
