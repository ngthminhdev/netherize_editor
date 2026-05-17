use std::collections::HashMap;

use bytemuck::{Pod, Zeroable};
use wgpu::util::DeviceExt;

use crate::render::color_space::srgb_color_target_state;

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
struct IconVertex {
    position: [f32; 2],
    uv: [f32; 2],
    color: [f32; 4],
}

impl IconVertex {
    const ATTRIBUTES: [wgpu::VertexAttribute; 3] =
        wgpu::vertex_attr_array![0 => Float32x2, 1 => Float32x2, 2 => Float32x4];

    fn layout() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Self>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &Self::ATTRIBUTES,
        }
    }
}

pub fn canonical_icon_id(icon: &str) -> Option<&'static str> {
    let asset_icon = icon.strip_prefix("built_in:")?;
    Some(match asset_icon {
        "ansible" => "built_in:ansible",
        "astro" => "built_in:astro",
        "c" => "built_in:c",
        "cargo" => "built_in:cargo",
        "cargolock" => "built_in:cargolock",
        "clojure" => "built_in:clojure",
        "cmake" => "built_in:cmake",
        "conf" | "config" => "built_in:conf",
        "cpp" => "built_in:cpp",
        "csharp" => "built_in:csharp",
        "css" => "built_in:css",
        "dart" | "dartlang" => "built_in:dart",
        "docker" => "built_in:docker",
        "elm" => "built_in:elm",
        "file" => "built_in:file",
        "folder" => "built_in:folder",
        "folder_open" => "built_in:folder_open",
        "fsharp" => "built_in:fsharp",
        "git" => "built_in:git",
        "go" => "built_in:go",
        "gradle" => "built_in:gradle",
        "graphql" => "built_in:graphql",
        "haskell" => "built_in:haskell",
        "hash" => "built_in:hash",
        "html" => "built_in:html",
        "identifier" | "symbol" | "function" | "method" | "property" | "field" | "variable" | "constant" | "class" | "interface" | "struct" | "enum" | "reference" | "event" | "operator" | "type_parameter" => "built_in:identifier",
        "image" => "built_in:image",
        "info" | "keyword" | "text" | "unit" | "value" | "color" | "snippet" => "built_in:info",
        "java" => "built_in:java",
        "node" | "javascript" => "built_in:node",
        "json" => "built_in:json",
        "key" => "built_in:key",
        "kotlin" => "built_in:kotlin",
        "lock" => "built_in:lock",
        "lua" => "built_in:lua",
        "makefile" => "built_in:makefile",
        "markdown" | "readme" => "built_in:markdown",
        "nginx" => "built_in:nginx",
        "nim" => "built_in:nim",
        "npm" => "built_in:npm",
        "ocaml" => "built_in:ocaml",
        "perl" => "built_in:perl",
        "php" => "built_in:php",
        "proto" => "built_in:proto",
        "python" => "built_in:python",
        "r" => "built_in:r",
        "reactjs" | "jsx" => "built_in:reactjs",
        "ruby" => "built_in:ruby",
        "rust" => "built_in:rust",
        "sass" | "scss" => "built_in:sass",
        "scala" => "built_in:scala",
        "shell" => "built_in:shell",
        "sol" | "solidity" => "built_in:sol",
        "sql" => "built_in:sql",
        "svelte" => "built_in:svelte",
        "swift" => "built_in:swift",
        "terraform" => "built_in:terraform",
        "todo" => "built_in:todo",
        "toml" => "built_in:toml",
        "tsx" => "built_in:tsx",
        "typescript" => "built_in:typescript",
        "vue" => "built_in:vue",
        "xml" => "built_in:xml",
        "yaml" => "built_in:yaml",
        "zig" => "built_in:zig",
        _ => "built_in:file",
    })
}

#[derive(Clone, Copy, Debug)]
pub struct IconDrawInstance {
    pub icon: &'static str,
    pub rect: [f32; 4],
    pub tint: [f32; 4],
}

#[derive(Clone, Copy, Debug)]
struct AtlasEntry {
    x: u32,
    y: u32,
    w: u32,
    h: u32,
}

pub struct IconPipeline {
    render_pipeline: wgpu::RenderPipeline,

    vertex_buffer: Option<wgpu::Buffer>,
    index_buffer: Option<wgpu::Buffer>,
    bind_group: Option<wgpu::BindGroup>,
    index_count: u32,
    atlas_size: u32,
    entries: HashMap<&'static str, AtlasEntry>,
}

impl IconPipeline {
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue, surface_format: wgpu::TextureFormat) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Netherize Icon Shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shaders/icon.wgsl").into()),
        });
        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Netherize Icon BindGroupLayout"),
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
            label: Some("Netherize Icon PipelineLayout"),
            bind_group_layouts: &[Some(&bind_group_layout)],
            immediate_size: 0,
        });
        let render_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Netherize Icon Pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[IconVertex::layout()],
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
            label: Some("Netherize Icon Sampler"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Nearest,
            ..Default::default()
        });

        let atlas = build_bearded_atlas();
        let texture = device.create_texture_with_data(
            queue,
            &wgpu::TextureDescriptor {
                label: Some("Netherize Bearded Icon Atlas"),
                size: wgpu::Extent3d {
                    width: atlas.size,
                    height: atlas.size,
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
            &atlas.rgba,
        );
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let bind_group = Some(device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Netherize Icon BindGroup"),
            layout: &bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
            ],
        }));

        Self {
            render_pipeline,
            vertex_buffer: None,
            index_buffer: None,
            bind_group,
            index_count: 0,
            atlas_size: atlas.size,
            entries: atlas.entries,
        }
    }

    pub fn upload_instances(
        &mut self,
        device: &wgpu::Device,
        instances: &[IconDrawInstance],
        viewport: [u32; 2],
    ) {
        if instances.is_empty() {
            self.vertex_buffer = None;
            self.index_buffer = None;
            self.index_count = 0;
            return;
        }

        let [vw, vh] = [viewport[0].max(1) as f32, viewport[1].max(1) as f32];
        let mut vertices = Vec::with_capacity(instances.len() * 4);
        let mut indices = Vec::with_capacity(instances.len() * 6);

        for instance in instances {
            let Some(entry) = self.entries.get(instance.icon) else {
                continue;
            };
            let [x, y, w, h] = instance.rect;
            let x0 = x / vw * 2.0 - 1.0;
            let x1 = (x + w) / vw * 2.0 - 1.0;
            let y0 = 1.0 - (y + h) / vh * 2.0;
            let y1 = 1.0 - y / vh * 2.0;
            let s = self.atlas_size as f32;
            let u0 = entry.x as f32 / s;
            let v0 = entry.y as f32 / s;
            let u1 = (entry.x + entry.w) as f32 / s;
            let v1 = (entry.y + entry.h) as f32 / s;
            let base = vertices.len() as u32;
            vertices.extend_from_slice(&[
                IconVertex { position: [x0, y0], uv: [u0, v1], color: instance.tint },
                IconVertex { position: [x1, y0], uv: [u1, v1], color: instance.tint },
                IconVertex { position: [x1, y1], uv: [u1, v0], color: instance.tint },
                IconVertex { position: [x0, y1], uv: [u0, v0], color: instance.tint },
            ]);
            indices.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
        }

        if vertices.is_empty() {
            self.vertex_buffer = None;
            self.index_buffer = None;
            self.index_count = 0;
            return;
        }

        self.vertex_buffer = Some(device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Netherize Icon Vertex Buffer"),
            contents: bytemuck::cast_slice(&vertices),
            usage: wgpu::BufferUsages::VERTEX,
        }));
        self.index_buffer = Some(device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Netherize Icon Index Buffer"),
            contents: bytemuck::cast_slice(&indices),
            usage: wgpu::BufferUsages::INDEX,
        }));
        self.index_count = indices.len() as u32;
    }

    pub fn draw<'pass>(&'pass self, pass: &mut wgpu::RenderPass<'pass>) {
        if self.index_count == 0 {
            return;
        }
        let (Some(vb), Some(ib), Some(bg)) = (&self.vertex_buffer, &self.index_buffer, &self.bind_group) else {
            return;
        };
        pass.set_pipeline(&self.render_pipeline);
        pass.set_bind_group(0, bg, &[]);
        pass.set_vertex_buffer(0, vb.slice(..));
        pass.set_index_buffer(ib.slice(..), wgpu::IndexFormat::Uint32);
        pass.draw_indexed(0..self.index_count, 0, 0..1);
    }
}

struct BuiltAtlas {
    size: u32,
    rgba: Vec<u8>,
    entries: HashMap<&'static str, AtlasEntry>,
}

fn build_bearded_atlas() -> BuiltAtlas {
    const ICONS: &[(&str, &[u8])] = &[
        ("built_in:ansible", include_bytes!("../../assets/bearded-icons/conf.svg")),
        ("built_in:astro", include_bytes!("../../assets/bearded-icons/astro.svg")),
        ("built_in:c", include_bytes!("../../assets/bearded-icons/c.svg")),
        ("built_in:cargo", include_bytes!("../../assets/bearded-icons/cargo.svg")),
        ("built_in:cargolock", include_bytes!("../../assets/bearded-icons/cargolock.svg")),
        ("built_in:clojure", include_bytes!("../../assets/bearded-icons/clojure.svg")),
        ("built_in:cmake", include_bytes!("../../assets/bearded-icons/cmake.svg")),
        ("built_in:conf", include_bytes!("../../assets/bearded-icons/conf.svg")),
        ("built_in:cpp", include_bytes!("../../assets/bearded-icons/cpp.svg")),
        ("built_in:csharp", include_bytes!("../../assets/bearded-icons/csharp.svg")),
        ("built_in:css", include_bytes!("../../assets/bearded-icons/css.svg")),
        ("built_in:dart", include_bytes!("../../assets/bearded-icons/dartlang.svg")),
        ("built_in:docker", include_bytes!("../../assets/bearded-icons/docker.svg")),
        ("built_in:elm", include_bytes!("../../assets/bearded-icons/elm.svg")),
        ("built_in:file", include_bytes!("../../assets/bearded-icons/file.svg")),
        ("built_in:folder", include_bytes!("../../assets/bearded-icons/folder.svg")),
        ("built_in:folder_open", include_bytes!("../../assets/bearded-icons/folder_open.svg")),
        ("built_in:fsharp", include_bytes!("../../assets/bearded-icons/fsharp.svg")),
        ("built_in:git", include_bytes!("../../assets/bearded-icons/git.svg")),
        ("built_in:go", include_bytes!("../../assets/bearded-icons/go.svg")),
        ("built_in:gradle", include_bytes!("../../assets/bearded-icons/gradle.svg")),
        ("built_in:graphql", include_bytes!("../../assets/bearded-icons/graphql.svg")),
        ("built_in:haskell", include_bytes!("../../assets/bearded-icons/haskell.svg")),
        ("built_in:hash", include_bytes!("../../assets/bearded-icons/hash.svg")),
        ("built_in:html", include_bytes!("../../assets/bearded-icons/html.svg")),
        ("built_in:identifier", include_bytes!("../../assets/bearded-icons/identifier.svg")),
        ("built_in:image", include_bytes!("../../assets/bearded-icons/image.svg")),
        ("built_in:info", include_bytes!("../../assets/bearded-icons/info.svg")),
        ("built_in:java", include_bytes!("../../assets/bearded-icons/java.svg")),
        ("built_in:node", include_bytes!("../../assets/bearded-icons/node.svg")),
        ("built_in:json", include_bytes!("../../assets/bearded-icons/json.svg")),
        ("built_in:key", include_bytes!("../../assets/bearded-icons/key.svg")),
        ("built_in:kotlin", include_bytes!("../../assets/bearded-icons/kotlin.svg")),
        ("built_in:lock", include_bytes!("../../assets/bearded-icons/lock.svg")),
        ("built_in:lua", include_bytes!("../../assets/bearded-icons/lua.svg")),
        ("built_in:makefile", include_bytes!("../../assets/bearded-icons/makefile.svg")),
        ("built_in:markdown", include_bytes!("../../assets/bearded-icons/markdown.svg")),
        ("built_in:nginx", include_bytes!("../../assets/bearded-icons/nginx.svg")),
        ("built_in:nim", include_bytes!("../../assets/bearded-icons/nim.svg")),
        ("built_in:npm", include_bytes!("../../assets/bearded-icons/npm.svg")),
        ("built_in:ocaml", include_bytes!("../../assets/bearded-icons/ocaml.svg")),
        ("built_in:perl", include_bytes!("../../assets/bearded-icons/perl.svg")),
        ("built_in:php", include_bytes!("../../assets/bearded-icons/php.svg")),
        ("built_in:proto", include_bytes!("../../assets/bearded-icons/proto.svg")),
        ("built_in:python", include_bytes!("../../assets/bearded-icons/python.svg")),
        ("built_in:r", include_bytes!("../../assets/bearded-icons/r.svg")),
        ("built_in:reactjs", include_bytes!("../../assets/bearded-icons/reactjs.svg")),
        ("built_in:ruby", include_bytes!("../../assets/bearded-icons/ruby.svg")),
        ("built_in:rust", include_bytes!("../../assets/bearded-icons/rust.svg")),
        ("built_in:sass", include_bytes!("../../assets/bearded-icons/sass.svg")),
        ("built_in:scala", include_bytes!("../../assets/bearded-icons/scala.svg")),
        ("built_in:shell", include_bytes!("../../assets/bearded-icons/shell.svg")),
        ("built_in:sol", include_bytes!("../../assets/bearded-icons/sol.svg")),
        ("built_in:sql", include_bytes!("../../assets/bearded-icons/sql.svg")),
        ("built_in:svelte", include_bytes!("../../assets/bearded-icons/svelte.svg")),
        ("built_in:swift", include_bytes!("../../assets/bearded-icons/swift.svg")),
        ("built_in:terraform", include_bytes!("../../assets/bearded-icons/terraform.svg")),
        ("built_in:todo", include_bytes!("../../assets/bearded-icons/todo.svg")),
        ("built_in:toml", include_bytes!("../../assets/bearded-icons/toml.svg")),
        ("built_in:tsx", include_bytes!("../../assets/bearded-icons/tsx.svg")),
        ("built_in:typescript", include_bytes!("../../assets/bearded-icons/typescript.svg")),
        ("built_in:vue", include_bytes!("../../assets/bearded-icons/vue.svg")),
        ("built_in:xml", include_bytes!("../../assets/bearded-icons/xml.svg")),
        ("built_in:yaml", include_bytes!("../../assets/bearded-icons/yaml.svg")),
        ("built_in:zig", include_bytes!("../../assets/bearded-icons/zig.svg")), 
    ];
    let icon_size = 96u32;
    let padding = 8u32;
    let cell = icon_size + padding * 2;
    let cols = 8u32;
    let rows = ((ICONS.len() as u32) + cols - 1) / cols;
    let size = (cols.max(rows) * cell).next_power_of_two().max(1024);
    let mut rgba = vec![0u8; (size * size * 4) as usize];
    let mut entries = HashMap::new();

    for (idx, (id, svg)) in ICONS.iter().enumerate() {
        let col = idx as u32 % cols;
        let row = idx as u32 / cols;
        let x = col * cell + padding;
        let y = row * cell + padding;
        let icon = rasterize_svg(svg, icon_size, icon_size).unwrap_or_else(|| vec![0; (icon_size * icon_size * 4) as usize]);
        blit_rgba(&mut rgba, size, x, y, icon_size, icon_size, &icon);
        entries.insert(*id, AtlasEntry { x, y, w: icon_size, h: icon_size });
    }

    BuiltAtlas { size, rgba, entries }
}

fn rasterize_svg(svg: &[u8], width: u32, height: u32) -> Option<Vec<u8>> {
    let options = usvg::Options::default();
    let tree = usvg::Tree::from_data(svg, &options).ok()?;
    let mut pixmap = tiny_skia::Pixmap::new(width, height)?;
    let tree_size = tree.size();
    let scale = (width as f32 / tree_size.width()).min(height as f32 / tree_size.height());
    let tx = (width as f32 - tree_size.width() * scale) * 0.5;
    let ty = (height as f32 - tree_size.height() * scale) * 0.5;
    let transform = tiny_skia::Transform::from_translate(tx, ty).pre_scale(scale, scale);
    resvg::render(&tree, transform, &mut pixmap.as_mut());
    Some(pixmap.take())
}

fn blit_rgba(dst: &mut [u8], dst_size: u32, x: u32, y: u32, w: u32, h: u32, src: &[u8]) {
    for row in 0..h {
        let dst_start = (((y + row) * dst_size + x) * 4) as usize;
        let src_start = (row * w * 4) as usize;
        let len = (w * 4) as usize;
        if dst_start + len <= dst.len() && src_start + len <= src.len() {
            dst[dst_start..dst_start + len].copy_from_slice(&src[src_start..src_start + len]);
        }
    }
}
