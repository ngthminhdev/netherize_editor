use std::{path::PathBuf, sync::mpsc as std_mpsc};

use sha2::{Digest, Sha256};
use winit::event_loop::EventLoopProxy;

use crate::{
    app::event_loop::AppEvent,
    async_runtime::message::{
        PersistedHistoryEnvelope, RequestTopic, WorkerEvent, WorkerEventKind, WorkerFailure,
        WorkerFailureKind, WorkerMessage, WorkerRequest, WorkerRequestPayload, WorkerResult,
        WorkerResultPayload,
    },
    config::paths::user_config_root,
};

use super::{emit::emit_message, emit::emit_message_and_wake};

fn local_history_path_for_file(file_path: &std::path::Path) -> PathBuf {
    let mut hasher = Sha256::new();
    hasher.update(file_path.to_string_lossy().as_bytes());
    let hash = format!("{:x}", hasher.finalize());
    user_config_root()
        .join("history")
        .join(format!("{hash}.hist"))
}

pub(super) async fn run_local_history_request(
    request: WorkerRequest,
    worker_tx: std_mpsc::Sender<WorkerMessage>,
    event_proxy: EventLoopProxy<AppEvent>,
) {
    let request_id = request.request_id;
    let revision_id = request.revision_id;
    let topic = request.topic;
    emit_message(
        &worker_tx,
        WorkerMessage::Event(WorkerEvent {
            request_id,
            revision_id,
            topic,
            kind: WorkerEventKind::Started,
        }),
    );

    match request.payload {
        WorkerRequestPayload::LoadLocalHistory { file_path } => {
            match execute_load_local_history(file_path)
                .await
                .map(|payload| WorkerResult {
                    request_id,
                    revision_id,
                    topic,
                    payload,
                }) {
                Ok(result) => {
                    emit_message_and_wake(&worker_tx, &event_proxy, WorkerMessage::Result(result));
                    emit_message_and_wake(
                        &worker_tx,
                        &event_proxy,
                        WorkerMessage::Event(WorkerEvent {
                            request_id,
                            revision_id,
                            topic,
                            kind: WorkerEventKind::Completed,
                        }),
                    );
                }
                Err(message) => emit_local_history_failure(
                    &worker_tx,
                    &event_proxy,
                    request_id,
                    revision_id,
                    topic,
                    message,
                ),
            }
        }
        WorkerRequestPayload::SaveLocalHistory {
            file_path,
            history,
            max_bytes,
        } => match execute_save_local_history(file_path, history, max_bytes).await {
            Ok(WorkerResultPayload::LocalHistorySaved {
                bytes_written,
                trimmed_transactions,
                ..
            }) => {
                super::async_trace!(
                    "[Worker] saved local history request_id={} revision={} bytes={} trimmed={}",
                    request_id,
                    revision_id,
                    bytes_written,
                    trimmed_transactions
                );
                emit_message_and_wake(
                    &worker_tx,
                    &event_proxy,
                    WorkerMessage::Event(WorkerEvent {
                        request_id,
                        revision_id,
                        topic,
                        kind: WorkerEventKind::Completed,
                    }),
                );
            }
            Ok(_) => emit_message_and_wake(
                &worker_tx,
                &event_proxy,
                WorkerMessage::Event(WorkerEvent {
                    request_id,
                    revision_id,
                    topic,
                    kind: WorkerEventKind::Completed,
                }),
            ),
            Err(message) => emit_local_history_failure(
                &worker_tx,
                &event_proxy,
                request_id,
                revision_id,
                topic,
                message,
            ),
        },
        _ => emit_local_history_failure(
            &worker_tx,
            &event_proxy,
            request_id,
            revision_id,
            topic,
            "unsupported local history request".to_string(),
        ),
    }
}

fn emit_local_history_failure(
    worker_tx: &std_mpsc::Sender<WorkerMessage>,
    event_proxy: &EventLoopProxy<AppEvent>,
    request_id: u64,
    revision_id: u64,
    topic: RequestTopic,
    message: String,
) {
    emit_message_and_wake(
        worker_tx,
        event_proxy,
        WorkerMessage::Event(WorkerEvent {
            request_id,
            revision_id,
            topic,
            kind: WorkerEventKind::Failed {
                error: WorkerFailure {
                    kind: WorkerFailureKind::Execution,
                    message,
                },
            },
        }),
    );
}

async fn execute_load_local_history(file_path: PathBuf) -> Result<WorkerResultPayload, String> {
    let history_path = local_history_path_for_file(&file_path);
    let bytes = match tokio::fs::read(&history_path).await {
        Ok(bytes) => bytes,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            return Ok(WorkerResultPayload::LocalHistoryLoaded {
                file_path,
                history: None,
            });
        }
        Err(err) => {
            return Err(format!(
                "read local history {:?} failed: {err}",
                history_path
            ));
        }
    };

    let history = serde_json::from_slice::<PersistedHistoryEnvelope>(&bytes)
        .map_err(|err| format!("parse local history {:?} failed: {err}", history_path))?;
    Ok(WorkerResultPayload::LocalHistoryLoaded {
        file_path,
        history: Some(history),
    })
}

async fn execute_save_local_history(
    file_path: PathBuf,
    mut history: PersistedHistoryEnvelope,
    max_bytes: usize,
) -> Result<WorkerResultPayload, String> {
    let history_path = local_history_path_for_file(&file_path);
    if let Some(parent) = history_path.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(|err| format!("create local history dir {:?} failed: {err}", parent))?;
    }

    let mut trimmed_transactions = 0usize;
    let bytes = loop {
        let encoded = serde_json::to_vec(&history)
            .map_err(|err| format!("serialize local history {:?} failed: {err}", file_path))?;
        if encoded.len() <= max_bytes || history.history.undo_stack.is_empty() {
            break encoded;
        }
        history.history.undo_stack.remove(0);
        trimmed_transactions += 1;
    };

    tokio::fs::write(&history_path, &bytes)
        .await
        .map_err(|err| format!("write local history {:?} failed: {err}", history_path))?;

    Ok(WorkerResultPayload::LocalHistorySaved {
        file_path,
        bytes_written: bytes.len(),
        trimmed_transactions,
    })
}
