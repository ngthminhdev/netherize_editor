use std::time::{Duration, Instant};

use winit::{
    event::{ElementState, KeyEvent, Modifiers},
    keyboard::{Key, KeyCode, ModifiersState, NamedKey, PhysicalKey},
};

use crate::{
    app::input_map::{InputMap, KeybindingContext, PrefixNamespace},
    core::commands::Command,
};

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
}

#[derive(Debug, Clone)]
pub enum InputRouteOutcome {
    Dispatch(TranslatedInput),
    NoDispatch {
        input_debug: String,
        route_debug: String,
    },
}

#[derive(Debug, Clone, Copy)]
struct PendingPrefix {
    namespace: PrefixNamespace,
    started_at: Instant,
}

/// InputHandler là lớp cầu nối:
/// winit key event -> normalized input -> command.
pub struct InputHandler {
    modifiers: ModifiersState,
    prefix_timeout: Duration,
    pending_prefix: Option<PendingPrefix>,
}

impl InputHandler {
    pub fn new() -> Self {
        Self {
            modifiers: ModifiersState::empty(),
            prefix_timeout: Duration::from_millis(1_500),
            pending_prefix: None,
        }
    }

    pub fn update_modifiers(&mut self, modifiers: Modifiers) {
        self.modifiers = modifiers.state();
    }

    pub fn on_focus_changed(&mut self, focused: bool) {
        if !focused {
            self.reset_prefix();
        }
    }

    /// Drop any pending prefix sequence when focus surface changes.
    pub fn clear_pending_prefix(&mut self) {
        self.reset_prefix();
    }

    pub fn current_modifiers(&self) -> ModifiersState {
        self.modifiers
    }

    pub fn translate_key_event(
        &mut self,
        key_event: &KeyEvent,
        input_map: &InputMap,
        context: KeybindingContext,
    ) -> Option<InputRouteOutcome> {
        if key_event.state != ElementState::Pressed || key_event.repeat {
            return None;
        }

        let normalized = NormalizedInput::from_key_event(key_event, self.modifiers);
        self.route_normalized_input(normalized, input_map, context, Instant::now())
    }

    fn route_normalized_input(
        &mut self,
        normalized: NormalizedInput,
        input_map: &InputMap,
        context: KeybindingContext,
        now: Instant,
    ) -> Option<InputRouteOutcome> {
        let input_debug = normalized.debug_label();
        self.reset_prefix_if_timed_out(now);

        if let Some(pending) = self.pending_prefix {
            if normalized.named_key == Some(NamedKey::Escape) {
                self.reset_prefix();
                return Some(InputRouteOutcome::NoDispatch {
                    input_debug,
                    route_debug: format!(
                        "mode={} focus={} -> prefix {} cancelled by Esc",
                        context.mode.as_str(),
                        context.focus.as_str(),
                        pending.namespace.as_str(),
                    ),
                });
            }

            if let Some(resolved) =
                input_map.resolve_prefixed(pending.namespace, &normalized, context)
            {
                self.reset_prefix();
                return Some(InputRouteOutcome::Dispatch(TranslatedInput {
                    input_debug,
                    route_debug: format!(
                        "mode={} focus={} -> {}",
                        context.mode.as_str(),
                        context.focus.as_str(),
                        resolved.reason
                    ),
                    command: resolved.command,
                }));
            }

            // Prefix bị gián đoạn: reset an toàn rồi fallback resolve key hiện tại như key thường.
            self.reset_prefix();
            if let Some(resolved) = input_map.resolve(&normalized, context) {
                return Some(InputRouteOutcome::Dispatch(TranslatedInput {
                    input_debug,
                    route_debug: format!(
                        "mode={} focus={} -> prefix interrupted, fallback -> {}",
                        context.mode.as_str(),
                        context.focus.as_str(),
                        resolved.reason
                    ),
                    command: resolved.command,
                }));
            }

            return Some(InputRouteOutcome::NoDispatch {
                input_debug,
                route_debug: format!(
                    "mode={} focus={} -> prefix interrupted and no fallback mapping",
                    context.mode.as_str(),
                    context.focus.as_str()
                ),
            });
        }

        if let Some((namespace, reason)) = input_map.resolve_prefix_start(&normalized, context) {
            self.pending_prefix = Some(PendingPrefix {
                namespace,
                started_at: now,
            });
            return Some(InputRouteOutcome::NoDispatch {
                input_debug,
                route_debug: format!(
                    "mode={} focus={} -> {}",
                    context.mode.as_str(),
                    context.focus.as_str(),
                    reason
                ),
            });
        }

        if let Some(resolved) = input_map.resolve(&normalized, context) {
            return Some(InputRouteOutcome::Dispatch(TranslatedInput {
                input_debug,
                route_debug: format!(
                    "mode={} focus={} -> {}",
                    context.mode.as_str(),
                    context.focus.as_str(),
                    resolved.reason
                ),
                command: resolved.command,
            }));
        }

        // Terminal surface fallback:
        // Route raw keys through command path instead of direct event-loop bypass.
        if context.focus == crate::app::input_map::InputFocusContext::Terminal
            && let Some(payload) = terminal_input_payload(&normalized)
        {
            return Some(InputRouteOutcome::Dispatch(TranslatedInput {
                input_debug,
                route_debug: format!(
                    "mode={} focus={} -> terminal raw input routing",
                    context.mode.as_str(),
                    context.focus.as_str()
                ),
                command: Command::TerminalWriteInput(payload),
            }));
        }

        None
    }

    /// IME commit path cho ngôn ngữ cần composition (ví dụ Vietnamese, Japanese...).
    pub fn translate_ime_commit(
        &self,
        text: &str,
        context: KeybindingContext,
    ) -> Option<TranslatedInput> {
        if text.is_empty() || text.chars().any(char::is_control) {
            return None;
        }

        if context.focus != crate::app::input_map::InputFocusContext::Editor {
            return None;
        }

        if context.mode == crate::core::mode::EditorMode::PaletteFocus && context.file_picker_open {
            return Some(TranslatedInput {
                input_debug: format!("IME Commit({text:?})"),
                route_debug: format!(
                    "mode={} focus={} -> IME commit -> FilePickerAppendQuery",
                    context.mode.as_str(),
                    context.focus.as_str()
                ),
                command: Command::FilePickerAppendQuery(text.to_string()),
            });
        }

        if context.mode != crate::core::mode::EditorMode::Insert {
            return None;
        }

        Some(TranslatedInput {
            input_debug: format!("IME Commit({text:?})"),
            route_debug: format!(
                "mode={} focus={} -> IME commit -> InsertText",
                context.mode.as_str(),
                context.focus.as_str()
            ),
            command: Command::InsertText(text.to_string()),
        })
    }

    fn reset_prefix_if_timed_out(&mut self, now: Instant) {
        if let Some(pending) = self.pending_prefix {
            if now.duration_since(pending.started_at) > self.prefix_timeout {
                self.reset_prefix();
            }
        }
    }

    fn reset_prefix(&mut self) {
        self.pending_prefix = None;
    }
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

fn terminal_input_payload(input: &NormalizedInput) -> Option<String> {
    // Esc is reserved for focus release (handled by normal command mapping).
    if input.named_key == Some(NamedKey::Escape) {
        return None;
    }

    // Avoid hijacking Command/Super shortcuts on macOS.
    if input.modifiers.super_key() {
        return None;
    }

    // Ctrl+key control sequences (Ctrl+C, Ctrl+L, ...)
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
        if input.modifiers.alt_key() && !input.modifiers.control_key() {
            // Common terminal behavior: Alt+key => ESC + key.
            return Some(format!("\u{1b}{text}"));
        }
        return Some(text.to_string());
    }

    None
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

#[cfg(test)]
mod tests {
    use std::{path::PathBuf, time::Duration};

    use winit::keyboard::{KeyCode, ModifiersState, NamedKey};

    use crate::{
        app::input_map::{InputMap, KeybindingContext},
        core::{commands::Command, mode::EditorMode},
    };

    use super::{InputHandler, InputRouteOutcome, NormalizedInput};

    #[test]
    fn debug_label_includes_key_and_modifiers() {
        let input = NormalizedInput {
            physical_key: Some(winit::keyboard::KeyCode::KeyA),
            named_key: None,
            text: Some("a".to_string()),
            modifiers: ModifiersState::CONTROL | ModifiersState::SHIFT,
        };

        assert_eq!(input.debug_label(), "KeyA + Ctrl+Shift");
    }

    fn char_input(ch: char, key: KeyCode) -> NormalizedInput {
        NormalizedInput {
            physical_key: Some(key),
            named_key: None,
            text: Some(ch.to_string()),
            modifiers: ModifiersState::empty(),
        }
    }

    fn named_input(named: NamedKey, physical: Option<KeyCode>) -> NormalizedInput {
        NormalizedInput {
            physical_key: physical,
            named_key: Some(named),
            text: None,
            modifiers: ModifiersState::empty(),
        }
    }

    #[test]
    fn prefix_leader_resolves_space_f() {
        let mut handler = InputHandler::new();
        let map = InputMap::new(PathBuf::from("phase7_test.txt"));
        let context = KeybindingContext::for_mode(EditorMode::Normal);
        let t0 = std::time::Instant::now();

        let start = handler.route_normalized_input(
            named_input(NamedKey::Space, Some(KeyCode::Space)),
            &map,
            context,
            t0,
        );
        assert!(matches!(start, Some(InputRouteOutcome::NoDispatch { .. })));

        let follow =
            handler.route_normalized_input(char_input('f', KeyCode::KeyF), &map, context, t0);
        match follow {
            Some(InputRouteOutcome::Dispatch(translated)) => {
                assert_eq!(translated.command, Command::OpenFileFinder);
            }
            other => panic!("expected dispatch, got {:?}", other),
        }
    }

    #[test]
    fn interrupted_prefix_falls_back_and_does_not_stick() {
        let mut handler = InputHandler::new();
        let map = InputMap::new(PathBuf::from("phase7_test.txt"));
        let context = KeybindingContext::for_mode(EditorMode::Normal);
        let t0 = std::time::Instant::now();

        let _ = handler.route_normalized_input(
            named_input(NamedKey::Space, Some(KeyCode::Space)),
            &map,
            context,
            t0,
        );

        // 'x' không có mapping ở prefix leader, cũng không có mapping thường -> NoDispatch.
        let interrupted =
            handler.route_normalized_input(char_input('x', KeyCode::KeyX), &map, context, t0);
        assert!(matches!(
            interrupted,
            Some(InputRouteOutcome::NoDispatch { .. })
        ));

        // Sau khi bị gián đoạn, router không kẹt trong prefix nữa.
        let after =
            handler.route_normalized_input(char_input('j', KeyCode::KeyJ), &map, context, t0);
        match after {
            Some(InputRouteOutcome::Dispatch(translated)) => {
                assert_eq!(translated.command, Command::MoveDown);
            }
            other => panic!("expected normal dispatch after interrupt, got {:?}", other),
        }
    }

    #[test]
    fn prefix_timeout_resets_safely() {
        let mut handler = InputHandler::new();
        let map = InputMap::new(PathBuf::from("phase7_test.txt"));
        let context = KeybindingContext::for_mode(EditorMode::Normal);
        let t0 = std::time::Instant::now();

        let _ = handler.route_normalized_input(
            named_input(NamedKey::Space, Some(KeyCode::Space)),
            &map,
            context,
            t0,
        );

        let after_timeout = t0 + Duration::from_millis(1_700);
        // Do timeout, 'f' không còn bị giải thích là leader f nữa.
        let expired = handler.route_normalized_input(
            char_input('f', KeyCode::KeyF),
            &map,
            context,
            after_timeout,
        );
        assert!(expired.is_none());

        let next = handler.route_normalized_input(
            char_input('j', KeyCode::KeyJ),
            &map,
            context,
            after_timeout,
        );
        match next {
            Some(InputRouteOutcome::Dispatch(translated)) => {
                assert_eq!(translated.command, Command::MoveDown);
            }
            other => panic!("expected router recovered after timeout, got {:?}", other),
        }
    }

    #[test]
    fn focus_loss_resets_prefix_safely() {
        let mut handler = InputHandler::new();
        let map = InputMap::new(PathBuf::from("phase7_test.txt"));
        let context = KeybindingContext::for_mode(EditorMode::Normal);
        let now = std::time::Instant::now();

        let _ = handler.route_normalized_input(
            named_input(NamedKey::Space, Some(KeyCode::Space)),
            &map,
            context,
            now,
        );
        handler.on_focus_changed(false);

        let after_focus_lost =
            handler.route_normalized_input(char_input('f', KeyCode::KeyF), &map, context, now);
        assert!(after_focus_lost.is_none());

        let next =
            handler.route_normalized_input(char_input('j', KeyCode::KeyJ), &map, context, now);
        match next {
            Some(InputRouteOutcome::Dispatch(translated)) => {
                assert_eq!(translated.command, Command::MoveDown);
            }
            other => panic!(
                "expected router recovered after focus loss, got {:?}",
                other
            ),
        }
    }

    #[test]
    fn leader_space_p_maps_to_open_command_palette() {
        let mut handler = InputHandler::new();
        let map = InputMap::new(PathBuf::from("phase7_test.txt"));
        let context = KeybindingContext::for_mode(EditorMode::Normal);
        let now = std::time::Instant::now();

        let _ = handler.route_normalized_input(
            named_input(NamedKey::Space, Some(KeyCode::Space)),
            &map,
            context,
            now,
        );
        let mapped =
            handler.route_normalized_input(char_input('p', KeyCode::KeyP), &map, context, now);

        match mapped {
            Some(InputRouteOutcome::Dispatch(translated)) => {
                assert_eq!(translated.command, Command::OpenCommandPalette);
            }
            other => panic!("expected dispatch for leader p, got {:?}", other),
        }
    }

    #[test]
    fn leader_space_t_maps_to_toggle_terminal() {
        let mut handler = InputHandler::new();
        let map = InputMap::new(PathBuf::from("phase7_test.txt"));
        let context = KeybindingContext::for_mode(EditorMode::Normal);
        let now = std::time::Instant::now();

        let _ = handler.route_normalized_input(
            named_input(NamedKey::Space, Some(KeyCode::Space)),
            &map,
            context,
            now,
        );
        let mapped =
            handler.route_normalized_input(char_input('t', KeyCode::KeyT), &map, context, now);

        match mapped {
            Some(InputRouteOutcome::Dispatch(translated)) => {
                assert_eq!(translated.command, Command::ToggleTerminal);
            }
            other => panic!("expected dispatch for leader t, got {:?}", other),
        }
    }

    #[test]
    fn terminal_focus_routes_printable_text_through_command_path() {
        let mut handler = InputHandler::new();
        let map = InputMap::new(PathBuf::from("phase7_test.txt"));
        let context = KeybindingContext::with_focus(
            EditorMode::TerminalFocus,
            crate::app::input_map::InputFocusContext::Terminal,
        );
        let now = std::time::Instant::now();

        let mapped =
            handler.route_normalized_input(char_input('j', KeyCode::KeyJ), &map, context, now);
        match mapped {
            Some(InputRouteOutcome::Dispatch(translated)) => {
                assert_eq!(
                    translated.command,
                    Command::TerminalWriteInput("j".to_string())
                );
            }
            other => panic!("expected terminal dispatch, got {:?}", other),
        }
    }

    #[test]
    fn terminal_focus_routes_arrow_keys_as_ansi_sequences() {
        let mut handler = InputHandler::new();
        let map = InputMap::new(PathBuf::from("phase7_test.txt"));
        let context = KeybindingContext::with_focus(
            EditorMode::TerminalFocus,
            crate::app::input_map::InputFocusContext::Terminal,
        );
        let now = std::time::Instant::now();

        let mapped = handler.route_normalized_input(
            named_input(NamedKey::ArrowUp, Some(KeyCode::ArrowUp)),
            &map,
            context,
            now,
        );
        match mapped {
            Some(InputRouteOutcome::Dispatch(translated)) => {
                assert_eq!(
                    translated.command,
                    Command::TerminalWriteInput("\u{1b}[A".to_string())
                );
            }
            other => panic!("expected terminal arrow dispatch, got {:?}", other),
        }
    }

    #[test]
    fn ime_commit_is_redirected_to_file_picker_when_palette_is_open() {
        let handler = InputHandler::new();
        let translated = handler
            .translate_ime_commit(
                "src",
                KeybindingContext::for_mode_with_picker(EditorMode::PaletteFocus, true),
            )
            .expect("palette ime commit should translate");

        assert_eq!(
            translated.command,
            Command::FilePickerAppendQuery("src".to_string())
        );
    }
}
