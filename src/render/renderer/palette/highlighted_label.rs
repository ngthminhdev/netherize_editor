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
    // ── Shared label renderer ──────────────────────────────────────────────────

    /// Render a label string with fuzzy-match character highlights.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn render_highlighted_label(
        label: &str,
        ranges: &[(usize, usize)],
        start_x: f32,
        y: f32,
        font_size: f32,
        model: &CommandPaletteRenderModel,
        text_system: &mut crate::text::text_system::TextSystem,
        atlas: &mut crate::text::atlas::GlyphAtlas,
        queue: &wgpu::Queue,
        glyphs: &mut Vec<GlyphInstance>,
    ) {
        let mut seg_x = start_x;
        let mut cursor = 0usize;
        let mut segs: Vec<(&str, [f32; 4])> = Vec::new();

        for &(raw_start, raw_end) in ranges {
            let Some((mut start, end)) = Self::sanitize_label_range(label, raw_start, raw_end)
            else {
                continue;
            };
            if end <= cursor {
                continue;
            }
            start = start.max(cursor);
            if start > cursor {
                segs.push((&label[cursor..start], model.label_color));
            }
            if end > start {
                segs.push((&label[start..end], model.match_color));
            }
            cursor = end;
        }
        if cursor < label.len() {
            segs.push((&label[cursor..], model.label_color));
        }
        if segs.is_empty() {
            segs.push((label, model.label_color));
        }

        for (seg_text, seg_color) in &segs {
            glyphs.extend(layout_panel_text(
                seg_text,
                text_system,
                atlas,
                queue,
                seg_x,
                y,
                *seg_color,
            ));
            seg_x += seg_text.chars().count() as f32 * font_size * 0.60;
        }
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
