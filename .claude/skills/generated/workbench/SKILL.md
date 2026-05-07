---
name: workbench
description: "Skill for the Workbench area of netherize_editor. 110 symbols across 17 files."
---

# Workbench

110 symbols | 17 files | Cohesion: 87%

## When to Use

- Working with code in `src/`
- Understanding how flatten, find, from_ui_theme work
- Modifying workbench-related functionality

## Key Files

| File | Symbols |
|------|---------|
| `src/workbench/layout_engine.rs` | default, from_ui_theme, new, status_bar_top_gap, compute (+21) |
| `src/workbench/panel_state.rs` | active_tab_id, active_tab_label, new, switch_to_prev_tab, default (+12) |
| `src/workbench/inspector_panel.rs` | title, visible_rows, move_selection_next, toggle_selected_expand, selected_row_label (+10) |
| `src/workbench/text_coordinate_map.rs` | from_text, map_line_column_to_rect, map_source_location_rect, gutter_marker_rect, maps_line_column_to_stable_pixel_rect (+7) |
| `src/workbench/focus_manager.rs` | default, set, ensure_valid, cycle_next, cycle_prev (+6) |
| `src/workbench/region_model.rs` | flatten, flatten_node, find, find_node, new (+2) |
| `src/workbench/overlay_manager.rs` | default, build_overlays, clamp_to_bounds, builds_both_window_and_editor_relative_overlays |
| `src/workbench/debug_state.rs` | default, toggle_breakpoint_on_execution_line, toggle_breakpoint_at_line, toggle_breakpoint_adds_and_removes_on_same_line |
| `src/app/event_loop/mod.rs` | active_terminal_grid_mut, focused_terminal_grid_mut, focused_terminal_session_id |
| `src/app/event_loop/commands_tests.rs` | explorer_filter_commands_update_workspace_state, leap_uses_editor_targets_even_when_explorer_is_focused |

## Entry Points

Start here when exploring this area:

- **`flatten`** (Function) — `src/workbench/region_model.rs:103`
- **`find`** (Function) — `src/workbench/region_model.rs:120`
- **`from_ui_theme`** (Function) — `src/workbench/layout_engine.rs:87`
- **`new`** (Function) — `src/workbench/layout_engine.rs:114`
- **`compute`** (Function) — `src/workbench/layout_engine.rs:125`

## Key Symbols

| Symbol | Type | File | Line |
|--------|------|------|------|
| `flatten` | Function | `src/workbench/region_model.rs` | 103 |
| `find` | Function | `src/workbench/region_model.rs` | 120 |
| `from_ui_theme` | Function | `src/workbench/layout_engine.rs` | 87 |
| `new` | Function | `src/workbench/layout_engine.rs` | 114 |
| `compute` | Function | `src/workbench/layout_engine.rs` | 125 |
| `apply_handle_drag` | Function | `src/workbench/layout_engine.rs` | 391 |
| `language_profile_for_extension` | Function | `src/lsp/registry.rs` | 200 |
| `lsp_entry_for_extension` | Function | `src/lsp/client.rs` | 42 |
| `new` | Function | `src/render/surface.rs` | 13 |
| `set` | Function | `src/workbench/focus_manager.rs` | 48 |
| `ensure_valid` | Function | `src/workbench/focus_manager.rs` | 56 |
| `cycle_next` | Function | `src/workbench/focus_manager.rs` | 68 |
| `cycle_prev` | Function | `src/workbench/focus_manager.rs` | 72 |
| `set_from_click` | Function | `src/workbench/focus_manager.rs` | 98 |
| `from_text` | Function | `src/workbench/text_coordinate_map.rs` | 13 |
| `map_line_column_to_rect` | Function | `src/workbench/text_coordinate_map.rs` | 75 |
| `map_source_location_rect` | Function | `src/workbench/text_coordinate_map.rs` | 97 |
| `gutter_marker_rect` | Function | `src/workbench/text_coordinate_map.rs` | 101 |
| `new` | Function | `src/workbench/region_model.rs` | 42 |
| `build_overlays` | Function | `src/workbench/overlay_manager.rs` | 83 |

## Execution Flows

| Flow | Type | Steps |
|------|------|-------|
| `Save_file_preserves_cursor_and_selection_state → Find_node` | cross_community | 9 |
| `Self_save_modify_event_is_ignored_without_reloading_cursor → Find_node` | cross_community | 9 |
| `External_reload_clamps_cursor_and_selection_to_new_buffer_length → Find_node` | cross_community | 9 |
| `Scenario_insert_and_scroll → Find_node` | cross_community | 9 |
| `Spawn_lsp_server → Find_node` | cross_community | 7 |
| `Spawn_lsp_server → FlatRegion` | cross_community | 7 |
| `Picker_open_query_select_flow → Find_node` | cross_community | 7 |
| `Run_pty_request → FlatRegion` | cross_community | 7 |
| `Run_lsp_request → Find_node` | cross_community | 6 |
| `Execute_lsp_request → FlatRegion` | cross_community | 5 |

## Connected Areas

| Area | Connections |
|------|-------------|
| App_state | 2 calls |
| Command_dispatch | 2 calls |

## How to Explore

1. `gitnexus_context({name: "flatten"})` — see callers and callees
2. `gitnexus_query({query: "workbench"})` — find related execution flows
3. Read key files listed above for implementation details
