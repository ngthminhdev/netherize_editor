---
name: app
description: "Skill for the App area of netherize_editor. 106 symbols across 15 files."
---

# App

106 symbols | 15 files | Cohesion: 75%

## When to Use

- Working with code in `src/`
- Understanding how parse_key_sequence, parse_key_spec, apply_overrides work
- Modifying app-related functionality

## Key Files

| File | Symbols |
|------|---------|
| `src/app/command_palette.rs` | prompt_prefix, empty_hint, title, is_complex_picker, render (+33) |
| `src/app/resolved_keymap.rs` | parse_key_sequence, parse_key_spec, new, insert_sequence, apply_overrides (+16) |
| `src/app/file_picker.rs` | default, open, append_query, backspace_query, select_next (+6) |
| `src/app/match_ranges.rs` | compute_label_match_ranges, score_label_match, build_lowercase_byte_map, map_lower_range_to_original, push_match_range (+2) |
| `src/app/async_bridge.rs` | new, bridge_counts_failed_event_in_summary, lsp_diagnostics_bypass_stale_revision_filter, bridge_tracks_multiple_worker_failure_events, pump (+2) |
| `src/app/persistence.rs` | most_recent_existing, configured_theme_profile, state_path, load_from_path, load (+1) |
| `src/app/clipboard.rs` | new, ensure_initialized, clipboard_mut, get_text, set_text |
| `src/app/event_loop/setup.rs` | new, new_with_scheduler, pump_bridge |
| `src/app/event_loop/commands_prompts.rs` | confirm_explorer_prompt, resolve_explorer_creation_target |
| `src/app/event_loop/async_results.rs` | read_file_preview |

## Entry Points

Start here when exploring this area:

- **`parse_key_sequence`** (Function) — `src/app/resolved_keymap.rs:143`
- **`parse_key_spec`** (Function) — `src/app/resolved_keymap.rs:154`
- **`apply_overrides`** (Function) — `src/app/resolved_keymap.rs:366`
- **`from_bindings`** (Function) — `src/app/resolved_keymap.rs:464`
- **`builtin_defaults`** (Function) — `src/app/resolved_keymap.rs:529`

## Key Symbols

| Symbol | Type | File | Line |
|--------|------|------|------|
| `parse_key_sequence` | Function | `src/app/resolved_keymap.rs` | 143 |
| `parse_key_spec` | Function | `src/app/resolved_keymap.rs` | 154 |
| `apply_overrides` | Function | `src/app/resolved_keymap.rs` | 366 |
| `from_bindings` | Function | `src/app/resolved_keymap.rs` | 464 |
| `builtin_defaults` | Function | `src/app/resolved_keymap.rs` | 529 |
| `build` | Function | `src/app/resolved_keymap.rs` | 924 |
| `open` | Function | `src/app/file_picker.rs` | 48 |
| `append_query` | Function | `src/app/file_picker.rs` | 65 |
| `backspace_query` | Function | `src/app/file_picker.rs` | 76 |
| `select_next` | Function | `src/app/file_picker.rs` | 87 |
| `select_prev` | Function | `src/app/file_picker.rs` | 97 |
| `selected_path` | Function | `src/app/file_picker.rs` | 107 |
| `refresh_from_workspace` | Function | `src/app/file_picker.rs` | 119 |
| `confirm_explorer_prompt` | Function | `src/app/event_loop/commands_prompts.rs` | 129 |
| `resolve_explorer_creation_target` | Function | `src/app/event_loop/commands_prompts.rs` | 229 |
| `load_image_buffer` | Function | `src/app/app_state/overlays.rs` | 1370 |
| `prompt_prefix` | Function | `src/app/command_palette.rs` | 54 |
| `empty_hint` | Function | `src/app/command_palette.rs` | 78 |
| `title` | Function | `src/app/command_palette.rs` | 102 |
| `is_complex_picker` | Function | `src/app/command_palette.rs` | 127 |

## Execution Flows

| Flow | Type | Steps |
|------|------|-------|
| `Save_file_preserves_cursor_and_selection_state → Find_node` | cross_community | 9 |
| `Self_save_modify_event_is_ignored_without_reloading_cursor → Find_node` | cross_community | 9 |
| `External_reload_clamps_cursor_and_selection_to_new_buffer_length → Find_node` | cross_community | 9 |
| `Scenario_insert_and_scroll → Find_node` | cross_community | 9 |
| `Update_markdown_preview_content → Normalize_modifier_alias` | cross_community | 8 |
| `Update_markdown_preview_content → New` | cross_community | 8 |
| `Picker_open_query_select_flow → Find_node` | cross_community | 7 |
| `Save_file_preserves_cursor_and_selection_state → Replace` | cross_community | 7 |
| `Save_file_preserves_cursor_and_selection_state → WorkspaceMatch` | cross_community | 7 |
| `Self_save_modify_event_is_ignored_without_reloading_cursor → Replace` | cross_community | 7 |

## Connected Areas

| Area | Connections |
|------|-------------|
| Theme_config | 5 calls |
| App_state | 4 calls |
| Config | 3 calls |
| Workbench | 3 calls |
| Terminal | 3 calls |
| Workspace | 2 calls |
| Command_dispatch | 2 calls |
| Event_loop | 1 calls |

## How to Explore

1. `gitnexus_context({name: "parse_key_sequence"})` — see callers and callees
2. `gitnexus_query({query: "app"})` — find related execution flows
3. Read key files listed above for implementation details
