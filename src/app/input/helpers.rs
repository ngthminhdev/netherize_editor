use winit::keyboard::{KeyCode, NamedKey};

use crate::{
    app::input_map::{InputFocusContext, KeybindingContext},
    core::commands::{FindMotionKind, Motion, TextObjectKind, TextObjectModifier},
    core::mode::EditorMode,
};

use super::{model::NormalizedInput, pending::PendingState};

/// Trả về true nếu đây là event modifier-only (ShiftLeft, ShiftRight, ControlLeft...).
/// Winit gửi những event này TRƯỚC key thật khi dùng Shift+Key,
/// nếu xử lý chúng sẽ phá vỡ pending text-object state.
pub(super) fn is_modifier_only_key(input: &NormalizedInput) -> bool {
    if input.text.is_some() {
        return false;
    }
    if let Some(key) = &input.named_key {
        return matches!(
            key,
            NamedKey::Shift
                | NamedKey::Control
                | NamedKey::Alt
                | NamedKey::AltGraph
                | NamedKey::Meta
                | NamedKey::Super
                | NamedKey::Hyper
                | NamedKey::CapsLock
                | NamedKey::NumLock
                | NamedKey::ScrollLock
                | NamedKey::Fn
                | NamedKey::FnLock
        );
    }
    if let Some(phys) = input.physical_key {
        return matches!(
            phys,
            KeyCode::ShiftLeft
                | KeyCode::ShiftRight
                | KeyCode::ControlLeft
                | KeyCode::ControlRight
                | KeyCode::AltLeft
                | KeyCode::AltRight
                | KeyCode::SuperLeft
                | KeyCode::SuperRight
                | KeyCode::Meta
                | KeyCode::CapsLock
                | KeyCode::NumLock
                | KeyCode::ScrollLock
                | KeyCode::Fn
        );
    }
    false
}

pub(super) fn numeric_count_digit_from_input(
    input: &NormalizedInput,
    context: KeybindingContext,
    pending_state: Option<&PendingState>,
) -> Option<usize> {
    if context.focus != InputFocusContext::Editor {
        return None;
    }
    if !matches!(context.mode, EditorMode::Normal | EditorMode::Visual) {
        return None;
    }
    if input.has_command_modifier() || input.modifiers.alt_key() || input.modifiers.shift_key() {
        return None;
    }
    if matches!(
        pending_state,
        Some(
            PendingState::ReplaceChar
                | PendingState::Leader
                | PendingState::Sequence
                | PendingState::OperatorFindChar { .. }
                | PendingState::OperatorWithObject { .. }
                | PendingState::VisualTextObjectModifier { .. }
                | PendingState::LeapChar
                | PendingState::LeapLabel
        )
    ) {
        return None;
    }

    let text = input.text.as_deref()?;
    if text.chars().count() != 1 {
        return None;
    }

    match text.chars().next()? {
        '1' => Some(1),
        '2' => Some(2),
        '3' => Some(3),
        '4' => Some(4),
        '5' => Some(5),
        '6' => Some(6),
        '7' => Some(7),
        '8' => Some(8),
        '9' => Some(9),
        _ => None,
    }
}

pub(super) fn should_start_replace_pending(
    input: &NormalizedInput,
    context: KeybindingContext,
) -> bool {
    if context.focus != InputFocusContext::Editor || context.mode != EditorMode::Normal {
        return false;
    }
    if input.has_command_modifier() || input.modifiers.alt_key() || input.modifiers.shift_key() {
        return false;
    }
    input.physical_key == Some(KeyCode::KeyR) || input.text.as_deref() == Some("r")
}

/// 'y' trong Normal mode -> bắt đầu yank-operator pending để hỗ trợ yi( / ya{ ...
pub(super) fn should_start_yank_pending(
    input: &NormalizedInput,
    context: KeybindingContext,
) -> bool {
    if context.focus != InputFocusContext::Editor || context.mode != EditorMode::Normal {
        return false;
    }
    if input.has_command_modifier() || input.modifiers.alt_key() || input.modifiers.shift_key() {
        return false;
    }
    input.physical_key == Some(KeyCode::KeyY) || input.text.as_deref() == Some("y")
}

/// Kiểm tra nếu input là 'i' (inner) hoặc 'a' (around)
pub(super) fn inner_or_around_from_input(input: &NormalizedInput) -> Option<TextObjectModifier> {
    if input.has_command_modifier() || input.modifiers.alt_key() || input.modifiers.shift_key() {
        return None;
    }
    if input.physical_key == Some(KeyCode::KeyI) || input.text.as_deref() == Some("i") {
        return Some(TextObjectModifier::Inner);
    }
    if input.physical_key == Some(KeyCode::KeyA) || input.text.as_deref() == Some("a") {
        return Some(TextObjectModifier::Around);
    }
    None
}

pub(super) fn text_object_kind_from_input(input: &NormalizedInput) -> Option<TextObjectKind> {
    if input.has_command_modifier() || input.modifiers.alt_key() {
        return None;
    }
    let shift = input.modifiers.shift_key();

    if let Some(text) = input.text.as_deref() {
        match text {
            "w" => return Some(TextObjectKind::Word),
            "b" => return Some(TextObjectKind::Bracket('(', ')')),
            "B" => return Some(TextObjectKind::Bracket('{', '}')),
            "\"" => return Some(TextObjectKind::Quote('"')),
            "'" => return Some(TextObjectKind::Quote('\'')),
            "`" => return Some(TextObjectKind::Quote('`')),
            _ => {}
        }
    }

    if let Some(KeyCode::KeyB) = input.physical_key {
        return if shift {
            Some(TextObjectKind::Bracket('{', '}'))
        } else {
            Some(TextObjectKind::Bracket('(', ')'))
        };
    }

    if let Some(text) = input.text.as_deref()
        && text.chars().count() == 1
        && let Some(ch) = text.chars().next()
    {
        match ch {
            '(' | ')' => return Some(TextObjectKind::Bracket('(', ')')),
            '{' | '}' => return Some(TextObjectKind::Bracket('{', '}')),
            '[' | ']' => return Some(TextObjectKind::Bracket('[', ']')),
            '<' | '>' => return Some(TextObjectKind::Bracket('<', '>')),
            '"' => return Some(TextObjectKind::Quote('"')),
            '\'' => return Some(TextObjectKind::Quote('\'')),
            '`' => return Some(TextObjectKind::Quote('`')),
            _ => {}
        }
    }

    match input.physical_key {
        Some(KeyCode::BracketLeft) => {
            return if shift {
                Some(TextObjectKind::Bracket('{', '}'))
            } else {
                Some(TextObjectKind::Bracket('[', ']'))
            };
        }
        Some(KeyCode::BracketRight) => {
            return if shift {
                Some(TextObjectKind::Bracket('{', '}'))
            } else {
                Some(TextObjectKind::Bracket('[', ']'))
            };
        }
        Some(KeyCode::Digit9) if shift => return Some(TextObjectKind::Bracket('(', ')')),
        Some(KeyCode::Digit0) if shift => return Some(TextObjectKind::Bracket('(', ')')),
        Some(KeyCode::Quote) => {
            return if shift {
                Some(TextObjectKind::Quote('"'))
            } else {
                Some(TextObjectKind::Quote('\''))
            };
        }
        Some(KeyCode::Backquote) => return Some(TextObjectKind::Quote('`')),
        Some(KeyCode::Comma) if shift => return Some(TextObjectKind::Bracket('<', '>')),
        Some(KeyCode::Period) if shift => return Some(TextObjectKind::Bracket('<', '>')),
        _ => {}
    }

    None
}

pub(super) fn motion_from_input(input: &NormalizedInput) -> Option<Motion> {
    if input.has_command_modifier() || input.modifiers.alt_key() {
        return None;
    }

    match input.text.as_deref() {
        Some("w") => Some(Motion::WordForward),
        Some("b") => Some(Motion::WordBackward),
        Some("e") => Some(Motion::WordEnd),
        Some("0") => Some(Motion::LineStart),
        Some("^") => Some(Motion::FirstNonWhitespace),
        Some("$") => Some(Motion::LineEnd),
        Some("G") => Some(Motion::LastLine),
        _ => match input.physical_key {
            Some(KeyCode::Digit0) => Some(Motion::LineStart),
            Some(KeyCode::Digit6) if input.modifiers.shift_key() => {
                Some(Motion::FirstNonWhitespace)
            }
            Some(KeyCode::Digit4) if input.modifiers.shift_key() => Some(Motion::LineEnd),
            Some(KeyCode::KeyW) => Some(Motion::WordForward),
            Some(KeyCode::KeyB) if !input.modifiers.shift_key() => Some(Motion::WordBackward),
            Some(KeyCode::KeyE) => Some(Motion::WordEnd),
            Some(KeyCode::KeyG) if input.modifiers.shift_key() => Some(Motion::LastLine),
            _ => None,
        },
    }
}

pub(super) fn find_motion_prefix_from_input(input: &NormalizedInput) -> Option<FindMotionKind> {
    if input.has_command_modifier() || input.modifiers.alt_key() {
        return None;
    }

    match input.text.as_deref() {
        Some("f") => Some(FindMotionKind::ForwardTo),
        Some("t") => Some(FindMotionKind::ForwardTill),
        Some("F") => Some(FindMotionKind::BackwardTo),
        Some("T") => Some(FindMotionKind::BackwardTill),
        _ => match input.physical_key {
            Some(KeyCode::KeyF) if !input.modifiers.shift_key() => Some(FindMotionKind::ForwardTo),
            Some(KeyCode::KeyT) if !input.modifiers.shift_key() => {
                Some(FindMotionKind::ForwardTill)
            }
            Some(KeyCode::KeyF) if input.modifiers.shift_key() => Some(FindMotionKind::BackwardTo),
            Some(KeyCode::KeyT) if input.modifiers.shift_key() => {
                Some(FindMotionKind::BackwardTill)
            }
            _ => None,
        },
    }
}

pub(super) fn replace_char_from_input(input: &NormalizedInput) -> Option<char> {
    if let Some(text) = input.text.as_deref()
        && text.chars().count() == 1
        && let Some(ch) = text.chars().next()
        && !ch.is_control()
    {
        return Some(ch);
    }
    if input.named_key == Some(NamedKey::Space) {
        return Some(' ');
    }
    None
}

/// Trả về printable char từ key event (không bao gồm Space, được xử lý riêng).
/// Dùng cho Leap target char và label char.
pub(super) fn printable_char_from_input(input: &NormalizedInput) -> Option<char> {
    if let Some(text) = input.text.as_deref()
        && text.chars().count() == 1
        && let Some(ch) = text.chars().next()
        && !ch.is_control()
    {
        return Some(ch);
    }
    None
}

pub(super) fn terminal_input_payload(input: &NormalizedInput) -> Option<String> {
    if input.named_key == Some(NamedKey::Escape) {
        return None;
    }

    if input.modifiers.super_key() {
        return None;
    }

    if input.modifiers.control_key()
        && !input.modifiers.alt_key()
        && let Some(seq) = control_sequence_for_physical(input.physical_key)
    {
        return Some(seq.to_string());
    }

    if let Some(named) = input.named_key {
        let seq = match named {
            NamedKey::Enter => Some("\r"),
            NamedKey::Tab => Some("\t"),
            NamedKey::Backspace => Some("\u{7f}"),
            NamedKey::ArrowUp => Some("\u{1b}[A"),
            NamedKey::ArrowDown => Some("\u{1b}[B"),
            NamedKey::ArrowRight => Some("\u{1b}[C"),
            NamedKey::ArrowLeft => Some("\u{1b}[D"),
            NamedKey::Home => Some("\u{1b}[H"),
            NamedKey::End => Some("\u{1b}[F"),
            NamedKey::Delete => Some("\u{1b}[3~"),
            NamedKey::PageUp => Some("\u{1b}[5~"),
            NamedKey::PageDown => Some("\u{1b}[6~"),
            NamedKey::Space => Some(" "),
            _ => None,
        };
        if let Some(seq) = seq {
            return Some(seq.to_string());
        }
    }

    if let Some(text) = input.text.as_deref() {
        if text.is_empty() || text.chars().any(char::is_control) {
            return None;
        }
        let payload = if input.modifiers.shift_key()
            && text.chars().count() == 1
            && text.chars().next().is_some_and(|ch| ch.is_ascii_lowercase())
        {
            text.to_ascii_uppercase()
        } else {
            text.to_string()
        };
        if input.modifiers.alt_key() && !input.modifiers.control_key() {
            return Some(format!("\u{1b}{payload}"));
        }
        return Some(payload);
    }

    if !input.modifiers.control_key()
        && let Some(payload) = shifted_letter_payload(input.physical_key, input.modifiers.shift_key())
    {
        if input.modifiers.alt_key() {
            return Some(format!("\u{1b}{payload}"));
        }
        return Some(payload.to_string());
    }

    None
}

fn shifted_letter_payload(physical_key: Option<KeyCode>, shifted: bool) -> Option<&'static str> {
    if !shifted {
        return None;
    }

    match physical_key? {
        KeyCode::KeyA => Some("A"),
        KeyCode::KeyB => Some("B"),
        KeyCode::KeyC => Some("C"),
        KeyCode::KeyD => Some("D"),
        KeyCode::KeyE => Some("E"),
        KeyCode::KeyF => Some("F"),
        KeyCode::KeyG => Some("G"),
        KeyCode::KeyH => Some("H"),
        KeyCode::KeyI => Some("I"),
        KeyCode::KeyJ => Some("J"),
        KeyCode::KeyK => Some("K"),
        KeyCode::KeyL => Some("L"),
        KeyCode::KeyM => Some("M"),
        KeyCode::KeyN => Some("N"),
        KeyCode::KeyO => Some("O"),
        KeyCode::KeyP => Some("P"),
        KeyCode::KeyQ => Some("Q"),
        KeyCode::KeyR => Some("R"),
        KeyCode::KeyS => Some("S"),
        KeyCode::KeyT => Some("T"),
        KeyCode::KeyU => Some("U"),
        KeyCode::KeyV => Some("V"),
        KeyCode::KeyW => Some("W"),
        KeyCode::KeyX => Some("X"),
        KeyCode::KeyY => Some("Y"),
        KeyCode::KeyZ => Some("Z"),
        _ => None,
    }
}

fn control_sequence_for_physical(physical_key: Option<KeyCode>) -> Option<&'static str> {
    match physical_key? {
        KeyCode::KeyA => Some("\u{01}"),
        KeyCode::KeyB => Some("\u{02}"),
        KeyCode::KeyC => Some("\u{03}"),
        KeyCode::KeyD => Some("\u{04}"),
        KeyCode::KeyE => Some("\u{05}"),
        KeyCode::KeyF => Some("\u{06}"),
        KeyCode::KeyG => Some("\u{07}"),
        KeyCode::KeyH => Some("\u{08}"),
        KeyCode::KeyI => Some("\u{09}"),
        KeyCode::KeyJ => Some("\u{0a}"),
        KeyCode::KeyK => Some("\u{0b}"),
        KeyCode::KeyL => Some("\u{0c}"),
        KeyCode::KeyM => Some("\u{0d}"),
        KeyCode::KeyN => Some("\u{0e}"),
        KeyCode::KeyO => Some("\u{0f}"),
        KeyCode::KeyP => Some("\u{10}"),
        KeyCode::KeyQ => Some("\u{11}"),
        KeyCode::KeyR => Some("\u{12}"),
        KeyCode::KeyS => Some("\u{13}"),
        KeyCode::KeyT => Some("\u{14}"),
        KeyCode::KeyU => Some("\u{15}"),
        KeyCode::KeyV => Some("\u{16}"),
        KeyCode::KeyW => Some("\u{17}"),
        KeyCode::KeyX => Some("\u{18}"),
        KeyCode::KeyY => Some("\u{19}"),
        KeyCode::KeyZ => Some("\u{1a}"),
        KeyCode::Space => Some("\u{00}"),
        KeyCode::BracketLeft => Some("\u{1b}"),
        KeyCode::Backslash => Some("\u{1c}"),
        KeyCode::BracketRight => Some("\u{1d}"),
        KeyCode::Digit6 => Some("\u{1e}"),
        KeyCode::Minus => Some("\u{1f}"),
        _ => None,
    }
}
