---
name: syntax
description: "Skill for the Syntax area of netherize_editor. 103 symbols across 12 files."
---

# Syntax

103 symbols | 12 files | Cohesion: 75%

## When to Use

- Working with code in `src/`
- Understanding how parse, as_str, root_node work
- Modifying syntax-related functionality

## Key Files

| File | Symbols |
|------|---------|
| `src/syntax/highlight.rs` | generate_highlight_spans, generate_dotenv_highlight_spans, should_highlight_inline, highlight_snippet, markdown_inline_highlight_query (+53) |
| `src/syntax/syntax_engine.rs` | as_str, root_node, new, new_rust, parse_source (+15) |
| `src/async_runtime/scheduler/syntax_jobs.rs` | execute_virtual_job, byte_range_for_line_window, highlight_byte_window, should_highlight_full_buffer, cpu_burn_checksum |
| `src/async_runtime/scheduler/git.rs` | run_git_blame_line, parse_git_blame_summary, run_workspace_git_status, parse_git_file_status, run_fetch_git_baseline |
| `src/syntax/parser.rs` | language_id_for_extension, tree_sitter_markdown_inline_language, tree_sitter_language |
| `src/async_runtime/scheduler/tests.rs` | file_preview_lines_center_around_target_line, file_preview_lines_without_target_use_file_start, parse_git_blame_summary_extracts_author_and_relative_time |
| `src/app/event_loop/helpers.rs` | build_preview_render_data, parse_markdown_preview_blocks, fallback_markdown_preview |
| `src/app/event_loop/commands.rs` | reconcile_highlight_spans_with_pending_edits, close_current_buffer_now |
| `benches/editor_bench.rs` | bench_incremental_parse |
| `src/core/command_ids.rs` | parse |

## Entry Points

Start here when exploring this area:

- **`parse`** (Function) — `src/core/command_ids.rs:432`
- **`as_str`** (Function) — `src/syntax/syntax_engine.rs:30`
- **`root_node`** (Function) — `src/syntax/syntax_engine.rs:75`
- **`new`** (Function) — `src/syntax/syntax_engine.rs:98`
- **`new_rust`** (Function) — `src/syntax/syntax_engine.rs:121`

## Key Symbols

| Symbol | Type | File | Line |
|--------|------|------|------|
| `parse` | Function | `src/core/command_ids.rs` | 432 |
| `as_str` | Function | `src/syntax/syntax_engine.rs` | 30 |
| `root_node` | Function | `src/syntax/syntax_engine.rs` | 75 |
| `new` | Function | `src/syntax/syntax_engine.rs` | 98 |
| `new_rust` | Function | `src/syntax/syntax_engine.rs` | 121 |
| `parse_source` | Function | `src/syntax/syntax_engine.rs` | 127 |
| `parse_incremental` | Function | `src/syntax/syntax_engine.rs` | 151 |
| `current_tree` | Function | `src/syntax/syntax_engine.rs` | 205 |
| `language_id_for_extension` | Function | `src/syntax/parser.rs` | 10 |
| `tree_sitter_markdown_inline_language` | Function | `src/syntax/parser.rs` | 38 |
| `generate_highlight_spans` | Function | `src/syntax/highlight.rs` | 313 |
| `generate_dotenv_highlight_spans` | Function | `src/syntax/highlight.rs` | 347 |
| `should_highlight_inline` | Function | `src/syntax/highlight.rs` | 426 |
| `highlight_snippet` | Function | `src/syntax/highlight.rs` | 435 |
| `highlight_markdown_inline` | Function | `src/syntax/highlight.rs` | 668 |
| `execute_virtual_job` | Function | `src/async_runtime/scheduler/syntax_jobs.rs` | 23 |
| `run_git_blame_line` | Function | `src/async_runtime/scheduler/git.rs` | 2 |
| `parse_git_blame_summary` | Function | `src/async_runtime/scheduler/git.rs` | 33 |
| `run_workspace_git_status` | Function | `src/async_runtime/scheduler/git.rs` | 87 |
| `run_fetch_git_baseline` | Function | `src/async_runtime/scheduler/git.rs` | 135 |

## Execution Flows

| Flow | Type | Steps |
|------|------|-------|
| `Handle_command_with_count → Root_node` | cross_community | 5 |
| `Execute_virtual_job → Parse` | intra_community | 4 |
| `Execute_virtual_job → New` | cross_community | 4 |
| `Execute_virtual_job → Sanitize_byte_range` | cross_community | 4 |

## Connected Areas

| Area | Connections |
|------|-------------|
| Workbench | 5 calls |
| Event_loop | 5 calls |
| Benches | 1 calls |
| Lsp | 1 calls |
| Scheduler | 1 calls |
| App | 1 calls |
| Command_dispatch | 1 calls |

## How to Explore

1. `gitnexus_context({name: "parse"})` — see callers and callees
2. `gitnexus_query({query: "syntax"})` — find related execution flows
3. Read key files listed above for implementation details
