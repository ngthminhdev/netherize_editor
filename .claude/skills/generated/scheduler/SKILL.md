---
name: scheduler
description: "Skill for the Scheduler area of netherize_editor. 93 symbols across 17 files."
---

# Scheduler

93 symbols | 17 files | Cohesion: 78%

## When to Use

- Working with code in `src/`
- Understanding how run_system_dep_install, run_pty_request, run_lsp_request work
- Modifying scheduler-related functionality

## Key Files

| File | Symbols |
|------|---------|
| `src/async_runtime/scheduler/lsp_parse.rs` | parse_hover_content, parse_completion_items, parse_text_edits, handle_lsp_hover, handle_lsp_formatting (+12) |
| `src/async_runtime/scheduler/fzf.rs` | run_fzf_request, build_file_preview_lines, build_fzf_find_file_script, build_fzf_live_grep_script, build_ripgrep_ignore_glob_args (+6) |
| `src/async_runtime/scheduler/tests.rs` | extend_unique_file_events_deduplicates_burst_entries, file_preview_lines_center_around_target_line, file_preview_lines_without_target_use_file_start, parse_git_blame_summary_extracts_author_and_relative_time, normalize_create_event_maps_to_internal_create (+5) |
| `src/editor_core.rs` | from_str, move_right, move_to_last_line, move_right_allows_eof_on_last_line_without_newline, move_right_crosses_to_next_line (+4) |
| `src/async_runtime/scheduler/syntax_jobs.rs` | run_system_dep_install, execute_virtual_job, byte_range_for_line_window, highlight_byte_window, should_highlight_full_buffer (+2) |
| `src/async_runtime/scheduler/ai_jobs.rs` | strip_ansi_sequences, should_skip_opencode_line, sanitize_opencode_line, build_prompt_with_file_context, resolve_opencode_binary (+2) |
| `src/async_runtime/scheduler/git.rs` | run_git_blame_line, parse_git_blame_summary, run_workspace_git_status, parse_git_file_status, run_fetch_git_baseline (+2) |
| `src/async_runtime/scheduler/file_watch.rs` | run_file_watch_request, execute_file_watch_loop, extend_unique_file_events, filter_file_watch_events, normalize_notify_event (+1) |
| `src/async_runtime/scheduler/local_history.rs` | local_history_path_for_file, run_local_history_request, emit_local_history_failure, execute_load_local_history, execute_save_local_history |
| `src/async_runtime/scheduler/emit.rs` | emit_message, emit_message_and_wake, failure_from_join_error, panic_payload_to_string |

## Entry Points

Start here when exploring this area:

- **`run_system_dep_install`** (Function) — `src/async_runtime/scheduler/syntax_jobs.rs:451`
- **`run_pty_request`** (Function) — `src/async_runtime/scheduler/pty.rs:22`
- **`run_lsp_request`** (Function) — `src/async_runtime/scheduler/lsp.rs:30`
- **`run_fzf_request`** (Function) — `src/async_runtime/scheduler/fzf.rs:18`
- **`run_file_watch_request`** (Function) — `src/async_runtime/scheduler/file_watch.rs:18`

## Key Symbols

| Symbol | Type | File | Line |
|--------|------|------|------|
| `run_system_dep_install` | Function | `src/async_runtime/scheduler/syntax_jobs.rs` | 451 |
| `run_pty_request` | Function | `src/async_runtime/scheduler/pty.rs` | 22 |
| `run_lsp_request` | Function | `src/async_runtime/scheduler/lsp.rs` | 30 |
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
| `execute_virtual_job` | Function | `src/async_runtime/scheduler/syntax_jobs.rs` | 23 |
| `resolve_system_path` | Function | `src/async_runtime/scheduler/syntax_jobs.rs` | 417 |
| `run_git_blame_line` | Function | `src/async_runtime/scheduler/git.rs` | 2 |
| `parse_git_blame_summary` | Function | `src/async_runtime/scheduler/git.rs` | 33 |
| `run_workspace_git_status` | Function | `src/async_runtime/scheduler/git.rs` | 87 |
| `run_fetch_git_baseline` | Function | `src/async_runtime/scheduler/git.rs` | 135 |

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
| Lsp | 9 calls |
| Syntax | 4 calls |
| App_state | 2 calls |
| Cluster_3 | 1 calls |
| Cluster_4 | 1 calls |
| Terminal | 1 calls |
| Workbench | 1 calls |
| Config | 1 calls |

## How to Explore

1. `gitnexus_context({name: "run_system_dep_install"})` — see callers and callees
2. `gitnexus_query({query: "scheduler"})` — find related execution flows
3. Read key files listed above for implementation details
