use super::*;

impl AppShell {
    pub(super) fn should_auto_trigger_lsp_completion_for_char(&self, ch: char) -> bool {
        self.app_state.current_mode() == EditorMode::Insert
            && self.active_lsp_server.is_some()
            && self.lsp_completion_trigger_chars.contains(&ch)
    }

    pub(super) fn queue_lsp_completion_after_debounce_if_needed(&mut self) {
        if self.app_state.current_mode() != EditorMode::Insert || self.active_lsp_server.is_none() {
            self.pending_lsp_completion_after_debounce = false;
            self.last_lsp_completion_type_at = None;
            return;
        }
        let Some(active_path) = self.app_state.active_file() else {
            self.pending_lsp_completion_after_debounce = false;
            self.last_lsp_completion_type_at = None;
            return;
        };
        let Some(profile) = crate::lsp::registry::language_profile_for_path(active_path) else {
            self.pending_lsp_completion_after_debounce = false;
            self.last_lsp_completion_type_at = None;
            return;
        };
        if !is_ts_js_profile_key(profile.key) {
            self.pending_lsp_completion_after_debounce = false;
            self.last_lsp_completion_type_at = None;
            return;
        }
        let (cursor_line, cursor_col) = self.app_state.cursor_line_col();
        let prefix = self
            .app_state
            .completion_prefix_info_at(cursor_line, cursor_col)
            .prefix;
        if prefix.chars().count() < 2 {
            self.pending_lsp_completion_after_debounce = false;
            self.last_lsp_completion_type_at = None;
            return;
        }
        self.pending_lsp_completion_after_debounce = true;
        self.last_lsp_completion_type_at = Some(std::time::Instant::now());
    }

    pub(in crate::app::event_loop) fn flush_pending_lsp_completion_after_debounce(&mut self) {
        if !self.pending_lsp_completion_after_debounce {
            return;
        }
        let Some(last) = self.last_lsp_completion_type_at else {
            self.pending_lsp_completion_after_debounce = false;
            return;
        };
        if last.elapsed() < LSP_COMPLETION_DEBOUNCE_INTERVAL {
            return;
        }
        self.pending_lsp_completion_after_debounce = false;
        self.last_lsp_completion_type_at = None;
        self.refresh_active_ts_js_export_cache();
        let _ = self.submit_lsp_completion();
    }

    fn refresh_active_ts_js_export_cache(&mut self) {
        let Some(active_path) = self.app_state.active_file().map(std::path::PathBuf::from) else {
            return;
        };
        let Some(profile) = crate::lsp::registry::language_profile_for_path(&active_path) else {
            return;
        };
        if !is_ts_js_profile_key(profile.key) {
            return;
        }
        let Some(workspace_root) = self.app_state.workspace_root_path().map(std::path::PathBuf::from)
        else {
            return;
        };
        let text = self.app_state.text_string();
        let symbols = crate::lsp::extract_ts_js_exports_from_text(&active_path, &workspace_root, &text);
        self.app_state
            .workspace_symbol_cache()
            .upsert_file_symbols(profile.key, &active_path, symbols);
    }

    pub(super) fn submit_lsp_completion(&mut self) -> bool {
        if self.active_lsp_server.is_none() {
            if self.pending_lsp_server.is_some() {
                self.show_transient_toast("LSP is starting up, please wait…".to_string());
            }
            return false;
        }
        self.app_state.set_completion_loading(true);
        self.force_flush_lsp_did_change_for_active_file();
        let Some((language_id, uri, line, character)) = self.lsp_cursor_context() else {
            return false;
        };
        let (cursor_line, cursor_col) = self.app_state.cursor_line_col();
        let prefix_info = self
            .app_state
            .completion_prefix_info_at(cursor_line, cursor_col);
        let mut changed = self.app_state.clear_current_overlays();
        changed |= self.app_state.clear_completion();
        self.submit(RequestSpec {
            revision_id: 0,
            topic: RequestTopic::LspRequest,
            payload: WorkerRequestPayload::LspCompletionRequest {
                language_id,
                uri,
                line,
                character,
                cursor_line,
                cursor_col,
                prefix_start_col: prefix_info.start_col,
                prefix: prefix_info.prefix,
            },
        });
        changed
    }

    pub(super) fn select_next_completion_item(&mut self) -> bool {
        let changed = self.app_state.completion_select_next();
        if changed {
            self.update_completion_hover_doc_for_selection();
            self.schedule_completion_resolve_debounced();
            self.editor_caret_needs_layout = true;
            self.request_redraw();
        }
        changed
    }

    pub(super) fn select_prev_completion_item(&mut self) -> bool {
        let changed = self.app_state.completion_select_prev();
        if changed {
            self.update_completion_hover_doc_for_selection();
            self.schedule_completion_resolve_debounced();
            self.editor_caret_needs_layout = true;
            self.request_redraw();
        }
        changed
    }

    /// Mirror inline documentation immediately when selection changes.
    /// This prevents showing "Loading…" during debounce when inline docs are available.
    fn update_completion_hover_doc_for_selection(&mut self) {
        let Some(completion) = self.app_state.completion() else {
            return;
        };
        let Some(entry) = completion.filtered_items.get(completion.selected_index) else {
            return;
        };

        // Mirror inline docs immediately if available
        if let Some(doc) = entry
            .item
            .documentation
            .as_ref()
            .filter(|d| !d.trim().is_empty())
            .cloned()
        {
            self.app_state.set_completion_hover_doc(Some(doc));
        } else {
            // No inline docs - clear and show "Loading…" during resolve
            self.app_state.set_completion_hover_doc(None);
        }

        // Also trigger LSP hover to get rich documentation (like Shift+K)
        self.submit_lsp_hover_for_completion();
    }

    /// Submit LSP hover request for the currently selected completion item.
    /// This fetches the same rich documentation shown when pressing Shift+K.
    fn submit_lsp_hover_for_completion(&mut self) {
        if self.active_lsp_server.is_none() {
            return;
        }
        let Some(completion) = self.app_state.completion() else {
            return;
        };
        let Some((language_id, uri, _line, _character)) = self.lsp_cursor_context() else {
            return;
        };

        // Calculate hover position: trigger position + length of selected item's label
        let trigger_line = completion.trigger_pos.line;
        let trigger_col = completion.trigger_pos.col;

        // For hover, we want to query at the position of the symbol being completed
        // Use trigger position as the hover target
        self.submit(RequestSpec {
            revision_id: 0,
            topic: RequestTopic::LspRequest,
            payload: WorkerRequestPayload::LspHoverRequest {
                language_id,
                uri,
                line: trigger_line as u32,
                character: trigger_col as u32,
                for_completion: true,
                completion_revision: Some(completion.current_revision),
            },
        });
    }

    /// Mark a debounced LSP `completionItem/resolve` for the current selection.
    /// The actual request fires from `flush_pending_completion_resolve_after_debounce`
    /// once the user has dwelt on the item for `COMPLETION_RESOLVE_DEBOUNCE_INTERVAL`.
    /// Cancels any pending dwell timer from a prior selection (only the latest wins).
    pub(in crate::app::event_loop) fn schedule_completion_resolve_debounced(&mut self) {
        let Some(state) = self.app_state.completion() else {
            self.pending_completion_resolve_after_debounce = false;
            self.last_completion_resolve_select_at = None;
            return;
        };
        self.pending_completion_resolve_revision = state.current_revision;
        self.pending_completion_resolve_after_debounce = true;
        self.last_completion_resolve_select_at = Some(std::time::Instant::now());
    }

    pub(in crate::app::event_loop) fn flush_pending_completion_resolve_after_debounce(&mut self) {
        if !self.pending_completion_resolve_after_debounce {
            return;
        }
        let Some(last) = self.last_completion_resolve_select_at else {
            self.pending_completion_resolve_after_debounce = false;
            return;
        };
        if last.elapsed() < COMPLETION_RESOLVE_DEBOUNCE_INTERVAL {
            return;
        }
        self.pending_completion_resolve_after_debounce = false;
        self.last_completion_resolve_select_at = None;
        if self.app_state.completion().is_none() {
            return;
        }
        self.submit_completion_resolve();
    }

    /// Send `completionItem/resolve` for the currently selected item so its
    /// `documentation` and `detail` fields get filled in. When no fetch is needed
    /// (or possible), marks the doc-state as resolved so the UI swaps "Loading…"
    /// for either inline docs or "No docs available".
    pub(in crate::app::event_loop) fn submit_completion_resolve(&mut self) {
        self.completion_resolve_request_id = None;
        let Some(completion) = self.app_state.completion() else {
            return;
        };
        let Some(entry) = completion.filtered_items.get(completion.selected_index) else {
            return;
        };
        // Inline doc already present: no resolve needed. Mirror it into hover_doc too
        // so the doc panel stays populated even after selection changes cleared the
        // transient resolved-doc slot.
        if let Some(doc) = entry
            .item
            .documentation
            .as_ref()
            .filter(|doc| !doc.trim().is_empty())
            .cloned()
        {
            self.app_state.set_completion_hover_doc(Some(doc));
            return;
        }
        let Some(item_json) = entry.item.raw_json.clone() else {
            // Synthesized item (tests) — nothing to resolve.
            self.app_state.mark_completion_hover_doc_resolved();
            return;
        };
        let Some((language_id, uri, _line, _character)) = self.lsp_cursor_context() else {
            self.app_state.mark_completion_hover_doc_resolved();
            return;
        };
        let item_label = entry.item.label.clone();
        let completion_revision = completion.current_revision;
        let request = self.submit(RequestSpec {
            revision_id: 0,
            topic: RequestTopic::LspRequest,
            payload: WorkerRequestPayload::LspCompletionResolveRequest {
                language_id,
                uri,
                item_json,
                item_label,
                completion_revision,
            },
        });
        match request {
            Some(req) => {
                self.completion_resolve_request_id = Some(req.request_id);
            }
            None => {
                // Scheduler refused to enqueue — treat as resolved with no docs.
                self.app_state.mark_completion_hover_doc_resolved();
            }
        }
    }

    pub(in crate::app::event_loop) fn submit_completion_virtual_hover_fallback(
        &mut self,
        item_label: String,
        completion_revision: u64,
    ) {
        if self.active_lsp_server.is_none() {
            return;
        }
        let Some(completion) = self.app_state.completion() else {
            return;
        };
        if completion.current_revision != completion_revision {
            return;
        }
        let Some(entry) = completion.filtered_items.get(completion.selected_index) else {
            return;
        };
        if entry.item.label != item_label {
            return;
        }
        let Some((language_id, uri, _line, _character)) = self.lsp_cursor_context() else {
            return;
        };

        let mut insert_text = entry
            .item
            .insert_text
            .clone()
            .or(entry.item.text_edit_text.clone())
            .unwrap_or_else(|| entry.item.label.clone());
        if insert_text.is_empty() {
            return;
        }
        let trigger_col = completion.trigger_pos.col.saturating_sub(1);
        if let Some(ch) = self
            .app_state
            .char_at_line_col(completion.trigger_pos.line, trigger_col)
        {
            if self.lsp_completion_trigger_chars.contains(&ch) && insert_text.starts_with(ch) {
                insert_text = insert_text.chars().skip(1).collect();
            }
        }
        if insert_text.is_empty() {
            return;
        }

        let original_text = self.app_state.text_string();
        let mut text = original_text.clone();
        let start_byte = self
            .app_state
            .line_char_to_byte_idx(completion.trigger_pos.line, completion.trigger_pos.col);
        let end_col = completion.trigger_pos.col + completion.typed_prefix.chars().count();
        let end_byte = self
            .app_state
            .line_char_to_byte_idx(completion.trigger_pos.line, end_col);
        if start_byte > end_byte || end_byte > text.len() {
            return;
        }
        text.replace_range(start_byte..end_byte, &insert_text);

        let hover_character = completion.trigger_pos.col
            + insert_text.chars().count().saturating_sub(1);
        self.submit(RequestSpec {
            revision_id: 0,
            topic: RequestTopic::LspRequest,
            payload: WorkerRequestPayload::LspCompletionVirtualHoverRequest {
                language_id,
                uri,
                original_text,
                text,
                hover_line: completion.trigger_pos.line as u32,
                hover_character: hover_character as u32,
                completion_revision,
            },
        });
    }

    pub(super) fn close_completion_popup(&mut self) -> bool {
        self.pending_lsp_completion_after_debounce = false;
        self.last_lsp_completion_type_at = None;
        self.pending_completion_resolve_after_debounce = false;
        self.last_completion_resolve_select_at = None;
        let changed = self.app_state.clear_completion();
        if changed {
            self.editor_caret_needs_layout = true;
            self.request_redraw();
        }
        changed
    }

    pub(super) fn accept_completion_item(&mut self) -> bool {
        let Some(completion) = self.app_state.completion().cloned() else {
            return false;
        };
        let Some(entry) = completion
            .filtered_items
            .get(completion.selected_index)
            .cloned()
        else {
            return false;
        };
        let item = entry.item;
        let mut insert_text = item
            .insert_text
            .clone()
            .or(item.text_edit_text.clone())
            .unwrap_or(item.label.clone());
        if insert_text.is_empty() {
            return self.close_completion_popup();
        }

        // Strip a leading trigger char from insert_text when the buffer already
        // contains that char just before the typed prefix. This handles both:
        //   - user typed just `.`  (prefix=""), char_before_cursor = '.'
        //   - user typed `.trim`  (prefix="trim"), trigger char is '.' before prefix start
        // Without this, LSP items that include the leading '.' in insert_text would
        // produce `obj..trim()` instead of `obj.trim()`.
        let trigger_col = completion.trigger_pos.col.saturating_sub(1);
        if let Some(ch) = self
            .app_state
            .char_at_line_col(completion.trigger_pos.line, trigger_col)
        {
            if self.lsp_completion_trigger_chars.contains(&ch) && insert_text.starts_with(ch) {
                insert_text = insert_text.chars().skip(1).collect();
            }
        }

        if insert_text.is_empty() {
            return self.close_completion_popup();
        }

        let prefix_len = completion.typed_prefix.chars().count();
        let mut primary_edit = item.text_edit.clone().unwrap_or_else(|| {
            completion_prefix_text_edit(&completion, prefix_len, insert_text.clone())
        });
        primary_edit.new_text = insert_text.clone();
        let text = self.app_state.text_string();
        let mut edits = vec![primary_edit.clone()];
        edits.extend(item.additional_text_edits.clone());
        if item.export_kind.as_deref() == Some("named")
            && let Some(source_path) = item.source_path.as_deref()
        {
            edits.extend(synthesize_ts_named_import_edits(
                &text,
                self.app_state.active_file(),
                &item.label,
                source_path,
            ));
        }
        let target_byte = completion_cursor_byte_after_edits(&text, &edits, &primary_edit);
        let popup_closed = self.app_state.clear_completion();
        self.pending_completion_resolve_after_debounce = false;
        self.last_completion_resolve_select_at = None;
        let next = match super::async_results::apply_lsp_text_edits(&text, &edits) {
            Ok(next) => next,
            Err(err) => {
                eprintln!("[AppShell] completion accept failed to apply edits: {err}");
                return popup_closed;
            }
        };
        let changed = self
            .app_state
            .replace_active_document_text_preserve_cursor_with_undo(&next);
        if changed {
            if let Some(target_byte) = target_byte {
                let clamped = target_byte.min(next.len());
                let line = self.app_state.byte_to_line_idx(clamped);
                let line_start = self.app_state.line_start_byte_idx(line);
                let col = next[line_start.min(next.len())..clamped].chars().count();
                let _ = self.app_state.jump_to_line_and_column(line, col);
            }
            self.reconcile_highlight_spans_with_pending_edits();
            self.editor_needs_layout = true;
            self.editor_caret_needs_layout = true;
            let viewport_lines = self.editor_viewport_lines();
            self.app_state.auto_scroll_to_cursor(viewport_lines);
            self.submit_lsp_did_open_for_active_file();
        } else if popup_closed {
            self.editor_caret_needs_layout = true;
        }
        if popup_closed || changed {
            self.request_redraw();
        }
        popup_closed || changed
    }

    pub(super) fn refresh_open_completion_after_text_edit(&mut self) -> bool {
        let Some(completion) = self.app_state.completion().cloned() else {
            return false;
        };

        let (cursor_line, cursor_col) = self.app_state.cursor_line_col();
        if cursor_line != completion.trigger_pos.line || cursor_col < completion.trigger_pos.col {
            return self.close_completion_popup();
        }

        let prefix_info = self
            .app_state
            .completion_prefix_info_at(cursor_line, cursor_col);
        if prefix_info.start_col != completion.trigger_pos.col {
            return self.close_completion_popup();
        }

        let changed = self
            .app_state
            .refresh_completion_with_prefix(&prefix_info.prefix);
        if self
            .app_state
            .completion()
            .is_some_and(|state| state.filtered_items.is_empty())
        {
            return self.close_completion_popup() || changed;
        }

        if changed {
            self.editor_caret_needs_layout = true;
        }
        changed
    }
}

fn is_ts_js_profile_key(key: &str) -> bool {
    matches!(key, "typescript" | "tsx" | "javascript" | "jsx")
}

fn completion_prefix_text_edit(
    completion: &crate::app::app_state::CompletionState,
    prefix_len: usize,
    new_text: String,
) -> crate::async_runtime::message::LspTextEdit {
    let start_col = completion.prefix_col;
    let end_col = completion
        .anchor_col
        .max(start_col.saturating_add(prefix_len));
    crate::async_runtime::message::LspTextEdit {
        range: crate::async_runtime::message::LspRange {
            start: crate::async_runtime::message::LspPosition {
                line: completion.trigger_pos.line as u32,
                character: start_col as u32,
            },
            end: crate::async_runtime::message::LspPosition {
                line: completion.trigger_pos.line as u32,
                character: end_col as u32,
            },
        },
        new_text,
    }
}

fn completion_cursor_byte_after_edits(
    source: &str,
    edits: &[crate::async_runtime::message::LspTextEdit],
    primary: &crate::async_runtime::message::LspTextEdit,
) -> Option<usize> {
    let (primary_start, _primary_end) = lsp_text_edit_byte_range(source, primary)?;
    let mut delta: isize = 0;
    for edit in edits {
        let (start, end) = lsp_text_edit_byte_range(source, edit)?;
        if start < primary_start {
            delta += edit.new_text.len() as isize - end.saturating_sub(start) as isize;
        }
    }
    let shifted_start = primary_start as isize + delta;
    (shifted_start >= 0).then_some(shifted_start as usize + primary.new_text.len())
}

fn lsp_text_edit_byte_range(
    source: &str,
    edit: &crate::async_runtime::message::LspTextEdit,
) -> Option<(usize, usize)> {
    let start = lsp_position_to_byte_idx(source, edit.range.start.line, edit.range.start.character)?;
    let end = lsp_position_to_byte_idx(source, edit.range.end.line, edit.range.end.character)?;
    (start <= end && end <= source.len()).then_some((start, end))
}

fn lsp_position_to_byte_idx(source: &str, line: u32, character: u32) -> Option<usize> {
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

    let mut lines = source.split_inclusive('\n');
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

fn synthesize_ts_named_import_edits(
    source: &str,
    active_file: Option<&std::path::Path>,
    symbol_name: &str,
    source_path: &std::path::Path,
) -> Vec<crate::async_runtime::message::LspTextEdit> {
    let Some(active_file) = active_file else {
        return Vec::new();
    };
    if active_file == source_path {
        return Vec::new();
    }
    let Some(module_specifier) = relative_module_specifier(active_file, source_path) else {
        return Vec::new();
    };
    if let Some(edit) = merge_named_import_edit(source, symbol_name, &module_specifier) {
        return vec![edit];
    }
    vec![new_named_import_edit(source, symbol_name, &module_specifier)]
}

fn merge_named_import_edit(
    source: &str,
    symbol_name: &str,
    module_specifier: &str,
) -> Option<crate::async_runtime::message::LspTextEdit> {
    for (line_idx, line) in source.lines().enumerate() {
        let trimmed = line.trim_start();
        if !trimmed.starts_with("import ") {
            continue;
        }
        if !(trimmed.contains(&format!("from '{}'", module_specifier))
            || trimmed.contains(&format!("from \"{}\"", module_specifier)))
        {
            continue;
        }
        let Some(open) = line.find('{') else {
            continue;
        };
        let Some(close) = line[open..].find('}').map(|idx| open + idx) else {
            continue;
        };
        if line[open + 1..close]
            .split(',')
            .any(|part| part.trim() == symbol_name)
        {
            return Some(empty_lsp_text_edit(line_idx, close));
        }
        let mut insert_col = close;
        while insert_col > open + 1 && line.as_bytes()[insert_col.saturating_sub(1)] == b' ' {
            insert_col = insert_col.saturating_sub(1);
        }
        let insert_text = if line[open + 1..close].trim().is_empty() {
            symbol_name.to_string()
        } else if insert_col == close {
            format!(", {symbol_name} ")
        } else {
            format!(", {symbol_name}")
        };
        return Some(lsp_insert_text_edit(line_idx, insert_col, insert_text));
    }
    None
}

fn new_named_import_edit(
    source: &str,
    symbol_name: &str,
    module_specifier: &str,
) -> crate::async_runtime::message::LspTextEdit {
    let insert_line = import_insert_line(source);
    let new_text = format!("import {{ {symbol_name} }} from '{module_specifier}';\n");
    crate::async_runtime::message::LspTextEdit {
        range: crate::async_runtime::message::LspRange {
            start: crate::async_runtime::message::LspPosition {
                line: insert_line as u32,
                character: 0,
            },
            end: crate::async_runtime::message::LspPosition {
                line: insert_line as u32,
                character: 0,
            },
        },
        new_text,
    }
}

fn empty_lsp_text_edit(line_idx: usize, col: usize) -> crate::async_runtime::message::LspTextEdit {
    lsp_insert_text_edit(line_idx, col, String::new())
}

fn lsp_insert_text_edit(
    line_idx: usize,
    col: usize,
    new_text: String,
) -> crate::async_runtime::message::LspTextEdit {
    crate::async_runtime::message::LspTextEdit {
        range: crate::async_runtime::message::LspRange {
            start: crate::async_runtime::message::LspPosition {
                line: line_idx as u32,
                character: col as u32,
            },
            end: crate::async_runtime::message::LspPosition {
                line: line_idx as u32,
                character: col as u32,
            },
        },
        new_text,
    }
}

fn import_insert_line(source: &str) -> usize {
    let mut insert_line = 0usize;
    for (idx, line) in source.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.starts_with("import ") {
            insert_line = idx + 1;
            continue;
        }
        if idx == 0 && trimmed.starts_with("#!") {
            insert_line = 1;
            continue;
        }
        if idx == insert_line
            && (trimmed == "'use strict';"
                || trimmed == "\"use strict\";"
                || trimmed == "'use client';"
                || trimmed == "\"use client\";")
        {
            insert_line = idx + 1;
            continue;
        }
        if idx > insert_line {
            break;
        }
    }
    insert_line
}

fn relative_module_specifier(
    active_file: &std::path::Path,
    source_path: &std::path::Path,
) -> Option<String> {
    let from_dir = active_file.parent()?;
    let from_components: Vec<_> = from_dir.components().collect();
    let to_without_ext = strip_ts_js_extension(source_path);
    let to_components: Vec<_> = to_without_ext.components().collect();
    let mut shared = 0usize;
    while shared < from_components.len()
        && shared < to_components.len()
        && from_components[shared] == to_components[shared]
    {
        shared += 1;
    }

    let mut parts = Vec::new();
    for _ in shared..from_components.len() {
        parts.push("..".to_string());
    }
    for component in &to_components[shared..] {
        parts.push(component.as_os_str().to_string_lossy().to_string());
    }
    if parts.last().is_some_and(|part| part == "index") {
        parts.pop();
    }
    if parts.is_empty() {
        return None;
    }
    let mut value = parts.join("/");
    if !value.starts_with('.') {
        value = format!("./{value}");
    }
    Some(value)
}

fn strip_ts_js_extension(path: &std::path::Path) -> std::path::PathBuf {
    let mut value = path.to_path_buf();
    if matches!(
        value.extension().and_then(|ext| ext.to_str()),
        Some("ts" | "tsx" | "js" | "jsx" | "mjs" | "cjs")
    ) {
        value.set_extension("");
    }
    value
}
