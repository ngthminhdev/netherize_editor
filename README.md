# Netherize Editor

A GPU-accelerated terminal/text editor written in Rust. Currently in active development (Module 12 / Phase 2–3).

---

## Quick Status

| Area | Status |
|------|--------|
| Editor buffer + cursor | ✅ Stable, fully tested |
| Vim-style mode system | ✅ Working (Normal / Insert / Visual / PaletteFocus / TerminalFocus) |
| GPU renderer (wgpu) | ✅ Renders text, cursor, gutter, sidebar, statusbar |
| Glyph atlas (texture packing) | ✅ Shelf-packer, uploaded to GPU |
| Text shaping (cosmic-text) | ✅ Rich text + syntax spans |
| Syntax highlighting (tree-sitter) | ✅ Rust grammar wired |
| Workbench layout engine | ✅ Region-based, resizable splits |
| File explorer sidebar | ✅ Tree with j/k/h/l/Enter navigation |
| Embedded terminal (PTY) | ✅ ANSI parser + grid |
| LSP client | 🚧 Skeleton (didOpen wired, partial) |
| Config / theme (TOML runtime) | ✅ Hot-reloadable at startup |
| Command palette | ✅ Overlay UI working |
| File picker (fuzzy) | ✅ Working |
| Multi-buffer | ✅ Buffer ring (next/prev/close) |

---

## Running

```sh
cargo run
```

```sh
cargo test -q          # 128 tests, all pass
cargo bench            # criterion benchmarks in benches/
```

---

## Repository Layout

```
netherize_editor/
├── src/
│   ├── main.rs                    # Entry point — delegates to app::event_loop::run()
│   ├── lib.rs                     # Module re-exports
│   ├── editor_core.rs             # Legacy EditorBuffer (ropey-backed), kept for reference
│   │
│   ├── app/                       # Application shell
│   │   ├── app_state.rs           # AppState — authoritative editor state (text, cursor, mode, buffers)
│   │   ├── event_loop/            # winit ApplicationHandler impl + command dispatch
│   │   │   ├── mod.rs             # run() entrypoint
│   │   │   ├── application.rs     # winit::ApplicationHandler impl
│   │   │   ├── commands.rs        # Command → AppState mutation handlers
│   │   │   ├── async_results.rs   # Polling async channel results into the main loop
│   │   │   ├── helpers.rs         # Shared render/layout helpers
│   │   │   ├── setup.rs           # GPU + window init
│   │   │   └── welcome.rs         # Welcome screen logic
│   │   ├── input_map/             # Keymap resolution (key event → Command)
│   │   │   ├── mod.rs
│   │   │   ├── focus.rs           # Focus-context-aware key routing
│   │   │   ├── helpers.rs
│   │   │   └── tests.rs
│   │   ├── input.rs               # Raw key event normalization
│   │   ├── resolved_keymap.rs     # Merged keymap at runtime
│   │   ├── command_palette.rs     # Command palette state + filtering
│   │   ├── file_picker.rs         # File picker state + fuzzy results
│   │   └── async_bridge.rs        # Tokio ↔ winit message bridge
│   │
│   ├── core/                      # Editor semantics (mode, commands)
│   │   ├── mode.rs                # EditorMode enum + ModeState transition machine
│   │   ├── commands.rs            # Command enum (all editor actions)
│   │   ├── command_ids.rs         # Stable string IDs for palette lookup
│   │   ├── command_dispatch.rs    # Routes Command → handler
│   │   └── mod.rs
│   │
│   ├── render/                    # GPU rendering layer (wgpu)
│   │   ├── renderer.rs            # Renderer struct — orchestrates all render passes
│   │   ├── pipeline.rs            # Generic wgpu pipeline builder
│   │   ├── text_pipeline.rs       # Glyph-instance pipeline (text quads)
│   │   ├── region_pipeline.rs     # Colored quad pipeline (backgrounds, highlights)
│   │   ├── caret.rs               # Cursor/caret rendering
│   │   ├── glyph_instance.rs      # GlyphInstance vertex layout
│   │   ├── surface.rs             # wgpu Surface + swapchain management
│   │   ├── shaders/               # WGSL shader sources
│   │   └── mod.rs
│   │
│   ├── text/                      # Text shaping + atlas
│   │   ├── text_system.rs         # TextSystem — wraps cosmic-text Buffer + FontSystem
│   │   ├── atlas.rs               # GlyphAtlas — shelf-packing texture atlas on GPU
│   │   ├── raster.rs              # RasterizedGlyph — swash image → alpha bytes
│   │   ├── layout_sync.rs         # Syncs editor content → TextSystem on change
│   │   └── mod.rs
│   │
│   ├── syntax/                    # Syntax highlighting
│   │   ├── syntax_engine.rs       # tree-sitter parse + highlight span extraction
│   │   ├── highlight.rs           # Highlight → StyledTextSpan mapping
│   │   └── mod.rs
│   │
│   ├── workbench/                 # UI layout + panel management
│   │   ├── layout_engine.rs       # WorkbenchLayoutEngine — computes RegionModel from panel sizes
│   │   ├── region_model.rs        # RegionId / RegionBounds / RegionModel tree
│   │   ├── panel_state.rs         # Sidebar/bottom panel open-state + sizes
│   │   ├── focus_manager.rs       # Which region currently holds keyboard focus
│   │   ├── overlay_manager.rs     # Overlay stack (palette, picker, etc.)
│   │   ├── inspector_panel.rs     # Right sidebar inspector content
│   │   ├── text_coordinate_map.rs # Screen pixel ↔ editor char-index mapping
│   │   ├── debug_state.rs         # Debug overlay lines
│   │   └── mod.rs
│   │
│   ├── workspace/                 # File system workspace
│   │   ├── scanner.rs             # Recursive directory scan → WorkspaceNode tree
│   │   ├── model.rs               # WorkspaceModel (node tree + path index)
│   │   ├── fuzzy.rs               # Fuzzy file search
│   │   └── mod.rs
│   │
│   ├── terminal/                  # Embedded PTY terminal
│   │   ├── pty.rs                 # portable-pty spawn + read/write
│   │   ├── ansi_parser.rs         # ANSI escape sequence parser
│   │   ├── grid.rs                # Terminal cell grid + scrollback
│   │   ├── terminal_renderer.rs   # Renders terminal grid via TextSystem
│   │   └── mod.rs
│   │
│   ├── lsp/                       # Language Server Protocol client
│   │   ├── client.rs              # LSP process spawn + JSON-RPC (partial)
│   │   └── mod.rs
│   │
│   ├── async_runtime/             # Tokio async bridge
│   │   ├── scheduler.rs           # Task queue + wakeup
│   │   ├── message.rs             # AsyncMessage enum (results sent back to main loop)
│   │   └── mod.rs
│   │
│   └── config/                    # Config loading
│       ├── theme_config.rs        # ThemeConfig — parsed from TOML, used by Renderer
│       ├── ui_config.rs           # UiConfig — layout sizes, cursor style, padding
│       ├── keymap_config.rs       # KeymapConfig — raw key binding table
│       ├── keymap_loader.rs       # Loads + merges keymap TOML files
│       └── mod.rs
│
├── config/
│   ├── themes/
│   │   └── default-dark.toml      # Color theme ([editor], [ui], [syntax] sections)
│   ├── ui/
│   │   └── default.toml           # Layout sizes, cursor shape, padding, dock visibility
│   └── keymaps/
│       ├── default.toml           # Default keymap (VSCode-ish)
│       ├── nvim.toml              # Neovim-style keymap
│       └── min.toml               # Minimal keymap
│
├── docs/
│   ├── MODULE12_HANDOFF_COMPACT.md  # Handoff notes for Module 12 (Phase 2+3)
│   └── perf_profiling.md
│
├── benches/
│   └── editor_bench.rs            # Criterion benchmarks
│
└── Cargo.toml
```

---

## Architecture: How Data Flows

```
User presses key
      │
      ▼
winit KeyEvent  (app/event_loop/application.rs)
      │
      ▼
input_map::resolve()  →  Command enum  (app/input_map/)
      │
      ▼
command_dispatch()  →  AppState mutation  (app/event_loop/commands.rs)
      │
      ▼
AppState  (app/app_state.rs)
  ├── text: Rope              (ropey)
  ├── cursor_char_idx
  ├── mode_state: ModeState   (core/mode.rs)
  ├── open_buffers: Vec<...>
  └── workspace_model
      │
      ▼
layout_sync → TextSystem.set_text_with_spans()  (text/layout_sync.rs)
      │
      ▼
syntax_engine → StyledTextSpan[]  (syntax/syntax_engine.rs)
      │
      ▼
Renderer.update_editor_content()  (render/renderer.rs)
  ├── TextSystem.collect_visible_glyphs()
  ├── GlyphAtlas.get_or_insert()  →  GPU texture upload
  └── GlyphInstance[] → vertex buffer
      │
      ▼
Renderer.render()  →  wgpu submit  →  screen
```

---

## Key Structs at a Glance

| Struct | File | Role |
|--------|------|------|
| `AppState` | `app/app_state.rs` | Central editor state (text, cursor, mode, buffers, workspace) |
| `ModeState` | `core/mode.rs` | Vim-style mode FSM with return-mode memory |
| `Command` | `core/commands.rs` | All possible editor actions as an enum |
| `Renderer` | `render/renderer.rs` | Owns all wgpu resources + orchestrates render passes |
| `TextSystem` | `text/text_system.rs` | Wraps cosmic-text Buffer; shapes + collects glyphs |
| `GlyphAtlas` | `text/atlas.rs` | GPU texture atlas with shelf-packing |
| `WorkbenchLayoutEngine` | `workbench/layout_engine.rs` | Computes pixel bounds for all UI regions |
| `RegionModel` | `workbench/region_model.rs` | Tree of named, bounded UI regions |
| `FocusManager` | `workbench/focus_manager.rs` | Tracks which region has keyboard focus |

---

## Mode System

```
Normal ──(i/a/o/O/s/S/A)──► Insert
Normal ──(v)──────────────► Visual
Normal ──(Ctrl+P)─────────► PaletteFocus
Normal ──(Ctrl+`)─────────► TerminalFocus

Insert ──(Esc)────────────► Normal
Visual ──(Esc)────────────► Normal

PaletteFocus ──(Esc/ExitFocus)──► [return to previous mode]
TerminalFocus ──(Esc)───────────► [return to previous mode]
```

Mode transitions are validated by `ModeState::apply(event)` — invalid transitions return `Err`.

---

## Workbench Regions

```
┌─────────────────────────────────────────────┐
│                  TopBar                      │
├──────────┬──────────────────────┬────────────┤
│          │                      │            │
│ Left     │       Center         │  Right     │
│ Sidebar  │      (Editor)        │  Sidebar   │
│(Explorer)│                      │(Inspector) │
│          │                      │            │
│          ├──────────────────────┤            │
│          │    BottomPanel       │            │
│          │    (Terminal)        │            │
├──────────┴──────────────────────┴────────────┤
│                 StatusBar                    │
└─────────────────────────────────────────────┘
```

Layout is computed by `WorkbenchLayoutEngine::compute(viewport, panel_state)` and stored as `RegionModel`. The renderer reads `RegionModel` to determine scissor rects and text origins for each panel.

---

## Configuration

All config files are TOML, loaded at startup. Restart required after changes.

### `config/ui/default.toml`

Controls layout geometry and cursor appearance:

```toml
[layout]
top_bar_height = ...
status_bar_height = ...
region_gap = ...

[docks]
left_size_px = ...
right_size_px = ...
bottom_size_px = ...
left_visible = true
right_visible = false
bottom_visible = true

[cursor]
shape = "nvim"    # "nvim" = block, "zed" = beam, "underline"
beam_width = ...
block_width = ...

[spacing]
editor_padding = ...
panel_padding = ...
```

### `config/themes/default-dark.toml`

Controls all colors:

```toml
[editor]
bg = ...
fg = ...

[ui]
sidebar_bg = ...
panel_bg = ...
status_bar_bg = ...
selection_bg = ...

[syntax]
keyword = ...
string = ...
comment = ...
```

### `config/keymaps/default.toml`

Maps key combos to `Command` IDs per mode context.

---

## Render Pipeline (wgpu)

Each frame in `Renderer::render()` runs these passes in order:

1. **Region quads** — background color fills for each visible region (via `region_pipeline`)
2. **Gutter text** — line numbers (via `gutter_text_pipeline`)
3. **Editor text** — syntax-highlighted content (via `text_pipeline`, scissored to Center bounds)
4. **Current line highlight** — colored quad behind active line
5. **Visual selection quads** — highlight quads for selected text
6. **Cursor / caret** — block, beam, or underline shape (via `caret_pipeline`)
7. **Sidebar text** — file explorer tree (via `sidebar_text_pipeline`)
8. **Terminal** — ANSI grid cells (via `terminal_text_pipeline`)
9. **Topbar** — tab/file labels
10. **Statusbar** — mode pill, file path, cursor position, diagnostics
11. **Palette overlay** — command palette or file picker (rendered last, on top)

---

## Dependencies

| Crate | Purpose |
|-------|---------|
| `ropey` | Gap-buffer text rope for efficient insert/delete |
| `winit` | Cross-platform window + keyboard events |
| `wgpu` | GPU rendering (WebGPU API) |
| `cosmic-text` | Font loading, text shaping, glyph rasterization |
| `swash` | Low-level glyph rasterizer used by cosmic-text |
| `tree-sitter` + `tree-sitter-rust` | Incremental syntax parsing for highlighting |
| `portable-pty` | PTY spawn + I/O for the embedded terminal |
| `tokio` | Async runtime for LSP + file watching |
| `notify` | File system watcher (external change detection) |
| `serde` + `serde_json` | JSON-RPC (LSP protocol) |
| `toml` | Config file parsing |
| `sysinfo` | System info for statusbar |
| `bytemuck` | Safe vertex buffer casting |
| `pollster` | Block-on-async for GPU device init |
| `criterion` | Benchmarks (dev-dependency) |

---

## Where to Start Reading (AI / New Contributor)

Read in this order to build a mental model quickly:

1. **`src/core/mode.rs`** — understand the mode FSM first; it's small and self-contained
2. **`src/core/commands.rs`** — see every possible action the editor can perform
3. **`src/app/app_state.rs`** — the central state; understand fields and key methods
4. **`src/workbench/region_model.rs`** + **`layout_engine.rs`** — how the UI is divided
5. **`src/text/text_system.rs`** + **`text/atlas.rs`** — how text goes from string → GPU
6. **`src/render/renderer.rs`** — how all of the above is assembled into a frame
7. **`src/app/event_loop/application.rs`** — the main loop tying it all together

---

## Known Gaps / Next Steps

- **LSP**: `client.rs` spawns the process and sends `didOpen`, but response handling and diagnostics display are not yet wired to the renderer.
- **Scrolling**: vertical scroll works via `scroll_line`; horizontal scroll not yet implemented.
- **Multiple splits**: layout engine supports Left/Center/Right/Bottom but no arbitrary splits yet.
- **Atlas overflow**: when the glyph atlas fills up, new glyphs are dropped silently. Atlas eviction/resize not yet implemented.
- **Undo/redo**: not implemented; `ropey` supports it via `Rope::clone()` snapshots but no history stack exists yet.
- **Visual line mode**: `EnterVisualLine` command exists but selection logic is partial.