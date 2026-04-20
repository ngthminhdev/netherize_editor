use std::{
    sync::Arc,
    time::{Duration, Instant},
};

use cosmic_text::Metrics;
use netherize_editor::{
    render::{glyph_instance::GlyphInstance, surface::SurfaceState, text_pipeline::TextPipeline},
    text::{atlas::GlyphAtlas, raster::rasterize_glyph_alpha, text_system::TextSystem},
};
use winit::{
    application::ApplicationHandler,
    dpi::PhysicalSize,
    event::WindowEvent,
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop},
    window::{Window, WindowId},
};

const WINDOW_WIDTH: u32 = 1000;
const WINDOW_HEIGHT: u32 = 700;
const WINDOW_TITLE: &str = "Netherize Phase3 - Text Window";
const PHASE3_TEXT: &str = "Hello Apple Silicon";

const TEXT_ORIGIN_X: f32 = 40.0;
const TEXT_ORIGIN_Y: f32 = 90.0;

const BG_COLOR: wgpu::Color = wgpu::Color {
    r: 0.06,
    g: 0.07,
    b: 0.10,
    a: 1.0,
};

const TEXT_COLOR: [f32; 4] = [0.92, 0.94, 0.98, 1.0];

struct Phase3Renderer {
    device: wgpu::Device,
    queue: wgpu::Queue,
    surface_state: SurfaceState,
    text_system: TextSystem,
    atlas: GlyphAtlas,
    text_pipeline: TextPipeline,
}

impl Phase3Renderer {
    async fn new(window: Arc<Window>) -> Result<Self, String> {
        let window_size = window.inner_size();

        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: wgpu::Backends::all(),
            ..wgpu::InstanceDescriptor::new_without_display_handle()
        });

        let surface = instance
            .create_surface(window)
            .map_err(|err| format!("create_surface failed: {err}"))?;

        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
            })
            .await
            .map_err(|err| format!("request_adapter failed: {err}"))?;

        let adapter_info = adapter.get_info();
        println!(
            "phase3 adapter: name='{}' type={:?} backend={:?}",
            adapter_info.name, adapter_info.device_type, adapter_info.backend
        );

        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: Some("Netherize Phase3 Device"),
                required_features: wgpu::Features::empty(),
                required_limits: wgpu::Limits::default(),
                experimental_features: Default::default(),
                memory_hints: Default::default(),
                trace: Default::default(),
            })
            .await
            .map_err(|err| format!("request_device failed: {err}"))?;

        let surface_state = SurfaceState::new(surface, window_size, &adapter, &device)?;

        let mut text_system = TextSystem::new(
            Metrics::new(38.0, 48.0),
            Some(surface_state.size.width as f32 - TEXT_ORIGIN_X * 2.0),
            Some(surface_state.size.height as f32),
        );
        text_system.set_text(PHASE3_TEXT);

        let atlas = GlyphAtlas::new(&device, 1024, 1024);
        let text_pipeline = TextPipeline::new(
            &device,
            surface_state.config.format,
            &atlas,
            surface_state.size.width,
            surface_state.size.height,
        );

        let mut renderer = Self {
            device,
            queue,
            surface_state,
            text_system,
            atlas,
            text_pipeline,
        };
        renderer.rebuild_text_instances()?;
        Ok(renderer)
    }

    fn resize(&mut self, new_size: PhysicalSize<u32>) -> Result<(), String> {
        if new_size.width == 0 || new_size.height == 0 {
            return Ok(());
        }

        self.surface_state.resize(&self.device, new_size);
        self.text_pipeline
            .update_screen_size(&self.queue, new_size.width, new_size.height);
        self.text_system.set_size(
            Some(new_size.width as f32 - TEXT_ORIGIN_X * 2.0),
            Some(new_size.height as f32),
        );
        self.rebuild_text_instances()
    }

    fn reconfigure_surface(&self) {
        self.surface_state.reconfigure(&self.device);
    }

    fn render(&mut self) -> Result<(), String> {
        let frame = match self.surface_state.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(surface_texture) => surface_texture,
            wgpu::CurrentSurfaceTexture::Suboptimal(surface_texture) => surface_texture,
            wgpu::CurrentSurfaceTexture::Timeout => return Ok(()),
            wgpu::CurrentSurfaceTexture::Occluded => return Ok(()),
            wgpu::CurrentSurfaceTexture::Outdated => {
                self.reconfigure_surface();
                return Ok(());
            }
            wgpu::CurrentSurfaceTexture::Lost => {
                self.reconfigure_surface();
                return Ok(());
            }
            wgpu::CurrentSurfaceTexture::Validation => {
                return Err("surface validation error".to_string());
            }
        };

        let view = frame
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Netherize Phase3 Encoder"),
            });

        {
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Netherize Phase3 RenderPass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(BG_COLOR),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });

            // Draw text instances (atlas sampled textured quads).
            self.text_pipeline.draw(&mut render_pass);
        }

        self.queue.submit(std::iter::once(encoder.finish()));
        frame.present();
        Ok(())
    }

    /// Nối CPU layout/raster với GPU atlas/pipeline:
    /// 1) lấy glyph positions từ TextSystem
    /// 2) rasterize glyph chưa có trong atlas
    /// 3) build instance buffer để GPU vẽ
    fn rebuild_text_instances(&mut self) -> Result<(), String> {
        let visible_glyphs =
            self.text_system
                .collect_visible_glyphs(TEXT_ORIGIN_X, TEXT_ORIGIN_Y, TEXT_COLOR);
        let visible_count = visible_glyphs.len();
        let mut instances = Vec::with_capacity(visible_glyphs.len());

        for glyph in visible_glyphs {
            let entry = if let Some(entry) = self.atlas.get(glyph.cache_key) {
                entry
            } else {
                let Some(rasterized) =
                    rasterize_glyph_alpha(&mut self.text_system, glyph.cache_key)
                else {
                    // Glyph rỗng (space, etc.) thì bỏ qua.
                    continue;
                };
                self.atlas
                    .get_or_insert(&self.queue, glyph.cache_key, &rasterized)?
            };

            if entry.region.width == 0 || entry.region.height == 0 {
                continue;
            }

            let (uv_min, uv_max) = self.atlas.uv_min_max(entry.region);
            let top_left_x = glyph.physical_x + entry.placement_left;
            let top_left_y = glyph.physical_y - entry.placement_top;

            instances.push(GlyphInstance::new(
                [top_left_x as f32, top_left_y as f32],
                [entry.region.width as f32, entry.region.height as f32],
                uv_min,
                uv_max,
                glyph.color,
            ));
        }

        self.text_pipeline
            .upload_instances(&self.device, &self.queue, &instances);

        println!(
            "phase3 instances rebuilt: visible={} drawable={}",
            visible_count,
            instances.len()
        );

        Ok(())
    }
}

struct Phase3App {
    window: Option<Arc<Window>>,
    renderer: Option<Phase3Renderer>,
    fps_debug: FpsDebug,
}

impl Phase3App {
    fn new(debug_fps: bool) -> Self {
        Self {
            window: None,
            renderer: None,
            fps_debug: FpsDebug::new(debug_fps),
        }
    }
}

struct FpsDebug {
    enabled: bool,
    frame_count: u32,
    last_sample: Instant,
}

impl FpsDebug {
    fn new(enabled: bool) -> Self {
        Self {
            enabled,
            frame_count: 0,
            last_sample: Instant::now(),
        }
    }

    /// Tick mỗi frame; trả về FPS trung bình theo cửa sổ thời gian ngắn
    /// để tránh update title quá dày.
    fn tick(&mut self) -> Option<f32> {
        if !self.enabled {
            return None;
        }

        self.frame_count += 1;
        let elapsed = self.last_sample.elapsed();
        if elapsed < Duration::from_millis(400) {
            return None;
        }

        let fps = self.frame_count as f32 / elapsed.as_secs_f32();
        self.frame_count = 0;
        self.last_sample = Instant::now();
        Some(fps)
    }
}

impl ApplicationHandler for Phase3App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }

        let window_attributes = Window::default_attributes()
            .with_title(WINDOW_TITLE)
            .with_inner_size(PhysicalSize::new(WINDOW_WIDTH, WINDOW_HEIGHT));

        let window = match event_loop.create_window(window_attributes) {
            Ok(window) => Arc::new(window),
            Err(err) => {
                eprintln!("create_window failed: {err}");
                event_loop.exit();
                return;
            }
        };

        let renderer = match pollster::block_on(Phase3Renderer::new(window.clone())) {
            Ok(renderer) => renderer,
            Err(err) => {
                eprintln!("phase3 renderer init failed: {err}");
                event_loop.exit();
                return;
            }
        };

        println!("Phase 3 ready: rendering text '{}'", PHASE3_TEXT);
        if self.fps_debug.enabled {
            println!("FPS debug enabled. Window title will show live FPS.");
        }
        self.window = Some(window);
        self.renderer = Some(renderer);
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        window_id: WindowId,
        event: WindowEvent,
    ) {
        if self
            .window
            .as_ref()
            .is_some_and(|window| window.id() != window_id)
        {
            return;
        }

        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(new_size) => {
                if let Some(renderer) = self.renderer.as_mut() {
                    if let Err(err) = renderer.resize(new_size) {
                        eprintln!("resize failed: {err}");
                    }
                }
            }
            WindowEvent::RedrawRequested => {
                if let Some(renderer) = self.renderer.as_mut() {
                    if let Err(err) = renderer.render() {
                        eprintln!("render failed: {err}");
                    } else if let Some(fps) = self.fps_debug.tick() {
                        if let Some(window) = &self.window {
                            window.set_title(&format!("{WINDOW_TITLE} | {:.1} FPS", fps));
                        }
                        println!("fps={:.1}", fps);
                    }
                }
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        if let Some(window) = &self.window {
            window.request_redraw();
        }
    }
}

fn main() {
    let debug_fps = std::env::args().any(|arg| arg == "--debug-fps");

    let event_loop = EventLoop::new().expect("failed to create event loop");
    event_loop.set_control_flow(ControlFlow::Poll);

    if debug_fps {
        println!("debug flag active: --debug-fps");
    }

    let mut app = Phase3App::new(debug_fps);
    if let Err(err) = event_loop.run_app(&mut app) {
        eprintln!("phase3_text_window exited with error: {err}");
    }
}
