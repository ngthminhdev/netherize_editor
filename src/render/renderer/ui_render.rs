//! Right-sidebar (AI Chat) background rendering.
//!
//! Generates region quads for the three-layer panel background used when the
//! RightSidebar is visible: outline border, inner fill, and input-box accent.

use crate::{
    render::{
        glyph_instance::GlyphInstance,
        region_pipeline::RegionDrawInstance,
        renderer::{
            Renderer, TextScissorBatch,
            helpers::{estimate_monospace_width, layout_panel_text, layout_panel_text_italic, rect_to_scissor},
        },
    },
    workbench::panel_state::{AiChatMessage, AiRole},
};

/// Build the three-layer background quads for a visible RightSidebar.
///
/// * **Step 1 — Outline border**: full `bounds`, `border_color` from
///   `theme.ui.border_color`.
/// * **Step 2 — Inner background**: inset by `panel_border_width`, filled
///   with `panel_bg`.
/// * **Step 3 — Input box**: `input_bounds` filled with `input_bg` (a
///   slightly different shade such as `editor.bg`).
pub fn right_sidebar_background_quads(
    bounds: [f32; 4],
    input_bounds: Option<[f32; 4]>,
    panel_border_width: f32,
    border_color: [f32; 4],
    panel_bg: [f32; 4],
    input_bg: [f32; 4],
    border_radius: f32,
) -> Vec<RegionDrawInstance> {
    let [x, y, w, h] = bounds;
    if w <= 0.0 || h <= 0.0 {
        return Vec::new();
    }

    let t = panel_border_width
        .max(0.0)
        .min(w * 0.5)
        .min(h * 0.5);
    let mut quads = Vec::with_capacity(4);

    // Step 1: Outline border — full bounds.
    quads.push(
        RegionDrawInstance::new(bounds, border_color).with_radius(border_radius),
    );

    // Step 2: Inner background — inset by panel_border_width.
    let inner_w = (w - t * 2.0).max(0.0);
    let inner_h = (h - t * 2.0).max(0.0);
    if inner_w > 0.0 && inner_h > 0.0 {
        quads.push(
            RegionDrawInstance::new([x + t, y + t, inner_w, inner_h], panel_bg)
                .with_radius((border_radius - t).max(0.0)),
        );
    }

    // Step 3: Input box background — slightly different shade.
    if let Some([ix, iy, iw, ih]) = input_bounds {
        if iw > 0.0 && ih > 0.0 {
            quads.push(
                RegionDrawInstance::new([ix, iy, iw, ih], input_bg)
                    .with_radius((border_radius - t).max(0.0)),
            );
        }
    }

    quads
}

#[cfg(test)]
mod tests {
    use super::*;

    const BORDER: [f32; 4] = [0.3, 0.3, 0.3, 1.0];
    const PANEL: [f32; 4] = [0.1, 0.1, 0.1, 1.0];
    const INPUT: [f32; 4] = [0.08, 0.08, 0.08, 1.0];

    #[test]
    fn empty_bounds_returns_no_quads() {
        let quads = right_sidebar_background_quads(
            [0.0, 0.0, 0.0, 0.0],
            None,
            1.0,
            BORDER,
            PANEL,
            INPUT,
            8.0,
        );
        assert!(quads.is_empty());
    }

    #[test]
    fn negative_dimensions_return_no_quads() {
        let quads = right_sidebar_background_quads(
            [10.0, 10.0, -5.0, 100.0],
            None,
            1.0,
            BORDER,
            PANEL,
            INPUT,
            8.0,
        );
        assert!(quads.is_empty());
    }

    #[test]
    fn produces_border_and_fill_without_input() {
        let quads = right_sidebar_background_quads(
            [10.0, 20.0, 300.0, 400.0],
            None,
            1.0,
            BORDER,
            PANEL,
            INPUT,
            8.0,
        );
        assert_eq!(quads.len(), 2);
        // First quad is the border (full bounds).
        assert_eq!(quads[0].rect, [10.0, 20.0, 300.0, 400.0]);
        assert_eq!(quads[0].color, BORDER);
        // Second quad is the inner fill (inset by 1px).
        assert_eq!(quads[1].rect, [11.0, 21.0, 298.0, 398.0]);
        assert_eq!(quads[1].color, PANEL);
    }

    #[test]
    fn produces_three_quads_with_input_bounds() {
        let quads = right_sidebar_background_quads(
            [10.0, 20.0, 300.0, 400.0],
            Some([14.0, 320.0, 292.0, 80.0]),
            1.0,
            BORDER,
            PANEL,
            INPUT,
            8.0,
        );
        assert_eq!(quads.len(), 3);
        // Third quad is the input box.
        assert_eq!(quads[2].rect, [14.0, 320.0, 292.0, 80.0]);
        assert_eq!(quads[2].color, INPUT);
    }

    #[test]
    fn zero_size_input_bounds_are_skipped() {
        let quads = right_sidebar_background_quads(
            [10.0, 20.0, 300.0, 400.0],
            Some([14.0, 320.0, 0.0, 0.0]),
            1.0,
            BORDER,
            PANEL,
            INPUT,
            8.0,
        );
        // Only border + fill; input is skipped.
        assert_eq!(quads.len(), 2);
    }

    #[test]
    fn border_radius_is_reduced_for_inner_quads() {
        let quads = right_sidebar_background_quads(
            [0.0, 0.0, 200.0, 200.0],
            Some([12.0, 150.0, 176.0, 40.0]),
            2.0,
            BORDER,
            PANEL,
            INPUT,
            12.0,
        );
        assert_eq!(quads.len(), 3);
        assert_eq!(quads[0].border_radius, 12.0);
        assert_eq!(quads[1].border_radius, 10.0);
        assert_eq!(quads[2].border_radius, 10.0);
    }
}

// ── AI Chat text rendering ──────────────────────────────────────────────────

impl Renderer {
    /// Render chat history + input text for the AI Chat panel.
    /// Returns cursor quads to be drawn via the region pipeline.
    pub fn update_ai_chat_content(
        &mut self,
        history_bounds: [f32; 4],
        input_bounds: [f32; 4],
        messages: &[AiChatMessage],
        input_buffer: &str,
        show_cursor: bool,
        inner_padding: f32,
    ) -> Vec<RegionDrawInstance> {
        if history_bounds[2] < 1.0 || history_bounds[3] < 1.0
            || input_bounds[2] < 1.0 || input_bounds[3] < 1.0
        {
            self.clear_ai_chat();
            return Vec::new();
        }

        let font_size = self.theme.ui.sidebar_font_size;
        let line_h = self.theme.ui.sidebar_line_height.max(font_size + 4.0);
        let accent = self.theme.ui.accent.as_f32();
        let fg_dim = self.theme.ui.fg_dim.as_f32();

        // Scissor rects — padded inward so text doesn't touch edges.
        let hclip = [
            history_bounds[0] + inner_padding,
            history_bounds[1] + inner_padding,
            (history_bounds[2] - inner_padding * 2.0).max(1.0),
            (history_bounds[3] - inner_padding * 2.0).max(1.0),
        ];
        self.ai_chat_history_scissor = rect_to_scissor(hclip);

        let iclip = [
            input_bounds[0] + inner_padding,
            input_bounds[1] + inner_padding,
            (input_bounds[2] - inner_padding * 2.0).max(1.0),
            (input_bounds[3] - inner_padding * 2.0).max(1.0),
        ];
        self.ai_chat_input_scissor = rect_to_scissor(iclip);

        self.ai_chat_text_system
            .set_size(Some(hclip[2].max(1.0)), Some(line_h));

        // ── History glyphs with auto-scroll ───────────────────────────────
        struct ChatLine { text: String, color: [f32; 4], italic: bool }
        let mut lines: Vec<ChatLine> = Vec::new();
        for msg in messages {
            let (pfx, col, ital) = match msg.role {
                AiRole::User      => ("You: ", accent, false),
                AiRole::Assistant  => ("AI: ", fg_dim, false),
                AiRole::System     => ("System: ", fg_dim, true),
            };
            for (i, seg) in msg.text.split('\n').enumerate() {
                let t = if i == 0 { format!("{pfx}{seg}") }
                        else       { format!("     {seg}") };
                lines.push(ChatLine { text: t, color: col, italic: ital });
            }
        }

        let total_h = lines.len() as f32 * line_h;
        let vis_h   = hclip[3];
        let scroll  = if total_h > vis_h { total_h - vis_h } else { 0.0 };

        let mut all: Vec<GlyphInstance> = Vec::new();
        let mut cy = hclip[1];
        for line in &lines {
            let y = cy - scroll;
            if y + line_h > hclip[1] && y < hclip[1] + vis_h {
                let gs = if line.italic {
                    layout_panel_text_italic(&line.text, &mut self.ai_chat_text_system,
                        &mut self.atlas, &self.queue, hclip[0], y, line.color)
                } else {
                    layout_panel_text(&line.text, &mut self.ai_chat_text_system,
                        &mut self.atlas, &self.queue, hclip[0], y, line.color)
                };
                all.extend(gs);
            }
            cy += line_h;
        }

        // ── Input box glyphs ──────────────────────────────────────────────
        let hist_count = all.len() as u32;
        let iy = iclip[1] + ((iclip[3] - line_h).max(0.0) * 0.5)
                    .min(inner_padding * 0.5);
        if !input_buffer.is_empty() {
            all.extend(layout_panel_text(input_buffer,
                &mut self.ai_chat_text_system, &mut self.atlas,
                &self.queue, iclip[0], iy, accent));
        }
        let in_count = all.len() as u32 - hist_count;
        self.ai_chat_input_batch = if in_count > 0 {
            Some(TextScissorBatch {
                scissor: self.ai_chat_input_scissor.unwrap_or([0,0,1,1]),
                range: crate::render::text_pipeline::InstanceDrawRange {
                    start: hist_count, count: in_count,
                },
            })
        } else { None };

        // ── Upload ────────────────────────────────────────────────────────
        self.ai_chat_glyph_instances = all;
        self.ai_chat_text_pipeline.upload_instances(
            &self.device, &self.queue, &self.ai_chat_glyph_instances);

        // ── Cursor quad (drawn via region pipeline) ───────────────────────
        let mut chrome = Vec::new();
        if show_cursor {
            let cx = iclip[0]
                + estimate_monospace_width(input_buffer, font_size);
            let mut cursor_color = accent;
            cursor_color[3] = 0.9;
            chrome.push(RegionDrawInstance::new(
                [cx, iy + 2.0, 8.0, (line_h - 4.0).max(1.0)],
                cursor_color,
            ));
        }
        chrome
    }

    /// Clear AI chat text — called when right sidebar is hidden.
    pub fn clear_ai_chat(&mut self) {
        self.ai_chat_history_scissor = None;
        self.ai_chat_input_scissor = None;
        self.ai_chat_input_batch = None;
        self.ai_chat_glyph_instances.clear();
        self.ai_chat_text_pipeline
            .upload_instances(&self.device, &self.queue, &[]);
    }
}
