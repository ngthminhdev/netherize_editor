use once_cell::sync::Lazy;
use regex::Regex;
use std::ops::Range;
use tree_sitter::Node;

use crate::config::theme_config::ThemeConfig;
use crate::syntax::{
    parser::language_id_for_extension,
    syntax_engine::{LanguageId, SyntaxEngine, SyntaxTreeState},
};

use std::collections::HashMap;

mod categories;
mod engine;
#[cfg(test)]
mod normalize_tests;
mod queries;
mod spans;

pub use categories::{HighlightCategory, HighlightPalette};
pub use queries::highlight_markdown_inline;
pub use spans::{
    HighlightEdit, HighlightSpan, apply_highlight_edits, expand_merge_window,
    merge_highlight_spans, overlay_highlight_layers,
};

use engine::{
    generate_injection_highlights, generate_query_highlight_spans, normalize_spans,
    sanitize_byte_range,
};

pub const INLINE_TREE_SITTER_BYTE_THRESHOLD: usize = 32 * 1024;
pub const INLINE_TREE_SITTER_LINE_THRESHOLD: usize = 300;
/// Plaintext regex highlighting runs inline on the main thread, so it must
/// stay bounded: above this size skip spans entirely (a 10 MB log freezes the
/// UI for seconds per keystroke otherwise).
pub const INLINE_PLAINTEXT_BYTE_THRESHOLD: usize = 1024 * 1024;

pub fn generate_highlight_spans(tree_state: &SyntaxTreeState, source: &str) -> Vec<HighlightSpan> {
    if tree_state.language_id() == LanguageId::Dotenv {
        return generate_dotenv_highlight_spans(source);
    }

    if tree_state.language_id() == LanguageId::Markdown {
        let base = generate_query_highlight_spans(
            tree_state.language_id(),
            tree_state.root_node(),
            source,
            None,
        );
        let inline = generate_markdown_inline_highlights(tree_state.root_node(), source, None);
        return overlay_highlight_layers(&base, &inline);
    }

    let base = generate_query_highlight_spans(
        tree_state.language_id(),
        tree_state.root_node(),
        source,
        None,
    );
    let injected = generate_injection_highlights(
        tree_state.language_id(),
        tree_state.root_node(),
        source,
        None,
    );
    if injected.is_empty() {
        base
    } else {
        overlay_highlight_layers(&base, &injected)
    }
}

/// Generate highlight spans with injection parser cache for better performance.
/// This version reuses parsers for embedded languages (e.g., bash in Dockerfile, code blocks in markdown).
pub fn generate_highlight_spans_with_cache(
    tree_state: &SyntaxTreeState,
    source: &str,
    injection_cache: &mut HashMap<LanguageId, SyntaxEngine>,
) -> Vec<HighlightSpan> {
    if tree_state.language_id() == LanguageId::Dotenv {
        return generate_dotenv_highlight_spans(source);
    }

    if tree_state.language_id() == LanguageId::Markdown {
        let base = generate_query_highlight_spans(
            tree_state.language_id(),
            tree_state.root_node(),
            source,
            None,
        );
        let inline = generate_markdown_inline_highlights(tree_state.root_node(), source, None);
        return overlay_highlight_layers(&base, &inline);
    }

    let base = generate_query_highlight_spans(
        tree_state.language_id(),
        tree_state.root_node(),
        source,
        None,
    );
    let injected = generate_injection_highlights(
        tree_state.language_id(),
        tree_state.root_node(),
        source,
        Some(injection_cache),
    );
    if injected.is_empty() {
        base
    } else {
        overlay_highlight_layers(&base, &injected)
    }
}

pub fn generate_dotenv_highlight_spans(source: &str) -> Vec<HighlightSpan> {
    let comment_re = Regex::new(r"^\s*#").unwrap();
    let export_re = Regex::new(r"^\s*(export)\s+([A-Za-z_][A-Za-z0-9_]*)").unwrap();
    let kv_re = Regex::new(r"^\s*([A-Za-z_][A-Za-z0-9_]*)(\s*=\s*)(.*)").unwrap();

    let mut spans = Vec::new();
    let mut byte_pos = 0usize;

    for line in source.lines() {
        let line_len = line.len();
        let line_start = byte_pos;

        if comment_re.is_match(line) {
            spans.push(HighlightSpan {
                range: line_start..line_start + line_len,
                category: HighlightCategory::Comment,
            });
        } else if let Some(caps) = export_re.captures(line) {
            let export_match = caps.get(1).unwrap();
            spans.push(HighlightSpan {
                range: line_start + export_match.start()..line_start + export_match.end(),
                category: HighlightCategory::Keyword,
            });
            let key_match = caps.get(2).unwrap();
            spans.push(HighlightSpan {
                range: line_start + key_match.start()..line_start + key_match.end(),
                category: HighlightCategory::Variable,
            });
        } else if let Some(caps) = kv_re.captures(line) {
            let key_match = caps.get(1).unwrap();
            spans.push(HighlightSpan {
                range: line_start + key_match.start()..line_start + key_match.end(),
                category: HighlightCategory::Keyword,
            });
            let op_match = caps.get(2).unwrap();
            spans.push(HighlightSpan {
                range: line_start + op_match.start()..line_start + op_match.end(),
                category: HighlightCategory::Operator,
            });
            let val_match = caps.get(3).unwrap();
            if val_match.end() > val_match.start() {
                spans.push(HighlightSpan {
                    range: line_start + val_match.start()..line_start + val_match.end(),
                    category: HighlightCategory::String,
                });
            }
        }

        byte_pos += line_len + 1; // +1 for newline
    }

    spans
}

static RE_PT_STRING: Lazy<Regex> =
    Lazy::new(|| Regex::new(r#""(?:[^"\\]|\\.)*"|'(?:[^'\\]|\\.)*'"#).unwrap());

static RE_PT_HASH: Lazy<Regex> = Lazy::new(|| Regex::new(r"\b[0-9a-fA-F]{8,}\b").unwrap());

static RE_PT_PATH: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?i)\b[a-z0-9_.-]+(?:/[a-z0-9_.-]+)+\b|/(?:[a-z0-9_.-]+/)+[a-z0-9_.-]+\b|\b[a-z0-9_.-]+\.(?:js|ts|jsx|tsx|rs|go|py|c|cpp|h|html|css|json|yaml|yml|toml|md|txt|sh|bash|xml|proto|rs)\b").unwrap()
});

static RE_PT_URL: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"\bhttps?://[a-zA-Z0-9_.-]+(?::\d+)?(?:/[a-zA-Z0-9_.-]+)*\b").unwrap()
});

static RE_PT_COMMENT: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?m)(?:^\s*#|^\s*//|^\s*;|(?:\s+#|\s+//|\s+;)).*$").unwrap());

static RE_PT_KEY_VALUE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?m)^[a-zA-Z0-9_.-]+\s*[:=]").unwrap());

static RE_PT_STATUS: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        r"\b(?:ERROR|WARN|WARNING|INFO|DEBUG|TRACE|FATAL|OK|SUCCESS|FAIL|FAILED|TODO|FIXME)\b",
    )
    .unwrap()
});

static RE_PT_DATE: Lazy<Regex> = Lazy::new(|| Regex::new(r"\b\d{4}-\d{2}-\d{2}\b").unwrap());

static RE_PT_TIME: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"\b\d{2}:\d{2}:\d{2}(?:\.\d+)?\b").unwrap());

static RE_PT_NUMBER: Lazy<Regex> = Lazy::new(|| Regex::new(r"\b\d+(?:\.\d+)?\b").unwrap());

static RE_PT_BOOL: Lazy<Regex> = Lazy::new(|| Regex::new(r"\b(?:true|false|null|nil)\b").unwrap());

pub fn generate_plaintext_highlight_spans(source: &str) -> Vec<HighlightSpan> {
    let mut raw = Vec::new();

    for m in RE_PT_STRING.find_iter(source) {
        raw.push(HighlightSpan {
            range: m.start()..m.end(),
            category: HighlightCategory::String,
        });
    }

    for m in RE_PT_URL.find_iter(source) {
        raw.push(HighlightSpan {
            range: m.start()..m.end(),
            category: HighlightCategory::Function,
        });
    }

    for m in RE_PT_PATH.find_iter(source) {
        raw.push(HighlightSpan {
            range: m.start()..m.end(),
            category: HighlightCategory::Function,
        });
    }

    for m in RE_PT_HASH.find_iter(source) {
        raw.push(HighlightSpan {
            range: m.start()..m.end(),
            category: HighlightCategory::Constant,
        });
    }

    for m in RE_PT_COMMENT.find_iter(source) {
        raw.push(HighlightSpan {
            range: m.start()..m.end(),
            category: HighlightCategory::Comment,
        });
    }

    for m in RE_PT_KEY_VALUE.find_iter(source) {
        let text = m.as_str();
        if let Some(pos) = text.find(':').or_else(|| text.find('=')) {
            let key_len = text[..pos].trim_end().len();
            raw.push(HighlightSpan {
                range: m.start()..(m.start() + key_len),
                category: HighlightCategory::Type,
            });
        }
    }

    for m in RE_PT_STATUS.find_iter(source) {
        raw.push(HighlightSpan {
            range: m.start()..m.end(),
            category: HighlightCategory::KeywordControl,
        });
    }

    for m in RE_PT_DATE.find_iter(source) {
        raw.push(HighlightSpan {
            range: m.start()..m.end(),
            category: HighlightCategory::Number,
        });
    }

    for m in RE_PT_TIME.find_iter(source) {
        raw.push(HighlightSpan {
            range: m.start()..m.end(),
            category: HighlightCategory::Number,
        });
    }

    for m in RE_PT_NUMBER.find_iter(source) {
        raw.push(HighlightSpan {
            range: m.start()..m.end(),
            category: HighlightCategory::Number,
        });
    }

    for m in RE_PT_BOOL.find_iter(source) {
        raw.push(HighlightSpan {
            range: m.start()..m.end(),
            category: HighlightCategory::Boolean,
        });
    }

    normalize_spans(source, raw, None)
}

pub fn generate_highlight_spans_in_byte_window(
    tree_state: &SyntaxTreeState,
    source: &str,
    window: Range<usize>,
) -> Vec<HighlightSpan> {
    let Some((start, end)) = sanitize_byte_range(source, window) else {
        return Vec::new();
    };
    let base = generate_query_highlight_spans(
        tree_state.language_id(),
        tree_state.root_node(),
        source,
        Some(start..end),
    );
    if tree_state.language_id() == LanguageId::Markdown {
        let inline =
            generate_markdown_inline_highlights(tree_state.root_node(), source, Some(start..end));
        return overlay_highlight_layers(&base, &inline);
    }
    base
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
    generate_highlight_spans(&tree_state, text)
}

pub(crate) fn generate_markdown_inline_highlights(
    root: Node<'_>,
    source: &str,
    byte_window: Option<Range<usize>>,
) -> Vec<HighlightSpan> {
    let mut spans = Vec::new();
    collect_markdown_inline_highlights(root, source, &byte_window, &mut spans);
    normalize_spans(source, spans, byte_window)
}

fn collect_markdown_inline_highlights(
    node: Node<'_>,
    source: &str,
    byte_window: &Option<Range<usize>>,
    out: &mut Vec<HighlightSpan>,
) {
    if node.is_error() || node.is_missing() {
        return;
    }

    if node.kind() == "inline" {
        let start = node.start_byte();
        let end = node.end_byte();
        if end > start && end <= source.len() {
            let overlaps = byte_window
                .as_ref()
                .is_none_or(|window| end > window.start && start < window.end);
            if overlaps {
                let text = &source[start..end];
                for mut span in highlight_markdown_inline(text) {
                    span.range = (span.range.start + start)..(span.range.end + start);
                    out.push(span);
                }
            }
        }
        return;
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        collect_markdown_inline_highlights(child, source, byte_window, out);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
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
        let spans = generate_highlight_spans(&tree, source);

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
        let spans = generate_highlight_spans(&tree, source);

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
const label = "hello";
function greet(name) {
    console.log(`hello ${name}`);
    return answer;
}
"#;

        let mut engine = SyntaxEngine::new(LanguageId::JavaScript).expect("init js parser");
        let tree = engine.parse_source(source, 12).expect("parse js");
        let spans = generate_highlight_spans(&tree, source);

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
        assert!(
            spans
                .iter()
                .any(|s| s.category == HighlightCategory::StringTemplate)
        );
    }

    #[test]
    fn javascript_template_interpolation_highlights_embedded_expression() {
        let source = "telegramClient.sendMessage(telegramId, `Game: ${listGameService.getServiceName(jackpotHistoryInfo.serviceId)} (${jackpotHistoryInfo.serviceId})\\n`);";

        let mut engine = SyntaxEngine::new(LanguageId::JavaScript).expect("init js parser");
        let tree = engine.parse_source(source, 1).expect("parse js");
        let spans = generate_highlight_spans(&tree, source);
        let interpolation_start = source.find("${").expect("find interpolation");
        let interpolation_end = source[interpolation_start..]
            .find('}')
            .map(|offset| interpolation_start + offset + 1)
            .expect("find interpolation end");

        assert!(
            spans.iter().any(|span| {
                span.range.start >= interpolation_start
                    && span.range.end <= interpolation_end
                    && span.category != HighlightCategory::String
            }),
            "expected template interpolation to contain non-string syntax spans, got {spans:?}"
        );
    }

    #[test]
    fn python_highlight_uses_default_query_captures() {
        let source = r#"
import os

MAX_SIZE = 32

@decorator
def greet(name: str) -> str:
    value = 42
    text = "hello"
    match value:
        case 42:
            return f"answer={value} {name}"
    return text
"#;

        let mut engine = SyntaxEngine::new(LanguageId::Python).expect("init python parser");
        let tree = engine.parse_source(source, 20).expect("parse python");
        let spans = generate_highlight_spans(&tree, source);

        assert!(!spans.is_empty(), "expected python highlight spans");
        assert!(
            spans
                .iter()
                .any(|s| s.category == HighlightCategory::Keyword
                    || s.category == HighlightCategory::KeywordControl
                    || s.category == HighlightCategory::KeywordStorage),
            "expected python keyword spans"
        );
        assert!(
            spans
                .iter()
                .any(|s| s.category == HighlightCategory::Function),
            "expected python function spans"
        );
        assert!(
            spans
                .iter()
                .any(|s| s.category == HighlightCategory::String),
            "expected python string spans"
        );
        assert!(
            spans
                .iter()
                .any(|s| s.category == HighlightCategory::Constant),
            "expected python constant spans"
        );
    }

    #[test]
    fn python_all_caps_name_is_constant_not_constructor() {
        // MAX_SIZE must render as a constant, MyClass as a constructor.
        let source = "MAX_SIZE = 10\nclass MyClass:\n    pass\n";

        let mut engine = SyntaxEngine::new(LanguageId::Python).expect("init python parser");
        let tree = engine.parse_source(source, 21).expect("parse python");
        let spans = generate_highlight_spans(&tree, source);

        let const_start = source.find("MAX_SIZE").expect("find MAX_SIZE");
        let const_end = const_start + "MAX_SIZE".len();
        assert!(
            spans.iter().any(|s| s.range.start == const_start
                && s.range.end == const_end
                && s.category == HighlightCategory::Constant),
            "ALL_CAPS name should be a Constant, got {spans:?}"
        );
        assert!(
            !spans.iter().any(|s| s.range.start == const_start
                && s.range.end == const_end
                && s.category == HighlightCategory::Constructor),
            "ALL_CAPS name must not be a Constructor"
        );
    }

    #[test]
    fn python_fstring_interpolation_highlights_embedded_expression() {
        let source = "count = 3\nlabel = f\"value={count}\"\n";

        let mut engine = SyntaxEngine::new(LanguageId::Python).expect("init python parser");
        let tree = engine.parse_source(source, 22).expect("parse python");
        let spans = generate_highlight_spans(&tree, source);

        // The `count` reference inside the f-string interpolation must not be
        // swallowed by the surrounding string span.
        let interp_start = source.find("{count}").expect("find interpolation") + 1;
        let interp_end = interp_start + "count".len();
        assert!(
            spans.iter().any(|s| {
                s.range.start >= interp_start
                    && s.range.end <= interp_end
                    && s.category != HighlightCategory::String
            }),
            "expected f-string interpolation to contain non-string spans, got {spans:?}"
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
        let spans = generate_highlight_spans(&tree, source);

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
        let spans = generate_highlight_spans(&tree, source);

        assert!(!spans.is_empty(), "expected sql highlight spans");
        assert!(
            spans
                .iter()
                .any(|s| s.category == HighlightCategory::Keyword)
        );
        assert!(
            spans
                .iter()
                .any(|s| s.category == HighlightCategory::Function
                    || s.category == HighlightCategory::FunctionBuiltin)
        );
        assert!(spans.iter().any(|s| s.category == HighlightCategory::Type
            || s.category == HighlightCategory::TypeBuiltin));
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
        let spans = generate_highlight_spans(&tree, source);

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

        assert_eq!(expand_merge_window(&existing, &replacement, 5..6), 0..10);
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
    fn later_semantic_overrides_win_when_override_spans_overlap() {
        let base = vec![HighlightSpan {
            range: 0..10,
            category: HighlightCategory::Identifier,
        }];
        let overrides = vec![
            HighlightSpan {
                range: 2..6,
                category: HighlightCategory::Constant,
            },
            HighlightSpan {
                range: 4..8,
                category: HighlightCategory::Field,
            },
        ];

        let merged = overlay_highlight_layers(&base, &overrides);

        assert_eq!(
            merged,
            vec![
                HighlightSpan {
                    range: 0..2,
                    category: HighlightCategory::Identifier,
                },
                HighlightSpan {
                    range: 2..4,
                    category: HighlightCategory::Constant,
                },
                HighlightSpan {
                    range: 4..8,
                    category: HighlightCategory::Field,
                },
                HighlightSpan {
                    range: 8..10,
                    category: HighlightCategory::Identifier,
                },
            ]
        );
    }

    #[test]
    fn inline_highlight_threshold_accepts_small_buffers() {
        assert!(should_highlight_inline("fn main() {}\n"));
        let large = "x".repeat(INLINE_TREE_SITTER_BYTE_THRESHOLD + 1);
        assert!(!should_highlight_inline(&large));
    }

    #[test]
    fn capture_category_maps_high_value_builtin_and_structure_tokens() {
        use engine::capture_category;
        assert_eq!(
            capture_category("function.builtin"),
            Some(HighlightCategory::FunctionBuiltin)
        );
        assert_eq!(
            capture_category("constant.builtin.boolean"),
            Some(HighlightCategory::Boolean)
        );
        assert_eq!(
            capture_category("module"),
            Some(HighlightCategory::Namespace)
        );
        assert_eq!(capture_category("label"), Some(HighlightCategory::Property));
    }

    #[test]
    fn test_all_highlight_queries_valid() {
        use crate::syntax::syntax_engine::LanguageId;
        use queries::highlight_query;
        for lang in [
            LanguageId::Java,
            LanguageId::Markdown,
            LanguageId::Rust,
            LanguageId::TypeScript,
            LanguageId::Go,
            LanguageId::Json,
            LanguageId::Protobuf,
            LanguageId::Dart,
        ] {
            let q = highlight_query(lang);
            assert!(
                q.is_some(),
                "{} highlight query failed to load",
                lang.as_str()
            );
        }
    }
}
