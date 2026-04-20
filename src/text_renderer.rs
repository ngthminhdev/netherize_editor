use cosmic_text::{Attrs, Buffer, Color, FontSystem, Metrics, Shaping, SwashCache};

/// 3 trụ cột của cosmic-text.
///
/// FontSystem + SwashCache: tài nguyên tĩnh, khởi tạo 1 lần.
/// Buffer: thay đổi mỗi khi nội dung text thay đổi.
pub struct TextRenderer {
    pub font_system: FontSystem,
    pub swash_cache: SwashCache,
    pub buffer: Buffer,
}

impl TextRenderer {
    pub fn new(font_size: f32, line_height: f32, width: f32) -> Self {
        // Trụ cột 1: FontSystem — tự load font từ hệ thống (macOS/Windows/Linux)
        let mut font_system = FontSystem::new();

        // Trụ cột 2: SwashCache — bộ nhớ tạm chứa bitmap của từng glyph đã rasterize
        let swash_cache = SwashCache::new();

        // Trụ cột 3: Buffer — "document" tính toán tọa độ X, Y cho từng chữ
        let metrics = Metrics::new(font_size, line_height);
        let mut buffer = Buffer::new(&mut font_system, metrics);
        buffer.set_size(&mut font_system, Some(width), None);

        Self {
            font_system,
            swash_cache,
            buffer,
        }
    }

    /// Cập nhật nội dung text. Gọi mỗi khi user gõ phím.
    pub fn set_text(&mut self, text: &str) {
        let attrs = Attrs::new();
        self.buffer
            .set_text(&mut self.font_system, text, &attrs, Shaping::Advanced, None);
        self.buffer.shape_until_scroll(&mut self.font_system, false);
    }

    /// Rasterize buffer hiện tại, trả về Vec<(x, y, r, g, b, a)> per pixel.
    pub fn rasterize(&mut self) -> Vec<(i32, i32, u8, u8, u8, u8)> {
        let mut pixels: Vec<(i32, i32, u8, u8, u8, u8)> = Vec::new();
        let text_color = Color::rgb(0xFF, 0xFF, 0xFF);

        self.buffer.draw(
            &mut self.font_system,
            &mut self.swash_cache,
            text_color,
            |x, y, w, h, color| {
                for dy in 0..h {
                    for dx in 0..w {
                        let a = color.a();
                        if a > 0 {
                            pixels.push((
                                x + dx as i32,
                                y + dy as i32,
                                color.r(),
                                color.g(),
                                color.b(),
                                a,
                            ));
                        }
                    }
                }
            },
        );

        pixels
    }
}
