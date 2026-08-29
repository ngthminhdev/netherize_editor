//! Scope-aware card snapshot selection (T4).
//!
//! `documentSymbol` arrives as a flat list with `ancestors`; "deepest enclosing"
//! == smallest containing span. Pure `(symbols, line) -> Option<range>` so it is
//! unit-testable with hand-built symbol lists.

use std::path::Path;

use crate::async_runtime::message::LspDocumentSymbol;
use crate::canvas::{BlockOrigin, BlockSnapshot};
use crate::config::theme_config::ThemeConfig;

/// Kinds that ARE a scope of their own: callables, plus declarations — a TS/JS
/// arrow function is reported as `Variable` (`const useFoo = () => …`) or
/// `Property` (`handler = () => …` in a class), a getter as `Property`, a Go
/// `var f = func(){}` as `Variable`. The deepest leaf containing the line wins.
const LEAF_KINDS: [&str; 7] = [
    "Function",
    "Method",
    "Constructor",
    "Constant",
    "Variable",
    "Property",
    "Field",
];
/// Kinds that only CONTAIN scopes. Used as the fallback when no leaf contains
/// the line (a type name, a line between methods). A container-scoped card is
/// row-capped ([`CONTAINER_CARD_ROWS`]) so it doesn't dump the whole body — `=`
/// reveals more.
const CONTAINER_KINDS: [&str; 7] = [
    "Class",
    "Struct",
    "Interface",
    "Enum",
    "Object",
    "Module",
    "Namespace",
];
/// Rows a container-scoped card shows on spawn (its header + first members).
pub(crate) const CONTAINER_CARD_ROWS: usize = 12;

/// The deepest enclosing **definition** symbol whose line range contains `line`:
/// the smallest leaf (function/method/…/variable/property), else the smallest
/// container (class/struct/…). Returns `None` when nothing definition-like
/// contains the line (caller keeps the ±N window). The returned symbol carries
/// both its `range` (for the scope snapshot) and its `name` (for the card title)
/// — a spawned card must show the TARGET's name, not the canvas's focal symbol.
pub(crate) fn enclosing_definition(
    symbols: &[LspDocumentSymbol],
    line: u32,
) -> Option<&LspDocumentSymbol> {
    let deepest = |kinds: &[&str]| {
        symbols
            .iter()
            .filter(|s| kinds.contains(&s.kind.as_str()))
            .filter(|s| s.range.start.line <= line && line <= s.range.end.line)
            .min_by_key(|s| s.range.end.line.saturating_sub(s.range.start.line))
    };
    deepest(&LEAF_KINDS).or_else(|| deepest(&CONTAINER_KINDS))
}

/// Rows a freshly scoped card should show for a symbol of `kind`: `None` = the
/// auto plateau (the whole scope, capped at `CARD_MAX_LINES`); `Some(n)` for
/// containers so a type card stays compact.
pub(crate) fn scope_rows_cap(kind: &str) -> Option<usize> {
    CONTAINER_KINDS
        .contains(&kind)
        .then_some(CONTAINER_CARD_ROWS)
}

/// Resolve `(line, character)` in `path` to its enclosing scope and build the
/// scoped card: `(origin, snapshot, (scope_start, scope_end), rows_cap)`. The
/// origin keeps the QUERY site — the card's dedup key — not the scope start.
/// `None` when no scope contains the line (the caller keeps its ±N window) or the
/// file can't be read. The ONE resolve path for the focal, the sync refine (cached
/// symbols) and the async refine (`CanvasCardScopeResult`).
pub(crate) fn scope_snapshot(
    theme: &ThemeConfig,
    symbols: &[LspDocumentSymbol],
    path: &Path,
    line: u32,
    character: u32,
) -> Option<(BlockOrigin, BlockSnapshot, (u32, u32), Option<usize>)> {
    let scope = enclosing_definition(symbols, line)?;
    let (start, end) = (scope.range.start.line, scope.range.end.line);
    let (mut origin, snapshot) = super::lsp::build_canvas_relation_snapshot_range(
        theme,
        path,
        start,
        end,
        character,
        &scope.name,
    )?;
    origin.lsp_line = line;
    Some((origin, snapshot, (start, end), scope_rows_cap(&scope.kind)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::async_runtime::message::{LspPosition, LspRange};

    fn sym(kind: &str, s: u32, e: u32) -> LspDocumentSymbol {
        named_sym("x", kind, s, e)
    }

    fn named_sym(name: &str, kind: &str, s: u32, e: u32) -> LspDocumentSymbol {
        LspDocumentSymbol {
            name: name.into(),
            kind: kind.into(),
            range: LspRange {
                start: LspPosition {
                    line: s,
                    character: 0,
                },
                end: LspPosition {
                    line: e,
                    character: 0,
                },
            },
            ancestors: Vec::new(),
        }
    }

    #[test]
    fn picks_deepest_enclosing_function_or_method() {
        // class [0..50] contains method [10..20]; line 12 → the method, not the class.
        let symbols = vec![
            sym("Class", 0, 50),
            sym("Method", 10, 20),
            sym("Function", 30, 40),
        ];
        let r = enclosing_definition(&symbols, 12).expect("enclosing");
        assert_eq!((r.range.start.line, r.range.end.line), (10, 20));
    }

    #[test]
    fn returns_the_targets_own_name_for_the_card_title() {
        // A card spawned onto line 15 must be titled by the METHOD it lands in,
        // not by some other symbol — regression for "child card inherits focal
        // symbol name".
        let symbols = vec![
            named_sym("start", "Method", 0, 50),
            named_sym("gracefulShutdown", "Method", 10, 20),
        ];
        let s = enclosing_definition(&symbols, 15).expect("enclosing");
        assert_eq!(s.name, "gracefulShutdown");
        assert_eq!((s.range.start.line, s.range.end.line), (10, 20));
    }

    #[test]
    fn const_enclosing_at_its_own_line() {
        let symbols = vec![sym("Constant", 5, 5)];
        let r = enclosing_definition(&symbols, 5).expect("const");
        assert_eq!((r.range.start.line, r.range.end.line), (5, 5));
    }

    #[test]
    fn non_definition_kinds_return_none() {
        // Line is inside a symbol of a kind that is neither a leaf definition nor
        // a container (e.g. TypeParameter, Key) → no scope.
        let symbols = vec![sym("TypeParameter", 0, 50), sym("Key", 0, 50)];
        assert!(enclosing_definition(&symbols, 3).is_none());
    }

    #[test]
    fn arrow_function_variable_or_property_beats_its_enclosing_class() {
        // TS/JS report `handler = () => {…}` as Property and `const useFoo = () =>
        // {…}` as Variable — functions in disguise. The deepest LEAF wins over the
        // enclosing Class, so the card shows the handler, not the class from its
        // top (which read as "the whole file").
        let symbols = vec![
            named_sym("App", "Class", 0, 300),
            named_sym("handleClick", "Property", 40, 60),
            named_sym("useFoo", "Variable", 100, 130),
        ];
        assert_eq!(
            enclosing_definition(&symbols, 45).unwrap().name,
            "handleClick"
        );
        assert_eq!(enclosing_definition(&symbols, 120).unwrap().name, "useFoo");
        // No leaf contains line 80 → the Class (container) is the fallback.
        assert_eq!(enclosing_definition(&symbols, 80).unwrap().name, "App");
    }

    #[test]
    fn container_scopes_get_a_row_cap_leaves_do_not() {
        for k in [
            "Class",
            "Struct",
            "Interface",
            "Enum",
            "Object",
            "Module",
            "Namespace",
        ] {
            assert_eq!(scope_rows_cap(k), Some(CONTAINER_CARD_ROWS), "{k}");
        }
        for k in [
            "Function",
            "Method",
            "Constructor",
            "Constant",
            "Variable",
            "Property",
            "Field",
        ] {
            assert_eq!(scope_rows_cap(k), None, "{k}");
        }
    }

    #[test]
    fn scope_snapshot_builds_the_scoped_card_with_the_container_cap() {
        use std::time::{SystemTime, UNIX_EPOCH};
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("netherize_canvas_scope_snap_{nanos}"));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("big.rs");
        let body = (0..40)
            .map(|i| format!("line {i}"))
            .collect::<Vec<_>>()
            .join("\n");
        std::fs::write(&path, body).unwrap();
        let theme = crate::config::theme_config::ThemeConfig::builtin_dark();

        let structs = vec![named_sym("Big", "Struct", 0, 39)];
        let (origin, snap, range, rows) =
            scope_snapshot(&theme, &structs, &path, 5, 2).expect("struct scope");
        assert_eq!(range, (0, 39));
        assert_eq!(rows, Some(CONTAINER_CARD_ROWS), "a type card is row-capped");
        assert_eq!(snap.symbol, "Big");
        assert_eq!(snap.start_line, 1);
        assert_eq!(snap.text.lines().count(), 40);
        // The origin keeps the QUERY site (the dedup key), not the scope start.
        assert_eq!((origin.lsp_line, origin.lsp_character), (5, 2));

        let fns = vec![named_sym("run", "Function", 0, 39)];
        let (_, _, _, rows) = scope_snapshot(&theme, &fns, &path, 5, 2).expect("fn scope");
        assert_eq!(rows, None, "a function card shows its whole scope");

        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn class_is_now_a_definition_kind() {
        let symbols = vec![sym("Class", 0, 50), sym("Method", 10, 20)];
        // Line 3 is inside Class but not Method → returns Class.
        let r = enclosing_definition(&symbols, 3).expect("class");
        assert_eq!((r.range.start.line, r.range.end.line), (0, 50));
        // Line 15 is inside Method → returns Method (smaller).
        let r = enclosing_definition(&symbols, 15).expect("method");
        assert_eq!((r.range.start.line, r.range.end.line), (10, 20));
    }

    #[test]
    fn line_outside_all_symbols_returns_none() {
        let symbols = vec![sym("Function", 10, 20)];
        assert!(enclosing_definition(&symbols, 99).is_none());
    }

    #[test]
    fn empty_symbols_returns_none_keep_window() {
        // The worker returns an EMPTY symbol list on any LSP failure (server not
        // ready / file not open / timeout). That MUST resolve to `None` so the
        // refine handler keeps the card's ±N window — applying a `(0, 0)` fallback
        // instead showed only the file's first import line (bug-239). Constructors
        // in not-yet-warmed target files hit this most.
        let symbols: Vec<LspDocumentSymbol> = Vec::new();
        assert!(enclosing_definition(&symbols, 0).is_none());
        assert!(enclosing_definition(&symbols, 42).is_none());
    }

    #[test]
    fn finds_constructor_scope_when_symbols_present() {
        // A class with a constructor: a card landing on the constructor's body
        // scopes to the CONSTRUCTOR, never collapsing to the file top.
        let symbols = vec![
            named_sym("Service", "Class", 3, 40),
            named_sym("constructor", "Constructor", 8, 16),
            named_sym("run", "Method", 18, 30),
        ];
        let s = enclosing_definition(&symbols, 10).expect("constructor scope");
        assert_eq!(s.name, "constructor");
        assert_eq!((s.range.start.line, s.range.end.line), (8, 16));
    }
}
