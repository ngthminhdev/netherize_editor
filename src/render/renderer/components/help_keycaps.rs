use crate::{
    render::{glyph_instance::GlyphInstance, region_pipeline::RegionDrawInstance},
    text::{atlas::GlyphAtlas, text_system::TextSystem},
};

use super::super::helpers::{estimate_monospace_width, layout_clamp, layout_panel_text_bold};

#[derive(Clone, Copy)]
pub struct HelpKeycapPalette {
    pub border: [f32; 4],
    pub fill: [f32; 4],
    pub shadow: [f32; 4],
    pub highlight: [f32; 4],
    pub text: [f32; 4],
}

fn centered_text_origin_x(origin_x: f32, content_width: f32, text_width: f32) -> f32 {
    origin_x + ((content_width - text_width) * 0.5).max(0.0)
}

fn centered_text_origin_y(origin_y: f32, content_height: f32, line_height: f32) -> f32 {
    origin_y + ((content_height - line_height) * 0.5).max(0.0)
}

fn mix(a: [f32; 4], b: [f32; 4], t: f32, alpha: f32) -> [f32; 4] {
    [
        a[0] * (1.0 - t) + b[0] * t,
        a[1] * (1.0 - t) + b[1] * t,
        a[2] * (1.0 - t) + b[2] * t,
        alpha,
    ]
}

pub fn help_keycap_palette(
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

    let (tone, text) = if matches!(normalized.as_str(), "cmd" | "⌘" | "option" | "opt" | "alt") {
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

pub fn layout_help_keycaps(
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
    if font_size.is_nan() || row_height.is_nan() {
        return Vec::new();
    }
    let mut glyphs = Vec::new();
    let mut cursor_x = origin_x;
    let scale = (font_size / 14.0).max(0.5);
    let key_gap = (font_size * 0.44).max(4.0 * scale);
    let key_height = layout_clamp(
        font_size + 12.0 * scale,
        24.0 * scale,
        row_height - 6.0 * scale,
    );
    let key_radius = layout_clamp(key_height * 0.20, 4.0 * scale, 12.0 * scale);
    let border_thickness = layout_clamp(key_height * 0.06, 1.0, 1.5 * scale);
    let key_padding_x = layout_clamp(font_size * 0.8, 8.0 * scale, 24.0 * scale);
    let key_shadow_height = layout_clamp(key_height * 0.10, 2.0 * scale, 4.0 * scale);
    let key_origin_y = centered_text_origin_y(origin_y, row_height, key_height);
    let key_line_height = font_size + 2.0 * scale;
    let inner_radius = (key_radius - border_thickness).max(2.0 * scale);

    text_system.set_size(None, Some(key_line_height));
    let dummy = [255u8, 255u8, 255u8, 255u8];
    text_system.set_text_bold_color("Ag", dummy);
    let key_line_y = text_system
        .buffer()
        .layout_runs()
        .next()
        .map(|run| run.line_y)
        .unwrap_or(key_line_height);
    let key_text_y =
        key_origin_y + ((key_height - key_line_height) * 0.5).max(0.0) + key_line_height
            - key_line_y;

    for key in keys.iter().copied() {
        text_system.set_text_bold_color(key, dummy);
        let label_w = text_system
            .buffer()
            .layout_runs()
            .next()
            .map(|run| run.line_w)
            .unwrap_or_else(|| estimate_monospace_width(key, font_size));
        if label_w.is_nan() {
            cursor_x += key_gap;
            continue;
        }
        let key_width = layout_clamp(label_w + key_padding_x * 2.0, 32.0 * scale, 400.0 * scale);
        let key_text_x = centered_text_origin_x(cursor_x, key_width, label_w);
        let palette = help_keycap_palette(key, fg, fg_dim, accent, info, warning, error, panel_bg);

        chrome.push(
            RegionDrawInstance::new(
                [cursor_x, key_origin_y, key_width, key_height],
                palette.border,
            )
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

pub fn estimate_help_keycaps_width(keys: &[&str], font_size: f32) -> f32 {
    if font_size.is_nan() {
        return 0.0;
    }
    let scale = (font_size / 14.0).max(0.5);
    let key_gap = (font_size * 0.44).max(4.0 * scale);
    let key_padding_x = layout_clamp(font_size * 0.8, 8.0 * scale, 24.0 * scale);
    let mut total = 0.0;
    for (i, key) in keys.iter().copied().enumerate() {
        if i > 0 {
            total += key_gap;
        }
        let label_w = estimate_monospace_width(key, font_size);
        total += layout_clamp(label_w + key_padding_x * 2.0, 32.0 * scale, 400.0 * scale);
    }
    total
}
