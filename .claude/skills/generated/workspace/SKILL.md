---
name: workspace
description: "Skill for the Workspace area of netherize_editor. 66 symbols across 4 files."
---

# Workspace

66 symbols | 4 files | Cohesion: 90%

## When to Use

- Working with code in `src/`
- Understanding how new, scan, should_ignore_dir work
- Modifying workspace-related functionality

## Key Files

| File | Symbols |
|------|---------|
| `src/workspace/model.rs` | should_ignore_dir, select_path, expand_path, collapse_path, collapse_path_and_descendants (+28) |
| `src/workspace/scanner.rs` | default, new, scan, scan_dir_recursive, build_gitignore_matcher (+11) |
| `src/app/app_state/workspace.rs` | workspace_select_path, workspace_expand_path, workspace_collapse_path, workspace_collapse_path_and_descendants, workspace_expand_path_and_descendants (+7) |
| `src/workspace/fuzzy.rs` | find_file_matches, score_candidate, unique_temp_dir, fuzzy_matches_rank_substring_hits_higher, empty_query_returns_first_files_only |

## Entry Points

Start here when exploring this area:

- **`new`** (Function) — `src/workspace/scanner.rs:30`
- **`scan`** (Function) — `src/workspace/scanner.rs:37`
- **`should_ignore_dir`** (Function) — `src/workspace/model.rs:63`
- **`select_path`** (Function) — `src/workspace/model.rs:280`
- **`expand_path`** (Function) — `src/workspace/model.rs:292`

## Key Symbols

| Symbol | Type | File | Line |
|--------|------|------|------|
| `new` | Function | `src/workspace/scanner.rs` | 30 |
| `scan` | Function | `src/workspace/scanner.rs` | 37 |
| `should_ignore_dir` | Function | `src/workspace/model.rs` | 63 |
| `select_path` | Function | `src/workspace/model.rs` | 280 |
| `expand_path` | Function | `src/workspace/model.rs` | 292 |
| `collapse_path` | Function | `src/workspace/model.rs` | 300 |
| `collapse_path_and_descendants` | Function | `src/workspace/model.rs` | 308 |
| `expand_path_and_descendants` | Function | `src/workspace/model.rs` | 328 |
| `reveal_path` | Function | `src/workspace/model.rs` | 343 |
| `expand_to_path` | Function | `src/workspace/model.rs` | 378 |
| `workspace_select_path` | Function | `src/app/app_state/workspace.rs` | 283 |
| `workspace_expand_path` | Function | `src/app/app_state/workspace.rs` | 289 |
| `workspace_collapse_path` | Function | `src/app/app_state/workspace.rs` | 295 |
| `workspace_collapse_path_and_descendants` | Function | `src/app/app_state/workspace.rs` | 301 |
| `workspace_expand_path_and_descendants` | Function | `src/app/app_state/workspace.rs` | 307 |
| `workspace_expand_to_path` | Function | `src/app/app_state/workspace.rs` | 313 |
| `workspace_reveal_path` | Function | `src/app/app_state/workspace.rs` | 319 |
| `new` | Function | `src/workspace/model.rs` | 48 |
| `load_with_rules` | Function | `src/workspace/model.rs` | 130 |
| `find_file_matches` | Function | `src/workspace/fuzzy.rs` | 11 |

## Execution Flows

| Flow | Type | Steps |
|------|------|-------|
| `Save_file_preserves_cursor_and_selection_state → Find_node` | cross_community | 9 |
| `Self_save_modify_event_is_ignored_without_reloading_cursor → Find_node` | cross_community | 9 |
| `External_reload_clamps_cursor_and_selection_to_new_buffer_length → Find_node` | cross_community | 9 |
| `Scenario_insert_and_scroll → Find_node` | cross_community | 9 |
| `External_reload_error_does_not_abort_workspace_updates → Is_hidden_name` | cross_community | 8 |
| `File_picker_results_refresh_while_overlay_is_open_after_external_create → Is_hidden_name` | cross_community | 8 |
| `External_modify_reloads_when_clean_and_warns_when_dirty → Is_hidden_name` | cross_community | 8 |
| `Modify_event_on_missing_path_triggers_workspace_rescan_for_rename_like_flow → Is_hidden_name` | cross_community | 8 |
| `Workspace_and_file_picker_state_are_tracked → Is_hidden_name` | cross_community | 8 |
| `Picker_open_query_select_flow → Find_node` | cross_community | 7 |

## Connected Areas

| Area | Connections |
|------|-------------|
| App_state | 1 calls |
| Workbench | 1 calls |

## How to Explore

1. `gitnexus_context({name: "new"})` — see callers and callees
2. `gitnexus_query({query: "workspace"})` — find related execution flows
3. Read key files listed above for implementation details
