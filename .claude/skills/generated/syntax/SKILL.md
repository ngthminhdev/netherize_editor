---
name: syntax
description: "Skill for the Syntax area of netherize_editor. 76 symbols across 7 files."
---

# Syntax

76 symbols | 7 files | Cohesion: 72%

## When to Use

- Working with code in `src/`
- Understanding how as_str, root_node, new work
- Modifying syntax-related functionality

## Key Files

| File | Symbols |
|------|---------|
| `src/syntax/highlight.rs` | generate_highlight_spans, generate_dotenv_highlight_spans, should_highlight_inline, highlight_snippet, generate_injection_highlights (+43) |
| `src/syntax/syntax_engine.rs` | as_str, root_node, new, new_rust, parse_source (+14) |
| `src/app/event_loop/helpers.rs` | build_preview_render_data, parse_markdown_preview_blocks, fallback_markdown_preview |
| `src/syntax/parser.rs` | language_id_for_extension, tree_sitter_language |
| `src/app/event_loop/commands.rs` | reconcile_highlight_spans_with_pending_edits, close_current_buffer_now |
| `benches/editor_bench.rs` | bench_incremental_parse |
| `src/app/event_loop/setup.rs` | refresh_inline_syntax_highlighting |

## Entry Points

Start here when exploring this area:

- **`as_str`** (Function) — `src/syntax/syntax_engine.rs:26`
- **`root_node`** (Function) — `src/syntax/syntax_engine.rs:67`
- **`new`** (Function) — `src/syntax/syntax_engine.rs:90`
- **`new_rust`** (Function) — `src/syntax/syntax_engine.rs:113`
- **`parse_source`** (Function) — `src/syntax/syntax_engine.rs:119`

## Key Symbols

| Symbol | Type | File | Line |
|--------|------|------|------|
| `as_str` | Function | `src/syntax/syntax_engine.rs` | 26 |
| `root_node` | Function | `src/syntax/syntax_engine.rs` | 67 |
| `new` | Function | `src/syntax/syntax_engine.rs` | 90 |
| `new_rust` | Function | `src/syntax/syntax_engine.rs` | 113 |
| `parse_source` | Function | `src/syntax/syntax_engine.rs` | 119 |
| `parse_incremental` | Function | `src/syntax/syntax_engine.rs` | 143 |
| `current_tree` | Function | `src/syntax/syntax_engine.rs` | 197 |
| `language_id_for_extension` | Function | `src/syntax/parser.rs` | 10 |
| `generate_highlight_spans` | Function | `src/syntax/highlight.rs` | 288 |
| `generate_dotenv_highlight_spans` | Function | `src/syntax/highlight.rs` | 311 |
| `should_highlight_inline` | Function | `src/syntax/highlight.rs` | 381 |
| `highlight_snippet` | Function | `src/syntax/highlight.rs` | 390 |
| `build_preview_render_data` | Function | `src/app/event_loop/helpers.rs` | 213 |
| `parse_markdown_preview_blocks` | Function | `src/app/event_loop/helpers.rs` | 296 |
| `language_id` | Function | `src/syntax/syntax_engine.rs` | 71 |
| `generate_highlight_spans_in_byte_window` | Function | `src/syntax/highlight.rs` | 365 |
| `merge_highlight_spans` | Function | `src/syntax/highlight.rs` | 237 |
| `overlay_highlight_layers` | Function | `src/syntax/highlight.rs` | 272 |
| `tree_sitter_language` | Function | `src/syntax/parser.rs` | 34 |
| `apply_highlight_edits` | Function | `src/syntax/highlight.rs` | 221 |

## Execution Flows

| Flow | Type | Steps |
|------|------|-------|
| `Handle_command_with_count → Tree_sitter_language` | cross_community | 6 |
| `Handle_command_with_count → Root_node` | cross_community | 5 |
| `Handle_palette_and_open_command → As_str` | cross_community | 4 |
| `Handle_palette_and_open_command → Tree_sitter_language` | cross_community | 4 |
| `Handle_palette_and_open_command → Parse` | cross_community | 4 |
| `Handle_palette_and_open_command → New` | cross_community | 4 |
| `Execute_virtual_job → Parse` | cross_community | 4 |
| `Execute_virtual_job → New` | cross_community | 4 |
| `Execute_virtual_job → Sanitize_byte_range` | cross_community | 4 |

## Connected Areas

| Area | Connections |
|------|-------------|
| Event_loop | 5 calls |
| Workbench | 4 calls |
| App | 3 calls |
| Benches | 1 calls |
| Command_dispatch | 1 calls |

## How to Explore

1. `gitnexus_context({name: "as_str"})` — see callers and callees
2. `gitnexus_query({query: "syntax"})` — find related execution flows
3. Read key files listed above for implementation details
