use std::{
    collections::HashMap,
    sync::{Arc, Mutex, mpsc as std_mpsc},
};

use tokio::sync::mpsc;
use winit::event_loop::EventLoopProxy;

use crate::{
    app::event_loop::AppEvent,
    async_runtime::message::{
        WorkerEvent, WorkerEventKind, WorkerFailure, WorkerFailureKind, WorkerMessage,
        WorkerRequest, WorkerRequestPayload, WorkerResult,
    },
};

use super::{
    LspSessionRegistry, PtySessionRegistry, SyntaxEngineCache,
    ai::execute_ai_inline_request,
    async_trace,
    emit::{emit_message, emit_message_and_wake, failure_from_join_error},
    file_watch::run_file_watch_request,
    fzf::run_fzf_request,
    local_history::run_local_history_request,
    lsp::run_lsp_request,
    pty::run_pty_request,
    syntax_jobs::execute_virtual_job,
};

pub(super) async fn dispatch_loop(
    mut request_rx: mpsc::UnboundedReceiver<WorkerRequest>,
    result_tx: std_mpsc::Sender<WorkerMessage>,
    event_proxy: EventLoopProxy<AppEvent>,
) {
    let pty_sessions = Arc::new(PtySessionRegistry::default());
    let lsp_sessions = Arc::new(LspSessionRegistry::default());
    let syntax_engine_cache: Arc<SyntaxEngineCache> = Arc::new(Mutex::new(HashMap::new()));
    let mut active_fzf_search: Option<tokio::task::JoinHandle<()>> = None;

    while let Some(request) = request_rx.recv().await {
        async_trace!(
            "[Scheduler] dispatch request_id={} revision={} topic={:?}",
            request.request_id,
            request.revision_id,
            request.topic
        );

        if matches!(request.payload, WorkerRequestPayload::StartFileWatch { .. }) {
            let worker_tx = result_tx.clone();
            tokio::spawn(async move {
                run_file_watch_request(request, worker_tx).await;
            });
            continue;
        }

        if matches!(
            request.payload,
            WorkerRequestPayload::SpawnPtyShell { .. }
                | WorkerRequestPayload::SpawnPtyCommand { .. }
                | WorkerRequestPayload::SpawnDetachedShellCommand { .. }
                | WorkerRequestPayload::WritePtyInput { .. }
                | WorkerRequestPayload::ResizePtySession { .. }
                | WorkerRequestPayload::ClosePtySession { .. }
        ) {
            let worker_tx = result_tx.clone();
            let pty_sessions = pty_sessions.clone();
            let event_proxy = event_proxy.clone();
            tokio::spawn(async move {
                run_pty_request(request, pty_sessions, worker_tx, event_proxy).await;
            });
            continue;
        }

        if matches!(
            request.payload,
            WorkerRequestPayload::StartLspServer { .. }
                | WorkerRequestPayload::LspDidOpen { .. }
                | WorkerRequestPayload::LspDidChange { .. }
                | WorkerRequestPayload::LspDidClose { .. }
                | WorkerRequestPayload::LspHoverRequest { .. }
                | WorkerRequestPayload::LspDefinitionRequest { .. }
                | WorkerRequestPayload::LspReferencesRequest { .. }
                | WorkerRequestPayload::LspDocumentSymbolsRequest { .. }
                | WorkerRequestPayload::LspFormattingRequest { .. }
                | WorkerRequestPayload::LspCompletionRequest { .. }
                | WorkerRequestPayload::StopLspServer
                | WorkerRequestPayload::ShutdownAllLspServers
        ) {
            let worker_tx = result_tx.clone();
            let lsp_sessions = lsp_sessions.clone();
            let event_proxy = event_proxy.clone();
            tokio::spawn(async move {
                run_lsp_request(request, lsp_sessions, worker_tx, event_proxy).await;
            });
            continue;
        }

        if matches!(request.payload, WorkerRequestPayload::FzfSearch { .. }) {
            if let Some(handle) = active_fzf_search.take() {
                handle.abort();
            }
            let worker_tx = result_tx.clone();
            let event_proxy = event_proxy.clone();
            let handle = tokio::spawn(async move {
                run_fzf_request(request, worker_tx, event_proxy).await;
            });
            active_fzf_search = Some(handle);
            continue;
        }

        if matches!(
            request.payload,
            WorkerRequestPayload::LoadLocalHistory { .. }
                | WorkerRequestPayload::SaveLocalHistory { .. }
        ) {
            let worker_tx = result_tx.clone();
            let event_proxy = event_proxy.clone();
            tokio::spawn(async move {
                run_local_history_request(request, worker_tx, event_proxy).await;
            });
            continue;
        }

        if matches!(
            request.payload,
            WorkerRequestPayload::AiInlineCompletionRequest { .. }
        ) {
            let worker_tx = result_tx.clone();
            let ai_event_proxy = event_proxy.clone();
            tokio::spawn(async move {
                emit_message(
                    &worker_tx,
                    WorkerMessage::Event(WorkerEvent {
                        request_id: request.request_id,
                        revision_id: request.revision_id,
                        topic: request.topic,
                        kind: WorkerEventKind::Started,
                    }),
                );
                match execute_ai_inline_request(&request).await {
                    Ok(payload) => {
                        emit_message(
                            &worker_tx,
                            WorkerMessage::Result(WorkerResult {
                                request_id: request.request_id,
                                revision_id: request.revision_id,
                                topic: request.topic,
                                payload,
                            }),
                        );
                        emit_message(
                            &worker_tx,
                            WorkerMessage::Event(WorkerEvent {
                                request_id: request.request_id,
                                revision_id: request.revision_id,
                                topic: request.topic,
                                kind: WorkerEventKind::Completed,
                            }),
                        );
                        let _ = ai_event_proxy.send_event(AppEvent::AiInlineReady);
                    }
                    Err(message) => {
                        emit_message(
                            &worker_tx,
                            WorkerMessage::Event(WorkerEvent {
                                request_id: request.request_id,
                                revision_id: request.revision_id,
                                topic: request.topic,
                                kind: WorkerEventKind::Failed {
                                    error: WorkerFailure {
                                        kind: WorkerFailureKind::Execution,
                                        message,
                                    },
                                },
                            }),
                        );
                    }
                }
            });
            continue;
        }

        let worker_tx = result_tx.clone();
        let syntax_cache_for_job = syntax_engine_cache.clone();
        let event_proxy = event_proxy.clone();
        tokio::spawn(async move {
            emit_message(
                &worker_tx,
                WorkerMessage::Event(WorkerEvent {
                    request_id: request.request_id,
                    revision_id: request.revision_id,
                    topic: request.topic,
                    kind: WorkerEventKind::Started,
                }),
            );
            async_trace!(
                "[Worker] started request_id={} revision={}",
                request.request_id,
                request.revision_id
            );

            let job_request = request.clone();
            let worker_handle = tokio::spawn(async move {
                execute_virtual_job(&job_request, syntax_cache_for_job).await
            });

            match worker_handle.await {
                Ok(Ok(payload)) => {
                    emit_message_and_wake(
                        &worker_tx,
                        &event_proxy,
                        WorkerMessage::Result(WorkerResult {
                            request_id: request.request_id,
                            revision_id: request.revision_id,
                            topic: request.topic,
                            payload,
                        }),
                    );
                    emit_message_and_wake(
                        &worker_tx,
                        &event_proxy,
                        WorkerMessage::Event(WorkerEvent {
                            request_id: request.request_id,
                            revision_id: request.revision_id,
                            topic: request.topic,
                            kind: WorkerEventKind::Completed,
                        }),
                    );
                    async_trace!(
                        "[Worker] completed request_id={} revision={}",
                        request.request_id,
                        request.revision_id
                    );
                }
                Ok(Err(message)) => {
                    emit_message_and_wake(
                        &worker_tx,
                        &event_proxy,
                        WorkerMessage::Event(WorkerEvent {
                            request_id: request.request_id,
                            revision_id: request.revision_id,
                            topic: request.topic,
                            kind: WorkerEventKind::Failed {
                                error: WorkerFailure {
                                    kind: WorkerFailureKind::Execution,
                                    message,
                                },
                            },
                        }),
                    );
                    async_trace!(
                        "[Worker] failed request_id={} revision={}",
                        request.request_id,
                        request.revision_id
                    );
                }
                Err(join_error) => {
                    emit_message_and_wake(
                        &worker_tx,
                        &event_proxy,
                        WorkerMessage::Event(WorkerEvent {
                            request_id: request.request_id,
                            revision_id: request.revision_id,
                            topic: request.topic,
                            kind: WorkerEventKind::Failed {
                                error: failure_from_join_error(join_error),
                            },
                        }),
                    );
                    async_trace!(
                        "[Worker] failed (panic/cancelled) request_id={} revision={}",
                        request.request_id,
                        request.revision_id
                    );
                }
            }
        });
    }
}
