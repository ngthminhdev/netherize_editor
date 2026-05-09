---
name: renderer
description: "Skill for the Renderer area of netherize_editor. 67 symbols across 14 files."
---

# Renderer

67 symbols | 14 files | Cohesion: 53%

## When to Use

- Working with code in `src/`
- Understanding how set_size, with_radius, layout_panel_text_bold work
- Modifying renderer-related functionality

## Key Files

| File | Symbols |
|------|---------|
| `src/render/renderer/ui_render.rs` | bundled_ai_chat_logo, slash_command_suggestions, current_at_token, ai_chat_input_suggestions, strip_ansi (+19) |
| `src/render/renderer/helpers.rs` | layout_panel_text_bold, estimate_monospace_width, layout_clamp, clamp_popup_width, clamp_x_in_bounds (+3) |
| `src/render/renderer/components.rs` | push_centered_highlight_chip, centered_text_origin_x, centered_text_origin_y, layout_shortcut_hint, mix (+3) |
| `src/render/renderer/ui/topbar.rs` | bundled_app_logo, inset_scissor_rect, topbar_tab_text_scissor, with_alpha, update_topbar_content |
| `src/app/app_state/mod.rs` | default, new, new, new, new |
| `src/render/renderer/ui/welcome.rs` | bundled_logo, update_welcome_screen_content, clear_welcome_logo |
| `src/config/theme_config/model.rs` | linear_to_srgb, linear_rgba_to_srgb_u8, f32_channel_to_u8 |
| `src/app/event_loop/application.rs` | focus_ring_instances, focus_ring_keeps_outline_and_panel_fill |
| `src/render/renderer/ui/popups.rs` | update_lsp_guide_popup, update_system_dep_popup |
| `src/render/renderer/editor/overlays/diagnostic_hover.rs` | diagnostic_popup_width_handles_narrow_viewport_without_panicking, diagnostic_popup_width_uses_half_of_editor_viewport_when_space_allows |

## Entry Points

Start here when exploring this area:

- **`set_size`** (Function) — `src/text/text_system.rs:123`
- **`with_radius`** (Function) — `src/render/region_pipeline.rs:76`
- **`layout_panel_text_bold`** (Function) — `src/render/renderer/helpers.rs:62`
- **`estimate_monospace_width`** (Function) — `src/render/renderer/helpers.rs:167`
- **`layout_clamp`** (Function) — `src/render/renderer/helpers.rs:242`

## Key Symbols

| Symbol | Type | File | Line |
|--------|------|------|------|
| `set_size` | Function | `src/text/text_system.rs` | 123 |
| `with_radius` | Function | `src/render/region_pipeline.rs` | 76 |
| `layout_panel_text_bold` | Function | `src/render/renderer/helpers.rs` | 62 |
| `estimate_monospace_width` | Function | `src/render/renderer/helpers.rs` | 167 |
| `layout_clamp` | Function | `src/render/renderer/helpers.rs` | 242 |
| `push_centered_highlight_chip` | Function | `src/render/renderer/components.rs` | 23 |
| `layout_shortcut_hint` | Function | `src/render/renderer/components.rs` | 60 |
| `help_keycap_palette` | Function | `src/render/renderer/components.rs` | 205 |
| `layout_help_keycaps` | Function | `src/render/renderer/components.rs` | 250 |
| `estimate_help_keycaps_width` | Function | `src/render/renderer/components.rs` | 372 |
| `update_welcome_screen_content` | Function | `src/render/renderer/ui/welcome.rs` | 42 |
| `clear_welcome_logo` | Function | `src/render/renderer/ui/welcome.rs` | 531 |
| `update_topbar_content` | Function | `src/render/renderer/ui/topbar.rs` | 56 |
| `new` | Function | `src/app/app_state/mod.rs` | 289 |
| `new` | Function | `src/app/app_state/mod.rs` | 1008 |
| `new` | Function | `src/app/app_state/mod.rs` | 1236 |
| `update_ai_chat_content` | Function | `src/render/renderer/ui_render.rs` | 742 |
| `linear_to_srgb` | Function | `src/config/theme_config/model.rs` | 97 |
| `linear_rgba_to_srgb_u8` | Function | `src/config/theme_config/model.rs` | 115 |
| `right_sidebar_background_quads` | Function | `src/render/renderer/ui_render.rs` | 344 |

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
| `Update_markdown_preview_content → From_str` | cross_community | 6 |

## Connected Areas

| Area | Connections |
|------|-------------|
| Palette | 20 calls |
| Text | 8 calls |
| Ui | 3 calls |
| Workbench | 2 calls |
| Scheduler | 1 calls |
| Syntax | 1 calls |
| Event_loop | 1 calls |
| App_state | 1 calls |

## How to Explore

1. `gitnexus_context({name: "set_size"})` — see callers and callees
2. `gitnexus_query({query: "renderer"})` — find related execution flows
3. Read key files listed above for implementation details
