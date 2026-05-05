---
name: app-state
description: "Skill for the App_state area of netherize_editor. 382 symbols across 30 files."
---

# App_state

382 symbols | 30 files | Cohesion: 75%

## When to Use

- Working with code in `src/`
- Understanding how exit_normal_mode, begin_selection, theme work
- Modifying app_state-related functionality

## Key Files

| File | Symbols |
|------|---------|
| `src/app/app_state/overlays.rs` | current_mode, can_apply_mode_event, apply_mode_event, len_chars, preview (+59) |
| `src/app/app_state/tests.rs` | unique_temp_path, text_edits_record_highlight_byte_deltas, backspace_between_empty_auto_pair_deletes_both_chars, backspace_between_empty_quotes_deletes_both_chars, backspace_between_empty_backticks_deletes_both_chars (+50) |
| `src/app/app_state/palette.rs` | open_command_palette_mode, push_jump, open_theme_selector_palette, open_document_symbols_palette_loading, close_command_palette (+43) |
| `src/app/app_state/state.rs` | begin_file_history_preview_session, accept_file_history_preview, cancel_file_history_preview, clipboard_record_kind_for_text, completion_prefix_info_at (+38) |
| `src/app/app_state/buffers.rs` | new_empty_buffer, buffer_next, buffer_prev, goto_buffer_index, close_current_buffer (+25) |
| `src/app/app_state/editor.rs` | jump_to_line, insert_tab, step_over_closing_char, insert_auto_pair, smart_insert_newline (+19) |
| `src/app/app_state/mod.rs` | is_supported_image_path, from_bindings, build_help_sections, build_help_lines, command_label_for_help (+10) |
| `src/core/command_dispatch/common.rs` | success, success_with_flags, failure, open_file, enter_insert_mode_if_needed (+9) |
| `src/editor_core.rs` | move_word_forward, move_word_end, delete_word_forward, change_word_forward, classify_char (+6) |
| `src/app/app_state/workspace.rs` | attach_workspace, workspace_git_status, workspace_is_expanded, recalculate_active_buffer_git_diff, recalculate_git_diff (+4) |

## Entry Points

Start here when exploring this area:

- **`exit_normal_mode`** (Function) — `src/terminal/grid.rs:503`
- **`begin_selection`** (Function) — `src/terminal/grid.rs:509`
- **`theme`** (Function) — `src/app/command_palette.rs:219`
- **`dispatch`** (Function) — `src/core/command_dispatch/session.rs:7`
- **`dispatch`** (Function) — `src/core/command_dispatch/palette.rs:12`

## Key Symbols

| Symbol | Type | File | Line |
|--------|------|------|------|
| `exit_normal_mode` | Function | `src/terminal/grid.rs` | 503 |
| `begin_selection` | Function | `src/terminal/grid.rs` | 509 |
| `theme` | Function | `src/app/command_palette.rs` | 219 |
| `dispatch` | Function | `src/core/command_dispatch/session.rs` | 7 |
| `dispatch` | Function | `src/core/command_dispatch/palette.rs` | 12 |
| `success` | Function | `src/core/command_dispatch/common.rs` | 20 |
| `success_with_flags` | Function | `src/core/command_dispatch/common.rs` | 29 |
| `failure` | Function | `src/core/command_dispatch/common.rs` | 42 |
| `open_file` | Function | `src/core/command_dispatch/common.rs` | 74 |
| `enter_insert_mode_if_needed` | Function | `src/core/command_dispatch/common.rs` | 86 |
| `close_palette_and_exit_focus` | Function | `src/core/command_dispatch/common.rs` | 155 |
| `terminal_grid_mut` | Function | `src/core/command_dispatch/common.rs` | 167 |
| `terminal_normal_active` | Function | `src/core/command_dispatch/common.rs` | 171 |
| `begin_file_history_preview_session` | Function | `src/app/app_state/state.rs` | 108 |
| `accept_file_history_preview` | Function | `src/app/app_state/state.rs` | 167 |
| `cancel_file_history_preview` | Function | `src/app/app_state/state.rs` | 200 |
| `open_command_palette_mode` | Function | `src/app/app_state/palette.rs` | 4 |
| `push_jump` | Function | `src/app/app_state/palette.rs` | 26 |
| `open_theme_selector_palette` | Function | `src/app/app_state/palette.rs` | 94 |
| `open_document_symbols_palette_loading` | Function | `src/app/app_state/palette.rs` | 117 |

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
| `Dispatch → Total_rows` | cross_community | 7 |

## Connected Areas

| Area | Connections |
|------|-------------|
| Command_dispatch | 33 calls |
| Cluster_3 | 6 calls |
| Workbench | 6 calls |
| App | 6 calls |
| Terminal | 4 calls |
| Benches | 3 calls |
| Renderer | 3 calls |
| Event_loop | 2 calls |

## How to Explore

1. `gitnexus_context({name: "exit_normal_mode"})` — see callers and callees
2. `gitnexus_query({query: "app_state"})` — find related execution flows
3. Read key files listed above for implementation details
