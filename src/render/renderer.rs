use std::sync::Arc;

use cosmic_text::Metrics;
use winit::{dpi::PhysicalSize, window::Window};

use crate::{
    app::{app_state::AppState, command_palette::CommandPaletteRenderModel},
    config::{
        theme_config::{ThemeColor, ThemeConfig},
        ui_config::{CursorShape, UiConfig},
    },
    core::mode::EditorMode,
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
        layout_sync::{compute_caret_layout, compute_cursor_overlay, rebuild_layout_projection},
        raster::rasterize_glyph_alpha,
        text_system::{StyledTextSpan, TextSystem},
    },
};

const ATLAS_SIZE: u32 = 2048;
const EMPTY_TERMINAL_HINT: &str = "(terminal ready — press Ctrl+` to toggle)";

/// One visible row of the Explorer tree as consumed by the renderer.
/// Depth drives x-indentation; `icon` is the disclosure glyph (▶/▼/·).
#[derive(Debug, Clone)]
pub struct SidebarRow {
    pub depth: usize,
    pub icon: &'static str,
    pub label: String,
    pub is_selected: bool,
}

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
    /// Re-draws the glyph under the block caret on top of it with a contrast
    /// color, so the character stays readable in Normal/Visual mode.
    editor_cursor_overlay_pipeline: TextPipeline,
    editor_scissor: Option<[u32; 4]>,
    // ── Gutter (line numbers) ─────────────────────────────────────────────────
    gutter_text_system: TextSystem,
    gutter_text_pipeline: TextPipeline,
    gutter_glyph_instances: Vec<GlyphInstance>,
    pub relative_numbers: bool,
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
    // ── Welcome ANSI logo overlay ─────────────────────────────────────────────
    welcome_logo_text_system: TextSystem,
    welcome_logo_text_pipeline: TextPipeline,
    welcome_logo_view_renderer: TerminalViewRenderer,
    welcome_logo_glyph_instances: Vec<GlyphInstance>,
    welcome_logo_scissor: Option<[u32; 4]>,
    // ── TopBar (tab bar) ──────────────────────────────────────────────────────
    topbar_text_system: TextSystem,
    topbar_text_pipeline: TextPipeline,
    topbar_glyph_instances: Vec<GlyphInstance>,
    topbar_scissor: Option<[u32; 4]>,
    // ── StatusBar ─────────────────────────────────────────────────────────────
    statusbar_text_system: TextSystem,
    statusbar_text_pipeline: TextPipeline,
    statusbar_glyph_instances: Vec<GlyphInstance>,
    statusbar_scissor: Option<[u32; 4]>,
    // ── Command Palette / File Picker overlay ─────────────────────────────────
    palette_text_system: TextSystem,
    palette_text_pipeline: TextPipeline,
    palette_glyph_instances: Vec<GlyphInstance>,
    palette_chrome_instances: Vec<RegionDrawInstance>,
    palette_scissor: Option<[u32; 4]>,
    // ── UI config driven runtime knobs ───────────────────────────────────────
    editor_padding_x: f32,
    editor_padding_y: f32,
    panel_padding: f32,
    sidebar_base_padding: f32,
    sidebar_indent_per_depth: f32,
    topbar_padding_x: f32,
    statusbar_padding_x: f32,
    statusbar_font_size: f32,
    statusbar_line_height: f32,
    cursor_shape: CursorShape,
    cursor_beam_width: f32,
    cursor_block_width: f32,
    cursor_underline_height: f32,
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
        let statusbar_font_size = theme.ui.sidebar_font_size;
        let statusbar_line_height = theme.ui.sidebar_line_height;
        let atlas = GlyphAtlas::new(&device, ATLAS_SIZE, ATLAS_SIZE);
        let font_family = theme.editor.font_family.as_deref();
        let mut text_system = TextSystem::new(
            Metrics::new(theme.editor.font_size, theme.editor.line_height),
            None,
            None,
        );
        text_system.set_font_family(font_family);
        let text_pipeline = TextPipeline::new(&device, fmt, &atlas, w, h);

        let mut gutter_text_system = TextSystem::new(
            Metrics::new(theme.editor.font_size, theme.editor.line_height),
            None,
            None,
        );
        gutter_text_system.set_font_family(font_family);
        let gutter_text_pipeline = TextPipeline::new(&device, fmt, &atlas, w, h);
        let caret_pipeline = CaretPipeline::new(&device, fmt, w, h);
        let editor_cursor_overlay_pipeline = TextPipeline::new(&device, fmt, &atlas, w, h);

        let mut sidebar_text_system = TextSystem::new(
            Metrics::new(theme.ui.sidebar_font_size, theme.ui.sidebar_line_height),
            None,
            None,
        );
        sidebar_text_system.set_font_family(font_family);
        let sidebar_text_pipeline = TextPipeline::new(&device, fmt, &atlas, w, h);

        let mut terminal_text_system = TextSystem::new(
            Metrics::new(theme.ui.panel_font_size, theme.ui.panel_line_height),
            None,
            None,
        );
        terminal_text_system.set_font_family(font_family);
        let terminal_text_pipeline = TextPipeline::new(&device, fmt, &atlas, w, h);

        let mut welcome_logo_text_system = TextSystem::new(
            Metrics::new(theme.ui.panel_font_size, theme.ui.panel_line_height),
            None,
            None,
        );
        welcome_logo_text_system.set_font_family(font_family);
        let welcome_logo_text_pipeline = TextPipeline::new(&device, fmt, &atlas, w, h);

        let mut topbar_text_system = TextSystem::new(
            Metrics::new(theme.ui.sidebar_font_size, theme.ui.sidebar_line_height),
            None,
            None,
        );
        topbar_text_system.set_font_family(font_family);
        let topbar_text_pipeline = TextPipeline::new(&device, fmt, &atlas, w, h);

        let mut statusbar_text_system = TextSystem::new(
            Metrics::new(theme.ui.sidebar_font_size, theme.ui.sidebar_line_height),
            None,
            None,
        );
        statusbar_text_system.set_font_family(font_family);
        let statusbar_text_pipeline = TextPipeline::new(&device, fmt, &atlas, w, h);

        let mut palette_text_system = TextSystem::new(
            Metrics::new(theme.ui.sidebar_font_size, theme.ui.sidebar_line_height),
            None,
            None,
        );
        palette_text_system.set_font_family(font_family);
        let palette_text_pipeline = TextPipeline::new(&device, fmt, &atlas, w, h);

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
            terminal_scissor: None,
            welcome_logo_text_system,
            welcome_logo_text_pipeline,
            welcome_logo_view_renderer: TerminalViewRenderer::default_monospace(),
            welcome_logo_glyph_instances: Vec::new(),
            welcome_logo_scissor: None,
            topbar_text_system,
            topbar_text_pipeline,
            topbar_glyph_instances: Vec::new(),
            topbar_scissor: None,
            statusbar_text_system,
            statusbar_text_pipeline,
            statusbar_glyph_instances: Vec::new(),
            statusbar_scissor: None,
            palette_text_system,
            palette_text_pipeline,
            palette_glyph_instances: Vec::new(),
            palette_chrome_instances: Vec::new(),
            palette_scissor: None,
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
        self.topbar_text_system.set_metrics(Metrics::new(
            theme.ui.sidebar_font_size,
            theme.ui.sidebar_line_height,
        ));
        self.statusbar_text_system.set_metrics(Metrics::new(
            theme.ui.sidebar_font_size,
            theme.ui.sidebar_line_height,
        ));
        self.palette_text_system.set_metrics(Metrics::new(
            theme.ui.sidebar_font_size,
            theme.ui.sidebar_line_height,
        ));
        let family = theme.editor.font_family.as_deref();
        self.text_system.set_font_family(family);
        self.sidebar_text_system.set_font_family(family);
        self.terminal_text_system.set_font_family(family);
        self.topbar_text_system.set_font_family(family);
        self.statusbar_text_system.set_font_family(family);
        self.palette_text_system.set_font_family(family);
        self.clear_color = theme_color_to_wgpu(theme.editor.bg);
        self.theme = theme;
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
    }

    /// Rebuild glyph instances and caret for the center editor region.
    pub fn update_editor_content(
        &mut self,
        text: &str,
        app_state: &AppState,
        center_bounds: [f32; 4],
        spans: &[StyledTextSpan],
    ) {
        let line_height = self.theme.editor.line_height;
        let font_size = self.theme.editor.font_size;
        let total_lines = app_state.total_lines().max(1);
        let scroll_line = app_state.scroll_line;

        // Gutter width: right-aligned digits + padding (sized for gutter_font_size)
        let gutter_digits = total_lines.to_string().len().max(3);
        let gutter_width = gutter_width_for_editor(gutter_digits, font_size, line_height);

        let scroll_y = scroll_line as f32 * line_height;
        let origin_x = center_bounds[0] + self.editor_padding_x + gutter_width;
        let origin_y = center_bounds[1] + self.editor_padding_y + line_height - scroll_y;
        // Keep left padding + gutter, but let text use the full remaining
        // center width so content reaches the right edge naturally.
        let width = (center_bounds[2] - self.editor_padding_x - gutter_width).max(1.0);

        self.editor_scissor = rect_to_scissor(center_bounds);

        // Let cosmic-text shape full-height content (height=None) and rely on
        // scissor clipping for viewport visibility. This avoids accidental
        // truncation when layout line heights differ from rough estimates.
        self.text_system.set_size(Some(width), None);

        let result = rebuild_layout_projection(
            text,
            app_state,
            &mut self.text_system,
            &mut self.atlas,
            &self.queue,
            [origin_x, origin_y],
            self.theme.editor.fg.as_f32(),
            self.theme.editor.bg.as_f32(),
            spans,
        );

        match result {
            Ok(projection) => {
                self.glyph_instances = projection.glyph_instances;
                let caret = caret_rect_for_mode(
                    projection.caret_layout,
                    app_state.current_mode(),
                    self.theme.editor.cursor.as_f32(),
                    self.theme.editor.font_size,
                    self.cursor_shape,
                    self.cursor_beam_width,
                    self.cursor_block_width,
                    self.cursor_underline_height,
                );
                self.caret_pipeline.upload_caret(&self.queue, Some(caret));
                let overlay_instances: Vec<GlyphInstance> = projection
                    .cursor_overlay
                    .filter(|_| {
                        should_draw_block_cursor(app_state.current_mode(), self.cursor_shape)
                    })
                    .into_iter()
                    .collect();
                self.editor_cursor_overlay_pipeline.upload_instances(
                    &self.device,
                    &self.queue,
                    &overlay_instances,
                );
            }
            Err(e) => {
                eprintln!("[Renderer] text layout: {e}");
                self.glyph_instances.clear();
                self.caret_pipeline.upload_caret(&self.queue, None);
                self.editor_cursor_overlay_pipeline.upload_instances(
                    &self.device,
                    &self.queue,
                    &[],
                );
            }
        }

        self.text_pipeline
            .upload_instances(&self.device, &self.queue, &self.glyph_instances);

        self.update_editor_gutter(
            app_state,
            center_bounds,
            line_height,
            font_size,
            gutter_digits,
            gutter_width,
        );
    }

    /// Fast path for cursor movement: reuse existing layout and update caret only.
    ///
    /// Must honor the same mode → shape mapping as `update_editor_content`, otherwise
    /// h/j/k/l in Normal mode would collapse the block caret back to a thin bar.
    pub fn update_editor_caret(&mut self, app_state: &AppState, center_bounds: [f32; 4]) {
        let line_height = self.theme.editor.line_height;
        let font_size = self.theme.editor.font_size;
        let total_lines = app_state.total_lines().max(1);
        let scroll_y = app_state.scroll_line as f32 * line_height;
        let gutter_digits = total_lines.to_string().len().max(3);
        let gutter_width = gutter_width_for_editor(gutter_digits, font_size, line_height);
        let origin_x = center_bounds[0] + self.editor_padding_x + gutter_width;
        let origin_y = center_bounds[1] + self.editor_padding_y + line_height - scroll_y;
        let caret_layout = compute_caret_layout(&self.text_system, app_state, [origin_x, origin_y]);
        let caret = caret_rect_for_mode(
            caret_layout,
            app_state.current_mode(),
            self.theme.editor.cursor.as_f32(),
            self.theme.editor.font_size,
            self.cursor_shape,
            self.cursor_beam_width,
            self.cursor_block_width,
            self.cursor_underline_height,
        );
        self.caret_pipeline.upload_caret(&self.queue, Some(caret));

        let overlay = if should_draw_block_cursor(app_state.current_mode(), self.cursor_shape) {
            compute_cursor_overlay(
                &mut self.text_system,
                app_state,
                &mut self.atlas,
                &self.queue,
                [origin_x, origin_y],
                self.theme.editor.bg.as_f32(),
            )
            .unwrap_or(None)
        } else {
            None
        };
        let overlay_instances: Vec<GlyphInstance> = overlay.into_iter().collect();
        self.editor_cursor_overlay_pipeline.upload_instances(
            &self.device,
            &self.queue,
            &overlay_instances,
        );

        self.update_editor_gutter(
            app_state,
            center_bounds,
            line_height,
            font_size,
            gutter_digits,
            gutter_width,
        );
    }

    pub fn current_line_highlight_quad(
        &self,
        app_state: &AppState,
        center_bounds: [f32; 4],
    ) -> Option<RegionDrawInstance> {
        let line_height = self.theme.editor.line_height;
        let total_lines = app_state.total_lines().max(1);
        let font_size = self.theme.editor.font_size;
        let gutter_digits = total_lines.to_string().len().max(3);
        let gutter_width = gutter_width_for_editor(gutter_digits, font_size, line_height);
        let text_area_x = center_bounds[0] + self.editor_padding_x + gutter_width;
        let text_area_w = (center_bounds[2] - self.editor_padding_x - gutter_width).max(1.0);
        let scroll_y = app_state.scroll_line as f32 * line_height;
        let origin_y = center_bounds[1] + self.editor_padding_y + line_height - scroll_y;
        let caret_layout =
            compute_caret_layout(&self.text_system, app_state, [text_area_x, origin_y]);

        let viewport_top = center_bounds[1] + self.editor_padding_y;
        let viewport_bottom =
            viewport_top + (center_bounds[3] - self.editor_padding_y * 2.0 - line_height).max(1.0);
        let line_top = caret_layout.top;
        let line_bottom = line_top + caret_layout.height.max(1.0);
        if line_bottom <= viewport_top || line_top >= viewport_bottom {
            return None;
        }

        let mut color = self.theme.editor.selection.as_f32();
        color[3] = (color[3] * 0.22).clamp(0.10, 0.30);
        Some(RegionDrawInstance::new(
            [
                text_area_x,
                line_top,
                text_area_w,
                caret_layout.height.max(1.0),
            ],
            color,
        ))
    }

    pub fn visual_selection_quads(
        &self,
        app_state: &AppState,
        center_bounds: [f32; 4],
    ) -> Vec<RegionDrawInstance> {
        if app_state.current_mode() != EditorMode::Visual {
            return Vec::new();
        }
        let Some(selection) = app_state.visual_selection_range() else {
            return Vec::new();
        };

        let line_height = self.theme.editor.line_height;
        let total_lines = app_state.total_lines().max(1);
        let font_size = self.theme.editor.font_size;
        let gutter_digits = total_lines.to_string().len().max(3);
        let gutter_width = gutter_width_for_editor(gutter_digits, font_size, line_height);
        let text_area_x = center_bounds[0] + self.editor_padding_x + gutter_width;
        let text_area_w = (center_bounds[2] - self.editor_padding_x - gutter_width).max(1.0);
        let scroll_y = app_state.scroll_line as f32 * line_height;
        let origin_y = center_bounds[1] + self.editor_padding_y + line_height - scroll_y;

        let viewport_top = center_bounds[1] + self.editor_padding_y;
        let viewport_bottom =
            viewport_top + (center_bounds[3] - self.editor_padding_y * 2.0 - line_height).max(1.0);

        let run_x_for_byte = |run: &cosmic_text::LayoutRun, byte_in_line: usize| {
            if run.glyphs.is_empty() {
                return text_area_x;
            }
            let mut x = text_area_x + run.line_w;
            for glyph in run.glyphs {
                let left = text_area_x + glyph.x;
                let right = left + glyph.w;
                if byte_in_line <= glyph.start {
                    return left;
                }
                if byte_in_line < glyph.end {
                    return left;
                }
                x = right;
            }
            x
        };

        let mut color = self.theme.editor.selection.as_f32();
        color[3] = (color[3] * 0.45).clamp(0.18, 0.42);

        let mut quads = Vec::new();
        for run in self.text_system.buffer().layout_runs() {
            if run.line_i < selection.start_line || run.line_i > selection.end_line {
                continue;
            }

            let line_top = origin_y + run.line_top;
            let line_height_px = run.line_height.max(1.0);
            let line_bottom = line_top + line_height_px;
            if line_bottom <= viewport_top || line_top >= viewport_bottom {
                continue;
            }

            let line_start_x = text_area_x;
            let line_end_x = (text_area_x + run.line_w).max(line_start_x + 1.0);
            let start_x = if run.line_i == selection.start_line {
                run_x_for_byte(&run, selection.start_byte_in_line)
            } else {
                line_start_x
            };
            let end_x = if run.line_i == selection.end_line {
                run_x_for_byte(&run, selection.end_byte_in_line)
            } else {
                line_end_x
            };

            let left = start_x.min(end_x).max(text_area_x);
            let right = start_x.max(end_x).min(text_area_x + text_area_w);
            let width = (right - left).max(1.0);
            quads.push(RegionDrawInstance::new(
                [left, line_top, width, line_height_px],
                color,
            ));
        }

        quads
    }

    fn update_editor_gutter(
        &mut self,
        app_state: &AppState,
        center_bounds: [f32; 4],
        line_height: f32,
        font_size: f32,
        gutter_digits: usize,
        gutter_width: f32,
    ) {
        let total_lines = app_state.total_lines().max(1);
        let scroll_line = app_state.scroll_line;
        let viewport_height =
            (center_bounds[3] - self.editor_padding_y * 2.0 - line_height).max(1.0);
        let viewport_lines = (viewport_height / line_height).ceil() as usize + 1;
        let (cursor_line, _) = app_state.cursor_line_col();
        let gutter_color = self.theme.editor.gutter.as_f32();
        let gutter_active_color = self.theme.editor.gutter_active.as_f32();
        let gutter_x = center_bounds[0] + self.editor_padding_x;
        let gutter_font_size = (font_size + 3.0).min(line_height - 2.0).max(8.0);
        self.gutter_text_system
            .set_metrics(Metrics::new(gutter_font_size, line_height));
        self.gutter_text_system
            .set_size(Some(gutter_width), Some(line_height));
        let mut gutter_glyphs: Vec<GlyphInstance> = Vec::new();
        for i in 0..viewport_lines {
            let abs_line = scroll_line + i;
            if abs_line >= total_lines {
                break;
            }
            let num_str = if self.relative_numbers {
                let dist = abs_line.abs_diff(cursor_line);
                if dist == 0 {
                    format!("{}", abs_line + 1)
                } else {
                    format!("{dist}")
                }
            } else {
                format!("{}", abs_line + 1)
            };
            let label = format!("{:>width$} ", num_str, width = gutter_digits);
            let color = if abs_line == cursor_line {
                gutter_active_color
            } else {
                gutter_color
            };
            let line_y = center_bounds[1] + self.editor_padding_y + (i as f32 + 1.0) * line_height;
            gutter_glyphs.extend(layout_panel_text(
                &label,
                &mut self.gutter_text_system,
                &mut self.atlas,
                &self.queue,
                gutter_x,
                line_y,
                color,
            ));
        }
        self.gutter_glyph_instances = gutter_glyphs;
        self.gutter_text_pipeline.upload_instances(
            &self.device,
            &self.queue,
            &self.gutter_glyph_instances,
        );
    }

    /// Render the explorer file tree into the left sidebar region.
    ///
    /// Each row is laid out independently so we can:
    /// - honor `base_padding + depth * 15px` per depth,
    /// - prefix the disclosure icon,
    /// - emit a selection quad for the focused row.
    ///
    /// The selection quads are returned so the caller can draw them via
    /// the region pipeline *before* the text pass.
    pub fn update_sidebar_content(
        &mut self,
        header: Option<&str>,
        rows: &[SidebarRow],
        bounds: [f32; 4],
        sidebar_focused: bool,
    ) -> Vec<RegionDrawInstance> {
        if bounds[2] < 1.0 || bounds[3] < 1.0 {
            self.sidebar_scissor = None;
            self.sidebar_glyph_instances.clear();
            self.sidebar_text_pipeline
                .upload_instances(&self.device, &self.queue, &[]);
            return Vec::new();
        }

        self.sidebar_scissor = rect_to_scissor(bounds);
        let line_h = self.theme.ui.sidebar_line_height;
        let fg = self.theme.editor.fg.as_f32();
        let accent = self.theme.ui.accent.as_f32();
        let sel_bg = if sidebar_focused {
            [accent[0], accent[1], accent[2], 0.18]
        } else {
            let c = self.theme.ui.selection_bg.as_f32();
            [c[0], c[1], c[2], 0.55]
        };

        let width = (bounds[2] - self.panel_padding * 2.0).max(1.0);
        let height = (bounds[3] - self.panel_padding * 2.0).max(1.0);
        self.sidebar_text_system.set_size(Some(width), Some(height));

        let mut glyphs: Vec<GlyphInstance> = Vec::new();
        let mut selection_quads: Vec<RegionDrawInstance> = Vec::new();

        let mut row_top = bounds[1] + self.panel_padding;

        // Each row is its own single-line buffer, so we pass row_top as origin
        // and cosmic-text adds the ascent internally — that lands the glyph
        // inside [row_top, row_top + line_h], which matches the selection quad.
        if let Some(header) = header {
            glyphs.extend(layout_panel_text(
                header,
                &mut self.sidebar_text_system,
                &mut self.atlas,
                &self.queue,
                bounds[0] + self.sidebar_base_padding,
                row_top,
                fg,
            ));
            row_top += line_h;
        }

        for row in rows {
            let x = bounds[0]
                + self.sidebar_base_padding
                + row.depth as f32 * self.sidebar_indent_per_depth;

            let row_fg = if row.is_selected {
                selection_quads.push(RegionDrawInstance::new(
                    [bounds[0] + 2.0, row_top, (bounds[2] - 4.0).max(0.0), line_h],
                    sel_bg,
                ));
                if sidebar_focused { accent } else { fg }
            } else {
                fg
            };

            let text = format!("{} {}", row.icon, row.label);
            glyphs.extend(layout_panel_text(
                &text,
                &mut self.sidebar_text_system,
                &mut self.atlas,
                &self.queue,
                x,
                row_top,
                row_fg,
            ));
            row_top += line_h;
        }

        self.sidebar_glyph_instances = glyphs;
        self.sidebar_text_pipeline.upload_instances(
            &self.device,
            &self.queue,
            &self.sidebar_glyph_instances,
        );
        selection_quads
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
        let origin_x = bounds[0] + self.panel_padding;
        let origin_y = bounds[1] + self.panel_padding + self.theme.ui.panel_line_height;
        let width = (bounds[2] - self.panel_padding * 2.0).max(1.0);
        let height =
            (bounds[3] - self.panel_padding * 2.0 - self.theme.ui.panel_line_height).max(1.0);

        self.terminal_scissor = rect_to_scissor(bounds);

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
            // Monospace-sized cells: derive from font metrics, never stretch to fit.
            // Without this, a wide panel inflates cell_width far past the glyph advance
            // and characters look spaced out like "N  e  t  h  e  r  i  z  e".
            let font_size = self.theme.ui.panel_font_size;
            let line_h = self.theme.ui.panel_line_height.max(1.0);
            self.terminal_view_renderer.origin_x = origin_x;
            self.terminal_view_renderer.origin_y = origin_y;
            self.terminal_view_renderer.cell_width = (font_size * 0.6).max(1.0);
            self.terminal_view_renderer.cell_height = line_h;
            self.terminal_view_renderer.font_size = font_size;

            self.terminal_glyph_instances = self.terminal_view_renderer.build_instances(
                grid,
                &mut self.atlas,
                &self.queue,
                &mut self.terminal_text_system,
                default_fg,
                default_bg,
                width,
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

    /// Render ANSI art into a dedicated welcome-logo layer.
    ///
    /// This is intentionally separate from the bottom terminal panel so the
    /// welcome screen can draw terminal-style ANSI blocks in the editor center
    /// without clobbering the real PTY panel state.
    pub fn update_welcome_logo_content(&mut self, grid: &TerminalGrid, bounds: [f32; 4]) {
        if bounds[2] < 1.0 || bounds[3] < 1.0 {
            self.welcome_logo_scissor = None;
            self.welcome_logo_glyph_instances.clear();
            self.welcome_logo_text_pipeline
                .upload_instances(&self.device, &self.queue, &[]);
            return;
        }

        self.welcome_logo_scissor = rect_to_scissor(bounds);

        let width = bounds[2].max(1.0);
        let height = bounds[3].max(1.0);
        let cols = grid.cols.max(1) as f32;
        let rows = grid.rows.max(1) as f32;

        // Keep the render metrics internally consistent with the cell size.
        // The welcome logo is ANSI art, so if glyph metrics stay at the default
        // panel font while the bounds grow, the art drifts and clips.
        let font_size = (height / rows).min(width / (cols * 0.6)).max(1.0);
        let cell_width = (font_size * 0.6).max(1.0);
        let cell_height = font_size.max(1.0);
        let rendered_width = (cell_width * cols).min(width);
        let rendered_height = (cell_height * rows).min(height);
        let origin_x = bounds[0] + ((width - rendered_width) * 0.5).max(0.0);
        let origin_y = bounds[1] + ((height - rendered_height) * 0.5).max(0.0);

        self.welcome_logo_text_system
            .set_metrics(Metrics::new(font_size, cell_height));

        self.welcome_logo_view_renderer.origin_x = origin_x;
        self.welcome_logo_view_renderer.origin_y = origin_y;
        self.welcome_logo_view_renderer.cell_width = cell_width;
        self.welcome_logo_view_renderer.cell_height = cell_height;
        self.welcome_logo_view_renderer.font_size = font_size;

        let default_fg = self.theme.editor.fg.as_f32();
        let default_bg = self.theme.editor.bg.as_f32();

        self.welcome_logo_glyph_instances = self.welcome_logo_view_renderer.build_instances(
            grid,
            &mut self.atlas,
            &self.queue,
            &mut self.welcome_logo_text_system,
            default_fg,
            default_bg,
            rendered_width,
        );

        self.welcome_logo_text_pipeline.upload_instances(
            &self.device,
            &self.queue,
            &self.welcome_logo_glyph_instances,
        );
    }

    pub fn clear_welcome_logo(&mut self) {
        self.welcome_logo_scissor = None;
        self.welcome_logo_glyph_instances.clear();
        self.welcome_logo_text_pipeline
            .upload_instances(&self.device, &self.queue, &[]);
    }

    /// Render the tab bar on top: shows the active file name with an accent bottom border.
    /// Returns the active-tab underline quad to be drawn over the region background.
    pub fn update_topbar_content(
        &mut self,
        active_file: Option<&str>,
        bounds: [f32; 4],
    ) -> Vec<RegionDrawInstance> {
        if bounds[2] < 1.0 || bounds[3] < 1.0 {
            self.topbar_scissor = None;
            self.topbar_glyph_instances.clear();
            self.topbar_text_pipeline
                .upload_instances(&self.device, &self.queue, &[]);
            return vec![];
        }
        self.topbar_scissor = rect_to_scissor(bounds);
        let line_h = self.statusbar_line_height;
        let font_size = self.statusbar_font_size;
        let width = (bounds[2] - self.topbar_padding_x * 2.0).max(1.0);
        self.topbar_text_system
            .set_size(Some(width), Some(bounds[3]));

        let tab_label = active_file.unwrap_or("[ no file ]");
        let tab_text = format!("  {}  ", tab_label);
        let tab_width = estimate_monospace_width(&tab_text, font_size);
        let origin_y = bounds[1] + ((bounds[3] - line_h) * 0.5).max(0.0);

        let fg = if active_file.is_some() {
            self.theme.editor.fg.as_f32()
        } else {
            self.theme.ui.fg_ghost.as_f32()
        };

        self.topbar_glyph_instances = layout_panel_text(
            &tab_text,
            &mut self.topbar_text_system,
            &mut self.atlas,
            &self.queue,
            bounds[0] + self.topbar_padding_x,
            origin_y,
            fg,
        );
        self.topbar_text_pipeline.upload_instances(
            &self.device,
            &self.queue,
            &self.topbar_glyph_instances,
        );

        // Accent underline on the bottom edge of the tab
        let tab_x = bounds[0] + self.topbar_padding_x;
        let underline_y = bounds[1] + bounds[3] - 2.0;
        let accent = self.theme.ui.accent.as_f32();
        vec![RegionDrawInstance::new(
            [tab_x, underline_y, tab_width.min(bounds[2]), 2.0],
            accent,
        )]
    }

    /// Render the status bar:
    /// left = mode badge + pending chord,
    /// right = git/filetype/encoding/line-column metadata.
    /// Returns background + border + badge quads to be drawn before text.
    pub fn update_statusbar_content(
        &mut self,
        mode: EditorMode,
        pending_keys: &str,
        git_branch: &str,
        filetype: &str,
        line: usize,
        col: usize,
        bounds: [f32; 4],
    ) -> Vec<RegionDrawInstance> {
        if bounds[2] < 1.0 || bounds[3] < 1.0 {
            self.statusbar_scissor = None;
            self.statusbar_glyph_instances.clear();
            self.statusbar_text_pipeline
                .upload_instances(&self.device, &self.queue, &[]);
            return vec![];
        }
        self.statusbar_scissor = rect_to_scissor(bounds);
        let line_h = self.statusbar_line_height;
        let font_size = self.statusbar_font_size;
        let width = (bounds[2] - self.statusbar_padding_x * 2.0).max(1.0);
        self.statusbar_text_system
            .set_size(Some(width), Some(bounds[3]));

        let mode_label = mode_display_label(mode);
        let mode_color = mode_pill_color(mode, &self.theme);
        let pill_text = format!("  {}  ", mode_label);
        let pill_width = estimate_monospace_width(&pill_text, font_size);
        let pill_x = bounds[0] + self.statusbar_padding_x;
        let pill_height = (bounds[3] - 6.0).max(line_h).min(bounds[3]);
        let pill_y = bounds[1] + ((bounds[3] - pill_height) * 0.5).max(0.0);
        let pill_rect = [pill_x, pill_y, pill_width, pill_height];

        let branch_label = if git_branch.trim().is_empty() {
            "git: -".to_string()
        } else {
            let branch = git_branch.trim();
            if branch.starts_with("git: ") {
                branch.to_string()
            } else {
                format!("git: {branch}")
            }
        };
        let right_text = format!(
            "{}  |  {filetype}  |  UTF-8  |  LF  |  Ln {}, Col {}",
            branch_label,
            line + 1,
            col + 1
        );

        let origin_y = bounds[1] + ((bounds[3] - line_h) * 0.5).max(0.0);
        let fg_dim = self.theme.ui.fg_dim.as_f32();
        let accent = self.theme.ui.accent.as_f32();

        // Mode pill text in dark bg color for contrast
        let pill_fg = [0.07, 0.08, 0.09, 1.0];
        let mut glyphs = layout_panel_text(
            &pill_text,
            &mut self.statusbar_text_system,
            &mut self.atlas,
            &self.queue,
            pill_x,
            origin_y,
            pill_fg,
        );

        let right_width = estimate_monospace_width(&right_text, font_size);
        let right_origin_x =
            (bounds[0] + bounds[2] - self.statusbar_padding_x - right_width)
                .max(bounds[0] + self.statusbar_padding_x);
        let pending_origin_x = pill_x + pill_width + self.statusbar_padding_x * 0.75;
        let pending_gap = self.statusbar_padding_x;
        let pending_max_width = (right_origin_x - pending_origin_x - pending_gap).max(0.0);
        let pending_text = clamp_monospace_text(pending_keys, pending_max_width, font_size);
        if !pending_text.is_empty() {
            glyphs.extend(layout_panel_text(
                &pending_text,
                &mut self.statusbar_text_system,
                &mut self.atlas,
                &self.queue,
                pending_origin_x,
                origin_y,
                accent,
            ));
        }

        glyphs.extend(layout_panel_text(
            &right_text,
            &mut self.statusbar_text_system,
            &mut self.atlas,
            &self.queue,
            right_origin_x,
            origin_y,
            fg_dim,
        ));

        self.statusbar_glyph_instances = glyphs;
        self.statusbar_text_pipeline.upload_instances(
            &self.device,
            &self.queue,
            &self.statusbar_glyph_instances,
        );

        vec![
            RegionDrawInstance::new(bounds, self.theme.ui.status_bar_bg.as_f32()),
            RegionDrawInstance::new(
                [bounds[0], bounds[1], bounds[2], 1.0_f32.min(bounds[3])],
                self.theme.ui.border_color.as_f32(),
            ),
            RegionDrawInstance::new(pill_rect, mode_color),
        ]
    }

    /// Layout + upload command palette overlay (chrome + text).
    ///
    /// The palette model is precomputed by `CommandPalette::render()` so the
    /// renderer only consumes geometry/text and uploads GPU instances.
    pub fn update_palette_content(&mut self, model: &CommandPaletteRenderModel) {
        let [panel_x, panel_y, panel_w, panel_h] = model.panel_bounds;
        self.palette_scissor = rect_to_scissor(model.panel_bounds);

        let inner_width = (panel_w - model.panel_padding * 2.0).max(1.0);
        let inner_height = (panel_h - model.panel_padding * 2.0).max(1.0);
        self.palette_text_system
            .set_size(Some(inner_width), Some(inner_height));

        let mut quads: Vec<RegionDrawInstance> = Vec::new();
        quads.push(RegionDrawInstance::new(
            model.overlay_bounds,
            model.scrim_color,
        ));
        quads.push(RegionDrawInstance::new(
            [panel_x - 1.0, panel_y - 1.0, panel_w + 2.0, panel_h + 2.0],
            model.border_color,
        ));
        quads.push(RegionDrawInstance::new(model.panel_bounds, model.panel_bg));

        let mut glyphs: Vec<GlyphInstance> = Vec::new();
        let text_x = panel_x + model.panel_padding;
        let mut row_top = panel_y + model.panel_padding;
        let line_h = model.line_height.max(16.0);

        glyphs.extend(layout_panel_text(
            &model.title,
            &mut self.palette_text_system,
            &mut self.atlas,
            &self.queue,
            text_x,
            row_top,
            model.hint_color,
        ));
        row_top += line_h;

        glyphs.extend(layout_panel_text(
            &model.prompt_line,
            &mut self.palette_text_system,
            &mut self.atlas,
            &self.queue,
            text_x,
            row_top,
            model.text_color,
        ));
        row_top += line_h;

        quads.push(RegionDrawInstance::new(
            [
                panel_x + model.panel_padding,
                row_top + line_h * 0.40,
                inner_width,
                1.0,
            ],
            model.border_color,
        ));
        row_top += line_h;

        let max_visible =
            (((panel_h - model.panel_padding * 2.0) / line_h).floor() as usize).saturating_sub(3);
        for (idx, label) in model.result_labels.iter().enumerate().take(max_visible) {
            if idx == model.selected_index {
                quads.push(RegionDrawInstance::new(
                    [panel_x + 4.0, row_top, (panel_w - 8.0).max(0.0), line_h],
                    model.selection_bg,
                ));
            }
            glyphs.extend(layout_panel_text(
                label,
                &mut self.palette_text_system,
                &mut self.atlas,
                &self.queue,
                text_x,
                row_top,
                model.text_color,
            ));
            row_top += line_h;
        }

        if model.result_labels.is_empty() {
            glyphs.extend(layout_panel_text(
                "(no matches)",
                &mut self.palette_text_system,
                &mut self.atlas,
                &self.queue,
                text_x,
                row_top,
                model.hint_color,
            ));
        }

        self.palette_chrome_instances = quads;
        self.palette_glyph_instances = glyphs;
        self.palette_text_pipeline.upload_instances(
            &self.device,
            &self.queue,
            &self.palette_glyph_instances,
        );
    }

    pub fn clear_palette(&mut self) {
        self.palette_scissor = None;
        self.palette_chrome_instances.clear();
        self.palette_glyph_instances.clear();
        self.palette_text_pipeline
            .upload_instances(&self.device, &self.queue, &[]);
    }

    pub fn resize(&mut self, new_size: PhysicalSize<u32>) {
        self.surface_state.resize(&self.device, new_size);
        let (w, h) = (new_size.width.max(1), new_size.height.max(1));
        self.region_pipeline.update_screen_size(&self.queue, w, h);
        self.text_pipeline.update_screen_size(&self.queue, w, h);
        self.caret_pipeline.update_screen_size(&self.queue, w, h);
        self.editor_cursor_overlay_pipeline
            .update_screen_size(&self.queue, w, h);
        self.sidebar_text_pipeline
            .update_screen_size(&self.queue, w, h);
        self.terminal_text_pipeline
            .update_screen_size(&self.queue, w, h);
        self.welcome_logo_text_pipeline
            .update_screen_size(&self.queue, w, h);
        self.topbar_text_pipeline
            .update_screen_size(&self.queue, w, h);
        self.statusbar_text_pipeline
            .update_screen_size(&self.queue, w, h);
        self.palette_text_pipeline
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

            // 2. Editor text + caret + cursor overlay + gutter: clipped to center region.
            draw_text_region(&mut pass, self.editor_scissor, vp_w, vp_h, |pass| {
                self.text_pipeline.draw(pass);
                self.caret_pipeline.draw(pass);
                self.editor_cursor_overlay_pipeline.draw(pass);
                self.gutter_text_pipeline.draw(pass);
            });

            // 3. Welcome ANSI logo overlay in the editor area.
            draw_text_region(&mut pass, self.welcome_logo_scissor, vp_w, vp_h, |pass| {
                self.welcome_logo_text_pipeline.draw(pass)
            });

            // 4. Explorer sidebar text
            draw_text_region(&mut pass, self.sidebar_scissor, vp_w, vp_h, |pass| {
                self.sidebar_text_pipeline.draw(pass)
            });

            // 5. Terminal panel text
            draw_text_region(&mut pass, self.terminal_scissor, vp_w, vp_h, |pass| {
                self.terminal_text_pipeline.draw(pass)
            });

            // 6. Top bar (tabs) text
            draw_text_region(&mut pass, self.topbar_scissor, vp_w, vp_h, |pass| {
                self.topbar_text_pipeline.draw(pass)
            });

            // 7. Status bar text (mode + Ln/Col)
            draw_text_region(&mut pass, self.statusbar_scissor, vp_w, vp_h, |pass| {
                self.statusbar_text_pipeline.draw(pass)
            });

            // 8. Command palette chrome (scrim + box) — must render after editor
            // text so it actually covers code beneath.
            if !self.palette_chrome_instances.is_empty() {
                self.region_pipeline.upload_instances(
                    &self.device,
                    &self.queue,
                    &self.palette_chrome_instances,
                );
                self.region_pipeline.draw(&mut pass);
            }

            // 9. Command palette / file picker overlay text (drawn last → on top)
            draw_text_region(&mut pass, self.palette_scissor, vp_w, vp_h, |pass| {
                self.palette_text_pipeline.draw(pass)
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

fn rect_to_scissor(bounds: [f32; 4]) -> Option<[u32; 4]> {
    let sx = bounds[0].max(0.0).round() as u32;
    let sy = bounds[1].max(0.0).round() as u32;
    let sw = bounds[2].max(0.0).round() as u32;
    let sh = bounds[3].max(0.0).round() as u32;
    (sw > 0 && sh > 0).then_some([sx, sy, sw, sh])
}

fn estimate_monospace_width(text: &str, font_size: f32) -> f32 {
    // Rough monospace advance (~0.6 * em). Scissor guarantees nothing spills.
    text.chars().count() as f32 * font_size * 0.6
}

fn clamp_monospace_text(text: &str, max_width: f32, font_size: f32) -> String {
    if text.is_empty() || max_width <= 0.0 {
        return String::new();
    }
    if estimate_monospace_width(text, font_size) <= max_width {
        return text.to_string();
    }

    let char_width = (font_size * 0.6).max(1.0);
    let max_chars = (max_width / char_width).floor() as usize;
    if max_chars == 0 {
        return String::new();
    }
    if text.chars().count() <= max_chars {
        return text.to_string();
    }
    if max_chars <= 3 {
        return text.chars().take(max_chars).collect();
    }

    let mut shortened: String = text.chars().take(max_chars - 3).collect();
    shortened.push_str("...");
    shortened
}

/// Build the caret rect for the current mode.
/// Normal/Visual → uniform block caret sized from font metrics (not the
///                 glyph under the cursor) so width stays constant even
///                 when cursor moves across narrow/wide chars.
/// Insert and other modes → thin vertical bar.
fn caret_rect_for_mode(
    layout: crate::text::layout_sync::CaretLayout,
    mode: EditorMode,
    color: [f32; 4],
    font_size: f32,
    cursor_shape: CursorShape,
    beam_width: f32,
    block_width: f32,
    underline_height: f32,
) -> CaretScreenRect {
    let (x, y, width, height) = match cursor_shape {
        CursorShape::Beam => (
            layout.x,
            layout.top,
            beam_width.max(1.0),
            layout.height.max(1.0),
        ),
        CursorShape::Underline => {
            let h = underline_height.max(1.0);
            (
                layout.x,
                layout.top + (layout.height - h).max(0.0),
                block_width.max(font_size * 0.6).max(1.0),
                h,
            )
        }
        CursorShape::Block => {
            if is_mode_block_cursor(mode) {
                (
                    layout.x,
                    layout.top,
                    block_width.max(font_size * 0.6).max(1.0),
                    layout.height.max(1.0),
                )
            } else {
                (
                    layout.x,
                    layout.top,
                    beam_width.max(1.0),
                    layout.height.max(1.0),
                )
            }
        }
    };
    CaretScreenRect {
        x,
        y,
        width,
        height,
        color,
    }
}

fn is_mode_block_cursor(mode: EditorMode) -> bool {
    matches!(mode, EditorMode::Normal | EditorMode::Visual)
}

fn should_draw_block_cursor(mode: EditorMode, cursor_shape: CursorShape) -> bool {
    matches!(cursor_shape, CursorShape::Block) && is_mode_block_cursor(mode)
}

fn mode_display_label(mode: EditorMode) -> &'static str {
    match mode {
        EditorMode::Normal => "NORMAL",
        EditorMode::Insert => "INSERT",
        EditorMode::Visual => "VISUAL",
        EditorMode::PaletteFocus => "PALETTE",
        EditorMode::TerminalFocus => "TERMINAL",
    }
}

fn mode_pill_color(mode: EditorMode, theme: &ThemeConfig) -> [f32; 4] {
    match mode {
        EditorMode::Normal => theme.ui.accent.as_f32(),
        EditorMode::Insert => theme.ui.cyan.as_f32(),
        EditorMode::Visual => theme.ui.magenta.as_f32(),
        EditorMode::PaletteFocus => theme.ui.amber.as_f32(),
        EditorMode::TerminalFocus => theme.ui.success.as_f32(),
    }
}

fn gutter_width_for_editor(gutter_digits: usize, font_size: f32, line_height: f32) -> f32 {
    let gutter_char_w = (font_size + 3.0).min(line_height - 2.0).max(8.0) * 0.6;
    gutter_digits as f32 * gutter_char_w + 18.0
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
