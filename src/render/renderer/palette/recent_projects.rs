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
    pub(super) fn render_recent_projects(&mut self, model: &CommandPaletteRenderModel) {
        let [panel_x, panel_y, panel_w, panel_h] = model.panel_bounds;
        self.palette_scissor = rect_to_scissor(model.panel_bounds);

        let inner_width = (panel_w - model.panel_padding * 2.0).max(1.0);
        self.palette_text_system
            .set_size(Some(inner_width), Some(model.line_height));

        let mut quads: Vec<RegionDrawInstance> = Vec::new();
        let mut glyphs: Vec<GlyphInstance> = Vec::new();

        let font_size = self.theme.ui.sidebar_font_size;
        let char_w = font_size * 0.62;
        let line_h = model.line_height.max(18.0);
        let row_v_pad = 4.0;
        let row_h = line_h + row_v_pad * 2.0;
        let text_x = panel_x + model.panel_padding + 8.0;

        // name column = 38% of inner_width, path column = rest
        let name_col_w = (inner_width * 0.38).max(120.0);
        let path_x = text_x + name_col_w + char_w * 2.0;

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

        // ── Header ─────────────────────────────────────────────────────────────
        let mut row_top = panel_y + model.panel_padding;

        let badge_text = format!(" {} ", model.title);
        let badge_w = badge_text.chars().count() as f32 * font_size * 0.60 + 4.0;
        let badge_color =
            if model.mode == crate::app::command_palette::CommandPaletteMode::ThemeSelector {
                model.success_color
            } else {
                model.info_color
            };
        quads.push(RegionDrawInstance::new(
            [text_x, row_top + 2.0, badge_w, line_h - 4.0],
            badge_color,
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

        let count_text = format!("{}", model.result_labels.len());
        let count_w = count_text.chars().count() as f32 * font_size * 0.60;
        let count_x =
            (panel_x + panel_w - model.panel_padding - count_w).max(text_x + badge_w + 8.0);
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

        // ── Column header labels ────────────────────────────────────────────────
        let mut col_header_color = model.hint_color;
        col_header_color[3] *= 0.7;
        let (name_header, path_header, empty_text, footer_text) =
            if model.mode == crate::app::command_palette::CommandPaletteMode::ThemeSelector {
                (
                    "PROFILE",
                    "SOURCE",
                    "  (no themes found in repo/user theme folders)",
                    "  ↑↓ navigate   ↵ apply theme   esc close",
                )
            } else {
                (
                    "NAME",
                    "PATH",
                    "  (no recent projects — open a folder with Cmd+O)",
                    "  ↑↓ navigate   ↵ open   esc close",
                )
            };
        glyphs.extend(layout_panel_text(
            name_header,
            &mut self.palette_text_system,
            &mut self.atlas,
            &self.queue,
            text_x,
            row_top,
            col_header_color,
        ));
        glyphs.extend(layout_panel_text(
            path_header,
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

        // ── Body rows ──────────────────────────────────────────────────────────
        let footer_h = line_h + 10.0 + 1.0;
        let body_h = (panel_h - (row_top - panel_y) - footer_h - model.panel_padding).max(row_h);
        let max_visible = (body_h / row_h).floor() as usize;
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

            let label_y = row_top + row_v_pad;

            // Col 1: folder name (highlighted if matched)
            let name_color = if absolute_idx == model.selected_index {
                model.text_color
            } else {
                model.text_color
            };
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
            let _ = name_color; // used implicitly via render_highlighted_label

            // Col 2: full path (always hint_color/dim)
            let available_w = (panel_x + panel_w - model.panel_padding - 4.0 - path_x).max(0.0);
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
        glyphs.extend(layout_panel_text(
            footer_text,
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
}
