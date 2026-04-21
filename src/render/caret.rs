use bytemuck::{Pod, Zeroable};
use wgpu::util::DeviceExt;

use crate::render::color_space::srgb_color_target_state;

#[derive(Debug, Clone, Copy)]
pub struct CaretScreenRect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub color: [f32; 4],
}

impl CaretScreenRect {
    pub fn from_layout(x: f32, top: f32, height: f32, width: f32, color: [f32; 4]) -> Self {
        Self {
            x,
            y: top,
            width,
            height,
            color,
        }
    }
}

#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
struct CaretVertex {
    local_pos: [f32; 2],
}

impl CaretVertex {
    fn layout() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Self>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &wgpu::vertex_attr_array![0 => Float32x2],
        }
    }
}

const CARET_VERTICES: [CaretVertex; 4] = [
    CaretVertex {
        local_pos: [0.0, 0.0],
    },
    CaretVertex {
        local_pos: [1.0, 0.0],
    },
    CaretVertex {
        local_pos: [1.0, 1.0],
    },
    CaretVertex {
        local_pos: [0.0, 1.0],
    },
];

const CARET_INDICES: [u16; 6] = [0, 1, 2, 0, 2, 3];

#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
struct CaretInstance {
    screen_pos: [f32; 2],
    caret_size: [f32; 2],
    color: [f32; 4],
}

impl CaretInstance {
    fn from_rect(rect: CaretScreenRect) -> Self {
        Self {
            screen_pos: [rect.x, rect.y],
            caret_size: [rect.width.max(1.0), rect.height.max(1.0)],
            color: rect.color,
        }
    }

    fn layout() -> wgpu::VertexBufferLayout<'static> {
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
                    format: wgpu::VertexFormat::Float32x4,
                },
            ],
        }
    }
}

#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
struct ScreenUniform {
    screen_size: [f32; 4],
}

impl ScreenUniform {
    fn new(width: u32, height: u32) -> Self {
        Self {
            screen_size: [width as f32, height as f32, 0.0, 0.0],
        }
    }
}

/// Pipeline tối thiểu để vẽ caret hình chữ nhật trên màn hình.
pub struct CaretPipeline {
    render_pipeline: wgpu::RenderPipeline,
    bind_group: wgpu::BindGroup,
    screen_uniform_buffer: wgpu::Buffer,
    vertex_buffer: wgpu::Buffer,
    index_buffer: wgpu::Buffer,
    index_count: u32,
    instance_buffer: wgpu::Buffer,
    instance_count: u32,
}

impl CaretPipeline {
    pub fn new(
        device: &wgpu::Device,
        surface_format: wgpu::TextureFormat,
        screen_width: u32,
        screen_height: u32,
    ) -> Self {
        let screen_uniform = ScreenUniform::new(screen_width.max(1), screen_height.max(1));
        let screen_uniform_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Netherize Caret Screen Uniform"),
            contents: bytemuck::bytes_of(&screen_uniform),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Netherize Caret Bind Group Layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Netherize Caret Bind Group"),
            layout: &bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: screen_uniform_buffer.as_entire_binding(),
            }],
        });

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Netherize Caret Shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shaders/caret.wgsl").into()),
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Netherize Caret Pipeline Layout"),
            bind_group_layouts: &[Some(&bind_group_layout)],
            immediate_size: 0,
        });

        let render_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Netherize Caret Render Pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[CaretVertex::layout(), CaretInstance::layout()],
            },
            primitive: wgpu::PrimitiveState::default(),
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

        let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Netherize Caret Vertex Buffer"),
            contents: bytemuck::cast_slice(&CARET_VERTICES),
            usage: wgpu::BufferUsages::VERTEX,
        });
        let index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Netherize Caret Index Buffer"),
            contents: bytemuck::cast_slice(&CARET_INDICES),
            usage: wgpu::BufferUsages::INDEX,
        });

        let instance_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Netherize Caret Instance Buffer"),
            size: std::mem::size_of::<CaretInstance>() as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        Self {
            render_pipeline,
            bind_group,
            screen_uniform_buffer,
            vertex_buffer,
            index_buffer,
            index_count: CARET_INDICES.len() as u32,
            instance_buffer,
            instance_count: 0,
        }
    }

    pub fn update_screen_size(&self, queue: &wgpu::Queue, width: u32, height: u32) {
        let uniform = ScreenUniform::new(width.max(1), height.max(1));
        queue.write_buffer(&self.screen_uniform_buffer, 0, bytemuck::bytes_of(&uniform));
    }

    pub fn upload_caret(&mut self, queue: &wgpu::Queue, rect: Option<CaretScreenRect>) {
        match rect {
            Some(rect) => {
                let instance = CaretInstance::from_rect(rect);
                queue.write_buffer(&self.instance_buffer, 0, bytemuck::bytes_of(&instance));
                self.instance_count = 1;
            }
            None => {
                self.instance_count = 0;
            }
        }
    }

    pub fn draw<'pass>(&'pass self, render_pass: &mut wgpu::RenderPass<'pass>) {
        if self.instance_count == 0 {
            return;
        }

        render_pass.set_pipeline(&self.render_pipeline);
        render_pass.set_bind_group(0, &self.bind_group, &[]);
        render_pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
        render_pass.set_vertex_buffer(1, self.instance_buffer.slice(..));
        render_pass.set_index_buffer(self.index_buffer.slice(..), wgpu::IndexFormat::Uint16);
        render_pass.draw_indexed(0..self.index_count, 0, 0..self.instance_count);
    }
}
