use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, Command};
use tokio::sync::{mpsc, oneshot};

use crate::dap::types::{Request, Response, Event};

pub struct DapClient {
    _child: Child,
    stdin: tokio::sync::Mutex<ChildStdin>,
    next_seq: AtomicU64,
    pending_requests: Arc<Mutex<HashMap<u64, oneshot::Sender<Response>>>>,
}

impl DapClient {
    pub fn launch(
        program: &str,
        args: &[String],
        working_dir: Option<PathBuf>,
        event_tx: mpsc::UnboundedSender<Event>,
    ) -> Result<Arc<Self>, std::io::Error> {
        Self::launch_with_env(program, args, working_dir, event_tx, None)
    }

    pub fn launch_with_env(
        program: &str,
        args: &[String],
        working_dir: Option<PathBuf>,
        event_tx: mpsc::UnboundedSender<Event>,
        env_vars: Option<std::collections::HashMap<String, String>>,
    ) -> Result<Arc<Self>, std::io::Error> {
        eprintln!("[DAP LOG] [DAP Client] DapClient::launch called for program: {}, args: {:?}", program, args);
        let path_buf = std::path::Path::new(program);
        let exists = if path_buf.is_absolute() || path_buf.components().count() > 1 {
            path_buf.exists()
        } else if let Ok(path) = std::env::var("PATH") {
            let mut found = false;
            for dir in std::env::split_paths(&path) {
                let joined = dir.join(program);
                eprintln!("[DAP LOG] exists check: program={}, checking joined={:?}, exists={}", program, joined, joined.exists());
                if joined.exists() {
                    found = true;
                    break;
                }
            }
            found
        } else {
            false
        };
        if !exists {
            return Err(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                format!("Binary '{}' not found in PATH or at specified path", program),
            ));
        }

        let mut cmd = Command::new(program);
        cmd.args(args);
        cmd.stdin(Stdio::piped());
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::null());

        // Set environment variables if provided (for FVM support)
        if let Some(env) = env_vars {
            for (key, value) in env {
                cmd.env(&key, &value);
            }
        }

        if let Some(wd) = working_dir {
            cmd.current_dir(wd);
        }

        let mut child = cmd.spawn()?;
        let stdin = child.stdin.take().ok_or_else(|| {
            std::io::Error::new(std::io::ErrorKind::Other, "Failed to capture stdin")
        })?;
        let stdout = child.stdout.take().ok_or_else(|| {
            std::io::Error::new(std::io::ErrorKind::Other, "Failed to capture stdout")
        })?;

        let pending_requests: Arc<Mutex<HashMap<u64, oneshot::Sender<Response>>>> = Arc::new(Mutex::new(HashMap::new()));
        let pending_requests_clone = pending_requests.clone();

        // Spawn background stdout reader thread
        tokio::spawn(async move {
            let mut reader = BufReader::new(stdout);
            let mut buffer = Vec::new();

            loop {
                match read_message(&mut reader, &mut buffer).await {
                    Ok(Some(raw_json)) => {
                        if let Ok(value) = serde_json::from_str::<serde_json::Value>(&raw_json) {
                            let msg_type = value.get("type").and_then(|v| v.as_str()).unwrap_or("");
                            match msg_type {
                                "response" => {
                                    if let Ok(resp) = serde_json::from_str::<Response>(&raw_json) {
                                        let mut guard = pending_requests_clone.lock().unwrap();
                                        if let Some(tx) = guard.remove(&resp.request_seq) {
                                            let _ = tx.send(resp);
                                        }
                                    }
                                }
                                "event" => {
                                    if let Ok(event) = serde_json::from_str::<Event>(&raw_json) {
                                        let _ = event_tx.send(event);
                                    }
                                }
                                _ => {}
                            }
                        }
                    }
                    Ok(None) => {
                        // EOF reached
                        break;
                    }
                    Err(err) => {
                        eprintln!("[DapClient] Error reading stdout message: {:?}", err);
                        break;
                    }
                }
            }
        });

        Ok(Arc::new(Self {
            _child: child,
            stdin: tokio::sync::Mutex::new(stdin),
            next_seq: AtomicU64::new(1),
            pending_requests,
        }))
    }

    pub async fn send_request(
        &self,
        command: &str,
        arguments: Option<serde_json::Value>,
    ) -> Result<Response, String> {
        let seq = self.next_seq.fetch_add(1, Ordering::SeqCst);
        let req = Request {
            seq,
            message_type: "request".to_string(),
            command: command.to_string(),
            arguments,
        };

        let raw_json = serde_json::to_string(&req)
            .map_err(|e| format!("Failed to serialize request: {}", e))?;

        let (tx, rx) = oneshot::channel();
        {
            let mut guard = self.pending_requests.lock().unwrap();
            guard.insert(seq, tx);
        }

        let payload = format!("Content-Length: {}\r\n\r\n{}", raw_json.len(), raw_json);
        {
            let mut stdin_guard = self.stdin.lock().await;
            stdin_guard
                .write_all(payload.as_bytes())
                .await
                .map_err(|e| format!("Failed to write to stdin: {}", e))?;
            stdin_guard
                .flush()
                .await
                .map_err(|e| format!("Failed to flush stdin: {}", e))?;
        }

        rx.await
            .map_err(|_| "Debug adapter disconnected".to_string())
    }
}

// ── Read DAP Protocol Header & Body ──────────────────────────────────────────

async fn read_message<R: AsyncRead + Unpin>(
    reader: &mut R,
    buffer: &mut Vec<u8>,
) -> std::io::Result<Option<String>> {
    let mut content_length: Option<usize> = None;

    loop {
        // Read header line by line
        let mut line = String::new();
        let mut header_bytes = [0u8; 1];

        loop {
            let bytes_read = reader.read(&mut header_bytes).await?;
            if bytes_read == 0 {
                return Ok(None); // EOF
            }
            let c = header_bytes[0] as char;
            line.push(c);
            if line.ends_with("\r\n") {
                break;
            }
        }

        if line == "\r\n" {
            // End of headers, break to read body
            break;
        }

        if line.starts_with("Content-Length:") {
            let parts: Vec<&str> = line.split(':').collect();
            if parts.len() == 2 {
                if let Ok(len) = parts[1].trim().parse::<usize>() {
                    content_length = Some(len);
                }
            }
        }
    }

    let len = content_length.ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "Missing Content-Length header",
        )
    })?;

    buffer.resize(len, 0);
    reader.read_exact(buffer).await?;

    let json_str = String::from_utf8(buffer.clone()).map_err(|e| {
        std::io::Error::new(std::io::ErrorKind::InvalidData, format!("Invalid UTF-8: {}", e))
    })?;

    Ok(Some(json_str))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::BufReader;

    fn make_dap_message(json: &str) -> Vec<u8> {
        let header = format!("Content-Length: {}\r\n\r\n", json.len());
        let mut bytes = header.into_bytes();
        bytes.extend_from_slice(json.as_bytes());
        bytes
    }

    #[tokio::test]
    async fn read_message_parses_valid_dap_frame() {
        let json = r#"{"seq":1,"type":"event","event":"initialized"}"#;
        let data = make_dap_message(json);
        let mut reader = BufReader::new(&data[..]);
        let mut buffer = Vec::new();

        let result = read_message(&mut reader, &mut buffer).await.unwrap();
        assert!(result.is_some());
        assert_eq!(result.unwrap(), json);
    }

    #[tokio::test]
    async fn read_message_returns_none_on_eof() {
        let data: &[u8] = &[];
        let mut reader = BufReader::new(data);
        let mut buffer = Vec::new();

        let result = read_message(&mut reader, &mut buffer).await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn read_message_errors_on_missing_content_length() {
        // Header with no Content-Length
        let data = b"\r\n";
        let mut reader = BufReader::new(&data[..]);
        let mut buffer = Vec::new();

        let result = read_message(&mut reader, &mut buffer).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn read_message_parses_multiple_frames_sequentially() {
        let json1 = r#"{"seq":1,"type":"event","event":"initialized"}"#;
        let json2 = r#"{"seq":2,"type":"response","request_seq":0,"success":true,"command":"initialize"}"#;
        let mut data = make_dap_message(json1);
        data.extend_from_slice(&make_dap_message(json2));

        let mut reader = BufReader::new(&data[..]);
        let mut buffer = Vec::new();

        let r1 = read_message(&mut reader, &mut buffer).await.unwrap().unwrap();
        assert_eq!(r1, json1);

        let r2 = read_message(&mut reader, &mut buffer).await.unwrap().unwrap();
        assert_eq!(r2, json2);
    }

    #[tokio::test]
    async fn read_message_handles_empty_body() {
        let json = "{}";
        let data = make_dap_message(json);
        let mut reader = BufReader::new(&data[..]);
        let mut buffer = Vec::new();

        let result = read_message(&mut reader, &mut buffer).await.unwrap();
        assert_eq!(result.unwrap(), "{}");
    }

    #[tokio::test]
    async fn read_message_handles_unicode_content() {
        let json = r#"{"output":"Xin chào thế giới 🌍"}"#;
        let data = make_dap_message(json);
        let mut reader = BufReader::new(&data[..]);
        let mut buffer = Vec::new();

        let result = read_message(&mut reader, &mut buffer).await.unwrap();
        assert_eq!(result.unwrap(), json);
    }

    #[tokio::test]
    async fn read_message_reuses_buffer_across_calls() {
        let json1 = r#"{"short":true}"#;
        let json2 = r#"{"a_much_longer_field_name":"with a longer value that exceeds the first buffer"}"#;
        let mut data = make_dap_message(json1);
        data.extend_from_slice(&make_dap_message(json2));

        let mut reader = BufReader::new(&data[..]);
        let mut buffer = Vec::new();

        let r1 = read_message(&mut reader, &mut buffer).await.unwrap().unwrap();
        assert_eq!(r1, json1);

        let r2 = read_message(&mut reader, &mut buffer).await.unwrap().unwrap();
        assert_eq!(r2, json2);
        // Buffer should have been resized for the larger message
        assert!(buffer.len() >= json2.len());
    }
}
