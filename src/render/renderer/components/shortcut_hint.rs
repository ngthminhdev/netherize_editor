use crate::{
    render::{glyph_instance::GlyphInstance, region_pipeline::RegionDrawInstance},
    text::{atlas::GlyphAtlas, text_system::TextSystem},
};

use super::super::helpers::{estimate_monospace_width, layout_panel_text, layout_panel_text_bold};
use super::help_keycaps::{flat_keycap_palette, push_flat_keycap};

#[derive(Clone, Copy)]
pub enum ShortcutHintSegment<'a> {
    Text(&'a str),
    Keys(&'a [&'a str]),
}

fn centered_text_origin_x(origin_x: f32, content_width: f32, text_width: f32) -> f32 {
    origin_x + ((content_width - text_width) * 0.5).max(0.0)
}

fn centered_text_origin_y(origin_y: f32, content_height: f32, line_height: f32) -> f32 {
    origin_y + ((content_height - line_height) * 0.5).max(0.0)
}

/// Welcome-screen key hints. Chips are drawn by the SAME flat-keycap helper as
/// `layout_help_keycaps`, so every key hint in the app shares one look.
#[allow(clippy::too_many_arguments)]
pub fn layout_shortcut_hint(
    segments: &[ShortcutHintSegment<'_>],
    text_system: &mut TextSystem,
    atlas: &mut GlyphAtlas,
    queue: &wgpu::Queue,
    chrome: &mut Vec<RegionDrawInstance>,
    origin_x: f32,
    origin_y: f32,
    font_size: f32,
    line_height: f32,
    text_color: [f32; 4],
    key_bg: [f32; 4],
    _key_shadow: [f32; 4],
    key_text_color: [f32; 4],
) -> Vec<GlyphInstance> {
    let mut glyphs = Vec::new();
    let mut cursor_x = origin_x;
    let key_gap = (font_size * 0.32).max(4.0);
    let segment_gap = (font_size * 0.64).max(8.0);
    let key_height = (line_height + font_size * 0.42).clamp(font_size + 8.0, font_size + 14.0);
    let key_radius = (key_height * 0.22).clamp(3.0, 6.0);
    let key_padding_x = (font_size * 0.52).max(6.0);
    let key_text_y = centered_text_origin_y(origin_y, key_height, line_height);
    let palette = flat_keycap_palette(key_text_color, key_bg);

    text_system.set_size(None, Some(line_height));

    for segment in segments {
        match segment {
            ShortcutHintSegment::Text(text) => {
                if !text.is_empty() {
                    glyphs.extend(layout_panel_text(
                        text,
                        text_system,
                        atlas,
                        queue,
                        cursor_x,
                        origin_y,
                        text_color,
                    ));
                    cursor_x += estimate_monospace_width(text, font_size);
                }
            }
            ShortcutHintSegment::Keys(keys) => {
                let mut first = true;
                for key in keys.iter().copied() {
                    if !first {
                        cursor_x += key_gap;
                    }
                    first = false;

                    let key_label_w = estimate_monospace_width(key, font_size);
                    let key_width = key_label_w + key_padding_x * 2.0;
                    let key_text_x = centered_text_origin_x(cursor_x, key_width, key_label_w);

                    push_flat_keycap(
                        chrome,
                        [cursor_x, origin_y, key_width, key_height],
                        key_radius,
                        &palette,
                    );
                    glyphs.extend(layout_panel_text_bold(
                        key,
                        text_system,
                        atlas,
                        queue,
                        key_text_x,
                        key_text_y,
                        palette.text,
                    ));
                    cursor_x += key_width;
                }
            }
        }

        cursor_x += segment_gap;
    }

    glyphs
}
