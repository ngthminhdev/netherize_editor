use crate::core::{
    commands::Command,
    mode::{EditorMode, ModeEvent},
};

use super::common::{DispatchCtx, DispatchReport};

/// Chụp lại (file, line, col) hiện tại trước một jump-motion (gg/G/{}/n/N).
fn jump_origin(ctx: &DispatchCtx<'_, '_, '_>) -> Option<(std::path::PathBuf, usize, usize)> {
    ctx.app_state.active_file().map(|active| {
        let (line, col) = ctx.app_state.cursor_line_col();
        (active.to_path_buf(), line, col)
    })
}

/// Push origin vào jump stack nếu motion thực sự di chuyển cursor (vim jumplist).
fn record_jump_if_moved(
    ctx: &mut DispatchCtx<'_, '_, '_>,
    origin: Option<(std::path::PathBuf, usize, usize)>,
    moved: bool,
) {
    if moved && let Some((path, line, col)) = origin {
        ctx.app_state.push_jump_entry(path, line, col);
    }
}

pub(super) fn dispatch(ctx: &mut DispatchCtx<'_, '_, '_>, command: Command) -> DispatchReport {
    if let Some(report) = dispatch_terminal_normal(ctx, &command) {
        return report;
    }

    match command {
        Command::MoveLeft => {
            let before_cursor = ctx.app_state.cursor_char_idx();
            ctx.app_state.move_left();
            let changed = ctx.app_state.cursor_char_idx() != before_cursor;
            DispatchReport::success(
                if changed {
                    "Dispatch: applied to active buffer (move left)".to_string()
                } else {
                    "Dispatch: move left ignored (already at line/file start)".to_string()
                },
                changed,
            )
        }
        Command::MoveRight => {
            let before_cursor = ctx.app_state.cursor_char_idx();
            ctx.app_state.move_right();
            let changed = ctx.app_state.cursor_char_idx() != before_cursor;
            DispatchReport::success(
                if changed {
                    "Dispatch: applied to active buffer (move right)".to_string()
                } else {
                    "Dispatch: move right ignored (already at line end)".to_string()
                },
                changed,
            )
        }
        Command::MoveUp => {
            let before_cursor = ctx.app_state.cursor_char_idx();
            ctx.app_state.move_up();
            let changed = ctx.app_state.cursor_char_idx() != before_cursor;
            DispatchReport::success(
                if changed {
                    "Dispatch: applied to active buffer (move up)".to_string()
                } else {
                    "Dispatch: move up ignored (already at first line)".to_string()
                },
                changed,
            )
        }
        Command::MoveDown => {
            let before_cursor = ctx.app_state.cursor_char_idx();
            ctx.app_state.move_down();
            let changed = ctx.app_state.cursor_char_idx() != before_cursor;
            DispatchReport::success(
                if changed {
                    "Dispatch: applied to active buffer (move down)".to_string()
                } else {
                    "Dispatch: move down ignored (already at last line)".to_string()
                },
                changed,
            )
        }
        Command::MoveWordForward => {
            let changed = ctx.app_state.move_word_forward();
            DispatchReport::success(
                if changed {
                    "Dispatch: applied to active buffer (move word forward)".to_string()
                } else {
                    "Dispatch: move word forward ignored".to_string()
                },
                changed,
            )
        }
        Command::MoveWordBackward => {
            let changed = ctx.app_state.move_word_backward();
            DispatchReport::success(
                if changed {
                    "Dispatch: applied to active buffer (move word backward)".to_string()
                } else {
                    "Dispatch: move word backward ignored".to_string()
                },
                changed,
            )
        }
        Command::MoveWordEnd => {
            let changed = ctx.app_state.move_word_end();
            DispatchReport::success(
                if changed {
                    "Dispatch: applied to active buffer (move word end)".to_string()
                } else {
                    "Dispatch: move word end ignored".to_string()
                },
                changed,
            )
        }
        Command::MoveToLineStart => {
            let changed = ctx.app_state.move_to_line_start();
            DispatchReport::success(
                if changed {
                    "Dispatch: move to line start".to_string()
                } else {
                    "Dispatch: move to line start ignored".to_string()
                },
                changed,
            )
        }
        Command::MoveToLineEnd => {
            let changed = ctx.app_state.move_to_line_end();
            DispatchReport::success(
                if changed {
                    "Dispatch: move to line end".to_string()
                } else {
                    "Dispatch: move to line end ignored".to_string()
                },
                changed,
            )
        }
        Command::MoveToFirstNonWhitespace => {
            let changed = ctx.app_state.move_to_first_non_whitespace();
            DispatchReport::success(
                if changed {
                    "Dispatch: move to first non-whitespace".to_string()
                } else {
                    "Dispatch: move to first non-whitespace ignored".to_string()
                },
                changed,
            )
        }
        Command::MoveToFirstLine => {
            let origin = jump_origin(ctx);
            let changed = ctx.app_state.move_to_first_line();
            record_jump_if_moved(ctx, origin, changed);
            DispatchReport::success("Dispatch: move to first line", changed)
        }
        Command::MoveToLastLine => {
            let origin = jump_origin(ctx);
            let changed = ctx.app_state.move_to_last_line();
            record_jump_if_moved(ctx, origin, changed);
            DispatchReport::success("Dispatch: move to last line", changed)
        }
        Command::MoveParagraphUp => {
            let origin = jump_origin(ctx);
            let changed = ctx.app_state.move_paragraph_up();
            record_jump_if_moved(ctx, origin, changed);
            DispatchReport::success("Dispatch: move paragraph up", changed)
        }
        Command::MoveParagraphDown => {
            let origin = jump_origin(ctx);
            let changed = ctx.app_state.move_paragraph_down();
            record_jump_if_moved(ctx, origin, changed);
            DispatchReport::success("Dispatch: move paragraph down", changed)
        }
        Command::MatchBracket => {
            let origin = jump_origin(ctx);
            let changed = ctx.app_state.match_bracket();
            record_jump_if_moved(ctx, origin, changed);
            DispatchReport::success(
                if changed {
                    "Dispatch: match bracket".to_string()
                } else {
                    "Dispatch: match bracket ignored".to_string()
                },
                changed,
            )
        }
        Command::ScrollHalfPageUp | Command::ScrollHalfPageDown | Command::CenterCursorLine => {
            DispatchReport::success_with_flags(
                "Dispatch: scroll (handled by event loop)",
                true,
                false,
            )
        }
        Command::LeapStart
        | Command::LeapActivate(_)
        | Command::LeapJump(_)
        | Command::LeapCancel => DispatchReport::success_with_flags(
            "Dispatch: leap (handled by event loop)",
            true,
            false,
        ),
        Command::MoveFindChar(kind, target) => {
            let changed = ctx.app_state.find_char_motion_and_highlight(kind, target);
            DispatchReport::success(
                if changed {
                    format!("Dispatch: find char '{target}' ({kind:?})")
                } else {
                    format!("Dispatch: find char '{target}' not found on line")
                },
                changed,
            )
        }
        Command::SearchNext => {
            let origin = jump_origin(ctx);
            let changed = ctx.app_state.search_next();
            record_jump_if_moved(ctx, origin, changed);
            DispatchReport::success(
                if changed {
                    "Dispatch: search next".to_string()
                } else {
                    "Dispatch: search next ignored".to_string()
                },
                changed,
            )
        }
        Command::SearchPrev => {
            let origin = jump_origin(ctx);
            let changed = ctx.app_state.search_prev();
            record_jump_if_moved(ctx, origin, changed);
            DispatchReport::success(
                if changed {
                    "Dispatch: search previous".to_string()
                } else {
                    "Dispatch: search previous ignored".to_string()
                },
                changed,
            )
        }
        Command::SearchWordUnderCursor => {
            // Push current position onto jump stack before searching (like gd, gD)
            // so Ctrl-O can return to the original position
            ctx.app_state.push_jump();

            let changed = ctx.app_state.search_word_under_cursor();
            // nvim behavior: * in visual mode searches selected text, exits to Normal
            if matches!(ctx.app_state.current_mode(), EditorMode::Visual) {
                if let Ok(result) = ctx.app_state.apply_mode_event(ModeEvent::Escape) {
                    return DispatchReport::success(
                        "Dispatch: search word under cursor",
                        changed || result.changed,
                    );
                }
            }
            DispatchReport::success(
                if changed {
                    "Dispatch: search word under cursor".to_string()
                } else {
                    "Dispatch: search word under cursor ignored".to_string()
                },
                changed,
            )
        }
        Command::ClearSearchHighlights => {
            let changed = ctx.app_state.clear_search_highlights();
            DispatchReport::success(
                if changed {
                    "Dispatch: cleared search highlights".to_string()
                } else {
                    "Dispatch: clear search highlights ignored".to_string()
                },
                changed,
            )
        }
        // A routing mistake (mod.rs groups a command here without an arm)
        // must never abort the editor — fail the dispatch loudly instead.
        other => DispatchReport::failure(format!(
            "Dispatch: navigation::dispatch has no arm for {other:?} — routing bug"
        )),
    }
}

fn dispatch_terminal_normal(
    ctx: &mut DispatchCtx<'_, '_, '_>,
    command: &Command,
) -> Option<DispatchReport> {
    if !ctx.terminal_normal_active() {
        return None;
    }

    let grid = ctx.terminal_grid_mut()?;
    let (message, changed) = match command {
        Command::MoveLeft => (
            "Dispatch: moved terminal virtual cursor left",
            grid.move_virtual_left(),
        ),
        Command::MoveRight => (
            "Dispatch: moved terminal virtual cursor right",
            grid.move_virtual_right(),
        ),
        Command::MoveUp => (
            "Dispatch: moved terminal virtual cursor up",
            grid.move_virtual_up(),
        ),
        Command::MoveDown => (
            "Dispatch: moved terminal virtual cursor down",
            grid.move_virtual_down(),
        ),
        Command::MoveWordForward => (
            "Dispatch: moved terminal virtual cursor word forward",
            grid.move_virtual_word_forward(),
        ),
        Command::MoveWordBackward => (
            "Dispatch: moved terminal virtual cursor word backward",
            grid.move_virtual_word_backward(),
        ),
        Command::MoveWordEnd => (
            "Dispatch: moved terminal virtual cursor to word end",
            grid.move_virtual_word_end(),
        ),
        Command::MoveToLineStart => (
            "Dispatch: moved terminal virtual cursor to line start",
            grid.move_virtual_to_line_start(),
        ),
        Command::MoveToLineEnd => (
            "Dispatch: moved terminal virtual cursor to line end",
            grid.move_virtual_to_line_end(),
        ),
        Command::MoveToFirstNonWhitespace => (
            "Dispatch: moved terminal virtual cursor to first non-whitespace",
            grid.move_virtual_to_first_non_whitespace(),
        ),
        Command::MoveToFirstLine => (
            "Dispatch: moved terminal virtual cursor to first history line",
            grid.move_virtual_to_first_line(),
        ),
        Command::MoveToLastLine => (
            "Dispatch: moved terminal virtual cursor to last history line",
            grid.move_virtual_to_last_line(),
        ),
        Command::ScrollHalfPageUp => {
            let half_page = (grid.rows / 2).max(1);
            (
                "Dispatch: scrolled terminal virtual cursor half page up",
                grid.move_virtual_half_page_up(half_page),
            )
        }
        Command::ScrollHalfPageDown => {
            let half_page = (grid.rows / 2).max(1);
            (
                "Dispatch: scrolled terminal virtual cursor half page down",
                grid.move_virtual_half_page_down(half_page),
            )
        }
        Command::CenterCursorLine => (
            "Dispatch: centered terminal virtual cursor line",
            grid.center_virtual_cursor_line(),
        ),
        _ => return None,
    };

    Some(DispatchReport::success(message, changed))
}
