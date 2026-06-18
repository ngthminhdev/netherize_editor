# Palette Vim-Mode Label Style Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace `-- NORMAL --` / `-- INSERT --` text in 3 palette renderers and the fuzzy picker with clean `NORMAL` / `INSERT` text colored by `mode_pill_color(editor_mode, theme)` (same helper the status bar uses). No pill, no border, no dot — just colored text.

**Architecture:** Add one new `Option<[f32;4]>` field (`vim_mode_color`) to the existing `CommandPaletteRenderModel`. The producer populates the color directly from `theme.ui.mode_normal/insert/visual.as_f32()` (no cross-module helper needed). The 3 palette renderers just read the field. The fuzzy picker computes the color locally via `mode_pill_color(editor_mode, &self.theme)` (it already has `EditorMode`). One updated test + one new producer test.

**Tech Stack:** Rust, wgpu, thiserror-free `anyhow::Result` patterns already in the codebase, no new crates.

**Spec:** `docs/superpowers/specs/2026-06-19-palette-vim-mode-label-style.md`

---

## File Structure

| File | Change | Reason |
|---|---|---|
| `src/app/command_palette.rs` | Add `vim_mode_color` field; populate in producer using `theme.ui.mode_*.as_f32()` directly; add 1 test | Single source of truth for the model. Producer has `theme: &ThemeConfig` already, so no cross-module helper needed |
| `src/render/renderer/palette/minimal.rs` | Switch to `model.vim_mode_color`; drop `--` wrapper | Command palette header |
| `src/render/renderer/palette/file_picker.rs` | Switch to `model.vim_mode_color`; drop `--` wrapper | File picker header |
| `src/render/renderer/palette/recent_projects.rs` | Switch to `model.vim_mode_color`; simplify helper to pass-through; update test | Recent projects header |
| `src/render/renderer/editor/fuzzy.rs` | Split header text into title + colored vim tag (3 text runs); import `mode_display_label` + `mode_pill_color` | Fuzzy picker title. Uses `mode_pill_color(editor_mode, &self.theme)` directly (EditorMode is already available) |

No new files, no modules restructured, no public API beyond the one additive model field. **No new helper needed** — early plans called for a `palette_vim_to_editor_mode` helper, but the producer has direct `ThemeConfig` access and the fuzzy picker has `EditorMode` directly, so the helper would be dead code. (Initial Task 1 attempt was reverted.)

---

## Task 1: Add `vim_mode_color` field + populate in producer

**Files:**
- Modify: `src/app/command_palette.rs:500-520` (struct definition) and `:1280-1310` (producer) and `:2082-2101` (test)
- Test: same file, add 1 new test next to `paste_overlay_render_exposes_vim_mode_label_and_block_caret`

- [ ] **Step 1: Write the failing test**

In `src/app/command_palette.rs`, add this test directly after `paste_overlay_render_exposes_vim_mode_label_and_block_caret` (currently ending at line 2101):

```rust
    #[test]
    fn paste_overlay_render_exposes_vim_mode_color_per_mode() {
        let theme = ThemeConfig::builtin_dark();
        let mut p = CommandPalette::default();
        p.open(CommandPaletteMode::ExplorerPasteFile, None);
        p.set_query("docker-compose (1).yml", None);

        // Insert: color must match theme.ui.mode_insert.
        let insert = p
            .render(&theme, [0.0, 0.0, 1200.0, 800.0])
            .expect("insert render model");
        assert_eq!(insert.vim_mode_color, Some(theme.ui.mode_insert.as_f32()));

        // Normal: color must match theme.ui.mode_normal.
        p.vim_input(PaletteVimKey::Esc, false, None);
        let normal = p
            .render(&theme, [0.0, 0.0, 1200.0, 800.0])
            .expect("normal render model");
        assert_eq!(normal.vim_mode_color, Some(theme.ui.mode_normal.as_f32()));
    }
```

- [ ] **Step 2: Run test to verify it fails**

Run:
```bash
cd /Users/qc-bright/Project/netherize_editor && cargo test -p netherize_editor paste_overlay_render_exposes_vim_mode_color_per_mode -- --nocapture
```
Expected: compile error (`vim_mode_color` field not found on `CommandPaletteRenderModel`).

- [ ] **Step 3: Add the field to the model**

In `src/app/command_palette.rs`, find the `CommandPaletteRenderModel` struct (search `pub vim_mode_label: Option<&'static str>` — line 508). Add the new field directly after it:

```rust
    pub vim_mode_label: Option<&'static str>,
    pub vim_mode_color: Option<[f32; 4]>,
```

- [ ] **Step 4: Populate the field in the producer**

In the same file, find the `vim_mode_label: match self.vim_mode { … }` block (line 1300). Read the surrounding producer code to see how `theme` flows through. The `render()` method (line 1004) takes `theme: &ThemeConfig` as a parameter. Inside the producer body you will either see `theme` available as a local binding, or you'll see `self.theme` if the palette struct caches the theme. **Whichever form is used in nearby lines, mirror that form for the new line.** Directly after the `vim_mode_label:` block, add:

```rust
            vim_mode_color: match self.vim_mode {
                PaletteVimMode::Insert => Some(<THEME_ACCESSOR>.ui.mode_insert.as_f32()),
                PaletteVimMode::Normal => Some(<THEME_ACCESSOR>.ui.mode_normal.as_f32()),
                PaletteVimMode::Visual => Some(<THEME_ACCESSOR>.ui.mode_visual.as_f32()),
            },
```

Where `<THEME_ACCESSOR>` is whatever the surrounding producer code uses for the theme reference (e.g. `theme` or `self.theme`).

- [ ] **Step 5: Run the test to verify it passes**

Run:
```bash
cd /Users/qc-bright/Project/netherize_editor && cargo test -p netherize_editor paste_overlay_render_exposes_vim_mode_color_per_mode -- --nocapture
```
Expected: PASS (1 test). If `assert_eq` fails, re-check that `<THEME_ACCESSOR>` resolves to the same `ThemeConfig` instance the test passed in.

- [ ] **Step 6: Verify the existing label test still passes**

Run:
```bash
cd /Users/qc-bright/Project/netherize_editor && cargo test -p netherize_editor paste_overlay_render_exposes_vim_mode_label_and_block_caret -- --nocapture
```
Expected: PASS (regression check).

- [ ] **Step 7: Commit**

```bash
cd /Users/qc-bright/Project/netherize_editor && git add src/app/command_palette.rs && git commit -m "feat(palette): add vim_mode_color field to render model"
```

---

## Task 2: Update `render_command_palette_minimalist` (minimal.rs)

**Files:**
- Modify: `src/render/renderer/palette/minimal.rs:390-403`

- [ ] **Step 1: Replace the label block**

Find (line 390):
```rust
            if let Some(label) = model.vim_mode_label {
                let text = format!("-- {label} --");
                let label_w = estimate_monospace_width(&text, font_size);
                let label_x = panel_x + panel_w - model.panel_padding - label_w;
                glyphs.extend(layout_panel_text(
                    &text,
                    &mut self.palette_text_system,
                    &mut self.atlas,
                    &self.queue,
                    label_x,
                    prompt_y,
                    model.hint_color,
                ));
            }
```

Replace with:
```rust
            if let Some(label) = model.vim_mode_label {
                let label_w = estimate_monospace_width(label, font_size);
                let label_x = panel_x + panel_w - model.panel_padding - label_w;
                glyphs.extend(layout_panel_text(
                    label,
                    &mut self.palette_text_system,
                    &mut self.atlas,
                    &self.queue,
                    label_x,
                    prompt_y,
                    model.vim_mode_color.unwrap_or(model.hint_color),
                ));
            }
```

- [ ] **Step 2: Build to confirm no warnings/errors**

Run:
```bash
cd /Users/qc-bright/Project/netherize_editor && cargo build -p netherize_editor 2>&1 | tail -40
```
Expected: success, no warnings about unused `format!` or `text` binding (we removed them).

- [ ] **Step 3: Commit**

```bash
cd /Users/qc-bright/Project/netherize_editor && git add src/render/renderer/palette/minimal.rs && git commit -m "refactor(palette): use vim_mode_color in command palette header"
```

---

## Task 3: Update `render_file_picker_complex` (file_picker.rs)

**Files:**
- Modify: `src/render/renderer/palette/file_picker.rs:120-134`

- [ ] **Step 1: Replace the label block**

Find (line 120):
```rust
        // Vim mode indicator, drawn to the left of the result count.
        if let Some(label) = model.vim_mode_label {
            let vim_text = format!("-- {label} --");
            let vim_w = vim_text.chars().count() as f32 * font_size * 0.60;
            let vim_x = (count_x - vim_w - 16.0).max(query_x + prefix_w);
            glyphs.extend(layout_panel_text(
                &vim_text,
                &mut self.palette_text_system,
                &mut self.atlas,
                &self.queue,
                vim_x,
                query_y,
                model.match_color,
            ));
        }
```

Replace with:
```rust
        // Vim mode indicator, drawn to the left of the result count.
        if let Some(label) = model.vim_mode_label {
            let vim_w = label.chars().count() as f32 * font_size * 0.60;
            let vim_x = (count_x - vim_w - 16.0).max(query_x + prefix_w);
            glyphs.extend(layout_panel_text(
                label,
                &mut self.palette_text_system,
                &mut self.atlas,
                &self.queue,
                vim_x,
                query_y,
                model.vim_mode_color.unwrap_or(model.match_color),
            ));
        }
```

- [ ] **Step 2: Build to confirm no warnings/errors**

Run:
```bash
cd /Users/qc-bright/Project/netherize_editor && cargo build -p netherize_editor 2>&1 | tail -40
```
Expected: success.

- [ ] **Step 3: Commit**

```bash
cd /Users/qc-bright/Project/netherize_editor && git add src/render/renderer/palette/file_picker.rs && git commit -m "refactor(palette): use vim_mode_color in file picker header"
```

---

## Task 4: Simplify `recent_projects_vim_mode_status` + update its test + use new color

**Files:**
- Modify: `src/render/renderer/palette/recent_projects.rs:111-125, 492-511`

- [ ] **Step 1: Update the test to expect pass-through (TDD — write first)**

In `src/render/renderer/palette/recent_projects.rs` (line 501), replace the existing test:

```rust
    #[test]
    fn recent_projects_passes_vim_mode_label_through() {
        assert_eq!(recent_projects_vim_mode_status(Some("NORMAL")), Some("NORMAL"));
        assert_eq!(recent_projects_vim_mode_status(Some("INSERT")), Some("INSERT"));
        assert_eq!(recent_projects_vim_mode_status(None), None);
    }
```

- [ ] **Step 2: Run test to verify it fails**

Run:
```bash
cd /Users/qc-bright/Project/netherize_editor && cargo test -p netherize_editor recent_projects_passes_vim_mode_label_through -- --nocapture
```
Expected: FAIL — `recent_projects_vim_mode_status` still returns `Some("-- NORMAL --".to_string())`.

- [ ] **Step 3: Simplify the helper**

Find (line 492):
```rust
fn recent_projects_vim_mode_status(label: Option<&'static str>) -> Option<String> {
    label.map(|label| format!("-- {label} --"))
}
```

Replace with:
```rust
fn recent_projects_vim_mode_status(label: Option<&'static str>) -> Option<&'static str> {
    label
}
```

- [ ] **Step 4: Run test to verify it passes**

Run:
```bash
cd /Users/qc-bright/Project/netherize_editor && cargo test -p netherize_editor recent_projects_passes_vim_mode_label_through -- --nocapture
```
Expected: PASS.

- [ ] **Step 5: Update the render call site to use `vim_mode_color`**

Find (line 111):
```rust
        if !is_theme_selector
            && let Some(vim_text) = recent_projects_vim_mode_status(model.vim_mode_label)
        {
            let vim_w = estimate_monospace_width(&vim_text, font_size);
            let vim_x = (count_x - vim_w - 16.0).max(text_x + badge_w + 16.0);
            glyphs.extend(layout_panel_text(
                &vim_text,
                &mut self.palette_text_system,
                &mut self.atlas,
                &self.queue,
                vim_x,
                header_text_y,
                model.match_color,
            ));
        }
```

Replace with:
```rust
        if !is_theme_selector
            && let Some(vim_text) = recent_projects_vim_mode_status(model.vim_mode_label)
        {
            let vim_w = estimate_monospace_width(vim_text, font_size);
            let vim_x = (count_x - vim_w - 16.0).max(text_x + badge_w + 16.0);
            glyphs.extend(layout_panel_text(
                vim_text,
                &mut self.palette_text_system,
                &mut self.atlas,
                &self.queue,
                vim_x,
                header_text_y,
                model.vim_mode_color.unwrap_or(model.match_color),
            ));
        }
```

- [ ] **Step 6: Build to confirm no warnings/errors**

Run:
```bash
cd /Users/qc-bright/Project/netherize_editor && cargo build -p netherize_editor 2>&1 | tail -40
```
Expected: success.

- [ ] **Step 7: Commit**

```bash
cd /Users/qc-bright/Project/netherize_editor && git add src/render/renderer/palette/recent_projects.rs && git commit -m "refactor(palette): simplify vim mode helper and use vim_mode_color"
```

---

## Task 5: Update fuzzy picker header (split text runs)

**Files:**
- Modify: `src/render/renderer/editor/fuzzy.rs:1-15` (imports), `:114-199` (header rendering)

- [ ] **Step 1: Add missing imports**

In `src/render/renderer/editor/fuzzy.rs`, find the `use super::…` or `use crate::…` block (top of file). Add `mode_display_label` and `mode_pill_color` to the helper imports. Current import pattern (search for `use super::super::helpers::`) — extend it. If the existing import line is:
```rust
use super::super::helpers::{clamp_monospace_text, estimate_monospace_width, layout_panel_text, …};
```
Add `mode_display_label, mode_pill_color` to that list. (Also check if `EditorMode` is imported — it almost certainly is since the file uses `editor_mode: EditorMode` in a match; if not, add it.)

- [ ] **Step 2: Replace the inline `vim_tag` and the format strings**

Find (line 114-158):
```rust
        let vim_tag = match editor_mode {
            crate::core::mode::EditorMode::Insert => "  -- INSERT --",
            crate::core::mode::EditorMode::Normal => "  -- NORMAL --",
            crate::core::mode::EditorMode::Visual | crate::core::mode::EditorMode::VisualBlock => {
                "  -- VISUAL --"
            }
            _ => "",
        };
        let unique_file_count = if is_live_grep {
            let mut paths = Vec::<String>::new();
            // … (unchanged)
        } else {
            0
        };
        let left_header = if is_live_grep {
            format!(
                "{}{}  {} results · {} files",
                title,
                vim_tag,
                fuzzy_state.results.len(),
                unique_file_count
            )
        } else {
            format!("{}{}  > {}", title, vim_tag, fuzzy_state.query)
        };
```

Replace this entire block (everything from the `let vim_tag = match …` line through `let left_header = …`) with:

```rust
        let (vim_tag, vim_color): (&str, Option<[f32; 4]>) = match editor_mode {
            crate::core::mode::EditorMode::Insert => (
                mode_display_label(crate::core::mode::EditorMode::Insert),
                Some(mode_pill_color(crate::core::mode::EditorMode::Insert, &self.theme)),
            ),
            crate::core::mode::EditorMode::Normal => (
                mode_display_label(crate::core::mode::EditorMode::Normal),
                Some(mode_pill_color(crate::core::mode::EditorMode::Normal, &self.theme)),
            ),
            crate::core::mode::EditorMode::Visual | crate::core::mode::EditorMode::VisualBlock => (
                mode_display_label(crate::core::mode::EditorMode::Visual),
                Some(mode_pill_color(crate::core::mode::EditorMode::Visual, &self.theme)),
            ),
            _ => ("", None),
        };
        let unique_file_count = if is_live_grep {
            let mut paths = Vec::<String>::new();
            // … (unchanged — the entire unique_file_count computation stays as-is)
        } else {
            0
        };
        // Compose the pieces: title (fg) · vim_tag (mode color) · optional tail (fg or fg_dim).
        let left_header = if is_live_grep {
            format!("{}  ", title)
        } else {
            format!("{}  > {}", title, fuzzy_state.query)
        };
```

- [ ] **Step 3: Replace the two `layout_panel_text` calls that consume `left_header`**

Find (around line 163-199):
```rust
        if is_live_grep {
            let header_count_w = estimate_monospace_width(" 999 results · 999 files", font_size).min(left_w * 0.55);
            glyphs.extend(layout_panel_text(
                title,
                …,
                fg,
            ));
            let count_label = format!("{} results · {} files", fuzzy_state.results.len(), unique_file_count);
            glyphs.extend(layout_panel_text(
                &clamp_monospace_text(&count_label, header_count_w, font_size),
                …,
                fg_dim,
            ));
        } else {
            glyphs.extend(layout_panel_text(
                &clamp_monospace_text(&left_header, (left_w - 20.0 * s).max(1.0), font_size),
                …,
                fg,
            ));
        }
```

Replace with:
```rust
        if is_live_grep {
            // 1) title (fg) + 2 spaces, 2) vim tag (mode color), 3) count suffix (fg_dim).
            let prefix_w = estimate_monospace_width(&format!("{}  ", title), font_size);
            glyphs.extend(layout_panel_text(
                title,
                &mut self.editor_overlay_text_system,
                &mut self.atlas,
                &self.queue,
                left_x + 10.0 * s,
                header_y,
                fg,
            ));
            if !vim_tag.is_empty() {
                glyphs.extend(layout_panel_text(
                    vim_tag,
                    &mut self.editor_overlay_text_system,
                    &mut self.atlas,
                    &self.queue,
                    left_x + 10.0 * s + prefix_w,
                    header_y,
                    vim_color.unwrap_or(fg),
                ));
            }
            let header_count_w = estimate_monospace_width(" 999 results · 999 files", font_size).min(left_w * 0.55);
            let count_label = format!("{} results · {} files", fuzzy_state.results.len(), unique_file_count);
            glyphs.extend(layout_panel_text(
                &clamp_monospace_text(&count_label, header_count_w, font_size),
                &mut self.editor_overlay_text_system,
                &mut self.atlas,
                &self.queue,
                (left_x + left_w - header_count_w - 10.0 * s).max(left_x + 90.0 * s),
                header_y,
                fg_dim,
            ));
        } else {
            // 1) prefix (title + spaces + query) in fg, 2) vim tag in mode color.
            let prefix_w = estimate_monospace_width(&left_header, font_size);
            glyphs.extend(layout_panel_text(
                &clamp_monospace_text(&left_header, (left_w - 20.0 * s).max(1.0), font_size),
                &mut self.editor_overlay_text_system,
                &mut self.atlas,
                &self.queue,
                left_x + 10.0 * s,
                header_y,
                fg,
            ));
            if !vim_tag.is_empty() {
                glyphs.extend(layout_panel_text(
                    vim_tag,
                    &mut self.editor_overlay_text_system,
                    &mut self.atlas,
                    &self.queue,
                    left_x + 10.0 * s + prefix_w,
                    header_y,
                    vim_color.unwrap_or(fg),
                ));
            }
        }
```

- [ ] **Step 4: Build to confirm no warnings/errors**

Run:
```bash
cd /Users/qc-bright/Project/netherize_editor && cargo build -p netherize_editor 2>&1 | tail -40
```
Expected: success. If you see "unused variable: `left_header`" or similar, the prefix composition or use site is wrong — re-check that `left_header` is still referenced in the non-live-grep branch.

- [ ] **Step 5: Commit**

```bash
cd /Users/qc-bright/Project/netherize_editor && git add src/render/renderer/editor/fuzzy.rs && git commit -m "refactor(fuzzy): split header into title + colored vim tag"
```

---

## Task 6: Final verification

**Files:** none (verification only)

- [ ] **Step 1: Run the full test suite**

Run:
```bash
cd /Users/qc-bright/Project/netherize_editor && cargo test --workspace 2>&1 | tail -60
```
Expected: all tests pass, including:
- `paste_overlay_render_exposes_vim_mode_label_and_block_caret` (regression)
- `paste_overlay_render_exposes_vim_mode_color_per_mode`
- `recent_projects_passes_vim_mode_label_through`

- [ ] **Step 2: Run lints (clippy)**

Run:
```bash
cd /Users/qc-bright/Project/netherize_editor && cargo clippy -p netherize_editor -- -D warnings 2>&1 | tail -40
```
Expected: no new warnings introduced (pre-existing warnings OK; flag any that come from files we touched).

- [ ] **Step 3: Re-run GitNexus analytics (per AGENTS.md)**

Run:
```bash
cd /Users/qc-bright/Project/netherize_editor && npx gitnexus analyze 2>&1 | tail -30
```
Expected: index refreshed without errors.

- [ ] **Step 4: Manual visual check**

Build and launch the editor (commands per `BUILD.md`). For each palette in both NORMAL and INSERT:
- Command palette (Ctrl+Shift+P or similar)
- File picker (Ctrl+P)
- Recent projects
- Fuzzy file search (Ctrl+Shift+F)
- Live grep
Confirm: no `--` markers around the mode label; label is colored (blue-ish for NORMAL, green-ish for INSERT in the default dark theme) and matches the status bar pill color.

- [ ] **Step 5: Commit (verification log, optional)**

If any verification artifact was generated, commit it. Otherwise skip — the feature work is already committed across tasks 1–5.

---

## Self-Review Notes

- **Spec coverage:**
  - Producer field + tests → Task 1 ✓
  - Command palette header → Task 2 ✓
  - File picker header → Task 3 ✓
  - Recent projects helper + test update → Task 4 ✓
  - Fuzzy picker split → Task 5 ✓
  - Build/test/lint/analyze → Task 6 ✓
- **Placeholder scan:** no TBDs, no "implement later", all code blocks complete. The `<THEME_ACCESSOR>` placeholder in Task 1 Step 4 is intentional — it forces the implementer to read the actual producer body and use the same theme-access pattern as surrounding code, rather than guessing `self.theme` vs `theme`.
- **Type consistency:** `vim_mode_color: Option<[f32; 4]>` is used identically across Tasks 1–4. `mode_pill_color` and `mode_display_label` are used identically in Task 5 (already `pub(super)`, accessible via `use super::super::helpers::…`).
- **Risk:** LOW. Each task is independently committable and testable. Task 5 is the only multi-step edit; the prefix/vim-tag x-coordinate math mirrors the status bar's `estimate_monospace_width` accumulator pattern.
- **Removed in revision:** Original plan had a `palette_vim_to_editor_mode` helper in `helpers.rs`. Reverted after code review revealed it would have been dead code (producer has direct `ThemeConfig` access, fuzzy picker has `EditorMode` directly). New plan uses `theme.ui.mode_*.as_f32()` in the producer instead.
