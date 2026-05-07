---
name: renderer
description: "Skill for the Renderer area of netherize_editor. 54 symbols across 13 files."
---

# Renderer

54 symbols | 13 files | Cohesion: 58%

## When to Use

- Working with code in `src/`
- Understanding how with_radius, layout_panel_text_bold, estimate_monospace_width work
- Modifying renderer-related functionality

## Key Files

| File | Symbols |
|------|---------|
| `src/render/renderer/ui_render.rs` | bundled_ai_chat_logo, slash_command_suggestions, current_at_token, ai_chat_input_suggestions, right_sidebar_background_quads (+15) |
| `src/render/renderer/components.rs` | push_centered_highlight_chip, centered_text_origin_x, centered_text_origin_y, layout_shortcut_hint, mix (+3) |
| `src/render/renderer/helpers.rs` | layout_panel_text_bold, estimate_monospace_width, layout_clamp, mode_display_label, mode_pill_color |
| `src/render/renderer/ui/topbar.rs` | bundled_app_logo, inset_scissor_rect, topbar_tab_text_scissor, with_alpha, update_topbar_content |
| `src/app/app_state/mod.rs` | default, new, new, new |
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
| `char_idx_for_line` | Function | `src/app/app_state/editor.rs` | 213 |
| `byte_to_char_in_line` | Function | `src/app/app_state/editor.rs` | 225 |
| `update_welcome_screen_content` | Function | `src/render/renderer/ui/welcome.rs` | 42 |
| `update_topbar_content` | Function | `src/render/renderer/ui/topbar.rs` | 56 |
| `update_editor_leap_labels` | Function | `src/render/renderer/palette/leap.rs` | 20 |
| `update_help_buffer_content` | Function | `src/render/renderer/editor/help.rs` | 47 |
| `new` | Function | `src/app/app_state/mod.rs` | 249 |
| `new` | Function | `src/app/app_state/mod.rs` | 1011 |
| `new` | Function | `src/app/app_state/mod.rs` | 1234 |
| `right_sidebar_background_quads` | Function | `src/render/renderer/ui_render.rs` | 246 |
| `update_ai_chat_content` | Function | `src/render/renderer/ui_render.rs` | 565 |

## Execution Flows

| Flow | Type | Steps |
|------|------|-------|
| `Bench_edit_loop_latency → HelpEntry` | cross_community | 7 |
| `Bench_edit_loop_latency → Command_label_for_help` | cross_community | 7 |
| `Bench_edit_loop_latency → HelpSection` | cross_community | 7 |
| `Bench_edit_loop_latency → Find_profile_path` | cross_community | 7 |
| `Bench_edit_loop_latency → Active_profile` | cross_community | 6 |
| `Layout_shortcut_hint → From_rgba_u8` | cross_community | 6 |
| `Terminal_normal_selection_yanks_terminal_grid_and_returns_to_typing_mode → Active_profile` | cross_community | 6 |
| `Terminal_normal_selection_yanks_terminal_grid_and_returns_to_typing_mode → HelpEntry` | cross_community | 6 |
| `Terminal_normal_selection_yanks_terminal_grid_and_returns_to_typing_mode → Command_label_for_help` | cross_community | 6 |
| `Terminal_normal_selection_yanks_terminal_grid_and_returns_to_typing_mode → HelpSection` | cross_community | 6 |

## Connected Areas

| Area | Connections |
|------|-------------|
| Ui | 11 calls |
| Palette | 7 calls |
| Text | 4 calls |
| App_state | 4 calls |
| Theme_config | 3 calls |
| Editor | 3 calls |
| Workbench | 2 calls |
| Command_dispatch | 2 calls |

## How to Explore

1. `gitnexus_context({name: "with_radius"})` — see callers and callees
2. `gitnexus_query({query: "renderer"})` — find related execution flows
3. Read key files listed above for implementation details
