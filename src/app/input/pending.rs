use std::time::Instant;

use winit::keyboard::KeyCode;

use crate::app::{input_map::PendingSequence, resolved_keymap::KeySpec};
use crate::core::commands::{FindMotionKind, Operator, TextObjectModifier};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LeapTarget {
    pub label: String,
    pub char_idx: usize,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LeapState {
    pub typed_prefix: String,
    pub targets: Vec<LeapTarget>,
}

impl LeapState {
    pub fn new(targets: Vec<LeapTarget>) -> Self {
        Self {
            typed_prefix: String::new(),
            targets,
        }
    }
}

pub fn generate_leap_labels(count: usize) -> Vec<String> {
    let alphabet: Vec<char> = "abcdefghijklmnopqrstuvwxyz".chars().collect();
    if count == 0 {
        return Vec::new();
    }

    let fast_jump = &alphabet[..13];
    let prefix_group = &alphabet[13..];

    let mut labels =
        Vec::with_capacity(count.min(fast_jump.len() + prefix_group.len() * alphabet.len()));

    for ch in fast_jump {
        labels.push(ch.to_string());
        if labels.len() >= count {
            return labels;
        }
    }

    for prefix in prefix_group {
        for suffix in &alphabet {
            labels.push(format!("{prefix}{suffix}"));
            if labels.len() >= count {
                return labels;
            }
        }
    }
    labels
}

#[derive(Debug, Clone)]
pub(super) struct PendingInput {
    pub(super) sequence: Option<PendingSequence>,
    pub(super) state: PendingState,
    pub(super) started_at: Instant,
}

#[derive(Debug, Clone)]
pub(super) enum PendingState {
    PendingOperator { op: Operator },
    /// Đã nhận operator + g, chờ g tiếp theo để tạo motion gg.
    OperatorG { op: Operator },
    /// Đã nhận operator (d/c/y) + find motion prefix (f/F/t/T) -> chờ target char.
    OperatorFindChar { op: Operator, kind: FindMotionKind },
    ReplaceChar,
    Leader,
    Sequence,
    /// Đã nhận operator (d/c/y) + modifier (i/a) -> chờ bracket char.
    OperatorWithObject {
        op: Operator,
        modifier: TextObjectModifier,
    },
    /// Trong Visual mode đã nhận 'i' hoặc 'a' -> chờ bracket char.
    VisualTextObjectModifier {
        modifier: TextObjectModifier,
    },
    /// Leap: chờ user gõ target char (sau LeapStart).
    LeapChar,
    /// Leap: chờ user gõ prefix/label nhiều ký tự để lọc và nhảy.
    LeapLabel,
}

pub(super) fn classify_pending_state(sequence: &PendingSequence) -> PendingState {
    if sequence.steps.len() == 1 {
        if matches!(sequence.steps[0], KeySpec::Leader) {
            return PendingState::Leader;
        }
        if matches!(sequence.steps[0], KeySpec::Physical(KeyCode::KeyD)) {
            return PendingState::PendingOperator {
                op: Operator::Delete,
            };
        }
        if matches!(sequence.steps[0], KeySpec::Physical(KeyCode::KeyC)) {
            return PendingState::PendingOperator {
                op: Operator::Change,
            };
        }
    }
    PendingState::Sequence
}

impl PendingState {
    pub(super) fn uses_operator_count(&self) -> bool {
        matches!(
            self,
            Self::PendingOperator { .. }
                | Self::OperatorG { .. }
                | Self::OperatorFindChar { .. }
                | Self::OperatorWithObject { .. }
        )
    }

    pub(super) fn as_str(&self) -> &'static str {
        match self {
            Self::PendingOperator { op } => match op {
                Operator::Delete => "pending operator delete",
                Operator::Change => "pending operator change",
                Operator::Yank => "pending operator yank",
                Operator::Visual => "pending operator visual",
            },
            Self::OperatorG { op } => match op {
                Operator::Delete => "pending delete g-motion",
                Operator::Change => "pending change g-motion",
                Operator::Yank => "pending yank g-motion",
                Operator::Visual => "pending visual g-motion",
            },
            Self::OperatorFindChar { op, kind } => match (op, kind) {
                (Operator::Delete, FindMotionKind::ForwardTo) => "pending delete find-char forward",
                (Operator::Delete, FindMotionKind::ForwardTill) => "pending delete till-char forward",
                (Operator::Delete, FindMotionKind::BackwardTo) => "pending delete find-char backward",
                (Operator::Delete, FindMotionKind::BackwardTill) => "pending delete till-char backward",
                (Operator::Change, FindMotionKind::ForwardTo) => "pending change find-char forward",
                (Operator::Change, FindMotionKind::ForwardTill) => "pending change till-char forward",
                (Operator::Change, FindMotionKind::BackwardTo) => "pending change find-char backward",
                (Operator::Change, FindMotionKind::BackwardTill) => "pending change till-char backward",
                (Operator::Yank, FindMotionKind::ForwardTo) => "pending yank find-char forward",
                (Operator::Yank, FindMotionKind::ForwardTill) => "pending yank till-char forward",
                (Operator::Yank, FindMotionKind::BackwardTo) => "pending yank find-char backward",
                (Operator::Yank, FindMotionKind::BackwardTill) => "pending yank till-char backward",
                (Operator::Visual, _) => "pending visual find-char",
            },
            Self::ReplaceChar => "pending replace char",
            Self::Leader => "pending leader",
            Self::Sequence => "pending sequence",
            Self::OperatorWithObject { op, modifier } => match op {
                Operator::Delete => {
                    if *modifier == TextObjectModifier::Inner {
                        "pending delete inner object"
                    } else {
                        "pending delete around object"
                    }
                }
                Operator::Change => {
                    if *modifier == TextObjectModifier::Inner {
                        "pending change inner object"
                    } else {
                        "pending change around object"
                    }
                }
                Operator::Yank => {
                    if *modifier == TextObjectModifier::Inner {
                        "pending yank inner object"
                    } else {
                        "pending yank around object"
                    }
                }
                Operator::Visual => "pending visual object",
            },
            Self::VisualTextObjectModifier { modifier } => {
                if *modifier == TextObjectModifier::Inner {
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

#[cfg(test)]
mod tests {
    use super::generate_leap_labels;

    #[test]
    fn generate_leap_labels_returns_single_chars_up_to_twenty_six() {
        let labels = generate_leap_labels(4);
        assert_eq!(labels, vec!["a", "b", "c", "d"]);
    }

    #[test]
    fn generate_leap_labels_uses_hybrid_boundary_after_fast_group() {
        let labels = generate_leap_labels(14);
        assert_eq!(labels[0], "a");
        assert_eq!(labels[12], "m");
        assert_eq!(labels[13], "na");
    }

    #[test]
    fn generate_leap_labels_generates_prefixed_two_char_labels_after_fast_group() {
        let labels = generate_leap_labels(28);
        assert_eq!(labels[0], "a");
        assert_eq!(labels[12], "m");
        assert_eq!(labels[13], "na");
        assert_eq!(labels[14], "nb");
        assert_eq!(labels[25], "nm");
        assert_eq!(labels[26], "nn");
        assert_eq!(labels[27], "no");
    }

    #[test]
    fn generate_leap_labels_supports_full_hybrid_capacity() {
        let labels = generate_leap_labels(351);
        assert_eq!(labels.len(), 351);
        assert_eq!(labels[0], "a");
        assert_eq!(labels[12], "m");
        assert_eq!(labels[13], "na");
        assert_eq!(labels[350], "zz");
    }
}
