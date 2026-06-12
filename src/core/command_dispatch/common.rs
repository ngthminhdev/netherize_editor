use std::path::PathBuf;

use crate::{
    app::{
        app_state::{AppState, ClipboardRecordKind},
        clipboard::ClipboardProvider,
    },
    core::mode::{EditorMode, ModeEvent},
    terminal::grid::TerminalGrid,
};

#[derive(Debug, Clone)]
pub struct DispatchReport {
    pub message: String,
    pub request_redraw: bool,
    pub success: bool,
    pub state_changed: bool,
}

impl DispatchReport {
    pub(super) fn success(message: impl Into<String>, state_changed: bool) -> Self {
        Self {
            message: message.into(),
            request_redraw: state_changed,
            success: true,
            state_changed,
        }
    }

    pub(super) fn success_with_flags(
        message: impl Into<String>,
        request_redraw: bool,
        state_changed: bool,
    ) -> Self {
        Self {
            message: message.into(),
            request_redraw,
            success: true,
            state_changed,
        }
    }

    pub(super) fn failure(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            request_redraw: false,
            success: false,
            state_changed: false,
        }
    }
}

pub(super) struct DispatchCtx<'state, 'clipboard, 'terminal> {
    pub(super) app_state: &'state mut AppState,
    pub(super) clipboard: &'state mut Option<&'clipboard mut dyn ClipboardProvider>,
    pub(super) terminal: &'state mut Option<&'terminal mut TerminalGrid>,
    auto_commit_text_transactions: bool,
}

impl<'state, 'clipboard, 'terminal> DispatchCtx<'state, 'clipboard, 'terminal> {
    pub(super) fn new(
        app_state: &'state mut AppState,
        clipboard: &'state mut Option<&'clipboard mut dyn ClipboardProvider>,
        terminal: &'state mut Option<&'terminal mut TerminalGrid>,
        auto_commit_text_transactions: bool,
    ) -> Self {
        Self {
            app_state,
            clipboard,
            terminal,
            auto_commit_text_transactions,
        }
    }

    pub(super) fn open_file(&mut self, path: PathBuf) -> DispatchReport {
        // Mở sang file khác là một jump (vim :e semantics) -> ghi lại vị trí
        // xuất phát để Ctrl+O quay về được. Mở lại chính file đang active thì bỏ qua.
        let origin = self
            .app_state
            .active_file()
            .filter(|active| *active != path.as_path())
            .map(|active| {
                let (line, col) = self.app_state.cursor_line_col();
                (active.to_path_buf(), line, col)
            });
        match self.app_state.open_file(path.clone()) {
            Ok(()) => {
                if let Some((origin_path, line, col)) = origin {
                    self.app_state.push_jump_entry(origin_path, line, col);
                }
                DispatchReport {
                    message: format!("Dispatch: open trigger succeeded -> {}", path.display()),
                    request_redraw: true,
                    success: true,
                    state_changed: true,
                }
            }
            Err(err) => DispatchReport::failure(format!("Dispatch: open trigger failed -> {err}")),
        }
    }

    pub(super) fn enter_insert_mode_if_needed(&mut self) -> Result<bool, String> {
        if self.app_state.current_mode() == EditorMode::Insert {
            return Ok(false);
        }

        if !self.app_state.can_apply_mode_event(ModeEvent::EnterInsert) {
            return Err(format!(
                "mode={} does not allow EnterInsert",
                self.app_state.current_mode().as_str()
            ));
        }

        self.app_state
            .apply_mode_event(ModeEvent::EnterInsert)
            .map(|result| result.changed)
            .map_err(|err| format!("{err:?}"))
    }

    pub(super) fn commit_text_transaction(&mut self, changed: bool) {
        if changed && self.auto_commit_text_transactions {
            let _ = self.app_state.commit_transaction();
        }
    }

    pub(super) fn write_text_to_clipboard(&mut self, text: Option<String>) -> bool {
        let Some(text) = text.filter(|text| !text.is_empty()) else {
            return false;
        };
        let Some(provider) = self.clipboard.as_deref_mut() else {
            return false;
        };

        match provider.set_text(&text) {
            Ok(()) => true,
            Err(err) => {
                eprintln!("[clipboard] {err}");
                false
            }
        }
    }

    pub(super) fn read_text_from_clipboard(&mut self) -> Result<String, String> {
        let Some(provider) = self.clipboard.as_deref_mut() else {
            return Err("system clipboard unavailable".to_string());
        };

        provider.get_text()
    }

    pub(super) fn remember_clipboard_text(&mut self, text: &str, kind: ClipboardRecordKind) {
        if text.is_empty() {
            return;
        }
        self.app_state
            .remember_clipboard_text(text.to_string(), kind);
    }

    pub(super) fn write_text_to_clipboard_and_remember(
        &mut self,
        text: Option<String>,
        kind: ClipboardRecordKind,
    ) -> bool {
        let Some(text) = text.filter(|text| !text.is_empty()) else {
            return false;
        };
        self.remember_clipboard_text(&text, kind);
        self.write_text_to_clipboard(Some(text))
    }

    pub(super) fn close_palette_and_exit_focus(&mut self) -> bool {
        let picker_closed = self.app_state.close_command_palette();
        let mut mode_changed = false;
        if self.app_state.current_mode() == EditorMode::PaletteFocus
            && let Ok(result) = self.app_state.apply_mode_event(ModeEvent::ExitFocus)
        {
            mode_changed = result.changed;
        }

        picker_closed || mode_changed
    }

    pub(super) fn terminal_grid_mut(&mut self) -> Option<&mut TerminalGrid> {
        self.terminal.as_deref_mut()
    }

    pub(super) fn terminal_normal_active(&self) -> bool {
        self.app_state.current_mode() == EditorMode::TerminalNormal && self.terminal.is_some()
    }
}

pub(super) fn normalize_palette_clipboard_text(text: &str) -> String {
    let mut normalized = String::new();
    let mut last_was_space = false;

    for ch in text.chars() {
        let next = if ch.is_control() {
            matches!(ch, '\n' | '\r' | '\t').then_some(' ')
        } else {
            Some(ch)
        };

        let Some(ch) = next else {
            continue;
        };

        if ch.is_whitespace() {
            if !last_was_space {
                normalized.push(' ');
                last_was_space = true;
            }
        } else {
            normalized.push(ch);
            last_was_space = false;
        }
    }

    normalized.trim().to_string()
}
