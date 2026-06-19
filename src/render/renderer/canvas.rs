//! Render layer for NetherCanvas (Phase A, overlay model).
//!
//! NetherCanvas is an **overlay on top of the live editor** — the sidebar,
//! editor and terminal stay visible underneath. We draw floating code blocks
//! over the editor's center region, plus a top "Spatial Canvas" strip and a
//! bottom keybinding hint bar. Drawn last in the frame so it sits on top, but
//! WITHOUT a full-screen backdrop (the editor must remain visible).

use cosmic_text::Metrics;

use super::Renderer;
use super::helpers::layout_panel_text;
use crate::canvas::{BlockRelation, CanvasState};
use crate::render::region_pipeline::RegionDrawInstance;

const BLOCK_PAD: f32 = 10.0;
const TITLE_LINES: f32 = 1.6;

impl Renderer {
    /// Build + upload the canvas overlay for the given state. Read-only.
    pub fn update_canvas_content(&mut self, canvas: &CanvasState) {
        self.canvas_chrome_instances.clear();
        self.canvas_glyph_instances.clear();
        self.canvas_icon_instances.clear();

        // Overlay area = the editor center region, so the sidebar/terminal stay
        // visible. Fall back to the whole surface if it's not known yet.
        let full = [
            0u32,
            0,
            self.surface_state.config.width,
            self.surface_state.config.height,
        ];
        let scissor = self.editor_scissor.unwrap_or(full);
        self.canvas_scissor = Some(scissor);
        let ax = scissor[0] as f32;
        let ay = scissor[1] as f32;
        let aw = scissor[2] as f32;
        let ah = scissor[3] as f32;

        let cam = canvas.camera;
        // Cards are sized (in app_state) to fit this exact font at zoom 1, so the
        // code reads at the editor's own size — like a mini editor.
        let fs = (self.theme.editor.font_size * cam.zoom).clamp(6.0, 80.0);
        let line_height = fs * 1.35;

        let editor_bg = self.theme.editor.bg.as_f32();
        let fg = self.theme.editor.fg.as_f32();
        // Opaque, clearly-elevated card surface so blocks read as distinct cards
        // over the editor (the previous panel_bg was ~identical to the editor bg).
        let card_bg = blend(editor_bg, fg, 0.10);
        let title_bar_bg = blend(card_bg, fg, 0.07);
        let divider = blend(card_bg, fg, 0.18);
        let border_dim = blend(editor_bg, fg, 0.30);
        let border_focus = self.theme.ui.accent.as_f32();
        let title_fg = self.theme.ui.fg.as_f32();
        let relation_fg = self.theme.ui.fg_dim.as_f32();
        let panel_bg = card_bg;
        let shadow_color = [0.0, 0.0, 0.0, 0.40];
        let c_def = self.theme.ui.success.as_f32();
        let c_caller = self.theme.ui.warning.as_f32();
        let c_callee = self.theme.ui.info.as_f32();
        let radius = 9.0;

        // ── Floating code blocks (over the editor's right side) ───────────────
        self.canvas_text_system.set_metrics(Metrics::new(fs, line_height));
        for block in &canvas.blocks {
            let [sx, sy, sw, sh] = cam.world_to_screen(block.world);
            // Cull blocks outside the overlay area.
            if sx + sw < ax || sy + sh < ay || sx > ax + aw || sy > ay + ah || sw < 2.0 || sh < 2.0
            {
                continue;
            }
            let title_h = (line_height * TITLE_LINES).min(sh * 0.5).max(line_height.min(sh));
            let focused = canvas.focused == Some(block.id);
            let border = if focused { border_focus } else { border_dim };
            let rel_color = match block.relation {
                BlockRelation::Focal => border_focus,
                BlockRelation::Definition => c_def,
                BlockRelation::Caller => c_caller,
                BlockRelation::Callee => c_callee,
            };

            // Drop shadow → border halo → opaque card → title bar → divider.
            self.canvas_chrome_instances.push(
                RegionDrawInstance::new([sx + 4.0, sy + 7.0, sw, sh], shadow_color)
                    .with_radius(radius + 2.0),
            );
            let halo = if focused { 2.5 } else { 1.0 };
            self.canvas_chrome_instances.push(
                RegionDrawInstance::new(
                    [sx - halo, sy - halo, sw + halo * 2.0, sh + halo * 2.0],
                    border,
                )
                .with_radius(radius + halo),
            );
            self.canvas_chrome_instances
                .push(RegionDrawInstance::new([sx, sy, sw, sh], panel_bg).with_radius(radius));
            self.canvas_chrome_instances.push(
                RegionDrawInstance::new([sx, sy, sw, title_h.min(sh)], title_bar_bg)
                    .with_radius(radius),
            );
            self.canvas_chrome_instances.push(RegionDrawInstance::new(
                [sx, sy + title_h - 1.0, sw, 1.0],
                divider,
            ));
            // Active-tab indicator: an accent underline under the focused card's
            // title bar (mimics the editor tab bar).
            if focused {
                self.canvas_chrome_instances.push(RegionDrawInstance::new(
                    [sx, sy + title_h - 2.0, sw, 2.0],
                    border_focus,
                ));
            }
            // A small relation accent bar on the title's left edge.
            self.canvas_chrome_instances.push(
                RegionDrawInstance::new([sx, sy, 3.0, title_h.min(sh)], rel_color)
                    .with_radius(0.0),
            );

            let inner_w = (sw - BLOCK_PAD * 2.0).max(1.0);
            let char_w = (fs * 0.62).max(1.0);
            let max_chars = (inner_w / char_w).floor().max(1.0) as usize;
            self.canvas_text_system.set_size(None, None);

            // Title (left), then the relation label right after it (colored).
            let tag = relation_tag(block.relation);
            let title_budget = max_chars.saturating_sub(tag.chars().count() + 2).max(1);
            let title = clip_line(&block.snapshot.title, title_budget);
            let title_chars = title.chars().count();
            let title_y = sy + (title_h - line_height) * 0.5;
            self.canvas_glyph_instances.extend(layout_panel_text(
                &title,
                &mut self.canvas_text_system,
                &mut self.atlas,
                &self.queue,
                sx + BLOCK_PAD + 4.0,
                title_y,
                title_fg,
            ));
            self.canvas_glyph_instances.extend(layout_panel_text(
                tag,
                &mut self.canvas_text_system,
                &mut self.atlas,
                &self.queue,
                sx + BLOCK_PAD + 4.0 + (title_chars as f32 + 1.0) * char_w,
                title_y,
                rel_color,
            ));

            let body_top = sy + title_h + BLOCK_PAD * 0.5;
            let body_h = sy + sh - BLOCK_PAD - body_top;
            let max_lines = (body_h / line_height).floor() as i32;
            if max_lines > 0 {
                let body = clip_block_text(&block.snapshot.text, max_lines as usize, max_chars);
                self.canvas_glyph_instances.extend(layout_panel_text(
                    &body,
                    &mut self.canvas_text_system,
                    &mut self.atlas,
                    &self.queue,
                    sx + BLOCK_PAD,
                    body_top,
                    fg,
                ));
            }
        }

        // ── Chrome: top strip + bottom hint bar (fixed UI size, not zoomed) ───
        let chrome_fs = (ah * 0.020).clamp(12.0, 24.0);
        let chrome_lh = chrome_fs * 1.35;
        self.canvas_text_system
            .set_metrics(Metrics::new(chrome_fs, chrome_lh));
        self.canvas_text_system.set_size(None, None);

        // Top strip: "‹ Spatial Canvas" + an accent pill.
        let strip_h = chrome_lh + 10.0;
        let strip_y = ay + 8.0;
        let strip_x = ax + 12.0;
        let label = "‹ Spatial Canvas";
        let label_w = label.chars().count() as f32 * chrome_fs * 0.6;
        let pill_label = "Bring Definition Here";
        let pill_w = pill_label.chars().count() as f32 * chrome_fs * 0.62 + 18.0;
        let strip_w = label_w + 16.0 + pill_w + 24.0;
        self.canvas_chrome_instances.push(
            RegionDrawInstance::new([strip_x, strip_y, strip_w, strip_h], panel_bg)
                .with_radius(7.0),
        );
        let text_y = strip_y + (strip_h - chrome_lh) * 0.5;
        self.canvas_glyph_instances.extend(layout_panel_text(
            label,
            &mut self.canvas_text_system,
            &mut self.atlas,
            &self.queue,
            strip_x + 10.0,
            text_y,
            title_fg,
        ));
        let pill_x = strip_x + 10.0 + label_w + 12.0;
        self.canvas_chrome_instances.push(
            RegionDrawInstance::new([pill_x, strip_y + 4.0, pill_w, strip_h - 8.0], border_focus)
                .with_radius(5.0),
        );
        self.canvas_glyph_instances.extend(layout_panel_text(
            pill_label,
            &mut self.canvas_text_system,
            &mut self.atlas,
            &self.queue,
            pill_x + 9.0,
            text_y,
            self.theme.editor.bg.as_f32(),
        ));

        // Bottom hint bar.
        let hint = "gc Canvas    E Expand callee    R Expand caller    P Pin    Tab Next    \u{2191}\u{2193}\u{2190}\u{2192} Navigate    hjkl Pan    Esc Exit";
        let bar_h = chrome_lh + 10.0;
        let bar_y = ay + ah - bar_h - 10.0;
        let bar_x = ax + 12.0;
        let bar_w = aw - 24.0;
        self.canvas_chrome_instances.push(
            RegionDrawInstance::new([bar_x, bar_y, bar_w, bar_h], panel_bg).with_radius(7.0),
        );
        self.canvas_glyph_instances.extend(layout_panel_text(
            hint,
            &mut self.canvas_text_system,
            &mut self.atlas,
            &self.queue,
            bar_x + 14.0,
            bar_y + (bar_h - chrome_lh) * 0.5,
            relation_fg,
        ));

        self.canvas_text_pipeline
            .upload_instances(&self.device, &self.queue, &self.canvas_glyph_instances);
        self.canvas_icon_pipeline.upload_instances(
            &self.device,
            &self.canvas_icon_instances,
            [self.surface_state.config.width, self.surface_state.config.height],
        );
    }

    /// Clear the canvas overlay (called each frame the canvas is inactive).
    pub fn clear_canvas(&mut self) {
        if self.canvas_chrome_instances.is_empty()
            && self.canvas_glyph_instances.is_empty()
            && self.canvas_scissor.is_none()
        {
            return;
        }
        self.canvas_scissor = None;
        self.canvas_chrome_instances.clear();
        self.canvas_glyph_instances.clear();
        self.canvas_icon_instances.clear();
        self.canvas_text_pipeline
            .upload_instances(&self.device, &self.queue, &self.canvas_glyph_instances);
        self.canvas_icon_pipeline.upload_instances(
            &self.device,
            &self.canvas_icon_instances,
            [self.surface_state.config.width, self.surface_state.config.height],
        );
    }
}

/// First line of `s`, clipped to `max_chars` with an ellipsis when truncated.
fn clip_line(s: &str, max_chars: usize) -> String {
    let line = s.lines().next().unwrap_or("");
    if line.chars().count() <= max_chars {
        return line.to_string();
    }
    if max_chars <= 1 {
        return line.chars().take(max_chars).collect();
    }
    let mut t: String = line.chars().take(max_chars - 1).collect();
    t.push('…');
    t
}

/// First `max_lines` of `s`, each clipped to `max_chars`, rejoined with `\n`.
fn clip_block_text(s: &str, max_lines: usize, max_chars: usize) -> String {
    let mut out = String::new();
    for (i, line) in s.lines().take(max_lines).enumerate() {
        if i > 0 {
            out.push('\n');
        }
        if line.chars().count() > max_chars {
            out.extend(line.chars().take(max_chars));
        } else {
            out.push_str(line);
        }
    }
    out
}

fn relation_tag(r: BlockRelation) -> &'static str {
    match r {
        BlockRelation::Focal => "focal",
        BlockRelation::Definition => "def",
        BlockRelation::Caller => "caller",
        BlockRelation::Callee => "callee",
    }
}

fn blend(a: [f32; 4], b: [f32; 4], t: f32) -> [f32; 4] {
    [
        a[0] + (b[0] - a[0]) * t,
        a[1] + (b[1] - a[1]) * t,
        a[2] + (b[2] - a[2]) * t,
        1.0,
    ]
}

#[cfg(test)]
mod tests {
    use super::{clip_block_text, clip_line};

    #[test]
    fn clip_line_truncates_with_ellipsis() {
        assert_eq!(clip_line("hello", 10), "hello");
        assert_eq!(clip_line("hello world", 5), "hell…");
        assert_eq!(clip_line("a\nb", 10), "a");
    }

    #[test]
    fn clip_block_text_limits_lines_and_chars() {
        let src = "line one is long\nline two\nline three\nline four";
        assert_eq!(clip_block_text(src, 2, 8), "line one\nline two");
        assert_eq!(clip_block_text("a\nb", 10, 10), "a\nb");
    }
}
