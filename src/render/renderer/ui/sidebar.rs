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

    /// Render the tab strip and active tab content for the left dock.
    pub fn update_left_dock_panel(
        &mut self,
        lb: [f32; 4],
        labels: &[&str],
        icons: &[Option<&'static str>],
        active: usize,
        strip_h: f32,
        strip_focused: bool,
        explorer_rows: Option<&[SidebarRow]>,
        outline: Option<(
            &[crate::async_runtime::message::LspDocumentSymbol],
            Option<usize>,
            f32,
        )>,
        filter_state: Option<&SidebarFilterState>,
    ) -> Vec<RegionDrawInstance> {
        if lb[2] <= 2.0 || lb[3] <= 2.0 {
            self.clear_sidebar();
            return Vec::new();
        }

        // Inset strip + content so the panel's focus-ring outline stays visible
        let inset = crate::workbench::layout_engine::LEFT_DOCK_OUTLINE_INSET
            .min(lb[2] * 0.5)
            .min(lb[3] * 0.5)
            .max(0.0);
        let ix = lb[0] + inset;
        let iy = lb[1] + inset;
        let iw = (lb[2] - inset * 2.0).max(0.0);
        let ih = (lb[3] - inset * 2.0).max(0.0);
        let strip_h = strip_h.min(ih).max(0.0);

        self.sidebar_scissor = rect_to_scissor([ix, iy, iw, ih]);

        let (mut chrome, mut glyphs, strip_icons) =
            self.build_left_tab_strip([ix, iy, iw, strip_h], labels, icons, active, strip_focused);

        let content_bounds = [ix, iy + strip_h, iw, (ih - strip_h).max(0.0)];

        let mut content_icons = Vec::new();

        if let Some(rows) = explorer_rows {
            // Explorer tab is active
            let mut current_y = sidebar_list_top(content_bounds, filter_state) + self.panel_padding;
            let list_bottom = sidebar_list_bottom(content_bounds);
            let line_h = self.theme.ui.sidebar_line_height;
            let font_size = self.theme.ui.sidebar_font_size;
            let fg_dim = self.theme.ui.fg_dim.as_f32();
            let fg_ghost = self.theme.ui.fg_ghost.as_f32();
            let accent = self.theme.ui.accent.as_f32();
            let sel_bg = if strip_focused {
                [accent[0], accent[1], accent[2], 0.18]
            } else {
                let c = self.theme.ui.selection_bg.as_f32();
                [c[0], c[1], c[2], 0.55]
            };

            // Search filter bar
            if let Some(filter_state) = filter_state {
                let panel_bg = self.theme.ui.panel_bg.as_f32();
                let border = self.theme.ui.border_color.as_f32();
                let filter_y = sidebar_filter_y(content_bounds);
                let filter_text = format!("Filter: {}", filter_state.query);
                let text_x = content_bounds[0] + self.sidebar_base_padding;
                let text_y = filter_y
                    + self.panel_padding
                    + ((SIDEBAR_FILTER_BAR_HEIGHT - self.panel_padding * 2.0 - line_h).max(0.0) * 0.5);

                chrome.push(
                    RegionDrawInstance::new(
                        [content_bounds[0], filter_y, content_bounds[2], SIDEBAR_FILTER_BAR_HEIGHT],
                        panel_bg,
                    )
                    .with_radius(
                        self.panel_corner_radius
                            .min(SIDEBAR_FILTER_BAR_HEIGHT * 0.45),
                    ),
                );

                chrome.push(RegionDrawInstance::new(
                    [
                        content_bounds[0],
                        filter_y + SIDEBAR_FILTER_BAR_HEIGHT,
                        content_bounds[2],
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
                    chrome.push(RegionDrawInstance::new(
                        [cursor_x, text_y + 2.0, 8.0, (line_h - 4.0).max(1.0)],
                        cursor_color,
                    ));
                }
            }

            for row in rows {
                if current_y + line_h > list_bottom {
                    break;
                }

                let x = content_bounds[0]
                    + self.sidebar_base_padding
                    + row.depth as f32 * self.sidebar_indent_per_depth;

                let label_base_color = row.git_color.unwrap_or(fg_dim);
                let label_color = if row.is_selected {
                    chrome.push(
                        RegionDrawInstance::new(
                            [
                                content_bounds[0] + self.panel_padding * 0.35,
                                current_y,
                                (content_bounds[2] - self.panel_padding * 0.7).max(0.0),
                                line_h,
                            ],
                            sel_bg,
                        )
                        .with_radius((line_h * 0.3).min(self.panel_corner_radius)),
                    );
                    if strip_focused {
                        row.git_color.unwrap_or(accent)
                    } else {
                        label_base_color
                    }
                } else {
                    label_base_color
                };

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

                let icon_color = row.icon_color;
                let icon_slot_w = font_size * 1.55;
                if let Some(asset_icon) = canonical_icon_id(&row.nerd_icon) {
                    let icon_size = (line_h * 0.82).min(font_size * 1.35);
                    content_icons.push(IconDrawInstance {
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
        } else if let Some((symbols, selected, inner_padding)) = outline {
            // Outline tab is active
            let (oc, og, oi) = self.build_left_outline_content(content_bounds, symbols, selected, inner_padding);
            chrome.extend(oc);
            glyphs.extend(og);
            content_icons.extend(oi);
        }

        self.sidebar_glyph_instances = glyphs;
        self.sidebar_text_pipeline.upload_instances(
            &self.device,
            &self.queue,
            &self.sidebar_glyph_instances,
        );

        content_icons.extend(strip_icons);
        self.sidebar_icon_instances = content_icons;
        self.sidebar_icon_pipeline.upload_instances(
            &self.device,
            &self.sidebar_icon_instances,
            [
                self.surface_state.config.width,
                self.surface_state.config.height,
            ],
        );

        chrome
    }

    /// Build the left-dock tab strip
    fn build_left_tab_strip(
        &mut self,
        bounds: [f32; 4],
        labels: &[&str],
        icons: &[Option<&'static str>],
        active: usize,
        focused: bool,
    ) -> (Vec<RegionDrawInstance>, Vec<crate::render::glyph_instance::GlyphInstance>, Vec<IconDrawInstance>) {
        let mut chrome: Vec<RegionDrawInstance> = Vec::new();
        let mut glyphs: Vec<crate::render::glyph_instance::GlyphInstance> = Vec::new();
        let mut icon_instances: Vec<IconDrawInstance> = Vec::new();
        if labels.is_empty() || bounds[2] <= 1.0 || bounds[3] <= 1.0 {
            return (chrome, glyphs, icon_instances);
        }

        let font = self.theme.editor.font_size;
        let line_h = self.theme.editor.line_height;
        let fg = self.theme.ui.fg.as_f32();
        let fg_dim = self.theme.ui.fg_dim.as_f32();
        let accent = self.theme.ui.accent.as_f32();
        // Unified tab-bar palette across every dock + the main editor: base on the
        // (darker) editor background, with a subtle lift for the selected tab so it
        // reads as active without going near-white.
        let tab_base = self.theme.editor.bg.as_f32();
        let border = self.theme.ui.border_color.as_f32();
        let active_bg = super::utils::blend_rgb(tab_base, fg, 0.05, tab_base[3]);
        let inactive_bg = tab_base;
        const TOP_BORDER: f32 = 2.0;

        let inset = crate::workbench::layout_engine::LEFT_DOCK_OUTLINE_INSET;
        let radius = (self.panel_corner_radius - inset)
            .min(bounds[3])
            .min(bounds[2] * 0.5)
            .max(0.0);

        chrome.push(
            RegionDrawInstance::new(bounds, inactive_bg)
                .with_corner_radii([radius, radius, 0.0, 0.0]),
        );

        let n = labels.len();
        let tab_w = bounds[2] / n as f32;
        let text_y = bounds[1] + ((bounds[3] - line_h) * 0.5).max(0.0);
        let _char_w = estimate_monospace_width("0", font).max(1.0);
        // Render tab titles at the main-editor title size so every dock tab bar
        // matches it. The text system is shared with the list below, so save and
        // restore its metrics around the labels.
        let saved_metrics = self.sidebar_text_system.buffer_metrics();
        self.sidebar_text_system
            .set_metrics(cosmic_text::Metrics::new(font, line_h));
        for (i, label) in labels.iter().enumerate() {
            let tab_x = bounds[0] + i as f32 * tab_w;
            let is_active = i == active;
            let is_first = i == 0;
            let is_last = i + 1 == n;
            let tab_corners = [
                if is_first { radius } else { 0.0 }, // top-left
                if is_last { radius } else { 0.0 },  // top-right
                0.0,
                0.0,
            ];
            if is_active {
                chrome.push(
                    RegionDrawInstance::new([tab_x, bounds[1], tab_w, bounds[3]], active_bg)
                        .with_corner_radii(tab_corners),
                );
                let bar_col = if focused { accent } else { fg_dim };
                let bar_x = if is_first { tab_x + radius } else { tab_x };
                let mut bar_w = tab_w;
                if is_first {
                    bar_w -= radius;
                }
                if is_last {
                    bar_w -= radius;
                }
                chrome.push(
                    RegionDrawInstance::new(
                        [bar_x, bounds[1], bar_w.max(0.0), TOP_BORDER],
                        bar_col,
                    )
                    .with_corner_radii([
                        if is_first { radius } else { 0.0 },
                        if is_last { radius } else { 0.0 },
                        0.0,
                        0.0,
                    ]),
                );
            }
            if i + 1 < n {
                let mut sep = border;
                sep[3] *= 0.5;
                chrome.push(RegionDrawInstance::new(
                    [
                        tab_x + tab_w - 0.5,
                        bounds[1] + 6.0,
                        1.0,
                        (bounds[3] - 12.0).max(0.0),
                    ],
                    sep,
                ));
            }
            let label_color = if is_active { fg } else { fg_dim };
            let icon = icons.get(i).and_then(|id| *id);
            if let Some(icon_id) = icon.and_then(|id| canonical_icon_id(id)) {
                let icon_size = (line_h * 0.72).min(font * 1.3);
                let label_w = estimate_monospace_width(label, font);
                let total_w = icon_size + 4.0 + label_w;
                let start_x = tab_x + ((tab_w - total_w) * 0.5).max(4.0);
                icon_instances.push(IconDrawInstance {
                    icon: icon_id,
                    rect: [
                        start_x,
                        bounds[1] + ((bounds[3] - icon_size) * 0.5).max(0.0),
                        icon_size,
                        icon_size,
                    ],
                    tint: label_color,
                });
                glyphs.extend(layout_panel_text(
                    label,
                    &mut self.sidebar_text_system,
                    &mut self.atlas,
                    &self.queue,
                    start_x + icon_size + 4.0,
                    text_y,
                    label_color,
                ));
            } else {
                let label_w = estimate_monospace_width(label, font);
                let start_x = tab_x + ((tab_w - label_w) * 0.5).max(4.0);
                glyphs.extend(layout_panel_text(
                    label,
                    &mut self.sidebar_text_system,
                    &mut self.atlas,
                    &self.queue,
                    start_x,
                    text_y,
                    label_color,
                ));
            }
        }
        self.sidebar_text_system.set_metrics(saved_metrics);
        (chrome, glyphs, icon_instances)
    }

    /// Hit-test a point against the left-dock tab strip.
    pub fn left_dock_tab_index_at(
        &self,
        tab_count: usize,
        strip_bounds: [f32; 4],
        pos: (f32, f32),
    ) -> Option<usize> {
        if tab_count == 0 {
            return None;
        }
        let (px, py) = pos;
        if px < strip_bounds[0]
            || px >= strip_bounds[0] + strip_bounds[2]
            || py < strip_bounds[1]
            || py >= strip_bounds[1] + strip_bounds[3]
        {
            return None;
        }
        let tab_w = strip_bounds[2] / tab_count as f32;
        if tab_w <= 0.0 {
            return None;
        }
        let idx = ((px - strip_bounds[0]) / tab_w) as usize;
        Some(idx.min(tab_count - 1))
    }

    /// Render outline content for the left dock.
    fn build_left_outline_content(
        &mut self,
        bounds: [f32; 4],
        symbols: &[crate::async_runtime::message::LspDocumentSymbol],
        selected: Option<usize>,
        inner_padding: f32,
    ) -> (
        Vec<RegionDrawInstance>,
        Vec<crate::render::glyph_instance::GlyphInstance>,
        Vec<IconDrawInstance>,
    ) {
        let mut chrome: Vec<RegionDrawInstance> = Vec::new();
        let mut glyphs: Vec<crate::render::glyph_instance::GlyphInstance> = Vec::new();
        let mut icons: Vec<IconDrawInstance> = Vec::new();
        if bounds[2] <= 2.0 || bounds[3] <= 2.0 {
            return (chrome, glyphs, icons);
        }

        let scale = self.ui_scale.max(0.5);
        let pad = inner_padding.max(8.0 * scale);
        let font = self.theme.ui.sidebar_font_size.max(11.0);
        let line_h = self.theme.ui.sidebar_line_height.max(font + 4.0);
        let fg = self.theme.ui.fg.as_f32();
        let fg_dim = self.theme.ui.fg_dim.as_f32();
        let fg_ghost = self.theme.ui.fg_ghost.as_f32();
        let selection_bg = self.theme.ui.selection_bg.as_f32();

        let x0 = bounds[0] + pad;
        let bottom = bounds[1] + bounds[3];
        let mut y = bounds[1] + pad * 0.5;

        if symbols.is_empty() {
            glyphs.extend(layout_panel_text(
                "No symbols — open a file with a language server.",
                &mut self.sidebar_text_system,
                &mut self.atlas,
                &self.queue,
                x0,
                y,
                fg_ghost,
            ));
            return (chrome, glyphs, icons);
        }

        let char_w = estimate_monospace_width("0", font).max(1.0);
        let indent_w = char_w * 2.0;
        for (i, sym) in symbols.iter().enumerate() {
            if y + line_h > bottom + 1.0 {
                break;
            }
            let depth = sym.ancestors.len() as f32;
            let row_x = x0 + depth * indent_w;

            if Some(i) == selected {
                chrome.push(RegionDrawInstance::new(
                    [bounds[0], y - 1.0, bounds[2], line_h],
                    selection_bg,
                ));
            }

            // Right-aligned dim line number
            let line_no = (sym.range.start.line + 1).to_string();
            let num_w = estimate_monospace_width(&line_no, font);
            let num_x = bounds[0] + bounds[2] - pad - num_w;
            glyphs.extend(layout_panel_text(
                &line_no,
                &mut self.sidebar_text_system,
                &mut self.atlas,
                &self.queue,
                num_x,
                y,
                fg_ghost,
            ));

            let icon_color = super::test_runner::outline_kind_color(
                &sym.kind,
                self.theme.ui.info.as_f32(),
                self.theme.ui.warning.as_f32(),
                self.theme.ui.success.as_f32(),
                fg_dim,
            );
            let icon_size = line_h.min(18.0 * scale).max(12.0);
            if let Some(icon) = super::test_runner::outline_symbol_icon_id(&sym.kind) {
                icons.push(IconDrawInstance {
                    icon,
                    rect: [row_x, y + (line_h - icon_size) * 0.5, icon_size, icon_size],
                    tint: icon_color,
                });
            }

            let name_x = row_x + icon_size + char_w;
            let avail = (num_x - name_x - char_w).max(0.0);
            let max_chars = (avail / char_w) as usize;
            let name = super::test_runner::clip_chars(&sym.name, max_chars.max(1));
            let name_color = if Some(i) == selected { fg } else { fg_dim };
            glyphs.extend(layout_panel_text(
                &name,
                &mut self.sidebar_text_system,
                &mut self.atlas,
                &self.queue,
                name_x,
                y,
                name_color,
            ));

            y += line_h;
        }

        (chrome, glyphs, icons)
    }
}
