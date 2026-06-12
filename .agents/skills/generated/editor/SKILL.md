---
name: editor
description: "Skill for the Editor area of netherize_editor. 17 symbols across 6 files."
---

# Editor

17 symbols | 6 files | Cohesion: 55%

## When to Use

- Working with code in `src/`
- Understanding how wrap_text_lines, current_overlays, completion work
- Modifying editor-related functionality

## Key Files

| File | Symbols |
|------|---------|
| `src/render/renderer/editor/settings.rs` | label, section, description, display_value, with_alpha (+3) |
| `src/render/renderer/editor/overlays.rs` | update_editor_overlays, blend_rgba, strip_markdown_inline |
| `src/render/renderer/editor.rs` | wrap_text_lines, wrap_text_lines_keeps_output_non_empty |
| `src/app/app_state/state.rs` | current_overlays, completion |
| `src/app/app_state/overlays.rs` | is_completion_loading |
| `src/render/renderer/editor/completion.rs` | completion_label_spans |

## Entry Points

Start here when exploring this area:

- **`wrap_text_lines`** (Function) — `src/render/renderer/editor.rs:41`
- **`current_overlays`** (Function) — `src/app/app_state/state.rs:705`
- **`completion`** (Function) — `src/app/app_state/state.rs:709`
- **`is_completion_loading`** (Function) — `src/app/app_state/overlays.rs:62`
- **`update_editor_overlays`** (Function) — `src/render/renderer/editor/overlays.rs:33`

## Key Symbols

| Symbol | Type | File | Line |
|--------|------|------|------|
| `wrap_text_lines` | Function | `src/render/renderer/editor.rs` | 41 |
| `current_overlays` | Function | `src/app/app_state/state.rs` | 705 |
| `completion` | Function | `src/app/app_state/state.rs` | 709 |
| `is_completion_loading` | Function | `src/app/app_state/overlays.rs` | 62 |
| `update_editor_overlays` | Function | `src/render/renderer/editor/overlays.rs` | 33 |
| `completion_label_spans` | Function | `src/render/renderer/editor/completion.rs` | 26 |
| `update_settings_buffer_content` | Function | `src/render/renderer/editor/settings.rs` | 268 |
| `wrap_text_lines_keeps_output_non_empty` | Function | `src/render/renderer/editor.rs` | 116 |
| `blend_rgba` | Function | `src/render/renderer/editor/overlays.rs` | 929 |
| `strip_markdown_inline` | Function | `src/render/renderer/editor/overlays.rs` | 940 |
| `label` | Function | `src/render/renderer/editor/settings.rs` | 22 |
| `section` | Function | `src/render/renderer/editor/settings.rs` | 34 |
| `description` | Function | `src/render/renderer/editor/settings.rs` | 52 |
| `display_value` | Function | `src/render/renderer/editor/settings.rs` | 121 |
| `with_alpha` | Function | `src/render/renderer/editor/settings.rs` | 156 |
| `current_row_value` | Function | `src/render/renderer/editor/settings.rs` | 161 |
| `settings_preview_lines` | Function | `src/render/renderer/editor/settings.rs` | 184 |

## Execution Flows

| Flow | Type | Steps |
|------|------|-------|
| `Update_settings_buffer_content → From_rgba_u8` | cross_community | 6 |
| `Update_settings_buffer_content → F32_channel_to_u8` | cross_community | 5 |
| `Update_settings_buffer_content → Linear_to_srgb` | cross_community | 5 |
| `Update_settings_buffer_content → VisibleGlyph` | cross_community | 5 |
| `Update_settings_buffer_content → Rasterize_cache_key` | cross_community | 5 |
| `Update_settings_buffer_content → Extract_alpha_from_image_data` | cross_community | 5 |
| `Update_settings_buffer_content → RasterizedGlyph` | cross_community | 5 |
| `Update_settings_buffer_content → Get` | cross_community | 5 |
| `Update_settings_buffer_content → PendingGlyphUpload` | cross_community | 5 |
| `Update_settings_buffer_content → AtlasEntry` | cross_community | 5 |

## Connected Areas

| Area | Connections |
|------|-------------|
| Palette | 7 calls |
| Renderer | 6 calls |
| App_state | 6 calls |
| Ui | 1 calls |
| Text | 1 calls |
| Benches | 1 calls |
| Event_loop | 1 calls |
| Workbench | 1 calls |

## How to Explore

1. `gitnexus_context({name: "wrap_text_lines"})` — see callers and callees
2. `gitnexus_query({query: "editor"})` — find related execution flows
3. Read key files listed above for implementation details
