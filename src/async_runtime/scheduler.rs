use std::{
    any::Any,
    collections::HashMap,
    io::{BufRead, BufReader, Read},
    ops::Range,
    path::PathBuf,
    process::ExitStatus,
    sync::{
        Arc, Mutex, OnceLock,
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
    FileSystemChangeKind, FileSystemEvent, FzfResultItem, FzfSearchMode, RequestSpec, RequestTopic,
    WorkerEvent, WorkerEventKind, WorkerFailure, WorkerFailureKind, WorkerMessage, WorkerRequest,
    WorkerRequestPayload, WorkerResult, WorkerResultPayload,
};
use crate::lsp::client::{
    build_did_change_notification, build_did_close_notification, build_did_open_notification,
    parse_publish_diagnostics, parse_window_log_message, read_json_rpc_message, spawn_lsp_server,
};
use crate::syntax::{
    highlight::{generate_highlight_spans, generate_highlight_spans_in_byte_window},
    syntax_engine::SyntaxEngine,
};
use crate::terminal::pty::{PtyProcess, PtyProvider};
use crate::workspace::model::WorkspaceIgnoreRules;

fn async_trace_enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| {
        std::env::var("NETHERIZE_ASYNC_TRACE")
            .map(|value| {
                matches!(
                    value.trim().to_ascii_lowercase().as_str(),
                    "1" | "true" | "yes" | "on"
                )
            })
            .unwrap_or(false)
    })
}

macro_rules! async_trace {
    ($($arg:tt)*) => {
        if async_trace_enabled() {
            println!($($arg)*);
        }
    };
}

/// Runtime wrapper duy nhất cho background jobs.
/// App layer chỉ submit request qua struct này, không spawn tokio rải rác.
pub struct AsyncScheduler {
    _runtime: tokio::runtime::Runtime,
    request_tx: mpsc::UnboundedSender<WorkerRequest>,
    next_request_id: Arc<AtomicU64>,
}

const FULL_BUFFER_HIGHLIGHT_BYTE_THRESHOLD: usize = 128 * 1024;
const FULL_BUFFER_HIGHLIGHT_LINE_THRESHOLD: usize = 1_500;
const VIEWPORT_HIGHLIGHT_OVERSCAN_MULTIPLIER: usize = 3;
const VIEWPORT_HIGHLIGHT_MIN_OVERSCAN_LINES: usize = 48;

#[derive(Default)]
struct PtySessionRegistry {
    next_session_id: AtomicU64,
    sessions: Mutex<HashMap<u64, Arc<PtyProcess>>>,
}

#[derive(Default)]
struct LspSessionRegistry {
    session: Mutex<Option<LspSessionHandle>>,
}

#[derive(Clone)]
struct LspSessionHandle {
    process: Arc<crate::lsp::client::LspClientProcess>,
    server_name: String,
    root_path: PathBuf,
}

impl LspSessionRegistry {
    fn replace(&self, session: LspSessionHandle) -> Result<Option<LspSessionHandle>, String> {
        let mut guard = self
            .session
            .lock()
            .map_err(|_| "lsp session lock poisoned".to_string())?;
        Ok(guard.replace(session))
    }

    fn get(&self) -> Result<Option<Arc<crate::lsp::client::LspClientProcess>>, String> {
        let guard = self
            .session
            .lock()
            .map_err(|_| "lsp session lock poisoned".to_string())?;
        Ok(guard.as_ref().map(|session| session.process.clone()))
    }

    fn take(&self) -> Result<Option<LspSessionHandle>, String> {
        let mut guard = self
            .session
            .lock()
            .map_err(|_| "lsp session lock poisoned".to_string())?;
        Ok(guard.take())
    }

    fn clear_if_process(
        &self,
        process: &Arc<crate::lsp::client::LspClientProcess>,
    ) -> Result<Option<LspSessionHandle>, String> {
        let mut guard = self
            .session
            .lock()
            .map_err(|_| "lsp session lock poisoned".to_string())?;
        if guard
            .as_ref()
            .is_some_and(|session| Arc::ptr_eq(&session.process, process))
        {
            return Ok(guard.take());
        }
        Ok(None)
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
        let runtime = build_worker_runtime()?;

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

        async_trace!(
            "[Scheduler] enqueue request_id={} revision={} topic={:?}",
            request.request_id,
            request.revision_id,
            request.topic
        );

        self.request_tx
            .send(request.clone())
            .map_err(|err| format!("submit request failed: {err}"))?;

        Ok(request)
    }
}

fn build_worker_runtime() -> Result<tokio::runtime::Runtime, String> {
    tokio::runtime::Builder::new_multi_thread()
        .enable_time()
        .enable_io()
        .worker_threads(2)
        .thread_name("netherize-worker")
        .build()
        .map_err(|err| format!("build tokio runtime failed: {err}"))
}

async fn dispatch_loop(
    mut request_rx: mpsc::UnboundedReceiver<WorkerRequest>,
    result_tx: std_mpsc::Sender<WorkerMessage>,
) {
    let pty_sessions = Arc::new(PtySessionRegistry::default());
    let lsp_sessions = Arc::new(LspSessionRegistry::default());
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
                | WorkerRequestPayload::WritePtyInput { .. }
                | WorkerRequestPayload::ResizePtySession { .. }
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

        if matches!(request.payload, WorkerRequestPayload::FzfSearch { .. }) {
            if let Some(handle) = active_fzf_search.take() {
                handle.abort();
            }
            let worker_tx = result_tx.clone();
            let handle = tokio::spawn(async move {
                run_fzf_request(request, worker_tx).await;
            });
            active_fzf_search = Some(handle);
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
            async_trace!(
                "[Worker] started request_id={} revision={}",
                request.request_id,
                request.revision_id
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
                    async_trace!(
                        "[Worker] completed request_id={} revision={}",
                        request.request_id,
                        request.revision_id
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
                    async_trace!(
                        "[Worker] failed request_id={} revision={}",
                        request.request_id,
                        request.revision_id
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
    async_trace!(
        "[Worker] started watcher request_id={} revision={}",
        request.request_id,
        request.revision_id
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
            async_trace!(
                "[Worker] completed watcher request_id={} revision={}",
                request.request_id,
                request.revision_id
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
            async_trace!(
                "[Worker] file watcher failed request_id={} revision={}",
                request.request_id,
                request.revision_id
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
            async_trace!(
                "[Worker] file watcher failed (panic/cancelled) request_id={} revision={}",
                request.request_id,
                request.revision_id
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
    async_trace!(
        "[Worker] started pty request_id={} revision={}",
        request.request_id,
        request.revision_id
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
            async_trace!(
                "[Worker] completed pty request_id={} revision={}",
                request.request_id,
                request.revision_id
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
            async_trace!(
                "[Worker] failed pty request_id={} revision={}",
                request.request_id,
                request.revision_id
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
            async_trace!(
                "[Worker] failed (panic/cancelled) pty request_id={} revision={}",
                request.request_id,
                request.revision_id
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
    async_trace!(
        "[Worker] started lsp request_id={} revision={}",
        request.request_id,
        request.revision_id
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
            async_trace!(
                "[Worker] completed lsp request_id={} revision={}",
                request.request_id,
                request.revision_id
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
            async_trace!(
                "[Worker] failed lsp request_id={} revision={}",
                request.request_id,
                request.revision_id
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
            async_trace!(
                "[Worker] failed (panic/cancelled) lsp request_id={} revision={}",
                request.request_id,
                request.revision_id
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
            let spawned = spawn_lsp_server(
                server_command.as_deref(),
                root_path,
                request.request_id,
                request.revision_id,
            )?;
            let session = spawned.process.clone();
            let previous = lsp_sessions.replace(LspSessionHandle {
                process: session.clone(),
                server_name: spawned.server_name.clone(),
                root_path: spawned.root_path.clone(),
            })?;

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
            if let Some(previous) = previous {
                eprintln!(
                    "[Worker] restarting LSP '{}' for {} -> '{}' for {}",
                    previous.server_name,
                    previous.root_path.display(),
                    spawned.server_name,
                    spawned.root_path.display()
                );
                previous
                    .process
                    .update_request_meta(request.request_id, request.revision_id);
                let _ = previous.process.graceful_shutdown();
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
            session
                .process
                .update_request_meta(request.request_id, request.revision_id);
            let exit_status = session.process.graceful_shutdown()?;
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
        WorkerRequestPayload::SpawnPtyCommand {
            program,
            args,
            working_dir,
        } => {
            let provider = PtyProvider::new();
            let spawned = provider.spawn_command(program, args, working_dir.as_deref())?;
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
        WorkerRequestPayload::ResizePtySession {
            session_id,
            cols,
            rows,
        } => {
            let Some(process) = pty_sessions.get(*session_id)? else {
                return Err(format!("pty session {} not found", session_id));
            };
            process.resize(*cols, *rows)?;
            Ok(WorkerResultPayload::PtyResized {
                session_id: *session_id,
                cols: *cols,
                rows: *rows,
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
                        let should_emit = lsp_sessions.clear_if_process(&session).ok().flatten();
                        if should_emit.is_some() {
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
                        let should_emit = lsp_sessions.clear_if_process(&session).ok().flatten();
                        if should_emit.is_some() {
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

const MAX_FZF_RESULTS: usize = 48;

async fn run_fzf_request(request: WorkerRequest, worker_tx: std_mpsc::Sender<WorkerMessage>) {
    let started = WorkerEvent {
        request_id: request.request_id,
        revision_id: request.revision_id,
        topic: request.topic,
        kind: WorkerEventKind::Started,
    };
    emit_message(&worker_tx, WorkerMessage::Event(started));

    match execute_fzf_async(&request).await {
        Ok(payload) => {
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
        }
        Err(message) => {
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
        }
    }
}

async fn execute_fzf_async(request: &WorkerRequest) -> Result<WorkerResultPayload, String> {
    let WorkerRequestPayload::FzfSearch {
        query,
        mode,
        workspace_root,
    } = &request.payload
    else {
        return Err("fzf runner received non-fzf payload".to_string());
    };

    if query.trim().is_empty() {
        return Ok(WorkerResultPayload::FzfResults {
            query: query.clone(),
            mode: *mode,
            items: Vec::new(),
        });
    }

    let items = match mode {
        FzfSearchMode::FindFile => fzf_find_file(query, workspace_root).await?,
        FzfSearchMode::LiveGrep => fzf_live_grep(query, workspace_root).await?,
    };

    Ok(WorkerResultPayload::FzfResults {
        query: query.clone(),
        mode: *mode,
        items,
    })
}

async fn fzf_find_file(
    query: &str,
    workspace_root: &std::path::Path,
) -> Result<Vec<FzfResultItem>, String> {
    let ignore_rules = WorkspaceIgnoreRules::default();
    let prune_expr = ignore_rules
        .ignored_directory_names()
        .map(|name| format!("-name {}", sh_single_quote(name)))
        .collect::<Vec<_>>()
        .join(" -o ");
    let script = if prune_expr.is_empty() {
        "find . -type f -print | fzf -f \"$1\"".to_string()
    } else {
        format!("find . \\( {prune_expr} \\) -prune -o -type f -print | fzf -f \"$1\"")
    };
    let fzf_output = run_fzf_shell(&script, query, workspace_root, "find|fzf").await?;

    let items: Vec<FzfResultItem> = String::from_utf8_lossy(&fzf_output.stdout)
        .lines()
        .take(MAX_FZF_RESULTS)
        .map(|line| {
            let rel = line.trim_start_matches("./").to_string();
            FzfResultItem {
                label: rel.clone(),
                preview: None,
                path: workspace_root.join(&rel),
                line: None,
                column: None,
            }
        })
        .collect();

    Ok(items)
}

async fn fzf_live_grep(
    query: &str,
    workspace_root: &std::path::Path,
) -> Result<Vec<FzfResultItem>, String> {
    let fzf_output = run_fzf_shell(
        "rg --line-number --column \"\" . | fzf -f \"$1\"",
        query,
        workspace_root,
        "rg|fzf",
    )
    .await?;

    let items: Vec<FzfResultItem> = String::from_utf8_lossy(&fzf_output.stdout)
        .lines()
        .take(MAX_FZF_RESULTS)
        .filter_map(|line| parse_rg_fzf_line(line, workspace_root))
        .collect();

    Ok(items)
}

async fn run_fzf_shell(
    script: &str,
    query: &str,
    workspace_root: &std::path::Path,
    label: &str,
) -> Result<std::process::Output, String> {
    use tokio::process::Command;

    let mut command = Command::new("sh");
    command.kill_on_drop(true);
    let output = command
        .args(["-lc", script, "--", query])
        .current_dir(workspace_root)
        .output()
        .await
        .map_err(|e| format!("{label} failed: {e}"))?;

    if output.status.success() || is_empty_fzf_status(output.status) {
        return Ok(output);
    }

    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    if stderr.is_empty() {
        Err(format!("{label} exited with status {}", output.status))
    } else {
        Err(format!(
            "{label} exited with status {}: {stderr}",
            output.status
        ))
    }
}

fn is_empty_fzf_status(status: ExitStatus) -> bool {
    status.code() == Some(1)
}

fn sh_single_quote(text: &str) -> String {
    format!("'{}'", text.replace('\'', "'\"'\"'"))
}

fn parse_rg_fzf_line(line: &str, workspace_root: &std::path::Path) -> Option<FzfResultItem> {
    // rg output format: path:line:col:content
    let mut parts = line.splitn(4, ':');
    let path_str = parts.next()?;
    let line_num: u32 = parts.next()?.parse().ok()?;
    let col_num: u32 = parts.next()?.parse().ok()?;
    let content = parts.next().unwrap_or("").trim();

    let rel = path_str.trim_start_matches("./");
    let label = format!("{rel}:{line_num}:{col_num}");
    let preview = (!content.is_empty()).then(|| content.to_string());

    Some(FzfResultItem {
        label,
        preview,
        path: workspace_root.join(rel),
        line: Some(line_num),
        column: Some(col_num),
    })
}

async fn run_git_blame_line(
    workspace_root: PathBuf,
    file_path: PathBuf,
    line_number: usize,
) -> Result<String, String> {
    use tokio::process::Command;

    let line_spec = format!("{line_number},{line_number}");
    let file_arg = file_path.to_string_lossy().to_string();
    let output = Command::new("git")
        .kill_on_drop(true)
        .args(["blame", "-L", &line_spec, "--porcelain", &file_arg])
        .current_dir(&workspace_root)
        .output()
        .await
        .map_err(|err| format!("git blame failed: {err}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        if stderr.is_empty() {
            return Err(format!("git blame exited with status {}", output.status));
        }
        return Err(format!(
            "git blame exited with status {}: {stderr}",
            output.status
        ));
    }

    parse_git_blame_summary(&String::from_utf8_lossy(&output.stdout))
}

fn parse_git_blame_summary(stdout: &str) -> Result<String, String> {
    let mut author: Option<String> = None;
    let mut author_time: Option<u64> = None;

    for line in stdout.lines() {
        if author.is_none()
            && let Some(value) = line.strip_prefix("author ")
        {
            author = Some(value.trim().to_string());
            continue;
        }
        if author_time.is_none()
            && let Some(value) = line.strip_prefix("author-time ")
        {
            author_time = value.trim().parse::<u64>().ok();
        }
        if author.is_some() && author_time.is_some() {
            break;
        }
    }

    let author = author.unwrap_or_else(|| "Unknown".to_string());
    let relative = author_time
        .map(format_relative_unix_time)
        .unwrap_or_else(|| "unknown time".to_string());
    Ok(format!("{author}, {relative}"))
}

fn format_relative_unix_time(timestamp: u64) -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let delta = now.saturating_sub(timestamp);

    match delta {
        0..=59 => "just now".to_string(),
        60..=3_599 => format_relative_duration(delta / 60, "minute"),
        3_600..=86_399 => format_relative_duration(delta / 3_600, "hour"),
        86_400..=604_799 => format_relative_duration(delta / 86_400, "day"),
        604_800..=2_592_000 => format_relative_duration(delta / 604_800, "week"),
        2_592_001..=31_536_000 => format_relative_duration(delta / 2_592_000, "month"),
        _ => format_relative_duration(delta / 31_536_000, "year"),
    }
}

fn format_relative_duration(value: u64, unit: &str) -> String {
    if value <= 1 {
        format!("1 {unit} ago")
    } else {
        format!("{value} {unit}s ago")
    }
}

async fn execute_virtual_job(request: &WorkerRequest) -> Result<WorkerResultPayload, String> {
    match &request.payload {
        WorkerRequestPayload::ParseAndHighlight {
            file_path,
            text_snapshot,
            language_id,
            buffer_revision,
            viewport_line_start,
            viewport_line_count,
        } => {
            let file_path = file_path.clone();
            let text_snapshot = text_snapshot.clone();
            let language_id = *language_id;
            let buffer_revision = *buffer_revision;
            let viewport_line_start = *viewport_line_start;
            let viewport_line_count = *viewport_line_count;
            let request_revision = request.revision_id;

            tokio::task::spawn_blocking(move || {
                let line_count = text_snapshot.lines().count();
                let char_count = text_snapshot.chars().count();
                let byte_count = text_snapshot.len();

                let parse_started = Instant::now();
                let mut syntax_engine = SyntaxEngine::new(language_id)
                    .map_err(|err| format!("init syntax engine failed: {err}"))?;
                let tree = syntax_engine
                    .parse_source(&text_snapshot, buffer_revision)
                    .map_err(|err| format!("parse source failed: {err}"))?;
                let parse_time_ms = parse_started.elapsed().as_millis();

                let highlight_started = Instant::now();
                let covered_byte_range = highlight_byte_window(
                    &text_snapshot,
                    viewport_line_start,
                    viewport_line_count,
                    line_count,
                    byte_count,
                );
                let spans = covered_byte_range
                    .clone()
                    .map(|window| generate_highlight_spans_in_byte_window(tree, &text_snapshot, window))
                    .unwrap_or_else(|| generate_highlight_spans(tree, &text_snapshot));
                let highlight_time_ms = highlight_started.elapsed().as_millis();
                let total_time_ms = parse_time_ms + highlight_time_ms;

                async_trace!(
                    "[Worker] parse profile request_revision={} buffer_revision={} language={} bytes={} lines={} chars={} spans={} parse_ms={} highlight_ms={} total_ms={}",
                    request_revision,
                    buffer_revision,
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
                    buffer_revision,
                    spans,
                    covered_byte_range,
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
        WorkerRequestPayload::GitBlameLine {
            workspace_root,
            file_path,
            line_number,
        } => {
            let workspace_root = workspace_root.clone();
            let file_path = file_path.clone();
            let line_number = *line_number;
            let summary =
                run_git_blame_line(workspace_root, file_path.clone(), line_number).await?;
            Ok(WorkerResultPayload::GitBlameLine {
                file_path,
                line_number,
                summary,
            })
        }
        WorkerRequestPayload::StartFileWatch { .. } => {
            Err("StartFileWatch request should be handled by dedicated watch loop".to_string())
        }
        WorkerRequestPayload::SpawnPtyShell { .. }
        | WorkerRequestPayload::SpawnPtyCommand { .. }
        | WorkerRequestPayload::WritePtyInput { .. }
        | WorkerRequestPayload::ResizePtySession { .. }
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
        WorkerRequestPayload::FzfSearch { .. } => {
            Err("FzfSearch request should be handled by dedicated fzf runner".to_string())
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

    async_trace!(
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

fn byte_range_for_line_window(
    source: &str,
    window_start_line: usize,
    window_line_count: usize,
) -> Option<Range<usize>> {
    if source.is_empty() {
        return None;
    }

    let mut line_starts = Vec::with_capacity(source.lines().count().max(1) + 1);
    line_starts.push(0);
    for (idx, byte) in source.bytes().enumerate() {
        if byte == b'\n' && idx + 1 <= source.len() {
            line_starts.push(idx + 1);
        }
    }
    if line_starts.is_empty() {
        return None;
    }

    let total_lines = line_starts.len();
    let clamped_start = window_start_line.min(total_lines.saturating_sub(1));
    let line_count = window_line_count.max(1);
    let end_line_exclusive = clamped_start.saturating_add(line_count).min(total_lines);

    let start = line_starts[clamped_start];
    let end = if end_line_exclusive < total_lines {
        line_starts[end_line_exclusive]
    } else {
        source.len()
    };

    (start < end).then_some(start..end)
}

fn highlight_byte_window(
    source: &str,
    viewport_line_start: usize,
    viewport_line_count: usize,
    line_count: usize,
    byte_count: usize,
) -> Option<Range<usize>> {
    if source.is_empty() {
        return None;
    }

    if should_highlight_full_buffer(line_count, byte_count) {
        return None;
    }

    let visible_lines = viewport_line_count.max(1);
    let overscan = visible_lines
        .saturating_mul(VIEWPORT_HIGHLIGHT_OVERSCAN_MULTIPLIER)
        .max(VIEWPORT_HIGHLIGHT_MIN_OVERSCAN_LINES);
    let window_line_count = visible_lines.saturating_add(overscan.saturating_mul(2));
    let window_start_line = viewport_line_start.saturating_sub(overscan);
    byte_range_for_line_window(source, window_start_line, window_line_count)
}

fn should_highlight_full_buffer(line_count: usize, byte_count: usize) -> bool {
    line_count <= FULL_BUFFER_HIGHLIGHT_LINE_THRESHOLD
        && byte_count <= FULL_BUFFER_HIGHLIGHT_BYTE_THRESHOLD
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

    use crate::async_runtime::{
        message::FileSystemChangeKind,
        scheduler::{build_worker_runtime, normalize_notify_event, parse_git_blame_summary},
    };

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

    #[test]
    fn worker_runtime_enables_io_for_tokio_process() {
        let runtime = build_worker_runtime().expect("worker runtime");
        let output = runtime
            .block_on(async {
                tokio::process::Command::new("sh")
                    .args(["-lc", "printf ok"])
                    .output()
                    .await
            })
            .expect("spawn process");

        assert!(output.status.success());
        assert_eq!(String::from_utf8_lossy(&output.stdout), "ok");
    }

    #[test]
    fn parse_git_blame_summary_extracts_author_and_relative_time() {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let stdout = format!(
            "deadbeef 10 10 1\nauthor Jane Dev\nauthor-time {}\nsummary hello\n",
            now.saturating_sub(7_200)
        );

        let summary = parse_git_blame_summary(&stdout).expect("parse blame summary");

        assert!(summary.starts_with("Jane Dev, "));
        assert!(summary.ends_with("ago"));
    }
}
