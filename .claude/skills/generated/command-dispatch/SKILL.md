---
name: command-dispatch
description: "Skill for the Command_dispatch area of netherize_editor. 93 symbols across 11 files."
---

# Command_dispatch

93 symbols | 11 files | Cohesion: 94%

## When to Use

- Working with code in `src/`
- Understanding how dispatch_command, dispatch_command_count, dispatch_command_with_clipboard work
- Modifying command_dispatch-related functionality

## Key Files

| File | Symbols |
|------|---------|
| `src/core/command_dispatch/tests.rs` | unique_temp_path, unique_temp_dir, insert_command_changes_state, insert_text_command_supports_combining_sequence, newline_command_inserts_line_break (+62) |
| `src/core/command_dispatch/mod.rs` | dispatch_command, dispatch_command_count, dispatch_command_with_clipboard, dispatch_command_with_terminal, dispatch_command_count_with_terminal (+4) |
| `src/app/event_loop/commands_tests.rs` | palette_paste_uses_clipboard_provider, leap_generates_multi_char_labels_after_twenty_six_matches, leap_fast_jump_label_resolves_immediately, leap_prefix_label_filters_and_waits_for_second_key, visual_selection_adds_code_context_to_ai_chat (+1) |
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
| `text_string` | Function | `src/app/app_state/state.rs` | 499 |
| `active_fuzzy_picker_buffer` | Function | `src/app/app_state/palette.rs` | 508 |
| `new` | Function | `src/app/app_state/mod.rs` | 84 |
| `from_text` | Function | `src/app/app_state/mod.rs` | 858 |
| `handle_palette_and_open_command` | Function | `src/app/event_loop/commands_palette.rs` | 3 |
| `supports_numeric_count` | Function | `src/core/commands.rs` | 339 |
| `groups_repeated_edits_into_single_transaction` | Function | `src/core/commands.rs` | 380 |
| `dispatch_command_with_clipboard_count` | Function | `src/core/command_dispatch/mod.rs` | 41 |
| `dispatch_command_with_clipboard_and_terminal` | Function | `src/core/command_dispatch/mod.rs` | 67 |
| `dispatch_command_with_clipboard_count_with_terminal` | Function | `src/core/command_dispatch/mod.rs` | 76 |
| `handle_insert_edit_command` | Function | `src/app/event_loop/commands_editor.rs` | 3 |
| `handle_generic_editor_command` | Function | `src/app/event_loop/commands_editor.rs` | 179 |
| `caret_uses_line_relative_byte_offset_for_second_line_start` | Function | `src/text/layout_sync.rs` | 292 |
| `unique_temp_path` | Function | `src/core/command_dispatch/tests.rs` | 43 |
| `unique_temp_dir` | Function | `src/core/command_dispatch/tests.rs` | 51 |

## Execution Flows

| Flow | Type | Steps |
|------|------|-------|
| `Handle_terminal_and_focus_command → Len_chars` | cross_community | 7 |
| `Bench_edit_loop_latency → HelpEntry` | cross_community | 7 |
| `Bench_edit_loop_latency → Command_label_for_help` | cross_community | 7 |
| `Bench_edit_loop_latency → HelpSection` | cross_community | 7 |
| `Bench_edit_loop_latency → Find_profile_path` | cross_community | 7 |
| `Handle_terminal_and_focus_command → StoredFileHistory` | cross_community | 6 |
| `Bench_edit_loop_latency → Active_profile` | cross_community | 6 |
| `Handle_command_with_count → StoredFileHistory` | cross_community | 6 |
| `Terminal_normal_selection_yanks_terminal_grid_and_returns_to_typing_mode → Active_profile` | cross_community | 6 |
| `Terminal_normal_selection_yanks_terminal_grid_and_returns_to_typing_mode → HelpEntry` | cross_community | 6 |

## Connected Areas

| Area | Connections |
|------|-------------|
| App_state | 7 calls |
| Renderer | 4 calls |
| Event_loop | 4 calls |
| Terminal | 2 calls |
| Text | 1 calls |

## How to Explore

1. `gitnexus_context({name: "dispatch_command"})` — see callers and callees
2. `gitnexus_query({query: "command_dispatch"})` — find related execution flows
3. Read key files listed above for implementation details
