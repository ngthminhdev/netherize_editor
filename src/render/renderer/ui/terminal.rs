use crate::{
    core::mode::EditorMode,
    render::{
        region_pipeline::RegionDrawInstance,
        renderer::{Renderer, TextScissorBatch},
        text_pipeline::InstanceDrawRange,
    },
    terminal::grid::TerminalGrid,
};

use super::super::helpers::{clamp_monospace_text, layout_panel_text, rect_to_scissor};

const EMPTY_TERMINAL_HINT: &str = "(terminal ready — press F12 to focus)";
const TERMINAL_SAFE_INSET_X: f32 = 2.0;

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
        self.terminal_tab_bar_batch = None;
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
        self.terminal_tab_bar_batch = None;
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

    // ── Terminal tab bar ─────────────────────────────────────────────────────────

    const TAB_BAR_HEIGHT: f32 = 50.0;
    const TAB_BAR_OUTLINE_INSET: f32 = 2.0;
    const TAB_BAR_PADDING_X: f32 = 3.0;
    const TAB_BAR_DOT_SIZE: f32 = 8.0;
    const TAB_BAR_DOT_GAP: f32 = 14.0;
    const TAB_BAR_TOP_BORDER: f32 = 2.0;

    /// Render terminal tab bar at the top of the bottom panel.
    ///
    /// Returns chrome quads (backgrounds, dots, borders) to be added to
    /// `region_instances` and the remaining `[x, y, w, h]` bounds for the
    /// terminal grid below the tab strip.
    pub fn terminal_tab_bar_content_bounds(&self, bounds: [f32; 4], tab_count: usize) -> [f32; 4] {
        if tab_count <= 1 {
            return bounds;
        }
        let inset = Self::TAB_BAR_OUTLINE_INSET.min(bounds[3].max(0.0));
        let tab_bar_h = Self::TAB_BAR_HEIGHT.min((bounds[3] - inset).max(0.0));
        if tab_bar_h < 1.0 {
            return bounds;
        }
        [
            bounds[0],
            bounds[1] + inset + tab_bar_h,
            bounds[2],
            (bounds[3] - inset - tab_bar_h).max(0.0),
        ]
    }

    /// Hit-test a point against the terminal tab bar laid out by
    /// [`Self::update_terminal_tab_bar`]. Returns the index of the tab under
    /// `pos`, or `None` when the point is outside the tab strip (or no strip is
    /// drawn because `tab_count <= 1`). The geometry here MUST stay in sync with
    /// `update_terminal_tab_bar` — both walk the same `tab_x`/`tab_w` sequence.
    pub fn terminal_tab_index_at(
        &self,
        tab_count: usize,
        bounds: [f32; 4],
        pos: (f32, f32),
    ) -> Option<usize> {
        if tab_count <= 1 {
            return None;
        }
        let outline_inset = Self::TAB_BAR_OUTLINE_INSET.min(bounds[3].max(0.0));
        let tab_bar_h = Self::TAB_BAR_HEIGHT.min((bounds[3] - outline_inset).max(0.0));
        if tab_bar_h < 1.0 {
            return None;
        }
        let tab_bar_y = bounds[1] + outline_inset;
        let (px, py) = pos;
        if py < tab_bar_y || py >= tab_bar_y + tab_bar_h {
            return None;
        }

        let mut tab_x = bounds[0] + Self::TAB_BAR_PADDING_X;
        let right_limit = bounds[0] + bounds[2] - Self::TAB_BAR_PADDING_X;
        let tab_width = ((right_limit - tab_x) / tab_count as f32).max(0.0);

        for i in 0..tab_count {
            let remaining_width = (right_limit - tab_x).max(0.0);
            let tabs_left = (tab_count - i) as f32;
            let tab_w = if i + 1 == tab_count {
                remaining_width
            } else {
                tab_width.min(remaining_width - (tabs_left - 1.0).max(0.0))
            };
            if tab_w <= 0.0 {
                break;
            }
            if px >= tab_x && px < tab_x + tab_w {
                return Some(i);
            }
            tab_x += tab_w;
        }
        None
    }

    pub fn update_terminal_tab_bar(
        &mut self,
        tab_labels: &[&str],
        tab_running: &[bool],
        active_tab: usize,
        bounds: [f32; 4],
    ) -> (Vec<RegionDrawInstance>, [f32; 4]) {
        let mut chrome = Vec::new();
        let tab_count = tab_labels.len();
        let outline_inset = Self::TAB_BAR_OUTLINE_INSET.min(bounds[3].max(0.0));
        let tab_bar_h = Self::TAB_BAR_HEIGHT.min((bounds[3] - outline_inset).max(0.0));
        self.terminal_tab_bar_batch = None;
        if tab_bar_h < 1.0 || tab_count <= 1 {
            self.terminal_text_pipeline.upload_instances(
                &self.device,
                &self.queue,
                &self.terminal_glyph_instances,
            );
            return (chrome, bounds);
        }

        let tab_bg = self.theme.ui.terminal_bg.as_f32();
        let active_bg = {
            let mut c = tab_bg;
            for i in 0..3 {
                c[i] = (c[i] + 0.055).min(1.0);
            }
            c
        };
        let inactive_bg = {
            let mut c = tab_bg;
            for i in 0..3 {
                c[i] = (c[i] + 0.018).min(1.0);
            }
            c[3] = 0.92;
            c
        };
        let accent = self.theme.ui.cyan.as_f32();
        let running_color = [0.32, 0.92, 0.52, 0.95_f32]; // green
        let dead_color = [0.45, 0.45, 0.45, 0.55_f32]; // gray
        let active_fg = self.theme.editor.fg.as_f32();
        let inactive_fg = self.theme.ui.fg_dim.as_f32();
        let font_size = self.theme.editor.font_size;
        let tab_bar_y = bounds[1] + outline_inset;
        let text_y = tab_bar_y + ((tab_bar_h - self.theme.editor.line_height) * 0.5).max(0.0);
        let label_x_offset = Self::TAB_BAR_DOT_GAP + Self::TAB_BAR_DOT_SIZE + 8.0;
        let label_right_padding = 16.0_f32;
        let body_count = self.terminal_glyph_instances.len() as u32;
        let tab_text_start = body_count;

        let tab_bar_bounds = [
            bounds[0] + Self::TAB_BAR_PADDING_X,
            tab_bar_y,
            (bounds[2] - Self::TAB_BAR_PADDING_X * 2.0).max(0.0),
            tab_bar_h,
        ];
        let terminal_bounds = self.terminal_tab_bar_content_bounds(bounds, tab_count);

        let radius = (self.panel_corner_radius - outline_inset)
            .min(tab_bar_h)
            .min(tab_bar_bounds[2] * 0.5)
            .max(0.0);

        // Tab bar background — top corners only
        chrome.push(
            RegionDrawInstance::new(tab_bar_bounds, tab_bg)
                .with_corner_radii([radius, radius, 0.0, 0.0]),
        );

        let show_tab_titles = tab_count >= 2;

        let mut tab_x = bounds[0] + Self::TAB_BAR_PADDING_X;
        let right_limit = bounds[0] + bounds[2] - Self::TAB_BAR_PADDING_X;

        let tab_width = if tab_count > 0 {
            ((right_limit - tab_x) / tab_count as f32).max(0.0)
        } else {
            0.0
        };

        for i in 0..tab_count {
            let remaining_width = (right_limit - tab_x).max(0.0);
            let tabs_left = (tab_count - i) as f32;
            let tab_w = if i + 1 == tab_count {
                remaining_width
            } else {
                tab_width.min(remaining_width - (tabs_left - 1.0).max(0.0))
            };
            if tab_w <= 0.0 {
                break;
            }

            let is_active = i == active_tab;
            let is_first = i == 0;
            let is_last = i + 1 == tab_count;
            let tab_corners = [
                if is_first { radius } else { 0.0 }, // top-left
                if is_last { radius } else { 0.0 },  // top-right
                0.0,
                0.0,
            ];

            // Tab background
            chrome.push(
                RegionDrawInstance::new(
                    [tab_x, tab_bar_y, tab_w, tab_bar_h],
                    if is_active { active_bg } else { inactive_bg },
                )
                .with_corner_radii(tab_corners),
            );
            if is_active {
                // Top accent border — inset on rounded corners
                let bar_x = if is_first { tab_x + radius } else { tab_x };
                let mut bar_w = tab_w;
                if is_first {
                    bar_w -= radius;
                }
                if is_last {
                    bar_w -= radius;
                }
                chrome.push(
                    RegionDrawInstance::new(
                        [bar_x, tab_bar_y, bar_w.max(0.0), Self::TAB_BAR_TOP_BORDER],
                        accent,
                    )
                    .with_corner_radii([
                        if is_first { radius } else { 0.0 },
                        if is_last { radius } else { 0.0 },
                        0.0,
                        0.0,
                    ]),
                );
            }

            // Status dot
            let running = tab_running.get(i).copied().unwrap_or(false);
            let dot_color = if running { running_color } else { dead_color };
            let dot_x = tab_x + Self::TAB_BAR_DOT_GAP;
            let dot_y = tab_bar_y + (tab_bar_h - Self::TAB_BAR_DOT_SIZE) * 0.5;
            chrome.push(RegionDrawInstance::new(
                [dot_x, dot_y, Self::TAB_BAR_DOT_SIZE, Self::TAB_BAR_DOT_SIZE],
                dot_color,
            ));

            // Text label
            if show_tab_titles {
                let label_x = tab_x + label_x_offset;
                let label_max_w = (tab_w - label_x_offset - label_right_padding).max(0.0);
                let label = clamp_monospace_text(tab_labels[i], label_max_w, font_size);
                if !label.is_empty() {
                    self.terminal_text_system
                        .set_size(Some(label_max_w), Some(tab_bar_h));
                    self.terminal_glyph_instances.extend(layout_panel_text(
                        &label,
                        &mut self.terminal_text_system,
                        &mut self.atlas,
                        &self.queue,
                        label_x,
                        text_y,
                        if is_active { active_fg } else { inactive_fg },
                    ));
                }
            }

            // Separator between tabs
            if i + 1 < tab_count {
                let sep_x = tab_x + tab_w - 0.5;
                chrome.push(RegionDrawInstance::new(
                    [sep_x, tab_bar_y + 5.0, 1.0, (tab_bar_h - 10.0).max(0.0)],
                    [0.2, 0.2, 0.25, 0.3],
                ));
            }

            tab_x += tab_w;
        }

        // Bottom divider between tab strip and terminal body.
        chrome.push(RegionDrawInstance::new(
            [
                bounds[0] + Self::TAB_BAR_PADDING_X,
                tab_bar_y + tab_bar_h - 1.0,
                (bounds[2] - Self::TAB_BAR_PADDING_X * 2.0).max(0.0),
                1.0,
            ],
            [0.16, 0.18, 0.24, 0.75],
        ));

        let tab_text_count = self
            .terminal_glyph_instances
            .len()
            .saturating_sub(tab_text_start as usize) as u32;
        self.terminal_tab_bar_batch =
            rect_to_scissor(tab_bar_bounds).map(|scissor| TextScissorBatch {
                scissor,
                range: InstanceDrawRange {
                    start: tab_text_start,
                    count: tab_text_count,
                },
            });
        self.terminal_text_pipeline.upload_instances(
            &self.device,
            &self.queue,
            &self.terminal_glyph_instances,
        );

        (chrome, terminal_bounds)
    }

    // ── Welcome logo ───────────────────────────────────────────────────────────
}
