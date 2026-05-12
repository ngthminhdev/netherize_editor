use crate::core::{
    commands::Command,
    mode::{EditorMode, ModeEvent},
};

use super::common::{DispatchCtx, DispatchReport};

pub(super) fn dispatch(ctx: &mut DispatchCtx<'_, '_, '_>, command: Command) -> DispatchReport {
    if let Some(report) = dispatch_terminal_normal(ctx, &command) {
        return report;
    }

    match command {
        Command::SaveFile => match ctx.app_state.save_file() {
            Ok(path) => DispatchReport::success_with_flags(
                format!("Dispatch: save trigger succeeded -> {}", path.display()),
                false,
                false,
            ),
            Err(err) => DispatchReport::failure(format!("Dispatch: save trigger failed -> {err}")),
        },
        Command::OpenFile(path) => ctx.open_file(path),
        Command::BufferNew => {
            let changed = ctx.app_state.new_empty_buffer();
            DispatchReport::success(
                if changed {
                    "Dispatch: created new empty buffer".to_string()
                } else {
                    "Dispatch: new buffer ignored".to_string()
                },
                changed,
            )
        }
        Command::BufferNext => match ctx.app_state.buffer_next() {
            Ok(changed) => DispatchReport::success(
                if changed {
                    "Dispatch: switched to next buffer".to_string()
                } else {
                    "Dispatch: next buffer ignored".to_string()
                },
                changed,
            ),
            Err(err) => DispatchReport::failure(format!("Dispatch: next buffer failed -> {err}")),
        },
        Command::BufferPrev => match ctx.app_state.buffer_prev() {
            Ok(changed) => DispatchReport::success(
                if changed {
                    "Dispatch: switched to previous buffer".to_string()
                } else {
                    "Dispatch: previous buffer ignored".to_string()
                },
                changed,
            ),
            Err(err) => {
                DispatchReport::failure(format!("Dispatch: previous buffer failed -> {err}"))
            }
        },
        Command::BufferCloseCurrent => match ctx.app_state.close_current_buffer() {
            Ok(changed) => DispatchReport::success(
                if changed {
                    "Dispatch: closed current buffer".to_string()
                } else {
                    "Dispatch: close current buffer ignored".to_string()
                },
                changed,
            ),
            Err(err) => {
                DispatchReport::failure(format!("Dispatch: close current buffer failed -> {err}"))
            }
        },
        Command::BufferGoto(index) => {
            let changed = ctx.app_state.goto_buffer_index(index);
            DispatchReport::success(
                if changed {
                    format!("Dispatch: switched to buffer {index}")
                } else {
                    format!("Dispatch: buffer {index} not found")
                },
                changed,
            )
        }
        Command::ToggleTerminal => toggle_terminal(ctx),
        Command::ToggleBottomDock
        | Command::ToggleLeftDock
        | Command::GitOpenLazygit
        | Command::GitOpenLazydocker
        | Command::GitBlameLine
        | Command::FocusEditor
        | Command::FocusExplorer
        | Command::FocusTerminal
        | Command::FocusInspector
        | Command::FocusLeft
        | Command::FocusRight
        | Command::FocusUp
        | Command::FocusDown
        | Command::MoveFocusCycle
        | Command::FocusBack
        | Command::TerminalWriteInput(_)
        | Command::TerminalPaste
        | Command::ExplorerMoveUp
        | Command::ExplorerMoveDown
        | Command::ExplorerCollapseOrParent
        | Command::ExplorerExpandNode
        | Command::ExplorerCollapseAllUnderNode
        | Command::ExplorerExpandOrChild
        | Command::ExplorerExpandAllUnderNode
        | Command::ExplorerToggleOrOpen
        | Command::ExplorerDeleteNode
        | Command::ExplorerCreateFile
        | Command::ExplorerCreateFolder
        | Command::ExplorerStartFilter
        | Command::ExplorerClearFilter
        | Command::ExplorerExpandCollapse
        | Command::ExplorerOpenFile
        | Command::NextPanelTab
        | Command::PrevPanelTab
        | Command::TerminalScrollUp
        | Command::TerminalScrollDown
        | Command::TerminalTabNew
        | Command::TerminalTabClose
        | Command::SwitchTerminalTab(_)
        | Command::OpenFolder
        | Command::OpenRecentProjects
        | Command::LspHover
        | Command::LspGoToDefinition
        | Command::LspPreviewDefinition
        | Command::LspReferences
        | Command::ReferencesSelectNext
        | Command::ReferencesSelectPrev
        | Command::ReferencesOpenSelection
        | Command::DiagnosticsOpenPicker
        | Command::DiagnosticsSelectNext
        | Command::DiagnosticsSelectPrev
        | Command::DiagnosticsOpenSelection
        | Command::JumpBack
        | Command::JumpForward
        | Command::AiChatToggle
        | Command::AiChatSend
        | Command::AiChatStop
        | Command::AiChatClose
        | Command::AiChatUnfocus
        | Command::AiChatFocus
        | Command::AiChatAddSelectionContext
        | Command::AiChatInputChar(_)
        | Command::AiChatBackspace
        | Command::AiChatClearInput
        | Command::AiChatAcceptSuggestion
        | Command::AiChatSuggestionNext
        | Command::AiChatSuggestionPrev
        | Command::AiChatInputText(_)
        | Command::AiChatPasteClipboard
        | Command::AiChatPromptInstall
        | Command::AiChatScrollHalfPageUp
        | Command::AiChatScrollHalfPageDown
        | Command::ToggleMarkdownPreview
        | Command::CloseSidebars
        | Command::FocusMarkdownPreview
        | Command::MarkdownPreviewScrollUp
        | Command::MarkdownPreviewScrollDown
        | Command::MarkdownPreviewScrollTop
        | Command::MarkdownPreviewScrollBottom
        | Command::MarkdownPreviewScrollHalfPageUp
        | Command::MarkdownPreviewScrollHalfPageDown => DispatchReport::success_with_flags(
            "Dispatch: workbench navigation (handled by event loop)",
            true,
            false,
        ),
        Command::HelpScrollDown
        | Command::HelpScrollUp
        | Command::HelpScrollHalfPageDown
        | Command::HelpScrollHalfPageUp => DispatchReport::success_with_flags(
            "Dispatch: help/cheatsheet scroll (handled by event loop)",
            true,
            false,
        ),
        Command::ToggleFold => {
            let (cursor_line, _) = ctx.app_state.cursor_line_col();
            let changed = ctx.app_state.toggle_fold_at_line(cursor_line);
            DispatchReport::success(
                if changed {
                    format!("Dispatch: toggled fold at line {cursor_line}")
                } else {
                    format!("Dispatch: no foldable scope at line {cursor_line}")
                },
                changed,
            )
        }
        Command::ToggleFoldAll => {
            let changed = ctx.app_state.toggle_fold_all();
            DispatchReport::success(
                if changed {
                    "Dispatch: toggled fold all".to_string()
                } else {
                    "Dispatch: no foldable ranges".to_string()
                },
                changed,
            )
        }
        Command::SwitchMode(event) => match ctx.app_state.apply_mode_event(event) {
            Ok(result) => {
                let mut changed = result.changed;
                if matches!(event, ModeEvent::FocusTerminal) {
                    changed |= ctx.app_state.set_terminal_panel_open(true);
                }
                if result.to == EditorMode::Visual {
                    changed |= ctx.app_state.begin_visual_selection();
                }
                if result.to == EditorMode::TerminalNormal
                    && let Some(grid) = ctx.terminal_grid_mut()
                {
                    changed |= grid.enter_normal_mode();
                }
                if result.from == EditorMode::TerminalNormal
                    && result.to != EditorMode::TerminalNormal
                    && let Some(grid) = ctx.terminal_grid_mut()
                {
                    changed |= grid.exit_normal_mode();
                }
                if result.from == EditorMode::Visual && result.to != EditorMode::Visual {
                    changed |= ctx.app_state.clear_visual_selection();
                }

                DispatchReport::success_with_flags(
                    format!(
                        "Dispatch: mode transition {:?} -> {:?} via {:?}",
                        result.from, result.to, result.event
                    ),
                    changed,
                    changed,
                )
            }
            Err(err) => {
                DispatchReport::failure(format!("Dispatch: mode transition rejected -> {:?}", err))
            }
        },
        Command::EnterVisualLine => {
            let changed = ctx
                .app_state
                .apply_mode_event(ModeEvent::EnterVisual)
                .map(|result| result.changed)
                .unwrap_or(false);
            let changed = changed | ctx.app_state.begin_visual_line_selection();
            DispatchReport::success("Dispatch: enter visual line mode", changed)
        }
        _ => unreachable!("session::dispatch received non-session command"),
    }
}

fn dispatch_terminal_normal(
    ctx: &mut DispatchCtx<'_, '_, '_>,
    command: &Command,
) -> Option<DispatchReport> {
    if !ctx.terminal_normal_active() {
        return None;
    }

    match command {
        Command::SwitchMode(ModeEvent::EnterVisual) => {
            let changed = ctx
                .terminal_grid_mut()
                .is_some_and(|grid| grid.begin_selection());
            Some(DispatchReport::success(
                if changed {
                    "Dispatch: began terminal visual selection"
                } else {
                    "Dispatch: terminal visual selection ignored"
                },
                changed,
            ))
        }
        Command::SwitchMode(ModeEvent::FocusTerminal) => {
            let mut changed = false;
            if let Some(grid) = ctx.terminal_grid_mut() {
                changed |= grid.exit_normal_mode();
            }
            if let Ok(result) = ctx.app_state.apply_mode_event(ModeEvent::FocusTerminal) {
                changed |= result.changed;
            }
            Some(DispatchReport::success(
                "Dispatch: returned terminal to typing mode",
                changed,
            ))
        }
        _ => None,
    }
}

fn toggle_terminal(ctx: &mut DispatchCtx<'_, '_, '_>) -> DispatchReport {
    let panel_open = ctx.app_state.is_terminal_panel_open();
    let current_mode = ctx.app_state.current_mode();

    if !panel_open {
        let panel_changed = ctx.app_state.set_terminal_panel_open(true);
        let result = match current_mode {
            EditorMode::PaletteFocus => {
                let _ = ctx.app_state.close_command_palette();
                ctx.app_state
                    .apply_mode_event(ModeEvent::ExitFocus)
                    .and_then(|_| ctx.app_state.apply_mode_event(ModeEvent::FocusTerminal))
            }
            EditorMode::TerminalFocus => {
                return DispatchReport::success_with_flags(
                    "Dispatch: terminal panel opened (terminal already focused)",
                    true,
                    panel_changed,
                );
            }
            _ => ctx.app_state.apply_mode_event(ModeEvent::FocusTerminal),
        };

        return match result {
            Ok(transition) => DispatchReport::success_with_flags(
                format!(
                    "Dispatch: terminal panel opened via mode transition {:?} -> {:?}",
                    transition.from, transition.to
                ),
                true,
                panel_changed || transition.changed,
            ),
            Err(err) => {
                let _ = ctx.app_state.set_terminal_panel_open(false);
                DispatchReport::failure(format!("Dispatch: terminal open rejected -> {:?}", err))
            }
        };
    }

    let mut mode_changed = false;
    if matches!(
        current_mode,
        EditorMode::TerminalFocus | EditorMode::TerminalNormal
    ) {
        match ctx.app_state.apply_mode_event(ModeEvent::ExitFocus) {
            Ok(transition) => {
                mode_changed = transition.changed;
            }
            Err(err) => {
                return DispatchReport::failure(format!(
                    "Dispatch: terminal close rejected -> {:?}",
                    err
                ));
            }
        }
    }

    let panel_changed = ctx.app_state.set_terminal_panel_open(false);
    DispatchReport::success_with_flags(
        "Dispatch: terminal panel closed",
        true,
        panel_changed || mode_changed,
    )
}
