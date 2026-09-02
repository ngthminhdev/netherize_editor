use crate::{
    render::{glyph_instance::GlyphInstance, region_pipeline::RegionDrawInstance},
    text::{atlas::GlyphAtlas, text_system::TextSystem},
};

use super::super::helpers::{estimate_monospace_width, layout_clamp, layout_panel_text_bold};

#[derive(Clone, Copy)]
pub struct HelpKeycapPalette {
    pub border: [f32; 4],
    pub fill: [f32; 4],
    pub text: [f32; 4],
}

fn centered_text_origin_x(origin_x: f32, content_width: f32, text_width: f32) -> f32 {
    origin_x + ((content_width - text_width) * 0.5).max(0.0)
}

fn centered_text_origin_y(origin_y: f32, content_height: f32, line_height: f32) -> f32 {
    origin_y + ((content_height - line_height) * 0.5).max(0.0)
}

pub(crate) fn mix(a: [f32; 4], b: [f32; 4], t: f32, alpha: f32) -> [f32; 4] {
    [
        a[0] * (1.0 - t) + b[0] * t,
        a[1] * (1.0 - t) + b[1] * t,
        a[2] * (1.0 - t) + b[2] * t,
        alpha,
    ]
}

/// The single source of truth for keycap chip colours — shared with
/// `shortcut_hint.rs` so both components render pixel-identical chips.
pub(crate) fn flat_keycap_palette(fg: [f32; 4], panel_bg: [f32; 4]) -> HelpKeycapPalette {
    HelpKeycapPalette {
        border: mix(panel_bg, fg, 0.30, 0.95),
        fill: mix(panel_bg, fg, 0.07, 1.0),
        text: [fg[0], fg[1], fg[2], fg[3] * 0.92],
    }
}

/// Draw one flat keycap chip (border + inset fill). Shared by both keycap
/// components so there is exactly one chip look in the whole app.
pub(crate) fn push_flat_keycap(
    chrome: &mut Vec<RegionDrawInstance>,
    rect: [f32; 4],
    radius: f32,
    palette: &HelpKeycapPalette,
) {
    let [x, y, w, h] = rect;
    let border = 1.0;
    chrome.push(RegionDrawInstance::new([x, y, w, h], palette.border).with_radius(radius));
    chrome.push(
        RegionDrawInstance::new(
            [
                x + border,
                y + border,
                (w - border * 2.0).max(1.0),
                (h - border * 2.0).max(1.0),
            ],
            palette.fill,
        )
        .with_radius((radius - border).max(2.0)),
    );
}

/// Geometry every keycap in the app shares, derived from the row it sits in.
pub(crate) struct KeycapMetrics {
    pub gap: f32,
    pub height: f32,
    pub radius: f32,
    pub padding_x: f32,
    pub origin_y: f32,
    pub line_height: f32,
}

pub(crate) fn keycap_metrics(origin_y: f32, font_size: f32, row_height: f32) -> KeycapMetrics {
    let scale = (font_size / 14.0).max(0.5);
    let height = layout_clamp(
        font_size + 10.0 * scale,
        20.0 * scale,
        row_height - 4.0 * scale,
    );
    KeycapMetrics {
        gap: (font_size * 0.44).max(4.0 * scale),
        height,
        radius: layout_clamp(height * 0.22, 3.0 * scale, 6.0 * scale),
        padding_x: layout_clamp(font_size * 0.62, 7.0 * scale, 18.0 * scale),
        origin_y: centered_text_origin_y(origin_y, row_height, height),
        line_height: font_size + 2.0 * scale,
    }
}

/// Width one keycap takes for `key` (label + padding, clamped) — the same
/// number `layout_keycap` ends up with, so hit-tests can be computed
/// without a text system.
pub(crate) fn estimate_keycap_width(key: &str, font_size: f32) -> f32 {
    let scale = (font_size / 14.0).max(0.5);
    let padding_x = layout_clamp(font_size * 0.62, 7.0 * scale, 18.0 * scale);
    let label_w = estimate_monospace_width(key, font_size);
    layout_clamp(label_w + padding_x * 2.0, 26.0 * scale, 400.0 * scale)
}

/// Draw ONE keycap chip with an explicit palette at `origin_x`. Returns the
/// glyphs and the chip width. `layout_help_keycaps` is a loop over this;
/// interactive surfaces (Dojo footer) call it directly with a hover/pressed
/// palette so every chip in the app keeps the same shape.
#[allow(clippy::too_many_arguments)]
pub(crate) fn layout_keycap(
    key: &str,
    text_system: &mut TextSystem,
    atlas: &mut GlyphAtlas,
    queue: &wgpu::Queue,
    chrome: &mut Vec<RegionDrawInstance>,
    origin_x: f32,
    metrics: &KeycapMetrics,
    font_size: f32,
    palette: &HelpKeycapPalette,
) -> (Vec<GlyphInstance>, f32) {
    let scale = (font_size / 14.0).max(0.5);
    text_system.set_size(None, Some(metrics.line_height));
    let dummy = [255u8, 255u8, 255u8, 255u8];
    text_system.set_text_bold_color("Ag", dummy);
    let key_line_y = text_system
        .buffer()
        .layout_runs()
        .next()
        .map(|run| run.line_y)
        .unwrap_or(metrics.line_height);
    let key_text_y = metrics.origin_y
        + ((metrics.height - metrics.line_height) * 0.5).max(0.0)
        + metrics.line_height
        - key_line_y;

    text_system.set_text_bold_color(key, dummy);
    let label_w = text_system
        .buffer()
        .layout_runs()
        .next()
        .map(|run| run.line_w)
        .unwrap_or_else(|| estimate_monospace_width(key, font_size));
    if label_w.is_nan() {
        return (Vec::new(), 0.0);
    }
    let key_width = layout_clamp(
        label_w + metrics.padding_x * 2.0,
        26.0 * scale,
        400.0 * scale,
    );
    let key_text_x = centered_text_origin_x(origin_x, key_width, label_w);
    push_flat_keycap(
        chrome,
        [origin_x, metrics.origin_y, key_width, metrics.height],
        metrics.radius,
        palette,
    );
    let glyphs = layout_panel_text_bold(
        key,
        text_system,
        atlas,
        queue,
        key_text_x,
        key_text_y,
        palette.text,
    );
    (glyphs, key_width)
}

#[allow(clippy::too_many_arguments)]
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
    _fg_dim: [f32; 4],
    _accent: [f32; 4],
    _info: [f32; 4],
    _warning: [f32; 4],
    _error: [f32; 4],
    panel_bg: [f32; 4],
) -> Vec<GlyphInstance> {
    if font_size.is_nan() || row_height.is_nan() {
        return Vec::new();
    }
    let metrics = keycap_metrics(origin_y, font_size, row_height);
    let palette = flat_keycap_palette(fg, panel_bg);
    let mut glyphs = Vec::new();
    let mut cursor_x = origin_x;
    for key in keys.iter().copied() {
        let (key_glyphs, key_width) = layout_keycap(
            key,
            text_system,
            atlas,
            queue,
            chrome,
            cursor_x,
            &metrics,
            font_size,
            &palette,
        );
        if key_width == 0.0 {
            cursor_x += metrics.gap;
            continue;
        }
        glyphs.extend(key_glyphs);
        cursor_x += key_width + metrics.gap;
    }
    glyphs
}

pub fn estimate_help_keycaps_width(keys: &[&str], font_size: f32) -> f32 {
    if font_size.is_nan() {
        return 0.0;
    }
    let scale = (font_size / 14.0).max(0.5);
    let key_gap = (font_size * 0.44).max(4.0 * scale);
    let mut total = 0.0;
    for (i, key) in keys.iter().copied().enumerate() {
        if i > 0 {
            total += key_gap;
        }
        total += estimate_keycap_width(key, font_size);
    }
    total
}
