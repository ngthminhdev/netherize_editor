---
name: event-loop
description: "Skill for the Event_loop area of netherize_editor. 289 symbols across 46 files."
---

# Event_loop

289 symbols | 46 files | Cohesion: 76%

## When to Use

- Working with code in `src/`
- Understanding how set_metrics, from_theme, editor_chrome_instances work
- Modifying event_loop-related functionality

## Key Files

| File | Symbols |
|------|---------|
| `src/app/event_loop/setup.rs` | startup_subsystems, submit, submit_workspace_git_status_refresh, submit_active_buffer_git_baseline_refresh, flush_lsp_retry_if_due (+41) |
| `src/app/event_loop/helpers.rs` | build_sidebar_rows, region_color, byte_inside_any_span, rainbow_bracket_spans, render_markdown_node (+33) |
| `src/app/event_loop/commands_lsp.rs` | handle_lsp_and_diagnostics_command, open_lazygit_buffer, open_lazydocker_buffer, submit_git_blame_line, select_next_reference_item (+20) |
| `src/app/event_loop/async_results.rs` | on_worker_result, lsp_uri_to_path, active_fuzzy_preview_target, active_references_preview_target, active_diagnostics_preview_target (+12) |
| `src/app/event_loop/commands.rs` | dismiss_system_dep_guide, accept_system_dep_guide, clear_expired_transient_toast, should_persist_history_after, finalize_post_command_hooks (+9) |
| `src/app/event_loop/commands_tests.rs` | move_to_first_line_uses_viewport_layout_path, move_to_last_line_uses_viewport_layout_path, center_cursor_line_uses_viewport_layout_path, scroll_half_page_down_uses_viewport_layout_path, fuzzy_picker_selection_clears_stale_preview_lines (+8) |
| `src/app/event_loop/commands_ai_chat.rs` | ai_slash_command_completion_at, slash_command_suggestion_count, clean_ai_file_ref_token, ai_models_help, ai_agent_help (+7) |
| `src/app/event_loop/application.rs` | redraw, focus_target_region_id, window_event, handle_explorer_filter_ime_commit, handle_explorer_filter_key_event (+6) |
| `src/app/event_loop/mod.rs` | is_running, active_terminal_grid_mut, focused_terminal_grid_mut, focused_terminal_session_id, active_terminal_tab (+5) |
| `src/app/event_loop/commands_terminal.rs` | default_terminal_working_dir, spawn_shell_for_terminal_tab, ensure_active_terminal_tab_spawned, map_directional_focus_command, handle_terminal_and_focus_command (+5) |

## Entry Points

Start here when exploring this area:

- **`set_metrics`** (Function) — `src/text/text_system.rs:127`
- **`from_theme`** (Function) — `src/terminal/grid.rs:128`
- **`editor_chrome_instances`** (Function) — `src/render/renderer.rs:311`
- **`clear_palette`** (Function) — `src/render/renderer/palette.rs:43`
- **`reconfigure_surface`** (Function) — `src/render/renderer/lifecycle.rs:463`

## Key Symbols

| Symbol | Type | File | Line |
|--------|------|------|------|
| `set_metrics` | Function | `src/text/text_system.rs` | 127 |
| `from_theme` | Function | `src/terminal/grid.rs` | 128 |
| `editor_chrome_instances` | Function | `src/render/renderer.rs` | 311 |
| `clear_palette` | Function | `src/render/renderer/palette.rs` | 43 |
| `reconfigure_surface` | Function | `src/render/renderer/lifecycle.rs` | 463 |
| `draw_text_region` | Function | `src/render/renderer/helpers.rs` | 141 |
| `as_f32` | Function | `src/config/theme_config/model.rs` | 83 |
| `sidebar_arrow` | Function | `src/config/theme_config/model.rs` | 277 |
| `get_icon_for_file` | Function | `src/config/theme_config/model.rs` | 368 |
| `is_running` | Function | `src/app/event_loop/mod.rs` | 365 |
| `build_sidebar_rows` | Function | `src/app/event_loop/helpers.rs` | 1414 |
| `region_color` | Function | `src/app/event_loop/helpers.rs` | 1475 |
| `confirm_theme_selection` | Function | `src/app/event_loop/commands_prompts.rs` | 336 |
| `switch_workspace_to` | Function | `src/app/event_loop/commands_explorer.rs` | 118 |
| `dismiss_system_dep_guide` | Function | `src/app/event_loop/commands.rs` | 449 |
| `accept_system_dep_guide` | Function | `src/app/event_loop/commands.rs` | 457 |
| `clear_expired_transient_toast` | Function | `src/app/event_loop/commands.rs` | 504 |
| `status_label` | Function | `src/app/app_state/mod.rs` | 81 |
| `is_dirty` | Function | `src/app/app_state/mod.rs` | 1111 |
| `update_terminal_content` | Function | `src/render/renderer/ui/terminal.rs` | 25 |

## Execution Flows

| Flow | Type | Steps |
|------|------|-------|
| `Handle_terminal_and_focus_command → Len_chars` | cross_community | 7 |
| `Handle_terminal_and_focus_command → Success` | cross_community | 6 |
| `Handle_terminal_and_focus_command → Open_python_env_selector` | cross_community | 6 |
| `Handle_terminal_and_focus_command → Success_with_flags` | cross_community | 6 |
| `Startup_subsystems → Find_node` | cross_community | 6 |
| `Handle_explorer_and_workspace_command → Login_shell_path_cache` | cross_community | 6 |
| `Handle_explorer_and_workspace_command → Probe_path_from_login_shell` | cross_community | 6 |
| `Handle_explorer_and_workspace_command → Resolve_nvm_bin` | cross_community | 6 |
| `Handle_explorer_and_workspace_command → Success` | cross_community | 6 |
| `Handle_explorer_and_workspace_command → Open_python_env_selector` | cross_community | 6 |

## Connected Areas

| Area | Connections |
|------|-------------|
| App_state | 23 calls |
| Renderer | 15 calls |
| Command_dispatch | 12 calls |
| Palette | 10 calls |
| Workbench | 9 calls |
| Syntax | 9 calls |
| Text | 7 calls |
| Theme_config | 4 calls |

## How to Explore

1. `gitnexus_context({name: "set_metrics"})` — see callers and callees
2. `gitnexus_query({query: "event_loop"})` — find related execution flows
3. Read key files listed above for implementation details
