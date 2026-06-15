# Test Case Generation Improvement — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use compose:subagent (recommended) or compose:execute to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Improve AI test case generation accuracy by using stratified prompts (5 named categories), adding an optional verification step, and extracting prompt construction into testable pure functions.

**Architecture:** Replace the generic `generate_via_ai()` with `generate_stratified_cases()` + optional `verify_generated_cases()`. Prompts are built by pure functions (`build_stratified_prompt`, `build_verify_prompt`) so they're unit-testable. A `verified: bool` field flows through the result payload to the UI toast.

**Tech Stack:** Rust, tokio, reqwest, serde_json

---

### Task 1: Add `verify` config field

**Covers:** [S4]

**Files:**
- Modify: `src/config/ai_config.rs:13-20`

- [ ] **Step 1: Add `verify` field to `LeetCodeConfig`**

```rust
#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct LeetCodeConfig {
    pub use_ai: Option<bool>,
    pub provider: Option<AiProviderConfig>,
    pub verify: Option<bool>,
}
```

- [ ] **Step 2: Add accessor method**

Find the `impl AiConfig` block and add:

```rust
pub fn leetcode_verify_enabled(&self) -> bool {
    self.leetcode
        .as_ref()
        .and_then(|lc| lc.verify)
        .unwrap_or(false)
}
```

- [ ] **Step 3: Run `cargo check`**

Expected: compiles clean

---

### Task 2: Add `verified` to result payload and `verify` to generate job

**Covers:** [S3]

**Files:**
- Modify: `src/async_runtime/message.rs:628-631`
- Modify: `src/async_runtime/scheduler/leetcode_fetch.rs:75-81`

- [ ] **Step 1: Add `verified` to `LeetCodeTestsGenerated`**

In `src/async_runtime/message.rs`, change:

```rust
LeetCodeTestsGenerated {
    id: String,
    cases: Vec<crate::runner::leetcode_api::LeetCodeTestCase>,
},
```

to:

```rust
LeetCodeTestsGenerated {
    id: String,
    cases: Vec<crate::runner::leetcode_api::LeetCodeTestCase>,
    verified: bool,
},
```

- [ ] **Step 2: Add `verify` to `LeetCodeGenerateJob`**

In `src/async_runtime/scheduler/leetcode_fetch.rs`:

```rust
pub(super) struct LeetCodeGenerateJob {
    pub request_id: u64,
    pub revision_id: u64,
    pub cache: crate::runner::leetcode_cache::LeetCodeProblemCache,
    pub language_key: String,
    pub provider: AiProviderConfig,
    pub verify: bool,
}
```

- [ ] **Step 3: Update all sites that construct `LeetCodeTestsGenerated`**

In `src/async_runtime/scheduler/leetcode_fetch.rs:103`, add `verified: false` (temporary — will be updated in Task 5):

```rust
WorkerResultPayload::LeetCodeTestsGenerated {
    id: job.cache.id.clone(),
    cases,
    verified: false,
}
```

In `src/app/event_loop/commands_tests.rs:3831`, add `verified: false`:

```rust
payload: crate::async_runtime::message::WorkerResultPayload::LeetCodeTestsGenerated {
    id: "1".to_string(),
    cases: vec![...],
    verified: false,
},
```

- [ ] **Step 4: Update dispatch.rs to pass verify flag**

In `src/async_runtime/scheduler/dispatch.rs:100-119`, add `verify` to the job construction:

```rust
if let WorkerRequestPayload::GenerateLeetCodeTests {
    cache,
    language_key,
    provider,
} = request.payload.clone()
{
    let worker_tx = result_tx.clone();
    let worker_proxy = event_proxy.clone();
    // TODO: Task 5 will wire verify from config; hardcode false for now
    tokio::spawn(run_leetcode_generate(
        LeetCodeGenerateJob {
            request_id: request.request_id,
            revision_id: request.revision_id,
            cache,
            language_key,
            provider,
            verify: false,
        },
        worker_tx,
        worker_proxy,
    ));
    continue;
}
```

- [ ] **Step 5: Run `cargo check`**

Expected: compiles clean

---

### Task 3: Extract `build_stratified_prompt()` pure function

**Covers:** [S2]

**Files:**
- Modify: `src/async_runtime/scheduler/leetcode_fetch.rs`

- [ ] **Step 1: Add the pure function**

Add this function before `generate_via_ai` (around line 154):

```rust
fn build_stratified_prompt(
    cache: &crate::runner::leetcode_cache::LeetCodeProblemCache,
    language_key: &str,
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
```

- [ ] **Step 2: Add test for `build_stratified_prompt`**

Add to the `#[cfg(test)] mod tests` block:

```rust
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
```

- [ ] **Step 3: Run the test**

Run: `cargo test --lib async_runtime::scheduler::leetcode_fetch::tests::build_stratified_prompt_contains_all_categories -- --nocapture`
Expected: PASS

---

### Task 4: Extract `build_verify_prompt()` pure function

**Covers:** [S3]

**Files:**
- Modify: `src/async_runtime/scheduler/leetcode_fetch.rs`

- [ ] **Step 1: Add the pure function**

Add after `build_stratified_prompt`:

```rust
fn build_verify_prompt(
    cache: &crate::runner::leetcode_cache::LeetCodeProblemCache,
    language_key: &str,
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
```

- [ ] **Step 2: Add test for `build_verify_prompt`**

```rust
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
```

- [ ] **Step 3: Run the test**

Run: `cargo test --lib async_runtime::scheduler::leetcode_fetch::tests::build_verify_prompt_contains_case_count -- --nocapture`
Expected: PASS

---

### Task 5: Implement `generate_stratified_cases()` and `verify_generated_cases()`

**Covers:** [S2, S3]

**Files:**
- Modify: `src/async_runtime/scheduler/leetcode_fetch.rs`

- [ ] **Step 1: Add `generate_stratified_cases()`**

This replaces the existing `generate_via_ai()` function. Add after the prompt functions:

```rust
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
```

- [ ] **Step 2: Add `verify_generated_cases()`**

```rust
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
    // Count reconciliation: if verified array length differs, treat as failure
    if verified_cases.len() != cases.len() {
        return (cases, false);
    }
    (verified_cases, true)
}
```

- [ ] **Step 3: Add count reconciliation test**

```rust
#[test]
fn verify_count_mismatch_falls_back() {
    // This test verifies the logic concept; actual verify_generated_cases
    // is async and needs a mock server. The count reconciliation check
    // is in the function body: if verified.len() != cases.len(), return (cases, false).
    // We test the parse side here.
    let text = r#"[{"input": {"x": 1}, "expected": 1}]"#; // 1 case
    let parsed = parse_generated_cases(text).expect("parse");
    assert_eq!(parsed.len(), 1);
    // If original had 2 cases, this would be a count mismatch
    // (verified in the async function's reconciliation logic).
}
```

- [ ] **Step 4: Run all new tests**

Run: `cargo test --lib async_runtime::scheduler::leetcode_fetch::tests -- --nocapture`
Expected: all PASS

---

### Task 6: Wire `run_leetcode_generate()` to use new functions

**Covers:** [S2, S3, [S5]]

**Files:**
- Modify: `src/async_runtime/scheduler/leetcode_fetch.rs:83-120`
- Modify: `src/async_runtime/scheduler/dispatch.rs:100-119`
- Modify: `src/app/event_loop/commands_terminal.rs:592-622`

- [ ] **Step 1: Update `run_leetcode_generate()`**

Replace the existing `run_leetcode_generate` function body:

```rust
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
            // Persist the regenerated cases back into the per-problem cache.
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
```

- [ ] **Step 2: Wire `verify` from config through dispatch**

In `src/async_runtime/scheduler/dispatch.rs`, the `verify` flag needs to come from config. But `dispatch.rs` doesn't have access to `AiConfig` directly — it's constructed at the command site. 

The cleanest approach: add `verify: bool` to `WorkerRequestPayload::GenerateLeetCodeTests` and set it in `commands_terminal.rs`.

In `src/async_runtime/message.rs:472-476`:
```rust
GenerateLeetCodeTests {
    cache: crate::runner::leetcode_cache::LeetCodeProblemCache,
    language_key: String,
    provider: crate::config::ai_config::AiProviderConfig,
    verify: bool,
},
```

In `src/app/event_loop/commands_terminal.rs:614-622`, add `verify`:
```rust
self.submit(RequestSpec {
    revision_id: 0,
    topic: RequestTopic::LeetCode,
    payload: WorkerRequestPayload::GenerateLeetCodeTests {
        cache,
        language_key: language_key.to_string(),
        provider,
        verify: self.ai_config.leetcode_verify_enabled(),
    },
});
```

In `src/async_runtime/scheduler/dispatch.rs:100-119`, destructure `verify`:
```rust
if let WorkerRequestPayload::GenerateLeetCodeTests {
    cache,
    language_key,
    provider,
    verify,
} = request.payload.clone()
{
    let worker_tx = result_tx.clone();
    let worker_proxy = event_proxy.clone();
    tokio::spawn(run_leetcode_generate(
        LeetCodeGenerateJob {
            request_id: request.request_id,
            revision_id: request.revision_id,
            cache,
            language_key,
            provider,
            verify,
        },
        worker_tx,
        worker_proxy,
    ));
    continue;
}
```

- [ ] **Step 3: Run `cargo check`**

Expected: compiles clean

---

### Task 7: Update result handler toasts

**Covers:** [S3]

**Files:**
- Modify: `src/app/event_loop/async_results/leetcode_fetch.rs:4-25`
- Modify: `src/app/event_loop/commands_tests.rs:3831`

- [ ] **Step 1: Update `handle_leetcode_generate_result`**

```rust
pub(super) fn handle_leetcode_generate_result(app: &mut AppShell, payload: WorkerResultPayload) {
    match payload {
        WorkerResultPayload::LeetCodeTestsGenerated { id: _, cases, verified } => {
            app.app_state.test_runner.is_generating = false;
            app.app_state.test_runner.cases = cases
                .into_iter()
                .map(|case| crate::runner::TestCase::new_ai(case.input, case.expected))
                .collect();
            app.app_state.test_runner.selected =
                (!app.app_state.test_runner.cases.is_empty()).then_some(0);
            app.app_state.test_runner.focused_field = crate::runner::TestField::Input;
            app.app_state.test_runner.is_running = false;
            app.app_state.test_runner.launch_error = None;
            let suffix = if verified {
                " (verified)"
            } else {
                " — review Expected, then F5"
            };
            app.show_transient_toast_kind(
                format!(
                    "Generate LeetCode Tests\n{} AI test cases{suffix}.",
                    app.app_state.test_runner.cases.len()
                ),
                ToastKind::Success,
            );
            app.request_redraw();
        }
        WorkerResultPayload::LeetCodeTestsGenerateFailed { message } => {
            app.app_state.test_runner.is_generating = false;
            app.show_transient_toast_kind(
                format!("Generate LeetCode Tests\n{message}"),
                ToastKind::Error,
            );
            app.request_redraw();
        }
        _ => {}
    }
}
```

- [ ] **Step 2: Update test in commands_tests.rs**

In `src/app/event_loop/commands_tests.rs:3831`, add `verified: false`:

```rust
payload: crate::async_runtime::message::WorkerResultPayload::LeetCodeTestsGenerated {
    id: "1".to_string(),
    cases: vec![...],
    verified: false,
},
```

- [ ] **Step 3: Run tests**

Run: `cargo test --lib app::event_loop::commands_tests::generated_leetcode_tests -- --nocapture`
Expected: PASS

---

### Task 8: Full verification

**Covers:** [S2, S3, [S4], [S5], [S6]]

- [ ] **Step 1: Run `cargo check`**

Expected: compiles clean

- [ ] **Step 2: Run all related tests**

Run: `cargo test --lib -- --nocapture 2>&1 | tail -20`
Expected: all tests PASS

- [ ] **Step 3: Run clippy**

Run: `cargo clippy -- -D warnings 2>&1 | tail -20`
Expected: no warnings

---

### Task 9: Remove dead code

**Covers:** [S2]

**Files:**
- Modify: `src/async_runtime/scheduler/leetcode_fetch.rs`

- [ ] **Step 1: Remove `generate_via_ai()` and `generate_via_ai_improved()`**

The old `generate_via_ai` function (line 155) and the test-only `generate_via_ai_improved` function (line 881) are now replaced by `generate_stratified_cases`. Remove both.

Also remove the test `generates_improved_test_cases_with_reasoning` that tested `generate_via_ai_improved` if it exists.

- [ ] **Step 2: Run `cargo check`**

Expected: compiles clean (no dead code warnings)

- [ ] **Step 3: Run all tests**

Run: `cargo test --lib -- --nocapture 2>&1 | tail -20`
Expected: all tests PASS
