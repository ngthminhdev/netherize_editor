# Dotenv Syntax Highlighting — Implementation Plan

## Vấn đề
File `.env`, `.env.local`, `env.dist`, `*.env` không có syntax highlight.

## Các bước

### Bước 1: Thêm crate dependency (Cargo.toml)
Thêm `tree-sitter-dotenv = "0.2"`

### Bước 2: Thêm LanguageId::Dotenv (syntax_engine.rs)
- Thêm variant `Dotenv` vào enum
- Thêm `"dotenv"` vào `as_str()`

### Bước 3: Parser mapping (parser.rs)
- `language_id_for_extension("env")` → `LanguageId::Dotenv`
- `tree_sitter_language(LanguageId::Dotenv)` → `tree_sitter_dotenv::LANGUAGE`

### Bước 4: Registry profile (lsp/registry.rs)
- `LanguageProfile` với `filenames: &[".env", ".env*", "env.*"]`
- `extensions: &["env"]`

### Bước 5: Highlight query (queries/dotenv/highlights.scm)
- comment → @syntax.comment
- key → @syntax.property
- value → @syntax.string
- "export" → @syntax.keyword
- "=" → @syntax.operator

### Bước 6: Đăng ký query (highlight.rs)
- `dotenv_highlight_query()` dùng `include_str!("queries/dotenv/highlights.scm")`
- Thêm arm `LanguageId::Dotenv` trong `highlight_query()`

### Bước 7: Build & test
