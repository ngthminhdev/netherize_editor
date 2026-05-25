#![allow(unused_imports)]

use crate::{
    app::{app_state::AppState, command_palette::CommandPaletteRenderModel, input::LeapTarget},
    render::{
        glyph_instance::GlyphInstance, region_pipeline::RegionDrawInstance, renderer::Renderer,
    },
    text::layout_sync::visual_y_for_logical_scroll_with_folds,
};

use super::super::components::{
    layout_prefix_icon_badge, PrefixIconBadge, PrefixIconBadgeChrome,
};
use super::super::editor::editor_viewport_geometry;
use super::super::helpers::{
    clamp_monospace_text, estimate_monospace_width, gutter_width_for_editor,
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
        let geometry = editor_viewport_geometry(self, app_state, center_bounds);
        let viewport_bounds = [
            center_bounds[0],
            geometry.viewport_text_top,
            center_bounds[2],
            geometry.viewport_text_height,
        ];
        self.leap_label_scissor = rect_to_scissor(viewport_bounds);

        if labels.is_empty() {
            self.leap_label_bg_instances.clear();
            self.leap_label_glyph_instances.clear();
            self.leap_label_text_pipeline
                .upload_instances(&self.device, &self.queue, &[]);
            return;
        }

        let font_size = geometry.font_size;
        let scroll_y = visual_y_for_logical_scroll_with_folds(
            &self.text_system,
            app_state.current_scroll_y.max(0.0),
            app_state.folded_ranges(),
        );
        let origin_x = geometry.origin_x;
        let origin_y = geometry.viewport_text_top + geometry.line_height - scroll_y;

        let label_color = self.theme.ui.cyan.as_f32();
        let panel_bg = self.theme.ui.panel_bg.as_f32();
        let mut overlay_color = self.theme.ui.overlay_bg.as_f32();
        overlay_color[3] = 0.34;

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

        let mut chrome: Vec<RegionDrawInstance> = Vec::with_capacity(labels.len() * 2);
        let mut glyph_instances: Vec<GlyphInstance> = Vec::with_capacity(labels.len());

        for run in self.text_system.buffer().layout_runs() {
            if app_state.is_line_folded(run.line_i) {
                continue;
            }
            let y_offset = app_state.folded_visual_y_offset_before(run.line_i, run.line_height);
            let line_top_physical = origin_y + run.line_top - y_offset;
            let line_bottom_physical = line_top_physical + run.line_height;

            // Viewport culling: only draw labels that are within the editor bounds.
            if line_bottom_physical < geometry.viewport_text_top
                || line_top_physical > geometry.viewport_text_top + geometry.viewport_text_height
            {
                continue;
            }

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
                let glyph_top = line_top_physical;
                let cell_w = glyph.w.max(font_size * 0.5);
                let cell_h = run.line_height.max(1.0);
                let badge_padding_x = (font_size * 0.34).max(5.0);
                let label_width = estimate_monospace_width(visible_label, font_size);
                let badge_w = (label_width + badge_padding_x * 2.0).max(cell_w + 8.0);
                let badge_h = (font_size * 1.42).clamp(cell_h * 0.82, cell_h + 4.0);
                let badge_x = glyph_x - ((badge_w - cell_w) * 0.5);
                let badge_y = glyph_top + ((cell_h - badge_h) * 0.5).max(0.0);

                // Use shared prefix icon badge component for outline + blended background + label text.
                let badge = PrefixIconBadge {
                    icon: visible_label,
                    color: label_color,
                    panel_bg,
                    bounds: [badge_x, badge_y, badge_w, badge_h],
                    icon_scale: if visible_label.chars().count() > 1 { 0.50 } else { 0.68 },
                    y_nudge_scale: 0.08,
                    chrome: PrefixIconBadgeChrome::Outline,
                };
                glyph_instances.extend(layout_prefix_icon_badge(
                    badge,
                    &mut self.leap_label_text_system,
                    &mut self.atlas,
                    &self.queue,
                    &mut chrome,
                ));
            }
        }

        // Dim overlay (whole editor area) + badge chrome (borders + backgrounds)
        let mut all_bg = vec![RegionDrawInstance::new(viewport_bounds, overlay_color)];
        all_bg.extend(chrome);
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
