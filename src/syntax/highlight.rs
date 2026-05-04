use std::{ops::Range, sync::OnceLock};

use streaming_iterator::StreamingIterator;
use tree_sitter::{Node, Query, QueryCursor};

use crate::config::theme_config::ThemeConfig;
use crate::syntax::{
    parser::{language_id_for_extension, tree_sitter_language},
    syntax_engine::{LanguageId, SyntaxEngine, SyntaxTreeState},
};

pub const INLINE_TREE_SITTER_BYTE_THRESHOLD: usize = 128 * 1024;
pub const INLINE_TREE_SITTER_LINE_THRESHOLD: usize = 1_500;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HighlightCategory {
    Keyword,
    String,
    Comment,
    Type,
    Function,
    Number,
    Boolean,
    Identifier,
    Variable,
    Parameter,
    Field,
    Property,
    Constant,
    Operator,
    Punctuation,
    Escape,
    Macro,
    Lifetime,
    Constructor,
    Attribute,
    Namespace,
    Tag,
}

impl HighlightCategory {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Keyword => "keyword",
            Self::String => "string",
            Self::Comment => "comment",
            Self::Type => "type",
            Self::Function => "function",
            Self::Number => "number",
            Self::Boolean => "boolean",
            Self::Identifier => "identifier",
            Self::Variable => "variable",
            Self::Parameter => "parameter",
            Self::Field => "field",
            Self::Property => "property",
            Self::Constant => "constant",
            Self::Operator => "operator",
            Self::Punctuation => "punctuation",
            Self::Escape => "escape",
            Self::Macro => "macro",
            Self::Lifetime => "lifetime",
            Self::Constructor => "constructor",
            Self::Attribute => "attribute",
            Self::Namespace => "namespace",
            Self::Tag => "tag",
        }
    }

    pub fn is_bold(self) -> bool {
        matches!(self, Self::Macro)
    }

    pub fn is_italic(self) -> bool {
        matches!(self, Self::Comment)
    }

    fn priority(self) -> u8 {
        match self {
            // Narrow but expressive captures should win over the generic fallback.
            Self::Comment => 120,
            Self::Escape => 115,
            Self::Macro => 110,
            Self::String => 100,
            Self::Lifetime => 95,
            Self::Attribute => 93,
            Self::Keyword => 90,
            Self::Boolean => 88,
            Self::Function => 85,
            Self::Constructor => 84,
            Self::Constant => 83,
            Self::Parameter => 80,
            Self::Field => 78,
            Self::Property => 76,
            Self::Namespace => 74,
            Self::Tag => 73,
            Self::Type => 72,
            Self::Number => 68,
            Self::Variable => 42,
            Self::Identifier => 40,
            Self::Operator => 20,
            Self::Punctuation => 10,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HighlightSpan {
    pub range: Range<usize>,
    pub category: HighlightCategory,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HighlightEdit {
    pub start: usize,
    pub old_end: usize,
    pub new_end: usize,
}

impl HighlightEdit {
    pub fn insert(start: usize, inserted_len: usize) -> Self {
        Self {
            start,
            old_end: start,
            new_end: start.saturating_add(inserted_len),
        }
    }

    pub fn delete(start: usize, old_end: usize) -> Self {
        Self {
            start,
            old_end,
            new_end: start,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct HighlightPalette {
    pub keyword: [u8; 4],
    pub string: [u8; 4],
    pub comment: [u8; 4],
    pub ty: [u8; 4],
    pub function: [u8; 4],
    pub number: [u8; 4],
    pub boolean: [u8; 4],
    pub identifier: [u8; 4],
    pub variable: [u8; 4],
    pub parameter: [u8; 4],
    pub field: [u8; 4],
    pub property: [u8; 4],
    pub constant: [u8; 4],
    pub operator: [u8; 4],
    pub punctuation: [u8; 4],
    pub escape: [u8; 4],
    pub macro_name: [u8; 4],
    pub lifetime: [u8; 4],
    pub constructor: [u8; 4],
    pub attribute: [u8; 4],
    pub namespace: [u8; 4],
    pub tag: [u8; 4],
}

impl Default for HighlightPalette {
    fn default() -> Self {
        Self {
            keyword: [234, 205, 97, 255],
            string: [60, 236, 133, 255],
            comment: [74, 94, 132, 255],
            ty: [183, 138, 255, 255],
            function: [105, 195, 255, 255],
            number: [227, 85, 53, 255],
            boolean: [255, 149, 92, 255],
            identifier: [208, 215, 228, 255],
            variable: [208, 215, 228, 255],
            parameter: [34, 236, 219, 255],
            field: [105, 195, 255, 255],
            property: [208, 215, 228, 255],
            constant: [255, 149, 92, 255],
            operator: [175, 187, 210, 255],
            punctuation: [129, 150, 181, 255],
            escape: [255, 149, 92, 255],
            macro_name: [105, 195, 255, 255],
            lifetime: [255, 149, 92, 255],
            constructor: [183, 138, 255, 255],
            attribute: [234, 205, 97, 255],
            namespace: [183, 138, 255, 255],
            tag: [183, 138, 255, 255],
        }
    }
}

impl HighlightPalette {
    pub fn color_for(self, category: HighlightCategory) -> [u8; 4] {
        match category {
            HighlightCategory::Keyword => self.keyword,
            HighlightCategory::String => self.string,
            HighlightCategory::Comment => self.comment,
            HighlightCategory::Type => self.ty,
            HighlightCategory::Function => self.function,
            HighlightCategory::Number => self.number,
            HighlightCategory::Boolean => self.boolean,
            HighlightCategory::Identifier => self.identifier,
            HighlightCategory::Variable => self.variable,
            HighlightCategory::Parameter => self.parameter,
            HighlightCategory::Field => self.field,
            HighlightCategory::Property => self.property,
            HighlightCategory::Constant => self.constant,
            HighlightCategory::Operator => self.operator,
            HighlightCategory::Punctuation => self.punctuation,
            HighlightCategory::Escape => self.escape,
            HighlightCategory::Macro => self.macro_name,
            HighlightCategory::Lifetime => self.lifetime,
            HighlightCategory::Constructor => self.constructor,
            HighlightCategory::Attribute => self.attribute,
            HighlightCategory::Namespace => self.namespace,
            HighlightCategory::Tag => self.tag,
        }
    }
}

pub fn apply_highlight_edits(spans: &mut Vec<HighlightSpan>, edits: &[HighlightEdit]) {
    if spans.is_empty() || edits.is_empty() {
        return;
    }

    let mut adjusted = std::mem::take(spans);
    for edit in edits {
        adjusted = adjusted
            .into_iter()
            .filter_map(|span| transform_span_by_edit(span, *edit))
            .collect();
    }

    *spans = coalesce_spans(adjusted);
}

pub fn merge_highlight_spans(
    spans: &mut Vec<HighlightSpan>,
    replacement: Vec<HighlightSpan>,
    covered_byte_range: Option<Range<usize>>,
) {
    let Some(window) = covered_byte_range else {
        *spans = coalesce_spans(replacement);
        return;
    };

    let mut merged = Vec::with_capacity(spans.len() + replacement.len() + 2);
    for span in std::mem::take(spans) {
        if span.range.end <= window.start || span.range.start >= window.end {
            merged.push(span);
            continue;
        }

        if span.range.start < window.start {
            merged.push(HighlightSpan {
                range: span.range.start..window.start,
                category: span.category,
            });
        }
        if span.range.end > window.end {
            merged.push(HighlightSpan {
                range: window.end..span.range.end,
                category: span.category,
            });
        }
    }

    merged.extend(replacement);
    *spans = coalesce_spans(merged);
}

pub fn overlay_highlight_layers(
    base: &[HighlightSpan],
    overrides: &[HighlightSpan],
) -> Vec<HighlightSpan> {
    if overrides.is_empty() {
        return base.to_vec();
    }

    let mut merged = coalesce_spans(base.to_vec());
    for span in overrides.iter().cloned() {
        let window = span.range.clone();
        merge_highlight_spans(&mut merged, vec![span], Some(window));
    }
    merged
}

pub fn generate_highlight_spans(tree_state: &SyntaxTreeState, source: &str) -> Vec<HighlightSpan> {
    generate_query_highlight_spans(
        tree_state.language_id(),
        tree_state.root_node(),
        source,
        None,
    )
}

pub fn generate_highlight_spans_in_byte_window(
    tree_state: &SyntaxTreeState,
    source: &str,
    window: Range<usize>,
) -> Vec<HighlightSpan> {
    let Some((start, end)) = sanitize_byte_range(source, window) else {
        return Vec::new();
    };
    generate_query_highlight_spans(
        tree_state.language_id(),
        tree_state.root_node(),
        source,
        Some(start..end),
    )
}

pub fn should_highlight_inline(text: &str) -> bool {
    if text.len() > INLINE_TREE_SITTER_BYTE_THRESHOLD {
        return false;
    }

    let line_count = text.bytes().filter(|byte| *byte == b'\n').count() + 1;
    line_count <= INLINE_TREE_SITTER_LINE_THRESHOLD
}

pub fn highlight_snippet(text: &str, extension: &str, _theme: &ThemeConfig) -> Vec<HighlightSpan> {
    if text.is_empty() || !should_highlight_inline(text) {
        return Vec::new();
    }
    let Some(language_id) = language_id_for_extension(extension) else {
        return Vec::new();
    };
    let Ok(mut engine) = SyntaxEngine::new(language_id) else {
        return Vec::new();
    };
    let Ok(tree_state) = engine.parse_source(text, 0) else {
        return Vec::new();
    };
    generate_highlight_spans(tree_state, text)
}

fn generate_query_highlight_spans(
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

fn highlight_query(language_id: LanguageId) -> Option<&'static Query> {
    match language_id {
        LanguageId::Rust => Some(rust_highlight_query()),
        LanguageId::JavaScript => Some(javascript_highlight_query()),
        LanguageId::Jsx => Some(jsx_highlight_query()),
        LanguageId::TypeScript => Some(typescript_highlight_query()),
        LanguageId::Tsx => Some(tsx_highlight_query()),
        LanguageId::Go => Some(go_highlight_query()),
        LanguageId::Sql => Some(sql_highlight_query()),
        LanguageId::Yaml => Some(yaml_highlight_query()),
        LanguageId::Dockerfile => Some(dockerfile_highlight_query()),
        LanguageId::Json => Some(json_highlight_query()),
        LanguageId::Bash => Some(bash_highlight_query()),
        LanguageId::Markdown => Some(markdown_highlight_query()),
    }
}

fn rust_highlight_query() -> &'static Query {
    static QUERY: OnceLock<Query> = OnceLock::new();
    QUERY.get_or_init(|| {
        build_highlight_query(
            LanguageId::Rust,
            include_str!("queries/rust/highlights.scm"),
            "rust",
        )
    })
}

fn javascript_highlight_query() -> &'static Query {
    static QUERY: OnceLock<Query> = OnceLock::new();
    QUERY.get_or_init(|| {
        build_highlight_query(
            LanguageId::JavaScript,
            tree_sitter_javascript::HIGHLIGHT_QUERY,
            "javascript",
        )
    })
}

fn jsx_highlight_query() -> &'static Query {
    static QUERY: OnceLock<Query> = OnceLock::new();
    QUERY.get_or_init(|| {
        let source = format!(
            "{}\n{}",
            tree_sitter_javascript::HIGHLIGHT_QUERY,
            tree_sitter_javascript::JSX_HIGHLIGHT_QUERY
        );
        build_highlight_query(LanguageId::Jsx, &source, "jsx")
    })
}

fn typescript_highlight_query() -> &'static Query {
    static QUERY: OnceLock<Query> = OnceLock::new();
    QUERY.get_or_init(|| {
        let source = format!(
            "{}\n{}",
            tree_sitter_javascript::HIGHLIGHT_QUERY,
            tree_sitter_typescript::HIGHLIGHTS_QUERY,
        );
        build_highlight_query(LanguageId::TypeScript, &source, "typescript")
    })
}

fn tsx_highlight_query() -> &'static Query {
    static QUERY: OnceLock<Query> = OnceLock::new();
    QUERY.get_or_init(|| {
        let source = format!(
            "{}\n{}\n{}",
            tree_sitter_javascript::HIGHLIGHT_QUERY,
            tree_sitter_javascript::JSX_HIGHLIGHT_QUERY,
            tree_sitter_typescript::HIGHLIGHTS_QUERY,
        );
        build_highlight_query(LanguageId::Tsx, &source, "tsx")
    })
}

fn go_highlight_query() -> &'static Query {
    static QUERY: OnceLock<Query> = OnceLock::new();
    QUERY.get_or_init(|| {
        build_highlight_query(LanguageId::Go, tree_sitter_go::HIGHLIGHTS_QUERY, "go")
    })
}

fn sql_highlight_query() -> &'static Query {
    static QUERY: OnceLock<Query> = OnceLock::new();
    QUERY.get_or_init(|| {
        build_highlight_query(
            LanguageId::Sql,
            include_str!("queries/sql/highlights.scm"),
            "sql",
        )
    })
}

fn yaml_highlight_query() -> &'static Query {
    static QUERY: OnceLock<Query> = OnceLock::new();
    QUERY.get_or_init(|| {
        build_highlight_query(LanguageId::Yaml, tree_sitter_yaml::HIGHLIGHTS_QUERY, "yaml")
    })
}

fn json_highlight_query() -> &'static Query {
    static QUERY: OnceLock<Query> = OnceLock::new();
    QUERY.get_or_init(|| {
        build_highlight_query(LanguageId::Json, tree_sitter_json::HIGHLIGHTS_QUERY, "json")
    })
}

fn bash_highlight_query() -> &'static Query {
    static QUERY: OnceLock<Query> = OnceLock::new();
    QUERY.get_or_init(|| {
        build_highlight_query(LanguageId::Bash, tree_sitter_bash::HIGHLIGHT_QUERY, "bash")
    })
}

fn markdown_highlight_query() -> &'static Query {
    static QUERY: OnceLock<Query> = OnceLock::new();
    QUERY.get_or_init(|| {
        build_highlight_query(
            LanguageId::Markdown,
            include_str!("queries/markdown/highlights.scm"),
            "markdown",
        )
    })
}

fn dockerfile_highlight_query() -> &'static Query {
    static QUERY: OnceLock<Query> = OnceLock::new();
    QUERY.get_or_init(|| {
        build_highlight_query(
            LanguageId::Dockerfile,
            include_str!("queries/dockerfile/highlights.scm"),
            "dockerfile",
        )
    })
}

fn build_highlight_query(language_id: LanguageId, source: &str, label: &str) -> Query {
    let language = tree_sitter_language(language_id).unwrap_or_else(|| {
        panic!(
            "tree-sitter language '{}' missing while building {label} highlight query",
            language_id.as_str()
        )
    });
    Query::new(&language, source)
        .unwrap_or_else(|err| panic!("invalid {label} highlight query: {err}"))
}

fn capture_category(capture_name: &str) -> Option<HighlightCategory> {
    match capture_name {
        "string.special.key" => Some(HighlightCategory::Property),
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
        "syntax.punctuation" | "punctuation.bracket" | "punctuation.delimiter" | "punctuation.special" => Some(HighlightCategory::Punctuation),
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
        "variable" | "variable.builtin" => Some(HighlightCategory::Variable),
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

fn normalize_spans(
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

fn transform_span_by_edit(span: HighlightSpan, edit: HighlightEdit) -> Option<HighlightSpan> {
    let start = span.range.start;
    let end = span.range.end;
    if start >= end {
        return None;
    }

    if edit.old_end == edit.start {
        let inserted_len = edit.new_end.saturating_sub(edit.start);
        if inserted_len == 0 {
            return Some(span);
        }

        if end <= edit.start {
            return Some(span);
        }

        if start > edit.start {
            return Some(HighlightSpan {
                range: (start + inserted_len)..(end + inserted_len),
                category: span.category,
            });
        }

        return Some(HighlightSpan {
            range: start..(end + inserted_len),
            category: span.category,
        });
    }

    if end <= edit.start {
        return Some(span);
    }

    let removed_len = edit.old_end.saturating_sub(edit.start);
    if removed_len == 0 {
        return Some(span);
    }

    if start >= edit.old_end {
        return Some(HighlightSpan {
            range: (start - removed_len)..(end - removed_len),
            category: span.category,
        });
    }

    let new_start = if start >= edit.start {
        edit.start
    } else {
        start
    };
    let new_end = if end <= edit.old_end {
        edit.start
    } else {
        end - removed_len
    };

    (new_start < new_end).then_some(HighlightSpan {
        range: new_start..new_end,
        category: span.category,
    })
}

pub fn expand_merge_window(
    existing: &[HighlightSpan],
    replacement: &[HighlightSpan],
    mut window: Range<usize>,
) -> Range<usize> {
    for span in existing {
        if span.range.end > window.start && span.range.start < window.end {
            window.start = window.start.min(span.range.start);
            window.end = window.end.max(span.range.end);
        }
    }

    for span in replacement {
        if span.range.end > window.start && span.range.start < window.end {
            window.start = window.start.min(span.range.start);
            window.end = window.end.max(span.range.end);
        }
    }

    window
}

fn coalesce_spans(mut spans: Vec<HighlightSpan>) -> Vec<HighlightSpan> {
    spans.retain(|span| span.range.start < span.range.end);
    spans.sort_by_key(|span| (span.range.start, span.range.end));

    let mut merged: Vec<HighlightSpan> = Vec::with_capacity(spans.len());
    for mut span in spans {
        if let Some(last) = merged.last_mut() {
            if span.range.start < last.range.end {
                if span.category == last.category {
                    last.range.end = last.range.end.max(span.range.end);
                    continue;
                }

                span.range.start = last.range.end;
                if span.range.start >= span.range.end {
                    continue;
                }
            }

            if last.category == span.category && last.range.end == span.range.start {
                last.range.end = span.range.end;
                continue;
            }
        }

        merged.push(span);
    }

    merged
}

fn sanitize_byte_range(source: &str, range: Range<usize>) -> Option<(usize, usize)> {
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

#[cfg(test)]
mod tests {
    use super::{
        HighlightCategory, HighlightEdit, HighlightSpan, apply_highlight_edits,
        generate_highlight_spans, merge_highlight_spans, overlay_highlight_layers,
        should_highlight_inline,
    };
    use crate::syntax::syntax_engine::{LanguageId, SyntaxEngine};

    #[test]
    fn rust_highlight_generates_core_categories() {
        let source = r#"
use std::fmt;

fn greet(name: &str) -> String {
    // note: demo
    let value = 42;
    let text = "hello";
    value
}
"#;

        let mut engine = SyntaxEngine::new_rust().expect("init parser");
        let tree = engine.parse_source(source, 10).expect("parse");
        let spans = generate_highlight_spans(tree, source);

        assert!(!spans.is_empty(), "expected at least one highlight span");
        assert!(
            spans
                .iter()
                .any(|s| s.category == HighlightCategory::Keyword)
        );
        assert!(
            spans
                .iter()
                .any(|s| s.category == HighlightCategory::Function)
        );
        assert!(spans.iter().any(|s| s.category == HighlightCategory::Type));
        assert!(
            spans
                .iter()
                .any(|s| s.category == HighlightCategory::Number)
        );
        assert!(
            spans
                .iter()
                .any(|s| s.category == HighlightCategory::String)
        );
        assert!(
            spans
                .iter()
                .any(|s| s.category == HighlightCategory::Comment)
        );
    }

    #[test]
    fn rust_highlight_generates_extended_categories() {
        let source = r#"
const MAX_SIZE: usize = 32;

struct Demo<'a> {
    label: &'a str,
}

fn render<'a>(label: &'a str, count: usize) -> Demo<'a> {
    println!("{}", label);
    let next = count + MAX_SIZE;
    Demo { label }
}
"#;

        let mut engine = SyntaxEngine::new_rust().expect("init parser");
        let tree = engine.parse_source(source, 11).expect("parse");
        let spans = generate_highlight_spans(tree, source);

        assert!(
            spans
                .iter()
                .any(|s| s.category == HighlightCategory::Identifier)
        );
        assert!(
            spans
                .iter()
                .any(|s| s.category == HighlightCategory::Parameter)
        );
        assert!(spans.iter().any(|s| s.category == HighlightCategory::Field));
        assert!(
            spans
                .iter()
                .any(|s| s.category == HighlightCategory::Property)
        );
        assert!(
            spans
                .iter()
                .any(|s| s.category == HighlightCategory::Constant)
        );
        assert!(
            spans
                .iter()
                .any(|s| s.category == HighlightCategory::Operator)
        );
        assert!(spans.iter().any(|s| s.category == HighlightCategory::Macro));
        assert!(
            spans
                .iter()
                .any(|s| s.category == HighlightCategory::Lifetime)
        );
    }

    #[test]
    fn javascript_highlight_uses_default_query_captures() {
        let source = r#"
const answer = 42;
function greet(name) {
    console.log(`hello ${name}`);
    return answer;
}
"#;

        let mut engine = SyntaxEngine::new(LanguageId::JavaScript).expect("init js parser");
        let tree = engine.parse_source(source, 12).expect("parse js");
        let spans = generate_highlight_spans(tree, source);

        assert!(!spans.is_empty(), "expected js highlight spans");
        assert!(
            spans
                .iter()
                .any(|s| s.category == HighlightCategory::Keyword)
        );
        assert!(
            spans
                .iter()
                .any(|s| s.category == HighlightCategory::Function)
        );
        assert!(
            spans
                .iter()
                .any(|s| s.category == HighlightCategory::Number)
        );
        assert!(
            spans
                .iter()
                .any(|s| s.category == HighlightCategory::String)
        );
    }

    #[test]
    fn go_highlight_uses_default_query_captures() {
        let source = r#"
package main

import "fmt"

func greet(name string) string {
    fmt.Println(name);
    return name
}
"#;

        let mut engine = SyntaxEngine::new(LanguageId::Go).expect("init go parser");
        let tree = engine.parse_source(source, 13).expect("parse go");
        let spans = generate_highlight_spans(tree, source);

        assert!(!spans.is_empty(), "expected go highlight spans");
        assert!(
            spans
                .iter()
                .any(|s| s.category == HighlightCategory::Keyword)
        );
        assert!(
            spans
                .iter()
                .any(|s| s.category == HighlightCategory::Function)
        );
        assert!(
            spans
                .iter()
                .any(|s| s.category == HighlightCategory::String)
        );
    }

    #[test]
    fn sql_highlight_maps_keywords_functions_and_schema_tokens() {
        let source = r#"
CREATE TABLE users (
    id bigint,
    email text
);

SELECT COUNT(id), MAX(id), users.email
FROM users
WHERE id = 42 AND email = 'hi@example.com';
"#;

        let mut engine = SyntaxEngine::new(LanguageId::Sql).expect("init sql parser");
        let tree = engine.parse_source(source, 14).expect("parse sql");
        let spans = generate_highlight_spans(tree, source);

        assert!(!spans.is_empty(), "expected sql highlight spans");
        assert!(
            spans
                .iter()
                .any(|s| s.category == HighlightCategory::Keyword)
        );
        assert!(
            spans
                .iter()
                .any(|s| s.category == HighlightCategory::Function)
        );
        assert!(spans.iter().any(|s| s.category == HighlightCategory::Type));
        assert!(
            spans
                .iter()
                .any(|s| s.category == HighlightCategory::Property)
        );
        assert!(
            spans
                .iter()
                .any(|s| s.category == HighlightCategory::Number)
        );
        assert!(
            spans
                .iter()
                .any(|s| s.category == HighlightCategory::String)
        );
    }

    #[test]
    fn syntax_categories_expose_emphasis_for_comment_and_macro() {
        assert!(HighlightCategory::Comment.is_italic());
        assert!(!HighlightCategory::Comment.is_bold());
        assert!(HighlightCategory::Macro.is_bold());
        assert!(!HighlightCategory::Macro.is_italic());
    }

    #[test]
    fn normalized_spans_do_not_overlap() {
        let source = "fn main() { let x = 1; }";
        let mut engine = SyntaxEngine::new_rust().expect("init parser");
        let tree = engine.parse_source(source, 1).expect("parse");
        let spans = generate_highlight_spans(tree, source);

        for window in spans.windows(2) {
            let left = &window[0];
            let right = &window[1];
            assert!(left.range.end <= right.range.start);
        }
    }

    #[test]
    fn highlight_edits_shift_existing_ranges_without_clearing() {
        let mut spans = vec![
            HighlightSpan {
                range: 10..20,
                category: HighlightCategory::Keyword,
            },
            HighlightSpan {
                range: 25..30,
                category: HighlightCategory::String,
            },
        ];

        apply_highlight_edits(&mut spans, &[HighlightEdit::insert(15, 3)]);
        assert_eq!(spans[0].range, 10..23);
        assert_eq!(spans[1].range, 28..33);

        apply_highlight_edits(&mut spans, &[HighlightEdit::delete(12, 18)]);
        assert_eq!(spans[0].range, 10..17);
        assert_eq!(spans[1].range, 22..27);
    }

    #[test]
    fn merge_highlight_window_only_replaces_overlapping_slice() {
        let mut spans = vec![HighlightSpan {
            range: 0..10,
            category: HighlightCategory::Comment,
        }];

        merge_highlight_spans(
            &mut spans,
            vec![HighlightSpan {
                range: 4..6,
                category: HighlightCategory::Keyword,
            }],
            Some(4..6),
        );

        assert_eq!(
            spans,
            vec![
                HighlightSpan {
                    range: 0..4,
                    category: HighlightCategory::Comment,
                },
                HighlightSpan {
                    range: 4..6,
                    category: HighlightCategory::Keyword,
                },
                HighlightSpan {
                    range: 6..10,
                    category: HighlightCategory::Comment,
                },
            ]
        );
    }

    #[test]
    fn expand_merge_window_absorbs_intersecting_existing_and_replacement_spans() {
        let existing = vec![HighlightSpan {
            range: 0..10,
            category: HighlightCategory::Comment,
        }];
        let replacement = vec![HighlightSpan {
            range: 4..6,
            category: HighlightCategory::Keyword,
        }];

        assert_eq!(
            super::expand_merge_window(&existing, &replacement, 5..6),
            0..10
        );
    }

    #[test]
    fn semantic_overrides_replace_tree_sitter_in_same_range() {
        let base = vec![
            HighlightSpan {
                range: 0..4,
                category: HighlightCategory::Identifier,
            },
            HighlightSpan {
                range: 4..8,
                category: HighlightCategory::Field,
            },
        ];
        let overrides = vec![HighlightSpan {
            range: 2..6,
            category: HighlightCategory::Constant,
        }];

        let merged = overlay_highlight_layers(&base, &overrides);

        assert_eq!(
            merged,
            vec![
                HighlightSpan {
                    range: 0..2,
                    category: HighlightCategory::Identifier,
                },
                HighlightSpan {
                    range: 2..6,
                    category: HighlightCategory::Constant,
                },
                HighlightSpan {
                    range: 6..8,
                    category: HighlightCategory::Field,
                },
            ]
        );
    }

    #[test]
    fn inline_highlight_threshold_accepts_small_buffers() {
        assert!(should_highlight_inline("fn main() {}\n"));
        let large = "x".repeat(super::INLINE_TREE_SITTER_BYTE_THRESHOLD + 1);
        assert!(!should_highlight_inline(&large));
    }

    #[test]
    fn capture_category_maps_high_value_builtin_and_structure_tokens() {
        assert_eq!(
            super::capture_category("function.builtin"),
            Some(HighlightCategory::Function)
        );
        assert_eq!(
            super::capture_category("constant.builtin.boolean"),
            Some(HighlightCategory::Boolean)
        );
        assert_eq!(
            super::capture_category("module"),
            Some(HighlightCategory::Namespace)
        );
        assert_eq!(
            super::capture_category("label"),
            Some(HighlightCategory::Property)
        );
    }
}
