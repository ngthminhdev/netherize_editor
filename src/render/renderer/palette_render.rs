//! Overlay rendering: Command Palette (minimalist), File Picker (complex), Leap labels.

use crate::{
    app::{app_state::AppState, command_palette::CommandPaletteRenderModel},
    render::{
        glyph_instance::GlyphInstance, region_pipeline::RegionDrawInstance, renderer::Renderer,
    },
};

use super::helpers::{
    clamp_monospace_text, ext_icon_dot, gutter_width_for_editor, layout_panel_text,
    layout_panel_text_bold, rect_to_scissor,
};

impl Renderer {
    // ── Public entry point ─────────────────────────────────────────────────────

    /// Layout + upload command palette overlay (chrome + text).
    ///
    /// The palette model is precomputed by `CommandPalette::render()` so the
    /// renderer only consumes geometry/text and uploads GPU instances.
    pub fn update_palette_content(&mut self, model: &CommandPaletteRenderModel) {
        if self.last_palette_model.as_ref() == Some(model) {
            return;
        }
        if model.mode == crate::app::command_palette::CommandPaletteMode::RecentProjects {
            self.render_recent_projects(model);
        } else if model.mode == crate::app::command_palette::CommandPaletteMode::LiveGrep {
            self.render_live_grep_picker(model);
        } else if model.mode.is_complex_picker() {
            self.render_file_picker_complex(model);
        } else {
            self.render_command_palette_minimalist(model);
        }
        self.palette_text_pipeline.upload_instances(
            &self.device,
            &self.queue,
            &self.palette_glyph_instances,
        );
        self.last_palette_model = Some(model.clone());
    }

    pub fn clear_palette(&mut self) {
        self.palette_scissor = None;
        self.palette_chrome_instances.clear();
        self.palette_glyph_instances.clear();
        self.last_palette_model = None;
        self.palette_text_pipeline
            .upload_instances(&self.device, &self.queue, &[]);
    }

    // ── Minimalist palette (Command / Symbol / VimCommand) ─────────────────────

    /// Single-line input box, TRUE center of screen.
    /// Results rendered below prompt with subtle row separators.
    /// VimCommand mode: prompt only, no results, no separator.
    fn render_command_palette_minimalist(&mut self, model: &CommandPaletteRenderModel) {
        let [panel_x, panel_y, panel_w, panel_h] = model.panel_bounds;
        self.palette_scissor = rect_to_scissor(model.panel_bounds);

        let inner_width = (panel_w - model.panel_padding * 2.0).max(1.0);
        self.palette_text_system
            .set_size(Some(inner_width), Some(model.line_height));

        let mut quads: Vec<RegionDrawInstance> = Vec::new();
        let mut glyphs: Vec<GlyphInstance> = Vec::new();

        let font_size = self.theme.ui.sidebar_font_size;
        let text_x = panel_x + model.panel_padding + 8.0; // 8px left indent
        let line_h = model.line_height.max(18.0);
        let mut row_top = panel_y + model.panel_padding;

        // Chrome: scrim → border → panel bg
        quads.push(RegionDrawInstance::new(
            model.overlay_bounds,
            model.scrim_color,
        ));
        quads.push(RegionDrawInstance::new(
            [panel_x - 1.0, panel_y - 1.0, panel_w + 2.0, panel_h + 2.0],
            model.border_color,
        ));
        quads.push(RegionDrawInstance::new(model.panel_bounds, model.panel_bg));

        // Prompt line
        let prefix_w = model.prompt_prefix.chars().count() as f32 * font_size * 0.60;
        let query_color =
            if !model.show_results || model.result_match_ranges.iter().any(|r| !r.is_empty()) {
                model.text_color
            } else {
                model.hint_color
            };
        glyphs.extend(layout_panel_text(
            &model.prompt_prefix,
            &mut self.palette_text_system,
            &mut self.atlas,
            &self.queue,
            text_x,
            row_top,
            model.hint_color,
        ));
        glyphs.extend(layout_panel_text(
            &model.prompt_query,
            &mut self.palette_text_system,
            &mut self.atlas,
            &self.queue,
            text_x + prefix_w,
            row_top,
            query_color,
        ));
        row_top += line_h;

        // VimCommand: only prompt — exit early
        if !model.show_results {
            self.palette_chrome_instances = quads;
            self.palette_glyph_instances = glyphs;
            return;
        }

        // Separator beneath prompt
        quads.push(RegionDrawInstance::new(
            [
                panel_x + model.panel_padding,
                row_top + 2.0,
                inner_width - model.panel_padding,
                1.0,
            ],
            model.border_color,
        ));
        row_top += 8.0;

        // Result rows
        let row_v_pad = 4.0;
        let row_h = line_h + row_v_pad * 2.0;
        let max_visible = (((panel_h - model.panel_padding * 2.0 - line_h - 8.0) / row_h).floor()
            as usize)
            .max(1);
        let scroll_offset = model
            .scroll_offset_rows
            .min(model.result_labels.len().saturating_sub(max_visible));

        for (visible_idx, (label, ranges)) in model
            .result_labels
            .iter()
            .zip(model.result_match_ranges.iter())
            .skip(scroll_offset)
            .take(max_visible)
            .enumerate()
        {
            let absolute_idx = scroll_offset + visible_idx;

            if absolute_idx == model.selected_index {
                quads.push(RegionDrawInstance::new(
                    [panel_x + 2.0, row_top, (panel_w - 4.0).max(0.0), row_h],
                    model.selection_bg,
                ));
            }
            if visible_idx > 0 {
                let mut sep = model.border_color;
                sep[3] *= 0.35;
                quads.push(RegionDrawInstance::new(
                    [text_x, row_top, inner_width - 8.0, 1.0],
                    sep,
                ));
            }

            let label_y = row_top + row_v_pad;
            Self::render_highlighted_label(
                label,
                ranges,
                text_x,
                label_y,
                font_size,
                model,
                &mut self.palette_text_system,
                &mut self.atlas,
                &self.queue,
                &mut glyphs,
            );
            row_top += row_h;
        }

        self.palette_chrome_instances = quads;
        self.palette_glyph_instances = glyphs;
    }

    // ── File Picker (complex) ──────────────────────────────────────────────────

    /// Live Grep overlay: larger panel + two-line rows so the snippet preview is readable.
    fn render_live_grep_picker(&mut self, model: &CommandPaletteRenderModel) {
        let [panel_x, panel_y, panel_w, panel_h] = model.panel_bounds;
        self.palette_scissor = rect_to_scissor(model.panel_bounds);

        let inner_width = (panel_w - model.panel_padding * 2.0).max(1.0);
        self.palette_text_system
            .set_size(Some(inner_width), Some(model.line_height));

        let mut quads: Vec<RegionDrawInstance> = Vec::new();
        let mut glyphs: Vec<GlyphInstance> = Vec::new();

        let font_size = self.theme.ui.sidebar_font_size;
        let line_h = model.line_height.max(18.0);
        let row_v_pad = 4.0;
        let row_h = line_h * 2.0 + 16.0;
        let text_x = panel_x + model.panel_padding + 8.0;

        let char_w = font_size * 0.62;
        let dot_char_w = char_w + 2.0;
        let icon_col_w = dot_char_w + 4.0 + 4.0 * char_w + 24.0;
        let name_x = text_x + icon_col_w;

        quads.push(RegionDrawInstance::new(
            model.overlay_bounds,
            model.scrim_color,
        ));
        quads.push(RegionDrawInstance::new(
            [panel_x - 1.0, panel_y - 1.0, panel_w + 2.0, panel_h + 2.0],
            model.border_color,
        ));
        quads.push(RegionDrawInstance::new(model.panel_bounds, model.panel_bg));

        let mut row_top = panel_y + model.panel_padding;

        let badge_text = format!(" {} ", model.title);
        let badge_w = badge_text.chars().count() as f32 * font_size * 0.60 + 4.0;
        quads.push(RegionDrawInstance::new(
            [text_x, row_top + 2.0, badge_w, line_h - 4.0],
            model.success_color,
        ));
        glyphs.extend(layout_panel_text(
            &badge_text,
            &mut self.palette_text_system,
            &mut self.atlas,
            &self.queue,
            text_x + 2.0,
            row_top,
            self.theme.ui.bg.as_f32(),
        ));

        let query_x = text_x + badge_w + 10.0;
        let prefix_w = model.prompt_prefix.chars().count() as f32 * font_size * 0.60;
        glyphs.extend(layout_panel_text(
            &model.prompt_prefix,
            &mut self.palette_text_system,
            &mut self.atlas,
            &self.queue,
            query_x,
            row_top,
            model.hint_color,
        ));
        glyphs.extend(layout_panel_text(
            &model.prompt_query,
            &mut self.palette_text_system,
            &mut self.atlas,
            &self.queue,
            query_x + prefix_w,
            row_top,
            model.text_color,
        ));

        let count_text = format!("{}/{}", model.result_labels.len(), model.total_results);
        let count_w = count_text.chars().count() as f32 * font_size * 0.60;
        let count_x = (panel_x + panel_w - model.panel_padding - count_w).max(query_x);
        glyphs.extend(layout_panel_text(
            &count_text,
            &mut self.palette_text_system,
            &mut self.atlas,
            &self.queue,
            count_x,
            row_top,
            model.hint_color,
        ));
        row_top += line_h;

        quads.push(RegionDrawInstance::new(
            [panel_x, row_top, panel_w, 1.0],
            model.border_color,
        ));
        row_top += 6.0;

        let footer_h = line_h + 10.0 + 1.0;
        let body_h = (panel_h - (row_top - panel_y) - footer_h - model.panel_padding).max(row_h);
        let max_visible = (body_h / row_h).floor() as usize;
        let scroll_offset = model
            .scroll_offset_rows
            .min(model.result_labels.len().saturating_sub(max_visible));

        let empty_str = String::new();
        for (visible_idx, header) in model
            .result_labels
            .iter()
            .skip(scroll_offset)
            .take(max_visible)
            .enumerate()
        {
            let absolute_idx = scroll_offset + visible_idx;
            let preview = model
                .secondary_labels
                .get(absolute_idx)
                .unwrap_or(&empty_str);
            let ranges = model
                .result_match_ranges
                .get(absolute_idx)
                .map(Vec::as_slice)
                .unwrap_or(&[]);

            if absolute_idx == model.selected_index {
                quads.push(RegionDrawInstance::new(
                    [panel_x + 2.0, row_top, (panel_w - 4.0).max(0.0), row_h],
                    model.selection_bg,
                ));
            }
            if visible_idx > 0 {
                let mut sep = model.border_color;
                sep[3] *= 0.30;
                quads.push(RegionDrawInstance::new(
                    [text_x, row_top, inner_width - 8.0, 1.0],
                    sep,
                ));
            }

            let file_path = header.split(':').next().unwrap_or(header);
            let ext = std::path::Path::new(file_path)
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("");
            let (dot_color, ext_label) = ext_icon_dot(ext, &self.theme);
            let picker_dot = self.theme.icons.file_picker_dot.as_str();

            let header_y = row_top + row_v_pad;
            let preview_y = header_y + line_h + 2.0;

            glyphs.extend(layout_panel_text(
                picker_dot,
                &mut self.palette_text_system,
                &mut self.atlas,
                &self.queue,
                text_x,
                header_y,
                dot_color,
            ));
            let mut ext_color = dot_color;
            ext_color[3] *= 0.75;
            glyphs.extend(layout_panel_text(
                ext_label,
                &mut self.palette_text_system,
                &mut self.atlas,
                &self.queue,
                text_x + dot_char_w + 4.0,
                header_y,
                ext_color,
            ));

            let available_w = (panel_x + panel_w - model.panel_padding - 4.0 - name_x).max(0.0);
            let clamped_header = clamp_monospace_text(header, available_w, font_size);
            let mut header_color = model.hint_color;
            header_color[3] = if absolute_idx == model.selected_index {
                model.text_color[3]
            } else {
                (header_color[3] * 0.95).clamp(0.45, 0.90)
            };
            glyphs.extend(layout_panel_text(
                &clamped_header,
                &mut self.palette_text_system,
                &mut self.atlas,
                &self.queue,
                name_x,
                header_y,
                header_color,
            ));

            if !preview.is_empty() {
                Self::render_highlighted_label(
                    preview,
                    ranges,
                    name_x,
                    preview_y,
                    font_size,
                    model,
                    &mut self.palette_text_system,
                    &mut self.atlas,
                    &self.queue,
                    &mut glyphs,
                );
            }

            row_top += row_h;
        }

        if model.result_labels.is_empty() {
            glyphs.extend(layout_panel_text(
                "  (no results — type to grep workspace)",
                &mut self.palette_text_system,
                &mut self.atlas,
                &self.queue,
                text_x,
                row_top + row_v_pad,
                model.hint_color,
            ));
        }

        let footer_y = panel_y + panel_h - footer_h;
        quads.push(RegionDrawInstance::new(
            [panel_x, footer_y, panel_w, 1.0],
            model.border_color,
        ));
        glyphs.extend(layout_panel_text(
            "  ↑↓ navigate   ↵ open match   esc close",
            &mut self.palette_text_system,
            &mut self.atlas,
            &self.queue,
            text_x,
            footer_y + 5.0,
            model.hint_color,
        ));

        self.palette_chrome_instances = quads;
        self.palette_glyph_instances = glyphs;
    }

    /// Header badge + result count + icon column + footer shortcuts.
    fn render_file_picker_complex(&mut self, model: &CommandPaletteRenderModel) {
        let [panel_x, panel_y, panel_w, panel_h] = model.panel_bounds;
        self.palette_scissor = rect_to_scissor(model.panel_bounds);

        let inner_width = (panel_w - model.panel_padding * 2.0).max(1.0);
        self.palette_text_system
            .set_size(Some(inner_width), Some(model.line_height));

        let mut quads: Vec<RegionDrawInstance> = Vec::new();
        let mut glyphs: Vec<GlyphInstance> = Vec::new();

        let font_size = self.theme.ui.sidebar_font_size;
        let line_h = model.line_height.max(18.0);
        let row_v_pad = 4.0;
        let row_h = line_h + row_v_pad * 2.0;
        let text_x = panel_x + model.panel_padding + 8.0;

        // Icon column: sized to always fit ● + longest ext ("toml", 4 chars)
        let char_w = font_size * 0.62;
        let dot_char_w = char_w + 2.0;
        let icon_col_w = dot_char_w + 4.0 + 4.0 * char_w + 24.0;
        let name_x = text_x + icon_col_w;

        // Chrome
        quads.push(RegionDrawInstance::new(
            model.overlay_bounds,
            model.scrim_color,
        ));
        quads.push(RegionDrawInstance::new(
            [panel_x - 1.0, panel_y - 1.0, panel_w + 2.0, panel_h + 2.0],
            model.border_color,
        ));
        quads.push(RegionDrawInstance::new(model.panel_bounds, model.panel_bg));

        // ── HEADER ─────────────────────────────────────────────────────────────
        let mut row_top = panel_y + model.panel_padding;

        let badge_text = format!(" {} ", model.title);
        let badge_w = badge_text.chars().count() as f32 * font_size * 0.60 + 4.0;
        quads.push(RegionDrawInstance::new(
            [text_x, row_top + 2.0, badge_w, line_h - 4.0],
            model.success_color,
        ));
        glyphs.extend(layout_panel_text(
            &badge_text,
            &mut self.palette_text_system,
            &mut self.atlas,
            &self.queue,
            text_x + 2.0,
            row_top,
            self.theme.ui.bg.as_f32(),
        ));

        let query_x = text_x + badge_w + 10.0;
        let query_col = if model.result_match_ranges.iter().any(|r| !r.is_empty()) {
            model.text_color
        } else {
            model.hint_color
        };
        let prefix_w = model.prompt_prefix.chars().count() as f32 * font_size * 0.60;
        glyphs.extend(layout_panel_text(
            &model.prompt_prefix,
            &mut self.palette_text_system,
            &mut self.atlas,
            &self.queue,
            query_x,
            row_top,
            model.hint_color,
        ));
        glyphs.extend(layout_panel_text(
            &model.prompt_query,
            &mut self.palette_text_system,
            &mut self.atlas,
            &self.queue,
            query_x + prefix_w,
            row_top,
            query_col,
        ));

        let count_text = format!("{}/{}", model.result_labels.len(), model.total_results);
        let count_w = count_text.chars().count() as f32 * font_size * 0.60;
        let count_x = (panel_x + panel_w - model.panel_padding - count_w).max(query_x);
        glyphs.extend(layout_panel_text(
            &count_text,
            &mut self.palette_text_system,
            &mut self.atlas,
            &self.queue,
            count_x,
            row_top,
            model.hint_color,
        ));
        row_top += line_h;

        quads.push(RegionDrawInstance::new(
            [panel_x, row_top, panel_w, 1.0],
            model.border_color,
        ));
        row_top += 6.0;

        // ── BODY ───────────────────────────────────────────────────────────────
        let footer_h = line_h + 10.0 + 1.0;
        let body_h = (panel_h - (row_top - panel_y) - footer_h - model.panel_padding).max(row_h);
        let max_visible = (body_h / row_h).floor() as usize;
        let scroll_offset = model
            .scroll_offset_rows
            .min(model.result_labels.len().saturating_sub(max_visible));

        for (visible_idx, (label, ranges)) in model
            .result_labels
            .iter()
            .zip(model.result_match_ranges.iter())
            .skip(scroll_offset)
            .take(max_visible)
            .enumerate()
        {
            let absolute_idx = scroll_offset + visible_idx;

            if absolute_idx == model.selected_index {
                quads.push(RegionDrawInstance::new(
                    [panel_x + 2.0, row_top, (panel_w - 4.0).max(0.0), row_h],
                    model.selection_bg,
                ));
            }
            if visible_idx > 0 {
                let mut sep = model.border_color;
                sep[3] *= 0.30;
                quads.push(RegionDrawInstance::new(
                    [text_x, row_top, inner_width - 8.0, 1.0],
                    sep,
                ));
            }

            let icon_y = row_top + row_v_pad;
            let file_path = label.split(':').next().unwrap_or(label);
            let ext = std::path::Path::new(file_path)
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("");
            let (dot_color, ext_label) = ext_icon_dot(ext, &self.theme);
            let picker_dot = self.theme.icons.file_picker_dot.as_str();

            // Dot icon
            glyphs.extend(layout_panel_text(
                picker_dot,
                &mut self.palette_text_system,
                &mut self.atlas,
                &self.queue,
                text_x,
                icon_y,
                dot_color,
            ));
            // ext label (75% alpha of dot color)
            let mut ext_color = dot_color;
            ext_color[3] *= 0.75;
            glyphs.extend(layout_panel_text(
                ext_label,
                &mut self.palette_text_system,
                &mut self.atlas,
                &self.queue,
                text_x + dot_char_w + 4.0,
                icon_y,
                ext_color,
            ));

            // filename — always starts at fixed name_x column
            Self::render_highlighted_label(
                label,
                ranges,
                name_x,
                icon_y,
                font_size,
                model,
                &mut self.palette_text_system,
                &mut self.atlas,
                &self.queue,
                &mut glyphs,
            );
            row_top += row_h;
        }

        if model.result_labels.is_empty() {
            glyphs.extend(layout_panel_text(
                "  (no results — type to search)",
                &mut self.palette_text_system,
                &mut self.atlas,
                &self.queue,
                text_x,
                row_top + row_v_pad,
                model.hint_color,
            ));
        }

        // ── FOOTER ─────────────────────────────────────────────────────────────
        let footer_y = panel_y + panel_h - footer_h;
        quads.push(RegionDrawInstance::new(
            [panel_x, footer_y, panel_w, 1.0],
            model.border_color,
        ));
        glyphs.extend(layout_panel_text(
            "  ↑↓ navigate   ↵ open   esc close",
            &mut self.palette_text_system,
            &mut self.atlas,
            &self.queue,
            text_x,
            footer_y + 5.0,
            model.hint_color,
        ));

        self.palette_chrome_instances = quads;
        self.palette_glyph_instances = glyphs;
    }

    // ── Recent Projects picker (2-column: name | path) ────────────────────────

    fn render_recent_projects(&mut self, model: &CommandPaletteRenderModel) {
        let [panel_x, panel_y, panel_w, panel_h] = model.panel_bounds;
        self.palette_scissor = rect_to_scissor(model.panel_bounds);

        let inner_width = (panel_w - model.panel_padding * 2.0).max(1.0);
        self.palette_text_system
            .set_size(Some(inner_width), Some(model.line_height));

        let mut quads: Vec<RegionDrawInstance> = Vec::new();
        let mut glyphs: Vec<GlyphInstance> = Vec::new();

        let font_size = self.theme.ui.sidebar_font_size;
        let char_w = font_size * 0.62;
        let line_h = model.line_height.max(18.0);
        let row_v_pad = 4.0;
        let row_h = line_h + row_v_pad * 2.0;
        let text_x = panel_x + model.panel_padding + 8.0;

        // name column = 38% of inner_width, path column = rest
        let name_col_w = (inner_width * 0.38).max(120.0);
        let path_x = text_x + name_col_w + char_w * 2.0;

        // Chrome
        quads.push(RegionDrawInstance::new(
            model.overlay_bounds,
            model.scrim_color,
        ));
        quads.push(RegionDrawInstance::new(
            [panel_x - 1.0, panel_y - 1.0, panel_w + 2.0, panel_h + 2.0],
            model.border_color,
        ));
        quads.push(RegionDrawInstance::new(model.panel_bounds, model.panel_bg));

        // ── Header ─────────────────────────────────────────────────────────────
        let mut row_top = panel_y + model.panel_padding;

        let badge_text = format!(" {} ", model.title);
        let badge_w = badge_text.chars().count() as f32 * font_size * 0.60 + 4.0;
        quads.push(RegionDrawInstance::new(
            [text_x, row_top + 2.0, badge_w, line_h - 4.0],
            model.info_color,
        ));
        glyphs.extend(layout_panel_text(
            &badge_text,
            &mut self.palette_text_system,
            &mut self.atlas,
            &self.queue,
            text_x + 2.0,
            row_top,
            self.theme.ui.bg.as_f32(),
        ));

        let count_text = format!("{}", model.result_labels.len());
        let count_w = count_text.chars().count() as f32 * font_size * 0.60;
        let count_x =
            (panel_x + panel_w - model.panel_padding - count_w).max(text_x + badge_w + 8.0);
        glyphs.extend(layout_panel_text(
            &count_text,
            &mut self.palette_text_system,
            &mut self.atlas,
            &self.queue,
            count_x,
            row_top,
            model.hint_color,
        ));
        row_top += line_h;

        quads.push(RegionDrawInstance::new(
            [panel_x, row_top, panel_w, 1.0],
            model.border_color,
        ));
        row_top += 6.0;

        // ── Column header labels ────────────────────────────────────────────────
        let mut col_header_color = model.hint_color;
        col_header_color[3] *= 0.7;
        glyphs.extend(layout_panel_text(
            "NAME",
            &mut self.palette_text_system,
            &mut self.atlas,
            &self.queue,
            text_x,
            row_top,
            col_header_color,
        ));
        glyphs.extend(layout_panel_text(
            "PATH",
            &mut self.palette_text_system,
            &mut self.atlas,
            &self.queue,
            path_x,
            row_top,
            col_header_color,
        ));
        row_top += line_h;

        quads.push(RegionDrawInstance::new(
            [panel_x, row_top, panel_w, 1.0],
            model.border_color,
        ));
        row_top += 4.0;

        // ── Body rows ──────────────────────────────────────────────────────────
        let footer_h = line_h + 10.0 + 1.0;
        let body_h = (panel_h - (row_top - panel_y) - footer_h - model.panel_padding).max(row_h);
        let max_visible = (body_h / row_h).floor() as usize;
        let scroll_offset = model
            .scroll_offset_rows
            .min(model.result_labels.len().saturating_sub(max_visible));

        let empty_str = String::new();
        for (visible_idx, (name, ranges)) in model
            .result_labels
            .iter()
            .zip(model.result_match_ranges.iter())
            .skip(scroll_offset)
            .take(max_visible)
            .enumerate()
        {
            let absolute_idx = scroll_offset + visible_idx;
            let full_path = model
                .secondary_labels
                .get(absolute_idx)
                .unwrap_or(&empty_str);

            if absolute_idx == model.selected_index {
                quads.push(RegionDrawInstance::new(
                    [panel_x + 2.0, row_top, (panel_w - 4.0).max(0.0), row_h],
                    model.selection_bg,
                ));
            }
            if visible_idx > 0 {
                let mut sep = model.border_color;
                sep[3] *= 0.30;
                quads.push(RegionDrawInstance::new(
                    [text_x, row_top, inner_width - 8.0, 1.0],
                    sep,
                ));
            }

            let label_y = row_top + row_v_pad;

            // Col 1: folder name (highlighted if matched)
            let name_color = if absolute_idx == model.selected_index {
                model.text_color
            } else {
                model.text_color
            };
            Self::render_highlighted_label(
                name,
                ranges,
                text_x,
                label_y,
                font_size,
                model,
                &mut self.palette_text_system,
                &mut self.atlas,
                &self.queue,
                &mut glyphs,
            );
            let _ = name_color; // used implicitly via render_highlighted_label

            // Col 2: full path (always hint_color/dim)
            let available_w = (panel_x + panel_w - model.panel_padding - 4.0 - path_x).max(0.0);
            let clamped_path = clamp_monospace_text(full_path, available_w, font_size);
            glyphs.extend(layout_panel_text(
                &clamped_path,
                &mut self.palette_text_system,
                &mut self.atlas,
                &self.queue,
                path_x,
                label_y,
                model.hint_color,
            ));

            row_top += row_h;
        }

        if model.result_labels.is_empty() {
            glyphs.extend(layout_panel_text(
                "  (no recent projects — open a folder with Cmd+O)",
                &mut self.palette_text_system,
                &mut self.atlas,
                &self.queue,
                text_x,
                row_top + row_v_pad,
                model.hint_color,
            ));
        }

        // ── Footer ─────────────────────────────────────────────────────────────
        let footer_y = panel_y + panel_h - footer_h;
        quads.push(RegionDrawInstance::new(
            [panel_x, footer_y, panel_w, 1.0],
            model.border_color,
        ));
        glyphs.extend(layout_panel_text(
            "  ↑↓ navigate   ↵ open   esc close",
            &mut self.palette_text_system,
            &mut self.atlas,
            &self.queue,
            text_x,
            footer_y + 5.0,
            model.hint_color,
        ));

        self.palette_chrome_instances = quads;
        self.palette_glyph_instances = glyphs;
    }

    // ── Shared label renderer ──────────────────────────────────────────────────

    /// Render a label string with fuzzy-match character highlights.
    #[allow(clippy::too_many_arguments)]
    fn render_highlighted_label(
        label: &str,
        ranges: &[(usize, usize)],
        start_x: f32,
        y: f32,
        font_size: f32,
        model: &CommandPaletteRenderModel,
        text_system: &mut crate::text::text_system::TextSystem,
        atlas: &mut crate::text::atlas::GlyphAtlas,
        queue: &wgpu::Queue,
        glyphs: &mut Vec<GlyphInstance>,
    ) {
        let mut seg_x = start_x;
        let mut cursor = 0usize;
        let mut segs: Vec<(&str, [f32; 4])> = Vec::new();

        for &(start, end) in ranges {
            if start > cursor {
                segs.push((&label[cursor..start], model.label_color));
            }
            if end <= label.len() {
                segs.push((&label[start..end], model.match_color));
            }
            cursor = end;
        }
        if cursor < label.len() {
            segs.push((&label[cursor..], model.label_color));
        }
        if segs.is_empty() {
            segs.push((label, model.label_color));
        }

        for (seg_text, seg_color) in &segs {
            glyphs.extend(layout_panel_text(
                seg_text,
                text_system,
                atlas,
                queue,
                seg_x,
                y,
                *seg_color,
            ));
            seg_x += seg_text.chars().count() as f32 * font_size * 0.60;
        }
    }

    // ── Leap label overlay ─────────────────────────────────────────────────────

    /// Draw cyan label chars + amber-bg badges over each Leap target glyph.
    ///
    /// `labels`: (label_char, char_idx_in_rope) pairs from `generate_leap_labels`.
    pub fn update_leap_labels(
        &mut self,
        labels: &[(char, usize)],
        app_state: &AppState,
        center_bounds: [f32; 4],
    ) {
        self.leap_label_scissor = rect_to_scissor(center_bounds);

        if labels.is_empty() {
            self.leap_label_bg_instances.clear();
            self.leap_label_glyph_instances.clear();
            self.leap_label_text_pipeline
                .upload_instances(&self.device, &self.queue, &[]);
            return;
        }

        let line_height = self.theme.editor.line_height;
        let font_size = self.theme.editor.font_size;
        let total_lines = app_state.total_lines().max(1);
        let scroll_y = app_state.scroll_line as f32 * line_height;
        let gutter_digits = total_lines.to_string().len().max(3);
        let gutter_width = gutter_width_for_editor(gutter_digits, font_size, line_height);
        let origin_x = center_bounds[0] + self.editor_padding_x + gutter_width;
        let origin_y = center_bounds[1] + self.editor_padding_y + line_height - scroll_y;

        let label_color = self.theme.ui.cyan.as_f32();
        let char_bg_color = self.theme.ui.panel_bg.as_f32();
        let mut overlay_color = self.theme.ui.overlay_bg.as_f32();
        overlay_color[3] = overlay_color[3].max(0.50);

        let label_map: std::collections::HashMap<usize, char> =
            labels.iter().map(|(lc, ci)| (*ci, *lc)).collect();

        // Measure the baseline shift of the 2x font used for label chars.
        let color_u8 = (label_color[0] * 255.0) as u8;
        let dummy = [color_u8, color_u8, color_u8, 255u8];
        self.leap_label_text_system.set_text_bold_color("a", dummy);
        let label_line_y = self
            .leap_label_text_system
            .buffer()
            .layout_runs()
            .next()
            .map(|r| r.line_y)
            .unwrap_or(0.0);

        let mut bg_per_char: Vec<RegionDrawInstance> = Vec::with_capacity(labels.len());
        let mut glyph_instances: Vec<GlyphInstance> = Vec::with_capacity(labels.len());

        for run in self.text_system.buffer().layout_runs() {
            for glyph in run.glyphs {
                let rope_char_idx = app_state
                    .char_idx_for_line(run.line_i)
                    .saturating_add(app_state.byte_to_char_in_line(run.line_i, glyph.start));
                let Some(&label_char) = label_map.get(&rope_char_idx) else {
                    continue;
                };

                let glyph_x = origin_x + glyph.x;
                let glyph_top = origin_y + run.line_top;
                let cell_w = glyph.w.max(font_size * 0.5);
                let cell_h = run.line_height.max(1.0);

                // 1. Background quad overwrites original char
                bg_per_char.push(RegionDrawInstance::new(
                    [glyph_x, glyph_top, cell_w, cell_h],
                    char_bg_color,
                ));

                // 2. Label char — aligned to editor baseline
                let baseline_y = origin_y + run.line_y;
                let label_origin_y = baseline_y - label_line_y;
                glyph_instances.extend(layout_panel_text_bold(
                    &label_char.to_string(),
                    &mut self.leap_label_text_system,
                    &mut self.atlas,
                    &self.queue,
                    glyph_x,
                    label_origin_y,
                    label_color,
                ));
            }
        }

        // Dim overlay (whole editor area) + per-char backgrounds
        let mut all_bg = vec![RegionDrawInstance::new(
            [
                center_bounds[0],
                center_bounds[1],
                center_bounds[2],
                center_bounds[3],
            ],
            overlay_color,
        )];
        all_bg.extend(bg_per_char);
        self.leap_label_bg_instances = all_bg;
        self.leap_label_glyph_instances = glyph_instances;
        self.leap_label_text_pipeline.upload_instances(
            &self.device,
            &self.queue,
            &self.leap_label_glyph_instances,
        );
    }

    /// Clear Leap overlay — called on LeapCancel or after a jump completes.
    pub fn clear_leap_labels(&mut self) {
        self.leap_label_scissor = None;
        self.leap_label_bg_instances.clear();
        self.leap_label_glyph_instances.clear();
        self.leap_label_text_pipeline
            .upload_instances(&self.device, &self.queue, &[]);
    }
}
