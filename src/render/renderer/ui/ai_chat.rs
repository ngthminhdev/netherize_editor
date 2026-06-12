//! Right-sidebar AI chat and markdown preview text rendering.

use std::sync::OnceLock;

use super::utils::{
    blend_rgb, clip_styled_span_to_range, slash_suggestion_rect, with_alpha, word_wrap,
    word_wrap_with_ranges,
};

use crate::{
    render::{
        glyph_instance::GlyphInstance,
        region_pipeline::RegionDrawInstance,
        renderer::{
            Renderer, TextScissorBatch,
            helpers::{
                estimate_monospace_width, layout_clamp, layout_panel_rich_text, layout_panel_text,
                layout_panel_text_bold, layout_panel_text_italic, rect_to_scissor,
            },
        },
    },
    text::text_system::StyledTextSpan,
    workbench::panel_state::{AiChatMessage, AiRole},
};

struct AiChatLogo {
    width: u32,
    height: u32,
    rgba: Vec<u8>,
}

fn bundled_ai_chat_logo() -> Option<&'static AiChatLogo> {
    static LOGO: OnceLock<Option<AiChatLogo>> = OnceLock::new();
    LOGO.get_or_init(|| {
        let bytes = include_bytes!("../../../../assets/app_logo.png");
        let decoded = image::load_from_memory(bytes).ok()?;
        let rgba = decoded.to_rgba8();
        let width = rgba.width();
        let height = rgba.height();
        let raw = rgba.into_raw();

        let mut min_x = width;
        let mut min_y = height;
        let mut max_x = 0;
        let mut max_y = 0;
        for y in 0..height {
            for x in 0..width {
                let alpha_idx = ((y * width + x) as usize) * 4 + 3;
                if raw.get(alpha_idx).copied().unwrap_or(0) > 8 {
                    min_x = min_x.min(x);
                    min_y = min_y.min(y);
                    max_x = max_x.max(x);
                    max_y = max_y.max(y);
                }
            }
        }

        if min_x <= max_x && min_y <= max_y {
            let crop_w = max_x - min_x + 1;
            let crop_h = max_y - min_y + 1;
            let mut cropped = Vec::with_capacity((crop_w * crop_h * 4) as usize);
            for y in min_y..=max_y {
                let start = ((y * width + min_x) * 4) as usize;
                let end = start + (crop_w * 4) as usize;
                cropped.extend_from_slice(&raw[start..end]);
            }
            return Some(AiChatLogo {
                width: crop_w,
                height: crop_h,
                rgba: cropped,
            });
        }

        Some(AiChatLogo {
            width,
            height,
            rgba: raw,
        })
    })
    .as_ref()
}

/// Strip ANSI escape sequences from opencode run output.
fn strip_ansi(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\x1b' && chars.peek() == Some(&'[') {
            chars.next(); // consume '['
            while let Some(&ch) = chars.peek() {
                chars.next();
                if ch.is_ascii_alphabetic() {
                    break;
                }
            }
        } else if c != '\r' {
            out.push(c);
        }
    }
    out
}

fn is_opencode_status_line(line: &str) -> bool {
    let trimmed = line.trim();
    trimmed == "[0m"
        || (trimmed.starts_with("> ")
            && (trimmed.contains("build ·")
                || trimmed.contains("build •")
                || trimmed.contains("mimo-")
                || trimmed.contains("tokens")))
}

const SLASH_COMMAND_SUGGESTIONS: &[(&str, &str)] = &[
    ("/clear", "clear chat history"),
    ("/new", "start a fresh chat"),
    ("/review", "review this code for bugs / improvements"),
    ("/explain", "explain the current file or selection"),
    ("/fix", "fix an issue in the current context"),
    ("/test", "generate unit tests for the current code"),
    ("/commit", "write a git commit message for staged changes"),
    ("/diff", "ask AI to summarise the current git diff"),
    ("/context", "show attached file contexts"),
    ("/plan", "switch to plan mode"),
    ("/build", "switch to build mode"),
    ("/mode", "show or set build/plan"),
    ("/agent", "alias for /mode"),
    ("/model", "show or set the active model"),
    ("/models", "show common model ids"),
    ("/status", "show current chat settings"),
    ("/compact", "summarize chat context"),
    ("/tokens", "show context/token usage hint"),
    ("/help", "show command help"),
];

const SUGGESTION_WINDOW: usize = 8;

fn slash_command_suggestions(input_buffer: &str) -> Vec<(&'static str, &'static str)> {
    let Some(rest) = input_buffer.trim_start().strip_prefix('/') else {
        return Vec::new();
    };
    let query = rest
        .split_whitespace()
        .next()
        .unwrap_or(rest)
        .to_ascii_lowercase();
    SLASH_COMMAND_SUGGESTIONS
        .iter()
        .copied()
        .filter(|(command, _)| {
            command
                .trim_start_matches('/')
                .to_ascii_lowercase()
                .starts_with(&query)
        })
        .collect()
}

fn current_at_token(input_buffer: &str) -> Option<&str> {
    let trimmed = input_buffer.trim_end();
    if trimmed.is_empty() {
        return None;
    }
    let start = trimmed
        .char_indices()
        .rev()
        .find(|(_, ch)| ch.is_whitespace())
        .map(|(idx, ch)| idx + ch.len_utf8())
        .unwrap_or(0);
    trimmed[start..].strip_prefix('@')
}

/// Returns `(visible_items, local_selected_index)`.
/// Applies a sliding window of [`SUGGESTION_WINDOW`] so the selected item
/// is always visible even when there are more matches than the window size.
fn ai_chat_input_suggestions(
    input_buffer: &str,
    file_suggestions: &[(String, String)],
    selected_index: usize,
) -> (Vec<(String, String)>, usize) {
    let all: Vec<(String, String)> = if current_at_token(input_buffer).is_some() {
        file_suggestions
            .iter()
            .map(|(path, detail)| (format!("@{path}"), detail.clone()))
            .collect()
    } else {
        slash_command_suggestions(input_buffer)
            .into_iter()
            .map(|(cmd, desc)| (cmd.to_string(), desc.to_string()))
            .collect()
    };

    let total = all.len();
    if total == 0 {
        return (Vec::new(), 0);
    }

    let sel = selected_index.min(total - 1);
    if total <= SUGGESTION_WINDOW {
        return (all, sel);
    }

    // Scroll the window so the selected item is always the last visible row
    // when scrolling down, and the first when near the top.
    let win_start = sel.saturating_sub(SUGGESTION_WINDOW - 1);
    let win_end = (win_start + SUGGESTION_WINDOW).min(total);
    (all[win_start..win_end].to_vec(), sel - win_start)
}

// ── AI Chat text rendering ──────────────────────────────────────────────────

/// Parse AI message text into styled lines, detecting fenced code blocks
/// and inline code spans with appropriate coloring.
/// Backticks are stripped from display — code blocks show with syntax highlighting,
/// inline code shows with distinct color.
fn build_styled_message_lines(
    text: &str,
    max_chars: usize,
    default_color: [u8; 4],
    code_text_color: [u8; 4],
    inline_code_color: [u8; 4],
    _dim_color: [u8; 4],
    theme: &crate::config::theme_config::ThemeConfig,
) -> (Vec<String>, Vec<Vec<StyledTextSpan>>, Vec<bool>) {
    let mut lines = Vec::new();
    let mut line_styles = Vec::new();
    let mut code_rows = Vec::new();
    let mut in_code_block = false;
    let mut code_lang = String::new();

    for raw_line in text.split('\n') {
        let trimmed = raw_line.trim();

        // Detect fenced code block boundaries — skip the ``` lines entirely
        if trimmed.starts_with("```") {
            if !in_code_block {
                in_code_block = true;
                code_lang = trimmed.trim_start_matches('`').trim().to_string();
            } else {
                in_code_block = false;
                code_lang.clear();
            }
            continue;
        }

        if !in_code_block
            && trimmed.chars().all(|ch| ch == '-' || ch.is_whitespace())
            && trimmed.chars().filter(|ch| *ch == '-').count() >= 3
        {
            lines.push("─".repeat(max_chars.min(48)));
            line_styles.push(vec![StyledTextSpan::new(
                0,
                max_chars.min(48),
                inline_code_color,
            )]);
            code_rows.push(false);
            continue;
        }

        let is_table_row = !in_code_block && trimmed.starts_with('|') && trimmed.ends_with('|');
        let is_table_separator = is_table_row
            && trimmed
                .chars()
                .all(|ch| matches!(ch, '|' | '-' | ':' | ' ' | '\t'));
        if is_table_separator {
            continue;
        }

        // Wrap the line
        for wrapped in word_wrap(raw_line, max_chars) {
            let line_str = wrapped.clone();

            if in_code_block {
                // Code block line: syntax highlight or fallback to code_bg_color
                let mut spans = Vec::new();
                if !code_lang.is_empty() {
                    let ext = match code_lang.as_str() {
                        "rust" | "rs" => "rs",
                        "javascript" | "js" => "js",
                        "typescript" | "ts" => "ts",
                        "tsx" => "tsx",
                        "jsx" => "jsx",
                        "go" => "go",
                        "py" | "python" => "py",
                        "json" => "json",
                        "yaml" | "yml" => "yaml",
                        "bash" | "sh" | "zsh" => "sh",
                        "sql" => "sql",
                        "toml" => "toml",
                        "css" => "css",
                        "html" | "htm" => "html",
                        "md" | "markdown" => "md",
                        _ => "",
                    };
                    if !ext.is_empty() {
                        let highlight_spans =
                            crate::syntax::highlight::highlight_snippet(&line_str, ext, theme);
                        for hs in &highlight_spans {
                            let color = highlight_category_color(hs.category);
                            spans.push(StyledTextSpan::new(hs.range.start, hs.range.end, color));
                        }
                    }
                }
                if spans.is_empty() {
                    spans.push(StyledTextSpan::new(0, line_str.len(), code_text_color));
                }
                lines.push(line_str);
                line_styles.push(spans);
                code_rows.push(true);
            } else {
                let mut line_str = line_str;
                let mut prefix = String::new();
                let mut prefix_color = inline_code_color;
                let mut make_bold = false;

                let trimmed_line = line_str.trim_start();
                let leading_ws = line_str.len().saturating_sub(trimmed_line.len());
                if let Some(rest) = trimmed_line.strip_prefix("### ") {
                    prefix.clear();
                    line_str = rest.to_string();
                    make_bold = true;
                    prefix_color = default_color;
                } else if let Some(rest) = trimmed_line.strip_prefix("## ") {
                    prefix.clear();
                    line_str = rest.to_string();
                    make_bold = true;
                    prefix_color = default_color;
                } else if let Some(rest) = trimmed_line.strip_prefix("# ") {
                    prefix.clear();
                    line_str = rest.to_string();
                    make_bold = true;
                    prefix_color = default_color;
                } else if let Some(rest) = trimmed_line.strip_prefix("> ") {
                    prefix = "│ ".to_string();
                    line_str = rest.to_string();
                    prefix_color = inline_code_color;
                } else if is_table_row {
                    line_str = trimmed_line
                        .trim_matches('|')
                        .split('|')
                        .map(str::trim)
                        .collect::<Vec<_>>()
                        .join("  │  ");
                    make_bold = raw_line.lines().next().is_some_and(|_| false);
                    prefix_color = inline_code_color;
                } else if leading_ws > 0 {
                    line_str = trimmed_line.to_string();
                }

                // Regular line: detect inline code, bold (**...**), italic (*...*)
                let mut spans = Vec::new();
                let mut clean = String::new();
                if !prefix.is_empty() {
                    let start = clean.len();
                    clean.push_str(&prefix);
                    spans.push(StyledTextSpan::new(start, clean.len(), prefix_color));
                }
                let chars: Vec<char> = line_str.chars().collect();
                let mut i = 0;
                while i < chars.len() {
                    // Inline code: `...`
                    if chars[i] == '`' {
                        let start = i + 1;
                        let mut end = start;
                        while end < chars.len() && chars[end] != '`' {
                            end += 1;
                        }
                        if end < chars.len() && end > start {
                            let code_start = clean.len();
                            for ch in &chars[start..end] {
                                clean.push(*ch);
                            }
                            let code_end = clean.len();
                            spans.push(StyledTextSpan::new(
                                code_start,
                                code_end,
                                inline_code_color,
                            ));
                            i = end + 1;
                            continue;
                        }
                    }
                    // Bold: **...**
                    if chars[i] == '*' && i + 1 < chars.len() && chars[i + 1] == '*' {
                        let start = i + 2;
                        let mut end = start;
                        while end + 1 < chars.len() && !(chars[end] == '*' && chars[end + 1] == '*')
                        {
                            end += 1;
                        }
                        if end + 1 < chars.len() && end > start {
                            let bold_start = clean.len();
                            for ch in &chars[start..end] {
                                clean.push(*ch);
                            }
                            let bold_end = clean.len();
                            spans.push(StyledTextSpan::with_style(
                                bold_start,
                                bold_end,
                                default_color,
                                true,
                                false,
                            ));
                            i = end + 2;
                            continue;
                        }
                    }
                    // Italic: *...*  (single asterisk, not followed by another *)
                    if chars[i] == '*' && (i + 1 >= chars.len() || chars[i + 1] != '*') {
                        let start = i + 1;
                        let mut end = start;
                        while end < chars.len() && chars[end] != '*' {
                            end += 1;
                        }
                        if end < chars.len() && end > start {
                            let italic_start = clean.len();
                            for ch in &chars[start..end] {
                                clean.push(*ch);
                            }
                            let italic_end = clean.len();
                            spans.push(StyledTextSpan::with_style(
                                italic_start,
                                italic_end,
                                default_color,
                                false,
                                true,
                            ));
                            i = end + 1;
                            continue;
                        }
                    }
                    clean.push(chars[i]);
                    i += 1;
                }
                if make_bold && !clean.is_empty() {
                    spans.push(StyledTextSpan::with_style(
                        0,
                        clean.len(),
                        default_color,
                        true,
                        false,
                    ));
                }
                lines.push(clean);
                line_styles.push(spans);
                code_rows.push(false);
            }
        }
    }

    (lines, line_styles, code_rows)
}

fn highlight_category_color(category: crate::syntax::highlight::HighlightCategory) -> [u8; 4] {
    use crate::syntax::highlight::HighlightCategory;
    match category {
        HighlightCategory::Keyword => [0xCC, 0x78, 0x32, 0xFF], // orange
        HighlightCategory::String => [0x6A, 0x87, 0x59, 0xFF],  // green
        HighlightCategory::Comment => [0x80, 0x80, 0x80, 0xFF], // gray
        HighlightCategory::Function => [0xFF, 0xC6, 0x6D, 0xFF], // yellow
        HighlightCategory::Type => [0x68, 0x97, 0xBB, 0xFF],    // blue
        HighlightCategory::Constant => [0x98, 0x76, 0xAA, 0xFF], // purple
        HighlightCategory::Number => [0x68, 0x97, 0xBB, 0xFF],  // blue
        HighlightCategory::Operator => [0xA9, 0xB7, 0xC6, 0xFF], // light gray
        HighlightCategory::Punctuation => [0xA9, 0xB7, 0xC6, 0xFF],
        HighlightCategory::Attribute => [0xBB, 0xB5, 0x29, 0xFF], // olive
        HighlightCategory::Tag => [0xE8, 0xBF, 0xD3, 0xFF],       // light green
        HighlightCategory::Escape => [0xCC, 0x78, 0x32, 0xFF],    // orange
        _ => [0xA9, 0xB7, 0xC6, 0xFF],                            // default light gray
    }
}

impl Renderer {
    /// Render chat history + input text for the AI Chat panel.
    /// Returns cursor quads to be drawn via the region pipeline.
    pub fn update_ai_chat_content(
        &mut self,
        history_bounds: [f32; 4],
        input_bounds: [f32; 4],
        messages: &[AiChatMessage],
        input_buffer: &str,
        file_suggestions: &[(String, String)],
        selected_suggestion_index: usize,
        show_cursor: bool,
        inner_padding: f32,
        is_opencode_missing: bool,
        active_model: Option<&str>,
        active_agent: &str,
        is_generating: bool,
        scroll_y: f32,
    ) -> (Vec<RegionDrawInstance>, f32) {
        if history_bounds[2] < 1.0
            || history_bounds[3] < 1.0
            || input_bounds[2] < 1.0
            || input_bounds[3] < 1.0
        {
            self.clear_ai_chat();
            return (Vec::new(), 0.0);
        }

        // ── Missing-binary banner ─────────────────────────────────────────
        if is_opencode_missing {
            self.ai_chat_image_scissor = None;
            self.ai_chat_header_image_pipeline.clear();
            self.ai_chat_hero_image_pipeline.clear();
            let font_size = self.theme.ui.sidebar_font_size;
            let line_h = self.theme.ui.sidebar_line_height.max(font_size + 4.0);
            let warn_color = self.theme.ui.warning.as_f32();

            let hclip = [
                history_bounds[0] + inner_padding,
                history_bounds[1] + inner_padding,
                (history_bounds[2] - inner_padding * 2.0).max(1.0),
                (history_bounds[3] - inner_padding * 2.0).max(1.0),
            ];
            self.ai_chat_history_scissor = rect_to_scissor(hclip);
            self.ai_chat_input_scissor = None;
            self.ai_chat_text_system
                .set_size(Some(hclip[2].max(1.0)), Some(line_h));

            let banner_lines: &[&str] = &[
                "OpenCode CLI is missing.",
                "Please install it to use AI workflows.",
                "Press Y in the install prompt to continue.",
            ];
            let total_banner_h = banner_lines.len() as f32 * line_h;
            let start_y = hclip[1] + (hclip[3] - total_banner_h) * 0.5;
            let mut all: Vec<GlyphInstance> = Vec::new();
            for (i, &line) in banner_lines.iter().enumerate() {
                let y = start_y + i as f32 * line_h;
                let gs = layout_panel_text(
                    line,
                    &mut self.ai_chat_text_system,
                    &mut self.atlas,
                    &self.queue,
                    hclip[0],
                    y,
                    warn_color,
                );
                all.extend(gs);
            }
            self.ai_chat_glyph_instances = all;
            self.ai_chat_text_pipeline.upload_instances(
                &self.device,
                &self.queue,
                &self.ai_chat_glyph_instances,
            );
            self.ai_chat_input_batch = None;
            return (Vec::new(), 0.0);
        }

        let font_size = self.theme.ui.sidebar_font_size;
        let line_h = self.theme.ui.sidebar_line_height.max(font_size + 4.0);
        let accent = self.theme.ui.accent.as_f32();
        let fg = self.theme.ui.fg.as_f32();
        let fg_dim = self.theme.ui.fg_dim.as_f32();
        let warning = self.theme.ui.warning.as_f32();
        let success = self.theme.ui.success.as_f32();
        let panel_bg = self.theme.ui.panel_bg.as_f32();
        let editor_bg = self.theme.editor.bg.as_f32();

        use crate::config::theme_config::linear_rgba_to_srgb_u8;
        let accent_u8 = linear_rgba_to_srgb_u8(accent);
        let success_u8 = linear_rgba_to_srgb_u8(success);

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

        let model_label = active_model.unwrap_or("default");
        let status_label = if is_generating { "running" } else { "ready" };
        let header_h = (line_h + 22.0).min((hclip[3] * 0.4).max(line_h + 8.0));
        let header_y = hclip[1] + (header_h - line_h) * 0.5;
        let body_y = hclip[1] + header_h;
        let body_h = (hclip[3] - header_h).max(1.0);
        self.ai_chat_image_scissor = rect_to_scissor(hclip);

        let mut chrome = Vec::new();
        chrome.push(RegionDrawInstance::new(
            [hclip[0], hclip[1], hclip[2], header_h],
            with_alpha(panel_bg, 0.28),
        ));
        chrome.push(RegionDrawInstance::new(
            [hclip[0], body_y - 1.0, hclip[2], 1.0],
            with_alpha(accent, 0.18),
        ));
        chrome.push(
            RegionDrawInstance::new(
                [
                    input_bounds[0] - 3.0,
                    input_bounds[1] - 3.0,
                    input_bounds[2] + 6.0,
                    input_bounds[3] + 6.0,
                ],
                with_alpha(accent, 0.14),
            )
            .with_radius(12.0),
        );
        chrome.push(
            RegionDrawInstance::new(input_bounds, with_alpha(editor_bg, 0.90)).with_radius(10.0),
        );

        self.ai_chat_header_image_pipeline.clear();
        let header_text_x = hclip[0] + 12.0;

        let header_text =
            format!("netherize  •  {active_agent}  •  {model_label}  •  {status_label}");
        let status_start = header_text.len().saturating_sub(status_label.len());
        let header_spans = [
            StyledTextSpan {
                start: 0,
                end: "netherize".len(),
                color_rgba: accent_u8,
                bold: false,
                italic: false,
            },
            StyledTextSpan {
                start: status_start,
                end: header_text.len(),
                color_rgba: success_u8,
                bold: false,
                italic: false,
            },
        ];
        let button_size = (line_h + 10.0).clamp(28.0, 42.0);
        let button_rect = [
            hclip[0] + hclip[2] - button_size - 8.0,
            hclip[1] + (header_h - button_size) * 0.5,
            button_size,
            button_size,
        ];
        chrome.push(
            RegionDrawInstance::new(button_rect, blend_rgb(panel_bg, fg_dim, 0.18, 0.42))
                .with_radius(8.0),
        );
        let slider_x = button_rect[0] + button_size * 0.27;
        let slider_w = button_size * 0.46;
        for (idx, knob_t) in [0.25_f32, 0.68, 0.42].into_iter().enumerate() {
            let y = button_rect[1] + button_size * (0.34 + idx as f32 * 0.16);
            chrome.push(RegionDrawInstance::new(
                [slider_x, y, slider_w, 1.5],
                with_alpha(fg_dim, 0.46),
            ));
            chrome.push(
                RegionDrawInstance::new(
                    [slider_x + slider_w * knob_t - 2.0, y - 1.8, 4.0, 4.0],
                    with_alpha(accent, 0.78),
                )
                .with_radius(2.0),
            );
        }

        // ── Welcome placeholder (no messages yet) ─────────────────────────
        if messages.is_empty() {
            let dim = fg_dim;
            let mut all: Vec<GlyphInstance> = Vec::new();
            all.extend(layout_panel_rich_text(
                &header_text,
                &header_spans,
                fg_dim,
                &mut self.ai_chat_text_system,
                &mut self.atlas,
                &self.queue,
                header_text_x,
                header_y,
            ));

            let hero_logo_size = (hclip[2] * 0.18).clamp(42.0, 76.0);
            let hero_center_y = body_y + body_h * 0.44;
            if let Some(logo) = bundled_ai_chat_logo() {
                self.ai_chat_hero_image_pipeline.upload_rgba(
                    &self.device,
                    &self.queue,
                    &logo.rgba,
                    logo.width,
                    logo.height,
                    [
                        hclip[0] + (hclip[2] - hero_logo_size) * 0.5,
                        hero_center_y - hero_logo_size - line_h * 1.2,
                        hero_logo_size,
                        hero_logo_size,
                    ],
                    [
                        self.surface_state.config.width,
                        self.surface_state.config.height,
                    ],
                );
            } else {
                self.ai_chat_hero_image_pipeline.clear();
            }

            let title = "Hello! I'm Netherize.";
            let title_x = hclip[0] + (hclip[2] - estimate_monospace_width(title, font_size)) * 0.5;
            let title_y = hero_center_y;
            all.extend(layout_panel_text_bold(
                title,
                &mut self.ai_chat_text_system,
                &mut self.atlas,
                &self.queue,
                title_x,
                title_y,
                fg,
            ));

            let body_lines = [
                "Type a message below to start.",
                "Use @path to attach file context.",
            ];
            let mut line_y = title_y + line_h * 1.8;
            for line in body_lines {
                let x = hclip[0] + (hclip[2] - estimate_monospace_width(line, font_size)) * 0.5;
                all.extend(layout_panel_text(
                    line,
                    &mut self.ai_chat_text_system,
                    &mut self.atlas,
                    &self.queue,
                    x,
                    line_y,
                    dim,
                ));
                line_y += line_h * 1.35;
            }
            let hint = "Try /help, /models, or @src.";
            let help_start = hint.find("/help").unwrap_or(4);
            let models_start = hint.find("/models").unwrap_or(hint.len());
            let file_start = hint.find("@src").unwrap_or(hint.len());
            let hint_spans = [
                StyledTextSpan {
                    start: help_start,
                    end: help_start + "/help".len(),
                    color_rgba: accent_u8,
                    bold: false,
                    italic: false,
                },
                StyledTextSpan {
                    start: models_start,
                    end: models_start + "/models".len(),
                    color_rgba: accent_u8,
                    bold: false,
                    italic: false,
                },
                StyledTextSpan {
                    start: file_start,
                    end: file_start + "@src".len(),
                    color_rgba: accent_u8,
                    bold: false,
                    italic: false,
                },
            ];
            let hint_x = hclip[0] + (hclip[2] - estimate_monospace_width(hint, font_size)) * 0.5;
            all.extend(layout_panel_rich_text(
                hint,
                &hint_spans,
                dim,
                &mut self.ai_chat_text_system,
                &mut self.atlas,
                &self.queue,
                hint_x,
                line_y,
            ));
            self.ai_chat_glyph_instances = all;
            self.ai_chat_suggestion_chrome_instances.clear();
            self.ai_chat_suggestion_glyph_start = None;
            let (suggestions, _local_sel) = ai_chat_input_suggestions(
                input_buffer,
                file_suggestions,
                selected_suggestion_index,
            );
            if !suggestions.is_empty() {
                let suggestion_rect =
                    slash_suggestion_rect(input_bounds, hclip, line_h, suggestions.len());
                // Use a separate chrome vec so the popup background is drawn
                // after message text, preventing bubble glyphs from covering it.
                self.ai_chat_suggestion_chrome_instances.push(
                    RegionDrawInstance::new(suggestion_rect, self.theme.ui.border_color.as_f32())
                        .with_radius(9.0),
                );
                self.ai_chat_suggestion_chrome_instances.push(
                    RegionDrawInstance::new(
                        [
                            suggestion_rect[0] + 1.0,
                            suggestion_rect[1] + 1.0,
                            (suggestion_rect[2] - 2.0).max(1.0),
                            (suggestion_rect[3] - 2.0).max(1.0),
                        ],
                        blend_rgb(editor_bg, panel_bg, 0.38, 0.98),
                    )
                    .with_radius(8.0),
                );
                self.ai_chat_suggestion_glyph_start =
                    Some(self.ai_chat_glyph_instances.len() as u32);
                let mut suggestion_y = suggestion_rect[1] + 6.0;
                for (label, detail) in &suggestions {
                    let row_text = format!("{label:<24} {detail}");
                    let spans = [StyledTextSpan {
                        start: 0,
                        end: label.len(),
                        color_rgba: accent_u8,
                        bold: false,
                        italic: false,
                    }];
                    self.ai_chat_glyph_instances.extend(layout_panel_rich_text(
                        &row_text,
                        &spans,
                        fg_dim,
                        &mut self.ai_chat_text_system,
                        &mut self.atlas,
                        &self.queue,
                        suggestion_rect[0] + 10.0,
                        suggestion_y,
                    ));
                    suggestion_y += line_h;
                }
            }
            // Still render the input box prompt/cursor.
            let input_text = if input_buffer.is_empty() {
                "> ask netherize... (/help, @file)".to_string()
            } else {
                format!("> {input_buffer}")
            };
            let input_start = self.ai_chat_glyph_instances.len() as u32;
            let input_default = if input_buffer.is_empty() { fg_dim } else { fg };
            let input_spans = [StyledTextSpan {
                start: 0,
                end: 2,
                color_rgba: accent_u8,
                bold: false,
                italic: false,
            }];
            let mut input_glyphs = Vec::new();

            let input_content_w = iclip[2];
            let input_max_chars = ((input_content_w / (font_size * 0.6)) as usize).max(12);
            let wrapped_lines = if input_buffer.is_empty() {
                vec![input_text.clone()]
            } else {
                word_wrap(&input_text, input_max_chars)
            };

            // Show as many wrapped lines as fit in the input box height.
            let max_input_lines = ((iclip[3] - 4.0) / line_h).floor().max(1.0).min(5.0) as usize;
            let line_trim = wrapped_lines.len().saturating_sub(max_input_lines);
            let visible_lines = &wrapped_lines[line_trim..];
            let is_first_visible = line_trim == 0;

            let total_visible_h = visible_lines.len() as f32 * line_h;
            let iy =
                iclip[1] + layout_clamp((iclip[3] - total_visible_h) * 0.5, 4.0, inner_padding);

            let mut input_line_y = iy;
            for (idx, line) in visible_lines.iter().enumerate() {
                let spans: &[StyledTextSpan] = if idx == 0 && is_first_visible {
                    &input_spans
                } else {
                    &[]
                };
                let inp = layout_panel_rich_text(
                    line,
                    spans,
                    input_default,
                    &mut self.ai_chat_text_system,
                    &mut self.atlas,
                    &self.queue,
                    iclip[0],
                    input_line_y,
                );
                input_glyphs.extend(inp);
                input_line_y += line_h;
            }

            let input_count = input_glyphs.len() as u32;
            self.ai_chat_glyph_instances.extend(input_glyphs);
            self.ai_chat_input_batch = Some(TextScissorBatch {
                scissor: self.ai_chat_input_scissor.unwrap_or([0, 0, 1, 1]),
                range: crate::render::text_pipeline::InstanceDrawRange {
                    start: input_start,
                    count: input_count,
                },
            });
            self.ai_chat_text_pipeline.upload_instances(
                &self.device,
                &self.queue,
                &self.ai_chat_glyph_instances,
            );
            if show_cursor {
                let last_line = visible_lines.last().unwrap_or(&input_text);
                // word_wrap strips trailing whitespace via split_whitespace; add
                // it back to the cursor X so pressing Space moves the cursor.
                let trailing_ws = input_buffer
                    .chars()
                    .rev()
                    .take_while(|c| c.is_whitespace())
                    .count();
                let cx = iclip[0]
                    + estimate_monospace_width(last_line, font_size)
                    + trailing_ws as f32 * font_size * 0.6;
                let cursor_y = iy + (visible_lines.len() as f32 - 1.0).max(0.0) * line_h;
                let mut cursor_color = accent;
                cursor_color[3] = 0.9;
                chrome.push(RegionDrawInstance::new(
                    [cx, cursor_y + 2.0, 8.0, (line_h - 4.0).max(1.0)],
                    cursor_color,
                ));
            }
            return (chrome, 0.0);
        }

        let mut all: Vec<GlyphInstance> = Vec::new();
        self.ai_chat_hero_image_pipeline.clear();
        all.extend(layout_panel_rich_text(
            &header_text,
            &header_spans,
            fg_dim,
            &mut self.ai_chat_text_system,
            &mut self.atlas,
            &self.queue,
            header_text_x,
            header_y,
        ));

        #[derive(Clone, Copy)]
        enum BubbleSide {
            Left,
            Right,
        }

        struct ChatBubble {
            label: &'static str,
            lines: Vec<String>,
            line_styles: Vec<Vec<StyledTextSpan>>,
            code_rows: Vec<bool>,
            side: BubbleSide,
            fill: [f32; 4],
            border: Option<[f32; 4]>,
            label_color: [f32; 4],
            body_color: [f32; 4],
            italic: bool,
        }

        let max_bubble_w = (hclip[2] * 0.85).max(1.0);
        let bubble_pad_x = 10.0;
        let bubble_content_w = (max_bubble_w - bubble_pad_x * 2.0).max(font_size * 8.0);
        let max_chars = ((bubble_content_w / (font_size * 0.6)) as usize).max(8);
        let user_fill = blend_rgb(editor_bg, accent, 0.24, 0.72);
        let netherize_fill = blend_rgb(panel_bg, accent, 0.06, 0.90);
        let system_fill = blend_rgb(panel_bg, warning, 0.04, 0.90);
        let assistant_border = with_alpha(accent, 0.18);
        let system_border = with_alpha(warning, 0.14);

        let fg_u8 = linear_rgba_to_srgb_u8(fg);
        let fg_dim_u8 = linear_rgba_to_srgb_u8(fg_dim);
        let accent_u8 = linear_rgba_to_srgb_u8(accent);
        let code_text_u8 = linear_rgba_to_srgb_u8(fg_dim);
        let inline_code_u8 = [0xDA, 0x70, 0xD6, 0xFF]; // orchid for inline code
        let code_bg = blend_rgb(panel_bg, self.theme.ui.selection_bg.as_f32(), 0.35, 0.92);
        let code_border = self.theme.ui.border_color.as_f32();

        let mut bubbles: Vec<ChatBubble> = Vec::new();
        for msg in messages {
            let clean = strip_ansi(&msg.text);
            let filtered: String = clean
                .split('\n')
                .filter(|line| !is_opencode_status_line(line))
                .collect::<Vec<_>>()
                .join("\n");
            if filtered.trim().is_empty() {
                continue;
            }

            let bubble = match msg.role {
                AiRole::User => {
                    let mut body_lines = Vec::new();
                    for para in filtered.split('\n') {
                        for wrapped in word_wrap(para, max_chars) {
                            if !wrapped.trim().is_empty() {
                                body_lines.push(wrapped);
                            }
                        }
                    }
                    if body_lines.is_empty() {
                        continue;
                    }
                    let code_rows = vec![false; body_lines.len()];
                    ChatBubble {
                        label: "you",
                        lines: body_lines,
                        line_styles: Vec::new(),
                        code_rows,
                        side: BubbleSide::Right,
                        fill: user_fill,
                        border: None,
                        label_color: accent,
                        body_color: fg,
                        italic: false,
                    }
                }
                AiRole::Assistant => {
                    let (body_lines, line_styles, code_rows) = build_styled_message_lines(
                        &filtered,
                        max_chars,
                        fg_u8,
                        code_text_u8,
                        inline_code_u8,
                        fg_dim_u8,
                        &self.theme,
                    );
                    if body_lines.is_empty() {
                        continue;
                    }
                    ChatBubble {
                        label: "netherize",
                        lines: body_lines,
                        line_styles,
                        code_rows,
                        side: BubbleSide::Left,
                        fill: netherize_fill,
                        border: Some(assistant_border),
                        label_color: accent,
                        body_color: fg,
                        italic: false,
                    }
                }
                AiRole::System => {
                    let is_err = clean.to_ascii_lowercase().contains("error");
                    let (body_lines, line_styles, code_rows) = build_styled_message_lines(
                        &filtered,
                        max_chars,
                        fg_u8,
                        code_text_u8,
                        inline_code_u8,
                        fg_dim_u8,
                        &self.theme,
                    );
                    if body_lines.is_empty() {
                        continue;
                    }
                    ChatBubble {
                        label: "netherize",
                        lines: body_lines,
                        line_styles,
                        code_rows,
                        side: BubbleSide::Left,
                        fill: system_fill,
                        border: Some(system_border),
                        label_color: if is_err { warning } else { fg_dim },
                        body_color: if is_err { warning } else { fg_dim },
                        italic: false,
                    }
                }
            };
            bubbles.push(bubble);
        }

        // ── Thinking indicator bubble ─────────────────────────────────────
        if is_generating {
            let phase = (std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis()
                / 400)
                % 4;
            let dots = match phase {
                0 => ".",
                1 => "..",
                2 => "...",
                _ => "",
            };
            bubbles.push(ChatBubble {
                label: "netherize",
                lines: vec![format!("thinking{dots}")],
                line_styles: Vec::new(),
                code_rows: vec![false],
                side: BubbleSide::Left,
                fill: netherize_fill,
                border: Some(assistant_border),
                label_color: accent,
                body_color: fg_dim,
                italic: true,
            });
        }

        let bubble_pad_y = 7.0;
        let bubble_gap = 12.0;
        let min_bubble_w = (font_size * 7.0).min(max_bubble_w);
        let total_h = bubbles.iter().fold(0.0, |acc, bubble| {
            acc + bubble_pad_y * 2.0
                + line_h
                + 2.0
                + bubble.lines.len() as f32 * line_h
                + bubble_gap
        });
        let vis_h = body_h;
        let max_scroll = (total_h - vis_h).max(0.0);
        let scroll = scroll_y.min(max_scroll);

        let mut cy = body_y + line_h * 0.5;
        for bubble in &bubbles {
            let content_w = bubble
                .lines
                .iter()
                .map(|line| estimate_monospace_width(line, font_size))
                .fold(estimate_monospace_width(bubble.label, font_size), f32::max);
            let bubble_w = (content_w + bubble_pad_x * 2.0)
                .min(max_bubble_w)
                .max(min_bubble_w);
            let bubble_h = bubble_pad_y * 2.0 + line_h + 2.0 + bubble.lines.len() as f32 * line_h;
            let bubble_x = match bubble.side {
                BubbleSide::Left => hclip[0] + 2.0,
                BubbleSide::Right => hclip[0] + hclip[2] - bubble_w - 2.0,
            };
            let y = cy - scroll;
            if y + bubble_h > body_y && y < body_y + vis_h {
                let clipped_y = y.max(body_y);
                let clipped_h = (y + bubble_h).min(body_y + vis_h) - clipped_y;
                if let Some(border) = bubble.border {
                    chrome.push(
                        RegionDrawInstance::new([bubble_x, clipped_y, bubble_w, clipped_h], border)
                            .with_radius(7.0),
                    );
                    chrome.push(
                        RegionDrawInstance::new(
                            [
                                bubble_x + 2.0,
                                clipped_y + 2.0,
                                (bubble_w - 4.0).max(1.0),
                                (clipped_h - 4.0).max(1.0),
                            ],
                            bubble.fill,
                        )
                        .with_radius(5.0),
                    );
                } else {
                    chrome.push(
                        RegionDrawInstance::new(
                            [bubble_x, clipped_y, bubble_w, clipped_h],
                            bubble.fill,
                        )
                        .with_radius(7.0),
                    );
                }
                let text_x = bubble_x + bubble_pad_x;
                let label_y = y + bubble_pad_y;
                // Only emit glyphs for lines within the visible body area so
                // scrolled-off bubbles don't bleed into the header region.
                if label_y >= body_y && label_y < body_y + vis_h {
                    all.extend(layout_panel_text(
                        bubble.label,
                        &mut self.ai_chat_text_system,
                        &mut self.atlas,
                        &self.queue,
                        text_x,
                        label_y,
                        bubble.label_color,
                    ));
                }
                let mut text_y = label_y + line_h + 2.0;
                for (line_idx, line) in bubble.lines.iter().enumerate() {
                    if text_y >= body_y && text_y < body_y + vis_h + line_h {
                        let is_code_row = bubble.code_rows.get(line_idx).copied().unwrap_or(false);
                        let line_text_x = if is_code_row { text_x + 8.0 } else { text_x };
                        if is_code_row {
                            let row_rect = [
                                bubble_x + bubble_pad_x * 0.5,
                                text_y + 1.0,
                                (bubble_w - bubble_pad_x).max(1.0),
                                (line_h + 2.0).max(1.0),
                            ];
                            chrome.push(
                                RegionDrawInstance::new(row_rect, code_border).with_radius(5.0),
                            );
                            chrome.push(
                                RegionDrawInstance::new(
                                    [
                                        row_rect[0] + 1.0,
                                        row_rect[1] + 1.0,
                                        (row_rect[2] - 2.0).max(1.0),
                                        (row_rect[3] - 2.0).max(1.0),
                                    ],
                                    code_bg,
                                )
                                .with_radius(4.0),
                            );
                        }
                        let has_styles = bubble.line_styles.len() > line_idx
                            && !bubble.line_styles[line_idx].is_empty();
                        let gs = if has_styles {
                            layout_panel_rich_text(
                                line,
                                &bubble.line_styles[line_idx],
                                bubble.body_color,
                                &mut self.ai_chat_text_system,
                                &mut self.atlas,
                                &self.queue,
                                line_text_x,
                                text_y,
                            )
                        } else if bubble.italic {
                            layout_panel_text_italic(
                                line,
                                &mut self.ai_chat_text_system,
                                &mut self.atlas,
                                &self.queue,
                                line_text_x,
                                text_y,
                                bubble.body_color,
                            )
                        } else {
                            layout_panel_text(
                                line,
                                &mut self.ai_chat_text_system,
                                &mut self.atlas,
                                &self.queue,
                                line_text_x,
                                text_y,
                                bubble.body_color,
                            )
                        };
                        all.extend(gs);
                    }
                    text_y += line_h;
                }
            }
            cy += bubble_h + bubble_gap;
        }

        // ── Input box glyphs ──────────────────────────────────────────────
        self.ai_chat_suggestion_chrome_instances.clear();
        self.ai_chat_suggestion_glyph_start = None;
        let (suggestions, sel) =
            ai_chat_input_suggestions(input_buffer, file_suggestions, selected_suggestion_index);
        if !suggestions.is_empty() {
            let suggestion_rect =
                slash_suggestion_rect(input_bounds, hclip, line_h, suggestions.len());
            // Route popup chrome into its own vec so it is drawn *after* message
            // bubble text, preventing bubble glyphs from covering the background.
            self.ai_chat_suggestion_chrome_instances.push(
                RegionDrawInstance::new(suggestion_rect, self.theme.ui.border_color.as_f32())
                    .with_radius(9.0),
            );
            self.ai_chat_suggestion_chrome_instances.push(
                RegionDrawInstance::new(
                    [
                        suggestion_rect[0] + 1.0,
                        suggestion_rect[1] + 1.0,
                        (suggestion_rect[2] - 2.0).max(1.0),
                        (suggestion_rect[3] - 2.0).max(1.0),
                    ],
                    blend_rgb(editor_bg, panel_bg, 0.38, 0.98),
                )
                .with_radius(8.0),
            );
            self.ai_chat_suggestion_glyph_start = Some(all.len() as u32);
            let mut suggestion_y = suggestion_rect[1] + 6.0;
            for (i, (label, detail)) in suggestions.iter().enumerate() {
                // Highlight the selected suggestion row.
                if i == sel {
                    self.ai_chat_suggestion_chrome_instances.push(
                        RegionDrawInstance::new(
                            [
                                suggestion_rect[0] + 2.0,
                                suggestion_y,
                                suggestion_rect[2] - 4.0,
                                line_h,
                            ],
                            blend_rgb(accent, editor_bg, 0.22, 0.55),
                        )
                        .with_radius(4.0),
                    );
                }
                let row_text = format!("{label:<24} {detail}");
                let spans = [StyledTextSpan {
                    start: 0,
                    end: label.len(),
                    color_rgba: accent_u8,
                    bold: false,
                    italic: false,
                }];
                all.extend(layout_panel_rich_text(
                    &row_text,
                    &spans,
                    fg_dim,
                    &mut self.ai_chat_text_system,
                    &mut self.atlas,
                    &self.queue,
                    suggestion_rect[0] + 10.0,
                    suggestion_y,
                ));
                suggestion_y += line_h;
            }
        }
        let hist_count = all.len() as u32;
        let input_text = if input_buffer.is_empty() {
            "> ask netherize... (/help, @file)".to_string()
        } else {
            format!("> {input_buffer}")
        };
        let input_default = if input_buffer.is_empty() { fg_dim } else { fg };
        let input_spans = [StyledTextSpan {
            start: 0,
            end: 2,
            color_rgba: accent_u8,
            bold: false,
            italic: false,
        }];

        let input_content_w = iclip[2];
        let input_max_chars = ((input_content_w / (font_size * 0.6)) as usize).max(12);
        let wrapped_lines = if input_buffer.is_empty() {
            vec![input_text.clone()]
        } else {
            word_wrap(&input_text, input_max_chars)
        };

        // Show wrapped lines that fit in the input box height.
        // Trim older lines so newly typed content stays visible.
        let max_input_lines = ((iclip[3] - 4.0) / line_h).floor().max(1.0).min(5.0) as usize;
        let line_trim = wrapped_lines.len().saturating_sub(max_input_lines);
        let visible_lines = &wrapped_lines[line_trim..];
        let is_first_visible = line_trim == 0;

        let total_visible_h = visible_lines.len() as f32 * line_h;
        let iy = iclip[1] + ((iclip[3] - total_visible_h) * 0.5).clamp(4.0, inner_padding);

        let mut input_line_y = iy;
        for (idx, line) in visible_lines.iter().enumerate() {
            let spans: &[StyledTextSpan] = if idx == 0 && is_first_visible {
                &input_spans
            } else {
                &[]
            };
            all.extend(layout_panel_rich_text(
                line,
                spans,
                input_default,
                &mut self.ai_chat_text_system,
                &mut self.atlas,
                &self.queue,
                iclip[0],
                input_line_y,
            ));
            input_line_y += line_h;
        }

        let in_count = all.len() as u32 - hist_count;
        self.ai_chat_input_batch = Some(TextScissorBatch {
            scissor: self.ai_chat_input_scissor.unwrap_or([0, 0, 1, 1]),
            range: crate::render::text_pipeline::InstanceDrawRange {
                start: hist_count,
                count: in_count,
            },
        });

        // ── Upload ────────────────────────────────────────────────────────
        self.ai_chat_glyph_instances = all;
        self.ai_chat_text_pipeline.upload_instances(
            &self.device,
            &self.queue,
            &self.ai_chat_glyph_instances,
        );

        // ── Cursor quad (drawn via region pipeline) ───────────────────────
        if show_cursor {
            let last_line = visible_lines.last().unwrap_or(&input_text);
            let trailing_ws = input_buffer
                .chars()
                .rev()
                .take_while(|c| c.is_whitespace())
                .count();
            let cx = iclip[0]
                + estimate_monospace_width(last_line, font_size)
                + trailing_ws as f32 * font_size * 0.6;
            let cursor_y = iy + (visible_lines.len() as f32 - 1.0).max(0.0) * line_h;
            let mut cursor_color = accent;
            cursor_color[3] = 0.9;
            chrome.push(RegionDrawInstance::new(
                [cx, cursor_y + 2.0, 8.0, (line_h - 4.0).max(1.0)],
                cursor_color,
            ));
        }
        (chrome, max_scroll)
    }

    /// Clear AI chat text — called when right sidebar is hidden.
    pub fn clear_ai_chat(&mut self) {
        self.ai_chat_history_scissor = None;
        self.ai_chat_image_scissor = None;
        self.ai_chat_input_scissor = None;
        self.ai_chat_input_batch = None;
        self.ai_chat_history_chrome_instances.clear();
        self.ai_chat_suggestion_chrome_instances.clear();
        self.ai_chat_suggestion_glyph_start = None;
        self.ai_chat_glyph_instances.clear();
        self.ai_chat_header_image_pipeline.clear();
        self.ai_chat_hero_image_pipeline.clear();
        self.ai_chat_text_pipeline
            .upload_instances(&self.device, &self.queue, &[]);
    }

    /// Render markdown preview content into the right sidebar area.
    /// Reuses the AI chat text pipeline since they're mutually exclusive tabs.
    /// Render markdown preview content into the right sidebar area.
    /// Reuses the AI chat text pipeline since they're mutually exclusive tabs.
    pub fn update_markdown_preview_content(
        &mut self,
        bounds: [f32; 4],
        lines: &[crate::app::app_state::MarkdownPreviewLine],
        scroll_y: f32,
        inner_padding: f32,
    ) -> f32 {
        use crate::app::app_state::MarkdownBlockType;

        if bounds[2] < 1.0 || bounds[3] < 1.0 {
            self.clear_ai_chat();
            return 0.0;
        }

        self.ai_chat_header_image_pipeline.clear();
        self.ai_chat_hero_image_pipeline.clear();
        self.ai_chat_image_scissor = None;
        self.ai_chat_input_scissor = None;
        self.ai_chat_input_batch = None;
        self.ai_chat_history_chrome_instances.clear();

        let clip = [
            bounds[0] + inner_padding,
            bounds[1] + inner_padding,
            (bounds[2] - inner_padding * 2.0).max(1.0),
            (bounds[3] - inner_padding * 2.0).max(1.0),
        ];
        self.ai_chat_history_scissor = rect_to_scissor(clip);

        let font_size = self.theme.ui.sidebar_font_size;
        let line_h = self.theme.ui.sidebar_line_height.max(font_size + 4.0);
        let fg = self.theme.ui.fg.as_f32();
        let fg_dim = self.theme.ui.fg_dim.as_f32();

        self.ai_chat_text_system.set_size(None, Some(line_h));

        let mut all_glyphs: Vec<GlyphInstance> = Vec::new();
        let mut chrome_instances: Vec<RegionDrawInstance> = Vec::new();

        let code_bg = blend_rgb(
            self.theme.ui.panel_bg.as_f32(),
            self.theme.ui.selection_bg.as_f32(),
            0.35,
            0.92,
        );
        let table_header_bg = blend_rgb(
            self.theme.ui.panel_bg.as_f32(),
            self.theme.ui.selection_bg.as_f32(),
            0.20,
            0.95,
        );
        let code_border = self.theme.ui.border_color.as_f32();
        let code_inset_x = 6.0;
        let code_pad_x = 8.0;
        let estimated_char_w = (font_size * 0.58).max(1.0);
        let prose_max_chars = (clip[2] / estimated_char_w).floor().max(8.0) as usize;
        let code_text_x = clip[0] + code_inset_x + code_pad_x;
        let code_max_chars = ((clip[2] - code_inset_x - code_pad_x * 2.0).max(1.0)
            / estimated_char_w)
            .floor()
            .max(8.0) as usize;

        // Step 1: Lay out all wrapped lines and flag code block boundaries
        struct LayoutedLine {
            block_type: MarkdownBlockType,
            text: String,
            spans: Vec<StyledTextSpan>,
            y_offset: f32,
            is_code_block: bool,
            is_code_block_start: bool,
            is_code_block_end: bool,
        }

        let mut layouted_lines: Vec<LayoutedLine> = Vec::new();

        for (line_idx, preview_line) in lines.iter().enumerate() {
            let is_code = matches!(preview_line.block_type, MarkdownBlockType::CodeBlock);
            let is_table = matches!(
                preview_line.block_type,
                MarkdownBlockType::TableHeader | MarkdownBlockType::TableRow
            );
            let wrapped_lines = if is_code {
                word_wrap_with_ranges(&preview_line.text, code_max_chars)
            } else if is_table {
                vec![(preview_line.text.clone(), 0..preview_line.text.len())]
            } else {
                word_wrap_with_ranges(&preview_line.text, prose_max_chars)
            };

            let num_wrapped = wrapped_lines.len();
            for (w_idx, (wrapped_text, byte_range)) in wrapped_lines.into_iter().enumerate() {
                let wrapped_spans: Vec<StyledTextSpan> = preview_line
                    .spans
                    .iter()
                    .filter_map(|span| clip_styled_span_to_range(*span, &byte_range))
                    .collect();

                let is_code_block_start = is_code
                    && (line_idx == 0
                        || !matches!(lines[line_idx - 1].block_type, MarkdownBlockType::CodeBlock))
                    && w_idx == 0;
                let is_code_block_end = is_code
                    && (line_idx + 1 == lines.len()
                        || !matches!(lines[line_idx + 1].block_type, MarkdownBlockType::CodeBlock))
                    && w_idx + 1 == num_wrapped;

                layouted_lines.push(LayoutedLine {
                    block_type: preview_line.block_type,
                    text: wrapped_text,
                    spans: wrapped_spans,
                    y_offset: 0.0,
                    is_code_block: is_code,
                    is_code_block_start,
                    is_code_block_end,
                });
            }
        }

        // Step 2: Compute running vertical positions with margin/padding rules
        let spacing_heading_large = 14.0;
        let spacing_heading_small = 8.0;
        let spacing_heading_bottom = 4.0;
        let spacing_paragraph = 6.0;
        let spacing_list_item = 3.0;
        let spacing_code_block_margin = 8.0;
        let spacing_code_block_padding = 6.0;

        let mut current_y = 0.0;

        for i in 0..layouted_lines.len() {
            let is_code_block_start = layouted_lines[i].is_code_block_start;
            let is_code_block_end = layouted_lines[i].is_code_block_end;
            let is_code_block = layouted_lines[i].is_code_block;
            let block_type = layouted_lines[i].block_type;

            if i > 0 {
                let prev_block_type = layouted_lines[i - 1].block_type;
                let prev_is_code = layouted_lines[i - 1].is_code_block;

                if is_code_block_start {
                    current_y += spacing_code_block_margin;
                } else if matches!(block_type, MarkdownBlockType::Heading(_)) {
                    if let MarkdownBlockType::Heading(level) = block_type {
                        if level <= 2 {
                            current_y += spacing_heading_large;
                        } else {
                            current_y += spacing_heading_small;
                        }
                    }
                } else if !is_code_block && prev_is_code {
                    current_y += spacing_code_block_margin;
                } else if !is_code_block {
                    match prev_block_type {
                        MarkdownBlockType::Heading(_) => current_y += spacing_heading_bottom,
                        MarkdownBlockType::Paragraph => current_y += spacing_paragraph,
                        MarkdownBlockType::ListItem => current_y += spacing_list_item,
                        MarkdownBlockType::BlockQuote => current_y += spacing_paragraph,
                        _ => {}
                    }
                }
            }

            if is_code_block_start {
                current_y += spacing_code_block_padding;
            }

            layouted_lines[i].y_offset = current_y;
            current_y += line_h;

            if is_code_block_end {
                current_y += spacing_code_block_padding;
            }
        }
        let total_doc_height = current_y;

        let max_scroll = (total_doc_height - clip[3]).max(0.0) / line_h;
        let clamped_scroll_y = scroll_y.min(max_scroll);
        let scroll_offset_y = clamped_scroll_y * line_h;

        // Step 3: Draw code block backgrounds (unified per block) and table header highlights
        let mut current_code_block_start: Option<usize> = None;
        let start_y = clip[1];

        for i in 0..layouted_lines.len() {
            let line = &layouted_lines[i];
            if line.is_code_block_start {
                current_code_block_start = Some(i);
            }
            if line.is_code_block_end {
                if let Some(start_idx) = current_code_block_start.take() {
                    let y_start = layouted_lines[start_idx].y_offset - spacing_code_block_padding;
                    let y_end = line.y_offset + line_h + spacing_code_block_padding;

                    let screen_y_start = start_y + y_start - scroll_offset_y;
                    let screen_y_end = start_y + y_end - scroll_offset_y;
                    let height = screen_y_end - screen_y_start;

                    let box_x = clip[0] + code_inset_x;
                    let box_w = (clip[2] - code_inset_x * 2.0).max(1.0);
                    let rect = [box_x, screen_y_start, box_w, height.max(1.0)];

                    chrome_instances.push(
                        RegionDrawInstance::new(rect, code_border)
                            .with_radius(self.panel_corner_radius.min(6.0)),
                    );
                    chrome_instances.push(
                        RegionDrawInstance::new(
                            [
                                rect[0] + 1.0,
                                rect[1] + 1.0,
                                (rect[2] - 2.0).max(1.0),
                                (rect[3] - 2.0).max(1.0),
                            ],
                            code_bg,
                        )
                        .with_radius((self.panel_corner_radius.min(6.0) - 1.0).max(0.0)),
                    );
                }
            }

            if matches!(line.block_type, MarkdownBlockType::TableHeader) {
                let y = start_y + line.y_offset - scroll_offset_y;
                let rect = [
                    clip[0] + code_inset_x,
                    y,
                    (clip[2] - code_inset_x * 2.0).max(1.0),
                    line_h,
                ];
                chrome_instances.push(
                    RegionDrawInstance::new(rect, table_header_bg)
                        .with_radius(self.panel_corner_radius.min(4.0)),
                );
            }
        }

        // Step 4: Draw all text lines
        let visible_bottom = clip[1] + clip[3];

        for line in &layouted_lines {
            let y = start_y + line.y_offset - scroll_offset_y;

            if y + line_h < clip[1] {
                continue;
            }
            if y > visible_bottom {
                break;
            }

            if line.text.is_empty() {
                continue;
            }

            let text_x = if line.is_code_block {
                code_text_x
            } else {
                clip[0]
            };

            let default_color = match line.block_type {
                MarkdownBlockType::Heading(_) => fg,
                MarkdownBlockType::CodeBlock => fg_dim,
                MarkdownBlockType::BlockQuote => fg_dim,
                _ => fg,
            };

            if line.spans.is_empty() {
                all_glyphs.extend(layout_panel_text(
                    &line.text,
                    &mut self.ai_chat_text_system,
                    &mut self.atlas,
                    &self.queue,
                    text_x,
                    y,
                    default_color,
                ));
            } else {
                all_glyphs.extend(layout_panel_rich_text(
                    &line.text,
                    &line.spans,
                    default_color,
                    &mut self.ai_chat_text_system,
                    &mut self.atlas,
                    &self.queue,
                    text_x,
                    y,
                ));
            }
        }

        self.ai_chat_history_chrome_instances = chrome_instances;
        self.ai_chat_glyph_instances = all_glyphs;
        self.ai_chat_text_pipeline.upload_instances(
            &self.device,
            &self.queue,
            &self.ai_chat_glyph_instances,
        );

        max_scroll
    }
}
