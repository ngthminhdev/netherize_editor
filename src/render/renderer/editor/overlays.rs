#![allow(unused_imports)]

mod diagnostic_hover;

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
use super::completion::{completion_kind_badge, completion_label_spans};
use super::{cursor_diagnostic, editor_viewport_geometry, run_x_for_byte, wrap_text_lines};
use crate::text::text_system::StyledTextSpan;
impl Renderer {
    pub fn update_editor_overlays(&mut self, app_state: &AppState, center_bounds: [f32; 4]) {
        let active_diagnostics = app_state
            .active_file()
            .and_then(|path| app_state.diagnostics_for_path(path));
        let active_has_visible_diagnostics = active_diagnostics.is_some_and(|items| {
            items
                .iter()
                .any(|diag| matches!(diag.severity.unwrap_or(2), 1 | 2))
        });

        if app_state.current_overlays().is_empty()
            && app_state.completion().is_none()
            && !active_has_visible_diagnostics
        {
            self.editor_overlay_scissor = rect_to_scissor(center_bounds);
            let header_h = self.theme.ui.panel_line_height.max(20.0);
            let title = "[Main Editor]";
            let title_w = estimate_monospace_width(title, self.theme.ui.panel_font_size.max(1.0));
            let title_x = center_bounds[0] + ((center_bounds[2] - title_w) * 0.5).max(0.0);
            self.editor_overlay_chrome_instances = vec![RegionDrawInstance::new(
                [
                    center_bounds[0],
                    center_bounds[1],
                    center_bounds[2],
                    header_h,
                ],
                self.theme.ui.panel_bg.as_f32(),
            )];
            self.editor_overlay_text_system.set_size(
                Some((center_bounds[2] - self.editor_padding_x * 2.0).max(1.0)),
                Some(header_h),
            );
            self.editor_overlay_glyph_instances = layout_panel_text(
                title,
                &mut self.editor_overlay_text_system,
                &mut self.atlas,
                &self.queue,
                title_x,
                center_bounds[1] + ((header_h - self.theme.ui.panel_line_height).max(0.0) * 0.5),
                self.theme.ui.fg.as_f32(),
            );
            self.editor_overlay_text_pipeline.upload_instances(
                &self.device,
                &self.queue,
                &self.editor_overlay_glyph_instances,
            );
            return;
        }

        let geometry = editor_viewport_geometry(self, app_state, center_bounds);
        let viewport_top = center_bounds[1] + self.editor_padding_y;
        let viewport_bottom =
            viewport_top + (center_bounds[3] - self.editor_padding_y * 2.0).max(1.0);
        let viewport_right = center_bounds[0] + center_bounds[2] - self.editor_padding_x;

        self.editor_overlay_scissor = rect_to_scissor(center_bounds);
        let mut glyphs = Vec::new();
        let mut chrome_quads: Vec<RegionDrawInstance> = Vec::new();

        if let Some(diagnostics) = active_diagnostics {
            for diagnostic in diagnostics {
                let severity = diagnostic.severity.unwrap_or(2);
                if severity != 1 && severity != 2 {
                    continue;
                }
                let mut line_color = if severity == 1 {
                    self.theme.ui.error.as_f32()
                } else {
                    self.theme.ui.warning.as_f32()
                };
                line_color[3] = if severity == 1 { 0.12 } else { 0.08 };

                let start_line = diagnostic.range.start.line as usize;
                let end_line = diagnostic.range.end.line as usize;
                for run in self.text_system.buffer().layout_runs() {
                    if run.line_i < start_line || run.line_i > end_line {
                        continue;
                    }
                    let line_top = geometry.origin_y + run.line_top;
                    let line_height_px = run.line_height.max(1.0);
                    let line_bottom = line_top + line_height_px;
                    if line_bottom <= viewport_top || line_top >= viewport_bottom {
                        continue;
                    }
                    chrome_quads.push(RegionDrawInstance::new(
                        [
                            geometry.viewport_text_left,
                            line_top,
                            geometry.viewport_text_width,
                            line_height_px,
                        ],
                        line_color,
                    ));
                }
            }
        }

        chrome_quads.extend(self.diagnostic_underline_quads(app_state, center_bounds));

        for overlay in app_state.current_overlays() {
            match overlay {
                EditorOverlay::VirtualText {
                    line,
                    column: _,
                    text,
                    color_token,
                } => {
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

                    let Some(line_top) = line_top else { continue };
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
                EditorOverlay::FloatingBox {
                    anchor_line,
                    anchor_col: _,
                    blocks,
                    style,
                } => {
                    const PAD_X: f32 = 10.0;
                    const PAD_Y: f32 = 6.0;
                    const BORDER: f32 = 1.0;
                    let char_w = (geometry.font_size * 0.6).max(1.0);
                    let max_len = blocks
                        .iter()
                        .map(|block| match block {
                            FloatingBoxBlock::Prose(text) => {
                                text.lines().map(|line| line.len()).max().unwrap_or(0)
                            }
                            FloatingBoxBlock::Code { text, .. } => {
                                text.lines().map(|line| line.len()).max().unwrap_or(0)
                            }
                        })
                        .max()
                        .unwrap_or(0);
                    let line_count = blocks
                        .iter()
                        .map(|block| match block {
                            FloatingBoxBlock::Prose(text) | FloatingBoxBlock::Code { text, .. } => {
                                text.lines().count().max(1)
                            }
                        })
                        .sum::<usize>();
                    let desired_popup_w = max_len as f32 * char_w + PAD_X * 2.0;
                    let available_popup_w = geometry.viewport_text_width.max(1.0);
                    let preferred_min_popup_w = 120.0_f32.min(available_popup_w);
                    let popup_w = desired_popup_w.clamp(preferred_min_popup_w, available_popup_w);
                    debug_assert!(
                        popup_w.is_finite() && popup_w >= 1.0,
                        "floating overlay popup width must stay finite: desired={desired_popup_w}, viewport_text_width={}",
                        geometry.viewport_text_width
                    );
                    let popup_h = (line_count as f32 * geometry.line_height + PAD_Y * 2.0)
                        .max(geometry.line_height);

                    let anchor_y = geometry.origin_y + (*anchor_line as f32) * geometry.line_height;
                    let mut popup_y = anchor_y + geometry.line_height;
                    if popup_y + popup_h > viewport_bottom {
                        popup_y = (anchor_y - popup_h).max(center_bounds[1]);
                    }
                    let popup_x = geometry.viewport_text_left.max(center_bounds[0]);

                    let bg_color = self.theme.ui.panel_bg.as_f32();
                    let border_color = match style {
                        FloatingBoxStyle::DocHover => self.theme.ui.accent.as_f32(),
                        FloatingBoxStyle::PeekWindow => self.theme.ui.warning.as_f32(),
                    };
                    chrome_quads.push(RegionDrawInstance::new(
                        [
                            popup_x - BORDER,
                            popup_y - BORDER,
                            popup_w + BORDER * 2.0,
                            popup_h + BORDER * 2.0,
                        ],
                        border_color,
                    ));
                    chrome_quads.push(RegionDrawInstance::new(
                        [popup_x, popup_y, popup_w, popup_h],
                        bg_color,
                    ));

                    let text_x = popup_x + PAD_X;
                    let line_w = (popup_w - PAD_X * 2.0).max(1.0);
                    let mut block_line_offset = 0usize;
                    for block in blocks {
                        match block {
                            FloatingBoxBlock::Prose(text) => {
                                let block_lines: Vec<&str> = text.lines().collect();
                                for (line_idx, line_text) in block_lines.iter().enumerate() {
                                    let text_y = popup_y
                                        + PAD_Y
                                        + (block_line_offset + line_idx) as f32
                                            * geometry.line_height;
                                    if text_y + geometry.line_height < viewport_top
                                        || text_y > viewport_bottom
                                    {
                                        continue;
                                    }
                                    self.editor_overlay_text_system
                                        .set_size(Some(line_w), Some(geometry.line_height));
                                    glyphs.extend(layout_panel_text_italic(
                                        line_text,
                                        &mut self.editor_overlay_text_system,
                                        &mut self.atlas,
                                        &self.queue,
                                        text_x,
                                        text_y,
                                        self.theme.ui.fg_ghost.as_f32(),
                                    ));
                                }
                                block_line_offset += block_lines.len().max(1);
                            }
                            FloatingBoxBlock::Code { text, spans } => {
                                let block_lines: Vec<&str> = text.lines().collect();
                                for (line_idx, line_text) in block_lines.iter().enumerate() {
                                    let text_y = popup_y
                                        + PAD_Y
                                        + (block_line_offset + line_idx) as f32
                                            * geometry.line_height;
                                    if text_y + geometry.line_height < viewport_top
                                        || text_y > viewport_bottom
                                    {
                                        continue;
                                    }
                                    let line_start = block_lines
                                        .iter()
                                        .take(line_idx)
                                        .map(|line| line.len() + 1)
                                        .sum::<usize>();
                                    let line_end = line_start + line_text.len();
                                    let line_spans: Vec<StyledTextSpan> = spans
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
                                    self.editor_overlay_text_system
                                        .set_size(Some(line_w), Some(geometry.line_height));
                                    glyphs.extend(layout_panel_rich_text(
                                        line_text,
                                        &line_spans,
                                        self.theme.ui.fg.as_f32(),
                                        &mut self.editor_overlay_text_system,
                                        &mut self.atlas,
                                        &self.queue,
                                        text_x,
                                        text_y,
                                    ));
                                }
                                block_line_offset += block_lines.len().max(1);
                            }
                        }
                    }
                }
            }
        }

        if let Some(completion) = app_state.completion() {
            const PAD_X: f32 = 10.0;
            const PAD_Y: f32 = 6.0;
            const BORDER: f32 = 1.0;
            const MAX_VISIBLE_ROWS: usize = 8;
            const ROW_SEPARATOR_H: f32 = 1.0;
            const BADGE_RADIUS: f32 = 5.0;

            let char_w = (geometry.font_size * 0.6).max(1.0);
            let max_len = completion
                .filtered_items
                .iter()
                .map(|item| {
                    let detail_len = item
                        .item
                        .detail
                        .as_deref()
                        .map(str::trim)
                        .filter(|detail| !detail.is_empty())
                        .map(|detail| detail.chars().count() + 3)
                        .unwrap_or(0);
                    item.item.label.chars().count() + detail_len + 8
                })
                .max()
                .unwrap_or(8);
            let visible_rows = completion.filtered_items.len().min(MAX_VISIBLE_ROWS).max(1);
            let popup_w =
                (max_len as f32 * char_w + PAD_X * 2.0).clamp(160.0, geometry.viewport_text_width);
            let popup_h = (visible_rows as f32 * geometry.line_height + PAD_Y * 2.0)
                .max(geometry.line_height);

            let anchor_x = (geometry.origin_x + completion.anchor_col as f32 * char_w)
                .clamp(geometry.viewport_text_left, viewport_right - popup_w);
            let anchor_y = geometry.origin_y + completion.anchor_line as f32 * geometry.line_height;
            let mut popup_y = anchor_y + geometry.line_height;
            if popup_y + popup_h > viewport_bottom {
                popup_y = (anchor_y - popup_h).max(center_bounds[1]);
            }

            let bg_color = self.theme.ui.panel_bg.as_f32();
            let border_color = self.theme.ui.border_color.as_f32();
            let selection_bg = self.theme.ui.selection_bg.as_f32();
            let separator_color = self.theme.ui.border_color.as_f32();
            let badge_color = self.theme.ui.border_color.as_f32();
            let label_color = self.theme.ui.fg.as_f32();
            let detail_color = self.theme.ui.fg_ghost.as_f32();
            let match_color = self.theme.ui.accent.as_f32();

            chrome_quads.push(RegionDrawInstance::new(
                [
                    anchor_x - BORDER,
                    popup_y - BORDER,
                    popup_w + BORDER * 2.0,
                    popup_h + BORDER * 2.0,
                ],
                border_color,
            ));
            chrome_quads.push(RegionDrawInstance::new(
                [anchor_x, popup_y, popup_w, popup_h],
                bg_color,
            ));

            let scroll_start = if completion.selected_index >= visible_rows {
                completion.selected_index + 1 - visible_rows
            } else {
                0
            };
            let visible_items = completion
                .filtered_items
                .iter()
                .enumerate()
                .skip(scroll_start)
                .take(visible_rows);

            for (row_idx, (item_idx, item)) in visible_items.enumerate() {
                let row_y = popup_y + PAD_Y + row_idx as f32 * geometry.line_height;
                if item_idx == completion.selected_index {
                    chrome_quads.push(RegionDrawInstance::new(
                        [
                            anchor_x + 1.0,
                            row_y,
                            (popup_w - 2.0).max(1.0),
                            geometry.line_height,
                        ],
                        selection_bg,
                    ));
                }

                if row_idx + 1 < visible_rows {
                    chrome_quads.push(RegionDrawInstance::new(
                        [
                            anchor_x + 1.0,
                            row_y + geometry.line_height - ROW_SEPARATOR_H,
                            (popup_w - 2.0).max(1.0),
                            ROW_SEPARATOR_H,
                        ],
                        separator_color,
                    ));
                }

                let kind_badge = completion_kind_badge(item.item.kind, &self.theme);
                let badge_label = kind_badge.icon;
                let badge_w =
                    (estimate_monospace_width(badge_label, geometry.font_size) + 10.0).max(18.0);
                let badge_h = (geometry.line_height - 6.0).max(12.0);
                let badge_x = anchor_x + PAD_X;
                let badge_y = row_y + ((geometry.line_height - badge_h) * 0.5);
                chrome_quads.push(
                    RegionDrawInstance::new([badge_x, badge_y, badge_w, badge_h], badge_color)
                        .with_radius(BADGE_RADIUS),
                );

                self.editor_overlay_text_system
                    .set_size(Some((badge_w - 6.0).max(1.0)), Some(geometry.line_height));
                glyphs.extend(layout_panel_text(
                    badge_label,
                    &mut self.editor_overlay_text_system,
                    &mut self.atlas,
                    &self.queue,
                    badge_x + 3.0,
                    row_y,
                    kind_badge.color,
                ));

                let label_x = badge_x + badge_w + 8.0;
                let detail_text = item
                    .item
                    .detail
                    .as_deref()
                    .map(str::trim)
                    .filter(|detail| !detail.is_empty())
                    .map(|detail| {
                        clamp_monospace_text(
                            detail,
                            (popup_w * 0.35).max(char_w * 8.0),
                            geometry.font_size,
                        )
                    })
                    .unwrap_or_default();
                let detail_width = estimate_monospace_width(&detail_text, geometry.font_size);
                let detail_gap = if detail_text.is_empty() { 0.0 } else { 10.0 };
                let detail_x = anchor_x + popup_w - PAD_X - detail_width;
                let label_width = if detail_text.is_empty() {
                    (popup_w - (label_x - anchor_x) - PAD_X).max(1.0)
                } else {
                    (detail_x - label_x - detail_gap).max(char_w * 6.0)
                };
                let spans = completion_label_spans(item, match_color);
                self.editor_overlay_text_system
                    .set_size(Some(label_width), Some(geometry.line_height));
                glyphs.extend(layout_panel_rich_text(
                    &item.item.label,
                    &spans,
                    label_color,
                    &mut self.editor_overlay_text_system,
                    &mut self.atlas,
                    &self.queue,
                    label_x,
                    row_y,
                ));

                if !detail_text.is_empty() {
                    self.editor_overlay_text_system
                        .set_size(Some(detail_width.max(1.0)), Some(geometry.line_height));
                    glyphs.extend(layout_panel_text(
                        &detail_text,
                        &mut self.editor_overlay_text_system,
                        &mut self.atlas,
                        &self.queue,
                        detail_x,
                        row_y,
                        detail_color,
                    ));
                }
            }
        }

        self.editor_overlay_chrome_instances = chrome_quads;
        self.editor_overlay_glyph_instances = glyphs;
        self.editor_overlay_text_pipeline.upload_instances(
            &self.device,
            &self.queue,
            &self.editor_overlay_glyph_instances,
        );
    }
}
