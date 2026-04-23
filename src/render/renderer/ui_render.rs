//! Panel UI rendering: Explorer sidebar, Terminal, Welcome logo, TopBar, StatusBar.

use cosmic_text::Metrics;

use crate::{
    config::theme_config::linear_rgba_to_srgb_u8,
    core::mode::EditorMode,
    render::{
        region_pipeline::RegionDrawInstance,
        renderer::{
            Renderer, SidebarFilterState, SidebarRow, StatusbarLayoutKey, TopbarLayoutKey,
            TopbarTab, TopbarTabKind,
        },
    },
    terminal::grid::TerminalGrid,
    text::text_system::{StyledTextSpan, TextSystem},
};

use super::helpers::{
    clamp_monospace_text, estimate_monospace_width, layout_panel_rich_text, layout_panel_text,
    layout_panel_text_bold, layout_panel_text_italic, mode_display_label, mode_pill_color,
    rect_to_scissor,
};
use crate::render::glyph_instance::GlyphInstance;

const EMPTY_TERMINAL_HINT: &str = "(terminal ready — press F12 to focus)";
const SIDEBAR_FILTER_BAR_HEIGHT: f32 = 30.0;

fn sidebar_list_top(bounds: [f32; 4], filter_state: Option<&SidebarFilterState>) -> f32 {
    bounds[1]
        + if filter_state.is_some() {
            SIDEBAR_FILTER_BAR_HEIGHT + 1.0
        } else {
            0.0
        }
}

fn sidebar_list_bottom(bounds: [f32; 4]) -> f32 {
    bounds[1] + bounds[3] - 2.0
}

fn sidebar_filter_y(bounds: [f32; 4]) -> f32 {
    bounds[1]
}

impl Renderer {
    /// Render the explorer file tree into the left sidebar region.
    ///
    /// Returns selection quads so the caller can draw them via the
    /// region pipeline *before* the text pass.
    pub fn update_sidebar_content(
        &mut self,
        header: Option<&str>,
        rows: &[SidebarRow],
        bounds: [f32; 4],
        sidebar_focused: bool,
        filter_state: Option<&SidebarFilterState>,
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
        let font_size = self.theme.ui.sidebar_font_size;

        let fg = self.theme.ui.fg.as_f32();
        let fg_dim = self.theme.ui.fg_dim.as_f32();
        let fg_ghost = self.theme.ui.fg_ghost.as_f32();
        let accent = self.theme.ui.accent.as_f32();
        let sel_bg = if sidebar_focused {
            [accent[0], accent[1], accent[2], 0.18]
        } else {
            let c = self.theme.ui.selection_bg.as_f32();
            [c[0], c[1], c[2], 0.55]
        };

        let width = (bounds[2] - self.panel_padding * 2.0).max(1.0);
        self.sidebar_text_system.set_size(Some(width), Some(line_h));

        let mut glyphs = Vec::new();
        let mut selection_quads: Vec<RegionDrawInstance> = Vec::new();
        let mut current_y = sidebar_list_top(bounds, filter_state) + self.panel_padding;
        let list_bottom = sidebar_list_bottom(bounds);

        if let Some(filter_state) = filter_state {
            let panel_bg = self.theme.ui.panel_bg.as_f32();
            let border = self.theme.ui.border_color.as_f32();
            let filter_y = sidebar_filter_y(bounds);
            let filter_text = format!("Filter: {}", filter_state.query);
            let text_x = bounds[0] + self.sidebar_base_padding;
            let text_y = filter_y
                + self.panel_padding
                + ((SIDEBAR_FILTER_BAR_HEIGHT - self.panel_padding * 2.0 - line_h).max(0.0) * 0.5);

            selection_quads.push(RegionDrawInstance::new(
                [bounds[0], filter_y, bounds[2], SIDEBAR_FILTER_BAR_HEIGHT],
                panel_bg,
            ));
            selection_quads.push(RegionDrawInstance::new(
                [
                    bounds[0],
                    filter_y + SIDEBAR_FILTER_BAR_HEIGHT,
                    bounds[2],
                    1.0,
                ],
                border,
            ));

            glyphs.extend(layout_panel_text(
                &filter_text,
                &mut self.sidebar_text_system,
                &mut self.atlas,
                &self.queue,
                text_x,
                text_y,
                accent,
            ));

            if filter_state.is_inputting && filter_state.show_cursor {
                let cursor_x = text_x + estimate_monospace_width(&filter_text, font_size);
                let mut cursor_color = accent;
                cursor_color[3] = 0.9;
                selection_quads.push(RegionDrawInstance::new(
                    [cursor_x, text_y + 2.0, 8.0, (line_h - 4.0).max(1.0)],
                    cursor_color,
                ));
            }
        }

        if let Some(header) = header {
            glyphs.extend(layout_panel_text(
                header,
                &mut self.sidebar_text_system,
                &mut self.atlas,
                &self.queue,
                bounds[0] + self.sidebar_base_padding,
                current_y,
                fg_ghost,
            ));
            current_y += line_h;
        }

        for row in rows {
            if current_y + line_h > list_bottom {
                break;
            }

            let x = bounds[0]
                + self.sidebar_base_padding
                + row.depth as f32 * self.sidebar_indent_per_depth;

            let label_color = if row.is_selected {
                selection_quads.push(RegionDrawInstance::new(
                    [
                        bounds[0] + 2.0,
                        current_y,
                        (bounds[2] - 4.0).max(0.0),
                        line_h,
                    ],
                    sel_bg,
                ));
                if sidebar_focused { accent } else { fg }
            } else {
                fg_dim
            };

            // 1. Disclosure arrow (▶ ▼ ·)
            let arrow_str = format!("{} ", row.arrow);
            let arrow_w = arrow_str.chars().count() as f32 * font_size * 0.60;
            glyphs.extend(layout_panel_text(
                &arrow_str,
                &mut self.sidebar_text_system,
                &mut self.atlas,
                &self.queue,
                x,
                current_y,
                fg_ghost,
            ));

            // 2. NerdFont icon (folder/filetype), colored per-filetype
            let nerd_str = format!("{} ", row.nerd_icon);
            let nerd_w = nerd_str.chars().count() as f32 * font_size * 0.60;
            let icon_color = row.icon_color;
            glyphs.extend(layout_panel_text(
                &nerd_str,
                &mut self.sidebar_text_system,
                &mut self.atlas,
                &self.queue,
                x + arrow_w,
                current_y,
                icon_color,
            ));

            // 3. Label
            glyphs.extend(layout_panel_text(
                &row.label,
                &mut self.sidebar_text_system,
                &mut self.atlas,
                &self.queue,
                x + arrow_w + nerd_w,
                current_y,
                label_color,
            ));

            current_y += line_h;
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

    // ── Terminal ───────────────────────────────────────────────────────────────

    /// Render PTY grid output into the bottom terminal panel.
    pub fn update_terminal_content(
        &mut self,
        grid: &TerminalGrid,
        bounds: [f32; 4],
        terminal_mode: EditorMode,
    ) {
        Self::render_terminal_region(
            &self.theme,
            self.panel_padding,
            self.cursor_shape,
            self.cursor_beam_width,
            self.cursor_underline_height,
            &mut self.terminal_text_system,
            &mut self.terminal_view_renderer,
            &mut self.terminal_glyph_instances,
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
        // Buffer terminal (lazygit, v.v.) chiếm toàn bộ center editor area.
        // Không có panel header → dùng editor padding thay vì panel_line_height.
        let panel_padding = self.panel_padding + 4.0;
        let cursor_shape = self.cursor_shape;
        let cursor_beam_width = self.cursor_beam_width;
        let cursor_underline_height = self.cursor_underline_height;

        if bounds[2] < 1.0 || bounds[3] < 1.0 {
            self.buffer_terminal_scissor = None;
            self.buffer_terminal_cursor_instances.clear();
            self.buffer_terminal_text_pipeline
                .upload_instances(&self.device, &self.queue, &[]);
            return None;
        }

        let terminal_bg_color = self.theme.ui.terminal_bg.as_f32();
        // Background quad được trả về để caller thêm vào region_instances.
        let bg_quad =
            crate::render::region_pipeline::RegionDrawInstance::new(bounds, terminal_bg_color);

        let origin_x = bounds[0] + panel_padding;
        let header_h = self.theme.ui.panel_line_height.max(18.0);
        let origin_y = bounds[1] + panel_padding + header_h;
        let width = (bounds[2] - panel_padding * 2.0).max(1.0);

        self.buffer_terminal_scissor = super::helpers::rect_to_scissor(bounds);

        let default_fg = self.theme.editor.fg.as_f32();
        let default_bg = terminal_bg_color;

        self.buffer_terminal_text_system
            .set_size(None, Some(header_h));
        let title = "[Terminal]";
        let title_w =
            super::helpers::estimate_monospace_width(title, self.theme.ui.panel_font_size.max(1.0));
        let title_x = bounds[0] + ((bounds[2] - title_w) * 0.5).max(0.0);
        self.buffer_terminal_cursor_instances
            .push(RegionDrawInstance::new(
                [bounds[0], bounds[1], bounds[2], header_h + panel_padding],
                self.theme.ui.panel_bg.as_f32(),
            ));
        let mut header_glyphs = super::helpers::layout_panel_text(
            title,
            &mut self.buffer_terminal_text_system,
            &mut self.atlas,
            &self.queue,
            title_x,
            bounds[1]
                + panel_padding
                + ((header_h - self.theme.ui.panel_line_height).max(0.0) * 0.5),
            self.theme.ui.fg.as_f32(),
        );

        if grid.used_rows() == 0 {
            self.buffer_terminal_text_system
                .set_size(Some(width), Some(bounds[3]));
            self.buffer_terminal_glyph_instances = super::helpers::layout_panel_text(
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
        header_glyphs.append(&mut self.buffer_terminal_glyph_instances);
        self.buffer_terminal_glyph_instances = header_glyphs;

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
            cursor_instances.clear();
            return;
        }
        let panel_padding = panel_padding + 4.0;
        let header_h = (theme.ui.panel_line_height + 4.0).max(20.0);
        let origin_x = bounds[0] + panel_padding;
        let origin_y = bounds[1] + panel_padding + header_h;
        let width = (bounds[2] - panel_padding * 2.0).max(1.0);
        let height = (bounds[3] - panel_padding * 2.0 - header_h).max(1.0);

        *scissor = rect_to_scissor(bounds);

        let default_fg = theme.editor.fg.as_f32();
        let default_bg = theme.ui.terminal_bg.as_f32();

        let title = "[Terminal]";
        let title_w = estimate_monospace_width(title, theme.ui.panel_font_size.max(1.0));
        let title_x = bounds[0] + ((bounds[2] - title_w) * 0.5).max(0.0);
        cursor_instances.push(RegionDrawInstance::new(
            [bounds[0], bounds[1], bounds[2], header_h + panel_padding],
            theme.ui.panel_bg.as_f32(),
        ));
        let mut header_glyphs = layout_panel_text(
            title,
            text_system,
            atlas,
            queue,
            title_x,
            bounds[1] + panel_padding + ((header_h - theme.ui.panel_line_height).max(0.0) * 0.5),
            theme.ui.fg.as_f32(),
        );

        if grid.used_rows() == 0 {
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
        header_glyphs.append(glyph_instances);
        *glyph_instances = header_glyphs;

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
        match terminal_mode {
            EditorMode::TerminalNormal => {
                let selection_color = theme.ui.selection_bg.as_f32();
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
            EditorMode::TerminalFocus if grid.used_rows() > 0 && grid.scroll_offset == 0 => {
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
                        crate::config::ui_config::CursorShape::Block => (
                            cell_x,
                            cell_y,
                            cursor_beam_width.max(1.0),
                            cell_h.max(1.0),
                            1.0,
                        ),
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
        self.terminal_glyph_instances.clear();
        self.terminal_cursor_instances.clear();
        self.terminal_text_pipeline
            .upload_instances(&self.device, &self.queue, &[]);
    }

    pub fn clear_buffer_terminal(&mut self) {
        self.buffer_terminal_scissor = None;
        self.buffer_terminal_glyph_instances.clear();
        self.buffer_terminal_cursor_instances.clear();
        self.buffer_terminal_text_pipeline
            .upload_instances(&self.device, &self.queue, &[]);
    }

    // ── Welcome logo ───────────────────────────────────────────────────────────

    /// Render centered welcome text into the dedicated welcome layer.
    pub fn update_welcome_screen_content(
        &mut self,
        text: &str,
        spans: &[StyledTextSpan],
        bounds: [f32; 4],
    ) {
        if bounds[2] < 1.0 || bounds[3] < 1.0 {
            self.clear_welcome_logo();
            return;
        }

        self.welcome_logo_scissor = rect_to_scissor(bounds);

        let max_width = (bounds[2] - self.panel_padding * 2.0).max(1.0);
        let font_size = self.theme.editor.font_size;
        let line_height = self.theme.editor.line_height.max(font_size + 4.0);

        use cosmic_text::Metrics;
        self.welcome_logo_text_system
            .set_metrics(Metrics::new(font_size, line_height));
        self.welcome_logo_text_system
            .set_size(Some(max_width), None);
        self.welcome_logo_text_system.set_text_with_spans(
            text,
            self.theme.editor.fg.as_u8(),
            spans,
        );

        let mut content_width = 0.0_f32;
        let mut content_height = 0.0_f32;
        for run in self.welcome_logo_text_system.buffer().layout_runs() {
            content_width = content_width.max(run.line_w.max(1.0));
            content_height = content_height.max(run.line_top + run.line_height.max(1.0));
        }
        content_width = content_width.min(max_width).max(1.0);
        content_height = content_height.max(line_height);

        let origin_x = bounds[0] + ((bounds[2] - content_width) * 0.5).max(self.panel_padding);
        let origin_y = bounds[1]
            + ((bounds[3] - content_height) * 0.5)
                .max(self.panel_padding)
                .min((bounds[3] - content_height - self.panel_padding).max(self.panel_padding));

        self.welcome_logo_glyph_instances = layout_panel_rich_text(
            text,
            spans,
            self.theme.editor.fg.as_f32(),
            &mut self.welcome_logo_text_system,
            &mut self.atlas,
            &self.queue,
            origin_x,
            origin_y,
        );
        self.welcome_logo_text_pipeline.upload_instances(
            &self.device,
            &self.queue,
            &self.welcome_logo_glyph_instances,
        );
    }

    /// Render ANSI art into the welcome-logo layer (separate from the real PTY panel).
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

        let font_size = (height / rows).min(width / (cols * 0.6)).max(1.0);
        let cell_width = (font_size * 0.6).max(1.0);
        let cell_height = font_size.max(1.0);
        let rendered_w = (cell_width * cols).min(width);
        let rendered_h = (cell_height * rows).min(height);
        let origin_x = bounds[0] + ((width - rendered_w) * 0.5).max(0.0);
        let origin_y = bounds[1] + ((height - rendered_h) * 0.5).max(0.0);

        use cosmic_text::Metrics;
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
            rendered_w,
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

    // ── TopBar ─────────────────────────────────────────────────────────────────

    /// Render the tab bar on top. Returns tab chrome quads.
    pub fn update_topbar_content(
        &mut self,
        tabs: &[TopbarTab],
        active_buffer_index: Option<usize>,
        bounds: [f32; 4],
    ) -> Vec<RegionDrawInstance> {
        if bounds[2] < 1.0 || bounds[3] < 1.0 {
            self.topbar_scissor = None;
            self.topbar_glyph_instances.clear();
            self.topbar_chrome_instances.clear();
            self.last_topbar_layout_key = None;
            self.topbar_text_pipeline
                .upload_instances(&self.device, &self.queue, &[]);
            return vec![];
        }

        let layout_key = TopbarLayoutKey {
            tabs: tabs.to_vec(),
            active_buffer_index,
            bounds,
        };
        if self.last_topbar_layout_key.as_ref() == Some(&layout_key) {
            return self.topbar_chrome_instances.clone();
        }

        self.topbar_scissor = rect_to_scissor(bounds);
        let line_h = self.statusbar_line_height;
        let font_size = self.statusbar_font_size;
        let width = (bounds[2] - self.topbar_padding_x * 2.0).max(1.0);
        self.topbar_text_system.set_size(None, Some(bounds[3]));
        let origin_y = bounds[1] + ((bounds[3] - line_h) * 0.5).max(0.0);
        let active_fg = self.theme.ui.fg.as_f32();
        let inactive_fg = self.theme.ui.fg_dim.as_f32();
        let empty_fg = self.theme.ui.fg_ghost.as_f32();
        let active_bg = self.theme.ui.selection_bg.as_f32();
        let accent = self.theme.ui.accent.as_f32();
        let font_family = self.theme.editor.font_family.as_deref();
        let nerd_family = self
            .theme
            .editor
            .nerd_font_family
            .as_deref()
            .filter(|family| !family.is_empty())
            .or(font_family);

        let mut glyphs = Vec::new();
        let mut chrome = Vec::new();
        let mut tab_x = bounds[0] + self.topbar_padding_x;
        let tab_gap = 6.0;
        let available_right = bounds[0] + bounds[2] - self.topbar_padding_x;
        let tab_pad_x = 14.0;
        let icon_gap = 6.0;

        if tabs.is_empty() {
            glyphs.extend(layout_panel_text(
                "[ no file ]",
                &mut self.topbar_text_system,
                &mut self.atlas,
                &self.queue,
                tab_x,
                origin_y,
                empty_fg,
            ));
        } else {
            for (idx, tab) in tabs.iter().enumerate() {
                let icon_glyph = match &tab.kind {
                    TopbarTabKind::Text { path } => self
                        .theme
                        .file_icon_for_path(path, false, false)
                        .glyph
                        .clone(),
                    TopbarTabKind::Terminal => {
                        self.theme.file_icon_for_extension("sh").glyph.clone()
                    }
                    TopbarTabKind::References => {
                        self.theme.file_icon_for_extension("txt").glyph.clone()
                    }
                    TopbarTabKind::Diagnostics => {
                        self.theme.file_icon_for_extension("log").glyph.clone()
                    }
                    TopbarTabKind::FuzzyPicker => {
                        self.theme.file_icon_for_extension("fzf").glyph.clone()
                    }
                    TopbarTabKind::Settings => "⚙".to_string(),
                };
                let icon_color = match &tab.kind {
                    TopbarTabKind::Text { path } => self
                        .theme
                        .icon_theme_for_path(path, false, false)
                        .color
                        .as_f32(),
                    TopbarTabKind::Terminal => self.theme.icons.shell.color.as_f32(),
                    TopbarTabKind::References => self.theme.icons.default_file.color.as_f32(),
                    TopbarTabKind::Diagnostics => self.theme.icons.default_file.color.as_f32(),
                    TopbarTabKind::FuzzyPicker => self.theme.ui.accent.as_f32(),
                    TopbarTabKind::Settings => self.theme.ui.accent.as_f32(),
                };
                let icon_text = format!("{} ", icon_glyph);
                let icon_width = estimate_monospace_width(&icon_text, font_size);
                let label_width = estimate_monospace_width(&tab.label, font_size);
                let tab_width = (tab_pad_x * 2.0 + icon_width + icon_gap + label_width).min(width);
                if tab_x + tab_width > available_right {
                    break;
                }

                let is_active = active_buffer_index == Some(idx);
                if is_active {
                    chrome.push(
                        RegionDrawInstance::new(
                            [
                                tab_x,
                                bounds[1] + 2.0,
                                tab_width,
                                (bounds[3] - 4.0).max(0.0),
                            ],
                            active_bg,
                        )
                        .with_radius(6.0),
                    );
                    chrome.push(RegionDrawInstance::new(
                        [
                            tab_x,
                            (bounds[1] + bounds[3] - 2.0).max(bounds[1]),
                            tab_width,
                            2.0,
                        ],
                        accent,
                    ));
                }

                if idx < tabs.len() - 1 {
                    let mut sep_color = self.theme.ui.fg_ghost.as_f32();
                    sep_color[3] = 0.8;
                    chrome.push(RegionDrawInstance::new(
                        [
                            tab_x + tab_width + (tab_gap * 0.5_f32).floor(),
                            bounds[1] + 8.0,
                            1.0,
                            (bounds[3] - 16.0).max(0.0),
                        ],
                        sep_color,
                    ));
                }

                let icon_x = tab_x + tab_pad_x;
                self.topbar_text_system.set_font_family(nerd_family);
                glyphs.extend(layout_panel_text(
                    &icon_text,
                    &mut self.topbar_text_system,
                    &mut self.atlas,
                    &self.queue,
                    icon_x,
                    origin_y,
                    icon_color,
                ));
                self.topbar_text_system.set_font_family(font_family);
                glyphs.extend(layout_panel_text(
                    &tab.label,
                    &mut self.topbar_text_system,
                    &mut self.atlas,
                    &self.queue,
                    icon_x + icon_width + icon_gap,
                    origin_y,
                    if is_active { active_fg } else { inactive_fg },
                ));
                tab_x += tab_width + tab_gap;
            }
        }

        self.topbar_glyph_instances = glyphs;
        self.topbar_text_pipeline.upload_instances(
            &self.device,
            &self.queue,
            &self.topbar_glyph_instances,
        );
        self.topbar_chrome_instances = chrome;
        self.last_topbar_layout_key = Some(layout_key);
        self.topbar_chrome_instances.clone()
    }

    // ── StatusBar ──────────────────────────────────────────────────────────────

    /// Render the status bar.
    /// Returns background + border + mode-badge quads to be drawn before text.
    pub fn update_statusbar_content(
        &mut self,
        mode: EditorMode,
        pending_keys: &str,
        git_branch: &str,
        filetype: &str,
        line: usize,
        col: usize,
        diagnostics_errors: usize,
        diagnostics_warnings: usize,
        bounds: [f32; 4],
    ) -> Vec<RegionDrawInstance> {
        if bounds[2] < 1.0 || bounds[3] < 1.0 {
            self.statusbar_scissor = None;
            self.statusbar_glyph_instances.clear();
            self.statusbar_chrome_instances.clear();
            self.last_statusbar_layout_key = None;
            self.statusbar_text_pipeline
                .upload_instances(&self.device, &self.queue, &[]);
            return vec![];
        }

        let layout_key = StatusbarLayoutKey {
            mode,
            pending_keys: pending_keys.to_string(),
            git_branch: git_branch.to_string(),
            filetype: filetype.to_string(),
            line,
            col,
            diagnostics_errors,
            diagnostics_warnings,
            bounds,
        };
        if self.last_statusbar_layout_key.as_ref() == Some(&layout_key) {
            return self.statusbar_chrome_instances.clone();
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
            let b = git_branch.trim();
            if b.starts_with("git: ") {
                b.to_string()
            } else {
                format!("git: {b}")
            }
        };
        let right_text = format!(
            "{}  |  {filetype}  |  UTF-8  |  LF  |  Ln {}, Col {}",
            branch_label,
            line + 1,
            col + 1
        );
        let show_diagnostics = diagnostics_errors > 0 || diagnostics_warnings > 0;
        let diagnostics_label = if show_diagnostics {
            format!("❌ {}  ⚠ {}", diagnostics_errors, diagnostics_warnings)
        } else {
            String::new()
        };

        let origin_y = bounds[1] + ((bounds[3] - line_h) * 0.5).max(0.0);
        let fg_dim = self.theme.ui.fg_dim.as_f32();
        let accent = self.theme.ui.accent.as_f32();
        let pill_fg = self.theme.ui.bg.as_f32();
        let error_fg = [0.95, 0.32, 0.32, 1.0];
        let warning_fg = [0.95, 0.78, 0.22, 1.0];

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
        let diagnostics_width = if show_diagnostics {
            estimate_monospace_width(&diagnostics_label, font_size)
        } else {
            0.0
        };
        let right_x = (bounds[0] + bounds[2] - self.statusbar_padding_x - right_width)
            .max(bounds[0] + self.statusbar_padding_x);
        let diag_x = pill_x + pill_width + self.statusbar_padding_x * 0.90;
        let pending_x = diag_x + diagnostics_width + if show_diagnostics { 20.0 } else { 0.0 };
        let pending_gap = self.statusbar_padding_x;
        let pending_maxw = (right_x - pending_x - pending_gap).max(0.0);
        let pending_text = clamp_monospace_text(pending_keys, pending_maxw, font_size);

        if !pending_text.is_empty() {
            glyphs.extend(layout_panel_text(
                &pending_text,
                &mut self.statusbar_text_system,
                &mut self.atlas,
                &self.queue,
                pending_x,
                origin_y,
                accent,
            ));
        }
        glyphs.extend(layout_panel_text(
            &right_text,
            &mut self.statusbar_text_system,
            &mut self.atlas,
            &self.queue,
            right_x,
            origin_y,
            fg_dim,
        ));
        if show_diagnostics {
            let error_part = format!("❌ {}", diagnostics_errors);
            glyphs.extend(layout_panel_text(
                &error_part,
                &mut self.statusbar_text_system,
                &mut self.atlas,
                &self.queue,
                diag_x,
                origin_y,
                error_fg,
            ));
            let warn_x = diag_x + estimate_monospace_width(&error_part, font_size) + 16.0;
            glyphs.extend(layout_panel_text(
                &format!("⚠ {}", diagnostics_warnings),
                &mut self.statusbar_text_system,
                &mut self.atlas,
                &self.queue,
                warn_x,
                origin_y,
                warning_fg,
            ));
        }

        self.statusbar_glyph_instances = glyphs;
        self.statusbar_text_pipeline.upload_instances(
            &self.device,
            &self.queue,
            &self.statusbar_glyph_instances,
        );

        self.statusbar_chrome_instances = vec![
            RegionDrawInstance::new(bounds, self.theme.ui.status_bar_bg.as_f32()),
            RegionDrawInstance::new(
                [bounds[0], bounds[1], bounds[2], 1.0_f32.min(bounds[3])],
                self.theme.ui.border_color.as_f32(),
            ),
            RegionDrawInstance::new(pill_rect, mode_color),
        ];
        self.last_statusbar_layout_key = Some(layout_key);
        self.statusbar_chrome_instances.clone()
    }

    // ── LSP Install Guide Popup ───────────────────────────────────────────────

    /// Render centered LSP install modal với word-wrap thật bằng cosmic-text.
    pub fn update_lsp_guide_popup(
        &mut self,
        binary: &str,
        install_cmd: &str,
        window_w: f32,
        window_h: f32,
    ) {
        const MODAL_W: f32 = 500.0;
        const MARGIN_X: f32 = 24.0;
        const BORDER: f32 = 1.5;
        const INNER_PAD_X: f32 = 22.0;
        const INNER_PAD_Y: f32 = 18.0;
        const BLOCK_GAP: f32 = 8.0;

        let popup_available_w = (window_w - MARGIN_X * 2.0).max(160.0);
        let popup_w = popup_available_w.min(MODAL_W);
        let content_w = (popup_w - INNER_PAD_X * 2.0).max(1.0);

        let bg_color = self.theme.ui.panel_bg.as_f32();
        let warning_color = self.theme.ui.warning.as_f32();
        let fg = self.theme.ui.fg.as_f32();
        let accent = self.theme.ui.accent.as_f32();
        let fg_ghost = self.theme.ui.fg_ghost.as_f32();
        let scrim = [0.0, 0.0, 0.0, 0.36];

        let title = "[ LSP Missing ]";
        let subtitle = format!("File type requires Language Server: {binary}");
        let command = format!("Command: {install_cmd}");
        let hint = "Press [Enter] to auto-install in Terminal  |  [Esc] to cancel";

        let title_font = (self.theme.ui.panel_font_size + 2.0).max(12.0);
        let title_line_h = (self.theme.ui.panel_line_height + 4.0).max(1.0);
        let body_font = self.theme.ui.panel_font_size.max(11.0);
        let body_line_h = self.theme.ui.panel_line_height.max(1.0);

        let title_h = measure_wrapped_block_height(
            &mut self.lsp_guide_text_system,
            title,
            content_w,
            title_font,
            title_line_h,
            warning_color,
            PopupTextStyle::Bold,
        );
        let subtitle_h = measure_wrapped_block_height(
            &mut self.lsp_guide_text_system,
            &subtitle,
            content_w,
            body_font,
            body_line_h,
            fg,
            PopupTextStyle::Normal,
        );
        let command_h = measure_wrapped_block_height(
            &mut self.lsp_guide_text_system,
            &command,
            content_w,
            body_font,
            body_line_h,
            accent,
            PopupTextStyle::Italic,
        );
        let hint_h = measure_wrapped_block_height(
            &mut self.lsp_guide_text_system,
            hint,
            content_w,
            body_font,
            body_line_h,
            fg_ghost,
            PopupTextStyle::Normal,
        );

        let popup_h =
            INNER_PAD_Y * 2.0 + title_h + subtitle_h + command_h + hint_h + BLOCK_GAP * 3.0;
        let x = ((window_w - popup_w) * 0.5).max(0.0);
        let y = ((window_h - popup_h) * 0.5).max(0.0);

        let text_x = x + INNER_PAD_X;
        let mut text_y = y + INNER_PAD_Y;

        let mut glyphs: Vec<GlyphInstance> = Vec::new();
        glyphs.extend(layout_wrapped_block(
            title,
            &mut self.lsp_guide_text_system,
            &mut self.atlas,
            &self.queue,
            text_x,
            text_y,
            warning_color,
            content_w,
            title_font,
            title_line_h,
            PopupTextStyle::Bold,
        ));
        text_y += title_h + BLOCK_GAP;

        glyphs.extend(layout_wrapped_block(
            &subtitle,
            &mut self.lsp_guide_text_system,
            &mut self.atlas,
            &self.queue,
            text_x,
            text_y,
            fg,
            content_w,
            body_font,
            body_line_h,
            PopupTextStyle::Normal,
        ));
        text_y += subtitle_h + BLOCK_GAP;

        glyphs.extend(layout_wrapped_block(
            &command,
            &mut self.lsp_guide_text_system,
            &mut self.atlas,
            &self.queue,
            text_x,
            text_y,
            accent,
            content_w,
            body_font,
            body_line_h,
            PopupTextStyle::Italic,
        ));
        text_y += command_h + BLOCK_GAP;

        glyphs.extend(layout_wrapped_block(
            hint,
            &mut self.lsp_guide_text_system,
            &mut self.atlas,
            &self.queue,
            text_x,
            text_y,
            fg_ghost,
            content_w,
            body_font,
            body_line_h,
            PopupTextStyle::Normal,
        ));

        self.lsp_guide_scissor = rect_to_scissor([0.0, 0.0, window_w, window_h]);
        self.lsp_guide_glyph_instances = glyphs;
        self.lsp_guide_text_pipeline.upload_instances(
            &self.device,
            &self.queue,
            &self.lsp_guide_glyph_instances,
        );

        self.lsp_guide_chrome_instances = vec![
            RegionDrawInstance::new([0.0, 0.0, window_w, window_h], scrim),
            RegionDrawInstance::new(
                [
                    x - BORDER,
                    y - BORDER,
                    popup_w + BORDER * 2.0,
                    popup_h + BORDER * 2.0,
                ],
                warning_color,
            ),
            RegionDrawInstance::new([x, y, popup_w, popup_h], bg_color),
        ];
    }

    /// Clear LSP guide popup state (sau khi user dismiss hoặc install).
    pub fn clear_lsp_guide_popup(&mut self) {
        self.lsp_guide_scissor = None;
        self.lsp_guide_chrome_instances.clear();
        self.lsp_guide_glyph_instances.clear();
        self.lsp_guide_text_pipeline
            .upload_instances(&self.device, &self.queue, &[]);
    }

    pub fn update_toast_popup(&mut self, message: &str, window_w: f32, window_h: f32) {
        const TOAST_W: f32 = 360.0;
        const MARGIN_X: f32 = 18.0;
        const MARGIN_Y: f32 = 18.0;
        const BORDER: f32 = 1.0;
        const INNER_PAD_X: f32 = 16.0;
        const INNER_PAD_Y: f32 = 12.0;

        let toast_available_w = (window_w - MARGIN_X * 2.0).max(140.0);
        let toast_w = toast_available_w.min(TOAST_W);
        let content_w = (toast_w - INNER_PAD_X * 2.0).max(1.0);
        let font_size = self.theme.ui.sidebar_font_size.max(11.0);
        let line_h = self.theme.ui.sidebar_line_height.max(1.0);
        let bg_color = self.theme.ui.panel_bg.as_f32();
        let accent = self.theme.ui.accent.as_f32();
        let fg = self.theme.ui.fg.as_f32();

        let text_h = measure_wrapped_block_height(
            &mut self.toast_text_system,
            message,
            content_w,
            font_size,
            line_h,
            fg,
            PopupTextStyle::Normal,
        );
        let toast_h = INNER_PAD_Y * 2.0 + text_h;
        let x = (window_w - toast_w - MARGIN_X).max(0.0);
        let y = MARGIN_Y.min((window_h - toast_h).max(0.0));

        self.toast_scissor = rect_to_scissor([0.0, 0.0, window_w, window_h]);
        self.toast_glyph_instances = layout_wrapped_block(
            message,
            &mut self.toast_text_system,
            &mut self.atlas,
            &self.queue,
            x + INNER_PAD_X,
            y + INNER_PAD_Y,
            fg,
            content_w,
            font_size,
            line_h,
            PopupTextStyle::Normal,
        );
        self.toast_text_pipeline.upload_instances(
            &self.device,
            &self.queue,
            &self.toast_glyph_instances,
        );

        self.toast_chrome_instances = vec![
            RegionDrawInstance::new(
                [
                    x - BORDER,
                    y - BORDER,
                    toast_w + BORDER * 2.0,
                    toast_h + BORDER * 2.0,
                ],
                accent,
            ),
            RegionDrawInstance::new([x, y, toast_w, toast_h], bg_color),
            RegionDrawInstance::new([x, y, 4.0, toast_h], accent),
        ];
    }

    pub fn clear_toast_popup(&mut self) {
        self.toast_scissor = None;
        self.toast_chrome_instances.clear();
        self.toast_glyph_instances.clear();
        self.toast_text_pipeline
            .upload_instances(&self.device, &self.queue, &[]);
    }
}

#[derive(Debug, Clone, Copy)]
enum PopupTextStyle {
    Normal,
    Bold,
    Italic,
}

fn measure_wrapped_block_height(
    text_system: &mut TextSystem,
    text: &str,
    width: f32,
    font_size: f32,
    line_height: f32,
    color: [f32; 4],
    style: PopupTextStyle,
) -> f32 {
    text_system.set_metrics(Metrics::new(font_size, line_height));
    text_system.set_size(Some(width), None);
    let color_u8 = linear_rgba_to_srgb_u8(color);
    match style {
        PopupTextStyle::Normal => text_system.set_text_with_color(text, color_u8),
        PopupTextStyle::Bold => text_system.set_text_bold_color(text, color_u8),
        PopupTextStyle::Italic => text_system.set_text_italic_color(text, color_u8),
    }
    let wrapped_lines = text_system.buffer().layout_runs().count().max(1);
    wrapped_lines as f32 * line_height
}

fn layout_wrapped_block(
    text: &str,
    text_system: &mut TextSystem,
    atlas: &mut crate::text::atlas::GlyphAtlas,
    queue: &wgpu::Queue,
    origin_x: f32,
    origin_y: f32,
    color: [f32; 4],
    width: f32,
    font_size: f32,
    line_height: f32,
    style: PopupTextStyle,
) -> Vec<GlyphInstance> {
    text_system.set_metrics(Metrics::new(font_size, line_height));
    text_system.set_size(Some(width), None);
    match style {
        PopupTextStyle::Normal => {
            layout_panel_text(text, text_system, atlas, queue, origin_x, origin_y, color)
        }
        PopupTextStyle::Bold => {
            layout_panel_text_bold(text, text_system, atlas, queue, origin_x, origin_y, color)
        }
        PopupTextStyle::Italic => {
            layout_panel_text_italic(text, text_system, atlas, queue, origin_x, origin_y, color)
        }
    }
}
