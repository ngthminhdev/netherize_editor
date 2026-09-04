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
            if bind_spawn_to_parked_session(app, request_id, session_id, &shell) {
                return;
            }
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
                // If a startup command was queued (e.g. "opencode\r"), write it
                // into the fresh shell after a brief moment so the prompt is ready.
                if let Some(cmd) = app.right_pty_startup_command.take() {
                    app.submit(RequestSpec {
                        revision_id: 0,
                        topic: RequestTopic::TerminalPty,
                        payload: WorkerRequestPayload::WritePtyInput {
                            session_id,
                            input: cmd,
                        },
                    });
                }
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
            if !app.owns_pty_session(session_id) {
                feed_parked_session_output(app, session_id, &chunk);
                return;
            }
            let preserve_viewport = app.app_state.current_mode() == EditorMode::TerminalNormal
                && app.focused_terminal_session_id() == Some(session_id);
            let mut should_redraw = false;
            // Route PTY output to the matching bottom-panel tab.
            for tab in &mut app.terminal_tabs {
                if tab.session_id == Some(session_id) {
                    let scrolled_rows = tab.grid.feed_bytes(&chunk);
                    tab.grid.apply_regex_highlights_incremental(scrolled_rows);
                    if preserve_viewport {
                        tab.grid.view_scroll_up(scrolled_rows);
                    } else {
                        tab.grid.view_scroll_to_bottom();
                    }
                    // Vim-style line editing: shell echo làm cursor di chuyển,
                    // virtual cursor (T-COPY) phải bám theo khi ở live prompt.
                    if app.app_state.current_mode() == EditorMode::TerminalNormal
                        && tab.grid.scroll_offset == 0
                    {
                        tab.grid.sync_virtual_cursor_to_shell();
                    }
                    app.terminal_needs_layout = true;
                    should_redraw = true;
                    break;
                }
            }
            if app.right_pty_session_id == Some(session_id) {
                let scrolled_rows = app.right_terminal_grid.feed_bytes(&chunk);
                app.right_terminal_grid
                    .apply_regex_highlights_incremental(scrolled_rows);
                let preserve_right_viewport =
                    preserve_viewport || app.right_terminal_grid.scroll_offset > 0;
                if preserve_right_viewport {
                    app.right_terminal_grid.view_scroll_up(scrolled_rows);
                } else {
                    app.right_terminal_grid.view_scroll_to_bottom();
                }
                app.right_terminal_needs_layout = true;
                should_redraw = true;
            }
            let is_terminal_buffer = app.terminal_buffer_grids.contains_key(&session_id);
            if let Some(grid) = app.terminal_buffer_grids.get_mut(&session_id) {
                let scrolled_rows = grid.feed_bytes(&chunk);
                grid.apply_regex_highlights_incremental(scrolled_rows);
                if preserve_viewport {
                    grid.view_scroll_up(scrolled_rows);
                } else {
                    grid.view_scroll_to_bottom();
                }
                if app.app_state.current_mode() == EditorMode::TerminalNormal
                    && grid.scroll_offset == 0
                {
                    grid.sync_virtual_cursor_to_shell();
                }
                if app.app_state.active_terminal_session_id() == Some(session_id) {
                    app.buffer_terminal_needs_layout = true;
                    should_redraw = true;
                }
            }
            if is_terminal_buffer && app.maybe_refresh_workspace_git_status() {
                should_redraw = true;
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
            if !app.owns_pty_session(session_id) {
                close_parked_session_pty(app, session_id, &reason);
                return;
            }
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
                // Clear the terminal grid so stale opencode output is not shown.
                app.right_terminal_grid = crate::terminal::grid::TerminalGrid::new(120, 40);
                app.right_terminal_grid.highlight_colors =
                    crate::terminal::grid::HighlightColors::from_theme(&app.theme);
                app.right_terminal_needs_layout = true;
                app.last_right_terminal_bounds = None;
                // Drop the renderer's uploaded terminal glyphs NOW. The redraw's
                // own cleanup branch (`else if last_right_terminal_bounds.is_some()`)
                // is skipped because we just nulled the bounds, so without this the
                // dead agent's TUI text lingers on screen behind the re-shown
                // picker. Clearing here guarantees the surface is blank.
                if let Some(renderer) = app.renderer.as_mut() {
                    renderer.clear_right_terminal();
                }
                // Exit TerminalFocus mode so the editor is not stuck in terminal input mode.
                if matches!(
                    app.app_state.current_mode(),
                    crate::core::mode::EditorMode::TerminalFocus
                        | crate::core::mode::EditorMode::TerminalNormal
                ) {
                    let _ = app
                        .app_state
                        .apply_mode_event(crate::core::mode::ModeEvent::ExitFocus);
                }
                app.request_redraw();
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

// ── Parked workspace sessions ────────────────────────────────────────────────
// PTYs keep running while their workspace is parked; their output lands in the
// parked grids without touching the active layout or requesting a redraw.

/// A spawn answered for a parked session: bind it there, never to the active
/// shell's tab (the old fallback would have hijacked the foreground tab).
fn bind_spawn_to_parked_session(
    app: &mut AppShell,
    request_id: u64,
    session_id: u64,
    shell: &str,
) -> bool {
    let active_expects = app.pending_right_pty_spawn
        || app.pending_lazygit_buffer_index.is_some()
        || app.pending_lazydocker_buffer_index.is_some()
        || app.pending_terminal_tab_spawns.contains_key(&request_id)
        || app.ignored_terminal_tab_spawns.contains(&request_id);
    if active_expects {
        return false;
    }
    let Some(parked) = app.parked_session_expecting_spawn(request_id) else {
        return false;
    };
    let mut resize: Option<(u16, u16)> = None;
    let mut close = false;
    if parked.shell.ignored_terminal_tab_spawns.remove(&request_id) {
        close = true;
    } else if let Some(idx) = parked.shell.pending_terminal_tab_spawns.remove(&request_id) {
        if let Some(tab) = parked.shell.terminal_tabs.get_mut(idx) {
            let label = shell.rsplit('/').next().unwrap_or(shell).to_string();
            tab.session_id = Some(session_id);
            tab.label = label.clone();
            tab.shell_label = label;
            tab.pending_input.clear();
            tab.status = TerminalTabStatus::Running;
            resize = Some((
                tab.grid.cols.min(u16::MAX as usize) as u16,
                tab.grid.rows.min(u16::MAX as usize) as u16,
            ));
        } else {
            close = true;
        }
    } else if parked.shell.pending_right_pty_spawn {
        parked.shell.pending_right_pty_spawn = false;
        parked.shell.right_pty_session_id = Some(session_id);
        resize = Some((
            parked.shell.right_terminal_grid.cols.min(u16::MAX as usize) as u16,
            parked.shell.right_terminal_grid.rows.min(u16::MAX as usize) as u16,
        ));
        if let Some(cmd) = parked.shell.right_pty_startup_command.take() {
            app.submit(RequestSpec {
                revision_id: 0,
                topic: RequestTopic::TerminalPty,
                payload: WorkerRequestPayload::WritePtyInput {
                    session_id,
                    input: cmd,
                },
            });
        }
    }
    if close {
        app.submit(RequestSpec {
            revision_id: 0,
            topic: RequestTopic::TerminalPty,
            payload: WorkerRequestPayload::ClosePtySession { session_id },
        });
    }
    if let Some((cols, rows)) = resize {
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
    true
}

fn feed_parked_session_output(app: &mut AppShell, session_id: u64, chunk: &[u8]) {
    let Some(parked) = app.parked_session_owning_pty(session_id) else {
        return;
    };
    let grid = if let Some(tab) = parked
        .shell
        .terminal_tabs
        .iter_mut()
        .find(|t| t.session_id == Some(session_id))
    {
        &mut tab.grid
    } else if parked.shell.right_pty_session_id == Some(session_id) {
        &mut parked.shell.right_terminal_grid
    } else if let Some(grid) = parked.shell.terminal_buffer_grids.get_mut(&session_id) {
        grid
    } else {
        return;
    };
    let scrolled_rows = grid.feed_bytes(chunk);
    grid.apply_regex_highlights_incremental(scrolled_rows);
    grid.view_scroll_to_bottom();
}

fn close_parked_session_pty(app: &mut AppShell, session_id: u64, reason: &str) {
    let Some(parked) = app.parked_session_owning_pty(session_id) else {
        return;
    };
    eprintln!("[AppShell] parked PTY {session_id} closed: {reason}");
    if let Some(tab) = parked
        .shell
        .terminal_tabs
        .iter_mut()
        .find(|t| t.session_id == Some(session_id))
    {
        tab.session_id = None;
        tab.status = TerminalTabStatus::Exited(0);
        tab.label = format!("{} (dead)", tab.label.trim_end_matches(" (dead)"));
    }
    if parked.shell.right_pty_session_id == Some(session_id) {
        parked.shell.right_pty_session_id = None;
        parked.shell.right_agent_label = None;
    }
    parked.shell.terminal_buffer_grids.remove(&session_id);
}
