use std::{sync::mpsc as std_mpsc, time::Duration};

use futures_util::StreamExt;
use tokio_util::sync::CancellationToken;

use crate::async_runtime::message::{
    WorkerMessage, WorkerRequest, WorkerRequestPayload, WorkerResult, WorkerResultPayload,
};

use super::{
    ai_client::{self, ChatOptions},
    emit::emit_message,
};

/// Marks the caret inside the file sent to the model.
pub const INLINE_CURSOR_MARKER: &str = "<|cursor|>";

const INLINE_SYSTEM_PROMPT: &str = "You are an inline code completion engine inside a code editor, like GitHub Copilot. \
The user's file is shown with <|cursor|> marking the caret. \
Reply with ONLY the raw text to insert at <|cursor|>: no explanation, no markdown fences, \
no repetition of the text before the caret, and nothing that already exists after it. \
Match the file's language, style and indentation exactly, including its quote style and naming conventions. \
Finish the current statement, expression or block and stop at a natural boundary. \
Prefer short, obviously-correct code over long speculative code. \
If diagnostics are listed and the mistake they point at is in the text right before the caret on its line, \
reply with that line rewritten from its first non-blank character — corrected and continued — instead of a plain insertion. \
If nothing sensible belongs at the caret, reply with an empty string.";

/// Single-line completions cap the budget: the rest of the line is all that
/// can be inserted before the text that already follows the caret.
const INLINE_SINGLE_LINE_MAX_TOKENS: u32 = 64;

/// Copilot-style FIM in a chat message: the whole window with the caret
/// marker, so the model sees what already follows and does not repeat it.
/// Neighbouring tabs (if any) go first as reference-only context.
pub(super) fn build_inline_prompt(
    prefix: &str,
    suffix: &str,
    language_id: Option<&str>,
    file_name: Option<&str>,
    neighbors: &[(String, String)],
    diagnostics: &[String],
) -> String {
    let file_name = file_name.unwrap_or("untitled");
    let mut prompt = format!(
        "Language: {}\nFile: {file_name}\n\n",
        language_id.unwrap_or("unknown"),
    );
    if !diagnostics.is_empty() {
        prompt.push_str("Diagnostics on the caret line:\n");
        for message in diagnostics {
            prompt.push_str(&format!("- {message}\n"));
        }
        prompt.push('\n');
    }
    if !neighbors.is_empty() {
        prompt.push_str(
            "Other open files in this project (reference only — never repeat them):\n",
        );
        for (name, snippet) in neighbors {
            prompt.push_str(&format!("--- {name} ---\n{}\n", snippet.trim_end()));
        }
        prompt.push_str(&format!(
            "\n--- {file_name} (insert at {INLINE_CURSOR_MARKER}) ---\n"
        ));
    }
    prompt.push_str(prefix);
    prompt.push_str(INLINE_CURSOR_MARKER);
    prompt.push_str(suffix);
    prompt
}

/// The caret has non-blank text after it on the same line → complete only
/// the rest of this line (Copilot does the same); otherwise allow one block.
pub(super) fn inline_is_single_line(suffix: &str) -> bool {
    !suffix.split('\n').next().unwrap_or("").trim().is_empty()
}

pub(super) fn inline_stop_sequences(single_line: bool) -> Vec<String> {
    if single_line {
        vec!["\n".to_string()]
    } else {
        vec!["\n\n".to_string()]
    }
}

pub(super) async fn execute_ai_inline_request(
    request: &WorkerRequest,
    worker_tx: Option<&std_mpsc::Sender<WorkerMessage>>,
) -> Result<WorkerResultPayload, String> {
    let WorkerRequestPayload::AiInlineCompletionRequest {
        provider,
        prefix,
        suffix,
        language_id,
        file_path,
        neighbors,
        diagnostics,
        max_tokens,
        cancel_token,
    } = &request.payload
    else {
        return Err("ai inline request payload mismatch".to_string());
    };

    if cancel_token.is_cancelled() {
        return Err(cancelled_message());
    }

    let file_name = file_path
        .as_ref()
        .and_then(|path| path.file_name())
        .map(|name| name.to_string_lossy().to_string());
    let user = build_inline_prompt(
        prefix,
        suffix,
        language_id.as_deref(),
        file_name.as_deref(),
        neighbors,
        diagnostics,
    );
    let single_line = inline_is_single_line(suffix);
    let mut opts = ChatOptions::new(
        if single_line {
            (*max_tokens).min(INLINE_SINGLE_LINE_MAX_TOKENS)
        } else {
            *max_tokens
        },
        Duration::from_secs(15),
    );
    opts.temperature = 0.0;
    opts.stop = inline_stop_sequences(single_line);
    opts.stream = worker_tx.is_some();
    opts.prefer_latency = true;

    let body = ai_client::build_chat_body(provider, INLINE_SYSTEM_PROMPT, &user, &opts);
    let client = ai_client::build_client(opts.timeout)?;
    let req = ai_client::post_json(&client, provider, ai_client::chat_endpoint(provider), &body);

    let response = tokio::select! {
        _ = cancel_token.cancelled() => {
            return Err(cancelled_message());
        }
        response = req.send() => response.map_err(|err| format!("ai request failed: {err}"))?,
    };
    let status = response.status();
    if opts.stream && status.is_success() {
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
    let json = ai_client::decode_json_body(&body_text)?;
    if !status.is_success() {
        return Err(ai_client::api_error_message(status, &json));
    }
    let suggestion = ai_client::extract_content(&json)?;
    Ok(WorkerResultPayload::AiInlineCompletionResult { suggestion })
}

pub(super) async fn execute_ai_rerank_request(
    request: &WorkerRequest,
) -> Result<WorkerResultPayload, String> {
    let WorkerRequestPayload::AiCompletionRerankRequest {
        provider,
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
    let mut opts = ChatOptions::new(512, Duration::from_secs(10));
    opts.temperature = 0.0;

    let json = tokio::select! {
        _ = cancel_token.cancelled() => {
            return Err(cancelled_message());
        }
        json = ai_client::chat(provider, system, &user, &opts) => json?,
    };
    let content = ai_client::extract_content(&json).unwrap_or_default();
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

pub(super) async fn execute_ai_list_models(
    request: &WorkerRequest,
) -> Result<WorkerResultPayload, String> {
    let WorkerRequestPayload::AiListModels { api_url, api_key } = &request.payload else {
        return Err("ai model list payload mismatch".to_string());
    };
    let models = ai_client::list_models(api_url, api_key.as_deref()).await?;
    Ok(WorkerResultPayload::AiModelsListed { models })
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
mod inline_prompt_tests {
    use super::*;

    #[test]
    fn prompt_embeds_the_caret_marker_between_prefix_and_suffix() {
        let prompt = build_inline_prompt("let x = ", ";\n", Some("rust"), Some("main.rs"), &[], &[]);
        assert_eq!(prompt, "Language: rust\nFile: main.rs\n\nlet x = <|cursor|>;\n");
        let prompt = build_inline_prompt("a", "b", None, None, &[], &[]);
        assert!(prompt.contains("Language: unknown\nFile: untitled"));
        assert!(!prompt.contains("Other open files"));
    }

    #[test]
    fn prompt_puts_neighbour_files_before_the_current_file() {
        let neighbors = vec![("client.ts".to_string(), "export const api = 1;\n".to_string())];
        let prompt = build_inline_prompt("const x = ", ";", Some("typescript"), Some("app.ts"), &neighbors, &[]);
        let reference = prompt.find("--- client.ts ---\nexport const api = 1;").expect("neighbour block");
        let current = prompt.find("--- app.ts (insert at <|cursor|>) ---\nconst x = <|cursor|>;").expect("current block");
        assert!(reference < current);
        assert!(prompt.ends_with("const x = <|cursor|>;"));
    }

    #[test]
    fn prompt_lists_caret_line_diagnostics_before_the_code() {
        let diagnostics = vec!["Cannot find name 'Promies'. Did you mean 'Promise'?".to_string()];
        let prompt = build_inline_prompt(
            "await new Promies.",
            "",
            Some("typescript"),
            Some("app.ts"),
            &[],
            &diagnostics,
        );
        let listed = prompt
            .find("Diagnostics on the caret line:\n- Cannot find name 'Promies'. Did you mean 'Promise'?\n")
            .expect("diagnostics block");
        let code = prompt.find("await new Promies.<|cursor|>").expect("code");
        assert!(listed < code);
    }

    #[test]
    fn single_line_when_text_follows_the_caret_on_its_line() {
        assert!(inline_is_single_line(")\n  next"));
        assert!(inline_is_single_line("x"));
        assert!(!inline_is_single_line(""));
        assert!(!inline_is_single_line("\n  next"));
        assert!(!inline_is_single_line("   \n}"));
    }

    #[test]
    fn stop_sequences_end_a_line_or_a_block() {
        assert_eq!(inline_stop_sequences(true), vec!["\n".to_string()]);
        assert_eq!(inline_stop_sequences(false), vec!["\n\n".to_string()]);
    }
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
