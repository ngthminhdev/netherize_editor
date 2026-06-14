# Fetch LeetCode Problem Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Fetch a LeetCode problem into a runnable workspace solution file with populated Test Runner cases.

**Architecture:** Extend the existing command-palette and async-worker patterns. Keep LeetCode parsing/adaptation pure and unit-testable; keep all HTTP and AI calls off the UI thread; apply files and UI state only in the result handler.

**Tech Stack:** Rust, tokio, reqwest, serde/serde_json, regex, existing command palette and Test Runner.

---

### Task 1: LeetCode API and Example Parsing

**Files:**
- Create: `src/runner/leetcode_api.rs`
- Modify: `src/runner/mod.rs`

- [ ] Write failing tests for ID/slug/URL normalization, metadata parameter parsing, and HTML examples.
- [ ] Run the focused runner tests and confirm expected failures.
- [ ] Implement DTOs, slug resolution helpers, and example extraction.
- [ ] Run focused tests until green.

### Task 2: Mechanical and AI Adaptation

**Files:**
- Create: `src/runner/leetcode_adapter.rs`
- Modify: `src/runner/mod.rs`

- [ ] Write failing tests for supported-language snippet selection and runnable `solve(data)` wrappers.
- [ ] Implement mechanical templates and AI prompt/response parsing.
- [ ] Run focused adapter and existing runner tests.

### Task 3: Configuration and Settings Toggle

**Files:**
- Modify: `src/config/ai_config.rs`
- Modify: `config/ai.toml`
- Modify: `src/app/app_state/settings.rs`
- Modify: `src/app/event_loop/commands_settings_helpers.rs`
- Modify: `src/render/renderer/editor/settings.rs`

- [ ] Write failing config/settings tests for default-off and toggle behavior.
- [ ] Add `LeetCodeConfig`, provider config, and persisted setter.
- [ ] Add and render `SettingItem::LeetCodeAi`.
- [ ] Run focused config/settings tests.

### Task 4: Command and Palette Workflow

**Files:**
- Modify: `src/core/commands.rs`
- Modify: `src/core/command_ids.rs`
- Modify: `src/app/command_palette.rs`
- Modify: `src/app/app_state/palette.rs`
- Modify: `src/core/command_dispatch/mod.rs`
- Modify: `src/core/command_dispatch/palette.rs`
- Modify: `src/app/event_loop/commands_palette.rs`
- Modify: `src/app/event_loop/commands_terminal.rs`

- [ ] Write failing tests for command registration, input palette, active-language detection, and language-picker fallback.
- [ ] Add command variants, IDs, palette modes/actions, and handlers.
- [ ] Reuse MRU ordering and preserve the pending problem input through language selection.
- [ ] Run command, palette, and event-loop tests.

### Task 5: Async Fetch Worker

**Files:**
- Create: `src/async_runtime/scheduler/leetcode_fetch.rs`
- Modify: `src/async_runtime/message.rs`
- Modify: `src/async_runtime/scheduler/mod.rs`
- Modify: `src/async_runtime/scheduler/dispatch.rs`

- [ ] Write failing tests for request/result payload construction and worker fallback behavior where practical.
- [ ] Add typed messages and the spawned fetch coordinator.
- [ ] Use `emit_message_and_wake` for completion and failure delivery.
- [ ] Run scheduler tests.

### Task 6: Result Application

**Files:**
- Create: `src/app/event_loop/async_results/leetcode_fetch.rs`
- Modify: `src/app/event_loop/async_results/mod.rs`
- Modify: `src/app/event_loop/commands_tests.rs`

- [ ] Write a failing result-handler test proving file creation, buffer opening, and Test Runner replacement.
- [ ] Implement unique workspace-root file creation and normal open-file setup.
- [ ] Populate cases and focus the Test Runner.
- [ ] Run focused event-loop tests.

### Task 7: Verification

- [ ] Run all focused LeetCode, runner, config, command-dispatch, and event-loop tests.
- [ ] Run `cargo check` and relevant broader tests.
- [ ] Run `cargo fmt --all -- --check` and `git diff --check`.
- [ ] Run GitNexus `detect_changes` and verify the affected flows are expected.
