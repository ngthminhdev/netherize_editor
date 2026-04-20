use std::path::PathBuf;

use crate::{
    app::app_state::AppState,
    core::{
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
        Command::OpenFileFinder => {
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

            match app_state.open_file_picker() {
                Ok(result_count) => {
                    let mode_changed = if current_mode == EditorMode::PaletteFocus {
                        false
                    } else {
                        match app_state.apply_mode_event(ModeEvent::OpenPalette) {
                            Ok(result) => result.changed,
                            Err(err) => {
                                let _ = app_state.close_file_picker();
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
            let Some(mut path) = app_state.file_picker_selected_path() else {
                return DispatchReport {
                    message: "Dispatch: file picker confirm ignored (no selection)".to_string(),
                    request_redraw: false,
                    success: true,
                    state_changed: false,
                };
            };

            // Selection có thể stale nếu file vừa bị rename/delete ngoài editor.
            // Refresh picker một lần trước khi mở để lấy path mới nhất.
            if !path.exists() {
                match app_state.refresh_file_picker_results_if_open() {
                    Ok(_) => {
                        let Some(refreshed_path) = app_state.file_picker_selected_path() else {
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

            let picker_closed = app_state.close_file_picker();
            let mut mode_changed = false;
            if app_state.current_mode() == EditorMode::PaletteFocus {
                if let Ok(result) = app_state.apply_mode_event(ModeEvent::ExitFocus) {
                    mode_changed = result.changed;
                }
            }

            open_report.message = format!(
                "Dispatch: file picker confirmed -> opened {}",
                path.display()
            );
            open_report.request_redraw = true;
            open_report.state_changed = open_report.state_changed || picker_closed || mode_changed;
            open_report
        }
        Command::CloseFilePicker => {
            let picker_closed = app_state.close_file_picker();
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
        Command::OpenCommandPalette => match app_state.apply_mode_event(ModeEvent::OpenPalette) {
            Ok(result) => DispatchReport {
                message: format!(
                    "Dispatch: command palette opened via mode transition {:?} -> {:?}",
                    result.from, result.to
                ),
                request_redraw: true,
                success: true,
                state_changed: result.changed,
            },
            Err(err) => DispatchReport {
                message: format!("Dispatch: open command palette rejected -> {:?}", err),
                request_redraw: false,
                success: false,
                state_changed: false,
            },
        },
        Command::ToggleTerminal => {
            let panel_open = app_state.is_terminal_panel_open();
            let current_mode = app_state.current_mode();

            // Case 1: panel đang tắt -> bật panel và chuyển focus vào terminal.
            if !panel_open {
                let panel_changed = app_state.set_terminal_panel_open(true);
                let result = match current_mode {
                    EditorMode::PaletteFocus => {
                        let _ = app_state.close_file_picker();
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
        // Workbench navigation commands are handled by event_loop, not AppState.
        // dispatch_command just acknowledges them so the router can act.
        Command::ToggleExplorer
        | Command::FocusEditor
        | Command::FocusExplorer
        | Command::FocusTerminal
        | Command::FocusInspector
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
        | Command::PrevPanelTab => DispatchReport {
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
        assert_eq!(app_state.current_mode(), EditorMode::Insert);

        let report = dispatch_command(&mut app_state, Command::SwitchMode(ModeEvent::EnterNormal));
        assert!(report.success);
        assert!(report.state_changed);
        assert_eq!(app_state.current_mode(), EditorMode::Normal);
    }

    #[test]
    fn invalid_switch_mode_command_is_rejected() {
        let mut app_state = AppState::new(unique_temp_path("save"));
        // Insert -> Visual là invalid theo transition table phase 7.
        let report = dispatch_command(
            &mut app_state,
            Command::SwitchMode(crate::core::mode::ModeEvent::EnterVisual),
        );
        assert!(!report.success);
        assert!(!report.state_changed);
        assert!(report.message.contains("rejected"));
        assert_eq!(app_state.current_mode(), EditorMode::Insert);
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
