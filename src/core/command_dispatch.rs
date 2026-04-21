use std::path::PathBuf;

use crate::{
    app::{
        app_state::AppState,
        command_palette::{CommandPaletteAction, CommandPaletteMode},
    },
    core::{
        command_ids,
        commands::Command,
        mode::{EditorMode, ModeEvent},
    },
};

#[derive(Debug, Clone)]
pub struct DispatchReport {
    pub message: String,
    pub request_redraw: bool,
    pub success: bool,
    pub state_changed: bool,
}

fn dispatch_open_file(app_state: &mut AppState, path: PathBuf) -> DispatchReport {
    match app_state.open_file(path.clone()) {
        Ok(()) => DispatchReport {
            message: format!("Dispatch: open trigger succeeded -> {}", path.display()),
            request_redraw: true,
            success: true,
            state_changed: true,
        },
        Err(err) => DispatchReport {
            message: format!("Dispatch: open trigger failed -> {err}"),
            request_redraw: false,
            success: false,
            state_changed: false,
        },
    }
}

fn enter_insert_mode_if_needed(app_state: &mut AppState) -> Result<bool, String> {
    if app_state.current_mode() == EditorMode::Insert {
        return Ok(false);
    }

    if !app_state.can_apply_mode_event(ModeEvent::EnterInsert) {
        return Err(format!(
            "mode={} does not allow EnterInsert",
            app_state.current_mode().as_str()
        ));
    }

    app_state
        .apply_mode_event(ModeEvent::EnterInsert)
        .map(|result| result.changed)
        .map_err(|err| format!("{err:?}"))
}

fn commit_text_transaction(app_state: &mut AppState, changed: bool) {
    if changed {
        let _ = app_state.commit_transaction();
    }
}

/// Dispatcher là điểm duy nhất được phép apply Command vào AppState.
/// Nhờ đó event loop chỉ làm nhiệm vụ chuyển tiếp, không tự mutate state.
pub fn dispatch_command(app_state: &mut AppState, command: Command) -> DispatchReport {
    match command {
        Command::InsertChar(ch) => {
            app_state.insert_char(ch);
            DispatchReport {
                message: format!("Dispatch: applied to active buffer (insert {ch:?})"),
                request_redraw: true,
                success: true,
                state_changed: true,
            }
        }
        Command::InsertText(text) => {
            if text.is_empty() {
                return DispatchReport {
                    message: "Dispatch: insert text ignored (empty payload)".to_string(),
                    request_redraw: false,
                    success: true,
                    state_changed: false,
                };
            }

            for ch in text.chars() {
                app_state.insert_char(ch);
            }
            DispatchReport {
                message: format!("Dispatch: applied to active buffer (insert text {text:?})"),
                request_redraw: true,
                success: true,
                state_changed: true,
            }
        }
        Command::Newline => {
            app_state.insert_char('\n');
            DispatchReport {
                message: "Dispatch: applied to active buffer (insert newline)".to_string(),
                request_redraw: true,
                success: true,
                state_changed: true,
            }
        }
        Command::Backspace => {
            let before_cursor = app_state.cursor_char_idx();
            app_state.backspace();
            let changed = app_state.cursor_char_idx() != before_cursor;
            DispatchReport {
                message: if changed {
                    "Dispatch: applied to active buffer (backspace)".to_string()
                } else {
                    "Dispatch: backspace ignored (at buffer start)".to_string()
                },
                request_redraw: changed,
                success: true,
                state_changed: changed,
            }
        }
        Command::InsertLineBelow => match enter_insert_mode_if_needed(app_state) {
            Ok(mode_changed) => {
                let text_changed = app_state.insert_line_below();
                let changed = text_changed || mode_changed;
                DispatchReport {
                    message: "Dispatch: applied to active buffer (insert line below)".to_string(),
                    request_redraw: changed,
                    success: true,
                    state_changed: changed,
                }
            }
            Err(err) => DispatchReport {
                message: format!("Dispatch: insert line below rejected ({err})"),
                request_redraw: false,
                success: false,
                state_changed: false,
            },
        },
        Command::InsertLineAbove => match enter_insert_mode_if_needed(app_state) {
            Ok(mode_changed) => {
                let text_changed = app_state.insert_line_above();
                let changed = text_changed || mode_changed;
                DispatchReport {
                    message: "Dispatch: applied to active buffer (insert line above)".to_string(),
                    request_redraw: changed,
                    success: true,
                    state_changed: changed,
                }
            }
            Err(err) => DispatchReport {
                message: format!("Dispatch: insert line above rejected ({err})"),
                request_redraw: false,
                success: false,
                state_changed: false,
            },
        },
        Command::InsertAtLineStart => match enter_insert_mode_if_needed(app_state) {
            Ok(mode_changed) => {
                let cursor_changed = app_state.move_to_line_first_non_blank();
                let changed = cursor_changed || mode_changed;
                DispatchReport {
                    message: if changed {
                        "Dispatch: moved to first non-blank and entered insert".to_string()
                    } else {
                        "Dispatch: insert at line start ignored".to_string()
                    },
                    request_redraw: changed,
                    success: true,
                    state_changed: changed,
                }
            }
            Err(err) => DispatchReport {
                message: format!("Dispatch: insert at line start rejected ({err})"),
                request_redraw: false,
                success: false,
                state_changed: false,
            },
        },
        Command::AppendAtLineEnd => match enter_insert_mode_if_needed(app_state) {
            Ok(mode_changed) => {
                let cursor_changed = app_state.move_to_line_end();
                let changed = cursor_changed || mode_changed;
                DispatchReport {
                    message: if changed {
                        "Dispatch: moved to line end and entered insert".to_string()
                    } else {
                        "Dispatch: append at line end ignored".to_string()
                    },
                    request_redraw: changed,
                    success: true,
                    state_changed: changed,
                }
            }
            Err(err) => DispatchReport {
                message: format!("Dispatch: append at line end rejected ({err})"),
                request_redraw: false,
                success: false,
                state_changed: false,
            },
        },
        Command::AppendAfterCursor => match enter_insert_mode_if_needed(app_state) {
            Ok(mode_changed) => {
                let cursor_changed = app_state.append_after_cursor();
                let changed = cursor_changed || mode_changed;
                DispatchReport {
                    message: if changed {
                        "Dispatch: append after cursor and entered insert".to_string()
                    } else {
                        "Dispatch: append after cursor ignored".to_string()
                    },
                    request_redraw: changed,
                    success: true,
                    state_changed: changed,
                }
            }
            Err(err) => DispatchReport {
                message: format!("Dispatch: append after cursor rejected ({err})"),
                request_redraw: false,
                success: false,
                state_changed: false,
            },
        },
        Command::SubstituteLine => match enter_insert_mode_if_needed(app_state) {
            Ok(mode_changed) => {
                let text_or_cursor_changed = app_state.substitute_current_line();
                let changed = text_or_cursor_changed || mode_changed;
                DispatchReport {
                    message: if changed {
                        "Dispatch: substituted current line and entered insert".to_string()
                    } else {
                        "Dispatch: substitute line ignored".to_string()
                    },
                    request_redraw: changed,
                    success: true,
                    state_changed: changed,
                }
            }
            Err(err) => DispatchReport {
                message: format!("Dispatch: substitute line rejected ({err})"),
                request_redraw: false,
                success: false,
                state_changed: false,
            },
        },
        Command::DeleteChar => {
            let changed = app_state.delete_char_at_cursor();
            commit_text_transaction(app_state, changed);
            DispatchReport {
                message: if changed {
                    "Dispatch: applied to active buffer (delete char)".to_string()
                } else {
                    "Dispatch: delete char ignored".to_string()
                },
                request_redraw: changed,
                success: true,
                state_changed: changed,
            }
        }
        Command::DeleteSelection => {
            let mut changed = app_state.delete_visual_selection();
            let mut mode_changed = false;
            if changed
                && app_state.current_mode() == EditorMode::Visual
                && let Ok(result) = app_state.apply_mode_event(ModeEvent::EnterNormal)
            {
                mode_changed = result.changed;
            }
            changed |= mode_changed;
            commit_text_transaction(app_state, changed);
            DispatchReport {
                message: if changed {
                    "Dispatch: deleted visual selection".to_string()
                } else {
                    "Dispatch: delete selection ignored".to_string()
                },
                request_redraw: changed,
                success: true,
                state_changed: changed,
            }
        }
        Command::DeleteCurrentLine => {
            let changed = app_state.delete_current_line();
            commit_text_transaction(app_state, changed);
            DispatchReport {
                message: if changed {
                    "Dispatch: applied to active buffer (delete current line)".to_string()
                } else {
                    "Dispatch: delete current line ignored".to_string()
                },
                request_redraw: changed,
                success: true,
                state_changed: changed,
            }
        }
        Command::DeleteWordForward => {
            let changed = app_state.delete_word_forward();
            commit_text_transaction(app_state, changed);
            DispatchReport {
                message: if changed {
                    "Dispatch: applied to active buffer (delete word forward)".to_string()
                } else {
                    "Dispatch: delete word forward ignored".to_string()
                },
                request_redraw: changed,
                success: true,
                state_changed: changed,
            }
        }
        Command::DeleteWordBackward => {
            let changed = app_state.delete_word_backward();
            commit_text_transaction(app_state, changed);
            DispatchReport {
                message: if changed {
                    "Dispatch: applied to active buffer (delete word backward)".to_string()
                } else {
                    "Dispatch: delete word backward ignored".to_string()
                },
                request_redraw: changed,
                success: true,
                state_changed: changed,
            }
        }
        Command::Undo => {
            let changed = app_state.undo();
            DispatchReport {
                message: if changed {
                    "Dispatch: undo applied".to_string()
                } else {
                    "Dispatch: undo ignored".to_string()
                },
                request_redraw: changed,
                success: true,
                state_changed: changed,
            }
        }
        Command::Redo => {
            let changed = app_state.redo();
            DispatchReport {
                message: if changed {
                    "Dispatch: redo applied".to_string()
                } else {
                    "Dispatch: redo ignored".to_string()
                },
                request_redraw: changed,
                success: true,
                state_changed: changed,
            }
        }
        Command::ChangeSelection => {
            let text_changed = app_state.delete_visual_selection();
            if !text_changed {
                return DispatchReport {
                    message: "Dispatch: change selection ignored".to_string(),
                    request_redraw: false,
                    success: true,
                    state_changed: false,
                };
            }

            let mut changed = true;
            if app_state.current_mode() == EditorMode::Visual {
                if let Ok(result) = app_state.apply_mode_event(ModeEvent::EnterNormal) {
                    changed |= result.changed;
                }
                if let Ok(result) = app_state.apply_mode_event(ModeEvent::EnterInsert) {
                    changed |= result.changed;
                }
            } else if let Ok(mode_changed) = enter_insert_mode_if_needed(app_state) {
                changed |= mode_changed;
            }

            DispatchReport {
                message: "Dispatch: changed visual selection and entered insert".to_string(),
                request_redraw: changed,
                success: true,
                state_changed: changed,
            }
        }
        Command::ChangeWordForward => match enter_insert_mode_if_needed(app_state) {
            Ok(mode_changed) => {
                let text_changed = app_state.change_word_forward();
                let changed = text_changed || mode_changed;
                DispatchReport {
                    message: if changed {
                        "Dispatch: changed word forward and entered insert".to_string()
                    } else {
                        "Dispatch: change word forward ignored".to_string()
                    },
                    request_redraw: changed,
                    success: true,
                    state_changed: changed,
                }
            }
            Err(err) => DispatchReport {
                message: format!("Dispatch: change word forward rejected ({err})"),
                request_redraw: false,
                success: false,
                state_changed: false,
            },
        },
        Command::ChangeWordBackward => match enter_insert_mode_if_needed(app_state) {
            Ok(mode_changed) => {
                let text_changed = app_state.change_word_backward();
                let changed = text_changed || mode_changed;
                DispatchReport {
                    message: if changed {
                        "Dispatch: changed word backward and entered insert".to_string()
                    } else {
                        "Dispatch: change word backward ignored".to_string()
                    },
                    request_redraw: changed,
                    success: true,
                    state_changed: changed,
                }
            }
            Err(err) => DispatchReport {
                message: format!("Dispatch: change word backward rejected ({err})"),
                request_redraw: false,
                success: false,
                state_changed: false,
            },
        },
        Command::ReplaceChar(ch) => {
            let changed = app_state.replace_char_at_cursor(ch);
            commit_text_transaction(app_state, changed);
            DispatchReport {
                message: if changed {
                    format!("Dispatch: replaced char at cursor with {ch:?}")
                } else {
                    "Dispatch: replace char ignored".to_string()
                },
                request_redraw: changed,
                success: true,
                state_changed: changed,
            }
        }
        Command::MoveLeft => {
            let before_cursor = app_state.cursor_char_idx();
            app_state.move_left();
            let changed = app_state.cursor_char_idx() != before_cursor;
            DispatchReport {
                message: if changed {
                    "Dispatch: applied to active buffer (move left)".to_string()
                } else {
                    "Dispatch: move left ignored (already at line/file start)".to_string()
                },
                request_redraw: changed,
                success: true,
                state_changed: changed,
            }
        }
        Command::MoveRight => {
            let before_cursor = app_state.cursor_char_idx();
            app_state.move_right();
            let changed = app_state.cursor_char_idx() != before_cursor;
            DispatchReport {
                message: if changed {
                    "Dispatch: applied to active buffer (move right)".to_string()
                } else {
                    "Dispatch: move right ignored (already at line end)".to_string()
                },
                request_redraw: changed,
                success: true,
                state_changed: changed,
            }
        }
        Command::MoveUp => {
            let before_cursor = app_state.cursor_char_idx();
            app_state.move_up();
            let changed = app_state.cursor_char_idx() != before_cursor;
            DispatchReport {
                message: if changed {
                    "Dispatch: applied to active buffer (move up)".to_string()
                } else {
                    "Dispatch: move up ignored (already at first line)".to_string()
                },
                request_redraw: changed,
                success: true,
                state_changed: changed,
            }
        }
        Command::MoveDown => {
            let before_cursor = app_state.cursor_char_idx();
            app_state.move_down();
            let changed = app_state.cursor_char_idx() != before_cursor;
            DispatchReport {
                message: if changed {
                    "Dispatch: applied to active buffer (move down)".to_string()
                } else {
                    "Dispatch: move down ignored (already at last line)".to_string()
                },
                request_redraw: changed,
                success: true,
                state_changed: changed,
            }
        }
        Command::MoveWordForward => {
            let changed = app_state.move_word_forward();
            DispatchReport {
                message: if changed {
                    "Dispatch: applied to active buffer (move word forward)".to_string()
                } else {
                    "Dispatch: move word forward ignored".to_string()
                },
                request_redraw: changed,
                success: true,
                state_changed: changed,
            }
        }
        Command::MoveWordBackward => {
            let changed = app_state.move_word_backward();
            DispatchReport {
                message: if changed {
                    "Dispatch: applied to active buffer (move word backward)".to_string()
                } else {
                    "Dispatch: move word backward ignored".to_string()
                },
                request_redraw: changed,
                success: true,
                state_changed: changed,
            }
        }
        Command::MoveWordEnd => {
            let changed = app_state.move_word_end();
            DispatchReport {
                message: if changed {
                    "Dispatch: applied to active buffer (move word end)".to_string()
                } else {
                    "Dispatch: move word end ignored".to_string()
                },
                request_redraw: changed,
                success: true,
                state_changed: changed,
            }
        }
        Command::MoveToLineStart => {
            let changed = app_state.move_to_line_start();
            DispatchReport {
                message: if changed {
                    "Dispatch: move to line start".to_string()
                } else {
                    "Dispatch: move to line start ignored".to_string()
                },
                request_redraw: changed,
                success: true,
                state_changed: changed,
            }
        }
        Command::MoveToLineEnd => {
            let changed = app_state.move_to_line_end();
            DispatchReport {
                message: if changed {
                    "Dispatch: move to line end".to_string()
                } else {
                    "Dispatch: move to line end ignored".to_string()
                },
                request_redraw: changed,
                success: true,
                state_changed: changed,
            }
        }
        Command::MoveToFirstNonWhitespace => {
            let changed = app_state.move_to_first_non_whitespace();
            DispatchReport {
                message: if changed {
                    "Dispatch: move to first non-whitespace".to_string()
                } else {
                    "Dispatch: move to first non-whitespace ignored".to_string()
                },
                request_redraw: changed,
                success: true,
                state_changed: changed,
            }
        }
        Command::MoveToFirstLine => {
            let changed = app_state.move_to_first_line();
            DispatchReport {
                message: "Dispatch: move to first line".to_string(),
                request_redraw: changed,
                success: true,
                state_changed: changed,
            }
        }
        Command::MoveToLastLine => {
            let changed = app_state.move_to_last_line();
            DispatchReport {
                message: "Dispatch: move to last line".to_string(),
                request_redraw: changed,
                success: true,
                state_changed: changed,
            }
        }
        // ScrollHalfPageUp/Down and CenterCursorLine need viewport info — handled by event_loop.
        Command::ScrollHalfPageUp | Command::ScrollHalfPageDown | Command::CenterCursorLine => {
            DispatchReport {
                message: "Dispatch: scroll (handled by event loop)".to_string(),
                request_redraw: true,
                success: true,
                state_changed: false,
            }
        }
        Command::SaveFile => match app_state.save_file() {
            Ok(path) => DispatchReport {
                message: format!("Dispatch: save trigger succeeded -> {}", path.display()),
                request_redraw: false,
                success: true,
                state_changed: false,
            },
            Err(err) => DispatchReport {
                message: format!("Dispatch: save trigger failed -> {err}"),
                request_redraw: false,
                success: false,
                state_changed: false,
            },
        },
        Command::OpenFile(path) => dispatch_open_file(app_state, path),
        Command::BufferNew => {
            let changed = app_state.new_empty_buffer();
            DispatchReport {
                message: if changed {
                    "Dispatch: created new empty buffer".to_string()
                } else {
                    "Dispatch: new buffer ignored".to_string()
                },
                request_redraw: changed,
                success: true,
                state_changed: changed,
            }
        }
        Command::BufferNext => match app_state.buffer_next() {
            Ok(changed) => DispatchReport {
                message: if changed {
                    "Dispatch: switched to next buffer".to_string()
                } else {
                    "Dispatch: next buffer ignored".to_string()
                },
                request_redraw: changed,
                success: true,
                state_changed: changed,
            },
            Err(err) => DispatchReport {
                message: format!("Dispatch: next buffer failed -> {err}"),
                request_redraw: false,
                success: false,
                state_changed: false,
            },
        },
        Command::BufferPrev => match app_state.buffer_prev() {
            Ok(changed) => DispatchReport {
                message: if changed {
                    "Dispatch: switched to previous buffer".to_string()
                } else {
                    "Dispatch: previous buffer ignored".to_string()
                },
                request_redraw: changed,
                success: true,
                state_changed: changed,
            },
            Err(err) => DispatchReport {
                message: format!("Dispatch: previous buffer failed -> {err}"),
                request_redraw: false,
                success: false,
                state_changed: false,
            },
        },
        Command::BufferCloseCurrent => match app_state.close_current_buffer() {
            Ok(changed) => DispatchReport {
                message: if changed {
                    "Dispatch: closed current buffer".to_string()
                } else {
                    "Dispatch: close current buffer ignored".to_string()
                },
                request_redraw: changed,
                success: true,
                state_changed: changed,
            },
            Err(err) => DispatchReport {
                message: format!("Dispatch: close current buffer failed -> {err}"),
                request_redraw: false,
                success: false,
                state_changed: false,
            },
        },
        Command::OpenFilePicker | Command::OpenFileFinder => {
            let was_open = app_state.is_file_picker_open();
            let current_mode = app_state.current_mode();

            if current_mode != EditorMode::PaletteFocus
                && !app_state.can_apply_mode_event(ModeEvent::OpenPalette)
            {
                return DispatchReport {
                    message: format!(
                        "Dispatch: open file finder rejected (mode={} does not allow OpenPalette)",
                        current_mode.as_str()
                    ),
                    request_redraw: false,
                    success: false,
                    state_changed: false,
                };
            }

            match app_state.open_command_palette_mode(CommandPaletteMode::FilePicker) {
                Ok(result_count) => {
                    let mode_changed = if current_mode == EditorMode::PaletteFocus {
                        false
                    } else {
                        match app_state.apply_mode_event(ModeEvent::OpenPalette) {
                            Ok(result) => result.changed,
                            Err(err) => {
                                let _ = app_state.close_command_palette();
                                return DispatchReport {
                                    message: format!(
                                        "Dispatch: open file finder rejected -> {:?}",
                                        err
                                    ),
                                    request_redraw: false,
                                    success: false,
                                    state_changed: false,
                                };
                            }
                        }
                    };

                    DispatchReport {
                        message: if was_open {
                            format!("Dispatch: file finder refreshed ({} results)", result_count)
                        } else {
                            format!("Dispatch: file finder opened ({} results)", result_count)
                        },
                        request_redraw: true,
                        success: true,
                        state_changed: mode_changed || !was_open,
                    }
                }
                Err(err) => DispatchReport {
                    message: format!("Dispatch: open file finder failed -> {err}"),
                    request_redraw: false,
                    success: false,
                    state_changed: false,
                },
            }
        }
        Command::OpenVimCommand => {
            let current_mode = app_state.current_mode();
            if current_mode != EditorMode::PaletteFocus
                && !app_state.can_apply_mode_event(ModeEvent::OpenPalette)
            {
                return DispatchReport {
                    message: format!(
                        "Dispatch: open vim command rejected (mode={} does not allow OpenPalette)",
                        current_mode.as_str()
                    ),
                    request_redraw: false,
                    success: false,
                    state_changed: false,
                };
            }

            match app_state.open_command_palette_mode(CommandPaletteMode::VimCommand) {
                Ok(result_count) => {
                    let mode_changed = if current_mode == EditorMode::PaletteFocus {
                        false
                    } else {
                        match app_state.apply_mode_event(ModeEvent::OpenPalette) {
                            Ok(result) => result.changed,
                            Err(err) => {
                                let _ = app_state.close_command_palette();
                                return DispatchReport {
                                    message: format!(
                                        "Dispatch: open vim command rejected -> {:?}",
                                        err
                                    ),
                                    request_redraw: false,
                                    success: false,
                                    state_changed: false,
                                };
                            }
                        }
                    };

                    DispatchReport {
                        message: format!("Dispatch: vim command opened ({} items)", result_count),
                        request_redraw: true,
                        success: true,
                        state_changed: mode_changed,
                    }
                }
                Err(err) => DispatchReport {
                    message: format!("Dispatch: open vim command failed -> {err}"),
                    request_redraw: false,
                    success: false,
                    state_changed: false,
                },
            }
        }
        cmd @ (Command::OpenWorkspaceSymbols | Command::SearchInFiles) => {
            let open_source = if matches!(cmd, Command::SearchInFiles) {
                "search in files"
            } else {
                "workspace symbols"
            };
            let current_mode = app_state.current_mode();
            if current_mode != EditorMode::PaletteFocus
                && !app_state.can_apply_mode_event(ModeEvent::OpenPalette)
            {
                return DispatchReport {
                    message: format!(
                        "Dispatch: open {open_source} rejected (mode={} does not allow OpenPalette)",
                        current_mode.as_str(),
                    ),
                    request_redraw: false,
                    success: false,
                    state_changed: false,
                };
            }

            match app_state.open_command_palette_mode(CommandPaletteMode::WorkspaceSymbols) {
                Ok(result_count) => {
                    let mode_changed = if current_mode == EditorMode::PaletteFocus {
                        false
                    } else {
                        match app_state.apply_mode_event(ModeEvent::OpenPalette) {
                            Ok(result) => result.changed,
                            Err(err) => {
                                let _ = app_state.close_command_palette();
                                return DispatchReport {
                                    message: format!(
                                        "Dispatch: open {open_source} rejected -> {:?}",
                                        err
                                    ),
                                    request_redraw: false,
                                    success: false,
                                    state_changed: false,
                                };
                            }
                        }
                    };

                    DispatchReport {
                        message: format!("Dispatch: {open_source} opened ({} items)", result_count),
                        request_redraw: true,
                        success: true,
                        state_changed: mode_changed,
                    }
                }
                Err(err) => DispatchReport {
                    message: format!("Dispatch: open {open_source} failed -> {err}"),
                    request_redraw: false,
                    success: false,
                    state_changed: false,
                },
            }
        }
        Command::FilePickerAppendQuery(text) => match app_state.file_picker_append_query(&text) {
            Ok(changed) => DispatchReport {
                message: if changed {
                    format!("Dispatch: file picker query append {:?}", text)
                } else {
                    "Dispatch: file picker query append ignored".to_string()
                },
                request_redraw: changed,
                success: true,
                state_changed: changed,
            },
            Err(err) => DispatchReport {
                message: format!("Dispatch: file picker query append failed -> {err}"),
                request_redraw: false,
                success: false,
                state_changed: false,
            },
        },
        Command::FilePickerBackspaceQuery => match app_state.file_picker_backspace_query() {
            Ok(changed) => DispatchReport {
                message: if changed {
                    "Dispatch: file picker query backspace".to_string()
                } else {
                    "Dispatch: file picker query backspace ignored".to_string()
                },
                request_redraw: changed,
                success: true,
                state_changed: changed,
            },
            Err(err) => DispatchReport {
                message: format!("Dispatch: file picker query backspace failed -> {err}"),
                request_redraw: false,
                success: false,
                state_changed: false,
            },
        },
        Command::FilePickerSelectNext => {
            let changed = app_state.file_picker_select_next();
            DispatchReport {
                message: if changed {
                    "Dispatch: file picker select next".to_string()
                } else {
                    "Dispatch: file picker select next ignored".to_string()
                },
                request_redraw: changed,
                success: true,
                state_changed: changed,
            }
        }
        Command::FilePickerSelectPrev => {
            let changed = app_state.file_picker_select_prev();
            DispatchReport {
                message: if changed {
                    "Dispatch: file picker select prev".to_string()
                } else {
                    "Dispatch: file picker select prev ignored".to_string()
                },
                request_redraw: changed,
                success: true,
                state_changed: changed,
            }
        }
        Command::FilePickerConfirmSelection => {
            let Some(selected_action) = app_state.command_palette_selected_action() else {
                return DispatchReport {
                    message: "Dispatch: command palette confirm ignored (no selection)".to_string(),
                    request_redraw: false,
                    success: true,
                    state_changed: false,
                };
            };

            match selected_action {
                CommandPaletteAction::OpenFile(mut path) => {
                    // Selection có thể stale nếu file vừa bị rename/delete ngoài editor.
                    // Refresh picker một lần trước khi mở để lấy path mới nhất.
                    if !path.exists() {
                        match app_state.refresh_file_picker_results_if_open() {
                            Ok(_) => {
                                let Some(refreshed_path) = app_state.file_picker_selected_path()
                                else {
                                    return DispatchReport {
                                        message:
                                            "Dispatch: file picker confirm failed -> selection is stale after external changes".to_string(),
                                        request_redraw: true,
                                        success: false,
                                        state_changed: true,
                                    };
                                };
                                path = refreshed_path;
                            }
                            Err(err) => {
                                return DispatchReport {
                                    message: format!(
                                        "Dispatch: file picker confirm failed -> refresh picker failed: {err}"
                                    ),
                                    request_redraw: false,
                                    success: false,
                                    state_changed: false,
                                };
                            }
                        }
                    }

                    let mut open_report = dispatch_open_file(app_state, path.clone());
                    if !open_report.success {
                        return open_report;
                    }

                    let picker_closed = app_state.close_command_palette();
                    let mut mode_changed = false;
                    if app_state.current_mode() == EditorMode::PaletteFocus
                        && let Ok(result) = app_state.apply_mode_event(ModeEvent::ExitFocus)
                    {
                        mode_changed = result.changed;
                    }

                    open_report.message = format!(
                        "Dispatch: file picker confirmed -> opened {}",
                        path.display()
                    );
                    open_report.request_redraw = true;
                    open_report.state_changed =
                        open_report.state_changed || picker_closed || mode_changed;
                    open_report
                }
                CommandPaletteAction::ExecuteCommand(command_id) => {
                    let Some(next) = command_ids::parse(&command_id, app_state.active_file())
                    else {
                        return DispatchReport {
                            message: format!(
                                "Dispatch: command palette confirm failed -> unknown command id '{}'",
                                command_id
                            ),
                            request_redraw: false,
                            success: false,
                            state_changed: false,
                        };
                    };
                    dispatch_command(app_state, next)
                }
                CommandPaletteAction::ExecuteVimCommand(vim) => {
                    let trimmed = vim.trim();
                    let report = if trimmed == "w" {
                        dispatch_command(app_state, Command::SaveFile)
                    } else if trimmed == "q" {
                        dispatch_command(app_state, Command::CloseFilePicker)
                    } else if trimmed == "wq" {
                        let _ = dispatch_command(app_state, Command::SaveFile);
                        dispatch_command(app_state, Command::CloseFilePicker)
                    } else if trimmed == "enew" {
                        dispatch_command(app_state, Command::BufferNew)
                    } else if trimmed == "bn" {
                        dispatch_command(app_state, Command::BufferNext)
                    } else if trimmed == "bp" {
                        dispatch_command(app_state, Command::BufferPrev)
                    } else if trimmed == "bd" {
                        dispatch_command(app_state, Command::BufferCloseCurrent)
                    } else {
                        DispatchReport {
                            message: format!("Dispatch: vim command captured -> {}", trimmed),
                            request_redraw: true,
                            success: true,
                            state_changed: false,
                        }
                    };
                    let _ = app_state.close_command_palette();
                    if app_state.current_mode() == EditorMode::PaletteFocus {
                        let _ = app_state.apply_mode_event(ModeEvent::ExitFocus);
                    }
                    report
                }
                CommandPaletteAction::JumpToSymbol(symbol) => {
                    let _ = app_state.close_command_palette();
                    if app_state.current_mode() == EditorMode::PaletteFocus {
                        let _ = app_state.apply_mode_event(ModeEvent::ExitFocus);
                    }
                    DispatchReport {
                        message: format!("Dispatch: workspace symbol selected -> {}", symbol),
                        request_redraw: true,
                        success: true,
                        state_changed: false,
                    }
                }
            }
        }
        Command::CloseFilePicker => {
            let picker_closed = app_state.close_command_palette();
            let mut mode_changed = false;
            if app_state.current_mode() == EditorMode::PaletteFocus {
                if let Ok(result) = app_state.apply_mode_event(ModeEvent::ExitFocus) {
                    mode_changed = result.changed;
                }
            }

            let changed = picker_closed || mode_changed;
            DispatchReport {
                message: if changed {
                    "Dispatch: file picker closed".to_string()
                } else {
                    "Dispatch: file picker close ignored".to_string()
                },
                request_redraw: changed,
                success: true,
                state_changed: changed,
            }
        }
        Command::OpenCommandPalette => {
            let current_mode = app_state.current_mode();
            if current_mode != EditorMode::PaletteFocus
                && !app_state.can_apply_mode_event(ModeEvent::OpenPalette)
            {
                return DispatchReport {
                    message: format!(
                        "Dispatch: open command palette rejected (mode={} does not allow OpenPalette)",
                        current_mode.as_str()
                    ),
                    request_redraw: false,
                    success: false,
                    state_changed: false,
                };
            }

            match app_state.open_command_palette_mode(CommandPaletteMode::CommandPalette) {
                Ok(result_count) => {
                    let mode_changed = if current_mode == EditorMode::PaletteFocus {
                        false
                    } else {
                        match app_state.apply_mode_event(ModeEvent::OpenPalette) {
                            Ok(result) => result.changed,
                            Err(err) => {
                                let _ = app_state.close_command_palette();
                                return DispatchReport {
                                    message: format!(
                                        "Dispatch: open command palette rejected -> {:?}",
                                        err
                                    ),
                                    request_redraw: false,
                                    success: false,
                                    state_changed: false,
                                };
                            }
                        }
                    };

                    DispatchReport {
                        message: format!(
                            "Dispatch: command palette opened ({} items)",
                            result_count
                        ),
                        request_redraw: true,
                        success: true,
                        state_changed: mode_changed,
                    }
                }
                Err(err) => DispatchReport {
                    message: format!("Dispatch: open command palette failed -> {err}"),
                    request_redraw: false,
                    success: false,
                    state_changed: false,
                },
            }
        }
        Command::ToggleTerminal => {
            let panel_open = app_state.is_terminal_panel_open();
            let current_mode = app_state.current_mode();

            // Case 1: panel đang tắt -> bật panel và chuyển focus vào terminal.
            if !panel_open {
                let panel_changed = app_state.set_terminal_panel_open(true);
                let result = match current_mode {
                    EditorMode::PaletteFocus => {
                        let _ = app_state.close_command_palette();
                        app_state
                            .apply_mode_event(ModeEvent::ExitFocus)
                            .and_then(|_| app_state.apply_mode_event(ModeEvent::FocusTerminal))
                    }
                    EditorMode::TerminalFocus => {
                        return DispatchReport {
                            message: "Dispatch: terminal panel opened (terminal already focused)"
                                .to_string(),
                            request_redraw: true,
                            success: true,
                            state_changed: panel_changed,
                        };
                    }
                    _ => app_state.apply_mode_event(ModeEvent::FocusTerminal),
                };

                return match result {
                    Ok(transition) => DispatchReport {
                        message: format!(
                            "Dispatch: terminal panel opened via mode transition {:?} -> {:?}",
                            transition.from, transition.to
                        ),
                        request_redraw: true,
                        success: true,
                        state_changed: panel_changed || transition.changed,
                    },
                    Err(err) => {
                        let _ = app_state.set_terminal_panel_open(false);
                        DispatchReport {
                            message: format!("Dispatch: terminal open rejected -> {:?}", err),
                            request_redraw: false,
                            success: false,
                            state_changed: false,
                        }
                    }
                };
            }

            // Case 2: panel đang mở và terminal đang focus -> đóng panel + thoát focus.
            if current_mode == EditorMode::TerminalFocus {
                let panel_changed = app_state.set_terminal_panel_open(false);
                return match app_state.apply_mode_event(ModeEvent::ExitFocus) {
                    Ok(transition) => DispatchReport {
                        message: format!(
                            "Dispatch: terminal panel closed via mode transition {:?} -> {:?}",
                            transition.from, transition.to
                        ),
                        request_redraw: true,
                        success: true,
                        state_changed: panel_changed || transition.changed,
                    },
                    Err(err) => {
                        let _ = app_state.set_terminal_panel_open(true);
                        DispatchReport {
                            message: format!("Dispatch: terminal close rejected -> {:?}", err),
                            request_redraw: false,
                            success: false,
                            state_changed: false,
                        }
                    }
                };
            }

            // Case 3: panel đã mở nhưng editor đang focus -> chuyển focus sang terminal.
            match app_state.apply_mode_event(ModeEvent::FocusTerminal) {
                Ok(transition) => DispatchReport {
                    message: format!(
                        "Dispatch: terminal focused via mode transition {:?} -> {:?}",
                        transition.from, transition.to
                    ),
                    request_redraw: true,
                    success: true,
                    state_changed: transition.changed,
                },
                Err(err) => DispatchReport {
                    message: format!("Dispatch: terminal focus rejected -> {:?}", err),
                    request_redraw: false,
                    success: false,
                    state_changed: false,
                },
            }
        }
        Command::SwitchMode(event) => match app_state.apply_mode_event(event) {
            Ok(result) => {
                let mut changed = result.changed;
                // Invariant: khi focus terminal, terminal panel luôn phải hiển thị.
                if matches!(event, ModeEvent::FocusTerminal) {
                    changed |= app_state.set_terminal_panel_open(true);
                }
                if result.to == EditorMode::Visual {
                    changed |= app_state.begin_visual_selection();
                }
                if result.from == EditorMode::Visual && result.to != EditorMode::Visual {
                    changed |= app_state.clear_visual_selection();
                }

                DispatchReport {
                    message: format!(
                        "Dispatch: mode transition {:?} -> {:?} via {:?}",
                        result.from, result.to, result.event
                    ),
                    request_redraw: changed,
                    success: true,
                    state_changed: changed,
                }
            }
            Err(err) => DispatchReport {
                message: format!("Dispatch: mode transition rejected -> {:?}", err),
                request_redraw: false,
                success: false,
                state_changed: false,
            },
        },
        Command::EnterVisualLine => {
            let changed = app_state
                .apply_mode_event(ModeEvent::EnterVisual)
                .map(|r| r.changed)
                .unwrap_or(false);
            let changed = changed | app_state.begin_visual_line_selection();
            DispatchReport {
                message: "Dispatch: enter visual line mode".to_string(),
                request_redraw: changed,
                success: true,
                state_changed: changed,
            }
        }
        // Workbench navigation commands are handled by event_loop, not AppState.
        // dispatch_command just acknowledges them so the router can act.
        Command::ToggleExplorer
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
        | Command::ExplorerExpandOrChild
        | Command::ExplorerToggleOrOpen
        | Command::ExplorerExpandCollapse
        | Command::ExplorerOpenFile
        | Command::NextPanelTab
        | Command::PrevPanelTab
        | Command::TerminalScrollUp
        | Command::TerminalScrollDown => DispatchReport {
            message: "Dispatch: workbench navigation (handled by event loop)".to_string(),
            request_redraw: true,
            success: true,
            state_changed: false,
        },
    }
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        time::{SystemTime, UNIX_EPOCH},
    };

    use crate::{
        app::app_state::AppState,
        core::{
            command_dispatch::dispatch_command,
            commands::Command,
            mode::{EditorMode, ModeEvent},
        },
    };

    fn unique_temp_path(suffix: &str) -> std::path::PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock drift")
            .as_nanos();
        std::env::temp_dir().join(format!("netherize_dispatch_{suffix}_{nanos}.txt"))
    }

    fn unique_temp_dir(suffix: &str) -> std::path::PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock drift")
            .as_nanos();
        std::env::temp_dir().join(format!("netherize_dispatch_dir_{suffix}_{nanos}"))
    }

    #[test]
    fn insert_command_changes_state() {
        let mut app_state = AppState::new(unique_temp_path("save"));
        let report = dispatch_command(&mut app_state, Command::InsertChar('x'));

        assert!(report.message.contains("applied to active buffer"));
        assert_eq!(app_state.preview(8), "x");
        assert!(app_state.is_dirty());
    }

    #[test]
    fn insert_text_command_supports_combining_sequence() {
        let mut app_state = AppState::new(unique_temp_path("save"));
        let report = dispatch_command(&mut app_state, Command::InsertText("ê".to_string()));

        assert!(report.success);
        assert!(report.state_changed);
        assert_eq!(app_state.text_string(), "ê");
    }

    #[test]
    fn newline_command_inserts_line_break() {
        let mut app_state = AppState::from_text(unique_temp_path("save"), "ab");
        app_state.move_right();
        app_state.move_right();
        let report = dispatch_command(&mut app_state, Command::Newline);

        assert!(report.success);
        assert!(report.state_changed);
        assert_eq!(app_state.text_string(), "ab\n");
    }

    #[test]
    fn insert_line_below_enters_insert_mode() {
        let mut app_state = AppState::from_text(unique_temp_path("save"), "abc\nxyz");
        let _ = dispatch_command(&mut app_state, Command::SwitchMode(ModeEvent::EnterNormal));
        app_state.move_right();
        app_state.move_right();

        let report = dispatch_command(&mut app_state, Command::InsertLineBelow);
        assert!(report.success);
        assert!(report.state_changed);
        assert_eq!(app_state.current_mode(), EditorMode::Insert);
        assert_eq!(app_state.text_string(), "abc\n\nxyz");
    }

    #[test]
    fn append_change_and_replace_dispatch_work() {
        let mut app_state = AppState::from_text(unique_temp_path("save"), "foo   bar");

        let append = dispatch_command(&mut app_state, Command::AppendAfterCursor);
        assert!(append.success);
        assert!(append.state_changed);
        assert_eq!(app_state.current_mode(), EditorMode::Insert);
        assert_eq!(app_state.cursor_line_col(), (0, 1));

        let _ = dispatch_command(&mut app_state, Command::SwitchMode(ModeEvent::EnterNormal));
        app_state.move_to_line_start();
        let change = dispatch_command(&mut app_state, Command::ChangeWordForward);
        assert!(change.success);
        assert!(change.state_changed);
        assert_eq!(app_state.current_mode(), EditorMode::Insert);
        assert_eq!(app_state.text_string(), "bar");

        let _ = dispatch_command(&mut app_state, Command::SwitchMode(ModeEvent::EnterNormal));
        let replace = dispatch_command(&mut app_state, Command::ReplaceChar('X'));
        assert!(replace.success);
        assert!(replace.state_changed);
        assert_eq!(app_state.current_mode(), EditorMode::Normal);
        assert_eq!(app_state.text_string(), "Xar");
    }

    #[test]
    fn visual_delete_and_change_selection_dispatch_work() {
        let mut app_state = AppState::from_text(unique_temp_path("visual_dispatch"), "abcdef");
        let _ = dispatch_command(&mut app_state, Command::SwitchMode(ModeEvent::EnterVisual));
        app_state.move_right();
        app_state.move_right();

        let delete = dispatch_command(&mut app_state, Command::DeleteSelection);
        assert!(delete.success);
        assert!(delete.state_changed);
        assert_eq!(app_state.text_string(), "def");
        assert_eq!(app_state.current_mode(), EditorMode::Normal);

        let _ = dispatch_command(&mut app_state, Command::SwitchMode(ModeEvent::EnterVisual));
        app_state.move_right();
        let change = dispatch_command(&mut app_state, Command::ChangeSelection);
        assert!(change.success);
        assert!(change.state_changed);
        assert_eq!(app_state.text_string(), "f");
        assert_eq!(app_state.current_mode(), EditorMode::Insert);
    }

    #[test]
    fn visual_word_and_line_motions_keep_selection_anchor() {
        let mut app_state = AppState::from_text(unique_temp_path("visual_motion"), "foo bar\nbaz");
        let _ = dispatch_command(&mut app_state, Command::SwitchMode(ModeEvent::EnterVisual));

        let initial = app_state
            .visual_selection_range()
            .expect("selection after entering visual");
        assert_eq!(initial.start_char, 0);
        assert_eq!(initial.end_char, 1);

        let word = dispatch_command(&mut app_state, Command::MoveWordForward);
        assert!(word.success);
        assert!(word.state_changed);
        let after_word = app_state
            .visual_selection_range()
            .expect("selection after word move");
        assert_eq!(after_word.start_char, 0);
        assert!(after_word.end_char > initial.end_char);

        let line_end = dispatch_command(&mut app_state, Command::MoveToLineEnd);
        assert!(line_end.success);
        let after_line_end = app_state
            .visual_selection_range()
            .expect("selection after line end");
        assert_eq!(after_line_end.start_char, 0);
        assert!(after_line_end.end_char >= after_word.end_char);

        let first_line = dispatch_command(&mut app_state, Command::MoveToFirstLine);
        assert!(first_line.success);
        let after_gg = app_state
            .visual_selection_range()
            .expect("selection after gg");
        assert_eq!(after_gg.start_char, 0);

        let _ = dispatch_command(&mut app_state, Command::SwitchMode(ModeEvent::EnterNormal));
        let _ = dispatch_command(&mut app_state, Command::MoveWordForward);
        let _ = dispatch_command(&mut app_state, Command::SwitchMode(ModeEvent::EnterVisual));
        let fresh = app_state
            .visual_selection_range()
            .expect("fresh selection after re-enter visual");
        assert_eq!(fresh.start_char + 1, fresh.end_char);
        assert_eq!(fresh.start_char, app_state.cursor_char_idx());
    }

    #[test]
    fn delete_current_line_removes_line_content() {
        let mut app_state = AppState::from_text(unique_temp_path("save"), "one\ntwo\nthree");
        let _ = dispatch_command(&mut app_state, Command::SwitchMode(ModeEvent::EnterNormal));
        app_state.move_down();

        let report = dispatch_command(&mut app_state, Command::DeleteCurrentLine);
        assert!(report.success);
        assert!(report.state_changed);
        assert_eq!(app_state.text_string(), "one\nthree");
    }

    #[test]
    fn delete_word_backward_removes_previous_word_span() {
        let mut app_state = AppState::from_text(unique_temp_path("save"), "foo   bar");
        let _ = dispatch_command(&mut app_state, Command::SwitchMode(ModeEvent::EnterNormal));
        let _ = dispatch_command(&mut app_state, Command::MoveToLineEnd);

        let report = dispatch_command(&mut app_state, Command::DeleteWordBackward);
        assert!(report.success);
        assert!(report.state_changed);
        assert_eq!(app_state.text_string(), "foo   ");
    }

    #[test]
    fn word_and_line_motions_dispatch_through_new_commands() {
        let mut app_state = AppState::from_text(unique_temp_path("save"), "   foo bar");
        let _ = dispatch_command(&mut app_state, Command::SwitchMode(ModeEvent::EnterNormal));

        let to_non_blank = dispatch_command(&mut app_state, Command::MoveToFirstNonWhitespace);
        assert!(to_non_blank.state_changed);
        assert_eq!(app_state.cursor_line_col(), (0, 3));

        let word_end = dispatch_command(&mut app_state, Command::MoveWordEnd);
        assert!(word_end.state_changed);
        assert_eq!(app_state.cursor_line_col(), (0, 5));

        let word_forward = dispatch_command(&mut app_state, Command::MoveWordForward);
        assert!(word_forward.state_changed);
        assert_eq!(app_state.cursor_line_col(), (0, 7));

        let word_backward = dispatch_command(&mut app_state, Command::MoveWordBackward);
        assert!(word_backward.state_changed);
        assert_eq!(app_state.cursor_line_col(), (0, 3));

        let line_start = dispatch_command(&mut app_state, Command::MoveToLineStart);
        assert!(line_start.state_changed);
        assert_eq!(app_state.cursor_line_col(), (0, 0));

        let line_end = dispatch_command(&mut app_state, Command::MoveToLineEnd);
        assert!(line_end.state_changed);
        assert_eq!(app_state.cursor_line_col(), (0, 10));
    }

    #[test]
    fn open_command_loads_content() {
        let save_path = unique_temp_path("save");
        let open_path = unique_temp_path("open");
        std::fs::write(&open_path, "open ok").expect("write");

        let mut app_state = AppState::new(save_path.clone());
        let report = dispatch_command(&mut app_state, Command::OpenFile(open_path.clone()));

        assert!(report.message.contains("open trigger succeeded"));
        assert_eq!(app_state.preview(16), "open ok");
        let canonical_open_path = open_path
            .canonicalize()
            .expect("canonical open path should exist");
        assert_eq!(
            app_state.active_file().expect("file"),
            canonical_open_path.as_path()
        );

        let _ = std::fs::remove_file(save_path);
        let _ = std::fs::remove_file(open_path);
    }

    #[test]
    fn buffer_dispatch_commands_cycle_and_close_current() {
        let mut app_state = AppState::new(unique_temp_path("buffer_dispatch"));
        let root = unique_temp_dir("buffer_dispatch");
        fs::create_dir_all(&root).expect("create buffer dispatch root");
        let file_a = root.join("a.rs");
        let file_b = root.join("b.rs");
        fs::write(&file_a, "aaa\n").expect("write a");
        fs::write(&file_b, "bbb\n").expect("write b");

        let _ = dispatch_command(&mut app_state, Command::OpenFile(file_a));
        let _ = dispatch_command(&mut app_state, Command::OpenFile(file_b));

        let prev = dispatch_command(&mut app_state, Command::BufferPrev);
        assert!(prev.success);
        assert!(prev.state_changed);
        assert!(
            app_state
                .active_file()
                .expect("active file")
                .ends_with("a.rs")
        );

        let close = dispatch_command(&mut app_state, Command::BufferCloseCurrent);
        assert!(close.success);
        assert!(close.state_changed);
        assert!(
            app_state
                .active_file()
                .expect("active file")
                .ends_with("b.rs")
        );

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn ignored_move_right_does_not_request_redraw() {
        let mut app_state = AppState::from_text(unique_temp_path("save"), "abc");
        // Đưa cursor tới EOF.
        app_state.move_right();
        app_state.move_right();
        app_state.move_right();
        assert_eq!(app_state.cursor_line_col(), (0, 3));

        let report = dispatch_command(&mut app_state, Command::MoveRight);
        assert!(!report.request_redraw);
        assert!(!report.state_changed);
        assert!(report.message.contains("ignored"));
    }

    #[test]
    fn switch_mode_command_changes_mode_state() {
        let mut app_state = AppState::new(unique_temp_path("save"));
        assert_eq!(app_state.current_mode(), EditorMode::Normal);

        let report = dispatch_command(&mut app_state, Command::SwitchMode(ModeEvent::EnterInsert));
        assert!(report.success);
        assert!(report.state_changed);
        assert_eq!(app_state.current_mode(), EditorMode::Insert);
    }

    #[test]
    fn invalid_switch_mode_command_is_rejected() {
        let mut app_state = AppState::new(unique_temp_path("save"));
        // Normal -> ExitFocus là invalid theo transition table phase 7.
        let report = dispatch_command(
            &mut app_state,
            Command::SwitchMode(crate::core::mode::ModeEvent::ExitFocus),
        );
        assert!(!report.success);
        assert!(!report.state_changed);
        assert!(report.message.contains("rejected"));
        assert_eq!(app_state.current_mode(), EditorMode::Normal);
    }

    #[test]
    fn open_command_palette_command_enters_palette_focus_mode() {
        let mut app_state = AppState::new(unique_temp_path("save"));
        let _ = dispatch_command(
            &mut app_state,
            Command::SwitchMode(crate::core::mode::ModeEvent::EnterNormal),
        );

        let report = dispatch_command(&mut app_state, Command::OpenCommandPalette);
        assert!(report.success);
        assert_eq!(app_state.current_mode(), EditorMode::PaletteFocus);
    }

    #[test]
    fn open_file_finder_command_enters_palette_focus_mode() {
        let mut app_state = AppState::new(unique_temp_path("save"));
        let workspace_root = unique_temp_dir("workspace");
        fs::create_dir_all(workspace_root.join("src")).expect("create workspace");
        fs::write(workspace_root.join("src/main.rs"), "fn main() {}\n").expect("write source");
        app_state
            .attach_workspace(workspace_root.clone())
            .expect("attach workspace");
        let _ = dispatch_command(
            &mut app_state,
            Command::SwitchMode(crate::core::mode::ModeEvent::EnterNormal),
        );

        let report = dispatch_command(&mut app_state, Command::OpenFileFinder);
        assert!(report.success);
        assert_eq!(app_state.current_mode(), EditorMode::PaletteFocus);
        assert!(app_state.is_file_picker_open());

        let _ = fs::remove_dir_all(workspace_root);
    }

    #[test]
    fn file_picker_confirm_selection_reuses_open_flow() {
        let mut app_state = AppState::new(unique_temp_path("save"));
        let workspace_root = unique_temp_dir("workspace_confirm");
        fs::create_dir_all(workspace_root.join("src")).expect("create workspace");
        fs::write(
            workspace_root.join("src/netherize_phase8.rs"),
            "pub fn hello() {}\n",
        )
        .expect("write source");
        app_state
            .attach_workspace(workspace_root.clone())
            .expect("attach workspace");

        let _ = dispatch_command(
            &mut app_state,
            Command::SwitchMode(crate::core::mode::ModeEvent::EnterNormal),
        );
        let _ = dispatch_command(&mut app_state, Command::OpenFileFinder);
        let _ = dispatch_command(
            &mut app_state,
            Command::FilePickerAppendQuery("phase8".to_string()),
        );

        let report = dispatch_command(&mut app_state, Command::FilePickerConfirmSelection);
        assert!(report.success);
        assert!(report.state_changed);
        assert_eq!(app_state.current_mode(), EditorMode::Normal);
        assert!(!app_state.is_file_picker_open());
        assert!(
            app_state
                .active_file()
                .expect("active file")
                .to_string_lossy()
                .contains("netherize_phase8.rs")
        );

        let _ = fs::remove_dir_all(workspace_root);
    }

    #[test]
    fn file_picker_confirm_refreshes_stale_selection_before_opening() {
        let mut app_state = AppState::new(unique_temp_path("save"));
        let workspace_root = unique_temp_dir("workspace_confirm_stale");
        fs::create_dir_all(workspace_root.join("src")).expect("create workspace");
        let old_path = workspace_root.join("src/phase1-hello.txt");
        let new_path = workspace_root.join("src/phase1-renamed.txt");
        fs::write(&old_path, "hello\n").expect("write old source");
        app_state
            .attach_workspace(workspace_root.clone())
            .expect("attach workspace");

        let _ = dispatch_command(
            &mut app_state,
            Command::SwitchMode(crate::core::mode::ModeEvent::EnterNormal),
        );
        let _ = dispatch_command(&mut app_state, Command::OpenFileFinder);
        let _ = dispatch_command(
            &mut app_state,
            Command::FilePickerAppendQuery("phase1-hello".to_string()),
        );

        fs::rename(&old_path, &new_path).expect("rename source");
        let report = dispatch_command(&mut app_state, Command::FilePickerConfirmSelection);

        assert!(!report.success);
        assert!(
            report.message.contains("selection is stale")
                || report.message.contains("open trigger failed"),
            "unexpected message: {}",
            report.message
        );
        assert!(app_state.is_file_picker_open());
        assert_eq!(app_state.current_mode(), EditorMode::PaletteFocus);

        let _ = fs::remove_dir_all(workspace_root);
    }

    #[test]
    fn undo_redo_groups_insert_session_until_escape() {
        let mut app_state = AppState::new(unique_temp_path("undo_insert"));

        let enter_insert = dispatch_command(
            &mut app_state,
            Command::SwitchMode(crate::core::mode::ModeEvent::EnterInsert),
        );
        assert!(enter_insert.success);
        assert_eq!(app_state.current_mode(), EditorMode::Insert);

        let _ = dispatch_command(&mut app_state, Command::InsertChar('a'));
        let _ = dispatch_command(&mut app_state, Command::InsertChar('b'));
        assert_eq!(app_state.text_string(), "ab");

        let exit_insert = dispatch_command(
            &mut app_state,
            Command::SwitchMode(crate::core::mode::ModeEvent::EnterNormal),
        );
        assert!(exit_insert.success);
        assert_eq!(app_state.current_mode(), EditorMode::Normal);

        let undo = dispatch_command(&mut app_state, Command::Undo);
        assert!(undo.success);
        assert!(undo.state_changed);
        assert_eq!(app_state.text_string(), "");

        let redo = dispatch_command(&mut app_state, Command::Redo);
        assert!(redo.success);
        assert!(redo.state_changed);
        assert_eq!(app_state.text_string(), "ab");
    }

    #[test]
    fn undo_redo_groups_change_word_with_following_insert() {
        let mut app_state = AppState::from_text(unique_temp_path("undo_change"), "foo");

        let change = dispatch_command(&mut app_state, Command::ChangeWordForward);
        assert!(change.success);
        assert_eq!(app_state.current_mode(), EditorMode::Insert);
        assert_eq!(app_state.text_string(), "");

        let _ = dispatch_command(&mut app_state, Command::InsertText("bar".to_string()));
        assert_eq!(app_state.text_string(), "bar");

        let exit_insert = dispatch_command(
            &mut app_state,
            Command::SwitchMode(crate::core::mode::ModeEvent::EnterNormal),
        );
        assert!(exit_insert.success);
        assert_eq!(app_state.current_mode(), EditorMode::Normal);

        let undo = dispatch_command(&mut app_state, Command::Undo);
        assert!(undo.success);
        assert_eq!(app_state.text_string(), "foo");

        let redo = dispatch_command(&mut app_state, Command::Redo);
        assert!(redo.success);
        assert_eq!(app_state.text_string(), "bar");
    }

    #[test]
    fn redo_stack_is_cleared_after_new_committed_edit() {
        let mut app_state = AppState::new(unique_temp_path("redo_clear"));

        let _ = dispatch_command(
            &mut app_state,
            Command::SwitchMode(crate::core::mode::ModeEvent::EnterInsert),
        );
        let _ = dispatch_command(&mut app_state, Command::InsertText("ab".to_string()));
        let _ = dispatch_command(
            &mut app_state,
            Command::SwitchMode(crate::core::mode::ModeEvent::EnterNormal),
        );
        let _ = dispatch_command(&mut app_state, Command::Undo);
        assert_eq!(app_state.text_string(), "");

        let _ = dispatch_command(
            &mut app_state,
            Command::SwitchMode(crate::core::mode::ModeEvent::EnterInsert),
        );
        let _ = dispatch_command(&mut app_state, Command::InsertChar('z'));
        let _ = dispatch_command(
            &mut app_state,
            Command::SwitchMode(crate::core::mode::ModeEvent::EnterNormal),
        );
        assert_eq!(app_state.text_string(), "z");

        let redo = dispatch_command(&mut app_state, Command::Redo);
        assert!(redo.success);
        assert!(!redo.state_changed);
        assert_eq!(app_state.text_string(), "z");
    }

    #[test]
    fn toggle_terminal_command_enters_and_exits_terminal_focus() {
        let mut app_state = AppState::new(unique_temp_path("save"));
        let _ = dispatch_command(
            &mut app_state,
            Command::SwitchMode(crate::core::mode::ModeEvent::EnterNormal),
        );

        let enter = dispatch_command(&mut app_state, Command::ToggleTerminal);
        assert!(enter.success);
        assert_eq!(app_state.current_mode(), EditorMode::TerminalFocus);
        assert!(app_state.is_terminal_panel_open());

        let exit = dispatch_command(&mut app_state, Command::ToggleTerminal);
        assert!(exit.success);
        assert_eq!(app_state.current_mode(), EditorMode::Normal);
        assert!(!app_state.is_terminal_panel_open());
    }

    #[test]
    fn toggle_terminal_focuses_existing_panel_without_closing_it() {
        let mut app_state = AppState::new(unique_temp_path("save"));
        let _ = dispatch_command(
            &mut app_state,
            Command::SwitchMode(crate::core::mode::ModeEvent::EnterNormal),
        );

        let _ = dispatch_command(&mut app_state, Command::ToggleTerminal); // open + focus terminal
        let _ = dispatch_command(
            &mut app_state,
            Command::SwitchMode(crate::core::mode::ModeEvent::ExitFocus),
        ); // quay lại editor, panel vẫn mở
        assert_eq!(app_state.current_mode(), EditorMode::Normal);
        assert!(app_state.is_terminal_panel_open());

        let report = dispatch_command(&mut app_state, Command::ToggleTerminal);
        assert!(report.success);
        assert_eq!(app_state.current_mode(), EditorMode::TerminalFocus);
        assert!(app_state.is_terminal_panel_open());
    }
}
