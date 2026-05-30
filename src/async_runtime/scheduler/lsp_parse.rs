use std::sync::Arc;

use serde_json::Value;

use crate::{
    async_runtime::message::{
        LspCodeAction, LspCompletionItem, LspDiagnostic, LspDocumentHighlight, LspDocumentSymbol,
        LspLocation, LspPosition, LspRange, LspTextEdit, WorkerResultPayload,
    },
    lsp::client::{LspClientProcess, build_did_change_notification},
};

use super::{
    LSP_CODE_ACTION_TIMEOUT_SECS, LSP_COMPLETION_RESOLVE_TIMEOUT_SECS, LSP_COMPLETION_TIMEOUT_SECS,
    LSP_DEFINITION_TIMEOUT_SECS, LSP_DOCUMENT_SYMBOLS_TIMEOUT_SECS, LSP_FORMATTING_TIMEOUT_SECS,
    LSP_HOVER_TIMEOUT_SECS, LSP_REFERENCES_TIMEOUT_SECS,
};

pub(super) fn lsp_request_response(
    session: &Arc<LspClientProcess>,
    method: &str,
    params: serde_json::Value,
    timeout_secs: u64,
) -> Result<serde_json::Value, String> {
    let request_id = session.allocate_request_id();
    let rx = session.register_pending_request(request_id);
    session.send_request_with_id(request_id, method, params)?;
    let deadline = std::time::Duration::from_secs(timeout_secs);
    match rx.recv_timeout(deadline) {
        Ok(value) => Ok(value),
        Err(_) => {
            session.clear_pending_request(request_id);
            Err(format!(
                "lsp {method} request timed out after {timeout_secs}s"
            ))
        }
    }
}

/// Same as `lsp_request_response`, but tagged with a cancellation `key`
/// (e.g. `"definition"`, `"hover"`). When the user fires the same kind of
/// request again before this one finishes, the worker dispatching the new
/// request will atomically replace the inflight slot for `key` and send
/// `$/cancelRequest` for the previous id, so the LSP server frees its slot
/// instead of letting the old request linger and starve the queue.
pub(super) fn lsp_cancellable_request_response(
    session: &Arc<LspClientProcess>,
    key: &'static str,
    method: &str,
    params: serde_json::Value,
    timeout_secs: u64,
) -> Result<serde_json::Value, String> {
    let request_id = session.allocate_request_id();
    if let Some(previous_id) = session.swap_inflight(key, request_id) {
        // Best-effort: tell the server to abandon the previous request. If the
        // server doesn't understand $/cancelRequest the worst case is the same
        // as today (old request finishes and main-thread reconciliation drops
        // it as stale).
        session.send_cancel_request(previous_id);
    }
    let rx = session.register_pending_request(request_id);
    if let Err(err) = session.send_request_with_id(request_id, method, params) {
        session.clear_pending_request(request_id);
        session.clear_inflight_if_matches(key, request_id);
        return Err(err);
    }
    let deadline = std::time::Duration::from_secs(timeout_secs);
    let outcome = match rx.recv_timeout(deadline) {
        Ok(value) => Ok(value),
        Err(_) => {
            session.clear_pending_request(request_id);
            Err(format!(
                "lsp {method} request timed out after {timeout_secs}s"
            ))
        }
    };
    session.clear_inflight_if_matches(key, request_id);
    outcome
}

fn parse_hover_content(result: &Value) -> String {
    let contents = match result.get("contents") {
        Some(contents) => contents,
        None => return String::new(),
    };
    if let Some(value) = contents.get("value").and_then(|value| value.as_str()) {
        return value.trim().to_string();
    }
    if let Some(text) = contents.as_str() {
        return text.trim().to_string();
    }
    if let Some(items) = contents.as_array() {
        return items
            .iter()
            .filter_map(|item| {
                item.get("value")
                    .and_then(|value| value.as_str())
                    .or_else(|| item.as_str())
            })
            .collect::<Vec<_>>()
            .join("\n");
    }
    String::new()
}

const MAX_COMPLETION_ITEMS: usize = 200;

fn parse_completion_item(item: &Value) -> Option<LspCompletionItem> {
    let label = item.get("label")?.as_str()?.to_string();
    let detail = item
        .get("detail")
        .and_then(Value::as_str)
        .map(str::to_string)
        .or_else(|| {
            item.pointer("/labelDetails/detail")
                .and_then(Value::as_str)
                .map(str::to_string)
        });
    let insert_text = item
        .get("insertText")
        .and_then(Value::as_str)
        .map(str::to_string);
    let text_edit = item.get("textEdit").and_then(parse_completion_text_edit);
    let text_edit_text = text_edit
        .as_ref()
        .map(|edit| edit.new_text.clone())
        .or_else(|| {
            item.get("textEdit")
                .and_then(|edit| edit.get("newText"))
                .and_then(Value::as_str)
                .map(str::to_string)
        });
    let additional_text_edits = item
        .get("additionalTextEdits")
        .map(parse_text_edits)
        .unwrap_or_default();
    let kind = item
        .get("kind")
        .and_then(Value::as_u64)
        .map(|kind| kind as u32);
    let callable = kind.map(completion_kind_is_callable);
    let has_parameters = infer_completion_has_parameters(
        &label,
        kind,
        detail.as_deref(),
        text_edit_text.as_deref().or(insert_text.as_deref()),
    );
    let documentation = item
        .get("documentation")
        .and_then(parse_documentation_field);
    let data = item
        .get("data")
        .and_then(|value| serde_json::to_string(value).ok());
    let raw_json = serde_json::to_string(item).ok();
    Some(LspCompletionItem {
        label,
        detail,
        insert_text,
        text_edit,
        text_edit_text,
        additional_text_edits,
        kind,
        callable,
        has_parameters,
        documentation,
        data,
        source_path: None,
        import_path: None,
        export_kind: None,
        raw_json,
    })
}

fn completion_kind_is_callable(kind: u32) -> bool {
    matches!(kind, 2 | 3 | 4)
}

fn infer_completion_has_parameters(
    label: &str,
    _kind: Option<u32>,
    detail: Option<&str>,
    insert_text: Option<&str>,
) -> Option<bool> {
    if let Some(text) = insert_text
        && let Some(has_parameters) = infer_call_parameters_from_insert_text(text)
    {
        return Some(has_parameters);
    }
    if let Some(detail) = detail
        && let Some(has_parameters) = infer_call_parameters_from_signature(label, detail)
    {
        return Some(has_parameters);
    }
    None
}

fn infer_call_parameters_from_insert_text(text: &str) -> Option<bool> {
    let open = text.rfind('(')?;
    let close = find_matching_paren(text, open)?;
    if text[close + 1..].trim().is_empty() {
        return Some(!text[open + 1..close].trim().is_empty());
    }
    None
}

fn infer_call_parameters_from_signature(label: &str, signature: &str) -> Option<bool> {
    let label = label
        .split('(')
        .next()
        .unwrap_or(label)
        .rsplit('.')
        .next()
        .unwrap_or(label)
        .trim();
    let mut search_from = 0usize;
    while let Some(relative_open) = signature[search_from..].find('(') {
        let open = search_from + relative_open;
        let Some(close) = find_matching_paren(signature, open) else {
            break;
        };
        if signature[..open].trim().is_empty() {
            let after = signature[close + 1..].trim_start();
            if after.is_empty()
                || after.starts_with("=>")
                || after.starts_with(':')
                || after.starts_with("->")
            {
                return Some(!signature[open + 1..close].trim().is_empty());
            }
        }
        if go_func_signature_parenthesis_looks_like_params(
            &signature[..open],
            &signature[close + 1..],
        ) {
            return Some(!signature[open + 1..close].trim().is_empty());
        }
        if signature_parenthesis_looks_like_call(label, &signature[..open]) {
            return Some(!signature[open + 1..close].trim().is_empty());
        }
        search_from = close + 1;
    }
    None
}

fn go_func_signature_parenthesis_looks_like_params(before_open: &str, after_close: &str) -> bool {
    let before = before_open.trim_end();
    if before != "func" {
        return false;
    }
    let after = after_close.trim_start();
    !after
        .chars()
        .next()
        .is_some_and(|ch| ch.is_ascii_alphabetic() || ch == '_' || ch == '$')
}

fn signature_parenthesis_looks_like_call(label: &str, before_open: &str) -> bool {
    let before = before_open.trim_end();
    if before.is_empty() {
        return false;
    }
    if label.is_empty() {
        return before.ends_with("=>") || before.ends_with(':');
    }
    let without_generics = strip_trailing_type_args(before);
    let token = without_generics
        .rsplit(|ch: char| !(ch.is_ascii_alphanumeric() || ch == '_' || ch == '$' || ch == '.'))
        .next()
        .unwrap_or("")
        .rsplit('.')
        .next()
        .unwrap_or("");
    token == label
        || before.ends_with(&format!("{label}:"))
        || before.contains(&format!(" {label}:"))
}

fn strip_trailing_type_args(text: &str) -> &str {
    let trimmed = text.trim_end();
    if !trimmed.ends_with('>') {
        return trimmed;
    }
    let mut depth = 0usize;
    for (idx, ch) in trimmed.char_indices().rev() {
        match ch {
            '>' => depth = depth.saturating_add(1),
            '<' => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    return trimmed[..idx].trim_end();
                }
            }
            _ => {}
        }
    }
    trimmed
}

fn find_matching_paren(text: &str, open: usize) -> Option<usize> {
    let mut depth = 0usize;
    let mut quote = None;
    let mut escape = false;
    for (idx, ch) in text[open..].char_indices() {
        let absolute = open + idx;
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
            '(' => depth = depth.saturating_add(1),
            ')' => {
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

fn parse_completion_text_edit(edit: &Value) -> Option<LspTextEdit> {
    let new_text = edit.get("newText")?.as_str()?.to_string();
    let range = edit
        .get("range")
        .or_else(|| edit.get("replace"))
        .or_else(|| edit.get("insert"))?;
    parse_lsp_text_edit_parts(range, new_text)
}

fn parse_completion_items(result: &Value) -> Vec<LspCompletionItem> {
    let items = result
        .get("items")
        .and_then(Value::as_array)
        .or_else(|| result.as_array());

    items
        .map(|items| {
            items
                .iter()
                .take(MAX_COMPLETION_ITEMS)
                .filter_map(parse_completion_item)
                .collect()
        })
        .unwrap_or_default()
}

fn parse_locations(result: &Value) -> Vec<LspLocation> {
    let items = match result.as_array() {
        Some(array) => array.as_slice(),
        None => {
            if let (Some(uri), Some(line), Some(character)) = (
                result.get("uri").and_then(|value| value.as_str()),
                result
                    .pointer("/range/start/line")
                    .and_then(|value| value.as_u64()),
                result
                    .pointer("/range/start/character")
                    .and_then(|value| value.as_u64()),
            ) {
                return vec![LspLocation {
                    uri: uri.to_string(),
                    line: line as u32,
                    character: character as u32,
                }];
            }
            return Vec::new();
        }
    };

    items
        .iter()
        .filter_map(|item| {
            let uri = item.get("uri").and_then(|value| value.as_str())?;
            let line = item
                .pointer("/range/start/line")
                .and_then(|value| value.as_u64())
                .or_else(|| {
                    item.pointer("/targetRange/start/line")
                        .and_then(|value| value.as_u64())
                })?;
            let character = item
                .pointer("/range/start/character")
                .and_then(|value| value.as_u64())
                .or_else(|| {
                    item.pointer("/targetRange/start/character")
                        .and_then(|value| value.as_u64())
                })
                .unwrap_or(0);
            Some(LspLocation {
                uri: uri.to_string(),
                line: line as u32,
                character: character as u32,
            })
        })
        .collect()
}

fn parse_text_edits(result: &Value) -> Vec<LspTextEdit> {
    result
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|item| {
            let start_line = item.pointer("/range/start/line")?.as_u64()? as u32;
            let start_character = item.pointer("/range/start/character")?.as_u64()? as u32;
            let end_line = item.pointer("/range/end/line")?.as_u64()? as u32;
            let end_character = item.pointer("/range/end/character")?.as_u64()? as u32;
            let new_text = item.get("newText")?.as_str()?.to_string();
            Some(LspTextEdit {
                range: LspRange {
                    start: LspPosition {
                        line: start_line,
                        character: start_character,
                    },
                    end: LspPosition {
                        line: end_line,
                        character: end_character,
                    },
                },
                new_text,
            })
        })
        .collect()
}

fn parse_lsp_text_edit_parts(range: &Value, new_text: String) -> Option<LspTextEdit> {
    let start_line = range.pointer("/start/line")?.as_u64()? as u32;
    let start_character = range.pointer("/start/character")?.as_u64()? as u32;
    let end_line = range.pointer("/end/line")?.as_u64()? as u32;
    let end_character = range.pointer("/end/character")?.as_u64()? as u32;
    Some(LspTextEdit {
        range: LspRange {
            start: LspPosition {
                line: start_line,
                character: start_character,
            },
            end: LspPosition {
                line: end_line,
                character: end_character,
            },
        },
        new_text,
    })
}

pub(super) fn handle_lsp_hover(
    session: &Arc<LspClientProcess>,
    uri: &str,
    line: u32,
    character: u32,
    cursor_line: usize,
    cursor_col: usize,
    for_completion: bool,
    completion_revision: Option<u64>,
) -> Result<WorkerResultPayload, String> {
    handle_lsp_hover_on_uri(
        session,
        uri,
        line,
        character,
        cursor_line,
        cursor_col,
        for_completion,
        completion_revision,
        "hover",
    )
}

pub(super) fn handle_lsp_completion_virtual_hover(
    session: &Arc<LspClientProcess>,
    uri: &str,
    original_text: &str,
    text: &str,
    hover_line: u32,
    hover_character: u32,
    completion_revision: u64,
) -> Result<WorkerResultPayload, String> {
    let version = completion_revision.min(i32::MAX as u64) as i32;
    session.send_notification(
        "textDocument/didChange",
        build_did_change_notification(uri, version, text),
    )?;

    let result = handle_lsp_hover_on_uri(
        session,
        uri,
        hover_line,
        hover_character,
        hover_line as usize,
        hover_character as usize,
        true,
        Some(completion_revision),
        "completion_virtual_hover",
    );

    if let Err(err) = session.send_notification(
        "textDocument/didChange",
        build_did_change_notification(uri, version.saturating_add(1), original_text),
    ) {
        eprintln!("[LSP] restore original document after completion hover failed: {err}");
    }

    result
}

fn handle_lsp_hover_on_uri(
    session: &Arc<LspClientProcess>,
    uri: &str,
    line: u32,
    character: u32,
    cursor_line: usize,
    cursor_col: usize,
    for_completion: bool,
    completion_revision: Option<u64>,
    cancellation_key: &'static str,
) -> Result<WorkerResultPayload, String> {
    let params = serde_json::json!({
        "textDocument": { "uri": uri },
        "position": { "line": line, "character": character }
    });
    let response = lsp_cancellable_request_response(
        session,
        cancellation_key,
        "textDocument/hover",
        params,
        LSP_HOVER_TIMEOUT_SECS,
    )?;
    let result = response
        .get("result")
        .ok_or_else(|| "hover: no result".to_string())?;
    if result.is_null() {
        return Err("hover: null result (no documentation)".to_string());
    }
    let content = parse_hover_content(result);
    if content.is_empty() {
        return Err("hover: empty documentation".to_string());
    }
    // Only the overlay path renders syntax-highlighted markdown blocks.
    // The completion-fallback path (`for_completion=true`) keeps the flat
    // string and lets the popup renderer strip markdown inline.
    let parsed_blocks = if for_completion {
        None
    } else {
        Some(parse_hover_doc_blocks(&content))
    };
    Ok(WorkerResultPayload::LspHoverResult {
        content,
        cursor_line,
        cursor_col,
        for_completion,
        completion_revision,
        parsed_blocks,
    })
}

/// Worker-side markdown block splitter. Produces `HoverDocBlock`s with
/// Tree-sitter highlight spans already attached to code blocks so the main
/// thread doesn't run `highlight_snippet` on its hot path. Theme colour
/// resolution still happens on main (cheap hash lookups in
/// `syntax_spans_to_styled`).
fn parse_hover_doc_blocks(content: &str) -> Vec<crate::async_runtime::message::HoverDocBlock> {
    use crate::async_runtime::message::HoverDocBlock;
    use crate::config::theme_config::ThemeConfig;
    use crate::syntax::highlight::highlight_snippet;

    let mut blocks = Vec::new();
    let mut prose_lines: Vec<String> = Vec::new();
    let mut code_lines: Vec<String> = Vec::new();
    let mut code_language = String::new();
    let mut in_code_block = false;
    // `highlight_snippet` ignores its theme parameter (`_theme`); we pass a
    // builtin theme purely to satisfy the signature without dragging the
    // live theme across thread boundaries. Colour resolution stays on the
    // main thread via `syntax_spans_to_styled`.
    let theme = ThemeConfig::builtin_dark();

    let flush_prose = |blocks: &mut Vec<HoverDocBlock>, prose_lines: &mut Vec<String>| {
        let text = prose_lines.join("\n").trim().to_string();
        prose_lines.clear();
        if !text.is_empty() {
            blocks.push(HoverDocBlock::Prose(text));
        }
    };

    let flush_code =
        |blocks: &mut Vec<HoverDocBlock>, code_lines: &mut Vec<String>, code_language: &str| {
            let text = code_lines.join("\n");
            code_lines.clear();
            if text.trim().is_empty() {
                return;
            }
            let spans = highlight_snippet(&text, code_language, &theme);
            blocks.push(HoverDocBlock::Code { text, spans });
        };

    for line in content.lines() {
        let trimmed = line.trim_start();
        if let Some(fence) = trimmed.strip_prefix("```") {
            if in_code_block {
                flush_code(&mut blocks, &mut code_lines, &code_language);
                code_language.clear();
                in_code_block = false;
            } else {
                flush_prose(&mut blocks, &mut prose_lines);
                code_language = fence.trim().to_string();
                in_code_block = true;
            }
            continue;
        }
        if in_code_block {
            code_lines.push(line.to_string());
        } else {
            prose_lines.push(line.to_string());
        }
    }

    if in_code_block {
        flush_code(&mut blocks, &mut code_lines, &code_language);
    } else {
        flush_prose(&mut blocks, &mut prose_lines);
    }

    blocks
}

/// Parse the `documentation` field of a CompletionItem (used both inline and in
/// `completionItem/resolve` responses). Handles every shape the LSP spec allows:
///   - plain string (legacy)
///   - `MarkupContent { kind, value }` (current)
///   - `MarkedString[]` array (deprecated, but tsserver/gopls still emit it)
fn parse_documentation_field(value: &Value) -> Option<String> {
    if let Some(s) = value.as_str() {
        let trimmed = s.trim();
        return (!trimmed.is_empty()).then(|| trimmed.to_string());
    }
    if let Some(s) = value.pointer("/value").and_then(Value::as_str) {
        let trimmed = s.trim();
        return (!trimmed.is_empty()).then(|| trimmed.to_string());
    }
    if let Some(arr) = value.as_array() {
        let parts: Vec<String> = arr
            .iter()
            .filter_map(|item| {
                item.as_str().map(str::to_string).or_else(|| {
                    item.pointer("/value")
                        .and_then(Value::as_str)
                        .map(str::to_string)
                })
            })
            .filter(|s| !s.trim().is_empty())
            .collect();
        if !parts.is_empty() {
            return Some(parts.join("\n\n"));
        }
    }
    None
}

pub(super) fn handle_lsp_completion_resolve(
    session: &Arc<LspClientProcess>,
    item_label: &str,
    item_json: &str,
    completion_revision: u64,
) -> Result<WorkerResultPayload, String> {
    let params: Value = serde_json::from_str(item_json)
        .map_err(|err| format!("completion resolve: invalid item JSON: {err}"))?;
    // Use the cancellable variant so that when the user navigates the
    // completion popup quickly, each new dispatch sends `$/cancelRequest`
    // for the previous in-flight resolve. Without this, slow LSP servers
    // (e.g. pyright on cold cache) would queue every selection step and
    // block newer items behind stale work.
    let response = lsp_cancellable_request_response(
        session,
        "completionResolve",
        "completionItem/resolve",
        params,
        LSP_COMPLETION_RESOLVE_TIMEOUT_SECS,
    )?;
    let result = response
        .get("result")
        .ok_or_else(|| "completion resolve: no result".to_string())?;
    if result.is_null() {
        return Err("completion resolve: null result".to_string());
    }
    let detail = result
        .get("detail")
        .and_then(Value::as_str)
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .or_else(|| {
            result
                .pointer("/labelDetails/detail")
                .and_then(Value::as_str)
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
        });
    let documentation = result
        .get("documentation")
        .and_then(parse_documentation_field);
    let resolved_item = parse_completion_item(result);

    // Diagnostic: if the resolve came back but neither detail nor documentation
    // could be extracted, log the raw response so we can spot unsupported shapes.
    if detail.is_none() && documentation.is_none() {
        if let Ok(serialized) = serde_json::to_string(result) {
            let snippet: String = serialized.chars().take(500).collect();
            eprintln!(
                "[LSP] completionItem/resolve for '{}' returned no detail/documentation. \
                 Raw result (first 500 chars): {}",
                item_label, snippet
            );
        }
    }

    Ok(WorkerResultPayload::LspCompletionResolveResult {
        item_label: item_label.to_string(),
        detail,
        documentation,
        resolved_item,
        completion_revision,
    })
}

pub(super) fn handle_lsp_definition(
    session: &Arc<LspClientProcess>,
    uri: &str,
    line: u32,
    character: u32,
    jump: bool,
) -> Result<WorkerResultPayload, String> {
    let params = serde_json::json!({
        "textDocument": { "uri": uri },
        "position": { "line": line, "character": character }
    });
    let response = lsp_cancellable_request_response(
        session,
        "definition",
        "textDocument/definition",
        params,
        LSP_DEFINITION_TIMEOUT_SECS,
    )?;
    let result = response
        .get("result")
        .ok_or_else(|| "definition: no result".to_string())?;
    if result.is_null() {
        return Err("definition: no definition found".to_string());
    }
    let locations = parse_locations(result);
    if locations.is_empty() {
        return Err("definition: no locations returned".to_string());
    }
    Ok(WorkerResultPayload::LspDefinitionResult { locations, jump })
}

pub(super) fn handle_lsp_formatting(
    session: &Arc<LspClientProcess>,
    uri: &str,
    tab_size: u32,
    insert_spaces: bool,
) -> Result<WorkerResultPayload, String> {
    let params = serde_json::json!({
        "textDocument": { "uri": uri },
        "options": {
            "tabSize": tab_size,
            "insertSpaces": insert_spaces,
        }
    });
    let response = lsp_request_response(
        session,
        "textDocument/formatting",
        params,
        LSP_FORMATTING_TIMEOUT_SECS,
    )?;
    let result = response
        .get("result")
        .ok_or_else(|| "formatting: no result".to_string())?;
    if result.is_null() {
        return Ok(WorkerResultPayload::LspFormattingResult {
            uri: uri.to_string(),
            edits: Vec::new(),
        });
    }

    Ok(WorkerResultPayload::LspFormattingResult {
        uri: uri.to_string(),
        edits: parse_text_edits(result),
    })
}

pub(super) fn handle_lsp_references(
    session: &Arc<LspClientProcess>,
    uri: &str,
    line: u32,
    character: u32,
) -> Result<Vec<LspLocation>, String> {
    let params = serde_json::json!({
        "textDocument": { "uri": uri },
        "position": { "line": line, "character": character },
        "context": { "includeDeclaration": true }
    });
    let response = lsp_request_response(
        session,
        "textDocument/references",
        params,
        LSP_REFERENCES_TIMEOUT_SECS,
    )?;
    if let Some(error) = response.get("error") {
        let msg = error.get("message").and_then(|m| m.as_str()).unwrap_or("unknown error");
        let code = error.get("code").and_then(|c| c.as_i64()).unwrap_or(0);
        return Err(format!("references: LSP error {code}: {msg}"));
    }
    let result = response
        .get("result")
        .ok_or_else(|| "references: no result".to_string())?;
    if result.is_null() {
        return Err("references: no references found".to_string());
    }
    let locations = parse_locations(result);
    if locations.is_empty() {
        return Err("references: empty result".to_string());
    }
    Ok(locations)
}

pub(super) fn handle_lsp_rename(
    session: &Arc<LspClientProcess>,
    uri: &str,
    line: u32,
    character: u32,
    new_name: &str,
) -> Result<WorkerResultPayload, String> {
    let params = serde_json::json!({
        "textDocument": { "uri": uri },
        "position": { "line": line, "character": character },
        "newName": new_name,
    });
    let response = lsp_request_response(
        session,
        "textDocument/rename",
        params,
        LSP_REFERENCES_TIMEOUT_SECS,
    )?;
    let result = response
        .get("result")
        .ok_or_else(|| "rename: no result".to_string())?;
    if result.is_null() {
        return Ok(WorkerResultPayload::LspRenameResult {
            uri: uri.to_string(),
            edits: Vec::new(),
            other_file_edit_count: 0,
        });
    }

    let (edits, other_file_edit_count) = parse_workspace_edit_for_uri(result, uri);
    Ok(WorkerResultPayload::LspRenameResult {
        uri: uri.to_string(),
        edits,
        other_file_edit_count,
    })
}

fn parse_workspace_edit_for_uri(result: &Value, target_uri: &str) -> (Vec<LspTextEdit>, usize) {
    let mut edits = Vec::new();
    let mut other_file_edit_count = 0usize;

    if let Some(changes) = result.get("changes").and_then(Value::as_object) {
        for (uri, value) in changes {
            let parsed = parse_text_edits(value);
            if uri == target_uri {
                edits.extend(parsed);
            } else {
                other_file_edit_count = other_file_edit_count.saturating_add(parsed.len());
            }
        }
    }

    if let Some(document_changes) = result.get("documentChanges").and_then(Value::as_array) {
        for change in document_changes {
            if let Some(uri) = change.pointer("/textDocument/uri").and_then(Value::as_str) {
                let parsed = change
                    .get("edits")
                    .map(parse_text_edits)
                    .unwrap_or_default();
                if uri == target_uri {
                    edits.extend(parsed);
                } else {
                    other_file_edit_count = other_file_edit_count.saturating_add(parsed.len());
                }
            } else {
                other_file_edit_count = other_file_edit_count.saturating_add(1);
            }
        }
    }

    (edits, other_file_edit_count)
}

pub(super) fn handle_lsp_document_highlight(
    session: &Arc<LspClientProcess>,
    uri: &str,
    line: u32,
    character: u32,
) -> Result<WorkerResultPayload, String> {
    let params = serde_json::json!({
        "textDocument": { "uri": uri },
        "position": { "line": line, "character": character }
    });
    let response = lsp_request_response(
        session,
        "textDocument/documentHighlight",
        params,
        LSP_REFERENCES_TIMEOUT_SECS,
    )?;
    let result = response
        .get("result")
        .ok_or_else(|| "document highlight: no result".to_string())?;
    if result.is_null() {
        return Ok(WorkerResultPayload::LspDocumentHighlightResult {
            uri: uri.to_string(),
            highlights: Vec::new(),
        });
    }

    Ok(WorkerResultPayload::LspDocumentHighlightResult {
        uri: uri.to_string(),
        highlights: parse_document_highlights(result),
    })
}

fn parse_document_highlights(result: &Value) -> Vec<LspDocumentHighlight> {
    result
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|item| {
            let start_line = item.pointer("/range/start/line")?.as_u64()? as u32;
            let start_character = item.pointer("/range/start/character")?.as_u64()? as u32;
            let end_line = item.pointer("/range/end/line")?.as_u64()? as u32;
            let end_character = item.pointer("/range/end/character")?.as_u64()? as u32;
            let kind = item
                .get("kind")
                .and_then(Value::as_u64)
                .map(|kind| kind as u32);
            Some(LspDocumentHighlight {
                range: LspRange {
                    start: LspPosition {
                        line: start_line,
                        character: start_character,
                    },
                    end: LspPosition {
                        line: end_line,
                        character: end_character,
                    },
                },
                kind,
            })
        })
        .collect()
}

pub(super) fn handle_lsp_document_symbols(
    session: &Arc<LspClientProcess>,
    uri: &str,
) -> Result<WorkerResultPayload, String> {
    use lsp_types::request::{DocumentSymbolRequest, Request};

    let params = serde_json::json!({
        "textDocument": { "uri": uri }
    });
    let response = lsp_request_response(
        session,
        DocumentSymbolRequest::METHOD,
        params,
        LSP_DOCUMENT_SYMBOLS_TIMEOUT_SECS,
    )?;
    let result = response
        .get("result")
        .ok_or_else(|| "document symbols: no result".to_string())?;
    if result.is_null() {
        return Ok(WorkerResultPayload::LspDocumentSymbolsResult {
            uri: uri.to_string(),
            symbols: Vec::new(),
        });
    }

    Ok(WorkerResultPayload::LspDocumentSymbolsResult {
        uri: uri.to_string(),
        symbols: parse_document_symbols(result),
    })
}

fn parse_document_symbols(result: &Value) -> Vec<LspDocumentSymbol> {
    let Ok(response) = serde_json::from_value::<lsp_types::DocumentSymbolResponse>(result.clone())
    else {
        return Vec::new();
    };

    fn range_from_lsp(range: &lsp_types::Range) -> LspRange {
        LspRange {
            start: LspPosition {
                line: range.start.line,
                character: range.start.character,
            },
            end: LspPosition {
                line: range.end.line,
                character: range.end.character,
            },
        }
    }

    fn kind_label(kind: &lsp_types::SymbolKind) -> String {
        let number = serde_json::to_value(kind)
            .ok()
            .and_then(|value| value.as_u64())
            .unwrap_or_default() as u32;
        match number {
            1 => "File",
            2 => "Module",
            3 => "Namespace",
            4 => "Package",
            5 => "Class",
            6 => "Method",
            7 => "Property",
            8 => "Field",
            9 => "Constructor",
            10 => "Enum",
            11 => "Interface",
            12 => "Function",
            13 => "Variable",
            14 => "Constant",
            15 => "String",
            16 => "Number",
            17 => "Boolean",
            18 => "Array",
            19 => "Object",
            20 => "Key",
            21 => "Null",
            22 => "EnumMember",
            23 => "Struct",
            24 => "Event",
            25 => "Operator",
            26 => "TypeParameter",
            _ => "Symbol",
        }
        .to_string()
    }

    fn position_leq(left: &LspPosition, right: &LspPosition) -> bool {
        left.line < right.line || (left.line == right.line && left.character <= right.character)
    }

    fn range_strictly_contains(outer: &LspRange, inner: &LspRange) -> bool {
        position_leq(&outer.start, &inner.start)
            && position_leq(&inner.end, &outer.end)
            && (outer.start.line != inner.start.line
                || outer.start.character != inner.start.character
                || outer.end.line != inner.end.line
                || outer.end.character != inner.end.character)
    }

    fn ancestor_sort_key(
        symbol: &LspDocumentSymbol,
    ) -> (u32, u32, std::cmp::Reverse<u32>, std::cmp::Reverse<u32>) {
        (
            symbol.range.start.line,
            symbol.range.start.character,
            std::cmp::Reverse(symbol.range.end.line),
            std::cmp::Reverse(symbol.range.end.character),
        )
    }

    fn attach_flat_symbol_ancestors(symbols: &mut [LspDocumentSymbol]) {
        let snapshot = symbols.to_vec();
        for (index, symbol) in symbols.iter_mut().enumerate() {
            let mut ancestors: Vec<_> = snapshot
                .iter()
                .enumerate()
                .filter(|(candidate_index, candidate)| {
                    *candidate_index != index
                        && range_strictly_contains(&candidate.range, &symbol.range)
                })
                .map(|(_, candidate)| candidate)
                .collect();
            ancestors.sort_by_key(|candidate| ancestor_sort_key(candidate));
            symbol.ancestors = ancestors
                .into_iter()
                .map(
                    |candidate| crate::async_runtime::message::LspDocumentSymbolSegment {
                        name: candidate.name.clone(),
                        kind: candidate.kind.clone(),
                    },
                )
                .collect();
        }
    }

    fn push_nested(
        out: &mut Vec<LspDocumentSymbol>,
        symbols: &[lsp_types::DocumentSymbol],
        ancestors: &[crate::async_runtime::message::LspDocumentSymbolSegment],
    ) {
        for symbol in symbols {
            let current = crate::async_runtime::message::LspDocumentSymbolSegment {
                name: symbol.name.clone(),
                kind: kind_label(&symbol.kind),
            };
            out.push(LspDocumentSymbol {
                name: current.name.clone(),
                kind: current.kind.clone(),
                range: range_from_lsp(&symbol.range),
                ancestors: ancestors.to_vec(),
            });
            if let Some(children) = &symbol.children {
                let mut next_ancestors = ancestors.to_vec();
                next_ancestors.push(current);
                push_nested(out, children, &next_ancestors);
            }
        }
    }

    let mut out = Vec::new();
    match response {
        lsp_types::DocumentSymbolResponse::Nested(symbols) => push_nested(&mut out, &symbols, &[]),
        lsp_types::DocumentSymbolResponse::Flat(symbols) => {
            out.extend(symbols.into_iter().map(|symbol| LspDocumentSymbol {
                name: symbol.name,
                kind: kind_label(&symbol.kind),
                range: range_from_lsp(&symbol.location.range),
                ancestors: Vec::new(),
            }));
            attach_flat_symbol_ancestors(&mut out);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::{parse_completion_item, parse_completion_items, parse_document_symbols};
    use serde_json::json;

    #[test]
    fn parse_completion_preserves_text_edits_and_data() {
        let response = json!({
            "items": [{
                "label": "connect",
                "kind": 3,
                "insertText": "connect",
                "textEdit": {
                    "range": {
                        "start": { "line": 4, "character": 8 },
                        "end": { "line": 4, "character": 11 }
                    },
                    "newText": "connect"
                },
                "additionalTextEdits": [{
                    "range": {
                        "start": { "line": 0, "character": 0 },
                        "end": { "line": 0, "character": 0 }
                    },
                    "newText": "import { connect } from './api';\n"
                }],
                "data": { "entryNames": ["connect"] }
            }]
        });

        let items = parse_completion_items(&response);

        assert_eq!(items.len(), 1);
        assert_eq!(
            items[0]
                .text_edit
                .as_ref()
                .map(|edit| edit.range.start.line),
            Some(4)
        );
        assert_eq!(items[0].additional_text_edits.len(), 1);
        assert!(
            items[0]
                .data
                .as_ref()
                .is_some_and(|data| data.contains("entryNames"))
        );
    }

    #[test]
    fn parse_resolved_completion_preserves_import_edits() {
        let resolved = json!({
            "label": "connect",
            "additionalTextEdits": [{
                "range": {
                    "start": { "line": 1, "character": 0 },
                    "end": { "line": 1, "character": 0 }
                },
                "newText": "import { connect } from './api';\n"
            }]
        });

        let item = parse_completion_item(&resolved).expect("resolved completion item");

        assert_eq!(item.label, "connect");
        assert_eq!(item.additional_text_edits.len(), 1);
        assert_eq!(
            item.additional_text_edits[0].new_text,
            "import { connect } from './api';\n"
        );
    }

    #[test]
    fn parse_completion_infers_callable_parameter_shape() {
        let response = json!({
            "items": [
                {
                    "label": "wait",
                    "kind": 3,
                    "insertText": "wait",
                    "detail": "function wait(ms: number): Promise<void>"
                },
                {
                    "label": "init",
                    "kind": 3,
                    "insertText": "init",
                    "detail": "function init(): void"
                },
                {
                    "label": "NewApp",
                    "kind": 3,
                    "insertText": "NewApp()",
                    "detail": "func() *bootstrap.App"
                },
                {
                    "label": "NewAppWithContext",
                    "kind": 3,
                    "insertText": "NewAppWithContext",
                    "detail": "func(ctx context.Context) *bootstrap.App"
                }
            ]
        });

        let items = parse_completion_items(&response);

        assert_eq!(items[0].callable, Some(true));
        assert_eq!(items[0].has_parameters, Some(true));
        assert_eq!(items[1].callable, Some(true));
        assert_eq!(items[1].has_parameters, Some(false));
        assert_eq!(items[2].callable, Some(true));
        assert_eq!(items[2].has_parameters, Some(false));
        assert_eq!(items[3].callable, Some(true));
        assert_eq!(items[3].has_parameters, Some(true));
    }

    #[test]
    fn parse_document_symbols_rebuilds_ancestors_for_flat_symbols() {
        let response = json!([
            {
                "name": "KafkaProducer",
                "kind": 5,
                "location": {
                    "uri": "file:///tmp/demo.ts",
                    "range": {
                        "start": { "line": 6, "character": 0 },
                        "end": { "line": 30, "character": 1 }
                    }
                }
            },
            {
                "name": "constructor",
                "kind": 9,
                "location": {
                    "uri": "file:///tmp/demo.ts",
                    "range": {
                        "start": { "line": 12, "character": 2 },
                        "end": { "line": 22, "character": 3 }
                    }
                }
            },
            {
                "name": "kafkaClient",
                "kind": 14,
                "location": {
                    "uri": "file:///tmp/demo.ts",
                    "range": {
                        "start": { "line": 14, "character": 8 },
                        "end": { "line": 14, "character": 30 }
                    }
                }
            }
        ]);

        let symbols = parse_document_symbols(&response);
        let kafka_client = symbols
            .iter()
            .find(|symbol| symbol.name == "kafkaClient")
            .expect("expected kafkaClient symbol");

        let labels: Vec<(&str, &str)> = kafka_client
            .ancestors
            .iter()
            .map(|segment| (segment.kind.as_str(), segment.name.as_str()))
            .collect();
        assert_eq!(
            labels,
            vec![("Class", "KafkaProducer"), ("Constructor", "constructor")]
        );
    }
}

pub(super) fn handle_lsp_completion(
    session: &Arc<LspClientProcess>,
    uri: &str,
    line: u32,
    character: u32,
    cursor_line: usize,
    cursor_col: usize,
    prefix_start_col: usize,
    prefix: &str,
) -> Result<WorkerResultPayload, String> {
    let params = serde_json::json!({
        "textDocument": { "uri": uri },
        "position": { "line": line, "character": character }
    });
    let response = lsp_request_response(
        session,
        "textDocument/completion",
        params,
        LSP_COMPLETION_TIMEOUT_SECS,
    )?;
    let result = response
        .get("result")
        .ok_or_else(|| "completion: no result".to_string())?;
    let items = if result.is_null() {
        Vec::new()
    } else {
        parse_completion_items(result)
    };
    Ok(WorkerResultPayload::LspCompletionResult {
        items,
        cursor_line,
        cursor_col,
        prefix_start_col,
        prefix: prefix.to_string(),
    })
}

pub(super) fn handle_lsp_code_action(
    session: &Arc<LspClientProcess>,
    uri: &str,
    line: u32,
    character: u32,
    diagnostics: &[LspDiagnostic],
) -> Result<WorkerResultPayload, String> {
    let diag_items: Vec<serde_json::Value> = diagnostics
        .iter()
        .map(|d| {
            serde_json::json!({
                "range": {
                    "start": { "line": d.range.start.line, "character": d.range.start.character },
                    "end": { "line": d.range.end.line, "character": d.range.end.character }
                },
                "severity": d.severity,
                "code": d.code,
                "source": d.source,
                "message": d.message
            })
        })
        .collect();

    let params = serde_json::json!({
        "textDocument": { "uri": uri },
        "range": {
            "start": { "line": line, "character": character },
            "end": { "line": line, "character": character }
        },
        "context": {
            "diagnostics": diag_items
        }
    });

    let response = lsp_request_response(
        session,
        "textDocument/codeAction",
        params,
        LSP_CODE_ACTION_TIMEOUT_SECS,
    )?;
    let result = response
        .get("result")
        .ok_or_else(|| "codeAction: no result".to_string())?;
    if result.is_null() {
        return Err("codeAction: no actions available".to_string());
    }

    let mut actions = parse_code_actions(result);
    if actions.is_empty() {
        return Err("codeAction: no actions available".to_string());
    }

    // Try to resolve actions that have commands but no edits via codeAction/resolve.
    for action in actions.iter_mut() {
        if !action.edits.is_empty() || action.raw_action.is_none() {
            continue;
        }
        let Some(raw) = &action.raw_action else {
            continue;
        };
        let Ok(raw_value) = serde_json::from_str::<serde_json::Value>(raw) else {
            continue;
        };
        match lsp_request_response(
            session,
            "codeAction/resolve",
            raw_value,
            LSP_CODE_ACTION_TIMEOUT_SECS,
        ) {
            Ok(resolve_resp) => {
                if let Some(resolved) = resolve_resp.get("result") {
                    action.edits = parse_edits_from_action(resolved);
                }
            }
            Err(_) => {}
        }
    }

    Ok(WorkerResultPayload::LspCodeActionResult { actions })
}

/// Handle workspace/symbol request — returns all symbols in the workspace.
pub(super) fn handle_workspace_symbol(
    session: &Arc<LspClientProcess>,
    query: &str,
) -> Result<Vec<crate::lsp::CachedSymbol>, String> {
    let params = serde_json::json!({
        "query": query
    });

    let response = lsp_request_response(
        session,
        "workspace/symbol",
        params,
        LSP_DOCUMENT_SYMBOLS_TIMEOUT_SECS,
    )?;

    if let Some(error) = response.get("error") {
        let msg = error.get("message").and_then(|m| m.as_str()).unwrap_or("unknown error");
        let code = error.get("code").and_then(|c| c.as_i64()).unwrap_or(0);
        return Err(format!("workspace/symbol: LSP error {code}: {msg}"));
    }

    let result = response
        .get("result")
        .ok_or_else(|| "workspace/symbol: no result".to_string())?;

    if result.is_null() {
        return Ok(Vec::new());
    }

    let symbols_array = result
        .as_array()
        .ok_or_else(|| "workspace/symbol: result is not an array".to_string())?;

    let mut symbols = Vec::new();
    for symbol in symbols_array {
        if let Some(cached_symbol) = parse_workspace_symbol(symbol) {
            symbols.push(cached_symbol);
        }
    }

    Ok(symbols)
}

/// Parse a single workspace symbol from JSON.
fn parse_workspace_symbol(symbol: &Value) -> Option<crate::lsp::CachedSymbol> {
    let name = symbol.get("name")?.as_str()?.to_string();
    let kind = symbol.get("kind")?.as_u64()? as u32;
    let kind_str = match kind {
        1 => "File",
        2 => "Module",
        3 => "Namespace",
        4 => "Package",
        5 => "Class",
        6 => "Method",
        7 => "Property",
        8 => "Field",
        9 => "Constructor",
        10 => "Enum",
        11 => "Interface",
        12 => "Function",
        13 => "Variable",
        14 => "Constant",
        15 => "String",
        16 => "Number",
        17 => "Boolean",
        18 => "Array",
        19 => "Object",
        20 => "Key",
        21 => "Null",
        22 => "EnumMember",
        23 => "Struct",
        24 => "Event",
        25 => "Operator",
        26 => "TypeParameter",
        _ => "Symbol",
    }
    .to_string();

    let container_name = symbol
        .get("containerName")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    // Parse location
    let location = symbol.get("location")?;
    let uri = location.get("uri")?.as_str()?;
    let file_path = uri.strip_prefix("file://")?.to_string();

    let range = location.get("range")?;
    let start = range.get("start")?;
    let line = start.get("line")?.as_u64()? as u32;
    let character = start.get("character")?.as_u64()? as u32;

    let callable = Some(matches!(
        kind_str.as_str(),
        "Function" | "Method" | "Constructor"
    ));
    Some(crate::lsp::CachedSymbol {
        name,
        kind: kind_str,
        container_name,
        file_path: std::path::PathBuf::from(file_path),
        line,
        character,
        source_path: None,
        import_path: None,
        export_kind: None,
        callable,
        has_parameters: None,
    })
}

/// Parse một raw TextEdit JSON object thành LspTextEdit.
fn parse_single_text_edit(edit: &Value) -> Option<LspTextEdit> {
    let new_text = edit.get("newText")?.as_str()?.to_string();
    parse_lsp_text_edit_parts(edit.get("range")?, new_text)
}

/// Parse edits từ WorkspaceEdit — hỗ trợ cả format cũ `changes` và format mới `documentChanges`.
/// TypeScript LSP (tsserver) dùng `documentChanges`; các LSP khác dùng `changes`.
fn parse_workspace_edit_into_edits(workspace_edit: &Value) -> Vec<LspTextEdit> {
    let mut edits = Vec::new();

    // Format mới: documentChanges: TextDocumentEdit[]
    // Mỗi entry có dạng { textDocument: {...}, edits: TextEdit[] }
    if let Some(doc_changes) = workspace_edit
        .get("documentChanges")
        .and_then(|v| v.as_array())
    {
        for doc_edit in doc_changes {
            if let Some(text_edits) = doc_edit.get("edits").and_then(|v| v.as_array()) {
                for edit in text_edits {
                    if let Some(lsp_edit) = parse_single_text_edit(edit) {
                        edits.push(lsp_edit);
                    }
                }
            }
        }
    }

    // Format cũ: changes: { uri: TextEdit[] }
    if edits.is_empty() {
        if let Some(changes) = workspace_edit.get("changes").and_then(|c| c.as_object()) {
            for (_uri, text_edits) in changes {
                if let Some(edit_array) = text_edits.as_array() {
                    for edit in edit_array {
                        if let Some(lsp_edit) = parse_single_text_edit(edit) {
                            edits.push(lsp_edit);
                        }
                    }
                }
            }
        }
    }

    edits
}

fn parse_edits_from_action(action: &Value) -> Vec<LspTextEdit> {
    action
        .get("edit")
        .map(parse_workspace_edit_into_edits)
        .unwrap_or_default()
}

fn parse_code_actions(result: &Value) -> Vec<LspCodeAction> {
    let Some(items) = result.as_array() else {
        return Vec::new();
    };

    items
        .iter()
        .filter_map(|item| {
            let title = item.get("title")?.as_str()?.to_string();

            // Extract workspace edit — hỗ trợ cả `changes` và `documentChanges`.
            let edits = item
                .get("edit")
                .map(parse_workspace_edit_into_edits)
                .unwrap_or_default();

            let has_edits = !edits.is_empty();
            Some(LspCodeAction {
                title,
                edits,
                raw_action: if has_edits {
                    None
                } else {
                    Some(serde_json::to_string(item).unwrap_or_default())
                },
            })
        })
        .collect()
}
