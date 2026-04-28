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

fn with_alpha(mut color: [f32; 4], alpha: f32) -> [f32; 4] {
    color[3] = alpha;
    color
}

fn push_badge(
    chrome: &mut Vec<RegionDrawInstance>,
    rect: [f32; 4],
    fill: [f32; 4],
    border: [f32; 4],
    radius: f32,
) {
    chrome.push(RegionDrawInstance::new(rect, fill).with_radius(radius));
    chrome.push(RegionDrawInstance::new(rect, border).with_radius(radius));
}

impl Renderer {
    pub fn update_statusbar_content(
        &mut self,
        mode: EditorMode,
        pending_keys: &str,
        git_branch: &str,
        filetype: &str,
        search_match_position: Option<(usize, usize)>,
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
            search_match_position,
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
        let pill_text = format!(" {} ", mode_label);
        let pill_width = estimate_monospace_width(&pill_text, font_size);
        let pill_x = bounds[0];
        let pill_height = bounds[3].max(0.0);
        let pill_y = bounds[1];
        let pill_rect = [pill_x, pill_y, pill_width, pill_height];

        let branch_label = if git_branch.trim().is_empty() {
            String::new()
        } else {
            let b = git_branch.trim();
            if b.starts_with("git: ") {
                b.to_string()
            } else {
                format!("git: {b}")
            }
        };
        let mut right_parts = Vec::new();
        if !branch_label.is_empty() {
            right_parts.push(branch_label);
        }
        if !filetype.trim().is_empty() {
            right_parts.push(filetype.trim().to_string());
        }
        if let Some((current, total)) = search_match_position {
            right_parts.push(format!("{current}/{total}"));
        }
        right_parts.push("utf-8".to_string());
        right_parts.push("LF".to_string());
        right_parts.push(format!("Ln {}, Col {}", line + 1, col + 1));
        let right_text = right_parts.join(" · ");

        let show_errors = diagnostics_errors > 0;
        let show_warnings = diagnostics_warnings > 0;
        let error_badge_text = if show_errors {
            format!("✗ {}", diagnostics_errors)
        } else {
            String::new()
        };
        let warning_badge_text = if show_warnings {
            format!(
                "{} warning{}",
                diagnostics_warnings,
                if diagnostics_warnings == 1 { "" } else { "s" }
            )
        } else {
            String::new()
        };

        let origin_y = bounds[1] + ((bounds[3] - line_h) * 0.5).max(0.0);
        let fg_faint = with_alpha(self.theme.ui.fg_dim.as_f32(), 0.72);
        let accent = self.theme.ui.accent.as_f32();
        let pill_fg = self.theme.ui.bg.as_f32();
        let error_fg = self.theme.ui.error.as_f32();
        let warning_fg = self.theme.ui.warning.as_f32();
        let status_bg = with_alpha(self.theme.ui.status_bar_bg.as_f32(), 0.98);
        let border = with_alpha(self.theme.ui.border_color.as_f32(), 0.85);
        let badge_border = with_alpha(self.theme.ui.border_color.as_f32(), 0.9);
        let error_badge_bg = with_alpha(error_fg, 0.12);
        let warning_badge_bg = with_alpha(warning_fg, 0.12);

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
        let error_badge_width = if show_errors {
            estimate_monospace_width(&format!(" {} ", error_badge_text), font_size)
        } else {
            0.0
        };
        let warning_badge_width = if show_warnings {
            estimate_monospace_width(&format!(" {} ", warning_badge_text), font_size)
        } else {
            0.0
        };
        let badge_gap = 6.0;
        let diagnostics_width = error_badge_width
            + warning_badge_width
            + if show_errors && show_warnings {
                badge_gap
            } else {
                0.0
            };
        let show_diagnostics = show_errors || show_warnings;
        let right_x = (bounds[0] + bounds[2] - self.statusbar_padding_x - right_width)
            .max(bounds[0] + self.statusbar_padding_x);
        let left_gap = 8.0;
        let diag_x = pill_x + pill_width + left_gap;
        let pending_x = diag_x + diagnostics_width + if show_diagnostics { 14.0 } else { 0.0 };
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
                with_alpha(accent, 0.95),
            ));
        }
        glyphs.extend(layout_panel_text(
            &right_text,
            &mut self.statusbar_text_system,
            &mut self.atlas,
            &self.queue,
            right_x,
            origin_y,
            fg_faint,
        ));
        let mut chrome = vec![
            RegionDrawInstance::new(bounds, status_bg),
            RegionDrawInstance::new(
                [bounds[0], bounds[1], bounds[2], 1.0_f32.min(bounds[3])],
                border,
            ),
            RegionDrawInstance::new(pill_rect, mode_color),
        ];

        let badge_height = (bounds[3] - 6.0).max(12.0);
        let badge_y = bounds[1] + ((bounds[3] - badge_height) * 0.5).max(0.0);
        let badge_radius = 5.0;
        let mut current_badge_x = diag_x;

        if show_errors {
            let rect = [current_badge_x, badge_y, error_badge_width, badge_height];
            push_badge(
                &mut chrome,
                rect,
                error_badge_bg,
                badge_border,
                badge_radius,
            );
            glyphs.extend(layout_panel_text(
                &format!(" {} ", error_badge_text),
                &mut self.statusbar_text_system,
                &mut self.atlas,
                &self.queue,
                current_badge_x,
                origin_y,
                error_fg,
            ));
            current_badge_x += error_badge_width + if show_warnings { badge_gap } else { 0.0 };
        }

        if show_warnings {
            let rect = [current_badge_x, badge_y, warning_badge_width, badge_height];
            push_badge(
                &mut chrome,
                rect,
                warning_badge_bg,
                badge_border,
                badge_radius,
            );
            glyphs.extend(layout_panel_text(
                &format!(" {} ", warning_badge_text),
                &mut self.statusbar_text_system,
                &mut self.atlas,
                &self.queue,
                current_badge_x,
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

        self.statusbar_chrome_instances = chrome;
        self.last_statusbar_layout_key = Some(layout_key);
        self.statusbar_chrome_instances.clone()
    }

    // ── LSP Install Guide Popup ───────────────────────────────────────────────
}
