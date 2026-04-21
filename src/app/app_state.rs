use std::{
    fs,
    path::{Path, PathBuf},
};

use ropey::Rope;

use crate::app::{
    command_palette::{
        CommandPalette, CommandPaletteAction, CommandPaletteMode, CommandPaletteRenderModel,
    },
    file_picker::FilePickerEntry,
};
use crate::async_runtime::message::{FileSystemChangeKind, FileSystemEvent};
use crate::core::mode::{
    EditorMode, ModeEvent, ModeState, ModeTransitionError, ModeTransitionResult,
};
use crate::core::transaction::{CursorState, EditAction, EditHistory, Transaction};
use crate::editor_core::filetype_label_for_path;
use crate::syntax::highlight::HighlightEdit;
use crate::workspace::model::{WorkspaceModel, WorkspaceNodeType};

#[derive(Debug, Clone, Default)]
pub struct ExternalChangeReport {
    pub workspace_reloaded: bool,
    pub active_file_reloaded: bool,
    pub conflict_detected: bool,
    pub notices: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VisualSelectionRange {
    pub start_char: usize,
    pub end_char: usize,
    pub start_line: usize,
    pub end_line: usize,
    pub start_byte_in_line: usize,
    pub end_byte_in_line: usize,
}

/// AppState giữ editor state tối thiểu cho phase 4.
///
/// Đây là nơi duy nhất được phép mutate text/cursor khi command dispatch chạy.
#[derive(Debug, Clone)]
pub struct AppState {
    text: Rope,
    cursor_char_idx: usize,
    target_col: usize,
    revision: u64,
    mode_state: ModeState,
    active_file: Option<PathBuf>,
    selection_anchor_char_idx: Option<usize>,
    visual_line_mode: bool,
    open_buffers: Vec<PathBuf>,
    active_buffer_index: Option<usize>,
    default_save_path: PathBuf,
    dirty: bool,
    pub scroll_line: usize,
    workspace_model: Option<WorkspaceModel>,
    command_palette: CommandPalette,
    file_picker_results_cache: Vec<FilePickerEntry>,
    terminal_panel_open: bool,
    external_conflict: Option<String>,
    external_notice: Option<String>,
    history: EditHistory,
    current_transaction: Option<Transaction>,
    pending_highlight_edits: Vec<HighlightEdit>,
}

impl AppState {
    pub fn new(default_save_path: PathBuf) -> Self {
        Self {
            text: Rope::new(),
            cursor_char_idx: 0,
            target_col: 0,
            revision: 0,
            mode_state: ModeState::default(),
            active_file: None,
            selection_anchor_char_idx: None,
            visual_line_mode: false,
            open_buffers: Vec::new(),
            active_buffer_index: None,
            default_save_path,
            dirty: false,
            scroll_line: 0,
            workspace_model: None,
            command_palette: CommandPalette::default(),
            file_picker_results_cache: Vec::new(),
            terminal_panel_open: false,
            external_conflict: None,
            external_notice: None,
            history: EditHistory::new(),
            current_transaction: None,
            pending_highlight_edits: Vec::new(),
        }
    }

    pub fn from_text(default_save_path: PathBuf, text: &str) -> Self {
        Self {
            text: Rope::from(text),
            cursor_char_idx: 0,
            target_col: 0,
            revision: 0,
            mode_state: ModeState::default(),
            active_file: None,
            selection_anchor_char_idx: None,
            visual_line_mode: false,
            open_buffers: Vec::new(),
            active_buffer_index: None,
            default_save_path,
            dirty: false,
            scroll_line: 0,
            workspace_model: None,
            command_palette: CommandPalette::default(),
            file_picker_results_cache: Vec::new(),
            terminal_panel_open: false,
            external_conflict: None,
            external_notice: None,
            history: EditHistory::new(),
            current_transaction: None,
            pending_highlight_edits: Vec::new(),
        }
    }

    pub fn ensure_probe_file(path: &Path, content: &str) -> Result<(), String> {
        if path.exists() {
            return Ok(());
        }

        fs::write(path, content)
            .map_err(|err| format!("create probe file {:?} failed: {err}", path))
    }

    pub fn attach_workspace(&mut self, root_path: PathBuf) -> Result<(), String> {
        let workspace = WorkspaceModel::load(root_path)?;
        self.workspace_model = Some(workspace);
        Ok(())
    }

    pub fn workspace_root_path(&self) -> Option<&Path> {
        self.workspace_model
            .as_ref()
            .map(|model| model.root_path.as_path())
    }

    pub fn workspace_nodes(&self) -> Option<&[crate::workspace::model::WorkspaceNode]> {
        self.workspace_model.as_ref().map(|m| m.nodes.as_slice())
    }

    pub fn workspace_file_count(&self) -> usize {
        self.workspace_model
            .as_ref()
            .map(|model| {
                model
                    .nodes
                    .iter()
                    .filter(|node| node.file_type == WorkspaceNodeType::File)
                    .count()
            })
            .unwrap_or(0)
    }

    pub fn open_command_palette_mode(&mut self, mode: CommandPaletteMode) -> Result<usize, String> {
        let workspace = self.workspace_model.as_ref();
        if matches!(mode, CommandPaletteMode::FilePicker) && workspace.is_none() {
            return Err("workspace is not attached".to_string());
        }

        let count = self.command_palette.open(mode, workspace);
        self.sync_file_picker_cache();
        Ok(count)
    }

    pub fn close_command_palette(&mut self) -> bool {
        let changed = self.command_palette.close();
        self.sync_file_picker_cache();
        changed
    }

    pub fn is_command_palette_visible(&self) -> bool {
        self.command_palette.is_visible
    }

    pub fn command_palette_mode(&self) -> Option<CommandPaletteMode> {
        self.command_palette
            .is_visible
            .then_some(self.command_palette.mode)
    }

    pub fn command_palette_query_text(&self) -> &str {
        &self.command_palette.query
    }

    pub fn command_palette_selected_index(&self) -> usize {
        self.command_palette.selected_index
    }

    pub fn command_palette_result_labels(&self) -> Vec<String> {
        self.command_palette
            .results
            .iter()
            .map(|entry| entry.label.clone())
            .collect()
    }

    pub fn command_palette_append_query(&mut self, text: &str) -> Result<bool, String> {
        let workspace = self.workspace_model.as_ref();
        if matches!(self.command_palette.mode, CommandPaletteMode::FilePicker)
            && workspace.is_none()
        {
            return Err("workspace is not attached".to_string());
        }

        let changed = self.command_palette.append_query(text, workspace);
        if changed {
            self.sync_file_picker_cache();
        }
        Ok(changed)
    }

    pub fn command_palette_backspace_query(&mut self) -> Result<bool, String> {
        let workspace = self.workspace_model.as_ref();
        if matches!(self.command_palette.mode, CommandPaletteMode::FilePicker)
            && workspace.is_none()
        {
            return Err("workspace is not attached".to_string());
        }

        let changed = self.command_palette.backspace_query(workspace);
        if changed {
            self.sync_file_picker_cache();
        }
        Ok(changed)
    }

    pub fn command_palette_select_next(&mut self) -> bool {
        self.command_palette.select_next()
    }

    pub fn command_palette_select_prev(&mut self) -> bool {
        self.command_palette.select_prev()
    }

    pub fn command_palette_selected_action(&self) -> Option<CommandPaletteAction> {
        self.command_palette.selected_action()
    }

    pub fn command_palette_render_model(
        &self,
        theme: &crate::config::theme_config::ThemeConfig,
        overlay_bounds: [f32; 4],
    ) -> Option<CommandPaletteRenderModel> {
        self.command_palette.render(theme, overlay_bounds)
    }

    pub fn open_file_picker(&mut self) -> Result<usize, String> {
        self.open_command_palette_mode(CommandPaletteMode::FilePicker)
    }

    pub fn close_file_picker(&mut self) -> bool {
        if !self.is_file_picker_open() {
            return false;
        }
        self.close_command_palette()
    }

    pub fn is_file_picker_open(&self) -> bool {
        self.command_palette.is_visible
            && self.command_palette.mode == CommandPaletteMode::FilePicker
    }

    pub fn file_picker_query_text(&self) -> &str {
        self.command_palette_query_text()
    }

    pub fn file_picker_selected_index(&self) -> usize {
        self.command_palette_selected_index()
    }

    pub fn file_picker_results(&self) -> &[FilePickerEntry] {
        &self.file_picker_results_cache
    }

    pub fn file_picker_append_query(&mut self, text: &str) -> Result<bool, String> {
        self.command_palette_append_query(text)
    }

    pub fn file_picker_backspace_query(&mut self) -> Result<bool, String> {
        self.command_palette_backspace_query()
    }

    pub fn file_picker_select_next(&mut self) -> bool {
        self.command_palette_select_next()
    }

    pub fn file_picker_select_prev(&mut self) -> bool {
        self.command_palette_select_prev()
    }

    pub fn file_picker_selected_path(&self) -> Option<PathBuf> {
        match self.command_palette_selected_action() {
            Some(CommandPaletteAction::OpenFile(path)) => Some(path),
            _ => None,
        }
    }

    pub fn is_terminal_panel_open(&self) -> bool {
        self.terminal_panel_open
    }

    pub fn set_terminal_panel_open(&mut self, open: bool) -> bool {
        if self.terminal_panel_open == open {
            return false;
        }
        self.terminal_panel_open = open;
        true
    }

    pub fn refresh_file_picker_results_if_open(&mut self) -> Result<bool, String> {
        if !self.is_file_picker_open() {
            return Ok(false);
        }
        let workspace = self
            .workspace_model
            .as_ref()
            .ok_or_else(|| "workspace is not attached".to_string())?;
        let changed = self.command_palette.refresh_if_open(Some(workspace));
        if changed {
            self.sync_file_picker_cache();
        }
        Ok(changed)
    }

    pub fn external_conflict_message(&self) -> Option<&str> {
        self.external_conflict.as_deref()
    }

    pub fn last_external_notice(&self) -> Option<&str> {
        self.external_notice.as_deref()
    }

    pub fn apply_external_file_events(
        &mut self,
        events: &[FileSystemEvent],
    ) -> Result<ExternalChangeReport, String> {
        let mut report = ExternalChangeReport::default();
        if events.is_empty() {
            return Ok(report);
        }

        let requires_workspace_rescan = events.iter().any(|event| {
            matches!(
                event.kind,
                FileSystemChangeKind::Create
                    | FileSystemChangeKind::Delete
                    | FileSystemChangeKind::Rename
            ) || (matches!(event.kind, FileSystemChangeKind::Modify) && !event.path.exists())
        });

        // Chỉ rescan khi tree shape có thể đổi (create/delete/rename).
        // Modify-only thường không đổi cấu trúc workspace, tránh quét cả cây quá nhiều.
        if requires_workspace_rescan && let Some(workspace) = self.workspace_model.as_mut() {
            workspace.rescan()?;
            report.workspace_reloaded = true;
        }

        if report.workspace_reloaded && self.is_file_picker_open() {
            if self.refresh_file_picker_results_if_open()? {
                let note = format!(
                    "file picker refreshed ({} results)",
                    self.file_picker_results().len()
                );
                self.external_notice = Some(note.clone());
                report.notices.push(note);
            }
        }

        let Some(active_path) = self.active_file.clone() else {
            return Ok(report);
        };

        for event in events {
            let touches_active = path_matches(&event.path, &active_path)
                || event
                    .new_path
                    .as_ref()
                    .is_some_and(|new_path| path_matches(new_path, &active_path));
            if !touches_active {
                continue;
            }

            if self.is_dirty() {
                let warning = format!(
                    "external {:?} detected on active file while dirty: {}",
                    event.kind,
                    active_path.display()
                );
                self.external_conflict = Some(warning.clone());
                self.external_notice = Some(warning.clone());
                report.conflict_detected = true;
                report.notices.push(warning);
                continue;
            }

            match event.kind {
                FileSystemChangeKind::Modify | FileSystemChangeKind::Create => {
                    match self.open_file(active_path.clone()) {
                        Ok(()) => {
                            let note = format!(
                                "auto reloaded active file from disk: {}",
                                active_path.display()
                            );
                            self.external_notice = Some(note.clone());
                            self.external_conflict = None;
                            report.active_file_reloaded = true;
                            report.notices.push(note);
                        }
                        Err(err) => {
                            let note = format!(
                                "auto reload skipped for active file {}: {}",
                                active_path.display(),
                                err
                            );
                            self.external_notice = Some(note.clone());
                            report.notices.push(note);
                        }
                    }
                }
                FileSystemChangeKind::Rename => {
                    if let Some(new_path) = &event.new_path {
                        match self.open_file(new_path.clone()) {
                            Ok(()) => {
                                let note = format!(
                                    "active file renamed externally, reloaded: {} -> {}",
                                    active_path.display(),
                                    new_path.display()
                                );
                                self.external_notice = Some(note.clone());
                                self.external_conflict = None;
                                report.active_file_reloaded = true;
                                report.notices.push(note);
                            }
                            Err(err) => {
                                let note = format!(
                                    "active file rename detected but reload failed {} -> {}: {}",
                                    active_path.display(),
                                    new_path.display(),
                                    err
                                );
                                self.external_notice = Some(note.clone());
                                report.notices.push(note);
                            }
                        }
                    }
                }
                FileSystemChangeKind::Delete => {
                    let note = format!(
                        "active file deleted externally: {} (buffer kept in memory)",
                        active_path.display()
                    );
                    self.external_notice = Some(note.clone());
                    report.notices.push(note);
                }
            }
        }

        Ok(report)
    }

    pub fn insert_char(&mut self, ch: char) {
        self.apply_insert(self.cursor_char_idx, ch.to_string());
        self.cursor_char_idx += 1;
        let (_, col) = self.cursor_line_col();
        self.target_col = col;
        self.dirty = true;
        self.bump_revision();
    }

    pub fn backspace(&mut self) {
        if self.cursor_char_idx == 0 {
            return;
        }

        let start = self.cursor_char_idx - 1;
        self.apply_delete(start, 1);
        self.cursor_char_idx -= 1;

        let (_, col) = self.cursor_line_col();
        self.target_col = col;
        self.dirty = true;
        self.bump_revision();
    }

    pub fn insert_line_below(&mut self) -> bool {
        let line_idx = self.text.char_to_line(self.cursor_char_idx);
        let insert_at = self.line_content_end_char_idx(line_idx);

        self.apply_insert(insert_at, "\n".to_string());
        let target_line = (line_idx + 1).min(self.text.len_lines().saturating_sub(1));
        self.cursor_char_idx = self.text.line_to_char(target_line);
        self.target_col = 0;
        self.dirty = true;
        self.bump_revision();
        true
    }

    pub fn insert_line_above(&mut self) -> bool {
        let line_idx = self.text.char_to_line(self.cursor_char_idx);
        let line_start = self.text.line_to_char(line_idx);

        self.apply_insert(line_start, "\n".to_string());
        self.cursor_char_idx = line_start;
        self.target_col = 0;
        self.dirty = true;
        self.bump_revision();
        true
    }

    pub fn move_to_line_first_non_blank(&mut self) -> bool {
        let line_idx = self.text.char_to_line(self.cursor_char_idx);
        let line_start = self.text.line_to_char(line_idx);
        let line_end = self.line_content_end_char_idx(line_idx);

        let mut target_idx = line_start;
        for char_idx in line_start..line_end {
            let ch = self.text.char(char_idx);
            if ch != ' ' && ch != '\t' {
                target_idx = char_idx;
                break;
            }
        }

        let previous_cursor = self.cursor_char_idx;
        let changed = self.update_cursor_position(target_idx);
        if self.cursor_char_idx != previous_cursor {
            let (_, col) = self.cursor_line_col();
            self.target_col = col;
        }
        changed
    }

    pub fn move_to_line_start(&mut self) -> bool {
        let line_idx = self.text.char_to_line(self.cursor_char_idx);
        let target_idx = self.text.line_to_char(line_idx);
        let changed = self.update_cursor_position(target_idx);
        self.target_col = 0;
        changed
    }

    pub fn move_to_first_non_whitespace(&mut self) -> bool {
        self.move_to_line_first_non_blank()
    }

    pub fn move_to_line_end(&mut self) -> bool {
        let line_idx = self.text.char_to_line(self.cursor_char_idx);
        let target_idx = self.line_content_end_char_idx(line_idx);
        let changed = self.update_cursor_position(target_idx);
        let (_, col) = self.cursor_line_col();
        self.target_col = col;
        changed
    }

    pub fn move_word_forward(&mut self) -> bool {
        let next_idx = next_word_start(&self.text, self.cursor_char_idx);
        let changed = self.update_cursor_position(next_idx);
        let (_, col) = self.cursor_line_col();
        self.target_col = col;
        changed
    }

    pub fn move_word_backward(&mut self) -> bool {
        let next_idx = previous_word_start(&self.text, self.cursor_char_idx);
        let changed = self.update_cursor_position(next_idx);
        let (_, col) = self.cursor_line_col();
        self.target_col = col;
        changed
    }

    pub fn move_word_end(&mut self) -> bool {
        let next_idx =
            word_end_at_or_after(&self.text, self.cursor_char_idx).unwrap_or(self.cursor_char_idx);
        let changed = self.update_cursor_position(next_idx);
        let (_, col) = self.cursor_line_col();
        self.target_col = col;
        changed
    }

    pub fn append_after_cursor(&mut self) -> bool {
        let line_idx = self.text.char_to_line(self.cursor_char_idx);
        let line_end = self.line_content_end_char_idx(line_idx);
        let target_idx = if self.cursor_char_idx < line_end {
            self.cursor_char_idx + 1
        } else {
            line_end
        };

        let changed = self.update_cursor_position(target_idx);
        let (_, col) = self.cursor_line_col();
        self.target_col = col;
        changed
    }

    pub fn substitute_current_line(&mut self) -> bool {
        let line_idx = self.text.char_to_line(self.cursor_char_idx);
        let line_start = self.text.line_to_char(line_idx);
        let line_end = self.line_content_end_char_idx(line_idx);
        let old_cursor = self.cursor_char_idx;

        let text_changed = line_end > line_start;
        if text_changed {
            self.apply_delete(line_start, line_end - line_start);
            self.dirty = true;
        }

        self.cursor_char_idx = line_start.min(self.text.len_chars());
        let (_, col) = self.cursor_line_col();
        self.target_col = col;

        let cursor_changed = self.cursor_char_idx != old_cursor;
        if text_changed || cursor_changed {
            self.bump_revision();
            return true;
        }
        false
    }

    pub fn delete_char_at_cursor(&mut self) -> bool {
        if self.text.len_chars() == 0 {
            return false;
        }

        let line_idx = self
            .text
            .char_to_line(self.cursor_char_idx.min(self.text.len_chars()));
        let line_start = self.text.line_to_char(line_idx);
        let line_end = self.line_content_end_char_idx(line_idx);
        if line_start == line_end {
            return false;
        }

        let mut delete_idx = if self.cursor_char_idx < line_end {
            self.cursor_char_idx
        } else {
            line_end.saturating_sub(1)
        };
        if delete_idx < line_start {
            delete_idx = line_start;
        }
        if delete_idx >= self.text.len_chars() {
            return false;
        }

        self.apply_delete(delete_idx, 1);
        self.cursor_char_idx = delete_idx.min(self.text.len_chars());
        let (_, col) = self.cursor_line_col();
        self.target_col = col;
        self.dirty = true;
        self.bump_revision();
        true
    }

    /// Vim `dw`: delete from cursor to the start of the next word on the same line,
    /// including trailing spaces/tabs. If cursor sits on a newline, delete just that
    /// newline (join with next line). No-op at EOF.
    pub fn delete_word_forward(&mut self) -> bool {
        let n = self.text.len_chars();
        if self.cursor_char_idx >= n {
            return false;
        }

        let end = next_word_start(&self.text, self.cursor_char_idx);
        if end <= self.cursor_char_idx {
            return false;
        }

        self.apply_delete(self.cursor_char_idx, end - self.cursor_char_idx);
        // Cursor stays at the same char index — the tail shifted in. Clamp in case
        // we just deleted everything after it (including a trailing newline).
        self.cursor_char_idx = self.cursor_char_idx.min(self.text.len_chars());
        let (_, col) = self.cursor_line_col();
        self.target_col = col;
        self.dirty = true;
        self.bump_revision();
        true
    }

    pub fn delete_word_backward(&mut self) -> bool {
        if self.cursor_char_idx == 0 {
            return false;
        }

        let start = previous_word_start(&self.text, self.cursor_char_idx);
        if start >= self.cursor_char_idx {
            return false;
        }

        self.apply_delete(start, self.cursor_char_idx - start);
        self.cursor_char_idx = start;
        let (_, col) = self.cursor_line_col();
        self.target_col = col;
        self.dirty = true;
        self.bump_revision();
        true
    }

    pub fn change_word_forward(&mut self) -> bool {
        let n = self.text.len_chars();
        if self.cursor_char_idx >= n {
            return false;
        }

        let end = next_word_start(&self.text, self.cursor_char_idx);
        if end <= self.cursor_char_idx {
            return false;
        }

        self.apply_delete(self.cursor_char_idx, end - self.cursor_char_idx);
        self.cursor_char_idx = self.cursor_char_idx.min(self.text.len_chars());
        let (_, col) = self.cursor_line_col();
        self.target_col = col;
        self.dirty = true;
        self.bump_revision();
        true
    }

    pub fn change_word_backward(&mut self) -> bool {
        if self.cursor_char_idx == 0 {
            return false;
        }

        let start = previous_word_start(&self.text, self.cursor_char_idx);
        if start >= self.cursor_char_idx {
            return false;
        }

        self.apply_delete(start, self.cursor_char_idx - start);
        self.cursor_char_idx = start;
        let (_, col) = self.cursor_line_col();
        self.target_col = col;
        self.dirty = true;
        self.bump_revision();
        true
    }

    pub fn replace_char_at_cursor(&mut self, ch: char) -> bool {
        if self.text.len_chars() == 0 {
            return false;
        }

        let line_idx = self
            .text
            .char_to_line(self.cursor_char_idx.min(self.text.len_chars()));
        let line_start = self.text.line_to_char(line_idx);
        let line_end = self.line_content_end_char_idx(line_idx);
        if line_start == line_end {
            return false;
        }

        let mut replace_idx = if self.cursor_char_idx < line_end {
            self.cursor_char_idx
        } else {
            line_end.saturating_sub(1)
        };
        if replace_idx < line_start {
            replace_idx = line_start;
        }
        if replace_idx >= self.text.len_chars() {
            return false;
        }

        self.apply_delete(replace_idx, 1);
        self.apply_insert(replace_idx, ch.to_string());
        self.cursor_char_idx = replace_idx.min(self.text.len_chars().saturating_sub(1));
        let (_, col) = self.cursor_line_col();
        self.target_col = col;
        self.dirty = true;
        self.bump_revision();
        true
    }

    pub fn delete_current_line(&mut self) -> bool {
        if self.text.len_lines() == 0 {
            return false;
        }

        let line_idx = self
            .text
            .char_to_line(self.cursor_char_idx.min(self.text.len_chars()));
        let line_start = self.text.line_to_char(line_idx);
        let mut line_end = if line_idx + 1 < self.text.len_lines() {
            self.text.line_to_char(line_idx + 1)
        } else {
            self.text.len_chars()
        };
        let mut delete_start = line_start;

        // Trailing empty line (from a final '\n') has an empty range.
        // Delete the previous '\n' so one logical line is removed.
        if delete_start == line_end && line_idx > 0 {
            delete_start = delete_start.saturating_sub(1);
            line_end = line_end.max(delete_start);
        }

        if delete_start == line_end {
            return false;
        }

        self.apply_delete(delete_start, line_end - delete_start);
        let remaining_lines = self.text.len_lines();
        let target_line = line_idx.min(remaining_lines.saturating_sub(1));
        self.cursor_char_idx = self.text.line_to_char(target_line);
        let (_, col) = self.cursor_line_col();
        self.target_col = col;
        self.dirty = true;
        self.bump_revision();
        true
    }

    pub fn move_left(&mut self) {
        let (line_idx, col) = self.cursor_line_col();
        let previous_cursor = self.cursor_char_idx;
        let mut target_idx = self.cursor_char_idx;
        let mut next_target_col = self.target_col;

        if col > 0 {
            target_idx = self.cursor_char_idx.saturating_sub(1);
            next_target_col = col - 1;
        } else if line_idx > 0 {
            // Ở đầu dòng và đi trái -> sang cuối dòng trước.
            target_idx = self.cursor_char_idx.saturating_sub(1);
            next_target_col = self.max_col_for_line(line_idx - 1);
        }

        let _ = self.update_cursor_position(target_idx);
        if self.cursor_char_idx != previous_cursor {
            self.target_col = next_target_col;
        }
    }

    pub fn move_right(&mut self) {
        let (line_idx, col) = self.cursor_line_col();
        let max_col = self.max_col_for_line(line_idx);
        let previous_cursor = self.cursor_char_idx;
        let mut target_idx = self.cursor_char_idx;
        let mut next_target_col = self.target_col;
        if col < max_col {
            target_idx = self.cursor_char_idx + 1;
            next_target_col = col + 1;
        } else {
            // Ở cuối dòng, ArrowRight sẽ nhảy sang đầu dòng kế tiếp (nếu có).
            let total_lines = self.text.len_lines();
            if line_idx + 1 < total_lines {
                target_idx = self.text.line_to_char(line_idx + 1);
                next_target_col = 0;
            }
        }

        let _ = self.update_cursor_position(target_idx);
        if self.cursor_char_idx != previous_cursor {
            self.target_col = next_target_col;
        }
    }

    pub fn move_up(&mut self) {
        let (line_idx, _) = self.cursor_line_col();
        let next_idx = if line_idx == 0 {
            self.cursor_char_idx
        } else {
            let target_line = line_idx - 1;
            let line_start = self.text.line_to_char(target_line);
            let new_col = self.target_col.min(self.max_col_for_line(target_line));
            line_start + new_col
        };
        let _ = self.update_cursor_position(next_idx);
    }

    pub fn move_down(&mut self) {
        let (line_idx, _) = self.cursor_line_col();
        let total_lines = self.text.len_lines();
        let next_idx = if line_idx + 1 >= total_lines {
            self.cursor_char_idx
        } else {
            let target_line = line_idx + 1;
            let line_start = self.text.line_to_char(target_line);
            let new_col = self.target_col.min(self.max_col_for_line(target_line));
            line_start + new_col
        };
        let _ = self.update_cursor_position(next_idx);
    }

    pub fn total_lines(&self) -> usize {
        self.text.len_lines()
    }

    pub fn move_to_first_line(&mut self) -> bool {
        let cursor_changed = self.update_cursor_position(0);
        let scroll_changed = if self.scroll_line != 0 {
            self.scroll_line = 0;
            true
        } else {
            false
        };
        self.target_col = 0;
        if scroll_changed && !cursor_changed {
            self.bump_revision();
        }
        cursor_changed || scroll_changed
    }

    pub fn move_to_last_line(&mut self) -> bool {
        let total = self.text.len_lines();
        let last = if total > 0 { total - 1 } else { 0 };
        let changed = self.update_cursor_position(self.text.line_to_char(last));
        self.target_col = 0;
        changed
    }

    pub fn center_cursor_line(&mut self, viewport_lines: usize) {
        let (cursor_line, _) = self.cursor_line_col();
        self.scroll_line = cursor_line.saturating_sub(viewport_lines / 2);
    }

    pub fn scroll_half_page_up(&mut self, half: usize) {
        self.scroll_line = self.scroll_line.saturating_sub(half);
        let new_line = self.cursor_line_col().0.saturating_sub(half);
        self.cursor_char_idx = self.text.line_to_char(new_line);
        self.target_col = 0;
        self.bump_revision();
    }

    pub fn scroll_half_page_down(&mut self, half: usize) {
        let total = self.text.len_lines();
        self.scroll_line = (self.scroll_line + half).min(total.saturating_sub(1));
        let new_line = (self.cursor_line_col().0 + half).min(total.saturating_sub(1));
        self.cursor_char_idx = self.text.line_to_char(new_line);
        self.target_col = 0;
        self.bump_revision();
    }

    /// Adjust scroll_line so the cursor is within the viewport.
    pub fn auto_scroll_to_cursor(&mut self, viewport_lines: usize) {
        let (cursor_line, _) = self.cursor_line_col();
        let margin = 3usize;
        if cursor_line < self.scroll_line + margin {
            self.scroll_line = cursor_line.saturating_sub(margin);
        } else if viewport_lines > margin
            && cursor_line + margin >= self.scroll_line + viewport_lines
        {
            self.scroll_line = cursor_line + margin + 1 - viewport_lines;
        }
    }

    pub fn open_file(&mut self, path: PathBuf) -> Result<(), String> {
        let canonical_path = path
            .canonicalize()
            .map_err(|err| format!("canonicalize file {:?} failed: {err}", path))?;
        self.load_buffer_from_file(&canonical_path)?;
        self.active_file = Some(canonical_path);
        self.register_open_buffer();
        self.selection_anchor_char_idx = None;
        self.dirty = false;
        self.external_conflict = None;
        self.bump_revision();
        Ok(())
    }

    pub fn save_file(&mut self) -> Result<PathBuf, String> {
        let path = self
            .active_file
            .clone()
            .unwrap_or_else(|| self.default_save_path.clone());

        fs::write(&path, self.text.to_string())
            .map_err(|err| format!("save file {:?} failed: {err}", path))?;

        let canonical_path = path
            .canonicalize()
            .map_err(|err| format!("canonicalize saved file {:?} failed: {err}", path))?;

        self.active_file = Some(canonical_path.clone());
        self.register_open_buffer();
        self.dirty = false;
        Ok(canonical_path)
    }

    pub fn new_empty_buffer(&mut self) -> bool {
        let changed = self.active_file.is_some() || self.dirty || self.text.len_chars() > 0;
        if !changed {
            return false;
        }

        self.text = Rope::new();
        self.cursor_char_idx = 0;
        self.target_col = 0;
        self.scroll_line = 0;
        self.active_file = None;
        self.selection_anchor_char_idx = None;
        self.active_buffer_index = None;
        self.dirty = false;
        self.external_conflict = None;
        self.visual_line_mode = false;
        self.clear_history();
        self.bump_revision();
        true
    }

    pub fn buffer_next(&mut self) -> Result<bool, String> {
        self.cycle_buffer(true)
    }

    pub fn buffer_prev(&mut self) -> Result<bool, String> {
        self.cycle_buffer(false)
    }

    pub fn close_current_buffer(&mut self) -> Result<bool, String> {
        let Some(active_path) = self.active_file.clone() else {
            return Ok(false);
        };
        let Some(current_idx) = self
            .open_buffers
            .iter()
            .position(|path| path == &active_path)
        else {
            return Ok(self.new_empty_buffer());
        };

        self.open_buffers.remove(current_idx);
        if self.open_buffers.is_empty() {
            self.text = Rope::new();
            self.cursor_char_idx = 0;
            self.target_col = 0;
            self.scroll_line = 0;
            self.active_file = None;
            self.selection_anchor_char_idx = None;
            self.active_buffer_index = None;
            self.dirty = false;
            self.external_conflict = None;
            self.visual_line_mode = false;
            self.clear_history();
            self.bump_revision();
            return Ok(true);
        }

        let mut next_idx = current_idx.min(self.open_buffers.len().saturating_sub(1));
        while !self.open_buffers.is_empty() {
            let next_path = self.open_buffers[next_idx].clone();
            match self.open_file(next_path) {
                Ok(()) => return Ok(true),
                Err(_) => {
                    self.open_buffers.remove(next_idx);
                    if self.open_buffers.is_empty() {
                        return Ok(self.new_empty_buffer());
                    }
                    if next_idx >= self.open_buffers.len() {
                        next_idx = 0;
                    }
                }
            }
        }

        Ok(self.new_empty_buffer())
    }

    pub fn begin_visual_selection(&mut self) -> bool {
        self.visual_line_mode = false;
        let anchor = if self.text.len_chars() == 0 {
            0
        } else {
            self.cursor_char_idx
                .min(self.text.len_chars().saturating_sub(1))
        };
        if self.selection_anchor_char_idx == Some(anchor) {
            return false;
        }
        self.selection_anchor_char_idx = Some(anchor);
        true
    }

    pub fn begin_visual_line_selection(&mut self) -> bool {
        self.visual_line_mode = true;
        let line_idx = self.text.char_to_line(
            self.cursor_char_idx
                .min(self.text.len_chars().saturating_sub(1).max(0)),
        );
        let anchor = self.text.line_to_char(line_idx);
        self.selection_anchor_char_idx = Some(anchor);
        // Move cursor to the last char of the line (before newline)
        let line_end = self.line_content_end_char_idx(line_idx);
        if self.cursor_char_idx != line_end {
            self.cursor_char_idx = line_end;
            let (_, col) = self.cursor_line_col();
            self.target_col = col;
        }
        true
    }

    pub fn clear_visual_selection(&mut self) -> bool {
        if self.selection_anchor_char_idx.is_none() && !self.visual_line_mode {
            return false;
        }
        self.selection_anchor_char_idx = None;
        self.visual_line_mode = false;
        true
    }

    pub fn visual_selection_range(&self) -> Option<VisualSelectionRange> {
        if self.current_mode() != EditorMode::Visual {
            return None;
        }
        let anchor = self.selection_anchor_char_idx?;
        let len_chars = self.text.len_chars();
        if len_chars == 0 {
            return None;
        }

        let anchor_idx = anchor.min(len_chars.saturating_sub(1));
        let focus_idx = self.cursor_char_idx.min(len_chars.saturating_sub(1));
        let (start_char, end_char) = if self.visual_line_mode {
            // Expand to full lines: from start of first line to start of line after last
            let first_line = self.text.char_to_line(anchor_idx.min(focus_idx));
            let last_line = self.text.char_to_line(anchor_idx.max(focus_idx));
            let sc = self.text.line_to_char(first_line);
            let ec = if last_line + 1 < self.text.len_lines() {
                self.text.line_to_char(last_line + 1)
            } else {
                len_chars
            };
            (sc, ec)
        } else {
            let sc = anchor_idx.min(focus_idx);
            let ec = anchor_idx.max(focus_idx).saturating_add(1).min(len_chars);
            (sc, ec)
        };

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

    pub fn delete_visual_selection(&mut self) -> bool {
        let Some(selection) = self.visual_selection_range() else {
            return false;
        };

        self.apply_delete(
            selection.start_char,
            selection.end_char - selection.start_char,
        );
        self.cursor_char_idx = selection.start_char.min(self.text.len_chars());
        let (_, col) = self.cursor_line_col();
        self.target_col = col;
        self.selection_anchor_char_idx = None;
        self.dirty = true;
        self.bump_revision();
        true
    }

    pub fn commit_transaction(&mut self) -> bool {
        let Some(mut transaction) = self.current_transaction.take() else {
            return false;
        };
        if transaction.is_empty() {
            return false;
        }

        transaction.after_cursor = self.cursor_state();
        self.history.undo_stack.push(transaction);
        self.history.redo_stack.clear();
        true
    }

    pub fn undo(&mut self) -> bool {
        let Some(transaction) = self.history.undo_stack.pop() else {
            return false;
        };

        self.current_transaction = None;
        for action in transaction.actions.iter().rev() {
            match action {
                EditAction::Insert { index, text } => {
                    self.record_delete_highlight_edit(*index, text.chars().count());
                    let _ = self.apply_delete_raw(*index, text.chars().count());
                }
                EditAction::Delete { index, text } => {
                    self.record_insert_highlight_edit(*index, text);
                    self.apply_insert_raw(*index, text);
                }
            }
        }

        self.restore_cursor_state(transaction.before_cursor);
        self.selection_anchor_char_idx = None;
        self.visual_line_mode = false;
        self.dirty = true;
        self.history.redo_stack.push(transaction);
        self.bump_revision();
        true
    }

    pub fn redo(&mut self) -> bool {
        let Some(transaction) = self.history.redo_stack.pop() else {
            return false;
        };

        self.current_transaction = None;
        for action in &transaction.actions {
            match action {
                EditAction::Insert { index, text } => {
                    self.record_insert_highlight_edit(*index, text);
                    self.apply_insert_raw(*index, text);
                }
                EditAction::Delete { index, text } => {
                    self.record_delete_highlight_edit(*index, text.chars().count());
                    let _ = self.apply_delete_raw(*index, text.chars().count());
                }
            }
        }

        self.restore_cursor_state(transaction.after_cursor);
        self.selection_anchor_char_idx = None;
        self.visual_line_mode = false;
        self.dirty = true;
        self.history.undo_stack.push(transaction);
        self.bump_revision();
        true
    }

    pub fn cursor_line_col(&self) -> (usize, usize) {
        let line_idx = self.text.char_to_line(self.cursor_char_idx);
        let line_start = self.text.line_to_char(line_idx);
        let col_idx = self.cursor_char_idx - line_start;
        (line_idx, col_idx)
    }

    pub fn cursor_char_idx(&self) -> usize {
        self.cursor_char_idx
    }

    pub fn cursor_byte_idx(&self) -> usize {
        self.text.char_to_byte(self.cursor_char_idx)
    }

    /// Byte offset tương đối trong dòng hiện tại (không phải toàn buffer).
    /// Hữu ích khi map với glyph.start/end của cosmic-text theo từng line run.
    pub fn cursor_byte_in_line(&self) -> usize {
        let line_idx = self.text.char_to_line(self.cursor_char_idx);
        let line_start_byte = self.text.line_to_byte(line_idx);
        self.cursor_byte_idx().saturating_sub(line_start_byte)
    }

    pub fn text_string(&self) -> String {
        self.text.to_string()
    }

    pub fn take_highlight_edits(&mut self) -> Vec<HighlightEdit> {
        std::mem::take(&mut self.pending_highlight_edits)
    }

    /// Lấy prefix text để render mode file lớn mà không cần clone toàn bộ buffer.
    pub fn prefix_text(&self, max_chars: usize) -> String {
        self.text.chars().take(max_chars).collect()
    }

    pub fn active_file(&self) -> Option<&Path> {
        self.active_file.as_deref()
    }

    pub fn active_filetype_label(&self) -> &'static str {
        self.active_file
            .as_deref()
            .map(filetype_label_for_path)
            .unwrap_or("Plain Text")
    }

    pub fn default_save_path(&self) -> &Path {
        &self.default_save_path
    }

    pub fn len_chars(&self) -> usize {
        self.text.len_chars()
    }

    pub fn len_lines(&self) -> usize {
        self.text.len_lines()
    }

    pub fn len_bytes(&self) -> usize {
        self.text.len_bytes()
    }

    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    pub fn revision(&self) -> u64 {
        self.revision
    }

    pub fn current_mode(&self) -> EditorMode {
        self.mode_state.current()
    }

    pub fn can_apply_mode_event(&self, event: ModeEvent) -> bool {
        self.mode_state.can_apply(event)
    }

    pub fn apply_mode_event(
        &mut self,
        event: ModeEvent,
    ) -> Result<ModeTransitionResult, ModeTransitionError> {
        let result = self.mode_state.apply(event)?;
        if result.from == EditorMode::Insert && result.to != EditorMode::Insert {
            let _ = self.commit_transaction();
        }
        Ok(result)
    }

    pub fn preview(&self, max_chars: usize) -> String {
        let mut preview = String::new();
        for ch in self.text.chars().take(max_chars) {
            for escaped in ch.escape_default() {
                preview.push(escaped);
            }
        }

        if preview.is_empty() {
            return "<empty>".to_string();
        }

        if self.text.len_chars() > max_chars {
            preview.push_str("...");
        }
        preview
    }

    pub fn debug_state_line(&self) -> String {
        let (line, col) = self.cursor_line_col();
        let file_text = self
            .active_file()
            .map(|path| path.display().to_string())
            .unwrap_or_else(|| "<none>".to_string());
        let palette_query = if self.is_command_palette_visible() {
            self.command_palette_query_text()
        } else {
            ""
        };
        let palette_mode = self
            .command_palette_mode()
            .map(|mode| format!("{mode:?}"))
            .unwrap_or_else(|| "None".to_string());
        let selection = self
            .visual_selection_range()
            .map(|range| format!("{}..{}", range.start_char, range.end_char))
            .unwrap_or_else(|| "-".to_string());

        format!(
            "mode={} cursor=({},{}) chars={} lines={} bytes={} dirty={} rev={} palette_visible={} palette_mode={} terminal_open={} open_buffers={} active_buffer_index={:?} visual_selection={} palette_query={:?} palette_results={} conflict={:?} notice={:?} file={} preview=\"{}\"",
            self.current_mode().as_str(),
            line,
            col,
            self.len_chars(),
            self.len_lines(),
            self.len_bytes(),
            self.is_dirty(),
            self.revision(),
            self.is_command_palette_visible(),
            palette_mode,
            self.is_terminal_panel_open(),
            self.open_buffers.len(),
            self.active_buffer_index,
            selection,
            palette_query,
            self.command_palette.results.len(),
            self.external_conflict_message(),
            self.last_external_notice(),
            file_text,
            self.preview(48)
        )
    }

    fn cursor_state(&self) -> CursorState {
        CursorState {
            char_idx: self.cursor_char_idx,
            target_col: self.target_col,
        }
    }

    fn restore_cursor_state(&mut self, state: CursorState) {
        self.cursor_char_idx = state.char_idx.min(self.text.len_chars());
        let line_idx = self
            .text
            .char_to_line(self.cursor_char_idx.min(self.text.len_chars()));
        self.target_col = state.target_col.min(self.max_col_for_line(line_idx));
    }

    fn ensure_current_transaction(&mut self) -> &mut Transaction {
        if self.current_transaction.is_none() {
            self.current_transaction = Some(Transaction::new(self.cursor_state()));
        }
        self.current_transaction
            .as_mut()
            .expect("current transaction initialized")
    }

    fn apply_insert(&mut self, index: usize, text: String) -> bool {
        if text.is_empty() {
            return false;
        }

        let insert_at = index.min(self.text.len_chars());
        self.record_insert_highlight_edit(insert_at, &text);
        self.apply_insert_raw(insert_at, &text);
        self.ensure_current_transaction()
            .actions
            .push(EditAction::Insert {
                index: insert_at,
                text,
            });
        true
    }

    fn apply_delete(&mut self, index: usize, len_chars: usize) -> bool {
        if len_chars == 0 || index >= self.text.len_chars() {
            return false;
        }

        let end = (index + len_chars).min(self.text.len_chars());
        self.record_delete_highlight_edit(index, end - index);
        let Some(text) = self.apply_delete_raw(index, end - index) else {
            return false;
        };

        self.ensure_current_transaction()
            .actions
            .push(EditAction::Delete { index, text });
        true
    }

    fn apply_insert_raw(&mut self, index: usize, text: &str) {
        if text.is_empty() {
            return;
        }

        let insert_at = index.min(self.text.len_chars());
        self.text.insert(insert_at, text);
    }

    fn apply_delete_raw(&mut self, index: usize, len_chars: usize) -> Option<String> {
        if len_chars == 0 || index >= self.text.len_chars() {
            return None;
        }

        let end = (index + len_chars).min(self.text.len_chars());
        if end <= index {
            return None;
        }

        let deleted = self.text.slice(index..end).to_string();
        self.text.remove(index..end);
        Some(deleted)
    }

    fn clear_history(&mut self) {
        self.history.clear();
        self.current_transaction = None;
        self.pending_highlight_edits.clear();
    }

    fn record_insert_highlight_edit(&mut self, index: usize, text: &str) {
        if text.is_empty() {
            return;
        }

        let insert_at = index.min(self.text.len_chars());
        let start_byte = self.text.char_to_byte(insert_at);
        self.pending_highlight_edits
            .push(HighlightEdit::insert(start_byte, text.len()));
    }

    fn record_delete_highlight_edit(&mut self, index: usize, len_chars: usize) {
        if len_chars == 0 || index >= self.text.len_chars() {
            return;
        }

        let start_char = index.min(self.text.len_chars());
        let end_char = (start_char + len_chars).min(self.text.len_chars());
        if start_char >= end_char {
            return;
        }

        let start_byte = self.text.char_to_byte(start_char);
        let end_byte = self.text.char_to_byte(end_char);
        self.pending_highlight_edits
            .push(HighlightEdit::delete(start_byte, end_byte));
    }

    fn sync_file_picker_cache(&mut self) {
        if !self.is_file_picker_open() {
            self.file_picker_results_cache.clear();
            return;
        }

        self.file_picker_results_cache = self
            .command_palette
            .results
            .iter()
            .filter_map(|item| match &item.action {
                CommandPaletteAction::OpenFile(path) => Some(FilePickerEntry {
                    absolute_path: path.clone(),
                    relative_path: item.label.clone(),
                    score: 0,
                }),
                _ => None,
            })
            .collect();
    }

    fn max_col_for_line(&self, line_idx: usize) -> usize {
        let line = self.text.line(line_idx);
        let len_chars = line.len_chars();
        if len_chars == 0 {
            return 0;
        }

        // Rope line thường chứa '\n' ở cuối (trừ dòng cuối của file).
        // Cursor nên dừng ở "cuối nội dung dòng", không đứng sau '\n'.
        if line.char(len_chars - 1) == '\n' {
            len_chars - 1
        } else {
            // Dòng không có '\n' => cho phép caret đi tới vị trí sau ký tự cuối.
            len_chars
        }
    }

    fn line_content_end_char_idx(&self, line_idx: usize) -> usize {
        let clamped_line = line_idx.min(self.text.len_lines().saturating_sub(1));
        let line_start = self.text.line_to_char(clamped_line);
        line_start + self.max_col_for_line(clamped_line)
    }

    /// Unified cursor update path used by all motion commands.
    ///
    /// - In `Visual` mode: keep `selection_anchor_char_idx` untouched and move
    ///   cursor/focus only.
    /// - In non-visual modes: clear stale selection anchor while moving.
    fn update_cursor_position(&mut self, new_index: usize) -> bool {
        let clamped = new_index.min(self.text.len_chars());
        let mut changed = false;

        if clamped != self.cursor_char_idx {
            self.cursor_char_idx = clamped;
            changed = true;
        }

        if self.current_mode() != EditorMode::Visual
            && self.selection_anchor_char_idx.take().is_some()
        {
            changed = true;
        }

        if changed {
            self.bump_revision();
        }
        changed
    }

    fn bump_revision(&mut self) {
        self.revision += 1;
    }

    fn load_buffer_from_file(&mut self, canonical_path: &Path) -> Result<(), String> {
        let content = fs::read_to_string(canonical_path)
            .map_err(|err| format!("open file {:?} failed: {err}", canonical_path))?;
        self.text = Rope::from(content.as_str());
        self.cursor_char_idx = 0;
        self.target_col = 0;
        self.scroll_line = 0;
        self.selection_anchor_char_idx = None;
        self.visual_line_mode = false;
        self.clear_history();
        Ok(())
    }

    fn register_open_buffer(&mut self) {
        let Some(active_path) = self.active_file.clone() else {
            self.active_buffer_index = None;
            return;
        };

        if let Some(existing_idx) = self
            .open_buffers
            .iter()
            .position(|path| path == &active_path)
        {
            self.active_buffer_index = Some(existing_idx);
            return;
        }

        self.open_buffers.push(active_path);
        self.active_buffer_index = Some(self.open_buffers.len().saturating_sub(1));
    }

    fn cycle_buffer(&mut self, forward: bool) -> Result<bool, String> {
        if self.open_buffers.is_empty() {
            return Ok(false);
        }

        let current_idx = self
            .active_buffer_index
            .filter(|idx| *idx < self.open_buffers.len())
            .or_else(|| {
                self.active_file
                    .as_ref()
                    .and_then(|path| self.open_buffers.iter().position(|entry| entry == path))
            });

        let next_idx = match current_idx {
            Some(idx) if forward => (idx + 1) % self.open_buffers.len(),
            Some(idx) => {
                if idx == 0 {
                    self.open_buffers.len() - 1
                } else {
                    idx - 1
                }
            }
            None if forward => 0,
            None => self.open_buffers.len() - 1,
        };

        if current_idx == Some(next_idx) {
            return Ok(false);
        }

        let mut candidate_idx = next_idx;
        let mut attempts = self.open_buffers.len();
        while attempts > 0 && !self.open_buffers.is_empty() {
            attempts -= 1;
            let next_path = self.open_buffers[candidate_idx].clone();
            match self.open_file(next_path) {
                Ok(()) => return Ok(true),
                Err(_) => {
                    self.open_buffers.remove(candidate_idx);
                    if self.open_buffers.is_empty() {
                        return Ok(self.new_empty_buffer());
                    }
                    if candidate_idx >= self.open_buffers.len() {
                        candidate_idx = 0;
                    }
                }
            }
        }

        Ok(false)
    }
}

/// Vim word-class for `dw` boundary detection.
///   Word  = alphanumeric + `_`
///   Punct = non-whitespace, non-word
///   Space = space/tab (newline handled separately)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum WordClass {
    Word,
    Punct,
    Space,
    Newline,
}

fn classify_char(ch: char) -> WordClass {
    if ch == '\n' {
        WordClass::Newline
    } else if ch.is_whitespace() {
        WordClass::Space
    } else if ch.is_alphanumeric() || ch == '_' {
        WordClass::Word
    } else {
        WordClass::Punct
    }
}

/// Returns the char index where vim's `dw` motion should stop, counting from
/// `cursor`. Crosses same-line whitespace after the current token so `dw` eats
/// one "word-like run" plus trailing spaces. Stops at newline.
fn next_word_start(text: &Rope, cursor: usize) -> usize {
    let n = text.len_chars();
    if cursor >= n {
        return cursor;
    }

    let start_class = classify_char(text.char(cursor));

    // Cursor sitting on a newline: delete just that one newline (line join).
    if start_class == WordClass::Newline {
        return cursor + 1;
    }

    let mut i = cursor;

    // If cursor starts on whitespace, skip same-line whitespace only.
    if start_class == WordClass::Space {
        while i < n {
            let cls = classify_char(text.char(i));
            if cls != WordClass::Space {
                break;
            }
            i += 1;
        }
        return i;
    }

    // On Word or Punct: skip the current run of the same class, then skip
    // same-line trailing whitespace to land at start of next token.
    while i < n && classify_char(text.char(i)) == start_class {
        i += 1;
    }
    while i < n && classify_char(text.char(i)) == WordClass::Space {
        i += 1;
    }
    i
}

fn previous_word_start(text: &Rope, cursor: usize) -> usize {
    if cursor == 0 {
        return 0;
    }

    let mut i = cursor.saturating_sub(1);
    while i > 0 {
        let cls = classify_char(text.char(i));
        if cls != WordClass::Space && cls != WordClass::Newline {
            break;
        }
        i -= 1;
    }

    if classify_char(text.char(i)) == WordClass::Space
        || classify_char(text.char(i)) == WordClass::Newline
    {
        return i;
    }

    let cls = classify_char(text.char(i));
    while i > 0 && classify_char(text.char(i - 1)) == cls {
        i -= 1;
    }
    i
}

fn word_end_at_or_after(text: &Rope, cursor: usize) -> Option<usize> {
    let n = text.len_chars();
    if n == 0 || cursor >= n {
        return None;
    }

    let mut i = cursor;

    // If already at a word-end (non-space char whose next char is a different class),
    // step forward one so we land on the NEXT word (Vim `e` behavior).
    if i + 1 < n {
        let cur_cls = classify_char(text.char(i));
        let next_cls = classify_char(text.char(i + 1));
        if cur_cls != WordClass::Space && cur_cls != WordClass::Newline && next_cls != cur_cls {
            i += 1;
        }
    }

    while i < n {
        let cls = classify_char(text.char(i));
        if cls != WordClass::Space && cls != WordClass::Newline {
            break;
        }
        i += 1;
    }
    if i >= n {
        return None;
    }

    let cls = classify_char(text.char(i));
    while i + 1 < n && classify_char(text.char(i + 1)) == cls {
        i += 1;
    }
    Some(i)
}

fn path_matches(left: &Path, right: &Path) -> bool {
    if left == right {
        return true;
    }

    match (left.canonicalize(), right.canonicalize()) {
        (Ok(left), Ok(right)) => left == right,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        time::{SystemTime, UNIX_EPOCH},
    };

    use crate::async_runtime::message::{FileSystemChangeKind, FileSystemEvent};
    use crate::core::mode::{EditorMode, ModeEvent};

    use super::AppState;
    use crate::syntax::highlight::HighlightEdit;

    fn unique_temp_path(suffix: &str) -> std::path::PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock drift")
            .as_nanos();
        std::env::temp_dir().join(format!("netherize_phase4_{suffix}_{nanos}.txt"))
    }

    fn unique_temp_dir(suffix: &str) -> std::path::PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock drift")
            .as_nanos();
        std::env::temp_dir().join(format!("netherize_phase4_dir_{suffix}_{nanos}"))
    }

    #[test]
    fn insert_move_and_backspace_flow() {
        let mut state = AppState::new(unique_temp_path("scratch"));
        state.insert_char('a');
        state.insert_char('b');
        state.insert_char('c');
        assert_eq!(state.len_chars(), 3);

        state.move_left();
        state.backspace();

        assert_eq!(state.len_chars(), 2);
        assert_eq!(state.preview(16), "ac");
        assert!(state.is_dirty());
    }

    #[test]
    fn text_edits_record_highlight_byte_deltas() {
        let mut state = AppState::from_text(unique_temp_path("scratch"), "");

        state.insert_char('a');
        assert_eq!(
            state.take_highlight_edits(),
            vec![HighlightEdit::insert(0, 1)]
        );

        state.backspace();
        assert_eq!(
            state.take_highlight_edits(),
            vec![HighlightEdit::delete(0, 1)]
        );
    }

    #[test]
    fn save_then_open_roundtrip() {
        let save_path = unique_temp_path("save");
        let open_path = unique_temp_path("open");

        let mut state = AppState::from_text(save_path.clone(), "hello");
        let saved = state.save_file().expect("save should succeed");
        let canonical_save_path = save_path
            .canonicalize()
            .expect("canonical save path should exist");
        assert_eq!(saved, canonical_save_path);
        assert!(!state.is_dirty());

        std::fs::write(&open_path, "world").expect("write open file");
        state
            .open_file(open_path.clone())
            .expect("open should succeed");

        assert_eq!(state.preview(16), "world");
        assert!(!state.is_dirty());
        let canonical_open_path = open_path
            .canonicalize()
            .expect("canonical open path should exist");
        assert_eq!(
            state.active_file().expect("has file"),
            canonical_open_path.as_path()
        );

        let _ = std::fs::remove_file(save_path);
        let _ = std::fs::remove_file(open_path);
    }

    #[test]
    fn buffer_cycle_and_close_follow_open_document_ring() {
        let mut state = AppState::new(unique_temp_path("buffer_ring"));
        let root = unique_temp_dir("buffer_ring");
        fs::create_dir_all(&root).expect("create buffer ring root");
        let file_a = root.join("a.rs");
        let file_b = root.join("b.rs");
        let file_c = root.join("c.rs");
        fs::write(&file_a, "alpha\n").expect("write a");
        fs::write(&file_b, "beta\n").expect("write b");
        fs::write(&file_c, "gamma\n").expect("write c");

        state.open_file(file_a.clone()).expect("open a");
        state.open_file(file_b.clone()).expect("open b");
        state.open_file(file_c.clone()).expect("open c");
        assert!(state.active_file().expect("active file").ends_with("c.rs"));

        assert!(state.buffer_prev().expect("buffer prev"));
        assert!(state.active_file().expect("active file").ends_with("b.rs"));
        assert!(state.buffer_next().expect("buffer next"));
        assert!(state.active_file().expect("active file").ends_with("c.rs"));

        assert!(state.close_current_buffer().expect("close current"));
        assert!(state.active_file().expect("active file").ends_with("b.rs"));
        assert!(state.close_current_buffer().expect("close current"));
        assert!(state.active_file().expect("active file").ends_with("a.rs"));
        assert!(state.close_current_buffer().expect("close current"));
        assert!(state.active_file().is_none());
        assert!(state.text_string().is_empty());

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn move_right_reaches_end_of_last_line_without_newline() {
        let mut state = AppState::from_text(unique_temp_path("cursor"), "abc");

        // Column có thể đi tới 3 (sau ký tự 'c').
        state.move_right();
        state.move_right();
        state.move_right();
        assert_eq!(state.cursor_line_col(), (0, 3));

        // Đi tiếp là no-op.
        state.move_right();
        assert_eq!(state.cursor_line_col(), (0, 3));
    }

    #[test]
    fn move_right_crosses_to_next_line_at_end_of_line() {
        let mut state = AppState::from_text(unique_temp_path("cursor"), "abc\ndef");

        state.move_right(); // col 1
        state.move_right(); // col 2
        state.move_right(); // col 3 (end of line 0)
        assert_eq!(state.cursor_line_col(), (0, 3));

        // ArrowRight ở cuối line 0 -> đầu line 1
        state.move_right();
        assert_eq!(state.cursor_line_col(), (1, 0));
    }

    #[test]
    fn delete_word_forward_eats_word_plus_trailing_space() {
        let mut state = AppState::from_text(unique_temp_path("dw"), "foo   bar baz");
        // cursor at col 0, on 'f'
        assert!(state.delete_word_forward());
        assert_eq!(state.text_string(), "bar baz");
        assert_eq!(state.cursor_line_col(), (0, 0));
        assert!(state.is_dirty());
    }

    #[test]
    fn delete_word_forward_stops_at_newline_and_preserves_next_line() {
        let mut state = AppState::from_text(unique_temp_path("dw_nl"), "foo\nbar");
        // cursor on 'f'; dw should eat "foo" but NOT the '\n'.
        assert!(state.delete_word_forward());
        assert_eq!(state.text_string(), "\nbar");
        assert_eq!(state.cursor_line_col(), (0, 0));
    }

    #[test]
    fn delete_word_forward_on_newline_joins_lines() {
        let mut state = AppState::from_text(unique_temp_path("dw_join"), "ab\ncd");
        // Two move_right steps leave the cursor ON the '\n' (char idx 2). A third
        // move_right would cross into line 1, per move_right's end-of-line jump.
        state.move_right();
        state.move_right();
        assert_eq!(state.cursor_line_col(), (0, 2));
        let cursor_before = state.cursor_char_idx();

        assert!(state.delete_word_forward());
        assert_eq!(state.text_string(), "abcd");
        assert_eq!(state.cursor_char_idx(), cursor_before);
    }

    #[test]
    fn delete_word_forward_on_punct_run_eats_only_punct() {
        let mut state = AppState::from_text(unique_temp_path("dw_punct"), "!!!foo");
        assert!(state.delete_word_forward());
        // Punct class is "!!!", then no trailing space, so cursor lands on 'f'.
        assert_eq!(state.text_string(), "foo");
    }

    #[test]
    fn delete_word_forward_at_eof_is_noop() {
        let mut state = AppState::from_text(unique_temp_path("dw_eof"), "abc");
        state.move_right();
        state.move_right();
        state.move_right(); // col 3 == len_chars
        assert!(!state.delete_word_forward());
        assert_eq!(state.text_string(), "abc");
    }

    #[test]
    fn delete_word_backward_erases_previous_word_span() {
        let mut state = AppState::from_text(unique_temp_path("db"), "foo   bar");
        for _ in 0..6 {
            state.move_right();
        }
        assert_eq!(state.cursor_line_col(), (0, 6));

        assert!(state.delete_word_backward());
        assert_eq!(state.text_string(), "bar");
        assert_eq!(state.cursor_line_col(), (0, 0));
    }

    #[test]
    fn append_after_cursor_moves_one_step_or_stays_at_line_end() {
        let mut state = AppState::from_text(unique_temp_path("a"), "abc");
        assert!(state.append_after_cursor());
        assert_eq!(state.cursor_line_col(), (0, 1));

        state.move_to_line_end();
        assert!(!state.append_after_cursor());
        assert_eq!(state.cursor_line_col(), (0, 3));
    }

    #[test]
    fn change_word_forward_deletes_span_and_sets_dirty() {
        let mut state = AppState::from_text(unique_temp_path("cw"), "foo   bar");
        assert!(state.change_word_forward());
        assert_eq!(state.text_string(), "bar");
        assert_eq!(state.cursor_line_col(), (0, 0));
        assert!(state.is_dirty());
    }

    #[test]
    fn change_word_backward_deletes_previous_span_and_sets_dirty() {
        let mut state = AppState::from_text(unique_temp_path("cb"), "foo   bar");
        for _ in 0..6 {
            state.move_right();
        }
        assert_eq!(state.cursor_line_col(), (0, 6));

        assert!(state.change_word_backward());
        assert_eq!(state.text_string(), "bar");
        assert_eq!(state.cursor_line_col(), (0, 0));
        assert!(state.is_dirty());
    }

    #[test]
    fn replace_char_at_cursor_replaces_without_mode_change() {
        let mut state = AppState::from_text(unique_temp_path("r"), "abc");
        let mode_before = state.current_mode();

        assert!(state.replace_char_at_cursor('X'));
        assert_eq!(state.text_string(), "Xbc");
        assert_eq!(state.current_mode(), mode_before);
        assert!(state.is_dirty());
    }

    #[test]
    fn visual_selection_anchor_focus_and_delete_work() {
        let mut state = AppState::from_text(unique_temp_path("visual"), "abcdef");
        state.move_right(); // anchor at char index 1 ('b')
        state
            .apply_mode_event(ModeEvent::EnterVisual)
            .expect("normal -> visual");
        assert!(state.begin_visual_selection());

        state.move_right();
        state.move_right(); // focus at index 3 ('d') -> selected "bcd"

        let selection = state.visual_selection_range().expect("selection exists");
        assert_eq!(selection.start_char, 1);
        assert_eq!(selection.end_char, 4);
        assert!(state.delete_visual_selection());
        assert_eq!(state.text_string(), "aef");
    }

    #[test]
    fn move_word_forward_jumps_to_next_word_start() {
        let mut state = AppState::from_text(unique_temp_path("w"), "foo   bar");
        assert!(state.move_word_forward());
        assert_eq!(state.cursor_line_col(), (0, 6));
    }

    #[test]
    fn move_word_backward_jumps_to_previous_word_start() {
        let mut state = AppState::from_text(unique_temp_path("b"), "foo   bar");
        for _ in 0..8 {
            state.move_right();
        }
        assert_eq!(state.cursor_line_col(), (0, 8));

        assert!(state.move_word_backward());
        assert_eq!(state.cursor_line_col(), (0, 6));
        assert!(state.move_word_backward());
        assert_eq!(state.cursor_line_col(), (0, 0));
    }

    #[test]
    fn move_word_end_stops_at_last_char_of_word() {
        let mut state = AppState::from_text(unique_temp_path("e"), "foo   bar");
        assert!(state.move_word_end());
        assert_eq!(state.cursor_line_col(), (0, 2));

        assert!(state.move_word_forward());
        assert!(state.move_word_end());
        assert_eq!(state.cursor_line_col(), (0, 8));
    }

    #[test]
    fn line_start_and_first_non_whitespace_motions_work() {
        let mut state = AppState::from_text(unique_temp_path("line_motion"), "   abc");
        for _ in 0..5 {
            state.move_right();
        }
        assert_eq!(state.cursor_line_col(), (0, 5));

        assert!(state.move_to_line_start());
        assert_eq!(state.cursor_line_col(), (0, 0));

        assert!(state.move_to_first_non_whitespace());
        assert_eq!(state.cursor_line_col(), (0, 3));
    }

    #[test]
    fn mode_state_is_centralized_and_defaults_to_normal() {
        let state = AppState::new(unique_temp_path("mode"));
        assert_eq!(state.current_mode(), EditorMode::Normal);
    }

    #[test]
    fn mode_transition_normal_to_insert_via_app_state() {
        let mut state = AppState::new(unique_temp_path("mode"));
        assert_eq!(state.current_mode(), EditorMode::Normal);

        let result = state
            .apply_mode_event(ModeEvent::EnterInsert)
            .expect("normal -> insert should be valid");

        assert_eq!(result.from, EditorMode::Normal);
        assert_eq!(result.to, EditorMode::Insert);
        assert_eq!(state.current_mode(), EditorMode::Insert);
    }

    #[test]
    fn invalid_mode_transition_is_rejected_from_app_state() {
        let mut state = AppState::new(unique_temp_path("mode"));
        state
            .apply_mode_event(ModeEvent::EnterInsert)
            .expect("normal -> insert should be valid");
        let error = state
            .apply_mode_event(ModeEvent::EnterVisual)
            .expect_err("insert -> visual should be invalid");
        assert_eq!(
            error,
            crate::core::mode::ModeTransitionError::InvalidTransition {
                from: EditorMode::Insert,
                event: ModeEvent::EnterVisual
            }
        );
        assert_eq!(state.current_mode(), EditorMode::Insert);
    }

    #[test]
    fn focus_mode_returns_to_previous_mode_via_app_state() {
        let mut state = AppState::new(unique_temp_path("mode"));
        state
            .apply_mode_event(ModeEvent::OpenPalette)
            .expect("normal -> palette focus");
        assert_eq!(state.current_mode(), EditorMode::PaletteFocus);

        state
            .apply_mode_event(ModeEvent::ExitFocus)
            .expect("palette -> previous");
        assert_eq!(state.current_mode(), EditorMode::Normal);
    }

    #[test]
    fn workspace_and_file_picker_state_are_tracked() {
        let mut state = AppState::new(unique_temp_path("workspace"));
        let root = unique_temp_dir("workspace");
        fs::create_dir_all(root.join("src")).expect("create src");
        fs::write(root.join("src/picker.rs"), "pub fn picker() {}\n").expect("write source");

        state
            .attach_workspace(root.clone())
            .expect("attach workspace should succeed");
        assert!(state.workspace_file_count() >= 1);

        let count = state.open_file_picker().expect("open picker");
        assert!(count >= 1);
        assert!(state.is_file_picker_open());

        let changed = state
            .file_picker_append_query("picker")
            .expect("append query");
        assert!(changed);
        assert_eq!(state.file_picker_query_text(), "picker");
        assert!(!state.file_picker_results().is_empty());

        let _ = state.close_file_picker();
        assert!(!state.is_file_picker_open());

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn file_picker_results_refresh_while_overlay_is_open_after_external_create() {
        let mut state = AppState::new(unique_temp_path("workspace_picker_refresh"));
        let root = unique_temp_dir("workspace_picker_refresh");
        fs::create_dir_all(root.join("src")).expect("create src");
        fs::write(root.join("src/old.rs"), "pub fn old() {}\n").expect("write old");

        state
            .attach_workspace(root.clone())
            .expect("attach workspace should succeed");
        state.open_file_picker().expect("open picker");

        assert!(
            state
                .file_picker_results()
                .iter()
                .all(|entry| !entry.relative_path.ends_with("src/new_file.rs"))
        );

        let created_path = root.join("src/new_file.rs");
        fs::write(&created_path, "pub fn new_file() {}\n").expect("write new file");
        let report = state
            .apply_external_file_events(&[FileSystemEvent {
                kind: FileSystemChangeKind::Create,
                path: created_path,
                new_path: None,
            }])
            .expect("apply external create");

        assert!(report.workspace_reloaded);
        assert!(state.is_file_picker_open());
        assert!(
            state
                .file_picker_results()
                .iter()
                .any(|entry| entry.relative_path.ends_with("src/new_file.rs"))
        );

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn external_modify_reloads_when_clean_and_warns_when_dirty() {
        let save_path = unique_temp_path("external");
        let mut state = AppState::new(save_path);
        let root = unique_temp_dir("external");
        fs::create_dir_all(root.join("src")).expect("create src");
        let active = root.join("src/main.rs");
        fs::write(&active, "fn main() {}\n").expect("write initial");

        state
            .attach_workspace(root.clone())
            .expect("attach workspace should succeed");
        state.open_file(active.clone()).expect("open active file");

        fs::write(&active, "fn main() { println!(\"reload\"); }\n").expect("write modified");
        let report = state
            .apply_external_file_events(&[FileSystemEvent {
                kind: FileSystemChangeKind::Modify,
                path: active.clone(),
                new_path: None,
            }])
            .expect("apply external clean");

        assert!(report.active_file_reloaded);
        assert!(state.preview(48).contains("reload"));

        state.insert_char('x');
        let dirty_report = state
            .apply_external_file_events(&[FileSystemEvent {
                kind: FileSystemChangeKind::Modify,
                path: active.clone(),
                new_path: None,
            }])
            .expect("apply external dirty");
        assert!(dirty_report.conflict_detected);
        assert!(state.external_conflict_message().is_some());

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn external_reload_error_does_not_abort_workspace_updates() {
        let save_path = unique_temp_path("external_reload_error");
        let mut state = AppState::new(save_path);
        let root = unique_temp_dir("external_reload_error");
        fs::create_dir_all(root.join("src")).expect("create src");
        let active = root.join("src/main.rs");
        fs::write(&active, "fn main() {}\n").expect("write initial");

        state
            .attach_workspace(root.clone())
            .expect("attach workspace should succeed");
        state.open_file(active.clone()).expect("open active file");
        state.open_file_picker().expect("open picker");

        // Mô phỏng file active bị xóa từ bên ngoài trước khi có event modify/reload.
        fs::remove_file(&active).expect("remove active file");

        let created_path = root.join("src/created_after_delete.rs");
        fs::write(&created_path, "pub fn created() {}\n").expect("write created file");
        let report = state
            .apply_external_file_events(&[
                FileSystemEvent {
                    kind: FileSystemChangeKind::Modify,
                    path: active.clone(),
                    new_path: None,
                },
                FileSystemEvent {
                    kind: FileSystemChangeKind::Create,
                    path: created_path.clone(),
                    new_path: None,
                },
            ])
            .expect("apply external events should not fail");

        assert!(report.workspace_reloaded);
        assert!(
            state
                .file_picker_results()
                .iter()
                .any(|entry| entry.relative_path.ends_with("src/created_after_delete.rs"))
        );

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn modify_event_on_missing_path_triggers_workspace_rescan_for_rename_like_flow() {
        let mut state = AppState::new(unique_temp_path("rename_like_modify"));
        let root = unique_temp_dir("rename_like_modify");
        fs::create_dir_all(root.join("src")).expect("create src");
        let old_path = root.join("src/old_name.rs");
        let new_path = root.join("src/new_name.rs");
        fs::write(&old_path, "pub fn old_name() {}\n").expect("write old");

        state
            .attach_workspace(root.clone())
            .expect("attach workspace should succeed");
        state.open_file_picker().expect("open picker");
        assert!(
            state
                .file_picker_results()
                .iter()
                .any(|entry| entry.relative_path.ends_with("src/old_name.rs"))
        );

        fs::rename(&old_path, &new_path).expect("rename file");
        let report = state
            .apply_external_file_events(&[FileSystemEvent {
                kind: FileSystemChangeKind::Modify,
                path: old_path.clone(),
                new_path: None,
            }])
            .expect("apply external rename-like modify");

        assert!(report.workspace_reloaded);
        assert!(
            state
                .file_picker_results()
                .iter()
                .any(|entry| entry.relative_path.ends_with("src/new_name.rs"))
        );
        assert!(
            state
                .file_picker_results()
                .iter()
                .all(|entry| !entry.relative_path.ends_with("src/old_name.rs"))
        );

        let _ = fs::remove_dir_all(root);
    }
}
