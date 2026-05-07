---
name: command-dispatch
description: "Skill for the Command_dispatch area of netherize_editor. 96 symbols across 12 files."
---

# Command_dispatch

96 symbols | 12 files | Cohesion: 88%

## When to Use

- Working with code in `src/`
- Understanding how dispatch_command, dispatch_command_count, dispatch_command_with_clipboard work
- Modifying command_dispatch-related functionality

## Key Files

| File | Symbols |
|------|---------|
| `src/core/command_dispatch/tests.rs` | unique_temp_path, insert_command_changes_state, insert_text_command_supports_combining_sequence, newline_command_inserts_line_break, insert_open_paren_auto_pairs_and_places_cursor_inside (+62) |
| `src/core/command_dispatch/mod.rs` | dispatch_command, dispatch_command_count, dispatch_command_with_clipboard, dispatch_command_with_terminal, dispatch_command_count_with_terminal (+4) |
| `src/app/event_loop/commands_tests.rs` | palette_paste_uses_clipboard_provider, leap_generates_multi_char_labels_after_twenty_six_matches, leap_fast_jump_label_resolves_immediately, leap_prefix_label_filters_and_waits_for_second_key, visual_selection_adds_code_context_to_ai_chat (+3) |
| `src/app/app_state/mod.rs` | new, from_text |
| `src/core/commands.rs` | supports_numeric_count, groups_repeated_edits_into_single_transaction |
| `src/app/event_loop/commands_editor.rs` | handle_insert_edit_command, handle_generic_editor_command |
| `src/text/layout_sync.rs` | caret_uses_line_relative_byte_offset_for_second_line_start |
| `src/app/app_state/state.rs` | text_string |
| `src/app/app_state/palette.rs` | active_fuzzy_picker_buffer |
| `src/app/event_loop/commands_palette.rs` | handle_palette_and_open_command |

## Entry Points

Start here when exploring this area:

- **`dispatch_command`** (Function) — `src/core/command_dispatch/mod.rs:21`
- **`dispatch_command_count`** (Function) — `src/core/command_dispatch/mod.rs:25`
- **`dispatch_command_with_clipboard`** (Function) — `src/core/command_dispatch/mod.rs:33`
- **`dispatch_command_with_terminal`** (Function) — `src/core/command_dispatch/mod.rs:50`
- **`dispatch_command_count_with_terminal`** (Function) — `src/core/command_dispatch/mod.rs:58`

## Key Symbols

| Symbol | Type | File | Line |
|--------|------|------|------|
| `dispatch_command` | Function | `src/core/command_dispatch/mod.rs` | 21 |
| `dispatch_command_count` | Function | `src/core/command_dispatch/mod.rs` | 25 |
| `dispatch_command_with_clipboard` | Function | `src/core/command_dispatch/mod.rs` | 33 |
| `dispatch_command_with_terminal` | Function | `src/core/command_dispatch/mod.rs` | 50 |
| `dispatch_command_count_with_terminal` | Function | `src/core/command_dispatch/mod.rs` | 58 |
| `text_string` | Function | `src/app/app_state/state.rs` | 509 |
| `active_fuzzy_picker_buffer` | Function | `src/app/app_state/palette.rs` | 517 |
| `new` | Function | `src/app/app_state/mod.rs` | 85 |
| `from_text` | Function | `src/app/app_state/mod.rs` | 1284 |
| `handle_palette_and_open_command` | Function | `src/app/event_loop/commands_palette.rs` | 3 |
| `supports_numeric_count` | Function | `src/core/commands.rs` | 369 |
| `groups_repeated_edits_into_single_transaction` | Function | `src/core/commands.rs` | 410 |
| `dispatch_command_with_clipboard_count` | Function | `src/core/command_dispatch/mod.rs` | 41 |
| `dispatch_command_with_clipboard_and_terminal` | Function | `src/core/command_dispatch/mod.rs` | 67 |
| `dispatch_command_with_clipboard_count_with_terminal` | Function | `src/core/command_dispatch/mod.rs` | 76 |
| `handle_insert_edit_command` | Function | `src/app/event_loop/commands_editor.rs` | 3 |
| `handle_generic_editor_command` | Function | `src/app/event_loop/commands_editor.rs` | 179 |
| `attach_workspace` | Function | `src/app/app_state/workspace.rs` | 4 |
| `caret_uses_line_relative_byte_offset_for_second_line_start` | Function | `src/text/layout_sync.rs` | 364 |
| `unique_temp_path` | Function | `src/core/command_dispatch/tests.rs` | 43 |

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
| App_state | 5 calls |
| Event_loop | 3 calls |
| Renderer | 3 calls |
| Terminal | 2 calls |
| Text | 1 calls |
| Workspace | 1 calls |
| Syntax | 1 calls |

## How to Explore

1. `gitnexus_context({name: "dispatch_command"})` — see callers and callees
2. `gitnexus_query({query: "command_dispatch"})` — find related execution flows
3. Read key files listed above for implementation details
