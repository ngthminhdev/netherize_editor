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
        glyph_instance::GlyphInstance,
        icon_pipeline::{canonical_icon_id, IconDrawInstance},
        region_pipeline::RegionDrawInstance,
        renderer::Renderer,
    },
    text::layout_sync::{compute_caret_layout, compute_cursor_overlay, rebuild_layout_projection},
};
use cosmic_text::Metrics;

use super::super::super::helpers::{
    caret_rect_for_mode, clamp_monospace_text, estimate_monospace_width, gutter_width_for_editor,
    layout_panel_rich_text, layout_panel_text, layout_panel_text_bold, layout_panel_text_italic,
    rect_to_scissor, should_draw_block_cursor,
};
use super::super::{cursor_diagnostic, editor_viewport_geometry, run_x_for_byte, wrap_text_lines};
use super::grouped_list_window_start;
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
        let gap = 0.0;
        let left_w = (panel_w * 0.56).clamp(420.0, 720.0).min(panel_w * 0.62);
        let right_w = (panel_w - left_w - gap).max(1.0);
        let left_x = panel_x;
        let right_x = left_x + left_w + gap;
        let header_h = line_height * 2.0 + 18.0;
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
        let mut icons = Vec::new();
        let mut chrome = vec![
            RegionDrawInstance::new([left_x, panel_y, left_w, panel_h], panel_bg),
            RegionDrawInstance::new([right_x, panel_y, right_w, panel_h], editor_bg),
            RegionDrawInstance::new([right_x - 1.0, panel_y, 1.0, panel_h], divider),
            RegionDrawInstance::new([left_x, panel_y + header_h - 1.0, left_w, 1.0], divider),
            RegionDrawInstance::new([right_x, panel_y + header_h - 1.0, right_w, 1.0], divider),
        ];

        let header_y = panel_y + 6.0;
        let error_count = diagnostics
            .results
            .iter()
            .filter(|item| item.severity == Some(1))
            .count();
        let warning_count = diagnostics
            .results
            .iter()
            .filter(|item| item.severity == Some(2))
            .count();
        let info_count = diagnostics
            .results
            .iter()
            .filter(|item| item.severity == Some(3))
            .count();
        let hint_count = diagnostics
            .results
            .iter()
            .filter(|item| item.severity == Some(4))
            .count();
        let none_count = diagnostics
            .results
            .iter()
            .filter(|item| item.severity.is_none())
            .count();

        let unique_file_count = count_diagnostic_files(&diagnostics.results);
        let count_label = format!(
            "{} problems · {} files",
            diagnostics.results.len(),
            unique_file_count
        );
        let header_count_w =
            estimate_monospace_width(" 999 problems · 999 files", font_size).min(left_w * 0.58);
        self.editor_overlay_text_system
            .set_size(Some((left_w - 20.0).max(1.0)), Some(line_height));
        glyphs.extend(layout_panel_text_bold(
            "Problems",
            &mut self.editor_overlay_text_system,
            &mut self.atlas,
            &self.queue,
            left_x + 10.0,
            header_y,
            fg,
        ));
        glyphs.extend(layout_panel_text(
            &clamp_monospace_text(&count_label, header_count_w, font_size),
            &mut self.editor_overlay_text_system,
            &mut self.atlas,
            &self.queue,
            (left_x + left_w - header_count_w - 10.0).max(left_x + 100.0),
            header_y,
            fg_dim,
        ));

        if let Some(item) = diagnostics.results.get(diagnostics.selected_index) {
            let severity_label = match item.severity {
                Some(1) => "Error",
                Some(2) => "Warning",
                _ => "Info",
            };
            let severity_color = match item.severity {
                Some(1) => error,
                Some(2) => warning,
                _ => fg_dim,
            };
            let message_w = (right_w - 20.0).max(1.0);
            let location = format!(
                "{}:{}:{}",
                item.file_path.display(),
                item.line + 1,
                item.col + 1
            );
            self.editor_overlay_text_system
                .set_size(Some(message_w), Some(line_height));
            glyphs.extend(layout_panel_text_bold(
                severity_label,
                &mut self.editor_overlay_text_system,
                &mut self.atlas,
                &self.queue,
                right_x + 10.0,
                header_y,
                severity_color,
            ));
            let severity_w = estimate_monospace_width(severity_label, font_size) + 12.0;
            glyphs.extend(layout_panel_text(
                &clamp_monospace_text(&item.message, (message_w - severity_w).max(1.0), font_size),
                &mut self.editor_overlay_text_system,
                &mut self.atlas,
                &self.queue,
                right_x + 10.0 + severity_w,
                header_y,
                fg,
            ));
            glyphs.extend(layout_panel_text(
                &clamp_monospace_text(&location, message_w, font_size),
                &mut self.editor_overlay_text_system,
                &mut self.atlas,
                &self.queue,
                right_x + 10.0,
                header_y + line_height + 2.0,
                fg_ghost,
            ));
        } else {
            self.editor_overlay_text_system
                .set_size(Some((right_w - 20.0).max(1.0)), Some(line_height));
            glyphs.extend(layout_panel_text(
                "No diagnostic selected",
                &mut self.editor_overlay_text_system,
                &mut self.atlas,
                &self.queue,
                right_x + 10.0,
                header_y,
                fg_ghost,
            ));
        }

        let footer_y = panel_y + panel_h - footer_h;
        chrome.push(RegionDrawInstance::new(
            [left_x, footer_y, left_w, 1.0],
            divider,
        ));
        let footer_text = format!(
            "↑↓ navigate  |  Enter open diagnostic  |  Esc/Q close   • ×{}  ▲{}  i{}  •{}  ?{}",
            error_count, warning_count, info_count, hint_count, none_count
        );
        self.editor_overlay_text_system
            .set_size(Some((left_w - 20.0).max(1.0)), Some(line_height));
        glyphs.extend(layout_panel_text(
            &clamp_monospace_text(&footer_text, (left_w - 20.0).max(1.0), font_size),
            &mut self.editor_overlay_text_system,
            &mut self.atlas,
            &self.queue,
            left_x + 10.0,
            footer_y + 4.0,
            fg_ghost,
        ));

        let row_h = line_height + 8.0;
        let group_header_h = line_height + 7.0;
        let visible_rows = ((content_h / row_h).floor() as usize).max(1);
        let start = grouped_list_window_start(
            diagnostics.results.len(),
            diagnostics.selected_index,
            content_h,
            row_h,
            group_header_h,
            |left, right| {
                diagnostics.results[left].file_path == diagnostics.results[right].file_path
            },
            |_| false,
        );
        let left_text_width = (left_w - 20.0).max(1.0);
        let mut draw_y = content_top;
        let mut previous_group_path: Option<String> = None;
        let mut rendered_rows = 0usize;

        for item_idx in start..diagnostics.results.len() {
            if rendered_rows >= visible_rows || draw_y + row_h > content_bottom + 1.0 {
                break;
            }
            let item = &diagnostics.results[item_idx];
            let current_group_path = compact_diagnostic_path(&item.file_path.display().to_string());
            if previous_group_path.as_deref() != Some(current_group_path.as_str()) {
                if draw_y + group_header_h + row_h > content_bottom + 1.0 {
                    break;
                }
                let (file_name, folder_label) = split_diagnostic_path(&current_group_path);
                let mut group_bg = self.theme.ui.overlay_bg.as_f32();
                group_bg[3] = (group_bg[3] * 0.85).clamp(0.18, 0.45);
                chrome.push(RegionDrawInstance::new(
                    [
                        left_x + 8.0,
                        draw_y + 2.0,
                        (left_w - 16.0).max(1.0),
                        (line_height + 3.0).max(12.0),
                    ],
                    group_bg,
                ));
                let group_count = count_diagnostics_for_path(
                    &diagnostics.results,
                    &item.file_path.display().to_string(),
                );
                let count_label = format!("{}", group_count);
                let count_w = estimate_monospace_width(&count_label, font_size).max(16.0);
                glyphs.extend(layout_panel_text_bold(
                    &clamp_monospace_text(
                        &format!("▾ {}", file_name),
                        left_text_width * 0.50,
                        font_size,
                    ),
                    &mut self.editor_overlay_text_system,
                    &mut self.atlas,
                    &self.queue,
                    left_x + 16.0,
                    draw_y + 4.0,
                    self.theme.ui.accent.as_f32(),
                ));
                let folder_x = left_x + left_w * 0.54;
                glyphs.extend(layout_panel_text(
                    &clamp_monospace_text(
                        &folder_label,
                        (left_x + left_w - folder_x - count_w - 26.0).max(1.0),
                        font_size,
                    ),
                    &mut self.editor_overlay_text_system,
                    &mut self.atlas,
                    &self.queue,
                    folder_x,
                    draw_y + 4.0,
                    fg_ghost,
                ));
                glyphs.extend(layout_panel_text_bold(
                    &count_label,
                    &mut self.editor_overlay_text_system,
                    &mut self.atlas,
                    &self.queue,
                    left_x + left_w - count_w - 18.0,
                    draw_y + 4.0,
                    fg,
                ));
                draw_y += group_header_h;
                previous_group_path = Some(current_group_path);
            }

            let row_y = draw_y;
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

            let (severity_color, severity_icon_name) = match item.severity {
                Some(1) => (error, "built_in:error"),
                Some(2) => (warning, "built_in:warning"),
                Some(3) => (fg_dim, "built_in:info"),
                Some(4) => (fg_ghost, "built_in:info"),
                _ => (fg_dim, "built_in:info"),
            };

            // Draw severity icon
            if let Some(icon_id) = canonical_icon_id(severity_icon_name) {
                let icon_size = font_size;
                let icon_x = left_x + 16.0;
                let icon_y = row_y + (row_h - icon_size) * 0.5;
                icons.push(IconDrawInstance {
                    icon: icon_id,
                    rect: [icon_x, icon_y, icon_size, icon_size],
                    tint: severity_color,
                });
            }

            let line_label = format!("{}", item.line + 1);
            let line_w = estimate_monospace_width("9999", font_size).max(34.0);
            let line_x = left_x + 48.0;
            glyphs.extend(layout_panel_text(
                &line_label,
                &mut self.editor_overlay_text_system,
                &mut self.atlas,
                &self.queue,
                line_x,
                row_y + 4.0,
                severity_color,
            ));
            let message_x = line_x + line_w;
            let message_w = (left_x + left_w - message_x - 10.0).max(1.0);
            glyphs.extend(layout_panel_text(
                &clamp_monospace_text(&item.message, message_w, font_size),
                &mut self.editor_overlay_text_system,
                &mut self.atlas,
                &self.queue,
                message_x,
                row_y + 4.0,
                if is_selected { fg } else { fg_dim },
            ));

            draw_y += row_h;
            rendered_rows += 1;
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
        self.editor_overlay_icon_instances = icons;
        self.editor_overlay_icon_pipeline.upload_instances(
            &self.device,
            &self.editor_overlay_icon_instances,
            [
                self.surface_state.config.width,
                self.surface_state.config.height,
            ],
        );
        self.editor_overlay_text_pipeline.upload_instances(
            &self.device,
            &self.queue,
            &self.editor_overlay_glyph_instances,
        );
    }
}

fn split_diagnostic_path(path: &str) -> (String, String) {
    let normalized = compact_diagnostic_path(path);
    let Some((folder, file)) = normalized.rsplit_once('/') else {
        return (normalized, String::new());
    };
    (file.to_string(), folder.to_string())
}

fn compact_diagnostic_path(path: &str) -> String {
    let marker = "/src/";
    if let Some(index) = path.find(marker) {
        return path[index + 1..].to_string();
    }

    let components: Vec<&str> = path.split('/').filter(|part| !part.is_empty()).collect();
    if components.len() > 3 {
        components[components.len().saturating_sub(3)..].join("/")
    } else {
        path.to_string()
    }
}

fn count_diagnostics_for_path(
    results: &[crate::app::app_state::DiagnosticItem],
    path_label: &str,
) -> usize {
    results
        .iter()
        .filter(|item| item.file_path.display().to_string() == path_label)
        .count()
}

fn count_diagnostic_files(results: &[crate::app::app_state::DiagnosticItem]) -> usize {
    let mut paths = Vec::<String>::new();
    for item in results {
        let path = item.file_path.display().to_string();
        if !paths.iter().any(|existing| existing == &path) {
            paths.push(path);
        }
    }
    paths.len()
}
