use std::{path::PathBuf, sync::mpsc as std_mpsc};

use notify::{
    Event as NotifyEvent, EventKind as NotifyEventKind, RecursiveMode, Watcher, event::ModifyKind,
};

use crate::async_runtime::message::{
    FileSystemChangeKind, FileSystemEvent, WorkerEvent, WorkerEventKind, WorkerFailure,
    WorkerFailureKind, WorkerMessage, WorkerRequest, WorkerRequestPayload, WorkerResult,
    WorkerResultPayload,
};
use crate::workspace::model::WorkspaceIgnoreRules;

use super::{
    FILE_WATCH_BATCH_WINDOW, async_trace,
    emit::{emit_message, failure_from_join_error},
};

pub(super) async fn run_file_watch_request(
    request: WorkerRequest,
    worker_tx: std_mpsc::Sender<WorkerMessage>,
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
                "[Worker] completed watcher request_id={} revision={}",
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
                "[Worker] file watcher failed request_id={} revision={}",
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
                "[Worker] file watcher failed (panic/cancelled) request_id={} revision={}",
                request.request_id,
                request.revision_id
            );
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
                let mut events = Vec::new();
                extend_unique_file_events(
                    &mut events,
                    filter_file_watch_events(event, &ignore_rules),
                );

                let mut channel_disconnected = false;
                loop {
                    match notify_rx.recv_timeout(FILE_WATCH_BATCH_WINDOW) {
                        Ok(Ok(event)) => {
                            extend_unique_file_events(
                                &mut events,
                                filter_file_watch_events(event, &ignore_rules),
                            );
                        }
                        Ok(Err(err)) => return Err(format!("file watcher error: {err}")),
                        Err(std_mpsc::RecvTimeoutError::Timeout) => break,
                        Err(std_mpsc::RecvTimeoutError::Disconnected) => {
                            channel_disconnected = true;
                            break;
                        }
                    }
                }

                if !events.is_empty() {
                    emit_message(
                        worker_tx,
                        WorkerMessage::Result(WorkerResult {
                            request_id: request.request_id,
                            revision_id: request.revision_id,
                            topic: request.topic,
                            payload: WorkerResultPayload::FileSystemEvents {
                                root_path: root_path.clone(),
                                events,
                            },
                        }),
                    );
                }

                if channel_disconnected {
                    return Err("file watcher channel disconnected".to_string());
                }
            }
            Ok(Err(err)) => return Err(format!("file watcher error: {err}")),
            Err(err) => return Err(format!("file watcher channel disconnected: {err}")),
        }
    }
}

fn filter_file_watch_events(
    event: NotifyEvent,
    ignore_rules: &WorkspaceIgnoreRules,
) -> Vec<FileSystemEvent> {
    normalize_notify_event(event)
        .into_iter()
        .filter(|event| {
            !ignore_rules.should_ignore_path(&event.path)
                && event
                    .new_path
                    .as_ref()
                    .is_none_or(|path| !ignore_rules.should_ignore_path(path))
        })
        .collect()
}

pub(super) fn extend_unique_file_events(
    target: &mut Vec<FileSystemEvent>,
    incoming: impl IntoIterator<Item = FileSystemEvent>,
) {
    for event in incoming {
        if !target.contains(&event) {
            target.push(event);
        }
    }
}

pub(super) fn normalize_notify_event(event: NotifyEvent) -> Vec<FileSystemEvent> {
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
        return vec![FileSystemEvent {
            kind: FileSystemChangeKind::Rename,
            path: paths[0].clone(),
            new_path: Some(paths[1].clone()),
        }];
    }

    paths
        .into_iter()
        .map(|path| FileSystemEvent {
            kind: FileSystemChangeKind::Rename,
            path,
            new_path: None,
        })
        .collect()
}
