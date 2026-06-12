use std::collections::HashMap;

use cosmic_text::CacheKey;

use crate::text::raster::RasterizedGlyph;

#[derive(Debug, Clone, Copy)]
pub struct AtlasRegion {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone, Copy)]
pub struct AtlasEntry {
    pub region: AtlasRegion,
    pub placement_left: i32,
    pub placement_top: i32,
}

/// GlyphAtlas quản lý:
/// - texture atlas trên GPU
/// - vị trí placement theo shelf packing đơn giản
/// - map từ CacheKey -> AtlasEntry
pub struct GlyphAtlas {
    texture: wgpu::Texture,
    view: wgpu::TextureView,
    sampler: wgpu::Sampler,
    width: u32,
    height: u32,
    next_x: u32,
    next_y: u32,
    row_height: u32,
    entries: HashMap<CacheKey, AtlasEntry>,
    /// Glyph mới cache miss trong frame hiện tại. Tích lũy CPU-side rồi gọi
    /// `flush_pending()` 1 lần cuối batch để gộp `queue.write_texture` —
    /// tránh N submit GPU khi mở file mới (mỗi glyph riêng = 1 submit).
    pending: Vec<PendingGlyphUpload>,
    /// Vùng texel đặc (alpha 255) — dùng vẽ hình chữ nhật solid (block/box
    /// drawing chars) qua text pipeline. Khởi tạo lazy lần đầu cần.
    solid: Option<AtlasEntry>,
}

struct PendingGlyphUpload {
    region: AtlasRegion,
    alpha: Vec<u8>,
}

impl GlyphAtlas {
    pub fn new(device: &wgpu::Device, width: u32, height: u32) -> Self {
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Netherize Glyph Atlas"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::R8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });

        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("Netherize Glyph Atlas Sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            mipmap_filter: wgpu::MipmapFilterMode::Nearest,
            ..Default::default()
        });

        Self {
            texture,
            view,
            sampler,
            width,
            height,
            next_x: 0,
            next_y: 0,
            row_height: 0,
            entries: HashMap::new(),
            pending: Vec::new(),
            solid: None,
        }
    }

    pub fn view(&self) -> &wgpu::TextureView {
        &self.view
    }

    pub fn sampler(&self) -> &wgpu::Sampler {
        &self.sampler
    }

    pub fn get(&self, cache_key: CacheKey) -> Option<AtlasEntry> {
        self.entries.get(&cache_key).copied()
    }

    /// Lấy entry nếu đã có, nếu chưa thì reserve region + queue alpha vào
    /// `pending`. **Không upload GPU ở đây** — caller phải gọi `flush_pending()`
    /// sau khi gom xong tất cả glyph của batch (thường là cuối render path).
    pub fn get_or_reserve(
        &mut self,
        cache_key: CacheKey,
        rasterized: &RasterizedGlyph,
    ) -> Result<AtlasEntry, String> {
        if let Some(entry) = self.get(cache_key) {
            return Ok(entry);
        }

        let region = self
            .allocate_region(rasterized.width, rasterized.height)
            .ok_or_else(|| {
                format!(
                    "glyph atlas full: cannot allocate {}x{} in {}x{} atlas",
                    rasterized.width, rasterized.height, self.width, self.height
                )
            })?;

        self.pending.push(PendingGlyphUpload {
            region,
            alpha: rasterized.alpha.clone(),
        });

        let entry = AtlasEntry {
            region,
            placement_left: rasterized.placement_left,
            placement_top: rasterized.placement_top,
        };
        self.entries.insert(cache_key, entry);
        Ok(entry)
    }

    /// Entry trỏ tới một vùng texel đặc (alpha = 255). Dùng làm texture cho
    /// các quad solid (block elements / box drawing) vẽ chung pass với text.
    /// Vùng 4×4 được reserve nhưng entry chỉ trỏ vào lõi 2×2 để Nearest
    /// sampling không bleed sang glyph kề cạnh.
    pub fn solid_entry(&mut self) -> Option<AtlasEntry> {
        if let Some(entry) = self.solid {
            return Some(entry);
        }
        let region = self.allocate_region(4, 4)?;
        self.pending.push(PendingGlyphUpload {
            region,
            alpha: vec![255u8; 16],
        });
        let entry = AtlasEntry {
            region: AtlasRegion {
                x: region.x + 1,
                y: region.y + 1,
                width: 2,
                height: 2,
            },
            placement_left: 0,
            placement_top: 0,
        };
        self.solid = Some(entry);
        Some(entry)
    }

    /// Upload toàn bộ glyph mới reserve trong batch. Idempotent khi rỗng.
    /// Mỗi region được upload riêng (chỉ phần đã reserve, không upload toàn
    /// texture); nếu nội dung text không đổi thì `pending` rỗng → no-op.
    pub fn flush_pending(&mut self, queue: &wgpu::Queue) {
        if self.pending.is_empty() {
            return;
        }
        for pending in self.pending.drain(..) {
            queue.write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: &self.texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d {
                        x: pending.region.x,
                        y: pending.region.y,
                        z: 0,
                    },
                    aspect: wgpu::TextureAspect::All,
                },
                &pending.alpha,
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(pending.region.width),
                    rows_per_image: Some(pending.region.height),
                },
                wgpu::Extent3d {
                    width: pending.region.width,
                    height: pending.region.height,
                    depth_or_array_layers: 1,
                },
            );
        }
    }

    /// Convert atlas pixel region -> uv normalized [0..1].
    pub fn uv_min_max(&self, region: AtlasRegion) -> ([f32; 2], [f32; 2]) {
        atlas_uv_min_max(self.width, self.height, region)
    }

    /// Shelf packing đơn giản:
    /// - đặt glyph trên một hàng
    /// - hết ngang thì xuống hàng mới
    fn allocate_region(&mut self, width: u32, height: u32) -> Option<AtlasRegion> {
        let state = AtlasPackingState {
            atlas_width: self.width,
            atlas_height: self.height,
            next_x: self.next_x,
            next_y: self.next_y,
            row_height: self.row_height,
        };
        let (region, next) = allocate_region_in_state(state, width, height)?;
        self.next_x = next.next_x;
        self.next_y = next.next_y;
        self.row_height = next.row_height;
        Some(region)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct AtlasPackingState {
    atlas_width: u32,
    atlas_height: u32,
    next_x: u32,
    next_y: u32,
    row_height: u32,
}

fn atlas_uv_min_max(width: u32, height: u32, region: AtlasRegion) -> ([f32; 2], [f32; 2]) {
    let w = width as f32;
    let h = height as f32;
    let uv_min = [region.x as f32 / w, region.y as f32 / h];
    let uv_max = [
        (region.x + region.width) as f32 / w,
        (region.y + region.height) as f32 / h,
    ];
    (uv_min, uv_max)
}

fn allocate_region_in_state(
    mut state: AtlasPackingState,
    width: u32,
    height: u32,
) -> Option<(AtlasRegion, AtlasPackingState)> {
    if width == 0 || height == 0 {
        return None;
    }
    if width > state.atlas_width || height > state.atlas_height {
        return None;
    }

    if state.next_x + width > state.atlas_width {
        state.next_x = 0;
        state.next_y = state.next_y.saturating_add(state.row_height);
        state.row_height = 0;
    }

    if state.next_y + height > state.atlas_height {
        return None;
    }

    let region = AtlasRegion {
        x: state.next_x,
        y: state.next_y,
        width,
        height,
    };

    state.next_x += width;
    state.row_height = state.row_height.max(height);

    Some((region, state))
}

#[cfg(test)]
mod tests {
    use super::{AtlasPackingState, AtlasRegion, allocate_region_in_state, atlas_uv_min_max};

    #[test]
    fn zero_sized_and_oversized_regions_are_rejected() {
        let state = AtlasPackingState {
            atlas_width: 16,
            atlas_height: 16,
            next_x: 0,
            next_y: 0,
            row_height: 0,
        };

        assert!(allocate_region_in_state(state, 0, 4).is_none());
        assert!(allocate_region_in_state(state, 4, 0).is_none());
        assert!(allocate_region_in_state(state, 17, 4).is_none());
        assert!(allocate_region_in_state(state, 4, 17).is_none());
    }

    #[test]
    fn allocation_wraps_to_next_row_without_overlap() {
        let state = AtlasPackingState {
            atlas_width: 10,
            atlas_height: 10,
            next_x: 0,
            next_y: 0,
            row_height: 0,
        };

        let (first, state) = allocate_region_in_state(state, 6, 3).expect("first region");
        let (second, _) = allocate_region_in_state(state, 5, 2).expect("wrapped region");

        assert_eq!((first.x, first.y, first.width, first.height), (0, 0, 6, 3));
        assert_eq!(
            (second.x, second.y, second.width, second.height),
            (0, 3, 5, 2)
        );
    }

    #[test]
    fn uv_coordinates_stay_normalized_at_texture_edge() {
        let (uv_min, uv_max) = atlas_uv_min_max(
            8,
            4,
            AtlasRegion {
                x: 6,
                y: 1,
                width: 2,
                height: 3,
            },
        );

        assert_eq!(uv_min, [0.75, 0.25]);
        assert_eq!(uv_max, [1.0, 1.0]);
    }
}
