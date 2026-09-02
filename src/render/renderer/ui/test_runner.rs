//! Test Runner panel (bottom dock): renders authored test cases with their
//! input/expected/actual and pass-fail status. Mirrors the dedicated-surface
//! pattern of the which-key overlay (text system + pipeline + chrome + scissor).
//!
//! Colors come from the theme `[ui]` tokens — never hard-coded.

use crate::runner::{TestField, TestRunnerState, TestStatus};

use super::super::helpers::{
    estimate_monospace_width, layout_panel_text, mode_display_label, mode_pill_color,
    rect_to_scissor,
};
use crate::core::mode::EditorMode;
use crate::render::{
    glyph_instance::GlyphInstance,
    icon_pipeline::{IconDrawInstance, canonical_icon_id},
    region_pipeline::RegionDrawInstance,
};

const RUNNER_HEADER_H: f32 = 58.0;
const RUNNER_FOOTER_H: f32 = 40.0;
const RUNNER_CARD_H: f32 = 154.0;
const RUNNER_CARD_ERROR_H: f32 = 188.0;
const RUNNER_FIELD_H: f32 = 48.0;
const RUNNER_CARD_GAP: f32 = 10.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TestRunnerPointerAction {
    Run,
    AddCase,
    SelectCase(usize),
    OpenField { case_index: usize, expected: bool },
}

fn point_inside(point: (f32, f32), rect: [f32; 4]) -> bool {
    point.0 >= rect[0]
        && point.0 < rect[0] + rect[2]
        && point.1 >= rect[1]
        && point.1 < rect[1] + rect[3]
}

fn runner_header_run_rect(bounds: [f32; 4], pad: f32, scale: f32) -> [f32; 4] {
    [
        bounds[0] + bounds[2] - pad - 130.0 * scale,
        bounds[1] + 12.0 * scale,
        130.0 * scale,
        30.0 * scale,
    ]
}

fn runner_add_rect(bounds: [f32; 4], pad: f32, scale: f32) -> [f32; 4] {
    [
        bounds[0] + pad,
        bounds[1] + bounds[3] - RUNNER_FOOTER_H * scale + 5.0 * scale,
        (bounds[2] - pad * 2.0).max(1.0),
        30.0 * scale,
    ]
}

fn runner_card_rect(bounds: [f32; 4], pad: f32, y: f32, height: f32) -> [f32; 4] {
    [bounds[0] + pad, y, (bounds[2] - pad * 2.0).max(1.0), height]
}

fn runner_field_rect(card: [f32; 4], expected: bool, scale: f32) -> [f32; 4] {
    let y = card[1] + if expected { 91.0 * scale } else { 35.0 * scale };
    [
        card[0] + 10.0 * scale,
        y,
        (card[2] - 20.0 * scale).max(1.0),
        RUNNER_FIELD_H * scale,
    ]
}

pub fn test_runner_pointer_action_at(
    bounds: [f32; 4],
    state: &TestRunnerState,
    inner_padding: f32,
    point: (f32, f32),
    scale: f32,
) -> Option<TestRunnerPointerAction> {
    let pad = inner_padding.max(8.0 * scale);
    if point_inside(point, runner_header_run_rect(bounds, pad, scale)) {
        return Some(TestRunnerPointerAction::Run);
    }
    if point_inside(point, runner_add_rect(bounds, pad, scale)) {
        return Some(TestRunnerPointerAction::AddCase);
    }
    let mut y = bounds[1] + RUNNER_HEADER_H * scale;
    for (index, case) in state.cases.iter().enumerate().skip(state.scroll_offset) {
        let show_actual = matches!(case.status, TestStatus::Failed | TestStatus::Error);
        let height = if show_actual {
            RUNNER_CARD_ERROR_H * scale
        } else {
            RUNNER_CARD_H * scale
        };
        let card = runner_card_rect(bounds, pad, y, height);
        if point_inside(point, runner_field_rect(card, false, scale)) {
            return Some(TestRunnerPointerAction::OpenField {
                case_index: index,
                expected: false,
            });
        }
        if point_inside(point, runner_field_rect(card, true, scale)) {
            return Some(TestRunnerPointerAction::OpenField {
                case_index: index,
                expected: true,
            });
        }
        if point_inside(point, card) {
            return Some(TestRunnerPointerAction::SelectCase(index));
        }
        y += height + RUNNER_CARD_GAP * scale;
        if y >= bounds[1] + bounds[3] - RUNNER_FOOTER_H * scale {
            break;
        }
    }
    None
}

impl crate::render::renderer::Renderer {
    pub fn test_runner_pointer_action_at(
        &self,
        bounds: [f32; 4],
        state: &TestRunnerState,
        inner_padding: f32,
        point: (f32, f32),
    ) -> Option<TestRunnerPointerAction> {
        let scale = self.ui_scale.max(0.5);
        test_runner_pointer_action_at(bounds, state, inner_padding, point, scale)
    }

    /// Build the Test Runner case-list chrome + glyphs for `bounds` (the right
    /// dock content rect below the tab strip). `show_cursor` lights the edit
    /// caret (true only while the panel is focused). Returns the buffers so the
    /// caller can merge them with the tab strip before a single upload.
    fn build_test_runner_content(
        &mut self,
        bounds: [f32; 4],
        state: &TestRunnerState,
        inner_padding: f32,
        _show_cursor: bool,
        file_label: &str,
        runtime_label: &str,
        mode: EditorMode,
    ) -> (Vec<RegionDrawInstance>, Vec<GlyphInstance>) {
        if bounds[2] <= 2.0 || bounds[3] <= 2.0 {
            return (Vec::new(), Vec::new());
        }

        let scale = self.ui_scale.max(0.5);
        let pad = inner_padding.max(8.0 * scale);
        let font = self.theme.ui.panel_font_size.max(11.0);
        let line_h = self.theme.ui.panel_line_height.max(font + 4.0);
        let radius = self.panel_corner_radius.max(6.0);

        let fg = self.theme.ui.fg.as_f32();
        let fg_dim = self.theme.ui.fg_dim.as_f32();
        let fg_ghost = self.theme.ui.fg_ghost.as_f32();
        let accent = self.theme.ui.accent.as_f32();
        let success = self.theme.ui.success.as_f32();
        let error = self.theme.ui.error.as_f32();
        let warning = self.theme.ui.warning.as_f32();
        let info = self.theme.ui.info.as_f32();
        let selection_bg = self.theme.ui.selection_bg.as_f32();
        let panel_bg = self.theme.ui.panel_bg.as_f32();

        let status_color = |status: TestStatus| -> [f32; 4] {
            match status {
                TestStatus::Passed => success,
                TestStatus::Failed => error,
                TestStatus::Error => warning,
                TestStatus::Running => info,
                TestStatus::Pending => fg_ghost,
            }
        };

        let mut chrome: Vec<RegionDrawInstance> = Vec::new();
        let mut glyphs: Vec<GlyphInstance> = Vec::new();
        let x0 = bounds[0] + pad;
        let header_y = bounds[1] + 9.0 * scale;
        let run_rect = runner_header_run_rect(bounds, pad, scale);

        let title = if file_label.is_empty() {
            "No active file"
        } else {
            file_label
        };
        glyphs.extend(layout_panel_text(
            title,
            &mut self.test_runner_text_system,
            &mut self.atlas,
            &self.queue,
            x0,
            header_y,
            fg,
        ));
        let protocol = if runtime_label.is_empty() {
            "JSON stdin → JSON stdout".to_string()
        } else {
            format!("{runtime_label} · JSON stdin → JSON stdout")
        };
        glyphs.extend(layout_panel_text(
            &protocol,
            &mut self.test_runner_text_system,
            &mut self.atlas,
            &self.queue,
            x0,
            header_y + line_h,
            fg_ghost,
        ));
        let mode_label = mode_display_label(mode);
        let mode_w = estimate_monospace_width(mode_label, font) + 14.0 * scale;
        let mode_x = (run_rect[0] - mode_w - 8.0 * scale).max(x0);
        let mode_pill_color = mode_pill_color(mode, &self.theme);
        chrome.push(
            RegionDrawInstance::new(
                [mode_x, run_rect[1], mode_w, run_rect[3]],
                lerp_color(panel_bg, mode_pill_color, 0.12),
            )
            .with_radius(5.0 * scale),
        );
        glyphs.extend(layout_panel_text(
            mode_label,
            &mut self.test_runner_text_system,
            &mut self.atlas,
            &self.queue,
            mode_x + 7.0 * scale,
            run_rect[1] + 6.0 * scale,
            mode_pill_color,
        ));
        chrome.push(
            RegionDrawInstance::new(run_rect, lerp_color(panel_bg, success, 0.24))
                .with_radius(6.0 * scale),
        );
        glyphs.extend(layout_panel_text(
            if state.is_generating {
                "󱥸 Generating…"
            } else if state.is_running {
                " Running"
            } else {
                " Run F5"
            },
            &mut self.test_runner_text_system,
            &mut self.atlas,
            &self.queue,
            run_rect[0] + 10.0 * scale,
            run_rect[1] + 6.0 * scale,
            success,
        ));

        let list_bottom = bounds[1] + bounds[3] - RUNNER_FOOTER_H * scale;
        let mut y = bounds[1] + RUNNER_HEADER_H * scale;
        for (i, case) in state.cases.iter().enumerate().skip(state.scroll_offset) {
            let selected = state.selected == Some(i);
            let show_actual = matches!(case.status, TestStatus::Failed | TestStatus::Error);
            let card_h = if show_actual {
                RUNNER_CARD_ERROR_H * scale
            } else {
                RUNNER_CARD_H * scale
            };
            if y + card_h > list_bottom {
                break;
            }
            let card = runner_card_rect(bounds, pad, y, card_h);
            chrome.push(
                RegionDrawInstance::new(
                    card,
                    if selected {
                        selection_bg
                    } else {
                        lerp_color(panel_bg, fg, 0.025)
                    },
                )
                .with_radius(radius),
            );
            if selected {
                chrome.push(
                    RegionDrawInstance::new([card[0], card[1], 2.0, card[3]], accent)
                        .with_radius(radius),
                );
            }
            let duration = case
                .duration_ms
                .map(|ms| format!(" · {ms} ms"))
                .unwrap_or_default();
            let head = if case.ai_generated {
                format!("Case {} · AI", i + 1)
            } else {
                format!("Case {}", i + 1)
            };
            glyphs.extend(layout_panel_text(
                &head,
                &mut self.test_runner_text_system,
                &mut self.atlas,
                &self.queue,
                card[0] + 10.0 * scale,
                card[1] + 8.0 * scale,
                if selected { fg } else { fg_dim },
            ));
            let status = format!("{}{}", case.status.label(), duration);
            let status_w = estimate_monospace_width(&status, font);
            glyphs.extend(layout_panel_text(
                &status,
                &mut self.test_runner_text_system,
                &mut self.atlas,
                &self.queue,
                card[0] + card[2] - 10.0 * scale - status_w,
                card[1] + 8.0 * scale,
                status_color(case.status),
            ));
            for field in [TestField::Input, TestField::Expected] {
                let expected_field = field == TestField::Expected;
                let rect = runner_field_rect(card, expected_field, scale);
                let (label, value) = match field {
                    TestField::Input => ("INPUT JSON", &case.input),
                    TestField::Expected => ("EXPECTED JSON", &case.expected),
                };
                chrome.push(
                    RegionDrawInstance::new(rect, lerp_color(panel_bg, fg, 0.06))
                        .with_radius(5.0 * scale),
                );
                glyphs.extend(layout_panel_text(
                    label,
                    &mut self.test_runner_text_system,
                    &mut self.atlas,
                    &self.queue,
                    rect[0] + 7.0 * scale,
                    rect[1] + 4.0 * scale,
                    fg_ghost,
                ));
                let max_chars = ((rect[2] - 14.0 * scale)
                    / estimate_monospace_width("0", font).max(1.0))
                    as usize;
                let preview = json_preview(value, max_chars);
                glyphs.extend(layout_panel_text(
                    &preview,
                    &mut self.test_runner_text_system,
                    &mut self.atlas,
                    &self.queue,
                    rect[0] + 7.0 * scale,
                    rect[1] + line_h + 2.0 * scale,
                    fg,
                ));
            }
            if show_actual {
                let actual = case.actual.as_deref().unwrap_or("");
                let raw = if matches!(case.status, TestStatus::Error) {
                    summarize_error(case.stderr.as_deref(), actual)
                } else {
                    case.stderr.clone().unwrap_or_else(|| actual.to_string())
                };
                let shown = clip_chars(&flatten_value(&raw), 60);
                glyphs.extend(layout_panel_text(
                    &format!("ACTUAL / ERROR  {shown}"),
                    &mut self.test_runner_text_system,
                    &mut self.atlas,
                    &self.queue,
                    card[0] + 10.0 * scale,
                    card[1] + card[3] - line_h - 7.0 * scale,
                    if matches!(case.status, TestStatus::Error) {
                        warning
                    } else {
                        error
                    },
                ));
            }
            y += card_h + RUNNER_CARD_GAP * scale;
        }

        let add_rect = runner_add_rect(bounds, pad, scale);
        chrome.push(
            RegionDrawInstance::new(add_rect, lerp_color(panel_bg, accent, 0.08))
                .with_radius(6.0 * scale),
        );
        glyphs.extend(layout_panel_text(
            "+ Add A   ·   X del   ·   G gen   ·   click to edit",
            &mut self.test_runner_text_system,
            &mut self.atlas,
            &self.queue,
            add_rect[0] + 12.0 * scale,
            add_rect[1] + 6.0 * scale,
            accent,
        ));

        (chrome, glyphs)
    }

    /// Render the right-dock tab strip plus (when the Test Runner tab is active)
    /// the case list, into the shared Test Runner surface in one upload. Drawn
    /// every frame the right dock is visible, regardless of the active tab —
    /// when AI Chat/Inspector/… is active the Test Runner pipeline is otherwise
    /// idle, so it doubles as the strip surface. `content` is `Some` only for
    /// the Test Runner tab.
    #[allow(clippy::type_complexity)]
    #[allow(clippy::too_many_arguments)]
    pub fn update_right_dock_panel(
        &mut self,
        rb: [f32; 4],
        labels: &[&str],
        icons: &[Option<&'static str>],
        active: usize,
        strip_h: f32,
        strip_focused: bool,
        hovered_tab_index: Option<usize>,
        content: Option<(&TestRunnerState, f32, bool, &str, &str)>,
        outline: Option<(
            &[crate::async_runtime::message::LspDocumentSymbol],
            Option<usize>,
            f32,
        )>,
        agent_picker: Option<(&[(&str, &str)], usize, bool)>,
        dojo: Option<(&crate::dojo::view::ProblemPanelModel, f32)>,
        mode: EditorMode,
    ) {
        if rb[2] <= 2.0 || rb[3] <= 2.0 {
            self.clear_test_runner();
            return;
        }

        // Inset the strip + content so the panel's focus-ring outline stays
        // visible around them (mirrors the bottom dock's tab-bar outline inset),
        // instead of the strip painting over the panel's top/side border.
        let inset = crate::workbench::layout_engine::RIGHT_DOCK_OUTLINE_INSET
            .min(rb[2] * 0.5)
            .min(rb[3] * 0.5)
            .max(0.0);
        let ix = rb[0] + inset;
        let iy = rb[1] + inset;
        let iw = (rb[2] - inset * 2.0).max(0.0);
        let ih = (rb[3] - inset * 2.0).max(0.0);
        let strip_h = strip_h.min(ih).max(0.0);
        let (mut chrome, mut glyphs, strip_icons) = self.build_right_tab_strip(
            [ix, iy, iw, strip_h],
            labels,
            icons,
            active,
            strip_focused,
            hovered_tab_index,
        );
        let mut content_icons = Vec::new();
        // Content sits BELOW the strip band; the strip is appended first so it
        // never overlaps content, and the strip's own band is reserved by layout
        // — keeping the strip visually on top of the dock content.
        let content_bounds = [ix, iy + strip_h, iw, (ih - strip_h).max(0.0)];

        if let Some((state, inner_padding, show_cursor, file_label, runtime_label)) = content {
            let (cc, cg) = self.build_test_runner_content(
                content_bounds,
                state,
                inner_padding,
                show_cursor,
                file_label,
                runtime_label,
                mode,
            );
            chrome.extend(cc);
            glyphs.extend(cg);
        } else if let Some((symbols, selected, inner_padding)) = outline {
            let (cc, cg, ci) =
                self.build_outline_content(content_bounds, symbols, selected, inner_padding);
            chrome.extend(cc);
            glyphs.extend(cg);
            content_icons.extend(ci);
        } else if let Some((agents, selected, focused)) = agent_picker {
            let (cc, cg) = self.build_ai_agent_picker(content_bounds, agents, selected, focused);
            chrome.extend(cc);
            glyphs.extend(cg);
        } else if let Some((model, inner_padding)) = dojo {
            let (cc, cg) = self.build_problem_content(content_bounds, model, inner_padding);
            chrome.extend(cc);
            glyphs.extend(cg);
        }

        self.test_runner_scissor = rect_to_scissor([ix, iy, iw, ih]);
        self.test_runner_chrome_instances = chrome;
        self.test_runner_glyph_instances = glyphs;
        content_icons.extend(strip_icons);
        self.test_runner_icon_instances = content_icons;
        self.test_runner_icon_pipeline.upload_instances(
            &self.device,
            &self.test_runner_icon_instances,
            [
                self.surface_state.config.width,
                self.surface_state.config.height,
            ],
        );
        self.test_runner_text_pipeline.upload_instances(
            &self.device,
            &self.queue,
            &self.test_runner_glyph_instances,
        );
    }

    /// Build the right-dock tab strip: equal-width chips, active tab highlighted
    /// with an accent underline. Geometry mirrors `right_dock_tab_index_at`.
    fn build_right_tab_strip(
        &mut self,
        bounds: [f32; 4],
        labels: &[&str],
        icons: &[Option<&'static str>],
        active: usize,
        focused: bool,
        hovered_tab_index: Option<usize>,
    ) -> (
        Vec<RegionDrawInstance>,
        Vec<GlyphInstance>,
        Vec<IconDrawInstance>,
    ) {
        let mut chrome: Vec<RegionDrawInstance> = Vec::new();
        let mut glyphs: Vec<GlyphInstance> = Vec::new();
        let mut icon_instances: Vec<IconDrawInstance> = Vec::new();
        if labels.is_empty() || bounds[2] <= 1.0 || bounds[3] <= 1.0 {
            return (chrome, glyphs, icon_instances);
        }

        let font = self.theme.editor.font_size;
        let line_h = self.theme.editor.line_height;
        let fg = self.theme.ui.fg.as_f32();
        let fg_dim = self.theme.ui.fg_dim.as_f32();
        let accent = self.theme.ui.accent.as_f32();
        let tab_base = super::utils::blend_rgb(
            self.theme.editor.bg.as_f32(),
            self.theme.ui.status_bar_bg.as_f32(),
            0.62,
            1.0,
        );
        let border = self.theme.ui.border_color.as_f32();
        let active_bg = self.theme.editor.bg.as_f32();
        let inactive_bg = tab_base;
        // Hover wash uses the fg token at the shared strip-hover alpha so it
        // stays theme-driven and matches the topbar's hover intensity.
        let mut hover_bg = fg;
        hover_bg[3] = super::utils::DOCK_TAB_HOVER_ALPHA;
        const TOP_BORDER: f32 = 2.0;
        // Round ONLY the strip's two top corners so they follow the panel's
        // rounded focus-ring outline (top-left under the first tab, top-right
        // under the last). The strip sits one outline-inset inside the ring, so
        // subtract that inset so the curve nests just within it instead of poking
        // out. The bottom stays square (flush with the content below).
        let inset = crate::workbench::layout_engine::RIGHT_DOCK_OUTLINE_INSET;
        let radius = (self.panel_corner_radius - inset)
            .min(bounds[3])
            .min(bounds[2] * 0.5)
            .max(0.0);

        // Strip background — top corners only ([TL, TR, BR, BL]).
        chrome.push(
            RegionDrawInstance::new(bounds, inactive_bg)
                .with_corner_radii([radius, radius, 0.0, 0.0]),
        );

        let n = labels.len();
        // Shared with `right_dock_tab_index_at` so rendered tabs and clickable
        // tabs can never drift apart.
        let Some(tab_w) = super::utils::dock_tab_width(bounds[2], n) else {
            return (chrome, glyphs, icon_instances);
        };
        let text_y = bounds[1] + ((bounds[3] - line_h) * 0.5).max(0.0);
        let char_w = estimate_monospace_width("0", font).max(1.0);
        // Render tab titles at the main-editor title size so every dock tab bar
        // matches it. The text system is shared with the panel body, so save and
        // restore its metrics around the labels.
        let saved_metrics = self.test_runner_text_system.buffer_metrics();
        self.test_runner_text_system
            .set_metrics(cosmic_text::Metrics::new(font, line_h));
        for (i, label) in labels.iter().enumerate() {
            let tab_x = bounds[0] + i as f32 * tab_w;
            let is_active = i == active;
            // Only the outer corner of the first/last tab is rounded; the inner
            // edge and every interior tab stay square so the curve appears solely
            // at the panel's outer corners.
            let is_first = i == 0;
            let is_last = i + 1 == n;
            let tab_corners = [
                if is_first { radius } else { 0.0 }, // top-left
                if is_last { radius } else { 0.0 },  // top-right
                0.0,
                0.0,
            ];
            // Subtle wash behind a non-active hovered tab so the pointer
            // target is visible before the user commits to a click (mirrors
            // the topbar tab hover).
            if !is_active && hovered_tab_index == Some(i) {
                chrome.push(
                    RegionDrawInstance::new([tab_x, bounds[1], tab_w, bounds[3]], hover_bg)
                        .with_corner_radii(tab_corners),
                );
            }
            if is_active {
                chrome.push(
                    RegionDrawInstance::new([tab_x, bounds[1], tab_w, bounds[3]], active_bg)
                        .with_corner_radii(tab_corners),
                );
                // Top accent border (like the bottom dock's active tab). Inset it
                // on the outer side of the first/last tab so it never overruns the
                // rounded corner into the panel outline.
                // Focused dock: full accent. Unfocused: keep the accent
                // identity at reduced alpha (the splitter-band low-alpha
                // accent convention) — dropping to fg_dim made the active tab
                // nearly indistinguishable from inactive ones.
                let mut bar_col = accent;
                if !focused {
                    bar_col[3] *= 0.45;
                }
                let bar_x = if is_first { tab_x + radius } else { tab_x };
                let mut bar_w = tab_w;
                if is_first {
                    bar_w -= radius;
                }
                if is_last {
                    bar_w -= radius;
                }
                chrome.push(
                    RegionDrawInstance::new(
                        [bar_x, bounds[1], bar_w.max(0.0), TOP_BORDER],
                        bar_col,
                    )
                    .with_corner_radii([
                        if is_first { radius } else { 0.0 },
                        if is_last { radius } else { 0.0 },
                        0.0,
                        0.0,
                    ]),
                );
            }
            // Thin separator between tabs.
            if i + 1 < n {
                let mut sep = border;
                sep[3] *= 0.5;
                chrome.push(RegionDrawInstance::new(
                    [
                        tab_x + tab_w - 0.5,
                        bounds[1] + 6.0,
                        1.0,
                        (bounds[3] - 12.0).max(0.0),
                    ],
                    sep,
                ));
            }
            let label_color = if is_active { fg } else { fg_dim };
            // SVG asset icons are rendered as textured quads via the icon pipeline;
            // legacy nerd-font glyphs fall back to the text pipeline.
            let icon = icons.get(i).and_then(|id| *id);
            if let Some(icon_id) = icon.and_then(|id| canonical_icon_id(id)) {
                let icon_size = (line_h * 0.72).min(font * 1.3);
                let label_w = estimate_monospace_width(label, font);
                let total_w = icon_size + 4.0 + label_w;
                let start_x = tab_x + ((tab_w - total_w) * 0.5).max(4.0);
                icon_instances.push(IconDrawInstance {
                    icon: icon_id,
                    rect: [
                        start_x,
                        text_y + (line_h - icon_size) * 0.5,
                        icon_size,
                        icon_size,
                    ],
                    tint: label_color,
                });
                let label_x = start_x + icon_size + 4.0;
                let max_chars = ((tab_w - (label_x - tab_x) - 4.0) / char_w).max(1.0) as usize;
                let shown = clip_chars(label, max_chars.max(1));
                glyphs.extend(layout_panel_text(
                    &shown,
                    &mut self.test_runner_text_system,
                    &mut self.atlas,
                    &self.queue,
                    label_x,
                    text_y,
                    label_color,
                ));
            } else {
                let display = match icon {
                    Some(glyph) => format!("{glyph} {label}"),
                    None => label.to_string(),
                };
                let text_w = estimate_monospace_width(&display, font);
                let text_x = tab_x + ((tab_w - text_w) * 0.5).max(4.0);
                let max_chars = ((tab_w - (text_x - tab_x) - 4.0) / char_w).max(1.0) as usize;
                let shown = clip_chars(&display, max_chars.max(1));
                glyphs.extend(layout_panel_text(
                    &shown,
                    &mut self.test_runner_text_system,
                    &mut self.atlas,
                    &self.queue,
                    text_x,
                    text_y,
                    label_color,
                ));
            }
        }

        // Bottom divider between the strip and the content below.
        chrome.push(RegionDrawInstance::new(
            [bounds[0], bounds[1] + bounds[3] - 1.0, bounds[2], 1.0],
            border,
        ));
        self.test_runner_text_system.set_metrics(saved_metrics);
        (chrome, glyphs, icon_instances)
    }

    /// Hit-test a point against the right-dock tab strip. Returns the tab index
    /// under `pos`, or `None`. `strip_bounds` is `[x, y, w, strip_h]`.
    pub fn right_dock_tab_index_at(
        &self,
        tab_count: usize,
        strip_bounds: [f32; 4],
        pos: (f32, f32),
    ) -> Option<usize> {
        if tab_count == 0 {
            return None;
        }
        let (px, py) = pos;
        if px < strip_bounds[0]
            || px >= strip_bounds[0] + strip_bounds[2]
            || py < strip_bounds[1]
            || py >= strip_bounds[1] + strip_bounds[3]
        {
            return None;
        }
        // Same shared geometry as the renderer, so hover/click targets always
        // line up with the painted tabs.
        let tab_w = super::utils::dock_tab_width(strip_bounds[2], tab_count)?;
        let idx = ((px - strip_bounds[0]) / tab_w) as usize;
        Some(idx.min(tab_count - 1))
    }

    /// Build the in-panel AI-agent picker: a header + a vertical list of agents
    /// (label + command), with the selected row highlighted. `agents` is
    /// `[(label, command)]`.
    fn build_ai_agent_picker(
        &mut self,
        bounds: [f32; 4],
        agents: &[(&str, &str)],
        selected: usize,
        focused: bool,
    ) -> (Vec<RegionDrawInstance>, Vec<GlyphInstance>) {
        let mut chrome: Vec<RegionDrawInstance> = Vec::new();
        let mut glyphs: Vec<GlyphInstance> = Vec::new();
        if bounds[2] <= 2.0 || bounds[3] <= 2.0 {
            return (chrome, glyphs);
        }

        let scale = self.ui_scale.max(0.5);
        let pad = 12.0 * scale;
        let font = self.theme.ui.panel_font_size.max(11.0);
        let line_h = self.theme.ui.panel_line_height.max(font + 4.0);
        let fg = self.theme.ui.fg.as_f32();
        let fg_dim = self.theme.ui.fg_dim.as_f32();
        let fg_ghost = self.theme.ui.fg_ghost.as_f32();
        let accent = self.theme.ui.accent.as_f32();
        let selection_bg = self.theme.ui.selection_bg.as_f32();

        let x0 = bounds[0] + pad;
        let bottom = bounds[1] + bounds[3];
        let mut y = bounds[1] + pad;

        // Header.
        glyphs.extend(layout_panel_text(
            "Start an AI agent   ↑↓/j k · Enter",
            &mut self.test_runner_text_system,
            &mut self.atlas,
            &self.queue,
            x0,
            y,
            fg_ghost,
        ));
        y += line_h + 6.0 * scale;

        let row_h = line_h + 10.0 * scale;
        let char_w = estimate_monospace_width("0", font).max(1.0);
        for (i, (label, command)) in agents.iter().enumerate() {
            if y + row_h > bottom + 1.0 {
                break;
            }
            let is_sel = i == selected;
            if is_sel {
                chrome.push(RegionDrawInstance::new(
                    [bounds[0] + pad * 0.5, y, (bounds[2] - pad).max(0.0), row_h],
                    selection_bg,
                ));
                // Accent bar on the selected row.
                chrome.push(RegionDrawInstance::new(
                    [bounds[0] + pad * 0.5, y, 2.0, row_h],
                    if focused { accent } else { fg_dim },
                ));
            }
            let row_text_y = y + (row_h - line_h) * 0.5;
            let label_color = if is_sel { fg } else { fg_dim };
            glyphs.extend(layout_panel_text(
                label,
                &mut self.test_runner_text_system,
                &mut self.atlas,
                &self.queue,
                x0,
                row_text_y,
                label_color,
            ));
            // Dim command to the right of the label.
            let cmd_text = format!("$ {command}");
            let cmd_w = estimate_monospace_width(&cmd_text, font);
            let label_w = estimate_monospace_width(label, font);
            let cmd_x = bounds[0] + bounds[2] - pad - cmd_w;
            if cmd_x > x0 + label_w + char_w * 2.0 {
                glyphs.extend(layout_panel_text(
                    &cmd_text,
                    &mut self.test_runner_text_system,
                    &mut self.atlas,
                    &self.queue,
                    cmd_x,
                    row_text_y,
                    fg_ghost,
                ));
            }
            y += row_h;
        }

        (chrome, glyphs)
    }

    /// Build the Outline panel: a flat, indented list of the active file's LSP
    /// document symbols. Each row = kind icon + name + right-aligned line number;
    /// the symbol containing the cursor is highlighted. Rows past the bottom are
    /// clipped (no scroll yet).
    fn build_outline_content(
        &mut self,
        bounds: [f32; 4],
        symbols: &[crate::async_runtime::message::LspDocumentSymbol],
        selected: Option<usize>,
        inner_padding: f32,
    ) -> (
        Vec<RegionDrawInstance>,
        Vec<GlyphInstance>,
        Vec<IconDrawInstance>,
    ) {
        let mut chrome: Vec<RegionDrawInstance> = Vec::new();
        let mut glyphs: Vec<GlyphInstance> = Vec::new();
        let mut icons: Vec<IconDrawInstance> = Vec::new();
        if bounds[2] <= 2.0 || bounds[3] <= 2.0 {
            return (chrome, glyphs, icons);
        }

        let scale = self.ui_scale.max(0.5);
        let pad = inner_padding.max(8.0 * scale);
        let font = self.theme.ui.panel_font_size.max(11.0);
        let line_h = self.theme.ui.panel_line_height.max(font + 4.0);
        let fg = self.theme.ui.fg.as_f32();
        let fg_dim = self.theme.ui.fg_dim.as_f32();
        let fg_ghost = self.theme.ui.fg_ghost.as_f32();
        let selection_bg = self.theme.ui.selection_bg.as_f32();

        let x0 = bounds[0] + pad;
        let bottom = bounds[1] + bounds[3];
        let mut y = bounds[1] + pad * 0.5;

        if symbols.is_empty() {
            glyphs.extend(layout_panel_text(
                "No symbols — open a file with a language server.",
                &mut self.test_runner_text_system,
                &mut self.atlas,
                &self.queue,
                x0,
                y,
                fg_ghost,
            ));
            return (chrome, glyphs, icons);
        }

        let char_w = estimate_monospace_width("0", font).max(1.0);
        let indent_w = char_w * 2.0;
        for (i, sym) in symbols.iter().enumerate() {
            if y + line_h > bottom + 1.0 {
                break;
            }
            let depth = sym.ancestors.len() as f32;
            let row_x = x0 + depth * indent_w;

            if Some(i) == selected {
                chrome.push(RegionDrawInstance::new(
                    [bounds[0], y - 1.0, bounds[2], line_h],
                    selection_bg,
                ));
            }

            // Right-aligned dim line number, reserved first so the name clips
            // before overlapping it.
            let line_no = (sym.range.start.line + 1).to_string();
            let num_w = estimate_monospace_width(&line_no, font);
            let num_x = bounds[0] + bounds[2] - pad - num_w;
            glyphs.extend(layout_panel_text(
                &line_no,
                &mut self.test_runner_text_system,
                &mut self.atlas,
                &self.queue,
                num_x,
                y,
                fg_ghost,
            ));

            let icon_color = outline_kind_color(
                &sym.kind,
                self.theme.ui.cyan.as_f32(),
                self.theme.ui.magenta.as_f32(),
                self.theme.ui.info.as_f32(),
                self.theme.ui.amber.as_f32(),
                fg_dim,
            );
            let icon_size = line_h.min(18.0 * scale).max(12.0);
            if let Some(icon) = outline_symbol_icon_id(&sym.kind) {
                icons.push(IconDrawInstance {
                    icon,
                    rect: [row_x, y + (line_h - icon_size) * 0.5, icon_size, icon_size],
                    tint: icon_color,
                });
            }

            let name_x = row_x + icon_size + char_w;
            let avail = (num_x - name_x - char_w).max(0.0);
            let max_chars = (avail / char_w) as usize;
            let name = clip_chars(&sym.name, max_chars.max(1));
            let name_color = if Some(i) == selected { fg } else { fg_dim };
            glyphs.extend(layout_panel_text(
                &name,
                &mut self.test_runner_text_system,
                &mut self.atlas,
                &self.queue,
                name_x,
                y,
                name_color,
            ));

            y += line_h;
        }

        (chrome, glyphs, icons)
    }

    /// Problem tab: header, the selected problem (statement, examples,
    /// hints) or SD case, the session clock. Pure model in, instances out.
    fn build_problem_content(
        &mut self,
        bounds: [f32; 4],
        model: &crate::dojo::view::ProblemPanelModel,
        inner_padding: f32,
    ) -> (Vec<RegionDrawInstance>, Vec<GlyphInstance>) {
        use crate::dojo::{
            session::SessionKind,
            view::{PanelContent, RowGlyph, difficulty_label, wrap_text},
        };
        let mut chrome: Vec<RegionDrawInstance> = Vec::new();
        let mut glyphs: Vec<GlyphInstance> = Vec::new();
        if bounds[2] <= 2.0 || bounds[3] <= 2.0 {
            return (chrome, glyphs);
        }

        let scale = self.ui_scale.max(0.5);
        let pad = inner_padding.max(8.0 * scale);
        let font = self.theme.ui.panel_font_size.max(11.0);
        let line_h = self.theme.ui.panel_line_height.max(font + 4.0);
        let fg = self.theme.ui.fg.as_f32();
        let fg_dim = self.theme.ui.fg_dim.as_f32();
        let fg_ghost = self.theme.ui.fg_ghost.as_f32();
        let accent = self.theme.ui.accent.as_f32();
        let border = self.theme.ui.border_color.as_f32();
        let success = self.theme.ui.success.as_f32();
        let warning = self.theme.ui.warning.as_f32();
        let error = self.theme.ui.error.as_f32();
        let info = self.theme.ui.info.as_f32();
        let magenta = self.theme.ui.magenta.as_f32();
        let cyan = self.theme.ui.cyan.as_f32();

        let x0 = bounds[0] + pad;
        let x1 = bounds[0] + bounds[2] - pad;
        let bottom = bounds[1] + bounds[3];
        let char_w = estimate_monospace_width("0", font).max(1.0);
        let width_chars = ((x1 - x0) / char_w).max(8.0) as usize;
        let mut y = bounds[1] + pad * 0.5;
        let footer_y = bottom - line_h - pad * 0.5;

        macro_rules! text {
            ($t:expr, $x:expr, $y:expr, $c:expr) => {
                glyphs.extend(layout_panel_text(
                    $t,
                    &mut self.test_runner_text_system,
                    &mut self.atlas,
                    &self.queue,
                    $x,
                    $y,
                    $c,
                ));
            };
        }
        macro_rules! text_right {
            ($t:expr, $y:expr, $c:expr) => {{
                let s: &str = $t;
                let w = estimate_monospace_width(s, font);
                text!(s, (x1 - w).max(x0), $y, $c);
            }};
        }
        macro_rules! separator {
            () => {{
                chrome.push(RegionDrawInstance::new(
                    [bounds[0], y + 2.0, bounds[2], 1.0],
                    border,
                ));
                y += line_h * 0.5;
            }};
        }

        // ── Header ────────────────────────────────────────────────────────────
        let h = &model.header;
        let mut right = format!(
            "{}/{} solved · streak {}",
            h.overall_done, h.overall_total, h.streak
        );
        if h.redo_due > 0 {
            right.push_str(&format!(" · {} redo due", h.redo_due));
        }
        let right_w = estimate_monospace_width(&right, font);
        let left_max = (((x1 - x0) - right_w - char_w) / char_w).max(4.0) as usize;
        text!(&clip_chars("DOJO", left_max), x0, y, accent);
        text_right!(&right, y, if h.redo_due > 0 { warning } else { fg_dim });
        y += line_h;
        separator!();

        // ── Session clock (any kind) ──────────────────────────────────────────
        let clock = model.session.as_ref().map(|s| {
            let color = if s.expired || s.remaining_s < 60 {
                error
            } else {
                match (s.kind, s.phase_index) {
                    (SessionKind::Sd, _) => cyan,
                    (_, 0) => info,
                    (_, 1) => accent,
                    (_, 2) => warning,
                    _ => magenta,
                }
            };
            (format!("⏱ {} {}", s.phase, s.remaining), color)
        });

        // Body lines: (text, color), wrapped to the panel width; `scroll`
        // skips leading lines.
        let mut lines: Vec<(String, [f32; 4])> = Vec::new();
        let footer: String = match &model.content {
            PanelContent::Empty(message) => {
                for line in wrap_text(message, width_chars) {
                    lines.push((line, fg_ghost));
                }
                "[j/k] move  [Enter] start  [c] language  [w] folder  [n] notebook  [Esc] editor"
                    .to_string()
            }
            PanelContent::Sd(sd) => {
                let title_color = fg;
                let (clock_text, clock_color) = clock.clone().unwrap_or_default();
                let clock_w = if clock_text.is_empty() {
                    0.0
                } else {
                    estimate_monospace_width(&clock_text, font) + char_w
                };
                let title_max = (((x1 - x0) - clock_w) / char_w).max(4.0) as usize;
                text!(&clip_chars(&sd.label, title_max), x0, y, title_color);
                if !clock_text.is_empty() {
                    text_right!(&clock_text, y, clock_color);
                }
                y += line_h;
                let meta = format!(
                    "System Design · {}",
                    if sd.done { "done" } else { "not attempted" }
                );
                text!(&clip_chars(&meta, width_chars), x0, y, fg_dim);
                y += line_h;
                separator!();
                if !sd.topic.is_empty() {
                    lines.push(("Focus".to_string(), fg));
                    for line in wrap_text(&sd.topic, width_chars) {
                        lines.push((line, fg_dim));
                    }
                    lines.push((String::new(), fg_dim));
                }
                for line in wrap_text(
                    "45 minutes: 1 Requirements 5' → 2 Scale 5' → 3 API + data model 5' → 4 High-level design 10' → 5 Deep dive 15' → 6 Bottlenecks + trade-offs 5'. Always ask: what happens when this request dies halfway?",
                    width_chars,
                ) {
                    lines.push((line, fg_dim));
                }
                if model.session.is_some() {
                    "[x] finish  [i] interviewer  [Esc] editor".to_string()
                } else {
                    "[Enter] start (creates the outline)  [i] interviewer  [Esc] editor".to_string()
                }
            }
            PanelContent::Problem(p) => {
                let title = format!("#{} {}", p.id, p.title);
                let diff = difficulty_label(&p.difficulty);
                let diff_color = match p.difficulty.as_str() {
                    "easy" => success,
                    "hard" => error,
                    _ => warning,
                };
                let diff_w = estimate_monospace_width(diff, font) + char_w;
                let title_max = (((x1 - x0) - diff_w) / char_w).max(4.0) as usize;
                text!(&clip_chars(&title, title_max), x0, y, fg);
                text_right!(diff, y, diff_color);
                y += line_h;

                let glyph_color = match p.glyph {
                    RowGlyph::Done => success,
                    RowGlyph::RedoDue => warning,
                    RowGlyph::RedoLater => fg_ghost,
                    RowGlyph::Todo => fg_dim,
                };
                let (clock_text, clock_color) = clock.clone().unwrap_or_default();
                let clock_w = if clock_text.is_empty() {
                    0.0
                } else {
                    estimate_monospace_width(&clock_text, font) + char_w
                };
                text!(p.glyph.symbol(), x0, y, glyph_color);
                let meta = format!("{} · {} · {}", p.status_line, p.category, p.language);
                let meta_max = (((x1 - x0) - clock_w - char_w * 2.0) / char_w).max(4.0) as usize;
                text!(&clip_chars(&meta, meta_max), x0 + char_w * 2.0, y, fg_dim);
                if !clock_text.is_empty() {
                    text_right!(&clock_text, y, clock_color);
                }
                y += line_h;
                separator!();

                if let Some(err) = &p.error {
                    for line in
                        wrap_text(&format!("Could not load the statement: {err}"), width_chars)
                    {
                        lines.push((line, error));
                    }
                    lines.push((
                        "Enter still starts (LeetCode is fetched again).".to_string(),
                        fg_ghost,
                    ));
                } else if p.statement_lines.is_empty() {
                    lines.push((
                        if p.loading {
                            "Loading statement…".to_string()
                        } else {
                            "Enter to fetch the statement and start.".to_string()
                        },
                        fg_ghost,
                    ));
                } else {
                    for source in &p.statement_lines {
                        if source.trim().is_empty() {
                            lines.push((String::new(), fg_dim));
                            continue;
                        }
                        for line in wrap_text(source, width_chars) {
                            lines.push((line, fg_dim));
                        }
                    }
                    lines.push((String::new(), fg_dim));
                    if !p.examples.is_empty() {
                        lines.push(("Examples".to_string(), fg));
                        for (i, (input, expected)) in p.examples.iter().enumerate() {
                            for line in wrap_text(&format!("{}. in  {input}", i + 1), width_chars) {
                                lines.push((line, fg_dim));
                            }
                            for line in wrap_text(&format!("   out {expected}"), width_chars) {
                                lines.push((line, fg_dim));
                            }
                        }
                        lines.push((String::new(), fg_dim));
                    }
                    if !p.hints.is_empty() {
                        if model.show_hints {
                            lines.push(("Hints".to_string(), fg));
                            for (i, hint) in p.hints.iter().enumerate() {
                                for line in wrap_text(&format!("{}. {hint}", i + 1), width_chars) {
                                    lines.push((line, info));
                                }
                            }
                        } else {
                            lines.push((
                                format!(
                                    "[?] show {} hint{}",
                                    p.hints.len(),
                                    if p.hints.len() == 1 { "" } else { "s" }
                                ),
                                fg_ghost,
                            ));
                        }
                    }
                }
                if model.session.is_some() {
                    "[Enter] back to code  [x] give up  [?] hints  [i] interviewer  [Esc] editor"
                        .to_string()
                } else {
                    "[Enter] start  [?] hints  [c] language  [n] notebook  [i] interviewer  [Esc] editor".to_string()
                }
            }
        };

        // Trim trailing blank lines so scrolling stops at real content.
        while lines.last().is_some_and(|(l, _)| l.is_empty()) {
            lines.pop();
        }
        let max_scroll = lines.len().saturating_sub(1);
        for (line, color) in lines.iter().skip(model.scroll.min(max_scroll)) {
            if y + line_h > footer_y - line_h * 0.25 {
                break;
            }
            if !line.is_empty() {
                text!(line, x0, y, *color);
            }
            y += line_h;
        }

        // ── Footer ────────────────────────────────────────────────────────────
        let mut footer_color = fg_ghost;
        if model.focused {
            footer_color = fg_dim;
        }
        text!(
            &clip_chars(&footer, width_chars),
            x0,
            footer_y,
            footer_color
        );
        (chrome, glyphs)
    }

    /// Hit-test a point against the Outline list. Returns the symbol index under
    /// `pos`, or `None`. Geometry mirrors `build_outline_content`.
    pub fn outline_row_at(
        &self,
        content_bounds: [f32; 4],
        count: usize,
        inner_padding: f32,
        pos: (f32, f32),
    ) -> Option<usize> {
        if count == 0 {
            return None;
        }
        let scale = self.ui_scale.max(0.5);
        let pad = inner_padding.max(8.0 * scale);
        let font = self.theme.ui.panel_font_size.max(11.0);
        let line_h = self.theme.ui.panel_line_height.max(font + 4.0);
        let (px, py) = pos;
        if px < content_bounds[0] || px >= content_bounds[0] + content_bounds[2] {
            return None;
        }
        let y0 = content_bounds[1] + pad * 0.5;
        if py < y0 {
            return None;
        }
        let idx = ((py - y0) / line_h) as usize;
        if idx >= count {
            return None;
        }
        // A row clipped below the panel bottom isn't clickable.
        let row_bottom = y0 + (idx as f32 + 1.0) * line_h;
        if row_bottom > content_bounds[1] + content_bounds[3] + 1.0 {
            return None;
        }
        Some(idx)
    }

    /// Drop the Test Runner buffers (called each frame the tab is not active).
    pub fn clear_test_runner(&mut self) {
        if self.test_runner_scissor.is_none()
            && self.test_runner_chrome_instances.is_empty()
            && self.test_runner_glyph_instances.is_empty()
            && self.test_runner_icon_instances.is_empty()
        {
            return;
        }
        self.test_runner_scissor = None;
        self.test_runner_chrome_instances.clear();
        self.test_runner_glyph_instances.clear();
        self.test_runner_icon_instances.clear();
        self.test_runner_icon_pipeline.upload_instances(
            &self.device,
            &[],
            [
                self.surface_state.config.width,
                self.surface_state.config.height,
            ],
        );
        self.test_runner_text_pipeline
            .upload_instances(&self.device, &self.queue, &[]);
    }
}

/// Replace newlines with a visible return glyph so a multiline value renders on
/// a single row. One char in → one char out, so caret char-indexing is stable.
fn flatten_value(value: &str) -> String {
    value.replace('\n', "⏎")
}

fn json_preview(value: &str, max_chars: usize) -> String {
    let compact = serde_json::from_str::<serde_json::Value>(value)
        .and_then(|json| serde_json::to_string(&json))
        .unwrap_or_else(|_| flatten_value(value));
    clip_chars(&compact, max_chars)
}

pub(super) fn outline_symbol_icon_id(kind: &str) -> Option<&'static str> {
    canonical_icon_id(crate::app::command_palette::symbol_icon(kind))
}

/// Color an Outline kind icon by category (mirrors the symbol-picker tones).
pub(super) fn outline_kind_color(
    kind: &str,
    function: [f32; 4],
    type_: [f32; 4],
    variable: [f32; 4],
    module: [f32; 4],
    default: [f32; 4],
) -> [f32; 4] {
    match kind {
        "Function" | "Method" | "Constructor" => function,
        "Class" | "Struct" | "Interface" | "Enum" | "TypeParameter" => type_,
        "Variable" | "Constant" | "Field" | "Property" | "EnumMember" => variable,
        "Namespace" | "Module" | "Package" => module,
        _ => default,
    }
}

/// Truncate `s` to at most `max` chars, appending `…` when it overflows.
pub(super) fn clip_chars(s: &str, max: usize) -> String {
    if max == 0 {
        return String::new();
    }
    let count = s.chars().count();
    if count <= max {
        return s.to_string();
    }
    let keep = max.saturating_sub(1).max(1);
    let mut out: String = s.chars().take(keep).collect();
    out.push('…');
    out
}

/// Distill a one-line summary from a program's stderr. Prefers the first line
/// that names an error/exception (the useful part of a stack trace); falls back
/// to the first non-empty line, then to captured stdout. Newlines flattened.
fn summarize_error(stderr: Option<&str>, actual: &str) -> String {
    let text = stderr.unwrap_or("").trim();
    if !text.is_empty() {
        let pick = text
            .lines()
            .map(str::trim)
            .find(|l| {
                let lower = l.to_ascii_lowercase();
                lower.contains("error") || lower.contains("exception") || lower.contains("panic")
            })
            .or_else(|| text.lines().map(str::trim).find(|l| !l.is_empty()))
            .unwrap_or(text);
        return flatten_value(pick);
    }
    flatten_value(actual)
}

/// Linear blend of two RGBA colors by factor `t` (0 = a, 1 = b). Alpha is kept
/// from `a` so a subtle tint over a solid panel stays opaque.
fn lerp_color(a: [f32; 4], b: [f32; 4], t: f32) -> [f32; 4] {
    [
        a[0] + (b[0] - a[0]) * t,
        a[1] + (b[1] - a[1]) * t,
        a[2] + (b[2] - a[2]) * t,
        a[3],
    ]
}

#[cfg(test)]
mod tests {
    use super::{TestRunnerPointerAction, outline_symbol_icon_id, test_runner_pointer_action_at};
    use crate::runner::TestRunnerState;

    #[test]
    fn outline_uses_document_symbol_svg_icons() {
        assert_eq!(
            outline_symbol_icon_id("Function"),
            Some("built_in:symbol-function")
        );
        assert_eq!(
            outline_symbol_icon_id("Method"),
            Some("built_in:symbol-method")
        );
        assert_eq!(
            outline_symbol_icon_id("Struct"),
            Some("built_in:symbol-struct")
        );
        assert_eq!(
            outline_symbol_icon_id("Field"),
            Some("built_in:symbol-field")
        );
    }

    #[test]
    fn case_card_pointer_hit_test_finds_actions_and_fields() {
        let mut state = TestRunnerState::new();
        state.add_case("{}", "null");
        let bounds = [0.0, 0.0, 320.0, 600.0];

        assert_eq!(
            test_runner_pointer_action_at(bounds, &state, 12.0, (275.0, 26.0), 1.0),
            Some(TestRunnerPointerAction::Run)
        );
        assert_eq!(
            test_runner_pointer_action_at(bounds, &state, 12.0, (80.0, 115.0), 1.0),
            Some(TestRunnerPointerAction::OpenField {
                case_index: 0,
                expected: false,
            })
        );
        assert_eq!(
            test_runner_pointer_action_at(bounds, &state, 12.0, (80.0, 176.0), 1.0),
            Some(TestRunnerPointerAction::OpenField {
                case_index: 0,
                expected: true,
            })
        );
        assert_eq!(
            test_runner_pointer_action_at(bounds, &state, 12.0, (160.0, 580.0), 1.0),
            Some(TestRunnerPointerAction::AddCase)
        );
    }
}
