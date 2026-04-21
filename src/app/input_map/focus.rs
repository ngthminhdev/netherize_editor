use winit::keyboard::KeyCode;

use super::*;

impl InputMap {
    pub(super) fn resolve_explorer_focus(
        &self,
        input: &NormalizedInput,
    ) -> Option<KeybindingMatch> {
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

    pub(super) fn resolve_bottom_panel_focus(
        &self,
        input: &NormalizedInput,
    ) -> Option<KeybindingMatch> {
        use KeyCode::*;

        if input.named_key == Some(NamedKey::Escape)
            || (input.has_command_modifier() && input.physical_key == Some(KeyW))
        {
            return Some(KeybindingMatch {
                command: Command::FocusEditor,
                reason: "bottom: Esc/Ctrl+W -> FocusEditor",
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
    ) -> Option<KeybindingMatch> {
        if !palette_visible {
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

    pub(super) fn resolve_terminal_focus(
        &self,
        input: &NormalizedInput,
    ) -> Option<KeybindingMatch> {
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

        if input.named_key == Some(NamedKey::Escape) {
            return Some(KeybindingMatch {
                command: Command::FocusEditor,
                reason: "terminal focus: Esc -> FocusEditor",
            });
        }
        None
    }
}
