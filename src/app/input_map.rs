use std::path::{Path, PathBuf};

use winit::keyboard::{KeyCode, NamedKey};

use crate::{
    app::{
        input::NormalizedInput,
        resolved_keymap::{self, ResolvedKeymap, build, editor_mode_str},
    },
    config::keymap_loader::KeymapLoader,
    core::{
        commands::Command,
        mode::{EditorMode, ModeEvent},
    },
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputFocusContext {
    /// Center editor — normal editing surface.
    Editor,
    /// Left sidebar file explorer.
    Explorer,
    /// Right sidebar inspector / outline.
    Inspector,
    /// Bottom panel (non-terminal tabs: Problems, Debug Console, etc.).
    BottomPanel,
    /// Bottom panel in PTY terminal mode.
    Terminal,
}

impl InputFocusContext {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Editor => "editor",
            Self::Explorer => "explorer",
            Self::Inspector => "inspector",
            Self::BottomPanel => "bottom_panel",
            Self::Terminal => "terminal",
        }
    }

    /// True for surfaces that can start a leader-prefix sequence.
    pub fn allows_leader(self) -> bool {
        matches!(self, Self::Editor | Self::Explorer | Self::Inspector)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KeybindingContext {
    pub mode: EditorMode,
    pub focus: InputFocusContext,
    pub file_picker_open: bool,
}

impl KeybindingContext {
    pub fn for_mode(mode: EditorMode) -> Self {
        let focus = if mode == EditorMode::TerminalFocus {
            InputFocusContext::Terminal
        } else {
            InputFocusContext::Editor
        };
        Self {
            mode,
            focus,
            file_picker_open: false,
        }
    }

    pub fn for_mode_with_picker(mode: EditorMode, file_picker_open: bool) -> Self {
        let mut context = Self::for_mode(mode);
        context.file_picker_open = file_picker_open;
        context
    }

    pub fn with_focus(mode: EditorMode, focus: InputFocusContext) -> Self {
        Self {
            mode,
            focus,
            file_picker_open: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeybindingMatch {
    pub command: Command,
    pub reason: &'static str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrefixNamespace {
    Leader,
}

impl PrefixNamespace {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Leader => "leader",
        }
    }
}

// ── InputMap ──────────────────────────────────────────────────────────────────

/// Central keybinding resolver.
///
/// All bindings for Insert / Normal / Visual modes and global shortcuts come
/// from `ResolvedKeymap`, which is built from layered TOML profiles at startup.
///
/// Palette-focus and terminal-focus modes retain hardcoded logic because their
/// behavior depends on runtime state (file_picker_open, PTY forwarding) that
/// cannot be expressed in a static config file.
#[derive(Debug, Clone)]
pub struct InputMap {
    open_file_path: PathBuf,
    keymap: ResolvedKeymap,
}

impl InputMap {
    /// Create from an already-resolved keymap.
    pub fn new(open_file_path: PathBuf) -> Self {
        // Load active profile (NETHERIZE_PROFILE env var, default: "default").
        let toml_bindings = KeymapLoader::load_active(None);
        let keymap = build(&toml_bindings);
        Self {
            open_file_path,
            keymap,
        }
    }

    /// Create with a pre-built keymap (useful for tests or custom startup).
    pub fn with_keymap(open_file_path: PathBuf, keymap: ResolvedKeymap) -> Self {
        Self {
            open_file_path,
            keymap,
        }
    }

    pub fn open_file_path(&self) -> &Path {
        &self.open_file_path
    }

    pub fn resolve(
        &self,
        input: &NormalizedInput,
        context: KeybindingContext,
    ) -> Option<KeybindingMatch> {
        // ── Terminal focus: PTY isolation ────────────────────────────────────
        if context.focus == InputFocusContext::Terminal {
            return self.resolve_terminal_focus(input);
        }

        // ── Explorer focus: file-tree navigation ─────────────────────────────
        if context.focus == InputFocusContext::Explorer {
            return self.resolve_explorer_focus(input);
        }

        // ── Inspector focus: outline/inspector navigation ─────────────────────
        if context.focus == InputFocusContext::Inspector {
            return self.resolve_inspector_focus(input);
        }

        // ── Bottom panel (non-terminal): tab cycling ──────────────────────────
        if context.focus == InputFocusContext::BottomPanel {
            return self.resolve_bottom_panel_focus(input);
        }

        // ── Palette focus: state-dependent, kept hardcoded ────────────────────
        if context.mode == EditorMode::PaletteFocus {
            return self.resolve_palette_focus(input, context.file_picker_open);
        }

        // ── All other modes: driven by ResolvedKeymap ─────────────────────────
        let mode_str = editor_mode_str(context.mode);
        if let Some(command) =
            resolved_keymap::resolve_command(&self.keymap, input, mode_str, &self.open_file_path)
        {
            return Some(KeybindingMatch {
                command,
                reason: "keymap binding",
            });
        }

        // ── Dynamic catch-all for insert mode ─────────────────────────────────
        // Handles text input that has no explicit keymap binding.
        if context.mode == EditorMode::Insert {
            // Some platforms send Space as NamedKey::Space with text=None.
            if input.named_key == Some(NamedKey::Space) && !input.has_command_modifier() {
                return Some(KeybindingMatch {
                    command: Command::InsertChar(' '),
                    reason: "insert mode: Space -> InsertChar(' ')",
                });
            }
            if let Some(command) = insert_command_from_text(&input.text) {
                return Some(KeybindingMatch {
                    command,
                    reason: "insert mode: printable text",
                });
            }
        }

        None
    }

    pub fn resolve_prefix_start(
        &self,
        input: &NormalizedInput,
        context: KeybindingContext,
    ) -> Option<(PrefixNamespace, &'static str)> {
        if !context.focus.allows_leader() {
            return None;
        }
        // Explorer/Inspector don't use editor modes; allow leader from any non-insert state.
        if context.focus == InputFocusContext::Editor
            && !matches!(context.mode, EditorMode::Normal | EditorMode::Visual)
        {
            return None;
        }
        let is_space = input.named_key == Some(NamedKey::Space)
            || input.physical_key == Some(KeyCode::Space)
            || input.text.as_deref() == Some(" ");
        if !is_space || input.has_command_modifier() {
            return None;
        }
        Some((
            PrefixNamespace::Leader,
            "prefix start: Space -> Leader namespace",
        ))
    }

    pub fn resolve_prefixed(
        &self,
        namespace: PrefixNamespace,
        input: &NormalizedInput,
        context: KeybindingContext,
    ) -> Option<KeybindingMatch> {
        if !context.focus.allows_leader() {
            return None;
        }
        match namespace {
            PrefixNamespace::Leader => {
                resolved_keymap::resolve_leader_command(&self.keymap, input, &self.open_file_path)
                    .map(|command| KeybindingMatch {
                        command,
                        reason: "keymap: leader binding",
                    })
            }
        }
    }

    pub fn translate(
        &self,
        input: &NormalizedInput,
        context: KeybindingContext,
    ) -> Option<Command> {
        self.resolve(input, context).map(|m| m.command)
    }

    // ── Explorer surface (left sidebar) ──────────────────────────────────────

    fn resolve_explorer_focus(&self, input: &NormalizedInput) -> Option<KeybindingMatch> {
        if let Some(command) = resolved_keymap::resolve_command_mode_only(
            &self.keymap,
            input,
            "explorer",
            &self.open_file_path,
        ) {
            return Some(KeybindingMatch {
                command,
                reason: "explorer: explorer-mode keymap binding",
            });
        }

        // Esc or Ctrl+W → return focus to editor
        if input.named_key == Some(NamedKey::Escape) {
            return Some(KeybindingMatch {
                command: Command::FocusEditor,
                reason: "explorer: Esc -> FocusEditor",
            });
        }
        if input.has_command_modifier() && input.physical_key == Some(KeyCode::KeyW) {
            return Some(KeybindingMatch {
                command: Command::FocusEditor,
                reason: "explorer: Ctrl+W -> FocusEditor",
            });
        }

        // Tab / Shift+Tab → cycle panel tabs
        if input.named_key == Some(NamedKey::Tab) {
            let cmd = if input.modifiers.shift_key() {
                Command::PrevPanelTab
            } else {
                Command::NextPanelTab
            };
            return Some(KeybindingMatch {
                command: cmd,
                reason: "explorer: Tab -> NextPanelTab",
            });
        }

        // Delegate to leader / global bindings
        resolved_keymap::resolve_command(&self.keymap, input, "normal", &self.open_file_path).map(
            |command| KeybindingMatch {
                command,
                reason: "explorer: global binding",
            },
        )
    }

    // ── Inspector surface (right sidebar) ─────────────────────────────────────

    fn resolve_inspector_focus(&self, input: &NormalizedInput) -> Option<KeybindingMatch> {
        use KeyCode::*;

        // Esc or Ctrl+W → return focus to editor
        if input.named_key == Some(NamedKey::Escape)
            || (input.has_command_modifier() && input.physical_key == Some(KeyW))
        {
            return Some(KeybindingMatch {
                command: Command::FocusEditor,
                reason: "inspector: Esc/Ctrl+W -> FocusEditor",
            });
        }

        // j/↓ and k/↑ for scrolling outline
        if !input.has_command_modifier()
            && (input.named_key == Some(NamedKey::ArrowDown) || input.physical_key == Some(KeyJ))
        {
            return Some(KeybindingMatch {
                command: Command::ExplorerMoveDown,
                reason: "inspector: j/↓ -> scroll down",
            });
        }
        if !input.has_command_modifier()
            && (input.named_key == Some(NamedKey::ArrowUp) || input.physical_key == Some(KeyK))
        {
            return Some(KeybindingMatch {
                command: Command::ExplorerMoveUp,
                reason: "inspector: k/↑ -> scroll up",
            });
        }

        // Tab → cycle inspector tabs
        if input.named_key == Some(NamedKey::Tab) {
            let cmd = if input.modifiers.shift_key() {
                Command::PrevPanelTab
            } else {
                Command::NextPanelTab
            };
            return Some(KeybindingMatch {
                command: cmd,
                reason: "inspector: Tab -> NextPanelTab",
            });
        }

        resolved_keymap::resolve_command(&self.keymap, input, "normal", &self.open_file_path).map(
            |command| KeybindingMatch {
                command,
                reason: "inspector: global binding",
            },
        )
    }

    // ── Bottom panel (non-terminal tabs) ─────────────────────────────────────

    fn resolve_bottom_panel_focus(&self, input: &NormalizedInput) -> Option<KeybindingMatch> {
        use KeyCode::*;

        // Esc or Ctrl+W → return focus to editor
        if input.named_key == Some(NamedKey::Escape)
            || (input.has_command_modifier() && input.physical_key == Some(KeyW))
        {
            return Some(KeybindingMatch {
                command: Command::FocusEditor,
                reason: "bottom: Esc/Ctrl+W -> FocusEditor",
            });
        }

        // Tab / Shift+Tab → cycle bottom panel tabs
        if input.named_key == Some(NamedKey::Tab) {
            let cmd = if input.modifiers.shift_key() {
                Command::PrevPanelTab
            } else {
                Command::NextPanelTab
            };
            return Some(KeybindingMatch {
                command: cmd,
                reason: "bottom: Tab -> NextPanelTab",
            });
        }

        // j/↓ and k/↑ for scrolling panel content
        if !input.has_command_modifier()
            && (input.named_key == Some(NamedKey::ArrowDown) || input.physical_key == Some(KeyJ))
        {
            return Some(KeybindingMatch {
                command: Command::ExplorerMoveDown,
                reason: "bottom: j/↓ -> scroll down",
            });
        }
        if !input.has_command_modifier()
            && (input.named_key == Some(NamedKey::ArrowUp) || input.physical_key == Some(KeyK))
        {
            return Some(KeybindingMatch {
                command: Command::ExplorerMoveUp,
                reason: "bottom: k/↑ -> scroll up",
            });
        }

        // Delegate global bindings (mod+keys)
        resolved_keymap::resolve_command(&self.keymap, input, "normal", &self.open_file_path).map(
            |command| KeybindingMatch {
                command,
                reason: "bottom: global binding",
            },
        )
    }

    // ── Hardcoded palette focus ───────────────────────────────────────────────

    fn resolve_palette_focus(
        &self,
        input: &NormalizedInput,
        file_picker_open: bool,
    ) -> Option<KeybindingMatch> {
        if !file_picker_open {
            // Palette open but file picker not yet shown: only Escape handled.
            if input.named_key == Some(NamedKey::Escape) {
                return Some(KeybindingMatch {
                    command: Command::SwitchMode(ModeEvent::ExitFocus),
                    reason: "palette focus: Esc -> ExitFocus",
                });
            }
            return None;
        }

        if let Some(named) = input.named_key {
            let mapped = match named {
                NamedKey::Escape => Some(KeybindingMatch {
                    command: Command::CloseFilePicker,
                    reason: "palette focus: Esc -> CloseFilePicker",
                }),
                NamedKey::Enter => Some(KeybindingMatch {
                    command: Command::FilePickerConfirmSelection,
                    reason: "palette focus: Enter -> FilePickerConfirmSelection",
                }),
                NamedKey::ArrowUp => Some(KeybindingMatch {
                    command: Command::FilePickerSelectPrev,
                    reason: "palette focus: ArrowUp -> FilePickerSelectPrev",
                }),
                NamedKey::ArrowDown => Some(KeybindingMatch {
                    command: Command::FilePickerSelectNext,
                    reason: "palette focus: ArrowDown -> FilePickerSelectNext",
                }),
                NamedKey::Backspace => Some(KeybindingMatch {
                    command: Command::FilePickerBackspaceQuery,
                    reason: "palette focus: Backspace -> FilePickerBackspaceQuery",
                }),
                NamedKey::Space => Some(KeybindingMatch {
                    command: Command::FilePickerAppendQuery(" ".to_string()),
                    reason: "palette focus: Space -> FilePickerAppendQuery",
                }),
                _ => None,
            };
            if mapped.is_some() {
                return mapped;
            }
        }

        if let Some(command) = file_picker_query_from_text(&input.text) {
            return Some(KeybindingMatch {
                command,
                reason: "palette focus: text input -> FilePickerAppendQuery",
            });
        }
        None
    }

    // ── Hardcoded terminal focus ──────────────────────────────────────────────

    fn resolve_terminal_focus(&self, input: &NormalizedInput) -> Option<KeybindingMatch> {
        if let Some(command) = resolved_keymap::resolve_command_mode_only(
            &self.keymap,
            input,
            "terminal",
            &self.open_file_path,
        ) {
            return Some(KeybindingMatch {
                command,
                reason: "terminal focus: terminal-mode keymap binding",
            });
        }

        if input.has_command_modifier() && input.physical_key == Some(KeyCode::Backslash) {
            return Some(KeybindingMatch {
                command: Command::ToggleTerminal,
                reason: "terminal focus: Cmd/Ctrl+\\ -> ToggleTerminal",
            });
        }
        if input.named_key == Some(NamedKey::Escape) {
            return Some(KeybindingMatch {
                command: Command::FocusEditor,
                reason: "terminal focus: Esc -> FocusEditor",
            });
        }
        None
    }
}

impl Default for InputMap {
    fn default() -> Self {
        Self::new(PathBuf::from("phase4_open_sample.txt"))
    }
}

// ── Dynamic text helpers ──────────────────────────────────────────────────────

fn insert_command_from_text(text: &Option<String>) -> Option<Command> {
    let text = text.as_ref()?;
    if text.is_empty() || text.chars().any(char::is_control) {
        return None;
    }
    if text.chars().count() == 1 {
        return text.chars().next().map(Command::InsertChar);
    }
    Some(Command::InsertText(text.clone()))
}

fn file_picker_query_from_text(text: &Option<String>) -> Option<Command> {
    let text = text.as_ref()?;
    if text.is_empty() || text.chars().any(char::is_control) {
        return None;
    }
    Some(Command::FilePickerAppendQuery(text.clone()))
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use winit::keyboard::{KeyCode, ModifiersState, NamedKey};

    use crate::{
        app::resolved_keymap::builtin_defaults,
        core::{
            commands::Command,
            mode::{EditorMode, ModeEvent},
        },
    };

    use super::{InputFocusContext, InputMap, KeybindingContext, NormalizedInput, PrefixNamespace};

    fn make_map() -> InputMap {
        // Use with_keymap + builtin_defaults so tests never need TOML files.
        InputMap::with_keymap(PathBuf::from("test_open.txt"), builtin_defaults())
    }

    fn input_from_named(named_key: NamedKey) -> NormalizedInput {
        NormalizedInput {
            physical_key: None,
            named_key: Some(named_key),
            text: None,
            modifiers: ModifiersState::empty(),
        }
    }

    #[test]
    fn table_driven_keybinding_resolution() {
        struct Case {
            name: &'static str,
            context: KeybindingContext,
            input: NormalizedInput,
            expected: Option<Command>,
        }

        let map = make_map();
        let cases = [
            Case {
                name: "insert j -> InsertChar",
                context: KeybindingContext::for_mode(EditorMode::Insert),
                input: NormalizedInput {
                    physical_key: Some(KeyCode::KeyJ),
                    named_key: None,
                    text: Some("j".to_string()),
                    modifiers: ModifiersState::empty(),
                },
                expected: Some(Command::InsertChar('j')),
            },
            Case {
                name: "normal j -> MoveDown",
                context: KeybindingContext::for_mode(EditorMode::Normal),
                input: NormalizedInput {
                    physical_key: Some(KeyCode::KeyJ),
                    named_key: None,
                    text: Some("j".to_string()),
                    modifiers: ModifiersState::empty(),
                },
                expected: Some(Command::MoveDown),
            },
            Case {
                name: "insert escape -> SwitchToNormal",
                context: KeybindingContext::for_mode(EditorMode::Insert),
                input: input_from_named(NamedKey::Escape),
                expected: Some(Command::SwitchMode(ModeEvent::EnterNormal)),
            },
            Case {
                name: "cmd/ctrl p -> OpenFileFinder",
                context: KeybindingContext::for_mode(EditorMode::Insert),
                input: NormalizedInput {
                    physical_key: Some(KeyCode::KeyP),
                    named_key: None,
                    text: Some("p".to_string()),
                    modifiers: ModifiersState::CONTROL,
                },
                expected: Some(Command::OpenFileFinder),
            },
            Case {
                name: "terminal focus j -> None",
                context: KeybindingContext::with_focus(
                    EditorMode::TerminalFocus,
                    InputFocusContext::Terminal,
                ),
                input: NormalizedInput {
                    physical_key: Some(KeyCode::KeyJ),
                    named_key: None,
                    text: Some("j".to_string()),
                    modifiers: ModifiersState::empty(),
                },
                expected: None,
            },
            Case {
                name: "terminal focus ctrl+s -> None (forward to PTY)",
                context: KeybindingContext::with_focus(
                    EditorMode::TerminalFocus,
                    InputFocusContext::Terminal,
                ),
                input: NormalizedInput {
                    physical_key: Some(KeyCode::KeyS),
                    named_key: None,
                    text: Some("s".to_string()),
                    modifiers: ModifiersState::CONTROL,
                },
                expected: None,
            },
            Case {
                name: "terminal focus escape -> FocusEditor",
                context: KeybindingContext::with_focus(
                    EditorMode::TerminalFocus,
                    InputFocusContext::Terminal,
                ),
                input: input_from_named(NamedKey::Escape),
                expected: Some(Command::FocusEditor),
            },
            Case {
                name: "normal p without prefix -> None",
                context: KeybindingContext::for_mode(EditorMode::Normal),
                input: NormalizedInput {
                    physical_key: Some(KeyCode::KeyP),
                    named_key: None,
                    text: Some("p".to_string()),
                    modifiers: ModifiersState::empty(),
                },
                expected: None,
            },
            Case {
                name: "normal backtick -> ToggleTerminal",
                context: KeybindingContext::for_mode(EditorMode::Normal),
                input: NormalizedInput {
                    physical_key: Some(KeyCode::Backquote),
                    named_key: None,
                    text: Some("`".to_string()),
                    modifiers: ModifiersState::empty(),
                },
                expected: Some(Command::ToggleTerminal),
            },
            Case {
                name: "palette text -> FilePickerAppendQuery",
                context: KeybindingContext::for_mode_with_picker(EditorMode::PaletteFocus, true),
                input: NormalizedInput {
                    physical_key: Some(KeyCode::KeyA),
                    named_key: None,
                    text: Some("a".to_string()),
                    modifiers: ModifiersState::empty(),
                },
                expected: Some(Command::FilePickerAppendQuery("a".to_string())),
            },
            Case {
                name: "palette enter -> FilePickerConfirmSelection",
                context: KeybindingContext::for_mode_with_picker(EditorMode::PaletteFocus, true),
                input: input_from_named(NamedKey::Enter),
                expected: Some(Command::FilePickerConfirmSelection),
            },
            Case {
                name: "palette without picker text -> None",
                context: KeybindingContext::for_mode(EditorMode::PaletteFocus),
                input: NormalizedInput {
                    physical_key: Some(KeyCode::KeyA),
                    named_key: None,
                    text: Some("a".to_string()),
                    modifiers: ModifiersState::empty(),
                },
                expected: None,
            },
        ];

        for case in cases {
            let actual = map.resolve(&case.input, case.context).map(|m| m.command);
            assert_eq!(actual, case.expected, "case={}", case.name);
        }
    }

    #[test]
    fn ctrl_s_maps_to_save_file() {
        let map = make_map();
        let input = NormalizedInput {
            physical_key: Some(KeyCode::KeyS),
            named_key: None,
            text: Some("s".to_string()),
            modifiers: ModifiersState::CONTROL,
        };
        assert_eq!(
            map.translate(&input, KeybindingContext::for_mode(EditorMode::Insert)),
            Some(Command::SaveFile)
        );
    }

    #[test]
    fn ctrl_backslash_maps_to_toggle_terminal() {
        let map = make_map();
        let input = NormalizedInput {
            physical_key: Some(KeyCode::Backslash),
            named_key: None,
            text: Some("\\".to_string()),
            modifiers: ModifiersState::CONTROL,
        };
        assert_eq!(
            map.translate(&input, KeybindingContext::for_mode(EditorMode::Normal)),
            Some(Command::ToggleTerminal)
        );
    }

    #[test]
    fn char_without_modifiers_maps_to_insert_char_in_insert_mode() {
        let map = make_map();
        let input = NormalizedInput {
            physical_key: Some(KeyCode::KeyA),
            named_key: None,
            text: Some("a".to_string()),
            modifiers: ModifiersState::empty(),
        };
        assert_eq!(
            map.translate(&input, KeybindingContext::for_mode(EditorMode::Insert)),
            Some(Command::InsertChar('a'))
        );
    }

    #[test]
    fn named_space_maps_to_insert_space() {
        let map = make_map();
        let input = NormalizedInput {
            physical_key: Some(KeyCode::Space),
            named_key: Some(NamedKey::Space),
            text: None,
            modifiers: ModifiersState::empty(),
        };
        assert_eq!(
            map.translate(&input, KeybindingContext::for_mode(EditorMode::Insert)),
            Some(Command::InsertChar(' '))
        );
    }

    #[test]
    fn named_enter_maps_to_newline() {
        let map = make_map();
        let input = NormalizedInput {
            physical_key: Some(KeyCode::Enter),
            named_key: Some(NamedKey::Enter),
            text: None,
            modifiers: ModifiersState::empty(),
        };
        assert_eq!(
            map.translate(&input, KeybindingContext::for_mode(EditorMode::Insert)),
            Some(Command::Newline)
        );
    }

    #[test]
    fn multi_codepoint_text_maps_to_insert_text() {
        let map = make_map();
        // Decomposed "ê": 'e' (U+0065) + combining circumflex (U+0302) = 2 codepoints.
        let multi = "e\u{0302}".to_string();
        let input = NormalizedInput {
            physical_key: None,
            named_key: None,
            text: Some(multi.clone()),
            modifiers: ModifiersState::empty(),
        };
        assert_eq!(
            map.translate(&input, KeybindingContext::for_mode(EditorMode::Insert)),
            Some(Command::InsertText(multi))
        );
    }

    #[test]
    fn resolve_contains_reason_for_dispatch_trace() {
        let map = make_map();
        let input = NormalizedInput {
            physical_key: Some(KeyCode::KeyJ),
            named_key: None,
            text: Some("j".to_string()),
            modifiers: ModifiersState::empty(),
        };
        let resolved = map
            .resolve(&input, KeybindingContext::for_mode(EditorMode::Normal))
            .expect("should resolve");
        assert_eq!(resolved.command, Command::MoveDown);
        assert!(!resolved.reason.is_empty());
    }

    #[test]
    fn leader_prefix_start_and_resolution_work() {
        let map = make_map();
        let start_input = input_from_named(NamedKey::Space);
        let context = KeybindingContext::for_mode(EditorMode::Normal);

        let (namespace, reason) = map
            .resolve_prefix_start(&start_input, context)
            .expect("space should start leader prefix");
        assert_eq!(namespace, PrefixNamespace::Leader);
        assert!(reason.contains("Leader"));

        let follow_input = NormalizedInput {
            physical_key: Some(KeyCode::KeyF),
            named_key: None,
            text: Some("f".to_string()),
            modifiers: ModifiersState::empty(),
        };
        let resolved = map
            .resolve_prefixed(namespace, &follow_input, context)
            .expect("leader+f should resolve");
        assert_eq!(resolved.command, Command::OpenFileFinder);
    }

    #[test]
    fn leader_prefix_is_not_started_in_insert_mode() {
        let map = make_map();
        let start_input = input_from_named(NamedKey::Space);
        let context = KeybindingContext::for_mode(EditorMode::Insert);
        assert!(map.resolve_prefix_start(&start_input, context).is_none());
    }
}
