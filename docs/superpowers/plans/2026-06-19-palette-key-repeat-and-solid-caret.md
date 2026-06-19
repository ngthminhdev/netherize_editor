# Key Repeat j/k + Solid Caret Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** (1) Let held `j`/`k` repeat through `PaletteVimInput` so palette/fuzzy-picker dropdowns scroll continuously. (2) Stop the editor caret from blinking so it stays solid.

**Architecture:** Two localized edits. Add two arms to the `supports_press_and_hold_repeat()` whitelist in `src/core/commands.rs` so `PaletteVimInput(Char('j' | 'k'))` may fire on OS key-repeat. Replace the body of `tick_caret_blink()` in `src/app/event_loop/application.rs` so it returns `false` and never toggles `caret_blink_visible` (which every other site already keeps at `true`).

**Tech Stack:** Rust, wgpu, existing input router + caret pipeline. No new crates.

**Spec:** `docs/superpowers/specs/2026-06-19-palette-key-repeat-and-solid-caret.md`

---

## File Structure

| File | Change | Reason |
|---|---|---|
| `src/core/commands.rs` | +2 arms in `supports_press_and_hold_repeat()` whitelist (line 578) | Allow `j`/`k` repeat in palette Normal mode |
| `src/app/input/tests.rs` | +1 test (`repeated_j_in_palette_normal_dispatches_vim_input`) | TDD guard for the whitelist |
| `src/app/event_loop/application.rs` | Body of `tick_caret_blink()` → return `false` (line 1269) | Disable blink, caret always visible |

No new files, no struct changes, no public API change.

---

## Task 1: Add `PaletteVimInput(Char('j' | 'k'))` to key repeat whitelist + test

**Files:**
- Modify: `src/core/commands.rs:578` (whitelist function)
- Test: `src/app/input/tests.rs` (add test after line 1193, next to `repeated_motion_key_dispatches_while_holding`)

- [ ] **Step 1: Write the failing test**

In `src/app/input/tests.rs`, add this test directly after `repeated_motion_key_dispatches_while_holding` (which ends at line 1193). The test needs `PaletteVimMode` and `PaletteVimKey` — these are NOT in the file's top imports, so import them inside the test (mirroring the pattern at line 2256-2257 of the same file):

```rust
#[test]
fn repeated_j_in_palette_normal_dispatches_vim_input() {
    use crate::app::command_palette::PaletteVimMode;
    use crate::core::commands::PaletteVimKey;

    let mut handler = InputHandler::new();
    let map = make_map();
    let mut context =
        KeybindingContext::for_mode_with_picker(EditorMode::PaletteFocus, true);
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

#[test]
fn repeated_k_in_palette_normal_dispatches_vim_input() {
    use crate::app::command_palette::PaletteVimMode;
    use crate::core::commands::PaletteVimKey;

    let mut handler = InputHandler::new();
    let map = make_map();
    let mut context =
        KeybindingContext::for_mode_with_picker(EditorMode::PaletteFocus, true);
    context.palette_vim_mode = Some(PaletteVimMode::Normal);

    let repeated =
        handler.route_repeated_normalized_input(char_input('k', KeyCode::KeyK), &map, context);
    match repeated {
        Some(InputRouteOutcome::Dispatch(translated)) => {
            assert_eq!(
                translated.command,
                Command::PaletteVimInput(PaletteVimKey::Char('k'))
            );
            assert_eq!(translated.repeat_count, 1);
        }
        other => panic!("expected repeated palette k dispatch, got {:?}", other),
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run:
```bash
cd /Users/qc-bright/Project/netherize_editor && cargo test -p netherize_editor repeated_j_in_palette_normal_dispatches_vim_input repeated_k_in_palette_normal_dispatches_vim_input -- --nocapture
```
Expected: both tests FAIL with `expected repeated palette j/k dispatch, got None` — because `PaletteVimInput` is not in the whitelist, so `route_repeated_normalized_input` returns `None` at `handler.rs:346-348`.

- [ ] **Step 3: Add the two whitelist arms**

In `src/core/commands.rs`, find `pub fn supports_press_and_hold_repeat(&self) -> bool {` at line 578. The function is a big `matches!(self, Self::A | Self::B | …)` expression. Find the line `| Self::PaletteMoveCursorToEnd` (around line 615) — or any other `Palette*` arm in the list — and add the two new arms directly after the last `Palette*` arm (before `Self::CompletionNext`):

```rust
                | Self::PaletteVimInput(PaletteVimKey::Char('j'))
                | Self::PaletteVimInput(PaletteVimKey::Char('k'))
```

`PaletteVimKey` is defined in the same file at line 8, so no import is needed — use the bare name `PaletteVimKey::Char('j')`.

The arms should be placed alphabetically/grouped with the other `Palette*` arms for readability, but placement within the `matches!` alternation does not affect behavior.

- [ ] **Step 4: Run tests to verify they pass**

Run:
```bash
cd /Users/qc-bright/Project/netherize_editor && cargo test -p netherize_editor repeated_j_in_palette_normal_dispatches_vim_input repeated_k_in_palette_normal_dispatches_vim_input -- --nocapture
```
Expected: both tests PASS.

- [ ] **Step 5: Verify no regression in existing repeat tests**

Run:
```bash
cd /Users/qc-bright/Project/netherize_editor && cargo test -p netherize_editor repeated_ -- --nocapture
```
Expected: all `repeated_*` tests pass (including `repeated_motion_key_dispatches_while_holding`, `repeated_backspace_dispatches_while_holding`, `repeated_enter_dispatches_newline_while_holding`, `repeated_toggle_command_is_ignored_while_holding`).

- [ ] **Step 6: Commit**

```bash
cd /Users/qc-bright/Project/netherize_editor && git add src/core/commands.rs src/app/input/tests.rs && git commit -m "feat(input): allow j/k key repeat in palette vim normal mode"
```

If `git commit` fails due to hook/author issues, use `git -c user.name=opencode -c user.email=opencode@local commit ...`.

---

## Task 2: Disable caret blink (always visible)

**Files:**
- Modify: `src/app/event_loop/application.rs:1269` (the `tick_caret_blink` function body)

- [ ] **Step 1: Replace the function body**

Find this function in `src/app/event_loop/application.rs` (line 1269):
```rust
    /// Tối ưu 3: Caret Blink — tick timer nhấp nháy, chỉ set caret_blink_dirty.
    /// KHÔNG set editor_needs_layout hay editor_caret_needs_layout.
    /// Nhờ đó toàn bộ text pipeline không bị trigger reshape chỉ vì con trỏ nháy.
    fn tick_caret_blink(&mut self) -> bool {
        let now = Instant::now();
        if now.duration_since(self.last_caret_blink_tick) >= Duration::from_millis(1000) {
            self.last_caret_blink_tick = now;
            self.caret_blink_visible = !self.caret_blink_visible;
            self.caret_blink_dirty = true;
            return true;
        }
        false
    }
```

Replace with:
```rust
    /// Caret blink is disabled — the caret stays solid (always visible).
    ///
    /// Previously this toggled `caret_blink_visible` every 1 s. Now it's a no-op:
    /// every cursor move / mode change / viewport scroll already resets
    /// `caret_blink_visible = true` (see application.rs, viewport.rs, terminal.rs),
    /// so with the toggle gone the caret never blinks off.
    fn tick_caret_blink(&mut self) -> bool {
        false
    }
```

Keep the function signature (`&mut self` -> `bool`) unchanged — it's called from `application.rs:1068` (`if self.tick_caret_blink() { … }`). Returning `false` makes that branch a no-op.

- [ ] **Step 2: Build to check for unused-variable warnings**

Run:
```bash
cd /Users/qc-bright/Project/netherize_editor && cargo build -p netherize_editor 2>&1 | tail -40
```
Expected: success. **If you see `warning: unused variable: now` or similar**, that's because the old body used `Instant::now()` and `Duration` — but we removed those usages. The `Instant` and `Duration` imports at the top of `application.rs` are still used by OTHER functions in the file, so no import cleanup is needed. If a warning does appear, it's about a now-unused local — but since the new body is just `false`, there are no locals, so no warning should appear.

- [ ] **Step 3: Run the full test suite to check for regressions**

Run:
```bash
cd /Users/qc-bright/Project/netherize_editor && cargo test --workspace 2>&1 | tail -20
```
Expected: all tests pass. No test directly asserts blink behavior; the caret pipeline tests (if any) only check that rects are uploaded on layout, which still happens.

- [ ] **Step 4: Commit**

```bash
cd /Users/qc-bright/Project/netherize_editor && git add src/app/event_loop/application.rs && git commit -m "feat(caret): disable blink, caret stays solid always visible"
```

If `git commit` fails due to hook/author issues, use `git -c user.name=opencode -c user.email=opencode@local commit ...`.

---

## Task 3: Final verification

**Files:** none (verification only)

- [ ] **Step 1: Run the full test suite**

Run:
```bash
cd /Users/qc-bright/Project/netherize_editor && cargo test --workspace 2>&1 | tail -30
```
Expected: all tests pass, including:
- `repeated_j_in_palette_normal_dispatches_vim_input` (new)
- `repeated_k_in_palette_normal_dispatches_vim_input` (new)
- `repeated_motion_key_dispatches_while_holding` (regression)
- `repeated_toggle_command_is_ignored_while_holding` (regression — toggles still NOT whitelisted)

- [ ] **Step 2: Run clippy on the touched files**

Run:
```bash
cd /Users/qc-bright/Project/netherize_editor && cargo clippy -p netherize_editor -- -D warnings 2>&1 | grep -E "src/core/commands.rs|src/app/input/tests.rs|src/app/event_loop/application.rs" | head -20
```
Expected: no output (no warnings in our 3 touched files). The codebase has many pre-existing clippy warnings elsewhere; this command filters to only our files.

- [ ] **Step 3: Verify commit history**

Run:
```bash
cd /Users/qc-bright/Project/netherize_editor && git log --oneline -4
```
Expected (newest first):
- `feat(caret): disable blink, caret stays solid always visible` (Task 2)
- `feat(input): allow j/k key repeat in palette vim normal mode` (Task 1)
- `docs(spec): key repeat j/k in palette + solid caret`
- (previous work)

- [ ] **Step 4: Manual visual check (deferred to user)**

Build and launch the editor. Verify:
1. **Key repeat:** Open command palette / file picker / fuzzy picker / live grep. Press Esc to enter Normal mode (if not already). Hold `j` — the result list scrolls down continuously. Hold `k` — scrolls up continuously. Single press still moves one item. `3j` still moves three items (count works; repeat guard blocks OS-repeat from hijacking the count).
2. **Solid caret:** The editor caret never blinks off — it stays solid in all modes (Normal, Insert, Visual, Terminal). Move the cursor with `hjkl` — caret follows and stays visible.

---

## Self-Review Notes

- **Spec coverage:**
  - Whitelist `PaletteVimInput(Char('j' | 'k'))` → Task 1 ✓
  - Disable blink → Task 2 ✓
  - Tests for key repeat → Task 1 ✓
  - Build/test/clippy → Task 3 ✓
- **Placeholder scan:** no TBDs, no "implement later", all code blocks complete. The `PaletteVimKey` bare-name note in Task 1 Step 3 is a verification prompt, not a placeholder.
- **Type consistency:** `PaletteVimKey::Char('j')` matches the enum definition at `commands.rs:8-12`. `Command::PaletteVimInput(PaletteVimKey)` matches usage at `commands_palette.rs:88`. `tick_caret_blink(&mut self) -> bool` signature unchanged.
- **Risk:** LOW. Two localized edits, both additive/no-op. The key-repeat guard at `handler.rs:248-253` protects against chord/count/operator auto-completion. The blink disable leaves all `caret_blink_visible = true` reset sites intact.
