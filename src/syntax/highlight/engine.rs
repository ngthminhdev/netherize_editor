use std::ops::Range;
use streaming_iterator::StreamingIterator;
use tree_sitter::{Node, Query, QueryCursor};

use super::categories::HighlightCategory;
use super::queries::{highlight_query, injection_query};
use super::spans::HighlightSpan;
use crate::syntax::syntax_engine::{LanguageId, SyntaxEngine};

pub(crate) fn generate_query_highlight_spans(
    language_id: LanguageId,
    root: Node<'_>,
    source: &str,
    byte_window: Option<Range<usize>>,
) -> Vec<HighlightSpan> {
    let Some(query) = highlight_query(language_id) else {
        return Vec::new();
    };
    let sanitized_window =
        byte_window.and_then(|window| sanitize_byte_range(source, window).map(|(s, e)| s..e));
    let mut cursor = QueryCursor::new();
    let mut raw_spans = Vec::new();
    let mut query_matches = cursor.matches(query, root, source.as_bytes());
    if let Some(window) = sanitized_window.clone() {
        query_matches.set_byte_range(window);
    }

    loop {
        query_matches.advance();
        let Some(query_match) = query_matches.get() else {
            break;
        };
        for capture in query_match.captures {
            let node = capture.node;
            if node.is_error() || node.is_missing() {
                continue;
            }

            let start = node.start_byte();
            let end = node.end_byte();
            if end <= start || end > source.len() {
                continue;
            }

            if let Some(window) = &sanitized_window
                && (end <= window.start || start >= window.end)
            {
                continue;
            }

            let capture_name = query.capture_names()[capture.index as usize];
            let Some(category) = capture_category(capture_name) else {
                continue;
            };

            raw_spans.push(HighlightSpan {
                range: start..end,
                category,
            });
        }
    }

    normalize_spans(source, raw_spans, sanitized_window)
}

pub(crate) fn generate_injection_highlights(
    language_id: LanguageId,
    root: Node<'_>,
    source: &str,
) -> Vec<HighlightSpan> {
    let Some(injection_q) = injection_query(language_id) else {
        return Vec::new();
    };

    let injected_lang = injection_language_for_query(injection_q);
    let Some(hl_query) = highlight_query(injected_lang) else {
        return Vec::new();
    };

    let mut spans = Vec::new();
    let mut cursor = QueryCursor::new();
    let mut matches = cursor.matches(injection_q, root, source.as_bytes());

    loop {
        matches.advance();
        let Some(m) = matches.get() else {
            break;
        };

        for capture in m.captures {
            let name = injection_q.capture_names()[capture.index as usize];
            if name != "injection.content" {
                continue;
            }

            let node = capture.node;
            if node.is_error() || node.is_missing() {
                continue;
            }

            let node_start = node.start_byte();
            let node_end = node.end_byte();
            if node_end <= node_start || node_end > source.len() {
                continue;
            }

            let content = &source[node_start..node_end];
            if content.is_empty() {
                continue;
            }

            let Ok(mut eng) = SyntaxEngine::new(injected_lang) else {
                continue;
            };
            let Ok(tree) = eng.parse_source(content, 0) else {
                continue;
            };

            let inner =
                generate_query_highlight_spans_for_node(hl_query, tree.root_node(), content);

            for mut span in inner {
                span.range = (span.range.start + node_start)..(span.range.end + node_start);
                spans.push(span);
            }
        }
    }

    spans
}

pub(crate) fn generate_query_highlight_spans_for_node(
    query: &Query,
    root: Node<'_>,
    source: &str,
) -> Vec<HighlightSpan> {
    let mut cursor = QueryCursor::new();
    let mut raw_spans = Vec::new();
    let mut query_matches = cursor.matches(query, root, source.as_bytes());

    loop {
        query_matches.advance();
        let Some(query_match) = query_matches.get() else {
            break;
        };
        for capture in query_match.captures {
            let node = capture.node;
            if node.is_error() || node.is_missing() {
                continue;
            }

            let start = node.start_byte();
            let end = node.end_byte();
            if end <= start || end > source.len() {
                continue;
            }

            let capture_name = query.capture_names()[capture.index as usize];
            let Some(category) = capture_category(capture_name) else {
                continue;
            };

            raw_spans.push(HighlightSpan {
                range: start..end,
                category,
            });
        }
    }

    normalize_spans(source, raw_spans, None)
}

fn injection_language_for_query(query: &Query) -> LanguageId {
    for i in 0..query.pattern_count() {
        for prop in query.property_settings(i) {
            if prop.key.as_ref() == "injection.language" {
                if let Some(ref val) = prop.value {
                    return match val.as_ref() {
                        "bash" => LanguageId::Bash,
                        "json" => LanguageId::Json,
                        "yaml" => LanguageId::Yaml,
                        _ => LanguageId::Bash,
                    };
                }
            }
        }
    }
    LanguageId::Bash
}

pub(crate) fn capture_category(capture_name: &str) -> Option<HighlightCategory> {
    match capture_name {
        "string.special.key" => Some(HighlightCategory::Property),
        "markup.strong" => Some(HighlightCategory::MarkupStrong),
        "markup.italic" => Some(HighlightCategory::MarkupItalic),
        "markup.raw.inline" => Some(HighlightCategory::MarkupInlineCode),
        "markup.link.text" => Some(HighlightCategory::MarkupLink),
        "syntax.keyword" => Some(HighlightCategory::Keyword),
        "syntax.string" => Some(HighlightCategory::String),
        "syntax.comment" => Some(HighlightCategory::Comment),
        "syntax.type" => Some(HighlightCategory::Type),
        "syntax.function" => Some(HighlightCategory::Function),
        "syntax.number" => Some(HighlightCategory::Number),
        "syntax.boolean" => Some(HighlightCategory::Boolean),
        "syntax.identifier" => Some(HighlightCategory::Identifier),
        "syntax.variable" => Some(HighlightCategory::Variable),
        "syntax.parameter" => Some(HighlightCategory::Parameter),
        "syntax.field" => Some(HighlightCategory::Field),
        "syntax.property" => Some(HighlightCategory::Property),
        "syntax.constant" => Some(HighlightCategory::Constant),
        "syntax.operator" => Some(HighlightCategory::Operator),
        "syntax.punctuation"
        | "punctuation.bracket"
        | "punctuation.delimiter"
        | "punctuation.special" => Some(HighlightCategory::Punctuation),
        "syntax.escape" => Some(HighlightCategory::Escape),
        "syntax.macro" => Some(HighlightCategory::Macro),
        "syntax.lifetime" => Some(HighlightCategory::Lifetime),
        "syntax.constructor" => Some(HighlightCategory::Constructor),
        "syntax.attribute" => Some(HighlightCategory::Attribute),
        "syntax.namespace" => Some(HighlightCategory::Namespace),
        "syntax.tag" => Some(HighlightCategory::Tag),
        "macro" | "function.macro" | "constructor.macro" => Some(HighlightCategory::Macro),
        "lifetime" => Some(HighlightCategory::Lifetime),
        "attribute" | "attribute.builtin" | "attribute.attribute" => {
            Some(HighlightCategory::Attribute)
        }
        "field" => Some(HighlightCategory::Field),
        "property" => Some(HighlightCategory::Property),
        "constructor" => Some(HighlightCategory::Constructor),
        "number" => Some(HighlightCategory::Number),
        "constant" => Some(HighlightCategory::Constant),
        "type.builtin" | "builtin.type" => Some(HighlightCategory::Type),
        "constant.builtin.boolean" | "boolean" => Some(HighlightCategory::Boolean),
        "constant.builtin" => Some(HighlightCategory::Constant),
        "function.builtin" | "function.method" | "method.call" => Some(HighlightCategory::Function),
        "module" | "namespace" => Some(HighlightCategory::Namespace),
        "tag" | "tag.builtin" => Some(HighlightCategory::Tag),
        "label" => Some(HighlightCategory::Property),
        "identifier" => Some(HighlightCategory::Identifier),
        "variable" => Some(HighlightCategory::Variable),
        "variable.builtin" => Some(HighlightCategory::Keyword),
        "operator" => Some(HighlightCategory::Operator),
        "escape" | "string.escape" | "character.escape" => Some(HighlightCategory::Escape),
        _ if capture_name.starts_with("comment") => Some(HighlightCategory::Comment),
        _ if capture_name.starts_with("keyword") => Some(HighlightCategory::Keyword),
        _ if capture_name.starts_with("string") => Some(HighlightCategory::String),
        _ if capture_name.starts_with("escape") || capture_name.ends_with(".escape") => {
            Some(HighlightCategory::Escape)
        }
        _ if capture_name.starts_with("embedded") => Some(HighlightCategory::String),
        _ if capture_name.starts_with("type") => Some(HighlightCategory::Type),
        _ if capture_name.starts_with("constructor") => Some(HighlightCategory::Constructor),
        _ if capture_name.starts_with("attribute") => Some(HighlightCategory::Attribute),
        _ if capture_name.starts_with("function") || capture_name.starts_with("method") => {
            Some(HighlightCategory::Function)
        }
        _ if capture_name.starts_with("number")
            || capture_name == "float"
            || capture_name == "integer" =>
        {
            Some(HighlightCategory::Number)
        }
        _ if capture_name.starts_with("variable.parameter")
            || capture_name.starts_with("parameter") =>
        {
            Some(HighlightCategory::Parameter)
        }
        _ if capture_name.starts_with("field") => Some(HighlightCategory::Field),
        _ if capture_name.starts_with("property") => Some(HighlightCategory::Property),
        _ if capture_name.starts_with("label") => Some(HighlightCategory::Property),
        _ if capture_name.starts_with("module") || capture_name.starts_with("namespace") => {
            Some(HighlightCategory::Namespace)
        }
        _ if capture_name.starts_with("tag") => Some(HighlightCategory::Tag),
        _ if capture_name.starts_with("constant.builtin.boolean")
            || capture_name.starts_with("boolean") =>
        {
            Some(HighlightCategory::Boolean)
        }
        _ if capture_name.starts_with("constant") || capture_name == "enum_member" => {
            Some(HighlightCategory::Constant)
        }
        _ if capture_name.starts_with("operator") => Some(HighlightCategory::Operator),
        _ if capture_name.starts_with("punctuation") => Some(HighlightCategory::Punctuation),
        _ if capture_name.starts_with("lifetime") => Some(HighlightCategory::Lifetime),
        _ if capture_name.starts_with("variable") => Some(HighlightCategory::Variable),
        _ => None,
    }
}

pub(crate) fn normalize_spans(
    source: &str,
    spans: Vec<HighlightSpan>,
    byte_window: Option<Range<usize>>,
) -> Vec<HighlightSpan> {
    if source.is_empty() || spans.is_empty() {
        return Vec::new();
    }

    let (paint_start, paint_end) = if let Some(window) = byte_window {
        let Some((start, end)) = sanitize_byte_range(source, window) else {
            return Vec::new();
        };
        (start, end)
    } else {
        (0, source.len())
    };
    if paint_start >= paint_end {
        return Vec::new();
    }

    let mut painted: Vec<Option<(HighlightCategory, u8)>> = vec![None; paint_end - paint_start];

    for span in spans {
        let Some((raw_start, raw_end)) = sanitize_byte_range(source, span.range) else {
            continue;
        };
        let start = raw_start.max(paint_start);
        let end = raw_end.min(paint_end);
        if start >= end {
            continue;
        }

        let priority = span.category.priority();
        let local_start = start - paint_start;
        let local_end = end - paint_start;
        for slot in painted.iter_mut().take(local_end).skip(local_start) {
            match slot {
                Some((_, existing_priority)) if *existing_priority >= priority => {}
                _ => *slot = Some((span.category, priority)),
            }
        }
    }

    let mut merged: Vec<HighlightSpan> = Vec::new();
    let mut cursor = 0usize;

    while cursor < painted.len() {
        let Some((category, _)) = painted[cursor] else {
            cursor += 1;
            continue;
        };

        let start = cursor;
        let mut end = cursor + 1;
        while end < painted.len() {
            match painted[end] {
                Some((next_category, _)) if next_category == category => end += 1,
                _ => break,
            }
        }

        let Some((safe_start, safe_end)) =
            sanitize_byte_range(source, (paint_start + start)..(paint_start + end))
        else {
            cursor = end;
            continue;
        };

        if let Some(last) = merged.last_mut() {
            if last.category == category && last.range.end == safe_start {
                last.range.end = safe_end;
                cursor = end;
                continue;
            }
        }

        merged.push(HighlightSpan {
            range: safe_start..safe_end,
            category,
        });
        cursor = end;
    }

    merged
}

pub(crate) fn sanitize_byte_range(source: &str, range: Range<usize>) -> Option<(usize, usize)> {
    if source.is_empty() {
        return None;
    }

    let len = source.len();
    let mut start = range.start.min(len);
    let mut end = range.end.min(len);
    if start >= end {
        return None;
    }

    while start > 0 && !source.is_char_boundary(start) {
        start -= 1;
    }
    while end < len && !source.is_char_boundary(end) {
        end += 1;
    }

    (start < end).then_some((start, end))
}
