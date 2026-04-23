use std::{
    collections::{BTreeSet, HashMap},
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
    scroll_offset: f32,
    filter_query: String,
    is_inputting_filter: bool,
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
            scroll_offset: 0.0,
            filter_query: String::new(),
            is_inputting_filter: false,
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

    pub fn scroll_offset(&self) -> f32 {
        self.scroll_offset
    }

    pub fn scroll_offset_rows(&self, line_height: f32) -> usize {
        let line_height = line_height.max(1.0);
        (self.scroll_offset.max(0.0) / line_height).floor() as usize
    }

    pub fn filter_query(&self) -> &str {
        &self.filter_query
    }

    pub fn is_inputting_filter(&self) -> bool {
        self.is_inputting_filter
    }

    pub fn start_filter_input(&mut self) -> bool {
        if self.is_inputting_filter {
            return false;
        }
        self.is_inputting_filter = true;
        true
    }

    pub fn stop_filter_input(&mut self) -> bool {
        if !self.is_inputting_filter {
            return false;
        }
        self.is_inputting_filter = false;
        true
    }

    pub fn clear_filter(&mut self) -> bool {
        if self.filter_query.is_empty() && !self.is_inputting_filter {
            return false;
        }
        self.filter_query.clear();
        self.is_inputting_filter = false;
        true
    }

    pub fn append_filter_text(&mut self, text: &str) -> bool {
        let filtered = text
            .chars()
            .filter(|ch| !ch.is_control())
            .collect::<String>();
        if filtered.is_empty() {
            return false;
        }
        self.filter_query.push_str(&filtered);
        true
    }

    pub fn backspace_filter(&mut self) -> bool {
        self.filter_query.pop().is_some()
    }

    pub fn has_active_filter(&self) -> bool {
        !self.filter_query.is_empty()
    }

    pub fn node_matches_filter(&self, path: &Path) -> bool {
        if self.filter_query.is_empty() {
            return true;
        }
        let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
            return false;
        };
        name.to_lowercase()
            .contains(&self.filter_query.to_lowercase())
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

    pub fn expand_to_path(&mut self, target_path: &Path) -> bool {
        self.reveal_path(target_path)
    }

    pub fn scroll_to_selected_node(&mut self, viewport_height: f32, line_height: f32) -> bool {
        let Some(selected_path) = self.selected_path.as_ref() else {
            return false;
        };
        let line_height = line_height.max(1.0);
        let viewport_height = viewport_height.max(line_height);
        let visible_paths = self.visible_node_paths();
        let Some(index) = visible_paths
            .iter()
            .position(|path| path.as_path() == selected_path.as_path())
        else {
            return false;
        };

        let node_y = index as f32 * line_height;
        let node_bottom = node_y + line_height;
        let padding = (line_height * 2.0).min(((viewport_height - line_height) * 0.5).max(0.0));
        let viewport_top = self.scroll_offset.max(0.0);
        let viewport_bottom = viewport_top + viewport_height;

        let next_offset = if node_y < viewport_top + padding {
            (node_y - padding).max(0.0)
        } else if node_bottom > viewport_bottom - padding {
            (node_bottom + padding - viewport_height).max(0.0)
        } else {
            return false;
        };

        if (self.scroll_offset - next_offset).abs() < f32::EPSILON {
            return false;
        }
        self.scroll_offset = next_offset;
        true
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
            self.scroll_offset = 0.0;
        }
    }

    fn normalize_path(path: &Path) -> PathBuf {
        path.canonicalize().unwrap_or_else(|_| path.to_path_buf())
    }

    fn visible_node_paths(&self) -> Vec<PathBuf> {
        let mut node_types: HashMap<PathBuf, WorkspaceNodeType> = HashMap::new();
        let mut children_map: HashMap<PathBuf, Vec<PathBuf>> = HashMap::new();

        for node in &self.nodes {
            node_types.insert(node.path.clone(), node.file_type);
        }

        for node in &self.nodes {
            if node.path == self.root_path {
                continue;
            }
            let Some(parent) = node.path.parent() else {
                continue;
            };
            if !parent.starts_with(&self.root_path) {
                continue;
            }
            children_map
                .entry(parent.to_path_buf())
                .or_default()
                .push(node.path.clone());
        }

        for children in children_map.values_mut() {
            children.sort_by(|left, right| {
                let left_type = node_types
                    .get(left)
                    .copied()
                    .unwrap_or(WorkspaceNodeType::File);
                let right_type = node_types
                    .get(right)
                    .copied()
                    .unwrap_or(WorkspaceNodeType::File);
                let left_rank = if left_type == WorkspaceNodeType::Folder {
                    0
                } else {
                    1
                };
                let right_rank = if right_type == WorkspaceNodeType::Folder {
                    0
                } else {
                    1
                };
                left_rank.cmp(&right_rank).then_with(|| left.cmp(right))
            });
        }

        let trimmed_filter = self.filter_query.trim();
        let filter_query = (!trimmed_filter.is_empty()).then(|| trimmed_filter.to_lowercase());
        let mut subtree_matches: HashMap<PathBuf, bool> = HashMap::new();
        if let Some(query) = filter_query.as_deref() {
            Self::compute_visible_filter_matches(
                &self.root_path,
                &children_map,
                query,
                &mut subtree_matches,
            );
        }

        let mut out = Vec::new();
        self.collect_visible_node_paths(
            &self.root_path,
            &node_types,
            &children_map,
            filter_query.as_deref(),
            &subtree_matches,
            &mut out,
        );
        out
    }

    fn compute_visible_filter_matches(
        path: &Path,
        children_map: &HashMap<PathBuf, Vec<PathBuf>>,
        query: &str,
        out: &mut HashMap<PathBuf, bool>,
    ) -> bool {
        let self_match = path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.to_lowercase().contains(query));
        let mut subtree_match = self_match;

        if let Some(children) = children_map.get(path) {
            for child in children {
                subtree_match |=
                    Self::compute_visible_filter_matches(child, children_map, query, out);
            }
        }

        out.insert(path.to_path_buf(), subtree_match);
        subtree_match
    }

    fn collect_visible_node_paths(
        &self,
        parent: &Path,
        node_types: &HashMap<PathBuf, WorkspaceNodeType>,
        children_map: &HashMap<PathBuf, Vec<PathBuf>>,
        filter_query: Option<&str>,
        subtree_matches: &HashMap<PathBuf, bool>,
        out: &mut Vec<PathBuf>,
    ) {
        let Some(children) = children_map.get(parent) else {
            return;
        };

        for child in children {
            let file_type = node_types
                .get(child)
                .copied()
                .unwrap_or(WorkspaceNodeType::File);
            let subtree_match =
                filter_query.is_none_or(|_| subtree_matches.get(child).copied().unwrap_or(false));
            if !subtree_match {
                continue;
            }

            let has_matching_descendant = filter_query.is_some()
                && file_type == WorkspaceNodeType::Folder
                && children_map.get(child).is_some_and(|children| {
                    children
                        .iter()
                        .any(|nested| subtree_matches.get(nested).copied().unwrap_or(false))
                });
            let is_expanded = file_type == WorkspaceNodeType::Folder
                && (self.expanded_paths.contains(child) || has_matching_descendant);

            out.push(child.clone());

            if file_type == WorkspaceNodeType::Folder && is_expanded {
                self.collect_visible_node_paths(
                    child,
                    node_types,
                    children_map,
                    filter_query,
                    subtree_matches,
                    out,
                );
            }
        }
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
    fn expand_to_path_expands_ancestors_and_selects_file() {
        let root = std::env::temp_dir().join(format!(
            "netherize_workspace_expand_to_path_{}",
            std::process::id()
        ));
        let nested_dir = root.join("src/ui");
        let nested_file = nested_dir.join("tabs.rs");
        fs::create_dir_all(&nested_dir).expect("create nested dirs");
        fs::write(&nested_file, "pub fn tabs() {}\n").expect("write nested file");
        let canonical_root = root.canonicalize().expect("canonical root");
        let canonical_src = canonical_root.join("src");
        let canonical_nested_dir = canonical_root.join("src/ui");
        let canonical_nested_file = canonical_nested_dir.join("tabs.rs");

        let mut workspace = WorkspaceModel::load(root.clone()).expect("load workspace");
        assert!(workspace.expand_to_path(&nested_file));
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
    fn scroll_to_selected_node_keeps_selected_row_in_view() {
        let root = std::env::temp_dir().join(format!(
            "netherize_workspace_scroll_selected_{}",
            std::process::id()
        ));
        fs::create_dir_all(&root).expect("create root");
        for idx in 0..30 {
            fs::write(root.join(format!("file_{idx:02}.rs")), "fn main() {}\n")
                .expect("write file");
        }
        let canonical_root = root.canonicalize().expect("canonical root");
        let lower_file = canonical_root.join("file_25.rs");
        let upper_file = canonical_root.join("file_01.rs");

        let mut workspace = WorkspaceModel::load(root.clone()).expect("load workspace");
        assert!(workspace.expand_to_path(&lower_file));
        assert!(workspace.scroll_to_selected_node(50.0, 10.0));
        assert!(workspace.scroll_offset() > 0.0);

        assert!(workspace.expand_to_path(&upper_file));
        assert!(workspace.scroll_to_selected_node(50.0, 10.0));
        assert_eq!(workspace.scroll_offset(), 0.0);

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

    #[test]
    fn filter_keeps_matching_descendant_and_auto_expands_ancestor() {
        let root = std::env::temp_dir().join(format!(
            "netherize_workspace_filter_tree_{}",
            std::process::id()
        ));
        let nested_dir = root.join("src/ui");
        let match_file = nested_dir.join("filter_panel.rs");
        let other_file = root.join("README.md");
        fs::create_dir_all(&nested_dir).expect("create nested dirs");
        fs::write(&match_file, "pub fn panel() {}\n").expect("write match file");
        fs::write(&other_file, "# docs\n").expect("write other file");

        let canonical_root = root.canonicalize().expect("canonical root");
        let canonical_src = canonical_root.join("src");
        let canonical_ui = canonical_root.join("src/ui");
        let canonical_match_file = canonical_root.join("src/ui/filter_panel.rs");

        let mut workspace = WorkspaceModel::load(root.clone()).expect("load workspace");
        assert!(workspace.append_filter_text("panel"));
        assert!(workspace.has_active_filter());
        assert!(workspace.node_matches_filter(&canonical_match_file));
        assert!(!workspace.node_matches_filter(&other_file));
        assert!(!workspace.node_matches_filter(&canonical_src));
        assert!(!workspace.node_matches_filter(&canonical_ui));

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn clear_filter_resets_query_and_input_state() {
        let root = std::env::temp_dir().join(format!(
            "netherize_workspace_filter_clear_{}",
            std::process::id()
        ));
        fs::create_dir_all(root.join("src")).expect("create dirs");

        let mut workspace = WorkspaceModel::load(root.clone()).expect("load workspace");
        assert!(workspace.start_filter_input());
        assert!(workspace.append_filter_text("abc"));
        assert!(workspace.clear_filter());
        assert_eq!(workspace.filter_query(), "");
        assert!(!workspace.is_inputting_filter());

        let _ = fs::remove_dir_all(root);
    }
}
