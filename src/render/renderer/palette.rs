//! Overlay rendering: Command Palette, File Picker, Recent Projects, Leap labels.

mod file_picker;
mod highlighted_label;
mod leap;
mod live_grep;
mod minimal;
mod recent_projects;

use crate::{
    app::command_palette::CommandPaletteRenderModel,
    render::{glyph_instance::GlyphInstance, region_pipeline::RegionDrawInstance, renderer::Renderer},
};

use super::helpers::{estimate_monospace_width, layout_panel_text};
use super::components::{estimate_help_keycaps_width, layout_help_keycaps};

pub(super) const PALETTE_FRAME_RADIUS: f32 = 16.0;
pub(super) const PALETTE_PANEL_RADIUS: f32 = 15.0;
pub(super) const PALETTE_ROW_RADIUS: f32 = 10.0;
pub(super) const PALETTE_ACCENT_STRIP_W: f32 = 5.0;
pub(super) const PALETTE_HEADER_BOTTOM_PAD: f32 = 12.0;
pub(super) const PALETTE_FOOTER_EXTRA_H: f32 = 36.0;
pub(super) const PALETTE_FOOTER_TOP_PAD: f32 = 10.0;
pub(super) const PALETTE_FOOTER_BOTTOM_PAD: f32 = 18.0;

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
    if model.mode == crate::app::command_palette::CommandPaletteMode::ThemeSelector {
        frame_border[3] = frame_border[3].clamp(0.18, 0.42);
    } else {
        frame_border[3] = frame_border[3].max(0.95);
    }
    chrome.push(
        RegionDrawInstance::new(
            [panel_x - 1.0, panel_y - 1.0, panel_w + 2.0, panel_h + 2.0],
            frame_border,
        )
        .with_radius(PALETTE_FRAME_RADIUS),
    );
    chrome.push(RegionDrawInstance::new(model.panel_bounds, model.panel_bg).with_radius(PALETTE_PANEL_RADIUS));
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
    let badge_text = format!(" {} ", title);
    let badge_h = (line_height + 10.0).max(30.0);
    let badge_w = badge_text.chars().count() as f32 * font_size * 0.60 + 22.0;
    chrome.push(
        RegionDrawInstance::new([x, y, badge_w, badge_h], fill)
            .with_radius((badge_h * 0.18).max(7.0)),
    );
    let glyphs = layout_panel_text(
        &badge_text,
        text_system,
        atlas,
        queue,
        x + 10.0,
        y + ((badge_h - line_height) * 0.5).max(0.0),
        text_color,
    );
    (badge_w, badge_h, glyphs)
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
    pub fn update_palette_content(&mut self, model: &CommandPaletteRenderModel) {
        if self.last_palette_model.as_ref() == Some(model) {
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
        }
        self.palette_text_pipeline.upload_instances(
            &self.device,
            &self.queue,
            &self.palette_glyph_instances,
        );
        self.last_palette_model = Some(model.clone());
    }

    pub fn clear_palette(&mut self) {
        self.palette_scissor = None;
        self.palette_chrome_instances.clear();
        self.palette_glyph_instances.clear();
        self.last_palette_model = None;
        self.palette_text_pipeline
            .upload_instances(&self.device, &self.queue, &[]);
    }
}
