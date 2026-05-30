# Cerebrum

> OpenWolf's learning memory. Updated automatically as the AI learns from interactions.
> Do not edit manually unless correcting an error.
> Last updated: 2026-05-30

## User Preferences

<!-- How the user likes things done. Code style, tools, patterns, communication. -->
- **RTK Usage:** Do not blindly prefix every shell command with `rtk`. Use `rtk` only for supported simple command shapes; use raw shell commands for compound `find` predicates/actions (`-exec`, `-not`, grouped predicates), pipelines, redirection, shell expansion/substitution, or when `rtk` reports unsupported syntax.

## Key Learnings

- **Project:** netherize_editor
- **Description:** A GPU-accelerated terminal/text editor written in Rust. Currently in active development (Module 12 / Phase 2–3).
- **LSP Diagnostics Filtering:** LSP servers send diagnostics for ALL files they analyze, including builtin/stdlib files (node_modules, Go stdlib, Rust stdlib, Python site-packages). Editor must filter these out by path pattern matching to avoid showing errors in dependency code. Filter location: `src/app/event_loop/async_results/lsp.rs` in `LspDiagnostics` handler.
- **Async I/O Pattern:** All blocking I/O operations (file copy, network, heavy parsing) MUST use `tokio::spawn` with `WorkerRequest`/`WorkerResult` pattern. Never use `std::fs` on the main thread. Pattern: (1) Add request/result to `message.rs`, (2) Handle in `scheduler/dispatch.rs` with `tokio::spawn`, (3) Route result in `async_results/mod.rs`.
- **Code Folding Text Truncation:** Auto-folded long lines (>100 chars) must be truncated BEFORE text shaping, not during rendering. The text layout system (cosmic-text) shapes the full text and wraps it, so truncation must happen in `update_editor_content()` before calling `set_text_with_spans()`. Location: `src/render/renderer/editor/viewport.rs` in `truncate_folded_lines()` helper.
- **Tree-sitter Span Adjustment on Truncation:** When truncating folded lines, syntax highlight spans must be adjusted using a proper byte offset map. The old line-by-line offset calculation was broken and caused highlight colors to appear on wrong tokens. Fixed approach: build a sorted `(old_byte, new_byte)` map for every byte position, then use binary search to remap span start/end positions. This preserves tree-sitter highlighting on truncated folded lines.
- **Markdown Preview Close Semantics:** Markdown preview focus should close its buffer tab with `q` / `Space+x`, matching live grep and references tabs. `q` is routed in `src/app/input_map/focus.rs`; leader chords need the `preview` keymap scope in both `src/app/resolved_keymap.rs` and `config/keymaps/default.toml`.
- **RTK Limitations:** `rtk find` does not support compound predicates/actions such as `-exec`; `rtk git diff -- <paths>` may also mis-handle pathspec-style commands. Fall back to the raw command instead of trying to force the proxy.
- **Terminal Cell Backgrounds:** ANSI background-colored terminal cells must render as `RegionDrawInstance` quads underneath text, not as `"█"` glyphs in `TerminalViewRenderer::build_instances()`. Full-block glyphs leave font-metric seams and make full-screen TUIs look striped/broken.
- **Terminal ANSI Color Fidelity:** ANSI colors are specified in sRGB but the renderer's sRGB target expects linear inputs. Convert ANSI RGB/xterm colors through `srgb_rgba_to_linear_f32`, emit every style event in combined SGR sequences like `0;38;5;...;48;2;...m`, shape cells with `CellStyle.bold` using bold font weight, and do not run regex foreground highlighting on interactive PTY output because it overwrites application-provided terminal colors.
- **Dart/Flutter LSP with FVM:** Dart LSP server must use FVM-managed dart binary when available, not system dart. Priority: `.fvm/flutter_sdk/bin/cache/dart-sdk/bin/dart` (workspace-local) > `~/.fvm/versions/*/bin/cache/dart-sdk/bin/dart` (global FVM cache, newest first) > system `dart` from PATH. Detection logic in `src/lsp/client.rs::detect_fvm_dart_binary()` checks for `pubspec.yaml` first, then resolves the correct dart binary path. This ensures LSP uses the same Flutter/Dart version as the project.

## Do-Not-Repeat

<!-- Mistakes made and corrected. Each entry prevents the same mistake recurring. -->
<!-- Format: [YYYY-MM-DD] Description of what went wrong and what to do instead. -->

- **[2026-05-19] Blocking File I/O on Main Thread:** `ExplorerPasteFile` used `std::fs::copy()` which blocks the UI thread. This violates the 0-latency architecture rule. **Fix:** Use `tokio::fs::copy()` in `tokio::spawn` with async message passing via `WorkerRequest::CopyFile` → `WorkerResult::FileCopyResult`. All file operations must be async.
- **[2026-05-22] Unnecessary Parse on Scroll Commands:** Scroll commands (Ctrl-U/D, gg, G, zz) called `submit_parse_for_active_buffer(true)` even though they don't modify text. This caused severe lag on large files (700+ lines) because tree-sitter re-parsed the entire file on every scroll. **Fix:** Remove parse call from scroll/navigation commands. Only text-modifying commands should trigger re-parse.
- **[2026-05-22] Star Search Missing Jump Stack Push:** Visual star search (`*` in Normal/Visual mode) didn't call `push_jump()` before jumping to search result, so Ctrl-O couldn't return to original position. **Fix:** Add `ctx.app_state.push_jump()` before `search_word_under_cursor()` in `navigation.rs`, matching LSP goto-definition behavior.
- **[2026-05-24] Code Actions Missing Undo Stack Integration:** `do_apply_code_action_edits()` used `replace_active_document_text_preserve_cursor()` (no-undo variant) instead of `replace_active_document_text_preserve_cursor_with_undo()`. This meant code actions (space ca) couldn't be undone with `u`. **Fix:** Always use the `_with_undo` variant for user-initiated text changes. The no-undo variant is only for file loading operations that intentionally reset history.
- **[2026-05-26] LSP Overlay Persisting After Editing Commands:** When executing editing commands like `cw`, `x`, `d` (change/delete), LSP hover overlays (documentation popups) remained visible even after text was deleted and mode changed to Insert. This happened because: (1) overlays were cleared before command dispatch but not after, and (2) LSP hover responses arrived asynchronously after the user had already edited/moved, and the response handler didn't validate whether the cursor was still at the original position. **Fix:** Two-layer protection: (1) Clear overlay immediately when executing text-modifying commands or transitioning to Insert mode, and invalidate `latest_hover_request_id` to mark pending requests as stale. (2) Add `latest_hover_request_id` tracking (similar to `latest_definition_request_id`) and validate it in the hover response handler to drop stale responses. This prevents hover overlays from reappearing after the user has moved on.
- **[2026-05-27] File Watcher Not Reloading After Lazygit Discard:** When saving a file and then immediately discarding changes in lazygit (space gf), the editor continues displaying the saved code instead of reloading the discarded (old) code. **Root cause:** The self-save ignore window (2 seconds after save) blindly ignores ALL Modify events, including legitimate external changes from lazygit. This prevents the file from reloading even when the disk content differs from memory. **Fix:** Instead of blindly ignoring events within the 2-second window, read the file content and compare it with in-memory text. Only ignore if content matches (true self-save echo). If content differs (external change like lazygit discard), reload immediately. Also clear `buffer.in_memory_text = None` when a dirty-file conflict is detected, so closing/reopening will reload from disk.

## Decision Log

<!-- Significant technical decisions with rationale. Why X was chosen over Y. -->
