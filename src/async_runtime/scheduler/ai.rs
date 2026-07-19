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
    if let Some(effort) = reasoning_effort
        .as_ref()
        .filter(|effort| !effort.is_empty())
    {
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

    let body_text = tokio::select! {
        _ = cancel_token.cancelled() => {
            return Err(cancelled_message());
        }
        text = response.text() => text.map_err(|err| format!("ai response read failed: {err}"))?,
    };
    if cancel_token.is_cancelled() {
        return Err(cancelled_message());
    }
    let mut cleaned = body_text.trim();
    if let Some(idx) = cleaned.rfind('}') {
        cleaned = &cleaned[..=idx];
    }
    let json: serde_json::Value =
        serde_json::from_str(cleaned).map_err(|err| format!("ai response decode failed: {err}"))?;
    if !status.is_success() {
        return Err(format!("ai request error {}: {}", status, json));
    }

    let suggestion = extract_non_streaming_suggestion(&json);
    Ok(WorkerResultPayload::AiInlineCompletionResult { suggestion })
}

pub(super) async fn execute_ai_rerank_request(
    request: &WorkerRequest,
) -> Result<WorkerResultPayload, String> {
    let WorkerRequestPayload::AiCompletionRerankRequest {
        api_url,
        api_key,
        model,
        endpoint_kind,
        reasoning_effort,
        prefix,
        suffix,
        language_id,
        candidates,
        prefix_token,
        completion_revision,
        cancel_token,
    } = &request.payload
    else {
        return Err("ai rerank request payload mismatch".to_string());
    };

    if cancel_token.is_cancelled() {
        return Err(cancelled_message());
    }

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .map_err(|err| format!("ai client build failed: {err}"))?;
    let endpoint = match endpoint_kind.as_deref() {
        Some("responses") => format!("{}/responses", api_url.trim_end_matches('/')),
        Some(path) if path.starts_with('/') => format!("{}{}", api_url.trim_end_matches('/'), path),
        _ => format!("{}/chat/completions", api_url.trim_end_matches('/')),
    };

    let system = "You re-rank code-completion candidates. You are given the code immediately before and after the cursor and a list of candidate identifiers the language server proposed. Return ONLY a JSON array of those SAME identifiers, reordered best-first for this exact cursor context. Never add, remove, rename, translate, or invent identifiers. Output the JSON array and nothing else.";
    let candidate_list = candidates
        .iter()
        .map(|label| format!("- {label}"))
        .collect::<Vec<_>>()
        .join("\n");
    let user = format!(
        "Language: {}\n\nCode before cursor:\n{}\n\nCode after cursor:\n{}\n\nCandidates:\n{}\n\nReturn the JSON array of these identifiers, best-first.",
        language_id.clone().unwrap_or_default(),
        prefix,
        suffix,
        candidate_list,
    );

    let mut body = serde_json::json!({
        "model": model,
        "messages": [
            {"role": "system", "content": system},
            {"role": "user", "content": user}
        ],
        "temperature": 0.0,
        "max_tokens": 512,
        "stream": false
    });
    if let Some(effort) = reasoning_effort
        .as_ref()
        .filter(|effort| !effort.is_empty())
    {
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
        response = req.send() => response.map_err(|err| format!("ai rerank request failed: {err}"))?,
    };
    let status = response.status();
    let body_text = tokio::select! {
        _ = cancel_token.cancelled() => {
            return Err(cancelled_message());
        }
        text = response.text() => text.map_err(|err| format!("ai rerank read failed: {err}"))?,
    };
    if cancel_token.is_cancelled() {
        return Err(cancelled_message());
    }
    let json: serde_json::Value = serde_json::from_str(body_text.trim())
        .map_err(|err| format!("ai rerank decode failed: {err}"))?;
    if !status.is_success() {
        return Err(format!("ai rerank error {}: {}", status, json));
    }

    let content = extract_non_streaming_suggestion(&json);
    // Keep only labels the server actually proposed: the model is instructed not
    // to invent, but a defensive filter guarantees membership can never change.
    let allowed: std::collections::HashSet<&str> = candidates.iter().map(String::as_str).collect();
    let ranked = parse_rerank_response(&content)
        .into_iter()
        .filter(|label| allowed.contains(label.as_str()))
        .collect();

    Ok(WorkerResultPayload::AiCompletionRerankResult {
        ranked,
        prefix_token: prefix_token.clone(),
        completion_revision: *completion_revision,
    })
}

/// Parse a re-rank model response into an ordered list of labels. Accepts a JSON
/// array (possibly wrapped in prose or code fences) and falls back to one label
/// per line with common list decorations stripped.
fn parse_rerank_response(text: &str) -> Vec<String> {
    let trimmed = text.trim();
    if let (Some(start), Some(end)) = (trimmed.find('['), trimmed.rfind(']'))
        && start < end
        && let Ok(array) = serde_json::from_str::<Vec<String>>(&trimmed[start..=end])
    {
        return array
            .into_iter()
            .map(|label| label.trim().to_string())
            .filter(|label| !label.is_empty())
            .collect();
    }

    trimmed
        .lines()
        .map(|line| {
            line.trim()
                .trim_start_matches(|c: char| {
                    c.is_ascii_digit() || matches!(c, '.' | ')' | '-' | '*' | ' ' | '\t')
                })
                .trim()
                .trim_matches(|c| matches!(c, '"' | '`' | ','))
                .trim()
                .to_string()
        })
        .filter(|label| !label.is_empty())
        .collect()
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

#[cfg(test)]
mod rerank_parse_tests {
    use super::parse_rerank_response;

    #[test]
    fn parses_plain_json_array() {
        assert_eq!(
            parse_rerank_response(r#"["connect", "configure"]"#),
            vec!["connect".to_string(), "configure".to_string()]
        );
    }

    #[test]
    fn parses_json_array_wrapped_in_prose_or_fences() {
        let text = "Sure! Here is the order:\n```json\n[\"beta\", \"alpha\"]\n```";
        assert_eq!(
            parse_rerank_response(text),
            vec!["beta".to_string(), "alpha".to_string()]
        );
    }

    #[test]
    fn falls_back_to_decorated_lines() {
        let text = "1. connect\n2. configure\n- consume";
        assert_eq!(
            parse_rerank_response(text),
            vec![
                "connect".to_string(),
                "configure".to_string(),
                "consume".to_string()
            ]
        );
    }

    #[test]
    fn empty_response_yields_no_order() {
        assert!(parse_rerank_response("   ").is_empty());
    }
}
