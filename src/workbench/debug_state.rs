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
    next_breakpoint_id: u64,
}

impl Default for DebugSharedState {
    fn default() -> Self {
        Self {
            paused: true,
            execution_location: Some(SourceLocation { line: 6, column: 8 }),
            breakpoints: vec![
                Breakpoint {
                    id: 1,
                    location: SourceLocation { line: 3, column: 0 },
                    enabled: true,
                },
                Breakpoint {
                    id: 2,
                    location: SourceLocation {
                        line: 10,
                        column: 0,
                    },
                    enabled: true,
                },
            ],
            variables: vec![
                DebugVariable {
                    name: "counter".to_string(),
                    value: "42".to_string(),
                    children: Vec::new(),
                },
                DebugVariable {
                    name: "user".to_string(),
                    value: "User".to_string(),
                    children: vec![
                        DebugVariable {
                            name: "id".to_string(),
                            value: "u_1024".to_string(),
                            children: Vec::new(),
                        },
                        DebugVariable {
                            name: "active".to_string(),
                            value: "true".to_string(),
                            children: Vec::new(),
                        },
                    ],
                },
            ],
            watch: vec![
                WatchEntry {
                    expression: "counter > max".to_string(),
                    value: "false".to_string(),
                },
                WatchEntry {
                    expression: "items.len()".to_string(),
                    value: "128".to_string(),
                },
            ],
            call_stack: vec![
                StackFrame {
                    function: "process_event".to_string(),
                    location: SourceLocation { line: 6, column: 8 },
                },
                StackFrame {
                    function: "main".to_string(),
                    location: SourceLocation {
                        line: 42,
                        column: 2,
                    },
                },
            ],
            inline_values: vec![
                InlineValue {
                    location: SourceLocation {
                        line: 6,
                        column: 14,
                    },
                    text: "counter=42".to_string(),
                },
                InlineValue {
                    location: SourceLocation {
                        line: 6,
                        column: 28,
                    },
                    text: "active=true".to_string(),
                },
            ],
            console_messages: vec!["paused at breakpoint #1".to_string()],
            next_breakpoint_id: 3,
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

    pub fn toggle_breakpoint_on_execution_line(&mut self) -> bool {
        let Some(loc) = self.execution_location else {
            return false;
        };
        self.toggle_breakpoint_at_line(loc.line)
    }

    pub fn toggle_breakpoint_at_line(&mut self, line: usize) -> bool {
        if let Some(index) = self
            .breakpoints
            .iter()
            .position(|bp| bp.location.line == line)
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
        });
        self.next_breakpoint_id += 1;
        self.console_messages
            .push(format!("breakpoint added at line {}", line + 1));
        true
    }
}

#[cfg(test)]
mod tests {
    use super::DebugSharedState;

    #[test]
    fn toggle_breakpoint_adds_and_removes_on_same_line() {
        let mut state = DebugSharedState::default();
        let initial = state.breakpoints.len();
        state.toggle_breakpoint_at_line(25);
        assert_eq!(state.breakpoints.len(), initial + 1);
        state.toggle_breakpoint_at_line(25);
        assert_eq!(state.breakpoints.len(), initial);
    }
}
