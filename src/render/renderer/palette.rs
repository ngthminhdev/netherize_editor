//! Overlay rendering: Command Palette, File Picker, Recent Projects, Leap labels.

mod file_picker;
mod highlighted_label;
mod leap;
mod live_grep;
mod minimal;
mod recent_projects;

use crate::{
    app::command_palette::CommandPaletteRenderModel,
    render::{
        glyph_instance::GlyphInstance,
        icon_pipeline::{IconDrawInstance, canonical_icon_id},
        region_pipeline::RegionDrawInstance,
        renderer::Renderer,
    },
};

use super::components::{estimate_help_keycaps_width, layout_help_keycaps};
use super::helpers::{estimate_monospace_width, layout_panel_text, layout_panel_text_bold};

pub(super) const PALETTE_FRAME_RADIUS: f32 = 16.0;
pub(super) const PALETTE_PANEL_RADIUS: f32 = 15.0;
pub(super) const PALETTE_ROW_RADIUS: f32 = 10.0;
pub(super) const PALETTE_ACCENT_STRIP_W: f32 = 5.0;
pub(super) const PALETTE_HEADER_BOTTOM_PAD: f32 = 12.0;
pub(super) const PALETTE_FOOTER_EXTRA_H: f32 = 36.0;
pub(super) const PALETTE_FOOTER_TOP_PAD: f32 = 10.0;
pub(super) const PALETTE_FOOTER_BOTTOM_PAD: f32 = 18.0;

/// Smallest scroll offset (in result rows) that keeps `selected_index`
/// rendered inside a body of `max_slots` uniform row slots, where entering a
/// new group draws a header that consumes one slot (the first visible row
/// always opens its group). This is the renderers' own ground truth — the
/// palette model's `scroll_offset_rows` ignores group headers, which made
/// Ctrl-N walk the selection off-screen for 1–2 presses before scrolling.
/// `group_of` returns a row's group key, or None for ungrouped modes.
/// ponytail: rows hidden in collapsed groups still cost a slot, so collapsed
/// lists may scroll a touch early — never late (the reported bug).
pub(super) fn palette_scroll_offset(
    selected_index: usize,
    result_count: usize,
    max_slots: usize,
    mut group_of: impl FnMut(usize) -> Option<String>,
) -> usize {
    if result_count == 0 || max_slots == 0 {
        return 0;
    }
    let selected = selected_index.min(result_count - 1);
    let mut offset = selected;
    let mut slots = 1 + usize::from(group_of(selected).is_some());
    while offset > 0 {
        let prev = offset - 1;
        let prev_group = group_of(prev);
        // Extending the window upward costs the row itself, plus one header
        // slot when `prev` sits in a different group than the current start.
        let cost = 1 + usize::from(prev_group.is_some() && prev_group != group_of(offset));
        if slots + cost > max_slots {
            break;
        }
        slots += cost;
        offset = prev;
    }
    offset
}

pub(super) fn palette_footer_height(line_height: f32) -> f32 {
    line_height + PALETTE_FOOTER_EXTRA_H + 1.0
}

pub(super) fn palette_footer_content_height(footer_height: f32) -> f32 {
    (footer_height - PALETTE_FOOTER_TOP_PAD - PALETTE_FOOTER_BOTTOM_PAD).max(1.0)
}

pub(super) fn render_palette_chrome(
    model: &CommandPaletteRenderModel,
    chrome: &mut Vec<RegionDrawInstance>,
) {
    let [panel_x, panel_y, panel_w, panel_h] = model.panel_bounds;
    chrome.push(RegionDrawInstance::new(
        model.overlay_bounds,
        model.scrim_color,
    ));
    let mut frame_border = model.border_color;
    frame_border[3] = frame_border[3].max(0.95);
    chrome.push(
        RegionDrawInstance::new(
            [panel_x - 1.0, panel_y - 1.0, panel_w + 2.0, panel_h + 2.0],
            frame_border,
        )
        .with_radius(PALETTE_FRAME_RADIUS),
    );
    chrome.push(
        RegionDrawInstance::new(model.panel_bounds, model.panel_bg)
            .with_radius(PALETTE_PANEL_RADIUS),
    );
}

pub(super) fn render_palette_badge(
    title: &str,
    text_system: &mut crate::text::text_system::TextSystem,
    atlas: &mut crate::text::atlas::GlyphAtlas,
    queue: &wgpu::Queue,
    chrome: &mut Vec<RegionDrawInstance>,
    x: f32,
    y: f32,
    font_size: f32,
    line_height: f32,
    fill: [f32; 4],
    text_color: [f32; 4],
) -> (f32, f32, Vec<GlyphInstance>) {
    let badge_text = title.to_string();
    let badge_h = (line_height + 10.0).max(30.0);
    let text_w = estimate_monospace_width(&badge_text, font_size);
    let badge_w = text_w + 44.0;
    let badge_radius = (badge_h * 0.18).max(7.0);
    let border_thickness = (badge_h * 0.075).clamp(2.0, 3.0);
    let badge_fill = blend_palette_badge_color(text_color, fill, 0.10);
    let badge_border = blend_palette_badge_color(text_color, fill, 0.86);
    let badge_text_color = blend_palette_badge_color(text_color, fill, 0.78);

    chrome.push(
        RegionDrawInstance::new([x, y, badge_w, badge_h], badge_border).with_radius(badge_radius),
    );
    chrome.push(
        RegionDrawInstance::new(
            [
                x + border_thickness,
                y + border_thickness,
                (badge_w - border_thickness * 2.0).max(1.0),
                (badge_h - border_thickness * 2.0).max(1.0),
            ],
            badge_fill,
        )
        .with_radius((badge_radius - border_thickness).max(2.0)),
    );
    let glyphs = layout_panel_text_bold(
        &badge_text,
        text_system,
        atlas,
        queue,
        x + (badge_w - text_w) * 0.5,
        y + ((badge_h - line_height) * 0.5).max(0.0),
        badge_text_color,
    );
    (badge_w, badge_h, glyphs)
}

pub(super) fn push_palette_icon_or_badge(
    icon: &str,
    color: [f32; 4],
    panel_bg: [f32; 4],
    bounds: [f32; 4],
    icon_scale: f32,
    chrome_style: super::components::PrefixIconBadgeChrome,
    text_system: &mut crate::text::text_system::TextSystem,
    atlas: &mut crate::text::atlas::GlyphAtlas,
    queue: &wgpu::Queue,
    chrome: &mut Vec<RegionDrawInstance>,
    glyphs: &mut Vec<GlyphInstance>,
    icons: &mut Vec<IconDrawInstance>,
) {
    if let Some(asset_icon) = canonical_icon_id(icon) {
        if matches!(
            chrome_style,
            super::components::PrefixIconBadgeChrome::Outline
        ) {
            let [x, y, w, h] = bounds;
            let radius = h * 0.22;
            let border = (h * 0.075).clamp(2.0, 3.0);
            let border_color = blend_palette_badge_color(panel_bg, color, 0.86);
            let bg_color = blend_palette_badge_color(panel_bg, color, 0.10);
            chrome.push(RegionDrawInstance::new(bounds, border_color).with_radius(radius));
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
        let [x, y, w, h] = bounds;
        let size = (h * icon_scale).max(10.0);
        icons.push(IconDrawInstance {
            icon: asset_icon,
            rect: [x + (w - size) * 0.5, y + (h - size) * 0.5, size, size],
            tint: color,
        });
    } else {
        glyphs.extend(super::components::layout_prefix_icon_badge(
            super::components::PrefixIconBadge {
                icon,
                color,
                panel_bg,
                bounds,
                icon_scale,
                y_nudge_scale: 0.10,
                chrome: chrome_style,
            },
            text_system,
            atlas,
            queue,
            chrome,
        ));
    }
}

fn blend_palette_badge_color(base: [f32; 4], tint: [f32; 4], amount: f32) -> [f32; 4] {
    let t = amount.clamp(0.0, 1.0);
    [
        base[0] * (1.0 - t) + tint[0] * t,
        base[1] * (1.0 - t) + tint[1] * t,
        base[2] * (1.0 - t) + tint[2] * t,
        1.0,
    ]
}

pub(super) fn render_palette_selection(
    model: &CommandPaletteRenderModel,
    chrome: &mut Vec<RegionDrawInstance>,
    row_top: f32,
    row_height: f32,
) {
    let [panel_x, _, panel_w, _] = model.panel_bounds;
    let mut selection_bg = model.selection_bg;
    selection_bg[3] = (selection_bg[3] * 0.88).clamp(0.18, 0.72);
    chrome.push(
        RegionDrawInstance::new(
            [panel_x + 2.0, row_top, (panel_w - 4.0).max(0.0), row_height],
            selection_bg,
        )
        .with_radius(PALETTE_ROW_RADIUS),
    );
    chrome.push(
        RegionDrawInstance::new(
            [panel_x + 2.0, row_top, PALETTE_ACCENT_STRIP_W, row_height],
            model.match_color,
        )
        .with_radius(3.0),
    );
}

pub(super) struct PaletteFooterAction<'a> {
    pub keys: &'a [&'a str],
    pub label: &'a str,
}

pub(super) fn render_palette_footer(
    model: &CommandPaletteRenderModel,
    text_system: &mut crate::text::text_system::TextSystem,
    atlas: &mut crate::text::atlas::GlyphAtlas,
    queue: &wgpu::Queue,
    chrome: &mut Vec<RegionDrawInstance>,
    origin_x: f32,
    origin_y: f32,
    font_size: f32,
    row_height: f32,
    actions: &[PaletteFooterAction<'_>],
) -> Vec<GlyphInstance> {
    let mut glyphs = Vec::new();
    let mut cursor_x = origin_x;
    let action_gap = (font_size * 1.85).max(24.0);
    let label_gap = (font_size * 0.56).max(8.0);
    let key_font_size = (font_size * 0.82).max(10.0);

    for action in actions {
        glyphs.extend(layout_help_keycaps(
            action.keys,
            text_system,
            atlas,
            queue,
            chrome,
            cursor_x,
            origin_y,
            key_font_size,
            row_height,
            model.text_color,
            model.label_color,
            model.match_color,
            model.info_color,
            model.warning_color,
            model.warning_color,
            model.panel_bg,
        ));
        let keys_w = estimate_help_keycaps_width(action.keys, key_font_size);
        cursor_x += keys_w + label_gap;
        glyphs.extend(layout_panel_text(
            action.label,
            text_system,
            atlas,
            queue,
            cursor_x,
            origin_y + ((row_height - model.line_height) * 0.5).max(0.0),
            model.hint_color,
        ));
        cursor_x += estimate_monospace_width(action.label, font_size) + action_gap;
    }

    glyphs
}

impl Renderer {
    // ── Public entry point ─────────────────────────────────────────────────────

    /// Layout + upload command palette overlay (chrome + text).
    ///
    /// The palette model is precomputed by `CommandPalette::render()` so the
    /// renderer only consumes geometry/text and uploads GPU instances.
    pub fn update_palette_content(
        &mut self,
        model: &CommandPaletteRenderModel,
        reveal: Option<crate::workbench::motion::RevealSample>,
    ) {
        if self.last_palette_model.as_ref() == Some(model) && self.last_palette_sample == reveal {
            return;
        }
        if matches!(
            model.mode,
            crate::app::command_palette::CommandPaletteMode::RecentProjects
                | crate::app::command_palette::CommandPaletteMode::ThemeSelector
        ) {
            self.render_recent_projects(model);
        } else if model.mode == crate::app::command_palette::CommandPaletteMode::LiveGrep {
            self.render_live_grep_picker(model);
        } else if model.mode.is_complex_picker() {
            self.render_file_picker_complex(model);
        } else {
            self.render_command_palette_minimalist(model);
            self.palette_icon_instances.clear();
            self.palette_icon_pipeline.upload_instances(
                &self.device,
                &self.palette_icon_instances,
                [
                    self.surface_state.config.width,
                    self.surface_state.config.height,
                ],
            );
        }
        // Dot → Line → Panel Reveal motion, applied to the freshly built
        // instances before upload. The frame/panel-bg quads are collapsed to a
        // growing reveal rect (dot → line → panel); the scrim only fades; all
        // content (rows, badges, glyphs, icons) is held hidden until the panel
        // is nearly open, then faded in.
        if let Some(r) = reveal {
            self.apply_palette_reveal(model, r);
            // Icons were uploaded at identity inside the build branches above;
            // re-upload the transformed set so the reveal fade applies to them.
            self.palette_icon_pipeline.upload_instances(
                &self.device,
                &self.palette_icon_instances,
                [
                    self.surface_state.config.width,
                    self.surface_state.config.height,
                ],
            );
        }
        self.palette_text_pipeline.upload_instances(
            &self.device,
            &self.queue,
            &self.palette_glyph_instances,
        );
        self.last_palette_model = Some(model.clone());
        self.last_palette_sample = reveal;
    }

    /// Apply a [`RevealSample`](crate::workbench::motion::RevealSample) to the
    /// already-built palette instances. The frame and panel-background quads are
    /// replaced with a centered reveal rect that grows horizontally (dot → line)
    /// then vertically (line → panel); the full-screen scrim only fades; every
    /// other quad plus all glyphs/icons fade in via `content_alpha`.
    fn apply_palette_reveal(
        &mut self,
        model: &CommandPaletteRenderModel,
        r: crate::workbench::motion::RevealSample,
    ) {
        let [px, py, pw, ph] = model.panel_bounds;
        let cx = px + pw * 0.5;
        let cy = py + ph * 0.5;
        let scrim = model.overlay_bounds;
        // render_palette_chrome pushes the frame border as panel_bounds grown 1px.
        let frame = [px - 1.0, py - 1.0, pw + 2.0, ph + 2.0];

        // The dot/line thickness — also the minimum width so frame 0 reads as a dot.
        let line_px = 3.0_f32;
        let rw = (pw * r.width_factor).max(line_px);
        let rh = line_px + (ph - line_px) * r.height_factor;
        let reveal = [cx - rw * 0.5, cy - rh * 0.5, rw, rh];
        let reveal_frame = [
            reveal[0] - 1.0,
            reveal[1] - 1.0,
            reveal[2] + 2.0,
            reveal[3] + 2.0,
        ];
        // Clamp corner radius so the thin line renders as a clean pill, not an
        // over-rounded blob, at every size.
        let radius_of = |base: f32, rect: [f32; 4]| base.min(rect[2].min(rect[3]) * 0.5);

        for inst in &mut self.palette_chrome_instances {
            if inst.rect == scrim {
                inst.color[3] *= r.scrim_alpha;
            } else if inst.rect == frame {
                inst.rect = reveal_frame;
                let radius = radius_of(PALETTE_FRAME_RADIUS, reveal_frame);
                inst.border_radius = radius;
                inst.corner_radii = [radius; 4];
            } else if inst.rect == model.panel_bounds {
                inst.rect = reveal;
                let radius = radius_of(PALETTE_PANEL_RADIUS, reveal);
                inst.border_radius = radius;
                inst.corner_radii = [radius; 4];
            } else {
                // Content chrome (rows, badges, dividers, footer) fades in last.
                inst.color[3] *= r.content_alpha;
            }
        }
        for g in &mut self.palette_glyph_instances {
            g.color[3] *= r.content_alpha;
        }
        for ic in &mut self.palette_icon_instances {
            ic.tint[3] *= r.content_alpha;
        }
    }

    pub fn clear_palette(&mut self) {
        self.palette_scissor = None;
        self.palette_chrome_instances.clear();
        self.palette_glyph_instances.clear();
        self.palette_icon_instances.clear();
        self.last_palette_model = None;
        self.palette_icon_pipeline.upload_instances(
            &self.device,
            &self.palette_icon_instances,
            [
                self.surface_state.config.width,
                self.surface_state.config.height,
            ],
        );
        self.palette_text_pipeline
            .upload_instances(&self.device, &self.queue, &[]);
    }
}

#[cfg(test)]
mod scroll_offset_tests {
    use super::palette_scroll_offset;

    #[test]
    fn ungrouped_matches_selected_pinned_to_bottom() {
        // Nothing scrolls while the selection fits the window.
        assert_eq!(palette_scroll_offset(0, 50, 10, |_| None), 0);
        assert_eq!(palette_scroll_offset(9, 50, 10, |_| None), 0);
        // One past the window scrolls exactly one row — no dead presses.
        assert_eq!(palette_scroll_offset(10, 50, 10, |_| None), 1);
        assert_eq!(palette_scroll_offset(49, 50, 10, |_| None), 40);
    }

    #[test]
    fn group_headers_consume_visible_slots() {
        // 50 rows, 5 rows per group, 10 slots per window. The window always
        // opens with the header of its first row's group, and each group
        // boundary inside the window draws another header.
        let group = |idx: usize| Some(format!("g{}", idx / 5));
        // Rows 0..=7 + 2 headers fill all 10 slots exactly.
        assert_eq!(palette_scroll_offset(7, 50, 10, group), 0);
        // Row 8 no longer fits — must scroll NOW, not two presses later.
        assert_eq!(palette_scroll_offset(8, 50, 10, group), 1);
        assert_eq!(palette_scroll_offset(9, 50, 10, group), 2);
    }

    #[test]
    fn empty_and_degenerate_inputs_do_not_scroll() {
        assert_eq!(palette_scroll_offset(3, 0, 10, |_| None), 0);
        assert_eq!(palette_scroll_offset(3, 10, 0, |_| None), 0);
        // Out-of-range selection is clamped into the list.
        assert_eq!(palette_scroll_offset(99, 5, 10, |_| None), 0);
    }
}
