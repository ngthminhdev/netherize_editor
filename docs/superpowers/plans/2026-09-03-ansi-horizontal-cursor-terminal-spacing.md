# ANSI Horizontal Cursor Terminal Spacing Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Preserve Claude Code's intended word and fragment spacing in the right-dock terminal by supporting ANSI Cursor Horizontal Absolute (`CSI n G`).

**Architecture:** Decode CHA in the existing streaming ANSI parser into a semantic terminal event, then apply that event in `TerminalGrid`, which remains the single source of terminal cell positions. The renderer stays unchanged because it already draws glyphs at their grid columns.

**Tech Stack:** Rust, existing ANSI parser, fixed-cell `TerminalGrid`, Cargo tests, GitNexus.

---

### Task 1: Add Cursor Horizontal Absolute support

**Files:**
- Modify: `src/terminal/ansi_parser.rs:68-103`
- Modify: `src/terminal/ansi_parser.rs:363-443`
- Modify: `src/terminal/grid.rs:239-428`
- Test: `src/terminal/ansi_parser.rs`
- Test: `src/terminal/grid.rs`

- [ ] **Step 1: Write failing parser and grid regression tests**

Add parser assertions that `\x1b[7G`, `\x1b[G`, and `\x1b[0G` emit `CursorHorizontalAbsolute { col: 6 }`, `{ col: 0 }`, and `{ col: 0 }`. Add a grid assertion that feeding `Fable\x1b[8G5.1` produces `Fable  5.1` across the first ten cells.

- [ ] **Step 2: Run the focused tests and verify RED**

Run:

```bash
cargo test ansi_parser::tests::cursor_horizontal_absolute --lib -- --nocapture
cargo test terminal::grid::tests::cursor_horizontal_absolute_preserves_fragment_spacing --lib -- --nocapture
```

Expected: compilation or assertion failure because `CursorHorizontalAbsolute` and `G` handling do not exist.

- [ ] **Step 3: Implement the minimal ANSI event and grid transition**

Add:

```rust
AnsiEvent::CursorHorizontalAbsolute { col }
```

Parse final byte `G` with a one-based default of one and map zero to column zero. In `TerminalGrid::apply_event`, set `cursor_col = col.min(self.cols.saturating_sub(1))` without changing the row.

- [ ] **Step 4: Run focused and broader verification**

Run:

```bash
cargo test cursor_horizontal_absolute --lib -- --nocapture
cargo test terminal --lib
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
git diff --check
```

Expected: every command exits zero.

- [ ] **Step 5: Inspect graph and diff scope**

Run GitNexus change detection and inspect `git diff -- src/terminal/ansi_parser.rs src/terminal/grid.rs`. Expected scope is the new ANSI event, its parser arm, its grid arm, and focused tests only.
