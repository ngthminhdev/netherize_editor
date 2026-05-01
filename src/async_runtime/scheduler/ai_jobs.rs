use std::sync::mpsc as std_mpsc;

use crate::async_runtime::message::WorkerMessage;

use super::emit::emit_message_and_wake;
use winit::event_loop::EventLoopProxy;

use crate::app::event_loop::AppEvent;

/// Run an AI chat streaming request. Builds a system message with buffer
/// context and streams the response back through the worker channel.
///
/// Currently uses a placeholder response; replace with actual API call
/// (OpenAI/Anthropic) when ready.
pub(super) async fn run_ai_chat_stream(
    worker_tx: std_mpsc::Sender<WorkerMessage>,
    event_proxy: EventLoopProxy<AppEvent>,
    prompt: String,
    buffer_context: String,
    cursor_position: (usize, usize),
    history: Vec<(String, String)>,
) {
    // TODO: Replace with actual API call (OpenAI/Anthropic)
    // Build system message with buffer context
    let _system_msg = format!(
        "You are a coding assistant. The user is editing a file.\n\nCursor: line {}, col {}\n\nFile content:\n{}",
        cursor_position.0, cursor_position.1, buffer_context
    );

    // Build conversation context from history
    let _conversation_context: String = history
        .iter()
        .map(|(role, text)| format!("[{}]: {}", role, text))
        .collect::<Vec<_>>()
        .join("\n");

    // For now, simulate streaming with a placeholder response
    let response = format!(
        "I received your message: {}\n\nContext: {} lines of code at cursor ({}, {})\n\nThis is a placeholder response. Implement actual API call here.",
        prompt,
        buffer_context.lines().count(),
        cursor_position.0,
        cursor_position.1
    );

    // Simulate chunked streaming
    for word in response.split_whitespace() {
        let chunk = format!("{} ", word);
        emit_message_and_wake(
            &worker_tx,
            &event_proxy,
            WorkerMessage::AiMessageChunk { text: chunk },
        );
        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
    }

    emit_message_and_wake(
        &worker_tx,
        &event_proxy,
        WorkerMessage::AiStreamComplete,
    );
}
