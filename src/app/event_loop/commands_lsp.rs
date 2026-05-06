use super::*;

impl AppShell {
    pub(super) fn handle_lsp_and_diagnostics_command(&mut self, command: &Command) -> Option<bool> {
        match command {
            Command::GitOpenLazygit => Some(self.open_lazygit_buffer()),
            Command::GitOpenLazydocker => Some(self.open_lazydocker_buffer()),
            Command::GitBlameLine => Some(self.submit_git_blame_line()),
            Command::LspHover => Some(self.submit_lsp_hover()),
            Command::LspGoToDefinition => Some(self.submit_lsp_definition(true)),
            Command::LspPreviewDefinition => Some(self.submit_lsp_definition(false)),
            Command::LspReferences => Some(self.submit_lsp_references()),
            Command::LspFormatDocument => Some(self.submit_lsp_format_document()),
            Command::TriggerCompletion => Some(self.submit_lsp_completion()),
            Command::CodeAction => Some(self.submit_lsp_code_action()),
            Command::CompletionNext => Some(self.select_next_completion_item()),
            Command::CompletionPrev => Some(self.select_prev_completion_item()),
            Command::CompletionAccept => Some(self.accept_completion_item()),
            Command::CompletionClose => Some(self.close_completion_popup()),
            Command::AiAcceptInline => {
                let report = dispatch_command(&mut self.app_state, command.clone());
                if report.state_changed {
                    self.reconcile_highlight_spans_with_pending_edits();
                    self.editor_needs_layout = true;
                    self.editor_caret_needs_layout = true;
                    let viewport_lines = self.editor_viewport_lines();
                    self.app_state.auto_scroll_to_cursor(viewport_lines);
                    self.queue_lsp_did_change_for_active_file();
                    self.submit_parse_for_active_buffer(true);
                }
                Some(report.request_redraw || report.state_changed)
            }
            Command::DiagnosticsOpenPicker => Some(self.open_diagnostics_picker()),
            Command::ReferencesSelectNext => Some(self.select_next_reference_item()),
            Command::ReferencesSelectPrev => Some(self.select_prev_reference_item()),
            Command::ReferencesOpenSelection => Some(self.open_selected_reference_item()),
            Command::DiagnosticsSelectNext => Some(self.select_next_diagnostic_item()),
            Command::DiagnosticsSelectPrev => Some(self.select_prev_diagnostic_item()),
            Command::DiagnosticsOpenSelection => Some(self.open_selected_diagnostic_item()),
            Command::JumpBack => Some(self.execute_jump_back()),
            Command::JumpForward => Some(self.execute_jump_forward()),
            _ => None,
        }
    }

    fn lsp_install_working_dir(&self) -> Option<PathBuf> {
        self.app_state
            .workspace_root_path()
            .map(PathBuf::from)
            .or_else(|| {
                self.app_state
                    .active_file()
                    .and_then(|path| path.parent())
                    .map(PathBuf::from)
            })
            .or_else(|| std::env::current_dir().ok())
    }

    pub(in crate::app::event_loop) fn accept_lsp_install_guide(&mut self) -> bool {
        let Some(guide) = self.active_lsp_guide.take() else {
            return false;
        };
        let LspInstallGuide {
            binary,
            install_cmd,
        } = guide;

        if let Some(renderer) = self.renderer.as_mut() {
            renderer.clear_lsp_guide_popup();
        }

        let mut changed = true;
        if let Some(session_id) = self.pty_session_id {
            changed |= self.handle_command(Command::FocusTerminal);
            self.forward_to_terminal_session(session_id, &format!("{install_cmd}\r"));
        } else {
            self.submit(RequestSpec {
                revision_id: 0,
                topic: RequestTopic::TerminalPty,
                payload: WorkerRequestPayload::SpawnDetachedShellCommand {
                    command: install_cmd,
                    working_dir: self.lsp_install_working_dir(),
                },
            });
            self.show_transient_toast(format!("Installing {binary} in background..."));
        }

        changed
    }

    pub(super) fn open_lazygit_buffer(&mut self) -> bool {
        let Some(workspace_root) = self.app_state.workspace_root_path().map(PathBuf::from) else {
            eprintln!("[AppShell] lazygit open skipped: workspace is not attached");
            return false;
        };

        let buffer_index = self
            .app_state
            .open_terminal_buffer("[Lazygit]", Some(workspace_root.clone()));
        self.pending_lazygit_buffer_index = Some(buffer_index);
        self.buffer_terminal_needs_layout = true;
        self.editor_needs_layout = true;
        self.editor_caret_needs_layout = false;
        self.clear_highlight_layers();
        let _ = self.sync_focus_mode_for_active_buffer();

        self.submit(RequestSpec {
            revision_id: 0,
            topic: RequestTopic::TerminalPty,
            payload: WorkerRequestPayload::SpawnPtyCommand {
                program: "lazygit".to_string(),
                args: Vec::new(),
                working_dir: Some(workspace_root),
            },
        });

        true
    }

    pub(super) fn open_lazydocker_buffer(&mut self) -> bool {
        let Some(workspace_root) = self.app_state.workspace_root_path().map(PathBuf::from) else {
            eprintln!("[AppShell] lazydocker open skipped: workspace is not attached");
            return false;
        };

        let buffer_index = self
            .app_state
            .open_terminal_buffer("[Lazydocker]", Some(workspace_root.clone()));
        self.pending_lazydocker_buffer_index = Some(buffer_index);
        self.buffer_terminal_needs_layout = true;
        self.editor_needs_layout = true;
        self.editor_caret_needs_layout = false;
        self.clear_highlight_layers();
        let _ = self.sync_focus_mode_for_active_buffer();

        self.submit(RequestSpec {
            revision_id: 0,
            topic: RequestTopic::TerminalPty,
            payload: WorkerRequestPayload::SpawnPtyCommand {
                program: "lazydocker".to_string(),
                args: Vec::new(),
                working_dir: Some(workspace_root),
            },
        });

        true
    }

    pub(super) fn submit_git_blame_line(&mut self) -> bool {
        if self.app_state.active_buffer_is_terminal() {
            return false;
        }
        let Some(workspace_root) = self.app_state.workspace_root_path().map(PathBuf::from) else {
            return false;
        };
        let Some(file_path) = self.app_state.active_file().map(PathBuf::from) else {
            return false;
        };

        self.git_overlay_revision = self.git_overlay_revision.saturating_add(1);
        let line_number = self.app_state.cursor_line_col().0 + 1;
        self.submit(RequestSpec {
            revision_id: self.git_overlay_revision,
            topic: RequestTopic::Git,
            payload: WorkerRequestPayload::GitBlameLine {
                workspace_root,
                file_path,
                line_number,
            },
        });
        false
    }

    pub(super) fn lsp_cursor_context(&self) -> Option<(String, String, u32, u32)> {
        if self.app_state.active_buffer_is_terminal() {
            return None;
        }
        let buffer = self.app_state.active_text_buffer()?;
        let language_id = buffer.language_id.clone()?;
        let uri = crate::lsp::client::path_to_lsp_uri(&buffer.path);
        let (line, col) = self.app_state.cursor_line_col();
        Some((language_id, uri, line as u32, col as u32))
    }

    pub(super) fn submit_lsp_hover(&mut self) -> bool {
        self.force_flush_lsp_did_change_for_active_file();
        let Some((language_id, uri, line, character)) = self.lsp_cursor_context() else {
            return false;
        };
        let changed = self.app_state.clear_current_overlays();
        self.submit(RequestSpec {
            revision_id: 0,
            topic: RequestTopic::LspRequest,
            payload: WorkerRequestPayload::LspHoverRequest {
                language_id,
                uri,
                line,
                character,
            },
        });
        changed
    }

    pub(super) fn submit_lsp_definition(&mut self, jump: bool) -> bool {
        self.force_flush_lsp_did_change_for_active_file();
        let Some((_language_id, uri, line, character)) = self.lsp_cursor_context() else {
            return false;
        };
        let changed = self.app_state.clear_current_overlays();
        self.submit(RequestSpec {
            revision_id: 0,
            topic: RequestTopic::LspRequest,
            payload: WorkerRequestPayload::LspDefinitionRequest {
                uri,
                line,
                character,
                jump,
            },
        });
        changed
    }

    pub(super) fn submit_lsp_references(&mut self) -> bool {
        self.force_flush_lsp_did_change_for_active_file();
        let Some((_language_id, uri, line, character)) = self.lsp_cursor_context() else {
            return false;
        };
        let mut changed = self.app_state.clear_current_overlays();
        let origin_path = self.app_state.active_file().map(PathBuf::from);
        let origin_line = self.app_state.cursor_line_col().0;
        self.references_request_revision = self.references_request_revision.saturating_add(1);
        let Some(request) = self.submit(RequestSpec {
            revision_id: self.references_request_revision,
            topic: RequestTopic::LspRequest,
            payload: WorkerRequestPayload::LspReferencesRequest {
                uri,
                line,
                character,
            },
        }) else {
            return changed;
        };

        self.app_state.open_pending_references_buffer(
            "References",
            origin_path,
            origin_line,
            request.request_id,
        );
        self.editor_needs_layout = true;
        self.editor_caret_needs_layout = false;
        let focus_changed = self.focus_manager.set(FocusTarget::CenterEditor);
        changed = true;
        if focus_changed {
            self.input_handler.clear_pending_prefix();
        }
        self.request_redraw();
        changed || focus_changed
    }

    pub(super) fn submit_lsp_document_symbols(&mut self) -> bool {
        self.force_flush_lsp_did_change_for_active_file();
        let Some((language_id, uri, _line, _character)) = self.lsp_cursor_context() else {
            let changed = self.app_state.finish_document_symbol_picker_loading();
            if changed {
                self.request_redraw();
            }
            return changed;
        };

        self.document_symbols_request_revision =
            self.document_symbols_request_revision.saturating_add(1);
        self.submit(RequestSpec {
            revision_id: self.document_symbols_request_revision,
            topic: RequestTopic::LspRequest,
            payload: WorkerRequestPayload::LspDocumentSymbolsRequest { language_id, uri },
        });
        true
    }

    pub(super) fn submit_lsp_format_document(&mut self) -> bool {
        self.force_flush_lsp_did_change_for_active_file();
        let Some((language_id, uri, _line, _character)) = self.lsp_cursor_context() else {
            return false;
        };
        let indent = self.app_state.indent_config();
        let changed = self.app_state.clear_current_overlays();
        self.submit(RequestSpec {
            revision_id: self.app_state.revision(),
            topic: RequestTopic::LspRequest,
            payload: WorkerRequestPayload::LspFormattingRequest {
                language_id,
                uri,
                tab_size: indent.tab_width as u32,
                insert_spaces: indent.insert_spaces,
            },
        });
        changed
    }

    pub(super) fn submit_lsp_code_action(&mut self) -> bool {
        self.force_flush_lsp_did_change_for_active_file();
        let Some((_language_id, uri, line, character)) = self.lsp_cursor_context() else {
            eprintln!("[CodeAction] skipped: no cursor context (terminal buffer or no active file)");
            self.show_transient_toast("Code Action: no active file".to_string());
            return false;
        };
        let diagnostics: Vec<crate::async_runtime::message::LspDiagnostic> = self
            .app_state
            .active_file()
            .and_then(|path| self.app_state.diagnostics_for_path(path))
            .map(|items| items.to_vec())
            .unwrap_or_default();
        eprintln!(
            "[CodeAction] submitting request uri={} line={} character={} diagnostics={}",
            uri, line, character, diagnostics.len()
        );
        self.show_transient_toast(format!(
            "Code Action: requesting... ({} diagnostics at cursor)",
            diagnostics.len()
        ));
        self.submit(RequestSpec {
            revision_id: 0,
            topic: RequestTopic::LspRequest,
            payload: WorkerRequestPayload::LspCodeActionRequest {
                uri,
                line,
                character,
                diagnostics,
            },
        });
        false
    }

    pub(super) fn select_next_reference_item(&mut self) -> bool {
        let changed = self.app_state.references_select_next();
        if changed {
            self.submit_references_preview_load();
            self.editor_needs_layout = true;
            self.editor_caret_needs_layout = false;
        }
        changed
    }

    pub(super) fn select_prev_reference_item(&mut self) -> bool {
        let changed = self.app_state.references_select_prev();
        if changed {
            self.submit_references_preview_load();
            self.editor_needs_layout = true;
            self.editor_caret_needs_layout = false;
        }
        changed
    }

    pub(super) fn open_selected_reference_item(&mut self) -> bool {
        let Some(item) = self.app_state.selected_reference_item_cloned() else {
            return false;
        };

        let closed = self.close_current_buffer_now();

        if let Some((origin_path, origin_line)) = self.app_state.active_references_origin() {
            self.app_state.push_jump_entry(origin_path, origin_line);
        }

        if let Err(err) = self.app_state.open_file(item.path.clone()) {
            eprintln!("[AppShell] references open_file failed: {err}");
            return false;
        }

        self.app_state
            .jump_to_line_and_column(item.line, item.column);
        let vp = self.editor_viewport_lines();
        self.app_state.auto_scroll_to_cursor(vp);
        self.invalidate_highlights_and_parse_active_buffer();
        self.submit_lsp_check_for_path(item.path.clone());
        self.submit_lsp_did_open_for_active_file();
        self.editor_needs_layout = true;
        self.editor_caret_needs_layout = false;
        let focus_changed = self.focus_manager.set(FocusTarget::CenterEditor);
        if focus_changed {
            self.input_handler.clear_pending_prefix();
        }
        let _ = closed;
        true
    }

    pub(super) fn open_diagnostics_picker(&mut self) -> bool {
        let mut items = self
            .app_state
            .diagnostics()
            .iter()
            .flat_map(|(path, diagnostics)| {
                diagnostics
                    .iter()
                    .map(|diagnostic| crate::app::app_state::DiagnosticItem {
                        file_path: path.clone(),
                        line: diagnostic.range.start.line as usize,
                        col: diagnostic.range.start.character as usize,
                        message: diagnostic.message.clone(),
                        severity: diagnostic.severity,
                    })
            })
            .collect::<Vec<_>>();

        if items.is_empty() {
            return false;
        }

        items.sort_by(|a, b| {
            a.severity
                .unwrap_or(u32::MAX)
                .cmp(&b.severity.unwrap_or(u32::MAX))
                .then_with(|| a.file_path.cmp(&b.file_path))
                .then_with(|| a.line.cmp(&b.line))
                .then_with(|| a.col.cmp(&b.col))
        });

        if let Err(err) = self.app_state.open_diagnostics_buffer(items) {
            eprintln!("[AppShell] diagnostics open buffer failed: {err}");
            return false;
        }

        self.submit_diagnostics_preview_load();
        self.editor_needs_layout = true;
        self.editor_caret_needs_layout = false;
        let focus_changed = self.focus_manager.set(FocusTarget::CenterEditor);
        if focus_changed {
            self.input_handler.clear_pending_prefix();
        }
        self.request_redraw();
        true
    }

    pub(super) fn select_next_diagnostic_item(&mut self) -> bool {
        let changed = self.app_state.diagnostics_select_next();
        if changed {
            self.submit_diagnostics_preview_load();
            self.editor_needs_layout = true;
            self.editor_caret_needs_layout = false;
        }
        changed
    }

    pub(super) fn select_prev_diagnostic_item(&mut self) -> bool {
        let changed = self.app_state.diagnostics_select_prev();
        if changed {
            self.submit_diagnostics_preview_load();
            self.editor_needs_layout = true;
            self.editor_caret_needs_layout = false;
        }
        changed
    }

    pub(super) fn open_selected_diagnostic_item(&mut self) -> bool {
        let Some(item) = self.app_state.selected_diagnostic_item_cloned() else {
            return false;
        };

        let origin = self
            .app_state
            .active_file()
            .map(PathBuf::from)
            .map(|path| (path, self.app_state.cursor_line_col().0));

        let _ = self.app_state.close_current_buffer();

        if let Some((active_path, active_line)) = origin {
            self.app_state.push_jump_entry(active_path, active_line);
        }

        if let Err(err) = self.app_state.open_file(item.file_path.clone()) {
            eprintln!("[AppShell] diagnostics open_file failed: {err}");
            return false;
        }

        self.app_state.jump_to_line_and_column(item.line, item.col);
        let vp = self.editor_viewport_lines();
        self.app_state.auto_scroll_to_cursor(vp);
        self.invalidate_highlights_and_parse_active_buffer();
        self.submit_lsp_check_for_path(item.file_path.clone());
        self.submit_lsp_did_open_for_active_file();
        self.editor_needs_layout = true;
        self.editor_caret_needs_layout = false;
        let focus_changed = self.focus_manager.set(FocusTarget::CenterEditor);
        if focus_changed {
            self.input_handler.clear_pending_prefix();
        }
        true
    }

    pub(super) fn execute_jump_back(&mut self) -> bool {
        let Some((path, line)) = self.app_state.pop_jump_back() else {
            return false;
        };
        if let Err(err) = self.app_state.open_file(path.clone()) {
            eprintln!("[AppShell] jump_back open_file failed: {err}");
            return false;
        }
        self.app_state.jump_to_line(line);
        let vp = self.editor_viewport_lines();
        self.app_state.auto_scroll_to_cursor(vp);
        self.invalidate_highlights_and_parse_active_buffer();
        self.submit_lsp_check_for_path(path);
        self.submit_lsp_did_open_for_active_file();
        self.editor_needs_layout = true;
        true
    }

    pub(super) fn execute_jump_forward(&mut self) -> bool {
        let Some((path, line)) = self.app_state.pop_jump_forward() else {
            return false;
        };
        if let Err(err) = self.app_state.open_file(path.clone()) {
            eprintln!("[AppShell] jump_forward open_file failed: {err}");
            return false;
        }
        self.app_state.jump_to_line(line);
        let vp = self.editor_viewport_lines();
        self.app_state.auto_scroll_to_cursor(vp);
        self.invalidate_highlights_and_parse_active_buffer();
        self.submit_lsp_check_for_path(path);
        self.submit_lsp_did_open_for_active_file();
        self.editor_needs_layout = true;
        true
    }

    /// Apply edits từ một LspCodeAction đã được resolve.
    pub(crate) fn do_apply_code_action_edits(
        &mut self,
        edits: &[crate::async_runtime::message::LspTextEdit],
        title: &str,
    ) {
        let text = self.app_state.text_string();
        match super::async_results::apply_lsp_text_edits(&text, edits) {
            Ok(next) => {
                if self
                    .app_state
                    .replace_active_document_text_preserve_cursor(&next)
                {
                    self.editor_needs_layout = true;
                    self.editor_caret_needs_layout = true;
                    self.submit_parse_for_active_buffer(true);
                    self.force_flush_lsp_did_change_for_active_file();
                    self.show_transient_toast(format!("Applied: {title}"));
                    self.request_redraw();
                } else {
                    self.show_transient_toast("Code Action: no changes".to_string());
                }
            }
            Err(err) => {
                eprintln!("[CodeAction] apply failed: {err}");
                self.show_transient_toast(format!("Code Action failed: {err}"));
            }
        }
    }

    /// Handle Enter khi user chọn một action trong CodeAction picker.
    pub(super) fn confirm_code_action_selection(&mut self) -> bool {
        let selected_idx = match self.app_state.command_palette_selected_action() {
            Some(crate::app::command_palette::CommandPaletteAction::ApplyCodeAction(idx)) => idx,
            _ => return false,
        };

        // Đóng palette trước.
        let _ = self.app_state.close_command_palette();
        if let Ok(result) = self.app_state.apply_mode_event(crate::core::mode::ModeEvent::ExitFocus) {
            if result.changed {
                self.editor_needs_layout = true;
            }
        }
        self.focus_manager.set(FocusTarget::CenterEditor);
        self.input_handler.clear_pending_prefix();

        let Some(action) = self.pending_code_actions.get(selected_idx).cloned() else {
            self.show_transient_toast("Code Action: selection out of range".to_string());
            return true;
        };

        if action.edits.is_empty() {
            self.show_transient_toast(format!(
                "Code Action '{}' has no edits (needs resolve support)",
                action.title
            ));
            return true;
        }

        let edits = action.edits.clone();
        let title = action.title.clone();
        self.do_apply_code_action_edits(&edits, &title);
        true
    }
}
