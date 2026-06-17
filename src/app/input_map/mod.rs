use std::path::{Path, PathBuf};

use winit::keyboard::{KeyCode, NamedKey};

use crate::{
    app::{
        command_palette::CommandPaletteMode,
        input::NormalizedInput,
        resolved_keymap::{self, KeySpec, ResolvedKeymap, SequenceLookup, build, editor_mode_str},
    },
    config::keymap_loader::KeymapLoader,
    core::{
        command_ids,
        commands::Command,
        mode::{EditorMode, ModeEvent},
    },
};

mod focus;
mod helpers;

#[cfg(test)]
mod tests;

use helpers::insert_command_from_text;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputFocusContext {
    Editor,
    /// Welcome home screen owns input independently of editor/sidebar focus.
    Welcome,
    References,
    Diagnostics,
    Explorer,
    Inspector,
    /// Right sidebar AI Chat tab — input goes to chat input_box.
    AiChat,
    /// Right sidebar Markdown Preview tab — scroll with j/k/Ctrl-u/Ctrl-d.
    MarkdownPreview,
    /// Right sidebar Help / Cheat Sheet — scroll with j/k/Ctrl-u/Ctrl-d.
    Help,
    ExtensionsManager,
    BottomPanel,
    /// Bottom panel Test Runner tab — modal: nav mode (j/k/i/a) + edit mode.
    TestRunner,
    /// Bottom panel terminal (ESC = unfocus).
    Terminal,
    /// Buffer terminal chiếm toàn bộ center (lazygit, v.v.).
    /// Mọi input — kể cả ESC — được forward thẳng vào PTY.
    BufferTerminal,
    FuzzyPicker,
    SettingsTab,
    /// Right sidebar Outline tab.
    Outline,
    /// Code Graph HUD overlay (gp) — owns hjkl/Enter/Esc while open.
    CodeGraph,
}

impl InputFocusContext {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Editor => "editor",
            Self::Welcome => "welcome",
            Self::References => "references",
            Self::Diagnostics => "diagnostics",
            Self::Explorer => "explorer",
            Self::Inspector => "inspector",
            Self::AiChat => "ai_chat",
            Self::MarkdownPreview => "markdown_preview",
            Self::Help => "help",
            Self::ExtensionsManager => "extensions_manager",
            Self::BottomPanel => "bottom_panel",
            Self::TestRunner => "test_runner",
            Self::Terminal => "terminal",
            Self::BufferTerminal => "buffer_terminal",
            Self::FuzzyPicker => "fuzzy_picker",
            Self::SettingsTab => "settings_tab",
            Self::Outline => "outline",
            Self::CodeGraph => "code_graph",
        }
    }

    pub fn allows_leader(self) -> bool {
        matches!(
            self,
            Self::Editor
                | Self::Welcome
                | Self::References
                | Self::Diagnostics
                | Self::Explorer
                | Self::Inspector
                | Self::Terminal
                | Self::AiChat
                | Self::MarkdownPreview
                | Self::Help
                | Self::ExtensionsManager
                | Self::FuzzyPicker
                | Self::SettingsTab
                | Self::Outline
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KeybindingContext {
    pub mode: EditorMode,
    pub focus: InputFocusContext,
    pub command_palette_visible: bool,
    pub command_palette_mode: Option<CommandPaletteMode>,
    pub welcome_visible: bool,
    pub completion_visible: bool,
    /// True when AI ghost text is showing at the caret. Lets the input layer
    /// route Tab to accept the suggestion (completion menu still wins).
    pub inline_suggestion_visible: bool,
    pub hover_overlay_visible: bool,
    pub zen_mode_active: bool,
    /// True when the focused terminal is the right-sidebar opencode chat. Lets
    /// the input layer rebind Ctrl+U/Ctrl+D to scroll instead of forwarding them
    /// as raw control bytes, without affecting the bottom-panel shell.
    pub right_sidebar_terminal: bool,
    /// True when the Test Runner panel is in edit mode (typing edits a field).
    /// Lets the input layer switch between nav-mode and edit-mode key handling.
    pub test_runner_editing: bool,
    /// True when the AI Chat tab is showing its in-panel agent picker (no agent
    /// running). Lets the input layer route j/k/Enter to the picker.
    pub ai_agent_picker_active: bool,
}

impl KeybindingContext {
    pub fn for_mode(mode: EditorMode) -> Self {
        let focus = match mode {
            EditorMode::TerminalFocus | EditorMode::TerminalNormal => InputFocusContext::Terminal,
            _ => InputFocusContext::Editor,
        };
        Self {
            mode,
            focus,
            command_palette_visible: false,
            command_palette_mode: None,
            welcome_visible: false,
            completion_visible: false,
            inline_suggestion_visible: false,
            hover_overlay_visible: false,
            zen_mode_active: false,
            right_sidebar_terminal: false,
            test_runner_editing: false,
            ai_agent_picker_active: false,
        }
    }

    pub fn for_mode_with_palette(mode: EditorMode, command_palette_visible: bool) -> Self {
        let mut context = Self::for_mode(mode);
        context.command_palette_visible = command_palette_visible;
        context
    }

    pub fn for_mode_with_picker(mode: EditorMode, file_picker_open: bool) -> Self {
        Self::for_mode_with_palette(mode, file_picker_open)
    }

    pub fn with_focus(mode: EditorMode, focus: InputFocusContext) -> Self {
        Self {
            mode,
            focus,
            command_palette_visible: false,
            command_palette_mode: None,
            welcome_visible: false,
            completion_visible: false,
            inline_suggestion_visible: false,
            hover_overlay_visible: false,
            zen_mode_active: false,
            right_sidebar_terminal: false,
            test_runner_editing: false,
            ai_agent_picker_active: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeybindingMatch {
    pub command: Command,
    pub reason: &'static str,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingSequence {
    pub steps: Vec<KeySpec>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SequenceMatch {
    Dispatch(KeybindingMatch),
    Pending(PendingSequence),
}

/// One row of the which-key overlay: the next key of a pending chord plus a
/// human-readable description of what it does.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WhichKeyEntry {
    /// Display token (e.g. "f", "<Space>").
    pub key: String,
    /// Humanized command label, or "+more" for a deeper chord group.
    pub label: String,
    pub is_group: bool,
}

/// "editor.move_word_forward" -> "Move word forward".
fn humanize_command_id(id: &str) -> String {
    let tail = id.rsplit('.').next().unwrap_or(id);
    let label = tail.replace('_', " ");
    let mut chars = label.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => label,
    }
}

#[derive(Debug, Clone)]
pub struct InputMap {
    open_file_path: PathBuf,
    keymap: ResolvedKeymap,
}

impl InputMap {
    pub fn new(open_file_path: PathBuf) -> Self {
        let toml_bindings = KeymapLoader::load_active(None);
        let keymap = build(&toml_bindings);
        Self {
            open_file_path,
            keymap,
        }
    }

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
        if context.command_palette_visible || context.mode == EditorMode::PaletteFocus {
            return self.resolve_palette_focus(
                input,
                context.command_palette_visible,
                context.command_palette_mode,
                context.welcome_visible,
            );
        }

        // Cmd + digit switches dock tabs when a dock panel is focused. This is
        // resolved here — ahead of the per-mode keymap — so it stays correct even
        // when the editor mode has drifted (e.g. the right dock keeps `TerminalFocus`
        // after switching off the AI Chat terminal tab to the Test Runner, which
        // would otherwise route Cmd+digit to the terminal-tab bindings).
        if input.has_command_modifier() {
            let digit = input.physical_key.and_then(|k| match k {
                KeyCode::Digit1 => Some(0),
                KeyCode::Digit2 => Some(1),
                KeyCode::Digit3 => Some(2),
                KeyCode::Digit4 => Some(3),
                KeyCode::Digit5 => Some(4),
                KeyCode::Digit6 => Some(5),
                KeyCode::Digit7 => Some(6),
                KeyCode::Digit8 => Some(7),
                KeyCode::Digit9 => Some(8),
                _ => None,
            });
            if let Some(digit) = digit {
                match context.focus {
                    InputFocusContext::Explorer | InputFocusContext::Outline => {
                        return Some(KeybindingMatch {
                            command: Command::SwitchLeftTab(digit),
                            reason: "left sidebar focus: cmd + digit -> switch left tab",
                        });
                    }
                    InputFocusContext::AiChat
                    | InputFocusContext::TestRunner
                    | InputFocusContext::Inspector => {
                        return Some(KeybindingMatch {
                            command: Command::SwitchRightTab(digit),
                            reason: "right sidebar focus: cmd + digit -> switch right tab",
                        });
                    }
                    _ => {}
                }
            }
        }

        // Resize mode is modal: once active it owns h/j/k/l + Esc regardless of
        // which panel holds focus, so the keys aren't swallowed by focus-specific
        // handlers (explorer navigation, terminal copy mode, …).
        if context.mode == EditorMode::Resize {
            return resolved_keymap::resolve_command(
                &self.keymap,
                input,
                "resize",
                &self.open_file_path,
            )
            .map(|command| KeybindingMatch {
                command,
                reason: "resize mode binding",
            });
        }

        if context.hover_overlay_visible
            && context.focus == InputFocusContext::Editor
            && context.mode == EditorMode::Normal
            && input.modifiers.control_key()
            && !input.modifiers.super_key()
        {
            match input.physical_key {
                Some(KeyCode::KeyD) => {
                    return Some(KeybindingMatch {
                        command: Command::ScrollHalfPageDown,
                        reason: "hover overlay: Ctrl+d -> scroll hover down",
                    });
                }
                Some(KeyCode::KeyU) => {
                    return Some(KeybindingMatch {
                        command: Command::ScrollHalfPageUp,
                        reason: "hover overlay: Ctrl+u -> scroll hover up",
                    });
                }
                _ => {}
            }
        }

        // BufferTerminal (lazygit, v.v.): bypass keymap hoàn toàn,
        // mọi input sẽ được forward thẳng vào PTY trong handler.rs.
        // NHƯNG: Cho phép Cmd-R và F12 đi qua để resolve bình thường.
        if context.focus == InputFocusContext::BufferTerminal {
            let is_global_layout_toggle = (input.has_command_modifier()
                && input.physical_key == Some(KeyCode::KeyR))
                || input.named_key == Some(NamedKey::F12);
            if !is_global_layout_toggle {
                return None;
            }
        }

        // AI Chat: bypass keymap — all input handled by handler.rs.
        if context.focus == InputFocusContext::AiChat {
            if !input.has_command_modifier() {
                return None;
            }
        }

        // Test Runner: bypass keymap — modal input handled by handler.rs.
        if context.focus == InputFocusContext::TestRunner {
            if !input.has_command_modifier() {
                return None;
            }
        }

        // Outline: bypass keymap — all input handled by handler.rs.
        if context.focus == InputFocusContext::Outline {
            if !input.has_command_modifier() {
                return None;
            }
        }

        if context.focus == InputFocusContext::CodeGraph {
            return self.resolve_code_graph_focus(input);
        }
        if context.focus == InputFocusContext::References {
            return self.resolve_references_focus(input);
        }
        if context.focus == InputFocusContext::Diagnostics {
            return self.resolve_diagnostics_focus(input);
        }
        if context.focus == InputFocusContext::FuzzyPicker {
            return self.resolve_fuzzy_picker_focus(input, context);
        }
        if context.focus == InputFocusContext::SettingsTab {
            return self.resolve_settings_focus(input, context);
        }
        if context.focus == InputFocusContext::Terminal {
            return self.resolve_terminal_focus(input, context);
        }
        if context.focus == InputFocusContext::Explorer {
            return self.resolve_explorer_focus(input, context.welcome_visible);
        }
        if context.focus == InputFocusContext::Inspector {
            return self.resolve_inspector_focus(input);
        }
        if context.focus == InputFocusContext::MarkdownPreview {
            return self.resolve_markdown_preview_focus(input);
        }
        if context.focus == InputFocusContext::Help {
            return self.resolve_help_focus(input);
        }
        if context.focus == InputFocusContext::ExtensionsManager {
            return self
                .resolve_extensions_manager_focus(input, context.mode == EditorMode::Insert);
        }
        if context.focus == InputFocusContext::BottomPanel {
            return self.resolve_bottom_panel_focus(input);
        }

        if context.welcome_visible {
            match input.physical_key {
                Some(KeyCode::KeyJ) if !input.has_command_modifier() => {
                    return Some(KeybindingMatch {
                        command: Command::OverlaySelectNext,
                        reason: "welcome recent projects: j -> SelectNext",
                    });
                }
                Some(KeyCode::KeyK) if !input.has_command_modifier() => {
                    return Some(KeybindingMatch {
                        command: Command::OverlaySelectPrev,
                        reason: "welcome recent projects: k -> SelectPrev",
                    });
                }
                Some(KeyCode::KeyN)
                    if input.modifiers.control_key() && !input.modifiers.super_key() =>
                {
                    return Some(KeybindingMatch {
                        command: Command::OverlaySelectNext,
                        reason: "welcome recent projects: Ctrl+n -> SelectNext",
                    });
                }
                Some(KeyCode::KeyP)
                    if input.modifiers.control_key() && !input.modifiers.super_key() =>
                {
                    return Some(KeybindingMatch {
                        command: Command::OverlaySelectPrev,
                        reason: "welcome recent projects: Ctrl+p -> SelectPrev",
                    });
                }
                _ => {}
            }
            if !input.has_command_modifier() && input.named_key == Some(NamedKey::Enter) {
                return Some(KeybindingMatch {
                    command: Command::FilePickerConfirmSelection,
                    reason: "welcome recent projects: Enter -> ConfirmSelection",
                });
            }
        }

        let mode_str = editor_mode_str(context.mode);
        if let Some(command) =
            resolved_keymap::resolve_command(&self.keymap, input, mode_str, &self.open_file_path)
        {
            if !Self::command_allowed_in_context(&command, context) {
                return None;
            }

            return Some(KeybindingMatch {
                command,
                reason: "keymap binding",
            });
        }

        if matches!(context.mode, EditorMode::Insert | EditorMode::MultiInsert) {
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

    pub fn resolve_sequence_start(
        &self,
        input: &NormalizedInput,
        context: KeybindingContext,
    ) -> Option<SequenceMatch> {
        self.resolve_sequence_from_steps(&[], input, context)
    }

    pub fn resolve_sequence_next(
        &self,
        pending: &PendingSequence,
        input: &NormalizedInput,
        context: KeybindingContext,
    ) -> Option<SequenceMatch> {
        self.resolve_sequence_from_steps(&pending.steps, input, context)
    }

    fn resolve_sequence_from_steps(
        &self,
        previous_steps: &[KeySpec],
        input: &NormalizedInput,
        context: KeybindingContext,
    ) -> Option<SequenceMatch> {
        if context.command_palette_visible || context.mode == EditorMode::PaletteFocus {
            return None;
        }
        // Block keybinding resolution for terminal focus (keys go to PTY)
        // but ALLOW leader sequences in TerminalNormal mode (copy mode)
        if context.focus == InputFocusContext::BufferTerminal {
            return None;
        }
        if context.focus == InputFocusContext::Terminal
            && context.mode != EditorMode::TerminalNormal
            && !context.zen_mode_active
        {
            return None;
        }
        if context.focus == InputFocusContext::AiChat && !context.zen_mode_active {
            return None;
        }

        let allow_leader = self.context_allows_leader_sequence(context);
        let mode_for_sequence = self.sequence_mode_str(context);
        let candidates = resolved_keymap::sequence_step_candidates(input, allow_leader);
        let mut pending: Option<PendingSequence> = None;

        for candidate in candidates {
            let mut steps = previous_steps.to_vec();
            steps.push(candidate);

            match self.keymap.lookup_sequence(&steps, mode_for_sequence) {
                SequenceLookup::Exact(id) => {
                    let command = command_ids::parse(id, Some(&self.open_file_path))?;
                    if !Self::command_allowed_in_context(&command, context) {
                        return None;
                    }
                    if !Self::zen_mode_allows_sequence_command(&command, context) {
                        continue;
                    }

                    return Some(SequenceMatch::Dispatch(KeybindingMatch {
                        command,
                        reason: "keymap: chord binding",
                    }));
                }
                SequenceLookup::Prefix => {
                    if pending.is_none() {
                        pending = Some(PendingSequence { steps });
                    }
                }
                SequenceLookup::None => {}
            }
        }

        pending.map(SequenceMatch::Pending)
    }

    fn command_allowed_in_context(command: &Command, context: KeybindingContext) -> bool {
        if matches!(command, Command::SwitchMode(ModeEvent::EnterResize)) {
            // Resize mode can be entered while the editor, the explorer, or the
            // terminal hold focus, so the focused panel can be resized in place.
            return matches!(
                context.focus,
                InputFocusContext::Editor
                    | InputFocusContext::Explorer
                    | InputFocusContext::Terminal
            );
        }

        true
    }

    fn zen_mode_allows_sequence_command(command: &Command, context: KeybindingContext) -> bool {
        if !context.zen_mode_active {
            return true;
        }

        match context.focus {
            // These focuses normally consume raw input. While Zen Mode is active we
            // still allow the dedicated "space z m" escape hatch, but other
            // sequences should not steal keystrokes from the embedded surface.
            InputFocusContext::AiChat => matches!(command, Command::ToggleMaximizeFocus),
            InputFocusContext::Terminal if context.mode != EditorMode::TerminalNormal => {
                matches!(command, Command::ToggleMaximizeFocus)
            }
            _ => true,
        }
    }

    fn context_allows_leader_sequence(&self, context: KeybindingContext) -> bool {
        if context.zen_mode_active {
            return true;
        }
        if !context.focus.allows_leader() {
            return false;
        }
        if context.welcome_visible
            && matches!(
                context.focus,
                InputFocusContext::Editor | InputFocusContext::Welcome
            )
        {
            return true;
        }
        if context.focus == InputFocusContext::Editor {
            return matches!(context.mode, EditorMode::Normal | EditorMode::Visual);
        }
        true
    }

    /// Continuations of a pending chord, ready for the which-key overlay.
    /// Entries whose command id cannot resolve to a real command are hidden,
    /// mirroring dispatch behavior.
    pub fn whichkey_entries(
        &self,
        pending: &PendingSequence,
        context: KeybindingContext,
    ) -> Vec<WhichKeyEntry> {
        let mode_str = self.sequence_mode_str(context);
        self.keymap
            .sequence_continuations(&pending.steps, mode_str)
            .into_iter()
            .filter_map(|cont| match cont.command_id {
                Some(id) => {
                    command_ids::parse(&id, Some(&self.open_file_path))?;
                    Some(WhichKeyEntry {
                        key: cont.key,
                        label: humanize_command_id(&id),
                        is_group: false,
                    })
                }
                None => Some(WhichKeyEntry {
                    key: cont.key,
                    label: "+more".to_string(),
                    is_group: true,
                }),
            })
            .collect()
    }

    fn sequence_mode_str(&self, context: KeybindingContext) -> &'static str {
        match context.focus {
            InputFocusContext::Editor if context.welcome_visible => "normal",
            InputFocusContext::Welcome => "normal",
            InputFocusContext::Editor => editor_mode_str(context.mode),
            InputFocusContext::References => editor_mode_str(context.mode),
            InputFocusContext::Diagnostics => editor_mode_str(context.mode),
            InputFocusContext::Explorer => "explorer",
            InputFocusContext::Inspector => "inspector",
            InputFocusContext::AiChat if context.zen_mode_active => "normal",
            InputFocusContext::AiChat => "ai_chat",
            InputFocusContext::MarkdownPreview => "preview",
            InputFocusContext::Help => "help",
            InputFocusContext::ExtensionsManager => "help",
            InputFocusContext::Terminal => editor_mode_str(context.mode),
            InputFocusContext::BufferTerminal => "terminal",
            InputFocusContext::BottomPanel => "bottom_panel",
            InputFocusContext::TestRunner => "test_runner",
            InputFocusContext::FuzzyPicker => editor_mode_str(context.mode),
            InputFocusContext::SettingsTab => editor_mode_str(context.mode),
            InputFocusContext::Outline => "normal",
            InputFocusContext::CodeGraph => "code_graph",
        }
    }

    pub fn translate(
        &self,
        input: &NormalizedInput,
        context: KeybindingContext,
    ) -> Option<Command> {
        self.resolve(input, context).map(|matched| matched.command)
    }
}

impl Default for InputMap {
    fn default() -> Self {
        Self::new(PathBuf::from("phase4_open_sample.txt"))
    }
}
