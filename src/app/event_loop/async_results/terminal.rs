use super::super::*;
use crate::async_runtime::message::{
    RequestSpec, RequestTopic, WorkerRequestPayload, WorkerResultPayload,
};

pub(super) fn handle_terminal_result(
    app: &mut AppShell,
    payload: WorkerResultPayload,
    request_id: u64,
) {
    match payload {
        WorkerResultPayload::PtySpawned {
            session_id,
            shell,
            working_dir,
        } => {
            if app.maybe_refresh_workspace_git_branch(true) {
                app.request_redraw();
            }
            if app.pending_right_pty_spawn {
                app.pending_right_pty_spawn = false;
                app.right_pty_session_id = Some(session_id);
                app.right_terminal_needs_layout = true;
                app.submit(RequestSpec {
                    revision_id: 0,
                    topic: RequestTopic::TerminalPty,
                    payload: WorkerRequestPayload::ResizePtySession {
                        session_id,
                        cols: app.right_terminal_grid.cols.min(u16::MAX as usize) as u16,
                        rows: app.right_terminal_grid.rows.min(u16::MAX as usize) as u16,
                    },
                });
            } else if let Some(buffer_index) = app
                .pending_lazygit_buffer_index
                .take()
                .or_else(|| app.pending_lazydocker_buffer_index.take())
            {
                {
                    let mut g = TerminalGrid::new(120, 40);
                    g.highlight_colors = HighlightColors::from_theme(&app.theme);
                    app.terminal_buffer_grids.insert(session_id, g);
                }
                let _ = app.app_state.bind_terminal_buffer_session(
                    buffer_index,
                    session_id,
                    working_dir.clone(),
                );
                app.buffer_terminal_needs_layout = true;
                if let Some(bounds) = app.last_buffer_terminal_bounds {
                    let _ = app.sync_terminal_buffer_layout(session_id, bounds);
                }
            } else {
                if app.ignored_terminal_tab_spawns.remove(&request_id) {
                    app.submit(RequestSpec {
                        revision_id: 0,
                        topic: RequestTopic::TerminalPty,
                        payload: WorkerRequestPayload::ClosePtySession { session_id },
                    });
                    app.request_redraw();
                    return;
                }
                eprintln!(
                    "[AppShell] PTY ready: session={session_id} shell={shell} dir={}",
                    working_dir.display()
                );
                // Create label from shell name (e.g. "/bin/bash" → "bash").
                let label = shell.rsplit('/').next().unwrap_or(&shell).to_string();
                let idx = app
                    .pending_terminal_tab_spawns
                    .remove(&request_id)
                    .unwrap_or(app.active_terminal_tab);
                let Some(tab) = app.terminal_tabs.get_mut(idx) else {
                    app.submit(RequestSpec {
                        revision_id: 0,
                        topic: RequestTopic::TerminalPty,
                        payload: WorkerRequestPayload::ClosePtySession { session_id },
                    });
                    app.request_redraw();
                    return;
                };
                let cols = tab.grid.cols.min(u16::MAX as usize) as u16;
                let rows = tab.grid.rows.min(u16::MAX as usize) as u16;
                tab.session_id = Some(session_id);
                tab.label = label.clone();
                tab.shell_label = label;
                tab.pending_input.clear();
                tab.status = TerminalTabStatus::Running;
                app.terminal_needs_layout = true;
                app.submit(RequestSpec {
                    revision_id: 0,
                    topic: RequestTopic::TerminalPty,
                    payload: WorkerRequestPayload::ResizePtySession {
                        session_id,
                        cols,
                        rows,
                    },
                });
            }
            app.request_redraw();
        }
        WorkerResultPayload::PtyOutput { session_id, chunk } => {
            let preserve_viewport = app.app_state.current_mode() == EditorMode::TerminalNormal
                && app.focused_terminal_session_id() == Some(session_id);
            let mut should_redraw = false;
            // Route PTY output to the matching bottom-panel tab.
            for tab in &mut app.terminal_tabs {
                if tab.session_id == Some(session_id) {
                    let scrolled_rows = tab.grid.feed_bytes(&chunk);
                    tab.grid.apply_regex_highlights();
                    if preserve_viewport {
                        tab.grid.view_scroll_up(scrolled_rows);
                    } else {
                        tab.grid.view_scroll_to_bottom();
                    }
                    app.terminal_needs_layout = true;
                    should_redraw = true;
                    break;
                }
            }
            if app.right_pty_session_id == Some(session_id) {
                let scrolled_rows = app.right_terminal_grid.feed_bytes(&chunk);
                app.right_terminal_grid.apply_regex_highlights();
                if preserve_viewport {
                    app.right_terminal_grid.view_scroll_up(scrolled_rows);
                } else {
                    app.right_terminal_grid.view_scroll_to_bottom();
                }
                app.right_terminal_needs_layout = true;
                should_redraw = true;
            }
            if let Some(grid) = app.terminal_buffer_grids.get_mut(&session_id) {
                let scrolled_rows = grid.feed_bytes(&chunk);
                grid.apply_regex_highlights();
                if preserve_viewport {
                    grid.view_scroll_up(scrolled_rows);
                } else {
                    grid.view_scroll_to_bottom();
                }
                if app.app_state.active_terminal_session_id() == Some(session_id) {
                    app.buffer_terminal_needs_layout = true;
                    should_redraw = true;
                }
            }
            if should_redraw {
                app.request_redraw();
            }
        }
        WorkerResultPayload::PtyInputWritten { .. } => {}
        WorkerResultPayload::PtyResized {
            session_id,
            cols,
            rows,
        } => {
            if app
                .terminal_tabs
                .iter()
                .any(|t| t.session_id == Some(session_id))
            {
                eprintln!("[AppShell] PTY {session_id} resized to {cols}x{rows}");
            }
            if app
                .app_state
                .active_terminal_session_id()
                .is_some_and(|active| active == session_id)
            {
                app.buffer_terminal_needs_layout = true;
            }
        }
        WorkerResultPayload::PtySessionClosed {
            session_id, reason, ..
        } => {
            if let Some(tab) = app
                .terminal_tabs
                .iter_mut()
                .find(|t| t.session_id == Some(session_id))
            {
                eprintln!("[AppShell] PTY {session_id} closed: {reason}");
                tab.session_id = None;
                tab.status = TerminalTabStatus::Exited(0);
                tab.label = format!("{} (dead)", tab.label.trim_end_matches(" (dead)"));
                app.terminal_needs_layout = true;
            }
            if app.right_pty_session_id == Some(session_id) {
                eprintln!("[AppShell] right PTY {session_id} closed: {reason}");
                app.right_pty_session_id = None;
            }
            if app.terminal_buffer_grids.contains_key(&session_id) {
                eprintln!("[AppShell] terminal buffer PTY {session_id} closed: {reason}");
                app.terminal_buffer_grids.remove(&session_id);
                if app.app_state.active_terminal_session_id() == Some(session_id) {
                    let _ = app.close_current_buffer_now();
                } else {
                    app.mark_explorer_dirty();
                    app.editor_needs_layout = true;
                    app.editor_caret_needs_layout = false;
                }
                app.request_redraw();
            }
        }
        _ => {}
    }
}
