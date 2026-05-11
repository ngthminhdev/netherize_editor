---
name: command-dispatch
description: "Skill for the Command_dispatch area of netherize_editor. 134 symbols across 18 files."
---

# Command_dispatch

134 symbols | 18 files | Cohesion: 83%

## When to Use

- Working with code in `src/`
- Understanding how supports_numeric_count, groups_repeated_edits_into_single_transaction, dispatch_command work
- Modifying command_dispatch-related functionality

## Key Files

| File | Symbols |
|------|---------|
| `src/core/command_dispatch/tests.rs` | unique_temp_path, insert_command_changes_state, insert_text_command_supports_combining_sequence, newline_command_inserts_line_break, insert_open_paren_auto_pairs_and_places_cursor_inside (+66) |
| `src/core/command_dispatch/mod.rs` | dispatch_command, dispatch_command_count, dispatch_command_with_clipboard, dispatch_command_with_clipboard_count, dispatch_command_with_terminal (+4) |
| `src/app/event_loop/commands_tests.rs` | palette_paste_uses_clipboard_provider, leap_generates_multi_char_labels_after_twenty_six_matches, leap_fast_jump_label_resolves_immediately, leap_prefix_label_filters_and_waits_for_second_key, visual_selection_adds_code_context_to_ai_chat (+3) |
| `src/core/command_dispatch/common.rs` | success, success_with_flags, failure, open_file, terminal_grid_mut (+3) |
| `src/app/app_state/buffers.rs` | new_empty_buffer, buffer_next, buffer_prev, goto_buffer_index, close_current_buffer (+2) |
| `src/app/app_state/palette.rs` | active_fuzzy_picker_buffer, open_python_env_selector, is_terminal_panel_open, set_terminal_panel_open, open_command_palette_mode (+1) |
| `src/app/event_loop/commands.rs` | dispatch_palette_overlay_command, handle_terminal_paste, forward_to_pty, forward_to_terminal_session, normalize_terminal_paste_text |
| `src/app/app_state/state.rs` | text_string, cancel_file_history_preview, clear_search_highlights |
| `src/core/command_dispatch/session.rs` | dispatch, dispatch_terminal_normal, toggle_terminal |
| `src/core/command_dispatch/palette.rs` | open_palette_mode, open_document_symbols, close_picker |

## Entry Points

Start here when exploring this area:

- **`supports_numeric_count`** (Function) — `src/core/commands.rs:395`
- **`groups_repeated_edits_into_single_transaction`** (Function) — `src/core/commands.rs:436`
- **`dispatch_command`** (Function) — `src/core/command_dispatch/mod.rs:21`
- **`dispatch_command_count`** (Function) — `src/core/command_dispatch/mod.rs:25`
- **`dispatch_command_with_clipboard`** (Function) — `src/core/command_dispatch/mod.rs:33`

## Key Symbols

| Symbol | Type | File | Line |
|--------|------|------|------|
| `supports_numeric_count` | Function | `src/core/commands.rs` | 395 |
| `groups_repeated_edits_into_single_transaction` | Function | `src/core/commands.rs` | 436 |
| `dispatch_command` | Function | `src/core/command_dispatch/mod.rs` | 21 |
| `dispatch_command_count` | Function | `src/core/command_dispatch/mod.rs` | 25 |
| `dispatch_command_with_clipboard` | Function | `src/core/command_dispatch/mod.rs` | 33 |
| `dispatch_command_with_clipboard_count` | Function | `src/core/command_dispatch/mod.rs` | 41 |
| `dispatch_command_with_terminal` | Function | `src/core/command_dispatch/mod.rs` | 50 |
| `dispatch_command_count_with_terminal` | Function | `src/core/command_dispatch/mod.rs` | 58 |
| `dispatch_command_with_clipboard_and_terminal` | Function | `src/core/command_dispatch/mod.rs` | 67 |
| `dispatch_command_with_clipboard_count_with_terminal` | Function | `src/core/command_dispatch/mod.rs` | 76 |
| `handle_palette_and_open_command` | Function | `src/app/event_loop/commands_palette.rs` | 3 |
| `handle_insert_edit_command` | Function | `src/app/event_loop/commands_editor.rs` | 3 |
| `handle_generic_editor_command` | Function | `src/app/event_loop/commands_editor.rs` | 179 |
| `text_string` | Function | `src/app/app_state/state.rs` | 468 |
| `active_fuzzy_picker_buffer` | Function | `src/app/app_state/palette.rs` | 524 |
| `from_text` | Function | `src/app/app_state/mod.rs` | 1288 |
| `exit_normal_mode` | Function | `src/terminal/grid.rs` | 522 |
| `begin_selection` | Function | `src/terminal/grid.rs` | 528 |
| `dispatch` | Function | `src/core/command_dispatch/session.rs` | 7 |
| `success` | Function | `src/core/command_dispatch/common.rs` | 20 |

## Execution Flows

| Flow | Type | Steps |
|------|------|-------|
| `External_reload_error_does_not_abort_workspace_updates → Is_hidden_name` | cross_community | 8 |
| `File_picker_results_refresh_while_overlay_is_open_after_external_create → Is_hidden_name` | cross_community | 8 |
| `External_modify_reloads_when_clean_and_warns_when_dirty → Is_hidden_name` | cross_community | 8 |
| `Modify_event_on_missing_path_triggers_workspace_rescan_for_rename_like_flow → Is_hidden_name` | cross_community | 8 |
| `Workspace_and_file_picker_state_are_tracked → Is_hidden_name` | cross_community | 8 |
| `Handle_terminal_and_focus_command → Len_chars` | cross_community | 7 |
| `Bench_edit_loop_latency → HelpEntry` | cross_community | 7 |
| `Bench_edit_loop_latency → Command_label_for_help` | cross_community | 7 |
| `Bench_edit_loop_latency → HelpSection` | cross_community | 7 |
| `Bench_edit_loop_latency → Find_profile_path` | cross_community | 7 |

## Connected Areas

| Area | Connections |
|------|-------------|
| App_state | 18 calls |
| Event_loop | 11 calls |
| Terminal | 5 calls |
| Renderer | 2 calls |
| Benches | 1 calls |
| Workspace | 1 calls |

## How to Explore

1. `gitnexus_context({name: "supports_numeric_count"})` — see callers and callees
2. `gitnexus_query({query: "command_dispatch"})` — find related execution flows
3. Read key files listed above for implementation details
