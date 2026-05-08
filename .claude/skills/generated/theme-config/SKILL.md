---
name: theme-config
description: "Skill for the Theme_config area of netherize_editor. 56 symbols across 9 files."
---

# Theme_config

56 symbols | 9 files | Cohesion: 76%

## When to Use

- Working with code in `src/`
- Understanding how from_rgba_u8, builtin_dark, layout_panel_text_italic work
- Modifying theme_config-related functionality

## Key Files

| File | Symbols |
|------|---------|
| `src/config/theme_config/model.rs` | from_rgba_u8, file_icon_lookup_prefers_dir_then_exact_then_extension_then_default, linear_to_srgb, linear_rgba_to_srgb_u8, f32_channel_to_u8 (+16) |
| `src/config/theme_config/loader.rs` | from_raw, parse_extension_file_icons, parse_exact_file_icons, parse_editor, parse_ui (+14) |
| `src/config/theme_config/builtin.rs` | builtin_dark, builtin_editor_tokens, builtin_ui_tokens, builtin_syntax_tokens, builtin_git_tokens (+4) |
| `src/render/renderer/helpers.rs` | layout_panel_text_italic, color_f32_to_u8 |
| `src/text/layout_sync.rs` | color_f32_to_u8 |
| `src/app/app_state/state.rs` | inline_suggestion |
| `src/render/renderer/editor/viewport.rs` | collect_inline_suggestion_glyphs |
| `src/text/text_system.rs` | rgba_f32_from_color |
| `src/app/event_loop/helpers.rs` | build_sidebar_rows |

## Entry Points

Start here when exploring this area:

- **`from_rgba_u8`** (Function) — `src/config/theme_config/model.rs:35`
- **`builtin_dark`** (Function) — `src/config/theme_config/builtin.rs:8`
- **`layout_panel_text_italic`** (Function) — `src/render/renderer/helpers.rs:76`
- **`linear_to_srgb`** (Function) — `src/config/theme_config/model.rs:97`
- **`linear_rgba_to_srgb_u8`** (Function) — `src/config/theme_config/model.rs:115`

## Key Symbols

| Symbol | Type | File | Line |
|--------|------|------|------|
| `from_rgba_u8` | Function | `src/config/theme_config/model.rs` | 35 |
| `builtin_dark` | Function | `src/config/theme_config/builtin.rs` | 8 |
| `layout_panel_text_italic` | Function | `src/render/renderer/helpers.rs` | 76 |
| `linear_to_srgb` | Function | `src/config/theme_config/model.rs` | 97 |
| `linear_rgba_to_srgb_u8` | Function | `src/config/theme_config/model.rs` | 115 |
| `inline_suggestion` | Function | `src/app/app_state/state.rs` | 781 |
| `collect_inline_suggestion_glyphs` | Function | `src/render/renderer/editor/viewport.rs` | 345 |
| `new` | Function | `src/config/theme_config/model.rs` | 16 |
| `as_srgb_f32` | Function | `src/config/theme_config/model.rs` | 69 |
| `as_linear` | Function | `src/config/theme_config/model.rs` | 78 |
| `srgb_to_linear` | Function | `src/config/theme_config/model.rs` | 88 |
| `srgb_rgba_to_linear_f32` | Function | `src/config/theme_config/model.rs` | 106 |
| `sidebar_arrow` | Function | `src/config/theme_config/model.rs` | 277 |
| `file_icon_for_path` | Function | `src/config/theme_config/model.rs` | 289 |
| `file_icon_for_extension` | Function | `src/config/theme_config/model.rs` | 307 |
| `icon_theme_for_filename` | Function | `src/config/theme_config/model.rs` | 331 |
| `icon_theme_for_path` | Function | `src/config/theme_config/model.rs` | 347 |
| `get_icon_for_file` | Function | `src/config/theme_config/model.rs` | 368 |
| `build_sidebar_rows` | Function | `src/app/event_loop/helpers.rs` | 1242 |
| `default_profile` | Function | `src/config/theme_config/loader.rs` | 23 |

## Execution Flows

| Flow | Type | Steps |
|------|------|-------|
| `Update_settings_buffer_content → From_rgba_u8` | cross_community | 6 |
| `Rebuild_layout_projection → From_rgba_u8` | cross_community | 6 |
| `Layout_shortcut_hint → From_rgba_u8` | cross_community | 6 |
| `Update_ai_chat_content → F32_channel_to_u8` | cross_community | 5 |
| `Update_ai_chat_content → Linear_to_srgb` | cross_community | 5 |
| `Update_settings_buffer_content → F32_channel_to_u8` | cross_community | 5 |
| `Update_settings_buffer_content → Linear_to_srgb` | cross_community | 5 |
| `Update_statusbar_content → As_srgb_f32` | cross_community | 5 |
| `Update_statusbar_content → New` | cross_community | 5 |
| `Update_statusbar_content → Srgb_to_linear` | cross_community | 5 |

## Connected Areas

| Area | Connections |
|------|-------------|
| Text | 3 calls |
| Config | 1 calls |
| Scheduler | 1 calls |
| Ui | 1 calls |
| App_state | 1 calls |

## How to Explore

1. `gitnexus_context({name: "from_rgba_u8"})` — see callers and callees
2. `gitnexus_query({query: "theme_config"})` — find related execution flows
3. Read key files listed above for implementation details
