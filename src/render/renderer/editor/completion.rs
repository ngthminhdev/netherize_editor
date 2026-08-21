#![allow(unused_imports)]

use crate::{
    app::app_state::{
        AppState, CompletionDisplayItem, DiagnosticsState, EditorOverlay, FloatingBoxBlock,
        FloatingBoxStyle, HelpState, OverlayColorToken, ReferencesBufferState, SettingItem,
        SettingsState,
    },
    async_runtime::message::LspDiagnostic,
    config::theme_config::ThemeConfig,
    core::mode::EditorMode,
    render::{
        glyph_instance::GlyphInstance, region_pipeline::RegionDrawInstance, renderer::Renderer,
    },
    text::layout_sync::{compute_caret_layout, compute_cursor_overlay, rebuild_layout_projection},
};
use cosmic_text::Metrics;

use super::super::helpers::{
    caret_rect_for_mode, clamp_monospace_text, clamp_monospace_text_left, clamp_popup_width,
    estimate_monospace_width, gutter_width_for_editor, layout_panel_rich_text, layout_panel_text,
    layout_panel_text_italic, rect_to_scissor, should_draw_block_cursor,
};
use super::{cursor_diagnostic, editor_viewport_geometry, run_x_for_byte, wrap_text_lines};
use crate::app::app_state::CompletionState;
use crate::render::icon_pipeline::{IconDrawInstance, canonical_icon_id};
use crate::text::atlas::GlyphAtlas;
use crate::text::text_system::{StyledTextSpan, TextSystem};

/// Where to place the completion menu + the metrics to draw it at. `anchor_x` is
/// the menu's intended left edge (already prefix-adjusted); the menu drops below
/// `caret_bottom`, flipping above `caret_top` when it would overflow the bottom of
/// `bounds` (`[x, y, w, h]`, the clamp region).
pub(crate) struct CompletionMenuGeom {
    pub anchor_x: f32,
    pub caret_top: f32,
    pub caret_bottom: f32,
    pub bounds: [f32; 4],
    pub font_size: f32,
    pub line_height: f32,
    pub ui_scale: f32,
}

/// Disjoint render-target borrows so the SAME single-column menu renders into
/// either the editor-overlay buffers or the canvas buffers. Built at the call
/// site from distinct `Renderer` fields (the editor passes its overlay text
/// system + local quad/glyph/icon vecs; the canvas passes its own).
pub(crate) struct MenuRenderTargets<'a> {
    pub text_system: &'a mut TextSystem,
    pub atlas: &'a mut GlyphAtlas,
    pub queue: &'a wgpu::Queue,
    pub chrome: &'a mut Vec<RegionDrawInstance>,
    pub glyphs: &'a mut Vec<GlyphInstance>,
    pub icons: &'a mut Vec<IconDrawInstance>,
}

/// Single-column completion menu (kind badge + label with match highlight +
/// optional dim inline detail + selection + scrollbar). Compact: no doc panel, no
/// footer. Shared by the main editor and the NetherCanvas in-card editor so both
/// look identical. `anchor`/buffers are passed in so it is position- and
/// surface-agnostic.
pub(crate) fn draw_completion_menu(
    completion: &CompletionState,
    geom: CompletionMenuGeom,
    theme: &ThemeConfig,
    t: MenuRenderTargets<'_>,
) -> Option<[f32; 4]> {
    if completion.filtered_items.is_empty() {
        return None;
    }
    let blend = |base: [f32; 4], tint: [f32; 4], amt: f32| -> [f32; 4] {
        [
            base[0] + (tint[0] - base[0]) * amt,
            base[1] + (tint[1] - base[1]) * amt,
            base[2] + (tint[2] - base[2]) * amt,
            1.0,
        ]
    };

    const MAX_VISIBLE_ROWS: usize = 10;
    let ui_s = geom.ui_scale;
    let pad_x = 12.0 * ui_s;
    let pad_y = 6.0 * ui_s;
    let badge_gap = 10.0 * ui_s;
    let scrollbar_w = 4.0 * ui_s;
    let char_w = (geom.font_size * 0.6).max(1.0);
    let row_h = (geom.line_height + 12.0 * ui_s).max(24.0 * ui_s);
    let label_line_h = geom.line_height;
    let text_v_center = (row_h - label_line_h) * 0.5;
    let badge_size = (row_h * 0.72).clamp(16.0 * ui_s, 26.0 * ui_s);
    let badge_radius = badge_size * 0.28;
    let badge_col_w = pad_x + badge_size + badge_gap;
    let detail_font_size = (geom.font_size * 0.92).max(9.0);

    let total = completion.filtered_items.len();
    let visible_rows = total.clamp(1, MAX_VISIBLE_ROWS);
    let rows_h = visible_rows as f32 * row_h;
    let footer_h = (geom.line_height * 0.9 + 10.0 * ui_s).max(20.0 * ui_s);
    let popup_h = rows_h + pad_y * 2.0 + footer_h;

    // Content-based width: the longest visible label (capped so one absurd
    // label can't blow the panel up) plus a share for the dim detail column.
    let max_label_chars = completion
        .filtered_items
        .iter()
        .take(40)
        .map(|i| i.item.label.chars().count())
        .max()
        .unwrap_or(8)
        .min(48);
    let has_details = completion.filtered_items.iter().take(40).any(|i| {
        i.item
            .detail
            .as_deref()
            .is_some_and(|d| !d.trim().is_empty())
    });
    let detail_share = if has_details {
        26.0 * char_w
    } else {
        9.0 * char_w
    };
    let desired_w =
        badge_col_w + max_label_chars as f32 * char_w + detail_share + pad_x * 2.0 + scrollbar_w;
    let [bx, by, bw, bh] = geom.bounds;
    let min_w = (300.0 * ui_s).min((bw - 16.0).max(1.0));
    let max_w = ((bw * 0.55).min(680.0 * ui_s)).max(min_w);
    let list_w = clamp_popup_width(desired_w, min_w, max_w);
    let popup_w = list_w;

    let viewport_right = bx + bw;
    let viewport_bottom = by + bh;
    let mut popup_x = geom.anchor_x;
    if popup_x + popup_w > viewport_right - 8.0 {
        popup_x = (viewport_right - 8.0 - popup_w).max(bx + 4.0);
    }
    popup_x = popup_x.max(bx + 4.0);
    let mut popup_y = geom.caret_bottom + 4.0;
    if popup_y + popup_h > viewport_bottom - 8.0 {
        popup_y = (geom.caret_top - popup_h - 4.0).max(by + 4.0);
    }

    // Opaque background so the menu fully covers whatever is behind it (in the
    // canvas, code text under the menu is dropped by the caller; the opaque fill
    // also hides the editor/cards underneath).
    let mut bg = theme.ui.panel_bg.as_f32();
    bg[3] = 1.0;
    let border = theme.ui.border_color.as_f32();
    let sel_bg = theme.ui.selection_bg.as_f32();
    let fg = theme.ui.fg.as_f32();
    let fg_dim = theme.ui.fg_dim.as_f32();
    let cyan = theme.ui.cyan.as_f32();
    let panel_radius = 8.0 * ui_s;

    // Soft drop shadow → border → background, all rounded (the old square
    // panel + offset black slab was the "méo méo" look).
    t.chrome.push(
        RegionDrawInstance::new(
            [
                popup_x - 2.0 * ui_s,
                popup_y + 3.0 * ui_s,
                popup_w + 4.0 * ui_s,
                popup_h + 4.0 * ui_s,
            ],
            [0.0, 0.0, 0.0, 0.30],
        )
        .with_radius(panel_radius + 2.0 * ui_s),
    );
    t.chrome.push(
        RegionDrawInstance::new([popup_x, popup_y, popup_w, popup_h], border)
            .with_radius(panel_radius),
    );
    t.chrome.push(
        RegionDrawInstance::new(
            [popup_x + 1.0, popup_y + 1.0, popup_w - 2.0, popup_h - 2.0],
            bg,
        )
        .with_radius((panel_radius - 1.0).max(1.0)),
    );

    // Keep the selected row on screen.
    let scroll_start = if total <= visible_rows {
        0
    } else if completion.selected_index + 1 > visible_rows {
        completion.selected_index + 1 - visible_rows
    } else {
        0
    };

    for (row_idx, (item_idx, item)) in completion
        .filtered_items
        .iter()
        .enumerate()
        .skip(scroll_start)
        .take(visible_rows)
        .enumerate()
    {
        let is_selected = item_idx == completion.selected_index;
        let row_y = popup_y + pad_y + row_idx as f32 * row_h;
        if is_selected {
            t.chrome.push(
                RegionDrawInstance::new(
                    [
                        popup_x + 4.0 * ui_s,
                        row_y + 1.0,
                        list_w - 8.0 * ui_s,
                        row_h - 2.0,
                    ],
                    sel_bg,
                )
                .with_radius(5.0 * ui_s),
            );
        }

        // Kind badge (rounded chip + icon).
        let badge = completion_kind_badge(item.item.kind, theme);
        let icon_tint = blend(bg, badge.color, 0.82);
        let kind_bg = blend(bg, badge.color, 0.08);
        let kind_border = blend(bg, badge.color, 0.55);
        let badge_x = popup_x + pad_x;
        let badge_y = row_y + (row_h - badge_size) * 0.5;
        let bt = (badge_size * 0.07).clamp(1.0, 2.0);
        t.chrome.push(
            RegionDrawInstance::new([badge_x, badge_y, badge_size, badge_size], kind_border)
                .with_radius(badge_radius),
        );
        t.chrome.push(
            RegionDrawInstance::new(
                [
                    badge_x + bt,
                    badge_y + bt,
                    (badge_size - bt * 2.0).max(1.0),
                    (badge_size - bt * 2.0).max(1.0),
                ],
                kind_bg,
            )
            .with_radius((badge_radius - bt).max(1.0)),
        );
        if let Some(icon) = canonical_icon_id(badge.icon) {
            let icon_size = (badge_size * 0.68).max(10.0);
            t.icons.push(IconDrawInstance {
                icon,
                rect: [
                    badge_x + (badge_size - icon_size) * 0.5,
                    badge_y + (badge_size - icon_size) * 0.5,
                    icon_size,
                    icon_size,
                ],
                tint: icon_tint,
            });
        }

        // Label (match-highlighted) + dim right-aligned "(kind) detail" column
        // (the VS Code layout in the reference).
        let label_x = popup_x + badge_col_w;
        let kind_name = completion_kind_name(item.item.kind);
        let raw_detail = item
            .item
            .detail
            .as_deref()
            .map(str::trim)
            .filter(|d| !d.is_empty());
        let inline_full = match raw_detail {
            Some(detail) => format!("({kind_name}) {detail}"),
            None => format!("({kind_name})"),
        };
        let inline_detail = clamp_completion_detail(
            &inline_full,
            (list_w * 0.42).max(char_w * 10.0),
            detail_font_size,
        );
        let inline_w = estimate_monospace_width(&inline_detail, detail_font_size);
        let label_max_w =
            (list_w - badge_col_w - pad_x * 2.0 - scrollbar_w - inline_w).max(char_w * 4.0);
        let spans = completion_label_spans(item, cyan);
        let label_color = if is_selected { fg } else { fg_dim };
        t.text_system
            .set_metrics(Metrics::new(geom.font_size, label_line_h));
        t.text_system
            .set_size(Some(label_max_w), Some(label_line_h));
        t.glyphs.extend(layout_panel_rich_text(
            &item.item.label,
            &spans,
            label_color,
            t.text_system,
            t.atlas,
            t.queue,
            label_x,
            row_y + text_v_center,
        ));
        let detail_x = popup_x + list_w - pad_x - scrollbar_w - inline_w;
        if detail_x > label_x + char_w * 3.0 {
            let mut detail_color = fg_dim;
            detail_color[3] *= 0.85;
            t.text_system
                .set_metrics(Metrics::new(detail_font_size, label_line_h));
            t.text_system
                .set_size(Some(inline_w.max(1.0)), Some(label_line_h));
            t.glyphs.extend(layout_panel_text(
                &inline_detail,
                t.text_system,
                t.atlas,
                t.queue,
                detail_x,
                row_y + text_v_center,
                detail_color,
            ));
            t.text_system
                .set_metrics(Metrics::new(geom.font_size, label_line_h));
        }
    }

    // Scrollbar (thin, rounded, only when the list overflows).
    if total > visible_rows {
        let sx = popup_x + list_w - scrollbar_w - 3.0 * ui_s;
        let sy = popup_y + pad_y;
        let thumb_h = (rows_h * (visible_rows as f32 / total as f32)).max(12.0);
        let progress = completion.selected_index as f32 / (total.saturating_sub(1).max(1) as f32);
        t.chrome.push(
            RegionDrawInstance::new([sx, sy, scrollbar_w, rows_h], blend(bg, fg, 0.08))
                .with_radius(scrollbar_w * 0.5),
        );
        t.chrome.push(
            RegionDrawInstance::new(
                [sx, sy + (rows_h - thumb_h) * progress, scrollbar_w, thumb_h],
                blend(bg, fg, 0.35),
            )
            .with_radius(scrollbar_w * 0.5),
        );
    }

    // Footer hint bar: hairline + key hints (mirrors the reference's
    // "Press ↵ to insert" strip).
    {
        let footer_y = popup_y + popup_h - footer_h;
        t.chrome.push(RegionDrawInstance::new(
            [popup_x + 1.0, footer_y, popup_w - 2.0, 1.0],
            blend(bg, border, 0.9),
        ));
        let hint = format!(
            "\u{21b5} insert    \u{2191}\u{2193} navigate    esc dismiss    {}/{}",
            completion.selected_index + 1,
            total
        );
        let hint_size = (geom.font_size * 0.82).max(8.0);
        let hint_line_h = footer_h;
        let mut hint_color = fg_dim;
        hint_color[3] *= 0.9;
        t.text_system
            .set_metrics(Metrics::new(hint_size, hint_line_h));
        t.text_system
            .set_size(Some(popup_w - pad_x * 2.0), Some(hint_line_h));
        t.glyphs.extend(layout_panel_text(
            &hint,
            t.text_system,
            t.atlas,
            t.queue,
            popup_x + pad_x,
            footer_y,
            hint_color,
        ));
    }

    // The on-screen popup rect (incl. the 1px shadow margin) so a caller that
    // shares one glyph pass (the canvas) can drop code text under it.
    Some([popup_x, popup_y, popup_w, popup_h])
}

const COMPLETION_KIND_TEXT: u32 = 1;
const COMPLETION_KIND_METHOD: u32 = 2;
const COMPLETION_KIND_FUNCTION: u32 = 3;
const COMPLETION_KIND_CONSTRUCTOR: u32 = 4;
const COMPLETION_KIND_FIELD: u32 = 5;
const COMPLETION_KIND_VARIABLE: u32 = 6;
const COMPLETION_KIND_CLASS: u32 = 7;
const COMPLETION_KIND_INTERFACE: u32 = 8;
const COMPLETION_KIND_MODULE: u32 = 9;
const COMPLETION_KIND_PROPERTY: u32 = 10;
const COMPLETION_KIND_UNIT: u32 = 11;
const COMPLETION_KIND_VALUE: u32 = 12;
const COMPLETION_KIND_ENUM: u32 = 13;
const COMPLETION_KIND_KEYWORD: u32 = 14;
const COMPLETION_KIND_SNIPPET: u32 = 15;
const COMPLETION_KIND_COLOR: u32 = 16;
const COMPLETION_KIND_FILE: u32 = 17;
const COMPLETION_KIND_REFERENCE: u32 = 18;
const COMPLETION_KIND_FOLDER: u32 = 19;
const COMPLETION_KIND_ENUM_MEMBER: u32 = 20;
const COMPLETION_KIND_CONSTANT: u32 = 21;
const COMPLETION_KIND_STRUCT: u32 = 22;
const COMPLETION_KIND_EVENT: u32 = 23;
const COMPLETION_KIND_OPERATOR: u32 = 24;
const COMPLETION_KIND_TYPE_PARAMETER: u32 = 25;

/// Human name for an LSP completion kind, used for the dim right-hand
/// "(method) signature" column (mirrors VS Code's rendering).
pub(super) fn completion_kind_name(kind: Option<u32>) -> &'static str {
    match kind {
        Some(COMPLETION_KIND_TEXT) => "text",
        Some(COMPLETION_KIND_METHOD) => "method",
        Some(COMPLETION_KIND_FUNCTION) => "function",
        Some(COMPLETION_KIND_CONSTRUCTOR) => "constructor",
        Some(COMPLETION_KIND_FIELD) => "field",
        Some(COMPLETION_KIND_VARIABLE) => "variable",
        Some(COMPLETION_KIND_CLASS) => "class",
        Some(COMPLETION_KIND_INTERFACE) => "interface",
        Some(COMPLETION_KIND_MODULE) => "module",
        Some(COMPLETION_KIND_PROPERTY) => "property",
        Some(COMPLETION_KIND_UNIT) => "unit",
        Some(COMPLETION_KIND_VALUE) => "value",
        Some(COMPLETION_KIND_ENUM) => "enum",
        Some(COMPLETION_KIND_KEYWORD) => "keyword",
        Some(COMPLETION_KIND_SNIPPET) => "snippet",
        Some(COMPLETION_KIND_COLOR) => "color",
        Some(COMPLETION_KIND_FILE) => "file",
        Some(COMPLETION_KIND_REFERENCE) => "reference",
        Some(COMPLETION_KIND_FOLDER) => "folder",
        Some(COMPLETION_KIND_ENUM_MEMBER) => "enum member",
        Some(COMPLETION_KIND_CONSTANT) => "constant",
        Some(COMPLETION_KIND_STRUCT) => "struct",
        Some(COMPLETION_KIND_EVENT) => "event",
        Some(COMPLETION_KIND_OPERATOR) => "operator",
        Some(COMPLETION_KIND_TYPE_PARAMETER) => "type parameter",
        _ => "symbol",
    }
}

/// Clamp a completion's inline detail to `max_width`. Path/import-like details
/// (module specifiers, "Auto import from …", file paths) keep their END so the
/// source stays visible (".../logger'") instead of "Auto import fro…"; other
/// details (type signatures) keep their start as before.
pub(super) fn clamp_completion_detail(detail: &str, max_width: f32, font_size: f32) -> String {
    let pathish = detail.contains('/') || detail.contains('\\') || detail.contains("import");
    if pathish {
        clamp_monospace_text_left(detail, max_width, font_size)
    } else {
        clamp_monospace_text(detail, max_width, font_size)
    }
}

pub(super) fn completion_label_spans(
    item: &CompletionDisplayItem,
    match_color: [f32; 4],
) -> Vec<StyledTextSpan> {
    let color = [
        (match_color[0] * 255.0) as u8,
        (match_color[1] * 255.0) as u8,
        (match_color[2] * 255.0) as u8,
        (match_color[3] * 255.0) as u8,
    ];

    item.match_ranges
        .iter()
        .map(|(start, end)| StyledTextSpan::new(*start, *end, color))
        .collect()
}

pub(super) struct CompletionKindBadge<'a> {
    pub(super) icon: &'a str,
    pub(super) color: [f32; 4],
}

pub(super) fn completion_kind_badge<'a>(
    kind: Option<u32>,
    theme: &'a ThemeConfig,
) -> CompletionKindBadge<'a> {
    match kind {
        Some(COMPLETION_KIND_TEXT) => CompletionKindBadge {
            icon: "built_in:info",
            color: theme.ui.fg_dim.as_f32(),
        },
        Some(COMPLETION_KIND_METHOD) | Some(COMPLETION_KIND_FUNCTION) => CompletionKindBadge {
            icon: "built_in:symbol-function",
            color: theme.syntax.function.as_f32(),
        },
        Some(COMPLETION_KIND_CONSTRUCTOR) => CompletionKindBadge {
            icon: "built_in:symbol-constructor",
            color: theme.syntax.constructor.as_f32(),
        },
        Some(COMPLETION_KIND_FIELD) => CompletionKindBadge {
            icon: "built_in:symbol-field",
            color: theme.syntax.field.as_f32(),
        },
        Some(COMPLETION_KIND_VARIABLE) => CompletionKindBadge {
            icon: "built_in:symbol-variable",
            color: theme.syntax.variable.as_f32(),
        },
        Some(COMPLETION_KIND_CLASS) => CompletionKindBadge {
            icon: "built_in:symbol-class",
            color: theme.syntax.r#type.as_f32(),
        },
        Some(COMPLETION_KIND_INTERFACE) => CompletionKindBadge {
            icon: "built_in:symbol-interface",
            color: theme.syntax.r#type.as_f32(),
        },
        Some(COMPLETION_KIND_MODULE) => CompletionKindBadge {
            icon: "built_in:symbol-module",
            color: theme.syntax.namespace.as_f32(),
        },
        Some(COMPLETION_KIND_PROPERTY) => CompletionKindBadge {
            icon: "built_in:symbol-property",
            color: theme.syntax.property.as_f32(),
        },
        Some(COMPLETION_KIND_UNIT) => CompletionKindBadge {
            icon: "built_in:info",
            color: theme.ui.fg_dim.as_f32(),
        },
        Some(COMPLETION_KIND_VALUE) => CompletionKindBadge {
            icon: "built_in:info",
            color: theme.syntax.constant.as_f32(),
        },
        Some(COMPLETION_KIND_ENUM) => CompletionKindBadge {
            icon: "built_in:symbol-enum",
            color: theme.syntax.r#type.as_f32(),
        },
        Some(COMPLETION_KIND_KEYWORD) => CompletionKindBadge {
            icon: "built_in:symbol-keyword",
            color: theme.syntax.keyword.as_f32(),
        },
        Some(COMPLETION_KIND_SNIPPET) => CompletionKindBadge {
            icon: "built_in:info",
            color: theme.ui.amber.as_f32(),
        },
        Some(COMPLETION_KIND_COLOR) => CompletionKindBadge {
            icon: "built_in:info",
            color: theme.ui.magenta.as_f32(),
        },
        Some(COMPLETION_KIND_FILE) => CompletionKindBadge {
            icon: "built_in:file",
            color: theme.ui.fg_dim.as_f32(),
        },
        Some(COMPLETION_KIND_REFERENCE) => CompletionKindBadge {
            icon: "built_in:symbol-reference",
            color: theme.ui.fg_dim.as_f32(),
        },
        Some(COMPLETION_KIND_FOLDER) => CompletionKindBadge {
            icon: "built_in:folder",
            color: theme.ui.amber.as_f32(),
        },
        Some(COMPLETION_KIND_ENUM_MEMBER) => CompletionKindBadge {
            icon: "built_in:symbol-constant",
            color: theme.syntax.constant.as_f32(),
        },
        Some(COMPLETION_KIND_CONSTANT) => CompletionKindBadge {
            icon: "built_in:symbol-constant",
            color: theme.syntax.constant.as_f32(),
        },
        Some(COMPLETION_KIND_STRUCT) => CompletionKindBadge {
            icon: "built_in:symbol-struct",
            color: theme.syntax.r#type.as_f32(),
        },
        Some(COMPLETION_KIND_EVENT) => CompletionKindBadge {
            icon: "built_in:symbol-event",
            color: theme.ui.warning.as_f32(),
        },
        Some(COMPLETION_KIND_OPERATOR) => CompletionKindBadge {
            icon: "built_in:symbol-operator",
            color: theme.syntax.operator.as_f32(),
        },
        Some(COMPLETION_KIND_TYPE_PARAMETER) => CompletionKindBadge {
            icon: "built_in:symbol-type-parameter",
            color: theme.ui.cyan.as_f32(),
        },
        _ => CompletionKindBadge {
            icon: "built_in:identifier",
            color: theme.ui.fg_ghost.as_f32(),
        },
    }
}

#[cfg(test)]
mod kind_name_tests {
    use super::completion_kind_name;

    #[test]
    fn kind_names_match_the_lsp_kind_table() {
        assert_eq!(completion_kind_name(Some(2)), "method");
        assert_eq!(completion_kind_name(Some(3)), "function");
        assert_eq!(completion_kind_name(Some(10)), "property");
        assert_eq!(completion_kind_name(Some(6)), "variable");
        assert_eq!(completion_kind_name(Some(14)), "keyword");
        assert_eq!(completion_kind_name(Some(22)), "struct");
    }

    #[test]
    fn unknown_kinds_fall_back_to_symbol() {
        assert_eq!(completion_kind_name(None), "symbol");
        assert_eq!(completion_kind_name(Some(999)), "symbol");
    }
}

#[cfg(test)]
mod detail_clamp_tests {
    use super::clamp_completion_detail;

    #[test]
    fn import_detail_keeps_the_source_tail() {
        // "Auto import from './logger'" must not become "Auto import fro..." —
        // the user needs to see WHERE it comes from, so we keep the tail.
        let out = clamp_completion_detail("Auto import from './logger'", 60.0, 10.0);
        assert!(
            out.starts_with("..."),
            "import detail should truncate from the left, got {out:?}"
        );
        assert!(
            out.ends_with("logger'"),
            "the import source must stay visible, got {out:?}"
        );
    }

    #[test]
    fn module_path_detail_keeps_the_tail() {
        let out = clamp_completion_detail("@/components/ui/logger", 70.0, 10.0);
        assert!(out.starts_with("..."), "got {out:?}");
        assert!(out.ends_with("logger"), "got {out:?}");
    }

    #[test]
    fn signature_detail_keeps_its_head() {
        let out = clamp_completion_detail("(value: number) => Promise<void>", 60.0, 10.0);
        assert!(
            out.ends_with("..."),
            "signature detail should truncate from the right, got {out:?}"
        );
    }

    #[test]
    fn short_detail_is_unchanged() {
        assert_eq!(clamp_completion_detail("winston", 400.0, 10.0), "winston");
    }
}
