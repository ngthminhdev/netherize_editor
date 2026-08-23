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
        compute_caret_layout, compute_caret_layout_at_with_folds, compute_caret_layout_with_folds,
        compute_cursor_overlay, rebuild_layout_projection,
    },
};
use cosmic_text::Metrics;

use super::super::components::{estimate_help_keycaps_width, layout_help_keycaps};
use super::super::helpers::{
    caret_rect_for_mode, clamp_monospace_text, clamp_monospace_text_left, clamp_popup_width,
    estimate_monospace_width, gutter_width_for_editor, layout_panel_rich_text, layout_panel_text,
    layout_panel_text_bold, layout_panel_text_italic, rect_to_scissor, should_draw_block_cursor,
};
use super::completion::{completion_kind_badge, completion_label_spans};
use super::{EDITOR_BREADCRUMB_GAP_Y, EDITOR_BREADCRUMB_TOP_INSET};
use crate::render::icon_pipeline::{IconDrawInstance, canonical_icon_id};

const DIAGNOSTIC_SEVERITY_ERROR: u32 = 1;
const DIAGNOSTIC_SEVERITY_WARNING: u32 = 2;
const EDITOR_BREADCRUMB_FRAME_INSET_X: f32 = 4.0;

/// (errors, warnings) — LSP severity `None` counts as warning, matching the
/// `unwrap_or` rule used for the diagnostic line highlights below.
fn diagnostic_counts(
    diagnostics: &[crate::async_runtime::message::LspDiagnostic],
) -> (usize, usize) {
    let mut errors = 0;
    let mut warnings = 0;
    for diagnostic in diagnostics {
        match diagnostic.severity.unwrap_or(DIAGNOSTIC_SEVERITY_WARNING) {
            DIAGNOSTIC_SEVERITY_ERROR => errors += 1,
            DIAGNOSTIC_SEVERITY_WARNING => warnings += 1,
            _ => {}
        }
    }
    (errors, warnings)
}
use super::{cursor_diagnostic, editor_viewport_geometry, run_x_for_byte, wrap_text_lines};
use crate::text::text_system::StyledTextSpan;

impl Renderer {
    pub fn update_editor_overlays(&mut self, app_state: &AppState, center_bounds: [f32; 4]) {
        let active_diagnostics = app_state
            .active_file()
            .and_then(|path| app_state.diagnostics_for_path(path));
        let geometry = editor_viewport_geometry(self, app_state, center_bounds);
        // Scale hardcoded popup/chrome px so they track runtime-scaled text
        // metrics across monitors (same pattern as extensions.rs).
        let ui_s = self.ui_scale.max(0.5);
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

            // `editor_overlay_text_system` is SHARED across editor overlays. The
            // file picker, diagnostics panel and Code Graph HUD leave a constrained
            // `set_size` (wrap width) and/or smaller `set_metrics` on it that would
            // otherwise leak into the breadcrumb on the next frame — causing the
            // segments to wrap and their spacing to drift. Reset to the editor's
            // metrics with an unbounded width so each segment lays out on one line.
            // (Same shared-state-leak family as the earlier HUD breadcrumb bug.)
            self.editor_overlay_text_system
                .set_metrics(Metrics::new(geometry.font_size, geometry.line_height));
            self.editor_overlay_text_system
                .set_size(None, Some(header_h.max(geometry.line_height)));

            let mut x = geometry.viewport_text_left;
            let text_y = header_y + ((header_h - geometry.line_height).max(0.0) * 0.5);
            let max_x = header_x + header_w - self.editor_padding_x;
            let separator = " › ";
            let separator_w = estimate_monospace_width(separator, geometry.font_size);
            let separator_color = self.theme.ui.fg_ghost.as_f32();
            let icon_size = (geometry.line_height * 0.82).min(geometry.font_size * 1.3);
            let icon_gap = (geometry.font_size * 0.35).max(4.0);
            let icon_y = header_y + ((header_h - icon_size).max(0.0) * 0.5);

            // Right side of the breadcrumb row (was dead space): workspace-
            // relative path + diagnostics counts, right-aligned. Skipped on
            // narrow panes; the symbol segments truncate before this block.
            let mut right_end = max_x;
            if header_w > geometry.font_size * 24.0 {
                let (error_count, warning_count) =
                    active_diagnostics.map(diagnostic_counts).unwrap_or((0, 0));
                let mut chips: Vec<(String, [f32; 4])> = Vec::new();
                if error_count > 0 {
                    chips.push((format!("✗ {error_count}"), self.theme.ui.error.as_f32()));
                }
                if warning_count > 0 {
                    chips.push((format!("⚠ {warning_count}"), self.theme.ui.warning.as_f32()));
                }
                let rel_path_label = app_state.active_file().and_then(|path| {
                    let shown = app_state
                        .workspace_root_path()
                        .and_then(|root| path.strip_prefix(root).ok())
                        .unwrap_or(path);
                    let text = shown.display().to_string();
                    let clamped = clamp_monospace_text(&text, header_w * 0.35, geometry.font_size);
                    (!clamped.is_empty()).then_some(clamped)
                });
                let item_gap = estimate_monospace_width("  ", geometry.font_size);
                for (text, color) in chips.iter().rev() {
                    right_end -= estimate_monospace_width(text, geometry.font_size);
                    glyphs.extend(layout_panel_text(
                        text,
                        &mut self.editor_overlay_text_system,
                        &mut self.atlas,
                        &self.queue,
                        right_end,
                        text_y,
                        *color,
                    ));
                    right_end -= item_gap;
                }
                if let Some(path_label) = rel_path_label {
                    right_end -= estimate_monospace_width(&path_label, geometry.font_size);
                    glyphs.extend(layout_panel_text(
                        &path_label,
                        &mut self.editor_overlay_text_system,
                        &mut self.atlas,
                        &self.queue,
                        right_end,
                        text_y,
                        self.theme.ui.fg_ghost.as_f32(),
                    ));
                    right_end -= item_gap;
                }
            }
            let max_x = right_end.min(max_x).max(geometry.viewport_text_left);

            for (index, segment) in self.editor_breadcrumb_segments.iter().enumerate() {
                let is_last = index + 1 == self.editor_breadcrumb_segments.len();
                let available = (max_x - x).max(1.0);
                if available <= 1.0 {
                    break;
                }
                let icon_w = if segment.icon_id.is_some() {
                    icon_size + icon_gap
                } else {
                    0.0
                };
                if available <= icon_w + 1.0 {
                    break;
                }
                if let Some(icon_id) = segment.icon_id {
                    icon_instances.push(IconDrawInstance {
                        icon: icon_id,
                        rect: [x, icon_y, icon_size, icon_size],
                        tint: [1.0_f32; 4],
                    });
                    x += icon_w;
                }
                let text_budget =
                    (available - icon_w - if is_last { 0.0 } else { separator_w }).max(1.0);
                let text = clamp_monospace_text(&segment.text, text_budget, geometry.font_size);
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
                // Alpha tuned against the dark editor bg: red keeps its hue
                // at low alpha, but the orange warning washes out to
                // near-neutral below ~0.25 (rgb(70,55,54) at 0.20).
                line_color[3] = if severity == DIAGNOSTIC_SEVERITY_ERROR {
                    0.12
                } else {
                    0.28
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
                    #[allow(non_snake_case)]
                    let PAD_X: f32 = 14.0 * ui_s;
                    #[allow(non_snake_case)]
                    let PAD_Y: f32 = 10.0 * ui_s;
                    #[allow(non_snake_case)]
                    let BORDER: f32 = 1.0;
                    #[allow(non_snake_case)]
                    let CODE_PAD_X: f32 = 12.0 * ui_s;
                    #[allow(non_snake_case)]
                    let CODE_PAD_Y: f32 = 8.0 * ui_s;
                    #[allow(non_snake_case)]
                    let CONTENT_INSET_X: f32 = 14.0 * ui_s;
                    #[allow(non_snake_case)]
                    let CONTENT_INSET_Y: f32 = 12.0 * ui_s;
                    let char_w = (geometry.font_size * 0.6).max(1.0);
                    let is_doc_hover = matches!(style, FloatingBoxStyle::DocHover);
                    let header_h = if is_doc_hover {
                        geometry.line_height + 14.0 * ui_s
                    } else {
                        0.0
                    };
                    let block_gap = if is_doc_hover { 10.0 * ui_s } else { 0.0 };

                    // ── Hover card size caps ─────────────────────────────────────
                    let max_popup_w = (center_bounds[2] * 0.58)
                        .max(260.0 * ui_s)
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
                        rendered.iter().filter(|(is_code, _, _)| *is_code).count() as f32
                            * (CODE_PAD_Y * 2.0 + block_gap)
                    } else {
                        0.0
                    };
                    let content_inner_pad_y = if is_doc_hover {
                        CONTENT_INSET_Y * 2.0
                    } else {
                        0.0
                    };

                    let desired_popup_w = max_len as f32 * char_w + PAD_X * 2.0;
                    let popup_w = clamp_popup_width(desired_popup_w, 260.0 * ui_s, max_popup_w);
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
                        let header_bg =
                            blend_rgba(panel_bg, self.theme.ui.status_bar_bg.as_f32(), 0.5, 1.0);
                        let header_divider = blend_rgba(header_bg, border_color, 0.55, 1.0);
                        chrome_quads.push(
                            RegionDrawInstance::new(
                                [popup_x, popup_y, popup_w, header_h],
                                header_bg,
                            )
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

                    let content_left =
                        popup_x + PAD_X + if is_doc_hover { CONTENT_INSET_X } else { 0.0 };
                    let content_top = popup_y
                        + PAD_Y
                        + header_h
                        + if is_doc_hover { CONTENT_INSET_Y } else { 0.0 };
                    let content_bottom = popup_y + popup_h
                        - PAD_Y
                        - if is_doc_hover { CONTENT_INSET_Y } else { 0.0 };
                    let needs_scrollbar =
                        is_doc_hover && header_h + total_content_h + PAD_Y * 2.0 > popup_h;
                    let scrollbar_w = if needs_scrollbar { 4.0 } else { 0.0 };
                    let line_w = (popup_w
                        - PAD_X * 2.0
                        - if is_doc_hover {
                            CONTENT_INSET_X * 2.0
                        } else {
                            0.0
                        }
                        - scrollbar_w
                        - 10.0)
                        .max(1.0);
                    let content_view_h = (content_bottom - content_top).max(geometry.line_height);
                    let max_scroll_lines =
                        ((total_content_h - content_inner_pad_y - content_view_h).max(0.0)
                            / geometry.line_height)
                            .ceil() as usize;
                    let effective_scroll_lines = scroll.offset_lines.min(max_scroll_lines);
                    let mut content_y =
                        content_top - effective_scroll_lines as f32 * geometry.line_height;

                    if needs_scrollbar {
                        let track_x = popup_x + popup_w - PAD_X - 4.0;
                        let track_y = content_top;
                        let track_h = content_view_h;
                        let scrollbar_track =
                            blend_rgba(panel_bg, self.theme.ui.border_color.as_f32(), 0.28, 1.0);
                        let scrollbar_thumb =
                            blend_rgba(scrollbar_track, self.theme.ui.accent.as_f32(), 0.62, 1.0);
                        chrome_quads.push(
                            RegionDrawInstance::new(
                                [track_x, track_y, scrollbar_w, track_h],
                                scrollbar_track,
                            )
                            .with_radius(2.0),
                        );
                        let visible_ratio =
                            (content_view_h / total_content_h.max(1.0)).clamp(0.08, 1.0);
                        let thumb_h = (track_h * visible_ratio).max(18.0).min(track_h);
                        let scroll_ratio = if max_scroll_lines == 0 {
                            0.0
                        } else {
                            effective_scroll_lines as f32 / max_scroll_lines as f32
                        };
                        let thumb_y = track_y + (track_h - thumb_h) * scroll_ratio;
                        chrome_quads.push(
                            RegionDrawInstance::new(
                                [track_x, thumb_y, scrollbar_w, thumb_h],
                                scrollbar_thumb,
                            )
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
                                let code_bg = blend_rgba(
                                    panel_bg,
                                    self.theme.ui.status_bar_bg.as_f32(),
                                    0.35,
                                    1.0,
                                );
                                let code_border = blend_rgba(
                                    code_bg,
                                    self.theme.ui.border_color.as_f32(),
                                    0.7,
                                    1.0,
                                );
                                let clipped_code_y = content_y.max(content_top);
                                let clipped_code_h =
                                    (content_y + code_block_h).min(content_bottom) - clipped_code_y;
                                chrome_quads.push(
                                    RegionDrawInstance::new(
                                        [
                                            content_left,
                                            clipped_code_y,
                                            line_w,
                                            clipped_code_h.max(0.0),
                                        ],
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
                                            - if is_doc_hover { CODE_PAD_X * 2.0 } else { 0.0 })
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
                            content_y +=
                                lines.len().max(1) as f32 * geometry.line_height + block_gap;
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
            #[allow(non_snake_case)]
            let PAD_X: f32 = 14.0 * ui_s;
            #[allow(non_snake_case)]
            let PAD_Y: f32 = 4.0 * ui_s;
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
            // Single-column completion menu — the SAME compact component the
            // NetherCanvas in-card editor uses (no doc panel). See
            // `completion::draw_completion_menu`.
            let char_w = (geometry.font_size * 0.6).max(1.0);
            let caret_layout = compute_caret_layout_with_folds(
                &self.text_system,
                app_state,
                [geometry.origin_x, geometry.origin_y],
                app_state.folded_ranges(),
            );
            let anchor_x = (caret_layout.x
                - completion.typed_prefix.chars().count() as f32 * char_w)
                .max(geometry.viewport_text_left);
            super::completion::draw_completion_menu(
                completion,
                super::completion::CompletionMenuGeom {
                    anchor_x,
                    caret_top: caret_layout.top,
                    caret_bottom: caret_layout.top + caret_layout.height,
                    bounds: center_bounds,
                    font_size: geometry.font_size,
                    line_height: geometry.line_height,
                    ui_scale: ui_s,
                },
                &self.theme,
                super::completion::MenuRenderTargets {
                    text_system: &mut self.editor_overlay_text_system,
                    atlas: &mut self.atlas,
                    queue: &self.queue,
                    chrome: &mut chrome_quads,
                    glyphs: &mut glyphs,
                    icons: &mut icon_instances,
                },
            );
        }

        let ghost_glyphs = self.collect_inline_suggestion_glyphs(
            app_state,
            geometry.origin_x,
            geometry.origin_y,
            geometry.viewport_text_width,
        );
        glyphs.extend(ghost_glyphs);

        // Code Graph HUD draws on top of all other editor overlays.
        self.append_code_graph_hud(
            app_state,
            center_bounds,
            geometry.font_size,
            geometry.line_height,
            &mut chrome_quads,
            &mut glyphs,
        );

        self.editor_overlay_chrome_instances = chrome_quads;
        self.editor_overlay_icon_instances = icon_instances;
        self.editor_overlay_icon_pipeline.upload_instances(
            &self.device,
            &self.editor_overlay_icon_instances,
            [
                self.surface_state.config.width,
                self.surface_state.config.height,
            ],
        );
        self.editor_overlay_glyph_instances = glyphs;
        self.editor_overlay_text_pipeline.upload_instances(
            &self.device,
            &self.queue,
            &self.editor_overlay_glyph_instances,
        );
    }

    /// Render the Code Graph HUD overlay (`gp`) on top of the editor. Appends
    /// flat quads and text glyphs to the editor-overlay batches.
    pub fn append_code_graph_hud(
        &mut self,
        app_state: &AppState,
        center_bounds: [f32; 4],
        font_size: f32,
        line_h: f32,
        quads: &mut Vec<RegionDrawInstance>,
        glyphs: &mut Vec<GlyphInstance>,
    ) {
        use crate::app::app_state::code_graph_hud::CodeGraphHudStatus;
        use crate::codegraph::edges::elbow;
        use crate::codegraph::layout::{PillRect, layout};
        use crate::codegraph::model::RiskLevel;
        use crate::codegraph::navigation::Focus;

        let hud = &app_state.code_graph_hud;
        if !hud.open {
            return;
        }

        let ui_s = self.ui_scale.max(0.5);
        let radius = self.panel_corner_radius;

        // The HUD uses a denser font than the editor. The overlay text system is
        // SHARED (breadcrumb etc.), so we restore its metrics before returning.
        let fs = (font_size * 0.82).max(11.0 * ui_s);
        let lh = fs * 1.34;
        self.editor_overlay_text_system
            .set_metrics(Metrics::new(fs, lh));

        let c_bg = self.theme.ui.panel_bg.as_f32();
        let c_border = self.theme.ui.border_color.as_f32();
        let c_fg = self.theme.ui.fg.as_f32();
        let c_dim = self.theme.ui.fg_dim.as_f32();
        let c_ghost = self.theme.ui.fg_ghost.as_f32();
        let c_cyan = self.theme.ui.cyan.as_f32();
        let c_amber = self.theme.ui.amber.as_f32();
        let c_green = self.theme.ui.success.as_f32();
        let c_red = self.theme.ui.error.as_f32();
        let c_accent = self.theme.ui.accent.as_f32();
        let c_info = self.theme.ui.info.as_f32();
        let c_overlay = self.theme.ui.overlay_bg.as_f32();

        let with_alpha = |c: [f32; 4], a: f32| [c[0], c[1], c[2], a];
        let risk_color = |r: RiskLevel| match r {
            RiskLevel::Focal => c_cyan,
            RiskLevel::Safe => c_green,
            RiskLevel::Medium => c_amber,
            RiskLevel::High => c_red,
        };
        let risk_label = |r: RiskLevel| match r {
            RiskLevel::Focal => "focal",
            RiskLevel::Safe => "safe",
            RiskLevel::Medium => "med risk",
            RiskLevel::High => "high risk",
        };

        let [bx, by, bw, bh] = center_bounds;

        // ── Backdrop dim + panel (70% of the editor area) ───────────────────
        quads.push(RegionDrawInstance::new(center_bounds, c_overlay));
        let pw = (bw * 0.80).min(bw - 8.0);
        let ph = (bh * 0.90).min(bh - 8.0);
        let px = bx + (bw - pw) * 0.5;
        let py = by + (bh - ph) * 0.5;
        quads.push(
            RegionDrawInstance::new([px - 1.0, py - 1.0, pw + 2.0, ph + 2.0], c_border)
                .with_radius(radius + 1.0),
        );
        quads.push(RegionDrawInstance::new([px, py, pw, ph], c_bg).with_radius(radius));

        let text = |this: &mut Self,
                    glyphs: &mut Vec<GlyphInstance>,
                    s: &str,
                    x: f32,
                    y: f32,
                    color: [f32; 4]| {
            glyphs.extend(layout_panel_text(
                s,
                &mut this.editor_overlay_text_system,
                &mut this.atlas,
                &this.queue,
                x,
                y,
                color,
            ));
        };

        // ── Top bar ─────────────────────────────────────────────────────────
        let pad = 16.0 * ui_s;
        let bar_h = lh + 16.0 * ui_s;
        quads.push(RegionDrawInstance::new(
            [px, py + bar_h - 1.0, pw, 1.0],
            c_border,
        ));
        let bar_text_y = py + (bar_h - lh) * 0.5;
        let focal = format!("◎ {}", hud.focal_symbol);
        text(self, glyphs, &focal, px + pad, bar_text_y, c_cyan);

        if let CodeGraphHudStatus::Ready(m) = &hud.status {
            let high = m.callers.iter().any(|n| n.risk == RiskLevel::High);
            let med = m.callers.iter().any(|n| n.risk == RiskLevel::Medium);
            let (lbl, col) = if high {
                ("blast radius: high", c_red)
            } else if med {
                ("blast radius: med", c_amber)
            } else {
                ("blast radius: low", c_green)
            };
            let nodes = 1 + m.callers.len() + m.callees.len();
            let edges = m.callers.len() + m.callees.len();
            let counts = format!("{nodes} nodes · {edges} edges");
            let gap = 20.0 * ui_s;
            let bx2 = px + pad + estimate_monospace_width(&focal, fs) + gap;
            text(self, glyphs, lbl, bx2, bar_text_y, col);
            let cx2 = bx2 + estimate_monospace_width(lbl, fs) + gap;
            text(self, glyphs, &counts, cx2, bar_text_y, c_dim);
        }
        let esc = "Esc  close";
        text(
            self,
            glyphs,
            esc,
            px + pw - pad - estimate_monospace_width(esc, fs),
            bar_text_y,
            c_ghost,
        );

        // ── Footer (shortcut guide rendered as keycaps) ─────────────────────
        let foot_h = lh + 16.0 * ui_s;
        let foot_y = py + ph - foot_h;
        quads.push(RegionDrawInstance::new([px, foot_y, pw, 1.0], c_border));
        {
            let groups: [(&[&str], &str); 3] = [
                (&["h", "j", "k", "l"], "navigate"),
                (&["Enter"], "jump"),
                (&["Esc"], "close"),
            ];
            let label_y = foot_y + (foot_h - lh) * 0.5;
            let mut fx = px + pad;
            for (keys, label) in groups {
                let kc = layout_help_keycaps(
                    keys,
                    &mut self.editor_overlay_text_system,
                    &mut self.atlas,
                    &self.queue,
                    quads,
                    fx,
                    foot_y,
                    fs,
                    foot_h,
                    c_fg,
                    c_dim,
                    c_accent,
                    c_info,
                    c_amber,
                    c_red,
                    c_bg,
                );
                glyphs.extend(kc);
                fx += estimate_help_keycaps_width(keys, fs) + 8.0 * ui_s;
                text(self, glyphs, label, fx, label_y, c_dim);
                fx += estimate_monospace_width(label, fs) + 22.0 * ui_s;
            }
        }

        // ── Content area ────────────────────────────────────────────────────
        let content = [
            px + 16.0 * ui_s,
            py + bar_h + 16.0 * ui_s,
            pw - 32.0 * ui_s,
            ph - bar_h - foot_h - 32.0 * ui_s,
        ];

        match &hud.status {
            CodeGraphHudStatus::Loading => {
                let msg = "indexing…  (building / syncing code graph)";
                let x =
                    content[0] + (content[2] - estimate_monospace_width(msg, fs)).max(0.0) * 0.5;
                text(self, glyphs, msg, x, content[1] + content[3] * 0.5, c_dim);
            }
            CodeGraphHudStatus::NotInstalled => {
                let msg = "codegraph not installed — press <leader>m e to install";
                let x =
                    content[0] + (content[2] - estimate_monospace_width(msg, fs)).max(0.0) * 0.5;
                text(self, glyphs, msg, x, content[1] + content[3] * 0.5, c_amber);
            }
            CodeGraphHudStatus::Empty => {
                let msg = "no callers or callees found for this symbol";
                let x =
                    content[0] + (content[2] - estimate_monospace_width(msg, fs)).max(0.0) * 0.5;
                text(self, glyphs, msg, x, content[1] + content[3] * 0.5, c_dim);
            }
            CodeGraphHudStatus::Error(emsg) => {
                let msg = format!("error: {emsg}");
                let m = clamp_monospace_text(&msg, content[2], fs);
                let x = content[0] + (content[2] - estimate_monospace_width(&m, fs)).max(0.0) * 0.5;
                text(self, glyphs, &m, x, content[1] + content[3] * 0.5, c_red);
            }
            CodeGraphHudStatus::Ready(model) => {
                let caller_focus = match hud.focus {
                    Focus::Caller(i) => Some(i),
                    _ => None,
                };
                let callee_focus = match hud.focus {
                    Focus::Callee(i) => Some(i),
                    _ => None,
                };
                // Reserve a FIXED-height bottom strip for the code preview so
                // navigating between nodes never reflows the graph (no UI jump).
                let detail = hud.detail.as_ref();
                const PREVIEW_ROWS: f32 = 7.0;
                let detail_h = (PREVIEW_ROWS + 1.6) * lh + 18.0 * ui_s;
                let dgap = 14.0 * ui_s;
                let graph_content = [
                    content[0],
                    content[1],
                    content[2],
                    (content[3] - detail_h - dgap).max(80.0 * ui_s),
                ];
                let gl = layout(
                    graph_content,
                    model.callers.len(),
                    model.callees.len(),
                    caller_focus,
                    callee_focus,
                    ui_s,
                );
                let center = gl.center;

                // ── Detail panel (focused node code preview) ────────────────
                if let Some(d) = detail {
                    if detail_h > 0.0 {
                        let dx = content[0];
                        let dy = content[1] + graph_content[3] + dgap;
                        let dw = content[2];
                        quads.push(
                            RegionDrawInstance::new(
                                [dx, dy, dw, detail_h],
                                blend_rgba(c_bg, c_fg, 0.05, 1.0),
                            )
                            .with_radius(8.0 * ui_s),
                        );
                        quads.push(RegionDrawInstance::new([dx, dy, dw, 1.0], c_border));
                        let tpad = 12.0 * ui_s;
                        text(self, glyphs, &d.name, dx + tpad, dy + 6.0 * ui_s, c_fg);
                        let loc = clamp_monospace_text_left(
                            &format!("{}:{}", d.file_path, d.line),
                            dw * 0.55,
                            fs,
                        );
                        text(
                            self,
                            glyphs,
                            &loc,
                            dx + dw - tpad - estimate_monospace_width(&loc, fs),
                            dy + 6.0 * ui_s,
                            c_dim,
                        );
                        let gutter_w = 48.0 * ui_s;
                        let code_x = dx + tpad + gutter_w;
                        let code_w = dw - gutter_w - tpad * 2.0;
                        let mut sy = dy + 6.0 * ui_s + lh * 1.5;
                        for (idx, code) in d.lines.iter().enumerate() {
                            let ln = d.start_line + idx as u32;
                            let is_target = ln == d.line;
                            if is_target {
                                // Highlight the line that holds the symbol definition.
                                quads.push(
                                    RegionDrawInstance::new(
                                        [dx + 4.0 * ui_s, sy, dw - 8.0 * ui_s, lh],
                                        with_alpha(c_cyan, 0.16),
                                    )
                                    .with_radius(3.0 * ui_s),
                                );
                                quads.push(RegionDrawInstance::new(
                                    [dx + 4.0 * ui_s, sy, 2.5 * ui_s, lh],
                                    c_cyan,
                                ));
                            }
                            let ln_color = if is_target { c_cyan } else { c_ghost };
                            text(self, glyphs, &format!("{ln:>4}"), dx + tpad, sy, ln_color);
                            // Slice the syntax-highlight spans down to this line.
                            let line_start: usize =
                                d.lines.iter().take(idx).map(|t| t.len() + 1).sum();
                            let line_end = line_start + code.len();
                            let line_spans: Vec<StyledTextSpan> = d
                                .spans
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
                            let shown = clamp_monospace_text(code, code_w, fs);
                            glyphs.extend(layout_panel_rich_text(
                                &shown,
                                &line_spans,
                                if is_target { c_fg } else { c_dim },
                                &mut self.editor_overlay_text_system,
                                &mut self.atlas,
                                &self.queue,
                                code_x,
                                sy,
                            ));
                            sy += lh;
                        }
                    }
                }

                // Column headers.
                text(
                    self,
                    glyphs,
                    "CALLERS",
                    content[0],
                    content[1] - lh,
                    c_ghost,
                );
                let chdr = "CALLEES";
                text(
                    self,
                    glyphs,
                    chdr,
                    content[0] + content[2] - estimate_monospace_width(chdr, fs),
                    content[1] - lh,
                    c_ghost,
                );

                // Edges (under pills).
                for (slot, pill) in gl.callers.iter().enumerate() {
                    let n = &model.callers[gl.caller_window_start + slot];
                    let focused = hud.focus == Focus::Caller(gl.caller_window_start + slot);
                    let col = with_alpha(risk_color(n.risk), if focused { 0.9 } else { 0.22 });
                    for seg in elbow(center, *pill, false, ui_s) {
                        quads.push(RegionDrawInstance::new([seg.x, seg.y, seg.w, seg.h], col));
                    }
                }
                for (slot, pill) in gl.callees.iter().enumerate() {
                    let n = &model.callees[gl.callee_window_start + slot];
                    let focused = hud.focus == Focus::Callee(gl.callee_window_start + slot);
                    let col = with_alpha(risk_color(n.risk), if focused { 0.9 } else { 0.22 });
                    for seg in elbow(center, *pill, true, ui_s) {
                        quads.push(RegionDrawInstance::new([seg.x, seg.y, seg.w, seg.h], col));
                    }
                }

                // Pill renderer — solid tinted bg + left accent stripe + 3 lines.
                let mut draw_pill = |this: &mut Self,
                                     glyphs: &mut Vec<GlyphInstance>,
                                     pill: PillRect,
                                     name: &str,
                                     kind: &str,
                                     file: &str,
                                     line: u32,
                                     risk: RiskLevel,
                                     focused: bool,
                                     is_center: bool| {
                    let col = risk_color(risk);
                    let pr = if is_center { 12.0 * ui_s } else { 8.0 * ui_s };
                    if focused {
                        let h = 5.0 * ui_s;
                        quads.push(
                            RegionDrawInstance::new(
                                [pill.x - h, pill.y - h, pill.w + h * 2.0, pill.h + h * 2.0],
                                with_alpha(col, 0.15),
                            )
                            .with_radius(pr + 4.0 * ui_s),
                        );
                    }
                    // Card look: a risk-colored border RING (outer rect) with a
                    // subtle tinted interior (inset rect) drawn on top — so each
                    // node reads as an outlined card like the design mockup.
                    let bt = if focused { 2.0 * ui_s } else { 1.0 * ui_s };
                    quads.push(
                        RegionDrawInstance::new(
                            [pill.x, pill.y, pill.w, pill.h],
                            with_alpha(col, if focused { 0.95 } else { 0.55 }),
                        )
                        .with_radius(pr),
                    );
                    quads.push(
                        RegionDrawInstance::new(
                            [
                                pill.x + bt,
                                pill.y + bt,
                                pill.w - bt * 2.0,
                                pill.h - bt * 2.0,
                            ],
                            blend_rgba(c_bg, col, if focused { 0.16 } else { 0.10 }, 1.0),
                        )
                        .with_radius((pr - bt).max(2.0)),
                    );

                    let tx = pill.x + 14.0 * ui_s;
                    let tw = pill.w - 22.0 * ui_s;
                    let mut ty = pill.y + 8.0 * ui_s;
                    // Line 1: name (bold, bright).
                    let nm = clamp_monospace_text(name, tw, fs);
                    glyphs.extend(layout_panel_text_bold(
                        &nm,
                        &mut this.editor_overlay_text_system,
                        &mut this.atlas,
                        &this.queue,
                        tx,
                        ty,
                        c_fg,
                    ));
                    ty += lh;
                    // Line 2: kind · risk (risk color) — or "at cursor" for center.
                    let sub = if is_center {
                        "‹ symbol at cursor ›".to_string()
                    } else {
                        format!("{kind} · {}", risk_label(risk))
                    };
                    let sub = clamp_monospace_text(&sub, tw, fs);
                    glyphs.extend(layout_panel_text(
                        &sub,
                        &mut this.editor_overlay_text_system,
                        &mut this.atlas,
                        &this.queue,
                        tx,
                        ty,
                        if is_center {
                            with_alpha(c_cyan, 0.7)
                        } else {
                            col
                        },
                    ));
                    ty += lh;
                    // Line 3: file:line (truncated from the LEFT to keep the tail).
                    let loc = clamp_monospace_text_left(&format!("{file}:{line}"), tw, fs);
                    glyphs.extend(layout_panel_text(
                        &loc,
                        &mut this.editor_overlay_text_system,
                        &mut this.atlas,
                        &this.queue,
                        tx,
                        ty,
                        c_dim,
                    ));
                };

                for (slot, pill) in gl.callers.iter().enumerate() {
                    let abs = gl.caller_window_start + slot;
                    let n = &model.callers[abs];
                    draw_pill(
                        self,
                        glyphs,
                        *pill,
                        &n.name,
                        &n.kind,
                        &n.file_path,
                        n.line,
                        n.risk,
                        hud.focus == Focus::Caller(abs),
                        false,
                    );
                }
                for (slot, pill) in gl.callees.iter().enumerate() {
                    let abs = gl.callee_window_start + slot;
                    let n = &model.callees[abs];
                    draw_pill(
                        self,
                        glyphs,
                        *pill,
                        &n.name,
                        &n.kind,
                        &n.file_path,
                        n.line,
                        n.risk,
                        hud.focus == Focus::Callee(abs),
                        false,
                    );
                }
                draw_pill(
                    self,
                    glyphs,
                    center,
                    &model.focal.name,
                    &model.focal.kind,
                    &model.focal.file_path,
                    model.focal.line,
                    RiskLevel::Focal,
                    hud.focus == Focus::Center,
                    true,
                );

                let overflow_y = content[1] + graph_content[3] - lh;
                if gl.caller_overflow > 0 {
                    let s = format!("+{} more callers", gl.caller_overflow);
                    text(self, glyphs, &s, content[0], overflow_y, c_ghost);
                }
                if gl.callee_overflow > 0 {
                    let s = format!("+{} more callees", gl.callee_overflow);
                    text(
                        self,
                        glyphs,
                        &s,
                        content[0] + content[2] - estimate_monospace_width(&s, fs),
                        overflow_y,
                        c_ghost,
                    );
                }
            }
        }

        // Restore the shared overlay text system's metrics.
        self.editor_overlay_text_system
            .set_metrics(Metrics::new(font_size, line_h));
    }

    pub fn add_matched_bracket_overlay(&mut self, app_state: &AppState, center_bounds: [f32; 4]) {
        let Some(char_idx) = app_state.matched_bracket_pos() else {
            return;
        };
        let Some(rect) = self.editor_char_rect(app_state, center_bounds, char_idx) else {
            return;
        };

        let mut color = self
            .theme
            .editor
            .bracket_ripple
            .unwrap_or(self.theme.ui.accent)
            .as_f32();
        color[3] = (color[3] * 0.22).clamp(0.0, 1.0);
        self.editor_overlay_chrome_instances
            .push(RegionDrawInstance::new(rect, color).with_radius(2.0 * self.ui_scale.max(0.5)));
    }

    pub fn add_bracket_ripple_overlay(&mut self, app_state: &AppState, center_bounds: [f32; 4]) {
        let Some(char_idx) = app_state.bracket_ripple_pos() else {
            return;
        };
        let alpha = app_state.bracket_ripple_alpha();
        if alpha <= 0.0 {
            return;
        }
        let Some(rect) = self.editor_char_rect(app_state, center_bounds, char_idx) else {
            return;
        };

        let ui_s = self.ui_scale.max(0.5);
        let progress = 1.0 - alpha;
        let expand = progress * 10.0 * ui_s;
        let ripple_rect = [
            rect[0] - expand,
            rect[1] - expand,
            rect[2] + expand * 2.0,
            rect[3] + expand * 2.0,
        ];
        let mut color = self
            .theme
            .editor
            .bracket_ripple
            .unwrap_or(self.theme.ui.accent)
            .as_f32();
        color[3] = (color[3] * alpha * 0.35).clamp(0.0, 1.0);

        self.editor_overlay_chrome_instances.push(
            RegionDrawInstance::new(ripple_rect, color).with_radius((3.0 + expand * 0.35) * ui_s),
        );
    }

    fn editor_char_rect(
        &self,
        app_state: &AppState,
        center_bounds: [f32; 4],
        char_idx: usize,
    ) -> Option<[f32; 4]> {
        if char_idx >= app_state.len_chars() {
            return None;
        }

        let (line_idx, start_byte_in_line) = app_state.char_idx_to_line_and_byte_in_line(char_idx);
        if app_state.is_line_folded(line_idx) {
            return None;
        }
        let (end_line_idx, end_byte_in_line) =
            app_state.char_idx_to_line_and_byte_in_line(char_idx + 1);
        if end_line_idx != line_idx {
            return None;
        }

        let geometry = super::editor_viewport_geometry(self, app_state, center_bounds);
        let viewport_top = geometry.viewport_text_top;
        let viewport_bottom = viewport_top + geometry.viewport_text_height.max(1.0);
        let viewport_left = geometry.viewport_text_left;
        let viewport_right = viewport_left + geometry.viewport_text_width.max(1.0);
        let scroll_y_px = crate::text::layout_sync::visual_y_for_logical_scroll_with_folds(
            &self.text_system,
            app_state.current_scroll_y,
            app_state.folded_ranges(),
        );
        let origin_y = geometry.viewport_text_top + geometry.line_height - scroll_y_px;

        for run in self.text_system.buffer().layout_runs() {
            if run.line_i != line_idx {
                continue;
            }
            let line_top = origin_y + run.line_top
                - app_state.folded_visual_y_offset_before(run.line_i, run.line_height);
            let line_height_px = run.line_height.max(1.0);
            let line_bottom = line_top + line_height_px;
            if line_bottom <= viewport_top || line_top >= viewport_bottom {
                return None;
            }

            let start_x = super::run_x_for_byte(viewport_left, &run, start_byte_in_line);
            let end_x = super::run_x_for_byte(viewport_left, &run, end_byte_in_line);
            let left = start_x.min(end_x).max(viewport_left);
            let right = start_x.max(end_x).min(viewport_right);
            let width = (right - left).max((geometry.font_size * 0.52).max(1.0));
            return Some([left, line_top, width, line_height_px]);
        }
        None
    }

    pub fn add_yank_flash_overlay(&mut self, app_state: &AppState, center_bounds: [f32; 4]) {
        let Some((start_char, end_char)) = app_state.yank_flash_range() else {
            return;
        };

        let alpha = app_state.yank_flash_alpha();
        if alpha <= 0.0 {
            return;
        }

        let (mut start_line, mut start_byte_in_line) =
            app_state.char_idx_to_line_and_byte_in_line(start_char);
        let (mut end_line, mut end_byte_in_line) =
            app_state.char_idx_to_line_and_byte_in_line(end_char);
        if start_line > end_line
            || (start_line == end_line && start_byte_in_line > end_byte_in_line)
        {
            std::mem::swap(&mut start_line, &mut end_line);
            std::mem::swap(&mut start_byte_in_line, &mut end_byte_in_line);
        }

        // Adjust if it ends at the start of a new line to avoid highlighting the first character of the next line.
        if end_line > start_line && end_byte_in_line == 0 {
            end_line -= 1;
            end_byte_in_line = app_state
                .line_end_byte_idx(end_line)
                .saturating_sub(app_state.line_start_byte_idx(end_line));
        }

        let geometry = super::editor_viewport_geometry(self, app_state, center_bounds);
        let line_height = geometry.line_height;
        let text_area_x = geometry.viewport_text_left;
        let text_area_w = geometry.viewport_text_width;
        let viewport_top = geometry.viewport_text_top;
        let viewport_bottom = viewport_top + geometry.viewport_text_height.max(1.0);

        let mut color = self
            .theme
            .editor
            .yank_flash
            .unwrap_or(self.theme.ui.accent)
            .as_f32();
        color[3] = (color[3] * alpha * 0.25).clamp(0.0, 1.0);

        let scroll_y_px = crate::text::layout_sync::visual_y_for_logical_scroll_with_folds(
            &self.text_system,
            app_state.current_scroll_y,
            app_state.folded_ranges(),
        );
        let origin_y = geometry.viewport_text_top + line_height - scroll_y_px;

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

            let line_start_x = text_area_x;
            let line_end_x = (text_area_x + run.line_w).max(line_start_x + 1.0);
            let start_x = if run.line_i == start_line {
                super::run_x_for_byte(text_area_x, &run, start_byte_in_line)
            } else {
                line_start_x
            };
            let end_x = if run.line_i == end_line {
                super::run_x_for_byte(text_area_x, &run, end_byte_in_line)
            } else {
                line_end_x
            };

            let left = start_x.min(end_x).max(text_area_x);
            let right = start_x.max(end_x).min(text_area_x + text_area_w);
            let width = (right - left).max(1.0);

            self.editor_overlay_chrome_instances
                .push(RegionDrawInstance::new(
                    [left, line_top, width, line_height_px],
                    color,
                ));
        }
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
/// Retained (with tests) for reuse; the completion doc panel that used it was
/// removed in favor of the shared single-column menu.
#[allow(dead_code)]
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

    #[test]
    fn diagnostic_counts_split_errors_and_warnings() {
        use crate::async_runtime::message::{LspDiagnostic, LspPosition, LspRange};
        let diag = |severity| LspDiagnostic {
            range: LspRange {
                start: LspPosition {
                    line: 0,
                    character: 0,
                },
                end: LspPosition {
                    line: 0,
                    character: 1,
                },
            },
            severity,
            code: None,
            source: None,
            message: String::new(),
            tags: Vec::new(),
        };
        // Severity None counts as warning (same unwrap_or rule as the overlay
        // highlights); info/hint (3/4) are excluded from the breadcrumb chips.
        let diags = [
            diag(Some(1)),
            diag(Some(2)),
            diag(Some(2)),
            diag(None),
            diag(Some(3)),
        ];
        assert_eq!(super::diagnostic_counts(&diags), (1, 3));
    }
}
