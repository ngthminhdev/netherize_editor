use crate::core::{
    commands::Command,
    mode::{EditorMode, ModeEvent},
};

use super::common::{DispatchCtx, DispatchReport};

pub(super) fn dispatch(ctx: &mut DispatchCtx<'_, '_>, command: Command) -> DispatchReport {
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
        Command::ToggleTerminal => toggle_terminal(ctx),
        Command::ToggleBottomDock
        | Command::ToggleLeftDock
        | Command::GitOpenLazygit
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
        | Command::ExplorerExpandCollapse
        | Command::ExplorerOpenFile
        | Command::NextPanelTab
        | Command::PrevPanelTab
        | Command::TerminalScrollUp
        | Command::TerminalScrollDown
        | Command::OpenFolder
        | Command::OpenRecentProjects => DispatchReport::success_with_flags(
            "Dispatch: workbench navigation (handled by event loop)",
            true,
            false,
        ),
        Command::SwitchMode(event) => match ctx.app_state.apply_mode_event(event) {
            Ok(result) => {
                let mut changed = result.changed;
                if matches!(event, ModeEvent::FocusTerminal) {
                    changed |= ctx.app_state.set_terminal_panel_open(true);
                }
                if result.to == EditorMode::Visual {
                    changed |= ctx.app_state.begin_visual_selection();
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

fn toggle_terminal(ctx: &mut DispatchCtx<'_, '_>) -> DispatchReport {
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
    if current_mode == EditorMode::TerminalFocus {
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
