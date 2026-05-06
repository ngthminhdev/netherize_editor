---
name: theme-config
description: "Skill for the Theme_config area of netherize_editor. 54 symbols across 7 files."
---

# Theme_config

54 symbols | 7 files | Cohesion: 78%

## When to Use

- Working with code in `src/`
- Understanding how from_rgba_u8, builtin_dark, default_profile work
- Modifying theme_config-related functionality

## Key Files

| File | Symbols |
|------|---------|
| `src/config/theme_config/loader.rs` | from_raw, parse_extension_file_icons, parse_exact_file_icons, parse_editor, parse_ui (+19) |
| `src/config/theme_config/model.rs` | from_rgba_u8, file_icon_lookup_prefers_dir_then_exact_then_extension_then_default, new, to_wgpu, as_srgb_f32 (+12) |
| `src/config/theme_config/builtin.rs` | builtin_dark, builtin_editor_tokens, builtin_ui_tokens, builtin_syntax_tokens, builtin_git_tokens (+4) |
| `src/app/event_loop/helpers.rs` | syntax_spans_to_styled_applies_theme_colors_and_emphasis |
| `src/render/renderer/lifecycle.rs` | apply_theme |
| `src/render/renderer/helpers.rs` | theme_color_to_wgpu |
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
| `apply_theme` | Function | `src/render/renderer/lifecycle.rs` | 288 |
| `theme_color_to_wgpu` | Function | `src/render/renderer/helpers.rs` | 365 |
| `new` | Function | `src/config/theme_config/model.rs` | 16 |
| `to_wgpu` | Function | `src/config/theme_config/model.rs` | 24 |
| `as_srgb_f32` | Function | `src/config/theme_config/model.rs` | 69 |
| `as_linear` | Function | `src/config/theme_config/model.rs` | 78 |
| `list_available_themes` | Function | `src/config/theme_config/loader.rs` | 45 |
| `list_available_theme_entries` | Function | `src/config/theme_config/loader.rs` | 52 |
| `srgb_to_linear` | Function | `src/config/theme_config/model.rs` | 88 |
| `srgb_rgba_to_linear_f32` | Function | `src/config/theme_config/model.rs` | 106 |
| `from_hex` | Function | `src/config/theme_config/model.rs` | 41 |

## Execution Flows

| Flow | Type | Steps |
|------|------|-------|
| `Update_settings_buffer_content → From_rgba_u8` | cross_community | 6 |
| `Rebuild_layout_projection → From_rgba_u8` | cross_community | 6 |
| `Update_welcome_screen_content → From_rgba_u8` | cross_community | 6 |
| `Update_topbar_content → From_rgba_u8` | cross_community | 6 |
| `Layout_shortcut_hint → From_rgba_u8` | cross_community | 6 |
| `New → From_rgba_u8` | cross_community | 5 |
| `Update_statusbar_content → As_srgb_f32` | cross_community | 5 |
| `Update_statusbar_content → New` | cross_community | 5 |
| `Update_statusbar_content → Srgb_to_linear` | cross_community | 5 |
| `New → UiThemeTokens` | cross_community | 4 |

## Connected Areas

| Area | Connections |
|------|-------------|
| Config | 2 calls |
| Cluster_2 | 1 calls |
| Workbench | 1 calls |
| Event_loop | 1 calls |

## How to Explore

1. `gitnexus_context({name: "from_rgba_u8"})` — see callers and callees
2. `gitnexus_query({query: "theme_config"})` — find related execution flows
3. Read key files listed above for implementation details
