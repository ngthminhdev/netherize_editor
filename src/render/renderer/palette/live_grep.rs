#![allow(unused_imports)]

use crate::{
    app::{app_state::AppState, command_palette::CommandPaletteRenderModel, input::LeapTarget},
    render::{
        glyph_instance::GlyphInstance, icon_pipeline::IconDrawInstance,
        region_pipeline::RegionDrawInstance, renderer::Renderer,
    },
};

use super::super::{
    components::PrefixIconBadgeChrome,
    helpers::{
        clamp_monospace_text, estimate_monospace_width, gutter_width_for_editor, layout_panel_text,
        layout_panel_text_bold, rect_to_scissor,
    },
};
use super::{
    PALETTE_FOOTER_TOP_PAD, PALETTE_HEADER_BOTTOM_PAD, PaletteFooterAction,
    palette_footer_content_height, palette_footer_height, push_palette_icon_or_badge,
    render_palette_badge, render_palette_chrome, render_palette_footer, render_palette_selection,
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
        let mut icons: Vec<IconDrawInstance> = Vec::new();

        let font_size = self.theme.ui.sidebar_font_size;
        let line_h = model.line_height.max(18.0);
        let row_v_pad = 4.0;
        let row_h = line_h * 2.0 + 16.0;
        let text_x = panel_x + model.panel_padding + 8.0;

        let file_badge_size = (row_h * 0.58).clamp(30.0, 42.0);
        let name_x = text_x + file_badge_size + 18.0;

        render_palette_chrome(model, &mut quads);

        let mut row_top = panel_y + model.panel_padding;

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
            model.success_color,
            self.theme.ui.bg.as_f32(),
        );
        glyphs.extend(badge_glyphs);

        let query_x = text_x + badge_w + 10.0;
        let query_y = row_top + ((badge_h - line_h) * 0.5).max(0.0);
        let prefix_w = model.prompt_prefix.chars().count() as f32 * font_size * 0.60;
        glyphs.extend(layout_panel_text(
            &model.prompt_prefix,
            &mut self.palette_text_system,
            &mut self.atlas,
            &self.queue,
            query_x,
            query_y,
            model.hint_color,
        ));
        glyphs.extend(layout_panel_text(
            &model.prompt_query,
            &mut self.palette_text_system,
            &mut self.atlas,
            &self.queue,
            query_x + prefix_w,
            query_y,
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
            query_y,
            model.hint_color,
        ));
        row_top += badge_h + PALETTE_HEADER_BOTTOM_PAD;

        quads.push(RegionDrawInstance::new(
            [panel_x, row_top, panel_w, 1.0],
            model.border_color,
        ));
        row_top += 6.0;

        let footer_h = palette_footer_height(line_h);
        let body_h = (panel_h - (row_top - panel_y) - footer_h - model.panel_padding).max(0.0);
        let max_visible = ((body_h / row_h).floor() as usize).max(1);
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
                render_palette_selection(model, &mut quads, row_top, row_h);
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
            let filename = std::path::Path::new(file_path)
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or(file_path);
            let file_icon = self.theme.icon_theme_for_filename(filename, false);
            let badge = file_icon.glyph.as_str();
            let badge_color = file_icon.color.as_f32();

            let header_y = row_top + row_v_pad;
            let preview_y = header_y + line_h + 2.0;
            let badge_y = row_top + (row_h - file_badge_size) * 0.5;
            push_palette_icon_or_badge(
                badge,
                badge_color,
                model.panel_bg,
                [text_x, badge_y, file_badge_size, file_badge_size],
                0.82,
                PrefixIconBadgeChrome::None,
                &mut self.palette_text_system,
                &mut self.atlas,
                &self.queue,
                &mut quads,
                &mut glyphs,
                &mut icons,
            );

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
                    label: "open match",
                },
                PaletteFooterAction {
                    keys: &["󱊷"],
                    label: "close",
                },
            ],
        ));

        self.palette_chrome_instances = quads;
        self.palette_icon_instances = icons;
        self.palette_icon_pipeline.upload_instances(
            &self.device,
            &self.palette_icon_instances,
            [
                self.surface_state.config.width,
                self.surface_state.config.height,
            ],
        );
        self.palette_glyph_instances = glyphs;
    }
}
