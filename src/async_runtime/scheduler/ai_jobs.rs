use std::path::PathBuf;
use std::process::Stdio;
use std::sync::mpsc as std_mpsc;
use std::sync::{Arc, Mutex};

/// Platform install command for opencode:
///   macOS/Linux — pipe the official installer shell script
#[cfg(unix)]
const INSTALL_CMD: &str = "sh";
#[cfg(unix)]
const INSTALL_ARGS: &[&str] = &["-c", "curl -fsSL https://opencode.ai/install | sh"];
#[cfg(windows)]
const INSTALL_CMD: &str = "powershell";
#[cfg(windows)]
const INSTALL_ARGS: &[&str] = &[
    "-NoProfile",
    "-Command",
    "irm https://opencode.ai/install.ps1 | iex",
];

use tokio::io::{AsyncBufReadExt, BufReader};
use winit::event_loop::EventLoopProxy;

use crate::app::event_loop::AppEvent;
use crate::async_runtime::message::WorkerMessage;

use super::emit::emit_message_and_wake;

const MAX_AI_FILE_REFS: usize = 4;
const MAX_AI_FILE_CONTEXT_BYTES: usize = 64 * 1024;

fn strip_ansi_sequences(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\x1b' && chars.peek() == Some(&'[') {
            chars.next();
            while let Some(&ch) = chars.peek() {
                chars.next();
                if ch.is_ascii_alphabetic() {
                    break;
                }
            }
        } else if c != '\r' {
            out.push(c);
        }
    }
    out
}

fn should_skip_opencode_line(line: &str) -> bool {
    let trimmed = line.trim();
    trimmed == "[0m"
        || (trimmed.starts_with("> ")
            && (trimmed.contains("build ·")
                || trimmed.contains("build •")
                || trimmed.contains("mimo-")
                || trimmed.contains("tokens")))
}

fn sanitize_opencode_line(line: &str) -> Option<String> {
    let cleaned = strip_ansi_sequences(line);
    (!should_skip_opencode_line(&cleaned)).then_some(cleaned)
}

async fn build_prompt_with_file_context(prompt: String, file_refs: Vec<PathBuf>) -> String {
    if file_refs.is_empty() {
        return prompt;
    }

    let mut sections = Vec::new();
    for path in file_refs.into_iter().take(MAX_AI_FILE_REFS) {
        let display_path = path.display().to_string();
        let section = match tokio::fs::read(&path).await {
            Ok(bytes) => {
                let truncated = bytes.len() > MAX_AI_FILE_CONTEXT_BYTES;
                let take = bytes.len().min(MAX_AI_FILE_CONTEXT_BYTES);
                let slice = &bytes[..take];
                if slice.contains(&0) {
                    format!(
                        "--- BEGIN FILE: {display_path} ---\n[Skipped binary-looking file]\n--- END FILE ---"
                    )
                } else {
                    let text = String::from_utf8_lossy(slice);
                    let suffix = if truncated {
                        "\n[Truncated: only the first 64 KiB was included]"
                    } else {
                        ""
                    };
                    format!("--- BEGIN FILE: {display_path} ---\n{text}{suffix}\n--- END FILE ---")
                }
            }
            Err(err) => {
                format!(
                    "--- BEGIN FILE: {display_path} ---\n[Could not read file: {err}]\n--- END FILE ---"
                )
            }
        };
        sections.push(section);
    }

    format!(
        "The user attached these files as extra context:\n\n{}\n\nUser request:\n{}",
        sections.join("\n\n"),
        prompt
    )
}

/// Resolve the opencode binary path.
/// Checks $PATH first, then falls back to the default install location
/// (~/.opencode/bin/opencode on Unix) which is not always on the editor's PATH.
pub(super) fn resolve_opencode_binary() -> Option<PathBuf> {
    if let Some(path_var) = std::env::var_os("PATH") {
        for dir in std::env::split_paths(&path_var) {
            let candidate = dir.join("opencode");
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    // Default install location used by the official installer.
    if let Some(home) = std::env::var_os("HOME") {
        let candidate = PathBuf::from(home)
            .join(".opencode")
            .join("bin")
            .join("opencode");
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

/// Spawn `opencode run <prompt>` (non-interactive mode) and stream stdout
/// line-by-line as `AiMessageChunk` events.
pub(super) async fn run_ai_chat_stream(
    worker_tx: std_mpsc::Sender<WorkerMessage>,
    event_proxy: EventLoopProxy<AppEvent>,
    prompt: String,
    active_buffer_path: Option<PathBuf>,
    workspace_root: Option<PathBuf>,
    _cursor_position: (usize, usize),
    _history: Vec<(String, String)>,
    file_refs: Vec<PathBuf>,
    model: Option<String>,
    agent: Option<String>,
) {
    let binary = match resolve_opencode_binary() {
        Some(p) => p,
        None => {
            emit_message_and_wake(
                &worker_tx,
                &event_proxy,
                WorkerMessage::AiStreamError {
                    error: "opencode not found. Install it first.".to_string(),
                },
            );
            return;
        }
    };

    let prompt = build_prompt_with_file_context(prompt, file_refs).await;

    let mut cmd = tokio::process::Command::new(&binary);
    cmd.arg("run");
    cmd.arg(&prompt);
    if let Some(ref m) = model {
        cmd.arg("--model").arg(m);
    }
    if let Some(ref agent) = agent {
        cmd.arg("--agent").arg(agent);
    }

    // Set working directory: workspace root → file parent → inherit.
    if let Some(ref root) = workspace_root {
        cmd.current_dir(root);
    } else if let Some(ref path) = active_buffer_path {
        if let Some(parent) = path.parent() {
            cmd.current_dir(parent);
        }
    }

    cmd.stdin(Stdio::null());
    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::piped());

    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(err) => {
            emit_message_and_wake(
                &worker_tx,
                &event_proxy,
                WorkerMessage::AiStreamError {
                    error: format!("Failed to launch opencode: {err}"),
                },
            );
            return;
        }
    };

    // Drain stderr in the background so it cannot block the child. opencode
    // prints spinner/model status there, so keep it out of the chat unless the
    // process fails.
    let stderr_lines: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
    if let Some(stderr) = child.stderr.take() {
        let stderr_lines = Arc::clone(&stderr_lines);
        tokio::spawn(async move {
            let mut lines = BufReader::new(stderr).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                if let Some(cleaned) = sanitize_opencode_line(&line)
                    && !cleaned.trim().is_empty()
                    && let Ok(mut collected) = stderr_lines.lock()
                {
                    collected.push(cleaned);
                }
            }
        });
    }

    let stdout = match child.stdout.take() {
        Some(s) => s,
        None => {
            emit_message_and_wake(
                &worker_tx,
                &event_proxy,
                WorkerMessage::AiStreamError {
                    error: "opencode stdout pipe unavailable".to_string(),
                },
            );
            return;
        }
    };

    let mut lines = BufReader::new(stdout).lines();
    let mut first_line = true;

    while let Ok(Some(line)) = lines.next_line().await {
        let Some(line) = sanitize_opencode_line(&line) else {
            continue;
        };
        if first_line && line.trim().is_empty() {
            continue;
        }
        let chunk = if first_line {
            first_line = false;
            line
        } else {
            format!("\n{line}")
        };

        emit_message_and_wake(
            &worker_tx,
            &event_proxy,
            WorkerMessage::AiMessageChunk { text: chunk },
        );
    }

    // Wait for the process to exit and surface any error.
    match child.wait().await {
        Ok(status) if status.success() => {
            emit_message_and_wake(&worker_tx, &event_proxy, WorkerMessage::AiStreamComplete);
        }
        Ok(status) => {
            let stderr_tail = stderr_lines
                .lock()
                .ok()
                .map(|lines| lines.join("\n"))
                .filter(|text| !text.trim().is_empty());
            let error = if let Some(stderr_tail) = stderr_tail {
                format!(
                    "opencode exited with status {}\n{}",
                    status.code().unwrap_or(-1),
                    stderr_tail
                )
            } else {
                format!(
                    "opencode exited with status {}",
                    status.code().unwrap_or(-1)
                )
            };
            emit_message_and_wake(
                &worker_tx,
                &event_proxy,
                WorkerMessage::AiStreamError { error },
            );
        }
        Err(err) => {
            emit_message_and_wake(
                &worker_tx,
                &event_proxy,
                WorkerMessage::AiStreamError {
                    error: format!("opencode wait failed: {err}"),
                },
            );
        }
    }
}

/// Install the opencode CLI using the platform's standard install script.
/// Streams install log lines as `AiMessageChunk` events, then emits
/// `AiInstallSuccess` on clean exit or `AiStreamError` on failure.
pub(super) async fn run_opencode_install(
    worker_tx: std_mpsc::Sender<WorkerMessage>,
    event_proxy: EventLoopProxy<AppEvent>,
) {
    let mut cmd = tokio::process::Command::new(INSTALL_CMD);
    cmd.args(INSTALL_ARGS);
    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::piped());

    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(err) => {
            emit_message_and_wake(
                &worker_tx,
                &event_proxy,
                WorkerMessage::AiStreamError {
                    error: format!("Failed to start install: {err}"),
                },
            );
            return;
        }
    };

    // Stream stdout AND stderr concurrently — install scripts typically write
    // progress to stderr, so we must drain both pipes to avoid blocking.
    if let Some(stderr) = child.stderr.take() {
        let tx2 = worker_tx.clone();
        let proxy2 = event_proxy.clone();
        tokio::spawn(async move {
            let mut lines = BufReader::new(stderr).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                let line = strip_ansi_sequences(&line);
                emit_message_and_wake(
                    &tx2,
                    &proxy2,
                    WorkerMessage::AiMessageChunk {
                        text: format!("{line}\n"),
                    },
                );
            }
        });
    }
    if let Some(stdout) = child.stdout.take() {
        let mut lines = BufReader::new(stdout).lines();
        while let Ok(Some(line)) = lines.next_line().await {
            let line = strip_ansi_sequences(&line);
            emit_message_and_wake(
                &worker_tx,
                &event_proxy,
                WorkerMessage::AiMessageChunk {
                    text: format!("{line}\n"),
                },
            );
        }
    }

    match child.wait().await {
        Ok(status) if status.success() => {
            emit_message_and_wake(&worker_tx, &event_proxy, WorkerMessage::AiInstallSuccess);
        }
        Ok(status) => {
            emit_message_and_wake(
                &worker_tx,
                &event_proxy,
                WorkerMessage::AiStreamError {
                    error: format!(
                        "Install failed with exit code {}",
                        status.code().unwrap_or(-1)
                    ),
                },
            );
        }
        Err(err) => {
            emit_message_and_wake(
                &worker_tx,
                &event_proxy,
                WorkerMessage::AiStreamError {
                    error: format!("Install process error: {err}"),
                },
            );
        }
    }
}
