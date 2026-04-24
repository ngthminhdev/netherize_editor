use std::{fs, io::ErrorKind, path::Path};

use ignore::gitignore::{Gitignore, GitignoreBuilder};

use crate::workspace::model::{WorkspaceIgnoreRules, WorkspaceNode, WorkspaceNodeType};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WorkspaceScanOptions {
    pub show_hidden: bool,
    pub show_ignored: bool,
}

impl Default for WorkspaceScanOptions {
    fn default() -> Self {
        Self {
            show_hidden: false,
            show_ignored: false,
        }
    }
}

/// Scanner chỉ làm nhiệm vụ đọc filesystem và tạo data model.
/// Không chứa logic UI để có thể tái sử dụng ở mọi layer.
#[derive(Debug, Clone)]
pub struct WorkspaceScanner {
    ignore_rules: WorkspaceIgnoreRules,
    options: WorkspaceScanOptions,
}

impl WorkspaceScanner {
    pub fn new(ignore_rules: WorkspaceIgnoreRules, options: WorkspaceScanOptions) -> Self {
        Self {
            ignore_rules,
            options,
        }
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

        let gitignore = self
            .build_gitignore_matcher(&root_path)
            .map_err(|err| format!("build gitignore matcher for {:?} failed: {err}", root_path))?;

        let mut nodes = Vec::new();
        self.push_node(&root_path, WorkspaceNodeType::Folder, &mut nodes)?;
        self.scan_dir_recursive(&root_path, &root_path, &gitignore, &mut nodes)?;

        // Sort path để output ổn định cho test/debug.
        nodes.sort_by(|left, right| left.path.cmp(&right.path));
        Ok(nodes)
    }

    fn scan_dir_recursive(
        &self,
        directory: &Path,
        root_path: &Path,
        gitignore: &Gitignore,
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
            let file_name = entry.file_name();
            let is_hidden = file_name.to_str().is_some_and(|name| name.starts_with('.'));
            let file_type = match entry.file_type() {
                Ok(file_type) => file_type,
                Err(err) if err.kind() == ErrorKind::NotFound => continue,
                Err(err) => return Err(format!("file_type {:?} failed: {err}", path)),
            };

            if self.should_skip_path(&path, root_path, gitignore, is_hidden, file_type.is_dir()) {
                continue;
            }

            if file_type.is_dir() {
                if self.ignore_rules.should_ignore_dir(&path) {
                    continue;
                }

                self.push_node(&path, WorkspaceNodeType::Folder, nodes)?;
                self.scan_dir_recursive(&path, root_path, gitignore, nodes)?;
                continue;
            }

            if file_type.is_file() {
                self.push_node(&path, WorkspaceNodeType::File, nodes)?;
            }
        }

        Ok(())
    }

    fn build_gitignore_matcher(&self, root_path: &Path) -> Result<Gitignore, String> {
        let mut builder = GitignoreBuilder::new(root_path);
        self.add_gitignore_files(root_path, &mut builder)?;
        builder
            .build()
            .map_err(|err| format!("gitignore build failed: {err}"))
    }

    fn add_gitignore_files(
        &self,
        directory: &Path,
        builder: &mut GitignoreBuilder,
    ) -> Result<(), String> {
        let read_dir = match fs::read_dir(directory) {
            Ok(entries) => entries,
            Err(err) if err.kind() == ErrorKind::NotFound => return Ok(()),
            Err(err) => return Err(format!("read_dir {:?} failed: {err}", directory)),
        };

        for entry in read_dir {
            let entry = match entry {
                Ok(entry) => entry,
                Err(err) if err.kind() == ErrorKind::NotFound => continue,
                Err(err) => return Err(format!("read_dir entry {:?} failed: {err}", directory)),
            };
            let path = entry.path();
            let file_type = match entry.file_type() {
                Ok(file_type) => file_type,
                Err(err) if err.kind() == ErrorKind::NotFound => continue,
                Err(err) => return Err(format!("file_type {:?} failed: {err}", path)),
            };

            if file_type.is_file() && entry.file_name().to_str() == Some(".gitignore") {
                builder.add(&path);
                continue;
            }

            if file_type.is_dir() && !self.is_hidden_name(path.file_name()) {
                self.add_gitignore_files(&path, builder)?;
            }
        }

        Ok(())
    }

    fn should_skip_path(
        &self,
        path: &Path,
        root_path: &Path,
        gitignore: &Gitignore,
        is_hidden: bool,
        is_dir: bool,
    ) -> bool {
        if !self.options.show_hidden && is_hidden {
            return true;
        }

        if !self.options.show_ignored {
            let relative = path.strip_prefix(root_path).unwrap_or(path);
            if gitignore
                .matched_path_or_any_parents(relative, is_dir)
                .is_ignore()
            {
                return true;
            }
        }

        false
    }

    fn is_hidden_name(&self, name: Option<&std::ffi::OsStr>) -> bool {
        name.and_then(|name| name.to_str())
            .is_some_and(|name| name.starts_with('.'))
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
        scanner::{WorkspaceScanOptions, WorkspaceScanner},
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

        let scanner = WorkspaceScanner::new(
            WorkspaceIgnoreRules::default(),
            WorkspaceScanOptions::default(),
        );
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

        let scanner = WorkspaceScanner::new(
            WorkspaceIgnoreRules::default(),
            WorkspaceScanOptions::default(),
        );
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

        let scanner = WorkspaceScanner::new(
            WorkspaceIgnoreRules::default(),
            WorkspaceScanOptions::default(),
        );
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

        let scanner = WorkspaceScanner::new(
            WorkspaceIgnoreRules::default(),
            WorkspaceScanOptions::default(),
        );
        let mut nodes = Vec::new();
        let gitignore = scanner
            .build_gitignore_matcher(&root)
            .expect("build gitignore");

        scanner
            .scan_dir_recursive(&deleted_dir, &root, &gitignore, &mut nodes)
            .expect("missing directory should be ignored");

        assert!(nodes.is_empty());

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn scanner_hides_hidden_and_gitignored_entries_by_default() {
        let root = unique_temp_dir("hidden_ignored_default");
        fs::create_dir_all(root.join("src")).expect("create src");
        fs::write(root.join(".gitignore"), "ignored.txt\nignored_dir/\n").expect("write gitignore");
        fs::write(root.join(".env"), "SECRET=1\n").expect("write hidden file");
        fs::write(root.join("ignored.txt"), "ignored\n").expect("write ignored file");
        fs::create_dir_all(root.join("ignored_dir")).expect("create ignored dir");
        fs::write(root.join("ignored_dir/file.txt"), "ignored dir file\n").expect("write nested");
        fs::write(root.join("src/main.rs"), "fn main() {}\n").expect("write main");

        let scanner = WorkspaceScanner::new(
            WorkspaceIgnoreRules::default(),
            WorkspaceScanOptions::default(),
        );
        let nodes = scanner.scan(&root).expect("scan workspace");

        assert!(
            nodes
                .iter()
                .all(|node| !contains_path_suffix(&node.path, "/.env"))
        );
        assert!(
            nodes
                .iter()
                .all(|node| !contains_path_suffix(&node.path, "/ignored.txt"))
        );
        assert!(
            nodes
                .iter()
                .all(|node| !node.path.to_string_lossy().contains("/ignored_dir"))
        );

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn scanner_can_show_hidden_and_gitignored_entries_when_enabled() {
        let root = unique_temp_dir("hidden_ignored_visible");
        fs::create_dir_all(root.join("src")).expect("create src");
        fs::write(root.join(".gitignore"), "ignored.txt\n").expect("write gitignore");
        fs::write(root.join(".env"), "SECRET=1\n").expect("write hidden file");
        fs::write(root.join("ignored.txt"), "ignored\n").expect("write ignored file");

        let scanner = WorkspaceScanner::new(
            WorkspaceIgnoreRules::default(),
            WorkspaceScanOptions {
                show_hidden: true,
                show_ignored: true,
            },
        );
        let nodes = scanner.scan(&root).expect("scan workspace");

        assert!(
            nodes
                .iter()
                .any(|node| contains_path_suffix(&node.path, "/.env"))
        );
        assert!(
            nodes
                .iter()
                .any(|node| contains_path_suffix(&node.path, "/ignored.txt"))
        );

        let _ = fs::remove_dir_all(root);
    }
}
