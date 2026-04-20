use std::{
    collections::HashMap,
    io::{BufRead, BufReader, Read, Write},
    path::{Path, PathBuf},
    process::{Child, ChildStderr, ChildStdin, ChildStdout, Command, Stdio},
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
    },
};

use serde_json::{Value, json};

use crate::async_runtime::message::{LspDiagnostic, LspPosition, LspRange};

pub struct LspClientProcess {
    child: Mutex<Child>,
    writer: Mutex<ChildStdin>,
    next_rpc_id: AtomicU64,
    latest_revision: AtomicU64,
    latest_request_id: AtomicU64,
}

impl LspClientProcess {
    fn new(child: Child, writer: ChildStdin) -> Self {
        Self {
            child: Mutex::new(child),
            writer: Mutex::new(writer),
            next_rpc_id: AtomicU64::new(1),
            latest_revision: AtomicU64::new(0),
            latest_request_id: AtomicU64::new(0),
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

    pub fn send_notification(&self, method: &str, params: Value) -> Result<(), String> {
        let payload = json!({
            "jsonrpc": "2.0",
            "method": method,
            "params": params,
        });
        let mut writer = self
            .writer
            .lock()
            .map_err(|_| "lsp writer lock poisoned".to_string())?;
        write_json_rpc_message(&mut *writer, &payload)
    }

    pub fn send_request(&self, method: &str, params: Value) -> Result<u64, String> {
        let request_id = self.next_rpc_id.fetch_add(1, Ordering::Relaxed);
        let payload = json!({
            "jsonrpc": "2.0",
            "id": request_id,
            "method": method,
            "params": params,
        });
        let mut writer = self
            .writer
            .lock()
            .map_err(|_| "lsp writer lock poisoned".to_string())?;
        write_json_rpc_message(&mut *writer, &payload)?;
        Ok(request_id)
    }

    pub fn graceful_shutdown(&self) -> Result<Option<i32>, String> {
        // Theo spec: gửi request shutdown rồi notification exit.
        let _ = self.send_request("shutdown", Value::Null);
        let _ = self.send_notification("exit", Value::Null);

        let mut child = self
            .child
            .lock()
            .map_err(|_| "lsp child lock poisoned".to_string())?;
        let status = match child.try_wait() {
            Ok(Some(status)) => Some(status.code().unwrap_or_default()),
            Ok(None) => {
                let _ = child.kill();
                match child.wait() {
                    Ok(status) => Some(status.code().unwrap_or_default()),
                    Err(_) => None,
                }
            }
            Err(err) => return Err(format!("lsp try_wait failed: {err}")),
        };
        Ok(status)
    }
}

pub struct SpawnedLspServer {
    pub process: Arc<LspClientProcess>,
    pub server_name: String,
    pub root_path: PathBuf,
    pub reader: Box<dyn BufRead + Send>,
    pub stderr: Option<Box<dyn Read + Send>>,
    pub capabilities_summary: String,
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

pub fn spawn_lsp_server(
    requested_command: Option<&str>,
    root_path: &Path,
    request_id: u64,
    revision_id: u64,
) -> Result<SpawnedLspServer, String> {
    let server_name = resolve_lsp_server_command(requested_command);
    let mut command = Command::new(&server_name);
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
    let stderr = child.stderr.take().map(|stderr| {
        let reader: Box<dyn Read + Send> = Box::new(stderr);
        reader
    });

    let process = Arc::new(LspClientProcess::new(child, stdin));
    process.update_request_meta(request_id, revision_id);
    let mut reader = Box::new(BufReader::new(stdout)) as Box<dyn BufRead + Send>;

    let init_request_id = process.send_request(
        "initialize",
        json!({
            "processId": std::process::id(),
            "clientInfo": {
                "name": "netherize-editor",
                "version": "0.1.0"
            },
            "rootUri": path_to_lsp_uri(root_path),
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
    )?;

    let initialize_response = wait_for_json_rpc_response(&mut *reader, init_request_id)?
        .ok_or_else(|| "lsp initialize returned EOF".to_string())?;

    if let Some(error) = initialize_response.get("error") {
        return Err(format!("lsp initialize error: {}", error));
    }

    let capabilities_summary = initialize_response
        .get("result")
        .and_then(|result| result.get("capabilities"))
        .map(ToString::to_string)
        .unwrap_or_else(|| "{}".to_string());

    process.send_notification("initialized", json!({}))?;

    Ok(SpawnedLspServer {
        process,
        server_name,
        root_path: root_path.to_path_buf(),
        reader,
        stderr,
        capabilities_summary,
    })
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

        if line == "\r\n" || line == "\n" {
            break;
        }

        if let Some((name, value)) = line.split_once(':') {
            headers.insert(name.trim().to_ascii_lowercase(), value.trim().to_string());
        }
    }

    let Some(length_header) = headers.get("content-length") else {
        return Err("lsp message missing Content-Length header".to_string());
    };
    let content_length = length_header
        .parse::<usize>()
        .map_err(|err| format!("invalid Content-Length {:?}: {err}", length_header))?;

    let mut body = vec![0_u8; content_length];
    reader
        .read_exact(&mut body)
        .map_err(|err| format!("read lsp body failed: {err}"))?;
    let value = serde_json::from_slice::<Value>(&body)
        .map_err(|err| format!("decode lsp json failed: {err}"))?;
    Ok(Some(value))
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

pub fn wait_for_json_rpc_response(
    reader: &mut dyn BufRead,
    expected_id: u64,
) -> Result<Option<Value>, String> {
    loop {
        let Some(message) = read_json_rpc_message(reader)? else {
            return Ok(None);
        };

        let message_id = message.get("id").and_then(Value::as_u64);
        if message_id == Some(expected_id) {
            return Ok(Some(message));
        }
        // Các message khác (notification/response không liên quan) sẽ được bỏ qua ở bootstrap.
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
        "contentChanges": [
            {
                "text": text
            }
        ]
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

pub fn resolve_lsp_server_command(requested_command: Option<&str>) -> String {
    if let Some(command) = requested_command {
        let command = command.trim();
        if !command.is_empty() {
            return command.to_string();
        }
    }
    "rust-analyzer".to_string()
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

pub fn into_stderr_reader(stderr: ChildStderr) -> Box<dyn Read + Send> {
    Box::new(BufReader::new(stderr))
}

pub fn into_stdout_reader(stdout: ChildStdout) -> Box<dyn BufRead + Send> {
    Box::new(BufReader::new(stdout))
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use serde_json::json;

    use super::{parse_publish_diagnostics, read_json_rpc_message, write_json_rpc_message};

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
}
