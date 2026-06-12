use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};

const MAX_TS_JS_PACKAGE_EXPORT_PACKAGES: usize = 256;

/// A workspace-wide symbol cache for fast import/completion suggestions.
/// Stores symbols indexed from LSP `workspace/symbol` requests and local TS/JS
/// project source.
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

/// A cached symbol from workspace/symbol response or local source indexing.
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
    pub callable: Option<bool>,
    pub has_parameters: Option<bool>,
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
            inner
                .symbols_by_language
                .insert(language_id.to_string(), symbols);
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
                let source = symbol
                    .source_path
                    .as_deref()
                    .unwrap_or(symbol.file_path.as_path());
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
            Some(lang) => inner
                .symbols_by_language
                .keys()
                .filter(|k| k.as_str() == lang)
                .collect(),
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
            inner
                .indexing_progress
                .insert(language_id.to_string(), (current, total));
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

pub fn index_ts_js_workspace_exports(workspace_root: &Path) -> Vec<CachedSymbol> {
    let mut files = Vec::new();
    collect_ts_js_files(workspace_root, &mut files);

    let mut symbols = Vec::new();
    let mut seen = HashSet::new();
    for file_path in files {
        let Ok(text) = fs::read_to_string(&file_path) else {
            continue;
        };
        for symbol in extract_ts_js_exports_from_text(&file_path, workspace_root, &text) {
            push_unique_cached_symbol(&mut symbols, &mut seen, symbol);
        }
    }
    for symbol in index_ts_js_package_exports(workspace_root) {
        push_unique_cached_symbol(&mut symbols, &mut seen, symbol);
    }
    symbols
}

fn push_unique_cached_symbol(
    symbols: &mut Vec<CachedSymbol>,
    seen: &mut HashSet<(String, PathBuf, Option<String>, u32)>,
    symbol: CachedSymbol,
) {
    let key = (
        symbol.name.clone(),
        symbol.file_path.clone(),
        symbol.container_name.clone(),
        symbol.line,
    );
    if seen.insert(key) {
        symbols.push(symbol);
    }
}

fn index_ts_js_package_exports(workspace_root: &Path) -> Vec<CachedSymbol> {
    let mut package_dirs = Vec::new();
    collect_node_package_dirs(
        &workspace_root.join("node_modules"),
        &mut package_dirs,
        MAX_TS_JS_PACKAGE_EXPORT_PACKAGES,
    );

    let mut symbols = Vec::new();
    for (package_name, package_dir) in package_dirs {
        let Some(type_entry_path) = package_type_entry_path(&package_dir) else {
            continue;
        };
        let Ok(text) = fs::read_to_string(&type_entry_path) else {
            continue;
        };

        let mut package_symbols =
            extract_ts_js_exports_from_text(&type_entry_path, &package_dir, &text)
                .into_iter()
                .filter(|symbol| symbol.export_kind.is_some())
                .map(|mut symbol| {
                    symbol.source_path = Some(type_entry_path.clone());
                    symbol.file_path = type_entry_path.clone();
                    symbol.import_path = Some(package_name.clone());
                    symbol
                })
                .collect::<Vec<_>>();

        if !package_symbols
            .iter()
            .any(|symbol| symbol.export_kind.as_deref() == Some("default"))
        {
            if let Some(default_symbol) =
                package_default_export_symbol(&type_entry_path, &package_name, &text)
            {
                package_symbols.push(default_symbol);
            }
        }

        symbols.extend(package_symbols);
    }
    symbols
}

fn collect_node_package_dirs(
    node_modules: &Path,
    out: &mut Vec<(String, PathBuf)>,
    max_packages: usize,
) {
    let Ok(entries) = fs::read_dir(node_modules) else {
        return;
    };
    for entry in entries.flatten() {
        if out.len() >= max_packages {
            return;
        }
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        if !file_type.is_dir() {
            continue;
        }
        let package_dir = entry.path();
        let Some(name) = package_dir.file_name().and_then(|value| value.to_str()) else {
            continue;
        };
        if name.starts_with('.') {
            continue;
        }
        if name.starts_with('@') {
            collect_scoped_node_package_dirs(name, &package_dir, out, max_packages);
            continue;
        }
        out.push((name.to_string(), package_dir));
    }
}

fn collect_scoped_node_package_dirs(
    scope: &str,
    scope_dir: &Path,
    out: &mut Vec<(String, PathBuf)>,
    max_packages: usize,
) {
    let Ok(entries) = fs::read_dir(scope_dir) else {
        return;
    };
    for entry in entries.flatten() {
        if out.len() >= max_packages {
            return;
        }
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        if !file_type.is_dir() {
            continue;
        }
        let package_dir = entry.path();
        let Some(name) = package_dir.file_name().and_then(|value| value.to_str()) else {
            continue;
        };
        if name.starts_with('.') {
            continue;
        }
        out.push((format!("{scope}/{name}"), package_dir));
    }
}

fn package_type_entry_path(package_dir: &Path) -> Option<PathBuf> {
    let package_json_path = package_dir.join("package.json");
    if let Ok(text) = fs::read_to_string(&package_json_path) {
        if let Ok(json) = serde_json::from_str::<serde_json::Value>(&text) {
            if let Some(entry) = package_type_entry_from_json(&json)
                .and_then(|entry| resolve_package_entry_path(package_dir, &entry))
            {
                return Some(entry);
            }
        }
    }
    let fallback = package_dir.join("index.d.ts");
    fallback.is_file().then_some(fallback)
}

fn package_type_entry_from_json(json: &serde_json::Value) -> Option<String> {
    json.get("types")
        .and_then(serde_json::Value::as_str)
        .or_else(|| json.get("typings").and_then(serde_json::Value::as_str))
        .map(str::to_string)
        .or_else(|| json.get("exports").and_then(export_types_entry))
}

fn export_types_entry(value: &serde_json::Value) -> Option<String> {
    if let Some(path) = value.as_str() {
        return path.ends_with(".d.ts").then(|| path.to_string());
    }
    if let Some(items) = value.as_array() {
        return items.iter().find_map(export_types_entry);
    }
    let object = value.as_object()?;
    if let Some(root) = object.get(".") {
        if let Some(entry) = export_types_entry(root) {
            return Some(entry);
        }
    }
    for key in ["types", "typings"] {
        if let Some(entry) = object.get(key).and_then(serde_json::Value::as_str) {
            return Some(entry.to_string());
        }
    }
    for key in ["import", "require", "default"] {
        if let Some(entry) = object.get(key).and_then(export_types_entry) {
            return Some(entry);
        }
    }
    None
}

fn resolve_package_entry_path(package_dir: &Path, entry: &str) -> Option<PathBuf> {
    let normalized = entry.trim_start_matches("./");
    if normalized.is_empty() || normalized.starts_with("../") || normalized.contains("/../") {
        return None;
    }
    let path = package_dir.join(normalized);
    if path.is_file() {
        return Some(path);
    }
    if path.is_dir() {
        let index = path.join("index.d.ts");
        if index.is_file() {
            return Some(index);
        }
    }
    None
}

fn package_default_export_symbol(
    type_entry_path: &Path,
    package_name: &str,
    text: &str,
) -> Option<CachedSymbol> {
    let export_name = package_default_export_name(text)?;
    let (kind, line, character) = package_declared_symbol_position(text, &export_name)
        .unwrap_or_else(|| ("Variable".to_string(), 0, 0));
    let (callable, has_parameters) = ts_js_symbol_call_metadata(&kind, &export_name, text);
    Some(CachedSymbol {
        name: export_name,
        kind,
        container_name: None,
        file_path: type_entry_path.to_path_buf(),
        line,
        character,
        source_path: Some(type_entry_path.to_path_buf()),
        import_path: Some(package_name.to_string()),
        export_kind: Some("default".to_string()),
        callable,
        has_parameters,
    })
}

fn package_default_export_name(text: &str) -> Option<String> {
    for line in text.lines() {
        let trimmed = line.trim_start();
        if let Some(rest) = strip_leading_word(trimmed, "export")
            .and_then(|rest| strip_leading_word(rest.trim_start(), "default"))
        {
            if let Some(name) = read_identifier(rest.trim_start()) {
                return Some(name);
            }
        }
        if let Some(rest) = strip_leading_word(trimmed, "export")
            .and_then(|rest| rest.trim_start().strip_prefix('='))
        {
            if let Some(name) = read_identifier(rest.trim_start()) {
                return Some(name);
            }
        }
    }
    None
}

fn package_declared_symbol_position(text: &str, symbol_name: &str) -> Option<(String, u32, u32)> {
    let mut in_block_comment = false;
    for (line_idx, raw_line) in text.lines().enumerate() {
        let code_line = strip_ts_js_comments(raw_line, &mut in_block_comment);
        let trimmed = code_line.trim_start();
        let (rest, _) = strip_declaration_modifiers(trimmed);
        for (keyword, kind) in [
            ("function", "Function"),
            ("class", "Class"),
            ("interface", "Interface"),
            ("type", "Type"),
        ] {
            if let Some((name, _)) = named_after_keyword(rest, keyword, kind) {
                if name == symbol_name {
                    return Some((
                        kind.to_string(),
                        line_idx as u32,
                        raw_line.find(symbol_name).unwrap_or(0) as u32,
                    ));
                }
            }
        }
        for keyword in ["const", "let", "var"] {
            let Some(after_keyword) = strip_leading_word(rest, keyword) else {
                continue;
            };
            for declaration in parse_variable_declarations(after_keyword, "Variable") {
                if declaration.name == symbol_name {
                    return Some((
                        declaration.kind,
                        line_idx as u32,
                        raw_line.find(symbol_name).unwrap_or(0) as u32,
                    ));
                }
            }
        }
    }
    None
}

pub fn extract_ts_js_exports_from_text(
    file_path: &Path,
    workspace_root: &Path,
    text: &str,
) -> Vec<CachedSymbol> {
    let import_path = workspace_relative_module_path(workspace_root, file_path);
    let mut symbols = Vec::new();
    let mut seen = HashSet::new();
    let mut contexts: Vec<TsJsIndexContext> = Vec::new();
    let mut brace_depth = 0usize;
    let mut in_block_comment = false;
    let mut accumulating_export: Option<(ExportType, String, usize)> = None;

    for (line_idx, raw_line) in text.lines().enumerate() {
        let code_line = strip_ts_js_comments(raw_line, &mut in_block_comment);
        let trimmed = code_line.trim_start();
        while contexts
            .last()
            .is_some_and(|context| brace_depth < context.body_depth)
        {
            contexts.pop();
        }

        if let Some((export_type, mut acc, start_line_idx)) = accumulating_export.take() {
            acc.push('\n');
            acc.push_str(&code_line);
            if code_line.contains('}') {
                let names = extract_names_from_braces(&acc);
                let export_kind = match export_type {
                    ExportType::ESModule => Some("named"),
                    ExportType::CommonJS => Some("named"),
                };
                for name in names {
                    let symbol = cached_ts_js_symbol(
                        name,
                        "Variable".to_string(),
                        None,
                        file_path,
                        raw_line,
                        line_idx,
                        import_path.as_deref(),
                        export_kind,
                    );
                    let key = (
                        symbol.name.clone(),
                        symbol.file_path.clone(),
                        symbol.container_name.clone(),
                        symbol.line,
                    );
                    if seen.insert(key) {
                        symbols.push(symbol);
                    }
                }
            } else {
                accumulating_export = Some((export_type, acc, start_line_idx));
            }
            brace_depth = brace_depth_after_line(brace_depth, &code_line);
            continue;
        }

        if trimmed.is_empty() {
            brace_depth = brace_depth_after_line(brace_depth, &code_line);
            continue;
        }

        // Check if the current line starts a multi-line export block
        let (rest, export_status) = strip_declaration_modifiers(trimmed);
        if export_status == ExportStatus::Named && rest.starts_with('{') && !rest.contains('}') {
            accumulating_export = Some((ExportType::ESModule, rest.to_string(), line_idx));
            brace_depth = brace_depth_after_line(brace_depth, &code_line);
            continue;
        }

        let is_cjs_start = (trimmed.starts_with("module.exports")
            || trimmed.starts_with("exports"))
            && trimmed.contains('{')
            && !trimmed.contains('}');
        if is_cjs_start {
            accumulating_export = Some((ExportType::CommonJS, trimmed.to_string(), line_idx));
            brace_depth = brace_depth_after_line(brace_depth, &code_line);
            continue;
        }

        let current_context = contexts.last();
        let (candidates, pending_context) = match current_context {
            Some(context) if context.kind == TsJsContextKind::Function => (Vec::new(), None),
            Some(context) if context.kind == TsJsContextKind::Class => {
                member_candidates_from_line(trimmed, raw_line, line_idx, file_path, context)
            }
            Some(context) if context.kind == TsJsContextKind::Interface => {
                member_candidates_from_line(trimmed, raw_line, line_idx, file_path, context)
            }
            Some(context) if context.kind == TsJsContextKind::Type => {
                member_candidates_from_line(trimmed, raw_line, line_idx, file_path, context)
            }
            Some(context) if context.kind == TsJsContextKind::Enum => {
                enum_member_candidates_from_line(trimmed, raw_line, line_idx, file_path, context)
            }
            Some(context) if context.kind == TsJsContextKind::Object => {
                member_candidates_from_line(trimmed, raw_line, line_idx, file_path, context)
            }
            Some(context) if context.kind == TsJsContextKind::Namespace => {
                declaration_candidates_from_line(
                    trimmed,
                    raw_line,
                    line_idx,
                    file_path,
                    import_path.as_deref(),
                    Some(context.name.as_str()),
                    false,
                )
            }
            _ => declaration_candidates_from_line(
                trimmed,
                raw_line,
                line_idx,
                file_path,
                import_path.as_deref(),
                None,
                true,
            ),
        };

        for symbol in candidates {
            let key = (
                symbol.name.clone(),
                symbol.file_path.clone(),
                symbol.container_name.clone(),
                symbol.line,
            );
            if !seen.insert(key) {
                continue;
            }
            symbols.push(symbol);
        }

        // ── CommonJS exports ──────────────────────────────────────────────
        let cjs_candidates = extract_commonjs_candidates_from_line(
            trimmed,
            raw_line,
            line_idx,
            file_path,
            import_path.as_deref(),
        );
        for symbol in cjs_candidates {
            let key = (
                symbol.name.clone(),
                symbol.file_path.clone(),
                symbol.container_name.clone(),
                symbol.line,
            );
            if seen.insert(key) {
                symbols.push(symbol);
            }
        }

        let next_brace_depth = brace_depth_after_line(brace_depth, &code_line);
        if let Some(pending_context) = pending_context {
            if next_brace_depth > brace_depth {
                contexts.push(TsJsIndexContext {
                    name: pending_context.name,
                    kind: pending_context.kind,
                    body_depth: brace_depth.saturating_add(1),
                });
            }
        }
        brace_depth = next_brace_depth;
    }

    symbols
}

fn extract_commonjs_candidates_from_line(
    trimmed: &str,
    raw_line: &str,
    line_idx: usize,
    file_path: &Path,
    import_path: Option<&str>,
) -> Vec<CachedSymbol> {
    let mut candidates = Vec::new();

    // Pattern 1: module.exports = { name1, name2 } or exports = { name1, name2 }
    if (trimmed.starts_with("module.exports") || trimmed.starts_with("exports"))
        && trimmed.contains('{')
    {
        if let Some(open) = trimmed.find('{') {
            if let Some(close) = trimmed[open..].find('}').map(|idx| open + idx) {
                let list = &trimmed[open + 1..close];
                for part in list.split(',') {
                    let name = part.trim();
                    if !name.is_empty() && is_valid_ts_js_identifier(name) {
                        candidates.push(cached_ts_js_symbol(
                            name.to_string(),
                            "Variable".to_string(),
                            None,
                            file_path,
                            raw_line,
                            line_idx,
                            import_path,
                            Some("named"),
                        ));
                    }
                }
            }
        }
    }
    // Pattern 2: exports.foo = ... or module.exports.foo = ...
    else if trimmed.starts_with("exports.") || trimmed.starts_with("module.exports.") {
        let rest = if trimmed.starts_with("exports.") {
            trimmed.strip_prefix("exports.").unwrap()
        } else {
            trimmed.strip_prefix("module.exports.").unwrap()
        };
        if let Some(eq_idx) = rest.find('=') {
            let name = rest[..eq_idx].trim();
            if is_valid_ts_js_identifier(name) {
                candidates.push(cached_ts_js_symbol(
                    name.to_string(),
                    "Variable".to_string(),
                    None,
                    file_path,
                    raw_line,
                    line_idx,
                    import_path,
                    Some("named"),
                ));
            }
        }
    }
    // Pattern 3: module.exports = TelegramClient (default assignment)
    else if let Some(after_exports) = trimmed.strip_prefix("module.exports") {
        let rest = after_exports.trim_start();
        if let Some(after_eq) = rest.strip_prefix('=') {
            let name = after_eq.trim().trim_end_matches(';').trim();
            if is_valid_ts_js_identifier(name) {
                candidates.push(cached_ts_js_symbol(
                    name.to_string(),
                    "Class".to_string(),
                    None,
                    file_path,
                    raw_line,
                    line_idx,
                    import_path,
                    Some("default"),
                ));
            }
        }
    }

    candidates
}

fn is_valid_ts_js_identifier(s: &str) -> bool {
    let mut chars = s.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    if !first.is_ascii_alphabetic() && first != '_' && first != '$' {
        return false;
    }
    chars.all(|ch| ch.is_ascii_alphanumeric() || ch == '_' || ch == '$')
}

fn collect_ts_js_files(root: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(root) else {
        return;
    };
    for entry in entries.flatten() {
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
            collect_ts_js_files(&path, out);
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
    let mut value = strip_ts_js_extension(relative)
        .to_string_lossy()
        .replace('\\', "/");
    if value.ends_with("/index") {
        value.truncate(value.len().saturating_sub("/index".len()));
    }
    if value.is_empty() { None } else { Some(value) }
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ExportType {
    ESModule,
    CommonJS,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TsJsContextKind {
    Class,
    Interface,
    Type,
    Enum,
    Object,
    Function,
    Namespace,
}

#[derive(Debug, Clone)]
struct TsJsIndexContext {
    name: String,
    kind: TsJsContextKind,
    body_depth: usize,
}

#[derive(Debug, Clone)]
struct PendingTsJsContext {
    name: String,
    kind: TsJsContextKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ExportStatus {
    None,
    Named,
    Default,
}

fn declaration_candidates_from_line(
    line: &str,
    raw_line: &str,
    line_idx: usize,
    file_path: &Path,
    import_path: Option<&str>,
    container_name: Option<&str>,
    allow_file_named_export: bool,
) -> (Vec<CachedSymbol>, Option<PendingTsJsContext>) {
    let (rest, export_status) = strip_declaration_modifiers(line);
    let is_file_importable_export = allow_file_named_export
        && matches!(export_status, ExportStatus::Named | ExportStatus::Default);
    let symbol_import_path = is_file_importable_export.then_some(import_path).flatten();
    let symbol_export_kind = match export_status {
        ExportStatus::Named if allow_file_named_export => Some("named"),
        ExportStatus::Default if allow_file_named_export => Some("default"),
        _ => None,
    };

    if export_status == ExportStatus::Named && rest.starts_with('{') {
        let symbols = parse_export_list(rest, "Symbol")
            .into_iter()
            .map(|(name, kind)| {
                cached_ts_js_symbol(
                    name,
                    kind,
                    container_name,
                    file_path,
                    raw_line,
                    line_idx,
                    symbol_import_path,
                    symbol_export_kind,
                )
            })
            .collect();
        return (symbols, None);
    }

    if let Some(after_type) = strip_leading_word(rest, "type") {
        if after_type.trim_start().starts_with('{') {
            let symbols = parse_export_list(after_type.trim_start(), "Type")
                .into_iter()
                .map(|(name, kind)| {
                    cached_ts_js_symbol(
                        name,
                        kind,
                        container_name,
                        file_path,
                        raw_line,
                        line_idx,
                        symbol_import_path,
                        symbol_export_kind,
                    )
                })
                .collect();
            return (symbols, None);
        }
        if let Some((name, kind)) = named_after_keyword(after_type, "", "Type") {
            let symbol = cached_ts_js_symbol(
                name.clone(),
                kind,
                container_name,
                file_path,
                raw_line,
                line_idx,
                symbol_import_path,
                symbol_export_kind,
            );
            return (
                vec![symbol],
                line_opens_block(rest).then_some(PendingTsJsContext {
                    name,
                    kind: TsJsContextKind::Type,
                }),
            );
        }
    }

    let function_rest = strip_leading_word(rest, "async")
        .map(str::trim_start)
        .unwrap_or(rest);
    if let Some((name, kind)) = named_after_keyword(function_rest, "function", "Function") {
        let symbol = cached_ts_js_symbol(
            name.clone(),
            kind,
            container_name,
            file_path,
            raw_line,
            line_idx,
            symbol_import_path,
            symbol_export_kind,
        );
        return (
            vec![symbol],
            line_opens_block(rest).then_some(PendingTsJsContext {
                name,
                kind: TsJsContextKind::Function,
            }),
        );
    }

    for (keyword, kind, context_kind) in [
        ("class", "Class", TsJsContextKind::Class),
        ("interface", "Interface", TsJsContextKind::Interface),
        ("enum", "Enum", TsJsContextKind::Enum),
        ("namespace", "Module", TsJsContextKind::Namespace),
        ("module", "Module", TsJsContextKind::Namespace),
    ] {
        if let Some((name, symbol_kind)) = named_after_keyword(rest, keyword, kind) {
            let symbol = cached_ts_js_symbol(
                name.clone(),
                symbol_kind,
                container_name,
                file_path,
                raw_line,
                line_idx,
                symbol_import_path,
                symbol_export_kind,
            );
            return (
                vec![symbol],
                line_opens_block(rest).then_some(PendingTsJsContext {
                    name,
                    kind: context_kind,
                }),
            );
        }
    }

    for (keyword, default_kind) in [
        ("const", "Constant"),
        ("let", "Variable"),
        ("var", "Variable"),
    ] {
        let Some(after_keyword) = strip_leading_word(rest, keyword) else {
            continue;
        };
        let declarations = parse_variable_declarations(after_keyword, default_kind);
        let mut symbols = Vec::new();
        let mut pending_context = None;
        for declaration in declarations {
            symbols.push(cached_ts_js_symbol(
                declaration.name.clone(),
                declaration.kind,
                container_name,
                file_path,
                raw_line,
                line_idx,
                symbol_import_path,
                symbol_export_kind,
            ));
            if pending_context.is_none() && declaration.opens_object && line_opens_block(rest) {
                pending_context = Some(PendingTsJsContext {
                    name: declaration.name,
                    kind: TsJsContextKind::Object,
                });
            }
        }
        return (symbols, pending_context);
    }

    (Vec::new(), None)
}

fn member_candidates_from_line(
    line: &str,
    raw_line: &str,
    line_idx: usize,
    file_path: &Path,
    context: &TsJsIndexContext,
) -> (Vec<CachedSymbol>, Option<PendingTsJsContext>) {
    let mut rest = strip_member_modifiers(line).trim_start();
    if rest.starts_with('}') || rest.starts_with('{') || rest.starts_with("...") {
        return (Vec::new(), None);
    }
    if let Some(after) = strip_leading_word(rest, "constructor") {
        if after.trim_start().starts_with('(') {
            return (
                vec![cached_ts_js_symbol(
                    "constructor".to_string(),
                    "Constructor".to_string(),
                    Some(context.name.as_str()),
                    file_path,
                    raw_line,
                    line_idx,
                    None,
                    None,
                )],
                line_opens_block(rest).then_some(PendingTsJsContext {
                    name: "constructor".to_string(),
                    kind: TsJsContextKind::Function,
                }),
            );
        }
    }
    if let Some(after_get) = strip_leading_word(rest, "get") {
        rest = after_get.trim_start();
    } else if let Some(after_set) = strip_leading_word(rest, "set") {
        rest = after_set.trim_start();
    }

    let Some((name, after_name)) = read_identifier_with_rest(rest) else {
        return (Vec::new(), None);
    };
    if is_ts_js_reserved_member_leader(&name) {
        return (Vec::new(), None);
    }
    let after_name = after_name.trim_start();
    let after_optional = after_name
        .strip_prefix('?')
        .or_else(|| after_name.strip_prefix('!'))
        .unwrap_or(after_name)
        .trim_start();

    let is_member = after_optional.starts_with('(')
        || after_optional.starts_with(':')
        || after_optional.starts_with('=')
        || after_optional.starts_with(',')
        || after_optional.starts_with(';')
        || after_optional.is_empty();
    if !is_member {
        return (Vec::new(), None);
    }

    let kind = if after_optional.starts_with('(') {
        "Method"
    } else if context.kind == TsJsContextKind::Class {
        "Field"
    } else {
        "Property"
    };
    (
        vec![cached_ts_js_symbol(
            name.clone(),
            kind.to_string(),
            Some(context.name.as_str()),
            file_path,
            raw_line,
            line_idx,
            None,
            None,
        )],
        line_opens_block(rest)
            .then_some(PendingTsJsContext {
                name,
                kind: TsJsContextKind::Function,
            })
            .filter(|_| {
                after_optional.starts_with('(')
                    || after_optional.contains("=>")
                    || after_optional.contains("function")
            }),
    )
}

fn enum_member_candidates_from_line(
    line: &str,
    raw_line: &str,
    line_idx: usize,
    file_path: &Path,
    context: &TsJsIndexContext,
) -> (Vec<CachedSymbol>, Option<PendingTsJsContext>) {
    let line = line.trim_start_matches('}').trim();
    let mut symbols = Vec::new();
    for part in split_top_level_commas(line) {
        let part = part.trim();
        if part.is_empty() || part.starts_with("//") {
            continue;
        }
        let Some(name) = read_identifier(part) else {
            continue;
        };
        symbols.push(cached_ts_js_symbol(
            name,
            "EnumMember".to_string(),
            Some(context.name.as_str()),
            file_path,
            raw_line,
            line_idx,
            None,
            None,
        ));
    }
    (symbols, None)
}

fn cached_ts_js_symbol(
    name: String,
    kind: String,
    container_name: Option<&str>,
    file_path: &Path,
    raw_line: &str,
    line_idx: usize,
    import_path: Option<&str>,
    export_kind: Option<&str>,
) -> CachedSymbol {
    let character = raw_line.find(&name).unwrap_or(0) as u32;
    let (callable, has_parameters) = ts_js_symbol_call_metadata(&kind, &name, raw_line);
    CachedSymbol {
        name,
        kind,
        container_name: container_name.map(str::to_string),
        file_path: file_path.to_path_buf(),
        line: line_idx as u32,
        character,
        source_path: Some(file_path.to_path_buf()),
        import_path: import_path.map(str::to_string),
        export_kind: export_kind.map(str::to_string),
        callable,
        has_parameters,
    }
}

fn ts_js_symbol_call_metadata(
    kind: &str,
    name: &str,
    raw_text: &str,
) -> (Option<bool>, Option<bool>) {
    if !matches!(kind, "Function" | "Method" | "Constructor") {
        return (Some(false), None);
    }
    if is_ts_js_accessor_line(name, raw_text) {
        return (Some(false), None);
    }
    (
        Some(true),
        infer_ts_js_callable_has_parameters(name, raw_text),
    )
}

fn is_ts_js_accessor_line(name: &str, raw_text: &str) -> bool {
    let trimmed = raw_text.trim_start();
    for keyword in ["get", "set"] {
        if let Some(rest) = strip_leading_word(trimmed, keyword)
            && read_identifier(rest.trim_start()).as_deref() == Some(name)
        {
            return true;
        }
    }
    false
}

fn infer_ts_js_callable_has_parameters(name: &str, raw_text: &str) -> Option<bool> {
    if let Some(has_parameters) = ts_js_parameters_after_named_call(raw_text, name) {
        return Some(has_parameters);
    }
    ts_js_initializer_after_identifier(raw_text, name)
        .and_then(ts_js_function_initializer_has_parameters)
}

fn ts_js_parameters_after_named_call(raw_text: &str, name: &str) -> Option<bool> {
    for (idx, _) in raw_text.match_indices(name) {
        if !identifier_match_at(raw_text, idx, name) {
            continue;
        }
        let mut rest = &raw_text[idx + name.len()..];
        rest = rest.trim_start();
        rest = rest
            .strip_prefix('?')
            .or_else(|| rest.strip_prefix('!'))
            .unwrap_or(rest)
            .trim_start();
        rest = strip_leading_type_arguments(rest)
            .unwrap_or(rest)
            .trim_start();
        if let Some(content) = parenthesized_prefix_content(rest) {
            return Some(!content.trim().is_empty());
        }
    }
    None
}

fn ts_js_initializer_after_identifier<'a>(raw_text: &'a str, name: &str) -> Option<&'a str> {
    for (idx, _) in raw_text.match_indices(name) {
        if !identifier_match_at(raw_text, idx, name) {
            continue;
        }
        let rest = &raw_text[idx + name.len()..];
        if let Some(initializer) = top_level_initializer(rest) {
            return Some(initializer);
        }
    }
    None
}

fn ts_js_function_initializer_has_parameters(initializer: &str) -> Option<bool> {
    let mut value = initializer.trim_start();
    if let Some(rest) = strip_leading_word(value, "async") {
        value = rest.trim_start();
    }
    if let Some(rest) = strip_leading_word(value, "function") {
        let rest = rest.trim_start();
        let rest = read_identifier_with_rest(rest)
            .map(|(_, after_name)| after_name.trim_start())
            .unwrap_or(rest);
        return parenthesized_prefix_content(rest).map(|content| !content.trim().is_empty());
    }
    if let Some(content) = parenthesized_prefix_content(value) {
        return Some(!content.trim().is_empty());
    }
    let arrow = value.find("=>")?;
    let before_arrow = value[..arrow].trim();
    read_identifier(before_arrow).map(|_| true)
}

fn identifier_match_at(text: &str, idx: usize, name: &str) -> bool {
    let before_ok = text[..idx]
        .chars()
        .next_back()
        .is_none_or(|ch| !is_ts_js_identifier_char(ch));
    let after_ok = text[idx + name.len()..]
        .chars()
        .next()
        .is_none_or(|ch| !is_ts_js_identifier_char(ch));
    before_ok && after_ok
}

fn is_ts_js_identifier_char(ch: char) -> bool {
    ch.is_ascii_alphanumeric() || ch == '_' || ch == '$'
}

fn strip_leading_type_arguments(text: &str) -> Option<&str> {
    let trimmed = text.trim_start();
    if !trimmed.starts_with('<') {
        return None;
    }
    let close = find_matching_delimiter(trimmed, 0, '<', '>')?;
    Some(&trimmed[close + 1..])
}

fn parenthesized_prefix_content(text: &str) -> Option<&str> {
    let trimmed = text.trim_start();
    if !trimmed.starts_with('(') {
        return None;
    }
    let close = find_matching_delimiter(trimmed, 0, '(', ')')?;
    Some(&trimmed[1..close])
}

fn find_matching_delimiter(
    text: &str,
    open: usize,
    open_ch: char,
    close_ch: char,
) -> Option<usize> {
    let mut depth = 0usize;
    let mut quote = None;
    let mut escape = false;
    for (idx, ch) in text[open..].char_indices() {
        let absolute = open + idx;
        if let Some(quote_ch) = quote {
            if escape {
                escape = false;
                continue;
            }
            if ch == '\\' {
                escape = true;
                continue;
            }
            if ch == quote_ch {
                quote = None;
            }
            continue;
        }
        match ch {
            '\'' | '"' | '`' => quote = Some(ch),
            _ if ch == open_ch => depth = depth.saturating_add(1),
            _ if ch == close_ch => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    return Some(absolute);
                }
            }
            _ => {}
        }
    }
    None
}

#[derive(Debug)]
struct VariableDeclaration {
    name: String,
    kind: String,
    opens_object: bool,
}

fn parse_variable_declarations(rest: &str, default_kind: &str) -> Vec<VariableDeclaration> {
    split_top_level_commas(rest)
        .into_iter()
        .filter_map(|part| {
            let part = part.trim_start();
            if part.starts_with('{') || part.starts_with('[') {
                return None;
            }
            let name = read_identifier(part)?;
            let initializer = top_level_initializer(part);
            let kind = initializer
                .map(|value| variable_kind_for_initializer(value, default_kind))
                .unwrap_or(default_kind)
                .to_string();
            let opens_object = initializer
                .map(|value| value.trim_start().starts_with('{'))
                .unwrap_or(false);
            Some(VariableDeclaration {
                name,
                kind,
                opens_object,
            })
        })
        .collect()
}

fn variable_kind_for_initializer(initializer: &str, default_kind: &str) -> &'static str {
    let initializer = initializer.trim_start();
    if initializer.starts_with("async ")
        || initializer.starts_with("function")
        || initializer.contains("=>")
    {
        "Function"
    } else if initializer.starts_with("class") {
        "Class"
    } else if default_kind == "Constant" {
        "Constant"
    } else {
        "Variable"
    }
}

fn top_level_initializer(part: &str) -> Option<&str> {
    let mut depth = 0usize;
    let mut quote = None;
    let mut escape = false;
    for (idx, ch) in part.char_indices() {
        if let Some(quote_char) = quote {
            if escape {
                escape = false;
                continue;
            }
            if ch == '\\' {
                escape = true;
                continue;
            }
            if ch == quote_char {
                quote = None;
            }
            continue;
        }
        match ch {
            '\'' | '"' | '`' => quote = Some(ch),
            '(' | '[' | '{' => depth = depth.saturating_add(1),
            ')' | ']' | '}' => depth = depth.saturating_sub(1),
            '=' if depth == 0 => return Some(&part[idx + ch.len_utf8()..]),
            _ => {}
        }
    }
    None
}

fn strip_declaration_modifiers(input: &str) -> (&str, ExportStatus) {
    let mut rest = input.trim_start();
    let mut export_status = ExportStatus::None;
    loop {
        if let Some(after_export) = strip_leading_word(rest, "export") {
            rest = after_export.trim_start();
            if let Some(after_default) = strip_leading_word(rest, "default") {
                export_status = ExportStatus::Default;
                rest = after_default.trim_start();
            } else {
                export_status = ExportStatus::Named;
            }
            continue;
        }
        let mut stripped = false;
        for modifier in ["declare", "abstract"] {
            if let Some(after_modifier) = strip_leading_word(rest, modifier) {
                rest = after_modifier.trim_start();
                stripped = true;
                break;
            }
        }
        if !stripped {
            return (rest, export_status);
        }
    }
}

fn strip_member_modifiers(input: &str) -> &str {
    let mut rest = input.trim_start();
    loop {
        let mut stripped = false;
        for modifier in [
            "public",
            "private",
            "protected",
            "static",
            "readonly",
            "abstract",
            "override",
            "async",
            "accessor",
            "declare",
        ] {
            if let Some(after_modifier) = strip_leading_word(rest, modifier) {
                rest = after_modifier.trim_start();
                stripped = true;
                break;
            }
        }
        if !stripped {
            return rest;
        }
    }
}

fn strip_leading_word<'a>(input: &'a str, word: &str) -> Option<&'a str> {
    let rest = input.strip_prefix(word)?;
    if rest
        .chars()
        .next()
        .is_some_and(|ch| ch.is_ascii_alphanumeric() || ch == '_' || ch == '$')
    {
        return None;
    }
    Some(rest)
}

fn strip_ts_js_comments(line: &str, in_block_comment: &mut bool) -> String {
    let mut out = String::with_capacity(line.len());
    let mut chars = line.char_indices().peekable();
    let mut quote = None;
    let mut escape = false;
    while let Some((idx, ch)) = chars.next() {
        if *in_block_comment {
            if ch == '*' && line[idx + ch.len_utf8()..].starts_with('/') {
                *in_block_comment = false;
                let _ = chars.next();
            }
            continue;
        }
        if let Some(quote_char) = quote {
            out.push(ch);
            if escape {
                escape = false;
                continue;
            }
            if ch == '\\' {
                escape = true;
                continue;
            }
            if ch == quote_char {
                quote = None;
            }
            continue;
        }
        if ch == '/' && line[idx + ch.len_utf8()..].starts_with('/') {
            break;
        }
        if ch == '/' && line[idx + ch.len_utf8()..].starts_with('*') {
            *in_block_comment = true;
            let _ = chars.next();
            continue;
        }
        if matches!(ch, '\'' | '"' | '`') {
            quote = Some(ch);
        }
        out.push(ch);
    }
    out
}

fn brace_depth_after_line(mut depth: usize, line: &str) -> usize {
    let mut quote = None;
    let mut escape = false;
    for ch in line.chars() {
        if let Some(quote_char) = quote {
            if escape {
                escape = false;
                continue;
            }
            if ch == '\\' {
                escape = true;
                continue;
            }
            if ch == quote_char {
                quote = None;
            }
            continue;
        }
        match ch {
            '\'' | '"' | '`' => quote = Some(ch),
            '{' => depth = depth.saturating_add(1),
            '}' => depth = depth.saturating_sub(1),
            _ => {}
        }
    }
    depth
}

fn line_opens_block(line: &str) -> bool {
    brace_depth_after_line(0, line) > 0
}

fn split_top_level_commas(input: &str) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut start = 0usize;
    let mut depth = 0usize;
    let mut quote = None;
    let mut escape = false;
    for (idx, ch) in input.char_indices() {
        if let Some(quote_char) = quote {
            if escape {
                escape = false;
                continue;
            }
            if ch == '\\' {
                escape = true;
                continue;
            }
            if ch == quote_char {
                quote = None;
            }
            continue;
        }
        match ch {
            '\'' | '"' | '`' => quote = Some(ch),
            '(' | '[' | '{' => depth = depth.saturating_add(1),
            ')' | ']' | '}' => depth = depth.saturating_sub(1),
            ',' if depth == 0 => {
                parts.push(&input[start..idx]);
                start = idx + ch.len_utf8();
            }
            _ => {}
        }
    }
    parts.push(&input[start..]);
    parts
}

fn named_after_keyword(rest: &str, keyword: &str, kind: &str) -> Option<(String, String)> {
    let after_keyword = if keyword.is_empty() {
        rest.trim_start()
    } else {
        strip_leading_word(rest, keyword)?.trim_start()
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
            let cleaned = strip_leading_word(cleaned, "type")
                .map(str::trim_start)
                .unwrap_or(cleaned);
            let exported_name = cleaned
                .split_once(" as ")
                .map(|(_, alias)| alias.trim())
                .unwrap_or(cleaned);
            let name = read_identifier(exported_name)?;
            Some((name, kind.to_string()))
        })
        .collect()
}

fn extract_names_from_braces(text: &str) -> Vec<String> {
    let Some(start) = text.find('{') else {
        return Vec::new();
    };
    let Some(end) = text[start..].find('}') else {
        return Vec::new();
    };
    let content = &text[start + 1..start + end];
    content
        .split(',')
        .filter_map(|entry| {
            let cleaned = entry.trim();
            if cleaned.is_empty() {
                return None;
            }
            let cleaned = strip_leading_word(cleaned, "type")
                .map(str::trim_start)
                .unwrap_or(cleaned);
            let exported_name = cleaned
                .split_once(" as ")
                .map(|(_, alias)| alias.trim())
                .unwrap_or(cleaned);
            read_identifier(exported_name)
        })
        .collect()
}

fn read_identifier(input: &str) -> Option<String> {
    read_identifier_with_rest(input).map(|(name, _)| name)
}

fn read_identifier_with_rest(input: &str) -> Option<(String, &str)> {
    let mut out = String::new();
    let mut end = 0usize;
    for (idx, ch) in input.char_indices() {
        if out.is_empty() {
            if ch.is_ascii_alphabetic() || ch == '_' || ch == '$' {
                out.push(ch);
                end = idx + ch.len_utf8();
                continue;
            }
            return None;
        }
        if ch.is_ascii_alphanumeric() || ch == '_' || ch == '$' {
            out.push(ch);
            end = idx + ch.len_utf8();
        } else {
            break;
        }
    }
    (!out.is_empty()).then_some((out, &input[end..]))
}

fn is_ts_js_reserved_member_leader(name: &str) -> bool {
    matches!(
        name,
        "if" | "for"
            | "while"
            | "switch"
            | "catch"
            | "return"
            | "throw"
            | "const"
            | "let"
            | "var"
            | "function"
            | "class"
            | "interface"
            | "type"
            | "enum"
            | "export"
            | "import"
            | "from"
            | "new"
            | "super"
            | "this"
            | "case"
            | "default"
            | "else"
            | "do"
            | "try"
            | "finally"
            | "await"
            | "yield"
    )
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

    fn cached_symbol(
        name: &str,
        kind: &str,
        file: &str,
        line: u32,
        character: u32,
    ) -> CachedSymbol {
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
            callable: Some(matches!(kind, "Function" | "Method" | "Constructor")),
            has_parameters: None,
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
        assert_eq!(names, vec!["sum", "PI", "publicName", "ignored"]);
        let named_exports: Vec<_> = symbols
            .iter()
            .filter(|symbol| symbol.export_kind.as_deref() == Some("named"))
            .map(|symbol| symbol.name.as_str())
            .collect();
        assert_eq!(named_exports, vec!["sum", "PI", "publicName"]);
        assert_eq!(
            symbols
                .iter()
                .find(|symbol| symbol.name == "ignored")
                .and_then(|symbol| symbol.export_kind.as_deref()),
            Some("default")
        );
    }

    #[test]
    fn extracts_commonjs_exports() {
        let root = PathBuf::from("/repo");
        let file = root.join("src/utils/math.js");
        let symbols = extract_ts_js_exports_from_text(
            &file,
            &root,
            "\
module.exports = { sum, PI };
exports.foo = bar;
module.exports.baz = qux;
module.exports = TelegramClient;
",
        );

        let has = |name: &str, kind: &str, export_kind: Option<&str>| {
            symbols.iter().any(|symbol| {
                symbol.name == name
                    && symbol.kind == kind
                    && symbol.export_kind.as_deref() == export_kind
            })
        };

        assert!(has("sum", "Variable", Some("named")));
        assert!(has("PI", "Variable", Some("named")));
        assert!(has("foo", "Variable", Some("named")));
        assert!(has("baz", "Variable", Some("named")));
        assert!(has("TelegramClient", "Class", Some("default")));
    }

    #[test]
    fn extracts_multiline_exports() {
        let root = PathBuf::from("/repo");
        let file = root.join("src/utils/math.js");
        let symbols = extract_ts_js_exports_from_text(
            &file,
            &root,
            "\
module.exports = {
  sum,
  PI,
  wait
};
export {
  foo,
  bar as baz
};
",
        );

        let names: Vec<_> = symbols.iter().map(|symbol| symbol.name.as_str()).collect();
        assert_eq!(names, vec!["sum", "PI", "wait", "foo", "baz"]);
    }

    #[test]
    fn extracts_ts_js_declarations_members_and_methods() {
        let root = PathBuf::from("/repo");
        let file = root.join("src/services/user.ts");
        let symbols = extract_ts_js_exports_from_text(
            &file,
            &root,
            "\
function localHelper() {}
const calculate = () => {}
export class UserService {
  private repo: Repo;
  constructor(repo: Repo) {}
  async createUser(input: UserInput) {}
  get size() { return 1; }
}
interface UserShape {
  id: string;
  load(): Promise<void>;
}
const handlers = {
  onCreate() {},
  onDelete: async () => {},
};
enum Role {
  Admin,
  User = 'user',
}
",
        );

        let has = |name: &str, kind: &str, container: Option<&str>| {
            symbols.iter().any(|symbol| {
                symbol.name == name
                    && symbol.kind == kind
                    && symbol.container_name.as_deref() == container
            })
        };
        let call_shape = |name: &str, container: Option<&str>| {
            symbols
                .iter()
                .find(|symbol| symbol.name == name && symbol.container_name.as_deref() == container)
                .map(|symbol| (symbol.callable, symbol.has_parameters))
        };
        assert!(has("localHelper", "Function", None));
        assert!(has("calculate", "Function", None));
        assert!(has("UserService", "Class", None));
        assert!(has("repo", "Field", Some("UserService")));
        assert!(has("constructor", "Constructor", Some("UserService")));
        assert!(has("createUser", "Method", Some("UserService")));
        assert!(has("size", "Method", Some("UserService")));
        assert!(has("UserShape", "Interface", None));
        assert!(has("id", "Property", Some("UserShape")));
        assert!(has("load", "Method", Some("UserShape")));
        assert!(has("handlers", "Constant", None));
        assert!(has("onCreate", "Method", Some("handlers")));
        assert!(has("onDelete", "Property", Some("handlers")));
        assert!(has("Role", "Enum", None));
        assert!(has("Admin", "EnumMember", Some("Role")));
        assert!(has("User", "EnumMember", Some("Role")));
        assert_eq!(
            call_shape("localHelper", None),
            Some((Some(true), Some(false)))
        );
        assert_eq!(
            call_shape("calculate", None),
            Some((Some(true), Some(false)))
        );
        assert_eq!(
            call_shape("constructor", Some("UserService")),
            Some((Some(true), Some(true)))
        );
        assert_eq!(
            call_shape("createUser", Some("UserService")),
            Some((Some(true), Some(true)))
        );
        assert_eq!(
            call_shape("size", Some("UserService")),
            Some((Some(false), None))
        );
        assert_eq!(
            call_shape("load", Some("UserShape")),
            Some((Some(true), Some(false)))
        );
        assert_eq!(
            symbols
                .iter()
                .find(|symbol| symbol.name == "UserService")
                .and_then(|symbol| symbol.export_kind.as_deref()),
            Some("named")
        );
        assert_eq!(
            symbols
                .iter()
                .find(|symbol| symbol.name == "createUser")
                .and_then(|symbol| symbol.export_kind.as_deref()),
            None
        );
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
        let root =
            std::env::temp_dir().join(format!("netherize_symbol_cache_{}", std::process::id()));
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

    #[test]
    fn workspace_export_index_reads_package_type_entry_exports() {
        let root = std::env::temp_dir().join(format!(
            "netherize_symbol_cache_packages_{}",
            std::process::id()
        ));
        let package_dir = root.join("node_modules/axios");
        std::fs::create_dir_all(&package_dir).expect("create package");
        std::fs::write(
            package_dir.join("package.json"),
            r#"{"name":"axios","types":"index.d.ts"}"#,
        )
        .expect("write package json");
        std::fs::write(
            package_dir.join("index.d.ts"),
            "declare const axios: AxiosStatic;\nexport default axios;\nexport { AxiosError };\n",
        )
        .expect("write package types");

        let symbols = index_ts_js_workspace_exports(&root);

        let axios = symbols
            .iter()
            .find(|symbol| symbol.name == "axios")
            .expect("default package export");
        assert_eq!(axios.import_path.as_deref(), Some("axios"));
        assert_eq!(axios.export_kind.as_deref(), Some("default"));
        let named = symbols
            .iter()
            .find(|symbol| symbol.name == "AxiosError")
            .expect("named package export");
        assert_eq!(named.import_path.as_deref(), Some("axios"));
        assert_eq!(named.export_kind.as_deref(), Some("named"));
        let _ = std::fs::remove_dir_all(root);
    }
}
