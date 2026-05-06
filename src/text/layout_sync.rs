use crate::{
    app::app_state::AppState,
    config::theme_config::linear_rgba_to_srgb_u8,
    render::glyph_instance::GlyphInstance,
    text::{
        atlas::GlyphAtlas,
        raster::rasterize_glyph_alpha,
        text_system::{StyledTextSpan, TextSystem},
    },
};

#[derive(Debug, Clone, Copy)]
pub struct CaretLayout {
    pub x: f32,
    pub top: f32,
    pub height: f32,
    /// Width of the glyph under the cursor. Used for block-style caret
    /// (Normal/Visual mode). Falls back to a single space advance when the
    /// cursor sits past the end of line.
    pub glyph_width: f32,
}

/// Dữ liệu projection từ buffer sang render text/caret.
#[derive(Debug, Clone)]
pub struct LayoutProjection {
    pub glyph_instances: Vec<GlyphInstance>,
    /// When the caller requests a block-style cursor, this is the same glyph
    /// that sits at the cursor position but re-colored so it stays readable
    /// on top of the caret. None when the cursor is past end-of-line or the
    /// block style isn't active.
    pub cursor_overlay: Option<GlyphInstance>,
    pub caret_layout: CaretLayout,
}

pub fn rebuild_layout_projection(
    text: &str,
    app_state: &AppState,
    text_system: &mut TextSystem,
    atlas: &mut GlyphAtlas,
    queue: &wgpu::Queue,
    viewport_origin: [f32; 2],
    viewport_clip: Option<(f32, f32)>,
    text_color: [f32; 4],
    cursor_overlay_color: [f32; 4],
    styled_spans: &[StyledTextSpan],
) -> Result<LayoutProjection, String> {
    let default_color_rgba = color_f32_to_u8(text_color);
    text_system.set_text_with_spans(text, default_color_rgba, styled_spans);

    let (cursor_line, _) = app_state.cursor_line_col();
    let cursor_byte_in_line = app_state.cursor_byte_in_line();

    let visible_glyphs = text_system.collect_visible_glyphs(
        viewport_origin[0],
        viewport_origin[1],
        text_color,
        viewport_clip,
    );
    let mut glyph_instances = Vec::with_capacity(visible_glyphs.len());
    let mut cursor_overlay: Option<GlyphInstance> = None;

    for glyph in visible_glyphs {
        let entry = if let Some(entry) = atlas.get(glyph.cache_key) {
            entry
        } else {
            let Some(rasterized) = rasterize_glyph_alpha(text_system, glyph.cache_key) else {
                continue;
            };

            atlas.get_or_reserve(glyph.cache_key, &rasterized)?
        };

        if entry.region.width == 0 || entry.region.height == 0 {
            continue;
        }

        let (uv_min, uv_max) = atlas.uv_min_max(entry.region);
        let top_left_x = glyph.physical_x + entry.placement_left;
        let top_left_y = glyph.physical_y - entry.placement_top;

        let position = [top_left_x as f32, top_left_y as f32];
        let size = [entry.region.width as f32, entry.region.height as f32];

        glyph_instances.push(GlyphInstance::new(
            position,
            size,
            uv_min,
            uv_max,
            glyph.color,
        ));

        let is_cursor_glyph = glyph.line_i == cursor_line
            && cursor_byte_in_line >= glyph.byte_start
            && cursor_byte_in_line < glyph.byte_end;
        if is_cursor_glyph && cursor_overlay.is_none() {
            cursor_overlay = Some(GlyphInstance::new(
                position,
                size,
                uv_min,
                uv_max,
                cursor_overlay_color,
            ));
        }
    }

    atlas.flush_pending(queue);

    let caret_layout = compute_caret_layout(text_system, app_state, viewport_origin);
    Ok(LayoutProjection {
        glyph_instances,
        cursor_overlay,
        caret_layout,
    })
}

/// Compute the overlay glyph (if any) from the already-shaped buffer.
/// Used by the caret-only fast path — when the cursor moves in Normal mode
/// without touching the text, we still need the contrast-colored glyph on top
/// of the new caret position.
pub fn compute_cursor_overlay(
    text_system: &mut TextSystem,
    app_state: &AppState,
    atlas: &mut GlyphAtlas,
    queue: &wgpu::Queue,
    viewport_origin: [f32; 2],
    overlay_color: [f32; 4],
) -> Result<Option<GlyphInstance>, String> {
    let (cursor_line, _) = app_state.cursor_line_col();
    let cursor_byte_in_line = app_state.cursor_byte_in_line();

    let target = text_system
        .collect_visible_glyphs(viewport_origin[0], viewport_origin[1], overlay_color, None)
        .into_iter()
        .find(|g| {
            g.line_i == cursor_line
                && cursor_byte_in_line >= g.byte_start
                && cursor_byte_in_line < g.byte_end
        });

    let Some(glyph) = target else {
        return Ok(None);
    };

    let entry = if let Some(entry) = atlas.get(glyph.cache_key) {
        entry
    } else {
        let Some(rasterized) = rasterize_glyph_alpha(text_system, glyph.cache_key) else {
            return Ok(None);
        };
        let entry = atlas.get_or_reserve(glyph.cache_key, &rasterized)?;
        atlas.flush_pending(queue);
        entry
    };

    if entry.region.width == 0 || entry.region.height == 0 {
        return Ok(None);
    }

    let (uv_min, uv_max) = atlas.uv_min_max(entry.region);
    let top_left_x = glyph.physical_x + entry.placement_left;
    let top_left_y = glyph.physical_y - entry.placement_top;

    Ok(Some(GlyphInstance::new(
        [top_left_x as f32, top_left_y as f32],
        [entry.region.width as f32, entry.region.height as f32],
        uv_min,
        uv_max,
        overlay_color,
    )))
}

fn color_f32_to_u8(color: [f32; 4]) -> [u8; 4] {
    linear_rgba_to_srgb_u8(color)
}

/// Map a logical-line scroll position to the physical Y offset inside the shaped buffer.
///
/// When logical line `i` has been soft-wrapped into k visual rows, all subsequent logical
/// lines start at a visual Y that is (k-1)*line_height higher than the naive
/// `i * line_height` estimate.  Walking `layout_runs()` gives the true `line_top` of
/// that line's first visual row.
///
/// `logical_scroll` is the same `app_state.current_scroll_y` value (may be fractional
/// for smooth scrolling).  Returns 0.0 if the buffer has not been shaped yet.
pub fn visual_y_for_logical_scroll(text_system: &TextSystem, logical_scroll: f32) -> f32 {
    if logical_scroll <= 0.0 {
        return 0.0;
    }
    let target_line = logical_scroll.floor() as usize;
    let frac = logical_scroll.fract();
    for run in text_system.buffer().layout_runs() {
        if run.line_i == target_line {
            return run.line_top + frac * run.line_height;
        }
    }
    // Scroll is past the last shaped line — clamp to the buffer bottom.
    text_system
        .buffer()
        .layout_runs()
        .last()
        .map(|r| r.line_top + r.line_height)
        .unwrap_or(0.0)
}

/// Tính caret từ buffer cursor + layout metrics hiện tại.
/// Không lưu caret state rời rạc để tránh lệch pha với buffer.
pub fn compute_caret_layout(
    text_system: &TextSystem,
    app_state: &AppState,
    viewport_origin: [f32; 2],
) -> CaretLayout {
    let (target_line, _) = app_state.cursor_line_col();
    // glyph.start/end ở run hiện tại là offset theo line text, không phải toàn buffer.
    // Vì vậy caret cũng phải so theo byte offset trong line.
    let cursor_byte_in_line = app_state.cursor_byte_in_line();

    let metrics_line_height = text_system.buffer().metrics().line_height.max(1.0);
    let default_glyph_width = (metrics_line_height * 0.5).max(1.0);

    let mut fallback = CaretLayout {
        x: viewport_origin[0],
        top: viewport_origin[1],
        height: metrics_line_height,
        glyph_width: default_glyph_width,
    };

    for run in text_system.buffer().layout_runs() {
        fallback = CaretLayout {
            x: viewport_origin[0] + run.line_w,
            top: viewport_origin[1] + run.line_top,
            height: run.line_height.max(1.0),
            glyph_width: default_glyph_width,
        };

        if run.line_i != target_line {
            continue;
        }

        if run.glyphs.is_empty() {
            return CaretLayout {
                x: viewport_origin[0],
                top: viewport_origin[1] + run.line_top,
                height: run.line_height.max(1.0),
                glyph_width: default_glyph_width,
            };
        }

        let mut caret_x = viewport_origin[0] + run.line_w;
        let mut caret_glyph_width = default_glyph_width;
        for glyph in run.glyphs {
            let left = viewport_origin[0] + glyph.x;
            let right = left + glyph.w;

            if cursor_byte_in_line <= glyph.start {
                caret_x = left;
                caret_glyph_width = glyph.w.max(1.0);
                break;
            }

            if cursor_byte_in_line < glyph.end {
                // Cursor ở giữa cluster thì đặt caret ở rìa phải cluster.
                caret_x = right;
                caret_glyph_width = glyph.w.max(1.0);
                break;
            }

            if cursor_byte_in_line == glyph.end {
                caret_x = right;
                caret_glyph_width = glyph.w.max(1.0);
            }
        }

        return CaretLayout {
            x: caret_x,
            top: viewport_origin[1] + run.line_top,
            height: run.line_height.max(1.0),
            glyph_width: caret_glyph_width,
        };
    }

    fallback
}

/// Variant of `compute_caret_layout` for an arbitrary buffer position given as
/// (line_idx, byte_in_line).  Used by the renderer to place virtual-cursor carets.
pub fn compute_caret_layout_at(
    text_system: &TextSystem,
    target_line: usize,
    cursor_byte_in_line: usize,
    viewport_origin: [f32; 2],
) -> CaretLayout {
    let metrics_line_height = text_system.buffer().metrics().line_height.max(1.0);
    let default_glyph_width = (metrics_line_height * 0.5).max(1.0);

    let mut fallback = CaretLayout {
        x: viewport_origin[0],
        top: viewport_origin[1],
        height: metrics_line_height,
        glyph_width: default_glyph_width,
    };

    for run in text_system.buffer().layout_runs() {
        fallback = CaretLayout {
            x: viewport_origin[0] + run.line_w,
            top: viewport_origin[1] + run.line_top,
            height: run.line_height.max(1.0),
            glyph_width: default_glyph_width,
        };

        if run.line_i != target_line {
            continue;
        }

        if run.glyphs.is_empty() {
            return CaretLayout {
                x: viewport_origin[0],
                top: viewport_origin[1] + run.line_top,
                height: run.line_height.max(1.0),
                glyph_width: default_glyph_width,
            };
        }

        let mut caret_x = viewport_origin[0] + run.line_w;
        let mut caret_glyph_width = default_glyph_width;
        for glyph in run.glyphs {
            let left = viewport_origin[0] + glyph.x;
            let right = left + glyph.w;

            if cursor_byte_in_line <= glyph.start {
                caret_x = left;
                caret_glyph_width = glyph.w.max(1.0);
                break;
            }
            if cursor_byte_in_line < glyph.end {
                caret_x = right;
                caret_glyph_width = glyph.w.max(1.0);
                break;
            }
            if cursor_byte_in_line == glyph.end {
                caret_x = right;
                caret_glyph_width = glyph.w.max(1.0);
            }
        }

        return CaretLayout {
            x: caret_x,
            top: viewport_origin[1] + run.line_top,
            height: run.line_height.max(1.0),
            glyph_width: caret_glyph_width,
        };
    }

    fallback
}

#[cfg(test)]
mod tests {
    use cosmic_text::Metrics;

    use crate::{app::app_state::AppState, text::text_system::TextSystem};

    use super::compute_caret_layout;

    #[test]
    fn caret_uses_line_relative_byte_offset_for_second_line_start() {
        let mut app_state =
            AppState::from_text(std::env::temp_dir().join("phase5_test.txt"), "abc\nxyz");
        // Từ đầu line 1 đi xuống đầu line 2.
        app_state.move_down();
        assert_eq!(app_state.cursor_line_col(), (1, 0));

        let mut text_system = TextSystem::new(Metrics::new(20.0, 28.0), Some(800.0), Some(300.0));
        text_system.set_text(&app_state.text_string());

        let origin = [40.0, 80.0];
        let caret = compute_caret_layout(&text_system, &app_state, origin);

        // Nếu dùng byte toàn buffer sai, caret sẽ bị đẩy lệch sang phải.
        assert!(
            (caret.x - origin[0]).abs() < 0.5,
            "caret.x={} origin.x={}",
            caret.x,
            origin[0]
        );
    }
}
