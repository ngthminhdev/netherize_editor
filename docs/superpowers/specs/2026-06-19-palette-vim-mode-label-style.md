# Palette Vim-Mode Label Rendering Consistency

**Date:** 2026-06-19
**Status:** Approved
**Scope:** Renderer / `src/render/renderer/palette/*` + `src/render/renderer/editor/fuzzy.rs` + producer in `src/app/command_palette.rs`

## Problem

The status bar (`src/render/renderer/ui/statusbar.rs`) renders the editor mode as a **full pill** (border + colored fill + colored dot + bold text) using `mode_display_label(mode)` and `mode_pill_color(mode, theme)` from `src/render/renderer/helpers.rs:436,451`.

Palettes and the fuzzy picker display the same mode as plain text wrapped in `-- … --` markers, e.g. `-- NORMAL --` and `-- INSERT --`, drawn in `model.match_color` or `model.hint_color` (both unrelated to the actual mode color). This looks visually inconsistent with the status bar.

## Goal

Unify the **mode-label text** in all palettes and the fuzzy picker with the status bar's mode color. The user explicitly chose the lightest level of consistency: **clean label + mode color only** — no pill background, no border, no dot. The status bar's pill UI is unchanged.

## Design

### Producer side — `src/app/command_palette.rs`

Add a sibling field next to the existing `vim_mode_label`:

```rust
pub vim_mode_label: Option<&'static str>,    // existing
pub vim_mode_color: Option<[f32; 4]>,        // NEW
```

The `CommandPaletteRenderModel` already carries a `&ThemeConfig` indirectly via the renderer; populating the color at producer time is the right place because `self.theme` and the live `EditorMode` (via `PaletteVimMode`) are both available there.

Producer (around line 1300) populates:
```rust
vim_mode_label: match self.vim_mode {
    PaletteVimMode::Insert => Some("INSERT"),
    PaletteVimMode::Normal => Some("NORMAL"),
    PaletteVimMode::Visual => Some("VISUAL"),
},
vim_mode_color: self.vim_mode.map(|m| {
    mode_pill_color(palette_vim_to_editor_mode(m), &self.theme)
}),
```

Add a tiny mapping helper next to `mode_display_label` / `mode_pill_color` in `src/render/renderer/helpers.rs`:
```rust
pub(super) fn palette_vim_to_editor_mode(m: PaletteVimMode) -> EditorMode {
    match m {
        PaletteVimMode::Insert  => EditorMode::Insert,
        PaletteVimMode::Normal  => EditorMode::Normal,
        PaletteVimMode::Visual  => EditorMode::Visual,
    }
}
```
This is also reused by the fuzzy picker (see below) so the two call sites stay in sync.

### Renderer side — 3 palette files

Same change pattern in each, applied to the existing `if let Some(label) = model.vim_mode_label { … }` block.

**Before** (e.g. `src/render/renderer/palette/minimal.rs:390-403`):
```rust
if let Some(label) = model.vim_mode_label {
    let text = format!("-- {label} --");
    let label_w = estimate_monospace_width(&text, font_size);
    let label_x = panel_x + panel_w - model.panel_padding - label_w;
    glyphs.extend(layout_panel_text(
        &text, …,
        model.hint_color,  // or model.match_color in file_picker.rs / recent_projects.rs
    ));
}
```

**After**:
```rust
if let Some(label) = model.vim_mode_label {
    let label_w = estimate_monospace_width(label, font_size);
    let label_x = panel_x + panel_w - model.panel_padding - label_w;
    glyphs.extend(layout_panel_text(
        label, …,
        model.vim_mode_color.unwrap_or(model.match_color),
    ));
}
```

Per file:
| File | Line range | Old color |
|---|---|---|
| `src/render/renderer/palette/minimal.rs` | 390–403 | `model.hint_color` |
| `src/render/renderer/palette/file_picker.rs` | 121–134 | `model.match_color` |
| `src/render/renderer/palette/recent_projects.rs` | 111–125 | `model.match_color` |

Width math already uses `chars().count() * font_size * 0.60` or `estimate_monospace_width` — both still work with the unwrapped `&'static str`. No need to remove `format!` allocation just for cosmetics, but it falls out naturally.

### Recent-projects helper

`recent_projects_vim_mode_status` becomes a simple pass-through. Update body and the test next to it.

**Before** (`src/render/renderer/palette/recent_projects.rs:492-511`):
```rust
fn recent_projects_vim_mode_status(label: Option<&'static str>) -> Option<String> {
    label.map(|label| format!("-- {label} --"))
}

#[test]
fn recent_projects_formats_vim_mode_status_like_other_pickers() {
    assert_eq!(recent_projects_vim_mode_status(Some("NORMAL")), Some("-- NORMAL --".to_string()));
    assert_eq!(recent_projects_vim_mode_status(Some("INSERT")), Some("-- INSERT --".to_string()));
    assert_eq!(recent_projects_vim_mode_status(None), None);
}
```

**After**:
```rust
fn recent_projects_vim_mode_status(label: Option<&'static str>) -> Option<&'static str> {
    label
}

#[test]
fn recent_projects_passes_vim_mode_label_through() {
    assert_eq!(recent_projects_vim_mode_status(Some("NORMAL")), Some("NORMAL"));
    assert_eq!(recent_projects_vim_mode_status(Some("INSERT")), Some("INSERT"));
    assert_eq!(recent_projects_vim_mode_status(None), None);
}
```
The function becomes trivial; consider inlining at the call site. Decision deferred to implementer — both are fine.

The render call site (line 111-125) switches color to `model.vim_mode_color.unwrap_or(model.match_color)` like the other two palettes.

### Fuzzy picker — `src/render/renderer/editor/fuzzy.rs`

The fuzzy picker does not use `CommandPaletteRenderModel`. It receives `editor_mode: EditorMode` directly and inlines a `vim_tag` string into the title via `format!`. To color the tag separately from the title we must split the header into two `layout_panel_text` calls.

**Before** (lines 114-158):
```rust
let vim_tag = match editor_mode {
    EditorMode::Insert  => "  -- INSERT --",
    EditorMode::Normal  => "  -- NORMAL --",
    EditorMode::Visual | EditorMode::VisualBlock => "  -- VISUAL --",
    _ => "",
};
let left_header = if is_live_grep {
    format!("{}{}  {} results · {} files", title, vim_tag, …)
} else {
    format!("{}{}  > {}", title, vim_tag, fuzzy_state.query)
};
glyphs.extend(layout_panel_text(&clamp_monospace_text(&left_header, …), …, fg));
```

**After**:
```rust
let (vim_tag, vim_color): (&str, Option<[f32;4]>) = match editor_mode {
    EditorMode::Insert  => (mode_display_label(EditorMode::Insert),  Some(mode_pill_color(EditorMode::Insert,  &self.theme))),
    EditorMode::Normal  => (mode_display_label(EditorMode::Normal),  Some(mode_pill_color(EditorMode::Normal,  &self.theme))),
    EditorMode::Visual | EditorMode::VisualBlock => (mode_display_label(EditorMode::Visual), Some(mode_pill_color(EditorMode::Visual, &self.theme))),
    _ => ("", None),
};
let prefix_text = if is_live_grep {
    format!("{}  ", title)
} else {
    format!("{}  > {}", title, fuzzy_state.query)
};
// 1) title + leading text in fg, 2) vim_tag in mode color, 3) optional tail in fg_dim
let prefix_w = estimate_monospace_width(&prefix_text, font_size);
glyphs.extend(layout_panel_text(&clamp_monospace_text(&prefix_text, …), …, fg));
if !vim_tag.is_empty() {
    let mut x = left_x + 10.0 * s + prefix_w;
    glyphs.extend(layout_panel_text(
        vim_tag, …, vim_color.unwrap_or(fg),
    ));
    x += estimate_monospace_width(vim_tag, font_size);
    // Live-grep: still need "  N results · N files" after the tag → append that suffix in fg_dim.
    if is_live_grep {
        let tail = format!("  {} results · {} files", fuzzy_state.results.len(), unique_file_count);
        glyphs.extend(layout_panel_text(&clamp_monospace_text(&tail, …), …, fg_dim));
    }
}
```

The exact x positioning can be simplified using the same width-measurement idiom already used in the status bar (`estimate_monospace_width` + accumulator). The implementer should keep clamp logic unchanged to avoid overflow on narrow widths.

`mode_display_label` and `mode_pill_color` are already `pub(super)` in `helpers.rs`; the fuzzy picker file needs to import them (it currently imports other helpers via `use super::super::helpers::…`).

## Theme & UX

- NORMAL → `theme.ui.mode_normal`
- INSERT → `theme.ui.mode_insert`
- VISUAL → `theme.ui.mode_visual` (also used for VisualBlock)
- Changing theme changes all four locations in lockstep with the status bar.
- When `vim_mode_label = None`: palette falls back to `model.match_color` (no behavior change for palettes that have no mode indicator). Fuzzy picker simply omits the tag (current behavior — no `-- --` rendered).

## Out of scope

- The status bar itself (already a full pill, untouched).
- The help screen's section title check (`src/render/renderer/editor/help.rs:380` — only matches the title string to pick a section color, not a label to render).
- Help content text in `src/app/app_state/mod.rs:1140-1141` ("NORMAL mode — Default…" prose) — these are documentation, not label rendering.
- The `--glob` fzf argument in `src/async_runtime/scheduler/fzf.rs:216` — unrelated token.

## Tests

### Update existing
- `recent_projects_formats_vim_mode_status_like_other_pickers` → renamed to `recent_projects_passes_vim_mode_label_through`, expected values drop the `--` markers (see above).

### Add new
1. **Producer test** in `src/app/command_palette.rs` (next to `paste_overlay_render_exposes_vim_mode_label_and_block_caret` at line 2082):
   - Build a `CommandPalette` in `Insert` and `Normal` vim modes.
   - Call `render_model(&self.theme)` and assert `model.vim_mode_color` is `Some(...)` and matches `mode_pill_color(EditorMode::Insert/Normal, theme)`.
2. **Producer fallback test** (same file):
   - Build a palette where `self.vim_mode = None` (if such a state exists) and assert `vim_mode_color = None`. If the type doesn't allow that state, skip this test.
3. **Renderer snapshot / glyph test** (optional, low priority):
   - Construct a minimal `CommandPaletteRenderModel` with `vim_mode_label = Some("NORMAL")`, `vim_mode_color = Some(red)`, and assert the produced glyphs contain a run with color = red and text = "NORMAL" (no "--"). Use the existing render test infrastructure if present; otherwise a manual visual check is acceptable.
4. **Fuzzy picker test** (optional, low priority):
   - Call `update_fuzzy_picker_buffer_content` with `editor_mode = Normal` and assert at least one glyph run has color close to `mode_pill_color(Normal, theme)` and text = "NORMAL" (no leading "--" or spaces inside).

If automated renderer tests are too heavy, the test suite still gains value from the producer tests and the helper test.

## Acceptance criteria

1. `cargo build --workspace` passes.
2. `cargo test --workspace` passes, including updated/added tests.
3. Manual: open each palette (Command palette, File picker, Recent projects, Fuzzy search) in NORMAL and INSERT modes and visually confirm:
   - No `--` markers around the mode label.
   - Label color matches the status bar's mode pill color.
4. The status bar itself is unchanged.
5. `npx gitnexus analyze` re-runs cleanly (per AGENTS.md structure-change rule — not strictly a structure change, but keeps the index fresh).

## Risk

- **LOW.** All four sites are local text-rendering changes. The data model change is additive (one new `Option<[f32;4]>` field). No async, no commands, no public API change beyond the model.
- The fuzzy-picker split into 3 text runs is the only non-trivial piece; careful x-coordinate math is required to keep alignment on narrow widths. The existing `clamp_monospace_text` + `estimate_monospace_width` pattern (used in the status bar) is the right tool.

## Rollback

Revert the commit. No data migrations, no persistent state.
