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
    text::layout_sync::{
            compute_caret_layout, compute_caret_layout_with_folds, compute_cursor_overlay,
            rebuild_layout_projection,
        },
};
use cosmic_text::Metrics;

use super::super::helpers::{
    caret_rect_for_mode, clamp_monospace_text, clamp_popup_width, estimate_monospace_width,
    gutter_width_for_editor, layout_panel_rich_text, layout_panel_text, layout_panel_text_bold,
    layout_panel_text_italic, rect_to_scissor, should_draw_block_cursor,
};
use crate::render::icon_pipeline::{canonical_icon_id, IconDrawInstance};
use super::completion::{completion_kind_badge, completion_label_spans};
use super::{EDITOR_BREADCRUMB_GAP_Y, EDITOR_BREADCRUMB_PAD_Y, EDITOR_BREADCRUMB_TOP_INSET};

const DIAGNOSTIC_SEVERITY_ERROR: u32 = 1;
const DIAGNOSTIC_SEVERITY_WARNING: u32 = 2;
const EDITOR_BREADCRUMB_FRAME_INSET_X: f32 = 4.0;
use super::{cursor_diagnostic, editor_viewport_geometry, run_x_for_byte, wrap_text_lines};
use crate::text::text_system::StyledTextSpan;

impl Renderer {
    pub fn update_editor_overlays(&mut self, app_state: &AppState, center_bounds: [f32; 4]) {
        let active_diagnostics = app_state
            .active_file()
            .and_then(|path| app_state.diagnostics_for_path(path));
        let geometry = editor_viewport_geometry(self, app_state, center_bounds);
        let viewport_top = geometry.viewport_text_top;
        let viewport_bottom = viewport_top + geometry.viewport_text_height.max(1.0);
        let viewport_right = center_bounds[0] + center_bounds[2] - self.editor_padding_x;

        self.editor_overlay_scissor = rect_to_scissor(center_bounds);
        let mut glyphs = Vec::new();
        let mut icon_instances: Vec<IconDrawInstance> = Vec::new();
        let mut chrome_quads: Vec<RegionDrawInstance> = Vec::new();

        let header_h = geometry.viewport_text_top
            - (center_bounds[1] + EDITOR_BREADCRUMB_TOP_INSET)
            - EDITOR_BREADCRUMB_GAP_Y;
        if header_h > 0.0 {
            let header_y = center_bounds[1] + EDITOR_BREADCRUMB_TOP_INSET;
            let header_x = center_bounds[0] + EDITOR_BREADCRUMB_FRAME_INSET_X;
            let header_w = (center_bounds[2] - EDITOR_BREADCRUMB_FRAME_INSET_X * 2.0).max(0.0);
            let header_bg = blend_rgba(
                self.theme.editor.bg.as_f32(),
                self.theme.ui.status_bar_bg.as_f32(),
                0.62,
                1.0,
            );
            let divider = blend_rgba(header_bg, self.theme.ui.border_color.as_f32(), 0.7, 1.0);
            chrome_quads.push(
                RegionDrawInstance::new([header_x, header_y, header_w, header_h], header_bg)
                    .with_radius(self.panel_corner_radius),
            );
            chrome_quads.push(RegionDrawInstance::new(
                [header_x, header_y + header_h - 1.0, header_w, 1.0],
                divider,
            ));

            let mut x = geometry.viewport_text_left;
            let text_y = header_y + ((header_h - geometry.line_height).max(0.0) * 0.5);
            let max_x = header_x + header_w - self.editor_padding_x;
            let separator = " › ";
            let separator_w = estimate_monospace_width(separator, geometry.font_size);
            let separator_color = self.theme.ui.fg_ghost.as_f32();

            for (index, segment) in self.editor_breadcrumb_segments.iter().enumerate() {
                let is_last = index + 1 == self.editor_breadcrumb_segments.len();
                let available = (max_x - x).max(1.0);
                if available <= 1.0 {
                    break;
                }
                let text = clamp_monospace_text(
                    &segment.text,
                    if is_last {
                        available
                    } else {
                        available - separator_w
                    },
                    geometry.font_size,
                );
                if text.is_empty() {
                    break;
                }
                glyphs.extend(layout_panel_text(
                    &text,
                    &mut self.editor_overlay_text_system,
                    &mut self.atlas,
                    &self.queue,
                    x,
                    text_y,
                    segment.color,
                ));
                x += estimate_monospace_width(&text, geometry.font_size);
                if !is_last && x + separator_w < max_x {
                    glyphs.extend(layout_panel_text(
                        separator,
                        &mut self.editor_overlay_text_system,
                        &mut self.atlas,
                        &self.queue,
                        x,
                        text_y,
                        separator_color,
                    ));
                    x += separator_w;
                }
            }
        }

        if let Some(diagnostics) = active_diagnostics {
            for diagnostic in diagnostics {
                let severity = diagnostic.severity.unwrap_or(DIAGNOSTIC_SEVERITY_WARNING);
                if severity != DIAGNOSTIC_SEVERITY_ERROR && severity != DIAGNOSTIC_SEVERITY_WARNING
                {
                    continue;
                }
                let mut line_color = if severity == DIAGNOSTIC_SEVERITY_ERROR {
                    self.theme.ui.error.as_f32()
                } else {
                    self.theme.ui.warning.as_f32()
                };
                line_color[3] = if severity == DIAGNOSTIC_SEVERITY_ERROR {
                    0.25
                } else {
                    0.20
                };

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
                    if severity == DIAGNOSTIC_SEVERITY_WARNING {
                        let mut warning_rail = self.theme.ui.warning.as_f32();
                        warning_rail[3] = 0.72;
                        chrome_quads.push(RegionDrawInstance::new(
                            [
                                geometry.viewport_text_left,
                                line_top + 2.0,
                                2.0,
                                (line_height_px - 4.0).max(1.0),
                            ],
                            warning_rail,
                        ));
                    }
                }
            }
        }

        chrome_quads.extend(self.semantic_symbol_highlight_quads(app_state, center_bounds));
        chrome_quads.extend(self.search_highlight_quads(app_state, center_bounds));
        chrome_quads.extend(self.multi_cursor_selection_quads(app_state, center_bounds));
        chrome_quads.extend(self.visual_selection_quads(app_state, center_bounds));
        chrome_quads.extend(self.visual_block_selection_quads(app_state, center_bounds));
        if let Some(quad) = self.current_line_highlight_quad(app_state, center_bounds) {
            chrome_quads.push(quad);
        }
        chrome_quads.extend(self.indent_guide_quads(app_state, center_bounds));
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
                    scroll,
                } => {
                    const PAD_X: f32 = 14.0;
                    const PAD_Y: f32 = 10.0;
                    const BORDER: f32 = 1.0;
                    const CODE_PAD_X: f32 = 12.0;
                    const CODE_PAD_Y: f32 = 8.0;
                    const CONTENT_INSET_X: f32 = 14.0;
                    const CONTENT_INSET_Y: f32 = 12.0;
                    let char_w = (geometry.font_size * 0.6).max(1.0);
                    let is_doc_hover = matches!(style, FloatingBoxStyle::DocHover);
                    let header_h = if is_doc_hover {
                        geometry.line_height + 14.0
                    } else {
                        0.0
                    };
                    let block_gap = if is_doc_hover { 10.0 } else { 0.0 };

                    // ── Hover card size caps ─────────────────────────────────────
                    let max_popup_w = (center_bounds[2] * 0.58)
                        .max(260.0)
                        .min(geometry.viewport_text_width);
                    let max_popup_h = (center_bounds[3] * 0.58).max(geometry.line_height * 3.0);
                    let wrap_cols = ((max_popup_w - PAD_X * 2.0) / char_w).floor() as usize;

                    // ── Pre-render: word-wrap prose, keep code verbatim ─────────
                    // (is_code, rendered_lines, spans_for_code)
                    let rendered: Vec<(bool, Vec<String>, &[StyledTextSpan])> = blocks
                        .iter()
                        .map(|block| match block {
                            FloatingBoxBlock::Prose(text) => (
                                false,
                                wrap_text_lines(text, wrap_cols.max(16)),
                                [].as_slice(),
                            ),
                            FloatingBoxBlock::Code { text, spans } => (
                                true,
                                text.lines().map(String::from).collect(),
                                spans.as_slice(),
                            ),
                        })
                        .collect();

                    let max_len = rendered
                        .iter()
                        .flat_map(|(_, lines, _)| lines.iter())
                        .map(|l| l.chars().count())
                        .max()
                        .unwrap_or(0);
                    let total_lines: usize = rendered
                        .iter()
                        .map(|(_, lines, _)| lines.len().max(1))
                        .sum();
                    let content_block_extra_h: f32 = if is_doc_hover {
                        rendered
                            .iter()
                            .filter(|(is_code, _, _)| *is_code)
                            .count() as f32
                            * (CODE_PAD_Y * 2.0 + block_gap)
                    } else {
                        0.0
                    };
                    let content_inner_pad_y = if is_doc_hover { CONTENT_INSET_Y * 2.0 } else { 0.0 };

                    let desired_popup_w = max_len as f32 * char_w + PAD_X * 2.0;
                    let popup_w = clamp_popup_width(desired_popup_w, 260.0, max_popup_w);
                    debug_assert!(
                        popup_w.is_finite() && popup_w >= 1.0,
                        "floating overlay popup width must stay finite: desired={desired_popup_w}, max_popup_w={max_popup_w}"
                    );
                    let total_content_h = total_lines as f32 * geometry.line_height
                        + content_block_extra_h
                        + content_inner_pad_y;
                    let popup_h = (header_h + total_content_h + PAD_Y * 2.0)
                        .max(geometry.line_height)
                        .min(max_popup_h);

                    let anchor_y = geometry.origin_y + (*anchor_line as f32) * geometry.line_height;
                    let mut popup_y = anchor_y + geometry.line_height;
                    if popup_y + popup_h > viewport_bottom {
                        popup_y = (anchor_y - popup_h).max(center_bounds[1]);
                    }
                    // Anchor left at text area; flip left if overflows right edge.
                    let raw_popup_x = geometry.viewport_text_left.max(center_bounds[0]);
                    let popup_x = if raw_popup_x + popup_w > viewport_right {
                        (viewport_right - popup_w).max(center_bounds[0])
                    } else {
                        raw_popup_x
                    };

                    // Content clip bottom — glyphs beyond this are skipped (GPU scissor
                    // is center_bounds; this provides popup-level clipping in software).

                    let base_bg = self.theme.editor.bg.as_f32();
                    let panel_bg = blend_rgba(base_bg, self.theme.ui.panel_bg.as_f32(), 0.72, 1.0);
                    let border_color = match style {
                        FloatingBoxStyle::DocHover => blend_rgba(
                            self.theme.ui.border_color.as_f32(),
                            self.theme.ui.accent.as_f32(),
                            0.45,
                            1.0,
                        ),
                        FloatingBoxStyle::PeekWindow => self.theme.ui.warning.as_f32(),
                    };
                    let shadow_color: [f32; 4] = [0.0, 0.0, 0.0, 0.38];
                    chrome_quads.push(
                        RegionDrawInstance::new(
                            [popup_x + 3.0, popup_y + 5.0, popup_w, popup_h],
                            shadow_color,
                        )
                        .with_radius(self.panel_corner_radius + 2.0),
                    );
                    chrome_quads.push(
                        RegionDrawInstance::new(
                            [
                                popup_x - BORDER,
                                popup_y - BORDER,
                                popup_w + BORDER * 2.0,
                                popup_h + BORDER * 2.0,
                            ],
                            border_color,
                        )
                        .with_radius(self.panel_corner_radius + 2.0),
                    );
                    chrome_quads.push(
                        RegionDrawInstance::new([popup_x, popup_y, popup_w, popup_h], panel_bg)
                            .with_radius(self.panel_corner_radius + 2.0),
                    );

                    if is_doc_hover {
                        let header_bg = blend_rgba(panel_bg, self.theme.ui.status_bar_bg.as_f32(), 0.5, 1.0);
                        let header_divider = blend_rgba(header_bg, border_color, 0.55, 1.0);
                        chrome_quads.push(
                            RegionDrawInstance::new([popup_x, popup_y, popup_w, header_h], header_bg)
                                .with_radius(self.panel_corner_radius + 2.0),
                        );
                        chrome_quads.push(RegionDrawInstance::new(
                            [popup_x, popup_y + header_h - 1.0, popup_w, 1.0],
                            header_divider,
                        ));

                        let badge_text = "docs";
                        let badge_w = badge_text.chars().count() as f32 * char_w + 16.0;
                        let badge_h = geometry.line_height + 2.0;
                        let badge_x = popup_x + PAD_X;
                        let badge_y = popup_y + (header_h - badge_h) * 0.5;
                        chrome_quads.push(
                            RegionDrawInstance::new(
                                [badge_x, badge_y, badge_w, badge_h],
                                blend_rgba(panel_bg, self.theme.ui.accent.as_f32(), 0.2, 1.0),
                            )
                            .with_radius(6.0),
                        );
                        self.editor_overlay_text_system
                            .set_size(Some(badge_w), Some(badge_h));
                        glyphs.extend(layout_panel_text(
                            badge_text,
                            &mut self.editor_overlay_text_system,
                            &mut self.atlas,
                            &self.queue,
                            badge_x + 8.0,
                            badge_y + 1.0,
                            self.theme.ui.accent.as_f32(),
                        ));

                        self.editor_overlay_text_system.set_size(
                            Some((popup_w - badge_w - PAD_X * 3.0).max(1.0)),
                            Some(geometry.line_height),
                        );
                        glyphs.extend(layout_panel_text(
                            "Hover Documentation",
                            &mut self.editor_overlay_text_system,
                            &mut self.atlas,
                            &self.queue,
                            badge_x + badge_w + 12.0,
                            popup_y + (header_h - geometry.line_height) * 0.5,
                            self.theme.ui.fg.as_f32(),
                        ));
                    }

                    let content_left = popup_x + PAD_X + if is_doc_hover { CONTENT_INSET_X } else { 0.0 };
                    let content_top = popup_y + PAD_Y + header_h + if is_doc_hover { CONTENT_INSET_Y } else { 0.0 };
                    let content_bottom = popup_y + popup_h - PAD_Y - if is_doc_hover { CONTENT_INSET_Y } else { 0.0 };
                    let needs_scrollbar = is_doc_hover && header_h + total_content_h + PAD_Y * 2.0 > popup_h;
                    let scrollbar_w = if needs_scrollbar { 4.0 } else { 0.0 };
                    let line_w = (popup_w
                        - PAD_X * 2.0
                        - if is_doc_hover { CONTENT_INSET_X * 2.0 } else { 0.0 }
                        - scrollbar_w
                        - 10.0)
                        .max(1.0);
                    let content_view_h = (content_bottom - content_top).max(geometry.line_height);
                    let max_scroll_lines = ((total_content_h - content_inner_pad_y - content_view_h).max(0.0)
                        / geometry.line_height)
                        .ceil() as usize;
                    let effective_scroll_lines = scroll.offset_lines.min(max_scroll_lines);
                    let mut content_y = content_top - effective_scroll_lines as f32 * geometry.line_height;

                    if needs_scrollbar {
                        let track_x = popup_x + popup_w - PAD_X - 4.0;
                        let track_y = content_top;
                        let track_h = content_view_h;
                        let scrollbar_track = blend_rgba(panel_bg, self.theme.ui.border_color.as_f32(), 0.28, 1.0);
                        let scrollbar_thumb = blend_rgba(scrollbar_track, self.theme.ui.accent.as_f32(), 0.62, 1.0);
                        chrome_quads.push(
                            RegionDrawInstance::new([track_x, track_y, scrollbar_w, track_h], scrollbar_track)
                                .with_radius(2.0),
                        );
                        let visible_ratio = (content_view_h / total_content_h.max(1.0)).clamp(0.08, 1.0);
                        let thumb_h = (track_h * visible_ratio).max(18.0).min(track_h);
                        let scroll_ratio = if max_scroll_lines == 0 {
                            0.0
                        } else {
                            effective_scroll_lines as f32 / max_scroll_lines as f32
                        };
                        let thumb_y = track_y + (track_h - thumb_h) * scroll_ratio;
                        chrome_quads.push(
                            RegionDrawInstance::new([track_x, thumb_y, scrollbar_w, thumb_h], scrollbar_thumb)
                                .with_radius(2.0),
                        );
                    }

                    for (is_code, lines, spans) in &rendered {
                        if *is_code {
                            let code_block_h = lines.len().max(1) as f32 * geometry.line_height
                                + if is_doc_hover { CODE_PAD_Y * 2.0 } else { 0.0 };
                            let code_text_x = if is_doc_hover {
                                content_left + CODE_PAD_X
                            } else {
                                content_left
                            };
                            let code_text_y = if is_doc_hover {
                                content_y + CODE_PAD_Y
                            } else {
                                content_y
                            };
                            if is_doc_hover
                                && content_y < content_bottom
                                && content_y + code_block_h > content_top
                            {
                                let code_bg = blend_rgba(panel_bg, self.theme.ui.status_bar_bg.as_f32(), 0.35, 1.0);
                                let code_border = blend_rgba(code_bg, self.theme.ui.border_color.as_f32(), 0.7, 1.0);
                                let clipped_code_y = content_y.max(content_top);
                                let clipped_code_h = (content_y + code_block_h).min(content_bottom) - clipped_code_y;
                                chrome_quads.push(
                                    RegionDrawInstance::new(
                                        [content_left, clipped_code_y, line_w, clipped_code_h.max(0.0)],
                                        code_border,
                                    )
                                    .with_radius(8.0),
                                );
                                chrome_quads.push(
                                    RegionDrawInstance::new(
                                        [
                                            content_left + 1.0,
                                            clipped_code_y + 1.0,
                                            line_w - 2.0,
                                            (clipped_code_h - 2.0).max(0.0),
                                        ],
                                        code_bg,
                                    )
                                    .with_radius(8.0),
                                );
                            }
                            let mut line_byte_start: usize = 0;
                            for (line_idx, line_text) in lines.iter().enumerate() {
                                let text_y = code_text_y + line_idx as f32 * geometry.line_height;
                                let line_byte_end = line_byte_start + line_text.len();
                                if text_y < content_top
                                    || text_y > content_bottom
                                    || text_y + geometry.line_height < viewport_top
                                    || text_y > viewport_bottom
                                {
                                    line_byte_start = line_byte_end + 1;
                                    continue;
                                }
                                let line_spans: Vec<StyledTextSpan> = spans
                                    .iter()
                                    .filter_map(|span| {
                                        if span.end <= line_byte_start
                                            || span.start >= line_byte_end
                                        {
                                            return None;
                                        }
                                        Some(StyledTextSpan::with_style(
                                            span.start.max(line_byte_start) - line_byte_start,
                                            span.end.min(line_byte_end) - line_byte_start,
                                            span.color_rgba,
                                            span.bold,
                                            span.italic,
                                        ))
                                    })
                                    .collect();
                                self.editor_overlay_text_system.set_size(
                                    Some(
                                        (line_w
                                            - if is_doc_hover {
                                                CODE_PAD_X * 2.0
                                            } else {
                                                0.0
                                            })
                                        .max(1.0),
                                    ),
                                    Some(geometry.line_height),
                                );
                                glyphs.extend(layout_panel_rich_text(
                                    line_text,
                                    &line_spans,
                                    self.theme.ui.fg.as_f32(),
                                    &mut self.editor_overlay_text_system,
                                    &mut self.atlas,
                                    &self.queue,
                                    code_text_x,
                                    text_y,
                                ));
                                line_byte_start = line_byte_end + 1;
                            }
                            content_y += code_block_h + block_gap;
                        } else {
                            for (line_idx, line_text) in lines.iter().enumerate() {
                                let text_y = content_y + line_idx as f32 * geometry.line_height;
                                if text_y < content_top
                                    || text_y > content_bottom
                                    || text_y + geometry.line_height < viewport_top
                                    || text_y > viewport_bottom
                                {
                                    continue;
                                }
                                self.editor_overlay_text_system
                                    .set_size(Some(line_w), Some(geometry.line_height));
                                glyphs.extend(layout_panel_text(
                                    line_text,
                                    &mut self.editor_overlay_text_system,
                                    &mut self.atlas,
                                    &self.queue,
                                    content_left,
                                    text_y,
                                    self.theme.ui.fg_dim.as_f32(),
                                ));
                            }
                            content_y += lines.len().max(1) as f32 * geometry.line_height + block_gap;
                        }
                    }
                }
            }
        }

        if app_state.is_completion_loading() && app_state.completion().is_none() {
            let char_w = (geometry.font_size * 0.6).max(1.0);
            let (cursor_line, cursor_col) = app_state.cursor_line_col();
            let spinner_x = geometry.origin_x + cursor_col as f32 * char_w;
            let spinner_y = geometry.origin_y
                + cursor_line as f32 * geometry.line_height
                + geometry.line_height
                + 2.0;
            let line_h = geometry.line_height;
            const PAD_X: f32 = 14.0;
            const PAD_Y: f32 = 4.0;
            const TEXT: &str = "⟳  Loading…";
            let spinner_w = TEXT.chars().count() as f32 * char_w + PAD_X * 2.0;
            let spinner_h = line_h + PAD_Y * 2.0;
            let bg = self.theme.ui.panel_bg.as_f32();
            let border = self.theme.ui.border_color.as_f32();
            let fg_dim = self.theme.ui.fg_dim.as_f32();
            // Border rect (full size), then inset bg
            chrome_quads.push(RegionDrawInstance::new(
                [spinner_x, spinner_y, spinner_w, spinner_h],
                border,
            ));
            chrome_quads.push(RegionDrawInstance::new(
                [
                    spinner_x + 1.0,
                    spinner_y + 1.0,
                    spinner_w - 2.0,
                    spinner_h - 2.0,
                ],
                bg,
            ));
            self.editor_overlay_text_system
                .set_metrics(Metrics::new(geometry.font_size, line_h));
            self.editor_overlay_text_system
                .set_size(Some(spinner_w), Some(spinner_h));
            glyphs.extend(layout_panel_text_italic(
                TEXT,
                &mut self.editor_overlay_text_system,
                &mut self.atlas,
                &self.queue,
                spinner_x + PAD_X,
                spinner_y + PAD_Y,
                fg_dim,
            ));
        }

        if let Some(completion) = app_state.completion() {
            // Don't render popup if there are no items to show
            if completion.filtered_items.is_empty() {
                return;
            }

            const PAD_X: f32 = 30.0;
            const PAD_Y: f32 = 20.0;
            const MAX_VISIBLE_ROWS: usize = 10;
            const BADGE_GAP: f32 = 25.0;
            const SCROLLBAR_W: f32 = 7.5;
            const FOOTER_H: f32 = 65.0;
            const DOC_PANEL_W: f32 = 650.0;
            const DOC_PAD: f32 = 35.0;
            const MIN_LIST_W: f32 = 700.0;
            const MAX_LIST_W_WITH_DOCS: f32 = 1200.0;

            let char_w = (geometry.font_size * 0.6).max(1.0);
            // Badge size tracks row height so it never overflows the row
            // Row height slightly taller than natural line height
            let popup_row_h = (geometry.line_height * 1.4).max(28.0);
            // Text uses editor's actual font metrics — no size cap
            let popup_label_line_h = geometry.line_height;
            // Offset so the text block sits centered inside the taller row
            let text_v_center = (popup_row_h - popup_label_line_h) * 0.5;
            let badge_size = (popup_row_h * 0.82).clamp(28.0, 44.0);
            let badge_radius = badge_size * 0.22;
            let badge_col_w = PAD_X + badge_size + BADGE_GAP;

            // Determine if doc panel should show
            let selected_item = (completion.selected_index < completion.filtered_items.len())
                .then(|| &completion.filtered_items[completion.selected_index]);
            // Show the doc panel when there is any displayable content OR when the
            // item came from LSP (has raw_json) so resolve can fill it in.
            // This ensures the panel appears consistently for TypeScript too, not just
            // for languages that return inline detail/documentation upfront.
            let has_doc = selected_item.is_some_and(|item| {
                item.item
                    .detail
                    .as_ref()
                    .is_some_and(|d| !d.trim().is_empty())
                    || item
                        .item
                        .documentation
                        .as_ref()
                        .is_some_and(|d| !d.trim().is_empty())
                    || item.item.raw_json.is_some()
            }) || completion
                .hover_doc
                .as_ref()
                .is_some_and(|d| !d.trim().is_empty());

            let visible_rows = completion.filtered_items.len().min(MAX_VISIBLE_ROWS).max(1);
            let row_gap = 4.0_f32;
            let row_h = popup_row_h + row_gap;
            let rows_h = visible_rows as f32 * row_h - row_gap;
            let popup_h = rows_h + PAD_Y * 2.0 + FOOTER_H;

            // List panel width
            let max_label_chars = completion
                .filtered_items
                .iter()
                .map(|item| item.item.label.chars().count())
                .max()
                .unwrap_or(8);
            let label_area_w = max_label_chars as f32 * char_w;
            let desired_list_w = badge_col_w + label_area_w + PAD_X + SCROLLBAR_W;
            let list_w = if has_doc {
                clamp_popup_width(desired_list_w, MIN_LIST_W, MAX_LIST_W_WITH_DOCS)
            } else {
                clamp_popup_width(desired_list_w, MIN_LIST_W, geometry.viewport_text_width)
            };
            let doc_w = if has_doc {
                DOC_PANEL_W.min(geometry.viewport_text_width - list_w - 16.0)
            } else {
                0.0
            };
            let popup_w = list_w + doc_w;

            debug_assert!(
                popup_w.is_finite() && popup_w >= 1.0,
                "completion popup width must stay finite: list={list_w}, doc={doc_w}, viewport={}",
                geometry.viewport_text_width
            );

            // Anchor the popup to the currently rendered caret, not to `line * line_height`.
            // `geometry.origin_y` already includes scroll, but using logical line math here breaks
            // for long files and for soft-wrapped/folded lines because cosmic-text layout positions
            // are visual. Reuse the same caret layout path as the cursor renderer so completion
            // stays attached to the cursor around line 700+ and with wrapped content.
            let caret_layout = compute_caret_layout_with_folds(
                &self.text_system,
                app_state,
                [geometry.origin_x, geometry.origin_y],
                app_state.folded_ranges(),
            );
            let anchor_x = (caret_layout.x - completion.typed_prefix.chars().count() as f32 * char_w)
                .max(geometry.viewport_text_left);
            let popup_right = anchor_x + popup_w;
            let anchor_x = if popup_right > viewport_right - 8.0 {
                (anchor_x - (popup_right - viewport_right + 8.0)).max(center_bounds[0] + 4.0)
            } else {
                anchor_x.max(center_bounds[0] + 4.0)
            };
            let anchor_y = caret_layout.top;
            let cursor_bottom = anchor_y + caret_layout.height;
            let mut popup_y = cursor_bottom + 2.0;
            let popup_bottom = popup_y + popup_h;
            if popup_bottom > viewport_bottom - 12.0 {
                popup_y = (anchor_y - popup_h - 2.0).max(center_bounds[1] + 4.0);
            }

            let bg = self.theme.ui.panel_bg.as_f32();
            let border = self.theme.ui.border_color.as_f32();
            let sel_bg = self.theme.ui.selection_bg.as_f32();
            let sel_accent = self.theme.ui.cyan.as_f32();
            let fg_dim = self.theme.ui.fg_dim.as_f32();
            let fg = self.theme.ui.fg.as_f32();
            let fg_ghost = self.theme.ui.fg_ghost.as_f32();
            let cyan = self.theme.ui.cyan.as_f32();
            let success = self.theme.ui.success.as_f32();

            let popup_x = anchor_x;
            // Doc panel background: slightly lighter blend
            let doc_bg = blend_rgba(bg, fg, 0.02, 1.0);
            // Drop shadow: black at 45% alpha, offset +2x/+4y, oversized
            let shadow_color: [f32; 4] = [0.0, 0.0, 0.0, 0.45];
            let shadow_offset_x = 2.0;
            let shadow_offset_y = 4.0;

            // --- Drop shadow ---
            chrome_quads.push(RegionDrawInstance::new(
                [
                    popup_x + shadow_offset_x,
                    popup_y + shadow_offset_y,
                    popup_w + 4.0,
                    popup_h + 8.0,
                ],
                shadow_color,
            ));

            // --- Outer border ---
            chrome_quads.push(RegionDrawInstance::new(
                [popup_x, popup_y, popup_w, popup_h],
                border,
            ));
            // --- Inner background: list panel ---
            chrome_quads.push(RegionDrawInstance::new(
                [popup_x + 1.0, popup_y + 1.0, list_w - 2.0, popup_h - 2.0],
                bg,
            ));
            // --- Inner background: doc panel (if present) ---
            if has_doc {
                chrome_quads.push(RegionDrawInstance::new(
                    [
                        popup_x + list_w + 1.0,
                        popup_y + 1.0,
                        doc_w - 1.0,
                        popup_h - 2.0,
                    ],
                    doc_bg,
                ));
                // Divider
                chrome_quads.push(RegionDrawInstance::new(
                    [popup_x + list_w, popup_y, 1.0, popup_h],
                    border,
                ));
            }

            // --- LIST PANEL ---
            let total_items = completion.filtered_items.len();
            let max_visible = visible_rows;
            let scroll_offset = if total_items <= max_visible {
                0
            } else if completion.selected_index + 1 > max_visible {
                // Scroll down so selected is at bottom
                completion.selected_index + 1 - max_visible
            } else {
                // Selected is within first page — keep at top
                0
            };
            let scroll_start = scroll_offset;
            let visible_items = completion
                .filtered_items
                .iter()
                .enumerate()
                .skip(scroll_start)
                .take(visible_rows);

            for (row_idx, (item_idx, item)) in visible_items.enumerate() {
                let is_selected = item_idx == completion.selected_index;
                let row_y = popup_y + PAD_Y + row_idx as f32 * row_h;

                // --- Selected row: bg + left accent bar (list panel only) ---
                if is_selected {
                    chrome_quads.push(RegionDrawInstance::new(
                        [popup_x, row_y, list_w, popup_row_h],
                        sel_bg,
                    ));
                    chrome_quads.push(RegionDrawInstance::new(
                        [popup_x, row_y, 2.0, popup_row_h],
                        sel_accent,
                    ));
                }

                // --- KIND BADGE ---
                let kind_badge = completion_kind_badge(item.item.kind, &self.theme);
                let kind_color = kind_badge.color;
                let icon_text_color = blend_rgba(bg, kind_color, 0.78, 1.0);
                let kind_bg = blend_rgba(bg, kind_color, 0.10, 1.0);
                let kind_border_color = blend_rgba(bg, kind_color, 0.86, 1.0);
                let badge_x = popup_x + PAD_X;
                let badge_y = row_y + (popup_row_h - badge_size) * 0.5;
                let badge_border_thickness = (badge_size * 0.075).clamp(2.0, 3.0);

                // Bold kind-colored glyph and outline with a subtle 10% kind-colored fill.
                chrome_quads.push(
                    RegionDrawInstance::new(
                        [badge_x, badge_y, badge_size, badge_size],
                        kind_border_color,
                    )
                    .with_radius(badge_radius),
                );
                chrome_quads.push(
                    RegionDrawInstance::new(
                        [
                            badge_x + badge_border_thickness,
                            badge_y + badge_border_thickness,
                            (badge_size - badge_border_thickness * 2.0).max(1.0),
                            (badge_size - badge_border_thickness * 2.0).max(1.0),
                        ],
                        kind_bg,
                    )
                    .with_radius((badge_radius - badge_border_thickness).max(2.0)),
                );

                if let Some(asset_icon) = canonical_icon_id(kind_badge.icon) {
                    let icon_size = (badge_size * 0.72).max(12.0);
                    icon_instances.push(IconDrawInstance {
                        icon: asset_icon,
                        rect: [
                            badge_x + (badge_size - icon_size) * 0.5,
                            badge_y + (badge_size - icon_size) * 0.5,
                            icon_size,
                            icon_size,
                        ],
                        tint: icon_text_color,
                    });
                } else {
                    // Legacy fallback for user-configured non-Bearded badge glyphs.
                    let icon_char_count = kind_badge.icon.chars().count();
                    let icon_size = if icon_char_count > 1 {
                        (badge_size * 0.50).max(10.0)
                    } else {
                        (badge_size * 0.68).max(12.0)
                    };
                    let icon_w = estimate_monospace_width(kind_badge.icon, icon_size);
                    let icon_x = badge_x + (badge_size - icon_w) * 0.5;
                    let icon_line_h = icon_size * 1.15;
                    let icon_y = badge_y + (badge_size - icon_line_h) * 0.5 + icon_size * 0.08;
                    self.editor_overlay_text_system
                        .set_metrics(Metrics::new(icon_size, icon_line_h));
                    self.editor_overlay_text_system
                        .set_size(Some(badge_size), Some(icon_line_h));
                    let icon_glyphs = layout_panel_text_bold(
                        kind_badge.icon,
                        &mut self.editor_overlay_text_system,
                        &mut self.atlas,
                        &self.queue,
                        icon_x,
                        icon_y,
                        icon_text_color,
                    );
                    glyphs.extend(icon_glyphs);
                    self.editor_overlay_text_system
                        .set_metrics(Metrics::new(geometry.font_size, geometry.line_height));
                }

                // --- LABEL ---
                let label_x = popup_x + badge_col_w;
                let inline_detail = item
                    .item
                    .detail
                    .as_deref()
                    .map(str::trim)
                    .filter(|detail| !detail.is_empty())
                    .map(|detail| {
                        clamp_monospace_text(
                            detail,
                            (list_w * 0.35).max(char_w * 8.0),
                            geometry.font_size,
                        )
                    })
                    .unwrap_or_default();
                let inline_detail_w = if inline_detail.is_empty() {
                    0.0
                } else {
                    estimate_monospace_width(&inline_detail, geometry.font_size)
                };
                let label_max_w = if inline_detail.is_empty() {
                    list_w - badge_col_w - PAD_X - SCROLLBAR_W
                } else {
                    list_w - badge_col_w - PAD_X - SCROLLBAR_W - inline_detail_w - PAD_X
                };
                let label_max_w = label_max_w.max(char_w * 4.0);

                let spans = completion_label_spans(item, cyan);
                let label_color = if is_selected { fg } else { fg_dim };
                // Label: restore editor metrics, render centered in row
                self.editor_overlay_text_system
                    .set_metrics(Metrics::new(geometry.font_size, popup_label_line_h));
                self.editor_overlay_text_system
                    .set_size(Some(label_max_w), Some(popup_label_line_h));
                glyphs.extend(layout_panel_rich_text(
                    &item.item.label,
                    &spans,
                    label_color,
                    &mut self.editor_overlay_text_system,
                    &mut self.atlas,
                    &self.queue,
                    label_x,
                    row_y + text_v_center,
                ));

                // Inline detail text
                if !inline_detail.is_empty() {
                    let idetail_x = popup_x + list_w - PAD_X - SCROLLBAR_W - inline_detail_w;
                    self.editor_overlay_text_system
                        .set_size(Some(inline_detail_w.max(1.0)), Some(popup_label_line_h));
                    glyphs.extend(layout_panel_text(
                        &inline_detail,
                        &mut self.editor_overlay_text_system,
                        &mut self.atlas,
                        &self.queue,
                        idetail_x,
                        row_y + text_v_center,
                        fg_dim,
                    ));
                }
                self.editor_overlay_text_system
                    .set_metrics(Metrics::new(geometry.font_size, geometry.line_height));
            }

            // --- LIST PANEL SCROLLBAR ---
            let total_items = completion.filtered_items.len();
            if total_items > visible_rows {
                let scroll_x = popup_x + list_w - SCROLLBAR_W;
                let scroll_y = popup_y + PAD_Y;
                let scroll_h = rows_h;
                chrome_quads.push(RegionDrawInstance::new(
                    [scroll_x, scroll_y, SCROLLBAR_W, scroll_h],
                    border,
                ));
                let thumb_ratio = visible_rows as f32 / total_items as f32;
                let thumb_h = (scroll_h * thumb_ratio).max(8.0);
                let progress = completion.selected_index as f32
                    / (total_items.saturating_sub(1).max(1) as f32);
                let thumb_y = scroll_y + (scroll_h - thumb_h) * progress;
                chrome_quads.push(RegionDrawInstance::new(
                    [scroll_x, thumb_y, SCROLLBAR_W, thumb_h],
                    sel_accent,
                ));
            }

            // --- DOC PANEL ---
            if has_doc {
                if let Some(item) = selected_item {
                    let doc_x = popup_x + list_w;

                    let doc_content_w = doc_w - DOC_PAD * 2.0;
                    let mut doc_y = popup_y + DOC_PAD;
                    let doc_signature_font_size = geometry.font_size * 0.85;
                    let doc_body_font_size = geometry.font_size * 0.80;
                    let doc_line_h = doc_body_font_size * 1.5;

                    // --- SIGNATURE ---
                    let mut signature_rendered = false;
                    if let Some(detail) = item.item.detail.as_ref() {
                        let detail_trimmed = detail.trim().to_string();
                        if !detail_trimmed.is_empty() {
                            signature_rendered = true;
                            let max_sig_chars =
                                (doc_content_w / (doc_signature_font_size * 0.6)) as usize;
                            let mut sig_lines =
                                wrap_text_lines(&detail_trimmed, max_sig_chars.max(12));
                            sig_lines.truncate(4);
                            if sig_lines.len() == 4 {
                                sig_lines = sig_lines.into_iter().take(3).collect();
                                let mut last = sig_lines.last().cloned().unwrap_or_default();
                                last = last.trim_end().to_string();
                                last.push('…');
                                sig_lines.push(last);
                            }
                            let sig_line_h = doc_signature_font_size * 1.4;
                            let sig_h = sig_lines.len() as f32 * sig_line_h;
                            self.editor_overlay_text_system
                                .set_metrics(Metrics::new(doc_signature_font_size, sig_line_h));
                            for (li, line) in sig_lines.iter().enumerate() {
                                let line_y = doc_y + li as f32 * sig_line_h;
                                self.editor_overlay_text_system
                                    .set_size(Some(doc_content_w), Some(sig_line_h));
                                glyphs.extend(layout_panel_text_italic(
                                    line,
                                    &mut self.editor_overlay_text_system,
                                    &mut self.atlas,
                                    &self.queue,
                                    doc_x + DOC_PAD,
                                    line_y,
                                    cyan,
                                ));
                            }
                            doc_y += sig_h + 4.0;

                            // Signature separator
                            chrome_quads.push(RegionDrawInstance::new(
                                [doc_x + DOC_PAD, doc_y, doc_content_w, 1.0],
                                border,
                            ));
                            doc_y += 8.0;
                        }
                    }

                    // --- DOCUMENTATION ---
                    let has_documentation = item
                        .item
                        .documentation
                        .as_ref()
                        .is_some_and(|d| !d.trim().is_empty());

                    if has_documentation {
                        let doc_str = item.item.documentation.as_ref().unwrap();
                        let doc_clean = strip_markdown_inline(doc_str);
                        if !doc_clean.trim().is_empty() {
                            let max_body_chars =
                                (doc_content_w / (doc_body_font_size * 0.52)) as usize;
                            let body_lines = wrap_text_lines(&doc_clean, max_body_chars.max(10));
                            let remaining_h = (popup_y + popup_h - 8.0 - doc_y).max(doc_line_h);
                            let max_visible = (remaining_h / doc_line_h) as usize;
                            let visible_body = body_lines
                                .iter()
                                .take(max_visible.max(1))
                                .collect::<Vec<_>>();
                            self.editor_overlay_text_system
                                .set_metrics(Metrics::new(doc_body_font_size, doc_line_h));
                            for (li, line) in visible_body.iter().enumerate() {
                                let line_y = doc_y + li as f32 * doc_line_h;
                                self.editor_overlay_text_system
                                    .set_size(Some(doc_content_w), Some(doc_line_h));
                                glyphs.extend(layout_panel_text_italic(
                                    line,
                                    &mut self.editor_overlay_text_system,
                                    &mut self.atlas,
                                    &self.queue,
                                    doc_x + DOC_PAD,
                                    line_y,
                                    fg_dim,
                                ));
                            }
                        }
                    } else if let Some(hover_text) = completion
                        .hover_doc
                        .as_ref()
                        .filter(|d| !d.trim().is_empty())
                    {
                        // Hover doc fetched via LSP hover request for this item
                        let doc_clean = strip_markdown_inline(hover_text);
                        if !doc_clean.trim().is_empty() {
                            let max_body_chars =
                                (doc_content_w / (doc_body_font_size * 0.52)) as usize;
                            let body_lines = wrap_text_lines(&doc_clean, max_body_chars.max(10));
                            let remaining_h = (popup_y + popup_h - 8.0 - doc_y).max(doc_line_h);
                            let max_visible = (remaining_h / doc_line_h) as usize;
                            let visible_body = body_lines
                                .iter()
                                .take(max_visible.max(1))
                                .collect::<Vec<_>>();
                            self.editor_overlay_text_system
                                .set_metrics(Metrics::new(doc_body_font_size, doc_line_h));
                            for (li, line) in visible_body.iter().enumerate() {
                                let line_y = doc_y + li as f32 * doc_line_h;
                                self.editor_overlay_text_system
                                    .set_size(Some(doc_content_w), Some(doc_line_h));
                                glyphs.extend(layout_panel_text_italic(
                                    line,
                                    &mut self.editor_overlay_text_system,
                                    &mut self.atlas,
                                    &self.queue,
                                    doc_x + DOC_PAD,
                                    line_y,
                                    fg_dim,
                                ));
                            }
                        }
                    } else {
                        // Either still loading, or resolve finished with no docs available.
                        // When the signature is already rendered above, suppress the
                        // "No documentation available" hint — the signature alone is
                        // enough information; the hint just adds noise. We still show
                        // "Loading…" so the user knows a fetch is in flight.
                        let hint_opt = if completion.hover_doc_resolved {
                            (!signature_rendered).then_some("No documentation available")
                        } else {
                            Some("Loading…")
                        };
                        if let Some(hint) = hint_opt {
                            let hint_font = doc_body_font_size;
                            let hint_line_h = hint_font * 1.5;
                            let hint_y =
                                doc_y + (popup_y + popup_h - doc_y - hint_line_h - FOOTER_H) * 0.5;
                            self.editor_overlay_text_system
                                .set_metrics(Metrics::new(hint_font, hint_line_h));
                            self.editor_overlay_text_system
                                .set_size(Some(doc_content_w), Some(hint_line_h));
                            glyphs.extend(layout_panel_text_italic(
                                hint,
                                &mut self.editor_overlay_text_system,
                                &mut self.atlas,
                                &self.queue,
                                doc_x + DOC_PAD,
                                hint_y.max(doc_y),
                                fg_ghost,
                            ));
                        }
                    }

                    // --- RETURN TYPE TAG ---
                    if let Some(detail) = item.item.detail.as_ref() {
                        if let Some(arrow_pos) = detail.find("->") {
                            let return_type = detail[arrow_pos + 2..].trim();
                            if !return_type.is_empty() {
                                let tag_text = return_type;
                                let tag_w =
                                    estimate_monospace_width(tag_text, doc_body_font_size) + 12.0;
                                let tag_h = doc_body_font_size * 1.3;
                                let tag_x = doc_x + DOC_PAD;
                                let tag_y = popup_y + popup_h - tag_h - 8.0;
                                // Background
                                chrome_quads.push(RegionDrawInstance::new(
                                    [tag_x, tag_y, tag_w, tag_h],
                                    [success[0], success[1], success[2], (success[3] * 0.08)],
                                ));
                                // Border
                                chrome_quads.push(RegionDrawInstance::new(
                                    [tag_x, tag_y, tag_w, tag_h],
                                    [success[0], success[1], success[2], (success[3] * 0.25)],
                                ));
                                let tag_text_w =
                                    estimate_monospace_width(tag_text, doc_body_font_size);
                                let tag_text_x = tag_x + (tag_w - tag_text_w) * 0.5;
                                self.editor_overlay_text_system
                                    .set_metrics(Metrics::new(doc_body_font_size, tag_h));
                                self.editor_overlay_text_system
                                    .set_size(Some(tag_w), Some(tag_h));
                                glyphs.extend(layout_panel_text(
                                    tag_text,
                                    &mut self.editor_overlay_text_system,
                                    &mut self.atlas,
                                    &self.queue,
                                    tag_text_x,
                                    tag_y,
                                    success,
                                ));
                            }
                        }
                    }
                }
            }

            // --- FOOTER ---
            let footer_y = popup_y + PAD_Y + rows_h + PAD_Y;
            chrome_quads.push(RegionDrawInstance::new(
                [popup_x, footer_y, popup_w, 1.0],
                border,
            ));
            let hint_font_size = (geometry.font_size * 0.85).max(11.0);
            let hint_line_h = hint_font_size * 1.4;

            // Count workspace symbol items
            let workspace_count = completion.filtered_items.iter()
                .filter(|item| matches!(item.source, crate::app::app_state::CompletionItemSource::WorkspaceSymbol))
                .count();
            let total_count = completion.filtered_items.len();

            // Check if indexing is in progress
            let is_indexing = completion.language_id.as_ref()
                .map(|lang_id| app_state.workspace_symbol_cache().is_indexing(lang_id))
                .unwrap_or(false);

            // Status text on the left
            let status_text = if is_indexing {
                format!("{} items (indexing…)", total_count)
            } else if workspace_count > 0 {
                format!("{} items ({} from workspace)", total_count, workspace_count)
            } else {
                format!("{} items", total_count)
            };
            let status_w = estimate_monospace_width(&status_text, hint_font_size);
            let status_x = popup_x + PAD_X;
            let status_y = footer_y + (FOOTER_H - hint_line_h) * 0.5;
            self.editor_overlay_text_system
                .set_metrics(Metrics::new(hint_font_size, hint_line_h));
            self.editor_overlay_text_system
                .set_size(Some(status_w + 4.0), Some(hint_line_h));
            glyphs.extend(layout_panel_text(
                &status_text,
                &mut self.editor_overlay_text_system,
                &mut self.atlas,
                &self.queue,
                status_x,
                status_y,
                if is_indexing { cyan } else { fg_dim },
            ));

            // Hint text on the right
            let hint_text = "↑↓ | ↩ accept | 󱊷 close";
            let hint_w = estimate_monospace_width(hint_text, hint_font_size);
            let hint_x = (popup_x + popup_w - hint_w - PAD_X).max(popup_x + PAD_X + status_w + 20.0);
            let hint_y = footer_y + (FOOTER_H - hint_line_h) * 0.5;
            self.editor_overlay_text_system
                .set_metrics(Metrics::new(hint_font_size, hint_line_h));
            self.editor_overlay_text_system
                .set_size(Some(hint_w + 4.0), Some(hint_line_h));
            glyphs.extend(layout_panel_text(
                hint_text,
                &mut self.editor_overlay_text_system,
                &mut self.atlas,
                &self.queue,
                hint_x,
                hint_y,
                fg_ghost,
            ));
            self.editor_overlay_text_system
                .set_metrics(Metrics::new(geometry.font_size, geometry.line_height));
        }

        let ghost_glyphs = self.collect_inline_suggestion_glyphs(
            app_state,
            geometry.origin_x,
            geometry.origin_y,
            geometry.viewport_text_width,
        );
        glyphs.extend(ghost_glyphs);

        self.editor_overlay_chrome_instances = chrome_quads;
        self.editor_overlay_icon_instances = icon_instances;
        self.editor_overlay_icon_pipeline.upload_instances(
            &self.device,
            &self.editor_overlay_icon_instances,
            [self.surface_state.config.width, self.surface_state.config.height],
        );
        self.editor_overlay_glyph_instances = glyphs;
        self.editor_overlay_text_pipeline.upload_instances(
            &self.device,
            &self.queue,
            &self.editor_overlay_glyph_instances,
        );
    }
}

/// Blend two RGBA colors.
fn blend_rgba(base: [f32; 4], tint: [f32; 4], amount: f32, alpha: f32) -> [f32; 4] {
    let t = amount.clamp(0.0, 1.0);
    [
        base[0] * (1.0 - t) + tint[0] * t,
        base[1] * (1.0 - t) + tint[1] * t,
        base[2] * (1.0 - t) + tint[2] * t,
        alpha.clamp(0.0, 1.0),
    ]
}

/// Strip markdown inline formatting (**, __, `, etc.) from a documentation string.
fn strip_markdown_inline(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let chars: Vec<char> = text.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        // Bold **...**
        if i + 1 < chars.len() && chars[i] == '*' && chars[i + 1] == '*' {
            i += 2;
            while i + 1 < chars.len() {
                if chars[i] == '*' && chars[i + 1] == '*' {
                    i += 2;
                    break;
                }
                out.push(chars[i]);
                i += 1;
            }
            continue;
        }
        // Italic *...*
        if i < chars.len() && chars[i] == '*' {
            i += 1;
            while i < chars.len() && chars[i] != '*' {
                out.push(chars[i]);
                i += 1;
            }
            if i < chars.len() {
                i += 1;
            } // skip closing *
            continue;
        }
        // Bold __...__
        if i + 1 < chars.len() && chars[i] == '_' && chars[i + 1] == '_' {
            i += 2;
            while i + 1 < chars.len() {
                if chars[i] == '_' && chars[i + 1] == '_' {
                    i += 2;
                    break;
                }
                out.push(chars[i]);
                i += 1;
            }
            continue;
        }
        // Italic _..._
        if i < chars.len() && chars[i] == '_' {
            i += 1;
            while i < chars.len() && chars[i] != '_' {
                out.push(chars[i]);
                i += 1;
            }
            if i < chars.len() {
                i += 1;
            }
            continue;
        }
        // Inline code `...`
        if i < chars.len() && chars[i] == '`' {
            i += 1;
            while i < chars.len() && chars[i] != '`' {
                out.push(chars[i]);
                i += 1;
            }
            if i < chars.len() {
                i += 1;
            }
            continue;
        }
        out.push(chars[i]);
        i += 1;
    }
    out
}

#[cfg(test)]
mod tests {
    #[test]
    fn completion_popup_width_and_anchor_remain_valid_on_narrow_viewport() {
        let char_w = 9.5_f32;
        let max_len = 32usize;
        let desired_popup_w = max_len as f32 * char_w + 20.0;
        let viewport_text_width = 80.7749_f32;
        let viewport_text_left = 110.525_f32;
        let viewport_right = 191.2999_f32;
        let anchor_candidate = 150.0_f32;

        let available_popup_w = viewport_text_width.max(1.0);
        let preferred_min_popup_w = 160.0_f32.min(available_popup_w);
        let popup_w = desired_popup_w.clamp(preferred_min_popup_w, available_popup_w);
        let max_anchor_x = (viewport_right - popup_w).max(viewport_text_left);
        let anchor_x = anchor_candidate.clamp(viewport_text_left, max_anchor_x);

        assert!((popup_w - 80.7749).abs() < 0.001);
        assert!((max_anchor_x - viewport_text_left).abs() < 0.001);
        assert!((anchor_x - viewport_text_left).abs() < 0.001);
    }

    #[test]
    fn strip_markdown_bold_and_italic() {
        let input = "**bold** and *italic* text";
        assert_eq!(super::strip_markdown_inline(input), "bold and italic text");
    }

    #[test]
    fn strip_markdown_code_and_underscore() {
        let input = "__bold__ `code` _italic_ normal";
        assert_eq!(
            super::strip_markdown_inline(input),
            "bold code italic normal"
        );
    }

    #[test]
    fn strip_markdown_keeps_plain_text() {
        let input = "just plain text here";
        assert_eq!(super::strip_markdown_inline(input), "just plain text here");
    }
}
