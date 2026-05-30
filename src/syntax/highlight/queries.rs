use super::spans::HighlightSpan;
use crate::syntax::parser::tree_sitter_language;
use crate::syntax::syntax_engine::LanguageId;
use std::sync::OnceLock;
use tree_sitter::Query;

pub(crate) fn highlight_query(language_id: LanguageId) -> Option<&'static Query> {
    match language_id {
        LanguageId::Rust => rust_highlight_query(),
        LanguageId::JavaScript => javascript_highlight_query(),
        LanguageId::Jsx => jsx_highlight_query(),
        LanguageId::TypeScript => typescript_highlight_query(),
        LanguageId::Tsx => tsx_highlight_query(),
        LanguageId::Go => go_highlight_query(),
        LanguageId::Sql => sql_highlight_query(),
        LanguageId::Yaml => yaml_highlight_query(),
        LanguageId::Dockerfile => dockerfile_highlight_query(),
        LanguageId::Json => json_highlight_query(),
        LanguageId::Bash => bash_highlight_query(),
        LanguageId::Markdown => markdown_highlight_query(),
        LanguageId::Dotenv => None,
        LanguageId::Java => java_highlight_query(),
        LanguageId::Python => python_highlight_query(),
        LanguageId::Html => html_highlight_query(),
        LanguageId::Css => css_highlight_query(),
        LanguageId::Protobuf => protobuf_highlight_query(),
        LanguageId::Xml => xml_highlight_query(),
        LanguageId::Dart => dart_highlight_query(),
        LanguageId::Plaintext => None,
    }
}

pub(crate) fn rust_highlight_query() -> Option<&'static Query> {
    static QUERY: OnceLock<Option<Query>> = OnceLock::new();
    QUERY
        .get_or_init(|| {
            build_highlight_query(
                LanguageId::Rust,
                include_str!("../queries/rust/highlights.scm"),
                "rust",
            )
        })
        .as_ref()
}

pub(crate) fn javascript_highlight_query() -> Option<&'static Query> {
    static QUERY: OnceLock<Option<Query>> = OnceLock::new();
    QUERY
        .get_or_init(|| {
            build_highlight_query(
                LanguageId::JavaScript,
                tree_sitter_javascript::HIGHLIGHT_QUERY,
                "javascript",
            )
        })
        .as_ref()
}

pub(crate) fn jsx_highlight_query() -> Option<&'static Query> {
    static QUERY: OnceLock<Option<Query>> = OnceLock::new();
    QUERY
        .get_or_init(|| {
            let source = format!(
                "{}\n{}",
                tree_sitter_javascript::HIGHLIGHT_QUERY,
                tree_sitter_javascript::JSX_HIGHLIGHT_QUERY
            );
            build_highlight_query(LanguageId::Jsx, &source, "jsx")
        })
        .as_ref()
}

pub(crate) fn typescript_highlight_query() -> Option<&'static Query> {
    static QUERY: OnceLock<Option<Query>> = OnceLock::new();
    QUERY
        .get_or_init(|| {
            let source = format!(
                "{}\n{}",
                tree_sitter_javascript::HIGHLIGHT_QUERY,
                tree_sitter_typescript::HIGHLIGHTS_QUERY,
            );
            build_highlight_query(LanguageId::TypeScript, &source, "typescript")
        })
        .as_ref()
}

pub(crate) fn tsx_highlight_query() -> Option<&'static Query> {
    static QUERY: OnceLock<Option<Query>> = OnceLock::new();
    QUERY
        .get_or_init(|| {
            let source = format!(
                "{}\n{}\n{}",
                tree_sitter_javascript::HIGHLIGHT_QUERY,
                tree_sitter_javascript::JSX_HIGHLIGHT_QUERY,
                tree_sitter_typescript::HIGHLIGHTS_QUERY,
            );
            build_highlight_query(LanguageId::Tsx, &source, "tsx")
        })
        .as_ref()
}

pub(crate) fn go_highlight_query() -> Option<&'static Query> {
    static QUERY: OnceLock<Option<Query>> = OnceLock::new();
    QUERY
        .get_or_init(|| {
            build_highlight_query(
                LanguageId::Go,
                include_str!("../queries/go/highlights.scm"),
                "go",
            )
        })
        .as_ref()
}

pub(crate) fn sql_highlight_query() -> Option<&'static Query> {
    static QUERY: OnceLock<Option<Query>> = OnceLock::new();
    QUERY
        .get_or_init(|| {
            build_highlight_query(
                LanguageId::Sql,
                include_str!("../queries/sql/highlights.scm"),
                "sql",
            )
        })
        .as_ref()
}

pub(crate) fn yaml_highlight_query() -> Option<&'static Query> {
    static QUERY: OnceLock<Option<Query>> = OnceLock::new();
    QUERY
        .get_or_init(|| {
            build_highlight_query(
                LanguageId::Yaml,
                include_str!("../queries/yaml/highlights.scm"),
                "yaml",
            )
        })
        .as_ref()
}

pub(crate) fn json_highlight_query() -> Option<&'static Query> {
    static QUERY: OnceLock<Option<Query>> = OnceLock::new();
    QUERY
        .get_or_init(|| {
            build_highlight_query(
                LanguageId::Json,
                include_str!("../queries/json/highlights.scm"),
                "json",
            )
        })
        .as_ref()
}

pub(crate) fn bash_highlight_query() -> Option<&'static Query> {
    static QUERY: OnceLock<Option<Query>> = OnceLock::new();
    QUERY
        .get_or_init(|| {
            build_highlight_query(LanguageId::Bash, tree_sitter_bash::HIGHLIGHT_QUERY, "bash")
        })
        .as_ref()
}

pub(crate) fn markdown_highlight_query() -> Option<&'static Query> {
    static QUERY: OnceLock<Option<Query>> = OnceLock::new();
    QUERY
        .get_or_init(|| {
            build_highlight_query(
                LanguageId::Markdown,
                include_str!("../queries/markdown/highlights.scm"),
                "markdown",
            )
        })
        .as_ref()
}

pub(crate) fn markdown_inline_highlight_query() -> Option<&'static Query> {
    static QUERY: OnceLock<Option<Query>> = OnceLock::new();
    QUERY
        .get_or_init(|| {
            let language = crate::syntax::parser::tree_sitter_markdown_inline_language();
            match Query::new(
                &language,
                include_str!("../queries/markdown/inline_highlights.scm"),
            ) {
                Ok(q) => Some(q),
                Err(err) => {
                    eprintln!("[highlight] invalid markdown-inline highlight query: {err}");
                    None
                }
            }
        })
        .as_ref()
}

pub fn highlight_markdown_inline(text: &str) -> Vec<HighlightSpan> {
    if text.is_empty() || !super::should_highlight_inline(text) {
        return Vec::new();
    }
    let Some(query) = markdown_inline_highlight_query() else {
        return Vec::new();
    };
    let mut parser = tree_sitter::Parser::new();
    let language = crate::syntax::parser::tree_sitter_markdown_inline_language();
    if parser.set_language(&language).is_err() {
        return Vec::new();
    }
    let Some(tree) = parser.parse(text, None) else {
        return Vec::new();
    };
    super::engine::generate_query_highlight_spans_for_node(query, tree.root_node(), text)
}

pub(crate) fn dockerfile_highlight_query() -> Option<&'static Query> {
    static QUERY: OnceLock<Option<Query>> = OnceLock::new();
    QUERY
        .get_or_init(|| {
            build_highlight_query(
                LanguageId::Dockerfile,
                include_str!("../queries/dockerfile/highlights.scm"),
                "dockerfile",
            )
        })
        .as_ref()
}

pub(crate) fn java_highlight_query() -> Option<&'static Query> {
    static QUERY: OnceLock<Option<Query>> = OnceLock::new();
    QUERY
        .get_or_init(|| {
            build_highlight_query(
                LanguageId::Java,
                include_str!("../queries/java/highlights.scm"),
                "java",
            )
        })
        .as_ref()
}

pub(crate) fn python_highlight_query() -> Option<&'static Query> {
    static QUERY: OnceLock<Option<Query>> = OnceLock::new();
    QUERY
        .get_or_init(|| {
            build_highlight_query(
                LanguageId::Python,
                include_str!("../queries/python/highlights.scm"),
                "python",
            )
        })
        .as_ref()
}

pub(crate) fn html_highlight_query() -> Option<&'static Query> {
    static QUERY: OnceLock<Option<Query>> = OnceLock::new();
    QUERY
        .get_or_init(|| {
            build_highlight_query(LanguageId::Html, tree_sitter_html::HIGHLIGHTS_QUERY, "html")
        })
        .as_ref()
}

pub(crate) fn css_highlight_query() -> Option<&'static Query> {
    static QUERY: OnceLock<Option<Query>> = OnceLock::new();
    QUERY
        .get_or_init(|| {
            build_highlight_query(LanguageId::Css, tree_sitter_css::HIGHLIGHTS_QUERY, "css")
        })
        .as_ref()
}

pub(crate) fn protobuf_highlight_query() -> Option<&'static Query> {
    static QUERY: OnceLock<Option<Query>> = OnceLock::new();
    QUERY
        .get_or_init(|| {
            build_highlight_query(
                LanguageId::Protobuf,
                include_str!("../queries/proto/highlights.scm"),
                "protobuf",
            )
        })
        .as_ref()
}

pub(crate) fn xml_highlight_query() -> Option<&'static Query> {
    static QUERY: OnceLock<Option<Query>> = OnceLock::new();
    QUERY
        .get_or_init(|| {
            build_highlight_query(
                LanguageId::Xml,
                include_str!("../queries/xml/highlights.scm"),
                "xml",
            )
        })
        .as_ref()
}

pub(crate) fn dart_highlight_query() -> Option<&'static Query> {
    static QUERY: OnceLock<Option<Query>> = OnceLock::new();
    QUERY
        .get_or_init(|| {
            build_highlight_query(
                LanguageId::Dart,
                include_str!("../queries/dart/highlights.scm"),
                "dart",
            )
        })
        .as_ref()
}

fn build_highlight_query(language_id: LanguageId, source: &str, label: &str) -> Option<Query> {
    let language = tree_sitter_language(language_id)?;
    match Query::new(&language, source) {
        Ok(q) => Some(q),
        Err(err) => {
            eprintln!(
                "[highlight] invalid {label} highlight query: {err}\n\
                 Source (first 500 chars):\n{}\n---",
                &source[..source.len().min(500)]
            );
            None
        }
    }
}

pub(crate) fn injection_query(language_id: LanguageId) -> Option<&'static Query> {
    match language_id {
        LanguageId::Dockerfile => dockerfile_injection_query(),
        _ => None,
    }
}

fn dockerfile_injection_query() -> Option<&'static Query> {
    static QUERY: OnceLock<Option<Query>> = OnceLock::new();
    QUERY
        .get_or_init(|| {
            build_injection_query(
                LanguageId::Dockerfile,
                include_str!("../queries/dockerfile/injections.scm"),
                "dockerfile-injection",
            )
        })
        .as_ref()
}

fn build_injection_query(language_id: LanguageId, source: &str, label: &str) -> Option<Query> {
    let language = tree_sitter_language(language_id)?;
    match Query::new(&language, source) {
        Ok(q) => Some(q),
        Err(err) => {
            eprintln!(
                "[highlight] invalid {label} injection query: {err}\n\
                 Source (first 500 chars):\n{}\n---",
                &source[..source.len().min(500)]
            );
            None
        }
    }
}
