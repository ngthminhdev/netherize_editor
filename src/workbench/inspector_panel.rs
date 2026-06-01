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
        let collapsed_h = 26.0_f32;
        let total_gap = header_gap * (self.sections.len().saturating_sub(1) as f32);

        let collapsed_count = self.sections.iter().filter(|s| !s.expanded).count();
        let expanded_count = self.sections.len() - collapsed_count;

        let collapsed_total_h = collapsed_count as f32 * collapsed_h;
        let remaining_h = (sidebar_bounds.height - total_gap - collapsed_total_h).max(0.0);

        let expanded_h = if expanded_count > 0 {
            (remaining_h / expanded_count as f32).max(20.0)
        } else {
            0.0
        };

        let mut surfaces = Vec::with_capacity(self.sections.len());
        let mut current_y = sidebar_bounds.y;

        for section in &self.sections {
            let section_h = if section.expanded { expanded_h } else { collapsed_h };
            surfaces.push(InspectorSectionSurface {
                kind: section.kind,
                bounds: RegionBounds::new(sidebar_bounds.x, current_y, sidebar_bounds.width, section_h),
                selected: selected_section.is_some_and(|kind| kind == section.kind),
            });
            current_y += section_h + header_gap;
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
    use crate::workbench::debug_state::{
        DebugSharedState, DebugVariable, SourceLocation, StackFrame, WatchEntry,
    };

    use super::*;

    #[test]
    fn inspector_rows_support_navigation_and_toggle() {
        let mut state = InspectorPanelState::default();
        state.sync_from_debug_state(&DebugSharedState::default());

        // Row đầu tiên là section header nên luôn có thể expand/collapse.
        assert!(state.toggle_selected_expand());
        assert!(state.move_selection_next());
        assert!(state.selected_row_label().is_some());
    }

    #[test]
    fn sync_populates_variables_section() {
        let mut debug = DebugSharedState::default();
        debug.variables = vec![
            DebugVariable {
                name: "x".to_string(),
                value: "42".to_string(),
                children: Vec::new(),
            },
            DebugVariable {
                name: "y".to_string(),
                value: "hello".to_string(),
                children: vec![DebugVariable {
                    name: "child".to_string(),
                    value: "nested".to_string(),
                    children: Vec::new(),
                }],
            },
        ];

        let mut state = InspectorPanelState::default();
        state.sync_from_debug_state(&debug);

        let var_section = &state.sections[0];
        assert_eq!(var_section.nodes.len(), 2);
        assert_eq!(var_section.nodes[0].label, "x = 42");
        assert!(var_section.nodes[0].children.is_empty());
        assert_eq!(var_section.nodes[1].label, "y = hello");
        assert_eq!(var_section.nodes[1].children.len(), 1);
        assert!(var_section.nodes[1].expanded); // branch nodes default to expanded
    }

    #[test]
    fn sync_populates_watch_section() {
        let mut debug = DebugSharedState::default();
        debug.watch = vec![
            WatchEntry {
                expression: "x + 1".to_string(),
                value: "43".to_string(),
            },
            WatchEntry {
                expression: "items.len()".to_string(),
                value: "5".to_string(),
            },
        ];

        let mut state = InspectorPanelState::default();
        state.sync_from_debug_state(&debug);

        let watch_section = &state.sections[1];
        assert_eq!(watch_section.nodes.len(), 2);
        assert_eq!(watch_section.nodes[0].label, "x + 1 = 43");
        assert_eq!(watch_section.nodes[1].label, "items.len() = 5");
    }

    #[test]
    fn sync_populates_call_stack_section() {
        let mut debug = DebugSharedState::default();
        debug.call_stack = vec![
            StackFrame {
                function: "main".to_string(),
                location: SourceLocation { line: 0, column: 0 },
                path: std::path::PathBuf::from("main.dart"),
            },
            StackFrame {
                function: "build".to_string(),
                location: SourceLocation { line: 41, column: 8 },
                path: std::path::PathBuf::from("widget.dart"),
            },
        ];

        let mut state = InspectorPanelState::default();
        state.sync_from_debug_state(&debug);

        let stack_section = &state.sections[2];
        assert_eq!(stack_section.nodes.len(), 2);
        assert!(stack_section.nodes[0].label.contains("main"));
        assert!(stack_section.nodes[0].label.contains("1:1")); // 0-indexed + 1
        assert!(stack_section.nodes[1].label.contains("build"));
        assert!(stack_section.nodes[1].label.contains("42:9")); // line+1, col+1
    }

    #[test]
    fn sync_populates_breakpoints_section() {
        let mut debug = DebugSharedState::default();
        debug.toggle_breakpoint_at_line(&std::path::PathBuf::from("main.dart"), 5);
        debug.toggle_breakpoint_at_line(&std::path::PathBuf::from("main.dart"), 10);

        let mut state = InspectorPanelState::default();
        state.sync_from_debug_state(&debug);

        let bp_section = &state.sections[3];
        assert_eq!(bp_section.nodes.len(), 2);
        assert!(bp_section.nodes[0].label.contains("line 6")); // 0-indexed + 1
        assert!(bp_section.nodes[1].label.contains("line 11"));
    }

    #[test]
    fn sync_clamps_selected_row_when_shrinking() {
        let mut debug = DebugSharedState::default();
        debug.variables = vec![
            DebugVariable {
                name: "a".to_string(),
                value: "1".to_string(),
                children: Vec::new(),
            },
            DebugVariable {
                name: "b".to_string(),
                value: "2".to_string(),
                children: Vec::new(),
            },
        ];

        let mut state = InspectorPanelState::default();
        state.sync_from_debug_state(&debug);
        state.selected_row = 100; // Way out of bounds

        // Sync again with empty data
        state.sync_from_debug_state(&DebugSharedState::default());
        let rows = state.visible_rows();
        assert!(state.selected_row < rows.len());
    }

    #[test]
    fn visible_rows_include_section_headers_and_items() {
        let mut debug = DebugSharedState::default();
        debug.variables = vec![DebugVariable {
            name: "x".to_string(),
            value: "1".to_string(),
            children: Vec::new(),
        }];

        let mut state = InspectorPanelState::default();
        state.sync_from_debug_state(&debug);

        let rows = state.visible_rows();
        // At minimum: 4 section headers + 1 variable
        assert!(rows.len() >= 5);

        // First row should be Variables header
        assert_eq!(rows[0].label, "Variables");
        assert!(rows[0].node_path.is_none()); // Section header
        assert_eq!(rows[0].depth, 0);

        // Second row should be the variable
        assert_eq!(rows[1].label, "x = 1");
        assert!(rows[1].node_path.is_some());
        assert_eq!(rows[1].depth, 1);
    }

    #[test]
    fn collapsed_section_hides_children() {
        let mut debug = DebugSharedState::default();
        debug.variables = vec![DebugVariable {
            name: "x".to_string(),
            value: "1".to_string(),
            children: Vec::new(),
        }];

        let mut state = InspectorPanelState::default();
        state.sync_from_debug_state(&debug);

        let rows_expanded = state.visible_rows();
        let expanded_count = rows_expanded.len();

        // Collapse the Variables section
        state.sections[0].expanded = false;
        let rows_collapsed = state.visible_rows();
        assert!(rows_collapsed.len() < expanded_count);
    }

    #[test]
    fn toggle_expand_on_section_header() {
        let mut state = InspectorPanelState::default();
        state.sync_from_debug_state(&DebugSharedState::default());

        let was_expanded = state.sections[0].expanded;
        state.selected_row = 0; // First row is Variables header
        assert!(state.toggle_selected_expand());
        assert_eq!(state.sections[0].expanded, !was_expanded);
    }

    #[test]
    fn toggle_expand_on_node_with_children() {
        let mut debug = DebugSharedState::default();
        debug.variables = vec![DebugVariable {
            name: "list".to_string(),
            value: "List(2)".to_string(),
            children: vec![
                DebugVariable {
                    name: "[0]".to_string(),
                    value: "a".to_string(),
                    children: Vec::new(),
                },
                DebugVariable {
                    name: "[1]".to_string(),
                    value: "b".to_string(),
                    children: Vec::new(),
                },
            ],
        }];

        let mut state = InspectorPanelState::default();
        state.sync_from_debug_state(&debug);

        // Find the row for "list = List(2)"
        let rows = state.visible_rows();
        let list_row_idx = rows.iter().position(|r| r.label.contains("list")).unwrap();
        state.selected_row = list_row_idx;

        // Collapse it
        assert!(state.toggle_selected_expand());
        let rows_after = state.visible_rows();
        // Should have fewer rows now (the children are hidden)
        assert!(rows_after.len() < rows.len());
    }

    #[test]
    fn toggle_expand_on_leaf_node_returns_false() {
        let mut debug = DebugSharedState::default();
        debug.variables = vec![DebugVariable {
            name: "x".to_string(),
            value: "42".to_string(),
            children: Vec::new(),
        }];

        let mut state = InspectorPanelState::default();
        state.sync_from_debug_state(&debug);

        // Find the leaf node row
        let rows = state.visible_rows();
        let leaf_idx = rows.iter().position(|r| r.label.contains("x = 42")).unwrap();
        state.selected_row = leaf_idx;

        // Toggle on leaf should return false (no children)
        assert!(!state.toggle_selected_expand());
    }

    #[test]
    fn section_surfaces_compute_dynamic_height() {
        let mut state = InspectorPanelState::default();
        state.sync_from_debug_state(&DebugSharedState::default());

        let bounds = RegionBounds::new(0.0, 0.0, 300.0, 800.0);
        let surfaces = state.section_surfaces(bounds, None);

        assert_eq!(surfaces.len(), 4);
        // All sections are expanded by default, so each should get ~1/4 of the height
        for surface in &surfaces {
            assert!(surface.bounds.height > 0.0);
        }
    }

    #[test]
    fn section_surfaces_collapsed_gets_fixed_height() {
        let mut state = InspectorPanelState::default();
        state.sections[0].expanded = false; // Collapse Variables

        let bounds = RegionBounds::new(0.0, 0.0, 300.0, 800.0);
        let surfaces = state.section_surfaces(bounds, None);

        // Collapsed section should have height ~26.0
        assert!((surfaces[0].bounds.height - 26.0).abs() < 1.0);
    }

    #[test]
    fn move_selection_wraps_correctly() {
        let mut state = InspectorPanelState::default();
        state.sync_from_debug_state(&DebugSharedState::default());

        let rows = state.visible_rows();
        let total = rows.len();

        // Move to end
        for _ in 0..total + 5 {
            state.move_selection_next();
        }
        assert_eq!(state.selected_row, total - 1);

        // Move back to start
        for _ in 0..total + 5 {
            state.move_selection_prev();
        }
        assert_eq!(state.selected_row, 0);
    }

    #[test]
    fn selected_row_label_returns_current_row_text() {
        let mut debug = DebugSharedState::default();
        debug.variables = vec![DebugVariable {
            name: "test_var".to_string(),
            value: "999".to_string(),
            children: Vec::new(),
        }];

        let mut state = InspectorPanelState::default();
        state.sync_from_debug_state(&debug);

        // Navigate to the variable row (skip Variables header)
        state.move_selection_next();
        let label = state.selected_row_label();
        assert!(label.is_some());
        assert!(label.unwrap().contains("test_var"));
    }
}
