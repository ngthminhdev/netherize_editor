use cosmic_text::Metrics;

use crate::render::{
    icon_pipeline::IconDrawInstance,
    region_pipeline::RegionDrawInstance,
    renderer::{Renderer, TextScissorBatch, TopbarLayoutKey, TopbarTab},
    text_pipeline::InstanceDrawRange,
};

use super::super::helpers::{
    estimate_monospace_width, layout_panel_text, layout_panel_text_bold, rect_to_scissor,
};

fn inset_scissor_rect(bounds: [f32; 4], inset_x: f32, inset_y: f32) -> Option<[u32; 4]> {
    let clip_x = bounds[0] + inset_x;
    let clip_y = bounds[1] + inset_y;
    let clip_width = (bounds[2] - inset_x * 2.0).max(0.0);
    let clip_height = (bounds[3] - inset_y * 2.0).max(0.0);
    rect_to_scissor([clip_x, clip_y, clip_width, clip_height])
}

fn topbar_tab_text_scissor(bounds: [f32; 4]) -> Option<[u32; 4]> {
    inset_scissor_rect(bounds, 4.0, 2.0)
}

fn clamp_project_name(text: &str, max_width: f32, font_size: f32) -> String {
    if text.is_empty() || max_width <= 0.0 {
        return String::new();
    }
    if estimate_monospace_width(text, font_size) <= max_width {
        return text.to_string();
    }
    let char_width = (font_size * 0.6).max(1.0);
    let max_chars = (max_width / char_width).floor() as usize;
    if max_chars == 0 {
        return String::new();
    }
    if text.chars().count() <= max_chars {
        return text.to_string();
    }
    if max_chars <= 3 {
        let count = text.chars().count();
        return text.chars().skip(count - max_chars).collect();
    }
    let count = text.chars().count();
    let mut shortened = String::from("...");
    let suffix: String = text.chars().skip(count - (max_chars - 3)).collect();
    shortened.push_str(&suffix);
    shortened
}

const TOPBAR_TRAFFIC_LIGHT_SPACE_MACOS: f32 = 150.0;
const TOPBAR_TAB_PADDING_X: f32 = 12.0;
const TOPBAR_TAB_SEPARATOR_WIDTH: f32 = 1.0;
const TOPBAR_ACTIVE_BORDER_HEIGHT: f32 = 2.0;
const TOPBAR_DIRTY_DOT: &str = "●";

struct BundledLogo {
    width: u32,
    height: u32,
    rgba: Vec<u8>,
}

fn bundled_logo() -> Option<&'static BundledLogo> {
    static LOGO: std::sync::OnceLock<Option<BundledLogo>> = std::sync::OnceLock::new();
    LOGO.get_or_init(|| {
        let bytes = include_bytes!("../../../../assets/app_logo.png");
        let decoded = image::load_from_memory(bytes).ok()?;
        let rgba = decoded.to_rgba8();
        Some(BundledLogo {
            width: rgba.width(),
            height: rgba.height(),
            rgba: rgba.into_raw(),
        })
    })
    .as_ref()
}

impl Renderer {
    pub fn update_topbar_content(
        &mut self,
        tabs: &[TopbarTab],
        active_buffer_index: Option<usize>,
        project_name: &str,
        center_x: f32,
        bounds: [f32; 4],
    ) -> Vec<RegionDrawInstance> {
        if bounds[2] < 1.0 || bounds[3] < 1.0 {
            self.topbar_scissor = None;
            self.topbar_glyph_instances.clear();
            self.topbar_icon_instances.clear();
            self.topbar_icon_pipeline.upload_instances(
                &self.device,
                &self.topbar_icon_instances,
                [
                    self.surface_state.config.width,
                    self.surface_state.config.height,
                ],
            );
            self.topbar_chrome_instances.clear();
            self.topbar_text_batches.clear();
            self.last_topbar_layout_key = None;
            self.topbar_text_pipeline
                .upload_instances(&self.device, &self.queue, &[]);
            self.topbar_logo_image_pipeline.clear();
            self.topbar_logo_scissor = None;
            return vec![];
        }

        let layout_key = TopbarLayoutKey {
            tabs: tabs.to_vec(),
            active_buffer_index,
            project_name: project_name.to_string(),
            center_x,
            bounds,
        };
        if self.last_topbar_layout_key.as_ref() == Some(&layout_key) {
            return self.topbar_chrome_instances.clone();
        }

        self.topbar_scissor = rect_to_scissor(bounds);

        let line_h = self.statusbar_line_height;
        let font_size = self.statusbar_font_size;
        let width = (bounds[2] - self.topbar_padding_x * 2.0).max(1.0);
        let content_top = 0.0;
        let content_bottom = 0.0;
        let content_y = bounds[1] + content_top;
        let content_h = (bounds[3] - content_top - content_bottom).max(1.0);
        self.topbar_text_system
            .set_metrics(Metrics::new(font_size, line_h));
        self.topbar_text_system.set_size(None, Some(content_h));
        let origin_y = content_y + ((content_h - line_h) * 0.5).max(0.0);
        let active_fg = self.theme.ui.fg.as_f32();
        let inactive_fg = self.theme.ui.fg_ghost.as_f32();
        let empty_fg = self.theme.ui.fg_ghost.as_f32();
        let dirty_fg = self.theme.ui.warning.as_f32();
        let active_bg = self.theme.editor.bg.as_f32();
        let accent = self.theme.ui.cyan.as_f32();
        let border = self.theme.ui.border_color.as_f32();
        let font_family = self.theme.editor.font_family.as_deref();

        let mut glyphs = Vec::new();
        self.topbar_icon_instances.clear();
        let mut text_batches = Vec::new();
        let mut chrome = Vec::new();

        let item_gap = (font_size * 0.95).max(8.0);
        let logo_size = (bounds[3] - 8.0).max(12.0);
        let logo_x = bounds[0] + bounds[2] - logo_size - 12.0;
        let logo_y = bounds[1] + (bounds[3] - logo_size) * 0.5;

        // Reduce available right by the logo space to avoid tabs rendering over the logo
        let available_right = logo_x - item_gap;
        let dirty_gap = self.topbar_dirty_gap;
        let topbar_start_x = if cfg!(target_os = "macos") {
            TOPBAR_TRAFFIC_LIGHT_SPACE_MACOS.min(bounds[2])
        } else {
            0.0
        };
        let mut tab_x = bounds[0] + topbar_start_x;

        // Render project name (current folder) after traffic light space and before tabs
        let project = project_name.trim();
        if !project.is_empty() && center_x > topbar_start_x {
            let start = glyphs.len() as u32;

            let left_pad = 16.0;
            let icon_text_gap = 12.0;
            let text_sep_gap = 16.0;
            let sep_tab_gap = 16.0;

            // let sep = "│";
            let sep = "";
            let sep_w = estimate_monospace_width(sep, font_size);
            let folder_icon_size = (content_h * 0.72).min(font_size * 1.2);
            let folder_icon_w = folder_icon_size;

            // Compute maximum width available for the text itself
            let allocated_fixed_w =
                left_pad + folder_icon_w + icon_text_gap + text_sep_gap + sep_w + sep_tab_gap;
            let max_text_w = (center_x - topbar_start_x - allocated_fixed_w).max(0.0);
            let clamped_project = clamp_project_name(project, max_text_w, font_size);

            if !clamped_project.is_empty() {
                let mut draw_x = tab_x + left_pad;

                // 1. Draw root folder icon
                self.topbar_icon_instances.push(IconDrawInstance {
                    icon: "built_in:root_folder",
                    rect: [
                        draw_x,
                        content_y + (content_h - folder_icon_size) * 0.5,
                        folder_icon_size,
                        folder_icon_size,
                    ],
                    tint: [1.0, 1.0, 1.0, 1.0],
                });
                draw_x += folder_icon_w + icon_text_gap;

                // 2. Draw folder text
                let label_w = estimate_monospace_width(&clamped_project, font_size);
                glyphs.extend(layout_panel_text(
                    &clamped_project,
                    &mut self.topbar_text_system,
                    &mut self.atlas,
                    &self.queue,
                    draw_x,
                    origin_y,
                    accent,
                ));
                draw_x += label_w + text_sep_gap;

                // 3. Draw separator
                glyphs.extend(layout_panel_text(
                    sep,
                    &mut self.topbar_text_system,
                    &mut self.atlas,
                    &self.queue,
                    draw_x,
                    origin_y,
                    inactive_fg,
                ));
                draw_x += sep_w;

                let scissor_w = draw_x - tab_x;
                let count = glyphs.len() as u32 - start;
                if count > 0 {
                    if let Some(scissor) = rect_to_scissor([tab_x, content_y, scissor_w, content_h])
                    {
                        text_batches.push(TextScissorBatch {
                            scissor,
                            range: InstanceDrawRange { start, count },
                        });
                    }
                }
            }
        }

        // Align tab start exactly at the start of the main editor (center_x)
        tab_x = bounds[0] + center_x.max(topbar_start_x);

        // Render Logo at the far right
        self.topbar_logo_image_pipeline.clear();
        self.topbar_logo_scissor = None;
        if let Some(logo) = bundled_logo() {
            let rect = [logo_x, logo_y, logo_size, logo_size];
            self.topbar_logo_scissor = rect_to_scissor(bounds);
            self.topbar_logo_image_pipeline.upload_rgba(
                &self.device,
                &self.queue,
                &logo.rgba,
                logo.width,
                logo.height,
                rect,
                [
                    self.surface_state.config.width,
                    self.surface_state.config.height,
                ],
            );
        }

        if tabs.is_empty() {
            let start = glyphs.len() as u32;
            glyphs.extend(layout_panel_text(
                "[ no file ]",
                &mut self.topbar_text_system,
                &mut self.atlas,
                &self.queue,
                tab_x + TOPBAR_TAB_PADDING_X,
                origin_y,
                empty_fg,
            ));
            let count = glyphs.len() as u32 - start;
            let empty_width = (available_right - tab_x).max(0.0);
            if let Some(scissor) =
                topbar_tab_text_scissor([tab_x, content_y, empty_width, content_h])
            {
                if count > 0 {
                    text_batches.push(TextScissorBatch {
                        scissor,
                        range: InstanceDrawRange { start, count },
                    });
                }
            }
        } else {
            for (idx, tab) in tabs.iter().enumerate() {
                let is_active = active_buffer_index == Some(idx);
                let label_width = estimate_monospace_width(&tab.label, font_size);
                let dirty_marker_width = if tab.is_dirty {
                    estimate_monospace_width(TOPBAR_DIRTY_DOT, font_size)
                } else {
                    0.0
                };
                let dirty_extra_width = if tab.is_dirty {
                    dirty_gap + dirty_marker_width
                } else {
                    0.0
                };
                let content_width = label_width + dirty_extra_width;
                let tab_min_w = bounds[2] * 0.10;
                let tab_max_w = bounds[2] * 0.15;
                let tab_width = (TOPBAR_TAB_PADDING_X * 2.0 + content_width)
                    .clamp(tab_min_w, tab_max_w)
                    .min(width);
                let separator_width = if idx < tabs.len() - 1 {
                    TOPBAR_TAB_SEPARATOR_WIDTH
                } else {
                    0.0
                };
                if tab_x + tab_width + separator_width > available_right {
                    break;
                }

                if is_active {
                    chrome.push(RegionDrawInstance::new(
                        [tab_x, content_y, tab_width, content_h.max(0.0)],
                        active_bg,
                    ));
                    chrome.push(RegionDrawInstance::new(
                        [
                            tab_x,
                            (content_y + content_h - TOPBAR_ACTIVE_BORDER_HEIGHT).max(content_y),
                            tab_width,
                            TOPBAR_ACTIVE_BORDER_HEIGHT.min(content_h.max(0.0)),
                        ],
                        accent,
                    ));
                }

                if idx < tabs.len() - 1 {
                    chrome.push(RegionDrawInstance::new(
                        [
                            tab_x + tab_width,
                            content_y,
                            TOPBAR_TAB_SEPARATOR_WIDTH,
                            content_h.max(0.0),
                        ],
                        border,
                    ));
                }

                let text_x = tab_x + ((tab_width - content_width) / 2.0).max(TOPBAR_TAB_PADDING_X);
                let batch_start = glyphs.len() as u32;
                self.topbar_text_system.set_font_family(font_family);

                let label_color = if tab.missing_on_disk {
                    inactive_fg
                } else {
                    tab.git_color
                        .unwrap_or(if is_active { active_fg } else { inactive_fg })
                };

                if is_active {
                    glyphs.extend(layout_panel_text_bold(
                        &tab.label,
                        &mut self.topbar_text_system,
                        &mut self.atlas,
                        &self.queue,
                        text_x,
                        origin_y,
                        label_color,
                    ));
                } else {
                    glyphs.extend(layout_panel_text(
                        &tab.label,
                        &mut self.topbar_text_system,
                        &mut self.atlas,
                        &self.queue,
                        text_x,
                        origin_y,
                        label_color,
                    ));
                }
                if tab.missing_on_disk {
                    let strike_y = origin_y + font_size * 0.55;
                    chrome.push(RegionDrawInstance::new(
                        [text_x, strike_y, label_width, 1.0],
                        label_color,
                    ));
                }
                if tab.is_dirty {
                    glyphs.extend(layout_panel_text(
                        TOPBAR_DIRTY_DOT,
                        &mut self.topbar_text_system,
                        &mut self.atlas,
                        &self.queue,
                        text_x + label_width + dirty_gap,
                        origin_y,
                        dirty_fg,
                    ));
                }
                let batch_count = glyphs.len() as u32 - batch_start;
                if let Some(scissor) =
                    topbar_tab_text_scissor([tab_x, content_y, tab_width, content_h])
                {
                    if batch_count > 0 {
                        text_batches.push(TextScissorBatch {
                            scissor,
                            range: InstanceDrawRange {
                                start: batch_start,
                                count: batch_count,
                            },
                        });
                    }
                }
                tab_x += tab_width + separator_width;
            }
        }

        self.topbar_icon_pipeline.upload_instances(
            &self.device,
            &self.topbar_icon_instances,
            [
                self.surface_state.config.width,
                self.surface_state.config.height,
            ],
        );
        self.topbar_glyph_instances = glyphs;
        self.topbar_text_pipeline.upload_instances(
            &self.device,
            &self.queue,
            &self.topbar_glyph_instances,
        );
        self.topbar_text_batches = text_batches;
        self.topbar_chrome_instances = chrome;
        self.last_topbar_layout_key = Some(layout_key);
        self.topbar_chrome_instances.clone()
    }
}
