use std::{path::Path, process::ExitStatus, sync::mpsc as std_mpsc};

use winit::event_loop::EventLoopProxy;

use crate::{
    app::event_loop::AppEvent,
    async_runtime::message::{
        FilePreviewLine, FzfResultItem, FzfSearchMode, WorkerEvent, WorkerEventKind, WorkerFailure,
        WorkerFailureKind, WorkerMessage, WorkerRequest, WorkerRequestPayload, WorkerResult,
        WorkerResultPayload,
    },
    workspace::model::WorkspaceIgnoreRules,
};

use super::emit::{emit_message, emit_message_and_wake};

const MAX_FZF_RESULTS: usize = 48;

pub(super) async fn run_fzf_request(
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

    match execute_fzf_async(&request).await {
        Ok(payload) => {
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
        Err(message) => {
            emit_message_and_wake(
                &worker_tx,
                &event_proxy,
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
}

async fn execute_fzf_async(request: &WorkerRequest) -> Result<WorkerResultPayload, String> {
    let WorkerRequestPayload::FzfSearch {
        query,
        mode,
        workspace_root,
        case_sensitive,
    } = &request.payload
    else {
        return Err("fzf runner received non-fzf payload".to_string());
    };

    if query.trim().is_empty() {
        return Ok(WorkerResultPayload::FzfResults {
            query: query.clone(),
            mode: *mode,
            case_sensitive: *case_sensitive,
            items: Vec::new(),
        });
    }

    let items = match mode {
        FzfSearchMode::FindFile => fzf_find_file(query, workspace_root).await?,
        FzfSearchMode::LiveGrep => fzf_live_grep(query, workspace_root, *case_sensitive).await?,
    };

    Ok(WorkerResultPayload::FzfResults {
        query: query.clone(),
        mode: *mode,
        case_sensitive: *case_sensitive,
        items,
    })
}

async fn fzf_find_file(query: &str, workspace_root: &Path) -> Result<Vec<FzfResultItem>, String> {
    let script = build_fzf_find_file_script();
    let fzf_output = run_fzf_shell(&script, query, workspace_root, "rg-files|fzf").await?;

    Ok(String::from_utf8_lossy(&fzf_output.stdout)
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
        .collect())
}

async fn fzf_live_grep(
    query: &str,
    workspace_root: &Path,
    case_sensitive: bool,
) -> Result<Vec<FzfResultItem>, String> {
    let script = build_fzf_live_grep_script(case_sensitive);
    let fzf_output = run_fzf_shell(&script, query, workspace_root, "rg|fzf").await?;

    Ok(String::from_utf8_lossy(&fzf_output.stdout)
        .lines()
        .take(MAX_FZF_RESULTS)
        .filter_map(|line| parse_rg_fzf_line(line, workspace_root))
        .collect())
}

async fn run_fzf_shell(
    script: &str,
    query: &str,
    workspace_root: &Path,
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
        .map_err(|err| format!("{label} failed: {err}"))?;

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

pub(super) fn build_fzf_find_file_script() -> String {
    let glob_args = build_ripgrep_ignore_glob_args();
    if glob_args.is_empty() {
        "rg --files --hidden | fzf -f \"$1\"".to_string()
    } else {
        format!("rg --files --hidden {glob_args} | fzf -f \"$1\"")
    }
}

pub(super) fn build_fzf_live_grep_script(case_sensitive: bool) -> String {
    let glob_args = build_ripgrep_ignore_glob_args();
    let rg_case_arg = if case_sensitive {
        "--case-sensitive"
    } else {
        "--ignore-case"
    };
    let fzf_case_arg = if case_sensitive { "+i" } else { "-i" };
    if glob_args.is_empty() {
        format!("rg --line-number --column --hidden --fixed-strings {rg_case_arg} -- \"$1\" . | fzf {fzf_case_arg} -f \"$1\"")
    } else {
        format!("rg --line-number --column --hidden --fixed-strings {rg_case_arg} {glob_args} -- \"$1\" . | fzf {fzf_case_arg} -f \"$1\"")
    }
}

fn build_ripgrep_ignore_glob_args() -> String {
    WorkspaceIgnoreRules::default()
        .ignored_directory_names()
        .map(|name| format!("--glob {}", sh_single_quote(&format!("!**/{name}/**"))))
        .collect::<Vec<_>>()
        .join(" ")
}

fn parse_rg_fzf_line(line: &str, workspace_root: &Path) -> Option<FzfResultItem> {
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

pub(super) fn build_file_preview_lines(
    file_path: &Path,
    max_lines: usize,
    target_line: Option<usize>,
) -> Vec<FilePreviewLine> {
    let text = std::fs::read_to_string(file_path).unwrap_or_default();
    let lines: Vec<&str> = text.lines().collect();
    if lines.is_empty() || max_lines == 0 {
        return Vec::new();
    }

    let total_lines = lines.len();
    let (start, end, target_idx) = if let Some(target_line) = target_line {
        let target_idx = target_line
            .saturating_sub(1)
            .min(total_lines.saturating_sub(1));
        let half_window = max_lines / 2;
        let mut start = target_idx.saturating_sub(half_window);
        let end = (start + max_lines).min(total_lines);
        if end - start < max_lines {
            start = end.saturating_sub(max_lines);
        }
        (start, end, Some(target_idx))
    } else {
        (0, max_lines.min(total_lines), None)
    };

    lines[start..end]
        .iter()
        .enumerate()
        .map(|(offset, line)| {
            let idx = start + offset;
            FilePreviewLine {
                line_number: idx + 1,
                text: (*line).to_string(),
                is_target: target_idx.is_some_and(|target| idx == target),
            }
        })
        .collect()
}
