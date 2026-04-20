use std::{path::PathBuf, sync::Arc};

use cosmic_text::Metrics;
use netherize_editor::{
    app::{
        app_state::AppState,
        editor_view_model::{EditorViewModel, ViewSyncStats, ViewportState},
        input::{InputHandler, InputRouteOutcome},
        input_map::{InputMap, KeybindingContext},
    },
    core::commands::Command,
    render::{caret::CaretPipeline, surface::SurfaceState, text_pipeline::TextPipeline},
    text::{atlas::GlyphAtlas, text_system::TextSystem},
};
use winit::{
    application::ApplicationHandler,
    dpi::PhysicalSize,
    event::{Ime, WindowEvent},
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop},
    window::{Window, WindowId},
};

const WINDOW_WIDTH: u32 = 1120;
const WINDOW_HEIGHT: u32 = 760;
const WINDOW_TITLE: &str = "Netherize Phase3 Slice - Open/Edit/Save";
const DEFAULT_DEMO_PATH: &str = "phase3_slice_demo.txt";
const DEFAULT_DEMO_CONTENT: &str =
    "Phase3 slice demo file.\nOpen -> Edit -> Save with Cmd/Ctrl+S.\n";

const VIEWPORT_ORIGIN_X: f32 = 40.0;
const VIEWPORT_ORIGIN_Y: f32 = 90.0;

const BG_COLOR: wgpu::Color = wgpu::Color {
    r: 0.06,
    g: 0.07,
    b: 0.10,
    a: 1.0,
};
const TEXT_COLOR: [f32; 4] = [0.92, 0.94, 0.98, 1.0];
const CARET_COLOR: [f32; 4] = [0.98, 0.62, 0.20, 0.95];
const LARGE_FILE_LINE_THRESHOLD: usize = 50_000;
const LARGE_FILE_BYTE_THRESHOLD: usize = 1_000_000;
const LARGE_FILE_RENDER_PREVIEW_CHARS: usize = 16_000;

struct SliceRenderer {
    device: wgpu::Device,
    queue: wgpu::Queue,
    surface_state: SurfaceState,
    text_system: TextSystem,
    atlas: GlyphAtlas,
    text_pipeline: TextPipeline,
    caret_pipeline: CaretPipeline,
}

impl SliceRenderer {
    async fn new(window: Arc<Window>, viewport_origin: [f32; 2]) -> Result<Self, String> {
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

        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: Some("Netherize Slice Device"),
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
            Metrics::new(34.0, 44.0),
            Some((window_size.width as f32 - viewport_origin[0] * 2.0).max(1.0)),
            Some(window_size.height as f32),
        );
        text_system.set_text("");

        let atlas = GlyphAtlas::new(&device, 2048, 2048);
        let text_pipeline = TextPipeline::new(
            &device,
            surface_state.config.format,
            &atlas,
            surface_state.size.width,
            surface_state.size.height,
        );
        let caret_pipeline = CaretPipeline::new(
            &device,
            surface_state.config.format,
            surface_state.size.width,
            surface_state.size.height,
        );

        Ok(Self {
            device,
            queue,
            surface_state,
            text_system,
            atlas,
            text_pipeline,
            caret_pipeline,
        })
    }

    fn resize(&mut self, new_size: PhysicalSize<u32>, viewport_origin: [f32; 2]) {
        if new_size.width == 0 || new_size.height == 0 {
            return;
        }

        self.surface_state.resize(&self.device, new_size);
        self.text_pipeline
            .update_screen_size(&self.queue, new_size.width, new_size.height);
        self.caret_pipeline
            .update_screen_size(&self.queue, new_size.width, new_size.height);
        self.text_system.set_size(
            Some((new_size.width as f32 - viewport_origin[0] * 2.0).max(1.0)),
            Some(new_size.height as f32),
        );
    }

    fn sync_view_model(
        &mut self,
        app_state: &AppState,
        render_text: &str,
        view_model: &mut EditorViewModel,
    ) -> Result<ViewSyncStats, String> {
        let sync_stats = view_model.sync_projection_with_text(
            app_state,
            &mut self.text_system,
            &mut self.atlas,
            &self.queue,
            render_text,
        )?;
        self.text_pipeline.upload_instances(
            &self.device,
            &self.queue,
            view_model.glyph_instances(),
        );
        self.caret_pipeline
            .upload_caret(&self.queue, view_model.caret_rect());
        Ok(sync_stats)
    }

    fn render(&mut self) -> Result<(), String> {
        let frame = match self.surface_state.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(surface_texture) => surface_texture,
            wgpu::CurrentSurfaceTexture::Suboptimal(surface_texture) => surface_texture,
            wgpu::CurrentSurfaceTexture::Timeout => return Ok(()),
            wgpu::CurrentSurfaceTexture::Occluded => return Ok(()),
            wgpu::CurrentSurfaceTexture::Outdated => {
                self.surface_state.reconfigure(&self.device);
                return Ok(());
            }
            wgpu::CurrentSurfaceTexture::Lost => {
                self.surface_state.reconfigure(&self.device);
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
                label: Some("Netherize Phase3 Slice Encoder"),
            });

        {
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Netherize Phase3 Slice RenderPass"),
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

            self.text_pipeline.draw(&mut render_pass);
            self.caret_pipeline.draw(&mut render_pass);
        }

        self.queue.submit(std::iter::once(encoder.finish()));
        frame.present();
        Ok(())
    }
}

struct Phase3SliceApp {
    target_path: PathBuf,
    window: Option<Arc<Window>>,
    renderer: Option<SliceRenderer>,
    input_handler: InputHandler,
    input_map: InputMap,
    app_state: AppState,
    view_model: EditorViewModel,
    large_file_mode: bool,
    large_file_reason: Option<String>,
}

impl Phase3SliceApp {
    fn new(target_path: PathBuf) -> Result<Self, String> {
        if !target_path.exists() {
            std::fs::write(&target_path, DEFAULT_DEMO_CONTENT)
                .map_err(|err| format!("create demo file {:?} failed: {err}", target_path))?;
        }

        Ok(Self {
            target_path: target_path.clone(),
            window: None,
            renderer: None,
            input_handler: InputHandler::new(),
            input_map: InputMap::new(target_path.clone()),
            app_state: {
                let mut state = AppState::new(target_path);
                if let Ok(cwd) = std::env::current_dir() {
                    if let Err(err) = state.attach_workspace(cwd.clone()) {
                        eprintln!("workspace attach failed for {}: {err}", cwd.display());
                    }
                }
                state
            },
            view_model: EditorViewModel::new(
                ViewportState::new(VIEWPORT_ORIGIN_X, VIEWPORT_ORIGIN_Y),
                TEXT_COLOR,
                CARET_COLOR,
            ),
            large_file_mode: false,
            large_file_reason: None,
        })
    }

    fn refresh_window_title(&self) {
        let Some(window) = &self.window else {
            return;
        };

        let dirty_mark = if self.app_state.is_dirty() { "*" } else { "" };
        let (line, col) = self.app_state.cursor_line_col();
        let mode = if self.large_file_mode {
            "large-file-preview"
        } else {
            "full-render"
        };
        window.set_title(&format!(
            "{WINDOW_TITLE}{dirty_mark} | Ln {}, Col {} | {} | {}",
            line + 1,
            col + 1,
            mode,
            self.target_path.display()
        ));
    }

    fn sync_and_schedule_redraw(&mut self) {
        let render_text = self.build_render_text_snapshot();

        let Some(renderer) = self.renderer.as_mut() else {
            return;
        };

        match renderer.sync_view_model(&self.app_state, &render_text, &mut self.view_model) {
            Ok(stats) => {
                println!(
                    "Sync: {} layout_rebuilt={} glyphs={} highlights={} syntax_rev={:?} caret_visible={} redraw_required={}",
                    stats.dirty_flags.summary(),
                    stats.layout_rebuilt,
                    stats.glyph_count,
                    stats.highlight_span_count,
                    stats.syntax_revision,
                    stats.caret_visible,
                    stats.redraw_required
                );
            }
            Err(err) => eprintln!("sync failed: {err}"),
        }

        self.refresh_window_title();

        if self.view_model.take_redraw_required() {
            if let Some(window) = &self.window {
                window.request_redraw();
            }
        }
    }

    fn evaluate_large_file_mode(&mut self) {
        let line_count = self.app_state.len_lines();
        let byte_count = self.app_state.len_bytes();
        let should_enable =
            line_count > LARGE_FILE_LINE_THRESHOLD || byte_count > LARGE_FILE_BYTE_THRESHOLD;

        if should_enable {
            let reason = format!(
                "lines={} bytes={} (threshold lines>{}, bytes>{})",
                line_count, byte_count, LARGE_FILE_LINE_THRESHOLD, LARGE_FILE_BYTE_THRESHOLD
            );
            if !self.large_file_mode {
                println!(
                    "[Render] large-file preview mode ON: {}. layout only a prefix snapshot.",
                    reason
                );
                self.view_model.clear_syntax_highlights();
            }
            self.large_file_mode = true;
            self.large_file_reason = Some(reason);
            return;
        }

        if self.large_file_mode {
            println!(
                "[Render] large-file preview mode OFF: lines={} bytes={} -> full render resumed.",
                line_count, byte_count
            );
        }
        self.large_file_mode = false;
        self.large_file_reason = None;
    }

    fn build_render_text_snapshot(&self) -> String {
        if !self.large_file_mode {
            return self.app_state.text_string();
        }

        let reason = self
            .large_file_reason
            .as_deref()
            .unwrap_or("fallback reason unavailable");
        let mut text = String::new();
        text.push_str("[Large File Preview Mode]\n");
        text.push_str("Rendering a prefix snapshot to keep input responsive.\n");
        text.push_str(&format!("Reason: {reason}\n\n"));
        text.push_str("Preview (prefix only):\n");
        text.push_str("----------------------------------------\n");
        text.push_str(&self.app_state.prefix_text(LARGE_FILE_RENDER_PREVIEW_CHARS));
        text.push_str("\n\n----------------------------------------\n");
        text.push_str("Preview is truncated. Full buffer is still editable and savable.\n");
        text
    }
}

impl ApplicationHandler for Phase3SliceApp {
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

        let renderer = match pollster::block_on(SliceRenderer::new(
            window.clone(),
            self.view_model.viewport_origin(),
        )) {
            Ok(renderer) => renderer,
            Err(err) => {
                eprintln!("slice renderer init failed: {err}");
                event_loop.exit();
                return;
            }
        };

        self.window = Some(window);
        self.renderer = Some(renderer);

        // Open file thật từ disk vào active buffer ở startup.
        let open_report = self.view_model.apply_command(
            &mut self.app_state,
            Command::OpenFile(self.target_path.clone()),
        );
        println!("{}", open_report.message);
        if !open_report.success {
            eprintln!("startup open failed, exiting");
            event_loop.exit();
            return;
        }

        self.evaluate_large_file_mode();
        self.sync_and_schedule_redraw();
        println!("Phase3 slice ready.");
        println!("Controls:");
        println!("  - Type text, Enter, Backspace, Arrow keys");
        println!("  - Cmd/Ctrl+S: save file");
        println!("  - Cmd/Ctrl+O: reload target file");
        println!("  - Cmd/Ctrl+P or <Space> f: open file picker overlay");
        println!(
            "  - Large-file preview thresholds: lines>{} OR bytes>{}",
            LARGE_FILE_LINE_THRESHOLD, LARGE_FILE_BYTE_THRESHOLD
        );
        println!("Active file: {}", self.target_path.display());
        println!("State: {}", self.app_state.debug_state_line());
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
            WindowEvent::ModifiersChanged(modifiers) => {
                self.input_handler.update_modifiers(modifiers);
            }
            WindowEvent::Focused(focused) => {
                self.input_handler.on_focus_changed(focused);
            }
            WindowEvent::Resized(new_size) => {
                if let Some(renderer) = self.renderer.as_mut() {
                    renderer.resize(new_size, self.view_model.viewport_origin());
                    self.view_model.mark_layout_dirty();
                    self.sync_and_schedule_redraw();
                }
            }
            WindowEvent::KeyboardInput {
                event: key_event, ..
            } => {
                if let Some(route) = self.input_handler.translate_key_event(
                    &key_event,
                    &self.input_map,
                    KeybindingContext::for_mode_with_picker(
                        self.app_state.current_mode(),
                        self.app_state.is_file_picker_open(),
                    ),
                ) {
                    match route {
                        InputRouteOutcome::Dispatch(translated) => {
                            println!("Input: {}", translated.input_debug);
                            println!("Route: {}", translated.route_debug);
                            println!("Command: {:?}", translated.command);

                            let report = self
                                .view_model
                                .apply_command(&mut self.app_state, translated.command);
                            println!("{}", report.message);
                            println!("State: {}", self.app_state.debug_state_line());
                            println!("Dirty: {}", self.view_model.dirty_flags().summary());

                            if !report.success {
                                eprintln!("command failed");
                            }
                            if report.success
                                && report.state_changed
                                && self.view_model.dirty_flags().text_dirty
                            {
                                self.evaluate_large_file_mode();
                            }

                            self.sync_and_schedule_redraw();
                        }
                        InputRouteOutcome::NoDispatch {
                            input_debug,
                            route_debug,
                        } => {
                            println!("Input: {}", input_debug);
                            println!("Route: {}", route_debug);
                        }
                    }
                }
            }
            WindowEvent::Ime(Ime::Commit(text)) => {
                if let Some(translated) = self.input_handler.translate_ime_commit(
                    &text,
                    KeybindingContext::for_mode_with_picker(
                        self.app_state.current_mode(),
                        self.app_state.is_file_picker_open(),
                    ),
                ) {
                    println!("Input: {}", translated.input_debug);
                    println!("Route: {}", translated.route_debug);
                    println!("Command: {:?}", translated.command);

                    let report = self
                        .view_model
                        .apply_command(&mut self.app_state, translated.command);
                    println!("{}", report.message);
                    println!("State: {}", self.app_state.debug_state_line());
                    println!("Dirty: {}", self.view_model.dirty_flags().summary());

                    if !report.success {
                        eprintln!("command failed");
                    }
                    if report.success
                        && report.state_changed
                        && self.view_model.dirty_flags().text_dirty
                    {
                        self.evaluate_large_file_mode();
                    }

                    self.sync_and_schedule_redraw();
                }
            }
            WindowEvent::RedrawRequested => {
                if let Some(renderer) = self.renderer.as_mut() {
                    if let Err(err) = renderer.render() {
                        eprintln!("render failed: {err}");
                    }
                }
            }
            _ => {}
        }
    }
}

fn parse_target_path() -> PathBuf {
    std::env::args()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(DEFAULT_DEMO_PATH))
}

fn main() {
    let target_path = parse_target_path();
    println!("phase3_slice target file: {}", target_path.display());

    let event_loop = EventLoop::new().expect("failed to create event loop");
    event_loop.set_control_flow(ControlFlow::Wait);

    let mut app = match Phase3SliceApp::new(target_path) {
        Ok(app) => app,
        Err(err) => {
            eprintln!("phase3_slice setup failed: {err}");
            return;
        }
    };

    if let Err(err) = event_loop.run_app(&mut app) {
        eprintln!("phase3_slice exited with error: {err}");
    }
}
