---
name: theme-config
description: "Skill for the Theme_config area of netherize_editor. 55 symbols across 9 files."
---

# Theme_config

55 symbols | 9 files | Cohesion: 76%

## When to Use

- Working with code in `src/`
- Understanding how from_rgba_u8, builtin_dark, default_profile work
- Modifying theme_config-related functionality

## Key Files

| File | Symbols |
|------|---------|
| `src/config/theme_config/loader.rs` | from_raw, parse_extension_file_icons, parse_exact_file_icons, parse_editor, parse_ui (+17) |
| `src/config/theme_config/model.rs` | from_rgba_u8, file_icon_lookup_prefers_dir_then_exact_then_extension_then_default, new, to_wgpu, as_srgb_f32 (+12) |
| `src/config/theme_config/builtin.rs` | builtin_dark, builtin_editor_tokens, builtin_ui_tokens, builtin_syntax_tokens, builtin_git_tokens (+4) |
| `src/render/renderer/helpers.rs` | theme_color_to_wgpu, ext_icon_dot |
| `src/app/command_palette.rs` | theme |
| `src/core/command_dispatch/palette.rs` | open_theme_selector |
| `src/app/app_state/palette.rs` | open_theme_selector_palette |
| `src/render/renderer/lifecycle.rs` | apply_theme |
| `src/text/text_system.rs` | rgba_f32_from_color |

## Entry Points

Start here when exploring this area:

- **`from_rgba_u8`** (Function) — `src/config/theme_config/model.rs:35`
- **`builtin_dark`** (Function) — `src/config/theme_config/builtin.rs:8`
- **`default_profile`** (Function) — `src/config/theme_config/loader.rs:23`
- **`resolved_profile`** (Function) — `src/config/theme_config/loader.rs:27`
- **`active_profile`** (Function) — `src/config/theme_config/loader.rs:41`

## Key Symbols

| Symbol | Type | File | Line |
|--------|------|------|------|
| `from_rgba_u8` | Function | `src/config/theme_config/model.rs` | 35 |
| `builtin_dark` | Function | `src/config/theme_config/builtin.rs` | 8 |
| `default_profile` | Function | `src/config/theme_config/loader.rs` | 23 |
| `resolved_profile` | Function | `src/config/theme_config/loader.rs` | 27 |
| `active_profile` | Function | `src/config/theme_config/loader.rs` | 41 |
| `load_active` | Function | `src/config/theme_config/loader.rs` | 70 |
| `load_preferred` | Function | `src/config/theme_config/loader.rs` | 74 |
| `load` | Function | `src/config/theme_config/loader.rs` | 89 |
| `load_from_path` | Function | `src/config/theme_config/loader.rs` | 95 |
| `theme` | Function | `src/app/command_palette.rs` | 233 |
| `list_available_themes` | Function | `src/config/theme_config/loader.rs` | 45 |
| `list_available_theme_entries` | Function | `src/config/theme_config/loader.rs` | 52 |
| `open_theme_selector_palette` | Function | `src/app/app_state/palette.rs` | 110 |
| `apply_theme` | Function | `src/render/renderer/lifecycle.rs` | 305 |
| `theme_color_to_wgpu` | Function | `src/render/renderer/helpers.rs` | 379 |
| `new` | Function | `src/config/theme_config/model.rs` | 16 |
| `to_wgpu` | Function | `src/config/theme_config/model.rs` | 24 |
| `as_srgb_f32` | Function | `src/config/theme_config/model.rs` | 69 |
| `as_linear` | Function | `src/config/theme_config/model.rs` | 78 |
| `ext_icon_dot` | Function | `src/render/renderer/helpers.rs` | 386 |

## Execution Flows

| Flow | Type | Steps |
|------|------|-------|
| `Update_settings_buffer_content → From_rgba_u8` | cross_community | 6 |
| `Layout_shortcut_hint → From_rgba_u8` | cross_community | 6 |
| `Update_statusbar_content → As_srgb_f32` | cross_community | 5 |
| `Update_statusbar_content → New` | cross_community | 5 |
| `Update_statusbar_content → Srgb_to_linear` | cross_community | 5 |
| `New → From_rgba_u8` | cross_community | 5 |
| `Render_live_grep_picker → As_array` | cross_community | 4 |
| `New → UiThemeTokens` | cross_community | 4 |
| `New → EditorThemeTokens` | cross_community | 4 |
| `New → GitThemeTokens` | cross_community | 4 |

## Connected Areas

| Area | Connections |
|------|-------------|
| Event_loop | 2 calls |
| App_state | 2 calls |
| Command_dispatch | 2 calls |
| Config | 2 calls |
| Scheduler | 1 calls |

## How to Explore

1. `gitnexus_context({name: "from_rgba_u8"})` — see callers and callees
2. `gitnexus_query({query: "theme_config"})` — find related execution flows
3. Read key files listed above for implementation details
