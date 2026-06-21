#![allow(unused_imports)]

mod diagnostics;

use crate::{
    app::app_state::{
        AppState, CompletionDisplayItem, DiagnosticsState, EditorOverlay, FloatingBoxBlock,
        FloatingBoxStyle, HelpState, OverlayColorToken, ReferencesBufferItem,
        ReferencesBufferState, SettingItem, SettingsState,
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
use std::fmt::Write;

use super::super::helpers::{
    caret_rect_for_mode, clamp_monospace_text, estimate_monospace_width, gutter_width_for_editor,
    layout_panel_rich_text, layout_panel_text, layout_panel_text_bold, layout_panel_text_italic,
    rect_to_scissor, should_draw_block_cursor,
};
use super::{cursor_diagnostic, editor_viewport_geometry, run_x_for_byte, wrap_text_lines};
use crate::text::text_system::StyledTextSpan;
impl Renderer {
    pub fn clear_editor_overlays(&mut self) {
        self.editor_overlay_scissor = None;
        self.editor_overlay_chrome_instances.clear();
        self.editor_overlay_glyph_instances.clear();
        self.editor_overlay_text_pipeline
            .upload_instances(&self.device, &self.queue, &[]);
        self.editor_overlay_icon_instances.clear();
        self.editor_overlay_icon_pipeline.upload_instances(
            &self.device,
            &[],
            [
                self.surface_state.config.width,
                self.surface_state.config.height,
            ],
        );
    }

    pub fn update_references_buffer_content(
        &mut self,
        references: &ReferencesBufferState,
        center_bounds: [f32; 4],
    ) {
        if center_bounds[2] < 1.0 || center_bounds[3] < 1.0 {
            self.clear_editor_overlays();
            return;
        }

        // Scale hardcoded chrome px so paddings/offsets track the runtime-scaled
        // text metrics across monitors (same pattern as extensions.rs).
        let s = self.ui_scale.max(0.5);
        let font_size = self.theme.editor.font_size;
        let line_height = self.theme.editor.line_height.max(font_size + 4.0 * s);
        self.editor_overlay_text_system
            .set_metrics(Metrics::new(font_size, line_height));
        self.editor_overlay_scissor = rect_to_scissor(center_bounds);

        let pad_x = self.editor_padding_x.max(14.0 * s);
        let pad_y = self.editor_padding_y.max(14.0 * s);
        let panel_x = center_bounds[0] + pad_x;
        let panel_y = center_bounds[1] + pad_y;
        let panel_w = (center_bounds[2] - pad_x * 2.0).max(1.0);
        let panel_h = (center_bounds[3] - pad_y * 2.0).max(1.0);
        let gap = 0.0;
        let left_w = (panel_w * 0.56)
            .clamp(420.0 * s, 720.0 * s)
            .min(panel_w * 0.62);
        let right_w = (panel_w - left_w - gap).max(1.0);
        let left_x = panel_x;
        let right_x = left_x + left_w + gap;
        let header_h = line_height + 14.0 * s;
        let footer_h = line_height + 10.0 * s;
        let content_top = panel_y + header_h;
        let content_bottom = (panel_y + panel_h - footer_h).max(content_top + line_height);
        let content_h = (content_bottom - content_top).max(line_height);

        let panel_bg = self.theme.ui.panel_bg.as_f32();
        let editor_bg = self.theme.editor.bg.as_f32();
        let mut divider = self.theme.ui.fg_ghost.as_f32();
        divider[3] = divider[3].clamp(0.28, 0.42);
        let accent = self.theme.ui.accent.as_f32();
        let warning = self.theme.ui.warning.as_f32();
        let fg = self.theme.ui.fg.as_f32();
        let fg_dim = self.theme.ui.fg_dim.as_f32();
        let fg_ghost = self.theme.ui.fg_ghost.as_f32();
        let selection_bg = self.theme.ui.selection_bg.as_f32();

        let mut glyphs = Vec::new();
        let mut chrome = vec![
            RegionDrawInstance::new([left_x, panel_y, left_w, panel_h], panel_bg),
            RegionDrawInstance::new([right_x, panel_y, right_w, panel_h], editor_bg),
            RegionDrawInstance::new([right_x - 1.0, panel_y, 1.0, panel_h], divider),
            RegionDrawInstance::new([left_x, panel_y + header_h - 1.0, left_w, 1.0], divider),
            RegionDrawInstance::new([right_x, panel_y + header_h - 1.0, right_w, 1.0], divider),
        ];

        let header_y = panel_y + 6.0 * s;
        let unique_file_count = references.path_counts.len();
        let left_title = if references.loading {
            "References · loading..."
        } else {
            "References"
        };
        let count_label = &mut self.temp_string_buffer;
        count_label.clear();
        let _ = write!(
            count_label,
            "{} refs · {} files",
            references.items.len(),
            unique_file_count
        );
        let header_count_w =
            estimate_monospace_width(" 999 refs · 999 files", font_size).min(left_w * 0.55);
        self.editor_overlay_text_system
            .set_size(Some((left_w - 20.0 * s).max(1.0)), Some(line_height));
        glyphs.extend(layout_panel_text_bold(
            left_title,
            &mut self.editor_overlay_text_system,
            &mut self.atlas,
            &self.queue,
            left_x + 10.0 * s,
            header_y,
            fg,
        ));
        glyphs.extend(layout_panel_text(
            &clamp_monospace_text(&count_label, header_count_w, font_size),
            &mut self.editor_overlay_text_system,
            &mut self.atlas,
            &self.queue,
            (left_x + left_w - header_count_w - 10.0 * s).max(left_x + 100.0 * s),
            header_y,
            fg_dim,
        ));

        let selected = references.items.get(references.selected_index);
        let right_header_buffer = &mut self.temp_string_buffer_alt;
        right_header_buffer.clear();
        let right_header = if let Some(item) = selected {
            let _ = write!(
                right_header_buffer,
                "{}:{}:{}",
                item.relative_path,
                item.line + 1,
                item.column + 1
            );
            right_header_buffer.as_str()
        } else {
            references
                .status_message
                .as_deref()
                .unwrap_or("No reference selected")
        };
        self.editor_overlay_text_system
            .set_size(Some((right_w - 20.0 * s).max(1.0)), Some(line_height));
        glyphs.extend(layout_panel_text(
            &clamp_monospace_text(&right_header, (right_w - 20.0 * s).max(1.0), font_size),
            &mut self.editor_overlay_text_system,
            &mut self.atlas,
            &self.queue,
            right_x + 10.0 * s,
            header_y,
            fg,
        ));

        let help_text = "J/K / Ctrl+N/P / Up/Down to navigate  |  Enter to open  |  Esc/Q to close";
        self.editor_overlay_text_system
            .set_size(Some((panel_w - 20.0 * s).max(1.0)), Some(line_height));
        glyphs.extend(layout_panel_text(
            &clamp_monospace_text(help_text, (panel_w - 20.0 * s).max(1.0), font_size),
            &mut self.editor_overlay_text_system,
            &mut self.atlas,
            &self.queue,
            panel_x + 10.0 * s,
            panel_y + panel_h - footer_h + 4.0 * s,
            fg_ghost,
        ));

        let left_text_width = (left_w - 20.0 * s).max(1.0);
        if references.items.is_empty() {
            let status = references
                .status_message
                .as_deref()
                .unwrap_or("No references found");
            self.editor_overlay_text_system
                .set_size(Some(left_text_width), Some(line_height));
            glyphs.extend(layout_panel_text(
                status,
                &mut self.editor_overlay_text_system,
                &mut self.atlas,
                &self.queue,
                left_x + 14.0 * s,
                content_top + 4.0 * s,
                fg_ghost,
            ));
        } else {
            let row_h = line_height + 8.0 * s;
            let group_header_h = line_height + 7.0 * s;
            let visible_rows = ((content_h / row_h).floor() as usize).max(1);
            let start_idx = grouped_list_window_start(
                references.items.len(),
                references.selected_index,
                content_h,
                row_h,
                group_header_h,
                |left, right| {
                    references.items[left].relative_path == references.items[right].relative_path
                },
            );

            let mut draw_y = content_top;
            let mut previous_group_path: Option<&str> = None;
            let mut rendered_rows = 0usize;
            for item_idx in start_idx..references.items.len() {
                if rendered_rows >= visible_rows || draw_y + row_h > content_bottom + 1.0 {
                    break;
                }
                let item = &references.items[item_idx];
                if previous_group_path.as_deref() != Some(item.relative_path.as_str()) {
                    if draw_y + group_header_h + row_h > content_bottom + 1.0 {
                        break;
                    }
                    let (file_name, folder_label) = split_reference_path(&item.relative_path);
                    let mut group_bg = self.theme.ui.overlay_bg.as_f32();
                    group_bg[3] = (group_bg[3] * 0.85).clamp(0.18, 0.45);
                    chrome.push(RegionDrawInstance::new(
                        [
                            left_x + 8.0 * s,
                            draw_y + 2.0 * s,
                            (left_w - 16.0 * s).max(1.0),
                            (line_height + 3.0 * s).max(12.0 * s),
                        ],
                        group_bg,
                    ));
                    let group_count = references
                        .path_counts
                        .get(&item.relative_path)
                        .copied()
                        .unwrap_or(0);
                    count_label.clear();
                    let _ = write!(count_label, "{}", group_count);
                    let count_w = estimate_monospace_width(count_label, font_size).max(16.0 * s);
                    let is_collapsed = references.collapsed_paths.contains(&item.relative_path);
                    let indicator = if is_collapsed { "▸" } else { "▾" };
                    right_header_buffer.clear();
                    let _ = write!(right_header_buffer, "{} {}", indicator, file_name);
                    glyphs.extend(layout_panel_text_bold(
                        &clamp_monospace_text(
                            right_header_buffer,
                            left_text_width * 0.50,
                            font_size,
                        ),
                        &mut self.editor_overlay_text_system,
                        &mut self.atlas,
                        &self.queue,
                        left_x + 16.0 * s,
                        draw_y + 4.0 * s,
                        self.theme.ui.accent.as_f32(),
                    ));
                    let folder_x = left_x + left_w * 0.54;
                    glyphs.extend(layout_panel_text(
                        &clamp_monospace_text(
                            &folder_label,
                            (left_x + left_w - folder_x - count_w - 26.0 * s).max(1.0),
                            font_size,
                        ),
                        &mut self.editor_overlay_text_system,
                        &mut self.atlas,
                        &self.queue,
                        folder_x,
                        draw_y + 4.0 * s,
                        fg_ghost,
                    ));
                    glyphs.extend(layout_panel_text_bold(
                        count_label,
                        &mut self.editor_overlay_text_system,
                        &mut self.atlas,
                        &self.queue,
                        left_x + left_w - count_w - 18.0 * s,
                        draw_y + 4.0 * s,
                        fg,
                    ));
                    draw_y += group_header_h;
                    previous_group_path = Some(item.relative_path.as_str());

                    if is_collapsed {
                        continue;
                    }
                }

                let row_y = draw_y;
                let is_selected = item_idx == references.selected_index;
                if is_selected {
                    chrome.push(RegionDrawInstance::new(
                        [
                            left_x + 6.0 * s,
                            row_y,
                            (left_w - 12.0 * s).max(1.0),
                            row_h - 4.0 * s,
                        ],
                        selection_bg,
                    ));
                    chrome.push(RegionDrawInstance::new(
                        [left_x + 6.0 * s, row_y, 3.0 * s, row_h - 4.0 * s],
                        accent,
                    ));
                }

                count_label.clear();
                let _ = write!(count_label, "{}", item.line + 1);
                let line_w = estimate_monospace_width("9999", font_size).max(34.0 * s);
                glyphs.extend(layout_panel_text(
                    count_label,
                    &mut self.editor_overlay_text_system,
                    &mut self.atlas,
                    &self.queue,
                    left_x + 22.0 * s,
                    row_y + 4.0 * s,
                    if is_selected { fg } else { fg_ghost },
                ));
                let snippet_x = left_x + 22.0 * s + line_w;
                let snippet_w = (left_x + left_w - snippet_x - 10.0 * s).max(1.0);
                let snippet = reference_row_summary(item, right_header_buffer);
                glyphs.extend(layout_panel_text(
                    &clamp_monospace_text(snippet, snippet_w, font_size),
                    &mut self.editor_overlay_text_system,
                    &mut self.atlas,
                    &self.queue,
                    snippet_x,
                    row_y + 4.0 * s,
                    if is_selected { fg } else { fg_dim },
                ));

                draw_y += row_h;
                rendered_rows += 1;
            }
        }

        if !references.preview_lines.is_empty() {
            count_label.clear();
            let _ = write!(
                count_label,
                "{:>4}",
                references
                    .preview_lines
                    .last()
                    .map(|line| line.line_number)
                    .unwrap_or(1)
            );
            let line_number_width = estimate_monospace_width(count_label, font_size) + 14.0 * s;
            let preview_text_width = (right_w - 20.0 * s - line_number_width).max(1.0);
            let preview_rows = ((content_h / line_height).floor() as usize).max(1);
            let mut preview_start = 0usize;
            if let Some(target_idx) = references
                .preview_lines
                .iter()
                .position(|line| line.is_target)
            {
                preview_start = target_idx.saturating_sub(preview_rows / 2);
                if preview_start + preview_rows > references.preview_lines.len() {
                    preview_start = references.preview_lines.len().saturating_sub(preview_rows);
                }
            }

            for (slot, line) in references
                .preview_lines
                .iter()
                .skip(preview_start)
                .take(preview_rows)
                .enumerate()
            {
                let row_y = content_top + slot as f32 * line_height;
                if line.is_target {
                    chrome.push(RegionDrawInstance::new(
                        [
                            right_x + 6.0 * s,
                            row_y,
                            (right_w - 12.0 * s).max(1.0),
                            line_height,
                        ],
                        selection_bg,
                    ));
                    chrome.push(RegionDrawInstance::new(
                        [right_x + 6.0 * s, row_y, 3.0 * s, line_height],
                        accent,
                    ));
                }

                self.editor_overlay_text_system
                    .set_size(Some(line_number_width), Some(line_height));
                count_label.clear();
                let _ = write!(count_label, "{:>4}", line.line_number);
                glyphs.extend(layout_panel_text(
                    count_label,
                    &mut self.editor_overlay_text_system,
                    &mut self.atlas,
                    &self.queue,
                    right_x + 10.0 * s,
                    row_y,
                    if line.is_target { warning } else { fg_ghost },
                ));

                self.editor_overlay_text_system
                    .set_size(Some(preview_text_width), Some(line_height));
                let line_start = references
                    .preview_lines
                    .iter()
                    .take(preview_start + slot)
                    .map(|item| item.text.len() + 1)
                    .sum::<usize>();
                let line_end = line_start + line.text.len();
                let line_spans: Vec<StyledTextSpan> = references
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
                    right_x + 10.0 * s + line_number_width,
                    row_y,
                ));
            }
        } else {
            let empty_message = if references.loading {
                "Loading references..."
            } else if references.items.is_empty() {
                references
                    .status_message
                    .as_deref()
                    .unwrap_or("No references found")
            } else {
                "Loading preview..."
            };
            self.editor_overlay_text_system
                .set_size(Some((right_w - 20.0 * s).max(1.0)), Some(line_height));
            glyphs.extend(layout_panel_text(
                empty_message,
                &mut self.editor_overlay_text_system,
                &mut self.atlas,
                &self.queue,
                right_x + 10.0 * s,
                content_top + 4.0 * s,
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

fn split_reference_path(path: &str) -> (String, String) {
    let normalized = compact_reference_path(path);
    let Some((folder, file)) = normalized.rsplit_once('/') else {
        return (normalized, String::new());
    };
    (file.to_string(), folder.to_string())
}

fn compact_reference_path(path: &str) -> String {
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

fn reference_row_summary<'a>(item: &'a ReferencesBufferItem, buffer: &'a mut String) -> &'a str {
    let summary = item.summary.trim();
    if summary.is_empty() || summary.starts_with("Ln ") {
        buffer.clear();
        let _ = write!(buffer, "Ln {}, Col {}", item.line + 1, item.column + 1);
        buffer.as_str()
    } else {
        summary
    }
}

pub(super) fn grouped_list_window_start(
    item_count: usize,
    selected_index: usize,
    content_height: f32,
    row_height: f32,
    group_header_height: f32,
    same_group: impl Fn(usize, usize) -> bool,
) -> usize {
    if item_count == 0 {
        return 0;
    }

    let selected_index = selected_index.min(item_count - 1);
    let visible_rows = ((content_height / row_height.max(1.0)).floor() as usize).max(1);
    let mut start = selected_index.saturating_sub(visible_rows / 2);
    if start + visible_rows > item_count {
        start = item_count.saturating_sub(visible_rows);
    }

    while start < selected_index {
        let mut used_height = 0.0;
        let mut selected_is_visible = false;

        for item_index in start..item_count {
            let starts_group = item_index == start || !same_group(item_index - 1, item_index);
            let item_height = row_height
                + if starts_group {
                    group_header_height
                } else {
                    0.0
                };
            if used_height + item_height > content_height + 1.0 {
                break;
            }
            used_height += item_height;
            if item_index == selected_index {
                selected_is_visible = true;
                break;
            }
        }

        if selected_is_visible {
            break;
        }
        start += 1;
    }

    start
}

#[cfg(test)]
mod tests {
    use super::grouped_list_window_start;

    #[test]
    fn grouped_list_window_keeps_selected_item_visible_with_group_headers() {
        let groups = [0, 0, 1, 1, 2, 2, 3, 3];
        let selected_index = groups.len() - 1;
        let start = grouped_list_window_start(
            groups.len(),
            selected_index,
            3.0,
            1.0,
            1.0,
            |left, right| groups[left] == groups[right],
        );

        assert_eq!(start, 6);
    }
}
