#![allow(unused_imports)]

use crate::{
    app::app_state::{
        AppState, CompletionDisplayItem, DiagnosticsState, EditorOverlay, FloatingBoxBlock,
        FloatingBoxStyle, HelpState, OverlayColorToken, ReferencesBufferState, SettingItem,
        SettingsState,
    },
    app::command_palette::CommandPaletteItemTone,
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
    layout_panel_rich_text, layout_panel_text, layout_panel_text_bold, layout_panel_text_italic,
    mode_display_label, mode_pill_color, rect_to_scissor, should_draw_block_cursor,
};
use super::{
    buffers::grouped_list_window_start, cursor_diagnostic, editor_viewport_geometry,
    run_x_for_byte, wrap_text_lines,
};
use crate::text::text_system::StyledTextSpan;

impl Renderer {
    pub fn update_fuzzy_picker_buffer_content(
        &mut self,
        fuzzy_state: &crate::app::app_state::FuzzyState,
        center_bounds: [f32; 4],
        editor_mode: crate::core::mode::EditorMode,
    ) {
        if center_bounds[2] < 1.0 || center_bounds[3] < 1.0 {
            self.clear_editor_overlays();
            return;
        }

        // Scale hardcoded chrome px so paddings/offsets track the runtime-scaled
        // text metrics across monitors (same pattern as extensions.rs).
        let s = self.ui_scale.max(0.5);
        let font_size = self.theme.editor.font_size;
        let line_height = self.theme.editor.line_height.max(font_size + 4.0 * s);
        self.editor_overlay_text_system
            .set_metrics(Metrics::new(font_size, line_height));
        self.editor_overlay_scissor = rect_to_scissor(center_bounds);

        let pad_x = self.editor_padding_x.max(14.0 * s);
        let pad_y = self.editor_padding_y.max(14.0 * s);
        let panel_x = center_bounds[0] + pad_x;
        let panel_y = center_bounds[1] + pad_y;
        let panel_w = (center_bounds[2] - pad_x * 2.0).max(1.0);
        let panel_h = (center_bounds[3] - pad_y * 2.0).max(1.0);
        let is_live_grep =
            fuzzy_state.mode == crate::app::command_palette::CommandPaletteMode::LiveGrep;
        let gap = if is_live_grep { 0.0 } else { 16.0 * s };

        let left_w = if is_live_grep {
            (panel_w * 0.56)
                .clamp(420.0 * s, 720.0 * s)
                .min(panel_w * 0.62)
        } else {
            (panel_w * 0.5).max(1.0)
        };
        let right_w = (panel_w - left_w - gap).max(1.0);
        let left_x = panel_x;
        let right_x = left_x + left_w + gap;

        let header_h = if is_live_grep {
            line_height * 2.0 + 30.0 * s
        } else {
            line_height + 14.0 * s
        };
        let footer_h = line_height + 10.0 * s;
        let content_top = panel_y + header_h;
        let content_bottom = (panel_y + panel_h - footer_h).max(content_top + line_height);
        let content_h = (content_bottom - content_top).max(line_height);

        let panel_bg = self.theme.ui.panel_bg.as_f32();
        let editor_bg = self.theme.editor.bg.as_f32();
        let mut divider = self.theme.ui.fg_ghost.as_f32();
        divider[3] = divider[3].clamp(0.28, 0.42);
        let accent = self.theme.ui.accent.as_f32();
        let fg = self.theme.ui.fg.as_f32();
        let fg_dim = self.theme.ui.fg_dim.as_f32();
        let fg_ghost = self.theme.ui.fg_ghost.as_f32();
        let selection_bg = self.theme.ui.selection_bg.as_f32();
        let warning = self.theme.ui.warning.as_f32();
        let added_fg = self.theme.git.added_gutter.as_f32();
        let removed_fg = self.theme.git.deleted_gutter.as_f32();
        let modified_fg = self.theme.git.modified_gutter.as_f32();
        let added_bg = [0.10, 0.34, 0.20, 0.34];
        let removed_bg = [0.42, 0.14, 0.16, 0.34];

        let mut glyphs = Vec::new();
        let mut chrome = vec![
            RegionDrawInstance::new([left_x, panel_y, left_w, panel_h], panel_bg),
            RegionDrawInstance::new([right_x, panel_y, right_w, panel_h], editor_bg),
            RegionDrawInstance::new([right_x - 1.0, panel_y, 1.0, panel_h], divider),
            RegionDrawInstance::new([left_x, panel_y + header_h - 1.0, left_w, 1.0], divider),
            RegionDrawInstance::new([right_x, panel_y + header_h - 1.0, right_w, 1.0], divider),
        ];

        let header_y = if is_live_grep {
            panel_y + 12.0 * s
        } else {
            panel_y + 6.0 * s
        };

        let title = match fuzzy_state.mode {
            crate::app::command_palette::CommandPaletteMode::FilePicker => "File Picker",
            crate::app::command_palette::CommandPaletteMode::LiveGrep => "Search Results",
            crate::app::command_palette::CommandPaletteMode::FileHistory => "File History",
            _ => "Search",
        };
        // Modal indicator driven by the editor mode (the fuzzy picker already
        // toggles Insert<->Normal via Esc; Normal gives j/k navigation and q/Esc
        // to close — this just makes the current mode visible).
        let (vim_tag, vim_color): (&str, Option<[f32; 4]>) = match editor_mode {
            crate::core::mode::EditorMode::Insert => (
                mode_display_label(crate::core::mode::EditorMode::Insert),
                Some(mode_pill_color(crate::core::mode::EditorMode::Insert, &self.theme)),
            ),
            crate::core::mode::EditorMode::Normal => (
                mode_display_label(crate::core::mode::EditorMode::Normal),
                Some(mode_pill_color(crate::core::mode::EditorMode::Normal, &self.theme)),
            ),
            crate::core::mode::EditorMode::Visual | crate::core::mode::EditorMode::VisualBlock => (
                mode_display_label(crate::core::mode::EditorMode::Visual),
                Some(mode_pill_color(crate::core::mode::EditorMode::Visual, &self.theme)),
            ),
            _ => ("", None),
        };
        let unique_file_count = if is_live_grep {
            let mut paths = Vec::<String>::new();
            for item in &fuzzy_state.results {
                if let crate::app::command_palette::CommandPaletteAction::OpenSearchMatch {
                    path,
                    ..
                } = &item.action
                {
                    let path_label = path.display().to_string();
                    if !paths.iter().any(|existing| existing == &path_label) {
                        paths.push(path_label);
                    }
                }
            }
            paths.len()
        } else {
            0
        };
        // Compose the pieces: title (fg) · vim_tag (mode color) · optional tail (fg_dim).
        let left_header = if is_live_grep {
            format!("{}  ", title)
        } else {
            format!("{}  > {}", title, fuzzy_state.query)
        };

        self.editor_overlay_text_system
            .set_size(Some((left_w - 20.0 * s).max(1.0)), Some(line_height));
        if is_live_grep {
            // 1) title (fg) + 2 spaces, 2) vim tag (mode color), 3) count suffix (fg_dim).
            let title_w = estimate_monospace_width(title, font_size);
            glyphs.extend(layout_panel_text(
                title,
                &mut self.editor_overlay_text_system,
                &mut self.atlas,
                &self.queue,
                left_x + 10.0 * s,
                header_y,
                fg,
            ));
            if !vim_tag.is_empty() {
                // 2 spaces after title, then the vim tag in mode color.
                let tag_x = left_x + 10.0 * s + title_w + estimate_monospace_width("  ", font_size);
                glyphs.extend(layout_panel_text(
                    vim_tag,
                    &mut self.editor_overlay_text_system,
                    &mut self.atlas,
                    &self.queue,
                    tag_x,
                    header_y,
                    vim_color.unwrap_or(fg),
                ));
            }
            let header_count_w =
                estimate_monospace_width(" 999 results · 999 files", font_size).min(left_w * 0.55);
            let count_label = format!(
                "{} results · {} files",
                fuzzy_state.results.len(),
                unique_file_count
            );
            glyphs.extend(layout_panel_text(
                &clamp_monospace_text(&count_label, header_count_w, font_size),
                &mut self.editor_overlay_text_system,
                &mut self.atlas,
                &self.queue,
                (left_x + left_w - header_count_w - 10.0 * s).max(left_x + 90.0 * s),
                header_y,
                fg_dim,
            ));
        } else {
            // 1) prefix (title + spaces + query) in fg, 2) vim tag in mode color.
            let clamped_prefix =
                clamp_monospace_text(&left_header, (left_w - 20.0 * s).max(1.0), font_size);
            let prefix_w = estimate_monospace_width(&clamped_prefix, font_size);
            glyphs.extend(layout_panel_text(
                &clamped_prefix,
                &mut self.editor_overlay_text_system,
                &mut self.atlas,
                &self.queue,
                left_x + 10.0 * s,
                header_y,
                fg,
            ));
            if !vim_tag.is_empty() {
                // 2-space separator mirrors the live-grep branch's gap.
                let sep_w = estimate_monospace_width("  ", font_size);
                glyphs.extend(layout_panel_text(
                    vim_tag,
                    &mut self.editor_overlay_text_system,
                    &mut self.atlas,
                    &self.queue,
                    left_x + 10.0 * s + prefix_w + sep_w,
                    header_y,
                    vim_color.unwrap_or(fg),
                ));
            }
        }

        if is_live_grep {
            let search_box_x = left_x + 14.0 * s;
            let search_box_y = header_y + line_height + 6.0 * s;
            let search_box_w = (left_w - 28.0 * s).max(1.0);
            let search_box_h = line_height + 8.0 * s;
            chrome.push(RegionDrawInstance::new(
                [search_box_x, search_box_y, search_box_w, search_box_h],
                self.theme.ui.overlay_bg.as_f32(),
            ));
            let search_text = if fuzzy_state.query.trim().is_empty() {
                "Search workspace...".to_string()
            } else {
                fuzzy_state.query.clone()
            };
            let aa_text = "Aa";
            let aa_text_w = estimate_monospace_width(aa_text, font_size);
            let option_w = aa_text_w + 22.0 * s;
            glyphs.extend(layout_panel_text(
                &clamp_monospace_text(
                    &search_text,
                    (search_box_w - option_w - 18.0 * s).max(1.0),
                    font_size,
                ),
                &mut self.editor_overlay_text_system,
                &mut self.atlas,
                &self.queue,
                search_box_x + 10.0 * s,
                search_box_y + 4.0 * s,
                if fuzzy_state.query.trim().is_empty() {
                    fg_ghost
                } else {
                    fg_dim
                },
            ));
            let aa_active = fuzzy_state.live_grep_case_sensitive;
            if aa_active {
                let mut aa_bg = self.theme.ui.accent.as_f32();
                aa_bg[3] = aa_bg[3].clamp(0.35, 0.70);
                chrome.push(RegionDrawInstance::new(
                    [
                        search_box_x + search_box_w - option_w - 8.0 * s,
                        search_box_y + 3.0 * s,
                        option_w,
                        (line_height + 2.0 * s).max(12.0 * s),
                    ],
                    aa_bg,
                ));
            }
            let aa_box_x = search_box_x + search_box_w - option_w - 8.0 * s;
            let aa_box_h = (line_height + 2.0 * s).max(12.0 * s);
            glyphs.extend(layout_panel_text(
                aa_text,
                &mut self.editor_overlay_text_system,
                &mut self.atlas,
                &self.queue,
                aa_box_x + ((option_w - aa_text_w) * 0.5).max(0.0),
                search_box_y + 3.0 * s + ((aa_box_h - line_height) * 0.5).max(0.0),
                if aa_active { fg } else { fg_ghost },
            ));
        }

        let selected = fuzzy_state.results.get(fuzzy_state.selected_index);
        let right_header = selected
            .map(|item| {
                if is_live_grep {
                    match &item.action {
                        crate::app::command_palette::CommandPaletteAction::OpenSearchMatch {
                            path,
                            line,
                            column,
                        } => {
                            format!(
                                "{}:{}:{}",
                                compact_search_path(&path.display().to_string()),
                                line,
                                column
                            )
                        }
                        _ => item.label.clone(),
                    }
                } else {
                    item.label.clone()
                }
            })
            .unwrap_or_else(|| "No file selected".to_string());

        self.editor_overlay_text_system
            .set_size(Some((right_w - 20.0 * s).max(1.0)), Some(line_height));
        glyphs.extend(layout_panel_text(
            &clamp_monospace_text(&right_header, (right_w - 20.0 * s).max(1.0), font_size),
            &mut self.editor_overlay_text_system,
            &mut self.atlas,
            &self.queue,
            right_x + 10.0 * s,
            header_y,
            fg,
        ));

        if !is_live_grep {
            let help_text = match fuzzy_state.mode {
                crate::app::command_palette::CommandPaletteMode::FileHistory => {
                    "J/K / Ctrl+N/P / Up/Down to preview  |  Enter to accept  |  Esc/Q to close"
                }
                _ => "Type to search  |  Ctrl+N/P / Up/Down to navigate  |  Enter to open",
            };
            self.editor_overlay_text_system
                .set_size(Some((panel_w - 20.0 * s).max(1.0)), Some(line_height));
            glyphs.extend(layout_panel_text(
                &clamp_monospace_text(help_text, (panel_w - 20.0 * s).max(1.0), font_size),
                &mut self.editor_overlay_text_system,
                &mut self.atlas,
                &self.queue,
                panel_x + 10.0 * s,
                panel_y + panel_h - footer_h + 4.0 * s,
                fg_ghost,
            ));
        }

        let row_h = if is_live_grep {
            line_height + 8.0 * s
        } else {
            line_height * 2.0 + 8.0 * s
        };
        let group_header_h = if is_live_grep {
            line_height + 7.0 * s
        } else {
            0.0
        };
        let visible_rows = ((content_h / row_h).floor() as usize).max(1);
        let start_idx = grouped_list_window_start(
            fuzzy_state.results.len(),
            fuzzy_state.selected_index,
            content_h,
            row_h,
            group_header_h,
            |left, right| {
                if !is_live_grep {
                    return true;
                }
                match (
                    &fuzzy_state.results[left].action,
                    &fuzzy_state.results[right].action,
                ) {
                    (
                        crate::app::command_palette::CommandPaletteAction::OpenSearchMatch {
                            path: left_path,
                            ..
                        },
                        crate::app::command_palette::CommandPaletteAction::OpenSearchMatch {
                            path: right_path,
                            ..
                        },
                    ) => left_path == right_path,
                    _ => false,
                }
            },
        );
        let left_text_width = (left_w - 20.0 * s).max(1.0);
        let mut draw_y = content_top;
        let mut previous_group_path: Option<String> = None;
        let mut rendered_rows = 0usize;
        for item_idx in start_idx..fuzzy_state.results.len() {
            if rendered_rows >= visible_rows || draw_y + row_h > content_bottom + 1.0 {
                break;
            }
            let item = &fuzzy_state.results[item_idx];
            let current_group_path = if is_live_grep {
                match &item.action {
                    crate::app::command_palette::CommandPaletteAction::OpenSearchMatch {
                        path,
                        ..
                    } => Some(path.display().to_string()),
                    _ => None,
                }
            } else {
                None
            };
            if is_live_grep && current_group_path != previous_group_path {
                if let Some(path_label) = current_group_path.as_deref() {
                    if draw_y + group_header_h + row_h > content_bottom + 1.0 {
                        break;
                    }
                    self.editor_overlay_text_system
                        .set_size(Some(left_text_width), Some(line_height));
                    let (file_name, folder_label) = split_search_path(path_label);
                    let mut group_bg = self.theme.ui.overlay_bg.as_f32();
                    group_bg[3] = (group_bg[3] * 0.85).clamp(0.18, 0.45);
                    chrome.push(RegionDrawInstance::new(
                        [
                            left_x + 8.0 * s,
                            draw_y + 2.0 * s,
                            (left_w - 16.0 * s).max(1.0),
                            (line_height + 3.0 * s).max(12.0 * s),
                        ],
                        group_bg,
                    ));
                    let group_count =
                        count_search_matches_for_path(&fuzzy_state.results, path_label);
                    let count_label = format!("{}", group_count);
                    let count_w = estimate_monospace_width(&count_label, font_size).max(16.0 * s);
                    glyphs.extend(layout_panel_text_bold(
                        &clamp_monospace_text(
                            &format!("▾ {}", file_name),
                            left_text_width * 0.50,
                            font_size,
                        ),
                        &mut self.editor_overlay_text_system,
                        &mut self.atlas,
                        &self.queue,
                        left_x + 16.0 * s,
                        draw_y + 4.0 * s,
                        self.theme.ui.accent.as_f32(),
                    ));
                    let folder_x = left_x + left_w * 0.54;
                    glyphs.extend(layout_panel_text(
                        &clamp_monospace_text(
                            &folder_label,
                            (left_x + left_w - folder_x - count_w - 26.0 * s).max(1.0),
                            font_size,
                        ),
                        &mut self.editor_overlay_text_system,
                        &mut self.atlas,
                        &self.queue,
                        folder_x,
                        draw_y + 4.0 * s,
                        fg_ghost,
                    ));
                    glyphs.extend(layout_panel_text_bold(
                        &count_label,
                        &mut self.editor_overlay_text_system,
                        &mut self.atlas,
                        &self.queue,
                        left_x + left_w - count_w - 18.0 * s,
                        draw_y + 4.0 * s,
                        fg,
                    ));
                    draw_y += group_header_h;
                    previous_group_path = current_group_path.clone();
                }
            }
            let row_y = draw_y;
            let is_selected = item_idx == fuzzy_state.selected_index;
            if is_selected {
                chrome.push(RegionDrawInstance::new(
                    [
                        left_x + 6.0 * s,
                        row_y,
                        (left_w - 12.0 * s).max(1.0),
                        row_h - 4.0 * s,
                    ],
                    selection_bg,
                ));
                chrome.push(RegionDrawInstance::new(
                    [left_x + 6.0 * s, row_y, 3.0 * s, row_h - 4.0 * s],
                    accent,
                ));
            }

            self.editor_overlay_text_system
                .set_size(Some(left_text_width), Some(line_height));
            let row_label_color = match item.tone {
                CommandPaletteItemTone::Added => added_fg,
                CommandPaletteItemTone::Removed => removed_fg,
                CommandPaletteItemTone::Modified => modified_fg,
                CommandPaletteItemTone::Function => self.theme.ui.cyan.as_f32(),
                CommandPaletteItemTone::Type => self.theme.ui.magenta.as_f32(),
                CommandPaletteItemTone::Variable => self.theme.ui.info.as_f32(),
                CommandPaletteItemTone::Module => self.theme.ui.amber.as_f32(),
                CommandPaletteItemTone::Default => {
                    if is_selected {
                        fg
                    } else {
                        fg_dim
                    }
                }
            };
            if is_live_grep {
                // Live grep rows intentionally avoid repeating file/line/column:
                // the file is represented by the group header and Enter opens
                // the exact selected match. The row itself focuses on the
                // matched text value.
            } else {
                glyphs.extend(layout_panel_text(
                    &clamp_monospace_text(&item.label, left_text_width, font_size),
                    &mut self.editor_overlay_text_system,
                    &mut self.atlas,
                    &self.queue,
                    left_x + 14.0 * s,
                    row_y + 4.0 * s,
                    row_label_color,
                ));
            }

            let summary = item.secondary_label.clone().unwrap_or_default();
            if !summary.is_empty() {
                let summary_x = if is_live_grep {
                    left_x + 22.0 * s
                } else {
                    left_x + 14.0 * s
                };
                let summary_w = (left_x + left_w - summary_x - 10.0 * s).max(1.0);
                self.editor_overlay_text_system
                    .set_size(Some(summary_w), Some(line_height));
                let summary_y = if is_live_grep {
                    row_y + 4.0 * s
                } else {
                    row_y + line_height + 2.0 * s
                };
                if is_live_grep && !fuzzy_state.query.trim().is_empty() {
                    let clamped_summary = clamp_monospace_text(&summary, summary_w, font_size);
                    let spans = search_summary_spans(
                        &clamped_summary,
                        &fuzzy_state.query,
                        self.theme.ui.warning.as_u8(),
                    );
                    glyphs.extend(layout_panel_rich_text(
                        &clamped_summary,
                        &spans,
                        if is_selected { fg } else { fg_dim },
                        &mut self.editor_overlay_text_system,
                        &mut self.atlas,
                        &self.queue,
                        summary_x,
                        summary_y,
                    ));
                } else {
                    glyphs.extend(layout_panel_text(
                        &clamp_monospace_text(&summary, summary_w, font_size),
                        &mut self.editor_overlay_text_system,
                        &mut self.atlas,
                        &self.queue,
                        summary_x,
                        summary_y,
                        if is_selected { fg } else { fg_ghost },
                    ));
                }
            }
            draw_y += row_h;
            rendered_rows += 1;
        }

        if is_live_grep {
            let footer_y = panel_y + panel_h - footer_h;
            chrome.push(RegionDrawInstance::new(
                [left_x, footer_y, left_w, 1.0],
                divider,
            ));
            let footer_text = format!(
                "↑↓ navigate  |  Enter open match  |  Esc/Q close   • {} results   {} files",
                fuzzy_state.results.len(),
                unique_file_count
            );
            self.editor_overlay_text_system
                .set_size(Some((left_w - 20.0 * s).max(1.0)), Some(line_height));
            glyphs.extend(layout_panel_text(
                &clamp_monospace_text(&footer_text, (left_w - 20.0 * s).max(1.0), font_size),
                &mut self.editor_overlay_text_system,
                &mut self.atlas,
                &self.queue,
                left_x + 10.0 * s,
                footer_y + 4.0 * s,
                fg_ghost,
            ));
        }

        if !fuzzy_state.preview_lines.is_empty() {
            let line_number_width = estimate_monospace_width(
                &format!(
                    "{:>4}",
                    fuzzy_state
                        .preview_lines
                        .last()
                        .map(|line| line.line_number)
                        .unwrap_or(1)
                ),
                font_size,
            ) + 14.0 * s;
            let preview_text_width = (right_w - 20.0 * s - line_number_width).max(1.0);
            let preview_rows = ((content_h / line_height).floor() as usize).max(1);
            let mut preview_start = 0usize;
            if let Some(target_idx) = fuzzy_state
                .preview_lines
                .iter()
                .position(|line| line.is_target)
            {
                preview_start = target_idx.saturating_sub(preview_rows / 2);
                if preview_start + preview_rows > fuzzy_state.preview_lines.len() {
                    preview_start = fuzzy_state.preview_lines.len().saturating_sub(preview_rows);
                }
            }

            for (slot, line) in fuzzy_state
                .preview_lines
                .iter()
                .skip(preview_start)
                .take(preview_rows)
                .enumerate()
            {
                let row_y = content_top + slot as f32 * line_height;
                let line_bg = if line.text.starts_with("+ ") {
                    Some(added_bg)
                } else if line.text.starts_with("- ") {
                    Some(removed_bg)
                } else {
                    None
                };
                if let Some(bg) = line_bg {
                    chrome.push(RegionDrawInstance::new(
                        [
                            right_x + 6.0 * s,
                            row_y,
                            (right_w - 12.0 * s).max(1.0),
                            line_height,
                        ],
                        bg,
                    ));
                } else if line.is_target {
                    chrome.push(RegionDrawInstance::new(
                        [
                            right_x + 6.0 * s,
                            row_y,
                            (right_w - 12.0 * s).max(1.0),
                            line_height,
                        ],
                        selection_bg,
                    ));
                }
                if line.is_target {
                    chrome.push(RegionDrawInstance::new(
                        [right_x + 6.0 * s, row_y, 3.0 * s, line_height],
                        accent,
                    ));
                }

                self.editor_overlay_text_system
                    .set_size(Some(line_number_width), Some(line_height));
                glyphs.extend(layout_panel_text(
                    &format!("{:>4}", line.line_number),
                    &mut self.editor_overlay_text_system,
                    &mut self.atlas,
                    &self.queue,
                    right_x + 10.0 * s,
                    row_y,
                    if line.is_target { warning } else { fg_ghost },
                ));

                self.editor_overlay_text_system
                    .set_size(Some(preview_text_width), Some(line_height));
                let line_start = fuzzy_state
                    .preview_lines
                    .iter()
                    .take(preview_start + slot)
                    .map(|item| item.text.len() + 1)
                    .sum::<usize>();
                let line_end = line_start + line.text.len();
                let line_spans: Vec<StyledTextSpan> = fuzzy_state
                    .preview_spans
                    .iter()
                    .filter_map(|span| {
                        if span.end <= line_start || span.start >= line_end {
                            return None;
                        }
                        Some(StyledTextSpan::with_style(
                            span.start.max(line_start) - line_start,
                            span.end.min(line_end) - line_start,
                            span.color_rgba,
                            span.bold,
                            span.italic,
                        ))
                    })
                    .collect();
                glyphs.extend(layout_panel_rich_text(
                    &clamp_monospace_text(&line.text, preview_text_width, font_size),
                    &line_spans,
                    if line.is_target { fg } else { fg_dim },
                    &mut self.editor_overlay_text_system,
                    &mut self.atlas,
                    &self.queue,
                    right_x + 10.0 * s + line_number_width,
                    row_y,
                ));
            }
        } else {
            let empty_message = if fuzzy_state.results.is_empty() {
                if fuzzy_state.query.trim().is_empty() {
                    "Type to search..."
                } else {
                    "No matches"
                }
            } else {
                "Loading preview..."
            };
            self.editor_overlay_text_system
                .set_size(Some((right_w - 20.0 * s).max(1.0)), Some(line_height));
            glyphs.extend(layout_panel_text(
                empty_message,
                &mut self.editor_overlay_text_system,
                &mut self.atlas,
                &self.queue,
                right_x + 10.0 * s,
                content_top + 4.0 * s,
                fg_ghost,
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

fn split_search_path(path: &str) -> (String, String) {
    let normalized = compact_search_path(path);
    let Some((folder, file)) = normalized.rsplit_once('/') else {
        return (normalized, String::new());
    };
    (file.to_string(), folder.to_string())
}

fn compact_search_path(path: &str) -> String {
    let marker = "/src/";
    if let Some(index) = path.find(marker) {
        return path[index + 1..].to_string();
    }

    let components: Vec<&str> = path.split('/').filter(|part| !part.is_empty()).collect();
    if components.len() > 3 {
        components[components.len().saturating_sub(3)..].join("/")
    } else {
        path.to_string()
    }
}

fn count_search_matches_for_path(
    results: &[crate::app::command_palette::CommandPaletteItem],
    path_label: &str,
) -> usize {
    results
        .iter()
        .filter(|item| match &item.action {
            crate::app::command_palette::CommandPaletteAction::OpenSearchMatch { path, .. } => {
                path.display().to_string() == path_label
            }
            _ => false,
        })
        .count()
}

fn search_summary_spans(summary: &str, query: &str, color: [u8; 4]) -> Vec<StyledTextSpan> {
    let needle = query.trim().to_lowercase();
    if needle.is_empty() {
        return Vec::new();
    }

    let haystack = summary.to_lowercase();
    let mut spans = Vec::new();
    let mut offset = 0usize;
    while let Some(relative_start) = haystack[offset..].find(&needle) {
        let start = offset + relative_start;
        let end = start + needle.len();
        spans.push(StyledTextSpan::with_style(start, end, color, true, false));
        offset = end;
        if offset >= haystack.len() {
            break;
        }
    }
    spans
}
