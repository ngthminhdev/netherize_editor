#![allow(unused_imports)]

use crate::{
    app::{app_state::AppState, command_palette::CommandPaletteRenderModel, input::LeapTarget},
    render::{
        glyph_instance::GlyphInstance, region_pipeline::RegionDrawInstance, renderer::Renderer,
    },
};

use super::super::components::{estimate_help_keycaps_width, layout_help_keycaps};
use super::super::helpers::{
    clamp_monospace_text, estimate_monospace_width, gutter_width_for_editor, layout_panel_text,
    layout_panel_text_bold, rect_to_scissor,
};
use super::{palette_scroll_offset, render_palette_chrome, render_palette_selection};

impl Renderer {
    // ── Minimalist palette (Command / Symbol / VimCommand) ─────────────────────

    fn render_confirmation_palette(&mut self, model: &CommandPaletteRenderModel) {
        let [panel_x, panel_y, panel_w, panel_h] = model.panel_bounds;
        self.palette_scissor = rect_to_scissor(model.panel_bounds);

        let inner_width = (panel_w - model.panel_padding * 2.0).max(1.0);
        self.palette_text_system
            .set_size(Some(inner_width), Some(model.line_height));

        let mut quads: Vec<RegionDrawInstance> = Vec::new();
        let mut glyphs: Vec<GlyphInstance> = Vec::new();

        let font_size = self.theme.ui.sidebar_font_size;
        let line_h = model.line_height.max(18.0);
        let text_x = panel_x + model.panel_padding + 10.0;
        let mut row_top = panel_y + model.panel_padding;

        let mut danger = model.warning_color;
        danger[3] = 0.98;
        let mut danger_border = model.warning_color;
        danger_border[3] = 0.85;
        let mut frame_border = model.border_color;
        frame_border[3] = frame_border[3].max(0.95);
        let mut inner_panel = model.panel_bg;
        inner_panel[3] = inner_panel[3].max(0.98);
        let mut hairline = model.border_color;
        hairline[3] *= 0.45;
        let mut subdued = model.hint_color;
        subdued[3] = subdued[3].max(0.82);

        let prompt = model.prompt_query.trim();
        let body_text = prompt
            .strip_suffix("(y/n)")
            .map(str::trim_end)
            .unwrap_or(prompt)
            .trim_end_matches('?')
            .trim();

        let (action_label, subject_label, descriptor) =
            if let Some(rest) = body_text.strip_prefix("Delete ") {
                (
                    "DELETE",
                    rest.trim(),
                    "This action removes the selected item immediately.",
                )
            } else if let Some(rest) = body_text.strip_prefix("Save changes to ") {
                (
                    "SAVE BEFORE CLOSE",
                    rest.trim_end_matches(" before closing").trim(),
                    "Confirm whether to save your edits before closing the buffer.",
                )
            } else if body_text.contains("OpenCode CLI not found") {
                (
                    "INSTALL CLI",
                    "opencode",
                    "The opencode CLI will be installed via the official installer script.",
                )
            } else {
                (model.title.as_str(), body_text, "Confirm this action.")
            };

        quads.push(RegionDrawInstance::new(
            model.overlay_bounds,
            model.scrim_color,
        ));
        quads.push(
            RegionDrawInstance::new(
                [panel_x - 1.0, panel_y - 1.0, panel_w + 2.0, panel_h + 2.0],
                frame_border,
            )
            .with_radius(18.0),
        );
        quads.push(RegionDrawInstance::new(model.panel_bounds, inner_panel).with_radius(17.0));

        let header_h = (line_h + 14.0).max(30.0);
        let badge_text = format!(" {} ", action_label);
        let badge_w = badge_text.chars().count() as f32 * font_size * 0.60 + 20.0;
        quads.push(
            RegionDrawInstance::new([text_x, row_top, badge_w, header_h], danger_border)
                .with_radius(header_h * 0.42),
        );
        quads.push(
            RegionDrawInstance::new(
                [
                    text_x + 1.0,
                    row_top + 1.0,
                    (badge_w - 2.0).max(1.0),
                    (header_h - 2.0).max(1.0),
                ],
                danger,
            )
            .with_radius((header_h * 0.42 - 1.0).max(4.0)),
        );
        glyphs.extend(layout_panel_text_bold(
            &badge_text,
            &mut self.palette_text_system,
            &mut self.atlas,
            &self.queue,
            text_x + 8.0,
            row_top + ((header_h - line_h) * 0.5).max(0.0),
            self.theme.ui.bg.as_f32(),
        ));

        let prompt_prefix_x = text_x + badge_w + 14.0;
        glyphs.extend(layout_panel_text(
            &model.prompt_prefix,
            &mut self.palette_text_system,
            &mut self.atlas,
            &self.queue,
            prompt_prefix_x,
            row_top + ((header_h - line_h) * 0.5).max(0.0),
            subdued,
        ));

        row_top += header_h + 14.0;
        quads.push(RegionDrawInstance::new(
            [text_x, row_top, inner_width - 20.0, 1.0],
            hairline,
        ));
        row_top += 14.0;

        glyphs.extend(layout_panel_text(
            descriptor,
            &mut self.palette_text_system,
            &mut self.atlas,
            &self.queue,
            text_x,
            row_top,
            model.hint_color,
        ));
        row_top += line_h + 4.0;

        let filename_text = format!("“{subject_label}”");
        let filename_w = estimate_monospace_width(&filename_text, font_size);
        let filename_chip_w = (filename_w + 28.0).min(inner_width - 20.0).max(160.0);
        let filename_chip_h = line_h + 16.0;
        quads.push(
            RegionDrawInstance::new(
                [text_x, row_top, filename_chip_w, filename_chip_h],
                frame_border,
            )
            .with_radius(12.0),
        );
        let mut filename_fill = model.selection_bg;
        filename_fill[3] = 0.40;
        quads.push(
            RegionDrawInstance::new(
                [
                    text_x + 1.0,
                    row_top + 1.0,
                    (filename_chip_w - 2.0).max(1.0),
                    (filename_chip_h - 2.0).max(1.0),
                ],
                filename_fill,
            )
            .with_radius(11.0),
        );
        glyphs.extend(layout_panel_text_bold(
            &filename_text,
            &mut self.palette_text_system,
            &mut self.atlas,
            &self.queue,
            text_x + 14.0,
            row_top + ((filename_chip_h - line_h) * 0.5).max(0.0),
            model.text_color,
        ));

        let content_bottom_y = row_top + filename_chip_h;
        let footer_y =
            (panel_y + panel_h - model.panel_padding - line_h - 18.0).max(content_bottom_y + 20.0);
        quads.push(RegionDrawInstance::new(
            [text_x, footer_y - 12.0, inner_width - 20.0, 1.0],
            hairline,
        ));

        glyphs.extend(layout_panel_text(
            "Choose with the keyboard:",
            &mut self.palette_text_system,
            &mut self.atlas,
            &self.queue,
            text_x,
            footer_y,
            model.hint_color,
        ));

        let yes_x =
            text_x + estimate_monospace_width("Choose with the keyboard:", font_size) + 18.0;
        glyphs.extend(layout_help_keycaps(
            &["Y"],
            &mut self.palette_text_system,
            &mut self.atlas,
            &self.queue,
            &mut quads,
            yes_x,
            footer_y - 10.0,
            font_size * 0.88,
            line_h + 20.0,
            model.text_color,
            model.label_color,
            model.match_color,
            model.info_color,
            model.warning_color,
            model.warning_color,
            inner_panel,
        ));
        let yes_w = estimate_help_keycaps_width(&["Y"], font_size * 0.88);
        glyphs.extend(layout_panel_text(
            "confirm",
            &mut self.palette_text_system,
            &mut self.atlas,
            &self.queue,
            yes_x + yes_w + 10.0,
            footer_y,
            model.label_color,
        ));

        let no_x = yes_x + yes_w + estimate_monospace_width("confirm", font_size) + 42.0;
        glyphs.extend(layout_help_keycaps(
            &["N"],
            &mut self.palette_text_system,
            &mut self.atlas,
            &self.queue,
            &mut quads,
            no_x,
            footer_y - 10.0,
            font_size * 0.88,
            line_h + 20.0,
            model.text_color,
            model.label_color,
            model.match_color,
            model.info_color,
            model.warning_color,
            model.warning_color,
            inner_panel,
        ));
        let no_w = estimate_help_keycaps_width(&["N"], font_size * 0.88);
        glyphs.extend(layout_panel_text(
            "cancel",
            &mut self.palette_text_system,
            &mut self.atlas,
            &self.queue,
            no_x + no_w + 10.0,
            footer_y,
            model.label_color,
        ));

        self.palette_chrome_instances = quads;
        self.palette_glyph_instances = glyphs;
    }

    /// Single-line input box, TRUE center of screen.
    /// Results rendered below prompt with subtle row separators.
    /// VimCommand mode: prompt only, no results, no separator.
    pub(super) fn render_command_palette_minimalist(&mut self, model: &CommandPaletteRenderModel) {
        if matches!(
            model.mode,
            crate::app::command_palette::CommandPaletteMode::ExplorerDeleteConfirm
                | crate::app::command_palette::CommandPaletteMode::BufferCloseConfirm
        ) {
            self.render_confirmation_palette(model);
            return;
        }

        let [panel_x, panel_y, panel_w, panel_h] = model.panel_bounds;
        self.palette_scissor = rect_to_scissor(model.panel_bounds);

        let inner_width = (panel_w - model.panel_padding * 2.0).max(1.0);
        self.palette_text_system
            .set_size(Some(inner_width), Some(model.line_height));

        let mut quads: Vec<RegionDrawInstance> = Vec::new();
        let mut glyphs: Vec<GlyphInstance> = Vec::new();

        let font_size = self.theme.ui.sidebar_font_size;
        let text_x = panel_x + model.panel_padding + 8.0; // 8px left indent
        let line_h = model.line_height.max(18.0);
        let mut row_top = panel_y + model.panel_padding;

        // Chrome: scrim → rounded border → rounded panel bg
        render_palette_chrome(model, &mut quads);

        // Prompt line
        let prompt_h = (line_h + 10.0).max(30.0);
        let prompt_y = row_top + ((prompt_h - line_h) * 0.5).max(0.0);
        let prefix_w = model.prompt_prefix.chars().count() as f32 * font_size * 0.60;
        let query_color =
            if !model.show_results || model.result_match_ranges.iter().any(|r| !r.is_empty()) {
                model.text_color
            } else {
                model.hint_color
            };
        glyphs.extend(layout_panel_text(
            &model.prompt_prefix,
            &mut self.palette_text_system,
            &mut self.atlas,
            &self.queue,
            text_x,
            prompt_y,
            model.hint_color,
        ));
        let aa_text = "Aa";
        let aa_text_w =
            crate::render::renderer::helpers::estimate_monospace_width(aa_text, font_size);
        let option_w =
            if model.mode == crate::app::command_palette::CommandPaletteMode::InFileSearch {
                aa_text_w + 22.0
            } else {
                0.0
            };
        let query_w = (inner_width - prefix_w - option_w - 18.0).max(1.0);
        self.palette_text_system
            .set_size(Some(query_w), Some(model.line_height));
        glyphs.extend(layout_panel_text(
            &crate::render::renderer::helpers::clamp_monospace_text(
                &model.prompt_query,
                query_w,
                font_size,
            ),
            &mut self.palette_text_system,
            &mut self.atlas,
            &self.queue,
            text_x + prefix_w,
            prompt_y,
            query_color,
        ));
        // Selection highlight + caret for editable palette modes
        let is_editable_prompt = matches!(
            model.mode,
            crate::app::command_palette::CommandPaletteMode::ExplorerPasteFile
                | crate::app::command_palette::CommandPaletteMode::ExplorerRenameFull
                | crate::app::command_palette::CommandPaletteMode::ExplorerRenameBase
                | crate::app::command_palette::CommandPaletteMode::ExplorerCreateFile
                | crate::app::command_palette::CommandPaletteMode::ExplorerCreateFolder
                | crate::app::command_palette::CommandPaletteMode::LspRename
        );
        if is_editable_prompt {
            let query_display = crate::render::renderer::helpers::clamp_monospace_text(
                &model.prompt_query,
                query_w,
                font_size,
            );
            let visible_len = query_display.len();
            let prefix_x = text_x + prefix_w;

            if let Some((sel_start, sel_end)) = model.prompt_selection_range {
                let sel_start = sel_start.min(visible_len);
                let sel_end = sel_end.min(visible_len);
                let before_w = estimate_monospace_width(&query_display[..sel_start], font_size);
                let sel_w = estimate_monospace_width(&query_display[sel_start..sel_end], font_size);
                let mut sel_bg = model.selection_bg;
                sel_bg[3] = sel_bg[3].max(0.55);
                quads.push(RegionDrawInstance::new(
                    [prefix_x + before_w, prompt_y - 2.0, sel_w, line_h + 4.0],
                    sel_bg,
                ));
            } else {
                let cursor_byte = model.prompt_cursor_byte.min(visible_len);
                let before_w = estimate_monospace_width(&query_display[..cursor_byte], font_size);
                let caret_w = if model.vim_caret_block {
                    estimate_monospace_width("M", font_size).max(2.0)
                } else {
                    2.0_f32
                };
                let mut caret_color = model.text_color;
                caret_color[3] = if model.vim_caret_block { 0.45 } else { 0.9 };
                quads.push(RegionDrawInstance::new(
                    [prefix_x + before_w, prompt_y - 2.0, caret_w, line_h + 4.0],
                    caret_color,
                ));
            }

            if let Some(label) = model.vim_mode_label {
                let label_w = estimate_monospace_width(label, font_size);
                let label_x = panel_x + panel_w - model.panel_padding - label_w;
                glyphs.extend(layout_panel_text(
                    label,
                    &mut self.palette_text_system,
                    &mut self.atlas,
                    &self.queue,
                    label_x,
                    prompt_y,
                    model.vim_mode_color.unwrap_or(model.hint_color),
                ));
            }
        }
        if model.mode == crate::app::command_palette::CommandPaletteMode::InFileSearch {
            let aa_box_x = panel_x + panel_w - model.panel_padding - option_w;
            let aa_box_y = prompt_y - 2.0;
            let aa_box_h = (model.line_height + 4.0).max(12.0);
            if model.search_case_sensitive {
                let mut aa_bg = model.match_color;
                aa_bg[3] = aa_bg[3].clamp(0.35, 0.70);
                quads.push(RegionDrawInstance::new(
                    [aa_box_x, aa_box_y, option_w, aa_box_h],
                    aa_bg,
                ));
            }
            glyphs.extend(layout_panel_text(
                aa_text,
                &mut self.palette_text_system,
                &mut self.atlas,
                &self.queue,
                aa_box_x + ((option_w - aa_text_w) * 0.5).max(0.0),
                aa_box_y + ((aa_box_h - model.line_height) * 0.5).max(0.0),
                if model.search_case_sensitive {
                    model.text_color
                } else {
                    model.hint_color
                },
            ));
        }
        row_top += prompt_h;

        // VimCommand: only prompt — exit early
        if !model.show_results {
            self.palette_chrome_instances = quads;
            self.palette_glyph_instances = glyphs;
            return;
        }

        // Separator beneath prompt
        quads.push(RegionDrawInstance::new(
            [
                panel_x + model.panel_padding,
                row_top + 2.0,
                inner_width - model.panel_padding,
                1.0,
            ],
            model.border_color,
        ));
        row_top += 8.0;

        // Result rows
        let row_v_pad = 4.0;
        let row_h = line_h + row_v_pad * 2.0;
        let max_visible = (((panel_h - model.panel_padding * 2.0 - prompt_h - 8.0) / row_h).floor()
            as usize)
            .max(1);
        // Renderer-side scroll: the model's offset trusts a row count computed
        // from a different prompt height, which let the selection walk below
        // the last drawn row before scrolling kicked in.
        let scroll_offset = palette_scroll_offset(
            model.selected_index,
            model.result_labels.len(),
            max_visible,
            |_| None::<String>,
        );

        for (visible_idx, (label, ranges)) in model
            .result_labels
            .iter()
            .zip(model.result_match_ranges.iter())
            .skip(scroll_offset)
            .take(max_visible)
            .enumerate()
        {
            let absolute_idx = scroll_offset + visible_idx;

            if absolute_idx == model.selected_index {
                render_palette_selection(model, &mut quads, row_top, row_h);
            }
            if visible_idx > 0 {
                let mut sep = model.border_color;
                sep[3] *= 0.35;
                quads.push(RegionDrawInstance::new(
                    [text_x, row_top, inner_width - 8.0, 1.0],
                    sep,
                ));
            }

            let label_y = row_top + row_v_pad;
            let tone = model
                .item_tones
                .get(absolute_idx)
                .copied()
                .unwrap_or_default();
            let mut row_model = model.clone();
            row_model.label_color = palette_tone_color(tone, model);
            Self::render_highlighted_label(
                label,
                ranges,
                text_x,
                label_y,
                font_size,
                &row_model,
                &mut self.palette_text_system,
                &mut self.atlas,
                &self.queue,
                &mut glyphs,
            );

            // Right-aligned dimmed secondary label (e.g. the LeetCode language
            // hint "python3 · .py", or a Python env path). Only drawn when it
            // fits to the right of the primary label.
            if let Some(secondary) = model
                .secondary_labels
                .get(absolute_idx)
                .filter(|s| !s.is_empty())
            {
                let label_w =
                    crate::render::renderer::helpers::estimate_monospace_width(label, font_size);
                let sec_w = crate::render::renderer::helpers::estimate_monospace_width(
                    secondary, font_size,
                );
                let sec_x = panel_x + panel_w - model.panel_padding - 8.0 - sec_w;
                if sec_x > text_x + label_w + 16.0 {
                    self.palette_text_system
                        .set_size(Some(sec_w.max(1.0)), Some(model.line_height));
                    glyphs.extend(layout_panel_text(
                        secondary,
                        &mut self.palette_text_system,
                        &mut self.atlas,
                        &self.queue,
                        sec_x,
                        label_y,
                        model.hint_color,
                    ));
                    self.palette_text_system
                        .set_size(Some(inner_width), Some(model.line_height));
                }
            }
            row_top += row_h;
        }

        self.palette_chrome_instances = quads;
        self.palette_glyph_instances = glyphs;
    }

    // ── File Picker (complex) ──────────────────────────────────────────────────
}

fn palette_tone_color(
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
