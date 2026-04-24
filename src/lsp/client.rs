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

pub fn check_lsp_installed(binary: &str) -> bool {
    std::process::Command::new("which")
        .arg(binary)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

pub fn lsp_entry_and_status_for_path(path: &Path) -> Option<(LspEntry, bool)> {
    let profile = language_profile_for_path(path)?;
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
        }
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
    command.current_dir(root_path);
    command.stdin(Stdio::piped());
    command.stdout(Stdio::piped());
    command.stderr(Stdio::piped());

    let mut child = command
        .spawn()
        .map_err(|err| format!("spawn LSP server {:?} failed: {err}", server_name))?;

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

    let initialize_response = wait_for_json_rpc_response_async(&mut reader, init_request_id)
        .await?
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
    language_profile_for_path(path).map(|profile| profile.lsp_binary.to_string())
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
}
