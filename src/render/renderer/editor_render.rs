//! Editor viewport rendering: content text, caret, gutter, visual selection,
//! current-line highlight.

use cosmic_text::Metrics;

use crate::{
    app::app_state::{
        AppState, DiagnosticsState, EditorOverlay, FloatingBoxBlock, FloatingBoxStyle,
        OverlayColorToken,
        ReferencesBufferState,
    },
    core::mode::EditorMode,
    render::{
        glyph_instance::GlyphInstance, region_pipeline::RegionDrawInstance, renderer::Renderer,
    },
    text::layout_sync::{compute_caret_layout, compute_cursor_overlay, rebuild_layout_projection},
};

use super::helpers::{
    caret_rect_for_mode, clamp_monospace_text, estimate_monospace_width, gutter_width_for_editor,
    layout_panel_rich_text, layout_panel_text, layout_panel_text_italic, rect_to_scissor,
    should_draw_block_cursor,
};
use crate::text::text_system::StyledTextSpan;

fn run_x_for_byte(text_area_x: f32, run: &cosmic_text::LayoutRun, byte_in_line: usize) -> f32 {
    if run.glyphs.is_empty() {
        return text_area_x;
    }

    let mut x = text_area_x + run.line_w;
    for glyph in run.glyphs {
        let left = text_area_x + glyph.x;
        let right = left + glyph.w;
        if byte_in_line <= glyph.start {
            return left;
        }
        if byte_in_line < glyph.end {
            return left;
        }
        x = right;
    }
    x
}

struct EditorViewportGeometry {
    line_height: f32,
    font_size: f32,
    gutter_width: f32,
    viewport_text_left: f32,
    viewport_text_width: f32,
    origin_x: f32,
    origin_y: f32,
}

fn editor_viewport_geometry(
    renderer: &Renderer,
    app_state: &AppState,
    center_bounds: [f32; 4],
) -> EditorViewportGeometry {
    let line_height = renderer.theme.editor.line_height;
    let font_size = renderer.theme.editor.font_size;
    let total_lines = app_state.total_lines().max(1);
    let gutter_digits = total_lines.to_string().len().max(3);
    let gutter_width = gutter_width_for_editor(gutter_digits, font_size, line_height);
    let scroll_y = app_state.scroll_line as f32 * line_height;
    let scroll_x = app_state.scroll_column as f32 * (font_size * 0.6).max(1.0);
    let viewport_text_left = center_bounds[0] + renderer.editor_padding_x + gutter_width;
    let origin_x = viewport_text_left - scroll_x;
    let origin_y = center_bounds[1] + renderer.editor_padding_y + line_height - scroll_y;
    let viewport_text_width =
        (center_bounds[2] - renderer.editor_padding_x - gutter_width).max(1.0);

    EditorViewportGeometry {
        line_height,
        font_size,
        gutter_width,
        viewport_text_left,
        viewport_text_width,
        origin_x,
        origin_y,
    }
}

impl Renderer {
    pub fn clear_editor_content(&mut self) {
        self.glyph_instances.clear();
        self.text_pipeline
            .upload_instances(&self.device, &self.queue, &[]);
        self.caret_pipeline.upload_caret(&self.queue, None);
        self.editor_cursor_overlay_pipeline
            .upload_instances(&self.device, &self.queue, &[]);
        self.gutter_glyph_instances.clear();
        self.gutter_text_pipeline
            .upload_instances(&self.device, &self.queue, &[]);
        self.clear_editor_overlays();
        self.editor_scissor = None;
    }

    /// Rebuild glyph instances and caret for the center editor region.
    pub fn update_editor_content(
        &mut self,
        text: &str,
        app_state: &AppState,
        center_bounds: [f32; 4],
        spans: &[StyledTextSpan],
    ) {
        let geometry = editor_viewport_geometry(self, app_state, center_bounds);
        let width = geometry.viewport_text_width;

        self.editor_scissor = rect_to_scissor(center_bounds);
        // Allow cosmic-text to shape full height; scissor clips the visible region.
        self.text_system.set_size(Some(width), None);

        let result = rebuild_layout_projection(
            text,
            app_state,
            &mut self.text_system,
            &mut self.atlas,
            &self.queue,
            [geometry.origin_x, geometry.origin_y],
            self.theme.editor.fg.as_f32(),
            self.theme.editor.bg.as_f32(),
            spans,
        );

        match result {
            Ok(projection) => {
                self.glyph_instances = projection.glyph_instances;
                let caret = caret_rect_for_mode(
                    projection.caret_layout,
                    app_state.current_mode(),
                    self.theme.editor.cursor.as_f32(),
                    self.theme.editor.font_size,
                    self.cursor_shape,
                    self.cursor_beam_width,
                    self.cursor_block_width,
                    self.cursor_underline_height,
                );
                self.caret_pipeline.upload_caret(&self.queue, Some(caret));
                let overlay_instances: Vec<GlyphInstance> = projection
                    .cursor_overlay
                    .filter(|_| {
                        should_draw_block_cursor(app_state.current_mode(), self.cursor_shape)
                    })
                    .into_iter()
                    .collect();
                self.editor_cursor_overlay_pipeline.upload_instances(
                    &self.device,
                    &self.queue,
                    &overlay_instances,
                );
            }
            Err(e) => {
                eprintln!("[Renderer] text layout: {e}");
                self.glyph_instances.clear();
                self.caret_pipeline.upload_caret(&self.queue, None);
                self.editor_cursor_overlay_pipeline.upload_instances(
                    &self.device,
                    &self.queue,
                    &[],
                );
            }
        }

        self.text_pipeline
            .upload_instances(&self.device, &self.queue, &self.glyph_instances);
        self.update_editor_gutter(
            app_state,
            center_bounds,
            geometry.line_height,
            geometry.font_size,
            app_state.total_lines().max(1).to_string().len().max(3),
            geometry.gutter_width,
        );
    }

    /// Fast path for cursor movement: reuse existing layout and update caret only.
    ///
    /// Must honor the same mode → shape mapping as `update_editor_content`, otherwise
    /// h/j/k/l in Normal mode would collapse the block caret back to a thin bar.
    pub fn update_editor_caret(&mut self, app_state: &AppState, center_bounds: [f32; 4]) {
        let geometry = editor_viewport_geometry(self, app_state, center_bounds);

        let caret_layout = compute_caret_layout(
            &self.text_system,
            app_state,
            [geometry.origin_x, geometry.origin_y],
        );
        let caret = caret_rect_for_mode(
            caret_layout,
            app_state.current_mode(),
            self.theme.editor.cursor.as_f32(),
            self.theme.editor.font_size,
            self.cursor_shape,
            self.cursor_beam_width,
            self.cursor_block_width,
            self.cursor_underline_height,
        );
        self.caret_pipeline.upload_caret(&self.queue, Some(caret));

        let overlay = if should_draw_block_cursor(app_state.current_mode(), self.cursor_shape) {
            compute_cursor_overlay(
                &mut self.text_system,
                app_state,
                &mut self.atlas,
                &self.queue,
                [geometry.origin_x, geometry.origin_y],
                self.theme.editor.bg.as_f32(),
            )
            .unwrap_or(None)
        } else {
            None
        };
        let overlay_instances: Vec<GlyphInstance> = overlay.into_iter().collect();
        self.editor_cursor_overlay_pipeline.upload_instances(
            &self.device,
            &self.queue,
            &overlay_instances,
        );

        self.update_editor_gutter(
            app_state,
            center_bounds,
            geometry.line_height,
            geometry.font_size,
            app_state.total_lines().max(1).to_string().len().max(3),
            geometry.gutter_width,
        );
    }

    pub fn clear_editor_overlays(&mut self) {
        self.editor_overlay_scissor = None;
        self.editor_overlay_chrome_instances.clear();
        self.editor_overlay_glyph_instances.clear();
        self.editor_overlay_text_pipeline
            .upload_instances(&self.device, &self.queue, &[]);
    }

    pub fn update_references_buffer_content(
        &mut self,
        references: &ReferencesBufferState,
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

        let pad_x = self.editor_padding_x.max(14.0);
        let pad_y = self.editor_padding_y.max(14.0);
        let panel_x = center_bounds[0] + pad_x;
        let panel_y = center_bounds[1] + pad_y;
        let panel_w = (center_bounds[2] - pad_x * 2.0).max(1.0);
        let panel_h = (center_bounds[3] - pad_y * 2.0).max(1.0);
        let gap = 16.0;
        let left_w = (panel_w * 0.5).max(1.0);
        let right_w = (panel_w - left_w - gap).max(1.0);
        let left_x = panel_x;
        let right_x = left_x + left_w + gap;
        let header_h = line_height + 14.0;
        let footer_h = line_height + 10.0;
        let content_top = panel_y + header_h;
        let content_bottom = (panel_y + panel_h - footer_h).max(content_top + line_height);
        let content_h = (content_bottom - content_top).max(line_height);

        let panel_bg = self.theme.ui.panel_bg.as_f32();
        let editor_bg = self.theme.editor.bg.as_f32();
        let mut divider = self.theme.ui.fg_ghost.as_f32();
        divider[3] = divider[3].clamp(0.28, 0.42);
        let accent = self.theme.ui.accent.as_f32();
        let warning = self.theme.ui.warning.as_f32();
        let fg = self.theme.ui.fg.as_f32();
        let fg_dim = self.theme.ui.fg_dim.as_f32();
        let fg_ghost = self.theme.ui.fg_ghost.as_f32();
        let selection_bg = self.theme.ui.selection_bg.as_f32();

        let mut glyphs = Vec::new();
        let mut chrome = vec![
            RegionDrawInstance::new([left_x, panel_y, left_w, panel_h], panel_bg),
            RegionDrawInstance::new([right_x, panel_y, right_w, panel_h], editor_bg),
            RegionDrawInstance::new([right_x - gap * 0.5, panel_y, 1.0, panel_h], divider),
            RegionDrawInstance::new([left_x, panel_y + header_h - 1.0, left_w, 1.0], divider),
            RegionDrawInstance::new([right_x, panel_y + header_h - 1.0, right_w, 1.0], divider),
        ];

        let header_y = panel_y + 6.0;
        let left_header = if references.loading {
            format!("{}  > loading...", references.title)
        } else {
            references.title.clone()
        };
        self.editor_overlay_text_system
            .set_size(Some((left_w - 20.0).max(1.0)), Some(line_height));
        glyphs.extend(layout_panel_text(
            &clamp_monospace_text(&left_header, (left_w - 20.0).max(1.0), font_size),
            &mut self.editor_overlay_text_system,
            &mut self.atlas,
            &self.queue,
            left_x + 10.0,
            header_y,
            fg,
        ));

        let selected = references.items.get(references.selected_index);
        let right_header = selected
            .map(|item| {
                format!(
                    "{}:{}:{}",
                    item.relative_path,
                    item.line + 1,
                    item.column + 1
                )
            })
            .unwrap_or_else(|| {
                references
                    .status_message
                    .clone()
                    .unwrap_or_else(|| "No reference selected".to_string())
            });
        self.editor_overlay_text_system
            .set_size(Some((right_w - 20.0).max(1.0)), Some(line_height));
        glyphs.extend(layout_panel_text(
            &clamp_monospace_text(&right_header, (right_w - 20.0).max(1.0), font_size),
            &mut self.editor_overlay_text_system,
            &mut self.atlas,
            &self.queue,
            right_x + 10.0,
            header_y,
            fg,
        ));

        let help_text = "J/K / Ctrl+N/P / Up/Down to navigate  |  Enter to open  |  Esc/Q to close";
        self.editor_overlay_text_system
            .set_size(Some((panel_w - 20.0).max(1.0)), Some(line_height));
        glyphs.extend(layout_panel_text(
            &clamp_monospace_text(help_text, (panel_w - 20.0).max(1.0), font_size),
            &mut self.editor_overlay_text_system,
            &mut self.atlas,
            &self.queue,
            panel_x + 10.0,
            panel_y + panel_h - footer_h + 4.0,
            fg_ghost,
        ));

        let left_text_width = (left_w - 20.0).max(1.0);
        if references.items.is_empty() {
            let status = references
                .status_message
                .as_deref()
                .unwrap_or("Loading references...");
            self.editor_overlay_text_system
                .set_size(Some(left_text_width), Some(line_height));
            glyphs.extend(layout_panel_text(
                status,
                &mut self.editor_overlay_text_system,
                &mut self.atlas,
                &self.queue,
                left_x + 14.0,
                content_top + 4.0,
                fg_ghost,
            ));
        } else {
            let row_h = line_height * 2.0 + 8.0;
            let visible_rows = ((content_h / row_h).floor() as usize).max(1);
            let mut start_idx = references.selected_index.saturating_sub(visible_rows / 2);
            if start_idx + visible_rows > references.items.len() {
                start_idx = references.items.len().saturating_sub(visible_rows);
            }

            for (slot, item_idx) in (start_idx..references.items.len())
                .take(visible_rows)
                .enumerate()
            {
                let item = &references.items[item_idx];
                let row_y = content_top + slot as f32 * row_h;
                let is_selected = item_idx == references.selected_index;
                if is_selected {
                    chrome.push(RegionDrawInstance::new(
                        [left_x + 6.0, row_y, (left_w - 12.0).max(1.0), row_h - 4.0],
                        selection_bg,
                    ));
                    chrome.push(RegionDrawInstance::new(
                        [left_x + 6.0, row_y, 3.0, row_h - 4.0],
                        accent,
                    ));
                }

                self.editor_overlay_text_system
                    .set_size(Some(left_text_width), Some(line_height));
                glyphs.extend(layout_panel_text(
                    &clamp_monospace_text(&item.relative_path, left_text_width, font_size),
                    &mut self.editor_overlay_text_system,
                    &mut self.atlas,
                    &self.queue,
                    left_x + 14.0,
                    row_y + 4.0,
                    if is_selected { fg } else { fg_dim },
                ));

                self.editor_overlay_text_system
                    .set_size(Some(left_text_width), Some(line_height));
                glyphs.extend(layout_panel_text(
                    &clamp_monospace_text(&item.summary, left_text_width, font_size),
                    &mut self.editor_overlay_text_system,
                    &mut self.atlas,
                    &self.queue,
                    left_x + 14.0,
                    row_y + line_height + 4.0,
                    if is_selected { fg_dim } else { fg_ghost },
                ));
            }
        }

        if !references.preview_lines.is_empty() {
            let line_number_width = estimate_monospace_width(
                &format!(
                    "{:>4}",
                    references
                        .preview_lines
                        .last()
                        .map(|line| line.line_number)
                        .unwrap_or(1)
                ),
                font_size,
            ) + 14.0;
            let preview_text_width = (right_w - 20.0 - line_number_width).max(1.0);
            let preview_rows = ((content_h / line_height).floor() as usize).max(1);
            let mut preview_start = 0usize;
            if let Some(target_idx) = references
                .preview_lines
                .iter()
                .position(|line| line.is_target)
            {
                preview_start = target_idx.saturating_sub(preview_rows / 2);
                if preview_start + preview_rows > references.preview_lines.len() {
                    preview_start = references.preview_lines.len().saturating_sub(preview_rows);
                }
            }

            for (slot, line) in references
                .preview_lines
                .iter()
                .skip(preview_start)
                .take(preview_rows)
                .enumerate()
            {
                let row_y = content_top + slot as f32 * line_height;
                if line.is_target {
                    chrome.push(RegionDrawInstance::new(
                        [right_x + 6.0, row_y, (right_w - 12.0).max(1.0), line_height],
                        selection_bg,
                    ));
                    chrome.push(RegionDrawInstance::new(
                        [right_x + 6.0, row_y, 3.0, line_height],
                        accent,
                    ));
                }

                self.editor_overlay_text_system
                    .set_size(Some(line_number_width), Some(line_height));
                glyphs.extend(layout_panel_text(
                    &format!("{:>4}", line.line_number),
                    &mut self.editor_overlay_text_system,
                    &mut self.atlas,
                    &self.queue,
                    right_x + 10.0,
                    row_y,
                    if line.is_target { warning } else { fg_ghost },
                ));

                self.editor_overlay_text_system
                    .set_size(Some(preview_text_width), Some(line_height));
                let line_start = references
                    .preview_lines
                    .iter()
                    .take(preview_start + slot)
                    .map(|item| item.text.len() + 1)
                    .sum::<usize>();
                let line_end = line_start + line.text.len();
                let line_spans: Vec<StyledTextSpan> = references
                    .preview_spans
                    .iter()
                    .filter_map(|span| {
                        if span.end <= line_start || span.start >= line_end {
                            return None;
                        }
                        Some(StyledTextSpan::with_style(
                            span.start.max(line_start) - line_start,
                            span.end.min(line_end) - line_start,
                            span.color_rgba,
                            span.bold,
                            span.italic,
                        ))
                    })
                    .collect();
                glyphs.extend(layout_panel_rich_text(
                    &clamp_monospace_text(&line.text, preview_text_width, font_size),
                    &line_spans,
                    if line.is_target { fg } else { fg_dim },
                    &mut self.editor_overlay_text_system,
                    &mut self.atlas,
                    &self.queue,
                    right_x + 10.0 + line_number_width,
                    row_y,
                ));
            }
        } else {
            let empty_message = if references.loading {
                "Loading references..."
            } else if references.items.is_empty() {
                references
                    .status_message
                    .as_deref()
                    .unwrap_or("No references found")
            } else {
                "Loading preview..."
            };
            self.editor_overlay_text_system
                .set_size(Some((right_w - 20.0).max(1.0)), Some(line_height));
            glyphs.extend(layout_panel_text(
                empty_message,
                &mut self.editor_overlay_text_system,
                &mut self.atlas,
                &self.queue,
                right_x + 10.0,
                content_top + 4.0,
                fg_ghost,
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

    pub fn update_diagnostics_buffer_content(
        &mut self,
        diagnostics: &DiagnosticsState,
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

        let pad_x = self.editor_padding_x.max(14.0);
        let pad_y = self.editor_padding_y.max(14.0);
        let panel_x = center_bounds[0] + pad_x;
        let panel_y = center_bounds[1] + pad_y;
        let panel_w = (center_bounds[2] - pad_x * 2.0).max(1.0);
        let panel_h = (center_bounds[3] - pad_y * 2.0).max(1.0);
        let gap = 16.0;
        let left_w = (panel_w * 0.5).max(1.0);
        let right_w = (panel_w - left_w - gap).max(1.0);
        let left_x = panel_x;
        let right_x = left_x + left_w + gap;
        let header_h = line_height + 14.0;
        let footer_h = line_height + 10.0;
        let content_top = panel_y + header_h;
        let content_bottom = (panel_y + panel_h - footer_h).max(content_top + line_height);
        let content_h = (content_bottom - content_top).max(line_height);

        let panel_bg = self.theme.ui.panel_bg.as_f32();
        let editor_bg = self.theme.editor.bg.as_f32();
        let mut divider = self.theme.ui.fg_ghost.as_f32();
        divider[3] = divider[3].clamp(0.28, 0.42);
        let accent = self.theme.ui.accent.as_f32();
        let warning = self.theme.ui.warning.as_f32();
        let error = [0.95, 0.32, 0.32, 1.0];
        let fg = self.theme.ui.fg.as_f32();
        let fg_dim = self.theme.ui.fg_dim.as_f32();
        let fg_ghost = self.theme.ui.fg_ghost.as_f32();
        let selection_bg = self.theme.ui.selection_bg.as_f32();

        let mut glyphs = Vec::new();
        let mut chrome = vec![
            RegionDrawInstance::new([left_x, panel_y, left_w, panel_h], panel_bg),
            RegionDrawInstance::new([right_x, panel_y, right_w, panel_h], editor_bg),
            RegionDrawInstance::new([right_x - gap * 0.5, panel_y, 1.0, panel_h], divider),
            RegionDrawInstance::new([left_x, panel_y + header_h - 1.0, left_w, 1.0], divider),
            RegionDrawInstance::new([right_x, panel_y + header_h - 1.0, right_w, 1.0], divider),
        ];

        let header_y = panel_y + 6.0;
        self.editor_overlay_text_system
            .set_size(Some((left_w - 20.0).max(1.0)), Some(line_height));
        glyphs.extend(layout_panel_text(
            "[Diagnostics]",
            &mut self.editor_overlay_text_system,
            &mut self.atlas,
            &self.queue,
            left_x + 10.0,
            header_y,
            fg,
        ));

        let right_header = diagnostics
            .results
            .get(diagnostics.selected_index)
            .map(|item| format!("{}:{}:{}", item.file_path.display(), item.line + 1, item.col + 1))
            .unwrap_or_else(|| "No diagnostic selected".to_string());
        self.editor_overlay_text_system
            .set_size(Some((right_w - 20.0).max(1.0)), Some(line_height));
        glyphs.extend(layout_panel_text(
            &clamp_monospace_text(&right_header, (right_w - 20.0).max(1.0), font_size),
            &mut self.editor_overlay_text_system,
            &mut self.atlas,
            &self.queue,
            right_x + 10.0,
            header_y,
            fg,
        ));

        let row_h = line_height * 2.0 + 8.0;
        let visible_rows = (content_h / row_h).floor() as usize;
        let start = diagnostics
            .selected_index
            .saturating_sub(visible_rows.saturating_sub(1).min(diagnostics.selected_index));
        let left_text_width = (left_w - 24.0).max(1.0);

        for (slot, (item_idx, item)) in diagnostics
            .results
            .iter()
            .enumerate()
            .skip(start)
            .take(visible_rows.max(1))
            .enumerate()
        {
            let row_y = content_top + slot as f32 * row_h;
            let is_selected = item_idx == diagnostics.selected_index;
            if is_selected {
                chrome.push(RegionDrawInstance::new(
                    [left_x + 6.0, row_y, (left_w - 12.0).max(1.0), row_h - 4.0],
                    selection_bg,
                ));
                chrome.push(RegionDrawInstance::new(
                    [left_x + 6.0, row_y, 3.0, row_h - 4.0],
                    accent,
                ));
            }

            let severity_color = match item.severity {
                Some(1) => error,
                Some(2) => warning,
                _ => fg_dim,
            };
            let title = format!("{}:{}:{}", item.file_path.display(), item.line + 1, item.col + 1);
            self.editor_overlay_text_system
                .set_size(Some(left_text_width), Some(line_height));
            glyphs.extend(layout_panel_text(
                &clamp_monospace_text(&title, left_text_width, font_size),
                &mut self.editor_overlay_text_system,
                &mut self.atlas,
                &self.queue,
                left_x + 14.0,
                row_y + 4.0,
                severity_color,
            ));
            self.editor_overlay_text_system
                .set_size(Some(left_text_width), Some(line_height));
            glyphs.extend(layout_panel_text(
                &clamp_monospace_text(&item.message, left_text_width, font_size),
                &mut self.editor_overlay_text_system,
                &mut self.atlas,
                &self.queue,
                left_x + 14.0,
                row_y + line_height + 4.0,
                if is_selected { fg } else { fg_ghost },
            ));
        }

        if !diagnostics.preview_lines.is_empty() {
            let line_number_width = estimate_monospace_width(
                &format!("{:>4}", diagnostics.preview_lines.last().map(|line| line.line_number).unwrap_or(1)),
                font_size,
            ) + 14.0;
            let preview_text_width = (right_w - 20.0 - line_number_width).max(1.0);
            let preview_rows = ((content_h / line_height).floor() as usize).max(1);
            let mut preview_start = 0usize;
            if let Some(target_idx) = diagnostics.preview_lines.iter().position(|line| line.is_target) {
                preview_start = target_idx.saturating_sub(preview_rows / 2);
                if preview_start + preview_rows > diagnostics.preview_lines.len() {
                    preview_start = diagnostics.preview_lines.len().saturating_sub(preview_rows);
                }
            }

            for (slot, line) in diagnostics
                .preview_lines
                .iter()
                .skip(preview_start)
                .take(preview_rows)
                .enumerate()
            {
                let row_y = content_top + slot as f32 * line_height;
                if line.is_target {
                    chrome.push(RegionDrawInstance::new(
                        [right_x + 6.0, row_y, (right_w - 12.0).max(1.0), line_height],
                        selection_bg,
                    ));
                    chrome.push(RegionDrawInstance::new(
                        [right_x + 6.0, row_y, 3.0, line_height],
                        accent,
                    ));
                }

                self.editor_overlay_text_system
                    .set_size(Some(line_number_width), Some(line_height));
                glyphs.extend(layout_panel_text(
                    &format!("{:>4}", line.line_number),
                    &mut self.editor_overlay_text_system,
                    &mut self.atlas,
                    &self.queue,
                    right_x + 10.0,
                    row_y,
                    if line.is_target { warning } else { fg_ghost },
                ));

                self.editor_overlay_text_system
                    .set_size(Some(preview_text_width), Some(line_height));
                glyphs.extend(layout_panel_text(
                    &clamp_monospace_text(&line.text, preview_text_width, font_size),
                    &mut self.editor_overlay_text_system,
                    &mut self.atlas,
                    &self.queue,
                    right_x + 10.0 + line_number_width,
                    row_y,
                    if line.is_target { fg } else { fg_dim },
                ));
            }
        } else {
            self.editor_overlay_text_system
                .set_size(Some((right_w - 20.0).max(1.0)), Some(line_height));
            glyphs.extend(layout_panel_text(
                if diagnostics.results.is_empty() { "No diagnostics" } else { "Loading preview..." },
                &mut self.editor_overlay_text_system,
                &mut self.atlas,
                &self.queue,
                right_x + 10.0,
                content_top + 4.0,
                fg_ghost,
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

    /// Rebuild overlay glyph instances and floating-box chrome for the editor layer.
    pub fn update_editor_overlays(&mut self, app_state: &AppState, center_bounds: [f32; 4]) {
        if app_state.current_overlays().is_empty() && app_state.completion().is_none() {
            self.clear_editor_overlays();
            return;
        }

        let geometry = editor_viewport_geometry(self, app_state, center_bounds);
        let viewport_top = center_bounds[1] + self.editor_padding_y;
        let viewport_bottom =
            viewport_top + (center_bounds[3] - self.editor_padding_y * 2.0).max(1.0);
        let viewport_right = center_bounds[0] + center_bounds[2] - self.editor_padding_x;

        self.editor_overlay_scissor = rect_to_scissor(center_bounds);
        let mut glyphs = Vec::new();
        let mut chrome_quads: Vec<RegionDrawInstance> = Vec::new();

        for overlay in app_state.current_overlays() {
            match overlay {
                EditorOverlay::VirtualText {
                    line,
                    column: _,
                    text,
                    color_token,
                } => {
                    let line_end_byte = app_state.line_content_end_byte_idx(*line);
                    let line_start_byte = app_state.line_start_byte_idx(*line);
                    let byte_in_line = line_end_byte.saturating_sub(line_start_byte);

                    let mut line_top: Option<f32> = None;
                    let mut line_height_px = geometry.line_height.max(1.0);
                    let mut tail_x = geometry.origin_x;

                    for run in self.text_system.buffer().layout_runs() {
                        if run.line_i != *line {
                            continue;
                        }
                        let candidate_top = geometry.origin_y + run.line_top;
                        let candidate_bottom = candidate_top + run.line_height.max(1.0);
                        if candidate_bottom <= viewport_top || candidate_top >= viewport_bottom {
                            continue;
                        }
                        line_top = Some(candidate_top);
                        line_height_px = run.line_height.max(1.0);
                        tail_x = tail_x.max(run_x_for_byte(geometry.origin_x, &run, byte_in_line));
                    }

                    let Some(line_top) = line_top else { continue };
                    let origin_x = (tail_x + 10.0).max(geometry.viewport_text_left + 4.0);
                    if origin_x >= viewport_right {
                        continue;
                    }
                    let width = (viewport_right - origin_x).max(1.0);
                    self.editor_overlay_text_system
                        .set_size(Some(width), Some(line_height_px));
                    glyphs.extend(layout_panel_text_italic(
                        text,
                        &mut self.editor_overlay_text_system,
                        &mut self.atlas,
                        &self.queue,
                        origin_x,
                        line_top,
                        match color_token {
                            OverlayColorToken::UiFgGhost => self.theme.ui.fg_ghost.as_f32(),
                        },
                    ));
                }
                EditorOverlay::FloatingBox {
                    anchor_line,
                    anchor_col: _,
                    blocks,
                    style,
                } => {
                    const PAD_X: f32 = 10.0;
                    const PAD_Y: f32 = 6.0;
                    const BORDER: f32 = 1.0;
                    let char_w = (geometry.font_size * 0.6).max(1.0);
                    let max_len = blocks
                        .iter()
                        .map(|block| match block {
                            FloatingBoxBlock::Prose(text) => {
                                text.lines().map(|line| line.len()).max().unwrap_or(0)
                            }
                            FloatingBoxBlock::Code { text, .. } => {
                                text.lines().map(|line| line.len()).max().unwrap_or(0)
                            }
                        })
                        .max()
                        .unwrap_or(0);
                    let line_count = blocks
                        .iter()
                        .map(|block| match block {
                            FloatingBoxBlock::Prose(text) | FloatingBoxBlock::Code { text, .. } => {
                                text.lines().count().max(1)
                            }
                        })
                        .sum::<usize>();
                    let popup_w = (max_len as f32 * char_w + PAD_X * 2.0)
                        .clamp(120.0, geometry.viewport_text_width);
                    let popup_h = (line_count as f32 * geometry.line_height + PAD_Y * 2.0)
                        .max(geometry.line_height);

                    let anchor_y = geometry.origin_y + (*anchor_line as f32) * geometry.line_height;
                    let mut popup_y = anchor_y + geometry.line_height;
                    if popup_y + popup_h > viewport_bottom {
                        popup_y = (anchor_y - popup_h).max(center_bounds[1]);
                    }
                    let popup_x = geometry.viewport_text_left.max(center_bounds[0]);

                    let bg_color = self.theme.ui.panel_bg.as_f32();
                    let border_color = match style {
                        FloatingBoxStyle::DocHover => self.theme.ui.accent.as_f32(),
                        FloatingBoxStyle::PeekWindow => self.theme.ui.warning.as_f32(),
                    };
                    chrome_quads.push(RegionDrawInstance::new(
                        [
                            popup_x - BORDER,
                            popup_y - BORDER,
                            popup_w + BORDER * 2.0,
                            popup_h + BORDER * 2.0,
                        ],
                        border_color,
                    ));
                    chrome_quads.push(RegionDrawInstance::new(
                        [popup_x, popup_y, popup_w, popup_h],
                        bg_color,
                    ));

                    let text_x = popup_x + PAD_X;
                    let line_w = (popup_w - PAD_X * 2.0).max(1.0);
                    let mut block_line_offset = 0usize;
                    for block in blocks {
                        match block {
                            FloatingBoxBlock::Prose(text) => {
                                let block_lines: Vec<&str> = text.lines().collect();
                                for (line_idx, line_text) in block_lines.iter().enumerate() {
                                    let text_y = popup_y
                                        + PAD_Y
                                        + (block_line_offset + line_idx) as f32 * geometry.line_height;
                                    if text_y + geometry.line_height < viewport_top
                                        || text_y > viewport_bottom
                                    {
                                        continue;
                                    }
                                    self.editor_overlay_text_system
                                        .set_size(Some(line_w), Some(geometry.line_height));
                                    glyphs.extend(layout_panel_text_italic(
                                        line_text,
                                        &mut self.editor_overlay_text_system,
                                        &mut self.atlas,
                                        &self.queue,
                                        text_x,
                                        text_y,
                                        self.theme.ui.fg_ghost.as_f32(),
                                    ));
                                }
                                block_line_offset += block_lines.len().max(1);
                            }
                            FloatingBoxBlock::Code { text, spans } => {
                                let block_lines: Vec<&str> = text.lines().collect();
                                for (line_idx, line_text) in block_lines.iter().enumerate() {
                                    let text_y = popup_y
                                        + PAD_Y
                                        + (block_line_offset + line_idx) as f32 * geometry.line_height;
                                    if text_y + geometry.line_height < viewport_top
                                        || text_y > viewport_bottom
                                    {
                                        continue;
                                    }
                                    let line_start = block_lines
                                        .iter()
                                        .take(line_idx)
                                        .map(|line| line.len() + 1)
                                        .sum::<usize>();
                                    let line_end = line_start + line_text.len();
                                    let line_spans: Vec<StyledTextSpan> = spans
                                        .iter()
                                        .filter_map(|span| {
                                            if span.end <= line_start || span.start >= line_end {
                                                return None;
                                            }
                                            Some(StyledTextSpan::with_style(
                                                span.start.max(line_start) - line_start,
                                                span.end.min(line_end) - line_start,
                                                span.color_rgba,
                                                span.bold,
                                                span.italic,
                                            ))
                                        })
                                        .collect();
                                    self.editor_overlay_text_system
                                        .set_size(Some(line_w), Some(geometry.line_height));
                                    glyphs.extend(layout_panel_rich_text(
                                        line_text,
                                        &line_spans,
                                        self.theme.ui.fg.as_f32(),
                                        &mut self.editor_overlay_text_system,
                                        &mut self.atlas,
                                        &self.queue,
                                        text_x,
                                        text_y,
                                    ));
                                }
                                block_line_offset += block_lines.len().max(1);
                            }
                        }
                    }
                }
            }
        }

        if let Some(completion) = app_state.completion() {
            const PAD_X: f32 = 10.0;
            const PAD_Y: f32 = 6.0;
            const BORDER: f32 = 1.0;
            const MAX_VISIBLE_ROWS: usize = 8;

            let char_w = (geometry.font_size * 0.6).max(1.0);
            let max_len = completion
                .items
                .iter()
                .map(|item| item.label.chars().count() + 3)
                .max()
                .unwrap_or(8);
            let visible_rows = completion.items.len().min(MAX_VISIBLE_ROWS).max(1);
            let popup_w = (max_len as f32 * char_w + PAD_X * 2.0).clamp(160.0, geometry.viewport_text_width);
            let popup_h = (visible_rows as f32 * geometry.line_height + PAD_Y * 2.0).max(geometry.line_height);

            let anchor_x = (geometry.origin_x + completion.anchor_col as f32 * char_w)
                .clamp(geometry.viewport_text_left, viewport_right - popup_w);
            let anchor_y = geometry.origin_y + completion.anchor_line as f32 * geometry.line_height;
            let mut popup_y = anchor_y + geometry.line_height;
            if popup_y + popup_h > viewport_bottom {
                popup_y = (anchor_y - popup_h).max(center_bounds[1]);
            }

            let bg_color = self.theme.ui.panel_bg.as_f32();
            let border_color = self.theme.ui.border_color.as_f32();
            let selection_bg = self.theme.ui.selection_bg.as_f32();

            chrome_quads.push(RegionDrawInstance::new(
                [anchor_x - BORDER, popup_y - BORDER, popup_w + BORDER * 2.0, popup_h + BORDER * 2.0],
                border_color,
            ));
            chrome_quads.push(RegionDrawInstance::new(
                [anchor_x, popup_y, popup_w, popup_h],
                bg_color,
            ));

            let scroll_start = if completion.selected_index >= visible_rows {
                completion.selected_index + 1 - visible_rows
            } else {
                0
            };
            let visible_items = completion
                .items
                .iter()
                .enumerate()
                .skip(scroll_start)
                .take(visible_rows);

            for (row_idx, (item_idx, item)) in visible_items.enumerate() {
                let row_y = popup_y + PAD_Y + row_idx as f32 * geometry.line_height;
                if item_idx == completion.selected_index {
                    chrome_quads.push(RegionDrawInstance::new(
                        [anchor_x + 1.0, row_y, (popup_w - 2.0).max(1.0), geometry.line_height],
                        selection_bg,
                    ));
                }

                let icon = match item.kind {
                    Some(3) => "ⓕ",
                    Some(6) => "ⓥ",
                    Some(7) => "ⓒ",
                    Some(14) => "ⓚ",
                    _ => "•",
                };
                let row_text = format!("{} {}", icon, item.label);
                self.editor_overlay_text_system
                    .set_size(Some((popup_w - PAD_X * 2.0).max(1.0)), Some(geometry.line_height));
                glyphs.extend(layout_panel_text(
                    &row_text,
                    &mut self.editor_overlay_text_system,
                    &mut self.atlas,
                    &self.queue,
                    anchor_x + PAD_X,
                    row_y,
                    self.theme.ui.fg.as_f32(),
                ));
            }
        }

        self.editor_overlay_chrome_instances = chrome_quads;
        self.editor_overlay_glyph_instances = glyphs;
        self.editor_overlay_text_pipeline.upload_instances(
            &self.device,
            &self.queue,
            &self.editor_overlay_glyph_instances,
        );
    }

    /// Returns a quad for the current-line highlight (drawn behind text).
    pub fn current_line_highlight_quad(
        &self,
        app_state: &AppState,
        center_bounds: [f32; 4],
    ) -> Option<RegionDrawInstance> {
        let line_height = self.theme.editor.line_height;
        let font_size = self.theme.editor.font_size;
        let total_lines = app_state.total_lines().max(1);
        let gutter_digits = total_lines.to_string().len().max(3);
        let gutter_width = gutter_width_for_editor(gutter_digits, font_size, line_height);
        let text_area_x = center_bounds[0] + self.editor_padding_x + gutter_width;
        let text_area_w = (center_bounds[2] - self.editor_padding_x - gutter_width).max(1.0);
        let scroll_y = app_state.scroll_line as f32 * line_height;
        let origin_y = center_bounds[1] + self.editor_padding_y + line_height - scroll_y;
        let caret_layout =
            compute_caret_layout(&self.text_system, app_state, [text_area_x, origin_y]);

        let viewport_top = center_bounds[1] + self.editor_padding_y;
        let viewport_bottom =
            viewport_top + (center_bounds[3] - self.editor_padding_y * 2.0).max(1.0);
        let line_top = caret_layout.top;
        let line_bottom = line_top + caret_layout.height.max(1.0);
        if line_bottom <= viewport_top || line_top >= viewport_bottom {
            return None;
        }

        let mut color = self.theme.editor.selection.as_f32();
        color[3] = (color[3] * 0.22).clamp(0.10, 0.30);
        Some(RegionDrawInstance::new(
            [
                text_area_x,
                line_top,
                text_area_w,
                caret_layout.height.max(1.0),
            ],
            color,
        ))
    }

    /// Returns per-line selection quads for Visual mode (empty outside Visual).
    pub fn visual_selection_quads(
        &self,
        app_state: &AppState,
        center_bounds: [f32; 4],
    ) -> Vec<RegionDrawInstance> {
        if app_state.current_mode() != EditorMode::Visual {
            return Vec::new();
        }
        let Some(selection) = app_state.visual_selection_range() else {
            return Vec::new();
        };

        let line_height = self.theme.editor.line_height;
        let font_size = self.theme.editor.font_size;
        let total_lines = app_state.total_lines().max(1);
        let gutter_digits = total_lines.to_string().len().max(3);
        let gutter_width = gutter_width_for_editor(gutter_digits, font_size, line_height);
        let text_area_x = center_bounds[0] + self.editor_padding_x + gutter_width;
        let text_area_w = (center_bounds[2] - self.editor_padding_x - gutter_width).max(1.0);
        let scroll_y = app_state.scroll_line as f32 * line_height;
        let origin_y = center_bounds[1] + self.editor_padding_y + line_height - scroll_y;

        let viewport_top = center_bounds[1] + self.editor_padding_y;
        let viewport_bottom =
            viewport_top + (center_bounds[3] - self.editor_padding_y * 2.0).max(1.0);

        let mut color = self.theme.editor.selection.as_f32();
        color[3] = (color[3] * 0.45).clamp(0.18, 0.42);

        let mut quads = Vec::new();
        for run in self.text_system.buffer().layout_runs() {
            if run.line_i < selection.start_line || run.line_i > selection.end_line {
                continue;
            }
            let line_top = origin_y + run.line_top;
            let line_height_px = run.line_height.max(1.0);
            let line_bottom = line_top + line_height_px;
            if line_bottom <= viewport_top || line_top >= viewport_bottom {
                continue;
            }

            let line_start_x = text_area_x;
            let line_end_x = (text_area_x + run.line_w).max(line_start_x + 1.0);
            let start_x = if run.line_i == selection.start_line {
                run_x_for_byte(text_area_x, &run, selection.start_byte_in_line)
            } else {
                line_start_x
            };
            let end_x = if run.line_i == selection.end_line {
                run_x_for_byte(text_area_x, &run, selection.end_byte_in_line)
            } else {
                line_end_x
            };

            let left = start_x.min(end_x).max(text_area_x);
            let right = start_x.max(end_x).min(text_area_x + text_area_w);
            let width = (right - left).max(1.0);
            quads.push(RegionDrawInstance::new(
                [left, line_top, width, line_height_px],
                color,
            ));
        }

        quads
    }

    /// Returns per-match quads for `/` and `*` search highlights in the active buffer.
    pub fn search_highlight_quads(
        &self,
        app_state: &AppState,
        center_bounds: [f32; 4],
    ) -> Vec<RegionDrawInstance> {
        if app_state.search_highlights().is_empty() {
            return Vec::new();
        }

        let line_height = self.theme.editor.line_height;
        let font_size = self.theme.editor.font_size;
        let total_lines = app_state.total_lines().max(1);
        let gutter_digits = total_lines.to_string().len().max(3);
        let gutter_width = gutter_width_for_editor(gutter_digits, font_size, line_height);
        let text_area_x = center_bounds[0] + self.editor_padding_x + gutter_width;
        let text_area_w = (center_bounds[2] - self.editor_padding_x - gutter_width).max(1.0);
        let scroll_y = app_state.scroll_line as f32 * line_height;
        let origin_y = center_bounds[1] + self.editor_padding_y + line_height - scroll_y;

        let viewport_top = center_bounds[1] + self.editor_padding_y;
        let viewport_bottom =
            viewport_top + (center_bounds[3] - self.editor_padding_y * 2.0).max(1.0);

        let mut color = self.theme.ui.warning.as_f32();
        color[3] = color[3].clamp(0.26, 0.38);

        let mut quads = Vec::new();
        for &(start_byte, end_byte) in app_state.search_highlights() {
            if start_byte >= end_byte {
                continue;
            }

            let start_line = app_state.byte_to_line_idx(start_byte);
            let end_line = app_state.byte_to_line_idx(end_byte.saturating_sub(1));

            for run in self.text_system.buffer().layout_runs() {
                if run.line_i < start_line || run.line_i > end_line {
                    continue;
                }

                let line_top = origin_y + run.line_top;
                let line_height_px = run.line_height.max(1.0);
                let line_bottom = line_top + line_height_px;
                if line_bottom <= viewport_top || line_top >= viewport_bottom {
                    continue;
                }

                let line_start_byte = app_state.line_start_byte_idx(run.line_i);
                let line_end_byte = app_state.line_end_byte_idx(run.line_i);
                let local_start = if run.line_i == start_line {
                    start_byte.saturating_sub(line_start_byte)
                } else {
                    0
                };
                let local_end = if run.line_i == end_line {
                    end_byte.saturating_sub(line_start_byte)
                } else {
                    line_end_byte.saturating_sub(line_start_byte)
                };
                if local_start >= local_end {
                    continue;
                }

                let line_start_x = text_area_x;
                let line_end_x = (text_area_x + run.line_w).max(line_start_x + 1.0);
                let start_x = if run.line_i == start_line {
                    run_x_for_byte(text_area_x, &run, local_start)
                } else {
                    line_start_x
                };
                let end_x = if run.line_i == end_line {
                    run_x_for_byte(text_area_x, &run, local_end)
                } else {
                    line_end_x
                };

                let left = start_x.min(end_x).max(text_area_x);
                let right = start_x.max(end_x).min(text_area_x + text_area_w);
                let width = (right - left).max(2.0);
                quads.push(RegionDrawInstance::new(
                    [left, line_top, width, line_height_px],
                    color,
                ));
            }
        }

        quads
    }

    /// Render line numbers in the gutter. Called by both `update_editor_content`
    /// and `update_editor_caret`.
    pub(super) fn update_editor_gutter(
        &mut self,
        app_state: &AppState,
        center_bounds: [f32; 4],
        line_height: f32,
        font_size: f32,
        gutter_digits: usize,
        gutter_width: f32,
    ) {
        let total_lines = app_state.total_lines().max(1);
        let scroll_line = app_state.scroll_line;
        let scroll_y = scroll_line as f32 * line_height;
        let (cursor_line, _) = app_state.cursor_line_col();
        let gutter_bg_color = self.theme.editor.gutter.as_f32();
        let gutter_text_color = self.theme.ui.fg_dim.as_f32();
        let gutter_active_color = self.theme.editor.gutter_active.as_f32();
        let gutter_x = center_bounds[0] + self.editor_padding_x;

        let gutter_font_size = (font_size + 3.0).min(line_height - 2.0).max(8.0);
        self.gutter_text_system
            .set_metrics(Metrics::new(gutter_font_size, line_height));
        self.gutter_text_system
            .set_size(Some(gutter_width), Some(line_height));

        let origin_y = center_bounds[1] + self.editor_padding_y + line_height - scroll_y;
        let viewport_top = center_bounds[1];
        let viewport_bottom = center_bounds[1] + center_bounds[3];

        let mut gutter_glyphs: Vec<GlyphInstance> = Vec::new();
        let mut quads: Vec<RegionDrawInstance> = Vec::new();

        // Clear gutter background to avoid stale pixels from previous frame.
        quads.push(RegionDrawInstance::new(
            [gutter_x, center_bounds[1], gutter_width, center_bounds[3]],
            gutter_bg_color,
        ));

        let active_line_color = {
            let mut c = self.theme.editor.selection.as_f32();
            c[3] = 0.22;
            c
        };

        // Only draw the first run of each logical line (skip soft-wrap continuations).
        let mut last_drawn_line: Option<usize> = None;
        for run in self.text_system.buffer().layout_runs() {
            let abs_line = run.line_i;
            if abs_line >= total_lines {
                break;
            }
            if last_drawn_line == Some(abs_line) {
                continue;
            }

            let line_top_y = origin_y + run.line_top;
            if line_top_y + line_height < viewport_top {
                continue;
            }
            if line_top_y > viewport_bottom {
                break;
            }

            last_drawn_line = Some(abs_line);

            if abs_line == cursor_line {
                quads.push(RegionDrawInstance::new(
                    [gutter_x, line_top_y, gutter_width, run.line_height.max(1.0)],
                    active_line_color,
                ));
            }

            let num_str = if self.relative_numbers {
                let dist = abs_line.abs_diff(cursor_line);
                if dist == 0 {
                    format!("{}", abs_line + 1)
                } else {
                    format!("{dist}")
                }
            } else {
                format!("{}", abs_line + 1)
            };
            let label = format!("{:>width$} ", num_str, width = gutter_digits);
            let color = if abs_line == cursor_line {
                gutter_text_color
            } else {
                gutter_active_color
            };

            gutter_glyphs.extend(layout_panel_text(
                &label,
                &mut self.gutter_text_system,
                &mut self.atlas,
                &self.queue,
                gutter_x,
                line_top_y,
                color,
            ));
        }

        self.gutter_glyph_instances = gutter_glyphs;
        self.gutter_text_pipeline.upload_instances(
            &self.device,
            &self.queue,
            &self.gutter_glyph_instances,
        );
        // Gutter background quads uploaded to region_pipeline (drawn before text in render()).
        self.region_pipeline
            .upload_instances(&self.device, &self.queue, &quads);
    }

    pub fn update_fuzzy_picker_buffer_content(
        &mut self,
        fuzzy_state: &crate::app::app_state::FuzzyState,
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

        let pad_x = self.editor_padding_x.max(14.0);
        let pad_y = self.editor_padding_y.max(14.0);
        let panel_x = center_bounds[0] + pad_x;
        let panel_y = center_bounds[1] + pad_y;
        let panel_w = (center_bounds[2] - pad_x * 2.0).max(1.0);
        let panel_h = (center_bounds[3] - pad_y * 2.0).max(1.0);
        let gap = 16.0;

        let left_w = (panel_w * 0.5).max(1.0);
        let right_w = (panel_w - left_w - gap).max(1.0);
        let left_x = panel_x;
        let right_x = left_x + left_w + gap;

        let header_h = line_height + 14.0;
        let footer_h = line_height + 10.0;
        let content_top = panel_y + header_h;
        let content_bottom = (panel_y + panel_h - footer_h).max(content_top + line_height);
        let content_h = (content_bottom - content_top).max(line_height);

        let panel_bg = self.theme.ui.panel_bg.as_f32();
        let editor_bg = self.theme.editor.bg.as_f32();
        let mut divider = self.theme.ui.fg_ghost.as_f32();
        divider[3] = divider[3].clamp(0.28, 0.42);
        let accent = self.theme.ui.accent.as_f32();
        let fg = self.theme.ui.fg.as_f32();
        let fg_dim = self.theme.ui.fg_dim.as_f32();
        let fg_ghost = self.theme.ui.fg_ghost.as_f32();
        let selection_bg = self.theme.ui.selection_bg.as_f32();
        let warning = self.theme.ui.warning.as_f32();

        let mut glyphs = Vec::new();
        let mut chrome = vec![
            RegionDrawInstance::new([left_x, panel_y, left_w, panel_h], panel_bg),
            RegionDrawInstance::new([right_x, panel_y, right_w, panel_h], editor_bg),
            RegionDrawInstance::new([right_x - gap * 0.5, panel_y, 1.0, panel_h], divider),
            RegionDrawInstance::new([left_x, panel_y + header_h - 1.0, left_w, 1.0], divider),
            RegionDrawInstance::new([right_x, panel_y + header_h - 1.0, right_w, 1.0], divider),
        ];

        let header_y = panel_y + 6.0;

        let title = match fuzzy_state.mode {
            crate::app::command_palette::CommandPaletteMode::FilePicker => "File Picker",
            crate::app::command_palette::CommandPaletteMode::LiveGrep => "Live Grep",
            _ => "Search",
        };
        let left_header = format!("{}  > {}", title, fuzzy_state.query);

        self.editor_overlay_text_system
            .set_size(Some((left_w - 20.0).max(1.0)), Some(line_height));
        glyphs.extend(layout_panel_text(
            &clamp_monospace_text(&left_header, (left_w - 20.0).max(1.0), font_size),
            &mut self.editor_overlay_text_system,
            &mut self.atlas,
            &self.queue,
            left_x + 10.0,
            header_y,
            fg,
        ));

        let selected = fuzzy_state.results.get(fuzzy_state.selected_index);
        let right_header = selected
            .map(|item| item.label.clone())
            .unwrap_or_else(|| "No file selected".to_string());

        self.editor_overlay_text_system
            .set_size(Some((right_w - 20.0).max(1.0)), Some(line_height));
        glyphs.extend(layout_panel_text(
            &clamp_monospace_text(&right_header, (right_w - 20.0).max(1.0), font_size),
            &mut self.editor_overlay_text_system,
            &mut self.atlas,
            &self.queue,
            right_x + 10.0,
            header_y,
            fg,
        ));

        let help_text = "Type to search  |  Ctrl+N/P / Up/Down to navigate  |  Enter to open";
        self.editor_overlay_text_system
            .set_size(Some((panel_w - 20.0).max(1.0)), Some(line_height));
        glyphs.extend(layout_panel_text(
            &clamp_monospace_text(help_text, (panel_w - 20.0).max(1.0), font_size),
            &mut self.editor_overlay_text_system,
            &mut self.atlas,
            &self.queue,
            panel_x + 10.0,
            panel_y + panel_h - footer_h + 4.0,
            fg_ghost,
        ));

        let row_h = line_height * 2.0 + 8.0;
        let visible_rows = ((content_h / row_h).floor() as usize).max(1);
        let mut start_idx = fuzzy_state.selected_index.saturating_sub(visible_rows / 2);
        if start_idx + visible_rows > fuzzy_state.results.len() {
            start_idx = fuzzy_state.results.len().saturating_sub(visible_rows);
        }
        let left_text_width = (left_w - 20.0).max(1.0);
        for (slot, item_idx) in (start_idx..fuzzy_state.results.len())
            .take(visible_rows)
            .enumerate()
        {
            let item = &fuzzy_state.results[item_idx];
            let row_y = content_top + slot as f32 * row_h;
            let is_selected = item_idx == fuzzy_state.selected_index;
            if is_selected {
                chrome.push(RegionDrawInstance::new(
                    [left_x + 6.0, row_y, (left_w - 12.0).max(1.0), row_h - 4.0],
                    selection_bg,
                ));
                chrome.push(RegionDrawInstance::new(
                    [left_x + 6.0, row_y, 3.0, row_h - 4.0],
                    accent,
                ));
            }

            self.editor_overlay_text_system
                .set_size(Some(left_text_width), Some(line_height));
            glyphs.extend(layout_panel_text(
                &clamp_monospace_text(&item.label, left_text_width, font_size),
                &mut self.editor_overlay_text_system,
                &mut self.atlas,
                &self.queue,
                left_x + 14.0,
                row_y + 4.0,
                if is_selected { fg } else { fg_dim },
            ));

            let summary = item.secondary_label.clone().unwrap_or_default();
            if !summary.is_empty() {
                self.editor_overlay_text_system
                    .set_size(Some(left_text_width), Some(line_height));
                glyphs.extend(layout_panel_text(
                    &clamp_monospace_text(&summary, left_text_width, font_size),
                    &mut self.editor_overlay_text_system,
                    &mut self.atlas,
                    &self.queue,
                    left_x + 14.0,
                    row_y + line_height + 4.0,
                    if is_selected { fg_dim } else { fg_ghost },
                ));
            }
        }

        if !fuzzy_state.preview_lines.is_empty() {
            let line_number_width = estimate_monospace_width(
                &format!(
                    "{:>4}",
                    fuzzy_state
                        .preview_lines
                        .last()
                        .map(|line| line.line_number)
                        .unwrap_or(1)
                ),
                font_size,
            ) + 14.0;
            let preview_text_width = (right_w - 20.0 - line_number_width).max(1.0);
            let preview_rows = ((content_h / line_height).floor() as usize).max(1);
            let mut preview_start = 0usize;
            if let Some(target_idx) = fuzzy_state
                .preview_lines
                .iter()
                .position(|line| line.is_target)
            {
                preview_start = target_idx.saturating_sub(preview_rows / 2);
                if preview_start + preview_rows > fuzzy_state.preview_lines.len() {
                    preview_start = fuzzy_state.preview_lines.len().saturating_sub(preview_rows);
                }
            }

            for (slot, line) in fuzzy_state
                .preview_lines
                .iter()
                .skip(preview_start)
                .take(preview_rows)
                .enumerate()
            {
                let row_y = content_top + slot as f32 * line_height;
                if line.is_target {
                    chrome.push(RegionDrawInstance::new(
                        [right_x + 6.0, row_y, (right_w - 12.0).max(1.0), line_height],
                        selection_bg,
                    ));
                    chrome.push(RegionDrawInstance::new(
                        [right_x + 6.0, row_y, 3.0, line_height],
                        accent,
                    ));
                }

                self.editor_overlay_text_system
                    .set_size(Some(line_number_width), Some(line_height));
                glyphs.extend(layout_panel_text(
                    &format!("{:>4}", line.line_number),
                    &mut self.editor_overlay_text_system,
                    &mut self.atlas,
                    &self.queue,
                    right_x + 10.0,
                    row_y,
                    if line.is_target { warning } else { fg_ghost },
                ));

                self.editor_overlay_text_system
                    .set_size(Some(preview_text_width), Some(line_height));
                let line_start = fuzzy_state
                    .preview_lines
                    .iter()
                    .take(preview_start + slot)
                    .map(|item| item.text.len() + 1)
                    .sum::<usize>();
                let line_end = line_start + line.text.len();
                let line_spans: Vec<StyledTextSpan> = fuzzy_state
                    .preview_spans
                    .iter()
                    .filter_map(|span| {
                        if span.end <= line_start || span.start >= line_end {
                            return None;
                        }
                        Some(StyledTextSpan::with_style(
                            span.start.max(line_start) - line_start,
                            span.end.min(line_end) - line_start,
                            span.color_rgba,
                            span.bold,
                            span.italic,
                        ))
                    })
                    .collect();
                glyphs.extend(layout_panel_rich_text(
                    &clamp_monospace_text(&line.text, preview_text_width, font_size),
                    &line_spans,
                    if line.is_target { fg } else { fg_dim },
                    &mut self.editor_overlay_text_system,
                    &mut self.atlas,
                    &self.queue,
                    right_x + 10.0 + line_number_width,
                    row_y,
                ));
            }
        } else {
            let empty_message = if fuzzy_state.results.is_empty() {
                if fuzzy_state.query.trim().is_empty() {
                    "Type to search..."
                } else {
                    "No matches"
                }
            } else {
                "Loading preview..."
            };
            self.editor_overlay_text_system
                .set_size(Some((right_w - 20.0).max(1.0)), Some(line_height));
            glyphs.extend(layout_panel_text(
                empty_message,
                &mut self.editor_overlay_text_system,
                &mut self.atlas,
                &self.queue,
                right_x + 10.0,
                content_top + 4.0,
                fg_ghost,
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
