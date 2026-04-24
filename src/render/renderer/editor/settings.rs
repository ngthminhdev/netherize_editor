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
    pub fn update_settings_buffer_content(
        &mut self,
        settings: &SettingsState,
        center_bounds: [f32; 4],
    ) {
        if center_bounds[2] < 1.0 || center_bounds[3] < 1.0 {
            self.clear_editor_overlays();
            return;
        }

        let font_size = self.theme.ui.panel_font_size.max(1.0);
        let line_height = self.theme.ui.panel_line_height.max(font_size + 4.0);
        self.editor_overlay_text_system
            .set_metrics(Metrics::new(font_size, line_height));
        self.editor_overlay_scissor = rect_to_scissor(center_bounds);

        let panel_x = center_bounds[0] + self.editor_padding_x.max(12.0);
        let panel_y = center_bounds[1] + self.editor_padding_y.max(12.0);
        let panel_w = (center_bounds[2] - self.editor_padding_x.max(12.0) * 2.0).max(1.0);
        let panel_h = (center_bounds[3] - self.editor_padding_y.max(12.0) * 2.0).max(1.0);

        let bg = self.theme.editor.bg.as_f32();
        let panel_bg = self.theme.ui.panel_bg.as_f32();
        let border = self.theme.ui.border_color.as_f32();
        let fg = self.theme.ui.fg.as_f32();
        let fg_dim = self.theme.ui.fg_dim.as_f32();
        let fg_ghost = self.theme.ui.fg_ghost.as_f32();
        let selection_bg = self.theme.ui.selection_bg.as_f32();
        let accent = self.theme.ui.accent.as_f32();
        let panel_radius = settings
            .items
            .iter()
            .find_map(|item| match item {
                SettingItem::UiRounding { enabled, radius_px } if *enabled => Some(*radius_px),
                _ => None,
            })
            .unwrap_or(0.0);

        let mut chrome = vec![
            RegionDrawInstance::new([panel_x, panel_y, panel_w, panel_h], bg)
                .with_radius(panel_radius),
            RegionDrawInstance::new([panel_x, panel_y, panel_w, 1.0], border),
        ];
        let mut glyphs = Vec::new();

        self.editor_overlay_text_system
            .set_size(Some((panel_w - 20.0).max(1.0)), Some(line_height));
        glyphs.extend(layout_panel_text(
            "Settings",
            &mut self.editor_overlay_text_system,
            &mut self.atlas,
            &self.queue,
            panel_x + 10.0,
            panel_y + 8.0,
            fg,
        ));
        glyphs.extend(layout_panel_text(
            &clamp_monospace_text(
                "j/k: navigate  |  Enter/l: change  |  Esc/q: close",
                (panel_w - 20.0).max(1.0),
                font_size,
            ),
            &mut self.editor_overlay_text_system,
            &mut self.atlas,
            &self.queue,
            panel_x + 10.0,
            panel_y + 8.0 + line_height,
            fg_ghost,
        ));

        let row_h = line_height + 10.0;
        let start_y = panel_y + line_height * 2.0 + 20.0;
        for (idx, item) in settings.items.iter().enumerate() {
            let row_y = start_y + idx as f32 * row_h;
            if row_y + row_h > panel_y + panel_h - 8.0 {
                break;
            }

            if idx == settings.selected_index {
                chrome.push(RegionDrawInstance::new(
                    [panel_x + 6.0, row_y - 2.0, (panel_w - 12.0).max(1.0), row_h],
                    selection_bg,
                ));
                chrome.push(RegionDrawInstance::new(
                    [panel_x + 6.0, row_y - 2.0, 3.0, row_h],
                    accent,
                ));
            }

            let value = match item {
                SettingItem::ThemeSelector { current } => current.clone(),
                SettingItem::FontFamily { current } => {
                    if current.trim().is_empty() {
                        "<default>".to_string()
                    } else {
                        current.clone()
                    }
                }
                SettingItem::FontSize { current } => format!("{current:.1}"),
                SettingItem::LineHeight { current } => format!("{current:.1}"),
                SettingItem::SidebarWidth { current }
                | SettingItem::RightSidebarWidth { current }
                | SettingItem::BottomPanelHeight { current } => format!("{} px", current),
                SettingItem::UiRounding { enabled, radius_px } => {
                    if !*enabled || *radius_px <= 0.0 {
                        "Off".to_string()
                    } else if *radius_px < 12.0 {
                        "8 px".to_string()
                    } else {
                        "16 px".to_string()
                    }
                }
            };

            let line = format!("{: <20} {}", item.title(), value);
            let display_line = if idx == settings.selected_index {
                if let Some(editing) = &settings.editing {
                    match (&editing.kind, item) {
                        (
                            crate::app::app_state::SettingsEditingKind::FontFamily,
                            SettingItem::FontFamily { .. },
                        )
                        | (
                            crate::app::app_state::SettingsEditingKind::FontSize,
                            SettingItem::FontSize { .. },
                        )
                        | (
                            crate::app::app_state::SettingsEditingKind::LineHeight,
                            SettingItem::LineHeight { .. },
                        )
                        | (
                            crate::app::app_state::SettingsEditingKind::SidebarWidth,
                            SettingItem::SidebarWidth { .. },
                        )
                        | (
                            crate::app::app_state::SettingsEditingKind::RightSidebarWidth,
                            SettingItem::RightSidebarWidth { .. },
                        )
                        | (
                            crate::app::app_state::SettingsEditingKind::BottomPanelHeight,
                            SettingItem::BottomPanelHeight { .. },
                        ) => {
                            format!("{: <20} {}_", item.title(), editing.draft)
                        }
                        _ => line.clone(),
                    }
                } else {
                    line.clone()
                }
            } else {
                line.clone()
            };
            self.editor_overlay_text_system
                .set_size(Some((panel_w - 28.0).max(1.0)), Some(line_height));
            glyphs.extend(layout_panel_text(
                &clamp_monospace_text(&display_line, (panel_w - 28.0).max(1.0), font_size),
                &mut self.editor_overlay_text_system,
                &mut self.atlas,
                &self.queue,
                panel_x + 14.0,
                row_y,
                if idx == settings.selected_index {
                    fg
                } else {
                    fg_dim
                },
            ));
            chrome.push(RegionDrawInstance::new(
                [
                    panel_x + 10.0,
                    row_y + row_h - 4.0,
                    (panel_w - 20.0).max(1.0),
                    1.0,
                ],
                panel_bg,
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
