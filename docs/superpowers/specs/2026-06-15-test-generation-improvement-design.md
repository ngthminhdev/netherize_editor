# Test Case Generation Improvement — Prompt, Verification, Stratification

Date: 2026-06-15
Status: Approved (chat), ready for implementation

## [S1] Problem

The current AI test case generator (`generate_via_ai` at `src/async_runtime/scheduler/leetcode_fetch.rs:155`) has two issues:

1. **High error rate** — Generated test cases often have incorrect expected outputs because the prompt is too vague ("5 diverse test cases, include edge cases") and doesn't enforce step-by-step reasoning or constraint verification.
2. **No hard cases** — The AI generates generic edge cases but misses the adversarial/hidden-test-style cases that LeetCode uses to catch subtle bugs (off-by-one, boundary violations, algorithmic pitfalls).

An improved prompt already exists in tests (`generate_via_ai_improved` at line 881) but is not used in production.

## [S2] Solution: Stratified Generation

Replace the generic "5 diverse test cases" prompt with a **stratified prompt** that explicitly assigns a category to each of the 5 test cases:

| # | Category | Purpose |
|---|----------|---------|
| 1 | **Basic/Example** | Simplest case, mirrors provided examples |
| 2 | **Constraint boundary** | Min/max of constraints (length=1, length=maxLength, value bounds) |
| 3 | **Common bug catcher** | Input that breaks wrong algorithms (off-by-one, empty input, negative values if allowed) |
| 4 | **Algorithmic stress** | Larger input or special structure (sorted reverse, all duplicates, single element) |
| 5 | **Adversarial/Hard** | LeetCode hidden-test style — designed to expose subtle implementation bugs |

The prompt must also require:
- Step-by-step algorithm trace for each case to calculate the correct expected output
- Explicit constraint verification (array lengths, value ranges) before finalizing
- Explanation of what edge case each test is targeting

Base the improved prompt on the existing `generate_via_ai_improved` (line 881) and integrate the stratified categories.

**Fixed count of 5.** The stratification is built around exactly these 5 named categories, so the generated count is fixed at 5 — there is no `generate_count` knob (see [S4]). If a future change needs more cases, categories 1–5 stay fixed and extras are appended as additional "Adversarial/Hard" cases; until then, do not expose a configurable count that has no defined category mapping.

**Prompt building must be a pure function.** Extract the prompt construction into `fn build_stratified_prompt(cache, language_key) -> String` (and `fn build_verify_prompt(cache, language_key, cases) -> String`) so the wording is unit-testable without an HTTP call. The async generate/verify functions call these helpers. This is what makes "assert the prompt contains all 5 category labels" testable (see [S5]).

## [S3] Verification Step

After generating the 5 cases, run a second AI call to **verify** the expected outputs:

**Verify prompt:** Send all 5 generated cases + problem context, ask the AI to:
- Re-trace each case using the optimal algorithm
- Check if the expected output is correct
- Check if the input satisfies all problem constraints
- Return a corrected JSON array (keep if correct, fix if wrong)

**Implementation:**
- New function: `verify_generated_cases(provider, cache, language_key, cases)`
- Flow: `generate_stratified_cases()` → `verify_generated_cases()` → return `(cases, verified: bool)`
- If verification fails to parse, use the original generate result and set `verified = false`
- **Count reconciliation:** if the verified array fails to parse OR its length differs from the input length, treat it as a verify-failure and fall back to the unverified set (`verified = false`). This prevents a "helpful" model that merges/drops cases from silently shrinking the user's test set.
- Timeout: 60s for verify step. Verify reuses the same reasoning-effort handling as generate (`reasoning_effort` from the provider config), since a reasoning model can exhaust its token budget on the verify call exactly as it can on generate (see existing handling at `leetcode_fetch.rs:197-205, 249-260`).

**UX toasts — requires a status field and (optionally) a progress message:**
- The "Done"/"Verify fail" distinction needs a verification status threaded through to the UI. `WorkerResultPayload::LeetCodeTestsGenerated` currently carries only `{ id, cases }` and `handle_leetcode_generate_result` (`src/app/event_loop/async_results/leetcode_fetch.rs:4`) hardcodes its success toast — so a `verified: bool` field must be **added to that payload** and the handler must branch on it.
- The mid-flight "Verifying test cases…" toast has **no delivery channel today**: `run_leetcode_generate` emits exactly one terminal `WorkerMessage::Result`. Showing it requires a new intermediate progress message variant + handler. If that is out of scope, **drop the "Verifying…" toast** rather than leaving it unimplementable.
- Start: "Generating 5 test cases..." (shown synchronously on the UI thread before dispatch — already feasible)
- After generate: "Verifying test cases..." (**requires** new progress message — see above; cut if not building it)
- Done: "Generated 5 test cases (verified)" (`verified == true`)
- Verify fail: "Generated 5 test cases (verification failed, review recommended)" (`verified == false`)

**Trade-off:** ~2x latency (extra API call), but significantly higher accuracy. With local providers the worst case is generate(90s) + verify(60s) = ~150s, and verify doubles the surface for reasoning-token-exhaustion failures — see [S4] for the default.

## [S4] Config

Add to `[leetcode]` section:
- `verify = true` — enable/disable verification step. **Default `false`** (opt-in): verify roughly doubles latency and failure surface, which is a real cost for the local-model setups this adapter targets. A user who wants the accuracy turns it on explicitly.

`generate_count` is intentionally **not** added — the count is fixed at 5 by the stratified design (see [S2]).

Plumbing note: `LeetCodeGenerateJob` (`leetcode_fetch.rs:75`) currently carries only `provider`. The `verify` flag must be read at the command site that builds the job and **added as a new field on `LeetCodeGenerateJob`**, since `run_leetcode_generate` has no access to `AiConfig`.

## [S5] Files to Change

| File | Change |
|------|--------|
| `src/async_runtime/scheduler/leetcode_fetch.rs` | Promote `generate_via_ai_improved()` to production as `generate_stratified_cases()`; add `verify_generated_cases()`; extract `build_stratified_prompt()` / `build_verify_prompt()` as pure helpers; update `run_leetcode_generate` to run generate→verify and pass `verified` through |
| `src/async_runtime/scheduler/leetcode_fetch.rs` (`LeetCodeGenerateJob`) | Add `verify: bool` field; populate it at the job-construction site from `AiConfig` |
| `src/config/ai_config.rs` | Add `verify: Option<bool>` to `LeetCodeConfig` + an accessor (e.g. `leetcode_verify_enabled()` defaulting to `false`) |
| `src/async_runtime/message.rs` | **Required:** add `verified: bool` to `WorkerResultPayload::LeetCodeTestsGenerated`; add a progress message variant only if the "Verifying…" toast is in scope |
| `src/app/event_loop/async_results/leetcode_fetch.rs` | Branch the toast in `handle_leetcode_generate_result` on `verified`; handle the progress message if added |
| Tests in `leetcode_fetch.rs` | Add tests for `build_stratified_prompt` (asserts all 5 category labels present) and `build_verify_prompt`; add a count-reconciliation test for the verify fallback. The existing `parse_generated_cases` tests are prompt-independent and stay as-is — they do **not** need updating for the new prompt format |

## [S6] Architecture Rules

- No blocking on UI thread: both generate and verify run via `tokio::spawn` + `mpsc`
- Graceful errors: verify failure (parse error OR count mismatch) falls back to unverified results with `verified = false` and a warning toast
- No panics: all error paths return `Result` with descriptive messages
- Prompt construction lives in pure, synchronous helper functions so it is unit-testable without network I/O
