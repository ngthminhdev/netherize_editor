---
name: benches
description: "Skill for the Benches area of netherize_editor. 30 symbols across 5 files."
---

# Benches

30 symbols | 5 files | Cohesion: 73%

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
| `src/app/app_state/overlays.rs` | revision |
| `src/app/app_state/editor.rs` | move_to_last_line |

## Entry Points

Start here when exploring this area:

- **`cursor_line_col`** (Function) — `src/app/app_state/state.rs:309`
- **`text_len_bytes`** (Function) — `src/app/app_state/state.rs:495`
- **`accept_inline_suggestion`** (Function) — `src/app/app_state/state.rs:791`
- **`revision`** (Function) — `src/app/app_state/overlays.rs:55`
- **`move_to_last_line`** (Function) — `src/app/app_state/editor.rs:847`

## Key Symbols

| Symbol | Type | File | Line |
|--------|------|------|------|
| `cursor_line_col` | Function | `src/app/app_state/state.rs` | 309 |
| `text_len_bytes` | Function | `src/app/app_state/state.rs` | 495 |
| `accept_inline_suggestion` | Function | `src/app/app_state/state.rs` | 791 |
| `revision` | Function | `src/app/app_state/overlays.rs` | 55 |
| `move_to_last_line` | Function | `src/app/app_state/editor.rs` | 847 |
| `cursor_byte_idx` | Function | `src/app/app_state/state.rs` | 320 |
| `cursor_byte_in_line` | Function | `src/app/app_state/state.rs` | 326 |
| `active_search_match_position` | Function | `src/app/app_state/state.rs` | 369 |
| `jump_to_line_and_column` | Function | `src/app/app_state/state.rs` | 436 |
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
| `scenario_insert_and_scroll` | Function | `benches/e2e_perf_runner.rs` | 129 |

## Execution Flows

| Flow | Type | Steps |
|------|------|-------|
| `Scenario_insert_and_scroll → Find_node` | cross_community | 9 |
| `Bench_edit_loop_latency → HelpEntry` | cross_community | 7 |
| `Bench_edit_loop_latency → Command_label_for_help` | cross_community | 7 |
| `Bench_edit_loop_latency → HelpSection` | cross_community | 7 |
| `Bench_edit_loop_latency → Find_profile_path` | cross_community | 7 |
| `Scenario_insert_and_scroll → Replace` | cross_community | 7 |
| `Scenario_insert_and_scroll → WorkspaceMatch` | cross_community | 7 |
| `Bench_edit_loop_latency → Active_profile` | cross_community | 6 |
| `Main → Bench_scratch_path` | intra_community | 4 |
| `Main → Is_supported_image_path` | cross_community | 4 |

## Connected Areas

| Area | Connections |
|------|-------------|
| Workbench | 5 calls |
| App_state | 5 calls |
| Syntax | 3 calls |
| Command_dispatch | 2 calls |

## How to Explore

1. `gitnexus_context({name: "cursor_line_col"})` — see callers and callees
2. `gitnexus_query({query: "benches"})` — find related execution flows
3. Read key files listed above for implementation details
