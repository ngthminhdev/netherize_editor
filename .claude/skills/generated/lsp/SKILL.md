---
name: lsp
description: "Skill for the Lsp area of netherize_editor. 91 symbols across 10 files."
---

# Lsp

91 symbols | 10 files | Cohesion: 75%

## When to Use

- Working with code in `src/`
- Understanding how language_profile_for_language_id, is_document_open, mark_document_open work
- Modifying lsp-related functionality

## Key Files

| File | Symbols |
|------|---------|
| `src/lsp/client.rs` | is_document_open, mark_document_open, mark_document_closed, build_did_open_notification, build_did_change_notification (+57) |
| `src/lsp/registry.rs` | language_profile_for_language_id, language_profile_for_path, language_profile_detects_dockerfile_by_filename, language_profile_detects_dockerfile_variants, language_profile_detects_sql_by_extension (+7) |
| `src/async_runtime/scheduler.rs` | get_by_binary, get_handle, get_handle_by_uri, take_any, drain_all (+1) |
| `src/async_runtime/scheduler/lsp_parse.rs` | parse_locations, handle_lsp_definition, handle_lsp_references, lsp_request_response |
| `src/async_runtime/scheduler/lsp_io.rs` | spawn_lsp_stderr_logger, spawn_lsp_stdout_reader |
| `src/async_runtime/scheduler/lsp.rs` | execute_lsp_request |
| `src/syntax/parser.rs` | language_id_for_path |
| `src/app/app_state/overlays.rs` | register_open_text_buffer |
| `src/app/event_loop/setup.rs` | submit_lsp_check_for_path |
| `src/app/event_loop/helpers.rs` | language_id_for_path |

## Entry Points

Start here when exploring this area:

- **`language_profile_for_language_id`** (Function) — `src/lsp/registry.rs:208`
- **`is_document_open`** (Function) — `src/lsp/client.rs:459`
- **`mark_document_open`** (Function) — `src/lsp/client.rs:466`
- **`mark_document_closed`** (Function) — `src/lsp/client.rs:472`
- **`build_did_open_notification`** (Function) — `src/lsp/client.rs:853`

## Key Symbols

| Symbol | Type | File | Line |
|--------|------|------|------|
| `language_profile_for_language_id` | Function | `src/lsp/registry.rs` | 208 |
| `is_document_open` | Function | `src/lsp/client.rs` | 459 |
| `mark_document_open` | Function | `src/lsp/client.rs` | 466 |
| `mark_document_closed` | Function | `src/lsp/client.rs` | 472 |
| `build_did_open_notification` | Function | `src/lsp/client.rs` | 853 |
| `build_did_change_notification` | Function | `src/lsp/client.rs` | 869 |
| `build_did_close_notification` | Function | `src/lsp/client.rs` | 881 |
| `get_by_binary` | Function | `src/async_runtime/scheduler.rs` | 103 |
| `get_handle` | Function | `src/async_runtime/scheduler.rs` | 117 |
| `get_handle_by_uri` | Function | `src/async_runtime/scheduler.rs` | 128 |
| `take_any` | Function | `src/async_runtime/scheduler.rs` | 139 |
| `drain_all` | Function | `src/async_runtime/scheduler.rs` | 167 |
| `handle_lsp_definition` | Function | `src/async_runtime/scheduler/lsp_parse.rs` | 227 |
| `handle_lsp_references` | Function | `src/async_runtime/scheduler/lsp_parse.rs` | 292 |
| `spawn_lsp_stderr_logger` | Function | `src/async_runtime/scheduler/lsp_io.rs` | 133 |
| `send_notification` | Function | `src/lsp/client.rs` | 378 |
| `send_request` | Function | `src/lsp/client.rs` | 398 |
| `send_request_with_id` | Function | `src/lsp/client.rs` | 427 |
| `write_json_rpc_message_async` | Function | `src/lsp/client.rs` | 742 |
| `read_json_rpc_message_async` | Function | `src/lsp/client.rs` | 677 |

## Execution Flows

| Flow | Type | Steps |
|------|------|-------|
| `Spawn_lsp_server → Find_node` | cross_community | 7 |
| `Spawn_lsp_server → FlatRegion` | cross_community | 7 |
| `Run_pty_request → FlatRegion` | cross_community | 7 |
| `Execute_lsp_request → Parse_go_version` | cross_community | 6 |
| `Run_lsp_request → All_language_profiles` | cross_community | 6 |
| `Run_lsp_request → Find_node` | cross_community | 6 |
| `Execute_lsp_request → Resolve_nvm_bin` | cross_community | 5 |
| `Execute_lsp_request → FlatRegion` | cross_community | 5 |
| `Submit_lsp_did_open_for_active_file → Find_node` | cross_community | 5 |
| `Submit_lsp_did_change_for_active_file → Find_node` | cross_community | 5 |

## Connected Areas

| Area | Connections |
|------|-------------|
| Workbench | 12 calls |
| Scheduler | 5 calls |
| Event_loop | 2 calls |
| App_state | 1 calls |

## How to Explore

1. `gitnexus_context({name: "language_profile_for_language_id"})` — see callers and callees
2. `gitnexus_query({query: "lsp"})` — find related execution flows
3. Read key files listed above for implementation details
