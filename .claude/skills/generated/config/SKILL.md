---
name: config
description: "Skill for the Config area of netherize_editor. 41 symbols across 8 files."
---

# Config

41 symbols | 8 files | Cohesion: 72%

## When to Use

- Working with code in `src/`
- Understanding how builtin, validate, is_valid work
- Modifying config-related functionality

## Key Files

| File | Symbols |
|------|---------|
| `src/config/ui_config.rs` | default, builtin, from_raw, validate, parse_positive_f32 (+11) |
| `src/config/keymap_loader.rs` | load, find_profile_path, load_validated_file, invalid_toml_returns_none, invalid_command_id_is_skipped (+3) |
| `src/config/ai_config.rs` | debounce_ms, prefix_chars, suffix_chars, max_tokens, load (+1) |
| `src/config/paths.rs` | user_config_root, home_dir, legacy_app_state_root |
| `src/app/persistence.rs` | state_dir, legacy_state_dir, legacy_state_path |
| `src/app/event_loop/setup.rs` | flush_pending_ai_inline_completion, next_ai_inline_flush_deadline |
| `src/app/input_map/mod.rs` | new, default |
| `src/core/command_ids.rs` | is_valid |

## Entry Points

Start here when exploring this area:

- **`builtin`** (Function) — `src/config/ui_config.rs:177`
- **`validate`** (Function) — `src/config/ui_config.rs:661`
- **`is_valid`** (Function) — `src/core/command_ids.rs:392`
- **`load`** (Function) — `src/config/keymap_loader.rs:35`
- **`debounce_ms`** (Function) — `src/config/ai_config.rs:49`

## Key Symbols

| Symbol | Type | File | Line |
|--------|------|------|------|
| `builtin` | Function | `src/config/ui_config.rs` | 177 |
| `validate` | Function | `src/config/ui_config.rs` | 661 |
| `is_valid` | Function | `src/core/command_ids.rs` | 392 |
| `load` | Function | `src/config/keymap_loader.rs` | 35 |
| `debounce_ms` | Function | `src/config/ai_config.rs` | 49 |
| `prefix_chars` | Function | `src/config/ai_config.rs` | 53 |
| `suffix_chars` | Function | `src/config/ai_config.rs` | 57 |
| `max_tokens` | Function | `src/config/ai_config.rs` | 61 |
| `flush_pending_ai_inline_completion` | Function | `src/app/event_loop/setup.rs` | 952 |
| `next_ai_inline_flush_deadline` | Function | `src/app/event_loop/setup.rs` | 1003 |
| `active_profile` | Function | `src/config/ui_config.rs` | 143 |
| `load_active` | Function | `src/config/ui_config.rs` | 147 |
| `load` | Function | `src/config/ui_config.rs` | 162 |
| `load_from_path` | Function | `src/config/ui_config.rs` | 168 |
| `user_config_root` | Function | `src/config/paths.rs` | 18 |
| `load` | Function | `src/config/ai_config.rs` | 30 |
| `load_user_editor_overrides` | Function | `src/config/ui_config.rs` | 628 |
| `user_override_path` | Function | `src/config/ui_config.rs` | 643 |
| `save_user_override` | Function | `src/config/ui_config.rs` | 647 |
| `legacy_app_state_root` | Function | `src/config/paths.rs` | 30 |

## Execution Flows

| Flow | Type | Steps |
|------|------|-------|
| `Bench_edit_loop_latency → Find_profile_path` | cross_community | 7 |
| `Bench_edit_loop_latency → Active_profile` | cross_community | 6 |
| `Load → Home_dir` | cross_community | 6 |
| `Terminal_normal_selection_yanks_terminal_grid_and_returns_to_typing_mode → Active_profile` | cross_community | 6 |
| `Update_ai_chat_content → Find_profile_path` | cross_community | 5 |
| `Load_active → WindowUiConfig` | cross_community | 5 |
| `Load_active → Parse_positive_u32` | cross_community | 5 |
| `Load_active → Parse_positive_f32` | cross_community | 5 |
| `Run_local_history_request → Home_dir` | cross_community | 5 |
| `Default_profile_leader_f_m_routes_to_lsp_format_document → From_str` | cross_community | 5 |

## Connected Areas

| Area | Connections |
|------|-------------|
| Scheduler | 2 calls |
| App | 1 calls |
| Event_loop | 1 calls |

## How to Explore

1. `gitnexus_context({name: "builtin"})` — see callers and callees
2. `gitnexus_query({query: "config"})` — find related execution flows
3. Read key files listed above for implementation details
