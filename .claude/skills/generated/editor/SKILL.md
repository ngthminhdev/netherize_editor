---
name: editor
description: "Skill for the Editor area of netherize_editor. 23 symbols across 9 files."
---

# Editor

23 symbols | 9 files | Cohesion: 50%

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
| `src/render/renderer/editor/completion.rs` | completion_label_spans, completion_kind_badge |
| `src/render/renderer/helpers.rs` | layout_panel_rich_text, clamp_monospace_text |
| `src/render/renderer/editor/buffers.rs` | clear_editor_overlays, update_references_buffer_content |
| `src/app/app_state/overlays.rs` | is_completion_loading |
| `src/render/renderer/editor/fuzzy.rs` | update_fuzzy_picker_buffer_content |

## Entry Points

Start here when exploring this area:

- **`wrap_text_lines`** (Function) — `src/render/renderer/editor.rs:41`
- **`current_overlays`** (Function) — `src/app/app_state/state.rs:751`
- **`completion`** (Function) — `src/app/app_state/state.rs:755`
- **`is_completion_loading`** (Function) — `src/app/app_state/overlays.rs:62`
- **`update_editor_overlays`** (Function) — `src/render/renderer/editor/overlays.rs:33`

## Key Symbols

| Symbol | Type | File | Line |
|--------|------|------|------|
| `wrap_text_lines` | Function | `src/render/renderer/editor.rs` | 41 |
| `current_overlays` | Function | `src/app/app_state/state.rs` | 751 |
| `completion` | Function | `src/app/app_state/state.rs` | 755 |
| `is_completion_loading` | Function | `src/app/app_state/overlays.rs` | 62 |
| `update_editor_overlays` | Function | `src/render/renderer/editor/overlays.rs` | 33 |
| `completion_label_spans` | Function | `src/render/renderer/editor/completion.rs` | 26 |
| `completion_kind_badge` | Function | `src/render/renderer/editor/completion.rs` | 48 |
| `update_settings_buffer_content` | Function | `src/render/renderer/editor/settings.rs` | 268 |
| `layout_panel_rich_text` | Function | `src/render/renderer/helpers.rs` | 46 |
| `clamp_monospace_text` | Function | `src/render/renderer/helpers.rs` | 171 |
| `update_fuzzy_picker_buffer_content` | Function | `src/render/renderer/editor/fuzzy.rs` | 28 |
| `clear_editor_overlays` | Function | `src/render/renderer/editor/buffers.rs` | 28 |
| `update_references_buffer_content` | Function | `src/render/renderer/editor/buffers.rs` | 36 |
| `wrap_text_lines_keeps_output_non_empty` | Function | `src/render/renderer/editor.rs` | 116 |
| `blend_rgba` | Function | `src/render/renderer/editor/overlays.rs` | 929 |
| `strip_markdown_inline` | Function | `src/render/renderer/editor/overlays.rs` | 940 |
| `label` | Function | `src/render/renderer/editor/settings.rs` | 22 |
| `section` | Function | `src/render/renderer/editor/settings.rs` | 34 |
| `description` | Function | `src/render/renderer/editor/settings.rs` | 52 |
| `display_value` | Function | `src/render/renderer/editor/settings.rs` | 121 |

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
| Renderer | 8 calls |
| Ui | 6 calls |
| App_state | 6 calls |
| Text | 5 calls |
| Palette | 4 calls |
| Theme_config | 2 calls |
| Event_loop | 1 calls |
| Workbench | 1 calls |

## How to Explore

1. `gitnexus_context({name: "wrap_text_lines"})` — see callers and callees
2. `gitnexus_query({query: "editor"})` — find related execution flows
3. Read key files listed above for implementation details
