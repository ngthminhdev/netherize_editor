#![allow(unused_imports)]

use crate::{
    app::app_state::{
        AppState, CompletionDisplayItem, DiagnosticsState, EditorOverlay, FloatingBoxBlock,
        FloatingBoxStyle, HelpState, OverlayColorToken, ReferencesBufferState, SettingItem,
        SettingsState,
    },
    async_runtime::message::LspDiagnostic,
    config::theme_config::ThemeConfig,
    core::mode::EditorMode,
    render::{
        glyph_instance::GlyphInstance, region_pipeline::RegionDrawInstance, renderer::Renderer,
    },
    text::layout_sync::{compute_caret_layout, compute_cursor_overlay, rebuild_layout_projection},
};
use cosmic_text::Metrics;

use super::super::{
    components::{estimate_help_keycaps_width, layout_help_keycaps},
    helpers::{
        caret_rect_for_mode, clamp_monospace_text, estimate_monospace_width,
        gutter_width_for_editor, layout_panel_rich_text, layout_panel_text,
        layout_panel_text_italic, rect_to_scissor, should_draw_block_cursor,
    },
};
use super::{cursor_diagnostic, editor_viewport_geometry, run_x_for_byte, wrap_text_lines};
use crate::text::text_system::StyledTextSpan;

impl Renderer {
    pub fn update_help_buffer_content(&mut self, help: &HelpState, center_bounds: [f32; 4]) {
        if center_bounds[2] < 1.0 || center_bounds[3] < 1.0 {
            self.clear_editor_overlays();
            return;
        }

        let font_size = self.theme.editor.font_size.max(14.0);
        let title_size = (font_size * 2.2).max(28.0);
        let small_size = (font_size * 0.82).max(11.0);
        let line_height = self.theme.editor.line_height.max(font_size + 5.0);
        self.editor_overlay_text_system
            .set_metrics(Metrics::new(font_size, line_height));
        self.editor_overlay_scissor = rect_to_scissor(center_bounds);

        // Keep the Cheat Sheet page on the same coordinate system as Settings:
        // both special buffer tabs should occupy the center editor region with
        // identical inset behavior instead of using independent full-screen
        // margins.
        let pad_x = self.editor_padding_x.max(18.0);
        let pad_y = self.editor_padding_y.max(18.0);
        let panel_x = center_bounds[0] + pad_x;
        let panel_y = center_bounds[1] + pad_y;
        let panel_w = (center_bounds[2] - pad_x * 2.0).max(1.0);
        let panel_h = (center_bounds[3] - pad_y * 2.0).max(1.0);
        let fg = self.theme.ui.fg.as_f32();
        let fg_dim = self.theme.ui.fg_dim.as_f32();
        let fg_ghost = self.theme.ui.fg_ghost.as_f32();
        let accent = self.theme.ui.accent.as_f32();
        let panel_bg = self.theme.ui.panel_bg.as_f32();
        let editor_bg = self.theme.editor.bg.as_f32();
        let mut divider = self.theme.ui.fg_ghost.as_f32();
        divider[3] = divider[3].clamp(0.28, 0.42);
        let mut card_bg = panel_bg;
        card_bg[3] = card_bg[3].max(0.94);
        let mut card_border = divider;
        card_border[3] = 0.72;

        // ── sizing constants (tweak here) ──────────────────────────────────────
        let hdr_logo_x = 168.0;
        let hdr_accent_x = 24.0;
        let hdr_accent_y = 28.0;
        let hdr_accent_size = 68.0;
        let hdr_subtitle_gap = 24.0;
        let hdr_meta_col_w = 420.0;
        let hdr_meta_lh_gap = 28.0;
        let hdr_wrap_margin = 56.0;
        let hdr_title_gap = 64.0;
        let hdr_min_h = 208.0;
        let legend_gap_top = 56.0;
        let legend_h = 116.0;
        let legend_text_x = 40.0;
        let legend_text_y = 36.0;
        let grid_gap_top = 56.0;
        let col_gap = 40.0;
        let card_h_min = 440.0;
        let card_h_max = 720.0;
        let card_title_div_y = 92.0;
        let card_title_x = 36.0;
        let card_title_y = 24.0;
        let card_hint_rx = 292.0;
        let hint_clamp_w = 264.0;
        let entry_row_h = 104.0;
        let key_col_ratio = 0.52_f32;
        let key_col_min = 440.0;
        let key_col_max = 680.0;
        let label_col_extra = 120.0;
        let label_col_min = 220.0;
        let entry_top_pad = 184.0;
        let entry_y_start = 136.0;
        let entry_key_x = 36.0;
        // let entry_key_y_adj = 6.0;
        let key_fs_ratio = 0.72_f32;
        let key_fs_min = 10.0;
        // ───────────────────────────────────────────────────────────────────────

        let mut glyphs = Vec::new();
        let mut chrome = vec![RegionDrawInstance::new(
            [
                center_bounds[0],
                center_bounds[1],
                center_bounds[2],
                center_bounds[3],
            ],
            editor_bg,
        )];

        self.editor_overlay_text_system
            .set_metrics(Metrics::new(title_size, title_size + 8.0));
        self.editor_overlay_text_system.set_size(
            Some((panel_w - hdr_wrap_margin).max(1.0)),
            Some(line_height),
        );
        glyphs.extend(layout_panel_text(
            "Netherize",
            &mut self.editor_overlay_text_system,
            &mut self.atlas,
            &self.queue,
            panel_x + hdr_logo_x,
            panel_y,
            fg,
        ));
        chrome.push(RegionDrawInstance::new(
            [
                panel_x + hdr_accent_x,
                panel_y + hdr_accent_y,
                hdr_accent_size,
                hdr_accent_size,
            ],
            accent,
        ));
        self.editor_overlay_text_system
            .set_metrics(Metrics::new(font_size, line_height));
        glyphs.extend(layout_panel_text(
            &format!("{} · {}", help.subtitle, help.profile_name.to_uppercase()),
            &mut self.editor_overlay_text_system,
            &mut self.atlas,
            &self.queue,
            panel_x + hdr_logo_x,
            panel_y + title_size + hdr_subtitle_gap,
            accent,
        ));
        self.editor_overlay_text_system
            .set_metrics(Metrics::new(small_size, small_size + 8.0));
        let meta_x = panel_x + panel_w - hdr_meta_col_w;
        for (idx, line) in [
            "leader = space".to_string(),
            "mod = ⌘ (macOS)".to_string(),
            "by nqthminhdev".to_string(),
            self.welcome_version.clone(),
        ]
        .iter()
        .enumerate()
        {
            glyphs.extend(layout_panel_text(
                line,
                &mut self.editor_overlay_text_system,
                &mut self.atlas,
                &self.queue,
                meta_x,
                panel_y + idx as f32 * (small_size + hdr_meta_lh_gap),
                fg_dim,
            ));
        }
        let header_bottom =
            (panel_y + title_size + line_height + hdr_title_gap).max(panel_y + hdr_min_h);
        chrome.push(RegionDrawInstance::new(
            [panel_x, header_bottom, panel_w, 1.0],
            divider,
        ));

        self.editor_overlay_text_system
            .set_metrics(Metrics::new(font_size, line_height));
        let legend_y = header_bottom + legend_gap_top;
        chrome.push(RegionDrawInstance::new(
            [panel_x, legend_y, panel_w, legend_h],
            card_border,
        ));
        chrome.push(RegionDrawInstance::new(
            [
                panel_x + 1.0,
                legend_y + 1.0,
                (panel_w - 2.0).max(1.0),
                legend_h - 2.0,
            ],
            card_bg,
        ));
        glyphs.extend(layout_panel_text(
            &format!(
                "Legend: spc = leader (Space)  ·  mod = ⌘ (Cmd)  ·  Ctrl = Control  ·  K = key chord  ·  Source: {}  ·  Profile: {}",
                help.source_label, help.profile_name
            ),
            &mut self.editor_overlay_text_system,
            &mut self.atlas,
            &self.queue,
            panel_x + legend_text_x,
            legend_y + legend_text_y,
            fg_dim,
        ));

        let grid_top = legend_y + legend_h + grid_gap_top;
        let columns = if panel_w > 1680.0 {
            4
        } else if panel_w > 1180.0 {
            3
        } else {
            2
        };
        let card_w = (panel_w - col_gap * (columns as f32 - 1.0)) / columns as f32;
        let available_grid_h = (panel_y + panel_h - grid_top).max(180.0);
        let visible_rows = ((help.sections.len() + columns - 1) / columns).max(1) as f32;
        let card_h = ((available_grid_h - col_gap * (visible_rows - 1.0)) / visible_rows)
            .clamp(card_h_min, card_h_max);
        for (idx, section) in help.sections.iter().enumerate() {
            let col = idx % columns;
            let row = idx / columns;
            let x = panel_x + col as f32 * (card_w + col_gap);
            let y = grid_top + row as f32 * (card_h + col_gap);
            if y > panel_y + panel_h - 40.0 {
                break;
            }
            chrome.push(RegionDrawInstance::new([x, y, card_w, card_h], card_border));
            chrome.push(RegionDrawInstance::new(
                [
                    x + 1.0,
                    y + 1.0,
                    (card_w - 2.0).max(1.0),
                    (card_h - 2.0).max(1.0),
                ],
                card_bg,
            ));
            chrome.push(RegionDrawInstance::new(
                [x + 1.0, y + card_title_div_y, (card_w - 2.0).max(1.0), 1.0],
                divider,
            ));
            let section_color = if section.title == "INSERT" {
                self.theme.ui.info.as_f32()
            } else if section.title == "PALETTE" || section.title == "VISUAL" {
                self.theme.ui.warning.as_f32()
            } else if section.title == "GLOBAL" {
                self.theme.ui.error.as_f32()
            } else {
                accent
            };
            glyphs.extend(layout_panel_text(
                &format!("● {}", section.title),
                &mut self.editor_overlay_text_system,
                &mut self.atlas,
                &self.queue,
                x + card_title_x,
                y + card_title_y,
                section_color,
            ));
            glyphs.extend(layout_panel_text(
                &clamp_monospace_text(&section.mode_hint, hint_clamp_w, font_size),
                &mut self.editor_overlay_text_system,
                &mut self.atlas,
                &self.queue,
                x + card_w - card_hint_rx,
                y + card_title_y,
                fg_ghost,
            ));
            let key_col_w = (card_w * key_col_ratio).clamp(key_col_min, key_col_max);
            let label_col_w = (card_w - key_col_w - label_col_extra).max(label_col_min);
            let max_entries = ((card_h - entry_top_pad) / entry_row_h).floor().max(1.0) as usize;
            for (entry_idx, entry) in section.entries.iter().take(max_entries).enumerate() {
                let ey = y + entry_y_start + entry_idx as f32 * entry_row_h;
                let key_refs: Vec<&str> = entry.keys.iter().map(String::as_str).collect();
                let keycaps_w = estimate_help_keycaps_width(
                    &key_refs,
                    (font_size * key_fs_ratio).max(key_fs_min),
                );
                let label_x = x + entry_key_x + key_col_w.max(keycaps_w + 20.0);
                glyphs.extend(layout_help_keycaps(
                    &key_refs,
                    &mut self.editor_overlay_text_system,
                    &mut self.atlas,
                    &self.queue,
                    &mut chrome,
                    x + entry_key_x,
                    ey,
                    (font_size * key_fs_ratio).max(key_fs_min),
                    entry_row_h,
                    fg,
                    fg_dim,
                    accent,
                    self.theme.ui.info.as_f32(),
                    self.theme.ui.warning.as_f32(),
                    self.theme.ui.error.as_f32(),
                    panel_bg,
                ));
                glyphs.extend(layout_panel_text(
                    &clamp_monospace_text(&entry.label, label_col_w, font_size),
                    &mut self.editor_overlay_text_system,
                    &mut self.atlas,
                    &self.queue,
                    label_x,
                    ey,
                    fg_dim,
                ));
            }
        }

        if help.sections.is_empty() {
            self.editor_overlay_text_system
                .set_metrics(Metrics::new(font_size, line_height));
            glyphs.extend(layout_panel_text(
                "No keymap bindings were loaded.",
                &mut self.editor_overlay_text_system,
                &mut self.atlas,
                &self.queue,
                panel_x + legend_text_x,
                grid_top,
                fg_dim,
            ));
        }

        self.editor_overlay_chrome_instances = chrome;
        self.editor_overlay_glyph_instances = glyphs;
        self.editor_overlay_text_pipeline.upload_instances(
            &self.device,
            &self.queue,
            &self.editor_overlay_glyph_instances,
        );
    }
}
