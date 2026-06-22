# Smooth Scroll (editor + canvas card) — Design

Date: 2026-06-22

## Goal

Animate viewport scrolling so it slides instead of snapping, at 90+ FPS like the
other workbench animations. Applies to the main editor (`Ctrl-D`/`Ctrl-U`, big
jumps) and the focused canvas card's content. Maximally smooth is the bar.

## Why the previous attempt failed

The infrastructure already exists but was gutted:

- `AppState.target_scroll_y` / `current_scroll_y` (f32, line units) drive the
  renderer, which **already renders at a fractional pixel offset**
  (`viewport.rs` → `visual_y_for_logical_scroll_with_folds(current_scroll_y)`).
- `tick_smooth_scroll_animation` (`application.rs`) is a **kill-switch**: it snaps
  `current = target` every frame.
- Several commands also snap `current = target` inline.
- Config `editor.smooth_scroll_lerp_rate = 18.0` exists but is unused.

Two root causes of "not smooth":
1. **No frame-pacing deadline.** Every other animation pushes a
   `ControlFlow::WaitUntil(now + 8ms)` (~120 Hz) in `about_to_wait`; smooth
   scroll never did, so the loop only woke on unrelated deadlines → stutter.
2. Lerp was frame-count based, not time based → uneven steps on uneven ticks.

## Scope

- **Animate**: half-page `Ctrl-D`/`Ctrl-U`; same-file jumps that move the cursor
  far (`gg`/`G`/`n`/`N`/search/`:N`) via `auto_scroll_to_cursor`/`jump_to_line`.
- **Snap (not animated)**: single-line `j`/`k` cursor follow (small delta);
  `center_cursor_line` / go-to-definition (often cross-file — animating a stale
  offset across a file change looks wrong); buffer switches and file open.
- **Canvas card**: vertical fractional scroll for the focused edit card, mirroring
  the horizontal smooth scroll (`h_scroll_px`) the card renderer already has.
- Out of scope: camera pan (Shift+hjkl), markdown preview, help, terminal.

## Timing model (chosen)

Fixed-duration ease-out, reusing `EaseCurve::EaseOutCubic` from `workbench::motion`:

```
t      = elapsed / duration        // 0..1, time-based → frame-rate independent
current = start + (target - start) * ease_out_cubic(t)
```

- `duration` default 140 ms, configurable.
- **Re-target seamlessly**: a new scroll command starts a fresh tween from the
  present `current` (`start = current`, `started_at = now`). Holding `Ctrl-D`
  chains smoothly with no hitch.

## Far-jump clamp (Neovide far-lines)

For very large jumps, clamp the start so it never crawls:

```
max   = viewport_lines                       // one screenful
start = target + (current - target).clamp(-max, +max)   // teleport the far part
```

→ always finishes in ~`duration`, animating only the last ~screenful.

## Architecture (low-risk: all anim state on AppShell)

No new fields on `AppState` / snapshots. The tween bookkeeping lives on `AppShell`
next to `last_scroll_animation_tick`:

- `scroll_anim_started_at: Option<Instant>`
- `scroll_anim_start: f32`
- `scroll_anim_last_target: f32`

`tick_smooth_scroll_animation(&mut self) -> bool` (replaces the kill-switch):

1. `target = app_state.target_scroll_y`, `current = app_state.current_scroll_y`,
   `viewport_lines = self.editor_viewport_lines()`.
2. If `target != scroll_anim_last_target` → a command changed the target:
   - `delta = target - current`.
   - If `!smooth_scroll` or `|delta| < SNAP_THRESHOLD` (≈ 1.5 lines): snap
     (`current = target`), clear anim.
   - Else: far-clamp `start`, teleport `current = start`, set `started_at = now`.
   - `scroll_anim_last_target = target`.
3. If animating: advance `current` via ease; on completion `current = target`,
   `started_at = None`.
4. Return `true` while `current` is moving (→ `request_redraw`).

Pure, unit-tested helper in `workbench::motion`:

```
pub fn ease_scroll(start, target, started_at, now, duration, curve) -> (f32, bool)
```

Commands: remove the inline `current = target` snaps in `scroll_half_page_up` /
`scroll_half_page_down` so the tick can animate them. Keep the snap in
`center_cursor_line`. Ensure file-open snaps `current = target`.

## Frame pacing

In `about_to_wait`, when `current != target` (scroll animating), push
`WaitUntil(now + 8ms)` — same ~120 Hz cadence the other animations use. Satisfies
90+ FPS on 90/120/144 Hz displays.

## Canvas card

The focused edit card renders a window of whole lines at `body_top + i*line_height`
and already does fractional **horizontal** scroll (`h_scroll_px`, clipped to both
edges). Add the symmetric **vertical** fractional offset:

- Track a per-card animated `card_scroll_px` (on AppShell, keyed to the active
  edit session) toward the window's natural line position.
- Draw rows at `body_top + i*line_height - v_scroll_px`, with one row of overscan,
  clipped to `[body_top, body_bottom]`.
- Same ease + 8ms pacing. When no card is focused, this path is inert.

The card window is selected by integer-line `follow_window_start`; the pixel
offset animates the transition between successive window positions.

## Config (`[editor]`)

- `smooth_scroll: bool` (default `true`) — replaces the kill-switch.
- `smooth_scroll_duration_ms: u32` (default `140`).
- `smooth_scroll_lerp_rate` kept parseable (ignored) so old configs don't break.

## Testing

- `ease_scroll`: endpoints (t=0 → start; t≥1 → target, done); monotonic ease-out;
  `duration = 0` → instant.
- Far-jump clamp geometry: start within `max` of target for huge deltas.
- Snap-threshold: tiny delta snaps; large delta animates.
- `smooth_scroll = false` → always snaps.
- Re-target resets `start`/`started_at`.
- Frame pacing: smoke (tick returns `true` while moving) — wall-clock not asserted.
</content>
</invoke>
