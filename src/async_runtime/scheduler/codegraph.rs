//! Code Graph HUD worker job — spawns the external `codegraph` CLI and builds
//! the renderable model off the UI thread. Mirrors the structure of `fzf.rs`.

use std::{path::Path, process::Output, sync::mpsc as std_mpsc};

use winit::event_loop::EventLoopProxy;

use crate::{
    app::event_loop::AppEvent,
    async_runtime::message::{
        WorkerEvent, WorkerEventKind, WorkerMessage, WorkerRequest, WorkerRequestPayload,
        WorkerResult, WorkerResultPayload,
    },
    codegraph::{
        cli_json::{parse_callees, parse_callers, parse_impact},
        model::build_model,
    },
};

use super::emit::{emit_message, emit_message_and_wake};

const MAX_PER_SIDE: &str = "20";
const IMPACT_DEPTH: &str = "2";

pub(super) async fn run_codegraph_request(
    request: WorkerRequest,
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

    let payload = match execute(&request).await {
        Ok(payload) => payload,
        Err((not_installed, message)) => WorkerResultPayload::CodeGraphFailed {
            not_installed,
            message,
        },
    };

    emit_message_and_wake(
        &worker_tx,
        &event_proxy,
        WorkerMessage::Result(WorkerResult {
            request_id: request.request_id,
            revision_id: request.revision_id,
            topic: request.topic,
            payload,
        }),
    );
    emit_message_and_wake(
        &worker_tx,
        &event_proxy,
        WorkerMessage::Event(WorkerEvent {
            request_id: request.request_id,
            revision_id: request.revision_id,
            topic: request.topic,
            kind: WorkerEventKind::Completed,
        }),
    );
}

async fn execute(request: &WorkerRequest) -> Result<WorkerResultPayload, (bool, String)> {
    let WorkerRequestPayload::CodeGraphQuery {
        symbol,
        focal_file,
        focal_line,
        workspace_root,
    } = &request.payload
    else {
        return Err((false, "codegraph runner received wrong payload".to_string()));
    };

    // Ensure the workspace has an index. `status --json` reports
    // `{"initialized":bool,...}`. If it's missing, build the initial index
    // (this is the slow first run — the HUD shows its loading state meanwhile);
    // otherwise do a cheap incremental sync.
    let initialized = run_cg(&["status", "--json"], workspace_root)
        .await
        .map(|out| out.contains("\"initialized\":true"))
        .unwrap_or(false);
    if initialized {
        // Incremental refresh; ignore failures (a stale index is still usable).
        let _ = run_cg(&["sync"], workspace_root).await;
    } else {
        // First run in this workspace — propagate hard errors (e.g. not installed).
        run_cg(&["init"], workspace_root).await?;
    }

    let callers_out = run_cg(
        &["callers", symbol, "--json", "--limit", MAX_PER_SIDE],
        workspace_root,
    )
    .await?;
    let callees_out = run_cg(
        &["callees", symbol, "--json", "--limit", MAX_PER_SIDE],
        workspace_root,
    )
    .await?;
    let impact_out = run_cg(
        &["impact", symbol, "--json", "--depth", IMPACT_DEPTH],
        workspace_root,
    )
    .await?;

    let callers = parse_callers(&callers_out).map_err(|e| (false, e))?;
    let callees = parse_callees(&callees_out).map_err(|e| (false, e))?;
    let impact = parse_impact(&impact_out).map_err(|e| (false, e))?;

    let model = build_model(symbol, focal_file, *focal_line, &callers, &callees, &impact);
    Ok(WorkerResultPayload::CodeGraphReady { model })
}

/// Run `codegraph <args>` in the workspace, returning stdout.
/// `Err.0 == true` means the binary is not installed.
async fn run_cg(args: &[&str], cwd: &Path) -> Result<String, (bool, String)> {
    use tokio::process::Command;
    let mut command = Command::new("codegraph");
    command.kill_on_drop(true);
    let output: Output = command
        .args(args)
        .current_dir(cwd)
        .output()
        .await
        .map_err(|err| {
            let not_installed = err.kind() == std::io::ErrorKind::NotFound;
            (
                not_installed,
                format!("codegraph {}: {err}", args.first().copied().unwrap_or("")),
            )
        })?;

    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        Err((
            false,
            format!(
                "codegraph {} failed: {stderr}",
                args.first().copied().unwrap_or("")
            ),
        ))
    }
}
