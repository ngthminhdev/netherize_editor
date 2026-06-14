# Right Dock Tabs And Outline Icons Design

## Goal

Match the right-dock tab strip height to the bottom terminal tab bar and render
Outline symbol kinds with the same built-in SVG assets used by Document Symbols
in the command palette.

## Design

- Change `RIGHT_TAB_STRIP_HEIGHT` from 50 logical pixels to 72 logical pixels.
  Keep this shared constant as the source for layout, rendering, and hit-testing.
- Reuse `app::command_palette::symbol_icon` for the LSP kind-to-asset mapping so
  Space F P and Outline cannot drift to different symbol icons.
- Give the right-dock surface its own `IconPipeline` and icon instance buffer.
  Outline rows upload SVG icon instances there; other right-dock tabs upload an
  empty icon list.
- Preserve Outline indentation, selection chrome, text, line numbers, clipping,
  and row hit-testing.

## Verification

- Unit-test the 72-pixel strip constant.
- Extend symbol icon mapping tests for kinds used by Outline.
- Run targeted layout and command-palette tests, then `cargo check`, rustfmt, and
  `git diff --check`.
