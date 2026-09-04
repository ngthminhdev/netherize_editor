use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc as std_mpsc,
    },
    time::{Duration, Instant},
};

use notify::{
    Event as NotifyEvent, EventKind as NotifyEventKind, RecursiveMode, Watcher, event::ModifyKind,
};
use winit::event_loop::EventLoopProxy;

use crate::app::event_loop::AppEvent;
use crate::async_runtime::message::{
    FileSystemChangeKind, FileSystemEvent, WorkerEvent, WorkerEventKind, WorkerFailure,
    WorkerFailureKind, WorkerMessage, WorkerRequest, WorkerRequestPayload, WorkerResult,
    WorkerResultPayload,
};
use crate::workspace::model::WorkspaceIgnoreRules;

use super::{
    FILE_WATCH_BATCH_WINDOW, async_trace,
    emit::{emit_message, emit_message_and_wake, failure_from_join_error},
};

/// Sau số lần restart này, báo cho UI biết watcher đang degraded (toast).
/// Watcher KHÔNG bỏ cuộc — mất live-update tree-level là mất vĩnh viễn khả năng
/// thấy file mới do agent/IDE khác tạo, trong khi poll 3s chỉ cứu buffer đang mở.
const FILE_WATCH_DEGRADED_THRESHOLD: u32 = 5;
/// Backoff giữa các lần restart watcher: 2s, 4s, 8s, 16s, rồi cap 30s.
pub(super) fn file_watch_restart_backoff(restarts: u32) -> Duration {
    Duration::from_secs((1u64 << restarts.min(5)).clamp(2, 30))
}
/// Watcher chạy ổn định ít nhất chừng này thì coi như lần chết kế tiếp là sự cố
/// mới, reset backoff về đầu thay vì leo tiếp lên cap.
const FILE_WATCH_STABLE_RUN: Duration = Duration::from_secs(60);
/// How often an idle watcher wakes to check its stop flag.
const FILE_WATCH_STOP_POLL: Duration = Duration::from_secs(1);

/// Live watchers keyed by root. One watcher per root, ever — before this the
/// dispatch loop spawned a fresh watcher on every workspace switch and never
/// stopped the old one (four `notify-rs fsevents loop` threads after four
/// switches).
#[derive(Default)]
pub(super) struct FileWatchRegistry {
    flags: HashMap<PathBuf, Arc<AtomicBool>>,
}

impl FileWatchRegistry {
    /// `Some(flag)` when a new watcher must be spawned; `None` when this root
    /// is already watched.
    pub(super) fn start(&mut self, root: &Path) -> Option<Arc<AtomicBool>> {
        if self.flags.contains_key(root) {
            return None;
        }
        let flag = Arc::new(AtomicBool::new(false));
        self.flags.insert(root.to_path_buf(), flag.clone());
        Some(flag)
    }

    pub(super) fn stop(&mut self, root: &Path) -> bool {
        match self.flags.remove(root) {
            Some(flag) => {
                flag.store(true, Ordering::Relaxed);
                true
            }
            None => false,
        }
    }
}

pub(super) async fn run_file_watch_request(
    request: WorkerRequest,
    worker_tx: std_mpsc::Sender<WorkerMessage>,
    event_proxy: EventLoopProxy<AppEvent>,
    stop: Arc<AtomicBool>,
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

    // #4: Watcher có thể chết giữa chừng (channel disconnect, FSEvents reset).
    // Tự dựng lại với backoff lũy tiến, KHÔNG bao giờ bỏ cuộc; qua ngưỡng degraded
    // thì emit Failed một lần duy nhất để UI toast cho user biết.
    let mut restarts = 0u32;
    let mut degraded_notified = false;
    loop {
        if stop.load(Ordering::Relaxed) {
            return;
        }
        let watcher_request = request.clone();
        let watcher_tx = worker_tx.clone();
        let watcher_proxy = event_proxy.clone();
        let stop_for_loop = stop.clone();
        let run_started = Instant::now();
        let worker_handle = tokio::task::spawn_blocking(move || {
            execute_file_watch_loop(
                &watcher_request,
                &watcher_tx,
                &watcher_proxy,
                &stop_for_loop,
            )
        });

        let failure = match worker_handle.await {
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
                return;
            }
            Ok(Err(message)) => {
                async_trace!(
                    "[Worker] file watcher loop failed request_id={} attempt={} err={}",
                    request.request_id,
                    restarts,
                    message
                );
                WorkerFailure {
                    kind: WorkerFailureKind::Execution,
                    message: format!("file watcher degraded (restart #{restarts}): {message}"),
                }
            }
            Err(join_error) => {
                async_trace!(
                    "[Worker] file watcher panicked/cancelled request_id={} attempt={}",
                    request.request_id,
                    restarts
                );
                failure_from_join_error(join_error)
            }
        };

        if run_started.elapsed() >= FILE_WATCH_STABLE_RUN {
            restarts = 0;
        }
        restarts += 1;

        if restarts >= FILE_WATCH_DEGRADED_THRESHOLD && !degraded_notified {
            degraded_notified = true;
            emit_message_and_wake(
                &worker_tx,
                &event_proxy,
                WorkerMessage::Event(WorkerEvent {
                    request_id: request.request_id,
                    revision_id: request.revision_id,
                    topic: request.topic,
                    kind: WorkerEventKind::Failed { error: failure },
                }),
            );
        }

        tokio::time::sleep(file_watch_restart_backoff(restarts)).await;
    }
}

fn execute_file_watch_loop(
    request: &WorkerRequest,
    worker_tx: &std_mpsc::Sender<WorkerMessage>,
    event_proxy: &EventLoopProxy<AppEvent>,
    stop: &AtomicBool,
) -> Result<(), String> {
    let WorkerRequestPayload::StartFileWatch {
        root_path,
        recursive,
    } = &request.payload
    else {
        return Err("file watch loop received non-watch payload".to_string());
    };

    let root_path = root_path.clone();
    let recursive_mode = if *recursive {
        RecursiveMode::Recursive
    } else {
        RecursiveMode::NonRecursive
    };
    let ignore_rules = WorkspaceIgnoreRules::default();
    let (notify_tx, notify_rx) = std_mpsc::channel::<notify::Result<NotifyEvent>>();

    let mut watcher = notify::recommended_watcher(move |result| {
        let _ = notify_tx.send(result);
    })
    .map_err(|err| format!("create file watcher failed: {err}"))?;

    watcher
        .watch(&root_path, recursive_mode)
        .map_err(|err| format!("watch {:?} failed: {err}", root_path))?;

    async_trace!(
        "[Worker] file watcher active request_id={} root={}",
        request.request_id,
        root_path.display()
    );

    loop {
        let first = match notify_rx.recv_timeout(FILE_WATCH_STOP_POLL) {
            Ok(event) => event,
            Err(std_mpsc::RecvTimeoutError::Timeout) => {
                if stop.load(Ordering::Relaxed) {
                    return Ok(());
                }
                continue;
            }
            Err(err) => return Err(format!("file watcher channel disconnected: {err}")),
        };
        match first {
            Ok(event) => {
                let mut events = Vec::new();
                // HashSet dedup: một đợt git checkout/agent sửa hàng loạt có thể
                // dồn hàng nghìn event vào một batch — Vec::contains là O(n²).
                let mut seen = HashSet::new();
                extend_unique_file_events_with_seen(
                    &mut events,
                    &mut seen,
                    filter_file_watch_events(event, &ignore_rules),
                );

                let mut channel_disconnected = false;
                loop {
                    match notify_rx.recv_timeout(FILE_WATCH_BATCH_WINDOW) {
                        Ok(Ok(event)) => {
                            extend_unique_file_events_with_seen(
                                &mut events,
                                &mut seen,
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
                    // #1: PHẢI wake event loop — nếu không, FileSystemEvents nằm im
                    // trong channel cho tới khi có event khác (gõ phím/chuột) đánh thức
                    // ControlFlow::Wait. Đây là nguyên nhân "IDE không update theo gì cả".
                    emit_message_and_wake(
                        worker_tx,
                        event_proxy,
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
            Err(err) => return Err(format!("file watcher error: {err}")),
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

#[cfg(test)]
pub(super) fn extend_unique_file_events(
    target: &mut Vec<FileSystemEvent>,
    incoming: impl IntoIterator<Item = FileSystemEvent>,
) {
    let mut seen: HashSet<FileSystemEvent> = target.iter().cloned().collect();
    extend_unique_file_events_with_seen(target, &mut seen, incoming);
}

fn extend_unique_file_events_with_seen(
    target: &mut Vec<FileSystemEvent>,
    seen: &mut HashSet<FileSystemEvent>,
    incoming: impl IntoIterator<Item = FileSystemEvent>,
) {
    for event in incoming {
        if seen.insert(event.clone()) {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn start_twice_creates_one_watcher_and_stop_sets_flag() {
        let mut reg = FileWatchRegistry::default();
        let root = PathBuf::from("/tmp/ws-a");
        let flag = reg.start(&root).expect("first start registers");
        assert!(reg.start(&root).is_none(), "second start is a no-op");
        assert!(!flag.load(Ordering::Relaxed));
        assert!(reg.stop(&root));
        assert!(flag.load(Ordering::Relaxed), "stop raises the flag");
        assert!(!reg.stop(&root), "stopping twice is harmless");
        assert!(reg.start(&root).is_some(), "root can be watched again");
    }
}
