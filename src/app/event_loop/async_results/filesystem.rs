use super::super::*;
use crate::async_runtime::message::WorkerResultPayload;

pub(super) fn handle_filesystem_result(app: &mut AppShell, payload: WorkerResultPayload) {
    if let WorkerResultPayload::FileSystemEvents { events, .. } = payload {
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
