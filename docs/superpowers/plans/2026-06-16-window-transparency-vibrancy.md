# Window Transparency + macOS Vibrancy Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.
>
> **PROJECT RULE — commits are the human's job.** Each "Commit" step means: stage the listed files with `git add`, then PAUSE and ask the human to commit. NEVER run `git commit` autonomously.

**Goal:** Lowering the Window Opacity slider makes every panel background see-through to a blurred view of the desktop behind the macOS window (Warp-style), while all text/icons/highlights stay fully opaque and keep their color.

**Architecture:** Bake the opacity into the alpha channel of the six UI background tokens + `editor.bg` once per theme application, so every `theme.X.as_f32()` site is translucent for free; glyphs keep alpha=1 and straight-alpha blending makes text solid automatically. The macOS window is made transparent, an `NSVisualEffectView` is inserted behind the content for the blur, the CAMetalLayer is forced non-opaque, and the surface uses `PostMultiplied` alpha. Window opacity is persisted as a global UI preference, independent of theme.

**Tech Stack:** Rust, winit 0.30, wgpu 29, objc2 / objc2-app-kit (macOS only), tree-sitter-free pure logic where possible.

---

## File Structure

- `src/config/theme_config/model.rs` — add `ThemeColor::scaled_alpha`; the bg-opacity baker helper + its unit tests.
- `src/app/event_loop/setup.rs` — call the baker inside `apply_scaled_runtime_config`; seed `bg_opacity` from `ui_config` at startup.
- `src/app/event_loop/helpers.rs` — revert `region_color` to plain (no per-quad multiply).
- `src/app/event_loop/application.rs` — revert center-terminal quad; add `.with_transparent(true)` (macOS).
- `src/render/renderer/helpers.rs` — `bg_opacity_factor(theme)` helper.
- `src/render/renderer/ui/statusbar.rs`, `editor/settings.rs`, `ui/ai_chat.rs` — scale forced-alpha bg sites.
- `src/render/surface.rs` — `pick_alpha_mode` helper + use it.
- `src/render/macos_vibrancy.rs` (new) — attach NSVisualEffectView + force non-opaque metal layer.
- `src/config/ui_config.rs` — `bg_opacity: Option<u8>` in `WindowUiConfig` (raw, load, save, default).
- `src/app/event_loop/commands_settings_helpers.rs` — persist opacity to `ui_config` on adjust/commit.
- `src/render/renderer/editor/settings.rs` — relabel slider + new description.
- `Cargo.toml` — macOS-only deps.

---

## Task 1: De-risk spike — prove the blur is reachable under wgpu 29 (manual)

> This is the highest-risk part (forcing a non-opaque CAMetalLayer through wgpu's abstraction). Do it FIRST. If it can't be made to work, stop and report — the rest of the plan assumes it does.

**Files:**
- Modify: `Cargo.toml`
- Modify: `src/app/event_loop/application.rs:15-23` (`apply_platform_window_chrome`)
- Modify: `src/render/surface.rs:48-51`
- Create: `src/render/macos_vibrancy.rs`
- Modify: `src/render/renderer/lifecycle.rs:53-54` (after surface creation)

- [ ] **Step 1: Add macOS deps**

In `Cargo.toml`, after the `winit`/`wgpu` lines, add:

```toml
[target.'cfg(target_os = "macos")'.dependencies]
objc2 = "0.6"
objc2-app-kit = { version = "0.3", features = ["NSView", "NSVisualEffectView", "NSResponder", "NSWindow"] }
objc2-foundation = "0.3"
raw-window-handle = "0.6"
```

(Confirm exact compatible versions during impl with `cargo update -p objc2`; pin whatever resolves against winit 0.30's `raw-window-handle 0.6`.)

- [ ] **Step 2: Make the window transparent (macOS)**

`src/app/event_loop/application.rs`, in the `#[cfg(target_os = "macos")]` `apply_platform_window_chrome`:

```rust
    attrs
        .with_titlebar_transparent(true)
        .with_title_hidden(true)
        .with_fullsize_content_view(true)
        .with_transparent(true)
```

- [ ] **Step 3: Prefer a translucent alpha mode**

`src/render/surface.rs`, replace the `alpha_mode` selection (lines ~48-51):

```rust
        let alpha_mode = [
            wgpu::CompositeAlphaMode::PostMultiplied,
            wgpu::CompositeAlphaMode::PreMultiplied,
        ]
        .into_iter()
        .find(|m| capabilities.alpha_modes.contains(m))
        .or_else(|| capabilities.alpha_modes.first().copied())
        .ok_or_else(|| "surface has no alpha mode".to_string())?;
```

- [ ] **Step 4: Write the vibrancy module**

Create `src/render/macos_vibrancy.rs`:

```rust
//! macOS-only: insert an NSVisualEffectView behind the window content so the
//! desktop shows through translucent backgrounds as a blur, and force the
//! CAMetalLayer non-opaque so wgpu's surface composites over it.
#![cfg(target_os = "macos")]

use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2_app_kit::{
    NSAutoresizingMaskOptions, NSView, NSVisualEffectBlendingMode, NSVisualEffectMaterial,
    NSVisualEffectState, NSVisualEffectView,
};
use raw_window_handle::{HasWindowHandle, RawWindowHandle};

/// Attach the blur view + make the metal layer non-opaque. Best-effort:
/// logs and returns on any failure so the app keeps running (opaque-ish).
pub fn install(window: &impl HasWindowHandle) {
    let handle = match window.window_handle() {
        Ok(h) => h.as_raw(),
        Err(err) => {
            eprintln!("[vibrancy] no window handle: {err}");
            return;
        }
    };
    let RawWindowHandle::AppKit(appkit) = handle else {
        eprintln!("[vibrancy] not an AppKit window");
        return;
    };

    // SAFETY: the pointer is a live NSView owned by winit for the window's lifetime;
    // we only add a subview and toggle the layer's opacity on the main thread.
    unsafe {
        let content_view: &NSView = &*(appkit.ns_view.as_ptr() as *const NSView);

        // 1) Force the backing CAMetalLayer non-opaque.
        if let Some(layer) = content_view.layer() {
            let layer_obj: &AnyObject = &*(Retained::as_ptr(&layer) as *const AnyObject);
            let _: () = objc2::msg_send![layer_obj, setOpaque: false];
        }

        // 2) Insert the blur view behind everything.
        let effect = NSVisualEffectView::new(objc2_app_kit::NSApp(/* mtm */).into());
        effect.setMaterial(NSVisualEffectMaterial::UnderWindowBackground);
        effect.setBlendingMode(NSVisualEffectBlendingMode::BehindWindow);
        effect.setState(NSVisualEffectState::Active);
        effect.setFrame(content_view.bounds());
        effect.setAutoresizingMask(
            NSAutoresizingMaskOptions::ViewWidthSizable
                | NSAutoresizingMaskOptions::ViewHeightSizable,
        );
        content_view.addSubview_positioned_relativeTo(
            &effect,
            objc2_app_kit::NSWindowOrderingMode::NSWindowBelow,
            None,
        );
    }
}
```

> NOTE: the exact objc2-app-kit constructor (`new`/`initWithFrame`, MainThreadMarker plumbing) and `msg_send!` shapes will need adjusting to the resolved crate version — this is the spike's job. Iterate until it compiles and runs.

Register the module: in `src/render/mod.rs` (or wherever `surface`/`renderer` are declared) add:

```rust
#[cfg(target_os = "macos")]
pub mod macos_vibrancy;
```

- [ ] **Step 5: Call it after surface creation + temporarily force clear alpha 0**

`src/render/renderer/lifecycle.rs`, right after `create_surface(window)` succeeds (~line 54):

```rust
        #[cfg(target_os = "macos")]
        crate::render::macos_vibrancy::install(window);
```

Temporarily (spike only) set the clear color alpha to 0 where `clear_color` is built (lifecycle.rs ~110): hand-edit `clear_color.a = 0.0;` just to see the blur. Revert in Task 6.

- [ ] **Step 6: Build & run on macOS, verify**

Run:
```bash
cargo run
```
Expected: the desktop is visibly blurred behind the editor window background. If yes, the approach works — proceed. If the metal layer cannot be made non-opaque (blur stays hidden), STOP and report; consider falling back to plain alpha (skip vibrancy).

- [ ] **Step 7: Commit (stage, ask human)**

```bash
git add Cargo.toml Cargo.lock src/render/macos_vibrancy.rs src/render/mod.rs \
        src/render/surface.rs src/app/event_loop/application.rs src/render/renderer/lifecycle.rs
```
Then ask the human to commit (suggested message: `feat: macOS transparent window + vibrancy spike`).

---

## Task 2: `ThemeColor::scaled_alpha` (pure, TDD)

**Files:**
- Modify: `src/config/theme_config/model.rs:35-87` (`impl ThemeColor`)
- Test: same file, `#[cfg(test)]` module at end.

- [ ] **Step 1: Write the failing test**

Add to `src/config/theme_config/model.rs`:

```rust
#[cfg(test)]
mod scaled_alpha_tests {
    use super::ThemeColor;

    #[test]
    fn scales_only_alpha() {
        let c = ThemeColor::from_rgba_u8(10, 20, 30, 200);
        let half = c.scaled_alpha(0.5);
        assert_eq!(half.as_u8(), [10, 20, 30, 100]);
    }

    #[test]
    fn factor_one_is_identity_and_clamps() {
        let c = ThemeColor::from_rgba_u8(255, 0, 0, 255);
        assert_eq!(c.scaled_alpha(1.0).as_u8(), [255, 0, 0, 255]);
        assert_eq!(c.scaled_alpha(0.0).as_u8(), [255, 0, 0, 0]);
        assert_eq!(c.scaled_alpha(5.0).as_u8(), [255, 0, 0, 255]); // clamped
    }
}
```

- [ ] **Step 2: Run test, verify it fails**

Run: `cargo test -p netherize_editor scaled_alpha_tests`
(Or the crate's actual name — check `Cargo.toml [package] name`.)
Expected: FAIL — `no method named scaled_alpha`.

- [ ] **Step 3: Implement**

In `impl ThemeColor`, after `as_f32` (line ~86):

```rust
    /// Return a copy with the alpha channel multiplied by `factor` (clamped 0..=1).
    /// RGB is unchanged. Used to bake window opacity into background tokens.
    pub fn scaled_alpha(self, factor: f32) -> Self {
        let f = factor.clamp(0.0, 1.0);
        let a = (f32::from(self.rgba_u8[3]) * f).round() as u8;
        let [r, g, b, _] = self.rgba_u8;
        Self::from_rgba_u8(r, g, b, a)
    }
```

- [ ] **Step 4: Run test, verify pass**

Run: `cargo test -p netherize_editor scaled_alpha_tests`
Expected: PASS (2 tests).

- [ ] **Step 5: Commit (stage, ask human)**

```bash
git add src/config/theme_config/model.rs
```
Ask human to commit (suggested: `feat: add ThemeColor::scaled_alpha`).

---

## Task 3: Background-opacity baker (pure, TDD)

**Files:**
- Modify: `src/config/theme_config/model.rs` (add free fn `apply_bg_opacity` near `UiThemeTokens`)
- Test: same file.

- [ ] **Step 1: Write the failing test**

```rust
#[cfg(test)]
mod bg_opacity_tests {
    use super::*;

    #[test]
    fn scales_bg_tokens_only() {
        let mut theme = crate::config::theme_config::builtin::builtin_dark();
        // capture originals
        let fg = theme.ui.fg.as_u8();
        let accent = theme.ui.accent.as_u8();
        let sel = theme.ui.selection_bg.as_u8();
        let panel_a = theme.ui.panel_bg.as_u8()[3];

        apply_bg_opacity(&mut theme, 50);

        // bg tokens: alpha halved
        assert_eq!(theme.ui.panel_bg.as_u8()[3], (panel_a as f32 * 0.5).round() as u8);
        assert!(theme.ui.bg.as_u8()[3] < 255);
        assert!(theme.editor.bg.as_u8()[3] < 255);
        // foreground / accent / selection untouched
        assert_eq!(theme.ui.fg.as_u8(), fg);
        assert_eq!(theme.ui.accent.as_u8(), accent);
        assert_eq!(theme.ui.selection_bg.as_u8(), sel);
    }

    #[test]
    fn opacity_100_is_identity() {
        let mut theme = crate::config::theme_config::builtin::builtin_dark();
        let before = theme.ui.panel_bg.as_u8();
        apply_bg_opacity(&mut theme, 100);
        assert_eq!(theme.ui.panel_bg.as_u8(), before);
    }
}
```

(Confirm `builtin_dark` is `pub` / reachable; it lives in `builtin.rs`. If private, use `ThemeConfig::default()` or whatever public constructor exists.)

- [ ] **Step 2: Run, verify fail**

Run: `cargo test -p netherize_editor bg_opacity_tests`
Expected: FAIL — `apply_bg_opacity` not found.

- [ ] **Step 3: Implement**

Add near `UiThemeTokens` in `model.rs`:

```rust
/// Bake window opacity into the alpha of background-fill tokens. Foreground,
/// accent, semantic, highlight, and syntax colors are left fully opaque so text
/// stays solid. `opacity` is 0–100; 100 is a no-op.
pub fn apply_bg_opacity(theme: &mut ThemeConfig, opacity: u8) {
    let factor = f32::from(opacity.min(100)) / 100.0;
    if (factor - 1.0).abs() < f32::EPSILON {
        return;
    }
    let ui = &mut theme.ui;
    ui.bg = ui.bg.scaled_alpha(factor);
    ui.sidebar_bg = ui.sidebar_bg.scaled_alpha(factor);
    ui.panel_bg = ui.panel_bg.scaled_alpha(factor);
    ui.terminal_bg = ui.terminal_bg.scaled_alpha(factor);
    ui.overlay_bg = ui.overlay_bg.scaled_alpha(factor);
    ui.status_bar_bg = ui.status_bar_bg.scaled_alpha(factor);
    theme.editor.bg = theme.editor.bg.scaled_alpha(factor);
}
```

(Confirm `ThemeConfig` and its `editor.bg` field path; `editor.bg` is a `ThemeColor` per `region_color`. Import/visibility: make `apply_bg_opacity` `pub`.)

- [ ] **Step 4: Run, verify pass**

Run: `cargo test -p netherize_editor bg_opacity_tests`
Expected: PASS.

- [ ] **Step 5: Commit (stage, ask human)**

```bash
git add src/config/theme_config/model.rs
```
Ask human to commit (suggested: `feat: add apply_bg_opacity theme baker`).

---

## Task 4: Wire the baker; revert per-quad multiply

**Files:**
- Modify: `src/app/event_loop/setup.rs:573-576`
- Modify: `src/app/event_loop/helpers.rs:1666-1679` (`region_color`)
- Modify: `src/app/event_loop/application.rs:1452-1458` (center terminal quad)

- [ ] **Step 1: Bake into the live theme**

`setup.rs`, in `apply_scaled_runtime_config`, right after `self.theme = scaled_theme.clone();` (line 576):

```rust
        self.theme = scaled_theme.clone();
        crate::config::theme_config::model::apply_bg_opacity(
            &mut self.theme,
            self.theme.ui.bg_opacity,
        );
```

(Use the actual module path that exports `apply_bg_opacity`. `scaled_theme` itself is left opaque — `self.base_theme`/`scaled_theme` keep raw alpha so re-baking is always from a clean base.)

- [ ] **Step 2: Revert `region_color` to plain**

`helpers.rs`, replace the whole body of `region_color` (the token alpha is now pre-baked, so no multiply):

```rust
pub(super) fn region_color(id: RegionId, theme: &ThemeConfig) -> [f32; 4] {
    match id {
        RegionId::TopBar => theme.ui.panel_bg.as_f32(),
        RegionId::LeftSidebar => theme.ui.sidebar_bg.as_f32(),
        RegionId::Center => theme.editor.bg.as_f32(),
        RegionId::RightSidebar => theme.ui.sidebar_bg.as_f32(),
        RegionId::BottomPanel => theme.ui.terminal_bg.as_f32(),
        RegionId::StatusBar => theme.ui.status_bar_bg.as_f32(),
        _ => theme.ui.border_color.as_f32(),
    }
}
```

- [ ] **Step 3: Revert center-terminal quad**

`application.rs` ~1452-1458, replace with the original plain version:

```rust
                region_instances.push(
                    RegionDrawInstance::new(center_bounds, self.theme.ui.terminal_bg.as_f32())
                        .with_radius(self.ui_config.border_radius_px),
                );
```

- [ ] **Step 4: Build**

Run: `cargo build`
Expected: compiles, no warnings about unused `opacity`.

- [ ] **Step 5: Commit (stage, ask human)**

```bash
git add src/app/event_loop/setup.rs src/app/event_loop/helpers.rs src/app/event_loop/application.rs
```
Ask human to commit (suggested: `refactor: bake bg opacity into theme tokens`).

---

## Task 5: Clear color tracks opacity (revert spike hack properly)

**Files:**
- Modify: `src/render/renderer/lifecycle.rs:110, 441`

- [ ] **Step 1: Confirm `theme_color_to_wgpu` preserves alpha**

`theme_color_to_wgpu(color)` → `color.as_linear().to_wgpu()`; `as_linear` keeps `a` straight (model.rs:81). So once `theme.ui.bg` is baked, the clear color alpha already scales. Remove the spike's hardcoded `clear_color.a = 0.0;` from Task 1 Step 5.

- [ ] **Step 2: Verify both clear-color assignments use the baked token**

Both `lifecycle.rs:110` and `:441` do `theme_color_to_wgpu(theme.ui.bg)`. Confirm the `theme` passed to `apply_theme` here is the baked `self.theme` (it is — `apply_scaled_runtime_config` passes `scaled_theme`... CHECK: `apply_theme(scaled_theme)` at setup.rs:608 passes the UN-baked `scaled_theme`, not `self.theme`). **Fix:** pass the baked theme to the renderer. Change setup.rs:608 to bake first:

```rust
        if let Some(renderer) = self.renderer.as_mut() {
            renderer.set_ui_scale(self.runtime_scale);
            let mut baked = scaled_theme.clone();
            crate::config::theme_config::model::apply_bg_opacity(&mut baked, baked.ui.bg_opacity);
            renderer.apply_theme(baked);
            renderer.apply_ui_config(&scaled_ui);
        }
```

And drop the separate bake of `self.theme` from Task 4 Step 1 if `self.theme` is not read by the renderer directly — KEEP both baked and consistent. (Simplest: bake `self.theme` in Task 4, and pass `self.theme.clone()` to `renderer.apply_theme` here instead of `scaled_theme`. Verify which the renderer's region/clear code reads. Renderer keeps its OWN `self.theme` copy via `apply_theme`, so the renderer MUST receive the baked theme.)

> Implementation note: there are two theme copies — `AppShell.theme` (read by `application.rs` region pushes) and `Renderer.theme` (read by `lifecycle/frame.rs` clear color + ui/* draws). BOTH must be baked. The cleanest is to bake `scaled_theme` ONCE before both assignments:
>
> ```rust
>         let mut baked_theme = scaled_theme.clone();
>         apply_bg_opacity(&mut baked_theme, baked_theme.ui.bg_opacity);
>         self.theme = baked_theme.clone();
>         // ...font_family override on self.theme as before...
>         // ...
>         renderer.apply_theme(baked_theme);
> ```
>
> Restructure Task 4 Step 1 + this step into that single bake to avoid double-baking. Keep `self.base_theme` and `scaled_theme` raw.

- [ ] **Step 3: Build & run on macOS**

Run: `cargo run`
Expected: at opacity 100 the window looks fully opaque (no blur); lower it via Settings and the blur appears through every panel AND the inter-panel gaps, uniformly.

- [ ] **Step 4: Commit (stage, ask human)**

```bash
git add src/app/event_loop/setup.rs src/render/renderer/lifecycle.rs
```
Ask human to commit (suggested: `feat: scale window clear color with opacity`).

---

## Task 6: Scale the forced-alpha background sites

**Files:**
- Modify: `src/render/renderer/helpers.rs` (add `bg_opacity_factor`)
- Modify: `src/render/renderer/ui/statusbar.rs:106`
- Modify: `src/render/renderer/editor/settings.rs:624, 625, 1072, 1144`
- Modify: `src/render/renderer/ui/ai_chat.rs:596, 615`

> These sites call `with_alpha(<bg token>, CONST)`, which REPLACES alpha with a constant and so discards the baked alpha. Multiply each constant by the opacity factor.

- [ ] **Step 1: Add the factor helper**

In `src/render/renderer/helpers.rs`, next to `theme_color_to_wgpu`:

```rust
/// Window opacity as a 0.0–1.0 multiplier, for sites that force a constant alpha
/// on a background token and would otherwise ignore the baked translucency.
pub(super) fn bg_opacity_factor(theme: &ThemeConfig) -> f32 {
    f32::from(theme.ui.bg_opacity.min(100)) / 100.0
}
```

- [ ] **Step 2: Apply at each site**

`statusbar.rs:106`:
```rust
        let status_bg = with_alpha(self.theme.ui.status_bar_bg.as_f32(), 0.98 * bg_opacity_factor(&self.theme));
```
`editor/settings.rs:624-625`:
```rust
        let status_bg = with_alpha(self.theme.ui.status_bar_bg.as_f32(), 0.96 * bg_opacity_factor(&self.theme));
        let titlebar_bg = with_alpha(self.theme.ui.panel_bg.as_f32(), 0.92 * bg_opacity_factor(&self.theme));
```
`editor/settings.rs:1072`:
```rust
                                with_alpha(panel_bg, 0.72 * bg_opacity_factor(&self.theme))
```
`editor/settings.rs:1144`:
```rust
                        with_alpha(panel_bg, 0.55 * bg_opacity_factor(&self.theme)),
```
`ai_chat.rs:596`:
```rust
            with_alpha(panel_bg, 0.28 * bg_opacity_factor(&self.theme)),
```
`ai_chat.rs:615`:
```rust
            RegionDrawInstance::new(input_bounds, with_alpha(editor_bg, 0.90 * bg_opacity_factor(&self.theme))).with_radius(10.0),
```

Add `use` for `bg_opacity_factor` in each file (mirror how `with_alpha` is imported there).

- [ ] **Step 3: Build & run**

Run: `cargo run`
Expected: the AI chat input, settings panel titlebar/status, and statusbar all become translucent in step with the slider (no opaque islands remaining at low opacity).

- [ ] **Step 4: Commit (stage, ask human)**

```bash
git add src/render/renderer/helpers.rs src/render/renderer/ui/statusbar.rs src/render/renderer/editor/settings.rs src/render/renderer/ui/ai_chat.rs
```
Ask human to commit (suggested: `fix: scale forced-alpha bg layers with window opacity`).

---

## Task 7: Persist window opacity as a global UI preference

> Today the value writes to `base_theme.ui.bg_opacity` then saves `ui_config` (which has no such field), so it is lost on restart and resets on theme switch. Mirror the `scale_factor_override` pattern in `WindowUiConfig`.

**Files:**
- Modify: `src/config/ui_config.rs` (`WindowUiConfig` struct ~53, Default ~197, raw struct, load ~307, save/apply ~632)
- Modify: `src/app/event_loop/setup.rs` (seed at startup)
- Modify: `src/app/event_loop/commands_settings_helpers.rs:313-323, 716-736` (persist on change)

- [ ] **Step 1: Add the field everywhere `scale_factor_override` appears**

In `src/config/ui_config.rs`:
- `WindowUiConfig` struct (~line 61, next to `scale_factor_override: Option<f32>`):
  ```rust
      /// Window background opacity 0–100 (global, theme-independent). None = use theme default.
      pub bg_opacity: Option<u8>,
  ```
- `Default`/constructor (~line 197): add `bg_opacity: None,`
- The raw/deserialized window struct (the `Raw*Window` near line 798/898): add `pub bg_opacity: Option<u8>,`
- Load (~line 307, after the `scale_factor_override` parse block): 
  ```rust
                  bg_opacity: raw.window.bg_opacity.map(|v| v.min(100)),
  ```
- The serialize-back / `save_user_override` mirror (~line 937, `scale_factor_override: value.window.scale_factor_override,`): add `bg_opacity: value.window.bg_opacity,`
- The apply/merge block (~line 632): 
  ```rust
          if let Some(op) = raw.window.bg_opacity {
              self.window.bg_opacity = Some(op.min(100));
          }
  ```

Follow the EXACT shape of the surrounding `scale_factor_override` code in each spot.

- [ ] **Step 2: Seed the theme from the preference at startup**

In `setup.rs`, where the initial `base_theme` is established (near where the theme is first loaded / before the first `apply_scaled_runtime_config`), add:

```rust
        if let Some(op) = self.ui_config.window.bg_opacity {
            self.base_theme.ui.bg_opacity = op.min(100);
        }
```

(Place it so it also re-applies after a theme switch — find the theme-switch handler that reloads `base_theme` and seed there too, or factor a small `seed_bg_opacity_from_prefs()` helper and call it after every `base_theme` (re)load.)

- [ ] **Step 3: Persist on adjust + commit**

In `commands_settings_helpers.rs`, the `BgOpacity` h/l handler (~313-323) and the typed-commit handler (~716-736): after setting `self.base_theme.ui.bg_opacity = next/value;`, also persist:

```rust
                self.ui_config.window.bg_opacity = Some(next); // or `value`
```

`finalize_settings_change` already calls `ui_config.save_user_override()`, so the typed-commit path persists via that; for the h/l path it also ends in `finalize_settings_change()` — verify both reach a save. If the commit path (716) does NOT call `finalize_settings_change`, add `let _ = self.ui_config.save_user_override();` there.

- [ ] **Step 4: Manual test**

Run: `cargo run` → set opacity to 70 → quit → `cargo run` again.
Expected: opens at 70. Switch theme → still 70.

- [ ] **Step 5: Commit (stage, ask human)**

```bash
git add src/config/ui_config.rs src/app/event_loop/setup.rs src/app/event_loop/commands_settings_helpers.rs
```
Ask human to commit (suggested: `feat: persist window opacity as global UI preference`).

---

## Task 8: Settings label + description

**Files:**
- Modify: `src/render/renderer/editor/settings.rs` (description ~150), `src/app/app_state/settings.rs:138` (display name)

- [ ] **Step 1: Rename the display label**

`src/app/app_state/settings.rs`, the `BgOpacity` arm of the name match (~line 138):
```rust
            Self::BgOpacity { .. } => "Window Opacity",
```

- [ ] **Step 2: Update the description**

`src/render/renderer/editor/settings.rs`, the `BgOpacity` description arm (~150):
```rust
            Self::BgOpacity { .. } => {
                "Window background opacity (0–100%). Lower values reveal a blurred view of the desktop behind every panel; text stays fully opaque. Use h/l for ±5 or Enter to type a value. macOS only."
            }
```

- [ ] **Step 3: Build**

Run: `cargo build`
Expected: compiles.

- [ ] **Step 4: Commit (stage, ask human)**

```bash
git add src/app/app_state/settings.rs src/render/renderer/editor/settings.rs
```
Ask human to commit (suggested: `feat: relabel window opacity setting`).

---

## Task 9: Final verification

- [ ] **Step 1: Full test + lint**

Run:
```bash
cargo test
cargo clippy --all-targets -- -D warnings
```
Expected: all tests pass; no clippy errors.

- [ ] **Step 2: Manual acceptance on macOS**

Run: `cargo run`. Verify each:
- [ ] At 100% the editor looks identical to before (no blur leak, gaps opaque).
- [ ] Lowering opacity reveals desktop blur behind every panel + gaps, uniformly.
- [ ] Text, icons, borders, selection, current-line, caret stay solid and readable.
- [ ] Window resize keeps the blur full-bleed (no unblurred band at edges).
- [ ] Value survives restart and theme switch.
- [ ] Non-macOS: `cargo build` on a non-mac target (or `cargo check --target` if available) still compiles (vibrancy code is `#[cfg]`-gated).

- [ ] **Step 3: Update project bookkeeping**

Per `.claude/rules/openwolf.md`: update `.wolf/anatomy.md` (new `macos_vibrancy.rs`) and append to `.wolf/memory.md`. If any bug was fixed during impl, log to `.wolf/buglog.json`.

- [ ] **Step 4: Commit (stage, ask human)**

```bash
git add -A
```
Ask human to commit (suggested: `chore: bookkeeping for window transparency feature`).

---

## Notes for the implementer

- The crate package name for `cargo test -p <name>` is in `Cargo.toml [package].name` — substitute it in every test command.
- The objc2 API in Task 1 Step 4 is the only place expecting iteration; everything else is ordinary Rust. Budget time for the spike.
- Keep `self.base_theme` and `scaled_theme` RAW (un-baked) at all times; only the renderer-facing `self.theme` / `Renderer.theme` carry baked alpha. Re-baking always starts from raw, so adjusting the slider repeatedly never compounds.
