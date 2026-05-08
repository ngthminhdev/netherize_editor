---
name: app-state
description: "Skill for the App_state area of netherize_editor. 400 symbols across 32 files."
---

# App_state

400 symbols | 32 files | Cohesion: 69%

## When to Use

- Working with code in `src/`
- Understanding how move_word_forward, move_word_end, delete_word_forward work
- Modifying app_state-related functionality

## Key Files

| File | Symbols |
|------|---------|
| `src/app/app_state/overlays.rs` | len_chars, preview, cursor_state, restore_cursor_state, snapshot_editor_view (+61) |
| `src/app/app_state/tests.rs` | unique_temp_path, text_edits_record_highlight_byte_deltas, backspace_between_empty_auto_pair_deletes_both_chars, backspace_between_empty_quotes_deletes_both_chars, backspace_between_empty_backticks_deletes_both_chars (+56) |
| `src/app/app_state/state.rs` | clipboard_record_kind_for_text, completion_prefix_info_at, begin_file_history_preview_session, cancel_file_history_preview, clear_search_highlights (+45) |
| `src/app/app_state/palette.rs` | active_buffer_is_fuzzy_picker, open_command_palette_mode, open_theme_selector_palette, open_document_symbols_palette_loading, close_command_palette (+40) |
| `src/app/app_state/editor.rs` | insert_tab, step_over_closing_char, insert_auto_pair, smart_insert_newline, backspace (+20) |
| `src/app/app_state/multi_cursor.rs` | multi_cursor_select_all_visual, multi_cursor_add_next, multi_cursor_skip, merge_overlapping_cursors, word_bounds_at_cursor (+18) |
| `src/app/app_state/buffers.rs` | clear_visual_selection, delete_char_text_at_cursor, substitute_current_line_text, delete_visual_selection, paste_after (+17) |
| `src/app/app_state/mod.rs` | from_bindings, build_help_sections, build_help_lines, command_label_for_help, append_help_binding (+9) |
| `src/editor_core.rs` | move_word_forward, move_word_end, delete_word_forward, change_word_forward, classify_char (+6) |
| `src/core/command_dispatch/common.rs` | write_text_to_clipboard, read_text_from_clipboard, remember_clipboard_text, write_text_to_clipboard_and_remember, normalize_palette_clipboard_text (+3) |

## Entry Points

Start here when exploring this area:

- **`move_word_forward`** (Function) — `src/editor_core.rs:137`
- **`move_word_end`** (Function) — `src/editor_core.rs:161`
- **`delete_word_forward`** (Function) — `src/editor_core.rs:340`
- **`change_word_forward`** (Function) — `src/editor_core.rs:377`
- **`delete`** (Function) — `src/syntax/highlight.rs:140`

## Key Symbols

| Symbol | Type | File | Line |
|--------|------|------|------|
| `move_word_forward` | Function | `src/editor_core.rs` | 137 |
| `move_word_end` | Function | `src/editor_core.rs` | 161 |
| `delete_word_forward` | Function | `src/editor_core.rs` | 340 |
| `change_word_forward` | Function | `src/editor_core.rs` | 377 |
| `delete` | Function | `src/syntax/highlight.rs` | 140 |
| `len_chars` | Function | `src/app/app_state/overlays.rs` | 94 |
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
| `delete_word_forward_range` | Function | `src/app/app_state/overlays.rs` | 384 |

## Execution Flows

| Flow | Type | Steps |
|------|------|-------|
| `Save_file_preserves_cursor_and_selection_state → Find_node` | cross_community | 9 |
| `Self_save_modify_event_is_ignored_without_reloading_cursor → Find_node` | cross_community | 9 |
| `External_reload_clamps_cursor_and_selection_to_new_buffer_length → Find_node` | cross_community | 9 |
| `Update_markdown_preview_content → Normalize_modifier_alias` | cross_community | 8 |
| `Update_markdown_preview_content → New` | cross_community | 8 |
| `External_reload_error_does_not_abort_workspace_updates → Is_hidden_name` | cross_community | 8 |
| `File_picker_results_refresh_while_overlay_is_open_after_external_create → Is_hidden_name` | cross_community | 8 |
| `External_modify_reloads_when_clean_and_warns_when_dirty → Is_hidden_name` | cross_community | 8 |
| `Modify_event_on_missing_path_triggers_workspace_rescan_for_rename_like_flow → Is_hidden_name` | cross_community | 8 |
| `Workspace_and_file_picker_state_are_tracked → Is_hidden_name` | cross_community | 8 |

## Connected Areas

| Area | Connections |
|------|-------------|
| Command_dispatch | 58 calls |
| Benches | 14 calls |
| Editor | 11 calls |
| Workbench | 10 calls |
| Cluster_3 | 6 calls |
| App | 4 calls |
| Event_loop | 3 calls |
| Ui | 2 calls |

## How to Explore

1. `gitnexus_context({name: "move_word_forward"})` — see callers and callees
2. `gitnexus_query({query: "app_state"})` — find related execution flows
3. Read key files listed above for implementation details
