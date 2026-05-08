---
name: palette
description: "Skill for the Palette area of netherize_editor. 27 symbols across 14 files."
---

# Palette

27 symbols | 14 files | Cohesion: 58%

## When to Use

- Working with code in `src/`
- Understanding how used_rows, upload_instances, rect_to_scissor work
- Modifying palette-related functionality

## Key Files

| File | Symbols |
|------|---------|
| `src/render/renderer/helpers.rs` | rect_to_scissor, layout_panel_text, layout_panel_rich_text, color_f32_to_u8, clamp_monospace_text (+1) |
| `src/render/renderer/ui/terminal.rs` | inset_bounds, update_buffer_terminal_content, render_terminal_region |
| `src/terminal/grid.rs` | is_visually_empty, used_rows |
| `src/render/text_pipeline.rs` | upload_instances, ensure_instance_capacity |
| `src/render/renderer/palette/minimal.rs` | render_command_palette_minimalist, palette_tone_color |
| `src/render/renderer/palette/highlighted_label.rs` | render_highlighted_label, sanitize_label_range |
| `src/render/renderer/palette/file_picker.rs` | render_file_picker_complex, file_picker_tone_color |
| `src/render/renderer/editor/help.rs` | cheat_sheet_logo_rgba, update_help_buffer_content |
| `src/render/renderer/ui/welcome.rs` | update_welcome_logo_content |
| `src/render/renderer/palette/recent_projects.rs` | render_recent_projects |

## Entry Points

Start here when exploring this area:

- **`used_rows`** (Function) — `src/terminal/grid.rs:799`
- **`upload_instances`** (Function) — `src/render/text_pipeline.rs:194`
- **`rect_to_scissor`** (Function) — `src/render/renderer/helpers.rs:20`
- **`layout_panel_text`** (Function) — `src/render/renderer/helpers.rs:32`
- **`layout_panel_rich_text`** (Function) — `src/render/renderer/helpers.rs:46`

## Key Symbols

| Symbol | Type | File | Line |
|--------|------|------|------|
| `used_rows` | Function | `src/terminal/grid.rs` | 799 |
| `upload_instances` | Function | `src/render/text_pipeline.rs` | 194 |
| `rect_to_scissor` | Function | `src/render/renderer/helpers.rs` | 20 |
| `layout_panel_text` | Function | `src/render/renderer/helpers.rs` | 32 |
| `layout_panel_rich_text` | Function | `src/render/renderer/helpers.rs` | 46 |
| `clamp_monospace_text` | Function | `src/render/renderer/helpers.rs` | 171 |
| `ext_icon_dot` | Function | `src/render/renderer/helpers.rs` | 386 |
| `update_welcome_logo_content` | Function | `src/render/renderer/ui/welcome.rs` | 476 |
| `update_buffer_terminal_content` | Function | `src/render/renderer/ui/terminal.rs` | 53 |
| `render_recent_projects` | Function | `src/render/renderer/palette/recent_projects.rs` | 15 |
| `render_command_palette_minimalist` | Function | `src/render/renderer/palette/minimal.rs` | 270 |
| `render_live_grep_picker` | Function | `src/render/renderer/palette/live_grep.rs` | 15 |
| `render_highlighted_label` | Function | `src/render/renderer/palette/highlighted_label.rs` | 19 |
| `sanitize_label_range` | Function | `src/render/renderer/palette/highlighted_label.rs` | 73 |
| `render_file_picker_complex` | Function | `src/render/renderer/palette/file_picker.rs` | 15 |
| `update_help_buffer_content` | Function | `src/render/renderer/editor/help.rs` | 47 |
| `update_fuzzy_picker_buffer_content` | Function | `src/render/renderer/editor/fuzzy.rs` | 28 |
| `update_references_buffer_content` | Function | `src/render/renderer/editor/buffers.rs` | 36 |
| `update_diagnostics_buffer_content` | Function | `src/render/renderer/editor/buffers/diagnostics.rs` | 26 |
| `is_visually_empty` | Function | `src/terminal/grid.rs` | 64 |

## Execution Flows

| Flow | Type | Steps |
|------|------|-------|
| `Update_settings_buffer_content → From_rgba_u8` | cross_community | 6 |
| `Layout_shortcut_hint → From_rgba_u8` | cross_community | 6 |
| `Update_ai_chat_content → F32_channel_to_u8` | cross_community | 5 |
| `Update_ai_chat_content → Linear_to_srgb` | cross_community | 5 |
| `Update_ai_chat_content → VisibleGlyph` | cross_community | 5 |
| `Update_ai_chat_content → Rasterize_cache_key` | cross_community | 5 |
| `Update_ai_chat_content → Extract_alpha_from_image_data` | cross_community | 5 |
| `Update_settings_buffer_content → F32_channel_to_u8` | cross_community | 5 |
| `Update_settings_buffer_content → Linear_to_srgb` | cross_community | 5 |
| `Update_settings_buffer_content → VisibleGlyph` | cross_community | 5 |

## Connected Areas

| Area | Connections |
|------|-------------|
| Text | 8 calls |
| Renderer | 8 calls |
| Terminal | 5 calls |
| Ui | 3 calls |
| Theme_config | 2 calls |
| Event_loop | 1 calls |

## How to Explore

1. `gitnexus_context({name: "used_rows"})` — see callers and callees
2. `gitnexus_query({query: "palette"})` — find related execution flows
3. Read key files listed above for implementation details
