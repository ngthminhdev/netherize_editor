use std::{
    collections::HashMap,
    sync::{Arc, Mutex, mpsc as std_mpsc},
};

use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use winit::event_loop::EventLoopProxy;

use crate::{
    app::event_loop::AppEvent,
    async_runtime::message::{
        WorkerEvent, WorkerEventKind, WorkerFailure, WorkerFailureKind, WorkerMessage,
        WorkerRequest, WorkerRequestPayload, WorkerResult, WorkerResultPayload,
    },
};

use super::{
    LspSessionRegistry, PtySessionRegistry, SyntaxEngineCache,
    ai::execute_ai_inline_request,
    ai_jobs::{run_ai_chat_stream, run_opencode_install},
    async_trace,
    emit::{emit_message, emit_message_and_wake, failure_from_join_error},
    file_watch::run_file_watch_request,
    fzf::run_fzf_request,
    lsp::run_lsp_request,
    pty::run_pty_request,
    syntax_jobs::{execute_virtual_job, run_system_dep_install},
};

async fn detect_python_version(python_binary: Option<&std::path::Path>) -> Option<String> {
    let cmd = python_binary.and_then(|p| p.to_str()).unwrap_or("python3");
    detect_command_version(
        cmd,
        &[
            "-c",
            "import sys; print(f'{sys.version_info.major}.{sys.version_info.minor}.{sys.version_info.micro}')",
        ],
    )
    .await
}

async fn detect_command_version(cmd: &str, args: &[&str]) -> Option<String> {
    let output = tokio::time::timeout(
        std::time::Duration::from_secs(5),
        tokio::process::Command::new(cmd).args(args).output(),
    )
    .await
    .ok()?
    .ok()?;
    if output.status.success() {
        let v = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if v.is_empty() { None } else { Some(v) }
    } else {
        None
    }
}

pub(super) async fn dispatch_loop(
    mut request_rx: mpsc::UnboundedReceiver<WorkerRequest>,
    result_tx: std_mpsc::Sender<WorkerMessage>,
    event_proxy: EventLoopProxy<AppEvent>,
) {
    let pty_sessions = Arc::new(PtySessionRegistry::default());
    let lsp_sessions = Arc::new(LspSessionRegistry::default());
    let syntax_engine_cache: Arc<SyntaxEngineCache> = Arc::new(Mutex::new(HashMap::new()));
    let mut active_fzf_search: Option<tokio::task::JoinHandle<()>> = None;
    let mut active_ai_chat_cancel: Option<CancellationToken> = None;

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
                | WorkerRequestPayload::LspRenameRequest { .. }
                | WorkerRequestPayload::LspDocumentHighlightRequest { .. }
                | WorkerRequestPayload::LspDocumentSymbolsRequest { .. }
                | WorkerRequestPayload::LspFormattingRequest { .. }
                | WorkerRequestPayload::LspCompletionRequest { .. }
                | WorkerRequestPayload::LspCodeActionRequest { .. }
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
                match execute_ai_inline_request(&request, Some(&worker_tx)).await {
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
                        let kind = if message.contains("cancelled") {
                            WorkerEventKind::Cancelled { reason: message }
                        } else {
                            WorkerEventKind::Failed {
                                error: WorkerFailure {
                                    kind: WorkerFailureKind::Execution,
                                    message,
                                },
                            }
                        };
                        emit_message(
                            &worker_tx,
                            WorkerMessage::Event(WorkerEvent {
                                request_id: request.request_id,
                                revision_id: request.revision_id,
                                topic: request.topic,
                                kind,
                            }),
                        );
                    }
                }
            });
            continue;
        }

        if matches!(request.payload, WorkerRequestPayload::AiChatCancel) {
            if let Some(cancel_token) = active_ai_chat_cancel.take() {
                cancel_token.cancel();
            }
            continue;
        }

        if matches!(request.payload, WorkerRequestPayload::AiChatRequest { .. }) {
            if let Some(cancel_token) = active_ai_chat_cancel.take() {
                cancel_token.cancel();
            }
            let cancel_token = CancellationToken::new();
            active_ai_chat_cancel = Some(cancel_token.clone());
            let (
                prompt,
                cursor_position,
                history,
                active_buffer_path,
                workspace_root,
                file_refs,
                model,
                agent,
            ) = match request.payload {
                WorkerRequestPayload::AiChatRequest {
                    prompt,
                    buffer_context: _,
                    cursor_position,
                    history,
                    active_buffer_path,
                    workspace_root,
                    file_refs,
                    model,
                    agent,
                } => (
                    prompt,
                    cursor_position,
                    history,
                    active_buffer_path,
                    workspace_root,
                    file_refs,
                    model,
                    agent,
                ),
                _ => unreachable!(),
            };
            let worker_tx = result_tx.clone();
            let ai_event_proxy = event_proxy.clone();
            tokio::spawn(async move {
                run_ai_chat_stream(
                    worker_tx,
                    ai_event_proxy,
                    prompt,
                    active_buffer_path,
                    workspace_root,
                    cursor_position,
                    history,
                    file_refs,
                    model,
                    agent,
                    cancel_token,
                )
                .await;
            });
            continue;
        }

        if matches!(request.payload, WorkerRequestPayload::AiInstallRequest) {
            let worker_tx = result_tx.clone();
            let ai_event_proxy = event_proxy.clone();
            tokio::spawn(async move {
                run_opencode_install(worker_tx, ai_event_proxy).await;
            });
            continue;
        }

        if matches!(
            request.payload,
            WorkerRequestPayload::InstallSystemDeps { .. }
        ) {
            let tools = match request.payload {
                WorkerRequestPayload::InstallSystemDeps { tools } => tools,
                _ => unreachable!(),
            };
            let worker_tx = result_tx.clone();
            let install_proxy = event_proxy.clone();
            tokio::spawn(async move {
                run_system_dep_install(tools, worker_tx, install_proxy).await;
            });
            continue;
        }

        if matches!(
            request.payload,
            WorkerRequestPayload::ScanPythonEnvironments { .. }
        ) {
            let workspace_root = match request.payload {
                WorkerRequestPayload::ScanPythonEnvironments { workspace_root } => workspace_root,
                _ => unreachable!(),
            };
            let worker_tx = result_tx.clone();
            let event_proxy = event_proxy.clone();
            tokio::spawn(async move {
                let environments =
                    crate::async_runtime::python_env::scan_python_environments(&workspace_root)
                        .await;
                emit_message_and_wake(
                    &worker_tx,
                    &event_proxy,
                    WorkerMessage::Result(WorkerResult {
                        request_id: request.request_id,
                        revision_id: request.revision_id,
                        topic: request.topic,
                        payload: WorkerResultPayload::PythonEnvironmentsDiscovered(environments),
                    }),
                );
            });
            continue;
        }

        if matches!(
            request.payload,
            WorkerRequestPayload::DetectRuntimeVersions { .. }
        ) {
            let (python_binary, _workspace_root) = match request.payload {
                WorkerRequestPayload::DetectRuntimeVersions {
                    python_binary,
                    workspace_root,
                } => (python_binary, workspace_root),
                _ => unreachable!(),
            };
            let worker_tx = result_tx.clone();
            let event_proxy = event_proxy.clone();
            tokio::spawn(async move {
                let python_version = detect_python_version(python_binary.as_deref()).await;
                let node_version = detect_command_version("node", &["--version"])
                    .await
                    .map(|v| v.trim_start_matches('v').to_string());
                let go_version = detect_command_version("go", &["version"])
                    .await
                    .and_then(|v| {
                        // "go version go1.22.0 darwin/arm64" → "1.22.0"
                        v.split_whitespace()
                            .find(|s| s.starts_with("go") && s.len() > 2)
                            .map(|s| s.trim_start_matches("go").to_string())
                    });
                emit_message_and_wake(
                    &worker_tx,
                    &event_proxy,
                    WorkerMessage::Result(WorkerResult {
                        request_id: request.request_id,
                        revision_id: request.revision_id,
                        topic: request.topic,
                        payload: WorkerResultPayload::RuntimeVersionsDetected {
                            python_version,
                            node_version,
                            go_version,
                        },
                    }),
                );
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
