use crate::core::commands::Command;

use super::common::{DispatchCtx, DispatchReport};

pub(super) fn dispatch(ctx: &mut DispatchCtx<'_, '_>, command: Command) -> DispatchReport {
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
            let changed = ctx.app_state.move_to_first_line();
            DispatchReport::success("Dispatch: move to first line", changed)
        }
        Command::MoveToLastLine => {
            let changed = ctx.app_state.move_to_last_line();
            DispatchReport::success("Dispatch: move to last line", changed)
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
        Command::SearchNext => {
            let changed = ctx.app_state.search_next();
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
            let changed = ctx.app_state.search_prev();
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
            let changed = ctx.app_state.search_word_under_cursor();
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
        _ => unreachable!("navigation::dispatch received non-navigation command"),
    }
}
