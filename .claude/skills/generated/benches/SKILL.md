---
name: benches
description: "Skill for the Benches area of netherize_editor. 31 symbols across 6 files."
---

# Benches

31 symbols | 6 files | Cohesion: 70%

## When to Use

- Working with code in `benches/`
- Understanding how cursor_line_col, text_len_bytes, accept_inline_suggestion work
- Modifying benches-related functionality

## Key Files

| File | Symbols |
|------|---------|
| `benches/e2e_perf_runner.rs` | bench_scratch_path, ensure_50mb_log_file, new, record, avg_ms (+9) |
| `benches/editor_bench.rs` | bench_scratch_path, ensure_50mb_log_file, bench_large_file_load, bench_language_cases, line_col_for_byte (+2) |
| `src/app/app_state/state.rs` | cursor_line_col, text_len_bytes, accept_inline_suggestion, cursor_byte_idx, cursor_byte_in_line (+2) |
| `src/app/app_state/mod.rs` | is_supported_image_path |
| `src/app/app_state/editor.rs` | move_to_last_line |
| `src/app/app_state/buffers.rs` | open_file |

## Entry Points

Start here when exploring this area:

- **`cursor_line_col`** (Function) — `src/app/app_state/state.rs:268`
- **`text_len_bytes`** (Function) — `src/app/app_state/state.rs:464`
- **`accept_inline_suggestion`** (Function) — `src/app/app_state/state.rs:760`
- **`is_supported_image_path`** (Function) — `src/app/app_state/mod.rs:161`
- **`move_to_last_line`** (Function) — `src/app/app_state/editor.rs:919`

## Key Symbols

| Symbol | Type | File | Line |
|--------|------|------|------|
| `cursor_line_col` | Function | `src/app/app_state/state.rs` | 268 |
| `text_len_bytes` | Function | `src/app/app_state/state.rs` | 464 |
| `accept_inline_suggestion` | Function | `src/app/app_state/state.rs` | 760 |
| `is_supported_image_path` | Function | `src/app/app_state/mod.rs` | 161 |
| `move_to_last_line` | Function | `src/app/app_state/editor.rs` | 919 |
| `open_file` | Function | `src/app/app_state/buffers.rs` | 4 |
| `cursor_byte_idx` | Function | `src/app/app_state/state.rs` | 279 |
| `cursor_byte_in_line` | Function | `src/app/app_state/state.rs` | 285 |
| `active_search_match_position` | Function | `src/app/app_state/state.rs` | 338 |
| `jump_to_line_and_column` | Function | `src/app/app_state/state.rs` | 405 |
| `bench_scratch_path` | Function | `benches/editor_bench.rs` | 30 |
| `ensure_50mb_log_file` | Function | `benches/editor_bench.rs` | 34 |
| `bench_large_file_load` | Function | `benches/editor_bench.rs` | 230 |
| `bench_scratch_path` | Function | `benches/e2e_perf_runner.rs` | 32 |
| `ensure_50mb_log_file` | Function | `benches/e2e_perf_runner.rs` | 36 |
| `new` | Function | `benches/e2e_perf_runner.rs` | 62 |
| `record` | Function | `benches/e2e_perf_runner.rs` | 68 |
| `avg_ms` | Function | `benches/e2e_perf_runner.rs` | 72 |
| `scenario_load_large_file` | Function | `benches/e2e_perf_runner.rs` | 98 |
| `scenario_jump_to_last_line` | Function | `benches/e2e_perf_runner.rs` | 113 |

## Execution Flows

| Flow | Type | Steps |
|------|------|-------|
| `Save_file_preserves_cursor_and_selection_state → Find_node` | cross_community | 9 |
| `Self_save_modify_event_is_ignored_without_reloading_cursor → Find_node` | cross_community | 9 |
| `External_reload_clamps_cursor_and_selection_to_new_buffer_length → Find_node` | cross_community | 9 |
| `Scenario_insert_and_scroll → Find_node` | cross_community | 9 |
| `Bench_edit_loop_latency → HelpEntry` | cross_community | 7 |
| `Bench_edit_loop_latency → Command_label_for_help` | cross_community | 7 |
| `Bench_edit_loop_latency → HelpSection` | cross_community | 7 |
| `Bench_edit_loop_latency → Find_profile_path` | cross_community | 7 |
| `Save_file_preserves_cursor_and_selection_state → Replace` | cross_community | 7 |
| `Save_file_preserves_cursor_and_selection_state → WorkspaceMatch` | cross_community | 7 |

## Connected Areas

| Area | Connections |
|------|-------------|
| Workbench | 5 calls |
| Syntax | 3 calls |
| Command_dispatch | 2 calls |
| App_state | 1 calls |
| Editor | 1 calls |
| App | 1 calls |
| Lsp | 1 calls |

## How to Explore

1. `gitnexus_context({name: "cursor_line_col"})` — see callers and callees
2. `gitnexus_query({query: "benches"})` — find related execution flows
3. Read key files listed above for implementation details
