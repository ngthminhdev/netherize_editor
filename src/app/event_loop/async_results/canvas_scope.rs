//! Scope-aware card snapshot selection (T4).
//!
//! `documentSymbol` arrives as a flat list with `ancestors`; "deepest enclosing"
//! == smallest containing span. Pure `(symbols, line) -> Option<range>` so it is
//! unit-testable with hand-built symbol lists.

use crate::async_runtime::message::LspDocumentSymbol;

/// The deepest enclosing **definition** symbol (function/method/const/constructor)
/// whose line range contains `line`. Returns `None` when no definition-kind
/// symbol contains the line (caller falls back to the ±N window). The returned
/// symbol carries both its `range` (for the scope snapshot) and its `name` (for
/// the card title) — a spawned card must show the TARGET's name, not the canvas's
/// focal symbol.
pub(crate) fn enclosing_definition(
    symbols: &[LspDocumentSymbol],
    line: u32,
) -> Option<&LspDocumentSymbol> {
    const DEF_KINDS: [&str; 8] = [
        "Function",
        "Method",
        "Constant",
        "Constructor",
        "Class",
        "Interface",
        "Struct",
        "Enum",
    ];
    symbols
        .iter()
        .filter(|s| DEF_KINDS.contains(&s.kind.as_str()))
        .filter(|s| s.range.start.line <= line && line <= s.range.end.line)
        .min_by_key(|s| s.range.end.line.saturating_sub(s.range.start.line))
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
    fn no_definition_kind_enclosing_returns_none() {
        // Line is inside a non-definition symbol (e.g. Variable, Namespace).
        let symbols = vec![sym("Variable", 0, 50)];
        assert!(enclosing_definition(&symbols, 3).is_none());
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
