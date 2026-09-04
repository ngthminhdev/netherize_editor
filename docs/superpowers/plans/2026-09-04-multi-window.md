# Multi-window Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** One process, one dock icon, N windows; `netherize repoB` opens a new window unless a window already hosts it.

**Architecture:** Keep `AppShell` whole (it already owns window, renderer, runtime, sessions). Turn its `ApplicationHandler` impl into inherent `on_*` methods; a new `MultiWindowApp` implements the trait, holds `Vec<AppShell>`, the single `AppPersistentState` (swapped into the shell being dispatched), routes remote opens, spawns and reaps windows.

**Tech Stack:** Rust 2024, winit 0.30, wgpu, tokio. `cargo test --lib`.

**Spec:** `docs/superpowers/specs/2026-09-04-multi-window-design.md`

## Global Constraints

- Never `git commit`; report instead.
- `gitnexus_impact` LOW for the touched symbols (`resumed`, `window_event`, `about_to_wait`, `user_event`, `run`, `new_with_scheduler`); re-run for anything else before editing.
- All existing tests keep passing; `AppShell::new_for_tests()` keeps its signature.

---

### Task A: Worker shutdown primitives

**Files:** `src/async_runtime/scheduler/runtime.rs`, `src/async_runtime/scheduler/file_watch.rs`

- [ ] `AsyncScheduler::shutdown(self, timeout: Duration)` → `self._runtime.shutdown_timeout(timeout)`.
- [ ] `impl Drop for FileWatchRegistry` sets every flag (test: drop registry, flag is true).

### Task B: `AppShell` becomes window-agnostic

**Files:** `src/app/event_loop/application.rs:2078-2792`, `src/app/event_loop/setup.rs:8-60`, `src/app/event_loop/mod.rs` (fields), `src/app/event_loop/window_lifecycle.rs` (new), `src/app/event_loop/commands.rs:352`, `src/core/commands.rs`, `src/core/command_ids.rs`, `src/core/command_dispatch/mod.rs`, `src/app/command_palette.rs`

- [ ] `impl ApplicationHandler<AppEvent> for AppShell` → `impl AppShell` with `on_resumed(&mut self, &ActiveEventLoop) -> Result<(), String>`, `on_window_event(&mut self, WindowId, WindowEvent)`, `on_about_to_wait(&mut self) -> Option<Instant>`, `on_user_event(&mut self, AppEvent)`. No `event_loop.exit()` / `set_control_flow` left in the shell.
- [ ] `AppShell::new(proxy, cli_args, persistent_state)`; `new_with_scheduler(scheduler, rx, cli_args, persistent_state)`; `new_for_tests()` passes env args + `AppPersistentState::load()`.
- [ ] Fields: `pending_new_windows: Vec<Vec<PathBuf>>`, `window_cascade: u32`, `closing_since: Option<Instant>`.
- [ ] `window_lifecycle.rs`: `window_id()`, `hosts_root()`, `request_new_window()`, `take_pending_new_windows()`, `begin_teardown(forget_sessions)`, `teardown_due()`, `finish_teardown(self)`, `live_roots()`.
- [ ] `Command::NewWindow` (`app.new_window`, palette "New Window"); Cmd+Shift+N hard-wired chord → `NewWindow`; `NewInstance` stays.
- [ ] Tests: `new_window_command_queues_active_root_and_persists`, `hosts_root_covers_active_and_parked`, `teardown_due_after_grace`.

### Task C: `MultiWindowApp`

**Files:** `src/app/event_loop/multi_window.rs` (new), `src/app/event_loop/mod.rs` (`run()`)

- [ ] `MultiWindowApp { shells, proxy, persistent, focused, cascade, stale_instance_running }` implementing `ApplicationHandler<AppEvent>`; `with_shell` swap helper; `spawn_window`; `reap_and_spawn`; pure `route_remote_open` + `min_deadline`.
- [ ] `run()` loads state + CLI once and runs `MultiWindowApp`.
- [ ] Tests: `route_remote_open` (hosted / unhosted / files-only / no focus), `min_deadline`.

### Task D: Verify

- [ ] `cargo test --lib` green; raw clippy adds no warnings in touched files.
- [ ] `scripts/bundle_macos.sh`; install; GUI checklist: `netherize repoA` then `netherize repoB` → two windows, one dock icon; `netherize repoA` again → focuses the first; Cmd+Shift+N → new window on the same repo with its tabs; close a window with a dirty buffer → prompt; close all → app quits; Activity Monitor: LSP servers of a closed window are gone within seconds.
