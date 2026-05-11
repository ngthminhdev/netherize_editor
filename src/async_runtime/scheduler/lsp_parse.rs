use std::sync::Arc;

use serde_json::Value;

use crate::{
    async_runtime::message::{
        LspCodeAction, LspCompletionItem, LspDiagnostic, LspDocumentHighlight,
        LspDocumentSymbol, LspLocation, LspPosition, LspRange, LspTextEdit,
        WorkerResultPayload,
    },
    lsp::client::LspClientProcess,
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
                .filter_map(|item| {
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
                    let text_edit_text = item
                        .get("textEdit")
                        .and_then(|edit| edit.get("newText"))
                        .and_then(Value::as_str)
                        .map(str::to_string);
                    let kind = item
                        .get("kind")
                        .and_then(Value::as_u64)
                        .map(|kind| kind as u32);
                    let documentation = item
                        .get("documentation")
                        .and_then(parse_documentation_field);
                    let raw_json = serde_json::to_string(item).ok();
                    Some(LspCompletionItem {
                        label,
                        detail,
                        insert_text,
                        text_edit_text,
                        kind,
                        documentation,
                        raw_json,
                    })
                })
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
    let params = serde_json::json!({
        "textDocument": { "uri": uri },
        "position": { "line": line, "character": character }
    });
    let response = lsp_cancellable_request_response(
        session,
        "hover",
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

    let flush_code = |blocks: &mut Vec<HoverDocBlock>,
                      code_lines: &mut Vec<String>,
                      code_language: &str| {
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
                item.as_str()
                    .map(str::to_string)
                    .or_else(|| {
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
            let kind = item.get("kind").and_then(Value::as_u64).map(|kind| kind as u32);
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

    fn push_nested(out: &mut Vec<LspDocumentSymbol>, symbols: &[lsp_types::DocumentSymbol]) {
        for symbol in symbols {
            out.push(LspDocumentSymbol {
                name: symbol.name.clone(),
                kind: kind_label(&symbol.kind),
                range: range_from_lsp(&symbol.range),
            });
            if let Some(children) = &symbol.children {
                push_nested(out, children);
            }
        }
    }

    let mut out = Vec::new();
    match response {
        lsp_types::DocumentSymbolResponse::Nested(symbols) => push_nested(&mut out, &symbols),
        lsp_types::DocumentSymbolResponse::Flat(symbols) => {
            out.extend(symbols.into_iter().map(|symbol| LspDocumentSymbol {
                name: symbol.name,
                kind: kind_label(&symbol.kind),
                range: range_from_lsp(&symbol.location.range),
            }));
        }
    }
    out
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
    if result.is_null() {
        return Err("completion: no items returned".to_string());
    }
    let items = parse_completion_items(result);
    if items.is_empty() {
        return Err("completion: empty completion list".to_string());
    }
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

/// Parse một raw TextEdit JSON object thành LspTextEdit.
fn parse_single_text_edit(edit: &Value) -> Option<LspTextEdit> {
    let start_line = edit.pointer("/range/start/line")?.as_u64()? as u32;
    let start_char = edit.pointer("/range/start/character")?.as_u64()? as u32;
    let end_line = edit.pointer("/range/end/line")?.as_u64()? as u32;
    let end_char = edit.pointer("/range/end/character")?.as_u64()? as u32;
    let new_text = edit.get("newText")?.as_str()?.to_string();
    Some(LspTextEdit {
        range: LspRange {
            start: LspPosition {
                line: start_line,
                character: start_char,
            },
            end: LspPosition {
                line: end_line,
                character: end_char,
            },
        },
        new_text,
    })
}

/// Parse edits từ WorkspaceEdit — hỗ trợ cả format cũ `changes` và format mới `documentChanges`.
/// TypeScript LSP (tsserver) dùng `documentChanges`; các LSP khác dùng `changes`.
fn parse_workspace_edit_into_edits(workspace_edit: &Value) -> Vec<LspTextEdit> {
    let mut edits = Vec::new();

    // Format mới: documentChanges: TextDocumentEdit[]
    // Mỗi entry có dạng { textDocument: {...}, edits: TextEdit[] }
    if let Some(doc_changes) = workspace_edit.get("documentChanges").and_then(|v| v.as_array()) {
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
