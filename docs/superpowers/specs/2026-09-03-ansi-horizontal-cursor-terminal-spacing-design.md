# ANSI Horizontal Cursor Terminal Spacing Design

## Problem

Claude Code renders portions of its terminal UI as separate styled fragments and positions those fragments with the ANSI Cursor Horizontal Absolute command (`CSI n G`). Netherize currently parses relative cursor movement (`A`/`B`/`C`/`D`) and absolute row/column movement (`H`/`f`), but treats `G` as unknown. The cursor therefore remains at the end of the previous fragment and the next fragment overwrites the intended gap, producing unreadable text such as `Fable5.1writesbetter...`.

## Design

Extend the existing terminal protocol rather than special-casing Claude output. Add a `CursorHorizontalAbsolute { col }` event to `AnsiEvent`. Parse `CSI n G` using VT's one-based coordinate convention, defaulting an omitted or zero parameter to column one. Apply the event in `TerminalGrid` by changing only `cursor_col`, clamped to the current grid width, while preserving `cursor_row`.

This keeps PTY decoding, grid state, rendering, cursor overlays, selection, and mouse hit-testing on the same cell geometry. It also benefits any other terminal application that emits CHA without changing existing ANSI behavior.

## Verification

Add parser coverage for explicit, default, and zero CHA parameters. Add a grid regression test that writes two fragments separated by `CSI G` and asserts that the intended blank cells remain between them. Run the focused tests once before implementation to prove the regression, then after implementation, followed by terminal/library tests, formatting, Clippy, and GitNexus change detection.

## Scope

Only `src/terminal/ansi_parser.rs` and `src/terminal/grid.rs` need production changes. No files, modules, dependencies, commands, or public configuration are added, so repository-layout and dependency documentation remain unchanged.
