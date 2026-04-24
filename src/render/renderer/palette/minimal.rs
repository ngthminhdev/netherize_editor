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
    // ── Minimalist palette (Command / Symbol / VimCommand) ─────────────────────

    /// Single-line input box, TRUE center of screen.
    /// Results rendered below prompt with subtle row separators.
    /// VimCommand mode: prompt only, no results, no separator.
    pub(super) fn render_command_palette_minimalist(&mut self, model: &CommandPaletteRenderModel) {
        let [panel_x, panel_y, panel_w, panel_h] = model.panel_bounds;
        self.palette_scissor = rect_to_scissor(model.panel_bounds);

        let inner_width = (panel_w - model.panel_padding * 2.0).max(1.0);
        self.palette_text_system
            .set_size(Some(inner_width), Some(model.line_height));

        let mut quads: Vec<RegionDrawInstance> = Vec::new();
        let mut glyphs: Vec<GlyphInstance> = Vec::new();

        let font_size = self.theme.ui.sidebar_font_size;
        let text_x = panel_x + model.panel_padding + 8.0; // 8px left indent
        let line_h = model.line_height.max(18.0);
        let mut row_top = panel_y + model.panel_padding;

        // Chrome: scrim → border → panel bg
        quads.push(RegionDrawInstance::new(
            model.overlay_bounds,
            model.scrim_color,
        ));
        quads.push(RegionDrawInstance::new(
            [panel_x - 1.0, panel_y - 1.0, panel_w + 2.0, panel_h + 2.0],
            model.border_color,
        ));
        quads.push(RegionDrawInstance::new(model.panel_bounds, model.panel_bg));

        // Prompt line
        let prefix_w = model.prompt_prefix.chars().count() as f32 * font_size * 0.60;
        let query_color =
            if !model.show_results || model.result_match_ranges.iter().any(|r| !r.is_empty()) {
                model.text_color
            } else {
                model.hint_color
            };
        glyphs.extend(layout_panel_text(
            &model.prompt_prefix,
            &mut self.palette_text_system,
            &mut self.atlas,
            &self.queue,
            text_x,
            row_top,
            model.hint_color,
        ));
        glyphs.extend(layout_panel_text(
            &model.prompt_query,
            &mut self.palette_text_system,
            &mut self.atlas,
            &self.queue,
            text_x + prefix_w,
            row_top,
            query_color,
        ));
        row_top += line_h;

        // VimCommand: only prompt — exit early
        if !model.show_results {
            self.palette_chrome_instances = quads;
            self.palette_glyph_instances = glyphs;
            return;
        }

        // Separator beneath prompt
        quads.push(RegionDrawInstance::new(
            [
                panel_x + model.panel_padding,
                row_top + 2.0,
                inner_width - model.panel_padding,
                1.0,
            ],
            model.border_color,
        ));
        row_top += 8.0;

        // Result rows
        let row_v_pad = 4.0;
        let row_h = line_h + row_v_pad * 2.0;
        let max_visible = (((panel_h - model.panel_padding * 2.0 - line_h - 8.0) / row_h).floor()
            as usize)
            .max(1);
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
                sep[3] *= 0.35;
                quads.push(RegionDrawInstance::new(
                    [text_x, row_top, inner_width - 8.0, 1.0],
                    sep,
                ));
            }

            let label_y = row_top + row_v_pad;
            Self::render_highlighted_label(
                label,
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
            row_top += row_h;
        }

        self.palette_chrome_instances = quads;
        self.palette_glyph_instances = glyphs;
    }

    // ── File Picker (complex) ──────────────────────────────────────────────────
}
