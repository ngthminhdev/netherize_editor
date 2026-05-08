---
name: input
description: "Skill for the Input area of netherize_editor. 101 symbols across 9 files."
---

# Input

101 symbols | 9 files | Cohesion: 91%

## When to Use

- Working with code in `src/`
- Understanding how supports_press_and_hold_repeat, for_mode, with_focus work
- Modifying input-related functionality

## Key Files

| File | Symbols |
|------|---------|
| `src/app/input/tests.rs` | char_input, ctrl_input, named_input, shift_named_input, completion_context (+58) |
| `src/app/input/handler.rs` | accumulate_pending_count, clear_pending_counts, consume_repeat_count_for_command, translate_dispatch, translate_key_event (+10) |
| `src/app/input/pending.rs` | classify_pending_state, uses_operator_count, generate_leap_labels, generate_leap_labels_returns_single_chars_up_to_twenty_six, generate_leap_labels_uses_hybrid_boundary_after_fast_group (+2) |
| `src/app/input/helpers.rs` | is_modifier_only_key, replace_char_from_input, printable_char_from_input, terminal_input_payload, shifted_letter_payload (+1) |
| `src/app/input_map/mod.rs` | for_mode, with_focus, with_keymap |
| `src/app/event_loop/commands_editor.rs` | handle_leap_command, normalize_leap_target, generate_editor_leap_state |
| `src/app/input/model.rs` | from_key_event, debug_label |
| `src/core/commands.rs` | supports_press_and_hold_repeat |
| `src/app/input_map/tests.rs` | table_driven_keybinding_resolution |

## Entry Points

Start here when exploring this area:

- **`supports_press_and_hold_repeat`** (Function) — `src/core/commands.rs:443`
- **`for_mode`** (Function) — `src/app/input_map/mod.rs:101`
- **`with_focus`** (Function) — `src/app/input_map/mod.rs:126`
- **`with_keymap`** (Function) — `src/app/input_map/mod.rs:171`
- **`classify_pending_state`** (Function) — `src/app/input/pending.rs:97`

## Key Symbols

| Symbol | Type | File | Line |
|--------|------|------|------|
| `supports_press_and_hold_repeat` | Function | `src/core/commands.rs` | 443 |
| `for_mode` | Function | `src/app/input_map/mod.rs` | 101 |
| `with_focus` | Function | `src/app/input_map/mod.rs` | 126 |
| `with_keymap` | Function | `src/app/input_map/mod.rs` | 171 |
| `classify_pending_state` | Function | `src/app/input/pending.rs` | 97 |
| `uses_operator_count` | Function | `src/app/input/pending.rs` | 117 |
| `from_key_event` | Function | `src/app/input/model.rs` | 18 |
| `debug_label` | Function | `src/app/input/model.rs` | 46 |
| `is_modifier_only_key` | Function | `src/app/input/helpers.rs` | 13 |
| `replace_char_from_input` | Function | `src/app/input/helpers.rs` | 272 |
| `printable_char_from_input` | Function | `src/app/input/helpers.rs` | 288 |
| `terminal_input_payload` | Function | `src/app/input/helpers.rs` | 299 |
| `translate_key_event` | Function | `src/app/input/handler.rs` | 180 |
| `route_repeated_normalized_input` | Function | `src/app/input/handler.rs` | 197 |
| `route_normalized_input` | Function | `src/app/input/handler.rs` | 260 |
| `generate_leap_labels` | Function | `src/app/input/pending.rs` | 28 |
| `handle_leap_command` | Function | `src/app/event_loop/commands_editor.rs` | 96 |
| `normalize_leap_target` | Function | `src/app/event_loop/commands_editor.rs` | 312 |
| `generate_editor_leap_state` | Function | `src/app/event_loop/commands_editor.rs` | 320 |
| `on_focus_changed` | Function | `src/app/input/handler.rs` | 51 |

## Execution Flows

| Flow | Type | Steps |
|------|------|-------|
| `Table_driven_keybinding_resolution → SequenceBindingKey` | cross_community | 5 |
| `Table_driven_keybinding_resolution → New` | cross_community | 4 |

## Connected Areas

| Area | Connections |
|------|-------------|
| Input_map | 17 calls |
| App | 2 calls |
| Config | 1 calls |
| Command_dispatch | 1 calls |
| Workbench | 1 calls |

## How to Explore

1. `gitnexus_context({name: "supports_press_and_hold_repeat"})` — see callers and callees
2. `gitnexus_query({query: "input"})` — find related execution flows
3. Read key files listed above for implementation details
