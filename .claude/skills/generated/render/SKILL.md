---
name: render
description: "Skill for the Render area of netherize_editor. 26 symbols across 8 files."
---

# Render

26 symbols | 8 files | Cohesion: 94%

## When to Use

- Working with code in `src/`
- Understanding how new, new, new work
- Modifying render-related functionality

## Key Files

| File | Symbols |
|------|---------|
| `src/render/caret.rs` | new, new, update_screen_size, from_rect, upload_caret (+2) |
| `src/render/text_pipeline.rs` | new, new, update_screen_size, draw, draw_range |
| `src/render/region_pipeline.rs` | layout, new, upload_instances, ensure_instance_capacity |
| `src/render/image_pipeline.rs` | layout, new, clear, upload_rgba |
| `src/render/pipeline.rs` | layout, new |
| `src/text/atlas.rs` | view, sampler |
| `src/render/color_space.rs` | srgb_color_target_state |
| `src/render/renderer.rs` | update_caret_visibility |

## Entry Points

Start here when exploring this area:

- **`new`** (Function) — `src/render/region_pipeline.rs:99`
- **`new`** (Function) — `src/render/pipeline.rs:51`
- **`new`** (Function) — `src/render/image_pipeline.rs:38`
- **`srgb_color_target_state`** (Function) — `src/render/color_space.rs:13`
- **`new`** (Function) — `src/render/caret.rs:134`

## Key Symbols

| Symbol | Type | File | Line |
|--------|------|------|------|
| `new` | Function | `src/render/region_pipeline.rs` | 99 |
| `new` | Function | `src/render/pipeline.rs` | 51 |
| `new` | Function | `src/render/image_pipeline.rs` | 38 |
| `srgb_color_target_state` | Function | `src/render/color_space.rs` | 13 |
| `new` | Function | `src/render/caret.rs` | 134 |
| `update_screen_size` | Function | `src/render/caret.rs` | 234 |
| `view` | Function | `src/text/atlas.rs` | 89 |
| `sampler` | Function | `src/text/atlas.rs` | 93 |
| `new` | Function | `src/render/text_pipeline.rs` | 51 |
| `update_screen_size` | Function | `src/render/text_pipeline.rs` | 189 |
| `update_caret_visibility` | Function | `src/render/renderer.rs` | 315 |
| `upload_caret` | Function | `src/render/caret.rs` | 240 |
| `upload_carets` | Function | `src/render/caret.rs` | 252 |
| `set_caret_visible` | Function | `src/render/caret.rs` | 270 |
| `draw` | Function | `src/render/text_pipeline.rs` | 210 |
| `draw_range` | Function | `src/render/text_pipeline.rs` | 220 |
| `upload_instances` | Function | `src/render/region_pipeline.rs` | 213 |
| `clear` | Function | `src/render/image_pipeline.rs` | 117 |
| `upload_rgba` | Function | `src/render/image_pipeline.rs` | 123 |
| `layout` | Function | `src/render/region_pipeline.rs` | 35 |

## How to Explore

1. `gitnexus_context({name: "new"})` — see callers and callees
2. `gitnexus_query({query: "render"})` — find related execution flows
3. Read key files listed above for implementation details
