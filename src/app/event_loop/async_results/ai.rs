use super::super::*;
use crate::async_runtime::message::WorkerResultPayload;

/// Chars of buffer text after the caret used to trim suggestion/suffix overlap.
const SANITIZE_SUFFIX_CONTEXT_CHARS: usize = 200;

/// Clean raw model output before showing it as ghost text: normalize newlines,
/// strip markdown fences, drop an echoed current-line prefix, and trim the tail
/// that duplicates text already after the caret (e.g. closing brackets).
/// Returns None when nothing usable remains.
pub(crate) fn sanitize_inline_suggestion(
    raw: &str,
    line_prefix: &str,
    suffix: &str,
) -> Option<String> {
    let mut text = raw.replace("\r\n", "\n").replace('\r', "\n");

    if text.trim_start().starts_with("```") {
        let mut lines: Vec<&str> = text.lines().collect();
        lines.remove(0);
        if lines.last().is_some_and(|line| line.trim() == "```") {
            lines.pop();
        }
        text = lines.join("\n");
    } else if let Some(idx) = text.find("\n```") {
        text.truncate(idx);
    }

    // Drop an echoed copy of the current line's prefix. Models often re-emit the
    // text before the caret — including, on a fresh indented line, the leading
    // whitespace itself. If we don't strip that whitespace, accepting a
    // multi-line completion double-indents its first line (the caret already
    // sits at that indentation). Try the full prefix first (covers the
    // whitespace-only case), then fall back to the non-whitespace remainder.
    if !line_prefix.is_empty() {
        if let Some(rest) = text.strip_prefix(line_prefix) {
            text = rest.to_string();
        } else {
            let trimmed_prefix = line_prefix.trim_start();
            if !trimmed_prefix.is_empty()
                && let Some(rest) = text.strip_prefix(trimmed_prefix)
            {
                text = rest.to_string();
            }
        }
    }

    // Trim a tail that duplicates text already after the caret. Match against the
    // FULL suffix (not just its first line) so a model that re-emits a multi-line
    // closing like "`);\n});" is cleaned, not only single-line overlaps.
    let suffix_match = suffix.trim_end();
    if !suffix_match.is_empty() && !text.is_empty() {
        for k in (1..=suffix_match.len()).rev() {
            if suffix_match.is_char_boundary(k) && text.ends_with(&suffix_match[..k]) {
                text.truncate(text.len() - k);
                break;
            }
        }
    }

    if text.trim().is_empty() {
        None
    } else {
        Some(text)
    }
}

/// Ghost text takes priority over the LSP completion popup: once an inline
/// suggestion is visible, dismiss the completion menu (and its loading state) so
/// the two overlays never render stacked on top of each other.
fn dismiss_completion_for_ghost_text(app: &mut AppShell) {
    app.app_state.set_completion_loading(false);
    if app.app_state.clear_completion() {
        app.editor_caret_needs_layout = true;
    }
}

pub(super) fn handle_ai_result(app: &mut AppShell, payload: WorkerResultPayload) {
    match payload {
        WorkerResultPayload::AiInlineCompletionChunk { chunk } => {
            app.ai_inline_inflight = false;
            // The caret left the position this request was made for (movement,
            // mode or buffer switch) — drop the result instead of showing it at
            // the caret's new location.
            if !app.ai_inline_anchor_is_current() {
                return;
            }
            if app.app_state.append_inline_suggestion_chunk(&chunk) {
                // Ghost text wins: dismiss the LSP completion popup so the two
                // overlays do not render on top of each other.
                dismiss_completion_for_ghost_text(app);
                app.editor_needs_layout = true;
                app.editor_caret_needs_layout = false;
                app.request_redraw();
            }
        }
        WorkerResultPayload::AiInlineCompletionResult { suggestion } => {
            app.ai_inline_inflight = false;
            app.ai_inline_failure_streak = 0;
            // Same guard as the chunk arm: never surface a completion at a
            // position the user has already left.
            if !app.ai_inline_anchor_is_current() {
                return;
            }
            let (line_prefix, suffix) = app
                .app_state
                .inline_suggestion_context(SANITIZE_SUFFIX_CONTEXT_CHARS);
            let cleaned = sanitize_inline_suggestion(&suggestion, &line_prefix, &suffix);
            if app.app_state.set_inline_suggestion(cleaned) {
                // Ghost text wins: dismiss the LSP completion popup so the two
                // overlays do not render on top of each other.
                dismiss_completion_for_ghost_text(app);
                app.editor_needs_layout = true;
                app.editor_caret_needs_layout = false;
            }
            app.request_redraw();
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::sanitize_inline_suggestion;

    #[test]
    fn keeps_plain_continuation() {
        assert_eq!(
            sanitize_inline_suggestion("bar(x);", "    foo.", ""),
            Some("bar(x);".to_string())
        );
    }

    #[test]
    fn strips_markdown_fences() {
        assert_eq!(
            sanitize_inline_suggestion("```rust\nlet x = 1;\n```", "", ""),
            Some("let x = 1;".to_string())
        );
        assert_eq!(
            sanitize_inline_suggestion("let x = 1;\n```", "", ""),
            Some("let x = 1;".to_string())
        );
    }

    #[test]
    fn drops_echoed_line_prefix() {
        assert_eq!(
            sanitize_inline_suggestion("let total = a + b;", "    let total = ", ""),
            Some("a + b;".to_string())
        );
    }

    #[test]
    fn trims_overlap_with_suffix() {
        // Buffer: foo(|)  → model returns "bar())" style duplicates.
        assert_eq!(
            sanitize_inline_suggestion("bar()", "foo(", ")"),
            Some("bar(".to_string())
        );
        // Entirely duplicated closing token → nothing left to suggest.
        assert_eq!(sanitize_inline_suggestion(")", "foo(", ")"), None);
    }

    #[test]
    fn strips_echoed_indentation_on_fresh_line() {
        // Caret on a fresh line indented by 8 spaces; the model re-emits that
        // indentation. Without stripping it the accepted line is double-indented.
        assert_eq!(
            sanitize_inline_suggestion("        results.append(item * 2)", "        ", "",),
            Some("results.append(item * 2)".to_string())
        );
        // Tab indentation, multi-line: line 1's echoed tab is dropped, deeper
        // lines keep their own absolute indentation.
        assert_eq!(
            sanitize_inline_suggestion("\tfor i := range xs {\n\t\tsum += i\n\t}", "\t", "",),
            Some("for i := range xs {\n\t\tsum += i\n\t}".to_string())
        );
    }

    #[test]
    fn keeps_deeper_indentation_relative_to_caret() {
        // Model returns MORE indentation than the caret (legitimate nesting):
        // only the echoed caret-level whitespace is removed, the extra stays.
        assert_eq!(
            sanitize_inline_suggestion("            nested()", "        ", ""),
            Some("    nested()".to_string())
        );
    }

    #[test]
    fn trims_multiline_suffix_overlap() {
        // Model re-emits the whole statement incl. a multi-line closing that
        // duplicates the suffix (`);\n});). Only the new content must remain.
        assert_eq!(
            sanitize_inline_suggestion(
                "  console.log(`Server running on ${PORT}`);\n});",
                "  console.log(`",
                "`);\n});",
            ),
            Some("Server running on ${PORT}".to_string())
        );
    }

    #[test]
    fn rejects_empty_and_whitespace() {
        assert_eq!(sanitize_inline_suggestion("", "", ""), None);
        assert_eq!(sanitize_inline_suggestion("  \n  ", "", ""), None);
    }

    #[test]
    fn normalizes_crlf() {
        assert_eq!(
            sanitize_inline_suggestion("a\r\nb", "", ""),
            Some("a\nb".to_string())
        );
    }
}
