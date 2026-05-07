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
        should_persist_history_after: bool,
        changed: bool,
    ) -> bool {
        if changed {
            match command_for_post_hooks {
                Command::OpenFile(_) | Command::BufferNext | Command::BufferPrev => {
                    self.submit_active_buffer_git_baseline_refresh();
                    self.submit_active_file_history_load();
                }
                Command::SaveFile => {
                    self.submit_active_file_history_save();
                    self.submit_workspace_git_status_refresh();
                    self.submit_active_buffer_git_baseline_refresh();
                }
                _ if should_persist_history_after => {
                    self.submit_active_file_history_save();
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
            let terminal = Some(&mut self.terminal_grid);
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
        if is_insert_typing {
            let _ = self.app_state.clear_inline_suggestion();
            self.pending_ai_inline_request = None;
        }
        if matches!(command, Command::TerminalPaste) {
            return self.handle_terminal_paste();
        }

        // Bất kỳ action nào trong lúc welcome đang hiện → dismiss về tabnone.
        if self.should_show_welcome() {
            if self.app_state.dismiss_initial_launch_welcome() {
                self.request_redraw();
            }
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
        if let Some(changed) = self.handle_markdown_preview_command(&command) {
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

        let changed = self.handle_generic_editor_command(command, repeat_count);

        if changed {
            match &command_for_post_hooks {
                Command::OpenFile(_) | Command::BufferNext | Command::BufferPrev => {
                    self.submit_active_buffer_git_baseline_refresh();
                    self.submit_active_file_history_load();
                }
                Command::SaveFile => {
                    self.submit_active_file_history_save();
                    self.submit_workspace_git_status_refresh();
                    self.submit_active_buffer_git_baseline_refresh();
                }
                _ if should_persist_history_after => {
                    self.submit_active_file_history_save();
                }
                _ => {}
            }
        }

        changed
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
        self.transient_toast = Some(TransientToast {
            message: message.into(),
            expires_at: Instant::now() + Duration::from_secs(4),
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
        }

        report.request_redraw || report.state_changed
    }

    fn handle_markdown_preview_command(&mut self, command: &Command) -> Option<bool> {
        match command {
            Command::ToggleMarkdownPreview => {
                let preview = &mut self.app_state.markdown_preview;
                preview.visible = !preview.visible;
                if preview.visible {
                    if !self.panel_state.right.visible {
                        self.panel_state.right.visible = true;
                        self.sidebar_needs_layout = true;
                    }
                    // Save original width before override so it can be
                    // restored when the preview is closed.  This prevents the
                    // 50 % width from leaking into other right-panel tabs
                    // such as AI chat.
                    self.pre_markdown_preview_right_width =
                        Some(self.panel_state.right.size_px);
                    // Auto-set width to 50% of window
                    let half_width = (self.window_size.width as f32 * 0.5).max(200.0);
                    self.panel_state.right.size_px = half_width;
                    self.panel_state.right.switch_to_tab(PanelTabId::MarkdownPreview);
                    self.focus_manager.set(FocusTarget::RightSidebar);
                    self.input_handler.clear_pending_prefix();
                    self.update_markdown_preview_content();
                } else {
                    // Restore the original width that was saved when the
                    // preview was opened, so other tabs keep their
                    // configured width.
                    if let Some(original_width) = self.pre_markdown_preview_right_width.take() {
                        self.panel_state.right.size_px = original_width;
                    }
                    if self.panel_state.right.active_tab_id()
                        == Some(PanelTabId::MarkdownPreview)
                    {
                        self.panel_state.right.visible = false;
                        self.sidebar_needs_layout = true;
                    }
                    if self.focus_manager.current() == FocusTarget::RightSidebar {
                        self.focus_manager.set(FocusTarget::CenterEditor);
                    }
                }
                Some(true)
            }
            Command::FocusMarkdownPreview => {
                let mut changed = self.release_focus_mode_to_editor();
                let preview = &mut self.app_state.markdown_preview;
                if !preview.visible {
                    preview.visible = true;
                    changed = true;
                }
                if !self.panel_state.right.visible {
                    self.panel_state.right.visible = true;
                    self.sidebar_needs_layout = true;
                    changed = true;
                }
                if self.pre_markdown_preview_right_width.is_none() {
                    self.pre_markdown_preview_right_width = Some(self.panel_state.right.size_px);
                }
                let half_width = (self.window_size.width as f32 * 0.5).max(200.0);
                if (self.panel_state.right.size_px - half_width).abs() > f32::EPSILON {
                    self.panel_state.right.size_px = half_width;
                    changed = true;
                }
                changed |= self
                    .panel_state
                    .right
                    .switch_to_tab(PanelTabId::MarkdownPreview);
                let focus_changed = self.focus_manager.set(FocusTarget::RightSidebar);
                changed |= focus_changed;
                if focus_changed {
                    self.input_handler.clear_pending_prefix();
                }
                self.update_markdown_preview_content();
                Some(changed)
            }
            Command::MarkdownPreviewScrollUp => {
                let preview = &mut self.app_state.markdown_preview;
                if !preview.visible {
                    return Some(false);
                }
                preview.scroll_y = (preview.scroll_y - 3.0).max(0.0);
                Some(true)
            }
            Command::MarkdownPreviewScrollDown => {
                let preview = &mut self.app_state.markdown_preview;
                if !preview.visible {
                    return Some(false);
                }
                let max_scroll = preview.rendered_lines.len().saturating_sub(1) as f32;
                preview.scroll_y = (preview.scroll_y + 3.0).min(max_scroll);
                Some(true)
            }
            Command::MarkdownPreviewScrollHalfPageUp => {
                let preview = &mut self.app_state.markdown_preview;
                if !preview.visible {
                    return Some(false);
                }
                preview.scroll_y = (preview.scroll_y - 15.0).max(0.0);
                Some(true)
            }
            Command::MarkdownPreviewScrollHalfPageDown => {
                let preview = &mut self.app_state.markdown_preview;
                if !preview.visible {
                    return Some(false);
                }
                let max_scroll = preview.rendered_lines.len().saturating_sub(1) as f32;
                preview.scroll_y = (preview.scroll_y + 15.0).min(max_scroll);
                Some(true)
            }
            Command::MarkdownPreviewScrollTop => {
                let preview = &mut self.app_state.markdown_preview;
                if !preview.visible {
                    return Some(false);
                }
                preview.scroll_y = 0.0;
                Some(true)
            }
            Command::MarkdownPreviewScrollBottom => {
                let preview = &mut self.app_state.markdown_preview;
                if !preview.visible {
                    return Some(false);
                }
                let max_scroll = preview.rendered_lines.len().saturating_sub(1) as f32;
                preview.scroll_y = max_scroll;
                Some(true)
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
        let source = self.app_state.text_string();
        let revision = self.app_state.revision();
        let preview = &mut self.app_state.markdown_preview;

        if preview.source_text == source && preview.source_revision == revision {
            return;
        }

        preview.source_text = source.clone();
        preview.source_revision = revision;
        preview.rendered_lines = crate::app::event_loop::helpers::parse_markdown_preview_blocks(
            &source,
            &self.theme,
        );
    }
}

/// Wraps a filesystem path in single quotes suitable for POSIX shell, escaping
/// any embedded single-quote characters so the path can be safely passed to cd.
fn shell_quote_path(path: &std::path::Path) -> String {
    let s = path.to_string_lossy();
    format!("'{}'", s.replace('\'', "'\\''"))
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
