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
│   │   ├── app_state/             # AppState module tree — authoritative editor state split by domain
│   │   │   ├── mod.rs             # Root AppState types, constructors, shared state models
│   │   │   ├── editor.rs          # Core editor mutation/cursor/navigation logic
│   │   │   ├── buffers.rs         # Buffer lifecycle, open/save, references/help/settings buffers
│   │   │   ├── workspace.rs       # Workspace/explorer attachment + query/mutation helpers
│   │   │   ├── palette.rs         # Command palette, file picker, diagnostics/reference picker state
│   │   │   ├── multi_cursor.rs    # Multi-cursor management
│   │   │   ├── settings.rs        # Settings tab models and editing state
│   │   │   ├── state.rs           # Derived/query helpers, completion/search/undo-redo state access
│   │   │   ├── overlays.rs        # Overlay + shared internal helper logic
│   │   │   └── tests.rs           # AppState regression tests
│   │   ├── event_loop/            # winit ApplicationHandler impl + command dispatch
│   │   │   ├── mod.rs             # run() entrypoint
│   │   │   ├── application.rs     # winit::ApplicationHandler impl
│   │   │   ├── async_results/     # Async result processing split by topic (LSP, AI, Terminal, etc.)
│   │   │   ├── commands.rs        # Command orchestration facade + shared helpers
│   │   │   ├── commands_editor.rs # Editor edit/navigation/leap command helpers
│   │   │   ├── commands_completion.rs # Completion popup and insert flows
│   │   │   ├── commands_terminal.rs # Terminal/panel/focus commands
│   │   │   ├── commands_explorer.rs # Explorer/sidebar/workspace commands
│   │   │   ├── commands_palette.rs # Palette/open-file/open-buffer commands
│   │   │   ├── commands_lsp.rs    # LSP/diagnostics/inline-AI commands
│   │   │   ├── commands_ai_chat.rs # AI chat panel commands
│   │   │   ├── commands_prompts.rs # Confirmation/prompt/theme/recent-project flows
│   │   │   ├── helpers.rs         # Shared render/layout helpers
│   │   │   ├── setup.rs           # GPU + window init
│   │   │   └── welcome.rs         # Welcome screen logic
│   │   ├── input/                 # Key normalization + pending-state router
│   │   │   ├── mod.rs             # Public input module surface
│   │   │   ├── handler.rs         # Main input state machine / router
│   │   │   ├── model.rs           # NormalizedInput / TranslatedInput
│   │   │   ├── pending.rs         # Pending chord/operator state types
│   │   ├── input_map/             # Keymap resolution (key event → Command)
│   │   │   ├── mod.rs
│   │   │   └── focus.rs           # Focus-context-aware key routing
│   │   ├── command_palette.rs     # Command palette state + filtering
│   │   ├── file_picker.rs         # File picker state + fuzzy results
│   │   └── async_bridge.rs        # Tokio ↔ winit message bridge
│   │
│   ├── core/                      # Editor semantics (mode, commands)
│   │   ├── mode.rs                # EditorMode enum + ModeState transition machine
│   │   ├── commands.rs            # Command enum (all editor actions)
│   │   ├── command_ids.rs         # Stable string IDs for palette lookup
│   │   ├── command_dispatch/      # Command execution logic split by domain
│   │   │   ├── mod.rs             # Routes Command → handler
│   │   │   ├── editing.rs         # Text mutation handlers
│   │   │   ├── navigation.rs      # Cursor movement handlers
│   │   │   └── palette.rs         # Command palette/picker handlers
│   │   └── mod.rs
│   │
│   ├── lsp/                       # Polyglot language server client
│   │   ├── registry.rs            # File extension/filename → language profile, install command, root markers
│   │   ├── client.rs              # JSON-RPC framing, async stdio transport, didOpen/didChange lifecycle
│   │   └── mod.rs
│   │
│   ├── render/                    # GPU rendering layer (wgpu)
│   │   ├── renderer.rs            # Renderer facade + shared render types
│   │   ├── renderer/              # Modular rendering implementation
│   │   │   ├── ui/                # UI components: sidebar, terminal, statusbar, AI chat, etc.
│   │   │   ├── editor/            # Editor components: buffers, selections, overlays, completion, etc.
│   │   │   ├── palette/           # Palette components: file picker, leap, recent projects, etc.
│   │   │   ├── lifecycle/         # GPU frame management and lifecycle
│   │   │   ├── components.rs      # Common UI component primitives
│   │   │   └── helpers.rs         # Shared pure helpers for render modules
│   │   ├── caret.rs               # Cursor/caret rendering
│   │   ├── text_pipeline.rs       # Glyph-instance pipeline (text quads)
│   │   ├── region_pipeline.rs     # Colored quad pipeline (backgrounds, highlights)
│   │   ├── image_pipeline.rs      # Image/texture rendering pipeline
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
│   │   ├── highlight/             # Highlighting logic split into categories, engine, queries, spans
│   │   ├── parser.rs              # Language registry bridge → tree-sitter grammar bootstrap
│   │   ├── syntax_engine.rs       # tree-sitter parser lifecycle for each supported language
│   │   ├── queries/               # Tree-sitter query sources (.scm files)
│   │   └── mod.rs
│   │
│   ├── workbench/                 # UI layout + panel management
│   │   ├── layout_engine.rs       # WorkbenchLayoutEngine — computes RegionModel from panel sizes
│   │   ├── region_model.rs        # RegionId / RegionBounds / RegionModel tree
│   │   ├── panel_state.rs         # Sidebar/bottom panel open-state + sizes
│   │   ├── focus_manager.rs       # Which region currently holds keyboard focus
│   │   ├── overlay_manager.rs     # Overlay stack (palette, picker, etc.)
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
│   │   └── mod.rs
│   │
│   ├── async_runtime/             # Tokio async bridge
│   │   ├── scheduler.rs           # Thin facade: shared registries/constants + AsyncScheduler surface
│   │   ├── scheduler/             # Modular scheduler tasks (LSP, PTY, Syntax, AI, etc.)
│   │   ├── message.rs             # Worker request/result/event types sent across the bridge
│   │   └── mod.rs
│   │
│   └── config/                    # Config loading
│       ├── theme_config/          # Theme loading and model logic
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
  (core/command_dispatch/mod.rs)
  - loop repeatable commands
  - group repeated text edits into one undo transaction
      │
      ▼
AppState  (app/app_state/mod.rs)
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
Renderer.render()  (render/renderer.rs)
  ├── Modular UI/Editor update passes
  ├── GlyphAtlas.get_or_insert()  →  GPU texture upload
  └── GlyphInstance[] → vertex buffer
      │
      ▼
wgpu submit  →  screen
```

### Async Runtime Flow

```
AppShell::submit_worker_request()
      │
      ▼
AsyncScheduler::submit()
  - stamp request_id + revision_id
  - send WorkerRequest into tokio mpsc
      │
      ▼
dispatch_loop()
  - classify request family
  - route into PTY / LSP / FZF / local history / syntax job worker
      │
      ▼
Worker task
  - do I/O / parse / tree-sitter / subprocess work off the UI thread
  - emit WorkerMessage::Result / Event
      │
      ▼
EventLoopProxy<AppEvent>
  - wakes winit when worker output matters for redraw
      │
      ▼
app/event_loop/async_results/mod.rs
  - drain bridge
  - reject stale buffer/revision results
  - delegate to focused handlers (lsp.rs, syntax.rs, etc.)
      │
      ▼
window.request_redraw()
```

### Async Runtime Ownership

| Area | Module |
|------|--------|
| Runtime bootstrap + request IDs | `src/async_runtime/scheduler/runtime.rs` |
| Request routing / wake-up plumbing | `src/async_runtime/scheduler/dispatch.rs`, `emit.rs` |
| Tree-sitter parse/highlight and lightweight virtual jobs | `src/async_runtime/scheduler/syntax_jobs.rs` |
| PTY lifecycle and terminal output streaming | `src/async_runtime/scheduler/pty.rs` |
| LSP lifecycle, transport readers, response parsing | `src/async_runtime/scheduler/lsp.rs`, `lsp_io.rs`, `lsp_parse.rs` |
| File watcher / local history / FZF / git helpers | `src/async_runtime/scheduler/file_watch.rs`, `local_history.rs`, `fzf.rs`, `git.rs` |

### Keyboard / Vim Path In One Line

For almost every "why didn't this key do what I expected?" bug, read the path below in order:

`application.rs` -> `app/input/handler.rs` -> `app/input_map/mod.rs` -> `app/resolved_keymap.rs` -> `app/event_loop/commands.rs` -> `core/command_dispatch/mod.rs` -> `app/app_state/mod.rs`

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
| A command should repeat `count` times or should/should not support counts | `src/core/commands.rs`, `src/core/command_dispatch/mod.rs` | Count policy and the actual execution loop are centralized here |
| Undo transaction boundaries for repeated delete/paste/edit commands | `src/core/command_dispatch/mod.rs`, `src/app/app_state/mod.rs` | Dispatch decides when to commit; AppState stores the transaction stack |
| Mode transitions such as Normal/Insert/Visual/TerminalFocus | `src/core/mode.rs`, `src/app/app_state/mod.rs` | `ModeState` validates transitions; `AppState` applies them |
| F12 terminal behavior, focus handoff, explorer/panel focus routing | `src/app/event_loop/commands.rs`, `src/app/event_loop/commands_terminal.rs`, `src/app/event_loop/commands_explorer.rs` | The facade routes by UI domain; terminal and explorer behavior now live in focused modules |
| Completion popup behavior, acceptance, and auto-trigger after typing | `src/app/event_loop/commands_completion.rs`, `src/app/event_loop/commands_lsp.rs`, `src/app/event_loop/async_results/` | Request submit lives with command helpers; result application lands in async results sub-modules |
| Delete/close confirmations, theme selection, recent-project palette, explorer create/rename prompts | `src/app/event_loop/commands_prompts.rs`, `src/app/event_loop/commands_palette.rs` | Prompt lifecycle and confirm flows were split out of the main command facade |
| Terminal raw input, ANSI behavior, PTY I/O | `src/app/input/helpers.rs`, `src/app/input/handler.rs`, `src/terminal/pty.rs`, `src/terminal/grid.rs` | Terminal key payload building lives in input helpers, then flows into PTY/grid behavior |
| Sidebar / bottom panel overlap, docking geometry, resize handles | `src/workbench/layout_engine.rs`, `src/workbench/panel_state.rs` | Region bounds and panel sizes come from the workbench layout engine |
| Cursor/caret rendering, terminal cursor visibility, status bar UI | `src/render/caret.rs`, `src/render/renderer/ui/`, `src/app/event_loop/application.rs` | Render prep happens in modular UI code, driven by event-loop state |
| Theme token bug or wrong color/icon | `config/themes/default-dark.toml`, `src/config/theme_config/` | Theme data is defined in TOML and validated/loaded in the theme module |
| UI spacing, panel sizes, cursor shape defaults | `config/ui/default.toml`, `src/config/ui_config.rs` | Geometry defaults come from UI config, not from the renderer |

### Three Common Debug Paths

1. Key maps wrong:
   `config/keymaps/default.toml` -> `src/app/resolved_keymap.rs` -> `src/app/input_map/mod.rs`

2. Key maps correctly but behavior is wrong:
   `src/app/input/handler.rs` -> `src/app/event_loop/commands.rs` -> `src/core/command_dispatch/mod.rs`

3. Command succeeds but screen looks wrong:
   `src/app/app_state/mod.rs` -> `src/workbench/layout_engine.rs` -> `src/render/renderer/ui/`

---

## Key Structs at a Glance

| Struct | File | Role |
|--------|------|------|
| `AppState` | `app/app_state/mod.rs` | Central editor state (text, cursor, mode, buffers, workspace) |
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
explorer_folder_collapsed_marker = ""

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

Each frame in `Renderer::render()` runs modularized update passes before submitting to GPU:

1. **Backgrounds**: region quads for visible panels (sidebar, editor, terminal, etc.)
2. **Editor**: gutter, text, selections, caret
3. **UI**: topbar, statusbar, sidebars, terminal grid
4. **Overlays**: command palette, file picker, completion popups

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
6. **`src/app/event_loop/commands.rs`** — the event-loop command facade; read this first to see how commands are delegated
7. **`src/app/event_loop/commands_editor.rs`** + **`src/app/event_loop/commands_terminal.rs`** + **`src/app/event_loop/commands_explorer.rs`** — the main workbench command domains
8. **`src/app/event_loop/commands_palette.rs`** + **`src/app/event_loop/commands_prompts.rs`** + **`src/app/event_loop/commands_completion.rs`** — overlay, prompt, and completion flows
9. **`src/core/command_dispatch/mod.rs`** — how commands mutate editor state and how undo grouping works
10. **`src/app/app_state/mod.rs`** — the central source of truth for text, cursor, mode, buffers, and transactions
11. **`src/workbench/layout_engine.rs`** — UI region geometry when a bug is visual/layout-related
12. **`src/render/renderer.rs`** + **`src/render/renderer/ui/`** — frame assembly and UI rendering
13. **`src/text/text_system.rs`** + **`src/text/atlas.rs`** — text shaping/raster path when glyph/render bugs appear
