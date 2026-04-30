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
    let alpha = extract_alpha_from_image_data(image.content, &image.data, pixel_count);

    Some(RasterizedGlyph {
        width,
        height,
        placement_left: image.placement.left,
        placement_top: image.placement.top,
        alpha,
    })
}

fn extract_alpha_from_image_data(
    content: SwashContent,
    data: &[u8],
    pixel_count: usize,
) -> Vec<u8> {
    let mut alpha = vec![0u8; pixel_count];

    match content {
        SwashContent::Mask => {
            // Mask: mỗi pixel là 1 byte alpha.
            for (i, value) in data.iter().take(pixel_count).enumerate() {
                alpha[i] = *value;
            }
        }
        SwashContent::Color => {
            // Color: RGBA, ta lấy alpha channel để blend màu text custom trong shader.
            for (i, rgba) in data.chunks_exact(4).take(pixel_count).enumerate() {
                alpha[i] = rgba[3];
            }
        }
        SwashContent::SubpixelMask => {
            // Subpixel: lấy max của 3 channel như coverage gần đúng.
            for (i, rgb) in data.chunks_exact(3).take(pixel_count).enumerate() {
                alpha[i] = rgb[0].max(rgb[1]).max(rgb[2]);
            }
        }
    }

    alpha
}

#[cfg(test)]
mod tests {
    use cosmic_text::SwashContent;

    use super::extract_alpha_from_image_data;

    #[test]
    fn mask_alpha_conversion_truncates_to_expected_pixel_count() {
        let alpha = extract_alpha_from_image_data(SwashContent::Mask, &[1, 2, 3, 4], 3);
        assert_eq!(alpha, vec![1, 2, 3]);
    }

    #[test]
    fn color_alpha_conversion_reads_only_alpha_channel() {
        let alpha =
            extract_alpha_from_image_data(SwashContent::Color, &[9, 8, 7, 6, 1, 2, 3, 4], 2);
        assert_eq!(alpha, vec![6, 4]);
    }

    #[test]
    fn subpixel_alpha_conversion_uses_max_channel_per_pixel() {
        let alpha =
            extract_alpha_from_image_data(SwashContent::SubpixelMask, &[1, 9, 3, 7, 2, 5], 2);
        assert_eq!(alpha, vec![9, 7]);
    }

    #[test]
    fn malformed_color_payload_leaves_missing_pixels_zeroed() {
        let alpha = extract_alpha_from_image_data(SwashContent::Color, &[1, 2, 3], 1);
        assert_eq!(alpha, vec![0]);
    }
}
