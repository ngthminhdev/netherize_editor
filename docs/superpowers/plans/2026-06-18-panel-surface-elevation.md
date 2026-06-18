# Panel Surface Elevation Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make editor panels read as a cohesive, layered surface system instead of disjointed text on a void with one glowing cyan focus ring.

**Architecture:** Add a third surface-elevation rung (`elevated_bg`) to the theme, derived from `panel_bg` at load time so the 84 existing theme files need no edits. Make the per-region fill elevation-aware (focused region → `elevated_bg`, others → their panel color), and replace the thick cyan focus ring with a quiet 1px accent border plus a `border_color` seam on unfocused panels. The existing uniform 2px dock insets become visible once panels have distinct bodies, so the gap rhythm fixes itself.

**Tech Stack:** Rust, wgpu region pipeline, TOML themes (`config/themes/*.toml`).

## Global Constraints

- **Committing requires the user's explicit authorization.** The commit steps below are part of the plan document; when executing, do NOT run `git commit` until the user explicitly says to. (Project rule overrides the skill's default.)
- Run tests with `cargo test`. Build with `cargo build`.
- Surface differentiation MUST work even when `ui_config.enable_outline == false` (the user runs with borders off; only the focused panel shows a ring today). Elevation via body lightness is the primary signal; borders are secondary.
- No existing `config/themes/*.toml` file may be required to change. New token is optional with a derived fallback.
- Color types: `ThemeColor::from_rgba_u8(r,g,b,a)`, `ThemeColor::as_u8() -> [u8;4]`, `ThemeColor::as_f32() -> [f32;4]`.

---

### Task 1: Add `elevated_bg` theme token with derived fallback

**Files:**
- Modify: `src/config/theme_config/raw.rs` (`RawUi` struct — add `elevated_bg: Option<String>`)
- Modify: `src/config/theme_config/model.rs` (`UiThemeTokens` struct — add `elevated_bg: ThemeColor`; add `derive_elevated_surface` fn)
- Modify: `src/config/theme_config/loader.rs` (`parse_ui` — populate `elevated_bg`)
- Test: `src/config/theme_config/model.rs` (unit tests for `derive_elevated_surface`)

**Interfaces:**
- Produces: `UiThemeTokens.elevated_bg: ThemeColor`; `pub fn derive_elevated_surface(panel_bg: ThemeColor) -> ThemeColor`

- [ ] **Step 1: Write the failing test** — append to the `#[cfg(test)] mod tests` in `src/config/theme_config/model.rs` (create the module if absent):

```rust
#[test]
fn derive_elevated_lightens_dark_panel() {
    // default-dark panel_bg #1c2433
    let panel = ThemeColor::from_rgba_u8(0x1c, 0x24, 0x33, 0xff);
    let elevated = derive_elevated_surface(panel);
    let [pr, pg, pb, _] = panel.as_u8();
    let [er, eg, eb, ea] = elevated.as_u8();
    assert!(er > pr && eg > pg && eb > pb, "dark panel should lighten: {:?} -> {:?}", panel.as_u8(), elevated.as_u8());
    assert_eq!(ea, 0xff, "alpha preserved");
}

#[test]
fn derive_elevated_darkens_light_panel() {
    // a light theme panel, e.g. #e8e8ec
    let panel = ThemeColor::from_rgba_u8(0xe8, 0xe8, 0xec, 0xff);
    let elevated = derive_elevated_surface(panel);
    let [pr, pg, pb, _] = panel.as_u8();
    let [er, eg, eb, _] = elevated.as_u8();
    assert!(er < pr && eg < pg && eb < pb, "light panel should darken");
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --lib derive_elevated`
Expected: FAIL — `derive_elevated_surface` not found.

- [ ] **Step 3: Implement `derive_elevated_surface`** — add to `src/config/theme_config/model.rs` (near `ThemeColor`):

```rust
/// Derive an "elevated" surface from a panel background by shifting it ~10%
/// toward higher contrast with foreground text: lighten dark panels, darken
/// light ones. Works in sRGB u8 space — the shift is subtle enough that
/// gamma error is invisible, and this keeps the math obvious.
pub fn derive_elevated_surface(panel_bg: ThemeColor) -> ThemeColor {
    let [r, g, b, a] = panel_bg.as_u8();
    let lum = 0.2126 * f32::from(r) + 0.7152 * f32::from(g) + 0.0722 * f32::from(b);
    let factor = 0.10_f32;
    let shift = |c: u8| -> u8 {
        let c = f32::from(c);
        let out = if lum < 128.0 {
            c + (255.0 - c) * factor // lighten toward white
        } else {
            c * (1.0 - factor) // darken toward black
        };
        out.round().clamp(0.0, 255.0) as u8
    };
    ThemeColor::from_rgba_u8(shift(r), shift(g), shift(b), a)
}
```

- [ ] **Step 4: Add the struct field** — in `UiThemeTokens` (same file), add after `panel_bg`:

```rust
    pub panel_bg: ThemeColor,
    /// Elevated surface for the focused panel and overlays. Derived from
    /// `panel_bg` when the theme omits `ui.elevated_bg`.
    pub elevated_bg: ThemeColor,
```

- [ ] **Step 5: Add the raw field** — in `RawUi` (`src/config/theme_config/raw.rs`), add after `panel_bg`:

```rust
    pub(in crate::config::theme_config) panel_bg: String,
    pub(in crate::config::theme_config) elevated_bg: Option<String>,
```

- [ ] **Step 6: Populate in `parse_ui`** — in `src/config/theme_config/loader.rs`, inside `parse_ui`, after the `panel_bg:` line add:

```rust
        panel_bg: parse_color("ui", "panel_bg", &raw.panel_bg)?,
        elevated_bg: match raw.elevated_bg.as_deref() {
            Some(hex) => parse_color("ui", "elevated_bg", hex)?,
            None => crate::config::theme_config::model::derive_elevated_surface(
                parse_color("ui", "panel_bg", &raw.panel_bg)?,
            ),
        },
```

(Adjust the import path to however `derive_elevated_surface` / `parse_color` are already referenced in this file; `parse_color` is local to the module.)

- [ ] **Step 7: Run tests to verify they pass**

Run: `cargo test --lib derive_elevated && cargo build`
Expected: PASS, and the crate compiles (every `UiThemeTokens { .. }` literal now includes `elevated_bg`; if any test constructs it manually, the compiler will flag it — fix those by adding `elevated_bg: derive_elevated_surface(panel_bg)` or a literal).

- [ ] **Step 8: Commit** *(only with user authorization — see Global Constraints)*

```bash
git add src/config/theme_config/
git commit -m "feat(theme): add derived elevated_bg surface token"
```

---

### Task 2: Elevation-aware region fill color

**Files:**
- Modify: `src/app/event_loop/helpers.rs:1800` (`region_color`; add `region_surface_color`)
- Test: `src/app/event_loop/helpers.rs` (tests module)

**Interfaces:**
- Consumes: `UiThemeTokens.elevated_bg` (Task 1), existing `region_color(id, theme)`.
- Produces: `pub(super) fn region_surface_color(id: RegionId, theme: &ThemeConfig, is_focused: bool) -> [f32; 4]`

- [ ] **Step 1: Write the failing test** — add to the tests module in `src/app/event_loop/helpers.rs` (import `region_surface_color`, `RegionId`, load a theme via `ThemeConfig::load("default-dark")`):

```rust
#[test]
fn focused_center_uses_elevated_surface() {
    let theme = ThemeConfig::load("default-dark").expect("load theme");
    let focused = region_surface_color(RegionId::Center, &theme, true);
    let unfocused = region_surface_color(RegionId::Center, &theme, false);
    assert_eq!(focused, theme.ui.elevated_bg.as_f32());
    assert_eq!(unfocused, theme.editor.bg.as_f32());
    assert_ne!(focused, unfocused, "focus must change the body shade");
}

#[test]
fn unfocused_sidebar_keeps_panel_color() {
    let theme = ThemeConfig::load("default-dark").expect("load theme");
    assert_eq!(
        region_surface_color(RegionId::LeftSidebar, &theme, false),
        theme.ui.sidebar_bg.as_f32()
    );
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --lib focused_center_uses_elevated_surface`
Expected: FAIL — `region_surface_color` not found.

- [ ] **Step 3: Implement** — add below `region_color` in `src/app/event_loop/helpers.rs`:

```rust
/// Fill color for a region, elevated when focused. The focused panel rises one
/// surface rung (`elevated_bg`) so focus is legible even with borders disabled.
pub(super) fn region_surface_color(
    id: RegionId,
    theme: &ThemeConfig,
    is_focused: bool,
) -> [f32; 4] {
    if is_focused
        && matches!(
            id,
            RegionId::Center
                | RegionId::LeftSidebar
                | RegionId::RightSidebar
                | RegionId::BottomPanel
        )
    {
        return theme.ui.elevated_bg.as_f32();
    }
    region_color(id, theme)
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test --lib region_surface_color`
Expected: PASS.

- [ ] **Step 5: Commit** *(only with user authorization)*

```bash
git add src/app/event_loop/helpers.rs
git commit -m "feat(ui): add elevation-aware region_surface_color"
```

---

### Task 3: Wire the region loop to elevation + quiet focus border

**Files:**
- Modify: `src/app/event_loop/application.rs:9` (`FOCUS_RING_THICKNESS`)
- Modify: `src/app/event_loop/application.rs:1397-1485` (region instance loop)
- Test: `src/app/event_loop/application.rs` (`focus_ring_keeps_outline_and_panel_fill`)

**Interfaces:**
- Consumes: `region_surface_color` (Task 2), `theme.ui.accent`, `theme.ui.elevated_bg`.

- [ ] **Step 1: Thin the focus ring** — change `src/app/event_loop/application.rs:9`:

```rust
const FOCUS_RING_THICKNESS: f32 = 1.0;
```

- [ ] **Step 2: Focused border uses accent, not cyan** — at `application.rs:1410-1415`, change the `else` branch from `self.theme.ui.cyan.as_f32()` to `self.theme.ui.accent.as_f32()`:

```rust
        let mut focused_outline =
            if focus_target == FocusTarget::CenterEditor && center_has_error_diagnostics {
                self.theme.ui.error.as_f32()
            } else {
                self.theme.ui.accent.as_f32()
            };
```

- [ ] **Step 3: Use elevation-aware fills in the loop** — in the closure at `application.rs:1421-1485`:
  - Replace `let rs_panel_bg = self.theme.ui.panel_bg.as_f32();` (line ~1419) usage so the RightSidebar fill is elevation-aware. Inside the `if region.id == RegionId::RightSidebar` arm, compute `let rs_fill = region_surface_color(RegionId::RightSidebar, &self.theme, is_focused);` and pass `rs_fill` to both the `suppress_ring` flat-fill quad and the `focus_ring_instances(..)` inner fill (replace the two `rs_panel_bg` uses).
  - In the `else if suppress_ring` arm (line ~1465) replace `region_color(region.id, &self.theme)` with `region_surface_color(region.id, &self.theme, is_focused)`.
  - In the final `else` arm (line ~1476) replace the `region_color(region.id, &self.theme)` inner-fill argument with `region_surface_color(region.id, &self.theme, is_focused)`.
  - Ensure `region_surface_color` is imported in this file (add to the existing `use super::helpers::{... region_color ...}` import).

Concretely, the RightSidebar arm becomes:

```rust
                if region.id == RegionId::RightSidebar {
                    let rs_fill = region_surface_color(RegionId::RightSidebar, &self.theme, is_focused);
                    if suppress_ring {
                        vec![RegionDrawInstance::new(bounds, rs_fill).with_radius(panel_radius)]
                    } else {
                        let outline_color = if is_focused { focused_outline } else { default_outline };
                        focus_ring_instances(bounds, outline_color, FOCUS_RING_THICKNESS, panel_radius, rs_fill)
                    }
                } else if suppress_ring {
                    vec![RegionDrawInstance::new(
                        bounds,
                        region_surface_color(region.id, &self.theme, is_focused),
                    )
                    .with_radius(panel_radius)]
                } else {
                    let outline_color = if is_focused { focused_outline } else { default_outline };
                    focus_ring_instances(
                        bounds,
                        outline_color,
                        FOCUS_RING_THICKNESS,
                        panel_radius,
                        region_surface_color(region.id, &self.theme, is_focused),
                    )
                }
```

- [ ] **Step 4: Update the focus-ring test** — `focus_ring_keeps_outline_and_panel_fill` (~line 2794) still passes explicit args, so it stays green. Add a thickness assertion to lock the 1px outline:

```rust
    #[test]
    fn focus_ring_keeps_outline_and_panel_fill() {
        let instances = focus_ring_instances(
            [12.0, 24.0, 320.0, 180.0],
            [0.7, 0.3, 1.0, 1.0],
            1.0,
            10.0,
            [0.08, 0.08, 0.1, 1.0],
        );
        assert_eq!(instances.len(), 2, "panel regions render both outline and fill");
        // Inner fill is inset by the 1px thickness on each side.
        assert_eq!(instances[1].rect, [13.0, 25.0, 318.0, 178.0]);
    }
```

- [ ] **Step 5: Build and test**

Run: `cargo build && cargo test --lib focus_ring`
Expected: compiles; focus-ring tests PASS.

- [ ] **Step 6: Commit** *(only with user authorization)*

```bash
git add src/app/event_loop/application.rs
git commit -m "feat(ui): elevation fills + quiet 1px accent focus border"
```

---

### Task 4: Consistent divider seam on unfocused panels

**Files:**
- Modify: `src/app/event_loop/application.rs:1397-1398` (`default_outline`)

**Interfaces:**
- Consumes: `theme.ui.border_color`.

- [ ] **Step 1: Make unfocused panel borders use the divider color** — change `application.rs:1397`:

```rust
        let mut default_outline = self.theme.ui.border_color.as_f32();
        default_outline[3] = default_outline[3].max(0.95);
```

Rationale: focused = `accent` (Task 3), unfocused = `border_color` seam. With `enable_outline` off, unfocused panels show no border and rely on elevation; with it on, they show a consistent 1px seam instead of an accent-colored ring that competed with focus.

- [ ] **Step 2: Build**

Run: `cargo build`
Expected: compiles.

- [ ] **Step 3: Commit** *(only with user authorization)*

```bash
git add src/app/event_loop/application.rs
git commit -m "feat(ui): unfocused panels use border_color divider seam"
```

---

### Task 5: Manual visual verification + dead-code note

**Files:**
- Optional: `src/render/renderer/ui/utils.rs` (`right_sidebar_background_quads`)

- [ ] **Step 1: Run the app and eyeball the result**

Run: launch the editor (per the project's run skill / `cargo run`). With `default-dark`:
  - Confirm Explorer, editor, Terminal, AI Chat each read as a distinct surface body even with `enable_outline = false`.
  - Confirm the focused panel is one shade lighter and (with outline on) wears a thin accent border — no thick cyan glow.
  - Confirm the existing 2px dock insets now read as a uniform gap on all sides.

- [ ] **Step 2: Check a light theme**

Switch to `bearded-light` (theme palette / command palette). Confirm `elevated_bg` darkens (not lightens) so the focused panel still separates and text contrast holds.

- [ ] **Step 3: Decide on dead code** — `right_sidebar_background_quads` (`src/render/renderer/ui/utils.rs:110`) is `#[cfg_attr(not(test), allow(dead_code))]` and never called in production (this plan uses `region_surface_color` + `focus_ring_instances` instead). Either delete it and its tests, or leave the note. Recommended: delete to avoid two divergent panel-surface paths.

- [ ] **Step 4: Commit any cleanup** *(only with user authorization)*

```bash
git add -A
git commit -m "chore(ui): remove unused right_sidebar_background_quads"
```

---

## Self-Review

- **Spec coverage:**
  - 3-rung ladder → Task 1 (`elevated_bg` derived) + Task 2 (focused→elevated, others→panel). `surface_base` = existing `ui.bg` (unchanged, used as clear color elsewhere). ✔
  - Derive, don't hand-author 84 themes → Task 1 Step 6 (optional `ui.elevated_bg`, derived fallback). ✔
  - One shared panel path → Task 3 routes all regions through `region_surface_color` + `focus_ring_instances`. (Deviation from spec: the spec named `right_sidebar_background_quads`, but that function is dead code; the real production path is `region_color`/`focus_ring_instances`. Task 5 retires the dead function.) ✔
  - Dividers → Task 4 (`border_color` seam). ✔
  - Focus = 1px accent, no glow → Task 3 (thickness 1.0, cyan→accent, focused body elevated). ✔
  - Gap rhythm → already uniform at 2.0 (`*_DOCK_OUTLINE_INSET`); becomes visible via elevation, verified in Task 5 Step 1. No code task needed. ✔
  - Light-theme contrast risk → `derive_elevated_surface` branches on luminance; Task 5 Step 2 verifies. ✔
- **Placeholder scan:** none — every code step has concrete code.
- **Type consistency:** `derive_elevated_surface(ThemeColor)->ThemeColor`, `region_surface_color(RegionId,&ThemeConfig,bool)->[f32;4]`, `elevated_bg: ThemeColor` used consistently across tasks. ✔
