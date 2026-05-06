use crate::{
    core::mode::EditorMode,
    render::{
        region_pipeline::RegionDrawInstance,
        renderer::{Renderer, StatusbarLayoutKey},
    },
};

use super::super::helpers::{
    clamp_monospace_text, estimate_monospace_width, layout_panel_text, layout_panel_text_bold,
    mode_display_label, mode_pill_color, rect_to_scissor,
};

fn with_alpha(mut color: [f32; 4], alpha: f32) -> [f32; 4] {
    color[3] = alpha;
    color
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
        let line_h   = self.statusbar_line_height;
        let font_size = self.statusbar_font_size;
        let left_pad  = self.statusbar_padding_x;
        let right_pad = self.statusbar_padding_x;

        // Gap between status-bar items — scales with font size.
        let item_gap = (font_size * 0.95).max(8.0);

        let width = (bounds[2] - left_pad * 2.0).max(1.0);
        self.statusbar_text_system.set_size(Some(width), Some(bounds[3]));

        // Vertical centering helpers.
        let origin_y = bounds[1] + ((bounds[3] - line_h) * 0.5).max(0.0);

        // ── Colors ──────────────────────────────────────────────────────────────
        let mode_color   = mode_pill_color(mode, &self.theme);
        let fg_dim       = with_alpha(self.theme.ui.fg_dim.as_f32(), 0.85);
        let fg_ghost     = with_alpha(self.theme.ui.fg_ghost.as_f32(), 0.75);
        let accent       = self.theme.ui.accent.as_f32();
        let status_bg    = with_alpha(self.theme.ui.status_bar_bg.as_f32(), 0.98);
        let border_color = with_alpha(self.theme.ui.border_color.as_f32(), 0.85);
        let error_fg     = self.theme.ui.error.as_f32();
        let warning_fg   = self.theme.ui.warning.as_f32();
        let success_fg   = self.theme.ui.success.as_f32();
        let badge_border = with_alpha(self.theme.ui.border_color.as_f32(), 0.9);

        let mut glyphs = Vec::new();
        let mut chrome = vec![
            // Bar background.
            RegionDrawInstance::new(bounds, status_bg),
            // Top 1 px separator.
            RegionDrawInstance::new(
                [bounds[0], bounds[1], bounds[2], 1.0_f32.min(bounds[3])],
                border_color,
            ),
        ];

        // ── ① Mode pill ─────────────────────────────────────────────────────────
        // Pill height = ~72 % of bar height, vertically centered.
        let pill_pad_v  = (bounds[3] * 0.14).max(2.0);
        let pill_height = (bounds[3] - pill_pad_v * 2.0).max(8.0);
        let pill_y      = bounds[1] + (bounds[3] - pill_height) * 0.5;

        // Dot inside the pill — sized as ~46 % of font_size, capped 4–8 px.
        let dot_size    = (font_size * 0.46).max(4.0).min(8.0);
        let dot_pad_l   = (font_size * 0.40).max(4.0); // from pill left edge to dot
        let dot_text_gap = (font_size * 0.32).max(3.0); // between dot and label
        let pill_pad_r  = (font_size * 0.45).max(4.0);
        let pill_radius = if self.round_ui { 4.0_f32 } else { 0.0 };

        let mode_label = mode_display_label(mode);
        let label_w    = estimate_monospace_width(mode_label, font_size);
        let pill_width = dot_pad_l + dot_size + dot_text_gap + label_w + pill_pad_r;
        let pill_x     = bounds[0] + left_pad;

        // Border quad (outer) — mode_color at 35 % alpha.
        chrome.push(
            RegionDrawInstance::new(
                [pill_x, pill_y, pill_width, pill_height],
                with_alpha(mode_color, 0.35),
            )
            .with_radius(pill_radius),
        );
        // Fill quad (1 px inset) — mode_color at 12 % alpha.
        chrome.push(
            RegionDrawInstance::new(
                [
                    pill_x + 1.0,
                    pill_y + 1.0,
                    (pill_width - 2.0).max(0.0),
                    (pill_height - 2.0).max(0.0),
                ],
                with_alpha(mode_color, 0.12),
            )
            .with_radius((pill_radius - 1.0).max(0.0)),
        );

        // Mode indicator dot — full mode_color, rendered as a circle via radius.
        let dot_x = pill_x + dot_pad_l;
        let dot_y = pill_y + (pill_height - dot_size) * 0.5;
        chrome.push(
            RegionDrawInstance::new([dot_x, dot_y, dot_size, dot_size], mode_color)
                .with_radius(dot_size * 0.5),
        );

        // Mode label text — bold, in mode_color.
        let text_x = dot_x + dot_size + dot_text_gap;
        glyphs.extend(layout_panel_text_bold(
            mode_label,
            &mut self.statusbar_text_system,
            &mut self.atlas,
            &self.queue,
            text_x,
            origin_y,
            mode_color,
        ));

        // ── Left cursor, tracking how far right we've drawn ──────────────────────
        let mut left_x = pill_x + pill_width + item_gap;

        // ── ② Git branch ─────────────────────────────────────────────────────────
        let branch = git_branch.trim();
        if !branch.is_empty() {
            let branch_clean = if branch.starts_with("git: ") {
                &branch[5..]
            } else {
                branch
            };
            let icon = "⎇ ";
            glyphs.extend(layout_panel_text(
                icon,
                &mut self.statusbar_text_system,
                &mut self.atlas,
                &self.queue,
                left_x,
                origin_y,
                fg_ghost,
            ));
            left_x += estimate_monospace_width(icon, font_size);
            glyphs.extend(layout_panel_text(
                branch_clean,
                &mut self.statusbar_text_system,
                &mut self.atlas,
                &self.queue,
                left_x,
                origin_y,
                fg_dim,
            ));
            left_x += estimate_monospace_width(branch_clean, font_size) + item_gap;
        }

        // ── Diagnostic badges (errors / warnings) — between git and right zone ──
        let show_errors   = diagnostics_errors > 0;
        let show_warnings = diagnostics_warnings > 0;
        let badge_gap     = (font_size * 0.5).max(5.0);
        let badge_height  = (pill_height - 2.0).max(8.0);
        let badge_y       = bounds[1] + (bounds[3] - badge_height) * 0.5;
        let badge_radius  = if self.round_ui { 5.0 } else { 0.0 };

        if show_errors {
            let text     = format!(" ✗ {} ", diagnostics_errors);
            let text_w   = estimate_monospace_width(&text, font_size);
            let err_fill = with_alpha(error_fg, 0.12);
            chrome.push(
                RegionDrawInstance::new([left_x, badge_y, text_w, badge_height], err_fill)
                    .with_radius(badge_radius),
            );
            chrome.push(
                RegionDrawInstance::new([left_x, badge_y, text_w, badge_height], badge_border)
                    .with_radius(badge_radius),
            );
            glyphs.extend(layout_panel_text(
                &text,
                &mut self.statusbar_text_system,
                &mut self.atlas,
                &self.queue,
                left_x,
                origin_y,
                error_fg,
            ));
            left_x += text_w + badge_gap;
        }

        if show_warnings {
            let text     = format!(
                " {} warning{} ",
                diagnostics_warnings,
                if diagnostics_warnings == 1 { "" } else { "s" }
            );
            let text_w    = estimate_monospace_width(&text, font_size);
            let warn_fill = with_alpha(warning_fg, 0.12);
            chrome.push(
                RegionDrawInstance::new([left_x, badge_y, text_w, badge_height], warn_fill)
                    .with_radius(badge_radius),
            );
            chrome.push(
                RegionDrawInstance::new([left_x, badge_y, text_w, badge_height], badge_border)
                    .with_radius(badge_radius),
            );
            glyphs.extend(layout_panel_text(
                &text,
                &mut self.statusbar_text_system,
                &mut self.atlas,
                &self.queue,
                left_x,
                origin_y,
                warning_fg,
            ));
            left_x += text_w + badge_gap;
        }

        // ── Pending key sequence — accent color, between zones ───────────────────
        // ── ③ RIGHT ZONE — built right-to-left, then rendered left-to-right ──────
        //
        // Slots (right → left): [lsp_dot] [language] [encoding] [cursor_pos] [search]
        let lsp_dot_size = (font_size * 0.46).max(4.0).min(8.0);
        let cursor_text  = format!("Ln {}, Col {}", line + 1, col + 1);
        let encoding_str = "UTF-8";
        let lang_str     = filetype.trim();
        let search_str   = search_match_position
            .map(|(c, t)| format!("{c}/{t}"))
            .unwrap_or_default();

        // Collect right-zone text items with their colors (left-to-right order).
        let mut right_items: Vec<(String, [f32; 4])> = Vec::new();
        if !search_str.is_empty() {
            right_items.push((search_str, accent));
        }
        right_items.push((cursor_text, fg_ghost));
        right_items.push((encoding_str.to_string(), fg_ghost));
        if !lang_str.is_empty() {
            right_items.push((lang_str.to_string(), fg_ghost));
        }

        // Total width of text items + gaps between them.
        let text_items_w: f32 = right_items
            .iter()
            .map(|(t, _)| estimate_monospace_width(t, font_size))
            .sum::<f32>()
            + item_gap * right_items.len().saturating_sub(1) as f32;
        // LSP dot sits at the far right, padded from the last text item.
        let lsp_zone_w   = lsp_dot_size + item_gap;
        let total_right_w = text_items_w + lsp_zone_w;

        let right_start = (bounds[0] + bounds[2] - right_pad - total_right_w)
            .max(left_x + item_gap);

        // Render right-zone text items.
        let mut rx = right_start;
        for (i, (text, color)) in right_items.iter().enumerate() {
            if i > 0 {
                rx += item_gap;
            }
            glyphs.extend(layout_panel_text(
                text,
                &mut self.statusbar_text_system,
                &mut self.atlas,
                &self.queue,
                rx,
                origin_y,
                *color,
            ));
            rx += estimate_monospace_width(text, font_size);
        }

        // ── ⑦ Pending keys — fits in remaining space left of right zone ──────────
        let pending_maxw = (right_start - left_x - item_gap).max(0.0);
        let pending_text = clamp_monospace_text(pending_keys, pending_maxw, font_size);
        if !pending_text.is_empty() {
            glyphs.extend(layout_panel_text(
                &pending_text,
                &mut self.statusbar_text_system,
                &mut self.atlas,
                &self.queue,
                left_x,
                origin_y,
                with_alpha(accent, 0.95),
            ));
        }

        // ── ⑦ LSP status dot ─────────────────────────────────────────────────────
        let lsp_color = if diagnostics_errors > 0 {
            error_fg
        } else {
            success_fg
        };
        let lsp_x = rx + item_gap;
        let lsp_y = bounds[1] + (bounds[3] - lsp_dot_size) * 0.5;
        chrome.push(
            RegionDrawInstance::new([lsp_x, lsp_y, lsp_dot_size, lsp_dot_size], lsp_color)
                .with_radius(lsp_dot_size * 0.5),
        );

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
