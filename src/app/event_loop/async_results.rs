use super::*;

impl AsyncResultRouter for AppShell {
    fn current_revision_for(&self, topic: RequestTopic) -> u64 {
        match topic {
            RequestTopic::ActiveBufferLayout => self.active_highlight_request_revision,
            RequestTopic::FzfSearch => self.fzf_search_revision,
            RequestTopic::LocalHistory => self.local_history_revision,
            RequestTopic::Git => self.git_overlay_revision,
            RequestTopic::GitStatus => self.git_status_revision,
            RequestTopic::GitBaseline => self.git_baseline_revision,
            RequestTopic::AiInlineCompletion => self.ai_inline_revision,
            RequestTopic::SystemDepCheck | RequestTopic::SystemDepInstall => 0,
            _ => 0,
        }
    }

    fn on_worker_event(&mut self, event: WorkerEvent) {
        let request_id = event.request_id;
        let revision_id = event.revision_id;
        let topic = event.topic;
        if let crate::async_runtime::message::WorkerEventKind::Failed { error } = event.kind {
            if topic == RequestTopic::LspClient {
                self.pending_lsp_server = None;
            }
            // completionItem/resolve failed (e.g. server doesn't advertise resolveProvider).
            // Fall back to a silent hover request to populate the completion doc panel.
            if self.completion_resolve_request_id == Some(request_id) {
                self.completion_resolve_request_id = None;
                self.submit_hover_for_completion_doc();
                self.editor_caret_needs_layout = true;
                self.request_redraw();
            }
            // Fallback hover also failed — give up and mark as resolved so the panel
            // shows "No documentation available" instead of spinning forever.
            if self.completion_doc_fallback_request_id == Some(request_id) {
                self.completion_doc_fallback_request_id = None;
                self.app_state.mark_completion_hover_doc_resolved();
                self.editor_caret_needs_layout = true;
                self.request_redraw();
            }
            // Hover request failed — dismiss the loading overlay we showed eagerly.
            if self.hover_loading_request_id == Some(request_id) {
                self.hover_loading_request_id = None;
                let changed = self.app_state.clear_current_overlays();
                if changed {
                    self.editor_caret_needs_layout = true;
                    self.request_redraw();
                }
            }
            // Definition request failed (timeout, server error, or our own
            // $/cancelRequest racing in). If this was the request we're still
            // waiting on, free the slot so the next `gd` isn't filtered out
            // as "superseded".
            if self.latest_definition_request_id == Some(request_id) {
                self.latest_definition_request_id = None;
            }
            // Completion request failed — clear the spinner.
            if self.app_state.is_completion_loading() {
                self.app_state.set_completion_loading(false);
                self.request_redraw();
            }
            if topic == RequestTopic::LspRequest
                && revision_id >= self.document_symbols_request_revision
                && self.app_state.command_palette_mode()
                    == Some(CommandPaletteMode::DocumentSymbols)
                && self.app_state.finish_document_symbol_picker_loading()
            {
                self.request_redraw();
            }
            let references_status = if revision_id < self.references_request_revision {
                stale_references_status()
            } else {
                friendly_references_status(&error.message)
            };
            if self
                .app_state
                .fail_pending_references_buffer(request_id, references_status)
            {
                self.editor_needs_layout = true;
                self.editor_caret_needs_layout = false;
            }
            if topic == RequestTopic::LspRequest && revision_id < self.references_request_revision {
                eprintln!(
                    "[AppShell] stale references failure ignored request_id={} revision={} latest_revision={}",
                    request_id, revision_id, self.references_request_revision
                );
            }
            eprintln!(
                "[AppShell] worker {:?} failed (revision={}): {}",
                topic, revision_id, error.message
            );
            self.request_redraw();
        }
    }

    fn on_worker_result(&mut self, result: WorkerResult) {
        let request_id = result.request_id;
        let revision_id = result.revision_id;
        match result.payload {
            WorkerResultPayload::ParseAndHighlight {
                buffer_id,
                file_path,
                spans,
                buffer_revision,
                covered_byte_range,
                ..
            } => {
                let active_buffer_id = self.app_state.active_file().map(PathBuf::from);
                if active_buffer_id.as_ref() != Some(&buffer_id) {
                    return;
                }
                if buffer_revision != self.app_state.revision() {
                    return;
                }
                if file_path != active_buffer_id {
                    return;
                }

                let covered_byte_range = covered_byte_range.map(|window| {
                    crate::syntax::highlight::expand_merge_window(
                        &self.highlight_spans,
                        &spans,
                        window,
                    )
                });

                crate::syntax::highlight::merge_highlight_spans(
                    &mut self.highlight_spans,
                    spans,
                    covered_byte_range,
                );
                self.editor_needs_layout = true;
                self.editor_caret_needs_layout = false;
                self.request_redraw();
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
                        if report.active_file_reloaded {
                            self.invalidate_highlights_and_parse_active_buffer();
                            self.force_flush_lsp_did_change_for_active_file();
                        }
                    }
                    Err(err) => {
                        eprintln!("[AppShell] fs-event apply failed: {err}");
                    }
                }
                if self.maybe_refresh_workspace_git_branch(true) {
                    self.request_redraw();
                }
                self.submit_workspace_git_status_refresh();
                self.submit_active_buffer_git_baseline_refresh();
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
                    self.editor_needs_layout = true;
                    self.editor_caret_needs_layout = false;
                    self.submit_fuzzy_picker_preview_load();
                    self.request_redraw();
                }
            }
            WorkerResultPayload::LocalHistoryLoaded { file_path, history } => {
                if self
                    .app_state
                    .reconcile_loaded_file_history(&file_path, history)
                {
                    self.editor_needs_layout = true;
                    self.editor_caret_needs_layout = false;
                    self.request_redraw();
                }
            }
            WorkerResultPayload::LocalHistorySaved { .. } => {}
            WorkerResultPayload::PtySpawned {
                session_id,
                shell,
                working_dir,
            } => {
                if self.maybe_refresh_workspace_git_branch(true) {
                    self.request_redraw();
                }
                if self.pending_right_pty_spawn {
                    eprintln!(
                        "[AppShell] right PTY ready: session={session_id} shell={shell} dir={}",
                        working_dir.display()
                    );
                    self.pending_right_pty_spawn = false;
                    self.right_pty_session_id = Some(session_id);
                    self.right_terminal_needs_layout = true;
                    self.submit(RequestSpec {
                        revision_id: 0,
                        topic: RequestTopic::TerminalPty,
                        payload: WorkerRequestPayload::ResizePtySession {
                            session_id,
                            cols: self.right_terminal_grid.cols.min(u16::MAX as usize) as u16,
                            rows: self.right_terminal_grid.rows.min(u16::MAX as usize) as u16,
                        },
                    });
                } else if let Some(buffer_index) = self
                    .pending_lazygit_buffer_index
                    .take()
                    .or_else(|| self.pending_lazydocker_buffer_index.take())
                {
                    eprintln!(
                        "[AppShell] terminal buffer ready: session={session_id} command={shell} dir={}",
                        working_dir.display()
                    );
                    {
                        let mut g = TerminalGrid::new(120, 40);
                        g.highlight_colors = HighlightColors::from_theme(&self.theme);
                        self.terminal_buffer_grids.insert(session_id, g);
                    }
                    let _ = self.app_state.bind_terminal_buffer_session(
                        buffer_index,
                        session_id,
                        working_dir.clone(),
                    );
                    self.buffer_terminal_needs_layout = true;
                    if let Some(bounds) = self.last_buffer_terminal_bounds {
                        let _ = self.sync_terminal_buffer_layout(session_id, bounds);
                    }
                } else {
                    eprintln!(
                        "[AppShell] PTY ready: session={session_id} shell={shell} dir={}",
                        working_dir.display()
                    );
                    self.pty_session_id = Some(session_id);
                    self.terminal_needs_layout = true;
                    self.submit(RequestSpec {
                        revision_id: 0,
                        topic: RequestTopic::TerminalPty,
                        payload: WorkerRequestPayload::ResizePtySession {
                            session_id,
                            cols: self.terminal_grid.cols.min(u16::MAX as usize) as u16,
                            rows: self.terminal_grid.rows.min(u16::MAX as usize) as u16,
                        },
                    });
                }
                self.request_redraw();
            }
            WorkerResultPayload::PtyOutput { session_id, chunk } => {
                let preserve_viewport = self.app_state.current_mode() == EditorMode::TerminalNormal
                    && self.focused_terminal_session_id() == Some(session_id);
                let mut should_redraw = false;
                if self.pty_session_id == Some(session_id) {
                    let scrolled_rows = self.terminal_grid.feed_bytes(&chunk);
                    self.terminal_grid.apply_regex_highlights();
                    if preserve_viewport {
                        self.terminal_grid.view_scroll_up(scrolled_rows);
                    } else {
                        self.terminal_grid.view_scroll_to_bottom();
                    }
                    self.terminal_needs_layout = true;
                    should_redraw = true;
                }
                if self.right_pty_session_id == Some(session_id) {
                    let scrolled_rows = self.right_terminal_grid.feed_bytes(&chunk);
                    self.right_terminal_grid.apply_regex_highlights();
                    if preserve_viewport {
                        self.right_terminal_grid.view_scroll_up(scrolled_rows);
                    } else {
                        self.right_terminal_grid.view_scroll_to_bottom();
                    }
                    self.right_terminal_needs_layout = true;
                    should_redraw = true;
                }
                if let Some(grid) = self.terminal_buffer_grids.get_mut(&session_id) {
                    let scrolled_rows = grid.feed_bytes(&chunk);
                    grid.apply_regex_highlights();
                    if preserve_viewport {
                        grid.view_scroll_up(scrolled_rows);
                    } else {
                        grid.view_scroll_to_bottom();
                    }
                    if self.app_state.active_terminal_session_id() == Some(session_id) {
                        self.buffer_terminal_needs_layout = true;
                        should_redraw = true;
                    }
                }
                if should_redraw {
                    self.request_redraw();
                }
            }
            WorkerResultPayload::PtyInputWritten { .. } => {}
            WorkerResultPayload::DetachedShellCommandSpawned { command, pid } => {
                eprintln!(
                    "[AppShell] background shell command started pid={:?}: {}",
                    pid, command
                );
            }
            WorkerResultPayload::PtyResized {
                session_id,
                cols,
                rows,
            } => {
                if self.pty_session_id == Some(session_id) {
                    eprintln!("[AppShell] PTY {session_id} resized to {cols}x{rows}");
                }
                if self
                    .app_state
                    .active_terminal_session_id()
                    .is_some_and(|active| active == session_id)
                {
                    self.buffer_terminal_needs_layout = true;
                }
            }
            WorkerResultPayload::PtySessionClosed {
                session_id, reason, ..
            } => {
                if self.pty_session_id == Some(session_id) {
                    eprintln!("[AppShell] PTY {session_id} closed: {reason}");
                    self.pty_session_id = None;
                }
                if self.right_pty_session_id == Some(session_id) {
                    eprintln!("[AppShell] right PTY {session_id} closed: {reason}");
                    self.right_pty_session_id = None;
                }
                if self.terminal_buffer_grids.contains_key(&session_id) {
                    eprintln!("[AppShell] terminal buffer PTY {session_id} closed: {reason}");
                    // Xóa grid ngay để dừng vòng lặp "TerminalPty failed" do
                    // các request còn tồn đọng gửi vào session đã đóng.
                    self.terminal_buffer_grids.remove(&session_id);
                    if self.app_state.active_terminal_session_id() == Some(session_id) {
                        // Buffer active → đóng luôn, quay lại editor.
                        let _ = self.close_current_buffer_now();
                    } else {
                        // Buffer không active → đánh dấu dirty để topbar cập nhật.
                        self.mark_explorer_dirty();
                        self.editor_needs_layout = true;
                        self.editor_caret_needs_layout = false;
                    }
                    self.request_redraw();
                }
            }

            WorkerResultPayload::GitBlameLine {
                file_path,
                line_number,
                summary,
            } => {
                let active_matches = self
                    .app_state
                    .active_file()
                    .is_some_and(|active| active == file_path.as_path());
                let cursor_line_matches = self.app_state.cursor_line_col().0 + 1 == line_number;
                if !active_matches
                    || !cursor_line_matches
                    || self.app_state.active_buffer_is_terminal()
                {
                    return;
                }
                let overlay_changed =
                    self.app_state
                        .set_current_overlays(vec![EditorOverlay::VirtualText {
                            line: line_number.saturating_sub(1),
                            column: self.app_state.cursor_line_col().1,
                            text: summary,
                            color_token: OverlayColorToken::UiFgGhost,
                        }]);
                if overlay_changed {
                    self.editor_needs_layout = true;
                    self.editor_caret_needs_layout = false;
                    self.request_redraw();
                }
            }
            WorkerResultPayload::LspServerStarted {
                server_name,
                root_path,
                completion_trigger_chars,
            } => {
                let started = ActiveLspServer {
                    server_name: server_name.clone(),
                    root_path: root_path.clone(),
                };
                self.active_lsp_server = Some(started.clone());
                self.lsp_completion_trigger_chars = completion_trigger_chars.clone();
                if self.pending_lsp_server.as_ref() == Some(&started) {
                    self.pending_lsp_server = None;
                }
                eprintln!(
                    "[AppShell] LSP '{}' ready for {}",
                    server_name,
                    root_path.display()
                );
                if self.pending_lsp_document_sync.is_some() {
                    let _ = self.force_flush_lsp_did_change_for_active_file();
                } else {
                    self.submit_lsp_did_open_for_active_file();
                }
            }
            WorkerResultPayload::LspServerStopped { .. } => {
                if let Some(server) = self.active_lsp_server.take() {
                    if self
                        .app_state
                        .clear_lsp_progress_for_server(&server.server_name)
                    {
                        self.request_redraw();
                    }
                }
                self.pending_lsp_document_sync = None;
                self.lsp_completion_trigger_chars.clear();
            }
            WorkerResultPayload::LspDiagnostics {
                uri, diagnostics, ..
            } => {
                eprintln!(
                    "[AppShell] LSP diagnostics: {} issue(s) in {uri}",
                    diagnostics.len()
                );
                if let Some(path) =
                    lsp_uri_to_path(&uri).and_then(|path| path.canonicalize().ok().or(Some(path)))
                {
                    let is_active_file = self
                        .app_state
                        .active_file()
                        .is_some_and(|active| active == path.as_path());
                    if self.app_state.set_file_diagnostics(path, diagnostics) {
                        self.editor_needs_layout |=
                            self.app_state.active_buffer_is_diagnostics() || is_active_file;
                        self.editor_caret_needs_layout |= is_active_file;
                    }
                    self.request_redraw();
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
                let changed = self.app_state.update_lsp_progress(
                    &server_name,
                    &token,
                    app_kind,
                    title,
                    message,
                    percentage,
                );
                if changed {
                    self.request_redraw();
                }
            }
            WorkerResultPayload::LspCheckResult {
                binary,
                install_cmd,
                is_installed,
                ..
            } => {
                if !is_installed && !self.dismissed_lsp_binaries.contains(&binary) {
                    // Hiển thị popup hướng dẫn cài LSP.
                    self.active_lsp_guide = Some(LspInstallGuide {
                        binary,
                        install_cmd,
                    });
                    self.request_redraw();
                }
                // Nếu đã cài hoặc user đã dismiss: không cần làm gì.
            }
            WorkerResultPayload::LspHoverResult {
                content,
                for_completion,
                completion_revision,
                parsed_blocks,
                ..
            } => {
                // Clear the in-flight tracker regardless of outcome.
                if self.hover_loading_request_id == Some(request_id) {
                    self.hover_loading_request_id = None;
                }
                if self.completion_doc_fallback_request_id == Some(request_id) {
                    self.completion_doc_fallback_request_id = None;
                }
                // Result Reconciliation for the fallback-hover path: if the
                // user moved to a different completion item since we asked
                // for this hover, drop it silently (don't update state).
                if for_completion {
                    let current_revision = self
                        .app_state
                        .completion()
                        .map(|state| state.current_revision);
                    if completion_revision != current_revision {
                        return;
                    }
                }
                if content.is_empty() {
                    if for_completion {
                        // Hover fallback also found nothing — mark resolved so the panel
                        // shows "No documentation available" instead of spinning.
                        self.app_state.mark_completion_hover_doc_resolved();
                        self.editor_caret_needs_layout = true;
                    } else {
                        // No docs — dismiss the loading overlay we showed eagerly.
                        let changed = self.app_state.clear_current_overlays();
                        if changed {
                            self.editor_caret_needs_layout = true;
                            self.request_redraw();
                        }
                    }
                    return;
                }
                if for_completion {
                    if self.app_state.has_completion() {
                        self.app_state
                            .set_completion_hover_doc(Some(content.clone()));
                        self.editor_caret_needs_layout = true;
                        self.request_redraw();
                    }
                    return;
                }
                use crate::app::app_state::{EditorOverlay, FloatingBoxStyle};
                let (anchor_line, anchor_col) = self.app_state.cursor_line_col();
                // Prefer worker-parsed blocks (Tree-sitter already done off the
                // main thread); only fall back to main-thread parsing if the
                // worker didn't ship them (e.g. legacy/mismatched build).
                let blocks = match parsed_blocks {
                    Some(raw) => convert_worker_hover_blocks(raw, &self.theme),
                    None => parse_hover_markdown_blocks(&content, &self.theme),
                };
                if blocks.is_empty() {
                    let changed = self.app_state.clear_current_overlays();
                    if changed {
                        self.editor_caret_needs_layout = true;
                        self.request_redraw();
                    }
                    return;
                }
                let changed =
                    self.app_state
                        .set_current_overlays(vec![EditorOverlay::FloatingBox {
                            anchor_line,
                            anchor_col,
                            blocks,
                            style: FloatingBoxStyle::DocHover,
                        }]);
                if changed {
                    self.editor_caret_needs_layout = true;
                    self.request_redraw();
                }
            }
            WorkerResultPayload::LspDefinitionResult {
                locations, jump, ..
            } => {
                use crate::app::app_state::{EditorOverlay, FloatingBoxStyle};
                // Drop any result whose request was superseded by a newer
                // `gd`/`gD`. The worker already sent $/cancelRequest, but the
                // response may have been on the wire before cancellation
                // reached the server.
                if self
                    .latest_definition_request_id
                    .is_some_and(|latest| latest != request_id)
                {
                    eprintln!(
                        "[AppShell] dropping stale LSP definition response request_id={request_id} latest={:?}",
                        self.latest_definition_request_id
                    );
                    return;
                }
                if self.latest_definition_request_id == Some(request_id) {
                    self.latest_definition_request_id = None;
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
                    // gd: mở file và nhảy đến dòng.
                    self.app_state.push_jump();
                    if let Err(err) = self.app_state.open_file(path.clone()) {
                        eprintln!("[AppShell] LSP gd open_file failed: {err}");
                        return;
                    }
                    let target_line = loc.line as usize;
                    self.app_state.jump_to_line(target_line);
                    let vp = self.editor_viewport_lines();
                    self.app_state.auto_scroll_to_cursor(vp);
                    self.invalidate_highlights_and_parse_active_buffer();
                    self.submit_lsp_check_for_path(path);
                    self.submit_lsp_did_open_for_active_file();
                    self.editor_needs_layout = true;
                    self.request_redraw();
                } else {
                    // gD: đọc ~17 dòng quanh vị trí định nghĩa và hiển FloatingBox.
                    let preview_lines = read_file_preview(&path, loc.line as usize, 8);
                    if preview_lines.is_empty() {
                        eprintln!(
                            "[AppShell] LSP gD: cannot read preview for {}",
                            path.display()
                        );
                        return;
                    }
                    let (anchor_line, anchor_col) = self.app_state.cursor_line_col();
                    let preview_text = preview_lines.join("\n");
                    let extension = path
                        .extension()
                        .and_then(|ext| ext.to_str())
                        .unwrap_or_default();
                    let preview_spans = syntax_spans_to_styled(
                        &crate::syntax::highlight::highlight_snippet(
                            &preview_text,
                            extension,
                            &self.theme,
                        ),
                        &preview_text,
                        &self.theme,
                    );
                    let changed =
                        self.app_state
                            .set_current_overlays(vec![EditorOverlay::FloatingBox {
                                anchor_line,
                                anchor_col,
                                blocks: vec![crate::app::app_state::FloatingBoxBlock::Code {
                                    text: preview_text,
                                    spans: preview_spans,
                                }],
                                style: FloatingBoxStyle::PeekWindow,
                            }]);
                    if changed {
                        self.editor_caret_needs_layout = true;
                        self.request_redraw();
                    }
                }
            }
            WorkerResultPayload::LspReferencesResult { locations, .. } => {
                if revision_id < self.references_request_revision {
                    if self
                        .app_state
                        .fail_pending_references_buffer(request_id, stale_references_status())
                    {
                        self.editor_needs_layout = true;
                        self.editor_caret_needs_layout = false;
                    }
                    eprintln!(
                        "[AppShell] stale references result ignored request_id={} revision={} latest_revision={}",
                        request_id, revision_id, self.references_request_revision
                    );
                    self.request_redraw();
                    return;
                }
                let workspace_root = self.app_state.workspace_root_path().map(PathBuf::from);
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
                if self
                    .app_state
                    .finish_pending_references_buffer(request_id, title, items)
                {
                    self.submit_references_preview_load();
                    self.editor_needs_layout = true;
                    self.editor_caret_needs_layout = false;
                }
                self.request_redraw();
            }
            WorkerResultPayload::LspDocumentSymbolsResult { uri, symbols } => {
                if revision_id < self.document_symbols_request_revision {
                    eprintln!(
                        "[AppShell] stale document symbols result ignored request_id={} revision={} latest_revision={}",
                        request_id, revision_id, self.document_symbols_request_revision
                    );
                    return;
                }
                let Some(path) = lsp_uri_to_path(&uri) else {
                    eprintln!("[AppShell] document symbols: cannot parse URI {uri}");
                    let _ = self.app_state.finish_document_symbol_picker_loading();
                    self.request_redraw();
                    return;
                };
                let Some(active_path) = self.app_state.active_file().map(PathBuf::from) else {
                    let _ = self.app_state.finish_document_symbol_picker_loading();
                    self.request_redraw();
                    return;
                };
                if active_path != path {
                    let _ = self.app_state.finish_document_symbol_picker_loading();
                    self.request_redraw();
                    return;
                }
                if self.app_state.command_palette_mode()
                    != Some(CommandPaletteMode::DocumentSymbols)
                {
                    return;
                }
                if self.app_state.set_document_symbol_picker_results(symbols) {
                    self.editor_needs_layout = true;
                    self.editor_caret_needs_layout = false;
                }
                self.request_redraw();
            }
            WorkerResultPayload::LspFormattingResult { uri, edits } => {
                let Some(path) = lsp_uri_to_path(&uri) else {
                    eprintln!("[AppShell] LSP formatting: cannot parse URI {uri}");
                    return;
                };
                let Some(active_path) = self.app_state.active_file().map(PathBuf::from) else {
                    return;
                };
                if active_path != path {
                    return;
                }

                let mut formatted = self.app_state.text_string();
                if !edits.is_empty() {
                    match apply_lsp_text_edits(&formatted, &edits) {
                        Ok(next) => formatted = next,
                        Err(err) => {
                            eprintln!("[AppShell] LSP formatting apply failed: {err}");
                            return;
                        }
                    }
                }

                let changed = self
                    .app_state
                    .replace_active_document_text_preserve_cursor(&formatted);
                if changed {
                    self.editor_needs_layout = true;
                    self.editor_caret_needs_layout = true;
                    self.submit_parse_for_active_buffer(true);
                    self.force_flush_lsp_did_change_for_active_file();
                    self.request_redraw();
                }
            }
            WorkerResultPayload::LspCodeActionResult { actions } => {
                if actions.is_empty() {
                    return;
                }
                // Luôn mở picker để user chọn, dù chỉ có 1 action.
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
                        action: CommandPaletteAction::ApplyCodeAction(i),
                        tone: CommandPaletteItemTone::Default,
                    })
                    .collect();

                self.pending_code_actions = actions;
                self.app_state.open_code_action_picker(items);
                if let Ok(result) = self
                    .app_state
                    .apply_mode_event(crate::core::mode::ModeEvent::OpenPalette)
                {
                    if result.changed {
                        self.editor_needs_layout = true;
                    }
                }
                self.focus_manager
                    .set(crate::workbench::focus_manager::FocusTarget::OverlayLayer);
                self.input_handler.clear_pending_prefix();
                self.request_redraw();
            }
            WorkerResultPayload::LspCompletionResult {
                items,
                cursor_line,
                cursor_col,
                prefix_start_col,
                prefix,
            } => {
                self.app_state.set_completion_loading(false);
                if items.is_empty() {
                    self.request_redraw();
                    return;
                }
                let completion = crate::app::app_state::CompletionState::from_lsp_items(
                    items,
                    cursor_line,
                    cursor_col,
                    prefix_start_col,
                    prefix,
                );
                if completion.filtered_items.is_empty() {
                    return;
                }
                let changed = self.app_state.set_completion(completion);
                if changed {
                    self.submit_completion_resolve();
                    self.editor_caret_needs_layout = true;
                    self.request_redraw();
                }
            }
            WorkerResultPayload::LspCompletionResolveResult {
                item_label,
                detail,
                documentation,
                completion_revision,
            } => {
                // Drop the in-flight tracker only if this result matches it.
                if self.completion_resolve_request_id == Some(request_id) {
                    self.completion_resolve_request_id = None;
                }
                // Result Reconciliation: if the user has selected a different
                // item since this resolve was issued, the echoed revision will
                // no longer match `current_revision`. Drop the entire result —
                // including the `detail` cache update — because the items list
                // may have been replaced by a re-trigger and we don't want to
                // pollute a freshly built list with stale data keyed by label.
                let Some(completion) = self.app_state.completion() else {
                    return;
                };
                if completion.current_revision != completion_revision {
                    return;
                }
                let cleaned_detail = detail.filter(|d| !d.trim().is_empty());
                if cleaned_detail.is_some() {
                    self.app_state
                        .update_completion_item_detail(&item_label, cleaned_detail);
                }
                // Re-borrow after the mutable update_completion_item_detail above.
                let Some(completion) = self.app_state.completion() else {
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
                    self.app_state.set_completion_hover_doc(cleaned);
                } else {
                    // Resolve returned no body docs — try a hover request as fallback.
                    // (Hover at the cursor mid-typing often returns empty too, in
                    // which case the panel will just show the signature alone.)
                    self.submit_hover_for_completion_doc();
                }
                self.editor_caret_needs_layout = true;
                self.request_redraw();
            }
            WorkerResultPayload::FilePreviewLoaded {
                file_path,
                target_line,
                lines,
            } => {
                let changed = if self.app_state.active_buffer_is_fuzzy_picker()
                    && active_fuzzy_preview_target(&self.app_state)
                        == Some((file_path.clone(), target_line))
                {
                    let (preview_text, preview_spans) =
                        build_preview_render_data(&lines, &file_path, &self.theme);
                    self.app_state
                        .set_fuzzy_picker_preview(lines, preview_text, preview_spans)
                } else if self.app_state.active_buffer_is_references()
                    && active_references_preview_target(&self.app_state)
                        == Some((file_path.clone(), target_line))
                {
                    let (preview_text, preview_spans) =
                        build_preview_render_data(&lines, &file_path, &self.theme);
                    self.app_state
                        .set_active_references_preview(lines, preview_text, preview_spans)
                } else if self.app_state.active_buffer_is_diagnostics()
                    && active_diagnostics_preview_target(&self.app_state)
                        == Some((file_path.clone(), target_line))
                {
                    let (preview_text, preview_spans) =
                        build_preview_render_data(&lines, &file_path, &self.theme);
                    self.app_state.set_active_diagnostics_preview(
                        lines,
                        preview_text,
                        preview_spans,
                    )
                } else {
                    false
                };
                if changed {
                    self.editor_needs_layout = true;
                    self.editor_caret_needs_layout = false;
                    self.request_redraw();
                }
            }
            WorkerResultPayload::WorkspaceGitStatus {
                workspace_root,
                statuses,
            } => {
                if self.app_state.workspace_root_path() != Some(workspace_root.as_path()) {
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
                if self.app_state.workspace_set_git_statuses(mapped) {
                    self.mark_explorer_dirty();
                    self.request_redraw();
                }
            }
            WorkerResultPayload::BufferGitBaseline {
                file_path,
                baseline,
            } => {
                let baseline_changed = self.app_state.set_buffer_git_baseline(&file_path, baseline);
                let status_changed = if self.app_state.active_file() == Some(file_path.as_path()) {
                    self.app_state.recalculate_active_buffer_git_diff()
                } else {
                    false
                };
                if baseline_changed || status_changed {
                    self.editor_needs_layout = true;
                    self.editor_caret_needs_layout = false;
                    self.request_redraw();
                }
            }
            WorkerResultPayload::AiInlineCompletionResult { suggestion } => {
                if self.app_state.set_inline_suggestion(Some(suggestion)) {
                    self.editor_needs_layout = true;
                    self.editor_caret_needs_layout = false;
                    self.request_redraw();
                }
            }
            WorkerResultPayload::SystemDepCheckResult { missing } => {
                if missing.is_empty() || self.dismissed_system_deps {
                    return;
                }
                let install_cmd = if cfg!(target_os = "macos") {
                    format!("brew install {}", missing.join(" "))
                } else {
                    format!("sudo apt-get install -y {}", missing.join(" "))
                };
                let missing_names: Vec<String> = missing.iter().map(|s| s.to_string()).collect();
                let tool_statuses = missing_names
                    .iter()
                    .map(|t| {
                        (t.clone(), crate::async_runtime::message::InstallStatus::Pending)
                    })
                    .collect();
                self.active_system_dep_guide = Some(SystemDepGuide {
                    state: SystemDepState::Detected,
                    missing_tools: Some(missing_names),
                    install_command: Some(install_cmd),
                    tool_statuses,
                });
                self.request_redraw();
            }
            WorkerResultPayload::RuntimeVersionsDetected {
                python_version,
                node_version,
                go_version,
            } => {
                self.runtime_versions.python_version = python_version;
                self.runtime_versions.node_version = node_version;
                self.runtime_versions.go_version = go_version;
                self.request_redraw();
            }
            WorkerResultPayload::PythonEnvironmentsDiscovered(envs) => {
                use crate::app::command_palette::{
                    CommandPaletteAction, CommandPaletteItem, CommandPaletteItemTone,
                };
                let items: Vec<CommandPaletteItem> = envs
                    .iter()
                    .map(|env| CommandPaletteItem {
                        label: format!(
                            "[{}] {}",
                            match &env.kind {
                                crate::async_runtime::python_env::PythonEnvKind::Venv(_) => "venv",
                                crate::async_runtime::python_env::PythonEnvKind::Pyenv(_) => "pyenv",
                                crate::async_runtime::python_env::PythonEnvKind::Global => "global",
                            },
                            env.display_name
                        ),
                        secondary_label: Some(env.executable.display().to_string()),
                        action: CommandPaletteAction::SelectPythonEnv(env.executable.clone()),
                        tone: CommandPaletteItemTone::Default,
                    })
                    .collect();
                self.app_state
                    .command_palette
                    .replace_static_results(items);
                self.request_redraw();
            }
            _ => {}
        }
    }

    fn on_stale_result(&mut self, stale: WorkerResult) {
        if let WorkerResultPayload::LspReferencesResult { .. } = stale.payload {
            if self
                .app_state
                .fail_pending_references_buffer(stale.request_id, stale_references_status())
            {
                self.editor_needs_layout = true;
                self.editor_caret_needs_layout = false;
            }
            eprintln!(
                "[AppShell] bridge discarded stale references result request_id={} revision={} latest_revision={}",
                stale.request_id, stale.revision_id, self.references_request_revision
            );
            self.request_redraw();
        }
    }

    fn on_ai_message_chunk(&mut self, text: String) {
        let chat = &mut self.panel_state.ai_chat;
        if let Some(last) = chat.messages.last_mut()
            && last.role == crate::workbench::panel_state::AiRole::Assistant
        {
            last.text.push_str(&text);
        } else {
            chat.messages
                .push(crate::workbench::panel_state::AiChatMessage {
                    role: crate::workbench::panel_state::AiRole::Assistant,
                    text,
                });
        }
        self.request_redraw();
    }

    fn on_ai_stream_complete(&mut self) {
        self.panel_state.ai_chat.is_generating = false;
        self.request_redraw();
    }

    fn on_ai_stream_error(&mut self, error: String) {
        self.panel_state.ai_chat.is_generating = false;
        self.panel_state
            .ai_chat
            .messages
            .push(crate::workbench::panel_state::AiChatMessage {
                role: crate::workbench::panel_state::AiRole::System,
                text: format!("Error: {}", error),
            });
        self.request_redraw();
    }

    fn on_ai_install_success(&mut self) {
        self.panel_state.ai_chat.is_generating = false;
        self.panel_state.ai_chat.is_opencode_missing = false;

        // Detect shell to give the exact source command.
        let shell = std::env::var("SHELL").unwrap_or_default();
        let source_cmd = if shell.contains("zsh") {
            "source ~/.zshrc"
        } else if shell.contains("bash") {
            "source ~/.bash_profile"
        } else if shell.contains("fish") {
            "source ~/.config/fish/config.fish"
        } else {
            "source ~/.profile"
        };

        let next_steps = format!(
            "opencode installed!\n\
             \n\
             PATH chưa được cập nhật trong session này.\n\
             Làm theo 2 bước:\n\
             1. Mở terminal, chạy:  {source_cmd}\n\
             2. Khởi động lại editor."
        );

        self.panel_state
            .ai_chat
            .messages
            .push(crate::workbench::panel_state::AiChatMessage {
                role: crate::workbench::panel_state::AiRole::System,
                text: next_steps,
            });
        self.request_redraw();
    }

    fn on_system_dep_tool_progress(
        &mut self,
        tool: String,
        status: crate::async_runtime::message::InstallStatus,
    ) {
        let Some(guide) = self.active_system_dep_guide.as_mut() else {
            return;
        };
        if let Some(entry) = guide.tool_statuses.iter_mut().find(|(t, _)| *t == tool) {
            entry.1 = status;
        }
        self.editor_needs_layout = true;
        self.request_redraw();
    }

    fn on_system_dep_install_done(&mut self) {
        if let Some(guide) = self.active_system_dep_guide.as_mut() {
            guide.state = SystemDepState::Complete;
        }
        self.editor_needs_layout = true;
        self.request_redraw();
    }

    fn on_lsp_missing_dependency(&mut self, _language_id: String, tool_name: String) {
        if self.dismissed_lsp_binaries.contains(&tool_name) {
            return;
        }
        let install_cmd = crate::lsp::registry::language_profile_for_binary(&tool_name)
            .map(|p| p.install_command.to_string())
            .unwrap_or_default();
        self.pending_lsp_server = None;
        self.active_lsp_guide = Some(LspInstallGuide {
            binary: tool_name,
            install_cmd,
        });
        self.request_redraw();
    }
}

/// Convert `file:///path/to/file` URI thành PathBuf.
fn lsp_uri_to_path(uri: &str) -> Option<std::path::PathBuf> {
    let url = url::Url::parse(uri).ok()?;
    let path = url.to_file_path().ok()?;
    path.canonicalize().ok().or(Some(path))
}

pub(super) fn apply_lsp_text_edits(
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

/// Đọc ~(context*2+1) dòng code quanh `center_line` từ file để preview (gD).
fn read_file_preview(path: &std::path::Path, center_line: usize, context: usize) -> Vec<String> {
    use std::io::{BufRead, BufReader};
    let file = match std::fs::File::open(path) {
        Ok(f) => f,
        Err(_) => return Vec::new(),
    };
    let start = center_line.saturating_sub(context);
    let end = center_line + context + 1;
    BufReader::new(file)
        .lines()
        .enumerate()
        .filter_map(|(i, line)| {
            if i >= start && i < end {
                let text = line.unwrap_or_default();
                let marker = if i == center_line { "▶" } else { " " };
                Some(format!("{marker} {:>4}  {}", i + 1, text))
            } else {
                None
            }
        })
        .collect()
}

fn friendly_references_status(message: &str) -> String {
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

fn stale_references_status() -> String {
    "References request superseded by newer request".to_string()
}

fn active_fuzzy_preview_target(app_state: &AppState) -> Option<(PathBuf, Option<usize>)> {
    match app_state.command_palette_selected_action()? {
        crate::app::command_palette::CommandPaletteAction::OpenFile(path) => Some((path, None)),
        crate::app::command_palette::CommandPaletteAction::OpenSearchMatch {
            path, line, ..
        } => Some((path, Some(line as usize))),
        _ => None,
    }
}

fn active_references_preview_target(app_state: &AppState) -> Option<(PathBuf, Option<usize>)> {
    let item = app_state.selected_reference_item()?;
    Some((item.path.clone(), Some(item.line + 1)))
}

fn active_diagnostics_preview_target(app_state: &AppState) -> Option<(PathBuf, Option<usize>)> {
    let item = app_state.selected_diagnostic_item()?;
    Some((item.file_path.clone(), Some(item.line + 1)))
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        path::PathBuf,
        time::{SystemTime, UNIX_EPOCH},
    };

    use crate::{
        app::{app_state::ReferencesBufferItem, async_bridge::AsyncResultRouter},
        async_runtime::message::{
            FilePreviewLine, LspLocation, RequestTopic, WorkerEvent, WorkerEventKind,
            WorkerFailure, WorkerFailureKind, WorkerResult, WorkerResultPayload,
        },
        lsp::client::path_to_lsp_uri,
        syntax::{
            highlight::{HighlightCategory, HighlightSpan},
            syntax_engine::LanguageId,
        },
    };

    use super::AppShell;

    fn unique_temp_path(suffix: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock drift")
            .as_nanos();
        std::env::temp_dir().join(format!("netherize_async_results_{suffix}_{nanos}"))
    }

    fn write_temp_file(suffix: &str, contents: &str) -> PathBuf {
        let path = unique_temp_path(suffix);
        fs::write(&path, contents).expect("write temp file");
        path.canonicalize().expect("canonical temp file")
    }

    fn parse_highlight_result(buffer_id: PathBuf, buffer_revision: u64) -> WorkerResult {
        WorkerResult {
            request_id: 1,
            revision_id: 1,
            topic: RequestTopic::ActiveBufferLayout,
            payload: WorkerResultPayload::ParseAndHighlight {
                buffer_id: buffer_id.clone(),
                file_path: Some(buffer_id),
                language_id: LanguageId::Rust,
                buffer_revision,
                spans: vec![HighlightSpan {
                    range: 0..2,
                    category: HighlightCategory::Keyword,
                }],
                covered_byte_range: None,
                line_count: 1,
                char_count: 10,
                byte_count: 10,
                parse_time_ms: 1,
                highlight_time_ms: 1,
            },
        }
    }

    #[test]
    fn parse_highlight_result_for_inactive_buffer_is_rejected() {
        let mut shell = AppShell::new_for_tests().expect("create app shell");
        let old_file = write_temp_file("old_highlight.rs", "fn old() {}\n");
        let active_file = write_temp_file("active_highlight.rs", "fn active() {}\n");

        shell
            .app_state
            .open_file(old_file.clone())
            .expect("open old file");
        shell
            .app_state
            .open_file(active_file)
            .expect("open active file");
        let active_revision = shell.app_state.revision();
        shell.editor_needs_layout = false;
        shell.editor_caret_needs_layout = true;

        AsyncResultRouter::on_worker_result(
            &mut shell,
            parse_highlight_result(old_file, active_revision),
        );

        assert!(shell.highlight_spans.is_empty());
        assert!(!shell.editor_needs_layout);
    }

    #[test]
    fn parse_highlight_result_for_stale_revision_is_rejected() {
        let mut shell = AppShell::new_for_tests().expect("create app shell");
        let file_path = write_temp_file("stale_highlight.rs", "fn stale() {}\n");
        shell
            .app_state
            .open_file(file_path.clone())
            .expect("open file");
        let stale_revision = shell.app_state.revision().saturating_sub(1);
        shell.editor_needs_layout = false;
        shell.editor_caret_needs_layout = true;

        AsyncResultRouter::on_worker_result(
            &mut shell,
            parse_highlight_result(file_path, stale_revision),
        );

        assert!(shell.highlight_spans.is_empty());
        assert!(!shell.editor_needs_layout);
    }

    #[test]
    fn parse_highlight_result_for_active_buffer_revision_is_applied() {
        let mut shell = AppShell::new_for_tests().expect("create app shell");
        let file_path = write_temp_file("fresh_highlight.rs", "fn fresh() {}\n");
        shell
            .app_state
            .open_file(file_path.clone())
            .expect("open file");
        let active_revision = shell.app_state.revision();
        shell.editor_needs_layout = false;
        shell.editor_caret_needs_layout = true;

        AsyncResultRouter::on_worker_result(
            &mut shell,
            parse_highlight_result(file_path, active_revision),
        );

        assert_eq!(shell.highlight_spans.len(), 1);
        assert!(shell.editor_needs_layout);
        assert!(!shell.editor_caret_needs_layout);
    }

    #[test]
    fn references_result_clears_loading_and_populates_items() {
        let mut shell = AppShell::new_for_tests().expect("create app shell");
        let file_path = unique_temp_path("references_result.rs");
        shell.app_state.open_pending_references_buffer(
            "References",
            Some(file_path.clone()),
            0,
            41,
        );
        shell.references_request_revision = 1;
        shell.editor_needs_layout = false;
        shell.editor_caret_needs_layout = true;

        AsyncResultRouter::on_worker_result(
            &mut shell,
            WorkerResult {
                request_id: 41,
                revision_id: 1,
                topic: RequestTopic::LspRequest,
                payload: WorkerResultPayload::LspReferencesResult {
                    locations: vec![LspLocation {
                        uri: path_to_lsp_uri(&file_path),
                        line: 7,
                        character: 3,
                    }],
                },
            },
        );

        let references = shell
            .app_state
            .active_references_buffer()
            .expect("references buffer");
        assert!(!references.loading);
        assert_eq!(references.pending_request_id, None);
        assert_eq!(references.items.len(), 1);
        assert_eq!(references.items[0].line, 7);
        assert!(shell.editor_needs_layout);
        assert!(!shell.editor_caret_needs_layout);
    }

    #[test]
    fn stale_references_result_stops_loading_and_keeps_items_empty() {
        let mut shell = AppShell::new_for_tests().expect("create app shell");
        let file_path = unique_temp_path("stale_references_result.rs");
        shell.app_state.open_pending_references_buffer(
            "References",
            Some(file_path.clone()),
            0,
            52,
        );
        shell.references_request_revision = 2;
        shell.editor_needs_layout = false;
        shell.editor_caret_needs_layout = true;

        AsyncResultRouter::on_worker_result(
            &mut shell,
            WorkerResult {
                request_id: 52,
                revision_id: 1,
                topic: RequestTopic::LspRequest,
                payload: WorkerResultPayload::LspReferencesResult {
                    locations: vec![LspLocation {
                        uri: path_to_lsp_uri(&file_path),
                        line: 4,
                        character: 1,
                    }],
                },
            },
        );

        let references = shell
            .app_state
            .active_references_buffer()
            .expect("references buffer");
        assert!(!references.loading);
        assert_eq!(references.pending_request_id, None);
        assert!(references.items.is_empty());
        assert_eq!(
            references.status_message.as_deref(),
            Some("References request superseded by newer request")
        );
        assert!(shell.editor_needs_layout);
        assert!(!shell.editor_caret_needs_layout);
    }

    #[test]
    fn failed_references_event_clears_loading_buffer() {
        let mut shell = AppShell::new_for_tests().expect("create app shell");
        shell
            .app_state
            .open_pending_references_buffer("References", None, 0, 77);
        shell.references_request_revision = 3;
        shell.editor_needs_layout = false;
        shell.editor_caret_needs_layout = true;

        AsyncResultRouter::on_worker_event(
            &mut shell,
            WorkerEvent {
                request_id: 77,
                revision_id: 3,
                topic: RequestTopic::LspRequest,
                kind: WorkerEventKind::Failed {
                    error: WorkerFailure {
                        kind: WorkerFailureKind::Execution,
                        message: "references: timed out waiting for response".to_string(),
                    },
                },
            },
        );

        let references = shell
            .app_state
            .active_references_buffer()
            .expect("references buffer");
        assert!(!references.loading);
        assert_eq!(
            references.status_message.as_deref(),
            Some("References request timed out")
        );
        assert!(shell.editor_needs_layout);
        assert!(!shell.editor_caret_needs_layout);
    }

    #[test]
    fn references_preview_result_marks_editor_layout_dirty() {
        let mut shell = AppShell::new_for_tests().expect("create app shell");
        let file_path = unique_temp_path("references_preview.rs");
        shell
            .app_state
            .open_references_buffer(
                "References (1)",
                None,
                0,
                vec![ReferencesBufferItem {
                    path: file_path.clone(),
                    relative_path: "references_preview.rs".to_string(),
                    line: 4,
                    column: 0,
                    summary: "Ln 5, Col 1".to_string(),
                }],
            )
            .expect("open references buffer");
        shell.editor_needs_layout = false;
        shell.editor_caret_needs_layout = true;

        AsyncResultRouter::on_worker_result(
            &mut shell,
            WorkerResult {
                request_id: 1,
                revision_id: 0,
                topic: RequestTopic::FilePreview,
                payload: WorkerResultPayload::FilePreviewLoaded {
                    file_path,
                    target_line: Some(5),
                    lines: vec![FilePreviewLine {
                        line_number: 5,
                        text: "demo()".to_string(),
                        is_target: true,
                    }],
                },
            },
        );

        let references = shell
            .app_state
            .active_references_buffer()
            .expect("references buffer");
        assert_eq!(references.preview_lines.len(), 1);
        assert!(shell.editor_needs_layout);
        assert!(!shell.editor_caret_needs_layout);
    }
}
