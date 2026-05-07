---
name: event-loop
description: "Skill for the Event_loop area of netherize_editor. 237 symbols across 38 files."
---

# Event_loop

237 symbols | 38 files | Cohesion: 74%

## When to Use

- Working with code in `src/`
- Understanding how editor_chrome_instances, clear_ai_chat, update_markdown_preview_content work
- Modifying event_loop-related functionality

## Key Files

| File | Symbols |
|------|---------|
| `src/app/event_loop/setup.rs` | startup_subsystems, submit, submit_workspace_git_status_refresh, submit_active_buffer_git_baseline_refresh, submit_active_palette_fzf_search (+42) |
| `src/app/event_loop/helpers.rs` | region_color, byte_inside_any_span, rainbow_bracket_spans, render_inline_spans, append_markdown_inline_fallback_spans (+27) |
| `src/app/event_loop/commands_lsp.rs` | handle_lsp_and_diagnostics_command, open_lazygit_buffer, open_lazydocker_buffer, submit_git_blame_line, select_next_reference_item (+19) |
| `src/app/event_loop/commands.rs` | dismiss_system_dep_guide, accept_system_dep_guide, clear_expired_transient_toast, dispatch_command_with_focused_terminal, mark_focused_terminal_layout_dirty (+13) |
| `src/app/event_loop/async_results.rs` | on_worker_result, lsp_uri_to_path, active_fuzzy_preview_target, active_references_preview_target, active_diagnostics_preview_target (+12) |
| `src/app/event_loop/application.rs` | redraw, focus_target_region_id, focus_ring_instances, focus_ring_keeps_outline_and_panel_fill, window_event (+7) |
| `src/app/event_loop/commands_ai_chat.rs` | ai_slash_command_completion_at, slash_command_suggestion_count, clean_ai_file_ref_token, ai_models_help, ai_agent_help (+7) |
| `src/app/event_loop/commands_prompts.rs` | confirm_theme_selection, pending_confirmation_prompt, begin_explorer_delete_confirmation, begin_dirty_buffer_close_confirmation, open_prompt_overlay (+3) |
| `src/app/event_loop/commands_tests.rs` | fuzzy_picker_selection_clears_stale_preview_lines, fuzzy_picker_open_search_match_confirm_closes_results_buffer, move_to_first_line_uses_viewport_layout_path, move_to_last_line_uses_viewport_layout_path, center_cursor_line_uses_viewport_layout_path (+3) |
| `src/app/event_loop/commands_explorer.rs` | explorer_selected_entry, explorer_rename_base_selection, open_explorer_rename_prompt, handle_explorer_and_workspace_command, prepare_for_workspace_switch (+1) |

## Entry Points

Start here when exploring this area:

- **`editor_chrome_instances`** (Function) — `src/render/renderer.rs:293`
- **`clear_ai_chat`** (Function) — `src/render/renderer/ui_render.rs:1416`
- **`update_markdown_preview_content`** (Function) — `src/render/renderer/ui_render.rs:1430`
- **`clear_palette`** (Function) — `src/render/renderer/palette.rs:43`
- **`reconfigure_surface`** (Function) — `src/render/renderer/lifecycle.rs:458`

## Key Symbols

| Symbol | Type | File | Line |
|--------|------|------|------|
| `editor_chrome_instances` | Function | `src/render/renderer.rs` | 293 |
| `clear_ai_chat` | Function | `src/render/renderer/ui_render.rs` | 1416 |
| `update_markdown_preview_content` | Function | `src/render/renderer/ui_render.rs` | 1430 |
| `clear_palette` | Function | `src/render/renderer/palette.rs` | 43 |
| `reconfigure_surface` | Function | `src/render/renderer/lifecycle.rs` | 458 |
| `draw_text_region` | Function | `src/render/renderer/helpers.rs` | 141 |
| `is_dirty` | Function | `src/app/app_state/mod.rs` | 1114 |
| `region_color` | Function | `src/app/event_loop/helpers.rs` | 1283 |
| `confirm_theme_selection` | Function | `src/app/event_loop/commands_prompts.rs` | 335 |
| `dismiss_system_dep_guide` | Function | `src/app/event_loop/commands.rs` | 449 |
| `accept_system_dep_guide` | Function | `src/app/event_loop/commands.rs` | 457 |
| `clear_expired_transient_toast` | Function | `src/app/event_loop/commands.rs` | 504 |
| `clear_welcome_logo` | Function | `src/render/renderer/ui/welcome.rs` | 531 |
| `update_terminal_content` | Function | `src/render/renderer/ui/terminal.rs` | 21 |
| `clear_terminal` | Function | `src/render/renderer/ui/terminal.rs` | 334 |
| `clear_buffer_terminal` | Function | `src/render/renderer/ui/terminal.rs` | 342 |
| `clear_sidebar` | Function | `src/render/renderer/ui/sidebar.rs` | 240 |
| `clear_system_dep_popup` | Function | `src/render/renderer/ui/popups.rs` | 449 |
| `clear_toast_popup` | Function | `src/render/renderer/ui/popups.rs` | 522 |
| `clear_leap_labels` | Function | `src/render/renderer/palette/leap.rs` | 162 |

## Execution Flows

| Flow | Type | Steps |
|------|------|-------|
| `Handle_command_with_count → Total_rows` | cross_community | 6 |
| `Handle_command_with_count → StoredFileHistory` | cross_community | 6 |
| `Handle_command_with_count → Tree_sitter_language` | cross_community | 6 |
| `Submit_lsp_did_open_for_active_file → Find_node` | cross_community | 5 |
| `Submit_lsp_did_change_for_active_file → Find_node` | cross_community | 5 |
| `Handle_command_with_count → Supports_numeric_count` | cross_community | 5 |
| `Handle_command_with_count → Dispatch_command_with_clipboard_once` | cross_community | 5 |
| `Handle_command_with_count → Groups_repeated_edits_into_single_transaction` | cross_community | 5 |
| `Handle_command_with_count → Root_node` | cross_community | 5 |
| `Handle_lsp_and_diagnostics_command → Path_to_lsp_uri` | cross_community | 4 |

## Connected Areas

| Area | Connections |
|------|-------------|
| App_state | 18 calls |
| Ui | 11 calls |
| Command_dispatch | 11 calls |
| Renderer | 9 calls |
| Syntax | 8 calls |
| Workbench | 7 calls |
| Editor | 6 calls |
| Text | 5 calls |

## How to Explore

1. `gitnexus_context({name: "editor_chrome_instances"})` — see callers and callees
2. `gitnexus_query({query: "event_loop"})` — find related execution flows
3. Read key files listed above for implementation details
