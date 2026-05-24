# Cerebrum

> OpenWolf's learning memory. Updated automatically as the AI learns from interactions.
> Do not edit manually unless correcting an error.
> Last updated: 2026-05-19

## User Preferences

<!-- How the user likes things done. Code style, tools, patterns, communication. -->

## Key Learnings

- **Project:** netherize_editor
- **Description:** A GPU-accelerated terminal/text editor written in Rust. Currently in active development (Module 12 / Phase 2–3).
- **LSP Diagnostics Filtering:** LSP servers send diagnostics for ALL files they analyze, including builtin/stdlib files (node_modules, Go stdlib, Rust stdlib, Python site-packages). Editor must filter these out by path pattern matching to avoid showing errors in dependency code. Filter location: `src/app/event_loop/async_results/lsp.rs` in `LspDiagnostics` handler.
- **Async I/O Pattern:** All blocking I/O operations (file copy, network, heavy parsing) MUST use `tokio::spawn` with `WorkerRequest`/`WorkerResult` pattern. Never use `std::fs` on the main thread. Pattern: (1) Add request/result to `message.rs`, (2) Handle in `scheduler/dispatch.rs` with `tokio::spawn`, (3) Route result in `async_results/mod.rs`.

## Do-Not-Repeat

<!-- Mistakes made and corrected. Each entry prevents the same mistake recurring. -->
<!-- Format: [YYYY-MM-DD] Description of what went wrong and what to do instead. -->

- **[2026-05-19] Blocking File I/O on Main Thread:** `ExplorerPasteFile` used `std::fs::copy()` which blocks the UI thread. This violates the 0-latency architecture rule. **Fix:** Use `tokio::fs::copy()` in `tokio::spawn` with async message passing via `WorkerRequest::CopyFile` → `WorkerResult::FileCopyResult`. All file operations must be async.
- **[2026-05-22] Unnecessary Parse on Scroll Commands:** Scroll commands (Ctrl-U/D, gg, G, zz) called `submit_parse_for_active_buffer(true)` even though they don't modify text. This caused severe lag on large files (700+ lines) because tree-sitter re-parsed the entire file on every scroll. **Fix:** Remove parse call from scroll/navigation commands. Only text-modifying commands should trigger re-parse.
- **[2026-05-22] Star Search Missing Jump Stack Push:** Visual star search (`*` in Normal/Visual mode) didn't call `push_jump()` before jumping to search result, so Ctrl-O couldn't return to original position. **Fix:** Add `ctx.app_state.push_jump()` before `search_word_under_cursor()` in `navigation.rs`, matching LSP goto-definition behavior.

## Decision Log

<!-- Significant technical decisions with rationale. Why X was chosen over Y. -->
