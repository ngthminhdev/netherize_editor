# Key Repeat j/k in Palette Dropdowns + Always-Visible Caret

**Date:** 2026-06-19
**Status:** Approved
**Scope:** `src/core/commands.rs`, `src/app/event_loop/application.rs`, optional test in `src/app/input/tests.rs`

## Problem

Two small UX gaps surfaced after the palette vim-mode label refactor:

1. **Holding `j`/`k` in a palette/fuzzy-picker dropdown doesn't repeat.** When the user presses and holds `j` (or `k`) to scroll the result list in Normal mode, the OS auto-repeat fires `KeyboardInput` events with `repeat=true`. The input router funnels those through `route_repeated_normalized_input` (`src/app/input/handler.rs:240`), which only forwards the resolved command if it appears in `Command::supports_press_and_hold_repeat()` (`src/core/commands.rs:578`). In palette Normal mode, `j`/`k` resolve to `Command::PaletteVimInput(PaletteVimKey::Char('j' | 'k'))` (see `src/app/input_map/focus.rs:980`), and that variant is **not** in the whitelist — so every repeat event is silently dropped. (RecentProjects already works because its `j`/`k` resolve directly to `OverlaySelectNext`/`Prev`, which ARE whitelisted.)

2. **The editor caret blinks every 1 s, which the user finds distracting.** `tick_caret_blink()` (`src/app/event_loop/application.rs:1269`) toggles `caret_blink_visible` off and on each second. The user wants the caret to stay solid (always visible).

## Goal

1. Let held `j`/`k` repeat through `PaletteVimInput` so the palette/fuzzy-picker dropdown scrolls continuously.
2. Stop the caret from blinking — it stays visible at all times.

## Design

### Feature 1 — Key repeat for `PaletteVimInput(Char('j' | 'k'))`

Add two arms to the `supports_press_and_hold_repeat()` whitelist in `src/core/commands.rs:578`:

```rust
| Self::PaletteVimInput(crate::core::commands::PaletteVimKey::Char('j'))
| Self::PaletteVimInput(crate::core::commands::PaletteVimKey::Char('k'))
```

(Rust will resolve `PaletteVimKey` from the same module since the enum is defined in this file at line 8 — use bare `PaletteVimKey::Char('j')`.)

**Safety:**
- The repeat guard at `handler.rs:248-253` already drops repeat events when a pending count, operator, or chord is in flight — so `3j`, `dj`, `g…` cannot be auto-completed by OS repeat. The whitelist only controls *whether a resolved command may fire on repeat*, not whether the pending-state guard runs.
- In palette Insert mode, `j`/`k` do NOT route to `PaletteVimInput` — they fall through to `Command::FilePickerAppendQuery` (text append), which is already handled. So the new whitelist arms only affect Normal/Visual palette modes.
- Applies to both the overlay palette and the fuzzy picker buffer, since both funnel through `Command::PaletteVimInput` → `handle_palette_vim_input` → `PaletteVimAction::ListNext/ListPrev` → `OverlaySelectNext/Prev` (`src/app/event_loop/commands_palette.rs:30-39, 58-67`).
- Only `j` and `k` are whitelisted — not arbitrary `PaletteVimInput(Char(_))`. Other palette Normal keys (`x`, `d`, `c`, `y`, `0`, `$`, etc.) remain single-press only, preserving Vim semantics (you don't want `dd` to fire from a held `d`).

### Feature 2 — Disable caret blink

Replace the body of `tick_caret_blink()` in `src/app/event_loop/application.rs:1269` so it never toggles visibility:

```rust
fn tick_caret_blink(&mut self) -> bool {
    // Blink disabled — caret stays visible at all times.
    // Cursor moves and mode changes still reset caret_blink_visible = true
    // (see the 10+ reset sites in application.rs, viewport.rs, terminal.rs).
    false
}
```

**Why this is enough:**
- `tick_caret_blink` is the ONLY place that ever sets `caret_blink_visible = false`. Every other site sets it to `true` (after cursor moves, viewport scrolls, terminal focus, mode transitions). With the toggle gone, `caret_blink_visible` is permanently `true`.
- The `caret_blink_dirty` flag never gets set, so `application.rs:2666-2675` (the blink-dirty apply block) becomes a no-op — no GPU upload churn from blink.
- `last_caret_blink_tick` is now unused on the read path; leaving the field in place keeps the struct layout stable and avoids touching `AppShell`'s definition. (A follow-up could remove it, but YAGNI — it's a single `Instant`.)

## Out of scope

- Removing the `caret_blink_visible` / `last_caret_blink_tick` fields entirely. They're now vestigial but harmless; a cleanup pass is out of scope.
- Making blink configurable via settings. The user explicitly chose "always visible, no toggle".
- Key repeat for other `PaletteVimInput` chars (e.g. `Ctrl+d`/`Ctrl+u` for page scroll — palette doesn't have those motions anyway).
- The `RecentProjects` palette — it already repeats correctly via `OverlaySelectNext/Prev` and needs no change.

## Tests

### New test — key repeat for palette `j`/`k`

Add to `src/app/input/tests.rs` next to `repeated_motion_key_dispatches_while_holding` (line 1169):

```rust
#[test]
fn repeated_j_in_palette_normal_dispatches_vim_input() {
    let mut handler = InputHandler::new();
    let map = make_map();
    let mut context = KeybindingContext::for_mode_with_picker(EditorMode::PaletteFocus, true);
    context.palette_vim_mode = Some(PaletteVimMode::Normal);

    let repeated =
        handler.route_repeated_normalized_input(char_input('j', KeyCode::KeyJ), &map, context);
    match repeated {
        Some(InputRouteOutcome::Dispatch(translated)) => {
            assert_eq!(
                translated.command,
                Command::PaletteVimInput(PaletteVimKey::Char('j'))
            );
            assert_eq!(translated.repeat_count, 1);
        }
        other => panic!("expected repeated palette j dispatch, got {:?}", other),
    }
}
```

Mirror with `k` for symmetry. The test fails before the whitelist change (returns `None` because `PaletteVimInput` isn't whitelisted) and passes after.

### Existing tests — no regression expected

- `repeated_motion_key_dispatches_while_holding` (`tests.rs:1169`) — unchanged, still passes.
- `repeated_toggle_command_is_ignored_while_holding` (`tests.rs:1233`) — unchanged; toggles still aren't whitelisted.
- All caret-rendering tests (if any) — unchanged; the caret pipeline still uploads rects on layout, only the blink toggle is gone.

### Manual verification (deferred to user)

- Hold `j` in command palette / file picker / fuzzy picker / live grep (all in Normal mode) — list scrolls continuously.
- Hold `k` — scrolls up continuously.
- Press `j` once — moves one item (no double-fire).
- Type `3j` — moves three items (count still works; repeat guard blocks the OS-repeat from interfering).
- Editor caret is solid, never blinks, in all modes (Normal, Insert, Visual, Terminal, etc.).

## Risk

- **LOW.** Two localized edits. The whitelist addition is additive (only `j`/`k` gain repeat support; no existing command loses it). The blink disable is a single function body change with no signature or field removal.
- The key-repeat guard at `handler.rs:248-253` is the safety net for chord/count/operator state — it runs before the whitelist check, so the whitelist change cannot accidentally complete a `dj` or `3j` from OS repeat.

## Rollback

Revert the two commits. No data migrations, no persistent state.
