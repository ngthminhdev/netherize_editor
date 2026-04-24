use winit::{
    event::KeyEvent,
    keyboard::{Key, KeyCode, ModifiersState, NamedKey, PhysicalKey},
};

use crate::core::commands::Command;

/// Dữ liệu key đã chuẩn hóa để tách phần "đọc winit event"
/// khỏi phần "map sang command".
#[derive(Debug, Clone)]
pub struct NormalizedInput {
    pub physical_key: Option<KeyCode>,
    pub named_key: Option<NamedKey>,
    pub text: Option<String>,
    pub modifiers: ModifiersState,
}

impl NormalizedInput {
    pub fn from_key_event(key_event: &KeyEvent, modifiers: ModifiersState) -> Self {
        let physical_key = match key_event.physical_key {
            PhysicalKey::Code(key_code) => Some(key_code),
            PhysicalKey::Unidentified(_) => None,
        };

        let named_key = match key_event.logical_key.as_ref() {
            Key::Named(named) => Some(named),
            _ => None,
        };

        let text = match key_event.logical_key.as_ref() {
            Key::Character(text) if !text.is_empty() => Some(text.to_string()),
            _ => None,
        };

        Self {
            physical_key,
            named_key,
            text,
            modifiers,
        }
    }

    pub fn has_command_modifier(&self) -> bool {
        self.modifiers.super_key() || self.modifiers.control_key()
    }

    pub fn debug_label(&self) -> String {
        let key_text = if let Some(code) = self.physical_key {
            format!("{code:?}")
        } else if let Some(named) = self.named_key {
            format!("{named:?}")
        } else if let Some(text) = &self.text {
            format!("Text({text:?})")
        } else {
            "UnknownKey".to_string()
        };

        format!("{key_text} + {}", format_modifiers(self.modifiers))
    }
}

#[derive(Debug, Clone)]
pub struct TranslatedInput {
    pub input_debug: String,
    pub route_debug: String,
    pub command: Command,
    pub repeat_count: usize,
}

#[derive(Debug, Clone)]
pub enum InputRouteOutcome {
    Dispatch(TranslatedInput),
    NoDispatch {
        input_debug: String,
        route_debug: String,
    },
}

fn format_modifiers(modifiers: ModifiersState) -> String {
    let mut labels = Vec::new();
    if modifiers.control_key() {
        labels.push("Ctrl");
    }
    if modifiers.alt_key() {
        labels.push("Alt");
    }
    if modifiers.shift_key() {
        labels.push("Shift");
    }
    if modifiers.super_key() {
        labels.push("Super");
    }

    if labels.is_empty() {
        "no modifiers".to_string()
    } else {
        labels.join("+")
    }
}
