use crate::workbench::{
    debug_state::{DebugSharedState, DebugVariable},
    region_model::RegionBounds,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum InspectorSectionKind {
    Variables,
    Watch,
    CallStack,
    Breakpoints,
}

impl InspectorSectionKind {
    pub fn title(self) -> &'static str {
        match self {
            Self::Variables => "Variables",
            Self::Watch => "Watch",
            Self::CallStack => "Call Stack",
            Self::Breakpoints => "Breakpoints",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct InspectorNode {
    pub label: String,
    pub expanded: bool,
    pub children: Vec<InspectorNode>,
}

impl InspectorNode {
    pub fn leaf(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            expanded: false,
            children: Vec::new(),
        }
    }

    pub fn branch(label: impl Into<String>, children: Vec<InspectorNode>) -> Self {
        Self {
            label: label.into(),
            expanded: true,
            children,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct InspectorSection {
    pub kind: InspectorSectionKind,
    pub expanded: bool,
    pub nodes: Vec<InspectorNode>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct InspectorVisibleRow {
    pub section_index: usize,
    pub node_path: Option<Vec<usize>>,
    pub depth: usize,
    pub label: String,
    pub expandable: bool,
    pub expanded: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct InspectorSectionSurface {
    pub kind: InspectorSectionKind,
    pub bounds: RegionBounds,
    pub selected: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct InspectorPanelState {
    pub sections: Vec<InspectorSection>,
    pub selected_row: usize,
}

impl Default for InspectorPanelState {
    fn default() -> Self {
        Self {
            sections: vec![
                InspectorSection {
                    kind: InspectorSectionKind::Variables,
                    expanded: true,
                    nodes: Vec::new(),
                },
                InspectorSection {
                    kind: InspectorSectionKind::Watch,
                    expanded: true,
                    nodes: Vec::new(),
                },
                InspectorSection {
                    kind: InspectorSectionKind::CallStack,
                    expanded: true,
                    nodes: Vec::new(),
                },
                InspectorSection {
                    kind: InspectorSectionKind::Breakpoints,
                    expanded: true,
                    nodes: Vec::new(),
                },
            ],
            selected_row: 0,
        }
    }
}

impl InspectorPanelState {
    pub fn sync_from_debug_state(&mut self, debug: &DebugSharedState) {
        for section in &mut self.sections {
            match section.kind {
                InspectorSectionKind::Variables => {
                    section.nodes = debug
                        .variables
                        .iter()
                        .map(variable_to_node)
                        .collect::<Vec<_>>();
                }
                InspectorSectionKind::Watch => {
                    section.nodes = debug
                        .watch
                        .iter()
                        .map(|watch| {
                            InspectorNode::leaf(format!("{} = {}", watch.expression, watch.value))
                        })
                        .collect::<Vec<_>>();
                }
                InspectorSectionKind::CallStack => {
                    section.nodes = debug
                        .call_stack
                        .iter()
                        .enumerate()
                        .map(|(idx, frame)| {
                            InspectorNode::leaf(format!(
                                "{}. {} ({}:{})",
                                idx + 1,
                                frame.function,
                                frame.location.line + 1,
                                frame.location.column + 1
                            ))
                        })
                        .collect::<Vec<_>>();
                }
                InspectorSectionKind::Breakpoints => {
                    section.nodes = debug
                        .breakpoints
                        .iter()
                        .map(|bp| {
                            InspectorNode::leaf(format!(
                                "#{} line {} ({})",
                                bp.id,
                                bp.location.line + 1,
                                if bp.enabled { "enabled" } else { "disabled" }
                            ))
                        })
                        .collect::<Vec<_>>();
                }
            }
        }

        let rows = self.visible_rows();
        if rows.is_empty() {
            self.selected_row = 0;
        } else if self.selected_row >= rows.len() {
            self.selected_row = rows.len() - 1;
        }
    }

    pub fn visible_rows(&self) -> Vec<InspectorVisibleRow> {
        let mut out = Vec::new();
        for (section_index, section) in self.sections.iter().enumerate() {
            out.push(InspectorVisibleRow {
                section_index,
                node_path: None,
                depth: 0,
                label: section.kind.title().to_string(),
                expandable: true,
                expanded: section.expanded,
            });
            if !section.expanded {
                continue;
            }
            for (node_index, node) in section.nodes.iter().enumerate() {
                let mut path = vec![node_index];
                push_node_row(&mut out, section_index, 1, &path, node);
                if node.expanded {
                    push_visible_children(&mut out, section_index, 2, &mut path, node);
                }
            }
        }
        out
    }

    pub fn move_selection_next(&mut self) -> bool {
        let rows = self.visible_rows();
        if rows.is_empty() {
            return false;
        }
        if self.selected_row + 1 >= rows.len() {
            return false;
        }
        self.selected_row += 1;
        true
    }

    pub fn move_selection_prev(&mut self) -> bool {
        if self.selected_row == 0 {
            return false;
        }
        self.selected_row -= 1;
        true
    }

    pub fn toggle_selected_expand(&mut self) -> bool {
        let rows = self.visible_rows();
        let Some(row) = rows.get(self.selected_row) else {
            return false;
        };

        match &row.node_path {
            None => {
                let section = &mut self.sections[row.section_index];
                section.expanded = !section.expanded;
                true
            }
            Some(path) => {
                let Some(node) = self.node_mut(row.section_index, path) else {
                    return false;
                };
                if node.children.is_empty() {
                    return false;
                }
                node.expanded = !node.expanded;
                true
            }
        }
    }

    pub fn selected_row_label(&self) -> Option<String> {
        self.visible_rows()
            .get(self.selected_row)
            .map(|row| row.label.clone())
    }

    pub fn section_surfaces(
        &self,
        sidebar_bounds: RegionBounds,
        selected_section: Option<InspectorSectionKind>,
    ) -> Vec<InspectorSectionSurface> {
        if sidebar_bounds.width <= 0.0 || sidebar_bounds.height <= 0.0 || self.sections.is_empty() {
            return Vec::new();
        }

        let header_gap = 6.0;
        let total_gap = header_gap * (self.sections.len().saturating_sub(1) as f32);
        let section_h =
            ((sidebar_bounds.height - total_gap) / self.sections.len() as f32).max(20.0);

        let mut surfaces = Vec::with_capacity(self.sections.len());
        for (index, section) in self.sections.iter().enumerate() {
            let y = sidebar_bounds.y + index as f32 * (section_h + header_gap);
            surfaces.push(InspectorSectionSurface {
                kind: section.kind,
                bounds: RegionBounds::new(sidebar_bounds.x, y, sidebar_bounds.width, section_h),
                selected: selected_section.is_some_and(|kind| kind == section.kind),
            });
        }
        surfaces
    }

    fn node_mut(&mut self, section_index: usize, path: &[usize]) -> Option<&mut InspectorNode> {
        let section = self.sections.get_mut(section_index)?;
        node_mut_from_slice(&mut section.nodes, path)
    }
}

fn variable_to_node(var: &DebugVariable) -> InspectorNode {
    if var.children.is_empty() {
        return InspectorNode::leaf(format!("{} = {}", var.name, var.value));
    }
    InspectorNode::branch(
        format!("{} = {}", var.name, var.value),
        var.children.iter().map(variable_to_node).collect(),
    )
}

fn push_visible_children(
    rows: &mut Vec<InspectorVisibleRow>,
    section_index: usize,
    depth: usize,
    current_path: &mut Vec<usize>,
    node: &InspectorNode,
) {
    for (idx, child) in node.children.iter().enumerate() {
        current_path.push(idx);
        push_node_row(rows, section_index, depth, current_path, child);
        if child.expanded {
            push_visible_children(rows, section_index, depth + 1, current_path, child);
        }
        current_path.pop();
    }
}

fn push_node_row(
    rows: &mut Vec<InspectorVisibleRow>,
    section_index: usize,
    depth: usize,
    path: &[usize],
    node: &InspectorNode,
) {
    rows.push(InspectorVisibleRow {
        section_index,
        node_path: Some(path.to_vec()),
        depth,
        label: node.label.clone(),
        expandable: !node.children.is_empty(),
        expanded: node.expanded,
    });
}

fn node_mut_from_slice<'a>(
    nodes: &'a mut [InspectorNode],
    path: &[usize],
) -> Option<&'a mut InspectorNode> {
    let (&head, tail) = path.split_first()?;
    let node = nodes.get_mut(head)?;
    if tail.is_empty() {
        return Some(node);
    }
    node_mut_from_slice(&mut node.children, tail)
}

#[cfg(test)]
mod tests {
    use crate::workbench::debug_state::DebugSharedState;

    use super::InspectorPanelState;

    #[test]
    fn inspector_rows_support_navigation_and_toggle() {
        let mut state = InspectorPanelState::default();
        state.sync_from_debug_state(&DebugSharedState::default());

        // Row đầu tiên là section header nên luôn có thể expand/collapse.
        assert!(state.toggle_selected_expand());
        assert!(state.move_selection_next());
        assert!(state.selected_row_label().is_some());
    }
}
