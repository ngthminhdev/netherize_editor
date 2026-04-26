use super::*;

impl AsyncResultRouter for AppShell {
    fn current_revision_for(&self, topic: RequestTopic) -> u64 {
        match topic {
            RequestTopic::ActiveBufferLayout => self.active_highlight_request_revision,
            RequestTopic::FzfSearch => self.fzf_search_revision,
            RequestTopic::Git => self.git_overlay_revision,
            _ => 0,
        }
    }

    fn on_worker_event(&mut self, event: WorkerEvent) {
        if let crate::async_runtime::message::WorkerEventKind::Failed { error } = event.kind {
            if event.topic == RequestTopic::LspClient {
                self.pending_lsp_server = None;
            }
            if self.app_state.fail_pending_references_buffer(
                event.request_id,
                friendly_references_status(&error.message),
            ) {
                self.editor_needs_layout = true;
                self.editor_caret_needs_layout = false;
                self.request_redraw();
            }
            eprintln!(
                "[AppShell] worker {:?} failed (revision={}): {}",
                event.topic, event.revision_id, error.message
            );
        }
    }

    fn on_worker_result(&mut self, result: WorkerResult) {
        let request_id = result.request_id;
        match result.payload {
            WorkerResultPayload::ParseAndHighlight {
                file_path,
                spans,
                buffer_revision,
                covered_byte_range,
                ..
            } => {
                if buffer_revision != self.app_state.revision() {
                    return;
                }
                if file_path != self.app_state.active_file().map(PathBuf::from) {
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
                    self.editor_needs_layout = true;
                    self.editor_caret_needs_layout = false;
                    self.submit_fuzzy_picker_preview_load();
                    self.request_redraw();
                }
            }
            WorkerResultPayload::PtySpawned {
                session_id,
                shell,
                working_dir,
            } => {
                if let Some(buffer_index) = self.pending_lazygit_buffer_index.take() {
                    eprintln!(
                        "[AppShell] terminal buffer ready: session={session_id} command={shell} dir={}",
                        working_dir.display()
                    );
                    self.terminal_buffer_grids
                        .insert(session_id, TerminalGrid::new(120, 40));
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
                    if preserve_viewport {
                        self.terminal_grid.view_scroll_up(scrolled_rows);
                    } else {
                        self.terminal_grid.view_scroll_to_bottom();
                    }
                    self.terminal_needs_layout = true;
                    should_redraw = true;
                }
                if let Some(grid) = self.terminal_buffer_grids.get_mut(&session_id) {
                    let scrolled_rows = grid.feed_bytes(&chunk);
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
                self.active_lsp_server = None;
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
            WorkerResultPayload::LspCheckResult {
                binary,
                install_cmd,
                is_installed,
                ..
            } => {
                if !is_installed {
                    // Hiển thị popup hướng dẫn cài LSP.
                    self.active_lsp_guide = Some(LspInstallGuide {
                        binary,
                        install_cmd,
                    });
                    self.request_redraw();
                }
                // Nếu đã cài: không cần làm gì (LSP đã/sẽ được start qua submit_lsp_did_open).
            }
            WorkerResultPayload::LspHoverResult { content, .. } => {
                use crate::app::app_state::{EditorOverlay, FloatingBoxStyle};
                if content.is_empty() {
                    return;
                }
                let (anchor_line, anchor_col) = self.app_state.cursor_line_col();
                let blocks = parse_hover_markdown_blocks(&content, &self.theme);
                if blocks.is_empty() {
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
                    self.request_redraw();
                }
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
            WorkerResultPayload::LspCompletionResult {
                items,
                cursor_line,
                cursor_col,
                prefix_start_col,
                prefix,
            } => {
                if items.is_empty() {
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
                    self.editor_caret_needs_layout = true;
                    self.request_redraw();
                }
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
            _ => {}
        }
    }

    fn on_stale_result(&mut self, _stale: WorkerResult) {}
}

/// Convert `file:///path/to/file` URI thành PathBuf.
fn lsp_uri_to_path(uri: &str) -> Option<std::path::PathBuf> {
    let url = url::Url::parse(uri).ok()?;
    let path = url.to_file_path().ok()?;
    path.canonicalize().ok().or(Some(path))
}

fn apply_lsp_text_edits(
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
