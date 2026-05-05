---
name: cluster-3
description: "Skill for the Cluster_3 area of netherize_editor. 11 symbols across 1 files."
---

# Cluster_3

11 symbols | 1 files | Cohesion: 60%

## When to Use

- Working with code in `src/`
- Understanding how current_position, delete_backward, append_after_cursor work
- Modifying cluster_3-related functionality

## Key Files

| File | Symbols |
|------|---------|
| `src/editor_core.rs` | current_position, delete_backward, append_after_cursor, move_to_line_end, move_to_first_non_whitespace (+6) |

## Entry Points

Start here when exploring this area:

- **`current_position`** (Function) — `src/editor_core.rs:45`
- **`delete_backward`** (Function) — `src/editor_core.rs:64`
- **`append_after_cursor`** (Function) — `src/editor_core.rs:175`
- **`move_to_line_end`** (Function) — `src/editor_core.rs:206`
- **`move_to_first_non_whitespace`** (Function) — `src/editor_core.rs:219`

## Key Symbols

| Symbol | Type | File | Line |
|--------|------|------|------|
| `current_position` | Function | `src/editor_core.rs` | 45 |
| `delete_backward` | Function | `src/editor_core.rs` | 64 |
| `append_after_cursor` | Function | `src/editor_core.rs` | 175 |
| `move_to_line_end` | Function | `src/editor_core.rs` | 206 |
| `move_to_first_non_whitespace` | Function | `src/editor_core.rs` | 219 |
| `insert_at_line_start` | Function | `src/editor_core.rs` | 278 |
| `append_at_line_end` | Function | `src/editor_core.rs` | 282 |
| `substitute_line` | Function | `src/editor_core.rs` | 286 |
| `delete_char_at_cursor` | Function | `src/editor_core.rs` | 306 |
| `replace_char_at_cursor` | Function | `src/editor_core.rs` | 414 |
| `line_content_end_char_idx` | Function | `src/editor_core.rs` | 522 |

## Connected Areas

| Area | Connections |
|------|-------------|
| App_state | 5 calls |
| Cluster_4 | 1 calls |

## How to Explore

1. `gitnexus_context({name: "current_position"})` — see callers and callees
2. `gitnexus_query({query: "cluster_3"})` — find related execution flows
3. Read key files listed above for implementation details
