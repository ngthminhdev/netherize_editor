use bytemuck::{Pod, Zeroable};
use wgpu::util::DeviceExt;

use crate::render::color_space::srgb_color_target_state;

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
struct ImageVertex {
    position: [f32; 2],
    uv: [f32; 2],
}

impl ImageVertex {
    const ATTRIBUTES: [wgpu::VertexAttribute; 2] =
        wgpu::vertex_attr_array![0 => Float32x2, 1 => Float32x2];

    fn layout() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Self>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &Self::ATTRIBUTES,
        }
    }
}

const INDICES: [u16; 6] = [0, 1, 2, 0, 2, 3];

pub struct ImagePipeline {
    render_pipeline: wgpu::RenderPipeline,
    bind_group_layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    vertex_buffer: Option<wgpu::Buffer>,
    index_buffer: wgpu::Buffer,
    bind_group: Option<wgpu::BindGroup>,
    has_image: bool,
}

impl ImagePipeline {
    pub fn new(device: &wgpu::Device, surface_format: wgpu::TextureFormat) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Netherize Image Shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shaders/image.wgsl").into()),
        });
        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Netherize Image BindGroupLayout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Netherize Image PipelineLayout"),
            bind_group_layouts: &[Some(&bind_group_layout)],
            immediate_size: 0,
        });
        let render_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Netherize Image Pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[ImageVertex::layout()],
            },
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                cull_mode: None,
                ..Default::default()
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                compilation_options: Default::default(),
                targets: &[Some(srgb_color_target_state(surface_format))],
            }),
            multiview_mask: None,
            cache: None,
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("Netherize Image Sampler"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Nearest,
            ..Default::default()
        });
        let index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Netherize Image Index Buffer"),
            contents: bytemuck::cast_slice(&INDICES),
            usage: wgpu::BufferUsages::INDEX,
        });
        Self {
            render_pipeline,
            bind_group_layout,
            sampler,
            vertex_buffer: None,
            index_buffer,
            bind_group: None,
            has_image: false,
        }
    }

    pub fn clear(&mut self) {
        self.vertex_buffer = None;
        self.bind_group = None;
        self.has_image = false;
    }

    pub fn upload_rgba(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        rgba: &[u8],
        width: u32,
        height: u32,
        rect: [f32; 4],
        viewport: [u32; 2],
    ) {
        if width == 0 || height == 0 || rgba.is_empty() {
            self.clear();
            return;
        }
        let tex = device.create_texture_with_data(
            queue,
            &wgpu::TextureDescriptor {
                label: Some("Netherize Image Texture"),
                size: wgpu::Extent3d {
                    width,
                    height,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8UnormSrgb,
                usage: wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            },
            wgpu::util::TextureDataOrder::LayerMajor,
            rgba,
        );
        let view = tex.create_view(&wgpu::TextureViewDescriptor::default());
        self.bind_group = Some(device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Netherize Image BindGroup"),
            layout: &self.bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
            ],
        }));
        let [vw, vh] = [viewport[0].max(1) as f32, viewport[1].max(1) as f32];
        let x0 = rect[0] / vw * 2.0 - 1.0;
        let x1 = (rect[0] + rect[2]) / vw * 2.0 - 1.0;
        let y0 = 1.0 - (rect[1] + rect[3]) / vh * 2.0;
        let y1 = 1.0 - rect[1] / vh * 2.0;
        let vertices = [
            ImageVertex {
                position: [x0, y0],
                uv: [0.0, 1.0],
            },
            ImageVertex {
                position: [x1, y0],
                uv: [1.0, 1.0],
            },
            ImageVertex {
                position: [x1, y1],
                uv: [1.0, 0.0],
            },
            ImageVertex {
                position: [x0, y1],
                uv: [0.0, 0.0],
            },
        ];
        self.vertex_buffer = Some(
            device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("Netherize Image Vertex Buffer"),
                contents: bytemuck::cast_slice(&vertices),
                usage: wgpu::BufferUsages::VERTEX,
            }),
        );
        self.has_image = true;
    }

    pub fn draw<'pass>(&'pass self, pass: &mut wgpu::RenderPass<'pass>) {
        if !self.has_image {
            return;
        }
        let (Some(vb), Some(bg)) = (&self.vertex_buffer, &self.bind_group) else {
            return;
        };
        pass.set_pipeline(&self.render_pipeline);
        pass.set_bind_group(0, bg, &[]);
        pass.set_vertex_buffer(0, vb.slice(..));
        pass.set_index_buffer(self.index_buffer.slice(..), wgpu::IndexFormat::Uint16);
        pass.draw_indexed(0..6, 0, 0..1);
    }
}
