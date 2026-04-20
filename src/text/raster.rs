use cosmic_text::{CacheKey, SwashContent};

use crate::text::text_system::TextSystem;

/// Bitmap coverage của một glyph sau bước rasterize.
///
/// Phase 3 dùng alpha coverage để blend text color trên GPU.
#[derive(Debug, Clone)]
pub struct RasterizedGlyph {
    pub width: u32,
    pub height: u32,
    pub placement_left: i32,
    pub placement_top: i32,
    pub alpha: Vec<u8>,
}

/// Rasterize một glyph key thành alpha bitmap.
///
/// Trả về `None` với glyph rỗng (ví dụ space) hoặc khi swash không tạo được image.
pub fn rasterize_glyph_alpha(
    text_system: &mut TextSystem,
    cache_key: CacheKey,
) -> Option<RasterizedGlyph> {
    let image = text_system.rasterize_cache_key(cache_key)?;
    let width = image.placement.width;
    let height = image.placement.height;

    if width == 0 || height == 0 {
        return None;
    }

    let pixel_count = (width as usize) * (height as usize);
    let mut alpha = vec![0u8; pixel_count];

    match image.content {
        SwashContent::Mask => {
            // Mask: mỗi pixel là 1 byte alpha.
            for (i, value) in image.data.iter().take(pixel_count).enumerate() {
                alpha[i] = *value;
            }
        }
        SwashContent::Color => {
            // Color: RGBA, ta lấy alpha channel để blend màu text custom trong shader.
            for (i, rgba) in image.data.chunks_exact(4).take(pixel_count).enumerate() {
                alpha[i] = rgba[3];
            }
        }
        SwashContent::SubpixelMask => {
            // Subpixel: lấy max của 3 channel như coverage gần đúng.
            for (i, rgb) in image.data.chunks_exact(3).take(pixel_count).enumerate() {
                alpha[i] = rgb[0].max(rgb[1]).max(rgb[2]);
            }
        }
    }

    Some(RasterizedGlyph {
        width,
        height,
        placement_left: image.placement.left,
        placement_top: image.placement.top,
        alpha,
    })
}
