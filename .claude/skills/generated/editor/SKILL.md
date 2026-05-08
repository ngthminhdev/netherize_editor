---
name: editor
description: "Skill for the Editor area of netherize_editor. 25 symbols across 10 files."
---

# Editor

25 symbols | 10 files | Cohesion: 53%

## When to Use

- Working with code in `src/`
- Understanding how compute_caret_layout, gutter_width_for_editor, caret_rect_for_mode work
- Modifying editor-related functionality

## Key Files

| File | Symbols |
|------|---------|
| `src/render/renderer/helpers.rs` | gutter_width_for_editor, caret_rect_for_mode, is_mode_block_cursor, should_draw_block_cursor |
| `src/app/app_state/state.rs` | line_string, indent_config, current_overlays, completion |
| `src/render/renderer/editor.rs` | editor_viewport_geometry, wrap_text_lines, wrap_text_lines_keeps_output_non_empty |
| `src/render/renderer/editor/viewport.rs` | spans_fingerprint, update_editor_content, update_editor_caret |
| `src/render/renderer/editor/selections.rs` | leading_indent_columns, indent_guide_quads, current_line_highlight_quad |
| `src/render/renderer/editor/overlays.rs` | update_editor_overlays, blend_rgba, strip_markdown_inline |
| `src/app/app_state/overlays.rs` | revision, is_completion_loading |
| `src/text/layout_sync.rs` | compute_caret_layout |
| `src/app/app_state/editor.rs` | total_lines |
| `src/render/renderer/editor/completion.rs` | completion_label_spans |

## Entry Points

Start here when exploring this area:

- **`compute_caret_layout`** (Function) — `src/text/layout_sync.rs:206`
- **`gutter_width_for_editor`** (Function) — `src/render/renderer/helpers.rs:194`
- **`caret_rect_for_mode`** (Function) — `src/render/renderer/helpers.rs:266`
- **`is_mode_block_cursor`** (Function) — `src/render/renderer/helpers.rs:344`
- **`should_draw_block_cursor`** (Function) — `src/render/renderer/helpers.rs:348`

## Key Symbols

| Symbol | Type | File | Line |
|--------|------|------|------|
| `compute_caret_layout` | Function | `src/text/layout_sync.rs` | 206 |
| `gutter_width_for_editor` | Function | `src/render/renderer/helpers.rs` | 194 |
| `caret_rect_for_mode` | Function | `src/render/renderer/helpers.rs` | 266 |
| `is_mode_block_cursor` | Function | `src/render/renderer/helpers.rs` | 344 |
| `should_draw_block_cursor` | Function | `src/render/renderer/helpers.rs` | 348 |
| `editor_viewport_geometry` | Function | `src/render/renderer/editor.rs` | 134 |
| `line_string` | Function | `src/app/app_state/state.rs` | 472 |
| `indent_config` | Function | `src/app/app_state/state.rs` | 777 |
| `revision` | Function | `src/app/app_state/overlays.rs` | 110 |
| `total_lines` | Function | `src/app/app_state/editor.rs` | 900 |
| `update_editor_content` | Function | `src/render/renderer/editor/viewport.rs` | 127 |
| `update_editor_caret` | Function | `src/render/renderer/editor/viewport.rs` | 269 |
| `indent_guide_quads` | Function | `src/render/renderer/editor/selections.rs` | 48 |
| `current_line_highlight_quad` | Function | `src/render/renderer/editor/selections.rs` | 110 |
| `wrap_text_lines` | Function | `src/render/renderer/editor.rs` | 41 |
| `current_overlays` | Function | `src/app/app_state/state.rs` | 705 |
| `completion` | Function | `src/app/app_state/state.rs` | 709 |
| `is_completion_loading` | Function | `src/app/app_state/overlays.rs` | 62 |
| `update_editor_overlays` | Function | `src/render/renderer/editor/overlays.rs` | 33 |
| `completion_label_spans` | Function | `src/render/renderer/editor/completion.rs` | 26 |

## Connected Areas

| Area | Connections |
|------|-------------|
| App_state | 11 calls |
| Renderer | 8 calls |
| Text | 3 calls |
| Ui | 2 calls |
| Theme_config | 1 calls |
| Benches | 1 calls |
| App | 1 calls |
| Workbench | 1 calls |

## How to Explore

1. `gitnexus_context({name: "compute_caret_layout"})` — see callers and callees
2. `gitnexus_query({query: "editor"})` — find related execution flows
3. Read key files listed above for implementation details
