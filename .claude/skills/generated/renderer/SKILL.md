---
name: renderer
description: "Skill for the Renderer area of netherize_editor. 55 symbols across 13 files."
---

# Renderer

55 symbols | 13 files | Cohesion: 56%

## When to Use

- Working with code in `src/`
- Understanding how with_radius, layout_panel_text_bold, estimate_monospace_width work
- Modifying renderer-related functionality

## Key Files

| File | Symbols |
|------|---------|
| `src/render/renderer/ui_render.rs` | right_sidebar_background_quads, empty_bounds_returns_no_quads, negative_dimensions_return_no_quads, produces_border_and_fill_without_input, produces_three_quads_with_input_bounds (+16) |
| `src/render/renderer/components.rs` | push_centered_highlight_chip, centered_text_origin_x, centered_text_origin_y, layout_shortcut_hint, mix (+3) |
| `src/render/renderer/helpers.rs` | layout_panel_text_bold, estimate_monospace_width, layout_clamp, theme_color_to_wgpu, mode_display_label (+1) |
| `src/render/renderer/ui/topbar.rs` | bundled_app_logo, inset_scissor_rect, topbar_tab_text_scissor, with_alpha, update_topbar_content |
| `src/render/renderer/lifecycle.rs` | make_text_pipeline, new, apply_theme |
| `src/app/app_state/editor.rs` | char_idx_for_line, byte_to_char_in_line |
| `src/render/renderer/ui/welcome.rs` | bundled_logo, update_welcome_screen_content |
| `src/render/renderer/editor/help.rs` | cheat_sheet_logo_rgba, update_help_buffer_content |
| `src/render/renderer/ui/statusbar.rs` | with_alpha, update_statusbar_content |
| `src/render/region_pipeline.rs` | with_radius |

## Entry Points

Start here when exploring this area:

- **`with_radius`** (Function) — `src/render/region_pipeline.rs:76`
- **`layout_panel_text_bold`** (Function) — `src/render/renderer/helpers.rs:62`
- **`estimate_monospace_width`** (Function) — `src/render/renderer/helpers.rs:167`
- **`layout_clamp`** (Function) — `src/render/renderer/helpers.rs:242`
- **`push_centered_highlight_chip`** (Function) — `src/render/renderer/components.rs:23`

## Key Symbols

| Symbol | Type | File | Line |
|--------|------|------|------|
| `with_radius` | Function | `src/render/region_pipeline.rs` | 76 |
| `layout_panel_text_bold` | Function | `src/render/renderer/helpers.rs` | 62 |
| `estimate_monospace_width` | Function | `src/render/renderer/helpers.rs` | 167 |
| `layout_clamp` | Function | `src/render/renderer/helpers.rs` | 242 |
| `push_centered_highlight_chip` | Function | `src/render/renderer/components.rs` | 23 |
| `layout_shortcut_hint` | Function | `src/render/renderer/components.rs` | 60 |
| `help_keycap_palette` | Function | `src/render/renderer/components.rs` | 205 |
| `layout_help_keycaps` | Function | `src/render/renderer/components.rs` | 250 |
| `estimate_help_keycaps_width` | Function | `src/render/renderer/components.rs` | 372 |
| `char_idx_for_line` | Function | `src/app/app_state/editor.rs` | 285 |
| `byte_to_char_in_line` | Function | `src/app/app_state/editor.rs` | 297 |
| `update_welcome_screen_content` | Function | `src/render/renderer/ui/welcome.rs` | 42 |
| `update_topbar_content` | Function | `src/render/renderer/ui/topbar.rs` | 56 |
| `update_editor_leap_labels` | Function | `src/render/renderer/palette/leap.rs` | 20 |
| `update_help_buffer_content` | Function | `src/render/renderer/editor/help.rs` | 47 |
| `right_sidebar_background_quads` | Function | `src/render/renderer/ui_render.rs` | 344 |
| `update_ai_chat_content` | Function | `src/render/renderer/ui_render.rs` | 742 |
| `new` | Function | `src/render/renderer/lifecycle.rs` | 42 |
| `apply_theme` | Function | `src/render/renderer/lifecycle.rs` | 303 |
| `theme_color_to_wgpu` | Function | `src/render/renderer/helpers.rs` | 379 |

## Execution Flows

| Flow | Type | Steps |
|------|------|-------|
| `Update_markdown_preview_content → Normalize_modifier_alias` | cross_community | 8 |
| `Update_markdown_preview_content → New` | cross_community | 8 |
| `Update_markdown_preview_content → Named_key_display` | cross_community | 7 |
| `Update_markdown_preview_content → Physical_key_display` | cross_community | 7 |
| `Update_markdown_preview_content → From_str` | cross_community | 6 |
| `Update_markdown_preview_content → Is_valid` | cross_community | 6 |
| `Layout_shortcut_hint → From_rgba_u8` | cross_community | 6 |
| `Update_ai_chat_content → HelpEntry` | cross_community | 5 |
| `Update_ai_chat_content → Command_label_for_help` | cross_community | 5 |
| `Update_ai_chat_content → HelpSection` | cross_community | 5 |

## Connected Areas

| Area | Connections |
|------|-------------|
| Ui | 12 calls |
| App_state | 10 calls |
| Palette | 8 calls |
| Text | 6 calls |
| Theme_config | 5 calls |
| Editor | 4 calls |
| Command_dispatch | 3 calls |
| Scheduler | 1 calls |

## How to Explore

1. `gitnexus_context({name: "with_radius"})` — see callers and callees
2. `gitnexus_query({query: "renderer"})` — find related execution flows
3. Read key files listed above for implementation details
