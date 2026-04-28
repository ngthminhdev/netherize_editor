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

fn centered_text_origin_x(origin_x: f32, content_width: f32, text_width: f32) -> f32 {
    origin_x + ((content_width - text_width) * 0.5).max(0.0)
}

fn centered_text_origin_y(origin_y: f32, content_height: f32, line_height: f32) -> f32 {
    origin_y + ((content_height - line_height) * 0.5).max(0.0)
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
    let key_gap = (font_size * 0.32).max(4.0);
    let segment_gap = (font_size * 0.64).max(8.0);
    let key_height = (line_height + font_size * 0.42).clamp(font_size + 8.0, font_size + 14.0);
    let key_radius = (key_height * 0.18).max(4.0);
    let key_bottom_height = 1.0f32.max(key_height * 0.06);
    let key_padding_x = (font_size * 0.52).max(6.0);
    let key_text_y = centered_text_origin_y(origin_y, key_height, line_height);

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
                    let border_thickness = (key_height * 0.06).clamp(1.0, 2.0);
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
                                (key_height * 0.34).max(1.0),
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
                        key_text_x,
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

#[derive(Clone, Copy)]
pub(super) struct HelpKeycapPalette {
    pub border: [f32; 4],
    pub fill: [f32; 4],
    pub shadow: [f32; 4],
    pub highlight: [f32; 4],
    pub text: [f32; 4],
}

fn mix(a: [f32; 4], b: [f32; 4], t: f32, alpha: f32) -> [f32; 4] {
    [
        a[0] * (1.0 - t) + b[0] * t,
        a[1] * (1.0 - t) + b[1] * t,
        a[2] * (1.0 - t) + b[2] * t,
        alpha,
    ]
}

pub(super) fn help_keycap_palette(
    key: &str,
    fg: [f32; 4],
    fg_dim: [f32; 4],
    accent: [f32; 4],
    info: [f32; 4],
    warning: [f32; 4],
    error: [f32; 4],
    panel_bg: [f32; 4],
) -> HelpKeycapPalette {
    let normalized = key
        .trim_matches(|ch| ch == '<' || ch == '>')
        .trim()
        .to_ascii_lowercase();

    let (tone, text) = if matches!(
        normalized.as_str(),
        "cmd" | "⌘" | "mod" | "option" | "opt" | "alt"
    ) {
        (accent, fg)
    } else if matches!(normalized.as_str(), "spc" | "space" | "leader") {
        (warning, fg)
    } else if matches!(
        normalized.as_str(),
        "ctrl" | "control" | "shift" | "enter" | "return" | "tab"
    ) {
        (info, fg)
    } else if matches!(
        normalized.as_str(),
        "esc" | "escape" | "backspace" | "delete"
    ) {
        (error, fg)
    } else {
        (fg_dim, fg)
    };

    HelpKeycapPalette {
        border: mix(panel_bg, tone, 0.82, 0.98),
        fill: mix(panel_bg, tone, 0.18, 0.98),
        shadow: mix(panel_bg, tone, 0.42, 0.98),
        highlight: mix(panel_bg, tone, 0.26, 1.0),
        text,
    }
}

pub(super) fn layout_help_keycaps(
    keys: &[&str],
    text_system: &mut TextSystem,
    atlas: &mut GlyphAtlas,
    queue: &wgpu::Queue,
    chrome: &mut Vec<RegionDrawInstance>,
    origin_x: f32,
    origin_y: f32,
    font_size: f32,
    row_height: f32,
    fg: [f32; 4],
    fg_dim: [f32; 4],
    accent: [f32; 4],
    info: [f32; 4],
    warning: [f32; 4],
    error: [f32; 4],
    panel_bg: [f32; 4],
) -> Vec<GlyphInstance> {
    let mut glyphs = Vec::new();
    let mut cursor_x = origin_x;
    let key_gap = (font_size * 0.44).max(8.0);
    let key_height = (font_size + 36.0).clamp(52.0, row_height - 8.0);
    let key_radius = (key_height * 0.24).clamp(8.0, 16.0);
    let border_thickness = (key_height * 0.075).clamp(1.0, 2.0);
    let key_padding_x = (font_size * 1.44).clamp(20.0, 36.0);
    let key_shadow_height = (key_height * 0.12).clamp(3.0, 6.0);
    let key_origin_y = centered_text_origin_y(origin_y, row_height, key_height);
    let key_line_height = font_size + 4.0;
    let inner_radius = (key_radius - border_thickness).max(4.0);

    text_system.set_size(None, Some(key_line_height));
    let dummy = [255u8, 255u8, 255u8, 255u8];
    text_system.set_text_bold_color("Ag", dummy);
    let key_line_y = text_system
        .buffer()
        .layout_runs()
        .next()
        .map(|run| run.line_y)
        .unwrap_or(key_line_height);
    let key_text_y = key_origin_y + ((key_height - key_line_height) * 0.5).max(0.0)
        + key_line_height
        - key_line_y;

    for key in keys.iter().copied() {
        text_system.set_text_bold_color(key, dummy);
        let label_w = text_system
            .buffer()
            .layout_runs()
            .next()
            .map(|run| run.line_w)
            .unwrap_or_else(|| estimate_monospace_width(key, font_size));
        let key_width = (label_w + key_padding_x * 2.0).clamp(88.0, 480.0);
        let key_text_x = centered_text_origin_x(cursor_x, key_width, label_w);
        let palette = help_keycap_palette(key, fg, fg_dim, accent, info, warning, error, panel_bg);

        chrome.push(
            RegionDrawInstance::new([cursor_x, key_origin_y, key_width, key_height], palette.border)
                .with_radius(key_radius),
        );
        chrome.push(
            RegionDrawInstance::new(
                [
                    cursor_x + border_thickness,
                    key_origin_y + border_thickness,
                    (key_width - border_thickness * 2.0).max(1.0),
                    (key_height - border_thickness * 2.0).max(1.0),
                ],
                palette.shadow,
            )
            .with_radius(inner_radius),
        );
        chrome.push(
            RegionDrawInstance::new(
                [
                    cursor_x + border_thickness,
                    key_origin_y + border_thickness,
                    (key_width - border_thickness * 2.0).max(1.0),
                    (key_height - border_thickness * 2.0 - key_shadow_height).max(1.0),
                ],
                palette.fill,
            )
            .with_radius(inner_radius),
        );
        chrome.push(
            RegionDrawInstance::new(
                [
                    cursor_x + border_thickness,
                    key_origin_y + border_thickness,
                    (key_width - border_thickness * 2.0).max(1.0),
                    ((key_height - border_thickness * 2.0 - key_shadow_height) * 0.38).max(1.0),
                ],
                palette.highlight,
            )
            .with_radius((inner_radius - 1.0).max(3.0)),
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

        cursor_x += key_width + key_gap;
    }

    glyphs
}

pub(super) fn estimate_help_keycaps_width(keys: &[&str], font_size: f32) -> f32 {
    let key_gap = (font_size * 0.44).max(8.0);
    let key_padding_x = (font_size * 1.44).clamp(20.0, 36.0);
    let mut total = 0.0;

    for (idx, key) in keys.iter().copied().enumerate() {
        if idx > 0 {
            total += key_gap;
        }
        let label_w = estimate_monospace_width(key, font_size);
        total += (label_w + key_padding_x * 2.0).clamp(88.0, 480.0);
    }

    total
}
