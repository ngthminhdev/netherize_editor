use std::time::Instant;

use winit::keyboard::KeyCode;

use crate::app::{input_map::PendingSequence, resolved_keymap::KeySpec};

#[derive(Debug, Clone)]
pub(super) struct PendingInput {
    pub(super) sequence: Option<PendingSequence>,
    pub(super) state: PendingState,
    pub(super) started_at: Instant,
}

/// Loại toán tử đang chờ text object.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum TextObjectOp {
    Delete,
    Change,
    Yank,
}

#[derive(Debug, Clone)]
pub(super) enum PendingState {
    OperatorDelete,
    OperatorChange,
    /// 'y' trong Normal mode -> chờ 'i'/'a' rồi bracket để yank text object.
    OperatorYank,
    ReplaceChar,
    Leader,
    Sequence,
    /// Đã nhận operator (d/c/y) + modifier (i/a) -> chờ bracket char.
    OperatorWithObject {
        op: TextObjectOp,
        inner: bool,
    },
    /// Trong Visual mode đã nhận 'i' hoặc 'a' -> chờ bracket char.
    VisualTextObjectModifier {
        inner: bool,
    },
    /// Leap: chờ user gõ target char (sau LeapStart).
    LeapChar,
    /// Leap: chờ user gõ label char để nhảy (sau LeapActivate).
    LeapLabel,
}

pub(super) fn classify_pending_state(sequence: &PendingSequence) -> PendingState {
    if sequence.steps.len() == 1 {
        if matches!(sequence.steps[0], KeySpec::Leader) {
            return PendingState::Leader;
        }
        if matches!(sequence.steps[0], KeySpec::Physical(KeyCode::KeyD)) {
            return PendingState::OperatorDelete;
        }
        if matches!(sequence.steps[0], KeySpec::Physical(KeyCode::KeyC)) {
            return PendingState::OperatorChange;
        }
    }
    PendingState::Sequence
}

impl PendingState {
    pub(super) fn uses_operator_count(&self) -> bool {
        matches!(
            self,
            Self::OperatorDelete
                | Self::OperatorChange
                | Self::OperatorYank
                | Self::OperatorWithObject { .. }
        )
    }

    pub(super) fn as_str(&self) -> &'static str {
        match self {
            Self::OperatorDelete => "pending operator delete",
            Self::OperatorChange => "pending operator change",
            Self::OperatorYank => "pending operator yank",
            Self::ReplaceChar => "pending replace char",
            Self::Leader => "pending leader",
            Self::Sequence => "pending sequence",
            Self::OperatorWithObject { op, inner } => match op {
                TextObjectOp::Delete => {
                    if *inner {
                        "pending delete inner object"
                    } else {
                        "pending delete around object"
                    }
                }
                TextObjectOp::Change => {
                    if *inner {
                        "pending change inner object"
                    } else {
                        "pending change around object"
                    }
                }
                TextObjectOp::Yank => {
                    if *inner {
                        "pending yank inner object"
                    } else {
                        "pending yank around object"
                    }
                }
            },
            Self::VisualTextObjectModifier { inner } => {
                if *inner {
                    "pending visual inner object"
                } else {
                    "pending visual around object"
                }
            }
            Self::LeapChar => "pending leap char",
            Self::LeapLabel => "pending leap label",
        }
    }
}
