---
name: config
description: "Skill for the Config area of netherize_editor. 44 symbols across 9 files."
---

# Config

44 symbols | 9 files | Cohesion: 76%

## When to Use

- Working with code in `src/`
- Understanding how user_config_root, legacy_app_state_root, load work
- Modifying config-related functionality

## Key Files

| File | Symbols |
|------|---------|
| `src/config/ui_config.rs` | default, builtin, from_raw, validate, parse_positive_f32 (+11) |
| `src/config/keymap_loader.rs` | default_user_overrides_path, load, find_profile_path, load_validated_file, invalid_toml_returns_none (+3) |
| `src/config/ai_config.rs` | load, candidate_paths, debounce_ms, prefix_chars, suffix_chars (+1) |
| `src/config/theme_config/loader.rs` | list_available_themes, list_available_theme_entries, find_profile_path, theme_search_dirs, list_available_theme_entries_in_dir |
| `src/config/paths.rs` | home_dir, user_config_root, legacy_app_state_root |
| `src/app/event_loop/setup.rs` | flush_pending_ai_inline_completion, next_ai_inline_flush_deadline |
| `src/app/input_map/mod.rs` | new, default |
| `src/app/persistence.rs` | state_dir |
| `src/core/command_ids.rs` | is_valid |

## Entry Points

Start here when exploring this area:

- **`user_config_root`** (Function) — `src/config/paths.rs:18`
- **`legacy_app_state_root`** (Function) — `src/config/paths.rs:30`
- **`load`** (Function) — `src/config/ai_config.rs:30`
- **`list_available_themes`** (Function) — `src/config/theme_config/loader.rs:45`
- **`list_available_theme_entries`** (Function) — `src/config/theme_config/loader.rs:52`

## Key Symbols

| Symbol | Type | File | Line |
|--------|------|------|------|
| `user_config_root` | Function | `src/config/paths.rs` | 18 |
| `legacy_app_state_root` | Function | `src/config/paths.rs` | 30 |
| `load` | Function | `src/config/ai_config.rs` | 30 |
| `list_available_themes` | Function | `src/config/theme_config/loader.rs` | 45 |
| `list_available_theme_entries` | Function | `src/config/theme_config/loader.rs` | 52 |
| `builtin` | Function | `src/config/ui_config.rs` | 177 |
| `validate` | Function | `src/config/ui_config.rs` | 661 |
| `load` | Function | `src/config/keymap_loader.rs` | 35 |
| `is_valid` | Function | `src/core/command_ids.rs` | 406 |
| `debounce_ms` | Function | `src/config/ai_config.rs` | 49 |
| `prefix_chars` | Function | `src/config/ai_config.rs` | 53 |
| `suffix_chars` | Function | `src/config/ai_config.rs` | 57 |
| `max_tokens` | Function | `src/config/ai_config.rs` | 61 |
| `flush_pending_ai_inline_completion` | Function | `src/app/event_loop/setup.rs` | 1018 |
| `next_ai_inline_flush_deadline` | Function | `src/app/event_loop/setup.rs` | 1069 |
| `active_profile` | Function | `src/config/ui_config.rs` | 143 |
| `load_active` | Function | `src/config/ui_config.rs` | 147 |
| `load` | Function | `src/config/ui_config.rs` | 162 |
| `load_from_path` | Function | `src/config/ui_config.rs` | 168 |
| `load_user_editor_overrides` | Function | `src/config/ui_config.rs` | 628 |

## Execution Flows

| Flow | Type | Steps |
|------|------|-------|
| `Bench_edit_loop_latency → Find_profile_path` | cross_community | 7 |
| `Bench_edit_loop_latency → Active_profile` | cross_community | 6 |
| `Update_markdown_preview_content → From_str` | cross_community | 6 |
| `Update_markdown_preview_content → Is_valid` | cross_community | 6 |
| `Load → Home_dir` | cross_community | 6 |
| `Terminal_normal_selection_yanks_terminal_grid_and_returns_to_typing_mode → Active_profile` | cross_community | 6 |
| `Terminal_normal_selection_yanks_terminal_grid_and_returns_to_typing_mode → Find_profile_path` | cross_community | 6 |
| `Update_ai_chat_content → Find_profile_path` | cross_community | 5 |
| `Update_markdown_preview_content → Find_profile_path` | cross_community | 5 |
| `Load_active → WindowUiConfig` | cross_community | 5 |

## Connected Areas

| Area | Connections |
|------|-------------|
| Scheduler | 2 calls |
| Workbench | 1 calls |
| App | 1 calls |
| Event_loop | 1 calls |

## How to Explore

1. `gitnexus_context({name: "user_config_root"})` — see callers and callees
2. `gitnexus_query({query: "config"})` — find related execution flows
3. Read key files listed above for implementation details
