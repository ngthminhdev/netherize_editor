use cosmic_text::Metrics;

use crate::{
    app::app_state::{SettingItem, SettingsEditingKind, SettingsState},
    render::{region_pipeline::RegionDrawInstance, renderer::Renderer},
};

use super::super::helpers::{
    clamp_monospace_text, estimate_monospace_width, layout_panel_text, rect_to_scissor,
};

#[derive(Clone, Copy, PartialEq, Eq)]
enum SettingsSection {
    Appearance,
    Typography,
    Layout,
}

impl SettingsSection {
    fn label(self) -> &'static str {
        match self {
            Self::Appearance => "APPEARANCE",
            Self::Typography => "TYPOGRAPHY",
            Self::Layout => "LAYOUT",
        }
    }
}

impl SettingItem {
    fn section(&self) -> SettingsSection {
        match self {
            Self::ThemeSelector { .. } | Self::UiRounding { .. } => SettingsSection::Appearance,
            Self::FontFamily { .. } | Self::FontSize { .. } | Self::LineHeight { .. } => {
                SettingsSection::Typography
            }
            Self::SidebarWidth { .. }
            | Self::RightSidebarWidth { .. }
            | Self::BottomPanelHeight { .. } => SettingsSection::Layout,
        }
    }

    fn description(&self) -> &'static str {
        match self {
            Self::ThemeSelector { .. } => {
                "Active palette for window chrome, panel surfaces, and editor accents."
            }
            Self::FontFamily { .. } => {
                "Primary UI/editor typeface used to keep reading rhythm consistent."
            }
            Self::FontSize { .. } => "Base text size. Use h/l for quick ±0.5 adjustments.",
            Self::LineHeight { .. } => {
                "Vertical spacing between lines for denser or calmer presentation."
            }
            Self::SidebarWidth { .. } => {
                "Reserved width for the left dock when the workspace sidebar is shown."
            }
            Self::RightSidebarWidth { .. } => {
                "Reserved width for inspector and secondary tool panels."
            }
            Self::BottomPanelHeight { .. } => {
                "Resting height for terminal, logs, and lower utility surfaces."
            }
            Self::UiRounding { .. } => {
                "Rounds shell and panel corners to match the softer visual style."
            }
        }
    }

    fn preview_value(&self) -> String {
        match self {
            Self::ThemeSelector { current } => format!("\"{}\"", current),
            Self::FontFamily { current } => {
                if current.trim().is_empty() {
                    "\"system\"".to_string()
                } else {
                    format!("\"{}\"", current)
                }
            }
            Self::FontSize { current } | Self::LineHeight { current } => format!("{current:.1}"),
            Self::SidebarWidth { current }
            | Self::RightSidebarWidth { current }
            | Self::BottomPanelHeight { current } => current.to_string(),
            Self::UiRounding { enabled, radius_px } => {
                if *enabled && *radius_px > 0.0 {
                    format!("{radius_px:.0}")
                } else {
                    "0".to_string()
                }
            }
        }
    }

    fn display_value(&self) -> String {
        match self {
            Self::ThemeSelector { current } => current.clone(),
            Self::FontFamily { current } => {
                if current.trim().is_empty() {
                    "System Default".to_string()
                } else {
                    current.clone()
                }
            }
            Self::FontSize { current } | Self::LineHeight { current } => format!("{current:.1}"),
            Self::SidebarWidth { current }
            | Self::RightSidebarWidth { current }
            | Self::BottomPanelHeight { current } => format!("{current} px"),
            Self::UiRounding { enabled, radius_px } => {
                if *enabled && *radius_px > 0.0 {
                    format!("{radius_px:.0} px")
                } else {
                    "Off".to_string()
                }
            }
        }
    }
}

fn with_alpha(mut color: [f32; 4], alpha: f32) -> [f32; 4] {
    color[3] = alpha;
    color
}

fn current_row_value(settings: &SettingsState, item: &SettingItem, is_selected: bool) -> String {
    if !is_selected {
        return item.display_value();
    }

    let Some(editing) = &settings.editing else {
        return item.display_value();
    };

    match (&editing.kind, item) {
        (SettingsEditingKind::FontFamily, SettingItem::FontFamily { .. })
        | (SettingsEditingKind::FontSize, SettingItem::FontSize { .. })
        | (SettingsEditingKind::LineHeight, SettingItem::LineHeight { .. })
        | (SettingsEditingKind::SidebarWidth, SettingItem::SidebarWidth { .. })
        | (SettingsEditingKind::RightSidebarWidth, SettingItem::RightSidebarWidth { .. })
        | (SettingsEditingKind::BottomPanelHeight, SettingItem::BottomPanelHeight { .. }) => {
            format!("{}_", editing.draft)
        }
        _ => item.display_value(),
    }
}

fn settings_preview_lines(settings: &SettingsState) -> Vec<String> {
    let mut lines = vec![
        "# Netherize Editor settings preview".to_string(),
        "".to_string(),
        "[theme]".to_string(),
    ];

    for item in &settings.items {
        if let SettingItem::ThemeSelector { .. } = item {
            lines.push(format!("profile = {}", item.preview_value()));
        }
    }

    lines.extend(["".to_string(), "[editor]".to_string()]);
    for item in &settings.items {
        if let SettingItem::FontFamily { .. } = item {
            lines.push(format!("font_family = {}", item.preview_value()));
        }
    }

    lines.extend(["".to_string(), "[ui]".to_string()]);
    for item in &settings.items {
        match item {
            SettingItem::FontSize { .. } => {
                lines.push(format!("font_size = {}", item.preview_value()))
            }
            SettingItem::LineHeight { .. } => {
                lines.push(format!("line_height = {}", item.preview_value()))
            }
            SettingItem::UiRounding { .. } => {
                lines.push(format!("border_radius = {}", item.preview_value()))
            }
            _ => {}
        }
    }

    lines.extend(["".to_string(), "[dock.left]".to_string()]);
    for item in &settings.items {
        if let SettingItem::SidebarWidth { .. } = item {
            lines.push(format!("width = {}", item.preview_value()));
        }
    }

    lines.extend(["".to_string(), "[dock.right]".to_string()]);
    for item in &settings.items {
        if let SettingItem::RightSidebarWidth { .. } = item {
            lines.push(format!("width = {}", item.preview_value()));
        }
    }

    lines.extend(["".to_string(), "[dock.bottom]".to_string()]);
    for item in &settings.items {
        if let SettingItem::BottomPanelHeight { .. } = item {
            lines.push(format!("height = {}", item.preview_value()));
        }
    }

    lines
}

impl Renderer {
    pub fn update_settings_buffer_content(
        &mut self,
        settings: &SettingsState,
        center_bounds: [f32; 4],
    ) {
        if center_bounds[2] < 1.0 || center_bounds[3] < 1.0 {
            self.clear_editor_overlays();
            return;
        }

        let font_size = self.theme.editor.font_size;
        let line_height = self.theme.editor.line_height.max(font_size + 4.0);
        self.editor_overlay_text_system
            .set_metrics(Metrics::new(font_size, line_height));
        self.editor_overlay_scissor = rect_to_scissor(center_bounds);

        let pad_x = self.editor_padding_x.max(18.0);
        let pad_y = self.editor_padding_y.max(18.0);
        let panel_x = center_bounds[0] + pad_x;
        let panel_y = center_bounds[1] + pad_y;
        let panel_w = (center_bounds[2] - pad_x * 2.0).max(1.0);
        let panel_h = (center_bounds[3] - pad_y * 2.0).max(1.0);
        let gap = 20.0;

        let left_w = (panel_w * 0.70).max(1.0);
        let right_w = (panel_w - left_w - gap).max(280.0);
        let left_x = panel_x;
        let right_x = left_x + left_w + gap;

        let titlebar_h = line_height + 10.0;
        let header_h = line_height + 24.0 + titlebar_h;
        let footer_h = line_height + 12.0;
        let content_top = panel_y + header_h;
        let content_bottom = panel_y + panel_h;
        let rows_bottom = (content_bottom - footer_h).max(content_top + line_height);
        let content_h = (rows_bottom - content_top).max(line_height);

        let panel_bg = self.theme.ui.panel_bg.as_f32();
        let editor_bg = self.theme.editor.bg.as_f32();
        let mut divider = self.theme.ui.fg_ghost.as_f32();
        divider[3] = divider[3].clamp(0.28, 0.42);
        let accent = self.theme.ui.accent.as_f32();
        let fg = self.theme.ui.fg.as_f32();
        let fg_dim = self.theme.ui.fg_dim.as_f32();
        let fg_ghost = self.theme.ui.fg_ghost.as_f32();
        let selection_bg = with_alpha(self.theme.ui.selection_bg.as_f32(), 0.72);
        let accent_soft = with_alpha(accent, 0.12);
        let accent_border = with_alpha(accent, 0.42);
        let status_bg = with_alpha(self.theme.ui.status_bar_bg.as_f32(), 0.96);
        let titlebar_bg = with_alpha(self.theme.ui.panel_bg.as_f32(), 0.92);

        let mut chrome = vec![
            RegionDrawInstance::new([left_x, panel_y, left_w, panel_h], panel_bg),
            RegionDrawInstance::new([right_x, panel_y, right_w, panel_h], editor_bg),
            RegionDrawInstance::new([left_x, panel_y, left_w, titlebar_h], titlebar_bg),
            RegionDrawInstance::new([right_x, panel_y, right_w, titlebar_h], titlebar_bg),
            RegionDrawInstance::new([right_x - gap * 0.5, panel_y, 1.0, panel_h], divider),
            RegionDrawInstance::new([left_x, panel_y + titlebar_h - 1.0, left_w, 1.0], divider),
            RegionDrawInstance::new([right_x, panel_y + titlebar_h - 1.0, right_w, 1.0], divider),
            RegionDrawInstance::new([left_x, panel_y + header_h - 1.0, left_w, 1.0], divider),
            RegionDrawInstance::new([right_x, panel_y + header_h - 1.0, right_w, 1.0], divider),
            RegionDrawInstance::new(
                [left_x, content_bottom - footer_h, left_w, footer_h],
                status_bg,
            ),
            RegionDrawInstance::new([left_x, content_bottom - footer_h, left_w, 1.0], divider),
        ];
        let mut glyphs = Vec::new();

        let left_text_width = (left_w - 40.0).max(1.0);
        let right_text_width = (right_w - 24.0).max(1.0);

        self.editor_overlay_text_system
            .set_size(Some(left_text_width), Some(line_height));
        glyphs.extend(layout_panel_text(
            "●  ●  ●",
            &mut self.editor_overlay_text_system,
            &mut self.atlas,
            &self.queue,
            left_x + 12.0,
            panel_y + 4.0,
            fg_ghost,
        ));
        glyphs.extend(layout_panel_text(
            "Settings",
            &mut self.editor_overlay_text_system,
            &mut self.atlas,
            &self.queue,
            left_x + 54.0,
            panel_y + 4.0,
            fg,
        ));

        self.editor_overlay_text_system
            .set_size(Some(right_text_width), Some(line_height));
        glyphs.extend(layout_panel_text(
            "settings.toml preview",
            &mut self.editor_overlay_text_system,
            &mut self.atlas,
            &self.queue,
            right_x + 10.0,
            panel_y + 4.0,
            fg_dim,
        ));

        self.editor_overlay_text_system
            .set_size(Some(left_text_width), Some(line_height));
        glyphs.extend(layout_panel_text(
            "Settings",
            &mut self.editor_overlay_text_system,
            &mut self.atlas,
            &self.queue,
            left_x + 14.0,
            panel_y + titlebar_h + 8.0,
            fg,
        ));
        let mode_label = if settings.editing.is_some() {
            "SETTING · EDIT"
        } else {
            "SETTING · NAV"
        };
        glyphs.extend(layout_panel_text(
            mode_label,
            &mut self.editor_overlay_text_system,
            &mut self.atlas,
            &self.queue,
            left_x + 14.0,
            panel_y + titlebar_h + 8.0 + line_height,
            accent,
        ));

        self.editor_overlay_text_system
            .set_size(Some(right_text_width), Some(line_height));
        glyphs.extend(layout_panel_text(
            "CONFIG PREVIEW",
            &mut self.editor_overlay_text_system,
            &mut self.atlas,
            &self.queue,
            right_x + 10.0,
            panel_y + titlebar_h + 8.0,
            fg_ghost,
        ));
        glyphs.extend(layout_panel_text(
            "LIVE",
            &mut self.editor_overlay_text_system,
            &mut self.atlas,
            &self.queue,
            right_x + right_w - estimate_monospace_width("LIVE", font_size) - 12.0,
            panel_y + titlebar_h + 8.0,
            accent,
        ));

        let key_col_w = 220.0;
        let value_col_w = 160.0;
        let desc_x_offset = 220.0;
        let section_gap = 12.0;
        let row_h = line_height * 2.0 + 14.0;
        let mut current_y = content_top + 8.0;
        let mut last_section: Option<SettingsSection> = None;

        for (idx, item) in settings.items.iter().enumerate() {
            let section = item.section();
            if last_section != Some(section) {
                if last_section.is_some() {
                    current_y += section_gap;
                }
                glyphs.extend(layout_panel_text(
                    section.label(),
                    &mut self.editor_overlay_text_system,
                    &mut self.atlas,
                    &self.queue,
                    left_x + 14.0,
                    current_y,
                    fg_ghost,
                ));
                chrome.push(RegionDrawInstance::new(
                    [
                        left_x + 14.0 + 140.0,
                        current_y + 7.0,
                        (left_w - 170.0).max(1.0),
                        1.0,
                    ],
                    divider,
                ));
                current_y += line_height + 4.0;
                last_section = Some(section);
            }

            if current_y + row_h > rows_bottom - 8.0 {
                break;
            }

            let is_selected = idx == settings.selected_index;
            if is_selected {
                chrome.push(
                    RegionDrawInstance::new(
                        [left_x + 6.0, current_y, (left_w - 12.0).max(1.0), row_h],
                        selection_bg,
                    )
                    .with_radius(8.0),
                );
                chrome.push(
                    RegionDrawInstance::new(
                        [left_x + 6.0, current_y + 4.0, 3.0, row_h - 8.0],
                        accent,
                    )
                    .with_radius(2.0),
                );
            }

            let key_x = left_x + 18.0;
            let key_y = current_y + 6.0;
            let desc_x = left_x + desc_x_offset;
            let desc_y = current_y + line_height + 8.0;
            let value_x = left_x + left_w - value_col_w - 18.0;
            let desc_w = (value_x - desc_x - 18.0).max(220.0);

            self.editor_overlay_text_system
                .set_size(Some(key_col_w), Some(line_height));
            glyphs.extend(layout_panel_text(
                &clamp_monospace_text(item.title(), key_col_w, font_size),
                &mut self.editor_overlay_text_system,
                &mut self.atlas,
                &self.queue,
                key_x,
                key_y,
                if is_selected { fg } else { fg_dim },
            ));

            self.editor_overlay_text_system
                .set_size(Some(desc_w), Some(line_height));
            glyphs.extend(layout_panel_text(
                &clamp_monospace_text(item.description(), desc_w, font_size),
                &mut self.editor_overlay_text_system,
                &mut self.atlas,
                &self.queue,
                desc_x,
                desc_y,
                fg_ghost,
            ));

            match item {
                SettingItem::UiRounding { enabled, .. } => {
                    let toggle_w = 34.0;
                    let toggle_h = 18.0;
                    let toggle_x = left_x + left_w - toggle_w - 18.0;
                    let toggle_y = current_y + (row_h - toggle_h) * 0.5;
                    chrome.push(
                        RegionDrawInstance::new(
                            [toggle_x, toggle_y, toggle_w, toggle_h],
                            if *enabled {
                                with_alpha(accent, 0.82)
                            } else {
                                with_alpha(fg_ghost, 0.28)
                            },
                        )
                        .with_radius(9.0),
                    );
                    chrome.push(
                        RegionDrawInstance::new(
                            [
                                if *enabled {
                                    toggle_x + toggle_w - 14.0
                                } else {
                                    toggle_x + 2.0
                                },
                                toggle_y + 2.0,
                                14.0,
                                14.0,
                            ],
                            if *enabled { editor_bg } else { fg_dim },
                        )
                        .with_radius(7.0),
                    );
                }
                _ => {
                    chrome.push(
                        RegionDrawInstance::new(
                            [value_x, current_y + 6.0, value_col_w, line_height + 12.0],
                            if is_selected {
                                accent_soft
                            } else {
                                with_alpha(panel_bg, 0.72)
                            },
                        )
                        .with_radius(5.0),
                    );
                    chrome.push(RegionDrawInstance::new(
                        [value_x, current_y + 6.0, value_col_w, 1.0],
                        if is_selected { accent_border } else { divider },
                    ));
                    self.editor_overlay_text_system
                        .set_size(Some((value_col_w - 16.0).max(1.0)), Some(line_height));
                    glyphs.extend(layout_panel_text(
                        &clamp_monospace_text(
                            &current_row_value(settings, item, is_selected),
                            (value_col_w - 16.0).max(1.0),
                            font_size,
                        ),
                        &mut self.editor_overlay_text_system,
                        &mut self.atlas,
                        &self.queue,
                        value_x + 12.0,
                        current_y + 12.0,
                        if is_selected { accent } else { fg_dim },
                    ));
                }
            }

            chrome.push(RegionDrawInstance::new(
                [
                    left_x + 14.0,
                    current_y + row_h - 2.0,
                    (left_w - 28.0).max(1.0),
                    1.0,
                ],
                divider,
            ));

            current_y += row_h + 4.0;
        }

        let preview_lines = settings_preview_lines(settings);
        let preview_rows = ((content_h - 10.0) / line_height).floor() as usize;
        self.editor_overlay_text_system
            .set_size(Some(right_text_width), Some(line_height));
        for (idx, line) in preview_lines
            .into_iter()
            .take(preview_rows.max(1))
            .enumerate()
        {
            let y = content_top + 8.0 + idx as f32 * line_height;
            let color = if line.starts_with('#') {
                fg_ghost
            } else if line.starts_with('[') {
                accent
            } else {
                fg_dim
            };
            glyphs.extend(layout_panel_text(
                &clamp_monospace_text(&line, right_text_width, font_size),
                &mut self.editor_overlay_text_system,
                &mut self.atlas,
                &self.queue,
                right_x + 10.0,
                y,
                color,
            ));
        }

        let left_status = if settings.editing.is_some() {
            "EDIT  Enter commit · Backspace delete · Esc cancel"
        } else {
            "NAV   Enter edit/open · h/l adjust · j/k move"
        };
        self.editor_overlay_text_system
            .set_size(Some(left_text_width), Some(line_height));
        glyphs.extend(layout_panel_text(
            &clamp_monospace_text(left_status, left_text_width, font_size),
            &mut self.editor_overlay_text_system,
            &mut self.atlas,
            &self.queue,
            left_x + 10.0,
            content_bottom - footer_h + 4.0,
            fg_dim,
        ));

        self.editor_overlay_text_system
            .set_size(Some(left_text_width), Some(line_height));
        let left_status = if settings.editing.is_some() {
            "EDIT  ·  type to change  ·  Enter commit  ·  Esc cancel"
        } else {
            "NAV  ·  Enter edit  ·  h/l adjust  ·  j/k move"
        };
        glyphs.extend(layout_panel_text(
            &clamp_monospace_text(left_status, left_text_width, font_size),
            &mut self.editor_overlay_text_system,
            &mut self.atlas,
            &self.queue,
            left_x + 10.0,
            content_bottom - footer_h + 4.0,
            if settings.editing.is_some() {
                accent
            } else {
                fg_dim
            },
        ));

        self.editor_overlay_chrome_instances = chrome;
        self.editor_overlay_glyph_instances = glyphs;
        self.editor_overlay_text_pipeline.upload_instances(
            &self.device,
            &self.queue,
            &self.editor_overlay_glyph_instances,
        );
    }
}
