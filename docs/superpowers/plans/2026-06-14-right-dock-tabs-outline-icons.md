# Right Dock Tabs And Outline Icons Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make right-dock tabs 72 pixels tall and use the Document Symbols SVG icons in Outline rows.

**Architecture:** Keep the existing shared right-dock strip constant for all geometry. Reuse the command-palette symbol mapping and render its built-in icon ids through a dedicated right-dock `IconPipeline`.

**Tech Stack:** Rust, wgpu, built-in SVG icon atlas, cargo test

---

### Task 1: Lock Geometry And Mapping

**Files:**
- Modify: `src/workbench/layout_engine.rs`
- Modify: `src/app/command_palette.rs`

- [ ] Add tests asserting a 72-pixel right tab strip and canonical SVG ids for Outline symbol kinds.
- [ ] Run the targeted tests and confirm they fail for the current 50-pixel/glyph implementation.
- [ ] Change the shared strip height and expose the existing icon mapping within the crate.
- [ ] Run the targeted tests and confirm they pass.

### Task 2: Render Outline SVG Icons

**Files:**
- Modify: `src/render/renderer.rs`
- Modify: `src/render/renderer/lifecycle.rs`
- Modify: `src/render/renderer/lifecycle/frame.rs`
- Modify: `src/render/renderer/ui/test_runner.rs`

- [ ] Add a right-dock icon pipeline and icon instance buffer to `Renderer`.
- [ ] Build `IconDrawInstance` values in Outline rows using `symbol_icon` and `canonical_icon_id`.
- [ ] Upload, draw, and clear the icon pipeline with the existing right-dock surface lifecycle.
- [ ] Remove the old text-glyph icon helper while preserving kind colors and row geometry.

### Task 3: Verify

**Files:**
- Verify all modified Rust files.

- [ ] Run targeted tests for layout and symbol mapping.
- [ ] Run `cargo check`.
- [ ] Run rustfmt check on modified Rust files.
- [ ] Run `git diff --check` and GitNexus change detection.
