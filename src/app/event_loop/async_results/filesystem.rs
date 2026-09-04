use super::super::*;
use crate::app::app_state::ExternalChangeReport;
use crate::async_runtime::message::WorkerResultPayload;

fn file_label(path: &std::path::Path) -> String {
    path.file_name()
        .and_then(|name| name.to_str())
        .map(str::to_string)
        .unwrap_or_else(|| path.to_string_lossy().into_owned())
}

/// Surface what the external-change pipeline just did. Deletes outrank
/// reloads; conflicts stay silent here because the confirmation prompt is
/// already on screen.
fn show_external_change_toasts(app: &mut AppShell, report: &ExternalChangeReport) {
    if let Some(first) = report.deleted_paths.first() {
        let extra = report.deleted_paths.len().saturating_sub(1);
        let message = if extra == 0 {
            format!("{} deleted on disk — buffer kept", file_label(first))
        } else {
            format!(
                "{} (+{extra} more) deleted on disk — buffers kept",
                file_label(first)
            )
        };
        app.show_transient_toast_kind(message, ToastKind::Warning);
        return;
    }
    if report.conflict_detected {
        return;
    }
    let inactive = report.inactive_reloaded_paths.len();
    let reloaded = inactive + usize::from(report.active_file_reloaded);
    if reloaded == 0 {
        return;
    }
    let message = if reloaded == 1 {
        let label = if report.active_file_reloaded {
            app.app_state.active_file().map(file_label)
        } else {
            report
                .inactive_reloaded_paths
                .first()
                .map(|p| file_label(p))
        };
        match label {
            Some(label) => format!("Reloaded {label} — changed on disk"),
            None => "Reloaded 1 file — changed on disk".to_string(),
        }
    } else {
        format!("Reloaded {reloaded} files — changed on disk")
    };
    app.show_transient_toast_kind(message, ToastKind::Info);
}

pub(in crate::app::event_loop) fn handle_filesystem_result(
    app: &mut AppShell,
    payload: WorkerResultPayload,
) {
    if let WorkerResultPayload::FileSystemEvents { root_path, events } = payload {
        // Only a PARKED workspace's root is diverted. Anything else (the
        // active root, or the parent-dir watcher of a file opened outside the
        // workspace) is handled here as before.
        if let Some(parked) = app.parked_session_for_root(&root_path) {
            parked.pending_fs_events.extend(events);
            parked.shell.explorer_snapshot_dirty = true;
            return;
        }
        for event in events.iter() {
            if let Ok(metadata) = std::fs::metadata(&event.path) {
                if let Ok(modified_time) = metadata.modified() {
                    app.last_external_file_check_times
                        .insert(event.path.clone(), modified_time);
                }
            }
            if let Some(ref new_path) = event.new_path {
                if let Ok(metadata) = std::fs::metadata(new_path) {
                    if let Ok(modified_time) = metadata.modified() {
                        app.last_external_file_check_times
                            .insert(new_path.clone(), modified_time);
                    }
                }
            }
        }
        match app.app_state.apply_external_file_events(&events) {
            Ok(report) => {
                // #4: nội dung file và tree walk đều chạy ở worker; phase 1 chỉ
                // quyết định cần đọc gì / có cần rescan không.
                if !report.pending_reload_paths.is_empty() {
                    app.submit(RequestSpec {
                        revision_id: 0,
                        topic: RequestTopic::WorkspaceWatch,
                        payload: WorkerRequestPayload::ReadExternalFiles {
                            paths: report.pending_reload_paths.clone(),
                        },
                    });
                }
                if report.workspace_rescan_needed {
                    app.submit_workspace_rescan();
                }
                if report.active_file_reloaded {
                    app.invalidate_highlights_and_parse_active_buffer();
                    app.force_flush_lsp_did_change_for_active_file();
                }
                for path in &report.inactive_reloaded_paths {
                    app.submit_lsp_sync_for_externally_reloaded_path(path);
                }
                if report.conflict_detected
                    && app.pending_confirmation.is_none()
                    && let Some(path) = report.conflict_path.clone()
                {
                    let _ = app.begin_external_overwrite_confirmation(path);
                }
                show_external_change_toasts(app, &report);
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

/// Phase 2 of the external-change pipeline: worker-fetched file contents are
/// applied to buffers on the UI thread (no disk I/O happens here).
pub(super) fn handle_external_files_read(app: &mut AppShell, payload: WorkerResultPayload) {
    if let WorkerResultPayload::ExternalFilesRead { files } = payload {
        for file in &files {
            if let Some(modified_time) = file.modified_time {
                app.last_external_file_check_times
                    .insert(file.path.clone(), modified_time);
            }
        }
        let report = app.app_state.apply_external_file_contents(&files);
        if report.active_file_reloaded {
            app.invalidate_highlights_and_parse_active_buffer();
            app.force_flush_lsp_did_change_for_active_file();
        }
        for path in &report.inactive_reloaded_paths {
            app.submit_lsp_sync_for_externally_reloaded_path(path);
        }
        if report.conflict_detected
            && app.pending_confirmation.is_none()
            && let Some(path) = report.conflict_path.clone()
        {
            let _ = app.begin_external_overwrite_confirmation(path);
        }
        show_external_change_toasts(app, &report);
        if report.active_file_reloaded || !report.inactive_reloaded_paths.is_empty() {
            app.submit_active_buffer_git_baseline_refresh();
            app.editor_needs_layout = true;
            app.editor_caret_needs_layout = false;
            app.request_redraw();
        }
    }
}

/// Fresh workspace tree from the async rescan worker — swap it in and refresh
/// everything that mirrors the tree (explorer, file picker, live grep).
pub(super) fn handle_workspace_rescanned(app: &mut AppShell, payload: WorkerResultPayload) {
    if let WorkerResultPayload::WorkspaceRescanned { root_path, nodes } = payload {
        match app.app_state.apply_workspace_rescan(&root_path, nodes) {
            Ok(true) => {
                if matches!(
                    app.app_state.command_palette_mode(),
                    Some(CommandPaletteMode::FilePicker | CommandPaletteMode::LiveGrep)
                ) && !app.app_state.command_palette_query_text().trim().is_empty()
                {
                    app.submit_active_palette_fzf_search();
                }
                app.sync_explorer_expanded_with_workspace();
                app.editor_needs_layout = true;
                app.request_redraw();
            }
            Ok(false) => {}
            Err(err) => {
                eprintln!("[AppShell] workspace rescan apply failed: {err}");
            }
        }
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
