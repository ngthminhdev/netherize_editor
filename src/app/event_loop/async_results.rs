use super::*;

impl AsyncResultRouter for AppShell {
    fn current_revision_for(&self, topic: RequestTopic) -> u64 {
        match topic {
            RequestTopic::ActiveBufferLayout => self.active_highlight_request_revision,
            RequestTopic::FzfSearch => self.fzf_search_revision,
            _ => 0,
        }
    }

    fn on_worker_event(&mut self, event: WorkerEvent) {
        if let crate::async_runtime::message::WorkerEventKind::Failed { error } = event.kind {
            if event.topic == RequestTopic::LspClient {
                self.pending_lsp_server = None;
            }
            eprintln!(
                "[AppShell] worker {:?} failed (revision={}): {}",
                event.topic, event.revision_id, error.message
            );
        }
    }

    fn on_worker_result(&mut self, result: WorkerResult) {
        match result.payload {
            WorkerResultPayload::ParseAndHighlight {
                spans,
                buffer_revision,
                covered_byte_range,
                ..
            } => {
                if buffer_revision != self.app_state.revision() {
                    return;
                }

                crate::syntax::highlight::merge_highlight_spans(
                    &mut self.highlight_spans,
                    spans,
                    covered_byte_range,
                );
                self.editor_needs_layout = true;
                self.editor_caret_needs_layout = false;
            }
            WorkerResultPayload::FileSystemEvents { events, .. } => {
                match self.app_state.apply_external_file_events(&events) {
                    Ok(report) => {
                        if report.workspace_reloaded
                            && matches!(
                                self.app_state.command_palette_mode(),
                                Some(CommandPaletteMode::FilePicker | CommandPaletteMode::LiveGrep)
                            )
                            && !self
                                .app_state
                                .command_palette_query_text()
                                .trim()
                                .is_empty()
                        {
                            self.submit_active_palette_fzf_search();
                        }
                    }
                    Err(err) => {
                        eprintln!("[AppShell] fs-event apply failed: {err}");
                    }
                }
                self.sync_explorer_expanded_with_workspace();
                self.editor_needs_layout = true;
                self.editor_caret_needs_layout = false;
            }
            WorkerResultPayload::FzfResults { query, mode, items } => {
                let palette_mode = match mode {
                    FzfSearchMode::FindFile => CommandPaletteMode::FilePicker,
                    FzfSearchMode::LiveGrep => CommandPaletteMode::LiveGrep,
                };
                let palette_items = items
                    .into_iter()
                    .map(|item| {
                        if let (Some(line), Some(column)) = (item.line, item.column) {
                            crate::app::command_palette::CommandPaletteItem::search_match(
                                item.label,
                                item.preview,
                                item.path,
                                line,
                                column,
                            )
                        } else {
                            crate::app::command_palette::CommandPaletteItem::file_match(
                                item.label, item.path,
                            )
                        }
                    })
                    .collect();
                if self
                    .app_state
                    .set_command_palette_results(palette_mode, &query, palette_items)
                {
                    self.request_redraw();
                }
            }
            WorkerResultPayload::PtySpawned {
                session_id,
                shell,
                working_dir,
            } => {
                eprintln!(
                    "[AppShell] PTY ready: session={session_id} shell={shell} dir={}",
                    working_dir.display()
                );
                self.pty_session_id = Some(session_id);
                self.terminal_needs_layout = true;
            }
            WorkerResultPayload::PtyOutput { session_id, chunk } => {
                if self.pty_session_id == Some(session_id) {
                    self.terminal_grid.feed_chunk(&chunk);
                    self.terminal_grid.view_scroll_to_bottom();
                    self.terminal_needs_layout = true;
                    self.request_redraw();
                }
            }
            WorkerResultPayload::PtySessionClosed {
                session_id, reason, ..
            } => {
                if self.pty_session_id == Some(session_id) {
                    eprintln!("[AppShell] PTY {session_id} closed: {reason}");
                    self.pty_session_id = None;
                }
            }
            WorkerResultPayload::LspServerStarted {
                server_name,
                root_path,
                ..
            } => {
                let started = ActiveLspServer {
                    server_name: server_name.clone(),
                    root_path: root_path.clone(),
                };
                self.active_lsp_server = Some(started.clone());
                if self.pending_lsp_server.as_ref() == Some(&started) {
                    self.pending_lsp_server = None;
                }
                eprintln!(
                    "[AppShell] LSP '{}' ready for {}",
                    server_name,
                    root_path.display()
                );
                self.submit_lsp_did_open_for_active_file();
            }
            WorkerResultPayload::LspServerStopped { .. } => {
                self.active_lsp_server = None;
            }
            WorkerResultPayload::LspDiagnostics {
                uri, diagnostics, ..
            } => {
                eprintln!(
                    "[AppShell] LSP diagnostics: {} issue(s) in {uri}",
                    diagnostics.len()
                );
            }
            WorkerResultPayload::LspLogMessage { level, message } => {
                eprintln!("[LSP/{level}] {message}");
            }
            _ => {}
        }
    }

    fn on_stale_result(&mut self, _stale: WorkerResult) {}
}
