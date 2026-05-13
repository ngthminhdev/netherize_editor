use super::super::*;
use crate::async_runtime::message::WorkerResultPayload;

pub(super) fn handle_ai_result(app: &mut AppShell, payload: WorkerResultPayload) {
    match payload {
        WorkerResultPayload::AiInlineCompletionChunk { chunk } => {
            if app.app_state.append_inline_suggestion_chunk(&chunk) {
                app.editor_needs_layout = true;
                app.editor_caret_needs_layout = false;
                app.request_redraw();
            }
        }
        WorkerResultPayload::AiInlineCompletionResult { suggestion } => {
            if app.app_state.set_inline_suggestion(Some(suggestion)) {
                app.editor_needs_layout = true;
                app.editor_caret_needs_layout = false;
                app.request_redraw();
            }
        }
        _ => {}
    }
}

pub(super) fn handle_ai_message_chunk(app: &mut AppShell, text: String) {
    let chat = &mut app.panel_state.ai_chat;
    if let Some(last) = chat.messages.last_mut()
        && last.role == crate::workbench::panel_state::AiRole::Assistant
    {
        last.text.push_str(&text);
    } else {
        chat.messages
            .push(crate::workbench::panel_state::AiChatMessage {
                role: crate::workbench::panel_state::AiRole::Assistant,
                text,
            });
    }
    app.request_redraw();
}

pub(super) fn handle_ai_stream_complete(app: &mut AppShell) {
    app.panel_state.ai_chat.is_generating = false;
    app.request_redraw();
}

pub(super) fn handle_ai_stream_cancelled(app: &mut AppShell) {
    app.panel_state.ai_chat.is_generating = false;
    app.panel_state
        .ai_chat
        .messages
        .push(crate::workbench::panel_state::AiChatMessage {
            role: crate::workbench::panel_state::AiRole::System,
            text: "Generation stopped.".to_string(),
        });
    app.request_redraw();
}

pub(super) fn handle_ai_stream_error(app: &mut AppShell, error: String) {
    app.panel_state.ai_chat.is_generating = false;
    app.panel_state
        .ai_chat
        .messages
        .push(crate::workbench::panel_state::AiChatMessage {
            role: crate::workbench::panel_state::AiRole::System,
            text: format!("Error: {}", error),
        });
    app.request_redraw();
}

pub(super) fn handle_ai_install_success(app: &mut AppShell) {
    app.panel_state.ai_chat.is_generating = false;
    app.panel_state.ai_chat.is_opencode_missing = false;

    let shell = std::env::var("SHELL").unwrap_or_default();
    let source_cmd = if shell.contains("zsh") {
        "source ~/.zshrc"
    } else if shell.contains("bash") {
        "source ~/.bash_profile"
    } else if shell.contains("fish") {
        "source ~/.config/fish/config.fish"
    } else {
        "source ~/.profile"
    };

    let next_steps = format!(
        "opencode installed!\n\
         \n\
         PATH chưa được cập nhật trong session này.\n\
         Làm theo 2 bước:\n\
         1. Mở terminal, chạy:  {source_cmd}\n\
         2. Khởi động lại editor."
    );

    app.panel_state
        .ai_chat
        .messages
        .push(crate::workbench::panel_state::AiChatMessage {
            role: crate::workbench::panel_state::AiRole::System,
            text: next_steps,
        });
    app.request_redraw();
}
