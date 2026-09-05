//! Shared OpenAI-compatible chat client used by every AI feature (inline
//! completion, completion re-rank, LeetCode generate/verify/adapt) and the
//! `/models` catalog fetch behind the Settings model picker.

use std::time::Duration;

use serde_json::{Value, json};

use crate::{async_runtime::message::AiModelInfo, config::ai_config::AiProviderConfig};

pub struct ChatOptions {
    pub temperature: f32,
    pub max_tokens: u32,
    pub stop: Vec<String>,
    pub stream: bool,
    pub timeout: Duration,
    /// OpenRouter only: route to the lowest-latency upstream (inline completion).
    pub prefer_latency: bool,
}

impl ChatOptions {
    pub fn new(max_tokens: u32, timeout: Duration) -> Self {
        Self {
            temperature: 0.1,
            max_tokens,
            stop: Vec::new(),
            stream: false,
            timeout,
            prefer_latency: false,
        }
    }
}

pub fn is_openrouter(api_url: &str) -> bool {
    api_url.contains("openrouter.ai")
}

/// `{base}/chat/completions`, or `{base}{endpoint_kind}` for a custom `/path`.
pub fn chat_endpoint(provider: &AiProviderConfig) -> String {
    let base = provider.api_url.trim().trim_end_matches('/');
    match provider.endpoint_kind.as_deref() {
        Some(path) if path.starts_with('/') => format!("{base}{path}"),
        _ => format!("{base}/chat/completions"),
    }
}

pub fn build_chat_body(
    provider: &AiProviderConfig,
    system: &str,
    user: &str,
    opts: &ChatOptions,
) -> Value {
    let mut body = json!({
        "model": provider.model,
        "messages": [
            {"role": "system", "content": system},
            {"role": "user", "content": user}
        ],
        "temperature": opts.temperature,
        "max_tokens": opts.max_tokens,
        "stream": opts.stream
    });
    if !opts.stop.is_empty() {
        body["stop"] = json!(opts.stop);
    }
    apply_reasoning(&mut body, provider);
    if opts.prefer_latency && is_openrouter(&provider.api_url) {
        body["provider"] = json!({"sort": "latency"});
    }
    body
}

/// Translate `reasoning_effort` per host: OpenRouter takes the unified
/// `reasoning` object (and can switch thinking off); plain OpenAI-compatible
/// servers get the legacy `reasoning_effort` field, never `"none"`.
fn apply_reasoning(body: &mut Value, provider: &AiProviderConfig) {
    let Some(effort) = provider
        .reasoning_effort
        .as_deref()
        .map(str::trim)
        .filter(|effort| !effort.is_empty())
    else {
        return;
    };
    let off = effort.eq_ignore_ascii_case("none");
    if is_openrouter(&provider.api_url) {
        body["reasoning"] = if off {
            json!({"enabled": false})
        } else {
            json!({"effort": effort})
        };
    } else if !off {
        body["reasoning_effort"] = json!(effort);
    }
}

pub fn build_client(timeout: Duration) -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .timeout(timeout)
        .build()
        .map_err(|err| format!("AI client build failed: {err}"))
}

/// POST `body` to `endpoint` with the provider's bearer key.
pub fn post_json(
    client: &reqwest::Client,
    provider: &AiProviderConfig,
    endpoint: String,
    body: &Value,
) -> reqwest::RequestBuilder {
    let mut request = client.post(endpoint).json(body);
    if let Some(key) = provider
        .api_key
        .as_ref()
        .filter(|key| !key.trim().is_empty())
    {
        request = request.bearer_auth(key.trim());
    }
    request
}

fn ensure_configured(provider: &AiProviderConfig) -> Result<(), String> {
    if provider.api_url.trim().is_empty() || provider.model.trim().is_empty() {
        return Err("AI provider is not configured".to_string());
    }
    Ok(())
}

/// One non-streaming chat round-trip. Returns the decoded response body on
/// HTTP success; HTTP errors are mapped to the API's `error.message`.
pub async fn chat(
    provider: &AiProviderConfig,
    system: &str,
    user: &str,
    opts: &ChatOptions,
) -> Result<Value, String> {
    ensure_configured(provider)?;
    let body = build_chat_body(provider, system, user, opts);
    let client = build_client(opts.timeout)?;
    let response = post_json(&client, provider, chat_endpoint(provider), &body)
        .send()
        .await
        .map_err(|err| format!("AI request failed: {err}"))?;
    let status = response.status();
    let text = response
        .text()
        .await
        .map_err(|err| format!("AI response read failed: {err}"))?;
    let json = decode_json_body(&text)?;
    if !status.is_success() {
        return Err(api_error_message(status, &json));
    }
    Ok(json)
}

/// Decode a JSON body, tolerating trailing garbage after the last `}` (some
/// proxies append SSE keep-alives to non-streaming replies).
pub fn decode_json_body(text: &str) -> Result<Value, String> {
    let mut cleaned = text.trim();
    if let Some(idx) = cleaned.rfind('}') {
        cleaned = &cleaned[..=idx];
    }
    serde_json::from_str(cleaned).map_err(|err| format!("AI returned invalid JSON: {err}"))
}

pub fn api_error_message(status: reqwest::StatusCode, json: &Value) -> String {
    match json["error"]["message"].as_str() {
        Some(message) => format!("AI error (HTTP {status}): {message}"),
        None => format!("AI returned HTTP {status}"),
    }
}

/// `choices[0].message.content` (or `.text`), or a diagnostic explaining why
/// there is none (reasoning ate the budget, truncated, refused).
pub fn extract_content(json: &Value) -> Result<String, String> {
    let choice = &json["choices"][0];
    if let Some(content) = choice["message"]["content"]
        .as_str()
        .or_else(|| choice["text"].as_str())
    {
        return Ok(content.to_string());
    }
    let finish = choice["finish_reason"].as_str().unwrap_or("unknown");
    let reasoning_tokens = json["usage"]["completion_tokens_details"]["reasoning_tokens"].as_u64();
    Err(match (finish, reasoning_tokens) {
        ("length", Some(rt)) if rt > 0 => format!(
            "AI model spent its token budget on reasoning ({rt} reasoning tokens). \
             Set reasoning_effort = \"none\" or pick a non-reasoning model."
        ),
        ("length", _) => {
            "AI response was truncated (finish_reason=length) — increase max_tokens.".to_string()
        }
        _ => format!("AI response contained no content (finish_reason={finish})"),
    })
}

/// Parse an OpenAI-shaped `GET /models` reply. OpenRouter extras (pricing,
/// context length, `supported_parameters`) are read when present. Batch
/// aliases and models that cannot output text are dropped.
pub fn parse_models_response(json: &Value) -> Vec<AiModelInfo> {
    let Some(entries) = json["data"].as_array() else {
        return Vec::new();
    };
    let per_million = |value: &Value| {
        value
            .as_str()
            .and_then(|s| s.parse::<f64>().ok())
            .or_else(|| value.as_f64())
            .map(|price| price * 1_000_000.0)
    };
    let mut models: Vec<AiModelInfo> = entries
        .iter()
        .filter_map(|entry| {
            let id = entry["id"].as_str()?.trim();
            if id.is_empty() || id.ends_with(":batch") {
                return None;
            }
            if let Some(outputs) = entry["architecture"]["output_modalities"].as_array()
                && !outputs.iter().any(|m| m.as_str() == Some("text"))
            {
                return None;
            }
            let reasoning = entry["supported_parameters"]
                .as_array()
                .is_some_and(|params| params.iter().any(|p| p.as_str() == Some("reasoning")));
            Some(AiModelInfo {
                id: id.to_string(),
                name: entry["name"].as_str().map(str::to_string),
                context_length: entry["context_length"].as_u64(),
                prompt_price_per_m: per_million(&entry["pricing"]["prompt"]),
                completion_price_per_m: per_million(&entry["pricing"]["completion"]),
                reasoning,
            })
        })
        .collect();
    models.sort_by(|a, b| a.id.cmp(&b.id));
    models
}

/// `GET {api_url}/models` with the bearer key.
pub async fn list_models(api_url: &str, api_key: Option<&str>) -> Result<Vec<AiModelInfo>, String> {
    let base = api_url.trim().trim_end_matches('/');
    if base.is_empty() {
        return Err("AI endpoint is not configured".to_string());
    }
    let client = build_client(Duration::from_secs(20))?;
    let mut request = client.get(format!("{base}/models"));
    if let Some(key) = api_key.map(str::trim).filter(|key| !key.is_empty()) {
        request = request.bearer_auth(key);
    }
    let response = request
        .send()
        .await
        .map_err(|err| format!("model list request failed: {err}"))?;
    let status = response.status();
    let text = response
        .text()
        .await
        .map_err(|err| format!("model list read failed: {err}"))?;
    let json = decode_json_body(&text)?;
    if !status.is_success() {
        return Err(api_error_message(status, &json));
    }
    let models = parse_models_response(&json);
    if models.is_empty() {
        return Err("endpoint returned no models".to_string());
    }
    Ok(models)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn provider(url: &str, effort: Option<&str>) -> AiProviderConfig {
        AiProviderConfig {
            api_url: url.to_string(),
            model: "m".to_string(),
            api_key: Some("k".to_string()),
            endpoint_kind: None,
            reasoning_effort: effort.map(str::to_string),
        }
    }

    #[test]
    fn endpoint_joins_base_and_custom_path() {
        let mut p = provider("https://openrouter.ai/api/v1/", None);
        assert_eq!(chat_endpoint(&p), "https://openrouter.ai/api/v1/chat/completions");
        p.endpoint_kind = Some("/custom".to_string());
        assert_eq!(chat_endpoint(&p), "https://openrouter.ai/api/v1/custom");
        p.endpoint_kind = Some("responses".to_string());
        assert_eq!(chat_endpoint(&p), "https://openrouter.ai/api/v1/chat/completions");
    }

    #[test]
    fn openrouter_gets_unified_reasoning_and_latency_routing() {
        let mut opts = ChatOptions::new(64, Duration::from_secs(1));
        opts.stop = vec!["\n".to_string()];
        opts.prefer_latency = true;
        let body = build_chat_body(
            &provider("https://openrouter.ai/api/v1", Some("none")),
            "s",
            "u",
            &opts,
        );
        assert_eq!(body["reasoning"], json!({"enabled": false}));
        assert_eq!(body["provider"], json!({"sort": "latency"}));
        assert_eq!(body["stop"], json!(["\n"]));
        assert_eq!(body["max_tokens"], json!(64));
        assert!(body.get("reasoning_effort").is_none());

        let body = build_chat_body(
            &provider("https://openrouter.ai/api/v1", Some("low")),
            "s",
            "u",
            &ChatOptions::new(64, Duration::from_secs(1)),
        );
        assert_eq!(body["reasoning"], json!({"effort": "low"}));
        assert!(body.get("provider").is_none());
        assert!(body.get("stop").is_none());
    }

    #[test]
    fn plain_openai_hosts_get_legacy_reasoning_effort_and_never_none() {
        let opts = ChatOptions::new(64, Duration::from_secs(1));
        let body = build_chat_body(&provider("http://localhost:20128/v1", Some("low")), "s", "u", &opts);
        assert_eq!(body["reasoning_effort"], json!("low"));
        assert!(body.get("reasoning").is_none());
        let body = build_chat_body(&provider("http://localhost:20128/v1", Some("none")), "s", "u", &opts);
        assert!(body.get("reasoning_effort").is_none());
        assert!(body.get("reasoning").is_none());
    }

    #[test]
    fn extract_content_reads_content_or_explains_its_absence() {
        assert_eq!(
            extract_content(&json!({"choices":[{"message":{"content":"x"}}]})).unwrap(),
            "x"
        );
        assert_eq!(extract_content(&json!({"choices":[{"text":"y"}]})).unwrap(), "y");
        let err = extract_content(&json!({
            "choices":[{"message":{"content":null},"finish_reason":"length"}],
            "usage":{"completion_tokens_details":{"reasoning_tokens":150}}
        }))
        .unwrap_err();
        assert!(err.contains("150 reasoning tokens"), "{err}");
        let err = extract_content(&json!({"choices":[{"finish_reason":"length"}]})).unwrap_err();
        assert!(err.contains("truncated"), "{err}");
        let err = extract_content(&json!({"choices":[]})).unwrap_err();
        assert!(err.contains("no content"), "{err}");
    }

    #[test]
    fn decode_tolerates_trailing_garbage_and_reports_api_errors() {
        let json = decode_json_body("{\"a\":1}\n\ndata: keepalive").unwrap();
        assert_eq!(json["a"], json!(1));
        assert!(decode_json_body("nope").is_err());
        let msg = api_error_message(
            reqwest::StatusCode::UNAUTHORIZED,
            &json!({"error":{"message":"No auth credentials found"}}),
        );
        assert_eq!(msg, "AI error (HTTP 401 Unauthorized): No auth credentials found");
    }

    #[test]
    fn parse_models_filters_batch_and_non_text_and_reads_openrouter_extras() {
        let json = json!({"data":[
            {"id":"b/model:batch","pricing":{"prompt":"0.1","completion":"0.2"}},
            {"id":"a/image","architecture":{"output_modalities":["image"]}},
            {"id":"z/plain"},
            {"id":"m/code","name":"Code","context_length":256000,
             "pricing":{"prompt":"0.0000003","completion":"0.0000009"},
             "supported_parameters":["reasoning","tools"],
             "architecture":{"output_modalities":["text"]}}
        ]});
        let models = parse_models_response(&json);
        assert_eq!(models.len(), 2);
        assert_eq!(models[0].id, "m/code");
        assert_eq!(models[0].name.as_deref(), Some("Code"));
        assert_eq!(models[0].context_length, Some(256000));
        assert!((models[0].prompt_price_per_m.unwrap() - 0.3).abs() < 1e-9);
        assert!((models[0].completion_price_per_m.unwrap() - 0.9).abs() < 1e-9);
        assert!(models[0].reasoning);
        assert_eq!(models[1].id, "z/plain");
        assert!(!models[1].reasoning);
        assert!(models[1].context_length.is_none());
        assert!(parse_models_response(&json!({})).is_empty());
    }
}
