---
name: renderer
description: "Skill for the Renderer area of netherize_editor. 181 symbols across 45 files."
---

# Renderer

181 symbols | 45 files | Cohesion: 85%

## When to Use

- Working with code in `src/`
- Understanding how with_style, set_size, set_metrics work
- Modifying renderer-related functionality

## Key Files

| File | Symbols |
|------|---------|
| `src/render/renderer/ui_render.rs` | strip_ansi, is_opencode_status_line, with_alpha, blend_rgb, slash_suggestion_rect (+17) |
| `src/render/renderer/helpers.rs` | rect_to_scissor, layout_panel_text, layout_panel_rich_text, layout_panel_text_bold, layout_panel_text_italic (+15) |
| `src/app/app_state/state.rs` | search_highlights, byte_to_line_idx, line_start_byte_idx, line_end_byte_idx, line_content_end_byte_idx (+7) |
| `src/render/renderer/components.rs` | push_centered_highlight_chip, centered_text_origin_x, centered_text_origin_y, layout_shortcut_hint, mix (+3) |
| `src/render/renderer/editor/settings.rs` | label, section, description, display_value, with_alpha (+3) |
| `src/render/renderer/ui/terminal.rs` | inset_bounds, update_terminal_content, update_buffer_terminal_content, render_terminal_region, append_terminal_overlay_quads (+2) |
| `src/render/renderer/editor/selections.rs` | leading_indent_columns, indent_guide_quads, current_line_highlight_quad, visual_selection_quads, search_highlight_quads (+2) |
| `src/config/theme_config/model.rs` | as_f32, linear_to_srgb, linear_rgba_to_srgb_u8, f32_channel_to_u8, sidebar_arrow (+1) |
| `src/render/renderer/editor/viewport.rs` | spans_fingerprint, clear_editor_content, update_image_content, update_editor_content, update_editor_caret (+1) |
| `src/text/text_system.rs` | with_style, set_size, set_metrics, set_text_bold_color, buffer |

## Entry Points

Start here when exploring this area:

- **`with_style`** (Function) — `src/text/text_system.rs:53`
- **`set_size`** (Function) — `src/text/text_system.rs:123`
- **`set_metrics`** (Function) — `src/text/text_system.rs:127`
- **`set_text_bold_color`** (Function) — `src/text/text_system.rs:151`
- **`buffer`** (Function) — `src/text/text_system.rs:274`

## Key Symbols

| Symbol | Type | File | Line |
|--------|------|------|------|
| `with_style` | Function | `src/text/text_system.rs` | 53 |
| `set_size` | Function | `src/text/text_system.rs` | 123 |
| `set_metrics` | Function | `src/text/text_system.rs` | 127 |
| `set_text_bold_color` | Function | `src/text/text_system.rs` | 151 |
| `buffer` | Function | `src/text/text_system.rs` | 274 |
| `visual_y_for_logical_scroll` | Function | `src/text/layout_sync.rs` | 184 |
| `compute_caret_layout` | Function | `src/text/layout_sync.rs` | 206 |
| `used_rows` | Function | `src/terminal/grid.rs` | 750 |
| `upload_instances` | Function | `src/render/text_pipeline.rs` | 194 |
| `editor_chrome_instances` | Function | `src/render/renderer.rs` | 290 |
| `with_radius` | Function | `src/render/region_pipeline.rs` | 76 |
| `update_ai_chat_content` | Function | `src/render/renderer/ui_render.rs` | 565 |
| `clear_ai_chat` | Function | `src/render/renderer/ui_render.rs` | 1416 |
| `update_markdown_preview_content` | Function | `src/render/renderer/ui_render.rs` | 1430 |
| `clear_palette` | Function | `src/render/renderer/palette.rs` | 43 |
| `reconfigure_surface` | Function | `src/render/renderer/lifecycle.rs` | 438 |
| `rect_to_scissor` | Function | `src/render/renderer/helpers.rs` | 20 |
| `layout_panel_text` | Function | `src/render/renderer/helpers.rs` | 32 |
| `layout_panel_rich_text` | Function | `src/render/renderer/helpers.rs` | 46 |
| `layout_panel_text_bold` | Function | `src/render/renderer/helpers.rs` | 62 |

## Execution Flows

| Flow | Type | Steps |
|------|------|-------|
| `Bench_edit_loop_latency → HelpEntry` | cross_community | 7 |
| `Bench_edit_loop_latency → Command_label_for_help` | cross_community | 7 |
| `Bench_edit_loop_latency → HelpSection` | cross_community | 7 |
| `Bench_edit_loop_latency → Find_profile_path` | cross_community | 7 |
| `Bench_edit_loop_latency → Active_profile` | cross_community | 6 |
| `Update_settings_buffer_content → From_rgba_u8` | cross_community | 6 |
| `Update_welcome_screen_content → From_rgba_u8` | cross_community | 6 |
| `Update_topbar_content → From_rgba_u8` | cross_community | 6 |
| `Layout_shortcut_hint → From_rgba_u8` | cross_community | 6 |
| `Terminal_normal_selection_yanks_terminal_grid_and_returns_to_typing_mode → Active_profile` | cross_community | 6 |

## Connected Areas

| Area | Connections |
|------|-------------|
| Text | 15 calls |
| App_state | 6 calls |
| Event_loop | 5 calls |
| Workbench | 4 calls |
| Benches | 4 calls |
| Theme_config | 3 calls |
| Terminal | 3 calls |
| Command_dispatch | 2 calls |

## How to Explore

1. `gitnexus_context({name: "with_style"})` — see callers and callees
2. `gitnexus_query({query: "renderer"})` — find related execution flows
3. Read key files listed above for implementation details
