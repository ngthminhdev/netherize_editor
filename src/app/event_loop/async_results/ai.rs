use super::super::*;
use crate::async_runtime::message::WorkerResultPayload;

/// Chars of buffer text after the caret used to trim suggestion/suffix overlap.
const SANITIZE_SUFFIX_CONTEXT_CHARS: usize = 200;

pub(crate) use crate::app::app_state::InlineEdit;

fn is_ident_char(ch: char) -> bool {
    ch.is_alphanumeric() || ch == '_'
}

/// Longest suffix of `line_prefix` that `text` starts with, in bytes. Models
/// often echo the token (or the few chars) right before the caret — `P` →
/// `Promise(`, `foo.` → `.bar()` — and inserting that verbatim doubles it.
fn echoed_tail_len(text: &str, line_prefix: &str) -> usize {
    for (idx, _) in line_prefix.char_indices() {
        let tail = &line_prefix[idx..];
        if text.starts_with(tail) {
            return tail.len();
        }
    }
    0
}

/// Detect a model that re-emitted the current line WITH changes (it fixed a
/// typo the caret sits after: `await new Promies.` → `await new Promise(`).
/// Returns `(chars of the line prefix to replace, byte offset into text where
/// the replacement starts)` when at least one whole leading token matches
/// and something after it differs; None for plain continuations.
fn line_rewrite(text: &str, trimmed_prefix: &str) -> Option<(usize, usize)> {
    let prefix_chars: Vec<char> = trimmed_prefix.chars().collect();
    let text_chars: Vec<char> = text.chars().collect();
    let mut common = 0usize;
    while common < prefix_chars.len()
        && common < text_chars.len()
        && prefix_chars[common] == text_chars[common]
    {
        common += 1;
    }
    if common < 3 || common == prefix_chars.len() {
        // Too little in common to call it the same line, or the whole prefix
        // matched (a plain echo, handled by the caller).
        return None;
    }
    // Back the boundary off to a token edge so we never split an identifier:
    // `const ` vs `config = 1` share `con` but are unrelated.
    let mut boundary = common;
    while boundary > 0 {
        let before = prefix_chars[boundary - 1];
        let splits_ident = is_ident_char(before)
            && (prefix_chars.get(boundary).copied().is_some_and(is_ident_char)
                || text_chars.get(boundary).copied().is_some_and(is_ident_char));
        if splits_ident {
            boundary -= 1;
        } else {
            break;
        }
    }
    if boundary == 0 {
        return None;
    }
    let replace_chars = prefix_chars.len() - boundary;
    let text_byte_start: usize = text_chars[..boundary].iter().map(|ch| ch.len_utf8()).sum();
    if replace_chars == 0 || text_byte_start >= text.len() {
        return None;
    }
    Some((replace_chars, text_byte_start))
}

/// Closers auto-pairing leaves after the caret (`');`) that the completion
/// already produced itself: the model closed the string and the call, so
/// inserting its text in front of the residue doubles them
/// (`…', err);');`). Returns how many leading chars of the caret-line
/// suffix the edit consumes: a closer is consumed when the kept line prefix
/// plus the completion is already balanced for it (bracket counts equal,
/// quote count even) or, for `;`/`,`, when the completion ends with it.
fn consumed_closers(kept_prefix: &str, text: &str, caret_line_suffix: &str) -> usize {
    fn count(hay: &str, ch: char) -> usize {
        hay.chars().filter(|c| *c == ch).count()
    }
    let residue: Vec<char> = caret_line_suffix
        .chars()
        .take_while(|ch| matches!(ch, ')' | ']' | '}' | '\'' | '"' | '`' | ';' | ','))
        .collect();
    if residue.is_empty() {
        return 0;
    }
    let joined = format!("{kept_prefix}{text}");
    let mut consumed = 0;
    for ch in residue {
        let redundant = match ch {
            ')' => count(&joined, '(') == count(&joined, ')'),
            ']' => count(&joined, '[') == count(&joined, ']'),
            '}' => count(&joined, '{') == count(&joined, '}'),
            '\'' | '"' | '`' => count(&joined, ch) % 2 == 0,
            ';' | ',' => text.ends_with(ch),
            _ => false,
        };
        if !redundant {
            break;
        }
        consumed += 1;
    }
    consumed
}

/// While streaming, a buffer that is still a prefix of some tail of the
/// current line may grow into an echo or a rewrite (`awa` of `await new P`):
/// hold it back instead of flashing it as ghost text and re-deciding later.
pub(crate) fn inline_stream_may_echo(buffer: &str, line_prefix: &str) -> bool {
    if buffer.is_empty() {
        return false;
    }
    line_prefix
        .char_indices()
        .any(|(idx, _)| line_prefix[idx..].starts_with(buffer))
}

/// Clean raw model output before showing it as ghost text: normalize newlines,
/// strip markdown fences, drop an echoed current-line prefix, detect a rewrite
/// of the line's tail, and trim the tail that duplicates text already after
/// the caret (e.g. closing brackets). Returns None when nothing usable remains.
pub(crate) fn sanitize_inline_suggestion(
    raw: &str,
    line_prefix: &str,
    suffix: &str,
) -> Option<InlineEdit> {
    let mut text = raw.replace("\r\n", "\n").replace('\r', "\n");
    // The prompt marks the caret with this token; a model that echoes it must
    // not have it inserted.
    if text.contains("<|cursor|>") {
        text = text.replace("<|cursor|>", "");
    }

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
    // whitespace-only case), then the non-whitespace remainder, then the
    // longest echoed tail (`P` → `Promise(`), and finally a rewrite of the
    // line's last token(s).
    let mut replace_before_caret = 0usize;
    if !line_prefix.is_empty() {
        let trimmed_prefix = line_prefix.trim_start();
        if let Some(rest) = text.strip_prefix(line_prefix) {
            text = rest.to_string();
        } else if !trimmed_prefix.is_empty()
            && let Some(rest) = text.strip_prefix(trimmed_prefix)
        {
            text = rest.to_string();
        } else if let Some((replace_chars, text_start)) = line_rewrite(&text, trimmed_prefix) {
            replace_before_caret = replace_chars;
            text = text[text_start..].to_string();
        } else {
            let echoed = echoed_tail_len(&text, line_prefix);
            if echoed > 0 {
                text = text[echoed..].to_string();
            }
        }
    }

    // Mid-line completion (text follows the caret on its line): only the rest
    // of this line can be inserted. The worker also sends `stop = "\n"`; this
    // guards models that ignore stop sequences.
    let caret_line_has_tail = !suffix.split('\n').next().unwrap_or("").trim().is_empty();
    if caret_line_has_tail && let Some(newline) = text.find('\n') {
        text.truncate(newline);
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
        return None;
    }

    // Auto-paired closers right after the caret that the completion already
    // closed itself are consumed on accept instead of being doubled.
    let kept_prefix: String = {
        let keep = line_prefix.chars().count().saturating_sub(replace_before_caret);
        line_prefix.chars().take(keep).collect()
    };
    let caret_line_suffix = suffix.split('\n').next().unwrap_or("");
    let replace_after_caret = consumed_closers(&kept_prefix, &text, caret_line_suffix);

    Some(InlineEdit {
        text,
        replace_before_caret,
        replace_after_caret,
    })
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
            // Ghost text streams in progressively. Every chunk re-sanitizes the
            // whole buffer against the caret context, so an echoed indent or
            // line prefix never flashes before the final result replaces it.
            // The completion menu (if open) stays open: both are visible, Tab
            // takes the ghost text, Enter takes the menu item.
            app.ai_inline_stream_buffer.push_str(&chunk);
            let (line_prefix, suffix) = app
                .app_state
                .inline_suggestion_context(SANITIZE_SUFFIX_CONTEXT_CHARS);
            if inline_stream_may_echo(&app.ai_inline_stream_buffer, &line_prefix) {
                // Still ambiguous (could be an echo of the line prefix or a
                // rewrite of it): wait for more before showing anything.
                return;
            }
            let cleaned =
                sanitize_inline_suggestion(&app.ai_inline_stream_buffer, &line_prefix, &suffix);
            if app.app_state.set_inline_suggestion_edit(cleaned) {
                app.editor_needs_layout = true;
                app.editor_caret_needs_layout = false;
                app.request_redraw();
            }
        }
        WorkerResultPayload::AiInlineCompletionResult { suggestion } => {
            app.ai_inline_inflight = false;
            app.ai_inline_failure_streak = 0;
            app.ai_inline_stream_buffer.clear();
            // Same guard as the chunk arm: never surface a completion at a
            // position the user has already left.
            if !app.ai_inline_anchor_is_current() {
                return;
            }
            let (line_prefix, suffix) = app
                .app_state
                .inline_suggestion_context(SANITIZE_SUFFIX_CONTEXT_CHARS);
            let cleaned = sanitize_inline_suggestion(&suggestion, &line_prefix, &suffix);
            if app.app_state.set_inline_suggestion_edit(cleaned) {
                app.editor_needs_layout = true;
                app.editor_caret_needs_layout = false;
            }
            app.request_redraw();
        }
        WorkerResultPayload::AiModelsListed { models } => {
            app.on_ai_models_listed(models);
        }
        WorkerResultPayload::AiCompletionRerankResult {
            ranked,
            prefix_token,
            completion_revision,
        } => {
            // Apply only if the very popup we ranked is still open and untouched:
            // the typed prefix must be unchanged AND the user must not have moved
            // the selection (which bumps `current_revision`). Otherwise drop it —
            // a re-rank must never yank a selection the user has already acted on.
            let still_current = app.app_state.completion().is_some_and(|completion| {
                completion.typed_prefix == prefix_token
                    && completion.current_revision == completion_revision
            });
            if !still_current {
                return;
            }
            if let Some(completion) = app.app_state.completion_mut()
                && completion.apply_ai_rerank(&ranked)
            {
                app.editor_caret_needs_layout = true;
                app.request_redraw();
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::{InlineEdit, inline_stream_may_echo};

    /// Plain-insertion view of the sanitizer for the existing cases.
    fn sanitize_inline_suggestion(raw: &str, line_prefix: &str, suffix: &str) -> Option<String> {
        super::sanitize_inline_suggestion(raw, line_prefix, suffix).map(|edit| {
            assert_eq!(edit.replace_before_caret, 0, "unexpected rewrite for {raw:?}");
            assert_eq!(edit.replace_after_caret, 0, "unexpected consumed closers for {raw:?}");
            edit.text
        })
    }

    #[test]
    fn consumes_auto_paired_closers_the_completion_already_closed() {
        // `console.error('|');` (quote + paren auto-paired, `;` typed) and the
        // model closes the string AND the call: every residue char is redundant.
        assert_eq!(
            super::sanitize_inline_suggestion(
                "Error during graceful shutdown', err);",
                "    console.error('",
                "');\n    this.isShuttingDown = false;"
            ),
            Some(InlineEdit {
                text: "Error during graceful shutdown', err);".to_string(),
                replace_before_caret: 0,
                replace_after_caret: 3,
            })
        );
        // Copilot-style output that stops before the closers: only the quote
        // it closed is redundant; `);` stays.
        assert_eq!(
            super::sanitize_inline_suggestion(
                "Error during graceful shutdown', err",
                "console.error('",
                "');"
            ),
            Some(InlineEdit {
                text: "Error during graceful shutdown', err".to_string(),
                replace_before_caret: 0,
                replace_after_caret: 1,
            })
        );
        // Output that only closes the string: the exact suffix overlap is
        // trimmed (as before) and nothing is consumed.
        assert_eq!(
            sanitize_inline_suggestion("Error during graceful shutdown'", "console.error('", "');"),
            Some("Error during graceful shutdown".to_string())
        );
        // Unbalanced: the model opened a call it did not close — the
        // auto-paired `)` is still needed.
        assert_eq!(
            sanitize_inline_suggestion("bar(x", "foo(", ")"),
            Some("bar(x".to_string())
        );
        // Residue stops at the first non-closer.
        assert_eq!(
            super::sanitize_inline_suggestion("a'", "f('", "') + 1;"),
            Some(InlineEdit {
                text: "a".to_string(),
                replace_before_caret: 0,
                replace_after_caret: 0,
            })
        );
    }

    #[test]
    fn strips_the_echoed_token_before_the_caret() {
        // Caret after `await new P`; the model re-emits the whole line.
        assert_eq!(
            sanitize_inline_suggestion(
                "await new Promise((resolve) => setTimeout(resolve, 100));",
                "    await new P",
                ""
            ),
            Some("romise((resolve) => setTimeout(resolve, 100));".to_string())
        );
        // Only the last token echoed.
        assert_eq!(
            sanitize_inline_suggestion("Promise.resolve()", "await new P", ""),
            Some("romise.resolve()".to_string())
        );
        // A duplicated separator.
        assert_eq!(
            sanitize_inline_suggestion(".then(r => r)", "fetch(url).", ""),
            Some("then(r => r)".to_string())
        );
    }

    #[test]
    fn detects_a_rewrite_of_the_lines_last_token() {
        // Typo before the caret; the model emits the corrected line.
        assert_eq!(
            super::sanitize_inline_suggestion(
                "await new Promise((resolve) => setTimeout(resolve, ms));",
                "    await new Promies.",
                ""
            ),
            Some(InlineEdit {
                text: "Promise((resolve) => setTimeout(resolve, ms));".to_string(),
                replace_before_caret: "Promies.".chars().count(),
                replace_after_caret: 0,
            })
        );
        // Mid-line: the rewrite still stops at the caret line's tail.
        assert_eq!(
            super::sanitize_inline_suggestion(
                "const total = sum(items);\nreturn total;",
                "const totl = ",
                "\n}"
            ),
            Some(InlineEdit {
                text: "total = sum(items);\nreturn total;".to_string(),
                replace_before_caret: "totl = ".chars().count(),
                replace_after_caret: 0,
            })
        );
    }

    #[test]
    fn a_shared_identifier_prefix_is_not_a_rewrite() {
        // `const ` + `config = …` share `con` — a continuation, not a rewrite.
        assert_eq!(
            sanitize_inline_suggestion("config = load();", "const ", ""),
            Some("config = load();".to_string())
        );
    }

    #[test]
    fn stream_holds_while_the_buffer_may_still_be_an_echo() {
        assert!(inline_stream_may_echo("awa", "    await new P"));
        assert!(inline_stream_may_echo("await new Promi", "await new Promies."));
        assert!(inline_stream_may_echo("  ", "    "));
        assert!(!inline_stream_may_echo("await new Promise", "await new Promies."));
        assert!(!inline_stream_may_echo("romise(", "await new P"));
        assert!(!inline_stream_may_echo("", "foo"));
    }

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
    fn strips_a_leaked_cursor_marker() {
        assert_eq!(
            sanitize_inline_suggestion("<|cursor|>bar()", "foo.", ""),
            Some("bar()".to_string())
        );
    }

    #[test]
    fn mid_line_completion_keeps_only_the_rest_of_the_line() {
        // Caret inside `foo(|)`: the model returns a value AND a next statement.
        assert_eq!(
            sanitize_inline_suggestion("x, y)\nbaz();", "foo(", ")\nnext();"),
            Some("x, y".to_string())
        );
        // End of line: multi-line completions stay intact.
        assert_eq!(
            sanitize_inline_suggestion("\n    return x;\n}", "fn f() {", "\n"),
            Some("\n    return x;\n}".to_string())
        );
    }

    #[test]
    fn normalizes_crlf() {
        assert_eq!(
            sanitize_inline_suggestion("a\r\nb", "", ""),
            Some("a\nb".to_string())
        );
    }
}
