use super::*;

impl AppShell {
    pub(super) fn should_auto_trigger_lsp_completion_for_char(&self, ch: char) -> bool {
        self.app_state.current_mode() == EditorMode::Insert
            && self.active_lsp_server.is_some()
            && self.lsp_completion_trigger_chars.contains(&ch)
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
        let Some(item) = completion
            .filtered_items
            .get(completion.selected_index)
            .map(|entry| entry.item.clone())
        else {
            return false;
        };
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
        let popup_closed = self.app_state.clear_completion();
        let changed = self
            .app_state
            .replace_completion_prefix_at_cursor(prefix_len, &insert_text);
        if changed {
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
