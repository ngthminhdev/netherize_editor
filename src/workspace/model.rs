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
}

impl WorkspaceModel {
    pub fn load(root_path: PathBuf) -> Result<Self, String> {
        Self::load_with_rules(root_path, WorkspaceIgnoreRules::default())
    }

    pub fn load_with_rules(
        root_path: PathBuf,
        ignore_rules: WorkspaceIgnoreRules,
    ) -> Result<Self, String> {
        let scanner = WorkspaceScanner::new(ignore_rules.clone());
        let nodes = scanner.scan(&root_path)?;
        Ok(Self {
            root_path,
            nodes,
            ignore_rules,
        })
    }

    pub fn rescan(&mut self) -> Result<(), String> {
        let scanner = WorkspaceScanner::new(self.ignore_rules.clone());
        self.nodes = scanner.scan(&self.root_path)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use crate::workspace::model::WorkspaceIgnoreRules;

    #[test]
    fn ignore_rules_detects_nested_ignored_components() {
        let rules = WorkspaceIgnoreRules::default();
        assert!(rules.should_ignore_path(Path::new("/tmp/demo/target/debug/app")));
        assert!(rules.should_ignore_path(Path::new("/tmp/demo/.git/config")));
        assert!(!rules.should_ignore_path(Path::new("/tmp/demo/src/main.rs")));
    }
}
