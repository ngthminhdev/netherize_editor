use crate::{
    core::mode::EditorMode,
    render::{region_pipeline::RegionDrawInstance, renderer::Renderer},
    terminal::grid::TerminalGrid,
};

use super::super::helpers::{layout_panel_text, rect_to_scissor};

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
        let panel_padding = self.panel_padding;
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
            crate::render::region_pipeline::RegionDrawInstance::new(bounds, terminal_bg_color)
                .with_radius(self.panel_corner_radius);

        let origin_x = bounds[0] + panel_padding + TERMINAL_SAFE_INSET_X;
        let origin_y = bounds[1] + panel_padding;
        let width = (bounds[2] - panel_padding * 2.0 - TERMINAL_SAFE_INSET_X * 2.0).max(1.0);

        self.buffer_terminal_scissor = rect_to_scissor(inset_bounds(bounds, 1.0, 1.0));

        let default_fg = self.theme.editor.fg.as_f32();
        let default_bg = terminal_bg_color;

        self.buffer_terminal_header_batch = None;

        if grid.used_rows() == 0 {
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
        let origin_x = bounds[0] + panel_padding + TERMINAL_SAFE_INSET_X;
        let origin_y = bounds[1] + panel_padding;
        let width = (bounds[2] - panel_padding * 2.0 - TERMINAL_SAFE_INSET_X * 2.0).max(1.0);
        let height = (bounds[3] - panel_padding * 2.0).max(1.0);

        *scissor = rect_to_scissor(inset_bounds(bounds, 1.0, 1.0));

        let default_fg = theme.editor.fg.as_f32();
        let default_bg = theme.ui.terminal_bg.as_f32();

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
                let width = (end_col_exclusive.saturating_sub(start_col) as f32)
                    * view_renderer.cell_width;
                cursor_instances.push(RegionDrawInstance::new(
                    [x, y, width.min((clip_right - x).max(0.0)), h.max(1.0)],
                    search_color,
                ));
            }
        }

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
}
