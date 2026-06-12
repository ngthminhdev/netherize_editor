use crate::render::{
    icon_pipeline::{IconDrawInstance, canonical_icon_id},
    region_pipeline::RegionDrawInstance,
    renderer::{Renderer, SidebarFilterState, SidebarRow},
};

use super::super::helpers::{estimate_monospace_width, layout_panel_text, rect_to_scissor};

const SIDEBAR_FILTER_BAR_HEIGHT: f32 = 30.0;

fn sidebar_list_top(bounds: [f32; 4], filter_state: Option<&SidebarFilterState>) -> f32 {
    bounds[1]
        + if filter_state.is_some() {
            SIDEBAR_FILTER_BAR_HEIGHT + 1.0
        } else {
            0.0
        }
}

fn sidebar_list_bottom(bounds: [f32; 4]) -> f32 {
    bounds[1] + bounds[3] - 2.0
}

fn sidebar_filter_y(bounds: [f32; 4]) -> f32 {
    bounds[1]
}

impl Renderer {
    /// Render the explorer file tree into the left sidebar region.
    ///
    /// Returns selection quads so the caller can draw them via the
    /// region pipeline *before* the text pass.
    pub fn update_sidebar_content(
        &mut self,
        header: Option<&str>,
        rows: &[SidebarRow],
        bounds: [f32; 4],
        sidebar_focused: bool,
        filter_state: Option<&SidebarFilterState>,
    ) -> Vec<RegionDrawInstance> {
        if bounds[2] < 1.0 || bounds[3] < 1.0 {
            self.sidebar_scissor = None;
            self.sidebar_glyph_instances.clear();
            self.sidebar_icon_instances.clear();
            self.sidebar_text_pipeline
                .upload_instances(&self.device, &self.queue, &[]);
            self.sidebar_icon_pipeline.upload_instances(
                &self.device,
                &self.sidebar_icon_instances,
                [
                    self.surface_state.config.width,
                    self.surface_state.config.height,
                ],
            );
            return Vec::new();
        }

        self.sidebar_scissor = rect_to_scissor(bounds);
        let line_h = self.theme.ui.sidebar_line_height;
        let font_size = self.theme.ui.sidebar_font_size;

        let fg_dim = self.theme.ui.fg_dim.as_f32();
        let fg_ghost = self.theme.ui.fg_ghost.as_f32();
        let accent = self.theme.ui.accent.as_f32();
        let sel_bg = if sidebar_focused {
            [accent[0], accent[1], accent[2], 0.18]
        } else {
            let c = self.theme.ui.selection_bg.as_f32();
            [c[0], c[1], c[2], 0.55]
        };

        let width = (bounds[2] - self.sidebar_base_padding * 2.0).max(1.0);
        self.sidebar_text_system.set_size(Some(width), Some(line_h));

        let mut glyphs = Vec::new();
        let mut icon_instances = Vec::new();
        let mut selection_quads: Vec<RegionDrawInstance> = Vec::new();
        let mut current_y = sidebar_list_top(bounds, filter_state) + self.panel_padding;
        let list_bottom = sidebar_list_bottom(bounds);

        if let Some(filter_state) = filter_state {
            let panel_bg = self.theme.ui.panel_bg.as_f32();
            let border = self.theme.ui.border_color.as_f32();
            let filter_y = sidebar_filter_y(bounds);
            let filter_text = format!("Filter: {}", filter_state.query);
            let text_x = bounds[0] + self.sidebar_base_padding;
            let text_y = filter_y
                + self.panel_padding
                + ((SIDEBAR_FILTER_BAR_HEIGHT - self.panel_padding * 2.0 - line_h).max(0.0) * 0.5);

            selection_quads.push(
                RegionDrawInstance::new(
                    [bounds[0], filter_y, bounds[2], SIDEBAR_FILTER_BAR_HEIGHT],
                    panel_bg,
                )
                .with_radius(
                    self.panel_corner_radius
                        .min(SIDEBAR_FILTER_BAR_HEIGHT * 0.45),
                ),
            );
            selection_quads.push(RegionDrawInstance::new(
                [
                    bounds[0],
                    filter_y + SIDEBAR_FILTER_BAR_HEIGHT,
                    bounds[2],
                    1.0,
                ],
                border,
            ));

            glyphs.extend(layout_panel_text(
                &filter_text,
                &mut self.sidebar_text_system,
                &mut self.atlas,
                &self.queue,
                text_x,
                text_y,
                accent,
            ));

            if filter_state.is_inputting && filter_state.show_cursor {
                let cursor_x = text_x + estimate_monospace_width(&filter_text, font_size);
                let mut cursor_color = accent;
                cursor_color[3] = 0.9;
                selection_quads.push(RegionDrawInstance::new(
                    [cursor_x, text_y + 2.0, 8.0, (line_h - 4.0).max(1.0)],
                    cursor_color,
                ));
            }
        }

        if let Some(header) = header {
            glyphs.extend(layout_panel_text(
                header,
                &mut self.sidebar_text_system,
                &mut self.atlas,
                &self.queue,
                bounds[0] + self.sidebar_base_padding,
                current_y,
                fg_ghost,
            ));
            current_y += line_h;
        }

        for row in rows {
            if current_y + line_h > list_bottom {
                break;
            }

            let x = bounds[0]
                + self.sidebar_base_padding
                + row.depth as f32 * self.sidebar_indent_per_depth;

            let label_base_color = row.git_color.unwrap_or(fg_dim);
            let label_color = if row.is_selected {
                selection_quads.push(
                    RegionDrawInstance::new(
                        [
                            bounds[0] + self.panel_padding * 0.35,
                            current_y,
                            (bounds[2] - self.panel_padding * 0.7).max(0.0),
                            line_h,
                        ],
                        sel_bg,
                    )
                    .with_radius((line_h * 0.3).min(self.panel_corner_radius)),
                );
                if sidebar_focused {
                    row.git_color.unwrap_or(accent)
                } else {
                    label_base_color
                }
            } else {
                label_base_color
            };

            // 1. Disclosure arrow (▶ ▼ ·)
            let arrow_str = format!("{} ", row.arrow);
            let arrow_w = arrow_str.chars().count() as f32 * font_size * 0.60;
            glyphs.extend(layout_panel_text(
                &arrow_str,
                &mut self.sidebar_text_system,
                &mut self.atlas,
                &self.queue,
                x,
                current_y,
                fg_ghost,
            ));

            // 2. File/folder icon. Bearded asset icons are rendered as textured quads;
            // legacy glyph icons still fall back to the text pipeline.
            let icon_color = row.icon_color;
            let icon_slot_w = font_size * 1.55;
            if let Some(asset_icon) = canonical_icon_id(&row.nerd_icon) {
                let icon_size = (line_h * 0.82).min(font_size * 1.35);
                icon_instances.push(IconDrawInstance {
                    icon: asset_icon,
                    rect: [
                        x + arrow_w,
                        current_y + (line_h - icon_size) * 0.5,
                        icon_size,
                        icon_size,
                    ],
                    tint: [1.0, 1.0, 1.0, 1.0],
                });
            } else {
                let nerd_str = format!("{} ", row.nerd_icon);
                glyphs.extend(layout_panel_text(
                    &nerd_str,
                    &mut self.sidebar_text_system,
                    &mut self.atlas,
                    &self.queue,
                    x + arrow_w,
                    current_y,
                    icon_color,
                ));
            }

            let mut label_x = x + arrow_w + icon_slot_w;
            if let (Some(marker), Some(color)) = (row.prefix_marker.as_deref(), row.prefix_color) {
                let marker_text = format!("{} ", marker);
                glyphs.extend(layout_panel_text(
                    &marker_text,
                    &mut self.sidebar_text_system,
                    &mut self.atlas,
                    &self.queue,
                    label_x,
                    current_y,
                    color,
                ));
                label_x += marker_text.chars().count() as f32 * font_size * 0.60;
            }

            if let (Some(marker), Some(color)) = (row.git_marker, row.git_color) {
                let marker_text = format!("{} ", marker);
                glyphs.extend(layout_panel_text(
                    &marker_text,
                    &mut self.sidebar_text_system,
                    &mut self.atlas,
                    &self.queue,
                    label_x,
                    current_y,
                    color,
                ));

                let marker_w = marker_text.chars().count() as f32 * font_size * 0.60;
                glyphs.extend(layout_panel_text(
                    &row.label,
                    &mut self.sidebar_text_system,
                    &mut self.atlas,
                    &self.queue,
                    label_x + marker_w,
                    current_y,
                    label_color,
                ));
            } else {
                // 3. Label
                glyphs.extend(layout_panel_text(
                    &row.label,
                    &mut self.sidebar_text_system,
                    &mut self.atlas,
                    &self.queue,
                    label_x,
                    current_y,
                    label_color,
                ));
            }

            current_y += line_h;
        }

        self.sidebar_icon_instances = icon_instances;
        self.sidebar_icon_pipeline.upload_instances(
            &self.device,
            &self.sidebar_icon_instances,
            [
                self.surface_state.config.width,
                self.surface_state.config.height,
            ],
        );
        self.sidebar_glyph_instances = glyphs;
        self.sidebar_text_pipeline.upload_instances(
            &self.device,
            &self.queue,
            &self.sidebar_glyph_instances,
        );
        selection_quads
    }

    /// Clear sidebar — called when the panel is hidden.
    pub fn clear_sidebar(&mut self) {
        self.sidebar_scissor = None;
        self.sidebar_glyph_instances.clear();
        self.sidebar_icon_instances.clear();
        self.sidebar_text_pipeline
            .upload_instances(&self.device, &self.queue, &[]);
        self.sidebar_icon_pipeline.upload_instances(
            &self.device,
            &self.sidebar_icon_instances,
            [
                self.surface_state.config.width,
                self.surface_state.config.height,
            ],
        );
    }
}
