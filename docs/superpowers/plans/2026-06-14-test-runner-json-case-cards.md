# Test Runner JSON Case Cards Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the compact raw-stdin Test Runner UI with JSON-first case cards, a focused multiline mini-editor, mouse interaction, and a JSON-aware JavaScript scaffold.

**Architecture:** Keep test data and editing state in `TestRunnerState`, render it on the existing right-dock surface, and route pointer actions through Commands before mutation. The worker continues to receive raw stdin; JSON validation and semantic output comparison stay in the pure runner core.

**Tech Stack:** Rust, winit, wgpu, cosmic-text, serde_json, tokio worker requests

---

### Task 1: JSON Runner Semantics

**Files:**
- Modify: `src/runner/mod.rs`
- Modify: `src/runner/leetcode.rs`

- [ ] Add failing tests for JSON value comparison, invalid JSON diagnostics, and the JavaScript scaffold using `JSON.parse`/`JSON.stringify`.
- [ ] Run targeted tests and confirm failures reflect missing JSON semantics.
- [ ] Implement JSON validation/comparison and update the JavaScript scaffold.
- [ ] Run targeted tests and confirm they pass.

### Task 2: Mini-Editor State

**Files:**
- Modify: `src/runner/mod.rs`
- Modify: `src/core/commands.rs`
- Modify: `src/core/command_dispatch/mod.rs`
- Modify: `src/app/event_loop/commands_terminal.rs`
- Modify: `src/app/input/handler.rs`
- Test: `src/app/input/tests.rs`

- [ ] Add failing tests for 2D cursor movement, Home/End, Delete, local undo/redo, field-open state, and invalid-run field selection.
- [ ] Add commands for mouse selection/opening, editor movement, delete, undo/redo, and scrolling.
- [ ] Implement state transitions in the command mutation path.
- [ ] Extend input routing tests and make the targeted suite pass.

### Task 3: Case Cards And Focused Editor Rendering

**Files:**
- Modify: `src/render/renderer/ui/test_runner.rs`
- Modify: `src/app/event_loop/application.rs`

- [ ] Add pure geometry and hit-test tests for Run, Add Case, case selection, and Input/Expected fields.
- [ ] Render active file/runtime header, mode badge, multiline cards, actual/error output, footer, and focused editor overlay.
- [ ] Expose hit-test actions and dispatch them from mouse input through Commands.
- [ ] Add mouse-wheel scrolling and keep-selection-visible behavior.

### Task 4: Verification

**Files:**
- Verify all modified Rust and documentation files.

- [ ] Run Test Runner, input-routing, command, and scaffold tests.
- [ ] Run `cargo test --lib` and `cargo check`.
- [ ] Run rustfmt check and `git diff --check`.
- [ ] Run GitNexus change detection and review the affected flows.
