use std::sync::{Arc, mpsc as std_mpsc};

use serde_json::Value;
use tokio::io::AsyncBufReadExt;

use crate::{
    async_runtime::message::{
        LspProgressKindWire, RequestTopic, WorkerEvent, WorkerEventKind, WorkerFailure,
        WorkerFailureKind, WorkerMessage, WorkerResult, WorkerResultPayload,
    },
    lsp::client::{
        LspClientProcess, ProgressKind, parse_progress_notification, parse_publish_diagnostics,
        parse_server_request, parse_window_log_message, read_json_rpc_message_async,
    },
};

use super::{LspSessionRegistry, emit::emit_message};

pub(super) fn spawn_lsp_stdout_reader(
    session: Arc<LspClientProcess>,
    mut reader: tokio::io::BufReader<tokio::process::ChildStdout>,
    topic: RequestTopic,
    server_name: String,
    lsp_sessions: Arc<LspSessionRegistry>,
    worker_tx: std_mpsc::Sender<WorkerMessage>,
) -> Result<(), String> {
    tokio::spawn(async move {
        loop {
            let message = match read_json_rpc_message_async(&mut reader).await {
                Ok(Some(message)) => message,
                Ok(None) => {
                    let should_emit = lsp_sessions.clear_if_process(&session).ok().flatten();
                    if should_emit.is_some() {
                        emit_message(
                            &worker_tx,
                            WorkerMessage::Result(WorkerResult {
                                request_id: session.latest_request_id(),
                                revision_id: session.latest_revision(),
                                topic,
                                payload: WorkerResultPayload::LspServerStopped {
                                    exit_status: None,
                                    reason: "lsp stdout reached EOF".to_string(),
                                },
                            }),
                        );
                    }
                    break;
                }
                Err(err) => {
                    let should_emit = lsp_sessions.clear_if_process(&session).ok().flatten();
                    if should_emit.is_some() {
                        emit_message(
                            &worker_tx,
                            WorkerMessage::Event(WorkerEvent {
                                request_id: session.latest_request_id(),
                                revision_id: session.latest_revision(),
                                topic,
                                kind: WorkerEventKind::Failed {
                                    error: WorkerFailure {
                                        kind: WorkerFailureKind::Execution,
                                        message: format!("lsp stdout reader failed: {err}"),
                                    },
                                },
                            }),
                        );
                    }
                    break;
                }
            };

            if let Some(parsed) = parse_publish_diagnostics(&message) {
                let revision_id = parsed.version.unwrap_or_else(|| session.latest_revision());
                emit_message(
                    &worker_tx,
                    WorkerMessage::Result(WorkerResult {
                        request_id: session.latest_request_id(),
                        revision_id,
                        topic,
                        payload: WorkerResultPayload::LspDiagnostics {
                            uri: parsed.uri,
                            version: parsed.version,
                            diagnostics: parsed.diagnostics,
                        },
                    }),
                );
                continue;
            }

            if let Some(parsed) = parse_window_log_message(&message) {
                emit_message(
                    &worker_tx,
                    WorkerMessage::Result(WorkerResult {
                        request_id: session.latest_request_id(),
                        revision_id: session.latest_revision(),
                        topic,
                        payload: WorkerResultPayload::LspLogMessage {
                            level: parsed.level,
                            message: parsed.message,
                        },
                    }),
                );
                continue;
            }

            if let Some(progress) = parse_progress_notification(&message) {
                let kind = match progress.kind {
                    ProgressKind::Begin => LspProgressKindWire::Begin,
                    ProgressKind::Report => LspProgressKindWire::Report,
                    ProgressKind::End => LspProgressKindWire::End,
                };
                emit_message(
                    &worker_tx,
                    WorkerMessage::Result(WorkerResult {
                        request_id: session.latest_request_id(),
                        revision_id: session.latest_revision(),
                        topic,
                        payload: WorkerResultPayload::LspProgress {
                            server_name: server_name.clone(),
                            token: progress.token,
                            kind,
                            title: progress.title,
                            message: progress.message,
                            percentage: progress.percentage,
                        },
                    }),
                );
                continue;
            }

            // A server → client request is a JSON-RPC payload that has BOTH an
            // `id` and a `method`. Plain responses only have `id` (and `result`
            // or `error`), and notifications only have `method`. Differentiating
            // here lets us auto-acknowledge requests like
            // `window/workDoneProgress/create` while still routing real
            // responses through the pending-response map.
            if let Some(server_request) = parse_server_request(&message) {
                if matches!(
                    server_request.method.as_str(),
                    "window/workDoneProgress/create"
                        | "client/registerCapability"
                        | "client/unregisterCapability"
                ) {
                    if let Err(err) = session
                        .send_response_async(server_request.id, Value::Null)
                        .await
                    {
                        eprintln!(
                            "[LSP] reply to {} (id={}) failed: {err}",
                            server_request.method, server_request.id
                        );
                    }
                }
                continue;
            }

            if let Some(id) = message.get("id").and_then(|value| value.as_u64()) {
                session.deliver_response(id, message.clone());
                continue;
            }

            let method = message
                .get("method")
                .and_then(Value::as_str)
                .unwrap_or("unknown")
                .to_string();
            if method != "unknown" {
                emit_message(
                    &worker_tx,
                    WorkerMessage::Result(WorkerResult {
                        request_id: session.latest_request_id(),
                        revision_id: session.latest_revision(),
                        topic,
                        payload: WorkerResultPayload::LspAck {
                            action: format!("notification:{method}"),
                            uri: None,
                            version: None,
                        },
                    }),
                );
            }
        }
    });
    Ok(())
}

pub(super) fn spawn_lsp_stderr_logger(
    mut stderr: tokio::io::BufReader<tokio::process::ChildStderr>,
    server_name: String,
) -> Result<(), String> {
    tokio::spawn(async move {
        let mut line = String::new();
        loop {
            line.clear();
            match stderr.read_line(&mut line).await {
                Ok(0) => break,
                Ok(_) => {
                    let line = line.trim_end();
                    if !line.is_empty() {
                        eprintln!("[LSP:{server_name}:stderr] {line}");
                    }
                }
                Err(err) => {
                    eprintln!("[LSP:{server_name}:stderr] read failed: {err}");
                    break;
                }
            }
        }
    });
    Ok(())
}
