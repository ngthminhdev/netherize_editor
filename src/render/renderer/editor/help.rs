#![allow(unused_imports)]

use std::sync::OnceLock;

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

fn cheat_sheet_logo_rgba() -> Option<&'static (u32, u32, Vec<u8>)> {
    static LOGO: OnceLock<Option<(u32, u32, Vec<u8>)>> = OnceLock::new();
    LOGO.get_or_init(|| {
        image::load_from_memory(include_bytes!("../../../../assets/app_logo.png"))
            .ok()
            .map(|image| {
                let rgba = image.to_rgba8();
                let width = rgba.width();
                let height = rgba.height();
                (width, height, rgba.into_raw())
            })
    })
    .as_ref()
}

const HELP_CARD_MIN_WIDTH: f32 = 680.0;
const HELP_CARD_GAP: f32 = 40.0;
const HELP_CARD_MIN_HEIGHT: f32 = 220.0;
const HELP_CARD_ENTRY_START: f32 = 136.0;
const HELP_CARD_ENTRY_ROW_HEIGHT: f32 = 52.0;
const HELP_CARD_BOTTOM_PADDING: f32 = 28.0;

#[derive(Debug, Clone, Copy)]
struct HelpCardLayout {
    x: f32,
    y: f32,
    width: f32,
    height: f32,
}

#[derive(Debug, Clone)]
struct HelpGridLayout {
    columns: usize,
    cards: Vec<HelpCardLayout>,
    height: f32,
}

fn help_card_height(entry_count: usize, scale: f32) -> f32 {
    let scale = scale.max(0.5);
    (HELP_CARD_ENTRY_START * scale
        + entry_count as f32 * HELP_CARD_ENTRY_ROW_HEIGHT * scale
        + HELP_CARD_BOTTOM_PADDING * scale)
        .max(HELP_CARD_MIN_HEIGHT * scale)
}

fn help_grid_layout(entry_counts: &[usize], panel_width: f32, scale: f32) -> HelpGridLayout {
    let scale = scale.max(0.5);
    let gap = HELP_CARD_GAP * scale;
    let max_columns = entry_counts.len().max(1).min(4);
    let columns = (((panel_width + gap) / (HELP_CARD_MIN_WIDTH * scale + gap)).floor() as usize)
        .clamp(1, max_columns);
    let card_width = ((panel_width - gap * (columns as f32 - 1.0)) / columns as f32).max(1.0);
    let mut column_heights = vec![0.0_f32; columns];
    let mut cards = Vec::with_capacity(entry_counts.len());

    for entry_count in entry_counts {
        let column = column_heights
            .iter()
            .enumerate()
            .min_by(|left, right| left.1.total_cmp(right.1))
            .map(|(index, _)| index)
            .unwrap_or(0);
        let height = help_card_height(*entry_count, scale);
        cards.push(HelpCardLayout {
            x: column as f32 * (card_width + gap),
            y: column_heights[column],
            width: card_width,
            height,
        });
        column_heights[column] += height + gap;
    }

    let height = (column_heights.into_iter().fold(0.0_f32, f32::max) - gap).max(0.0);
    HelpGridLayout {
        columns,
        cards,
        height,
    }
}

impl Renderer {
    pub fn update_help_buffer_content(&mut self, help: &HelpState, center_bounds: [f32; 4]) -> f32 {
        if center_bounds[2] < 1.0 || center_bounds[3] < 1.0 {
            self.image_pipeline.clear();
            self.image_scissor = None;
            self.clear_editor_overlays();
            return 0.0;
        }

        // Scale hardcoded chrome px so layout tracks the runtime-scaled text
        // metrics across monitors (same pattern as extensions.rs).
        let s = self.ui_scale.max(0.5);
        let font_size = self.theme.editor.font_size.max(14.0 * s);
        let title_size = (font_size * 2.2).max(28.0 * s);
        let small_size = (font_size * 0.82).max(11.0 * s);
        let line_height = self.theme.editor.line_height.max(font_size + 5.0 * s);
        self.editor_overlay_text_system
            .set_metrics(Metrics::new(font_size, line_height));
        self.editor_overlay_scissor = rect_to_scissor(center_bounds);

        // Keep the Cheat Sheet page on the same coordinate system as Settings:
        // both special buffer tabs should occupy the center editor region with
        // identical inset behavior instead of using independent full-screen
        // margins.
        let pad_x = self.editor_padding_x.max(18.0 * s);
        let pad_y = self.editor_padding_y.max(18.0 * s);
        let panel_x = center_bounds[0] + pad_x;
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

        // ── sizing constants (tweak here; all device px scaled by ui_scale) ────
        let hdr_logo_x = 176.0 * s;
        let hdr_accent_x = 24.0 * s;
        let hdr_accent_y = 24.0 * s;
        let hdr_accent_size = 120.0 * s;
        let hdr_subtitle_gap = 24.0 * s;
        let narrow_header = panel_w < 1100.0 * s;
        let hdr_meta_col_w = 420.0 * s;
        let hdr_meta_lh_gap = 28.0 * s;
        let hdr_wrap_margin = 56.0 * s;
        let hdr_title_gap = 64.0 * s;
        let hdr_min_h = if narrow_header { 360.0 * s } else { 208.0 * s };
        let legend_gap_top = 56.0 * s;
        let legend_h = if narrow_header { 160.0 * s } else { 116.0 * s };
        let legend_text_x = 40.0 * s;
        let legend_text_y = 36.0 * s;
        let grid_gap_top = 56.0 * s;
        let card_title_div_y = 92.0 * s;
        let card_title_x = 36.0 * s;
        let card_title_y = 24.0 * s;
        let entry_row_h = HELP_CARD_ENTRY_ROW_HEIGHT * s;
        let key_col_ratio = 0.38_f32;
        let key_col_min = 180.0 * s;
        let key_col_max = 320.0 * s;
        let label_col_extra = 80.0 * s;
        let label_col_min = 140.0 * s;
        let entry_y_start = HELP_CARD_ENTRY_START * s;
        let entry_key_x = 36.0 * s;
        let key_fs_ratio = 0.72_f32;
        let key_fs_min = 10.0 * s;
        // ───────────────────────────────────────────────────────────────────────

        let header_height = (title_size + line_height + hdr_title_gap).max(hdr_min_h);
        let legend_y_offset = header_height + legend_gap_top;
        let grid_top_offset = legend_y_offset + legend_h + grid_gap_top;
        let entry_counts = help
            .sections
            .iter()
            .map(|section| section.entries.len())
            .collect::<Vec<_>>();
        let grid_layout = help_grid_layout(&entry_counts, panel_w, s);
        debug_assert!((1..=4).contains(&grid_layout.columns));
        let content_height = grid_top_offset + grid_layout.height + 32.0 * s;
        let max_scroll_y = (content_height - panel_h).max(0.0);
        let panel_y = center_bounds[1] + pad_y - help.scroll_y.min(max_scroll_y);

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

        self.image_scissor = rect_to_scissor(center_bounds);
        if let Some((logo_width, logo_height, rgba)) = cheat_sheet_logo_rgba() {
            let max_logo_w = hdr_accent_size;
            let max_logo_h = hdr_accent_size;
            let scale = (max_logo_w / *logo_width as f32)
                .min(max_logo_h / *logo_height as f32)
                .max(0.01);
            let draw_w = *logo_width as f32 * scale;
            let draw_h = *logo_height as f32 * scale;
            let rect = [
                panel_x + hdr_accent_x + (hdr_accent_size - draw_w) * 0.5,
                panel_y + hdr_accent_y + (hdr_accent_size - draw_h) * 0.5,
                draw_w,
                draw_h,
            ];
            self.image_pipeline.upload_rgba(
                &self.device,
                &self.queue,
                rgba,
                *logo_width,
                *logo_height,
                rect,
                [
                    self.surface_state.config.width,
                    self.surface_state.config.height,
                ],
            );
        } else {
            self.image_pipeline.clear();
        }

        self.editor_overlay_text_system
            .set_metrics(Metrics::new(title_size, title_size + 8.0 * s));
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
            .set_metrics(Metrics::new(small_size, small_size + 8.0 * s));
        let meta_x = if narrow_header {
            panel_x + hdr_logo_x
        } else {
            panel_x + panel_w - hdr_meta_col_w
        };
        let meta_y = if narrow_header {
            panel_y + title_size + line_height + 72.0 * s
        } else {
            panel_y
        };
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
                meta_y + idx as f32 * (small_size + hdr_meta_lh_gap),
                fg_dim,
            ));
        }
        let header_bottom = panel_y + header_height;
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
        let legend_primary = if narrow_header {
            "Legend: spc = Space  ·  mod = Cmd  ·  Ctrl = Control  ·  K = chord".to_string()
        } else {
            format!(
                "Legend: spc = leader (Space)  ·  mod = ⌘ (Cmd)  ·  Ctrl = Control  ·  K = key chord  ·  Source: {}  ·  Profile: {}",
                help.source_label, help.profile_name
            )
        };
        glyphs.extend(layout_panel_text(
            &legend_primary,
            &mut self.editor_overlay_text_system,
            &mut self.atlas,
            &self.queue,
            panel_x + legend_text_x,
            legend_y + legend_text_y,
            fg_dim,
        ));
        if narrow_header {
            glyphs.extend(layout_panel_text(
                &format!(
                    "Source: {}  ·  Profile: {}",
                    help.source_label, help.profile_name
                ),
                &mut self.editor_overlay_text_system,
                &mut self.atlas,
                &self.queue,
                panel_x + legend_text_x,
                legend_y + legend_text_y + line_height,
                fg_dim,
            ));
        }

        let grid_top = legend_y + legend_h + grid_gap_top;
        for (section, card) in help.sections.iter().zip(&grid_layout.cards) {
            let x = panel_x + card.x;
            let y = grid_top + card.y;
            let card_w = card.width;
            let card_h = card.height;
            if y + card_h < center_bounds[1] {
                // card is entirely above the visible area
                continue;
            }
            if y > center_bounds[1] + center_bounds[3] {
                continue;
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
                &clamp_monospace_text(
                    &section.mode_hint,
                    (card_w * 0.34).max(100.0 * s),
                    font_size,
                ),
                &mut self.editor_overlay_text_system,
                &mut self.atlas,
                &self.queue,
                x + card_w * 0.62,
                y + card_title_y,
                fg_ghost,
            ));
            let key_col_w = (card_w * key_col_ratio).clamp(key_col_min, key_col_max);
            let label_col_w = (card_w - key_col_w - label_col_extra).max(label_col_min);
            for (entry_idx, entry) in section.entries.iter().enumerate() {
                let ey = y + entry_y_start + entry_idx as f32 * entry_row_h;
                let key_refs: Vec<&str> = entry.keys.iter().map(String::as_str).collect();
                let keycaps_w = estimate_help_keycaps_width(
                    &key_refs,
                    (font_size * key_fs_ratio).max(key_fs_min),
                );
                let label_x = x + entry_key_x + key_col_w.max(keycaps_w + 20.0 * s);
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
        max_scroll_y
    }
}

#[cfg(test)]
mod tests {
    use super::{help_card_height, help_grid_layout};

    #[test]
    fn help_layout_uses_one_column_for_a_narrow_panel() {
        let layout = help_grid_layout(&[8, 9, 3], 720.0, 1.0);

        assert_eq!(layout.columns, 1);
        assert_eq!(layout.cards.len(), 3);
        assert!(layout.cards[1].y >= layout.cards[0].y + layout.cards[0].height);
        assert!(layout.cards[2].y >= layout.cards[1].y + layout.cards[1].height);
    }

    #[test]
    fn help_card_height_grows_to_render_every_entry() {
        let short = help_card_height(2, 1.0);
        let normal_mode = help_card_height(85, 1.0);

        assert!(normal_mode > short);
        assert!(normal_mode >= 85.0 * 44.0);
    }
}
