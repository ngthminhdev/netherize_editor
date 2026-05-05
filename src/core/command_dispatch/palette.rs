use crate::{
    app::command_palette::{CommandPaletteAction, CommandPaletteMode},
    config::theme_config::ThemeConfig,
    core::{
        command_ids,
        commands::Command,
        mode::{EditorMode, ModeEvent},
    },
};

use super::{DispatchCtx, common::DispatchReport, dispatch_command};

pub(super) fn dispatch(ctx: &mut DispatchCtx<'_, '_, '_>, command: Command) -> DispatchReport {
    match command {
        Command::OpenFilePicker => {
            match open_palette_mode(ctx, CommandPaletteMode::FilePicker, "file picker") {
                Ok((result_count, mode_changed)) => DispatchReport::success_with_flags(
                    format!("Dispatch: file picker opened ({result_count} items)"),
                    true,
                    mode_changed,
                ),
                Err(report) => report,
            }
        }
        Command::OpenFileFinder => {
            let _ = ctx.app_state.apply_mode_event(ModeEvent::EnterInsert);
            let buffer_index = ctx
                .app_state
                .open_fuzzy_picker_buffer(CommandPaletteMode::FilePicker);
            DispatchReport::success_with_flags(
                format!(
                    "Dispatch: file picker buffer opened at index {}",
                    buffer_index
                ),
                true,
                true,
            )
        }
        Command::OpenVimCommand => {
            match open_palette_mode(ctx, CommandPaletteMode::VimCommand, "vim command") {
                Ok((result_count, mode_changed)) => DispatchReport::success_with_flags(
                    format!("Dispatch: vim command opened ({result_count} items)"),
                    true,
                    mode_changed,
                ),
                Err(report) => report,
            }
        }
        Command::OpenInFileSearch => {
            match open_palette_mode(ctx, CommandPaletteMode::InFileSearch, "in-file search") {
                Ok((result_count, mode_changed)) => DispatchReport::success_with_flags(
                    format!("Dispatch: in-file search opened ({result_count} items)"),
                    true,
                    mode_changed,
                ),
                Err(report) => report,
            }
        }
        Command::TerminalSearchOpen => {
            match open_palette_mode(ctx, CommandPaletteMode::InFileSearch, "terminal search") {
                Ok((result_count, mode_changed)) => DispatchReport::success_with_flags(
                    format!("Dispatch: terminal search opened ({result_count} items)"),
                    true,
                    mode_changed,
                ),
                Err(report) => report,
            }
        }
        Command::OpenWorkspaceSymbols => match open_palette_mode(
            ctx,
            CommandPaletteMode::WorkspaceSymbols,
            "workspace symbols",
        ) {
            Ok((result_count, mode_changed)) => DispatchReport::success_with_flags(
                format!("Dispatch: workspace symbols opened ({result_count} items)"),
                true,
                mode_changed,
            ),
            Err(report) => report,
        },
        Command::OpenDocumentSymbols => match open_document_symbols(ctx) {
            Ok((result_count, mode_changed)) => DispatchReport::success_with_flags(
                format!("Dispatch: document symbols opened ({result_count} items)"),
                true,
                mode_changed,
            ),
            Err(report) => report,
        },
        Command::SearchInFiles => {
            let _ = ctx.app_state.apply_mode_event(ModeEvent::EnterInsert);
            let buffer_index = ctx
                .app_state
                .open_fuzzy_picker_buffer(CommandPaletteMode::LiveGrep);
            DispatchReport::success_with_flags(
                format!(
                    "Dispatch: live grep buffer opened at index {}",
                    buffer_index
                ),
                true,
                true,
            )
        }
        Command::OpenThemeSelector => match open_theme_selector(ctx) {
            Ok((result_count, mode_changed)) => DispatchReport::success_with_flags(
                format!("Dispatch: theme selector opened ({result_count} items)"),
                true,
                mode_changed,
            ),
            Err(report) => report,
        },
        Command::OpenHelp => {
            let _ = ctx.app_state.close_command_palette();
            if ctx.app_state.current_mode() == EditorMode::PaletteFocus {
                let _ = ctx.app_state.apply_mode_event(ModeEvent::ExitFocus);
            }
            let index = ctx.app_state.open_help_buffer();
            DispatchReport::success_with_flags(
                format!("Dispatch: help buffer opened at index {index}"),
                true,
                true,
            )
        }
        Command::FilePickerAppendQuery(text) => match ctx.app_state.file_picker_append_query(&text)
        {
            Ok(changed) => DispatchReport::success(
                if changed {
                    format!("Dispatch: file picker query append {:?}", text)
                } else {
                    "Dispatch: file picker query append ignored".to_string()
                },
                changed,
            ),
            Err(err) => DispatchReport::failure(format!(
                "Dispatch: file picker query append failed -> {err}"
            )),
        },
        Command::FilePickerBackspaceQuery => match ctx.app_state.file_picker_backspace_query() {
            Ok(changed) => DispatchReport::success(
                if changed {
                    "Dispatch: file picker query backspace".to_string()
                } else {
                    "Dispatch: file picker query backspace ignored".to_string()
                },
                changed,
            ),
            Err(err) => DispatchReport::failure(format!(
                "Dispatch: file picker query backspace failed -> {err}"
            )),
        },
        Command::OverlaySelectNext | Command::FilePickerSelectNext => {
            let changed = ctx.app_state.command_palette_select_next();
            DispatchReport::success(
                if changed {
                    "Dispatch: overlay select next".to_string()
                } else {
                    "Dispatch: overlay select next ignored".to_string()
                },
                changed,
            )
        }
        Command::OverlaySelectPrev | Command::FilePickerSelectPrev => {
            let changed = ctx.app_state.command_palette_select_prev();
            DispatchReport::success(
                if changed {
                    "Dispatch: overlay select prev".to_string()
                } else {
                    "Dispatch: overlay select prev ignored".to_string()
                },
                changed,
            )
        }
        Command::FilePickerConfirmSelection => confirm_selection(ctx),
        Command::CloseFilePicker => close_picker(ctx),
        Command::OpenCommandPalette => {
            match open_palette_mode(ctx, CommandPaletteMode::CommandPalette, "command palette") {
                Ok((result_count, mode_changed)) => DispatchReport::success_with_flags(
                    format!("Dispatch: command palette opened ({result_count} items)"),
                    true,
                    mode_changed,
                ),
                Err(report) => report,
            }
        }
        Command::OpenFileHistory => {
            let _ = ctx.app_state.apply_mode_event(ModeEvent::EnterInsert);
            let _ = ctx.app_state.begin_file_history_preview_session();
            let buffer_index = ctx
                .app_state
                .open_fuzzy_picker_buffer(CommandPaletteMode::FileHistory);
            let items = ctx.app_state.file_history_picker_items();
            let _ = ctx.app_state.set_command_palette_results(
                CommandPaletteMode::FileHistory,
                "",
                items,
            );
            DispatchReport::success_with_flags(
                format!("Dispatch: file history buffer opened at index {buffer_index}"),
                true,
                true,
            )
        }
        _ => unreachable!("palette::dispatch received non-palette command"),
    }
}

fn open_palette_mode(
    ctx: &mut DispatchCtx<'_, '_, '_>,
    mode: CommandPaletteMode,
    label: &str,
) -> Result<(usize, bool), DispatchReport> {
    let current_mode = ctx.app_state.current_mode();
    if current_mode != EditorMode::PaletteFocus
        && !ctx.app_state.can_apply_mode_event(ModeEvent::OpenPalette)
    {
        return Err(DispatchReport::failure(format!(
            "Dispatch: open {label} rejected (mode={} does not allow OpenPalette)",
            current_mode.as_str()
        )));
    }

    let result_count = match ctx.app_state.open_command_palette_mode(mode) {
        Ok(result_count) => result_count,
        Err(err) => {
            return Err(DispatchReport::failure(format!(
                "Dispatch: open {label} failed -> {err}"
            )));
        }
    };

    let mode_changed = if current_mode == EditorMode::PaletteFocus {
        false
    } else {
        match ctx.app_state.apply_mode_event(ModeEvent::OpenPalette) {
            Ok(result) => result.changed,
            Err(err) => {
                let _ = ctx.app_state.close_command_palette();
                return Err(DispatchReport::failure(format!(
                    "Dispatch: open {label} rejected -> {:?}",
                    err
                )));
            }
        }
    };

    Ok((result_count, mode_changed))
}

fn open_theme_selector(ctx: &mut DispatchCtx<'_, '_, '_>) -> Result<(usize, bool), DispatchReport> {
    let current_mode = ctx.app_state.current_mode();
    if current_mode != EditorMode::PaletteFocus
        && !ctx.app_state.can_apply_mode_event(ModeEvent::OpenPalette)
    {
        return Err(DispatchReport::failure(format!(
            "Dispatch: open theme selector rejected (mode={} does not allow OpenPalette)",
            current_mode.as_str()
        )));
    }

    let themes = ThemeConfig::list_available_theme_entries();
    if themes.is_empty() {
        return Err(DispatchReport::failure(
            "Dispatch: open theme selector failed -> no theme profiles found",
        ));
    }

    let result_count = match ctx.app_state.open_theme_selector_palette(&themes) {
        Ok(result_count) => result_count,
        Err(err) => {
            return Err(DispatchReport::failure(format!(
                "Dispatch: open theme selector failed -> {err}"
            )));
        }
    };

    let mode_changed = if current_mode == EditorMode::PaletteFocus {
        false
    } else {
        match ctx.app_state.apply_mode_event(ModeEvent::OpenPalette) {
            Ok(result) => result.changed,
            Err(err) => {
                let _ = ctx.app_state.close_command_palette();
                return Err(DispatchReport::failure(format!(
                    "Dispatch: open theme selector rejected -> {:?}",
                    err
                )));
            }
        }
    };

    Ok((result_count, mode_changed))
}

fn open_document_symbols(
    ctx: &mut DispatchCtx<'_, '_, '_>,
) -> Result<(usize, bool), DispatchReport> {
    let current_mode = ctx.app_state.current_mode();
    if current_mode != EditorMode::PaletteFocus
        && !ctx.app_state.can_apply_mode_event(ModeEvent::OpenPalette)
    {
        return Err(DispatchReport::failure(format!(
            "Dispatch: open document symbols rejected (mode={} does not allow OpenPalette)",
            current_mode.as_str()
        )));
    }

    let result_count = match ctx.app_state.open_document_symbols_palette_loading() {
        Ok(result_count) => result_count,
        Err(err) => {
            return Err(DispatchReport::failure(format!(
                "Dispatch: open document symbols failed -> {err}"
            )));
        }
    };

    let mode_changed = if current_mode == EditorMode::PaletteFocus {
        false
    } else {
        match ctx.app_state.apply_mode_event(ModeEvent::OpenPalette) {
            Ok(result) => result.changed,
            Err(err) => {
                let _ = ctx.app_state.close_command_palette();
                return Err(DispatchReport::failure(format!(
                    "Dispatch: open document symbols rejected -> {:?}",
                    err
                )));
            }
        }
    };

    Ok((result_count, mode_changed))
}

fn confirm_selection(ctx: &mut DispatchCtx<'_, '_, '_>) -> DispatchReport {
    if matches!(
        ctx.app_state.command_palette_mode(),
        Some(CommandPaletteMode::InFileSearch)
    ) {
        let moved = ctx.app_state.search_next();
        let changed = moved || ctx.close_palette_and_exit_focus();
        return DispatchReport::success_with_flags(
            if moved {
                "Dispatch: in-file search confirmed".to_string()
            } else {
                "Dispatch: in-file search confirmed (no further matches)".to_string()
            },
            true,
            changed,
        );
    }

    let Some(selected_action) = ctx.app_state.command_palette_selected_action() else {
        return DispatchReport::success_with_flags(
            "Dispatch: command palette confirm ignored (no selection)",
            false,
            false,
        );
    };

    match selected_action {
        CommandPaletteAction::OpenFile(mut path) => {
            if !path.exists() {
                match ctx.app_state.refresh_file_picker_results_if_open() {
                    Ok(_) => {
                        let Some(refreshed_path) = ctx.app_state.file_picker_selected_path() else {
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
                        return DispatchReport::failure(format!(
                            "Dispatch: file picker confirm failed -> refresh picker failed: {err}"
                        ));
                    }
                }
            }

            let mut open_report = ctx.open_file(path.clone());
            if !open_report.success {
                return open_report;
            }

            let changed = ctx.close_palette_and_exit_focus();
            open_report.message = format!(
                "Dispatch: file picker confirmed -> opened {}",
                path.display()
            );
            open_report.request_redraw = true;
            open_report.state_changed |= changed;
            open_report
        }
        CommandPaletteAction::OpenSearchMatch { path, line, column } => {
            let changed = ctx.close_palette_and_exit_focus();
            let mut open_report = ctx.open_file(path.clone());
            if !open_report.success {
                open_report.state_changed |= changed;
                return open_report;
            }

            let jumped = ctx.app_state.jump_to_line_and_column(
                line.saturating_sub(1) as usize,
                column.saturating_sub(1) as usize,
            );
            open_report.message = format!(
                "Dispatch: live grep confirmed -> opened {}:{}:{}",
                path.display(),
                line,
                column
            );
            open_report.request_redraw = true;
            open_report.state_changed |= jumped || changed;
            open_report
        }
        CommandPaletteAction::ExecuteCommand(command_id) => {
            let Some(next) = command_ids::parse(&command_id, ctx.app_state.active_file()) else {
                return DispatchReport::failure(format!(
                    "Dispatch: command palette confirm failed -> unknown command id '{}'",
                    command_id
                ));
            };
            dispatch_command(ctx.app_state, next)
        }
        CommandPaletteAction::ExecuteVimCommand(vim) => {
            let trimmed = vim.trim();
            let command_text = trimmed.strip_prefix(':').unwrap_or(trimmed).trim();
            let report = if command_text == "w" {
                dispatch_command(ctx.app_state, Command::SaveFile)
            } else if command_text == "q" {
                dispatch_command(ctx.app_state, Command::CloseFilePicker)
            } else if command_text == "wq" {
                let _ = dispatch_command(ctx.app_state, Command::SaveFile);
                dispatch_command(ctx.app_state, Command::CloseFilePicker)
            } else if command_text == "enew" {
                dispatch_command(ctx.app_state, Command::BufferNew)
            } else if command_text == "bn" {
                dispatch_command(ctx.app_state, Command::BufferNext)
            } else if command_text == "bp" {
                dispatch_command(ctx.app_state, Command::BufferPrev)
            } else if command_text == "bd" {
                dispatch_command(ctx.app_state, Command::BufferCloseCurrent)
            } else if command_text == "h" || command_text == "help" {
                dispatch_command(ctx.app_state, Command::OpenHelp)
            } else if let Ok(line_number) = command_text.parse::<usize>() {
                let target_line = line_number
                    .saturating_sub(1)
                    .min(ctx.app_state.total_lines());
                let changed = ctx.app_state.jump_to_line(target_line);
                DispatchReport::success_with_flags(
                    format!("Dispatch: jumped to line {line_number} (char_idx updated)"),
                    changed,
                    changed,
                )
            } else {
                DispatchReport::success_with_flags(
                    format!("Dispatch: vim command captured -> {}", trimmed),
                    true,
                    false,
                )
            };
            let _ = ctx.app_state.close_command_palette();
            if ctx.app_state.current_mode() == EditorMode::PaletteFocus {
                let _ = ctx.app_state.apply_mode_event(ModeEvent::ExitFocus);
            }
            report
        }
        CommandPaletteAction::SelectTheme(theme_name) => DispatchReport::success_with_flags(
            format!("Dispatch: theme selection deferred -> {}", theme_name),
            false,
            false,
        ),
        CommandPaletteAction::JumpToSymbol(symbol) => {
            let _ = ctx.app_state.close_command_palette();
            if ctx.app_state.current_mode() == EditorMode::PaletteFocus {
                let _ = ctx.app_state.apply_mode_event(ModeEvent::ExitFocus);
            }
            DispatchReport::success_with_flags(
                format!("Dispatch: workspace symbol selected -> {}", symbol),
                true,
                false,
            )
        }
        CommandPaletteAction::JumpToDocumentSymbol { name, line, column } => {
            ctx.app_state.push_jump();
            let jumped = ctx
                .app_state
                .jump_to_line_and_column(line as usize, column as usize);
            let closed = ctx.close_palette_and_exit_focus();
            DispatchReport::success_with_flags(
                format!(
                    "Dispatch: document symbol selected -> {} @ {}:{}",
                    name,
                    line + 1,
                    column + 1
                ),
                true,
                jumped || closed,
            )
        }
        CommandPaletteAction::SelectFileHistoryEntry(index) => {
            let changed = ctx.app_state.preview_file_history_index(index);
            let closed = ctx.close_palette_and_exit_focus();
            let accepted = ctx.app_state.accept_file_history_preview();
            DispatchReport::success_with_flags(
                format!("Dispatch: file history confirmed -> entry #{index}"),
                true,
                changed || closed || accepted,
            )
        }
    }
}

fn close_picker(ctx: &mut DispatchCtx<'_, '_, '_>) -> DispatchReport {
    let clears_search = matches!(
        ctx.app_state.command_palette_mode(),
        Some(CommandPaletteMode::InFileSearch)
    );
    let cancels_file_history = matches!(
        ctx.app_state.command_palette_mode(),
        Some(CommandPaletteMode::FileHistory)
    );
    let mut changed = ctx.close_palette_and_exit_focus();
    if clears_search {
        changed |= ctx.app_state.clear_search_highlights();
    }
    if cancels_file_history {
        changed |= ctx.app_state.cancel_file_history_preview();
    }

    DispatchReport::success(
        if changed {
            "Dispatch: file picker closed".to_string()
        } else {
            "Dispatch: file picker close ignored".to_string()
        },
        changed,
    )
}
