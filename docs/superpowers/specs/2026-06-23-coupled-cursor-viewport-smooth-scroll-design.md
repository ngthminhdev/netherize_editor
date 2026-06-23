# Coupled Cursor + Viewport Smooth Scroll (v2) — Design

> **Supersedes** the cursor model in `SMOOTH_SCROLL.md` and the v1 design
> `docs/superpowers/specs/2026-06-22-neovide-pixel-smooth-editor-scroll-design.md`.
> `SMOOTH_SCROLL.md` says *"cursor position updates immediately; only the viewport
> animates."* That is the behavior the user identified as **wrong**: the caret pins
> to its final logical line while the scroll lags, so it teleports down-screen then
> the viewport chases it. This v2 couples the caret to the scroll clock instead.

**Status:** Approved (design dialogue 2026-06-23). Awaiting spec review → plan.

---

## 1. Problem

The existing smooth scroll feels wrong. Five root causes, each traced to code:

| Symptom (user words) | Root cause |
|---|---|
| "cursor nhảy 1 phát từ trên xuống dưới, sau đó viewport mới scroll theo" | The caret is glued to its **instantly-jumped logical line** while `current_scroll_y` lags. At tween frame 0 the caret is drawn far down-screen (`caret_logical − old_scroll`), then rises as scroll catches up. No animation of the caret's *travel*. |
| "đè j/k ra mép → UI giật ngược rồi mới scroll" | `auto_scroll_to_cursor` (`src/app/app_state/editor.rs:1116`) jumps `target_scroll_y` by several lines at once (`cursor + margin + 1 − viewport`) and fights the in-flight tween. |
| "j/k chỉ cần change từng line cơ bản" | A single j/k that crosses the margin kicks off the full ~300 ms viewport tween, so one keypress reads as a long laggy glide. |
| "fold scope mà cursor đi ngang → UI giật giật lên xuống" | `auto_scroll_to_cursor` computes the target in **logical** lines but compares it against the **visual** `viewport_lines`. Where a fold compresses height the two disagree, so the viewport oscillates. |
| (smoothness ceiling) | Every animation frame rebuilds styled spans for the **whole file** — `app_state.text_string()` + `syntax_spans_to_styled` + `spans_fingerprint` at `src/app/event_loop/application.rs:2340` — even though glyph *reshaping* is already cached. Wasted per-frame O(file) work. |

## 2. Goal & non-goals

**Goal:** Vertical navigation feels continuous and calm. The caret never teleports
relative to the moving text; the viewport slides smoothly; input stays zero-latency
(logical state always updates immediately). Folded regions scroll without jitter.

**Non-goals:**
- No change to Vim movement *semantics* — commands still compute the same logical
  cursor/scroll. Animation is a visual layer on top.
- No change to cross-file / go-to-definition / search-LSP jumps: those **snap**
  (instant), unchanged.
- NetherCanvas is untouched.
- `j`/`k` do **not** grow a cursor-trail effect; they move one line.

## 3. The unified animation model

**One clock drives two eased values, both in VISUAL-line space (fold-safe):**

- `scroll_anim` → `current_scroll_y` (exists today; already eased in visual space
  via `logical_scroll_to_visual` / `visual_scroll_to_logical`).
- **NEW** `caret_anim` → the caret's visual-line position. The renderer draws the
  caret at `caret_anim`, **not** at the snapped logical line.

### The one rule

> **When the viewport animates, `caret_anim` eases on the same clock from the old
> caret visual-line to the new one. When the viewport does not move, the caret is
> instant.**

Because both are eased on one clock in the same (visual) space, the on-screen caret
row is `caret_anim − scroll_anim`. When caret and scroll move by the same delta this
stays constant (calm); when they differ the caret glides to its new row. Either way
it never teleports.

### Behaviors that fall out of the one rule

| Command(s) | Cursor Δ | Viewport Δ | Result |
|---|---|---|---|
| `j`/`k`/`h`/`l`, no margin hit | ±1 | 0 | No tween triggered → caret instant, no scroll animation. |
| `j`/`k` at the scroll margin | ±1 | ±1 visual line | Both ease 1 line on one short clock → `caret_screen` constant (screen-locked). No jolt, no chase. |
| `Ctrl-D` / `Ctrl-U` | ±half page | ±half page | Caret + scroll ease together; caret stays ~centered, text flows. |
| `gg` / `G` | large | large (clamped) | Caret + scroll ease the last N visual lines; the rest is teleported (far-clamp). |
| plain `zz` | 0 | recenter | `caret_anim` delta is 0 → no-op; only scroll glides, caret slides with its text line. |
| `Gzz` / `ggzz` (batch: both change) | large | large | Both changed in one batch → both animate together. |

### Motion classification

Each editor command, after mutating logical state, reports whether it produced an
**animatable viewport move**. The animation tick (render-time) then:

1. Detects target changes (`scroll_anim`, `caret_anim`) since last frame.
2. Decides **snap vs animate** via the existing `plan_scroll_retarget` policy
   (snap when smooth disabled, or when the delta is sub-threshold and not forced).
3. On animate: far-clamp the start (`clamp_scroll_start`) and ease both values in
   visual space from their current positions toward their targets, sharing
   `started_at` so they stay phase-locked.
4. Retargeting: repeated input recomputes the start from the **current** eased
   position (never a stale origin), so fast repeats don't block or snap back.

## 4. Timing, far-clamp, retarget

- **Distance-scaled, short, fixed durations** (not a lines/sec model), configured in
  `[motion]`, `0` disables animation entirely:
  - 1–3 lines (j/k edge follow): ~70–90 ms
  - half-page (Ctrl-D/U): ~110–130 ms
  - recenter (zz): ~120–140 ms
  - The duration is derived from the **animated** (post-clamp) visual distance, so a
    far jump that clamps to N lines uses the short-distance duration.
- **Far-jump clamp:** animate only the last N visual lines
  (`scroll_far_clamp_lines` = viewport + `editor_smooth_scroll_far_lines`); the
  logical target still jumps immediately.
- **Ease curve:** `EaseOutCubic` (existing), configurable.

## 5. Bundled bug fixes (apply under every model)

1. **`auto_scroll_to_cursor` rewritten in visual-line space.** Compute the cursor's
   *visual* line and compare against the *visual* viewport height; the edge follow
   moves the target by exactly the overflow (1 visual line for a single j/k). Fixes
   both the fold-scope jitter and the multi-line backward jolt.
2. **Styled-span caching across scroll-only frames.** Only rebuild `text_string()` +
   `syntax_spans_to_styled` + diagnostics when the revision / spans / viewport width
   actually change (same trigger set the glyph reshape cache already uses at
   `viewport.rs:358`). Scroll-only frames reuse the cached styled spans.
3. **Caret rendered from `caret_anim`**, not the post-jump logical line (the
   decoupling fix that removes the teleport-then-chase).

## 6. Configuration (`[motion]`)

Extend the existing `MotionConfig` (`src/config/ui_config.rs`). Keep current keys;
add per-distance duration knobs. Legacy `editor.smooth_scroll_*` keys remain
parseable. `editor_smooth_scroll_animation_ms = 0` (or `enabled = false`) disables.

```toml
[motion]
enabled = true
ease = "ease_out_cubic"
editor_smooth_scroll_enabled = true
editor_smooth_scroll_far_lines = 1
# Distance-scaled durations (ms). 0 disables the editor scroll animation.
editor_scroll_step_ms = 80      # 1–3 line edge follow
editor_scroll_halfpage_ms = 120 # Ctrl-D / Ctrl-U
editor_scroll_center_ms = 130   # zz / gg / G recenter
```

(Exact key names finalized in the plan; `editor_smooth_scroll_animation_ms` stays as
a back-compat fallback when the per-distance keys are absent.)

## 7. Components / files

- `src/workbench/motion.rs` — caret ease alongside scroll ease (reuse `ease_scroll`);
  `distance → duration` helper; keep `plan_scroll_retarget`, `clamp_scroll_start`,
  `scroll_far_clamp_lines`. Pure, unit-tested.
- `src/config/ui_config.rs` — per-distance duration fields on `MotionConfig` +
  `RawMotion` parse + back-compat. Unit-tested.
- `src/app/app_state/` — `caret_anim` current/target (visual-line); rewrite
  `auto_scroll_to_cursor` in visual space; helpers reuse existing
  `logical_scroll_to_visual` / `visual_scroll_to_logical`.
- `src/app/event_loop/application.rs` — advance caret anim alongside scroll in one
  tick sharing `now`; cache styled spans across scroll-only frames; render caret at
  the animated visual position.
- `src/app/event_loop/commands_editor.rs` — tag commands that produce an animatable
  viewport move (force-animate flag for Ctrl-D/U/gg/G/zz; edge-follow path for j/k).

## 8. Testing strategy (TDD throughout)

- **Pure (`motion.rs`):** distance→duration mapping; caret+scroll sampled on one
  clock stay phase-locked; `caret_screen = caret_anim − scroll_anim` is constant
  for an equal-delta follow; far-clamp limits animated distance; retarget recomputes
  from current.
- **`app_state`:** `auto_scroll_to_cursor` in visual space — a single edge-crossing
  j moves the target by exactly 1 visual line (no over-jump); crossing a folded
  region yields a monotonic visual target (no oscillation); round-trip
  visual↔logical identity without folds.
- **Shell (`commands_tests.rs`, `AppShell::new_for_tests`):**
  - `Ctrl-D` animates the caret through intermediate lines (`caret_anim` strictly
    between old and new mid-tween) coupled to scroll.
  - `j` with no margin hit: caret instant, no scroll tween.
  - `j` at the margin: viewport eases 1 line **and** caret screen-row stays constant
    mid-tween (no jolt).
  - plain `zz`: scroll glides, caret buffer line unchanged.
  - far jump (`G`) clamps then snaps; cross-file/go-to-def snaps.
  - canvas card state untouched by editor scroll.

## 9. Acceptance criteria

- j/k inside the viewport: caret moves one line instantly, nothing animates.
- j/k at the edge: viewport slides exactly one visual line; caret never jolts or
  chases — its screen row is stable through the 1-line glide.
- Ctrl-D/U: logical state instant; caret + viewport glide together, caret calm.
- gg/G/zz: animate the final clamped distance; never animate across hundreds of
  lines; `Gzz`/`ggzz` animate caret+viewport together.
- Crossing a folded scope produces no up/down jitter.
- Cross-file / go-to-def / search-LSP snap (no animation).
- `duration = 0` (or smooth disabled) → everything snaps; input latency unaffected.
- No per-frame whole-file styled-span rebuild during a scroll animation.
- NetherCanvas behavior unchanged.
- Full test suite green.

---

## Implementation status (2026-06-23)

Implemented in the working tree (uncommitted — commits are the human's). Full suite:
**1077 passed, 1 ignored, 0 failed**; clean build.

**Done (Tasks 1–6):**
- `motion.rs`: `ease_fraction` (shared clock) + `scroll_duration_for_distance`.
- `MotionConfig`: `editor_scroll_{step,halfpage,center}_ms` (80/120/130) + parse +
  legacy fallback + `scroll_duration_for`. `default.toml` updated.
- `auto_scroll_to_cursor` rewritten in **visual-line space** — fixes premature
  scrolling when a fold sits above the cursor (the fold-crossing jitter; proven by
  `auto_scroll_does_not_scroll_when_fold_above_keeps_cursor_visible`, red→green).
- `AppState::caret_scroll_lag` + `cursor_visual_line()`.
- `advance_scroll_anim`: one shared eased fraction drives scroll **and** the caret.
  Caret state is `caret_visual_current` (displayed caret visual line, maintained
  each frame like `current_scroll_y`); on a tween (re)start
  `caret_anim_start = clamp_scroll_start(caret_visual_current, cursor_visual, far_max)`.
  This couples Ctrl-D/j-edge, keeps zz glued (caret Δ 0), clamps far jumps, and
  retargets smoothly. The renderer offsets the caret (and block overlay / virtual
  cursors) by `caret_scroll_lag × line_height`; text is not shifted.

**Done (Task 7 — perf):** the whole-buffer `text_string()` clone is cached across
scroll-only frames, keyed on `(revision, byte length, active file)` via
`editor_text_cache_key`. This removes the per-frame full-rope→String allocation
during a tween. Deliberately *not* caching the styled spans: highlights are
viewport-scoped (refreshed on scroll, async, no cheap change-counter), so the span
build is already cheap AND must rebuild each frame to avoid stale colors. The key
covers every content change — edits bump the revision; buffer switches and the
canvas edit-session swap change the active file (`canvas_edit` swaps it) and/or the
byte length. Verified by `editor_text_cache_key_{distinguishes_content_and_is_stable,
changes_when_text_edited}`.

**Manual verification still needed (GPU feel):** Ctrl-D/U caret calm (no teleport),
j/k at the viewport edge (no backward jolt, caret screen-locked), zz glide, gg/G
clamp, and cursor crossing a fold scope (no up/down jitter).
