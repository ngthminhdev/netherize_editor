---
name: ui
description: "Skill for the Ui area of netherize_editor. 14 symbols across 4 files."
---

# Ui

14 symbols | 4 files | Cohesion: 57%

## When to Use

- Working with code in `src/`
- Understanding how set_metrics, buffer, clamp_popup_width work
- Modifying ui-related functionality

## Key Files

| File | Symbols |
|------|---------|
| `src/render/renderer/ui/popups.rs` | update_lsp_guide_popup, update_system_dep_popup, update_toast_popup, measure_wrapped_block_height, layout_wrapped_block |
| `src/render/renderer/editor/overlays/diagnostic_hover.rs` | clear_diagnostic_hover_popup, update_diagnostic_hover_popup, diagnostic_popup_width_handles_narrow_viewport_without_panicking, diagnostic_popup_width_uses_half_of_editor_viewport_when_space_allows |
| `src/render/renderer/helpers.rs` | clamp_popup_width, clamp_x_in_bounds, clamp_popup_width_saturates_to_available_width_when_viewport_is_narrow |
| `src/text/text_system.rs` | set_metrics, buffer |

## Entry Points

Start here when exploring this area:

- **`set_metrics`** (Function) — `src/text/text_system.rs:127`
- **`buffer`** (Function) — `src/text/text_system.rs:274`
- **`clamp_popup_width`** (Function) — `src/render/renderer/helpers.rs:203`
- **`clamp_x_in_bounds`** (Function) — `src/render/renderer/helpers.rs:227`
- **`update_lsp_guide_popup`** (Function) — `src/render/renderer/ui/popups.rs:16`

## Key Symbols

| Symbol | Type | File | Line |
|--------|------|------|------|
| `set_metrics` | Function | `src/text/text_system.rs` | 127 |
| `buffer` | Function | `src/text/text_system.rs` | 274 |
| `clamp_popup_width` | Function | `src/render/renderer/helpers.rs` | 203 |
| `clamp_x_in_bounds` | Function | `src/render/renderer/helpers.rs` | 227 |
| `update_lsp_guide_popup` | Function | `src/render/renderer/ui/popups.rs` | 16 |
| `update_system_dep_popup` | Function | `src/render/renderer/ui/popups.rs` | 197 |
| `update_toast_popup` | Function | `src/render/renderer/ui/popups.rs` | 466 |
| `clear_diagnostic_hover_popup` | Function | `src/render/renderer/editor/overlays/diagnostic_hover.rs` | 27 |
| `update_diagnostic_hover_popup` | Function | `src/render/renderer/editor/overlays/diagnostic_hover.rs` | 35 |
| `clamp_popup_width_saturates_to_available_width_when_viewport_is_narrow` | Function | `src/render/renderer/helpers.rs` | 324 |
| `measure_wrapped_block_height` | Function | `src/render/renderer/ui/popups.rs` | 547 |
| `layout_wrapped_block` | Function | `src/render/renderer/ui/popups.rs` | 568 |
| `diagnostic_popup_width_handles_narrow_viewport_without_panicking` | Function | `src/render/renderer/editor/overlays/diagnostic_hover.rs` | 159 |
| `diagnostic_popup_width_uses_half_of_editor_viewport_when_space_allows` | Function | `src/render/renderer/editor/overlays/diagnostic_hover.rs` | 181 |

## Connected Areas

| Area | Connections |
|------|-------------|
| Renderer | 10 calls |
| Text | 3 calls |
| App_state | 2 calls |
| Editor | 2 calls |
| Theme_config | 1 calls |
| App | 1 calls |

## How to Explore

1. `gitnexus_context({name: "set_metrics"})` — see callers and callees
2. `gitnexus_query({query: "ui"})` — find related execution flows
3. Read key files listed above for implementation details
