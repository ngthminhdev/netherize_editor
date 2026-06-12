---
name: terminal
description: "Skill for the Terminal area of netherize_editor. 140 symbols across 11 files."
---

# Terminal

140 symbols | 11 files | Cohesion: 83%

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
| `src/async_runtime/scheduler.rs` | alloc_session_id, async_trace_enabled |
| `src/render/renderer/lifecycle.rs` | make_text_pipeline, new |
| `src/app/event_loop/commands_terminal.rs` | handle_terminal_search_command, word_at_virtual_cursor |
| `src/core/command_dispatch/navigation.rs` | dispatch_terminal_normal |
| `src/async_runtime/scheduler/pty.rs` | execute_pty_request |
| `src/render/renderer/ui/terminal.rs` | append_terminal_overlay_quads |

## Entry Points

Start here when exploring this area:

- **`new`** (Function) — `src/terminal/grid.rs:187`
- **`feed_chunk`** (Function) — `src/terminal/grid.rs:210`
- **`cell_at`** (Function) — `src/terminal/grid.rs:757`
- **`apply_regex_highlights`** (Function) — `src/terminal/grid.rs:865`
- **`total_rows`** (Function) — `src/terminal/grid.rs:502`

## Key Symbols

| Symbol | Type | File | Line |
|--------|------|------|------|
| `new` | Function | `src/terminal/grid.rs` | 187 |
| `feed_chunk` | Function | `src/terminal/grid.rs` | 210 |
| `cell_at` | Function | `src/terminal/grid.rs` | 757 |
| `apply_regex_highlights` | Function | `src/terminal/grid.rs` | 865 |
| `total_rows` | Function | `src/terminal/grid.rs` | 502 |
| `live_cursor_absolute_position` | Function | `src/terminal/grid.rs` | 506 |
| `enter_normal_mode` | Function | `src/terminal/grid.rs` | 513 |
| `move_virtual_left` | Function | `src/terminal/grid.rs` | 541 |
| `move_virtual_right` | Function | `src/terminal/grid.rs` | 551 |
| `move_virtual_up` | Function | `src/terminal/grid.rs` | 561 |
| `move_virtual_down` | Function | `src/terminal/grid.rs` | 571 |
| `move_virtual_word_forward` | Function | `src/terminal/grid.rs` | 582 |
| `move_virtual_word_backward` | Function | `src/terminal/grid.rs` | 589 |
| `move_virtual_word_end` | Function | `src/terminal/grid.rs` | 596 |
| `move_virtual_to_line_start` | Function | `src/terminal/grid.rs` | 603 |
| `move_virtual_to_line_end` | Function | `src/terminal/grid.rs` | 610 |
| `move_virtual_to_first_non_whitespace` | Function | `src/terminal/grid.rs` | 617 |
| `move_virtual_to_first_line` | Function | `src/terminal/grid.rs` | 624 |
| `move_virtual_to_last_line` | Function | `src/terminal/grid.rs` | 631 |
| `move_virtual_half_page_up` | Function | `src/terminal/grid.rs` | 638 |

## Execution Flows

| Flow | Type | Steps |
|------|------|-------|
| `Dispatch → Total_rows` | cross_community | 7 |
| `Dispatch → TerminalPoint` | cross_community | 6 |
| `Dispatch_terminal_normal → Total_rows` | intra_community | 6 |
| `Handle_command_with_count → Total_rows` | cross_community | 6 |
| `Dispatch → Point_leq` | cross_community | 5 |
| `New → From_rgba_u8` | cross_community | 5 |
| `Terminal_normal_selection_yanks_terminal_grid_and_returns_to_typing_mode → Clamp_add` | cross_community | 5 |
| `Terminal_normal_selection_yanks_terminal_grid_and_returns_to_typing_mode → Blank` | cross_community | 5 |
| `Terminal_normal_selection_yanks_terminal_grid_and_returns_to_typing_mode → Total_rows` | cross_community | 5 |
| `Terminal_normal_selection_yanks_terminal_grid_and_returns_to_typing_mode → TerminalPoint` | cross_community | 5 |

## Connected Areas

| Area | Connections |
|------|-------------|
| Command_dispatch | 3 calls |
| Workbench | 2 calls |
| Palette | 2 calls |
| Theme_config | 2 calls |
| Scheduler | 2 calls |
| App_state | 1 calls |
| Text | 1 calls |
| Event_loop | 1 calls |

## How to Explore

1. `gitnexus_context({name: "new"})` — see callers and callees
2. `gitnexus_query({query: "terminal"})` — find related execution flows
3. Read key files listed above for implementation details
