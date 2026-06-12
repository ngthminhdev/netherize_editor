use std::sync::mpsc as std_mpsc;

use futures_util::StreamExt;
use tokio_util::sync::CancellationToken;

use crate::async_runtime::message::{
    WorkerMessage, WorkerRequest, WorkerRequestPayload, WorkerResult, WorkerResultPayload,
};

use super::emit::emit_message;

pub(super) async fn execute_ai_inline_request(
    request: &WorkerRequest,
    worker_tx: Option<&std_mpsc::Sender<WorkerMessage>>,
) -> Result<WorkerResultPayload, String> {
    let WorkerRequestPayload::AiInlineCompletionRequest {
        api_url,
        api_key,
        model,
        endpoint_kind,
        reasoning_effort,
        prefix,
        suffix,
        language_id,
        file_path,
        max_tokens,
        cancel_token,
    } = &request.payload
    else {
        return Err("ai inline request payload mismatch".to_string());
    };

    if cancel_token.is_cancelled() {
        return Err(cancelled_message());
    }

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .build()
        .map_err(|err| format!("ai client build failed: {err}"))?;
    let endpoint = match endpoint_kind.as_deref() {
        Some("responses") => format!("{}/responses", api_url.trim_end_matches('/')),
        Some(path) if path.starts_with('/') => format!("{}{}", api_url.trim_end_matches('/'), path),
        _ => format!("{}/chat/completions", api_url.trim_end_matches('/')),
    };

    let system = "You are an inline code completion engine. Return only the continuation text to insert at the cursor. Do not repeat the prefix. Do not add markdown fences or explanations.";
    let user = format!(
        "File: {}\nLanguage: {}\n\nPrefix:\n{}\n\nSuffix:\n{}\n\nReturn only the best continuation for the cursor position.",
        file_path
            .as_ref()
            .map(|path| path.display().to_string())
            .unwrap_or_default(),
        language_id.clone().unwrap_or_default(),
        prefix,
        suffix,
    );

    let stream_response = worker_tx.is_some() && endpoint_kind.as_deref() != Some("responses");
    let mut body = serde_json::json!({
        "model": model,
        "messages": [
            {"role": "system", "content": system},
            {"role": "user", "content": user}
        ],
        "temperature": 0.2,
        "max_tokens": max_tokens,
        "stream": stream_response
    });
    if let Some(effort) = reasoning_effort.as_ref().filter(|effort| !effort.is_empty()) {
        body["reasoning_effort"] = serde_json::Value::String(effort.clone());
    }

    let mut req = client.post(endpoint).json(&body);
    if let Some(key) = api_key.as_ref().filter(|key| !key.is_empty()) {
        req = req.bearer_auth(key);
    }

    let response = tokio::select! {
        _ = cancel_token.cancelled() => {
            return Err(cancelled_message());
        }
        response = req.send() => response.map_err(|err| format!("ai request failed: {err}"))?,
    };
    let status = response.status();
    if stream_response && status.is_success() {
        let Some(worker_tx) = worker_tx else {
            return Err("ai inline stream missing worker channel".to_string());
        };
        return read_streaming_response(request, worker_tx, response, cancel_token).await;
    }

    let json: serde_json::Value = tokio::select! {
        _ = cancel_token.cancelled() => {
            return Err(cancelled_message());
        }
        json = response.json() => json.map_err(|err| format!("ai response decode failed: {err}"))?,
    };
    if cancel_token.is_cancelled() {
        return Err(cancelled_message());
    }
    if !status.is_success() {
        return Err(format!("ai request error {}: {}", status, json));
    }

    let suggestion = extract_non_streaming_suggestion(&json);
    Ok(WorkerResultPayload::AiInlineCompletionResult { suggestion })
}

async fn read_streaming_response(
    request: &WorkerRequest,
    worker_tx: &std_mpsc::Sender<WorkerMessage>,
    response: reqwest::Response,
    cancel_token: &CancellationToken,
) -> Result<WorkerResultPayload, String> {
    let mut stream = response.bytes_stream();
    let mut buffer = String::new();
    let mut suggestion = String::new();

    loop {
        let next = tokio::select! {
            _ = cancel_token.cancelled() => {
                return Err(cancelled_message());
            }
            next = stream.next() => next,
        };
        let Some(chunk) = next else {
            break;
        };
        let chunk = chunk.map_err(|err| format!("ai stream read failed: {err}"))?;
        let text = String::from_utf8_lossy(&chunk);
        buffer.push_str(&text);
        drain_sse_buffer(&mut buffer, |payload| {
            if payload == "[DONE]" {
                return;
            }
            if let Some(delta) = extract_streaming_delta(payload) {
                if delta.is_empty() {
                    return;
                }
                suggestion.push_str(&delta);
                emit_message(
                    worker_tx,
                    WorkerMessage::Result(WorkerResult {
                        request_id: request.request_id,
                        revision_id: request.revision_id,
                        topic: request.topic,
                        payload: WorkerResultPayload::AiInlineCompletionChunk { chunk: delta },
                    }),
                );
            }
        });
    }

    Ok(WorkerResultPayload::AiInlineCompletionResult { suggestion })
}

fn drain_sse_buffer(buffer: &mut String, mut on_payload: impl FnMut(&str)) {
    while let Some(event_end) = buffer.find("\n\n") {
        let event = buffer[..event_end].to_string();
        let rest = buffer[event_end + 2..].to_string();
        *buffer = rest;
        for line in event.lines() {
            let line = line.trim_end_matches('\r');
            let Some(payload) = line.strip_prefix("data:") else {
                continue;
            };
            on_payload(payload.trim());
        }
    }
}

fn extract_non_streaming_suggestion(json: &serde_json::Value) -> String {
    json.get("choices")
        .and_then(|choices| choices.as_array())
        .and_then(|choices| choices.first())
        .and_then(|choice| {
            choice
                .get("message")
                .and_then(|message| message.get("content"))
                .and_then(|content| content.as_str())
                .or_else(|| choice.get("text").and_then(|content| content.as_str()))
        })
        .unwrap_or_default()
        .to_string()
}

fn extract_streaming_delta(payload: &str) -> Option<String> {
    let json = serde_json::from_str::<serde_json::Value>(payload).ok()?;
    json.get("choices")
        .and_then(|choices| choices.as_array())
        .and_then(|choices| choices.first())
        .and_then(|choice| {
            choice
                .get("delta")
                .and_then(|delta| delta.get("content"))
                .and_then(|content| content.as_str())
                .or_else(|| choice.get("text").and_then(|content| content.as_str()))
        })
        .map(str::to_string)
}

fn cancelled_message() -> String {
    "ai inline request cancelled".to_string()
}
