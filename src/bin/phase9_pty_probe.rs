use std::{
    io::Write,
    thread,
    time::{Duration, Instant},
};

use netherize_editor::{
    app::async_bridge::{AppAsyncBridge, AsyncResultRouter},
    async_runtime::{
        message::{
            RequestSpec, RequestTopic, WorkerEvent, WorkerEventKind, WorkerRequestPayload,
            WorkerResult, WorkerResultPayload,
        },
        scheduler::AsyncScheduler,
    },
};

const TERMINAL_REVISION: u64 = 1;

#[derive(Debug, Default)]
struct PtyProbeState {
    session_id: Option<u64>,
    sent_demo_input: bool,
    saw_close: bool,
}

impl PtyProbeState {
    fn submit_demo_input(&mut self, scheduler: &AsyncScheduler) -> Result<(), String> {
        if self.sent_demo_input {
            return Ok(());
        }
        let Some(session_id) = self.session_id else {
            return Ok(());
        };

        let input = "pwd\nls\necho hi from netherize pty\n";
        scheduler.submit(RequestSpec {
            revision_id: TERMINAL_REVISION,
            topic: RequestTopic::TerminalPty,
            payload: WorkerRequestPayload::WritePtyInput {
                session_id,
                input: input.to_string(),
            },
        })?;
        self.sent_demo_input = true;
        println!(
            "[Probe] submitted demo input to session={} commands={:?}",
            session_id, input
        );
        Ok(())
    }
}

impl AsyncResultRouter for PtyProbeState {
    fn current_revision_for(&self, topic: RequestTopic) -> u64 {
        match topic {
            RequestTopic::TerminalPty => TERMINAL_REVISION,
            _ => 0,
        }
    }

    fn on_worker_event(&mut self, event: WorkerEvent) {
        match event.kind {
            WorkerEventKind::Started => println!(
                "[PTY Event] started request_id={} rev={} topic={:?}",
                event.request_id, event.revision_id, event.topic
            ),
            WorkerEventKind::Completed => println!(
                "[PTY Event] completed request_id={} rev={} topic={:?}",
                event.request_id, event.revision_id, event.topic
            ),
            WorkerEventKind::Failed { error } => println!(
                "[PTY Event] failed request_id={} rev={} topic={:?} kind={:?} error={}",
                event.request_id, event.revision_id, event.topic, error.kind, error.message
            ),
            WorkerEventKind::Cancelled { reason } => println!(
                "[PTY Event] cancelled request_id={} rev={} topic={:?} reason={}",
                event.request_id, event.revision_id, event.topic, reason
            ),
        }
    }

    fn on_worker_result(&mut self, result: WorkerResult) {
        match result.payload {
            WorkerResultPayload::PtySpawned {
                session_id,
                shell,
                working_dir,
            } => {
                self.session_id = Some(session_id);
                println!(
                    "[PTY] spawned session={} shell={} cwd={}",
                    session_id,
                    shell,
                    working_dir.display()
                );
            }
            WorkerResultPayload::PtyOutput { session_id, chunk } => {
                print!("[PTY:{session_id}] {chunk}");
                let _ = std::io::stdout().flush();
            }
            WorkerResultPayload::PtyInputWritten { session_id, bytes } => {
                println!("[PTY] input written session={} bytes={}", session_id, bytes);
            }
            WorkerResultPayload::PtySessionClosed {
                session_id,
                exit_status,
                reason,
            } => {
                self.saw_close = true;
                println!(
                    "[PTY] closed session={} exit_status={:?} reason={}",
                    session_id, exit_status, reason
                );
            }
            other => {
                println!("[PTY] unexpected worker payload: {:?}", other);
            }
        }
    }

    fn on_stale_result(&mut self, stale: WorkerResult) {
        println!(
            "[PTY] stale result discarded request_id={} rev={} topic={:?}",
            stale.request_id, stale.revision_id, stale.topic
        );
    }
}

fn main() {
    println!("=== Phase 9 PTY Probe: setup scheduler + bridge ===");
    let (scheduler, receiver) = AsyncScheduler::new().expect("failed to init async scheduler");
    let mut bridge = AppAsyncBridge::new(receiver);
    let mut state = PtyProbeState::default();

    let spawn_request = scheduler
        .submit(RequestSpec {
            revision_id: TERMINAL_REVISION,
            topic: RequestTopic::TerminalPty,
            payload: WorkerRequestPayload::SpawnPtyShell {
                shell: None,
                working_dir: None,
            },
        })
        .expect("submit spawn pty request failed");
    println!(
        "[Probe] spawn request submitted request_id={} topic={:?}",
        spawn_request.request_id, spawn_request.topic
    );

    // Pump bridge ~10s để nhận stream output từ shell.
    let started = Instant::now();
    while started.elapsed() < Duration::from_secs(10) {
        let stats = bridge.pump(&mut state);
        if let Err(err) = state.submit_demo_input(&scheduler) {
            eprintln!("[Probe] submit demo input failed: {err}");
        }

        if state.saw_close {
            break;
        }

        if stats.routed == 0 {
            thread::sleep(Duration::from_millis(20));
        }
    }

    if let Some(session_id) = state.session_id
        && !state.saw_close
    {
        let close_result = scheduler.submit(RequestSpec {
            revision_id: TERMINAL_REVISION,
            topic: RequestTopic::TerminalPty,
            payload: WorkerRequestPayload::ClosePtySession { session_id },
        });
        if let Err(err) = close_result {
            eprintln!("[Probe] close session {} failed: {}", session_id, err);
        }
    }

    // Drain events một đoạn ngắn để nhận status đóng session.
    let drain_started = Instant::now();
    while drain_started.elapsed() < Duration::from_millis(400) {
        let stats = bridge.pump(&mut state);
        if stats.routed == 0 {
            thread::sleep(Duration::from_millis(20));
        }
    }

    println!("=== Phase 9 PTY Probe: done ===");
}
