use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
    mpsc as std_mpsc,
};

use tokio::sync::mpsc;
use winit::event_loop::EventLoopProxy;

use crate::{
    app::event_loop::AppEvent,
    async_runtime::message::{RequestSpec, WorkerMessage, WorkerRequest},
};

use super::{async_trace, dispatch::dispatch_loop};

/// Runtime wrapper duy nhất cho background jobs.
/// App layer chỉ submit request qua struct này, không spawn tokio rải rác.
pub struct AsyncScheduler {
    _runtime: tokio::runtime::Runtime,
    request_tx: mpsc::UnboundedSender<WorkerRequest>,
    next_request_id: Arc<AtomicU64>,
}

impl AsyncScheduler {
    pub fn new(
        event_proxy: EventLoopProxy<AppEvent>,
    ) -> Result<(Self, std_mpsc::Receiver<WorkerMessage>), String> {
        let runtime = build_worker_runtime()?;
        let (request_tx, request_rx) = mpsc::unbounded_channel();
        let (result_tx, result_rx) = std_mpsc::channel();

        runtime.spawn(dispatch_loop(request_rx, result_tx, event_proxy));

        let scheduler = Self {
            _runtime: runtime,
            request_tx,
            next_request_id: Arc::new(AtomicU64::new(1)),
        };
        Ok((scheduler, result_rx))
    }

    #[cfg(test)]
    pub fn new_for_tests() -> Result<(Self, std_mpsc::Receiver<WorkerMessage>), String> {
        let runtime = build_worker_runtime()?;
        let (request_tx, mut request_rx) = mpsc::unbounded_channel();
        let (result_tx, result_rx) = std_mpsc::channel();

        runtime.spawn(async move { while request_rx.recv().await.is_some() {} });
        runtime.spawn(async move {
            let _hold_sender = result_tx;
            std::future::pending::<()>().await;
        });

        let scheduler = Self {
            _runtime: runtime,
            request_tx,
            next_request_id: Arc::new(AtomicU64::new(1)),
        };
        Ok((scheduler, result_rx))
    }

    /// Stop the worker runtime (window closed). Cancels the dispatch loop —
    /// dropping its registries kills PTY children and raises every watcher's
    /// stop flag — and waits up to `timeout` for blocking threads to exit.
    pub fn shutdown(self, timeout: std::time::Duration) {
        self._runtime.shutdown_timeout(timeout);
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

pub(super) fn build_worker_runtime() -> Result<tokio::runtime::Runtime, String> {
    tokio::runtime::Builder::new_multi_thread()
        .enable_time()
        .enable_io()
        .worker_threads(2)
        .thread_name("netherize-worker")
        .build()
        .map_err(|err| format!("build tokio runtime failed: {err}"))
}
