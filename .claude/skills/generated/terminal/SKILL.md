---
name: terminal
description: "Skill for the Terminal area of netherize_editor. 139 symbols across 9 files."
---

# Terminal

139 symbols | 9 files | Cohesion: 82%

## When to Use

- Working with code in `src/`
- Understanding how new, feed_chunk, cell_at work
- Modifying terminal-related functionality

## Key Files

| File | Symbols |
|------|---------|
| `src/terminal/grid.rs` | new, feed_chunk, cell_at, apply_regex_highlights, set_visible_row_style_fg (+77) |
| `src/terminal/ansi_parser.rs` | collect_events, plain_text_emits_print_chars, newline_and_cr, sgr_reset, sgr_empty_is_reset (+31) |
| `src/terminal/pty.rs` | spawn_shell, spawn_command, new, write_input, resolve_shell_program (+2) |
| `src/terminal/terminal_renderer.rs` | new, default_monospace, cell_rect, cell_rect_calculation, default_renderer_has_positive_cell_size |
| `src/app/event_loop/commands_terminal.rs` | handle_terminal_search_command, word_at_virtual_cursor, map_directional_focus_command, handle_terminal_and_focus_command |
| `src/async_runtime/scheduler.rs` | alloc_session_id, async_trace_enabled |
| `src/core/command_dispatch/navigation.rs` | dispatch_terminal_normal |
| `src/async_runtime/scheduler/pty.rs` | execute_pty_request |
| `src/app/event_loop/setup.rs` | sync_in_file_search_with_palette_query |

## Entry Points

Start here when exploring this area:

- **`new`** (Function) — `src/terminal/grid.rs:186`
- **`feed_chunk`** (Function) — `src/terminal/grid.rs:209`
- **`cell_at`** (Function) — `src/terminal/grid.rs:756`
- **`apply_regex_highlights`** (Function) — `src/terminal/grid.rs:864`
- **`total_rows`** (Function) — `src/terminal/grid.rs:501`

## Key Symbols

| Symbol | Type | File | Line |
|--------|------|------|------|
| `new` | Function | `src/terminal/grid.rs` | 186 |
| `feed_chunk` | Function | `src/terminal/grid.rs` | 209 |
| `cell_at` | Function | `src/terminal/grid.rs` | 756 |
| `apply_regex_highlights` | Function | `src/terminal/grid.rs` | 864 |
| `total_rows` | Function | `src/terminal/grid.rs` | 501 |
| `move_virtual_left` | Function | `src/terminal/grid.rs` | 540 |
| `move_virtual_right` | Function | `src/terminal/grid.rs` | 550 |
| `move_virtual_up` | Function | `src/terminal/grid.rs` | 560 |
| `move_virtual_down` | Function | `src/terminal/grid.rs` | 570 |
| `move_virtual_word_forward` | Function | `src/terminal/grid.rs` | 581 |
| `move_virtual_word_backward` | Function | `src/terminal/grid.rs` | 588 |
| `move_virtual_word_end` | Function | `src/terminal/grid.rs` | 595 |
| `move_virtual_to_line_start` | Function | `src/terminal/grid.rs` | 602 |
| `move_virtual_to_line_end` | Function | `src/terminal/grid.rs` | 609 |
| `move_virtual_to_first_non_whitespace` | Function | `src/terminal/grid.rs` | 616 |
| `move_virtual_to_first_line` | Function | `src/terminal/grid.rs` | 623 |
| `move_virtual_to_last_line` | Function | `src/terminal/grid.rs` | 630 |
| `move_virtual_half_page_up` | Function | `src/terminal/grid.rs` | 637 |
| `move_virtual_half_page_down` | Function | `src/terminal/grid.rs` | 646 |
| `center_virtual_cursor_line` | Function | `src/terminal/grid.rs` | 656 |

## Execution Flows

| Flow | Type | Steps |
|------|------|-------|
| `Dispatch → Total_rows` | cross_community | 7 |
| `Handle_terminal_and_focus_command → Len_chars` | cross_community | 7 |
| `Dispatch → TerminalPoint` | cross_community | 6 |
| `Handle_terminal_and_focus_command → Success` | cross_community | 6 |
| `Handle_terminal_and_focus_command → Open_python_env_selector` | cross_community | 6 |
| `Handle_terminal_and_focus_command → Success_with_flags` | cross_community | 6 |
| `Handle_terminal_and_focus_command → StoredFileHistory` | cross_community | 6 |
| `Dispatch_terminal_normal → Total_rows` | intra_community | 6 |
| `Handle_command_with_count → Total_rows` | cross_community | 6 |
| `Dispatch → Point_leq` | cross_community | 5 |

## Connected Areas

| Area | Connections |
|------|-------------|
| App_state | 4 calls |
| Workbench | 2 calls |
| Scheduler | 2 calls |
| Ui | 1 calls |
| Command_dispatch | 1 calls |

## How to Explore

1. `gitnexus_context({name: "new"})` — see callers and callees
2. `gitnexus_query({query: "terminal"})` — find related execution flows
3. Read key files listed above for implementation details
