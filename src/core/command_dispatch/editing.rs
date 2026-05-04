use crate::{
    app::{app_state::ClipboardRecordKind, command_palette::CommandPaletteMode},
    core::{
        commands::{Command, OperationTarget, Operator},
        mode::{EditorMode, ModeEvent},
    },
};

use super::common::{DispatchCtx, DispatchReport, normalize_palette_clipboard_text};

fn matching_open_char(ch: char) -> Option<char> {
    match ch {
        '(' | '[' | '{' | '"' | '\'' | '`' => Some(ch),
        _ => None,
    }
}

fn matching_close_char(ch: char) -> Option<char> {
    match ch {
        ')' | ']' | '}' | '"' | '\'' | '`' => Some(ch),
        _ => None,
    }
}

pub(super) fn dispatch(ctx: &mut DispatchCtx<'_, '_, '_>, command: Command) -> DispatchReport {
    if let Some(report) = dispatch_terminal_normal(ctx, &command) {
        return report;
    }

    match command {
        Command::InsertChar(ch) => {
            let changed = if matching_close_char(ch).is_some() {
                ctx.app_state.step_over_closing_char(ch) || {
                    if matching_open_char(ch).is_some() {
                        ctx.app_state.insert_auto_pair(ch)
                    } else {
                        ctx.app_state.insert_char(ch);
                        true
                    }
                }
            } else if matching_open_char(ch).is_some() {
                ctx.app_state.insert_auto_pair(ch)
            } else {
                ctx.app_state.insert_char(ch);
                true
            };
            DispatchReport::success(
                format!("Dispatch: applied to active buffer (insert {ch:?})"),
                changed,
            )
        }
        Command::InsertText(text) => {
            if text.is_empty() {
                return DispatchReport::success_with_flags(
                    "Dispatch: insert text ignored (empty payload)",
                    false,
                    false,
                );
            }

            for ch in text.chars() {
                ctx.app_state.insert_char(ch);
            }

            DispatchReport::success(
                format!("Dispatch: applied to active buffer (insert text {text:?})"),
                true,
            )
        }
        Command::Newline => {
            let changed = ctx.app_state.smart_insert_newline();
            DispatchReport::success(
                "Dispatch: applied to active buffer (insert newline)",
                changed,
            )
        }
        Command::InsertTab => {
            let changed = ctx.app_state.insert_tab();
            DispatchReport::success("Dispatch: applied to active buffer (insert tab)", changed)
        }
        Command::AiAcceptInline => {
            let changed = ctx.app_state.accept_inline_suggestion();
            DispatchReport::success(
                if changed {
                    "Dispatch: applied to active buffer (accept inline suggestion)"
                } else {
                    "Dispatch: accept inline suggestion ignored (no suggestion)"
                },
                changed,
            )
        }
        Command::Backspace => {
            let changed = ctx.app_state.backspace();
            DispatchReport::success(
                if changed {
                    "Dispatch: applied to active buffer (backspace)".to_string()
                } else {
                    "Dispatch: backspace ignored (at buffer start)".to_string()
                },
                changed,
            )
        }
        Command::InsertLineBelow => match ctx.enter_insert_mode_if_needed() {
            Ok(mode_changed) => {
                let text_changed = ctx.app_state.insert_line_below();
                let changed = text_changed || mode_changed;
                DispatchReport::success(
                    "Dispatch: applied to active buffer (insert line below)",
                    changed,
                )
            }
            Err(err) => {
                DispatchReport::failure(format!("Dispatch: insert line below rejected ({err})"))
            }
        },
        Command::InsertLineAbove => match ctx.enter_insert_mode_if_needed() {
            Ok(mode_changed) => {
                let text_changed = ctx.app_state.insert_line_above();
                let changed = text_changed || mode_changed;
                DispatchReport::success(
                    "Dispatch: applied to active buffer (insert line above)",
                    changed,
                )
            }
            Err(err) => {
                DispatchReport::failure(format!("Dispatch: insert line above rejected ({err})"))
            }
        },
        Command::InsertAtLineStart => match ctx.enter_insert_mode_if_needed() {
            Ok(mode_changed) => {
                let cursor_changed = ctx.app_state.move_to_line_first_non_blank();
                let changed = cursor_changed || mode_changed;
                DispatchReport::success(
                    if changed {
                        "Dispatch: moved to first non-blank and entered insert".to_string()
                    } else {
                        "Dispatch: insert at line start ignored".to_string()
                    },
                    changed,
                )
            }
            Err(err) => {
                DispatchReport::failure(format!("Dispatch: insert at line start rejected ({err})"))
            }
        },
        Command::AppendAtLineEnd => match ctx.enter_insert_mode_if_needed() {
            Ok(mode_changed) => {
                let cursor_changed = ctx.app_state.move_to_line_end();
                let changed = cursor_changed || mode_changed;
                DispatchReport::success(
                    if changed {
                        "Dispatch: moved to line end and entered insert".to_string()
                    } else {
                        "Dispatch: append at line end ignored".to_string()
                    },
                    changed,
                )
            }
            Err(err) => {
                DispatchReport::failure(format!("Dispatch: append at line end rejected ({err})"))
            }
        },
        Command::AppendAfterCursor => match ctx.enter_insert_mode_if_needed() {
            Ok(mode_changed) => {
                let cursor_changed = ctx.app_state.append_after_cursor();
                let changed = cursor_changed || mode_changed;
                DispatchReport::success(
                    if changed {
                        "Dispatch: append after cursor and entered insert".to_string()
                    } else {
                        "Dispatch: append after cursor ignored".to_string()
                    },
                    changed,
                )
            }
            Err(err) => {
                DispatchReport::failure(format!("Dispatch: append after cursor rejected ({err})"))
            }
        },
        Command::SubstituteLine => match ctx.enter_insert_mode_if_needed() {
            Ok(mode_changed) => {
                let clipboard_text = ctx.app_state.substitute_current_line_text();
                ctx.write_text_to_clipboard_and_remember(
                    clipboard_text,
                    ClipboardRecordKind::Charwise,
                );
                let text_or_cursor_changed = ctx.app_state.substitute_current_line();
                let changed = text_or_cursor_changed || mode_changed;
                DispatchReport::success(
                    if changed {
                        "Dispatch: substituted current line and entered insert".to_string()
                    } else {
                        "Dispatch: substitute line ignored".to_string()
                    },
                    changed,
                )
            }
            Err(err) => {
                DispatchReport::failure(format!("Dispatch: substitute line rejected ({err})"))
            }
        },
        Command::DeleteChar => {
            let clipboard_text = ctx.app_state.delete_char_text_at_cursor();
            ctx.write_text_to_clipboard_and_remember(clipboard_text, ClipboardRecordKind::Charwise);
            let changed = ctx.app_state.delete_char_at_cursor();
            ctx.commit_text_transaction(changed);
            DispatchReport::success(
                if changed {
                    "Dispatch: applied to active buffer (delete char)".to_string()
                } else {
                    "Dispatch: delete char ignored".to_string()
                },
                changed,
            )
        }
        Command::DeleteSelection => {
            let clipboard_text = ctx.app_state.visual_selection_text();
            ctx.write_text_to_clipboard_and_remember(clipboard_text, ClipboardRecordKind::Charwise);

            let mut changed = ctx.app_state.delete_visual_selection();
            let mut mode_changed = false;
            if changed
                && ctx.app_state.current_mode() == EditorMode::Visual
                && let Ok(result) = ctx.app_state.apply_mode_event(ModeEvent::EnterNormal)
            {
                mode_changed = result.changed;
            }
            changed |= mode_changed;
            ctx.commit_text_transaction(changed);

            DispatchReport::success(
                if changed {
                    "Dispatch: deleted visual selection".to_string()
                } else {
                    "Dispatch: delete selection ignored".to_string()
                },
                changed,
            )
        }
        Command::DeleteCurrentLine => dispatch(
            ctx,
            Command::Operate {
                op: Operator::Delete,
                target: OperationTarget::CurrentLine,
            },
        ),
        Command::DeleteToLineEnd => dispatch(
            ctx,
            Command::Operate {
                op: Operator::Delete,
                target: OperationTarget::Motion(crate::core::commands::Motion::LineEnd),
            },
        ),
        Command::ToggleLineComment => {
            let changed = ctx.app_state.toggle_line_comment();
            ctx.commit_text_transaction(changed);
            DispatchReport::success(
                if changed {
                    "Dispatch: toggled current line comment".to_string()
                } else {
                    "Dispatch: toggle current line comment ignored".to_string()
                },
                changed,
            )
        }
        Command::DeleteWordForward => dispatch(
            ctx,
            Command::Operate {
                op: Operator::Delete,
                target: OperationTarget::Motion(crate::core::commands::Motion::WordForward),
            },
        ),
        Command::DeleteWordBackward => dispatch(
            ctx,
            Command::Operate {
                op: Operator::Delete,
                target: OperationTarget::Motion(crate::core::commands::Motion::WordBackward),
            },
        ),
        Command::YankSelection => {
            let Some(selection_text) = ctx.app_state.visual_selection_text() else {
                return DispatchReport::success_with_flags(
                    "Dispatch: yank selection ignored",
                    false,
                    false,
                );
            };

            ctx.remember_clipboard_text(&selection_text, ClipboardRecordKind::Charwise);
            if !ctx.write_text_to_clipboard(Some(selection_text)) {
                return DispatchReport::failure(
                    "Dispatch: yank selection failed (clipboard unavailable)",
                );
            }

            let mut changed = false;
            if ctx.app_state.current_mode() == EditorMode::Visual
                && let Ok(result) = ctx.app_state.apply_mode_event(ModeEvent::EnterNormal)
            {
                changed |= result.changed;
            }
            changed |= ctx.app_state.clear_visual_selection();

            DispatchReport::success("Dispatch: yanked visual selection", changed)
        }
        Command::YankCurrentLine => dispatch(
            ctx,
            Command::Operate {
                op: Operator::Yank,
                target: OperationTarget::CurrentLine,
            },
        ),
        Command::YankToWordEnd => dispatch(
            ctx,
            Command::Operate {
                op: Operator::Yank,
                target: OperationTarget::Motion(crate::core::commands::Motion::WordEnd),
            },
        ),
        Command::Undo => {
            let changed = ctx.app_state.undo();
            DispatchReport::success(
                if changed {
                    "Dispatch: undo applied".to_string()
                } else {
                    "Dispatch: undo ignored".to_string()
                },
                changed,
            )
        }
        Command::Redo => {
            let changed = ctx.app_state.redo();
            DispatchReport::success(
                if changed {
                    "Dispatch: redo applied".to_string()
                } else {
                    "Dispatch: redo ignored".to_string()
                },
                changed,
            )
        }
        Command::ChangeSelection => {
            let clipboard_text = ctx.app_state.visual_selection_text();
            ctx.write_text_to_clipboard_and_remember(clipboard_text, ClipboardRecordKind::Charwise);

            let text_changed = ctx.app_state.delete_visual_selection();
            if !text_changed {
                return DispatchReport::success_with_flags(
                    "Dispatch: change selection ignored",
                    false,
                    false,
                );
            }

            let mut changed = true;
            if ctx.app_state.current_mode() == EditorMode::Visual {
                if let Ok(result) = ctx.app_state.apply_mode_event(ModeEvent::EnterNormal) {
                    changed |= result.changed;
                }
                if let Ok(result) = ctx.app_state.apply_mode_event(ModeEvent::EnterInsert) {
                    changed |= result.changed;
                }
            } else if let Ok(mode_changed) = ctx.enter_insert_mode_if_needed() {
                changed |= mode_changed;
            }

            DispatchReport::success(
                "Dispatch: changed visual selection and entered insert",
                changed,
            )
        }
        Command::ToggleSelectionComment => {
            let text_changed = ctx.app_state.toggle_selection_comment();
            if !text_changed {
                return DispatchReport::success_with_flags(
                    "Dispatch: toggle selection comment ignored",
                    false,
                    false,
                );
            }

            let mut changed = true;
            if ctx.app_state.current_mode() == EditorMode::Visual
                && let Ok(result) = ctx.app_state.apply_mode_event(ModeEvent::EnterNormal)
            {
                changed |= result.changed;
            }
            changed |= ctx.app_state.clear_visual_selection();
            ctx.commit_text_transaction(changed);

            DispatchReport::success("Dispatch: toggled selection comment", changed)
        }
        Command::WrapSelectionWithStar => {
            let text_changed = ctx.app_state.wrap_selection_with_star();
            if !text_changed {
                return DispatchReport::success_with_flags(
                    "Dispatch: wrap selection with star ignored",
                    false,
                    false,
                );
            }

            let mut changed = true;
            if ctx.app_state.current_mode() == EditorMode::Visual
                && let Ok(result) = ctx.app_state.apply_mode_event(ModeEvent::EnterNormal)
            {
                changed |= result.changed;
            }
            changed |= ctx.app_state.clear_visual_selection();
            ctx.commit_text_transaction(changed);

            DispatchReport::success("Dispatch: wrapped selection with star", changed)
        }
        Command::PasteAfter => {
            let clipboard_text = match ctx.read_text_from_clipboard() {
                Ok(text) => text,
                Err(err) => {
                    return DispatchReport::failure(format!(
                        "Dispatch: paste after failed -> {err}"
                    ));
                }
            };

            if clipboard_text.is_empty() {
                return DispatchReport::success_with_flags(
                    "Dispatch: paste after ignored",
                    false,
                    false,
                );
            }

            let mut changed = replace_visual_selection_if_needed(ctx);
            let is_linewise = ctx.app_state.current_mode() == EditorMode::Normal
                && ctx
                    .app_state
                    .clipboard_record_kind_for_text(&clipboard_text)
                    == Some(ClipboardRecordKind::Linewise);
            let text_changed = if is_linewise {
                ctx.app_state.paste_linewise_after(&clipboard_text)
            } else {
                ctx.app_state.paste_after(&clipboard_text)
            };
            changed |= text_changed;
            ctx.commit_text_transaction(changed);

            DispatchReport::success(
                if changed {
                    "Dispatch: pasted text after cursor".to_string()
                } else {
                    "Dispatch: paste after ignored".to_string()
                },
                changed,
            )
        }
        Command::PasteBefore => {
            let clipboard_text = match ctx.read_text_from_clipboard() {
                Ok(text) => text,
                Err(err) => {
                    return DispatchReport::failure(format!(
                        "Dispatch: paste before failed -> {err}"
                    ));
                }
            };

            if clipboard_text.is_empty() {
                return DispatchReport::success_with_flags(
                    "Dispatch: paste before ignored",
                    false,
                    false,
                );
            }

            let mut changed = replace_visual_selection_if_needed(ctx);
            let is_linewise = ctx.app_state.current_mode() == EditorMode::Normal
                && ctx
                    .app_state
                    .clipboard_record_kind_for_text(&clipboard_text)
                    == Some(ClipboardRecordKind::Linewise);
            let text_changed = if is_linewise {
                ctx.app_state.paste_linewise_before(&clipboard_text)
            } else {
                ctx.app_state.paste_before(&clipboard_text)
            };
            changed |= text_changed;
            ctx.commit_text_transaction(changed);

            DispatchReport::success(
                if changed {
                    "Dispatch: pasted text before cursor".to_string()
                } else {
                    "Dispatch: paste before ignored".to_string()
                },
                changed,
            )
        }
        Command::EditorPaste | Command::PasteSystemClipboard => {
            let clipboard_text = match ctx.read_text_from_clipboard() {
                Ok(text) => text,
                Err(err) => {
                    return DispatchReport::failure(format!(
                        "Dispatch: paste system clipboard failed -> {err}"
                    ));
                }
            };

            if clipboard_text.is_empty() {
                return DispatchReport::success_with_flags(
                    "Dispatch: paste system clipboard ignored",
                    false,
                    false,
                );
            }

            if matches!(
                ctx.app_state.command_palette_mode(),
                Some(
                    CommandPaletteMode::FilePicker
                        | CommandPaletteMode::LiveGrep
                        | CommandPaletteMode::InFileSearch
                        | CommandPaletteMode::ExplorerCreateFile
                        | CommandPaletteMode::ExplorerCreateFolder
                        | CommandPaletteMode::ExplorerDeleteConfirm
                        | CommandPaletteMode::BufferCloseConfirm
                )
            ) && (ctx.app_state.current_mode() == EditorMode::PaletteFocus
                || ctx.app_state.active_buffer_is_fuzzy_picker())
            {
                let normalized = normalize_palette_clipboard_text(&clipboard_text);
                if normalized.is_empty() {
                    return DispatchReport::success_with_flags(
                        "Dispatch: paste system clipboard ignored",
                        false,
                        false,
                    );
                }

                return match ctx.app_state.command_palette_append_query(&normalized) {
                    Ok(changed) => DispatchReport::success(
                        if changed {
                            "Dispatch: pasted system clipboard into palette query".to_string()
                        } else {
                            "Dispatch: paste system clipboard ignored".to_string()
                        },
                        changed,
                    ),
                    Err(err) => DispatchReport::failure(format!(
                        "Dispatch: paste system clipboard into palette failed -> {err}"
                    )),
                };
            }

            let mut changed = replace_visual_selection_if_needed(ctx);
            changed |= ctx.app_state.insert_text_at_cursor(&clipboard_text);
            ctx.commit_text_transaction(changed);

            DispatchReport::success(
                if changed {
                    "Dispatch: pasted system clipboard".to_string()
                } else {
                    "Dispatch: paste system clipboard ignored".to_string()
                },
                changed,
            )
        }
        Command::VisualPaste => {
            let clipboard_text = match ctx.read_text_from_clipboard() {
                Ok(text) => text,
                Err(err) => {
                    return DispatchReport::failure(format!(
                        "Dispatch: visual paste failed -> {err}"
                    ));
                }
            };

            if clipboard_text.is_empty() {
                return DispatchReport::success_with_flags(
                    "Dispatch: visual paste ignored (empty clipboard)",
                    false,
                    false,
                );
            }

            // Save the selected text to clipboard (nvim register-swap behavior:
            // the replaced text becomes the new register content).
            let selected_text = ctx.app_state.visual_selection_text();
            ctx.write_text_to_clipboard_and_remember(
                selected_text,
                ClipboardRecordKind::Charwise,
            );

            // Replace selection with clipboard content and exit visual mode.
            let text_changed = ctx.app_state.replace_selection_with_text(&clipboard_text);
            let mut changed = text_changed;
            let mut mode_changed = false;
            if text_changed
                && ctx.app_state.current_mode() == EditorMode::Visual
                && let Ok(result) = ctx.app_state.apply_mode_event(ModeEvent::EnterNormal)
            {
                mode_changed = result.changed;
            }
            changed |= mode_changed;
            ctx.commit_text_transaction(changed);

            DispatchReport::success(
                if changed {
                    "Dispatch: visual paste"
                } else {
                    "Dispatch: visual paste ignored"
                },
                changed,
            )
        }
        Command::ChangeWordForward => dispatch(
            ctx,
            Command::Operate {
                op: Operator::Change,
                target: OperationTarget::Motion(crate::core::commands::Motion::WordForward),
            },
        ),
        Command::ChangeWordBackward => dispatch(
            ctx,
            Command::Operate {
                op: Operator::Change,
                target: OperationTarget::Motion(crate::core::commands::Motion::WordBackward),
            },
        ),
        Command::ChangeToLineEnd => dispatch(
            ctx,
            Command::Operate {
                op: Operator::Change,
                target: OperationTarget::Motion(crate::core::commands::Motion::LineEnd),
            },
        ),
        Command::JoinLines => {
            let changed = ctx.app_state.join_line_below();
            ctx.commit_text_transaction(changed);
            DispatchReport::success(
                if changed {
                    "Dispatch: joined line below".to_string()
                } else {
                    "Dispatch: join lines ignored".to_string()
                },
                changed,
            )
        }
        Command::ReplaceChar(ch) => {
            let changed = ctx.app_state.replace_char_at_cursor(ch);
            ctx.commit_text_transaction(changed);
            DispatchReport::success(
                if changed {
                    format!("Dispatch: replaced char at cursor with {ch:?}")
                } else {
                    "Dispatch: replace char ignored".to_string()
                },
                changed,
            )
        }
        Command::TextObjectAction { op, modifier, kind } => dispatch(
            ctx,
            Command::Operate {
                op,
                target: OperationTarget::TextObject { modifier, kind },
            },
        ),
        Command::Operate { op, target } => {
            if op == Operator::Visual {
                if let OperationTarget::TextObject { modifier, kind } = target {
                    let changed = ctx.app_state.select_text_object(modifier, kind);
                    return DispatchReport::success(
                        if changed {
                            format!("Dispatch: selected text object {:?} {:?}", modifier, kind)
                        } else {
                            "Dispatch: select text object ignored (bounds not found)".to_string()
                        },
                        changed,
                    );
                }
                return DispatchReport::success_with_flags(
                    "Dispatch: visual operator ignored for non-text-object target",
                    false,
                    false,
                );
            }

            let Some((clipboard_text, kind)) = ctx.app_state.operation_text(target, op) else {
                return DispatchReport::success_with_flags(
                    "Dispatch: operate ignored (range not found)",
                    false,
                    false,
                );
            };
            ctx.write_text_to_clipboard_and_remember(Some(clipboard_text), kind);

            if op == Operator::Yank {
                let mut changed = false;
                if ctx.app_state.current_mode() == EditorMode::Visual
                    && let Ok(result) = ctx.app_state.apply_mode_event(ModeEvent::EnterNormal)
                {
                    changed |= result.changed;
                }
                changed |= ctx.app_state.clear_visual_selection();
                return DispatchReport::success("Dispatch: yanked operation target", changed);
            }

            let changed = ctx.app_state.apply_operation(target, op);
            if !changed {
                return DispatchReport::success_with_flags(
                    "Dispatch: operation ignored (apply failed)",
                    false,
                    false,
                );
            }

            if op == Operator::Change {
                let mode_changed = ctx.enter_insert_mode_if_needed().unwrap_or(false);
                return DispatchReport::success(
                    "Dispatch: changed operation target and entered insert",
                    changed || mode_changed,
                );
            }

            if ctx.app_state.current_mode() != EditorMode::Normal {
                let _ = ctx.app_state.apply_mode_event(ModeEvent::EnterNormal);
            }
            ctx.commit_text_transaction(true);
            DispatchReport::success("Dispatch: deleted operation target", true)
        }
        _ => unreachable!("editing::dispatch received non-editing command"),
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
        Command::YankSelection => {
            let selection_text = {
                let grid = ctx.terminal_grid_mut()?;
                grid.yank_selection_text()
            };
            let Some(selection_text) = selection_text else {
                return Some(DispatchReport::success_with_flags(
                    "Dispatch: terminal yank ignored (no selection)",
                    false,
                    false,
                ));
            };

            ctx.remember_clipboard_text(&selection_text, ClipboardRecordKind::Charwise);
            if !ctx.write_text_to_clipboard(Some(selection_text)) {
                return Some(DispatchReport::failure(
                    "Dispatch: terminal yank failed (clipboard unavailable)",
                ));
            }

            let mut changed = false;
            if let Some(grid) = ctx.terminal_grid_mut() {
                changed |= grid.exit_normal_mode();
            }
            if let Ok(result) = ctx.app_state.apply_mode_event(ModeEvent::FocusTerminal) {
                changed |= result.changed;
            }

            Some(DispatchReport::success(
                "Dispatch: yanked terminal selection",
                changed,
            ))
        }
        _ => None,
    }
}

fn replace_visual_selection_if_needed(ctx: &mut DispatchCtx<'_, '_, '_>) -> bool {
    let mut changed = false;
    if ctx.app_state.current_mode() == EditorMode::Visual {
        changed |= ctx.app_state.delete_visual_selection();
        if let Ok(result) = ctx.app_state.apply_mode_event(ModeEvent::EnterNormal) {
            changed |= result.changed;
        }
    }
    changed |= ctx.app_state.clear_visual_selection();
    changed
}
