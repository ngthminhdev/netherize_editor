use std::{
    fs,
    path::{Path, PathBuf},
};

use ropey::Rope;

use crate::app::file_picker::{FilePickerEntry, FilePickerState};
use crate::async_runtime::message::{FileSystemChangeKind, FileSystemEvent};
use crate::core::mode::{
    EditorMode, ModeEvent, ModeState, ModeTransitionError, ModeTransitionResult,
};
use crate::workspace::model::{WorkspaceModel, WorkspaceNodeType};

#[derive(Debug, Clone, Default)]
pub struct ExternalChangeReport {
    pub workspace_reloaded: bool,
    pub active_file_reloaded: bool,
    pub conflict_detected: bool,
    pub notices: Vec<String>,
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
    default_save_path: PathBuf,
    dirty: bool,
    workspace_model: Option<WorkspaceModel>,
    file_picker: FilePickerState,
    terminal_panel_open: bool,
    external_conflict: Option<String>,
    external_notice: Option<String>,
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
            default_save_path,
            dirty: false,
            workspace_model: None,
            file_picker: FilePickerState::default(),
            terminal_panel_open: false,
            external_conflict: None,
            external_notice: None,
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
            default_save_path,
            dirty: false,
            workspace_model: None,
            file_picker: FilePickerState::default(),
            terminal_panel_open: false,
            external_conflict: None,
            external_notice: None,
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

    pub fn open_file_picker(&mut self) -> Result<usize, String> {
        let workspace = self
            .workspace_model
            .as_ref()
            .ok_or_else(|| "workspace is not attached".to_string())?;
        Ok(self.file_picker.open(workspace))
    }

    pub fn close_file_picker(&mut self) -> bool {
        self.file_picker.close()
    }

    pub fn is_file_picker_open(&self) -> bool {
        self.file_picker.is_open
    }

    pub fn file_picker_query_text(&self) -> &str {
        &self.file_picker.query_text
    }

    pub fn file_picker_selected_index(&self) -> usize {
        self.file_picker.selected_index
    }

    pub fn file_picker_results(&self) -> &[FilePickerEntry] {
        self.file_picker.results()
    }

    pub fn file_picker_append_query(&mut self, text: &str) -> Result<bool, String> {
        let workspace = self
            .workspace_model
            .as_ref()
            .ok_or_else(|| "workspace is not attached".to_string())?;
        Ok(self.file_picker.append_query(text, workspace))
    }

    pub fn file_picker_backspace_query(&mut self) -> Result<bool, String> {
        let workspace = self
            .workspace_model
            .as_ref()
            .ok_or_else(|| "workspace is not attached".to_string())?;
        Ok(self.file_picker.backspace_query(workspace))
    }

    pub fn file_picker_select_next(&mut self) -> bool {
        self.file_picker.select_next()
    }

    pub fn file_picker_select_prev(&mut self) -> bool {
        self.file_picker.select_prev()
    }

    pub fn file_picker_selected_path(&self) -> Option<PathBuf> {
        self.file_picker.selected_path()
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
        if !self.file_picker.is_open {
            return Ok(false);
        }

        let workspace = self
            .workspace_model
            .as_ref()
            .ok_or_else(|| "workspace is not attached".to_string())?;
        Ok(self.file_picker.refresh_from_workspace(workspace))
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

        if report.workspace_reloaded && self.file_picker.is_open {
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
        self.text.insert_char(self.cursor_char_idx, ch);
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
        self.text.remove(start..self.cursor_char_idx);
        self.cursor_char_idx -= 1;

        let (_, col) = self.cursor_line_col();
        self.target_col = col;
        self.dirty = true;
        self.bump_revision();
    }

    pub fn move_left(&mut self) {
        let (line_idx, col) = self.cursor_line_col();

        if col > 0 {
            self.cursor_char_idx -= 1;
            self.target_col = col - 1;
            self.bump_revision();
            return;
        }

        if line_idx > 0 {
            // Ở đầu dòng và đi trái -> sang cuối dòng trước.
            let prev_line_max_col = self.max_col_for_line(line_idx - 1);
            self.cursor_char_idx -= 1;
            self.target_col = prev_line_max_col;
            self.bump_revision();
        }
    }

    pub fn move_right(&mut self) {
        let (line_idx, col) = self.cursor_line_col();
        let max_col = self.max_col_for_line(line_idx);
        if col < max_col {
            self.cursor_char_idx += 1;
            self.target_col = col + 1;
            self.bump_revision();
            return;
        }

        // Ở cuối dòng, ArrowRight sẽ nhảy sang đầu dòng kế tiếp (nếu có).
        let total_lines = self.text.len_lines();
        if line_idx + 1 < total_lines {
            self.cursor_char_idx = self.text.line_to_char(line_idx + 1);
            self.target_col = 0;
            self.bump_revision();
        }
    }

    pub fn move_up(&mut self) {
        let (line_idx, _) = self.cursor_line_col();
        if line_idx == 0 {
            return;
        }

        let target_line = line_idx - 1;
        let line_start = self.text.line_to_char(target_line);
        let new_col = self.target_col.min(self.max_col_for_line(target_line));
        let next_idx = line_start + new_col;
        if next_idx != self.cursor_char_idx {
            self.cursor_char_idx = next_idx;
            self.bump_revision();
        }
    }

    pub fn move_down(&mut self) {
        let (line_idx, _) = self.cursor_line_col();
        let total_lines = self.text.len_lines();
        if line_idx + 1 >= total_lines {
            return;
        }

        let target_line = line_idx + 1;
        let line_start = self.text.line_to_char(target_line);
        let new_col = self.target_col.min(self.max_col_for_line(target_line));
        let next_idx = line_start + new_col;
        if next_idx != self.cursor_char_idx {
            self.cursor_char_idx = next_idx;
            self.bump_revision();
        }
    }

    pub fn open_file(&mut self, path: PathBuf) -> Result<(), String> {
        let canonical_path = path
            .canonicalize()
            .map_err(|err| format!("canonicalize file {:?} failed: {err}", path))?;

        let content = fs::read_to_string(&canonical_path)
            .map_err(|err| format!("open file {:?} failed: {err}", canonical_path))?;

        self.text = Rope::from(content.as_str());
        self.cursor_char_idx = 0;
        self.target_col = 0;
        self.active_file = Some(canonical_path);
        self.dirty = false;
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
        self.dirty = false;
        Ok(canonical_path)
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

    /// Lấy prefix text để render mode file lớn mà không cần clone toàn bộ buffer.
    pub fn prefix_text(&self, max_chars: usize) -> String {
        self.text.chars().take(max_chars).collect()
    }

    pub fn active_file(&self) -> Option<&Path> {
        self.active_file.as_deref()
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
        self.mode_state.apply(event)
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
        let picker_query = if self.is_file_picker_open() {
            self.file_picker_query_text()
        } else {
            ""
        };

        format!(
            "mode={} cursor=({},{}) chars={} lines={} bytes={} dirty={} rev={} picker_open={} terminal_open={} picker_query={:?} picker_results={} conflict={:?} notice={:?} file={} preview=\"{}\"",
            self.current_mode().as_str(),
            line,
            col,
            self.len_chars(),
            self.len_lines(),
            self.len_bytes(),
            self.is_dirty(),
            self.revision(),
            self.is_file_picker_open(),
            self.is_terminal_panel_open(),
            picker_query,
            self.file_picker_results().len(),
            self.external_conflict_message(),
            self.last_external_notice(),
            file_text,
            self.preview(48)
        )
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

    fn bump_revision(&mut self) {
        self.revision += 1;
    }
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
    fn mode_state_is_centralized_and_defaults_to_insert() {
        let state = AppState::new(unique_temp_path("mode"));
        assert_eq!(state.current_mode(), EditorMode::Insert);
    }

    #[test]
    fn mode_transition_normal_to_insert_via_app_state() {
        let mut state = AppState::new(unique_temp_path("mode"));
        state
            .apply_mode_event(ModeEvent::EnterNormal)
            .expect("insert -> normal should be valid");
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
            .apply_mode_event(ModeEvent::EnterNormal)
            .expect("insert -> normal");
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
