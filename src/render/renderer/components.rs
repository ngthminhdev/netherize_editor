use crate::{
    render::{glyph_instance::GlyphInstance, region_pipeline::RegionDrawInstance},
    text::{atlas::GlyphAtlas, text_system::TextSystem},
};

use super::helpers::{estimate_monospace_width, layout_panel_text, layout_panel_text_bold};

#[derive(Clone, Copy)]
pub(super) enum ShortcutHintSegment<'a> {
    Text(&'a str),
    Keys(&'a [&'a str]),
}

#[derive(Clone, Copy)]
pub(super) struct HighlightChipStyle {
    pub bg: [f32; 4],
    pub border: [f32; 4],
    pub radius: f32,
    pub border_thickness: f32,
}

pub(super) fn push_centered_highlight_chip(
    chrome: &mut Vec<RegionDrawInstance>,
    center_x: f32,
    origin_y: f32,
    width: f32,
    height: f32,
    style: HighlightChipStyle,
) {
    let chip_x = center_x - width * 0.5;
    let border_thickness = style.border_thickness.max(1.0);

    chrome.push(
        RegionDrawInstance::new([chip_x, origin_y, width, height], style.border)
            .with_radius(style.radius),
    );
    chrome.push(
        RegionDrawInstance::new(
            [
                chip_x + border_thickness,
                origin_y + border_thickness,
                (width - border_thickness * 2.0).max(1.0),
                (height - border_thickness * 2.0).max(1.0),
            ],
            style.bg,
        )
        .with_radius((style.radius - border_thickness).max(2.0)),
    );
}

pub(super) fn layout_shortcut_hint(
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
    key_shadow: [f32; 4],
    key_text_color: [f32; 4],
) -> Vec<GlyphInstance> {
    let mut glyphs = Vec::new();
    let mut cursor_x = origin_x;
    let key_gap = (font_size * 0.36).max(4.0);
    let segment_gap = (font_size * 0.72).max(8.0);
    let key_height = (line_height + 8.0).max(font_size + 10.0);
    let key_radius = (key_height * 0.22).max(4.0);
    let key_bottom_height = 2.0f32.max(key_height * 0.08);
    let key_padding_x = (font_size * 0.64).max(7.0);
    let key_text_y = origin_y + ((key_height - line_height) * 0.5).max(0.0);

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

                    let key_width = estimate_monospace_width(key, font_size) + key_padding_x * 2.0;
                    let border_thickness = (key_height * 0.08).clamp(1.0, 2.0);
                    let mut key_outline = key_text_color;
                    key_outline[3] = key_outline[3].max(0.8);
                    let mut key_fill = key_bg;
                    key_fill[3] = key_fill[3].max(0.92);
                    let mut key_base = key_shadow;
                    key_base[3] = key_base[3].max(0.95);
                    let mut key_inner = key_bg;
                    key_inner[3] = (key_inner[3] + 0.08).min(1.0);

                    chrome.push(
                        RegionDrawInstance::new(
                            [cursor_x, origin_y, key_width, key_height],
                            key_outline,
                        )
                        .with_radius(key_radius),
                    );
                    chrome.push(
                        RegionDrawInstance::new(
                            [
                                cursor_x + border_thickness,
                                origin_y + border_thickness,
                                (key_width - border_thickness * 2.0).max(1.0),
                                (key_height - border_thickness * 2.0).max(1.0),
                            ],
                            key_fill,
                        )
                        .with_radius((key_radius - border_thickness).max(3.0)),
                    );
                    chrome.push(
                        RegionDrawInstance::new(
                            [
                                cursor_x + border_thickness,
                                origin_y + key_height - key_bottom_height - border_thickness,
                                (key_width - border_thickness * 2.0).max(1.0),
                                key_bottom_height.max(1.0),
                            ],
                            key_base,
                        )
                        .with_radius((key_radius * 0.5).max(2.0)),
                    );
                    chrome.push(
                        RegionDrawInstance::new(
                            [
                                cursor_x + border_thickness,
                                origin_y + border_thickness,
                                (key_width - border_thickness * 2.0).max(1.0),
                                (key_height * 0.42).max(1.0),
                            ],
                            key_inner,
                        )
                        .with_radius((key_radius - border_thickness - 1.0).max(2.0)),
                    );
                    glyphs.extend(layout_panel_text_bold(
                        key,
                        text_system,
                        atlas,
                        queue,
                        cursor_x + key_padding_x,
                        key_text_y,
                        key_text_color,
                    ));
                    cursor_x += key_width;
                }
            }
        }

        cursor_x += segment_gap;
    }

    glyphs
}
