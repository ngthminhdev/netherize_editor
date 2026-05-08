---
name: theme-config
description: "Skill for the Theme_config area of netherize_editor. 57 symbols across 10 files."
---

# Theme_config

57 symbols | 10 files | Cohesion: 77%

## When to Use

- Working with code in `src/`
- Understanding how from_rgba_u8, builtin_dark, sidebar_arrow work
- Modifying theme_config-related functionality

## Key Files

| File | Symbols |
|------|---------|
| `src/config/theme_config/model.rs` | from_rgba_u8, file_icon_lookup_prefers_dir_then_exact_then_extension_then_default, sidebar_arrow, file_icon_for_path, file_icon_for_extension (+17) |
| `src/config/theme_config/loader.rs` | from_raw, parse_extension_file_icons, parse_exact_file_icons, parse_editor, parse_ui (+14) |
| `src/config/theme_config/builtin.rs` | builtin_dark, builtin_editor_tokens, builtin_ui_tokens, builtin_syntax_tokens, builtin_git_tokens (+4) |
| `src/app/event_loop/helpers.rs` | build_sidebar_rows |
| `src/text/layout_sync.rs` | color_f32_to_u8 |
| `src/app/app_state/state.rs` | inline_suggestion |
| `src/render/renderer/editor/viewport.rs` | collect_inline_suggestion_glyphs |
| `src/render/renderer/lifecycle.rs` | apply_theme |
| `src/render/renderer/helpers.rs` | theme_color_to_wgpu |
| `src/text/text_system.rs` | rgba_f32_from_color |

## Entry Points

Start here when exploring this area:

- **`from_rgba_u8`** (Function) — `src/config/theme_config/model.rs:35`
- **`builtin_dark`** (Function) — `src/config/theme_config/builtin.rs:8`
- **`sidebar_arrow`** (Function) — `src/config/theme_config/model.rs:277`
- **`file_icon_for_path`** (Function) — `src/config/theme_config/model.rs:289`
- **`file_icon_for_extension`** (Function) — `src/config/theme_config/model.rs:307`

## Key Symbols

| Symbol | Type | File | Line |
|--------|------|------|------|
| `from_rgba_u8` | Function | `src/config/theme_config/model.rs` | 35 |
| `builtin_dark` | Function | `src/config/theme_config/builtin.rs` | 8 |
| `sidebar_arrow` | Function | `src/config/theme_config/model.rs` | 277 |
| `file_icon_for_path` | Function | `src/config/theme_config/model.rs` | 289 |
| `file_icon_for_extension` | Function | `src/config/theme_config/model.rs` | 307 |
| `icon_theme_for_filename` | Function | `src/config/theme_config/model.rs` | 331 |
| `icon_theme_for_path` | Function | `src/config/theme_config/model.rs` | 347 |
| `get_icon_for_file` | Function | `src/config/theme_config/model.rs` | 368 |
| `build_sidebar_rows` | Function | `src/app/event_loop/helpers.rs` | 1242 |
| `default_profile` | Function | `src/config/theme_config/loader.rs` | 23 |
| `resolved_profile` | Function | `src/config/theme_config/loader.rs` | 27 |
| `active_profile` | Function | `src/config/theme_config/loader.rs` | 41 |
| `load_active` | Function | `src/config/theme_config/loader.rs` | 70 |
| `load_preferred` | Function | `src/config/theme_config/loader.rs` | 74 |
| `load` | Function | `src/config/theme_config/loader.rs` | 89 |
| `load_from_path` | Function | `src/config/theme_config/loader.rs` | 95 |
| `linear_to_srgb` | Function | `src/config/theme_config/model.rs` | 97 |
| `linear_rgba_to_srgb_u8` | Function | `src/config/theme_config/model.rs` | 115 |
| `inline_suggestion` | Function | `src/app/app_state/state.rs` | 735 |
| `collect_inline_suggestion_glyphs` | Function | `src/render/renderer/editor/viewport.rs` | 345 |

## Execution Flows

| Flow | Type | Steps |
|------|------|-------|
| `Update_settings_buffer_content → From_rgba_u8` | cross_community | 6 |
| `Rebuild_layout_projection → From_rgba_u8` | cross_community | 6 |
| `Layout_shortcut_hint → From_rgba_u8` | cross_community | 6 |
| `Update_ai_chat_content → F32_channel_to_u8` | cross_community | 5 |
| `Update_ai_chat_content → Linear_to_srgb` | cross_community | 5 |
| `Render_file_picker_complex → F32_channel_to_u8` | cross_community | 5 |
| `Render_file_picker_complex → Linear_to_srgb` | cross_community | 5 |
| `Update_settings_buffer_content → F32_channel_to_u8` | cross_community | 5 |
| `Update_settings_buffer_content → Linear_to_srgb` | cross_community | 5 |
| `Update_statusbar_content → As_srgb_f32` | cross_community | 5 |

## Connected Areas

| Area | Connections |
|------|-------------|
| Config | 1 calls |
| Scheduler | 1 calls |
| App | 1 calls |
| Editor | 1 calls |
| Text | 1 calls |

## How to Explore

1. `gitnexus_context({name: "from_rgba_u8"})` — see callers and callees
2. `gitnexus_query({query: "theme_config"})` — find related execution flows
3. Read key files listed above for implementation details
