# LeetCode Test Runner — AI Generate, Per-File Cache, UX Rework

Date: 2026-06-14
Status: Approved (chat), implementing

## Goal

Improve the LeetCode test-runner UX and add an AI-powered test-case generator.

## Scope

### 1. Solution file header (metadata)
At fetch, prepend a language-appropriate comment header to the generated file:

```
// netherize-leetcode id=1 slug=two-sum
// Two Sum — https://leetcode.com/problems/two-sum/
```

- Comment prefix: `#` for Python/Ruby, `//` for JS/TS/Go/Rust.
- First line is machine-readable: `netherize-leetcode id=<id> slug=<slug>`.
- A parser extracts `(id, slug)` from the first matching comment line near the top.

### 2. Per-problem test-case cache (persist + auto-reload)
- JSON cache under the user cache dir, **keyed by problem id**.
- Stores: `id, slug, title, statement, parameters, cases[] (input, expected)`.
- Written at fetch and whenever cases change (add / edit / delete / generate).
- On opening/activating a solution file whose header carries a leetcode id, the
  test runner loads the cached context + cases into the panel.

### 3. `g` — AI generate (async, non-blocking)
- Reads id/slug from the active file header → loads problem context from cache
  (re-fetch by slug if the cached statement is missing).
- Sends context to the `[leetcode.provider]` AI → asks for exactly **5** test
  cases as input+expected JSON.
- On result: **replace all existing cases with the 5 new ones**, mark them as
  AI-generated (distinct tone in the panel), update the cache.
- Runs in `tokio::spawn` + `mpsc` (never blocks the UI). While in flight the
  panel shows a `Generating…` spinner state.
- If AI is disabled/unconfigured, or the file has no leetcode header → toast hint
  instead of generating.

### 4. Test-runner keybinding rework
- Remove `i` and `Enter` field-edit bindings → **field editing is click-only**
  (the mouse hit-test pipeline already supports it).
- Keep `a` (add), `x` (delete), `F5` (run), `j/k/↑/↓` (nav), `h/l/Tab` (column).
- Add `g` → generate.
- Update the panel help/NAV chip to match.

### 5. Paste + hint fix (independent bug)
- Add `LeetCodeProblemInput` (and `LeetCodeLanguageSelector`) to the paste
  allow-list at `src/core/command_dispatch/editing.rs:575` so Cmd+V pastes into
  the palette query instead of the editor buffer.
- The empty-state hint already renders; verify it shows.

## Decisions
- Cache key = problem id (same problem shares cache across files).
- AI-generated `expected` may be wrong → flagged "AI" in the UI for review.
- `g` on a non-leetcode file → toast, no-op.

## Key files
- `src/runner/leetcode.rs`, `src/runner/leetcode_adapter.rs` — header generation.
- `src/runner/leetcode_cache.rs` (new) — per-id cache load/save + header parse.
- `src/async_runtime/scheduler/leetcode_fetch.rs` — AI generate worker.
- `src/async_runtime/message.rs` — generate request/result payloads.
- `src/app/event_loop/async_results/leetcode_fetch.rs` — generate result handler.
- `src/app/input/handler.rs` — keybind rework (`route_test_runner_input`).
- `src/core/commands.rs`, `src/core/command_ids.rs` — `TestRunnerGenerateCases`.
- `src/render/renderer/ui/test_runner.rs` — spinner + AI tone + help chip.
- `src/core/command_dispatch/editing.rs` — paste allow-list.

## Architecture rules respected
- No blocking on UI thread: AI generate runs via `tokio::spawn` + `mpsc`.
- AI fallback / graceful errors: no panics; toast on failure.
