---
name: syntax
description: "Skill for the Syntax area of netherize_editor. 97 symbols across 11 files."
---

# Syntax

97 symbols | 11 files | Cohesion: 72%

## When to Use

- Working with code in `src/`
- Understanding how root_node, new, new_rust work
- Modifying syntax-related functionality

## Key Files

| File | Symbols |
|------|---------|
| `src/syntax/highlight.rs` | generate_highlight_spans, generate_dotenv_highlight_spans, generate_injection_highlights, injection_language_for_query, rust_highlight_generates_core_categories (+50) |
| `src/syntax/syntax_engine.rs` | root_node, new, new_rust, parse_source, parse_incremental (+15) |
| `src/async_runtime/scheduler/syntax_jobs.rs` | execute_virtual_job, byte_range_for_line_window, highlight_byte_window, should_highlight_full_buffer, cpu_burn_checksum |
| `src/async_runtime/scheduler/git.rs` | run_git_blame_line, parse_git_blame_summary, run_workspace_git_status, parse_git_file_status, run_fetch_git_baseline |
| `src/async_runtime/scheduler/tests.rs` | file_preview_lines_center_around_target_line, file_preview_lines_without_target_use_file_start, parse_git_blame_summary_extracts_author_and_relative_time |
| `src/app/event_loop/helpers.rs` | parse_markdown_preview_blocks, fallback_markdown_preview |
| `src/syntax/parser.rs` | tree_sitter_markdown_inline_language, tree_sitter_language |
| `src/app/event_loop/commands.rs` | reconcile_highlight_spans_with_pending_edits, close_current_buffer_now |
| `benches/editor_bench.rs` | bench_incremental_parse |
| `src/async_runtime/scheduler/fzf.rs` | build_file_preview_lines |

## Entry Points

Start here when exploring this area:

- **`root_node`** (Function) — `src/syntax/syntax_engine.rs:75`
- **`new`** (Function) — `src/syntax/syntax_engine.rs:98`
- **`new_rust`** (Function) — `src/syntax/syntax_engine.rs:121`
- **`parse_source`** (Function) — `src/syntax/syntax_engine.rs:127`
- **`parse_incremental`** (Function) — `src/syntax/syntax_engine.rs:151`

## Key Symbols

| Symbol | Type | File | Line |
|--------|------|------|------|
| `root_node` | Function | `src/syntax/syntax_engine.rs` | 75 |
| `new` | Function | `src/syntax/syntax_engine.rs` | 98 |
| `new_rust` | Function | `src/syntax/syntax_engine.rs` | 121 |
| `parse_source` | Function | `src/syntax/syntax_engine.rs` | 127 |
| `parse_incremental` | Function | `src/syntax/syntax_engine.rs` | 151 |
| `current_tree` | Function | `src/syntax/syntax_engine.rs` | 205 |
| `generate_highlight_spans` | Function | `src/syntax/highlight.rs` | 304 |
| `generate_dotenv_highlight_spans` | Function | `src/syntax/highlight.rs` | 338 |
| `execute_virtual_job` | Function | `src/async_runtime/scheduler/syntax_jobs.rs` | 23 |
| `run_git_blame_line` | Function | `src/async_runtime/scheduler/git.rs` | 2 |
| `parse_git_blame_summary` | Function | `src/async_runtime/scheduler/git.rs` | 33 |
| `run_workspace_git_status` | Function | `src/async_runtime/scheduler/git.rs` | 87 |
| `run_fetch_git_baseline` | Function | `src/async_runtime/scheduler/git.rs` | 135 |
| `build_file_preview_lines` | Function | `src/async_runtime/scheduler/fzf.rs` | 223 |
| `parse_markdown_preview_blocks` | Function | `src/app/event_loop/helpers.rs` | 330 |
| `language_id` | Function | `src/syntax/syntax_engine.rs` | 79 |
| `generate_highlight_spans_in_byte_window` | Function | `src/syntax/highlight.rs` | 392 |
| `merge_highlight_spans` | Function | `src/syntax/highlight.rs` | 253 |
| `overlay_highlight_layers` | Function | `src/syntax/highlight.rs` | 288 |
| `tree_sitter_markdown_inline_language` | Function | `src/syntax/parser.rs` | 38 |

## Execution Flows

| Flow | Type | Steps |
|------|------|-------|
| `Handle_command_with_count → Root_node` | cross_community | 5 |
| `Execute_virtual_job → Parse` | cross_community | 4 |
| `Execute_virtual_job → New` | cross_community | 4 |
| `Execute_virtual_job → Sanitize_byte_range` | cross_community | 4 |

## Connected Areas

| Area | Connections |
|------|-------------|
| Workbench | 4 calls |
| Event_loop | 4 calls |
| Input_map | 3 calls |
| Benches | 1 calls |
| Lsp | 1 calls |
| Scheduler | 1 calls |
| App | 1 calls |
| Command_dispatch | 1 calls |

## How to Explore

1. `gitnexus_context({name: "root_node"})` — see callers and callees
2. `gitnexus_query({query: "syntax"})` — find related execution flows
3. Read key files listed above for implementation details
