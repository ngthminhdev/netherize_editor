use std::{fs, path::{Path, PathBuf}, sync::OnceLock};

use cosmic_text::Metrics;

use crate::{
    render::{
        glyph_instance::GlyphInstance,
        icon_pipeline::{canonical_icon_id, IconDrawInstance},
        region_pipeline::RegionDrawInstance, renderer::Renderer,
    },
    terminal::grid::TerminalGrid,
    text::text_system::StyledTextSpan,
};

use super::super::{
    components::{
        layout_prefix_icon_badge, layout_shortcut_hint, push_centered_highlight_chip,
        HighlightChipStyle, PrefixIconBadge, PrefixIconBadgeChrome,
        ShortcutHintSegment,
    },
    helpers::{
        estimate_monospace_width, layout_clamp, layout_panel_text, layout_panel_text_bold,
        rect_to_scissor,
    },
};

struct BundledLogo {
    width: u32,
    height: u32,
    rgba: Vec<u8>,
}

fn welcome_project_git_branch(path: &Path) -> Option<String> {
    let git_dir = find_welcome_git_dir(path)?;
    let head = fs::read_to_string(git_dir.join("HEAD")).ok()?;
    parse_welcome_git_head(head.trim())
}

fn find_welcome_git_dir(start: &Path) -> Option<PathBuf> {
    for dir in start.ancestors() {
        let dot_git = dir.join(".git");
        if dot_git.is_dir() {
            return Some(dot_git);
        }
        if dot_git.is_file() {
            let raw = fs::read_to_string(&dot_git).ok()?;
            let gitdir = raw.trim().strip_prefix("gitdir:")?.trim();
            let gitdir_path = PathBuf::from(gitdir);
            return Some(if gitdir_path.is_absolute() {
                gitdir_path
            } else {
                dir.join(gitdir_path)
            });
        }
    }
    None
}

fn parse_welcome_git_head(head: &str) -> Option<String> {
    if let Some(reference) = head.strip_prefix("ref:") {
        return reference
            .trim()
            .strip_prefix("refs/heads/")
            .or_else(|| reference.trim().rsplit_once('/').map(|(_, branch)| branch))
            .map(str::to_string)
            .filter(|branch| !branch.is_empty());
    }

    (!head.is_empty()).then(|| {
        let short_len = head.len().min(7);
        format!("detached:{}", &head[..short_len])
    })
}



fn bundled_logo() -> Option<&'static BundledLogo> {
    static LOGO: OnceLock<Option<BundledLogo>> = OnceLock::new();
    LOGO.get_or_init(|| {
        let bytes = include_bytes!("../../../../assets/app_logo.png");
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
    pub fn update_welcome_screen_content(
        &mut self,
        _text: &str,
        _spans: &[StyledTextSpan],
        bounds: [f32; 4],
        recent_projects: &[PathBuf],
        selected_recent_index: usize,
    ) {
        if bounds[2] < 1.0 || bounds[3] < 1.0 {
            self.clear_welcome_logo();
            return;
        }

        self.welcome_logo_scissor = rect_to_scissor(bounds);
        let mut glyphs = Vec::new();
        let mut chrome = Vec::new();
        let mut icons = Vec::new();
        let bg = self.theme.editor.bg.as_f32();
        let panel = self.theme.ui.panel_bg.as_f32();
        let border = self.theme.ui.border_color.as_f32();
        let fg = self.theme.ui.fg.as_f32();
        let fg_dim = self.theme.ui.fg_dim.as_f32();
        let fg_ghost = self.theme.ui.fg_ghost.as_f32();
        let accent = self.theme.ui.accent.as_f32();
        let runtime_scale = (self.theme.editor.font_size / 14.0).max(0.5);
        let welcome_scale = layout_clamp(
            (self.welcome_card_max_width / 560.0) * runtime_scale,
            0.5,
            3.0,
        );
        let sx = |value: f32| value * welcome_scale;
        let text_w = |text: &str, size: f32| estimate_monospace_width(text, size);
        let centered_x = |center: f32, text: &str, size: f32| center - text_w(text, size) * 0.5;
        let ellipsize = |text: &str, size: f32, max_width: f32| -> String {
            if max_width <= 0.0 {
                return String::new();
            }
            if text_w(text, size) <= max_width {
                return text.to_string();
            }

            let suffix = "...";
            let suffix_width = text_w(suffix, size);
            if max_width <= suffix_width {
                return String::new();
            }

            let mut out = String::new();
            for ch in text.chars() {
                let next_width = text_w(&out, size) + text_w(ch.encode_utf8(&mut [0; 4]), size);
                if next_width + suffix_width > max_width {
                    break;
                }
                out.push(ch);
            }
            out.push_str(suffix);
            out
        };

        chrome.push(RegionDrawInstance::new(bounds, bg));

        let full_screen_width = self.surface_state.config.width.max(1) as f32;
        let two_column = bounds[2] > full_screen_width * 0.55;
        let left_w = if two_column { bounds[2] * 0.55 } else { bounds[2] };
        let divider_x = bounds[0] + left_w;
        let body_top = bounds[1];
        let body_h = bounds[3];

        if two_column {
            let mut divider = border;
            divider[3] = 0.65;
            chrome.push(RegionDrawInstance::new(
                [
                    divider_x,
                    body_top + sx(40.0),
                    sx(1.0).max(1.0),
                    (body_h - sx(80.0)).max(1.0),
                ],
                divider,
            ));
        }

        let line = |this: &mut Renderer,
                    glyphs: &mut Vec<GlyphInstance>,
                    s: &str,
                    x: f32,
                    y: f32,
                    size: f32,
                    lh: f32,
                    color: [f32; 4],
                    bold: bool| {
            this.welcome_logo_text_system
                .set_metrics(Metrics::new(size, lh));
            this.welcome_logo_text_system.set_size(None, Some(lh));
            if bold {
                glyphs.extend(layout_panel_text_bold(
                    s,
                    &mut this.welcome_logo_text_system,
                    &mut this.atlas,
                    &this.queue,
                    x,
                    y,
                    color,
                ));
            } else {
                glyphs.extend(layout_panel_text(
                    s,
                    &mut this.welcome_logo_text_system,
                    &mut this.atlas,
                    &this.queue,
                    x,
                    y,
                    color,
                ));
            }
        };

        let icon_badge = |this: &mut Renderer,
                          glyphs: &mut Vec<GlyphInstance>,
                          icons: &mut Vec<IconDrawInstance>,
                          chrome: &mut Vec<RegionDrawInstance>,
                          icon: &str,
                          color: [f32; 4],
                          panel_bg: [f32; 4],
                          bounds: [f32; 4],
                          scale: f32,
                          badge_chrome: PrefixIconBadgeChrome| {
            if matches!(badge_chrome, PrefixIconBadgeChrome::Outline) {
                let [x, y, w, h] = bounds;
                let radius = h * 0.22;
                let border = (h * 0.075).clamp(2.0, 3.0);
                let border_color = [
                    panel_bg[0] * 0.14 + color[0] * 0.86,
                    panel_bg[1] * 0.14 + color[1] * 0.86,
                    panel_bg[2] * 0.14 + color[2] * 0.86,
                    1.0,
                ];
                let bg_color = [
                    panel_bg[0] * 0.90 + color[0] * 0.10,
                    panel_bg[1] * 0.90 + color[1] * 0.10,
                    panel_bg[2] * 0.90 + color[2] * 0.10,
                    1.0,
                ];
                chrome.push(RegionDrawInstance::new(bounds, border_color).with_radius(radius));
                chrome.push(RegionDrawInstance::new([x + border, y + border, (w - border * 2.0).max(1.0), (h - border * 2.0).max(1.0)], bg_color).with_radius((radius - border).max(2.0)));
            }
            if let Some(asset_icon) = canonical_icon_id(icon) {
                let [x, y, w, h] = bounds;
                let size = (h * scale).max(10.0);
                icons.push(IconDrawInstance {
                    icon: asset_icon,
                    rect: [x + (w - size) * 0.5, y + (h - size) * 0.5, size, size],
                    tint: [1.0, 1.0, 1.0, 1.0],
                });
            } else {
                glyphs.extend(layout_prefix_icon_badge(
                    PrefixIconBadge {
                        icon,
                        color,
                        panel_bg,
                        bounds,
                        icon_scale: scale,
                        y_nudge_scale: 0.10,
                        chrome: PrefixIconBadgeChrome::None,
                    },
                    &mut this.welcome_logo_text_system,
                    &mut this.atlas,
                    &this.queue,
                    chrome,
                ));
            }
        };

        let key_hint = |this: &mut Renderer,
                        glyphs: &mut Vec<GlyphInstance>,
                        chrome: &mut Vec<RegionDrawInstance>,
                        keys: &[&str],
                        x: f32,
                        y: f32,
                        size: f32| {
            this.welcome_logo_text_system
                .set_metrics(Metrics::new(size, size * 1.18));
            glyphs.extend(layout_shortcut_hint(
                &[ShortcutHintSegment::Keys(keys)],
                &mut this.welcome_logo_text_system,
                &mut this.atlas,
                &this.queue,
                chrome,
                x,
                y,
                size,
                size * 1.18,
                fg_dim,
                panel,
                border,
                fg_dim,
            ));
        };

        let cx = bounds[0] + left_w * 0.5;
        let hero_top = body_top + sx(48.0);
        let logo_max_w = sx(170.0).min((left_w - sx(88.0)).max(sx(120.0)));
        let logo_max_h = sx(170.0);
        let logo_y = hero_top + sx(6.0);
        let mut logo_height = sx(120.0);

        if let Some(logo) = bundled_logo() {
            let scale = (logo_max_w / logo.width as f32)
                .min(logo_max_h / logo.height.max(1) as f32)
                .min(1.0);
            let draw_w = (logo.width as f32 * scale).max(1.0);
            let draw_h = (logo.height as f32 * scale).max(1.0);
            let rect = [cx - draw_w * 0.5, logo_y, draw_w, draw_h];
            self.welcome_image_scissor = self.welcome_logo_scissor;
            self.welcome_image_pipeline.upload_rgba(
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
            logo_height = draw_h;
        }

        let title = "Netherize";
        let title_size = sx(28.0);
        line(
            self,
            &mut glyphs,
            title,
            centered_x(cx, title, title_size),
            logo_y + logo_height + sx(12.0),
            title_size,
            sx(34.0),
            fg,
            true,
        );
        let tagline = "GPU-accelerated terminal editor";
        let tagline_size = sx(11.0);
        line(
            self,
            &mut glyphs,
            tagline,
            centered_x(cx, tagline, tagline_size),
            logo_y + logo_height + sx(50.0),
            tagline_size,
            sx(16.0),
            accent,
            true,
        );
        let meta_y = logo_y + logo_height + sx(86.0);
        let meta_size = sx(11.0);
        let meta_line_height = sx(16.0);
        let meta_chip_height = sx(30.0);
        let meta_chip_gap = sx(10.0);
        let meta_chip_padding_x = sx(14.0);
        let version_label = self.welcome_version.clone();
        let meta_chips = [
            (version_label, fg, true),
            ("built 2026-05-16".to_string(), fg_ghost, false),
            ("Rust 1.82.0".to_string(), fg_ghost, false),
        ];
        let meta_total_w: f32 = meta_chips
            .iter()
            .map(|(label, _, _)| text_w(label.as_str(), meta_size) + meta_chip_padding_x * 2.0)
            .sum::<f32>()
            + meta_chip_gap * (meta_chips.len().saturating_sub(1) as f32);
        let mut meta_center_x = cx - meta_total_w * 0.5;
        let mut meta_border = border;
        meta_border[3] = 0.92;
        let mut meta_fill = panel;
        meta_fill[3] = 0.98;

        for (label, color, bold) in &meta_chips {
            let label = label.as_str();
            let chip_w = text_w(label, meta_size) + meta_chip_padding_x * 2.0;
            push_centered_highlight_chip(
                &mut chrome,
                meta_center_x + chip_w * 0.5,
                meta_y - (meta_chip_height - meta_line_height) * 0.5,
                chip_w,
                meta_chip_height,
                HighlightChipStyle {
                    bg: meta_fill,
                    border: meta_border,
                    radius: sx(6.0),
                    border_thickness: sx(1.0),
                },
            );
            line(
                self,
                &mut glyphs,
                label,
                meta_center_x + (chip_w - text_w(label, meta_size)) * 0.5,
                meta_y,
                meta_size,
                meta_line_height,
                *color,
                *bold,
            );
            meta_center_x += chip_w + meta_chip_gap;
        }

        let available_card_w = (left_w - sx(64.0)).max(sx(260.0));
        let card_w = available_card_w.min(if two_column { sx(420.0) } else { sx(460.0) });
        let card_x = bounds[0] + ((left_w - card_w) * 0.5).max(sx(24.0));
        let card_h = sx(58.0);
        let card_gap = sx(10.0);
        let card_title_size = sx(13.0);
        let card_sub_size = sx(10.5);
        let section_size = sx(10.5);
        let mut card_y = meta_y + sx(58.0);
        let action_cards = [
            ("START", "built_in:file", self.theme.ui.cyan.as_f32(), "New Instance", "Open another editor window", &["⌘", "⇧", "N"][..], false),
            ("", "built_in:folder", self.theme.ui.info.as_f32(), "Open Folder", "Browse for a project folder", &["⌘", "O"][..], false),
            ("", "built_in:conf", self.theme.ui.magenta.as_f32(), "Command Palette", "Search commands, files, symbols", &["⌘", "P"][..], false),
        ];
        for (section, icon, color, title, sub, keys, active) in action_cards {
            if !section.is_empty() {
                line(
                    self,
                    &mut glyphs,
                    section,
                    card_x + sx(4.0),
                    card_y,
                    section_size,
                    sx(14.0),
                    fg_ghost,
                    true,
                );
                card_y += sx(22.0);
            }
            let mut card_fill = panel;
            card_fill[3] = if active { 0.98 } else { 0.72 };
            let mut card_border = if active { accent } else { border };
            card_border[3] = if active { 0.72 } else { 0.60 };
            chrome.push(RegionDrawInstance::new([card_x, card_y, card_w, card_h], card_border).with_radius(sx(8.0)));
            chrome.push(RegionDrawInstance::new([card_x + sx(1.0), card_y + sx(1.0), card_w - sx(2.0), card_h - sx(2.0)], card_fill).with_radius(sx(7.0)));
            icon_badge(
                self,
                &mut glyphs,
                &mut icons,
                &mut chrome,
                icon,
                color,
                panel,
                [card_x + sx(18.0), card_y + sx(11.0), sx(36.0), sx(36.0)],
                0.56,
                PrefixIconBadgeChrome::Outline,
            );
            line(self, &mut glyphs, title, card_x + sx(70.0), card_y + sx(14.0), card_title_size, sx(16.0), fg, true);
            line(self, &mut glyphs, sub, card_x + sx(70.0), card_y + sx(33.0), card_sub_size, sx(14.0), fg_ghost, false);
            if card_w >= sx(350.0) {
                let key_x = card_x + card_w - sx(122.0);
                key_hint(self, &mut glyphs, &mut chrome, keys, key_x, card_y + sx(17.0), sx(9.5));
            }
            card_y += card_h + card_gap;
        }

        if !two_column {
            let footer = "Press ⌘ ⇧ N for New Instance  |  Press ⌘ O for Open Folder";
            line(self, &mut glyphs, footer, centered_x(bounds[0] + bounds[2] * 0.5, footer, sx(10.0)), bounds[1] + bounds[3] - sx(30.0), sx(10.0), sx(13.0), fg_ghost, false);

            self.welcome_logo_chrome_instances = chrome;
            self.welcome_icon_instances = icons;
            self.welcome_icon_pipeline.upload_instances(
                &self.device,
                &self.welcome_icon_instances,
                [
                    self.surface_state.config.width,
                    self.surface_state.config.height,
                ],
            );
            self.welcome_logo_glyph_instances = glyphs;
            self.welcome_logo_text_pipeline.upload_instances(
                &self.device,
                &self.queue,
                &self.welcome_logo_glyph_instances,
            );
            return;
        }

        let rx = divider_x + sx(28.0);
        let rw = bounds[0] + bounds[2] - rx - sx(32.0);
        let mut y = body_top + sx(36.0);
        line(
            self,
            &mut glyphs,
            "RECENT PROJECTS",
            rx + sx(14.0),
            y,
            sx(15.0),
            sx(14.0),
            fg_ghost,
            true,
        );
        y += sx(25.0);
        if recent_projects.is_empty() {
            line(
                self,
                &mut glyphs,
                "No recent projects yet",
                rx + sx(14.0),
                y + sx(8.0),
                sx(12.0),
                sx(16.0),
                fg_dim,
                false,
            );
            line(
                self,
                &mut glyphs,
                "Open a folder with ⌘ O to pin it here.",
                rx + sx(14.0),
                y + sx(28.0),
                sx(10.0),
                sx(14.0),
                fg_ghost,
                false,
            );
            y += sx(74.0);
        }
        let mut recent_panel_fill = panel;
        recent_panel_fill[3] = 0.44;
        let mut recent_panel_border = border;
        recent_panel_border[3] = 0.45;
        let recent_panel_top = y - sx(8.0);
        let recent_rows = recent_projects.len().min(5).max(1) as f32;
        let recent_panel_h = sx(10.0) + recent_rows * sx(44.0);
        chrome.push(RegionDrawInstance::new([rx, recent_panel_top, rw, recent_panel_h], recent_panel_border).with_radius(sx(8.0)));
        chrome.push(RegionDrawInstance::new([rx + sx(1.0), recent_panel_top + sx(1.0), rw - sx(2.0), recent_panel_h - sx(2.0)], recent_panel_fill).with_radius(sx(7.0)));
        for (index, project) in recent_projects.iter().take(5).enumerate() {
            let name = project
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("unknown");
            let path = project.display().to_string();
            let icon_source = crate::app::persistence::AppPersistentState::infer_project_icon_source(project);
            let icon_name = Path::new(&icon_source)
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or(name);
            let project_icon = self.theme.icon_theme_for_filename(icon_name, false);
            let icon = project_icon.glyph.clone();
            let icon_color = project_icon.color.as_f32();
            let active = index == selected_recent_index;
            let tag = match index {
                0 => "2 min ago",
                1 => "18 min ago",
                2 => "1 hr ago",
                3 => "2 hrs ago",
                4 => "3 hrs ago",
                _ => "yesterday",
            };
            let tag_size = sx(10.0);
            let tag_x = rx + rw - sx(18.0) - text_w(tag, tag_size);
            let text_x = rx + sx(64.0);
            let text_right = (tag_x - sx(18.0)).max(text_x);
            let name_size = sx(13.0);
            let path_size = sx(10.0);
            let name_label = ellipsize(name, name_size, text_right - text_x);
            let path_label = ellipsize(&path, path_size, text_right - text_x - sx(70.0));
            let branch_label = welcome_project_git_branch(project)
                .map(|branch| format!(" {branch}"))
                .unwrap_or_else(|| " -".to_string());
            if active {
                let mut a = accent;
                a[3] = 0.20;
                chrome.push(
                    RegionDrawInstance::new([rx, y - 1.0, rw, sx(42.0)], a).with_radius(sx(7.0)),
                );
                chrome.push(
                    RegionDrawInstance::new([rx, y + sx(5.0), sx(3.0), sx(30.0)], accent)
                        .with_radius(2.0),
                );
            }
            icon_badge(
                self,
                &mut glyphs,
                &mut icons,
                &mut chrome,
                &icon,
                icon_color,
                panel,
                [rx + sx(16.0), y + sx(6.0), sx(30.0), sx(30.0)],
                0.82,
                PrefixIconBadgeChrome::None,
            );
            line(
                self,
                &mut glyphs,
                &name_label,
                text_x,
                y + sx(3.0),
                name_size,
                sx(16.0),
                if active { fg } else { fg_dim },
                active,
            );
            line(
                self,
                &mut glyphs,
                &path_label,
                text_x,
                y + sx(20.0),
                path_size,
                sx(13.0),
                fg_ghost,
                false,
            );
            line(
                self,
                &mut glyphs,
                &branch_label,
                text_x + text_w(&path_label, path_size) + sx(10.0),
                y + sx(20.0),
                path_size,
                sx(13.0),
                accent,
                false,
            );
            line(
                self,
                &mut glyphs,
                tag,
                tag_x,
                y + sx(12.0),
                tag_size,
                sx(13.0),
                fg_ghost,
                false,
            );
            y += sx(44.0);
        }
        // line(
        //     self,
        //     &mut glyphs,
        //     "show all  →",
        //     rx + sx(14.0),
        //     y + sx(2.0),
        //     10.0,
        //     14.0,
        //     accent,
        //     false,
        // );
        y += sx(34.0);
        line(self, &mut glyphs, "MORE ACTIONS", rx, y, sx(12.0), sx(14.0), fg_ghost, true);
        y += sx(24.0);
        let more_cards = [
            ("built_in:folder_open", self.theme.ui.warning.as_f32(), "Recent Projects", "Jump back into previous work", &["Space", "P", "J"][..], false),
            ("built_in:json", self.theme.ui.amber.as_f32(), "Search in Files", "Find text across project", &["Space", "F", "W"][..], false),
            ("built_in:todo", self.theme.ui.magenta.as_f32(), "Explore Extensions", "Themes, languages, tools", &["Space", "X"][..], false),
            ("built_in:conf", self.theme.ui.info.as_f32(), "Settings", "Configure editor behavior", &["⌘", ","][..], false),
        ];
        let more_card_h = sx(54.0);
        let more_gap = sx(10.0);
        for (icon, color, title, sub, keys, active) in more_cards {
            let mut card_fill = panel;
            card_fill[3] = if active { 0.90 } else { 0.52 };
            let mut card_border = if active { accent } else { border };
            card_border[3] = if active { 0.65 } else { 0.40 };
            chrome.push(RegionDrawInstance::new([rx, y, rw, more_card_h], card_border).with_radius(sx(8.0)));
            chrome.push(RegionDrawInstance::new([rx + sx(1.0), y + sx(1.0), rw - sx(2.0), more_card_h - sx(2.0)], card_fill).with_radius(sx(7.0)));
            icon_badge(
                self,
                &mut glyphs,
                &mut icons,
                &mut chrome,
                icon,
                color,
                panel,
                [rx + sx(16.0), y + sx(10.0), sx(34.0), sx(34.0)],
                0.54,
                PrefixIconBadgeChrome::Outline,
            );
            line(self, &mut glyphs, title, rx + sx(64.0), y + sx(12.0), sx(12.5), sx(15.0), fg, true);
            line(self, &mut glyphs, sub, rx + sx(64.0), y + sx(31.0), sx(10.0), sx(13.0), fg_ghost, false);
            if rw >= sx(330.0) {
                key_hint(self, &mut glyphs, &mut chrome, keys, rx + rw - sx(146.0), y + sx(16.0), sx(9.0));
            }
            y += more_card_h + more_gap;
        }
        let footer = "Press Space P J for Recent Projects  |  Press ⌘ , for Settings";
        line(self, &mut glyphs, footer, centered_x(bounds[0] + bounds[2] * 0.5, footer, sx(10.0)), bounds[1] + bounds[3] - sx(34.0), sx(10.0), sx(13.0), fg_ghost, false);

        self.welcome_logo_chrome_instances = chrome;
        self.welcome_icon_instances = icons;
        self.welcome_icon_pipeline.upload_instances(
            &self.device,
            &self.welcome_icon_instances,
            [
                self.surface_state.config.width,
                self.surface_state.config.height,
            ],
        );
        self.welcome_logo_glyph_instances = glyphs;
        self.welcome_logo_text_pipeline.upload_instances(
            &self.device,
            &self.queue,
            &self.welcome_logo_glyph_instances,
        );
    }

    /// Render ANSI art into the welcome-logo layer (separate from the real PTY panel).
    pub fn update_welcome_logo_content(&mut self, grid: &TerminalGrid, bounds: [f32; 4]) {
        if bounds[2] < 1.0 || bounds[3] < 1.0 {
            self.welcome_logo_scissor = None;
            self.welcome_logo_glyph_instances.clear();
            self.welcome_logo_chrome_instances.clear();
            self.welcome_logo_text_pipeline
                .upload_instances(&self.device, &self.queue, &[]);
            return;
        }

        self.welcome_logo_scissor = rect_to_scissor(bounds);
        self.welcome_logo_chrome_instances.clear();

        let width = bounds[2].max(1.0);
        let height = bounds[3].max(1.0);
        let cols = grid.cols.max(1) as f32;
        let rows = grid.rows.max(1) as f32;

        let font_size = (height / rows).min(width / (cols * 0.6)).max(1.0);
        let cell_width = (font_size * 0.6).max(1.0);
        let cell_height = font_size.max(1.0);
        let rendered_w = (cell_width * cols).min(width);
        let rendered_h = (cell_height * rows).min(height);
        let origin_x = bounds[0] + ((width - rendered_w) * 0.5).max(0.0);
        let origin_y = bounds[1] + ((height - rendered_h) * 0.5).max(0.0);

        use cosmic_text::Metrics;
        self.welcome_logo_text_system
            .set_metrics(Metrics::new(font_size, cell_height));

        self.welcome_logo_view_renderer.origin_x = origin_x;
        self.welcome_logo_view_renderer.origin_y = origin_y;
        self.welcome_logo_view_renderer.cell_width = cell_width;
        self.welcome_logo_view_renderer.cell_height = cell_height;
        self.welcome_logo_view_renderer.font_size = font_size;

        let default_fg = self.theme.editor.fg.as_f32();
        let default_bg = self.theme.editor.bg.as_f32();

        self.welcome_logo_glyph_instances = self.welcome_logo_view_renderer.build_instances(
            grid,
            &mut self.atlas,
            &self.queue,
            &mut self.welcome_logo_text_system,
            default_fg,
            default_bg,
            rendered_w,
        );
        self.welcome_logo_text_pipeline.upload_instances(
            &self.device,
            &self.queue,
            &self.welcome_logo_glyph_instances,
        );
    }

    pub fn clear_welcome_logo(&mut self) {
        self.welcome_logo_scissor = None;
        self.welcome_logo_glyph_instances.clear();
        self.welcome_logo_chrome_instances.clear();
        self.welcome_logo_text_pipeline
            .upload_instances(&self.device, &self.queue, &[]);
        self.welcome_image_pipeline.clear();
        self.welcome_image_scissor = None;
        self.welcome_icon_instances.clear();
        self.welcome_icon_pipeline.upload_instances(
            &self.device,
            &self.welcome_icon_instances,
            [
                self.surface_state.config.width,
                self.surface_state.config.height,
            ],
        );
    }

    // ── TopBar ─────────────────────────────────────────────────────────────────
}
