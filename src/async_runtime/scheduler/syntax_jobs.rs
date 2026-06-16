use std::{
    ops::Range,
    path::PathBuf,
    sync::Arc,
    time::{Duration, Instant},
};

use winit::event_loop::EventLoopProxy;

use crate::app::event_loop::AppEvent;

use crate::{
    async_runtime::message::{WorkerRequest, WorkerRequestPayload, WorkerResultPayload},
    syntax::{
        highlight::{generate_highlight_spans_in_byte_window, generate_highlight_spans_with_cache},
        syntax_engine::SyntaxEngine,
    },
};

use super::{
    FULL_BUFFER_HIGHLIGHT_BYTE_THRESHOLD, FULL_BUFFER_HIGHLIGHT_LINE_THRESHOLD,
    SyntaxEngineCacheHandle, VIEWPORT_HIGHLIGHT_MIN_OVERSCAN_LINES,
    VIEWPORT_HIGHLIGHT_OVERSCAN_MULTIPLIER, async_trace,
};

pub(super) async fn execute_virtual_job(
    request: &WorkerRequest,
    syntax_cache: Arc<SyntaxEngineCacheHandle>,
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
            line_starts,
            edit_hint,
        } => {
            let buffer_id = buffer_id.clone();
            let file_path = file_path.clone();
            let text_snapshot = text_snapshot.clone();
            let language_id = *language_id;
            let buffer_revision = *buffer_revision;
            let viewport_line_start = *viewport_line_start;
            let viewport_line_count = *viewport_line_count;
            let line_starts = line_starts.clone();
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
                    match guard.take_main_parser(&file_key) {
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
                    &line_starts,
                );

                // Get injection parser cache from the syntax cache
                let mut injection_cache = {
                    let mut guard = syntax_cache
                        .lock()
                        .map_err(|_| "syntax engine cache lock poisoned".to_string())?;
                    std::mem::take(&mut guard.injection_parsers)
                };

                let spans = covered_byte_range
                    .clone()
                    .map(|window| {
                        generate_highlight_spans_in_byte_window(tree, &text_snapshot, window)
                    })
                    .unwrap_or_else(|| generate_highlight_spans_with_cache(tree, &text_snapshot, &mut injection_cache));

                let foldable_ranges = crate::syntax::fold::compute_foldable_ranges(
                    tree.root_node(),
                    tree.language_id(),
                );

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
                    guard.return_main_parser(file_key, engine);
                    // Return injection cache back
                    guard.injection_parsers = injection_cache;
                }

                Ok(WorkerResultPayload::ParseAndHighlight {
                    buffer_id,
                    file_path,
                    language_id,
                    buffer_revision,
                    spans,
                    covered_byte_range,
                    foldable_ranges,
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
        WorkerRequestPayload::CheckSystemDeps => {
            let resolved_path = resolve_system_path();
            let tools = [
                "fzf",
                "lazygit",
                "lazydocker",
                "rg",
                "fd",
                "bat",
                "delta",
                "opencode",
                "rust-analyzer",
                "typescript-language-server",
                "gopls",
                "dart",
                "pyright-langserver",
                "ruff",
                "jdtls",
                "sqls",
                "yaml-language-server",
                "docker-langserver",
                "vscode-json-language-server",
                "bash-language-server",
            ];
            let missing: Vec<String> = tools
                .iter()
                .filter(|tool| {
                    !std::process::Command::new("which")
                        .arg(tool)
                        .env("PATH", &resolved_path)
                        .stdout(std::process::Stdio::null())
                        .stderr(std::process::Stdio::null())
                        .status()
                        .map(|s| s.success())
                        .unwrap_or(false)
                })
                .map(|s| s.to_string())
                .collect();
            Ok(WorkerResultPayload::SystemDepCheckResult { missing })
        }
        WorkerRequestPayload::InstallSystemDeps { .. } => {
            Err("InstallSystemDeps should be handled by dedicated install runner".to_string())
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
        WorkerRequestPayload::WorkspaceExportIndexRequest {
            language_id,
            workspace_root,
        } => {
            let language_id = language_id.clone();
            let workspace_root = workspace_root.clone();
            tokio::task::spawn_blocking(move || WorkerResultPayload::WorkspaceSymbols {
                language_id,
                symbols: crate::lsp::index_ts_js_workspace_exports(&workspace_root),
            })
            .await
            .map_err(|err| format!("workspace export index join error: {err}"))
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
        | WorkerRequestPayload::LspRenameRequest { .. }
        | WorkerRequestPayload::LspDocumentSymbolsRequest { .. }
        | WorkerRequestPayload::LspFormattingRequest { .. }
        | WorkerRequestPayload::LspCompletionRequest { .. }
        | WorkerRequestPayload::LspCompletionResolveRequest { .. }
        | WorkerRequestPayload::LspCodeActionRequest { .. }
        | WorkerRequestPayload::WorkspaceSymbolRequest { .. }
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
        WorkerRequestPayload::ScanPythonEnvironments { .. }
        | WorkerRequestPayload::ScanDartEnvironments { .. }
        | WorkerRequestPayload::DetectRuntimeVersions { .. } => {
            Err("request should be handled by dedicated runner".to_string())
        }
        _ => Err("execute_virtual_job received non-virtual payload".to_string()),
    }
}

fn byte_range_for_line_window(
    source: &str,
    window_start_line: usize,
    window_line_count: usize,
    line_starts: &[usize],
) -> Option<Range<usize>> {
    if source.is_empty() || line_starts.is_empty() {
        return None;
    }

    let total_lines = line_starts.len();
    let clamped_start = window_start_line.min(total_lines.saturating_sub(1));
    let line_count = window_line_count.max(1);
    let end_line_exclusive = clamped_start.saturating_add(line_count).min(total_lines);

    let start = line_starts[clamped_start].min(source.len());
    let end = if end_line_exclusive < total_lines {
        line_starts[end_line_exclusive].min(source.len())
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
    line_starts: &[usize],
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
    byte_range_for_line_window(source, window_start_line, window_line_count, line_starts)
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

/// Xây dựng $PATH đầy đủ cho GUI bundle — bổ sung các thư mục mà app GUI thường thiếu.
pub(crate) fn resolve_system_path() -> String {
    let current = std::env::var("PATH").unwrap_or_default();
    let home = std::env::var("HOME").unwrap_or_default();

    let extras: &[&str] = &[
        "/opt/homebrew/bin",
        "/opt/homebrew/sbin",
        "/usr/local/bin",
        "/usr/bin",
        "/bin",
        "/usr/sbin",
        "/sbin",
    ];

    let mut home_extras = Vec::new();
    push_unique_path(&mut home_extras, format!("{home}/.cargo/bin"));
    let gvm_root = std::env::var("GVM_ROOT").unwrap_or_else(|_| format!("{home}/.gvm"));
    for path in crate::lsp::client::resolve_gvm_paths(&gvm_root) {
        push_unique_path(&mut home_extras, path);
    }
    push_unique_path(&mut home_extras, format!("{home}/go/bin"));
    // nvm has no "current" symlink by default — scan installed versions and add
    // the newest ones so node-based language servers are detected.
    for path in resolve_nvm_bin_paths(&home) {
        push_unique_path(&mut home_extras, path);
    }
    push_unique_path(&mut home_extras, format!("{home}/.npm-global/bin"));
    push_unique_path(&mut home_extras, format!("{home}/.jenv/shims"));
    push_unique_path(&mut home_extras, format!("{home}/.jenv/bin"));
    push_unique_path(&mut home_extras, format!("{home}/.pyenv/shims"));
    push_unique_path(&mut home_extras, format!("{home}/.pyenv/bin"));
    push_unique_path(&mut home_extras, format!("{home}/.rbenv/shims"));
    push_unique_path(&mut home_extras, format!("{home}/.rbenv/bin"));
    push_unique_path(&mut home_extras, format!("{home}/.volta/bin"));
    push_unique_path(&mut home_extras, format!("{home}/.asdf/shims"));
    push_unique_path(&mut home_extras, format!("{home}/.bun/bin"));
    push_unique_path(&mut home_extras, format!("{home}/.deno/bin"));
    // Dart/Flutter: pub global executables + FVM-managed SDK
    push_unique_path(&mut home_extras, format!("{home}/.pub-cache/bin"));
    push_unique_path(&mut home_extras, format!("{home}/fvm/default/bin"));
    push_unique_path(&mut home_extras, format!("{home}/.fvm/default/bin"));
    push_unique_path(
        &mut home_extras,
        format!("{home}/.local/share/netherize/bin"),
    );
    push_unique_path(&mut home_extras, format!("{home}/.local/bin"));

    let mut dirs: Vec<&str> = current.split(':').filter(|s| !s.is_empty()).collect();
    for extra in extras {
        if !dirs.contains(extra) {
            dirs.push(extra);
        }
    }
    for extra in &home_extras {
        if !dirs.iter().any(|d| *d == extra.as_str()) {
            dirs.push(extra.as_str());
        }
    }
    dirs.join(":")
}

/// Collect bin dirs of installed nvm node versions, newest first (capped to 3).
/// Standard nvm has no stable "current" symlink, so enumerate ~/.nvm/versions/node/*.
fn resolve_nvm_bin_paths(home: &str) -> Vec<String> {
    let versions_root = std::path::Path::new(home).join(".nvm/versions/node");
    let Ok(entries) = std::fs::read_dir(&versions_root) else {
        return Vec::new();
    };
    let mut versions: Vec<String> = entries
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.path().join("bin").is_dir())
        .filter_map(|entry| entry.file_name().into_string().ok())
        .collect();
    // Sort semver-ish descending: v20.11.1 > v18.19.0
    versions.sort_by(|a, b| {
        let parse = |v: &str| -> Vec<u64> {
            v.trim_start_matches('v')
                .split('.')
                .filter_map(|part| part.parse::<u64>().ok())
                .collect()
        };
        parse(b).cmp(&parse(a))
    });
    versions
        .into_iter()
        .take(3)
        .map(|version| {
            versions_root
                .join(version)
                .join("bin")
                .to_string_lossy()
                .into_owned()
        })
        .collect()
}

fn push_unique_path(paths: &mut Vec<String>, path: String) {
    if path.is_empty() || paths.iter().any(|existing| existing == &path) {
        return;
    }
    paths.push(path);
}

/// Cài đặt từng tool một, gửi progress message về main thread sau mỗi bước.
pub(super) async fn run_extension_command(
    binary: String,
    command: String,
    uninstall: bool,
    working_dir: Option<PathBuf>,
    tx: std::sync::mpsc::Sender<crate::async_runtime::message::WorkerMessage>,
    event_proxy: EventLoopProxy<AppEvent>,
) {
    use super::emit::emit_message_and_wake;
    use crate::async_runtime::message::WorkerMessage;
    use tokio::io::{AsyncBufReadExt, BufReader};

    emit_message_and_wake(
        &tx,
        &event_proxy,
        WorkerMessage::ExtensionCommandStarted {
            binary: binary.clone(),
            uninstall,
        },
    );

    let mut child_cmd = tokio::process::Command::new("sh");
    child_cmd
        .arg("-c")
        .arg(&command)
        .env("PATH", resolve_system_path())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    if let Some(dir) = working_dir {
        child_cmd.current_dir(dir);
    }

    let mut child = match child_cmd.spawn() {
        Ok(child) => child,
        Err(err) => {
            emit_message_and_wake(
                &tx,
                &event_proxy,
                WorkerMessage::ExtensionCommandLog {
                    binary: binary.clone(),
                    line: format!("spawn failed: {err}"),
                },
            );
            emit_message_and_wake(
                &tx,
                &event_proxy,
                WorkerMessage::ExtensionCommandFinished {
                    binary,
                    uninstall,
                    success: false,
                    exit_code: None,
                },
            );
            return;
        }
    };

    let mut stdout_task = None;
    if let Some(stdout) = child.stdout.take() {
        let tx_clone = tx.clone();
        let proxy_clone = event_proxy.clone();
        let binary_clone = binary.clone();
        stdout_task = Some(tokio::spawn(async move {
            let mut lines = BufReader::new(stdout).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                emit_message_and_wake(
                    &tx_clone,
                    &proxy_clone,
                    WorkerMessage::ExtensionCommandLog {
                        binary: binary_clone.clone(),
                        line,
                    },
                );
            }
        }));
    }

    let mut stderr_task = None;
    if let Some(stderr) = child.stderr.take() {
        let tx_clone = tx.clone();
        let proxy_clone = event_proxy.clone();
        let binary_clone = binary.clone();
        stderr_task = Some(tokio::spawn(async move {
            let mut lines = BufReader::new(stderr).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                emit_message_and_wake(
                    &tx_clone,
                    &proxy_clone,
                    WorkerMessage::ExtensionCommandLog {
                        binary: binary_clone.clone(),
                        line,
                    },
                );
            }
        }));
    }

    let status = child.wait().await;
    if let Some(task) = stdout_task {
        let _ = task.await;
    }
    if let Some(task) = stderr_task {
        let _ = task.await;
    }
    let (success, exit_code) = match status {
        Ok(status) => (status.success(), status.code()),
        Err(err) => {
            emit_message_and_wake(
                &tx,
                &event_proxy,
                WorkerMessage::ExtensionCommandLog {
                    binary: binary.clone(),
                    line: format!("wait failed: {err}"),
                },
            );
            (false, None)
        }
    };

    emit_message_and_wake(
        &tx,
        &event_proxy,
        WorkerMessage::ExtensionCommandFinished {
            binary,
            uninstall,
            success,
            exit_code,
        },
    );
}

pub(super) async fn run_system_dep_install(
    tools: Vec<String>,
    tx: std::sync::mpsc::Sender<crate::async_runtime::message::WorkerMessage>,
    event_proxy: EventLoopProxy<AppEvent>,
) {
    use super::emit::emit_message_and_wake;
    use crate::async_runtime::message::{InstallStatus, WorkerMessage};

    let resolved_path = resolve_system_path();

    for tool in &tools {
        emit_message_and_wake(
            &tx,
            &event_proxy,
            WorkerMessage::SystemDepToolProgress {
                tool: tool.clone(),
                status: InstallStatus::Installing,
            },
        );

        let install_cmd = if cfg!(target_os = "macos") {
            format!("brew install {tool}")
        } else {
            format!("sudo apt-get install -y {tool}")
        };

        let success = tokio::process::Command::new("sh")
            .arg("-c")
            .arg(&install_cmd)
            .env("PATH", &resolved_path)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .await
            .map(|s| s.success())
            .unwrap_or(false);

        emit_message_and_wake(
            &tx,
            &event_proxy,
            WorkerMessage::SystemDepToolProgress {
                tool: tool.clone(),
                status: if success {
                    InstallStatus::Success
                } else {
                    InstallStatus::Failed
                },
            },
        );
    }

    emit_message_and_wake(&tx, &event_proxy, WorkerMessage::SystemDepInstallDone);
}

/// Execute a single test case: spawn `program args`, feed `input` to stdin,
/// and capture stdout/stderr with a wall-clock `timeout`. Never panics — every
/// failure path returns a `spawn_error`. Free of `tx`/`event_proxy` so it is
/// directly integration-testable under `#[tokio::test]`.
pub(crate) async fn execute_one_case(
    program: &str,
    args: &[String],
    working_dir: Option<&std::path::Path>,
    input: &str,
    timeout: Duration,
) -> crate::runner::TestCaseOutcome {
    use crate::runner::TestCaseOutcome;
    use tokio::io::AsyncWriteExt;

    let started = Instant::now();
    let mut cmd = tokio::process::Command::new(program);
    cmd.args(args)
        .env("PATH", resolve_system_path())
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    if let Some(dir) = working_dir {
        cmd.current_dir(dir);
    }

    let mut child = match cmd.spawn() {
        Ok(child) => child,
        Err(err) => {
            return TestCaseOutcome {
                actual: String::new(),
                stderr: String::new(),
                exit_code: None,
                duration_ms: started.elapsed().as_millis() as u64,
                spawn_error: Some(format!("failed to run `{program}`: {err}")),
            };
        }
    };

    // Write stdin (ignore broken-pipe if the program never reads it), then drop
    // the handle so the child sees EOF.
    if let Some(mut stdin) = child.stdin.take() {
        let _ = stdin.write_all(input.as_bytes()).await;
        let _ = stdin.shutdown().await;
    }

    match tokio::time::timeout(timeout, child.wait_with_output()).await {
        Ok(Ok(output)) => TestCaseOutcome {
            actual: String::from_utf8_lossy(&output.stdout).to_string(),
            stderr: String::from_utf8_lossy(&output.stderr).to_string(),
            exit_code: output.status.code(),
            duration_ms: started.elapsed().as_millis() as u64,
            spawn_error: None,
        },
        Ok(Err(err)) => TestCaseOutcome {
            actual: String::new(),
            stderr: String::new(),
            exit_code: None,
            duration_ms: started.elapsed().as_millis() as u64,
            spawn_error: Some(format!("process error: {err}")),
        },
        Err(_) => TestCaseOutcome {
            actual: String::new(),
            stderr: String::new(),
            exit_code: None,
            duration_ms: started.elapsed().as_millis() as u64,
            spawn_error: Some(format!("timed out after {} ms", timeout.as_millis())),
        },
    }
}

/// Run a one-time compile step (e.g. `rustc`). Returns `Err(stderr+stdout)` on
/// spawn failure, non-zero exit, or timeout so the caller can surface the
/// compiler diagnostics. Compilation gets a generous fixed deadline independent
/// of the per-case run timeout.
async fn run_compile_step(
    step: &crate::runner::CompileStep,
    working_dir: Option<&std::path::Path>,
) -> Result<(), String> {
    const COMPILE_TIMEOUT: Duration = Duration::from_secs(60);

    let mut cmd = tokio::process::Command::new(&step.program);
    cmd.args(&step.args)
        .env("PATH", resolve_system_path())
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    if let Some(dir) = working_dir {
        cmd.current_dir(dir);
    }

    let child = match cmd.spawn() {
        Ok(child) => child,
        Err(err) => return Err(format!("failed to run `{}`: {err}", step.program)),
    };

    match tokio::time::timeout(COMPILE_TIMEOUT, child.wait_with_output()).await {
        Ok(Ok(output)) if output.status.success() => Ok(()),
        Ok(Ok(output)) => {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let stdout = String::from_utf8_lossy(&output.stdout);
            let mut msg = stderr.trim().to_string();
            if msg.is_empty() {
                msg = stdout.trim().to_string();
            }
            if msg.is_empty() {
                msg = format!("compiler exited with {:?}", output.status.code());
            }
            Err(msg)
        }
        Ok(Err(err)) => Err(format!("compiler process error: {err}")),
        Err(_) => Err(format!(
            "compilation timed out after {} s",
            COMPILE_TIMEOUT.as_secs()
        )),
    }
}

/// Worker entry point: run every case sequentially and emit a single
/// `TestCasesCompleted` result. Sequential (not parallel) keeps output
/// deterministic and avoids a fork-bomb of compiler/interpreter processes.
#[allow(clippy::too_many_arguments)]
pub(super) async fn run_test_cases(
    request_id: u64,
    revision_id: u64,
    compile: Option<crate::runner::CompileStep>,
    program: String,
    args: Vec<String>,
    working_dir: Option<PathBuf>,
    inputs: Vec<String>,
    timeout_ms: u64,
    command_preview: String,
    tx: std::sync::mpsc::Sender<crate::async_runtime::message::WorkerMessage>,
    event_proxy: EventLoopProxy<AppEvent>,
) {
    use super::emit::emit_message_and_wake;
    use crate::async_runtime::message::{
        RequestTopic, WorkerMessage, WorkerResult, WorkerResultPayload,
    };

    let timeout = Duration::from_millis(timeout_ms.max(1));

    // Compiled languages (Rust): build once before any case. A compile failure
    // marks every case Error with the compiler output, so the user sees the
    // diagnostics instead of N identical spawn errors.
    if let Some(step) = &compile {
        if let Err(compile_error) = run_compile_step(step, working_dir.as_deref()).await {
            let outcomes = inputs
                .iter()
                .map(|_| crate::runner::TestCaseOutcome {
                    actual: String::new(),
                    stderr: compile_error.clone(),
                    exit_code: None,
                    duration_ms: 0,
                    spawn_error: Some("compilation failed".to_string()),
                })
                .collect();
            emit_message_and_wake(
                &tx,
                &event_proxy,
                WorkerMessage::Result(WorkerResult {
                    request_id,
                    revision_id,
                    topic: RequestTopic::TestRunner,
                    payload: WorkerResultPayload::TestCasesCompleted {
                        command_preview,
                        outcomes,
                    },
                }),
            );
            return;
        }
    }

    let mut outcomes = Vec::with_capacity(inputs.len());
    for input in &inputs {
        let outcome =
            execute_one_case(&program, &args, working_dir.as_deref(), input, timeout).await;
        outcomes.push(outcome);
    }

    emit_message_and_wake(
        &tx,
        &event_proxy,
        WorkerMessage::Result(WorkerResult {
            request_id,
            revision_id,
            topic: RequestTopic::TestRunner,
            payload: WorkerResultPayload::TestCasesCompleted {
                command_preview,
                outcomes,
            },
        }),
    );
}

#[cfg(test)]
mod test_runner_tests {
    use super::execute_one_case;
    use std::time::Duration;

    #[tokio::test]
    async fn runs_python_with_stdin_and_captures_stdout() {
        // Skip silently if python3 isn't installed on this host.
        if tokio::process::Command::new("python3")
            .arg("--version")
            .output()
            .await
            .is_err()
        {
            return;
        }
        let args = vec![
            "-c".to_string(),
            "import sys; print(int(sys.stdin.read()) * 2)".to_string(),
        ];
        let outcome =
            execute_one_case("python3", &args, None, "21\n", Duration::from_secs(10)).await;
        assert_eq!(outcome.spawn_error, None);
        assert_eq!(outcome.exit_code, Some(0));
        assert_eq!(outcome.actual.trim(), "42");
    }

    #[tokio::test]
    async fn missing_program_reports_spawn_error() {
        let outcome = execute_one_case(
            "definitely_not_a_real_binary_xyz",
            &[],
            None,
            "",
            Duration::from_secs(2),
        )
        .await;
        assert!(outcome.spawn_error.is_some());
        assert_eq!(outcome.exit_code, None);
    }

    #[tokio::test]
    async fn compiles_and_runs_rust_via_run_plan() {
        // Skip silently if rustc isn't installed on this host.
        if tokio::process::Command::new("rustc")
            .arg("--version")
            .output()
            .await
            .is_err()
        {
            return;
        }
        // Write a tiny doubling program, resolve its run plan, compile, run.
        let dir = std::env::temp_dir();
        let src = dir.join("netherize_lc_double_test.rs");
        let bin = dir.join("netherize_lc_double_test_bin");
        std::fs::write(
            &src,
            "use std::io::Read;\nfn main(){let mut s=String::new();\
             std::io::stdin().read_to_string(&mut s).unwrap();\
             println!(\"{}\", s.trim().parse::<i64>().unwrap()*2);}\n",
        )
        .unwrap();

        let plan = crate::runner::resolve_run_plan(&src, &bin).expect("rust run plan");
        let compile = plan.compile.expect("rust compiles");
        super::run_compile_step(&compile, None)
            .await
            .expect("compile succeeds");

        let outcome = execute_one_case(
            &plan.program,
            &plan.args,
            None,
            "21\n",
            Duration::from_secs(10),
        )
        .await;
        assert_eq!(outcome.spawn_error, None);
        assert_eq!(outcome.exit_code, Some(0));
        assert_eq!(outcome.actual.trim(), "42");

        let _ = std::fs::remove_file(&src);
        let _ = std::fs::remove_file(&bin);
    }

    #[tokio::test]
    async fn rust_compile_failure_is_reported() {
        if tokio::process::Command::new("rustc")
            .arg("--version")
            .output()
            .await
            .is_err()
        {
            return;
        }
        let dir = std::env::temp_dir();
        let src = dir.join("netherize_lc_broken_test.rs");
        let bin = dir.join("netherize_lc_broken_test_bin");
        std::fs::write(&src, "fn main() { this is not valid rust }\n").unwrap();

        let plan = crate::runner::resolve_run_plan(&src, &bin).expect("rust run plan");
        let compile = plan.compile.expect("rust compiles");
        let result = super::run_compile_step(&compile, None).await;
        assert!(result.is_err(), "broken rust must fail to compile");

        let _ = std::fs::remove_file(&src);
    }
}
