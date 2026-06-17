//! Runtime state for the Code Graph HUD overlay (the `gp` graph view).

use crate::codegraph::model::{CodeGraphModel, GraphNode};
use crate::codegraph::navigation::{Focus, NavKey, navigate};

#[derive(Debug, Clone, PartialEq)]
pub enum CodeGraphHudStatus {
    Loading,
    Ready(CodeGraphModel),
    Empty,
    NotInstalled,
    Error(String),
}

/// A small, syntax-highlighted code snippet around the focused node's
/// definition, shown in the HUD's detail panel (hover-style preview). Built with
/// the same in-process highlighter as the fuzzy/references previews.
#[derive(Debug, Clone, PartialEq)]
pub struct NodeDetail {
    pub name: String,
    pub file_path: String,
    /// Target line (1-based) — the symbol's definition line.
    pub line: u32,
    /// Line number (1-based) of the first snippet row.
    pub start_line: u32,
    /// Snippet line texts (no trailing newline).
    pub lines: Vec<String>,
    /// Highlight spans over the snippet joined with `\n`.
    pub spans: Vec<crate::text::text_system::StyledTextSpan>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CodeGraphHudState {
    pub open: bool,
    pub status: CodeGraphHudStatus,
    pub focus: Focus,
    /// Focal symbol name, echoed in the HUD top bar while loading.
    pub focal_symbol: String,
    /// Code preview for the currently focused node (filled by the event loop).
    pub detail: Option<NodeDetail>,
}

impl Default for CodeGraphHudState {
    fn default() -> Self {
        Self {
            open: false,
            status: CodeGraphHudStatus::Loading,
            focus: Focus::Center,
            focal_symbol: String::new(),
            detail: None,
        }
    }
}

impl CodeGraphHudState {
    pub fn open_loading(&mut self, focal_symbol: String) {
        self.open = true;
        self.status = CodeGraphHudStatus::Loading;
        self.focus = Focus::Center;
        self.focal_symbol = focal_symbol;
        self.detail = None;
    }

    pub fn set_model(&mut self, model: CodeGraphModel) {
        self.focus = Focus::Center;
        self.detail = None;
        self.status = if model.is_empty() {
            CodeGraphHudStatus::Empty
        } else {
            CodeGraphHudStatus::Ready(model)
        };
    }

    pub fn set_not_installed(&mut self) {
        self.status = CodeGraphHudStatus::NotInstalled;
    }

    pub fn set_error(&mut self, message: String) {
        self.status = CodeGraphHudStatus::Error(message);
    }

    pub fn close(&mut self) {
        self.open = false;
    }

    /// Apply a navigation key; returns true if focus changed.
    pub fn nav(&mut self, key: NavKey) -> bool {
        let CodeGraphHudStatus::Ready(model) = &self.status else {
            return false;
        };
        let next = navigate(self.focus, key, model.callers.len(), model.callees.len());
        let changed = next != self.focus;
        self.focus = next;
        changed
    }

    /// The node currently focused, if the graph is ready.
    pub fn focused_node(&self) -> Option<&GraphNode> {
        let CodeGraphHudStatus::Ready(model) = &self.status else {
            return None;
        };
        match self.focus {
            Focus::Center => Some(&model.focal),
            Focus::Caller(i) => model.callers.get(i),
            Focus::Callee(i) => model.callees.get(i),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegraph::model::{CodeGraphModel, GraphNode, NodeRole, RiskLevel};

    fn node(name: &str, role: NodeRole) -> GraphNode {
        GraphNode {
            name: name.into(),
            kind: "fn".into(),
            file_path: "f.rs".into(),
            line: 1,
            role,
            risk: RiskLevel::Safe,
        }
    }

    fn ready_model() -> CodeGraphModel {
        CodeGraphModel {
            focal: node("validate", NodeRole::Center),
            callers: vec![node("login", NodeRole::Caller)],
            callees: vec![node("find", NodeRole::Callee)],
        }
    }

    #[test]
    fn empty_model_yields_empty_status() {
        let mut s = CodeGraphHudState::default();
        s.set_model(CodeGraphModel {
            focal: node("x", NodeRole::Center),
            callers: vec![],
            callees: vec![],
        });
        assert_eq!(s.status, CodeGraphHudStatus::Empty);
    }

    #[test]
    fn nav_left_from_center_focuses_first_caller() {
        let mut s = CodeGraphHudState::default();
        s.set_model(ready_model());
        assert!(s.nav(NavKey::Left));
        assert_eq!(s.focus, Focus::Caller(0));
        assert_eq!(s.focused_node().unwrap().name, "login");
    }
}
