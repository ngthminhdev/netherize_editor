---
name: input-map
description: "Skill for the Input_map area of netherize_editor. 79 symbols across 9 files."
---

# Input_map

79 symbols | 9 files | Cohesion: 70%

## When to Use

- Working with code in `src/`
- Understanding how matches, is_leader_input, lookup_mode_only work
- Modifying input_map-related functionality

## Key Files

| File | Symbols |
|------|---------|
| `src/app/input_map/tests.rs` | make_default_profile_map, input_from_named, input_from_physical, default_profile_leader_f_m_routes_to_lsp_format_document, default_profile_leader_m_f_routes_to_focus_markdown_preview (+26) |
| `src/app/resolved_keymap.rs` | matches, is_leader_input, lookup_mode_only, lookup_global, input_to_specs (+9) |
| `src/app/input_map/focus.rs` | resolve_settings_focus, resolve_diagnostics_focus, resolve_references_focus, resolve_explorer_focus, resolve_inspector_focus (+6) |
| `src/app/input_map/mod.rs` | resolve_sequence_start, resolve_sequence_next, resolve, translate, allows_leader (+5) |
| `src/app/input/helpers.rs` | numeric_count_digit_from_input, should_start_replace_pending, should_start_yank_pending, inner_or_around_from_input, text_object_kind_from_input (+2) |
| `src/app/input_map/helpers.rs` | palette_query_from_text, insert_command_from_text |
| `src/app/input/tests.rs` | ime_commit_is_redirected_to_file_picker_when_palette_is_open, ime_commit_is_redirected_to_ai_chat_text |
| `src/app/input/model.rs` | has_command_modifier |
| `src/app/input/handler.rs` | translate_ime_commit |

## Entry Points

Start here when exploring this area:

- **`matches`** (Function) — `src/app/resolved_keymap.rs:33`
- **`is_leader_input`** (Function) — `src/app/resolved_keymap.rs:284`
- **`lookup_mode_only`** (Function) — `src/app/resolved_keymap.rs:395`
- **`lookup_global`** (Function) — `src/app/resolved_keymap.rs:410`
- **`resolve_command_mode_only`** (Function) — `src/app/resolved_keymap.rs:944`

## Key Symbols

| Symbol | Type | File | Line |
|--------|------|------|------|
| `matches` | Function | `src/app/resolved_keymap.rs` | 33 |
| `is_leader_input` | Function | `src/app/resolved_keymap.rs` | 284 |
| `lookup_mode_only` | Function | `src/app/resolved_keymap.rs` | 395 |
| `lookup_global` | Function | `src/app/resolved_keymap.rs` | 410 |
| `resolve_command_mode_only` | Function | `src/app/resolved_keymap.rs` | 944 |
| `resolve_global_command` | Function | `src/app/resolved_keymap.rs` | 954 |
| `palette_query_from_text` | Function | `src/app/input_map/helpers.rs` | 13 |
| `resolve_settings_focus` | Function | `src/app/input_map/focus.rs` | 6 |
| `resolve_diagnostics_focus` | Function | `src/app/input_map/focus.rs` | 121 |
| `resolve_references_focus` | Function | `src/app/input_map/focus.rs` | 175 |
| `resolve_explorer_focus` | Function | `src/app/input_map/focus.rs` | 229 |
| `resolve_inspector_focus` | Function | `src/app/input_map/focus.rs` | 312 |
| `resolve_markdown_preview_focus` | Function | `src/app/input_map/focus.rs` | 364 |
| `resolve_help_focus` | Function | `src/app/input_map/focus.rs` | 441 |
| `resolve_bottom_panel_focus` | Function | `src/app/input_map/focus.rs` | 497 |
| `resolve_palette_focus` | Function | `src/app/input_map/focus.rs` | 549 |
| `resolve_fuzzy_picker_focus` | Function | `src/app/input_map/focus.rs` | 707 |
| `has_command_modifier` | Function | `src/app/input/model.rs` | 42 |
| `numeric_count_digit_from_input` | Function | `src/app/input/helpers.rs` | 55 |
| `should_start_replace_pending` | Function | `src/app/input/helpers.rs` | 104 |

## Execution Flows

| Flow | Type | Steps |
|------|------|-------|
| `Table_driven_keybinding_resolution → SequenceBindingKey` | cross_community | 5 |
| `Resolve_settings_focus → Has_command_modifier` | intra_community | 5 |
| `Resolve_explorer_focus → Has_command_modifier` | intra_community | 5 |
| `Resolve_palette_focus → Has_command_modifier` | intra_community | 5 |
| `Resolve_fuzzy_picker_focus → Has_command_modifier` | intra_community | 5 |
| `Default_profile_leader_f_m_routes_to_lsp_format_document → From_str` | cross_community | 5 |
| `Default_profile_leader_f_m_routes_to_lsp_format_document → Is_valid` | cross_community | 5 |
| `Default_profile_leader_f_m_routes_to_lsp_format_document → New` | cross_community | 5 |
| `Default_profile_leader_f_m_routes_to_lsp_format_document → Allows_leader` | cross_community | 5 |
| `Default_profile_leader_f_m_routes_to_lsp_format_document → Editor_mode_str` | cross_community | 5 |

## Connected Areas

| Area | Connections |
|------|-------------|
| Input | 22 calls |
| App | 5 calls |
| Syntax | 4 calls |
| Config | 1 calls |

## How to Explore

1. `gitnexus_context({name: "matches"})` — see callers and callees
2. `gitnexus_query({query: "input_map"})` — find related execution flows
3. Read key files listed above for implementation details
