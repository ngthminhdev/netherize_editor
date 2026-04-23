use std::{fs, io::ErrorKind, path::Path};

use crate::workspace::model::{WorkspaceIgnoreRules, WorkspaceNode, WorkspaceNodeType};

/// Scanner chỉ làm nhiệm vụ đọc filesystem và tạo data model.
/// Không chứa logic UI để có thể tái sử dụng ở mọi layer.
#[derive(Debug, Clone)]
pub struct WorkspaceScanner {
    ignore_rules: WorkspaceIgnoreRules,
}

impl WorkspaceScanner {
    pub fn new(ignore_rules: WorkspaceIgnoreRules) -> Self {
        Self { ignore_rules }
    }

    pub fn scan(&self, root_path: &Path) -> Result<Vec<WorkspaceNode>, String> {
        if !root_path.exists() {
            return Err(format!("workspace root {:?} does not exist", root_path));
        }
        if !root_path.is_dir() {
            return Err(format!("workspace root {:?} is not a directory", root_path));
        }

        let root_path = root_path
            .canonicalize()
            .map_err(|err| format!("canonicalize root {:?} failed: {err}", root_path))?;

        let mut nodes = Vec::new();
        self.push_node(&root_path, WorkspaceNodeType::Folder, &mut nodes)?;
        self.scan_dir_recursive(&root_path, &mut nodes)?;

        // Sort path để output ổn định cho test/debug.
        nodes.sort_by(|left, right| left.path.cmp(&right.path));
        Ok(nodes)
    }

    fn scan_dir_recursive(
        &self,
        directory: &Path,
        nodes: &mut Vec<WorkspaceNode>,
    ) -> Result<(), String> {
        let read_dir = match fs::read_dir(directory) {
            Ok(entries) => entries,
            Err(err) if err.kind() == ErrorKind::NotFound => {
                // File watcher có thể báo event cho path vừa bị Git/xử lý nền xóa đi.
                // Bỏ qua để tránh fail cả workspace rescan vì transient ENOENT.
                return Ok(());
            }
            Err(err) => return Err(format!("read_dir {:?} failed: {err}", directory)),
        };

        let mut entries = Vec::new();
        for entry in read_dir {
            match entry {
                Ok(entry) => entries.push(entry),
                Err(err) if err.kind() == ErrorKind::NotFound => continue,
                Err(err) => {
                    return Err(format!("read_dir entry {:?} failed: {err}", directory));
                }
            }
        }
        entries.sort_by_key(|entry| entry.file_name());

        for entry in entries {
            let path = entry.path();
            let file_type = match entry.file_type() {
                Ok(file_type) => file_type,
                Err(err) if err.kind() == ErrorKind::NotFound => continue,
                Err(err) => return Err(format!("file_type {:?} failed: {err}", path)),
            };

            if file_type.is_dir() {
                if self.ignore_rules.should_ignore_dir(&path) {
                    continue;
                }

                self.push_node(&path, WorkspaceNodeType::Folder, nodes)?;
                self.scan_dir_recursive(&path, nodes)?;
                continue;
            }

            if file_type.is_file() {
                self.push_node(&path, WorkspaceNodeType::File, nodes)?;
            }
        }

        Ok(())
    }

    fn push_node(
        &self,
        path: &Path,
        file_type: WorkspaceNodeType,
        nodes: &mut Vec<WorkspaceNode>,
    ) -> Result<(), String> {
        let metadata = match fs::metadata(path) {
            Ok(metadata) => metadata,
            Err(err) if err.kind() == ErrorKind::NotFound => return Ok(()),
            Err(err) => return Err(format!("metadata {:?} failed: {err}", path)),
        };
        let modified_time = metadata.modified().ok();

        nodes.push(WorkspaceNode::new(
            path.to_path_buf(),
            file_type,
            modified_time,
        ));
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        path::Path,
        time::{SystemTime, UNIX_EPOCH},
    };

    use crate::workspace::{
        model::{WorkspaceIgnoreRules, WorkspaceNodeType},
        scanner::WorkspaceScanner,
    };

    fn unique_temp_dir(prefix: &str) -> std::path::PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock drift")
            .as_nanos();
        std::env::temp_dir().join(format!("netherize_workspace_{prefix}_{nanos}"))
    }

    fn contains_path_suffix(path: &Path, suffix: &str) -> bool {
        path.to_string_lossy().replace('\\', "/").ends_with(suffix)
    }

    #[test]
    fn scanner_builds_tree_and_respects_default_ignore_rules() {
        let root = unique_temp_dir("scan");
        fs::create_dir_all(root.join("src")).expect("create src");
        fs::create_dir_all(root.join(".git")).expect("create .git");
        fs::create_dir_all(root.join("target/debug")).expect("create target/debug");
        fs::create_dir_all(root.join("notes")).expect("create notes");

        fs::write(root.join("Cargo.toml"), "[package]\nname='demo'\n").expect("write cargo");
        fs::write(root.join("src/main.rs"), "fn main() {}\n").expect("write main");
        fs::write(root.join(".git/config"), "[core]\n").expect("write git config");
        fs::write(root.join("target/debug/bin"), "binary").expect("write target bin");
        fs::write(root.join("notes/todo.txt"), "scan me").expect("write todo");

        let scanner = WorkspaceScanner::new(WorkspaceIgnoreRules::default());
        let nodes = scanner.scan(&root).expect("scan workspace");

        assert!(nodes.iter().any(|node| {
            node.file_type == WorkspaceNodeType::Folder && contains_path_suffix(&node.path, "/src")
        }));
        assert!(nodes.iter().any(|node| {
            node.file_type == WorkspaceNodeType::File
                && contains_path_suffix(&node.path, "/src/main.rs")
        }));
        assert!(nodes.iter().any(|node| {
            node.file_type == WorkspaceNodeType::File
                && contains_path_suffix(&node.path, "/Cargo.toml")
        }));
        assert!(
            nodes
                .iter()
                .all(|node| !node.path.to_string_lossy().contains("/.git/"))
        );
        assert!(
            nodes
                .iter()
                .all(|node| !node.path.to_string_lossy().contains("/target/"))
        );
        assert!(nodes.iter().any(|node| node.modified_time.is_some()));

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn scanner_returns_error_when_root_is_not_directory() {
        let file_path = unique_temp_dir("root_file");
        fs::write(&file_path, "not a folder").expect("write root file");

        let scanner = WorkspaceScanner::new(WorkspaceIgnoreRules::default());
        let result = scanner.scan(&file_path);
        assert!(result.is_err());

        let _ = fs::remove_file(file_path);
    }

    #[test]
    fn push_node_skips_missing_path_without_failing() {
        let root = unique_temp_dir("missing_push_node");
        fs::create_dir_all(&root).expect("create root");
        let ghost = root.join("ghost.txt");
        fs::write(&ghost, "ghost").expect("write ghost");
        fs::remove_file(&ghost).expect("remove ghost");

        let scanner = WorkspaceScanner::new(WorkspaceIgnoreRules::default());
        let mut nodes = Vec::new();

        scanner
            .push_node(&ghost, WorkspaceNodeType::File, &mut nodes)
            .expect("missing path should be skipped");

        assert!(nodes.is_empty());

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn scan_dir_recursive_treats_missing_directory_as_empty() {
        let root = unique_temp_dir("missing_dir");
        fs::create_dir_all(&root).expect("create root");
        let deleted_dir = root.join("deleted");
        fs::create_dir_all(&deleted_dir).expect("create deleted dir");
        fs::remove_dir_all(&deleted_dir).expect("remove deleted dir");

        let scanner = WorkspaceScanner::new(WorkspaceIgnoreRules::default());
        let mut nodes = Vec::new();

        scanner
            .scan_dir_recursive(&deleted_dir, &mut nodes)
            .expect("missing directory should be ignored");

        assert!(nodes.is_empty());

        let _ = fs::remove_dir_all(root);
    }
}
