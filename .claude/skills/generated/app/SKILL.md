---
name: app
description: "Skill for the App area of netherize_editor. 135 symbols across 20 files."
---

# App

135 symbols | 20 files | Cohesion: 77%

## When to Use

- Working with code in `src/`
- Understanding how parse, matches, is_leader_input work
- Modifying app-related functionality

## Key Files

| File | Symbols |
|------|---------|
| `src/app/command_palette.rs` | command, open, append_query, backspace_query, selected_action (+33) |
| `src/app/resolved_keymap.rs` | matches, is_leader_input, lookup, lookup_mode_only, lookup_global (+26) |
| `src/app/file_picker.rs` | default, open, append_query, backspace_query, select_next (+6) |
| `src/app/input_map/focus.rs` | resolve_settings_focus, resolve_diagnostics_focus, resolve_references_focus, resolve_explorer_focus, resolve_inspector_focus (+4) |
| `src/app/input/helpers.rs` | numeric_count_digit_from_input, should_start_replace_pending, should_start_yank_pending, inner_or_around_from_input, text_object_kind_from_input (+2) |
| `src/app/match_ranges.rs` | compute_label_match_ranges, score_label_match, build_lowercase_byte_map, map_lower_range_to_original, push_match_range (+2) |
| `src/app/async_bridge.rs` | new, bridge_discards_stale_result_when_old_revision_arrives_last, bridge_accepts_same_revision_result, bridge_tracks_multiple_worker_failure_events, pump (+2) |
| `src/app/persistence.rs` | most_recent_existing, configured_theme_profile, state_path, load_from_path, load (+1) |
| `src/app/clipboard.rs` | new, ensure_initialized, clipboard_mut, get_text, set_text |
| `src/app/event_loop/setup.rs` | new, new_with_scheduler, pump_bridge |

## Entry Points

Start here when exploring this area:

- **`parse`** (Function) — `src/core/command_ids.rs:370`
- **`matches`** (Function) — `src/app/resolved_keymap.rs:33`
- **`is_leader_input`** (Function) — `src/app/resolved_keymap.rs:284`
- **`lookup`** (Function) — `src/app/resolved_keymap.rs:371`
- **`lookup_mode_only`** (Function) — `src/app/resolved_keymap.rs:393`

## Key Symbols

| Symbol | Type | File | Line |
|--------|------|------|------|
| `parse` | Function | `src/core/command_ids.rs` | 370 |
| `matches` | Function | `src/app/resolved_keymap.rs` | 33 |
| `is_leader_input` | Function | `src/app/resolved_keymap.rs` | 284 |
| `lookup` | Function | `src/app/resolved_keymap.rs` | 371 |
| `lookup_mode_only` | Function | `src/app/resolved_keymap.rs` | 393 |
| `lookup_global` | Function | `src/app/resolved_keymap.rs` | 408 |
| `resolve_command` | Function | `src/app/resolved_keymap.rs` | 897 |
| `resolve_command_mode_only` | Function | `src/app/resolved_keymap.rs` | 907 |
| `resolve_global_command` | Function | `src/app/resolved_keymap.rs` | 917 |
| `palette_query_from_text` | Function | `src/app/input_map/helpers.rs` | 13 |
| `resolve_settings_focus` | Function | `src/app/input_map/focus.rs` | 6 |
| `resolve_diagnostics_focus` | Function | `src/app/input_map/focus.rs` | 121 |
| `resolve_references_focus` | Function | `src/app/input_map/focus.rs` | 175 |
| `resolve_explorer_focus` | Function | `src/app/input_map/focus.rs` | 229 |
| `resolve_inspector_focus` | Function | `src/app/input_map/focus.rs` | 312 |
| `resolve_markdown_preview_focus` | Function | `src/app/input_map/focus.rs` | 364 |
| `resolve_bottom_panel_focus` | Function | `src/app/input_map/focus.rs` | 431 |
| `resolve_palette_focus` | Function | `src/app/input_map/focus.rs` | 483 |
| `resolve_fuzzy_picker_focus` | Function | `src/app/input_map/focus.rs` | 642 |
| `has_command_modifier` | Function | `src/app/input/model.rs` | 42 |

## Execution Flows

| Flow | Type | Steps |
|------|------|-------|
| `Save_file_preserves_cursor_and_selection_state → Find_node` | cross_community | 9 |
| `Self_save_modify_event_is_ignored_without_reloading_cursor → Find_node` | cross_community | 9 |
| `External_reload_clamps_cursor_and_selection_to_new_buffer_length → Find_node` | cross_community | 9 |
| `Scenario_insert_and_scroll → Find_node` | cross_community | 9 |
| `Picker_open_query_select_flow → Find_node` | cross_community | 7 |
| `Save_file_preserves_cursor_and_selection_state → Replace` | cross_community | 7 |
| `Save_file_preserves_cursor_and_selection_state → WorkspaceMatch` | cross_community | 7 |
| `Self_save_modify_event_is_ignored_without_reloading_cursor → Replace` | cross_community | 7 |
| `Self_save_modify_event_is_ignored_without_reloading_cursor → WorkspaceMatch` | cross_community | 7 |
| `External_reload_clamps_cursor_and_selection_to_new_buffer_length → Replace` | cross_community | 7 |

## Connected Areas

| Area | Connections |
|------|-------------|
| Theme_config | 5 calls |
| App_state | 5 calls |
| Renderer | 4 calls |
| Config | 3 calls |
| Workbench | 3 calls |
| Workspace | 2 calls |
| Event_loop | 1 calls |
| Command_dispatch | 1 calls |

## How to Explore

1. `gitnexus_context({name: "parse"})` — see callers and callees
2. `gitnexus_query({query: "app"})` — find related execution flows
3. Read key files listed above for implementation details
