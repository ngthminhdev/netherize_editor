use super::*;

impl AsyncResultRouter for AppShell {
    fn current_revision_for(&self, topic: RequestTopic) -> u64 {
        match topic {
            RequestTopic::ActiveBufferLayout => self.active_highlight_request_revision,
            _ => 0,
        }
    }

    fn on_worker_event(&mut self, _event: WorkerEvent) {}

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
                if let Err(err) = self.app_state.apply_external_file_events(&events) {
                    eprintln!("[AppShell] fs-event apply failed: {err}");
                }
                self.sync_explorer_expanded_with_workspace();
                self.editor_needs_layout = true;
                self.editor_caret_needs_layout = false;
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
                eprintln!(
                    "[AppShell] LSP '{}' ready for {}",
                    server_name,
                    root_path.display()
                );
                self.submit_lsp_did_open_for_active_file();
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
