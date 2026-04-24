use crate::{
    core::mode::EditorMode,
    render::{
        region_pipeline::RegionDrawInstance,
        renderer::{Renderer, StatusbarLayoutKey},
    },
};

use super::super::helpers::{
    clamp_monospace_text, estimate_monospace_width, layout_panel_text, mode_display_label,
    mode_pill_color, rect_to_scissor,
};

impl Renderer {
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
}
