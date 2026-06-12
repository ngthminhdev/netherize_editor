---
name: scheduler
description: "Skill for the Scheduler area of netherize_editor. 83 symbols across 21 files."
---

# Scheduler

83 symbols | 21 files | Cohesion: 79%

## When to Use

- Working with code in `src/`
- Understanding how try_wait_status, language_profile_for_binary, scan_python_environments work
- Modifying scheduler-related functionality

## Key Files

| File | Symbols |
|------|---------|
| `src/async_runtime/scheduler/lsp_parse.rs` | parse_hover_content, parse_completion_items, parse_text_edits, handle_lsp_hover, parse_hover_doc_blocks (+14) |
| `src/async_runtime/scheduler/fzf.rs` | run_fzf_request, execute_fzf_async, fzf_live_grep, build_fzf_live_grep_script, build_ripgrep_ignore_glob_args (+5) |
| `src/editor_core.rs` | from_str, move_right, move_to_last_line, move_right_allows_eof_on_last_line_without_newline, move_right_crosses_to_next_line (+4) |
| `src/async_runtime/scheduler/tests.rs` | extend_unique_file_events_deduplicates_burst_entries, normalize_create_event_maps_to_internal_create, normalize_rename_event_maps_old_and_new_paths, normalize_single_path_rename_still_maps_to_rename, fzf_live_grep_script_uses_ripgrep_and_ignore_globs (+2) |
| `src/async_runtime/scheduler/ai_jobs.rs` | strip_ansi_sequences, should_skip_opencode_line, sanitize_opencode_line, build_prompt_with_file_context, resolve_opencode_binary (+2) |
| `src/async_runtime/scheduler/file_watch.rs` | run_file_watch_request, execute_file_watch_loop, extend_unique_file_events, filter_file_watch_events, normalize_notify_event (+1) |
| `src/async_runtime/scheduler/emit.rs` | emit_message, emit_message_and_wake, failure_from_join_error, panic_payload_to_string |
| `src/async_runtime/scheduler/dispatch.rs` | detect_python_version, detect_command_version, dispatch_loop |
| `src/async_runtime/scheduler/runtime.rs` | new, new_for_tests, build_worker_runtime |
| `src/async_runtime/scheduler/syntax_jobs.rs` | resolve_system_path, run_system_dep_install |

## Entry Points

Start here when exploring this area:

- **`try_wait_status`** (Function) — `src/terminal/pty.rs:134`
- **`language_profile_for_binary`** (Function) — `src/lsp/registry.rs:264`
- **`scan_python_environments`** (Function) — `src/async_runtime/python_env.rs:18`
- **`resolve_system_path`** (Function) — `src/async_runtime/scheduler/syntax_jobs.rs:417`
- **`run_system_dep_install`** (Function) — `src/async_runtime/scheduler/syntax_jobs.rs:451`

## Key Symbols

| Symbol | Type | File | Line |
|--------|------|------|------|
| `try_wait_status` | Function | `src/terminal/pty.rs` | 134 |
| `language_profile_for_binary` | Function | `src/lsp/registry.rs` | 264 |
| `scan_python_environments` | Function | `src/async_runtime/python_env.rs` | 18 |
| `resolve_system_path` | Function | `src/async_runtime/scheduler/syntax_jobs.rs` | 417 |
| `run_system_dep_install` | Function | `src/async_runtime/scheduler/syntax_jobs.rs` | 451 |
| `run_lsp_request` | Function | `src/async_runtime/scheduler/lsp.rs` | 30 |
| `run_fzf_request` | Function | `src/async_runtime/scheduler/fzf.rs` | 18 |
| `run_file_watch_request` | Function | `src/async_runtime/scheduler/file_watch.rs` | 18 |
| `extend_unique_file_events` | Function | `src/async_runtime/scheduler/file_watch.rs` | 196 |
| `emit_message` | Function | `src/async_runtime/scheduler/emit.rs` | 10 |
| `emit_message_and_wake` | Function | `src/async_runtime/scheduler/emit.rs` | 16 |
| `failure_from_join_error` | Function | `src/async_runtime/scheduler/emit.rs` | 25 |
| `panic_payload_to_string` | Function | `src/async_runtime/scheduler/emit.rs` | 41 |
| `dispatch_loop` | Function | `src/async_runtime/scheduler/dispatch.rs` | 59 |
| `resolve_opencode_binary` | Function | `src/async_runtime/scheduler/ai_jobs.rs` | 111 |
| `run_ai_chat_stream` | Function | `src/async_runtime/scheduler/ai_jobs.rs` | 135 |
| `run_opencode_install` | Function | `src/async_runtime/scheduler/ai_jobs.rs` | 301 |
| `as_str` | Function | `src/syntax/highlight.rs` | 55 |
| `handle_lsp_hover` | Function | `src/async_runtime/scheduler/lsp_parse.rs` | 240 |
| `handle_lsp_formatting` | Function | `src/async_runtime/scheduler/lsp_parse.rs` | 489 |

## Execution Flows

| Flow | Type | Steps |
|------|------|-------|
| `Update_markdown_preview_content → From_str` | cross_community | 6 |
| `Run_lsp_request → All_language_profiles` | cross_community | 6 |
| `Run_lsp_request → Find_node` | cross_community | 6 |
| `Run_lsp_request → Login_shell_path_cache` | cross_community | 6 |
| `Run_fzf_request → Ignored_directory_names` | cross_community | 6 |
| `Run_lsp_request → Is_header_separator` | cross_community | 5 |
| `Run_lsp_request → Parse_header_line` | cross_community | 5 |
| `Run_lsp_request → Parse_content_length` | cross_community | 5 |
| `Run_fzf_request → Is_empty_fzf_status` | cross_community | 5 |
| `Run_fzf_request → FzfResultItem` | cross_community | 5 |

## Connected Areas

| Area | Connections |
|------|-------------|
| Lsp | 8 calls |
| Workbench | 3 calls |
| App_state | 2 calls |
| Syntax | 2 calls |
| Cluster_3 | 1 calls |
| Cluster_4 | 1 calls |
| Theme_config | 1 calls |
| Workspace | 1 calls |

## How to Explore

1. `gitnexus_context({name: "try_wait_status"})` — see callers and callees
2. `gitnexus_query({query: "scheduler"})` — find related execution flows
3. Read key files listed above for implementation details
