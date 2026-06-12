//! Which-key overlay: while a chord is pending (e.g. Space …), show every
//! possible next key with a short description, anchored above the status bar.

use crate::{
    app::input_map::WhichKeyEntry,
    render::{
        glyph_instance::GlyphInstance, region_pipeline::RegionDrawInstance, renderer::Renderer,
    },
};

use super::super::{
    components::{estimate_help_keycaps_width, layout_help_keycaps},
    helpers::{estimate_monospace_width, layout_panel_text, rect_to_scissor},
};

const MAX_ROWS: usize = 8;

impl Renderer {
    /// Rebuild the which-key panel. `statusbar_h` anchors the panel right above
    /// the status bar; pass 0.0 when no status bar is visible.
    pub fn update_whichkey_popup(
        &mut self,
        prefix_label: &str,
        entries: &[WhichKeyEntry],
        window_w: f32,
        window_h: f32,
        statusbar_h: f32,
    ) {
        if entries.is_empty() || window_w <= 1.0 || window_h <= 1.0 {
            self.clear_whichkey_popup();
            return;
        }

        let scale = self.ui_scale.max(0.5);
        let margin_x = 14.0 * scale;
        let pad_x = 16.0 * scale;
        let pad_y = 10.0 * scale;
        let font_size = self.theme.ui.sidebar_font_size.max(11.0);
        let line_h = self.theme.ui.sidebar_line_height.max(font_size + 4.0);
        let row_h = line_h + 10.0 * scale;
        let key_label_gap = (font_size * 0.5).max(6.0 * scale);
        let col_gap = (font_size * 1.6).max(18.0 * scale);

        let fg = self.theme.ui.fg.as_f32();
        let fg_dim = self.theme.ui.fg_dim.as_f32();
        let fg_ghost = self.theme.ui.fg_ghost.as_f32();
        let accent = self.theme.ui.accent.as_f32();
        let info = self.theme.ui.info.as_f32();
        let warning = self.theme.ui.warning.as_f32();
        let error = self.theme.ui.error.as_f32();
        let panel_bg = self.theme.ui.panel_bg.as_f32();
        let mut border_color = self.theme.ui.border_color.as_f32();
        border_color[3] = border_color[3].clamp(0.45, 0.75);

        let key_font_size = (font_size * 0.86).max(10.0);
        let panel_w = (window_w - margin_x * 2.0).max(120.0);
        let content_w = (panel_w - pad_x * 2.0).max(1.0);

        // Cell = keycap + gap + label; column width fits the widest cell.
        let cell_w = entries
            .iter()
            .map(|entry| {
                estimate_help_keycaps_width(&[entry.key.as_str()], key_font_size)
                    + key_label_gap
                    + estimate_monospace_width(&entry.label, font_size)
            })
            .fold(80.0_f32, f32::max)
            .min(content_w);
        let cols = ((content_w + col_gap) / (cell_w + col_gap))
            .floor()
            .max(1.0) as usize;
        let visible = entries.len().min(cols * MAX_ROWS);
        let rows = visible.div_ceil(cols);
        let hidden = entries.len() - visible;

        let header_h = line_h + 6.0 * scale;
        let panel_h = pad_y * 2.0 + header_h + rows as f32 * row_h;
        let panel_x = margin_x;
        let panel_y = (window_h - statusbar_h - panel_h - 8.0 * scale).max(0.0);

        let mut chrome = vec![
            RegionDrawInstance::new(
                [
                    panel_x - 1.0,
                    panel_y - 1.0,
                    panel_w + 2.0,
                    panel_h + 2.0,
                ],
                border_color,
            )
            .with_radius(self.panel_corner_radius + 1.0),
            RegionDrawInstance::new([panel_x, panel_y, panel_w, panel_h], panel_bg)
                .with_radius(self.panel_corner_radius),
        ];
        let mut glyphs: Vec<GlyphInstance> = Vec::new();

        // Header: pending prefix as keycaps + binding count hint.
        let header_y = panel_y + pad_y;
        let prefix_tokens: Vec<&str> = prefix_label.split_whitespace().collect();
        let mut header_x = panel_x + pad_x;
        if !prefix_tokens.is_empty() {
            glyphs.extend(layout_help_keycaps(
                &prefix_tokens,
                &mut self.whichkey_text_system,
                &mut self.atlas,
                &self.queue,
                &mut chrome,
                header_x,
                header_y,
                key_font_size,
                header_h,
                fg,
                fg_dim,
                accent,
                info,
                warning,
                error,
                panel_bg,
            ));
            header_x +=
                estimate_help_keycaps_width(&prefix_tokens, key_font_size) + key_label_gap * 2.0;
        }
        let hint = if hidden > 0 {
            format!("{} bindings (+{hidden} hidden)", entries.len())
        } else {
            format!("{} bindings", entries.len())
        };
        glyphs.extend(layout_panel_text(
            &hint,
            &mut self.whichkey_text_system,
            &mut self.atlas,
            &self.queue,
            header_x,
            header_y + ((header_h - line_h) * 0.5).max(0.0),
            fg_ghost,
        ));

        // Entry grid, column-major within each row.
        let grid_y = header_y + header_h;
        for (idx, entry) in entries.iter().take(visible).enumerate() {
            let row = idx / cols;
            let col = idx % cols;
            let cell_x = panel_x + pad_x + col as f32 * (cell_w + col_gap);
            let cell_y = grid_y + row as f32 * row_h;

            glyphs.extend(layout_help_keycaps(
                &[entry.key.as_str()],
                &mut self.whichkey_text_system,
                &mut self.atlas,
                &self.queue,
                &mut chrome,
                cell_x,
                cell_y,
                key_font_size,
                row_h,
                fg,
                fg_dim,
                accent,
                info,
                warning,
                error,
                panel_bg,
            ));
            let label_x = cell_x
                + estimate_help_keycaps_width(&[entry.key.as_str()], key_font_size)
                + key_label_gap;
            let label_color = if entry.is_group { accent } else { fg_dim };
            glyphs.extend(layout_panel_text(
                &entry.label,
                &mut self.whichkey_text_system,
                &mut self.atlas,
                &self.queue,
                label_x,
                cell_y + ((row_h - line_h) * 0.5).max(0.0),
                label_color,
            ));
        }

        self.whichkey_scissor = rect_to_scissor([panel_x - 2.0, panel_y - 2.0, panel_w + 4.0, panel_h + 4.0]);
        self.whichkey_chrome_instances = chrome;
        self.whichkey_glyph_instances = glyphs;
        self.whichkey_text_pipeline.upload_instances(
            &self.device,
            &self.queue,
            &self.whichkey_glyph_instances,
        );
    }

    pub fn clear_whichkey_popup(&mut self) {
        if self.whichkey_scissor.is_none() && self.whichkey_chrome_instances.is_empty() {
            return;
        }
        self.whichkey_scissor = None;
        self.whichkey_chrome_instances.clear();
        self.whichkey_glyph_instances.clear();
        self.whichkey_text_pipeline
            .upload_instances(&self.device, &self.queue, &[]);
    }
}
