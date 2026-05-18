use crate::{
    app::app_state::{ExtensionCategory, ExtensionsManagerState},
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

        let font_size = self.theme.editor.font_size.max(13.0);
        let line_height = self.theme.editor.line_height.max(font_size + 6.0).max(21.0);
        self.editor_overlay_text_system
            .set_metrics(Metrics::new(font_size, line_height));
        self.editor_overlay_scissor = rect_to_scissor(center_bounds);

        let pad_x = self.editor_padding_x.max(18.0);
        let pad_y = self.editor_padding_y.max(18.0);
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

        let titlebar_h = line_height + 10.0;
        let header_h = titlebar_h + line_height * 4.0 + 72.0;
        let footer_h = (line_height * 5.0 + 48.0).max(140.0);
        let footer_gap = 48.0;
        let content_top = panel_y + header_h;
        let content_bottom = panel_y + panel_h;
        let rows_bottom = (content_bottom - footer_h - footer_gap).max(content_top + line_height);
        let content_h = (rows_bottom - content_top).max(line_height);

        let mut chrome = vec![
            RegionDrawInstance::new([panel_x, panel_y, panel_w, panel_h], panel_bg),
            RegionDrawInstance::new([panel_x, panel_y, panel_w, titlebar_h], panel_bg),
            RegionDrawInstance::new([panel_x, panel_y + titlebar_h - 1.0, panel_w, 1.0], divider),
            RegionDrawInstance::new([panel_x, panel_y + header_h - 1.0, panel_w, 1.0], divider),
            RegionDrawInstance::new([panel_x, content_bottom - footer_h - footer_gap, panel_w, footer_h + footer_gap], editor_bg),
            RegionDrawInstance::new([panel_x, content_bottom - footer_h - footer_gap, panel_w, 1.0], divider),
        ];
        let mut glyphs = Vec::new();

        let inner_x = panel_x + 14.0;
        let inner_w = (panel_w - 28.0).max(1.0);
        let text_w = inner_w.max(1.0);

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

        let shortcuts_y = panel_y + titlebar_h + 8.0;
        let shortcut_h = (line_height * 0.82).max(16.0);
        chrome.push(
            RegionDrawInstance::new([inner_x, shortcuts_y - 2.0, inner_w, shortcut_h + 6.0], overlay)
                .with_radius(4.0),
        );
        let shortcut_font = (font_size * 0.72).max(10.0);
        let shortcut_line_h = (line_height * 0.72).max(14.0);
        self.editor_overlay_text_system
            .set_metrics(Metrics::new(shortcut_font, shortcut_line_h));
        self.editor_overlay_text_system
            .set_size(None, Some(shortcut_line_h));
        let shortcuts: &[(&str, &str)] = if inner_w < 520.0 {
            &[("Ctrl-N/P", "nav"), ("i", "install"), ("u", "uninstall"), ("/", "filter")]
        } else {
            &[
                ("Ctrl-N/P", "nav"),
                ("i", "install"),
                ("u", "uninstall"),
                ("Enter", "expand"),
                ("/", "filter"),
            ]
        };
        let mut shortcut_x = inner_x + 10.0;
        let key_text_gap = 14.0;
        let group_gap = 28.0;
        let key_pad_x = 7.0;
        let key_h = shortcut_h.max(shortcut_line_h + 7.0);
        for (key, label) in shortcuts {
            let key_w = estimate_monospace_width(key, shortcut_font) + key_pad_x * 2.0;
            let label_w = estimate_monospace_width(label, shortcut_font);
            if shortcut_x + key_w + key_text_gap + label_w > inner_x + inner_w - 8.0 {
                break;
            }
            let mut key_outline = accent;
            key_outline[3] = key_outline[3].max(0.82);
            let mut key_fill = editor_bg;
            key_fill[3] = key_fill[3].max(0.92);
            chrome.push(RegionDrawInstance::new([shortcut_x, shortcuts_y - 1.0, key_w, key_h], key_outline).with_radius(4.0));
            chrome.push(
                RegionDrawInstance::new([shortcut_x + 1.5, shortcuts_y + 0.5, key_w - 3.0, key_h - 3.0], key_fill)
                    .with_radius(3.0),
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

        let tabs_y = panel_y + titlebar_h + line_height + 24.0;
        let cli_total = state
            .items
            .iter()
            .filter(|item| item.category == ExtensionCategory::CliTools)
            .count();
        let lsp_total = state
            .items
            .iter()
            .filter(|item| item.category == ExtensionCategory::LanguageServers)
            .count();
        let tab_gap = 16.0;
        let all_label = format!("All {}", state.items.len());
        glyphs.extend(layout_panel_text_bold(
            &all_label,
            &mut self.editor_overlay_text_system,
            &mut self.atlas,
            &self.queue,
            inner_x,
            tabs_y,
            accent,
        ));
        let cli_x = inner_x + estimate_monospace_width(&all_label, font_size) + tab_gap;
        let cli_label = format!("CLI Tools {cli_total}");
        glyphs.extend(layout_panel_text(
            &cli_label,
            &mut self.editor_overlay_text_system,
            &mut self.atlas,
            &self.queue,
            cli_x,
            tabs_y,
            ghost,
        ));
        let lsp_x = cli_x + estimate_monospace_width(&cli_label, font_size) + tab_gap;
        glyphs.extend(layout_panel_text(
            &format!("Language Servers {lsp_total}"),
            &mut self.editor_overlay_text_system,
            &mut self.atlas,
            &self.queue,
            lsp_x,
            tabs_y,
            ghost,
        ));

        let summary_y = tabs_y + line_height + 12.0;
        chrome.push(RegionDrawInstance::new(
            [inner_x, summary_y, inner_w, line_height + 18.0],
            overlay,
        ));
        let installed_label = format!("⚡ {} installed", state.installed_count());
        glyphs.extend(layout_panel_text_bold(
            &installed_label,
            &mut self.editor_overlay_text_system,
            &mut self.atlas,
            &self.queue,
            inner_x + 14.0,
            summary_y + 8.0,
            success,
        ));
        let available_x = inner_x + 14.0 + estimate_monospace_width(&installed_label, font_size) + 28.0;
        let available_label = format!("↓ {} available", state.available_count());
        glyphs.extend(layout_panel_text_bold(
            &available_label,
            &mut self.editor_overlay_text_system,
            &mut self.atlas,
            &self.queue,
            available_x,
            summary_y + 8.0,
            warning,
        ));
        let platform_label = format!("Platform: {}", state.platform);
        let platform_x = available_x + estimate_monospace_width(&available_label, font_size) + 34.0;
        let total_label = format!("{} total extensions", state.items.len());
        let total_w = estimate_monospace_width(&total_label, font_size);
        let total_x = inner_x + inner_w - total_w - 14.0;
        if platform_x + estimate_monospace_width(&platform_label, font_size) + 18.0 < total_x {
            glyphs.extend(layout_panel_text(
                &platform_label,
                &mut self.editor_overlay_text_system,
                &mut self.atlas,
                &self.queue,
                platform_x,
                summary_y + 8.0,
                dim,
            ));
        }
        if total_x > available_x + estimate_monospace_width(&available_label, font_size) + 18.0 {
            glyphs.extend(layout_panel_text(
                &total_label,
                &mut self.editor_overlay_text_system,
                &mut self.atlas,
                &self.queue,
                total_x,
                summary_y + 8.0,
                ghost,
            ));
        }

        let filter_y = summary_y + line_height + 30.0;
        let filter_bg = if state.filter_focused { selection } else { editor_bg };
        chrome.push(RegionDrawInstance::new(
            [inner_x, filter_y, inner_w, line_height + 16.0],
            filter_bg,
        ));
        if state.filter_focused {
            chrome.push(RegionDrawInstance::new([inner_x, filter_y, inner_w, 2.0], accent));
            chrome.push(RegionDrawInstance::new(
                [inner_x, filter_y + line_height + 14.0, inner_w, 2.0],
                accent,
            ));
        }
        let filter_text = if state.filter.is_empty() {
            "ext› filter...".to_string()
        } else {
            format!("ext› {}", state.filter)
        };
        glyphs.extend(layout_panel_text(
            &clamp_monospace_text(&filter_text, inner_w - 24.0, font_size),
            &mut self.editor_overlay_text_system,
            &mut self.atlas,
            &self.queue,
            inner_x + 12.0,
            filter_y + 8.0,
            if state.filter.is_empty() { ghost } else { fg },
        ));

        let entries = build_extension_entries(state);
        let section_h = line_height + 10.0;
        let row_h = (line_height * 2.0 + 16.0).max(54.0);
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
                }
            }
        }
        let total_virtual_h = virtual_y;
        let scroll_y = (selected_virtual_y + row_h - content_h + 8.0).max(0.0);
        let scroll_y = scroll_y.min((total_virtual_h - content_h).max(0.0));

        if total_virtual_h > content_h {
            let track_x = panel_x + panel_w - 7.0;
            let track_top = content_top + 4.0;
            let track_h = (rows_bottom - track_top - 4.0).max(8.0);
            let thumb_h = (content_h / total_virtual_h * track_h).max(18.0).min(track_h);
            let thumb_y = track_top + (scroll_y / total_virtual_h * track_h).min(track_h - thumb_h);
            chrome.push(RegionDrawInstance::new([track_x, track_top, 3.0, track_h], with_alpha(ghost, 0.12)).with_radius(1.5));
            chrome.push(RegionDrawInstance::new([track_x, thumb_y, 3.0, thumb_h], with_alpha(ghost, 0.45)).with_radius(1.5));
        }

        let mut entry_y = content_top + 8.0 - scroll_y;
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
                            entry_y + 4.0,
                            fg,
                        ));
                        let count = format!("{installed}/{total} installed");
                        glyphs.extend(layout_panel_text_bold(
                            &count,
                            &mut self.editor_overlay_text_system,
                            &mut self.atlas,
                            &self.queue,
                            inner_x + inner_w - estimate_monospace_width(&count, font_size),
                            entry_y + 4.0,
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
                            [inner_x, entry_y, inner_w, row_h - 4.0],
                            if is_selected { selection } else { overlay },
                        ));
                        if is_selected {
                            chrome.push(RegionDrawInstance::new([inner_x, entry_y, 3.0, row_h - 4.0], accent));
                        }
                        let status_color = if item.installed { success } else { warning };
                        let status = if item.installed { "✓ Installed" } else { "⇩ Not installed" };
                        let compact_row = inner_w < 480.0;
                        let right_pad = 14.0;
                        let status_w = if compact_row { 90.0 } else { 180.0 };
                        let tag_w = if compact_row { 0.0 } else { 120.0 };
                        let status_x = inner_x + inner_w - status_w - right_pad;
                        let tag_x = status_x - tag_w - 18.0;
                        let name_right = if compact_row { status_x - 14.0 } else { tag_x - 18.0 };
                        let name_w = (name_right - (inner_x + 16.0)).max(36.0);
                        glyphs.extend(layout_panel_text_bold(
                            &clamp_monospace_text(&item.name, name_w, font_size),
                            &mut self.editor_overlay_text_system,
                            &mut self.atlas,
                            &self.queue,
                            inner_x + 16.0,
                            entry_y + 8.0,
                            if is_selected { fg } else { dim },
                        ));
                        glyphs.extend(layout_panel_text(
                            &clamp_monospace_text(&item.subtitle, name_w, font_size),
                            &mut self.editor_overlay_text_system,
                            &mut self.atlas,
                            &self.queue,
                            inner_x + 16.0,
                            entry_y + 8.0 + line_height,
                            dim,
                        ));
                        if !compact_row {
                            glyphs.extend(layout_panel_text(
                                &clamp_monospace_text(&item.tag, tag_w, font_size),
                                &mut self.editor_overlay_text_system,
                                &mut self.atlas,
                                &self.queue,
                                tag_x,
                                entry_y + 8.0 + line_height * 0.5,
                                ghost,
                            ));
                        }
                        glyphs.extend(layout_panel_text_bold(
                            &clamp_monospace_text(status, status_w, font_size),
                            &mut self.editor_overlay_text_system,
                            &mut self.atlas,
                            &self.queue,
                            status_x,
                            entry_y + 8.0 + line_height * 0.5,
                            status_color,
                        ));
                    }
                    entry_y += row_h;
                }
            }
        }

        if let Some(item) = state.selected_item() {
            let footer_x = inner_x;
            let footer_y = content_bottom - footer_h + 16.0;
            let footer_w = inner_w;
            let footer_panel_h = (footer_h - 32.0).max(line_height + 18.0);
            let command_for_item = state
                .command
                .as_ref()
                .filter(|command| command.binary == item.binary);
            let mut footer_border = if command_for_item.is_some_and(|command| command.running) {
                accent
            } else if command_for_item.and_then(|command| command.success) == Some(false) {
                self.theme.ui.error.as_f32()
            } else if item.installed {
                success
            } else {
                warning
            };
            footer_border[3] = 0.72;
            let mut footer_bg = overlay;
            footer_bg[3] = footer_bg[3].max(0.92);
            chrome.push(RegionDrawInstance::new([footer_x, footer_y, footer_w, footer_panel_h], footer_border).with_radius(6.0));
            chrome.push(
                RegionDrawInstance::new(
                    [footer_x + 1.5, footer_y + 1.5, (footer_w - 3.0).max(1.0), (footer_panel_h - 3.0).max(1.0)],
                    footer_bg,
                )
                .with_radius(5.0),
            );

            let badge_label = if item.category == ExtensionCategory::CliTools { "CLI" } else { "LSP" };
            let badge_font_size = (font_size * 0.68).max(9.0);
            let badge_line_h = badge_font_size * 1.15;
            let badge_w = estimate_monospace_width(badge_label, badge_font_size) + 18.0;
            let badge_h = line_height * 0.82;
            let badge_x = footer_x + footer_w - badge_w - 16.0;
            let badge_y = footer_y + 11.0;
            let mut badge_border = if item.installed { success } else { warning };
            badge_border[3] = 0.86;
            let mut badge_bg = footer_bg;
            badge_bg[3] = 1.0;
            chrome.push(RegionDrawInstance::new([badge_x, badge_y, badge_w, badge_h], badge_border).with_radius(badge_h * 0.22));
            chrome.push(
                RegionDrawInstance::new(
                    [badge_x + 2.0, badge_y + 2.0, (badge_w - 4.0).max(1.0), (badge_h - 4.0).max(1.0)],
                    badge_bg,
                )
                .with_radius((badge_h * 0.22 - 2.0).max(2.0)),
            );
            self.editor_overlay_text_system
                .set_metrics(Metrics::new(badge_font_size, badge_line_h));
            self.editor_overlay_text_system
                .set_size(Some(badge_w), Some(badge_line_h));
            glyphs.extend(layout_panel_text_bold(
                badge_label,
                &mut self.editor_overlay_text_system,
                &mut self.atlas,
                &self.queue,
                badge_x + (badge_w - estimate_monospace_width(badge_label, badge_font_size)) * 0.5,
                badge_y + (badge_h - badge_line_h) * 0.5,
                badge_border,
            ));
            self.editor_overlay_text_system
                .set_metrics(Metrics::new(font_size, line_height));
            self.editor_overlay_text_system
                .set_size(Some(text_w), Some(line_height));

            let install = if state.platform == "macOS" { &item.macos_install } else { &item.linux_install };
            let title = if let Some(command) = command_for_item {
                let action = if command.uninstall { "uninstall" } else { "install" };
                let status = if command.running {
                    "running"
                } else if command.success == Some(true) {
                    "done"
                } else {
                    "failed"
                };
                format!("{}  •  {action} {status}", item.name)
            } else {
                format!("{}  •  binary: {}", item.name, item.binary)
            };
            let detail = if let Some(command) = command_for_item {
                command
                    .logs
                    .last()
                    .cloned()
                    .unwrap_or_else(|| format!("running: {install}"))
            } else {
                format!("install: {install}")
            };
            let text_w = (footer_w - badge_w - 52.0).max(32.0);
            glyphs.extend(layout_panel_text_bold(
                &clamp_monospace_text(&title, text_w, font_size),
                &mut self.editor_overlay_text_system,
                &mut self.atlas,
                &self.queue,
                footer_x + 14.0,
                footer_y + 14.0,
                fg,
            ));
            glyphs.extend(layout_panel_text(
                &clamp_monospace_text(&detail, footer_w - 28.0, font_size),
                &mut self.editor_overlay_text_system,
                &mut self.atlas,
                &self.queue,
                footer_x + 14.0,
                footer_y + 14.0 + line_height + 4.0,
                if command_for_item.is_some() { fg } else { dim },
            ));
            if let Some(command) = command_for_item {
                let log_start = command.logs.len().saturating_sub(2);
                for (line_idx, line) in command.logs[log_start..].iter().enumerate() {
                    glyphs.extend(layout_panel_text(
                        &clamp_monospace_text(line, footer_w - 28.0, font_size),
                        &mut self.editor_overlay_text_system,
                        &mut self.atlas,
                        &self.queue,
                        footer_x + 14.0,
                        footer_y + 14.0 + line_height * (2.0 + line_idx as f32) + 8.0,
                        dim,
                    ));
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
