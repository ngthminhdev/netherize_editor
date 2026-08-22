use super::{
    AppState, VirtualCursor, VisualSelectionRange, overlays::WordClass, overlays::classify_char,
};
use crate::core::mode::{EditorMode, ModeEvent};

impl AppState {
    // ── Public API ────────────────────────────────────────────────────────────

    /// Returns all virtual cursors (read-only, used by the renderer).
    pub fn virtual_cursors(&self) -> &[VirtualCursor] {
        &self.virtual_cursors
    }

    /// Returns selection ranges for all cursors in MultiCursor mode (primary + virtual).
    /// Used by the renderer to draw per-match highlight backgrounds.
    pub fn multi_cursor_selection_ranges(&self) -> Vec<VisualSelectionRange> {
        if !matches!(
            self.current_mode(),
            EditorMode::MultiCursor | EditorMode::MultiInsert
        ) {
            return Vec::new();
        }
        let len_chars = self.text.len_chars();
        if len_chars == 0 {
            return Vec::new();
        }

        let mut ranges = Vec::new();

        if let Some(anchor) = self.selection_anchor_char_idx {
            let sc = anchor.min(self.cursor_char_idx).min(len_chars);
            let ec = anchor
                .max(self.cursor_char_idx)
                .saturating_add(1)
                .min(len_chars);
            if let Some(r) = self.char_range_to_vsr(sc, ec) {
                ranges.push(r);
            }
        }

        for vc in &self.virtual_cursors {
            if let (Some(ss), Some(se)) = (vc.selection_start, vc.selection_end) {
                let sc = ss.min(len_chars);
                let ec = se.min(len_chars);
                if let Some(r) = self.char_range_to_vsr(sc, ec) {
                    ranges.push(r);
                }
            }
        }

        ranges
    }

    fn char_range_to_vsr(
        &self,
        start_char: usize,
        end_char: usize,
    ) -> Option<VisualSelectionRange> {
        if start_char >= end_char {
            return None;
        }
        let start_line = self.text.char_to_line(start_char);
        let end_line = self.text.char_to_line(end_char.saturating_sub(1));
        let start_byte = self.text.char_to_byte(start_char);
        let end_byte = self.text.char_to_byte(end_char);
        let start_byte_in_line = start_byte.saturating_sub(self.text.line_to_byte(start_line));
        let end_byte_in_line = end_byte.saturating_sub(self.text.line_to_byte(end_line));
        Some(VisualSelectionRange {
            start_char,
            end_char,
            start_line,
            end_line,
            start_byte_in_line,
            end_byte_in_line,
        })
    }

    /// Clear all virtual cursors and reset multi-cursor search state.
    pub fn clear_virtual_cursors(&mut self) {
        self.virtual_cursors.clear();
        self.mc_search_word = None;
        self.mc_search_start = 0;
        self.mc_whole_word = true;
        self.selection_anchor_char_idx = None;
    }

    /// Visual + Ctrl+n — find ALL occurrences of the visually-selected text in
    /// the buffer and place a cursor on each one simultaneously.  Unlike
    /// `multi_cursor_add_next` which adds matches one-by-one, this enters
    /// MultiCursor mode with every match already selected.
    pub fn multi_cursor_select_all_visual(&mut self) -> bool {
        if self.current_mode() != EditorMode::Visual {
            return false;
        }
        let len_chars = self.text.len_chars();
        if len_chars == 0 {
            return false;
        }

        let Some(sel) = self.visual_selection_range() else {
            return false;
        };
        let sel_text: String = self.text.slice(sel.start_char..sel.end_char).to_string();
        if sel_text.is_empty() {
            return false;
        }

        let text_string = self.text.to_string();

        // Collect every occurrence (simple substring, no whole-word requirement — the
        // user already made an explicit selection).
        let mut all_matches: Vec<(usize, usize)> = Vec::new();
        let mut from_byte = 0usize;
        while from_byte < text_string.len() {
            let Some(rel) = text_string[from_byte..].find(&sel_text) else {
                break;
            };
            let abs_start = from_byte + rel;
            let char_start = text_string[..abs_start].chars().count();
            let char_end = char_start + sel_text.chars().count();
            all_matches.push((char_start, char_end));
            from_byte = abs_start + sel_text.len().max(1);
        }

        if all_matches.is_empty() {
            return false;
        }

        // Transition to MultiCursor before changing any cursor state.
        // Direct mode_state.apply bypasses apply_mode_event's gv capture — do it here.
        self.capture_last_visual_selection();
        if self.mode_state.apply(ModeEvent::EnterMultiCursor).is_err() {
            return false;
        }

        // The original visual selection becomes the primary cursor.
        self.selection_anchor_char_idx = Some(sel.start_char);
        self.cursor_char_idx = sel
            .end_char
            .saturating_sub(1)
            .min(len_chars.saturating_sub(1));
        let (_, col) = self.cursor_line_col();
        self.target_col = col;

        // All other matches become virtual cursors.
        self.virtual_cursors.clear();
        for (fs, fe) in &all_matches {
            if *fs == sel.start_char && *fe == sel.end_char {
                continue; // already the primary
            }
            self.virtual_cursors.push(VirtualCursor {
                char_idx: fe.saturating_sub(1).min(len_chars.saturating_sub(1)),
                selection_start: Some(*fs),
                selection_end: Some(*fe),
            });
        }

        self.mc_search_word = Some(sel_text);
        self.mc_search_start = sel.end_char;
        self.merge_overlapping_cursors();
        self.bump_revision();
        true
    }

    /// Ctrl+n — select word under cursor (first call) or add next match
    /// (subsequent calls).  Enters MultiCursor mode on the first call.
    pub fn multi_cursor_add_next(&mut self) -> bool {
        let len_chars = self.text.len_chars();
        if len_chars == 0 {
            return false;
        }

        // ── First call: seed word from Visual selection or word under cursor ─────
        if self.mc_search_word.is_none() {
            let (word, ws, we) = if self.current_mode() == EditorMode::Visual {
                // Use the visually selected text as the search word.
                let Some(sel) = self.visual_selection_range() else {
                    return false;
                };
                let text: String = self.text.slice(sel.start_char..sel.end_char).to_string();
                if text.is_empty() {
                    return false;
                }
                (text, sel.start_char, sel.end_char)
            } else {
                let Some(w) = self.word_under_cursor() else {
                    return false;
                };
                let Some((ws, we)) = self.word_bounds_at_cursor() else {
                    return false;
                };
                (w, ws, we)
            };

            let whole_word = self.current_mode() != EditorMode::Visual;
            // Direct mode_state.apply bypasses apply_mode_event's gv capture — do it here.
            self.capture_last_visual_selection();
            if self.mode_state.apply(ModeEvent::EnterMultiCursor).is_err() {
                return false;
            }
            self.selection_anchor_char_idx = Some(ws);
            self.cursor_char_idx = we.saturating_sub(1).min(len_chars.saturating_sub(1));
            let (_, col) = self.cursor_line_col();
            self.target_col = col;

            self.mc_search_word = Some(word);
            self.mc_search_start = we;
            self.mc_whole_word = whole_word;
            self.bump_revision();
            return true;
        }

        // ── Subsequent calls: add next match ──────────────────────────────────
        let word = self.mc_search_word.clone().unwrap();
        let text_string = self.text.to_string();

        let found = if self.mc_whole_word {
            find_whole_word(&text_string, &word, self.mc_search_start)
                .or_else(|| find_whole_word(&text_string, &word, 0))
        } else {
            find_substring(&text_string, &word, self.mc_search_start)
                .or_else(|| find_substring(&text_string, &word, 0))
        };

        let Some((fs, fe)) = found else {
            return false;
        };

        // Avoid re-adding the primary selection or a duplicate match.
        if Some(fs) == self.selection_anchor_char_idx {
            return false;
        }
        if self
            .virtual_cursors
            .iter()
            .any(|vc| vc.selection_start == Some(fs))
        {
            return false;
        }

        self.virtual_cursors.push(VirtualCursor {
            char_idx: fe.saturating_sub(1).min(len_chars.saturating_sub(1)),
            selection_start: Some(fs),
            selection_end: Some(fe),
        });
        self.mc_search_start = fe;
        self.merge_overlapping_cursors();
        self.bump_revision();
        true
    }

    /// Alt+Click — add a bare caret at `char_idx` without word matching.
    /// Enters MultiCursor mode on the first click.
    pub fn add_cursor_at(&mut self, char_idx: usize) -> bool {
        let len_chars = self.text.len_chars();
        if len_chars == 0 {
            return false;
        }
        if !matches!(
            self.mode_state.current(),
            EditorMode::Normal | EditorMode::MultiCursor
        ) {
            return false;
        }
        if self.mode_state.current() == EditorMode::Normal
            && self.mode_state.apply(ModeEvent::EnterMultiCursor).is_err()
        {
            return false;
        }
        let pos = char_idx.min(len_chars);
        if pos == self.cursor_char_idx || self.virtual_cursors.iter().any(|vc| vc.char_idx == pos) {
            return true; // idempotent — cursor already there
        }
        self.virtual_cursors.push(VirtualCursor {
            char_idx: pos,
            selection_start: None,
            selection_end: None,
        });
        self.bump_revision();
        true
    }

    /// q — skip the most recently added match and jump to the next one.
    pub fn multi_cursor_skip(&mut self) -> bool {
        let len_chars = self.text.len_chars();
        if len_chars == 0 || self.mc_search_word.is_none() {
            return false;
        }

        // Remove the last added virtual cursor (the one to skip).
        let last_vc = self.virtual_cursors.pop();
        let search_from = last_vc
            .and_then(|vc| vc.selection_end)
            .unwrap_or(self.mc_search_start);

        let word = self.mc_search_word.clone().unwrap();
        let text_string = self.text.to_string();
        let found = if self.mc_whole_word {
            find_whole_word(&text_string, &word, search_from)
                .or_else(|| find_whole_word(&text_string, &word, 0))
        } else {
            find_substring(&text_string, &word, search_from)
                .or_else(|| find_substring(&text_string, &word, 0))
        };

        let Some((fs, fe)) = found else {
            return false;
        };

        if Some(fs) == self.selection_anchor_char_idx {
            return false;
        }

        self.virtual_cursors.push(VirtualCursor {
            char_idx: fe.saturating_sub(1).min(len_chars.saturating_sub(1)),
            selection_start: Some(fs),
            selection_end: Some(fe),
        });
        self.mc_search_start = fe;
        self.bump_revision();
        true
    }

    /// I — move all cursors to the *start* of their selection, enter MultiInsert.
    pub fn multi_cursor_insert_before(&mut self) -> bool {
        if !self.is_multi_cursor_mode() {
            return false;
        }
        let len_chars = self.text.len_chars();

        // Move primary cursor to start of primary selection.
        if let Some(anchor) = self.selection_anchor_char_idx {
            let start = anchor.min(self.cursor_char_idx);
            self.cursor_char_idx = start.min(len_chars);
        }
        self.selection_anchor_char_idx = None;

        // Move each virtual cursor to its selection start.
        for vc in &mut self.virtual_cursors {
            if let Some(ss) = vc.selection_start {
                vc.char_idx = ss.min(len_chars);
            }
            vc.selection_start = None;
            vc.selection_end = None;
        }

        let (_, col) = self.cursor_line_col();
        self.target_col = col;
        let _ = self.mode_state.apply(ModeEvent::EnterMultiInsert);
        self.bump_revision();
        true
    }

    /// A — move all cursors to the *end* of their selection, enter MultiInsert.
    pub fn multi_cursor_append_after(&mut self) -> bool {
        if !self.is_multi_cursor_mode() {
            return false;
        }
        let len_chars = self.text.len_chars();

        // Move primary cursor to end of primary selection.
        if let Some(anchor) = self.selection_anchor_char_idx {
            let end = anchor.max(self.cursor_char_idx) + 1;
            self.cursor_char_idx = end.min(len_chars);
        }
        self.selection_anchor_char_idx = None;

        // Move each virtual cursor to its selection end.
        for vc in &mut self.virtual_cursors {
            if let Some(se) = vc.selection_end {
                vc.char_idx = se.min(len_chars);
            }
            vc.selection_start = None;
            vc.selection_end = None;
        }

        let (_, col) = self.cursor_line_col();
        self.target_col = col;
        let _ = self.mode_state.apply(ModeEvent::EnterMultiInsert);
        self.bump_revision();
        true
    }

    /// I in VisualBlock — insert at start of block on each line.
    pub fn visual_block_insert_before(&mut self) -> bool {
        if self.current_mode() != EditorMode::VisualBlock {
            return false;
        }
        let Some(block) = self.visual_block_range() else {
            return false;
        };

        self.ensure_current_transaction();

        // Generate cursors for each line in block
        self.virtual_cursors.clear();
        let mut primary_set = false;

        for line_idx in block.start_line..=block.end_line {
            if line_idx >= self.text.len_lines() {
                break;
            }

            let line_start_char = self.text.line_to_char(line_idx);
            let line_content = self.text.line(line_idx);

            // Calculate target column position
            let target_col = block.start_col;
            let mut char_idx = line_start_char;
            let mut current_col = 0;

            // Walk to target column or line end
            for ch in line_content.chars() {
                if ch == '\n' || current_col >= target_col {
                    break;
                }
                char_idx += 1;
                current_col += if ch == '\t' { 4 } else { 1 };
            }

            // If line is shorter, extend with spaces
            if current_col < target_col {
                let spaces_needed = target_col - current_col;
                let spaces = " ".repeat(spaces_needed);
                self.apply_insert_raw(char_idx, &spaces);
                char_idx += spaces_needed;
            }

            // First line becomes primary cursor
            if !primary_set {
                self.cursor_char_idx = char_idx.min(self.text.len_chars());
                primary_set = true;
            } else {
                self.virtual_cursors.push(VirtualCursor {
                    char_idx: char_idx.min(self.text.len_chars()),
                    selection_start: None,
                    selection_end: None,
                });
            }
        }

        // Clear block state and transition to MultiInsert
        self.visual_block_anchor_line = None;
        self.visual_block_anchor_col = None;
        let (_, col) = self.cursor_line_col();
        self.target_col = col;
        let _ = self.mode_state.apply(ModeEvent::EnterMultiInsert);
        self.bump_revision();
        true
    }

    /// A in VisualBlock — append at end of block on each line.
    pub fn visual_block_append_after(&mut self) -> bool {
        if self.current_mode() != EditorMode::VisualBlock {
            return false;
        }
        let Some(block) = self.visual_block_range() else {
            return false;
        };

        self.ensure_current_transaction();

        // Generate cursors for each line in block
        self.virtual_cursors.clear();
        let mut primary_set = false;

        for line_idx in block.start_line..=block.end_line {
            if line_idx >= self.text.len_lines() {
                break;
            }

            let line_start_char = self.text.line_to_char(line_idx);
            let line_content = self.text.line(line_idx);

            // Calculate target column position (end_col + 1 for append)
            let target_col = block.end_col + 1;
            let mut char_idx = line_start_char;
            let mut current_col = 0;

            // Walk to target column or line end
            for ch in line_content.chars() {
                if ch == '\n' {
                    break;
                }
                if current_col >= target_col {
                    break;
                }
                char_idx += 1;
                current_col += if ch == '\t' { 4 } else { 1 };
            }

            // If line is shorter, extend with spaces
            if current_col < target_col {
                let spaces_needed = target_col - current_col;
                let spaces = " ".repeat(spaces_needed);
                self.apply_insert_raw(char_idx, &spaces);
                char_idx += spaces_needed;
            }

            // First line becomes primary cursor
            if !primary_set {
                self.cursor_char_idx = char_idx.min(self.text.len_chars());
                primary_set = true;
            } else {
                self.virtual_cursors.push(VirtualCursor {
                    char_idx: char_idx.min(self.text.len_chars()),
                    selection_start: None,
                    selection_end: None,
                });
            }
        }

        // Clear block state and transition to MultiInsert
        self.visual_block_anchor_line = None;
        self.visual_block_anchor_col = None;
        let (_, col) = self.cursor_line_col();
        self.target_col = col;
        let _ = self.mode_state.apply(ModeEvent::EnterMultiInsert);
        self.bump_revision();
        true
    }

    /// c in VisualBlock — delete selected rectangular ranges, enter MultiInsert.
    pub fn visual_block_change(&mut self) -> bool {
        if self.current_mode() != EditorMode::VisualBlock {
            return false;
        }
        if !self.delete_visual_block_ranges_and_place_cursors() {
            return false;
        }
        let _ = self.mode_state.apply(ModeEvent::EnterMultiInsert);
        true
    }

    /// d in VisualBlock — delete selected rectangular ranges, return to Normal.
    pub fn visual_block_delete(&mut self) -> bool {
        if self.current_mode() != EditorMode::VisualBlock {
            return false;
        }
        if !self.delete_visual_block_ranges_and_place_cursors() {
            return false;
        }
        let _ = self.mode_state.apply(ModeEvent::EnterNormal);
        self.clear_virtual_cursors();
        true
    }

    /// c — delete all selections, enter MultiInsert.
    pub fn multi_cursor_change(&mut self) -> bool {
        if !self.is_multi_cursor_mode() {
            return false;
        }
        self.delete_all_multi_cursor_selections();
        let _ = self.mode_state.apply(ModeEvent::EnterMultiInsert);
        true
    }

    /// d — delete all selections, stay in MultiCursor.
    pub fn multi_cursor_delete(&mut self) -> bool {
        if !self.is_multi_cursor_mode() {
            return false;
        }
        self.delete_all_multi_cursor_selections();
        true
    }

    /// Insert `ch` simultaneously at all cursor positions (MultiInsert mode).
    /// Follows the Reverse-Order Index Shifting rule.
    pub fn multi_insert_char(&mut self, ch: char) {
        self.ensure_current_transaction();
        let len_chars = self.text.len_chars();

        let mut positions = self.collect_all_cursor_positions();
        // Sort descending so each insert doesn't affect lower-indexed positions.
        positions.sort_unstable_by(|a, b| b.cmp(a));
        positions.dedup();

        let n = positions.len();
        for (k, &pos) in positions.iter().enumerate() {
            let insert_at = pos.min(len_chars + k); // rope grows by k prior inserts
            self.apply_insert_raw(insert_at, &ch.to_string());
        }

        // Update positions: cursor at original p[k] → p[k] + (n − k).
        let updated: Vec<(usize, usize)> = positions
            .iter()
            .enumerate()
            .map(|(k, &orig)| (orig, orig + (n - k)))
            .collect();

        self.apply_position_updates(&updated);
        let (_, col) = self.cursor_line_col();
        self.target_col = col;
        self.dirty = true;
        self.bump_revision();
    }

    /// Paste `text` at every cursor (MultiCursor/MultiInsert).
    ///
    /// Visual-p semantics: a cursor with an active selection has that selection
    /// REPLACED by the text; a bare caret (MultiInsert phase) gets a plain
    /// insert. Sites are processed in descending order so earlier edits never
    /// shift later ones, and each caret ends up right after its pasted copy.
    pub fn multi_insert_text(&mut self, text: &str) -> bool {
        if text.is_empty() {
            return false;
        }
        self.ensure_current_transaction();

        // (old_caret, del_start, del_len) per cursor — del_len 0 = pure insert.
        let mut sites: Vec<(usize, usize, usize)> = self
            .virtual_cursors
            .iter()
            .map(|vc| match (vc.selection_start, vc.selection_end) {
                (Some(ss), Some(se)) if ss < se => {
                    (vc.char_idx.min(se.saturating_sub(1)), ss, se - ss)
                }
                _ => (vc.char_idx, vc.char_idx, 0),
            })
            .collect();
        let primary_site = match self.selection_anchor_char_idx {
            Some(anchor) => {
                let ss = anchor.min(self.cursor_char_idx);
                let se = anchor.max(self.cursor_char_idx) + 1;
                (self.cursor_char_idx.min(se.saturating_sub(1)), ss, se - ss)
            }
            None => (self.cursor_char_idx, self.cursor_char_idx, 0),
        };
        sites.push(primary_site);

        // Descending by deletion start; collapse cursors sharing an identical site.
        sites.sort_unstable_by_key(|&(_, ds, _)| std::cmp::Reverse(ds));
        sites.dedup();

        let inserted_len = text.chars().count();
        for (_, ds, dl) in &sites {
            if *dl > 0 {
                self.apply_delete_raw(*ds, *dl);
            }
            self.apply_insert_raw(*ds, text);
        }

        // New caret for site k: end of its pasted copy, shifted by the net delta
        // (insert − delete) of every later site — later starts are strictly
        // smaller and the ranges are disjoint, so all of them shift it.
        // Signed: pasting text SHORTER than the replaced selection shifts later
        // sites left (negative delta) — usize saturation would strand carets.
        let mut suffix_shift = 0isize;
        let mut updated: Vec<(usize, usize)> = Vec::with_capacity(sites.len());
        for &(caret, ds, dl) in sites.iter().rev() {
            let new_pos = (ds + inserted_len) as isize + suffix_shift;
            updated.push((caret, new_pos.max(0) as usize));
            suffix_shift += inserted_len as isize - dl as isize;
        }
        updated.reverse();
        self.apply_position_updates(&updated);

        // The paste consumes the selections.
        self.selection_anchor_char_idx = None;
        for vc in &mut self.virtual_cursors {
            vc.selection_start = None;
            vc.selection_end = None;
        }

        let (_, col) = self.cursor_line_col();
        self.target_col = col;
        self.dirty = true;
        self.bump_revision();
        true
    }

    /// Backspace simultaneously at all cursor positions (MultiInsert mode).
    /// Follows the Ascending-Order Deletion rule.
    pub fn multi_backspace(&mut self) -> bool {
        let mut positions = self.collect_all_cursor_positions();
        positions.sort_unstable();
        positions.dedup();

        if positions.iter().all(|&p| p == 0) {
            return false;
        }

        self.ensure_current_transaction();

        let mut deletions_done: usize = 0;
        for &orig_pos in &positions {
            if orig_pos == 0 {
                continue;
            }
            let delete_at = orig_pos - 1 - deletions_done;
            if self.apply_delete_raw(delete_at, 1).is_some() {
                deletions_done += 1;
            }
        }

        let mut actual_deletes: usize = 0;
        let updated: Vec<(usize, usize)> = positions
            .iter()
            .map(|&orig| {
                let new_pos = if orig == 0 {
                    0
                } else {
                    let new = orig.saturating_sub(1 + actual_deletes);
                    actual_deletes += 1;
                    new
                };
                (orig, new_pos)
            })
            .collect();

        self.apply_position_updates(&updated);
        let (_, col) = self.cursor_line_col();
        self.target_col = col;
        self.dirty = true;
        self.bump_revision();
        true
    }

    // ── Private helpers ───────────────────────────────────────────────────────

    pub(crate) fn is_multi_cursor_mode(&self) -> bool {
        matches!(
            self.mode_state.current(),
            EditorMode::MultiCursor | EditorMode::MultiInsert
        )
    }

    /// Delete VisualBlock ranges line-by-line and place cursors at each deleted range start.
    fn delete_visual_block_ranges_and_place_cursors(&mut self) -> bool {
        let Some(block) = self.visual_block_range() else {
            return false;
        };

        self.ensure_current_transaction();

        let mut ranges: Vec<(usize, usize)> = Vec::new();
        for line_idx in block.start_line..=block.end_line {
            if line_idx >= self.text.len_lines() {
                break;
            }

            let line_start_char = self.text.line_to_char(line_idx);
            let max_col = self.max_col_for_line(line_idx);
            let start_col = block.start_col.min(max_col);
            let end_col = block.end_col.saturating_add(1).min(max_col);
            if start_col >= end_col {
                continue;
            }

            let start = line_start_char + start_col;
            let end = line_start_char + end_col;
            if start < end {
                ranges.push((start, end));
            }
        }

        if ranges.is_empty() {
            return false;
        }

        for &(start, end) in &ranges {
            self.record_delete_highlight_edit(start, end - start);
        }

        ranges.sort_by(|a, b| b.0.cmp(&a.0));
        for &(start, end) in &ranges {
            self.apply_delete_raw(start, end - start);
        }

        fn adjusted_pos(orig: usize, ranges: &[(usize, usize)]) -> usize {
            let shift: usize = ranges
                .iter()
                .filter(|&&(start, _)| start < orig)
                .map(|&(start, end)| end - start)
                .sum();
            orig.saturating_sub(shift)
        }

        let mut cursor_positions: Vec<usize> = ranges
            .iter()
            .map(|&(start, _)| adjusted_pos(start, &ranges))
            .collect();
        cursor_positions.sort_unstable();
        cursor_positions.dedup();

        if let Some(primary) = cursor_positions.first().copied() {
            self.cursor_char_idx = primary.min(self.text.len_chars());
            self.virtual_cursors = cursor_positions
                .iter()
                .skip(1)
                .map(|&char_idx| VirtualCursor {
                    char_idx: char_idx.min(self.text.len_chars()),
                    selection_start: None,
                    selection_end: None,
                })
                .collect();
        }

        self.visual_block_anchor_line = None;
        self.visual_block_anchor_col = None;
        self.selection_anchor_char_idx = None;
        let (_, col) = self.cursor_line_col();
        self.target_col = col;
        self.dirty = true;
        self.bump_revision();
        true
    }

    /// Delete all selections (primary + virtual) in reverse order to preserve indices.
    fn delete_all_multi_cursor_selections(&mut self) {
        self.ensure_current_transaction();

        // Collect (start, end) pairs for every selection, sorted in reverse.
        let mut ranges: Vec<(usize, usize)> = Vec::new();

        // Primary cursor selection.
        if let Some(anchor) = self.selection_anchor_char_idx {
            let (s, e) = sort_pair(anchor, self.cursor_char_idx + 1);
            let e = e.min(self.text.len_chars());
            if s < e {
                ranges.push((s, e));
            }
        }

        // Virtual cursor selections.
        for vc in &self.virtual_cursors {
            if let (Some(ss), Some(se)) = (vc.selection_start, vc.selection_end) {
                let se = se.min(self.text.len_chars());
                if ss < se {
                    ranges.push((ss, se));
                }
            }
        }

        // Sort descending by start so each delete doesn't shift subsequent ranges.
        ranges.sort_by(|a, b| b.0.cmp(&a.0));

        for &(s, e) in &ranges {
            self.apply_delete_raw(s, e - s);
        }

        // After bulk deletion in descending order, cursors at lower positions are
        // shifted downward by each deletion that occurred above them.  For an original
        // start position `orig`, the correct post-deletion position is:
        //   orig - Σ(e_k - s_k) for every deleted range (s_k, e_k) where s_k < orig
        fn adjusted_pos(orig: usize, ranges: &[(usize, usize)]) -> usize {
            let shift: usize = ranges
                .iter()
                .filter(|&&(s, _)| s < orig)
                .map(|&(s, e)| e - s)
                .sum();
            orig.saturating_sub(shift)
        }

        let primary_orig = self
            .selection_anchor_char_idx
            .map(|a| a.min(self.cursor_char_idx))
            .unwrap_or(self.cursor_char_idx);
        self.cursor_char_idx = adjusted_pos(primary_orig, &ranges).min(self.text.len_chars());
        let (_, col) = self.cursor_line_col();
        self.target_col = col;
        self.selection_anchor_char_idx = None;

        // Collect original starts before mutating virtual_cursors.
        let vc_orig_starts: Vec<usize> = self
            .virtual_cursors
            .iter()
            .map(|vc| vc.selection_start.unwrap_or(vc.char_idx))
            .collect();

        let len_chars = self.text.len_chars();
        for (vc, orig) in self.virtual_cursors.iter_mut().zip(vc_orig_starts) {
            vc.char_idx = adjusted_pos(orig, &ranges).min(len_chars);
            vc.selection_start = None;
            vc.selection_end = None;
        }

        self.dirty = true;
        self.bump_revision();
    }

    fn collect_all_cursor_positions(&self) -> Vec<usize> {
        let mut v: Vec<usize> = self.virtual_cursors.iter().map(|vc| vc.char_idx).collect();
        v.push(self.cursor_char_idx);
        v
    }

    /// Apply a set of (old_pos → new_pos) updates to primary and virtual cursors.
    fn apply_position_updates(&mut self, updates: &[(usize, usize)]) {
        let len_chars = self.text.len_chars();
        for &(old, new) in updates {
            if old == self.cursor_char_idx {
                self.cursor_char_idx = new.min(len_chars);
            }
            for vc in &mut self.virtual_cursors {
                if vc.char_idx == old {
                    vc.char_idx = new.min(len_chars);
                }
            }
        }
    }

    /// Merge any overlapping virtual cursor selections.  Called after add_next.
    fn merge_overlapping_cursors(&mut self) {
        if self.virtual_cursors.len() < 2 {
            return;
        }
        self.virtual_cursors
            .sort_by_key(|vc| vc.selection_start.unwrap_or(vc.char_idx));
        let mut merged: Vec<VirtualCursor> = Vec::with_capacity(self.virtual_cursors.len());
        for vc in self.virtual_cursors.drain(..) {
            if let Some(last) = merged.last_mut() {
                let last_end = last.selection_end.unwrap_or(last.char_idx + 1);
                let this_start = vc.selection_start.unwrap_or(vc.char_idx);
                if this_start < last_end {
                    // Overlap: extend the last cursor to cover both.
                    let new_end = last_end.max(vc.selection_end.unwrap_or(vc.char_idx + 1));
                    last.selection_end = Some(new_end);
                    last.char_idx = new_end.saturating_sub(1);
                    continue;
                }
            }
            merged.push(vc);
        }
        self.virtual_cursors = merged;
    }

    /// Returns the (start, end) char range of the word at the current cursor.
    fn word_bounds_at_cursor(&self) -> Option<(usize, usize)> {
        let len_chars = self.text.len_chars();
        if len_chars == 0 {
            return None;
        }
        let focus = self.cursor_char_idx.min(len_chars.saturating_sub(1));
        if classify_char(self.text.char(focus)) != WordClass::Word {
            return None;
        }
        let mut start = focus;
        while start > 0 && classify_char(self.text.char(start - 1)) == WordClass::Word {
            start -= 1;
        }
        let mut end = focus + 1;
        while end < len_chars && classify_char(self.text.char(end)) == WordClass::Word {
            end += 1;
        }
        Some((start, end))
    }
}

// ── Free helpers ──────────────────────────────────────────────────────────────

/// Find the next whole-word occurrence of `word` in `text` starting from
/// `from_char` (char index).  Returns `(start_char, end_char)` if found.
fn find_whole_word(text: &str, word: &str, from_char: usize) -> Option<(usize, usize)> {
    if word.is_empty() {
        return None;
    }

    // Build char→byte and byte→char maps lazily via iterators.
    let from_byte = char_idx_to_byte(text, from_char);
    let search_slice = &text[from_byte..];

    let mut byte_offset = 0usize;
    while byte_offset < search_slice.len() {
        let Some(rel) = search_slice[byte_offset..].find(word) else {
            break;
        };
        let abs_byte = from_byte + byte_offset + rel;
        let abs_end_byte = abs_byte + word.len();

        let boundary_start =
            abs_byte == 0 || !is_word_char(text[..abs_byte].chars().next_back().unwrap_or(' '));
        let boundary_end = abs_end_byte >= text.len()
            || !is_word_char(text[abs_end_byte..].chars().next().unwrap_or(' '));

        if boundary_start && boundary_end {
            let char_start = text[..abs_byte].chars().count();
            let char_end = text[..abs_end_byte].chars().count();
            return Some((char_start, char_end));
        }
        byte_offset += rel + 1;
    }
    None
}

/// Plain substring search (no word-boundary requirement).  Used when the
/// search word was seeded from a Visual selection that may not be a whole word.
fn find_substring(text: &str, word: &str, from_char: usize) -> Option<(usize, usize)> {
    if word.is_empty() {
        return None;
    }
    let from_byte = char_idx_to_byte(text, from_char);
    let search_slice = &text[from_byte..];
    let rel = search_slice.find(word)?;
    let abs_byte = from_byte + rel;
    let abs_end_byte = abs_byte + word.len();
    let char_start = text[..abs_byte].chars().count();
    let char_end = text[..abs_end_byte].chars().count();
    Some((char_start, char_end))
}

fn char_idx_to_byte(text: &str, char_idx: usize) -> usize {
    text.char_indices()
        .nth(char_idx)
        .map(|(b, _)| b)
        .unwrap_or(text.len())
}

fn is_word_char(c: char) -> bool {
    classify_char(c) == WordClass::Word
}

fn sort_pair(a: usize, b: usize) -> (usize, usize) {
    if a <= b { (a, b) } else { (b, a) }
}
