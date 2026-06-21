//! Render layer for NetherCanvas v2 (overlay model).
//!
//! Cards render like mini editors: a tab-style header, a line-number gutter and
//! syntax-highlighted code laid out **per line** at the editor's own geometry
//! (the same technique as the PeekWindow), so scale, highlight and clipping are
//! correct. The canvas overlays the editor's center region; the **focal block is
//! not drawn** — the editor itself is the focal anchor.

use cosmic_text::Metrics;

use super::Renderer;
use super::components::{estimate_help_keycaps_width, layout_help_keycaps};
use super::helpers::{
    card_mode_label, estimate_monospace_width, layout_panel_rich_text, layout_panel_text,
    mode_pill_color,
};
use crate::canvas::{BlockRelation, CARD_HEADER_LINES, CanvasSpan, CanvasState};
use crate::core::mode::EditorMode;
use crate::codegraph::edges::elbow;
use crate::codegraph::layout::PillRect;
use crate::render::icon_pipeline::{IconDrawInstance, canonical_icon_id};
use crate::render::region_pipeline::RegionDrawInstance;
use crate::text::text_system::StyledTextSpan;

const PAD: f32 = 10.0;
/// Tab width (in columns) the card body **and** the edit caret use. Set on the
/// card text system so shaped tabs and the caret's visual-column math agree —
/// otherwise the caret drifts left of the text on tab-indented lines.
const CARD_TAB_WIDTH: usize = 4;
const CANVAS_TITLE_ICON_GAP: f32 = 4.0;

impl Renderer {
    /// Physical-pixel height of the canvas pane (the full editor region incl. the
    /// breadcrumb row) — the area the canvas actually floats over. Auto-arrange
    /// uses it (÷ zoom → world units) so cards wrap into columns that fit the
    /// VISIBLE pane rather than the whole window (which includes tab/status bars
    /// and any bottom panel). `None` before the first editor render.
    pub(crate) fn canvas_pane_height(&self) -> Option<f32> {
        self.editor_full_scissor.map(|s| s[3] as f32)
    }

    /// Build + upload the canvas overlay for the given state. Read-only.
    /// `mode` is the shared global editor mode the card borrows while editing
    /// (S2). It drives: the caret shape (block in Normal/Visual, thin beam in
    /// Insert), the edit-target card's border/ring + header underline color, and
    /// the mode badge shown in that card's header — so the card reads like a mini
    /// main editor with its own status.
    pub fn update_canvas_content(
        &mut self,
        canvas: &CanvasState,
        mode: EditorMode,
        card_completion: Option<&crate::app::app_state::CompletionState>,
        pending: bool,
    ) {
        // Block caret in Normal/Visual; thin beam in Insert (matches main editor).
        let caret_block = matches!(
            mode,
            EditorMode::Normal | EditorMode::Visual | EditorMode::VisualBlock
        );
        // The edit-target card's accent follows the active mode (NORMAL/INSERT/…).
        let mode_color = mode_pill_color(mode, &self.theme);
        // None for app-global modes (palette/terminal/multi-cursor/resize) — the
        // card is not being edited in those, so the badge hides and the ring falls
        // back to cyan focus instead of the mode pill color.
        let card_mode = card_mode_label(mode);
        self.canvas_chrome_instances.clear();
        self.canvas_glyph_instances.clear();
        self.canvas_icon_instances.clear();

        let full = [
            0u32,
            0,
            self.surface_state.config.width,
            self.surface_state.config.height,
        ];
        // Float over the full editor pane (incl. the breadcrumb row), NOT the
        // text-clipped `editor_scissor` — so a card panned upward isn't cut off
        // under the breadcrumb. Falls back to the text scissor, then the surface.
        let scissor = self
            .editor_full_scissor
            .or(self.editor_scissor)
            .unwrap_or(full);
        self.canvas_scissor = Some(scissor);
        let ax = scissor[0] as f32;
        let ay = scissor[1] as f32;
        let aw = scissor[2] as f32;
        let ah = scissor[3] as f32;

        let cam = canvas.camera;
        // Match the main editor EXACTLY (font size + line height) so a card reads
        // like a mini editor pane, not a differently-spaced popup. Sizing in
        // app_state uses the same editor.line_height, so capacity stays in sync.
        let fs = (self.theme.editor.font_size * cam.zoom).clamp(6.0, 80.0);
        let line_height = self.theme.editor.line_height * cam.zoom;
        let char_w = (fs * 0.6).max(1.0);

        let editor_bg = self.theme.editor.bg.as_f32();
        let fg = self.theme.editor.fg.as_f32();
        // Card body uses the EDITOR background (it's a mini editor pane); the
        // header is a slightly lighter "tab bar", with a clearly visible border
        // so the pane is delineated against the editor behind it.
        let card_bg = editor_bg;
        let header_bg = blend(editor_bg, fg, 0.06);
        let divider = blend(editor_bg, fg, 0.16);
        let border_dim = blend(editor_bg, fg, 0.34);
        let border_focus = self.theme.ui.cyan.as_f32();
        let title_fg = self.theme.ui.fg.as_f32();
        let gutter_fg = self.theme.ui.fg_dim.as_f32();
        let shadow = [0.0, 0.0, 0.0, 0.45];
        let c_def = self.theme.ui.success.as_f32();
        let c_caller = self.theme.ui.warning.as_f32();
        let c_callee = self.theme.ui.info.as_f32();
        let pin_color = self.theme.ui.amber.as_f32();
        let caret_color = self.theme.ui.cyan.as_f32();
        let radius = 9.0;
        let title_h = line_height * CARD_HEADER_LINES;
        let gutter_w = 5.0 * char_w;

        // Phase B interaction sub-state drives the chrome: in the background (S3)
        // the editor is primary — no scrim, no hints, cards float de-emphasized;
        // while editing (S2) the edit-target card renders live with a caret.
        let background = canvas.is_background();
        let (edit_block, edit_cursor) = match canvas.interaction {
            crate::canvas::CanvasInteraction::EditCard {
                block,
                cursor_line,
                cursor_col,
            } => (Some(block), Some((cursor_line, cursor_col))),
            _ => (None, None),
        };

        // Dim scrim over the editor region so the cards read as floating ABOVE
        // the live editor. Skipped in the background state so the editor stays
        // fully usable with the cards as floating reference.
        if !background {
            // A light scrim so the cards read as floating ABOVE the live editor.
            // Editing a card (v2) does NOT switch the active buffer, so the main
            // editor keeps showing the gc-origin file — it must stay VISIBLE
            // behind the cards (no heavy edit scrim).
            self.canvas_chrome_instances
                .push(RegionDrawInstance::new([ax, ay, aw, ah], [0.0, 0.0, 0.0, 0.35]));
        }

        // ── Connectors: elbow from the editor's focal symbol → each card, drawn
        //    first so cards sit on top. The editor (not a card) is the focal
        //    anchor; the connector starts at the **focal symbol's line** (where `gc`
        //    was triggered) and runs rightward into each card's left edge. We use
        //    `editor_focal_screen` (the gc line, projected) so coding in the editor
        //    does NOT drag the connector with the caret; fall back to the live caret,
        //    then to the invisible focal block's screen position when unknown.
        // Project an editor anchor (focal line / caret) to a connector origin,
        // BUT only while that line is inside the visible text region `[ay, ay+ah]`.
        // When the gc line scrolls off the top/bottom, a clamped-to-edge anchor
        // reads as a stale line stuck to the viewport edge — hide it instead.
        let visible_anchor = |[cx, cy, _, ch]: [f32; 4]| -> Option<PillRect> {
            let center_y = cy + ch * 0.5;
            if center_y < ay || center_y > ay + ah {
                None
            } else {
                Some(PillRect {
                    x: cx.clamp(ax, ax + aw),
                    y: center_y,
                    w: 0.0,
                    h: 0.0,
                })
            }
        };
        let focal_source = match self.editor_focal_screen {
            // Editor shows the focal file: anchor at the gc line, or hide when it
            // scrolled out of view (do NOT fall back to the caret here — that would
            // re-introduce the caret-following the focal anchor was added to avoid).
            Some(rect) => visible_anchor(rect),
            // No focal position (editor on a different file): try the caret, else
            // the invisible focal block's screen position as a last resort.
            None => self
                .editor_caret_screen
                .and_then(visible_anchor)
                .or_else(|| {
                    canvas
                        .blocks
                        .iter()
                        .find(|b| b.relation == BlockRelation::Focal)
                        .map(|focal| {
                            let [fx, fy, fw, fh] = cam.world_to_screen(focal.world);
                            PillRect {
                                x: fx,
                                y: fy,
                                w: fw,
                                h: fh,
                            }
                        })
                }),
        };
        {
            let conn = [border_focus[0], border_focus[1], border_focus[2], 0.55];
            for block in &canvas.blocks {
                if block.relation == BlockRelation::Focal {
                    continue;
                }
                let [bx, by, bw, bh] = cam.world_to_screen(block.world);
                if bx > ax + aw || bx + bw < ax || by > ay + ah || by + bh < ay {
                    continue;
                }
                let pill = PillRect {
                    x: bx,
                    y: by,
                    w: bw,
                    h: bh,
                };
                // A card spawned from another card (in-card gd/gr) runs its
                // connector from the PARENT card; otherwise from the focal source.
                let source = block
                    .parent
                    .and_then(|pid| canvas.blocks.iter().find(|b| b.id == pid))
                    .map(|p| {
                        let [px, py, pw, ph] = cam.world_to_screen(p.world);
                        PillRect {
                            x: px,
                            y: py,
                            w: pw,
                            h: ph,
                        }
                    })
                    .or(focal_source);
                if let Some(src) = source {
                    for seg in elbow(src, pill, true, cam.zoom.max(0.6)) {
                        self.canvas_chrome_instances
                            .push(RegionDrawInstance::new([seg.x, seg.y, seg.w, seg.h], conn));
                    }
                }
            }
        }

        self.canvas_text_system.set_metrics(Metrics::new(fs, line_height));
        // Shape tabs at a known width so the edit caret's tab-expansion matches.
        self.canvas_text_system.set_tab_width(CARD_TAB_WIDTH as u16);

        // Anchor for the in-card LSP popup (hover/completion): captured at the edit
        // caret, rendered AFTER the card loop so it floats above the other cards.
        // [caret_x, caret_bottom_y, card_x, card_w].
        let mut overlay_anchor: Option<[f32; 4]> = None;

        for block in &canvas.blocks {
            // The editor is the focal anchor — never draw the focal as a card.
            if block.relation == BlockRelation::Focal {
                continue;
            }
            let [sx, sy, sw, sh] = cam.world_to_screen(block.world);
            if sx + sw < ax || sy + sh < ay || sx > ax + aw || sy > ay + ah || sw < 8.0 || sh < 8.0
            {
                continue;
            }
            // The edit-target card is always emphasized; in the background no
            // card shows a focus ring (the editor is primary).
            let is_edit_target = edit_block == Some(block.id);
            // While editing, ONLY the edit-target card shows a focus ring — the
            // freshly-spawned child card (`canvas.focused`) shouldn't also ring
            // (it gets its ring once you Esc back to Navigate). Otherwise the
            // navigation-focused card rings.
            let focused = is_edit_target
                || (!background && edit_block.is_none() && canvas.focused == Some(block.id));
            // The card being edited rings in the ACTIVE MODE's color (NORMAL/
            // INSERT/VISUAL); a merely navigation-focused card rings cyan; the
            // rest are dim. App-global modes (palette/terminal/…) fall back to
            // cyan focus because the card isn't really being edited in those.
            let ring_color = if is_edit_target {
                if card_mode.is_some() { mode_color } else { border_focus }
            } else if focused {
                border_focus
            } else {
                border_dim
            };
            let rel_color = match block.relation {
                BlockRelation::Definition => c_def,
                BlockRelation::Caller => c_caller,
                BlockRelation::Callee => c_callee,
                BlockRelation::Focal => border_focus,
            };

            // Shadow → border halo → card → header → divider → active underline.
            self.canvas_chrome_instances.push(
                RegionDrawInstance::new([sx + 4.0, sy + 7.0, sw, sh], shadow)
                    .with_radius(radius + 2.0),
            );
            // The edit-target card is lifted further off the plane than a merely
            // focused card, so it reads as the active mini-editor pane.
            let halo = if is_edit_target {
                5.0
            } else if focused {
                3.0
            } else {
                1.5
            };
            self.canvas_chrome_instances.push(
                RegionDrawInstance::new(
                    [sx - halo, sy - halo, sw + halo * 2.0, sh + halo * 2.0],
                    ring_color,
                )
                .with_radius(radius + halo),
            );
            self.canvas_chrome_instances
                .push(RegionDrawInstance::new([sx, sy, sw, sh], card_bg).with_radius(radius));
            self.canvas_chrome_instances.push(
                RegionDrawInstance::new([sx, sy, sw, title_h.min(sh)], header_bg).with_radius(radius),
            );
            self.canvas_chrome_instances.push(RegionDrawInstance::new(
                [sx, sy + title_h - 1.0, sw, 1.0],
                divider,
            ));
            self.canvas_chrome_instances.push(
                RegionDrawInstance::new([sx, sy, 3.0, title_h.min(sh)], rel_color).with_radius(0.0),
            );
            if focused {
                self.canvas_chrome_instances.push(RegionDrawInstance::new(
                    [sx, sy + title_h - 2.0, sw, 2.0],
                    ring_color,
                ));
            }

            self.canvas_text_system.set_size(None, None);
            let header_chars = ((sw - PAD * 2.0) / char_w).floor().max(1.0) as usize;
            let line_count = block.snapshot.text.split('\n').count();
            let end_line = block.snapshot.start_line + line_count.saturating_sub(1) as u32;
            let range = format!("{}-{}", block.snapshot.start_line, end_line);
            let header_y = sy + (title_h - line_height) * 0.5;
            // Breadcrumb-style header (like the editor's): filename bright, then a
            // dim "· symbol"; line-range right-aligned in the relation color.
            let icon_size = (line_height * 0.82).min(fs * 1.2);
            let icon_gap = CANVAS_TITLE_ICON_GAP;
            let icon_slot_w = icon_size + icon_gap;
            let title_for_icon = block
                .snapshot
                .title
                .rsplit(['/', '\\'])
                .next()
                .unwrap_or(&block.snapshot.title);
            let icon_id = canonical_icon_id(self.theme.get_icon_for_file(title_for_icon, false));
            let icon_slot_chars = (icon_slot_w / char_w).ceil().max(1.0) as usize;
            let head_budget = header_chars
                .saturating_sub(range.chars().count() + 2)
                .saturating_sub(icon_slot_chars)
                .max(1);
            let file = clip_line(&block.snapshot.title, head_budget);
            let file_n = file.chars().count();

            if let Some(icon) = icon_id {
                self.canvas_icon_instances.push(IconDrawInstance {
                    icon,
                    rect: [
                        sx + PAD + 4.0,
                        header_y + (line_height - icon_size) * 0.5,
                        icon_size,
                        icon_size,
                    ],
                    tint: [1.0, 1.0, 1.0, 1.0],
                });
            }

            let file_x = sx + PAD + 4.0 + icon_slot_w;
            self.canvas_glyph_instances.extend(layout_panel_text(
                &file,
                &mut self.canvas_text_system,
                &mut self.atlas,
                &self.queue,
                file_x,
                header_y,
                title_fg,
            ));
            let sym = &block.snapshot.symbol;
            if !sym.is_empty() && file_n + 3 < head_budget {
                let crumb = clip_line(&format!(" \u{00b7} {sym}"), head_budget - file_n);
                self.canvas_glyph_instances.extend(layout_panel_text(
                    &crumb,
                    &mut self.canvas_text_system,
                    &mut self.atlas,
                    &self.queue,
                    file_x + file_n as f32 * char_w,
                    header_y,
                    gutter_fg,
                ));
            }
            // Line range, right-aligned, in the relation color.
            let range_w = range.chars().count() as f32 * char_w;
            let range_x = (sx + sw - PAD - range_w).max(sx + PAD);
            self.canvas_glyph_instances.extend(layout_panel_text(
                &range,
                &mut self.canvas_text_system,
                &mut self.atlas,
                &self.queue,
                range_x,
                header_y,
                rel_color,
            ));
            // Mode badge (edit-target card only): the active mode label (NORMAL/
            // INSERT/VISUAL) in the mode color, just left of the line-range — the
            // card's own little status line. Hidden for app-global modes
            // (palette/terminal/multi-cursor/resize) since the card isn't being
            // edited in those. Skipped if the card is too narrow to fit it
            // without colliding with the filename breadcrumb.
            if is_edit_target {
                if let Some(label) = card_mode {
                    let label_w = label.chars().count() as f32 * char_w;
                    let label_x = range_x - char_w * 1.4 - label_w;
                    if label_x > sx + PAD + char_w {
                        self.canvas_glyph_instances.extend(layout_panel_text(
                            label,
                            &mut self.canvas_text_system,
                            &mut self.atlas,
                            &self.queue,
                            label_x,
                            header_y,
                            mode_color,
                        ));
                    }
                }
            }
            // Pin indicator: a small amber dot left of the range when pinned.
            if block.pinned {
                let dot = line_height * 0.34;
                let dot_x = (sx + sw - PAD - range_w - char_w * 1.4 - dot).max(sx + PAD);
                let dot_y = sy + (title_h - dot) * 0.5;
                self.canvas_chrome_instances.push(
                    RegionDrawInstance::new([dot_x, dot_y, dot, dot], pin_color)
                        .with_radius(dot * 0.5),
                );
            }

            // ── Body: gutter + per-line syntax-highlighted code ──────────────
            // Hard-cap the line count to what fits inside [body_top, body_bottom]
            // so code can NEVER spill past the card border, regardless of how
            // many lines the snapshot carries (cosmic-text doesn't clip glyphs
            // vertically and the scissor is the whole editor region, so a strict
            // capacity bound is the only guarantee).
            let code_x = sx + PAD + gutter_w;
            let body_top = sy + title_h + 4.0;
            // Clamp the body's bottom to the visible region so a card taller than
            // the viewport shows a "+N more" marker instead of letting rows fall
            // silently off-screen (the canvas shares one scissor — there's no
            // per-card vertical clip, so capacity must respect the viewport).
            let body_bottom = (sy + sh - 4.0).min(ay + ah - 4.0);
            // Hard horizontal clip: the char-count truncation uses an *estimated*
            // advance (`fs*0.6`) which under-counts the real font, so long lines
            // can still spill past the right border. Drop any glyph crossing this
            // edge (the canvas shares one scissor, so there's no per-card clip).
            let clip_right = sx + sw - PAD;
            let visible_px = (clip_right - code_x).max(char_w);
            let max_code_chars = (visible_px / char_w).floor().max(1.0) as usize;
            let total_lines = block.snapshot.text.split('\n').count();
            let capacity = ((body_bottom - body_top) / line_height).floor().max(0.0) as usize;
            // Horizontal scroll (edit-target card only): when the live caret runs
            // past the card's right edge, shift ALL code lines left so the cursor
            // stays visible — exactly like the main editor, so `h`/`l` on a long
            // line no longer make the text vanish off the right. Stateless: derived
            // from the caret column each frame, keeping ~6 cols of look-ahead.
            let h_scroll_px = if is_edit_target {
                edit_cursor
                    .map(|(cl, cc)| {
                        let row = cl as i64 - (block.snapshot.start_line as i64 - 1);
                        let cur_line = if row >= 0 {
                            block.snapshot.text.split('\n').nth(row as usize).unwrap_or("")
                        } else {
                            ""
                        };
                        let mut v = 0usize;
                        for ch in cur_line.chars().take(cc as usize) {
                            v += if ch == '\t' { CARD_TAB_WIDTH - (v % CARD_TAB_WIDTH) } else { 1 };
                        }
                        let cursor_vx = v as f32 * char_w;
                        (cursor_vx - (visible_px - char_w * 6.0)).max(0.0)
                    })
                    .unwrap_or(0.0)
            } else {
                0.0
            };
            // Code origin shifted by the horizontal scroll; glyphs left of `code_x`
            // and right of `clip_right` are clipped out below.
            let code_x0 = code_x - h_scroll_px;
            let h_scroll_cols = (h_scroll_px / char_w).round() as usize;
            // Editor-style active-line highlight on the focal line (where the
            // symbol sits), so the card reads like a mini main editor. For the
            // edit-target card the band follows the LIVE caret (the transient
            // edit cursor) — `origin.lsp_line` stays pinned to the source site.
            let band_line = if is_edit_target {
                edit_cursor.map(|(cl, _)| cl).unwrap_or(block.origin.lsp_line)
            } else {
                block.origin.lsp_line
            };
            let focal_local = band_line as i64 - (block.snapshot.start_line as i64 - 1);
            // Subtle current-line highlight, EXACTLY like the main editor: the
            // selection color at low alpha — not the green/relation tint it used to
            // be. The card is a mini main editor, so its active line reads the same.
            let active_band = {
                let mut c = self.theme.editor.selection.as_f32();
                c[3] = (c[3] * 0.22).clamp(0.10, 0.30);
                c
            };
            // Visual-mode selection band (edit-target card only): the session's
            // live selection mirrored onto the canvas, painted like the main
            // editor's selection so in-card `v`/`V` highlights match.
            let selection = if is_edit_target {
                canvas.edit_selection
            } else {
                None
            };
            let sel_band = {
                let mut c = self.theme.editor.selection.as_f32();
                c[3] = c[3].clamp(0.32, 0.55);
                c
            };
            let mut byte = 0usize;
            for (i, raw_line) in block.snapshot.text.split('\n').enumerate() {
                let line_byte_start = byte;
                byte += raw_line.len() + 1; // +1 for the '\n'
                if i >= capacity {
                    break;
                }
                let line_y = body_top + i as f32 * line_height;
                // The last visible row becomes a "⋯ +N more" marker when the
                // snapshot has more lines than the card can show.
                let truncated = i + 1 == capacity && total_lines > capacity;
                let is_focal = !truncated && i as i64 == focal_local;
                if is_focal {
                    self.canvas_chrome_instances.push(RegionDrawInstance::new(
                        [sx + 2.0, line_y, sw - 4.0, line_height],
                        active_band,
                    ));
                }
                // Selection band for this row (drawn as chrome, so code glyphs sit
                // on top). Absolute 0-based file line = (start_line-1) + i.
                let abs_line = (block.snapshot.start_line as usize).saturating_sub(1) + i;
                if let Some(sel) = selection
                    && !truncated
                    && abs_line >= sel.start_line as usize
                    && abs_line <= sel.end_line as usize
                {
                    let nchars = raw_line.chars().count();
                    let (sc, ec) = if sel.line_mode {
                        (0usize, nchars)
                    } else {
                        let sc = if abs_line == sel.start_line as usize {
                            sel.start_col as usize
                        } else {
                            0
                        };
                        let ec = if abs_line == sel.end_line as usize {
                            (sel.end_col as usize).min(nchars)
                        } else {
                            nchars
                        };
                        (sc, ec)
                    };
                    // Tab-aware visual column → x, matching the caret/gutter math.
                    let visx = |upto: usize| -> f32 {
                        let mut v = 0usize;
                        for ch in raw_line.chars().take(upto) {
                            v += if ch == '\t' {
                                CARD_TAB_WIDTH - (v % CARD_TAB_WIDTH)
                            } else {
                                1
                            };
                        }
                        v as f32 * char_w
                    };
                    let x0 = (code_x0 + visx(sc)).clamp(code_x, clip_right);
                    let x1 = if sel.line_mode {
                        clip_right
                    } else {
                        (code_x0 + visx(ec)).clamp(code_x, clip_right)
                    };
                    // Empty line / zero-width (e.g. selected newline) → a thin sliver
                    // so the row still reads as selected.
                    let w = (x1 - x0).max(char_w * 0.5);
                    self.canvas_chrome_instances.push(RegionDrawInstance::new(
                        [x0, line_y, w.min(clip_right - x0).max(0.0), line_height],
                        sel_band,
                    ));
                }
                let ln = block.snapshot.start_line as usize + i;
                let num = format!("{ln:>4}");
                self.canvas_glyph_instances.extend(layout_panel_text(
                    if truncated { "    " } else { &num },
                    &mut self.canvas_text_system,
                    &mut self.atlas,
                    &self.queue,
                    sx + PAD,
                    line_y,
                    if is_focal { title_fg } else { gutter_fg },
                ));
                if truncated {
                    let more = total_lines - capacity + 1;
                    let marker = format!("\u{22ef} +{more} more");
                    self.canvas_glyph_instances.extend(layout_panel_text(
                        &clip_line(&marker, max_code_chars),
                        &mut self.canvas_text_system,
                        &mut self.atlas,
                        &self.queue,
                        code_x,
                        line_y,
                        gutter_fg,
                    ));
                    break;
                }
                // Code drawn from the scrolled origin (`code_x0`), taking enough
                // chars to cover the scrolled-in window, with rebased syntax spans.
                // Glyphs are clipped to BOTH edges so scrolled-off text disappears
                // cleanly on the left and right.
                let take_chars = h_scroll_cols + max_code_chars + 2;
                let clipped: String = raw_line.chars().take(take_chars).collect();
                if clipped.is_empty() {
                    continue;
                }
                let styled = line_styled_spans(&block.snapshot.spans, line_byte_start, clipped.len());
                self.canvas_glyph_instances.extend(
                    layout_panel_rich_text(
                        &clipped,
                        &styled,
                        fg,
                        &mut self.canvas_text_system,
                        &mut self.atlas,
                        &self.queue,
                        code_x0,
                        line_y,
                    )
                    .into_iter()
                    .filter(|g| {
                        g.screen_pos[0] + g.glyph_size[0] <= clip_right
                            && g.screen_pos[0] >= code_x - 0.5
                    }),
                );
            }

            // ── Edit caret (S2): a thin vertical bar at the live cursor on the
            //    edit-target card. The cursor row equals the active-line band
            //    (origin.lsp_line tracks the cursor line while editing), so the
            //    caret rides the highlighted line.
            if is_edit_target
                && let Some((cl, cc)) = edit_cursor
            {
                let row = cl as i64 - (block.snapshot.start_line as i64 - 1);
                if row >= 0 && (row as usize) < capacity {
                    let line = block.snapshot.text.split('\n').nth(row as usize).unwrap_or("");
                    // Exact caret X: shape the cursor prefix with the SAME text
                    // system + tab width the body uses, so the caret lands on the
                    // glyph under the cursor (handles tabs / proportional drift).
                    let prefix: String = line.chars().take(cc as usize).collect();
                    let fallback = || {
                        let mut v = 0usize;
                        for ch in prefix.chars() {
                            v += if ch == '\t' {
                                CARD_TAB_WIDTH - (v % CARD_TAB_WIDTH)
                            } else {
                                1
                            };
                        }
                        v as f32 * char_w
                    };
                    self.canvas_text_system.set_size(None, None);
                    self.canvas_text_system.set_text(&prefix);
                    let prefix_w = self
                        .canvas_text_system
                        .buffer()
                        .layout_runs()
                        .next()
                        .map(|r| r.line_w)
                        .filter(|w| *w > 0.0 || prefix.is_empty())
                        .unwrap_or_else(fallback);
                    let caret_y = body_top + row as f32 * line_height;
                    // Apply the same horizontal scroll so the caret rides the glyph
                    // under the cursor even on long, scrolled lines.
                    let caret_x = (code_x0 + prefix_w).clamp(code_x, clip_right - 1.0);
                    // Block cursor (Normal/Visual) sits on the whole cell, drawn
                    // BEHIND the glyph (chrome renders under the text pipeline) so
                    // the character shows through; a thin beam in Insert.
                    let (caret_w, caret_a) = if caret_block {
                        (char_w.max(2.0), 0.5)
                    } else {
                        ((char_w * 0.14).max(2.0), 0.9)
                    };
                    let cc_col = [caret_color[0], caret_color[1], caret_color[2], caret_a];
                    self.canvas_chrome_instances.push(RegionDrawInstance::new(
                        [caret_x, caret_y, caret_w, line_height],
                        cc_col,
                    ));
                    // Remember where to float the hover/completion popup.
                    overlay_anchor = Some([caret_x, caret_y + line_height, sx, sw]);
                }
            }
        }

        // ── Empty / loading state: when no relation cards exist yet, a bare hint
        //    bar over an empty plane reads as broken. Show a centered panel — a
        //    "searching…" line while a `gc` request is in flight, or a "nothing to
        //    show" hint once it resolved with no results. Hidden in the background
        //    (the editor is primary there).
        let relation_count = canvas
            .blocks
            .iter()
            .filter(|b| b.relation != crate::canvas::BlockRelation::Focal)
            .count();
        if !background && relation_count == 0 {
            let (title, subtitle, accent) = if pending {
                (
                    "Loading source function…",
                    "Resolving via LSP",
                    self.theme.ui.info.as_f32(),
                )
            } else {
                (
                    "Nothing to show",
                    "Put the cursor on a symbol, then press F8",
                    self.theme.ui.fg_dim.as_f32(),
                )
            };
            let t_fs = (ah * 0.022).clamp(15.0, 26.0);
            let s_fs = (t_fs * 0.72).max(12.0);
            let row_gap = t_fs * 0.55;
            self.canvas_text_system.set_metrics(Metrics::new(t_fs, t_fs * 1.3));
            self.canvas_text_system.set_size(None, None);
            let t_w = estimate_monospace_width(title, t_fs);
            let s_w = estimate_monospace_width(subtitle, s_fs);
            let inner_w = t_w.max(s_w);
            let box_pad = t_fs * 1.4;
            let box_w = (inner_w + box_pad * 2.0).min(aw - 40.0);
            let box_h = t_fs * 1.3 + row_gap + s_fs * 1.3 + box_pad * 1.6;
            let box_x = ax + (aw - box_w) * 0.5;
            let box_y = ay + (ah - box_h) * 0.5;
            // A dim "accent dot" + soft panel so the message reads as intentional.
            self.canvas_chrome_instances.push(
                RegionDrawInstance::new(
                    [box_x - 1.0, box_y - 1.0, box_w + 2.0, box_h + 2.0],
                    divider,
                )
                .with_radius(13.0),
            );
            self.canvas_chrome_instances.push(
                RegionDrawInstance::new([box_x, box_y, box_w, box_h], header_bg).with_radius(12.0),
            );
            let dot = t_fs * 0.34;
            self.canvas_chrome_instances.push(
                RegionDrawInstance::new(
                    [box_x + box_pad, box_y + box_pad * 0.8 + (t_fs - dot) * 0.5, dot, dot],
                    accent,
                )
                .with_radius(dot * 0.5),
            );
            let t_x = box_x + (box_w - t_w) * 0.5;
            let t_y = box_y + box_pad * 0.8;
            self.canvas_glyph_instances.extend(layout_panel_text(
                title,
                &mut self.canvas_text_system,
                &mut self.atlas,
                &self.queue,
                t_x,
                t_y,
                title_fg,
            ));
            self.canvas_text_system.set_metrics(Metrics::new(s_fs, s_fs * 1.3));
            let s_x = box_x + (box_w - s_w) * 0.5;
            let s_y = t_y + t_fs * 1.3 + row_gap;
            self.canvas_glyph_instances.extend(layout_panel_text(
                subtitle,
                &mut self.canvas_text_system,
                &mut self.atlas,
                &self.queue,
                s_x,
                s_y,
                gutter_fg,
            ));
        }

        // ── Chrome: a centered bottom hint bar (hidden in the background state,
        //    where the editor is primary). Reuse the shared IDE keycap component
        //    so the hints read like the which-key / palette footer.
        if !background {
            let key_fs = (ah * 0.017).clamp(15.0, 24.0);
            let key_lh = key_fs * 1.4;
            self.canvas_text_system
                .set_metrics(Metrics::new(key_fs, key_lh));
            self.canvas_text_system.set_size(None, None);

            let hint_fg = self.theme.ui.fg.as_f32();
            let hint_dim = self.theme.ui.fg_dim.as_f32();
            let kc_accent = self.theme.ui.accent.as_f32();
            let kc_info = self.theme.ui.info.as_f32();
            let kc_warning = self.theme.ui.warning.as_f32();
            let kc_error = self.theme.ui.error.as_f32();
            // While editing a card the keys mean editor things (hjkl = cursor,
            // ⌘S = save); otherwise they drive card navigation.
            let nav_hints: [(&[&str], &str); 8] = [
                (&["gd"], "Def"),
                (&["gr"], "Refs"),
                (&["Enter"], "Edit"),
                (&["hjkl"], "Move"),
                (&["P"], "Pin"),
                (&["Spc", "x"], "Close"),
                (&["Tab"], "Next"),
                (&["Esc"], "Back"),
            ];
            let edit_hints: [(&[&str], &str); 4] = [
                (&["Esc"], "Done"),
                (&["hjkl"], "Cursor"),
                (&["i"], "Insert"),
                (&["\u{2318}S"], "Save"),
            ];
            let hints: &[(&[&str], &str)] = if edit_block.is_some() {
                &edit_hints
            } else {
                &nav_hints
            };
            let label_gap = (key_fs * 0.55).max(7.0);
            let entry_gap = (key_fs * 1.7).max(20.0);
            let mut total = 0.0f32;
            for &(keys, label) in hints {
                total += estimate_help_keycaps_width(keys, key_fs)
                    + label_gap
                    + estimate_monospace_width(label, key_fs)
                    + entry_gap;
            }
            total -= entry_gap; // drop the trailing gap
            let bar_pad = key_fs * 1.3;
            let row_h = key_fs + 12.0 * (key_fs / 14.0).max(0.5) + 10.0;
            let bar_h = row_h + 10.0;
            let bar_w = (total + bar_pad * 2.0).min(aw - 24.0);
            let bar_y = ay + ah - bar_h - 16.0;
            let bar_x = ax + (aw - bar_w) * 0.5;
            self.canvas_chrome_instances.push(
                RegionDrawInstance::new(
                    [bar_x - 1.0, bar_y - 1.0, bar_w + 2.0, bar_h + 2.0],
                    divider,
                )
                .with_radius(12.0),
            );
            self.canvas_chrome_instances.push(
                RegionDrawInstance::new([bar_x, bar_y, bar_w, bar_h], header_bg).with_radius(11.0),
            );
            let row_y = bar_y + (bar_h - row_h) * 0.5;
            let label_ty = row_y + (row_h - key_lh) * 0.5;
            let mut x = bar_x + bar_pad;
            for &(keys, label) in hints {
                let kg = layout_help_keycaps(
                    keys,
                    &mut self.canvas_text_system,
                    &mut self.atlas,
                    &self.queue,
                    &mut self.canvas_chrome_instances,
                    x,
                    row_y,
                    key_fs,
                    row_h,
                    hint_fg,
                    hint_dim,
                    kc_accent,
                    kc_info,
                    kc_warning,
                    kc_error,
                    header_bg,
                );
                self.canvas_glyph_instances.extend(kg);
                x += estimate_help_keycaps_width(keys, key_fs) + label_gap;
                self.canvas_glyph_instances.extend(layout_panel_text(
                    label,
                    &mut self.canvas_text_system,
                    &mut self.atlas,
                    &self.queue,
                    x,
                    label_ty,
                    hint_dim,
                ));
                x += estimate_monospace_width(label, key_fs) + entry_gap;
            }
        }

        // ── In-card LSP popups (Phase 2/3), drawn LAST (above the cards + hint bar)
        //    at the edit caret. Hover = a doc box; completion = the SAME
        //    `draw_completion_menu` the main editor uses (shared single-column UI).
        //    The canvas draws ALL chrome then ALL text in one pass, so an opaque
        //    popup background can't occlude code text added earlier — after drawing
        //    a popup we DROP the card-code glyphs under it so only the popup shows.
        let popup_glyph_start = self.canvas_glyph_instances.len();
        let mut popup_rect: Option<[f32; 4]> = None;
        if let (Some(anchor), Some(lines)) = (overlay_anchor, canvas.card_hover.as_ref()) {
            popup_rect = self.draw_card_hover(lines, anchor, fs, line_height, char_w, [ax, ay, aw, ah]);
        }
        if let (Some([caret_x, below_y, _, _]), Some(completion)) = (overlay_anchor, card_completion)
        {
            use super::editor::completion::{
                CompletionMenuGeom, MenuRenderTargets, draw_completion_menu,
            };
            popup_rect = draw_completion_menu(
                completion,
                CompletionMenuGeom {
                    anchor_x: caret_x,
                    caret_top: below_y - line_height,
                    caret_bottom: below_y,
                    bounds: [ax, ay, aw, ah],
                    font_size: fs,
                    line_height,
                    ui_scale: cam.zoom.max(0.5),
                },
                &self.theme,
                MenuRenderTargets {
                    text_system: &mut self.canvas_text_system,
                    atlas: &mut self.atlas,
                    queue: &self.queue,
                    chrome: &mut self.canvas_chrome_instances,
                    glyphs: &mut self.canvas_glyph_instances,
                    icons: &mut self.canvas_icon_instances,
                },
            );
        }
        if let Some([px, py, pw, ph]) = popup_rect {
            // Keep the popup's own glyphs (added after `popup_glyph_start`); drop any
            // earlier glyph whose rect overlaps the popup so the opaque bg reads clean.
            let popup_glyphs = self.canvas_glyph_instances.split_off(popup_glyph_start);
            self.canvas_glyph_instances.retain(|g| {
                let gx = g.screen_pos[0];
                let gy = g.screen_pos[1];
                !(gx + g.glyph_size[0] > px
                    && gx < px + pw
                    && gy + g.glyph_size[1] > py
                    && gy < py + ph)
            });
            self.canvas_glyph_instances.extend(popup_glyphs);
        }

        self.canvas_text_pipeline
            .upload_instances(&self.device, &self.queue, &self.canvas_glyph_instances);
        self.canvas_icon_pipeline.upload_instances(
            &self.device,
            &self.canvas_icon_instances,
            [self.surface_state.config.width, self.surface_state.config.height],
        );
    }

    /// Build the `K` hover doc box at the in-card edit caret. The anchor is
    /// `[caret_x, caret_bottom_y, card_x, card_w]`; placed below the caret and
    /// flipped above when it would fall off the bottom. (Completion uses the shared
    /// `draw_completion_menu` instead — same UI as the main editor.)
    fn draw_card_hover(
        &mut self,
        lines: &[String],
        anchor: [f32; 4],
        fs: f32,
        line_height: f32,
        char_w: f32,
        region: [f32; 4],
    ) -> Option<[f32; 4]> {
        let [caret_x, below_y, _card_x, _card_w] = anchor;
        let [ax, ay, aw, ah] = region;
        let editor_bg = self.theme.editor.bg.as_f32();
        let fg = self.theme.editor.fg.as_f32();
        let pad = 8.0;
        let radius = 7.0;
        let mut popup_bg = blend(editor_bg, fg, 0.10);
        popup_bg[3] = 1.0;
        let popup_border = blend(editor_bg, fg, 0.40);
        // The bottom hint bar ran before this and changed the text metrics — reset
        // to the card font so the popup text matches the code size.
        self.canvas_text_system
            .set_metrics(Metrics::new(fs, line_height));
        self.canvas_text_system.set_tab_width(CARD_TAB_WIDTH as u16);
        self.canvas_text_system.set_size(None, None);

        let max_rows = 14usize;
        let max_cols = 64usize;
        let rows: Vec<String> = lines
            .iter()
            .take(max_rows)
            .map(|l| clip_line(l, max_cols))
            .collect();
        if rows.is_empty() {
            return None;
        }
        let widest = rows.iter().map(|t| t.chars().count()).max().unwrap_or(0).max(8);
        let box_w = (widest as f32 * char_w + pad * 2.0).min(aw * 0.7).max(60.0);
        let box_h = rows.len() as f32 * line_height + pad * 2.0;

        let mut box_y = below_y + 2.0;
        if box_y + box_h > ay + ah - 4.0 {
            box_y = (below_y - line_height - box_h - 2.0).max(ay + 4.0);
        }
        let box_x = caret_x.min(ax + aw - box_w - 4.0).max(ax + 4.0);

        self.canvas_chrome_instances.push(
            RegionDrawInstance::new(
                [box_x - 1.0, box_y - 1.0, box_w + 2.0, box_h + 2.0],
                popup_border,
            )
            .with_radius(radius + 1.0),
        );
        self.canvas_chrome_instances
            .push(RegionDrawInstance::new([box_x, box_y, box_w, box_h], popup_bg).with_radius(radius));

        let text_x = box_x + pad;
        for (i, text) in rows.iter().enumerate() {
            let row_y = box_y + pad + i as f32 * line_height;
            self.canvas_glyph_instances.extend(layout_panel_text(
                text,
                &mut self.canvas_text_system,
                &mut self.atlas,
                &self.queue,
                text_x,
                row_y,
                fg,
            ));
        }
        Some([box_x, box_y, box_w, box_h])
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

/// Spans for one line: keep those overlapping the line, rebase to line-local
/// byte offsets, and clamp to the (possibly width-truncated) `clipped_len`.
fn line_styled_spans(
    spans: &[CanvasSpan],
    line_byte_start: usize,
    clipped_len: usize,
) -> Vec<StyledTextSpan> {
    let mut out = Vec::new();
    for s in spans {
        if s.end <= line_byte_start || s.start >= line_byte_start + clipped_len {
            continue;
        }
        let st = s.start.saturating_sub(line_byte_start).min(clipped_len);
        let en = (s.end - line_byte_start).min(clipped_len);
        if st >= en {
            continue;
        }
        out.push(StyledTextSpan {
            start: st,
            end: en,
            color_rgba: s.color,
            bold: s.bold,
            italic: s.italic,
            underline: false,
            strikethrough: false,
        });
    }
    out
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
    use super::*;

    #[test]
    fn clip_line_truncates_with_ellipsis() {
        assert_eq!(clip_line("hello", 10), "hello");
        assert_eq!(clip_line("hello world", 5), "hell…");
        assert_eq!(clip_line("a\nb", 10), "a");
    }

    #[test]
    fn line_styled_spans_rebases_and_clamps() {
        // One span [12,18) on a line starting at byte 10, clipped to 6 bytes.
        let spans = [CanvasSpan {
            start: 12,
            end: 18,
            color: [1, 2, 3, 255],
            bold: false,
            italic: false,
        }];
        let out = line_styled_spans(&spans, 10, 6);
        assert_eq!(out.len(), 1);
        assert_eq!((out[0].start, out[0].end), (2, 6)); // rebased + clamped
    }

    #[test]
    fn line_styled_spans_drops_out_of_range() {
        let spans = [CanvasSpan {
            start: 100,
            end: 110,
            color: [0, 0, 0, 255],
            bold: false,
            italic: false,
        }];
        assert!(line_styled_spans(&spans, 0, 20).is_empty());
    }
}
