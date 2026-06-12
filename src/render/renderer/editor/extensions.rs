use crate::{
    app::app_state::{ExtensionCategory, ExtensionsManagerState, ExtensionsTab},
    render::{region_pipeline::RegionDrawInstance, renderer::Renderer},
};
use cosmic_text::Metrics;

use super::super::helpers::{
    clamp_monospace_text, estimate_monospace_width, layout_panel_text, layout_panel_text_bold,
    rect_to_scissor,
};

#[derive(Clone, Copy)]
enum ExtensionRenderEntry {
    Section(ExtensionCategory),
    Item(usize),
}

impl Renderer {
    pub fn update_extensions_manager_content(
        &mut self,
        state: &ExtensionsManagerState,
        center_bounds: [f32; 4],
    ) {
        if center_bounds[2] < 1.0 || center_bounds[3] < 1.0 {
            self.clear_editor_overlays();
            return;
        }

        // Single scale source for hardcoded chrome px so paddings/radii track the
        // (already runtime-scaled) text metrics across monitors.
        let s = self.ui_scale.max(0.5);
        let font_size = self.theme.editor.font_size.max(13.0 * s);
        let line_height = self
            .theme
            .editor
            .line_height
            .max(font_size + 6.0 * s)
            .max(21.0 * s);
        self.editor_overlay_text_system
            .set_metrics(Metrics::new(font_size, line_height));
        self.editor_overlay_scissor = rect_to_scissor(center_bounds);

        let pad_x = self.editor_padding_x.max(18.0 * s);
        let pad_y = self.editor_padding_y.max(18.0 * s);
        let panel_x = center_bounds[0] + pad_x;
        let panel_y = center_bounds[1] + pad_y;
        let panel_w = (center_bounds[2] - pad_x * 2.0).max(1.0);
        let panel_h = (center_bounds[3] - pad_y * 2.0).max(1.0);

        let panel_bg = self.theme.ui.panel_bg.as_f32();
        let editor_bg = self.theme.editor.bg.as_f32();
        let overlay = self.theme.ui.overlay_bg.as_f32();
        let accent = self.theme.ui.accent.as_f32();
        let fg = self.theme.ui.fg.as_f32();
        let dim = self.theme.ui.fg_dim.as_f32();
        let ghost = self.theme.ui.fg_ghost.as_f32();
        let success = self.theme.ui.success.as_f32();
        let warning = self.theme.ui.warning.as_f32();
        let selection = self.theme.ui.selection_bg.as_f32();
        let mut divider = ghost;
        divider[3] = divider[3].clamp(0.28, 0.42);

        let titlebar_h = line_height + 10.0 * s;
        let header_h = titlebar_h + line_height * 4.0 + 72.0 * s;
        let content_top = panel_y + header_h;
        let content_bottom = panel_y + panel_h;
        let rows_bottom = content_bottom.max(content_top + line_height);
        let content_h = (rows_bottom - content_top).max(line_height);

        let inner_x = panel_x + 14.0 * s;
        let inner_w = (panel_w - 28.0 * s).max(1.0);
        let text_w = inner_w.max(1.0);

        if let Some(cmd_state) = &state.command {
            // Render ONLY the base solid panel, title bar, and log popup modal.
            // Bypassing background rendering completely resolves text-bleed overlay and opacity issues!
            let mut chrome = vec![
                RegionDrawInstance::new([panel_x, panel_y, panel_w, panel_h], panel_bg),
                RegionDrawInstance::new([panel_x, panel_y, panel_w, titlebar_h], panel_bg),
                RegionDrawInstance::new(
                    [panel_x, panel_y + titlebar_h - 1.0, panel_w, 1.0],
                    divider,
                ),
            ];
            let mut glyphs = Vec::new();

            self.editor_overlay_text_system
                .set_size(Some(text_w), Some(line_height));
            glyphs.extend(layout_panel_text(
                "◇ Extensions / netherize-editor",
                &mut self.editor_overlay_text_system,
                &mut self.atlas,
                &self.queue,
                inner_x,
                panel_y + 5.0,
                fg,
            ));

            let popup_w = ((panel_w * 0.82).max(380.0 * s))
                .min(panel_w - 16.0 * s)
                .max(180.0);
            let popup_h = ((panel_h * 0.72).max(260.0 * s))
                .min(panel_h - 16.0 * s)
                .max(120.0);
            let popup_x = panel_x + (panel_w - popup_w) * 0.5;
            let popup_y = panel_y + (panel_h - popup_h) * 0.5;

            // Make the popup card background COMPLETELY solid (alpha = 1.0) using solid editor bg
            let mut card_bg = editor_bg;
            card_bg[3] = 1.0;

            let border_color = if cmd_state.running {
                accent
            } else if cmd_state.success == Some(true) {
                success
            } else {
                warning
            };

            // Draw solid popup border and body card
            chrome.push(
                RegionDrawInstance::new(
                    [
                        popup_x - 1.5 * s,
                        popup_y - 1.5 * s,
                        popup_w + 3.0 * s,
                        popup_h + 3.0 * s,
                    ],
                    border_color,
                )
                .with_radius(6.0 * s),
            );
            chrome.push(
                RegionDrawInstance::new([popup_x, popup_y, popup_w, popup_h], card_bg)
                    .with_radius(5.5 * s),
            );

            let header_divider_h = line_height + 14.0 * s;
            let mut divider_color = ghost;
            divider_color[3] = 0.25;
            chrome.push(RegionDrawInstance::new(
                [popup_x, popup_y + header_divider_h, popup_w, 1.0],
                divider_color,
            ));

            let action_txt = if cmd_state.uninstall {
                "Uninstalling"
            } else {
                "Installing"
            };
            let status_label = if cmd_state.running {
                format!("⚡ {} {}...", action_txt, cmd_state.binary)
            } else if cmd_state.success == Some(true) {
                format!("✓ {} complete for {}", action_txt, cmd_state.binary)
            } else {
                format!(
                    "✗ {} failed for {} (code {:?})",
                    action_txt, cmd_state.binary, cmd_state.exit_code
                )
            };

            glyphs.extend(layout_panel_text_bold(
                &status_label,
                &mut self.editor_overlay_text_system,
                &mut self.atlas,
                &self.queue,
                popup_x + 16.0 * s,
                popup_y + 8.0 * s,
                border_color,
            ));

            let log_box_x = popup_x + 16.0 * s;
            let log_box_y = popup_y + header_divider_h + 12.0 * s;
            let log_box_w = popup_w - 32.0 * s;
            let log_box_h = (popup_h - header_divider_h - line_height - 36.0 * s).max(40.0 * s);

            // Make the logs container box also COMPLETELY solid (alpha = 1.0) using solid panel bg
            let mut inner_box_bg = panel_bg;
            inner_box_bg[3] = 1.0;
            chrome.push(
                RegionDrawInstance::new([log_box_x, log_box_y, log_box_w, log_box_h], inner_box_bg)
                    .with_radius(4.0 * s),
            );

            let log_font_size = (font_size * 0.85).max(11.0 * s);
            let log_line_h = (line_height * 0.85).max(18.0 * s);
            self.editor_overlay_text_system
                .set_metrics(Metrics::new(log_font_size, log_line_h));
            self.editor_overlay_text_system
                .set_size(Some(log_box_w - 16.0 * s), Some(log_line_h));

            if cmd_state.logs.is_empty() {
                glyphs.extend(layout_panel_text(
                    "Waiting for process output...",
                    &mut self.editor_overlay_text_system,
                    &mut self.atlas,
                    &self.queue,
                    log_box_x + 10.0 * s,
                    log_box_y + 8.0 * s,
                    ghost,
                ));
            } else {
                let char_width = (log_font_size * 0.6).max(1.0);
                let max_chars = ((log_box_w - 20.0 * s) / char_width).floor() as usize;
                let max_chars = max_chars.max(10);

                let mut wrapped_lines = Vec::new();
                for line in &cmd_state.logs {
                    let is_err =
                        line.contains("npm error") || line.to_lowercase().contains("error");
                    let chars: Vec<char> = line.chars().collect();
                    if chars.is_empty() {
                        wrapped_lines.push((String::new(), is_err));
                    } else {
                        let mut start = 0;
                        while start < chars.len() {
                            let end = (start + max_chars).min(chars.len());
                            let chunk: String = chars[start..end].iter().collect();
                            wrapped_lines.push((chunk, is_err));
                            start = end;
                        }
                    }
                }

                let max_lines = ((log_box_h / log_line_h).floor() as usize).max(1);
                let start_idx = wrapped_lines.len().saturating_sub(max_lines);
                let mut cur_y = log_box_y + 6.0 * s;
                for (line_text, is_err) in &wrapped_lines[start_idx..] {
                    let color = if *is_err { warning } else { dim };
                    glyphs.extend(layout_panel_text(
                        line_text,
                        &mut self.editor_overlay_text_system,
                        &mut self.atlas,
                        &self.queue,
                        log_box_x + 10.0 * s,
                        cur_y,
                        color,
                    ));
                    cur_y += log_line_h;
                }
            }

            self.editor_overlay_text_system
                .set_metrics(Metrics::new(font_size, line_height));
            self.editor_overlay_text_system
                .set_size(Some(text_w), Some(line_height));

            let helper_text = if cmd_state.running {
                "Logs are streaming in real-time. Please wait for the operation to finish..."
            } else {
                "Operation finished. Press Esc to dismiss this popup."
            };
            glyphs.extend(layout_panel_text(
                helper_text,
                &mut self.editor_overlay_text_system,
                &mut self.atlas,
                &self.queue,
                popup_x + 16.0 * s,
                popup_y + popup_h - line_height - 10.0 * s,
                ghost,
            ));

            self.editor_overlay_chrome_instances = chrome;
            self.editor_overlay_glyph_instances = glyphs;
            self.editor_overlay_text_pipeline.upload_instances(
                &self.device,
                &self.queue,
                &self.editor_overlay_glyph_instances,
            );
            return;
        }

        let mut chrome = vec![
            RegionDrawInstance::new([panel_x, panel_y, panel_w, panel_h], panel_bg),
            RegionDrawInstance::new([panel_x, panel_y, panel_w, titlebar_h], panel_bg),
            RegionDrawInstance::new([panel_x, panel_y + titlebar_h - 1.0, panel_w, 1.0], divider),
            RegionDrawInstance::new([panel_x, panel_y + header_h - 1.0, panel_w, 1.0], divider),
        ];
        let mut glyphs = Vec::new();

        let inner_x = panel_x + 14.0 * s;
        let inner_w = (panel_w - 28.0 * s).max(1.0);
        let text_w = inner_w.max(1.0);

        self.editor_overlay_text_system
            .set_size(Some(text_w), Some(line_height));
        glyphs.extend(layout_panel_text(
            "◇ Extensions / netherize-editor",
            &mut self.editor_overlay_text_system,
            &mut self.atlas,
            &self.queue,
            inner_x,
            panel_y + 5.0 * s,
            fg,
        ));

        let shortcuts_y = panel_y + titlebar_h + 8.0 * s;
        let shortcut_h = (line_height * 0.82).max(16.0 * s);
        chrome.push(
            RegionDrawInstance::new(
                [inner_x, shortcuts_y - 2.0 * s, inner_w, shortcut_h + 6.0 * s],
                overlay,
            )
            .with_radius(4.0 * s),
        );
        let shortcut_font = (font_size * 0.72).max(10.0 * s);
        let shortcut_line_h = (line_height * 0.72).max(14.0 * s);
        self.editor_overlay_text_system
            .set_metrics(Metrics::new(shortcut_font, shortcut_line_h));
        self.editor_overlay_text_system
            .set_size(None, Some(shortcut_line_h));
        let shortcuts: &[(&str, &str)] = if inner_w < 520.0 * s {
            &[
                ("Ctrl-N/P", "nav"),
                ("i", "install"),
                ("u", "uninstall"),
                ("/", "filter"),
            ]
        } else {
            &[
                ("Ctrl-N/P", "nav"),
                ("Tab", "tabs"),
                ("Enter", "expand"),
                ("i/u", "install"),
                ("/", "filter"),
            ]
        };
        let mut shortcut_x = inner_x + 10.0 * s;
        let key_text_gap = 14.0 * s;
        let group_gap = 28.0 * s;
        let key_pad_x = 7.0 * s;
        let key_h = shortcut_h.max(shortcut_line_h + 7.0 * s);
        for (key, label) in shortcuts {
            let key_w = estimate_monospace_width(key, shortcut_font) + key_pad_x * 2.0;
            let label_w = estimate_monospace_width(label, shortcut_font);
            if shortcut_x + key_w + key_text_gap + label_w > inner_x + inner_w - 8.0 * s {
                break;
            }
            let mut key_outline = accent;
            key_outline[3] = key_outline[3].max(0.82);
            let mut key_fill = editor_bg;
            key_fill[3] = key_fill[3].max(0.92);
            chrome.push(
                RegionDrawInstance::new([shortcut_x, shortcuts_y - 1.0, key_w, key_h], key_outline)
                    .with_radius(4.0 * s),
            );
            chrome.push(
                RegionDrawInstance::new(
                    [
                        shortcut_x + 1.5 * s,
                        shortcuts_y + 0.5 * s,
                        key_w - 3.0 * s,
                        key_h - 3.0 * s,
                    ],
                    key_fill,
                )
                .with_radius(3.0 * s),
            );
            glyphs.extend(layout_panel_text_bold(
                key,
                &mut self.editor_overlay_text_system,
                &mut self.atlas,
                &self.queue,
                shortcut_x + key_pad_x,
                shortcuts_y + (key_h - shortcut_line_h) * 0.5,
                accent,
            ));
            glyphs.extend(layout_panel_text(
                label,
                &mut self.editor_overlay_text_system,
                &mut self.atlas,
                &self.queue,
                shortcut_x + key_w + key_text_gap,
                shortcuts_y + (key_h - shortcut_line_h) * 0.5,
                ghost,
            ));
            shortcut_x += key_w + key_text_gap + label_w + group_gap;
        }
        self.editor_overlay_text_system
            .set_metrics(Metrics::new(font_size, line_height));
        self.editor_overlay_text_system
            .set_size(Some(text_w), Some(line_height));

        let tabs_y = panel_y + titlebar_h + line_height + 24.0 * s;
        let tab_gap = 8.0 * s;
        let tabs = [
            (ExtensionsTab::All, format!("All {}", state.items.len())),
            (
                ExtensionsTab::Installed,
                format!("Installed {}", state.installed_count()),
            ),
            (
                ExtensionsTab::Available,
                format!("Available {}", state.available_count()),
            ),
        ];
        let mut tab_x = inner_x;
        for (tab_kind, label) in tabs {
            let active = state.tab == tab_kind;
            let tab_w = estimate_monospace_width(&label, font_size) + 22.0 * s;
            let mut tab_bg = if active { selection } else { overlay };
            tab_bg[3] = tab_bg[3].max(if active { 0.95 } else { 0.62 });
            chrome.push(
                RegionDrawInstance::new(
                    [tab_x, tabs_y - 5.0 * s, tab_w, line_height + 10.0 * s],
                    tab_bg,
                )
                .with_radius(5.0 * s),
            );
            if active {
                chrome.push(
                    RegionDrawInstance::new(
                        [tab_x, tabs_y + line_height + 3.0 * s, tab_w, 2.0 * s],
                        accent,
                    )
                    .with_radius(1.0 * s),
                );
            }
            glyphs.extend(if active {
                layout_panel_text_bold(
                    &label,
                    &mut self.editor_overlay_text_system,
                    &mut self.atlas,
                    &self.queue,
                    tab_x + 11.0 * s,
                    tabs_y,
                    accent,
                )
            } else {
                layout_panel_text(
                    &label,
                    &mut self.editor_overlay_text_system,
                    &mut self.atlas,
                    &self.queue,
                    tab_x + 11.0 * s,
                    tabs_y,
                    ghost,
                )
            });
            tab_x += tab_w + tab_gap;
        }

        let summary_y = tabs_y + line_height + 12.0 * s;
        chrome.push(RegionDrawInstance::new(
            [inner_x, summary_y, inner_w, line_height + 18.0 * s],
            overlay,
        ));
        let installed_label = format!("⚡ {} installed", state.installed_count());
        glyphs.extend(layout_panel_text_bold(
            &installed_label,
            &mut self.editor_overlay_text_system,
            &mut self.atlas,
            &self.queue,
            inner_x + 14.0 * s,
            summary_y + 8.0 * s,
            success,
        ));
        let available_x =
            inner_x + 14.0 * s + estimate_monospace_width(&installed_label, font_size) + 28.0 * s;
        let available_label = format!("↓ {} available", state.available_count());
        glyphs.extend(layout_panel_text_bold(
            &available_label,
            &mut self.editor_overlay_text_system,
            &mut self.atlas,
            &self.queue,
            available_x,
            summary_y + 8.0 * s,
            warning,
        ));
        let platform_label = format!("Platform: {}", state.platform);
        let platform_x =
            available_x + estimate_monospace_width(&available_label, font_size) + 34.0 * s;
        let total_label = format!("{} total extensions", state.items.len());
        let total_w = estimate_monospace_width(&total_label, font_size);
        let total_x = inner_x + inner_w - total_w - 14.0 * s;
        if platform_x + estimate_monospace_width(&platform_label, font_size) + 18.0 * s < total_x {
            glyphs.extend(layout_panel_text(
                &platform_label,
                &mut self.editor_overlay_text_system,
                &mut self.atlas,
                &self.queue,
                platform_x,
                summary_y + 8.0 * s,
                dim,
            ));
        }
        if total_x > available_x + estimate_monospace_width(&available_label, font_size) + 18.0 * s
        {
            glyphs.extend(layout_panel_text(
                &total_label,
                &mut self.editor_overlay_text_system,
                &mut self.atlas,
                &self.queue,
                total_x,
                summary_y + 8.0 * s,
                ghost,
            ));
        }

        let filter_y = summary_y + line_height + 30.0 * s;
        let filter_bg = if state.filter_focused {
            selection
        } else {
            editor_bg
        };
        chrome.push(RegionDrawInstance::new(
            [inner_x, filter_y, inner_w, line_height + 16.0 * s],
            filter_bg,
        ));
        if state.filter_focused {
            chrome.push(RegionDrawInstance::new(
                [inner_x, filter_y, inner_w, 2.0 * s],
                accent,
            ));
            chrome.push(RegionDrawInstance::new(
                [inner_x, filter_y + line_height + 14.0 * s, inner_w, 2.0 * s],
                accent,
            ));
        }
        let filter_text = if state.filter.is_empty() {
            "ext› filter...".to_string()
        } else {
            format!("ext› {}", state.filter)
        };
        glyphs.extend(layout_panel_text(
            &clamp_monospace_text(&filter_text, inner_w - 24.0 * s, font_size),
            &mut self.editor_overlay_text_system,
            &mut self.atlas,
            &self.queue,
            inner_x + 12.0 * s,
            filter_y + 8.0 * s,
            if state.filter.is_empty() { ghost } else { fg },
        ));

        let entries = build_extension_entries(state);
        let section_h = line_height + 10.0 * s;
        let row_h = (line_height * 2.0 + 16.0 * s).max(54.0 * s);
        let detail_h = (line_height * 4.0 + 42.0 * s).max(124.0 * s);
        let mut virtual_y = 0.0f32;
        let mut selected_virtual_y = 0.0f32;
        for entry in &entries {
            match *entry {
                ExtensionRenderEntry::Section(_) => virtual_y += section_h,
                ExtensionRenderEntry::Item(abs_idx) => {
                    if state.selected_item_index() == Some(abs_idx) {
                        selected_virtual_y = virtual_y;
                    }
                    virtual_y += row_h;
                    if state.items.get(abs_idx).is_some_and(|item| {
                        state.expanded_binary.as_deref() == Some(item.binary.as_str())
                    }) {
                        virtual_y += detail_h;
                    }
                }
            }
        }
        let total_virtual_h = virtual_y;
        let scroll_y = (selected_virtual_y + row_h - content_h + 8.0 * s).max(0.0);
        let scroll_y = scroll_y.min((total_virtual_h - content_h).max(0.0));

        if total_virtual_h > content_h {
            let track_x = panel_x + panel_w - 7.0 * s;
            let track_top = content_top + 4.0 * s;
            let track_h = (rows_bottom - track_top - 4.0 * s).max(8.0 * s);
            let thumb_h = (content_h / total_virtual_h * track_h)
                .max(18.0 * s)
                .min(track_h);
            let thumb_y = track_top + (scroll_y / total_virtual_h * track_h).min(track_h - thumb_h);
            chrome.push(
                RegionDrawInstance::new(
                    [track_x, track_top, 3.0 * s, track_h],
                    with_alpha(ghost, 0.12),
                )
                .with_radius(1.5 * s),
            );
            chrome.push(
                RegionDrawInstance::new(
                    [track_x, thumb_y, 3.0 * s, thumb_h],
                    with_alpha(ghost, 0.45),
                )
                .with_radius(1.5 * s),
            );
        }

        let mut entry_y = content_top + 8.0 * s - scroll_y;
        let selected_abs = state.selected_item_index();
        for entry in entries {
            match entry {
                ExtensionRenderEntry::Section(category) => {
                    if entry_y + section_h > content_top && entry_y < rows_bottom {
                        let title = match category {
                            ExtensionCategory::CliTools => "▹ CLI Tools",
                            ExtensionCategory::LanguageServers => "▹ Language Servers (LSP)",
                        };
                        let (installed, total) = state.category_counts(category);
                        glyphs.extend(layout_panel_text_bold(
                            title,
                            &mut self.editor_overlay_text_system,
                            &mut self.atlas,
                            &self.queue,
                            inner_x,
                            entry_y + 4.0 * s,
                            fg,
                        ));
                        let count = format!("{installed}/{total} installed");
                        glyphs.extend(layout_panel_text_bold(
                            &count,
                            &mut self.editor_overlay_text_system,
                            &mut self.atlas,
                            &self.queue,
                            inner_x + inner_w - estimate_monospace_width(&count, font_size),
                            entry_y + 4.0 * s,
                            success,
                        ));
                    }
                    entry_y += section_h;
                }
                ExtensionRenderEntry::Item(abs_idx) => {
                    let Some(item) = state.items.get(abs_idx) else {
                        entry_y += row_h;
                        continue;
                    };
                    if entry_y + row_h > content_top && entry_y < rows_bottom {
                        let is_selected = selected_abs == Some(abs_idx);
                        chrome.push(RegionDrawInstance::new(
                            [inner_x, entry_y, inner_w, row_h - 4.0 * s],
                            if is_selected { selection } else { overlay },
                        ));
                        if is_selected {
                            chrome.push(RegionDrawInstance::new(
                                [inner_x, entry_y, 3.0 * s, row_h - 4.0 * s],
                                accent,
                            ));
                        }
                        let status_color = if !state.deps_checked {
                            ghost
                        } else if item.installed {
                            success
                        } else {
                            warning
                        };
                        let status = if !state.deps_checked {
                            "… Checking"
                        } else if item.installed {
                            "✓ Installed"
                        } else {
                            "⇩ Not installed"
                        };
                        let compact_row = inner_w < 480.0 * s;
                        let right_pad = 14.0 * s;

                        // Dynamically scale column widths using screen percentage clamped to optimal premium ranges.
                        // This prevents columns from expanding excessively and squishing everything on 2K/High-DPI displays.
                        let status_w = if compact_row {
                            (inner_w * 0.22).clamp(100.0 * s, 140.0 * s)
                        } else {
                            (inner_w * 0.16).clamp(130.0 * s, 180.0 * s)
                        };
                        let tag_w = if compact_row {
                            0.0
                        } else {
                            (inner_w * 0.12).clamp(90.0 * s, 140.0 * s)
                        };

                        let status_x = inner_x + inner_w - status_w - right_pad;
                        let tag_x = status_x - tag_w - 18.0 * s;
                        let name_right = if compact_row {
                            status_x - 14.0 * s
                        } else {
                            tag_x - 18.0 * s
                        };
                        let name_x = inner_x + 36.0 * s;
                        let name_w = (name_right - name_x).max(40.0 * s);
                        let chevron =
                            if state.expanded_binary.as_deref() == Some(item.binary.as_str()) {
                                "▾"
                            } else {
                                "▸"
                            };
                        glyphs.extend(layout_panel_text_bold(
                            chevron,
                            &mut self.editor_overlay_text_system,
                            &mut self.atlas,
                            &self.queue,
                            inner_x + 16.0 * s,
                            entry_y + 8.0 * s,
                            if is_selected { accent } else { ghost },
                        ));
                        glyphs.extend(layout_panel_text_bold(
                            &clamp_monospace_text(&item.name, name_w, font_size),
                            &mut self.editor_overlay_text_system,
                            &mut self.atlas,
                            &self.queue,
                            name_x,
                            entry_y + 8.0 * s,
                            if is_selected { fg } else { dim },
                        ));
                        glyphs.extend(layout_panel_text(
                            &clamp_monospace_text(&item.subtitle, name_w, font_size),
                            &mut self.editor_overlay_text_system,
                            &mut self.atlas,
                            &self.queue,
                            name_x,
                            entry_y + 8.0 * s + line_height,
                            dim,
                        ));
                        if !compact_row {
                            glyphs.extend(layout_panel_text(
                                &clamp_monospace_text(&item.tag, tag_w, font_size),
                                &mut self.editor_overlay_text_system,
                                &mut self.atlas,
                                &self.queue,
                                tag_x,
                                entry_y + 8.0 * s + line_height * 0.5,
                                ghost,
                            ));
                        }
                        glyphs.extend(layout_panel_text_bold(
                            &clamp_monospace_text(status, status_w, font_size),
                            &mut self.editor_overlay_text_system,
                            &mut self.atlas,
                            &self.queue,
                            status_x,
                            entry_y + 8.0 * s + line_height * 0.5,
                            status_color,
                        ));
                    }
                    entry_y += row_h;
                    if state.expanded_binary.as_deref() == Some(item.binary.as_str()) {
                        let detail_y = entry_y;
                        let detail_color = with_alpha(selection, 0.56);
                        chrome.push(
                            RegionDrawInstance::new(
                                [
                                    inner_x + 12.0 * s,
                                    detail_y,
                                    inner_w - 24.0 * s,
                                    detail_h - 8.0 * s,
                                ],
                                detail_color,
                            )
                            .with_radius(6.0 * s),
                        );
                        chrome.push(
                            RegionDrawInstance::new(
                                [inner_x + 12.0 * s, detail_y, 3.0 * s, detail_h - 8.0 * s],
                                accent,
                            )
                            .with_radius(1.5 * s),
                        );
                        let install = if state.platform == "macOS" {
                            &item.macos_install
                        } else {
                            &item.linux_install
                        };
                        let uninstall = if state.platform == "macOS" {
                            &item.macos_uninstall
                        } else {
                            &item.linux_uninstall
                        };
                        let files = if item.extensions.is_empty() {
                            "files: any".to_string()
                        } else {
                            format!("files: {}", item.extensions.join(", "))
                        };
                        glyphs.extend(layout_panel_text_bold(
                            &clamp_monospace_text(
                                &format!("{} details", item.name),
                                inner_w - 52.0 * s,
                                font_size,
                            ),
                            &mut self.editor_overlay_text_system,
                            &mut self.atlas,
                            &self.queue,
                            inner_x + 28.0 * s,
                            detail_y + 12.0 * s,
                            fg,
                        ));
                        glyphs.extend(layout_panel_text(
                            &clamp_monospace_text(
                                &format!("binary: {}  •  {}", item.binary, files),
                                inner_w - 52.0 * s,
                                font_size,
                            ),
                            &mut self.editor_overlay_text_system,
                            &mut self.atlas,
                            &self.queue,
                            inner_x + 28.0 * s,
                            detail_y + 12.0 * s + line_height,
                            dim,
                        ));
                        glyphs.extend(layout_panel_text(
                            &clamp_monospace_text(
                                &format!("install: {install}"),
                                inner_w - 52.0 * s,
                                font_size,
                            ),
                            &mut self.editor_overlay_text_system,
                            &mut self.atlas,
                            &self.queue,
                            inner_x + 28.0 * s,
                            detail_y + 12.0 * s + line_height * 2.0,
                            if item.installed { dim } else { warning },
                        ));
                        if !uninstall.trim().is_empty() {
                            glyphs.extend(layout_panel_text(
                                &clamp_monospace_text(
                                    &format!("uninstall: {uninstall}"),
                                    inner_w - 52.0 * s,
                                    font_size,
                                ),
                                &mut self.editor_overlay_text_system,
                                &mut self.atlas,
                                &self.queue,
                                inner_x + 28.0 * s,
                                detail_y + 12.0 * s + line_height * 3.0,
                                if item.installed { success } else { ghost },
                            ));
                        }
                        entry_y += detail_h;
                    }
                }
            }
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

fn build_extension_entries(state: &ExtensionsManagerState) -> Vec<ExtensionRenderEntry> {
    let visible = state.visible_item_indices();
    let mut entries = Vec::new();
    let mut current_category: Option<ExtensionCategory> = None;
    for abs_idx in visible {
        let Some(item) = state.items.get(abs_idx) else {
            continue;
        };
        if current_category != Some(item.category) {
            current_category = Some(item.category);
            entries.push(ExtensionRenderEntry::Section(item.category));
        }
        entries.push(ExtensionRenderEntry::Item(abs_idx));
    }
    entries
}

fn with_alpha(mut color: [f32; 4], alpha: f32) -> [f32; 4] {
    color[3] = alpha;
    color
}
