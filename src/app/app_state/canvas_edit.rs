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

    pub fn is_dirty(&self) -> bool {
        self.dirty
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
}
