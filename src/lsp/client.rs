use std::{
    collections::{HashMap, HashSet},
    fs,
    future::Future,
    io::{BufRead, Write},
    path::{Path, PathBuf},
    process::Stdio,
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
        mpsc as std_mpsc,
    },
    time::{Duration, Instant},
};

use serde_json::{Value, json};
use tokio::{
    io::{AsyncBufRead, AsyncBufReadExt, AsyncReadExt, AsyncWrite, AsyncWriteExt, BufReader},
    process::{Child, ChildStderr, ChildStdin, ChildStdout, Command},
    sync::Mutex as AsyncMutex,
};

use crate::{
    async_runtime::message::{LspDiagnostic, LspPosition, LspRange},
    lsp::{
        capabilities::ServerCapabilities,
        registry::{
            all_language_profiles, language_profile_for_binary, language_profile_for_extension,
            language_profile_for_path,
        },
    },
};

const LSP_INITIALIZE_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Debug, Clone)]
pub struct LspEntry {
    pub binary: &'static str,
    pub language_label: &'static str,
    pub install_cmd: &'static str,
}

pub fn lsp_entry_for_extension(ext: &str) -> Option<LspEntry> {
    let profile = language_profile_for_extension(ext)?;
    Some(LspEntry {
        binary: profile.lsp_binary,
        language_label: profile.language_label,
        install_cmd: profile.install_command,
    })
}

/// Resolves the active nvm Node version's bin directory.
///
/// Reads `~/.nvm/alias/default`, resolves partial versions (e.g. `"22"` →
/// `"v22.20.0"`) and lts aliases (e.g. `"lts/iron"`), then returns the
/// matching `bin/` directory path.
fn resolve_nvm_bin(home: &str) -> Option<String> {
    let nvm_dir = format!("{home}/.nvm");
    if !std::path::Path::new(&nvm_dir).exists() {
        return None;
    }

    let alias_raw = std::fs::read_to_string(format!("{nvm_dir}/alias/default")).ok()?;
    let alias = alias_raw.trim();

    // "lts/iron" → read ~/.nvm/alias/lts/iron for the concrete version
    let version_spec = if let Some(lts_name) = alias.strip_prefix("lts/") {
        let lts_raw = std::fs::read_to_string(format!("{nvm_dir}/alias/lts/{lts_name}")).ok()?;
        lts_raw.trim().to_string()
    } else {
        alias.to_string()
    };

    // Normalise: "22" → "v22", "v22.20.0" stays as-is
    let prefix = if version_spec.starts_with('v') {
        version_spec.clone()
    } else {
        format!("v{version_spec}")
    };

    let versions_dir = format!("{nvm_dir}/versions/node");

    // Exact match first
    let exact = format!("{versions_dir}/{prefix}/bin");
    if std::path::Path::new(&exact).exists() {
        return Some(exact);
    }

    // Partial match: find the highest version whose name starts with the prefix
    // (e.g. "v22" matches "v22.20.0").
    let mut matches: Vec<String> = std::fs::read_dir(&versions_dir)
        .ok()?
        .filter_map(|e| {
            let name = e.ok()?.file_name().to_string_lossy().to_string();
            name.starts_with(&prefix).then_some(name)
        })
        .collect();
    matches.sort();

    let bin = format!("{versions_dir}/{}/bin", matches.last()?);
    std::path::Path::new(&bin).exists().then_some(bin)
}

/// Parses a Go version string like `"go1.24.11"` into a `(major, minor, patch)`
/// tuple for ordering.  Unrecognised strings sort as `(0, 0, 0)`.
fn parse_go_version(name: &str) -> (u32, u32, u32) {
    let s = name.strip_prefix("go").unwrap_or(name);
    let mut parts = s.splitn(3, '.');
    let major = parts.next().and_then(|p| p.parse().ok()).unwrap_or(0);
    let minor = parts.next().and_then(|p| p.parse().ok()).unwrap_or(0);
    let patch = parts.next().and_then(|p| p.parse().ok()).unwrap_or(0);
    (major, minor, patch)
}

/// Returns gvm Go bin directories sorted **newest version first** under `gvm_root`.
///
/// Scans `<gvm_root>/gos/` and `<gvm_root>/pkgsets/` so that `gopls` (or any
/// Go tool) is found regardless of which version installed it and whether
/// `--default` was used.  Newest-first ordering ensures that a freshly
/// installed `gopls@latest` (in the newest pkgset) takes priority over an
/// older version installed in an older pkgset.
/// The caller resolves `gvm_root` from `$GVM_ROOT` or the `~/.gvm` fallback.
fn resolve_gvm_paths(gvm_root: &str) -> Vec<String> {
    if !std::path::Path::new(gvm_root).exists() {
        return vec![];
    }

    let mut paths = vec![format!("{gvm_root}/bin")];

    // Collect and sort Go version names (newest first) so that the latest
    // pkgset bin — where `gopls@latest` lives — appears first in PATH.
    let sorted_versions = |subdir: &str| -> Vec<String> {
        let Ok(entries) = std::fs::read_dir(format!("{gvm_root}/{subdir}")) else {
            return vec![];
        };
        let mut names: Vec<String> = entries
            .flatten()
            .filter_map(|e| {
                let n = e.file_name().to_string_lossy().into_owned();
                // Keep only entries that look like Go versions ("go1.x.y").
                n.starts_with("go").then_some(n)
            })
            .collect();
        names.sort_by(|a, b| parse_go_version(b).cmp(&parse_go_version(a)));
        names
    };

    // pkgsets first: this is where `go install golang.org/x/tools/gopls@latest`
    // puts the binary — take priority over the bare Go toolchain bin.
    for ver in sorted_versions("pkgsets") {
        let bin = format!("{gvm_root}/pkgsets/{ver}/global/bin");
        if std::path::Path::new(&bin).is_dir() {
            paths.push(bin);
        }
    }

    // gos: the Go toolchain binaries (`go`, `gofmt`) per version, newest first.
    for ver in sorted_versions("gos") {
        let bin = format!("{gvm_root}/gos/{ver}/bin");
        if std::path::Path::new(&bin).is_dir() {
            paths.push(bin);
        }
    }

    paths
}

/// Probes the user's login shell **once per process** and caches the result.
///
/// Spawning an interactive login shell (`-ilc`) sources `~/.zshrc` and
/// `~/.bash_profile`, so version managers (nvm, gvm, rbenv, pyenv, volta …)
/// are fully initialised — giving us the exact `$PATH` the user sees in a
/// terminal.  stderr is discarded so shell startup chatter (nvm banners,
/// "Now using node …") never contaminates the stdout we parse.
///
/// Returns `None` when every candidate shell fails or emits empty output
/// (sandboxed build, CI runner, missing shell binary).
fn login_shell_path_cache() -> &'static Mutex<Option<Option<String>>> {
    static CACHED: std::sync::OnceLock<Mutex<Option<Option<String>>>> = std::sync::OnceLock::new();
    CACHED.get_or_init(|| Mutex::new(None))
}

fn extract_path_from_login_shell() -> Option<String> {
    let cached = login_shell_path_cache();
    let mut guard = cached.lock().ok()?;
    if let Some(value) = guard.clone() {
        return value;
    }

    let value = probe_path_from_login_shell();
    *guard = Some(value.clone());
    value
}

pub fn refresh_patched_env_path() -> String {
    let cached = login_shell_path_cache();
    if let Ok(mut guard) = cached.lock() {
        *guard = Some(probe_path_from_login_shell());
    }
    patched_env_path()
}

fn probe_path_from_login_shell() -> Option<String> {
    let shell_var = std::env::var("SHELL").unwrap_or_default();
    let candidates: Vec<String> = {
                let mut v: Vec<String> = vec![];
                if !shell_var.is_empty() {
                    v.push(shell_var);
                }
                for s in ["/bin/zsh", "/bin/bash", "/bin/sh"] {
                    v.push(s.to_string());
                }
                // Deduplicate while preserving preference order.
                let mut seen = HashSet::new();
                v.into_iter().filter(|s| seen.insert(s.clone())).collect()
    };

    for shell in &candidates {
                let Ok(output) = std::process::Command::new(shell)
                    .args(["-ilc", "printenv PATH"])
                    .stdout(Stdio::piped())
                    .stderr(Stdio::null())
                    .output()
                else {
                    continue;
                };
                if !output.status.success() {
                    continue;
                }
                let Ok(raw) = String::from_utf8(output.stdout) else {
                    continue;
                };
                let trimmed = raw.trim().to_string();
                // Sanity-check: a real PATH is non-empty and contains at least one '/'.
                if !trimmed.is_empty() && trimmed.contains('/') {
                    return Some(trimmed);
                }
    }
    None
}

/// Returns an augmented `PATH` by first attempting a live login-shell probe
/// and then falling back to a hard-coded augmentation list.
///
/// **Primary (Task 1):** spawns `$SHELL -ilc "printenv PATH"` once per
/// process (result is cached via `OnceLock`) to capture the exact PATH the
/// user's shell exposes — nvm, gvm, rbenv, cargo, Homebrew, and all other
/// user-managed version managers are included automatically.
///
/// **Fallback (Task 2):** if the shell probe fails, prepends a curated list
/// of well-known directories for nvm (`current` symlink + alias resolution),
/// gvm, plain `~/go/bin`, cargo, Homebrew, and Netherize-managed LSP bins.
pub fn patched_env_path() -> String {
    let home = std::env::var("HOME").unwrap_or_default();
    let netherize_bin = format!("{home}/.local/share/netherize/bin");

    // Primary: one-shot login-shell extraction (nvm, gvm, cargo, etc. all active).
    if let Some(live_path) = extract_path_from_login_shell() {
        // Inject Netherize-managed LSP binaries at front — not present in the user shell.
        return if live_path
            .split(':')
            .any(|seg| seg == netherize_bin.as_str())
        {
            live_path
        } else {
            format!("{netherize_bin}:{live_path}")
        };
    }

    // Fallback: static augmentation for sandboxed / CI environments.
    static_patched_env_path()
}

/// Static PATH augmentation — fallback when the login-shell probe is unavailable.
///
/// Prepends directories commonly absent when the app launches as a macOS .app
/// bundle (launchd provides only a minimal system PATH, omitting Homebrew,
/// Cargo, nvm, gvm, and npm-global bins).
fn static_patched_env_path() -> String {
    let current = std::env::var("PATH").unwrap_or_default();
    let home = std::env::var("HOME").unwrap_or_default();

    // User-managed version managers take priority over system package managers
    // (Homebrew, /usr/local) so that freshly installed tool versions win.
    let mut candidates: Vec<String> = vec![];

    // Cargo — rust-analyzer and other Rust tools.
    candidates.push(format!("{home}/.cargo/bin"));

    // gvm — newest pkgset/gos bin first (gopls@latest lives in pkgset).
    let gvm_root = std::env::var("GVM_ROOT").unwrap_or_else(|_| format!("{home}/.gvm"));
    candidates.extend(resolve_gvm_paths(&gvm_root));
    // gvm bare bin directory (shell wrappers: go, gofmt, gvm itself).
    candidates.push(format!("{home}/.gvm/bin"));

    // Plain ~/go/bin fallback for Go installs without gvm.
    candidates.push(format!("{home}/go/bin"));

    // nvm — smart alias resolution first, then belt-and-suspenders current symlink.
    if let Some(nvm_bin) = resolve_nvm_bin(&home) {
        candidates.push(nvm_bin);
    }
    candidates.push(format!("{home}/.nvm/versions/node/current/bin"));

    // npm global with custom prefix.
    candidates.push(format!("{home}/.npm-global/bin"));

    // jenv — Java version manager (shims for java, javac, jdtls, etc.).
    candidates.push(format!("{home}/.jenv/shims"));
    candidates.push(format!("{home}/.jenv/bin"));

    // Homebrew (system fallback — lower priority than user-managed tools).
    candidates.push("/opt/homebrew/bin".to_string());
    candidates.push("/opt/homebrew/sbin".to_string());
    candidates.push("/usr/local/bin".to_string());
    candidates.push("/usr/local/sbin".to_string());

    // Netherize-managed LSP binaries.
    candidates.push(format!("{home}/.local/share/netherize/bin"));

    let mut extra: Vec<String> = candidates
        .into_iter()
        .filter(|p| !p.is_empty() && !current.split(':').any(|seg| seg == p.as_str()))
        .collect();

    if extra.is_empty() {
        return current;
    }
    extra.push(current);
    extra.join(":")
}

pub fn check_lsp_installed(binary: &str) -> bool {
    std::process::Command::new("which")
        .arg(binary)
        .env("PATH", patched_env_path())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

pub fn lsp_entry_and_status_for_path(path: &Path) -> Option<(LspEntry, bool)> {
    let profile = language_profile_for_path(path)?;
    if profile.lsp_binary.is_empty() {
        return None;
    }
    let entry = LspEntry {
        binary: profile.lsp_binary,
        language_label: profile.language_label,
        install_cmd: profile.install_command,
    };
    Some((entry.clone(), check_lsp_installed(entry.binary)))
}

pub struct LspClientProcess {
    child: AsyncMutex<Child>,
    writer: AsyncMutex<Option<ChildStdin>>,
    next_rpc_id: AtomicU64,
    latest_revision: AtomicU64,
    latest_request_id: AtomicU64,
    pending_responses: Mutex<HashMap<u64, std_mpsc::SyncSender<Value>>>,
    open_documents: Mutex<HashSet<String>>,
    /// Per-category latest in-flight RPC id (e.g. "definition", "hover"). When
    /// the client dispatches a new cancellable request, the previous id stored
    /// here is the one we send `$/cancelRequest` for so the server stops work.
    cancellable_inflight: Mutex<HashMap<&'static str, u64>>,
}

impl LspClientProcess {
    fn new(child: Child, writer: ChildStdin) -> Self {
        Self {
            child: AsyncMutex::new(child),
            writer: AsyncMutex::new(Some(writer)),
            next_rpc_id: AtomicU64::new(1),
            latest_revision: AtomicU64::new(0),
            latest_request_id: AtomicU64::new(0),
            pending_responses: Mutex::new(HashMap::new()),
            open_documents: Mutex::new(HashSet::new()),
            cancellable_inflight: Mutex::new(HashMap::new()),
        }
    }

    /// Atomically records `new_id` as the latest in-flight request for `key`
    /// and returns the previous id (if any). Callers send `$/cancelRequest`
    /// for the returned id so the server can free its worker slot.
    pub fn swap_inflight(&self, key: &'static str, new_id: u64) -> Option<u64> {
        let mut guard = self.cancellable_inflight.lock().ok()?;
        guard.insert(key, new_id)
    }

    /// Drop the inflight entry for `key` only when it still matches `id`.
    /// We never want to clobber a *newer* entry recorded by another task.
    pub fn clear_inflight_if_matches(&self, key: &'static str, id: u64) {
        if let Ok(mut guard) = self.cancellable_inflight.lock() {
            if guard.get(key) == Some(&id) {
                guard.remove(key);
            }
        }
    }

    /// Fire-and-forget `$/cancelRequest` notification for the given JSON-RPC
    /// id. The server should reply to that id with an error response, which
    /// flows back through `deliver_response` and unblocks the original waiter.
    pub fn send_cancel_request(&self, id: u64) {
        if let Err(err) = self
            .send_notification("$/cancelRequest", json!({ "id": id }))
        {
            eprintln!("[LSP] send $/cancelRequest({id}) failed: {err}");
        }
    }

    /// Reply to a server→client request with `result` (use `Value::Null` for a
    /// no-op acknowledgement, e.g. `window/workDoneProgress/create`).
    pub fn send_response(&self, id: u64, result: Value) -> Result<(), String> {
        block_on_runtime(self.send_response_async(id, result))
    }

    pub async fn send_response_async(&self, id: u64, result: Value) -> Result<(), String> {
        let payload = json!({
            "jsonrpc": "2.0",
            "id": id,
            "result": result,
        });
        let mut writer = self.writer.lock().await;
        let writer = writer
            .as_mut()
            .ok_or_else(|| format!("lsp writer unavailable while responding to id {id}"))?;
        write_json_rpc_message_async(writer, &payload).await
    }

    pub fn update_request_meta(&self, request_id: u64, revision_id: u64) {
        self.latest_request_id.store(request_id, Ordering::Relaxed);
        self.latest_revision.store(revision_id, Ordering::Relaxed);
    }

    pub fn latest_revision(&self) -> u64 {
        self.latest_revision.load(Ordering::Relaxed)
    }

    pub fn latest_request_id(&self) -> u64 {
        self.latest_request_id.load(Ordering::Relaxed)
    }

    async fn send_notification_async(&self, method: &str, params: Value) -> Result<(), String> {
        let payload = json!({
            "jsonrpc": "2.0",
            "method": method,
            "params": params,
        });
        let mut writer = self.writer.lock().await;
        let writer = writer
            .as_mut()
            .ok_or_else(|| format!("lsp writer unavailable while sending notification {method}"))?;
        write_json_rpc_message_async(writer, &payload).await
    }

    pub fn send_notification(&self, method: &str, params: Value) -> Result<(), String> {
        block_on_runtime(self.send_notification_async(method, params))
    }

    async fn send_request_async(&self, method: &str, params: Value) -> Result<u64, String> {
        let request_id = self.next_rpc_id.fetch_add(1, Ordering::Relaxed);
        let payload = json!({
            "jsonrpc": "2.0",
            "id": request_id,
            "method": method,
            "params": params,
        });
        let mut writer = self.writer.lock().await;
        let writer = writer
            .as_mut()
            .ok_or_else(|| format!("lsp writer unavailable while sending request {method}"))?;
        write_json_rpc_message_async(writer, &payload).await?;
        Ok(request_id)
    }

    pub fn send_request(&self, method: &str, params: Value) -> Result<u64, String> {
        block_on_runtime(self.send_request_async(method, params))
    }

    /// Pre-allocate the next JSON-RPC request id without sending anything.
    /// Call this before `register_pending_request` so both share the same id.
    pub fn allocate_request_id(&self) -> u64 {
        self.next_rpc_id.fetch_add(1, Ordering::Relaxed)
    }

    async fn send_request_with_id_async(
        &self,
        id: u64,
        method: &str,
        params: Value,
    ) -> Result<(), String> {
        let payload = json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params,
        });
        let mut writer = self.writer.lock().await;
        let writer = writer
            .as_mut()
            .ok_or_else(|| format!("lsp writer unavailable while sending request {method}"))?;
        write_json_rpc_message_async(writer, &payload).await
    }

    pub fn send_request_with_id(&self, id: u64, method: &str, params: Value) -> Result<(), String> {
        block_on_runtime(self.send_request_with_id_async(id, method, params))
    }

    /// Register a channel for `id`. Must be called BEFORE `send_request_with_id`
    /// to prevent a race where the response arrives before the rx is stored.
    pub fn register_pending_request(&self, id: u64) -> std_mpsc::Receiver<Value> {
        let (tx, rx) = std_mpsc::sync_channel(4);
        if let Ok(mut guard) = self.pending_responses.lock() {
            guard.insert(id, tx);
        }
        rx
    }

    /// Route the response to the correct waiting caller by request id.
    /// Removes the sender from the map so a timeout cleanup is a no-op.
    pub fn deliver_response(&self, id: u64, value: Value) {
        if let Ok(mut guard) = self.pending_responses.lock() {
            if let Some(tx) = guard.remove(&id) {
                let _ = tx.try_send(value);
            }
        }
    }

    /// Remove a pending entry on timeout/cancellation.
    /// Safe to call even if the entry was already consumed by `deliver_response`.
    pub fn clear_pending_request(&self, id: u64) {
        if let Ok(mut guard) = self.pending_responses.lock() {
            guard.remove(&id);
        }
    }

    pub fn is_document_open(&self, uri: &str) -> bool {
        self.open_documents
            .lock()
            .map(|guard| guard.contains(uri))
            .unwrap_or(false)
    }

    pub fn mark_document_open(&self, uri: &str) {
        if let Ok(mut guard) = self.open_documents.lock() {
            guard.insert(uri.to_string());
        }
    }

    pub fn mark_document_closed(&self, uri: &str) {
        if let Ok(mut guard) = self.open_documents.lock() {
            guard.remove(uri);
        }
    }

    fn clear_open_documents(&self) {
        if let Ok(mut guard) = self.open_documents.lock() {
            guard.clear();
        }
    }

    async fn shutdown_and_exit_async(&self) -> Result<Option<i32>, String> {
        let shutdown_request_id = self.allocate_request_id();
        let shutdown_rx = self.register_pending_request(shutdown_request_id);

        if let Err(err) = self
            .send_request_with_id_async(shutdown_request_id, "shutdown", Value::Null)
            .await
        {
            self.clear_pending_request(shutdown_request_id);
            return Err(err);
        }

        let _shutdown_response = shutdown_rx.recv_timeout(Duration::from_secs(2)).ok();
        self.clear_pending_request(shutdown_request_id);

        let _ = self.send_notification_async("exit", Value::Null).await;

        {
            let mut writer = self.writer.lock().await;
            *writer = None;
        }

        if let Ok(mut guard) = self.pending_responses.lock() {
            guard.clear();
        }
        self.clear_open_documents();

        let deadline = Instant::now() + Duration::from_secs(2);
        let mut child = self.child.lock().await;
        loop {
            match child.try_wait() {
                Ok(Some(status)) => return Ok(status.code()),
                Ok(None) if Instant::now() < deadline => {
                    tokio::time::sleep(Duration::from_millis(25)).await;
                }
                Ok(None) => {
                    let _ = child.start_kill();
                    return match child.wait().await {
                        Ok(status) => Ok(status.code()),
                        Err(_) => Ok(None),
                    };
                }
                Err(err) => return Err(format!("lsp try_wait failed: {err}")),
            }
        }
    }

    pub fn shutdown_and_exit(&self) -> Result<Option<i32>, String> {
        block_on_runtime(self.shutdown_and_exit_async())
    }

    pub fn graceful_shutdown(&self) -> Result<Option<i32>, String> {
        self.shutdown_and_exit()
    }
}

pub struct SpawnedLspServer {
    pub process: Arc<LspClientProcess>,
    pub server_name: String,
    pub root_path: PathBuf,
    pub reader: BufReader<ChildStdout>,
    pub stderr: Option<BufReader<ChildStderr>>,
    /// Raw JSON string kept cho UI display / backward-compat.
    pub capabilities_summary: String,
    /// Parsed một lần tại init — dùng để gate requests trong scheduler.
    pub capabilities: ServerCapabilities,
}

pub struct ParsedDiagnostics {
    pub uri: String,
    pub version: Option<u64>,
    pub diagnostics: Vec<LspDiagnostic>,
}

pub struct ParsedLogMessage {
    pub level: String,
    pub message: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProgressKind {
    Begin,
    Report,
    End,
}

/// Decoded `$/progress` notification (`WorkDoneProgress*` payloads).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedProgress {
    pub token: String,
    pub kind: ProgressKind,
    pub title: Option<String>,
    pub message: Option<String>,
    pub percentage: Option<u32>,
}

/// Decoded server→client request that the client must answer (e.g.
/// `window/workDoneProgress/create`). Carries the `id` so the caller can
/// reply with a result envelope.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedServerRequest {
    pub id: u64,
    pub method: String,
}

pub fn parse_progress_notification(message: &Value) -> Option<ParsedProgress> {
    if message.get("method")?.as_str()? != "$/progress" {
        return None;
    }
    let params = message.get("params")?;
    let token = match params.get("token")? {
        Value::String(s) => s.clone(),
        Value::Number(n) => n.to_string(),
        _ => return None,
    };
    let value = params.get("value")?;
    let kind_str = value.get("kind")?.as_str()?;
    let kind = match kind_str {
        "begin" => ProgressKind::Begin,
        "report" => ProgressKind::Report,
        "end" => ProgressKind::End,
        _ => return None,
    };
    let title = value
        .get("title")
        .and_then(Value::as_str)
        .map(str::to_string);
    let message = value
        .get("message")
        .and_then(Value::as_str)
        .map(str::to_string);
    let percentage = value
        .get("percentage")
        .and_then(Value::as_u64)
        .and_then(|v| u32::try_from(v).ok());
    Some(ParsedProgress {
        token,
        kind,
        title,
        message,
        percentage,
    })
}

/// Detect a server→client request that the client must respond to. The
/// scheduler currently auto-replies with `null` for the small set of
/// requests we know are safe to acknowledge.
pub fn parse_server_request(message: &Value) -> Option<ParsedServerRequest> {
    let id = message.get("id").and_then(Value::as_u64)?;
    let method = message.get("method")?.as_str()?.to_string();
    Some(ParsedServerRequest { id, method })
}

pub async fn spawn_lsp_server(
    requested_command: Option<&str>,
    root_path: &Path,
    request_id: u64,
    revision_id: u64,
) -> Result<SpawnedLspServer, String> {
    let server_name = resolve_lsp_server_command(requested_command, root_path)
        .ok_or_else(|| format!("no supported LSP server found for {}", root_path.display()))?;
    let mut command = Command::new(&server_name);
    if let Some(profile) = language_profile_for_binary(&server_name) {
        command.args(profile.launch_args);
    }
    command.env("PATH", patched_env_path());
    command.current_dir(root_path);
    command.stdin(Stdio::piped());
    command.stdout(Stdio::piped());
    command.stderr(Stdio::piped());

    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            return Err(format!("LSPMISSING:{}", server_name));
        }
        Err(err) => return Err(format!("spawn LSP server {:?} failed: {err}", server_name)),
    };

    let stdin = child
        .stdin
        .take()
        .ok_or_else(|| "lsp child stdin unavailable".to_string())?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| "lsp child stdout unavailable".to_string())?;
    let stderr = child.stderr.take().map(BufReader::new);

    let process = Arc::new(LspClientProcess::new(child, stdin));
    process.update_request_meta(request_id, revision_id);
    let mut reader = BufReader::new(stdout);
    let root_uri = path_to_lsp_uri(root_path);

    let init_request_id = process
        .send_request_async(
            "initialize",
            json!({
                "processId": std::process::id(),
                "clientInfo": {
                    "name": "netherize-editor",
                    "version": "0.1.0"
                },
                "rootUri": root_uri,
                "capabilities": {
                    "textDocument": {
                        "synchronization": {
                            "didSave": true,
                            "willSave": false,
                            "willSaveWaitUntil": false
                        },
                        "publishDiagnostics": {
                            "relatedInformation": true
                        }
                    },
                    "window": {
                        "workDoneProgress": true
                    }
                },
                "trace": "off",
                "workspaceFolders": [{
                    "uri": path_to_lsp_uri(root_path),
                    "name": root_path
                        .file_name()
                        .and_then(|value| value.to_str())
                        .unwrap_or("workspace")
                }]
            }),
        )
        .await?;

    let initialize_response = match tokio::time::timeout(
        LSP_INITIALIZE_TIMEOUT,
        wait_for_json_rpc_response_async(&mut reader, init_request_id),
    )
    .await
    {
        Ok(response) => response?,
        Err(_) => {
            return Err(format!(
                "lsp initialize timed out after {}s",
                LSP_INITIALIZE_TIMEOUT.as_secs()
            ));
        }
    }
    .ok_or_else(|| "lsp initialize returned EOF".to_string())?;

    if let Some(error) = initialize_response.get("error") {
        return Err(format!("lsp initialize error: {}", error));
    }

    let caps_value = initialize_response
        .get("result")
        .and_then(|result| result.get("capabilities"))
        .cloned()
        .unwrap_or(serde_json::Value::Object(Default::default()));
    let capabilities_summary = caps_value.to_string();
    let capabilities = ServerCapabilities::from_json(&caps_value);

    process
        .send_notification_async("initialized", json!({}))
        .await?;

    Ok(SpawnedLspServer {
        process,
        server_name,
        root_path: root_path.to_path_buf(),
        reader,
        stderr,
        capabilities_summary,
        capabilities,
    })
}

pub async fn read_json_rpc_message_async<R>(reader: &mut R) -> Result<Option<Value>, String>
where
    R: AsyncBufRead + Unpin + ?Sized,
{
    let mut headers = HashMap::new();
    loop {
        let mut line = String::new();
        let read = reader
            .read_line(&mut line)
            .await
            .map_err(|err| format!("read lsp header failed: {err}"))?;
        if read == 0 {
            return Ok(None);
        }

        if is_header_separator(&line) {
            break;
        }

        if let Some((name, value)) = parse_header_line(&line) {
            headers.insert(name, value);
        }
    }

    let content_length = parse_content_length(&headers)?;
    let mut body = vec![0_u8; content_length];
    reader
        .read_exact(&mut body)
        .await
        .map_err(|err| format!("read lsp body failed: {err}"))?;
    let value = serde_json::from_slice::<Value>(&body)
        .map_err(|err| format!("decode lsp json failed: {err}"))?;
    Ok(Some(value))
}

pub fn read_json_rpc_message(reader: &mut dyn BufRead) -> Result<Option<Value>, String> {
    let mut headers = HashMap::new();
    loop {
        let mut line = String::new();
        let read = reader
            .read_line(&mut line)
            .map_err(|err| format!("read lsp header failed: {err}"))?;
        if read == 0 {
            return Ok(None);
        }

        if is_header_separator(&line) {
            break;
        }

        if let Some((name, value)) = parse_header_line(&line) {
            headers.insert(name, value);
        }
    }

    let content_length = parse_content_length(&headers)?;
    let mut body = vec![0_u8; content_length];
    reader
        .read_exact(&mut body)
        .map_err(|err| format!("read lsp body failed: {err}"))?;
    let value = serde_json::from_slice::<Value>(&body)
        .map_err(|err| format!("decode lsp json failed: {err}"))?;
    Ok(Some(value))
}

pub async fn write_json_rpc_message_async<W>(writer: &mut W, payload: &Value) -> Result<(), String>
where
    W: AsyncWrite + Unpin + ?Sized,
{
    let body =
        serde_json::to_vec(payload).map_err(|err| format!("encode lsp json failed: {err}"))?;
    writer
        .write_all(format!("Content-Length: {}\r\n\r\n", body.len()).as_bytes())
        .await
        .map_err(|err| format!("write lsp header failed: {err}"))?;
    writer
        .write_all(&body)
        .await
        .map_err(|err| format!("write lsp body failed: {err}"))?;
    writer
        .flush()
        .await
        .map_err(|err| format!("flush lsp message failed: {err}"))?;
    Ok(())
}

pub fn write_json_rpc_message(writer: &mut dyn Write, payload: &Value) -> Result<(), String> {
    let body =
        serde_json::to_vec(payload).map_err(|err| format!("encode lsp json failed: {err}"))?;
    writer
        .write_all(format!("Content-Length: {}\r\n\r\n", body.len()).as_bytes())
        .map_err(|err| format!("write lsp header failed: {err}"))?;
    writer
        .write_all(&body)
        .map_err(|err| format!("write lsp body failed: {err}"))?;
    writer
        .flush()
        .map_err(|err| format!("flush lsp message failed: {err}"))?;
    Ok(())
}

async fn wait_for_json_rpc_response_async<R>(
    reader: &mut R,
    expected_id: u64,
) -> Result<Option<Value>, String>
where
    R: AsyncBufRead + Unpin + ?Sized,
{
    loop {
        let Some(message) = read_json_rpc_message_async(reader).await? else {
            return Ok(None);
        };

        if message.get("id").and_then(Value::as_u64) == Some(expected_id) {
            return Ok(Some(message));
        }
    }
}

pub fn parse_publish_diagnostics(message: &Value) -> Option<ParsedDiagnostics> {
    if message.get("method")?.as_str()? != "textDocument/publishDiagnostics" {
        return None;
    }

    let params = message.get("params")?;
    let uri = params.get("uri")?.as_str()?.to_string();
    let version = params.get("version").and_then(Value::as_u64);
    let diagnostics = params
        .get("diagnostics")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(parse_diagnostic)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    Some(ParsedDiagnostics {
        uri,
        version,
        diagnostics,
    })
}

pub fn parse_window_log_message(message: &Value) -> Option<ParsedLogMessage> {
    let method = message.get("method")?.as_str()?;
    if method != "window/logMessage" && method != "window/showMessage" {
        return None;
    }

    let params = message.get("params")?;
    let level = params
        .get("type")
        .and_then(Value::as_u64)
        .map(|value| match value {
            1 => "error",
            2 => "warning",
            3 => "info",
            4 => "log",
            _ => "unknown",
        })
        .unwrap_or("info")
        .to_string();
    let message_text = params
        .get("message")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();

    Some(ParsedLogMessage {
        level,
        message: message_text,
    })
}

pub fn build_did_open_notification(
    uri: &str,
    language_id: &str,
    version: i32,
    text: &str,
) -> Value {
    json!({
        "textDocument": {
            "uri": uri,
            "languageId": language_id,
            "version": version,
            "text": text
        }
    })
}

pub fn build_did_change_notification(uri: &str, version: i32, text: &str) -> Value {
    json!({
        "textDocument": {
            "uri": uri,
            "version": version
        },
        "contentChanges": [{
            "text": text
        }]
    })
}

pub fn build_did_close_notification(uri: &str) -> Value {
    json!({
        "textDocument": {
            "uri": uri
        }
    })
}

pub fn path_to_lsp_uri(path: &Path) -> String {
    let path = path.to_string_lossy();
    if path.starts_with('/') {
        format!("file://{path}")
    } else {
        format!("file:///{path}")
    }
}

pub fn resolve_lsp_server_command(
    requested_command: Option<&str>,
    root_path: &Path,
) -> Option<String> {
    if let Some(command) = requested_command {
        let command = command.trim();
        if !command.is_empty() {
            return Some(command.to_string());
        }
    }
    detect_lsp_server_for_workspace(root_path)
}

pub fn detect_lsp_server_for_workspace(root_path: &Path) -> Option<String> {
    for profile in all_language_profiles() {
        if profile
            .root_markers
            .iter()
            .copied()
            .filter(|marker| *marker != ".git")
            .any(|marker| root_path.join(marker).exists())
        {
            return Some(profile.lsp_binary.to_string());
        }
    }

    fs::read_dir(root_path)
        .ok()?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .find_map(|path| detect_lsp_server_for_path(&path))
}

pub fn detect_lsp_server_for_path(path: &Path) -> Option<String> {
    language_profile_for_path(path)
        .map(|profile| profile.lsp_binary.to_string())
        .filter(|binary| !binary.is_empty())
}

fn parse_diagnostic(value: &Value) -> Option<LspDiagnostic> {
    let range = value.get("range")?;
    let start = parse_position(range.get("start")?)?;
    let end = parse_position(range.get("end")?)?;

    Some(LspDiagnostic {
        range: LspRange { start, end },
        severity: value
            .get("severity")
            .and_then(Value::as_u64)
            .and_then(|value| u32::try_from(value).ok()),
        code: match value.get("code") {
            Some(Value::String(code)) => Some(code.clone()),
            Some(Value::Number(code)) => Some(code.to_string()),
            _ => None,
        },
        source: value
            .get("source")
            .and_then(Value::as_str)
            .map(ToString::to_string),
        message: value
            .get("message")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
    })
}

fn parse_position(value: &Value) -> Option<LspPosition> {
    Some(LspPosition {
        line: u32::try_from(value.get("line")?.as_u64()?).ok()?,
        character: u32::try_from(value.get("character")?.as_u64()?).ok()?,
    })
}

fn runtime_handle() -> Result<tokio::runtime::Handle, String> {
    tokio::runtime::Handle::try_current()
        .map_err(|_| "tokio runtime handle unavailable for lsp io".to_string())
}

fn block_on_runtime<F, T>(future: F) -> Result<T, String>
where
    F: Future<Output = Result<T, String>>,
{
    runtime_handle()?.block_on(future)
}

fn parse_header_line(line: &str) -> Option<(String, String)> {
    let trimmed = line.trim_end_matches(['\r', '\n']);
    let (name, value) = trimmed.split_once(':')?;
    Some((name.trim().to_ascii_lowercase(), value.trim().to_string()))
}

fn is_header_separator(line: &str) -> bool {
    line.trim_end_matches(['\r', '\n']).is_empty()
}

fn parse_content_length(headers: &HashMap<String, String>) -> Result<usize, String> {
    let Some(length_header) = headers.get("content-length") else {
        return Err("lsp message missing Content-Length header".to_string());
    };
    length_header
        .trim()
        .parse::<usize>()
        .map_err(|err| format!("invalid Content-Length {:?}: {err}", length_header))
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        io::Cursor,
        path::{Path, PathBuf},
        time::{SystemTime, UNIX_EPOCH},
    };

    use serde_json::json;

    use super::{
        detect_lsp_server_for_path, detect_lsp_server_for_workspace, parse_publish_diagnostics,
        read_json_rpc_message, resolve_lsp_server_command, write_json_rpc_message,
    };

    fn unique_temp_dir(name: &str) -> PathBuf {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time")
            .as_nanos();
        std::env::temp_dir().join(format!("netherize_{name}_{stamp}"))
    }

    #[test]
    fn json_rpc_frame_roundtrip() {
        let payload = json!({
            "jsonrpc": "2.0",
            "method": "test/ping",
            "params": { "value": 42 }
        });

        let mut raw = Vec::<u8>::new();
        write_json_rpc_message(&mut raw, &payload).expect("write frame");

        let mut reader = Cursor::new(raw);
        let decoded = read_json_rpc_message(&mut reader)
            .expect("read frame")
            .expect("frame should exist");

        assert_eq!(decoded["method"], "test/ping");
        assert_eq!(decoded["params"]["value"], 42);
    }

    #[test]
    fn json_rpc_frame_accepts_crlf_headers() {
        let body = "{\"jsonrpc\":\"2.0\"}";
        let raw = format!(
            "Content-Length: {}\r\nContent-Type: application/vscode-jsonrpc; charset=utf-8\r\n\r\n{}",
            body.len(),
            body
        )
        .into_bytes();
        let mut reader = Cursor::new(raw);
        let decoded = read_json_rpc_message(&mut reader)
            .expect("read frame")
            .expect("frame should exist");

        assert_eq!(decoded["jsonrpc"], "2.0");
    }

    #[test]
    fn publish_diagnostics_parser_extracts_core_fields() {
        let message = json!({
            "jsonrpc": "2.0",
            "method": "textDocument/publishDiagnostics",
            "params": {
                "uri": "file:///tmp/main.rs",
                "version": 3,
                "diagnostics": [{
                    "range": {
                        "start": { "line": 1, "character": 4 },
                        "end": { "line": 1, "character": 9 }
                    },
                    "severity": 1,
                    "code": "E0425",
                    "source": "rustc",
                    "message": "cannot find value `hello` in this scope"
                }]
            }
        });

        let parsed = parse_publish_diagnostics(&message).expect("diagnostics should parse");
        assert_eq!(parsed.uri, "file:///tmp/main.rs");
        assert_eq!(parsed.version, Some(3));
        assert_eq!(parsed.diagnostics.len(), 1);
        assert_eq!(
            parsed.diagnostics[0].message,
            "cannot find value `hello` in this scope"
        );
    }

    #[test]
    fn workspace_server_detection_prefers_rust_markers() {
        let root = unique_temp_dir("rust_lsp_workspace");
        fs::create_dir_all(&root).expect("create workspace");
        fs::write(root.join("Cargo.toml"), "[package]\nname='demo'\n").expect("write cargo");

        let server = detect_lsp_server_for_workspace(&root);
        assert_eq!(server.as_deref(), Some("rust-analyzer"));

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn workspace_server_detection_supports_go_markers() {
        let root = unique_temp_dir("go_lsp_workspace");
        fs::create_dir_all(&root).expect("create workspace");
        fs::write(root.join("go.mod"), "module demo\n\ngo 1.24\n").expect("write go.mod");

        let server = detect_lsp_server_for_workspace(&root);
        assert_eq!(server.as_deref(), Some("gopls"));

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn file_server_detection_matches_supported_extensions() {
        assert_eq!(
            detect_lsp_server_for_path(Path::new("/tmp/main.rs")).as_deref(),
            Some("rust-analyzer")
        );
        assert_eq!(
            detect_lsp_server_for_path(Path::new("/tmp/main.go")).as_deref(),
            Some("gopls")
        );
        assert_eq!(
            detect_lsp_server_for_path(Path::new("/tmp/schema.sql")).as_deref(),
            Some("sqls")
        );
        assert_eq!(
            detect_lsp_server_for_path(Path::new("/tmp/Dockerfile")).as_deref(),
            Some("docker-langserver")
        );
        assert_eq!(
            detect_lsp_server_for_path(Path::new("/tmp/README.md")),
            None
        );
    }

    #[test]
    fn explicit_server_command_overrides_workspace_detection() {
        let root = unique_temp_dir("explicit_lsp_workspace");
        fs::create_dir_all(&root).expect("create workspace");

        let server = resolve_lsp_server_command(Some("custom-lsp"), &root);
        assert_eq!(server.as_deref(), Some("custom-lsp"));

        let _ = fs::remove_dir_all(root);
    }

    // ── patched_env_path helpers ──────────────────────────────────────────────

    fn temp_dir(label: &str) -> PathBuf {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .subsec_nanos();
        std::env::temp_dir().join(format!("netherize_{label}_{stamp}"))
    }

    #[test]
    fn resolve_nvm_bin_exact_version() {
        use super::resolve_nvm_bin;

        let root = temp_dir("nvm_exact");
        let bin = root.join("versions/node/v22.20.0/bin");
        fs::create_dir_all(&bin).unwrap();
        fs::create_dir_all(root.join("alias")).unwrap();
        fs::write(root.join("alias/default"), "v22.20.0").unwrap();

        let home = root.to_string_lossy();
        // Override NVM_DIR by passing home so that resolve_nvm_bin looks in <home>/.nvm
        // The function appends "/.nvm" to `home`, so we place the structure under root/.nvm
        let nvm_root = root.join(".nvm");
        fs::create_dir_all(nvm_root.join("versions/node/v22.20.0/bin")).unwrap();
        fs::create_dir_all(nvm_root.join("alias")).unwrap();
        fs::write(nvm_root.join("alias/default"), "v22.20.0").unwrap();

        let result = resolve_nvm_bin(&home).expect("should resolve");
        assert!(result.ends_with("v22.20.0/bin"), "got: {result}");

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn resolve_nvm_bin_partial_version() {
        use super::resolve_nvm_bin;

        let root = temp_dir("nvm_partial");
        let nvm_root = root.join(".nvm");
        // Three v22 installs; resolver should pick highest (v22.20.0).
        for v in ["v22.18.0", "v22.19.1", "v22.20.0"] {
            fs::create_dir_all(nvm_root.join(format!("versions/node/{v}/bin"))).unwrap();
        }
        fs::create_dir_all(nvm_root.join("alias")).unwrap();
        // Alias contains partial "22" (no leading 'v', no patch).
        fs::write(nvm_root.join("alias/default"), "22").unwrap();

        let home = root.to_string_lossy();
        let result = resolve_nvm_bin(&home).expect("should resolve partial");
        assert!(result.ends_with("v22.20.0/bin"), "got: {result}");

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn resolve_nvm_bin_lts_alias() {
        use super::resolve_nvm_bin;

        let root = temp_dir("nvm_lts");
        let nvm_root = root.join(".nvm");
        fs::create_dir_all(nvm_root.join("versions/node/v20.19.5/bin")).unwrap();
        fs::create_dir_all(nvm_root.join("alias/lts")).unwrap();
        // default → lts/iron → v20.19.5
        fs::write(nvm_root.join("alias/default"), "lts/iron").unwrap();
        fs::write(nvm_root.join("alias/lts/iron"), "v20.19.5").unwrap();

        let home = root.to_string_lossy();
        let result = resolve_nvm_bin(&home).expect("should resolve lts");
        assert!(result.ends_with("v20.19.5/bin"), "got: {result}");

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn resolve_gvm_paths_scans_all_installed_versions() {
        use super::resolve_gvm_paths;

        let gvm_root = temp_dir("gvm_root");
        // Simulate two installed versions; only go1.25.4 has gopls in its pkgset.
        for v in ["go1.24.11", "go1.25.4"] {
            fs::create_dir_all(gvm_root.join(format!("gos/{v}/bin"))).unwrap();
            fs::create_dir_all(gvm_root.join(format!("pkgsets/{v}/global/bin"))).unwrap();
        }
        fs::create_dir_all(gvm_root.join("bin")).unwrap();
        // No environments/default — proves we don't depend on it.

        let paths = resolve_gvm_paths(&gvm_root.to_string_lossy());

        assert!(
            paths.iter().any(|p| p.contains("go1.24.11/bin")),
            "{paths:?}"
        );
        assert!(
            paths.iter().any(|p| p.contains("go1.25.4/bin")),
            "{paths:?}"
        );
        assert!(
            paths
                .iter()
                .any(|p| p.contains("go1.25.4") && p.ends_with("global/bin")),
            "{paths:?}"
        );

        let _ = fs::remove_dir_all(gvm_root);
    }
}
