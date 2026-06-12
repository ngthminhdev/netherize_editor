use super::super::*;
use crate::async_runtime::message::{RequestTopic, WorkerEvent};

pub(super) fn handle_worker_failure(app: &mut AppShell, event: WorkerEvent) {
    let request_id = event.request_id;
    let revision_id = event.revision_id;
    let topic = event.topic;

    if let crate::async_runtime::message::WorkerEventKind::Failed { error } = event.kind {
        if topic == RequestTopic::TerminalPty
            && let Some(tab_idx) = app.pending_terminal_tab_spawns.remove(&request_id)
        {
            if let Some(tab) = app.terminal_tabs.get_mut(tab_idx) {
                tab.status = TerminalTabStatus::Exited(1);
                tab.label = "terminal failed".to_string();
                app.terminal_needs_layout = true;
            }
        }
        if topic == RequestTopic::LspClient {
            app.pending_lsp_server = None;
        }
        // File watcher keeps restarting forever; it emits Failed exactly once
        // when crossing the degraded threshold so the user learns that live
        // external updates may lag (only the 3s open-buffer poll is guaranteed).
        if topic == RequestTopic::WorkspaceWatch {
            app.show_transient_toast(
                "File watcher degraded — external changes may lag (3s poll fallback)".to_string(),
            );
        }
        // completionItem/resolve failed (e.g. server doesn't advertise resolveProvider).
        // Mark as resolved so the panel shows "No documentation available".
        if app.completion_resolve_request_id == Some(request_id) {
            app.completion_resolve_request_id = None;
            app.app_state.mark_completion_hover_doc_resolved();
            if app.pending_completion_accept_after_resolve.is_some() {
                let _ = app.accept_completion_item();
                return;
            }
            app.editor_caret_needs_layout = true;
            app.request_redraw();
        }
        // Hover request failed — dismiss the loading overlay we showed eagerly.
        if app.hover_loading_request_id == Some(request_id) {
            app.hover_loading_request_id = None;
            let changed = app.app_state.clear_current_overlays();
            if changed {
                app.editor_caret_needs_layout = true;
                app.request_redraw();
            }
        }
        // Definition request failed (timeout, server error, or our own
        // $/cancelRequest racing in). If this was the request we're still
        // waiting on, free the slot so the next `gd` isn't filtered out
        // as "superseded".
        if app.latest_definition_request_id == Some(request_id) {
            app.latest_definition_request_id = None;
        }
        if app.latest_rename_request_id == Some(request_id) {
            app.latest_rename_request_id = None;
            let message = if error.message.contains("renameProvider") {
                "LSP rename is not supported by this server".to_string()
            } else if error.message.contains("LSP server not running") {
                "LSP rename: server is not ready yet".to_string()
            } else if error.message.contains("timed out") {
                "LSP rename timed out".to_string()
            } else {
                "LSP rename failed".to_string()
            };
            app.show_transient_toast(message);
        }
        // Completion request failed — clear the spinner.
        if app.app_state.is_completion_loading() {
            app.app_state.set_completion_loading(false);
            app.request_redraw();
        }
        if topic == RequestTopic::LspRequest
            && revision_id >= app.document_symbols_request_revision
            && app.app_state.command_palette_mode() == Some(CommandPaletteMode::DocumentSymbols)
            && app.app_state.finish_document_symbol_picker_loading()
        {
            app.request_redraw();
        }
        let references_status = if revision_id < app.references_request_revision {
            super::stale_references_status()
        } else {
            super::friendly_references_status(&error.message)
        };
        if app
            .app_state
            .fail_pending_references_buffer(request_id, references_status)
        {
            app.editor_needs_layout = true;
            app.editor_caret_needs_layout = false;
        }
        if topic == RequestTopic::LspRequest && revision_id < app.references_request_revision {
            eprintln!(
                "[AppShell] stale references failure ignored request_id={} revision={} latest_revision={}",
                request_id, revision_id, app.references_request_revision
            );
        }
        eprintln!(
            "[AppShell] worker {:?} failed (revision={}): {}",
            topic, revision_id, error.message
        );
        app.request_redraw();
    }
}
