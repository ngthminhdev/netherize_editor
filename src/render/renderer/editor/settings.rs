use cosmic_text::Metrics;

use crate::{
    app::app_state::{SettingItem, SettingsEditingKind, SettingsState},
    render::{region_pipeline::RegionDrawInstance, renderer::Renderer},
};

use super::super::{
    components::{ShortcutHintSegment, layout_shortcut_hint},
    helpers::{
        clamp_monospace_text, estimate_monospace_width, layout_panel_text, layout_panel_text_bold,
        rect_to_scissor,
    },
};

#[derive(Clone, Copy, PartialEq, Eq)]
enum SettingsSection {
    Appearance,
    Typography,
    Editor,
    Ai,
    Layout,
}

impl SettingsSection {
    fn label(self) -> &'static str {
        match self {
            Self::Appearance => "APPEARANCE",
            Self::Typography => "TYPOGRAPHY",
            Self::Editor => "EDITOR",
            Self::Ai => "AI",
            Self::Layout => "LAYOUT",
        }
    }
}

impl SettingItem {
    fn section(&self) -> SettingsSection {
        match self {
            Self::ThemeSelector { .. } | Self::UiRounding { .. } | Self::EnableOutline { .. } => {
                SettingsSection::Appearance
            }
            Self::FontFamily { .. } | Self::FontSize { .. } | Self::LineHeight { .. } => {
                SettingsSection::Typography
            }
            Self::IndentTabWidth { .. } | Self::IndentInsertSpaces { .. } => {
                SettingsSection::Editor
            }
            Self::InlineSuggestion { .. } => SettingsSection::Ai,
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
            Self::IndentTabWidth { .. } => {
                "Spaces inserted per Tab key press. Use h/l or Enter to edit (range 1–8)."
            }
            Self::UiRounding { .. } => {
                "Corner radius for panels and shells. Use h/l for quick 8 px steps or Enter to edit (range 0–24)."
            }
            Self::IndentInsertSpaces { .. } => {
                "Press Enter to toggle: spaces keep indent visible; tabs compress display."
            }
            Self::InlineSuggestion { .. } => {
                "Press Enter to toggle AI inline completion from config/ai.toml."
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
            Self::EnableOutline { .. } => {
                "Show borders on all panels. Off = only unfocused panels keep their accent border."
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
            Self::IndentTabWidth { current } => current.to_string(),
            Self::IndentInsertSpaces { enabled }
            | Self::InlineSuggestion { enabled }
            | Self::EnableOutline { enabled } => {
                if *enabled { "true" } else { "false" }.to_string()
            }
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
            Self::IndentTabWidth { current } => format!("{current} spaces"),
            Self::IndentInsertSpaces { enabled } => {
                if *enabled { "Spaces" } else { "Tabs" }.to_string()
            }
            Self::InlineSuggestion { enabled } => {
                if *enabled { "Enabled" } else { "Disabled" }.to_string()
            }
            Self::EnableOutline { enabled } => if *enabled { "On" } else { "Off" }.to_string(),
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

fn setting_value_ratio(item: &SettingItem) -> Option<f32> {
    let ratio = match item {
        SettingItem::FontSize { current } => (*current - 8.0) / (40.0 - 8.0),
        SettingItem::LineHeight { current } => (*current - 10.0) / (64.0 - 10.0),
        SettingItem::IndentTabWidth { current } => (*current as f32 - 1.0) / (8.0 - 1.0),
        SettingItem::UiRounding { enabled, radius_px } => {
            if *enabled {
                *radius_px / 24.0
            } else {
                0.0
            }
        }
        SettingItem::SidebarWidth { current } => (*current as f32 - 160.0) / (1280.0 - 160.0),
        SettingItem::RightSidebarWidth { current } => (*current as f32 - 180.0) / (1440.0 - 180.0),
        SettingItem::BottomPanelHeight { current } => (*current as f32 - 120.0) / (1040.0 - 120.0),
        _ => return None,
    };
    Some(ratio.clamp(0.0, 1.0))
}

fn setting_control_hint(item: &SettingItem) -> &'static [(&'static str, &'static str)] {
    match item {
        SettingItem::ThemeSelector { .. } => &[("Enter", "theme selector")],
        SettingItem::EnableOutline { .. }
        | SettingItem::IndentInsertSpaces { .. }
        | SettingItem::InlineSuggestion { .. } => &[("Enter", "toggle")],
        _ => &[("h/l", "adjust"), ("Enter", "edit exact")],
    }
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
        | (SettingsEditingKind::IndentTabWidth, SettingItem::IndentTabWidth { .. })
        | (SettingsEditingKind::UiRounding, SettingItem::UiRounding { .. })
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

    lines.extend(["".to_string(), "[indent]".to_string()]);
    for item in &settings.items {
        match item {
            SettingItem::IndentTabWidth { .. } => {
                lines.push(format!("tab_width = {}", item.preview_value()))
            }
            SettingItem::IndentInsertSpaces { .. } => {
                lines.push(format!("insert_spaces = {}", item.preview_value()))
            }
            _ => {}
        }
    }

    lines.extend(["".to_string(), "[ai.inline_completion]".to_string()]);
    for item in &settings.items {
        if let SettingItem::InlineSuggestion { .. } = item {
            lines.push(format!("enabled = {}", item.preview_value()));
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
            SettingItem::EnableOutline { .. } => {
                lines.push(format!("enable_outline = {}", item.preview_value()))
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
        let header_h = line_height * 2.0 + 46.0 + titlebar_h;
        let footer_h = line_height + 12.0;
        let content_top = panel_y + header_h;
        let content_bottom = panel_y + panel_h;
        let rows_bottom = (content_bottom - footer_h).max(content_top + line_height);

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

        let selected_item = settings.selected_item();
        let item_count_label = format!("{} SETTINGS", settings.items.len());

        self.editor_overlay_text_system
            .set_size(Some(left_text_width), Some(line_height));
        glyphs.extend(layout_panel_text(
            &item_count_label,
            &mut self.editor_overlay_text_system,
            &mut self.atlas,
            &self.queue,
            left_x + left_w - estimate_monospace_width(&item_count_label, font_size) - 14.0,
            panel_y + titlebar_h + 8.0,
            fg_ghost,
        ));

        if let Some(item) = selected_item {
            let shortcuts_y = panel_y + titlebar_h + 14.0 + line_height * 2.0;
            let shortcut_font = (font_size * 0.72).max(10.0);
            let shortcut_line_h = (line_height * 0.72).max(14.0);
            let key_h = (line_height * 0.82).max(shortcut_line_h + 7.0).max(16.0);
            self.editor_overlay_text_system
                .set_metrics(Metrics::new(shortcut_font, shortcut_line_h));
            self.editor_overlay_text_system
                .set_size(None, Some(shortcut_line_h));
            let mut shortcut_x = left_x + 14.0;
            let key_text_gap = 10.0;
            let group_gap = 22.0;
            let key_pad_x = 7.0;
            for (key, label) in setting_control_hint(item) {
                let key_w = estimate_monospace_width(key, shortcut_font) + key_pad_x * 2.0;
                let label_w = estimate_monospace_width(label, shortcut_font);
                if shortcut_x + key_w + key_text_gap + label_w > left_x + left_w - 14.0 {
                    break;
                }
                let mut key_outline = accent;
                key_outline[3] = key_outline[3].max(0.82);
                let mut key_fill = editor_bg;
                key_fill[3] = key_fill[3].max(0.92);
                chrome.push(
                    RegionDrawInstance::new(
                        [shortcut_x, shortcuts_y - 1.0, key_w, key_h],
                        key_outline,
                    )
                    .with_radius(4.0),
                );
                chrome.push(
                    RegionDrawInstance::new(
                        [
                            shortcut_x + 1.5,
                            shortcuts_y + 0.5,
                            key_w - 3.0,
                            key_h - 3.0,
                        ],
                        key_fill,
                    )
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
                    fg_dim,
                ));
                shortcut_x += key_w + key_text_gap + label_w + group_gap;
            }
            self.editor_overlay_text_system
                .set_metrics(Metrics::new(font_size, line_height));
            self.editor_overlay_text_system
                .set_size(Some(left_text_width), Some(line_height));
        }

        let key_col_w = 400.0;
        let value_col_w = 300.0;
        let desc_x_offset = 410.0;
        let section_gap = 14.0;
        let row_h = line_height * 2.0 + 20.0;

        // ── Virtual layout pre-pass ───────────────────────────────────────────
        // Compute each item's virtual Y (relative to content_top + 8.0 at scroll=0)
        // so we can derive a scroll offset that keeps the selected item visible.
        struct ItemLayout {
            sec_header: Option<SettingsSection>,
            sec_y: f32,
            item_y: f32,
        }
        let mut layouts: Vec<ItemLayout> = Vec::with_capacity(settings.items.len());
        let total_virt_h;
        {
            let mut vy = 0.0f32;
            let mut last_sec: Option<SettingsSection> = None;
            for item in &settings.items {
                let sec = item.section();
                let (sec_header, sec_y) = if last_sec != Some(sec) {
                    if last_sec.is_some() {
                        vy += section_gap;
                    }
                    let y = vy;
                    vy += line_height + 4.0;
                    last_sec = Some(sec);
                    (Some(sec), y)
                } else {
                    (None, 0.0)
                };
                layouts.push(ItemLayout {
                    sec_header,
                    sec_y,
                    item_y: vy,
                });
                vy += row_h + 4.0;
            }
            total_virt_h = vy;
        }

        // Compute scroll_y: enough to keep selected item fully visible at the bottom.
        let vis_h = (rows_bottom - content_top - 8.0).max(1.0);
        let sel_virt_y = layouts
            .get(settings.selected_index)
            .map(|l| l.item_y)
            .unwrap_or(0.0);
        let scroll_y = (sel_virt_y + row_h - vis_h + 8.0).max(0.0);
        let base_y = content_top + 8.0 - scroll_y;

        // Scrollbar (shown only when content overflows)
        if total_virt_h > vis_h {
            let track_x = left_x + left_w - 7.0;
            let track_top = content_top + 4.0;
            let track_h = (rows_bottom - footer_h - track_top - 4.0).max(8.0);
            let thumb_h = (vis_h / total_virt_h * track_h).max(18.0).min(track_h);
            let thumb_y = track_top + (scroll_y / total_virt_h * track_h).min(track_h - thumb_h);
            chrome.push(
                RegionDrawInstance::new(
                    [track_x, track_top, 3.0, track_h],
                    with_alpha(fg_ghost, 0.12),
                )
                .with_radius(1.5),
            );
            chrome.push(
                RegionDrawInstance::new(
                    [track_x, thumb_y, 3.0, thumb_h],
                    with_alpha(fg_ghost, 0.45),
                )
                .with_radius(1.5),
            );
        }

        // ── Main render loop with scroll ─────────────────────────────────────
        for (idx, (item, layout)) in settings.items.iter().zip(layouts.iter()).enumerate() {
            let item_abs_y = base_y + layout.item_y;

            // Render section header if this item starts a new section and header is visible.
            if let Some(sec) = layout.sec_header {
                let sec_abs_y = base_y + layout.sec_y;
                if sec_abs_y + line_height > content_top && sec_abs_y < rows_bottom - 8.0 {
                    self.editor_overlay_text_system
                        .set_size(Some(key_col_w), Some(line_height));
                    glyphs.extend(layout_panel_text(
                        &clamp_monospace_text(sec.label(), key_col_w, font_size),
                        &mut self.editor_overlay_text_system,
                        &mut self.atlas,
                        &self.queue,
                        left_x + 14.0,
                        sec_abs_y,
                        fg_ghost,
                    ));
                    chrome.push(RegionDrawInstance::new(
                        [
                            left_x + 14.0 + key_col_w,
                            sec_abs_y + 7.0,
                            (left_w - (key_col_w + 36.0)).max(1.0),
                            1.0,
                        ],
                        divider,
                    ));
                }
            }

            // Skip items that are entirely outside the viewport.
            if item_abs_y + row_h <= content_top || item_abs_y >= rows_bottom - 8.0 {
                continue;
            }

            let is_selected = idx == settings.selected_index;
            if is_selected {
                chrome.push(
                    RegionDrawInstance::new(
                        [left_x + 6.0, item_abs_y, (left_w - 18.0).max(1.0), row_h],
                        selection_bg,
                    )
                    .with_radius(8.0),
                );
                chrome.push(
                    RegionDrawInstance::new(
                        [left_x + 6.0, item_abs_y + 4.0, 3.0, row_h - 8.0],
                        accent,
                    )
                    .with_radius(2.0),
                );
            }

            let key_x = left_x + 18.0;
            let key_y = item_abs_y + 6.0;
            let desc_x = left_x + desc_x_offset;
            let desc_y = item_abs_y + line_height + 8.0;
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
                SettingItem::EnableOutline { enabled }
                | SettingItem::IndentInsertSpaces { enabled }
                | SettingItem::InlineSuggestion { enabled } => {
                    let toggle_w = 40.0;
                    let toggle_h = 20.0;
                    let toggle_x = left_x + left_w - toggle_w - 22.0;
                    let toggle_y = item_abs_y + (row_h - toggle_h) * 0.5;
                    chrome.push(
                        RegionDrawInstance::new(
                            [toggle_x, toggle_y, toggle_w, toggle_h],
                            if *enabled {
                                with_alpha(accent, 0.82)
                            } else {
                                with_alpha(fg_ghost, 0.28)
                            },
                        )
                        .with_radius(10.0),
                    );
                    chrome.push(
                        RegionDrawInstance::new(
                            [
                                if *enabled {
                                    toggle_x + toggle_w - 16.0
                                } else {
                                    toggle_x + 2.0
                                },
                                toggle_y + 2.0,
                                16.0,
                                16.0,
                            ],
                            if *enabled { editor_bg } else { fg_dim },
                        )
                        .with_radius(8.0),
                    );
                    let badge_label = match item {
                        SettingItem::IndentInsertSpaces { enabled } => {
                            if *enabled {
                                "SPC"
                            } else {
                                "TAB"
                            }
                        }
                        SettingItem::InlineSuggestion { .. } => "OFF",
                        _ => "",
                    };
                    if !badge_label.is_empty() {
                        let badge_x =
                            toggle_x - estimate_monospace_width(badge_label, font_size) - 10.0;
                        self.editor_overlay_text_system
                            .set_size(Some(80.0), Some(line_height));
                        glyphs.extend(layout_panel_text(
                            badge_label,
                            &mut self.editor_overlay_text_system,
                            &mut self.atlas,
                            &self.queue,
                            badge_x,
                            toggle_y + (toggle_h - line_height) * 0.5,
                            if *enabled { accent } else { fg_ghost },
                        ));
                    }
                }
                _ => {
                    chrome.push(
                        RegionDrawInstance::new(
                            [value_x, item_abs_y + 6.0, value_col_w, line_height + 12.0],
                            if is_selected {
                                accent_soft
                            } else {
                                with_alpha(panel_bg, 0.72)
                            },
                        )
                        .with_radius(5.0),
                    );
                    chrome.push(RegionDrawInstance::new(
                        [value_x, item_abs_y + 6.0, value_col_w, 1.0],
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
                        item_abs_y + 12.0,
                        if is_selected { accent } else { fg_dim },
                    ));
                }
            }

            if let Some(ratio) = setting_value_ratio(item) {
                let meter_w = 96.0;
                let meter_h = 3.0;
                let meter_x = value_x + value_col_w - meter_w - 8.0;
                let meter_y = item_abs_y + row_h - 10.0;
                chrome.push(
                    RegionDrawInstance::new(
                        [meter_x, meter_y, meter_w, meter_h],
                        with_alpha(fg_ghost, 0.18),
                    )
                    .with_radius(1.5),
                );
                chrome.push(
                    RegionDrawInstance::new(
                        [meter_x, meter_y, meter_w * ratio, meter_h],
                        if is_selected {
                            accent
                        } else {
                            with_alpha(accent, 0.48)
                        },
                    )
                    .with_radius(1.5),
                );
            }

            chrome.push(RegionDrawInstance::new(
                [
                    left_x + 14.0,
                    item_abs_y + row_h - 2.0,
                    (left_w - 28.0).max(1.0),
                    1.0,
                ],
                divider,
            ));
        }

        let mut preview_start_y = content_top + 8.0;
        if let Some(item) = selected_item {
            let detail_top = content_top + 8.0;
            let detail_h = line_height * 3.0 + 22.0;
            preview_start_y = detail_top + detail_h + 12.0;
            chrome.push(
                RegionDrawInstance::new(
                    [right_x + 10.0, detail_top, right_w - 20.0, detail_h],
                    with_alpha(panel_bg, 0.55),
                )
                .with_radius(8.0),
            );
            chrome.push(
                RegionDrawInstance::new([right_x + 10.0, detail_top, 3.0, detail_h], accent)
                    .with_radius(2.0),
            );
            self.editor_overlay_text_system
                .set_size(Some(right_text_width - 20.0), Some(line_height));
            glyphs.extend(layout_panel_text(
                &clamp_monospace_text(item.title(), right_text_width - 20.0, font_size),
                &mut self.editor_overlay_text_system,
                &mut self.atlas,
                &self.queue,
                right_x + 22.0,
                detail_top + 8.0,
                fg,
            ));
            glyphs.extend(layout_panel_text(
                &clamp_monospace_text(&item.display_value(), right_text_width - 20.0, font_size),
                &mut self.editor_overlay_text_system,
                &mut self.atlas,
                &self.queue,
                right_x + 22.0,
                detail_top + 8.0 + line_height,
                accent,
            ));
            glyphs.extend(layout_panel_text(
                &clamp_monospace_text(item.description(), right_text_width - 24.0, font_size),
                &mut self.editor_overlay_text_system,
                &mut self.atlas,
                &self.queue,
                right_x + 22.0,
                detail_top + 8.0 + line_height * 2.0,
                fg_ghost,
            ));
        }

        let preview_lines = settings_preview_lines(settings);
        let preview_h = (rows_bottom - preview_start_y - 8.0).max(line_height);
        let preview_rows = (preview_h / line_height).floor() as usize;
        self.editor_overlay_text_system
            .set_size(Some(right_text_width), Some(line_height));
        for (idx, line) in preview_lines
            .into_iter()
            .take(preview_rows.max(1))
            .enumerate()
        {
            let y = preview_start_y + idx as f32 * line_height;
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

        self.editor_overlay_text_system
            .set_size(Some(left_text_width), Some(line_height));
        let footer_y = content_bottom - footer_h + 4.0;
        let left_status = if settings.editing.is_some() {
            [
                ShortcutHintSegment::Keys(&["EDIT"]),
                ShortcutHintSegment::Text("type to change  ·"),
                ShortcutHintSegment::Keys(&["Enter"]),
                ShortcutHintSegment::Text("commit  ·"),
                ShortcutHintSegment::Keys(&["󱊷"]),
                ShortcutHintSegment::Text("cancel"),
            ]
        } else {
            [
                ShortcutHintSegment::Keys(&["NAV"]),
                ShortcutHintSegment::Text("Enter edit  ·"),
                ShortcutHintSegment::Keys(&["h", "l"]),
                ShortcutHintSegment::Text("adjust  ·"),
                ShortcutHintSegment::Keys(&["j", "k"]),
                ShortcutHintSegment::Text("move"),
            ]
        };
        glyphs.extend(layout_shortcut_hint(
            &left_status,
            &mut self.editor_overlay_text_system,
            &mut self.atlas,
            &self.queue,
            &mut chrome,
            left_x + 10.0,
            footer_y,
            font_size,
            line_height,
            if settings.editing.is_some() {
                accent
            } else {
                fg_dim
            },
            panel_bg,
            divider,
            fg,
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
