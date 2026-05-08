---
name: app-state
description: "Skill for the App_state area of netherize_editor. 438 symbols across 37 files."
---

# App_state

438 symbols | 37 files | Cohesion: 73%

## When to Use

- Working with code in `src/`
- Understanding how delete_current_line, find_text_object_range, len_chars work
- Modifying app_state-related functionality

## Key Files

| File | Symbols |
|------|---------|
| `src/app/app_state/overlays.rs` | len_chars, len_lines, preview, cursor_state, restore_cursor_state (+61) |
| `src/app/app_state/tests.rs` | text_object_delete_removes_inner, text_object_select_enters_visual_mode, unique_temp_path, text_edits_record_highlight_byte_deltas, backspace_between_empty_auto_pair_deletes_both_chars (+56) |
| `src/app/app_state/state.rs` | begin_file_history_preview_session, accept_file_history_preview, cancel_file_history_preview, clipboard_record_kind_for_text, completion_prefix_info_at (+47) |
| `src/app/app_state/palette.rs` | open_command_palette_mode, open_python_env_selector, push_jump, open_theme_selector_palette, open_document_symbols_palette_loading (+42) |
| `src/app/app_state/buffers.rs` | text_object_text, delete_text_object, clear_visual_selection, delete_char_text_at_cursor, substitute_current_line_text (+25) |
| `src/app/app_state/editor.rs` | jump_to_line, insert_tab, step_over_closing_char, insert_auto_pair, smart_insert_newline (+21) |
| `src/app/app_state/multi_cursor.rs` | multi_cursor_selection_ranges, char_range_to_vsr, multi_cursor_select_all_visual, multi_cursor_add_next, multi_cursor_skip (+18) |
| `src/app/app_state/mod.rs` | is_supported_image_path, from_bindings, build_help_sections, build_help_lines, command_label_for_help (+14) |
| `src/core/command_dispatch/common.rs` | success, success_with_flags, failure, open_file, enter_insert_mode_if_needed (+9) |
| `src/app/app_state/workspace.rs` | attach_workspace, workspace_git_status, workspace_is_expanded, recalculate_active_buffer_git_diff, recalculate_git_diff (+7) |

## Entry Points

Start here when exploring this area:

- **`delete_current_line`** (Function) — `src/editor_core.rs:449`
- **`find_text_object_range`** (Function) — `src/core/text_object.rs:3`
- **`len_chars`** (Function) — `src/app/app_state/overlays.rs:94`
- **`len_lines`** (Function) — `src/app/app_state/overlays.rs:98`
- **`preview`** (Function) — `src/app/app_state/overlays.rs:146`

## Key Symbols

| Symbol | Type | File | Line |
|--------|------|------|------|
| `delete_current_line` | Function | `src/editor_core.rs` | 449 |
| `find_text_object_range` | Function | `src/core/text_object.rs` | 3 |
| `len_chars` | Function | `src/app/app_state/overlays.rs` | 94 |
| `len_lines` | Function | `src/app/app_state/overlays.rs` | 98 |
| `preview` | Function | `src/app/app_state/overlays.rs` | 146 |
| `cursor_state` | Function | `src/app/app_state/overlays.rs` | 209 |
| `restore_cursor_state` | Function | `src/app/app_state/overlays.rs` | 216 |
| `snapshot_editor_view` | Function | `src/app/app_state/overlays.rs` | 224 |
| `restore_editor_view` | Function | `src/app/app_state/overlays.rs` | 237 |
| `ensure_current_transaction` | Function | `src/app/app_state/overlays.rs` | 249 |
| `apply_insert` | Function | `src/app/app_state/overlays.rs` | 258 |
| `apply_delete` | Function | `src/app/app_state/overlays.rs` | 270 |
| `apply_insert_raw` | Function | `src/app/app_state/overlays.rs` | 284 |
| `apply_delete_raw` | Function | `src/app/app_state/overlays.rs` | 294 |
| `char_range_text` | Function | `src/app/app_state/overlays.rs` | 310 |
| `linewise_text_for_range` | Function | `src/app/app_state/overlays.rs` | 323 |
| `delete_char_range_at_cursor` | Function | `src/app/app_state/overlays.rs` | 331 |
| `current_line_delete_range` | Function | `src/app/app_state/overlays.rs` | 360 |
| `delete_word_forward_range` | Function | `src/app/app_state/overlays.rs` | 384 |
| `yank_word_end_range` | Function | `src/app/app_state/overlays.rs` | 403 |

## Execution Flows

| Flow | Type | Steps |
|------|------|-------|
| `Save_file_preserves_cursor_and_selection_state → Find_node` | cross_community | 9 |
| `Self_save_modify_event_is_ignored_without_reloading_cursor → Find_node` | cross_community | 9 |
| `External_reload_clamps_cursor_and_selection_to_new_buffer_length → Find_node` | cross_community | 9 |
| `Scenario_insert_and_scroll → Find_node` | cross_community | 9 |
| `Update_markdown_preview_content → Normalize_modifier_alias` | cross_community | 8 |
| `Update_markdown_preview_content → New` | cross_community | 8 |
| `External_reload_error_does_not_abort_workspace_updates → Is_hidden_name` | cross_community | 8 |
| `File_picker_results_refresh_while_overlay_is_open_after_external_create → Is_hidden_name` | cross_community | 8 |
| `External_modify_reloads_when_clean_and_warns_when_dirty → Is_hidden_name` | cross_community | 8 |
| `Modify_event_on_missing_path_triggers_workspace_rescan_for_rename_like_flow → Is_hidden_name` | cross_community | 8 |

## Connected Areas

| Area | Connections |
|------|-------------|
| Command_dispatch | 35 calls |
| Workbench | 11 calls |
| Event_loop | 10 calls |
| App | 7 calls |
| Terminal | 3 calls |
| Text | 3 calls |
| Config | 2 calls |
| Benches | 2 calls |

## How to Explore

1. `gitnexus_context({name: "delete_current_line"})` — see callers and callees
2. `gitnexus_query({query: "app_state"})` — find related execution flows
3. Read key files listed above for implementation details
