use std::path::{Path, PathBuf};

use winit::keyboard::NamedKey;

use crate::{
    app::{
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

use helpers::{insert_command_from_text, palette_query_from_text};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputFocusContext {
    Editor,
    References,
    Diagnostics,
    Explorer,
    Inspector,
    BottomPanel,
    /// Bottom panel terminal (ESC = unfocus).
    Terminal,
    /// Buffer terminal chiếm toàn bộ center (lazygit, v.v.).
    /// Mọi input — kể cả ESC — được forward thẳng vào PTY.
    BufferTerminal,
    FuzzyPicker,
}

impl InputFocusContext {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Editor => "editor",
            Self::References => "references",
            Self::Diagnostics => "diagnostics",
            Self::Explorer => "explorer",
            Self::Inspector => "inspector",
            Self::BottomPanel => "bottom_panel",
            Self::Terminal => "terminal",
            Self::BufferTerminal => "buffer_terminal",
            Self::FuzzyPicker => "fuzzy_picker",
        }
    }

    pub fn allows_leader(self) -> bool {
        matches!(
            self,
            Self::Editor
                | Self::References
                | Self::Diagnostics
                | Self::Explorer
                | Self::Inspector
                | Self::FuzzyPicker
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KeybindingContext {
    pub mode: EditorMode,
    pub focus: InputFocusContext,
    pub command_palette_visible: bool,
    pub completion_visible: bool,
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
            completion_visible: false,
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
            completion_visible: false,
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
            return self.resolve_palette_focus(input, context.command_palette_visible);
        }

        // BufferTerminal (lazygit, v.v.): bypass keymap hoàn toàn,
        // mọi input sẽ được forward thẳng vào PTY trong handler.rs.
        if context.focus == InputFocusContext::BufferTerminal {
            return None;
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
        if context.focus == InputFocusContext::Terminal {
            return self.resolve_terminal_focus(input, context.mode);
        }
        if context.focus == InputFocusContext::Explorer {
            return self.resolve_explorer_focus(input);
        }
        if context.focus == InputFocusContext::Inspector {
            return self.resolve_inspector_focus(input);
        }
        if context.focus == InputFocusContext::BottomPanel {
            return self.resolve_bottom_panel_focus(input);
        }

        let mode_str = editor_mode_str(context.mode);
        if let Some(command) =
            resolved_keymap::resolve_command(&self.keymap, input, mode_str, &self.open_file_path)
        {
            return Some(KeybindingMatch {
                command,
                reason: "keymap binding",
            });
        }

        if context.mode == EditorMode::Insert {
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
        if context.focus == InputFocusContext::Terminal
            || context.focus == InputFocusContext::BufferTerminal
        {
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

    fn context_allows_leader_sequence(&self, context: KeybindingContext) -> bool {
        if !context.focus.allows_leader() {
            return false;
        }
        if context.focus == InputFocusContext::Editor {
            return matches!(context.mode, EditorMode::Normal | EditorMode::Visual);
        }
        true
    }

    fn sequence_mode_str(&self, context: KeybindingContext) -> &'static str {
        match context.focus {
            InputFocusContext::Editor => editor_mode_str(context.mode),
            InputFocusContext::References => editor_mode_str(context.mode),
            InputFocusContext::Diagnostics => editor_mode_str(context.mode),
            InputFocusContext::Explorer => "explorer",
            InputFocusContext::Inspector => "inspector",
            InputFocusContext::Terminal => editor_mode_str(context.mode),
            InputFocusContext::BufferTerminal => "terminal",
            InputFocusContext::BottomPanel => "bottom_panel",
            InputFocusContext::FuzzyPicker => editor_mode_str(context.mode),
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
