use super::super::*;
use crate::async_runtime::message::WorkerResultPayload;

pub(super) fn handle_git_result(app: &mut AppShell, payload: WorkerResultPayload) {
    match payload {
        WorkerResultPayload::WorkspaceGitStatus {
            workspace_root,
            statuses,
        } => {
            if app.app_state.workspace_root_path() != Some(workspace_root.as_path()) {
                return;
            }
            let mapped = statuses
                .into_iter()
                .map(|(path, status)| {
                    let status = match status {
                        crate::async_runtime::message::GitFileStatus::Modified => {
                            crate::workspace::model::WorkspaceGitStatus::Modified
                        }
                        crate::async_runtime::message::GitFileStatus::Added => {
                            crate::workspace::model::WorkspaceGitStatus::Added
                        }
                    };
                    (path, status)
                })
                .collect();
            if app.app_state.workspace_set_git_statuses(mapped) {
                app.mark_explorer_dirty();
                app.request_redraw();
            }
        }
        WorkerResultPayload::BufferGitBaseline {
            file_path,
            baseline,
        } => {
            let baseline_changed = app.app_state.set_buffer_git_baseline(&file_path, baseline);
            let status_changed = if app.app_state.active_file() == Some(file_path.as_path()) {
                app.app_state.recalculate_active_buffer_git_diff()
            } else {
                false
            };
            if baseline_changed || status_changed {
                app.editor_needs_layout = true;
                app.editor_caret_needs_layout = false;
                app.request_redraw();
            }
        }
        WorkerResultPayload::GitBlameLine {
            file_path,
            line_number,
            summary,
        } => {
            let active_matches = app
                .app_state
                .active_file()
                .is_some_and(|active| active == file_path.as_path());
            let cursor_line_matches = app.app_state.cursor_line_col().0 + 1 == line_number;
            if !active_matches || !cursor_line_matches || app.app_state.active_buffer_is_terminal()
            {
                return;
            }
            let overlay_changed =
                app.app_state
                    .set_current_overlays(vec![EditorOverlay::VirtualText {
                        line: line_number.saturating_sub(1),
                        column: app.app_state.cursor_line_col().1,
                        text: summary,
                        color_token: OverlayColorToken::UiFgGhost,
                    }]);
            if overlay_changed {
                app.editor_needs_layout = true;
                app.editor_caret_needs_layout = false;
                app.request_redraw();
            }
        }
        _ => {}
    }
}
