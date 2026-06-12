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
//! ANSI background cells are emitted as `RegionDrawInstance` quads so full-cell
//! TUI panels do not depend on font glyph coverage.
use crate::{
    render::{glyph_instance::GlyphInstance, region_pipeline::RegionDrawInstance},
    terminal::cell_shapes::solid_cell_rects,
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

    /// Build background quads từ ANSI background-colored cells.
    ///
    /// Native terminals render cell backgrounds as rectangles. Do the same here
    /// instead of rasterizing a block glyph, otherwise TUIs show visible row/column
    /// seams whenever they paint full-screen panels.
    pub fn build_background_instances(
        &self,
        grid: &TerminalGrid,
        default_fg: [f32; 4],
        default_bg: [f32; 4],
        clip_width: f32,
    ) -> Vec<RegionDrawInstance> {
        let mut instances = Vec::new();
        let clip_right = self.origin_x + clip_width;
        let mut run: Option<(usize, f32, f32, f32, [f32; 4])> = None;

        for (row, col, cell) in grid.iter_visible_cells() {
            let screen_x = self.origin_x + col as f32 * self.cell_width;
            let screen_y = self.origin_y + row as f32 * self.cell_height;

            if screen_x >= clip_right
                || cell.style.bg == crate::terminal::ansi_parser::AnsiColor::Default
            {
                flush_background_run(&mut instances, &mut run, self.cell_height);
                continue;
            }

            let bg_rgba = cell
                .style
                .bg
                .to_rgba_f32_with_defaults(default_fg, default_bg, false);
            let cell_w = self.cell_width.min((clip_right - screen_x).max(0.0));
            if cell_w <= 0.0 {
                flush_background_run(&mut instances, &mut run, self.cell_height);
                continue;
            }

            match run.as_mut() {
                Some((run_row, _run_x, run_y, run_w, run_color))
                    if *run_row == row
                        && (*run_y - screen_y).abs() < f32::EPSILON
                        && *run_color == bg_rgba =>
                {
                    *run_w += cell_w;
                }
                _ => {
                    flush_background_run(&mut instances, &mut run, self.cell_height);
                    run = Some((row, screen_x, screen_y, cell_w, bg_rgba));
                }
            }
        }

        flush_background_run(&mut instances, &mut run, self.cell_height);
        instances
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
            let has_foreground_glyph = cell.ch != ' ' && cell.ch != '\0';

            // Backgrounds are rendered by `build_background_instances()` as real
            // quads. Keep this text path foreground-only so spaces remain cells,
            // not block glyphs.
            if !has_foreground_glyph {
                continue;
            }

            let screen_x = self.origin_x + col as f32 * self.cell_width;
            let screen_y = self.origin_y + row as f32 * self.cell_height;

            // Skip cells that start beyond the clipping boundary.
            if screen_x >= clip_right {
                continue;
            }

            let fg_rgba = cell.style_fg.unwrap_or_else(|| {
                cell.style
                    .fg
                    .to_rgba_f32_with_defaults(default_fg, default_bg, true)
            });

            // Block elements / box drawing: vẽ quad solid phủ kín cell thay vì
            // glyph font (glyph chỉ phủ line-box của font, nhỏ hơn cell height
            // panel_line_height → border/logo bị hở khe "đứt nét").
            if let Some(rects) = solid_cell_rects(cell.ch, self.cell_width, self.cell_height) {
                if let Some(solid) = atlas.solid_entry() {
                    let (uv_min, uv_max) = atlas.uv_min_max(solid.region);
                    for rect in rects {
                        let x = screen_x + rect.x;
                        let w = rect.w.min((clip_right - x).max(0.0));
                        if w <= 0.0 {
                            continue;
                        }
                        let mut color = fg_rgba;
                        color[3] *= rect.alpha;
                        instances.push(GlyphInstance::new(
                            [x, screen_y + rect.y],
                            [w, rect.h],
                            uv_min,
                            uv_max,
                            color,
                        ));
                    }
                    continue;
                }
            }

            // Cấu hình text system để shape 1 ký tự.
            text_system.set_size(Some(self.cell_width), Some(self.cell_height));
            let ch_str = cell.ch.to_string();
            // Dùng set_text để shape qua TextSystem API.
            // Màu sẽ được override bởi fg_rgba khi tạo GlyphInstance.
            if cell.style.bold {
                text_system.set_text_bold_color(&ch_str, [255, 255, 255, 255]);
            } else {
                text_system.set_text(&ch_str);
            }

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

fn flush_background_run(
    instances: &mut Vec<RegionDrawInstance>,
    run: &mut Option<(usize, f32, f32, f32, [f32; 4])>,
    cell_height: f32,
) {
    if let Some((_row, x, y, w, color)) = run.take()
        && w > 0.0
    {
        instances.push(RegionDrawInstance::new(
            [x, y, w, cell_height.max(1.0)],
            color,
        ));
    }
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use cosmic_text::Metrics;

    use crate::text::text_system::TextSystem;

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

    #[test]
    fn background_instances_merge_adjacent_cells() {
        let mut grid = TerminalGrid::new(6, 2);
        grid.feed_chunk("\x1b[44m  \x1b[0mA");

        let renderer = TerminalViewRenderer::new(10.0, 20.0, 5.0, 7.0, 14.0);
        let instances = renderer.build_background_instances(
            &grid,
            [1.0, 1.0, 1.0, 1.0],
            [0.0, 0.0, 0.0, 1.0],
            100.0,
        );

        assert_eq!(instances.len(), 1);
        assert_eq!(instances[0].rect, [5.0, 7.0, 20.0, 20.0]);
    }

    #[test]
    fn vietnamese_cells_shape_to_visible_glyphs() {
        let mut text_system =
            TextSystem::new(Metrics::new(14.0, 20.0), Some(14.0 * 0.6), Some(20.0));

        for ch in ['đ', 'ổ', 'ệ'] {
            text_system.set_size(Some(14.0 * 0.6), Some(20.0));
            text_system.set_text(&ch.to_string());
            let glyphs = text_system.collect_visible_glyphs(0.0, 0.0, [1.0, 1.0, 1.0, 1.0], None);
            assert!(!glyphs.is_empty(), "expected visible glyph for {ch}");
        }
    }
}
