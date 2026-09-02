//! Core renderer types and module layout.
//!
//! Heavy per-region rendering logic lives in child modules:
//! - [`editor`]         — editor content, caret, gutter, visual selection
//! - [`ui`]             — sidebar, terminal, welcome logo, topbar, statusbar
//! - [`palette`]        — command palette, file picker, leap label overlay
//! - [`lifecycle`]      — GPU init, theme/config application, resize, render loop
//! - [`helpers`]        — pure free functions shared by all render modules

mod canvas;
mod components;
mod editor;
mod helpers;
mod lifecycle;
mod palette;
mod ui;
pub(crate) use ui::test_runner::TestRunnerPointerAction;
// Re-exported so the app layer can build per-tab status values for the
// bottom-dock terminal tab strip (`build_bottom_tab_strip`) without reaching
// into this private module tree.
pub(crate) use ui::TerminalTabDot;

use std::path::PathBuf;

use crate::{
    app::command_palette::CommandPaletteRenderModel,
    config::{theme_config::ThemeConfig, ui_config::CursorShape},
    core::mode::EditorMode,
    render::{
        caret::CaretPipeline,
        glyph_instance::GlyphInstance,
        icon_pipeline::{IconDrawInstance, IconPipeline},
        image_pipeline::ImagePipeline,
        region_pipeline::{RegionDrawInstance, RegionPipeline},
        surface::SurfaceState,
        text_pipeline::{InstanceDrawRange, TextPipeline},
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
    pub path: Option<PathBuf>,
    pub depth: usize,
    /// Disclosure arrow ▶/▼/· — always rendered in fg_ghost.
    pub arrow: String,
    /// NerdFont icon char (folder/filetype icon).
    pub nerd_icon: String,
    /// RGBA color for `nerd_icon` — per-filetype.
    pub icon_color: [f32; 4],
    pub label: String,
    pub prefix_marker: Option<String>,
    pub prefix_color: Option<[f32; 4]>,
    pub git_marker: Option<char>,
    pub git_color: Option<[f32; 4]>,
    pub is_selected: bool,
    /// Hidden/ignored entry — label rendered in the dim `fg_ghost` color.
    pub is_dim: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SidebarFilterState {
    pub query: String,
    pub is_inputting: bool,
    pub show_cursor: bool,
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
pub enum TopbarTabKind {
    Text { path: PathBuf },
    Image { path: PathBuf },
    Terminal,
    References,
    Diagnostics,
    MarkdownPreview,
    FuzzyPicker,
    Settings,
    Help,
    ExtensionsManager,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TopbarTab {
    /// Stable identity of the underlying buffer at the time this tab was drawn.
    /// Mouse hit-testing validates it before dispatching a cached index.
    pub identity: u64,
    pub label: String,
    pub kind: TopbarTabKind,
    pub is_dirty: bool,
    pub git_color: Option<[f32; 4]>,
    pub missing_on_disk: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub(super) struct TopbarLayoutKey {
    pub(super) tabs: Vec<TopbarTab>,
    pub(super) active_buffer_index: Option<usize>,
    /// Index of the tab currently under the pointer. Part of the key so the
    /// layout cache invalidates whenever hover moves between tabs.
    pub(super) hovered_tab_index: Option<usize>,
    pub(super) center_x: f32,
    /// Workspace folder name + git branch shown in the topbar's left segment.
    /// Part of the key so the cache invalidates when they change.
    pub(super) project_label: String,
    pub(super) bounds: [f32; 4],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct TextScissorBatch {
    pub(super) scissor: [u32; 4],
    pub(super) range: InstanceDrawRange,
}

/// Status-bar LSP indicator for the active file's language server.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LspStatusIndicator {
    /// Active file has no language profile — hide the indicator entirely.
    NotApplicable,
    /// A profile exists but no server is running.
    Inactive,
    /// Server process is being spawned/initialized.
    Starting(String),
    /// Server is alive and serving this workspace.
    Running(String),
    /// Server binary is missing — install guide shown.
    Missing(String),
}

/// Status-bar chip state for AI inline completion. None (not passed) hides the chip.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AiInlineStatus {
    /// Provider configured and idle.
    Ready,
    /// A completion request is in flight.
    Loading,
    /// Circuit breaker tripped — completions paused after repeated failures.
    Error,
}

#[derive(Debug, Clone, PartialEq)]
pub(super) struct StatusbarLayoutKey {
    pub(super) mode: EditorMode,
    /// NetherCanvas status pill label when the canvas owns/overlays focus
    /// (`CANVAS` / `CANVAS·EDIT`); `None` shows the normal editor-mode pill.
    pub(super) canvas_label: Option<String>,
    pub(super) pending_keys: String,
    pub(super) git_branch: String,
    /// (added, modified) file counts rendered after the branch label.
    pub(super) git_changes: (usize, usize),
    pub(super) is_dirty: bool,
    pub(super) filetype: String,
    pub(super) search_match_position: Option<(usize, usize)>,
    pub(super) line: usize,
    pub(super) col: usize,
    pub(super) lsp_loading: bool,
    pub(super) lsp_loading_frame: u8,
    pub(super) lsp_progress: Option<String>,
    pub(super) lsp_indicator: LspStatusIndicator,
    pub(super) ai_status: Option<AiInlineStatus>,
    pub(super) bounds: [f32; 4],
    pub(super) venv_name: Option<String>,
    pub(super) python_version: Option<String>,
    pub(super) node_version: Option<String>,
    pub(super) go_version: Option<String>,
    /// Dojo session clock chip: (label, color code) — see `update_statusbar_content`.
    pub(super) dojo_chip: Option<(String, u8)>,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct EditorBreadcrumbSegment {
    pub(crate) text: String,
    pub(crate) color: [f32; 4],
    pub(crate) icon_id: Option<&'static str>,
}

/// Cached minimap geometry so the scroll-dependent viewport indicator can be
/// recomputed on the caret fast path (gg/G jump through it, changing scroll_y
/// without a full re-render). The static bars live in `minimap_instances`.
#[derive(Clone, Copy, Default)]
pub(crate) struct MinimapLayout {
    pub(crate) active: bool,
    pub(crate) strip_x: f32,
    pub(crate) strip_w: f32,
    pub(crate) strip_y: f32,
    pub(crate) content_h: f32,
    pub(crate) step: f32,
    pub(crate) ind_h: f32,
    pub(crate) line_count: f32,
    pub(crate) accent: [f32; 4],
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
    /// Scissor for the FULL center editor pane — `editor_scissor` clips to the
    /// text area BELOW the breadcrumb, but the NetherCanvas layer floats over the
    /// whole pane (incl. the breadcrumb row), so cards panned upward must not be
    /// clipped under the breadcrumb. Set alongside `editor_scissor`.
    pub(super) editor_full_scissor: Option<[u32; 4]>,
    /// Screen-space rect `[x, y, w, h]` of the primary editor caret, captured
    /// each editor render. The NetherCanvas overlay uses it to anchor relation
    /// connectors at the real cursor line (the editor is the focal anchor).
    pub(super) editor_caret_screen: Option<[f32; 4]>,
    /// Screen-space rect of the canvas FOCAL symbol's line (where `gc` was
    /// triggered), captured each editor render while a canvas is active and the
    /// editor shows the focal file. Connectors anchor here instead of the live
    /// caret, so coding in the editor doesn't drag them around. `None` clears it.
    pub(super) editor_focal_screen: Option<[f32; 4]>,
    pub(super) editor_overlay_text_system: TextSystem,
    pub(super) editor_overlay_text_pipeline: TextPipeline,
    pub(super) editor_overlay_glyph_instances: Vec<GlyphInstance>,
    pub(super) editor_overlay_icon_pipeline: IconPipeline,
    pub(super) editor_overlay_icon_instances: Vec<IconDrawInstance>,
    pub(super) editor_overlay_chrome_instances: Vec<RegionDrawInstance>,
    pub(super) editor_overlay_scissor: Option<[u32; 4]>,
    pub(super) temp_string_buffer: String,
    pub(super) temp_string_buffer_alt: String,
    pub(super) image_pipeline: ImagePipeline,
    pub(super) image_scissor: Option<[u32; 4]>,
    pub(super) welcome_image_pipeline: ImagePipeline,
    pub(super) welcome_image_scissor: Option<[u32; 4]>,
    pub(super) welcome_icon_pipeline: IconPipeline,
    pub(super) welcome_icon_instances: Vec<IconDrawInstance>,

    // ── Gutter (line numbers) ─────────────────────────────────────────────────
    pub(super) gutter_text_system: TextSystem,
    pub(super) gutter_text_pipeline: TextPipeline,
    pub(super) gutter_glyph_instances: Vec<GlyphInstance>,
    pub relative_numbers: bool,
    pub(super) last_editor_chrome_instances: Vec<RegionDrawInstance>,
    /// Cached minimap quads, rebuilt on full re-render and re-appended on the
    /// caret fast path so the minimap doesn't flicker on cursor moves.
    pub(super) minimap_instances: Vec<RegionDrawInstance>,
    /// Cached minimap geometry so the scroll indicator tracks gg/G jumps that
    /// go through the caret fast path (no full re-render).
    pub(super) minimap_layout: MinimapLayout,
    pub(super) editor_breadcrumb_segments: Vec<EditorBreadcrumbSegment>,

    // ── Explorer sidebar ──────────────────────────────────────────────────────
    pub(super) sidebar_text_system: TextSystem,
    pub(super) sidebar_text_pipeline: TextPipeline,
    pub(super) sidebar_glyph_instances: Vec<GlyphInstance>,
    pub(super) sidebar_icon_pipeline: IconPipeline,
    pub(super) sidebar_icon_instances: Vec<IconDrawInstance>,
    pub(super) sidebar_scissor: Option<[u32; 4]>,

    // ── Terminal panel (bottom dock) ──────────────────────────────────────────
    pub(super) terminal_text_system: TextSystem,
    pub(super) terminal_text_pipeline: TextPipeline,
    pub(super) terminal_view_renderer: TerminalViewRenderer,
    pub(super) terminal_glyph_instances: Vec<GlyphInstance>,
    pub(super) terminal_cell_background_instances: Vec<RegionDrawInstance>,
    pub(super) terminal_cursor_instances: Vec<RegionDrawInstance>,
    pub(super) terminal_scissor: Option<[u32; 4]>,
    pub(super) terminal_body_batch: Option<TextScissorBatch>,
    pub(super) terminal_outer_tab_batch: Option<TextScissorBatch>,

    // ── Bottom dock outer tab strip icons ──────────────────────────────────────
    /// Tab-strip icons built by `build_bottom_tab_strip`. `frame.rs` appends them
    /// to the sidebar icon buffer and draws them with the outer-strip scissor.
    pub(super) bottom_dock_tab_icon_instances: Vec<IconDrawInstance>,

    // ── Right-dock terminal (opencode) ────────────────────────────────────────
    pub(super) right_terminal_text_system: TextSystem,
    pub(super) right_terminal_text_pipeline: TextPipeline,
    pub(super) right_terminal_view_renderer: TerminalViewRenderer,
    pub(super) right_terminal_glyph_instances: Vec<GlyphInstance>,
    pub(super) right_terminal_cell_background_instances: Vec<RegionDrawInstance>,
    pub(super) right_terminal_cursor_instances: Vec<RegionDrawInstance>,
    pub(super) right_terminal_scissor: Option<[u32; 4]>,
    pub(super) right_terminal_body_batch: Option<TextScissorBatch>,

    // ── Full-screen terminal buffer tabs ─────────────────────────────────────
    pub(super) buffer_terminal_text_system: TextSystem,
    pub(super) buffer_terminal_text_pipeline: TextPipeline,
    pub(super) buffer_terminal_view_renderer: TerminalViewRenderer,
    pub(super) buffer_terminal_glyph_instances: Vec<GlyphInstance>,
    pub(super) buffer_terminal_cell_background_instances: Vec<RegionDrawInstance>,
    pub(super) buffer_terminal_cursor_instances: Vec<RegionDrawInstance>,
    pub(super) buffer_terminal_scissor: Option<[u32; 4]>,

    // ── Welcome ANSI logo overlay ─────────────────────────────────────────────
    pub(super) welcome_logo_text_system: TextSystem,
    pub(super) welcome_logo_text_pipeline: TextPipeline,
    pub(super) welcome_logo_view_renderer: TerminalViewRenderer,
    pub(super) welcome_logo_glyph_instances: Vec<GlyphInstance>,
    pub(super) welcome_logo_chrome_instances: Vec<RegionDrawInstance>,
    pub(super) welcome_logo_scissor: Option<[u32; 4]>,

    // ── TopBar ────────────────────────────────────────────────────────────────
    pub(super) topbar_text_system: TextSystem,
    pub(super) topbar_text_pipeline: TextPipeline,
    pub(super) topbar_glyph_instances: Vec<GlyphInstance>,
    pub(super) topbar_icon_pipeline: IconPipeline,
    pub(super) topbar_icon_instances: Vec<IconDrawInstance>,
    pub(super) topbar_chrome_instances: Vec<RegionDrawInstance>,
    pub(super) topbar_scissor: Option<[u32; 4]>,
    pub(super) topbar_text_batches: Vec<TextScissorBatch>,
    pub(super) topbar_tab_hitboxes: Vec<(usize, u64, [f32; 4])>,
    /// Hitboxes for the per-tab close ("×") buttons: `(buffer index, identity,
    /// rect)`. Kept separate from `topbar_tab_hitboxes` so a press on the
    /// button never also activates the tab body underneath it.
    pub(super) topbar_close_hitboxes: Vec<(usize, u64, [f32; 4])>,
    /// Clickable region of the project/branch label in the topbar's left
    /// segment. `None` when there is no workspace or the segment is too narrow.
    pub(super) topbar_project_hitbox: Option<[f32; 4]>,
    pub(super) last_topbar_layout_key: Option<TopbarLayoutKey>,
    pub(super) topbar_logo_image_pipeline: ImagePipeline,
    pub(super) topbar_logo_scissor: Option<[u32; 4]>,

    // ── StatusBar ─────────────────────────────────────────────────────────────
    pub(super) statusbar_text_system: TextSystem,
    pub(super) statusbar_text_pipeline: TextPipeline,
    pub(super) statusbar_glyph_instances: Vec<GlyphInstance>,
    pub(super) statusbar_chrome_instances: Vec<RegionDrawInstance>,
    pub(super) statusbar_scissor: Option<[u32; 4]>,
    pub(super) buffer_terminal_header_batch: Option<TextScissorBatch>,
    pub(super) last_statusbar_layout_key: Option<StatusbarLayoutKey>,

    // ── Command Palette / File Picker overlay ─────────────────────────────────
    pub(super) palette_text_system: TextSystem,
    pub(super) palette_text_pipeline: TextPipeline,
    pub(super) palette_glyph_instances: Vec<GlyphInstance>,
    pub(super) palette_chrome_instances: Vec<RegionDrawInstance>,
    pub(super) palette_icon_pipeline: IconPipeline,
    pub(super) palette_icon_instances: Vec<IconDrawInstance>,
    pub(super) palette_scissor: Option<[u32; 4]>,
    pub(super) last_palette_model: Option<CommandPaletteRenderModel>,
    /// Overlay enter/leave motion last applied to the palette instances. Part of
    /// the memo key so an animating sample re-uploads each frame even when the
    /// model is unchanged.
    pub(super) last_palette_sample: Option<crate::workbench::motion::RevealSample>,

    // ── NetherCanvas (Spatial Canvas) full-screen layer ───────────────────────
    pub(super) canvas_text_system: TextSystem,
    pub(super) canvas_text_pipeline: TextPipeline,
    pub(super) canvas_glyph_instances: Vec<GlyphInstance>,
    pub(super) canvas_chrome_instances: Vec<RegionDrawInstance>,
    /// Optional accent-colored quad drawn on top of everything else to
    /// highlight a hovered drag handle (dock splitter / card resize edge).
    /// Set from the event loop via [`Renderer::set_pointer_hover_highlight`].
    pub(super) pointer_hover_highlight: Option<RegionDrawInstance>,
    pub(super) canvas_icon_pipeline: IconPipeline,
    pub(super) canvas_icon_instances: Vec<IconDrawInstance>,
    pub(super) canvas_scissor: Option<[u32; 4]>,

    // ── Window overlays ──────────────────────────────────────────────────────
    pub(super) lsp_guide_text_system: TextSystem,
    pub(super) lsp_guide_text_pipeline: TextPipeline,
    pub(super) lsp_guide_scissor: Option<[u32; 4]>,
    pub(super) lsp_guide_chrome_instances: Vec<RegionDrawInstance>,
    pub(super) lsp_guide_glyph_instances: Vec<GlyphInstance>,
    pub(super) system_dep_text_system: TextSystem,
    pub(super) system_dep_text_pipeline: TextPipeline,
    pub(super) system_dep_scissor: Option<[u32; 4]>,
    pub(super) system_dep_chrome_instances: Vec<RegionDrawInstance>,
    pub(super) system_dep_glyph_instances: Vec<GlyphInstance>,
    pub(super) diagnostic_hover_text_system: TextSystem,
    pub(super) diagnostic_hover_text_pipeline: TextPipeline,
    pub(super) diagnostic_hover_glyph_instances: Vec<GlyphInstance>,
    pub(super) diagnostic_hover_chrome_instances: Vec<RegionDrawInstance>,
    pub(super) diagnostic_hover_scissor: Option<[u32; 4]>,

    // ── UI config knobs ───────────────────────────────────────────────────────
    pub(super) editor_padding_x: f32,
    pub(super) editor_padding_y: f32,
    pub(super) panel_padding: f32,
    pub(super) panel_corner_radius: f32,
    pub(super) round_ui: bool,
    /// Effective runtime UI scale (dpi_scale × content_scale). Single source for
    /// scaling hardcoded px chrome (paddings, radii) so they track text size.
    pub(super) ui_scale: f32,
    pub(super) sidebar_base_padding: f32,
    pub(super) sidebar_indent_per_depth: f32,
    pub(super) topbar_padding_x: f32,
    pub(super) topbar_dirty_gap: f32,
    pub(super) statusbar_padding_x: f32,
    pub(super) statusbar_font_size: f32,
    pub(super) statusbar_line_height: f32,
    pub(super) welcome_version: String,
    pub(super) welcome_card_max_width: f32,
    pub(super) welcome_card_padding_x: f32,
    pub(super) welcome_card_padding_y: f32,
    pub(super) welcome_section_gap: f32,
    pub(super) welcome_border_radius_px: f32,
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

    // ── Transient toast popup ────────────────────────────────────────────────
    pub(super) toast_text_system: TextSystem,
    pub(super) toast_text_pipeline: TextPipeline,
    pub(super) toast_glyph_instances: Vec<GlyphInstance>,
    pub(super) toast_chrome_instances: Vec<RegionDrawInstance>,
    pub(super) toast_scissor: Option<[u32; 4]>,

    // ── Which-key overlay (pending chord hints) ──────────────────────────────
    pub(super) whichkey_text_system: TextSystem,
    pub(super) whichkey_text_pipeline: TextPipeline,
    pub(super) whichkey_glyph_instances: Vec<GlyphInstance>,
    pub(super) whichkey_chrome_instances: Vec<RegionDrawInstance>,
    pub(super) whichkey_scissor: Option<[u32; 4]>,

    // ── Test Runner (BottomPanel) ──────────────────────────────────────────
    pub(super) test_runner_text_system: TextSystem,
    pub(super) test_runner_text_pipeline: TextPipeline,
    pub(super) test_runner_glyph_instances: Vec<GlyphInstance>,
    pub(super) test_runner_icon_pipeline: IconPipeline,
    pub(super) test_runner_icon_instances: Vec<IconDrawInstance>,
    pub(super) test_runner_chrome_instances: Vec<RegionDrawInstance>,
    pub(super) test_runner_scissor: Option<[u32; 4]>,

    // ── Markdown Preview (RightSidebar) ─────────────────────────────────────────────
    pub(super) markdown_preview_text_system: TextSystem,
    pub(super) markdown_preview_text_pipeline: TextPipeline,
    pub(super) markdown_preview_header_image_pipeline: ImagePipeline,
    pub(super) markdown_preview_hero_image_pipeline: ImagePipeline,
    pub(super) markdown_preview_glyph_instances: Vec<GlyphInstance>,
    pub(super) markdown_preview_chrome_instances: Vec<RegionDrawInstance>,
    /// Background rects for the slash-command suggestion popup, rendered as a
    /// separate pass *after* message bubble text so they appear on top.
    pub(super) markdown_preview_overlay_chrome_instances: Vec<RegionDrawInstance>,
    /// Index into `markdown_preview_glyph_instances` where suggestion-popup glyphs begin.
    pub(super) markdown_preview_overlay_glyph_start: Option<u32>,
    pub(super) markdown_preview_scissor: Option<[u32; 4]>,
    pub(super) markdown_preview_image_scissor: Option<[u32; 4]>,
    pub(super) markdown_preview_input_scissor: Option<[u32; 4]>,
    /// Instance range for input-box glyphs inside `markdown_preview_glyph_instances`.
    pub(super) markdown_preview_input_batch: Option<TextScissorBatch>,

    // ── Tối ưu 2: Text Caching ────────────────────────────────────────────
    /// Revision của text content lần cuối được shaped bởi cosmic-text.
    /// Khi `app_state.revision()` khớp, bỏ qua `set_text_with_spans`.
    pub(super) last_shaped_revision: u64,
    /// Fingerprint nhanh của spans để phát hiện thay đổi highlight/diagnostics.
    pub(super) last_shaped_spans_fingerprint: u64,
    /// Viewport width lần cuối reshape — phát hiện khi word-wrap boundary thay đổi.
    pub(super) last_shaped_viewport_width: f32,
    pub(super) caret_blink_visible: bool,
    /// Present only when NETH_PERF_PROBE=1 (GPU timestamp instrumentation).
    pub(super) gpu_timing: Option<crate::render::gpu_timing::GpuTiming>,
}

impl Renderer {
    /// Wall-GPU duration of the most recent measured render pass, if timing
    /// was enabled at startup.
    pub fn latest_gpu_pass_ms(&self) -> Option<f64> {
        self.gpu_timing.as_ref().and_then(|g| g.latest_pass_ms())
    }

    pub fn soft_wrap_visual_move_target(
        &self,
        app_state: &crate::app::app_state::AppState,
        center_bounds: [f32; 4],
        down: bool,
    ) -> Option<usize> {
        crate::render::renderer::editor::soft_wrap_visual_move_target(
            self,
            app_state,
            center_bounds,
            down,
        )
    }

    pub fn editor_chrome_instances(&self) -> &[RegionDrawInstance] {
        &self.last_editor_chrome_instances
    }

    /// Set (or clear) the pointer-hover highlight quad — a thin accent-colored
    /// band drawn over a hovered drag handle. `None` clears it.
    pub fn set_pointer_hover_highlight(&mut self, quad: Option<RegionDrawInstance>) {
        self.pointer_hover_highlight = quad;
    }

    /// Tối ưu 3: Caret Blink — chỉ flip visibility của caret pipeline mà không
    /// trigger bất kỳ text layout hay glyph rebuild nào.
    pub fn update_caret_visibility(&mut self, visible: bool) {
        self.caret_pipeline.set_caret_visible(&self.queue, visible);
        self.caret_blink_visible = visible;
    }
}
