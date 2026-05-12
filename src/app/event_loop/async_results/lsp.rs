use super::super::*;
use crate::async_runtime::message::WorkerResultPayload;
use std::path::PathBuf;

pub(super) fn handle_lsp_result(
    app: &mut AppShell,
    payload: WorkerResultPayload,
    request_id: u64,
    revision_id: u64,
) {
    match payload {
        WorkerResultPayload::LspServerStarted {
            server_name,
            root_path,
            completion_trigger_chars,
        } => {
            let started = ActiveLspServer {
                server_name: server_name.clone(),
                root_path: root_path.clone(),
            };
            app.active_lsp_server = Some(started.clone());
            app.lsp_completion_trigger_chars = completion_trigger_chars.clone();
            if app.pending_lsp_server.as_ref() == Some(&started) {
                app.pending_lsp_server = None;
            }
            eprintln!(
                "[AppShell] LSP '{}' ready for {}",
                server_name,
                root_path.display()
            );
            if app.pending_lsp_document_sync.is_some() {
                let _ = app.force_flush_lsp_did_change_for_active_file();
            } else {
                app.submit_lsp_did_open_for_active_file();
            }
        }
        WorkerResultPayload::LspServerStopped { .. } => {
            if let Some(server) = app.active_lsp_server.take() {
                if app
                    .app_state
                    .clear_lsp_progress_for_server(&server.server_name)
                {
                    app.request_redraw();
                }
            }
            app.pending_lsp_document_sync = None;
            app.lsp_completion_trigger_chars.clear();
        }
        WorkerResultPayload::LspDiagnostics {
            uri, diagnostics, ..
        } => {
            eprintln!(
                "[AppShell] LSP diagnostics: {} issue(s) in {uri}",
                diagnostics.len()
            );
            if let Some(path) =
                lsp_uri_to_path(&uri).and_then(|path| path.canonicalize().ok().or(Some(path)))
            {
                let is_active_file = app
                    .app_state
                    .active_file()
                    .is_some_and(|active| active == path.as_path());
                if app.app_state.set_file_diagnostics(path, diagnostics) {
                    app.editor_needs_layout |=
                        app.app_state.active_buffer_is_diagnostics() || is_active_file;
                    app.editor_caret_needs_layout |= is_active_file;
                }
                app.request_redraw();
            }
        }
        WorkerResultPayload::LspLogMessage { level, message } => {
            eprintln!("[LSP/{level}] {message}");
        }
        WorkerResultPayload::LspProgress {
            server_name,
            token,
            kind,
            title,
            message,
            percentage,
        } => {
            use crate::app::app_state::LspProgressKind;
            use crate::async_runtime::message::LspProgressKindWire;
            let app_kind = match kind {
                LspProgressKindWire::Begin => LspProgressKind::Begin,
                LspProgressKindWire::Report => LspProgressKind::Report,
                LspProgressKindWire::End => LspProgressKind::End,
            };
            let changed = app.app_state.update_lsp_progress(
                &server_name,
                &token,
                app_kind,
                title,
                message,
                percentage,
            );
            if changed {
                app.request_redraw();
            }
        }
        WorkerResultPayload::LspCheckResult {
            binary,
            install_cmd,
            is_installed,
            ..
        } => {
            if !is_installed && !app.dismissed_lsp_binaries.contains(&binary) {
                app.active_lsp_guide = Some(LspInstallGuide {
                    binary,
                    install_cmd,
                });
                app.request_redraw();
            }
        }
        WorkerResultPayload::LspHoverResult {
            content,
            for_completion,
            completion_revision,
            parsed_blocks,
            ..
        } => {
            if app.hover_loading_request_id == Some(request_id) {
                app.hover_loading_request_id = None;
            }
            if for_completion {
                let current_revision = app
                    .app_state
                    .completion()
                    .map(|state| state.current_revision);
                if completion_revision != current_revision {
                    return;
                }
            }
            if content.is_empty() {
                if for_completion {
                    app.app_state.mark_completion_hover_doc_resolved();
                    app.editor_caret_needs_layout = true;
                } else {
                    let changed = app.app_state.clear_current_overlays();
                    if changed {
                        app.editor_caret_needs_layout = true;
                        app.request_redraw();
                    }
                }
                return;
            }
            if for_completion {
                if app.app_state.has_completion() {
                    app.app_state
                        .set_completion_hover_doc(Some(content.clone()));
                    app.editor_caret_needs_layout = true;
                    app.request_redraw();
                }
                return;
            }
            use crate::app::app_state::{EditorOverlay, FloatingBoxStyle};
            let (anchor_line, anchor_col) = app.app_state.cursor_line_col();
            let blocks = match parsed_blocks {
                Some(raw) => convert_worker_hover_blocks(raw, &app.theme),
                None => parse_hover_markdown_blocks(&content, &app.theme),
            };
            if blocks.is_empty() {
                let changed = app.app_state.clear_current_overlays();
                if changed {
                    app.editor_caret_needs_layout = true;
                    app.request_redraw();
                }
                return;
            }
            let changed = app
                .app_state
                .set_current_overlays(vec![EditorOverlay::FloatingBox {
                    anchor_line,
                    anchor_col,
                    blocks,
                    style: FloatingBoxStyle::DocHover,
                }]);
            if changed {
                app.editor_caret_needs_layout = true;
                app.request_redraw();
            }
        }
        WorkerResultPayload::LspDefinitionResult {
            locations, jump, ..
        } => {
            use crate::app::app_state::{EditorOverlay, FloatingBoxStyle};
            if app
                .latest_definition_request_id
                .is_some_and(|latest| latest != request_id)
            {
                eprintln!(
                    "[AppShell] dropping stale LSP definition response request_id={request_id} latest={:?}",
                    app.latest_definition_request_id
                );
                return;
            }
            if app.latest_definition_request_id == Some(request_id) {
                app.latest_definition_request_id = None;
            }
            let Some(loc) = locations.into_iter().next() else {
                eprintln!("[AppShell] LSP definition: no locations");
                return;
            };
            let path = match lsp_uri_to_path(&loc.uri) {
                Some(p) => p,
                None => {
                    eprintln!("[AppShell] LSP definition: cannot parse URI {}", loc.uri);
                    return;
                }
            };
            if jump {
                app.app_state.push_jump();
                if let Err(err) = app.app_state.open_file(path.clone()) {
                    eprintln!("[AppShell] LSP gd open_file failed: {err}");
                    return;
                }
                let target_line = loc.line as usize;
                app.app_state.jump_to_line(target_line);
                let vp = app.editor_viewport_lines();
                app.app_state.auto_scroll_to_cursor(vp);
                app.invalidate_highlights_and_parse_active_buffer();
                app.submit_lsp_check_for_path(path);
                app.submit_lsp_did_open_for_active_file();
                app.editor_needs_layout = true;
                app.request_redraw();
            } else {
                let preview_lines = super::preview::read_file_preview(&path, loc.line as usize, 8);
                if preview_lines.is_empty() {
                    eprintln!(
                        "[AppShell] LSP gD: cannot read preview for {}",
                        path.display()
                    );
                    return;
                }
                let (anchor_line, anchor_col) = app.app_state.cursor_line_col();
                let preview_text = preview_lines.join("\n");
                let extension = path
                    .extension()
                    .and_then(|ext| ext.to_str())
                    .unwrap_or_default();
                let preview_spans = syntax_spans_to_styled(
                    &crate::syntax::highlight::highlight_snippet(
                        &preview_text,
                        extension,
                        &app.theme,
                    ),
                    &preview_text,
                    &app.theme,
                );
                let changed =
                    app.app_state
                        .set_current_overlays(vec![EditorOverlay::FloatingBox {
                            anchor_line,
                            anchor_col,
                            blocks: vec![crate::app::app_state::FloatingBoxBlock::Code {
                                text: preview_text,
                                spans: preview_spans,
                            }],
                            style: FloatingBoxStyle::PeekWindow,
                        }]);
                if changed {
                    app.editor_caret_needs_layout = true;
                    app.request_redraw();
                }
            }
        }
        WorkerResultPayload::LspDocumentHighlightResult { uri, highlights } => {
            if revision_id < app.semantic_highlight_request_revision {
                eprintln!(
                    "[AppShell] stale document highlight result ignored request_id={} revision={} latest_revision={}",
                    request_id, revision_id, app.semantic_highlight_request_revision
                );
                return;
            }
            let Some(path) = lsp_uri_to_path(&uri) else {
                return;
            };
            if app.app_state.active_file().map(PathBuf::from) != Some(path) {
                return;
            }
            let mut next: Vec<(usize, usize)> = highlights
                .into_iter()
                .filter_map(|highlight| {
                    let start_line = highlight.range.start.line as usize;
                    let end_line = highlight.range.end.line as usize;
                    let start_byte = app.app_state.line_char_to_byte_idx(
                        start_line,
                        highlight.range.start.character as usize,
                    );
                    let mut end_byte = app
                        .app_state
                        .line_char_to_byte_idx(end_line, highlight.range.end.character as usize);
                    if end_byte <= start_byte {
                        end_byte = start_byte.saturating_add(1);
                    }
                    (end_byte > start_byte).then_some((start_byte, end_byte))
                })
                .collect();
            if next.is_empty() {
                next = app.app_state.fallback_symbol_highlights_under_cursor();
            }
            if app.app_state.set_semantic_symbol_highlights(next) {
                app.editor_caret_needs_layout = true;
                app.request_redraw();
            }
        }
        WorkerResultPayload::LspReferencesResult { locations, .. } => {
            if revision_id < app.references_request_revision {
                if app
                    .app_state
                    .fail_pending_references_buffer(request_id, stale_references_status())
                {
                    app.editor_needs_layout = true;
                    app.editor_caret_needs_layout = false;
                }
                eprintln!(
                    "[AppShell] stale references result ignored request_id={} revision={} latest_revision={}",
                    request_id, revision_id, app.references_request_revision
                );
                app.request_redraw();
                return;
            }
            let workspace_root = app.app_state.workspace_root_path().map(PathBuf::from);
            let items: Vec<crate::app::app_state::ReferencesBufferItem> = locations
                .iter()
                .filter_map(|loc| {
                    let path = lsp_uri_to_path(&loc.uri)?;
                    let relative_path = workspace_root
                        .as_ref()
                        .and_then(|root| path.strip_prefix(root).ok())
                        .map(|relative| relative.display().to_string())
                        .unwrap_or_else(|| path.display().to_string());
                    Some(crate::app::app_state::ReferencesBufferItem {
                        path,
                        relative_path,
                        line: loc.line as usize,
                        column: loc.character as usize,
                        summary: format!("Ln {}, Col {}", loc.line + 1, loc.character + 1),
                    })
                })
                .collect();
            let title = format!("References ({})", items.len());
            if app
                .app_state
                .finish_pending_references_buffer(request_id, title, items)
            {
                app.submit_references_preview_load();
                app.editor_needs_layout = true;
                app.editor_caret_needs_layout = false;
            }
            app.request_redraw();
        }
        WorkerResultPayload::LspDocumentSymbolsResult { uri, symbols } => {
            if revision_id < app.document_symbols_request_revision {
                eprintln!(
                    "[AppShell] stale document symbols result ignored request_id={} revision={} latest_revision={}",
                    request_id, revision_id, app.document_symbols_request_revision
                );
                return;
            }
            let Some(path) = lsp_uri_to_path(&uri) else {
                eprintln!("[AppShell] document symbols: cannot parse URI {uri}");
                let _ = app.app_state.finish_document_symbol_picker_loading();
                app.request_redraw();
                return;
            };
            let Some(active_path) = app.app_state.active_file().map(PathBuf::from) else {
                let _ = app.app_state.finish_document_symbol_picker_loading();
                app.request_redraw();
                return;
            };
            if active_path != path {
                let _ = app.app_state.finish_document_symbol_picker_loading();
                app.request_redraw();
                return;
            }
            if app.app_state.command_palette_mode() != Some(CommandPaletteMode::DocumentSymbols) {
                return;
            }
            if app.app_state.set_document_symbol_picker_results(symbols) {
                app.editor_needs_layout = true;
                app.editor_caret_needs_layout = false;
            }
            app.request_redraw();
        }
        WorkerResultPayload::LspFormattingResult { uri, edits } => {
            let Some(path) = lsp_uri_to_path(&uri) else {
                eprintln!("[AppShell] LSP formatting: cannot parse URI {uri}");
                return;
            };
            let Some(active_path) = app.app_state.active_file().map(PathBuf::from) else {
                return;
            };
            if active_path != path {
                return;
            }

            let mut formatted = app.app_state.text_string();
            if !edits.is_empty() {
                match apply_lsp_text_edits(&formatted, &edits) {
                    Ok(next) => formatted = next,
                    Err(err) => {
                        eprintln!("[AppShell] LSP formatting apply failed: {err}");
                        return;
                    }
                }
            }

            let changed = app
                .app_state
                .replace_active_document_text_preserve_cursor(&formatted);
            if changed {
                app.editor_needs_layout = true;
                app.editor_caret_needs_layout = true;
                app.submit_parse_for_active_buffer(true);
                app.force_flush_lsp_did_change_for_active_file();
                app.request_redraw();
            }
        }
        WorkerResultPayload::LspCodeActionResult { actions } => {
            if actions.is_empty() {
                return;
            }
            use crate::app::command_palette::{
                CommandPaletteAction, CommandPaletteItem, CommandPaletteItemTone,
            };
            let items: Vec<CommandPaletteItem> = actions
                .iter()
                .enumerate()
                .map(|(i, a)| CommandPaletteItem {
                    label: a.title.clone(),
                    secondary_label: if a.edits.is_empty() {
                        Some("needs resolve".to_string())
                    } else {
                        None
                    },
                    action: CommandPaletteAction::ApplyCodeAction(i),
                    tone: CommandPaletteItemTone::Default,
                })
                .collect();

            app.pending_code_actions = actions;
            app.app_state.open_code_action_picker(items);
            if let Ok(result) = app
                .app_state
                .apply_mode_event(crate::core::mode::ModeEvent::OpenPalette)
            {
                if result.changed {
                    app.editor_needs_layout = true;
                }
            }
            app.focus_manager
                .set(crate::workbench::focus_manager::FocusTarget::OverlayLayer);
            app.input_handler.clear_pending_prefix();
            app.request_redraw();
        }
        WorkerResultPayload::LspCompletionResult {
            items,
            cursor_line,
            cursor_col,
            prefix_start_col,
            prefix,
        } => {
            app.app_state.set_completion_loading(false);
            if items.is_empty() {
                app.request_redraw();
                return;
            }
            let completion = crate::app::app_state::CompletionState::from_lsp_items(
                items,
                cursor_line,
                cursor_col,
                prefix_start_col,
                prefix,
            );
            if completion.filtered_items.is_empty() {
                return;
            }
            let changed = app.app_state.set_completion(completion);
            if changed {
                app.submit_completion_resolve();
                app.editor_caret_needs_layout = true;
                app.request_redraw();
            }
        }
        WorkerResultPayload::LspCompletionResolveResult {
            item_label,
            detail,
            documentation,
            completion_revision,
        } => {
            if app.completion_resolve_request_id == Some(request_id) {
                app.completion_resolve_request_id = None;
            }
            let Some(completion) = app.app_state.completion() else {
                return;
            };
            if completion.current_revision != completion_revision {
                return;
            }
            let cleaned_detail = detail.filter(|d| !d.trim().is_empty());
            if cleaned_detail.is_some() {
                app.app_state
                    .update_completion_item_detail(&item_label, cleaned_detail);
            }
            let Some(completion) = app.app_state.completion() else {
                return;
            };
            let Some(entry) = completion.filtered_items.get(completion.selected_index) else {
                return;
            };
            if entry.item.label != item_label {
                return;
            }
            let cleaned = documentation.filter(|d| !d.trim().is_empty());
            if cleaned.is_some() {
                app.app_state.set_completion_hover_doc(cleaned);
            } else {
                app.app_state.mark_completion_hover_doc_resolved();
            }
            app.editor_caret_needs_layout = true;
            app.request_redraw();
        }
        _ => {}
    }
}

pub(super) fn handle_stale_result(app: &mut AppShell, stale: WorkerResult) {
    if let WorkerResultPayload::LspReferencesResult { .. } = stale.payload {
        if app
            .app_state
            .fail_pending_references_buffer(stale.request_id, stale_references_status())
        {
            app.editor_needs_layout = true;
            app.editor_caret_needs_layout = false;
        }
        eprintln!(
            "[AppShell] bridge discarded stale references result request_id={} revision={} latest_revision={}",
            stale.request_id, stale.revision_id, app.references_request_revision
        );
        app.request_redraw();
    }
}

pub(super) fn handle_lsp_missing_dependency(app: &mut AppShell, tool_name: String) {
    if app.dismissed_lsp_binaries.contains(&tool_name) {
        return;
    }
    let install_cmd = crate::lsp::registry::language_profile_for_binary(&tool_name)
        .map(|p| p.install_command.to_string())
        .unwrap_or_default();
    app.pending_lsp_server = None;
    app.active_lsp_guide = Some(LspInstallGuide {
        binary: tool_name,
        install_cmd,
    });
    app.request_redraw();
}

pub(crate) fn lsp_uri_to_path(uri: &str) -> Option<std::path::PathBuf> {
    let url = url::Url::parse(uri).ok()?;
    let path = url.to_file_path().ok()?;
    path.canonicalize().ok().or(Some(path))
}

pub(super) fn friendly_references_status(message: &str) -> String {
    let normalized = message.trim();
    if normalized.contains("no references found") || normalized.contains("empty result") {
        "No references found".to_string()
    } else if normalized.contains("LSP server not running") {
        "LSP server is not ready yet".to_string()
    } else if normalized.contains("timed out") {
        "References request timed out".to_string()
    } else {
        "References request failed".to_string()
    }
}

pub(super) fn stale_references_status() -> String {
    "References request superseded by newer request".to_string()
}

pub(crate) fn apply_lsp_text_edits(
    source: &str,
    edits: &[crate::async_runtime::message::LspTextEdit],
) -> Result<String, String> {
    fn utf16_code_unit_to_byte_idx(text: &str, utf16_units: u32) -> Option<usize> {
        let target = utf16_units as usize;
        let mut seen = 0usize;
        for (byte_idx, ch) in text.char_indices() {
            if seen == target {
                return Some(byte_idx);
            }
            seen += ch.len_utf16();
            if seen > target {
                return None;
            }
        }
        (seen == target).then_some(text.len())
    }

    fn lsp_position_to_byte_idx(text: &str, line: u32, character: u32) -> Option<usize> {
        let mut lines = text.split_inclusive('\n');
        let mut byte_offset = 0usize;
        for _ in 0..line {
            byte_offset += lines.next()?.len();
        }
        let line_text = lines.next().unwrap_or("");
        let line_without_newline = line_text.strip_suffix('\n').unwrap_or(line_text);
        let byte_in_line = utf16_code_unit_to_byte_idx(line_without_newline, character)
            .or_else(|| utf16_code_unit_to_byte_idx(line_text, character))?;
        Some(byte_offset + byte_in_line)
    }

    let mut resolved = Vec::with_capacity(edits.len());
    for edit in edits {
        let start =
            lsp_position_to_byte_idx(source, edit.range.start.line, edit.range.start.character)
                .ok_or_else(|| "invalid LSP formatting start position".to_string())?;
        let end = lsp_position_to_byte_idx(source, edit.range.end.line, edit.range.end.character)
            .ok_or_else(|| "invalid LSP formatting end position".to_string())?;
        if start > end || end > source.len() {
            return Err("invalid LSP formatting edit range".to_string());
        }
        resolved.push((start, end, edit.new_text.as_str()));
    }

    resolved.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| b.1.cmp(&a.1)));
    let mut result = source.to_string();
    for (start, end, replacement) in resolved {
        result.replace_range(start..end, replacement);
    }
    Ok(result)
}
