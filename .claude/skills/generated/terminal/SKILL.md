---
name: terminal
description: "Skill for the Terminal area of netherize_editor. 138 symbols across 9 files."
---

# Terminal

138 symbols | 9 files | Cohesion: 83%

## When to Use

- Working with code in `src/`
- Understanding how new, feed_chunk, cell_at work
- Modifying terminal-related functionality

## Key Files

| File | Symbols |
|------|---------|
| `src/terminal/grid.rs` | new, feed_chunk, cell_at, debug_dump, apply_regex_highlights (+75) |
| `src/terminal/ansi_parser.rs` | collect_events, plain_text_emits_print_chars, newline_and_cr, sgr_reset, sgr_empty_is_reset (+31) |
| `src/terminal/pty.rs` | spawn_shell, spawn_command, new, resolve_shell_program, resolve_shell_never_returns_empty (+3) |
| `src/terminal/terminal_renderer.rs` | new, default_monospace, cell_rect, cell_rect_calculation, default_renderer_has_positive_cell_size |
| `src/render/renderer/lifecycle.rs` | make_text_pipeline, new |
| `src/async_runtime/scheduler.rs` | alloc_session_id, async_trace_enabled |
| `src/async_runtime/scheduler/pty.rs` | execute_pty_request, spawn_pty_output_reader |
| `src/app/event_loop/commands_terminal.rs` | map_directional_focus_command, handle_terminal_and_focus_command |
| `src/core/command_dispatch/navigation.rs` | dispatch_terminal_normal |

## Entry Points

Start here when exploring this area:

- **`new`** (Function) — `src/terminal/grid.rs:170`
- **`feed_chunk`** (Function) — `src/terminal/grid.rs:191`
- **`cell_at`** (Function) — `src/terminal/grid.rs:707`
- **`debug_dump`** (Function) — `src/terminal/grid.rs:762`
- **`apply_regex_highlights`** (Function) — `src/terminal/grid.rs:815`

## Key Symbols

| Symbol | Type | File | Line |
|--------|------|------|------|
| `new` | Function | `src/terminal/grid.rs` | 170 |
| `feed_chunk` | Function | `src/terminal/grid.rs` | 191 |
| `cell_at` | Function | `src/terminal/grid.rs` | 707 |
| `debug_dump` | Function | `src/terminal/grid.rs` | 762 |
| `apply_regex_highlights` | Function | `src/terminal/grid.rs` | 815 |
| `total_rows` | Function | `src/terminal/grid.rs` | 483 |
| `live_cursor_absolute_position` | Function | `src/terminal/grid.rs` | 487 |
| `enter_normal_mode` | Function | `src/terminal/grid.rs` | 494 |
| `move_virtual_left` | Function | `src/terminal/grid.rs` | 522 |
| `move_virtual_right` | Function | `src/terminal/grid.rs` | 532 |
| `move_virtual_up` | Function | `src/terminal/grid.rs` | 542 |
| `move_virtual_down` | Function | `src/terminal/grid.rs` | 552 |
| `move_virtual_word_forward` | Function | `src/terminal/grid.rs` | 563 |
| `move_virtual_word_backward` | Function | `src/terminal/grid.rs` | 570 |
| `move_virtual_word_end` | Function | `src/terminal/grid.rs` | 577 |
| `move_virtual_to_line_start` | Function | `src/terminal/grid.rs` | 584 |
| `move_virtual_to_line_end` | Function | `src/terminal/grid.rs` | 591 |
| `move_virtual_to_first_non_whitespace` | Function | `src/terminal/grid.rs` | 598 |
| `move_virtual_to_first_line` | Function | `src/terminal/grid.rs` | 605 |
| `move_virtual_to_last_line` | Function | `src/terminal/grid.rs` | 612 |

## Execution Flows

| Flow | Type | Steps |
|------|------|-------|
| `Dispatch → Total_rows` | cross_community | 7 |
| `Handle_terminal_and_focus_command → Len_chars` | cross_community | 7 |
| `Dispatch → TerminalPoint` | cross_community | 6 |
| `Handle_terminal_and_focus_command → StoredFileHistory` | cross_community | 6 |
| `Dispatch_terminal_normal → Total_rows` | intra_community | 6 |
| `Handle_command_with_count → Total_rows` | cross_community | 6 |
| `Dispatch → Point_leq` | cross_community | 5 |
| `Handle_terminal_and_focus_command → Supports_numeric_count` | cross_community | 5 |
| `Handle_terminal_and_focus_command → Dispatch_command_with_clipboard_once` | cross_community | 5 |
| `Handle_terminal_and_focus_command → Groups_repeated_edits_into_single_transaction` | cross_community | 5 |

## Connected Areas

| Area | Connections |
|------|-------------|
| App_state | 4 calls |
| Theme_config | 2 calls |
| Renderer | 1 calls |
| Workbench | 1 calls |
| Text | 1 calls |
| Scheduler | 1 calls |
| Command_dispatch | 1 calls |

## How to Explore

1. `gitnexus_context({name: "new"})` — see callers and callees
2. `gitnexus_query({query: "terminal"})` — find related execution flows
3. Read key files listed above for implementation details
