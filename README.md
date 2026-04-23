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
| Syntax highlighting (tree-sitter) | ✅ Polyglot bootstrap (Rust / JS / TS / Go / YAML / JSON / Bash) |
| Workbench layout engine | ✅ Region-based, resizable splits |
| File explorer sidebar | ✅ Tree with j/k/h/l/Enter navigation + theme-correct colors |
| Embedded terminal (PTY) | ✅ ANSI parser + grid |
| LSP client | ✅ Smart root detection + async stdio transport + Mason-style install prompt |
| Config / theme (TOML runtime) | ✅ Repo profiles + user theme discovery + persisted runtime selection |
| Command palette | ✅ Overlay UI with prompt prefix/query color split |
| File picker (fuzzy) | ✅ Matched-char accent highlight + fg_dim labels |
| Multi-buffer | ✅ Buffer ring (next/prev/close) |
| Leap navigation | ✅ EasyMotion-style jump with dim overlay + per-char quad |

---

## Running

```sh
cargo run
```

```sh
cargo test             # full unit + doc test suite
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
│   │   ├── input/                 # Key normalization + pending-state router
│   │   │   ├── mod.rs             # Public input module surface
│   │   │   ├── handler.rs         # Main input state machine / router
│   │   │   ├── model.rs           # NormalizedInput / TranslatedInput
│   │   │   ├── pending.rs         # Pending chord/operator state types
│   │   │   ├── helpers.rs         # Key classification + terminal payload helpers
│   │   │   └── tests.rs           # Input routing regression tests
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
│   ├── lsp/                       # Polyglot language server client
│   │   ├── registry.rs            # File extension/filename → language profile, install command, root markers
│   │   ├── client.rs              # JSON-RPC framing, async stdio transport, didOpen/didChange lifecycle
│   │   └── mod.rs
│   │
│   ├── render/                    # GPU rendering layer (wgpu)
│   │   ├── renderer.rs            # Renderer facade + shared render types
│   │   ├── renderer/
│   │   │   ├── lifecycle.rs       # GPU init, resize, theme/ui config apply, frame submit
│   │   │   ├── editor_render.rs   # Editor/gutter/caret/selection render prep
│   │   │   ├── ui_render.rs       # Sidebar, terminal, topbar, statusbar, welcome UI
│   │   │   ├── palette_render.rs  # Command palette, file picker, leap overlay
│   │   │   └── helpers.rs         # Shared pure helpers for render modules
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
│   │   ├── parser.rs              # Language registry bridge → tree-sitter grammar bootstrap
│   │   ├── syntax_engine.rs       # tree-sitter parser lifecycle for each supported language
│   │   ├── highlight.rs           # Query capture → theme token spans / emphasis flags
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
│       ├── theme_config.rs        # Theme module entrypoint + public re-exports
│       ├── theme_config/
│       │   ├── model.rs           # Public theme tokens + file-icon lookup helpers
│       │   ├── loader.rs          # TOML loading, validation, profile lookup
│       │   ├── raw.rs             # Serde-only structs matching theme TOML
│       │   └── builtin.rs         # Built-in dark fallback theme
│       ├── paths.rs               # Shared user-config / legacy-state path helpers
│       ├── ui_config.rs           # UiConfig — layout sizes, cursor style, padding
│       ├── keymap_config.rs       # KeymapConfig — raw key binding table
│       ├── keymap_loader.rs       # Loads + merges keymap TOML files
│       └── mod.rs
│
├── config/
│   ├── themes/
│   │   └── default-dark.toml      # Theme profile ([theme], [editor], [ui], [syntax], [icons])
│   ├── ui/
│   │   └── default.toml           # Layout sizes, cursor shape, padding, dock visibility
│   └── keymaps/
│       └── default.toml           # Shared repo baseline; local remaps can live in ~/.config/netherize/keymaps/user.toml
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

### Runtime State And User Config

Repo-shipped profiles still live under `config/`, but user-specific state/config now resolves like this:

- `~/.config/netherize/state.toml` — persisted app state (`recent_projects`, selected `theme_profile`)
- `~/.config/netherize/themes/*.toml` — user theme profiles discovered by Theme Selector / `ThemeConfig`
- `~/.config/netherize/keymaps/user.toml` — optional local keymap overrides
- `~/.netherize_editor/state.toml` — legacy state path; current code reads it as fallback and migrates on next save

Theme lookup order is now:

1. `./config/themes`
2. `~/.config/netherize/themes`
3. `~/.netherize_editor/themes` (legacy fallback)
4. `config/themes` next to the built binary

---

## Architecture: How Data Flows

```
User presses key
      │
      ▼
winit KeyEvent  (app/event_loop/application.rs)
      │
      ▼
InputHandler::translate_key_event()
  (app/input/handler.rs)
  - normalize KeyEvent
  - keep pending chord / operator / replace / leap state
  - accumulate numeric counts (1..9 only; 0 falls through as a normal key)
      │
      ▼
InputMap::resolve() / resolve_sequence_*()
  (app/input_map/mod.rs)
  - turn normalized key(s) into Command
  - load merged bindings from resolved keymap
      │
      ▼
TranslatedInput { command, repeat_count }
      │
      ▼
AppShell::handle_command_with_count()
  (app/event_loop/commands.rs)
  - workbench/focus commands
  - terminal open/focus behavior
  - forward repeat_count into editor dispatch
      │
      ▼
dispatch_command_with_clipboard_count()
  (core/command_dispatch.rs)
  - loop repeatable commands
  - group repeated text edits into one undo transaction
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

### Keyboard / Vim Path In One Line

For almost every "why didn't this key do what I expected?" bug, read the path below in order:

`application.rs` -> `app/input/handler.rs` -> `app/input_map/mod.rs` -> `app/resolved_keymap.rs` -> `app/event_loop/commands.rs` -> `core/command_dispatch.rs` -> `app/app_state.rs`

That path is the fastest way to debug:

- missing keybinding
- wrong chord/operator behavior
- numeric counts like `5j`, `d2w`, `3dw`
- terminal focus vs editor focus
- command runs but mutates the wrong state

---

## Where To Fix What

Use this table when you want to jump straight to the likely file instead of reading the whole repo.

| If you want to change... | Start here | Why |
|------|------|------|
| Vim counts, pending operators, chord interruption, `r<char>`, Leap pending states | `src/app/input/handler.rs`, `src/app/input/pending.rs` | Handler owns the state machine; pending types keep the router states readable |
| A shortcut does not fire, or `0/F12/<leader>` maps wrong | `config/keymaps/default.toml`, `src/app/input_map/mod.rs`, `src/app/resolved_keymap.rs` | Binding definition, sequence matching, and merged runtime keymap live here |
| A command should repeat `count` times or should/should not support counts | `src/core/commands.rs`, `src/core/command_dispatch.rs` | Count policy and the actual execution loop are centralized here |
| Undo transaction boundaries for repeated delete/paste/edit commands | `src/core/command_dispatch.rs`, `src/app/app_state.rs` | Dispatch decides when to commit; AppState stores the transaction stack |
| Mode transitions such as Normal/Insert/Visual/TerminalFocus | `src/core/mode.rs`, `src/app/app_state.rs` | `ModeState` validates transitions; `AppState` applies them |
| F12 terminal behavior, focus handoff, explorer/panel focus routing | `src/app/event_loop/commands.rs` | Workbench commands are handled here, not in core dispatch |
| Terminal raw input, ANSI behavior, PTY I/O | `src/app/input/helpers.rs`, `src/app/input/handler.rs`, `src/terminal/pty.rs`, `src/terminal/grid.rs` | Terminal key payload building lives in input helpers, then flows into PTY/grid behavior |
| Sidebar / bottom panel overlap, docking geometry, resize handles | `src/workbench/layout_engine.rs`, `src/workbench/panel_state.rs` | Region bounds and panel sizes come from the workbench layout engine |
| Cursor/caret rendering, terminal cursor visibility, status bar UI | `src/render/caret.rs`, `src/render/renderer/ui_render.rs`, `src/app/event_loop/application.rs` | Render prep happens in UI/caret code, driven by event-loop state |
| Theme token bug or wrong color/icon | `config/themes/default-dark.toml`, `src/config/theme_config/` | Theme data is defined in TOML and validated/loaded in the theme module |
| UI spacing, panel sizes, cursor shape defaults | `config/ui/default.toml`, `src/config/ui_config.rs` | Geometry defaults come from UI config, not from the renderer |

### Three Common Debug Paths

1. Key maps wrong:
   `config/keymaps/default.toml` -> `src/app/resolved_keymap.rs` -> `src/app/input_map/mod.rs`

2. Key maps correctly but behavior is wrong:
   `src/app/input/handler.rs` -> `src/app/event_loop/commands.rs` -> `src/core/command_dispatch.rs`

3. Command succeeds but screen looks wrong:
   `src/app/app_state.rs` -> `src/workbench/layout_engine.rs` -> `src/render/renderer/ui_render.rs`

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
Normal ──(F12)────────────► TerminalFocus

Insert ──(Esc)────────────► Normal
Visual ──(Esc)────────────► Normal

PaletteFocus ──(Esc/ExitFocus)──► [return to previous mode]
TerminalFocus ──(Esc/F12)───────► [return to previous mode]
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

Controls theme metadata, colors, sizes, and file icons.

Theme loading order:

1. `ThemeConfig::load_preferred(...)` reads `NETHERIZE_THEME` if it is set
2. Otherwise it falls back to the persisted `theme_profile` from `~/.config/netherize/state.toml`
3. Otherwise it defaults to `default-dark`
4. The profile name is resolved across the search roots listed above
5. If the file is missing or invalid, the renderer falls back to `ThemeConfig::builtin_dark()`

```toml
[theme]
name = "default-dark"

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

[icons]
explorer_folder_collapsed_marker = "▶"

[icons.rust]
glyph = "\uE7A8"
color = "#FF955C"
```

### `config/keymaps/default.toml`

Maps key combos to `Command` IDs per mode context.

### Active Profiles And Override Flow

Profiles are selected via environment variables at startup:

```sh
NETHERIZE_THEME=default-dark NETHERIZE_PROFILE=default NETHERIZE_UI=default cargo run
```

- `NETHERIZE_THEME` -> theme profile name resolved across repo/user theme search roots
- `NETHERIZE_PROFILE` -> `config/keymaps/<profile>.toml` (repo currently ships `default.toml`)
- `NETHERIZE_UI` -> `config/ui/<profile>.toml`

### Theme Override Workflow

Theme loading is profile-based and now also supports runtime persistence:

1. Copy an existing file such as `config/themes/default-dark.toml`
2. Save it either as a repo profile (`config/themes/my-dark.toml`) or a user profile (`~/.config/netherize/themes/my-dark.toml`)
3. Edit the tokens you want to change
4. Either start the editor with `NETHERIZE_THEME=my-dark cargo run` or pick it from `<Space> t h`
5. When selected from Theme Selector, the active profile is saved to `~/.config/netherize/state.toml` and restored on the next launch unless `NETHERIZE_THEME` overrides it

Example:

```toml
[theme]
name = "my-dark"

[editor]
bg = "#0b1020"
fg = "#d9e2f2"
cursor = "#9be564"
selection = "#1d2a44"
gutter = "#5f6b82"

[ui]
sidebar_bg = "#0f1628"
panel_bg = "#111a30"
terminal_bg = "#0d1424"
status_bar_bg = "#0a1220"
border_color = "#24314d"
accent = "#9be564"
```

Theme profiles are now discovered from both repo and user folders, and Theme Selector shows the source path for each profile so duplicate names are easier to reason about.

### Keymap Override Workflow

Keymaps load in this order, with later layers winning:

1. Built-in Rust defaults
2. Selected profile from `config/keymaps/<profile>.toml` (`default.toml` in this repo unless you add another one later)
3. Optional user overrides from `~/.config/netherize/keymaps/user.toml`

That means repo profiles stay as the shared baseline, while local machine-specific remaps can live in `~/.config/netherize/keymaps/user.toml`.

Example user override file:

```toml
[profile]
name = "user"

[[bindings]]
key = "mod+Shift+p"
command = "app.open_command_palette"

[[bindings]]
mode = "terminal"
key = "F12"
command = "app.toggle_terminal"
```

Terminal shortcut semantics are now intentionally centralized on `F12`:

- If the terminal panel is hidden, `F12` opens it and focuses the terminal
- If the terminal panel is already visible, `F12` focuses the terminal
- If the terminal already has focus, `F12` returns focus to the editor and keeps the panel open

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

1. **`src/core/commands.rs`** — the command surface area; know the verbs first
2. **`src/core/mode.rs`** — understand the mode FSM and focus-return rules
3. **`src/app/input/handler.rs`** — key routing state machine, numeric counts, operator flow
4. **`src/app/input/model.rs`** + **`src/app/input/pending.rs`** — normalized input types and pending-state definitions
5. **`src/app/input_map/mod.rs`** + **`src/app/resolved_keymap.rs`** — how keys become commands
6. **`src/app/event_loop/commands.rs`** — workbench behavior, panel focus, terminal open/focus logic
7. **`src/core/command_dispatch.rs`** — how commands mutate editor state and how undo grouping works
8. **`src/app/app_state.rs`** — the central source of truth for text, cursor, mode, buffers, and transactions
9. **`src/workbench/layout_engine.rs`** — UI region geometry when a bug is visual/layout-related
10. **`src/render/renderer.rs`** + **`src/render/renderer/ui_render.rs`** — frame assembly and UI rendering
11. **`src/text/text_system.rs`** + **`src/text/atlas.rs`** — text shaping/raster path when glyph/render bugs appear

---

## Known Gaps / Next Steps

- **LSP UI**: diagnostics are parsed and logged, but inline squiggles / gutter markers / semantic-token overlays are not rendered yet.
- **Scrolling**: vertical scroll works via `scroll_line`; horizontal scroll not yet implemented.
- **Multiple splits**: layout engine supports Left/Center/Right/Bottom but no arbitrary splits yet.
- **Atlas overflow**: when the glyph atlas fills up, new glyphs are dropped silently. Atlas eviction/resize not yet implemented.
- **Undo/redo**: implemented for normal edit flows, including grouped repeated commands, but coverage is still strongest around editor-core mutations rather than every future UI action.
- **Visual line mode**: `EnterVisualLine` command exists but selection logic is partial.
- **Dockerfile tree-sitter highlight**: Dockerfile is wired in the language registry + LSP install flow, but syntax highlighting still depends on a compatible modern grammar wrapper.

---

## Render Layout Fixes (Module 12 — Phase 2)

### Explorer Tree — `update_sidebar_content`

Field-level color hierarchy now matches theme tokens exactly:

| Element | Token | Color |
|---------|-------|-------|
| Arrow icon (`▶ ▼ ·`) | `ui.fg_ghost` | Muted gray — de-emphasized |
| File/folder (normal) | `ui.fg_dim` | Soft gray — readable but not dominant |
| Selected item (unfocused) | `ui.fg` | Bright white |
| Selected item (focused) | `ui.accent` | Green accent |
| Header label | `ui.fg_ghost` | Same as icon — muted |

Icon and label are rendered as two separate `layout_panel_text` calls so colors can differ within the same row. Y-coordinate uses `current_y` accumulated each node (`current_y += line_h`) to prevent node overlap.

### Gutter (Line Numbers) — `update_editor_gutter`

Gutter quads (background clear + active-line highlight) are uploaded directly inside the function via `region_pipeline.upload_instances()`. Previously the function returned the quads, which caused a compile error when callers didn't collect the return value.

| Element | Token | Color |
|---------|-------|-------|
| Gutter background | `editor.bg` | Clears old frame artifacts |
| Active line highlight | `editor.selection` @ 22% alpha | Subtle row highlight |
| Active line number | `editor.gutter_active` | Bright |
| Other line numbers | `editor.gutter` | Muted gray |

### File Picker (Fuzzy Finder) — `update_palette_content` + `CommandPaletteRenderModel`

`CommandPaletteRenderModel` now carries split prompt fields and match ranges:

```rust
pub prompt_prefix: String,        // "find> " — rendered with hint_color
pub prompt_query: String,         // user query — rendered with text_color / hint_color
pub result_match_ranges: Vec<Vec<(usize, usize)>>,  // byte ranges for accent highlight
pub match_color: [f32; 4],        // ui.accent — matched chars
pub label_color: [f32; 4],        // ui.fg_dim  — normal label text
```

Renderer splits each result label into segments: non-matched parts use `label_color` (`fg_dim`), matched parts use `match_color` (`accent`). Substring matches produce one continuous highlight range; fuzzy matches highlight individual matching chars.

| Element | Token | Behavior |
|---------|-------|----------|
| Prompt prefix (`find> `) | `hint_color` | Muted — always dim |
| Query text (user input) | `text_color` | Bright when user has typed |
| Empty hint placeholder | `hint_color` | Dim when query is empty |
| Result labels (normal) | `label_color` = `fg_dim` | Soft gray |
| Matched chars in labels | `match_color` = `accent` | Green accent |
| Active item background | `selection_bg` | Subtle highlight strip |
| "(no matches)" | `hint_color` | Muted, below separator |
