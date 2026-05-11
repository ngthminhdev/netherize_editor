use std::sync::OnceLock;

use crate::render::{
    region_pipeline::RegionDrawInstance,
    renderer::{Renderer, TextScissorBatch, TopbarLayoutKey, TopbarTab},
    text_pipeline::InstanceDrawRange,
};

use super::super::helpers::{
    estimate_monospace_width, layout_panel_text, layout_panel_text_bold, rect_to_scissor,
};

struct AppLogo {
    width: u32,
    height: u32,
    rgba: Vec<u8>,
}

fn bundled_app_logo() -> Option<&'static AppLogo> {
    static LOGO: OnceLock<Option<AppLogo>> = OnceLock::new();
    LOGO.get_or_init(|| {
        let bytes = include_bytes!("../../../../assets/app_logo.png");
        let decoded = image::load_from_memory(bytes).ok()?;
        let rgba = decoded.to_rgba8();
        let (width, height) = (rgba.width(), rgba.height());
        Some(AppLogo { width, height, rgba: rgba.into_raw() })
    })
    .as_ref()
}

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

fn with_alpha(mut color: [f32; 4], alpha: f32) -> [f32; 4] {
    color[3] = alpha;
    color
}

const TOPBAR_PADDING_TOP: f32 = 0.0;
const TOPBAR_PADDING_BOTTOM: f32 = 0.0;
const TOPBAR_LOGO_AREA_WIDTH: f32 = 44.0;
const TOPBAR_TAB_PADDING_X: f32 = 14.0;
const TOPBAR_TAB_SEPARATOR_WIDTH: f32 = 1.0;
const TOPBAR_ACTIVE_BORDER_HEIGHT: f32 = 2.0;
const TOPBAR_DIRTY_DOT: &str = "●";

impl Renderer {
    pub fn update_topbar_content(
        &mut self,
        tabs: &[TopbarTab],
        active_buffer_index: Option<usize>,
        bounds: [f32; 4],
    ) -> Vec<RegionDrawInstance> {
        if bounds[2] < 1.0 || bounds[3] < 1.0 {
            self.topbar_scissor = None;
            self.topbar_glyph_instances.clear();
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
            bounds,
        };
        if self.last_topbar_layout_key.as_ref() == Some(&layout_key) {
            return self.topbar_chrome_instances.clone();
        }

        self.topbar_scissor = rect_to_scissor(bounds);

        let line_h = self.statusbar_line_height;
        let font_size = self.statusbar_font_size;
        let width = (bounds[2] - self.topbar_padding_x * 2.0).max(1.0);
        let content_top = TOPBAR_PADDING_TOP.min((bounds[3] * 0.3).max(0.0));
        let content_bottom = TOPBAR_PADDING_BOTTOM.min((bounds[3] * 0.45).max(0.0));
        let content_y = bounds[1] + content_top;
        let content_h = (bounds[3] - content_top - content_bottom).max(1.0);
        self.topbar_text_system.set_size(None, Some(content_h));
        let origin_y = content_y + ((content_h - line_h) * 0.5).max(0.0);
        let active_fg = self.theme.ui.fg.as_f32();
        let inactive_fg = self.theme.ui.fg_ghost.as_f32();
        let empty_fg = self.theme.ui.fg_ghost.as_f32();
        let dirty_fg = self.theme.ui.warning.as_f32();
        let active_bg = self.theme.editor.bg.as_f32();
        let accent = self.theme.ui.cyan.as_f32();
        let hover_bg = with_alpha(self.theme.ui.selection_bg.as_f32(), 0.4);
        let border = self.theme.ui.border_color.as_f32();
        let font_family = self.theme.editor.font_family.as_deref();

        let mut glyphs = Vec::new();
        let mut text_batches = Vec::new();
        let mut chrome = Vec::new();
        let logo_bounds = [
            bounds[0],
            bounds[1],
            TOPBAR_LOGO_AREA_WIDTH.min(bounds[2]),
            bounds[3],
        ];
        let mut tab_x = bounds[0] + logo_bounds[2];
        let available_right = bounds[0] + bounds[2];
        let dirty_gap = self.topbar_dirty_gap;

        // Separator between blank logo-area gap and the logo tile / tabs.
        chrome.push(RegionDrawInstance::new(
            [
                logo_bounds[0] + logo_bounds[2] - TOPBAR_TAB_SEPARATOR_WIDTH,
                logo_bounds[1],
                TOPBAR_TAB_SEPARATOR_WIDTH.min(logo_bounds[2]),
                logo_bounds[3],
            ],
            border,
        ));

        // Logo tile: full-height square placed right at tab_x.
        let logo_tile_size = bounds[3];
        if let Some(logo) = bundled_app_logo() {
            self.topbar_logo_image_pipeline.upload_rgba(
                &self.device,
                &self.queue,
                &logo.rgba,
                logo.width,
                logo.height,
                [tab_x, bounds[1], logo_tile_size, logo_tile_size],
                [
                    self.surface_state.config.width,
                    self.surface_state.config.height,
                ],
            );
            self.topbar_logo_scissor =
                rect_to_scissor([tab_x, bounds[1], logo_tile_size, bounds[3]]);
        } else {
            self.topbar_logo_image_pipeline.clear();
            self.topbar_logo_scissor = None;
        }

        // Separator after logo tile, before first file tab.
        chrome.push(RegionDrawInstance::new(
            [
                tab_x + logo_tile_size,
                bounds[1],
                TOPBAR_TAB_SEPARATOR_WIDTH,
                bounds[3],
            ],
            border,
        ));

        tab_x += logo_tile_size + TOPBAR_TAB_SEPARATOR_WIDTH;

        let _ = hover_bg;

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
            if let Some(scissor) = topbar_tab_text_scissor([tab_x, content_y, width, content_h]) {
                if count > 0 {
                    text_batches.push(TextScissorBatch {
                        scissor,
                        range: InstanceDrawRange { start, count },
                    });
                }
            }
        } else {
            for (idx, tab) in tabs.iter().enumerate() {
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

                let is_active = active_buffer_index == Some(idx);
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

                let label_color = tab
                    .git_color
                    .unwrap_or(if is_active { active_fg } else { inactive_fg });

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
