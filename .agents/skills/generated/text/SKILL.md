---
name: text
description: "Skill for the Text area of netherize_editor. 49 symbols across 13 files."
---

# Text

49 symbols | 13 files | Cohesion: 72%

## When to Use

- Working with code in `src/`
- Understanding how set_font_family, set_text, rasterize_cache_key work
- Modifying text-related functionality

## Key Files

| File | Symbols |
|------|---------|
| `src/text/text_system.rs` | set_font_family, set_text, rasterize_cache_key, collect_visible_glyphs, unbounded_height_shapes_deep_lines (+12) |
| `src/text/atlas.rs` | get, get_or_reserve, flush_pending, uv_min_max, allocate_region (+4) |
| `src/text/raster.rs` | rasterize_glyph_alpha, extract_alpha_from_image_data, mask_alpha_conversion_truncates_to_expected_pixel_count, color_alpha_conversion_reads_only_alpha_channel, subpixel_alpha_conversion_uses_max_channel_per_pixel (+1) |
| `src/text/layout_sync.rs` | rebuild_layout_projection, compute_cursor_overlay, compute_caret_layout, caret_uses_line_relative_byte_offset_for_second_line_start, compute_caret_layout_at |
| `src/terminal/ansi_parser.rs` | to_rgba_f32, to_rgba_f32_with_defaults, xterm256_to_rgb |
| `src/app/event_loop/helpers.rs` | clip_styled_span_to_line, heading_spans |
| `src/terminal/terminal_renderer.rs` | build_instances |
| `src/terminal/grid.rs` | iter_visible_cells |
| `src/render/renderer/lifecycle.rs` | make_text_system |
| `src/render/renderer/helpers.rs` | collect_instances |

## Entry Points

Start here when exploring this area:

- **`set_font_family`** (Function) — `src/text/text_system.rs:98`
- **`set_text`** (Function) — `src/text/text_system.rs:131`
- **`rasterize_cache_key`** (Function) — `src/text/text_system.rs:278`
- **`collect_visible_glyphs`** (Function) — `src/text/text_system.rs:294`
- **`rasterize_glyph_alpha`** (Function) — `src/text/raster.rs:19`

## Key Symbols

| Symbol | Type | File | Line |
|--------|------|------|------|
| `set_font_family` | Function | `src/text/text_system.rs` | 98 |
| `set_text` | Function | `src/text/text_system.rs` | 131 |
| `rasterize_cache_key` | Function | `src/text/text_system.rs` | 278 |
| `collect_visible_glyphs` | Function | `src/text/text_system.rs` | 294 |
| `rasterize_glyph_alpha` | Function | `src/text/raster.rs` | 19 |
| `rebuild_layout_projection` | Function | `src/text/layout_sync.rs` | 33 |
| `compute_cursor_overlay` | Function | `src/text/layout_sync.rs` | 123 |
| `compute_caret_layout` | Function | `src/text/layout_sync.rs` | 206 |
| `get` | Function | `src/text/atlas.rs` | 97 |
| `get_or_reserve` | Function | `src/text/atlas.rs` | 104 |
| `flush_pending` | Function | `src/text/atlas.rs` | 139 |
| `uv_min_max` | Function | `src/text/atlas.rs` | 171 |
| `build_instances` | Function | `src/terminal/terminal_renderer.rs` | 80 |
| `iter_visible_cells` | Function | `src/terminal/grid.rs` | 768 |
| `to_rgba_f32` | Function | `src/terminal/ansi_parser.rs` | 32 |
| `to_rgba_f32_with_defaults` | Function | `src/terminal/ansi_parser.rs` | 40 |
| `xterm256_to_rgb` | Function | `src/terminal/ansi_parser.rs` | 568 |
| `inline_suggestion` | Function | `src/app/app_state/state.rs` | 735 |
| `collect_inline_suggestion_glyphs` | Function | `src/render/renderer/editor/viewport.rs` | 337 |
| `set_text_with_color` | Function | `src/text/text_system.rs` | 139 |

## Execution Flows

| Flow | Type | Steps |
|------|------|-------|
| `Update_settings_buffer_content → From_rgba_u8` | cross_community | 6 |
| `Layout_shortcut_hint → From_rgba_u8` | cross_community | 6 |
| `Update_ai_chat_content → VisibleGlyph` | cross_community | 5 |
| `Update_ai_chat_content → Rasterize_cache_key` | cross_community | 5 |
| `Update_ai_chat_content → Extract_alpha_from_image_data` | cross_community | 5 |
| `Render_file_picker_complex → VisibleGlyph` | cross_community | 5 |
| `Render_file_picker_complex → Rasterize_cache_key` | cross_community | 5 |
| `Render_file_picker_complex → Extract_alpha_from_image_data` | cross_community | 5 |
| `Render_file_picker_complex → RasterizedGlyph` | cross_community | 5 |
| `Render_file_picker_complex → Get` | cross_community | 5 |

## Connected Areas

| Area | Connections |
|------|-------------|
| Renderer | 5 calls |
| Theme_config | 2 calls |
| Command_dispatch | 2 calls |
| Workbench | 1 calls |
| Terminal | 1 calls |
| Event_loop | 1 calls |
| Ui | 1 calls |

## How to Explore

1. `gitnexus_context({name: "set_font_family"})` — see callers and callees
2. `gitnexus_query({query: "text"})` — find related execution flows
3. Read key files listed above for implementation details
