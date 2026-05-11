use tree_sitter::Node;

use super::syntax_engine::LanguageId;

/// Walk the tree-sitter AST and return foldable line ranges (start/end inclusive).
/// The start line stays visible as the marker; following lines through end are hidden.
/// Only ranges spanning ≥ 2 lines are included. Ranges are sorted by start line.
/// Nested ranges are preserved to allow folding at multiple levels.
pub fn compute_foldable_ranges(root_node: Node, language_id: LanguageId) -> Vec<(usize, usize)> {
    let foldable_kinds = foldable_node_kinds(language_id);
    if foldable_kinds.is_empty() {
        return Vec::new();
    }

    let mut ranges: Vec<(usize, usize)> = Vec::new();
    collect_foldable_ranges(root_node, &foldable_kinds, &mut ranges);

    // Sort by start line, then by end line (descending) for consistent ordering
    ranges.sort_by(|a, b| {
        if a.0 != b.0 {
            a.0.cmp(&b.0)
        } else {
            b.1.cmp(&a.1) // Larger ranges first when starting at same line
        }
    });

    // Remove exact duplicates only
    ranges.dedup();

    ranges
}

fn collect_foldable_ranges(
    node: Node,
    foldable: &std::collections::HashSet<&str>,
    ranges: &mut Vec<(usize, usize)>,
) {
    let kind = node.kind();

    // Nếu node này là foldable và đủ lớn (≥2 dòng), thêm vào ranges
    if foldable.contains(kind) {
        let start = node.start_position().row;
        let end = node.end_position().row;
        if end > start + 1 {
            ranges.push((start, end));
        }
    }

    // Luôn traverse children để tìm nested foldable scopes
    for i in 0..node.child_count() {
        if let Some(child) = node.child(i) {
            collect_foldable_ranges(child, foldable, ranges);
        }
    }
}

fn foldable_node_kinds(
    language_id: LanguageId,
) -> &'static std::collections::HashSet<&'static str> {
    use LanguageId::*;
    use std::collections::HashSet;
    use std::sync::LazyLock;

    macro_rules! set {
        ($($item:literal),* $(,)?) => {{
            static SET: LazyLock<HashSet<&'static str>> = LazyLock::new(|| {
                let mut s = HashSet::new();
                $(s.insert($item);)*
                s
            });
            &*SET
        }};
    }

    match language_id {
        Rust => set! {
            "block", "impl_item", "trait_item", "struct_item", "enum_item",
            "enum_variant", "match_arm", "if_expression", "while_expression",
            "for_expression", "loop_expression", "mod_item", "macro_definition",
            "closure_expression", "function_item", "use_declaration"
        },
        JavaScript | Jsx => set! {
            "statement_block", "class_body", "object", "arrow_function",
            "function_declaration", "method_definition", "switch_body",
            "if_statement", "for_statement", "while_statement", "try_statement",
            "catch_clause"
        },
        TypeScript | Tsx => set! {
            "statement_block", "class_body", "object_type", "arrow_function",
            "function_declaration", "method_definition", "switch_body",
            "if_statement", "for_statement", "while_statement", "try_statement",
            "catch_clause", "interface_declaration", "enum_declaration",
            "object", "type_alias_declaration"
        },
        Go => set! {
            "block", "if_statement", "for_statement", "func_literal",
            "struct_type", "interface_type", "switch_statement", "select_statement"
        },
        Python => set! {
            "block", "function_definition", "class_definition", "if_statement",
            "for_statement", "while_statement", "with_statement", "try_statement",
            "match_statement"
        },
        Java => set! {
            "block", "class_body", "interface_body", "enum_body",
            "constructor_body", "if_statement", "for_statement",
            "while_statement", "try_statement", "switch_block"
        },
        Html | Xml => set! {
            "element"
        },
        Json => set! {
            "object", "array"
        },
        Markdown => set! {
            "section", "fenced_code_block"
        },
        Css => set! {
            "block", "rule_set", "keyframe_block", "supports_statement",
            "media_statement", "font_feature_values_statement"
        },
        Bash => set! {
            "compound_statement", "if_statement", "for_statement",
            "while_statement", "case_statement", "function_definition",
            "do_group", "subshell"
        },
        Yaml => set! {
            "block_mapping", "block_sequence", "flow_node"
        },
        Sql => set! {
            "statement", "cte", "subquery"
        },
        Dotenv | Protobuf | Dockerfile | Plaintext => {
            static EMPTY: std::sync::LazyLock<std::collections::HashSet<&'static str>> =
                std::sync::LazyLock::new(std::collections::HashSet::new);
            &*EMPTY
        }
    }
}
