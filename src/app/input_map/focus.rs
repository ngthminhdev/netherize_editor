use winit::keyboard::KeyCode;

use super::helpers::palette_query_from_text;
use super::*;

impl InputMap {
    pub(super) fn resolve_settings_focus(
        &self,
        input: &NormalizedInput,
        context: KeybindingContext,
    ) -> Option<KeybindingMatch> {
        use KeyCode::*;

        let is_insert = context.mode == EditorMode::Insert;

        if is_insert {
            if input.has_command_modifier() && input.physical_key == Some(KeyV) {
                return Some(KeybindingMatch {
                    command: Command::EditorPaste,
                    reason: "settings edit: mod+v -> EditorPaste",
                });
            }

            if input.named_key == Some(NamedKey::Escape) {
                return Some(KeybindingMatch {
                    command: Command::CloseFilePicker,
                    reason: "settings edit: Esc -> cancel edit",
                });
            }

            if input.named_key == Some(NamedKey::Enter) {
                return Some(KeybindingMatch {
                    command: Command::SettingsActivate,
                    reason: "settings edit: Enter -> commit edit",
                });
            }

            if input.named_key == Some(NamedKey::Backspace) {
                return Some(KeybindingMatch {
                    command: Command::FilePickerBackspaceQuery,
                    reason: "settings edit: Backspace -> delete editing char",
                });
            }

            if input.named_key == Some(NamedKey::Space) {
                return Some(KeybindingMatch {
                    command: Command::FilePickerAppendQuery(" ".to_string()),
                    reason: "settings edit: Space -> append editing char",
                });
            }

            if let Some(command) = palette_query_from_text(&input.text) {
                return Some(KeybindingMatch {
                    command,
                    reason: "settings edit: text input -> append editing char",
                });
            }

            return None;
        }

        if (!input.has_command_modifier() && input.named_key == Some(NamedKey::ArrowDown))
            || (!input.has_command_modifier() && input.physical_key == Some(KeyJ))
        {
            return Some(KeybindingMatch {
                command: Command::SettingsSelectNext,
                reason: "settings: down/j -> SettingsSelectNext",
            });
        }

        if (!input.has_command_modifier() && input.named_key == Some(NamedKey::ArrowUp))
            || (!input.has_command_modifier() && input.physical_key == Some(KeyK))
        {
            return Some(KeybindingMatch {
                command: Command::SettingsSelectPrev,
                reason: "settings: up/k -> SettingsSelectPrev",
            });
        }

        if input.named_key == Some(NamedKey::Enter) {
            return Some(KeybindingMatch {
                command: Command::SettingsActivate,
                reason: "settings: Enter -> SettingsActivate",
            });
        }

        if input.named_key == Some(NamedKey::ArrowRight)
            || (!input.has_command_modifier() && input.physical_key == Some(KeyL))
        {
            return Some(KeybindingMatch {
                command: Command::SettingsAdjustIncrease,
                reason: "settings: l/right -> SettingsAdjustIncrease",
            });
        }

        if input.named_key == Some(NamedKey::ArrowLeft)
            || (!input.has_command_modifier() && input.physical_key == Some(KeyH))
        {
            return Some(KeybindingMatch {
                command: Command::SettingsAdjustDecrease,
                reason: "settings: h/left -> SettingsAdjustDecrease",
            });
        }

        if input.named_key == Some(NamedKey::Escape)
            || (!input.has_command_modifier() && input.physical_key == Some(KeyQ))
        {
            return Some(KeybindingMatch {
                command: Command::CloseFilePicker,
                reason: "settings: Esc/q -> cancel edit or close",
            });
        }

        resolved_keymap::resolve_global_command(&self.keymap, input, &self.open_file_path).map(
            |command| KeybindingMatch {
                command,
                reason: "settings: global binding",
            },
        )
    }

    pub(super) fn resolve_diagnostics_focus(
        &self,
        input: &NormalizedInput,
    ) -> Option<KeybindingMatch> {
        use KeyCode::*;

        if (!input.has_command_modifier() && input.named_key == Some(NamedKey::ArrowDown))
            || (!input.has_command_modifier() && input.physical_key == Some(KeyJ))
            || (input.modifiers.control_key()
                && !input.modifiers.super_key()
                && input.physical_key == Some(KeyN))
        {
            return Some(KeybindingMatch {
                command: Command::DiagnosticsSelectNext,
                reason: "diagnostics: down/ctrl+n -> DiagnosticsSelectNext",
            });
        }

        if (!input.has_command_modifier() && input.named_key == Some(NamedKey::ArrowUp))
            || (!input.has_command_modifier() && input.physical_key == Some(KeyK))
            || (input.modifiers.control_key()
                && !input.modifiers.super_key()
                && input.physical_key == Some(KeyP))
        {
            return Some(KeybindingMatch {
                command: Command::DiagnosticsSelectPrev,
                reason: "diagnostics: up/ctrl+p -> DiagnosticsSelectPrev",
            });
        }

        if input.named_key == Some(NamedKey::Escape)
            || (!input.has_command_modifier() && input.physical_key == Some(KeyQ))
        {
            return Some(KeybindingMatch {
                command: Command::BufferCloseCurrent,
                reason: "diagnostics: Esc/q -> BufferCloseCurrent",
            });
        }

        if input.named_key == Some(NamedKey::Enter) {
            return Some(KeybindingMatch {
                command: Command::DiagnosticsOpenSelection,
                reason: "diagnostics: Enter -> DiagnosticsOpenSelection",
            });
        }

        resolved_keymap::resolve_global_command(&self.keymap, input, &self.open_file_path).map(
            |command| KeybindingMatch {
                command,
                reason: "diagnostics: global binding",
            },
        )
    }

    pub(super) fn resolve_references_focus(
        &self,
        input: &NormalizedInput,
    ) -> Option<KeybindingMatch> {
        use KeyCode::*;

        if (!input.has_command_modifier() && input.named_key == Some(NamedKey::ArrowDown))
            || (!input.has_command_modifier() && input.physical_key == Some(KeyJ))
            || (input.modifiers.control_key()
                && !input.modifiers.super_key()
                && input.physical_key == Some(KeyN))
        {
            return Some(KeybindingMatch {
                command: Command::ReferencesSelectNext,
                reason: "references: down/ctrl+n -> ReferencesSelectNext",
            });
        }

        if (!input.has_command_modifier() && input.named_key == Some(NamedKey::ArrowUp))
            || (!input.has_command_modifier() && input.physical_key == Some(KeyK))
            || (input.modifiers.control_key()
                && !input.modifiers.super_key()
                && input.physical_key == Some(KeyP))
        {
            return Some(KeybindingMatch {
                command: Command::ReferencesSelectPrev,
                reason: "references: up/ctrl+p -> ReferencesSelectPrev",
            });
        }

        if input.named_key == Some(NamedKey::Escape)
            || (!input.has_command_modifier() && input.physical_key == Some(KeyQ))
        {
            return Some(KeybindingMatch {
                command: Command::BufferCloseCurrent,
                reason: "references: Esc/q -> BufferCloseCurrent",
            });
        }

        if input.named_key == Some(NamedKey::Enter) {
            return Some(KeybindingMatch {
                command: Command::ReferencesOpenSelection,
                reason: "references: Enter -> ReferencesOpenSelection",
            });
        }

        resolved_keymap::resolve_global_command(&self.keymap, input, &self.open_file_path).map(
            |command| KeybindingMatch {
                command,
                reason: "references: global binding",
            },
        )
    }

    pub(super) fn resolve_explorer_focus(
        &self,
        input: &NormalizedInput,
        welcome_visible: bool,
    ) -> Option<KeybindingMatch> {
        if welcome_visible {
            if (!input.has_command_modifier() && input.named_key == Some(NamedKey::ArrowDown))
                || (!input.has_command_modifier() && input.physical_key == Some(KeyCode::KeyJ))
                || (input.modifiers.control_key()
                    && !input.modifiers.super_key()
                    && input.physical_key == Some(KeyCode::KeyN))
            {
                return Some(KeybindingMatch {
                    command: Command::OverlaySelectNext,
                    reason: "welcome explorer focus: down/j/Ctrl+n -> SelectNext",
                });
            }

            if (!input.has_command_modifier() && input.named_key == Some(NamedKey::ArrowUp))
                || (!input.has_command_modifier() && input.physical_key == Some(KeyCode::KeyK))
                || (input.modifiers.control_key()
                    && !input.modifiers.super_key()
                    && input.physical_key == Some(KeyCode::KeyP))
            {
                return Some(KeybindingMatch {
                    command: Command::OverlaySelectPrev,
                    reason: "welcome explorer focus: up/k/Ctrl+p -> SelectPrev",
                });
            }

            if input.named_key == Some(NamedKey::Enter) {
                return Some(KeybindingMatch {
                    command: Command::FilePickerConfirmSelection,
                    reason: "welcome explorer focus: Enter -> ConfirmSelection",
                });
            }
        }

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

        if input.named_key == Some(NamedKey::Escape) {
            return Some(KeybindingMatch {
                command: Command::FocusEditor,
                reason: "explorer: Esc -> FocusEditor",
            });
        }
        if !input.has_command_modifier() && input.physical_key == Some(KeyCode::KeyQ) {
            return Some(KeybindingMatch {
                command: Command::CloseSidebars,
                reason: "explorer: q -> CloseSidebars (close explorer)",
            });
        }
        if input.has_command_modifier() && input.physical_key == Some(KeyCode::KeyW) {
            return Some(KeybindingMatch {
                command: Command::FocusEditor,
                reason: "explorer: Ctrl+W -> FocusEditor",
            });
        }

        if input.named_key == Some(NamedKey::Tab) {
            let command = if input.modifiers.shift_key() {
                Command::PrevPanelTab
            } else {
                Command::NextPanelTab
            };
            return Some(KeybindingMatch {
                command,
                reason: "explorer: Tab -> NextPanelTab",
            });
        }

        resolved_keymap::resolve_global_command(&self.keymap, input, &self.open_file_path).map(
            |command| KeybindingMatch {
                command,
                reason: "explorer: global binding",
            },
        )
    }

    pub(super) fn resolve_inspector_focus(
        &self,
        input: &NormalizedInput,
    ) -> Option<KeybindingMatch> {
        use KeyCode::*;

        if input.named_key == Some(NamedKey::Escape)
            || (input.has_command_modifier() && input.physical_key == Some(KeyW))
        {
            return Some(KeybindingMatch {
                command: Command::FocusEditor,
                reason: "inspector: Esc/Ctrl+W -> FocusEditor",
            });
        }

        if !input.has_command_modifier()
            && (input.named_key == Some(NamedKey::ArrowDown) || input.physical_key == Some(KeyJ))
        {
            return Some(KeybindingMatch {
                command: Command::ExplorerMoveDown,
                reason: "inspector: j/down -> scroll down",
            });
        }
        if !input.has_command_modifier()
            && (input.named_key == Some(NamedKey::ArrowUp) || input.physical_key == Some(KeyK))
        {
            return Some(KeybindingMatch {
                command: Command::ExplorerMoveUp,
                reason: "inspector: k/up -> scroll up",
            });
        }

        if input.named_key == Some(NamedKey::Tab) {
            let command = if input.modifiers.shift_key() {
                Command::PrevPanelTab
            } else {
                Command::NextPanelTab
            };
            return Some(KeybindingMatch {
                command,
                reason: "inspector: Tab -> NextPanelTab",
            });
        }

        resolved_keymap::resolve_global_command(&self.keymap, input, &self.open_file_path).map(
            |command| KeybindingMatch {
                command,
                reason: "inspector: global binding",
            },
        )
    }

    pub(super) fn resolve_markdown_preview_focus(
        &self,
        input: &NormalizedInput,
    ) -> Option<KeybindingMatch> {
        use KeyCode::*;

        if input.named_key == Some(NamedKey::Escape) {
            return Some(KeybindingMatch {
                command: Command::FocusBack,
                reason: "preview: Esc -> FocusBack",
            });
        }
        if !input.has_command_modifier() && input.physical_key == Some(KeyQ) {
            return Some(KeybindingMatch {
                command: Command::CloseSidebars,
                reason: "preview: q -> CloseSidebars (close preview)",
            });
        }
        if input.has_command_modifier() && input.physical_key == Some(KeyW) {
            return Some(KeybindingMatch {
                command: Command::FocusEditor,
                reason: "preview: Ctrl+W -> FocusEditor",
            });
        }

        if !input.has_command_modifier()
            && (input.named_key == Some(NamedKey::ArrowDown) || input.physical_key == Some(KeyJ))
        {
            return Some(KeybindingMatch {
                command: Command::MarkdownPreviewScrollDown,
                reason: "preview: j/down -> scroll down",
            });
        }
        if !input.has_command_modifier()
            && (input.named_key == Some(NamedKey::ArrowUp) || input.physical_key == Some(KeyK))
        {
            return Some(KeybindingMatch {
                command: Command::MarkdownPreviewScrollUp,
                reason: "preview: k/up -> scroll up",
            });
        }

        if !input.has_command_modifier()
            && input.physical_key == Some(KeyG)
            && input.modifiers.shift_key()
        {
            return Some(KeybindingMatch {
                command: Command::MarkdownPreviewScrollBottom,
                reason: "preview: G -> scroll bottom",
            });
        }

        if input.modifiers.control_key()
            && !input.modifiers.super_key()
            && input.physical_key == Some(KeyD)
        {
            return Some(KeybindingMatch {
                command: Command::MarkdownPreviewScrollHalfPageDown,
                reason: "preview: Ctrl+d -> scroll down half page",
            });
        }
        if input.modifiers.control_key()
            && !input.modifiers.super_key()
            && input.physical_key == Some(KeyU)
        {
            return Some(KeybindingMatch {
                command: Command::MarkdownPreviewScrollHalfPageUp,
                reason: "preview: Ctrl+u -> scroll up half page",
            });
        }

        if input.named_key == Some(NamedKey::Tab) {
            let command = if input.modifiers.shift_key() {
                Command::PrevPanelTab
            } else {
                Command::NextPanelTab
            };
            return Some(KeybindingMatch {
                command,
                reason: "preview: Tab -> NextPanelTab",
            });
        }

        resolved_keymap::resolve_global_command(&self.keymap, input, &self.open_file_path).map(
            |command| KeybindingMatch {
                command,
                reason: "preview: global binding",
            },
        )
    }

    pub(super) fn resolve_help_focus(&self, input: &NormalizedInput) -> Option<KeybindingMatch> {
        use KeyCode::*;

        if input.named_key == Some(NamedKey::Escape)
            || (input.has_command_modifier() && input.physical_key == Some(KeyW))
            || (!input.has_command_modifier() && input.physical_key == Some(KeyQ))
        {
            return Some(KeybindingMatch {
                command: Command::BufferCloseCurrent,
                reason: "help: Esc/q/Ctrl+W -> close help buffer",
            });
        }

        if !input.has_command_modifier()
            && (input.named_key == Some(NamedKey::ArrowDown) || input.physical_key == Some(KeyJ))
        {
            return Some(KeybindingMatch {
                command: Command::HelpScrollDown,
                reason: "help: j/down -> scroll down",
            });
        }
        if !input.has_command_modifier()
            && (input.named_key == Some(NamedKey::ArrowUp) || input.physical_key == Some(KeyK))
        {
            return Some(KeybindingMatch {
                command: Command::HelpScrollUp,
                reason: "help: k/up -> scroll up",
            });
        }

        if input.modifiers.control_key()
            && !input.modifiers.super_key()
            && input.physical_key == Some(KeyD)
        {
            return Some(KeybindingMatch {
                command: Command::HelpScrollHalfPageDown,
                reason: "help: Ctrl+d -> scroll down half page",
            });
        }
        if input.modifiers.control_key()
            && !input.modifiers.super_key()
            && input.physical_key == Some(KeyU)
        {
            return Some(KeybindingMatch {
                command: Command::HelpScrollHalfPageUp,
                reason: "help: Ctrl+u -> scroll up half page",
            });
        }

        resolved_keymap::resolve_global_command(&self.keymap, input, &self.open_file_path).map(
            |command| KeybindingMatch {
                command,
                reason: "help: global binding",
            },
        )
    }

    pub(super) fn resolve_bottom_panel_focus(
        &self,
        input: &NormalizedInput,
    ) -> Option<KeybindingMatch> {
        use KeyCode::*;

        if input.named_key == Some(NamedKey::Escape) {
            return Some(KeybindingMatch {
                command: Command::FocusBack,
                reason: "bottom: Esc -> FocusBack",
            });
        }
        if input.has_command_modifier() && input.physical_key == Some(KeyW) {
            return Some(KeybindingMatch {
                command: Command::FocusEditor,
                reason: "bottom: Ctrl+W -> FocusEditor",
            });
        }

        if input.named_key == Some(NamedKey::Tab) {
            let command = if input.modifiers.shift_key() {
                Command::PrevPanelTab
            } else {
                Command::NextPanelTab
            };
            return Some(KeybindingMatch {
                command,
                reason: "bottom: Tab -> NextPanelTab",
            });
        }

        if !input.has_command_modifier()
            && (input.named_key == Some(NamedKey::ArrowDown) || input.physical_key == Some(KeyJ))
        {
            return Some(KeybindingMatch {
                command: Command::TerminalScrollDown,
                reason: "bottom: j/down -> TerminalScrollDown",
            });
        }
        if !input.has_command_modifier()
            && (input.named_key == Some(NamedKey::ArrowUp) || input.physical_key == Some(KeyK))
        {
            return Some(KeybindingMatch {
                command: Command::TerminalScrollUp,
                reason: "bottom: k/up -> TerminalScrollUp",
            });
        }

        resolved_keymap::resolve_global_command(&self.keymap, input, &self.open_file_path).map(
            |command| KeybindingMatch {
                command,
                reason: "bottom: global binding",
            },
        )
    }

    pub(super) fn resolve_palette_focus(
        &self,
        input: &NormalizedInput,
        palette_visible: bool,
        palette_mode: Option<CommandPaletteMode>,
        welcome_visible: bool,
    ) -> Option<KeybindingMatch> {
        if !palette_visible {
            if welcome_visible {
                if input.modifiers.control_key() && !input.modifiers.super_key() {
                    use KeyCode::*;
                    match input.physical_key {
                        Some(KeyN) => {
                            return Some(KeybindingMatch {
                                command: Command::OverlaySelectNext,
                                reason: "palette focus without visible overlay: welcome Ctrl+n -> SelectNext",
                            });
                        }
                        Some(KeyP) => {
                            return Some(KeybindingMatch {
                                command: Command::OverlaySelectPrev,
                                reason: "palette focus without visible overlay: welcome Ctrl+p -> SelectPrev",
                            });
                        }
                        _ => {}
                    }
                }

                if !input.has_command_modifier() {
                    use KeyCode::*;
                    match input.physical_key {
                        Some(KeyJ) => {
                            return Some(KeybindingMatch {
                                command: Command::OverlaySelectNext,
                                reason: "palette focus without visible overlay: welcome j -> SelectNext",
                            });
                        }
                        Some(KeyK) => {
                            return Some(KeybindingMatch {
                                command: Command::OverlaySelectPrev,
                                reason: "palette focus without visible overlay: welcome k -> SelectPrev",
                            });
                        }
                        _ => {}
                    }
                }

                if input.named_key == Some(NamedKey::Enter) {
                    return Some(KeybindingMatch {
                        command: Command::FilePickerConfirmSelection,
                        reason: "palette focus without visible overlay: welcome Enter -> ConfirmSelection",
                    });
                }
            }

            if input.named_key == Some(NamedKey::Escape) {
                return Some(KeybindingMatch {
                    command: Command::SwitchMode(ModeEvent::ExitFocus),
                    reason: "palette focus: Esc -> ExitFocus",
                });
            }
            return None;
        }

        if let Some(command) = resolved_keymap::resolve_command_mode_only(
            &self.keymap,
            input,
            "palette",
            &self.open_file_path,
        ) {
            return Some(KeybindingMatch {
                command,
                reason: "palette focus: palette-mode keymap binding",
            });
        }

        if input.modifiers.control_key() && !input.modifiers.super_key() {
            use KeyCode::*;
            match input.physical_key {
                Some(KeyN) => {
                    return Some(KeybindingMatch {
                        command: Command::OverlaySelectNext,
                        reason: "palette focus: Ctrl+n -> SelectNext",
                    });
                }
                Some(KeyP) => {
                    return Some(KeybindingMatch {
                        command: Command::OverlaySelectPrev,
                        reason: "palette focus: Ctrl+p -> SelectPrev",
                    });
                }
                _ => {}
            }
        }

        if palette_mode == Some(CommandPaletteMode::RecentProjects) && !input.has_command_modifier()
        {
            use KeyCode::*;
            match input.physical_key {
                Some(KeyJ) => {
                    return Some(KeybindingMatch {
                        command: Command::OverlaySelectNext,
                        reason: "recent projects palette: j -> SelectNext",
                    });
                }
                Some(KeyK) => {
                    return Some(KeybindingMatch {
                        command: Command::OverlaySelectPrev,
                        reason: "recent projects palette: k -> SelectPrev",
                    });
                }
                _ => {}
            }
        }

        if let Some(named) = input.named_key {
            let mapped = match named {
                NamedKey::Escape => Some(KeybindingMatch {
                    command: Command::CloseFilePicker,
                    reason: "palette focus: Esc -> CloseCommandPalette",
                }),
                NamedKey::Enter => Some(KeybindingMatch {
                    command: Command::FilePickerConfirmSelection,
                    reason: "palette focus: Enter -> ConfirmSelection",
                }),
                NamedKey::ArrowUp => Some(KeybindingMatch {
                    command: Command::OverlaySelectPrev,
                    reason: "palette focus: ArrowUp -> SelectPrev",
                }),
                NamedKey::ArrowDown => Some(KeybindingMatch {
                    command: Command::OverlaySelectNext,
                    reason: "palette focus: ArrowDown -> SelectNext",
                }),
                NamedKey::Backspace => Some(KeybindingMatch {
                    command: Command::FilePickerBackspaceQuery,
                    reason: "palette focus: Backspace -> DeleteQueryChar",
                }),
                NamedKey::Space => Some(KeybindingMatch {
                    command: Command::FilePickerAppendQuery(" ".to_string()),
                    reason: "palette focus: Space -> AppendQueryChar",
                }),
                _ => None,
            };
            if mapped.is_some() {
                return mapped;
            }
        }

        if let Some(command) = palette_query_from_text(&input.text) {
            return Some(KeybindingMatch {
                command,
                reason: "palette focus: text input -> AppendQuery",
            });
        }
        None
    }

    pub(super) fn resolve_fuzzy_picker_focus(
        &self,
        input: &NormalizedInput,
        context: KeybindingContext,
    ) -> Option<KeybindingMatch> {
        use KeyCode::*;
        let is_insert = context.mode == EditorMode::Insert;

        if is_insert {
            if input.has_command_modifier() && input.physical_key == Some(KeyV) {
                return Some(KeybindingMatch {
                    command: Command::EditorPaste,
                    reason: "fuzzy picker: mod+v -> EditorPaste",
                });
            }
            if input.named_key == Some(NamedKey::Escape) {
                return Some(KeybindingMatch {
                    command: Command::SwitchMode(ModeEvent::Escape),
                    reason: "fuzzy picker: Esc -> Escape (Normal mode)",
                });
            }
            if input.named_key == Some(NamedKey::Enter) {
                return Some(KeybindingMatch {
                    command: Command::FilePickerConfirmSelection,
                    reason: "fuzzy picker: Enter -> ConfirmSelection",
                });
            }
            if input.named_key == Some(NamedKey::ArrowUp)
                || (input.modifiers.control_key()
                    && !input.modifiers.super_key()
                    && input.physical_key == Some(KeyP))
            {
                return Some(KeybindingMatch {
                    command: Command::OverlaySelectPrev,
                    reason: "fuzzy picker: ArrowUp/Ctrl+P -> SelectPrev",
                });
            }
            if input.named_key == Some(NamedKey::ArrowDown)
                || (input.modifiers.control_key()
                    && !input.modifiers.super_key()
                    && input.physical_key == Some(KeyN))
            {
                return Some(KeybindingMatch {
                    command: Command::OverlaySelectNext,
                    reason: "fuzzy picker: ArrowDown/Ctrl+N -> SelectNext",
                });
            }
            if input.named_key == Some(NamedKey::Backspace) {
                return Some(KeybindingMatch {
                    command: Command::FilePickerBackspaceQuery,
                    reason: "fuzzy picker: Backspace -> DeleteQueryChar",
                });
            }
            if input.named_key == Some(NamedKey::Space) {
                return Some(KeybindingMatch {
                    command: Command::FilePickerAppendQuery(" ".to_string()),
                    reason: "fuzzy picker: Space -> AppendQueryChar",
                });
            }
            if let Some(command) = palette_query_from_text(&input.text) {
                return Some(KeybindingMatch {
                    command,
                    reason: "fuzzy picker: text input -> AppendQuery",
                });
            }
            return None;
        }

        // In Normal mode, allow navigation and global commands (like `q`, `space x`)
        if input.named_key == Some(NamedKey::Enter) {
            return Some(KeybindingMatch {
                command: Command::FilePickerConfirmSelection,
                reason: "fuzzy picker: Enter -> ConfirmSelection",
            });
        }
        if input.named_key == Some(NamedKey::ArrowUp)
            || (!input.has_command_modifier() && input.physical_key == Some(KeyK))
        {
            return Some(KeybindingMatch {
                command: Command::OverlaySelectPrev,
                reason: "fuzzy picker: ArrowUp/k -> SelectPrev",
            });
        }
        if input.named_key == Some(NamedKey::ArrowDown)
            || (!input.has_command_modifier() && input.physical_key == Some(KeyJ))
        {
            return Some(KeybindingMatch {
                command: Command::OverlaySelectNext,
                reason: "fuzzy picker: ArrowDown/j -> SelectNext",
            });
        }
        if input.named_key == Some(NamedKey::Escape)
            || (!input.has_command_modifier() && input.physical_key == Some(KeyQ))
        {
            return Some(KeybindingMatch {
                command: Command::BufferCloseCurrent,
                reason: "fuzzy picker: Esc/q -> BufferCloseCurrent",
            });
        }
        if input.has_command_modifier() && input.physical_key == Some(KeyV) {
            return Some(KeybindingMatch {
                command: Command::EditorPaste,
                reason: "fuzzy picker: mod+v -> EditorPaste",
            });
        }

        resolved_keymap::resolve_global_command(&self.keymap, input, &self.open_file_path).map(
            |command| KeybindingMatch {
                command,
                reason: "fuzzy picker: global binding",
            },
        )
    }

    pub(super) fn resolve_terminal_focus(
        &self,
        input: &NormalizedInput,
        mode: EditorMode,
    ) -> Option<KeybindingMatch> {
        // Strict terminal mode: only consult terminal-mode bindings here.
        // Unmapped keys must fall through as raw PTY input instead of triggering globals.
        if let Some(command) = resolved_keymap::resolve_command_mode_only(
            &self.keymap,
            input,
            resolved_keymap::editor_mode_str(mode),
            &self.open_file_path,
        ) {
            return Some(KeybindingMatch {
                command,
                reason: "terminal focus: terminal-mode keymap binding",
            });
        }

        if mode == EditorMode::TerminalFocus && input.named_key == Some(NamedKey::Escape) {
            return Some(KeybindingMatch {
                command: Command::FocusBack,
                reason: "terminal focus: Esc -> FocusBack",
            });
        }
        None
    }
}
