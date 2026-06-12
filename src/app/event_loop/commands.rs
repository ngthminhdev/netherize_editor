#[path = "commands_ai_chat.rs"]
mod commands_ai_chat;
#[path = "commands_completion.rs"]
mod commands_completion;
#[path = "commands_editor.rs"]
mod commands_editor;
#[path = "commands_explorer.rs"]
mod commands_explorer;
#[path = "commands_lsp.rs"]
mod commands_lsp;
#[path = "commands_palette.rs"]
mod commands_palette;
#[path = "commands_prompts.rs"]
mod commands_prompts;
#[path = "commands_settings.rs"]
mod commands_settings;
#[path = "commands_settings_helpers.rs"]
mod commands_settings_helpers;
#[path = "commands_terminal.rs"]
mod commands_terminal;
#[cfg(test)]
#[path = "commands_tests.rs"]
mod tests;

use super::*;
use crate::{
    app::clipboard::ClipboardProvider,
    app::command_palette::CommandPaletteMode,
    app::input::{LeapState, LeapTarget, generate_leap_labels},
    core::command_dispatch::{
        DispatchReport, dispatch_command_with_clipboard_count,
        dispatch_command_with_clipboard_count_with_terminal,
    },
};

fn dispatch_palette_overlay_command(
    app_state: &mut AppState,
    clipboard: &mut dyn ClipboardProvider,
    command: Command,
) -> crate::core::command_dispatch::DispatchReport {
    match command {
        Command::EditorPaste | Command::PasteSystemClipboard => {
            dispatch_command_with_clipboard(app_state, command, Some(clipboard))
        }
        _ => dispatch_command(app_state, command),
    }
}

impl AppShell {
    pub(super) fn handle_command(&mut self, command: Command) -> bool {
        self.handle_command_with_count(command, 1)
    }

    fn should_persist_history_after(command: &Command) -> bool {
        matches!(
            command,
            Command::InsertChar(_)
                | Command::InsertText(_)
                | Command::Newline
                | Command::Backspace
                | Command::InsertTab
                | Command::InsertLineBelow
                | Command::InsertLineAbove
                | Command::InsertAtLineStart
                | Command::AppendAtLineEnd
                | Command::AppendAfterCursor
                | Command::SubstituteLine
                | Command::DeleteChar
                | Command::DeleteSelection
                | Command::DeleteCurrentLine
                | Command::DeleteToLineEnd
                | Command::ToggleLineComment
                | Command::ToggleSelectionComment
                | Command::DeleteWordForward
                | Command::DeleteWordBackward
                | Command::ChangeSelection
                | Command::ChangeWordForward
                | Command::ChangeWordBackward
                | Command::ChangeToLineEnd
                | Command::JoinLines
                | Command::PasteAfter
                | Command::PasteBefore
                | Command::EditorPaste
                | Command::Undo
                | Command::Redo
        )
    }

    fn finalize_post_command_hooks(
        &mut self,
        command_for_post_hooks: &Command,
        _should_persist_history_after: bool,
        changed: bool,
    ) -> bool {
        if changed {
            match command_for_post_hooks {
                Command::OpenFile(_) | Command::BufferNext | Command::BufferPrev => {
                    self.submit_active_buffer_git_baseline_refresh();
                }
                Command::SaveFile => {
                    self.submit_workspace_git_status_refresh();
                    self.submit_active_buffer_git_baseline_refresh();
                }
                _ => {}
            }

            if self.app_state.markdown_preview.visible {
                self.update_markdown_preview_content();
            }
        }

        changed
    }

    fn reconcile_highlight_spans_with_pending_edits(&mut self) {
        let edits = self.app_state.take_highlight_edits();
        if edits.is_empty() {
            return;
        }

        crate::syntax::highlight::apply_highlight_edits(&mut self.highlight_spans, &edits);
        crate::syntax::highlight::apply_highlight_edits(&mut self.semantic_highlight_spans, &edits);

        // Invalidate line starts cache when text changes
        self.app_state.invalidate_line_starts_cache();

        // Store an incremental-parse hint when the transaction was a single edit.
        // Multiple edits (undo/redo, replace-all, paste of many chars) clear the hint
        // so the worker falls back to a safe full reparse.
        self.last_syntax_edit_hint = if edits.len() == 1 {
            Some(SyntaxEditHint {
                start_byte: edits[0].start,
                old_end_byte: edits[0].old_end,
                new_end_byte: edits[0].new_end,
            })
        } else {
            None
        };
    }

    fn dispatch_command_with_focused_terminal(
        &mut self,
        command: Command,
        repeat_count: usize,
    ) -> DispatchReport {
        let focus_target = self.focus_manager.current();
        let active_terminal_session = self.app_state.active_terminal_session_id();
        let active_buffer_is_terminal = self.app_state.active_buffer_is_terminal();

        if active_buffer_is_terminal && focus_target == FocusTarget::CenterEditor {
            let terminal = active_terminal_session
                .and_then(|session_id| self.terminal_buffer_grids.get_mut(&session_id));
            let (app_state, clipboard) = (&mut self.app_state, &mut self.clipboard);
            return dispatch_command_with_clipboard_count_with_terminal(
                app_state,
                command,
                repeat_count,
                Some(clipboard),
                terminal,
            );
        }

        if focus_target == FocusTarget::BottomPanel {
            // Access grid via direct indexing to avoid borrowing self
            // (focused_terminal_grid_mut borrows self, conflicting with app_state/clipboard).
            let idx = self.active_terminal_tab;
            let terminal = Some(&mut self.terminal_tabs[idx].grid);
            let (app_state, clipboard) = (&mut self.app_state, &mut self.clipboard);
            return dispatch_command_with_clipboard_count_with_terminal(
                app_state,
                command,
                repeat_count,
                Some(clipboard),
                terminal,
            );
        }

        let (app_state, clipboard) = (&mut self.app_state, &mut self.clipboard);
        dispatch_command_with_clipboard_count_with_terminal(
            app_state,
            command,
            repeat_count,
            Some(clipboard),
            None,
        )
    }

    pub(super) fn mark_focused_terminal_layout_dirty(&mut self) {
        if self.app_state.active_buffer_is_terminal()
            && self.focus_manager.current() == FocusTarget::CenterEditor
        {
            self.buffer_terminal_needs_layout = true;
        } else if self.focus_manager.current() == FocusTarget::RightSidebar {
            self.right_terminal_needs_layout = true;
        } else {
            self.terminal_needs_layout = true;
        }
    }

    fn handle_terminal_normal_command(
        &mut self,
        command: &Command,
        repeat_count: usize,
    ) -> Option<bool> {
        let terminal_copy_routing = self.app_state.current_mode() == EditorMode::TerminalNormal
            || matches!(command, Command::SwitchMode(ModeEvent::EnterTerminalNormal));
        if !terminal_copy_routing {
            return None;
        }

        let supported = matches!(
            command,
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
                | Command::MoveToFirstLine
                | Command::MoveToLastLine
                | Command::ScrollHalfPageUp
                | Command::ScrollHalfPageDown
                | Command::CenterCursorLine
                | Command::YankSelection
                | Command::EnterVisualLine
                | Command::SwitchMode(ModeEvent::EnterTerminalNormal)
                | Command::SwitchMode(ModeEvent::EnterVisual | ModeEvent::FocusTerminal)
        );
        if !supported {
            return None;
        }

        // Clear terminal search highlights when leaving terminal_normal via Esc
        if matches!(command, Command::SwitchMode(ModeEvent::FocusTerminal)) {
            if let Some(grid) = self.focused_terminal_grid_mut() {
                grid.search_matches.clear();
                grid.search_cursor = 0;
            }
        }

        let report = self.dispatch_command_with_focused_terminal(command.clone(), repeat_count);
        if report.state_changed {
            self.mark_focused_terminal_layout_dirty();
        }
        Some(report.request_redraw || report.state_changed)
    }

    pub(super) fn handle_command_with_count(
        &mut self,
        command: Command,
        repeat_count: usize,
    ) -> bool {
        if matches!(command, Command::NewInstance) {
            return self.spawn_new_instance();
        }

        let command_for_post_hooks = command.clone();
        let should_persist_history_after =
            Self::should_persist_history_after(&command_for_post_hooks);
        let is_insert_typing = matches!(
            command,
            Command::InsertChar(_)
                | Command::InsertText(_)
                | Command::Backspace
                | Command::Newline
                | Command::InsertTab
        ) && self.app_state.current_mode() == EditorMode::Insert;
        self.ai_inline_suggestion_retained = false;
        if is_insert_typing {
            let retained = repeat_count == 1
                && match &command {
                    // Auto-pairing chars insert a closing char the suggestion
                    // doesn't know about — never retain across those.
                    Command::InsertChar(ch)
                        if !matches!(ch, '(' | '[' | '{' | '"' | '\'' | '`') =>
                    {
                        let mut buf = [0u8; 4];
                        self.app_state
                            .retain_inline_suggestion_for_typed_text(ch.encode_utf8(&mut buf))
                    }
                    Command::InsertText(text) => self
                        .app_state
                        .retain_inline_suggestion_for_typed_text(text),
                    _ => false,
                };
            if retained {
                // In-flight results no longer match the retained remainder;
                // bump the revision so they get dropped as stale.
                self.ai_inline_revision = self.ai_inline_revision.saturating_add(1);
                self.ai_inline_suggestion_retained = true;
            } else {
                let _ = self.app_state.clear_inline_suggestion();
            }
            self.cancel_ai_inline_completion();
        }
        if matches!(command, Command::SwitchMode(ModeEvent::Escape)) {
            self.cancel_lsp_completion_pipeline();
        }
        if matches!(command, Command::TerminalPaste) {
            return self.handle_terminal_paste();
        }

        if matches!(command, Command::ToggleMaximizeFocus)
            && self.panel_state.maximized_region.is_some()
            && let Some(changed) = self.handle_terminal_and_focus_command(&command)
        {
            return self.finalize_post_command_hooks(
                &command_for_post_hooks,
                should_persist_history_after,
                changed,
            );
        }

        if let Some(changed) = self.handle_terminal_normal_command(&command, repeat_count) {
            return changed;
        }

        if let Some(changed) = self.handle_terminal_search_command(&command) {
            return self.finalize_post_command_hooks(
                &command_for_post_hooks,
                should_persist_history_after,
                changed,
            );
        }

        if let Some(changed) = self.handle_terminal_and_focus_command(&command) {
            return self.finalize_post_command_hooks(
                &command_for_post_hooks,
                should_persist_history_after,
                changed,
            );
        }
        if let Some(changed) = self.handle_ai_chat_command(&command) {
            return self.finalize_post_command_hooks(
                &command_for_post_hooks,
                should_persist_history_after,
                changed,
            );
        }

        if let Some(changed) = self.handle_settings_command(&command) {
            return self.finalize_post_command_hooks(
                &command_for_post_hooks,
                should_persist_history_after,
                changed,
            );
        }
        if let Some(changed) = self.handle_lsp_and_diagnostics_command(&command) {
            return self.finalize_post_command_hooks(
                &command_for_post_hooks,
                should_persist_history_after,
                changed,
            );
        }
        if let Some(changed) =
            self.handle_palette_and_open_command(&command, repeat_count, &command_for_post_hooks)
        {
            return self.finalize_post_command_hooks(
                &command_for_post_hooks,
                should_persist_history_after,
                changed,
            );
        }
        if let Some(changed) = self.handle_explorer_and_workspace_command(&command) {
            return self.finalize_post_command_hooks(
                &command_for_post_hooks,
                should_persist_history_after,
                changed,
            );
        }
        if let Some(changed) = self.handle_insert_edit_command(&command, repeat_count) {
            return self.finalize_post_command_hooks(
                &command_for_post_hooks,
                should_persist_history_after,
                changed,
            );
        }
        if let Some(changed) = self.handle_markdown_preview_command(&command) {
            return self.finalize_post_command_hooks(
                &command_for_post_hooks,
                should_persist_history_after,
                changed,
            );
        }
        if let Some(changed) = self.handle_viewport_navigation_command(&command) {
            return self.finalize_post_command_hooks(
                &command_for_post_hooks,
                should_persist_history_after,
                changed,
            );
        }
        if let Some(changed) = self.handle_leap_command(&command) {
            return self.finalize_post_command_hooks(
                &command_for_post_hooks,
                should_persist_history_after,
                changed,
            );
        }
        if let Some(changed) = self.handle_help_command(&command) {
            return self.finalize_post_command_hooks(
                &command_for_post_hooks,
                should_persist_history_after,
                changed,
            );
        }

        let mode_before = self.app_state.current_mode();
        // Check if this is a text-modifying command before moving `command`
        let is_text_modifying_command = matches!(
            command,
            Command::DeleteChar
                | Command::DeleteSelection
                | Command::DeleteCurrentLine
                | Command::DeleteToLineEnd
                | Command::DeleteWordForward
                | Command::DeleteWordBackward
                | Command::ChangeSelection
                | Command::ChangeWordForward
                | Command::ChangeWordBackward
                | Command::ChangeToLineEnd
                | Command::SubstituteLine
                | Command::Operate {
                    op: crate::core::commands::Operator::Delete
                        | crate::core::commands::Operator::Change,
                    ..
                }
        );
        let changed = self.handle_generic_editor_command(command, repeat_count);
        let mode_after = self.app_state.current_mode();

        // Clear overlays when transitioning to Insert mode from editing commands,
        // or when performing text-modifying operations (delete, change, etc.)
        let should_clear_overlay = changed
            && (mode_before != EditorMode::Insert && mode_after == EditorMode::Insert
                || is_text_modifying_command);
        if should_clear_overlay {
            let overlay_cleared = self.app_state.clear_current_overlays();
            if overlay_cleared {
                self.editor_needs_layout = true;
            }
            // Invalidate pending hover requests so stale responses are dropped
            self.latest_hover_request_id = None;
        }

        if changed {
            match &command_for_post_hooks {
                Command::OpenFile(_) | Command::BufferNext | Command::BufferPrev => {
                    self.submit_active_buffer_git_baseline_refresh();
                }
                Command::SaveFile => {
                    self.submit_workspace_git_status_refresh();
                    self.submit_active_buffer_git_baseline_refresh();
                }
                _ => {}
            }
        }

        changed
    }

    fn spawn_new_instance(&mut self) -> bool {
        let exe_path = match std::env::current_exe() {
            Ok(path) => path,
            Err(err) => {
                eprintln!("[AppShell] resolve current executable failed: {err}");
                self.show_transient_toast("Unable to open new instance".to_string());
                return false;
            }
        };

        let mut command = std::process::Command::new(exe_path);
        if let Some(workspace_root) = self.app_state.workspace_root_path() {
            command.arg(workspace_root);
        }
        match command.spawn() {
            Ok(_) => {
                self.show_transient_toast("Opened new instance".to_string());
                true
            }
            Err(err) => {
                eprintln!("[AppShell] spawn new instance failed: {err}");
                self.show_transient_toast("Unable to open new instance".to_string());
                false
            }
        }
    }

    fn handle_terminal_paste(&mut self) -> bool {
        let clipboard_text = match self.clipboard.get_text() {
            Ok(text) => text,
            Err(err) => {
                eprintln!("[terminal] paste failed: {err}");
                return false;
            }
        };
        if clipboard_text.is_empty() {
            return false;
        }

        let payload = normalize_terminal_paste_text(&clipboard_text);
        if payload.is_empty() {
            return false;
        }

        let mut changed = false;
        if self.app_state.current_mode() == EditorMode::TerminalNormal {
            if let Some(grid) = self.focused_terminal_grid_mut() {
                changed |= grid.exit_normal_mode();
            }
            if let Ok(result) = self.app_state.apply_mode_event(ModeEvent::FocusTerminal) {
                changed |= result.changed;
            }
            self.mark_focused_terminal_layout_dirty();
        }

        let Some(session_id) = self.focused_terminal_session_id() else {
            eprintln!("[terminal] paste ignored: no focused PTY session");
            return changed;
        };

        self.forward_to_terminal_session(session_id, &payload);
        changed
    }

    fn forward_to_pty(&self, text: &str) {
        if let Some(session_id) = self.focused_terminal_session_id() {
            self.forward_to_terminal_session(session_id, text);
        }
    }

    fn forward_to_terminal_session(&self, session_id: u64, text: &str) {
        self.submit(RequestSpec {
            revision_id: 0,
            topic: RequestTopic::TerminalPty,
            payload: WorkerRequestPayload::WritePtyInput {
                session_id,
                input: text.to_string(),
            },
        });
    }

    pub(super) fn dismiss_initial_launch_welcome_if_active(&mut self) -> bool {
        if !self.app_state.is_initial_launch_welcome() {
            return false;
        }
        let changed = self.app_state.dismiss_initial_launch_welcome();
        if changed {
            self.request_redraw();
        }
        changed
    }

    pub(super) fn dismiss_lsp_guide(&mut self) {
        if let Some(guide) = self.active_lsp_guide.take() {
            self.dismissed_lsp_binaries.insert(guide.binary);
        }
        if let Some(renderer) = self.renderer.as_mut() {
            renderer.clear_lsp_guide_popup();
        }
    }

    pub(super) fn dismiss_system_dep_guide(&mut self) {
        self.active_system_dep_guide = None;
        self.dismissed_system_deps = true;
        if let Some(renderer) = self.renderer.as_mut() {
            renderer.clear_system_dep_popup();
        }
    }

    pub(super) fn accept_system_dep_guide(&mut self) -> bool {
        let tools_to_install = {
            let Some(guide) = self.active_system_dep_guide.as_mut() else {
                return false;
            };
            use crate::app::event_loop::SystemDepState;
            match guide.state {
                SystemDepState::Complete => {
                    self.active_system_dep_guide = None;
                    self.dismissed_system_deps = true;
                    if let Some(renderer) = self.renderer.as_mut() {
                        renderer.clear_system_dep_popup();
                    }
                    return true;
                }
                SystemDepState::Installing => {
                    return false;
                }
                SystemDepState::Detected => {
                    let tools = guide.missing_tools.clone().unwrap_or_default();
                    guide.state = SystemDepState::Installing;
                    // Reset per-tool statuses to Pending before install starts.
                    for entry in &mut guide.tool_statuses {
                        entry.1 = crate::async_runtime::message::InstallStatus::Pending;
                    }
                    tools
                }
            }
        };

        self.submit(RequestSpec {
            revision_id: 0,
            topic: RequestTopic::SystemDepInstall,
            payload: WorkerRequestPayload::InstallSystemDeps {
                tools: tools_to_install,
            },
        });
        true
    }

    pub(super) fn show_transient_toast(&mut self, message: impl Into<String>) {
        self.show_transient_toast_kind(message, ToastKind::Info);
    }

    pub(super) fn show_transient_toast_kind(
        &mut self,
        message: impl Into<String>,
        kind: ToastKind,
    ) {
        let now = Instant::now();
        self.transient_toast = Some(TransientToast {
            message: message.into(),
            kind,
            created_at: now,
            expires_at: now + Duration::from_secs(4),
        });
    }

    /// One-time onboarding toast for brand-new installs: the three keys a new
    /// user needs before anything else. Marks persistent state so it never
    /// shows again.
    pub(super) fn show_first_run_tour_if_needed(&mut self) {
        if self.persistent_state.first_run_tour_shown {
            return;
        }
        self.persistent_state.first_run_tour_shown = true;
        self.persistent_state.save();

        let now = Instant::now();
        self.transient_toast = Some(TransientToast {
            message: "Welcome to Netherize\nPress i to type, Esc to go back to Normal mode, and Space ? (or F1) to see every keybinding.".to_string(),
            kind: ToastKind::Info,
            created_at: now,
            expires_at: now + Duration::from_secs(12),
        });
    }

    pub(super) fn clear_expired_transient_toast(&mut self) -> bool {
        let expired = self
            .transient_toast
            .as_ref()
            .is_some_and(|toast| Instant::now() >= toast.expires_at);
        if !expired {
            return false;
        }

        self.transient_toast = None;
        if let Some(renderer) = self.renderer.as_mut() {
            renderer.clear_toast_popup();
        }
        true
    }

    pub(super) fn close_current_buffer_now(&mut self) -> bool {
        let closed_terminal_session = self.app_state.active_terminal_session_id();
        let report = dispatch_command(&mut self.app_state, Command::BufferCloseCurrent);
        self.reconcile_highlight_spans_with_pending_edits();

        if report.state_changed {
            if let Some(session_id) = closed_terminal_session {
                self.terminal_buffer_grids.remove(&session_id);
                self.submit(RequestSpec {
                    revision_id: 0,
                    topic: RequestTopic::TerminalPty,
                    payload: WorkerRequestPayload::ClosePtySession { session_id },
                });
            }
            self.clear_highlight_layers();
            self.mark_explorer_dirty();
            let viewport_lines = self.editor_viewport_lines();
            self.app_state.auto_scroll_to_cursor(viewport_lines);
            self.editor_needs_layout = true;
            self.editor_caret_needs_layout = false;
            self.buffer_terminal_needs_layout = true;
            self.submit_parse_for_active_buffer(true);
            self.submit_lsp_did_open_for_active_file();
            let _ = self.sync_focus_mode_for_active_buffer();
            self.submit_active_buffer_git_baseline_refresh();
        }

        report.request_redraw || report.state_changed
    }

    fn handle_markdown_preview_command(&mut self, command: &Command) -> Option<bool> {
        if matches!(
            command,
            Command::ScrollHalfPageUp
                | Command::ScrollHalfPageDown
                | Command::MoveToFirstLine
                | Command::MoveToLastLine
                | Command::CenterCursorLine
                | Command::MarkdownPreviewScrollUp
                | Command::MarkdownPreviewScrollDown
                | Command::MarkdownPreviewScrollTop
                | Command::MarkdownPreviewScrollBottom
                | Command::MarkdownPreviewScrollHalfPageUp
                | Command::MarkdownPreviewScrollHalfPageDown
        ) && self.app_state.active_buffer_is_markdown_preview()
            && let Some(preview) = self.app_state.active_markdown_preview_buffer().cloned()
        {
            self.app_state.markdown_preview = preview;
            self.app_state.markdown_preview.visible = true;
        }

        match command {
            Command::ToggleMarkdownPreview => {
                let is_markdown = self
                    .app_state
                    .active_file()
                    .and_then(|p| p.extension())
                    .is_some_and(|ext| ext == "md");

                if !is_markdown && !self.app_state.active_buffer_is_markdown_preview() {
                    return Some(false);
                }

                let source = if self.app_state.active_text_buffer().is_some() {
                    self.app_state.text_string()
                } else if let Some(preview) = self.app_state.active_markdown_preview_buffer() {
                    preview.source_text.clone()
                } else {
                    String::new()
                };
                let rendered_lines = crate::app::event_loop::helpers::parse_markdown_preview_blocks(
                    &source,
                    &self.theme,
                );

                self.app_state.markdown_preview.visible = true;
                self.app_state.markdown_preview.scroll_y = 0.0;
                self.app_state.markdown_preview.source_path =
                    self.app_state.active_file().map(|path| path.to_path_buf());
                self.app_state.markdown_preview.source_text = source;
                self.app_state.markdown_preview.source_revision = self.app_state.revision();
                self.app_state.markdown_preview.rendered_lines = rendered_lines;
                let preview = self.app_state.markdown_preview.clone();
                self.app_state.open_markdown_preview_buffer(preview);
                self.focus_manager.set(FocusTarget::CenterEditor);
                self.input_handler.clear_pending_prefix();
                self.editor_needs_layout = true;
                Some(true)
            }
            Command::CloseSidebars => {
                let mut changed = false;

                let focus = self.focus_manager.current();
                let close_right = focus != FocusTarget::LeftSidebar;
                let close_left = focus != FocusTarget::RightSidebar;

                // Close right panel if it's on Markdown Preview
                if close_right && self.app_state.markdown_preview.visible {
                    self.app_state.markdown_preview.visible = false;

                    if let Some(original_width) = self.pre_markdown_preview_right_width.take() {
                        self.panel_state.right.size_px = original_width;
                    }
                    if self.panel_state.right.active_tab_id() == Some(PanelTabId::MarkdownPreview) {
                        self.panel_state.right.visible = false;
                        self.sidebar_needs_layout = true;
                    }
                    if self.focus_manager.current() == FocusTarget::RightSidebar {
                        self.focus_manager.set(FocusTarget::CenterEditor);
                    }
                    changed = true;
                }

                // Close left panel (file tree/explorer)
                if close_left && self.panel_state.left.visible {
                    self.panel_state.left.visible = false;
                    self.sidebar_needs_layout = true;
                    if self.focus_manager.current() == FocusTarget::LeftSidebar {
                        self.focus_manager.set(FocusTarget::CenterEditor);
                    }
                    changed = true;
                }

                Some(changed)
            }
            Command::FocusMarkdownPreview => {
                self.handle_markdown_preview_command(&Command::ToggleMarkdownPreview)
            }
            Command::ScrollHalfPageUp if self.app_state.active_buffer_is_markdown_preview() => {
                self.handle_markdown_preview_command(&Command::MarkdownPreviewScrollHalfPageUp)
            }
            Command::ScrollHalfPageDown if self.app_state.active_buffer_is_markdown_preview() => {
                self.handle_markdown_preview_command(&Command::MarkdownPreviewScrollHalfPageDown)
            }
            Command::MoveToFirstLine if self.app_state.active_buffer_is_markdown_preview() => {
                self.handle_markdown_preview_command(&Command::MarkdownPreviewScrollTop)
            }
            Command::MoveToLastLine if self.app_state.active_buffer_is_markdown_preview() => {
                self.handle_markdown_preview_command(&Command::MarkdownPreviewScrollBottom)
            }
            Command::CenterCursorLine if self.app_state.active_buffer_is_markdown_preview() => {
                self.editor_needs_layout = true;
                Some(true)
            }
            Command::MarkdownPreviewScrollUp => {
                let preview = &mut self.app_state.markdown_preview;
                if !preview.visible {
                    return Some(false);
                }
                let previous = preview.scroll_y;
                preview.scroll_y = (preview.scroll_y - 3.0).max(0.0);
                let changed = (preview.scroll_y - previous).abs() > f32::EPSILON;
                if changed {
                    self.editor_needs_layout = true;
                    let _ = self
                        .app_state
                        .sync_markdown_preview_buffer(self.app_state.markdown_preview.clone());
                }
                Some(changed)
            }
            Command::MarkdownPreviewScrollDown => {
                let preview = &mut self.app_state.markdown_preview;
                if !preview.visible {
                    return Some(false);
                }
                let previous = preview.scroll_y;
                let max_scroll = if preview.max_scroll > 0.0 {
                    preview.max_scroll
                } else {
                    preview.rendered_lines.len().saturating_sub(1) as f32
                };
                preview.scroll_y = (preview.scroll_y + 3.0).min(max_scroll);
                let changed = (preview.scroll_y - previous).abs() > f32::EPSILON;
                if changed {
                    self.editor_needs_layout = true;
                    let _ = self
                        .app_state
                        .sync_markdown_preview_buffer(self.app_state.markdown_preview.clone());
                }
                Some(changed)
            }
            Command::MarkdownPreviewScrollHalfPageUp => {
                let preview = &mut self.app_state.markdown_preview;
                if !preview.visible {
                    return Some(false);
                }
                let previous = preview.scroll_y;
                preview.scroll_y = (preview.scroll_y - 15.0).max(0.0);
                let changed = (preview.scroll_y - previous).abs() > f32::EPSILON;
                if changed {
                    self.editor_needs_layout = true;
                    let _ = self
                        .app_state
                        .sync_markdown_preview_buffer(self.app_state.markdown_preview.clone());
                }
                Some(changed)
            }
            Command::MarkdownPreviewScrollHalfPageDown => {
                let preview = &mut self.app_state.markdown_preview;
                if !preview.visible {
                    return Some(false);
                }
                let previous = preview.scroll_y;
                let max_scroll = if preview.max_scroll > 0.0 {
                    preview.max_scroll
                } else {
                    preview.rendered_lines.len().saturating_sub(1) as f32
                };
                preview.scroll_y = (preview.scroll_y + 15.0).min(max_scroll);
                let changed = (preview.scroll_y - previous).abs() > f32::EPSILON;
                if changed {
                    self.editor_needs_layout = true;
                    let _ = self
                        .app_state
                        .sync_markdown_preview_buffer(self.app_state.markdown_preview.clone());
                }
                Some(changed)
            }
            Command::MarkdownPreviewScrollTop => {
                let preview = &mut self.app_state.markdown_preview;
                if !preview.visible {
                    return Some(false);
                }
                let previous = preview.scroll_y;
                preview.scroll_y = 0.0;
                let changed = (preview.scroll_y - previous).abs() > f32::EPSILON;
                if changed {
                    self.editor_needs_layout = true;
                    let _ = self
                        .app_state
                        .sync_markdown_preview_buffer(self.app_state.markdown_preview.clone());
                }
                Some(changed)
            }
            Command::MarkdownPreviewScrollBottom => {
                let preview = &mut self.app_state.markdown_preview;
                if !preview.visible {
                    return Some(false);
                }
                let previous = preview.scroll_y;
                let max_scroll = if preview.max_scroll > 0.0 {
                    preview.max_scroll
                } else {
                    preview.rendered_lines.len().saturating_sub(1) as f32
                };
                preview.scroll_y = max_scroll;
                let changed = (preview.scroll_y - previous).abs() > f32::EPSILON;
                if changed {
                    self.editor_needs_layout = true;
                    let _ = self
                        .app_state
                        .sync_markdown_preview_buffer(self.app_state.markdown_preview.clone());
                }
                Some(changed)
            }
            _ => None,
        }
    }

    fn handle_help_command(&mut self, command: &Command) -> Option<bool> {
        match command {
            Command::HelpScrollDown => {
                self.app_state.help_scroll_down(100.0);
                Some(true)
            }
            Command::HelpScrollUp => {
                self.app_state.help_scroll_up(100.0);
                Some(true)
            }
            Command::HelpScrollHalfPageDown => {
                self.app_state.help_scroll_down(400.0);
                Some(true)
            }
            Command::HelpScrollHalfPageUp => {
                self.app_state.help_scroll_up(400.0);
                Some(true)
            }
            _ => None,
        }
    }

    fn update_markdown_preview_content(&mut self) {
        let (source, revision) = if self.app_state.active_text_buffer().is_some() {
            (self.app_state.text_string(), self.app_state.revision())
        } else if let Some(preview) = self.app_state.active_markdown_preview_buffer() {
            (preview.source_text.clone(), preview.source_revision)
        } else {
            return;
        };

        let preview = &mut self.app_state.markdown_preview;

        if preview.source_text != source || preview.source_revision != revision {
            preview.source_text = source.clone();
            preview.source_revision = revision;
            preview.rendered_lines = crate::app::event_loop::helpers::parse_markdown_preview_blocks(
                &source,
                &self.theme,
            );
        }

        self.app_state
            .sync_markdown_preview_buffer(self.app_state.markdown_preview.clone());
    }
}

fn normalize_terminal_paste_text(text: &str) -> String {
    let mut normalized = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();

    while let Some(ch) = chars.next() {
        match ch {
            '\r' => {
                if chars.peek() == Some(&'\n') {
                    let _ = chars.next();
                }
                normalized.push('\r');
            }
            '\n' => normalized.push('\r'),
            _ => normalized.push(ch),
        }
    }

    normalized
}
