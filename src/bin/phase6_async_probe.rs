use std::{
    path::PathBuf,
    thread,
    time::{Duration, Instant},
};

use netherize_editor::{
    app::{
        async_bridge::{AppAsyncBridge, AsyncResultRouter, BridgePumpStats},
        revision_state::RevisionState,
    },
    async_runtime::{
        message::{
            RequestSpec, RequestTopic, WorkerEvent, WorkerEventKind, WorkerRequestPayload,
            WorkerResult, WorkerResultPayload,
        },
        scheduler::AsyncScheduler,
    },
    syntax::syntax_engine::LanguageId,
};

#[derive(Debug, Default)]
struct ProbeAppState {
    revisions: RevisionState,
    accepted_results: Vec<String>,
    stale_results: Vec<String>,
    failed_events: Vec<String>,
}

impl ProbeAppState {
    fn bump_revision(&mut self, topic: RequestTopic) -> u64 {
        self.revisions.bump(topic)
    }

    fn current_revision(&self, topic: RequestTopic) -> u64 {
        self.revisions.current(topic)
    }
}

impl AsyncResultRouter for ProbeAppState {
    fn current_revision_for(&self, topic: RequestTopic) -> u64 {
        self.current_revision(topic)
    }

    fn on_worker_event(&mut self, event: WorkerEvent) {
        match event.kind {
            WorkerEventKind::Started => println!(
                "[App] lifecycle started request_id={} revision={} topic={:?}",
                event.request_id, event.revision_id, event.topic
            ),
            WorkerEventKind::Completed => println!(
                "[App] lifecycle completed request_id={} revision={} topic={:?}",
                event.request_id, event.revision_id, event.topic
            ),
            WorkerEventKind::Failed { error } => {
                let line = format!(
                    "lifecycle failed request_id={} revision={} topic={:?} kind={:?} error={}",
                    event.request_id, event.revision_id, event.topic, error.kind, error.message
                );
                println!("[App] {line}");
                self.failed_events.push(line);
            }
            WorkerEventKind::Cancelled { reason } => println!(
                "[App] lifecycle cancelled request_id={} revision={} topic={:?} reason={}",
                event.request_id, event.revision_id, event.topic, reason
            ),
        }
    }

    fn on_worker_result(&mut self, result: WorkerResult) {
        let summary = match result.payload {
            WorkerResultPayload::ParseAndHighlight {
                file_path,
                language_id,
                spans,
                line_count,
                char_count,
                byte_count,
                parse_time_ms,
                highlight_time_ms,
            } => {
                let target = file_path
                    .as_ref()
                    .map(|path| path.display().to_string())
                    .unwrap_or_else(|| "<active-buffer>".to_string());
                format!(
                    "syntax accepted request_id={} revision={} lang={} file={} lines={} chars={} bytes={} spans={} parse_ms={} highlight_ms={}",
                    result.request_id,
                    result.revision_id,
                    language_id.as_str(),
                    target,
                    line_count,
                    char_count,
                    byte_count,
                    spans.len(),
                    parse_time_ms,
                    highlight_time_ms
                )
            }
            WorkerResultPayload::ParseSummary {
                file_path,
                line_count,
                char_count,
            } => format!(
                "parse accepted request_id={} revision={} file={} lines={} chars={}",
                result.request_id,
                result.revision_id,
                file_path.display(),
                line_count,
                char_count
            ),
            WorkerResultPayload::SearchMatches { query, matches } => format!(
                "search accepted request_id={} revision={} query={:?} matches={}",
                result.request_id,
                result.revision_id,
                query,
                matches.len()
            ),
            WorkerResultPayload::CpuBurnSummary {
                job_label,
                busy_millis,
                checksum,
            } => format!(
                "cpu burn accepted request_id={} revision={} label={} busy_ms={} checksum={:#x}",
                result.request_id, result.revision_id, job_label, busy_millis, checksum
            ),
            WorkerResultPayload::FileSystemEvents { root_path, events } => format!(
                "watch accepted request_id={} revision={} root={} events={}",
                result.request_id,
                result.revision_id,
                root_path.display(),
                events.len()
            ),
            WorkerResultPayload::PtySpawned {
                session_id,
                shell,
                working_dir,
            } => format!(
                "pty spawned request_id={} revision={} session={} shell={} cwd={}",
                result.request_id,
                result.revision_id,
                session_id,
                shell,
                working_dir.display()
            ),
            WorkerResultPayload::PtyOutput { session_id, chunk } => format!(
                "pty output request_id={} revision={} session={} bytes={}",
                result.request_id,
                result.revision_id,
                session_id,
                chunk.len()
            ),
            WorkerResultPayload::PtyInputWritten { session_id, bytes } => format!(
                "pty input written request_id={} revision={} session={} bytes={}",
                result.request_id, result.revision_id, session_id, bytes
            ),
            WorkerResultPayload::PtySessionClosed {
                session_id,
                exit_status,
                reason,
            } => format!(
                "pty closed request_id={} revision={} session={} exit={:?} reason={}",
                result.request_id, result.revision_id, session_id, exit_status, reason
            ),
            WorkerResultPayload::LspServerStarted {
                server_name,
                root_path,
                capabilities_summary,
            } => format!(
                "lsp started request_id={} revision={} server={} root={} capabilities={}",
                result.request_id,
                result.revision_id,
                server_name,
                root_path.display(),
                capabilities_summary
            ),
            WorkerResultPayload::LspServerStopped {
                exit_status,
                reason,
            } => format!(
                "lsp stopped request_id={} revision={} exit={:?} reason={}",
                result.request_id, result.revision_id, exit_status, reason
            ),
            WorkerResultPayload::LspAck {
                action,
                uri,
                version,
            } => format!(
                "lsp ack request_id={} revision={} action={} uri={:?} version={:?}",
                result.request_id, result.revision_id, action, uri, version
            ),
            WorkerResultPayload::LspDiagnostics {
                uri,
                version,
                diagnostics,
            } => format!(
                "lsp diagnostics request_id={} revision={} uri={} version={:?} count={}",
                result.request_id,
                result.revision_id,
                uri,
                version,
                diagnostics.len()
            ),
            WorkerResultPayload::LspLogMessage { level, message } => format!(
                "lsp log request_id={} revision={} level={} message={}",
                result.request_id, result.revision_id, level, message
            ),
        };
        println!("[App] {summary}");
        self.accepted_results.push(summary);
    }

    fn on_stale_result(&mut self, stale: WorkerResult) {
        let summary = format!(
            "stale result request_id={} revision={} topic={:?}",
            stale.request_id, stale.revision_id, stale.topic
        );
        println!("[App] {summary}");
        self.stale_results.push(summary);
    }
}

fn main() {
    let target_path = std::env::args()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("phase3_slice_demo.txt"));

    let text_snapshot = std::fs::read_to_string(&target_path)
        .unwrap_or_else(|_| "Fallback text snapshot for async probe.\nLine 2".to_string());

    println!("Phase 6 probe target file: {}", target_path.display());
    println!("=== Setup scheduler + bridge ===");

    let (scheduler, receiver) = AsyncScheduler::new().expect("failed to init scheduler");
    let mut bridge = AppAsyncBridge::new(receiver);
    let mut app_state = ProbeAppState::default();

    println!("=== Submit requests ===");

    // Request A: revision 1 -> sẽ stale khi request B revision 2 đã được observe.
    let rev1 = app_state.bump_revision(RequestTopic::ActiveBufferLayout);
    let _req1 = scheduler
        .submit(RequestSpec {
            revision_id: rev1,
            topic: RequestTopic::ActiveBufferLayout,
            payload: WorkerRequestPayload::ParseAndHighlight {
                file_path: Some(target_path.clone()),
                text_snapshot: text_snapshot.clone(),
                language_id: LanguageId::Rust,
            },
        })
        .expect("submit rev1 failed");

    // Request B: revision 2 -> accepted, request A sẽ bị stale.
    let rev2 = app_state.bump_revision(RequestTopic::ActiveBufferLayout);
    let _req2 = scheduler
        .submit(RequestSpec {
            revision_id: rev2,
            topic: RequestTopic::ActiveBufferLayout,
            payload: WorkerRequestPayload::ParseAndHighlight {
                file_path: Some(target_path.clone()),
                text_snapshot: text_snapshot.clone(),
                language_id: LanguageId::Rust,
            },
        })
        .expect("submit rev2 failed");

    // Search request (success).
    let search_rev = app_state.bump_revision(RequestTopic::ProjectSearch);
    let _search_req = scheduler
        .submit(RequestSpec {
            revision_id: search_rev,
            topic: RequestTopic::ProjectSearch,
            payload: WorkerRequestPayload::MockSearch {
                query: "netherize".to_string(),
                simulated_delay_ms: 180,
            },
        })
        .expect("submit search failed");

    // Search request (forced fail) để thấy lifecycle failed.
    let search_fail_rev = app_state.bump_revision(RequestTopic::ProjectSearch);
    let _search_fail_req = scheduler
        .submit(RequestSpec {
            revision_id: search_fail_rev,
            topic: RequestTopic::ProjectSearch,
            payload: WorkerRequestPayload::MockSearch {
                query: "fail".to_string(),
                simulated_delay_ms: 80,
            },
        })
        .expect("submit fail search failed");

    // CPU-bound demo request: mô phỏng task nặng không block UI thread.
    let cpu_demo_rev = app_state.bump_revision(RequestTopic::BackgroundDemo);
    let _cpu_demo_req = scheduler
        .submit(RequestSpec {
            revision_id: cpu_demo_rev,
            topic: RequestTopic::BackgroundDemo,
            payload: WorkerRequestPayload::MockCpuBurn {
                job_label: "phase6-probe-cpu".to_string(),
                busy_millis: 700,
            },
        })
        .expect("submit cpu demo failed");

    // Panic demo request: kiểm tra panic từ worker được bridge nhận thành Failed event.
    let _panic_demo_req = scheduler
        .submit(RequestSpec {
            revision_id: cpu_demo_rev,
            topic: RequestTopic::BackgroundDemo,
            payload: WorkerRequestPayload::MockPanic {
                reason: "phase6 forced panic".to_string(),
            },
        })
        .expect("submit panic demo failed");

    println!("=== Pump bridge ===");
    let mut bridge_totals = BridgePumpStats::default();
    let started = Instant::now();
    while started.elapsed() < Duration::from_secs(3) {
        let stats = bridge.pump(&mut app_state);
        bridge_totals.routed += stats.routed;
        bridge_totals.accepted += stats.accepted;
        bridge_totals.stale += stats.stale;
        bridge_totals.failed += stats.failed;
        bridge_totals.started += stats.started;
        bridge_totals.completed += stats.completed;
        bridge_totals.cancelled += stats.cancelled;
        bridge_totals.disconnected |= stats.disconnected;

        if stats.routed == 0 {
            thread::sleep(Duration::from_millis(20));
        }
    }

    println!("=== Summary ===");
    println!(
        "{:<12} {:>8}\n{:<12} {:>8}\n{:<12} {:>8}",
        "Accepted",
        app_state.accepted_results.len(),
        "Stale",
        app_state.stale_results.len(),
        "Failed",
        app_state.failed_events.len()
    );
    println!(
        "BridgeTotals: routed={} started={} completed={} failed_events={} cancelled={} disconnected={}",
        bridge_totals.routed,
        bridge_totals.started,
        bridge_totals.completed,
        bridge_totals.failed,
        bridge_totals.cancelled,
        bridge_totals.disconnected
    );
    println!("LatestRevisions={:?}", app_state.revisions.snapshot());
}
