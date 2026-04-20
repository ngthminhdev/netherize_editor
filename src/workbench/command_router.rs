use winit::{
    event::{ElementState, KeyEvent},
    keyboard::{Key, KeyCode, ModifiersState, NamedKey, PhysicalKey},
};

use crate::workbench::focus_manager::FocusTarget;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorkbenchCommand {
    ToggleLeftSidebar,
    ToggleRightSidebar,
    ToggleBottomPanel,
    ToggleOverlay,
    ToggleCommandPaletteOverlay,
    CommandPaletteBackspace,
    CommandPaletteConfirm,
    FocusNext,
    FocusPrev,
    FocusCenterEditor,
    FocusBottomPanel,
    FocusLeftSidebar,
    FocusRightSidebar,
    NextTabInFocusedPanel,
    PrevTabInFocusedPanel,
    ExplorerMoveDown,
    ExplorerMoveUp,
    ExplorerCollapseOrParent,
    ExplorerExpandOrChild,
    ExplorerToggleOrOpen,
    InspectorSelectNext,
    InspectorSelectPrev,
    InspectorToggleExpand,
    TogglePausedState,
    MoveExecutionUp,
    MoveExecutionDown,
    ToggleBreakpointAtExecutionLine,
    RouteTextToFocused(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RoutedWorkbenchCommand {
    pub command: WorkbenchCommand,
    pub reason: &'static str,
}

#[derive(Debug, Default, Clone, Copy)]
pub struct WorkbenchCommandRouter;

impl WorkbenchCommandRouter {
    pub fn resolve_key_event(
        &self,
        key_event: &KeyEvent,
        modifiers: ModifiersState,
        focus: FocusTarget,
        command_palette_open: bool,
    ) -> Option<RoutedWorkbenchCommand> {
        if key_event.state != ElementState::Pressed || key_event.repeat {
            return None;
        }

        let physical = match key_event.physical_key {
            PhysicalKey::Code(code) => Some(code),
            PhysicalKey::Unidentified(_) => None,
        };

        let named = match key_event.logical_key.as_ref() {
            Key::Named(named) => Some(named),
            _ => None,
        };

        let text = match key_event.logical_key.as_ref() {
            Key::Character(text) if !text.is_empty() => Some(text.to_string()),
            _ => None,
        };

        let ctrl_or_cmd = modifiers.control_key() || modifiers.super_key();

        // Global layout commands.
        if ctrl_or_cmd {
            match physical {
                Some(KeyCode::KeyP) => {
                    return Some(RoutedWorkbenchCommand {
                        command: WorkbenchCommand::ToggleCommandPaletteOverlay,
                        reason: "global: Cmd/Ctrl+P -> ToggleCommandPaletteOverlay",
                    });
                }
                Some(KeyCode::Digit1) => {
                    return Some(RoutedWorkbenchCommand {
                        command: WorkbenchCommand::ToggleLeftSidebar,
                        reason: "global: Cmd/Ctrl+1 -> ToggleLeftSidebar",
                    });
                }
                Some(KeyCode::Digit2) => {
                    return Some(RoutedWorkbenchCommand {
                        command: WorkbenchCommand::ToggleRightSidebar,
                        reason: "global: Cmd/Ctrl+2 -> ToggleRightSidebar",
                    });
                }
                Some(KeyCode::Digit3) => {
                    return Some(RoutedWorkbenchCommand {
                        command: WorkbenchCommand::ToggleBottomPanel,
                        reason: "global: Cmd/Ctrl+3 -> ToggleBottomPanel",
                    });
                }
                Some(KeyCode::Digit4) => {
                    return Some(RoutedWorkbenchCommand {
                        command: WorkbenchCommand::ToggleOverlay,
                        reason: "global: Cmd/Ctrl+4 -> ToggleOverlay",
                    });
                }
                Some(KeyCode::Tab) => {
                    return Some(RoutedWorkbenchCommand {
                        command: if modifiers.shift_key() {
                            WorkbenchCommand::FocusPrev
                        } else {
                            WorkbenchCommand::FocusNext
                        },
                        reason: "global: Cmd/Ctrl+Tab -> cycle focus",
                    });
                }
                _ => {}
            }
        }

        match named {
            Some(NamedKey::F6) => {
                return Some(RoutedWorkbenchCommand {
                    command: if modifiers.shift_key() {
                        WorkbenchCommand::FocusPrev
                    } else {
                        WorkbenchCommand::FocusNext
                    },
                    reason: "global: F6 -> cycle focus",
                });
            }
            Some(NamedKey::Escape) => {
                if focus == FocusTarget::OverlayLayer && command_palette_open {
                    return Some(RoutedWorkbenchCommand {
                        command: WorkbenchCommand::ToggleCommandPaletteOverlay,
                        reason: "overlay: Esc -> close command palette",
                    });
                }
                if focus != FocusTarget::CenterEditor {
                    return Some(RoutedWorkbenchCommand {
                        command: WorkbenchCommand::FocusCenterEditor,
                        reason: "global: Esc -> focus center editor",
                    });
                }
            }
            Some(NamedKey::Enter) => {
                if focus == FocusTarget::OverlayLayer && command_palette_open {
                    return Some(RoutedWorkbenchCommand {
                        command: WorkbenchCommand::CommandPaletteConfirm,
                        reason: "overlay: Enter -> confirm command palette query",
                    });
                }
            }
            Some(NamedKey::F8) => {
                return Some(RoutedWorkbenchCommand {
                    command: WorkbenchCommand::FocusBottomPanel,
                    reason: "global: F8 -> focus bottom panel",
                });
            }
            Some(NamedKey::F7) => {
                return Some(RoutedWorkbenchCommand {
                    command: if modifiers.shift_key() {
                        WorkbenchCommand::PrevTabInFocusedPanel
                    } else {
                        WorkbenchCommand::NextTabInFocusedPanel
                    },
                    reason: "global: F7 -> switch tab in focused panel",
                });
            }
            Some(NamedKey::F5) => {
                return Some(RoutedWorkbenchCommand {
                    command: WorkbenchCommand::TogglePausedState,
                    reason: "global: F5 -> toggle paused state",
                });
            }
            Some(NamedKey::F9) => {
                return Some(RoutedWorkbenchCommand {
                    command: WorkbenchCommand::ToggleBreakpointAtExecutionLine,
                    reason: "global: F9 -> toggle breakpoint at execution line",
                });
            }
            _ => {}
        }

        if let Some(code) = physical {
            match code {
                KeyCode::Backspace => {
                    if focus == FocusTarget::OverlayLayer && command_palette_open {
                        return Some(RoutedWorkbenchCommand {
                            command: WorkbenchCommand::CommandPaletteBackspace,
                            reason: "overlay: Backspace -> delete query char",
                        });
                    }
                }
                KeyCode::ArrowDown => {
                    if focus == FocusTarget::LeftSidebar {
                        return Some(RoutedWorkbenchCommand {
                            command: WorkbenchCommand::ExplorerMoveDown,
                            reason: "explorer: ArrowDown -> move down",
                        });
                    }
                    if focus == FocusTarget::RightSidebar {
                        return Some(RoutedWorkbenchCommand {
                            command: WorkbenchCommand::InspectorSelectNext,
                            reason: "inspector: ArrowDown -> select next",
                        });
                    }
                    if focus == FocusTarget::CenterEditor {
                        return Some(RoutedWorkbenchCommand {
                            command: WorkbenchCommand::MoveExecutionDown,
                            reason: "editor: ArrowDown -> move execution pointer",
                        });
                    }
                }
                KeyCode::ArrowUp => {
                    if focus == FocusTarget::LeftSidebar {
                        return Some(RoutedWorkbenchCommand {
                            command: WorkbenchCommand::ExplorerMoveUp,
                            reason: "explorer: ArrowUp -> move up",
                        });
                    }
                    if focus == FocusTarget::RightSidebar {
                        return Some(RoutedWorkbenchCommand {
                            command: WorkbenchCommand::InspectorSelectPrev,
                            reason: "inspector: ArrowUp -> select prev",
                        });
                    }
                    if focus == FocusTarget::CenterEditor {
                        return Some(RoutedWorkbenchCommand {
                            command: WorkbenchCommand::MoveExecutionUp,
                            reason: "editor: ArrowUp -> move execution pointer",
                        });
                    }
                }
                KeyCode::ArrowLeft | KeyCode::ArrowRight | KeyCode::Enter | KeyCode::Space => {
                    if focus == FocusTarget::LeftSidebar {
                        let command = match code {
                            KeyCode::ArrowLeft => WorkbenchCommand::ExplorerCollapseOrParent,
                            KeyCode::ArrowRight => WorkbenchCommand::ExplorerExpandOrChild,
                            KeyCode::Enter => WorkbenchCommand::ExplorerToggleOrOpen,
                            _ => WorkbenchCommand::ExplorerExpandOrChild,
                        };
                        return Some(RoutedWorkbenchCommand {
                            command,
                            reason: "explorer: ArrowLeft/ArrowRight/Enter -> tree action",
                        });
                    }
                    if focus == FocusTarget::RightSidebar {
                        return Some(RoutedWorkbenchCommand {
                            command: WorkbenchCommand::InspectorToggleExpand,
                            reason: "inspector: toggle expand/collapse",
                        });
                    }
                }
                KeyCode::BracketLeft => {
                    return Some(RoutedWorkbenchCommand {
                        command: WorkbenchCommand::PrevTabInFocusedPanel,
                        reason: "global: [ -> previous tab in focused panel",
                    });
                }
                KeyCode::BracketRight => {
                    return Some(RoutedWorkbenchCommand {
                        command: WorkbenchCommand::NextTabInFocusedPanel,
                        reason: "global: ] -> next tab in focused panel",
                    });
                }
                KeyCode::KeyH => {
                    if focus == FocusTarget::LeftSidebar {
                        return Some(RoutedWorkbenchCommand {
                            command: WorkbenchCommand::ExplorerCollapseOrParent,
                            reason: "explorer: h -> collapse or parent",
                        });
                    }
                    if focus == FocusTarget::RightSidebar {
                        return Some(RoutedWorkbenchCommand {
                            command: WorkbenchCommand::InspectorSelectPrev,
                            reason: "inspector: h -> select prev",
                        });
                    }
                    if focus == FocusTarget::CenterEditor && modifiers.alt_key() {
                        return Some(RoutedWorkbenchCommand {
                            command: WorkbenchCommand::FocusLeftSidebar,
                            reason: "global: Alt+H -> focus left sidebar",
                        });
                    }
                }
                KeyCode::KeyL => {
                    if focus == FocusTarget::LeftSidebar {
                        return Some(RoutedWorkbenchCommand {
                            command: WorkbenchCommand::ExplorerExpandOrChild,
                            reason: "explorer: l -> expand or child",
                        });
                    }
                    if focus == FocusTarget::RightSidebar {
                        return Some(RoutedWorkbenchCommand {
                            command: WorkbenchCommand::InspectorToggleExpand,
                            reason: "inspector: l -> toggle expand",
                        });
                    }
                    if focus == FocusTarget::CenterEditor && modifiers.alt_key() {
                        return Some(RoutedWorkbenchCommand {
                            command: WorkbenchCommand::FocusRightSidebar,
                            reason: "global: Alt+L -> focus right sidebar",
                        });
                    }
                }
                KeyCode::KeyJ => {
                    if focus == FocusTarget::LeftSidebar {
                        return Some(RoutedWorkbenchCommand {
                            command: WorkbenchCommand::ExplorerMoveDown,
                            reason: "explorer: j -> move down",
                        });
                    }
                    if focus == FocusTarget::RightSidebar {
                        return Some(RoutedWorkbenchCommand {
                            command: WorkbenchCommand::InspectorSelectNext,
                            reason: "inspector: j -> select next",
                        });
                    }
                }
                KeyCode::KeyK => {
                    if focus == FocusTarget::LeftSidebar {
                        return Some(RoutedWorkbenchCommand {
                            command: WorkbenchCommand::ExplorerMoveUp,
                            reason: "explorer: k -> move up",
                        });
                    }
                    if focus == FocusTarget::RightSidebar {
                        return Some(RoutedWorkbenchCommand {
                            command: WorkbenchCommand::InspectorSelectPrev,
                            reason: "inspector: k -> select prev",
                        });
                    }
                }
                _ => {}
            }
        }

        // Route printable text to focused region for debug confirmation.
        if let Some(text) = text
            && !modifiers.control_key()
            && !modifiers.super_key()
            && !text.chars().any(char::is_control)
        {
            return Some(RoutedWorkbenchCommand {
                command: WorkbenchCommand::RouteTextToFocused(text),
                reason: "route: printable text -> focused region",
            });
        }

        None
    }
}
