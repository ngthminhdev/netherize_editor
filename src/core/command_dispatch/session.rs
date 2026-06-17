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
        Command::SaveFile => {
            if let Some(ref session) = ctx.app_state.test_field_edit {
                if Some(session.scratch_path.as_path()) == ctx.app_state.active_file() {
                    let text_str = ctx.app_state.text_string();
                    match crate::runner::validate_json_text(session.field.label(), &text_str) {
                        Ok(_) => {
                            let case_index = session.case_index;
                            if let Some(case) = ctx.app_state.test_runner.cases.get_mut(case_index)
                            {
                                match session.field {
                                    crate::runner::TestField::Input => case.input = text_str,
                                    crate::runner::TestField::Expected => case.expected = text_str,
                                }
                                case.reset_result();
                            }
                            let scratch_path = session.scratch_path.clone();
                            let return_buffer_index = session.return_buffer_index;
                            let scratch_idx_opt = ctx.app_state.buffers().iter().position(|b| {
                                matches!(&b.content, crate::app::app_state::BufferContent::Text(tb) if tb.path == scratch_path)
                            });
                            let _ = ctx.app_state.close_buffer_for_path(&scratch_path);
                            if let Some(mut r_idx) = return_buffer_index {
                                if let Some(scratch_idx) = scratch_idx_opt {
                                    if r_idx >= scratch_idx && r_idx > 0 {
                                        r_idx -= 1;
                                    }
                                }
                                if r_idx < ctx.app_state.buffers().len() {
                                    let _ = ctx.app_state.activate_buffer_index(r_idx);
                                }
                            }
                            // The solution file is active again; mirror the edited
                            // cases into the per-problem cache.
                            ctx.app_state.persist_leetcode_cases_for_active_file();
                            return DispatchReport::success_with_flags(
                                "Committed test field edit".to_string(),
                                true,
                                true,
                            );
                        }
                        Err(err) => {
                            return DispatchReport::failure(err);
                        }
                    }
                }
            }
            match ctx.app_state.save_file() {
                Ok(path) => DispatchReport::success_with_flags(
                    format!("Dispatch: save trigger succeeded -> {}", path.display()),
                    false,
                    false,
                ),
                Err(err) => {
                    DispatchReport::failure(format!("Dispatch: save trigger failed -> {err}"))
                }
            }
        }
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
        Command::BufferCloseCurrent => {
            if let Some(ref session) = ctx.app_state.test_field_edit {
                if Some(session.scratch_path.as_path()) == ctx.app_state.active_file() {
                    let scratch_path = session.scratch_path.clone();
                    let return_buffer_index = session.return_buffer_index;
                    let scratch_idx_opt = ctx.app_state.buffers().iter().position(|b| {
                        matches!(&b.content, crate::app::app_state::BufferContent::Text(tb) if tb.path == scratch_path)
                    });
                    let _ = ctx.app_state.close_buffer_for_path(&scratch_path);
                    if let Some(mut r_idx) = return_buffer_index {
                        if let Some(scratch_idx) = scratch_idx_opt {
                            if r_idx >= scratch_idx && r_idx > 0 {
                                r_idx -= 1;
                            }
                        }
                        if r_idx < ctx.app_state.buffers().len() {
                            let _ = ctx.app_state.activate_buffer_index(r_idx);
                        }
                    }
                    return DispatchReport::success_with_flags(
                        "Cancelled test field edit".to_string(),
                        true,
                        true,
                    );
                }
            }
            match ctx.app_state.close_current_buffer() {
                Ok(changed) => DispatchReport::success(
                    if changed {
                        "Dispatch: closed current buffer".to_string()
                    } else {
                        "Dispatch: close current buffer ignored".to_string()
                    },
                    changed,
                ),
                Err(err) => DispatchReport::failure(format!(
                    "Dispatch: close current buffer failed -> {err}"
                )),
            }
        }
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
        | Command::FocusOutline
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
        | Command::TerminalScrollHalfPageUp
        | Command::TerminalScrollHalfPageDown
        | Command::TerminalTabNew
        | Command::TerminalTabClose
        | Command::SwitchTerminalTab(_)
        | Command::OpenFolder
        | Command::OpenRecentProjects
        | Command::LspHover
        | Command::LspGoToDefinition
        | Command::LspPreviewDefinition
        | Command::LspReferences
        | Command::CodeGraphOpenGraphHud
        | Command::CodeGraphNavLeft
        | Command::CodeGraphNavRight
        | Command::CodeGraphNavUp
        | Command::CodeGraphNavDown
        | Command::CodeGraphJump
        | Command::CodeGraphClose
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
        | Command::AiChatClose
        | Command::AiChatUnfocus
        | Command::AiChatFocus
        | Command::ToggleMarkdownPreview
        | Command::CloseSidebars
        | Command::FocusMarkdownPreview
        | Command::MarkdownPreviewScrollUp
        | Command::MarkdownPreviewScrollDown
        | Command::MarkdownPreviewScrollTop
        | Command::MarkdownPreviewScrollBottom
        | Command::MarkdownPreviewScrollHalfPageUp
        | Command::MarkdownPreviewScrollHalfPageDown
        | Command::MarkdownPreviewScrollLeft
        | Command::MarkdownPreviewScrollRight
        | Command::SwitchBottomTab(_)
        | Command::SwitchRightTab(_)
        | Command::SwitchLeftTab(_) => DispatchReport::success_with_flags(
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
                if result.to == EditorMode::VisualBlock {
                    changed |= ctx.app_state.begin_visual_block_selection();
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
                if result.from == EditorMode::VisualBlock && result.to != EditorMode::VisualBlock {
                    changed |= ctx.app_state.clear_visual_block_selection();
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
        Command::EnterVisualLine => {
            let changed = ctx
                .terminal_grid_mut()
                .is_some_and(|grid| grid.begin_line_selection());
            Some(DispatchReport::success(
                if changed {
                    "Dispatch: began terminal visual line selection"
                } else {
                    "Dispatch: terminal visual line selection ignored"
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
