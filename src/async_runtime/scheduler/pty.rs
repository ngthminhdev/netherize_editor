use std::{
    io::Read,
    process::Stdio,
    sync::{Arc, mpsc as std_mpsc},
};

use winit::event_loop::EventLoopProxy;

use crate::{
    app::event_loop::AppEvent,
    async_runtime::message::{
        WorkerEvent, WorkerEventKind, WorkerFailure, WorkerFailureKind, WorkerMessage,
        WorkerRequest, WorkerRequestPayload, WorkerResult, WorkerResultPayload,
    },
    terminal::pty::{PtyProcess, PtyProvider},
};

use super::{
    PtySessionRegistry, async_trace,
    emit::{emit_message, failure_from_join_error},
};

pub(super) async fn run_pty_request(
    request: WorkerRequest,
    pty_sessions: Arc<PtySessionRegistry>,
    worker_tx: std_mpsc::Sender<WorkerMessage>,
    event_proxy: EventLoopProxy<AppEvent>,
) {
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
        "[Worker] started pty request_id={} revision={}",
        request.request_id,
        request.revision_id
    );

    if let WorkerRequestPayload::SpawnDetachedShellCommand {
        command,
        working_dir,
    } = &request.payload
    {
        let result = (|| async {
            let login_shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/zsh".to_string());
            let mut child = tokio::process::Command::new(&login_shell);
            child.arg("-lc").arg(command);
            child.env("PATH", crate::lsp::client::patched_env_path());
            if let Some(dir) = working_dir {
                child.current_dir(dir);
            }
            child.stdin(Stdio::null());
            child.stdout(Stdio::null());
            child.stderr(Stdio::null());

            let child = child
                .spawn()
                .map_err(|err| format!("spawn detached shell command failed: {err}"))?;
            let pid = child.id();

            tokio::spawn(async move {
                let mut child = child;
                let _ = child.wait().await;
            });

            Ok::<WorkerResultPayload, String>(WorkerResultPayload::DetachedShellCommandSpawned {
                command: command.clone(),
                pid,
            })
        })()
        .await;

        match result {
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
        return;
    }

    let pty_request = request.clone();
    let pty_sessions_for_task = pty_sessions.clone();
    let worker_tx_for_task = worker_tx.clone();
    let event_proxy_for_task = event_proxy.clone();
    let worker_handle = tokio::task::spawn_blocking(move || {
        execute_pty_request(
            &pty_request,
            &pty_sessions_for_task,
            &worker_tx_for_task,
            &event_proxy_for_task,
        )
    });

    match worker_handle.await {
        Ok(Ok(payload)) => {
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
            async_trace!(
                "[Worker] completed pty request_id={} revision={}",
                request.request_id,
                request.revision_id
            );
        }
        Ok(Err(message)) => {
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
            async_trace!(
                "[Worker] failed pty request_id={} revision={}",
                request.request_id,
                request.revision_id
            );
        }
        Err(join_error) => {
            emit_message(
                &worker_tx,
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
                "[Worker] failed (panic/cancelled) pty request_id={} revision={}",
                request.request_id,
                request.revision_id
            );
        }
    }
}

fn execute_pty_request(
    request: &WorkerRequest,
    pty_sessions: &Arc<PtySessionRegistry>,
    worker_tx: &std_mpsc::Sender<WorkerMessage>,
    event_proxy: &EventLoopProxy<AppEvent>,
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
                event_proxy.clone(),
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
                event_proxy.clone(),
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
        WorkerRequestPayload::SpawnDetachedShellCommand { .. } => {
            Err("detached shell command should be handled by async PTY runner".to_string())
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
    event_proxy: EventLoopProxy<AppEvent>,
) -> Result<(), String> {
    let request_id = request.request_id;
    let revision_id = request.revision_id;
    let topic = request.topic;
    std::thread::Builder::new()
        .name(format!("netherize-pty-reader-{session_id}"))
        .spawn(move || {
            let mut buffer = [0_u8; 8192];
            loop {
                match reader.read(&mut buffer) {
                    Ok(0) => {
                        let _ = pty_sessions.remove(session_id);
                        let exit_status = process.try_wait_status().unwrap_or(None);
                        emit_message(
                            &worker_tx,
                            WorkerMessage::Result(WorkerResult {
                                request_id,
                                revision_id,
                                topic,
                                payload: WorkerResultPayload::PtySessionClosed {
                                    session_id,
                                    exit_status,
                                    reason: "pty stream reached EOF".to_string(),
                                },
                            }),
                        );
                        break;
                    }
                    Ok(read_bytes) => {
                        let chunk = buffer[..read_bytes].to_vec();
                        if chunk.is_empty() {
                            continue;
                        }
                        emit_message(
                            &worker_tx,
                            WorkerMessage::Result(WorkerResult {
                                request_id,
                                revision_id,
                                topic,
                                payload: WorkerResultPayload::PtyOutput { session_id, chunk },
                            }),
                        );
                        let _ = event_proxy.send_event(AppEvent::TerminalOutputReady);
                    }
                    Err(err) => {
                        emit_message(
                            &worker_tx,
                            WorkerMessage::Event(WorkerEvent {
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
                            }),
                        );
                        let _ = pty_sessions.remove(session_id);
                        break;
                    }
                }
            }
        })
        .map_err(|err| format!("spawn pty output reader thread failed: {err}"))?;

    Ok(())
}
