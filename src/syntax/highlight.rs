use std::ops::Range;

use tree_sitter::Node;

use crate::syntax::syntax_engine::{LanguageId, SyntaxTreeState};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HighlightCategory {
    Keyword,
    String,
    Comment,
    Type,
    Function,
    Number,
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
        }
    }

    fn priority(self) -> u8 {
        match self {
            // Ưu tiên cao hơn để span hẹp nhưng quan trọng không bị đè.
            Self::Comment => 100,
            Self::String => 90,
            Self::Keyword => 80,
            Self::Function => 70,
            Self::Type => 60,
            Self::Number => 50,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HighlightSpan {
    pub range: Range<usize>,
    pub category: HighlightCategory,
}

#[derive(Debug, Clone, Copy)]
pub struct HighlightPalette {
    pub keyword: [u8; 4],
    pub string: [u8; 4],
    pub comment: [u8; 4],
    pub ty: [u8; 4],
    pub function: [u8; 4],
    pub number: [u8; 4],
}

impl Default for HighlightPalette {
    fn default() -> Self {
        // Palette tối ưu cho nền tối đang dùng ở probe hiện tại.
        Self {
            keyword: [214, 153, 255, 255],
            string: [153, 214, 255, 255],
            comment: [120, 130, 146, 255],
            ty: [255, 198, 128, 255],
            function: [166, 232, 189, 255],
            number: [255, 177, 177, 255],
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
        }
    }
}

pub fn generate_highlight_spans(tree_state: &SyntaxTreeState, source: &str) -> Vec<HighlightSpan> {
    match tree_state.language_id() {
        LanguageId::Rust => generate_rust_highlight_spans(tree_state.root_node(), source),
    }
}

fn generate_rust_highlight_spans(root: Node<'_>, source: &str) -> Vec<HighlightSpan> {
    let mut raw_spans = Vec::new();
    let mut stack = vec![root];

    while let Some(node) = stack.pop() {
        if let Some(category) = classify_rust_node(node) {
            raw_spans.push(HighlightSpan {
                range: node.start_byte()..node.end_byte(),
                category,
            });
        }

        let mut cursor = node.walk();
        if cursor.goto_first_child() {
            let mut children = Vec::new();
            loop {
                children.push(cursor.node());
                if !cursor.goto_next_sibling() {
                    break;
                }
            }

            // Giữ thứ tự trái -> phải ổn định khi duyệt DFS bằng stack.
            for child in children.into_iter().rev() {
                stack.push(child);
            }
        }
    }

    normalize_spans(source, raw_spans)
}

fn classify_rust_node(node: Node<'_>) -> Option<HighlightCategory> {
    let kind = node.kind();

    if matches!(kind, "line_comment" | "block_comment") {
        return Some(HighlightCategory::Comment);
    }
    if matches!(
        kind,
        "string_literal" | "raw_string_literal" | "char_literal"
    ) {
        return Some(HighlightCategory::String);
    }
    if matches!(kind, "integer_literal" | "float_literal") {
        return Some(HighlightCategory::Number);
    }
    if matches!(
        kind,
        "primitive_type"
            | "type_identifier"
            | "scoped_type_identifier"
            | "generic_type"
            | "bounded_type"
    ) {
        return Some(HighlightCategory::Type);
    }
    if is_rust_keyword_token(kind) {
        return Some(HighlightCategory::Keyword);
    }

    if kind == "identifier" && is_function_identifier(node) {
        return Some(HighlightCategory::Function);
    }
    if kind == "field_identifier" && is_method_identifier(node) {
        return Some(HighlightCategory::Function);
    }

    None
}

fn is_rust_keyword_token(kind: &str) -> bool {
    matches!(
        kind,
        "fn" | "let"
            | "mut"
            | "pub"
            | "use"
            | "struct"
            | "enum"
            | "impl"
            | "trait"
            | "mod"
            | "match"
            | "if"
            | "else"
            | "for"
            | "while"
            | "loop"
            | "return"
            | "break"
            | "continue"
            | "async"
            | "await"
            | "const"
            | "static"
            | "where"
            | "in"
            | "as"
            | "crate"
            | "super"
            | "self"
            | "Self"
    )
}

fn is_function_identifier(node: Node<'_>) -> bool {
    is_child_field(node, "function_item", "name")
        || is_child_field(node, "function_signature_item", "name")
        || is_child_field(node, "call_expression", "function")
}

fn is_method_identifier(node: Node<'_>) -> bool {
    is_child_field(node, "method_call_expression", "method")
}

fn is_child_field(node: Node<'_>, parent_kind: &str, field_name: &str) -> bool {
    let Some(parent) = node.parent() else {
        return false;
    };
    if parent.kind() != parent_kind {
        return false;
    }

    parent
        .child_by_field_name(field_name)
        .is_some_and(|field_node| field_node.id() == node.id())
}

fn normalize_spans(source: &str, spans: Vec<HighlightSpan>) -> Vec<HighlightSpan> {
    if source.is_empty() || spans.is_empty() {
        return Vec::new();
    }

    let mut painted: Vec<Option<(HighlightCategory, u8)>> = vec![None; source.len()];

    for span in spans {
        let Some((start, end)) = sanitize_byte_range(source, span.range) else {
            continue;
        };
        let priority = span.category.priority();

        for slot in painted.iter_mut().take(end).skip(start) {
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

        let Some((safe_start, safe_end)) = sanitize_byte_range(source, start..end) else {
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
    use super::{HighlightCategory, generate_highlight_spans};
    use crate::syntax::syntax_engine::SyntaxEngine;

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
}
