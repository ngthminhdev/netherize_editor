use std::path::PathBuf;

use cosmic_text::Metrics;

use crate::{
    render::{
        glyph_instance::GlyphInstance, region_pipeline::RegionDrawInstance, renderer::Renderer,
    },
    terminal::grid::TerminalGrid,
    text::text_system::StyledTextSpan,
};

use super::super::helpers::{
    estimate_monospace_width, layout_panel_text, layout_panel_text_bold, rect_to_scissor,
};

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
        let bg = self.theme.editor.bg.as_f32();
        let panel = self.theme.ui.panel_bg.as_f32();
        let border = self.theme.ui.border_color.as_f32();
        let fg = self.theme.ui.fg.as_f32();
        let fg_dim = self.theme.ui.fg_dim.as_f32();
        let fg_ghost = self.theme.ui.fg_ghost.as_f32();
        let accent = self.theme.ui.accent.as_f32();
        let welcome_scale = (self.welcome_card_max_width / 560.0).clamp(0.5, 3.0);
        let sx = |value: f32| value * welcome_scale;
        let text_w = |text: &str, size: f32| estimate_monospace_width(text, size);
        let centered_x = |center: f32, text: &str, size: f32| center - text_w(text, size) * 0.5;

        chrome.push(RegionDrawInstance::new(bounds, bg));

        let left_w = bounds[2] * 0.55;
        let divider_x = bounds[0] + left_w;
        let body_top = bounds[1];
        let body_h = bounds[3];
        let mut glow = accent;
        glow[3] = 0.055;
        chrome.push(
            RegionDrawInstance::new(
                [
                    bounds[0] + left_w * 0.38,
                    body_top + body_h * 0.20,
                    left_w * 0.52,
                    body_h * 0.30,
                ],
                glow,
            )
            .with_radius(sx(180.0)),
        );
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

        let cx = bounds[0] + left_w * 0.5;
        let hero_top = body_top + body_h * 0.5 - sx(205.0);
        let logo_size = sx(86.0);
        let lx = cx - logo_size * 0.5;
        let ly = hero_top;
        let mut logo_glow = accent;
        logo_glow[3] = 0.14;
        chrome.push(
            RegionDrawInstance::new(
                [
                    lx - sx(18.0),
                    ly - sx(18.0),
                    logo_size + sx(36.0),
                    logo_size + sx(36.0),
                ],
                logo_glow,
            )
            .with_radius(sx(70.0)),
        );
        // Geometric logo approximation: brackets + double chevrons from the HTML SVG.
        let s = logo_size / 500.0;
        for r in [
            [130.0, 130.0, 25.0, 240.0],
            [130.0, 130.0, 65.0, 25.0],
            [130.0, 345.0, 65.0, 25.0],
            [345.0, 130.0, 25.0, 90.0],
            [305.0, 130.0, 65.0, 25.0],
            [345.0, 245.0, 25.0, 125.0],
            [305.0, 345.0, 65.0, 25.0],
            [205.0, 215.0, 110.0, 28.0],
            [225.0, 265.0, 110.0, 28.0],
        ] {
            chrome.push(
                RegionDrawInstance::new([lx + r[0] * s, ly + r[1] * s, r[2] * s, r[3] * s], accent)
                    .with_radius(sx(3.0)),
            );
        }

        let title = "Netherize";
        let title_size = sx(32.0);
        line(
            self,
            &mut glyphs,
            title,
            centered_x(cx, title, title_size),
            hero_top + sx(118.0),
            title_size,
            sx(38.0),
            fg,
            true,
        );
        let tagline = "GPU · Zero Latency · Keyboard Driven";
        let tagline_size = sx(11.0);
        line(
            self,
            &mut glyphs,
            tagline,
            centered_x(cx, tagline, tagline_size),
            hero_top + sx(156.0),
            tagline_size,
            sx(16.0),
            accent,
            true,
        );
        let meta_y = hero_top + sx(212.0);
        chrome.push(
            RegionDrawInstance::new(
                [cx - sx(190.0), meta_y - sx(2.0), sx(72.0), sx(20.0)],
                panel,
            )
            .with_radius(sx(4.0)),
        );
        chrome.push(
            RegionDrawInstance::new(
                [cx - sx(190.0), meta_y - sx(2.0), sx(72.0), sx(20.0)],
                border,
            )
            .with_radius(sx(4.0)),
        );
        let meta_prefix = format!(
            "{}   ·   Rust 1.78   ·   wgpu 0.20   ·   by ",
            self.welcome_version
        );
        let meta_author = "ngthminhdev";
        let meta_size = sx(11.0);
        let meta_gap = sx(3.0);
        let meta_w = text_w(&meta_prefix, meta_size) + text_w(meta_author, meta_size) + meta_gap;
        let meta_x = cx - meta_w * 0.5;
        line(
            self,
            &mut glyphs,
            &meta_prefix,
            meta_x,
            meta_y,
            meta_size,
            sx(16.0),
            fg_ghost,
            false,
        );
        line(
            self,
            &mut glyphs,
            meta_author,
            meta_x + text_w(&meta_prefix, meta_size) + meta_gap,
            meta_y,
            meta_size,
            sx(16.0),
            fg,
            true,
        );

        let actions = [
            ("New buffer", ":new"),
            ("Open project", "⌘ O"),
            ("File finder", "spc ff"),
            ("Word search", "spc fw"),
        ];
        let action_gap = sx(10.0);
        let action_width = |label: &str, key: &str| {
            (sx(112.0) + key.chars().count() as f32 * sx(5.0))
                .max(sx(32.0) + text_w(label, sx(12.0)) + text_w(key, sx(10.0)))
        };
        let total_actions_w: f32 = actions
            .iter()
            .map(|(label, key)| action_width(label, key))
            .sum::<f32>()
            + action_gap * (actions.len().saturating_sub(1) as f32);
        let mut ax = cx - total_actions_w * 0.5;
        let ay = meta_y + sx(58.0);
        for (label, key) in actions {
            let w = action_width(label, key);
            chrome.push(RegionDrawInstance::new([ax, ay, w, sx(32.0)], panel).with_radius(sx(7.0)));
            chrome
                .push(RegionDrawInstance::new([ax, ay, w, sx(32.0)], border).with_radius(sx(7.0)));
            line(
                self,
                &mut glyphs,
                label,
                ax + sx(12.0),
                ay + sx(8.0),
                sx(12.0),
                sx(16.0),
                fg_dim,
                false,
            );
            let kw = key.chars().count() as f32 * sx(6.5) + sx(12.0);
            let mut abg = accent;
            abg[3] = 0.12;
            chrome.push(
                RegionDrawInstance::new([ax + w - kw - sx(10.0), ay + sx(8.0), kw, sx(16.0)], abg)
                    .with_radius(sx(3.0)),
            );
            line(
                self,
                &mut glyphs,
                key,
                ax + w - kw - sx(4.0),
                ay + sx(8.0),
                sx(10.0),
                sx(14.0),
                accent,
                true,
            );
            ax += w + action_gap;
        }
        let rust_line = "100% Rust · entire editor rendered on the GPU";
        let rust_line_size = sx(10.5);
        line(
            self,
            &mut glyphs,
            rust_line,
            centered_x(cx, rust_line, rust_line_size),
            ay + sx(76.0),
            rust_line_size,
            sx(17.0),
            fg_ghost,
            false,
        );
        let no_electron_line = "no Electron · no compromise";
        let no_electron_size = sx(10.5);
        line(
            self,
            &mut glyphs,
            no_electron_line,
            centered_x(cx, no_electron_line, no_electron_size),
            ay + sx(94.0),
            no_electron_size,
            sx(17.0),
            fg_ghost,
            false,
        );

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
        for (index, project) in recent_projects.iter().take(5).enumerate() {
            let name = project
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("unknown");
            let path = project.display().to_string();
            let lang = project
                .extension()
                .and_then(|ext| ext.to_str())
                .map(|ext| ext.to_ascii_uppercase())
                .unwrap_or_else(|| "DIR".to_string());
            let active = index == selected_recent_index;
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
            chrome.push(
                RegionDrawInstance::new([rx + sx(14.0), y + sx(9.0), sx(34.0), sx(16.0)], panel)
                    .with_radius(sx(3.0)),
            );
            line(
                self,
                &mut glyphs,
                &lang,
                rx + sx(19.0),
                y + sx(10.0),
                sx(9.0),
                sx(12.0),
                if lang == "TS" {
                    self.theme.ui.info.as_f32()
                } else {
                    accent
                },
                true,
            );
            line(
                self,
                &mut glyphs,
                name,
                rx + sx(60.0),
                y + sx(3.0),
                sx(13.0),
                sx(16.0),
                if active { fg } else { fg_dim },
                active,
            );
            line(
                self,
                &mut glyphs,
                &path,
                rx + sx(60.0),
                y + sx(20.0),
                sx(10.0),
                sx(13.0),
                fg_ghost,
                false,
            );
            line(
                self,
                &mut glyphs,
                if index == 0 { "latest" } else { "recent" },
                rx + rw - sx(70.0),
                y + sx(12.0),
                sx(10.0),
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
        y += sx(54.0);
        line(
            self,
            &mut glyphs,
            "KEYBOARD SHORTCUTS",
            rx,
            y,
            sx(15.0),
            sx(14.0),
            fg_ghost,
            true,
        );
        y += sx(24.0);
        let shortcuts = [
            (vec!["⌘", "O"], "Open file or project"),
            (vec!["j", "k", "enter"], "Select recent project"),
            (vec!["<space>", "f", "f"], "File picker (fuzzy find)"),
            (vec!["<space>", "f", "w"], "Find word / grep (fzf)"),
        ];
        for (keys, label) in shortcuts {
            let row_y = y;
            let mut kx = rx;
            for k in keys {
                let kw = k.chars().count() as f32 * sx(6.5) + sx(14.0);
                chrome.push(
                    RegionDrawInstance::new([kx, row_y, kw, sx(22.0)], panel).with_radius(sx(5.0)),
                );
                chrome.push(
                    RegionDrawInstance::new([kx, row_y + sx(21.0), kw, sx(2.0)], bg)
                        .with_radius(sx(2.0)),
                );
                line(
                    self,
                    &mut glyphs,
                    k,
                    kx + sx(7.0),
                    row_y + sx(4.0),
                    sx(11.0),
                    sx(14.0),
                    fg,
                    true,
                );
                kx += kw + sx(4.0);
            }
            line(
                self,
                &mut glyphs,
                label,
                rx + sx(172.0),
                row_y + sx(4.0),
                sx(12.5),
                sx(16.0),
                fg_dim,
                false,
            );
            let mut sub = border;
            sub[3] = 0.45;
            chrome.push(RegionDrawInstance::new(
                [rx, row_y + sx(34.0), rw, sx(1.0).max(1.0)],
                sub,
            ));
            y += sx(40.0);
        }
        line(
            self,
            &mut glyphs,
            "press ? for all bindings   ·   :help for docs",
            rx,
            y + sx(8.0),
            sx(14.0),
            sx(14.0),
            fg_ghost,
            false,
        );

        self.welcome_logo_chrome_instances = chrome;
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
    }

    // ── TopBar ─────────────────────────────────────────────────────────────────
}
