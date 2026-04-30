use crate::render::{
    region_pipeline::RegionDrawInstance,
    renderer::{Renderer, TextScissorBatch, TopbarLayoutKey, TopbarTab, TopbarTabKind},
    text_pipeline::InstanceDrawRange,
};

use super::super::helpers::{estimate_monospace_width, layout_panel_text, rect_to_scissor};

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
const TOPBAR_PADDING_BOTTOM: f32 = 6.0;

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
        let inactive_fg = with_alpha(self.theme.ui.fg_dim.as_f32(), 0.95);
        let empty_fg = self.theme.ui.fg_ghost.as_f32();
        let dirty_fg = self.theme.ui.dirty_indicator.as_f32();
        let active_bg = with_alpha(self.theme.ui.selection_bg.as_f32(), 0.78);
        let accent = self.theme.ui.accent.as_f32();
        let top_bg = with_alpha(self.theme.ui.panel_bg.as_f32(), 0.92);
        let border = with_alpha(self.theme.ui.border_color.as_f32(), 0.9);
        let font_family = self.theme.editor.font_family.as_deref();
        let nerd_family = self
            .theme
            .editor
            .nerd_font_family
            .as_deref()
            .filter(|family| !family.is_empty())
            .or(font_family);

        let mut glyphs = Vec::new();
        let mut text_batches = Vec::new();
        let mut chrome = Vec::new();
        let mut tab_x = bounds[0] + self.topbar_padding_x;
        let tab_gap = 4.0;
        let available_right = bounds[0] + bounds[2] - self.topbar_padding_x;
        let tab_pad_x = 10.0;
        let icon_gap = 4.0;
        let dirty_marker = "•";
        let dirty_gap = self.topbar_dirty_gap;

        chrome.push(RegionDrawInstance::new(bounds, top_bg));
        chrome.push(RegionDrawInstance::new(
            [
                bounds[0],
                (bounds[1] + bounds[3] - 1.0).max(bounds[1]),
                bounds[2],
                1.0_f32.min(bounds[3]),
            ],
            border,
        ));

        if tabs.is_empty() {
            let start = glyphs.len() as u32;
            glyphs.extend(layout_panel_text(
                "[ no file ]",
                &mut self.topbar_text_system,
                &mut self.atlas,
                &self.queue,
                tab_x,
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
                let icon_glyph = match &tab.kind {
                    TopbarTabKind::Text { path } => self
                        .theme
                        .file_icon_for_path(path, false, false)
                        .glyph
                        .clone(),
                    TopbarTabKind::Image { path } => self
                        .theme
                        .file_icon_for_path(path, false, false)
                        .glyph
                        .clone(),
                    TopbarTabKind::Terminal => {
                        self.theme.file_icon_for_extension("sh").glyph.clone()
                    }
                    TopbarTabKind::References => {
                        self.theme.file_icon_for_extension("txt").glyph.clone()
                    }
                    TopbarTabKind::Diagnostics => {
                        self.theme.file_icon_for_extension("log").glyph.clone()
                    }
                    TopbarTabKind::FuzzyPicker => {
                        self.theme.file_icon_for_extension("fzf").glyph.clone()
                    }
                    TopbarTabKind::Settings => "⚙".to_string(),
                    TopbarTabKind::Help => "?".to_string(),
                };
                let icon_color = match &tab.kind {
                    TopbarTabKind::Text { path } => self
                        .theme
                        .icon_theme_for_path(path, false, false)
                        .color
                        .as_f32(),
                    TopbarTabKind::Image { path } => self
                        .theme
                        .icon_theme_for_path(path, false, false)
                        .color
                        .as_f32(),
                    TopbarTabKind::Terminal => self.theme.icons.shell.color.as_f32(),
                    TopbarTabKind::References => self.theme.icons.default_file.color.as_f32(),
                    TopbarTabKind::Diagnostics => self.theme.icons.default_file.color.as_f32(),
                    TopbarTabKind::FuzzyPicker => self.theme.ui.accent.as_f32(),
                    TopbarTabKind::Settings => self.theme.ui.accent.as_f32(),
                    TopbarTabKind::Help => self.theme.ui.accent.as_f32(),
                };
                let icon_text = format!("{} ", icon_glyph);
                let icon_width = estimate_monospace_width(&icon_text, font_size);
                let label_width = estimate_monospace_width(&tab.label, font_size);
                let dirty_marker_width = if tab.is_dirty {
                    estimate_monospace_width(dirty_marker, font_size)
                } else {
                    0.0
                };
                let dirty_extra_width = if tab.is_dirty {
                    dirty_gap + dirty_marker_width
                } else {
                    0.0
                };
                let tab_width =
                    (tab_pad_x * 2.0 + icon_width + icon_gap + label_width + dirty_extra_width)
                        .min(width);
                if tab_x + tab_width > available_right {
                    break;
                }

                let is_active = active_buffer_index == Some(idx);
                if is_active {
                    chrome.push(
                        RegionDrawInstance::new(
                            [tab_x, content_y, tab_width, content_h.max(0.0)],
                            active_bg,
                        )
                        .with_radius(7.0),
                    );
                    chrome.push(RegionDrawInstance::new(
                        [
                            tab_x + 8.0,
                            (content_y + content_h - 2.0).max(content_y),
                            (tab_width - 16.0).max(0.0),
                            1.0,
                        ],
                        accent,
                    ));
                }

                if idx < tabs.len() - 1 {
                    let mut sep_color = self.theme.ui.fg_ghost.as_f32();
                    sep_color[3] = 0.8;
                    chrome.push(RegionDrawInstance::new(
                        [
                            tab_x + tab_width + (tab_gap * 0.5_f32).floor(),
                            content_y + 8.0,
                            1.0,
                            (content_h - 16.0).max(0.0),
                        ],
                        with_alpha(sep_color, 0.55),
                    ));
                }

                let icon_x = tab_x + tab_pad_x;
                let batch_start = glyphs.len() as u32;
                self.topbar_text_system.set_font_family(nerd_family);
                glyphs.extend(layout_panel_text(
                    &icon_text,
                    &mut self.topbar_text_system,
                    &mut self.atlas,
                    &self.queue,
                    icon_x,
                    origin_y,
                    icon_color,
                ));
                self.topbar_text_system.set_font_family(font_family);
                glyphs.extend(layout_panel_text(
                    &tab.label,
                    &mut self.topbar_text_system,
                    &mut self.atlas,
                    &self.queue,
                    icon_x + icon_width + icon_gap,
                    origin_y,
                    if is_active { active_fg } else { inactive_fg },
                ));
                if tab.is_dirty {
                    glyphs.extend(layout_panel_text(
                        dirty_marker,
                        &mut self.topbar_text_system,
                        &mut self.atlas,
                        &self.queue,
                        icon_x + icon_width + icon_gap + label_width + dirty_gap,
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
                tab_x += tab_width + tab_gap;
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
