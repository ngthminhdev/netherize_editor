use super::super::*;
use crate::async_runtime::message::WorkerResultPayload;

pub(super) fn handle_filesystem_result(app: &mut AppShell, payload: WorkerResultPayload) {
    if let WorkerResultPayload::FileSystemEvents { events, .. } = payload {
        for event in events.iter() {
            if let Ok(metadata) = std::fs::metadata(&event.path) {
                if let Ok(modified_time) = metadata.modified() {
                    app.last_external_file_check_times.insert(event.path.clone(), modified_time);
                }
            }
            if let Some(ref new_path) = event.new_path {
                if let Ok(metadata) = std::fs::metadata(new_path) {
                    if let Ok(modified_time) = metadata.modified() {
                        app.last_external_file_check_times.insert(new_path.clone(), modified_time);
                    }
                }
            }
        }
        match app.app_state.apply_external_file_events(&events) {
            Ok(report) => {
                if report.workspace_reloaded
                    && matches!(
                        app.app_state.command_palette_mode(),
                        Some(CommandPaletteMode::FilePicker | CommandPaletteMode::LiveGrep)
                    )
                    && !app.app_state.command_palette_query_text().trim().is_empty()
                {
                    app.submit_active_palette_fzf_search();
                }
                if report.active_file_reloaded {
                    app.invalidate_highlights_and_parse_active_buffer();
                    app.force_flush_lsp_did_change_for_active_file();
                }
                if report.conflict_detected
                    && app.pending_confirmation.is_none()
                    && let Some(path) = report.conflict_path.clone()
                {
                    let _ = app.begin_external_overwrite_confirmation(path);
                }
            }
            Err(err) => {
                eprintln!("[AppShell] fs-event apply failed: {err}");
            }
        }
        if app.maybe_refresh_workspace_git_branch(true) {
            app.request_redraw();
        }
        app.submit_workspace_git_status_refresh();
        app.submit_active_buffer_git_baseline_refresh();
        app.sync_explorer_expanded_with_workspace();
        app.editor_needs_layout = true;
        app.editor_caret_needs_layout = false;
    }
}

pub(super) fn handle_file_copy_result(app: &mut AppShell, payload: WorkerResultPayload) {
    if let WorkerResultPayload::FileCopyResult {
        target_path,
        success,
        error_message,
        ..
    } = payload
    {
        if success {
            app.show_transient_toast(format!("Pasted: {}", target_path.display()));
            let _ = app.app_state.rescan_workspace();
            app.submit_workspace_git_status_refresh();
            app.mark_explorer_dirty();
        } else {
            let error = error_message.unwrap_or_else(|| "Unknown error".to_string());
            app.show_transient_toast(format!("Paste failed: {error}"));
        }
    }
}
