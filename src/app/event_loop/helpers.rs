use std::fs;
use std::path::Path;

use super::*;

use crate::{
    app::app_state::{FloatingBoxBlock, MarkdownBlockType, MarkdownPreviewLine},
    async_runtime::message::FilePreviewLine,
    syntax::highlight::highlight_snippet,
};

const BRACKET_PAIRS: &[(char, char)] = &[('(', ')'), ('[', ']'), ('{', '}')];

fn byte_inside_any_span(
    byte_idx: usize,
    spans: &[HighlightSpan],
    categories: &[crate::syntax::highlight::HighlightCategory],
) -> bool {
    spans.iter().any(|span| {
        span.range.start <= byte_idx
            && byte_idx < span.range.end
            && categories.contains(&span.category)
    })
}

fn rainbow_bracket_spans(
    text: &str,
    syntax_spans: &[HighlightSpan],
    theme: &ThemeConfig,
) -> Vec<StyledTextSpan> {
    let palette: Vec<[u8; 4]> = theme
        .editor
        .rainbow_brackets
        .iter()
        .map(|color| color.as_u8())
        .collect();
    if palette.is_empty() || text.is_empty() {
        return Vec::new();
    }

    let ignored = [
        crate::syntax::highlight::HighlightCategory::String,
        crate::syntax::highlight::HighlightCategory::Comment,
    ];
    let mut out = Vec::new();
    let mut stack: Vec<(char, usize)> = Vec::new();

    for (byte_idx, ch) in text.char_indices() {
        if !matches!(ch, '(' | ')' | '[' | ']' | '{' | '}') {
            continue;
        }
        if byte_inside_any_span(byte_idx, syntax_spans, &ignored) {
            continue;
        }

        if let Some((_, close)) = BRACKET_PAIRS.iter().find(|(open, _)| *open == ch) {
            let depth = stack.len();
            let color = palette[depth % palette.len()];
            out.push(StyledTextSpan::new(
                byte_idx,
                byte_idx + ch.len_utf8(),
                color,
            ));
            stack.push((ch, *close as usize));
            continue;
        }

        if let Some(pair_idx) = BRACKET_PAIRS.iter().position(|(_, close)| *close == ch) {
            if let Some(open_pos) = stack
                .iter()
                .rposition(|(open, _)| *open == BRACKET_PAIRS[pair_idx].0)
            {
                let depth = open_pos;
                stack.truncate(open_pos);
                let color = palette[depth % palette.len()];
                out.push(StyledTextSpan::new(
                    byte_idx,
                    byte_idx + ch.len_utf8(),
                    color,
                ));
            }
        }
    }

    out
}

pub(super) fn syntax_spans_to_styled(
    spans: &[HighlightSpan],
    text: &str,
    theme: &ThemeConfig,
) -> Vec<StyledTextSpan> {
    let mut styled: Vec<StyledTextSpan> = spans
        .iter()
        .map(|span| {
            let color = match span.category {
                crate::syntax::highlight::HighlightCategory::Keyword => {
                    theme.syntax.keyword.as_u8()
                }
                crate::syntax::highlight::HighlightCategory::KeywordControl => {
                                    theme.syntax.keyword_control.as_u8()
                                }
                                crate::syntax::highlight::HighlightCategory::KeywordStorage => {
                                    theme.syntax.keyword_storage.as_u8()
                                }
                crate::syntax::highlight::HighlightCategory::String => theme.syntax.string.as_u8(),
                crate::syntax::highlight::HighlightCategory::StringEscape => {
                                    theme.syntax.string_escape.as_u8()
                                }
                crate::syntax::highlight::HighlightCategory::Comment => {
                    theme.syntax.comment.as_u8()
                }
                crate::syntax::highlight::HighlightCategory::CommentDoc => {
                                    theme.syntax.comment_doc.as_u8()
                                }
                crate::syntax::highlight::HighlightCategory::Type => theme.syntax.r#type.as_u8(),
                crate::syntax::highlight::HighlightCategory::TypeBuiltin => {
                                    theme.syntax.type_builtin.as_u8()
                                }
                crate::syntax::highlight::HighlightCategory::Function => {
                    theme.syntax.function.as_u8()
                }
                crate::syntax::highlight::HighlightCategory::FunctionBuiltin => {
                                    theme.syntax.function_builtin.as_u8()
                                }
                crate::syntax::highlight::HighlightCategory::Number => theme.syntax.number.as_u8(),
                crate::syntax::highlight::HighlightCategory::Boolean => {
                    theme.syntax.boolean.as_u8()
                }
                crate::syntax::highlight::HighlightCategory::Identifier => {
                    theme.syntax.identifier.as_u8()
                }
                crate::syntax::highlight::HighlightCategory::Variable => {
                    theme.syntax.variable.as_u8()
                }
                crate::syntax::highlight::HighlightCategory::VariableBuiltin => {
                                    theme.syntax.variable_builtin.as_u8()
                                }
                crate::syntax::highlight::HighlightCategory::Parameter => {
                    theme.syntax.parameter.as_u8()
                }
                crate::syntax::highlight::HighlightCategory::Field => theme.syntax.field.as_u8(),
                crate::syntax::highlight::HighlightCategory::Property => {
                    theme.syntax.property.as_u8()
                }
                crate::syntax::highlight::HighlightCategory::Constant => {
                    theme.syntax.constant.as_u8()
                }
                crate::syntax::highlight::HighlightCategory::Operator => {
                    theme.syntax.operator.as_u8()
                }
                crate::syntax::highlight::HighlightCategory::Punctuation => {
                    theme.syntax.punctuation.as_u8()
                }
                crate::syntax::highlight::HighlightCategory::Escape => theme.syntax.escape.as_u8(),
                crate::syntax::highlight::HighlightCategory::Macro => theme.syntax.r#macro.as_u8(),
                crate::syntax::highlight::HighlightCategory::Lifetime => {
                    theme.syntax.lifetime.as_u8()
                }
                crate::syntax::highlight::HighlightCategory::Constructor => {
                    theme.syntax.constructor.as_u8()
                }
                crate::syntax::highlight::HighlightCategory::Attribute => {
                    theme.syntax.attribute.as_u8()
                }
                crate::syntax::highlight::HighlightCategory::Namespace => {
                    theme.syntax.namespace.as_u8()
                }
                crate::syntax::highlight::HighlightCategory::Tag => theme.syntax.tag.as_u8(),
                crate::syntax::highlight::HighlightCategory::MarkupStrong => {
                                    theme.syntax.markup_strong.as_u8()
                                }
                                crate::syntax::highlight::HighlightCategory::MarkupItalic => {
                                    theme.syntax.markup_italic.as_u8()
                                }
                                crate::syntax::highlight::HighlightCategory::MarkupInlineCode => {
                                    theme.syntax.markup_inline_code.as_u8()
                                }
                                crate::syntax::highlight::HighlightCategory::MarkupLink => {
                                    theme.syntax.markup_link.as_u8()
                                }
            };
            StyledTextSpan::with_style(
                span.range.start,
                span.range.end,
                color,
                span.category.is_bold(),
                span.category.is_italic(),
            )
        })
        .collect();
    styled.extend(rainbow_bracket_spans(text, spans, theme));
    styled
}

pub(super) fn diagnostic_spans_to_styled(
    app_state: &AppState,
    theme: &ThemeConfig,
) -> Vec<StyledTextSpan> {
    let Some(path) = app_state.active_file() else {
        return Vec::new();
    };
    let Some(diagnostics) = app_state.diagnostics_for_path(path) else {
        return Vec::new();
    };

    let mut ordered = diagnostics.iter().collect::<Vec<_>>();
    ordered.sort_by_key(|diagnostic| diagnostic.severity.unwrap_or(u32::MAX));

    ordered
        .into_iter()
        .filter_map(|diagnostic| {
            const DIAGNOSTIC_TAG_UNNECESSARY: u32 = 1;

            let severity = diagnostic.severity.unwrap_or(2);
            let is_unnecessary = diagnostic.tags.contains(&DIAGNOSTIC_TAG_UNNECESSARY);
            let color = if is_unnecessary {
                theme.ui.fg_ghost.as_u8()
            } else {
                match severity {
                    1 => theme.ui.error.as_u8(),
                    2 => theme.ui.warning.as_u8(),
                    _ => return None,
                }
            };

            let start_line = diagnostic.range.start.line as usize;
            let end_line = diagnostic.range.end.line as usize;
            let start_byte = app_state
                .line_char_to_byte_idx(start_line, diagnostic.range.start.character as usize);
            let mut end_byte =
                app_state.line_char_to_byte_idx(end_line, diagnostic.range.end.character as usize);
            if end_byte <= start_byte {
                end_byte = start_byte
                    .saturating_add(1)
                    .min(app_state.line_end_byte_idx(start_line))
                    .min(app_state.text_len_bytes());
            }
            if end_byte <= start_byte {
                return None;
            }

            Some(StyledTextSpan::with_style(
                start_byte,
                end_byte,
                color,
                severity == 1,
                false,
            ))
        })
        .collect()
}

pub(super) fn build_preview_render_data(
    lines: &[FilePreviewLine],
    path: &Path,
    theme: &ThemeConfig,
) -> (String, Vec<StyledTextSpan>) {
    if lines.is_empty() {
        return (String::new(), Vec::new());
    }
    let text = lines
        .iter()
        .map(|line| line.text.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    let extension = path
        .extension()
        .and_then(|ext| ext.to_str())
        .unwrap_or_default();
    let raw_spans = highlight_snippet(&text, extension, theme);
    let spans = syntax_spans_to_styled(&raw_spans, &text, theme);
    (text, spans)
}

/// Convert worker-pre-parsed hover blocks into the renderer's `FloatingBoxBlock`
/// shape. The worker has already done the expensive Tree-sitter parsing; here
/// we only resolve theme colours (cheap hash lookups in `syntax_spans_to_styled`).
pub(super) fn convert_worker_hover_blocks(
    raw: Vec<crate::async_runtime::message::HoverDocBlock>,
    theme: &ThemeConfig,
) -> Vec<FloatingBoxBlock> {
    use crate::async_runtime::message::HoverDocBlock;
    raw.into_iter()
        .map(|block| match block {
            HoverDocBlock::Prose(text) => FloatingBoxBlock::Prose(text),
            HoverDocBlock::Code { text, spans } => {
                let styled = syntax_spans_to_styled(&spans, &text, theme);
                FloatingBoxBlock::Code {
                    text,
                    spans: styled,
                }
            }
        })
        .collect()
}

pub(super) fn parse_hover_markdown_blocks(
    content: &str,
    theme: &ThemeConfig,
) -> Vec<FloatingBoxBlock> {
    let mut blocks = Vec::new();
    let mut prose_lines: Vec<String> = Vec::new();
    let mut code_lines: Vec<String> = Vec::new();
    let mut code_language = String::new();
    let mut in_code_block = false;

    let flush_prose = |blocks: &mut Vec<FloatingBoxBlock>, prose_lines: &mut Vec<String>| {
        let text = prose_lines.join("\n").trim().to_string();
        prose_lines.clear();
        if !text.is_empty() {
            blocks.push(FloatingBoxBlock::Prose(text));
        }
    };

    let flush_code =
        |blocks: &mut Vec<FloatingBoxBlock>, code_lines: &mut Vec<String>, code_language: &str| {
            let text = code_lines.join("\n");
            code_lines.clear();
            if text.trim().is_empty() {
                return;
            }
            let raw_spans = highlight_snippet(&text, code_language, theme);
            let spans = syntax_spans_to_styled(&raw_spans, &text, theme);
            blocks.push(FloatingBoxBlock::Code { text, spans });
        };

    for line in content.lines() {
        let trimmed = line.trim_start();
        if let Some(fence) = trimmed.strip_prefix("```") {
            if in_code_block {
                flush_code(&mut blocks, &mut code_lines, &code_language);
                code_language.clear();
                in_code_block = false;
            } else {
                flush_prose(&mut blocks, &mut prose_lines);
                code_language = fence.trim().to_string();
                in_code_block = true;
            }
            continue;
        }

        if in_code_block {
            code_lines.push(line.to_string());
        } else {
            prose_lines.push(line.to_string());
        }
    }

    if in_code_block {
        flush_code(&mut blocks, &mut code_lines, &code_language);
    } else {
        flush_prose(&mut blocks, &mut prose_lines);
    }

    blocks
}

pub(super) fn parse_markdown_preview_blocks(
    source: &str,
    theme: &ThemeConfig,
) -> Vec<MarkdownPreviewLine> {
    use crate::syntax::syntax_engine::{LanguageId, SyntaxEngine};

    if source.is_empty() {
        return Vec::new();
    }

    let mut engine = match SyntaxEngine::new(LanguageId::Markdown) {
        Ok(engine) => engine,
        Err(_) => return fallback_markdown_preview(source, theme),
    };

    let tree_state = match engine.parse_source(source, 0) {
        Ok(state) => state,
        Err(_) => return fallback_markdown_preview(source, theme),
    };

    let mut lines = Vec::new();
    let root = tree_state.root_node();
    render_markdown_node(root, source, theme, &mut lines);
    preserve_markdown_blank_lines(source, &mut lines);
    lines
}

fn preserve_markdown_blank_lines(source: &str, lines: &mut Vec<MarkdownPreviewLine>) {
    let source_lines = source.lines().collect::<Vec<_>>();
    let blank_count = source_lines
        .iter()
        .filter(|line| line.trim().is_empty())
        .count();
    if blank_count == 0 {
        return;
    }

    let mut expanded = Vec::with_capacity(lines.len().saturating_add(blank_count));
    let mut rendered_iter = lines.drain(..);
    for idx in 0..source_lines.len() {
        let source_line = source_lines[idx];
        if source_line.trim().is_empty() {
            if should_preserve_markdown_blank_line(&source_lines, idx) {
                expanded.push(MarkdownPreviewLine {
                    text: String::new(),
                    spans: Vec::new(),
                    block_type: MarkdownBlockType::Empty,
                    code_language: None,
                });
            }
        } else if let Some(line) = rendered_iter.next() {
            expanded.push(line);
        }
    }
    expanded.extend(rendered_iter);
    *lines = expanded;
}

fn should_preserve_markdown_blank_line(lines: &[&str], blank_idx: usize) -> bool {
    let prev = lines[..blank_idx]
        .iter()
        .rev()
        .find(|line| !line.trim().is_empty())
        .map(|line| line.trim_start());
    let next = lines[blank_idx.saturating_add(1)..]
        .iter()
        .find(|line| !line.trim().is_empty())
        .map(|line| line.trim_start());

    match (prev, next) {
        (Some(prev), Some(next)) => {
            markdown_line_indent_level(prev) != markdown_line_indent_level(next)
        }
        _ => false,
    }
}

fn markdown_line_indent_level(line: &str) -> usize {
    line.chars().take_while(|ch| ch.is_whitespace()).count() / 2
}

fn fallback_markdown_preview(source: &str, _theme: &ThemeConfig) -> Vec<MarkdownPreviewLine> {
    source
        .lines()
        .map(|line| MarkdownPreviewLine {
            text: line.to_string(),
            spans: Vec::new(),
            block_type: MarkdownBlockType::Paragraph,
            code_language: None,
        })
        .collect()
}

/// Calculate display width for text in monospace font.
/// Uses a simple heuristic: ASCII = 1, CJK/fullwidth = 2, others = 1.
/// This is more accurate than `.chars().count()` for Unicode text.
fn display_width(text: &str) -> usize {
    text.chars()
        .map(|ch| {
            let code = ch as u32;
            // CJK Unified Ideographs, Hangul, Katakana, Hiragana, fullwidth forms
            if (0x4E00..=0x9FFF).contains(&code)  // CJK
                || (0x3040..=0x30FF).contains(&code)  // Hiragana, Katakana
                || (0xAC00..=0xD7AF).contains(&code)  // Hangul
                || (0xFF00..=0xFFEF).contains(&code)  // Fullwidth
                || (0x1F300..=0x1F9FF).contains(&code) // Emoji
            {
                2
            } else {
                1
            }
        })
        .sum()
}

fn render_markdown_node(
    node: tree_sitter::Node<'_>,
    source: &str,
    theme: &ThemeConfig,
    out: &mut Vec<MarkdownPreviewLine>,
) {
    let kind = node.kind();

    match kind {
        "document" => {
            render_children(node, source, theme, out);
        }
        "atx_heading" => {
            let level = heading_level_from_atx(node, source);
            let raw_content = heading_content_text(node, source);
            let color = heading_color(level, theme);
            let (content_text, inline_spans) = render_markdown_inline_text(&raw_content, theme);
            let spans = heading_spans(&content_text, inline_spans, color, level);
            out.push(MarkdownPreviewLine {
                text: content_text,
                spans,
                block_type: MarkdownBlockType::Heading(level),
                code_language: None,
            });
        }
        "setext_heading" => {
            let level = setext_heading_level(node, source);
            let raw_content = setext_heading_content(node, source);
            let color = heading_color(level, theme);
            let (content_text, inline_spans) = render_markdown_inline_text(&raw_content, theme);
            let spans = heading_spans(&content_text, inline_spans, color, level);
            out.push(MarkdownPreviewLine {
                text: content_text,
                spans,
                block_type: MarkdownBlockType::Heading(level),
                code_language: None,
            });
        }
        "paragraph" => {
            let raw_text = node_text(node, source);
            let (text, spans) = render_markdown_inline_text(&raw_text, theme);
            out.push(MarkdownPreviewLine {
                text,
                spans,
                block_type: MarkdownBlockType::Paragraph,
                code_language: None,
            });
        }
        "link_reference_definition" | "link_reference_definition_block" => {
            let raw_text = node_text(node, source);
            let raw_text = raw_text.trim();
            if let Some((label, url)) = parse_link_reference_definition(raw_text) {
                let text = format!("{label}: {url}");
                let label_end = label.len();
                let separator_end = label_end.saturating_add(2).min(text.len());
                out.push(MarkdownPreviewLine {
                    spans: vec![
                        StyledTextSpan::with_style(
                            0,
                            label_end,
                            theme.syntax.function.as_u8(),
                            false,
                            false,
                        ),
                        StyledTextSpan::new(
                            label_end,
                            separator_end,
                            theme.syntax.punctuation.as_u8(),
                        ),
                        StyledTextSpan::new(separator_end, text.len(), theme.syntax.string.as_u8()),
                    ],
                    text,
                    block_type: MarkdownBlockType::Paragraph,
                    code_language: None,
                });
            } else if !raw_text.is_empty() {
                out.push(MarkdownPreviewLine {
                    text: raw_text.to_string(),
                    spans: vec![StyledTextSpan::new(
                        0,
                        raw_text.len(),
                        theme.syntax.identifier.as_u8(),
                    )],
                    block_type: MarkdownBlockType::Paragraph,
                    code_language: None,
                });
            }
        }
        "fenced_code_block" | "indented_code_block" => {
            let code_text = code_block_content(node, source);
            let lang = code_block_language(node, source);
            let preview_lang = if lang.trim().is_empty() {
                "txt".to_string()
            } else {
                lang.clone()
            };
            let raw_spans = highlight_snippet(&code_text, &lang, theme);
            let styled = syntax_spans_to_styled(&raw_spans, &code_text, theme);
            let mut line_start = 0usize;
            for line_str in code_text.lines() {
                let line_end = line_start + line_str.len();
                let spans = styled
                    .iter()
                    .filter_map(|span| clip_styled_span_to_line(*span, line_start, line_end))
                    .collect();
                out.push(MarkdownPreviewLine {
                    text: line_str.to_string(),
                    spans,
                    block_type: MarkdownBlockType::CodeBlock,
                    code_language: Some(preview_lang.clone()),
                });
                line_start = line_end.saturating_add(1);
            }
            if code_text.ends_with('\n') || code_text.is_empty() {
                out.push(MarkdownPreviewLine {
                    text: String::new(),
                    spans: Vec::new(),
                    block_type: MarkdownBlockType::CodeBlock,
                    code_language: Some(preview_lang.clone()),
                });
            }
        }
        "block_quote" => {
            let text = node_text(node, source);
            for line_str in text.lines() {
                let content = line_str.trim_start_matches('>').trim_start();
                let prefixed = format!("  │ {}", content);
                let prefix_len = prefixed.len().saturating_sub(content.len());
                let mut spans = vec![StyledTextSpan::new(
                    0,
                    prefix_len,
                    theme.syntax.constant.as_u8(),
                )];
                let (rendered_content, inline_spans) = render_markdown_inline_text(content, theme);
                let prefixed = format!("  │ {}", rendered_content);
                spans.extend(
                    inline_spans
                        .into_iter()
                        .map(|span| offset_styled_span(span, prefix_len)),
                );
                out.push(MarkdownPreviewLine {
                    text: prefixed,
                    spans,
                    block_type: MarkdownBlockType::BlockQuote,
                    code_language: None,
                });
            }
        }
        "list" | "tight_list" | "loose_list" => {
            render_children(node, source, theme, out);
        }
        "list_item" | "task_list_item" => {
            let indent = "  ".repeat(markdown_list_item_depth(node));
            let marker = format!("{indent}{}", list_marker_text(node, source));
            let marker_len = marker.len();
            let content = list_item_content(node, source);
            let (rendered_content, inline_spans) = render_markdown_inline_text(&content, theme);
            let full_text = format!("{marker}{rendered_content}");
            let mut spans = Vec::new();
            spans.push(StyledTextSpan::new(
                0,
                marker_len,
                theme.syntax.operator.as_u8(),
            ));
            if rendered_content.len() > 0 {
                spans.extend(
                    inline_spans
                        .into_iter()
                        .map(|span| offset_styled_span(span, marker_len)),
                );
                if spans.len() == 1 {
                    spans.push(StyledTextSpan::new(
                        marker_len,
                        full_text.len(),
                        theme.syntax.identifier.as_u8(),
                    ));
                }
            }
            out.push(MarkdownPreviewLine {
                text: full_text,
                spans,
                block_type: MarkdownBlockType::ListItem,
                code_language: None,
            });

            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                if matches!(child.kind(), "list" | "tight_list" | "loose_list") {
                    render_markdown_node(child, source, theme, out);
                }
            }
        }
        "thematic_break" => {
            // Use 80 chars for horizontal rule (will be wrapped by renderer if needed)
            let rule_width = 80;
            let rule_text = "─".repeat(rule_width);
            out.push(MarkdownPreviewLine {
                text: rule_text.clone(),
                spans: vec![StyledTextSpan::new(
                    0,
                    rule_text.len(),
                    theme.syntax.punctuation.as_u8(),
                )],
                block_type: MarkdownBlockType::HorizontalRule,
                code_language: None,
            });
        }
        "table" | "pipe_table" => {
            render_table(node, source, theme, out);
        }
        "html_block" => {
            let text = node_text(node, source);
            for line_str in text.lines() {
                out.push(MarkdownPreviewLine {
                    text: line_str.to_string(),
                    spans: vec![StyledTextSpan::new(
                        0,
                        line_str.len(),
                        theme.syntax.tag.as_u8(),
                    )],
                    block_type: MarkdownBlockType::Paragraph,
                    code_language: None,
                });
            }
        }
        _ => {
            render_children(node, source, theme, out);
        }
    }
}

fn render_children(
    node: tree_sitter::Node<'_>,
    source: &str,
    theme: &ThemeConfig,
    out: &mut Vec<MarkdownPreviewLine>,
) {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        render_markdown_node(child, source, theme, out);
    }
}

fn clip_styled_span_to_line(
    span: StyledTextSpan,
    line_start: usize,
    line_end: usize,
) -> Option<StyledTextSpan> {
    let start = span.start.max(line_start);
    let end = span.end.min(line_end);
    if start >= end {
        return None;
    }
    Some(StyledTextSpan::with_style(
        start.saturating_sub(line_start),
        end.saturating_sub(line_start),
        span.color_rgba,
        span.bold,
        span.italic,
    ))
}

fn offset_styled_span(span: StyledTextSpan, offset: usize) -> StyledTextSpan {
    StyledTextSpan::with_style(
        span.start.saturating_add(offset),
        span.end.saturating_add(offset),
        span.color_rgba,
        span.bold,
        span.italic,
    )
}

fn render_markdown_inline_text(text: &str, theme: &ThemeConfig) -> (String, Vec<StyledTextSpan>) {
    let mut rendered = String::new();
    let mut spans = Vec::new();
    let mut i = 0usize;

    while i < text.len() {
        let rest = &text[i..];
        if let Some(after_label_open) = rest.strip_prefix('[')
            && let Some(label_close_rel) = after_label_open.find(']')
        {
            let after_label = &after_label_open[label_close_rel + 1..];
            if let Some(after_url_open) = after_label.strip_prefix('(')
                && let Some(url_close_rel) = after_url_open.find(')')
            {
                let label = &after_label_open[..label_close_rel];
                let url = &after_url_open[..url_close_rel];
                let start = rendered.len();
                rendered.push_str(label);
                let label_end = rendered.len();
                if !url.trim().is_empty() {
                    rendered.push_str(" ↗");
                }
                let end = rendered.len();
                if label_end > start {
                    spans.push(StyledTextSpan::with_style(
                        start,
                        label_end,
                        theme.syntax.function.as_u8(),
                        false,
                        false,
                    ));
                }
                if end > label_end {
                    spans.push(StyledTextSpan::new(
                        label_end,
                        end,
                        theme.syntax.punctuation.as_u8(),
                    ));
                }
                i += 1 + label_close_rel + 1 + 1 + url_close_rel + 1;
                continue;
            }
        }
        if let Some(after_open) = rest.strip_prefix("**")
            && let Some(close_rel) = after_open.find("**")
        {
            let content = &after_open[..close_rel];
            let start = rendered.len();
            rendered.push_str(content);
            let end = rendered.len();
            if end > start {
                spans.push(StyledTextSpan::with_style(
                    start,
                    end,
                    theme.syntax.keyword.as_u8(),
                    true,
                    false,
                ));
            }
            i += 2 + close_rel + 2;
            continue;
        }
        if let Some(after_open) = rest.strip_prefix('`')
            && let Some(close_rel) = after_open.find('`')
        {
            let content = &after_open[..close_rel];
            let start = rendered.len();
            rendered.push_str(content);
            let end = rendered.len();
            if end > start {
                spans.push(StyledTextSpan::new(start, end, theme.syntax.string.as_u8()));
            }
            i += 1 + close_rel + 1;
            continue;
        }
        if let Some(after_open) = rest.strip_prefix('*')
            && let Some(close_rel) = after_open.find('*')
        {
            let content = &after_open[..close_rel];
            let start = rendered.len();
            rendered.push_str(content);
            let end = rendered.len();
            if end > start {
                spans.push(StyledTextSpan::with_style(
                    start,
                    end,
                    theme.syntax.comment.as_u8(),
                    false,
                    true,
                ));
            }
            i += 1 + close_rel + 1;
            continue;
        }

        if let Some(ch) = rest.chars().next() {
            rendered.push(ch);
            i += ch.len_utf8();
        } else {
            break;
        }
    }

    (rendered, spans)
}

fn parse_link_reference_definition(text: &str) -> Option<(String, String)> {
    let after_open = text.strip_prefix('[')?;
    let close_idx = after_open.find("]: ").or_else(|| after_open.find("]:"))?;
    let label = after_open[..close_idx].trim();
    if label.is_empty() {
        return None;
    }
    let after_label = &after_open[close_idx + 1..];
    let url = after_label.strip_prefix(':')?.trim();
    if url.is_empty() {
        return None;
    }
    Some((label.to_string(), url.to_string()))
}

fn heading_level_from_atx(node: tree_sitter::Node<'_>, _source: &str) -> u8 {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        match child.kind() {
            "atx_h1_marker" => return 1,
            "atx_h2_marker" => return 2,
            "atx_h3_marker" => return 3,
            "atx_h4_marker" => return 4,
            "atx_h5_marker" => return 5,
            "atx_h6_marker" => return 6,
            _ => {}
        }
    }
    1
}

fn heading_content_text(node: tree_sitter::Node<'_>, source: &str) -> String {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "inline" {
            return child
                .utf8_text(source.as_bytes())
                .unwrap_or("")
                .trim()
                .to_string();
        }
    }
    node_text(node, source)
}

fn setext_heading_level(node: tree_sitter::Node<'_>, _source: &str) -> u8 {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        match child.kind() {
            "setext_h1_underline" => return 1,
            "setext_h2_underline" => return 2,
            _ => {}
        }
    }
    1
}

fn setext_heading_content(node: tree_sitter::Node<'_>, source: &str) -> String {
    let mut lines = Vec::new();
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() != "setext_h1_underline" && child.kind() != "setext_h2_underline" {
            let text = child.utf8_text(source.as_bytes()).unwrap_or("").to_string();
            if !text.is_empty() {
                lines.push(text);
            }
        }
    }
    lines.join(" ").trim().to_string()
}

fn heading_spans(
    text: &str,
    inline_spans: Vec<StyledTextSpan>,
    color: [u8; 4],
    level: u8,
) -> Vec<StyledTextSpan> {
    if text.is_empty() {
        return Vec::new();
    }
    let heading_bold = level <= 2;
    let mut spans = vec![StyledTextSpan::with_style(
        0,
        text.len(),
        color,
        heading_bold,
        false,
    )];
    spans.extend(inline_spans.into_iter().map(|span| {
        StyledTextSpan::with_style(
            span.start,
            span.end,
            span.color_rgba,
            heading_bold || span.bold,
            span.italic,
        )
    }));
    spans
}

fn heading_color(level: u8, theme: &ThemeConfig) -> [u8; 4] {
    match level {
        1 => theme.syntax.keyword.as_u8(),
        2 => theme.syntax.function.as_u8(),
        3 => theme.syntax.r#type.as_u8(),
        _ => theme.syntax.constant.as_u8(),
    }
}

fn node_text(node: tree_sitter::Node<'_>, source: &str) -> String {
    node.utf8_text(source.as_bytes()).unwrap_or("").to_string()
}

fn code_block_content(node: tree_sitter::Node<'_>, source: &str) -> String {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "code_fence_content" {
            return node_text(child, source);
        }
        if child.kind() == "text" {
            return node_text(child, source);
        }
    }
    node_text(node, source)
}

fn code_block_language(node: tree_sitter::Node<'_>, source: &str) -> String {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "info_string" || child.kind() == "language" {
            return node_text(child, source).trim().to_string();
        }
    }
    String::new()
}

fn markdown_list_item_depth(node: tree_sitter::Node<'_>) -> usize {
    let mut depth = 0usize;
    let mut parent = node.parent();
    while let Some(current) = parent {
        if matches!(current.kind(), "list_item" | "task_list_item") {
            depth = depth.saturating_add(1);
        }
        parent = current.parent();
    }
    depth
}

fn list_marker_text(node: tree_sitter::Node<'_>, source: &str) -> String {
    let raw = node_text(node, source);
    let trimmed = raw.trim_start();
    if trimmed.starts_with("- [x]")
        || trimmed.starts_with("* [x]")
        || trimmed.starts_with("+ [x]")
        || trimmed.starts_with("- [X]")
        || trimmed.starts_with("* [X]")
        || trimmed.starts_with("+ [X]")
    {
        return "☑ ".to_string();
    }
    if trimmed.starts_with("- [ ]") || trimmed.starts_with("* [ ]") || trimmed.starts_with("+ [ ]")
    {
        return "☐ ".to_string();
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        match child.kind() {
            "list_marker_plus"
            | "list_marker_minus"
            | "list_marker_star"
            | "list_marker_dot"
            | "list_marker_parenthesis"
            | "list_marker"
            | "task_list_item_marker" => {
                return format!("{} ", node_text(child, source).trim());
            }
            "task_list_marker_checked" => return "☑ ".to_string(),
            "task_list_marker_unchecked" => return "☐ ".to_string(),
            _ => {}
        }
    }
    "• ".to_string()
}

fn list_item_content(node: tree_sitter::Node<'_>, source: &str) -> String {
    let raw = node_text(node, source);
    let first_line = raw.lines().next().unwrap_or_default();
    let trimmed = first_line.trim_start();
    for prefix in [
        "- [x]", "* [x]", "+ [x]", "- [X]", "* [X]", "+ [X]", "- [ ]", "* [ ]", "+ [ ]",
    ] {
        if let Some(rest) = trimmed.strip_prefix(prefix) {
            return rest.trim_start().to_string();
        }
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        match child.kind() {
            "list_marker_plus"
            | "list_marker_minus"
            | "list_marker_star"
            | "list_marker_dot"
            | "list_marker_parenthesis"
            | "task_list_marker_checked"
            | "task_list_marker_unchecked"
            | "list_marker"
            | "task_list_item_marker"
            | "paragraph_continuation"
            | "list"
            | "tight_list"
            | "loose_list" => {}
            _ => {
                let text = node_text(child, source);
                if !text.is_empty() {
                    return text.lines().next().unwrap_or_default().trim().to_string();
                }
            }
        }
    }

    let marker = list_marker_text(node, source);
    trimmed
        .strip_prefix(marker.trim())
        .unwrap_or(trimmed)
        .trim_start()
        .to_string()
}

fn render_table(
    node: tree_sitter::Node<'_>,
    source: &str,
    theme: &ThemeConfig,
    out: &mut Vec<MarkdownPreviewLine>,
) {
    let raw = node_text(node, source);
    let mut rows: Vec<Vec<String>> = Vec::new();
    let mut header_idx: Option<usize> = None;

    for line in raw.lines() {
        let trimmed = line.trim();
        if !trimmed.starts_with('|') || !trimmed.ends_with('|') {
            continue;
        }
        let cells: Vec<String> = trimmed
            .trim_matches('|')
            .split('|')
            .map(|cell| cell.trim().to_string())
            .collect();
        if cells.is_empty() {
            continue;
        }
        let is_delimiter = cells.iter().all(|cell| {
            let clean = cell.trim();
            !clean.is_empty()
                && clean.chars().all(|ch| matches!(ch, '-' | ':' | ' ' | '\t'))
                && clean.chars().any(|ch| ch == '-')
        });
        if is_delimiter {
            if !rows.is_empty() {
                header_idx = Some(rows.len().saturating_sub(1));
            }
            continue;
        }
        rows.push(cells);
    }

    if rows.is_empty() {
        render_children(node, source, theme, out);
        return;
    }

    let col_count = rows.iter().map(Vec::len).max().unwrap_or(0);
    let rendered_rows: Vec<Vec<(String, Vec<StyledTextSpan>)>> = rows
        .iter()
        .map(|row| {
            row.iter()
                .map(|cell| render_markdown_inline_text(cell, theme))
                .collect()
        })
        .collect();
    let mut widths = vec![0usize; col_count];
    for row in &rendered_rows {
        for (idx, (cell, _)) in row.iter().enumerate() {
            widths[idx] = widths[idx].max(display_width(cell));
        }
    }

    push_table_rule(out, &widths, '┌', '┬', '┐', theme);

    for (row_idx, row) in rendered_rows.iter().enumerate() {
        let mut text = String::new();
        let mut spans = Vec::new();
        text.push('│');
        spans.push(StyledTextSpan::new(
            0,
            text.len(),
            theme.syntax.punctuation.as_u8(),
        ));

        for idx in 0..col_count {
            text.push(' ');
            let cell_start = text.len();
            let (cell, cell_spans) = row
                .get(idx)
                .map(|(cell, spans)| (cell.as_str(), spans.as_slice()))
                .unwrap_or(("", &[]));
            text.push_str(cell);
            for span in cell_spans {
                spans.push(offset_styled_span(*span, cell_start));
            }
            if cell_spans.is_empty() && !cell.is_empty() {
                spans.push(StyledTextSpan::new(
                    cell_start,
                    text.len(),
                    theme.syntax.identifier.as_u8(),
                ));
            }
            let pad = widths[idx].saturating_sub(display_width(cell));
            text.extend(std::iter::repeat(' ').take(pad));
            text.push(' ');
            let border_start = text.len();
            text.push('│');
            spans.push(StyledTextSpan::new(
                border_start,
                text.len(),
                theme.syntax.punctuation.as_u8(),
            ));
        }

        let is_header = header_idx == Some(row_idx);
        if is_header {
            spans.push(StyledTextSpan::with_style(
                0,
                text.len(),
                theme.syntax.keyword.as_u8(),
                true,
                false,
            ));
        }
        out.push(MarkdownPreviewLine {
            text,
            spans,
            block_type: if is_header {
                MarkdownBlockType::TableHeader
            } else {
                MarkdownBlockType::TableRow
            },
            code_language: None,
        });

        if is_header {
            push_table_rule(out, &widths, '├', '┼', '┤', theme);
        }
    }

    push_table_rule(out, &widths, '└', '┴', '┘', theme);
}

fn push_table_rule(
    out: &mut Vec<MarkdownPreviewLine>,
    widths: &[usize],
    left: char,
    join: char,
    right: char,
    theme: &ThemeConfig,
) {
    if widths.is_empty() {
        return;
    }

    let mut text = String::new();
    text.push(left);
    for (idx, width) in widths.iter().enumerate() {
        text.extend(std::iter::repeat('─').take(width.saturating_add(2)));
        text.push(if idx + 1 == widths.len() { right } else { join });
    }
    out.push(MarkdownPreviewLine {
        spans: vec![StyledTextSpan::new(
            0,
            text.len(),
            theme.syntax.punctuation.as_u8(),
        )],
        text,
        block_type: MarkdownBlockType::TableRow,
        code_language: None,
    });
}

pub(super) fn scale_theme(base: &ThemeConfig, scale: f32) -> ThemeConfig {
    let mut theme = base.clone();
    theme.editor.font_size = scale_metric(theme.editor.font_size, scale, 8.0);
    theme.editor.line_height = scale_metric(theme.editor.line_height, scale, 12.0);
    theme.ui.sidebar_font_size = scale_metric(theme.ui.sidebar_font_size, scale, 8.0);
    theme.ui.sidebar_line_height = scale_metric(theme.ui.sidebar_line_height, scale, 12.0);
    theme.ui.panel_font_size = scale_metric(theme.ui.panel_font_size, scale, 8.0);
    theme.ui.panel_line_height = scale_metric(theme.ui.panel_line_height, scale, 12.0);
    theme.ui.sidebar_width = scale_metric(theme.ui.sidebar_width, scale, 120.0);
    theme.ui.right_sidebar_width = scale_metric(theme.ui.right_sidebar_width, scale, 120.0);
    theme.ui.bottom_panel_height = scale_metric(theme.ui.bottom_panel_height, scale, 80.0);
    theme.ui.top_bar_height = scale_metric(theme.ui.top_bar_height, scale, 22.0);
    theme.ui.status_bar_height = scale_metric(theme.ui.status_bar_height, scale, 18.0);
    theme
}

pub(super) fn scale_ui_config(base: &UiConfig, scale: f32) -> UiConfig {
    let mut ui = base.clone();
    ui.layout.outer_gap = scale_metric(ui.layout.outer_gap, scale, 0.0);
    ui.layout.panel_gap = scale_metric(ui.layout.panel_gap, scale, 0.0);
    ui.layout.inner_padding = scale_metric(ui.layout.inner_padding, scale, 0.0);
    ui.layout.top_bar_height = scale_metric(ui.layout.top_bar_height, scale, 20.0);
    ui.layout.status_bar_height = scale_metric(ui.layout.status_bar_height, scale, 18.0);
    ui.layout.center_min_width = scale_metric(ui.layout.center_min_width, scale, 240.0);
    ui.layout.center_min_height = scale_metric(ui.layout.center_min_height, scale, 120.0);
    ui.layout.sidebar_min_width = scale_metric(ui.layout.sidebar_min_width, scale, 140.0);
    ui.layout.bottom_min_height = scale_metric(ui.layout.bottom_min_height, scale, 80.0);

    ui.docks.left.size_px = scale_metric(ui.docks.left.size_px, scale, 120.0);
    ui.docks.right.size_px = scale_metric(ui.docks.right.size_px, scale, 120.0);
    ui.docks.bottom.size_px = scale_metric(ui.docks.bottom.size_px, scale, 80.0);

    ui.cursor.beam_width = scale_metric(ui.cursor.beam_width, scale, 1.0);
    ui.cursor.block_width = scale_metric(ui.cursor.block_width, scale, 6.0);
    ui.cursor.underline_height = scale_metric(ui.cursor.underline_height, scale, 1.0);

    ui.spacing.editor_padding = scale_metric(ui.spacing.editor_padding, scale, 4.0);
    ui.spacing.panel_padding = scale_metric(ui.spacing.panel_padding, scale, 4.0);
    ui.spacing.explorer_padding = scale_metric(ui.spacing.explorer_padding, scale, 4.0);

    ui.status_bar.padding_x = scale_metric(ui.status_bar.padding_x, scale, 4.0);
    ui.status_bar.font_size = scale_metric(ui.status_bar.font_size, scale, 8.0);
    ui.status_bar.line_height = scale_metric(ui.status_bar.line_height, scale, 12.0);

    ui.welcome.card_max_width = scale_metric(ui.welcome.card_max_width, scale, 320.0);
    ui.welcome.card_padding_x = scale_metric(ui.welcome.card_padding_x, scale, 12.0);
    ui.welcome.card_padding_y = scale_metric(ui.welcome.card_padding_y, scale, 12.0);
    ui.welcome.section_gap = scale_metric(ui.welcome.section_gap, scale, 6.0);
    ui.welcome.border_radius_px = scale_metric(ui.welcome.border_radius_px, scale, 4.0);
    ui
}

fn scale_metric(value: f32, scale: f32, min: f32) -> f32 {
    (value * scale).max(min)
}

pub(super) fn collect_explorer_entries(app_state: &AppState) -> Vec<ExplorerEntry> {
    let Some(nodes) = app_state.workspace_nodes() else {
        return Vec::new();
    };
    let root = match app_state.workspace_root_path() {
        Some(root) => root.to_path_buf(),
        None => return Vec::new(),
    };

    let mut node_types: HashMap<PathBuf, WorkspaceNodeType> = HashMap::new();
    let mut hidden_flags: HashMap<PathBuf, bool> = HashMap::new();
    let mut ignored_flags: HashMap<PathBuf, bool> = HashMap::new();
    let mut children_map: HashMap<PathBuf, Vec<PathBuf>> = HashMap::new();

    for node in nodes {
        node_types.insert(node.path.clone(), node.file_type);
        hidden_flags.insert(node.path.clone(), node.is_hidden);
        ignored_flags.insert(node.path.clone(), node.is_ignored);
    }

    for node in nodes.iter() {
        if node.path == root {
            continue;
        }
        let Some(parent) = node.path.parent() else {
            continue;
        };
        if !parent.starts_with(&root) {
            continue;
        }
        children_map
            .entry(parent.to_path_buf())
            .or_default()
            .push(node.path.clone());
    }

    for children in children_map.values_mut() {
        children.sort_by(|left, right| {
            let left_type = node_types
                .get(left)
                .copied()
                .unwrap_or(WorkspaceNodeType::File);
            let right_type = node_types
                .get(right)
                .copied()
                .unwrap_or(WorkspaceNodeType::File);
            let left_rank = if left_type == WorkspaceNodeType::Folder {
                0
            } else {
                1
            };
            let right_rank = if right_type == WorkspaceNodeType::Folder {
                0
            } else {
                1
            };
            left_rank.cmp(&right_rank).then_with(|| left.cmp(right))
        });
    }

    let filter_query = app_state
        .workspace_filter_query()
        .map(str::trim)
        .filter(|query| !query.is_empty())
        .map(str::to_lowercase);
    let mut subtree_matches: HashMap<PathBuf, bool> = HashMap::new();
    if let Some(query) = filter_query.as_deref() {
        compute_filter_matches(&root, &children_map, query, &mut subtree_matches);
    }

    let mut entries = Vec::new();
    collect_visible_explorer_entries(
        app_state,
        &root,
        0,
        &node_types,
        &hidden_flags,
        &ignored_flags,
        &children_map,
        filter_query.as_deref(),
        &subtree_matches,
        &mut entries,
    );
    entries
}

fn compute_filter_matches(
    path: &Path,
    children_map: &HashMap<PathBuf, Vec<PathBuf>>,
    query: &str,
    out: &mut HashMap<PathBuf, bool>,
) -> bool {
    let self_match = path
        .file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.to_lowercase().contains(query));
    let mut subtree_match = self_match;

    if let Some(children) = children_map.get(path) {
        for child in children {
            subtree_match |= compute_filter_matches(child, children_map, query, out);
        }
    }

    out.insert(path.to_path_buf(), subtree_match);
    subtree_match
}

fn collect_visible_explorer_entries(
    app_state: &AppState,
    parent: &Path,
    depth: usize,
    node_types: &HashMap<PathBuf, WorkspaceNodeType>,
    hidden_flags: &HashMap<PathBuf, bool>,
    ignored_flags: &HashMap<PathBuf, bool>,
    children_map: &HashMap<PathBuf, Vec<PathBuf>>,
    filter_query: Option<&str>,
    subtree_matches: &HashMap<PathBuf, bool>,
    out: &mut Vec<ExplorerEntry>,
) {
    let Some(children) = children_map.get(parent) else {
        return;
    };

    for child in children {
        let file_type = node_types
            .get(child)
            .copied()
            .unwrap_or(WorkspaceNodeType::File);
        let name = child
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("?");
        let subtree_match =
            filter_query.is_none_or(|_| subtree_matches.get(child).copied().unwrap_or(false));
        if !subtree_match {
            continue;
        }

        let has_matching_descendant = filter_query.is_some()
            && file_type == WorkspaceNodeType::Folder
            && children_map.get(child).is_some_and(|children| {
                children
                    .iter()
                    .any(|nested| subtree_matches.get(nested).copied().unwrap_or(false))
            });
        let is_expanded = file_type == WorkspaceNodeType::Folder
            && (app_state.workspace_is_expanded(child) || has_matching_descendant);

        out.push(ExplorerEntry {
            path: child.clone(),
            parent_path: Some(parent.to_path_buf()),
            file_type,
            depth,
            is_expanded,
            name: name.to_string(),
            git_status: {
                let is_dirty_path = app_state.is_dirty()
                    && app_state
                        .active_file()
                        .is_some_and(|active| active.starts_with(child));
                if is_dirty_path {
                    Some(WorkspaceGitStatus::Dirty)
                } else {
                    app_state.workspace_git_status(child)
                }
            },
            is_hidden: hidden_flags.get(child).copied().unwrap_or(false),
            is_ignored: ignored_flags.get(child).copied().unwrap_or(false),
        });

        if file_type == WorkspaceNodeType::Folder && is_expanded {
            collect_visible_explorer_entries(
                app_state,
                child,
                depth + 1,
                node_types,
                hidden_flags,
                ignored_flags,
                children_map,
                filter_query,
                subtree_matches,
                out,
            );
        }
    }
}

pub(super) fn build_sidebar_rows(
    entries: &[ExplorerEntry],
    selected_idx: usize,
    theme: &ThemeConfig,
    filter_active: bool,
    scroll_offset_rows: usize,
) -> Vec<SidebarRow> {
    if entries.is_empty() {
        return vec![SidebarRow {
            path: None,
            depth: 0,
            arrow: theme.sidebar_arrow(false, false).to_string(),
            nerd_icon: theme.icon_theme_for_filename("", false).glyph.clone(),
            icon_color: theme.icons.default_file.color.as_f32(),
            label: if filter_active {
                "(no matches)".to_string()
            } else {
                "(no files)".to_string()
            },
            prefix_marker: None,
            prefix_color: None,
            git_marker: None,
            git_color: None,
            is_selected: false,
        }];
    }

    let selected = selected_idx.min(entries.len().saturating_sub(1));
    let scroll_start = scroll_offset_rows.min(entries.len().saturating_sub(1));
    entries
        .iter()
        .enumerate()
        .skip(scroll_start)
        .map(|(idx, entry)| {
            let is_dir = entry.file_type == WorkspaceNodeType::Folder;
            let arrow = theme.sidebar_arrow(is_dir, entry.is_expanded).to_string();
            let is_hidden_or_ignored = entry.is_hidden || entry.is_ignored;
            let icon_theme = theme.icon_theme_for_path(&entry.path, is_dir, entry.is_expanded);
            let filename = entry
                .path
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or(entry.name.as_str());
            let icon = if is_hidden_or_ignored {
                if is_dir {
                    "󱞞".to_string()
                } else {
                    "󰘓".to_string()
                }
            } else {
                theme.get_icon_for_file(filename, is_dir).to_string()
            };
            SidebarRow {
                path: Some(entry.path.clone()),
                depth: entry.depth,
                arrow,
                nerd_icon: icon,
                icon_color: if is_hidden_or_ignored {
                    theme.ui.warning.as_f32()
                } else {
                    icon_theme.color.as_f32()
                },
                label: entry.name.clone(),
                prefix_marker: None,
                prefix_color: None,
                git_marker: match entry.git_status {
                    Some(WorkspaceGitStatus::Modified) => Some('M'),
                    Some(WorkspaceGitStatus::Added) => Some('A'),
                    Some(WorkspaceGitStatus::Dirty) => Some('●'),
                    None => None,
                },
                git_color: match entry.git_status {
                    Some(WorkspaceGitStatus::Modified) => Some(theme.git.modified_sidebar.as_f32()),
                    Some(WorkspaceGitStatus::Added) => Some(theme.git.added_sidebar.as_f32()),
                    Some(WorkspaceGitStatus::Dirty) => Some(theme.git.modified_sidebar.as_f32()),
                    None => None,
                },
                is_selected: idx == selected,
            }
        })
        .collect()
}

pub(super) fn region_color(id: RegionId, theme: &ThemeConfig) -> [f32; 4] {
    match id {
        RegionId::TopBar => theme.ui.panel_bg.as_f32(),
        RegionId::LeftSidebar => theme.ui.sidebar_bg.as_f32(),
        RegionId::Center => theme.editor.bg.as_f32(),
        RegionId::RightSidebar => theme.ui.sidebar_bg.as_f32(),
        RegionId::BottomPanel => theme.ui.terminal_bg.as_f32(),
        RegionId::StatusBar => theme.ui.status_bar_bg.as_f32(),
        _ => theme.ui.border_color.as_f32(),
    }
}

pub(super) fn language_id_for_path(path: &Path) -> String {
    if let Some(profile) = crate::lsp::registry::language_profile_for_path(path) {
        return profile.language_id.to_string();
    }

    match path.extension().and_then(|ext| ext.to_str()) {
        Some("toml") => "toml",
        Some("md") => "markdown",
        _ => "plaintext",
    }
    .to_string()
}

pub(super) fn detect_git_branch(root: &Path) -> Option<String> {
    let git_dir = find_git_dir(root)?;
    let head = fs::read_to_string(git_dir.join("HEAD")).ok()?;
    parse_git_head(head.trim())
}

fn find_git_dir(start: &Path) -> Option<PathBuf> {
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

/// Shift global byte coordinates to local (0-based) by subtracting `offset`.
/// Filters out spans that collapse to zero length after offsetting.
#[allow(dead_code)]
fn normalize_spans(spans: Vec<StyledTextSpan>, offset: usize) -> Vec<StyledTextSpan> {
    spans
        .into_iter()
        .filter_map(|mut span| {
            span.start = span.start.saturating_sub(offset);
            span.end = span.end.saturating_sub(offset);
            if span.start < span.end {
                Some(span)
            } else {
                None
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{display_width, normalize_spans, syntax_spans_to_styled};
    use crate::{
        config::theme_config::ThemeConfig,
        syntax::highlight::{HighlightCategory, HighlightSpan},
        text::text_system::StyledTextSpan,
    };

    #[test]
    fn syntax_spans_to_styled_applies_theme_colors_and_emphasis() {
        let theme = ThemeConfig::builtin_dark();
        let spans = vec![
            HighlightSpan {
                range: 0..4,
                category: HighlightCategory::Comment,
            },
            HighlightSpan {
                range: 5..12,
                category: HighlightCategory::Macro,
            },
            HighlightSpan {
                range: 13..17,
                category: HighlightCategory::Parameter,
            },
        ];

        let styled = syntax_spans_to_styled(&spans, "comment macro param", &theme);

        assert_eq!(styled[0].color_rgba, theme.syntax.comment.as_u8());
        assert!(styled[0].italic);
        assert!(!styled[0].bold);

        assert_eq!(styled[1].color_rgba, theme.syntax.r#macro.as_u8());
        assert!(styled[1].bold);
        assert!(!styled[1].italic);

        assert_eq!(styled[2].color_rgba, theme.syntax.parameter.as_u8());
        assert!(!styled[2].bold);
        assert!(!styled[2].italic);
    }

    #[test]
    fn normalize_spans_shifts_global_coordinates_to_local() {
        let spans = vec![
            StyledTextSpan::with_style(10, 12, [255, 0, 0, 255], true, false),
            StyledTextSpan::new(15, 17, [0, 255, 0, 255]),
            StyledTextSpan::with_style(10, 11, [0, 0, 255, 255], false, true),
        ];
        let normalized = normalize_spans(spans, 10);
        assert_eq!(normalized.len(), 3);
        assert_eq!(normalized[0].start, 0);
        assert_eq!(normalized[0].end, 2);
        assert!(normalized[0].bold);
        assert_eq!(normalized[1].start, 5);
        assert_eq!(normalized[1].end, 7);
        assert_eq!(normalized[2].start, 0);
        assert_eq!(normalized[2].end, 1);
        assert!(normalized[2].italic);
    }

    #[test]
    fn normalize_spans_filters_zero_length() {
        let spans = vec![StyledTextSpan::new(5, 5, [255, 0, 0, 255])];
        let normalized = normalize_spans(spans, 0);
        assert!(normalized.is_empty());
    }

    #[test]
    fn normalize_spans_clamps_negative_start_to_zero() {
        let spans = vec![StyledTextSpan::new(5, 15, [255, 0, 0, 255])];
        let normalized = normalize_spans(spans, 10);
        assert_eq!(normalized.len(), 1);
        assert_eq!(normalized[0].start, 0);
        assert_eq!(normalized[0].end, 5);
    }

    #[test]
    fn display_width_handles_ascii() {
        assert_eq!(display_width("hello"), 5);
        assert_eq!(display_width("API Endpoint"), 12);
    }

    #[test]
    fn display_width_handles_vietnamese() {
        // Vietnamese with diacritics should count as 1 per char
        assert_eq!(display_width("Tình trạng"), 10);
        assert_eq!(display_width("Xung đột"), 8);
        assert_eq!(display_width("Tương đồng"), 10);
        assert_eq!(display_width("đ"), 1); // single char with diacritic
    }

    #[test]
    fn display_width_handles_mixed_content() {
        // Mix of ASCII and Vietnamese
        assert_eq!(display_width("Source A"), 8);
        assert_eq!(display_width("Common"), 6);
        assert_eq!(display_width("Conflict"), 8);
    }

    #[test]
    fn display_width_handles_cjk() {
        // CJK characters should count as 2
        assert_eq!(display_width("你好"), 4); // 2 chars × 2 width
        assert_eq!(display_width("日本語"), 6); // 3 chars × 2 width
    }
}

fn parse_git_head(head: &str) -> Option<String> {
    if let Some(reference) = head.strip_prefix("ref:") {
        return reference
            .trim()
            .rsplit('/')
            .next()
            .map(str::to_string)
            .filter(|branch| !branch.is_empty());
    }

    (!head.is_empty()).then(|| {
        let short_len = head.len().min(7);
        format!("detached: {}", &head[..short_len])
    })
}
