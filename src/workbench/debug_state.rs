#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SourceLocation {
    pub line: usize,
    pub column: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Breakpoint {
    pub id: u64,
    pub location: SourceLocation,
    pub enabled: bool,
    pub path: std::path::PathBuf,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DebugVariable {
    pub name: String,
    pub value: String,
    pub children: Vec<DebugVariable>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct WatchEntry {
    pub expression: String,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct StackFrame {
    pub function: String,
    pub location: SourceLocation,
    pub path: std::path::PathBuf,
}

#[derive(Debug, Clone, PartialEq)]
pub struct InlineValue {
    pub location: SourceLocation,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DebugSharedState {
    pub paused: bool,
    pub execution_location: Option<SourceLocation>,
    pub breakpoints: Vec<Breakpoint>,
    pub variables: Vec<DebugVariable>,
    pub watch: Vec<WatchEntry>,
    pub call_stack: Vec<StackFrame>,
    pub inline_values: Vec<InlineValue>,
    pub console_messages: Vec<String>,
    pub terminated: bool,
    next_breakpoint_id: u64,
}

impl Default for DebugSharedState {
    fn default() -> Self {
        Self {
            paused: false,
            execution_location: None,
            breakpoints: Vec::new(),
            variables: Vec::new(),
            watch: Vec::new(),
            call_stack: Vec::new(),
            inline_values: Vec::new(),
            console_messages: Vec::new(),
            terminated: false,
            next_breakpoint_id: 1,
        }
    }
}

impl DebugSharedState {
    pub fn toggle_paused(&mut self) {
        self.paused = !self.paused;
        self.console_messages.push(if self.paused {
            "debugger paused".to_string()
        } else {
            "debugger resumed".to_string()
        });
    }

    pub fn move_execution_down(&mut self, max_line: usize) -> bool {
        let Some(mut loc) = self.execution_location else {
            return false;
        };
        if loc.line + 1 >= max_line {
            return false;
        }
        loc.line += 1;
        self.execution_location = Some(loc);
        true
    }

    pub fn move_execution_up(&mut self) -> bool {
        let Some(mut loc) = self.execution_location else {
            return false;
        };
        if loc.line == 0 {
            return false;
        }
        loc.line -= 1;
        self.execution_location = Some(loc);
        true
    }

    pub fn toggle_breakpoint_on_execution_line(&mut self, path: &std::path::Path) -> bool {
        let Some(loc) = self.execution_location else {
            return false;
        };
        self.toggle_breakpoint_at_line(path, loc.line)
    }

    pub fn toggle_breakpoint_at_line(&mut self, path: &std::path::Path, line: usize) -> bool {
        if let Some(index) = self
            .breakpoints
            .iter()
            .position(|bp| bp.path == path && bp.location.line == line)
        {
            self.breakpoints.remove(index);
            self.console_messages
                .push(format!("breakpoint removed at line {}", line + 1));
            return true;
        }

        self.breakpoints.push(Breakpoint {
            id: self.next_breakpoint_id,
            location: SourceLocation { line, column: 0 },
            enabled: true,
            path: path.to_path_buf(),
        });
        self.next_breakpoint_id += 1;
        self.console_messages
            .push(format!("breakpoint added at line {}", line + 1));
        true
    }

    pub fn add_watch_expression(&mut self, expression: String) {
        if self.watch.iter().any(|w| w.expression == expression) {
            return;
        }
        self.watch.push(WatchEntry {
            expression,
            value: String::new(),
        });
    }

    pub fn remove_watch_expression(&mut self, index: usize) -> bool {
        if index < self.watch.len() {
            self.watch.remove(index);
            return true;
        }
        false
    }

    pub fn update_watch_values(&mut self, values: Vec<(String, String)>) {
        for (expr, value) in values {
            if let Some(entry) = self.watch.iter_mut().find(|w| w.expression == expr) {
                entry.value = value;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn toggle_breakpoint_adds_and_removes_on_same_line() {
        let mut state = DebugSharedState::default();
        let initial = state.breakpoints.len();
        state.toggle_breakpoint_at_line(&std::path::PathBuf::new(), 25);
        assert_eq!(state.breakpoints.len(), initial + 1);
        state.toggle_breakpoint_at_line(&std::path::PathBuf::new(), 25);
        assert_eq!(state.breakpoints.len(), initial);
    }

    #[test]
    fn default_state_is_empty() {
        let state = DebugSharedState::default();
        assert!(!state.paused);
        assert!(state.execution_location.is_none());
        assert!(state.breakpoints.is_empty());
        assert!(state.variables.is_empty());
        assert!(state.watch.is_empty());
        assert!(state.call_stack.is_empty());
        assert!(state.inline_values.is_empty());
        assert!(state.console_messages.is_empty());
        assert!(!state.terminated);
    }

    #[test]
    fn toggle_breakpoint_tracks_path_correctly() {
        let mut state = DebugSharedState::default();
        let path_a = std::path::PathBuf::from("/src/main.dart");
        let path_b = std::path::PathBuf::from("/src/lib.rs");

        state.toggle_breakpoint_at_line(&path_a, 10);
        state.toggle_breakpoint_at_line(&path_b, 10);
        assert_eq!(state.breakpoints.len(), 2);

        // Same line, different path -> both stay
        state.toggle_breakpoint_at_line(&path_a, 10);
        assert_eq!(state.breakpoints.len(), 1);
        assert_eq!(state.breakpoints[0].path, path_b);
    }

    #[test]
    fn toggle_breakpoint_assigns_incrementing_ids() {
        let mut state = DebugSharedState::default();
        let path = std::path::PathBuf::from("main.dart");

        state.toggle_breakpoint_at_line(&path, 5);
        state.toggle_breakpoint_at_line(&path, 10);
        state.toggle_breakpoint_at_line(&path, 15);

        assert_eq!(state.breakpoints[0].id, 1);
        assert_eq!(state.breakpoints[1].id, 2);
        assert_eq!(state.breakpoints[2].id, 3);
    }

    #[test]
    fn toggle_breakpoint_on_execution_line() {
        let mut state = DebugSharedState::default();
        state.execution_location = Some(SourceLocation {
            line: 42,
            column: 0,
        });
        let path = std::path::PathBuf::from("main.dart");

        assert!(state.toggle_breakpoint_on_execution_line(&path));
        assert_eq!(state.breakpoints.len(), 1);
        assert_eq!(state.breakpoints[0].location.line, 42);
    }

    #[test]
    fn toggle_breakpoint_on_execution_line_returns_false_when_no_location() {
        let mut state = DebugSharedState::default();
        state.execution_location = None;
        assert!(!state.toggle_breakpoint_on_execution_line(&std::path::PathBuf::new()));
    }

    #[test]
    fn add_watch_expression_adds_new() {
        let mut state = DebugSharedState::default();
        state.add_watch_expression("x + 1".to_string());
        assert_eq!(state.watch.len(), 1);
        assert_eq!(state.watch[0].expression, "x + 1");
        assert!(state.watch[0].value.is_empty());
    }

    #[test]
    fn add_watch_expression_prevents_duplicates() {
        let mut state = DebugSharedState::default();
        state.add_watch_expression("x".to_string());
        state.add_watch_expression("x".to_string());
        assert_eq!(state.watch.len(), 1);
    }

    #[test]
    fn add_watch_expression_allows_different_expressions() {
        let mut state = DebugSharedState::default();
        state.add_watch_expression("x".to_string());
        state.add_watch_expression("y".to_string());
        state.add_watch_expression("x + y".to_string());
        assert_eq!(state.watch.len(), 3);
    }

    #[test]
    fn remove_watch_expression_by_index() {
        let mut state = DebugSharedState::default();
        state.add_watch_expression("a".to_string());
        state.add_watch_expression("b".to_string());
        state.add_watch_expression("c".to_string());

        assert!(state.remove_watch_expression(1));
        assert_eq!(state.watch.len(), 2);
        assert_eq!(state.watch[0].expression, "a");
        assert_eq!(state.watch[1].expression, "c");
    }

    #[test]
    fn remove_watch_expression_out_of_bounds_returns_false() {
        let mut state = DebugSharedState::default();
        state.add_watch_expression("x".to_string());
        assert!(!state.remove_watch_expression(5));
        assert_eq!(state.watch.len(), 1);
    }

    #[test]
    fn remove_watch_expression_empty_list_returns_false() {
        let mut state = DebugSharedState::default();
        assert!(!state.remove_watch_expression(0));
    }

    #[test]
    fn update_watch_values_matches_by_expression() {
        let mut state = DebugSharedState::default();
        state.add_watch_expression("x".to_string());
        state.add_watch_expression("y".to_string());

        state.update_watch_values(vec![
            ("x".to_string(), "42".to_string()),
            ("y".to_string(), "hello".to_string()),
        ]);

        assert_eq!(state.watch[0].value, "42");
        assert_eq!(state.watch[1].value, "hello");
    }

    #[test]
    fn update_watch_values_ignores_unknown_expressions() {
        let mut state = DebugSharedState::default();
        state.add_watch_expression("x".to_string());

        state.update_watch_values(vec![("unknown".to_string(), "999".to_string())]);

        assert!(state.watch[0].value.is_empty());
    }

    #[test]
    fn update_watch_values_overwrites_existing() {
        let mut state = DebugSharedState::default();
        state.add_watch_expression("x".to_string());

        state.update_watch_values(vec![("x".to_string(), "first".to_string())]);
        assert_eq!(state.watch[0].value, "first");

        state.update_watch_values(vec![("x".to_string(), "second".to_string())]);
        assert_eq!(state.watch[0].value, "second");
    }

    #[test]
    fn move_execution_down_increments_line() {
        let mut state = DebugSharedState::default();
        state.execution_location = Some(SourceLocation { line: 0, column: 0 });
        assert!(state.move_execution_down(10));
        assert_eq!(state.execution_location.unwrap().line, 1);
    }

    #[test]
    fn move_execution_down_stops_at_max() {
        let mut state = DebugSharedState::default();
        state.execution_location = Some(SourceLocation { line: 9, column: 0 });
        assert!(!state.move_execution_down(10));
        assert_eq!(state.execution_location.unwrap().line, 9);
    }

    #[test]
    fn move_execution_up_decrements_line() {
        let mut state = DebugSharedState::default();
        state.execution_location = Some(SourceLocation { line: 5, column: 0 });
        assert!(state.move_execution_up());
        assert_eq!(state.execution_location.unwrap().line, 4);
    }

    #[test]
    fn move_execution_up_stops_at_zero() {
        let mut state = DebugSharedState::default();
        state.execution_location = Some(SourceLocation { line: 0, column: 0 });
        assert!(!state.move_execution_up());
    }

    #[test]
    fn move_execution_returns_false_when_no_location() {
        let mut state = DebugSharedState::default();
        assert!(!state.move_execution_down(10));
        assert!(!state.move_execution_up());
    }

    #[test]
    fn toggle_paused_flips_state() {
        let mut state = DebugSharedState::default();
        assert!(!state.paused);
        state.toggle_paused();
        assert!(state.paused);
        state.toggle_paused();
        assert!(!state.paused);
    }

    #[test]
    fn toggle_paused_pushes_console_messages() {
        let mut state = DebugSharedState::default();
        state.toggle_paused();
        assert!(state.console_messages.iter().any(|m| m.contains("paused")));
        state.toggle_paused();
        assert!(state.console_messages.iter().any(|m| m.contains("resumed")));
    }

    #[test]
    fn breakpoint_defaults_are_correct() {
        let bp = Breakpoint {
            id: 42,
            location: SourceLocation {
                line: 10,
                column: 5,
            },
            enabled: true,
            path: std::path::PathBuf::from("main.dart"),
        };
        assert_eq!(bp.id, 42);
        assert!(bp.enabled);
        assert_eq!(bp.location.line, 10);
    }

    #[test]
    fn stack_frame_stores_location_and_path() {
        let frame = StackFrame {
            function: "main".to_string(),
            location: SourceLocation {
                line: 42,
                column: 2,
            },
            path: std::path::PathBuf::from("/lib/main.dart"),
        };
        assert_eq!(frame.function, "main");
        assert_eq!(frame.location.line, 42);
        assert_eq!(frame.path.to_str(), Some("/lib/main.dart"));
    }

    #[test]
    fn debug_variable_supports_nested_children() {
        let var = DebugVariable {
            name: "list".to_string(),
            value: "List(3)".to_string(),
            children: vec![
                DebugVariable {
                    name: "[0]".to_string(),
                    value: "1".to_string(),
                    children: Vec::new(),
                },
                DebugVariable {
                    name: "[1]".to_string(),
                    value: "2".to_string(),
                    children: vec![DebugVariable {
                        name: "nested".to_string(),
                        value: "deep".to_string(),
                        children: Vec::new(),
                    }],
                },
            ],
        };
        assert_eq!(var.children.len(), 2);
        assert_eq!(var.children[1].children.len(), 1);
        assert_eq!(var.children[1].children[0].name, "nested");
    }
}
