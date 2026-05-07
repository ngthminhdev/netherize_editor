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
    /// Render the three-zone status bar.
    ///
    /// Left Zone   (→): mode-pill · git-branch · dirty-dot · file-name · pending-keys
    /// Diag Zone (⊕): error/warning badges, centered in remaining space
    /// Right Zone  (←): lsp-progress · search · cursor · encoding · language · lsp-dot
    #[allow(clippy::too_many_arguments)]
    pub fn update_statusbar_content(
        &mut self,
        mode: EditorMode,
        pending_keys: &str,
        git_branch: &str,
        is_dirty: bool,
        active_file_name: &str,
        filetype: &str,
        search_match_position: Option<(usize, usize)>,
        line: usize,
        col: usize,
        diagnostics_errors: usize,
        diagnostics_warnings: usize,
        lsp_loading: bool,
        lsp_loading_frame: u8,
        lsp_progress: Option<&str>,
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
            is_dirty,
            active_file_name: active_file_name.to_string(),
            filetype: filetype.to_string(),
            search_match_position,
            line,
            col,
            diagnostics_errors,
            diagnostics_warnings,
            lsp_loading,
            lsp_loading_frame,
            lsp_progress: lsp_progress.map(str::to_string),
            bounds,
        };
        if self.last_statusbar_layout_key.as_ref() == Some(&layout_key) {
            return self.statusbar_chrome_instances.clone();
        }

        self.statusbar_scissor = rect_to_scissor(bounds);
        let line_h    = self.statusbar_line_height;
        let font_size = self.statusbar_font_size;
        let left_pad  = self.statusbar_padding_x;
        let right_pad = self.statusbar_padding_x;
        let item_gap  = (font_size * 0.95).max(8.0);

        let width = (bounds[2] - left_pad * 2.0).max(1.0);
        self.statusbar_text_system.set_size(Some(width), Some(bounds[3]));

        let origin_y = bounds[1] + ((bounds[3] - line_h) * 0.5).max(0.0);

        // ── Colors ───────────────────────────────────────────────────────────────
        let mode_color    = mode_pill_color(mode, &self.theme);
        let fg            = self.theme.ui.fg.as_f32();
        let fg_dim        = with_alpha(self.theme.ui.fg_dim.as_f32(), 0.85);
        let fg_ghost      = with_alpha(self.theme.ui.fg_ghost.as_f32(), 0.75);
        let accent        = self.theme.ui.accent.as_f32();
        let status_bg     = with_alpha(self.theme.ui.status_bar_bg.as_f32(), 0.98);
        let border_color  = with_alpha(self.theme.ui.border_color.as_f32(), 0.85);
        let error_fg      = self.theme.ui.error.as_f32();
        let warning_fg    = self.theme.ui.warning.as_f32();
        let success_fg    = self.theme.ui.success.as_f32();
        let dirty_color   = self.theme.ui.dirty_indicator.as_f32();
        let badge_border  = with_alpha(self.theme.ui.border_color.as_f32(), 0.9);
        let amber: [f32; 4] = [0.95, 0.65, 0.15, 0.90];

        let mut glyphs: Vec<crate::render::glyph_instance::GlyphInstance> = Vec::new();
        let mut chrome = vec![
            // Bar background.
            RegionDrawInstance::new(bounds, status_bg),
            // Top 1 px separator.
            RegionDrawInstance::new(
                [bounds[0], bounds[1], bounds[2], 1.0_f32.min(bounds[3])],
                border_color,
            ),
        ];

        // ── Pill geometry ─────────────────────────────────────────────────────────
        let pill_pad_v   = (bounds[3] * 0.14).max(2.0);
        let pill_height  = (bounds[3] - pill_pad_v * 2.0).max(8.0);
        let pill_y       = bounds[1] + (bounds[3] - pill_height) * 0.5;
        let dot_size     = (font_size * 0.46).max(4.0).min(8.0);
        let dot_pad_l    = (font_size * 0.40).max(4.0);
        let dot_text_gap = (font_size * 0.32).max(3.0);
        let pill_pad_r   = (font_size * 0.45).max(4.0);
        let pill_radius  = if self.round_ui { 4.0_f32 } else { 0.0 };
        let mode_label   = mode_display_label(mode);
        let label_w      = estimate_monospace_width(mode_label, font_size);
        let pill_width   = dot_pad_l + dot_size + dot_text_gap + label_w + pill_pad_r;
        let pill_x       = bounds[0] + left_pad;

        // Pill — border outer quad, fill inner quad (1 px inset).
        chrome.push(
            RegionDrawInstance::new([pill_x, pill_y, pill_width, pill_height], with_alpha(mode_color, 0.35))
                .with_radius(pill_radius),
        );
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
        // Mode indicator dot (circle).
        let dot_x = pill_x + dot_pad_l;
        let dot_y = pill_y + (pill_height - dot_size) * 0.5;
        chrome.push(
            RegionDrawInstance::new([dot_x, dot_y, dot_size, dot_size], mode_color)
                .with_radius(dot_size * 0.5),
        );
        // Mode label text.
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

        // ══════════════════════════════════════════════════════════════════════════
        // LEFT ZONE  (fixed items — mode-pill already placed above)
        // ══════════════════════════════════════════════════════════════════════════
        let mut left_x = pill_x + pill_width + item_gap;

        // ── Git branch ────────────────────────────────────────────────────────────
        let branch = git_branch.trim();
        if !branch.is_empty() {
            let branch_name = branch.strip_prefix("git: ").unwrap_or(branch);
            let icon   = "⎇ ";
            let icon_w = estimate_monospace_width(icon, font_size);
            let name_w = estimate_monospace_width(branch_name, font_size);
            glyphs.extend(layout_panel_text(
                icon,
                &mut self.statusbar_text_system,
                &mut self.atlas,
                &self.queue,
                left_x,
                origin_y,
                fg_ghost,
            ));
            left_x += icon_w;
            glyphs.extend(layout_panel_text(
                branch_name,
                &mut self.statusbar_text_system,
                &mut self.atlas,
                &self.queue,
                left_x,
                origin_y,
                fg_dim,
            ));
            left_x += name_w + item_gap;
        }

        // ── Dirty dot ─────────────────────────────────────────────────────────────
        if is_dirty {
            let dot_str = "●";
            let dot_w   = estimate_monospace_width(dot_str, font_size);
            glyphs.extend(layout_panel_text(
                dot_str,
                &mut self.statusbar_text_system,
                &mut self.atlas,
                &self.queue,
                left_x,
                origin_y,
                dirty_color,
            ));
            left_x += dot_w + item_gap;
        }

        // ── Active file name ──────────────────────────────────────────────────────
        let file_name = active_file_name.trim();
        if !file_name.is_empty() {
            let file_w = estimate_monospace_width(file_name, font_size);
            glyphs.extend(layout_panel_text(
                file_name,
                &mut self.statusbar_text_system,
                &mut self.atlas,
                &self.queue,
                left_x,
                origin_y,
                fg,
            ));
            left_x += file_w + item_gap;
        }

        // left_zone_end is the x-coord after all fixed left items.
        let left_zone_end = left_x;

        // ══════════════════════════════════════════════════════════════════════════
        // RIGHT ZONE — measure width first, then place from right edge
        // ══════════════════════════════════════════════════════════════════════════
        let lsp_dot_size = (font_size * 0.46).max(4.0).min(8.0);
        let cursor_text  = format!("Ln {}, Col {}", line + 1, col + 1);
        let search_str   = search_match_position
            .map(|(c, t)| format!("{c}/{t}"))
            .unwrap_or_default();

        // Right zone items in display order (left → right as rendered).
        let mut right_items: Vec<(String, [f32; 4])> = Vec::new();
        // LSP progress label (leftmost in right zone, most prominent).
        if let Some(prog) = lsp_progress {
            let trimmed = prog.trim();
            if !trimmed.is_empty() {
                let label = format!(
                    "⏳ {}",
                    clamp_monospace_text(trimmed, font_size * 18.0, font_size)
                );
                right_items.push((label, warning_fg));
            }
        }
        if !search_str.is_empty() {
            right_items.push((search_str, accent));
        }
        right_items.push((cursor_text, fg_ghost));
        right_items.push(("UTF-8".to_string(), fg_ghost));
        let lang_str = filetype.trim();
        if !lang_str.is_empty() {
            right_items.push((lang_str.to_string(), fg_ghost));
        }

        let right_text_w: f32 = right_items
            .iter()
            .map(|(t, _)| estimate_monospace_width(t, font_size))
            .sum::<f32>()
            + item_gap * right_items.len().saturating_sub(1) as f32;
        // LSP dot/spinner sits at the far right, separated by one item_gap.
        let lsp_zone_w   = lsp_dot_size + item_gap;
        let total_right_w = right_text_w + lsp_zone_w;
        let right_start   = (bounds[0] + bounds[2] - right_pad - total_right_w)
            .max(left_zone_end + item_gap);

        // ══════════════════════════════════════════════════════════════════════════
        // DIAG ZONE — centered on screen, clamped between the two side zones
        // ══════════════════════════════════════════════════════════════════════════
        let show_errors   = diagnostics_errors > 0;
        let show_warnings = diagnostics_warnings > 0;
        let badge_height  = (pill_height - 2.0).max(8.0);
        let badge_y       = bounds[1] + (bounds[3] - badge_height) * 0.5;
        let badge_radius  = if self.round_ui { 5.0 } else { 0.0 };
        let between_gap   = (font_size * 0.5).max(5.0);

        let err_text  = if show_errors { format!(" ✗ {} ", diagnostics_errors) } else { String::new() };
        let warn_text = if show_warnings { format!(" ⚠ {} ", diagnostics_warnings) } else { String::new() };
        let err_w     = if show_errors { estimate_monospace_width(&err_text, font_size) } else { 0.0 };
        let warn_w    = if show_warnings { estimate_monospace_width(&warn_text, font_size) } else { 0.0 };
        let pair_gap  = if show_errors && show_warnings { between_gap } else { 0.0 };
        let diag_total_w = err_w + pair_gap + warn_w;

        if diag_total_w > 0.0 {
            let screen_center = bounds[0] + bounds[2] * 0.5;
            let diag_ideal_x  = screen_center - diag_total_w * 0.5;
            let diag_min_x    = left_zone_end;
            let diag_max_x    = (right_start - diag_total_w - item_gap).max(diag_min_x);
            let diag_x        = diag_ideal_x.clamp(diag_min_x, diag_max_x);

            let mut dx = diag_x;
            if show_errors {
                let fill = with_alpha(error_fg, 0.12);
                chrome.push(
                    RegionDrawInstance::new([dx, badge_y, err_w, badge_height], fill)
                        .with_radius(badge_radius),
                );
                chrome.push(
                    RegionDrawInstance::new([dx + 1.0, badge_y + 1.0, (err_w - 2.0).max(0.0), (badge_height - 2.0).max(0.0)], badge_border)
                        .with_radius((badge_radius - 1.0).max(0.0)),
                );
                glyphs.extend(layout_panel_text(
                    &err_text,
                    &mut self.statusbar_text_system,
                    &mut self.atlas,
                    &self.queue,
                    dx,
                    origin_y,
                    error_fg,
                ));
                dx += err_w + pair_gap;
            }
            if show_warnings {
                let fill = with_alpha(warning_fg, 0.12);
                chrome.push(
                    RegionDrawInstance::new([dx, badge_y, warn_w, badge_height], fill)
                        .with_radius(badge_radius),
                );
                chrome.push(
                    RegionDrawInstance::new([dx + 1.0, badge_y + 1.0, (warn_w - 2.0).max(0.0), (badge_height - 2.0).max(0.0)], badge_border)
                        .with_radius((badge_radius - 1.0).max(0.0)),
                );
                glyphs.extend(layout_panel_text(
                    &warn_text,
                    &mut self.statusbar_text_system,
                    &mut self.atlas,
                    &self.queue,
                    dx,
                    origin_y,
                    warning_fg,
                ));
            }
        }

        // ── Pending keys (between file name and diag/right zone) ─────────────────
        let pending_right_bound = if diag_total_w > 0.0 {
            let screen_center = bounds[0] + bounds[2] * 0.5;
            let diag_ideal_x  = screen_center - diag_total_w * 0.5;
            let diag_max_x    = (right_start - diag_total_w - item_gap).max(left_zone_end);
            diag_ideal_x.clamp(left_zone_end, diag_max_x)
        } else {
            right_start
        };
        let pending_maxw  = (pending_right_bound - left_zone_end - item_gap).max(0.0);
        let pending_text  = clamp_monospace_text(pending_keys, pending_maxw, font_size);
        if !pending_text.is_empty() {
            glyphs.extend(layout_panel_text(
                &pending_text,
                &mut self.statusbar_text_system,
                &mut self.atlas,
                &self.queue,
                left_zone_end,
                origin_y,
                with_alpha(accent, 0.95),
            ));
        }

        // ── Render right zone text items (left → right) ───────────────────────────
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

        // ── LSP status dot / install spinner ─────────────────────────────────────
        let lsp_x = rx + item_gap;
        let lsp_y = bounds[1] + (bounds[3] - lsp_dot_size) * 0.5;
        if lsp_loading {
            const SPINNER: &[char] = &['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];
            let spinner_char: String = SPINNER[(lsp_loading_frame as usize) % SPINNER.len()].into();
            glyphs.extend(layout_panel_text(
                &spinner_char,
                &mut self.statusbar_text_system,
                &mut self.atlas,
                &self.queue,
                lsp_x,
                origin_y,
                amber,
            ));
        } else {
            let lsp_color = if lsp_progress.is_some() {
                amber
            } else if diagnostics_errors > 0 {
                error_fg
            } else {
                success_fg
            };
            chrome.push(
                RegionDrawInstance::new([lsp_x, lsp_y, lsp_dot_size, lsp_dot_size], lsp_color)
                    .with_radius(lsp_dot_size * 0.5),
            );
        }

        // ── Commit ────────────────────────────────────────────────────────────────
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
}
