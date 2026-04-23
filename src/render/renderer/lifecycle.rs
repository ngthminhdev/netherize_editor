//! Renderer lifecycle: GPU bootstrap, theme/config application, resize handling,
//! and the top-level render pass ordering.

use std::sync::Arc;

use cosmic_text::Metrics;
use winit::{dpi::PhysicalSize, window::Window};

use crate::{
    config::{
        theme_config::ThemeConfig,
        ui_config::{CursorShape, UiConfig},
    },
    render::{
        caret::CaretPipeline,
        region_pipeline::{RegionDrawInstance, RegionPipeline},
        surface::SurfaceState,
        text_pipeline::TextPipeline,
    },
    terminal::terminal_renderer::TerminalViewRenderer,
    text::{atlas::GlyphAtlas, text_system::TextSystem},
};

use super::{
    ATLAS_SIZE, RenderError, Renderer,
    helpers::{draw_text_region, theme_color_to_wgpu},
};

fn make_text_system(metrics: Metrics, family: Option<&str>) -> TextSystem {
    let mut text_system = TextSystem::new(metrics, None, None);
    text_system.set_font_family(family);
    text_system
}

fn make_text_pipeline(
    device: &wgpu::Device,
    atlas: &GlyphAtlas,
    format: wgpu::TextureFormat,
    width: u32,
    height: u32,
) -> TextPipeline {
    TextPipeline::new(device, format, atlas, width, height)
}

impl Renderer {
    pub async fn new(window: Arc<Window>) -> Result<Self, String> {
        let window_size = window.inner_size();
        let width = window_size.width.max(1);
        let height = window_size.height.max(1);

        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: wgpu::Backends::all(),
            ..wgpu::InstanceDescriptor::new_without_display_handle()
        });
        let surface = instance
            .create_surface(window)
            .map_err(|error| format!("create_surface: {error}"))?;
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
            })
            .await
            .map_err(|error| format!("request_adapter: {error}"))?;

        let info = adapter.get_info();
        println!(
            "wgpu: name='{}' type={:?} backend={:?}",
            info.name, info.device_type, info.backend
        );

        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: Some("Netherize Device"),
                required_features: wgpu::Features::empty(),
                required_limits: wgpu::Limits::default(),
                experimental_features: Default::default(),
                memory_hints: Default::default(),
                trace: Default::default(),
            })
            .await
            .map_err(|error| format!("request_device: {error}"))?;

        let surface_state = SurfaceState::new(surface, window_size, &adapter, &device)?;
        let surface_format = surface_state.config.format;
        let region_pipeline = RegionPipeline::new(&device, surface_format, width, height);
        let caret_pipeline = CaretPipeline::new(&device, surface_format, width, height);

        let theme = ThemeConfig::builtin_dark();
        let clear_color = theme_color_to_wgpu(theme.ui.bg);
        let statusbar_font_size = theme.ui.sidebar_font_size;
        let statusbar_line_height = theme.ui.sidebar_line_height;

        let font_family = theme.editor.font_family.clone();
        let nerd_family = theme
            .editor
            .nerd_font_family
            .clone()
            .filter(|family| !family.is_empty())
            .or_else(|| font_family.clone());

        let editor_metrics = Metrics::new(theme.editor.font_size, theme.editor.line_height);
        let ui_metrics = Metrics::new(theme.ui.sidebar_font_size, theme.ui.sidebar_line_height);
        let panel_metrics = Metrics::new(theme.ui.panel_font_size, theme.ui.panel_line_height);
        let leap_metrics =
            Metrics::new(theme.editor.font_size * 2.0, theme.editor.line_height * 2.0);

        let text_system = make_text_system(editor_metrics, font_family.as_deref());
        let gutter_text_system = make_text_system(editor_metrics, font_family.as_deref());
        let sidebar_text_system = make_text_system(ui_metrics, nerd_family.as_deref());
        let terminal_text_system = make_text_system(panel_metrics, font_family.as_deref());
        let welcome_logo_text_system = make_text_system(panel_metrics, font_family.as_deref());
        let topbar_text_system = make_text_system(ui_metrics, font_family.as_deref());
        let statusbar_text_system = make_text_system(ui_metrics, font_family.as_deref());
        let palette_text_system = make_text_system(ui_metrics, font_family.as_deref());
        let lsp_guide_text_system = make_text_system(panel_metrics, font_family.as_deref());
        let toast_text_system = make_text_system(ui_metrics, font_family.as_deref());
        let leap_label_text_system = make_text_system(leap_metrics, font_family.as_deref());

        let atlas = GlyphAtlas::new(&device, ATLAS_SIZE, ATLAS_SIZE);
        let text_pipeline = make_text_pipeline(&device, &atlas, surface_format, width, height);
        let editor_cursor_overlay_pipeline =
            make_text_pipeline(&device, &atlas, surface_format, width, height);
        let editor_overlay_text_pipeline =
            make_text_pipeline(&device, &atlas, surface_format, width, height);
        let gutter_text_pipeline =
            make_text_pipeline(&device, &atlas, surface_format, width, height);
        let sidebar_text_pipeline =
            make_text_pipeline(&device, &atlas, surface_format, width, height);
        let terminal_text_pipeline =
            make_text_pipeline(&device, &atlas, surface_format, width, height);
        let buffer_terminal_text_pipeline =
            make_text_pipeline(&device, &atlas, surface_format, width, height);
        let welcome_logo_text_pipeline =
            make_text_pipeline(&device, &atlas, surface_format, width, height);
        let topbar_text_pipeline =
            make_text_pipeline(&device, &atlas, surface_format, width, height);
        let statusbar_text_pipeline =
            make_text_pipeline(&device, &atlas, surface_format, width, height);
        let palette_text_pipeline =
            make_text_pipeline(&device, &atlas, surface_format, width, height);
        let lsp_guide_text_pipeline =
            make_text_pipeline(&device, &atlas, surface_format, width, height);
        let toast_text_pipeline =
            make_text_pipeline(&device, &atlas, surface_format, width, height);
        let leap_label_text_pipeline =
            make_text_pipeline(&device, &atlas, surface_format, width, height);

        Ok(Self {
            device,
            queue,
            surface_state,
            region_pipeline,
            theme,
            clear_color,
            atlas,
            text_system,
            text_pipeline,
            glyph_instances: Vec::new(),
            caret_pipeline,
            editor_cursor_overlay_pipeline,
            editor_scissor: None,
            editor_overlay_text_system: make_text_system(editor_metrics, font_family.as_deref()),
            editor_overlay_text_pipeline,
            editor_overlay_glyph_instances: Vec::new(),
            editor_overlay_chrome_instances: Vec::new(),
            editor_overlay_scissor: None,
            gutter_text_system,
            gutter_text_pipeline,
            gutter_glyph_instances: Vec::new(),
            relative_numbers: false,
            sidebar_text_system,
            sidebar_text_pipeline,
            sidebar_glyph_instances: Vec::new(),
            sidebar_scissor: None,
            terminal_text_system,
            terminal_text_pipeline,
            terminal_view_renderer: TerminalViewRenderer::default_monospace(),
            terminal_glyph_instances: Vec::new(),
            terminal_cursor_instances: Vec::new(),
            terminal_scissor: None,
            buffer_terminal_text_system: make_text_system(panel_metrics, font_family.as_deref()),
            buffer_terminal_text_pipeline,
            buffer_terminal_view_renderer: TerminalViewRenderer::default_monospace(),
            buffer_terminal_glyph_instances: Vec::new(),
            buffer_terminal_cursor_instances: Vec::new(),
            buffer_terminal_scissor: None,
            welcome_logo_text_system,
            welcome_logo_text_pipeline,
            welcome_logo_view_renderer: TerminalViewRenderer::default_monospace(),
            welcome_logo_glyph_instances: Vec::new(),
            welcome_logo_scissor: None,
            topbar_text_system,
            topbar_text_pipeline,
            topbar_glyph_instances: Vec::new(),
            topbar_chrome_instances: Vec::new(),
            topbar_scissor: None,
            last_topbar_layout_key: None,
            statusbar_text_system,
            statusbar_text_pipeline,
            statusbar_glyph_instances: Vec::new(),
            statusbar_chrome_instances: Vec::new(),
            statusbar_scissor: None,
            last_statusbar_layout_key: None,
            palette_text_system,
            palette_text_pipeline,
            palette_glyph_instances: Vec::new(),
            palette_chrome_instances: Vec::new(),
            palette_scissor: None,
            last_palette_model: None,
            lsp_guide_text_system,
            lsp_guide_text_pipeline,
            lsp_guide_scissor: None,
            editor_padding_x: 14.0,
            editor_padding_y: 14.0,
            panel_padding: 10.0,
            sidebar_base_padding: 10.0,
            sidebar_indent_per_depth: 15.0,
            topbar_padding_x: 14.0,
            statusbar_padding_x: 14.0,
            statusbar_font_size,
            statusbar_line_height,
            cursor_shape: CursorShape::Block,
            cursor_beam_width: 1.8,
            cursor_block_width: 10.0,
            cursor_underline_height: 2.0,
            leap_label_text_system,
            leap_label_text_pipeline,
            leap_label_glyph_instances: Vec::new(),
            leap_label_bg_instances: Vec::new(),
            leap_label_scissor: None,
            lsp_guide_chrome_instances: Vec::new(),
            lsp_guide_glyph_instances: Vec::new(),
            toast_text_system,
            toast_text_pipeline,
            toast_glyph_instances: Vec::new(),
            toast_chrome_instances: Vec::new(),
            toast_scissor: None,
        })
    }

    pub fn apply_theme(&mut self, theme: ThemeConfig) {
        self.text_system.set_metrics(Metrics::new(
            theme.editor.font_size,
            theme.editor.line_height,
        ));
        self.editor_overlay_text_system.set_metrics(Metrics::new(
            theme.editor.font_size,
            theme.editor.line_height,
        ));
        let ui_metrics = Metrics::new(theme.ui.sidebar_font_size, theme.ui.sidebar_line_height);
        self.sidebar_text_system.set_metrics(ui_metrics);
        self.terminal_text_system.set_metrics(Metrics::new(
            theme.ui.panel_font_size,
            theme.ui.panel_line_height,
        ));
        self.buffer_terminal_text_system.set_metrics(Metrics::new(
            theme.ui.panel_font_size,
            theme.ui.panel_line_height,
        ));
        self.topbar_text_system.set_metrics(ui_metrics);
        self.statusbar_text_system.set_metrics(ui_metrics);
        self.palette_text_system.set_metrics(ui_metrics);
        self.lsp_guide_text_system.set_metrics(Metrics::new(
            theme.ui.panel_font_size,
            theme.ui.panel_line_height,
        ));
        self.toast_text_system.set_metrics(ui_metrics);

        let family = theme.editor.font_family.as_deref();
        let nerd_family = theme
            .editor
            .nerd_font_family
            .as_deref()
            .filter(|family| !family.is_empty())
            .or(family);

        // Sidebar text must use NerdFont to render PUA glyphs without mojibake.
        self.text_system.set_font_family(family);
        self.editor_overlay_text_system.set_font_family(family);
        self.sidebar_text_system.set_font_family(nerd_family);
        self.terminal_text_system.set_font_family(nerd_family);
        self.buffer_terminal_text_system
            .set_font_family(nerd_family);
        self.welcome_logo_text_system.set_font_family(family);
        self.topbar_text_system.set_font_family(family);
        self.statusbar_text_system.set_font_family(family);
        self.palette_text_system.set_font_family(family);
        self.lsp_guide_text_system.set_font_family(family);
        self.toast_text_system.set_font_family(family);
        self.leap_label_text_system.set_font_family(family);

        self.clear_color = theme_color_to_wgpu(theme.ui.bg);
        self.theme = theme;
        self.last_topbar_layout_key = None;
        self.last_statusbar_layout_key = None;
        self.last_palette_model = None;
    }

    pub fn apply_ui_config(&mut self, ui: &UiConfig) {
        self.editor_padding_x = ui.spacing.editor_padding;
        self.editor_padding_y = ui.spacing.editor_padding;
        self.panel_padding = ui.spacing.panel_padding;
        self.sidebar_base_padding = ui.spacing.explorer_padding;
        self.sidebar_indent_per_depth = (ui.spacing.explorer_padding * 1.5).max(10.0);
        self.topbar_padding_x = ui.status_bar.padding_x;
        self.statusbar_padding_x = ui.status_bar.padding_x;
        self.statusbar_font_size = ui.status_bar.font_size;
        self.statusbar_line_height = ui.status_bar.line_height;
        self.cursor_shape = ui.cursor.shape;
        self.cursor_beam_width = ui.cursor.beam_width;
        self.cursor_block_width = ui.cursor.block_width;
        self.cursor_underline_height = ui.cursor.underline_height;
        self.relative_numbers = ui.editor.relative_numbers;

        let status_metrics = Metrics::new(ui.status_bar.font_size, ui.status_bar.line_height);
        self.topbar_text_system.set_metrics(status_metrics);
        self.statusbar_text_system.set_metrics(status_metrics);
        self.toast_text_system.set_metrics(status_metrics);
        self.last_topbar_layout_key = None;
        self.last_statusbar_layout_key = None;
        self.last_palette_model = None;
    }

    pub fn resize(&mut self, new_size: PhysicalSize<u32>) {
        self.surface_state.resize(&self.device, new_size);
        let (width, height) = (new_size.width.max(1), new_size.height.max(1));
        for pipeline in [
            &mut self.text_pipeline,
            &mut self.editor_cursor_overlay_pipeline,
            &mut self.editor_overlay_text_pipeline,
            &mut self.gutter_text_pipeline,
            &mut self.sidebar_text_pipeline,
            &mut self.terminal_text_pipeline,
            &mut self.buffer_terminal_text_pipeline,
            &mut self.welcome_logo_text_pipeline,
            &mut self.topbar_text_pipeline,
            &mut self.statusbar_text_pipeline,
            &mut self.palette_text_pipeline,
            &mut self.lsp_guide_text_pipeline,
            &mut self.toast_text_pipeline,
            &mut self.leap_label_text_pipeline,
        ] {
            pipeline.update_screen_size(&self.queue, width, height);
        }
        self.region_pipeline
            .update_screen_size(&self.queue, width, height);
        self.caret_pipeline
            .update_screen_size(&self.queue, width, height);
    }

    pub fn reconfigure_surface(&self) {
        self.surface_state.reconfigure(&self.device);
    }

    pub fn render(&mut self, region_instances: &[RegionDrawInstance]) -> Result<(), RenderError> {
        let frame = match self.surface_state.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(texture) => texture,
            wgpu::CurrentSurfaceTexture::Suboptimal(texture) => texture,
            wgpu::CurrentSurfaceTexture::Timeout => return Err(RenderError::Timeout),
            wgpu::CurrentSurfaceTexture::Occluded => return Err(RenderError::Occluded),
            wgpu::CurrentSurfaceTexture::Outdated => return Err(RenderError::Outdated),
            wgpu::CurrentSurfaceTexture::Lost => return Err(RenderError::Lost),
            wgpu::CurrentSurfaceTexture::Validation => return Err(RenderError::Validation),
        };

        self.region_pipeline
            .upload_instances(&self.device, &self.queue, region_instances);

        let view = frame
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Netherize Encoder"),
            });

        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Netherize RenderPass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(self.clear_color),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });

            let viewport_width = self.surface_state.config.width;
            let viewport_height = self.surface_state.config.height;

            // 1. Panel backgrounds (no scissor).
            self.region_pipeline.draw(&mut pass);

            // 2. Editor text + caret + cursor overlay + gutter.
            draw_text_region(
                &mut pass,
                self.editor_scissor,
                viewport_width,
                viewport_height,
                |render_pass| {
                    self.text_pipeline.draw(render_pass);
                    self.caret_pipeline.draw(render_pass);
                    self.editor_cursor_overlay_pipeline.draw(render_pass);
                    self.gutter_text_pipeline.draw(render_pass);
                },
            );

            if !self.editor_overlay_chrome_instances.is_empty() {
                self.region_pipeline.upload_instances(
                    &self.device,
                    &self.queue,
                    &self.editor_overlay_chrome_instances,
                );
            let topbar_inner_scissor = self.topbar_scissor.and_then(|[sx, sy, sw, sh]| {
                let inset_x = 4_u32;
                let inset_y = 2_u32;
                let clipped_w = sw.saturating_sub(inset_x.saturating_mul(2));
                let clipped_h = sh.saturating_sub(inset_y.saturating_mul(2));
                (clipped_w > 0 && clipped_h > 0).then_some([
                    sx.saturating_add(inset_x),
                    sy.saturating_add(inset_y),
                    clipped_w,
                    clipped_h,
                ])
            });
            draw_text_region(
                &mut pass,
                topbar_inner_scissor,
                viewport_width,
                viewport_height,
                |render_pass| {
                    self.topbar_text_pipeline.draw(render_pass);
                },
            );
            }

            draw_text_region(
                &mut pass,
                self.editor_overlay_scissor,
                viewport_width,
                viewport_height,
                |render_pass| {
                    self.editor_overlay_text_pipeline.draw(render_pass);
                },
            );

            // 3. Welcome ANSI logo.
            draw_text_region(
                &mut pass,
                self.welcome_logo_scissor,
                viewport_width,
                viewport_height,
                |render_pass| {
                    self.welcome_logo_text_pipeline.draw(render_pass);
                },
            );

            // 4. Explorer sidebar.
            draw_text_region(
                &mut pass,
                self.sidebar_scissor,
                viewport_width,
                viewport_height,
                |render_pass| {
                    self.sidebar_text_pipeline.draw(render_pass);
                },
            );

            // 5. Leap label overlay: dim + per-char bg + label chars.
            if !self.leap_label_glyph_instances.is_empty() {
                if !self.leap_label_bg_instances.is_empty() {
                    self.region_pipeline.upload_instances(
                        &self.device,
                        &self.queue,
                        &self.leap_label_bg_instances,
                    );
                    draw_text_region(
                        &mut pass,
                        self.leap_label_scissor,
                        viewport_width,
                        viewport_height,
                        |render_pass| {
                            self.region_pipeline.draw(render_pass);
                        },
                    );
                }
                draw_text_region(
                    &mut pass,
                    self.leap_label_scissor,
                    viewport_width,
                    viewport_height,
                    |render_pass| {
                        self.leap_label_text_pipeline.draw(render_pass);
                    },
                );
            }

            // 6. Terminal panel.
            draw_text_region(
                &mut pass,
                self.terminal_scissor,
                viewport_width,
                viewport_height,
                |render_pass| {
                    self.terminal_text_pipeline.draw(render_pass);
                },
            );
            if !self.terminal_cursor_instances.is_empty() {
                self.region_pipeline.upload_instances(
                    &self.device,
                    &self.queue,
                    &self.terminal_cursor_instances,
                );
                draw_text_region(
                    &mut pass,
                    self.terminal_scissor,
                    viewport_width,
                    viewport_height,
                    |render_pass| {
                        self.region_pipeline.draw(render_pass);
                    },
                );
            }

            draw_text_region(
                &mut pass,
                self.buffer_terminal_scissor,
                viewport_width,
                viewport_height,
                |render_pass| {
                    self.buffer_terminal_text_pipeline.draw(render_pass);
                },
            );
            if !self.buffer_terminal_cursor_instances.is_empty() {
                self.region_pipeline.upload_instances(
                    &self.device,
                    &self.queue,
                    &self.buffer_terminal_cursor_instances,
                );
                draw_text_region(
                    &mut pass,
                    self.buffer_terminal_scissor,
                    viewport_width,
                    viewport_height,
                    |render_pass| {
                        self.region_pipeline.draw(render_pass);
                    },
                );
            }

            // 7. TopBar.
            let topbar_inner_scissor = self.topbar_scissor.and_then(|[sx, sy, sw, sh]| {
                let inset_x = 4_u32;
                let inset_y = 2_u32;
                let clipped_w = sw.saturating_sub(inset_x.saturating_mul(2));
                let clipped_h = sh.saturating_sub(inset_y.saturating_mul(2));
                (clipped_w > 0 && clipped_h > 0).then_some([
                    sx.saturating_add(inset_x),
                    sy.saturating_add(inset_y),
                    clipped_w,
                    clipped_h,
                ])
            });
            draw_text_region(
                &mut pass,
                topbar_inner_scissor,
                viewport_width,
                viewport_height,
                |render_pass| {
                    self.topbar_text_pipeline.draw(render_pass);
                },
            );

            // 8. StatusBar.
            draw_text_region(
                &mut pass,
                self.statusbar_scissor,
                viewport_width,
                viewport_height,
                |render_pass| {
                    self.statusbar_text_pipeline.draw(render_pass);
                },
            );

            // 9. Command palette chrome (scrim + box) above editor text.
            if !self.palette_chrome_instances.is_empty() {
                self.region_pipeline.upload_instances(
                    &self.device,
                    &self.queue,
                    &self.palette_chrome_instances,
                );
                self.region_pipeline.draw(&mut pass);
            }

            // 10. Command palette / file picker text (topmost layer).
            draw_text_region(
                &mut pass,
                self.palette_scissor,
                viewport_width,
                viewport_height,
                |render_pass| {
                    self.palette_text_pipeline.draw(render_pass);
                },
            );

            if !self.lsp_guide_chrome_instances.is_empty() {
                self.region_pipeline.upload_instances(
                    &self.device,
                    &self.queue,
                    &self.lsp_guide_chrome_instances,
                );
                draw_text_region(
                    &mut pass,
                    self.lsp_guide_scissor,
                    viewport_width,
                    viewport_height,
                    |render_pass| {
                        self.region_pipeline.draw(render_pass);
                    },
                );
            }
            draw_text_region(
                &mut pass,
                self.lsp_guide_scissor,
                viewport_width,
                viewport_height,
                |render_pass| {
                    self.lsp_guide_text_pipeline.draw(render_pass);
                },
            );

            if !self.toast_chrome_instances.is_empty() {
                self.region_pipeline.upload_instances(
                    &self.device,
                    &self.queue,
                    &self.toast_chrome_instances,
                );
                draw_text_region(
                    &mut pass,
                    self.toast_scissor,
                    viewport_width,
                    viewport_height,
                    |render_pass| {
                        self.region_pipeline.draw(render_pass);
                    },
                );
            }
            draw_text_region(
                &mut pass,
                self.toast_scissor,
                viewport_width,
                viewport_height,
                |render_pass| {
                    self.toast_text_pipeline.draw(render_pass);
                },
            );
        }

        self.queue.submit(std::iter::once(encoder.finish()));
        frame.present();
        Ok(())
    }
}
