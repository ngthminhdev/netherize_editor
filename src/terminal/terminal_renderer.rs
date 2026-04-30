//! Terminal View Renderer — Phase 9b.
//!
//! Chuyển đổi `TerminalGrid` → `Vec<GlyphInstance>` để `TextPipeline` tiêu thụ.
//!
//! # Kiến trúc
//!
//! ```text
//! TerminalGrid  →  TerminalViewRenderer  →  Vec<GlyphInstance>
//!                        ↑                        ↓
//!      TextSystem (FontSystem + SwashCache)  TextPipeline::upload_instances()
//!      GlyphAtlas (atlas texture + UV map)
//! ```
//!
//! # Cách hoạt động
//!
//! Với mỗi visible cell có `ch != ' '`:
//! 1. Dùng `TextSystem` để shape + layout ký tự, lấy `CacheKey` của glyph.
//! 2. Rasterize glyph qua `rasterize_glyph_alpha()`.
//! 3. Upload vào `GlyphAtlas::get_or_insert()` để lấy `AtlasEntry`.
//! 4. Tính UV coords bằng `atlas.uv_min_max()`.
//! 5. Tạo `GlyphInstance` với màu lấy từ `CellStyle.fg.to_rgba_f32(true)`.
//!
//! Background cells không render ở phase này (future: QuadPipeline).
use crate::{
    render::glyph_instance::GlyphInstance,
    terminal::grid::TerminalGrid,
    text::{atlas::GlyphAtlas, raster::rasterize_glyph_alpha, text_system::TextSystem},
};

/// Renderer chuyển TerminalGrid → GlyphInstances.
///
/// Giữ font metrics và origin offset để map `(row, col)` → pixel screen coords.
pub struct TerminalViewRenderer {
    /// Pixel width của một terminal cell (monospace).
    pub cell_width: f32,
    /// Pixel height của một terminal cell.
    pub cell_height: f32,
    /// Pixel x offset của góc trên-trái của terminal area trên màn hình.
    pub origin_x: f32,
    /// Pixel y offset của góc trên-trái của terminal area trên màn hình.
    pub origin_y: f32,
    /// Font size (points) dùng cho shaping.
    pub font_size: f32,
}

impl TerminalViewRenderer {
    /// Tạo renderer với cell size cụ thể.
    pub fn new(
        cell_width: f32,
        cell_height: f32,
        origin_x: f32,
        origin_y: f32,
        font_size: f32,
    ) -> Self {
        Self {
            cell_width,
            cell_height,
            origin_x,
            origin_y,
            font_size,
        }
    }

    /// Defaults hợp lý cho terminal monospace 14pt.
    pub fn default_monospace() -> Self {
        Self::new(8.4, 17.0, 0.0, 0.0, 14.0)
    }

    /// Build danh sách `GlyphInstance` từ grid.
    ///
    /// - Bỏ qua cells có `ch == ' '` (blank).
    /// - Mỗi ký tự được shape, rasterize, atlas-pack rồi tạo instance.
    /// - Caller upload kết quả vào `TextPipeline::upload_instances()`.
    ///
    /// # Parameters
    ///
    /// - `grid`: grid hiện tại để iterate cells
    /// - `atlas`: GPU texture atlas (đọc entry nếu có, insert mới nếu chưa)
    /// - `queue`: wgpu queue để upload glyph bitmap lên GPU
    /// - `text_system`: `TextSystem` có `FontSystem` + `SwashCache` nội bộ
    pub fn build_instances(
        &self,
        grid: &TerminalGrid,
        atlas: &mut GlyphAtlas,
        queue: &wgpu::Queue,
        text_system: &mut TextSystem,
        default_fg: [f32; 4],
        default_bg: [f32; 4],
        clip_width: f32,
    ) -> Vec<GlyphInstance> {
        let mut instances = Vec::new();
        let clip_right = self.origin_x + clip_width;

        for (row, col, cell) in grid.iter_visible_cells() {
            let has_background = cell.style.bg != crate::terminal::ansi_parser::AnsiColor::Default;
            let has_foreground_glyph = cell.ch != ' ' && cell.ch != '\0';

            // Bỏ qua ô hoàn toàn trống, nhưng vẫn giữ các ô có background ANSI.
            if !has_background && !has_foreground_glyph {
                continue;
            }

            let screen_x = self.origin_x + col as f32 * self.cell_width;
            let screen_y = self.origin_y + row as f32 * self.cell_height;

            // Skip cells that start beyond the clipping boundary.
            if screen_x >= clip_right {
                continue;
            }

            if has_background {
                let bg_rgba = cell
                    .style
                    .bg
                    .to_rgba_f32_with_defaults(default_fg, default_bg, false);

                // Dùng full block glyph để giả lập background cell màu.
                text_system.set_size(Some(self.cell_width), Some(self.cell_height));
                text_system.set_text("█");
                let bg_glyphs = text_system.collect_visible_glyphs(0.0, 0.0, bg_rgba, None);

                for vg in bg_glyphs {
                    let atlas_entry = if let Some(existing) = atlas.get(vg.cache_key) {
                        existing
                    } else {
                        match rasterize_glyph_alpha(text_system, vg.cache_key) {
                            Some(rasterized) => {
                                match atlas.get_or_reserve(vg.cache_key, &rasterized) {
                                    Ok(entry) => entry,
                                    Err(err) => {
                                        eprintln!("[TerminalRenderer] atlas insert failed: {err}");
                                        continue;
                                    }
                                }
                            }
                            None => continue,
                        }
                    };

                    let (uv_min, uv_max) = atlas.uv_min_max(atlas_entry.region);
                    let glyph_w = atlas_entry.region.width as f32;
                    let glyph_h = atlas_entry.region.height as f32;
                    let glyph_x =
                        screen_x + vg.physical_x as f32 + atlas_entry.placement_left as f32;
                    let glyph_y =
                        screen_y + vg.physical_y as f32 - atlas_entry.placement_top as f32;

                    instances.push(GlyphInstance::new(
                        [glyph_x, glyph_y],
                        [glyph_w, glyph_h],
                        uv_min,
                        uv_max,
                        bg_rgba,
                    ));
                }
            }

            if !has_foreground_glyph {
                continue;
            }

            let fg_rgba = cell
                .style
                .fg
                .to_rgba_f32_with_defaults(default_fg, default_bg, true);

            // Cấu hình text system để shape 1 ký tự.
            text_system.set_size(Some(self.cell_width), Some(self.cell_height));
            let ch_str = cell.ch.to_string();
            // Dùng set_text để shape qua TextSystem API.
            // Màu sẽ được override bởi fg_rgba khi tạo GlyphInstance.
            text_system.set_text(&ch_str);

            // Thu thập glyph từ layout (origin 0,0 — ta tính thêm screen offset sau).
            let visible_glyphs = text_system.collect_visible_glyphs(0.0, 0.0, fg_rgba, None);

            for vg in visible_glyphs {
                // Kiểm tra cache trước khi rasterize.
                let atlas_entry = if let Some(existing) = atlas.get(vg.cache_key) {
                    existing
                } else {
                    // Rasterize và reserve trong atlas (upload qua flush_pending sau).
                    match rasterize_glyph_alpha(text_system, vg.cache_key) {
                        Some(rasterized) => {
                            match atlas.get_or_reserve(vg.cache_key, &rasterized) {
                                Ok(entry) => entry,
                                Err(err) => {
                                    // Atlas đầy hoặc glyph quá lớn — bỏ qua cell này.
                                    eprintln!("[TerminalRenderer] atlas insert failed: {err}");
                                    continue;
                                }
                            }
                        }
                        None => {
                            // Glyph rỗng (ví dụ space, control char) → skip.
                            continue;
                        }
                    }
                };

                let (uv_min, uv_max) = atlas.uv_min_max(atlas_entry.region);
                let glyph_w = atlas_entry.region.width as f32;
                let glyph_h = atlas_entry.region.height as f32;

                // screen_pos tính bằng cách kết hợp:
                //   - origin của terminal area (origin_x / origin_y)
                //   - vị trí cell (row/col * cell size)
                //   - placement offset từ cosmic-text (placement_left / placement_top)
                let glyph_x = screen_x + vg.physical_x as f32 + atlas_entry.placement_left as f32;
                let glyph_y = screen_y + vg.physical_y as f32 - atlas_entry.placement_top as f32;

                instances.push(GlyphInstance::new(
                    [glyph_x, glyph_y],
                    [glyph_w, glyph_h],
                    uv_min,
                    uv_max,
                    fg_rgba,
                ));
            }
        }

        atlas.flush_pending(queue);
        instances
    }

    /// Tính pixel rect của cell `(row, col)` — hữu ích cho caret / highlight.
    ///
    /// Trả `[x, y, width, height]` theo pixel.
    pub fn cell_rect(&self, row: usize, col: usize) -> [f32; 4] {
        let x = self.origin_x + col as f32 * self.cell_width;
        let y = self.origin_y + row as f32 * self.cell_height;
        [x, y, self.cell_width, self.cell_height]
    }
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cell_rect_calculation() {
        let renderer = TerminalViewRenderer::new(8.0, 16.0, 10.0, 20.0, 14.0);
        let rect = renderer.cell_rect(2, 3);
        // x = 10.0 + 3 * 8.0 = 34.0
        // y = 20.0 + 2 * 16.0 = 52.0
        assert!((rect[0] - 34.0).abs() < 0.001, "x={}", rect[0]);
        assert!((rect[1] - 52.0).abs() < 0.001, "y={}", rect[1]);
        assert!((rect[2] - 8.0).abs() < 0.001);
        assert!((rect[3] - 16.0).abs() < 0.001);
    }

    #[test]
    fn default_renderer_has_positive_cell_size() {
        let renderer = TerminalViewRenderer::default_monospace();
        assert!(renderer.cell_width > 0.0);
        assert!(renderer.cell_height > 0.0);
        assert!(renderer.font_size > 0.0);
    }
}
