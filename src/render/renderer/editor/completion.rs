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
            icon: "",
            color: theme.ui.fg_dim.as_f32(),
        },
        Some(COMPLETION_KIND_METHOD) | Some(COMPLETION_KIND_FUNCTION) => CompletionKindBadge {
            icon: "󰊕",
            color: theme.syntax.function.as_f32(),
        },
        Some(COMPLETION_KIND_CONSTRUCTOR) => CompletionKindBadge {
            icon: "",
            color: theme.syntax.constructor.as_f32(),
        },
        Some(COMPLETION_KIND_FIELD) => CompletionKindBadge {
            icon: "",
            color: theme.syntax.field.as_f32(),
        },
        Some(COMPLETION_KIND_VARIABLE) => CompletionKindBadge {
            icon: "",
            color: theme.syntax.variable.as_f32(),
        },
        Some(COMPLETION_KIND_CLASS) => CompletionKindBadge {
            icon: "",
            color: theme.syntax.r#type.as_f32(),
        },
        Some(COMPLETION_KIND_INTERFACE) => CompletionKindBadge {
            icon: "",
            color: theme.syntax.r#type.as_f32(),
        },
        Some(COMPLETION_KIND_MODULE) => CompletionKindBadge {
            icon: "",
            color: theme.syntax.namespace.as_f32(),
        },
        Some(COMPLETION_KIND_PROPERTY) => CompletionKindBadge {
            icon: "",
            color: theme.syntax.property.as_f32(),
        },
        Some(COMPLETION_KIND_UNIT) => CompletionKindBadge {
            icon: "",
            color: theme.ui.fg_dim.as_f32(),
        },
        Some(COMPLETION_KIND_VALUE) => CompletionKindBadge {
            icon: "",
            color: theme.syntax.constant.as_f32(),
        },
        Some(COMPLETION_KIND_ENUM) => CompletionKindBadge {
            icon: "",
            color: theme.syntax.r#type.as_f32(),
        },
        Some(COMPLETION_KIND_KEYWORD) => CompletionKindBadge {
            icon: "",
            color: theme.syntax.keyword.as_f32(),
        },
        Some(COMPLETION_KIND_SNIPPET) => CompletionKindBadge {
            icon: "",
            color: theme.ui.amber.as_f32(),
        },
        Some(COMPLETION_KIND_COLOR) => CompletionKindBadge {
            icon: "",
            color: theme.ui.magenta.as_f32(),
        },
        Some(COMPLETION_KIND_FILE) => CompletionKindBadge {
            icon: "built_in:file",
            color: theme.ui.fg_dim.as_f32(),
        },
        Some(COMPLETION_KIND_REFERENCE) => CompletionKindBadge {
            icon: "",
            color: theme.ui.fg_dim.as_f32(),
        },
        Some(COMPLETION_KIND_FOLDER) => CompletionKindBadge {
            icon: "built_in:folder",
            color: theme.ui.amber.as_f32(),
        },
        Some(COMPLETION_KIND_ENUM_MEMBER) => CompletionKindBadge {
            icon: "",
            color: theme.syntax.constant.as_f32(),
        },
        Some(COMPLETION_KIND_CONSTANT) => CompletionKindBadge {
            icon: "",
            color: theme.syntax.constant.as_f32(),
        },
        Some(COMPLETION_KIND_STRUCT) => CompletionKindBadge {
            icon: "",
            color: theme.syntax.r#type.as_f32(),
        },
        Some(COMPLETION_KIND_EVENT) => CompletionKindBadge {
            icon: "",
            color: theme.ui.warning.as_f32(),
        },
        Some(COMPLETION_KIND_OPERATOR) => CompletionKindBadge {
            icon: "",
            color: theme.syntax.operator.as_f32(),
        },
        Some(COMPLETION_KIND_TYPE_PARAMETER) => CompletionKindBadge {
            icon: "",
            color: theme.ui.cyan.as_f32(),
        },
        _ => CompletionKindBadge {
            icon: "",
            color: theme.ui.fg_ghost.as_f32(),
        },
    }
}
