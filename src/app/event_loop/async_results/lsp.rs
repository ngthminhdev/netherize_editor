use super::super::*;
use crate::{app::app_state::FloatingBoxScrollState, async_runtime::message::WorkerResultPayload};
use std::path::{Path, PathBuf};

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
            // A companion server (e.g. ruff) runs alongside the primary. It must
            // NOT take over `active_lsp_server` (which routes completion/hover to
            // pyright) or contribute trigger chars; it only needs the document
            // synced so it can publish lint diagnostics and serve formatting.
            if crate::lsp::registry::binary_is_companion(&server_name) {
                eprintln!(
                    "[AppShell] companion LSP '{}' ready for {}",
                    server_name,
                    root_path.display()
                );
                app.submit_lsp_did_open_for_active_file();
                return;
            }
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

            // A canvas opened (F8) before the server was ready deferred its
            // source-function fetch; fire it now so the "Loading…" state resolves
            // automatically without the user re-pressing F8.
            if app.canvas_def_deferred && app.app_state.is_canvas_active() {
                app.canvas_def_deferred = false;
                app.canvas_submit_definition();
                app.request_redraw();
            }

            if app.panel_state.left.visible
                && app.panel_state.left.active_tab_id() == Some(PanelTabId::Outline)
            {
                app.outline_fetch_path = None;
                app.ensure_outline_symbols();
                app.sidebar_needs_layout = true;
                app.request_redraw();
            }

            // Trigger workspace symbol indexing for fast import suggestions
            if let Some(language_id) = app.app_state.active_file().and_then(|path| {
                crate::lsp::registry::language_profile_for_path(std::path::Path::new(path))
                    .map(|profile| profile.key.to_string())
            }) {
                app.submit_workspace_symbol_indexing(language_id);
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

            // A user-requested restart shuts the old session down first, then
            // spawns the fresh server here — once the stop has fully completed —
            // so the new spawn can't race the shutdown's concurrent drain.
            if std::mem::take(&mut app.pending_lsp_restart) {
                app.sync_lsp_server_for_workspace();
            }
        }
        WorkerResultPayload::LspDiagnostics {
            uri,
            diagnostics,
            server_name,
            ..
        } => {
            if let Some(path) =
                lsp_uri_to_path(&uri).and_then(|path| path.canonicalize().ok().or(Some(path)))
            {
                if is_dependency_diagnostic_path(&path) {
                    return;
                }

                if !diagnostics.is_empty() {
                    eprintln!(
                        "[AppShell] LSP diagnostics: {} issue(s) in {uri}",
                        diagnostics.len()
                    );
                }

                let is_active_file = app
                    .app_state
                    .active_file()
                    .is_some_and(|active| active == path.as_path());
                if app
                    .app_state
                    .set_file_diagnostics(path, server_name, diagnostics)
                {
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
            // In-card hover (Phase 2): a card-issued `K` hover fills the card
            // overlay (a doc box at the caret), never the main editor's FloatingBox.
            if app.canvas_hover_request_id == Some(request_id) {
                app.canvas_hover_request_id = None;
                if content.is_empty() {
                    return;
                }
                let blocks = match parsed_blocks {
                    Some(raw) => convert_worker_hover_blocks(raw, &app.theme),
                    None => parse_hover_markdown_blocks(&content, &app.theme),
                };
                let lines = flatten_hover_blocks_to_lines(&blocks);
                if lines.is_empty() {
                    return;
                }
                if app.app_state.canvas_set_card_hover(lines) {
                    app.request_redraw();
                }
                return;
            }
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
            } else {
                // Drop stale hover responses (from old cursor positions/edits)
                if app
                    .latest_hover_request_id
                    .is_some_and(|latest| latest != request_id)
                {
                    return;
                }
                if app.latest_hover_request_id == Some(request_id) {
                    app.latest_hover_request_id = None;
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
                    scroll: FloatingBoxScrollState { offset_lines: 0 },
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
            // NetherCanvas redirect: a canvas-issued definition request becomes a
            // Definition block instead of the editor peek overlay.
            if app.canvas_def_request_id == Some(request_id) {
                app.canvas_def_request_id = None;
                let parent = app.canvas_def_parent.take();
                attach_canvas_relations(
                    app,
                    locations,
                    crate::canvas::BlockRelation::Definition,
                    parent,
                );
                return;
            }
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
                let target_col = loc.character as usize;
                app.app_state.jump_to_line_col(target_line, target_col);
                let vp = app.editor_viewport_lines();
                app.app_state.auto_scroll_to_cursor(vp);
                app.invalidate_highlights_and_parse_active_buffer();
                app.submit_lsp_check_for_path(path);
                app.submit_lsp_did_open_for_active_file();
                // gd is a jump too: re-run the cursor-dependent reactions so the
                // symbol highlight + breadcrumb (and canvas context) land at the
                // new location without an hjkl nudge. See bug-018.
                app.react_to_cursor_jump();
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
                            scroll: FloatingBoxScrollState { offset_lines: 0 },
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
            // NetherCanvas redirect: canvas-issued references become Caller blocks.
            if app.canvas_refs_request_id == Some(request_id) {
                app.canvas_refs_request_id = None;
                let parent = app.canvas_refs_parent.take();
                attach_canvas_relations(
                    app,
                    locations,
                    crate::canvas::BlockRelation::Caller,
                    parent,
                );
                return;
            }
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
        WorkerResultPayload::LspRenameResult {
            uri,
            edits,
            other_file_edit_count,
        } => {
            if app
                .latest_rename_request_id
                .is_some_and(|latest| latest != request_id)
            {
                eprintln!(
                    "[AppShell] dropping stale LSP rename response request_id={request_id} latest={:?}",
                    app.latest_rename_request_id
                );
                return;
            }
            if revision_id < app.lsp_rename_request_revision {
                eprintln!(
                    "[AppShell] stale rename result ignored request_id={} revision={} latest_revision={}",
                    request_id, revision_id, app.lsp_rename_request_revision
                );
                return;
            }
            if app.latest_rename_request_id == Some(request_id) {
                app.latest_rename_request_id = None;
            }
            if other_file_edit_count > 0 {
                app.show_transient_toast(format!(
                    "Rename skipped: {other_file_edit_count} edit(s) are outside the active file"
                ));
                app.request_redraw();
                return;
            }
            if edits.is_empty() {
                app.show_transient_toast("Rename: no edits returned".to_string());
                app.request_redraw();
                return;
            }
            let Some(path) = lsp_uri_to_path(&uri) else {
                eprintln!("[AppShell] LSP rename: cannot parse URI {uri}");
                return;
            };
            let Some(active_path) = app.app_state.active_file().map(PathBuf::from) else {
                return;
            };
            if active_path != path {
                return;
            }

            let text = app.app_state.text_string();
            let next = match apply_lsp_text_edits(&text, &edits) {
                Ok(next) => next,
                Err(err) => {
                    app.show_transient_toast(format!("Rename failed: {err}"));
                    app.request_redraw();
                    return;
                }
            };
            if app
                .app_state
                .replace_active_document_text_preserve_cursor_with_undo(&next)
            {
                app.editor_needs_layout = true;
                app.editor_caret_needs_layout = true;
                app.submit_parse_for_active_buffer(true);
                app.force_flush_lsp_did_change_for_active_file();
                app.show_transient_toast(format!("Renamed {} occurrence(s)", edits.len()));
                app.request_redraw();
            } else {
                app.show_transient_toast("Rename: no changes".to_string());
                app.request_redraw();
            }
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
                let _ = app.clear_document_symbol_breadcrumb_cache();
                app.request_redraw();
                return;
            };
            if active_path != path {
                let _ = app.app_state.finish_document_symbol_picker_loading();
                app.request_redraw();
                return;
            }
            app.cached_document_symbols_path = Some(path);
            app.cached_document_symbols = symbols.clone();
            app.sidebar_needs_layout = true;
            if app.app_state.command_palette_mode() != Some(CommandPaletteMode::DocumentSymbols) {
                app.editor_needs_layout = true;
                app.editor_caret_needs_layout = false;
                app.request_redraw();
                return;
            }
            if app.app_state.set_document_symbol_picker_results(symbols) {
                app.editor_needs_layout = true;
                app.editor_caret_needs_layout = false;
            }
            app.request_redraw();
        }
        WorkerResultPayload::CanvasCardScopeResult {
            card_id,
            uri,
            symbols,
        } => {
            // Progressive scope refine for a spawned def/caller card whose target
            // file wasn't the active file (so symbols weren't cached at spawn).
            // Stale results are harmlessly dropped: `canvas_card_scope_target`
            // returns None when the card was closed / opened-as-tab / canvas gone.
            let Some(path) = lsp_uri_to_path(&uri) else {
                return;
            };
            let Some((line, character, _symbol)) = app.app_state.canvas_card_scope_target(card_id)
            else {
                return;
            };
            // No enclosing definition — either the LSP returned an EMPTY list
            // (server not ready / file not open / timeout) or the line sits in no
            // function/class. Either way KEEP the card's spawn-time ±N window (the
            // documented best-effort contract). The old `(0, total)` fallback
            // collapsed to `(0, 0)` on empty symbols and showed only the file's
            // first import line — flaky for constructors in cold-LSP files (bug-239).
            let Some((_o, snap, range, rows)) =
                super::canvas_scope::scope_snapshot(&app.theme, &symbols, &path, line, character)
            else {
                return;
            };
            if app
                .app_state
                .canvas_apply_card_scope(card_id, snap, Some(range), rows)
            {
                app.request_redraw();
            }
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
                .replace_active_document_text_preserve_cursor_with_undo(&formatted);
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
                    icon: None,
                    action: CommandPaletteAction::ApplyCodeAction(i),
                    tone: CommandPaletteItemTone::Default,
                    preview_colors: Vec::new(),
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
            // In-card completion (Phase 3): a card-issued request fills the card
            // menu (card-scoped state), never the main editor's completion.
            if app.canvas_completion_request_id == Some(request_id) {
                app.canvas_completion_request_id = None;
                if app.app_state.canvas_edit_session_block().is_none()
                    || app.app_state.current_mode() != EditorMode::Insert
                {
                    return;
                }
                let language_id = app.app_state.canvas_edit_session_path().and_then(|p| {
                    crate::lsp::registry::language_profile_for_path(&p).map(|pr| pr.key.to_string())
                });
                let completion = crate::app::app_state::CompletionState::from_lsp_items(
                    items,
                    cursor_line,
                    cursor_col,
                    prefix_start_col,
                    prefix,
                    app.app_state.workspace_symbol_cache(),
                    language_id.as_deref(),
                );
                if completion.filtered_items.is_empty() {
                    app.dismiss_canvas_card_completion();
                    return;
                }
                app.canvas_set_card_completion(completion);
                app.request_redraw();
                return;
            }
            if app.active_lsp_completion_request_id != Some(request_id) {
                return;
            }
            app.active_lsp_completion_request_id = None;
            app.app_state.set_completion_loading(false);
            if app.app_state.current_mode() != EditorMode::Insert {
                return;
            }
            let (current_line, current_col) = app.app_state.cursor_line_col();
            let current_prefix = app
                .app_state
                .completion_prefix_info_at(current_line, current_col);
            if current_line != cursor_line
                || current_col != cursor_col
                || current_prefix.start_col != prefix_start_col
                || current_prefix.prefix != prefix
            {
                return;
            }
            // Get language_id from active file for workspace symbol fallback
            let language_id = app.app_state.active_file().and_then(|path| {
                crate::lsp::registry::language_profile_for_path(std::path::Path::new(path))
                    .map(|profile| profile.key.to_string())
            });

            let completion = crate::app::app_state::CompletionState::from_lsp_items(
                items,
                cursor_line,
                cursor_col,
                prefix_start_col,
                prefix,
                app.app_state.workspace_symbol_cache(),
                language_id.as_deref(),
            );
            if completion.filtered_items.is_empty() {
                return;
            }
            // Ghost text wins: if an AI inline suggestion is already visible, do
            // not pop the LSP completion menu over it. The next keystroke clears
            // the ghost text and re-requests completion, so the menu comes back
            // once the ghost text is gone.
            if app.app_state.inline_suggestion().is_some() {
                return;
            }
            let changed = app.app_state.set_completion(completion);
            if changed {
                app.submit_completion_resolve();
                app.maybe_request_ai_completion_rerank();
                app.editor_caret_needs_layout = true;
                app.request_redraw();
            }
        }
        WorkerResultPayload::LspCompletionResolveResult {
            item_label,
            detail,
            documentation,
            resolved_item,
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
            if let Some(resolved_item) = resolved_item {
                app.app_state
                    .update_completion_item_from_resolve(&item_label, resolved_item);
            }
            let cleaned_detail = detail.filter(|d| !d.trim().is_empty());
            if cleaned_detail.is_some() {
                app.app_state
                    .update_completion_item_detail(&item_label, cleaned_detail);
            }
            let should_accept_after_resolve = app
                .pending_completion_accept_after_resolve
                .as_ref()
                .is_some_and(|(label, revision)| {
                    label == &item_label && *revision == completion_revision
                });
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
                app.submit_completion_virtual_hover_fallback(item_label, completion_revision);
            }
            if should_accept_after_resolve {
                let _ = app.accept_completion_item();
                return;
            }
            app.editor_caret_needs_layout = true;
            app.request_redraw();
        }
        WorkerResultPayload::WorkspaceSymbols {
            language_id,
            symbols,
        } => {
            let count = symbols.len();
            app.app_state
                .workspace_symbol_cache()
                .insert_symbols(&language_id, symbols);
            eprintln!(
                "[AppShell] indexed {} workspace symbols for {}",
                count, language_id
            );
            app.editor_caret_needs_layout = true;
            app.request_redraw();
        }
        _ => {}
    }
}

fn is_dependency_diagnostic_path(path: &Path) -> bool {
    let path = path.to_string_lossy().replace('\\', "/");
    let in_fvm_version = path.contains("/fvm/versions/") || path.contains("/.fvm/versions/");
    path.contains("/node_modules/")
        || path.contains("/go/src/")
        || path.contains("/.rustup/")
        || path.contains("/site-packages/")
        || path.contains("/dist-packages/")
        || path.contains("/lib/python")
        || path.contains("/vendor/")
        || path.contains("/.pub-cache/")
        || path.contains("/.dart_tool/")
        || path.contains("/.fvm/flutter_sdk/")
        || path.contains("/flutter/bin/cache/dart-sdk/")
        || (in_fvm_version
            && (path.contains("/packages/flutter/") || path.contains("/bin/cache/dart-sdk/")))
}

pub(super) fn handle_stale_result(app: &mut AppShell, stale: WorkerResult) {
    match stale.payload {
        WorkerResultPayload::LspReferencesResult { .. } => {
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
        WorkerResultPayload::LspRenameResult { .. } => {
            if app.latest_rename_request_id == Some(stale.request_id) {
                app.latest_rename_request_id = None;
            }
            eprintln!(
                "[AppShell] bridge discarded stale rename result request_id={} revision={} latest_revision={}",
                stale.request_id, stale.revision_id, app.lsp_rename_request_revision
            );
        }
        _ => {}
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

/// Syntax-highlight a code snippet and pack the spans into the canvas-core
/// [`crate::canvas::CanvasSpan`] form. Shared by relation-card snapshots
/// (read from disk) and the live edit-card snapshot (read from the buffer).
pub(crate) fn canvas_snapshot_spans(
    theme: &crate::config::theme_config::ThemeConfig,
    text: &str,
    extension: &str,
) -> Vec<crate::canvas::CanvasSpan> {
    use crate::canvas::CanvasSpan;
    syntax_spans_to_styled(
        &crate::syntax::highlight::highlight_snippet(text, extension, theme),
        text,
        theme,
    )
    .into_iter()
    .map(|s| CanvasSpan {
        start: s.start,
        end: s.end,
        color: s.color_rgba,
        bold: s.bold,
        italic: s.italic,
    })
    .collect()
}

/// Build a relation card's `(origin, snapshot)` from a file location: read
/// `context` lines around `line`, syntax-highlight them, and pack the spans
/// into the canvas-core [`crate::canvas::CanvasSpan`] form. Shared by the
/// initial LSP attach and the `+`/`-` context re-source so both produce
/// identical snapshots. Returns `None` when the file can't be read (no lines).
pub(crate) fn build_canvas_relation_snapshot(
    theme: &crate::config::theme_config::ThemeConfig,
    path: &Path,
    line: u32,
    character: u32,
    symbol: &str,
    context: usize,
) -> Option<(crate::canvas::BlockOrigin, crate::canvas::BlockSnapshot)> {
    use crate::canvas::{BlockOrigin, BlockSnapshot};
    let (lines, start0) = super::preview::read_file_lines(path, line as usize, context);
    if lines.is_empty() {
        return None;
    }
    let text = lines.join("\n");
    let extension = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or_default();
    let spans = canvas_snapshot_spans(theme, &text, extension);
    let file = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    Some((
        BlockOrigin {
            path: path.to_path_buf(),
            start_byte: 0,
            end_byte: 0,
            symbol_name: symbol.to_string(),
            lsp_line: line,
            lsp_character: character,
        },
        BlockSnapshot {
            title: file,
            symbol: symbol.to_string(),
            start_line: start0 as u32 + 1,
            text,
            spans,
        },
    ))
}

/// Like `build_canvas_relation_snapshot` but for an explicit enclosing-symbol line
/// range (scope-aware). Clamps to 60 lines + a "+N more" marker so a huge function
/// never makes a giant card; the full body is available on Enter-to-edit.
pub(crate) fn build_canvas_relation_snapshot_range(
    theme: &crate::config::theme_config::ThemeConfig,
    path: &Path,
    start_line: u32,
    end_line: u32,
    character: u32,
    symbol: &str,
) -> Option<(crate::canvas::BlockOrigin, crate::canvas::BlockSnapshot)> {
    use crate::canvas::{BlockOrigin, BlockSnapshot};
    const MAX_LINES: usize = 60;
    let (mut lines, start0, total) = super::preview::read_file_lines_range(
        path,
        start_line as usize,
        end_line as usize,
        MAX_LINES,
    );
    if lines.is_empty() {
        return None;
    }
    if total > MAX_LINES {
        lines.push(format!("    \u{2026} +{} more", total - MAX_LINES));
    }
    let text = lines.join("\n");
    let extension = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or_default();
    let spans = canvas_snapshot_spans(theme, &text, extension);
    let file = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    Some((
        BlockOrigin {
            path: path.to_path_buf(),
            start_byte: 0,
            end_byte: 0,
            symbol_name: symbol.to_string(),
            lsp_line: start_line,
            lsp_character: character,
        },
        BlockSnapshot {
            title: file,
            symbol: symbol.to_string(),
            start_line: start0 as u32 + 1,
            text,
            spans,
        },
    ))
}

/// Convert LSP locations into NetherCanvas relation blocks and append them.
/// `Definition` keeps the first location; `Caller` keeps several. Each block's
/// snapshot is a read-only preview of the lines around the location, sized to
/// the canvas's current context-line count.
fn attach_canvas_relations(
    app: &mut AppShell,
    locations: Vec<crate::async_runtime::message::LspLocation>,
    relation: crate::canvas::BlockRelation,
    parent: Option<crate::canvas::BlockId>,
) {
    use crate::canvas::BlockRelation;
    let cap = if relation == BlockRelation::Caller {
        6
    } else {
        1
    };
    let context = app.app_state.canvas_context_lines();
    // Snapshot the ids present before the spawn so we can identify the newly-added
    // cards (for progressive scope resolution) without poking the private next_id.
    let pre_ids: Vec<crate::canvas::BlockId> = app
        .app_state
        .canvas()
        .map(|c| c.blocks.iter().map(|b| b.id).collect())
        .unwrap_or_default();
    let mut rels = Vec::new();
    for loc in locations.into_iter().take(cap) {
        let Some(path) = lsp_uri_to_path(&loc.uri) else {
            continue;
        };
        // Spawn with the ±N window; the scope refine below runs in this SAME
        // handler for cached files (no frame in between → no ±N flash), async
        // otherwise.
        if let Some((origin, snapshot)) =
            build_canvas_relation_snapshot(&app.theme, &path, loc.line, loc.character, "", context)
        {
            rels.push((relation, origin, snapshot, None));
        }
    }
    if rels.is_empty() {
        return;
    }
    if app.app_state.canvas_add_relations_with_parent(rels, parent) {
        // In-card gd/gr: focus already jumped to the spawned child card. Leave
        // EditCard (→ Navigate) so the focus visibly lands on the child instead of
        // waiting for an Esc, and release the parent card's LSP document. Only when
        // we're still editing the very card that spawned this (don't yank focus if
        // the user has moved on to another card meanwhile).
        if let Some(parent_id) = parent
            && app.app_state.canvas_end_edit_for_spawn(parent_id)
        {
            app.dismiss_canvas_card_completion();
            app.canvas_hover_request_id = None;
            app.submit_canvas_card_did_close();
        }
        // Progressive scope resolution (T4/T5): for each newly-spawned card, if its
        // target file's documentSymbols are already cached (the active file),
        // resolve the enclosing-definition scope synchronously right now (one
        // reflow, no async request). Other cards get an async CanvasCardScopeRequest.
        let cached_path = app.cached_document_symbols_path.clone();
        let cached_symbols = app.cached_document_symbols.clone();
        let new_cards: Vec<(crate::canvas::BlockId, std::path::PathBuf, u32, u32)> = app
            .app_state
            .canvas()
            .map(|c| {
                c.blocks
                    .iter()
                    .filter(|b| {
                        !pre_ids.contains(&b.id)
                            && b.relation != BlockRelation::Focal
                            && b.scope_lines.is_none()
                    })
                    .map(|b| {
                        (
                            b.id,
                            b.origin.path.clone(),
                            b.origin.lsp_line,
                            b.origin.lsp_character,
                        )
                    })
                    .collect()
            })
            .unwrap_or_default();
        for (id, path, line, character) in new_cards {
            let is_cached = cached_path.as_ref().is_some_and(|p| path == *p);
            if is_cached {
                // Sync resolve: symbols already in memory → one reflow, no request.
                // Titles the card by the TARGET symbol it lands in (bug-021).
                if let Some((_o, snap, range, rows)) = super::canvas_scope::scope_snapshot(
                    &app.theme,
                    &cached_symbols,
                    &path,
                    line,
                    character,
                ) {
                    app.app_state
                        .canvas_apply_card_scope(id, snap, Some(range), rows);
                }
            } else if let Some((snap, range, rows)) =
                super::canvas_scope::local_scope_snapshot(&app.theme, &path, line, character)
            {
                // Local tree-sitter scope: instant, no ±N flash, no LSP round-trip
                // (documentSymbol is EMPTY for files the server hasn't opened, so
                // the async path would leave the ±N window — imports and all —
                // until Enter didOpen'd the file and re-asked: a visible jump).
                app.app_state
                    .canvas_apply_card_scope(id, snap, Some(range), rows);
            } else {
                // Async resolve: ask the worker for documentSymbol of the card's
                // target file, correlated by `card_id`. Best-effort — an empty
                // result keeps the ±N window (no regression).
                let uri = crate::lsp::client::path_to_lsp_uri(&path);
                app.submit(RequestSpec {
                    revision_id: 0,
                    topic: RequestTopic::LspRequest,
                    payload: WorkerRequestPayload::CanvasCardScopeRequest { card_id: id, uri },
                });
            }
        }
        let w = app.window_size.width as f32;
        let (pane_top, pane_h) = app.canvas_pane_metrics();
        // Tidy the freshly-spawned cards into columns that fit the VISIBLE canvas
        // pane (in world units, so they wrap inward instead of overflowing), no-op
        // once the user has hand-arranged; then anchor the camera so the whole
        // cluster sits inside the pane.
        let arrange_h = app.canvas_arrange_viewport_height();
        app.app_state.canvas_auto_arrange(arrange_h);
        app.app_state.canvas_anchor_cards_right(w, pane_top, pane_h);
        app.request_redraw();
    }
}

/// Flatten hover doc blocks into plain display lines for the card overlay box
/// (the card popup renders monospace lines, not the rich main-editor FloatingBox).
fn flatten_hover_blocks_to_lines(
    blocks: &[crate::app::app_state::FloatingBoxBlock],
) -> Vec<String> {
    use crate::app::app_state::FloatingBoxBlock;
    let mut lines = Vec::new();
    for block in blocks {
        match block {
            FloatingBoxBlock::Prose(text) | FloatingBoxBlock::Code { text, .. } => {
                for line in text.split('\n') {
                    lines.push(line.to_string());
                }
            }
        }
    }
    while lines.last().is_some_and(|l| l.trim().is_empty()) {
        lines.pop();
    }
    lines
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

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::is_dependency_diagnostic_path;

    #[test]
    fn dependency_diagnostic_filter_ignores_flutter_sdk_paths() {
        assert!(is_dependency_diagnostic_path(Path::new(
            "/Users/qc-bright/fvm/versions/3.35.3/packages/flutter/lib/src/material/button.dart"
        )));
        assert!(is_dependency_diagnostic_path(Path::new(
            "/Users/qc-bright/.fvm/versions/3.35.3/bin/cache/dart-sdk/lib/core/core.dart"
        )));
        assert!(is_dependency_diagnostic_path(Path::new(
            "/workspace/.fvm/flutter_sdk/packages/flutter/lib/src/material/button.dart"
        )));
        assert!(is_dependency_diagnostic_path(Path::new(
            "/workspace/.fvm/flutter_sdk/bin/cache/dart-sdk/lib/core/core.dart"
        )));
        assert!(is_dependency_diagnostic_path(Path::new(
            "/Users/qc-bright/.pub-cache/hosted/pub.dev/provider-6.1.0/lib/provider.dart"
        )));
        assert!(!is_dependency_diagnostic_path(Path::new(
            "/workspace/lib/src/material/button.dart"
        )));
    }

    #[test]
    fn snapshot_range_reads_exact_lines_and_clamps() {
        use std::time::{SystemTime, UNIX_EPOCH};
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("netherize_scope_{nanos}"));
        std::fs::create_dir_all(&dir).unwrap();
        let f = dir.join("a.rs");
        let body = (0..100)
            .map(|i| format!("line{i}"))
            .collect::<Vec<_>>()
            .join("\n");
        std::fs::write(&f, body).unwrap();

        let theme = crate::config::theme_config::ThemeConfig::builtin_dark();
        // Range lines 10..=80 (0-based) is 71 lines → clamp to 60 + "+N more".
        let (_o, snap) =
            super::build_canvas_relation_snapshot_range(&theme, &f, 10, 80, 0, "s").unwrap();
        assert_eq!(snap.start_line, 11); // 1-based
        let n = snap.text.split('\n').count();
        assert!(n <= 61, "clamped to <=60 lines + marker, got {n}");
        assert!(
            snap.text.contains("+") && snap.text.contains("more"),
            "has +N more marker"
        );

        std::fs::remove_dir_all(&dir).ok();
    }
}
