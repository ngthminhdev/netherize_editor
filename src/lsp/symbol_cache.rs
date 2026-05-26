use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};

/// A workspace-wide symbol cache for fast import suggestions.
/// Stores symbols indexed from LSP `workspace/symbol` requests and local TS/JS
/// project exports.
#[derive(Debug, Clone)]
pub struct WorkspaceSymbolCache {
    inner: Arc<RwLock<SymbolCacheInner>>,
}

#[derive(Debug, Default)]
struct SymbolCacheInner {
    /// Symbols grouped by language ID (e.g., "rust", "python", "typescript")
    symbols_by_language: HashMap<String, Vec<CachedSymbol>>,
    /// Indexing progress: (current, total) per language
    indexing_progress: HashMap<String, (usize, usize)>,
}

/// A cached symbol from workspace/symbol response
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CachedSymbol {
    pub name: String,
    pub kind: String,
    pub container_name: Option<String>,
    pub file_path: PathBuf,
    pub line: u32,
    pub character: u32,
    pub source_path: Option<PathBuf>,
    pub import_path: Option<String>,
    pub export_kind: Option<String>,
}

impl WorkspaceSymbolCache {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(RwLock::new(SymbolCacheInner::default())),
        }
    }

    /// Insert symbols for a language, replacing any existing cache
    pub fn insert_symbols(&self, language_id: &str, symbols: Vec<CachedSymbol>) {
        if let Ok(mut inner) = self.inner.write() {
            inner.symbols_by_language.insert(language_id.to_string(), symbols);
            inner.indexing_progress.remove(language_id);
        }
    }

    /// Replace cached export symbols for a single file while preserving symbols
    /// indexed from other files.
    pub fn upsert_file_symbols(
        &self,
        language_id: &str,
        file_path: &Path,
        symbols: Vec<CachedSymbol>,
    ) {
        if let Ok(mut inner) = self.inner.write() {
            let bucket = inner
                .symbols_by_language
                .entry(language_id.to_string())
                .or_default();
            bucket.retain(|symbol| {
                let source = symbol.source_path.as_deref().unwrap_or(symbol.file_path.as_path());
                source != file_path
            });
            bucket.extend(symbols);
        }
    }

    /// Query symbols by prefix, optionally filtered by language
    pub fn query_symbols(&self, prefix: &str, language_id: Option<&str>) -> Vec<CachedSymbol> {
        let Ok(inner) = self.inner.read() else {
            return Vec::new();
        };

        let prefix_lower = prefix.to_lowercase();
        let mut results = Vec::new();

        let languages: Vec<&String> = match language_id {
            Some(lang) => inner.symbols_by_language.keys().filter(|k| k.as_str() == lang).collect(),
            None => inner.symbols_by_language.keys().collect(),
        };

        for lang in languages {
            if let Some(symbols) = inner.symbols_by_language.get(lang) {
                for symbol in symbols {
                    if fuzzy_match(&symbol.name.to_lowercase(), &prefix_lower) {
                        results.push(symbol.clone());
                    }
                }
            }
        }

        // Sort by relevance: exact prefix match first, then importable local
        // exports, then by name length.
        results.sort_by(|a, b| {
            let a_starts = a.name.to_lowercase().starts_with(&prefix_lower);
            let b_starts = b.name.to_lowercase().starts_with(&prefix_lower);
            match (a_starts, b_starts) {
                (true, false) => std::cmp::Ordering::Less,
                (false, true) => std::cmp::Ordering::Greater,
                _ => b
                    .import_path
                    .is_some()
                    .cmp(&a.import_path.is_some())
                    .then_with(|| a.name.len().cmp(&b.name.len()))
                    .then_with(|| a.name.cmp(&b.name)),
            }
        });

        results
    }

    /// Get the number of cached symbols for a language
    pub fn symbol_count(&self, language_id: &str) -> usize {
        self.inner
            .read()
            .ok()
            .and_then(|inner| inner.symbols_by_language.get(language_id).map(|v| v.len()))
            .unwrap_or(0)
    }

    /// Clear all cached symbols for a language
    pub fn clear_language(&self, language_id: &str) {
        if let Ok(mut inner) = self.inner.write() {
            inner.symbols_by_language.remove(language_id);
            inner.indexing_progress.remove(language_id);
        }
    }

    /// Clear all cached symbols
    pub fn clear_all(&self) {
        if let Ok(mut inner) = self.inner.write() {
            inner.symbols_by_language.clear();
            inner.indexing_progress.clear();
        }
    }

    /// Set indexing progress for a language
    pub fn set_indexing_progress(&self, language_id: &str, current: usize, total: usize) {
        if let Ok(mut inner) = self.inner.write() {
            inner.indexing_progress.insert(language_id.to_string(), (current, total));
        }
    }

    /// Get indexing progress for a language
    pub fn indexing_progress(&self, language_id: &str) -> Option<(usize, usize)> {
        self.inner
            .read()
            .ok()
            .and_then(|inner| inner.indexing_progress.get(language_id).copied())
    }

    /// Check if a language is currently being indexed
    pub fn is_indexing(&self, language_id: &str) -> bool {
        self.indexing_progress(language_id).is_some()
    }
}

impl Default for WorkspaceSymbolCache {
    fn default() -> Self {
        Self::new()
    }
}

const MAX_TS_JS_INDEX_FILES: usize = 4000;
const MAX_TS_JS_FILE_BYTES: u64 = 512 * 1024;

pub fn index_ts_js_workspace_exports(workspace_root: &Path) -> Vec<CachedSymbol> {
    let mut files = Vec::new();
    collect_ts_js_files(workspace_root, &mut files, MAX_TS_JS_INDEX_FILES);

    let mut symbols = Vec::new();
    let mut seen = HashSet::new();
    for file_path in files {
        let Ok(metadata) = fs::metadata(&file_path) else {
            continue;
        };
        if metadata.len() > MAX_TS_JS_FILE_BYTES {
            continue;
        }
        let Ok(text) = fs::read_to_string(&file_path) else {
            continue;
        };
        for symbol in extract_ts_js_exports_from_text(&file_path, workspace_root, &text) {
            let key = (symbol.name.clone(), symbol.file_path.clone());
            if seen.insert(key) {
                symbols.push(symbol);
            }
        }
    }
    symbols
}

pub fn extract_ts_js_exports_from_text(
    file_path: &Path,
    workspace_root: &Path,
    text: &str,
) -> Vec<CachedSymbol> {
    let import_path = workspace_relative_module_path(workspace_root, file_path);
    let mut symbols = Vec::new();
    let mut seen = HashSet::new();

    for (line_idx, line) in text.lines().enumerate() {
        let trimmed = line.trim_start();
        if !trimmed.starts_with("export ") || trimmed.starts_with("export default") {
            continue;
        }

        let rest = trimmed.trim_start_matches("export ").trim_start();
        let candidates = export_candidates_from_line(rest);
        for (name, kind) in candidates {
            if name == "default" || !seen.insert(name.clone()) {
                continue;
            }
            let character = line.find(&name).unwrap_or(0) as u32;
            symbols.push(CachedSymbol {
                name,
                kind,
                container_name: None,
                file_path: file_path.to_path_buf(),
                line: line_idx as u32,
                character,
                source_path: Some(file_path.to_path_buf()),
                import_path: import_path.clone(),
                export_kind: Some("named".to_string()),
            });
        }
    }

    symbols
}

fn collect_ts_js_files(root: &Path, out: &mut Vec<PathBuf>, limit: usize) {
    if out.len() >= limit {
        return;
    }
    let Ok(entries) = fs::read_dir(root) else {
        return;
    };
    for entry in entries.flatten() {
        if out.len() >= limit {
            break;
        }
        let path = entry.path();
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        if file_type.is_dir() {
            let Some(name) = path.file_name().and_then(|value| value.to_str()) else {
                continue;
            };
            if should_skip_index_dir(name) {
                continue;
            }
            collect_ts_js_files(&path, out, limit);
        } else if file_type.is_file() && is_ts_js_source_path(&path) {
            out.push(path);
        }
    }
}

fn should_skip_index_dir(name: &str) -> bool {
    matches!(
        name,
        ".git" | "node_modules" | "dist" | "build" | "target" | ".next" | ".turbo"
    )
}

fn is_ts_js_source_path(path: &Path) -> bool {
    matches!(
        path.extension().and_then(|value| value.to_str()),
        Some("ts" | "tsx" | "js" | "jsx" | "mjs" | "cjs")
    )
}

fn workspace_relative_module_path(workspace_root: &Path, file_path: &Path) -> Option<String> {
    let relative = file_path.strip_prefix(workspace_root).ok()?;
    let mut value = strip_ts_js_extension(relative).to_string_lossy().replace('\\', "/");
    if value.ends_with("/index") {
        value.truncate(value.len().saturating_sub("/index".len()));
    }
    if value.is_empty() {
        None
    } else {
        Some(value)
    }
}

fn strip_ts_js_extension(path: &Path) -> PathBuf {
    let mut value = path.to_path_buf();
    if matches!(
        value.extension().and_then(|ext| ext.to_str()),
        Some("ts" | "tsx" | "js" | "jsx" | "mjs" | "cjs")
    ) {
        value.set_extension("");
    }
    value
}

fn export_candidates_from_line(rest: &str) -> Vec<(String, String)> {
    if rest.starts_with('{') {
        return parse_export_list(rest, "Symbol");
    }
    if let Some(after_type) = rest.strip_prefix("type ") {
        if after_type.trim_start().starts_with('{') {
            return parse_export_list(after_type.trim_start(), "Type");
        }
        return named_after_keyword(after_type, "", "Type").into_iter().collect();
    }
    if let Some(after_async) = rest.strip_prefix("async ") {
        if let Some(name) = named_after_keyword(after_async, "function", "Function") {
            return vec![name];
        }
    }
    for (keyword, kind) in [
        ("function", "Function"),
        ("const", "Constant"),
        ("let", "Variable"),
        ("var", "Variable"),
        ("class", "Class"),
        ("interface", "Interface"),
        ("enum", "Enum"),
    ] {
        if let Some(name) = named_after_keyword(rest, keyword, kind) {
            return vec![name];
        }
    }
    Vec::new()
}

fn named_after_keyword(rest: &str, keyword: &str, kind: &str) -> Option<(String, String)> {
    let after_keyword = if keyword.is_empty() {
        rest.trim_start()
    } else {
        rest.strip_prefix(keyword)?.trim_start()
    };
    let name = read_identifier(after_keyword)?;
    Some((name, kind.to_string()))
}

fn parse_export_list(rest: &str, kind: &str) -> Vec<(String, String)> {
    let Some(end) = rest.find('}') else {
        return Vec::new();
    };
    rest[1..end]
        .split(',')
        .filter_map(|entry| {
            let cleaned = entry.trim();
            if cleaned.is_empty() {
                return None;
            }
            let exported_name = cleaned
                .split_once(" as ")
                .map(|(_, alias)| alias.trim())
                .unwrap_or(cleaned);
            let name = read_identifier(exported_name)?;
            Some((name, kind.to_string()))
        })
        .collect()
}

fn read_identifier(input: &str) -> Option<String> {
    let mut out = String::new();
    for ch in input.chars() {
        if out.is_empty() {
            if ch.is_ascii_alphabetic() || ch == '_' || ch == '$' {
                out.push(ch);
                continue;
            }
            return None;
        }
        if ch.is_ascii_alphanumeric() || ch == '_' || ch == '$' {
            out.push(ch);
        } else {
            break;
        }
    }
    (!out.is_empty()).then_some(out)
}

/// Simple fuzzy matching: all characters in needle must appear in haystack in order
fn fuzzy_match(haystack: &str, needle: &str) -> bool {
    if needle.is_empty() {
        return true;
    }
    let mut needle_chars = needle.chars();
    let mut current = needle_chars.next();

    for hay_char in haystack.chars() {
        if Some(hay_char) == current {
            current = needle_chars.next();
            if current.is_none() {
                return true;
            }
        }
    }

    current.is_none()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cached_symbol(name: &str, kind: &str, file: &str, line: u32, character: u32) -> CachedSymbol {
        CachedSymbol {
            name: name.to_string(),
            kind: kind.to_string(),
            container_name: None,
            file_path: PathBuf::from(file),
            line,
            character,
            source_path: None,
            import_path: None,
            export_kind: None,
        }
    }

    #[test]
    fn test_fuzzy_match() {
        assert!(fuzzy_match("hello_world", "hw"));
        assert!(fuzzy_match("hello_world", "hew"));
        assert!(fuzzy_match("hello_world", "hello"));
        assert!(fuzzy_match("hello_world", "world"));
        assert!(!fuzzy_match("hello_world", "hw2"));
        assert!(!fuzzy_match("hello_world", "wh"));
        assert!(fuzzy_match("hello_world", ""));
    }

    #[test]
    fn test_insert_and_query_symbols() {
        let cache = WorkspaceSymbolCache::new();

        let symbols = vec![
            cached_symbol("hello_world", "Function", "test.rs", 10, 5),
            cached_symbol("HelloWorld", "Class", "test.rs", 20, 0),
            cached_symbol("test_function", "Function", "test.rs", 30, 0),
        ];

        cache.insert_symbols("rust", symbols);

        // Query with prefix
        let results = cache.query_symbols("hello", Some("rust"));
        assert_eq!(results.len(), 2);
        assert!(results.iter().any(|s| s.name == "hello_world"));
        assert!(results.iter().any(|s| s.name == "HelloWorld"));

        // Query with fuzzy match
        let results = cache.query_symbols("hw", Some("rust"));
        assert!(results.len() >= 1);
        assert!(results.iter().any(|s| s.name == "hello_world"));

        // Query all languages
        let results = cache.query_symbols("test", None);
        assert!(results.len() >= 1);
        assert!(results.iter().any(|s| s.name == "test_function"));
    }

    #[test]
    fn test_symbol_count() {
        let cache = WorkspaceSymbolCache::new();

        let symbols = vec![
            cached_symbol("func1", "Function", "test.rs", 10, 0),
            cached_symbol("func2", "Function", "test.rs", 20, 0),
        ];

        cache.insert_symbols("rust", symbols);
        assert_eq!(cache.symbol_count("rust"), 2);
        assert_eq!(cache.symbol_count("python"), 0);
    }

    #[test]
    fn test_clear_language() {
        let cache = WorkspaceSymbolCache::new();

        let symbols = vec![cached_symbol("func1", "Function", "test.rs", 10, 0)];

        cache.insert_symbols("rust", symbols.clone());
        cache.insert_symbols("python", symbols);

        assert_eq!(cache.symbol_count("rust"), 1);
        assert_eq!(cache.symbol_count("python"), 1);

        cache.clear_language("rust");
        assert_eq!(cache.symbol_count("rust"), 0);
        assert_eq!(cache.symbol_count("python"), 1);
    }

    #[test]
    fn test_indexing_progress() {
        let cache = WorkspaceSymbolCache::new();

        assert!(!cache.is_indexing("rust"));
        assert_eq!(cache.indexing_progress("rust"), None);

        cache.set_indexing_progress("rust", 50, 100);
        assert!(cache.is_indexing("rust"));
        assert_eq!(cache.indexing_progress("rust"), Some((50, 100)));

        // Inserting symbols clears indexing progress
        cache.insert_symbols("rust", vec![]);
        assert!(!cache.is_indexing("rust"));
        assert_eq!(cache.indexing_progress("rust"), None);
    }

    #[test]
    fn test_query_sorting() {
        let cache = WorkspaceSymbolCache::new();

        let symbols = vec![
            cached_symbol("test_long_function_name", "Function", "test.rs", 10, 0),
            cached_symbol("test", "Function", "test.rs", 20, 0),
            cached_symbol("my_test", "Function", "test.rs", 30, 0),
        ];

        cache.insert_symbols("rust", symbols);

        let results = cache.query_symbols("test", Some("rust"));
        assert_eq!(results.len(), 3);
        // Exact prefix matches should come first
        assert_eq!(results[0].name, "test");
        // Then sorted by length - but "my_test" and "test_long_function_name" both contain "test"
        // so we just verify "test" is first
    }

    #[test]
    fn extracts_named_ts_js_exports() {
        let root = PathBuf::from("/repo");
        let file = root.join("src/utils/math.ts");
        let symbols = extract_ts_js_exports_from_text(
            &file,
            &root,
            "export function sum() {}\nexport const PI = 3.14\nexport { localName as publicName };\nexport default function ignored() {}\n",
        );

        let names: Vec<_> = symbols.iter().map(|symbol| symbol.name.as_str()).collect();
        assert_eq!(names, vec!["sum", "PI", "publicName"]);
        assert!(symbols.iter().all(|symbol| symbol.export_kind.as_deref() == Some("named")));
        assert!(symbols.iter().all(|symbol| symbol.import_path.as_deref() == Some("src/utils/math")));
    }

    #[test]
    fn upsert_file_symbols_replaces_only_that_file() {
        let cache = WorkspaceSymbolCache::new();
        let first = cached_symbol("first", "Function", "a.ts", 0, 0);
        let mut stale = cached_symbol("stale", "Function", "b.ts", 0, 0);
        stale.source_path = Some(PathBuf::from("b.ts"));
        cache.insert_symbols("typescript", vec![first.clone(), stale]);

        let mut fresh = cached_symbol("fresh", "Function", "b.ts", 0, 0);
        fresh.source_path = Some(PathBuf::from("b.ts"));
        cache.upsert_file_symbols("typescript", Path::new("b.ts"), vec![fresh]);

        let results = cache.query_symbols("", Some("typescript"));
        assert!(results.iter().any(|symbol| symbol.name == "first"));
        assert!(results.iter().any(|symbol| symbol.name == "fresh"));
        assert!(!results.iter().any(|symbol| symbol.name == "stale"));
    }

    #[test]
    fn workspace_export_index_skips_node_modules() {
        let root = std::env::temp_dir().join(format!(
            "netherize_symbol_cache_{}",
            std::process::id()
        ));
        let src_dir = root.join("src");
        let node_dir = root.join("node_modules/pkg");
        std::fs::create_dir_all(&src_dir).expect("create src");
        std::fs::create_dir_all(&node_dir).expect("create node_modules");
        std::fs::write(src_dir.join("api.ts"), "export function connect() {}\n")
            .expect("write src export");
        std::fs::write(node_dir.join("index.ts"), "export function ignored() {}\n")
            .expect("write node export");

        let symbols = index_ts_js_workspace_exports(&root);

        assert!(symbols.iter().any(|symbol| symbol.name == "connect"));
        assert!(!symbols.iter().any(|symbol| symbol.name == "ignored"));
        let _ = std::fs::remove_dir_all(root);
    }
}
