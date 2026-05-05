---
name: scheduler
description: "Skill for the Scheduler area of netherize_editor. 60 symbols across 15 files."
---

# Scheduler

60 symbols | 15 files | Cohesion: 80%

## When to Use

- Working with code in `src/`
- Understanding how run_pty_request, run_lsp_request, run_fzf_request work
- Modifying scheduler-related functionality

## Key Files

| File | Symbols |
|------|---------|
| `src/async_runtime/scheduler/fzf.rs` | run_fzf_request, build_fzf_find_file_script, build_fzf_live_grep_script, build_ripgrep_ignore_glob_args, execute_fzf_async (+5) |
| `src/async_runtime/scheduler/lsp_parse.rs` | parse_hover_content, parse_completion_items, handle_lsp_hover, handle_lsp_completion, handle_lsp_document_symbols (+4) |
| `src/async_runtime/scheduler/tests.rs` | extend_unique_file_events_deduplicates_burst_entries, normalize_create_event_maps_to_internal_create, normalize_rename_event_maps_old_and_new_paths, normalize_single_path_rename_still_maps_to_rename, fzf_find_file_script_uses_ripgrep_files_and_ignore_globs (+2) |
| `src/async_runtime/scheduler/ai_jobs.rs` | strip_ansi_sequences, should_skip_opencode_line, sanitize_opencode_line, build_prompt_with_file_context, resolve_opencode_binary (+2) |
| `src/async_runtime/scheduler/file_watch.rs` | run_file_watch_request, execute_file_watch_loop, extend_unique_file_events, filter_file_watch_events, normalize_notify_event (+1) |
| `src/async_runtime/scheduler/local_history.rs` | local_history_path_for_file, run_local_history_request, emit_local_history_failure, execute_load_local_history, execute_save_local_history |
| `src/async_runtime/scheduler/emit.rs` | emit_message, emit_message_and_wake, failure_from_join_error, panic_payload_to_string |
| `src/async_runtime/scheduler/runtime.rs` | new, new_for_tests, build_worker_runtime |
| `src/workspace/model.rs` | should_ignore_path, ignored_directory_names |
| `src/async_runtime/scheduler/git.rs` | format_relative_unix_time, format_relative_duration |

## Entry Points

Start here when exploring this area:

- **`run_pty_request`** (Function) — `src/async_runtime/scheduler/pty.rs:22`
- **`run_lsp_request`** (Function) — `src/async_runtime/scheduler/lsp.rs:29`
- **`run_fzf_request`** (Function) — `src/async_runtime/scheduler/fzf.rs:18`
- **`run_file_watch_request`** (Function) — `src/async_runtime/scheduler/file_watch.rs:18`
- **`extend_unique_file_events`** (Function) — `src/async_runtime/scheduler/file_watch.rs:196`

## Key Symbols

| Symbol | Type | File | Line |
|--------|------|------|------|
| `run_pty_request` | Function | `src/async_runtime/scheduler/pty.rs` | 22 |
| `run_lsp_request` | Function | `src/async_runtime/scheduler/lsp.rs` | 29 |
| `run_fzf_request` | Function | `src/async_runtime/scheduler/fzf.rs` | 18 |
| `run_file_watch_request` | Function | `src/async_runtime/scheduler/file_watch.rs` | 18 |
| `extend_unique_file_events` | Function | `src/async_runtime/scheduler/file_watch.rs` | 196 |
| `emit_message` | Function | `src/async_runtime/scheduler/emit.rs` | 10 |
| `emit_message_and_wake` | Function | `src/async_runtime/scheduler/emit.rs` | 16 |
| `failure_from_join_error` | Function | `src/async_runtime/scheduler/emit.rs` | 25 |
| `panic_payload_to_string` | Function | `src/async_runtime/scheduler/emit.rs` | 41 |
| `dispatch_loop` | Function | `src/async_runtime/scheduler/dispatch.rs` | 30 |
| `resolve_opencode_binary` | Function | `src/async_runtime/scheduler/ai_jobs.rs` | 111 |
| `run_ai_chat_stream` | Function | `src/async_runtime/scheduler/ai_jobs.rs` | 135 |
| `run_opencode_install` | Function | `src/async_runtime/scheduler/ai_jobs.rs` | 301 |
| `should_ignore_path` | Function | `src/workspace/model.rs` | 75 |
| `normalize_notify_event` | Function | `src/async_runtime/scheduler/file_watch.rs` | 207 |
| `ignored_directory_names` | Function | `src/workspace/model.rs` | 84 |
| `build_fzf_find_file_script` | Function | `src/async_runtime/scheduler/fzf.rs` | 177 |
| `build_fzf_live_grep_script` | Function | `src/async_runtime/scheduler/fzf.rs` | 186 |
| `handle_lsp_hover` | Function | `src/async_runtime/scheduler/lsp_parse.rs` | 192 |
| `handle_lsp_completion` | Function | `src/async_runtime/scheduler/lsp_parse.rs` | 436 |

## Execution Flows

| Flow | Type | Steps |
|------|------|-------|
| `Run_pty_request → FlatRegion` | cross_community | 7 |
| `Run_lsp_request → All_language_profiles` | cross_community | 6 |
| `Run_lsp_request → Find_node` | cross_community | 6 |
| `Run_fzf_request → Ignored_directory_names` | cross_community | 6 |
| `Run_pty_request → Parse_go_version` | cross_community | 5 |
| `Run_lsp_request → Extract_path_from_login_shell` | cross_community | 5 |
| `Run_lsp_request → Is_header_separator` | cross_community | 5 |
| `Run_lsp_request → Parse_header_line` | cross_community | 5 |
| `Run_lsp_request → Parse_content_length` | cross_community | 5 |
| `Run_local_history_request → Home_dir` | cross_community | 5 |

## Connected Areas

| Area | Connections |
|------|-------------|
| Lsp | 5 calls |
| Terminal | 1 calls |
| Config | 1 calls |
| Workspace | 1 calls |
| Syntax | 1 calls |

## How to Explore

1. `gitnexus_context({name: "run_pty_request"})` — see callers and callees
2. `gitnexus_query({query: "scheduler"})` — find related execution flows
3. Read key files listed above for implementation details
