use crate::{
    core::mode::EditorMode,
    render::{
        icon_pipeline::{IconDrawInstance, canonical_icon_id},
        region_pipeline::RegionDrawInstance,
        renderer::{Renderer, TextScissorBatch},
        text_pipeline::InstanceDrawRange,
    },
    terminal::grid::TerminalGrid,
};

use super::super::helpers::{
    clamp_monospace_text_left, estimate_monospace_width, layout_panel_text, rect_to_scissor,
};

const EMPTY_TERMINAL_HINT: &str = "(terminal ready — press F12 to focus)";
const TERMINAL_SAFE_INSET_X: f32 = 2.0;

/// Logical-px ceiling for one tab on the live bottom-dock strip (call sites
/// scale it by `ui_scale`, like every other runtime-scaled constant here).
/// Without a cap two or three terminals would split the whole dock width and
/// grow absurdly wide.
const BOTTOM_DOCK_TAB_MAX_WIDTH: f32 = 240.0;

/// Effective width of one tab on the live bottom-dock strip.
///
/// This is the single source of truth for that strip's tab geometry: BOTH the
/// renderer (`build_bottom_tab_strip`) and the hit-test
/// (`bottom_dock_tab_index_at`) must go through it, or rendered tabs and
/// clickable tabs drift apart — same contract as `utils::dock_tab_width` for
/// the side docks.
///
/// Tabs divide `tabs_w` equally, clamped DOWN to `max_tab_w`; when the cap
/// engages the tabs are left-aligned and the space between the last tab and
/// the "+" button stays empty (the hit-test reports no target there).
/// Per-tab status shown as a small dot on the live bottom-dock strip.
///
/// The app layer owns the real status type (`event_loop::TerminalTabStatus`),
/// which is `pub(super)` and cannot be imported by the renderer module — so
/// this is a deliberately tiny mirror: the render call site maps one onto the
/// other, keeping the renderer free of app-layer types.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TerminalTabDot {
    /// PTY session alive.
    Running,
    /// PTY exited cleanly.
    ExitedOk,
    /// PTY exited with a nonzero code.
    ExitedFail,
}

pub(crate) fn bottom_dock_tab_width(tabs_w: f32, term_count: usize, max_tab_w: f32) -> Option<f32> {
    if term_count == 0 || tabs_w <= 0.0 {
        return None;
    }
    Some((tabs_w / term_count as f32).min(max_tab_w.max(1.0)))
}

/// Which tab sits at pointer x `px` on the live bottom-dock strip.
///
/// Pure geometry shared by the hit-test (`Renderer::bottom_dock_tab_index_at`)
/// so rendered and clickable rects can never drift. Tabs are LEFT-aligned
/// once the width cap engages, so the gap between the last tab's right edge
/// and the "+" button reports NO target (`None`) instead of silently
/// mapping to the last tab.
pub(crate) fn bottom_dock_tab_hit_index(
    px: f32,
    strip_x: f32,
    tab_w: f32,
    term_count: usize,
) -> Option<usize> {
    if term_count == 0 || tab_w <= 0.0 {
        return None;
    }
    let rel = px - strip_x;
    if rel < 0.0 {
        return None;
    }
    let idx = (rel / tab_w) as usize;
    // Past the last tab (the capped zone / under the "+" button) → no target.
    if idx < term_count {
        Some(idx)
    } else {
        None
    }
}

fn inset_bounds(bounds: [f32; 4], inset_x: f32, inset_y: f32) -> [f32; 4] {
    [
        bounds[0] + inset_x,
        bounds[1] + inset_y,
        (bounds[2] - inset_x * 2.0).max(0.0),
        (bounds[3] - inset_y * 2.0).max(0.0),
    ]
}

impl Renderer {
    pub fn update_terminal_content(
        &mut self,
        grid: &TerminalGrid,
        bounds: [f32; 4],
        terminal_mode: EditorMode,
    ) {
        self.caret_blink_visible = true;
        Self::render_terminal_region(
            &self.theme,
            self.panel_padding,
            self.cursor_shape,
            self.cursor_beam_width,
            self.cursor_underline_height,
            &mut self.terminal_text_system,
            &mut self.terminal_view_renderer,
            &mut self.terminal_glyph_instances,
            &mut self.terminal_cell_background_instances,
            &mut self.terminal_cursor_instances,
            &mut self.atlas,
            &self.device,
            &self.queue,
            &mut self.terminal_text_pipeline,
            &mut self.terminal_scissor,
            grid,
            bounds,
            terminal_mode,
        );
        self.terminal_body_batch = self.terminal_scissor.map(|scissor| TextScissorBatch {
            scissor,
            range: InstanceDrawRange {
                start: 0,
                count: self.terminal_glyph_instances.len() as u32,
            },
        });
    }

    /// Render the right-dock terminal (opencode) into the right sidebar area.
    /// Uses a dedicated pipeline separate from the bottom-panel terminal.
    pub fn update_right_terminal_content(
        &mut self,
        grid: &TerminalGrid,
        bounds: [f32; 4],
        terminal_mode: EditorMode,
    ) {
        self.caret_blink_visible = true;
        Self::render_terminal_region(
            &self.theme,
            self.panel_padding,
            self.cursor_shape,
            self.cursor_beam_width,
            self.cursor_underline_height,
            &mut self.right_terminal_text_system,
            &mut self.right_terminal_view_renderer,
            &mut self.right_terminal_glyph_instances,
            &mut self.right_terminal_cell_background_instances,
            &mut self.right_terminal_cursor_instances,
            &mut self.atlas,
            &self.device,
            &self.queue,
            &mut self.right_terminal_text_pipeline,
            &mut self.right_terminal_scissor,
            grid,
            bounds,
            terminal_mode,
        );
        self.right_terminal_body_batch =
            self.right_terminal_scissor.map(|scissor| TextScissorBatch {
                scissor,
                range: InstanceDrawRange {
                    start: 0,
                    count: self.right_terminal_glyph_instances.len() as u32,
                },
            });
        self.right_terminal_text_pipeline.upload_instances(
            &self.device,
            &self.queue,
            &self.right_terminal_glyph_instances,
        );
    }

    /// Clear right-dock terminal — called when the right panel is hidden.
    pub fn clear_right_terminal(&mut self) {
        self.right_terminal_scissor = None;
        self.right_terminal_body_batch = None;
        self.right_terminal_glyph_instances.clear();
        self.right_terminal_cell_background_instances.clear();
        self.right_terminal_cursor_instances.clear();
        self.right_terminal_text_pipeline
            .upload_instances(&self.device, &self.queue, &[]);
    }

    /// Render PTY grid của buffer terminal (lazygit, v.v.) vào center editor area.
    ///
    /// Trả về `RegionDrawInstance` là solid background quad màu `terminal_bg`
    /// cần được thêm vào `region_instances` trước khi render pass để che phủ hoàn toàn
    /// bất kỳ editor content nào còn sót lại.
    pub fn update_buffer_terminal_content(
        &mut self,
        grid: &TerminalGrid,
        bounds: [f32; 4],
        terminal_mode: EditorMode,
    ) -> Option<crate::render::region_pipeline::RegionDrawInstance> {
        self.caret_blink_visible = true;
        // Buffer terminal (lazygit, v.v.) chiếm toàn bộ center editor area.
        // Không có panel header → dùng editor padding thay vì panel_line_height.
        let panel_padding = self.panel_padding;
        let cursor_shape = self.cursor_shape;
        let cursor_beam_width = self.cursor_beam_width;
        let cursor_underline_height = self.cursor_underline_height;

        if bounds[2] < 1.0 || bounds[3] < 1.0 {
            self.buffer_terminal_scissor = None;
            self.buffer_terminal_header_batch = None;
            self.buffer_terminal_cell_background_instances.clear();
            self.buffer_terminal_cursor_instances.clear();
            self.buffer_terminal_text_pipeline
                .upload_instances(&self.device, &self.queue, &[]);
            return None;
        }

        let terminal_bg_color = self.theme.ui.terminal_bg.as_f32();
        // Background quad được trả về để caller thêm vào region_instances.
        let bg_quad =
            crate::render::region_pipeline::RegionDrawInstance::new(bounds, terminal_bg_color)
                .with_radius(self.panel_corner_radius);

        let origin_x = bounds[0] + panel_padding + TERMINAL_SAFE_INSET_X;
        let origin_y = bounds[1] + panel_padding;
        let width = (bounds[2] - panel_padding * 2.0 - TERMINAL_SAFE_INSET_X * 2.0).max(1.0);

        self.buffer_terminal_scissor = rect_to_scissor(inset_bounds(bounds, 1.0, 1.0));

        let default_fg = self.theme.editor.fg.as_f32();
        let default_bg = terminal_bg_color;

        self.buffer_terminal_header_batch = None;

        if grid.is_empty() {
            self.buffer_terminal_cell_background_instances.clear();
            self.buffer_terminal_text_system
                .set_size(Some(width), Some(bounds[3]));
            self.buffer_terminal_glyph_instances = layout_panel_text(
                "(terminal ready — lazygit starting...)",
                &mut self.buffer_terminal_text_system,
                &mut self.atlas,
                &self.queue,
                origin_x,
                origin_y,
                default_fg,
            );
        } else {
            let font_size = self.theme.ui.panel_font_size;
            let line_h = self.theme.ui.panel_line_height.max(1.0);
            self.buffer_terminal_view_renderer.origin_x = origin_x;
            self.buffer_terminal_view_renderer.origin_y = origin_y;
            self.buffer_terminal_view_renderer.cell_width = (font_size * 0.6).max(1.0);
            self.buffer_terminal_view_renderer.cell_height = line_h;
            self.buffer_terminal_view_renderer.font_size = font_size;

            self.buffer_terminal_cell_background_instances = self
                .buffer_terminal_view_renderer
                .build_background_instances(grid, default_fg, default_bg, width);
            self.buffer_terminal_glyph_instances =
                self.buffer_terminal_view_renderer.build_instances(
                    grid,
                    &mut self.atlas,
                    &self.queue,
                    &mut self.buffer_terminal_text_system,
                    default_fg,
                    default_bg,
                    width,
                );
        }
        self.buffer_terminal_cursor_instances.clear();
        let clip_right = origin_x + width;
        Self::append_terminal_overlay_quads(
            &self.theme,
            cursor_shape,
            cursor_beam_width,
            cursor_underline_height,
            &self.buffer_terminal_view_renderer,
            &mut self.buffer_terminal_cursor_instances,
            grid,
            clip_right,
            terminal_mode,
        );

        self.buffer_terminal_text_pipeline.upload_instances(
            &self.device,
            &self.queue,
            &self.buffer_terminal_glyph_instances,
        );

        Some(bg_quad)
    }

    fn render_terminal_region(
        theme: &crate::config::theme_config::ThemeConfig,
        panel_padding: f32,
        cursor_shape: crate::config::ui_config::CursorShape,
        cursor_beam_width: f32,
        cursor_underline_height: f32,
        text_system: &mut crate::text::text_system::TextSystem,
        view_renderer: &mut crate::terminal::terminal_renderer::TerminalViewRenderer,
        glyph_instances: &mut Vec<crate::render::glyph_instance::GlyphInstance>,
        background_instances: &mut Vec<RegionDrawInstance>,
        cursor_instances: &mut Vec<RegionDrawInstance>,
        atlas: &mut crate::text::atlas::GlyphAtlas,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        text_pipeline: &mut crate::render::text_pipeline::TextPipeline,
        scissor: &mut Option<[u32; 4]>,
        grid: &TerminalGrid,
        bounds: [f32; 4],
        terminal_mode: EditorMode,
    ) {
        if bounds[2] < 1.0 || bounds[3] < 1.0 {
            *scissor = None;
            background_instances.clear();
            cursor_instances.clear();
            return;
        }
        let origin_x = bounds[0] + panel_padding + TERMINAL_SAFE_INSET_X;
        let origin_y = bounds[1] + panel_padding;
        let width = (bounds[2] - panel_padding * 2.0 - TERMINAL_SAFE_INSET_X * 2.0).max(1.0);
        let height = (bounds[3] - panel_padding * 2.0).max(1.0);

        *scissor = rect_to_scissor(inset_bounds(bounds, 1.0, 1.0));

        let default_fg = theme.editor.fg.as_f32();
        let default_bg = theme.ui.terminal_bg.as_f32();

        if grid.is_empty() {
            background_instances.clear();
            text_system.set_size(Some(width), Some(height));
            *glyph_instances = layout_panel_text(
                EMPTY_TERMINAL_HINT,
                text_system,
                atlas,
                queue,
                origin_x,
                origin_y,
                default_fg,
            );
        } else {
            let font_size = theme.ui.panel_font_size;
            let line_h = theme.ui.panel_line_height.max(1.0);
            view_renderer.origin_x = origin_x;
            view_renderer.origin_y = origin_y;
            view_renderer.cell_width = (font_size * 0.6).max(1.0);
            view_renderer.cell_height = line_h;
            view_renderer.font_size = font_size;

            *background_instances =
                view_renderer.build_background_instances(grid, default_fg, default_bg, width);
            *glyph_instances = view_renderer.build_instances(
                grid,
                atlas,
                queue,
                text_system,
                default_fg,
                default_bg,
                width,
            );
        }
        cursor_instances.clear();
        let clip_right = origin_x + width;
        Self::append_terminal_overlay_quads(
            theme,
            cursor_shape,
            cursor_beam_width,
            cursor_underline_height,
            view_renderer,
            cursor_instances,
            grid,
            clip_right,
            terminal_mode,
        );

        text_pipeline.upload_instances(device, queue, glyph_instances);
    }

    fn append_terminal_overlay_quads(
        theme: &crate::config::theme_config::ThemeConfig,
        cursor_shape: crate::config::ui_config::CursorShape,
        cursor_beam_width: f32,
        cursor_underline_height: f32,
        view_renderer: &crate::terminal::terminal_renderer::TerminalViewRenderer,
        cursor_instances: &mut Vec<RegionDrawInstance>,
        grid: &TerminalGrid,
        clip_right: f32,
        terminal_mode: EditorMode,
    ) {
        // Search match highlights — rendered in all terminal modes.
        // Uses the same color as editor search highlights (theme.ui.warning with alpha).
        if !grid.search_matches.is_empty() {
            let mut search_color = theme.ui.warning.as_f32();
            search_color[3] = search_color[3].clamp(0.26, 0.38);
            for (display_row, start_col, end_col_exclusive) in grid.visible_search_match_spans() {
                if start_col >= end_col_exclusive {
                    continue;
                }
                let [x, y, _, h] = view_renderer.cell_rect(display_row, start_col);
                if x >= clip_right {
                    continue;
                }
                let width =
                    (end_col_exclusive.saturating_sub(start_col) as f32) * view_renderer.cell_width;
                cursor_instances.push(RegionDrawInstance::new(
                    [x, y, width.min((clip_right - x).max(0.0)), h.max(1.0)],
                    search_color,
                ));
            }
        }

        match terminal_mode {
            EditorMode::TerminalNormal => {
                let mut selection_color = theme.ui.selection_bg.as_f32();
                selection_color[3] = selection_color[3].clamp(0.18, 0.28);
                for (display_row, start_col, end_col_exclusive) in grid.visible_selection_spans() {
                    if start_col >= end_col_exclusive {
                        continue;
                    }
                    let [x, y, _, h] = view_renderer.cell_rect(display_row, start_col);
                    if x >= clip_right {
                        continue;
                    }
                    let width = (end_col_exclusive.saturating_sub(start_col) as f32)
                        * view_renderer.cell_width;
                    cursor_instances.push(RegionDrawInstance::new(
                        [x, y, width.min((clip_right - x).max(0.0)), h.max(1.0)],
                        selection_color,
                    ));
                }

                if let Some((display_row, col)) = grid.virtual_cursor_display_position() {
                    let [x, y, w, h] = view_renderer.cell_rect(display_row, col);
                    if x < clip_right {
                        let mut cursor_color = theme.ui.accent.as_f32();
                        cursor_color[3] = 0.85;
                        cursor_instances.push(RegionDrawInstance::new(
                            [x, y, w.min((clip_right - x).max(0.0)), h.max(1.0)],
                            cursor_color,
                        ));
                    }
                }
            }
            EditorMode::TerminalFocus if !grid.is_empty() && grid.scroll_offset == 0 => {
                let [cell_x, cell_y, cell_w, cell_h] =
                    view_renderer.cell_rect(grid.cursor_row, grid.cursor_col);
                if cell_x < clip_right {
                    let cursor_color = theme.editor.cursor.as_f32();
                    let (x, y, w, h, alpha) = match cursor_shape {
                        crate::config::ui_config::CursorShape::Beam => (
                            cell_x,
                            cell_y,
                            cursor_beam_width.max(1.0),
                            cell_h.max(1.0),
                            1.0,
                        ),
                        crate::config::ui_config::CursorShape::Underline => {
                            let underline_h = cursor_underline_height.max(1.0);
                            (
                                cell_x,
                                cell_y + (cell_h - underline_h).max(0.0),
                                cell_w.max(1.0),
                                underline_h,
                                1.0,
                            )
                        }
                        crate::config::ui_config::CursorShape::Block => {
                            (cell_x, cell_y, cell_w.max(1.0), cell_h.max(1.0), 0.45)
                        }
                    };
                    cursor_instances.push(RegionDrawInstance::new(
                        [x, y, w, h],
                        [cursor_color[0], cursor_color[1], cursor_color[2], alpha],
                    ));
                }
            }
            _ => {}
        }
    }

    /// Clear terminal — called when the panel is hidden.
    pub fn clear_terminal(&mut self) {
        self.terminal_scissor = None;
        self.terminal_body_batch = None;
        self.terminal_outer_tab_batch = None;
        self.terminal_glyph_instances.clear();
        self.terminal_cell_background_instances.clear();
        self.terminal_cursor_instances.clear();
        self.terminal_text_pipeline
            .upload_instances(&self.device, &self.queue, &[]);
    }

    pub fn clear_buffer_terminal(&mut self) {
        self.buffer_terminal_scissor = None;
        self.buffer_terminal_header_batch = None;
        self.buffer_terminal_glyph_instances.clear();
        self.buffer_terminal_cell_background_instances.clear();
        self.buffer_terminal_cursor_instances.clear();
        self.buffer_terminal_text_pipeline
            .upload_instances(&self.device, &self.queue, &[]);
    }

    // ── Bottom dock outer tab strip ────────────────────────────────────────────

    /// Build the bottom-dock outer tab strip: capped-width tabs left-aligned
    /// under the cap, active tab highlighted with an accent underline, hover
    /// wash behind non-active hovered tabs, and a per-tab status dot. Mirrors
    /// `build_left_tab_strip` and `build_right_tab_strip`.
    ///
    /// `statuses` must be aligned with `labels` (one entry per terminal tab).
    /// `pub(crate)` because the signature carries the crate-only
    /// `TerminalTabDot` mirror enum.
    pub(crate) fn build_bottom_tab_strip(
        &mut self,
        bounds: [f32; 4],
        labels: &[&str],
        icons: &[Option<&'static str>],
        statuses: &[TerminalTabDot],
        active: usize,
        focused: bool,
        hovered_tab_index: Option<usize>,
    ) -> Vec<RegionDrawInstance> {
        let mut chrome: Vec<RegionDrawInstance> = Vec::new();
        self.terminal_outer_tab_batch = None;
        self.bottom_dock_tab_icon_instances.clear();
        if bounds[2] <= 1.0 || bounds[3] <= 1.0 {
            return chrome;
        }

        let font = self.theme.editor.font_size;
        let line_h = self.theme.editor.line_height;
        let fg = self.theme.ui.fg.as_f32();
        let fg_dim = self.theme.ui.fg_dim.as_f32();
        let accent = self.theme.ui.accent.as_f32();
        let tab_base = super::utils::blend_rgb(
            self.theme.editor.bg.as_f32(),
            self.theme.ui.status_bar_bg.as_f32(),
            0.62,
            1.0,
        );
        let border = self.theme.ui.border_color.as_f32();
        let active_bg = self.theme.editor.bg.as_f32();
        let inactive_bg = tab_base;
        // Hover wash uses the fg token at the shared strip-hover alpha so it
        // stays theme-driven and matches the topbar / side-dock intensity.
        let mut hover_bg = fg;
        hover_bg[3] = super::utils::DOCK_TAB_HOVER_ALPHA;
        // Status-dot colors: green while running, ghost when exited cleanly,
        // red on a failed exit — all theme tokens, no hardcoded RGBA.
        let dot_running = self.theme.ui.success.as_f32();
        let dot_exited_ok = self.theme.ui.fg_ghost.as_f32();
        let dot_exited_fail = self.theme.ui.error.as_f32();
        const TOP_BORDER: f32 = 2.0;

        let inset = crate::workbench::layout_engine::BOTTOM_DOCK_OUTLINE_INSET;
        let radius = (self.panel_corner_radius - inset)
            .min(bounds[3])
            .min(bounds[2] * 0.5)
            .max(0.0);

        chrome.push(
            RegionDrawInstance::new(bounds, inactive_bg)
                .with_corner_radii([radius, radius, 0.0, 0.0]),
        );

        let n = labels.len();
        // The "new terminal" button is a compact square pinned to the far right
        // with its own background; the terminal tabs share the width left of it.
        let add_w = Self::bottom_dock_add_button_w(bounds, n);
        let tabs_w = (bounds[2] - add_w).max(0.0);
        // Shared geometry with the hit-test: equal division clamped DOWN to
        // the scaled cap; under the cap tabs stay left-aligned and the space
        // before the "+" button stays empty (rendered AND clickable).
        let tab_w = bottom_dock_tab_width(
            tabs_w,
            n,
            BOTTOM_DOCK_TAB_MAX_WIDTH * self.ui_scale.max(0.5),
        )
        .unwrap_or(0.0);
        let text_y = bounds[1] + ((bounds[3] - line_h) * 0.5).max(0.0);

        // Icon sizing: capped so it always keeps vertical breathing room inside
        // the strip (matches the padded look of the main-editor breadcrumb).
        const ICON_V_PAD: f32 = 9.0;
        let icon_size = (line_h * 0.72)
            .min(font * 1.3)
            .min((bounds[3] - ICON_V_PAD * 2.0).max(8.0));
        let icon_y = bounds[1] + ((bounds[3] - icon_size) * 0.5).max(0.0);

        // Anchor to the terminal body glyphs (everything before the body batch's
        // end) and drop any tab titles left over from a previous frame, so the
        // buffer can't accumulate stale title glyphs when the body isn't rebuilt.
        let body_count = self
            .terminal_body_batch
            .map(|b| b.range.start + b.range.count)
            .unwrap_or(0);
        self.terminal_glyph_instances.truncate(body_count as usize);
        let tab_text_start = body_count;

        let saved_metrics = self.terminal_text_system.buffer_metrics();
        self.terminal_text_system
            .set_metrics(cosmic_text::Metrics::new(font, line_h));
        // The terminal body shapes glyph-by-glyph and leaves the shared text
        // buffer sized to a single cell, which would wrap every label down to one
        // character ("T…"). Give it the whole strip width so each label lays out
        // on one line; we still clamp the text to its tab below.
        self.terminal_text_system
            .set_size(Some(bounds[2].max(1.0)), Some(bounds[3].max(1.0)));
        for (i, label) in labels.iter().enumerate() {
            let tab_x = bounds[0] + i as f32 * tab_w;
            let is_active = i == active;
            let is_first = i == 0;
            // Terminal tabs never own the top-right corner — the add button does.
            let tab_corners = [if is_first { radius } else { 0.0 }, 0.0, 0.0, 0.0];
            // Subtle wash behind a non-active hovered tab so the pointer
            // target is visible before the user commits to a click (mirrors
            // the side-dock strips).
            if !is_active && hovered_tab_index == Some(i) {
                chrome.push(
                    RegionDrawInstance::new([tab_x, bounds[1], tab_w, bounds[3]], hover_bg)
                        .with_corner_radii(tab_corners),
                );
            }
            if is_active {
                chrome.push(
                    RegionDrawInstance::new([tab_x, bounds[1], tab_w, bounds[3]], active_bg)
                        .with_corner_radii(tab_corners),
                );
                let bar_col = if focused { accent } else { fg_dim };
                let bar_x = if is_first { tab_x + radius } else { tab_x };
                let mut bar_w = tab_w;
                if is_first {
                    bar_w -= radius;
                }
                chrome.push(
                    RegionDrawInstance::new(
                        [bar_x, bounds[1], bar_w.max(0.0), TOP_BORDER],
                        bar_col,
                    )
                    .with_corner_radii([
                        if is_first { radius } else { 0.0 },
                        0.0,
                        0.0,
                        0.0,
                    ]),
                );
            }
            if i + 1 < n {
                let mut sep = border;
                sep[3] *= 0.5;
                chrome.push(RegionDrawInstance::new(
                    [
                        tab_x + tab_w - 0.5,
                        bounds[1] + 6.0,
                        1.0,
                        (bounds[3] - 12.0).max(0.0),
                    ],
                    sep,
                ));
            }
            let label_color = if is_active { fg } else { fg_dim };
            let icon = icons.get(i).and_then(|id| *id);
            // Left-align the icon + label (don't center): the icon sits at a fixed
            // left pad so it always renders in full, and the title — which also
            // carries the latest command — flows after it, clamped to the room
            // that's left so it never bleeds into the next tab.
            const TAB_PAD_LEFT: f32 = 12.0;
            const TAB_PAD_RIGHT: f32 = 10.0;
            const ICON_GAP: f32 = 6.0;
            // Status dot leads the row; the icon + label shift right so every
            // tab keeps the same fixed left pad. Dot is vertically centered on
            // the icon so they read as one aligned cluster.
            const DOT_SIZE: f32 = 6.0;
            const DOT_GAP: f32 = 5.0;
            let mut content_x = tab_x + TAB_PAD_LEFT;
            if let Some(status) = statuses.get(i) {
                let dot_color = match *status {
                    TerminalTabDot::Running => dot_running,
                    TerminalTabDot::ExitedOk => dot_exited_ok,
                    TerminalTabDot::ExitedFail => dot_exited_fail,
                };
                chrome.push(
                    RegionDrawInstance::new(
                        [
                            content_x,
                            icon_y + (icon_size - DOT_SIZE).max(0.0) * 0.5,
                            DOT_SIZE,
                            DOT_SIZE,
                        ],
                        dot_color,
                    )
                    .with_corner_radii([DOT_SIZE * 0.5; 4]),
                );
                content_x += DOT_SIZE + DOT_GAP;
            }
            let tab_right = tab_x + tab_w - TAB_PAD_RIGHT;
            let label_x = if let Some(svg_icon) = icon.and_then(|id| canonical_icon_id(id)) {
                self.bottom_dock_tab_icon_instances.push(IconDrawInstance {
                    icon: svg_icon,
                    rect: [content_x, icon_y, icon_size, icon_size],
                    tint: label_color,
                });
                content_x + icon_size + ICON_GAP
            } else {
                content_x
            };
            let label_max_w = (tab_right - label_x).max(0.0);
            // Keep the TAIL of the label: the informative part is the command
            // after "Terminal N · ", so an ellipsis prefix beats head-clipping.
            let label = clamp_monospace_text_left(label, label_max_w, font);
            if !label.is_empty() {
                self.terminal_glyph_instances.extend(layout_panel_text(
                    &label,
                    &mut self.terminal_text_system,
                    &mut self.atlas,
                    &self.queue,
                    label_x,
                    text_y,
                    label_color,
                ));
            }
        }

        // ── "New terminal" button ────────────────────────────────────────────
        if add_w > 1.0 {
            let add_x = bounds[0] + tabs_w;
            // Distinct, slightly accent-tinted background so it reads as a button.
            let add_bg = super::utils::blend_rgb(active_bg, accent, 0.20, 1.0);
            chrome.push(
                RegionDrawInstance::new([add_x, bounds[1], add_w, bounds[3]], add_bg)
                    .with_corner_radii([0.0, radius, 0.0, 0.0]),
            );
            if n > 0 {
                let mut sep = border;
                sep[3] *= 0.5;
                chrome.push(RegionDrawInstance::new(
                    [
                        add_x - 0.5,
                        bounds[1] + 6.0,
                        1.0,
                        (bounds[3] - 12.0).max(0.0),
                    ],
                    sep,
                ));
            }
            // "+" glyph, centered in the button.
            let plus = "+";
            let plus_w = estimate_monospace_width(plus, font);
            let plus_x = add_x + ((add_w - plus_w) * 0.5).max(0.0);
            self.terminal_glyph_instances.extend(layout_panel_text(
                plus,
                &mut self.terminal_text_system,
                &mut self.atlas,
                &self.queue,
                plus_x,
                text_y,
                accent,
            ));
        }

        let divider = super::utils::blend_rgb(tab_base, border, 0.7, 1.0);
        chrome.push(RegionDrawInstance::new(
            [bounds[0], bounds[1] + bounds[3] - 1.0, bounds[2], 1.0],
            divider,
        ));
        self.terminal_text_system.set_metrics(saved_metrics);

        let tab_text_count = self
            .terminal_glyph_instances
            .len()
            .saturating_sub(tab_text_start as usize) as u32;
        self.terminal_outer_tab_batch = rect_to_scissor(bounds).map(|scissor| TextScissorBatch {
            scissor,
            range: InstanceDrawRange {
                start: tab_text_start,
                count: tab_text_count,
            },
        });
        // Re-upload the combined [body + tab titles] buffer. This is the last
        // touch of `terminal_glyph_instances` for the frame, so it guarantees the
        // tab titles reach the GPU even when the terminal body wasn't rebuilt.
        self.terminal_text_pipeline.upload_instances(
            &self.device,
            &self.queue,
            &self.terminal_glyph_instances,
        );
        chrome
    }

    /// Width of the square "new terminal" button pinned to the right of the
    /// strip. Shared by the renderer and the hit-test so they stay in sync. With
    /// no terminals the button spans the whole strip.
    fn bottom_dock_add_button_w(bounds: [f32; 4], term_count: usize) -> f32 {
        if term_count == 0 {
            bounds[2].max(0.0)
        } else {
            bounds[3].min(bounds[2] * 0.5).max(0.0)
        }
    }

    /// Hit-test a point against the bottom-dock outer tab strip. `term_count` is
    /// the number of terminal tabs; returns `Some(i)` for terminal `i`, or
    /// `Some(term_count)` for the "new terminal" (+) button on the right. Tab
    /// geometry goes through the same helpers as `build_bottom_tab_strip`
    /// (`bottom_dock_tab_width` + `bottom_dock_tab_hit_index`) so rendered and
    /// clickable tabs can never drift — including the capped zone, which
    /// reports no terminal target.
    pub fn bottom_dock_tab_index_at(
        &self,
        term_count: usize,
        strip_bounds: [f32; 4],
        pos: (f32, f32),
    ) -> Option<usize> {
        let (px, py) = pos;
        if px < strip_bounds[0]
            || px >= strip_bounds[0] + strip_bounds[2]
            || py < strip_bounds[1]
            || py >= strip_bounds[1] + strip_bounds[3]
        {
            return None;
        }
        let add_w = Self::bottom_dock_add_button_w(strip_bounds, term_count);
        let tabs_w = (strip_bounds[2] - add_w).max(0.0);
        // Click landed on the add button (or there are no terminal tabs).
        if term_count == 0 || px >= strip_bounds[0] + tabs_w {
            return Some(term_count);
        }
        let tab_w = bottom_dock_tab_width(
            tabs_w,
            term_count,
            BOTTOM_DOCK_TAB_MAX_WIDTH * self.ui_scale.max(0.5),
        )?;
        bottom_dock_tab_hit_index(px, strip_bounds[0], tab_w, term_count)
    }

    // ── Welcome logo ───────────────────────────────────────────────────────────
}

#[cfg(test)]
mod tests {
    use super::{BOTTOM_DOCK_TAB_MAX_WIDTH, bottom_dock_tab_hit_index, bottom_dock_tab_width};

    #[test]
    fn bottom_tab_width_rejects_degenerate_inputs() {
        assert_eq!(bottom_dock_tab_width(200.0, 0, 240.0), None, "no tabs → no geometry");
        assert_eq!(bottom_dock_tab_width(0.0, 2, 240.0), None, "empty strip → no geometry");
        assert_eq!(
            bottom_dock_tab_width(-10.0, 2, 240.0),
            None,
            "negative strip → no geometry"
        );
    }

    #[test]
    fn bottom_tab_width_divides_equally_below_cap() {
        // Few terminals: plain equal division, cap never engages.
        assert_eq!(bottom_dock_tab_width(600.0, 3, 240.0), Some(200.0));
        assert_eq!(bottom_dock_tab_width(400.0, 2, 240.0), Some(200.0));
    }

    #[test]
    fn bottom_tab_width_caps_at_max_and_leaves_rest_empty() {
        // Two terminals over a wide strip: each tab is capped at 240, so the
        // rendered tabs only span 480 of 1200 — the remaining 720 before the
        // "+" button stay empty (left-aligned tabs).
        let w = bottom_dock_tab_width(1200.0, 2, BOTTOM_DOCK_TAB_MAX_WIDTH)
            .expect("nonzero strip has geometry");
        assert!((w - BOTTOM_DOCK_TAB_MAX_WIDTH).abs() < f32::EPSILON);
        assert_eq!(bottom_dock_tab_width(100.0, 3, 240.0), Some(100.0 / 3.0));
    }

    #[test]
    fn bottom_tab_hit_matches_rendered_geometry() {
        let strip_x = 10.0;
        let tab_w = 240.0;
        // Inside each left-aligned tab → that tab.
        assert_eq!(bottom_dock_tab_hit_index(10.0, strip_x, tab_w, 2), Some(0));
        assert_eq!(bottom_dock_tab_hit_index(249.9, strip_x, tab_w, 2), Some(0));
        assert_eq!(bottom_dock_tab_hit_index(250.1, strip_x, tab_w, 2), Some(1));
        // Capped zone after the last tab: NO target (must not clamp to the
        // last tab — that's how rendered and clickable rects used to drift).
        assert_eq!(bottom_dock_tab_hit_index(490.0, strip_x, tab_w, 2), None);
        // Before the strip start: no target either.
        assert_eq!(bottom_dock_tab_hit_index(9.5, strip_x, tab_w, 2), None);
    }

    #[test]
    fn bottom_tab_hit_rejects_degenerate_inputs() {
        assert_eq!(bottom_dock_tab_hit_index(50.0, 0.0, 100.0, 0), None);
        assert_eq!(bottom_dock_tab_hit_index(50.0, 0.0, 0.0, 3), None);
        assert_eq!(bottom_dock_tab_hit_index(50.0, 0.0, -5.0, 3), None);
    }
}
