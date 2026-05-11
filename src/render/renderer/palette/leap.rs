#![allow(unused_imports)]

use crate::{
    app::{app_state::AppState, command_palette::CommandPaletteRenderModel, input::LeapTarget},
    render::{
        glyph_instance::GlyphInstance, region_pipeline::RegionDrawInstance, renderer::Renderer,
    },
    text::layout_sync::visual_y_for_logical_scroll_with_folds,
};

use super::super::helpers::{
    clamp_monospace_text, estimate_monospace_width, ext_icon_dot, gutter_width_for_editor,
    layout_panel_text, layout_panel_text_bold, rect_to_scissor,
};

impl Renderer {
    // ── Leap label overlay ─────────────────────────────────────────────────────

    /// Draw cyan Leap labels over editor glyphs, filtered by the currently typed prefix.
    ///
    /// `typed_prefix`: phần prefix user đã gõ; renderer chỉ vẽ suffix còn lại.
    pub fn update_editor_leap_labels(
        &mut self,
        labels: &[LeapTarget],
        typed_prefix: &str,
        app_state: &AppState,
        center_bounds: [f32; 4],
    ) {
        self.leap_label_scissor = rect_to_scissor(center_bounds);

        if labels.is_empty() {
            self.leap_label_bg_instances.clear();
            self.leap_label_glyph_instances.clear();
            self.leap_label_text_pipeline
                .upload_instances(&self.device, &self.queue, &[]);
            return;
        }

        let line_height = self.theme.editor.line_height;
        let font_size = self.theme.editor.font_size;
        let total_lines = app_state.total_lines().max(1);
        let scroll_y = visual_y_for_logical_scroll_with_folds(
            &self.text_system,
            app_state.current_scroll_y,
            app_state.folded_ranges(),
        );
        let gutter_digits = total_lines.to_string().len().max(3);
        let gutter_width = gutter_width_for_editor(gutter_digits, font_size, line_height);
        let origin_x = center_bounds[0] + self.editor_padding_x + gutter_width;
        let origin_y = center_bounds[1] + self.editor_padding_y + line_height - scroll_y;

        let label_color = self.theme.ui.cyan.as_f32();
        let mut char_bg_color = self.theme.ui.panel_bg.as_f32();
        char_bg_color[3] = char_bg_color[3].min(0.82);
        let mut overlay_color = self.theme.ui.overlay_bg.as_f32();
        overlay_color[3] = overlay_color[3].min(0.28);

        let label_map: std::collections::HashMap<usize, &str> = labels
            .iter()
            .filter_map(|target| {
                target
                    .label
                    .starts_with(typed_prefix)
                    .then_some((target.char_idx, target.label.as_str()))
            })
            .collect();

        if label_map.is_empty() {
            self.leap_label_bg_instances.clear();
            self.leap_label_glyph_instances.clear();
            self.leap_label_text_pipeline
                .upload_instances(&self.device, &self.queue, &[]);
            return;
        }

        // Measure the baseline shift of the 2x font used for label chars.
        let color_u8 = (label_color[0] * 255.0) as u8;
        let dummy = [color_u8, color_u8, color_u8, 255u8];
        let sample_text = label_map
            .values()
            .find_map(|label| label.strip_prefix(typed_prefix))
            .filter(|remaining| !remaining.is_empty())
            .unwrap_or("a");
        self.leap_label_text_system
            .set_text_bold_color(sample_text, dummy);
        let label_line_y = self
            .leap_label_text_system
            .buffer()
            .layout_runs()
            .next()
            .map(|r| r.line_y)
            .unwrap_or(0.0);

        let mut bg_per_char: Vec<RegionDrawInstance> = Vec::with_capacity(labels.len());
        let mut glyph_instances: Vec<GlyphInstance> = Vec::with_capacity(labels.len());

        for run in self.text_system.buffer().layout_runs() {
            if app_state.is_line_folded(run.line_i) {
                continue;
            }
            let y_offset = app_state.folded_visual_y_offset_before(run.line_i, run.line_height);
            for glyph in run.glyphs {
                let rope_char_idx = app_state
                    .char_idx_for_line(run.line_i)
                    .saturating_add(app_state.byte_to_char_in_line(run.line_i, glyph.start));
                let Some(&full_label) = label_map.get(&rope_char_idx) else {
                    continue;
                };
                let Some(visible_label) = full_label.strip_prefix(typed_prefix) else {
                    continue;
                };
                if visible_label.is_empty() {
                    continue;
                }

                let glyph_x = origin_x + glyph.x;
                let glyph_top = origin_y + run.line_top - y_offset;
                let cell_w = glyph.w.max(font_size * 0.5);
                let cell_h = run.line_height.max(1.0);
                let badge_padding_x = (font_size * 0.30).max(4.0);
                let badge_padding_y = (font_size * 0.08).max(2.0);
                let label_width =
                    estimate_monospace_width(visible_label, font_size * 2.0).max(cell_w);
                let badge_w = (label_width + badge_padding_x * 2.0).max(cell_w + 6.0);
                let badge_h = (cell_h + badge_padding_y * 2.0).max(font_size * 1.1);
                let badge_x = glyph_x - ((badge_w - cell_w) * 0.5);
                let badge_y = glyph_top - badge_padding_y;

                // Solid badge background overwrites the original glyph before drawing suffix text.
                bg_per_char.push(RegionDrawInstance::new(
                    [badge_x, badge_y, badge_w, badge_h],
                    char_bg_color,
                ));

                // Render only the remaining suffix so the overlay visually narrows as the user types.
                let baseline_y = origin_y + run.line_y - y_offset;
                let label_origin_y = baseline_y - label_line_y;
                let label_origin_x = badge_x + (badge_w - label_width) * 0.5;
                glyph_instances.extend(layout_panel_text_bold(
                    visible_label,
                    &mut self.leap_label_text_system,
                    &mut self.atlas,
                    &self.queue,
                    label_origin_x,
                    label_origin_y,
                    label_color,
                ));
            }
        }

        // Dim overlay (whole editor area) + per-char backgrounds
        let mut all_bg = vec![RegionDrawInstance::new(
            [
                center_bounds[0],
                center_bounds[1],
                center_bounds[2],
                center_bounds[3],
            ],
            overlay_color,
        )];
        all_bg.extend(bg_per_char);
        self.leap_label_bg_instances = all_bg;
        self.leap_label_glyph_instances = glyph_instances;
        self.leap_label_text_pipeline.upload_instances(
            &self.device,
            &self.queue,
            &self.leap_label_glyph_instances,
        );
    }

    /// Clear Leap overlay — called on LeapCancel or after a jump completes.
    pub fn clear_leap_labels(&mut self) {
        self.leap_label_scissor = None;
        self.leap_label_bg_instances.clear();
        self.leap_label_glyph_instances.clear();
        self.leap_label_text_pipeline
            .upload_instances(&self.device, &self.queue, &[]);
    }
}
