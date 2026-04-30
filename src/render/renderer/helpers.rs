//! Pure free-standing helper functions shared across renderer submodules.
//!
//! Nothing in here depends on `Renderer` state — callers pass in the
//! exact resources they need so the functions remain testable in isolation.

use crate::{
    config::{
        theme_config::{ThemeColor, ThemeConfig, linear_rgba_to_srgb_u8},
        ui_config::CursorShape,
    },
    core::mode::EditorMode,
    render::{caret::CaretScreenRect, glyph_instance::GlyphInstance},
    text::{
        atlas::GlyphAtlas, raster::rasterize_glyph_alpha, text_system::StyledTextSpan,
        text_system::TextSystem,
    },
};

// ── Scissor ────────────────────────────────────────────────────────────────────

pub(super) fn rect_to_scissor(bounds: [f32; 4]) -> Option<[u32; 4]> {
    let sx = bounds[0].max(0.0).round() as u32;
    let sy = bounds[1].max(0.0).round() as u32;
    let sw = bounds[2].max(0.0).round() as u32;
    let sh = bounds[3].max(0.0).round() as u32;
    (sw > 0 && sh > 0).then_some([sx, sy, sw, sh])
}

// ── Text layout ───────────────────────────────────────────────────────────────

/// Lay out `text` into `GlyphInstance`s.
/// Shares the single `GlyphAtlas` with the editor region.
pub(super) fn layout_panel_text(
    text: &str,
    text_system: &mut TextSystem,
    atlas: &mut GlyphAtlas,
    queue: &wgpu::Queue,
    origin_x: f32,
    origin_y: f32,
    color: [f32; 4],
) -> Vec<GlyphInstance> {
    let color_u8 = color_f32_to_u8(color);
    text_system.set_text_with_color(text, color_u8);
    collect_instances(text_system, atlas, queue, origin_x, origin_y, color)
}

pub(super) fn layout_panel_rich_text(
    text: &str,
    spans: &[StyledTextSpan],
    default_color: [f32; 4],
    text_system: &mut TextSystem,
    atlas: &mut GlyphAtlas,
    queue: &wgpu::Queue,
    origin_x: f32,
    origin_y: f32,
) -> Vec<GlyphInstance> {
    let color_u8 = color_f32_to_u8(default_color);
    text_system.set_text_with_spans(text, color_u8, spans);
    collect_instances(text_system, atlas, queue, origin_x, origin_y, default_color)
}

/// Like `layout_panel_text` but rasterizes **bold** — for Leap label overlay.
pub(super) fn layout_panel_text_bold(
    text: &str,
    text_system: &mut TextSystem,
    atlas: &mut GlyphAtlas,
    queue: &wgpu::Queue,
    origin_x: f32,
    origin_y: f32,
    color: [f32; 4],
) -> Vec<GlyphInstance> {
    let color_u8 = color_f32_to_u8(color);
    text_system.set_text_bold_color(text, color_u8);
    collect_instances(text_system, atlas, queue, origin_x, origin_y, color)
}

pub(super) fn layout_panel_text_italic(
    text: &str,
    text_system: &mut TextSystem,
    atlas: &mut GlyphAtlas,
    queue: &wgpu::Queue,
    origin_x: f32,
    origin_y: f32,
    color: [f32; 4],
) -> Vec<GlyphInstance> {
    let color_u8 = color_f32_to_u8(color);
    text_system.set_text_italic_color(text, color_u8);
    collect_instances(text_system, atlas, queue, origin_x, origin_y, color)
}

fn color_f32_to_u8(color: [f32; 4]) -> [u8; 4] {
    linear_rgba_to_srgb_u8(color)
}

fn collect_instances(
    text_system: &mut TextSystem,
    atlas: &mut GlyphAtlas,
    queue: &wgpu::Queue,
    origin_x: f32,
    origin_y: f32,
    color: [f32; 4],
) -> Vec<GlyphInstance> {
    let visible = text_system.collect_visible_glyphs(origin_x, origin_y, color, None);
    let mut instances = Vec::with_capacity(visible.len());

    for glyph in visible {
        let entry = if let Some(e) = atlas.get(glyph.cache_key) {
            e
        } else {
            let Some(rasterized) = rasterize_glyph_alpha(text_system, glyph.cache_key) else {
                continue;
            };
            match atlas.get_or_reserve(glyph.cache_key, &rasterized) {
                Ok(e) => e,
                Err(_) => continue,
            }
        };

        if entry.region.width == 0 || entry.region.height == 0 {
            continue;
        }

        let (uv_min, uv_max) = atlas.uv_min_max(entry.region);
        let tl_x = glyph.physical_x + entry.placement_left;
        let tl_y = glyph.physical_y - entry.placement_top;
        instances.push(GlyphInstance::new(
            [tl_x as f32, tl_y as f32],
            [entry.region.width as f32, entry.region.height as f32],
            uv_min,
            uv_max,
            glyph.color,
        ));
    }

    atlas.flush_pending(queue);
    instances
}

// ── GPU draw helper ────────────────────────────────────────────────────────────

/// Draw text content clipped to `scissor`, then reset to full viewport.
pub(super) fn draw_text_region<'a, F>(
    pass: &mut wgpu::RenderPass<'a>,
    scissor: Option<[u32; 4]>,
    vp_w: u32,
    vp_h: u32,
    draw_fn: F,
) where
    F: FnOnce(&mut wgpu::RenderPass<'a>),
{
    if let Some([sx, sy, sw, sh]) = scissor {
        let cx = sx.min(vp_w);
        let cy = sy.min(vp_h);
        let cw = sw.min(vp_w.saturating_sub(cx));
        let ch = sh.min(vp_h.saturating_sub(cy));
        if cw > 0 && ch > 0 {
            pass.set_scissor_rect(cx, cy, cw, ch);
            draw_fn(pass);
            pass.set_scissor_rect(0, 0, vp_w, vp_h);
        }
    } else {
        draw_fn(pass);
    }
}

// ── Monospace width utilities ──────────────────────────────────────────────────

pub(super) fn estimate_monospace_width(text: &str, font_size: f32) -> f32 {
    text.chars().count() as f32 * font_size * 0.6
}

pub(super) fn clamp_monospace_text(text: &str, max_width: f32, font_size: f32) -> String {
    if text.is_empty() || max_width <= 0.0 {
        return String::new();
    }
    if estimate_monospace_width(text, font_size) <= max_width {
        return text.to_string();
    }
    let char_width = (font_size * 0.6).max(1.0);
    let max_chars = (max_width / char_width).floor() as usize;
    if max_chars == 0 {
        return String::new();
    }
    if text.chars().count() <= max_chars {
        return text.to_string();
    }
    if max_chars <= 3 {
        return text.chars().take(max_chars).collect();
    }
    let mut shortened: String = text.chars().take(max_chars - 3).collect();
    shortened.push_str("...");
    shortened
}

pub(super) fn gutter_width_for_editor(
    gutter_digits: usize,
    font_size: f32,
    line_height: f32,
) -> f32 {
    let gutter_char_w = (font_size + 3.0).min(line_height - 2.0).max(8.0) * 0.6;
    gutter_digits as f32 * gutter_char_w + 18.0
}

pub(super) fn clamp_popup_width(
    desired_width: f32,
    preferred_min_width: f32,
    available_width: f32,
) -> f32 {
    let available_width = if available_width.is_nan() {
        1.0
    } else {
        available_width.max(1.0)
    };
    let preferred_min_width = if preferred_min_width.is_nan() {
        1.0
    } else {
        preferred_min_width.min(available_width).max(1.0)
    };
    let desired_width = if desired_width.is_nan() {
        preferred_min_width
    } else {
        desired_width
    };

    desired_width.clamp(preferred_min_width, available_width)
}

pub(super) fn clamp_x_in_bounds(x: f32, min_x: f32, max_x: f32) -> f32 {
    let min_x = if min_x.is_nan() { 0.0 } else { min_x };
    let max_x = if max_x.is_nan() {
        min_x
    } else {
        max_x.max(min_x)
    };
    let x = if x.is_nan() { min_x } else { x };
    x.clamp(min_x, max_x)
}

// ── Caret geometry ────────────────────────────────────────────────────────────

/// Build the caret rect for the current mode.
///
/// Normal/Visual → uniform block caret sized from font metrics (not the
///                 glyph under the cursor) so width stays constant even
///                 when cursor moves across narrow/wide chars.
/// Insert and other modes → thin vertical bar.
pub(super) fn caret_rect_for_mode(
    layout: crate::text::layout_sync::CaretLayout,
    mode: EditorMode,
    color: [f32; 4],
    font_size: f32,
    cursor_shape: CursorShape,
    beam_width: f32,
    block_width: f32,
    underline_height: f32,
) -> CaretScreenRect {
    let (x, y, width, height) = match cursor_shape {
        CursorShape::Beam => (
            layout.x,
            layout.top,
            beam_width.max(1.0),
            layout.height.max(1.0),
        ),
        CursorShape::Underline => {
            let h = underline_height.max(1.0);
            (
                layout.x,
                layout.top + (layout.height - h).max(0.0),
                block_width.max(font_size * 0.6).max(1.0),
                h,
            )
        }
        CursorShape::Block => {
            if is_mode_block_cursor(mode) {
                (
                    layout.x,
                    layout.top,
                    block_width.max(font_size * 0.6).max(1.0),
                    layout.height.max(1.0),
                )
            } else {
                (
                    layout.x,
                    layout.top,
                    beam_width.max(1.0),
                    layout.height.max(1.0),
                )
            }
        }
    };
    CaretScreenRect {
        x,
        y,
        width,
        height,
        color,
    }
}

#[cfg(test)]
mod tests {
    use super::{clamp_popup_width, clamp_x_in_bounds};

    #[test]
    fn clamp_popup_width_saturates_to_available_width_when_viewport_is_narrow() {
        let popup_w = clamp_popup_width(324.0, 160.0, 80.7749);
        assert!((popup_w - 80.7749).abs() < 0.001);
    }

    #[test]
    fn clamp_popup_width_handles_nan_inputs() {
        assert_eq!(clamp_popup_width(f32::NAN, 160.0, 80.0), 80.0);
        assert_eq!(clamp_popup_width(120.0, f32::NAN, 80.0), 80.0);
        assert_eq!(clamp_popup_width(120.0, 160.0, f32::NAN), 1.0);
    }

    #[test]
    fn clamp_x_in_bounds_normalizes_inverted_and_nan_ranges() {
        assert_eq!(clamp_x_in_bounds(150.0, 110.525, 80.7749), 110.525);
        assert_eq!(clamp_x_in_bounds(f32::NAN, 20.0, 80.0), 20.0);
        assert_eq!(clamp_x_in_bounds(50.0, f32::NAN, 80.0), 50.0);
    }
}

pub(super) fn is_mode_block_cursor(mode: EditorMode) -> bool {
    matches!(mode, EditorMode::Normal | EditorMode::Visual)
}

pub(super) fn should_draw_block_cursor(mode: EditorMode, cursor_shape: CursorShape) -> bool {
    matches!(cursor_shape, CursorShape::Block) && is_mode_block_cursor(mode)
}

// ── Status bar labels ─────────────────────────────────────────────────────────

pub(super) fn mode_display_label(mode: EditorMode) -> &'static str {
    match mode {
        EditorMode::Normal => "NORMAL",
        EditorMode::Insert => "INSERT",
        EditorMode::Visual => "VISUAL",
        EditorMode::PaletteFocus => "PALETTE",
        EditorMode::TerminalFocus => "TERMINAL",
        EditorMode::TerminalNormal => "T-COPY",
    }
}

pub(super) fn mode_pill_color(mode: EditorMode, theme: &ThemeConfig) -> [f32; 4] {
    match mode {
        EditorMode::Normal => theme.ui.mode_normal.as_f32(),
        EditorMode::Insert => theme.ui.mode_insert.as_f32(),
        EditorMode::Visual => theme.ui.mode_visual.as_f32(),
        EditorMode::PaletteFocus => theme.ui.amber.as_f32(),
        EditorMode::TerminalFocus => theme.ui.success.as_f32(),
        EditorMode::TerminalNormal => theme.ui.accent.as_f32(),
    }
}

pub(super) fn theme_color_to_wgpu(color: ThemeColor) -> wgpu::Color {
    color.as_linear().to_wgpu()
}

// ── File picker icon helpers ───────────────────────────────────────────────────

/// (dot_color, ext_label) for `●` icon + short ext text in File Picker.
pub(super) fn ext_icon_dot(ext: &str, theme: &ThemeConfig) -> ([f32; 4], &'static str) {
    let icon = theme.file_icon_for_extension(ext);
    match ext {
        "rs" => (icon.color.as_f32(), "rs"),
        "js" | "mjs" | "cjs" => (icon.color.as_f32(), "js"),
        "ts" => (icon.color.as_f32(), "ts"),
        "tsx" => (icon.color.as_f32(), "tsx"),
        "jsx" => (icon.color.as_f32(), "jsx"),
        "py" | "pyw" => (icon.color.as_f32(), "py"),
        "go" => (icon.color.as_f32(), "go"),
        "md" | "mdx" | "markdown" => (icon.color.as_f32(), "md"),
        "html" | "htm" => (icon.color.as_f32(), "html"),
        "css" => (icon.color.as_f32(), "css"),
        "scss" | "sass" => (icon.color.as_f32(), "scss"),
        "toml" => (icon.color.as_f32(), "toml"),
        "yaml" | "yml" => (icon.color.as_f32(), "yaml"),
        "json" | "jsonc" => (icon.color.as_f32(), "json"),
        "sh" | "bash" | "zsh" | "fish" => (icon.color.as_f32(), "sh"),
        "lock" => (icon.color.as_f32(), "lock"),
        "env" => (icon.color.as_f32(), "env"),
        "png" | "jpg" | "jpeg" | "gif" | "svg" | "webp" | "ico" => (icon.color.as_f32(), "img"),
        _ => (icon.color.as_f32(), ""),
    }
}
