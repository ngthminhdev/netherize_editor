//! Scope-aware card snapshot selection (T4).
//!
//! `documentSymbol` arrives as a flat list with `ancestors`; "deepest enclosing"
//! == smallest containing span. Pure `(symbols, line) -> Option<range>` so it is
//! unit-testable with hand-built symbol lists.

use crate::async_runtime::message::{LspDocumentSymbol, LspRange};

/// The deepest enclosing **definition** symbol (function/method/const/constructor)
/// whose line range contains `line`. Returns `None` when no definition-kind
/// symbol contains the line (caller falls back to the ±N window).
pub(crate) fn enclosing_definition(symbols: &[LspDocumentSymbol], line: u32) -> Option<LspRange> {
    const DEF_KINDS: [&str; 4] = ["Function", "Method", "Constant", "Constructor"];
    symbols
        .iter()
        .filter(|s| DEF_KINDS.contains(&s.kind.as_str()))
        .filter(|s| s.range.start.line <= line && line <= s.range.end.line)
        .min_by_key(|s| s.range.end.line.saturating_sub(s.range.start.line))
        .map(|s| s.range.clone())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::async_runtime::message::{LspPosition, LspRange};

    fn sym(kind: &str, s: u32, e: u32) -> LspDocumentSymbol {
        LspDocumentSymbol {
            name: "x".into(),
            kind: kind.into(),
            range: LspRange {
                start: LspPosition { line: s, character: 0 },
                end: LspPosition { line: e, character: 0 },
            },
            ancestors: Vec::new(),
        }
    }

    #[test]
    fn picks_deepest_enclosing_function_or_method() {
        // class [0..50] contains method [10..20]; line 12 → the method, not the class.
        let symbols = vec![sym("Class", 0, 50), sym("Method", 10, 20), sym("Function", 30, 40)];
        let r = enclosing_definition(&symbols, 12).expect("enclosing");
        assert_eq!((r.start.line, r.end.line), (10, 20));
    }

    #[test]
    fn const_enclosing_at_its_own_line() {
        let symbols = vec![sym("Constant", 5, 5)];
        let r = enclosing_definition(&symbols, 5).expect("const");
        assert_eq!((r.start.line, r.end.line), (5, 5));
    }

    #[test]
    fn no_definition_kind_enclosing_returns_none() {
        // Line is inside a class body but not inside any function/method/const.
        let symbols = vec![sym("Class", 0, 50)];
        assert!(enclosing_definition(&symbols, 3).is_none());
    }

    #[test]
    fn line_outside_all_symbols_returns_none() {
        let symbols = vec![sym("Function", 10, 20)];
        assert!(enclosing_definition(&symbols, 99).is_none());
    }
}
