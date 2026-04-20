use std::path::PathBuf;

use crate::workspace::{
    fuzzy::{WorkspaceMatch, find_file_matches},
    model::WorkspaceModel,
};

const DEFAULT_MAX_RESULTS: usize = 24;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FilePickerEntry {
    pub absolute_path: PathBuf,
    pub relative_path: String,
    pub score: i64,
}

impl From<WorkspaceMatch> for FilePickerEntry {
    fn from(value: WorkspaceMatch) -> Self {
        Self {
            absolute_path: value.absolute_path,
            relative_path: value.relative_path,
            score: value.score,
        }
    }
}

#[derive(Debug, Clone)]
pub struct FilePickerState {
    pub is_open: bool,
    pub query_text: String,
    pub selected_index: usize,
    results: Vec<FilePickerEntry>,
    max_results: usize,
}

impl Default for FilePickerState {
    fn default() -> Self {
        Self {
            is_open: false,
            query_text: String::new(),
            selected_index: 0,
            results: Vec::new(),
            max_results: DEFAULT_MAX_RESULTS,
        }
    }
}

impl FilePickerState {
    pub fn open(&mut self, workspace: &WorkspaceModel) -> usize {
        self.is_open = true;
        self.query_text.clear();
        self.selected_index = 0;
        self.refresh_results(workspace);
        self.results.len()
    }

    pub fn close(&mut self) -> bool {
        let was_open = self.is_open;
        self.is_open = false;
        self.query_text.clear();
        self.selected_index = 0;
        self.results.clear();
        was_open
    }

    pub fn append_query(&mut self, text: &str, workspace: &WorkspaceModel) -> bool {
        if text.is_empty() {
            return false;
        }

        self.query_text.push_str(text);
        self.selected_index = 0;
        self.refresh_results(workspace);
        true
    }

    pub fn backspace_query(&mut self, workspace: &WorkspaceModel) -> bool {
        if self.query_text.is_empty() {
            return false;
        }

        self.query_text.pop();
        self.selected_index = 0;
        self.refresh_results(workspace);
        true
    }

    pub fn select_next(&mut self) -> bool {
        if self.results.is_empty() {
            return false;
        }
        let next = (self.selected_index + 1).min(self.results.len() - 1);
        let changed = next != self.selected_index;
        self.selected_index = next;
        changed
    }

    pub fn select_prev(&mut self) -> bool {
        if self.results.is_empty() {
            return false;
        }
        let prev = self.selected_index.saturating_sub(1);
        let changed = prev != self.selected_index;
        self.selected_index = prev;
        changed
    }

    pub fn selected_path(&self) -> Option<PathBuf> {
        self.results
            .get(self.selected_index)
            .map(|entry| entry.absolute_path.clone())
    }

    pub fn results(&self) -> &[FilePickerEntry] {
        &self.results
    }

    /// Dùng khi workspace model đã thay đổi (create/delete/rename) mà picker
    /// đang mở: giữ nguyên query hiện tại và rebuild danh sách kết quả.
    pub fn refresh_from_workspace(&mut self, workspace: &WorkspaceModel) -> bool {
        if !self.is_open {
            return false;
        }

        let old_len = self.results.len();
        let old_selected = self.selected_path();

        self.refresh_results(workspace);

        old_len != self.results.len() || old_selected != self.selected_path()
    }

    fn refresh_results(&mut self, workspace: &WorkspaceModel) {
        self.results = find_file_matches(workspace, &self.query_text, self.max_results)
            .into_iter()
            .map(FilePickerEntry::from)
            .collect();
        if self.results.is_empty() {
            self.selected_index = 0;
            return;
        }
        self.selected_index = self.selected_index.min(self.results.len() - 1);
    }
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        time::{SystemTime, UNIX_EPOCH},
    };

    use crate::{
        app::file_picker::FilePickerState,
        workspace::model::{WorkspaceIgnoreRules, WorkspaceModel},
    };

    fn unique_temp_dir(prefix: &str) -> std::path::PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock drift")
            .as_nanos();
        std::env::temp_dir().join(format!("netherize_picker_{prefix}_{nanos}"))
    }

    #[test]
    fn picker_open_query_select_flow() {
        let root = unique_temp_dir("flow");
        fs::create_dir_all(root.join("src")).expect("create src");
        fs::write(root.join("src/main.rs"), "fn main() {}\n").expect("write main");
        fs::write(root.join("src/lib.rs"), "pub fn lib() {}\n").expect("write lib");

        let workspace = WorkspaceModel::load_with_rules(
            root.clone(),
            WorkspaceIgnoreRules::new(Vec::<String>::new()),
        )
        .expect("load workspace");
        let mut picker = FilePickerState::default();

        let initial = picker.open(&workspace);
        assert!(picker.is_open);
        assert!(initial >= 2);
        assert_eq!(picker.selected_index, 0);

        let changed = picker.append_query("main", &workspace);
        assert!(changed);
        assert_eq!(picker.query_text, "main");
        assert!(!picker.results().is_empty());

        let selected = picker.selected_path().expect("selected path");
        assert!(selected.to_string_lossy().contains("main.rs"));

        let _ = picker.backspace_query(&workspace);
        assert_eq!(picker.query_text, "mai");

        let _ = picker.select_next();
        let _ = picker.select_prev();
        assert!(picker.close());
        assert!(!picker.is_open);

        let _ = fs::remove_dir_all(root);
    }
}
