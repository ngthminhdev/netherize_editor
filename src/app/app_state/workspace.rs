use super::*;
use crate::workspace::model::WorkspaceGitStatus;

impl AppState {
    pub fn attach_workspace(&mut self, root_path: PathBuf) -> Result<(), String> {
        let workspace = WorkspaceModel::load(root_path)?;
        self.workspace_model = Some(workspace);
        Ok(())
    }

    pub fn clear_workspace_session_state(&mut self) -> bool {
        let mut changed = false;

        changed |= !self.diagnostics.is_empty();
        self.diagnostics.clear();

        changed |= !self.buffers.is_empty();
        self.buffers.clear();

        changed |= self.active_buffer_index.is_some();
        self.active_buffer_index = None;

        changed |= self.command_palette.is_visible;
        changed |= self.close_command_palette();

        changed |= self.pending_explorer_rename_path.is_some();
        self.pending_explorer_rename_path = None;

        changed |= !self.jump_back_stack.is_empty() || !self.jump_forward_stack.is_empty();
        self.jump_back_stack.clear();
        self.jump_forward_stack.clear();

        let had_active_file = self.active_file.is_some();
        let had_text = self.text.len_chars() > 0;
        let had_dirty = self.dirty;
        self.reset_text_editor_state();
        changed |= had_active_file || had_text || had_dirty;

        changed |= self.clear_current_overlays();

        if changed {
            self.bump_revision();
        }

        changed
    }

    pub fn workspace_root_path(&self) -> Option<&Path> {
        self.workspace_model
            .as_ref()
            .map(|model| model.root_path.as_path())
    }

    pub fn workspace_nodes(&self) -> Option<&[crate::workspace::model::WorkspaceNode]> {
        self.workspace_model.as_ref().map(|m| m.nodes.as_slice())
    }

    pub fn workspace_git_status(&self, path: &Path) -> Option<WorkspaceGitStatus> {
        self.workspace_model
            .as_ref()
            .and_then(|model| model.git_status_for_path(path))
    }

    pub fn workspace_set_git_statuses(
        &mut self,
        statuses: HashMap<PathBuf, WorkspaceGitStatus>,
    ) -> bool {
        self.workspace_model
            .as_mut()
            .is_some_and(|model| model.set_git_statuses(statuses))
    }

    pub fn set_buffer_git_baseline(&mut self, path: &Path, baseline: Option<String>) -> bool {
        let Some(index) = self.buffers.iter().position(
            |buffer| matches!(&buffer.content, BufferContent::Text(text) if text.path == path),
        ) else {
            return false;
        };
        let BufferContent::Text(buffer) = &mut self.buffers[index].content else {
            return false;
        };
        if buffer.git_baseline == baseline {
            return false;
        }
        buffer.git_baseline = baseline;
        true
    }

    pub fn recalculate_active_buffer_git_diff(&mut self) -> bool {
        let Some(idx) = self.active_buffer_index else {
            return false;
        };
        let Some(entry) = self.buffers.get_mut(idx) else {
            return false;
        };
        let BufferContent::Text(buffer) = &mut entry.content else {
            return false;
        };
        let current_text = self.text.to_string();
        recalculate_git_diff(buffer, &current_text)
    }
}

pub fn recalculate_git_diff(buffer: &mut EditorBuffer, current_text: &str) -> bool {
    let Some(baseline) = buffer.git_baseline.as_deref() else {
        if buffer.git_line_statuses.is_empty() {
            return false;
        }
        buffer.git_line_statuses.clear();
        return true;
    };

    let new_statuses = compute_git_line_statuses(baseline, current_text);
    if buffer.git_line_statuses == new_statuses {
        return false;
    }
    buffer.git_line_statuses = new_statuses;
    true
}

fn compute_git_line_statuses(baseline: &str, current: &str) -> HashMap<usize, GitLineStatus> {
    use similar::{ChangeTag, TextDiff};

    let diff = TextDiff::from_lines(baseline, current);
    let mut statuses: HashMap<usize, GitLineStatus> = HashMap::new();

    let mut in_delete_block = false;
    let mut in_modify_block = false;
    let mut last_new_idx: usize = 0;

    for change in diff.iter_all_changes() {
        match change.tag() {
            ChangeTag::Equal => {
                if in_delete_block && !in_modify_block {
                    if let Some(idx) = change.new_index() {
                        statuses.entry(idx).or_insert(GitLineStatus::DeletedAbove);
                    }
                }
                in_delete_block = false;
                in_modify_block = false;
                if let Some(idx) = change.new_index() {
                    last_new_idx = idx;
                }
            }
            ChangeTag::Delete => {
                in_delete_block = true;
            }
            ChangeTag::Insert => {
                let idx = change.new_index().unwrap_or(last_new_idx + 1);
                last_new_idx = idx;
                if in_delete_block {
                    in_modify_block = true;
                    statuses.insert(idx, GitLineStatus::Modified);
                } else {
                    statuses.entry(idx).or_insert(GitLineStatus::Added);
                }
            }
        }
    }

    // Trailing deletion at end of file
    if in_delete_block && !in_modify_block {
        statuses
            .entry(last_new_idx)
            .or_insert(GitLineStatus::DeletedBelow);
    }

    statuses
}

#[cfg(test)]
mod tests {
    use super::{GitLineStatus, compute_git_line_statuses};

    #[test]
    fn git_line_statuses_use_new_buffer_indexes_for_inserts() {
        let statuses = compute_git_line_statuses("a\nb\n", "a\n\nb\n");

        assert!(matches!(statuses.get(&1), Some(GitLineStatus::Added)));
        assert!(!statuses.contains_key(&2));
    }

    #[test]
    fn git_line_statuses_mark_replacement_as_modified() {
        let statuses = compute_git_line_statuses("a\nold\nc\n", "a\nnew\nc\n");

        assert!(matches!(statuses.get(&1), Some(GitLineStatus::Modified)));
    }

    #[test]
    fn git_line_statuses_anchor_deleted_lines_to_current_buffer() {
        let middle = compute_git_line_statuses("a\nremoved\nc\n", "a\nc\n");
        assert!(matches!(middle.get(&1), Some(GitLineStatus::DeletedAbove)));

        let trailing = compute_git_line_statuses("a\nremoved\n", "a\n");
        assert!(matches!(
            trailing.get(&0),
            Some(GitLineStatus::DeletedBelow)
        ));
    }
}

// Re-open impl block for workspace methods that follow
impl AppState {
    pub fn workspace_selected_path(&self) -> Option<&Path> {
        self.workspace_model
            .as_ref()
            .and_then(WorkspaceModel::selected_path)
    }

    pub fn workspace_filter_query(&self) -> Option<&str> {
        self.workspace_model
            .as_ref()
            .map(WorkspaceModel::filter_query)
    }

    pub fn workspace_show_hidden(&self) -> bool {
        self.workspace_model
            .as_ref()
            .is_some_and(WorkspaceModel::show_hidden)
    }

    pub fn workspace_show_ignored(&self) -> bool {
        self.workspace_model
            .as_ref()
            .is_some_and(WorkspaceModel::show_ignored)
    }

    pub fn workspace_visible_node_paths(&self) -> Vec<PathBuf> {
        self.workspace_model
            .as_ref()
            .map(WorkspaceModel::visible_node_paths)
            .unwrap_or_default()
    }

    pub fn workspace_has_active_filter(&self) -> bool {
        self.workspace_model
            .as_ref()
            .is_some_and(WorkspaceModel::has_active_filter)
    }

    pub fn workspace_is_inputting_filter(&self) -> bool {
        self.workspace_model
            .as_ref()
            .is_some_and(WorkspaceModel::is_inputting_filter)
    }

    pub fn workspace_start_filter_input(&mut self) -> bool {
        self.workspace_model
            .as_mut()
            .is_some_and(WorkspaceModel::start_filter_input)
    }

    pub fn workspace_stop_filter_input(&mut self) -> bool {
        self.workspace_model
            .as_mut()
            .is_some_and(WorkspaceModel::stop_filter_input)
    }

    pub fn workspace_clear_filter(&mut self) -> bool {
        self.workspace_model
            .as_mut()
            .is_some_and(WorkspaceModel::clear_filter)
    }

    pub fn workspace_append_filter_text(&mut self, text: &str) -> bool {
        self.workspace_model
            .as_mut()
            .is_some_and(|workspace| workspace.append_filter_text(text))
    }

    pub fn workspace_backspace_filter(&mut self) -> bool {
        self.workspace_model
            .as_mut()
            .is_some_and(WorkspaceModel::backspace_filter)
    }

    pub fn workspace_is_expanded(&self, path: &Path) -> bool {
        self.workspace_model
            .as_ref()
            .is_some_and(|workspace| workspace.is_expanded(path))
    }

    pub fn workspace_select_path(&mut self, path: &Path) -> bool {
        self.workspace_model
            .as_mut()
            .is_some_and(|workspace| workspace.select_path(path))
    }

    pub fn workspace_expand_path(&mut self, path: &Path) -> bool {
        self.workspace_model
            .as_mut()
            .is_some_and(|workspace| workspace.expand_path(path))
    }

    pub fn workspace_collapse_path(&mut self, path: &Path) -> bool {
        self.workspace_model
            .as_mut()
            .is_some_and(|workspace| workspace.collapse_path(path))
    }

    pub fn workspace_collapse_path_and_descendants(&mut self, path: &Path) -> bool {
        self.workspace_model
            .as_mut()
            .is_some_and(|workspace| workspace.collapse_path_and_descendants(path))
    }

    pub fn workspace_expand_path_and_descendants(&mut self, path: &Path) -> bool {
        self.workspace_model
            .as_mut()
            .is_some_and(|workspace| workspace.expand_path_and_descendants(path))
    }

    pub fn workspace_expand_to_path(&mut self, path: &Path) -> bool {
        self.workspace_model
            .as_mut()
            .is_some_and(|workspace| workspace.expand_to_path(path))
    }

    pub fn workspace_reveal_path(&mut self, path: &Path) -> bool {
        self.workspace_expand_to_path(path)
    }

    pub fn workspace_scroll_to_selected_node(
        &mut self,
        viewport_height: f32,
        line_height: f32,
    ) -> bool {
        self.workspace_model.as_mut().is_some_and(|workspace| {
            workspace.scroll_to_selected_node(viewport_height, line_height)
        })
    }

    pub fn workspace_scroll_offset_rows(&self, line_height: f32) -> usize {
        self.workspace_model
            .as_ref()
            .map_or(0, |workspace| workspace.scroll_offset_rows(line_height))
    }

    pub fn rescan_workspace(&mut self) -> Result<bool, String> {
        let Some(workspace) = self.workspace_model.as_mut() else {
            return Ok(false);
        };
        workspace.rescan()?;
        if self.is_file_picker_open() {
            let _ = self.refresh_file_picker_results_if_open()?;
        }
        Ok(true)
    }

    pub fn workspace_toggle_show_hidden(&mut self) -> bool {
        self.workspace_model
            .as_mut()
            .is_some_and(WorkspaceModel::toggle_show_hidden)
    }

    pub fn workspace_toggle_show_ignored(&mut self) -> bool {
        self.workspace_model
            .as_mut()
            .is_some_and(WorkspaceModel::toggle_show_ignored)
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
}
