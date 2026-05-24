/// Các mode cốt lõi của editor.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EditorMode {
    Insert,
    Normal,
    Visual,
    VisualBlock,
    PaletteFocus,
    TerminalFocus,
    TerminalNormal,
    /// Multiple cursors selected (match phase: adding cursors to each occurrence).
    MultiCursor,
    /// Simultaneous insert across all virtual cursors (action phase).
    MultiInsert,
    /// Workbench dock/window resize mode.
    Resize,
}

impl EditorMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Insert => "insert",
            Self::Normal => "normal",
            Self::Visual => "visual",
            Self::VisualBlock => "visual_block",
            Self::PaletteFocus => "palette_focus",
            Self::TerminalFocus => "terminal_focus",
            Self::TerminalNormal => "terminal_normal",
            Self::MultiCursor => "multicursor",
            Self::MultiInsert => "multiinsert",
            Self::Resize => "resize",
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
    EnterVisualBlock,
    EnterTerminalNormal,
    OpenPalette,
    FocusTerminal,
    ExitFocus,
    Escape,
    /// Enter MultiCursor mode (triggered by MultiCursorAddNext command).
    EnterMultiCursor,
    /// Enter MultiInsert mode (I / A / c action on multi-cursor selection).
    EnterMultiInsert,
    /// Enter workbench resize mode.
    EnterResize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ModeTransitionRule {
    pub from: EditorMode,
    pub event: ModeEvent,
    pub to: EditorMode,
}

const TRANSITION_RULES: [ModeTransitionRule; 38] = [
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
        from: EditorMode::Normal,
        event: ModeEvent::EnterVisualBlock,
        to: EditorMode::VisualBlock,
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
    ModeTransitionRule {
        from: EditorMode::VisualBlock,
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
        from: EditorMode::VisualBlock,
        event: ModeEvent::Escape,
        to: EditorMode::Normal,
    },
    ModeTransitionRule {
        from: EditorMode::Normal,
        event: ModeEvent::Escape,
        to: EditorMode::Normal,
    },
    // VisualBlock → MultiInsert (I/A commands)
    ModeTransitionRule {
        from: EditorMode::VisualBlock,
        event: ModeEvent::EnterMultiInsert,
        to: EditorMode::MultiInsert,
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
        from: EditorMode::VisualBlock,
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
    ModeTransitionRule {
        from: EditorMode::VisualBlock,
        event: ModeEvent::FocusTerminal,
        to: EditorMode::TerminalFocus,
    },
    ModeTransitionRule {
        from: EditorMode::TerminalFocus,
        event: ModeEvent::EnterTerminalNormal,
        to: EditorMode::TerminalNormal,
    },
    ModeTransitionRule {
        from: EditorMode::TerminalNormal,
        event: ModeEvent::FocusTerminal,
        to: EditorMode::TerminalFocus,
    },
    // ── MultiCursor transitions ────────────────────────────────────────────────
    ModeTransitionRule {
        from: EditorMode::Normal,
        event: ModeEvent::EnterMultiCursor,
        to: EditorMode::MultiCursor,
    },
    ModeTransitionRule {
        from: EditorMode::Visual,
        event: ModeEvent::EnterMultiCursor,
        to: EditorMode::MultiCursor,
    },
    ModeTransitionRule {
        from: EditorMode::MultiCursor,
        event: ModeEvent::EnterMultiInsert,
        to: EditorMode::MultiInsert,
    },
    ModeTransitionRule {
        from: EditorMode::MultiCursor,
        event: ModeEvent::Escape,
        to: EditorMode::Normal,
    },
    ModeTransitionRule {
        from: EditorMode::MultiCursor,
        event: ModeEvent::EnterNormal,
        to: EditorMode::Normal,
    },
    ModeTransitionRule {
        from: EditorMode::MultiInsert,
        event: ModeEvent::Escape,
        to: EditorMode::Normal,
    },
    ModeTransitionRule {
        from: EditorMode::MultiInsert,
        event: ModeEvent::EnterNormal,
        to: EditorMode::Normal,
    },
    ModeTransitionRule {
        from: EditorMode::MultiCursor,
        event: ModeEvent::OpenPalette,
        to: EditorMode::PaletteFocus,
    },
    ModeTransitionRule {
        from: EditorMode::MultiInsert,
        event: ModeEvent::OpenPalette,
        to: EditorMode::PaletteFocus,
    },
    ModeTransitionRule {
        from: EditorMode::TerminalNormal,
        event: ModeEvent::OpenPalette,
        to: EditorMode::PaletteFocus,
    },
    // ── Resize mode ───────────────────────────────────────────────────────────
    ModeTransitionRule {
        from: EditorMode::Normal,
        event: ModeEvent::EnterResize,
        to: EditorMode::Resize,
    },
    ModeTransitionRule {
        from: EditorMode::TerminalNormal,
        event: ModeEvent::EnterResize,
        to: EditorMode::Resize,
    },
    ModeTransitionRule {
        from: EditorMode::Resize,
        event: ModeEvent::Escape,
        to: EditorMode::Normal,
    },
    ModeTransitionRule {
        from: EditorMode::Resize,
        event: ModeEvent::EnterNormal,
        to: EditorMode::Normal,
    },
    ModeTransitionRule {
        from: EditorMode::Resize,
        event: ModeEvent::OpenPalette,
        to: EditorMode::PaletteFocus,
    },
    ModeTransitionRule {
        from: EditorMode::Resize,
        event: ModeEvent::FocusTerminal,
        to: EditorMode::TerminalFocus,
    },
    ModeTransitionRule {
        from: EditorMode::Resize,
        event: ModeEvent::EnterInsert,
        to: EditorMode::Insert,
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
        Self::new(EditorMode::Normal)
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
            EditorMode::PaletteFocus
                | EditorMode::TerminalFocus
                | EditorMode::TerminalNormal
                | EditorMode::Resize
        ) && matches!(event, ModeEvent::ExitFocus | ModeEvent::Escape)
        {
            return Some(self.return_mode.unwrap_or(EditorMode::Normal));
        }

        lookup_rule(self.current, event).map(|rule| rule.to)
    }

    fn update_focus_memory(&mut self, from: EditorMode, to: EditorMode) {
        let from_is_focus = matches!(
            from,
            EditorMode::PaletteFocus
                | EditorMode::TerminalFocus
                | EditorMode::TerminalNormal
                | EditorMode::Resize
        );
        let to_is_focus = matches!(
            to,
            EditorMode::PaletteFocus
                | EditorMode::TerminalFocus
                | EditorMode::TerminalNormal
                | EditorMode::Resize
        );

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
    fn terminal_normal_keeps_focus_memory_and_can_return_to_typing() {
        let mut state = ModeState::new(EditorMode::Insert);
        state
            .apply_with_side_effects(ModeEvent::FocusTerminal)
            .expect("focus terminal");
        assert_eq!(state.current(), EditorMode::TerminalFocus);
        assert_eq!(state.return_mode(), Some(EditorMode::Insert));

        state
            .apply_with_side_effects(ModeEvent::EnterTerminalNormal)
            .expect("enter terminal normal");
        assert_eq!(state.current(), EditorMode::TerminalNormal);
        assert_eq!(state.return_mode(), Some(EditorMode::Insert));

        state
            .apply_with_side_effects(ModeEvent::FocusTerminal)
            .expect("return to typing");
        assert_eq!(state.current(), EditorMode::TerminalFocus);
        assert_eq!(state.return_mode(), Some(EditorMode::Insert));

        state
            .apply_with_side_effects(ModeEvent::ExitFocus)
            .expect("exit terminal focus");
        assert_eq!(state.current(), EditorMode::Insert);
        assert_eq!(state.return_mode(), None);
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
        assert!(rules.iter().any(|rule| {
            rule.from == EditorMode::TerminalFocus
                && rule.event == ModeEvent::EnterTerminalNormal
                && rule.to == EditorMode::TerminalNormal
        }));
    }
}
