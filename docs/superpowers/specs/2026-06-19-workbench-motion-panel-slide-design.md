# Workbench Motion — Panel Slide Animation — Design

**Date:** 2026-06-19
**Status:** Approved
**Goal:** Hyprland-style smooth slide animation when panels/docks toggle, zen
mode changes, and modal overlays open/close — at **90+ FPS or not at all**.

## 1. Goal & non-goals

When the user toggles a dock (left/right/bottom), enters/leaves Zen (maximize),
or opens/closes a modal overlay (Command Palette, Code Graph HUD, popups), the
transition is **animated smoothly** instead of snapping instantly.

Hard requirement: **silky (target 90+ FPS)**. A janky animation is worse than
none — every per-frame cost is budgeted, and there is a global reduce-motion
kill-switch that snaps instantly.

Docks use a **Push (tiling)** model: the panel pushes the editor; the editor
shrinks/grows with it and is never overlapped.

**Non-goals (deferred / YAGNI):** spring physics, background blur, animating
drag-resize (drag stays direct), staggered multi-panel choreography, per-panel
distinct curves.

## 2. Existing infrastructure this builds on

The editor already has a time-driven animation pattern — this feature follows it,
it does **not** invent a new loop:

- `about_to_wait` runs `tick_*` animations (`tick_smooth_scroll_animation`,
  `tick_yank_flash`, `tick_bracket_ripple`, `tick_lsp_loading_animation`), each
  returns `bool` (changed → `request_redraw()`), and schedules the next wake via
  `ControlFlow::WaitUntil(deadline)` at an ~8 ms cadence.
- Rendering uses dirty flags (`*_needs_layout`) and batches **all** region quads
  into a **single** GPU upload per frame (`frame.rs`), with
  `FRAME_TIME_WARN_THRESHOLD = 8 ms`.
- Layout is a **pure function**: `WorkbenchLayoutEngine::compute(size, panels)
  -> WorkbenchLayout`, driven by `panels.{left,right,bottom}.{visible,size_px}`
  and `panels.maximized_region`. This is the clean injection point.
- Editor has **no soft-wrap** (horizontal scroll) — verified. Changing the
  editor's width therefore does **not** reshape glyphs and produces **no
  end-of-animation jump** (final pixels equal the clipped pixels).

## 3. Two animation primitives (separated by responsibility)

### A. Layout Transition — docks + Zen/Maximize

Rather than interpolating each panel's `size_px`, interpolate the **whole
computed layout**: capture `layout_before` (the layout currently on screen) and
`layout_after` (the layout for the new logical panel state), then **lerp each
region's bounds** by an eased `t`. One model covers both dock push and zen.

### B. Overlay Enter/Leave — Command Palette, Code Graph HUD, popups

Modal overlays live on their own layer, so they animate with **opacity fade +
small scale/translate** (not bounds-lerp) — the natural "pop in/out" feel.

## 4. Architecture

New pure module **`src/workbench/motion.rs`** (UI-free, fully unit-tested, no
GPU/IO):

- `EaseCurve` + `ease_out_cubic(t)` and friends; `t` always clamped to `[0,1]`.
- `LayoutTransition { from: WorkbenchLayout, to: WorkbenchLayout, started_at:
  Instant, duration: Duration, curve: EaseCurve }`
  - `sample(now) -> WorkbenchLayout` — lerp every region's `RegionBounds`
    (`x,y,width,height`) from `from` to `to` at eased progress.
  - `is_done(now) -> bool`.
- `OverlayMotion { phase: Enter | Leave, started_at, duration, curve }`
  - `sample(now) -> OverlaySample { alpha: f32, scale: f32 }`.
  - `is_done(now) -> bool`.

`AppShell` (event loop) holds:
- `panel_transition: Option<LayoutTransition>`
- overlay motion state (per active overlay, or one slot for the single active
  modal — see §7).
- `last_committed_layout: WorkbenchLayout` (the authoritative layout, used as
  `from` when a transition starts and as the steady-state render input).

## 5. Data flow (follows the Golden Data Flow)

1. A toggle/zen command (`ToggleLeftDock`, `ToggleRightDock`,
   `ToggleBottomDock`, `ToggleMaximizeFocus`) dispatches as today and mutates the
   **logical** `WorkbenchPanelState` immediately — all state/queries stay
   correct from frame 0.
2. Immediately after the mutation, AppShell:
   - takes `from = current effective layout` (the sampled layout if a transition
     is already running, else `last_committed_layout`),
   - computes `to = layout_engine.compute(size, new_panels)`,
   - if `motion.enabled`: installs `LayoutTransition { from, to, ... }`;
     else: sets `last_committed_layout = to` (instant snap).
3. **`tick_panel_animation()`** in `about_to_wait` (mirrors
   `tick_smooth_scroll_animation`):
   - if a transition is active → `sample(now)` into the **effective layout**, set
     the needed dirty flags, `return true` (→ `request_redraw`), and contribute
     an `now + 8ms` (capped at end time) entry to `next_deadline`.
   - when `is_done` → set `last_committed_layout = to`, clear the transition, do
     one final authoritative layout pass.
4. The renderer consumes the **effective layout** (sampled) during animation and
   `last_committed_layout` otherwise. Overlay draws multiply their color/alpha by
   the `OverlayMotion` sample and apply the scale.

The same `tick`/`next_deadline` wiring drives overlay motions.

## 6. Performance strategy (how 90+ is held)

**Key realization:** `redraw()` already recomputes the layout every frame via the
pure `compute()` and lays out content **gated by viewport** — glyph/cell
instances are generated only for *visible* lines/cells, not for the whole
document. So per-frame relayout cost is bounded by the viewport (≈ tens of
lines), **not** by file size. This makes the simplest correct approach —
re-deriving content from the sampled (animated) bounds each frame — cheap enough
to hold 90+.

v1 strategy (simple + correct):
- Feed an **effective layout** into `redraw()`: the sampled transition layout
  while animating, else the plain `compute()` result. All downstream content
  (sidebar/center/bottom bounds → text, terminal, etc.) already derives from this
  single `layout` value, so everything moves together — true Push, no overlap.
- During an active transition, set the content dirty flags each tick so text/grid
  follow the animated bounds.
- Editor has no soft-wrap → width changes never reshape glyphs and produce no
  end-of-animation jump (final pixels equal the last animated frame).
- Per-frame work: pure layout sample + viewport-bounded content layout + the
  existing **single** batched region-quad upload.

**Perf gate:** keep frame `< 8 ms` (`FRAME_TIME_WARN_THRESHOLD`); validate with
`benches/e2e_perf_runner` and a manual toggle on `benchmarks/inputs/
rust_10k_lines.rs`.

**Deferred fallback (only if the gate fails for some content type):** lay that
content out once at its target bounds and clip+translate during the slide instead
of relaying out each frame. Not implemented in v1 unless the bench requires it.

## 7. Mid-flight reversal & overlap

- Toggling again while a transition runs starts a **new** transition with
  `from = current sampled layout` → seamless reversal, no snap/flash.
- Only one `panel_transition` exists at a time; multiple panel changes within the
  window collapse into one transition toward the newest target.
- Overlays: a modal that is closed while still entering animates Leave from its
  current sampled alpha/scale.

## 8. Config — `[animation]` in `config/ui/default.toml` + `ui_config.rs`

```toml
[animation]
enabled = true            # reduce-motion kill-switch → instant snap (cf. smooth_scroll)
dock_duration_ms = 150    # ease-out for dock push + zen
overlay_duration_ms = 110 # fade/scale for palette / HUD / popups
curve = "ease_out_cubic"
```

`ui_config.rs` parses these with safe fallbacks (mirror the `smooth_scroll_*`
parsing). `enabled = false` makes every transition snap instantly (the kill-switch
short-circuits in §5 step 2).

## 9. Scope

**v1 (shipped):** push animation for the 3 docks, layout transition for
zen/maximize, the `[animation]` config with reduce-motion kill-switch, and
mid-flight reversal. Implemented via a central interception in
`handle_command_with_count` that compares a `panel_layout_signature`
(dock visibility + zen target, excluding `size_px`) before/after each command —
so every dock toggle, zen change, and the test-runner auto-open animate, while
drag-resize stays direct.

**Command Palette enter — "Dot → Line → Panel Reveal" — shipped.** The palette
is the one modal with a fully self-contained render boundary
(`palette_chrome_instances` + `palette_text_pipeline` + `palette_icon_pipeline`),
so the motion is applied as a single post-transform to those three instance sets
right before upload. `OverlayMotion::reveal_sample(now) -> RevealSample`
(`width_factor`, `height_factor`, `content_alpha`, `scrim_alpha`; pure,
unit-tested) drives it. The scrim dims **first and fast** (`scrim_alpha` over
`[0, 0.2]`) so the backdrop is set before anything is drawn — without this the
horizontal stroke is drawn against a still-bright editor and is invisible. A
centered **reveal rect** then sweeps horizontally from a dot into a line
(`width_factor` over `[0.12, 0.66]`, **linear** — no easing, so the stroke reads
as a steady draw instead of an ease-out "pop" that finishes almost instantly),
then unfolds vertically into the full panel (`height_factor` over `[0.66, 0.9]`).
The renderer maps the factors to pixels with a 3px min thickness, clamps corner
radius to ≤ half the short side (so the line reads as a clean pill), and
identifies the frame-border / panel-bg quads by exact rect match (`panel_bounds`
grown 1px / `panel_bounds`) to swap in the reveal rect. Every other quad plus all
glyphs/icons fade in last via `content_alpha` (`[0.85, 1.0]`), so text only
appears once the panel is essentially open. Total timing is
`overlay_duration_ms` (default **280 ms**). The rising edge of
`is_command_palette_visible()` starts the motion; `tick_palette_motion` drives
~120 Hz frames and clears it on completion so the settled frame renders at
identity. Enter-only for v1 (leave-on-close would require deferring
`clear_palette` teardown).

**Still deferred — completion popup / hover floating box / Code Graph HUD
fade.** These live in `update_editor_overlays`'s mixed instance vectors
(interleaved with always-on chrome: selection, diagnostics, indent guides), so a
blanket transform would wrongly fade editor chrome. Isolating their instance
ranges is a separate, larger effort. They remain instant for now.

**Deferred (other):** spring physics, blur, drag-resize animation, multi-panel
stagger, per-panel curves.

## 10. Components / seams touched

- `src/workbench/motion.rs` — **new** pure module (transitions, overlay motion,
  easing).
- `src/workbench/mod.rs` — export `motion`.
- `src/app/event_loop/mod.rs` — `AppShell` fields (`panel_transition`,
  overlay motion, `last_committed_layout`).
- `src/app/event_loop/application.rs` — `tick_panel_animation()` +
  `tick_overlay_motion()`, `about_to_wait` deadline contribution, capture
  `from`/`to` on toggle/zen.
- `src/app/event_loop/commands_settings*.rs` / wherever dock-toggle & maximize
  are handled — install the transition after mutating panel state.
- `src/render/renderer/lifecycle/frame.rs` + `lifecycle.rs` — render from the
  effective layout; apply overlay alpha/scale.
- `src/render/renderer/palette.rs` / overlay draws — multiply by overlay alpha,
  apply scale.
- `src/config/ui_config.rs` + `config/ui/default.toml` — `[animation]` block.

## 11. Testing

- **Unit (pure, in `motion.rs`):** easing curve values & clamping; `sample`
  bounds-lerp at t=0/0.5/1; `is_done` timing; reduce-motion snap; mid-flight
  reversal uses current sampled `from`; overlay alpha/scale ramp.
- **Perf:** run `benches/e2e_perf_runner`; manually confirm frame `< 8 ms` /
  90+ FPS during a left/right/bottom toggle on a large file (e.g.
  `benchmarks/inputs/rust_10k_lines.rs`).
- Follow the existing `overlay_manager`/`layout_engine` test style for any
  layout-coordinate assertions.
