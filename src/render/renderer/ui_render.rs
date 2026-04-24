//! Panel UI rendering: Explorer sidebar, Terminal, Welcome logo, TopBar, StatusBar.

use cosmic_text::Metrics;

use crate::{
    config::theme_config::linear_rgba_to_srgb_u8,
    core::mode::EditorMode,
    render::{
        region_pipeline::RegionDrawInstance,
        renderer::{
            Renderer, SidebarFilterState, SidebarRow, StatusbarLayoutKey, TextScissorBatch,
            TopbarLayoutKey, TopbarTab, TopbarTabKind, WelcomeAccent, WelcomeLang,
            WelcomeQuickAction, WelcomeRecentProject, WelcomeScreenKind, WelcomeScreenModel,
            WelcomeSection, WelcomeShortcut,
        },
        text_pipeline::InstanceDrawRange,
    },
    terminal::grid::TerminalGrid,
    text::text_system::TextSystem,
};

use super::helpers::{
    clamp_monospace_text, estimate_monospace_width, layout_panel_text, layout_panel_text_bold,
    layout_panel_text_italic, mode_display_label, mode_pill_color, rect_to_scissor,
};
use crate::render::glyph_instance::GlyphInstance;

const EMPTY_TERMINAL_HINT: &str = "(terminal ready — press F12 to focus)";
const SIDEBAR_FILTER_BAR_HEIGHT: f32 = 30.0;
const TERMINAL_HEADER_PADDING_HEIGHT: f32 = 8.0;

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

fn inset_scissor_rect(bounds: [f32; 4], inset_x: f32, inset_y: f32) -> Option<[u32; 4]> {
    let clip_x = bounds[0] + inset_x;
    let clip_y = bounds[1] + inset_y;
    let clip_width = (bounds[2] - inset_x * 2.0).max(0.0);
    let clip_height = (bounds[3] - inset_y * 2.0).max(0.0);
    rect_to_scissor([clip_x, clip_y, clip_width, clip_height])
}

fn topbar_tab_text_scissor(bounds: [f32; 4]) -> Option<[u32; 4]> {
    inset_scissor_rect(bounds, 4.0, 2.0)
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
            self.buffer_terminal_header_batch = None;
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
        let header_h = TERMINAL_HEADER_PADDING_HEIGHT;
        let origin_y = bounds[1] + panel_padding + header_h;
        let width = (bounds[2] - panel_padding * 2.0).max(1.0);

        self.buffer_terminal_scissor = super::helpers::rect_to_scissor(bounds);

        let default_fg = self.theme.editor.fg.as_f32();
        let default_bg = terminal_bg_color;

        self.buffer_terminal_cursor_instances
            .push(RegionDrawInstance::new(
                [bounds[0], bounds[1], bounds[2], header_h + panel_padding],
                self.theme.ui.panel_bg.as_f32(),
            ));
        self.buffer_terminal_header_batch = None;

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
        self.buffer_terminal_cursor_instances.clear();
        self.buffer_terminal_cursor_instances
            .push(RegionDrawInstance::new(
                [bounds[0], bounds[1], bounds[2], header_h + panel_padding],
                self.theme.ui.panel_bg.as_f32(),
            ));
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
        let header_h = TERMINAL_HEADER_PADDING_HEIGHT;
        let origin_x = bounds[0] + panel_padding;
        let origin_y = bounds[1] + panel_padding + header_h;
        let width = (bounds[2] - panel_padding * 2.0).max(1.0);
        let height = (bounds[3] - panel_padding * 2.0 - header_h).max(1.0);

        *scissor = rect_to_scissor(bounds);

        let default_fg = theme.editor.fg.as_f32();
        let default_bg = theme.ui.terminal_bg.as_f32();

        cursor_instances.push(RegionDrawInstance::new(
            [bounds[0], bounds[1], bounds[2], header_h + panel_padding],
            theme.ui.panel_bg.as_f32(),
        ));

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
        cursor_instances.clear();
        cursor_instances.push(RegionDrawInstance::new(
            [bounds[0], bounds[1], bounds[2], header_h + panel_padding],
            theme.ui.panel_bg.as_f32(),
        ));
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
        self.buffer_terminal_header_batch = None;
        self.buffer_terminal_glyph_instances.clear();
        self.buffer_terminal_cursor_instances.clear();
        self.buffer_terminal_text_pipeline
            .upload_instances(&self.device, &self.queue, &[]);
    }

    // ── Welcome logo ───────────────────────────────────────────────────────────

    /// Render the startup welcome dashboard into the dedicated welcome layer.
    pub fn update_welcome_screen_content(&mut self, model: &WelcomeScreenModel, bounds: [f32; 4]) {
        if bounds[2] < 1.0 || bounds[3] < 1.0 {
            self.clear_welcome_logo();
            return;
        }

        self.welcome_logo_scissor = rect_to_scissor(bounds);

        let is_cheat_sheet = model.kind == WelcomeScreenKind::CheatSheet;
        let colors = WelcomeRenderColors::from_theme(&self.theme);
        let mut chrome = Vec::new();
        let mut glyphs = Vec::new();

        if is_cheat_sheet {
            let outer_margin = self.panel_padding.max(10.0);
            let available_w = (bounds[2] - outer_margin * 2.0).max(1.0);
            let page_width = available_w;
            let columns = if page_width >= 980.0 {
                4usize
            } else if page_width >= 760.0 {
                3usize
            } else if page_width >= 520.0 {
                2usize
            } else {
                1usize
            };
            let rows = model.sections.len().div_ceil(columns).max(1);
            let raw_metrics = WelcomeRenderMetrics::new(
                self.theme.editor.font_size,
                self.theme.editor.line_height,
                self.theme.ui.panel_font_size,
                self.theme.ui.panel_line_height,
                self.welcome_section_gap,
            );
            let raw_height = raw_metrics.page_height(rows);
            let available_h = (bounds[3] - outer_margin * 2.0).max(1.0);
            let scale = (available_h / raw_height).clamp(0.82, 1.0);
            let metrics = raw_metrics.scaled(scale);
            let page_height = metrics.page_height(rows).min(available_h);
            let page_x = bounds[0] + ((bounds[2] - page_width) * 0.5).max(0.0);
            let page_y = bounds[1] + outer_margin;

            self.render_welcome_header(
                &mut glyphs,
                &mut chrome,
                model,
                [page_x, page_y, page_width, metrics.header_h],
                metrics,
                colors,
            );
            let legend_y = page_y + metrics.header_h + metrics.gap;
            self.render_welcome_legend(
                &mut glyphs,
                &mut chrome,
                model,
                [page_x, legend_y, page_width, metrics.legend_h],
                metrics,
                colors,
            );
            let card_grid_y = legend_y + metrics.legend_h + metrics.gap;
            let card_gap_total = metrics.gap * (columns.saturating_sub(1) as f32);
            let card_w = ((page_width - card_gap_total) / columns as f32).max(1.0);
            for (index, section) in model.sections.iter().enumerate() {
                let col = index % columns;
                let row = index / columns;
                let x = page_x + col as f32 * (card_w + metrics.gap);
                let y = card_grid_y + row as f32 * (metrics.card_h + metrics.gap);
                self.render_welcome_section_card(
                    &mut glyphs,
                    &mut chrome,
                    section,
                    [x, y, card_w, metrics.card_h],
                    metrics,
                    colors,
                );
            }
            let footer_y = card_grid_y
                + rows as f32 * metrics.card_h
                + rows.saturating_sub(1) as f32 * metrics.gap;
            self.render_welcome_footer(
                &mut glyphs,
                model,
                [page_x, footer_y, page_width, metrics.footer_h],
                metrics,
                colors,
            );
            let _ = page_height;
        } else {
            let raw = WelcomeRenderMetrics::new(
                self.theme.editor.font_size,
                self.theme.editor.line_height,
                self.theme.ui.panel_font_size,
                self.theme.ui.panel_line_height,
                self.welcome_section_gap,
            );
            // Estimate left-panel content height to derive a scale factor
            let raw_logo: f32 = 72.0_f32.max(52.0);
            let raw_btn_h: f32 = 28.0_f32.max(22.0);
            let raw_btn_gap: f32 = 8.0_f32.max(5.0);
            let raw_footer_lh = (raw.footer_font * 1.65_f32).max(14.0);
            let needed_h = raw_logo
                + 20.0
                + raw.title_line_h
                + raw.line_h
                + 6.0
                + raw.line_h
                + 32.0
                + raw_btn_h
                + raw_btn_gap
                + raw_btn_h
                + 36.0
                + raw_footer_lh
                + 4.0
                + raw_footer_lh;
            let available_h = (bounds[3] - 32.0).max(1.0);
            let scale = (available_h / needed_h).clamp(0.70, 1.0);
            let metrics = raw.scaled(scale);
            self.render_welcome_two_panel(&mut glyphs, &mut chrome, model, bounds, metrics, colors);
        }

        self.welcome_logo_chrome_instances = chrome;
        self.welcome_logo_glyph_instances = glyphs;
        self.welcome_logo_text_pipeline.upload_instances(
            &self.device,
            &self.queue,
            &self.welcome_logo_glyph_instances,
        );
    }

    fn render_welcome_header(
        &mut self,
        glyphs: &mut Vec<GlyphInstance>,
        chrome: &mut Vec<RegionDrawInstance>,
        model: &WelcomeScreenModel,
        bounds: [f32; 4],
        metrics: WelcomeRenderMetrics,
        colors: WelcomeRenderColors,
    ) {
        let logo = (52.0 * metrics.scale).max(42.0);
        let logo_x = bounds[0];
        let logo_y = bounds[1] + ((bounds[3] - logo) * 0.5).max(0.0);
        chrome.push(
            RegionDrawInstance::new([logo_x, logo_y, logo, logo], colors.border)
                .with_radius(8.0 * metrics.scale),
        );
        chrome.push(
            RegionDrawInstance::new(
                [logo_x + 1.0, logo_y + 1.0, logo - 2.0, logo - 2.0],
                colors.surface,
            )
            .with_radius(7.0 * metrics.scale),
        );
        let bar_w = (4.0 * metrics.scale).max(2.0);
        let bar_h = logo * 0.58;
        let bar_y = logo_y + logo * 0.21;
        chrome.push(RegionDrawInstance::new(
            [logo_x + logo * 0.26, bar_y, bar_w, bar_h],
            colors.accent,
        ));
        chrome.push(RegionDrawInstance::new(
            [logo_x + logo * 0.68, bar_y, bar_w, bar_h],
            colors.accent,
        ));
        chrome.push(RegionDrawInstance::new(
            [
                logo_x + logo * 0.36,
                logo_y + logo * 0.48,
                logo * 0.28,
                bar_w,
            ],
            colors.accent_dim,
        ));

        let title_x = logo_x + logo + 16.0 * metrics.scale;
        let title_y = bounds[1] + 10.0 * metrics.scale;
        self.append_welcome_text(
            glyphs,
            &model.title,
            title_x,
            title_y,
            colors.fg,
            metrics.title_font,
            metrics.title_line_h,
            PopupTextStyle::Bold,
            Some(bounds[2] * 0.62),
        );
        self.append_welcome_text(
            glyphs,
            &model.subtitle,
            title_x,
            title_y + metrics.title_line_h,
            colors.accent,
            metrics.caption_font,
            metrics.line_h,
            PopupTextStyle::Normal,
            Some(bounds[2] * 0.62),
        );

        let meta = clamp_monospace_text(&model.meta, bounds[2] * 0.34, metrics.caption_font);
        let meta_w = estimate_monospace_width(&meta, metrics.caption_font);
        self.append_welcome_text(
            glyphs,
            &meta,
            bounds[0] + bounds[2] - meta_w,
            title_y + metrics.line_h * 0.55,
            colors.fg_ghost,
            metrics.caption_font,
            metrics.line_h,
            PopupTextStyle::Normal,
            Some(bounds[2] * 0.34),
        );
    }

    fn render_welcome_legend(
        &mut self,
        glyphs: &mut Vec<GlyphInstance>,
        chrome: &mut Vec<RegionDrawInstance>,
        model: &WelcomeScreenModel,
        bounds: [f32; 4],
        metrics: WelcomeRenderMetrics,
        colors: WelcomeRenderColors,
    ) {
        chrome
            .push(RegionDrawInstance::new(bounds, colors.border).with_radius(8.0 * metrics.scale));
        chrome.push(
            RegionDrawInstance::new(
                [
                    bounds[0] + 1.0,
                    bounds[1] + 1.0,
                    bounds[2] - 2.0,
                    bounds[3] - 2.0,
                ],
                colors.surface,
            )
            .with_radius(7.0 * metrics.scale),
        );

        let mut x = bounds[0] + 14.0 * metrics.scale;
        let y = bounds[1] + ((bounds[3] - metrics.key_h) * 0.5).max(0.0);
        self.append_welcome_text(
            glyphs,
            "Legend:",
            x,
            y + 1.0 * metrics.scale,
            colors.fg_dim,
            metrics.body_font,
            metrics.line_h,
            PopupTextStyle::Bold,
            Some(bounds[2]),
        );
        x += estimate_monospace_width("Legend:", metrics.body_font) + 14.0 * metrics.scale;

        for shortcut in &model.legend {
            let used = self.append_welcome_key_sequence(
                glyphs,
                chrome,
                &shortcut.keys,
                x,
                y,
                bounds[0] + bounds[2] - x,
                metrics,
                colors.accent,
                colors,
            );
            x += used + 6.0 * metrics.scale;
            let label = clamp_monospace_text(
                &shortcut.label,
                (bounds[0] + bounds[2] - x).max(1.0),
                metrics.caption_font,
            );
            self.append_welcome_text(
                glyphs,
                &label,
                x,
                y + 2.0 * metrics.scale,
                colors.fg_ghost,
                metrics.caption_font,
                metrics.line_h,
                PopupTextStyle::Normal,
                Some(bounds[0] + bounds[2] - x),
            );
            x += estimate_monospace_width(&label, metrics.caption_font) + 18.0 * metrics.scale;
            if x > bounds[0] + bounds[2] - 72.0 * metrics.scale {
                break;
            }
        }
    }

    fn render_welcome_section_card(
        &mut self,
        glyphs: &mut Vec<GlyphInstance>,
        chrome: &mut Vec<RegionDrawInstance>,
        section: &WelcomeSection,
        bounds: [f32; 4],
        metrics: WelcomeRenderMetrics,
        colors: WelcomeRenderColors,
    ) {
        let accent = colors.accent_for(section.accent);
        chrome
            .push(RegionDrawInstance::new(bounds, colors.border).with_radius(8.0 * metrics.scale));
        chrome.push(
            RegionDrawInstance::new(
                [
                    bounds[0] + 1.0,
                    bounds[1] + 1.0,
                    bounds[2] - 2.0,
                    bounds[3] - 2.0,
                ],
                colors.panel,
            )
            .with_radius(7.0 * metrics.scale),
        );
        chrome.push(
            RegionDrawInstance::new(
                [
                    bounds[0] + 1.0,
                    bounds[1] + 1.0,
                    bounds[2] - 2.0,
                    metrics.card_header_h,
                ],
                colors.surface,
            )
            .with_radius(7.0 * metrics.scale),
        );

        let dot = (8.0 * metrics.scale).max(5.0);
        chrome.push(
            RegionDrawInstance::new(
                [
                    bounds[0] + metrics.card_pad_x,
                    bounds[1] + (metrics.card_header_h - dot) * 0.5,
                    dot,
                    dot,
                ],
                accent,
            )
            .with_radius(dot * 0.5),
        );

        let title = section.title.to_ascii_uppercase();
        let title_x = bounds[0] + metrics.card_pad_x + dot + 8.0 * metrics.scale;
        let caption_w = estimate_monospace_width(&section.caption, metrics.caption_font);
        let title_max_w = (bounds[2] - metrics.card_pad_x * 2.0 - dot - caption_w - 24.0).max(1.0);
        let title = clamp_monospace_text(&title, title_max_w, metrics.section_font);
        self.append_welcome_text(
            glyphs,
            &title,
            title_x,
            bounds[1] + 9.0 * metrics.scale,
            accent,
            metrics.section_font,
            metrics.line_h,
            PopupTextStyle::Bold,
            Some(title_max_w),
        );

        let caption =
            clamp_monospace_text(&section.caption, bounds[2] * 0.28, metrics.caption_font);
        let caption_w = estimate_monospace_width(&caption, metrics.caption_font);
        self.append_welcome_text(
            glyphs,
            &caption,
            bounds[0] + bounds[2] - metrics.card_pad_x - caption_w,
            bounds[1] + 9.0 * metrics.scale,
            colors.fg_ghost,
            metrics.caption_font,
            metrics.line_h,
            PopupTextStyle::Normal,
            Some(bounds[2] * 0.28),
        );

        let inner_x = bounds[0] + metrics.card_pad_x;
        let inner_w = (bounds[2] - metrics.card_pad_x * 2.0).max(1.0);
        let key_area_w = (inner_w * 0.43)
            .clamp(96.0 * metrics.scale, 156.0 * metrics.scale)
            .min(inner_w * 0.62);
        let label_x = inner_x + key_area_w + 10.0 * metrics.scale;
        let label_w = (bounds[0] + bounds[2] - metrics.card_pad_x - label_x).max(1.0);
        let mut row_y = bounds[1] + metrics.card_header_h + 8.0 * metrics.scale;

        for shortcut in &section.shortcuts {
            self.append_welcome_shortcut_row(
                glyphs,
                chrome,
                shortcut,
                inner_x,
                label_x,
                row_y,
                key_area_w,
                label_w,
                metrics.row_h,
                metrics,
                accent,
                colors,
            );
            row_y += metrics.row_h;
            if row_y + metrics.row_h > bounds[1] + bounds[3] {
                break;
            }
        }
    }

    fn append_welcome_shortcut_row(
        &mut self,
        glyphs: &mut Vec<GlyphInstance>,
        chrome: &mut Vec<RegionDrawInstance>,
        shortcut: &WelcomeShortcut,
        key_x: f32,
        label_x: f32,
        y: f32,
        key_area_w: f32,
        label_w: f32,
        row_h: f32,
        metrics: WelcomeRenderMetrics,
        accent: [f32; 4],
        colors: WelcomeRenderColors,
    ) {
        self.append_welcome_key_sequence(
            glyphs,
            chrome,
            &shortcut.keys,
            key_x,
            y + (row_h - metrics.key_h) * 0.5,
            key_area_w,
            metrics,
            accent,
            colors,
        );
        let label = clamp_monospace_text(&shortcut.label, label_w, metrics.body_font);
        let color = if shortcut.important {
            colors.fg
        } else {
            colors.fg_dim
        };
        self.append_welcome_text(
            glyphs,
            &label,
            label_x,
            y + (row_h - metrics.line_h) * 0.5,
            color,
            metrics.body_font,
            metrics.line_h,
            PopupTextStyle::Normal,
            Some(label_w),
        );
    }

    fn append_welcome_key_sequence(
        &mut self,
        glyphs: &mut Vec<GlyphInstance>,
        chrome: &mut Vec<RegionDrawInstance>,
        keys: &[String],
        x: f32,
        y: f32,
        max_width: f32,
        metrics: WelcomeRenderMetrics,
        accent: [f32; 4],
        colors: WelcomeRenderColors,
    ) -> f32 {
        let mut cursor_x = x;
        let key_gap = 4.0 * metrics.scale;
        for key in keys {
            let remaining = x + max_width - cursor_x;
            if remaining < 18.0 * metrics.scale {
                break;
            }
            let label =
                clamp_monospace_text(key, remaining - 8.0 * metrics.scale, metrics.key_font);
            let text_w = estimate_monospace_width(&label, metrics.key_font);
            let key_w = (text_w + 12.0 * metrics.scale)
                .max(22.0 * metrics.scale)
                .min(remaining);
            let outer = alpha(accent, 0.58);
            let inner = colors.key_bg;
            chrome.push(
                RegionDrawInstance::new([cursor_x, y, key_w, metrics.key_h], outer)
                    .with_radius(4.0 * metrics.scale),
            );
            chrome.push(
                RegionDrawInstance::new(
                    [
                        cursor_x + 1.0,
                        y + 1.0,
                        (key_w - 2.0).max(1.0),
                        (metrics.key_h - 2.0).max(1.0),
                    ],
                    inner,
                )
                .with_radius(3.0 * metrics.scale),
            );
            self.append_welcome_text(
                glyphs,
                &label,
                cursor_x + ((key_w - text_w) * 0.5).max(0.0),
                y + ((metrics.key_h - metrics.line_h) * 0.5).max(0.0) + 1.0 * metrics.scale,
                colors.fg,
                metrics.key_font,
                metrics.line_h,
                PopupTextStyle::Bold,
                Some(key_w),
            );
            cursor_x += key_w + key_gap;
        }

        (cursor_x - x - key_gap).max(0.0)
    }

    fn render_welcome_footer(
        &mut self,
        glyphs: &mut Vec<GlyphInstance>,
        model: &WelcomeScreenModel,
        bounds: [f32; 4],
        metrics: WelcomeRenderMetrics,
        colors: WelcomeRenderColors,
    ) {
        let y = bounds[1] + ((bounds[3] - metrics.line_h) * 0.5).max(0.0);
        let right = clamp_monospace_text(&model.footer_right, bounds[2] * 0.4, metrics.footer_font);
        let right_w = estimate_monospace_width(&right, metrics.footer_font);
        let left_max_w = (bounds[2] - right_w - 24.0 * metrics.scale).max(1.0);
        let left = clamp_monospace_text(&model.footer_left, left_max_w, metrics.footer_font);
        self.append_welcome_text(
            glyphs,
            &left,
            bounds[0],
            y,
            colors.fg_ghost,
            metrics.footer_font,
            metrics.line_h,
            PopupTextStyle::Normal,
            Some(left_max_w),
        );
        self.append_welcome_text(
            glyphs,
            &right,
            bounds[0] + bounds[2] - right_w,
            y,
            colors.fg_ghost,
            metrics.footer_font,
            metrics.line_h,
            PopupTextStyle::Normal,
            Some(right_w),
        );
    }

    // ── Two-panel Welcome layout ───────────────────────────────────────────────

    fn render_welcome_two_panel(
        &mut self,
        glyphs: &mut Vec<GlyphInstance>,
        chrome: &mut Vec<RegionDrawInstance>,
        model: &WelcomeScreenModel,
        bounds: [f32; 4],
        metrics: WelcomeRenderMetrics,
        colors: WelcomeRenderColors,
    ) {
        let left_w = (bounds[2] * 0.55).floor();
        let right_w = bounds[2] - left_w;
        let div_x = bounds[0] + left_w;
        let div_margin = bounds[3] * 0.07;
        chrome.push(RegionDrawInstance::new(
            [
                div_x,
                bounds[1] + div_margin,
                0.5,
                bounds[3] - div_margin * 2.0,
            ],
            alpha(colors.border, 0.65),
        ));

        let left_pad = (left_w * 0.09).max(20.0);
        let right_pad_l = (right_w * 0.08).max(18.0);
        let right_pad_r = (right_w * 0.04).max(8.0);
        let left_inner = [
            bounds[0] + left_pad,
            bounds[1],
            left_w - left_pad * 2.0,
            bounds[3],
        ];
        let right_inner = [
            div_x + right_pad_l,
            bounds[1],
            (right_w - right_pad_l - right_pad_r).max(1.0),
            bounds[3],
        ];

        self.render_welcome_hero(glyphs, chrome, model, left_inner, metrics, colors);
        self.render_welcome_sidebar(glyphs, chrome, model, right_inner, metrics, colors);
    }

    fn render_welcome_hero(
        &mut self,
        glyphs: &mut Vec<GlyphInstance>,
        chrome: &mut Vec<RegionDrawInstance>,
        model: &WelcomeScreenModel,
        bounds: [f32; 4],
        metrics: WelcomeRenderMetrics,
        colors: WelcomeRenderColors,
    ) {
        let sc = metrics.scale;
        let logo_size = (72.0 * sc).max(52.0);
        let btn_h = (28.0 * sc).max(22.0);
        let btn_gap = (8.0 * sc).max(5.0);
        let footer_line_h = (metrics.footer_font * 1.65).max(14.0);
        let btn_inner_gap = (8.0 * sc).max(5.0);

        // Decide button row count up front (needed for correct total_h).
        let btn_row_count: usize = if model.quick_actions.is_empty() {
            0
        } else {
            let all_w: f32 = model
                .quick_actions
                .iter()
                .map(|a| {
                    let label_w = a.label.chars().count() as f32 * metrics.body_font * 0.52;
                    let key_w = estimate_monospace_width(&a.key, metrics.key_font) + 8.0 * sc;
                    (12.0 * sc * 2.0 + label_w + 8.0 * sc + key_w + 2.0).ceil()
                })
                .sum::<f32>()
                + btn_inner_gap * model.quick_actions.len().saturating_sub(1) as f32;
            if all_w <= bounds[2] { 1 } else { 2 }
        };
        let btns_h =
            btn_h * btn_row_count as f32 + btn_gap * btn_row_count.saturating_sub(1) as f32;

        let total_h = logo_size
            + 20.0 * sc
            + metrics.title_line_h
            + metrics.line_h
            + 6.0 * sc
            + metrics.line_h
            + 32.0 * sc
            + btns_h
            + 36.0 * sc
            + footer_line_h
            + 4.0 * sc
            + footer_line_h;

        let start_y = bounds[1] + ((bounds[3] - total_h) * 0.5).max(20.0 * sc);
        let cx = bounds[0] + bounds[2] * 0.5;
        let mut cy = start_y;

        // Logo
        let logo_x = cx - logo_size * 0.5;
        self.render_welcome_logo_mark(glyphs, chrome, logo_x, cy, logo_size, metrics, colors);
        cy += logo_size + 20.0 * sc;

        // Title
        let title_approx_w = model.title.chars().count() as f32 * metrics.title_font * 0.52;
        let title_x = (cx - title_approx_w * 0.5).max(bounds[0]);
        self.append_welcome_text(
            glyphs,
            &model.title,
            title_x,
            cy,
            colors.fg,
            metrics.title_font,
            metrics.title_line_h,
            PopupTextStyle::Bold,
            Some(bounds[2]),
        );
        cy += metrics.title_line_h;

        // Subtitle (uppercase, accent, spaced)
        let sub = model.subtitle.to_ascii_uppercase();
        let sub_w = estimate_monospace_width(&sub, metrics.caption_font);
        let sub_x = (cx - sub_w * 0.5).max(bounds[0]);
        self.append_welcome_text(
            glyphs,
            &sub,
            sub_x,
            cy,
            colors.accent,
            metrics.caption_font,
            metrics.line_h,
            PopupTextStyle::Normal,
            Some(bounds[2]),
        );
        cy += metrics.line_h + 6.0 * sc;

        // Meta row
        self.render_welcome_meta_row(glyphs, chrome, model, cx, cy, bounds[2], metrics, colors);
        cy += metrics.line_h + 32.0 * sc;

        // Quick actions
        self.render_welcome_quick_actions(
            glyphs,
            chrome,
            &model.quick_actions,
            cx,
            cy,
            bounds[2],
            btn_h,
            btn_gap,
            metrics,
            colors,
        );
        cy += btns_h + 36.0 * sc;

        // Footer tagline (2 lines, centered)
        let fl_w = estimate_monospace_width(&model.footer_left, metrics.footer_font);
        let fl_x = (cx - fl_w * 0.5).max(bounds[0]);
        self.append_welcome_text(
            glyphs,
            &model.footer_left,
            fl_x,
            cy,
            colors.fg_ghost,
            metrics.footer_font,
            footer_line_h,
            PopupTextStyle::Normal,
            Some(bounds[2]),
        );
        cy += footer_line_h + 4.0 * sc;
        let fr_w = estimate_monospace_width(&model.footer_right, metrics.footer_font);
        let fr_x = (cx - fr_w * 0.5).max(bounds[0]);
        self.append_welcome_text(
            glyphs,
            &model.footer_right,
            fr_x,
            cy,
            colors.fg_ghost,
            metrics.footer_font,
            footer_line_h,
            PopupTextStyle::Normal,
            Some(bounds[2]),
        );
    }

    fn render_welcome_logo_mark(
        &mut self,
        glyphs: &mut Vec<GlyphInstance>,
        chrome: &mut Vec<RegionDrawInstance>,
        logo_x: f32,
        logo_y: f32,
        logo: f32,
        metrics: WelcomeRenderMetrics,
        colors: WelcomeRenderColors,
    ) {
        let sc = metrics.scale;
        let thick = (logo * 0.054).max(2.5);

        // Subtle accent glow background
        chrome.push(
            RegionDrawInstance::new([logo_x, logo_y, logo, logo], alpha(colors.accent, 0.09))
                .with_radius(12.0 * sc),
        );

        // Left bracket [
        let lx = logo_x + logo * 0.26;
        let ly = logo_y + logo * 0.26;
        let lh = logo * 0.48;
        let cap_w = logo * 0.13;
        chrome.push(RegionDrawInstance::new([lx, ly, thick, lh], colors.accent));
        chrome.push(RegionDrawInstance::new(
            [lx, ly, cap_w, thick],
            colors.accent,
        ));
        chrome.push(RegionDrawInstance::new(
            [lx, ly + lh - thick, cap_w, thick],
            colors.accent,
        ));

        // Right bracket partial ] (two corners)
        let rx = logo_x + logo * 0.61;
        let rright = logo_x + logo * 0.74;
        let corner_v = logo * 0.18;
        chrome.push(RegionDrawInstance::new(
            [rx, ly, rright - rx, thick],
            colors.accent,
        ));
        chrome.push(RegionDrawInstance::new(
            [rright - thick, ly, thick, corner_v],
            colors.accent,
        ));
        chrome.push(RegionDrawInstance::new(
            [rx, ly + lh - thick, rright - rx, thick],
            colors.accent,
        ));
        chrome.push(RegionDrawInstance::new(
            [rright - thick, ly + lh - corner_v, thick, corner_v],
            colors.accent,
        ));

        // Chevron < (primary, accent color)
        let ch_font = (logo * 0.34).max(16.0);
        let ch_w = estimate_monospace_width("<", ch_font);
        let ch_x = logo_x + logo * 0.395 - ch_w * 0.5;
        let ch_y = logo_y + (logo - ch_font * 1.15) * 0.5;
        self.append_welcome_text(
            glyphs,
            "<",
            ch_x,
            ch_y,
            colors.accent,
            ch_font,
            ch_font * 1.2,
            PopupTextStyle::Bold,
            Some(logo * 0.4),
        );
        // Second chevron (dim, offset right+down)
        self.append_welcome_text(
            glyphs,
            "<",
            ch_x + logo * 0.10,
            ch_y + logo * 0.065,
            alpha(colors.accent, 0.50),
            ch_font,
            ch_font * 1.2,
            PopupTextStyle::Bold,
            Some(logo * 0.4),
        );
    }

    fn render_welcome_meta_row(
        &mut self,
        glyphs: &mut Vec<GlyphInstance>,
        chrome: &mut Vec<RegionDrawInstance>,
        model: &WelcomeScreenModel,
        cx: f32,
        y: f32,
        avail_w: f32,
        metrics: WelcomeRenderMetrics,
        colors: WelcomeRenderColors,
    ) {
        let sc = metrics.scale;
        let font = metrics.caption_font;
        let sep = "  ·  ";
        let ver = &model.meta_version;
        let meta_rest = &model.meta;

        let sep_w = estimate_monospace_width(sep, font);
        let ver_text_w = estimate_monospace_width(ver, font);
        let ver_pad = 6.0 * sc;
        let badge_w = ver_text_w + ver_pad * 2.0;
        let badge_h = (font * 1.55).max(metrics.line_h * 0.85);
        let meta_rest_w = estimate_monospace_width(meta_rest, font);

        let total_w = if ver.is_empty() {
            meta_rest_w
        } else {
            badge_w + sep_w + meta_rest_w
        };
        let total_w = total_w.min(avail_w);
        let mut x = cx - total_w * 0.5;

        if !ver.is_empty() {
            // Version badge
            chrome.push(
                RegionDrawInstance::new([x, y, badge_w, badge_h], alpha(colors.border, 1.0))
                    .with_radius(4.0 * sc),
            );
            chrome.push(
                RegionDrawInstance::new(
                    [x + 1.0, y + 1.0, badge_w - 2.0, badge_h - 2.0],
                    alpha(colors.surface, 0.95),
                )
                .with_radius(3.0 * sc),
            );
            self.append_welcome_text(
                glyphs,
                ver,
                x + ver_pad,
                y + (badge_h - metrics.line_h) * 0.5,
                colors.fg_dim,
                font,
                metrics.line_h,
                PopupTextStyle::Normal,
                Some(badge_w),
            );
            x += badge_w;
        }

        let meta_full = if ver.is_empty() {
            meta_rest.clone()
        } else {
            format!("{sep}{meta_rest}")
        };
        if let Some(by_pos) = meta_full.find("by ") {
            let before = &meta_full[..by_pos + 3];
            let author = &meta_full[by_pos + 3..];
            let before_w = estimate_monospace_width(before, font);
            self.append_welcome_text(
                glyphs,
                before,
                x,
                y,
                colors.fg_ghost,
                font,
                metrics.line_h,
                PopupTextStyle::Normal,
                None,
            );
            self.append_welcome_text(
                glyphs,
                author,
                x + before_w,
                y,
                colors.fg,
                font,
                metrics.line_h,
                PopupTextStyle::Bold,
                None,
            );
        } else {
            self.append_welcome_text(
                glyphs,
                &meta_full,
                x,
                y,
                colors.fg_ghost,
                font,
                metrics.line_h,
                PopupTextStyle::Normal,
                None,
            );
        }
    }

    fn render_welcome_quick_actions(
        &mut self,
        glyphs: &mut Vec<GlyphInstance>,
        chrome: &mut Vec<RegionDrawInstance>,
        actions: &[WelcomeQuickAction],
        cx: f32,
        start_y: f32,
        avail_w: f32,
        btn_h: f32,
        btn_gap: f32,
        metrics: WelcomeRenderMetrics,
        colors: WelcomeRenderColors,
    ) -> usize {
        if actions.is_empty() {
            return 0;
        }
        let sc = metrics.scale;
        let btn_inner_gap = (8.0 * sc).max(5.0);
        let btn_widths: Vec<f32> = actions
            .iter()
            .map(|a| {
                let label_w = a.label.chars().count() as f32 * metrics.body_font * 0.52;
                let key_w = estimate_monospace_width(&a.key, metrics.key_font) + 8.0 * sc;
                (12.0 * sc * 2.0 + label_w + 8.0 * sc + key_w + 2.0).ceil()
            })
            .collect();

        // Prefer a single row; fall back to 3+rest split when they don't all fit.
        let all_w: f32 =
            btn_widths.iter().sum::<f32>() + btn_inner_gap * actions.len().saturating_sub(1) as f32;
        let split = if all_w <= avail_w {
            actions.len()
        } else {
            3.min(actions.len())
        };

        let row1_w: f32 = btn_widths[..split].iter().sum::<f32>()
            + btn_inner_gap * (split.saturating_sub(1)) as f32;
        let mut x = (cx - row1_w.min(avail_w) * 0.5).max(0.0);
        for (action, &w) in actions[..split].iter().zip(btn_widths[..split].iter()) {
            self.render_welcome_action_btn(
                glyphs, chrome, action, x, start_y, w, btn_h, metrics, colors,
            );
            x += w + btn_inner_gap;
        }

        if split < actions.len() {
            let row2_w: f32 = btn_widths[split..].iter().sum::<f32>()
                + btn_inner_gap * (actions.len() - split).saturating_sub(1) as f32;
            let mut x = (cx - row2_w.min(avail_w) * 0.5).max(0.0);
            let y2 = start_y + btn_h + btn_gap;
            for (action, &w) in actions[split..].iter().zip(btn_widths[split..].iter()) {
                self.render_welcome_action_btn(
                    glyphs, chrome, action, x, y2, w, btn_h, metrics, colors,
                );
                x += w + btn_inner_gap;
            }
            2
        } else {
            1
        }
    }

    fn render_welcome_action_btn(
        &mut self,
        glyphs: &mut Vec<GlyphInstance>,
        chrome: &mut Vec<RegionDrawInstance>,
        action: &WelcomeQuickAction,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        metrics: WelcomeRenderMetrics,
        colors: WelcomeRenderColors,
    ) {
        let sc = metrics.scale;
        let pad_x = 12.0 * sc;
        let inner_gap = 8.0 * sc;
        // Button border + fill
        chrome.push(
            RegionDrawInstance::new([x, y, w, h], alpha(colors.border, 0.85)).with_radius(6.0 * sc),
        );
        chrome.push(
            RegionDrawInstance::new([x + 1.0, y + 1.0, w - 2.0, h - 2.0], colors.surface)
                .with_radius(5.0 * sc),
        );
        // Label
        let label_w = action.label.chars().count() as f32 * metrics.body_font * 0.52;
        self.append_welcome_text(
            glyphs,
            &action.label,
            x + pad_x,
            y + (h - metrics.line_h) * 0.5,
            colors.fg_dim,
            metrics.body_font,
            metrics.line_h,
            PopupTextStyle::Normal,
            None,
        );
        // Key badge
        let key_text_w = estimate_monospace_width(&action.key, metrics.key_font);
        let key_badge_w = key_text_w + 8.0 * sc;
        let key_x = x + pad_x + label_w + inner_gap;
        let key_badge_h = (h - 8.0 * sc).max(h * 0.6);
        let key_y = y + (h - key_badge_h) * 0.5;
        chrome.push(
            RegionDrawInstance::new(
                [key_x, key_y, key_badge_w, key_badge_h],
                alpha(colors.accent, 0.30),
            )
            .with_radius(3.5 * sc),
        );
        chrome.push(
            RegionDrawInstance::new(
                [
                    key_x + 0.5,
                    key_y + 0.5,
                    (key_badge_w - 1.0).max(1.0),
                    (key_badge_h - 1.0).max(1.0),
                ],
                alpha(colors.accent, 0.12),
            )
            .with_radius(3.0 * sc),
        );
        self.append_welcome_text(
            glyphs,
            &action.key,
            key_x + 4.0 * sc,
            key_y + (key_badge_h - metrics.line_h) * 0.5,
            colors.accent,
            metrics.key_font,
            metrics.line_h,
            PopupTextStyle::Normal,
            None,
        );
    }

    fn render_welcome_sidebar(
        &mut self,
        glyphs: &mut Vec<GlyphInstance>,
        chrome: &mut Vec<RegionDrawInstance>,
        model: &WelcomeScreenModel,
        bounds: [f32; 4],
        metrics: WelcomeRenderMetrics,
        colors: WelcomeRenderColors,
    ) {
        let sc = metrics.scale;
        let top_pad = (bounds[3] * 0.07).max(18.0 * sc);
        let label_font = metrics.section_font;
        let label_h = (label_font * 2.2).max(18.0);
        let proj_row_h = (40.0 * sc).max(30.0);
        let short_row_h = (28.0 * sc).max(22.0);
        let mut y = bounds[1] + top_pad;

        // ── Recent projects ──
        if !model.recent_projects.is_empty() {
            self.append_welcome_text(
                glyphs,
                "RECENT PROJECTS",
                bounds[0],
                y,
                colors.fg_ghost,
                label_font,
                label_h,
                PopupTextStyle::Normal,
                Some(bounds[2]),
            );
            y += label_h;

            for proj in &model.recent_projects {
                self.render_welcome_recent_row(
                    glyphs, chrome, proj, bounds[0], y, bounds[2], proj_row_h, metrics, colors,
                );
                y += proj_row_h;
            }

            // "show all →"
            let show_all = "show all  \u{2192}";
            self.append_welcome_text(
                glyphs,
                show_all,
                bounds[0] + 8.0 * sc,
                y + 3.0 * sc,
                colors.accent,
                metrics.caption_font,
                label_h * 0.85,
                PopupTextStyle::Normal,
                Some(bounds[2]),
            );
            y += label_h * 0.85 + 16.0 * sc;
        }

        // ── Keyboard shortcuts ──
        if !model.sidebar_shortcuts.is_empty() {
            self.append_welcome_text(
                glyphs,
                "KEYBOARD SHORTCUTS",
                bounds[0],
                y,
                colors.fg_ghost,
                label_font,
                label_h,
                PopupTextStyle::Normal,
                Some(bounds[2]),
            );
            y += label_h;

            let key_area_w = (120.0 * sc).clamp(90.0, bounds[2] * 0.52);
            let label_x = bounds[0] + key_area_w;
            let label_w = (bounds[2] - key_area_w).max(1.0);

            for shortcut in &model.sidebar_shortcuts {
                // Separator
                chrome.push(RegionDrawInstance::new(
                    [bounds[0], y + short_row_h - 0.5, bounds[2], 0.5],
                    alpha(colors.border, 0.38),
                ));
                self.append_welcome_shortcut_row(
                    glyphs,
                    chrome,
                    shortcut,
                    bounds[0],
                    label_x,
                    y,
                    key_area_w,
                    label_w,
                    short_row_h,
                    metrics,
                    colors.accent,
                    colors,
                );
                y += short_row_h;
                if y + short_row_h > bounds[1] + bounds[3] - 24.0 * sc {
                    break;
                }
            }

            // Hint
            if !model.shortcuts_hint.is_empty() {
                y += 8.0 * sc;
                self.render_welcome_shortcuts_hint(
                    glyphs,
                    &model.shortcuts_hint,
                    bounds[0],
                    y,
                    bounds[2],
                    metrics,
                    colors,
                );
            }
        }
    }

    fn render_welcome_recent_row(
        &mut self,
        glyphs: &mut Vec<GlyphInstance>,
        chrome: &mut Vec<RegionDrawInstance>,
        proj: &WelcomeRecentProject,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        metrics: WelcomeRenderMetrics,
        colors: WelcomeRenderColors,
    ) {
        let sc = metrics.scale;

        if proj.active {
            chrome.push(
                RegionDrawInstance::new([x, y, w, h], alpha(colors.accent, 0.10))
                    .with_radius(5.0 * sc),
            );
            let rail_w = 2.5 * sc;
            let rail_mg = h * 0.15;
            chrome.push(
                RegionDrawInstance::new([x, y + rail_mg, rail_w, h - rail_mg * 2.0], colors.accent)
                    .with_radius(rail_w * 0.5),
            );
        }

        // Lang badge
        let badge_font = metrics.caption_font;
        let badge_label = proj.lang.label();
        let badge_text_w = estimate_monospace_width(badge_label, badge_font);
        let badge_pad = 5.0 * sc;
        let badge_w = badge_text_w + badge_pad * 2.0;
        let badge_h = (badge_font * 1.85).max(14.0);
        let badge_off = if proj.active { 8.0 * sc } else { 2.0 * sc };
        let badge_x = x + badge_off;
        let badge_y = y + (h - badge_h) * 0.5;

        let lang_color = match &proj.lang {
            WelcomeLang::Rust => colors.leader,
            WelcomeLang::TypeScript => colors.lsp,
            _ => colors.fg_ghost,
        };

        chrome.push(
            RegionDrawInstance::new(
                [badge_x, badge_y, badge_w, badge_h],
                alpha(colors.border, 0.9),
            )
            .with_radius(3.0 * sc),
        );
        chrome.push(
            RegionDrawInstance::new(
                [badge_x + 1.0, badge_y + 1.0, badge_w - 2.0, badge_h - 2.0],
                colors.surface,
            )
            .with_radius(2.5 * sc),
        );
        self.append_welcome_text(
            glyphs,
            badge_label,
            badge_x + badge_pad,
            badge_y + (badge_h - metrics.line_h) * 0.5,
            lang_color,
            badge_font,
            metrics.line_h,
            PopupTextStyle::Bold,
            None,
        );

        // Name + path
        let info_x = badge_x + badge_w + 10.0 * sc;
        let ago_w = estimate_monospace_width(&proj.ago, metrics.caption_font);
        let info_w = (x + w - info_x - ago_w - 8.0 * sc).max(1.0);

        let name_color = if proj.active {
            colors.fg
        } else {
            colors.fg_dim
        };
        let name_style = if proj.active {
            PopupTextStyle::Bold
        } else {
            PopupTextStyle::Normal
        };
        let name_y = y + (h * 0.20).max(3.0 * sc);
        self.append_welcome_text(
            glyphs,
            &proj.name,
            info_x,
            name_y,
            name_color,
            metrics.body_font,
            metrics.line_h,
            name_style,
            Some(info_w),
        );
        let path = clamp_monospace_text(&proj.path, info_w, metrics.caption_font);
        self.append_welcome_text(
            glyphs,
            &path,
            info_x,
            name_y + metrics.line_h,
            colors.fg_ghost,
            metrics.caption_font,
            metrics.line_h,
            PopupTextStyle::Normal,
            Some(info_w),
        );

        // Time ago (right-aligned)
        let ago_x = x + w - ago_w;
        self.append_welcome_text(
            glyphs,
            &proj.ago,
            ago_x,
            y + (h - metrics.line_h) * 0.5,
            colors.fg_ghost,
            metrics.caption_font,
            metrics.line_h,
            PopupTextStyle::Normal,
            None,
        );
    }

    fn render_welcome_shortcuts_hint(
        &mut self,
        glyphs: &mut Vec<GlyphInstance>,
        hint: &str,
        x: f32,
        y: f32,
        avail_w: f32,
        metrics: WelcomeRenderMetrics,
        colors: WelcomeRenderColors,
    ) {
        // Render hint with highlighted segments (text between ? and :help styled in accent)
        let font = metrics.caption_font;
        let line_h = metrics.line_h;
        let mut cursor_x = x;
        // Simple segmented render: split on highlighted tokens
        let segments: Vec<(&str, bool)> = {
            let mut segs = Vec::new();
            let mut remaining = hint;
            while !remaining.is_empty() {
                if let Some(pos) = remaining.find('?') {
                    if pos > 0 {
                        segs.push((&remaining[..pos], false));
                    }
                    segs.push((&remaining[pos..pos + 1], true));
                    remaining = &remaining[pos + 1..];
                } else if let Some(pos) = remaining.find(":help") {
                    if pos > 0 {
                        segs.push((&remaining[..pos], false));
                    }
                    segs.push((&remaining[pos..pos + 5], true));
                    remaining = &remaining[pos + 5..];
                } else {
                    segs.push((remaining, false));
                    remaining = "";
                }
            }
            segs
        };
        for (seg, highlighted) in segments {
            if seg.is_empty() {
                continue;
            }
            let color = if highlighted {
                colors.accent
            } else {
                colors.fg_ghost
            };
            let seg_w = estimate_monospace_width(seg, font);
            if cursor_x + seg_w > x + avail_w {
                break;
            }
            self.append_welcome_text(
                glyphs,
                seg,
                cursor_x,
                y,
                color,
                font,
                line_h,
                PopupTextStyle::Normal,
                Some(avail_w - (cursor_x - x)),
            );
            cursor_x += seg_w;
        }
    }

    fn append_welcome_text(
        &mut self,
        glyphs: &mut Vec<GlyphInstance>,
        text: &str,
        x: f32,
        y: f32,
        color: [f32; 4],
        font_size: f32,
        line_height: f32,
        style: PopupTextStyle,
        max_width: Option<f32>,
    ) {
        self.welcome_logo_text_system
            .set_metrics(Metrics::new(font_size, line_height));
        self.welcome_logo_text_system
            .set_size(max_width, Some(line_height * 2.0));

        let mut next = match style {
            PopupTextStyle::Normal => layout_panel_text(
                text,
                &mut self.welcome_logo_text_system,
                &mut self.atlas,
                &self.queue,
                x,
                y,
                color,
            ),
            PopupTextStyle::Bold => layout_panel_text_bold(
                text,
                &mut self.welcome_logo_text_system,
                &mut self.atlas,
                &self.queue,
                x,
                y,
                color,
            ),
            PopupTextStyle::Italic => layout_panel_text_italic(
                text,
                &mut self.welcome_logo_text_system,
                &mut self.atlas,
                &self.queue,
                x,
                y,
                color,
            ),
        };
        glyphs.append(&mut next);
    }

    /// Render ANSI art into the welcome-logo layer (separate from the real PTY panel).
    pub fn update_welcome_logo_content(&mut self, grid: &TerminalGrid, bounds: [f32; 4]) {
        if bounds[2] < 1.0 || bounds[3] < 1.0 {
            self.welcome_logo_scissor = None;
            self.welcome_logo_glyph_instances.clear();
            self.welcome_logo_chrome_instances.clear();
            self.welcome_logo_text_pipeline
                .upload_instances(&self.device, &self.queue, &[]);
            return;
        }

        self.welcome_logo_scissor = rect_to_scissor(bounds);
        self.welcome_logo_chrome_instances.clear();

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
        self.welcome_logo_chrome_instances.clear();
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
            self.topbar_text_batches.clear();
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
        let mut text_batches = Vec::new();
        let mut chrome = Vec::new();
        let mut tab_x = bounds[0] + self.topbar_padding_x;
        let tab_gap = 6.0;
        let available_right = bounds[0] + bounds[2] - self.topbar_padding_x;
        let tab_pad_x = 14.0;
        let icon_gap = 6.0;

        if tabs.is_empty() {
            let start = glyphs.len() as u32;
            glyphs.extend(layout_panel_text(
                "[ no file ]",
                &mut self.topbar_text_system,
                &mut self.atlas,
                &self.queue,
                tab_x,
                origin_y,
                empty_fg,
            ));
            let count = glyphs.len() as u32 - start;
            if let Some(scissor) = topbar_tab_text_scissor([tab_x, bounds[1], width, bounds[3]]) {
                if count > 0 {
                    text_batches.push(TextScissorBatch {
                        scissor,
                        range: InstanceDrawRange { start, count },
                    });
                }
            }
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
                    TopbarTabKind::CheatSheet => "?".to_string(),
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
                    TopbarTabKind::CheatSheet => self.theme.ui.warning.as_f32(),
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
                let batch_start = glyphs.len() as u32;
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
                let batch_count = glyphs.len() as u32 - batch_start;
                if let Some(scissor) =
                    topbar_tab_text_scissor([tab_x, bounds[1], tab_width, bounds[3]])
                {
                    if batch_count > 0 {
                        text_batches.push(TextScissorBatch {
                            scissor,
                            range: InstanceDrawRange {
                                start: batch_start,
                                count: batch_count,
                            },
                        });
                    }
                }
                tab_x += tab_width + tab_gap;
            }
        }

        self.topbar_glyph_instances = glyphs;
        self.topbar_text_pipeline.upload_instances(
            &self.device,
            &self.queue,
            &self.topbar_glyph_instances,
        );
        self.topbar_text_batches = text_batches;
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
struct WelcomeRenderMetrics {
    scale: f32,
    gap: f32,
    header_h: f32,
    legend_h: f32,
    footer_h: f32,
    card_h: f32,
    card_header_h: f32,
    card_pad_x: f32,
    row_h: f32,
    key_h: f32,
    title_font: f32,
    title_line_h: f32,
    section_font: f32,
    body_font: f32,
    caption_font: f32,
    key_font: f32,
    footer_font: f32,
    line_h: f32,
}

impl WelcomeRenderMetrics {
    fn new(
        editor_font: f32,
        editor_line_h: f32,
        panel_font: f32,
        panel_line_h: f32,
        section_gap: f32,
    ) -> Self {
        let title_font = (editor_font * 1.48).max(22.0);
        let body_font = (panel_font * 0.96).max(12.0);
        let line_h = panel_line_h.max(body_font + 7.0);
        Self {
            scale: 1.0,
            gap: section_gap.max(14.0),
            header_h: 76.0,
            legend_h: 42.0,
            footer_h: 30.0,
            card_h: 190.0,
            card_header_h: 32.0,
            card_pad_x: 14.0,
            row_h: 24.0,
            key_h: 20.0,
            title_font,
            title_line_h: editor_line_h.max(title_font + 6.0),
            section_font: (panel_font * 0.82).max(10.0),
            body_font,
            caption_font: (panel_font * 0.78).max(9.5),
            key_font: (panel_font * 0.78).max(9.5),
            footer_font: (panel_font * 0.76).max(9.0),
            line_h,
        }
    }

    fn scaled(self, scale: f32) -> Self {
        Self {
            scale,
            gap: self.gap * scale,
            header_h: self.header_h * scale,
            legend_h: self.legend_h * scale,
            footer_h: self.footer_h * scale,
            card_h: self.card_h * scale,
            card_header_h: self.card_header_h * scale,
            card_pad_x: self.card_pad_x * scale,
            row_h: self.row_h * scale,
            key_h: self.key_h * scale,
            title_font: (self.title_font * scale).max(18.0),
            title_line_h: (self.title_line_h * scale).max(24.0),
            section_font: (self.section_font * scale).max(9.0),
            body_font: (self.body_font * scale).max(10.5),
            caption_font: (self.caption_font * scale).max(8.5),
            key_font: (self.key_font * scale).max(8.5),
            footer_font: (self.footer_font * scale).max(8.0),
            line_h: (self.line_h * scale).max(16.0),
        }
    }

    fn page_height(self, rows: usize) -> f32 {
        self.header_h
            + self.gap
            + self.legend_h
            + self.gap
            + rows as f32 * self.card_h
            + rows.saturating_sub(1) as f32 * self.gap
            + self.footer_h
    }
}

#[derive(Debug, Clone, Copy)]
struct WelcomeRenderColors {
    surface: [f32; 4],
    panel: [f32; 4],
    border: [f32; 4],
    key_bg: [f32; 4],
    fg: [f32; 4],
    fg_dim: [f32; 4],
    fg_ghost: [f32; 4],
    accent: [f32; 4],
    accent_dim: [f32; 4],
    normal: [f32; 4],
    insert: [f32; 4],
    visual: [f32; 4],
    leader: [f32; 4],
    lsp: [f32; 4],
    explorer: [f32; 4],
    terminal: [f32; 4],
    palette: [f32; 4],
}

impl WelcomeRenderColors {
    fn from_theme(theme: &crate::config::theme_config::ThemeConfig) -> Self {
        Self {
            surface: alpha(theme.ui.overlay_bg.as_f32(), 0.90),
            panel: alpha(theme.ui.panel_bg.as_f32(), 0.94),
            border: alpha(theme.ui.border_color.as_f32(), 0.82),
            key_bg: alpha(theme.ui.bg.as_f32(), 0.72),
            fg: theme.ui.fg.as_f32(),
            fg_dim: theme.ui.fg_dim.as_f32(),
            fg_ghost: theme.ui.fg_ghost.as_f32(),
            accent: theme.ui.accent.as_f32(),
            accent_dim: alpha(theme.ui.accent.as_f32(), 0.62),
            normal: theme.ui.mode_normal.as_f32(),
            insert: theme.ui.mode_insert.as_f32(),
            visual: theme.ui.mode_visual.as_f32(),
            leader: theme.ui.amber.as_f32(),
            lsp: theme.ui.info.as_f32(),
            explorer: theme.ui.warning.as_f32(),
            terminal: theme.ui.error.as_f32(),
            palette: theme.ui.magenta.as_f32(),
        }
    }

    fn accent_for(self, accent: WelcomeAccent) -> [f32; 4] {
        match accent {
            WelcomeAccent::Global => self.accent,
            WelcomeAccent::Normal => self.normal,
            WelcomeAccent::Insert => self.insert,
            WelcomeAccent::Visual => self.visual,
            WelcomeAccent::Leader => self.leader,
            WelcomeAccent::Lsp => self.lsp,
            WelcomeAccent::Explorer => self.explorer,
            WelcomeAccent::Terminal => self.terminal,
            WelcomeAccent::Palette => self.palette,
        }
    }
}

fn alpha(mut color: [f32; 4], alpha: f32) -> [f32; 4] {
    color[3] = alpha;
    color
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
