#![allow(unused_imports)]

use crate::{
    app::{app_state::AppState, command_palette::CommandPaletteRenderModel, input::LeapTarget},
    render::{
        glyph_instance::GlyphInstance, icon_pipeline::IconDrawInstance,
        region_pipeline::RegionDrawInstance, renderer::Renderer,
    },
};

use super::{
    PALETTE_FOOTER_TOP_PAD, PALETTE_HEADER_BOTTOM_PAD, PaletteFooterAction,
    palette_footer_content_height, palette_footer_height, push_palette_icon_or_badge,
    render_palette_badge, render_palette_chrome, render_palette_footer, render_palette_selection,
};

use super::super::{
    components::{PrefixIconBadge, PrefixIconBadgeChrome, layout_prefix_icon_badge},
    helpers::{
        clamp_monospace_text, estimate_monospace_width, gutter_width_for_editor, layout_panel_text,
        rect_to_scissor,
    },
};

impl Renderer {
    pub(super) fn render_file_picker_complex(&mut self, model: &CommandPaletteRenderModel) {
        let [panel_x, panel_y, panel_w, panel_h] = model.panel_bounds;
        self.palette_scissor = rect_to_scissor(model.panel_bounds);

        let inner_width = (panel_w - model.panel_padding * 2.0).max(1.0);
        self.palette_text_system
            .set_size(Some(inner_width), Some(model.line_height));

        let mut quads: Vec<RegionDrawInstance> = Vec::new();
        let mut glyphs: Vec<GlyphInstance> = Vec::new();
        let mut icons: Vec<IconDrawInstance> = Vec::new();

        let font_size = self.theme.ui.sidebar_font_size;
        let line_h = model.line_height.max(18.0);
        let row_v_pad =
            if model.mode == crate::app::command_palette::CommandPaletteMode::DocumentSymbols {
                8.0
            } else {
                4.0
            };
        let is_file_picker =
            model.mode == crate::app::command_palette::CommandPaletteMode::FilePicker;
        let row_h = model.row_height.max(1.0);
        let text_x = panel_x + model.panel_padding + 8.0;

        let char_w = font_size * 0.62;
        let file_badge_size = (row_h * 0.58).clamp(30.0, 42.0);
        let _icon_col_w = if is_file_picker {
            file_badge_size + 18.0
        } else {
            char_w + 2.0 + 4.0 + 4.0 * char_w + 24.0
        };

        // Chrome
        render_palette_chrome(model, &mut quads);

        // ── HEADER ─────────────────────────────────────────────────────────────
        let mut row_top = panel_y + model.panel_padding;

        let (badge_w, badge_h, badge_glyphs) = render_palette_badge(
            &model.title,
            &mut self.palette_text_system,
            &mut self.atlas,
            &self.queue,
            &mut quads,
            text_x,
            row_top,
            font_size,
            line_h,
            model.success_color,
            self.theme.ui.bg.as_f32(),
        );
        glyphs.extend(badge_glyphs);

        let query_x = text_x + badge_w + 10.0;
        let query_y = row_top + ((badge_h - line_h) * 0.5).max(0.0);
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
            query_y,
            model.hint_color,
        ));
        glyphs.extend(layout_panel_text(
            &model.prompt_query,
            &mut self.palette_text_system,
            &mut self.atlas,
            &self.queue,
            query_x + prefix_w,
            query_y,
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
            query_y,
            model.hint_color,
        ));

        // Vim mode indicator, drawn to the left of the result count.
        if let Some(label) = model.vim_mode_label {
            let vim_w = label.chars().count() as f32 * font_size * 0.60;
            let vim_x = (count_x - vim_w - 16.0).max(query_x + prefix_w);
            glyphs.extend(layout_panel_text(
                label,
                &mut self.palette_text_system,
                &mut self.atlas,
                &self.queue,
                vim_x,
                query_y,
                model.vim_mode_color.unwrap_or(model.match_color),
            ));
        }
        row_top += badge_h + PALETTE_HEADER_BOTTOM_PAD;

        quads.push(RegionDrawInstance::new(
            [panel_x, row_top, panel_w, 1.0],
            model.border_color,
        ));
        row_top += 6.0;

        // ── BODY ───────────────────────────────────────────────────────────────
        let footer_h = palette_footer_height(line_h);
        let body_h = (panel_h - (row_top - panel_y) - footer_h - model.panel_padding).max(0.0);
        let max_visible = ((body_h / row_h).floor() as usize).max(1);
        let scroll_offset = model
            .scroll_offset_rows
            .min(model.result_labels.len().saturating_sub(max_visible));

        // Group results by parent folder for file picker modes
        let mut current_folder: Option<String> = None;
        let mut rendered_rows = 0usize;

        for (absolute_idx, (label, ranges)) in model
            .result_labels
            .iter()
            .zip(model.result_match_ranges.iter())
            .enumerate()
            .skip(scroll_offset)
        {
            if rendered_rows >= max_visible {
                break;
            }

            // Determine folder for this item (only in FilePicker mode)
            let folder = if is_file_picker {
                let file_path = label.split(':').next().unwrap_or(label);
                std::path::Path::new(file_path)
                    .parent()
                    .and_then(|p| p.to_str())
                    .map(|s| s.to_string())
            } else {
                None
            };

            // Render group header when folder changes
            let is_collapsed = if let Some(ref folder_str) = folder {
                let collapsed = model.collapsed_paths.contains(folder_str);
                if current_folder.as_ref() != Some(folder_str) {
                    // New group header
                    if rendered_rows >= max_visible {
                        break;
                    }
                    current_folder = Some(folder_str.clone());

                    let header_bg = [
                        model.panel_bg[0] * 1.05,
                        model.panel_bg[1] * 1.05,
                        model.panel_bg[2] * 1.05,
                        model.panel_bg[3],
                    ];
                    quads.push(RegionDrawInstance::new(
                        [panel_x, row_top, panel_w, row_h],
                        header_bg,
                    ));

                    let arrow = if collapsed { "▸" } else { "▾" };
                    let header_text = format!("{} {}", arrow, folder_str);
                    let header_color = model.hint_color;
                    let header_y = row_top + (row_h - line_h) * 0.5;
                    glyphs.extend(layout_panel_text(
                        &header_text,
                        &mut self.palette_text_system,
                        &mut self.atlas,
                        &self.queue,
                        text_x,
                        header_y,
                        header_color,
                    ));

                    row_top += row_h;
                    rendered_rows += 1;

                    if rendered_rows >= max_visible {
                        break;
                    }
                }
                collapsed
            } else {
                false
            };

            // Skip items in collapsed groups
            if is_collapsed {
                continue;
            }

            if absolute_idx == model.selected_index {
                render_palette_selection(model, &mut quads, row_top, row_h);
            }
            if rendered_rows > 0 {
                let mut sep = model.border_color;
                sep[3] *= 0.30;
                quads.push(RegionDrawInstance::new(
                    [text_x, row_top, inner_width - 8.0, 1.0],
                    sep,
                ));
            }

            self.palette_text_system
                .set_size(Some(inner_width), Some(line_h));
            let tone = model
                .item_tones
                .get(absolute_idx)
                .copied()
                .unwrap_or_default();
            let mut row_model = model.clone();
            row_model.label_color = file_picker_tone_color(tone, model);

            let (label_text, label_ranges, label_x, label_y) =
                if model.mode == crate::app::command_palette::CommandPaletteMode::DocumentSymbols {
                    let (badge, stripped_label) = split_symbol_badge(label);
                    let badge_color = file_picker_tone_color(tone, model);
                    let badge_size = (row_h * 0.58).clamp(30.0, 42.0);
                    let badge_x = text_x;
                    let badge_y = row_top + (row_h - badge_size) * 0.5;
                    push_palette_icon_or_badge(
                        &badge,
                        badge_color,
                        model.panel_bg,
                        [badge_x, badge_y, badge_size, badge_size],
                        0.82,
                        PrefixIconBadgeChrome::None,
                        &mut self.palette_text_system,
                        &mut self.atlas,
                        &self.queue,
                        &mut quads,
                        &mut glyphs,
                        &mut icons,
                    );
                    self.palette_text_system
                        .set_metrics(cosmic_text::Metrics::new(font_size, line_h));
                    self.palette_text_system.set_size(
                        Some((inner_width - badge_size - 12.0).max(1.0)),
                        Some(line_h),
                    );
                    (
                        stripped_label,
                        shift_ranges_after_badge(label, ranges),
                        text_x + badge_size + 18.0,
                        row_top + (row_h - line_h) * 0.5,
                    )
                } else {
                    let file_path = label.split(':').next().unwrap_or(label);
                    let filename = std::path::Path::new(file_path)
                        .file_name()
                        .and_then(|name| name.to_str())
                        .unwrap_or(file_path);
                    let badge = self.theme.get_icon_for_file(filename, false);
                    // Bearded-icons are full-color SVGs — use white tint so the
                    // icon shader multiplies by 1.0 and preserves the original
                    // colors (same as the sidebar does).
                    let icon_tint = [1.0_f32; 4];
                    let badge_size = file_badge_size;
                    let badge_x = text_x;
                    let badge_y = row_top + (row_h - badge_size) * 0.5;
                    push_palette_icon_or_badge(
                        badge,
                        icon_tint,
                        model.panel_bg,
                        [badge_x, badge_y, badge_size, badge_size],
                        0.82,
                        PrefixIconBadgeChrome::None,
                        &mut self.palette_text_system,
                        &mut self.atlas,
                        &self.queue,
                        &mut quads,
                        &mut glyphs,
                        &mut icons,
                    );
                    self.palette_text_system
                        .set_metrics(cosmic_text::Metrics::new(font_size, line_h));
                    self.palette_text_system.set_size(
                        Some((inner_width - badge_size - 18.0).max(1.0)),
                        Some(line_h),
                    );
                    (
                        label.clone(),
                        ranges.clone(),
                        text_x + badge_size + 18.0,
                        row_top + (row_h - line_h) * 0.5,
                    )
                };

            Self::render_highlighted_label(
                &label_text,
                &label_ranges,
                label_x,
                label_y,
                font_size,
                &row_model,
                &mut self.palette_text_system,
                &mut self.atlas,
                &self.queue,
                &mut glyphs,
            );
            row_top += row_h;
            rendered_rows += 1;
        }

        if model.result_labels.is_empty()
            && model.mode != crate::app::command_palette::CommandPaletteMode::FilePicker
        {
            let empty_label = if model.is_loading {
                "  Loading..."
            } else {
                "  (no results — type to search)"
            };
            let empty_color = if model.is_loading {
                model.amber_color
            } else {
                model.hint_color
            };
            glyphs.extend(layout_panel_text(
                empty_label,
                &mut self.palette_text_system,
                &mut self.atlas,
                &self.queue,
                text_x,
                row_top + row_v_pad,
                empty_color,
            ));
        }

        // ── FOOTER ─────────────────────────────────────────────────────────────
        let footer_y = panel_y + panel_h - footer_h;
        quads.push(RegionDrawInstance::new(
            [panel_x, footer_y, panel_w, 1.0],
            model.border_color,
        ));
        glyphs.extend(render_palette_footer(
            model,
            &mut self.palette_text_system,
            &mut self.atlas,
            &self.queue,
            &mut quads,
            text_x,
            footer_y + PALETTE_FOOTER_TOP_PAD,
            font_size,
            palette_footer_content_height(footer_h),
            &[
                PaletteFooterAction {
                    keys: &["↑↓"],
                    label: "navigate",
                },
                PaletteFooterAction {
                    keys: &["↵"],
                    label: "open",
                },
                PaletteFooterAction {
                    keys: &["󱊷"],
                    label: "close",
                },
            ],
        ));

        self.palette_chrome_instances = quads;
        self.palette_icon_instances = icons;
        self.palette_icon_pipeline.upload_instances(
            &self.device,
            &self.palette_icon_instances,
            [
                self.surface_state.config.width,
                self.surface_state.config.height,
            ],
        );
        self.palette_glyph_instances = glyphs;
    }

    // ── Name/path picker (Recent Projects + Theme Selector) ───────────────────
}

fn split_symbol_badge(label: &str) -> (String, String) {
    let Some(rest) = label.strip_prefix('[') else {
        return ("·".to_string(), label.to_string());
    };
    let Some(end) = rest.find(']') else {
        return ("·".to_string(), label.to_string());
    };

    let badge = rest[..end].to_string();
    let stripped = rest[end + 1..].trim_start().to_string();
    (badge, stripped)
}

fn shift_ranges_after_badge(label: &str, ranges: &[(usize, usize)]) -> Vec<(usize, usize)> {
    let offset = label
        .strip_prefix('[')
        .and_then(|rest| rest.find(']').map(|end| end + 2))
        .unwrap_or(0);

    ranges
        .iter()
        .filter_map(|(start, end)| {
            if *end <= offset {
                None
            } else {
                Some((start.saturating_sub(offset), end.saturating_sub(offset)))
            }
        })
        .collect()
}

fn file_picker_tone_color(
    tone: crate::app::command_palette::CommandPaletteItemTone,
    model: &CommandPaletteRenderModel,
) -> [f32; 4] {
    match tone {
        crate::app::command_palette::CommandPaletteItemTone::Function => model.cyan_color,
        crate::app::command_palette::CommandPaletteItemTone::Type => model.magenta_color,
        crate::app::command_palette::CommandPaletteItemTone::Variable => model.info_color,
        crate::app::command_palette::CommandPaletteItemTone::Module => model.amber_color,
        _ => model.label_color,
    }
}
