#![allow(unused_imports)]

use crate::{
    app::{app_state::AppState, command_palette::CommandPaletteRenderModel, input::LeapTarget},
    render::{
        glyph_instance::GlyphInstance, icon_pipeline::IconDrawInstance,
        region_pipeline::RegionDrawInstance, renderer::Renderer,
    },
};

use super::super::{
    components::{PrefixIconBadge, PrefixIconBadgeChrome, layout_prefix_icon_badge},
    helpers::{clamp_monospace_text, estimate_monospace_width, layout_panel_text, rect_to_scissor},
};
use super::{
    PALETTE_FOOTER_TOP_PAD, PALETTE_HEADER_BOTTOM_PAD, PaletteFooterAction,
    palette_footer_content_height, palette_footer_height, push_palette_icon_or_badge,
    render_palette_badge, render_palette_chrome, render_palette_footer, render_palette_selection,
};

impl Renderer {
    pub(super) fn render_recent_projects(&mut self, model: &CommandPaletteRenderModel) {
        let [panel_x, panel_y, panel_w, panel_h] = model.panel_bounds;
        self.palette_scissor = rect_to_scissor(model.panel_bounds);

        let inner_width = (panel_w - model.panel_padding * 2.0).max(1.0);
        self.palette_text_system
            .set_size(Some(inner_width), Some(model.line_height));

        let mut quads: Vec<RegionDrawInstance> = Vec::new();
        let mut glyphs: Vec<GlyphInstance> = Vec::new();
        let mut icons: Vec<IconDrawInstance> = Vec::new();

        // Theme Selector shares every metric with Recent Projects (font, row
        // padding, text inset) so both pickers read as the same component;
        // only the row *content* differs (swatch strip vs project icon).
        let is_theme_selector =
            model.mode == crate::app::command_palette::CommandPaletteMode::ThemeSelector;
        let font_size = self.theme.ui.sidebar_font_size;
        let char_w = font_size * 0.62;
        let line_h = model.line_height.max(18.0);
        let row_v_pad = 9.0;
        let row_h = if is_theme_selector {
            model.row_height
        } else {
            model.row_height.max(line_h * 2.0 + row_v_pad * 2.0 + 2.0)
        };
        let text_x = panel_x + model.panel_padding + 8.0;

        // Theme Selector keeps the existing rich browser row. Recent Projects uses
        // the same renderer/chrome pipeline but upgrades to a project launcher row.

        // Chrome
        render_palette_chrome(model, &mut quads);

        // ── Header ─────────────────────────────────────────────────────────────
        let mut row_top = panel_y + model.panel_padding;

        let badge_color = if is_theme_selector {
            model.match_color
        } else {
            model.info_color
        };
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
            badge_color,
            self.theme.ui.bg.as_f32(),
        );
        glyphs.extend(badge_glyphs);
        let header_text_y = row_top + ((badge_h - line_h) * 0.5).max(0.0);

        // Search query input next to the title badge — the picker opens in Insert
        // mode so typing filters immediately. Shows the dimmed placeholder hint
        // until the user types (mirrors the file picker / live grep header).
        let query_x = text_x + badge_w + 12.0;
        let prefix_w = model.prompt_prefix.chars().count() as f32 * font_size * 0.60;
        glyphs.extend(layout_panel_text(
            &model.prompt_prefix,
            &mut self.palette_text_system,
            &mut self.atlas,
            &self.queue,
            query_x,
            header_text_y,
            model.hint_color,
        ));
        let query_typed = model.result_match_ranges.iter().any(|r| !r.is_empty());
        let query_col = if query_typed {
            model.text_color
        } else {
            model.hint_color
        };
        glyphs.extend(layout_panel_text(
            &model.prompt_query,
            &mut self.palette_text_system,
            &mut self.atlas,
            &self.queue,
            query_x + prefix_w,
            header_text_y,
            query_col,
        ));

        let count_text = if is_theme_selector && !model.result_labels.is_empty() {
            format!(
                "{}/{}",
                model
                    .selected_index
                    .saturating_add(1)
                    .min(model.result_labels.len()),
                model.total_results
            )
        } else if is_theme_selector {
            format!("{}", model.result_labels.len())
        } else {
            format!("{} projects", model.result_labels.len())
        };
        let count_w = count_text.chars().count() as f32 * font_size * 0.60;
        let count_x =
            (panel_x + panel_w - model.panel_padding - count_w).max(text_x + badge_w + 8.0);
        glyphs.extend(layout_panel_text(
            &count_text,
            &mut self.palette_text_system,
            &mut self.atlas,
            &self.queue,
            count_x,
            header_text_y,
            model.hint_color,
        ));
        if !is_theme_selector
            && let Some(vim_text) = recent_projects_vim_mode_status(model.vim_mode_label)
        {
            let vim_w = estimate_monospace_width(vim_text, font_size);
            let vim_x = (count_x - vim_w - 16.0).max(text_x + badge_w + 16.0);
            glyphs.extend(layout_panel_text(
                vim_text,
                &mut self.palette_text_system,
                &mut self.atlas,
                &self.queue,
                vim_x,
                header_text_y,
                model.vim_mode_color.unwrap_or(model.match_color),
            ));
        }
        row_top += badge_h + PALETTE_HEADER_BOTTOM_PAD;

        quads.push(RegionDrawInstance::new(
            [panel_x, row_top, panel_w, 1.0],
            model.border_color,
        ));
        row_top += 6.0;

        // ── Column header labels ────────────────────────────────────────────────
        let empty_text = if is_theme_selector {
            "  (no themes found in repo/user theme folders)"
        } else {
            let mut col_header_color = model.hint_color;
            col_header_color[3] *= 0.7;
            // Start the column header at the same x as the row names (after the
            // project-icon slot) so the header lines up with the column below it,
            // mirroring the right-aligned "LAST OPENED" header.
            let header_badge_w = (row_h * 0.58).clamp(30.0, 42.0);
            let name_col_x = text_x + header_badge_w + 14.0;
            glyphs.extend(layout_panel_text(
                "RECENT PROJECTS",
                &mut self.palette_text_system,
                &mut self.atlas,
                &self.queue,
                name_col_x,
                row_top,
                col_header_color,
            ));
            let last_opened_header = "LAST OPENED";
            let last_opened_w = estimate_monospace_width(last_opened_header, font_size);
            glyphs.extend(layout_panel_text(
                last_opened_header,
                &mut self.palette_text_system,
                &mut self.atlas,
                &self.queue,
                panel_x + panel_w - model.panel_padding - last_opened_w,
                row_top,
                col_header_color,
            ));
            row_top += line_h;

            quads.push(RegionDrawInstance::new(
                [panel_x, row_top, panel_w, 1.0],
                model.border_color,
            ));
            row_top += 4.0;

            "  (no recent projects — open a folder with Cmd+O)"
        };

        // ── Body rows ──────────────────────────────────────────────────────────
        let footer_h = palette_footer_height(line_h);
        let body_h = (panel_h - (row_top - panel_y) - footer_h - model.panel_padding).max(0.0);
        let max_visible = ((body_h / row_h).floor() as usize).max(1);
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
            let raw_secondary = model
                .secondary_labels
                .get(absolute_idx)
                .unwrap_or(&empty_str);
            let recent_meta = parse_recent_secondary(raw_secondary);
            let full_path = recent_meta.path.as_str();

            let is_selected = absolute_idx == model.selected_index;
            if is_selected {
                render_palette_selection(model, &mut quads, row_top, row_h);
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

            if is_theme_selector {
                // Swatch strip fills the same leading slot the project icon
                // occupies in Recent Projects, so both pickers line up.
                let swatch_size = 16.0;
                let swatch_stride = 22.0;
                let swatch_y = row_top + (row_h - swatch_size) * 0.5;
                let fallback_swatches = theme_preview_colors(name, model);
                let swatches = model
                    .item_preview_colors
                    .get(absolute_idx)
                    .filter(|colors| !colors.is_empty())
                    .map(Vec::as_slice)
                    .unwrap_or(&fallback_swatches);
                for (swatch_idx, color) in swatches.iter().take(6).enumerate() {
                    quads.push(
                        RegionDrawInstance::new(
                            [
                                text_x + swatch_idx as f32 * swatch_stride,
                                swatch_y,
                                swatch_size,
                                swatch_size,
                            ],
                            *color,
                        )
                        .with_radius(swatch_size * 0.5),
                    );
                }

                let name_x = text_x + 6.0 * swatch_stride + 14.0;
                Self::render_highlighted_label(
                    name,
                    ranges,
                    name_x,
                    label_y,
                    font_size,
                    model,
                    &mut self.palette_text_system,
                    &mut self.atlas,
                    &self.queue,
                    &mut glyphs,
                );
            } else {
                let full_path_buf = std::path::Path::new(full_path);
                let inferred_icon_source =
                    crate::app::persistence::AppPersistentState::infer_project_icon_source(
                        full_path_buf,
                    );
                let icon_source = recent_meta
                    .icon_source
                    .as_deref()
                    .unwrap_or(inferred_icon_source.as_str());
                let icon_name = std::path::Path::new(icon_source)
                    .file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or(name);
                let file_icon = self.theme.icon_theme_for_filename(icon_name, false);
                let badge = file_icon.glyph.as_str();
                let badge_color = file_icon.color.as_f32();
                let badge_w = (row_h * 0.58).clamp(30.0, 42.0);
                let badge_h = badge_w;
                let badge_y = row_top + (row_h - badge_h) * 0.5;
                push_palette_icon_or_badge(
                    badge,
                    badge_color,
                    model.panel_bg,
                    [text_x, badge_y, badge_w, badge_h],
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
                self.palette_text_system
                    .set_size(Some(inner_width), Some(line_h));

                let name_x = text_x + badge_w + 14.0;
                let primary_y = row_top + row_v_pad;
                Self::render_highlighted_label(
                    name,
                    ranges,
                    name_x,
                    primary_y,
                    font_size,
                    model,
                    &mut self.palette_text_system,
                    &mut self.atlas,
                    &self.queue,
                    &mut glyphs,
                );

                let secondary_y = primary_y + line_h + 2.0;
                let last_opened = recent_meta
                    .last_opened_unix_secs
                    .map(relative_last_opened_label)
                    .unwrap_or_else(|| "recently".to_string());
                let last_opened_w = estimate_monospace_width(&last_opened, font_size);
                let last_opened_x = panel_x + panel_w - model.panel_padding - last_opened_w;
                let path_available = (last_opened_x - name_x - char_w * 2.0).max(0.0);
                let clamped_path = clamp_monospace_text(full_path, path_available, font_size);
                glyphs.extend(layout_panel_text(
                    &clamped_path,
                    &mut self.palette_text_system,
                    &mut self.atlas,
                    &self.queue,
                    name_x,
                    secondary_y,
                    model.hint_color,
                ));

                glyphs.extend(layout_panel_text(
                    &last_opened,
                    &mut self.palette_text_system,
                    &mut self.atlas,
                    &self.queue,
                    last_opened_x,
                    primary_y,
                    model.hint_color,
                ));
            }

            row_top += row_h;
        }

        if model.result_labels.is_empty() {
            glyphs.extend(layout_panel_text(
                empty_text,
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
        let enter_label = if is_theme_selector {
            "apply theme"
        } else {
            "open"
        };
        let recent_project_footer_actions = [
            PaletteFooterAction {
                keys: &["↑↓"],
                label: "navigate",
            },
            PaletteFooterAction {
                keys: &["↵"],
                label: enter_label,
            },
            PaletteFooterAction {
                keys: &["x"],
                label: "remove",
            },
            PaletteFooterAction {
                keys: &["󱊷"],
                label: "close",
            },
        ];
        let theme_footer_actions = [
            PaletteFooterAction {
                keys: &["↑↓"],
                label: "navigate",
            },
            PaletteFooterAction {
                keys: &["↵"],
                label: enter_label,
            },
            PaletteFooterAction {
                keys: &["󱊷"],
                label: "close",
            },
        ];
        let footer_actions: &[PaletteFooterAction<'_>] = if is_theme_selector {
            &theme_footer_actions
        } else {
            &recent_project_footer_actions
        };
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
            footer_actions,
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
}

struct RecentProjectRenderMeta {
    path: String,
    icon_source: Option<String>,
    last_opened_unix_secs: Option<u64>,
}

fn parse_recent_secondary(raw: &str) -> RecentProjectRenderMeta {
    let (path, meta) = raw.split_once('\u{1f}').unwrap_or((raw, ""));
    let mut icon_source = None;
    let mut last_opened_unix_secs = None;
    for part in meta.split(';') {
        if let Some(value) = part.strip_prefix("icon=").filter(|value| !value.is_empty()) {
            icon_source = Some(value.to_string());
        } else if let Some(value) = part.strip_prefix("last=") {
            last_opened_unix_secs = value.parse::<u64>().ok();
        }
    }
    RecentProjectRenderMeta {
        path: path.to_string(),
        icon_source,
        last_opened_unix_secs,
    }
}

fn recent_projects_vim_mode_status(label: Option<&'static str>) -> Option<&'static str> {
    label
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recent_projects_passes_vim_mode_label_through() {
        assert_eq!(
            recent_projects_vim_mode_status(Some("NORMAL")),
            Some("NORMAL")
        );
        assert_eq!(
            recent_projects_vim_mode_status(Some("INSERT")),
            Some("INSERT")
        );
        assert_eq!(recent_projects_vim_mode_status(None), None);
    }
}

fn relative_last_opened_label(last_opened_unix_secs: u64) -> String {
    let Ok(now) = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH) else {
        return "recently".to_string();
    };
    let elapsed = now.as_secs().saturating_sub(last_opened_unix_secs);
    if elapsed < 60 {
        "just now".to_string()
    } else if elapsed < 60 * 60 {
        format!("{} min ago", elapsed / 60)
    } else if elapsed < 60 * 60 * 24 {
        format!("{} hr ago", elapsed / (60 * 60))
    } else if elapsed < 60 * 60 * 24 * 2 {
        "yesterday".to_string()
    } else if elapsed < 60 * 60 * 24 * 7 {
        format!("{} days ago", elapsed / (60 * 60 * 24))
    } else {
        "last week".to_string()
    }
}

fn theme_preview_colors(name: &str, model: &CommandPaletteRenderModel) -> [[f32; 4]; 6] {
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    for byte in name.bytes() {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }

    let mut colors = [
        model.match_color,
        model.info_color,
        model.success_color,
        model.warning_color,
        model.text_color,
        model.label_color,
    ];

    for (idx, color) in colors.iter_mut().enumerate() {
        let shift = (idx * 9) as u32;
        let hue = ((hash >> shift) & 0xff) as f32 / 255.0;
        color[0] = (color[0] * 0.70 + hue * 0.30).clamp(0.0, 1.0);
        color[1] = (color[1] * 0.72 + (1.0 - hue) * 0.20).clamp(0.0, 1.0);
        color[2] = (color[2] * 0.74 + ((hue * 1.7) % 1.0) * 0.22).clamp(0.0, 1.0);
        color[3] = 1.0;
    }

    colors
}
