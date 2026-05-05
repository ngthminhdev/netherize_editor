---
name: cluster-1
description: "Skill for the Cluster_1 area of netherize_editor. 8 symbols across 1 files."
---

# Cluster_1

8 symbols | 1 files | Cohesion: 89%

## When to Use

- Working with code in `src/`
- Understanding how new, insert_char, insert_newline work
- Modifying cluster_1-related functionality

## Key Files

| File | Symbols |
|------|---------|
| `src/editor_core.rs` | new, insert_char, insert_newline, open_file, save_file_as (+3) |

## Entry Points

Start here when exploring this area:

- **`new`** (Function) — `src/editor_core.rs:18`
- **`insert_char`** (Function) — `src/editor_core.rs:52`
- **`insert_newline`** (Function) — `src/editor_core.rs:60`
- **`open_file`** (Function) — `src/editor_core.rs:484`
- **`save_file_as`** (Function) — `src/editor_core.rs:509`

## Key Symbols

| Symbol | Type | File | Line |
|--------|------|------|------|
| `new` | Function | `src/editor_core.rs` | 18 |
| `insert_char` | Function | `src/editor_core.rs` | 52 |
| `insert_newline` | Function | `src/editor_core.rs` | 60 |
| `open_file` | Function | `src/editor_core.rs` | 484 |
| `save_file_as` | Function | `src/editor_core.rs` | 509 |
| `unique_temp_path` | Function | `src/editor_core.rs` | 682 |
| `open_and_save_roundtrip` | Function | `src/editor_core.rs` | 777 |
| `filetype_label_uses_active_file_extension` | Function | `src/editor_core.rs` | 886 |

## Connected Areas

| Area | Connections |
|------|-------------|
| Cluster_3 | 1 calls |

## How to Explore

1. `gitnexus_context({name: "new"})` — see callers and callees
2. `gitnexus_query({query: "cluster_1"})` — find related execution flows
3. Read key files listed above for implementation details
