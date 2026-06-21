//! NetherCanvas in-card editing (v2): the focused card is edited by the full
//! editor engine via a *scoped swap*, so the main editor never leaves the
//! gc-origin file. While a card is active, its text/cursor/history live in a
//! [`CanvasEditSession`]; each editor command swaps it into `AppState`, runs,
//! and swaps it back out — `self.text` is the original file at render time.

use super::*;
use crate::canvas::BlockId;

/// The full text-edit state for the one card currently being edited. Mirrors the
/// `AppState` fields the editor engine mutates. `mode_state` is intentionally
/// **excluded** — it stays the shared global mode so the input layer routes card
/// keystrokes (Normal vs Insert) correctly while a card is active.
#[derive(Debug, Clone)]
pub struct CanvasEditSession {
    pub block: BlockId,
    pub path: PathBuf,
    text: Rope,
    active_file: Option<PathBuf>,
    cursor_char_idx: usize,
    target_col: usize,
    selection_anchor_char_idx: Option<usize>,
    visual_line_mode: bool,
    visual_block_anchor_line: Option<usize>,
    visual_block_anchor_col: Option<usize>,
    dirty: bool,
    history: EditHistory,
    current_transaction: Option<PendingTransaction>,
    target_scroll_y: f32,
    current_scroll_y: f32,
    scroll_column: usize,
    // Transient edit-affected state that the editing/motion commands mutate —
    // swapped so a card edit never corrupts the MAIN editor's folds, search
    // highlights, incremental-highlight queue, bracket-match, or yank flash.
    pending_highlight_edits: Vec<HighlightEdit>,
    search_highlights: Vec<(usize, usize)>,
    folded_ranges: Vec<(usize, usize)>,
    foldable_ranges_cache: Option<Vec<(usize, usize)>>,
    auto_folded_long_lines: Vec<usize>,
    matched_bracket_pos: Option<usize>,
    bracket_ripple_pos: Option<usize>,
    bracket_ripple_start: Option<Instant>,
    yank_flash_range: Option<(usize, usize)>,
    yank_flash_start: Option<Instant>,
    // The jump list (Ctrl-O/I) and the last search query are pushed/overwritten
    // by card-routed jump motions (gg/G/{/}/%) and find-char (f/F/t/T); swap them
    // so the main editor's jump history and search query survive a card edit.
    jump_back_stack: Vec<(PathBuf, usize, usize)>,
    jump_forward_stack: Vec<(PathBuf, usize, usize)>,
    last_search_query: String,
}

impl CanvasEditSession {
    /// Build a fresh session for `block`/`path` holding `text`, with the cursor
    /// positioned at `(line, col)` (0-based, clamped to the line's content).
    pub fn new(block: BlockId, path: PathBuf, text: Rope, line: usize, col: usize) -> Self {
        let total = text.len_lines().max(1);
        let l = line.min(total.saturating_sub(1));
        let line_start = text.line_to_char(l);
        // Content length excludes the trailing newline so the cursor can't land
        // past the line end.
        let content_len = text.line(l).len_chars().saturating_sub(
            if text.line(l).to_string().ends_with('\n') { 1 } else { 0 },
        );
        let clamped_col = col.min(content_len);
        let cursor = (line_start + clamped_col).min(text.len_chars());
        Self {
            block,
            active_file: Some(path.clone()),
            path,
            text,
            cursor_char_idx: cursor,
            target_col: clamped_col,
            selection_anchor_char_idx: None,
            visual_line_mode: false,
            visual_block_anchor_line: None,
            visual_block_anchor_col: None,
            dirty: false,
            history: EditHistory::new(),
            current_transaction: None,
            target_scroll_y: 0.0,
            current_scroll_y: 0.0,
            scroll_column: 0,
            pending_highlight_edits: Vec::new(),
            search_highlights: Vec::new(),
            folded_ranges: Vec::new(),
            foldable_ranges_cache: None,
            auto_folded_long_lines: Vec::new(),
            matched_bracket_pos: None,
            bracket_ripple_pos: None,
            bracket_ripple_start: None,
            yank_flash_range: None,
            yank_flash_start: None,
            jump_back_stack: Vec::new(),
            jump_forward_stack: Vec::new(),
            last_search_query: String::new(),
        }
    }

}

impl AppState {
    /// Exchange the card session's text-edit state with `self`'s. Calling it once
    /// swaps the card **in** (the engine now edits the card); calling it again
    /// swaps it **out** (engine back on the main editor). `mode_state` is NOT
    /// swapped — the card borrows the shared global mode while EditCard is active.
    pub(crate) fn swap_canvas_edit_session(&mut self, s: &mut CanvasEditSession) {
        std::mem::swap(&mut self.text, &mut s.text);
        std::mem::swap(&mut self.active_file, &mut s.active_file);
        std::mem::swap(&mut self.cursor_char_idx, &mut s.cursor_char_idx);
        std::mem::swap(&mut self.target_col, &mut s.target_col);
        std::mem::swap(
            &mut self.selection_anchor_char_idx,
            &mut s.selection_anchor_char_idx,
        );
        std::mem::swap(&mut self.visual_line_mode, &mut s.visual_line_mode);
        std::mem::swap(
            &mut self.visual_block_anchor_line,
            &mut s.visual_block_anchor_line,
        );
        std::mem::swap(
            &mut self.visual_block_anchor_col,
            &mut s.visual_block_anchor_col,
        );
        std::mem::swap(&mut self.dirty, &mut s.dirty);
        std::mem::swap(&mut self.history, &mut s.history);
        std::mem::swap(&mut self.current_transaction, &mut s.current_transaction);
        std::mem::swap(&mut self.target_scroll_y, &mut s.target_scroll_y);
        std::mem::swap(&mut self.current_scroll_y, &mut s.current_scroll_y);
        std::mem::swap(&mut self.scroll_column, &mut s.scroll_column);
        std::mem::swap(&mut self.pending_highlight_edits, &mut s.pending_highlight_edits);
        std::mem::swap(&mut self.search_highlights, &mut s.search_highlights);
        std::mem::swap(&mut self.folded_ranges, &mut s.folded_ranges);
        std::mem::swap(&mut self.foldable_ranges_cache, &mut s.foldable_ranges_cache);
        std::mem::swap(&mut self.auto_folded_long_lines, &mut s.auto_folded_long_lines);
        std::mem::swap(&mut self.matched_bracket_pos, &mut s.matched_bracket_pos);
        std::mem::swap(&mut self.bracket_ripple_pos, &mut s.bracket_ripple_pos);
        std::mem::swap(&mut self.bracket_ripple_start, &mut s.bracket_ripple_start);
        std::mem::swap(&mut self.yank_flash_range, &mut s.yank_flash_range);
        std::mem::swap(&mut self.yank_flash_start, &mut s.yank_flash_start);
        std::mem::swap(&mut self.jump_back_stack, &mut s.jump_back_stack);
        std::mem::swap(&mut self.jump_forward_stack, &mut s.jump_forward_stack);
        std::mem::swap(&mut self.last_search_query, &mut s.last_search_query);
        // The line-start cache is keyed to `self.text`; invalidate on every swap.
        self.cached_line_starts = None;
        self.bump_revision();
    }

    pub(crate) fn take_canvas_edit_session(&mut self) -> Option<CanvasEditSession> {
        self.canvas_edit_session.take()
    }

    pub(crate) fn put_canvas_edit_session(&mut self, s: CanvasEditSession) {
        self.canvas_edit_session = Some(s);
    }

    /// The block id of the active in-card edit session, if any.
    pub fn canvas_edit_session_block(&self) -> Option<BlockId> {
        self.canvas_edit_session.as_ref().map(|s| s.block)
    }

    /// Clamp the **swapped-in** card cursor (`self.cursor_char_idx`/`self.text`
    /// transiently hold the card during a card-routed command) into the card's
    /// resolved scope so a motion can't park the caret outside the enclosing
    /// function. No-op without a scope (whole-file fallback) or when already in
    /// range. Returns whether the cursor moved. Called after card MOTION
    /// commands only (see [`Command::is_card_motion_command`]).
    pub(crate) fn canvas_clamp_active_cursor_to_scope(&mut self, block_id: BlockId) -> bool {
        let Some((lo, hi)) = self
            .canvas
            .as_ref()
            .and_then(|c| c.block(block_id))
            .and_then(|b| b.scope_lines)
        else {
            return false;
        };
        let clamped =
            clamp_char_to_line_range(&self.text, lo as usize, hi as usize, self.cursor_char_idx);
        if clamped == self.cursor_char_idx {
            return false;
        }
        self.cursor_char_idx = clamped;
        let line = self.text.char_to_line(clamped);
        self.target_col = clamped - self.text.line_to_char(line);
        true
    }

    /// The file path of the active in-card edit session, if any.
    pub(crate) fn canvas_edit_session_path(&self) -> Option<PathBuf> {
        self.canvas_edit_session.as_ref().map(|s| s.path.clone())
    }

    /// The `(line, col)` (0-based) of the active session's cursor — used to
    /// submit `gd`/`gr` from the card cursor so results spawn new cards.
    pub(crate) fn canvas_edit_session_cursor(&self) -> Option<(usize, usize)> {
        let s = self.canvas_edit_session.as_ref()?;
        let line = s.text.char_to_line(s.cursor_char_idx.min(s.text.len_chars()));
        let col = s.cursor_char_idx.saturating_sub(s.text.line_to_char(line));
        Some((line, col))
    }

    /// The active session's full text — the `didOpen`/`didChange` payload that
    /// registers the card file with the LSP server (Phase 1 in-card LSP).
    pub(crate) fn canvas_edit_session_text(&self) -> Option<String> {
        self.canvas_edit_session.as_ref().map(|s| s.text.to_string())
    }

    /// Completion context at the session cursor: `(cursor_line, cursor_col,
    /// prefix_start_col, typed_prefix)` (all 0-based). The prefix is the run of
    /// identifier characters (`alphanumeric`/`_`) immediately before the cursor —
    /// what the LSP completes (Phase 3 in-card LSP).
    pub(crate) fn canvas_edit_session_completion_context(
        &self,
    ) -> Option<(usize, usize, usize, String)> {
        let s = self.canvas_edit_session.as_ref()?;
        let cursor = s.cursor_char_idx.min(s.text.len_chars());
        let line = s.text.char_to_line(cursor);
        let line_start = s.text.line_to_char(line);
        let col = cursor - line_start;
        let chars: Vec<char> = s.text.line(line).chars().collect();
        let end = col.min(chars.len());
        let mut start = end;
        while start > 0 {
            let c = chars[start - 1];
            if c.is_alphanumeric() || c == '_' {
                start -= 1;
            } else {
                break;
            }
        }
        let prefix: String = chars[start..end].iter().collect();
        Some((line, col, start, prefix))
    }

    /// The card file we must register with the LSP ourselves while editing it:
    /// `Some(path)` iff a session is active AND its file is **neither** the main
    /// editor's `active_file` **nor** an already-open text buffer. `None` when the
    /// document is already open elsewhere — re-`didOpen`'ing it would push the
    /// card's unsaved text over the main editor's view, and a later `didClose`
    /// would yank a document the main editor still needs. (Phase 1 in-card LSP:
    /// only files *we* open get the didOpen/didChange/didClose lifecycle.)
    pub(crate) fn canvas_card_lsp_target(&self) -> Option<PathBuf> {
        let path = self.canvas_edit_session.as_ref().map(|s| s.path.clone())?;
        let matches = |p: &std::path::Path| crate::app::app_state::overlays::path_matches(p, &path);
        if self.active_file().is_some_and(matches) {
            return None;
        }
        let already_open_buffer = self.buffers.iter().any(|entry| {
            matches!(&entry.content, BufferContent::Text(buf) if matches(&buf.path))
        });
        if already_open_buffer {
            return None;
        }
        Some(path)
    }

    /// ⌘S inside a card: write the session text to its file and clear the
    /// session's dirty flag. Handled specially (NOT via the generic save path)
    /// because the card is not a `self.buffers` slot — routing it through
    /// `save_file` would write card text into the *original* buffer's slot
    /// (`active_buffer_index` points at the main editor). If the card file is
    /// also an open buffer/tab, its in-memory copy is kept in sync. Returns
    /// whether a save happened.
    pub(crate) fn canvas_save_edit_session(&mut self) -> bool {
        let Some(session) = self.canvas_edit_session.as_mut() else {
            return false;
        };
        let path = session.path.clone();
        let content = session.text.to_string();
        if std::fs::write(&path, &content).is_err() {
            return false;
        }
        session.dirty = false;
        let saved = session.text.clone();
        for entry in self.buffers.iter_mut() {
            if let BufferContent::Text(ref mut buf) = entry.content
                && crate::app::app_state::overlays::path_matches(&buf.path, &path)
            {
                buf.in_memory_text = Some(saved.clone());
                buf.dirty = false;
            }
        }
        // If the card IS the main editor's active file, sync the live buffer so
        // the main editor reflects the saved content (and isn't left dirty with
        // stale text that would clobber the save on its own next ⌘S).
        if self
            .active_file()
            .is_some_and(|a| crate::app::app_state::overlays::path_matches(a, &path))
        {
            self.text = saved;
            self.dirty = false;
            self.cached_line_starts = None;
            self.bump_revision();
        }
        true
    }

    /// Whether `block` currently has a stashed (dirty) edit session — used to
    /// resume unsaved edits when re-entering the same card.
    pub(crate) fn canvas_edit_session_is_for(&self, block: BlockId) -> bool {
        self.canvas_edit_session
            .as_ref()
            .is_some_and(|s| s.block == block)
    }

    /// A windowed view of the **session** text around its cursor: `(lines,
    /// window_start_0based, cursor_line_0based, cursor_col)`. Drives the live
    /// in-card render and caret directly from the session (never the main
    /// buffer). Returns `None` when there is no active session.
    pub(crate) fn canvas_edit_session_window(
        &self,
        context: usize,
    ) -> Option<(Vec<String>, usize, usize, usize)> {
        let s = self.canvas_edit_session.as_ref()?;
        let total = s.text.len_lines().max(1);
        let cursor_line = s.text.char_to_line(s.cursor_char_idx.min(s.text.len_chars()));
        let line_start = s.text.line_to_char(cursor_line);
        let cursor_col = s.cursor_char_idx.saturating_sub(line_start);
        let start = cursor_line.saturating_sub(context);
        let end = (cursor_line + context + 1).min(total);
        let lines = (start..end)
            .map(|l| {
                let a = s.text.line_to_char(l);
                let b = s.text.line_to_char((l + 1).min(total));
                s.text.slice(a..b).to_string().trim_end_matches('\n').to_string()
            })
            .collect();
        Some((lines, start, cursor_line, cursor_col))
    }

    /// The number of code rows the edit-target card currently shows (its snapshot
    /// line count) — the "page" size for in-card `Ctrl-d`/`Ctrl-u` half-page
    /// scrolling. `None` when no card is being edited.
    pub(crate) fn canvas_edit_card_visible_lines(&self) -> Option<usize> {
        let block_id = self.canvas_edit_session_block()?;
        let state = self.canvas.as_ref()?;
        let block = state.block(block_id)?;
        Some(block.snapshot.text.split('\n').count().max(1))
    }

    /// The live visual selection inside the edit-target card, computed from the
    /// **session** (the main editor's `self.*` is the origin file at render time,
    /// so its `visual_selection_range` would describe the wrong buffer). `None`
    /// outside Visual / Visual-block mode or with no anchor. Drives the card's
    /// selection bands so `v` highlights like the main editor.
    pub(crate) fn canvas_edit_session_selection(&self) -> Option<crate::canvas::CardSelection> {
        let s = self.canvas_edit_session.as_ref()?;
        if !matches!(
            self.current_mode(),
            crate::core::mode::EditorMode::Visual | crate::core::mode::EditorMode::VisualBlock
        ) {
            return None;
        }
        let anchor = s.selection_anchor_char_idx?;
        compute_card_selection(&s.text, anchor, s.cursor_char_idx, s.visual_line_mode)
    }

    /// Visible edit-card text while editing. The card is a vim-style mini-editor
    /// viewport BOUNDED to the card's scope (the enclosing function/definition):
    /// it shows only the scope's lines and **scrolls within that scope** to follow
    /// the cursor ([`follow_window_start`]) — it never pulls in lines outside the
    /// function. The visible row count is stable (`min(scope_size, CARD_MAX_LINES)`)
    /// so the card height doesn't jump while scrolling; on Enter the cursor is
    /// inside the scope so the window stays pinned to the scope top (no jump).
    /// Unscoped cards (scope not yet resolved) fall back to a whole-file window.
    pub(crate) fn canvas_edit_session_snapshot_window(
        &self,
        block_id: BlockId,
    ) -> Option<(Vec<String>, usize, usize, usize)> {
        let s = self.canvas_edit_session.as_ref()?;
        let state = self.canvas.as_ref()?;
        let block = state.block(block_id)?;
        let total = s.text.len_lines().max(1);
        let cursor_line = s.text.char_to_line(s.cursor_char_idx.min(s.text.len_chars()));
        let line_start = s.text.line_to_char(cursor_line);
        let cursor_col = s.cursor_char_idx.saturating_sub(line_start);

        // The scroll bounds + visible row count come from the resolved scope; an
        // unscoped card falls back to the ±N snapshot size over the whole file.
        let last = total.saturating_sub(1);
        let (lo, hi, count) = match block.scope_lines {
            Some((a, b)) => {
                let a = (a as usize).min(last);
                let b = (b as usize).min(last).max(a);
                let scope_size = b - a + 1;
                // The visible-row count follows the card's HEIGHT (`=`/`-`): an
                // explicit `height_rows` caps how many scope rows show at once (so
                // a shorter card scrolls within the scope); unset → auto-fit to the
                // whole scope, capped at `CARD_MAX_LINES`.
                let rows = match block.height_rows {
                    Some(r) => r.clamp(1, scope_size),
                    None => scope_size.min(crate::canvas::CARD_MAX_LINES),
                };
                (a, b, rows)
            }
            None => {
                let cnt = block.snapshot.text.split('\n').count().max(1);
                (0usize, last, cnt)
            }
        };
        let prev_start = block.snapshot.start_line.saturating_sub(1) as usize;
        let start = follow_window_start(prev_start, count, cursor_line, lo, hi);
        if start >= total {
            return None;
        }
        // Never read past the scope end (hi) or EOF.
        let end = (start + count).min(hi + 1).min(total);
        let lines = (start..end)
            .map(|l| {
                let a = s.text.line_to_char(l);
                let b = s.text.line_to_char((l + 1).min(total));
                s.text.slice(a..b).to_string().trim_end_matches('\n').to_string()
            })
            .collect();
        Some((lines, start, cursor_line, cursor_col))
    }
}

/// Viewport-follow scroll for an in-card edit window, BOUNDED to a scope range
/// `[lo, hi]` (0-based inclusive — the enclosing function/scope). Given the
/// previous window `prev_start`, the `count` visible rows and the absolute
/// `cursor_line`, return the new window start so the cursor stays visible WITHOUT
/// ever scrolling outside `[lo, hi]` (the card never pulls in lines beyond its
/// scope). Vim-like: only scrolls when the cursor would leave `[start, start+count)`.
/// For a whole-file (unscoped) window pass `lo = 0`, `hi = total - 1`.
pub(crate) fn follow_window_start(
    prev_start: usize,
    count: usize,
    cursor_line: usize,
    lo: usize,
    hi: usize,
) -> usize {
    let count = count.max(1);
    // Clamp the cursor into the scope for follow purposes: an edit just past the
    // scope end keeps the last scope rows visible rather than scrolling out.
    let cur = cursor_line.clamp(lo, hi);
    let mut start = prev_start.max(lo);
    if cur < start {
        start = cur;
    } else if cur >= start + count {
        start = cur + 1 - count;
    }
    // Never scroll past the scope's end; never above its start.
    let max_start = (hi + 1).saturating_sub(count).max(lo);
    start.clamp(lo, max_start)
}

/// Clamp `cursor` (a char index into `text`) into the line range `[lo, hi]`
/// (0-based, inclusive). The lower bound is the first char of line `lo`; the
/// upper bound is the last selectable char of line `hi` (its trailing newline is
/// excluded, unless `hi` is the final line). Bounds are themselves clamped to the
/// buffer so a stale scope range can't index past the end. Pure — drives the
/// in-card scope cursor clamp ([`AppState::canvas_clamp_active_cursor_to_scope`]).
pub(crate) fn clamp_char_to_line_range(text: &Rope, lo: usize, hi: usize, cursor: usize) -> usize {
    let total = text.len_lines();
    if total == 0 {
        return cursor;
    }
    let last = total - 1;
    let lo = lo.min(last);
    let hi = hi.min(last).max(lo);
    let lo_char = text.line_to_char(lo);
    let hi_next = text.line_to_char((hi + 1).min(total));
    let hi_char = hi_next
        .saturating_sub(if hi + 1 < total { 1 } else { 0 })
        .max(lo_char);
    cursor.clamp(lo_char, hi_char)
}

/// Convert a `(anchor, cursor)` char-index pair into a renderable [`CardSelection`]
/// (absolute 0-based lines; `end_col` exclusive). Mirrors the main editor's
/// `visual_selection_range` charwise/linewise math so in-card `v` selects the same
/// span. `None` for an empty buffer or a zero-width selection.
pub(crate) fn compute_card_selection(
    text: &Rope,
    anchor: usize,
    cursor: usize,
    line_mode: bool,
) -> Option<crate::canvas::CardSelection> {
    let len = text.len_chars();
    if len == 0 {
        return None;
    }
    let anchor_idx = anchor.min(len.saturating_sub(1));
    let focus_idx = cursor.min(len.saturating_sub(1));
    let (start_char, end_char) = if line_mode {
        let first = text.char_to_line(anchor_idx.min(focus_idx));
        let last = text.char_to_line(anchor_idx.max(focus_idx));
        let sc = text.line_to_char(first);
        let ec = if last + 1 < text.len_lines() {
            text.line_to_char(last + 1)
        } else {
            len
        };
        (sc, ec)
    } else {
        let sc = anchor_idx.min(focus_idx);
        let ec = anchor_idx.max(focus_idx).saturating_add(1).min(len);
        (sc, ec)
    };
    if start_char >= end_char {
        return None;
    }
    let start_line = text.char_to_line(start_char);
    let end_line = text.char_to_line(end_char.saturating_sub(1));
    let start_col = start_char - text.line_to_char(start_line);
    let end_col = end_char - text.line_to_char(end_line);
    Some(crate::canvas::CardSelection {
        start_line: start_line as u32,
        start_col: start_col as u32,
        end_line: end_line as u32,
        end_col: end_col as u32,
        line_mode,
    })
}

#[cfg(test)]
mod card_selection_tests {
    use super::compute_card_selection;
    use ropey::Rope;

    #[test]
    fn charwise_same_line() {
        // "hello\nworld" — select chars [0..3] on line 0 (anchor 0, cursor 2).
        let t = Rope::from_str("hello\nworld\n");
        let sel = compute_card_selection(&t, 0, 2, false).unwrap();
        assert_eq!((sel.start_line, sel.start_col), (0, 0));
        assert_eq!((sel.end_line, sel.end_col), (0, 3)); // end exclusive (incl. cursor)
        assert!(!sel.line_mode);
    }

    #[test]
    fn charwise_spans_lines_and_is_anchor_order_agnostic() {
        let t = Rope::from_str("hello\nworld\n");
        // cursor before anchor → still normalized start<end.
        let sel = compute_card_selection(&t, 8, 1, false).unwrap();
        assert_eq!((sel.start_line, sel.start_col), (0, 1));
        assert_eq!((sel.end_line, sel.end_col), (1, 3)); // char 8 = 'r' (col 2), +1 → 3
    }

    #[test]
    fn line_mode_covers_full_lines() {
        let t = Rope::from_str("aaa\nbbb\nccc\n");
        let sel = compute_card_selection(&t, 1, 5, true).unwrap();
        assert_eq!(sel.start_line, 0);
        assert_eq!(sel.end_line, 1);
        assert!(sel.line_mode);
    }

    #[test]
    fn empty_buffer_is_none() {
        let t = Rope::from_str("");
        assert!(compute_card_selection(&t, 0, 0, false).is_none());
    }
}

#[cfg(test)]
mod clamp_scope_tests {
    use super::clamp_char_to_line_range;
    use ropey::Rope;

    #[test]
    fn keeps_cursor_inside_scope() {
        let t = Rope::from_str("a\nbb\nccc\ndddd\ne\n");
        // scope = lines 1..=2 ("bb","ccc"). char range [2, 7] ('b'=2.. 'c' last=7).
        assert_eq!(clamp_char_to_line_range(&t, 1, 2, 4), 4); // in range → unchanged
    }

    #[test]
    fn pulls_back_above_and_below_scope() {
        let t = Rope::from_str("a\nbb\nccc\ndddd\ne\n");
        // above scope (cursor on line 0, char 0) → clamped to first char of line 1.
        assert_eq!(clamp_char_to_line_range(&t, 1, 2, 0), 2);
        // below scope (cursor on line 3 'dddd' start, char 8) → last char of line 2.
        // line 2 "ccc": line_to_char(2)=5, line_to_char(3)=9, minus newline → 8.
        assert_eq!(clamp_char_to_line_range(&t, 1, 2, 12), 8);
    }

    #[test]
    fn final_line_stays_on_its_line() {
        let t = Rope::from_str("a\nbb\nccc"); // no trailing newline; last line = 2
        // Scope is the whole last line; a cursor past EOF clamps to the buffer end,
        // which still resolves to line 2 (never spills past the scope).
        let clamped = clamp_char_to_line_range(&t, 2, 2, 99);
        assert_eq!(clamped, t.len_chars());
        assert_eq!(t.char_to_line(clamped), 2, "clamp must keep cursor on line hi");
    }

    #[test]
    fn out_of_range_scope_is_clamped_to_buffer() {
        let t = Rope::from_str("a\nbb\n");
        // hi beyond the buffer must not panic; clamps into existing lines.
        let _ = clamp_char_to_line_range(&t, 0, 99, 1);
    }
}

#[cfg(test)]
mod follow_window_tests {
    use super::follow_window_start;

    // Whole-file behaviour: lo = 0, hi = total - 1.
    #[test]
    fn stays_put_when_cursor_already_visible() {
        assert_eq!(follow_window_start(5, 3, 6, 0, 99), 5);
        assert_eq!(follow_window_start(5, 3, 5, 0, 99), 5);
        assert_eq!(follow_window_start(5, 3, 7, 0, 99), 5);
    }

    #[test]
    fn scrolls_down_to_keep_cursor_on_last_row() {
        assert_eq!(follow_window_start(5, 3, 20, 0, 99), 18);
    }

    #[test]
    fn scrolls_up_to_keep_cursor_on_first_row() {
        assert_eq!(follow_window_start(18, 3, 4, 0, 99), 4);
    }

    #[test]
    fn clamps_to_file_end_without_phantom_rows() {
        assert_eq!(follow_window_start(4, 4, 9, 0, 9), 6);
        assert_eq!(follow_window_start(50, 4, 9, 0, 9), 6);
    }

    // Scope-bounded behaviour: the window never leaves [lo, hi].
    #[test]
    fn never_scrolls_above_scope_start() {
        assert_eq!(follow_window_start(10, 5, 10, 10, 40), 10);
        // A stale prev_start below the scope is pulled up to lo.
        assert_eq!(follow_window_start(3, 5, 12, 10, 40), 10);
    }

    #[test]
    fn never_scrolls_below_scope_end() {
        // Scope [10..40] (31 lines), count 5 → max start = 40+1-5 = 36.
        assert_eq!(follow_window_start(34, 5, 40, 10, 40), 36);
        // Cursor past the scope end clamps in → still bounded to 36.
        assert_eq!(follow_window_start(34, 5, 99, 10, 40), 36);
    }

    #[test]
    fn scrolls_within_scope_to_follow_cursor() {
        assert_eq!(follow_window_start(10, 5, 30, 10, 40), 26);
    }

    #[test]
    fn scope_smaller_than_window_pins_to_lo() {
        assert_eq!(follow_window_start(10, 5, 12, 10, 12), 10);
    }
}
