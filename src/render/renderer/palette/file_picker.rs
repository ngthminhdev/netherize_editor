#![allow(unused_imports)]

use crate::{
    app::{app_state::AppState, command_palette::CommandPaletteRenderModel, input::LeapTarget},
    render::{
        glyph_instance::GlyphInstance, region_pipeline::RegionDrawInstance, renderer::Renderer,
    },
};

use super::super::helpers::{
    clamp_monospace_text, estimate_monospace_width, ext_icon_dot, gutter_width_for_editor,
    layout_panel_text, layout_panel_text_bold, rect_to_scissor,
};

impl Renderer {
    pub(super) fn render_file_picker_complex(&mut self, model: &CommandPaletteRenderModel) {
        let [panel_x, panel_y, panel_w, panel_h] = model.panel_bounds;
        self.palette_scissor = rect_to_scissor(model.panel_bounds);

        let inner_width = (panel_w - model.panel_padding * 2.0).max(1.0);
        self.palette_text_system
            .set_size(Some(inner_width), Some(model.line_height));

        let mut quads: Vec<RegionDrawInstance> = Vec::new();
        let mut glyphs: Vec<GlyphInstance> = Vec::new();

        let font_size = self.theme.ui.sidebar_font_size;
        let line_h = model.line_height.max(18.0);
        let row_v_pad = if model.mode == crate::app::command_palette::CommandPaletteMode::DocumentSymbols {
            8.0
        } else {
            4.0
        };
        let row_h = if model.mode == crate::app::command_palette::CommandPaletteMode::DocumentSymbols {
            (line_h + row_v_pad * 2.0).max(34.0)
        } else {
            line_h + row_v_pad * 2.0
        };
        let text_x = panel_x + model.panel_padding + 8.0;

        // Icon column: sized to always fit ● + longest ext ("toml", 4 chars)
        let char_w = font_size * 0.62;
        let dot_char_w = char_w + 2.0;
        let icon_col_w = dot_char_w + 4.0 + 4.0 * char_w + 24.0;
        let name_x = text_x + icon_col_w;

        // Chrome
        quads.push(RegionDrawInstance::new(
            model.overlay_bounds,
            model.scrim_color,
        ));
        quads.push(RegionDrawInstance::new(
            [panel_x - 1.0, panel_y - 1.0, panel_w + 2.0, panel_h + 2.0],
            model.border_color,
        ));
        quads.push(RegionDrawInstance::new(model.panel_bounds, model.panel_bg));

        // ── HEADER ─────────────────────────────────────────────────────────────
        let mut row_top = panel_y + model.panel_padding;

        let badge_text = format!(" {} ", model.title);
        let badge_w = badge_text.chars().count() as f32 * font_size * 0.60 + 4.0;
        quads.push(RegionDrawInstance::new(
            [text_x, row_top + 2.0, badge_w, line_h - 4.0],
            model.success_color,
        ));
        glyphs.extend(layout_panel_text(
            &badge_text,
            &mut self.palette_text_system,
            &mut self.atlas,
            &self.queue,
            text_x + 2.0,
            row_top,
            self.theme.ui.bg.as_f32(),
        ));

        let query_x = text_x + badge_w + 10.0;
        let query_col = if model.result_match_ranges.iter().any(|r| !r.is_empty()) {
            model.text_color
        } else {
            model.hint_color
        };
        let prefix_w = model.prompt_prefix.chars().count() as f32 * font_size * 0.60;
        glyphs.extend(layout_panel_text(
            &model.prompt_prefix,
            &mut self.palette_text_system,
            &mut self.atlas,
            &self.queue,
            query_x,
            row_top,
            model.hint_color,
        ));
        glyphs.extend(layout_panel_text(
            &model.prompt_query,
            &mut self.palette_text_system,
            &mut self.atlas,
            &self.queue,
            query_x + prefix_w,
            row_top,
            query_col,
        ));

        let count_text = format!("{}/{}", model.result_labels.len(), model.total_results);
        let count_w = count_text.chars().count() as f32 * font_size * 0.60;
        let count_x = (panel_x + panel_w - model.panel_padding - count_w).max(query_x);
        glyphs.extend(layout_panel_text(
            &count_text,
            &mut self.palette_text_system,
            &mut self.atlas,
            &self.queue,
            count_x,
            row_top,
            model.hint_color,
        ));
        row_top += line_h;

        quads.push(RegionDrawInstance::new(
            [panel_x, row_top, panel_w, 1.0],
            model.border_color,
        ));
        row_top += 6.0;

        // ── BODY ───────────────────────────────────────────────────────────────
        let footer_h = line_h + 10.0 + 1.0;
        let body_h = (panel_h - (row_top - panel_y) - footer_h - model.panel_padding).max(row_h);
        let max_visible = (body_h / row_h).floor() as usize;
        let scroll_offset = model
            .scroll_offset_rows
            .min(model.result_labels.len().saturating_sub(max_visible));

        for (visible_idx, (label, ranges)) in model
            .result_labels
            .iter()
            .zip(model.result_match_ranges.iter())
            .skip(scroll_offset)
            .take(max_visible)
            .enumerate()
        {
            let absolute_idx = scroll_offset + visible_idx;

            if absolute_idx == model.selected_index {
                quads.push(RegionDrawInstance::new(
                    [panel_x + 2.0, row_top, (panel_w - 4.0).max(0.0), row_h],
                    model.selection_bg,
                ));
            }
            if visible_idx > 0 {
                let mut sep = model.border_color;
                sep[3] *= 0.30;
                quads.push(RegionDrawInstance::new(
                    [text_x, row_top, inner_width - 8.0, 1.0],
                    sep,
                ));
            }

            let icon_y = row_top + row_v_pad;
            self.palette_text_system
                .set_size(Some(inner_width), Some(line_h));
            let tone = model
                .item_tones
                .get(absolute_idx)
                .copied()
                .unwrap_or_default();
            let mut row_model = model.clone();
            row_model.label_color = file_picker_tone_color(tone, model);

            let (label_text, label_ranges, label_x, label_y) = if model.mode
                == crate::app::command_palette::CommandPaletteMode::DocumentSymbols
            {
                let (badge, stripped_label) = split_symbol_badge(label);
                let badge_color = file_picker_tone_color(tone, model);
                let badge_size = (row_h * 0.58).clamp(20.0, 28.0);
                let badge_x = text_x;
                let badge_y = row_top + (row_h - badge_size) * 0.5;
                quads.push(
                    RegionDrawInstance::new(
                        [badge_x, badge_y, badge_size, badge_size],
                        [badge_color[0], badge_color[1], badge_color[2], 0.90],
                    )
                    .with_radius(badge_size * 0.22),
                );
                quads.push(
                    RegionDrawInstance::new(
                        [badge_x + 0.5, badge_y + 0.5, badge_size - 1.0, badge_size - 1.0],
                        [badge_color[0], badge_color[1], badge_color[2], 1.0],
                    )
                    .with_radius((badge_size * 0.22 - 0.5).max(0.5)),
                );

                let icon_size = if badge.chars().count() > 1 {
                    (badge_size * 0.42).max(8.0)
                } else {
                    (badge_size * 0.62).max(10.0)
                };
                let icon_w = estimate_monospace_width(&badge, icon_size);
                self.palette_text_system
                    .set_metrics(cosmic_text::Metrics::new(icon_size, badge_size));
                self.palette_text_system
                    .set_size(Some(badge_size), Some(badge_size));
                glyphs.extend(layout_panel_text(
                    &badge,
                    &mut self.palette_text_system,
                    &mut self.atlas,
                    &self.queue,
                    badge_x + (badge_size - icon_w) * 0.5,
                    badge_y,
                    self.theme.ui.bg.as_f32(),
                ));
                self.palette_text_system
                    .set_metrics(cosmic_text::Metrics::new(font_size, line_h));
                self.palette_text_system
                    .set_size(Some((inner_width - badge_size - 12.0).max(1.0)), Some(line_h));
                (
                    stripped_label,
                    shift_ranges_after_badge(label, ranges),
                    text_x + badge_size + 12.0,
                    row_top + (row_h - line_h) * 0.5,
                )
            } else {
                let file_path = label.split(':').next().unwrap_or(label);
                let ext = std::path::Path::new(file_path)
                    .extension()
                    .and_then(|e| e.to_str())
                    .unwrap_or("");
                let (dot_color, ext_label) = ext_icon_dot(ext, &self.theme);
                let picker_dot = self.theme.icons.file_picker_dot.as_str();

                glyphs.extend(layout_panel_text(
                    picker_dot,
                    &mut self.palette_text_system,
                    &mut self.atlas,
                    &self.queue,
                    text_x,
                    icon_y,
                    dot_color,
                ));
                let mut ext_color = dot_color;
                ext_color[3] *= 0.75;
                glyphs.extend(layout_panel_text(
                    ext_label,
                    &mut self.palette_text_system,
                    &mut self.atlas,
                    &self.queue,
                    text_x + dot_char_w + 4.0,
                    icon_y,
                    ext_color,
                ));
                (label.clone(), ranges.clone(), name_x, icon_y)
            };

            Self::render_highlighted_label(
                &label_text,
                &label_ranges,
                label_x,
                label_y,
                font_size,
                &row_model,
                &mut self.palette_text_system,
                &mut self.atlas,
                &self.queue,
                &mut glyphs,
            );
            row_top += row_h;
        }

        if model.result_labels.is_empty() {
            glyphs.extend(layout_panel_text(
                "  (no results — type to search)",
                &mut self.palette_text_system,
                &mut self.atlas,
                &self.queue,
                text_x,
                row_top + row_v_pad,
                model.hint_color,
            ));
        }

        // ── FOOTER ─────────────────────────────────────────────────────────────
        let footer_y = panel_y + panel_h - footer_h;
        quads.push(RegionDrawInstance::new(
            [panel_x, footer_y, panel_w, 1.0],
            model.border_color,
        ));
        glyphs.extend(layout_panel_text(
            "  ↑↓ navigate   ↵ open   esc close",
            &mut self.palette_text_system,
            &mut self.atlas,
            &self.queue,
            text_x,
            footer_y + 5.0,
            model.hint_color,
        ));

        self.palette_chrome_instances = quads;
        self.palette_glyph_instances = glyphs;
    }

    // ── Name/path picker (Recent Projects + Theme Selector) ───────────────────
}

fn split_symbol_badge(label: &str) -> (String, String) {
    let Some(rest) = label.strip_prefix('[') else {
        return ("·".to_string(), label.to_string());
    };
    let Some(end) = rest.find(']') else {
        return ("·".to_string(), label.to_string());
    };

    let badge = rest[..end].to_string();
    let stripped = rest[end + 1..].trim_start().to_string();
    (badge, stripped)
}

fn shift_ranges_after_badge(label: &str, ranges: &[(usize, usize)]) -> Vec<(usize, usize)> {
    let offset = label
        .strip_prefix('[')
        .and_then(|rest| rest.find(']').map(|end| end + 2))
        .unwrap_or(0);

    ranges
        .iter()
        .filter_map(|(start, end)| {
            if *end <= offset {
                None
            } else {
                Some((start.saturating_sub(offset), end.saturating_sub(offset)))
            }
        })
        .collect()
}

fn file_picker_tone_color(
    tone: crate::app::command_palette::CommandPaletteItemTone,
    model: &CommandPaletteRenderModel,
) -> [f32; 4] {
    match tone {
        crate::app::command_palette::CommandPaletteItemTone::Function => model.info_color,
        crate::app::command_palette::CommandPaletteItemTone::Type => model.warning_color,
        crate::app::command_palette::CommandPaletteItemTone::Variable => model.success_color,
        _ => model.label_color,
    }
}
