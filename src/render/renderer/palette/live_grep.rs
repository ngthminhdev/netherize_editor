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
    pub(super) fn render_live_grep_picker(&mut self, model: &CommandPaletteRenderModel) {
        let [panel_x, panel_y, panel_w, panel_h] = model.panel_bounds;
        self.palette_scissor = rect_to_scissor(model.panel_bounds);

        let inner_width = (panel_w - model.panel_padding * 2.0).max(1.0);
        self.palette_text_system
            .set_size(Some(inner_width), Some(model.line_height));

        let mut quads: Vec<RegionDrawInstance> = Vec::new();
        let mut glyphs: Vec<GlyphInstance> = Vec::new();

        let font_size = self.theme.ui.sidebar_font_size;
        let line_h = model.line_height.max(18.0);
        let row_v_pad = 4.0;
        let row_h = line_h * 2.0 + 16.0;
        let text_x = panel_x + model.panel_padding + 8.0;

        let char_w = font_size * 0.62;
        let dot_char_w = char_w + 2.0;
        let icon_col_w = dot_char_w + 4.0 + 4.0 * char_w + 24.0;
        let name_x = text_x + icon_col_w;

        quads.push(RegionDrawInstance::new(
            model.overlay_bounds,
            model.scrim_color,
        ));
        quads.push(RegionDrawInstance::new(
            [panel_x - 1.0, panel_y - 1.0, panel_w + 2.0, panel_h + 2.0],
            model.border_color,
        ));
        quads.push(RegionDrawInstance::new(model.panel_bounds, model.panel_bg));

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
            model.text_color,
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

        let footer_h = line_h + 10.0 + 1.0;
        let body_h = (panel_h - (row_top - panel_y) - footer_h - model.panel_padding).max(row_h);
        let max_visible = (body_h / row_h).floor() as usize;
        let scroll_offset = model
            .scroll_offset_rows
            .min(model.result_labels.len().saturating_sub(max_visible));

        let empty_str = String::new();
        for (visible_idx, header) in model
            .result_labels
            .iter()
            .skip(scroll_offset)
            .take(max_visible)
            .enumerate()
        {
            let absolute_idx = scroll_offset + visible_idx;
            let preview = model
                .secondary_labels
                .get(absolute_idx)
                .unwrap_or(&empty_str);
            let ranges = model
                .result_match_ranges
                .get(absolute_idx)
                .map(Vec::as_slice)
                .unwrap_or(&[]);

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

            let file_path = header.split(':').next().unwrap_or(header);
            let ext = std::path::Path::new(file_path)
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("");
            let (dot_color, ext_label) = ext_icon_dot(ext, &self.theme);
            let picker_dot = self.theme.icons.file_picker_dot.as_str();

            let header_y = row_top + row_v_pad;
            let preview_y = header_y + line_h + 2.0;

            glyphs.extend(layout_panel_text(
                picker_dot,
                &mut self.palette_text_system,
                &mut self.atlas,
                &self.queue,
                text_x,
                header_y,
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
                header_y,
                ext_color,
            ));

            let available_w = (panel_x + panel_w - model.panel_padding - 4.0 - name_x).max(0.0);
            let clamped_header = clamp_monospace_text(header, available_w, font_size);
            let mut header_color = model.hint_color;
            header_color[3] = if absolute_idx == model.selected_index {
                model.text_color[3]
            } else {
                (header_color[3] * 0.95).clamp(0.45, 0.90)
            };
            glyphs.extend(layout_panel_text(
                &clamped_header,
                &mut self.palette_text_system,
                &mut self.atlas,
                &self.queue,
                name_x,
                header_y,
                header_color,
            ));

            if !preview.is_empty() {
                Self::render_highlighted_label(
                    preview,
                    ranges,
                    name_x,
                    preview_y,
                    font_size,
                    model,
                    &mut self.palette_text_system,
                    &mut self.atlas,
                    &self.queue,
                    &mut glyphs,
                );
            }

            row_top += row_h;
        }

        if model.result_labels.is_empty() {
            glyphs.extend(layout_panel_text(
                "  (no results — type to grep workspace)",
                &mut self.palette_text_system,
                &mut self.atlas,
                &self.queue,
                text_x,
                row_top + row_v_pad,
                model.hint_color,
            ));
        }

        let footer_y = panel_y + panel_h - footer_h;
        quads.push(RegionDrawInstance::new(
            [panel_x, footer_y, panel_w, 1.0],
            model.border_color,
        ));
        glyphs.extend(layout_panel_text(
            "  ↑↓ navigate   ↵ open match   esc close",
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
