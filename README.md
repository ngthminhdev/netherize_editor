[![Ask DeepWiki](https://deepwiki.com/badge.svg)](https://deepwiki.com/ngthminhdev/netherize_editor)

# Netherize Editor

A GPU-accelerated terminal/text editor written in Rust. Currently in active development.

---

## Quick Status

| Area | Status |
|------|--------|
| Editor buffer + cursor | ✅ Stable, fully tested |
| Vim-style mode system | ✅ Working (Normal / Insert / Visual / PaletteFocus / TerminalFocus) |
| GPU renderer (wgpu) | ✅ Renders text, cursor, gutter, sidebar, statusbar |
| Glyph atlas (texture packing) | ✅ Shelf-packer, uploaded to GPU |
| Text shaping (cosmic-text) | ✅ Rich text + syntax spans |
| Syntax highlighting (tree-sitter) | ✅ 17 languages (Rust / JS / TS / JSX / TSX / Go / Python / Dart / Java / Bash / JSON / YAML / TOML / Markdown / SQL / XML / Dockerfile / Proto) |
| Workbench layout engine | ✅ Region-based, resizable splits |
| Native window lifecycle | ✅ Thresholded titlebar drag, clickable tabs, preference-aware double-click, safe dirty quit, remembered window frame |
| File reliability | ✅ Atomic durable saves, save-error toast, 10 MiB interactive guard, dirty-buffer panic recovery |
| File explorer sidebar | ✅ Tree with j/k/h/l/Enter navigation + theme-correct colors |
| Embedded terminal (PTY) | ✅ ANSI parser + grid |
| LSP client | ✅ Smart root detection + async stdio transport + Mason-style install prompt |
| Config / theme (TOML runtime) | ✅ 83 built-in themes + repo profiles + user theme discovery + persisted runtime selection |
| Command palette | ✅ Overlay UI with prompt prefix/query color split |
| File picker (fuzzy) | ✅ Matched-char accent highlight + fg_dim labels |
| Multi-buffer | ✅ Buffer ring (next/prev/close) |
| Leap navigation | ✅ EasyMotion-style jump with dim overlay + per-char quad |
| AI chat panel | ✅ Inline AI assistant with dedicated commands |
| LeetCode integration | ✅ Problem fetch, code runner, cache |
| Test runner | ✅ In-editor test execution with results UI |
| Live grep | ✅ Workspace-wide search with results overlay |
| Which-key | ✅ Keybinding hint popup |
| Code Graph HUD | ✅ Interactive symbol dependency + risk visualization overlay |
| Markdown preview | ✅ In-editor markdown rendering |
| Editor breadcrumb | ✅ File icon + path breadcrumb in editor header |
| Editor minimap | ✅ Toggleable overview-block minimap via Space M N |
| Vim `%` match-bracket | ✅ Jump to matching bracket with ripple overlay |
| Yank flash | ✅ Visual feedback for copy/yank with fade-out animation |
| LeetCode test generation | ✅ Stratified case generation + AI verification |
| Single-instance routing, `--new-instance`, remote open of dirs/files | `src/app/single_instance.rs`, `src/app/event_loop/mod.rs` (`run`), `src/app/event_loop/commands_explorer.rs` (`handle_remote_open`) | Socket protocol, startup forwarding, and the in-window open handling |
| Workspace switch (Open Folder / recent / worktrees), dirty-switch guard | `src/app/event_loop/commands_explorer.rs` (`switch_workspace_with_files`), `src/app/event_loop/commands_prompts.rs` | Switch pipeline, save/discard confirmation, worktree palette |
| External file change toasts / conflict prompt wording | `src/app/event_loop/async_results/filesystem.rs`, `src/app/app_state/palette.rs` | Toast aggregation sits on the external-change pipeline reports |
| Spatial Canvas (NetherCanvas) | ✅ Navigable 2D code canvas with LSP-driven cards, auto-arrange, scope-aware editing |
| Workbench panel slide animation | ✅ Hyprland-style timeline-based slide for docks + zen mode |
| Solid caret (no blink) | ✅ Caret stays visible always |
| Palette vim key repeat | ✅ j/k repeat in palette Normal mode |

---

## Running

```sh
cargo run
```

```sh
cargo test             # full unit + doc test suite
cargo bench            # criterion benchmarks in benches/
```

### Bundling / Distribution

```sh
scripts/bundle_macos.sh      # macOS .app bundle
scripts/bundle_linux.sh      # Linux AppImage / tarball
scripts/bundle_windows.sh    # Windows installer
scripts/install.sh           # Quick install to local bin
```

### Profiling

```sh
scripts/profile_flamegraph.sh   # Generate flamegraph
scripts/run_perf_baseline.sh    # Run performance baseline
scripts/generate_bench_samples.sh  # Generate benchmark samples
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
│   │   │   ├── code_graph_hud.rs  # Code graph HUD overlay state
│   │   │   ├── canvas.rs          # Spatial canvas state management
│   │   │   ├── canvas_edit.rs     # Scope-aware card editing logic
│   │   │   └── tests.rs           # AppState regression tests
│   │   ├── event_loop/            # winit ApplicationHandler impl + command dispatch
│   │   │   ├── mod.rs             # run() entrypoint
│   │   │   ├── application.rs     # winit::ApplicationHandler impl
│   │   │   ├── async_results/     # Async result processing split by topic
│   │   │   │   ├── mod.rs         # Drain bridge, reject stale results, delegate to handlers
│   │   │   │   ├── ai.rs          # AI chat results
│   │   │   │   ├── canvas_scope.rs # Canvas scope resolution results (LSP symbols)
│   │   │   │   ├── failure.rs     # Error/failure handling
│   │   │   │   ├── filesystem.rs  # File system operation results
│   │   │   │   ├── fzf.rs         # Fuzzy finder results
│   │   │   │   ├── git.rs         # Git operation results
│   │   │   │   ├── leetcode_fetch.rs  # LeetCode problem fetch results
│   │   │   │   ├── lsp.rs         # LSP response handling
│   │   │   │   ├── preview.rs     # File/buffer preview results
│   │   │   │   ├── runner.rs      # Code runner results
│   │   │   │   ├── shell.rs       # Shell command results
│   │   │   │   ├── syntax.rs      # Syntax highlighting results
│   │   │   │   ├── system.rs      # System info results
│   │   │   │   └── terminal.rs    # Terminal PTY output
│   │   │   ├── commands.rs        # Command orchestration facade + shared helpers
│   │   │   ├── commands_editor.rs # Editor edit/navigation/leap command helpers
│   │   │   ├── commands_completion.rs # Completion popup and insert flows
│   │   │   ├── commands_terminal.rs # Terminal/panel/focus commands
│   │   │   ├── commands_explorer.rs # Explorer/sidebar/workspace commands
│   │   │   ├── commands_palette.rs # Palette/open-file/open-buffer commands
│   │   │   ├── commands_lsp.rs    # LSP/diagnostics/inline-AI commands
│   │   │   ├── commands_ai_agent.rs # AI agent/chat panel commands
│   │   │   ├── commands_canvas.rs # Spatial canvas commands
│   │   │   ├── commands_prompts.rs # Confirmation/prompt/theme/recent-project flows
│   │   │   ├── commands_settings.rs # Settings panel commands
│   │   │   ├── commands_settings_helpers.rs # Settings editing helpers
│   │   │   ├── commands_tests.rs  # Test runner commands
│   │   │   ├── helpers.rs         # Shared render/layout helpers
│   │   │   ├── setup.rs           # GPU + window init
│   │   │   └── welcome.rs         # Welcome screen logic
│   │   ├── input/                 # Key normalization + pending-state router
│   │   │   ├── mod.rs             # Public input module surface
│   │   │   ├── handler.rs         # Main input state machine / router
│   │   │   ├── model.rs           # NormalizedInput / TranslatedInput
│   │   │   ├── pending.rs         # Pending chord/operator state types
│   │   │   ├── helpers.rs         # Terminal key payload building, input utilities
│   │   │   └── tests.rs           # Input handler tests
│   │   ├── input_map/             # Keymap resolution (key event → Command)
│   │   │   ├── mod.rs
│   │   │   ├── focus.rs           # Focus-context-aware key routing
│   │   │   ├── helpers.rs         # Input map utilities
│   │   │   └── tests.rs           # Input map tests
│   │   ├── ai_agents.rs           # AI agent integration
│   │   ├── clipboard.rs           # Clipboard read/write (arboard wrapper)
│   │   ├── command_palette.rs     # Command palette state + filtering
│   │   ├── file_picker.rs         # File picker state + fuzzy results
│   │   ├── match_ranges.rs        # Match range tracking for search/highlight
│   │   ├── persistence.rs         # App state persistence (recent projects, etc.)
│   │   ├── resolved_keymap.rs     # Merged runtime keymap from config layers
│   │   ├── single_instance.rs     # Unix-socket single-instance routing (2nd launch → running window)
│   │   └── async_bridge.rs        # Tokio ↔ winit message bridge
│   │
│   ├── core/                      # Editor semantics (mode, commands)
│   │   ├── mode.rs                # EditorMode enum + ModeState transition machine
│   │   ├── commands.rs            # Command enum (all editor actions)
│   │   ├── command_ids.rs         # Stable string IDs for palette lookup
│   │   ├── text_object.rs         # Text object definitions (word, paragraph, etc.)
│   │   ├── transaction.rs         # Undo/redo transaction grouping
│   │   ├── command_dispatch/      # Command execution logic split by domain
│   │   │   ├── mod.rs             # Routes Command → handler
│   │   │   ├── editing.rs         # Text mutation handlers
│   │   │   ├── navigation.rs      # Cursor movement handlers
│   │   │   ├── palette.rs         # Command palette/picker handlers
│   │   │   ├── common.rs          # Shared dispatch utilities
│   │   │   ├── session.rs         # Session management dispatch
│   │   │   └── tests.rs           # Command dispatch tests
│   │   └── mod.rs
│   │
│   ├── codegraph/                  # Code graph data model + HUD
│   │   ├── mod.rs
│   │   ├── model.rs               # Symbol/edge graph model
│   │   ├── edges.rs               # Edge type definitions
│   │   ├── layout.rs              # Graph layout computation
│   │   ├── navigation.rs          # Graph navigation logic
│   │   └── cli_json.rs            # CLI JSON parser for code graph data
│   │
│   ├── lsp/                       # Polyglot language server client
│   │   ├── registry.rs            # File extension/filename → language profile, install command, root markers
│   │   ├── client.rs              # JSON-RPC framing, async stdio transport, didOpen/didChange lifecycle
│   │   ├── capabilities.rs        # LSP server capability negotiation
│   │   ├── symbol_cache.rs        # Workspace symbol caching
│   │   └── mod.rs
│   │
│   ├── render/                    # GPU rendering layer (wgpu)
│   │   ├── renderer.rs            # Renderer facade + shared render types
│   │   ├── renderer/              # Modular rendering implementation
│   │   │   ├── ui/                # UI components
│   │   │   │   ├── sidebar.rs     # File explorer sidebar
│   │   │   │   ├── terminal.rs    # Terminal grid rendering
│   │   │   │   ├── statusbar.rs   # Status bar
│   │   │   │   ├── topbar.rs      # Top bar / tab bar
│   │   │   │   ├── markdown_preview.rs # Markdown preview rendering
│   │   │   │   ├── test_runner.rs # Test runner results panel
│   │   │   │   ├── whichkey.rs    # Which-key hint popup
│   │   │   │   ├── popups.rs      # Generic popup rendering
│   │   │   │   ├── welcome.rs     # Welcome screen
│   │   │   │   └── utils.rs       # UI render utilities
│   │   │   ├── editor/            # Editor components
│   │   │   │   ├── buffers.rs     # Buffer rendering
│   │   │   │   ├── buffers/       # Buffer sub-components
│   │   │   │   ├── selections.rs  # Selection highlight rendering
│   │   │   │   ├── overlays.rs    # Editor overlays
│   │   │   │   ├── overlays/      # Overlay sub-components
│   │   │   │   ├── completion.rs  # Completion popup rendering
│   │   │   │   ├── extensions.rs  # Editor extension rendering
│   │   │   │   ├── fuzzy.rs       # Fuzzy match rendering
│   │   │   │   ├── help.rs        # Help panel rendering
│   │   │   │   ├── settings.rs    # Settings panel rendering
│   │   │   │   └── viewport.rs    # Viewport management
│   │   │   ├── palette/           # Palette components
│   │   │   │   ├── file_picker.rs # File picker overlay
│   │   │   │   ├── leap.rs        # Leap/EasyMotion overlay
│   │   │   │   ├── live_grep.rs   # Live grep overlay
│   │   │   │   ├── recent_projects.rs # Recent projects overlay
│   │   │   │   ├── highlighted_label.rs # Highlighted text label
│   │   │   │   └── minimal.rs     # Minimal palette variant
│   │   │   ├── components/        # Reusable UI primitives
│   │   │   │   ├── help_keycaps.rs
│   │   │   │   ├── highlight_chip.rs
│   │   │   │   ├── prefix_icon_badge.rs
│   │   │   │   └── shortcut_hint.rs
│   │   │   ├── lifecycle/         # GPU frame management
│   │   │   │   └── frame.rs
│   │   │   ├── components.rs      # Common UI component primitives
│   │   │   └── helpers.rs         # Shared pure helpers for render modules
│   │   ├── caret.rs               # Cursor/caret rendering
│   │   ├── text_pipeline.rs       # Glyph-instance pipeline (text quads)
│   │   ├── region_pipeline.rs     # Colored quad pipeline (backgrounds, highlights)
│   │   ├── image_pipeline.rs      # Image/texture rendering pipeline
│   │   ├── icon_pipeline.rs       # Icon/SVG rendering pipeline
│   │   ├── pipeline.rs            # Base pipeline abstractions
│   │   ├── glyph_instance.rs      # Glyph instance data structures
│   │   ├── color_space.rs         # Color space conversion utilities
│   │   ├── surface.rs             # GPU surface management
│   │   ├── shaders/               # WGSL shader sources
│   │   │   ├── caret.wgsl
│   │   │   ├── glyph.wgsl
│   │   │   ├── icon.wgsl
│   │   │   ├── image.wgsl
│   │   │   ├── quad.wgsl
│   │   │   └── region.wgsl
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
│   │   ├── highlight/             # Highlighting logic
│   │   │   ├── categories.rs      # Token category definitions
│   │   │   ├── engine.rs          # Highlight engine
│   │   │   ├── queries.rs         # Query loading
│   │   │   ├── spans.rs           # Highlight span generation
│   │   │   ├── mod.rs
│   │   │   └── normalize_tests.rs # Highlight normalization tests
│   │   ├── parser.rs              # Language registry bridge → tree-sitter grammar bootstrap
│   │   ├── syntax_engine.rs       # tree-sitter parser lifecycle for each supported language
│   │   ├── fold.rs                # Code folding support
│   │   ├── queries/               # Tree-sitter query sources (.scm files)
│   │   │   ├── bash/
│   │   │   ├── dart/
│   │   │   ├── dockerfile/
│   │   │   ├── go/
│   │   │   ├── java/
│   │   │   ├── javascript/
│   │   │   ├── json/
│   │   │   ├── jsx/
│   │   │   ├── markdown/
│   │   │   ├── proto/
│   │   │   ├── python/
│   │   │   ├── rust/
│   │   │   ├── sql/
│   │   │   ├── tsx/
│   │   │   ├── typescript/
│   │   │   ├── xml/
│   │   │   └── yaml/
│   │   └── mod.rs
│   │
│   ├── canvas/                    # Spatial canvas (NetherCanvas)
│   │   ├── mod.rs
│   │   ├── model.rs               # CanvasBlock, CanvasCamera, CanvasState — pure data model
│   │   ├── layout.rs              # Auto-arrange algorithm (column wrap + inward fill)
│   │   └── navigation.rs          # Spatial navigation helpers (nearest-block search)
│   │
│   ├── workbench/                 # UI layout + panel management
│   │   ├── layout_engine.rs       # WorkbenchLayoutEngine — computes RegionModel from panel sizes
│   │   ├── region_model.rs        # RegionId / RegionBounds / RegionModel tree
│   │   ├── panel_state.rs         # Sidebar/bottom panel open-state + sizes
│   │   ├── focus_manager.rs       # Which region currently holds keyboard focus
│   │   ├── overlay_manager.rs     # Overlay stack (palette, picker, etc.)
│   │   ├── inspector_panel.rs     # Inspector/Right sidebar panel
│   │   ├── text_coordinate_map.rs # Text ↔ pixel coordinate mapping
│   │   ├── motion.rs              # Timeline-based motion primitives (ease, spring, slide)
│   │   ├── debug_state.rs         # Debug/development state tracking
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
│   │   ├── cell_shapes.rs         # Terminal cell shape definitions
│   │   ├── highlighter.rs         # Terminal output syntax highlighting
│   │   ├── terminal_renderer.rs   # Terminal-specific rendering logic
│   │   └── mod.rs
│   │
│   ├── runner/                    # Code runner (LeetCode integration)
│   │   ├── leetcode.rs            # LeetCode problem management
│   │   ├── leetcode_adapter.rs    # LeetCode API adapter
│   │   ├── leetcode_api.rs        # LeetCode HTTP API client
│   │   ├── leetcode_cache.rs      # LeetCode problem cache
│   │   └── mod.rs
│   │
│   ├── async_runtime/             # Tokio async bridge
│   │   ├── scheduler.rs           # Thin facade: shared registries/constants + AsyncScheduler surface
│   │   ├── scheduler/             # Modular scheduler tasks
│   │   │   ├── runtime.rs         # Runtime bootstrap + request IDs
│   │   │   ├── dispatch.rs        # Request routing / wake-up plumbing
│   │   │   ├── emit.rs            # Event emission to UI thread
│   │   │   ├── ai_jobs.rs         # AI inference jobs
│   │   │   ├── ai.rs              # AI service integration
│   │   │   ├── lsp.rs             # LSP lifecycle
│   │   │   ├── lsp_io.rs          # LSP transport readers
│   │   │   ├── lsp_parse.rs       # LSP response parsing
│   │   │   ├── pty.rs             # PTY lifecycle and terminal output streaming
│   │   │   ├── syntax_jobs.rs     # Tree-sitter parse/highlight and lightweight virtual jobs
│   │   │   ├── file_watch.rs      # File watcher
│   │   │   ├── local_history.rs   # Local file history
│   │   │   ├── fzf.rs             # FZF integration
│   │   │   ├── git.rs             # Git helpers
│   │   │   ├── leetcode_fetch.rs  # LeetCode problem fetching
│   │   │   ├── codegraph.rs       # Code graph analysis jobs
│   │   │   └── tests.rs           # Scheduler tests
│   │   ├── message.rs             # Worker request/result/event types sent across the bridge
│   │   ├── dart_env.rs            # Dart environment setup
│   │   ├── python_env.rs          # Python environment setup
│   │   └── mod.rs
│   │
│   ├── config/                    # Config loading
│   │   ├── theme_config/          # Theme loading and model logic
│   │   │   ├── builtin.rs         # Built-in theme definitions
│   │   │   ├── loader.rs          # Theme file loading
│   │   │   ├── model.rs           # Theme data model
│   │   │   └── raw.rs             # Raw TOML theme deserialization
│   │   ├── theme_config.rs        # Theme config top-level facade
│   │   ├── paths.rs               # Shared user-config / legacy-state path helpers
│   │   ├── ui_config.rs           # UiConfig — layout sizes, cursor style, padding
│   │   ├── keymap_config.rs       # KeymapConfig — raw key binding table
│   │   ├── keymap_loader.rs       # Loads + merges keymap TOML files
│   │   ├── ai_config.rs           # AI service configuration
│   │   └── mod.rs
│   │
│   ├── platform/                  # Platform-specific code (empty, reserved)
│   └── bin/                       # Additional binary targets (empty, reserved)
│
├── config/
│   ├── themes/                    # 83 built-in theme profiles
│   │   ├── default-dark.toml      # Default dark theme
│   │   ├── catppuccin-mocha.toml
│   │   ├── dracula.toml
│   │   ├── tokyo-night.toml
│   │   └── ... (83 total: bearded-*, gruvbox, nord, one-dark, rose-pine, etc.)
│   ├── ui/
│   │   └── default.toml           # Layout sizes, cursor shape, padding, dock visibility
│   ├── keymaps/
│   │   └── default.toml           # Shared repo baseline; local remaps live in ~/.config/netherize/keymaps/user.toml
│   ├── fonts/
│   │   ├── GoogleSansCode.ttf     # Bundled UI font
│   │   └── HackNerdFont-Regular.ttf  # Bundled monospace/terminal font
│   └── ai.toml                    # AI service configuration
│
├── tests/
│   ├── lsp_fvm_detection.rs       # LSP FVM detection integration test
│   └── vietnamese_terminal_render.rs  # Vietnamese terminal rendering test
│
├── benches/
│   ├── editor_bench.rs            # Criterion editor benchmarks
│   └── e2e_perf_runner.rs         # End-to-end performance benchmarks
│
├── benchmarks/
│   ├── baselines/                 # Performance baseline data
│   └── inputs/                    # Benchmark input files
│
├── scripts/
│   ├── bundle_macos.sh            # macOS .app bundler
│   ├── bundle_linux.sh            # Linux AppImage / tarball bundler
│   ├── bundle_windows.sh          # Windows installer bundler
│   ├── install.sh                 # Quick install to local bin
│   ├── profile_flamegraph.sh      # Flamegraph profiling
│   ├── run_perf_baseline.sh       # Performance baseline runner
│   └── generate_bench_samples.sh  # Benchmark sample generator
│
├── assets/
│   ├── app_logo.png               # App logo (color)
│   ├── app_logo_black.png         # App logo (black)
│   ├── app_logo_1.png             # App logo variant
│   ├── app_logo_black_1.png       # App logo black variant
│   ├── app_logo_keyboard.svg      # Keyboard logo variant
│   └── bearded-icons/             # Bearded icon set
│
├── docs/
│   ├── MODULE12_HANDOFF_COMPACT.md  # Handoff notes for Module 12 (Phase 2+3)
│   ├── FVM_LSP_FIX.md             # FVM LSP fix documentation
│   ├── perf_profiling.md          # Performance profiling guide
│   └── superpowers/               # Superpowers documentation
│
├── Cargo.toml
├── Cargo.lock
├── Cross.toml                     # Cross-compilation config
├── AGENTS.md                      # Agent instructions
├── CLAUDE.md                      # Claude-specific instructions
├── BUILD.md                       # Build instructions
└── DEPENDENCIES.md                # Dependency documentation
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
  - route into PTY / LSP / FZF / local history / syntax / AI / git / leetcode job worker
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
  - delegate to focused handlers (lsp.rs, syntax.rs, ai.rs, git.rs, etc.)
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
| AI inference and service integration | `src/async_runtime/scheduler/ai_jobs.rs`, `ai.rs` |
| LeetCode problem fetching | `src/async_runtime/scheduler/leetcode_fetch.rs` |
| Code graph analysis | `src/async_runtime/scheduler/codegraph.rs` |
| Canvas scope resolution (LSP) | `src/async_runtime/scheduler/lsp.rs`, `src/app/event_loop/async_results/canvas_scope.rs` |

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
| Undo transaction boundaries for repeated delete/paste/edit commands | `src/core/command_dispatch/mod.rs`, `src/core/transaction.rs`, `src/app/app_state/mod.rs` | Dispatch decides when to commit; transaction.rs groups; AppState stores the stack |
| Mode transitions such as Normal/Insert/Visual/TerminalFocus | `src/core/mode.rs`, `src/app/app_state/mod.rs` | `ModeState` validates transitions; `AppState` applies them |
| F12 terminal behavior, focus handoff, explorer/panel focus routing | `src/app/event_loop/commands.rs`, `src/app/event_loop/commands_terminal.rs`, `src/app/event_loop/commands_explorer.rs` | The facade routes by UI domain; terminal and explorer behavior now live in focused modules |
| Completion popup behavior, acceptance, and auto-trigger after typing | `src/app/event_loop/commands_completion.rs`, `src/app/event_loop/commands_lsp.rs`, `src/app/event_loop/async_results/` | Request submit lives with command helpers; result application lands in async results sub-modules |
| Delete/close confirmations, theme selection, recent-project palette, explorer create/rename prompts | `src/app/event_loop/commands_prompts.rs`, `src/app/event_loop/commands_palette.rs` | Prompt lifecycle and confirm flows were split out of the main command facade |
| Settings panel behavior, settings editing | `src/app/event_loop/commands_settings.rs`, `src/app/event_loop/commands_settings_helpers.rs` | Settings commands and their editing helpers live in dedicated modules |
| Terminal raw input, ANSI behavior, PTY I/O | `src/app/input/helpers.rs`, `src/app/input/handler.rs`, `src/terminal/pty.rs`, `src/terminal/grid.rs` | Terminal key payload building lives in input helpers, then flows into PTY/grid behavior |
| Sidebar / bottom panel overlap, docking geometry, resize handles | `src/workbench/layout_engine.rs`, `src/workbench/panel_state.rs` | Region bounds and panel sizes come from the workbench layout engine |
| Cursor/caret rendering, terminal cursor visibility, status bar UI | `src/render/caret.rs`, `src/render/renderer/ui/`, `src/app/event_loop/application.rs` | Render prep happens in modular UI code, driven by event-loop state |
| Theme token bug or wrong color/icon | `config/themes/default-dark.toml`, `src/config/theme_config/` | Theme data is defined in TOML and validated/loaded in the theme module |
| UI spacing, panel sizes, cursor shape defaults | `config/ui/default.toml`, `src/config/ui_config.rs` | Geometry defaults come from UI config, not from the renderer |
| AI chat panel behavior | `src/app/event_loop/commands_ai_agent.rs`, `src/app/event_loop/async_results/ai.rs`, `src/app/ai_agents.rs` | AI agent commands, async results, and agent integration |
| Code Graph HUD | `src/codegraph/`, `src/app/app_state/code_graph_hud.rs`, `src/app/event_loop/commands_lsp.rs`, `src/async_runtime/scheduler/codegraph.rs` | Graph model, HUD state, trigger commands, and async analysis |
| Markdown preview | `src/render/renderer/ui/markdown_preview.rs`, `src/app/event_loop/commands.rs` | Markdown rendering and preview toggle |
| LeetCode problem fetch, code runner | `src/runner/`, `src/app/event_loop/async_results/runner.rs`, `src/async_runtime/scheduler/leetcode_fetch.rs` | Runner logic, async results, and scheduler tasks |
| Test runner behavior | `src/app/event_loop/commands_tests.rs`, `src/render/renderer/ui/test_runner.rs` | Test runner commands and rendering |
| Live grep / workspace search | `src/app/event_loop/commands_palette.rs`, `src/render/renderer/palette/live_grep.rs`, `src/workspace/fuzzy.rs` | Search initiation, rendering, and fuzzy matching |
| Spatial Canvas (NetherCanvas) | `src/canvas/`, `src/app/app_state/canvas.rs`, `src/app/app_state/canvas_edit.rs`, `src/app/event_loop/commands_canvas.rs`, `src/render/renderer/canvas.rs` | Canvas model, state, editing, commands, and rendering |
| Workbench panel slide / motion | `src/workbench/motion.rs`, `src/app/event_loop/application.rs`, `src/config/ui_config.rs` | Motion timeline primitives, animation tick, and easing config |
| Which-key hints | `src/render/renderer/ui/whichkey.rs` | Which-key popup rendering |
| Clipboard behavior | `src/app/clipboard.rs` | Clipboard read/write via arboard |
| LSP capabilities or symbol caching | `src/lsp/capabilities.rs`, `src/lsp/symbol_cache.rs` | LSP negotiation and symbol cache |
| Dart/Python environment setup | `src/async_runtime/dart_env.rs`, `src/async_runtime/python_env.rs` | Language environment initialization |

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
| `AsyncScheduler` | `async_runtime/scheduler.rs` | Tokio bridge facade for off-thread work |
| `ThemeConfig` | `config/theme_config/` | Theme data model loaded from TOML |
| `CodeGraphModel` | `codegraph/model.rs` | Symbol dependency graph data model |
| `CodeGraphHudState` | `app/app_state/code_graph_hud.rs` | Code graph HUD overlay state |
| `CanvasState` | `canvas/model.rs` | Spatial canvas state — blocks, camera, zoom, relations |
| `CanvasBlock` | `canvas/model.rs` | A single code card on the infinite canvas plane |
| `CanvasCamera` | `canvas/model.rs` | World↔screen mapping + zoom level for canvas viewport |
| `MotionTimeline` | `workbench/motion.rs` | Timeline-based animation state for panel slide transitions |

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

## Spatial Canvas (NetherCanvas)

An infinite 2D canvas for exploring code spatially. Open it with `F8` from any buffer — it spawns cards from LSP symbol data (definitions, callers, callees) and lets you navigate code relationships visually.

```
┌─ Canvas ──────────────────────────────────────┐
│                                               │
│   ┌──────────┐         ┌──────────┐           │
│   │ Focal    │────────►│ Def      │           │
│   │ symbol   │         │ site     │           │
│   └──────────┘         └──────────┘           │
│        │                                      │
│        ▼                                      │
│   ┌──────────┐         ┌──────────┐           │
│   │ Caller   │         │ Callee   │           │
│   │ sites    │         │ sites    │           │
│   └──────────┘         └──────────┘           │
│                                               │
└───────────────────────────────────────────────┘
```

### Key features

| Feature | Description |
|---------|-------------|
| LSP-driven cards | Each card shows a code snapshot sourced from LSP (definition, callers, callees) |
| Spatial navigation | hjkl-style movement between cards on the infinite plane |
| Auto-arrange (`gca`) | Re-flow cards into neat columns wrapping inward |
| Scope-aware editing | Cards support 4-state focus: navigate → select → edit → live buffer |
| Zoom | Scroll-wheel zoom in/out on the canvas |
| Spawn reveal animation | New cards animate in with a fade/scale effect |
| Block relations | Cards are tagged as Focal / Definition / Caller / Callee |
| Keybindings | `F8` open, `gca` auto-arrange, `Enter` drill deeper, `hjkl` navigate |

### Canvas focus modes

```
Navigate  ──(Enter on card)──►  Select   (card highlighted, ready to act)
Select    ──(Enter)───────────►  Edit     (live buffer in card, cursor active)
Select    ──(Escape)──────────►  Navigate
Edit      ──(Escape)──────────►  Select
```

### Module map

| File | Role |
|------|------|
| `src/canvas/model.rs` | Pure data model — `CanvasBlock`, `CanvasCamera`, `CanvasState` |
| `src/canvas/layout.rs` | Auto-arrange algorithm |
| `src/canvas/navigation.rs` | Nearest-block spatial search |
| `src/app/app_state/canvas.rs` | Canvas state management + mutation |
| `src/app/app_state/canvas_edit.rs` | Scope-aware card editing logic |
| `src/app/event_loop/commands_canvas.rs` | Canvas command handlers |
| `src/render/renderer/canvas.rs` | GPU rendering for canvas + cards |

---

## Workbench Panel Slide Animation

Dock panels (sidebar, terminal) now animate open/close with Hyprland-style slide transitions powered by timeline-based motion primitives in `src/workbench/motion.rs`.

Config in `config/ui/default.toml`:

```toml
[motion]
enabled = true
duration_ms = 250
ease = "ease_out_cubic"     # ease_in_out, ease_out_back, spring, etc.
```

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

The editor ships with 83 built-in themes including popular choices like Catppuccin Mocha, Dracula, Tokyo Night, Gruvbox, Nord, One Dark, Rose Pine, and the full Bearded theme family.

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
explorer_folder_collapsed_marker = ""

[icons.rust]
glyph = "\uE7A8"
color = "#FF955C"
```

### `config/keymaps/default.toml`

Maps key combos to `Command` IDs per mode context.

### `config/ai.toml`

AI service configuration (API endpoints, model selection, etc.).

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
3. **UI**: topbar, statusbar, sidebars, terminal grid, markdown preview, test runner, which-key
4. **Overlays**: command palette, file picker, completion popups, live grep

---

## Dependencies

| Crate | Purpose |
|-------|---------|
| `ropey` | Gap-buffer text rope for efficient insert/delete |
| `winit` | Cross-platform window + keyboard events |
| `wgpu` | GPU rendering (WebGPU API) |
| `cosmic-text` | Font loading, text shaping, glyph rasterization |
| `swash` | Low-level glyph rasterizer used by cosmic-text |
| `font-kit` | System font discovery |
| `tree-sitter` + language grammars | Incremental syntax parsing for 17 languages |
| `portable-pty` | PTY spawn + I/O for the embedded terminal |
| `tokio` | Async runtime for LSP + file watching + AI |
| `notify` | File system watcher (external change detection) |
| `serde` + `serde_json` | JSON-RPC (LSP protocol) |
| `lsp-types` | LSP protocol type definitions |
| `toml` | Config file parsing |
| `sysinfo` | System info for statusbar |
| `bytemuck` | Safe vertex buffer casting |
| `pollster` | Block-on-async for GPU device init |
| `arboard` | Cross-platform clipboard |
| `reqwest` | HTTP client (LeetCode API, etc.) |
| `image` | Image loading (PNG, JPEG, GIF, etc.) |
| `rfd` | Native file dialogs |
| `ignore` | Git-aware file filtering |
| `regex` | Regular expression support |
| `resvg` / `tiny-skia` / `usvg` | SVG rendering for icons |
| `similar` | Text diffing |
| `sha2` | Hashing |
| `criterion` | Benchmarks (dev-dependency) |
| `naga` | Shader validation (dev-dependency) |

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
14. **`src/canvas/`** — spatial canvas data model, layout, and navigation (NetherCanvas)
15. **`src/workbench/motion.rs`** — timeline-based animation primitives for panel slide transitions
