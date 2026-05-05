---
name: input-map
description: "Skill for the Input_map area of netherize_editor. 49 symbols across 7 files."
---

# Input_map

49 symbols | 7 files | Cohesion: 63%

## When to Use

- Working with code in `src/`
- Understanding how resolve_sequence_start, resolve_sequence_next, editor_mode_str work
- Modifying input_map-related functionality

## Key Files

| File | Symbols |
|------|---------|
| `src/app/input_map/tests.rs` | make_default_profile_map, input_from_named, input_from_physical, default_profile_leader_f_m_routes_to_lsp_format_document, welcome_explorer_focus_routes_jk_to_recent_project_selection (+25) |
| `src/app/input_map/mod.rs` | resolve_sequence_start, resolve_sequence_next, allows_leader, resolve_sequence_from_steps, context_allows_leader_sequence (+5) |
| `src/app/resolved_keymap.rs` | editor_mode_str, lookup_sequence, sequence_step_candidates, builtin_defaults_include_expected_static_chords |
| `src/app/input/tests.rs` | ime_commit_is_redirected_to_file_picker_when_palette_is_open, ime_commit_is_redirected_to_ai_chat_text |
| `src/app/input_map/focus.rs` | resolve_terminal_focus |
| `src/app/input_map/helpers.rs` | insert_command_from_text |
| `src/app/input/handler.rs` | translate_ime_commit |

## Entry Points

Start here when exploring this area:

- **`resolve_sequence_start`** (Function) — `src/app/input_map/mod.rs:297`
- **`resolve_sequence_next`** (Function) — `src/app/input_map/mod.rs:305`
- **`editor_mode_str`** (Function) — `src/app/resolved_keymap.rs:291`
- **`lookup_sequence`** (Function) — `src/app/resolved_keymap.rs:421`
- **`sequence_step_candidates`** (Function) — `src/app/resolved_keymap.rs:513`

## Key Symbols

| Symbol | Type | File | Line |
|--------|------|------|------|
| `resolve_sequence_start` | Function | `src/app/input_map/mod.rs` | 297 |
| `resolve_sequence_next` | Function | `src/app/input_map/mod.rs` | 305 |
| `editor_mode_str` | Function | `src/app/resolved_keymap.rs` | 291 |
| `lookup_sequence` | Function | `src/app/resolved_keymap.rs` | 421 |
| `sequence_step_candidates` | Function | `src/app/resolved_keymap.rs` | 513 |
| `allows_leader` | Function | `src/app/input_map/mod.rs` | 68 |
| `resolve_terminal_focus` | Function | `src/app/input_map/focus.rs` | 756 |
| `resolve` | Function | `src/app/input_map/mod.rs` | 176 |
| `translate` | Function | `src/app/input_map/mod.rs` | 396 |
| `insert_command_from_text` | Function | `src/app/input_map/helpers.rs` | 2 |
| `for_mode_with_palette` | Function | `src/app/input_map/mod.rs` | 110 |
| `for_mode_with_picker` | Function | `src/app/input_map/mod.rs` | 116 |
| `translate_ime_commit` | Function | `src/app/input/handler.rs` | 1189 |
| `make_default_profile_map` | Function | `src/app/input_map/tests.rs` | 22 |
| `input_from_named` | Function | `src/app/input_map/tests.rs` | 27 |
| `input_from_physical` | Function | `src/app/input_map/tests.rs` | 36 |
| `default_profile_leader_f_m_routes_to_lsp_format_document` | Function | `src/app/input_map/tests.rs` | 647 |
| `welcome_explorer_focus_routes_jk_to_recent_project_selection` | Function | `src/app/input_map/tests.rs` | 736 |
| `explorer_focus_jk_still_use_explorer_commands_outside_welcome` | Function | `src/app/input_map/tests.rs` | 757 |
| `palette_focus_empty_welcome_routes_selection_without_visible_overlay` | Function | `src/app/input_map/tests.rs` | 810 |

## Execution Flows

| Flow | Type | Steps |
|------|------|-------|
| `Welcome_context_can_open_recent_projects_with_leader_sequence → Normalize_modifier_alias` | cross_community | 7 |
| `Welcome_context_can_open_recent_projects_with_leader_sequence → SequenceBindingKey` | cross_community | 6 |
| `Welcome_context_can_open_recent_projects_with_leader_sequence → New` | cross_community | 6 |
| `Leader_f_w_sequence_maps_to_search_in_files → Has_command_modifier` | cross_community | 6 |
| `Leader_space_x_maps_to_close_current_buffer → Has_command_modifier` | cross_community | 6 |
| `Table_driven_keybinding_resolution → SequenceBindingKey` | cross_community | 5 |
| `Default_profile_leader_f_m_routes_to_lsp_format_document → From_str` | cross_community | 5 |
| `Default_profile_leader_f_m_routes_to_lsp_format_document → Is_valid` | cross_community | 5 |
| `Default_profile_leader_f_m_routes_to_lsp_format_document → New` | cross_community | 5 |
| `Default_profile_leader_f_m_routes_to_lsp_format_document → Allows_leader` | cross_community | 5 |

## Connected Areas

| Area | Connections |
|------|-------------|
| Input | 21 calls |
| App | 10 calls |
| Config | 1 calls |

## How to Explore

1. `gitnexus_context({name: "resolve_sequence_start"})` — see callers and callees
2. `gitnexus_query({query: "input_map"})` — find related execution flows
3. Read key files listed above for implementation details
