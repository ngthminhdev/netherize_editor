---
name: lsp
description: "Skill for the Lsp area of netherize_editor. 106 symbols across 12 files."
---

# Lsp

106 symbols | 12 files | Cohesion: 75%

## When to Use

- Working with code in `src/`
- Understanding how language_profile_for_language_id, is_document_open, mark_document_open work
- Modifying lsp-related functionality

## Key Files

| File | Symbols |
|------|---------|
| `src/lsp/client.rs` | is_document_open, mark_document_open, mark_document_closed, build_did_open_notification, build_did_change_notification (+67) |
| `src/lsp/registry.rs` | language_profile_for_language_id, language_profile_for_path, language_profile_detects_dockerfile_by_filename, language_profile_detects_dockerfile_variants, language_profile_detects_sql_by_extension (+9) |
| `src/async_runtime/scheduler.rs` | get_by_binary, get_handle, get_handle_by_uri, take_any, drain_all (+1) |
| `src/async_runtime/scheduler/lsp_parse.rs` | lsp_request_response, lsp_cancellable_request_response, parse_locations, handle_lsp_definition, handle_lsp_references |
| `src/async_runtime/scheduler/lsp_io.rs` | spawn_lsp_stderr_logger, spawn_lsp_stdout_reader |
| `src/async_runtime/scheduler/lsp.rs` | execute_lsp_request |
| `src/syntax/parser.rs` | language_id_for_path |
| `src/app/event_loop/setup.rs` | submit_lsp_check_for_path |
| `src/app/event_loop/helpers.rs` | language_id_for_path |
| `src/app/app_state/overlays.rs` | register_open_text_buffer |

## Entry Points

Start here when exploring this area:

- **`language_profile_for_language_id`** (Function) — `src/lsp/registry.rs:258`
- **`is_document_open`** (Function) — `src/lsp/client.rs:539`
- **`mark_document_open`** (Function) — `src/lsp/client.rs:546`
- **`mark_document_closed`** (Function) — `src/lsp/client.rs:552`
- **`build_did_open_notification`** (Function) — `src/lsp/client.rs:1014`

## Key Symbols

| Symbol | Type | File | Line |
|--------|------|------|------|
| `language_profile_for_language_id` | Function | `src/lsp/registry.rs` | 258 |
| `is_document_open` | Function | `src/lsp/client.rs` | 539 |
| `mark_document_open` | Function | `src/lsp/client.rs` | 546 |
| `mark_document_closed` | Function | `src/lsp/client.rs` | 552 |
| `build_did_open_notification` | Function | `src/lsp/client.rs` | 1014 |
| `build_did_change_notification` | Function | `src/lsp/client.rs` | 1030 |
| `build_did_close_notification` | Function | `src/lsp/client.rs` | 1042 |
| `get_by_binary` | Function | `src/async_runtime/scheduler.rs` | 104 |
| `get_handle` | Function | `src/async_runtime/scheduler.rs` | 118 |
| `get_handle_by_uri` | Function | `src/async_runtime/scheduler.rs` | 129 |
| `take_any` | Function | `src/async_runtime/scheduler.rs` | 140 |
| `drain_all` | Function | `src/async_runtime/scheduler.rs` | 168 |
| `spawn_lsp_stderr_logger` | Function | `src/async_runtime/scheduler/lsp_io.rs` | 185 |
| `swap_inflight` | Function | `src/lsp/client.rs` | 387 |
| `clear_inflight_if_matches` | Function | `src/lsp/client.rs` | 394 |
| `allocate_request_id` | Function | `src/lsp/client.rs` | 484 |
| `send_request_with_id` | Function | `src/lsp/client.rs` | 507 |
| `register_pending_request` | Function | `src/lsp/client.rs` | 513 |
| `clear_pending_request` | Function | `src/lsp/client.rs` | 533 |
| `lsp_request_response` | Function | `src/async_runtime/scheduler/lsp_parse.rs` | 18 |

## Execution Flows

| Flow | Type | Steps |
|------|------|-------|
| `Spawn_lsp_server → Find_node` | cross_community | 7 |
| `Spawn_lsp_server → FlatRegion` | cross_community | 7 |
| `Run_pty_request → FlatRegion` | cross_community | 7 |
| `Startup_subsystems → Find_node` | cross_community | 6 |
| `Handle_explorer_and_workspace_command → Login_shell_path_cache` | cross_community | 6 |
| `Handle_explorer_and_workspace_command → Probe_path_from_login_shell` | cross_community | 6 |
| `Handle_explorer_and_workspace_command → Resolve_nvm_bin` | cross_community | 6 |
| `Run_lsp_request → All_language_profiles` | cross_community | 6 |
| `Run_lsp_request → Find_node` | cross_community | 6 |
| `Run_lsp_request → Login_shell_path_cache` | cross_community | 6 |

## Connected Areas

| Area | Connections |
|------|-------------|
| Scheduler | 12 calls |
| Workbench | 10 calls |
| Event_loop | 2 calls |
| Terminal | 1 calls |
| App_state | 1 calls |

## How to Explore

1. `gitnexus_context({name: "language_profile_for_language_id"})` — see callers and callees
2. `gitnexus_query({query: "lsp"})` — find related execution flows
3. Read key files listed above for implementation details
