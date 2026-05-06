# System Dependency Checker — Implementation Plan  
**Project**: Netherize Editor | **Date**: 2026-05-05 | **Branch**: development  

## Overview  
Implement a popup that checks for missing CLI tools (fzf, lazygit, lazydocker, rg, fd, bat, delta) on IDE boot.  
If tools are missing, show a modal popup with 3 states: Detection → Installing → Complete.  
Uses the **LSP Install Guide pattern** (async worker → `AppShell` state → input hijack → renderer popup).  

## Architecture Decision  
**Approach A — Dedicated Async Topic**. This mirrors the existing `LspInstallGuide` pattern with its own `RequestTopic`, state on `AppShell`, and renderer fields.  

## Files to Modify (11 files)  

### Phase 1 — Async Runtime Types  
**File: `src/async_runtime/message.rs`**  
1. Add `SystemDepCheck` to `RequestTopic` enum  
2. Add `CheckSystemDeps` to `WorkerRequestPayload` enum  
3. Add `SystemDepCheckResult { missing: Vec<String> }` to `WorkerResultPayload` enum  

### Phase 2 — Worker Execution  
**File: `src/async_runtime/scheduler/syntax_jobs.rs`**  
Add match arm in `execute_virtual_job` (after `CheckLspForPath`). Runs `which` for each tool, collects missing. No dispatch changes needed — falls through to `execute_virtual_job`.  

### Phase 3 — AppShell State  
**File: `src/app/event_loop/mod.rs`**  
Add `SystemDepGuide` struct with fields: `missing_list`, `install_cmd`, `is_complete`.  
Add `active_system_dep_guide` and `dismissed_system_deps` fields to `AppShell`.  

### Phase 4 — Boot Submission  
**File: `src/app/event_loop/setup.rs`**  
In `startup_subsystems()`, submit `CheckSystemDeps` request.  

### Phase 5 — Result Handling  
**File: `src/app/event_loop/async_results.rs`**  
Add `current_revision_for` arm and `on_worker_result` handler. Shows popup when tools are missing.  

### Phase 6 — Input Hijacking  
**File: `src/app/event_loop/application.rs`**  
Intercept Enter/Escape when popup is active, before normal key routing.  

### Phase 7 — Render Dispatch  
**File: `src/app/event_loop/application.rs`**  
Render popup after LSP guide in z-order.  

### Phase 8 — Accept/Dismiss Methods  
**File: `src/app/event_loop/commands.rs`**  
`dismiss_system_dep_guide()` and `accept_system_dep_guide()` alongside LSP guide methods.  

### Phase 9-12 — Renderer  
**Files: `src/render/renderer.rs`, `lifecycle.rs`, `lifecycle/frame.rs`, `ui/popups.rs`**  
Add 5 GPU resource fields, lifecycle init, frame drawing, and popup render methods.  
