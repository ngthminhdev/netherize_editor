---
name: event-loop
description: "Skill for the Event_loop area of netherize_editor. 202 symbols across 28 files."
---

# Event_loop

202 symbols | 28 files | Cohesion: 80%

## When to Use

- Working with code in `src/`
- Understanding how is_bold, is_italic, as_u8 work
- Modifying event_loop-related functionality

## Key Files

| File | Symbols |
|------|---------|
| `src/app/event_loop/setup.rs` | startup_subsystems, submit, submit_workspace_git_status_refresh, submit_active_buffer_git_baseline_refresh, submit_active_palette_fzf_search (+42) |
| `src/app/event_loop/helpers.rs` | byte_inside_any_span, rainbow_bracket_spans, syntax_spans_to_styled, render_markdown_node, render_children (+23) |
| `src/app/event_loop/commands_lsp.rs` | handle_lsp_and_diagnostics_command, open_lazygit_buffer, open_lazydocker_buffer, submit_git_blame_line, select_next_reference_item (+16) |
| `src/app/event_loop/async_results.rs` | on_worker_result, lsp_uri_to_path, apply_lsp_text_edits, utf16_code_unit_to_byte_idx, lsp_position_to_byte_idx (+12) |
| `src/app/event_loop/commands_ai_chat.rs` | ai_slash_command_completion_at, slash_command_suggestion_count, clean_ai_file_ref_token, ai_models_help, ai_agent_help (+7) |
| `src/app/event_loop/commands.rs` | should_persist_history_after, finalize_post_command_hooks, dispatch_command_with_focused_terminal, mark_focused_terminal_layout_dirty, handle_terminal_normal_command (+6) |
| `src/app/event_loop/commands_tests.rs` | delete_confirmation_removes_selected_file_after_y, fuzzy_picker_selection_clears_stale_preview_lines, fuzzy_picker_open_search_match_confirm_closes_results_buffer, move_to_first_line_uses_viewport_layout_path, move_to_last_line_uses_viewport_layout_path (+3) |
| `src/app/event_loop/application.rs` | window_event, handle_explorer_filter_ime_commit, handle_explorer_filter_key_event, handle_pending_confirmation_key_event, about_to_wait (+3) |
| `src/app/event_loop/commands_prompts.rs` | pending_confirmation_prompt, begin_explorer_delete_confirmation, begin_dirty_buffer_close_confirmation, open_prompt_overlay, begin_ai_chat_install_confirmation (+2) |
| `src/app/event_loop/commands_explorer.rs` | explorer_selected_entry, explorer_rename_base_selection, open_explorer_rename_prompt, handle_explorer_and_workspace_command, prepare_for_workspace_switch (+1) |

## Entry Points

Start here when exploring this area:

- **`is_bold`** (Function) — `src/syntax/highlight.rs:68`
- **`is_italic`** (Function) — `src/syntax/highlight.rs:72`
- **`as_u8`** (Function) — `src/config/theme_config/model.rs:65`
- **`welcome_screen_content`** (Function) — `src/app/event_loop/welcome.rs:2`
- **`syntax_spans_to_styled`** (Function) — `src/app/event_loop/helpers.rs:87`

## Key Symbols

| Symbol | Type | File | Line |
|--------|------|------|------|
| `is_bold` | Function | `src/syntax/highlight.rs` | 68 |
| `is_italic` | Function | `src/syntax/highlight.rs` | 72 |
| `as_u8` | Function | `src/config/theme_config/model.rs` | 65 |
| `welcome_screen_content` | Function | `src/app/event_loop/welcome.rs` | 2 |
| `syntax_spans_to_styled` | Function | `src/app/event_loop/helpers.rs` | 87 |
| `path_to_lsp_uri` | Function | `src/lsp/client.rs` | 889 |
| `startup_subsystems` | Function | `src/app/event_loop/setup.rs` | 198 |
| `submit` | Function | `src/app/event_loop/setup.rs` | 231 |
| `submit_workspace_git_status_refresh` | Function | `src/app/event_loop/setup.rs` | 306 |
| `submit_active_buffer_git_baseline_refresh` | Function | `src/app/event_loop/setup.rs` | 318 |
| `submit_active_palette_fzf_search` | Function | `src/app/event_loop/setup.rs` | 998 |
| `submit_fuzzy_picker_preview_load` | Function | `src/app/event_loop/setup.rs` | 1033 |
| `submit_active_file_history_load` | Function | `src/app/event_loop/setup.rs` | 1064 |
| `submit_active_file_history_save` | Function | `src/app/event_loop/setup.rs` | 1076 |
| `submit_references_preview_load` | Function | `src/app/event_loop/setup.rs` | 1095 |
| `submit_diagnostics_preview_load` | Function | `src/app/event_loop/setup.rs` | 1115 |
| `sync_lsp_server_for_workspace` | Function | `src/app/event_loop/setup.rs` | 1167 |
| `submit_lsp_did_open_for_active_file` | Function | `src/app/event_loop/setup.rs` | 1199 |
| `submit_lsp_did_change_for_active_file` | Function | `src/app/event_loop/setup.rs` | 1225 |
| `force_flush_lsp_did_change_for_active_file` | Function | `src/app/event_loop/setup.rs` | 1250 |

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
| App_state | 14 calls |
| Command_dispatch | 8 calls |
| Syntax | 6 calls |
| Workbench | 5 calls |
| Renderer | 4 calls |
| App | 3 calls |
| Benches | 3 calls |
| Terminal | 3 calls |

## How to Explore

1. `gitnexus_context({name: "is_bold"})` — see callers and callees
2. `gitnexus_query({query: "event_loop"})` — find related execution flows
3. Read key files listed above for implementation details
