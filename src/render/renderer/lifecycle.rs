//! Renderer lifecycle: GPU bootstrap, theme/config application, resize handling,
//! and the top-level render pass ordering.

mod frame;

use std::sync::Arc;

use cosmic_text::Metrics;
use winit::{dpi::PhysicalSize, window::Window};

use crate::{
    config::{
        theme_config::ThemeConfig,
        ui_config::{CursorShape, UiConfig},
    },
    render::{
        caret::CaretPipeline, region_pipeline::RegionPipeline, surface::SurfaceState,
        text_pipeline::TextPipeline,
    },
    terminal::terminal_renderer::TerminalViewRenderer,
    text::{atlas::GlyphAtlas, text_system::TextSystem},
};

use super::{ATLAS_SIZE, Renderer, helpers::theme_color_to_wgpu};

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
        let image_pipeline =
            crate::render::image_pipeline::ImagePipeline::new(&device, surface_format);
        let welcome_image_pipeline =
            crate::render::image_pipeline::ImagePipeline::new(&device, surface_format);
        let ai_chat_header_image_pipeline =
            crate::render::image_pipeline::ImagePipeline::new(&device, surface_format);
        let ai_chat_hero_image_pipeline =
            crate::render::image_pipeline::ImagePipeline::new(&device, surface_format);
        let topbar_logo_image_pipeline =
            crate::render::image_pipeline::ImagePipeline::new(&device, surface_format);

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
        let editor_overlay_text_system = make_text_system(editor_metrics, font_family.as_deref());
        let gutter_text_system = make_text_system(editor_metrics, font_family.as_deref());
        let sidebar_text_system = make_text_system(ui_metrics, nerd_family.as_deref());
        let terminal_text_system = make_text_system(panel_metrics, font_family.as_deref());
        let welcome_logo_text_system = make_text_system(panel_metrics, font_family.as_deref());
        let topbar_text_system = make_text_system(ui_metrics, font_family.as_deref());
        let statusbar_text_system = make_text_system(ui_metrics, font_family.as_deref());
        let palette_text_system = make_text_system(ui_metrics, font_family.as_deref());
        let lsp_guide_text_system = make_text_system(panel_metrics, font_family.as_deref());
        let system_dep_text_system = make_text_system(panel_metrics, font_family.as_deref());
        let diagnostic_hover_text_system = make_text_system(editor_metrics, font_family.as_deref());
        let ai_chat_text_system = make_text_system(ui_metrics, font_family.as_deref());
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
        let system_dep_text_pipeline =
            make_text_pipeline(&device, &atlas, surface_format, width, height);
        let diagnostic_hover_text_pipeline =
            make_text_pipeline(&device, &atlas, surface_format, width, height);
        let toast_text_pipeline =
            make_text_pipeline(&device, &atlas, surface_format, width, height);
        let leap_label_text_pipeline =
            make_text_pipeline(&device, &atlas, surface_format, width, height);
        let ai_chat_text_pipeline =
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
            editor_overlay_text_system,
            editor_overlay_text_pipeline,
            editor_overlay_glyph_instances: Vec::new(),
            editor_overlay_chrome_instances: Vec::new(),
            editor_overlay_scissor: None,
            image_pipeline,
            image_scissor: None,
            welcome_image_pipeline,
            welcome_image_scissor: None,
            gutter_text_system,
            gutter_text_pipeline,
            gutter_glyph_instances: Vec::new(),
            relative_numbers: false,
            last_editor_chrome_instances: Vec::new(),
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
            buffer_terminal_header_batch: None,
            welcome_logo_text_system,
            welcome_logo_text_pipeline,
            welcome_logo_view_renderer: TerminalViewRenderer::default_monospace(),
            welcome_logo_glyph_instances: Vec::new(),
            welcome_logo_chrome_instances: Vec::new(),
            welcome_logo_scissor: None,
            topbar_text_system,
            topbar_text_pipeline,
            topbar_glyph_instances: Vec::new(),
            topbar_chrome_instances: Vec::new(),
            topbar_scissor: None,
            topbar_text_batches: Vec::new(),
            last_topbar_layout_key: None,
            topbar_logo_image_pipeline,
            topbar_logo_scissor: None,
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
            diagnostic_hover_text_system,
            diagnostic_hover_text_pipeline,
            diagnostic_hover_glyph_instances: Vec::new(),
            diagnostic_hover_chrome_instances: Vec::new(),
            diagnostic_hover_scissor: None,
            editor_padding_x: 14.0,
            editor_padding_y: 14.0,
            panel_padding: 10.0,
            panel_corner_radius: 10.0,
            round_ui: true,
            sidebar_base_padding: 10.0,
            sidebar_indent_per_depth: 15.0,
            topbar_padding_x: 14.0,
            topbar_dirty_gap: 6.0,
            statusbar_padding_x: 14.0,
            statusbar_font_size,
            statusbar_line_height,
            welcome_version: crate::APP_VERSION.to_string(),
            welcome_card_max_width: 560.0,
            welcome_card_padding_x: 42.0,
            welcome_card_padding_y: 34.0,
            welcome_section_gap: 16.0,
            welcome_border_radius_px: 18.0,
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
            system_dep_text_system,
            system_dep_text_pipeline,
            system_dep_chrome_instances: Vec::new(),
            system_dep_glyph_instances: Vec::new(),
            system_dep_scissor: None,
            toast_text_system,
            toast_text_pipeline,
            toast_glyph_instances: Vec::new(),
            toast_chrome_instances: Vec::new(),
            toast_scissor: None,
            ai_chat_text_system,
            ai_chat_text_pipeline,
            ai_chat_header_image_pipeline,
            ai_chat_hero_image_pipeline,
            ai_chat_glyph_instances: Vec::new(),
            ai_chat_history_chrome_instances: Vec::new(),
            ai_chat_suggestion_chrome_instances: Vec::new(),
            ai_chat_suggestion_glyph_start: None,
            ai_chat_history_scissor: None,
            ai_chat_image_scissor: None,
            ai_chat_input_scissor: None,
            ai_chat_input_batch: None,
            last_shaped_revision: u64::MAX,
            last_shaped_spans_fingerprint: u64::MAX,
            last_shaped_viewport_width: 0.0,
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
        self.system_dep_text_system.set_metrics(Metrics::new(
            theme.ui.panel_font_size,
            theme.ui.panel_line_height,
        ));
        self.diagnostic_hover_text_system.set_metrics(Metrics::new(
            theme.editor.font_size,
            theme.editor.line_height,
        ));
        self.toast_text_system.set_metrics(ui_metrics);
        self.ai_chat_text_system.set_metrics(ui_metrics);

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
        self.system_dep_text_system.set_font_family(family);
        self.diagnostic_hover_text_system.set_font_family(family);
        self.toast_text_system.set_font_family(family);
        self.leap_label_text_system.set_font_family(family);
        self.ai_chat_text_system.set_font_family(family);

        self.clear_color = theme_color_to_wgpu(theme.ui.bg);
        self.theme = theme;
        self.topbar_scissor = None;
        self.topbar_glyph_instances.clear();
        self.topbar_chrome_instances.clear();
        self.topbar_text_batches.clear();
        self.topbar_logo_image_pipeline.clear();
        self.topbar_logo_scissor = None;
        self.topbar_text_pipeline
            .upload_instances(&self.device, &self.queue, &[]);
        self.statusbar_scissor = None;
        self.statusbar_glyph_instances.clear();
        self.statusbar_chrome_instances.clear();
        self.buffer_terminal_header_batch = None;
        self.statusbar_text_pipeline
            .upload_instances(&self.device, &self.queue, &[]);
        self.clear_sidebar();
        self.clear_palette();
        self.clear_editor_overlays();
        self.clear_diagnostic_hover_popup();
        self.clear_leap_labels();
        self.clear_ai_chat();
        self.last_topbar_layout_key = None;
        self.last_statusbar_layout_key = None;
        self.last_palette_model = None;
    }

    pub fn apply_ui_config(&mut self, ui: &UiConfig) {
        self.editor_padding_x = ui.layout.inner_padding.max(ui.spacing.editor_padding);
        self.editor_padding_y = ui.layout.inner_padding.max(ui.spacing.editor_padding);
        self.panel_padding = ui.layout.inner_padding.max(ui.spacing.panel_padding);
        self.panel_corner_radius = if ui.layout.round_ui {
            ui.border_radius_px.max(0.0)
        } else {
            0.0
        };
        self.round_ui = ui.layout.round_ui;
        self.sidebar_base_padding = ui.layout.inner_padding.max(ui.spacing.explorer_padding);
        self.sidebar_indent_per_depth = (self.sidebar_base_padding * 1.5).max(10.0);
        self.topbar_padding_x = ui.status_bar.padding_x;
        self.topbar_dirty_gap = ui.spacing.topbar_dirty_gap;
        self.statusbar_padding_x = ui.status_bar.padding_x;
        self.statusbar_font_size = ui.status_bar.font_size;
        self.statusbar_line_height = ui.status_bar.line_height;
        self.welcome_version = ui.welcome.version.clone();
        self.welcome_card_max_width = ui.welcome.card_max_width;
        self.welcome_card_padding_x = ui.welcome.card_padding_x;
        self.welcome_card_padding_y = ui.welcome.card_padding_y;
        self.welcome_section_gap = ui.welcome.section_gap;
        self.welcome_border_radius_px = ui.welcome.border_radius_px;
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
            &mut self.system_dep_text_pipeline,
            &mut self.diagnostic_hover_text_pipeline,
            &mut self.toast_text_pipeline,
            &mut self.leap_label_text_pipeline,
            &mut self.ai_chat_text_pipeline,
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
}
