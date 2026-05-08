---
name: ui
description: "Skill for the Ui area of netherize_editor. 30 symbols across 11 files."
---

# Ui

30 symbols | 11 files | Cohesion: 56%

## When to Use

- Working with code in `src/`
- Understanding how set_size, set_metrics, used_rows work
- Modifying ui-related functionality

## Key Files

| File | Symbols |
|------|---------|
| `src/render/renderer/ui/popups.rs` | update_lsp_guide_popup, update_system_dep_popup, update_toast_popup, measure_wrapped_block_height, layout_wrapped_block |
| `src/render/renderer/helpers.rs` | rect_to_scissor, clamp_popup_width, clamp_x_in_bounds, clamp_popup_width_saturates_to_available_width_when_viewport_is_narrow |
| `src/render/renderer/ui/terminal.rs` | inset_bounds, update_buffer_terminal_content, render_terminal_region, append_terminal_overlay_quads |
| `src/render/renderer/ui/sidebar.rs` | sidebar_list_top, sidebar_list_bottom, sidebar_filter_y, update_sidebar_content |
| `src/terminal/grid.rs` | is_visually_empty, used_rows, debug_dump |
| `src/render/renderer/editor/overlays/diagnostic_hover.rs` | update_diagnostic_hover_popup, diagnostic_popup_width_handles_narrow_viewport_without_panicking, diagnostic_popup_width_uses_half_of_editor_viewport_when_space_allows |
| `src/text/text_system.rs` | set_size, set_metrics |
| `src/render/text_pipeline.rs` | upload_instances, ensure_instance_capacity |
| `src/config/theme_config/model.rs` | as_f32 |
| `src/render/renderer/ui/welcome.rs` | update_welcome_logo_content |

## Entry Points

Start here when exploring this area:

- **`set_size`** (Function) — `src/text/text_system.rs:123`
- **`set_metrics`** (Function) — `src/text/text_system.rs:127`
- **`used_rows`** (Function) — `src/terminal/grid.rs:799`
- **`debug_dump`** (Function) — `src/terminal/grid.rs:811`
- **`upload_instances`** (Function) — `src/render/text_pipeline.rs:194`

## Key Symbols

| Symbol | Type | File | Line |
|--------|------|------|------|
| `set_size` | Function | `src/text/text_system.rs` | 123 |
| `set_metrics` | Function | `src/text/text_system.rs` | 127 |
| `used_rows` | Function | `src/terminal/grid.rs` | 799 |
| `debug_dump` | Function | `src/terminal/grid.rs` | 811 |
| `upload_instances` | Function | `src/render/text_pipeline.rs` | 194 |
| `rect_to_scissor` | Function | `src/render/renderer/helpers.rs` | 20 |
| `clamp_popup_width` | Function | `src/render/renderer/helpers.rs` | 203 |
| `clamp_x_in_bounds` | Function | `src/render/renderer/helpers.rs` | 227 |
| `as_f32` | Function | `src/config/theme_config/model.rs` | 83 |
| `update_welcome_logo_content` | Function | `src/render/renderer/ui/welcome.rs` | 476 |
| `update_buffer_terminal_content` | Function | `src/render/renderer/ui/terminal.rs` | 53 |
| `update_lsp_guide_popup` | Function | `src/render/renderer/ui/popups.rs` | 16 |
| `update_system_dep_popup` | Function | `src/render/renderer/ui/popups.rs` | 197 |
| `update_toast_popup` | Function | `src/render/renderer/ui/popups.rs` | 466 |
| `update_diagnostic_hover_popup` | Function | `src/render/renderer/editor/overlays/diagnostic_hover.rs` | 35 |
| `update_diagnostics_buffer_content` | Function | `src/render/renderer/editor/buffers/diagnostics.rs` | 26 |
| `update_sidebar_content` | Function | `src/render/renderer/ui/sidebar.rs` | 31 |
| `is_visually_empty` | Function | `src/terminal/grid.rs` | 64 |
| `ensure_instance_capacity` | Function | `src/render/text_pipeline.rs` | 245 |
| `clamp_popup_width_saturates_to_available_width_when_viewport_is_narrow` | Function | `src/render/renderer/helpers.rs` | 324 |

## Execution Flows

| Flow | Type | Steps |
|------|------|-------|
| `Update_statusbar_content → As_srgb_f32` | cross_community | 5 |
| `Update_statusbar_content → New` | cross_community | 5 |
| `Update_statusbar_content → Srgb_to_linear` | cross_community | 5 |
| `Update_diagnostics_buffer_content → F32_channel_to_u8` | cross_community | 5 |
| `Update_diagnostics_buffer_content → Linear_to_srgb` | cross_community | 5 |
| `Update_diagnostics_buffer_content → VisibleGlyph` | cross_community | 5 |
| `Update_diagnostics_buffer_content → Rasterize_cache_key` | cross_community | 5 |
| `Update_diagnostics_buffer_content → Extract_alpha_from_image_data` | cross_community | 5 |
| `Update_diagnostics_buffer_content → RasterizedGlyph` | cross_community | 5 |
| `Update_diagnostics_buffer_content → Get` | cross_community | 5 |

## Connected Areas

| Area | Connections |
|------|-------------|
| Palette | 6 calls |
| Renderer | 5 calls |
| Text | 5 calls |
| App_state | 5 calls |
| Terminal | 4 calls |
| Theme_config | 3 calls |
| Editor | 3 calls |
| Scheduler | 1 calls |

## How to Explore

1. `gitnexus_context({name: "set_size"})` — see callers and callees
2. `gitnexus_query({query: "ui"})` — find related execution flows
3. Read key files listed above for implementation details
