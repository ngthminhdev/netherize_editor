use bytemuck::{Pod, Zeroable};

/// Vertex tĩnh của một quad đơn vị (0..1).
/// Shader sẽ scale/translate bằng dữ liệu instance.
#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
pub struct GlyphVertex {
    pub local_pos: [f32; 2],
}

impl GlyphVertex {
    pub fn layout() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Self>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &wgpu::vertex_attr_array![0 => Float32x2],
        }
    }
}

pub const QUAD_VERTICES: [GlyphVertex; 4] = [
    GlyphVertex {
        local_pos: [0.0, 0.0],
    }, // top-left
    GlyphVertex {
        local_pos: [1.0, 0.0],
    }, // top-right
    GlyphVertex {
        local_pos: [1.0, 1.0],
    }, // bottom-right
    GlyphVertex {
        local_pos: [0.0, 1.0],
    }, // bottom-left
];

pub const QUAD_INDICES: [u16; 6] = [0, 1, 2, 0, 2, 3];

/// Dữ liệu instance cho mỗi glyph visible.
#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
pub struct GlyphInstance {
    pub screen_pos: [f32; 2],
    pub glyph_size: [f32; 2],
    pub uv_min: [f32; 2],
    pub uv_max: [f32; 2],
    pub color: [f32; 4],
}

impl GlyphInstance {
    pub fn new(
        screen_pos: [f32; 2],
        glyph_size: [f32; 2],
        uv_min: [f32; 2],
        uv_max: [f32; 2],
        color: [f32; 4],
    ) -> Self {
        Self {
            screen_pos,
            glyph_size,
            uv_min,
            uv_max,
            color,
        }
    }

    pub fn layout() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Self>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: &[
                wgpu::VertexAttribute {
                    offset: 0,
                    shader_location: 1,
                    format: wgpu::VertexFormat::Float32x2,
                },
                wgpu::VertexAttribute {
                    offset: std::mem::size_of::<[f32; 2]>() as u64,
                    shader_location: 2,
                    format: wgpu::VertexFormat::Float32x2,
                },
                wgpu::VertexAttribute {
                    offset: (std::mem::size_of::<[f32; 2]>() * 2) as u64,
                    shader_location: 3,
                    format: wgpu::VertexFormat::Float32x2,
                },
                wgpu::VertexAttribute {
                    offset: (std::mem::size_of::<[f32; 2]>() * 3) as u64,
                    shader_location: 4,
                    format: wgpu::VertexFormat::Float32x2,
                },
                wgpu::VertexAttribute {
                    offset: (std::mem::size_of::<[f32; 2]>() * 4) as u64,
                    shader_location: 5,
                    format: wgpu::VertexFormat::Float32x4,
                },
            ],
        }
    }
}
