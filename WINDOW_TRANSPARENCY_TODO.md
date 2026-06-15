# TODO — Window Transparency + macOS Vibrancy (Warp-style)

**Status:** Designed & planned, NOT implemented. Pick up later.
**Date saved:** 2026-06-16

## What we want
Lowering the opacity slider makes every panel **background** see-through to a
**blurred view of the desktop** behind the macOS window (like Warp). **Text /
icons / highlights stay fully opaque and keep their color.** macOS only, one
global slider for the whole window.

## Key documents (read these first)
- Design/spec: `docs/superpowers/specs/2026-06-16-window-transparency-vibrancy-design.md`
- Step-by-step plan (9 tasks, TDD, exact files/lines/code): `docs/superpowers/plans/2026-06-16-window-transparency-vibrancy.md`

## Why the current `bg_opacity` code does NOT work yet
It only multiplies alpha on 5 region quads + the center terminal. But:
- The window is opaque (no `winit .with_transparent(true)`).
- Surface `alpha_mode` = `.first()` (usually `Opaque`).
- Clear color is always opaque (explicitly excluded).
- ~40 other background layers still draw opaque on top.
- `bg_opacity` isn't persisted (lost on restart / theme switch).

Net effect today: panels just tint toward `theme.ui.bg`; desktop is never visible.

Good news already in place: the blend state (`alpha: One / OneMinusSrcAlpha`)
correctly accumulates framebuffer alpha, so **text auto-stays solid** once
backgrounds carry reduced alpha. No per-text work needed (piece E done).

## Design decisions (locked)
1. **Platform:** macOS + OS blur (NSVisualEffectView vibrancy).
2. **Scope:** whole window, single slider.
3. **How opacity reaches all backgrounds (piece D):** bake the opacity into the
   alpha of background tokens (`ui.bg`, `sidebar_bg`, `panel_bg`, `terminal_bg`,
   `overlay_bg`, `status_bar_bg`, `editor.bg`) once in
   `apply_scaled_runtime_config`. Foreground/accent/semantic/highlight tokens
   stay alpha=1. → near-zero call-site churn.
4. **Persistence:** store window opacity in `ui_config` (`WindowUiConfig`,
   mirror `scale_factor_override`), theme-independent, re-seed after theme load.

## The 9 tasks (see plan for full detail)
1. **Spike (do FIRST, highest risk):** transparent window + `NSVisualEffectView`
   + force `CAMetalLayer.opaque=false` under wgpu 29 + `PostMultiplied` alpha
   mode. Add macOS deps (`objc2`, `objc2-app-kit`, `objc2-foundation`,
   `raw-window-handle`). **If the metal layer can't be made non-opaque, stop &
   reconsider (fall back to plain alpha, no blur).**
2. `ThemeColor::scaled_alpha` (pure, TDD).
3. `apply_bg_opacity` baker (pure, TDD).
4. Wire baker into `apply_scaled_runtime_config`; revert per-quad multiply in
   `region_color` + center terminal.
5. Clear color tracks opacity (ensure renderer receives the BAKED theme — both
   `AppShell.theme` and `Renderer.theme` must be baked; keep `base_theme` raw).
6. Scale forced-alpha bg sites (`with_alpha(<bg>, CONST)` in statusbar.rs,
   editor/settings.rs ×4, ai_chat.rs ×2) by `bg_opacity_factor`.
7. Persist `bg_opacity` as global UI preference + seed on startup/theme switch.
8. Settings: relabel "Window Opacity" + new description.
9. Final verification (cargo test, clippy, manual macOS acceptance checklist).

## Open risk
Task 1 is the gate. Forcing a non-opaque CAMetalLayer through wgpu 29's
abstraction is unverified — prove it with the spike before building the rest.

## Notes
- Commits are the human's job — plan stages files and pauses; never auto-commit.
- Crate name for `cargo test -p <name>`: see `Cargo.toml [package].name`.
- Chosen execution mode when resuming: Inline, starting at Task 1 (spike).
