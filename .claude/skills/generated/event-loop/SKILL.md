---
name: event-loop
description: "Skill for the Event_loop area of netherize_editor. 260 symbols across 40 files."
---

# Event_loop

260 symbols | 40 files | Cohesion: 78%

## When to Use

- Working with code in `src/`
- Understanding how editor_chrome_instances, clear_palette, reconfigure_surface work
- Modifying event_loop-related functionality

## Key Files

| File | Symbols |
|------|---------|
| `src/app/event_loop/setup.rs` | startup_subsystems, submit, submit_workspace_git_status_refresh, submit_active_buffer_git_baseline_refresh, flush_lsp_retry_if_due (+41) |
| `src/app/event_loop/helpers.rs` | byte_inside_any_span, rainbow_bracket_spans, render_markdown_node, render_children, offset_styled_span (+30) |
| `src/app/event_loop/commands_lsp.rs` | handle_lsp_and_diagnostics_command, open_lazygit_buffer, open_lazydocker_buffer, submit_git_blame_line, select_next_reference_item (+21) |
| `src/app/event_loop/async_results.rs` | on_worker_result, lsp_uri_to_path, active_fuzzy_preview_target, active_references_preview_target, active_diagnostics_preview_target (+12) |
| `src/app/event_loop/commands.rs` | dismiss_system_dep_guide, accept_system_dep_guide, clear_expired_transient_toast, should_persist_history_after, finalize_post_command_hooks (+10) |
| `src/app/event_loop/application.rs` | redraw, focus_target_region_id, focus_ring_instances, focus_ring_keeps_outline_and_panel_fill, window_event (+8) |
| `src/app/event_loop/commands_tests.rs` | fuzzy_picker_selection_clears_stale_preview_lines, fuzzy_picker_open_search_match_confirm_closes_results_buffer, move_to_first_line_uses_viewport_layout_path, move_to_last_line_uses_viewport_layout_path, center_cursor_line_uses_viewport_layout_path (+8) |
| `src/app/event_loop/commands_ai_chat.rs` | ai_slash_command_completion_at, slash_command_suggestion_count, clean_ai_file_ref_token, ai_models_help, ai_agent_help (+7) |
| `src/app/event_loop/commands_prompts.rs` | confirm_theme_selection, pending_confirmation_prompt, begin_explorer_delete_confirmation, begin_dirty_buffer_close_confirmation, open_prompt_overlay (+3) |
| `src/app/event_loop/commands_completion.rs` | select_next_completion_item, select_prev_completion_item, schedule_completion_resolve_debounced, close_completion_popup, accept_completion_item (+3) |

## Entry Points

Start here when exploring this area:

- **`editor_chrome_instances`** (Function) — `src/render/renderer.rs:309`
- **`clear_palette`** (Function) — `src/render/renderer/palette.rs:43`
- **`reconfigure_surface`** (Function) — `src/render/renderer/lifecycle.rs:461`
- **`draw_text_region`** (Function) — `src/render/renderer/helpers.rs:141`
- **`confirm_theme_selection`** (Function) — `src/app/event_loop/commands_prompts.rs:336`

## Key Symbols

| Symbol | Type | File | Line |
|--------|------|------|------|
| `editor_chrome_instances` | Function | `src/render/renderer.rs` | 309 |
| `clear_palette` | Function | `src/render/renderer/palette.rs` | 43 |
| `reconfigure_surface` | Function | `src/render/renderer/lifecycle.rs` | 461 |
| `draw_text_region` | Function | `src/render/renderer/helpers.rs` | 141 |
| `confirm_theme_selection` | Function | `src/app/event_loop/commands_prompts.rs` | 336 |
| `dismiss_system_dep_guide` | Function | `src/app/event_loop/commands.rs` | 446 |
| `accept_system_dep_guide` | Function | `src/app/event_loop/commands.rs` | 454 |
| `clear_expired_transient_toast` | Function | `src/app/event_loop/commands.rs` | 501 |
| `status_label` | Function | `src/app/app_state/mod.rs` | 81 |
| `is_dirty` | Function | `src/app/app_state/mod.rs` | 1098 |
| `clear_welcome_logo` | Function | `src/render/renderer/ui/welcome.rs` | 531 |
| `update_terminal_content` | Function | `src/render/renderer/ui/terminal.rs` | 21 |
| `clear_terminal` | Function | `src/render/renderer/ui/terminal.rs` | 334 |
| `clear_buffer_terminal` | Function | `src/render/renderer/ui/terminal.rs` | 342 |
| `clear_sidebar` | Function | `src/render/renderer/ui/sidebar.rs` | 240 |
| `clear_system_dep_popup` | Function | `src/render/renderer/ui/popups.rs` | 458 |
| `clear_toast_popup` | Function | `src/render/renderer/ui/popups.rs` | 531 |
| `render` | Function | `src/render/renderer/lifecycle/frame.rs` | 11 |
| `clear_leap_labels` | Function | `src/render/renderer/palette/leap.rs` | 162 |
| `clear_editor_content` | Function | `src/render/renderer/editor/viewport.rs` | 43 |

## Execution Flows

| Flow | Type | Steps |
|------|------|-------|
| `Startup_subsystems → Find_node` | cross_community | 6 |
| `Handle_explorer_and_workspace_command → Login_shell_path_cache` | cross_community | 6 |
| `Handle_explorer_and_workspace_command → Probe_path_from_login_shell` | cross_community | 6 |
| `Handle_explorer_and_workspace_command → Resolve_nvm_bin` | cross_community | 6 |
| `Handle_explorer_and_workspace_command → Success` | cross_community | 6 |
| `Handle_explorer_and_workspace_command → Open_python_env_selector` | cross_community | 6 |
| `Handle_command_with_count → Total_rows` | cross_community | 6 |
| `Handle_command_with_count → Success` | cross_community | 6 |
| `Handle_command_with_count → Open_python_env_selector` | cross_community | 6 |
| `Handle_command_with_count → Success_with_flags` | cross_community | 6 |

## Connected Areas

| Area | Connections |
|------|-------------|
| Renderer | 20 calls |
| App_state | 18 calls |
| Command_dispatch | 11 calls |
| Syntax | 8 calls |
| Workbench | 8 calls |
| Text | 6 calls |
| App | 5 calls |
| Editor | 5 calls |

## How to Explore

1. `gitnexus_context({name: "editor_chrome_instances"})` — see callers and callees
2. `gitnexus_query({query: "event_loop"})` — find related execution flows
3. Read key files listed above for implementation details
