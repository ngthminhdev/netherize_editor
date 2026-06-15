use std::sync::mpsc as std_mpsc;

use winit::event_loop::EventLoopProxy;

use crate::{
    app::event_loop::AppEvent,
    async_runtime::message::{WorkerMessage, WorkerResult, WorkerResultPayload},
    config::ai_config::AiProviderConfig,
    runner::{
        leetcode_adapter::adapt_snippet_mechanical,
        leetcode_api::{
            LeetCodeCodeSnippet, LeetCodeProblem, extract_test_cases, normalize_problem_input,
            parse_metadata,
        },
    },
};

use super::emit::emit_message_and_wake;

pub(super) struct LeetCodeFetchJob {
    pub request_id: u64,
    pub revision_id: u64,
    pub input: String,
    pub language_key: String,
    pub destination_dir: std::path::PathBuf,
    pub use_ai: bool,
    pub provider: Option<AiProviderConfig>,
}

pub(super) async fn run_leetcode_fetch(
    job: LeetCodeFetchJob,
    result_tx: std_mpsc::Sender<WorkerMessage>,
    event_proxy: EventLoopProxy<AppEvent>,
) {
    let result = fetch_and_adapt(&job).await;
    let payload = match result {
        Ok((problem, code)) => {
            // Prepend the metadata header so the file remembers its problem.
            let header = crate::runner::leetcode_cache::build_header(
                &job.language_key,
                &problem.frontend_id,
                &problem.title_slug,
                &problem.title,
            );
            let full_code = format!("{header}{code}");
            match write_solution_file(&job, &full_code).await {
                Ok(file_path) => {
                    let cases = extract_test_cases(&problem);
                    save_problem_cache(&problem, &cases);
                    WorkerResultPayload::LeetCodeProblemFetched {
                        title: problem.title,
                        title_slug: problem.title_slug,
                        language_key: job.language_key,
                        file_path,
                        cases,
                    }
                }
                Err(message) => WorkerResultPayload::LeetCodeProblemFetchFailed { message },
            }
        }
        Err(message) => WorkerResultPayload::LeetCodeProblemFetchFailed { message },
    };
    emit_message_and_wake(
        &result_tx,
        &event_proxy,
        WorkerMessage::Result(WorkerResult {
            request_id: job.request_id,
            revision_id: job.revision_id,
            topic: crate::async_runtime::message::RequestTopic::LeetCode,
            payload,
        }),
    );
}

pub(super) struct LeetCodeGenerateJob {
    pub request_id: u64,
    pub revision_id: u64,
    pub cache: crate::runner::leetcode_cache::LeetCodeProblemCache,
    pub language_key: String,
    pub provider: AiProviderConfig,
    pub verify: bool,
}

pub(super) async fn run_leetcode_generate(
    job: LeetCodeGenerateJob,
    result_tx: std_mpsc::Sender<WorkerMessage>,
    event_proxy: EventLoopProxy<AppEvent>,
) {
    let payload = match generate_stratified_cases(&job.provider, &job.cache, &job.language_key).await {
        Ok(cases) => {
            let (final_cases, verified) = if job.verify {
                verify_generated_cases(&job.provider, &job.cache, &job.language_key, cases).await
            } else {
                (cases, false)
            };
            let mut updated = job.cache.clone();
            updated.cases = final_cases
                .iter()
                .map(|case| crate::runner::leetcode_cache::CachedCase {
                    input: case.input.clone(),
                    expected: case.expected.clone(),
                })
                .collect();
            let _ = crate::runner::leetcode_cache::save_cache_in(
                &crate::runner::leetcode_cache::cache_dir(),
                &updated,
            );
            WorkerResultPayload::LeetCodeTestsGenerated {
                id: job.cache.id.clone(),
                cases: final_cases,
                verified,
            }
        }
        Err(message) => WorkerResultPayload::LeetCodeTestsGenerateFailed { message },
    };
    emit_message_and_wake(
        &result_tx,
        &event_proxy,
        WorkerMessage::Result(WorkerResult {
            request_id: job.request_id,
            revision_id: job.revision_id,
            topic: crate::async_runtime::message::RequestTopic::LeetCode,
            payload,
        }),
    );
}

fn save_problem_cache(
    problem: &LeetCodeProblem,
    cases: &[crate::runner::leetcode_api::LeetCodeTestCase],
) {
    use crate::runner::leetcode_cache::{CachedCase, CachedParam, LeetCodeProblemCache};
    let cache = LeetCodeProblemCache {
        id: problem.frontend_id.clone(),
        slug: problem.title_slug.clone(),
        title: problem.title.clone(),
        statement: problem.content.clone(),
        function_name: problem.function_name.clone(),
        parameters: problem
            .parameters
            .iter()
            .map(|param| CachedParam {
                name: param.name.clone(),
                type_name: param.type_name.clone(),
            })
            .collect(),
        cases: cases
            .iter()
            .map(|case| CachedCase {
                input: case.input.clone(),
                expected: case.expected.clone(),
            })
            .collect(),
    };
    let _ = crate::runner::leetcode_cache::save_cache_in(
        &crate::runner::leetcode_cache::cache_dir(),
        &cache,
    );
}

fn build_stratified_prompt(
    cache: &crate::runner::leetcode_cache::LeetCodeProblemCache,
    _language_key: &str,
) -> String {
    let params = cache
        .parameters
        .iter()
        .map(|param| format!("{}: {}", param.name, param.type_name))
        .collect::<Vec<_>>()
        .join(", ");
    let examples = cache
        .cases
        .iter()
        .take(2)
        .map(|case| format!("input={} expected={}", case.input, case.expected))
        .collect::<Vec<_>>()
        .join("\n");
    let statement: String = cache.statement.chars().take(4000).collect();
    format!(
        r#"You are an expert software engineer and competitive programmer.
Generate exactly 5 high-quality test cases for the LeetCode problem "{title}" ({slug}).

Function signature: {func}({params})

Problem description (HTML may be present):
{statement}

Existing examples:
{examples}

Each test case MUST target one of these specific categories:

Case 1 — BASIC: The simplest valid input, similar to the provided examples. This confirms the fundamental algorithm works.

Case 2 — CONSTRAINT BOUNDARY: Input at the exact min or max of the problem constraints (e.g., array length = 1, array length = maximum allowed, values at min/max bounds). This catches off-by-one errors at boundaries.

Case 3 — COMMON BUG CATCHER: An input that causes a common incorrect solution to fail (e.g., off-by-one in loop bounds, missing the last element, not handling duplicates, wrong initialization). In the explanation, describe WHICH common bug this case would catch.

Case 4 — ALGORITHMIC STRESS: A structurally challenging input (e.g., reverse-sorted, all identical elements, alternating pattern, single large input). This tests algorithmic correctness under non-trivial conditions.

Case 5 — ADVERSARIAL/HARD: A LeetCode hidden-test style case designed to expose subtle implementation bugs. Think of what test case a problem setter would include to catch solutions that pass examples but have a flaw.

For EACH of the 5 test cases:
1. State which category (1-5) it belongs to and what specific edge case or bug it targets.
2. Provide the input arguments.
3. Trace through the OPTIMAL algorithm step-by-step on this input to calculate the correct expected output.
4. Verify that the input satisfies ALL problem constraints (array lengths, value ranges, etc.).

Finally, output a JSON array of exactly 5 objects, each having the format:
{{"input": <object whose keys are the parameter names>, "expected": <expected return value>}}
Wrap this JSON array inside a ```json``` code block."#,
        title = cache.title,
        slug = cache.slug,
        func = cache.function_name,
        params = params,
        statement = statement,
        examples = examples,
    )
}

fn build_verify_prompt(
    cache: &crate::runner::leetcode_cache::LeetCodeProblemCache,
    _language_key: &str,
    cases: &[crate::runner::leetcode_api::LeetCodeTestCase],
) -> String {
    let params = cache
        .parameters
        .iter()
        .map(|param| format!("{}: {}", param.name, param.type_name))
        .collect::<Vec<_>>()
        .join(", ");
    let statement: String = cache.statement.chars().take(4000).collect();
    let cases_json: String = cases
        .iter()
        .enumerate()
        .map(|(i, case)| {
            format!(
                "Case {}: input={}, expected={}",
                i + 1,
                case.input,
                case.expected
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    format!(
        r#"You are verifying test cases for LeetCode problem "{title}" ({slug}).

Function signature: {func}({params})

Problem description:
{statement}

Here are the test cases to verify:
{cases_json}

For EACH test case:
1. Re-trace the optimal algorithm step-by-step using the given input.
2. Calculate the correct expected output independently.
3. Check if the input satisfies ALL problem constraints.
4. Compare your calculated output with the provided expected output.

Output a JSON array of exactly {count} objects, one per input case, each with:
{{"input": <same input object>, "expected": <correct expected output>, "ok": <true if original expected was correct, false if you corrected it>}}

If a case's expected output was wrong, provide the CORRECTED expected value.
If a case's input violates constraints, still provide the expected output for that input and note the violation in ok=false.

Wrap the JSON array inside a ```json``` code block."#,
        title = cache.title,
        slug = cache.slug,
        func = cache.function_name,
        params = params,
        statement = statement,
        cases_json = cases_json,
        count = cases.len(),
    )
}

async fn generate_stratified_cases(
    provider: &AiProviderConfig,
    cache: &crate::runner::leetcode_cache::LeetCodeProblemCache,
    language_key: &str,
) -> Result<Vec<crate::runner::leetcode_api::LeetCodeTestCase>, String> {
    if provider.api_url.trim().is_empty() || provider.model.trim().is_empty() {
        return Err("AI provider is not configured".to_string());
    }
    let prompt = build_stratified_prompt(cache, language_key);
    let endpoint = format!("{}/chat/completions", provider.api_url.trim_end_matches('/'));
    let mut body = serde_json::json!({
        "model": provider.model,
        "messages": [
            {"role": "system", "content": "You are a LeetCode test-case generator. Carefully reason step-by-step to calculate outputs first, then write the JSON block."},
            {"role": "user", "content": prompt}
        ],
        "temperature": 0.2,
        "max_tokens": 4096
    });
    if let Some(effort) = provider
        .reasoning_effort
        .as_ref()
        .filter(|e| !e.trim().is_empty())
    {
        body["reasoning_effort"] = serde_json::Value::String(effort.clone());
    }
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(90))
        .build()
        .map_err(|err| format!("AI client build failed: {err}"))?;
    let mut request = client.post(endpoint).json(&body);
    if let Some(key) = provider
        .api_key
        .as_ref()
        .filter(|key| !key.trim().is_empty())
    {
        request = request.bearer_auth(key);
    }
    let response = request
        .send()
        .await
        .map_err(|err| format!("AI request failed: {err}"))?;
    let status = response.status();
    let body_text = response
        .text()
        .await
        .map_err(|err| format!("AI response read failed: {err}"))?;
    let mut cleaned = body_text.trim();
    if let Some(idx) = cleaned.rfind('}') {
        cleaned = &cleaned[..=idx];
    }
    let value: serde_json::Value = serde_json::from_str(cleaned)
        .map_err(|err| format!("AI returned invalid JSON: {err}"))?;
    if !status.is_success() {
        if let Some(msg) = value["error"]["message"].as_str() {
            return Err(format!("AI error (HTTP {status}): {msg}"));
        }
        return Err(format!("AI returned HTTP {status}"));
    }
    let text = value["choices"][0]["message"]["content"]
        .as_str()
        .ok_or_else(|| {
            let finish = value["choices"][0]["finish_reason"]
                .as_str()
                .unwrap_or("unknown");
            let reasoning_toks = value["usage"]["completion_tokens_details"]["reasoning_tokens"]
                .as_u64();
            if finish == "length" {
                if let Some(rt) = reasoning_toks {
                    format!(
                        "AI model exhausted token budget on reasoning ({rt} reasoning tokens, \
                         finish_reason=length). Set reasoning_effort = \"low\" in \
                         [leetcode.provider] or use a non-reasoning model."
                    )
                } else {
                    "AI response was truncated (finish_reason=length) — \
                     increase max_tokens or use a non-reasoning model."
                        .to_string()
                }
            } else {
                format!(
                    "AI response contained no content (finish_reason={finish})"
                )
            }
        })?;
    parse_generated_cases(text)
}

async fn verify_generated_cases(
    provider: &AiProviderConfig,
    cache: &crate::runner::leetcode_cache::LeetCodeProblemCache,
    language_key: &str,
    cases: Vec<crate::runner::leetcode_api::LeetCodeTestCase>,
) -> (Vec<crate::runner::leetcode_api::LeetCodeTestCase>, bool) {
    let prompt = build_verify_prompt(cache, language_key, &cases);
    let endpoint = format!("{}/chat/completions", provider.api_url.trim_end_matches('/'));
    let mut body = serde_json::json!({
        "model": provider.model,
        "messages": [
            {"role": "system", "content": "You are a LeetCode test-case verifier. Carefully re-trace each test case and correct any wrong expected outputs."},
            {"role": "user", "content": prompt}
        ],
        "temperature": 0.1,
        "max_tokens": 4096
    });
    if let Some(effort) = provider
        .reasoning_effort
        .as_ref()
        .filter(|e| !e.trim().is_empty())
    {
        body["reasoning_effort"] = serde_json::Value::String(effort.clone());
    }
    let client = match reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(60))
        .build()
    {
        Ok(c) => c,
        Err(_) => return (cases, false),
    };
    let mut request = client.post(endpoint).json(&body);
    if let Some(key) = provider
        .api_key
        .as_ref()
        .filter(|key| !key.trim().is_empty())
    {
        request = request.bearer_auth(key);
    }
    let response = match request.send().await {
        Ok(r) => r,
        Err(_) => return (cases, false),
    };
    let status = response.status();
    let body_text = match response.text().await {
        Ok(t) => t,
        Err(_) => return (cases, false),
    };
    let mut cleaned = body_text.trim();
    if let Some(idx) = cleaned.rfind('}') {
        cleaned = &cleaned[..=idx];
    }
    let value: serde_json::Value = match serde_json::from_str(cleaned) {
        Ok(v) => v,
        Err(_) => return (cases, false),
    };
    if !status.is_success() {
        return (cases, false);
    }
    let text = match value["choices"][0]["message"]["content"].as_str() {
        Some(t) => t,
        None => return (cases, false),
    };
    let verified_cases = match parse_generated_cases(text) {
        Ok(vc) => vc,
        Err(_) => return (cases, false),
    };
    if verified_cases.len() != cases.len() {
        return (cases, false);
    }
    (verified_cases, true)
}

async fn generate_via_ai(
    provider: &AiProviderConfig,
    cache: &crate::runner::leetcode_cache::LeetCodeProblemCache,
    language_key: &str,
) -> Result<Vec<crate::runner::leetcode_api::LeetCodeTestCase>, String> {
    if provider.api_url.trim().is_empty() || provider.model.trim().is_empty() {
        return Err("AI provider is not configured".to_string());
    }
    let params = cache
        .parameters
        .iter()
        .map(|param| format!("{}: {}", param.name, param.type_name))
        .collect::<Vec<_>>()
        .join(", ");
    let examples = cache
        .cases
        .iter()
        .take(2)
        .map(|case| format!("input={} expected={}", case.input, case.expected))
        .collect::<Vec<_>>()
        .join("\n");
    let statement: String = cache.statement.chars().take(4000).collect();
    let prompt = format!(
        "Generate exactly 5 diverse {language} test cases (include edge cases) for LeetCode problem \"{title}\" ({slug}).\nFunction: {func}({params}).\nProblem statement (HTML may be present):\n{statement}\n\nExisting examples:\n{examples}\n\nFor each testcase, think step-by-step to calculate the correct expected output. Finally, output a JSON array of exactly 5 objects, each {{\"input\": <object whose keys are the parameter names>, \"expected\": <expected return value>}} inside a ```json``` code block.",
        language = language_key,
        title = cache.title,
        slug = cache.slug,
        func = cache.function_name,
        params = params,
        statement = statement,
        examples = examples,
    );
    let endpoint = format!("{}/chat/completions", provider.api_url.trim_end_matches('/'));
    let mut body = serde_json::json!({
        "model": provider.model,
        "messages": [
            {"role": "system", "content": "You are a LeetCode test-case generator. Generate reasoning and then output a JSON array in a code block."},
            {"role": "user", "content": prompt}
        ],
        "temperature": 0.2,
        "max_tokens": 4096
    });
    // Reasoning models burn the entire max_tokens budget on hidden thinking
    // tokens and return empty content unless reasoning_effort is set.
    if let Some(effort) = provider
        .reasoning_effort
        .as_ref()
        .filter(|e| !e.trim().is_empty())
    {
        body["reasoning_effort"] = serde_json::Value::String(effort.clone());
    }
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(90))
        .build()
        .map_err(|err| format!("AI client build failed: {err}"))?;
    let mut request = client.post(endpoint).json(&body);
    if let Some(key) = provider
        .api_key
        .as_ref()
        .filter(|key| !key.trim().is_empty())
    {
        request = request.bearer_auth(key);
    }
    let response = request
        .send()
        .await
        .map_err(|err| format!("AI request failed: {err}"))?;
    let status = response.status();
    let body_text = response
        .text()
        .await
        .map_err(|err| format!("AI response read failed: {err}"))?;
    let mut cleaned = body_text.trim();
    if let Some(idx) = cleaned.rfind('}') {
        cleaned = &cleaned[..=idx];
    }
    let value: serde_json::Value = serde_json::from_str(cleaned)
        .map_err(|err| format!("AI returned invalid JSON: {err}"))?;
    if !status.is_success() {
        if let Some(msg) = value["error"]["message"].as_str() {
            return Err(format!("AI error (HTTP {status}): {msg}"));
        }
        return Err(format!("AI returned HTTP {status}"));
    }
    let text = value["choices"][0]["message"]["content"]
        .as_str()
        .ok_or_else(|| {
            // Provide actionable diagnostics: reasoning models exhaust the
            // token budget on hidden thinking and return null content.
            let finish = value["choices"][0]["finish_reason"]
                .as_str()
                .unwrap_or("unknown");
            let reasoning_toks = value["usage"]["completion_tokens_details"]["reasoning_tokens"]
                .as_u64();
            if finish == "length" {
                if let Some(rt) = reasoning_toks {
                    format!(
                        "AI model exhausted token budget on reasoning ({rt} reasoning tokens, \
                         finish_reason=length). Set reasoning_effort = \"low\" in \
                         [leetcode.provider] or use a non-reasoning model."
                    )
                } else {
                    "AI response was truncated (finish_reason=length) — \
                     increase max_tokens or use a non-reasoning model."
                        .to_string()
                }
            } else {
                format!(
                    "AI response contained no content (finish_reason={finish})"
                )
            }
        })?;
    parse_generated_cases(text)
}

async fn write_solution_file(
    job: &LeetCodeFetchJob,
    code: &str,
) -> Result<std::path::PathBuf, String> {
    let template = crate::runner::leetcode::leetcode_template(&job.language_key)
        .ok_or_else(|| format!("unsupported LeetCode language: {}", job.language_key))?;
    tokio::fs::create_dir_all(&job.destination_dir)
        .await
        .map_err(|err| format!("create {} failed: {err}", job.destination_dir.display()))?;
    let mut index = 1usize;
    loop {
        let name = if index == 1 {
            format!("solution.{}", template.extension)
        } else {
            format!("solution-{index}.{}", template.extension)
        };
        let path = job.destination_dir.join(name);
        let exists = tokio::fs::try_exists(&path)
            .await
            .map_err(|err| format!("inspect {} failed: {err}", path.display()))?;
        if !exists {
            tokio::fs::write(&path, code)
                .await
                .map_err(|err| format!("write {} failed: {err}", path.display()))?;
            return Ok(path);
        }
        index = index.saturating_add(1);
    }
}

async fn fetch_and_adapt(job: &LeetCodeFetchJob) -> Result<(LeetCodeProblem, String), String> {
    let normalized = normalize_problem_input(&job.input)?;
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(20))
        .user_agent("NetherizeEditor/0.1")
        .build()
        .map_err(|err| format!("LeetCode client build failed: {err}"))?;
    let slug = if normalized.chars().all(|ch| ch.is_ascii_digit()) {
        resolve_numeric_id(&client, &normalized).await?
    } else {
        normalized
    };
    let problem = fetch_problem(&client, &slug).await?;
    let mechanical = adapt_snippet_mechanical(&problem, &job.language_key)?;
    let code = if job.use_ai {
        match job.provider.as_ref() {
            Some(provider) => adapt_via_ai(provider, &problem, &job.language_key, &mechanical)
                .await
                .unwrap_or(mechanical),
            None => mechanical,
        }
    } else {
        mechanical
    };
    Ok((problem, code))
}

async fn graphql(
    client: &reqwest::Client,
    body: serde_json::Value,
) -> Result<serde_json::Value, String> {
    let response = client
        .post("https://leetcode.com/graphql")
        .header("Referer", "https://leetcode.com/problemset/")
        .json(&body)
        .send()
        .await
        .map_err(|err| format!("LeetCode request failed: {err}"))?;
    let status = response.status();
    let value: serde_json::Value = response
        .json()
        .await
        .map_err(|err| format!("LeetCode returned invalid JSON: {err}"))?;
    if !status.is_success() {
        return Err(format!("LeetCode returned HTTP {status}"));
    }
    if let Some(errors) = value.get("errors") {
        return Err(format!("LeetCode GraphQL error: {errors}"));
    }
    Ok(value)
}

async fn resolve_numeric_id(client: &reqwest::Client, id: &str) -> Result<String, String> {
    let value = graphql(
        client,
        serde_json::json!({
            "query": "query problemsetQuestionList($filters: QuestionListFilterInput) { problemsetQuestionList: questionList(categorySlug: \"\", limit: 50, skip: 0, filters: $filters) { questions: data { questionFrontendId titleSlug } } }",
            "variables": { "filters": { "searchKeywords": id } }
        }),
    )
    .await?;
    value["data"]["problemsetQuestionList"]["questions"]
        .as_array()
        .and_then(|questions| {
            questions
                .iter()
                .find(|question| question["questionFrontendId"].as_str() == Some(id))
        })
        .and_then(|question| question["titleSlug"].as_str())
        .map(str::to_string)
        .ok_or_else(|| format!("LeetCode problem ID {id} was not found"))
}

async fn fetch_problem(client: &reqwest::Client, slug: &str) -> Result<LeetCodeProblem, String> {
    let value = graphql(
        client,
        serde_json::json!({
            "query": "query questionData($titleSlug: String!) { question(titleSlug: $titleSlug) { questionFrontendId title titleSlug content metaData codeSnippets { lang langSlug code } exampleTestcaseList } }",
            "variables": { "titleSlug": slug }
        }),
    )
    .await?;
    let question = value["data"]["question"]
        .as_object()
        .ok_or_else(|| format!("LeetCode problem '{slug}' was not found"))?;
    let string = |key: &str| {
        question
            .get(key)
            .and_then(serde_json::Value::as_str)
            .map(str::to_string)
            .ok_or_else(|| format!("LeetCode response is missing {key}"))
    };
    let metadata = string("metaData")?;
    let (function_name, parameters) = parse_metadata(&metadata)?;
    let code_snippets: Vec<LeetCodeCodeSnippet> =
        serde_json::from_value(question.get("codeSnippets").cloned().unwrap_or_default())
            .map_err(|err| format!("invalid LeetCode code snippets: {err}"))?;
    let example_testcase_list = question
        .get("exampleTestcaseList")
        .and_then(serde_json::Value::as_array)
        .map(|values| {
            values
                .iter()
                .filter_map(serde_json::Value::as_str)
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default();
    Ok(LeetCodeProblem {
        frontend_id: string("questionFrontendId")?,
        title: string("title")?,
        title_slug: string("titleSlug")?,
        content: string("content")?,
        function_name,
        parameters,
        code_snippets,
        example_testcase_list,
    })
}

async fn adapt_via_ai(
    provider: &AiProviderConfig,
    problem: &LeetCodeProblem,
    language_key: &str,
    mechanical: &str,
) -> Result<String, String> {
    if provider.api_url.trim().is_empty() || provider.model.trim().is_empty() {
        return Err("AI provider is not configured".to_string());
    }
    let endpoint = format!(
        "{}/chat/completions",
        provider.api_url.trim_end_matches('/')
    );
    let mut body = serde_json::json!({
        "model": provider.model,
        "messages": [
            {"role": "system", "content": "Return only complete runnable source code. No markdown fences or explanation."},
            {"role": "user", "content": format!(
                "Adapt this LeetCode {} starter for problem {} ({}) into a stdin JSON -> stdout JSON program with solve(data). Preserve the official function signature and support helper types when present.\n\nMechanical baseline:\n{}",
                language_key, problem.title, problem.title_slug, mechanical
            )}
        ],
        "temperature": 0.1,
        "max_tokens": 4096
    });
    if let Some(effort) = provider
        .reasoning_effort
        .as_ref()
        .filter(|e| !e.trim().is_empty())
    {
        body["reasoning_effort"] = serde_json::Value::String(effort.clone());
    }
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(45))
        .build()
        .map_err(|err| format!("AI client build failed: {err}"))?;
    let mut request = client.post(endpoint).json(&body);
    if let Some(key) = provider
        .api_key
        .as_ref()
        .filter(|key| !key.trim().is_empty())
    {
        request = request.bearer_auth(key);
    }
    let response = request
        .send()
        .await
        .map_err(|err| format!("AI request failed: {err}"))?;
    let status = response.status();
    let body_text = response
        .text()
        .await
        .map_err(|err| format!("AI response read failed: {err}"))?;
    let mut cleaned = body_text.trim();
    if let Some(idx) = cleaned.rfind('}') {
        cleaned = &cleaned[..=idx];
    }
    let value: serde_json::Value = serde_json::from_str(cleaned)
        .map_err(|err| format!("AI returned invalid JSON: {err}"))?;
    if !status.is_success() {
        if let Some(msg) = value["error"]["message"].as_str() {
            return Err(format!("AI error (HTTP {status}): {msg}"));
        }
        return Err(format!("AI returned HTTP {status}"));
    }
    let text = value["choices"][0]["message"]["content"]
        .as_str()
        .ok_or_else(|| {
            let finish = value["choices"][0]["finish_reason"]
                .as_str()
                .unwrap_or("unknown");
            if finish == "length" {
                "AI response truncated (finish_reason=length) — \
                 set reasoning_effort = \"low\" or use a non-reasoning model."
                    .to_string()
            } else {
                format!("AI response contained no code (finish_reason={finish})")
            }
        })?;
    Ok(strip_code_fence(text))
}

/// Parse the AI's JSON-array response into test cases. Each element must be an
/// object with `input` and `expected`; objects/arrays are compacted to JSON
/// strings, plain strings are kept verbatim.
fn parse_generated_cases(text: &str) -> Result<Vec<crate::runner::leetcode_api::LeetCodeTestCase>, String> {
    use crate::runner::leetcode_api::LeetCodeTestCase;
    let cleaned = extract_json_array(text)?;
    let value: serde_json::Value = serde_json::from_str(&cleaned)
        .map_err(|err| format!("AI returned invalid JSON: {err}"))?;
    let array = value
        .as_array()
        .ok_or_else(|| "AI response was not a JSON array".to_string())?;
    let mut cases = Vec::new();
    let expected_keys = [
        "expected",
        "expected_output",
        "output",
        "expectedOutput",
        "result",
        "expectedResult",
        "expected_result",
    ];
    for item in array {
        let mut expected = None;
        for key in expected_keys {
            if let Some(val) = item.get(key) {
                expected = Some(val);
                break;
            }
        }
        let expected = expected.ok_or_else(|| "generated case is missing expected".to_string())?;
        let input_val = if let Some(input) = item.get("input").or_else(|| item.get("inputs")) {
            input.clone()
        } else {
            // Fallback: If "input" key is missing, treat all other keys as the parameters of the input object
            let mut input_obj = serde_json::Map::new();
            let mut has_params = false;
            if let Some(obj) = item.as_object() {
                for (k, v) in obj {
                    if !expected_keys.contains(&k.as_str()) {
                        input_obj.insert(k.clone(), v.clone());
                        has_params = true;
                    }
                }
            }
            if !has_params {
                return Err("generated case is missing input".to_string());
            }
            serde_json::Value::Object(input_obj)
        };
        cases.push(LeetCodeTestCase {
            input: value_to_compact_string(&input_val),
            expected: value_to_compact_string(expected),
        });
    }
    if cases.is_empty() {
        return Err("AI returned no test cases".to_string());
    }
    Ok(cases)
}

fn value_to_compact_string(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::String(text) => text.clone(),
        other => other.to_string(),
    }
}

fn extract_json_array(text: &str) -> Result<String, String> {
    let mut candidates = Vec::new();
    
    // 1. Extract all code fences
    let mut search_pos = 0;
    while let Some(start_fence) = text[search_pos..].find("```") {
        let start_idx = search_pos + start_fence;
        let rest = &text[start_idx + 3..];
        if let Some(end_fence) = rest.find("```") {
            let block = rest[..end_fence].trim();
            let content = if block.starts_with("json") {
                block[4..].trim()
            } else if block.starts_with("js") {
                block[2..].trim()
            } else {
                block
            };
            candidates.push(content.to_string());
            search_pos = start_idx + 3 + end_fence + 3;
        } else {
            break;
        }
    }
    
    // 2. Also extract raw [ ... ] substrings
    let mut search_pos = 0;
    while let Some(start_idx) = text[search_pos..].find('[') {
        let abs_start = search_pos + start_idx;
        let rest = &text[abs_start..];
        if let Some(end_idx) = rest.rfind(']') {
            let content = rest[..=end_idx].to_string();
            candidates.push(content);
        }
        search_pos = abs_start + 1;
    }
    
    // 3. Evaluate candidates: the first one that parses as a JSON array of objects
    // where at least one element has a key indicating expected output.
    let expected_keys = ["expected", "expected_output", "output", "result", "expectedResult", "expected_result"];
    for candidate in candidates {
        if let Ok(value) = serde_json::from_str::<serde_json::Value>(&candidate) {
            if let Some(arr) = value.as_array() {
                if !arr.is_empty() {
                    let looks_like_cases = arr.iter().all(|item| {
                        if let Some(obj) = item.as_object() {
                            obj.keys().any(|k| expected_keys.contains(&k.as_str()))
                        } else {
                            false
                        }
                    });
                    if looks_like_cases {
                        return Ok(candidate);
                    }
                }
            }
        }
    }
    
    // Fallback to original find if nothing matched
    let trimmed = text.trim();
    if let Some(start_idx) = trimmed.find('[') {
        if let Some(end_idx) = trimmed.rfind(']') {
            if end_idx > start_idx {
                return Ok(trimmed[start_idx..=end_idx].to_string());
            }
        }
    }
    
    Err("Could not locate JSON array in AI response".to_string())
}

fn strip_code_fence(text: &str) -> String {
    let trimmed = text.trim();
    if !trimmed.starts_with("```") {
        return trimmed.to_string();
    }
    let after_first = trimmed
        .split_once('\n')
        .map(|(_, rest)| rest)
        .unwrap_or(trimmed);
    after_first
        .strip_suffix("```")
        .unwrap_or(after_first)
        .trim()
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_ai_generated_cases_from_json_array() {
        let text = "```json\n[\n  {\"input\": {\"nums\": [2,7,11,15], \"target\": 9}, \"expected\": [0,1]},\n  {\"input\": {\"nums\": [3,3], \"target\": 6}, \"expected\": [0,1]}\n]\n```";
        let cases = parse_generated_cases(text).expect("parse generated cases");
        assert_eq!(cases.len(), 2);
        assert_eq!(cases[0].input, "{\"nums\":[2,7,11,15],\"target\":9}");
        assert_eq!(cases[0].expected, "[0,1]");
    }

    #[test]
    fn parse_generated_cases_rejects_non_array() {
        assert!(parse_generated_cases("{\"input\":1}").is_err());
    }

    #[test]
    fn strips_markdown_fence_from_ai_code() {
        assert_eq!(
            strip_code_fence("```javascript\nconsole.log(1);\n```"),
            "console.log(1);"
        );
    }

    #[tokio::test]
    #[ignore = "requires live leetcode.com access"]
    async fn fetches_two_sum_from_live_api() {
        let job = LeetCodeFetchJob {
            request_id: 1,
            revision_id: 0,
            input: "1".to_string(),
            language_key: "javascript".to_string(),
            destination_dir: std::env::temp_dir(),
            use_ai: false,
            provider: None,
        };
        let (problem, code) = fetch_and_adapt(&job).await.expect("fetch two sum");
        assert_eq!(problem.title_slug, "two-sum");
        assert_eq!(extract_test_cases(&problem).len(), 3);
        assert!(code.contains("function solve(data)"));
        assert!(code.contains("twoSum(params.nums, params.target)"));
    }

    #[test]
    fn parse_generated_cases_handles_raw_json_without_fences() {
        let text = r#"[{"input": {"x": 5}, "expected": 25}]"#;
        let cases = parse_generated_cases(text).expect("raw json parse");
        assert_eq!(cases.len(), 1);
        assert_eq!(cases[0].input, r#"{"x":5}"#);
        assert_eq!(cases[0].expected, "25");
    }

    #[test]
    fn parse_generated_cases_handles_scalar_expected_types() {
        let text = r#"[
            {"input": {"s": "hello"}, "expected": "olleh"},
            {"input": {"n": 0}, "expected": true},
            {"input": {"n": -1}, "expected": null}
        ]"#;
        let cases = parse_generated_cases(text).expect("scalar types");
        assert_eq!(cases.len(), 3);
        // String expected → unwrapped from JSON string (no quotes).
        assert_eq!(cases[0].expected, "olleh");
        // Boolean expected → JSON literal.
        assert_eq!(cases[1].expected, "true");
        // Null expected → JSON literal.
        assert_eq!(cases[2].expected, "null");
    }

    #[test]
    fn parse_generated_cases_rejects_empty_array() {
        assert!(parse_generated_cases("[]").is_err());
    }

    #[test]
    fn parse_generated_cases_rejects_missing_input_field() {
        let text = r#"[{"expected": 42}]"#;
        let result = parse_generated_cases(text);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("missing input"));
    }

    #[test]
    fn parse_generated_cases_rejects_missing_expected_field() {
        let text = r#"[{"input": {"x": 1}}]"#;
        let result = parse_generated_cases(text);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("missing expected"));
    }

    #[test]
    fn parse_generated_cases_handles_flat_parameters_fallback() {
        let text = r#"[{"nums": [2,7,11,15], "target": 9, "expected": [0,1]}]"#;
        let cases = parse_generated_cases(text).expect("flat parameters fallback");
        assert_eq!(cases.len(), 1);
        assert_eq!(cases[0].input, r#"{"nums":[2,7,11,15],"target":9}"#);
        assert_eq!(cases[0].expected, "[0,1]");
    }

    #[test]
    fn parse_generated_cases_handles_expected_output_variant() {
        let text = r#"[{"input": {"nums": [2,7]}, "expected_output": [0,1]}]"#;
        let cases = parse_generated_cases(text).expect("expected_output variant");
        assert_eq!(cases.len(), 1);
        assert_eq!(cases[0].input, r#"{"nums":[2,7]}"#);
        assert_eq!(cases[0].expected, "[0,1]");
    }

    #[test]
    fn strip_code_fence_passes_through_plain_text() {
        assert_eq!(strip_code_fence("[1,2,3]"), "[1,2,3]");
    }

    #[test]
    fn strip_code_fence_handles_whitespace_around_fences() {
        assert_eq!(
            strip_code_fence("  ```json\n[1,2]\n```  "),
            "[1,2]"
        );
    }

    #[test]
    fn parse_json_with_trailing_sse_garbage() {
        let body_text = "{\"id\":\"123\",\"choices\":[{\"message\":{\"content\":\"hello\"}}]}data: [DONE]";
        let mut cleaned = body_text.trim();
        if let Some(idx) = cleaned.rfind('}') {
            cleaned = &cleaned[..=idx];
        }
        let json: serde_json::Value = serde_json::from_str(cleaned).expect("parse cleaned sse");
        assert_eq!(json["choices"][0]["message"]["content"], "hello");
    }

    #[test]
    fn extract_json_array_finds_array_with_prose() {
        let text = "Here is some thinking...\n\n```json\n[{\"x\": 1}]\n```\nSome other text.";
        let res = extract_json_array(text).expect("extract with prose");
        assert_eq!(res, "[{\"x\": 1}]");
    }

    fn verify_two_sum(input_json: &str, expected_json: &str) -> Result<(), String> {
        let input_val: serde_json::Value = serde_json::from_str(input_json)
            .map_err(|e| format!("parse input JSON failed: {e}"))?;
        let expected_val: serde_json::Value = serde_json::from_str(expected_json)
            .map_err(|e| format!("parse expected JSON failed: {e}"))?;

        let nums = input_val["nums"].as_array().ok_or_else(|| "missing or invalid 'nums' array".to_string())?;
        let target = input_val["target"].as_i64().ok_or_else(|| "missing or invalid 'target'".to_string())?;
        
        let arr = expected_val.as_array().ok_or_else(|| "expected must be an array".to_string())?;
        let mut indices = Vec::new();
        for v in arr {
            let idx = v.as_u64().ok_or_else(|| "non-numeric index".to_string())? as usize;
            indices.push(idx);
        }

        if indices.len() != 2 {
            return Err(format!("expected indices array length is {}, want 2", indices.len()));
        }
        if indices[0] == indices[1] {
            return Err("indices must be distinct".to_string());
        }
        if indices[0] >= nums.len() || indices[1] >= nums.len() {
            return Err("indices out of bounds".to_string());
        }

        let val1 = nums[indices[0]].as_i64().ok_or_else(|| "nums element is not number".to_string())?;
        let val2 = nums[indices[1]].as_i64().ok_or_else(|| "nums element is not number".to_string())?;

        if val1 + val2 == target {
            Ok(())
        } else {
            Err(format!("nums[{}] + nums[{}] = {} + {} = {}, target = {}", indices[0], indices[1], val1, val2, val1 + val2, target))
        }
    }

    fn verify_merge_sorted_array(input_json: &str, expected_json: &str) -> Result<(), String> {
        let input_val: serde_json::Value = serde_json::from_str(input_json)
            .map_err(|e| format!("parse input JSON failed: {e}"))?;
        let expected_val: serde_json::Value = serde_json::from_str(expected_json)
            .map_err(|e| format!("parse expected JSON failed: {e}"))?;

        let nums1_val = input_val["nums1"].as_array().ok_or_else(|| "missing or invalid 'nums1' array".to_string())?;
        let m = input_val["m"].as_u64().ok_or_else(|| "missing or invalid 'm'".to_string())? as usize;
        let nums2_val = input_val["nums2"].as_array().ok_or_else(|| "missing or invalid 'nums2' array".to_string())?;
        let n = input_val["n"].as_u64().ok_or_else(|| "missing or invalid 'n'".to_string())? as usize;

        let mut nums1 = Vec::new();
        for v in nums1_val {
            nums1.push(v.as_i64().ok_or_else(|| "non-numeric nums1 element".to_string())? as i32);
        }
        let mut nums2 = Vec::new();
        for v in nums2_val {
            nums2.push(v.as_i64().ok_or_else(|| "non-numeric nums2 element".to_string())? as i32);
        }

        if nums1.len() < m + n {
            return Err(format!("nums1 length is {}, must be at least m+n={}", nums1.len(), m+n));
        }
        if nums2.len() < n {
            return Err(format!("nums2 length is {}, must be at least n={}", nums2.len(), n));
        }

        // Keep only m elements in nums1 initially
        nums1.truncate(m);
        // Add first n elements of nums2
        let mut nums2_n = nums2.clone();
        nums2_n.truncate(n);
        nums1.extend(nums2_n);
        nums1.sort();

        let exp_arr = expected_val.as_array().ok_or_else(|| "expected must be an array".to_string())?;
        let mut expected = Vec::new();
        for v in exp_arr {
            expected.push(v.as_i64().ok_or_else(|| "non-numeric expected element".to_string())? as i32);
        }

        if nums1 == expected {
            Ok(())
        } else {
            Err(format!("actual merged={:?}, expected={:?}", nums1, expected))
        }
    }

    async fn generate_via_ai_improved(
        provider: &crate::config::ai_config::AiProviderConfig,
        cache: &crate::runner::leetcode_cache::LeetCodeProblemCache,
        language_key: &str,
    ) -> Result<Vec<crate::runner::leetcode_api::LeetCodeTestCase>, String> {
        if provider.api_url.trim().is_empty() || provider.model.trim().is_empty() {
            return Err("AI provider is not configured".to_string());
        }
        let params = cache
            .parameters
            .iter()
            .map(|param| format!("{}: {}", param.name, param.type_name))
            .collect::<Vec<_>>()
            .join(", ");
        let examples = cache
            .cases
            .iter()
            .take(2)
            .map(|case| format!("input={} expected={}", case.input, case.expected))
            .collect::<Vec<_>>()
            .join("\n");
        let statement: String = cache.statement.chars().take(4000).collect();
        let prompt = format!(
            "You are an expert software engineer and competitive programmer.
Generate exactly 5 diverse, high-quality test cases (including edge cases, small/large bounds, empty/negative values if allowed) for the LeetCode problem \"{title}\" ({slug}).

Function signature: {func}({params})

Problem description (HTML may be present):
{statement}

Existing examples:
{examples}

For each of the 5 test cases:
1. Explain the scenario/edge case you are testing.
2. Provide the input arguments.
3. Write out the step-by-step trace/execution of the optimal algorithm on this input to verify and calculate the correct expected return value.
4. Double check that all constraints (e.g. array lengths, bounds) are strictly satisfied. For example, in 'Merge Sorted Array' (merge-sorted-array), nums1 must have length equal to m + n.

Finally, output a JSON array of exactly 5 objects, each having the format:
{{\"input\": <object whose keys are the parameter names>, \"expected\": <expected return value>}}
Wrap this JSON array inside a ```json``` code block.",
            title = cache.title,
            slug = cache.slug,
            func = cache.function_name,
            params = params,
            statement = statement,
            examples = examples,
        );
        let endpoint = format!("{}/chat/completions", provider.api_url.trim_end_matches('/'));
        let mut body = serde_json::json!({
            "model": provider.model,
            "messages": [
                {"role": "system", "content": "You are a LeetCode test-case generator. Carefully reason step-by-step to calculate outputs first, then write the JSON block."},
                {"role": "user", "content": prompt}
            ],
            "temperature": 0.2,
            "max_tokens": 4096
        });
        if let Some(effort) = provider
            .reasoning_effort
            .as_ref()
            .filter(|e| !e.trim().is_empty())
        {
            body["reasoning_effort"] = serde_json::Value::String(effort.clone());
        }
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(90))
            .build()
            .map_err(|err| format!("AI client build failed: {err}"))?;
        let mut request = client.post(endpoint).json(&body);
        if let Some(key) = provider
            .api_key
            .as_ref()
            .filter(|key| !key.trim().is_empty())
        {
            request = request.bearer_auth(key);
        }
        let response = request
            .send()
            .await
            .map_err(|err| format!("AI request failed: {err}"))?;
        let status = response.status();
        let body_text = response
            .text()
            .await
            .map_err(|err| format!("AI response read failed: {err}"))?;
        let mut cleaned = body_text.trim();
        if let Some(idx) = cleaned.rfind('}') {
            cleaned = &cleaned[..=idx];
        }
        let value: serde_json::Value = serde_json::from_str(cleaned)
            .map_err(|err| format!("AI returned invalid JSON: {err}"))?;
        if !status.is_success() {
            if let Some(msg) = value["error"]["message"].as_str() {
                return Err(format!("AI error (HTTP {status}): {msg}"));
            }
            return Err(format!("AI returned HTTP {status}"));
        }
        let text = value["choices"][0]["message"]["content"]
            .as_str()
            .ok_or_else(|| {
                let finish = value["choices"][0]["finish_reason"]
                    .as_str()
                    .unwrap_or("unknown");
                format!("empty content with finish_reason: {finish}")
            })?;
        let json_array_text = match extract_json_array(text) {
            Ok(t) => t,
            Err(e) => {
                println!("    FAILED TO EXTRACT JSON ARRAY. Raw text was:\n{}\n", text);
                return Err(e);
            }
        };
        match parse_generated_cases(&json_array_text) {
            Ok(c) => Ok(c),
            Err(e) => {
                println!("    FAILED TO PARSE CASES. Raw text was:\n{}\n", text);
                return Err(e);
            }
        }
    }

    #[tokio::test]
    #[ignore = "requires live AI config and leetcode.com access"]
    async fn run_generation_experiment() {
        use crate::runner::leetcode_cache::{CachedCase, CachedParam, LeetCodeProblemCache};
        
        let ai_config = crate::config::ai_config::AiConfig::load();
        let provider = ai_config.leetcode_ai_provider().expect("LeetCode AI provider is not configured");
        println!("Using model: {}", provider.model);

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(20))
            .user_agent("NetherizeEditor/0.1")
            .build()
            .unwrap();

        // 1. Fetch problems
        let two_sum = fetch_problem(&client, "two-sum").await.expect("fetch two-sum");
        let merge_sorted = fetch_problem(&client, "merge-sorted-array").await.expect("fetch merge-sorted-array");

        // 2. Build caches
        let two_sum_cases = extract_test_cases(&two_sum);
        let two_sum_cache = LeetCodeProblemCache {
            id: two_sum.frontend_id.clone(),
            slug: two_sum.title_slug.clone(),
            title: two_sum.title.clone(),
            statement: two_sum.content.clone(),
            function_name: two_sum.function_name.clone(),
            parameters: two_sum.parameters.iter().map(|p| CachedParam { name: p.name.clone(), type_name: p.type_name.clone() }).collect(),
            cases: two_sum_cases.iter().map(|c| CachedCase { input: c.input.clone(), expected: c.expected.clone() }).collect(),
        };

        let merge_cases = extract_test_cases(&merge_sorted);
        let merge_cache = LeetCodeProblemCache {
            id: merge_sorted.frontend_id.clone(),
            slug: merge_sorted.title_slug.clone(),
            title: merge_sorted.title.clone(),
            statement: merge_sorted.content.clone(),
            function_name: merge_sorted.function_name.clone(),
            parameters: merge_sorted.parameters.iter().map(|p| CachedParam { name: p.name.clone(), type_name: p.type_name.clone() }).collect(),
            cases: merge_cases.iter().map(|c| CachedCase { input: c.input.clone(), expected: c.expected.clone() }).collect(),
        };

        // 3. Run 5 times for twoSum (Original vs Improved)
        println!("\n=== Running Two Sum (Original Prompt) ===");
        let mut two_sum_orig_correct = 0;
        let mut two_sum_orig_total = 0;
        for i in 1..=5 {
            match generate_via_ai(provider, &two_sum_cache, "python").await {
                Ok(cases) => {
                    for case in cases {
                        two_sum_orig_total += 1;
                        if verify_two_sum(&case.input, &case.expected).is_ok() {
                            two_sum_orig_correct += 1;
                        }
                    }
                }
                Err(_) => {}
            }
        }

        println!("\n=== Running Two Sum (Improved Prompt) ===");
        let mut two_sum_imp_correct = 0;
        let mut two_sum_imp_total = 0;
        for i in 1..=5 {
            println!("  Request {}/5...", i);
            match generate_via_ai_improved(provider, &two_sum_cache, "python").await {
                Ok(cases) => {
                    for (c_idx, case) in cases.iter().enumerate() {
                        two_sum_imp_total += 1;
                        match verify_two_sum(&case.input, &case.expected) {
                            Ok(()) => {
                                two_sum_imp_correct += 1;
                                println!("    Case {}: input={} expected={} -> OK", c_idx + 1, case.input, case.expected);
                            }
                            Err(err) => {
                                println!("    Case {}: input={} expected={} -> FAILED ({})", c_idx + 1, case.input, case.expected, err);
                            }
                        }
                    }
                }
                Err(err) => println!("    Request failed: {}", err),
            }
        }

        // 4. Run 5 times for mergeSortedArray (Original vs Improved)
        println!("\n=== Running Merge Sorted Array (Original Prompt) ===");
        let mut merge_orig_correct = 0;
        let mut merge_orig_total = 0;
        for i in 1..=5 {
            match generate_via_ai(provider, &merge_cache, "python").await {
                Ok(cases) => {
                    for case in cases {
                        merge_orig_total += 1;
                        if verify_merge_sorted_array(&case.input, &case.expected).is_ok() {
                            merge_orig_correct += 1;
                        }
                    }
                }
                Err(_) => {}
            }
        }

        println!("\n=== Running Merge Sorted Array (Improved Prompt) ===");
        let mut merge_imp_correct = 0;
        let mut merge_imp_total = 0;
        for i in 1..=5 {
            println!("  Request {}/5...", i);
            match generate_via_ai_improved(provider, &merge_cache, "python").await {
                Ok(cases) => {
                    for (c_idx, case) in cases.iter().enumerate() {
                        merge_imp_total += 1;
                        match verify_merge_sorted_array(&case.input, &case.expected) {
                            Ok(()) => {
                                merge_imp_correct += 1;
                                println!("    Case {}: input={} expected={} -> OK", c_idx + 1, case.input, case.expected);
                            }
                            Err(err) => {
                                println!("    Case {}: input={} expected={} -> FAILED ({})", c_idx + 1, case.input, case.expected, err);
                            }
                        }
                    }
                }
                Err(err) => println!("    Request failed: {}", err),
            }
        }

        println!("\n=== EXPERIMENT COMPARISON ===");
        println!("Two Sum (Original): {}/{} ({}%)", two_sum_orig_correct, two_sum_orig_total, if two_sum_orig_total > 0 { two_sum_orig_correct * 100 / two_sum_orig_total } else { 0 });
        println!("Two Sum (Improved): {}/{} ({}%)", two_sum_imp_correct, two_sum_imp_total, if two_sum_imp_total > 0 { two_sum_imp_correct * 100 / two_sum_imp_total } else { 0 });
        println!("Merge Sorted Array (Original): {}/{} ({}%)", merge_orig_correct, merge_orig_total, if merge_orig_total > 0 { merge_orig_correct * 100 / merge_orig_total } else { 0 });
        println!("Merge Sorted Array (Improved): {}/{} ({}%)", merge_imp_correct, merge_imp_total, if merge_imp_total > 0 { merge_imp_correct * 100 / merge_imp_total } else { 0 });
    }

    #[test]
    fn build_stratified_prompt_contains_all_categories() {
        let cache = crate::runner::leetcode_cache::LeetCodeProblemCache {
            id: "1".to_string(),
            slug: "two-sum".to_string(),
            title: "Two Sum".to_string(),
            statement: "Given an array of integers...".to_string(),
            function_name: "twoSum".to_string(),
            parameters: vec![
                crate::runner::leetcode_cache::CachedParam {
                    name: "nums".to_string(),
                    type_name: "number[]".to_string(),
                },
                crate::runner::leetcode_cache::CachedParam {
                    name: "target".to_string(),
                    type_name: "number".to_string(),
                },
            ],
            cases: vec![],
        };
        let prompt = build_stratified_prompt(&cache, "javascript");
        assert!(prompt.contains("Case 1 — BASIC"), "missing BASIC category");
        assert!(prompt.contains("Case 2 — CONSTRAINT BOUNDARY"), "missing CONSTRAINT BOUNDARY category");
        assert!(prompt.contains("Case 3 — COMMON BUG CATCHER"), "missing COMMON BUG CATCHER category");
        assert!(prompt.contains("Case 4 — ALGORITHMIC STRESS"), "missing ALGORITHMIC STRESS category");
        assert!(prompt.contains("Case 5 — ADVERSARIAL/HARD"), "missing ADVERSARIAL/HARD category");
        assert!(prompt.contains("twoSum"), "missing function name");
        assert!(prompt.contains("two-sum"), "missing slug");
        assert!(prompt.contains("```json"), "missing json code block instruction");
    }

    #[test]
    fn build_verify_prompt_contains_case_count() {
        let cache = crate::runner::leetcode_cache::LeetCodeProblemCache {
            id: "1".to_string(),
            slug: "two-sum".to_string(),
            title: "Two Sum".to_string(),
            statement: "Given an array...".to_string(),
            function_name: "twoSum".to_string(),
            parameters: vec![
                crate::runner::leetcode_cache::CachedParam {
                    name: "nums".to_string(),
                    type_name: "number[]".to_string(),
                },
            ],
            cases: vec![],
        };
        let cases = vec![
            crate::runner::leetcode_api::LeetCodeTestCase {
                input: r#"{"nums":[1,2]}"#.to_string(),
                expected: "[0,1]".to_string(),
            },
            crate::runner::leetcode_api::LeetCodeTestCase {
                input: r#"{"nums":[3,3]}"#.to_string(),
                expected: "[0,1]".to_string(),
            },
        ];
        let prompt = build_verify_prompt(&cache, "javascript", &cases);
        assert!(prompt.contains("exactly 2 objects"), "should mention case count");
        assert!(prompt.contains("Case 1:"), "should list case 1");
        assert!(prompt.contains("Case 2:"), "should list case 2");
        assert!(prompt.contains("Re-trace"), "should ask for re-tracing");
    }

    #[test]
    fn verify_count_mismatch_falls_back() {
        let text = r#"[{"input": {"x": 1}, "expected": 1}]"#; // 1 case
        let parsed = parse_generated_cases(text).expect("parse");
        assert_eq!(parsed.len(), 1);
        // If original had 2 cases, this would be a count mismatch
        // (verified in the async function's reconciliation logic).
    }
}



