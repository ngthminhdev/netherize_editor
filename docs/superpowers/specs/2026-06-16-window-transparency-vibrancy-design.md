# Window Transparency + macOS Vibrancy (Warp-style) — Design

**Date:** 2026-06-16
**Status:** Draft for review
**Scope:** macOS only. Whole-window uniform transparency with OS blur (vibrancy)
behind all panels. A single opacity slider controls everything. Text/foreground
always stays fully opaque (keeps its color).

## Problem

The current `bg_opacity` work multiplies the alpha of 5 region quads (and the
center terminal quad), but the window surface is opaque, the clear color stays
fully opaque, and dozens of content background layers still draw opaque on top.
Net effect: panels merely tint toward `theme.ui.bg` — the desktop is never
visible. The goal is true Warp-style see-through with a blurred backdrop.

## Goal

Lowering the opacity slider makes every panel background progressively
see-through to a blurred view of the desktop behind the window, while all
glyphs, icons, borders, and highlights keep their normal color/opacity. At
100% the editor looks exactly as it does today (blur fully hidden).

## Five pieces required (and current status)

| # | Requirement | Status before this work |
|---|---|---|
| A | Transparent NSWindow (`winit .with_transparent(true)`) | Missing |
| B | Surface `alpha_mode = PostMultiplied` | Picks `alpha_modes.first()` (usually Opaque) |
| C | Clear color alpha scales with opacity | Always opaque, explicitly excluded |
| D | Every background **fill** layer carries the opacity alpha | Only 5 region quads + center terminal |
| E | Blend state accumulates framebuffer alpha correctly | **Already correct** — `alpha: One / OneMinusSrcAlpha` |

Piece E is already satisfied: because glyphs draw with alpha=1 over a
reduced-alpha background, straight-alpha blending pulls the framebuffer alpha
back to 1 under text — so **text becomes solid automatically** and only the
bare background stays transparent. No per-text handling needed.

## Architecture

### D — How opacity reaches every background (chosen: bake into theme tokens)

In `apply_scaled_runtime_config` (src/app/event_loop/setup.rs), after cloning
`base_theme → self.theme`, multiply the **alpha channel** of the background-fill
tokens by `bg_opacity / 100`:

- `ui.bg`, `ui.sidebar_bg`, `ui.panel_bg`, `ui.terminal_bg`, `ui.overlay_bg`,
  `ui.status_bar_bg`, and `editor.bg`.

Untouched (stay alpha=1): `fg`, `fg_dim`, `fg_ghost`, `accent`, `border_color`,
`selection_bg`, `dirty_indicator`, all semantic colors (`cyan`, `error`, …), and
all syntax colors. These are text/foreground/highlight, not backgrounds.

Because every renderer site reads these via `theme.X.as_f32()`, baking the alpha
once means **~zero call-site changes** for the simple fill sites. This replaces
the current per-site multiplication in `region_color` (helpers.rs) and the
center-terminal quad (application.rs) — both revert to plain `.as_f32()` since
the token already carries the alpha.

**Forced-alpha sites that still need edits.** A small set of sites compute a
blend and force the output alpha to a constant, overriding the baked alpha.
These must scale that constant by `bg_opacity/100`:

- `ui/popups.rs` titlebar: `with_alpha(panel_bg, 0.92)`
- `ui/ai_chat.rs` blends like `blend_rgba(base_bg, panel_bg, 0.72, 1.0)`,
  `blend_rgba(panel_bg, status_bar_bg, 0.5, 1.0)`, code-block bg blends
- Any other `with_alpha(<bg token>, …)` / `blend_*(…, 1.0)` whose base is a
  background token (audit during implementation; grep `with_alpha`, `blend_rgb`,
  `blend_rgba`).

Highlight overlays (current-line, selection, search match, focus ring) keep
alpha=1 intentionally — they are foreground accents and should read as solid
over the translucent background.

### A — Transparent window (macOS)

Add `.with_transparent(true)` to the macOS branch of `apply_platform_window_chrome`
(application.rs). This sets the NSWindow `opaque = false` / clear background.

### macOS vibrancy view (the blur)

Add an `NSVisualEffectView` as the **backmost** subview of the window's content
view, filling it with both autoresizing masks so it tracks resize:

- `material = .underWindowBackground` (closest to Warp; finalize during impl —
  candidates: `.underWindowBackground`, `.fullScreenUI`, `.hudWindow`)
- `blendingMode = .behindWindow`
- `state = .active`

Inserted once, right after the window + surface are created
(`Renderer::new` / surface creation path in lifecycle.rs, or just after window
creation in application.rs). It stays permanently; visibility is governed purely
by how opaque the backgrounds drawn over it are, so no add/remove on slider
change.

**CAMetalLayer must be non-opaque.** wgpu 29 does not expose the layer, so the
blur can be fully occluded by an opaque Metal layer even with a transparent
window. We must set `metalLayer.opaque = false` (and ensure its
`backgroundColor`/clear is transparent). Reaching the layer requires objc.

**New dependencies (macOS only, `[target.'cfg(target_os = "macos")'.dependencies]`):**
- `objc2`
- `objc2-app-kit` (NSVisualEffectView, NSView)
- `objc2-foundation` (geometry)
- `raw-window-handle` is already transitively available via winit 0.30
  (`window.window_handle()` → `RawWindowHandle::AppKit`) to obtain the `NSView`.

This is the **highest-risk part**. Plan a small spike first: prove a transparent
window + NSVisualEffectView + non-opaque Metal layer shows desktop blur, before
wiring the slider. If the Metal layer cannot be made non-opaque under wgpu 29,
fall back to plain alpha (skip vibrancy) and revisit.

### B — Surface alpha mode

In `src/render/surface.rs`, choose alpha mode by preference instead of `.first()`:

```
PostMultiplied (preferred) → PreMultiplied → fallback to capabilities.first()
```

Add a helper that picks the first supported in that priority order. If only
`Opaque` is available, transparency silently degrades to today's behavior
(panels just tint) — acceptable, no crash.

### C — Clear color tracks opacity

`clear_color` is set from `theme.ui.bg` in lifecycle.rs (lines ~110, ~441). Once
`ui.bg`'s alpha is baked by piece D, `theme_color_to_wgpu(theme.ui.bg)` already
yields the right alpha — so the inter-panel gaps (outer_gap / panel_gap) scale
uniformly with everything else. Verify `theme_color_to_wgpu` preserves alpha
(not forced to 1.0); fix if it drops it.

## Settings / UX

- Reuse the existing `SettingItem::BgOpacity` slider (Appearance section).
- Rename label `"Panel Background Opacity"` → `"Window Opacity"`.
- Update description: remove "Does not affect the window clear color"; new text
  e.g. *"Window background opacity (0–100%). Lower values reveal a blurred view
  of the desktop behind all panels. Text stays fully opaque. macOS only."*
- Keep h/l = ±5 and Enter-to-type, clamp 0–100. Default 100.

## Persistence (must fix — currently broken)

`bg_opacity` lives in `UiThemeTokens` (per-theme) and the settings handler writes
`base_theme.ui.bg_opacity` then calls `finalize_settings_change` →
`ui_config.save_user_override()`, which saves **UI config, not the theme**. So
today the value is lost on restart and resets to 100 when switching themes.

**Decision:** treat window opacity as a global, theme-independent user preference.
- Store it in `ui_config` (persisted via the existing `save_user_override`
  path / `ui.toml`), not per-theme.
- On startup and on every theme switch, inject the saved value into
  `base_theme.ui.bg_opacity` / `self.theme.ui.bg_opacity` before
  `apply_scaled_runtime_config`, so it survives restarts and theme changes.
- `UiThemeTokens.bg_opacity` remains the field the renderer reads (the bake in
  piece D reads it); the theme TOML `bg_opacity` becomes an optional default
  only, overridden by the user preference when present.

## Error handling / fallbacks

- Non-macOS build: `with_transparent`/vibrancy code is `#[cfg(target_os = "macos")]`;
  other platforms compile without it and the slider still tints (degraded).
- objc calls wrapped defensively; failure to attach the vibrancy view logs and
  continues with a transparent-but-unblurred window (or opaque if alpha mode
  unsupported). Never panic.
- Surface with only `Opaque` alpha mode → behaves like today.

## Testing

- Unit: a pure helper `apply_bg_opacity(tokens, opacity) ` (or equivalent) that
  scales exactly the six UI bg tokens + editor.bg and leaves fg/accent/semantic
  untouched — assert alphas at 0 / 50 / 100. This is the one piece of pure logic
  worth a test (TDD).
- Unit: alpha-mode selection helper picks PostMultiplied when present, falls back
  correctly.
- Manual (the visual parts can't be unit-tested): build on macOS, lower slider,
  confirm desktop blur appears behind every panel, text stays solid, gaps scale,
  100% looks unchanged, resize keeps the blur full-bleed, restart + theme-switch
  preserve the value.

## Out of scope

- Windows / Linux transparency and blur.
- Per-panel independent opacity.
- Animated/auto opacity (focus-follow, etc.).

## Key risks

1. **Non-opaque CAMetalLayer under wgpu 29** — may need objc layer poking; spike first.
2. Choosing a vibrancy material that matches Warp's look — iterate visually.
3. sRGB target + premultiplied compositing edge cases — verify no color/gamma
   shift on the translucent backdrop during the spike.
