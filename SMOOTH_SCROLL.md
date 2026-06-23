## Feature: Pixel Smooth Scroll for Editor Viewport

### Context

Netherize Editor is a native Rust, GPU-rendered, keyboard-first editor with Vim-style navigation. The editor already has a logical navigation pipeline where key input is translated into commands, commands mutate editor state, and the renderer draws the latest state.

This feature adds **pixel smooth scrolling** to the editor viewport.

The key design principle is:

```txt
Logical state updates immediately.
Visual viewport catches up smoothly.
```

In other words, smooth scroll must not slow down navigation. Cursor position, viewport `top_line`, selections, and editor state should be updated instantly. Only the rendered viewport position should animate for a short time.

---

### Goal

Implement a smooth scrolling layer for the editor viewport so that vertical navigation feels continuous instead of jumping line-by-line or teleporting after large motions.

This should apply to motions that change the viewport, such as:

```txt
j
k
Ctrl-d
Ctrl-u
zz
PageDown
PageUp
Goto/search jumps
Mouse wheel, if supported
```

The feature should make the editor feel fast, spatial, and easier to visually track while preserving zero-latency input.

---

### Non-goals

This feature must not reimplement Vim movement semantics.

Commands such as `j`, `k`, `Ctrl-d`, `Ctrl-u`, and `zz` should continue to be handled by the existing command/navigation system.

Smooth scroll is only a visual animation layer on top of already-computed editor state.

Do not put scroll animation logic inside the key input handler.

---

### Core Concept

The implementation should separate the viewport into two layers:

```txt
Logical viewport:
  The real editor state.
  This contains the actual top line, cursor line, and scroll position.
  It is updated immediately after every navigation command.

Visual viewport:
  The temporary animated render state.
  This contains a pixel offset used only by the renderer.
  It smoothly moves toward the logical viewport.
```

Example:

```txt
old_top_line = 10
new_top_line = 25
delta_lines = 15
line_height_px = 24
delta_px = 15 * 24 = 360px
```

The editor state should immediately become:

```txt
top_line = 25
```

But the renderer should temporarily draw the new logical viewport with an offset:

```txt
visual_scroll_offset_px = -360px
```

Then, over a short duration, animate:

```txt
visual_scroll_offset_px → 0px
```

This creates the illusion that the viewport slides smoothly from the old position to the new one.

---

### When Smooth Scroll Should Trigger

Smooth scroll should only trigger when the viewport changes.

For example, pressing `j` does not always require smooth scroll.

No viewport change:

```txt
Before:
top_line = 10
cursor_line = 20

After pressing j:
top_line = 10
cursor_line = 21
```

In this case, there is no smooth scroll. The cursor may animate separately, but the viewport does not move.

Viewport changed:

```txt
Before:
top_line = 10
cursor_line = 35

After pressing j:
top_line = 11
cursor_line = 36
```

In this case, smooth scroll should animate one line.

For `Ctrl-d`:

```txt
Before:
top_line = 10
cursor_line = 20

After:
top_line = 25
cursor_line = 35
```

Smooth scroll should animate a larger downward movement.

For `Ctrl-u`:

```txt
Before:
top_line = 25
cursor_line = 35

After:
top_line = 10
cursor_line = 20
```

Smooth scroll should animate upward.

For `zz`:

```txt
Before:
cursor_line = 120
top_line = 110

After:
cursor_line = 120
top_line = 95
```

Even if the cursor does not move, smooth scroll should still run because the viewport was recentered.

---

### Internal Event: Scroll Retarget

After a navigation command mutates the editor state, the system should compare the old viewport position and the new viewport position.

If they are different, emit an internal scroll retarget event.

Suggested data model:

```rust
pub struct ScrollRetarget {
    pub old_top_line: usize,
    pub new_top_line: usize,
    pub delta_lines: isize,
    pub line_height_px: f32,
    pub reason: ScrollReason,
}
```

Suggested reason enum:

```rust
pub enum ScrollReason {
    CursorMove,
    HalfPageDown,
    HalfPageUp,
    CenterCursor,
    PageDown,
    PageUp,
    MouseWheel,
    Goto,
    SearchJump,
}
```

The command layer should only produce this event. The renderer or viewport animation layer should decide how to animate it.

---

### Animation Model

The renderer should convert line delta into pixel delta:

```rust
let delta_px = delta_lines as f32 * line_height_px;
```

At the start of the animation:

```rust
visual_scroll_offset_px = -delta_px;
```

Each frame:

```rust
let t = elapsed / duration;
let progress = ease_out_cubic(t);
visual_scroll_offset_px = lerp(-delta_px, 0.0, progress);
```

Suggested easing:

```rust
fn ease_out_cubic(t: f32) -> f32 {
    1.0 - (1.0 - t).powi(3)
}
```

When the animation completes:

```rust
visual_scroll_offset_px = 0.0;
scroll_animation = None;
```

The renderer should draw the logical viewport using the current visual offset:

```rust
draw_y = editor_top_y + visual_scroll_offset_px;
```

---

### Suggested Duration

The animation should be short and responsive.

Recommended values:

```txt
Small scroll, 1–3 lines:     70–90ms
Ctrl-d / Ctrl-u:             90–130ms
zz recenter:                 110–150ms
Very large jumps:            snap near target, animate only the final few lines
```

The editor must still feel instant. Smooth scroll is a visual aid, not a slow cinematic transition.

---

### Far Scroll Clamp

If the scroll distance is very large, do not animate the entire distance.

For example:

```txt
actual delta_lines = 120
animated delta_lines = 8
```

The logical viewport still jumps immediately to the real target, but the renderer only animates a short final offset.

Suggested config:

```rust
pub const FAR_SCROLL_CLAMP_LINES: isize = 8;
```

This prevents long jumps, search jumps, or goto-line actions from feeling slow.

---

### Retargeting Behavior

If the user keeps pressing keys while a scroll animation is still running, the animation must not block input.

Instead, the current visual position should be used as the new starting point, and the target should be updated.

Example:

```txt
User presses Ctrl-d
animation starts

Before animation finishes:
User presses Ctrl-d again

Expected:
Do not wait.
Do not restart from a stale old position.
Retarget the animation toward the newest logical viewport.
```

This is required for fast repeated navigation.

---

### Rendering Requirements

During the animation, the renderer may need to draw extra rows above or below the visible viewport to avoid blank gaps.

Suggested approach:

```txt
rendered_rows = visible_rows + overscan_rows
```

Where:

```txt
overscan_rows >= abs(animated_delta_lines)
```

Or use a clamped value:

```rust
let overscan_rows = animated_delta_lines.abs().min(FAR_SCROLL_CLAMP_LINES);
```

The renderer should ensure that scrolling never exposes empty space while the visual offset is non-zero.

---

### Input Behavior

Smooth scroll must support the following behavior:

```txt
j / k:
  Only trigger smooth scroll if the viewport top line changes.

Ctrl-d:
  Move logical viewport down immediately.
  Animate the visual viewport downward.

Ctrl-u:
  Move logical viewport up immediately.
  Animate the visual viewport upward.

zz:
  Recenter logical viewport around cursor immediately.
  Animate visual viewport to the new centered position.

Repeated input:
  Retarget the current animation without blocking.
```

---

### Implementation Flow

Expected high-level flow:

```txt
1. User presses a navigation key.

2. Input layer translates the key into a command.

3. Command dispatch executes the navigation command.

4. Before mutation:
   store old_top_line.

5. After mutation:
   read new_top_line.

6. If old_top_line != new_top_line:
   create ScrollRetarget.

7. Renderer receives ScrollRetarget.

8. Renderer computes pixel delta.

9. Renderer starts or retargets smooth scroll animation.

10. Each frame:
    apply visual_scroll_offset_px when drawing editor content.

11. When animation reaches target:
    clear animation state.
```

---

### Important Rule

Never delay logical state updates for animation.

Bad:

```txt
Wait for scroll animation to finish,
then update cursor/top_line.
```

Good:

```txt
Update cursor/top_line immediately,
then animate the visual viewport toward the new state.
```

The editor should always behave as if navigation is instant.

---

### Acceptance Criteria

* `j` and `k` only trigger smooth scroll when the viewport actually moves.
* `Ctrl-d` and `Ctrl-u` scroll immediately in logical state and animate visually.
* `zz` recenters immediately in logical state and animates the viewport transition.
* Repeated navigation retargets the animation instead of blocking input.
* Large jumps are clamped so they do not animate across hundreds of lines.
* Smooth scroll can be disabled by setting duration to `0`.
* Animation does not create blank gaps at the top or bottom of the editor.
* The renderer always ends with `visual_scroll_offset_px = 0`.
* Logical editor state remains the single source of truth.
* Input latency is not affected by the animation.
