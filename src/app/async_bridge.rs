use std::sync::{
    OnceLock,
    mpsc::{Receiver, TryRecvError},
};

use crate::async_runtime::message::{
    RequestTopic, WorkerEvent, WorkerEventKind, WorkerMessage, WorkerResult,
};

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

/// App state implement trait này để bridge route message về đúng nơi.
/// Event loop chỉ gọi bridge.pump(...) tại một điểm duy nhất.
pub trait AsyncResultRouter {
    fn current_revision_for(&self, topic: RequestTopic) -> u64;
    fn on_worker_event(&mut self, event: WorkerEvent);
    fn on_worker_result(&mut self, result: WorkerResult);
    fn on_stale_result(&mut self, stale: WorkerResult);
}

#[derive(Debug, Default, Clone, Copy)]
pub struct BridgePumpStats {
    pub routed: usize,
    pub accepted: usize,
    pub stale: usize,
    pub failed: usize,
    pub started: usize,
    pub completed: usize,
    pub cancelled: usize,
    pub disconnected: bool,
}

pub struct AppAsyncBridge {
    receiver: Receiver<WorkerMessage>,
}

impl AppAsyncBridge {
    pub fn new(receiver: Receiver<WorkerMessage>) -> Self {
        Self { receiver }
    }

    /// Pump channel non-blocking và route vào app state.
    /// Trả về thống kê routing để app loop theo dõi Accepted/Stale/Failed.
    pub fn pump<R: AsyncResultRouter>(&mut self, router: &mut R) -> BridgePumpStats {
        let mut stats = BridgePumpStats::default();

        loop {
            match self.receiver.try_recv() {
                Ok(message) => {
                    stats.routed += 1;
                    match message {
                        WorkerMessage::Event(event) => {
                            match &event.kind {
                                WorkerEventKind::Started => {
                                    stats.started += 1;
                                }
                                WorkerEventKind::Completed => {
                                    stats.completed += 1;
                                }
                                WorkerEventKind::Failed { .. } => {
                                    stats.failed += 1;
                                }
                                WorkerEventKind::Cancelled { .. } => {
                                    stats.cancelled += 1;
                                }
                            }
                            async_trace!(
                                "[Bridge] event request_id={} revision={} topic={:?} kind={:?}",
                                event.request_id,
                                event.revision_id,
                                event.topic,
                                event.kind
                            );
                            router.on_worker_event(event);
                        }
                        WorkerMessage::Result(result) => {
                            let latest_revision = router.current_revision_for(result.topic);
                            if result.revision_id < latest_revision {
                                async_trace!(
                                    "[Bridge] stale result discarded request_id={} revision={} latest_revision={} topic={:?}",
                                    result.request_id,
                                    result.revision_id,
                                    latest_revision,
                                    result.topic
                                );
                                stats.stale += 1;
                                router.on_stale_result(result);
                            } else {
                                async_trace!(
                                    "[Bridge] accepted result request_id={} revision={} topic={:?}",
                                    result.request_id,
                                    result.revision_id,
                                    result.topic
                                );
                                stats.accepted += 1;
                                router.on_worker_result(result);
                            }
                        }
                    }
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    async_trace!("[Bridge] worker channel disconnected");
                    stats.disconnected = true;
                    break;
                }
            }
        }

        stats
    }
}

#[cfg(test)]
mod tests {
    use std::{collections::HashMap, sync::mpsc};

    use crate::async_runtime::message::{
        RequestTopic, WorkerEvent, WorkerEventKind, WorkerMessage, WorkerResult,
        WorkerResultPayload,
    };

    use super::{AppAsyncBridge, AsyncResultRouter};

    #[derive(Default)]
    struct RouterMock {
        revisions: HashMap<RequestTopic, u64>,
        accepted: Vec<u64>,
        stale: Vec<u64>,
        failed_events: usize,
    }

    impl AsyncResultRouter for RouterMock {
        fn current_revision_for(&self, topic: RequestTopic) -> u64 {
            self.revisions.get(&topic).copied().unwrap_or(0)
        }

        fn on_worker_event(&mut self, event: WorkerEvent) {
            if let WorkerEventKind::Failed { .. } = event.kind {
                self.failed_events += 1;
            }
        }

        fn on_worker_result(&mut self, result: WorkerResult) {
            self.accepted.push(result.revision_id);
        }

        fn on_stale_result(&mut self, stale: WorkerResult) {
            self.stale.push(stale.revision_id);
        }
    }

    #[test]
    fn bridge_discards_stale_result_when_old_revision_arrives_last() {
        let (tx, rx) = mpsc::channel();
        let mut bridge = AppAsyncBridge::new(rx);
        let mut router = RouterMock::default();
        router.revisions.insert(RequestTopic::ActiveBufferLayout, 2);

        // B (rev=2) đi trước -> accepted.
        tx.send(WorkerMessage::Result(WorkerResult {
            request_id: 2,
            revision_id: 2,
            topic: RequestTopic::ActiveBufferLayout,
            payload: WorkerResultPayload::ParseSummary {
                file_path: "demo.txt".into(),
                line_count: 10,
                char_count: 100,
            },
        }))
        .expect("send B");

        // A (rev=1) về sau -> stale, phải discard.
        tx.send(WorkerMessage::Result(WorkerResult {
            request_id: 1,
            revision_id: 1,
            topic: RequestTopic::ActiveBufferLayout,
            payload: WorkerResultPayload::ParseSummary {
                file_path: "demo.txt".into(),
                line_count: 8,
                char_count: 80,
            },
        }))
        .expect("send A");

        let stats = bridge.pump(&mut router);

        assert_eq!(stats.accepted, 1);
        assert_eq!(stats.stale, 1);
        assert_eq!(router.accepted, vec![2]);
        assert_eq!(router.stale, vec![1]);
    }

    #[test]
    fn bridge_counts_failed_event_in_summary() {
        let (tx, rx) = mpsc::channel();
        let mut bridge = AppAsyncBridge::new(rx);
        let mut router = RouterMock::default();

        tx.send(WorkerMessage::Event(WorkerEvent {
            request_id: 77,
            revision_id: 3,
            topic: RequestTopic::ProjectSearch,
            kind: WorkerEventKind::Failed {
                error: crate::async_runtime::message::WorkerFailure {
                    kind: crate::async_runtime::message::WorkerFailureKind::Execution,
                    message: "mock failure".to_string(),
                },
            },
        }))
        .expect("send failed event");

        let stats = bridge.pump(&mut router);
        assert_eq!(stats.failed, 1);
        assert_eq!(router.failed_events, 1);
    }
}
