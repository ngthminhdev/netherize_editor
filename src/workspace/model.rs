use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
    time::SystemTime,
};

use crate::workspace::scanner::WorkspaceScanner;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkspaceNodeType {
    File,
    Folder,
}

#[derive(Debug, Clone)]
pub struct WorkspaceNode {
    pub path: PathBuf,
    pub file_type: WorkspaceNodeType,
    pub modified_time: Option<SystemTime>,
}

impl WorkspaceNode {
    pub fn new(
        path: PathBuf,
        file_type: WorkspaceNodeType,
        modified_time: Option<SystemTime>,
    ) -> Self {
        Self {
            path,
            file_type,
            modified_time,
        }
    }
}

#[derive(Debug, Clone)]
pub struct WorkspaceIgnoreRules {
    ignored_directory_names: BTreeSet<String>,
}

impl WorkspaceIgnoreRules {
    pub fn new<I, S>(ignored_directory_names: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        let ignored_directory_names = ignored_directory_names
            .into_iter()
            .map(Into::into)
            .collect::<BTreeSet<_>>();

        Self {
            ignored_directory_names,
        }
    }

    pub fn should_ignore_dir(&self, path: &Path) -> bool {
        let Some(name) = path.file_name() else {
            return false;
        };
        let Some(name) = name.to_str() else {
            return false;
        };
        self.ignored_directory_names.contains(name)
    }

    /// Dùng cho watcher/event filtering: nếu bất kỳ path component nào khớp
    /// tên directory bị ignore thì xem như path này nên bị bỏ qua.
    pub fn should_ignore_path(&self, path: &Path) -> bool {
        path.components().any(|component| {
            let Some(text) = component.as_os_str().to_str() else {
                return false;
            };
            self.ignored_directory_names.contains(text)
        })
    }

    pub fn ignored_directory_names(&self) -> impl Iterator<Item = &str> {
        self.ignored_directory_names
            .iter()
            .map(|name| name.as_str())
    }
}

impl Default for WorkspaceIgnoreRules {
    fn default() -> Self {
        Self::new([".git", "target"])
    }
}

#[derive(Debug, Clone)]
pub struct WorkspaceModel {
    pub root_path: PathBuf,
    pub nodes: Vec<WorkspaceNode>,
    pub ignore_rules: WorkspaceIgnoreRules,
    expanded_paths: BTreeSet<PathBuf>,
    selected_path: Option<PathBuf>,
}

impl WorkspaceModel {
    pub fn load(root_path: PathBuf) -> Result<Self, String> {
        Self::load_with_rules(root_path, WorkspaceIgnoreRules::default())
    }

    pub fn load_with_rules(
        root_path: PathBuf,
        ignore_rules: WorkspaceIgnoreRules,
    ) -> Result<Self, String> {
        let canonical_root = root_path
            .canonicalize()
            .unwrap_or_else(|_| root_path.clone());
        let scanner = WorkspaceScanner::new(ignore_rules.clone());
        let nodes = scanner.scan(&canonical_root)?;
        let mut model = Self {
            root_path: canonical_root,
            nodes,
            ignore_rules,
            expanded_paths: BTreeSet::new(),
            selected_path: None,
        };
        model.expanded_paths.insert(model.root_path.clone());
        model.prune_explorer_state();
        Ok(model)
    }

    pub fn rescan(&mut self) -> Result<(), String> {
        let scanner = WorkspaceScanner::new(self.ignore_rules.clone());
        self.nodes = scanner.scan(&self.root_path)?;
        self.prune_explorer_state();
        Ok(())
    }

    pub fn is_expanded(&self, path: &Path) -> bool {
        self.expanded_paths.contains(path)
    }

    pub fn selected_path(&self) -> Option<&Path> {
        self.selected_path.as_deref()
    }

    pub fn select_path(&mut self, path: &Path) -> bool {
        let normalized = Self::normalize_path(path);
        if !self.contains_path(&normalized) {
            return false;
        }
        if self.selected_path.as_deref() == Some(normalized.as_path()) {
            return false;
        }
        self.selected_path = Some(normalized);
        true
    }

    pub fn expand_path(&mut self, path: &Path) -> bool {
        let normalized = Self::normalize_path(path);
        if !self.is_folder_path(&normalized) {
            return false;
        }
        self.expanded_paths.insert(normalized)
    }

    pub fn collapse_path(&mut self, path: &Path) -> bool {
        let normalized = Self::normalize_path(path);
        if normalized == self.root_path {
            return false;
        }
        self.expanded_paths.remove(normalized.as_path())
    }

    pub fn collapse_path_and_descendants(&mut self, path: &Path) -> bool {
        let normalized = Self::normalize_path(path);
        if !self.is_folder_path(&normalized) || normalized == self.root_path {
            return false;
        }

        let paths_to_remove = self
            .expanded_paths
            .iter()
            .filter(|expanded| *expanded == &normalized || expanded.starts_with(&normalized))
            .cloned()
            .collect::<Vec<_>>();

        let mut changed = false;
        for expanded in paths_to_remove {
            changed |= self.expanded_paths.remove(&expanded);
        }
        changed
    }

    pub fn expand_path_and_descendants(&mut self, path: &Path) -> bool {
        let normalized = Self::normalize_path(path);
        if !self.is_folder_path(&normalized) {
            return false;
        }

        let mut changed = self.expanded_paths.insert(normalized.clone());
        for node in &self.nodes {
            if node.file_type == WorkspaceNodeType::Folder && node.path.starts_with(&normalized) {
                changed |= self.expanded_paths.insert(node.path.clone());
            }
        }
        changed
    }

    pub fn reveal_path(&mut self, path: &Path) -> bool {
        let normalized = Self::normalize_path(path);
        if !normalized.starts_with(&self.root_path) || !self.contains_path(&normalized) {
            return false;
        }

        let mut changed = false;
        changed |= self.expanded_paths.insert(self.root_path.clone());

        let mut ancestor = if self.is_folder_path(&normalized) {
            Some(normalized.as_path())
        } else {
            normalized.parent()
        };
        while let Some(dir) = ancestor {
            if !dir.starts_with(&self.root_path) {
                break;
            }
            if self.is_folder_path(dir) {
                changed |= self.expanded_paths.insert(dir.to_path_buf());
            }
            if dir == self.root_path {
                break;
            }
            ancestor = dir.parent();
        }

        if self.selected_path.as_deref() != Some(normalized.as_path()) {
            self.selected_path = Some(normalized);
            changed = true;
        }

        changed
    }

    fn contains_path(&self, path: &Path) -> bool {
        path == self.root_path || self.nodes.iter().any(|node| node.path == path)
    }

    fn is_folder_path(&self, path: &Path) -> bool {
        path == self.root_path
            || self
                .nodes
                .iter()
                .any(|node| node.file_type == WorkspaceNodeType::Folder && node.path == path)
    }

    fn prune_explorer_state(&mut self) {
        let mut valid_folders: BTreeSet<PathBuf> = BTreeSet::new();
        valid_folders.insert(self.root_path.clone());
        for node in &self.nodes {
            if node.file_type == WorkspaceNodeType::Folder {
                valid_folders.insert(node.path.clone());
            }
        }

        self.expanded_paths
            .retain(|path| valid_folders.contains(path));
        self.expanded_paths.insert(self.root_path.clone());

        if self
            .selected_path
            .as_ref()
            .is_some_and(|path| !self.contains_path(path))
        {
            self.selected_path = None;
        }
    }

    fn normalize_path(path: &Path) -> PathBuf {
        path.canonicalize().unwrap_or_else(|_| path.to_path_buf())
    }
}

#[cfg(test)]
mod tests {
    use std::{fs, path::Path};

    use crate::workspace::model::{WorkspaceIgnoreRules, WorkspaceModel};

    #[test]
    fn ignore_rules_detects_nested_ignored_components() {
        let rules = WorkspaceIgnoreRules::default();
        assert!(rules.should_ignore_path(Path::new("/tmp/demo/target/debug/app")));
        assert!(rules.should_ignore_path(Path::new("/tmp/demo/.git/config")));
        assert!(!rules.should_ignore_path(Path::new("/tmp/demo/src/main.rs")));
    }

    #[test]
    fn reveal_path_expands_ancestors_and_selects_file() {
        let root =
            std::env::temp_dir().join(format!("netherize_workspace_reveal_{}", std::process::id()));
        let nested_dir = root.join("src/ui");
        let nested_file = nested_dir.join("tabs.rs");
        fs::create_dir_all(&nested_dir).expect("create nested dirs");
        fs::write(&nested_file, "pub fn tabs() {}\n").expect("write nested file");
        let canonical_root = root.canonicalize().expect("canonical root");
        let canonical_src = canonical_root.join("src");
        let canonical_nested_dir = canonical_root.join("src/ui");
        let canonical_nested_file = canonical_root.join("src/ui/tabs.rs");

        let mut workspace = WorkspaceModel::load(root.clone()).expect("load workspace");
        assert!(workspace.reveal_path(&nested_file));
        assert!(workspace.is_expanded(&canonical_root));
        assert!(workspace.is_expanded(&canonical_src));
        assert!(workspace.is_expanded(&canonical_nested_dir));
        assert_eq!(
            workspace.selected_path(),
            Some(canonical_nested_file.as_path())
        );

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn collapse_path_and_descendants_closes_entire_subtree() {
        let root = std::env::temp_dir().join(format!(
            "netherize_workspace_collapse_descendants_{}",
            std::process::id()
        ));
        let nested_dir = root.join("src/ui/components");
        fs::create_dir_all(&nested_dir).expect("create nested dirs");
        fs::write(nested_dir.join("tabs.rs"), "pub fn tabs() {}\n").expect("write nested file");

        let canonical_root = root.canonicalize().expect("canonical root");
        let canonical_src = canonical_root.join("src");
        let canonical_ui = canonical_root.join("src/ui");
        let canonical_components = canonical_root.join("src/ui/components");

        let mut workspace = WorkspaceModel::load(root.clone()).expect("load workspace");
        assert!(workspace.expand_path(&canonical_src));
        assert!(workspace.expand_path(&canonical_ui));
        assert!(workspace.expand_path(&canonical_components));
        assert!(workspace.collapse_path_and_descendants(&canonical_src));
        assert!(!workspace.is_expanded(&canonical_src));
        assert!(!workspace.is_expanded(&canonical_ui));
        assert!(!workspace.is_expanded(&canonical_components));
        assert!(workspace.is_expanded(&canonical_root));

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn expand_path_and_descendants_opens_entire_subtree() {
        let root = std::env::temp_dir().join(format!(
            "netherize_workspace_expand_descendants_{}",
            std::process::id()
        ));
        let nested_dir = root.join("src/ui/components");
        fs::create_dir_all(&nested_dir).expect("create nested dirs");
        fs::write(nested_dir.join("tabs.rs"), "pub fn tabs() {}\n").expect("write nested file");

        let canonical_root = root.canonicalize().expect("canonical root");
        let canonical_src = canonical_root.join("src");
        let canonical_ui = canonical_root.join("src/ui");
        let canonical_components = canonical_root.join("src/ui/components");

        let mut workspace = WorkspaceModel::load(root.clone()).expect("load workspace");
        assert!(workspace.expand_path_and_descendants(&canonical_src));
        assert!(workspace.is_expanded(&canonical_root));
        assert!(workspace.is_expanded(&canonical_src));
        assert!(workspace.is_expanded(&canonical_ui));
        assert!(workspace.is_expanded(&canonical_components));

        let _ = fs::remove_dir_all(root);
    }
}
