use std::{
    fs,
    path::PathBuf,
    thread,
    time::{Duration, Instant},
};

use netherize_editor::{
    app::{
        async_bridge::{AppAsyncBridge, AsyncResultRouter},
        revision_state::RevisionState,
    },
    async_runtime::{
        message::{
            RequestSpec, RequestTopic, WorkerEvent, WorkerEventKind, WorkerRequestPayload,
            WorkerResult, WorkerResultPayload,
        },
        scheduler::AsyncScheduler,
    },
    lsp::client::path_to_lsp_uri,
};

const INITIAL_ERROR_TEXT: &str = "fn main() {\n    let value = ;\n}\n";
const FIXED_TEXT: &str = "fn main() {\n    let value = 42;\n    println!(\"{}\", value);\n}\n";

#[derive(Debug, Default)]
struct LspProbeState {
    revisions: RevisionState,
    started: bool,
    stopped: bool,
    diagnostics_total: usize,
    diagnostics_non_empty: usize,
    did_change_ack: bool,
    did_close_ack: bool,
    failed_events: Vec<String>,
}

impl LspProbeState {
    fn set_revision(&mut self, topic: RequestTopic, revision: u64) {
        self.revisions.observe(topic, revision);
    }
}

impl AsyncResultRouter for LspProbeState {
    fn current_revision_for(&self, topic: RequestTopic) -> u64 {
        self.revisions.current(topic)
    }

    fn on_worker_event(&mut self, event: WorkerEvent) {
        match event.kind {
            WorkerEventKind::Started => println!(
                "[LSP Probe] started request_id={} rev={} topic={:?}",
                event.request_id, event.revision_id, event.topic
            ),
            WorkerEventKind::Completed => println!(
                "[LSP Probe] completed request_id={} rev={} topic={:?}",
                event.request_id, event.revision_id, event.topic
            ),
            WorkerEventKind::Failed { error } => {
                let line = format!(
                    "failed request_id={} rev={} topic={:?} kind={:?} error={}",
                    event.request_id, event.revision_id, event.topic, error.kind, error.message
                );
                eprintln!("[LSP Probe] {line}");
                self.failed_events.push(line);
            }
            WorkerEventKind::Cancelled { reason } => println!(
                "[LSP Probe] cancelled request_id={} rev={} topic={:?} reason={}",
                event.request_id, event.revision_id, event.topic, reason
            ),
        }
    }

    fn on_worker_result(&mut self, result: WorkerResult) {
        match result.payload {
            WorkerResultPayload::LspServerStarted {
                server_name,
                root_path,
                capabilities_summary,
            } => {
                self.started = true;
                println!(
                    "[LSP Probe] server started name={} root={} capabilities={}",
                    server_name,
                    root_path.display(),
                    capabilities_summary
                );
            }
            WorkerResultPayload::LspAck {
                action,
                uri,
                version,
            } => {
                if action == "didChange" {
                    self.did_change_ack = true;
                }
                if action == "didClose" {
                    self.did_close_ack = true;
                }
                println!(
                    "[LSP Probe] ack action={} uri={:?} version={:?}",
                    action, uri, version
                );
            }
            WorkerResultPayload::LspDiagnostics {
                uri,
                version,
                diagnostics,
            } => {
                self.diagnostics_total += 1;
                if !diagnostics.is_empty() {
                    self.diagnostics_non_empty += 1;
                }

                println!(
                    "[LSP Probe] diagnostics uri={} version={:?} count={}",
                    uri,
                    version,
                    diagnostics.len()
                );
                for (idx, diag) in diagnostics.iter().enumerate().take(6) {
                    println!(
                        "  [{}] {:?}:{:?}-{:?}:{:?} severity={:?} source={:?} code={:?} message={}",
                        idx,
                        diag.range.start.line,
                        diag.range.start.character,
                        diag.range.end.line,
                        diag.range.end.character,
                        diag.severity,
                        diag.source,
                        diag.code,
                        diag.message
                    );
                }
            }
            WorkerResultPayload::LspLogMessage { level, message } => {
                println!("[LSP Probe] log level={} message={}", level, message);
            }
            WorkerResultPayload::LspServerStopped {
                exit_status,
                reason,
            } => {
                self.stopped = true;
                println!(
                    "[LSP Probe] server stopped exit_status={:?} reason={}",
                    exit_status, reason
                );
            }
            payload => {
                println!("[LSP Probe] unexpected payload: {:?}", payload);
            }
        }
    }

    fn on_stale_result(&mut self, stale: WorkerResult) {
        println!(
            "[LSP Probe] stale result discarded request_id={} rev={} topic={:?}",
            stale.request_id, stale.revision_id, stale.topic
        );
    }
}

fn submit_lsp_request(
    scheduler: &AsyncScheduler,
    state: &mut LspProbeState,
    revision_id: u64,
    payload: WorkerRequestPayload,
) -> Result<(), String> {
    state.set_revision(RequestTopic::LspClient, revision_id);
    let request = scheduler.submit(RequestSpec {
        revision_id,
        topic: RequestTopic::LspClient,
        payload,
    })?;
    println!(
        "[LSP Probe] submitted request_id={} revision={} topic={:?}",
        request.request_id, request.revision_id, request.topic
    );
    Ok(())
}

fn pump_until(
    bridge: &mut AppAsyncBridge,
    state: &mut LspProbeState,
    timeout: Duration,
    predicate: impl Fn(&LspProbeState) -> bool,
) -> bool {
    let started = Instant::now();
    while started.elapsed() < timeout {
        let stats = bridge.pump(state);
        if predicate(state) {
            return true;
        }
        if stats.routed == 0 {
            thread::sleep(Duration::from_millis(25));
        }
    }
    predicate(state)
}

fn main() {
    let target_path = std::env::args()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("phase10_lsp_sample.rs"));
    let root_path = std::env::current_dir().expect("resolve cwd");

    fs::write(&target_path, INITIAL_ERROR_TEXT).expect("write initial sample file");
    let target_abs = target_path
        .canonicalize()
        .unwrap_or_else(|_| root_path.join(&target_path));
    let uri = path_to_lsp_uri(&target_abs);

    println!("=== Phase 10 LSP Probe ===");
    println!("workspace_root={}", root_path.display());
    println!("target_file={} uri={}", target_abs.display(), uri);

    let (scheduler, receiver) = AsyncScheduler::new().expect("init async scheduler failed");
    let mut bridge = AppAsyncBridge::new(receiver);
    let mut state = LspProbeState::default();

    println!("=== 1) Start LSP server + initialize ===");
    if let Err(err) = submit_lsp_request(
        &scheduler,
        &mut state,
        1,
        WorkerRequestPayload::StartLspServer {
            root_path: root_path.clone(),
            server_command: Some("rust-analyzer".to_string()),
        },
    ) {
        eprintln!("start lsp request failed: {err}");
        return;
    }

    if !pump_until(&mut bridge, &mut state, Duration::from_secs(6), |state| {
        state.started || !state.failed_events.is_empty()
    }) {
        eprintln!("timeout waiting for initialize");
        return;
    }
    if !state.failed_events.is_empty() {
        eprintln!("initialize failed: {:?}", state.failed_events);
        return;
    }

    println!("=== 2) didOpen (invalid text) -> expect diagnostics > 0 ===");
    if let Err(err) = submit_lsp_request(
        &scheduler,
        &mut state,
        1,
        WorkerRequestPayload::LspDidOpen {
            uri: uri.clone(),
            language_id: "rust".to_string(),
            version: 1,
            text: INITIAL_ERROR_TEXT.to_string(),
        },
    ) {
        eprintln!("didOpen request failed: {err}");
        return;
    }
    let _ = pump_until(&mut bridge, &mut state, Duration::from_secs(5), |state| {
        state.diagnostics_non_empty > 0 || !state.failed_events.is_empty()
    });

    println!("=== 3) didChange (fixed text) -> expect diagnostics update ===");
    fs::write(&target_abs, FIXED_TEXT).expect("write fixed sample file");
    if let Err(err) = submit_lsp_request(
        &scheduler,
        &mut state,
        2,
        WorkerRequestPayload::LspDidChange {
            uri: uri.clone(),
            version: 2,
            text: FIXED_TEXT.to_string(),
        },
    ) {
        eprintln!("didChange request failed: {err}");
        return;
    }
    let _ = pump_until(&mut bridge, &mut state, Duration::from_secs(5), |state| {
        state.did_change_ack || !state.failed_events.is_empty()
    });

    println!("=== 4) didClose ===");
    if let Err(err) = submit_lsp_request(
        &scheduler,
        &mut state,
        3,
        WorkerRequestPayload::LspDidClose { uri: uri.clone() },
    ) {
        eprintln!("didClose request failed: {err}");
        return;
    }
    let _ = pump_until(&mut bridge, &mut state, Duration::from_secs(3), |state| {
        state.did_close_ack || !state.failed_events.is_empty()
    });

    println!("=== 5) shutdown ===");
    let _ = submit_lsp_request(
        &scheduler,
        &mut state,
        4,
        WorkerRequestPayload::StopLspServer,
    );

    let _ = pump_until(&mut bridge, &mut state, Duration::from_secs(3), |state| {
        state.stopped || !state.failed_events.is_empty()
    });

    println!("=== Summary ===");
    println!("started={}", state.started);
    println!("diagnostics_total={}", state.diagnostics_total);
    println!("diagnostics_non_empty={}", state.diagnostics_non_empty);
    println!("stopped={}", state.stopped);
    println!("failed_events={}", state.failed_events.len());
}
