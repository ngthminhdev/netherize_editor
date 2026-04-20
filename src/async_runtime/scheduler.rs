use std::{
    any::Any,
    collections::HashMap,
    io::{BufRead, BufReader, Read},
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
        mpsc as std_mpsc,
    },
    time::{Duration, Instant},
};

use notify::{
    Event as NotifyEvent, EventKind as NotifyEventKind, RecursiveMode, Watcher, event::ModifyKind,
};
use serde_json::Value;
use tokio::sync::mpsc;

use crate::async_runtime::message::{
    FileSystemChangeKind, FileSystemEvent, RequestSpec, RequestTopic, WorkerEvent, WorkerEventKind,
    WorkerFailure, WorkerFailureKind, WorkerMessage, WorkerRequest, WorkerRequestPayload,
    WorkerResult, WorkerResultPayload,
};
use crate::lsp::client::{
    build_did_change_notification, build_did_close_notification, build_did_open_notification,
    parse_publish_diagnostics, parse_window_log_message, read_json_rpc_message, spawn_lsp_server,
};
use crate::syntax::{highlight::generate_highlight_spans, syntax_engine::SyntaxEngine};
use crate::terminal::pty::{PtyProcess, PtyProvider};
use crate::workspace::model::WorkspaceIgnoreRules;

/// Runtime wrapper duy nhất cho background jobs.
/// App layer chỉ submit request qua struct này, không spawn tokio rải rác.
pub struct AsyncScheduler {
    _runtime: tokio::runtime::Runtime,
    request_tx: mpsc::UnboundedSender<WorkerRequest>,
    next_request_id: Arc<AtomicU64>,
}

#[derive(Default)]
struct PtySessionRegistry {
    next_session_id: AtomicU64,
    sessions: Mutex<HashMap<u64, Arc<PtyProcess>>>,
}

#[derive(Default)]
struct LspSessionRegistry {
    session: Mutex<Option<Arc<crate::lsp::client::LspClientProcess>>>,
}

impl LspSessionRegistry {
    fn has_active_session(&self) -> Result<bool, String> {
        let guard = self
            .session
            .lock()
            .map_err(|_| "lsp session lock poisoned".to_string())?;
        Ok(guard.is_some())
    }

    fn set(&self, session: Arc<crate::lsp::client::LspClientProcess>) -> Result<(), String> {
        let mut guard = self
            .session
            .lock()
            .map_err(|_| "lsp session lock poisoned".to_string())?;
        *guard = Some(session);
        Ok(())
    }

    fn get(&self) -> Result<Option<Arc<crate::lsp::client::LspClientProcess>>, String> {
        let guard = self
            .session
            .lock()
            .map_err(|_| "lsp session lock poisoned".to_string())?;
        Ok(guard.clone())
    }

    fn take(&self) -> Result<Option<Arc<crate::lsp::client::LspClientProcess>>, String> {
        let mut guard = self
            .session
            .lock()
            .map_err(|_| "lsp session lock poisoned".to_string())?;
        Ok(guard.take())
    }
}

impl PtySessionRegistry {
    fn alloc_session_id(&self) -> u64 {
        self.next_session_id.fetch_add(1, Ordering::Relaxed) + 1
    }

    fn insert(&self, session_id: u64, process: Arc<PtyProcess>) -> Result<(), String> {
        let mut sessions = self
            .sessions
            .lock()
            .map_err(|_| "pty sessions lock poisoned".to_string())?;
        sessions.insert(session_id, process);
        Ok(())
    }

    fn get(&self, session_id: u64) -> Result<Option<Arc<PtyProcess>>, String> {
        let sessions = self
            .sessions
            .lock()
            .map_err(|_| "pty sessions lock poisoned".to_string())?;
        Ok(sessions.get(&session_id).cloned())
    }

    fn remove(&self, session_id: u64) -> Result<Option<Arc<PtyProcess>>, String> {
        let mut sessions = self
            .sessions
            .lock()
            .map_err(|_| "pty sessions lock poisoned".to_string())?;
        Ok(sessions.remove(&session_id))
    }
}

impl AsyncScheduler {
    pub fn new() -> Result<(Self, std_mpsc::Receiver<WorkerMessage>), String> {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .enable_time()
            .worker_threads(2)
            .thread_name("netherize-worker")
            .build()
            .map_err(|err| format!("build tokio runtime failed: {err}"))?;

        let (request_tx, request_rx) = mpsc::unbounded_channel();
        let (result_tx, result_rx) = std_mpsc::channel();

        runtime.spawn(dispatch_loop(request_rx, result_tx));

        let scheduler = Self {
            _runtime: runtime,
            request_tx,
            next_request_id: Arc::new(AtomicU64::new(1)),
        };
        Ok((scheduler, result_rx))
    }

    pub fn submit(&self, spec: RequestSpec) -> Result<WorkerRequest, String> {
        let request_id = self.next_request_id.fetch_add(1, Ordering::Relaxed);
        let request = WorkerRequest {
            request_id,
            revision_id: spec.revision_id,
            topic: spec.topic,
            payload: spec.payload,
        };

        println!(
            "[Scheduler] enqueue request_id={} revision={} topic={:?}",
            request.request_id, request.revision_id, request.topic
        );

        self.request_tx
            .send(request.clone())
            .map_err(|err| format!("submit request failed: {err}"))?;

        Ok(request)
    }
}

async fn dispatch_loop(
    mut request_rx: mpsc::UnboundedReceiver<WorkerRequest>,
    result_tx: std_mpsc::Sender<WorkerMessage>,
) {
    let pty_sessions = Arc::new(PtySessionRegistry::default());
    let lsp_sessions = Arc::new(LspSessionRegistry::default());

    while let Some(request) = request_rx.recv().await {
        println!(
            "[Scheduler] dispatch request_id={} revision={} topic={:?}",
            request.request_id, request.revision_id, request.topic
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
                | WorkerRequestPayload::WritePtyInput { .. }
                | WorkerRequestPayload::ClosePtySession { .. }
        ) {
            let worker_tx = result_tx.clone();
            let pty_sessions = pty_sessions.clone();
            tokio::spawn(async move {
                run_pty_request(request, pty_sessions, worker_tx).await;
            });
            continue;
        }

        if matches!(
            request.payload,
            WorkerRequestPayload::StartLspServer { .. }
                | WorkerRequestPayload::LspDidOpen { .. }
                | WorkerRequestPayload::LspDidChange { .. }
                | WorkerRequestPayload::LspDidClose { .. }
                | WorkerRequestPayload::StopLspServer
        ) {
            let worker_tx = result_tx.clone();
            let lsp_sessions = lsp_sessions.clone();
            tokio::spawn(async move {
                run_lsp_request(request, lsp_sessions, worker_tx).await;
            });
            continue;
        }

        let worker_tx = result_tx.clone();
        tokio::spawn(async move {
            let started = WorkerEvent {
                request_id: request.request_id,
                revision_id: request.revision_id,
                topic: request.topic,
                kind: WorkerEventKind::Started,
            };
            emit_message(&worker_tx, WorkerMessage::Event(started));
            println!(
                "[Worker] started request_id={} revision={}",
                request.request_id, request.revision_id
            );

            let job_request = request.clone();
            let worker_handle =
                tokio::spawn(async move { execute_virtual_job(&job_request).await });

            match worker_handle.await {
                Ok(Ok(payload)) => {
                    let result = WorkerResult {
                        request_id: request.request_id,
                        revision_id: request.revision_id,
                        topic: request.topic,
                        payload,
                    };
                    emit_message(&worker_tx, WorkerMessage::Result(result));

                    let completed = WorkerEvent {
                        request_id: request.request_id,
                        revision_id: request.revision_id,
                        topic: request.topic,
                        kind: WorkerEventKind::Completed,
                    };
                    emit_message(&worker_tx, WorkerMessage::Event(completed));
                    println!(
                        "[Worker] completed request_id={} revision={}",
                        request.request_id, request.revision_id
                    );
                }
                Ok(Err(message)) => {
                    let failed = WorkerEvent {
                        request_id: request.request_id,
                        revision_id: request.revision_id,
                        topic: request.topic,
                        kind: WorkerEventKind::Failed {
                            error: WorkerFailure {
                                kind: WorkerFailureKind::Execution,
                                message,
                            },
                        },
                    };
                    emit_message(&worker_tx, WorkerMessage::Event(failed));
                    println!(
                        "[Worker] failed request_id={} revision={}",
                        request.request_id, request.revision_id
                    );
                }
                Err(join_error) => {
                    let failed = WorkerEvent {
                        request_id: request.request_id,
                        revision_id: request.revision_id,
                        topic: request.topic,
                        kind: WorkerEventKind::Failed {
                            error: failure_from_join_error(join_error),
                        },
                    };
                    emit_message(&worker_tx, WorkerMessage::Event(failed));
                    println!(
                        "[Worker] failed (panic/cancelled) request_id={} revision={}",
                        request.request_id, request.revision_id
                    );
                }
            }
        });
    }
}

async fn run_file_watch_request(
    request: WorkerRequest,
    worker_tx: std_mpsc::Sender<WorkerMessage>,
) {
    let started = WorkerEvent {
        request_id: request.request_id,
        revision_id: request.revision_id,
        topic: request.topic,
        kind: WorkerEventKind::Started,
    };
    emit_message(&worker_tx, WorkerMessage::Event(started));
    println!(
        "[Worker] started watcher request_id={} revision={}",
        request.request_id, request.revision_id
    );

    let watcher_request = request.clone();
    let watcher_tx = worker_tx.clone();
    let worker_handle =
        tokio::task::spawn_blocking(move || execute_file_watch_loop(&watcher_request, &watcher_tx));

    match worker_handle.await {
        Ok(Ok(())) => {
            let completed = WorkerEvent {
                request_id: request.request_id,
                revision_id: request.revision_id,
                topic: request.topic,
                kind: WorkerEventKind::Completed,
            };
            emit_message(&worker_tx, WorkerMessage::Event(completed));
            println!(
                "[Worker] completed watcher request_id={} revision={}",
                request.request_id, request.revision_id
            );
        }
        Ok(Err(message)) => {
            let failed = WorkerEvent {
                request_id: request.request_id,
                revision_id: request.revision_id,
                topic: request.topic,
                kind: WorkerEventKind::Failed {
                    error: WorkerFailure {
                        kind: WorkerFailureKind::Execution,
                        message,
                    },
                },
            };
            emit_message(&worker_tx, WorkerMessage::Event(failed));
            println!(
                "[Worker] file watcher failed request_id={} revision={}",
                request.request_id, request.revision_id
            );
        }
        Err(join_error) => {
            let failed = WorkerEvent {
                request_id: request.request_id,
                revision_id: request.revision_id,
                topic: request.topic,
                kind: WorkerEventKind::Failed {
                    error: failure_from_join_error(join_error),
                },
            };
            emit_message(&worker_tx, WorkerMessage::Event(failed));
            println!(
                "[Worker] file watcher failed (panic/cancelled) request_id={} revision={}",
                request.request_id, request.revision_id
            );
        }
    }
}

async fn run_pty_request(
    request: WorkerRequest,
    pty_sessions: Arc<PtySessionRegistry>,
    worker_tx: std_mpsc::Sender<WorkerMessage>,
) {
    let started = WorkerEvent {
        request_id: request.request_id,
        revision_id: request.revision_id,
        topic: request.topic,
        kind: WorkerEventKind::Started,
    };
    emit_message(&worker_tx, WorkerMessage::Event(started));
    println!(
        "[Worker] started pty request_id={} revision={}",
        request.request_id, request.revision_id
    );

    let pty_request = request.clone();
    let pty_sessions_for_task = pty_sessions.clone();
    let worker_tx_for_task = worker_tx.clone();
    let worker_handle = tokio::task::spawn_blocking(move || {
        execute_pty_request(&pty_request, &pty_sessions_for_task, &worker_tx_for_task)
    });

    match worker_handle.await {
        Ok(Ok(payload)) => {
            let result = WorkerResult {
                request_id: request.request_id,
                revision_id: request.revision_id,
                topic: request.topic,
                payload,
            };
            emit_message(&worker_tx, WorkerMessage::Result(result));

            let completed = WorkerEvent {
                request_id: request.request_id,
                revision_id: request.revision_id,
                topic: request.topic,
                kind: WorkerEventKind::Completed,
            };
            emit_message(&worker_tx, WorkerMessage::Event(completed));
            println!(
                "[Worker] completed pty request_id={} revision={}",
                request.request_id, request.revision_id
            );
        }
        Ok(Err(message)) => {
            let failed = WorkerEvent {
                request_id: request.request_id,
                revision_id: request.revision_id,
                topic: request.topic,
                kind: WorkerEventKind::Failed {
                    error: WorkerFailure {
                        kind: WorkerFailureKind::Execution,
                        message,
                    },
                },
            };
            emit_message(&worker_tx, WorkerMessage::Event(failed));
            println!(
                "[Worker] failed pty request_id={} revision={}",
                request.request_id, request.revision_id
            );
        }
        Err(join_error) => {
            let failed = WorkerEvent {
                request_id: request.request_id,
                revision_id: request.revision_id,
                topic: request.topic,
                kind: WorkerEventKind::Failed {
                    error: failure_from_join_error(join_error),
                },
            };
            emit_message(&worker_tx, WorkerMessage::Event(failed));
            println!(
                "[Worker] failed (panic/cancelled) pty request_id={} revision={}",
                request.request_id, request.revision_id
            );
        }
    }
}

async fn run_lsp_request(
    request: WorkerRequest,
    lsp_sessions: Arc<LspSessionRegistry>,
    worker_tx: std_mpsc::Sender<WorkerMessage>,
) {
    let started = WorkerEvent {
        request_id: request.request_id,
        revision_id: request.revision_id,
        topic: request.topic,
        kind: WorkerEventKind::Started,
    };
    emit_message(&worker_tx, WorkerMessage::Event(started));
    println!(
        "[Worker] started lsp request_id={} revision={}",
        request.request_id, request.revision_id
    );

    let lsp_request = request.clone();
    let lsp_sessions_for_task = lsp_sessions.clone();
    let worker_tx_for_task = worker_tx.clone();
    let worker_handle = tokio::task::spawn_blocking(move || {
        execute_lsp_request(&lsp_request, &lsp_sessions_for_task, &worker_tx_for_task)
    });

    match worker_handle.await {
        Ok(Ok(payload)) => {
            let result = WorkerResult {
                request_id: request.request_id,
                revision_id: request.revision_id,
                topic: request.topic,
                payload,
            };
            emit_message(&worker_tx, WorkerMessage::Result(result));

            let completed = WorkerEvent {
                request_id: request.request_id,
                revision_id: request.revision_id,
                topic: request.topic,
                kind: WorkerEventKind::Completed,
            };
            emit_message(&worker_tx, WorkerMessage::Event(completed));
            println!(
                "[Worker] completed lsp request_id={} revision={}",
                request.request_id, request.revision_id
            );
        }
        Ok(Err(message)) => {
            let failed = WorkerEvent {
                request_id: request.request_id,
                revision_id: request.revision_id,
                topic: request.topic,
                kind: WorkerEventKind::Failed {
                    error: WorkerFailure {
                        kind: WorkerFailureKind::Execution,
                        message,
                    },
                },
            };
            emit_message(&worker_tx, WorkerMessage::Event(failed));
            println!(
                "[Worker] failed lsp request_id={} revision={}",
                request.request_id, request.revision_id
            );
        }
        Err(join_error) => {
            let failed = WorkerEvent {
                request_id: request.request_id,
                revision_id: request.revision_id,
                topic: request.topic,
                kind: WorkerEventKind::Failed {
                    error: failure_from_join_error(join_error),
                },
            };
            emit_message(&worker_tx, WorkerMessage::Event(failed));
            println!(
                "[Worker] failed (panic/cancelled) lsp request_id={} revision={}",
                request.request_id, request.revision_id
            );
        }
    }
}

fn execute_lsp_request(
    request: &WorkerRequest,
    lsp_sessions: &Arc<LspSessionRegistry>,
    worker_tx: &std_mpsc::Sender<WorkerMessage>,
) -> Result<WorkerResultPayload, String> {
    match &request.payload {
        WorkerRequestPayload::StartLspServer {
            root_path,
            server_command,
        } => {
            if lsp_sessions.has_active_session()? {
                return Err("lsp server is already running".to_string());
            }

            let spawned = spawn_lsp_server(
                server_command.as_deref(),
                root_path,
                request.request_id,
                request.revision_id,
            )?;
            let session = spawned.process.clone();
            lsp_sessions.set(session.clone())?;

            spawn_lsp_stdout_reader(
                session,
                spawned.reader,
                request.topic,
                lsp_sessions.clone(),
                worker_tx.clone(),
            )?;
            if let Some(stderr) = spawned.stderr {
                spawn_lsp_stderr_logger(stderr, spawned.server_name.clone())?;
            }

            Ok(WorkerResultPayload::LspServerStarted {
                server_name: spawned.server_name,
                root_path: spawned.root_path,
                capabilities_summary: spawned.capabilities_summary,
            })
        }
        WorkerRequestPayload::LspDidOpen {
            uri,
            language_id,
            version,
            text,
        } => {
            let Some(session) = lsp_sessions.get()? else {
                return Err("lsp didOpen rejected: server is not running".to_string());
            };
            session.update_request_meta(request.request_id, request.revision_id);
            session.send_notification(
                "textDocument/didOpen",
                build_did_open_notification(uri, language_id, *version, text),
            )?;

            Ok(WorkerResultPayload::LspAck {
                action: "didOpen".to_string(),
                uri: Some(uri.clone()),
                version: Some(*version),
            })
        }
        WorkerRequestPayload::LspDidChange { uri, version, text } => {
            let Some(session) = lsp_sessions.get()? else {
                return Err("lsp didChange rejected: server is not running".to_string());
            };
            session.update_request_meta(request.request_id, request.revision_id);
            session.send_notification(
                "textDocument/didChange",
                build_did_change_notification(uri, *version, text),
            )?;

            Ok(WorkerResultPayload::LspAck {
                action: "didChange".to_string(),
                uri: Some(uri.clone()),
                version: Some(*version),
            })
        }
        WorkerRequestPayload::LspDidClose { uri } => {
            let Some(session) = lsp_sessions.get()? else {
                return Err("lsp didClose rejected: server is not running".to_string());
            };
            session.update_request_meta(request.request_id, request.revision_id);
            session
                .send_notification("textDocument/didClose", build_did_close_notification(uri))?;

            Ok(WorkerResultPayload::LspAck {
                action: "didClose".to_string(),
                uri: Some(uri.clone()),
                version: None,
            })
        }
        WorkerRequestPayload::StopLspServer => {
            let Some(session) = lsp_sessions.take()? else {
                return Err("stop lsp rejected: no active server".to_string());
            };
            session.update_request_meta(request.request_id, request.revision_id);
            let exit_status = session.graceful_shutdown()?;
            Ok(WorkerResultPayload::LspServerStopped {
                exit_status,
                reason: "shutdown requested by app".to_string(),
            })
        }
        _ => Err("execute_lsp_request received non-lsp payload".to_string()),
    }
}

fn execute_pty_request(
    request: &WorkerRequest,
    pty_sessions: &Arc<PtySessionRegistry>,
    worker_tx: &std_mpsc::Sender<WorkerMessage>,
) -> Result<WorkerResultPayload, String> {
    match &request.payload {
        WorkerRequestPayload::SpawnPtyShell { shell, working_dir } => {
            let provider = PtyProvider::new();
            let spawned = provider.spawn_shell(shell.as_deref(), working_dir.as_deref())?;
            let session_id = pty_sessions.alloc_session_id();
            pty_sessions.insert(session_id, spawned.process.clone())?;

            spawn_pty_output_reader(
                request,
                session_id,
                spawned.reader,
                spawned.process,
                pty_sessions.clone(),
                worker_tx.clone(),
            )?;

            Ok(WorkerResultPayload::PtySpawned {
                session_id,
                shell: spawned.shell_program,
                working_dir: spawned.working_dir,
            })
        }
        WorkerRequestPayload::WritePtyInput { session_id, input } => {
            let Some(process) = pty_sessions.get(*session_id)? else {
                return Err(format!("pty session {} not found", session_id));
            };
            let bytes = process.write_input(input)?;
            Ok(WorkerResultPayload::PtyInputWritten {
                session_id: *session_id,
                bytes,
            })
        }
        WorkerRequestPayload::ClosePtySession { session_id } => {
            let Some(process) = pty_sessions.remove(*session_id)? else {
                return Err(format!("pty session {} not found", session_id));
            };
            process.close()?;
            let exit_status = process.try_wait_status()?;
            Ok(WorkerResultPayload::PtySessionClosed {
                session_id: *session_id,
                exit_status,
                reason: "close requested by app".to_string(),
            })
        }
        _ => Err("execute_pty_request received non-pty payload".to_string()),
    }
}

fn spawn_pty_output_reader(
    request: &WorkerRequest,
    session_id: u64,
    mut reader: Box<dyn Read + Send>,
    process: Arc<PtyProcess>,
    pty_sessions: Arc<PtySessionRegistry>,
    worker_tx: std_mpsc::Sender<WorkerMessage>,
) -> Result<(), String> {
    let request_id = request.request_id;
    let revision_id = request.revision_id;
    let topic = request.topic;
    std::thread::Builder::new()
        .name(format!("netherize-pty-reader-{session_id}"))
        .spawn(move || {
            let mut buffer = [0_u8; 4096];
            loop {
                match reader.read(&mut buffer) {
                    Ok(0) => {
                        let _ = pty_sessions.remove(session_id);
                        let exit_status = process.try_wait_status().unwrap_or(None);
                        let result = WorkerResult {
                            request_id,
                            revision_id,
                            topic,
                            payload: WorkerResultPayload::PtySessionClosed {
                                session_id,
                                exit_status,
                                reason: "pty stream reached EOF".to_string(),
                            },
                        };
                        emit_message(&worker_tx, WorkerMessage::Result(result));
                        break;
                    }
                    Ok(read_bytes) => {
                        let chunk = String::from_utf8_lossy(&buffer[..read_bytes]).to_string();
                        if chunk.is_empty() {
                            continue;
                        }
                        let result = WorkerResult {
                            request_id,
                            revision_id,
                            topic,
                            payload: WorkerResultPayload::PtyOutput { session_id, chunk },
                        };
                        emit_message(&worker_tx, WorkerMessage::Result(result));
                    }
                    Err(err) => {
                        let failed = WorkerEvent {
                            request_id,
                            revision_id,
                            topic,
                            kind: WorkerEventKind::Failed {
                                error: WorkerFailure {
                                    kind: WorkerFailureKind::Execution,
                                    message: format!(
                                        "pty output reader failed for session {}: {}",
                                        session_id, err
                                    ),
                                },
                            },
                        };
                        emit_message(&worker_tx, WorkerMessage::Event(failed));
                        let _ = pty_sessions.remove(session_id);
                        break;
                    }
                }
            }
        })
        .map_err(|err| format!("spawn pty output reader thread failed: {err}"))?;

    Ok(())
}

fn spawn_lsp_stdout_reader(
    session: Arc<crate::lsp::client::LspClientProcess>,
    mut reader: Box<dyn BufRead + Send>,
    topic: RequestTopic,
    lsp_sessions: Arc<LspSessionRegistry>,
    worker_tx: std_mpsc::Sender<WorkerMessage>,
) -> Result<(), String> {
    std::thread::Builder::new()
        .name("netherize-lsp-stdout".to_string())
        .spawn(move || {
            loop {
                let message = match read_json_rpc_message(&mut *reader) {
                    Ok(Some(message)) => message,
                    Ok(None) => {
                        let should_emit = lsp_sessions.take().ok().flatten().is_some();
                        if should_emit {
                            let request_id = session.latest_request_id();
                            let revision_id = session.latest_revision();
                            let result = WorkerResult {
                                request_id,
                                revision_id,
                                topic,
                                payload: WorkerResultPayload::LspServerStopped {
                                    exit_status: None,
                                    reason: "lsp stdout reached EOF".to_string(),
                                },
                            };
                            emit_message(&worker_tx, WorkerMessage::Result(result));
                        }
                        break;
                    }
                    Err(err) => {
                        let should_emit = lsp_sessions.take().ok().flatten().is_some();
                        if should_emit {
                            let failed = WorkerEvent {
                                request_id: session.latest_request_id(),
                                revision_id: session.latest_revision(),
                                topic,
                                kind: WorkerEventKind::Failed {
                                    error: WorkerFailure {
                                        kind: WorkerFailureKind::Execution,
                                        message: format!("lsp stdout reader failed: {err}"),
                                    },
                                },
                            };
                            emit_message(&worker_tx, WorkerMessage::Event(failed));
                        }
                        break;
                    }
                };

                if let Some(parsed) = parse_publish_diagnostics(&message) {
                    let revision_id = parsed.version.unwrap_or_else(|| session.latest_revision());
                    let result = WorkerResult {
                        request_id: session.latest_request_id(),
                        revision_id,
                        topic,
                        payload: WorkerResultPayload::LspDiagnostics {
                            uri: parsed.uri,
                            version: parsed.version,
                            diagnostics: parsed.diagnostics,
                        },
                    };
                    emit_message(&worker_tx, WorkerMessage::Result(result));
                    continue;
                }

                if let Some(parsed) = parse_window_log_message(&message) {
                    let result = WorkerResult {
                        request_id: session.latest_request_id(),
                        revision_id: session.latest_revision(),
                        topic,
                        payload: WorkerResultPayload::LspLogMessage {
                            level: parsed.level,
                            message: parsed.message,
                        },
                    };
                    emit_message(&worker_tx, WorkerMessage::Result(result));
                    continue;
                }

                let method = message
                    .get("method")
                    .and_then(Value::as_str)
                    .unwrap_or("unknown")
                    .to_string();
                if method != "unknown" {
                    let result = WorkerResult {
                        request_id: session.latest_request_id(),
                        revision_id: session.latest_revision(),
                        topic,
                        payload: WorkerResultPayload::LspAck {
                            action: format!("notification:{method}"),
                            uri: None,
                            version: None,
                        },
                    };
                    emit_message(&worker_tx, WorkerMessage::Result(result));
                }
            }
        })
        .map_err(|err| format!("spawn lsp stdout reader thread failed: {err}"))?;

    Ok(())
}

fn spawn_lsp_stderr_logger(
    stderr: Box<dyn Read + Send>,
    server_name: String,
) -> Result<(), String> {
    std::thread::Builder::new()
        .name("netherize-lsp-stderr".to_string())
        .spawn(move || {
            let mut reader = BufReader::new(stderr);
            let mut line = String::new();
            loop {
                line.clear();
                match reader.read_line(&mut line) {
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
        })
        .map_err(|err| format!("spawn lsp stderr logger failed: {err}"))?;
    Ok(())
}

async fn execute_virtual_job(request: &WorkerRequest) -> Result<WorkerResultPayload, String> {
    match &request.payload {
        WorkerRequestPayload::ParseAndHighlight {
            file_path,
            text_snapshot,
            language_id,
        } => {
            let file_path = file_path.clone();
            let text_snapshot = text_snapshot.clone();
            let language_id = *language_id;
            let revision_id = request.revision_id;

            tokio::task::spawn_blocking(move || {
                let line_count = text_snapshot.lines().count();
                let char_count = text_snapshot.chars().count();
                let byte_count = text_snapshot.len();

                let parse_started = Instant::now();
                let mut syntax_engine = SyntaxEngine::new(language_id)
                    .map_err(|err| format!("init syntax engine failed: {err}"))?;
                let tree = syntax_engine
                    .parse_source(&text_snapshot, revision_id)
                    .map_err(|err| format!("parse source failed: {err}"))?;
                let parse_time_ms = parse_started.elapsed().as_millis();

                let highlight_started = Instant::now();
                let spans = generate_highlight_spans(tree, &text_snapshot);
                let highlight_time_ms = highlight_started.elapsed().as_millis();
                let total_time_ms = parse_time_ms + highlight_time_ms;

                println!(
                    "[Worker] parse profile revision={} language={} bytes={} lines={} chars={} spans={} parse_ms={} highlight_ms={} total_ms={}",
                    revision_id,
                    language_id.as_str(),
                    byte_count,
                    line_count,
                    char_count,
                    spans.len(),
                    parse_time_ms,
                    highlight_time_ms,
                    total_time_ms
                );

                Ok(WorkerResultPayload::ParseAndHighlight {
                    file_path,
                    language_id,
                    spans,
                    line_count,
                    char_count,
                    byte_count,
                    parse_time_ms,
                    highlight_time_ms,
                })
            })
            .await
            .map_err(|err| format!("parse/highlight join error: {err}"))?
        }
        WorkerRequestPayload::MockParseBuffer {
            file_path,
            text_snapshot,
            simulated_delay_ms,
        } => {
            tokio::time::sleep(Duration::from_millis(*simulated_delay_ms)).await;
            let line_count = text_snapshot.lines().count();
            let char_count = text_snapshot.chars().count();
            Ok(WorkerResultPayload::ParseSummary {
                file_path: file_path.clone(),
                line_count,
                char_count,
            })
        }
        WorkerRequestPayload::MockSearch {
            query,
            simulated_delay_ms,
        } => {
            tokio::time::sleep(Duration::from_millis(*simulated_delay_ms)).await;
            if query.eq_ignore_ascii_case("fail") {
                return Err("mock search forced failure by query='fail'".to_string());
            }

            let corpus = [
                "hello apple silicon",
                "netherize editor async scheduler",
                "rust winit wgpu cosmic-text",
                "message protocol and generation id",
            ];

            let matches = corpus
                .iter()
                .filter(|line| line.contains(query))
                .map(|line| (*line).to_string())
                .collect::<Vec<_>>();

            Ok(WorkerResultPayload::SearchMatches {
                query: query.clone(),
                matches,
            })
        }
        WorkerRequestPayload::MockCpuBurn {
            job_label,
            busy_millis,
        } => {
            let label = job_label.clone();
            let millis = *busy_millis;
            let checksum = tokio::task::spawn_blocking(move || cpu_burn_checksum(millis))
                .await
                .map_err(|err| format!("cpu burn join error: {err}"))?;

            Ok(WorkerResultPayload::CpuBurnSummary {
                job_label: label,
                busy_millis: millis,
                checksum,
            })
        }
        WorkerRequestPayload::MockPanic { reason } => {
            panic!("mock worker panic: {reason}");
        }
        WorkerRequestPayload::StartFileWatch { .. } => {
            Err("StartFileWatch request should be handled by dedicated watch loop".to_string())
        }
        WorkerRequestPayload::SpawnPtyShell { .. }
        | WorkerRequestPayload::WritePtyInput { .. }
        | WorkerRequestPayload::ClosePtySession { .. } => {
            Err("PTY request should be handled by dedicated PTY runner".to_string())
        }
        WorkerRequestPayload::StartLspServer { .. }
        | WorkerRequestPayload::LspDidOpen { .. }
        | WorkerRequestPayload::LspDidChange { .. }
        | WorkerRequestPayload::LspDidClose { .. }
        | WorkerRequestPayload::StopLspServer => {
            Err("LSP request should be handled by dedicated LSP runner".to_string())
        }
    }
}

fn execute_file_watch_loop(
    request: &WorkerRequest,
    worker_tx: &std_mpsc::Sender<WorkerMessage>,
) -> Result<(), String> {
    let WorkerRequestPayload::StartFileWatch { root_path } = &request.payload else {
        return Err("file watch loop received non-watch payload".to_string());
    };

    let root_path = root_path.clone();
    let ignore_rules = WorkspaceIgnoreRules::default();
    let (notify_tx, notify_rx) = std_mpsc::channel::<notify::Result<NotifyEvent>>();

    let mut watcher = notify::recommended_watcher(move |result| {
        let _ = notify_tx.send(result);
    })
    .map_err(|err| format!("create file watcher failed: {err}"))?;

    watcher
        .watch(&root_path, RecursiveMode::Recursive)
        .map_err(|err| format!("watch {:?} failed: {err}", root_path))?;

    println!(
        "[Worker] file watcher active request_id={} root={}",
        request.request_id,
        root_path.display()
    );

    loop {
        match notify_rx.recv() {
            Ok(Ok(event)) => {
                let events = normalize_notify_event(event)
                    .into_iter()
                    .filter(|event| {
                        !ignore_rules.should_ignore_path(&event.path)
                            && event
                                .new_path
                                .as_ref()
                                .map_or(true, |path| !ignore_rules.should_ignore_path(path))
                    })
                    .collect::<Vec<_>>();
                if events.is_empty() {
                    continue;
                }

                let result = WorkerResult {
                    request_id: request.request_id,
                    revision_id: request.revision_id,
                    topic: request.topic,
                    payload: WorkerResultPayload::FileSystemEvents {
                        root_path: root_path.clone(),
                        events,
                    },
                };
                emit_message(worker_tx, WorkerMessage::Result(result));
            }
            Ok(Err(err)) => return Err(format!("file watcher error: {err}")),
            Err(err) => return Err(format!("file watcher channel disconnected: {err}")),
        }
    }
}

fn normalize_notify_event(event: NotifyEvent) -> Vec<FileSystemEvent> {
    match event.kind {
        NotifyEventKind::Create(_) => event
            .paths
            .into_iter()
            .map(|path| FileSystemEvent {
                kind: FileSystemChangeKind::Create,
                path,
                new_path: None,
            })
            .collect(),
        NotifyEventKind::Remove(_) => event
            .paths
            .into_iter()
            .map(|path| FileSystemEvent {
                kind: FileSystemChangeKind::Delete,
                path,
                new_path: None,
            })
            .collect(),
        NotifyEventKind::Modify(ModifyKind::Name(_)) => normalize_rename_event(event.paths),
        NotifyEventKind::Modify(_) => event
            .paths
            .into_iter()
            .map(|path| FileSystemEvent {
                kind: FileSystemChangeKind::Modify,
                path,
                new_path: None,
            })
            .collect(),
        _ => Vec::new(),
    }
}

fn normalize_rename_event(paths: Vec<PathBuf>) -> Vec<FileSystemEvent> {
    if paths.len() >= 2 {
        let old_path = paths[0].clone();
        let new_path = paths[1].clone();
        return vec![FileSystemEvent {
            kind: FileSystemChangeKind::Rename,
            path: old_path,
            new_path: Some(new_path),
        }];
    }

    paths
        .into_iter()
        .map(|path| FileSystemEvent {
            // macOS/FSEvents có thể emit rename chỉ với một path (From hoặc To).
            // Vẫn coi đây là Rename để app layer trigger workspace rescan.
            kind: FileSystemChangeKind::Rename,
            path,
            new_path: None,
        })
        .collect()
}

fn cpu_burn_checksum(busy_millis: u64) -> u64 {
    let started = Instant::now();
    let budget = Duration::from_millis(busy_millis);

    // Vòng lặp deterministic để mô phỏng tác vụ CPU-bound vài trăm ms -> vài giây.
    let mut state = 0x9E37_79B9_7F4A_7C15_u64;
    let mut checksum = 0u64;

    while started.elapsed() < budget {
        state = state
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        checksum ^= state.rotate_left((state & 31) as u32);
    }

    checksum
}

fn emit_message(tx: &std_mpsc::Sender<WorkerMessage>, message: WorkerMessage) {
    if let Err(err) = tx.send(message) {
        eprintln!("[Scheduler] bridge send failed: {err}");
    }
}

fn failure_from_join_error(join_error: tokio::task::JoinError) -> WorkerFailure {
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

fn panic_payload_to_string(payload: Box<dyn Any + Send + 'static>) -> String {
    if let Some(text) = payload.downcast_ref::<&str>() {
        (*text).to_string()
    } else if let Some(text) = payload.downcast_ref::<String>() {
        text.clone()
    } else {
        "non-string panic payload".to_string()
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use notify::{Event as NotifyEvent, EventKind as NotifyEventKind, event::ModifyKind};

    use crate::async_runtime::{message::FileSystemChangeKind, scheduler::normalize_notify_event};

    #[test]
    fn normalize_create_event_maps_to_internal_create() {
        let raw = NotifyEvent {
            kind: NotifyEventKind::Create(notify::event::CreateKind::File),
            paths: vec![PathBuf::from("/tmp/a.rs")],
            attrs: Default::default(),
        };
        let mapped = normalize_notify_event(raw);

        assert_eq!(mapped.len(), 1);
        assert_eq!(mapped[0].kind, FileSystemChangeKind::Create);
        assert_eq!(mapped[0].path, PathBuf::from("/tmp/a.rs"));
    }

    #[test]
    fn normalize_rename_event_maps_old_and_new_paths() {
        let raw = NotifyEvent {
            kind: NotifyEventKind::Modify(ModifyKind::Name(notify::event::RenameMode::Both)),
            paths: vec![PathBuf::from("/tmp/old.rs"), PathBuf::from("/tmp/new.rs")],
            attrs: Default::default(),
        };
        let mapped = normalize_notify_event(raw);

        assert_eq!(mapped.len(), 1);
        assert_eq!(mapped[0].kind, FileSystemChangeKind::Rename);
        assert_eq!(mapped[0].path, PathBuf::from("/tmp/old.rs"));
        assert_eq!(mapped[0].new_path, Some(PathBuf::from("/tmp/new.rs")));
    }

    #[test]
    fn normalize_single_path_rename_still_maps_to_rename() {
        let raw = NotifyEvent {
            kind: NotifyEventKind::Modify(ModifyKind::Name(notify::event::RenameMode::From)),
            paths: vec![PathBuf::from("/tmp/old.rs")],
            attrs: Default::default(),
        };
        let mapped = normalize_notify_event(raw);

        assert_eq!(mapped.len(), 1);
        assert_eq!(mapped[0].kind, FileSystemChangeKind::Rename);
        assert_eq!(mapped[0].path, PathBuf::from("/tmp/old.rs"));
        assert_eq!(mapped[0].new_path, None);
    }
}
