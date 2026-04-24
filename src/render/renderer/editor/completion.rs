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
    caret_rect_for_mode, clamp_monospace_text, estimate_monospace_width, gutter_width_for_editor,
    layout_panel_rich_text, layout_panel_text, layout_panel_text_italic, rect_to_scissor,
    should_draw_block_cursor,
};
use super::{cursor_diagnostic, editor_viewport_geometry, run_x_for_byte, wrap_text_lines};
use crate::text::text_system::StyledTextSpan;

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
        Some(2 | 3 | 4) => CompletionKindBadge {
            icon: "fn",
            color: theme.syntax.function.as_f32(),
        },
        Some(5) => CompletionKindBadge {
            icon: "v",
            color: theme.syntax.field.as_f32(),
        },
        Some(6 | 10 | 12) => CompletionKindBadge {
            icon: "v",
            color: theme.syntax.identifier.as_f32(),
        },
        Some(7 | 8 | 13 | 22 | 25) => CompletionKindBadge {
            icon: "C",
            color: theme.syntax.r#type.as_f32(),
        },
        Some(9 | 17 | 18 | 19) => CompletionKindBadge {
            icon: "m",
            color: theme.ui.fg_dim.as_f32(),
        },
        Some(14 | 24) => CompletionKindBadge {
            icon: "k",
            color: theme.syntax.keyword.as_f32(),
        },
        Some(20 | 21) => CompletionKindBadge {
            icon: "c",
            color: theme.syntax.constant.as_f32(),
        },
        _ => CompletionKindBadge {
            icon: "•",
            color: theme.ui.fg_dim.as_f32(),
        },
    }
}
