# Neovide-style pixel-smooth editor viewport scrolling — Design

Date: 2026-06-22
Supersedes the timing/config portions of `2026-06-22-smooth-scroll-design.md`.

## Goal

Make same-buffer page / jump / center scrolling in the **normal editor buffer
viewport** slide pixel-by-pixel (Neovide-style) instead of snapping line-by-line,
so navigation feels continuous and spatially trackable. Keep `j`/`k` instant and
keep cross-file / LSP navigation instant. **NetherCanvas is entirely out of
scope** — it owns a separate camera / card / zoom viewport system.

The feature name is deliberately specific — *Neovide-style pixel-smooth editor
viewport scrolling* — because Netherize now has multiple viewport/camera systems
(editor buffer, NetherCanvas camera, canvas cards).

## What already exists (do not rebuild)

The previous iteration already shipped most of the machinery, so this is a small,
surgical change:

- `AppState.target_scroll_y` / `current_scroll_y` (f32, line units). The renderer
  already consumes `current_scroll_y` as a **fractional sub-line pixel offset**
  (`layout_sync::visual_y_for_logical_scroll_with_folds` → `frac * line_height`).
- The caret is rendered against the same `current_scroll_y`, so it stays **glued
  to its text line** and slides with the buffer during a tween — no separate
  cursor animation is needed or wanted.
- `workbench::motion::ease_scroll(start, target, started_at, now, duration, curve)`
  — a pure, time-based, fixed-duration ease-out sampler (frame-rate independent).
- `workbench::motion::clamp_scroll_start(current, target, max)` — Neovide far-jump
  clamp: begins the tween at most `max` lines from `target`.
- `AppShell` tween bookkeeping: `scroll_anim_started_at`, `scroll_anim_start`,
  `scroll_anim_last_target`; sampled at render time in `advance_scroll_anim`;
  ~120 Hz `WaitUntil` frame pacing in `about_to_wait`.

## Why it still feels janky (root cause)

1. The fixed duration is **140 ms** and the curve is **front-loaded ease-out**, so
   ~50 % of the distance is covered in the first ~30 ms → reads as a snap-then-
   settle rather than a glide.
2. `zz` (`CenterCursorLine`) and `gg` (`MoveToFirstLine`) call
   `center_cursor_line`, which **snaps** `current = target` — bypassing the tween.
3. The central snap-vs-animate decision keys off `|delta| < 1.5 lines` only, so an
   explicit `zz`/`gg` with a small delta would wrongly snap.

## Decisions (from review)

- **Timing model: fixed length** (Neovide `scroll_animation_length` = 0.3 s), not a
  distance-proportional `lines_per_sec` model. Reuse `ease_scroll` unchanged; just
  feed it the configured duration.
- **Curve:** `ease_out_cubic` (matches the existing workbench motion feel).
- **Far-jump clamp:** keep clamping so large jumps animate only the final visible
  span. Make the clamp width configurable via `editor_smooth_scroll_far_lines`.
- **Config home:** a new `[motion]` section with Neovide-like naming. The legacy
  `[editor].smooth_scroll_*` keys remain parseable (deprecated, mapped as
  fallbacks) so existing configs don't break.

## Command behavior

| Command | Behavior |
|---|---|
| `Ctrl-D` / `Ctrl-U` (half page) | **animate** |
| `G` / last line | **animate** |
| `gg` / first line | **animate** |
| `zz` / center cursor | **animate** |
| `j` / `k` single-line follow | snap instantly (unchanged) |
| go-to-def / LSP jump | snap instantly (unchanged) |
| cross-file jump | snap instantly (unchanged) |
| NetherCanvas camera / card / zoom | **untouched, out of scope** |

## Timing & far-jump model

```
# at render time, per frame:
duration = editor_smooth_scroll_animation_ms        # fixed, default 300 ms
curve    = motion.ease                              # default ease_out_cubic

# on a target change (retarget):
max   = (viewport_lines + far_lines).max(1)          # far-jump clamp width
start = clamp_scroll_start(current, target, max)     # teleport the far part
current = start ; started_at = now                   # tween start == current pos
```

- **Far-lines semantics (adapted from Neovide).** Neovide's
  `scroll_animation_far_lines` (default 1) animates only `far_lines` lines at the
  *end* of an over-one-screen jump — which at default 1 nearly snaps. To honor the
  acceptance criterion that `gg`/`G` *visibly* animate while still clamping far
  jumps, Netherize clamps the animated span to **one screenful + `far_lines`**
  (default → ~one screenful animates; higher → more of the jump animates; this
  matches "far jump lands quickly, then smooths into place"). Documented as an
  intentional adaptation, per "match the visible UX, not the exact internals."

## Snap-vs-animate decision (pure, in motion.rs)

A new pure helper centralizes the retarget decision so it is unit-testable without
an `AppShell`:

```rust
pub enum ScrollRetarget { Snap, Animate { start: f32 } }

pub fn scroll_far_clamp_lines(viewport_lines: usize, far_lines: u32) -> f32
//   = (viewport_lines as f32 + far_lines as f32).max(1.0)

pub fn plan_scroll_retarget(
    current: f32,
    target: f32,
    smooth_enabled: bool,   // motion.enabled && editor_enabled && animation_ms > 0
    force: bool,            // explicit command bypasses the snap threshold
    snap_threshold: f32,    // SCROLL_SNAP_THRESHOLD_LINES = 1.5
    viewport_lines: usize,
    far_lines: u32,
) -> ScrollRetarget {
    let delta = target - current;
    if !smooth_enabled || (!force && delta.abs() < snap_threshold) {
        return ScrollRetarget::Snap;
    }
    let max = scroll_far_clamp_lines(viewport_lines, far_lines);
    ScrollRetarget::Animate { start: clamp_scroll_start(current, target, max) }
}
```

- `j`/`k` reach the scroll target via `auto_scroll_to_cursor` with `force = false`
  → small delta → `Snap` (stays instant).
- `zz`/`gg`/`G`/`Ctrl-D`/`Ctrl-U` set `force = true` → animate when `distance > 0`
  even if `delta < 1.5`.
- `smooth_enabled = false` (any of the three disables) → always `Snap`.
- **Retarget from current:** `current` passed in is the live animated position, so
  a new command mid-tween recomputes `start`/clamp from where the buffer is *now* —
  no one-frame jump.

## Centering: animated variant without touching go-to-def

- Add `AppState::center_cursor_line_animated(viewport_lines)` — sets
  `target_scroll_y` only; **does not** set `current_scroll_y`.
- Keep the existing `center_cursor_line(viewport_lines)` (snaps both) for all
  LSP / palette / go-to-def / cross-file / safety callers — **unchanged**.
- Route only the editor command sites `Command::CenterCursorLine` and
  `Command::MoveToFirstLine` (in `commands_editor.rs`) to the animated variant.

## Force flag (explicit-command bypass)

- New `AppShell` field `scroll_anim_force: bool` (presentation/motion state only;
  **not** added to `AppState` or any snapshot).
- The editor command arm for `ScrollHalfPageUp | ScrollHalfPageDown |
  CenterCursorLine | MoveToFirstLine | MoveToLastLine` sets `scroll_anim_force =
  true` after moving the target.
- `advance_scroll_anim` reads it **one-shot** at the top
  (`let force = std::mem::take(&mut self.scroll_anim_force);`) so it cannot leak
  into a later unrelated `j`/`k` retarget.

## Config (`[motion]`)

```toml
[motion]
enabled = true                         # master gate for editor smooth scroll
duration_ms = 250                      # general motion default (reserved)
ease = "ease_out_cubic"                # curve for editor smooth scroll
editor_smooth_scroll_enabled = true
editor_smooth_scroll_animation_ms = 300
editor_smooth_scroll_far_lines = 1
```

`MotionConfig` (new struct in `ui_config.rs`):

```rust
pub struct MotionConfig {
    pub enabled: bool,                         // default true
    pub duration_ms: u32,                      // default 250
    pub ease: EaseCurve,                       // default EaseOutCubic
    pub editor_smooth_scroll_enabled: bool,    // default true
    pub editor_smooth_scroll_animation_ms: u32,// default 300
    pub editor_smooth_scroll_far_lines: u32,   // default 1
}
impl MotionConfig {
    pub fn editor_smooth_scroll_active(&self) -> bool {
        self.enabled && self.editor_smooth_scroll_enabled
            && self.editor_smooth_scroll_animation_ms > 0
    }
    pub fn editor_scroll_duration(&self) -> Duration {
        Duration::from_millis(self.editor_smooth_scroll_animation_ms as u64)
    }
}
```

**Disable semantics (all three independently disable; produce `Snap`):**
- `motion.enabled = false`
- `editor_smooth_scroll_enabled = false`
- `editor_smooth_scroll_animation_ms = 0`

**Back-compat mapping** (in `from_raw`, when the `[motion]` key is absent):
- `editor_smooth_scroll_enabled` ← legacy `[editor].smooth_scroll_enabled` else `true`.
- `editor_smooth_scroll_animation_ms` ← legacy `[editor].smooth_scroll_duration_ms` else `300`.
- Legacy `[editor]` smooth-scroll keys stay parseable (deprecated); never panic.

**Invalid values fall back safely:** `ease` uses `EaseCurve::from_str_or_default`
(unknown → ease-out-cubic); numeric fields are `u32` so malformed TOML yields a
graceful `Err` from `load`/`from_str` (never a panic).

## Runtime wiring (`advance_scroll_anim`)

Replace the reads of `ui_config.editor.smooth_scroll_*` with `ui_config.motion.*`:
`smooth = motion.editor_smooth_scroll_active()`, `duration =
motion.editor_scroll_duration()`, `curve = motion.ease`, `far_lines =
motion.editor_smooth_scroll_far_lines`. Use `plan_scroll_retarget(...)` for the
snap-vs-animate branch; pass `curve` to `ease_scroll` (was hardcoded EaseOutCubic).

## State ownership

All new state is presentation/motion state on `AppShell`
(`scroll_anim_force: bool`). No new `AppState` fields, **no snapshot changes**, no
persistence. `current_scroll_y` / `target_scroll_y` already live on `AppState`
(pre-existing) and are not added to.

## Out of scope (explicitly untouched)

NetherCanvas camera pan, card vertical scroll, canvas zoom; markdown preview, help
panel, terminal scrollback; any animated cursor trail; cross-file jump animation;
LSP go-to-def animation.

## Testing

Pure (no AppShell):
- `scroll_far_clamp_lines`: `(vp + far_lines).max(1)`; `vp=0, far=0 → 1`.
- `plan_scroll_retarget`:
  - smooth disabled → `Snap` (covers all three disable causes via `smooth_enabled`).
  - `j`/`k` small delta, `force=false` → `Snap`.
  - explicit `zz` small delta (`< 1.5`), `force=true`, `distance>0` → `Animate`.
  - `Ctrl-D`/`Ctrl-U` large delta, `force=true` → `Animate`.
  - `gg`/`G` far jump → `Animate { start }` clamped to `target ± (vp+far_lines)`.
  - retarget mid-tween: pass a fresh `current` → `start` recomputed from it.
- `MotionConfig::editor_smooth_scroll_active()`: false for each of the three
  disable causes; true otherwise.
- `center_cursor_line_animated` sets `target_scroll_y` but leaves `current_scroll_y`
  unchanged; `center_cursor_line` sets both (snaps).
- Config parse: `[motion]` overrides; empty file → defaults (300 ms, far 1);
  `ease = "garbage"` → ease-out-cubic; legacy `[editor]` keys map as fallbacks.

Shell-level (reuse the existing `AppShell` test harness):
- `Ctrl-D` then `advance_scroll_anim` leaves `scroll_anim_started_at = Some` and
  `current_scroll_y` between start and target (animating, not snapped).
- Editor scroll command + `advance_scroll_anim` does **not** mutate canvas
  camera/card state (canvas isolation).

## Follow-up revisions (2026-06-23)

After live use, three refinements (all tested, no-fold path unchanged):

1. **`j`/`k` edge scroll now glides** — the original "single-line `j`/`k` snaps"
   read as a jarring jump against the otherwise-smooth motion. Lowered
   `SCROLL_SNAP_THRESHOLD_LINES` 1.5 → **0.5**, so a whole-line cursor-follow at
   the viewport edge animates; only sub-line jitter / true no-ops still snap.
2. **Long-line auto-fold threshold raised** — `AUTO_FOLD_LINE_CHAR_THRESHOLD`
   200 → **1000** chars; ordinary long lines render normally, only pathological
   lines fold.
3. **Smooth scroll across folds** — the tween now eases in **visual** line space
   (`AppState::logical_scroll_to_visual` / `visual_scroll_to_logical`) and
   converts back to logical for the renderer. Easing in logical space made
   `current_scroll_y` cross folded (hidden) lines whose zero-height visual span
   produced a non-monotonic on-screen y → the Ctrl-D/U stutter. The conversions
   are identity when nothing is folded, so the common path is byte-for-byte
   unchanged.

## Acceptance criteria

Same-buffer page/jump/center feels pixel-smooth; `j`/`k` instant; `gg`/`G`/`zz`/
`Ctrl-D`/`Ctrl-U` animate; far jumps clamp the animated span; cross-file & LSP
instant; `motion.enabled=false` and `editor_smooth_scroll_enabled=false` and
`...animation_ms=0` each disable; NetherCanvas unaffected; existing tests pass.
