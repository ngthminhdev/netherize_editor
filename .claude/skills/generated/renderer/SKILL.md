---
name: renderer
description: "Skill for the Renderer area of netherize_editor. 95 symbols across 25 files."
---

# Renderer

95 symbols | 25 files | Cohesion: 76%

## When to Use

- Working with code in `src/`
- Understanding how set_size, used_rows, upload_instances work
- Modifying renderer-related functionality

## Key Files

| File | Symbols |
|------|---------|
| `src/render/renderer/ui_render.rs` | strip_ansi, is_opencode_status_line, with_alpha, blend_rgb, slash_suggestion_rect (+18) |
| `src/render/renderer/helpers.rs` | rect_to_scissor, layout_panel_text, layout_panel_rich_text, layout_panel_text_bold, layout_panel_text_italic (+6) |
| `src/render/renderer/components.rs` | push_centered_highlight_chip, centered_text_origin_x, centered_text_origin_y, layout_shortcut_hint, mix (+3) |
| `src/render/renderer/editor/settings.rs` | label, section, description, display_value, with_alpha (+3) |
| `src/app/app_state/mod.rs` | new, default, new, new, new |
| `src/render/renderer/ui/topbar.rs` | bundled_app_logo, inset_scissor_rect, topbar_tab_text_scissor, with_alpha, update_topbar_content |
| `src/render/renderer/ui/sidebar.rs` | sidebar_list_top, sidebar_list_bottom, sidebar_filter_y, update_sidebar_content |
| `src/render/renderer/ui/welcome.rs` | bundled_logo, update_welcome_screen_content, update_welcome_logo_content |
| `src/render/renderer/ui/terminal.rs` | inset_bounds, update_buffer_terminal_content, render_terminal_region |
| `src/render/renderer/palette/minimal.rs` | render_confirmation_palette, render_command_palette_minimalist, palette_tone_color |

## Entry Points

Start here when exploring this area:

- **`set_size`** (Function) — `src/text/text_system.rs:123`
- **`used_rows`** (Function) — `src/terminal/grid.rs:799`
- **`upload_instances`** (Function) — `src/render/text_pipeline.rs:194`
- **`with_radius`** (Function) — `src/render/region_pipeline.rs:76`
- **`update_ai_chat_content`** (Function) — `src/render/renderer/ui_render.rs:742`

## Key Symbols

| Symbol | Type | File | Line |
|--------|------|------|------|
| `set_size` | Function | `src/text/text_system.rs` | 123 |
| `used_rows` | Function | `src/terminal/grid.rs` | 799 |
| `upload_instances` | Function | `src/render/text_pipeline.rs` | 194 |
| `with_radius` | Function | `src/render/region_pipeline.rs` | 76 |
| `update_ai_chat_content` | Function | `src/render/renderer/ui_render.rs` | 742 |
| `clear_ai_chat` | Function | `src/render/renderer/ui_render.rs` | 1659 |
| `update_markdown_preview_content` | Function | `src/render/renderer/ui_render.rs` | 1676 |
| `rect_to_scissor` | Function | `src/render/renderer/helpers.rs` | 20 |
| `layout_panel_text` | Function | `src/render/renderer/helpers.rs` | 32 |
| `layout_panel_rich_text` | Function | `src/render/renderer/helpers.rs` | 46 |
| `layout_panel_text_bold` | Function | `src/render/renderer/helpers.rs` | 62 |
| `layout_panel_text_italic` | Function | `src/render/renderer/helpers.rs` | 76 |
| `estimate_monospace_width` | Function | `src/render/renderer/helpers.rs` | 167 |
| `clamp_monospace_text` | Function | `src/render/renderer/helpers.rs` | 171 |
| `layout_clamp` | Function | `src/render/renderer/helpers.rs` | 242 |
| `mode_display_label` | Function | `src/render/renderer/helpers.rs` | 354 |
| `ext_icon_dot` | Function | `src/render/renderer/helpers.rs` | 386 |
| `push_centered_highlight_chip` | Function | `src/render/renderer/components.rs` | 23 |
| `layout_shortcut_hint` | Function | `src/render/renderer/components.rs` | 60 |
| `help_keycap_palette` | Function | `src/render/renderer/components.rs` | 205 |

## Execution Flows

| Flow | Type | Steps |
|------|------|-------|
| `Update_markdown_preview_content → Normalize_modifier_alias` | cross_community | 8 |
| `Update_markdown_preview_content → New` | cross_community | 8 |
| `Bench_edit_loop_latency → HelpEntry` | cross_community | 7 |
| `Bench_edit_loop_latency → Command_label_for_help` | cross_community | 7 |
| `Bench_edit_loop_latency → HelpSection` | cross_community | 7 |
| `Bench_edit_loop_latency → Find_profile_path` | cross_community | 7 |
| `Update_markdown_preview_content → Named_key_display` | cross_community | 7 |
| `Update_markdown_preview_content → Physical_key_display` | cross_community | 7 |
| `Bench_edit_loop_latency → Active_profile` | cross_community | 6 |
| `Update_settings_buffer_content → From_rgba_u8` | cross_community | 6 |

## Connected Areas

| Area | Connections |
|------|-------------|
| Text | 15 calls |
| Workbench | 4 calls |
| App | 4 calls |
| Event_loop | 3 calls |
| Theme_config | 3 calls |
| Ui | 2 calls |
| Terminal | 2 calls |
| Editor | 2 calls |

## How to Explore

1. `gitnexus_context({name: "set_size"})` — see callers and callees
2. `gitnexus_query({query: "renderer"})` — find related execution flows
3. Read key files listed above for implementation details
