/// Các mode cốt lõi của editor.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EditorMode {
    Insert,
    Normal,
    Visual,
    PaletteFocus,
    TerminalFocus,
}

impl EditorMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Insert => "insert",
            Self::Normal => "normal",
            Self::Visual => "visual",
            Self::PaletteFocus => "palette_focus",
            Self::TerminalFocus => "terminal_focus",
        }
    }
}

/// Trigger logic cho việc chuyển mode.
/// Ở phase này ta giữ intent-level (chưa buộc mapping key cụ thể).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ModeEvent {
    EnterInsert,
    EnterNormal,
    EnterVisual,
    OpenPalette,
    FocusTerminal,
    ExitFocus,
    Escape,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ModeTransitionRule {
    pub from: EditorMode,
    pub event: ModeEvent,
    pub to: EditorMode,
}

const TRANSITION_RULES: [ModeTransitionRule; 13] = [
    // Core editing transitions
    ModeTransitionRule {
        from: EditorMode::Normal,
        event: ModeEvent::EnterInsert,
        to: EditorMode::Insert,
    },
    ModeTransitionRule {
        from: EditorMode::Normal,
        event: ModeEvent::EnterVisual,
        to: EditorMode::Visual,
    },
    ModeTransitionRule {
        from: EditorMode::Insert,
        event: ModeEvent::EnterNormal,
        to: EditorMode::Normal,
    },
    ModeTransitionRule {
        from: EditorMode::Visual,
        event: ModeEvent::EnterNormal,
        to: EditorMode::Normal,
    },
    // Escape cho mode edit
    ModeTransitionRule {
        from: EditorMode::Insert,
        event: ModeEvent::Escape,
        to: EditorMode::Normal,
    },
    ModeTransitionRule {
        from: EditorMode::Visual,
        event: ModeEvent::Escape,
        to: EditorMode::Normal,
    },
    ModeTransitionRule {
        from: EditorMode::Normal,
        event: ModeEvent::Escape,
        to: EditorMode::Normal,
    },
    // Focus modes
    ModeTransitionRule {
        from: EditorMode::Insert,
        event: ModeEvent::OpenPalette,
        to: EditorMode::PaletteFocus,
    },
    ModeTransitionRule {
        from: EditorMode::Normal,
        event: ModeEvent::OpenPalette,
        to: EditorMode::PaletteFocus,
    },
    ModeTransitionRule {
        from: EditorMode::Visual,
        event: ModeEvent::OpenPalette,
        to: EditorMode::PaletteFocus,
    },
    ModeTransitionRule {
        from: EditorMode::Insert,
        event: ModeEvent::FocusTerminal,
        to: EditorMode::TerminalFocus,
    },
    ModeTransitionRule {
        from: EditorMode::Normal,
        event: ModeEvent::FocusTerminal,
        to: EditorMode::TerminalFocus,
    },
    ModeTransitionRule {
        from: EditorMode::Visual,
        event: ModeEvent::FocusTerminal,
        to: EditorMode::TerminalFocus,
    },
];

pub fn transition_rules() -> &'static [ModeTransitionRule] {
    &TRANSITION_RULES
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ModeTransitionResult {
    pub from: EditorMode,
    pub to: EditorMode,
    pub event: ModeEvent,
    pub changed: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModeTransitionError {
    InvalidTransition { from: EditorMode, event: ModeEvent },
}

/// State model tập trung cho mode.
///
/// `return_mode` dùng để thoát khỏi focus mode và quay về mode trước đó.
#[derive(Debug, Clone)]
pub struct ModeState {
    current: EditorMode,
    return_mode: Option<EditorMode>,
}

impl Default for ModeState {
    fn default() -> Self {
        Self::new(EditorMode::Insert)
    }
}

impl ModeState {
    pub fn new(initial_mode: EditorMode) -> Self {
        Self {
            current: initial_mode,
            return_mode: None,
        }
    }

    pub fn current(&self) -> EditorMode {
        self.current
    }

    pub fn can_apply(&self, event: ModeEvent) -> bool {
        self.resolve_target(event).is_some()
    }

    /// Áp dụng transition và cập nhật state phụ (return_mode) theo protocol.
    pub fn apply(&mut self, event: ModeEvent) -> Result<ModeTransitionResult, ModeTransitionError> {
        let from = self.current;
        let Some(target) = self.resolve_target(event) else {
            return Err(ModeTransitionError::InvalidTransition { from, event });
        };

        self.update_focus_memory(from, target);

        let changed = target != from;
        self.current = target;

        Ok(ModeTransitionResult {
            from,
            to: target,
            event,
            changed,
        })
    }

    fn resolve_target(&self, event: ModeEvent) -> Option<EditorMode> {
        // Transition phụ thuộc ngữ cảnh trước: focus mode sẽ quay về mode trước đó.
        if matches!(
            self.current,
            EditorMode::PaletteFocus | EditorMode::TerminalFocus
        ) && matches!(event, ModeEvent::ExitFocus | ModeEvent::Escape)
        {
            return Some(self.return_mode.unwrap_or(EditorMode::Normal));
        }

        lookup_rule(self.current, event).map(|rule| rule.to)
    }

    fn update_focus_memory(&mut self, from: EditorMode, to: EditorMode) {
        let from_is_focus = matches!(from, EditorMode::PaletteFocus | EditorMode::TerminalFocus);
        let to_is_focus = matches!(to, EditorMode::PaletteFocus | EditorMode::TerminalFocus);

        // Đi vào focus mode: nhớ mode trước đó để lúc thoát có chỗ quay về.
        if !from_is_focus && to_is_focus {
            self.return_mode = Some(from);
            return;
        }

        // Thoát focus mode: clear return_mode để tránh leak state cũ.
        if from_is_focus && !to_is_focus {
            self.return_mode = None;
        }
    }

    pub fn apply_with_side_effects(
        &mut self,
        event: ModeEvent,
    ) -> Result<ModeTransitionResult, ModeTransitionError> {
        self.apply(event)
    }
}

fn lookup_rule(from: EditorMode, event: ModeEvent) -> Option<ModeTransitionRule> {
    TRANSITION_RULES
        .iter()
        .find(|rule| rule.from == from && rule.event == event)
        .copied()
}

impl ModeState {
    pub fn return_mode(&self) -> Option<EditorMode> {
        self.return_mode
    }
}

#[cfg(test)]
mod tests {
    use super::{EditorMode, ModeEvent, ModeState, ModeTransitionError, transition_rules};

    #[test]
    fn normal_to_insert_is_allowed() {
        let mut state = ModeState::new(EditorMode::Normal);
        let result = state
            .apply_with_side_effects(ModeEvent::EnterInsert)
            .expect("transition should be valid");

        assert_eq!(result.from, EditorMode::Normal);
        assert_eq!(result.to, EditorMode::Insert);
        assert!(result.changed);
        assert_eq!(state.current(), EditorMode::Insert);
    }

    #[test]
    fn visual_to_normal_by_escape_is_allowed() {
        let mut state = ModeState::new(EditorMode::Visual);
        let result = state
            .apply_with_side_effects(ModeEvent::Escape)
            .expect("escape should return to normal");

        assert_eq!(result.to, EditorMode::Normal);
        assert_eq!(state.current(), EditorMode::Normal);
    }

    #[test]
    fn invalid_transition_is_rejected() {
        let mut state = ModeState::new(EditorMode::Insert);
        let error = state
            .apply_with_side_effects(ModeEvent::EnterVisual)
            .expect_err("insert -> visual should be invalid");

        assert_eq!(
            error,
            ModeTransitionError::InvalidTransition {
                from: EditorMode::Insert,
                event: ModeEvent::EnterVisual
            }
        );
        assert_eq!(state.current(), EditorMode::Insert);
    }

    #[test]
    fn palette_focus_returns_to_previous_mode() {
        let mut state = ModeState::new(EditorMode::Visual);
        state
            .apply_with_side_effects(ModeEvent::OpenPalette)
            .expect("open palette");
        assert_eq!(state.current(), EditorMode::PaletteFocus);

        state
            .apply_with_side_effects(ModeEvent::ExitFocus)
            .expect("exit focus");
        assert_eq!(state.current(), EditorMode::Visual);
    }

    #[test]
    fn transition_table_contains_core_rules() {
        let rules = transition_rules();
        assert!(rules.iter().any(|rule| {
            rule.from == EditorMode::Normal
                && rule.event == ModeEvent::EnterInsert
                && rule.to == EditorMode::Insert
        }));
        assert!(rules.iter().any(|rule| {
            rule.from == EditorMode::Insert
                && rule.event == ModeEvent::Escape
                && rule.to == EditorMode::Normal
        }));
    }
}
