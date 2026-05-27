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
        let prefix_info = self
            .app_state
            .completion_prefix_info_at(cursor_line, cursor_col);
        let is_member_access = prefix_info
            .start_col
            .checked_sub(1)
            .and_then(|col| self.app_state.char_at_line_col(cursor_line, col))
            .is_some_and(|ch| self.lsp_completion_trigger_chars.contains(&ch));
        if prefix_info.prefix.chars().count() < 2 && !is_member_access {
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
        self.force_flush_lsp_did_change_for_active_file();
        let Some((language_id, uri, line, character)) = self.lsp_cursor_context() else {
            self.app_state.set_completion_loading(false);
            return false;
        };
        self.app_state.set_completion_loading(true);
        let (cursor_line, cursor_col) = self.app_state.cursor_line_col();
        let prefix_info = self
            .app_state
            .completion_prefix_info_at(cursor_line, cursor_col);
        let mut changed = self.app_state.clear_current_overlays();
        changed |= self.app_state.clear_completion();
        let request = self.submit(RequestSpec {
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
        match request {
            Some(request) => {
                self.active_lsp_completion_request_id = Some(request.request_id);
            }
            None => {
                self.active_lsp_completion_request_id = None;
                self.app_state.set_completion_loading(false);
            }
        }
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
        let _ = self.submit_completion_resolve();
    }

    /// Send `completionItem/resolve` for the currently selected item so its
    /// `documentation` and `detail` fields get filled in. When no fetch is needed
    /// (or possible), marks the doc-state as resolved so the UI swaps "Loading…"
    /// for either inline docs or "No docs available".
    pub(in crate::app::event_loop) fn submit_completion_resolve(&mut self) -> bool {
        self.completion_resolve_request_id = None;
        let Some((item_json, item_label, completion_revision, documentation)) = self
            .app_state
            .completion()
            .and_then(|completion| {
                completion
                    .filtered_items
                    .get(completion.selected_index)
                    .map(|entry| {
                        (
                            entry.item.raw_json.clone(),
                            entry.item.label.clone(),
                            completion.current_revision,
                            entry
                                .item
                                .documentation
                                .as_ref()
                                .filter(|doc| !doc.trim().is_empty())
                                .cloned(),
                        )
                    })
            })
        else {
            return false;
        };

        // Inline doc already present: no resolve needed. Mirror it into hover_doc too
        // so the doc panel stays populated even after selection changes cleared the
        // transient resolved-doc slot.
        if let Some(doc) = documentation {
            self.app_state.set_completion_hover_doc(Some(doc));
            if item_json.is_none() {
                return false;
            }
        }
        let Some(item_json) = item_json else {
            // Synthesized item (tests) — nothing to resolve.
            self.app_state.mark_completion_hover_doc_resolved();
            return false;
        };
        let Some((language_id, uri, _line, _character)) = self.lsp_cursor_context() else {
            self.app_state.mark_completion_hover_doc_resolved();
            return false;
        };
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
                true
            }
            None => {
                // Scheduler refused to enqueue — treat as resolved with no docs.
                self.app_state.mark_completion_hover_doc_resolved();
                false
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

    pub(super) fn cancel_lsp_completion_pipeline(&mut self) {
        self.pending_lsp_completion_after_debounce = false;
        self.last_lsp_completion_type_at = None;
        self.pending_completion_resolve_after_debounce = false;
        self.last_completion_resolve_select_at = None;
        self.pending_completion_accept_after_resolve = None;
        self.active_lsp_completion_request_id = None;
        self.completion_resolve_request_id = None;
        self.app_state.set_completion_loading(false);
    }

    pub(super) fn close_completion_popup(&mut self) -> bool {
        self.cancel_lsp_completion_pipeline();
        let changed = self.app_state.clear_completion();
        if changed {
            self.editor_caret_needs_layout = true;
            self.request_redraw();
        }
        changed
    }

    pub(super) fn cancel_completion_and_return_normal(&mut self) -> bool {
        let mut changed = self.close_completion_popup();
        changed |= self.app_state.clear_current_overlays();
        if let Ok(result) = self.app_state.apply_mode_event(ModeEvent::Escape) {
            changed |= result.changed;
        }
        if changed {
            self.editor_caret_needs_layout = true;
            self.request_redraw();
        }
        changed
    }

    pub(in crate::app::event_loop) fn accept_completion_item(&mut self) -> bool {
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
            .text_edit_text
            .clone()
            .or(item.insert_text.clone())
            .unwrap_or(item.label.clone());
        if insert_text.is_empty() {
            return self.close_completion_popup();
        }

        let accept_after_resolve = self
            .pending_completion_accept_after_resolve
            .as_ref()
            .is_some_and(|(label, revision)| {
                label == &item.label && *revision == completion.current_revision
            });
        if should_resolve_lsp_completion_before_accept(&item) && !accept_after_resolve {
            self.pending_completion_accept_after_resolve =
                Some((item.label.clone(), completion.current_revision));
            self.pending_completion_resolve_after_debounce = false;
            self.last_completion_resolve_select_at = None;
            if self.submit_completion_resolve() {
                return true;
            }
            self.pending_completion_accept_after_resolve = None;
        }

        let prefix_len = completion.typed_prefix.chars().count();
        let text = self.app_state.text_string();
        let fallback_primary_edit =
            completion_prefix_text_edit(&completion, prefix_len, insert_text.clone());
        let mut primary_edit = item
            .text_edit
            .clone()
            .filter(|edit| completion_text_edit_covers_typed_prefix(edit, &completion, prefix_len))
            .unwrap_or(fallback_primary_edit);
        insert_text = normalize_member_completion_insert_text(
            &text,
            &completion,
            &primary_edit,
            insert_text,
            &self.lsp_completion_trigger_chars,
        );
        if insert_text.is_empty() {
            return self.close_completion_popup();
        }
        let has_existing_call_parens =
            completion_source_has_call_parens_after_edit(&text, &primary_edit);
        let (insert_text, primary_cursor_offset) =
            completion_accept_text_and_cursor_offset(&item, insert_text, has_existing_call_parens);
        primary_edit.new_text = insert_text.clone();
        let mut edits = vec![primary_edit.clone()];
        edits.extend(item.additional_text_edits.clone());
        if item.additional_text_edits.is_empty() {
            let import_style = ts_js_module_style_for_completion(&text, self.app_state.active_file());
            if let Some(module_specifier) = completion_module_specifier(
                self.app_state.active_file(),
                item.source_path.as_deref(),
                item.import_path.as_deref(),
            ) {
                match item.export_kind.as_deref() {
                    Some("named") => edits.extend(synthesize_ts_named_import_edits(
                        &text,
                        &item.label,
                        &module_specifier,
                        import_style,
                    )),
                    Some("default") => edits.extend(synthesize_ts_default_import_edits(
                        &text,
                        &item.label,
                        &module_specifier,
                        import_style,
                    )),
                    _ => {}
                }
            }
        }
        let target_byte =
            completion_cursor_byte_after_edits(&text, &edits, &primary_edit, primary_cursor_offset);
        let popup_closed = self.app_state.clear_completion();
        self.pending_completion_accept_after_resolve = None;
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
            let scroll_line_before_cursor_restore = self.app_state.scroll_line();
            if let Some(target_byte) = target_byte {
                let clamped = target_byte.min(next.len());
                let line = self.app_state.byte_to_line_idx(clamped);
                let line_start = self.app_state.line_start_byte_idx(line);
                let col = next[line_start.min(next.len())..clamped].chars().count();
                let _ = self.app_state.jump_to_line_and_column(line, col);
            }
            self.app_state
                .set_target_scroll_line(scroll_line_before_cursor_restore);
            self.editor_needs_layout = true;
            self.editor_caret_needs_layout = true;
            self.invalidate_highlights_and_parse_active_buffer();
            self.editor_caret_needs_layout = true;
            self.force_flush_lsp_did_change_for_active_file();
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
            // Context changed (e.g., deleted trigger char like "bootstrap.")
            // Cancel pending completion to prevent immediate re-trigger
            self.pending_lsp_completion_after_debounce = false;
            self.last_lsp_completion_type_at = None;
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TsJsModuleStyle {
    EsModule,
    CommonJs,
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

fn completion_text_edit_covers_typed_prefix(
    edit: &crate::async_runtime::message::LspTextEdit,
    completion: &crate::app::app_state::CompletionState,
    prefix_len: usize,
) -> bool {
    if prefix_len == 0 {
        return true;
    }
    let line = completion.trigger_pos.line as u32;
    if edit.range.start.line != line || edit.range.end.line != line {
        return false;
    }
    let prefix_start = completion.prefix_col as u32;
    let prefix_end = completion
        .anchor_col
        .max(completion.prefix_col.saturating_add(prefix_len)) as u32;
    edit.range.start.character <= prefix_start && edit.range.end.character >= prefix_end
}

fn normalize_member_completion_insert_text(
    source: &str,
    completion: &crate::app::app_state::CompletionState,
    primary_edit: &crate::async_runtime::message::LspTextEdit,
    mut insert_text: String,
    trigger_chars: &[char],
) -> String {
    if !completion_edit_covers_trigger_char(primary_edit, completion) {
        if let Some(trigger_char) = completion_char_before_prefix(source, completion) {
            if trigger_chars.contains(&trigger_char) && insert_text.starts_with(trigger_char) {
                insert_text = insert_text.chars().skip(1).collect();
            }
        }
    }

    if !completion_edit_covers_member_qualifier(source, primary_edit, completion) {
        if let Some(qualifier) = completion_member_qualifier_before_prefix(source, completion) {
            if insert_text.starts_with(&qualifier.text) {
                insert_text = insert_text[qualifier.text.len()..].to_string();
            }
        }
    }

    insert_text
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct CompletionMemberQualifier {
    start_col: usize,
    end_col: usize,
    text: String,
}

fn completion_edit_covers_trigger_char(
    edit: &crate::async_runtime::message::LspTextEdit,
    completion: &crate::app::app_state::CompletionState,
) -> bool {
    let Some(trigger_col) = completion.prefix_col.checked_sub(1) else {
        return false;
    };
    let line = completion.trigger_pos.line as u32;
    edit.range.start.line == line
        && edit.range.end.line == line
        && edit.range.start.character <= trigger_col as u32
        && edit.range.end.character >= completion.prefix_col as u32
}

fn completion_edit_covers_member_qualifier(
    source: &str,
    edit: &crate::async_runtime::message::LspTextEdit,
    completion: &crate::app::app_state::CompletionState,
) -> bool {
    let Some(qualifier) = completion_member_qualifier_before_prefix(source, completion) else {
        return false;
    };
    let line = completion.trigger_pos.line as u32;
    edit.range.start.line == line
        && edit.range.end.line == line
        && edit.range.start.character <= qualifier.start_col as u32
        && edit.range.end.character >= qualifier.end_col as u32
}

fn completion_char_before_prefix(
    source: &str,
    completion: &crate::app::app_state::CompletionState,
) -> Option<char> {
    let line = completion_source_line(source, completion.trigger_pos.line)?;
    let prefix_byte = char_col_to_byte_idx(line, completion.prefix_col)?;
    line[..prefix_byte].chars().next_back()
}

fn completion_member_qualifier_before_prefix(
    source: &str,
    completion: &crate::app::app_state::CompletionState,
) -> Option<CompletionMemberQualifier> {
    let line = completion_source_line(source, completion.trigger_pos.line)?;
    let prefix_byte = char_col_to_byte_idx(line, completion.prefix_col)?;
    let before_prefix = &line[..prefix_byte];
    if !before_prefix.ends_with('.') {
        return None;
    }

    let start_byte = before_prefix
        .char_indices()
        .rev()
        .find_map(|(idx, ch)| {
            (!is_member_qualifier_char(ch)).then_some(idx.saturating_add(ch.len_utf8()))
        })
        .unwrap_or(0);
    let text = before_prefix[start_byte..].to_string();
    if text.len() <= 1 {
        return None;
    }
    Some(CompletionMemberQualifier {
        start_col: before_prefix[..start_byte].chars().count(),
        end_col: completion.prefix_col,
        text,
    })
}

fn completion_source_line(source: &str, line_idx: usize) -> Option<&str> {
    source
        .split('\n')
        .nth(line_idx)
        .map(|line| line.strip_suffix('\r').unwrap_or(line))
}

fn char_col_to_byte_idx(text: &str, col: usize) -> Option<usize> {
    let mut current_col = 0usize;
    for (byte_idx, _ch) in text.char_indices() {
        if current_col == col {
            return Some(byte_idx);
        }
        current_col = current_col.saturating_add(1);
    }
    (current_col == col).then_some(text.len())
}

fn is_member_qualifier_char(ch: char) -> bool {
    ch.is_ascii_alphanumeric() || matches!(ch, '_' | '$' | '.' | '?')
}

fn should_resolve_lsp_completion_before_accept(
    item: &crate::async_runtime::message::LspCompletionItem,
) -> bool {
    item.raw_json.is_some()
        && item.additional_text_edits.is_empty()
        && item.source_path.is_none()
        && item.export_kind.is_none()
}

fn completion_accept_text_and_cursor_offset(
    item: &crate::async_runtime::message::LspCompletionItem,
    mut insert_text: String,
    has_existing_call_parens: bool,
) -> (String, usize) {
    let mut cursor_offset = insert_text.len();
    if !completion_item_is_callable(item) {
        return (insert_text, cursor_offset);
    }

    if has_existing_call_parens {
        if let Some((open, _close)) = completion_insert_text_call_parens(&insert_text) {
            insert_text = insert_text[..open].trim_end().to_string();
        }
        cursor_offset = insert_text.len();
        return (insert_text, cursor_offset);
    }

    let has_parameters = item
        .has_parameters
        .or_else(|| completion_insert_text_has_parameters(&insert_text))
        .or_else(|| {
            item.detail
                .as_deref()
                .and_then(|detail| completion_detail_has_parameters(&item.label, detail))
        })
        .unwrap_or(false);

    if let Some((open, close)) = completion_insert_text_call_parens(&insert_text) {
        cursor_offset = if has_parameters && insert_text[open + 1..close].trim().is_empty() {
            open + 1
        } else {
            insert_text.len()
        };
        return (insert_text, cursor_offset);
    }

    if completion_insert_text_can_append_call(&insert_text) {
        insert_text.push_str("()");
        cursor_offset = if has_parameters {
            insert_text.len().saturating_sub(1)
        } else {
            insert_text.len()
        };
    }

    (insert_text, cursor_offset)
}

fn completion_source_has_call_parens_after_edit(
    source: &str,
    primary_edit: &crate::async_runtime::message::LspTextEdit,
) -> bool {
    let Some((_start, end)) = lsp_text_edit_byte_range(source, primary_edit) else {
        return false;
    };
    source
        .get(end..)
        .is_some_and(|suffix| suffix.trim_start().starts_with('('))
}

fn completion_item_is_callable(item: &crate::async_runtime::message::LspCompletionItem) -> bool {
    item.callable.unwrap_or_else(|| {
        item.has_parameters.is_some() || item.kind.is_some_and(completion_kind_is_callable)
    })
}

fn completion_kind_is_callable(kind: u32) -> bool {
    matches!(kind, 2 | 3 | 4)
}

fn completion_insert_text_has_parameters(text: &str) -> Option<bool> {
    let (open, close) = completion_insert_text_call_parens(text)?;
    Some(!text[open + 1..close].trim().is_empty())
}

fn completion_insert_text_call_parens(text: &str) -> Option<(usize, usize)> {
    let trimmed = text.trim_end();
    let close = trimmed.len().checked_sub(1)?;
    if trimmed.as_bytes().get(close) != Some(&b')') {
        return None;
    }
    let mut depth = 0usize;
    for (idx, ch) in trimmed.char_indices().rev() {
        match ch {
            ')' => depth = depth.saturating_add(1),
            '(' => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    return Some((idx, close));
                }
            }
            _ => {}
        }
    }
    None
}

fn completion_insert_text_can_append_call(text: &str) -> bool {
    let trimmed = text.trim_end();
    if trimmed.is_empty() || trimmed.contains('\n') || trimmed.contains('\r') {
        return false;
    }
    !matches!(
        trimmed.chars().last(),
        Some(')' | ']' | '}' | ';' | '"' | '\'' | '`')
    )
}

fn completion_detail_has_parameters(label: &str, detail: &str) -> Option<bool> {
    let label = label
        .split('(')
        .next()
        .unwrap_or(label)
        .rsplit('.')
        .next()
        .unwrap_or(label)
        .trim();
    let mut search_from = 0usize;
    while let Some(relative_open) = detail[search_from..].find('(') {
        let open = search_from + relative_open;
        let Some(close) = completion_matching_paren(detail, open) else {
            break;
        };
        if detail[..open].trim().is_empty() {
            let after = detail[close + 1..].trim_start();
            if after.is_empty()
                || after.starts_with("=>")
                || after.starts_with(':')
                || after.starts_with("->")
            {
                return Some(!detail[open + 1..close].trim().is_empty());
            }
        }
        if completion_go_func_parens_look_like_params(&detail[..open], &detail[close + 1..]) {
            return Some(!detail[open + 1..close].trim().is_empty());
        }
        if completion_signature_parens_look_like_call(label, &detail[..open]) {
            return Some(!detail[open + 1..close].trim().is_empty());
        }
        search_from = close + 1;
    }
    None
}

fn completion_go_func_parens_look_like_params(before_open: &str, after_close: &str) -> bool {
    let before = before_open.trim_end();
    if before != "func" {
        return false;
    }
    let after = after_close.trim_start();
    !after
        .chars()
        .next()
        .is_some_and(|ch| ch.is_ascii_alphabetic() || ch == '_' || ch == '$')
}

fn completion_signature_parens_look_like_call(label: &str, before_open: &str) -> bool {
    let before = before_open.trim_end();
    if before.is_empty() || label.is_empty() {
        return false;
    }
    let before = completion_strip_trailing_type_args(before);
    let token = before
        .rsplit(|ch: char| !(ch.is_ascii_alphanumeric() || ch == '_' || ch == '$' || ch == '.'))
        .next()
        .unwrap_or("")
        .rsplit('.')
        .next()
        .unwrap_or("");
    token == label
        || before_open.trim_end().ends_with(&format!("{label}:"))
        || before_open.contains(&format!(" {label}:"))
}

fn completion_strip_trailing_type_args(text: &str) -> &str {
    let trimmed = text.trim_end();
    if !trimmed.ends_with('>') {
        return trimmed;
    }
    let mut depth = 0usize;
    for (idx, ch) in trimmed.char_indices().rev() {
        match ch {
            '>' => depth = depth.saturating_add(1),
            '<' => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    return trimmed[..idx].trim_end();
                }
            }
            _ => {}
        }
    }
    trimmed
}

fn completion_matching_paren(text: &str, open: usize) -> Option<usize> {
    let mut depth = 0usize;
    let mut quote = None;
    let mut escape = false;
    for (idx, ch) in text[open..].char_indices() {
        let absolute = open + idx;
        if let Some(quote_char) = quote {
            if escape {
                escape = false;
                continue;
            }
            if ch == '\\' {
                escape = true;
                continue;
            }
            if ch == quote_char {
                quote = None;
            }
            continue;
        }
        match ch {
            '\'' | '"' | '`' => quote = Some(ch),
            '(' => depth = depth.saturating_add(1),
            ')' => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    return Some(absolute);
                }
            }
            _ => {}
        }
    }
    None
}

fn completion_cursor_byte_after_edits(
    source: &str,
    edits: &[crate::async_runtime::message::LspTextEdit],
    primary: &crate::async_runtime::message::LspTextEdit,
    primary_cursor_offset: usize,
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
    (shifted_start >= 0).then_some(shifted_start as usize + primary_cursor_offset)
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
    symbol_name: &str,
    module_specifier: &str,
    import_style: TsJsModuleStyle,
) -> Vec<crate::async_runtime::message::LspTextEdit> {
    match import_style {
        TsJsModuleStyle::EsModule => {
            if let Some(edit) = merge_named_import_edit(source, symbol_name, module_specifier) {
                return vec![edit];
            }
            vec![new_named_import_edit(source, symbol_name, module_specifier)]
        }
        TsJsModuleStyle::CommonJs => {
            if let Some(edit) = merge_named_require_edit(source, symbol_name, module_specifier) {
                return vec![edit];
            }
            vec![new_named_require_edit(source, symbol_name, module_specifier)]
        }
    }
}

fn synthesize_ts_default_import_edits(
    source: &str,
    symbol_name: &str,
    module_specifier: &str,
    import_style: TsJsModuleStyle,
) -> Vec<crate::async_runtime::message::LspTextEdit> {
    match import_style {
        TsJsModuleStyle::EsModule => {
            if let Some(edit) = merge_default_import_edit(source, symbol_name, module_specifier) {
                return vec![edit];
            }
            vec![new_default_import_edit(source, symbol_name, module_specifier)]
        }
        TsJsModuleStyle::CommonJs => {
            if let Some(edit) = merge_default_require_edit(source, symbol_name, module_specifier) {
                return vec![edit];
            }
            vec![new_default_require_edit(source, symbol_name, module_specifier)]
        }
    }
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

fn merge_default_import_edit(
    source: &str,
    symbol_name: &str,
    module_specifier: &str,
) -> Option<crate::async_runtime::message::LspTextEdit> {
    for (line_idx, line) in source.lines().enumerate() {
        let trimmed = line.trim_start();
        if !trimmed.starts_with("import ") || trimmed.starts_with("import type ") {
            continue;
        }
        if !(trimmed.contains(&format!("from '{}'", module_specifier))
            || trimmed.contains(&format!("from \"{}\"", module_specifier)))
        {
            continue;
        }
        let Some(import_col) = line.find("import ").map(|idx| idx + "import ".len()) else {
            continue;
        };
        let after_import = line[import_col..].trim_start();
        let leading_spaces = line[import_col..].len() - after_import.len();
        let insert_col = import_col + leading_spaces;
        if after_import.starts_with(symbol_name)
            && after_import[symbol_name.len()..]
                .chars()
                .next()
                .is_some_and(|ch| ch == ',' || ch.is_whitespace())
        {
            return Some(empty_lsp_text_edit(line_idx, insert_col));
        }
        if after_import.starts_with('{') {
            return Some(lsp_insert_text_edit(
                line_idx,
                insert_col,
                format!("{symbol_name}, "),
            ));
        }
        return None;
    }
    None
}

fn merge_named_require_edit(
    source: &str,
    symbol_name: &str,
    module_specifier: &str,
) -> Option<crate::async_runtime::message::LspTextEdit> {
    for (line_idx, line) in source.lines().enumerate() {
        let trimmed = line.trim_start();
        if !is_commonjs_require_line(trimmed) || !line_requires_module(trimmed, module_specifier) {
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

fn merge_default_require_edit(
    source: &str,
    symbol_name: &str,
    module_specifier: &str,
) -> Option<crate::async_runtime::message::LspTextEdit> {
    for (line_idx, line) in source.lines().enumerate() {
        let trimmed = line.trim_start();
        if !is_commonjs_require_line(trimmed) || !line_requires_module(trimmed, module_specifier) {
            continue;
        }
        let Some(after_keyword) = strip_commonjs_declaration_keyword(trimmed) else {
            continue;
        };
        let Some((name, _rest)) = read_ts_js_identifier_with_rest(after_keyword.trim_start()) else {
            continue;
        };
        if name == symbol_name {
            let col = line.find(&name).unwrap_or(0);
            return Some(empty_lsp_text_edit(line_idx, col));
        }
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

fn new_default_import_edit(
    source: &str,
    symbol_name: &str,
    module_specifier: &str,
) -> crate::async_runtime::message::LspTextEdit {
    let insert_line = import_insert_line(source);
    let new_text = format!("import {symbol_name} from '{module_specifier}';\n");
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

fn new_named_require_edit(
    source: &str,
    symbol_name: &str,
    module_specifier: &str,
) -> crate::async_runtime::message::LspTextEdit {
    let insert_line = commonjs_require_insert_line(source);
    let new_text = format!("const {{ {symbol_name} }} = require('{module_specifier}');\n");
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

fn new_default_require_edit(
    source: &str,
    symbol_name: &str,
    module_specifier: &str,
) -> crate::async_runtime::message::LspTextEdit {
    let insert_line = commonjs_require_insert_line(source);
    let new_text = format!("const {symbol_name} = require('{module_specifier}');\n");
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

fn commonjs_require_insert_line(source: &str) -> usize {
    let mut insert_line = 0usize;
    for (idx, line) in source.lines().enumerate() {
        let trimmed = line.trim();
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
        if is_commonjs_require_line(trimmed) {
            insert_line = idx + 1;
            continue;
        }
        if idx > insert_line {
            break;
        }
    }
    insert_line
}

fn completion_module_specifier(
    active_file: Option<&std::path::Path>,
    source_path: Option<&std::path::Path>,
    import_path: Option<&str>,
) -> Option<String> {
    if source_path.is_some_and(path_contains_node_modules) {
        return import_path.map(str::to_string);
    }
    let Some(source_path) = source_path else {
        return import_path.map(str::to_string);
    };
    let Some(active_file) = active_file else {
        return import_path.map(str::to_string);
    };
    if active_file == source_path {
        return None;
    }
    relative_module_specifier(active_file, source_path).or_else(|| import_path.map(str::to_string))
}

fn path_contains_node_modules(path: &std::path::Path) -> bool {
    path.components().any(|component| {
        matches!(
            component,
            std::path::Component::Normal(name) if name.to_str() == Some("node_modules")
        )
    })
}

fn ts_js_module_style_for_completion(
    source: &str,
    active_file: Option<&std::path::Path>,
) -> TsJsModuleStyle {
    if let Some(extension) = active_file
        .and_then(|path| path.extension())
        .and_then(|extension| extension.to_str())
        .map(|extension| extension.to_ascii_lowercase())
    {
        match extension.as_str() {
            "cjs" | "cts" => return TsJsModuleStyle::CommonJs,
            "mjs" | "mts" | "jsx" | "tsx" => return TsJsModuleStyle::EsModule,
            _ => {}
        }
    }

    if source_uses_es_modules(source) {
        return TsJsModuleStyle::EsModule;
    }
    if source_uses_commonjs(source) {
        return TsJsModuleStyle::CommonJs;
    }
    if active_file
        .and_then(|path| path.extension())
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case("ts"))
    {
        return TsJsModuleStyle::EsModule;
    }
    TsJsModuleStyle::CommonJs
}

fn source_uses_es_modules(source: &str) -> bool {
    source.lines().any(|line| {
        let trimmed = line.trim_start();
        trimmed.starts_with("import ") || trimmed.starts_with("export ")
    })
}

fn source_uses_commonjs(source: &str) -> bool {
    source.lines().any(|line| {
        let trimmed = line.trim_start();
        trimmed.contains("require(")
            || trimmed.starts_with("module.exports")
            || trimmed.starts_with("exports.")
    })
}

fn is_commonjs_require_line(trimmed: &str) -> bool {
    trimmed.starts_with("require(")
        || strip_commonjs_declaration_keyword(trimmed)
            .is_some_and(|rest| rest.contains("require("))
}

fn line_requires_module(line: &str, module_specifier: &str) -> bool {
    line.contains(&format!("require('{module_specifier}')"))
        || line.contains(&format!("require(\"{module_specifier}\")"))
}

fn strip_commonjs_declaration_keyword(input: &str) -> Option<&str> {
    ["const", "let", "var"]
        .into_iter()
        .find_map(|keyword| strip_ts_js_leading_word(input, keyword))
}

fn strip_ts_js_leading_word<'a>(input: &'a str, word: &str) -> Option<&'a str> {
    let rest = input.strip_prefix(word)?;
    if rest
        .chars()
        .next()
        .is_some_and(|ch| ch.is_ascii_alphanumeric() || ch == '_' || ch == '$')
    {
        return None;
    }
    Some(rest)
}

fn read_ts_js_identifier_with_rest(input: &str) -> Option<(String, &str)> {
    let mut out = String::new();
    let mut end = 0usize;
    for (idx, ch) in input.char_indices() {
        if out.is_empty() {
            if ch.is_ascii_alphabetic() || ch == '_' || ch == '$' {
                out.push(ch);
                end = idx + ch.len_utf8();
                continue;
            }
            return None;
        }
        if ch.is_ascii_alphanumeric() || ch == '_' || ch == '$' {
            out.push(ch);
            end = idx + ch.len_utf8();
        } else {
            break;
        }
    }
    (!out.is_empty()).then_some((out, &input[end..]))
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
