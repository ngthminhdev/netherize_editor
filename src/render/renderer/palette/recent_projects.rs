#![allow(unused_imports)]

use crate::{
    app::{app_state::AppState, command_palette::CommandPaletteRenderModel, input::LeapTarget},
    render::{
        glyph_instance::GlyphInstance, region_pipeline::RegionDrawInstance, renderer::Renderer,
    },
};

use super::{
    palette_footer_content_height, palette_footer_height, render_palette_badge,
    render_palette_chrome, render_palette_footer, render_palette_selection, PaletteFooterAction,
    PALETTE_FOOTER_TOP_PAD, PALETTE_HEADER_BOTTOM_PAD,
};
use super::super::helpers::{
    clamp_monospace_text, estimate_monospace_width, ext_icon_dot, gutter_width_for_editor,
    layout_panel_text, layout_panel_text_bold, rect_to_scissor,
};

impl Renderer {
    pub(super) fn render_recent_projects(&mut self, model: &CommandPaletteRenderModel) {
        let [panel_x, panel_y, panel_w, panel_h] = model.panel_bounds;
        self.palette_scissor = rect_to_scissor(model.panel_bounds);

        let inner_width = (panel_w - model.panel_padding * 2.0).max(1.0);
        self.palette_text_system
            .set_size(Some(inner_width), Some(model.line_height));

        let mut quads: Vec<RegionDrawInstance> = Vec::new();
        let mut glyphs: Vec<GlyphInstance> = Vec::new();

        let is_theme_selector =
            model.mode == crate::app::command_palette::CommandPaletteMode::ThemeSelector;
        let font_size = if is_theme_selector {
            self.theme.ui.sidebar_font_size * 1.18
        } else {
            self.theme.ui.sidebar_font_size
        };
        let char_w = font_size * 0.62;
        let line_h = if is_theme_selector {
            (model.line_height * 1.18).max(24.0)
        } else {
            model.line_height.max(18.0)
        };
        let row_v_pad = if is_theme_selector { 8.0 } else { 4.0 };
        let row_h = line_h + row_v_pad * 2.0;
        let text_x = panel_x + model.panel_padding + if is_theme_selector { 16.0 } else { 8.0 };

        // Recent Projects keeps a 2-column table. Theme Selector becomes a richer
        // browser row: palette preview + theme name + source path + active marker.
        let name_col_w = if is_theme_selector {
            (inner_width * 0.68).max(260.0)
        } else {
            (inner_width * 0.38).max(120.0)
        };
        let path_x = text_x + name_col_w + char_w * 2.0;

        // Chrome
        render_palette_chrome(model, &mut quads);

        // ── Header ─────────────────────────────────────────────────────────────
        let mut row_top = panel_y + model.panel_padding;

        let badge_color = if is_theme_selector {
            model.match_color
        } else {
            model.info_color
        };
        let (badge_w, badge_h, badge_glyphs) = render_palette_badge(
            &model.title,
            &mut self.palette_text_system,
            &mut self.atlas,
            &self.queue,
            &mut quads,
            text_x,
            row_top,
            font_size,
            line_h,
            badge_color,
            self.theme.ui.bg.as_f32(),
        );
        glyphs.extend(badge_glyphs);
        let header_text_y = row_top + ((badge_h - line_h) * 0.5).max(0.0);

        let count_text = if is_theme_selector && !model.result_labels.is_empty() {
            format!(
                "{}/{}",
                model.selected_index.saturating_add(1).min(model.result_labels.len()),
                model.total_results
            )
        } else {
            format!("{}", model.result_labels.len())
        };
        let count_w = count_text.chars().count() as f32 * font_size * 0.60;
        let count_x =
            (panel_x + panel_w - model.panel_padding - count_w).max(text_x + badge_w + 8.0);
        glyphs.extend(layout_panel_text(
            &count_text,
            &mut self.palette_text_system,
            &mut self.atlas,
            &self.queue,
            count_x,
            header_text_y,
            model.hint_color,
        ));
        row_top += badge_h + PALETTE_HEADER_BOTTOM_PAD;

        quads.push(RegionDrawInstance::new(
            [panel_x, row_top, panel_w, 1.0],
            model.border_color,
        ));
        row_top += 6.0;

        // ── Column header labels ────────────────────────────────────────────────
        let empty_text = if is_theme_selector {
            "  (no themes found in repo/user theme folders)"
        } else {
            let mut col_header_color = model.hint_color;
            col_header_color[3] *= 0.7;
            glyphs.extend(layout_panel_text(
                "NAME",
                &mut self.palette_text_system,
                &mut self.atlas,
                &self.queue,
                text_x,
                row_top,
                col_header_color,
            ));
            glyphs.extend(layout_panel_text(
                "PATH",
                &mut self.palette_text_system,
                &mut self.atlas,
                &self.queue,
                path_x,
                row_top,
                col_header_color,
            ));
            row_top += line_h;

            quads.push(RegionDrawInstance::new(
                [panel_x, row_top, panel_w, 1.0],
                model.border_color,
            ));
            row_top += 4.0;

            "  (no recent projects — open a folder with Cmd+O)"
        };

        // ── Body rows ──────────────────────────────────────────────────────────
        let footer_h = palette_footer_height(line_h);
        let row_h = if is_theme_selector { line_h + 18.0 } else { row_h };
        let body_h = (panel_h - (row_top - panel_y) - footer_h - model.panel_padding).max(0.0);
        let max_visible = ((body_h / row_h).floor() as usize).max(1);
        if is_theme_selector && body_h > 1.0 {
            let mut frost = model.panel_bg;
            frost[3] = frost[3].max(0.74);
            quads.push(
                RegionDrawInstance::new(
                    [
                        panel_x + model.panel_padding * 0.5,
                        row_top - 2.0,
                        panel_w - model.panel_padding,
                        body_h + 4.0,
                    ],
                    frost,
                )
                .with_radius(12.0),
            );
            let mut veil = self.theme.ui.bg.as_f32();
            veil[3] = 0.28;
            quads.push(
                RegionDrawInstance::new(
                    [
                        panel_x + model.panel_padding * 0.5,
                        row_top - 2.0,
                        panel_w - model.panel_padding,
                        body_h + 4.0,
                    ],
                    veil,
                )
                .with_radius(12.0),
            );
        }
        let scroll_offset = model
            .scroll_offset_rows
            .min(model.result_labels.len().saturating_sub(max_visible));

        let empty_str = String::new();
        for (visible_idx, (name, ranges)) in model
            .result_labels
            .iter()
            .zip(model.result_match_ranges.iter())
            .skip(scroll_offset)
            .take(max_visible)
            .enumerate()
        {
            let absolute_idx = scroll_offset + visible_idx;
            let full_path = model
                .secondary_labels
                .get(absolute_idx)
                .unwrap_or(&empty_str);

            let is_selected = absolute_idx == model.selected_index;
            if is_selected {
                render_palette_selection(model, &mut quads, row_top, row_h);
            }
            if visible_idx > 0 {
                let mut sep = model.border_color;
                sep[3] *= if is_theme_selector { 0.18 } else { 0.30 };
                quads.push(RegionDrawInstance::new(
                    [text_x, row_top, inner_width - 8.0, 1.0],
                    sep,
                ));
            }

            let label_y = row_top + row_v_pad;

            if is_theme_selector {
                let palette_x = text_x;
                let palette_y = row_top + (row_h - 24.0) * 0.5;
                let fallback_swatches = theme_preview_colors(name, model);
                let swatches = model
                    .item_preview_colors
                    .get(absolute_idx)
                    .filter(|colors| !colors.is_empty())
                    .map(Vec::as_slice)
                    .unwrap_or(&fallback_swatches);
                for (swatch_idx, color) in swatches.iter().take(6).enumerate() {
                    quads.push(
                        RegionDrawInstance::new(
                            [palette_x + swatch_idx as f32 * 34.0, palette_y, 24.0, 24.0],
                            *color,
                        )
                        .with_radius(12.0),
                    );
                }

                let name_x = text_x + 224.0;
                Self::render_highlighted_label(
                    name,
                    ranges,
                    name_x,
                    label_y,
                    font_size,
                    model,
                    &mut self.palette_text_system,
                    &mut self.atlas,
                    &self.queue,
                    &mut glyphs,
                );

                let marker_w = 34.0;
                let marker_h = 16.0;
                let marker_x = panel_x + panel_w - model.panel_padding - marker_w - 8.0;
                let marker_y = row_top + (row_h - marker_h) * 0.5;
                let mut marker = if is_selected {
                    model.match_color
                } else {
                    model.border_color
                };
                marker[3] = if is_selected { 0.88 } else { 0.35 };
                quads.push(
                    RegionDrawInstance::new([marker_x, marker_y, marker_w, marker_h], marker)
                        .with_radius(4.0),
                );
            } else {
                Self::render_highlighted_label(
                    name,
                    ranges,
                    text_x,
                    label_y,
                    font_size,
                    model,
                    &mut self.palette_text_system,
                    &mut self.atlas,
                    &self.queue,
                    &mut glyphs,
                );

                let available_w =
                    (panel_x + panel_w - model.panel_padding - 4.0 - path_x).max(0.0);
                let clamped_path = clamp_monospace_text(full_path, available_w, font_size);
                glyphs.extend(layout_panel_text(
                    &clamped_path,
                    &mut self.palette_text_system,
                    &mut self.atlas,
                    &self.queue,
                    path_x,
                    label_y,
                    model.hint_color,
                ));
            }

            row_top += row_h;
        }

        if model.result_labels.is_empty() {
            glyphs.extend(layout_panel_text(
                empty_text,
                &mut self.palette_text_system,
                &mut self.atlas,
                &self.queue,
                text_x,
                row_top + row_v_pad,
                model.hint_color,
            ));
        }

        // ── Footer ─────────────────────────────────────────────────────────────
        let footer_y = panel_y + panel_h - footer_h;
        quads.push(RegionDrawInstance::new(
            [panel_x, footer_y, panel_w, 1.0],
            model.border_color,
        ));
        let enter_label = if is_theme_selector { "apply theme" } else { "open" };
        glyphs.extend(render_palette_footer(
            model,
            &mut self.palette_text_system,
            &mut self.atlas,
            &self.queue,
            &mut quads,
            text_x,
            footer_y + PALETTE_FOOTER_TOP_PAD,
            font_size,
            palette_footer_content_height(footer_h),
            &[
                PaletteFooterAction {
                    keys: &["↑↓"],
                    label: "navigate",
                },
                PaletteFooterAction {
                    keys: &["↵"],
                    label: enter_label,
                },
                PaletteFooterAction {
                    keys: &["󱊷"],
                    label: "close",
                },
            ],
        ));

        self.palette_chrome_instances = quads;
        self.palette_glyph_instances = glyphs;
    }
}

fn theme_preview_colors(name: &str, model: &CommandPaletteRenderModel) -> [[f32; 4]; 6] {
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    for byte in name.bytes() {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }

    let mut colors = [
        model.match_color,
        model.info_color,
        model.success_color,
        model.warning_color,
        model.text_color,
        model.label_color,
    ];

    for (idx, color) in colors.iter_mut().enumerate() {
        let shift = (idx * 9) as u32;
        let hue = ((hash >> shift) & 0xff) as f32 / 255.0;
        color[0] = (color[0] * 0.70 + hue * 0.30).clamp(0.0, 1.0);
        color[1] = (color[1] * 0.72 + (1.0 - hue) * 0.20).clamp(0.0, 1.0);
        color[2] = (color[2] * 0.74 + ((hue * 1.7) % 1.0) * 0.22).clamp(0.0, 1.0);
        color[3] = 1.0;
    }

    colors
}
