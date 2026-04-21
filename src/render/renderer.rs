//! Core renderer types and module layout.
//!
//! Heavy per-region rendering logic lives in child modules:
//! - [`editor_render`]  — editor content, caret, gutter, visual selection
//! - [`ui_render`]      — sidebar, terminal, welcome logo, topbar, statusbar
//! - [`palette_render`] — command palette, file picker, leap label overlay
//! - [`lifecycle`]      — GPU init, theme/config application, resize, render loop
//! - [`helpers`]        — pure free functions shared by all render modules

mod editor_render;
mod helpers;
mod lifecycle;
mod palette_render;
mod ui_render;

use std::path::PathBuf;

use crate::{
    app::command_palette::CommandPaletteRenderModel,
    config::{theme_config::ThemeConfig, ui_config::CursorShape},
    core::mode::EditorMode,
    render::{
        caret::CaretPipeline,
        glyph_instance::GlyphInstance,
        region_pipeline::{RegionDrawInstance, RegionPipeline},
        surface::SurfaceState,
        text_pipeline::TextPipeline,
    },
    terminal::terminal_renderer::TerminalViewRenderer,
    text::{atlas::GlyphAtlas, text_system::TextSystem},
};

// ── Public re-exports ──────────────────────────────────────────────────────────

const ATLAS_SIZE: u32 = 2048;

/// One visible row of the Explorer tree as consumed by the renderer.
/// Depth drives x-indentation; `icon` is the disclosure glyph (▶/▼/·).
#[derive(Debug, Clone)]
pub struct SidebarRow {
    pub depth: usize,
    /// Disclosure arrow ▶/▼/· — always rendered in fg_ghost.
    pub arrow: String,
    /// NerdFont icon char (folder/filetype icon).
    pub nerd_icon: String,
    /// RGBA color for `nerd_icon` — per-filetype.
    pub icon_color: [f32; 4],
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

// ── Layout cache keys (used for dirty-check caching) ──────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub struct TopbarTab {
    pub label: String,
    pub path: PathBuf,
}

#[derive(Debug, Clone, PartialEq)]
pub(super) struct TopbarLayoutKey {
    pub(super) tabs: Vec<TopbarTab>,
    pub(super) active_buffer_index: Option<usize>,
    pub(super) bounds: [f32; 4],
}

#[derive(Debug, Clone, PartialEq)]
pub(super) struct StatusbarLayoutKey {
    pub(super) mode: EditorMode,
    pub(super) pending_keys: String,
    pub(super) git_branch: String,
    pub(super) filetype: String,
    pub(super) line: usize,
    pub(super) col: usize,
    pub(super) bounds: [f32; 4],
}

// ── Renderer struct ────────────────────────────────────────────────────────────

pub struct Renderer {
    pub(super) device: wgpu::Device,
    pub(super) queue: wgpu::Queue,
    pub surface_state: SurfaceState,
    pub(super) region_pipeline: RegionPipeline,
    pub(super) theme: ThemeConfig,
    clear_color: wgpu::Color,
    pub(super) atlas: GlyphAtlas,

    // ── Editor ────────────────────────────────────────────────────────────────
    pub(super) text_system: TextSystem,
    pub(super) text_pipeline: TextPipeline,
    pub(super) glyph_instances: Vec<GlyphInstance>,
    pub(super) caret_pipeline: CaretPipeline,
    /// Redraws the glyph under the block cursor with a contrast color.
    pub(super) editor_cursor_overlay_pipeline: TextPipeline,
    pub(super) editor_scissor: Option<[u32; 4]>,

    // ── Gutter (line numbers) ─────────────────────────────────────────────────
    pub(super) gutter_text_system: TextSystem,
    pub(super) gutter_text_pipeline: TextPipeline,
    pub(super) gutter_glyph_instances: Vec<GlyphInstance>,
    pub relative_numbers: bool,

    // ── Explorer sidebar ──────────────────────────────────────────────────────
    pub(super) sidebar_text_system: TextSystem,
    pub(super) sidebar_text_pipeline: TextPipeline,
    pub(super) sidebar_glyph_instances: Vec<GlyphInstance>,
    pub(super) sidebar_scissor: Option<[u32; 4]>,

    // ── Terminal panel ────────────────────────────────────────────────────────
    pub(super) terminal_text_system: TextSystem,
    pub(super) terminal_text_pipeline: TextPipeline,
    pub(super) terminal_view_renderer: TerminalViewRenderer,
    pub(super) terminal_glyph_instances: Vec<GlyphInstance>,
    pub(super) terminal_cursor_instances: Vec<RegionDrawInstance>,
    pub(super) terminal_scissor: Option<[u32; 4]>,

    // ── Welcome ANSI logo overlay ─────────────────────────────────────────────
    pub(super) welcome_logo_text_system: TextSystem,
    pub(super) welcome_logo_text_pipeline: TextPipeline,
    pub(super) welcome_logo_view_renderer: TerminalViewRenderer,
    pub(super) welcome_logo_glyph_instances: Vec<GlyphInstance>,
    pub(super) welcome_logo_scissor: Option<[u32; 4]>,

    // ── TopBar ────────────────────────────────────────────────────────────────
    pub(super) topbar_text_system: TextSystem,
    pub(super) topbar_text_pipeline: TextPipeline,
    pub(super) topbar_glyph_instances: Vec<GlyphInstance>,
    pub(super) topbar_chrome_instances: Vec<RegionDrawInstance>,
    pub(super) topbar_scissor: Option<[u32; 4]>,
    pub(super) last_topbar_layout_key: Option<TopbarLayoutKey>,

    // ── StatusBar ─────────────────────────────────────────────────────────────
    pub(super) statusbar_text_system: TextSystem,
    pub(super) statusbar_text_pipeline: TextPipeline,
    pub(super) statusbar_glyph_instances: Vec<GlyphInstance>,
    pub(super) statusbar_chrome_instances: Vec<RegionDrawInstance>,
    pub(super) statusbar_scissor: Option<[u32; 4]>,
    pub(super) last_statusbar_layout_key: Option<StatusbarLayoutKey>,

    // ── Command Palette / File Picker overlay ─────────────────────────────────
    pub(super) palette_text_system: TextSystem,
    pub(super) palette_text_pipeline: TextPipeline,
    pub(super) palette_glyph_instances: Vec<GlyphInstance>,
    pub(super) palette_chrome_instances: Vec<RegionDrawInstance>,
    pub(super) palette_scissor: Option<[u32; 4]>,
    pub(super) last_palette_model: Option<CommandPaletteRenderModel>,

    // ── UI config knobs ───────────────────────────────────────────────────────
    pub(super) editor_padding_x: f32,
    pub(super) editor_padding_y: f32,
    pub(super) panel_padding: f32,
    pub(super) sidebar_base_padding: f32,
    pub(super) sidebar_indent_per_depth: f32,
    pub(super) topbar_padding_x: f32,
    pub(super) statusbar_padding_x: f32,
    pub(super) statusbar_font_size: f32,
    pub(super) statusbar_line_height: f32,
    pub(super) cursor_shape: CursorShape,
    pub(super) cursor_beam_width: f32,
    pub(super) cursor_block_width: f32,
    pub(super) cursor_underline_height: f32,

    // ── Leap label overlay ────────────────────────────────────────────────────
    pub(super) leap_label_text_system: TextSystem,
    pub(super) leap_label_text_pipeline: TextPipeline,
    pub(super) leap_label_glyph_instances: Vec<GlyphInstance>,
    pub(super) leap_label_bg_instances: Vec<RegionDrawInstance>,
    pub(super) leap_label_scissor: Option<[u32; 4]>,
}
