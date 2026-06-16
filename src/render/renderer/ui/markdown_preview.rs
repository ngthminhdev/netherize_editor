//! Right-sidebar markdown preview text rendering.

use super::utils::{
    blend_rgb, clip_styled_span_to_range, word_wrap_with_ranges,
};

use crate::{
    render::{
        glyph_instance::GlyphInstance,
        region_pipeline::RegionDrawInstance,
        renderer::{
            Renderer,
            helpers::{
                layout_panel_rich_text,
                layout_panel_rich_text_with_bytes, layout_panel_text, rect_to_scissor,
            },
        },
    },
    text::text_system::StyledTextSpan,
};

impl Renderer {
    /// Clear markdown preview text — called when right sidebar is hidden.
    pub fn clear_markdown_preview(&mut self) {
        self.markdown_preview_scissor = None;
        self.markdown_preview_image_scissor = None;
        self.markdown_preview_input_scissor = None;
        self.markdown_preview_input_batch = None;
        self.markdown_preview_chrome_instances.clear();
        self.markdown_preview_overlay_chrome_instances.clear();
        self.markdown_preview_overlay_glyph_start = None;
        self.markdown_preview_glyph_instances.clear();
        self.markdown_preview_header_image_pipeline.clear();
        self.markdown_preview_hero_image_pipeline.clear();
        self.markdown_preview_text_pipeline
            .upload_instances(&self.device, &self.queue, &[]);
    }

    /// Render markdown preview content into the right sidebar area.
    /// Reuses the markdown preview text pipeline since they're mutually exclusive tabs.
    /// Render markdown preview content into the right sidebar area.
    /// Reuses the markdown preview text pipeline since they're mutually exclusive tabs.
    pub fn update_markdown_preview_content(
        &mut self,
        bounds: [f32; 4],
        lines: &[crate::app::app_state::MarkdownPreviewLine],
        scroll_y: f32,
        scroll_x: f32,
        inner_padding: f32,
    ) -> (f32, f32) {
        use crate::app::app_state::MarkdownBlockType;
        use cosmic_text::Metrics;

        if bounds[2] < 1.0 || bounds[3] < 1.0 {
            self.clear_markdown_preview();
            return (0.0, 0.0);
        }

        self.markdown_preview_header_image_pipeline.clear();
        self.markdown_preview_hero_image_pipeline.clear();
        self.markdown_preview_image_scissor = None;
        self.markdown_preview_input_scissor = None;
        self.markdown_preview_input_batch = None;
        self.markdown_preview_chrome_instances.clear();

        let clip = [
            bounds[0] + inner_padding,
            bounds[1] + inner_padding,
            (bounds[2] - inner_padding * 2.0).max(1.0),
            (bounds[3] - inner_padding * 2.0).max(1.0),
        ];
        self.markdown_preview_scissor = rect_to_scissor(clip);

        let base_font_size = self.theme.ui.sidebar_font_size;
        let base_line_h = self.theme.ui.sidebar_line_height.max(base_font_size + 4.0);
        let fg = self.theme.ui.fg.as_f32();
        let fg_dim = self.theme.ui.fg_dim.as_f32();
        let fg_muted = blend_rgb(fg, fg_dim, 0.5, 1.0);

        let mut all_glyphs: Vec<GlyphInstance> = Vec::new();
        let mut chrome_instances: Vec<RegionDrawInstance> = Vec::new();

        let code_bg = blend_rgb(
            self.theme.ui.panel_bg.as_f32(),
            self.theme.ui.selection_bg.as_f32(),
            0.35,
            0.92,
        );
        let table_header_bg = blend_rgb(
            self.theme.ui.panel_bg.as_f32(),
            self.theme.ui.selection_bg.as_f32(),
            0.20,
            0.95,
        );
        let table_row_alt_bg = blend_rgb(
            self.theme.ui.panel_bg.as_f32(),
            self.theme.ui.selection_bg.as_f32(),
            0.08,
            0.97,
        );
        let blockquote_bg = blend_rgb(
            self.theme.ui.panel_bg.as_f32(),
            self.theme.ui.selection_bg.as_f32(),
            0.12,
            0.96,
        );
        let code_border = self.theme.ui.border_color.as_f32();
        let code_inset_x = 6.0;
        let code_pad_x = 8.0;
        let estimated_char_w = (base_font_size * 0.58).max(1.0);
        let code_text_x = clip[0] + code_inset_x + code_pad_x;
        let code_max_chars = ((clip[2] - code_inset_x - code_pad_x * 2.0).max(1.0)
            / estimated_char_w)
            .floor()
            .max(8.0) as usize;

        // Step 1: Lay out all wrapped lines and flag code block boundaries
        struct LayoutedLine {
            block_type: MarkdownBlockType,
            text: String,
            spans: Vec<StyledTextSpan>,
            y_offset: f32,
            line_h: f32,
            font_size: f32,
            is_code_block: bool,
            is_code_block_start: bool,
            is_code_block_end: bool,
            code_language: Option<String>,
        }

        let mut layouted_lines: Vec<LayoutedLine> = Vec::new();

        for (line_idx, preview_line) in lines.iter().enumerate() {
            let is_code = matches!(preview_line.block_type, MarkdownBlockType::CodeBlock);
            let is_table = matches!(
                preview_line.block_type,
                MarkdownBlockType::TableHeader | MarkdownBlockType::TableRow
            );
            let line_font_size = preview_line
                .font_size
                .map(|s| s * base_font_size)
                .unwrap_or(base_font_size);
            let line_h = if preview_line.font_size.is_some() {
                (line_font_size * 1.35).max(base_line_h)
            } else {
                base_line_h
            };
            let line_char_w = (line_font_size * 0.58).max(1.0);
            let line_max_chars = if is_code {
                code_max_chars
            } else if is_table {
                usize::MAX
            } else {
                (clip[2] / line_char_w).floor().max(8.0) as usize
            };
            let wrapped_lines = word_wrap_with_ranges(&preview_line.text, line_max_chars);

            let num_wrapped = wrapped_lines.len();
            for (w_idx, (wrapped_text, byte_range)) in wrapped_lines.into_iter().enumerate() {
                let wrapped_spans: Vec<StyledTextSpan> = preview_line
                    .spans
                    .iter()
                    .filter_map(|span| clip_styled_span_to_range(*span, &byte_range))
                    .collect();

                let is_code_block_start = is_code
                    && (line_idx == 0
                        || !matches!(lines[line_idx - 1].block_type, MarkdownBlockType::CodeBlock))
                    && w_idx == 0;
                let is_code_block_end = is_code
                    && (line_idx + 1 == lines.len()
                        || !matches!(lines[line_idx + 1].block_type, MarkdownBlockType::CodeBlock))
                    && w_idx + 1 == num_wrapped;

                layouted_lines.push(LayoutedLine {
                    block_type: preview_line.block_type,
                    text: wrapped_text,
                    spans: wrapped_spans,
                    y_offset: 0.0,
                    line_h,
                    font_size: line_font_size,
                    is_code_block: is_code,
                    is_code_block_start,
                    is_code_block_end,
                    code_language: if w_idx == 0 {
                        preview_line.code_language.clone()
                    } else {
                        None
                    },
                });
            }
        }

        // Step 2: Compute running vertical positions with margin/padding rules
        let spacing_heading_large = 18.0;
        let spacing_heading_small = 12.0;
        let spacing_heading_bottom = 6.0;
        let spacing_paragraph = 6.0;
        let spacing_list_item = 3.0;
        let spacing_code_block_margin = 10.0;
        let spacing_code_block_padding = 6.0;
        let spacing_blockquote_padding = 4.0;
        let spacing_hr = 12.0;

        let mut current_y = 0.0;

        for i in 0..layouted_lines.len() {
            let is_code_block_start = layouted_lines[i].is_code_block_start;
            let is_code_block_end = layouted_lines[i].is_code_block_end;
            let is_code_block = layouted_lines[i].is_code_block;
            let block_type = layouted_lines[i].block_type;
            let this_line_h = layouted_lines[i].line_h;

            if i > 0 {
                let prev_block_type = layouted_lines[i - 1].block_type;
                let prev_is_code = layouted_lines[i - 1].is_code_block;

                if is_code_block_start {
                    current_y += spacing_code_block_margin;
                } else if matches!(block_type, MarkdownBlockType::Heading(_)) {
                    if let MarkdownBlockType::Heading(level) = block_type {
                        if level <= 2 {
                            current_y += spacing_heading_large;
                        } else {
                            current_y += spacing_heading_small;
                        }
                    }
                } else if matches!(block_type, MarkdownBlockType::HorizontalRule) {
                    current_y += spacing_hr;
                } else if !is_code_block && prev_is_code {
                    current_y += spacing_code_block_margin;
                } else if !is_code_block {
                    match prev_block_type {
                        MarkdownBlockType::Heading(_) => current_y += spacing_heading_bottom,
                        MarkdownBlockType::Paragraph => current_y += spacing_paragraph,
                        MarkdownBlockType::ListItem => current_y += spacing_list_item,
                        MarkdownBlockType::BlockQuote => current_y += spacing_paragraph,
                        MarkdownBlockType::HorizontalRule => current_y += spacing_hr,
                        _ => {}
                    }
                }
            }

            if is_code_block_start {
                current_y += spacing_code_block_padding;
            }
            if matches!(block_type, MarkdownBlockType::BlockQuote) && i > 0 {
                let prev = layouted_lines[i - 1].block_type;
                if !matches!(prev, MarkdownBlockType::BlockQuote) {
                    current_y += spacing_blockquote_padding;
                }
            }

            layouted_lines[i].y_offset = current_y;
            current_y += this_line_h;

            if is_code_block_end {
                current_y += spacing_code_block_padding;
            }
            if matches!(block_type, MarkdownBlockType::BlockQuote) {
                let next_is_quote = i + 1 < layouted_lines.len()
                    && matches!(
                        layouted_lines[i + 1].block_type,
                        MarkdownBlockType::BlockQuote
                    );
                if !next_is_quote {
                    current_y += spacing_blockquote_padding;
                }
            }
        }
        let total_doc_height = current_y;

        let max_scroll = (total_doc_height - clip[3]).max(0.0) / base_line_h;
        let clamped_scroll_y = scroll_y.min(max_scroll);
        let scroll_offset_y = clamped_scroll_y * base_line_h;

        // Step 3: Draw chrome backgrounds (code blocks, tables, blockquotes, heading accents, HR)
        let mut current_code_block_start: Option<usize> = None;
        let start_y = clip[1];

        for i in 0..layouted_lines.len() {
            let line = &layouted_lines[i];
            if line.is_code_block_start {
                current_code_block_start = Some(i);
            }
            if line.is_code_block_end {
                if let Some(start_idx) = current_code_block_start.take() {
                    let y_start =
                        layouted_lines[start_idx].y_offset - spacing_code_block_padding;
                    let y_end = line.y_offset + line.line_h + spacing_code_block_padding;

                    let screen_y_start = start_y + y_start - scroll_offset_y;
                    let screen_y_end = start_y + y_end - scroll_offset_y;
                    let height = screen_y_end - screen_y_start;

                    let box_x = clip[0] + code_inset_x;
                    let box_w = (clip[2] - code_inset_x * 2.0).max(1.0);
                    let rect = [box_x, screen_y_start, box_w, height.max(1.0)];

                    chrome_instances.push(
                        RegionDrawInstance::new(rect, code_border)
                            .with_radius(self.panel_corner_radius.min(6.0)),
                    );
                    chrome_instances.push(
                        RegionDrawInstance::new(
                            [
                                rect[0] + 1.0,
                                rect[1] + 1.0,
                                (rect[2] - 2.0).max(1.0),
                                (rect[3] - 2.0).max(1.0),
                            ],
                            code_bg,
                        )
                        .with_radius((self.panel_corner_radius.min(6.0) - 1.0).max(0.0)),
                    );

                    // Language label badge
                    if let Some(start_line) = layouted_lines.get(start_idx) {
                        if let Some(ref lang) = start_line.code_language {
                            if !lang.is_empty() {
                                let badge_padding = 4.0;
                                let badge_font_size = base_font_size * 0.75;
                                let badge_char_w = (badge_font_size * 0.58).max(1.0);
                                let badge_w =
                                    (lang.len() as f32 * badge_char_w + badge_padding * 2.0)
                                        .min(box_w - 8.0);
                                let badge_h = badge_font_size + badge_padding * 2.0;
                                let badge_x = box_x + box_w - badge_w - 6.0;
                                let badge_y = screen_y_start + 4.0;
                                let badge_color = blend_rgb(
                                    self.theme.ui.panel_bg.as_f32(),
                                    self.theme.ui.selection_bg.as_f32(),
                                    0.55,
                                    0.88,
                                );
                                chrome_instances.push(
                                    RegionDrawInstance::new(
                                        [badge_x, badge_y, badge_w, badge_h],
                                        badge_color,
                                    )
                                    .with_radius(3.0),
                                );
                                let saved_metrics = self.markdown_preview_text_system.buffer_metrics();
                                self.markdown_preview_text_system.set_metrics(Metrics::new(
                                    badge_font_size,
                                    badge_font_size + 2.0,
                                ));
                                all_glyphs.extend(layout_panel_text(
                                    lang,
                                    &mut self.markdown_preview_text_system,
                                    &mut self.atlas,
                                    &self.queue,
                                    badge_x + badge_padding,
                                    badge_y + badge_padding,
                                    fg_muted,
                                ));
                                self.markdown_preview_text_system.set_metrics(saved_metrics);
                            }
                        }
                    }
                }
            }

            // Table header background
            if matches!(line.block_type, MarkdownBlockType::TableHeader) {
                let y = start_y + line.y_offset - scroll_offset_y;
                let rect = [
                    clip[0] + code_inset_x,
                    y,
                    (clip[2] - code_inset_x * 2.0).max(1.0),
                    line.line_h,
                ];
                chrome_instances.push(
                    RegionDrawInstance::new(rect, table_header_bg)
                        .with_radius(self.panel_corner_radius.min(4.0)),
                );
            }

            // Table alternating row background
            if matches!(line.block_type, MarkdownBlockType::TableRow) {
                let row_idx = layouted_lines[..i]
                    .iter()
                    .filter(|l| {
                        matches!(
                            l.block_type,
                            MarkdownBlockType::TableHeader | MarkdownBlockType::TableRow
                        )
                    })
                    .count();
                if row_idx % 2 == 0 {
                    let y = start_y + line.y_offset - scroll_offset_y;
                    let rect = [
                        clip[0] + code_inset_x,
                        y,
                        (clip[2] - code_inset_x * 2.0).max(1.0),
                        line.line_h,
                    ];
                    chrome_instances.push(
                        RegionDrawInstance::new(rect, table_row_alt_bg)
                            .with_radius(self.panel_corner_radius.min(3.0)),
                    );
                }
            }

            // Blockquote background + left accent bar
            if matches!(line.block_type, MarkdownBlockType::BlockQuote) {
                let y = start_y + line.y_offset - scroll_offset_y;
                let accent_w = 3.0;
                let accent_color = blend_rgb(
                    self.theme.syntax.constant.as_f32(),
                    self.theme.ui.panel_bg.as_f32(),
                    0.6,
                    0.9,
                );
                chrome_instances.push(RegionDrawInstance::new(
                    [clip[0], y, accent_w, line.line_h],
                    accent_color,
                ));
                chrome_instances.push(RegionDrawInstance::new(
                    [
                        clip[0] + accent_w,
                        y,
                        (clip[2] - accent_w).max(1.0),
                        line.line_h,
                    ],
                    blockquote_bg,
                ));
            }

            // Heading left accent bar
            if let MarkdownBlockType::Heading(level) = line.block_type {
                let y = start_y + line.y_offset - scroll_offset_y;
                let accent_w = 3.0;
                let accent_color = match level {
                    1 => self.theme.syntax.keyword.as_f32(),
                    2 => self.theme.syntax.function.as_f32(),
                    3 => self.theme.syntax.r#type.as_f32(),
                    _ => self.theme.syntax.constant.as_f32(),
                };
                chrome_instances.push(RegionDrawInstance::new(
                    [clip[0], y, accent_w, line.line_h],
                    accent_color,
                ));
            }

            // Horizontal rule - thin centered line
            if matches!(line.block_type, MarkdownBlockType::HorizontalRule) {
                let y = start_y + line.y_offset - scroll_offset_y + line.line_h * 0.5;
                let hr_color = blend_rgb(
                    self.theme.ui.border_color.as_f32(),
                    self.theme.ui.panel_bg.as_f32(),
                    0.5,
                    0.9,
                );
                chrome_instances.push(RegionDrawInstance::new(
                    [clip[0] + 8.0, y, clip[2] - 16.0, 1.0],
                    hr_color,
                ));
            }
        }

        // Step 4: Compute max horizontal scroll for tables
        let mut max_table_width: f32 = 0.0;
        let table_char_w = (base_font_size * 0.58).max(1.0);
        for line in &layouted_lines {
            if matches!(
                line.block_type,
                MarkdownBlockType::TableHeader | MarkdownBlockType::TableRow
            ) {
                let line_w = line.text.len() as f32 * table_char_w + code_inset_x * 2.0 + 8.0;
                if line_w > max_table_width {
                    max_table_width = line_w;
                }
            }
        }
        let max_scroll_x = (max_table_width - clip[2]).max(0.0);
        let clamped_scroll_x = scroll_x.min(max_scroll_x);
        let scroll_offset_x = if max_table_width > clip[2] {
            clamped_scroll_x
        } else {
            0.0
        };

        // Step 5: Draw all text lines
        let visible_bottom = clip[1] + clip[3];

        for line in &layouted_lines {
            let y = start_y + line.y_offset - scroll_offset_y;

            if y + line.line_h < clip[1] {
                continue;
            }
            if y > visible_bottom {
                break;
            }

            if line.text.is_empty() {
                continue;
            }

            // Skip rendering text for horizontal rules (drawn as chrome line above)
            if matches!(line.block_type, MarkdownBlockType::HorizontalRule) {
                continue;
            }

            let is_heading = matches!(line.block_type, MarkdownBlockType::Heading(_));
            let is_table_line = matches!(
                line.block_type,
                MarkdownBlockType::TableHeader | MarkdownBlockType::TableRow
            );
            let heading_offset_x: f32 = if is_heading { 6.0 } else { 0.0 };
            let table_offset_x: f32 = if is_table_line { -scroll_offset_x } else { 0.0 };

            let text_x = if line.is_code_block {
                code_text_x
            } else {
                clip[0] + heading_offset_x + table_offset_x
            };

            let default_color = match line.block_type {
                MarkdownBlockType::Heading(_) => fg,
                MarkdownBlockType::CodeBlock => fg_dim,
                MarkdownBlockType::BlockQuote => fg_dim,
                _ => fg,
            };

            // Set font metrics for headings
            let saved_metrics = if is_heading {
                let m = self.markdown_preview_text_system.buffer_metrics();
                self.markdown_preview_text_system
                    .set_metrics(Metrics::new(line.font_size, line.line_h));
                Some(m)
            } else {
                None
            };

            if line.spans.is_empty() {
                all_glyphs.extend(layout_panel_text(
                    &line.text,
                    &mut self.markdown_preview_text_system,
                    &mut self.atlas,
                    &self.queue,
                    text_x,
                    y,
                    default_color,
                ));
            } else {
                let has_decorations = line
                    .spans
                    .iter()
                    .any(|s| s.underline || s.strikethrough);
                if has_decorations {
                    let (glyphs, byte_ranges) = layout_panel_rich_text_with_bytes(
                        &line.text,
                        &line.spans,
                        default_color,
                        &mut self.markdown_preview_text_system,
                        &mut self.atlas,
                        &self.queue,
                        text_x,
                        y,
                    );
                    all_glyphs.extend(glyphs);

                    // Draw underline rectangles for underlined spans
                    let glyphs_start = all_glyphs.len().saturating_sub(byte_ranges.len());
                    for span in &line.spans {
                        if span.underline {
                            for (g_idx, &(g_start, g_end)) in byte_ranges.iter().enumerate() {
                                if g_start < span.end && g_end > span.start {
                                    if let Some(glyph) = all_glyphs.get(glyphs_start + g_idx) {
                                        let ul_y = glyph.screen_pos[1] + glyph.glyph_size[1] + 1.0;
                                        chrome_instances.push(RegionDrawInstance::new(
                                            [glyph.screen_pos[0], ul_y, glyph.glyph_size[0], 1.0],
                                            glyph.color,
                                        ));
                                    }
                                }
                            }
                        }
                    }
                } else {
                    all_glyphs.extend(layout_panel_rich_text(
                        &line.text,
                        &line.spans,
                        default_color,
                        &mut self.markdown_preview_text_system,
                        &mut self.atlas,
                        &self.queue,
                        text_x,
                        y,
                    ));
                }
            }

            // Restore base metrics
            if let Some(m) = saved_metrics {
                self.markdown_preview_text_system.set_metrics(m);
            }
        }

        self.markdown_preview_chrome_instances = chrome_instances;
        self.markdown_preview_glyph_instances = all_glyphs;
        self.markdown_preview_text_pipeline.upload_instances(
            &self.device,
            &self.queue,
            &self.markdown_preview_glyph_instances,
        );

        (max_scroll, max_scroll_x)
    }
}
