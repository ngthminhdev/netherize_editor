#![allow(unused_imports)]

use crate::{
    app::{app_state::AppState, command_palette::CommandPaletteRenderModel, input::LeapTarget},
    render::{
        glyph_instance::GlyphInstance, region_pipeline::RegionDrawInstance, renderer::Renderer,
    },
};

use crate::{config::theme_config::linear_rgba_to_srgb_u8, text::text_system::StyledTextSpan};

use super::super::helpers::{
    clamp_monospace_text, estimate_monospace_width, gutter_width_for_editor, layout_panel_rich_text,
    layout_panel_text, layout_panel_text_bold, rect_to_scissor,
};

impl Renderer {
    // ── Shared label renderer ──────────────────────────────────────────────────

    /// Render a label string with fuzzy-match character highlights.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn render_highlighted_label(
        label: &str,
        ranges: &[(usize, usize)],
        start_x: f32,
        y: f32,
        _font_size: f32,
        model: &CommandPaletteRenderModel,
        text_system: &mut crate::text::text_system::TextSystem,
        atlas: &mut crate::text::atlas::GlyphAtlas,
        queue: &wgpu::Queue,
        glyphs: &mut Vec<GlyphInstance>,
    ) {
        if label.is_empty() {
            return;
        }

        // Highlight the fuzzy-match ranges by rendering the WHOLE label in one
        // shaped pass (styled spans) rather than advancing x per-segment with a
        // monospace width estimate. That estimate drifts for any font whose
        // advance isn't exactly `font_size * 0.60` and drops kerning across
        // segment boundaries — both of which made filtered palette results look
        // jittery / misaligned. Shaping once keeps every glyph in its true
        // position and the highlight colors land on the right characters.
        let match_rgba = linear_rgba_to_srgb_u8(model.match_color);
        let mut spans: Vec<StyledTextSpan> = Vec::new();
        for &(raw_start, raw_end) in ranges {
            if let Some((start, end)) = Self::sanitize_label_range(label, raw_start, raw_end) {
                spans.push(StyledTextSpan::new(start, end, match_rgba));
            }
        }

        // Single line: drop the wrap width so a long label never folds onto the
        // next row (horizontal overflow is clipped by the palette scissor).
        text_system.set_size(None, None);
        glyphs.extend(layout_panel_rich_text(
            label,
            &spans,
            model.label_color,
            text_system,
            atlas,
            queue,
            start_x,
            y,
        ));
    }

    pub(super) fn sanitize_label_range(
        label: &str,
        start: usize,
        end: usize,
    ) -> Option<(usize, usize)> {
        if label.is_empty() {
            return None;
        }

        let len = label.len();
        let mut start = start.min(len);
        let mut end = end.min(len);
        if start >= end {
            return None;
        }

        while start > 0 && !label.is_char_boundary(start) {
            start -= 1;
        }
        while end < len && !label.is_char_boundary(end) {
            end += 1;
        }

        (start < end).then_some((start, end))
    }
}
