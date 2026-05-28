use std::collections::HashMap;
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
    if !tree_root_matches_source(root, source) {
        return Vec::new();
    }

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
            let Some(mut category) = capture_category(capture_name) else {
                continue;
            };

            if category == HighlightCategory::String && node.kind() == "template_string" {
                category = HighlightCategory::StringTemplate;
            }

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
    mut injection_cache: Option<&mut HashMap<LanguageId, SyntaxEngine>>,
) -> Vec<HighlightSpan> {
    if !tree_root_matches_source(root, source) {
        return Vec::new();
    }

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

    // Collect all injection content ranges first
    let mut injection_ranges = Vec::new();
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
            if !content.is_empty() {
                injection_ranges.push((node_start, node_end));
            }
        }
    }

    // Ensure parser exists in cache if we have one
    if let Some(cache) = injection_cache.as_mut() {
        if !cache.contains_key(&injected_lang) {
            if let Ok(eng) = SyntaxEngine::new(injected_lang) {
                cache.insert(injected_lang, eng);
            }
        }
    }

    // Process each injection range
    for (node_start, node_end) in injection_ranges {
        let content = &source[node_start..node_end];

        // Get spans from cached or temporary parser
        let inner = if let Some(cache) = injection_cache.as_mut() {
            // Use cached parser
            if let Some(engine) = cache.get_mut(&injected_lang) {
                if let Ok(tree) = engine.parse_source(content, 0) {
                    generate_query_highlight_spans_for_node(hl_query, tree.root_node(), content)
                } else {
                    continue;
                }
            } else {
                continue;
            }
        } else {
            // No cache: create temporary parser and immediately generate spans
            match SyntaxEngine::new(injected_lang) {
                Ok(mut eng) => match eng.parse_source(content, 0) {
                    Ok(tree) => {
                        generate_query_highlight_spans_for_node(hl_query, tree.root_node(), content)
                    }
                    Err(_) => continue,
                },
                Err(_) => continue,
            }
        };

        for mut span in inner {
            span.range = (span.range.start + node_start)..(span.range.end + node_start);
            spans.push(span);
        }
    }

    spans
}

pub(crate) fn generate_query_highlight_spans_for_node(
    query: &Query,
    root: Node<'_>,
    source: &str,
) -> Vec<HighlightSpan> {
    if !tree_root_matches_source(root, source) {
        return Vec::new();
    }

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
            let Some(mut category) = capture_category(capture_name) else {
                continue;
            };

            // Template literals in JavaScript/TypeScript are captured as
            // `@string` (priority 100) which masks nested interpolation
            // expressions (Variable priority 42, Function priority 85).
            // Override to StringTemplate (priority 30) so inner tokens win.
            if category == HighlightCategory::String
                && node.kind() == "template_string"
            {
                category = HighlightCategory::StringTemplate;
            }

            raw_spans.push(HighlightSpan {
                range: start..end,
                category,
            });
        }
    }

    normalize_spans(source, raw_spans, None)
}

fn tree_root_matches_source(root: Node<'_>, source: &str) -> bool {
    root.start_byte() <= source.len() && root.end_byte() <= source.len()
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
        "markup.strong" | "syntax.markup.strong" => Some(HighlightCategory::MarkupStrong),
        "markup.italic" | "syntax.markup.italic" => Some(HighlightCategory::MarkupItalic),
        "markup.raw.inline" | "syntax.markup.raw.inline" => Some(HighlightCategory::MarkupInlineCode),
        "markup.link.text" | "syntax.markup.link.text" => Some(HighlightCategory::MarkupLink),
        "syntax.keyword" => Some(HighlightCategory::Keyword),
        "syntax.keyword.control" | "keyword.control" | "keyword.control.return"
        | "keyword.control.conditional" | "keyword.control.repeat" | "keyword.control.import" => {
            Some(HighlightCategory::KeywordControl)
        }
        "syntax.keyword.storage" | "keyword.storage" | "keyword.storage.type"
        | "keyword.storage.modifier" => {
            Some(HighlightCategory::KeywordStorage)
        }
        "syntax.string.template" => Some(HighlightCategory::StringTemplate),
        "syntax.string" => Some(HighlightCategory::String),
        "syntax.string.escape" | "string.escape" | "character.escape" | "escape.sequence" => {
            Some(HighlightCategory::StringEscape)
        }
        "syntax.comment" => Some(HighlightCategory::Comment),
        "syntax.comment.doc" | "syntax.comment.documentation" | "comment.documentation" | "comment.doc" | "comment.block.documentation" => {
            Some(HighlightCategory::CommentDoc)
        }
        "syntax.type" => Some(HighlightCategory::Type),
        "syntax.type.builtin" | "type.builtin" | "builtin.type" => Some(HighlightCategory::TypeBuiltin),
        "syntax.function" => Some(HighlightCategory::Function),
        "syntax.function.builtin" | "function.builtin" | "function.method.builtin" => Some(HighlightCategory::FunctionBuiltin),
        "syntax.number" => Some(HighlightCategory::Number),
        "syntax.boolean" => Some(HighlightCategory::Boolean),
        "syntax.identifier" => Some(HighlightCategory::Identifier),
        "syntax.variable" => Some(HighlightCategory::Variable),
        "syntax.variable.builtin" | "variable.builtin" | "variable.language" => Some(HighlightCategory::VariableBuiltin),
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
        "constant.builtin.boolean" | "boolean" => Some(HighlightCategory::Boolean),
        "constant.builtin" => Some(HighlightCategory::Constant),
        "function.method" | "method.call" => Some(HighlightCategory::Function),
        "module" | "namespace" => Some(HighlightCategory::Namespace),
        "tag" | "tag.builtin" => Some(HighlightCategory::Tag),
        "label" => Some(HighlightCategory::Property),
        "identifier" => Some(HighlightCategory::Identifier),
        "variable" => Some(HighlightCategory::Variable),
        "operator" => Some(HighlightCategory::Operator),
        "escape" => Some(HighlightCategory::Escape),
        _ if capture_name.starts_with("comment.doc") => Some(HighlightCategory::CommentDoc),
        _ if capture_name.starts_with("comment") => Some(HighlightCategory::Comment),
        _ if capture_name.starts_with("keyword.control") => Some(HighlightCategory::KeywordControl),
        _ if capture_name.starts_with("keyword.storage") => Some(HighlightCategory::KeywordStorage),
        _ if capture_name.starts_with("keyword") => Some(HighlightCategory::Keyword),
        _ if capture_name.starts_with("string") => Some(HighlightCategory::String),
        _ if capture_name.starts_with("escape") || capture_name.ends_with(".escape") => {
            Some(HighlightCategory::StringEscape)
        }
        _ if capture_name.starts_with("embedded") => Some(HighlightCategory::StringTemplate),
        _ if capture_name.starts_with("type.builtin") => Some(HighlightCategory::TypeBuiltin),
        _ if capture_name.starts_with("type") => Some(HighlightCategory::Type),
        _ if capture_name.starts_with("constructor") => Some(HighlightCategory::Constructor),
        _ if capture_name.starts_with("attribute") => Some(HighlightCategory::Attribute),
        _ if capture_name.starts_with("function.builtin") => Some(HighlightCategory::FunctionBuiltin),
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

#[derive(Debug, Clone, Copy)]
struct SpanRun {
    start: usize,
    end: usize,
    category: HighlightCategory,
    priority: u8,
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

    let mut runs: Vec<SpanRun> = Vec::with_capacity(spans.len());
    let mut boundaries: Vec<usize> = Vec::with_capacity(spans.len().saturating_mul(2).saturating_add(2));
    boundaries.push(paint_start);
    boundaries.push(paint_end);

    for span in spans {
        let Some((raw_start, raw_end)) = sanitize_byte_range(source, span.range) else {
            continue;
        };
        let start = raw_start.max(paint_start);
        let end = raw_end.min(paint_end);
        if start >= end {
            continue;
        }

        runs.push(SpanRun {
            start,
            end,
            category: span.category,
            priority: span.category.priority(),
        });
        boundaries.push(start);
        boundaries.push(end);
    }

    if runs.is_empty() {
        return Vec::new();
    }

    boundaries.sort_unstable();
    boundaries.dedup();

    let mut merged: Vec<HighlightSpan> = Vec::with_capacity(runs.len());
    for window in boundaries.windows(2) {
        let [start, end] = window else {
            continue;
        };
        if start >= end {
            continue;
        }

        let Some(best) = runs
            .iter()
            .filter(|run| run.start <= *start && run.end >= *end)
            .max_by_key(|run| run.priority)
        else {
            continue;
        };

        if let Some(last) = merged.last_mut() {
            if last.category == best.category && last.range.end == *start {
                last.range.end = *end;
                continue;
            }
        }

        merged.push(HighlightSpan {
            range: *start..*end,
            category: best.category,
        });
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
