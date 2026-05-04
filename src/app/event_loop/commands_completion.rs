use super::*;

impl AppShell {
    pub(super) fn should_auto_trigger_lsp_completion_for_char(&self, ch: char) -> bool {
        self.app_state.current_mode() == EditorMode::Insert
            && self.active_lsp_server.is_some()
            && self.lsp_completion_trigger_chars.contains(&ch)
    }

    pub(super) fn submit_lsp_completion(&mut self) -> bool {
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
            self.editor_caret_needs_layout = true;
            self.request_redraw();
        }
        changed
    }

    pub(super) fn select_prev_completion_item(&mut self) -> bool {
        let changed = self.app_state.completion_select_prev();
        if changed {
            self.editor_caret_needs_layout = true;
            self.request_redraw();
        }
        changed
    }

    pub(super) fn close_completion_popup(&mut self) -> bool {
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

        if let Some(char_left) = self.app_state.char_before_cursor() {
            if self.lsp_completion_trigger_chars.contains(&char_left) {
                if insert_text.starts_with(char_left) {
                    insert_text = insert_text.chars().skip(1).collect();
                }
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
