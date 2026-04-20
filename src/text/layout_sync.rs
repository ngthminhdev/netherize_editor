use crate::{
    app::app_state::AppState,
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
}

/// Dữ liệu projection từ buffer sang render text/caret.
#[derive(Debug, Clone)]
pub struct LayoutProjection {
    pub glyph_instances: Vec<GlyphInstance>,
    pub caret_layout: CaretLayout,
}

pub fn rebuild_layout_projection(
    text: &str,
    app_state: &AppState,
    text_system: &mut TextSystem,
    atlas: &mut GlyphAtlas,
    queue: &wgpu::Queue,
    viewport_origin: [f32; 2],
    text_color: [f32; 4],
    styled_spans: &[StyledTextSpan],
) -> Result<LayoutProjection, String> {
    let default_color_rgba = color_f32_to_u8(text_color);
    text_system.set_text_with_spans(text, default_color_rgba, styled_spans);

    let visible_glyphs =
        text_system.collect_visible_glyphs(viewport_origin[0], viewport_origin[1], text_color);
    let mut glyph_instances = Vec::with_capacity(visible_glyphs.len());

    for glyph in visible_glyphs {
        let entry = if let Some(entry) = atlas.get(glyph.cache_key) {
            entry
        } else {
            let Some(rasterized) = rasterize_glyph_alpha(text_system, glyph.cache_key) else {
                continue;
            };

            atlas.get_or_insert(queue, glyph.cache_key, &rasterized)?
        };

        if entry.region.width == 0 || entry.region.height == 0 {
            continue;
        }

        let (uv_min, uv_max) = atlas.uv_min_max(entry.region);
        let top_left_x = glyph.physical_x + entry.placement_left;
        let top_left_y = glyph.physical_y - entry.placement_top;

        glyph_instances.push(GlyphInstance::new(
            [top_left_x as f32, top_left_y as f32],
            [entry.region.width as f32, entry.region.height as f32],
            uv_min,
            uv_max,
            glyph.color,
        ));
    }

    let caret_layout = compute_caret_layout(text_system, app_state, viewport_origin);
    Ok(LayoutProjection {
        glyph_instances,
        caret_layout,
    })
}

fn color_f32_to_u8(color: [f32; 4]) -> [u8; 4] {
    [
        f32_channel_to_u8(color[0]),
        f32_channel_to_u8(color[1]),
        f32_channel_to_u8(color[2]),
        f32_channel_to_u8(color[3]),
    ]
}

fn f32_channel_to_u8(channel: f32) -> u8 {
    let clamped = channel.clamp(0.0, 1.0);
    (clamped * 255.0).round() as u8
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

    let mut fallback = CaretLayout {
        x: viewport_origin[0],
        top: viewport_origin[1],
        height: text_system.buffer().metrics().line_height.max(1.0),
    };

    for run in text_system.buffer().layout_runs() {
        fallback = CaretLayout {
            x: viewport_origin[0] + run.line_w,
            top: viewport_origin[1] + run.line_top,
            height: run.line_height.max(1.0),
        };

        if run.line_i != target_line {
            continue;
        }

        if run.glyphs.is_empty() {
            return CaretLayout {
                x: viewport_origin[0],
                top: viewport_origin[1] + run.line_top,
                height: run.line_height.max(1.0),
            };
        }

        let mut caret_x = viewport_origin[0] + run.line_w;
        for glyph in run.glyphs {
            let left = viewport_origin[0] + glyph.x;
            let right = left + glyph.w;

            if cursor_byte_in_line <= glyph.start {
                caret_x = left;
                break;
            }

            if cursor_byte_in_line < glyph.end {
                // Cursor ở giữa cluster thì đặt caret ở rìa phải cluster.
                caret_x = right;
                break;
            }

            if cursor_byte_in_line == glyph.end {
                caret_x = right;
            }
        }

        return CaretLayout {
            x: caret_x,
            top: viewport_origin[1] + run.line_top,
            height: run.line_height.max(1.0),
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
