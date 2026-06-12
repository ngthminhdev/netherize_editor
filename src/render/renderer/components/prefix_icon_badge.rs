use crate::{
    render::{glyph_instance::GlyphInstance, region_pipeline::RegionDrawInstance},
    text::{atlas::GlyphAtlas, text_system::TextSystem},
};

use super::super::helpers::{estimate_monospace_width, layout_panel_text_bold};

pub struct PrefixIconBadge<'a> {
    pub icon: &'a str,
    pub color: [f32; 4],
    pub panel_bg: [f32; 4],
    pub bounds: [f32; 4],
    pub icon_scale: f32,
    pub y_nudge_scale: f32,
    pub chrome: PrefixIconBadgeChrome,
}

#[derive(Clone, Copy)]
pub enum PrefixIconBadgeChrome {
    None,
    Outline,
}

pub fn layout_prefix_icon_badge(
    badge: PrefixIconBadge<'_>,
    text_system: &mut TextSystem,
    atlas: &mut GlyphAtlas,
    queue: &wgpu::Queue,
    chrome: &mut Vec<RegionDrawInstance>,
) -> Vec<GlyphInstance> {
    let [x, y, w, h] = badge.bounds;
    let icon_color = match badge.chrome {
        PrefixIconBadgeChrome::None => badge.color,
        PrefixIconBadgeChrome::Outline => blend_icon_badge_color(badge.panel_bg, badge.color, 0.78),
    };

    if matches!(badge.chrome, PrefixIconBadgeChrome::Outline) {
        let radius = h * 0.22;
        let border = (h * 0.075).clamp(2.0, 3.0);
        let border_color = blend_icon_badge_color(badge.panel_bg, badge.color, 0.86);
        let bg_color = blend_icon_badge_color(badge.panel_bg, badge.color, 0.10);

        chrome.push(RegionDrawInstance::new([x, y, w, h], border_color).with_radius(radius));
        chrome.push(
            RegionDrawInstance::new(
                [
                    x + border,
                    y + border,
                    (w - border * 2.0).max(1.0),
                    (h - border * 2.0).max(1.0),
                ],
                bg_color,
            )
            .with_radius((radius - border).max(2.0)),
        );
    }

    let icon_size = (h * badge.icon_scale).max(10.0);
    let icon_w = estimate_monospace_width(badge.icon, icon_size);
    let icon_line_h = icon_size * 1.15;
    let icon_x = x + (w - icon_w) * 0.5;
    let icon_y = y + (h - icon_line_h) * 0.5 + icon_size * badge.y_nudge_scale;
    text_system.set_metrics(cosmic_text::Metrics::new(icon_size, icon_line_h));
    text_system.set_size(Some(w), Some(icon_line_h));
    layout_panel_text_bold(
        badge.icon,
        text_system,
        atlas,
        queue,
        icon_x,
        icon_y,
        icon_color,
    )
}

fn blend_icon_badge_color(base: [f32; 4], tint: [f32; 4], amount: f32) -> [f32; 4] {
    let t = amount.clamp(0.0, 1.0);
    [
        base[0] * (1.0 - t) + tint[0] * t,
        base[1] * (1.0 - t) + tint[1] * t,
        base[2] * (1.0 - t) + tint[2] * t,
        1.0,
    ]
}
