use std::path::Path;

use tree_sitter::Language;

use crate::{lsp::registry::language_profile_for_path, syntax::syntax_engine::LanguageId};

pub fn language_id_for_path(path: &Path) -> Option<LanguageId> {
    language_profile_for_path(path).and_then(|profile| profile.syntax_language_id)
}

pub fn language_id_for_extension(extension: &str) -> Option<LanguageId> {
    let ext = extension
        .trim()
        .trim_start_matches('.')
        .to_ascii_lowercase();
    match ext.as_str() {
        "rs" | "rust" => Some(LanguageId::Rust),
        "js" | "javascript" => Some(LanguageId::JavaScript),
        "jsx" => Some(LanguageId::Jsx),
        "ts" | "typescript" => Some(LanguageId::TypeScript),
        "tsx" => Some(LanguageId::Tsx),
        "go" => Some(LanguageId::Go),
        "sql" => Some(LanguageId::Sql),
        "yaml" | "yml" => Some(LanguageId::Yaml),
        "json" => Some(LanguageId::Json),
        "bash" | "sh" | "zsh" | "shell" => Some(LanguageId::Bash),
        "dockerfile" => Some(LanguageId::Dockerfile),
        "md" | "markdown" | "mdx" => Some(LanguageId::Markdown),
        _ => None,
    }
}

pub fn tree_sitter_language(language_id: LanguageId) -> Option<Language> {
    match language_id {
        LanguageId::Rust => Some(tree_sitter_rust::LANGUAGE.into()),
        LanguageId::JavaScript | LanguageId::Jsx => Some(tree_sitter_javascript::LANGUAGE.into()),
        LanguageId::TypeScript => Some(tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into()),
        LanguageId::Tsx => Some(tree_sitter_typescript::LANGUAGE_TSX.into()),
        LanguageId::Go => Some(tree_sitter_go::LANGUAGE.into()),
        LanguageId::Sql => Some(tree_sitter_sequel::LANGUAGE.into()),
        LanguageId::Yaml => Some(tree_sitter_yaml::LANGUAGE.into()),
        LanguageId::Json => Some(tree_sitter_json::LANGUAGE.into()),
        LanguageId::Bash => Some(tree_sitter_bash::LANGUAGE.into()),
        LanguageId::Dockerfile => Some(tree_sitter_containerfile::LANGUAGE.into()),
        LanguageId::Markdown => Some(tree_sitter_md::LANGUAGE.into()),
    }
}
