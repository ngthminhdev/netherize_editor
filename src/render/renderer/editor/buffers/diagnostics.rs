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
use super::super::{cursor_diagnostic, editor_viewport_geometry, run_x_for_byte, wrap_text_lines};
use crate::text::text_system::StyledTextSpan;
impl Renderer {
    pub fn update_diagnostics_buffer_content(
        &mut self,
        diagnostics: &DiagnosticsState,
        center_bounds: [f32; 4],
    ) {
        if center_bounds[2] < 1.0 || center_bounds[3] < 1.0 {
            self.clear_editor_overlays();
            return;
        }

        let font_size = self.theme.editor.font_size;
        let line_height = self.theme.editor.line_height.max(font_size + 4.0);
        self.editor_overlay_text_system
            .set_metrics(Metrics::new(font_size, line_height));
        self.editor_overlay_scissor = rect_to_scissor(center_bounds);

        let pad_x = self.editor_padding_x.max(14.0);
        let pad_y = self.editor_padding_y.max(14.0);
        let panel_x = center_bounds[0] + pad_x;
        let panel_y = center_bounds[1] + pad_y;
        let panel_w = (center_bounds[2] - pad_x * 2.0).max(1.0);
        let panel_h = (center_bounds[3] - pad_y * 2.0).max(1.0);
        let gap = 16.0;
        let left_w = (panel_w * 0.5).max(1.0);
        let right_w = (panel_w - left_w - gap).max(1.0);
        let left_x = panel_x;
        let right_x = left_x + left_w + gap;
        let header_h = line_height + 14.0;
        let footer_h = line_height + 10.0;
        let content_top = panel_y + header_h;
        let content_bottom = (panel_y + panel_h - footer_h).max(content_top + line_height);
        let content_h = (content_bottom - content_top).max(line_height);

        let panel_bg = self.theme.ui.panel_bg.as_f32();
        let editor_bg = self.theme.editor.bg.as_f32();
        let mut divider = self.theme.ui.fg_ghost.as_f32();
        divider[3] = divider[3].clamp(0.28, 0.42);
        let accent = self.theme.ui.accent.as_f32();
        let warning = self.theme.ui.warning.as_f32();
        let error = [0.95, 0.32, 0.32, 1.0];
        let fg = self.theme.ui.fg.as_f32();
        let fg_dim = self.theme.ui.fg_dim.as_f32();
        let fg_ghost = self.theme.ui.fg_ghost.as_f32();
        let selection_bg = self.theme.ui.selection_bg.as_f32();

        let mut glyphs = Vec::new();
        let mut chrome = vec![
            RegionDrawInstance::new([left_x, panel_y, left_w, panel_h], panel_bg),
            RegionDrawInstance::new([right_x, panel_y, right_w, panel_h], editor_bg),
            RegionDrawInstance::new([right_x - gap * 0.5, panel_y, 1.0, panel_h], divider),
            RegionDrawInstance::new([left_x, panel_y + header_h - 1.0, left_w, 1.0], divider),
            RegionDrawInstance::new([right_x, panel_y + header_h - 1.0, right_w, 1.0], divider),
        ];

        let header_y = panel_y + 6.0;
        self.editor_overlay_text_system
            .set_size(Some((left_w - 20.0).max(1.0)), Some(line_height));
        glyphs.extend(layout_panel_text(
            "[Diagnostics]",
            &mut self.editor_overlay_text_system,
            &mut self.atlas,
            &self.queue,
            left_x + 10.0,
            header_y,
            fg,
        ));

        let right_header = diagnostics
            .results
            .get(diagnostics.selected_index)
            .map(|item| {
                format!(
                    "{}:{}:{}",
                    item.file_path.display(),
                    item.line + 1,
                    item.col + 1
                )
            })
            .unwrap_or_else(|| "No diagnostic selected".to_string());
        self.editor_overlay_text_system
            .set_size(Some((right_w - 20.0).max(1.0)), Some(line_height));
        glyphs.extend(layout_panel_text(
            &clamp_monospace_text(&right_header, (right_w - 20.0).max(1.0), font_size),
            &mut self.editor_overlay_text_system,
            &mut self.atlas,
            &self.queue,
            right_x + 10.0,
            header_y,
            fg,
        ));

        let row_h = line_height * 2.0 + 8.0;
        let visible_rows = (content_h / row_h).floor() as usize;
        let start = diagnostics.selected_index.saturating_sub(
            visible_rows
                .saturating_sub(1)
                .min(diagnostics.selected_index),
        );
        let left_text_width = (left_w - 24.0).max(1.0);

        for (slot, (item_idx, item)) in diagnostics
            .results
            .iter()
            .enumerate()
            .skip(start)
            .take(visible_rows.max(1))
            .enumerate()
        {
            let row_y = content_top + slot as f32 * row_h;
            let is_selected = item_idx == diagnostics.selected_index;
            if is_selected {
                chrome.push(RegionDrawInstance::new(
                    [left_x + 6.0, row_y, (left_w - 12.0).max(1.0), row_h - 4.0],
                    selection_bg,
                ));
                chrome.push(RegionDrawInstance::new(
                    [left_x + 6.0, row_y, 3.0, row_h - 4.0],
                    accent,
                ));
            }

            let severity_color = match item.severity {
                Some(1) => error,
                Some(2) => warning,
                _ => fg_dim,
            };
            let title = format!(
                "{}:{}:{}",
                item.file_path.display(),
                item.line + 1,
                item.col + 1
            );
            self.editor_overlay_text_system
                .set_size(Some(left_text_width), Some(line_height));
            glyphs.extend(layout_panel_text(
                &clamp_monospace_text(&title, left_text_width, font_size),
                &mut self.editor_overlay_text_system,
                &mut self.atlas,
                &self.queue,
                left_x + 14.0,
                row_y + 4.0,
                severity_color,
            ));
            self.editor_overlay_text_system
                .set_size(Some(left_text_width), Some(line_height));
            glyphs.extend(layout_panel_text(
                &clamp_monospace_text(&item.message, left_text_width, font_size),
                &mut self.editor_overlay_text_system,
                &mut self.atlas,
                &self.queue,
                left_x + 14.0,
                row_y + line_height + 4.0,
                if is_selected { fg } else { fg_ghost },
            ));
        }

        if !diagnostics.preview_lines.is_empty() {
            let line_number_width = estimate_monospace_width(
                &format!(
                    "{:>4}",
                    diagnostics
                        .preview_lines
                        .last()
                        .map(|line| line.line_number)
                        .unwrap_or(1)
                ),
                font_size,
            ) + 14.0;
            let preview_text_width = (right_w - 20.0 - line_number_width).max(1.0);
            let preview_rows = ((content_h / line_height).floor() as usize).max(1);
            let mut preview_start = 0usize;
            if let Some(target_idx) = diagnostics
                .preview_lines
                .iter()
                .position(|line| line.is_target)
            {
                preview_start = target_idx.saturating_sub(preview_rows / 2);
                if preview_start + preview_rows > diagnostics.preview_lines.len() {
                    preview_start = diagnostics.preview_lines.len().saturating_sub(preview_rows);
                }
            }

            for (slot, line) in diagnostics
                .preview_lines
                .iter()
                .skip(preview_start)
                .take(preview_rows)
                .enumerate()
            {
                let row_y = content_top + slot as f32 * line_height;
                if line.is_target {
                    chrome.push(RegionDrawInstance::new(
                        [right_x + 6.0, row_y, (right_w - 12.0).max(1.0), line_height],
                        selection_bg,
                    ));
                    chrome.push(RegionDrawInstance::new(
                        [right_x + 6.0, row_y, 3.0, line_height],
                        accent,
                    ));
                }

                self.editor_overlay_text_system
                    .set_size(Some(line_number_width), Some(line_height));
                glyphs.extend(layout_panel_text(
                    &format!("{:>4}", line.line_number),
                    &mut self.editor_overlay_text_system,
                    &mut self.atlas,
                    &self.queue,
                    right_x + 10.0,
                    row_y,
                    if line.is_target { warning } else { fg_ghost },
                ));

                self.editor_overlay_text_system
                    .set_size(Some(preview_text_width), Some(line_height));
                let line_start = diagnostics
                    .preview_lines
                    .iter()
                    .take(preview_start + slot)
                    .map(|item| item.text.len() + 1)
                    .sum::<usize>();
                let line_end = line_start + line.text.len();
                let line_spans: Vec<StyledTextSpan> = diagnostics
                    .preview_spans
                    .iter()
                    .filter_map(|span| {
                        if span.end <= line_start || span.start >= line_end {
                            return None;
                        }
                        Some(StyledTextSpan::with_style(
                            span.start.max(line_start) - line_start,
                            span.end.min(line_end) - line_start,
                            span.color_rgba,
                            span.bold,
                            span.italic,
                        ))
                    })
                    .collect();
                glyphs.extend(layout_panel_rich_text(
                    &clamp_monospace_text(&line.text, preview_text_width, font_size),
                    &line_spans,
                    if line.is_target { fg } else { fg_dim },
                    &mut self.editor_overlay_text_system,
                    &mut self.atlas,
                    &self.queue,
                    right_x + 10.0 + line_number_width,
                    row_y,
                ));
            }
        } else {
            self.editor_overlay_text_system
                .set_size(Some((right_w - 20.0).max(1.0)), Some(line_height));
            glyphs.extend(layout_panel_text(
                if diagnostics.results.is_empty() {
                    "No diagnostics"
                } else {
                    "Loading preview..."
                },
                &mut self.editor_overlay_text_system,
                &mut self.atlas,
                &self.queue,
                right_x + 10.0,
                content_top + 4.0,
                fg_ghost,
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
