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

use super::super::helpers::{
    caret_rect_for_mode, clamp_monospace_text, estimate_monospace_width, gutter_width_for_editor,
    layout_panel_rich_text, layout_panel_text, layout_panel_text_italic, rect_to_scissor,
    should_draw_block_cursor,
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
        card_bg[3] = card_bg[3].max(0.88);

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
        self.editor_overlay_text_system
            .set_size(Some((panel_w - 28.0).max(1.0)), Some(line_height));
        glyphs.extend(layout_panel_text(
            "Netherize",
            &mut self.editor_overlay_text_system,
            &mut self.atlas,
            &self.queue,
            panel_x + 84.0,
            panel_y,
            fg,
        ));
        chrome.push(RegionDrawInstance::new(
            [panel_x + 12.0, panel_y + 14.0, 34.0, 34.0],
            accent,
        ));
        self.editor_overlay_text_system
            .set_metrics(Metrics::new(font_size, line_height));
        glyphs.extend(layout_panel_text(
            &format!("{} · {}", help.subtitle, help.profile_name.to_uppercase()),
            &mut self.editor_overlay_text_system,
            &mut self.atlas,
            &self.queue,
            panel_x + 84.0,
            panel_y + title_size + 12.0,
            accent,
        ));
        self.editor_overlay_text_system
            .set_metrics(Metrics::new(small_size, small_size + 8.0));
        let meta_x = panel_x + panel_w - 210.0;
        for (idx, line) in [
            "leader = space".to_string(),
            "mod = ⌘ (macOS)".to_string(),
            "by nqthminhdev".to_string(),
            "v0.3.1-alpha".to_string(),
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
                panel_y + idx as f32 * (small_size + 14.0),
                fg_dim,
            ));
        }
        let header_bottom = (panel_y + title_size + line_height + 32.0).max(panel_y + 104.0);
        chrome.push(RegionDrawInstance::new(
            [panel_x, header_bottom, panel_w, 1.0],
            divider,
        ));

        self.editor_overlay_text_system
            .set_metrics(Metrics::new(font_size, line_height));
        let legend_y = header_bottom + 28.0;
        chrome.push(RegionDrawInstance::new(
            [panel_x, legend_y, panel_w, 44.0],
            card_bg,
        ));
        glyphs.extend(layout_panel_text(
            &format!(
                "Legend:   spc = leader (Space)  ·  mod = ⌘ (Cmd)  ·  Ctrl = Control  ·  K = key chord       All bindings from {} · profile: {}",
                help.source_label, help.profile_name
            ),
            &mut self.editor_overlay_text_system,
            &mut self.atlas,
            &self.queue,
            panel_x + 20.0,
            legend_y + 12.0,
            fg_dim,
        ));

        let grid_top = legend_y + 76.0;
        let gap = 20.0;
        let columns = if panel_w > 1500.0 { 4 } else { 3 };
        let card_w = (panel_w - gap * (columns as f32 - 1.0)) / columns as f32;
        let available_grid_h = (panel_y + panel_h - grid_top).max(180.0);
        let visible_rows = ((help.sections.len() + columns - 1) / columns).max(1) as f32;
        let card_h =
            ((available_grid_h - gap * (visible_rows - 1.0)) / visible_rows).clamp(220.0, 360.0);
        for (idx, section) in help.sections.iter().enumerate() {
            let col = idx % columns;
            let row = idx / columns;
            let x = panel_x + col as f32 * (card_w + gap);
            let y = grid_top + row as f32 * (card_h + gap);
            if y > panel_y + panel_h - 40.0 {
                break;
            }
            chrome.push(RegionDrawInstance::new([x, y, card_w, card_h], card_bg));
            chrome.push(RegionDrawInstance::new([x, y + 42.0, card_w, 1.0], divider));
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
                x + 18.0,
                y + 12.0,
                section_color,
            ));
            glyphs.extend(layout_panel_text(
                &section.mode_hint,
                &mut self.editor_overlay_text_system,
                &mut self.atlas,
                &self.queue,
                x + card_w - 150.0,
                y + 12.0,
                fg_ghost,
            ));
            let max_entries = ((card_h - 64.0) / 28.0).floor().max(1.0) as usize;
            for (entry_idx, entry) in section.entries.iter().take(max_entries).enumerate() {
                let ey = y + 64.0 + entry_idx as f32 * 28.0;
                glyphs.extend(layout_panel_text(
                    &entry.keys.join("  "),
                    &mut self.editor_overlay_text_system,
                    &mut self.atlas,
                    &self.queue,
                    x + 22.0,
                    ey,
                    fg,
                ));
                glyphs.extend(layout_panel_text(
                    &clamp_monospace_text(&entry.label, card_w - 190.0, font_size),
                    &mut self.editor_overlay_text_system,
                    &mut self.atlas,
                    &self.queue,
                    x + 170.0,
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
                panel_x + 20.0,
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
