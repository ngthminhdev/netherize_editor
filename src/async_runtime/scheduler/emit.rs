use std::{any::Any, sync::mpsc as std_mpsc};

use tokio::task::JoinError;
use winit::event_loop::EventLoopProxy;

use crate::{
    app::event_loop::AppEvent,
    async_runtime::message::{WorkerFailure, WorkerFailureKind, WorkerMessage},
};

pub(super) fn emit_message(tx: &std_mpsc::Sender<WorkerMessage>, message: WorkerMessage) {
    if let Err(err) = tx.send(message) {
        eprintln!("[Scheduler] bridge send failed: {err}");
    }
}

pub(super) fn emit_message_and_wake(
    tx: &std_mpsc::Sender<WorkerMessage>,
    event_proxy: &EventLoopProxy<AppEvent>,
    message: WorkerMessage,
) {
    emit_message(tx, message);
    let _ = event_proxy.send_event(AppEvent::WorkerMessageReady);
}

pub(super) fn failure_from_join_error(join_error: JoinError) -> WorkerFailure {
    if join_error.is_panic() {
        let panic_payload = join_error.into_panic();
        let panic_message = panic_payload_to_string(panic_payload);
        WorkerFailure {
            kind: WorkerFailureKind::Panic,
            message: format!("worker task panicked: {panic_message}"),
        }
    } else {
        WorkerFailure {
            kind: WorkerFailureKind::JoinCancelled,
            message: format!("worker task cancelled before completion: {join_error}"),
        }
    }
}

pub(super) fn panic_payload_to_string(payload: Box<dyn Any + Send + 'static>) -> String {
    if let Some(text) = payload.downcast_ref::<&str>() {
        (*text).to_string()
    } else if let Some(text) = payload.downcast_ref::<String>() {
        text.clone()
    } else {
        "non-string panic payload".to_string()
    }
}
