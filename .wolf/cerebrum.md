# Cerebrum

> OpenWolf's learning memory. Updated automatically as the AI learns from interactions.
> Do not edit manually unless correcting an error.
> Last updated: 2026-05-16

## User Preferences

<!-- How the user likes things done. Code style, tools, patterns, communication. -->

## Key Learnings

- **Project:** netherize_editor
- **Description:** A GPU-accelerated terminal/text editor written in Rust. Currently in active development (Module 12 / Phase 2–3).
- **LSP Diagnostics Filtering:** LSP servers send diagnostics for ALL files they analyze, including builtin/stdlib files (node_modules, Go stdlib, Rust stdlib, Python site-packages). Editor must filter these out by path pattern matching to avoid showing errors in dependency code. Filter location: `src/app/event_loop/async_results/lsp.rs` in `LspDiagnostics` handler.

## Do-Not-Repeat

<!-- Mistakes made and corrected. Each entry prevents the same mistake recurring. -->
<!-- Format: [YYYY-MM-DD] Description of what went wrong and what to do instead. -->

## Decision Log

<!-- Significant technical decisions with rationale. Why X was chosen over Y. -->
