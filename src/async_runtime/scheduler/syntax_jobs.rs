use std::{
    ops::Range,
    sync::Arc,
    time::{Duration, Instant},
};

use crate::{
    async_runtime::message::{WorkerRequest, WorkerRequestPayload, WorkerResultPayload},
    syntax::{
        highlight::{generate_highlight_spans, generate_highlight_spans_in_byte_window},
        syntax_engine::SyntaxEngine,
    },
};

use super::{
    FULL_BUFFER_HIGHLIGHT_BYTE_THRESHOLD, FULL_BUFFER_HIGHLIGHT_LINE_THRESHOLD, SyntaxEngineCache,
    VIEWPORT_HIGHLIGHT_MIN_OVERSCAN_LINES, VIEWPORT_HIGHLIGHT_OVERSCAN_MULTIPLIER, async_trace,
};

pub(super) async fn execute_virtual_job(
    request: &WorkerRequest,
    syntax_cache: Arc<SyntaxEngineCache>,
) -> Result<WorkerResultPayload, String> {
    match &request.payload {
        WorkerRequestPayload::ParseAndHighlight {
            buffer_id,
            file_path,
            text_snapshot,
            language_id,
            buffer_revision,
            viewport_line_start,
            viewport_line_count,
            edit_hint,
        } => {
            let buffer_id = buffer_id.clone();
            let file_path = file_path.clone();
            let text_snapshot = text_snapshot.clone();
            let language_id = *language_id;
            let buffer_revision = *buffer_revision;
            let viewport_line_start = *viewport_line_start;
            let viewport_line_count = *viewport_line_count;
            let edit_hint = *edit_hint;
            let request_revision = request.revision_id;

            tokio::task::spawn_blocking(move || {
                let line_count = text_snapshot.lines().count();
                let char_count = text_snapshot.chars().count();
                let byte_count = text_snapshot.len();

                let file_key = buffer_id.clone();
                let mut engine: SyntaxEngine = {
                    let mut guard = syntax_cache
                        .lock()
                        .map_err(|_| "syntax engine cache lock poisoned".to_string())?;
                    match guard.remove(&file_key) {
                        Some(cached) if cached.language_id() == language_id => cached,
                        _ => SyntaxEngine::new(language_id)
                            .map_err(|err| format!("init syntax engine failed: {err}"))?,
                    }
                };

                let parse_started = Instant::now();
                let tree = match edit_hint {
                    Some(hint) => engine
                        .parse_incremental(
                            &text_snapshot,
                            hint.start_byte,
                            hint.old_end_byte,
                            hint.new_end_byte,
                            buffer_revision,
                        )
                        .map_err(|err| format!("incremental parse failed: {err}"))?,
                    None => engine
                        .parse_source(&text_snapshot, buffer_revision)
                        .map_err(|err| format!("parse source failed: {err}"))?,
                };
                let parse_time_ms = parse_started.elapsed().as_millis();

                let highlight_started = Instant::now();
                let covered_byte_range = highlight_byte_window(
                    &text_snapshot,
                    viewport_line_start,
                    viewport_line_count,
                    line_count,
                    byte_count,
                );
                let spans = covered_byte_range
                    .clone()
                    .map(|window| {
                        generate_highlight_spans_in_byte_window(tree, &text_snapshot, window)
                    })
                    .unwrap_or_else(|| generate_highlight_spans(tree, &text_snapshot));
                let highlight_time_ms = highlight_started.elapsed().as_millis();
                let total_time_ms = parse_time_ms + highlight_time_ms;

                async_trace!(
                    "[Worker] parse profile request_revision={} buffer_revision={} language={} bytes={} lines={} chars={} spans={} parse_ms={} highlight_ms={} total_ms={}",
                    request_revision,
                    buffer_revision,
                    language_id.as_str(),
                    byte_count,
                    line_count,
                    char_count,
                    spans.len(),
                    parse_time_ms,
                    highlight_time_ms,
                    total_time_ms
                );

                if let Ok(mut guard) = syntax_cache.lock() {
                    guard.insert(file_key, engine);
                }

                Ok(WorkerResultPayload::ParseAndHighlight {
                    buffer_id,
                    file_path,
                    language_id,
                    buffer_revision,
                    spans,
                    covered_byte_range,
                    line_count,
                    char_count,
                    byte_count,
                    parse_time_ms,
                    highlight_time_ms,
                })
            })
            .await
            .map_err(|err| format!("parse/highlight join error: {err}"))?
        }
        WorkerRequestPayload::MockParseBuffer {
            file_path,
            text_snapshot,
            simulated_delay_ms,
        } => {
            tokio::time::sleep(Duration::from_millis(*simulated_delay_ms)).await;
            Ok(WorkerResultPayload::ParseSummary {
                file_path: file_path.clone(),
                line_count: text_snapshot.lines().count(),
                char_count: text_snapshot.chars().count(),
            })
        }
        WorkerRequestPayload::MockSearch {
            query,
            simulated_delay_ms,
        } => {
            tokio::time::sleep(Duration::from_millis(*simulated_delay_ms)).await;
            if query.eq_ignore_ascii_case("fail") {
                return Err("mock search forced failure by query='fail'".to_string());
            }

            let corpus = [
                "hello apple silicon",
                "netherize editor async scheduler",
                "rust winit wgpu cosmic-text",
                "message protocol and generation id",
            ];

            Ok(WorkerResultPayload::SearchMatches {
                query: query.clone(),
                matches: corpus
                    .iter()
                    .filter(|line| line.contains(query))
                    .map(|line| (*line).to_string())
                    .collect(),
            })
        }
        WorkerRequestPayload::MockCpuBurn {
            job_label,
            busy_millis,
        } => {
            let label = job_label.clone();
            let millis = *busy_millis;
            let checksum = tokio::task::spawn_blocking(move || cpu_burn_checksum(millis))
                .await
                .map_err(|err| format!("cpu burn join error: {err}"))?;

            Ok(WorkerResultPayload::CpuBurnSummary {
                job_label: label,
                busy_millis: millis,
                checksum,
            })
        }
        WorkerRequestPayload::MockPanic { reason } => {
            panic!("mock worker panic: {reason}");
        }
        WorkerRequestPayload::CheckLspForPath { path } => {
            let path = path.clone();
            let ext_hint = path
                .extension()
                .and_then(|ext| ext.to_str())
                .unwrap_or("?")
                .to_string();
            let result = tokio::task::spawn_blocking(move || {
                crate::lsp::client::lsp_entry_and_status_for_path(&path).map(
                    |(entry, is_installed)| WorkerResultPayload::LspCheckResult {
                        path: path.clone(),
                        binary: entry.binary.to_string(),
                        language_label: entry.language_label.to_string(),
                        install_cmd: entry.install_cmd.to_string(),
                        is_installed,
                    },
                )
            })
            .await
            .map_err(|err| format!("lsp check join error: {err}"))?;

            match result {
                Some(payload) => Ok(payload),
                None => Err(format!("no LSP registry entry for .{ext_hint}")),
            }
        }
        WorkerRequestPayload::GitBlameLine {
            workspace_root,
            file_path,
            line_number,
        } => {
            let summary = super::git::run_git_blame_line(
                workspace_root.clone(),
                file_path.clone(),
                *line_number,
            )
            .await?;
            Ok(WorkerResultPayload::GitBlameLine {
                file_path: file_path.clone(),
                line_number: *line_number,
                summary,
            })
        }
        WorkerRequestPayload::RefreshWorkspaceGitStatus { workspace_root } => {
            let statuses = super::git::run_workspace_git_status(workspace_root.clone()).await?;
            Ok(WorkerResultPayload::WorkspaceGitStatus {
                workspace_root: workspace_root.clone(),
                statuses,
            })
        }
        WorkerRequestPayload::FetchGitBaseline {
            workspace_root,
            file_path,
        } => {
            let baseline =
                super::git::run_fetch_git_baseline(workspace_root.clone(), file_path.clone())
                    .await?;
            Ok(WorkerResultPayload::BufferGitBaseline {
                file_path: file_path.clone(),
                baseline,
            })
        }
        WorkerRequestPayload::LoadFilePreview {
            file_path,
            max_lines,
            target_line,
        } => {
            let file_path = file_path.clone();
            let max_lines = *max_lines;
            let target_line = *target_line;
            tokio::task::spawn_blocking(move || WorkerResultPayload::FilePreviewLoaded {
                lines: super::fzf::build_file_preview_lines(&file_path, max_lines, target_line),
                file_path,
                target_line,
            })
            .await
            .map_err(|err| format!("file preview join error: {err}"))
        }
        WorkerRequestPayload::StartFileWatch { .. } => {
            Err("StartFileWatch request should be handled by dedicated watch loop".to_string())
        }
        WorkerRequestPayload::SpawnPtyShell { .. }
        | WorkerRequestPayload::SpawnPtyCommand { .. }
        | WorkerRequestPayload::SpawnDetachedShellCommand { .. }
        | WorkerRequestPayload::WritePtyInput { .. }
        | WorkerRequestPayload::ResizePtySession { .. }
        | WorkerRequestPayload::ClosePtySession { .. } => {
            Err("PTY request should be handled by dedicated PTY runner".to_string())
        }
        WorkerRequestPayload::StartLspServer { .. }
        | WorkerRequestPayload::LspDidOpen { .. }
        | WorkerRequestPayload::LspDidChange { .. }
        | WorkerRequestPayload::LspDidClose { .. }
        | WorkerRequestPayload::LspHoverRequest { .. }
        | WorkerRequestPayload::LspDefinitionRequest { .. }
        | WorkerRequestPayload::LspReferencesRequest { .. }
        | WorkerRequestPayload::LspDocumentSymbolsRequest { .. }
        | WorkerRequestPayload::LspFormattingRequest { .. }
        | WorkerRequestPayload::LspCompletionRequest { .. }
        | WorkerRequestPayload::StopLspServer
        | WorkerRequestPayload::ShutdownAllLspServers => {
            Err("LSP request should be handled by dedicated LSP runner".to_string())
        }
        WorkerRequestPayload::FzfSearch { .. } => {
            Err("FzfSearch request should be handled by dedicated fzf runner".to_string())
        }
        WorkerRequestPayload::AiInlineCompletionRequest { .. } => {
            Err("AI inline completion request should be handled by dedicated AI runner".to_string())
        }
        WorkerRequestPayload::AiChatRequest { .. } => {
            Err("AI chat request should be handled by dedicated AI chat runner".to_string())
        }
        WorkerRequestPayload::LoadLocalHistory { .. }
        | WorkerRequestPayload::SaveLocalHistory { .. } => {
            Err("local history request should be handled by dedicated history runner".to_string())
        }
    }
}

fn byte_range_for_line_window(
    source: &str,
    window_start_line: usize,
    window_line_count: usize,
) -> Option<Range<usize>> {
    if source.is_empty() {
        return None;
    }

    let mut line_starts = Vec::with_capacity(source.lines().count().max(1) + 1);
    line_starts.push(0);
    for (idx, byte) in source.bytes().enumerate() {
        if byte == b'\n' && idx + 1 <= source.len() {
            line_starts.push(idx + 1);
        }
    }
    if line_starts.is_empty() {
        return None;
    }

    let total_lines = line_starts.len();
    let clamped_start = window_start_line.min(total_lines.saturating_sub(1));
    let line_count = window_line_count.max(1);
    let end_line_exclusive = clamped_start.saturating_add(line_count).min(total_lines);

    let start = line_starts[clamped_start];
    let end = if end_line_exclusive < total_lines {
        line_starts[end_line_exclusive]
    } else {
        source.len()
    };

    (start < end).then_some(start..end)
}

fn highlight_byte_window(
    source: &str,
    viewport_line_start: usize,
    viewport_line_count: usize,
    line_count: usize,
    byte_count: usize,
) -> Option<Range<usize>> {
    if source.is_empty() {
        return None;
    }

    if should_highlight_full_buffer(line_count, byte_count) {
        return None;
    }

    let visible_lines = viewport_line_count.max(1);
    let overscan = visible_lines
        .saturating_mul(VIEWPORT_HIGHLIGHT_OVERSCAN_MULTIPLIER)
        .max(VIEWPORT_HIGHLIGHT_MIN_OVERSCAN_LINES);
    let window_line_count = visible_lines.saturating_add(overscan.saturating_mul(2));
    let window_start_line = viewport_line_start.saturating_sub(overscan);
    byte_range_for_line_window(source, window_start_line, window_line_count)
}

fn should_highlight_full_buffer(line_count: usize, byte_count: usize) -> bool {
    line_count <= FULL_BUFFER_HIGHLIGHT_LINE_THRESHOLD
        && byte_count <= FULL_BUFFER_HIGHLIGHT_BYTE_THRESHOLD
}

fn cpu_burn_checksum(busy_millis: u64) -> u64 {
    let started = Instant::now();
    let budget = Duration::from_millis(busy_millis);
    let mut state = 0x9E37_79B9_7F4A_7C15_u64;
    let mut checksum = 0u64;

    while started.elapsed() < budget {
        state = state
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        checksum ^= state.rotate_left((state & 31) as u32);
    }

    checksum
}
