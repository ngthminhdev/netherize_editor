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
                |idx| {
                    references
                        .collapsed_paths
                        .contains(&references.items[idx].relative_path)
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
                    let is_collapsed = references.collapsed_paths.contains(&item.relative_path);
                    // When the selection lives inside this collapsed group its own row
                    // is hidden, so the highlight rides the group header instead.
                    let header_selected = is_collapsed
                        && references
                            .items
                            .get(references.selected_index)
                            .map(|sel| sel.relative_path == item.relative_path)
                            .unwrap_or(false);
                    let header_h_px = (line_height + 3.0 * s).max(12.0 * s);
                    if header_selected {
                        chrome.push(RegionDrawInstance::new(
                            [
                                left_x + 6.0 * s,
                                draw_y + 2.0 * s,
                                (left_w - 12.0 * s).max(1.0),
                                header_h_px,
                            ],
                            selection_bg,
                        ));
                        chrome.push(RegionDrawInstance::new(
                            [left_x + 6.0 * s, draw_y + 2.0 * s, 3.0 * s, header_h_px],
                            accent,
                        ));
                    } else {
                        let mut group_bg = self.theme.ui.overlay_bg.as_f32();
                        group_bg[3] = (group_bg[3] * 0.85).clamp(0.18, 0.45);
                        chrome.push(RegionDrawInstance::new(
                            [
                                left_x + 8.0 * s,
                                draw_y + 2.0 * s,
                                (left_w - 16.0 * s).max(1.0),
                                header_h_px,
                            ],
                            group_bg,
                        ));
                    }
                    let group_count = references
                        .path_counts
                        .get(&item.relative_path)
                        .copied()
                        .unwrap_or(0);
                    count_label.clear();
                    let _ = write!(count_label, "{}", group_count);
                    let count_w = estimate_monospace_width(count_label, font_size).max(16.0 * s);
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

/// Compute the first item index a grouped result list should render from so the
/// selection stays visible and the list scrolls smoothly.
///
/// The calculation works in *rendered* height: a group always draws one header,
/// an expanded member draws a row, and a **collapsed** member (per `collapsed`)
/// draws nothing — exactly mirroring the renderer. Accounting for collapse here
/// is what stops the list from jumping when groups toggle, and centering in
/// rendered space (rather than raw index space) keeps navigation smooth even when
/// moving the selection leaps over the hidden members of a collapsed group.
pub(super) fn grouped_list_window_start(
    item_count: usize,
    selected_index: usize,
    content_height: f32,
    row_height: f32,
    group_header_height: f32,
    same_group: impl Fn(usize, usize) -> bool,
    collapsed: impl Fn(usize) -> bool,
) -> usize {
    if item_count == 0 {
        return 0;
    }

    let selected_index = selected_index.min(item_count - 1);
    let row_height = row_height.max(1.0);

    let row_of = |i: usize| if collapsed(i) { 0.0 } else { row_height };

    // Rendered height added by prepending item `prev` to a window whose current
    // first item is `head`. The new first item always draws a header; `head` only
    // keeps its own header if it still begins a group (i.e. `prev` is a different
    // group). This mirrors the renderer, which redraws the header of whichever item
    // the window happens to start on.
    let prepend_height = |prev: usize, head: usize| {
        let header = if same_group(prev, head) {
            0.0
        } else {
            group_header_height
        };
        row_of(prev) + header
    };

    // The window's first item always draws a header, so a single-item window is its
    // row plus one header.
    let single = |i: usize| row_of(i) + group_header_height;

    // Center the selection: extend the window upward from the selection while the
    // rendered height from the new start down to the selection stays within half the
    // viewport. Working in rendered height keeps scrolling smooth even when the
    // selection leaps over the hidden members of a collapsed group.
    let half = content_height * 0.5;
    let mut centered = selected_index;
    let mut span = single(selected_index);
    while centered > 0 {
        let prev = centered - 1;
        let next_span = span + prepend_height(prev, centered);
        if next_span > half {
            break;
        }
        span = next_span;
        centered = prev;
    }

    // End clamp: never scroll past the point where the last item fills the bottom of
    // the viewport, so the final group never leaves blank space below it.
    let last = item_count - 1;
    let mut max_start = last;
    let mut tail = single(last);
    while max_start > 0 {
        let prev = max_start - 1;
        let next_tail = tail + prepend_height(prev, max_start);
        if next_tail > content_height + 1.0 {
            break;
        }
        tail = next_tail;
        max_start = prev;
    }

    centered.min(max_start)
}

#[cfg(test)]
mod tests {
    use super::grouped_list_window_start;

    /// Mirror the renderer's vertical layout: each group draws one header, an
    /// expanded member draws a row, a collapsed member draws nothing. Returns the
    /// top-y at which the selected item becomes visible (its row, or — when the
    /// selected item lives in a collapsed group — its group header), or `None`
    /// when `start` scrolls the selection out of view.
    fn simulate_selected_y(
        start: usize,
        groups: &[usize],
        collapsed: &dyn Fn(usize) -> bool,
        selected: usize,
        content_height: f32,
        row_height: f32,
        group_header_height: f32,
    ) -> Option<f32> {
        let mut y = 0.0;
        for i in start..groups.len() {
            let starts_group = i == start || groups[i - 1] != groups[i];
            if starts_group {
                if y + group_header_height > content_height + 1.0 {
                    return None;
                }
                if collapsed(i) && groups[i] == groups[selected] {
                    // The selected item is inside this collapsed group: its header
                    // is the visible anchor.
                    return Some(y);
                }
                y += group_header_height;
            }
            if collapsed(i) {
                continue;
            }
            if y + row_height > content_height + 1.0 {
                return None;
            }
            if i == selected {
                return Some(y);
            }
            y += row_height;
        }
        None
    }

    #[test]
    fn keeps_selected_visible_with_group_headers() {
        let groups = [0, 0, 1, 1, 2, 2, 3, 3];
        let selected_index = groups.len() - 1;
        let collapsed = |_: usize| false;
        let start = grouped_list_window_start(
            groups.len(),
            selected_index,
            3.0,
            1.0,
            1.0,
            |left, right| groups[left] == groups[right],
            &collapsed,
        );

        let y = simulate_selected_y(
            start,
            &groups,
            &collapsed,
            selected_index,
            3.0,
            1.0,
            1.0,
        );
        assert!(y.is_some(), "selected row must be visible, start={start}");
    }

    #[test]
    fn collapsed_group_above_does_not_push_selection_out_of_view() {
        // group 1 (indices 2,3,4) is collapsed; selection sits in group 2.
        let groups = [0, 0, 1, 1, 1, 2, 2];
        let collapsed = |i: usize| groups[i] == 1;
        let selected_index = 6;
        let start = grouped_list_window_start(
            groups.len(),
            selected_index,
            4.0,
            1.0,
            1.0,
            |left, right| groups[left] == groups[right],
            &collapsed,
        );

        let y = simulate_selected_y(
            start,
            &groups,
            &collapsed,
            selected_index,
            4.0,
            1.0,
            1.0,
        );
        let y = y.expect("selected row must stay visible with a collapsed group above");
        assert!(
            (0.0..=3.0).contains(&y),
            "selected row should sit fully inside the viewport, got y={y} start={start}"
        );
    }

    #[test]
    fn selection_inside_collapsed_group_keeps_header_visible() {
        // selection points into the collapsed group 1; its header must be shown.
        let groups = [0, 0, 1, 1, 1, 2, 2];
        let collapsed = |i: usize| groups[i] == 1;
        let selected_index = 3;
        let start = grouped_list_window_start(
            groups.len(),
            selected_index,
            4.0,
            1.0,
            1.0,
            |left, right| groups[left] == groups[right],
            &collapsed,
        );

        let y = simulate_selected_y(
            start,
            &groups,
            &collapsed,
            selected_index,
            4.0,
            1.0,
            1.0,
        );
        assert!(
            y.is_some(),
            "collapsed selected group's header must be visible, start={start}"
        );
    }

    #[test]
    fn centers_selection_in_rendered_space() {
        // single group, no headers: selecting the middle should leave roughly half
        // the viewport of rendered rows above the selection (stable centering).
        let groups = [0usize; 30];
        let collapsed = |_: usize| false;
        let selected_index = 20;
        let start = grouped_list_window_start(
            groups.len(),
            selected_index,
            10.0,
            1.0,
            0.0,
            |left, right| groups[left] == groups[right],
            &collapsed,
        );

        // ~5 rows above the selection (content_height/2), tolerate off-by-one.
        assert!(
            (14..=17).contains(&start),
            "selection should be centered, start={start}"
        );
    }

    #[test]
    fn collapsed_navigation_leap_does_not_jump_the_window() {
        // group 1 (indices 2..=11, ten items) is collapsed. Navigating from the last
        // item of group 0 (index 1) to the first visible item of group 2 (index 12)
        // is a single visible-row move even though the raw index leaps by 11. The
        // selected row's screen position must stay stable (no viewport-sized jump).
        let mut groups = vec![0usize, 0];
        groups.extend(std::iter::repeat(1usize).take(10)); // indices 2..=11
        groups.extend([2usize, 2, 3, 3, 4, 4, 5, 5]); // indices 12..
        let collapsed = |i: usize| groups[i] == 1;
        let same = |l: usize, r: usize| groups[l] == groups[r];
        let (content, row, header) = (6.0f32, 1.0f32, 1.0f32);

        let start_before = grouped_list_window_start(
            groups.len(), 1, content, row, header, &same, &collapsed,
        );
        let y_before =
            simulate_selected_y(start_before, &groups, &collapsed, 1, content, row, header)
                .expect("selection visible before leap");

        let start_after = grouped_list_window_start(
            groups.len(), 12, content, row, header, &same, &collapsed,
        );
        let y_after =
            simulate_selected_y(start_after, &groups, &collapsed, 12, content, row, header)
                .expect("selection visible after leap");

        assert!(
            (y_after - y_before).abs() <= row + header + 0.001,
            "selection screen position jumped: before={y_before} after={y_after}"
        );
    }
}
