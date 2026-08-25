use cosmic_text::Metrics;

use crate::render::{
    icon_pipeline::{IconDrawInstance, canonical_icon_id},
    region_pipeline::RegionDrawInstance,
    renderer::{Renderer, TextScissorBatch, TopbarLayoutKey, TopbarTab, TopbarTabKind},
    text_pipeline::InstanceDrawRange,
};

use super::super::helpers::{
    estimate_monospace_width, layout_panel_text, layout_panel_text_bold, rect_to_scissor,
};

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

const TOPBAR_TRAFFIC_LIGHT_SPACE_MACOS: f32 = 78.0;
const TOPBAR_TAB_PADDING_X: f32 = 12.0;
const TOPBAR_TAB_SEPARATOR_WIDTH: f32 = 1.0;
const TOPBAR_ACTIVE_BORDER_HEIGHT: f32 = 2.0;
const TOPBAR_DIRTY_DOT: &str = "●";
const TOPBAR_TAB_ICON_GAP: f32 = 6.0;
/// Close-button glyph shown at the right edge of each tab. Temporarily
/// disabled per user feedback (2026-08-24): the × felt cluttered; flip to
/// `true` to bring it back (render + hitbox + layout reserve all key off
/// this flag, middle-click close works either way).
const TOPBAR_TAB_CLOSE_ENABLED: bool = false;
const TOPBAR_TAB_CLOSE: &str = "\u{00D7}";
/// Absolute logical width range for tabs, scaled by the runtime UI scale.
/// Fixed px bounds keep tabs readable on any window size — the old
/// fraction-of-topbar formula collapsed on narrow windows and ballooned on
/// wide ones.
const TOPBAR_TAB_MIN_WIDTH: f32 = 88.0;
const TOPBAR_TAB_MAX_WIDTH: f32 = 240.0;
const TOPBAR_TAB_CLOSE_HITBOX: f32 = 16.0;
/// Gap between the close hitbox and the tab's right edge.
const TOPBAR_TAB_CLOSE_RIGHT_PAD: f32 = 5.0;
/// Horizontal space reserved inside every tab so the label can never slide
/// underneath the "×" glyph (right pad + hitbox + breathing room). Zero while
/// the button is hidden so tab layout matches the pre-button geometry.
const TOPBAR_TAB_CLOSE_RESERVE: f32 = if TOPBAR_TAB_CLOSE_ENABLED {
    TOPBAR_TAB_CLOSE_RIGHT_PAD + TOPBAR_TAB_CLOSE_HITBOX + 2.0
} else {
    0.0
};
/// Subtle hover wash over non-active tabs, applied to the theme fg color so it
/// adapts to light/dark palettes instead of hardcoding an RGB value.
const TOPBAR_TAB_HOVER_ALPHA: f32 = 0.08;

fn macos_traffic_light_space(runtime_scale: f32, bounds_width: f32) -> f32 {
    (TOPBAR_TRAFFIC_LIGHT_SPACE_MACOS * runtime_scale.max(0.5)).min(bounds_width)
}

fn topbar_tab_at_position(
    position: (f32, f32),
    hitboxes: &[(usize, u64, [f32; 4])],
) -> Option<(usize, u64)> {
    hitboxes.iter().find_map(|(index, identity, bounds)| {
        let inside = position.0 >= bounds[0]
            && position.0 <= bounds[0] + bounds[2]
            && position.1 >= bounds[1]
            && position.1 <= bounds[1] + bounds[3];
        inside.then_some((*index, *identity))
    })
}

fn topbar_tab_asset_icon(
    kind: &TopbarTabKind,
    theme: &crate::config::theme_config::ThemeConfig,
) -> Option<&'static str> {
    let raw: &str = match kind {
        TopbarTabKind::Text { path } | TopbarTabKind::Image { path } => {
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            theme.get_icon_for_file(name, false)
        }
        TopbarTabKind::Terminal => "built_in:terminal",
        TopbarTabKind::References => "built_in:symbol-reference",
        TopbarTabKind::Diagnostics => "built_in:error",
        TopbarTabKind::MarkdownPreview => "built_in:markdown",
        TopbarTabKind::FuzzyPicker => "built_in:text-search",
        TopbarTabKind::Settings => "built_in:conf",
        TopbarTabKind::Help => "built_in:info",
        TopbarTabKind::ExtensionsManager => "built_in:file",
    };
    canonical_icon_id(raw)
}

fn topbar_visible_tab_x(
    positions: &[f32],
    idx: usize,
    first_visible: usize,
    tab_start_x: f32,
) -> f32 {
    tab_start_x + positions[idx] - positions[first_visible]
}

/// Linear blend of two RGBA colors; `t` is the weight of `b`, clamped to 0..1.
/// Used to derive UI washes from existing theme tokens instead of hardcoding
/// RGB values that break on alternate palettes.
fn blend_colors(a: [f32; 4], b: [f32; 4], t: f32) -> [f32; 4] {
    let t = t.clamp(0.0, 1.0);
    [
        a[0] + (b[0] - a[0]) * t,
        a[1] + (b[1] - a[1]) * t,
        a[2] + (b[2] - a[2]) * t,
        a[3] + (b[3] - a[3]) * t,
    ]
}

/// Distribute tab widths across the visible strip.
///
/// Desired widths are clamped into `[min_w, max_w]` first. When the clamped
/// total under-fills the strip, the leftover space is spread evenly across tabs
/// (respecting `max_w`) so tabs flex like an editor's tab bar. When it
/// overfills, tabs shrink proportionally toward `min_w`; anything that still
/// doesn't fit is handled downstream by the scroll/"+"-overflow pass, which
/// only needs consistent per-tab widths.
fn distribute_tab_widths(desired: &[f32], min_w: f32, max_w: f32, available: f32) -> Vec<f32> {
    if desired.is_empty() {
        return Vec::new();
    }
    let mut widths: Vec<f32> = desired.iter().map(|&d| d.clamp(min_w, max_w)).collect();
    let total: f32 = widths.iter().sum();
    if total < available {
        // Grow every tab evenly toward `max_w` until the row is filled.
        let mut leftover = available - total;
        loop {
            let open: Vec<usize> = (0..widths.len())
                .filter(|&i| widths[i] < max_w - f32::EPSILON)
                .collect();
            if open.is_empty() || leftover <= f32::EPSILON {
                break;
            }
            let share = leftover / open.len() as f32;
            for &i in &open {
                let grow = share.min(max_w - widths[i]);
                widths[i] += grow;
                leftover -= grow;
            }
            if share <= f32::EPSILON {
                break;
            }
        }
    } else if total > available {
        // Shrink proportionally toward `min_w`; tabs already at the minimum
        // keep it while flexible tabs absorb the deficit.
        let shrinkable: f32 = widths.iter().map(|w| (w - min_w).max(0.0)).sum();
        if shrinkable > f32::EPSILON {
            let ratio = ((total - available) / shrinkable).min(1.0);
            for w in widths.iter_mut() {
                *w -= (*w - min_w).max(0.0) * ratio;
            }
        }
    }
    widths
}

/// Truncate `label` with a trailing '…' (U+2026) so its estimated width fits
/// `max_width` minus a small right safety margin. The font is proportional, so
/// monospace estimates are approximate — the margin keeps truncation honest.
/// Returns the original label unchanged when it already fits.
fn ellipsize_label(label: &str, max_width: f32, font_size: f32) -> String {
    const RIGHT_SAFETY_MARGIN: f32 = 6.0;
    let budget = (max_width - RIGHT_SAFETY_MARGIN).max(0.0);
    if estimate_monospace_width(label, font_size) <= budget {
        return label.to_string();
    }
    let chars: Vec<char> = label.chars().collect();
    // Start from an estimate of how many chars fit and walk back from there to
    // avoid rescanning the whole label for long file names.
    let per_char = estimate_monospace_width("n", font_size).max(1.0);
    let mut end = ((budget / per_char) as usize).min(chars.len());
    while end > 0 {
        let mut candidate = String::with_capacity(end + 3);
        candidate.extend(chars[..end].iter());
        candidate.push('…');
        if estimate_monospace_width(&candidate, font_size) <= budget {
            return candidate;
        }
        end -= 1;
    }
    // Even a bare ellipsis overflows the budget; draw it anyway so the user
    // can see the tab has content rather than an empty slot.
    "…".to_string()
}

/// Split `label` over at most two lines of `max_width`. Line one takes as many
/// chars as fit whole; the remainder is ellipsized onto line two. Returns
/// `(line1, None)` when the label already fits on one line.
fn wrap_label_two_lines(
    label: &str,
    max_width: f32,
    font_size: f32,
) -> (String, Option<String>) {
    if estimate_monospace_width(label, font_size) <= (max_width - 6.0).max(0.0) {
        return (label.to_string(), None);
    }
    let chars: Vec<char> = label.chars().collect();
    let per_char = estimate_monospace_width("n", font_size).max(1.0);
    let mut end = ((max_width / per_char) as usize).min(chars.len()).max(1);
    while end > 1 {
        let head: String = chars[..end].iter().collect();
        if estimate_monospace_width(&head, font_size) <= max_width {
            break;
        }
        end -= 1;
    }
    let head: String = chars[..end].iter().collect();
    let tail: String = chars[end..].iter().collect();
    (head, Some(ellipsize_label(&tail, max_width, font_size)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn visible_tab_x_positions_are_rebased_to_tab_start_when_scrolled() {
        let positions = vec![100.0, 201.0, 302.0, 403.0];

        assert_eq!(topbar_visible_tab_x(&positions, 2, 2, 100.0), 100.0);
        assert_eq!(topbar_visible_tab_x(&positions, 3, 2, 100.0), 201.0);
    }

    #[test]
    fn macos_traffic_light_space_scales_with_runtime_scale() {
        assert_eq!(macos_traffic_light_space(1.0, 900.0), 78.0);
        assert_eq!(macos_traffic_light_space(2.0, 900.0), 156.0);
        assert_eq!(macos_traffic_light_space(2.0, 100.0), 100.0);
    }

    #[test]
    fn long_labels_wrap_onto_a_second_line() {
        let (a, b) = wrap_label_two_lines("short", 400.0, 13.0);
        assert_eq!(a, "short");
        assert!(b.is_none());

        let (a, b) = wrap_label_two_lines("platform-core--bo-cong-thuong", 60.0, 13.0);
        let b = b.expect("long label should wrap");
        assert!(!a.is_empty() && a.chars().count() < 29);
        assert!(estimate_monospace_width(&a, 13.0) <= 60.0);
        assert!(estimate_monospace_width(&b, 13.0) <= 60.0);
        assert!(b.ends_with('…') || a.chars().count() + b.chars().count() == 29);
    }

    #[test]
    fn tab_hit_testing_returns_the_rendered_buffer_index() {
        let hitboxes = vec![
            (2, 20, [200.0, 0.0, 100.0, 36.0]),
            (3, 30, [301.0, 0.0, 90.0, 36.0]),
        ];

        assert_eq!(
            topbar_tab_at_position((250.0, 18.0), &hitboxes),
            Some((2, 20))
        );
        assert_eq!(
            topbar_tab_at_position((350.0, 18.0), &hitboxes),
            Some((3, 30))
        );
        assert_eq!(topbar_tab_at_position((100.0, 18.0), &hitboxes), None);
    }

    #[test]
    fn width_distribution_clamps_and_grows_to_fill_available_space() {
        // Desired widths below the minimum are clamped up; leftover space is
        // spread evenly until every tab reaches the max.
        let widths = distribute_tab_widths(&[40.0, 60.0], 88.0, 240.0, 400.0);
        assert_eq!(widths.len(), 2);
        for w in &widths {
            assert!((88.0..=240.0).contains(w), "width {w} out of range");
        }
        // Two clamped tabs (88 + 88 = 176) grow evenly by the 224 leftover,
        // capped at max 240 each.
        let total: f32 = widths.iter().sum();
        assert!((total - 400.0).abs() < 1e-3, "expected fill, got {total}");

        // When even maxed-out tabs can't fill a huge strip, they stay at max.
        let widths = distribute_tab_widths(&[100.0], 88.0, 240.0, 10_000.0);
        assert_eq!(widths[0], 240.0);

        // Empty input stays empty.
        assert!(distribute_tab_widths(&[], 88.0, 240.0, 400.0).is_empty());
    }

    #[test]
    fn width_distribution_shrinks_proportionally_but_never_below_min() {
        let desired = [300.0, 300.0, 300.0];
        let min_w = 88.0;
        let max_w = 240.0;
        // Tight strip: proportional shrink toward min.
        let widths = distribute_tab_widths(&desired, min_w, max_w, 600.0);
        let total: f32 = widths.iter().sum();
        assert!(
            (total - 600.0).abs() < 1e-2,
            "expected shrink to fit, got {total}"
        );
        for w in &widths {
            assert!(*w >= min_w - 1e-3, "width {w} below minimum");
            assert_eq!(*w, widths[0], "shrink must be symmetric");
        }
        // Extremely tight strip: everything bottoms out at the minimum so the
        // downstream overflow pass can hide the excess.
        let widths = distribute_tab_widths(&desired, min_w, max_w, 100.0);
        for w in &widths {
            assert_eq!(*w, min_w);
        }
    }

    #[test]
    fn ellipsis_truncation_fits_budget_and_keeps_short_labels() {
        let font_size = 13.0;
        let label = "a_very_long_file_name_that_will_not_fit.rs";
        let full_width = estimate_monospace_width(label, font_size);

        // Short labels pass through untouched.
        assert_eq!(ellipsize_label("main.rs", full_width, font_size), "main.rs");

        // Long labels get exactly one trailing '…' and fit the budget minus
        // the right safety margin.
        let truncated = ellipsize_label(label, full_width * 0.5, font_size);
        assert!(truncated.ends_with('…'));
        assert!(truncated.len() < label.len());
        assert!(
            estimate_monospace_width(&truncated, font_size)
                <= full_width * 0.5 - 6.0 + f32::EPSILON
        );

        // Degenerate budget still yields a visible placeholder.
        assert_eq!(ellipsize_label(label, 0.0, font_size), "…");
    }

    #[test]
    fn close_hitboxes_contain_their_right_edge_zone() {
        // Same rect shape update_topbar_content pushes: 16x16 logical px
        // centered vertically, inset from the tab's right edge.
        let hitboxes = vec![
            (0, 7_u64, [90.0, 10.0, TOPBAR_TAB_CLOSE_HITBOX, TOPBAR_TAB_CLOSE_HITBOX]),
            (1, 9_u64, [210.5, 10.0, TOPBAR_TAB_CLOSE_HITBOX, TOPBAR_TAB_CLOSE_HITBOX]),
        ];
        // Center and corners of the box hit; points outside don't.
        assert_eq!(
            topbar_tab_at_position((98.0, 18.0), &hitboxes),
            Some((0, 7))
        );
        assert_eq!(
            topbar_tab_at_position((90.0 + TOPBAR_TAB_CLOSE_HITBOX - 0.01, 18.0), &hitboxes),
            Some((0, 7))
        );
        assert_eq!(
            topbar_tab_at_position((89.99, 18.0), &hitboxes),
            None,
            "left of the close zone must fall through to the tab body"
        );
        assert_eq!(
            topbar_tab_at_position((218.0, 18.0), &hitboxes),
            Some((1, 9))
        );
    }
}

struct BundledLogo {
    width: u32,
    height: u32,
    rgba: Vec<u8>,
}

fn bundled_logo() -> Option<&'static BundledLogo> {
    static LOGO: std::sync::OnceLock<Option<BundledLogo>> = std::sync::OnceLock::new();
    LOGO.get_or_init(|| {
        let bytes = include_bytes!("../../../../assets/logo_welcome.png");
        let decoded = image::load_from_memory(bytes).ok()?;
        let rgba = decoded.to_rgba8();
        Some(BundledLogo {
            width: rgba.width(),
            height: rgba.height(),
            rgba: rgba.into_raw(),
        })
    })
    .as_ref()
}

impl Renderer {
    pub fn update_topbar_content(
        &mut self,
        tabs: &[TopbarTab],
        active_buffer_index: Option<usize>,
        center_x: f32,
        bounds: [f32; 4],
        hovered_tab_index: Option<usize>,
        project_label: &str,
    ) -> Vec<RegionDrawInstance> {
        if bounds[2] < 1.0 || bounds[3] < 1.0 {
            self.topbar_scissor = None;
            self.topbar_glyph_instances.clear();
            self.topbar_icon_instances.clear();
            self.topbar_icon_pipeline.upload_instances(
                &self.device,
                &self.topbar_icon_instances,
                [
                    self.surface_state.config.width,
                    self.surface_state.config.height,
                ],
            );
            self.topbar_chrome_instances.clear();
            self.topbar_text_batches.clear();
            self.topbar_tab_hitboxes.clear();
            self.topbar_close_hitboxes.clear();
            self.topbar_project_hitbox = None;
            self.last_topbar_layout_key = None;
            self.topbar_text_pipeline
                .upload_instances(&self.device, &self.queue, &[]);
            self.topbar_logo_image_pipeline.clear();
            self.topbar_logo_scissor = None;
            return vec![];
        }

        let layout_key = TopbarLayoutKey {
            tabs: tabs.to_vec(),
            active_buffer_index,
            hovered_tab_index,
            center_x,
            project_label: project_label.to_string(),
            bounds,
        };
        if self.last_topbar_layout_key.as_ref() == Some(&layout_key) {
            return self.topbar_chrome_instances.clone();
        }

        self.topbar_scissor = rect_to_scissor(bounds);
        self.topbar_tab_hitboxes.clear();
        self.topbar_close_hitboxes.clear();

        let line_h = self.statusbar_line_height;
        let font_size = self.statusbar_font_size;
        let content_top = 0.0;
        let content_bottom = 0.0;
        let content_y = bounds[1] + content_top;
        let content_h = (bounds[3] - content_top - content_bottom).max(1.0);
        self.topbar_text_system
            .set_metrics(Metrics::new(font_size, line_h));
        self.topbar_text_system.set_size(None, Some(content_h));
        let origin_y = content_y + ((content_h - line_h) * 0.5).max(0.0);
        let active_fg = self.theme.ui.fg.as_f32();
        let inactive_fg = self.theme.ui.fg_ghost.as_f32();
        let empty_fg = self.theme.ui.fg_ghost.as_f32();
        let dirty_fg = self.theme.ui.warning.as_f32();
        // Blend elevated_bg toward editor.bg so the active tab stays distinct
        // even in themes where the two tokens are near-identical.
        let active_bg =
            blend_colors(self.theme.ui.elevated_bg.as_f32(), self.theme.editor.bg.as_f32(), 0.30);
        let accent = self.theme.ui.accent.as_f32();
        let mut hover_bg = self.theme.ui.fg.as_f32();
        hover_bg[3] = TOPBAR_TAB_HOVER_ALPHA;
        let border = self.theme.ui.border_color.as_f32();
        // Owned copy: `draw_project_segment` takes `&mut Renderer`, which
        // cannot coexist with a borrow into `self.theme`.
        let font_family = self.theme.editor.font_family.as_deref().map(str::to_string);
        let font_family = font_family.as_deref();

        let mut glyphs = Vec::new();
        self.topbar_icon_instances.clear();
        let mut text_batches = Vec::new();
        let mut chrome = Vec::new();

        let item_gap = (font_size * 0.95).max(8.0);
        let logo_size = (bounds[3] - 8.0).max(12.0);
        let logo_x = bounds[0] + bounds[2] - logo_size - 12.0;
        let logo_y = bounds[1] + (bounds[3] - logo_size) * 0.5;

        // Reduce available right by the logo space to avoid tabs rendering over the logo
        let available_right = logo_x - item_gap;
        let dirty_gap = self.topbar_dirty_gap;
        let topbar_start_x = if cfg!(target_os = "macos") {
            macos_traffic_light_space(self.ui_scale, bounds[2])
        } else {
            0.0
        };

        // Align tab start exactly at the start of the main editor (center_x)
        let tab_x = bounds[0] + center_x.max(topbar_start_x);

        // ── Project label + branch, in the segment above the left dock ────────
        // Single line when it fits ("project ⎇branch"), otherwise two stacked
        // lines (name over branch) — the segment is only as wide as the gap
        // between the macOS traffic lights and the editor start, so it can get
        // quite narrow. Hidden entirely when there is no workspace or no room.
        self.topbar_project_hitbox = None;
        let project = project_label.trim();
        if !project.is_empty() {
            let seg_x = bounds[0] + topbar_start_x + TOPBAR_TAB_PADDING_X;
            let seg_limit = tab_x - 16.0;
            let avail_w = (seg_limit - seg_x).max(0.0);
            const SEG_MIN_WIDTH: f32 = 64.0;
            if avail_w >= SEG_MIN_WIDTH {
                // Folder name only, vertically centered — nothing else. The
                // git branch and change counts live in the status bar.
                self.topbar_text_system.set_font_family(font_family);
                // Long folder names wrap onto a second line when the segment
                // is tall enough for two; otherwise they ellipsize as before.
                let (line1, line2) = if content_h >= line_h * 2.0 {
                    wrap_label_two_lines(project, avail_w, font_size)
                } else {
                    (ellipsize_label(project, avail_w, font_size), None)
                };
                let lines = if line2.is_some() { 2.0 } else { 1.0 };
                let text_y =
                    content_y + ((content_h - line_h * lines) * 0.5).max(0.0);
                let proj_start = glyphs.len() as u32;
                for (i, line) in [Some(&line1), line2.as_ref()]
                    .into_iter()
                    .flatten()
                    .enumerate()
                {
                    glyphs.extend(layout_panel_text_bold(
                        line,
                        &mut self.topbar_text_system,
                        &mut self.atlas,
                        &self.queue,
                        seg_x,
                        text_y + line_h * i as f32,
                        active_fg,
                    ));
                }
                let shown_name = line1;
                let proj_count = glyphs.len() as u32 - proj_start;
                // Own scissor batch: these glyphs precede the tab glyphs, and
                // each tab batch clips to its own tab rect — without this the
                // project text would be clipped away entirely.
                if proj_count > 0 {
                    let name_w =
                        estimate_monospace_width(&shown_name, font_size).min(avail_w);
                    self.topbar_project_hitbox =
                        Some([seg_x - 4.0, content_y, name_w + 8.0, content_h]);
                    if let Some(scissor) = rect_to_scissor([
                        seg_x - 8.0,
                        content_y,
                        avail_w + 24.0,
                        content_h,
                    ]) {
                        text_batches.push(TextScissorBatch {
                            scissor,
                            range: InstanceDrawRange {
                                start: proj_start,
                                count: proj_count,
                            },
                        });
                    }
                }
            }
        }

        // Render Logo at the far right
        self.topbar_logo_image_pipeline.clear();
        self.topbar_logo_scissor = None;
        if let Some(logo) = bundled_logo() {
            let rect = [logo_x, logo_y, logo_size, logo_size];
            self.topbar_logo_scissor = rect_to_scissor(bounds);
            self.topbar_logo_image_pipeline.upload_rgba(
                &self.device,
                &self.queue,
                &logo.rgba,
                logo.width,
                logo.height,
                rect,
                [
                    self.surface_state.config.width,
                    self.surface_state.config.height,
                ],
            );
        }

        if tabs.is_empty() {
            let start = glyphs.len() as u32;
            glyphs.extend(layout_panel_text(
                "[ no file ]",
                &mut self.topbar_text_system,
                &mut self.atlas,
                &self.queue,
                tab_x + TOPBAR_TAB_PADDING_X,
                origin_y,
                empty_fg,
            ));
            let count = glyphs.len() as u32 - start;
            let empty_width = (available_right - tab_x).max(0.0);
            if let Some(scissor) =
                topbar_tab_text_scissor([tab_x, content_y, empty_width, content_h])
            {
                if count > 0 {
                    text_batches.push(TextScissorBatch {
                        scissor,
                        range: InstanceDrawRange { start, count },
                    });
                }
            }
        } else {
            // ── Pass 1: measure every tab so we can keep the active tab visible and
            //    show a "+N" overflow indicator when tabs don't fit.
            struct TabMeasure {
                icon_size: f32,
                icon_id: Option<&'static str>,
                icon_gap_eff: f32,
                dirty_extra_width: f32,
                desired_width: f32,
            }
            let mut measured: Vec<TabMeasure> = Vec::with_capacity(tabs.len());
            for (idx, tab) in tabs.iter().enumerate() {
                // A dirty tab that is NOT hovered shows the ● marker instead of
                // the close button, so the marker only reserves space then.
                let show_dirty_dot = tab.is_dirty && hovered_tab_index != Some(idx);
                let label_width = estimate_monospace_width(&tab.label, font_size);
                let dirty_extra_width = if show_dirty_dot {
                    dirty_gap + estimate_monospace_width(TOPBAR_DIRTY_DOT, font_size)
                } else {
                    0.0
                };
                let icon_size = (content_h * 0.72).min(font_size * 1.15);
                let icon_id = topbar_tab_asset_icon(&tab.kind, &self.theme);
                let icon_w = icon_id.map(|_| icon_size).unwrap_or(0.0);
                let icon_gap_eff = if icon_id.is_some() {
                    TOPBAR_TAB_ICON_GAP
                } else {
                    0.0
                };
                let content_width = icon_w + icon_gap_eff + label_width + dirty_extra_width;
                let desired_width =
                    TOPBAR_TAB_PADDING_X * 2.0 + TOPBAR_TAB_CLOSE_RESERVE + content_width;
                measured.push(TabMeasure {
                    icon_size,
                    icon_id,
                    icon_gap_eff,
                    dirty_extra_width,
                    desired_width,
                });
            }

            // Distribute widths against the visible strip: flex up to the max
            // when there's room, shrink proportionally to the min when tight.
            let visible_width = (available_right - tab_x).max(0.0);
            let runtime_scale = self.ui_scale.max(0.5);
            let tab_min_w = TOPBAR_TAB_MIN_WIDTH * runtime_scale;
            let tab_max_w = TOPBAR_TAB_MAX_WIDTH * runtime_scale;
            let widths = distribute_tab_widths(
                &measured.iter().map(|m| m.desired_width).collect::<Vec<_>>(),
                tab_min_w,
                tab_max_w,
                visible_width,
            );

            struct TabGeom {
                width: f32,
                separator: f32,
                icon_size: f32,
                icon_id: Option<&'static str>,
                icon_gap_eff: f32,
                dirty_extra_width: f32,
            }
            let mut geoms: Vec<TabGeom> = Vec::with_capacity(measured.len());
            let mut positions: Vec<f32> = Vec::with_capacity(measured.len());
            let mut pos = tab_x;
            for (idx, m) in measured.into_iter().enumerate() {
                let tab_width = widths[idx];
                let separator_width = if idx < tabs.len() - 1 {
                    TOPBAR_TAB_SEPARATOR_WIDTH
                } else {
                    0.0
                };
                positions.push(pos);
                geoms.push(TabGeom {
                    width: tab_width,
                    separator: separator_width,
                    icon_size: m.icon_size,
                    icon_id: m.icon_id,
                    icon_gap_eff: m.icon_gap_eff,
                    dirty_extra_width: m.dirty_extra_width,
                });
                pos += tab_width + separator_width;
            }

            let active_idx = active_buffer_index
                .unwrap_or(0)
                .min(tabs.len().saturating_sub(1));
            let total_tabs_width = positions
                .last()
                .zip(geoms.last())
                .map(|(&p, g)| p + g.width - tab_x)
                .unwrap_or(0.0);

            // Decide the first visible tab. When everything fits, start at 0.
            // Otherwise scroll so the active tab is visible while keeping as many
            // preceding tabs on screen as possible.
            let first_visible = if total_tabs_width <= visible_width {
                0
            } else {
                let active_right = positions[active_idx] + geoms[active_idx].width;
                let mut first = active_idx;
                for i in (0..active_idx).rev() {
                    if active_right - positions[i] <= visible_width {
                        first = i;
                    } else {
                        break;
                    }
                }
                first
            };

            // Decide the last visible tab, reserving space for a "+N" indicator
            // whenever tabs remain hidden on the right. The active tab is never
            // clipped out.
            let mut last_visible = tabs.len().saturating_sub(1);
            loop {
                let used_width =
                    positions[last_visible] + geoms[last_visible].width - positions[first_visible];
                let overflow_count = tabs.len().saturating_sub(last_visible + 1);
                let indicator_w = if overflow_count > 0 {
                    estimate_monospace_width(&format!("+{overflow_count}"), font_size)
                        + TOPBAR_TAB_PADDING_X * 2.0
                } else {
                    0.0
                };
                if used_width + indicator_w <= visible_width || last_visible <= first_visible {
                    break;
                }
                if last_visible == active_idx {
                    // Can't shrink past the active tab; accept that the indicator
                    // may be partially clipped by the topbar scissor.
                    break;
                }
                last_visible -= 1;
            }
            let overflow_count = tabs.len().saturating_sub(last_visible + 1);
            let overflow_label = if overflow_count > 0 {
                format!("+{overflow_count}")
            } else {
                String::new()
            };
            let overflow_width = if overflow_count > 0 {
                estimate_monospace_width(&overflow_label, font_size) + TOPBAR_TAB_PADDING_X * 2.0
            } else {
                0.0
            };

            // ── Pass 2: draw the visible slice.
            for idx in first_visible..=last_visible {
                let tab = &tabs[idx];
                let is_active = active_buffer_index == Some(idx);
                let tab_width = geoms[idx].width;
                let tab_x = topbar_visible_tab_x(&positions, idx, first_visible, tab_x);
                let icon_size = geoms[idx].icon_size;
                let icon_id = geoms[idx].icon_id;
                let icon_w = icon_id.map(|_| icon_size).unwrap_or(0.0);
                let icon_gap_eff = geoms[idx].icon_gap_eff;
                let dirty_extra_width = geoms[idx].dirty_extra_width;
                // Ellipsize the label to the space actually available inside the
                // tab (left pad .. close-button zone) instead of relying only on
                // the hard scissor clip to chop mid-glyph.
                let label_budget = (tab_width
                    - TOPBAR_TAB_PADDING_X
                    - TOPBAR_TAB_CLOSE_RESERVE
                    - icon_w
                    - icon_gap_eff
                    - dirty_extra_width)
                    .max(0.0);
                let display_label = ellipsize_label(&tab.label, label_budget, font_size);
                let label_draw_width = estimate_monospace_width(&display_label, font_size);
                let content_width = icon_w + icon_gap_eff + label_draw_width + dirty_extra_width;
                self.topbar_tab_hitboxes.push((
                    idx,
                    tab.identity,
                    [tab_x, content_y, tab_width, content_h],
                ));
                // Close button hitbox, centered vertically at the tab's right
                // edge. Kept separate from the body hitbox so a press here can
                // be routed to BufferClose without also activating the tab.
                // Only registered while the button is enabled.
                let close_rect = [
                    tab_x + tab_width - TOPBAR_TAB_CLOSE_RIGHT_PAD - TOPBAR_TAB_CLOSE_HITBOX,
                    content_y + (content_h - TOPBAR_TAB_CLOSE_HITBOX).max(0.0) * 0.5,
                    TOPBAR_TAB_CLOSE_HITBOX,
                    TOPBAR_TAB_CLOSE_HITBOX,
                ];
                if TOPBAR_TAB_CLOSE_ENABLED {
                    self.topbar_close_hitboxes
                        .push((idx, tab.identity, close_rect));
                }

                // Subtle wash behind a non-active hovered tab so the pointer
                // target is visible before the user commits to a click.
                if !is_active && hovered_tab_index == Some(idx) {
                    chrome.push(RegionDrawInstance::new(
                        [tab_x, content_y, tab_width, content_h.max(0.0)],
                        hover_bg,
                    ));
                }

                if is_active {
                    chrome.push(RegionDrawInstance::new(
                        [tab_x, content_y, tab_width, content_h.max(0.0)],
                        active_bg,
                    ));
                    chrome.push(RegionDrawInstance::new(
                        [
                            tab_x,
                            (content_y + content_h - TOPBAR_ACTIVE_BORDER_HEIGHT).max(content_y),
                            tab_width,
                            TOPBAR_ACTIVE_BORDER_HEIGHT.min(content_h.max(0.0)),
                        ],
                        accent,
                    ));
                }

                let is_last_visible = idx == last_visible && overflow_count > 0;
                if idx < tabs.len() - 1 && !is_last_visible {
                    chrome.push(RegionDrawInstance::new(
                        [
                            tab_x + tab_width,
                            content_y,
                            TOPBAR_TAB_SEPARATOR_WIDTH,
                            content_h.max(0.0),
                        ],
                        border,
                    ));
                }

                // Center the content inside the area left of the close button
                // so the label can never slide underneath the "×".
                let inner_left = tab_x + TOPBAR_TAB_PADDING_X;
                let inner_width =
                    (tab_width - TOPBAR_TAB_PADDING_X - TOPBAR_TAB_CLOSE_RESERVE).max(0.0);
                let content_start = inner_left + ((inner_width - content_width) / 2.0).max(0.0);
                let icon_x = content_start;
                let text_x = content_start + icon_w + icon_gap_eff;
                let batch_start = glyphs.len() as u32;

                if let Some(icon) = icon_id {
                    // Tint icons from the theme fg tokens so they match the
                    // label weight instead of staying pure white.
                    self.topbar_icon_instances.push(IconDrawInstance {
                        icon,
                        rect: [
                            icon_x,
                            content_y + (content_h - icon_size) * 0.5,
                            icon_size,
                            icon_size,
                        ],
                        // Tint must stay white: file-type icons are COLORED
                        // SVG assets and the pipeline multiplies tint by the
                        // asset's own colors — any fg-based tint desaturates
                        // them to gray.
                        tint: [1.0, 1.0, 1.0, 1.0],
                    });
                }

                self.topbar_text_system.set_font_family(font_family);

                let label_color = if tab.missing_on_disk {
                    inactive_fg
                } else {
                    tab.git_color
                        .unwrap_or(if is_active { active_fg } else { inactive_fg })
                };

                if is_active {
                    glyphs.extend(layout_panel_text_bold(
                        &display_label,
                        &mut self.topbar_text_system,
                        &mut self.atlas,
                        &self.queue,
                        text_x,
                        origin_y,
                        label_color,
                    ));
                } else {
                    glyphs.extend(layout_panel_text(
                        &display_label,
                        &mut self.topbar_text_system,
                        &mut self.atlas,
                        &self.queue,
                        text_x,
                        origin_y,
                        label_color,
                    ));
                }
                if tab.missing_on_disk {
                    let strike_y = origin_y + font_size * 0.55;
                    chrome.push(RegionDrawInstance::new(
                        [text_x, strike_y, label_draw_width, 1.0],
                        label_color,
                    ));
                }
                // Dirty tabs show the ● marker; with the "×" enabled it takes
                // over while hovered (VS Code behavior). With the button
                // hidden the dot is unconditional again.
                let show_dirty_dot = tab.is_dirty
                    && (!TOPBAR_TAB_CLOSE_ENABLED || hovered_tab_index != Some(idx));
                if show_dirty_dot {
                    glyphs.extend(layout_panel_text(
                        TOPBAR_DIRTY_DOT,
                        &mut self.topbar_text_system,
                        &mut self.atlas,
                        &self.queue,
                        text_x + label_draw_width + dirty_gap,
                        origin_y,
                        dirty_fg,
                    ));
                } else if TOPBAR_TAB_CLOSE_ENABLED {
                    let close_color = if is_active { active_fg } else { inactive_fg };
                    let close_text_w = estimate_monospace_width(TOPBAR_TAB_CLOSE, font_size);
                    let close_glyph_x = close_rect[0] + (close_rect[2] - close_text_w) * 0.5;
                    glyphs.extend(layout_panel_text(
                        TOPBAR_TAB_CLOSE,
                        &mut self.topbar_text_system,
                        &mut self.atlas,
                        &self.queue,
                        close_glyph_x,
                        origin_y,
                        close_color,
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
            }

            // ── Overflow indicator: a small "+N" label at the right edge when
            //    tabs are hidden. The separator of the last visible tab already
            //    sits immediately to its left.
            if overflow_count > 0 {
                let indicator_x =
                    topbar_visible_tab_x(&positions, last_visible, first_visible, tab_x)
                        + geoms[last_visible].width
                        + geoms[last_visible].separator;
                let indicator_text_w = estimate_monospace_width(&overflow_label, font_size);
                let indicator_text_x = indicator_x
                    + ((overflow_width - indicator_text_w) / 2.0).max(TOPBAR_TAB_PADDING_X);
                let start = glyphs.len() as u32;
                glyphs.extend(layout_panel_text(
                    &overflow_label,
                    &mut self.topbar_text_system,
                    &mut self.atlas,
                    &self.queue,
                    indicator_text_x,
                    origin_y,
                    inactive_fg,
                ));
                let count = glyphs.len() as u32 - start;
                if let Some(scissor) =
                    topbar_tab_text_scissor([indicator_x, content_y, overflow_width, content_h])
                {
                    if count > 0 {
                        text_batches.push(TextScissorBatch {
                            scissor,
                            range: InstanceDrawRange { start, count },
                        });
                    }
                }
            }
        }

        chrome.push(RegionDrawInstance::new(
            [bounds[0], bounds[1] + bounds[3] - 1.0, bounds[2], 1.0],
            border,
        ));

        self.topbar_icon_pipeline.upload_instances(
            &self.device,
            &self.topbar_icon_instances,
            [
                self.surface_state.config.width,
                self.surface_state.config.height,
            ],
        );
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

    pub fn topbar_tab_at_position(&self, position: (f32, f32)) -> Option<(usize, u64)> {
        topbar_tab_at_position(position, &self.topbar_tab_hitboxes)
    }

    /// Hit-test the per-tab close ("×") buttons. Returns the rendered buffer
    /// index + identity, validated by the caller before dispatching commands.
    pub fn topbar_close_at_position(&self, position: (f32, f32)) -> Option<(usize, u64)> {
        topbar_tab_at_position(position, &self.topbar_close_hitboxes)
    }

    /// True when the pointer is over the project/branch label in the topbar's
    /// left segment (click opens the Recent Projects palette).
    pub fn topbar_project_at_position(&self, position: (f32, f32)) -> bool {
        self.topbar_project_hitbox.is_some_and(|b| {
            position.0 >= b[0]
                && position.0 <= b[0] + b[2]
                && position.1 >= b[1]
                && position.1 <= b[1] + b[3]
        })
    }
}
