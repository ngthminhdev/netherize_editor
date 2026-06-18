# Panel Surface Elevation — Design

**Date:** 2026-06-18
**Status:** Awaiting review
**Goal:** Make the editor's window panels read as a cohesive, layered surface
system instead of disjointed text floating in a black void with one glowing
cyan focus ring.

## Problem

Today the UI has only **two visual states**:

1. The "void" background (`ui.bg`).
2. One panel wearing a thick, saturated cyan focus ring (`ui.cyan`).

There is nothing in between. Every region — Explorer, editor, Terminal, AI Chat
— is painted the **same** color and has **no body, no divider, no border**, so
the panels do not read as surfaces. They read as text on a void, separated only
by empty gaps and one glowing box.

Evidence from `config/themes/default-dark.toml`:

| Token | Value | Note |
|---|---|---|
| `ui.bg` | `#181f2c` | base / clear color / gap |
| `ui.sidebar_bg` | `#1c2433` | same as panel |
| `ui.panel_bg` | `#1c2433` | — |
| `ui.terminal_bg` | `#1c2433` | same as panel |
| `editor.bg` | `#1c2433` | **identical to every panel** |
| `ui.border_color` | `#11161f` | darker-than-bg seam |
| `ui.accent` | `#8196b5` | muted blue-grey (good for focus) |
| `ui.cyan` | `#22ECDB` | the loud focus ring |

The base→panel delta is ~3% lightness — imperceptible. So the surface tokens
*exist* but are effectively unused as an elevation system.

The drawing mechanism for a layered panel **already exists** but is only wired
to the right sidebar: `right_sidebar_background_quads()` in
`src/render/renderer/ui/utils.rs:110` (outline border → inner panel fill →
optional inner input fill). The focus ring is drawn separately by
`focus_ring_instances()` in `src/app/event_loop/application.rs:2733`.

## Design

A three-rung **surface elevation ladder**, one shared panel-drawing path for
every region, thin 1px dividers, and a quiet 1px accent focus border.

### 1. Surface elevation ladder (3 rungs)

Introduce a clear, perceptible ladder. Steps are ~6–8% relative lightness so the
eye separates layers without stripes.

| Rung | Role | default-dark value |
|---|---|---|
| `surface_base` | window clear color, gaps | `#181f2c` (unchanged — maps to existing `ui.bg`) |
| `surface_panel` | **unfocused** panels: Explorer, Terminal, AI Chat, inactive editor | `#1d2535` |
| `surface_elevated` | **focused** panel + popups/palette/overlays | `#26314a` |

These map onto the **existing** theme keys where possible. `surface_base` ←
`ui.bg`; `surface_panel` ← `ui.panel_bg`. `surface_elevated` is **new**.

### 2. Derive, don't hand-author 84 themes

There are 84 theme files. Hand-tuning a third rung in each is wasteful and
error-prone. Instead:

- `surface_elevated` is **derived** from `panel_bg` by a fixed lightening factor
  (lighten toward white by a small fraction in linear space, e.g. +8% L),
  computed once at theme-load time.
- Add an **optional** `ui.elevated_bg` key. If a theme specifies it, use it;
  otherwise use the derived value. No existing theme files need to change.
- Optionally widen the base→panel delta for dark themes at load time if the
  measured delta is below a perceptibility threshold (deferred — see Non-goals).

### 3. One shared panel-surface path

Generalize `right_sidebar_background_quads()` into a region-agnostic
`panel_surface_quads()` (same 3-step structure: outline → fill → optional inner
box). Route **every** region through it:

- Explorer / Outline sidebar → `surface_panel`
- Terminal panel → `surface_panel`
- AI Chat / Test Runner sidebar → `surface_panel` (already does)
- Editor → `surface_elevated` when focused, `surface_panel` when not

Each region gets a consistent body, corner radius, and border in one place.

### 4. Dividers

Draw thin **1px** dividers in `ui.border_color` between adjacent regions
(sidebar│editor, editor│terminal, editor│right-sidebar) rather than relying on
empty gap. This gives structure even where two panels are close.

### 5. Focus = quiet 1px accent border (replaces the cyan ring)

Per decision: **replace** the thick cyan ring.

- Focused panel border becomes **1px** in `ui.accent` (`#8196b5`), not the
  3px+ saturated `ui.cyan`.
- Focused panel body uses `surface_elevated`; unfocused uses `surface_panel`.
- Focus therefore reads through **two** quiet channels (body lightness + thin
  accent border) instead of one loud one. No glow.
- `focus_ring_instances()` shrinks to a 1px outline; the thickness constant and
  the color source (cyan → accent) change. Its existing 2-quad outline+fill
  structure is preserved, so its test stays valid (assert color = accent).

### 6. Consistent gap rhythm

Apply the **same** outer padding on all four sides of every panel, including the
panels that currently bleed to the screen edge. One spacing constant, used
everywhere.

## Components touched

| Unit | Change |
|---|---|
| `config/themes/*` loader (`theme_config`) | add optional `ui.elevated_bg`; derive `surface_elevated` from `panel_bg` when absent |
| `src/render/renderer/ui/utils.rs` | rename/generalize `right_sidebar_background_quads` → `panel_surface_quads`; keep tests, add elevation-tier test |
| region rendering (Explorer, Terminal, editor chrome) | route each through `panel_surface_quads` with the correct rung |
| `src/app/event_loop/application.rs::focus_ring_instances` | 1px, accent color, body = elevated |
| layout/padding constants | single shared gap constant on all sides |

## Testing

- Unit: `panel_surface_quads` produces border+fill (+optional inner) with
  correct rung colors and reduced inner radius (extend existing tests in
  `utils.rs`).
- Unit: theme loader derives `surface_elevated` lighter than `panel_bg`, and
  honors `ui.elevated_bg` override when present.
- Unit: `focus_ring_instances` emits a 1px outline in accent + elevated fill
  (update existing `focus_ring_keeps_outline_and_panel_fill`).
- Visual: manual before/after on `default-dark` and one light theme
  (`bearded-light`) to confirm the ladder is perceptible but not stripey, and
  that focus reads clearly without the cyan glow.

## Non-goals (YAGNI)

- Per-theme hand-tuned elevated colors (derive instead).
- Auto-widening base→panel delta for low-contrast themes (possible later; not
  needed to fix the reported problem).
- Animations / transitions on focus change.
- Reworking the topbar tab strip styling (separate concern).

## Open risk

`surface_elevated` derived by lightening works for dark themes; for light themes
lightening reduces contrast. The loader should lighten *or* darken toward the
opposite of the theme's luminance (move away from `panel_bg` toward higher
contrast with text). Validate on `bearded-light`.
