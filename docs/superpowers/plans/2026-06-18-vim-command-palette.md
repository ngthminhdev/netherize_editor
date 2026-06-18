# Vim-mode Command Palette Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a single-line modal (Vim) editor — Normal/Insert/Visual with core motions, operators, and an internal register — to the shared `CommandPalette`, so every palette/prompt overlay gains Vim editing on its query field.

**Architecture:** A self-contained state machine `CommandPalette::vim_input` owns all Vim semantics over the existing `query`/`cursor_byte`/`selection_range`. A single carrier command `Command::PaletteVimInput(PaletteVimKey)` routes keystrokes in; `vim_input` returns a `PaletteVimAction` that the event loop maps onto the **existing** list-nav / confirm / close plumbing (no per-key command explosion). Two focus resolvers (`resolve_palette_focus` for single-line prompts, `resolve_fuzzy_picker_focus` for list pickers) become Vim-aware.

**Tech Stack:** Rust, winit input events, custom GPU renderer.

## Global Constraints

- Never run `git commit`, `git push`, `git merge`, or `git tag` without explicit user instruction. (This project's owner commits manually — the `git commit` steps below are written for completeness; ASK before running them.)
- Palette opens in **Insert** mode; behavior on open is unchanged (type-and-go). Vim is always active because the editor is always modal (neovim profile); there is no enable flag.
- `Command::PaletteVimInput` is synthesized by `focus.rs` from raw keys; it MUST NOT be added to `command_ids::ALL_IDS` or `parse()`, and MUST NOT appear in any keymap TOML.
- All query mutations go through the existing `refresh_results(workspace)` so filtered result lists stay in sync (mirror `delete_char_forward`).
- Single-line only: no `j`/`k` cursor movement, no `dd`/`cc`/`yy`.
- Follow existing patterns in `command_palette.rs`, `focus.rs`, `commands_palette.rs`.

---

## Task 1: Vim Types, State Fields, and Word-Boundary Helpers

**Files:**
- Modify: `src/core/commands.rs`
- Modify: `src/app/command_palette.rs`
- Test: `src/app/command_palette.rs` (`#[cfg(test)] mod tests`)

**Interfaces:**
- Produces:
  - `enum PaletteVimKey { Char(char), Esc, Enter }` (in `commands.rs`, `pub`)
  - `enum PaletteVimMode { Insert, Normal, Visual }` (in `command_palette.rs`, `pub`, `Default = Insert`)
  - `enum PaletteVimOperator { Delete, Change, Yank }` (private)
  - `enum PaletteVimAction { Consumed, ListPrev, ListNext, Confirm, Close, Ignore }` (`pub`)
  - `CommandPalette` fields: `vim_mode: PaletteVimMode`, `pending_operator: Option<PaletteVimOperator>`, `vim_register: String`
  - `CommandPalette::vim_word_forward/backward/end(&self, byte: usize) -> usize` (private)

- [ ] **Step 1: Add `PaletteVimKey` to `commands.rs`**

In `src/core/commands.rs`, above the `Command` enum, add:

```rust
/// A keystroke forwarded into the palette's single-line Vim state machine.
/// Synthesized by the input layer — never parsed from a keymap.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PaletteVimKey {
    Char(char),
    Esc,
    Enter,
}
```

- [ ] **Step 2: Add Vim enums to `command_palette.rs`**

In `src/app/command_palette.rs`, near the top (after the existing `use` lines), add:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PaletteVimMode {
    #[default]
    Insert,
    Normal,
    Visual,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PaletteVimOperator {
    Delete,
    Change,
    Yank,
}

/// What the event loop should do after a Vim keystroke is processed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PaletteVimAction {
    /// Text/cursor/mode/register changed — just redraw.
    Consumed,
    /// `k` in a list-picker — move result selection up.
    ListPrev,
    /// `j` in a list-picker — move result selection down.
    ListNext,
    /// `Enter` — run the active palette's confirm path.
    Confirm,
    /// `Esc` in Normal — close the palette.
    Close,
    /// Nothing happened.
    Ignore,
}
```

- [ ] **Step 3: Add fields to the `CommandPalette` struct**

Find the `pub struct CommandPalette { ... }` definition. After the existing `selection_range` field, add:

```rust
    /// Current Vim sub-mode for the single-line query editor.
    pub vim_mode: PaletteVimMode,
    /// Pending operator (`d`/`c`/`y`) awaiting a motion.
    pending_operator: Option<PaletteVimOperator>,
    /// Internal unnamed register for `x`/`d`/`c`/`y` ↔ `p`/`P`.
    vim_register: String,
```

If `CommandPalette` derives `Default`, `PaletteVimMode`'s `#[default]` and the `Option`/`String` defaults cover the new fields — no manual `Default` impl change needed. If there is a hand-written `Default`/`new`, initialize `vim_mode: PaletteVimMode::Insert, pending_operator: None, vim_register: String::new()`.

- [ ] **Step 4: Reset Vim state in `open()`**

Find `CommandPalette::open(` and, where it resets `cursor_byte`/`selection_range` for a fresh prompt, add:

```rust
        self.vim_mode = PaletteVimMode::Insert;
        self.pending_operator = None;
```

(Do NOT clear `vim_register` here — registers persist across opens, like Vim.)

- [ ] **Step 5: Write failing tests for word-boundary helpers**

Append to the `#[cfg(test)] mod tests` block:

```rust
    #[test]
    fn vim_word_motions_ascii() {
        let mut p = CommandPalette::default();
        p.open(CommandPaletteMode::ExplorerPasteFile, None);
        p.set_query("foo bar.baz qux", None);
        // forward from 0: "foo" -> "bar"
        assert_eq!(p.vim_word_forward(0), 4);
        // forward from 4 ("bar"): punctuation '.' is its own word
        assert_eq!(p.vim_word_forward(4), 7);
        // word end from 0: end of "foo" is the 'o' at byte 2
        assert_eq!(p.vim_word_end(0), 2);
        // backward from 4 ("bar") -> start of "foo"
        assert_eq!(p.vim_word_backward(4), 0);
    }
```

- [ ] **Step 6: Run the test to confirm it fails**

Run: `cargo test --lib vim_word_motions_ascii`
Expected: FAIL — `vim_word_forward` not found.

- [ ] **Step 7: Implement the helpers**

Add these private methods inside `impl CommandPalette` (next to `prev_char_boundary`/`next_char_boundary`):

```rust
    fn vim_word_forward(&self, byte: usize) -> usize {
        let s = &self.query;
        let chars: Vec<(usize, char)> = s.char_indices().collect();
        if chars.is_empty() {
            return 0;
        }
        let mut i = chars.iter().position(|(b, _)| *b >= byte).unwrap_or(chars.len());
        if i >= chars.len() {
            return s.len();
        }
        let start_class = vim_char_class(chars[i].1);
        if start_class != VimCharClass::Whitespace {
            while i < chars.len() && vim_char_class(chars[i].1) == start_class {
                i += 1;
            }
        }
        while i < chars.len() && vim_char_class(chars[i].1) == VimCharClass::Whitespace {
            i += 1;
        }
        if i >= chars.len() { s.len() } else { chars[i].0 }
    }

    fn vim_word_backward(&self, byte: usize) -> usize {
        let s = &self.query;
        let chars: Vec<(usize, char)> = s.char_indices().collect();
        if chars.is_empty() || byte == 0 {
            return 0;
        }
        let mut i = chars.iter().position(|(b, _)| *b >= byte).unwrap_or(chars.len());
        if i == 0 {
            return 0;
        }
        i -= 1;
        while i > 0 && vim_char_class(chars[i].1) == VimCharClass::Whitespace {
            i -= 1;
        }
        let cls = vim_char_class(chars[i].1);
        while i > 0 && vim_char_class(chars[i - 1].1) == cls {
            i -= 1;
        }
        chars[i].0
    }

    fn vim_word_end(&self, byte: usize) -> usize {
        let s = &self.query;
        let chars: Vec<(usize, char)> = s.char_indices().collect();
        if chars.is_empty() {
            return 0;
        }
        let mut i = chars.iter().position(|(b, _)| *b >= byte).unwrap_or(chars.len());
        i += 1; // move at least one forward (vim `e`)
        while i < chars.len() && vim_char_class(chars[i].1) == VimCharClass::Whitespace {
            i += 1;
        }
        if i >= chars.len() {
            return chars.last().map(|(b, _)| *b).unwrap_or(0);
        }
        let cls = vim_char_class(chars[i].1);
        while i + 1 < chars.len() && vim_char_class(chars[i + 1].1) == cls {
            i += 1;
        }
        chars[i].0
    }
```

Add this free function and enum at the bottom of the file (module scope, not inside `impl`):

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum VimCharClass {
    Whitespace,
    Word,
    Punct,
}

fn vim_char_class(c: char) -> VimCharClass {
    if c.is_whitespace() {
        VimCharClass::Whitespace
    } else if c.is_alphanumeric() || c == '_' {
        VimCharClass::Word
    } else {
        VimCharClass::Punct
    }
}
```

- [ ] **Step 8: Run the test to confirm it passes**

Run: `cargo test --lib vim_word_motions_ascii`
Expected: PASS.

- [ ] **Step 9: Build check**

Run: `cargo check --lib`
Expected: no errors.

- [ ] **Step 10: Commit** (ask first)

```bash
git add src/core/commands.rs src/app/command_palette.rs
git commit -m "feat: add palette Vim types, state, and word-boundary helpers"
```

---

## Task 2: `vim_input` — Insert/Normal Motions and Mode Transitions

**Files:**
- Modify: `src/app/command_palette.rs`
- Test: `src/app/command_palette.rs`

**Interfaces:**
- Consumes: Task 1 enums/fields/helpers, existing `prev_char_boundary`/`next_char_boundary`, `normalized_selection_range`, `refresh_results`.
- Produces: `CommandPalette::vim_input(&mut self, key: PaletteVimKey, has_result_list: bool, workspace: Option<&WorkspaceModel>) -> PaletteVimAction`

- [ ] **Step 1: Write failing tests**

Append to the tests module:

```rust
    #[test]
    fn vim_esc_enters_normal_and_clamps_left() {
        let mut p = CommandPalette::default();
        p.open(CommandPaletteMode::ExplorerPasteFile, None);
        p.set_query("abc", None);
        p.cursor_byte = 3;
        let action = p.vim_input(PaletteVimKey::Esc, false, None);
        assert_eq!(action, PaletteVimAction::Consumed);
        assert_eq!(p.vim_mode, PaletteVimMode::Normal);
        assert_eq!(p.cursor_byte, 2); // clamped one char left
    }

    #[test]
    fn vim_normal_hl_and_word_motions() {
        let mut p = CommandPalette::default();
        p.open(CommandPaletteMode::ExplorerPasteFile, None);
        p.set_query("foo bar", None);
        p.vim_input(PaletteVimKey::Esc, false, None); // -> Normal, cursor 6
        p.cursor_byte = 0;
        p.vim_input(PaletteVimKey::Char('l'), false, None);
        assert_eq!(p.cursor_byte, 1);
        p.vim_input(PaletteVimKey::Char('h'), false, None);
        assert_eq!(p.cursor_byte, 0);
        p.vim_input(PaletteVimKey::Char('w'), false, None);
        assert_eq!(p.cursor_byte, 4); // start of "bar"
        p.vim_input(PaletteVimKey::Char('$'), false, None);
        assert_eq!(p.cursor_byte, "foo bar".len());
        p.vim_input(PaletteVimKey::Char('0'), false, None);
        assert_eq!(p.cursor_byte, 0);
    }

    #[test]
    fn vim_insert_transitions_and_x() {
        let mut p = CommandPalette::default();
        p.open(CommandPaletteMode::ExplorerPasteFile, None);
        p.set_query("abc", None);
        p.vim_input(PaletteVimKey::Esc, false, None); // Normal, cursor 2
        p.cursor_byte = 0;
        p.vim_input(PaletteVimKey::Char('x'), false, None);
        assert_eq!(p.query, "bc");
        assert_eq!(p.vim_register, "a");
        p.vim_input(PaletteVimKey::Char('A'), false, None);
        assert_eq!(p.vim_mode, PaletteVimMode::Insert);
        assert_eq!(p.cursor_byte, "bc".len());
    }

    #[test]
    fn vim_jk_in_list_picker_returns_list_actions() {
        let mut p = CommandPalette::default();
        p.open(CommandPaletteMode::FilePicker, None);
        p.set_query("x", None);
        p.vim_input(PaletteVimKey::Esc, false, None); // Normal
        assert_eq!(p.vim_input(PaletteVimKey::Char('j'), true, None), PaletteVimAction::ListNext);
        assert_eq!(p.vim_input(PaletteVimKey::Char('k'), true, None), PaletteVimAction::ListPrev);
        // single-line prompt: j/k ignored
        assert_eq!(p.vim_input(PaletteVimKey::Char('j'), false, None), PaletteVimAction::Ignore);
    }

    #[test]
    fn vim_enter_returns_confirm_esc_normal_returns_close() {
        let mut p = CommandPalette::default();
        p.open(CommandPaletteMode::ExplorerPasteFile, None);
        p.set_query("a", None);
        assert_eq!(p.vim_input(PaletteVimKey::Enter, false, None), PaletteVimAction::Confirm);
        p.vim_input(PaletteVimKey::Esc, false, None); // Insert -> Normal
        assert_eq!(p.vim_input(PaletteVimKey::Esc, false, None), PaletteVimAction::Close);
    }
```

- [ ] **Step 2: Run tests to confirm they fail**

Run: `cargo test --lib vim_`
Expected: FAIL — `vim_input` not found.

- [ ] **Step 3: Implement `vim_input` (Insert + Normal core; operators/Visual are stubs for now)**

Add inside `impl CommandPalette`:

```rust
    pub fn vim_input(
        &mut self,
        key: PaletteVimKey,
        has_result_list: bool,
        workspace: Option<&WorkspaceModel>,
    ) -> PaletteVimAction {
        match self.vim_mode {
            PaletteVimMode::Insert => match key {
                PaletteVimKey::Esc => {
                    self.vim_mode = PaletteVimMode::Normal;
                    self.selection_range = None;
                    self.pending_operator = None;
                    if self.cursor_byte > 0 {
                        self.cursor_byte = self.prev_char_boundary(self.cursor_byte);
                    }
                    PaletteVimAction::Consumed
                }
                PaletteVimKey::Enter => PaletteVimAction::Confirm,
                PaletteVimKey::Char(_) => PaletteVimAction::Ignore,
            },
            PaletteVimMode::Normal => self.vim_input_normal(key, has_result_list, workspace),
            PaletteVimMode::Visual => self.vim_input_visual(key, workspace),
        }
    }

    fn vim_input_normal(
        &mut self,
        key: PaletteVimKey,
        has_result_list: bool,
        workspace: Option<&WorkspaceModel>,
    ) -> PaletteVimAction {
        let c = match key {
            PaletteVimKey::Enter => return PaletteVimAction::Confirm,
            PaletteVimKey::Esc => {
                if self.pending_operator.take().is_some() {
                    return PaletteVimAction::Consumed;
                }
                return PaletteVimAction::Close;
            }
            PaletteVimKey::Char(c) => c,
        };

        // If an operator is pending, treat this key as its motion (Task 3).
        if self.pending_operator.is_some() {
            return self.vim_apply_operator_motion(c, workspace);
        }

        match c {
            'h' => {
                if self.cursor_byte > 0 {
                    self.cursor_byte = self.prev_char_boundary(self.cursor_byte);
                }
                PaletteVimAction::Consumed
            }
            'l' => {
                if self.cursor_byte < self.query.len() {
                    self.cursor_byte = self.next_char_boundary(self.cursor_byte);
                }
                PaletteVimAction::Consumed
            }
            'w' => {
                self.cursor_byte = self.vim_word_forward(self.cursor_byte);
                PaletteVimAction::Consumed
            }
            'b' => {
                self.cursor_byte = self.vim_word_backward(self.cursor_byte);
                PaletteVimAction::Consumed
            }
            'e' => {
                self.cursor_byte = self.vim_word_end(self.cursor_byte);
                PaletteVimAction::Consumed
            }
            '0' => {
                self.cursor_byte = 0;
                PaletteVimAction::Consumed
            }
            '^' => {
                self.cursor_byte = self
                    .query
                    .char_indices()
                    .find(|(_, ch)| !ch.is_whitespace())
                    .map(|(b, _)| b)
                    .unwrap_or(0);
                PaletteVimAction::Consumed
            }
            '$' => {
                self.cursor_byte = self.query.len();
                PaletteVimAction::Consumed
            }
            'i' => {
                self.vim_mode = PaletteVimMode::Insert;
                PaletteVimAction::Consumed
            }
            'a' => {
                if self.cursor_byte < self.query.len() {
                    self.cursor_byte = self.next_char_boundary(self.cursor_byte);
                }
                self.vim_mode = PaletteVimMode::Insert;
                PaletteVimAction::Consumed
            }
            'I' => {
                self.cursor_byte = self
                    .query
                    .char_indices()
                    .find(|(_, ch)| !ch.is_whitespace())
                    .map(|(b, _)| b)
                    .unwrap_or(0);
                self.vim_mode = PaletteVimMode::Insert;
                PaletteVimAction::Consumed
            }
            'A' => {
                self.cursor_byte = self.query.len();
                self.vim_mode = PaletteVimMode::Insert;
                PaletteVimAction::Consumed
            }
            'x' => {
                if self.cursor_byte < self.query.len() {
                    let end = self.next_char_boundary(self.cursor_byte);
                    self.vim_register = self.query[self.cursor_byte..end].to_string();
                    self.query.replace_range(self.cursor_byte..end, "");
                    if self.cursor_byte > self.query.len() {
                        self.cursor_byte = self.query.len();
                    }
                    self.selected_index = 0;
                    self.refresh_results(workspace);
                }
                PaletteVimAction::Consumed
            }
            'v' => {
                self.vim_mode = PaletteVimMode::Visual;
                self.selection_range = Some((self.cursor_byte, self.cursor_byte));
                PaletteVimAction::Consumed
            }
            'd' | 'c' | 'y' => {
                self.pending_operator = Some(match c {
                    'd' => PaletteVimOperator::Delete,
                    'c' => PaletteVimOperator::Change,
                    _ => PaletteVimOperator::Yank,
                });
                PaletteVimAction::Consumed
            }
            'p' | 'P' => self.vim_paste_register(c == 'p', workspace),
            'j' => {
                if has_result_list {
                    PaletteVimAction::ListNext
                } else {
                    PaletteVimAction::Ignore
                }
            }
            'k' => {
                if has_result_list {
                    PaletteVimAction::ListPrev
                } else {
                    PaletteVimAction::Ignore
                }
            }
            _ => PaletteVimAction::Ignore,
        }
    }
```

Add temporary stubs (replaced in Tasks 3 and 4) so the file compiles:

```rust
    fn vim_apply_operator_motion(
        &mut self,
        _motion: char,
        _workspace: Option<&WorkspaceModel>,
    ) -> PaletteVimAction {
        self.pending_operator = None;
        PaletteVimAction::Consumed
    }

    fn vim_paste_register(
        &mut self,
        _after: bool,
        _workspace: Option<&WorkspaceModel>,
    ) -> PaletteVimAction {
        PaletteVimAction::Consumed
    }

    fn vim_input_visual(
        &mut self,
        key: PaletteVimKey,
        _workspace: Option<&WorkspaceModel>,
    ) -> PaletteVimAction {
        if matches!(key, PaletteVimKey::Esc) {
            self.vim_mode = PaletteVimMode::Normal;
            self.selection_range = None;
        }
        PaletteVimAction::Consumed
    }
```

- [ ] **Step 4: Run tests to confirm they pass**

Run: `cargo test --lib vim_`
Expected: PASS (all Task 2 tests).

- [ ] **Step 5: Build check**

Run: `cargo check --lib`
Expected: no errors.

- [ ] **Step 6: Commit** (ask first)

```bash
git add src/app/command_palette.rs
git commit -m "feat: palette Vim Insert/Normal motions and transitions"
```

---

## Task 3: `vim_input` — Operators, Register, Paste

**Files:**
- Modify: `src/app/command_palette.rs`
- Test: `src/app/command_palette.rs`

**Interfaces:**
- Consumes: Task 2 `vim_input`, `pending_operator`, `vim_register`, word helpers.
- Produces: real `vim_apply_operator_motion` and `vim_paste_register` (replacing the Task 2 stubs).

- [ ] **Step 1: Write failing tests**

```rust
    #[test]
    fn vim_dw_cw_yw_and_paste() {
        let mut p = CommandPalette::default();
        p.open(CommandPaletteMode::ExplorerPasteFile, None);
        p.set_query("foo bar baz", None);
        p.vim_input(PaletteVimKey::Esc, false, None);
        p.cursor_byte = 0;
        // dw deletes "foo " -> "bar baz", register = "foo "
        p.vim_input(PaletteVimKey::Char('d'), false, None);
        p.vim_input(PaletteVimKey::Char('w'), false, None);
        assert_eq!(p.query, "bar baz");
        assert_eq!(p.vim_register, "foo ");
        // p pastes register after cursor
        p.cursor_byte = p.query.len();
        p.vim_input(PaletteVimKey::Char('p'), false, None);
        assert_eq!(p.query, "bar bazfoo ");
    }

    #[test]
    fn vim_cw_enters_insert() {
        let mut p = CommandPalette::default();
        p.open(CommandPaletteMode::ExplorerPasteFile, None);
        p.set_query("foo bar", None);
        p.vim_input(PaletteVimKey::Esc, false, None);
        p.cursor_byte = 0;
        p.vim_input(PaletteVimKey::Char('c'), false, None);
        p.vim_input(PaletteVimKey::Char('w'), false, None);
        assert_eq!(p.query, "bar"); // "foo " removed (cw)
        assert_eq!(p.vim_mode, PaletteVimMode::Insert);
    }

    #[test]
    fn vim_d_dollar_deletes_to_end() {
        let mut p = CommandPalette::default();
        p.open(CommandPaletteMode::ExplorerPasteFile, None);
        p.set_query("hello world", None);
        p.vim_input(PaletteVimKey::Esc, false, None);
        p.cursor_byte = 5;
        p.vim_input(PaletteVimKey::Char('d'), false, None);
        p.vim_input(PaletteVimKey::Char('$'), false, None);
        assert_eq!(p.query, "hello");
        assert_eq!(p.vim_register, " world");
    }
```

- [ ] **Step 2: Run tests to confirm they fail**

Run: `cargo test --lib vim_dw_cw_yw_and_paste vim_cw_enters_insert vim_d_dollar_deletes_to_end`
Expected: FAIL (stubs do nothing).

- [ ] **Step 3: Replace the stubs with real implementations**

Replace `vim_apply_operator_motion` and `vim_paste_register` from Task 2 with:

```rust
    fn vim_apply_operator_motion(
        &mut self,
        motion: char,
        workspace: Option<&WorkspaceModel>,
    ) -> PaletteVimAction {
        let op = match self.pending_operator.take() {
            Some(op) => op,
            None => return PaletteVimAction::Consumed,
        };
        let start = self.cursor_byte;
        // `cw` behaves like `ce` in Vim: change to end of word (inclusive).
        let target = match (op, motion) {
            (PaletteVimOperator::Change, 'w') => {
                let end = self.vim_word_end(start);
                self.next_char_boundary(end.max(start))
            }
            (_, 'w') => self.vim_word_forward(start),
            (_, 'b') => self.vim_word_backward(start),
            (_, 'e') => self.next_char_boundary(self.vim_word_end(start)),
            (_, '$') => self.query.len(),
            (_, '0' | '^') => 0,
            _ => return PaletteVimAction::Consumed, // unsupported motion: cancel
        };
        let (lo, hi) = if target >= start { (start, target) } else { (target, start) };
        let lo = lo.min(self.query.len());
        let hi = hi.min(self.query.len());
        if lo == hi {
            return PaletteVimAction::Consumed;
        }
        self.vim_register = self.query[lo..hi].to_string();
        match op {
            PaletteVimOperator::Yank => {
                self.cursor_byte = lo;
            }
            PaletteVimOperator::Delete | PaletteVimOperator::Change => {
                self.query.replace_range(lo..hi, "");
                self.cursor_byte = lo;
                self.selected_index = 0;
                self.refresh_results(workspace);
                if matches!(op, PaletteVimOperator::Change) {
                    self.vim_mode = PaletteVimMode::Insert;
                }
            }
        }
        PaletteVimAction::Consumed
    }

    fn vim_paste_register(
        &mut self,
        after: bool,
        workspace: Option<&WorkspaceModel>,
    ) -> PaletteVimAction {
        if self.vim_register.is_empty() {
            return PaletteVimAction::Consumed;
        }
        let at = if after && self.cursor_byte < self.query.len() {
            self.next_char_boundary(self.cursor_byte)
        } else {
            self.cursor_byte
        };
        let reg = self.vim_register.clone();
        self.query.insert_str(at, &reg);
        self.cursor_byte = at + reg.len();
        self.selected_index = 0;
        self.refresh_results(workspace);
        PaletteVimAction::Consumed
    }
```

- [ ] **Step 4: Run tests to confirm they pass**

Run: `cargo test --lib vim_`
Expected: PASS (all Vim tests so far).

- [ ] **Step 5: Build check**

Run: `cargo check --lib`
Expected: no errors.

- [ ] **Step 6: Commit** (ask first)

```bash
git add src/app/command_palette.rs
git commit -m "feat: palette Vim operators, register, and paste"
```

---

## Task 4: `vim_input` — Visual Mode

**Files:**
- Modify: `src/app/command_palette.rs`
- Test: `src/app/command_palette.rs`

**Interfaces:**
- Consumes: Task 2/3 helpers, `selection_range`, `vim_register`.
- Produces: real `vim_input_visual` (replacing the Task 2 stub).

- [ ] **Step 1: Write failing tests**

```rust
    #[test]
    fn vim_visual_select_and_delete() {
        let mut p = CommandPalette::default();
        p.open(CommandPaletteMode::ExplorerPasteFile, None);
        p.set_query("foo bar", None);
        p.vim_input(PaletteVimKey::Esc, false, None);
        p.cursor_byte = 0;
        p.vim_input(PaletteVimKey::Char('v'), false, None);
        assert_eq!(p.vim_mode, PaletteVimMode::Visual);
        p.vim_input(PaletteVimKey::Char('w'), false, None); // extend to "bar"
        p.vim_input(PaletteVimKey::Char('d'), false, None); // delete selection
        assert_eq!(p.query, "bar");
        assert_eq!(p.vim_mode, PaletteVimMode::Normal);
        assert_eq!(p.vim_register, "foo ");
    }

    #[test]
    fn vim_visual_yank_keeps_text() {
        let mut p = CommandPalette::default();
        p.open(CommandPaletteMode::ExplorerPasteFile, None);
        p.set_query("hello", None);
        p.vim_input(PaletteVimKey::Esc, false, None);
        p.cursor_byte = 0;
        p.vim_input(PaletteVimKey::Char('v'), false, None);
        p.vim_input(PaletteVimKey::Char('$'), false, None);
        p.vim_input(PaletteVimKey::Char('y'), false, None);
        assert_eq!(p.query, "hello");
        assert_eq!(p.vim_mode, PaletteVimMode::Normal);
        assert_eq!(p.vim_register, "hello");
    }

    #[test]
    fn vim_visual_esc_returns_to_normal() {
        let mut p = CommandPalette::default();
        p.open(CommandPaletteMode::ExplorerPasteFile, None);
        p.set_query("ab", None);
        p.vim_input(PaletteVimKey::Esc, false, None);
        p.vim_input(PaletteVimKey::Char('v'), false, None);
        p.vim_input(PaletteVimKey::Esc, false, None);
        assert_eq!(p.vim_mode, PaletteVimMode::Normal);
        assert!(p.selection_range.is_none());
    }
```

- [ ] **Step 2: Run tests to confirm they fail**

Run: `cargo test --lib vim_visual`
Expected: FAIL (stub doesn't extend selection or delete).

- [ ] **Step 3: Replace the `vim_input_visual` stub**

```rust
    fn vim_input_visual(
        &mut self,
        key: PaletteVimKey,
        workspace: Option<&WorkspaceModel>,
    ) -> PaletteVimAction {
        let anchor = self.selection_range.map(|(a, _)| a).unwrap_or(self.cursor_byte);
        let c = match key {
            PaletteVimKey::Enter => return PaletteVimAction::Confirm,
            PaletteVimKey::Esc => {
                self.vim_mode = PaletteVimMode::Normal;
                self.selection_range = None;
                return PaletteVimAction::Consumed;
            }
            PaletteVimKey::Char(c) => c,
        };

        // Motions move cursor and extend the selection from the anchor.
        let moved = match c {
            'h' => {
                if self.cursor_byte > 0 {
                    self.cursor_byte = self.prev_char_boundary(self.cursor_byte);
                }
                true
            }
            'l' => {
                if self.cursor_byte < self.query.len() {
                    self.cursor_byte = self.next_char_boundary(self.cursor_byte);
                }
                true
            }
            'w' => { self.cursor_byte = self.vim_word_forward(self.cursor_byte); true }
            'b' => { self.cursor_byte = self.vim_word_backward(self.cursor_byte); true }
            'e' => { self.cursor_byte = self.next_char_boundary(self.vim_word_end(self.cursor_byte)); true }
            '0' => { self.cursor_byte = 0; true }
            '$' => { self.cursor_byte = self.query.len(); true }
            _ => false,
        };
        if moved {
            self.selection_range = Some((anchor, self.cursor_byte));
            return PaletteVimAction::Consumed;
        }

        // Operators apply to the current selection.
        if matches!(c, 'd' | 'c' | 'y') {
            if let Some((lo, hi)) = self.normalized_selection_range() {
                self.vim_register = self.query[lo..hi].to_string();
                if c == 'y' {
                    self.cursor_byte = lo;
                } else {
                    self.query.replace_range(lo..hi, "");
                    self.cursor_byte = lo;
                    self.selected_index = 0;
                    self.refresh_results(workspace);
                }
                self.selection_range = None;
                self.vim_mode = if c == 'c' {
                    PaletteVimMode::Insert
                } else {
                    PaletteVimMode::Normal
                };
            }
            return PaletteVimAction::Consumed;
        }

        PaletteVimAction::Ignore
    }
```

- [ ] **Step 4: Run tests to confirm they pass**

Run: `cargo test --lib vim_`
Expected: PASS (all Vim tests).

- [ ] **Step 5: Build check**

Run: `cargo check --lib`
Expected: no errors.

- [ ] **Step 6: Commit** (ask first)

```bash
git add src/app/command_palette.rs
git commit -m "feat: palette Vim Visual mode"
```

---

## Task 5: Render Mode-aware Caret and Status Indicator

**Files:**
- Modify: `src/app/command_palette.rs`
- Modify: `src/render/renderer/palette/minimal.rs`
- Test: `cargo check --lib` (visual/manual)

**Interfaces:**
- Consumes: `vim_mode`, `cursor_byte`, `selection_range`.
- Produces: `CommandPaletteRenderModel.vim_mode_label: Option<&'static str>` and `vim_caret_block: bool`.

- [ ] **Step 1: Extend the render model**

In `CommandPaletteRenderModel`, after `prompt_selection_range`, add:

```rust
    pub vim_mode_label: Option<&'static str>,
    pub vim_caret_block: bool,
```

- [ ] **Step 2: Populate the fields in `render()`**

In `CommandPalette::render()`, inside the `Some(CommandPaletteRenderModel { ... })` literal, after `prompt_selection_range`, add:

```rust
            vim_mode_label: match self.vim_mode {
                PaletteVimMode::Insert => Some("INSERT"),
                PaletteVimMode::Normal => Some("NORMAL"),
                PaletteVimMode::Visual => Some("VISUAL"),
            },
            vim_caret_block: matches!(self.vim_mode, PaletteVimMode::Normal | PaletteVimMode::Visual),
```

- [ ] **Step 3: Make the caret mode-aware in the minimalist renderer**

In `src/render/renderer/palette/minimal.rs`, in the editable-prompt caret block added previously (the `else` branch that draws the caret), replace the fixed `let caret_w = 2.0_f32;` with:

```rust
                let caret_w = if model.vim_caret_block {
                    estimate_monospace_width("M", font_size).max(2.0)
                } else {
                    2.0_f32
                };
                let mut caret_color = model.text_color;
                caret_color[3] = if model.vim_caret_block { 0.45 } else { 0.9 };
```

(The block caret is wider and semi-transparent so the character under it stays readable.)

- [ ] **Step 4: Render the mode label**

Still in `render_command_palette_minimalist`, after the caret/selection block, add a right-aligned status label:

```rust
        if let Some(label) = model.vim_mode_label {
            let text = format!("-- {label} --");
            let label_w = estimate_monospace_width(&text, font_size);
            let label_x = panel_x + panel_w - model.panel_padding - label_w;
            text_runs.push(TextRun::new(
                text,
                [label_x, prompt_y],
                font_size,
                model.dim_text_color,
            ));
        }
```

> Adjust `text_runs`/`TextRun::new`/`model.dim_text_color` to match the exact text-batching API used elsewhere in this file (grep for an existing `TextRun::new(` call in `minimal.rs` and copy its shape — field names like `dim_text_color` may differ; use whatever dim/secondary color the file already exposes on `model`).

- [ ] **Step 5: Build check**

Run: `cargo check --lib`
Expected: no errors. If `TextRun`/color field names differ, fix to match the existing calls in the same function.

- [ ] **Step 6: Commit** (ask first)

```bash
git add src/app/command_palette.rs src/render/renderer/palette/minimal.rs
git commit -m "feat: render palette Vim caret and mode indicator"
```

---

## Task 6: Carrier Command, Context Plumbing, and Event-Loop Handler

**Files:**
- Modify: `src/core/commands.rs`
- Modify: `src/app/input_map/mod.rs`
- Modify: `src/app/event_loop/setup.rs`
- Modify: `src/app/event_loop/commands_palette.rs`
- Test: `cargo check --lib`

**Interfaces:**
- Consumes: `PaletteVimKey`, `CommandPalette::vim_input`, `PaletteVimAction`, existing `handle_palette_and_open_command`.
- Produces:
  - `Command::PaletteVimInput(PaletteVimKey)`
  - `KeybindingContext.palette_vim_mode: Option<PaletteVimMode>`
  - `AppShell::handle_palette_vim_input(&mut self, key: PaletteVimKey) -> Option<bool>`
  - `AppState::command_palette_vim_input(&mut self, key, has_result_list) -> PaletteVimAction`

- [ ] **Step 1: Add the `Command` variant**

In `src/core/commands.rs`, add to the `Command` enum (in the palette section):

```rust
    /// A keystroke routed into the palette's single-line Vim state machine.
    PaletteVimInput(PaletteVimKey),
```

If `Command` has a hand-written list/`is_*` matcher that must enumerate every variant (e.g. the `dispatch_command_with_clipboard_once` no-clipboard group in `command_dispatch/mod.rs`), add `Command::PaletteVimInput(_)` alongside the existing `PaletteMoveCursor*` entries there.

- [ ] **Step 2: Thread `palette_vim_mode` through `KeybindingContext`**

In `src/app/input_map/mod.rs`, add to `struct KeybindingContext` (after `command_palette_mode`):

```rust
    pub palette_vim_mode: Option<crate::app::command_palette::PaletteVimMode>,
```

Initialize it to `None` in BOTH `for_mode` constructors (the two `command_palette_mode: None,` sites at lines ~139 and ~166 get a sibling `palette_vim_mode: None,`).

- [ ] **Step 3: Populate it in `build_context`**

In `src/app/event_loop/setup.rs::build_context` (the `KeybindingContext { ... }` literal near line 823), add:

```rust
            palette_vim_mode: if self.app_state.is_command_palette_visible() {
                Some(self.app_state.command_palette_vim_mode())
            } else {
                None
            },
```

Add the accessor in `src/app/app_state/palette.rs` (next to `command_palette_mode`):

```rust
    pub fn command_palette_vim_mode(&self) -> crate::app::command_palette::PaletteVimMode {
        self.command_palette.vim_mode
    }
```

- [ ] **Step 4: Add the `AppState` entry point**

In `src/app/app_state/palette.rs`, after `command_palette_delete_char_forward`:

```rust
    pub fn command_palette_vim_input(
        &mut self,
        key: crate::core::commands::PaletteVimKey,
        has_result_list: bool,
    ) -> crate::app::command_palette::PaletteVimAction {
        let workspace = self.workspace_model.as_ref();
        let action = self.command_palette.vim_input(key, has_result_list, workspace);
        self.sync_file_picker_cache();
        action
    }
```

- [ ] **Step 5: Add the event-loop handler**

In `src/app/event_loop/commands_palette.rs`, add a method on `AppShell`:

```rust
    pub(super) fn handle_palette_vim_input(&mut self, key: PaletteVimKey) -> Option<bool> {
        let mode = self.app_state.command_palette_mode();
        let has_result_list = !matches!(
            mode,
            Some(
                CommandPaletteMode::ExplorerCreateFile
                    | CommandPaletteMode::ExplorerCreateFolder
                    | CommandPaletteMode::ExplorerRenameFull
                    | CommandPaletteMode::ExplorerRenameBase
                    | CommandPaletteMode::ExplorerPasteFile
                    | CommandPaletteMode::LspRename
            )
        );
        match self
            .app_state
            .command_palette_vim_input(key, has_result_list)
        {
            PaletteVimAction::Consumed | PaletteVimAction::Ignore => Some(true),
            PaletteVimAction::ListNext => self.handle_palette_and_open_command(
                &Command::OverlaySelectNext,
                1,
                &Command::OverlaySelectNext,
            ),
            PaletteVimAction::ListPrev => self.handle_palette_and_open_command(
                &Command::OverlaySelectPrev,
                1,
                &Command::OverlaySelectPrev,
            ),
            PaletteVimAction::Confirm => self.handle_palette_and_open_command(
                &Command::FilePickerConfirmSelection,
                1,
                &Command::FilePickerConfirmSelection,
            ),
            PaletteVimAction::Close => self.handle_palette_and_open_command(
                &Command::CloseFilePicker,
                1,
                &Command::CloseFilePicker,
            ),
        }
    }
```

- [ ] **Step 6: Route the carrier command into the handler**

Near the top of the `match command` in `handle_palette_and_open_command`, add an arm (before the generic fallthrough):

```rust
            Command::PaletteVimInput(key) => self.handle_palette_vim_input(*key),
```

Ensure `PaletteVimKey` and `PaletteVimAction` are imported (they re-export through `use super::*;` if `command_palette`/`commands` are already glob-imported; otherwise add explicit `use` lines).

- [ ] **Step 7: Build check**

Run: `cargo check --lib`
Expected: no errors. Fix any missing `use` for `PaletteVimKey`/`PaletteVimAction`/`PaletteVimMode`.

- [ ] **Step 8: Commit** (ask first)

```bash
git add src/core/commands.rs src/app/input_map/mod.rs src/app/event_loop/setup.rs src/app/event_loop/commands_palette.rs src/app/app_state/palette.rs
git commit -m "feat: route PaletteVimInput through context and event loop"
```

---

## Task 7: Vim-aware Routing in `resolve_palette_focus` (Single-line Prompts)

**Files:**
- Modify: `src/app/input_map/focus.rs`
- Test: `src/app/input_map/tests.rs`

**Interfaces:**
- Consumes: `KeybindingContext.palette_vim_mode`, `Command::PaletteVimInput`, `PaletteVimKey`.
- Produces: Vim routing for the prompt-style palette focus path (lines ~931-990).

- [ ] **Step 1: Write a failing test**

In `src/app/input_map/tests.rs`, add (adapt helper names to the file's existing test scaffolding for building a `NormalizedInput` + palette-focus `KeybindingContext`):

```rust
    #[test]
    fn palette_focus_normal_mode_routes_chars_to_vim_input() {
        let input_map = InputMap::new_default();
        let mut context = KeybindingContext::for_mode(EditorMode::PaletteFocus);
        context.command_palette_visible = true;
        context.command_palette_mode = Some(CommandPaletteMode::ExplorerPasteFile);
        context.palette_vim_mode = Some(crate::app::command_palette::PaletteVimMode::Normal);

        // 'd' in Normal must become PaletteVimInput, not an AppendQuery.
        let m = input_map
            .resolve(&normalized_char('d'), &context)
            .expect("should resolve");
        assert_eq!(
            m.command,
            Command::PaletteVimInput(crate::core::commands::PaletteVimKey::Char('d'))
        );

        // Esc in Insert routes to PaletteVimInput(Esc) (enter Normal), not close.
        context.palette_vim_mode = Some(crate::app::command_palette::PaletteVimMode::Insert);
        let esc = input_map
            .resolve(&normalized_named(NamedKey::Escape), &context)
            .expect("should resolve");
        assert_eq!(
            esc.command,
            Command::PaletteVimInput(crate::core::commands::PaletteVimKey::Esc)
        );
    }
```

> If `normalized_char` / `normalized_named` helpers don't exist in `tests.rs`, build the `NormalizedInput` the same way neighboring tests in this file do (grep for an existing `resolve(` call in `tests.rs`).

- [ ] **Step 2: Run the test to confirm it fails**

Run: `cargo test --lib palette_focus_normal_mode_routes_chars_to_vim_input`
Expected: FAIL — `'d'` currently resolves to `FilePickerAppendQuery`.

- [ ] **Step 3: Add the Vim branch at the top of `resolve_palette_focus`**

In `src/app/input_map/focus.rs::resolve_palette_focus`, BEFORE the existing `if let Some(named) = input.named_key {` block (i.e. first thing after the function's existing guards), add:

```rust
        // Vim routing: in Normal/Visual forward keys to the palette state machine;
        // in Insert, only Esc is intercepted (to enter Normal) — everything else
        // falls through to the normal type-and-go handling below.
        if let Some(vim_mode) = self.palette_vim_mode_for(context) {
            use crate::core::commands::PaletteVimKey;
            let to_vim = |key: PaletteVimKey, reason: &'static str| {
                Some(KeybindingMatch {
                    command: Command::PaletteVimInput(key),
                    reason,
                })
            };
            match vim_mode {
                PaletteVimMode::Insert => {
                    if input.named_key == Some(NamedKey::Escape) {
                        return to_vim(PaletteVimKey::Esc, "palette Vim: Esc -> Normal");
                    }
                    // fall through to existing Insert handling
                }
                PaletteVimMode::Normal | PaletteVimMode::Visual => {
                    if input.named_key == Some(NamedKey::Escape) {
                        return to_vim(PaletteVimKey::Esc, "palette Vim: Esc");
                    }
                    if input.named_key == Some(NamedKey::Enter) {
                        return to_vim(PaletteVimKey::Enter, "palette Vim: Enter");
                    }
                    // Preserve existing list-nav keys (arrows) so they keep working.
                    if matches!(
                        input.named_key,
                        Some(NamedKey::ArrowUp | NamedKey::ArrowDown | NamedKey::ArrowLeft | NamedKey::ArrowRight)
                    ) {
                        // fall through to existing named-key handling
                    } else if let Some(ch) = single_char(&input.text) {
                        return to_vim(PaletteVimKey::Char(ch), "palette Vim: char");
                    }
                }
            }
        }
```

Add two small helpers in the same `impl InputMap`:

```rust
    fn palette_vim_mode_for(&self, context: &KeybindingContext) -> Option<PaletteVimMode> {
        if context.command_palette_visible {
            context.palette_vim_mode
        } else {
            None
        }
    }
```

And a module-scope helper near the other free fns in `focus.rs`:

```rust
fn single_char(text: &str) -> Option<char> {
    let mut it = text.chars();
    match (it.next(), it.next()) {
        (Some(c), None) if !c.is_control() => Some(c),
        _ => None,
    }
}
```

Add `use crate::app::command_palette::PaletteVimMode;` to the top of `focus.rs` if not already imported.

- [ ] **Step 4: Run the test to confirm it passes**

Run: `cargo test --lib palette_focus_normal_mode_routes_chars_to_vim_input`
Expected: PASS.

- [ ] **Step 5: Run the full input-map test suite (regression)**

Run: `cargo test --lib input_map`
Expected: PASS (Insert-mode typing still appends; existing palette tests unaffected).

- [ ] **Step 6: Build check**

Run: `cargo check --lib`
Expected: no errors.

- [ ] **Step 7: Commit** (ask first)

```bash
git add src/app/input_map/focus.rs src/app/input_map/tests.rs
git commit -m "feat: Vim-aware routing in palette prompt focus"
```

---

## Task 8: Vim-aware Routing in `resolve_fuzzy_picker_focus` (List Pickers)

**Files:**
- Modify: `src/app/input_map/focus.rs`
- Test: `src/app/input_map/tests.rs`

**Interfaces:**
- Consumes: same as Task 7.
- Produces: Vim routing in the fuzzy-picker focus path so `j/k` (Normal) navigate the list and text-Vim works on the query, while `Ctrl+N/P` + arrows are preserved.

- [ ] **Step 1: Write a failing test**

```rust
    #[test]
    fn fuzzy_picker_normal_mode_routes_jk_to_vim_input() {
        let input_map = InputMap::new_default();
        let mut context = KeybindingContext::for_mode(EditorMode::Insert);
        context.command_palette_visible = true;
        context.command_palette_mode = Some(CommandPaletteMode::FilePicker);
        context.palette_vim_mode = Some(crate::app::command_palette::PaletteVimMode::Normal);

        let j = input_map
            .resolve(&normalized_char('j'), &context)
            .expect("should resolve");
        assert_eq!(
            j.command,
            Command::PaletteVimInput(crate::core::commands::PaletteVimKey::Char('j'))
        );
    }
```

- [ ] **Step 2: Run the test to confirm it fails**

Run: `cargo test --lib fuzzy_picker_normal_mode_routes_jk_to_vim_input`
Expected: FAIL — `'j'` currently appends to the query.

- [ ] **Step 3: Add the Vim branch in `resolve_fuzzy_picker_focus`**

In `src/app/input_map/focus.rs::resolve_fuzzy_picker_focus`, inside the `if is_insert {` block, AFTER the existing `mod+v` / live-grep `Ctrl+A` early returns but BEFORE the `Escape`/`Enter`/arrow handlers, add:

```rust
            if let Some(vim_mode) = self.palette_vim_mode_for(&context) {
                use crate::core::commands::PaletteVimKey;
                match vim_mode {
                    PaletteVimMode::Insert => {
                        if input.named_key == Some(NamedKey::Escape) {
                            return Some(KeybindingMatch {
                                command: Command::PaletteVimInput(PaletteVimKey::Esc),
                                reason: "fuzzy picker Vim: Esc -> Normal",
                            });
                        }
                    }
                    PaletteVimMode::Normal | PaletteVimMode::Visual => {
                        if input.named_key == Some(NamedKey::Escape) {
                            return Some(KeybindingMatch {
                                command: Command::PaletteVimInput(PaletteVimKey::Esc),
                                reason: "fuzzy picker Vim: Esc",
                            });
                        }
                        if input.named_key == Some(NamedKey::Enter) {
                            return Some(KeybindingMatch {
                                command: Command::PaletteVimInput(PaletteVimKey::Enter),
                                reason: "fuzzy picker Vim: Enter",
                            });
                        }
                        // Keep Ctrl+N/P and arrows working (fall through to existing handlers).
                        let is_navlike = input.modifiers.control_key()
                            || matches!(
                                input.named_key,
                                Some(NamedKey::ArrowUp | NamedKey::ArrowDown)
                            );
                        if !is_navlike {
                            if let Some(ch) = single_char(&input.text) {
                                return Some(KeybindingMatch {
                                    command: Command::PaletteVimInput(PaletteVimKey::Char(ch)),
                                    reason: "fuzzy picker Vim: char",
                                });
                            }
                        }
                    }
                }
            }
```

> `context` here is owned (`context: KeybindingContext`), so call `self.palette_vim_mode_for(&context)`.

- [ ] **Step 4: Run the test to confirm it passes**

Run: `cargo test --lib fuzzy_picker_normal_mode_routes_jk_to_vim_input`
Expected: PASS.

- [ ] **Step 5: Run the full input-map suite (regression)**

Run: `cargo test --lib input_map`
Expected: PASS — Insert-mode typing, `Ctrl+N/P`, arrows, and Enter still resolve as before.

- [ ] **Step 6: Build check**

Run: `cargo check --lib`
Expected: no errors.

- [ ] **Step 7: Commit** (ask first)

```bash
git add src/app/input_map/focus.rs src/app/input_map/tests.rs
git commit -m "feat: Vim-aware routing in fuzzy picker focus"
```

---

## Task 9: Integration, Regression, and Manual Verification

**Files:**
- Test: existing suites.
- Manual: run the app.

**Interfaces:**
- Consumes: all prior tasks.

- [ ] **Step 1: Full lib test suite**

Run: `cargo test --lib`
Expected: PASS (all Vim unit tests, all input-map tests, and the existing suite).

- [ ] **Step 2: Keymap validation (carrier command must not leak into keymaps)**

Run: `cargo test --lib default_keymap_has_no_unknown_commands`
Expected: PASS. (`PaletteVimInput` is never in a keymap, so the validator is unaffected.)

- [ ] **Step 3: Clippy**

Run: `cargo clippy --lib -- -D warnings`
Expected: no new warnings.

- [ ] **Step 4: Manual smoke test**

Run: `cargo run --release`

1. Open the command palette (e.g. file finder). Confirm it opens in Insert and typing filters as before.
2. Press `Esc` → status shows `-- NORMAL --`, caret becomes a block.
3. Press `j`/`k` → result selection moves down/up. `Ctrl+N`/`Ctrl+P` and arrows still work.
4. Press `i` → back to `-- INSERT --`, type to filter.
5. Open a paste/rename prompt (copy a file in Explorer, paste). With the name pre-filled and selected:
   - `Esc` → Normal; `0`, `$`, `w`, `b`, `e` move the caret.
   - `cw` changes a word and drops into Insert; `dw`, `x` delete; `p` pastes the register.
   - `v` + `w` + `d` deletes a Visual selection.
   - `Enter` confirms — **the file is actually created** (regression guard for bug-178).
   - `Esc` from Normal closes the popup and returns focus to the Explorer.
6. Confirm `j`/`k` in the single-line paste prompt do NOT move anything (no result list) and are ignored (they should not type `j`/`k` either).

- [ ] **Step 5: Commit final state** (ask first)

```bash
git add -A
git commit -m "feat: Vim-mode command palette (Normal/Insert/Visual, shared base)"
```

---

## Spec Coverage Checklist

| Spec Requirement | Implementing Task |
|------------------|-------------------|
| `PaletteVimMode` Insert/Normal/Visual + state fields | Task 1 |
| Word-boundary helpers (reuse algorithm) | Task 1 |
| Open in Insert; type-and-go preserved | Task 1 (open reset) + Task 7 (Insert fall-through) |
| Motions `h l w b e 0 ^ $` | Task 2 |
| Enter Insert `i a I A` | Task 2 |
| `x` delete char → register | Task 2 |
| `j/k` → list nav in pickers, ignore in prompts | Task 2 (engine) + Task 6 (`has_result_list`) + Task 8 (routing) |
| Operators `d/c/y` + motion, `cw`=`ce` | Task 3 |
| `p/P` from internal register | Task 3 |
| Visual `v` + motion-extend + `d/c/y` | Task 4 |
| Mode-aware caret (block/bar) + `-- MODE --` status | Task 5 |
| Single carrier command (no enum explosion) | Task 1 + Task 6 |
| `PaletteVimInput` not in ALL_IDS/parse/keymap | Task 1 (note) + Task 9 Step 2 |
| `Confirm` reaches existing confirm path (bug-178 guard) | Task 6 + Task 9 Step 4 |
| `Close` reuses `CloseFilePicker` | Task 6 |
| Vim routing for prompt palettes | Task 7 |
| Vim routing for list pickers (preserve Ctrl+N/P, arrows) | Task 8 |
| Glue tests (focus routing, j/k, confirm) | Tasks 7, 8, 9 |

## Placeholder Scan

No TBD/TODO. Two steps intentionally say "adapt to existing helpers" (Task 5 `TextRun` API, Task 7/8 test scaffolding) because those exact local APIs must be copied from neighboring code; each names the grep target to find the shape.

## Type Consistency Notes

- `vim_input(key, has_result_list, workspace) -> PaletteVimAction` — same signature in Tasks 2/3/4 (engine), Task 6 (`AppState::command_palette_vim_input` wraps it), and the tests.
- `PaletteVimKey { Char(char), Esc, Enter }` defined in Task 1, used identically in Tasks 2-8.
- `PaletteVimAction { Consumed, ListPrev, ListNext, Confirm, Close, Ignore }` defined in Task 1; mapped exhaustively in Task 6's handler.
- `palette_vim_mode: Option<PaletteVimMode>` on `KeybindingContext` (Task 6) is read by `palette_vim_mode_for` in Tasks 7/8.
- `handle_palette_and_open_command(&Command, usize, &Command) -> Option<bool>` — the existing entry reused in Task 6 for List/Confirm/Close.

## Execution Handoff

**Plan complete and saved to `docs/superpowers/plans/2026-06-18-vim-command-palette.md`.**

Two execution options:

1. **Subagent-Driven (recommended)** — dispatch a fresh subagent per task, review between tasks.
2. **Inline Execution** — execute tasks in this session with checkpoints.

Which approach would you like?
