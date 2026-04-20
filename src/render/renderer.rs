use std::sync::Arc;

use cosmic_text::Metrics;
use winit::{dpi::PhysicalSize, window::Window};

use crate::{
    app::app_state::AppState,
    config::theme_config::{ThemeColor, ThemeConfig},
    render::{
        caret::{CaretPipeline, CaretScreenRect},
        glyph_instance::GlyphInstance,
        region_pipeline::{RegionDrawInstance, RegionPipeline},
        surface::SurfaceState,
        text_pipeline::TextPipeline,
    },
    terminal::{grid::TerminalGrid, terminal_renderer::TerminalViewRenderer},
    text::{
        atlas::GlyphAtlas,
        layout_sync::{compute_caret_layout, rebuild_layout_projection},
        raster::rasterize_glyph_alpha,
        text_system::{StyledTextSpan, TextSystem},
    },
};

const CARET_WIDTH: f32 = 2.0;
const ATLAS_SIZE: u32 = 2048;
const EDITOR_PADDING: f32 = 10.0;
const PANEL_PADDING: f32 = 8.0;
const EMPTY_TERMINAL_HINT: &str = "(terminal ready — press Ctrl+` to toggle)";

#[derive(Debug)]
pub enum RenderError {
    Timeout,
    Occluded,
    Outdated,
    Lost,
    Validation,
}

pub struct Renderer {
    device: wgpu::Device,
    queue: wgpu::Queue,
    pub surface_state: SurfaceState,
    region_pipeline: RegionPipeline,
    theme: ThemeConfig,
    clear_color: wgpu::Color,
    atlas: GlyphAtlas,
    // ── Editor ────────────────────────────────────────────────────────────────
    text_system: TextSystem,
    text_pipeline: TextPipeline,
    glyph_instances: Vec<GlyphInstance>,
    caret_pipeline: CaretPipeline,
    editor_scissor: Option<[u32; 4]>,
    // ── Explorer sidebar ──────────────────────────────────────────────────────
    sidebar_text_system: TextSystem,
    sidebar_text_pipeline: TextPipeline,
    sidebar_glyph_instances: Vec<GlyphInstance>,
    sidebar_scissor: Option<[u32; 4]>,
    // ── Terminal panel ────────────────────────────────────────────────────────
    terminal_text_system: TextSystem,
    terminal_text_pipeline: TextPipeline,
    terminal_view_renderer: TerminalViewRenderer,
    terminal_glyph_instances: Vec<GlyphInstance>,
    terminal_scissor: Option<[u32; 4]>,
}

impl Renderer {
    pub async fn new(window: Arc<Window>) -> Result<Self, String> {
        let window_size = window.inner_size();
        let w = window_size.width.max(1);
        let h = window_size.height.max(1);

        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: wgpu::Backends::all(),
            ..wgpu::InstanceDescriptor::new_without_display_handle()
        });

        let surface = instance
            .create_surface(window)
            .map_err(|e| format!("create_surface: {e}"))?;

        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
            })
            .await
            .map_err(|e| format!("request_adapter: {e}"))?;

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
            .map_err(|e| format!("request_device: {e}"))?;

        let surface_state = SurfaceState::new(surface, window_size, &adapter, &device)?;
        let fmt = surface_state.config.format;

        let region_pipeline = RegionPipeline::new(&device, fmt, w, h);

        let theme = ThemeConfig::builtin_dark();
        let clear_color = theme_color_to_wgpu(theme.editor.bg);
        let atlas = GlyphAtlas::new(&device, ATLAS_SIZE, ATLAS_SIZE);
        let text_system = TextSystem::new(
            Metrics::new(theme.editor.font_size, theme.editor.line_height),
            None,
            None,
        );
        let text_pipeline = TextPipeline::new(&device, fmt, &atlas, w, h);
        let caret_pipeline = CaretPipeline::new(&device, fmt, w, h);

        let sidebar_text_system = TextSystem::new(
            Metrics::new(theme.ui.sidebar_font_size, theme.ui.sidebar_line_height),
            None,
            None,
        );
        let sidebar_text_pipeline = TextPipeline::new(&device, fmt, &atlas, w, h);

        let terminal_text_system = TextSystem::new(
            Metrics::new(theme.ui.panel_font_size, theme.ui.panel_line_height),
            None,
            None,
        );
        let terminal_text_pipeline = TextPipeline::new(&device, fmt, &atlas, w, h);

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
            editor_scissor: None,
            sidebar_text_system,
            sidebar_text_pipeline,
            sidebar_glyph_instances: Vec::new(),
            sidebar_scissor: None,
            terminal_text_system,
            terminal_text_pipeline,
            terminal_view_renderer: TerminalViewRenderer::default_monospace(),
            terminal_glyph_instances: Vec::new(),
            terminal_scissor: None,
        })
    }

    pub fn apply_theme(&mut self, theme: ThemeConfig) {
        self.text_system.set_metrics(Metrics::new(
            theme.editor.font_size,
            theme.editor.line_height,
        ));
        self.sidebar_text_system.set_metrics(Metrics::new(
            theme.ui.sidebar_font_size,
            theme.ui.sidebar_line_height,
        ));
        self.terminal_text_system.set_metrics(Metrics::new(
            theme.ui.panel_font_size,
            theme.ui.panel_line_height,
        ));
        self.clear_color = theme_color_to_wgpu(theme.editor.bg);
        self.theme = theme;
    }

    /// Rebuild glyph instances and caret for the center editor region.
    pub fn update_editor_content(
        &mut self,
        text: &str,
        app_state: &AppState,
        center_bounds: [f32; 4],
        spans: &[StyledTextSpan],
    ) {
        let origin_x = center_bounds[0] + EDITOR_PADDING;
        let origin_y = center_bounds[1] + EDITOR_PADDING + self.theme.editor.line_height;
        let width = (center_bounds[2] - EDITOR_PADDING * 2.0).max(1.0);
        let height =
            (center_bounds[3] - EDITOR_PADDING * 2.0 - self.theme.editor.line_height).max(1.0);

        let sx = center_bounds[0].round() as u32;
        let sy = center_bounds[1].round() as u32;
        let sw = center_bounds[2].round() as u32;
        let sh = center_bounds[3].round() as u32;
        self.editor_scissor = (sw > 0 && sh > 0).then_some([sx, sy, sw, sh]);

        self.text_system.set_size(Some(width), Some(height));

        let result = rebuild_layout_projection(
            text,
            app_state,
            &mut self.text_system,
            &mut self.atlas,
            &self.queue,
            [origin_x, origin_y],
            self.theme.editor.fg.as_f32(),
            spans,
        );

        match result {
            Ok(projection) => {
                self.glyph_instances = projection.glyph_instances;
                let caret = CaretScreenRect::from_layout(
                    projection.caret_layout.x,
                    projection.caret_layout.top,
                    projection.caret_layout.height,
                    CARET_WIDTH,
                    self.theme.editor.cursor.as_f32(),
                );
                self.caret_pipeline.upload_caret(&self.queue, Some(caret));
            }
            Err(e) => {
                eprintln!("[Renderer] text layout: {e}");
                self.glyph_instances.clear();
                self.caret_pipeline.upload_caret(&self.queue, None);
            }
        }

        self.text_pipeline
            .upload_instances(&self.device, &self.queue, &self.glyph_instances);
    }

    /// Fast path for cursor movement: reuse existing layout and update caret only.
    pub fn update_editor_caret(&mut self, app_state: &AppState, center_bounds: [f32; 4]) {
        let origin_x = center_bounds[0] + EDITOR_PADDING;
        let origin_y = center_bounds[1] + EDITOR_PADDING + self.theme.editor.line_height;
        let caret_layout = compute_caret_layout(&self.text_system, app_state, [origin_x, origin_y]);
        let caret = CaretScreenRect::from_layout(
            caret_layout.x,
            caret_layout.top,
            caret_layout.height,
            CARET_WIDTH,
            self.theme.editor.cursor.as_f32(),
        );
        self.caret_pipeline.upload_caret(&self.queue, Some(caret));
    }

    /// Render the explorer file tree into the left sidebar region.
    pub fn update_sidebar_content(&mut self, text: &str, bounds: [f32; 4]) {
        if bounds[2] < 1.0 || bounds[3] < 1.0 {
            self.sidebar_scissor = None;
            return;
        }
        let origin_x = bounds[0] + PANEL_PADDING;
        let origin_y = bounds[1] + PANEL_PADDING + self.theme.ui.sidebar_line_height;
        let width = (bounds[2] - PANEL_PADDING * 2.0).max(1.0);
        let height = (bounds[3] - PANEL_PADDING * 2.0 - self.theme.ui.sidebar_line_height).max(1.0);

        self.sidebar_scissor = Some([
            bounds[0].round() as u32,
            bounds[1].round() as u32,
            bounds[2].round() as u32,
            bounds[3].round() as u32,
        ]);

        self.sidebar_text_system.set_size(Some(width), Some(height));
        self.sidebar_glyph_instances = layout_panel_text(
            text,
            &mut self.sidebar_text_system,
            &mut self.atlas,
            &self.queue,
            origin_x,
            origin_y,
            self.theme.editor.fg.as_f32(),
        );
        self.sidebar_text_pipeline.upload_instances(
            &self.device,
            &self.queue,
            &self.sidebar_glyph_instances,
        );
    }

    /// Clear sidebar — called when the panel is hidden.
    pub fn clear_sidebar(&mut self) {
        self.sidebar_scissor = None;
        self.sidebar_glyph_instances.clear();
        self.sidebar_text_pipeline
            .upload_instances(&self.device, &self.queue, &[]);
    }

    /// Render PTY grid output into the bottom terminal panel.
    pub fn update_terminal_content(&mut self, grid: &TerminalGrid, bounds: [f32; 4]) {
        if bounds[2] < 1.0 || bounds[3] < 1.0 {
            self.terminal_scissor = None;
            return;
        }
        let origin_x = bounds[0] + PANEL_PADDING;
        let origin_y = bounds[1] + PANEL_PADDING + self.theme.ui.panel_line_height;
        let width = (bounds[2] - PANEL_PADDING * 2.0).max(1.0);
        let height = (bounds[3] - PANEL_PADDING * 2.0 - self.theme.ui.panel_line_height).max(1.0);

        self.terminal_scissor = Some([
            bounds[0].round() as u32,
            bounds[1].round() as u32,
            bounds[2].round() as u32,
            bounds[3].round() as u32,
        ]);

        let default_fg = self.theme.editor.fg.as_f32();
        let default_bg = self.theme.ui.panel_bg.as_f32();

        if grid.used_rows() == 0 {
            self.terminal_text_system
                .set_size(Some(width), Some(height));
            self.terminal_glyph_instances = layout_panel_text(
                EMPTY_TERMINAL_HINT,
                &mut self.terminal_text_system,
                &mut self.atlas,
                &self.queue,
                origin_x,
                origin_y,
                default_fg,
            );
        } else {
            self.terminal_view_renderer.origin_x = origin_x;
            self.terminal_view_renderer.origin_y = origin_y;
            self.terminal_view_renderer.cell_width = (width / grid.cols.max(1) as f32).max(1.0);
            self.terminal_view_renderer.cell_height = (height / grid.rows.max(1) as f32).max(1.0);

            self.terminal_glyph_instances = self.terminal_view_renderer.build_instances(
                grid,
                &mut self.atlas,
                &self.queue,
                &mut self.terminal_text_system,
                default_fg,
                default_bg,
            );
        }

        self.terminal_text_pipeline.upload_instances(
            &self.device,
            &self.queue,
            &self.terminal_glyph_instances,
        );
    }

    /// Clear terminal — called when the panel is hidden.
    pub fn clear_terminal(&mut self) {
        self.terminal_scissor = None;
        self.terminal_glyph_instances.clear();
        self.terminal_text_pipeline
            .upload_instances(&self.device, &self.queue, &[]);
    }

    pub fn resize(&mut self, new_size: PhysicalSize<u32>) {
        self.surface_state.resize(&self.device, new_size);
        let (w, h) = (new_size.width.max(1), new_size.height.max(1));
        self.region_pipeline.update_screen_size(&self.queue, w, h);
        self.text_pipeline.update_screen_size(&self.queue, w, h);
        self.caret_pipeline.update_screen_size(&self.queue, w, h);
        self.sidebar_text_pipeline
            .update_screen_size(&self.queue, w, h);
        self.terminal_text_pipeline
            .update_screen_size(&self.queue, w, h);
    }

    pub fn reconfigure_surface(&self) {
        self.surface_state.reconfigure(&self.device);
    }

    pub fn render(&mut self, region_instances: &[RegionDrawInstance]) -> Result<(), RenderError> {
        let frame = match self.surface_state.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(t) => t,
            wgpu::CurrentSurfaceTexture::Suboptimal(t) => t,
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

            let vp_w = self.surface_state.config.width;
            let vp_h = self.surface_state.config.height;

            // 1. Panel backgrounds (no scissor)
            self.region_pipeline.draw(&mut pass);

            // 2. Editor text + caret: clipped to center region
            draw_text_region(&mut pass, self.editor_scissor, vp_w, vp_h, |pass| {
                self.text_pipeline.draw(pass);
                self.caret_pipeline.draw(pass);
            });

            // 3. Explorer sidebar text
            draw_text_region(&mut pass, self.sidebar_scissor, vp_w, vp_h, |pass| {
                self.sidebar_text_pipeline.draw(pass)
            });

            // 4. Terminal panel text
            draw_text_region(&mut pass, self.terminal_scissor, vp_w, vp_h, |pass| {
                self.terminal_text_pipeline.draw(pass)
            });
        }

        self.queue.submit(std::iter::once(encoder.finish()));
        frame.present();
        Ok(())
    }
}

// ── Helpers ────────────────────────────────────────────────────────────────────

/// Draw text content clipped to `scissor`, then reset to full viewport.
fn draw_text_region<'a, F>(
    pass: &mut wgpu::RenderPass<'a>,
    scissor: Option<[u32; 4]>,
    vp_w: u32,
    vp_h: u32,
    draw_fn: F,
) where
    F: FnOnce(&mut wgpu::RenderPass<'a>),
{
    if let Some([sx, sy, sw, sh]) = scissor {
        let cx = sx.min(vp_w);
        let cy = sy.min(vp_h);
        let cw = sw.min(vp_w.saturating_sub(cx));
        let ch = sh.min(vp_h.saturating_sub(cy));
        if cw > 0 && ch > 0 {
            pass.set_scissor_rect(cx, cy, cw, ch);
            draw_fn(pass);
            pass.set_scissor_rect(0, 0, vp_w, vp_h);
        }
    } else {
        draw_fn(pass);
    }
}

/// Lay out `text` into `GlyphInstance`s without cursor/caret logic.
/// Shares the single `GlyphAtlas` with the editor region.
fn layout_panel_text(
    text: &str,
    text_system: &mut TextSystem,
    atlas: &mut GlyphAtlas,
    queue: &wgpu::Queue,
    origin_x: f32,
    origin_y: f32,
    color: [f32; 4],
) -> Vec<GlyphInstance> {
    let color_u8 = [
        (color[0].clamp(0.0, 1.0) * 255.0).round() as u8,
        (color[1].clamp(0.0, 1.0) * 255.0).round() as u8,
        (color[2].clamp(0.0, 1.0) * 255.0).round() as u8,
        (color[3].clamp(0.0, 1.0) * 255.0).round() as u8,
    ];
    text_system.set_text_with_color(text, color_u8);
    let visible = text_system.collect_visible_glyphs(origin_x, origin_y, color);
    let mut instances = Vec::with_capacity(visible.len());

    for glyph in visible {
        let entry = if let Some(e) = atlas.get(glyph.cache_key) {
            e
        } else {
            let Some(rasterized) = rasterize_glyph_alpha(text_system, glyph.cache_key) else {
                continue;
            };
            match atlas.get_or_insert(queue, glyph.cache_key, &rasterized) {
                Ok(e) => e,
                Err(_) => continue,
            }
        };

        if entry.region.width == 0 || entry.region.height == 0 {
            continue;
        }

        let (uv_min, uv_max) = atlas.uv_min_max(entry.region);
        let tl_x = glyph.physical_x + entry.placement_left;
        let tl_y = glyph.physical_y - entry.placement_top;
        instances.push(GlyphInstance::new(
            [tl_x as f32, tl_y as f32],
            [entry.region.width as f32, entry.region.height as f32],
            uv_min,
            uv_max,
            glyph.color,
        ));
    }

    instances
}

fn theme_color_to_wgpu(color: ThemeColor) -> wgpu::Color {
    let [r, g, b, a] = color.as_f32();
    wgpu::Color {
        r: f64::from(r),
        g: f64::from(g),
        b: f64::from(b),
        a: f64::from(a),
    }
}
