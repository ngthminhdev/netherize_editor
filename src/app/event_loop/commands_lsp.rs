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
            Command::LspRename => Some(self.open_lsp_rename_prompt()),
            Command::LspFormatDocument => Some(self.submit_lsp_format_document()),
            Command::TriggerCompletion => Some(self.submit_lsp_completion_manual()),
            Command::CodeAction => Some(self.submit_lsp_code_action()),
            Command::LspSelectPythonEnv => Some(self.handle_lsp_select_python_env()),
            Command::LspSelectDartEnv => Some(self.handle_lsp_select_dart_env()),
            Command::LspRestart => Some(self.handle_lsp_restart()),
            Command::CompletionNext => Some(self.select_next_completion_item()),
            Command::CompletionPrev => Some(self.select_prev_completion_item()),
            Command::CompletionAccept => Some(self.accept_completion_item()),
            Command::CompletionClose => Some(self.cancel_completion_and_return_normal()),
            Command::AiAcceptInline | Command::AiAcceptInlineWord => {
                let report = dispatch_command(&mut self.app_state, command.clone());
                if report.state_changed {
                    self.reconcile_highlight_spans_with_pending_edits();
                    self.editor_needs_layout = true;
                    self.editor_caret_needs_layout = true;
                    let viewport_lines = self.editor_viewport_lines();
                    self.app_state.auto_scroll_to_cursor(viewport_lines);
                    self.queue_lsp_did_change_for_active_file();
                    self.submit_parse_for_active_buffer(true);
                    // A word accept keeps the rest of the suggestion visible at
                    // the caret's new position; re-anchor so the watchdog keeps
                    // it instead of clearing it as a stale ghost.
                    if self.app_state.inline_suggestion().is_some() {
                        self.reanchor_ai_inline();
                    } else {
                        self.ai_inline_anchor = None;
                    }
                }
                Some(report.request_redraw || report.state_changed)
            }
            Command::DiagnosticsOpenPicker => Some(self.open_diagnostics_picker()),
            Command::ReferencesSelectNext => Some(self.select_next_reference_item()),
            Command::ReferencesSelectPrev => Some(self.select_prev_reference_item()),
            Command::ReferencesOpenSelection => Some(self.open_selected_reference_item()),
            Command::CodeGraphOpenGraphHud => Some(self.open_code_graph_hud()),
            Command::CodeGraphNavLeft => {
                Some(self.code_graph_nav(crate::codegraph::navigation::NavKey::Left))
            }
            Command::CodeGraphNavRight => {
                Some(self.code_graph_nav(crate::codegraph::navigation::NavKey::Right))
            }
            Command::CodeGraphNavUp => {
                Some(self.code_graph_nav(crate::codegraph::navigation::NavKey::Up))
            }
            Command::CodeGraphNavDown => {
                Some(self.code_graph_nav(crate::codegraph::navigation::NavKey::Down))
            }
            Command::CodeGraphJump => Some(self.code_graph_jump()),
            Command::CodeGraphClose => Some(self.code_graph_close()),
            Command::DiagnosticsSelectNext => Some(self.select_next_diagnostic_item()),
            Command::DiagnosticsSelectPrev => Some(self.select_prev_diagnostic_item()),
            Command::DiagnosticsOpenSelection => Some(self.open_selected_diagnostic_item()),
            Command::JumpBack => Some(self.execute_jump_back()),
            Command::JumpForward => Some(self.execute_jump_forward()),
            Command::OutlineNext => {
                let current_idx = self
                    .outline_selected
                    .or_else(|| self.outline_cursor_symbol_index());
                let count = self.cached_document_symbols.len();
                if count > 0 {
                    let next_idx = match current_idx {
                        Some(idx) => (idx + 1).min(count - 1),
                        None => 0,
                    };
                    self.outline_selected = Some(next_idx);
                    let symbol = &self.cached_document_symbols[next_idx];
                    let line = symbol.range.start.line as usize;
                    let col = symbol.range.start.character as usize;
                    self.app_state.push_jump();
                    self.app_state.jump_to_line_and_column(line, col);
                    let vp = self.editor_viewport_lines();
                    self.app_state.center_cursor_line(vp);
                    self.editor_needs_layout = true;
                    self.editor_caret_needs_layout = false;
                    // Re-render the sidebar so the moved outline highlight is shown.
                    self.sidebar_needs_layout = true;
                    Some(true)
                } else {
                    Some(false)
                }
            }
            Command::OutlinePrev => {
                let current_idx = self
                    .outline_selected
                    .or_else(|| self.outline_cursor_symbol_index());
                let count = self.cached_document_symbols.len();
                if count > 0 {
                    let prev_idx = match current_idx {
                        Some(idx) => idx.saturating_sub(1),
                        None => 0,
                    };
                    self.outline_selected = Some(prev_idx);
                    let symbol = &self.cached_document_symbols[prev_idx];
                    let line = symbol.range.start.line as usize;
                    let col = symbol.range.start.character as usize;
                    self.app_state.push_jump();
                    self.app_state.jump_to_line_and_column(line, col);
                    let vp = self.editor_viewport_lines();
                    self.app_state.center_cursor_line(vp);
                    self.editor_needs_layout = true;
                    self.editor_caret_needs_layout = false;
                    // Re-render the sidebar so the moved outline highlight is shown.
                    self.sidebar_needs_layout = true;
                    Some(true)
                } else {
                    Some(false)
                }
            }
            Command::OutlineFirst => {
                if let Some(symbol) = self.cached_document_symbols.first() {
                    self.outline_selected = Some(0);
                    let line = symbol.range.start.line as usize;
                    let col = symbol.range.start.character as usize;
                    self.app_state.push_jump();
                    self.app_state.jump_to_line_and_column(line, col);
                    let vp = self.editor_viewport_lines();
                    self.app_state.center_cursor_line(vp);
                    self.editor_needs_layout = true;
                    self.editor_caret_needs_layout = false;
                    self.sidebar_needs_layout = true;
                    Some(true)
                } else {
                    Some(false)
                }
            }
            Command::OutlineLast => {
                if let Some((last_idx, symbol)) = self
                    .cached_document_symbols
                    .len()
                    .checked_sub(1)
                    .and_then(|idx| {
                        self.cached_document_symbols
                            .get(idx)
                            .map(|symbol| (idx, symbol))
                    })
                {
                    self.outline_selected = Some(last_idx);
                    let line = symbol.range.start.line as usize;
                    let col = symbol.range.start.character as usize;
                    self.app_state.push_jump();
                    self.app_state.jump_to_line_and_column(line, col);
                    let vp = self.editor_viewport_lines();
                    self.app_state.center_cursor_line(vp);
                    self.editor_needs_layout = true;
                    self.editor_caret_needs_layout = false;
                    self.sidebar_needs_layout = true;
                    Some(true)
                } else {
                    Some(false)
                }
            }
            Command::OutlineConfirm => {
                let current_idx = self
                    .outline_selected
                    .or_else(|| self.outline_cursor_symbol_index());
                let count = self.cached_document_symbols.len();
                if count > 0 {
                    let idx = current_idx.unwrap_or(0);
                    let symbol = &self.cached_document_symbols[idx];
                    let line = symbol.range.start.line as usize;
                    let col = symbol.range.start.character as usize;
                    self.app_state.push_jump();
                    self.app_state.jump_to_line_and_column(line, col);
                    let vp = self.editor_viewport_lines();
                    self.app_state.center_cursor_line(vp);
                }
                self.focus_manager.set(FocusTarget::CenterEditor);
                let _ = self.release_focus_mode_to_editor();
                self.outline_selected = None;
                self.editor_needs_layout = true;
                self.editor_caret_needs_layout = false;
                self.sidebar_needs_layout = true;
                Some(true)
            }
            _ => None,
        }
    }

    pub(in crate::app::event_loop) fn clear_document_symbol_breadcrumb_cache(&mut self) -> bool {
        let had_cache = self.cached_document_symbols_path.take().is_some()
            || !self.cached_document_symbols.is_empty();
        self.cached_document_symbols.clear();
        self.outline_selected = None;
        had_cache
    }

    pub(in crate::app::event_loop) fn ensure_document_symbol_breadcrumbs(
        &mut self,
        force_refresh: bool,
    ) -> bool {
        let Some(active_path) = self.app_state.active_file().map(PathBuf::from) else {
            return self.clear_document_symbol_breadcrumb_cache();
        };

        if !force_refresh
            && self.cached_document_symbols_path.as_deref() == Some(active_path.as_path())
            && !self.cached_document_symbols.is_empty()
        {
            return false;
        }

        if self.active_lsp_server.is_none() {
            return self.clear_document_symbol_breadcrumb_cache();
        }

        self.force_flush_lsp_did_change_for_active_file();
        let Some((language_id, uri, _line, _character)) = self.lsp_cursor_context() else {
            return self.clear_document_symbol_breadcrumb_cache();
        };

        self.document_symbols_request_revision =
            self.document_symbols_request_revision.saturating_add(1);
        self.submit(RequestSpec {
            revision_id: self.document_symbols_request_revision,
            topic: RequestTopic::LspRequest,
            payload: WorkerRequestPayload::LspDocumentSymbolsRequest { language_id, uri },
        });
        false
    }

    /// Ensure the Outline panel has the active file's document symbols. Fetches
    /// once per file (guarded by `outline_fetch_path`) so it doesn't re-request
    /// every frame the Outline tab is rendered.
    pub(in crate::app::event_loop) fn ensure_outline_symbols(&mut self) {
        let Some(active) = self.app_state.active_file().map(PathBuf::from) else {
            self.outline_fetch_path = None;
            if self.clear_document_symbol_breadcrumb_cache() {
                self.sidebar_needs_layout = true;
            }
            return;
        };
        if self.cached_document_symbols_path.as_deref() != Some(active.as_path())
            && self.clear_document_symbol_breadcrumb_cache()
        {
            self.sidebar_needs_layout = true;
        }
        if self.outline_fetch_path.as_deref() == Some(active.as_path()) {
            return;
        }
        if self.active_lsp_server.is_none() || self.lsp_cursor_context().is_none() {
            self.outline_fetch_path = None;
            return;
        }
        self.outline_selected = None;
        // Reuses the breadcrumb document-symbol pipeline; the result lands in
        // `cached_document_symbols` (shared) and triggers a redraw.
        let _ = self.ensure_document_symbol_breadcrumbs(true);
        self.outline_fetch_path = Some(active);
    }

    /// Index of the cached document symbol whose range contains the cursor line
    /// (deepest match wins), for the Outline "you are here" highlight.
    pub(in crate::app::event_loop) fn outline_cursor_symbol_index(&self) -> Option<usize> {
        if self.cached_document_symbols.is_empty() {
            return None;
        }
        let (line, _) = self.app_state.cursor_line_col();
        let line = line as u32;
        let mut best: Option<usize> = None;
        let mut best_depth = 0usize;
        for (i, sym) in self.cached_document_symbols.iter().enumerate() {
            if sym.range.start.line <= line && line <= sym.range.end.line {
                let depth = sym.ancestors.len();
                if best.is_none() || depth >= best_depth {
                    best = Some(i);
                    best_depth = depth;
                }
            }
        }
        best
    }

    pub(in crate::app::event_loop) fn accept_lsp_install_guide(&mut self) -> bool {
        let Some(guide) = self.active_lsp_guide.take() else {
            return false;
        };
        let LspInstallGuide { binary, .. } = guide;

        if let Some(renderer) = self.renderer.as_mut() {
            renderer.clear_lsp_guide_popup();
        }

        let _ = self.app_state.open_extensions_manager_buffer();
        let _ = self.sync_focus_mode_for_active_buffer();
        self.editor_needs_layout = true;
        self.editor_caret_needs_layout = false;
        self.show_transient_toast(format!(
            "Open Extensions Manager\nSelect {binary} and press i to install with live logs."
        ));
        true
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
        if self.active_lsp_server.is_none() {
            if self.pending_lsp_server.is_some() {
                self.show_transient_toast("LSP is starting up, please wait…".to_string());
            }
            return false;
        }
        self.force_flush_lsp_did_change_for_active_file();
        let Some((language_id, uri, line, character)) = self.lsp_cursor_context() else {
            return false;
        };
        let (anchor_line, anchor_col) = self.app_state.cursor_line_col();
        // Show a loading overlay immediately so the user sees feedback right away.
        let loading_block =
            crate::app::app_state::FloatingBoxBlock::Prose("⟳  Loading documentation…".to_string());
        self.app_state.set_current_overlays(vec![
            crate::app::app_state::EditorOverlay::FloatingBox {
                anchor_line,
                anchor_col,
                blocks: vec![loading_block],
                style: crate::app::app_state::FloatingBoxStyle::DocHover,
                scroll: crate::app::app_state::FloatingBoxScrollState { offset_lines: 0 },
            },
        ]);
        let request = self.submit(RequestSpec {
            revision_id: 0,
            topic: RequestTopic::LspRequest,
            payload: WorkerRequestPayload::LspHoverRequest {
                language_id,
                uri,
                line,
                character,
                for_completion: false,
                completion_revision: None,
            },
        });
        if let Some(req) = request {
            self.hover_loading_request_id = Some(req.request_id);
            self.latest_hover_request_id = Some(req.request_id);
        }
        true
    }

    pub(super) fn submit_lsp_definition(&mut self, jump: bool) -> bool {
        self.force_flush_lsp_did_change_for_active_file();
        let Some((_language_id, uri, line, character)) = self.lsp_cursor_context() else {
            return false;
        };
        let changed = self.app_state.clear_current_overlays();
        // Track the latest in-flight definition id so a stale response
        // arriving after the user fired a new `gd`/`gD` is dropped on the
        // main thread (the worker also sends `$/cancelRequest` to the LSP
        // server for the previous id; this guard handles the race window
        // where the old response is already on the wire).
        let request = self.submit(RequestSpec {
            revision_id: 0,
            topic: RequestTopic::LspRequest,
            payload: WorkerRequestPayload::LspDefinitionRequest {
                uri,
                line,
                character,
                jump,
            },
        });
        self.latest_definition_request_id = request.map(|r| r.request_id);
        changed
    }

    pub(super) fn submit_lsp_document_highlight(&mut self) -> bool {
        if self.active_lsp_server.is_none() {
            let changed = self.app_state.clear_semantic_symbol_highlights();
            if changed {
                self.editor_caret_needs_layout = true;
                self.request_redraw();
            }
            return changed;
        }
        self.force_flush_lsp_did_change_for_active_file();
        let Some((language_id, uri, line, character)) = self.lsp_cursor_context() else {
            let changed = self.app_state.clear_semantic_symbol_highlights();
            if changed {
                self.editor_caret_needs_layout = true;
                self.request_redraw();
            }
            return changed;
        };

        self.semantic_highlight_request_revision =
            self.semantic_highlight_request_revision.saturating_add(1);
        self.submit(RequestSpec {
            revision_id: self.semantic_highlight_request_revision,
            topic: RequestTopic::LspRequest,
            payload: WorkerRequestPayload::LspDocumentHighlightRequest {
                language_id,
                uri,
                line,
                character,
            },
        });
        false
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

    pub(super) fn open_lsp_rename_prompt(&mut self) -> bool {
        if self.active_lsp_server.is_none() {
            if self.pending_lsp_server.is_some() {
                self.show_transient_toast("LSP is starting up, please wait...".to_string());
            } else {
                self.show_transient_toast("LSP rename: no active language server".to_string());
            }
            return false;
        }

        let report = dispatch_command(&mut self.app_state, Command::LspRename);
        if !report.success {
            self.show_transient_toast("LSP rename: could not open prompt".to_string());
            return report.request_redraw;
        }

        let focus_changed = self.focus_manager.set(FocusTarget::OverlayLayer);
        if focus_changed {
            self.input_handler.clear_pending_prefix();
        }
        self.arm_palette_ime_commit_suppression();
        report.request_redraw || report.state_changed || focus_changed
    }

    pub(super) fn confirm_lsp_rename_prompt(&mut self) -> bool {
        let new_name = self
            .app_state
            .command_palette_query_text()
            .trim()
            .to_string();
        if new_name.is_empty() {
            self.show_transient_toast("LSP rename: enter a new name".to_string());
            return true;
        }

        let _ = self.app_state.close_command_palette();
        if let Ok(result) = self
            .app_state
            .apply_mode_event(crate::core::mode::ModeEvent::ExitFocus)
            && result.changed
        {
            self.editor_needs_layout = true;
        }
        if self.focus_manager.set(FocusTarget::CenterEditor) {
            self.input_handler.clear_pending_prefix();
        }
        self.clear_palette_ime_commit_suppression();

        if !self.submit_lsp_rename(new_name) {
            self.request_redraw();
        }
        true
    }

    pub(super) fn submit_lsp_rename(&mut self, new_name: String) -> bool {
        if self.active_lsp_server.is_none() {
            self.show_transient_toast("LSP rename: no active language server".to_string());
            return false;
        }
        self.force_flush_lsp_did_change_for_active_file();
        let Some((_language_id, uri, line, character)) = self.lsp_cursor_context() else {
            self.show_transient_toast("LSP rename: no active file".to_string());
            return false;
        };

        self.lsp_rename_request_revision = self.lsp_rename_request_revision.saturating_add(1);
        let request = self.submit(RequestSpec {
            revision_id: self.lsp_rename_request_revision,
            topic: RequestTopic::LspRequest,
            payload: WorkerRequestPayload::LspRenameRequest {
                uri,
                line,
                character,
                new_name,
            },
        });
        self.latest_rename_request_id = request.map(|r| r.request_id);
        self.show_transient_toast("Renaming symbol...".to_string());
        false
    }

    pub(super) fn submit_lsp_document_symbols(&mut self) -> bool {
        let changed = self.ensure_document_symbol_breadcrumbs(true);
        let Some((_language_id, _uri, _line, _character)) = self.lsp_cursor_context() else {
            let changed = self.app_state.finish_document_symbol_picker_loading();
            if changed {
                self.request_redraw();
            }
            return changed;
        };
        if changed {
            self.request_redraw();
        }
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
            self.show_transient_toast("Code Action: no active file".to_string());
            return false;
        };
        let diagnostics: Vec<crate::async_runtime::message::LspDiagnostic> = self
            .app_state
            .active_file()
            .and_then(|path| self.app_state.diagnostics_for_path(path))
            .map(|items| {
                items
                    .iter()
                    .filter(|d| {
                        let s = &d.range.start;
                        let e = &d.range.end;
                        // cursor must be within [start, end)
                        (s.line < line || (s.line == line && s.character <= character))
                            && (e.line > line || (e.line == line && e.character >= character))
                    })
                    .cloned()
                    .collect()
            })
            .unwrap_or_default();
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
            self.app_state.push_jump_entry(origin_path, origin_line, 0);
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

    /// `gp`: resolve the symbol enclosing the caret (from the document-symbol
    /// outline) and open the Code Graph HUD in its loading state, submitting the
    /// codegraph query off the UI thread.
    pub(super) fn open_code_graph_hud(&mut self) -> bool {
        let idx = self
            .outline_selected
            .filter(|i| *i < self.cached_document_symbols.len())
            .or_else(|| self.outline_cursor_symbol_index());
        let Some(idx) = idx else {
            self.app_state
                .code_graph_hud
                .open_loading("(cursor)".to_string());
            self.app_state
                .code_graph_hud
                .set_error("No symbol under cursor".to_string());
            self.editor_needs_layout = true;
            self.request_redraw();
            return true;
        };
        let symbol = &self.cached_document_symbols[idx];
        let symbol_name = symbol.name.clone();
        // codegraph startLine is 1-based; LSP range is 0-based.
        let focal_line = symbol.range.start.line + 1;

        let Some(active) = self.app_state.active_file().map(PathBuf::from) else {
            self.app_state
                .code_graph_hud
                .open_loading(symbol_name.clone());
            self.app_state
                .code_graph_hud
                .set_error("No active file".to_string());
            self.editor_needs_layout = true;
            self.request_redraw();
            return true;
        };
        let workspace_root = self
            .app_state
            .workspace_root_path()
            .map(|p| p.to_path_buf())
            .unwrap_or_else(|| active.parent().map(|p| p.to_path_buf()).unwrap_or_default());
        let focal_file = active
            .strip_prefix(&workspace_root)
            .unwrap_or(&active)
            .to_string_lossy()
            .to_string();

        self.app_state
            .code_graph_hud
            .open_loading(symbol_name.clone());

        let _ = self.submit(RequestSpec {
            revision_id: 0,
            topic: RequestTopic::CodeGraph,
            payload: WorkerRequestPayload::CodeGraphQuery {
                symbol: symbol_name,
                focal_file,
                focal_line,
                workspace_root,
            },
        });
        self.editor_needs_layout = true;
        self.request_redraw();
        true
    }

    pub(super) fn code_graph_nav(&mut self, key: crate::codegraph::navigation::NavKey) -> bool {
        let changed = self.app_state.code_graph_hud.nav(key);
        if changed {
            self.refresh_code_graph_detail();
        }
        // The HUD (incl. the focus ring) is rebuilt by update_editor_overlays,
        // which only runs when the editor overlay is marked dirty.
        self.editor_needs_layout = true;
        self.request_redraw();
        true
    }

    /// Load a small code preview around the focused node's definition into the
    /// HUD state (shown as a hover-style detail panel). Reads the file on disk;
    /// cheap enough to run on each navigation.
    pub(in crate::app::event_loop) fn refresh_code_graph_detail(&mut self) {
        use crate::app::app_state::code_graph_hud::NodeDetail;
        use crate::async_runtime::message::FilePreviewLine;
        let Some(node) = self.app_state.code_graph_hud.focused_node().cloned() else {
            self.app_state.code_graph_hud.detail = None;
            return;
        };
        let root = self
            .app_state
            .workspace_root_path()
            .map(|p| p.to_path_buf());
        let path = match root {
            Some(root) => root.join(&node.file_path),
            None => std::path::PathBuf::from(&node.file_path),
        };
        let Ok(content) = std::fs::read_to_string(&path) else {
            self.app_state.code_graph_hud.detail = None;
            return;
        };
        let all: Vec<&str> = content.lines().collect();
        let target = (node.line as usize).saturating_sub(1); // 0-based
        let start = target.saturating_sub(1);
        let end = (target + 6).min(all.len());
        let preview_lines: Vec<FilePreviewLine> = (start..end)
            .map(|i| FilePreviewLine {
                line_number: i + 1,
                text: all[i].to_string(),
                is_target: i == target,
            })
            .collect();
        // Same in-process tree-sitter highlighter the fuzzy/references previews use.
        let (_text, spans) =
            super::helpers::build_preview_render_data(&preview_lines, &path, &self.theme);
        let lines: Vec<String> = preview_lines.into_iter().map(|l| l.text).collect();
        self.app_state.code_graph_hud.detail = Some(NodeDetail {
            name: node.name.clone(),
            file_path: node.file_path.clone(),
            line: node.line,
            start_line: (start + 1) as u32,
            lines,
            spans,
        });
    }

    pub(super) fn code_graph_close(&mut self) -> bool {
        self.app_state.code_graph_hud.close();
        self.editor_needs_layout = true;
        self.request_redraw();
        true
    }

    pub(super) fn code_graph_jump(&mut self) -> bool {
        let Some(node) = self.app_state.code_graph_hud.focused_node().cloned() else {
            return true;
        };
        let target = match self.app_state.workspace_root_path() {
            Some(root) => root.join(&node.file_path),
            None => PathBuf::from(&node.file_path),
        };
        self.app_state.code_graph_hud.close();
        self.app_state.push_jump();
        if let Err(err) = self.app_state.open_file(target.clone()) {
            eprintln!("[AppShell] code graph jump open_file failed: {err}");
            self.request_redraw();
            return true;
        }
        // codegraph startLine is 1-based; the editor expects 0-based.
        let line = (node.line as usize).saturating_sub(1);
        self.app_state.jump_to_line_and_column(line, 0);
        let vp = self.editor_viewport_lines();
        self.app_state.center_cursor_line(vp);
        self.invalidate_highlights_and_parse_active_buffer();
        self.submit_lsp_did_open_for_active_file();
        self.editor_needs_layout = true;
        self.editor_caret_needs_layout = false;
        let focus_changed = self.focus_manager.set(FocusTarget::CenterEditor);
        if focus_changed {
            self.input_handler.clear_pending_prefix();
        }
        self.request_redraw();
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

        let origin = self.app_state.active_file().map(PathBuf::from).map(|path| {
            let (line, col) = self.app_state.cursor_line_col();
            (path, line, col)
        });

        let _ = self.app_state.close_current_buffer();

        if let Some((active_path, active_line, active_col)) = origin {
            self.app_state
                .push_jump_entry(active_path, active_line, active_col);
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
        let Some((path, line, col)) = self.app_state.pop_jump_back() else {
            return false;
        };
        if let Err(err) = self.app_state.open_file(path.clone()) {
            eprintln!("[AppShell] jump_back open_file failed: {err}");
            return false;
        }
        self.app_state.jump_to_line_col(line, col);
        let vp = self.editor_viewport_lines();
        self.app_state.auto_scroll_to_cursor(vp);
        self.invalidate_highlights_and_parse_active_buffer();
        self.submit_lsp_check_for_path(path);
        self.submit_lsp_did_open_for_active_file();
        self.react_to_cursor_jump();
        self.editor_needs_layout = true;
        true
    }

    pub(super) fn execute_jump_forward(&mut self) -> bool {
        let Some((path, line, col)) = self.app_state.pop_jump_forward() else {
            return false;
        };
        if let Err(err) = self.app_state.open_file(path.clone()) {
            eprintln!("[AppShell] jump_forward open_file failed: {err}");
            return false;
        }
        self.app_state.jump_to_line_col(line, col);
        let vp = self.editor_viewport_lines();
        self.app_state.auto_scroll_to_cursor(vp);
        self.invalidate_highlights_and_parse_active_buffer();
        self.submit_lsp_check_for_path(path);
        self.submit_lsp_did_open_for_active_file();
        self.react_to_cursor_jump();
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
                    .replace_active_document_text_preserve_cursor_with_undo(&next)
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
        if let Ok(result) = self
            .app_state
            .apply_mode_event(crate::core::mode::ModeEvent::ExitFocus)
        {
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

    pub(super) fn handle_lsp_select_python_env(&mut self) -> bool {
        let Some(workspace_root) = self
            .app_state
            .workspace_root_path()
            .map(|p| p.to_path_buf())
        else {
            self.show_transient_toast("Python Env: no workspace open".to_string());
            return false;
        };

        self.app_state.open_python_env_selector();
        self.request_redraw();

        self.submit(RequestSpec {
            revision_id: 0,
            topic: RequestTopic::SystemTask,
            payload: WorkerRequestPayload::ScanPythonEnvironments { workspace_root },
        });
        true
    }

    pub(super) fn confirm_python_env_selection(&mut self) -> bool {
        let selected_path = match self.app_state.command_palette_selected_action() {
            Some(CommandPaletteAction::SelectPythonEnv(path)) => path,
            _ => return false,
        };

        let _ = self.app_state.close_command_palette();
        if let Ok(result) = self.app_state.apply_mode_event(ModeEvent::ExitFocus) {
            if result.changed {
                self.editor_needs_layout = true;
            }
        }
        self.focus_manager.set(FocusTarget::CenterEditor);
        self.input_handler.clear_pending_prefix();

        // Derive a short venv display name from the parent directory of the binary.
        // e.g. /project/venv/bin/python → "venv"  or  /project/.venv/bin/python → ".venv"
        let venv_name = selected_path
            .parent() // bin/
            .and_then(|p| p.parent()) // venv/
            .and_then(|p| p.file_name())
            .and_then(|n| n.to_str())
            .map(str::to_string);
        self.runtime_versions.venv_name = venv_name;

        // Store selected env and re-detect versions against the chosen interpreter.
        self.selected_python_env = Some(selected_path.clone());
        self.sync_lsp_server_for_workspace();

        let workspace_root = self
            .app_state
            .workspace_root_path()
            .map(std::path::PathBuf::from)
            .unwrap_or_default();
        self.submit(RequestSpec {
            revision_id: 0,
            topic: RequestTopic::SystemTask,
            payload: WorkerRequestPayload::DetectRuntimeVersions {
                python_binary: Some(selected_path.clone()),
                workspace_root,
            },
        });

        self.show_transient_toast(format!("Python env selected: {}", selected_path.display()));
        true
    }

    /// Restart the language server backing the active file. The app picks the
    /// server from whichever file is focused, so this also re-applies the
    /// current Python interpreter / Dart SDK selection (the fresh spawn re-sends
    /// `workspace/didChangeConfiguration`).
    pub(super) fn handle_lsp_restart(&mut self) -> bool {
        let Some(desired) = self.desired_lsp_server_for_active_file() else {
            self.show_transient_toast("Restart LSP: no language server for this file".to_string());
            return false;
        };

        // Drop the tracking so `queue_lsp_server_start`'s dedupe doesn't
        // short-circuit the respawn, then shut down every running session
        // (the primary plus any companion like ruff). LSP requests run
        // concurrently in the worker, so spawning the new server now would
        // race the shutdown's drain — instead we arm `pending_lsp_restart`
        // and spawn once the `LspServerStopped` result lands.
        self.active_lsp_server = None;
        self.pending_lsp_server = None;
        self.pending_lsp_document_sync = None;
        self.lsp_completion_trigger_chars.clear();
        self.pending_lsp_restart = true;

        self.submit(RequestSpec {
            revision_id: 0,
            topic: RequestTopic::LspClient,
            payload: WorkerRequestPayload::ShutdownAllLspServers,
        });

        self.show_transient_toast(format!(
            "Restarting language server: {}",
            desired.server_name
        ));
        true
    }

    pub(super) fn handle_lsp_select_dart_env(&mut self) -> bool {
        let Some(workspace_root) = self
            .app_state
            .workspace_root_path()
            .map(|p| p.to_path_buf())
        else {
            self.show_transient_toast("Dart Env: no workspace open".to_string());
            return false;
        };

        self.app_state.open_dart_env_selector();
        self.request_redraw();

        self.submit(RequestSpec {
            revision_id: 0,
            topic: RequestTopic::SystemTask,
            payload: WorkerRequestPayload::ScanDartEnvironments { workspace_root },
        });
        true
    }

    pub(super) fn confirm_dart_env_selection(&mut self) -> bool {
        let selected_path = match self.app_state.command_palette_selected_action() {
            Some(CommandPaletteAction::SelectDartEnv(path)) => path,
            _ => return false,
        };

        let _ = self.app_state.close_command_palette();
        if let Ok(result) = self.app_state.apply_mode_event(ModeEvent::ExitFocus) {
            if result.changed {
                self.editor_needs_layout = true;
            }
        }
        self.focus_manager.set(FocusTarget::CenterEditor);
        self.input_handler.clear_pending_prefix();

        // Store selected env and restart LSP
        self.selected_dart_env = Some(selected_path.clone());
        self.sync_lsp_server_for_workspace();

        self.show_transient_toast(format!("Dart SDK selected: {}", selected_path.display()));
        true
    }
}
