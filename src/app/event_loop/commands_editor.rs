use super::*;

impl AppShell {
    fn try_move_soft_wrap_visual_line(&mut self, down: bool) -> bool {
        let Some(renderer) = self.renderer.as_ref() else {
            return false;
        };
        let Some(center_bounds) = self.last_editor_bounds else {
            return false;
        };
        let (cursor_line, _) = self.app_state.cursor_line_col();
        let Some(target_col) =
            renderer.soft_wrap_visual_move_target(&self.app_state, center_bounds, down)
        else {
            return false;
        };
        let before = self.app_state.cursor_char_idx();
        self.app_state
            .jump_to_line_and_column(cursor_line, target_col)
            && self.app_state.cursor_char_idx() != before
    }

    pub(super) fn handle_insert_edit_command(
        &mut self,
        command: &Command,
        repeat_count: usize,
    ) -> Option<bool> {
        match command {
            Command::InsertChar(_)
            | Command::InsertText(_)
            | Command::Backspace
            | Command::Newline => {
                // Clear semantic highlights and increment revision when typing/deleting
                self.semantic_highlight_request_revision =
                    self.semantic_highlight_request_revision.saturating_add(1);
                let semantic_symbols_cleared = self.app_state.clear_semantic_symbol_highlights();
                let semantic_spans_cleared = !self.semantic_highlight_spans.is_empty();
                if semantic_spans_cleared {
                    self.semantic_highlight_spans.clear();
                }
                let highlights_cleared = semantic_symbols_cleared || semantic_spans_cleared;
                if highlights_cleared {
                    self.editor_needs_layout = true;
                    self.editor_caret_needs_layout = false;
                }

                let report = {
                    let (app_state, clipboard) = (&mut self.app_state, &mut self.clipboard);
                    dispatch_command_with_clipboard_count(
                        app_state,
                        command.clone(),
                        repeat_count,
                        Some(clipboard),
                    )
                };
                if !report.success {
                    if highlights_cleared {
                        self.request_redraw();
                    }
                    return Some(report.request_redraw || highlights_cleared);
                }
                if report.state_changed {
                    self.reconcile_highlight_spans_with_pending_edits();
                    self.editor_needs_layout = true;
                    self.editor_caret_needs_layout = true;
                    let viewport_lines = self.editor_viewport_lines();
                    self.app_state.auto_scroll_to_cursor(viewport_lines);
                    self.queue_lsp_did_change_for_active_file();
                    self.pending_parse_after_debounce = true;
                    self.last_parse_submit_at = Some(Instant::now());
                    if self.ai_inline_suggestion_retained {
                        // Typing consumed the head of the visible ghost text;
                        // follow the caret with the retained remainder instead
                        // of letting the anchor watchdog clear it.
                        self.reanchor_ai_inline();
                    } else {
                        self.queue_ai_inline_completion();
                    }
                }
                let completion_changed = if report.state_changed {
                    self.refresh_open_completion_after_text_edit()
                } else {
                    false
                };
                if report.state_changed {
                    self.apply_typed_char_completion_policy(command);
                }
                if report.request_redraw
                    || report.state_changed
                    || completion_changed
                    || highlights_cleared
                {
                    self.request_redraw();
                }
                Some(
                    report.request_redraw
                        || report.state_changed
                        || completion_changed
                        || highlights_cleared,
                )
            }
            _ => None,
        }
    }

    pub(super) fn handle_viewport_navigation_command(&mut self, command: &Command) -> Option<bool> {
        match command {
            Command::ScrollHalfPageUp | Command::ScrollHalfPageDown
                if self.app_state.has_scrollable_floating_overlay() =>
            {
                let changed = self.app_state.scroll_floating_overlay_half_page(matches!(
                    command,
                    Command::ScrollHalfPageDown
                ));
                if changed {
                    self.editor_caret_needs_layout = true;
                    self.request_redraw();
                }
                Some(changed)
            }
            Command::ScrollHalfPageUp
            | Command::ScrollHalfPageDown
            | Command::CenterCursorLine
            | Command::ScrollCursorLineTop
            | Command::ScrollCursorLineBottom
            | Command::MoveToFirstLine
            | Command::MoveToLastLine => {
                let viewport_lines = self.editor_viewport_lines();
                let prev_scroll = self.app_state.target_scroll_y;
                match command {
                    Command::ScrollHalfPageUp => {
                        self.app_state
                            .scroll_half_page_up((viewport_lines / 2).max(1));
                    }
                    Command::ScrollHalfPageDown => {
                        self.app_state
                            .scroll_half_page_down((viewport_lines / 2).max(1));
                    }
                    Command::CenterCursorLine => {
                        self.app_state.center_cursor_line_animated(viewport_lines);
                    }
                    Command::ScrollCursorLineTop => {
                        let (cursor_line, _) = self.app_state.cursor_line_col();
                        self.app_state.set_target_scroll_line(cursor_line);
                    }
                    Command::ScrollCursorLineBottom => {
                        let (cursor_line, _) = self.app_state.cursor_line_col();
                        self.app_state.set_target_scroll_line(
                            cursor_line.saturating_sub(viewport_lines.saturating_sub(1)),
                        );
                    }
                    Command::MoveToFirstLine => {
                        self.app_state.move_to_first_line();
                        self.app_state.center_cursor_line_animated(viewport_lines);
                    }
                    Command::MoveToLastLine => {
                        self.app_state.move_to_last_line();
                        let (cursor_line, _) = self.app_state.cursor_line_col();
                        let margin = 3usize;
                        let target_line = if cursor_line + margin + 1 >= viewport_lines {
                            cursor_line + margin + 1 - viewport_lines
                        } else {
                            0
                        };
                        self.app_state.set_target_scroll_line(target_line);
                    }
                    _ => {}
                }
                // Explicit scroll command: animate even when the delta is below the
                // snap threshold (consumed one-shot in `advance_scroll_anim`).
                self.scroll_anim_force = true;
                self.editor_needs_layout = true;
                self.editor_caret_needs_layout = false;
                // Large buffers use viewport-scoped tree-sitter highlights, so viewport jumps
                // need a refresh for the newly visible byte window. Debounce it to avoid cloning
                // and scheduling the whole document on every repeated Ctrl-U/Ctrl-D key event.
                if (self.app_state.target_scroll_y - prev_scroll).abs() > f32::EPSILON {
                    self.submit_parse_for_active_buffer(false);
                }
                Some(true)
            }
            _ => None,
        }
    }

    /// Re-run the cursor-dependent reactions a normal motion triggers (LSP symbol
    /// highlight + document-symbol breadcrumb refresh) for a *jump* that bypasses
    /// the per-motion `is_cursor_move` path — Leap, the Ctrl-O/Ctrl-I jump list,
    /// etc. Without this, the symbol highlight / breadcrumb / canvas context stay
    /// stale after a jump until the next `hjkl` nudge re-triggers them.
    /// `ensure_document_symbol_breadcrumbs(false)` refetches automatically when the
    /// jump crossed into a different file (cache-path mismatch) and is a cheap
    /// no-op within the same file.
    pub(crate) fn react_to_cursor_jump(&mut self) {
        self.submit_lsp_document_highlight();
        self.ensure_document_symbol_breadcrumbs(false);
    }

    pub(super) fn handle_leap_command(&mut self, command: &Command) -> Option<bool> {
        match command {
            Command::LeapStart => {
                self.input_handler.set_pending_leap_char();
                self.editor_caret_needs_layout = true;
                Some(true)
            }
            Command::LeapActivate(target_char) => {
                let leap_state = self.generate_editor_leap_state(*target_char);
                if leap_state.targets.is_empty() {
                    self.leap_state = None;
                    Some(false)
                } else {
                    self.input_handler.set_pending_leap_label();
                    self.leap_state = Some(leap_state);
                    self.editor_needs_layout = true;
                    self.editor_caret_needs_layout = false;
                    Some(true)
                }
            }
            Command::LeapJump(label_char) => {
                let Some(mut leap_state) = self.leap_state.take() else {
                    return Some(false);
                };

                leap_state
                    .typed_prefix
                    .push(Self::normalize_leap_target(*label_char));

                let matching_targets: Vec<LeapTarget> = leap_state
                    .targets
                    .iter()
                    .filter(|target| target.label.starts_with(&leap_state.typed_prefix))
                    .cloned()
                    .collect();

                if matching_targets.is_empty() {
                    self.editor_needs_layout = true;
                    self.editor_caret_needs_layout = false;
                    return Some(true);
                }

                let resolved_target = matching_targets
                    .iter()
                    .find(|target| target.label == leap_state.typed_prefix)
                    .or_else(|| (matching_targets.len() == 1).then_some(&matching_targets[0]))
                    .cloned();

                if let Some(target) = resolved_target {
                    // Leap là một jump -> ghi origin để Ctrl+O quay về.
                    self.app_state.push_jump();
                    let changed = self.app_state.leap_jump_to_char(target.char_idx);
                    let viewport_lines = self.editor_viewport_lines();
                    let prev_scroll = self.app_state.target_scroll_y;
                    self.app_state.auto_scroll_to_cursor(viewport_lines);
                    if (self.app_state.target_scroll_y - prev_scroll).abs() > f32::EPSILON {
                        self.submit_parse_for_active_buffer(true);
                    }
                    // A leap is a jump, so re-run the same cursor-dependent
                    // reactions a normal motion does (highlight + breadcrumb),
                    // not just the document highlight.
                    self.react_to_cursor_jump();
                    self.editor_needs_layout = true;
                    self.editor_caret_needs_layout = false;
                    Some(
                        changed
                            || (self.app_state.target_scroll_y - prev_scroll).abs() > f32::EPSILON,
                    )
                } else {
                    leap_state.targets = matching_targets;
                    self.input_handler.set_pending_leap_label();
                    self.leap_state = Some(leap_state);
                    self.editor_needs_layout = true;
                    self.editor_caret_needs_layout = false;
                    Some(true)
                }
            }
            Command::LeapCancel => {
                let had_leap_state = self.leap_state.take().is_some();
                if had_leap_state {
                    self.editor_needs_layout = true;
                    self.editor_caret_needs_layout = false;
                }
                Some(had_leap_state)
            }
            _ => None,
        }
    }

    pub(super) fn handle_generic_editor_command(
        &mut self,
        command: Command,
        repeat_count: usize,
    ) -> bool {
        let is_save_command = matches!(&command, Command::SaveFile);
        let is_open_file_command = matches!(&command, Command::OpenFile(_));
        let should_notify_did_open = matches!(
            &command,
            Command::BufferNext
                | Command::BufferPrev
                | Command::BufferCloseCurrent
                | Command::BufferGoto(_)
        );
        let mut parsed_after_buffer_switch = false;

        let is_cursor_move = matches!(
            &command,
            Command::MoveLeft
                | Command::MoveRight
                | Command::MoveUp
                | Command::MoveDown
                | Command::MoveWordForward
                | Command::MoveWordBackward
                | Command::MoveWordEnd
                | Command::MoveToLineStart
                | Command::MoveToLineEnd
                | Command::MoveToFirstNonWhitespace
                | Command::MoveParagraphUp
                | Command::MoveParagraphDown
                | Command::MatchBracket
                | Command::InsertAtLineStart
                | Command::AppendAtLineEnd
                | Command::AppendAfterCursor
                | Command::EnterVisualLine
                | Command::SearchNext
                | Command::SearchPrev
                | Command::SearchWordUnderCursor
        );
        let should_reparse = matches!(
            &command,
            Command::InsertChar(_)
                | Command::InsertText(_)
                | Command::Newline
                | Command::Backspace
                | Command::InsertLineBelow
                | Command::InsertLineAbove
                | Command::SubstituteLine
                | Command::DeleteChar
                | Command::DeleteSelection
                | Command::DeleteCurrentLine
                | Command::ToggleLineComment
                | Command::ToggleSelectionComment
                | Command::WrapSelectionWithStar
                | Command::DeleteWordForward
                | Command::DeleteWordBackward
                | Command::Operate {
                    op: crate::core::commands::Operator::Delete
                        | crate::core::commands::Operator::Change,
                    ..
                }
                | Command::ChangeSelection
                | Command::ChangeWordForward
                | Command::ChangeWordBackward
                | Command::MultiCursorChange
                | Command::MultiCursorDelete
                | Command::DeleteToLineEnd
                | Command::ChangeToLineEnd
                | Command::PasteAfter
                | Command::PasteBefore
                | Command::EditorPaste
                | Command::PasteSystemClipboard
                | Command::VisualPaste
                | Command::Undo
                | Command::Redo
                | Command::ReplaceChar(_)
                | Command::BufferNew
                | Command::BufferNext
                | Command::BufferPrev
                | Command::BufferCloseCurrent
        );
        let is_typing_edit = matches!(
            &command,
            Command::InsertChar(_) | Command::InsertText(_) | Command::Backspace
        );
        if is_typing_edit {
            let _ = self.app_state.clear_completion();
            self.semantic_highlight_request_revision =
                self.semantic_highlight_request_revision.saturating_add(1);
            if self.app_state.clear_semantic_symbol_highlights() {
                self.editor_caret_needs_layout = true;
                self.request_redraw();
            }
        }
        let prev_dirty = self.app_state.is_dirty();
        // `gi` can land far off-screen; center the viewport on it like other
        // jumps instead of leaving the caret invisible.
        let is_gi_jump = matches!(&command, Command::InsertAtLastEdit);
        let report = if matches!(command, Command::MoveUp | Command::MoveDown)
            && self.try_move_soft_wrap_visual_line(matches!(command, Command::MoveDown))
        {
            crate::core::command_dispatch::DispatchReport {
                message: "Dispatch: applied to active buffer (move visual wrap line)".to_string(),
                request_redraw: true,
                success: true,
                state_changed: true,
            }
        } else {
            let (app_state, clipboard) = (&mut self.app_state, &mut self.clipboard);
            dispatch_command_with_clipboard_count(app_state, command, repeat_count, Some(clipboard))
        };
        // Surface hard failures for commands whose silent no-op would confuse
        // the user: save errors and file-open refusals (10 MiB limit, missing
        // file…). Everything else keeps its quiet dispatch semantics.
        let surface_failure = is_save_command || is_open_file_command;
        let save_failure_toast = if surface_failure && !report.success {
            self.show_transient_toast_kind(report.message.clone(), ToastKind::Error);
            true
        } else {
            false
        };
        self.reconcile_highlight_spans_with_pending_edits();
        if report.success && should_notify_did_open {
            self.close_completion_popup();
            self.invalidate_highlights_and_parse_active_buffer();
            parsed_after_buffer_switch = true;
            self.mark_explorer_dirty();
            let _ = self.sync_focus_mode_for_active_buffer();
        }
        if self.app_state.is_dirty() != prev_dirty {
            self.mark_explorer_dirty();
        }

        if report.state_changed && is_cursor_move {
            self.refresh_open_completion_after_text_edit();
            let prev_scroll = self.app_state.target_scroll_y;
            let viewport_lines = self.editor_viewport_lines();
            self.app_state.auto_scroll_to_cursor(viewport_lines);
            if (self.app_state.target_scroll_y - prev_scroll).abs() > f32::EPSILON {
                self.editor_needs_layout = true;
                self.editor_caret_needs_layout = false;
                self.submit_parse_for_active_buffer(true);
            } else {
                self.editor_caret_needs_layout = true;
            }
            self.submit_lsp_document_highlight();
            self.ensure_document_symbol_breadcrumbs(false);
        } else if report.state_changed {
            self.refresh_open_completion_after_text_edit();
            self.editor_needs_layout = true;
            self.editor_caret_needs_layout = false;
        }
        if is_gi_jump && report.state_changed {
            let viewport_lines = self.editor_viewport_lines();
            self.app_state.center_cursor_line_animated(viewport_lines);
            self.scroll_anim_force = true;
            self.submit_parse_for_active_buffer(false);
            self.react_to_cursor_jump();
        }
        if report.state_changed && should_reparse {
            self.schedule_active_buffer_git_diff_recalculation(!is_typing_edit);
            if !parsed_after_buffer_switch {
                self.submit_parse_for_active_buffer(!is_typing_edit);
            }
            if is_typing_edit {
                self.submit_lsp_did_change_for_active_file();
            } else {
                self.force_flush_lsp_did_change_for_active_file();
                self.ensure_document_symbol_breadcrumbs(true);
            }
            self.semantic_highlight_request_revision =
                self.semantic_highlight_request_revision.saturating_add(1);
            if self.app_state.clear_semantic_symbol_highlights() {
                self.editor_caret_needs_layout = true;
                self.request_redraw();
            }
        }
        if report.success && should_notify_did_open {
            self.submit_lsp_did_open_for_active_file();
            self.submit_lsp_document_highlight();
            self.ensure_document_symbol_breadcrumbs(true);
        }

        report.request_redraw || save_failure_toast
    }

    pub(super) fn normalize_leap_target(target: char) -> char {
        if target.is_ascii_alphabetic() {
            target.to_ascii_lowercase()
        } else {
            target
        }
    }

    pub(super) fn generate_editor_leap_state(&self, target: char) -> LeapState {
        let viewport_lines = self.editor_viewport_lines().max(1);
        let scroll_line = self.app_state.scroll_line();
        let total_chars = self.app_state.text_len_chars();

        let viewport_start_char = self.app_state.char_idx_for_line(scroll_line);
        let viewport_end_char = self
            .app_state
            .char_idx_for_line(scroll_line + viewport_lines)
            .min(total_chars);

        if viewport_start_char >= viewport_end_char {
            return LeapState::default();
        }

        let text = self.app_state.text_string();
        let target_lower = Self::normalize_leap_target(target);
        let mut matches = Vec::new();

        let mut char_idx: usize = 0;
        for ch in text.chars() {
            if char_idx >= viewport_start_char && char_idx < viewport_end_char {
                let ch_lower = if ch.is_ascii_alphabetic() {
                    ch.to_ascii_lowercase()
                } else {
                    ch
                };
                if ch_lower == target_lower {
                    matches.push(char_idx);
                }
            }
            char_idx += 1;
            if char_idx >= viewport_end_char {
                break;
            }
        }

        let targets = generate_leap_labels(matches.len())
            .into_iter()
            .zip(matches)
            .map(|(label, char_idx)| LeapTarget { label, char_idx })
            .collect();
        LeapState::new(targets)
    }
}
