use crate::async_runtime::message::{WorkerRequest, WorkerRequestPayload, WorkerResultPayload};

pub(super) async fn execute_ai_inline_request(
    request: &WorkerRequest,
) -> Result<WorkerResultPayload, String> {
    let WorkerRequestPayload::AiInlineCompletionRequest {
        api_url,
        api_key,
        model,
        endpoint_kind,
        prefix,
        suffix,
        language_id,
        file_path,
        max_tokens,
    } = &request.payload
    else {
        return Err("ai inline request payload mismatch".to_string());
    };

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .build()
        .unwrap_or_default();
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

    let body = serde_json::json!({
        "model": model,
        "messages": [
            {"role": "system", "content": system},
            {"role": "user", "content": user}
        ],
        "temperature": 0.2,
        "max_tokens": max_tokens,
        "stream": false
    });

    let mut req = client.post(endpoint).json(&body);
    if let Some(key) = api_key.as_ref().filter(|key| !key.is_empty()) {
        req = req.bearer_auth(key);
    }
    let response = req
        .send()
        .await
        .map_err(|err| format!("ai request failed: {err}"))?;
    let status = response.status();
    let json: serde_json::Value = response
        .json()
        .await
        .map_err(|err| format!("ai response decode failed: {err}"))?;
    if !status.is_success() {
        return Err(format!("ai request error {}: {}", status, json));
    }

    let suggestion = json
        .get("choices")
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
        .to_string();

    Ok(WorkerResultPayload::AiInlineCompletionResult { suggestion })
}
