use std::{fs, path::PathBuf};

use netherize_editor::syntax::{highlight::generate_highlight_spans, syntax_engine::SyntaxEngine};
use tree_sitter::Node;

const DEFAULT_SAMPLE_RUST: &str = r#"fn greet(name: &str) -> String {
    format!("hello, {}", name)
}

fn main() {
    let msg = greet("Netherize");
    println!("{}", msg);
}
"#;

fn main() {
    let input_path = std::env::args().nth(1).map(PathBuf::from);
    let (source_label, source_code) = load_source(input_path);

    println!("=== Phase6 Parser Bootstrap ===");
    println!("source={source_label}");

    let revision = 1_u64;
    let mut syntax_engine = match SyntaxEngine::new_rust() {
        Ok(engine) => engine,
        Err(err) => {
            eprintln!("init parser failed: {err}");
            return;
        }
    };

    let state = match syntax_engine.parse_source(&source_code, revision) {
        Ok(state) => state,
        Err(err) => {
            eprintln!("parse failed: {err}");
            return;
        }
    };

    println!(
        "parse ok: language={} revision={}",
        state.language_id().as_str(),
        state.revision()
    );

    let root = state.root_node();
    println!(
        "root: kind={} named={} child_count={} named_child_count={} bytes={}..{}",
        root.kind(),
        root.is_named(),
        root.child_count(),
        root.named_child_count(),
        root.start_byte(),
        root.end_byte()
    );

    let max_children = root.child_count().min(8);
    println!("--- first {max_children} root children ---");
    for index in 0..max_children {
        if let Some(child) = root.child(index) {
            println!("{}", format_node_line(index as usize, child, &source_code));
        }
    }

    let highlight_spans = generate_highlight_spans(state, &source_code);
    println!(
        "--- highlight spans (first {}) ---",
        highlight_spans.len().min(24)
    );
    for (index, span) in highlight_spans.iter().take(24).enumerate() {
        let snippet = source_code
            .get(span.range.clone())
            .map(preview_snippet)
            .unwrap_or_else(|| "<invalid-utf8-range>".to_string());
        println!(
            "[{index}] {} bytes={}..{} text=\"{}\"",
            span.category.as_str(),
            span.range.start,
            span.range.end,
            snippet
        );
    }
}

fn load_source(path: Option<PathBuf>) -> (String, String) {
    if let Some(path) = path {
        match fs::read_to_string(&path) {
            Ok(content) => return (path.display().to_string(), content),
            Err(err) => {
                eprintln!(
                    "read source file '{}' failed: {err}. fallback to built-in sample.",
                    path.display()
                );
            }
        }
    }

    (
        "<built-in sample rust>".to_string(),
        DEFAULT_SAMPLE_RUST.to_string(),
    )
}

fn format_node_line(index: usize, node: Node<'_>, source: &str) -> String {
    let start = node.start_position();
    let end = node.end_position();
    let snippet = source
        .get(node.start_byte()..node.end_byte())
        .map(preview_snippet)
        .unwrap_or_else(|| "<non-utf8-boundary>".to_string());

    format!(
        "[{index}] kind={} named={} bytes={}..{} pos=({}:{})..({}:{}) text=\"{}\"",
        node.kind(),
        node.is_named(),
        node.start_byte(),
        node.end_byte(),
        start.row,
        start.column,
        end.row,
        end.column,
        snippet
    )
}

fn preview_snippet(text: &str) -> String {
    let escaped = text.replace('\n', "\\n");
    let mut chars = escaped.chars();
    let preview: String = chars.by_ref().take(80).collect();
    if chars.next().is_some() {
        format!("{preview}...")
    } else {
        preview
    }
}
