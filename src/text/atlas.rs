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

    /// Lấy entry nếu đã có, nếu chưa thì pack + upload vào atlas.
    pub fn get_or_insert(
        &mut self,
        queue: &wgpu::Queue,
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

        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &self.texture,
                mip_level: 0,
                origin: wgpu::Origin3d {
                    x: region.x,
                    y: region.y,
                    z: 0,
                },
                aspect: wgpu::TextureAspect::All,
            },
            &rasterized.alpha,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(rasterized.width),
                rows_per_image: Some(rasterized.height),
            },
            wgpu::Extent3d {
                width: rasterized.width,
                height: rasterized.height,
                depth_or_array_layers: 1,
            },
        );

        let entry = AtlasEntry {
            region,
            placement_left: rasterized.placement_left,
            placement_top: rasterized.placement_top,
        };
        self.entries.insert(cache_key, entry);
        Ok(entry)
    }

    /// Convert atlas pixel region -> uv normalized [0..1].
    pub fn uv_min_max(&self, region: AtlasRegion) -> ([f32; 2], [f32; 2]) {
        let w = self.width as f32;
        let h = self.height as f32;
        let uv_min = [region.x as f32 / w, region.y as f32 / h];
        let uv_max = [
            (region.x + region.width) as f32 / w,
            (region.y + region.height) as f32 / h,
        ];
        (uv_min, uv_max)
    }

    /// Shelf packing đơn giản:
    /// - đặt glyph trên một hàng
    /// - hết ngang thì xuống hàng mới
    fn allocate_region(&mut self, width: u32, height: u32) -> Option<AtlasRegion> {
        if width == 0 || height == 0 {
            return None;
        }
        if width > self.width || height > self.height {
            return None;
        }

        if self.next_x + width > self.width {
            self.next_x = 0;
            self.next_y = self.next_y.saturating_add(self.row_height);
            self.row_height = 0;
        }

        if self.next_y + height > self.height {
            return None;
        }

        let region = AtlasRegion {
            x: self.next_x,
            y: self.next_y,
            width,
            height,
        };

        self.next_x += width;
        self.row_height = self.row_height.max(height);

        Some(region)
    }
}
