use std::{
    fs,
    time::{SystemTime, UNIX_EPOCH},
};
use sysinfo::{Pid, ProcessesToUpdate, System};

use crate::{
    app::{
        app_state::AppState,
        clipboard::ClipboardProvider,
        command_palette::{CommandPaletteItem, CommandPaletteMode},
    },
    config::theme_config::ThemeConfig,
    core::{
        command_dispatch::{
            dispatch_command, dispatch_command_count, dispatch_command_with_clipboard,
            dispatch_command_with_clipboard_and_terminal, dispatch_command_with_terminal,
        },
        commands::{
            Command, FindMotionKind, Motion, OperationTarget, Operator, TextObjectKind,
            TextObjectModifier,
        },
        mode::{EditorMode, ModeEvent},
    },
    terminal::grid::TerminalGrid,
};

#[derive(Default)]
struct MockClipboard {
    text: String,
}

impl ClipboardProvider for MockClipboard {
    fn get_text(&mut self) -> Result<String, String> {
        Ok(self.text.clone())
    }

    fn set_text(&mut self, text: &str) -> Result<(), String> {
        self.text = text.to_string();
        Ok(())
    }
}

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
fn toggle_minimap_command_flips_visibility_without_text_mutation() {
    let mut app_state = AppState::from_text(unique_temp_path("minimap"), "one\ntwo");

    let first = dispatch_command(&mut app_state, Command::ToggleMinimap);
    assert!(first.success);
    assert!(first.state_changed);
    assert!(first.request_redraw);
    assert!(app_state.minimap_visible());
    assert_eq!(app_state.text_string(), "one\ntwo");

    let second = dispatch_command(&mut app_state, Command::ToggleMinimap);
    assert!(second.success);
    assert!(!app_state.minimap_visible());
    assert_eq!(app_state.text_string(), "one\ntwo");
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
fn insert_open_paren_auto_pairs_and_places_cursor_inside() {
    let mut app_state = AppState::from_text(unique_temp_path("auto_pair"), "ab");
    app_state.move_right();

    let report = dispatch_command(&mut app_state, Command::InsertChar('('));

    assert!(report.success);
    assert!(report.state_changed);
    assert_eq!(app_state.text_string(), "a()b");
    assert_eq!(app_state.cursor_line_col(), (0, 2));
}

#[test]
fn insert_quote_auto_pairs_and_step_over_works() {
    let mut app_state = AppState::new(unique_temp_path("quote_pair"));

    let open = dispatch_command(&mut app_state, Command::InsertChar('"'));
    assert!(open.success);
    assert_eq!(app_state.text_string(), "\"\"");
    assert_eq!(app_state.cursor_line_col(), (0, 1));

    let step = dispatch_command(&mut app_state, Command::InsertChar('"'));
    assert!(step.success);
    assert!(step.state_changed);
    assert_eq!(app_state.text_string(), "\"\"");
    assert_eq!(app_state.cursor_line_col(), (0, 2));
}

#[test]
fn insert_backtick_auto_pairs_and_step_over_works() {
    let mut app_state = AppState::new(unique_temp_path("backtick_pair"));

    let open = dispatch_command(&mut app_state, Command::InsertChar('`'));
    assert!(open.success);
    assert_eq!(app_state.text_string(), "``");
    assert_eq!(app_state.cursor_line_col(), (0, 1));

    let step = dispatch_command(&mut app_state, Command::InsertChar('`'));
    assert!(step.success);
    assert!(step.state_changed);
    assert_eq!(app_state.text_string(), "``");
    assert_eq!(app_state.cursor_line_col(), (0, 2));
}

#[test]
fn insert_closing_paren_steps_over_existing_closer() {
    let mut app_state = AppState::new(unique_temp_path("step_over"));
    let _ = dispatch_command(&mut app_state, Command::InsertChar('('));

    let report = dispatch_command(&mut app_state, Command::InsertChar(')'));

    assert!(report.success);
    assert!(report.state_changed);
    assert_eq!(app_state.text_string(), "()");
    assert_eq!(app_state.cursor_line_col(), (0, 2));
}

#[test]
fn newline_between_braces_expands_block_with_indent() {
    let mut app_state = AppState::new(unique_temp_path("smart_enter"));
    let _ = dispatch_command(&mut app_state, Command::InsertChar('{'));

    let report = dispatch_command(&mut app_state, Command::Newline);

    assert!(report.success);
    assert!(report.state_changed);
    assert_eq!(app_state.text_string(), "{\n    \n}");
    assert_eq!(app_state.cursor_line_col(), (1, 4));
}

#[test]
fn auto_pair_and_smart_enter_group_into_insert_session_undo_transaction() {
    let mut app_state = AppState::new(unique_temp_path("pair_undo"));

    let enter_insert =
        dispatch_command(&mut app_state, Command::SwitchMode(ModeEvent::EnterInsert));
    assert!(enter_insert.success);

    let pair = dispatch_command(&mut app_state, Command::InsertChar('{'));
    assert!(pair.success);
    assert_eq!(app_state.text_string(), "{}");

    let exit_after_pair =
        dispatch_command(&mut app_state, Command::SwitchMode(ModeEvent::EnterNormal));
    assert!(exit_after_pair.success);

    let undo_pair = dispatch_command(&mut app_state, Command::Undo);
    assert!(undo_pair.success);
    assert!(undo_pair.state_changed);
    assert_eq!(app_state.text_string(), "");

    let reenter_insert =
        dispatch_command(&mut app_state, Command::SwitchMode(ModeEvent::EnterInsert));
    assert!(reenter_insert.success);
    let _ = dispatch_command(&mut app_state, Command::InsertChar('{'));
    let smart_enter = dispatch_command(&mut app_state, Command::Newline);
    assert!(smart_enter.success);
    assert_eq!(app_state.text_string(), "{\n    \n}");

    let exit_after_newline =
        dispatch_command(&mut app_state, Command::SwitchMode(ModeEvent::EnterNormal));
    assert!(exit_after_newline.success);

    let undo_newline = dispatch_command(&mut app_state, Command::Undo);
    assert!(undo_newline.success);
    assert!(undo_newline.state_changed);
    assert_eq!(app_state.text_string(), "");
}

#[test]
fn backspace_between_empty_pair_deletes_both_chars_in_insert_mode() {
    let mut app_state = AppState::new(unique_temp_path("smart_backspace_dispatch_pair"));

    let enter_insert =
        dispatch_command(&mut app_state, Command::SwitchMode(ModeEvent::EnterInsert));
    assert!(enter_insert.success);

    let pair = dispatch_command(&mut app_state, Command::InsertChar('('));
    assert!(pair.success);
    assert_eq!(app_state.text_string(), "()");

    let backspace = dispatch_command(&mut app_state, Command::Backspace);
    assert!(backspace.success);
    assert!(backspace.state_changed);
    assert_eq!(app_state.text_string(), "");

    let exit_insert = dispatch_command(&mut app_state, Command::SwitchMode(ModeEvent::EnterNormal));
    assert!(exit_insert.success);

    let undo = dispatch_command(&mut app_state, Command::Undo);
    assert!(undo.success);
    assert!(undo.state_changed);
    assert_eq!(app_state.text_string(), "");
}

#[test]
fn backspace_between_empty_quotes_deletes_both_chars() {
    let mut app_state = AppState::new(unique_temp_path("smart_backspace_dispatch_quotes"));

    let open = dispatch_command(&mut app_state, Command::InsertChar('"'));
    assert!(open.success);
    assert_eq!(app_state.text_string(), "\"\"");

    let backspace = dispatch_command(&mut app_state, Command::Backspace);
    assert!(backspace.success);
    assert!(backspace.state_changed);
    assert_eq!(app_state.text_string(), "");
}

#[test]
fn backspace_between_empty_backticks_deletes_both_chars() {
    let mut app_state = AppState::new(unique_temp_path("smart_backspace_dispatch_backticks"));

    let open = dispatch_command(&mut app_state, Command::InsertChar('`'));
    assert!(open.success);
    assert_eq!(app_state.text_string(), "``");

    let backspace = dispatch_command(&mut app_state, Command::Backspace);
    assert!(backspace.success);
    assert!(backspace.state_changed);
    assert_eq!(app_state.text_string(), "");
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
    assert_eq!(app_state.text_string(), "   bar");

    let _ = dispatch_command(&mut app_state, Command::SwitchMode(ModeEvent::EnterNormal));
    let replace = dispatch_command(&mut app_state, Command::ReplaceChar('X'));
    assert!(replace.success);
    assert!(replace.state_changed);
    assert_eq!(app_state.current_mode(), EditorMode::Normal);
    assert_eq!(app_state.text_string(), "X  bar");
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
fn open_theme_selector_dispatch_populates_static_theme_items() {
    let mut app_state = AppState::new(unique_temp_path("theme_selector"));

    let report = dispatch_command(&mut app_state, Command::OpenThemeSelector);

    assert!(report.success);
    assert_eq!(
        app_state.command_palette_mode(),
        Some(CommandPaletteMode::ThemeSelector)
    );
    assert!(
        app_state
            .command_palette_result_labels()
            .contains(&ThemeConfig::active_profile())
    );
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
fn delete_current_line_copies_deleted_text_to_clipboard() {
    let mut app_state = AppState::from_text(unique_temp_path("cut_line"), "one\ntwo\nthree");
    let mut clipboard = MockClipboard::default();
    let _ = dispatch_command(&mut app_state, Command::SwitchMode(ModeEvent::EnterNormal));
    app_state.move_down();

    let report = dispatch_command_with_clipboard(
        &mut app_state,
        Command::DeleteCurrentLine,
        Some(&mut clipboard),
    );

    assert!(report.success);
    assert_eq!(clipboard.text, "two\n");
    assert_eq!(app_state.text_string(), "one\nthree");
}

#[test]
fn counted_delete_char_groups_into_single_undo_transaction() {
    let mut app_state = AppState::from_text(unique_temp_path("count_delete_char"), "abcd");
    let _ = dispatch_command(&mut app_state, Command::SwitchMode(ModeEvent::EnterNormal));

    let delete = dispatch_command_count(&mut app_state, Command::DeleteChar, 2);
    assert!(delete.success);
    assert!(delete.state_changed);
    assert_eq!(app_state.text_string(), "cd");

    let undo = dispatch_command(&mut app_state, Command::Undo);
    assert!(undo.success);
    assert!(undo.state_changed);
    assert_eq!(app_state.text_string(), "abcd");
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
fn yank_selection_copies_text_and_returns_to_normal_mode() {
    let mut app_state = AppState::from_text(unique_temp_path("yank_selection"), "abcdef");
    let mut clipboard = MockClipboard::default();
    let _ = dispatch_command(&mut app_state, Command::SwitchMode(ModeEvent::EnterVisual));
    app_state.move_right();
    app_state.move_right();

    let report = dispatch_command_with_clipboard(
        &mut app_state,
        Command::YankSelection,
        Some(&mut clipboard),
    );

    assert!(report.success);
    assert!(report.state_changed);
    assert_eq!(clipboard.text, "abc");
    assert_eq!(app_state.text_string(), "abcdef");
    assert_eq!(app_state.current_mode(), EditorMode::Normal);
    assert!(app_state.visual_selection_range().is_none());
}

#[test]
fn terminal_normal_mode_initializes_virtual_cursor_and_vim_motions() {
    let mut app_state = AppState::from_text(unique_temp_path("terminal_normal_mode"), "buffer");
    let mut grid = TerminalGrid::new(8, 3);
    let _ = grid.feed_chunk("one\r\ntwo\r\nthree");

    let _ = dispatch_command(
        &mut app_state,
        Command::SwitchMode(ModeEvent::FocusTerminal),
    );
    let enter = dispatch_command_with_terminal(
        &mut app_state,
        Command::SwitchMode(ModeEvent::EnterTerminalNormal),
        Some(&mut grid),
    );
    assert!(enter.success);
    assert_eq!(app_state.current_mode(), EditorMode::TerminalNormal);
    assert_eq!(
        grid.virtual_cursor.row,
        grid.live_cursor_absolute_position().row
    );

    let move_up = dispatch_command_with_terminal(&mut app_state, Command::MoveUp, Some(&mut grid));
    assert!(move_up.success);
    assert!(move_up.state_changed);
    assert_eq!(grid.virtual_cursor.row, 1);

    let move_start =
        dispatch_command_with_terminal(&mut app_state, Command::MoveToLineStart, Some(&mut grid));
    assert!(move_start.success);
    assert_eq!(grid.virtual_cursor.col, 0);
}

#[test]
fn terminal_normal_selection_yanks_terminal_grid_and_returns_to_typing_mode() {
    let mut app_state = AppState::from_text(unique_temp_path("terminal_normal_yank"), "buffer");
    let mut clipboard = MockClipboard::default();
    let mut grid = TerminalGrid::new(8, 4);
    let _ = grid.feed_chunk("alpha\r\nbeta\r\ngamma");

    let _ = dispatch_command(
        &mut app_state,
        Command::SwitchMode(ModeEvent::FocusTerminal),
    );
    let _ = dispatch_command_with_terminal(
        &mut app_state,
        Command::SwitchMode(ModeEvent::EnterTerminalNormal),
        Some(&mut grid),
    );
    let _ = dispatch_command_with_terminal(&mut app_state, Command::MoveUp, Some(&mut grid));
    let _ =
        dispatch_command_with_terminal(&mut app_state, Command::MoveToLineStart, Some(&mut grid));
    let _ = dispatch_command_with_terminal(
        &mut app_state,
        Command::SwitchMode(ModeEvent::EnterVisual),
        Some(&mut grid),
    );
    let _ = dispatch_command_with_terminal(&mut app_state, Command::MoveWordEnd, Some(&mut grid));

    let yank = dispatch_command_with_clipboard_and_terminal(
        &mut app_state,
        Command::YankSelection,
        Some(&mut clipboard),
        Some(&mut grid),
    );

    assert!(yank.success);
    assert_eq!(clipboard.text, "beta");
    assert_eq!(app_state.current_mode(), EditorMode::TerminalFocus);
    assert!(grid.selection_anchor.is_none());
}

#[test]
fn yank_current_line_copies_line_to_clipboard_without_mutating_buffer() {
    let mut app_state =
        AppState::from_text(unique_temp_path("yank_current_line"), "one\ntwo\nthree");
    let mut clipboard = MockClipboard::default();
    let _ = dispatch_command(&mut app_state, Command::SwitchMode(ModeEvent::EnterNormal));
    app_state.move_down();

    let report = dispatch_command_with_clipboard(
        &mut app_state,
        Command::YankCurrentLine,
        Some(&mut clipboard),
    );

    assert!(report.success);
    assert!(!report.state_changed);
    assert_eq!(clipboard.text, "two\n");
    assert_eq!(app_state.text_string(), "one\ntwo\nthree");
}

#[test]
fn yank_to_word_end_copies_suffix_of_current_word_to_clipboard() {
    let mut app_state = AppState::from_text(unique_temp_path("yank_to_word_end"), "hello world");
    let mut clipboard = MockClipboard::default();
    let _ = dispatch_command(&mut app_state, Command::SwitchMode(ModeEvent::EnterNormal));
    let _ = dispatch_command(&mut app_state, Command::MoveRight);
    let _ = dispatch_command(&mut app_state, Command::MoveRight);

    let report = dispatch_command_with_clipboard(
        &mut app_state,
        Command::YankToWordEnd,
        Some(&mut clipboard),
    );

    assert!(report.success);
    assert!(!report.state_changed);
    assert_eq!(clipboard.text, "llo");
    assert_eq!(app_state.text_string(), "hello world");
}

#[test]
fn operate_change_word_forward_uses_cw_semantics_like_ce() {
    let mut app_state = AppState::from_text(unique_temp_path("operate_cw"), "hello world");
    let _ = dispatch_command(&mut app_state, Command::SwitchMode(ModeEvent::EnterNormal));

    let report = dispatch_command(
        &mut app_state,
        Command::Operate {
            op: Operator::Change,
            target: OperationTarget::Motion(Motion::WordForward),
        },
    );

    assert!(report.success);
    assert_eq!(app_state.text_string(), " world");
    assert_eq!(app_state.current_mode(), EditorMode::Insert);
}

#[test]
fn operate_delete_line_end_matches_dollar_motion() {
    let mut app_state = AppState::from_text(unique_temp_path("operate_dollar"), "abc def\nzzz");
    let _ = dispatch_command(&mut app_state, Command::SwitchMode(ModeEvent::EnterNormal));
    let _ = dispatch_command(&mut app_state, Command::MoveRight);
    let _ = dispatch_command(&mut app_state, Command::MoveRight);

    let report = dispatch_command(
        &mut app_state,
        Command::Operate {
            op: Operator::Delete,
            target: OperationTarget::Motion(Motion::LineEnd),
        },
    );

    assert!(report.success);
    assert_eq!(app_state.text_string(), "ab\nzzz");
}

#[test]
fn operate_delete_inner_word_removes_current_word() {
    let mut app_state = AppState::from_text(unique_temp_path("operate_diw"), "one two three");
    let _ = dispatch_command(&mut app_state, Command::SwitchMode(ModeEvent::EnterNormal));
    let _ = dispatch_command(&mut app_state, Command::MoveWordForward);

    let report = dispatch_command(
        &mut app_state,
        Command::Operate {
            op: Operator::Delete,
            target: OperationTarget::TextObject {
                modifier: TextObjectModifier::Inner,
                kind: TextObjectKind::Word,
            },
        },
    );

    assert!(report.success);
    assert_eq!(app_state.text_string(), "one  three");
}

#[test]
fn operate_delete_find_forward_includes_target_char() {
    let mut app_state = AppState::from_text(unique_temp_path("operate_df"), "abc def ghi");
    let _ = dispatch_command(&mut app_state, Command::SwitchMode(ModeEvent::EnterNormal));

    let report = dispatch_command(
        &mut app_state,
        Command::Operate {
            op: Operator::Delete,
            target: OperationTarget::Motion(Motion::FindChar(FindMotionKind::ForwardTo, 'd')),
        },
    );

    assert!(report.success);
    assert_eq!(app_state.text_string(), "ef ghi");
}

#[test]
fn move_find_char_jumps_to_char_and_highlights_all_matches() {
    let mut app_state = AppState::from_text(unique_temp_path("move_find_char"), "abc abc\nabc");
    let _ = dispatch_command(&mut app_state, Command::SwitchMode(ModeEvent::EnterNormal));

    let report = dispatch_command(
        &mut app_state,
        Command::MoveFindChar(FindMotionKind::ForwardTo, 'b'),
    );

    assert!(report.success);
    assert_eq!(app_state.cursor_line_col(), (0, 1));
    assert_eq!(app_state.last_search_query(), "b");
    assert_eq!(app_state.search_highlights().len(), 3);

    // n (SearchNext) nhảy tới match kế tiếp — kể cả ở dòng khác.
    let _ = dispatch_command(&mut app_state, Command::SearchNext);
    assert_eq!(app_state.cursor_line_col(), (0, 5));
    let _ = dispatch_command(&mut app_state, Command::SearchNext);
    assert_eq!(app_state.cursor_line_col(), (1, 1));
}

#[test]
fn move_find_char_till_stops_before_target() {
    let mut app_state = AppState::from_text(unique_temp_path("move_till_char"), "abc def");
    let _ = dispatch_command(&mut app_state, Command::SwitchMode(ModeEvent::EnterNormal));

    let report = dispatch_command(
        &mut app_state,
        Command::MoveFindChar(FindMotionKind::ForwardTill, 'd'),
    );

    assert!(report.success);
    assert_eq!(app_state.cursor_line_col(), (0, 3));
}

#[test]
fn move_find_char_backward_jumps_to_previous_occurrence() {
    let mut app_state = AppState::from_text(unique_temp_path("move_find_back"), "abc abc");
    let _ = dispatch_command(&mut app_state, Command::SwitchMode(ModeEvent::EnterNormal));
    let _ = dispatch_command(&mut app_state, Command::MoveToLineEnd);

    let report = dispatch_command(
        &mut app_state,
        Command::MoveFindChar(FindMotionKind::BackwardTo, 'a'),
    );

    assert!(report.success);
    assert_eq!(app_state.cursor_line_col(), (0, 4));
}

#[test]
fn move_find_char_missing_target_keeps_cursor_but_still_highlights() {
    let mut app_state = AppState::from_text(unique_temp_path("move_find_miss"), "abc\nxyz");
    let _ = dispatch_command(&mut app_state, Command::SwitchMode(ModeEvent::EnterNormal));

    // 'x' không có trên dòng 1 -> cursor đứng yên (Vim), nhưng highlight vẫn set
    // để n nhảy tới occurrence ở dòng khác.
    let _ = dispatch_command(
        &mut app_state,
        Command::MoveFindChar(FindMotionKind::ForwardTo, 'x'),
    );
    assert_eq!(app_state.cursor_line_col(), (0, 0));
    assert_eq!(app_state.search_highlights().len(), 1);

    let _ = dispatch_command(&mut app_state, Command::SearchNext);
    assert_eq!(app_state.cursor_line_col(), (1, 0));
}

#[test]
fn yank_to_line_end_id_parses_to_yank_operate() {
    let command = crate::core::command_ids::parse("editor.yank_to_line_end", None);
    assert_eq!(
        command,
        Some(Command::Operate {
            op: Operator::Yank,
            target: OperationTarget::Motion(Motion::LineEnd),
        })
    );
}

#[test]
fn move_to_last_line_pushes_jump_origin() {
    let path = unique_temp_path("jump_origin_g");
    std::fs::write(&path, "one\ntwo\nthree").expect("write temp file");
    let mut app_state = AppState::from_text(path.clone(), "");
    let _ = dispatch_command(&mut app_state, Command::OpenFile(path.clone()));
    let canonical = app_state.active_file().expect("file opened").to_path_buf();
    let _ = dispatch_command(&mut app_state, Command::SwitchMode(ModeEvent::EnterNormal));

    let _ = dispatch_command(&mut app_state, Command::MoveToLastLine);
    assert_eq!(app_state.cursor_line_col(), (2, 0));

    let back = app_state.pop_jump_back();
    assert_eq!(back, Some((canonical, 0, 0)));
    let _ = std::fs::remove_file(path);
}

#[test]
fn search_next_pushes_jump_origin_for_ctrl_o() {
    let path = unique_temp_path("jump_origin_n");
    std::fs::write(&path, "alpha\nbeta\nalpha").expect("write temp file");
    let mut app_state = AppState::from_text(path.clone(), "");
    let _ = dispatch_command(&mut app_state, Command::OpenFile(path.clone()));
    let canonical = app_state.active_file().expect("file opened").to_path_buf();
    let _ = dispatch_command(&mut app_state, Command::SwitchMode(ModeEvent::EnterNormal));

    // * đặt query + nhảy tới match kế tiếp (đã push origin từ trước).
    let _ = dispatch_command(&mut app_state, Command::SearchWordUnderCursor);
    let after_star = app_state.cursor_line_col();
    assert_eq!(after_star, (2, 0));

    // n tiếp tục: phải push origin (line 2) để Ctrl+O quay về.
    let _ = dispatch_command(&mut app_state, Command::SearchNext);
    let back = app_state.pop_jump_back();
    assert_eq!(back, Some((canonical, 2, 0)));
    let _ = std::fs::remove_file(path);
}

#[test]
fn jump_stack_dedups_same_line_and_caps_at_100() {
    let path = unique_temp_path("jump_stack_cap");
    let mut app_state = AppState::from_text(path.clone(), "x");

    // Dedup: cùng (file, line) chỉ giữ một entry, cột được cập nhật.
    app_state.push_jump_entry(path.clone(), 5, 1);
    app_state.push_jump_entry(path.clone(), 5, 9);
    assert_eq!(app_state.pop_jump_back(), Some((path.clone(), 5, 9)));
    assert!(app_state.pop_jump_back().is_none());

    // Cap: 150 entry khác nhau -> chỉ giữ 100 entry mới nhất.
    for line in 0..150 {
        app_state.push_jump_entry(path.clone(), line, 0);
    }
    let mut count = 0;
    let mut last = None;
    while let Some(entry) = app_state.pop_jump_back() {
        last = Some(entry);
        count += 1;
        // pop_jump_back đẩy current pos sang forward stack, không ảnh hưởng back stack.
        if count > 200 {
            panic!("jump stack not capped");
        }
    }
    assert_eq!(count, 100);
    assert_eq!(last, Some((path, 50, 0)));
}

#[test]
fn change_to_line_end_alias_enters_insert_and_removes_suffix() {
    let mut app_state = AppState::from_text(unique_temp_path("change_to_eol"), "alpha beta");
    let _ = dispatch_command(&mut app_state, Command::SwitchMode(ModeEvent::EnterNormal));
    let _ = dispatch_command(&mut app_state, Command::MoveRight);
    let _ = dispatch_command(&mut app_state, Command::MoveRight);

    let report = dispatch_command(&mut app_state, Command::ChangeToLineEnd);

    assert!(report.success);
    assert_eq!(app_state.text_string(), "al");
    assert_eq!(app_state.current_mode(), EditorMode::Insert);
}

#[test]
fn delete_to_line_end_alias_removes_suffix_and_stays_normal() {
    let mut app_state = AppState::from_text(unique_temp_path("delete_to_eol"), "alpha beta");
    let _ = dispatch_command(&mut app_state, Command::SwitchMode(ModeEvent::EnterNormal));
    let _ = dispatch_command(&mut app_state, Command::MoveRight);
    let _ = dispatch_command(&mut app_state, Command::MoveRight);

    let report = dispatch_command(&mut app_state, Command::DeleteToLineEnd);

    assert!(report.success);
    assert_eq!(app_state.text_string(), "al");
    assert_eq!(app_state.current_mode(), EditorMode::Normal);
}

#[test]
fn substitute_line_keeps_indent_cursor_target() {
    let mut app_state =
        AppState::from_text(unique_temp_path("substitute_indent"), "    alpha\nnext");
    let _ = dispatch_command(&mut app_state, Command::SwitchMode(ModeEvent::EnterNormal));
    let report = dispatch_command(&mut app_state, Command::SubstituteLine);

    assert!(report.success);
    assert_eq!(app_state.current_mode(), EditorMode::Insert);
    assert_eq!(app_state.cursor_line_col(), (0, 4));
    assert_eq!(app_state.text_string(), "    \nnext");
}

#[test]
fn join_lines_alias_merges_next_line_like_shift_j() {
    let mut app_state = AppState::from_text(unique_temp_path("join_lines"), "alpha\n  beta\n");
    let _ = dispatch_command(&mut app_state, Command::SwitchMode(ModeEvent::EnterNormal));

    let report = dispatch_command(&mut app_state, Command::JoinLines);

    assert!(report.success);
    assert_eq!(app_state.text_string(), "alpha beta\n");
    assert_eq!(app_state.current_mode(), EditorMode::Normal);
}

#[test]
fn paragraph_motions_jump_between_blank_line_separated_blocks() {
    let mut app_state = AppState::from_text(
        unique_temp_path("paragraph_motion"),
        "one\ntwo\n\nthree\nfour\n\nfive\n",
    );
    let _ = dispatch_command(&mut app_state, Command::SwitchMode(ModeEvent::EnterNormal));

    let down = dispatch_command(&mut app_state, Command::MoveParagraphDown);
    assert!(down.success);
    assert_eq!(app_state.cursor_line_col().0, 2);

    let down_again = dispatch_command(&mut app_state, Command::MoveParagraphDown);
    assert!(down_again.success);
    assert_eq!(app_state.cursor_line_col().0, 5);

    let up = dispatch_command(&mut app_state, Command::MoveParagraphUp);
    assert!(up.success);
    assert_eq!(app_state.cursor_line_col().0, 2);
}

#[test]
fn visual_paragraph_motions_expand_selection_to_blank_separators() {
    let mut app_state = AppState::from_text(
        unique_temp_path("visual_paragraph_motion"),
        "one\ntwo\n\nthree\nfour\n\nfive\n",
    );
    let _ = dispatch_command(&mut app_state, Command::SwitchMode(ModeEvent::EnterVisual));

    let down = dispatch_command(&mut app_state, Command::MoveParagraphDown);
    assert!(down.success);
    let selection = app_state
        .visual_selection_range()
        .expect("selection after visual paragraph down");
    assert_eq!(selection.start_line, 0);
    assert_eq!(selection.end_line, 2);

    let down_again = dispatch_command(&mut app_state, Command::MoveParagraphDown);
    assert!(down_again.success);
    let selection = app_state
        .visual_selection_range()
        .expect("selection after second visual paragraph down");
    assert_eq!(selection.start_line, 0);
    assert_eq!(selection.end_line, 5);
}

#[test]
fn paste_after_participates_in_undo_transaction() {
    let mut app_state = AppState::from_text(unique_temp_path("paste_after"), "abc");
    let mut clipboard = MockClipboard {
        text: "XYZ".to_string(),
    };
    let _ = dispatch_command(&mut app_state, Command::SwitchMode(ModeEvent::EnterNormal));

    let paste =
        dispatch_command_with_clipboard(&mut app_state, Command::PasteAfter, Some(&mut clipboard));
    assert!(paste.success);
    assert!(paste.state_changed);
    assert_eq!(app_state.text_string(), "aXYZbc");

    let undo = dispatch_command(&mut app_state, Command::Undo);
    assert!(undo.success);
    assert!(undo.state_changed);
    assert_eq!(app_state.text_string(), "abc");
}

#[test]
fn paste_before_replaces_visual_selection_from_clipboard() {
    let mut app_state = AppState::from_text(unique_temp_path("paste_visual"), "abcdef");
    let mut clipboard = MockClipboard {
        text: "XYZ".to_string(),
    };
    let _ = dispatch_command(&mut app_state, Command::SwitchMode(ModeEvent::EnterVisual));
    app_state.move_right();
    app_state.move_right();

    let paste =
        dispatch_command_with_clipboard(&mut app_state, Command::PasteBefore, Some(&mut clipboard));
    assert!(paste.success);
    assert!(paste.state_changed);
    assert_eq!(app_state.text_string(), "XYZdef");
    assert_eq!(app_state.current_mode(), EditorMode::Normal);

    let undo = dispatch_command(&mut app_state, Command::Undo);
    assert!(undo.success);
    assert!(undo.state_changed);
    assert_eq!(app_state.text_string(), "abcdef");
}

#[test]
fn editor_paste_in_insert_mode_keeps_cursor_after_inserted_text() {
    let mut app_state = AppState::from_text(unique_temp_path("paste_system_insert"), "abc");
    let mut clipboard = MockClipboard {
        text: "XYZ".to_string(),
    };

    app_state.move_right();
    let paste =
        dispatch_command_with_clipboard(&mut app_state, Command::EditorPaste, Some(&mut clipboard));

    assert!(paste.success);
    assert!(paste.state_changed);
    assert_eq!(app_state.text_string(), "aXYZbc");
    assert_eq!(app_state.cursor_char_idx(), 4);
}

#[test]
fn match_bracket_dispatch_jumps_and_sets_ripple() {
    let mut app_state =
        AppState::from_text(unique_temp_path("match_bracket_dispatch"), "{ alpha }");

    let report = dispatch_command(&mut app_state, Command::MatchBracket);

    assert!(report.success);
    assert!(report.state_changed);
    assert_eq!(app_state.cursor_char_idx(), 8);
    assert_eq!(app_state.bracket_ripple_pos(), Some(8));
}

#[test]
fn editor_paste_appends_to_palette_query() {
    let mut app_state = AppState::from_text(unique_temp_path("paste_system_palette"), "alpha");
    let mut clipboard = MockClipboard {
        text: "foo\nbar".to_string(),
    };

    let open = dispatch_command(&mut app_state, Command::OpenInFileSearch);
    assert!(open.success);
    assert_eq!(app_state.current_mode(), EditorMode::PaletteFocus);

    let paste =
        dispatch_command_with_clipboard(&mut app_state, Command::EditorPaste, Some(&mut clipboard));

    assert!(paste.success);
    assert!(paste.state_changed);
    assert_eq!(app_state.command_palette_query_text(), "foo bar");
}

#[test]
fn editor_paste_appends_to_leetcode_problem_input_query() {
    let mut app_state = AppState::from_text(unique_temp_path("paste_leetcode_input"), "alpha");
    let mut clipboard = MockClipboard {
        text: "https://leetcode.com/problems/two-sum/".to_string(),
    };

    let open = dispatch_command(&mut app_state, Command::FetchLeetCodeProblem);
    assert!(open.success);
    assert_eq!(
        app_state.command_palette_mode(),
        Some(CommandPaletteMode::LeetCodeProblemInput)
    );

    let paste =
        dispatch_command_with_clipboard(&mut app_state, Command::EditorPaste, Some(&mut clipboard));

    assert!(paste.success);
    assert_eq!(
        app_state.command_palette_query_text(),
        "https://leetcode.com/problems/two-sum/"
    );
}

#[test]
fn editor_paste_appends_to_active_fuzzy_picker_query() {
    let mut app_state = AppState::from_text(unique_temp_path("paste_fuzzy_buffer"), "alpha");
    let mut clipboard = MockClipboard {
        text: "src\nmain".to_string(),
    };
    app_state.open_fuzzy_picker_buffer(CommandPaletteMode::FilePicker);

    let paste =
        dispatch_command_with_clipboard(&mut app_state, Command::EditorPaste, Some(&mut clipboard));

    assert!(paste.success);
    assert!(paste.state_changed);
    assert_eq!(app_state.command_palette_query_text(), "src main");
}

#[test]
fn linewise_yank_from_last_line_pastes_as_new_line_below() {
    let mut app_state = AppState::from_text(unique_temp_path("linewise_last_line"), "one\ntwo");
    let mut clipboard = MockClipboard::default();
    let _ = dispatch_command(&mut app_state, Command::SwitchMode(ModeEvent::EnterNormal));
    app_state.move_down();

    let yank = dispatch_command_with_clipboard(
        &mut app_state,
        Command::YankCurrentLine,
        Some(&mut clipboard),
    );
    assert!(yank.success);
    assert_eq!(clipboard.text, "two\n");

    app_state.move_to_first_line();
    let paste =
        dispatch_command_with_clipboard(&mut app_state, Command::PasteAfter, Some(&mut clipboard));
    assert!(paste.success);
    assert!(paste.state_changed);
    assert_eq!(app_state.text_string(), "one\ntwo\ntwo");
}

#[test]
fn counted_delete_word_forward_groups_into_single_undo_transaction() {
    let mut app_state =
        AppState::from_text(unique_temp_path("count_delete_word"), "one two three four");
    let _ = dispatch_command(&mut app_state, Command::SwitchMode(ModeEvent::EnterNormal));

    let delete = dispatch_command_count(&mut app_state, Command::DeleteWordForward, 2);
    assert!(delete.success);
    assert!(delete.state_changed);
    assert_eq!(app_state.text_string(), "three four");

    let undo = dispatch_command(&mut app_state, Command::Undo);
    assert!(undo.success);
    assert!(undo.state_changed);
    assert_eq!(app_state.text_string(), "one two three four");
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
fn repeated_operator_undo_cycles_do_not_show_runaway_memory_growth() {
    let mut system = System::new_all();
    let pid = Pid::from_u32(std::process::id());
    system.refresh_processes(ProcessesToUpdate::Some(&[pid]), true);
    let before = system.process(pid).map(|p| p.memory()).unwrap_or(0);

    let mut app_state = AppState::from_text(
        unique_temp_path("memory_operator_regression"),
        &"alpha beta gamma delta epsilon zeta eta theta\n".repeat(512),
    );
    let _ = dispatch_command(&mut app_state, Command::SwitchMode(ModeEvent::EnterNormal));

    for _ in 0..2_000 {
        let _ = dispatch_command(
            &mut app_state,
            Command::Operate {
                op: Operator::Delete,
                target: OperationTarget::Motion(Motion::WordForward),
            },
        );
        let _ = dispatch_command(&mut app_state, Command::Undo);
    }

    system.refresh_processes(ProcessesToUpdate::Some(&[pid]), true);
    let after = system.process(pid).map(|p| p.memory()).unwrap_or(before);

    assert!(
        after <= before.saturating_add(64 * 1024 * 1024),
        "memory grew too much: before={before} after={after}"
    );
}

#[test]
fn ignored_move_right_does_not_request_redraw() {
    let mut app_state = AppState::from_text(unique_temp_path("save"), "abc");
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
fn open_file_picker_command_enters_palette_focus_mode() {
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

    let report = dispatch_command(&mut app_state, Command::OpenFilePicker);
    assert!(report.success);
    assert_eq!(app_state.current_mode(), EditorMode::PaletteFocus);
    assert!(app_state.is_file_picker_open());
    assert!(!app_state.active_buffer_is_fuzzy_picker());

    let _ = fs::remove_dir_all(workspace_root);
}

#[test]
fn open_file_finder_command_opens_fuzzy_picker_buffer() {
    let mut app_state = AppState::new(unique_temp_path("save"));
    let workspace_root = unique_temp_dir("workspace_finder_buffer");
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
    assert_eq!(app_state.current_mode(), EditorMode::Insert);
    assert!(app_state.active_buffer_is_fuzzy_picker());
    assert_eq!(
        app_state.command_palette_mode(),
        Some(CommandPaletteMode::FilePicker)
    );
    assert!(!app_state.is_command_palette_visible());

    let _ = fs::remove_dir_all(workspace_root);
}

#[test]
fn closing_in_file_search_clears_search_highlights() {
    let mut app_state =
        AppState::from_text(unique_temp_path("close_in_file_search"), "alpha beta alpha");
    let _ = dispatch_command(
        &mut app_state,
        Command::SwitchMode(crate::core::mode::ModeEvent::EnterNormal),
    );

    let open = dispatch_command(&mut app_state, Command::OpenInFileSearch);
    assert!(open.success);
    assert_eq!(app_state.current_mode(), EditorMode::PaletteFocus);

    assert!(app_state.set_in_file_search_query("alpha"));
    assert_eq!(app_state.search_highlights().len(), 2);

    let close = dispatch_command(&mut app_state, Command::CloseFilePicker);
    assert!(close.success);
    assert_eq!(app_state.current_mode(), EditorMode::Normal);
    assert!(app_state.last_search_query().is_empty());
    assert!(app_state.search_highlights().is_empty());
}

#[test]
fn toggle_line_comment_command_comments_and_uncomments_current_line() {
    let mut app_state = AppState::from_text(
        std::path::PathBuf::from("comment_toggle.rs"),
        "let value = 1;\n",
    );
    let _ = dispatch_command(
        &mut app_state,
        Command::SwitchMode(crate::core::mode::ModeEvent::EnterNormal),
    );

    let comment = dispatch_command(&mut app_state, Command::ToggleLineComment);
    assert!(comment.success);
    assert_eq!(app_state.text_string(), "// let value = 1;\n");

    let uncomment = dispatch_command(&mut app_state, Command::ToggleLineComment);
    assert!(uncomment.success);
    assert_eq!(app_state.text_string(), "let value = 1;\n");
}

#[test]
fn toggle_selection_comment_wraps_and_unwraps_block_comment_for_rs_file() {
    let mut app_state = AppState::from_text(
        std::path::PathBuf::from("comment_selection.rs"),
        "let first = 1;\nlet second = 2;\n",
    );
    let _ = dispatch_command(
        &mut app_state,
        Command::SwitchMode(crate::core::mode::ModeEvent::EnterNormal),
    );

    let enter_visual = dispatch_command(
        &mut app_state,
        Command::SwitchMode(crate::core::mode::ModeEvent::EnterVisual),
    );
    assert!(enter_visual.success);

    let move_down = dispatch_command(&mut app_state, Command::MoveDown);
    assert!(move_down.success);

    let comment = dispatch_command(&mut app_state, Command::ToggleSelectionComment);
    assert!(comment.success);
    assert_eq!(app_state.current_mode(), EditorMode::Normal);
    assert_eq!(
        app_state.text_string(),
        "/* let first = 1;\nlet second = 2; */\n"
    );

    let _ = dispatch_command(&mut app_state, Command::MoveToFirstLine);

    let enter_visual2 = dispatch_command(
        &mut app_state,
        Command::SwitchMode(crate::core::mode::ModeEvent::EnterVisual),
    );
    assert!(enter_visual2.success);
    let move_down2 = dispatch_command(&mut app_state, Command::MoveDown);
    assert!(move_down2.success);

    let uncomment = dispatch_command(&mut app_state, Command::ToggleSelectionComment);
    assert!(uncomment.success);
    assert_eq!(app_state.current_mode(), EditorMode::Normal);
    assert_eq!(app_state.text_string(), "let first = 1;\nlet second = 2;\n");
}

#[test]
fn toggle_selection_comment_falls_back_to_line_comment_for_py_file() {
    let mut app_state =
        AppState::from_text(std::path::PathBuf::from("script.py"), "a = 1\nb = 2\n");
    let _ = dispatch_command(
        &mut app_state,
        Command::SwitchMode(crate::core::mode::ModeEvent::EnterNormal),
    );

    let enter_visual = dispatch_command(
        &mut app_state,
        Command::SwitchMode(crate::core::mode::ModeEvent::EnterVisual),
    );
    assert!(enter_visual.success);

    let move_down = dispatch_command(&mut app_state, Command::MoveDown);
    assert!(move_down.success);

    let comment = dispatch_command(&mut app_state, Command::ToggleSelectionComment);
    assert!(comment.success);
    assert_eq!(app_state.current_mode(), EditorMode::Normal);
    assert_eq!(app_state.text_string(), "# a = 1\n# b = 2\n");

    let undo = dispatch_command(&mut app_state, Command::Undo);
    assert!(undo.success);
    assert_eq!(app_state.text_string(), "a = 1\nb = 2\n");
}

#[test]
fn toggle_selection_comment_falls_back_to_hash_for_unknown_extension() {
    let mut app_state =
        AppState::from_text(std::path::PathBuf::from("scratch.xyz"), "hello\nworld\n");
    let _ = dispatch_command(
        &mut app_state,
        Command::SwitchMode(crate::core::mode::ModeEvent::EnterNormal),
    );

    let enter_visual = dispatch_command(
        &mut app_state,
        Command::SwitchMode(crate::core::mode::ModeEvent::EnterVisual),
    );
    assert!(enter_visual.success);

    let move_down = dispatch_command(&mut app_state, Command::MoveDown);
    assert!(move_down.success);

    let comment = dispatch_command(&mut app_state, Command::ToggleSelectionComment);
    assert!(comment.state_changed);
    assert_eq!(app_state.text_string(), "# hello\n# world\n");
}

#[test]
fn toggle_selection_comment_wraps_block_for_html_file() {
    let mut app_state = AppState::from_text(
        std::path::PathBuf::from("page.html"),
        "<div>\n  <p>text</p>\n</div>\n",
    );
    let _ = dispatch_command(
        &mut app_state,
        Command::SwitchMode(crate::core::mode::ModeEvent::EnterNormal),
    );

    let enter_visual = dispatch_command(
        &mut app_state,
        Command::SwitchMode(crate::core::mode::ModeEvent::EnterVisual),
    );
    assert!(enter_visual.success);
    let _ = dispatch_command(&mut app_state, Command::MoveDown);
    let _ = dispatch_command(&mut app_state, Command::MoveDown);

    let comment = dispatch_command(&mut app_state, Command::ToggleSelectionComment);
    assert!(comment.success);
    assert_eq!(
        app_state.text_string(),
        "<!-- <div>\n  <p>text</p>\n</div> -->\n"
    );

    let _ = dispatch_command(&mut app_state, Command::MoveToFirstLine);

    let enter_visual2 = dispatch_command(
        &mut app_state,
        Command::SwitchMode(crate::core::mode::ModeEvent::EnterVisual),
    );
    assert!(enter_visual2.success);
    let _ = dispatch_command(&mut app_state, Command::MoveDown);
    let _ = dispatch_command(&mut app_state, Command::MoveDown);

    let uncomment = dispatch_command(&mut app_state, Command::ToggleSelectionComment);
    assert!(uncomment.success);
    assert_eq!(app_state.text_string(), "<div>\n  <p>text</p>\n</div>\n");
}

#[test]
fn toggle_selection_comment_wraps_and_unwraps_block_for_css_file() {
    let mut app_state = AppState::from_text(
        std::path::PathBuf::from("styles.css"),
        "body {\n  color: red;\n}\n",
    );
    let _ = dispatch_command(
        &mut app_state,
        Command::SwitchMode(crate::core::mode::ModeEvent::EnterNormal),
    );

    let enter_visual = dispatch_command(
        &mut app_state,
        Command::SwitchMode(crate::core::mode::ModeEvent::EnterVisual),
    );
    assert!(enter_visual.success);
    let _ = dispatch_command(&mut app_state, Command::MoveDown);
    let _ = dispatch_command(&mut app_state, Command::MoveDown);

    let comment = dispatch_command(&mut app_state, Command::ToggleSelectionComment);
    assert!(comment.success);
    assert_eq!(app_state.text_string(), "/* body {\n  color: red;\n} */\n");

    let _ = dispatch_command(&mut app_state, Command::MoveToFirstLine);

    let enter_visual2 = dispatch_command(
        &mut app_state,
        Command::SwitchMode(crate::core::mode::ModeEvent::EnterVisual),
    );
    assert!(enter_visual2.success);
    let _ = dispatch_command(&mut app_state, Command::MoveDown);
    let _ = dispatch_command(&mut app_state, Command::MoveDown);

    let uncomment = dispatch_command(&mut app_state, Command::ToggleSelectionComment);
    assert!(uncomment.success);
    assert_eq!(app_state.text_string(), "body {\n  color: red;\n}\n");
}

#[test]
fn toggle_line_comment_ignored_for_language_without_line_syntax() {
    let mut app_state = AppState::from_text(
        std::path::PathBuf::from("styles.css"),
        "body { color: red; }\n",
    );
    let _ = dispatch_command(
        &mut app_state,
        Command::SwitchMode(crate::core::mode::ModeEvent::EnterNormal),
    );

    let comment = dispatch_command(&mut app_state, Command::ToggleLineComment);
    assert!(!comment.state_changed);
    assert_eq!(app_state.text_string(), "body { color: red; }\n");
}

#[test]
fn toggle_line_comment_uses_hash_for_txt_file() {
    let mut app_state = AppState::from_text(std::path::PathBuf::from("notes.txt"), "hello world\n");
    let _ = dispatch_command(
        &mut app_state,
        Command::SwitchMode(crate::core::mode::ModeEvent::EnterNormal),
    );

    let comment = dispatch_command(&mut app_state, Command::ToggleLineComment);
    assert!(comment.state_changed);
    assert_eq!(app_state.text_string(), "# hello world\n");

    let uncomment = dispatch_command(&mut app_state, Command::ToggleLineComment);
    assert!(uncomment.state_changed);
    assert_eq!(app_state.text_string(), "hello world\n");
}

#[test]
fn toggle_line_comment_uses_hash_for_env_dist_file() {
    let mut app_state =
        AppState::from_text(std::path::PathBuf::from("env.dist"), "DB_HOST=localhost\n");
    let _ = dispatch_command(
        &mut app_state,
        Command::SwitchMode(crate::core::mode::ModeEvent::EnterNormal),
    );

    let comment = dispatch_command(&mut app_state, Command::ToggleLineComment);
    assert!(comment.state_changed);
    assert_eq!(app_state.text_string(), "# DB_HOST=localhost\n");

    let uncomment = dispatch_command(&mut app_state, Command::ToggleLineComment);
    assert!(uncomment.state_changed);
    assert_eq!(app_state.text_string(), "DB_HOST=localhost\n");
}

#[test]
fn toggle_line_comment_noop_for_json_file() {
    let mut app_state = AppState::from_text(
        std::path::PathBuf::from("config.json"),
        "{\n  \"key\": 1\n}\n",
    );
    let _ = dispatch_command(
        &mut app_state,
        Command::SwitchMode(crate::core::mode::ModeEvent::EnterNormal),
    );

    let comment = dispatch_command(&mut app_state, Command::ToggleLineComment);
    assert!(!comment.state_changed);
    assert_eq!(app_state.text_string(), "{\n  \"key\": 1\n}\n");
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
    let _ = dispatch_command(&mut app_state, Command::OpenFilePicker);
    let _ = dispatch_command(
        &mut app_state,
        Command::FilePickerAppendQuery("phase8".to_string()),
    );
    assert!(app_state.set_command_palette_results(
        CommandPaletteMode::FilePicker,
        "phase8",
        vec![CommandPaletteItem::file_match(
            "src/netherize_phase8.rs".to_string(),
            workspace_root.join("src/netherize_phase8.rs"),
        )],
    ));

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
    let _ = dispatch_command(&mut app_state, Command::OpenFilePicker);
    let _ = dispatch_command(
        &mut app_state,
        Command::FilePickerAppendQuery("phase1-hello".to_string()),
    );
    assert!(app_state.set_command_palette_results(
        CommandPaletteMode::FilePicker,
        "phase1-hello",
        vec![CommandPaletteItem::file_match(
            "src/phase1-hello.txt".to_string(),
            old_path.clone(),
        )],
    ));

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
fn undo_restores_exact_snapshots_after_30_edit_and_save_steps() {
    let file_path = unique_temp_path("undo_30_save_steps");
    let mut app_state = AppState::new(file_path.clone());

    let enter_insert = dispatch_command(
        &mut app_state,
        Command::SwitchMode(crate::core::mode::ModeEvent::EnterInsert),
    );
    assert!(enter_insert.success);

    let mut expected_states = vec![app_state.text_string()];

    for step in 0..30usize {
        let report = if step % 2 == 0 {
            let text = format!("step_{step:02}");
            dispatch_command(&mut app_state, Command::InsertText(text))
        } else {
            dispatch_command(&mut app_state, Command::Newline)
        };
        assert!(report.success, "edit step {step} should succeed");
        assert!(report.state_changed, "edit step {step} should change state");

        let save = dispatch_command(&mut app_state, Command::SaveFile);
        assert!(save.success, "save step {step} should succeed");

        expected_states.push(app_state.text_string());
    }

    let exit_insert = dispatch_command(
        &mut app_state,
        Command::SwitchMode(crate::core::mode::ModeEvent::EnterNormal),
    );
    assert!(exit_insert.success);

    assert_eq!(expected_states.len(), 31, "initial + 30 edit snapshots");

    for undo_idx in (1..expected_states.len()).rev() {
        let undo = dispatch_command(&mut app_state, Command::Undo);
        assert!(undo.success, "undo at snapshot {undo_idx} should succeed");
        assert!(
            undo.state_changed,
            "undo at snapshot {undo_idx} should change state"
        );
        assert_eq!(
            app_state.text_string(),
            expected_states[undo_idx - 1],
            "undo should restore exact snapshot for step {}",
            undo_idx - 1
        );
    }

    let extra_undo = dispatch_command(&mut app_state, Command::Undo);
    assert!(extra_undo.success);
    assert!(
        !extra_undo.state_changed,
        "undo past beginning should no-op"
    );
    assert_eq!(app_state.text_string(), expected_states[0]);

    let _ = fs::remove_file(file_path);
}

#[test]
fn save_does_not_write_previewed_history_state_back_to_disk() {
    let file_path = unique_temp_path("save_not_preview_state");
    let mut app_state = AppState::new(file_path.clone());

    let _ = dispatch_command(
        &mut app_state,
        Command::SwitchMode(crate::core::mode::ModeEvent::EnterInsert),
    );
    let _ = dispatch_command(&mut app_state, Command::InsertText("old".to_string()));
    let _ = dispatch_command(&mut app_state, Command::SaveFile);
    let _ = dispatch_command(&mut app_state, Command::InsertText(" new".to_string()));
    let _ = dispatch_command(&mut app_state, Command::SaveFile);

    assert!(app_state.begin_file_history_preview_session());
    assert!(app_state.preview_file_history_index(0));
    assert_eq!(app_state.text_string(), "old");

    let save = dispatch_command(&mut app_state, Command::SaveFile);
    assert!(save.success);

    let disk_text = fs::read_to_string(&file_path).expect("read saved file");
    assert_eq!(disk_text, "old new");
    assert_eq!(app_state.text_string(), "old new");

    let _ = fs::remove_file(file_path);
}

#[test]
fn toggle_terminal_command_closes_panel_when_pressed_again() {
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
fn toggle_terminal_closes_existing_panel_even_when_editor_has_focus() {
    let mut app_state = AppState::new(unique_temp_path("save"));
    let _ = dispatch_command(
        &mut app_state,
        Command::SwitchMode(crate::core::mode::ModeEvent::EnterNormal),
    );

    let _ = dispatch_command(&mut app_state, Command::ToggleTerminal);
    let _ = dispatch_command(
        &mut app_state,
        Command::SwitchMode(crate::core::mode::ModeEvent::ExitFocus),
    );
    assert_eq!(app_state.current_mode(), EditorMode::Normal);
    assert!(app_state.is_terminal_panel_open());

    let report = dispatch_command(&mut app_state, Command::ToggleTerminal);
    assert!(report.success);
    assert_eq!(app_state.current_mode(), EditorMode::Normal);
    assert!(!app_state.is_terminal_panel_open());
}

// ── Per-Buffer Undo Isolation Tests ────────────────────────────────────────

/// Mô phỏng chính xác bug cross-tab: edit+save Tab A, switch sang B, edit+save B,
/// quay lại A bấm Undo → phải khôi phục đúng nội dung A, B không bị ảnh hưởng.
/// (Buffer text được reload từ disk khi switch tab nên cần save trước khi switch.)
#[test]
fn undo_is_isolated_per_buffer() {
    let path_a = unique_temp_path("cross_buf_a");
    let path_b = unique_temp_path("cross_buf_b");
    fs::write(&path_a, "File A").expect("write file A");
    fs::write(&path_b, "File B").expect("write file B");

    let mut app = AppState::new(unique_temp_path("cross_buf_save"));

    // Open buffer A, append " - Edit 1", save.
    app.open_file(path_a.clone()).expect("open A");
    let _ = dispatch_command(&mut app, Command::SwitchMode(ModeEvent::EnterNormal));
    let _ = dispatch_command(&mut app, Command::AppendAtLineEnd);
    for ch in " - Edit 1".chars() {
        let _ = dispatch_command(&mut app, Command::InsertChar(ch));
    }
    let _ = dispatch_command(&mut app, Command::SwitchMode(ModeEvent::EnterNormal));
    assert_eq!(app.text_string(), "File A - Edit 1");
    let _ = dispatch_command(&mut app, Command::SaveFile);

    // Switch to buffer B, delete the last character 'B', save.
    app.open_file(path_b.clone()).expect("open B");
    assert_eq!(app.text_string(), "File B");
    let _ = dispatch_command(&mut app, Command::SwitchMode(ModeEvent::EnterNormal));
    let _ = dispatch_command(&mut app, Command::MoveToLineEnd);
    let _ = dispatch_command(&mut app, Command::DeleteChar);
    assert_eq!(app.text_string(), "File ");
    let _ = dispatch_command(&mut app, Command::SaveFile);

    // Return to buffer A and undo one transaction.
    app.open_file(path_a.clone()).expect("re-open A");
    assert_eq!(
        app.text_string(),
        "File A - Edit 1",
        "buffer A must load saved content"
    );
    let undo = dispatch_command(&mut app, Command::Undo);
    assert!(undo.success, "undo must succeed on buffer A");
    assert!(undo.state_changed, "undo must change state");
    assert_eq!(
        app.text_string(),
        "File A",
        "undo must restore buffer A to pre-edit state"
    );

    // Verify buffer B is untouched — its undo stack must be independent.
    app.open_file(path_b.clone()).expect("re-open B");
    assert_eq!(
        app.text_string(),
        "File ",
        "buffer B must still show its own saved state"
    );
    let b_undo = dispatch_command(&mut app, Command::Undo);
    assert!(b_undo.success);
    assert!(
        b_undo.state_changed,
        "buffer B must have its own undo entry"
    );
    assert_eq!(
        app.text_string(),
        "File B",
        "undo on buffer B must restore 'File B'"
    );

    let _ = fs::remove_file(&path_a);
    let _ = fs::remove_file(&path_b);
}

/// Undo một lần phải xóa toàn bộ word "Hello" được gõ trong một insert session.
#[test]
fn undo_removes_entire_insert_session_word() {
    let mut app = AppState::new(unique_temp_path("session_word_undo"));

    let _ = dispatch_command(&mut app, Command::SwitchMode(ModeEvent::EnterInsert));
    for ch in "Hello".chars() {
        let _ = dispatch_command(&mut app, Command::InsertChar(ch));
    }
    assert_eq!(app.text_string(), "Hello");

    // Exit insert → commits the transaction.
    let _ = dispatch_command(&mut app, Command::SwitchMode(ModeEvent::EnterNormal));

    let undo = dispatch_command(&mut app, Command::Undo);
    assert!(undo.success);
    assert!(undo.state_changed);
    assert_eq!(
        app.text_string(),
        "",
        "single undo must remove the whole word typed in one session"
    );
}

/// Paste nhiều dòng rồi Undo phải khôi phục lại đúng số dòng cũ.
#[test]
fn paste_multiline_undo_restores_line_count() {
    let mut app = AppState::new(unique_temp_path("paste_multiline_undo"));
    let mut clipboard = MockClipboard::default();

    let _ = dispatch_command(&mut app, Command::SwitchMode(ModeEvent::EnterInsert));
    let _ = dispatch_command(
        &mut app,
        Command::InsertText("line1\nline2\nline3".to_string()),
    );
    assert_eq!(app.text_string(), "line1\nline2\nline3");
    let lines_before = app.text_string().lines().count();

    // Commit the insert transaction.
    let _ = dispatch_command(&mut app, Command::SwitchMode(ModeEvent::EnterNormal));

    // Paste a multi-line block via clipboard.
    clipboard.text = "A\nB\nC".to_string();
    let _ = dispatch_command(&mut app, Command::MoveToFirstLine);
    let _ = dispatch_command_with_clipboard(
        &mut app,
        Command::PasteSystemClipboard,
        Some(&mut clipboard),
    );
    let lines_after_paste = app.text_string().lines().count();
    assert!(lines_after_paste > lines_before, "paste should add lines");

    // Commit if still in insert mode.
    if app.current_mode() == EditorMode::Insert {
        let _ = dispatch_command(&mut app, Command::SwitchMode(ModeEvent::EnterNormal));
    }

    let undo = dispatch_command(&mut app, Command::Undo);
    assert!(undo.success);
    assert!(undo.state_changed);
    assert_eq!(
        app.text_string().lines().count(),
        lines_before,
        "undo must restore original line count"
    );
}

/// Ctrl+A select-all, delete, Undo phải khôi phục nguyên vẹn kể cả emoji (Unicode multi-byte).
#[test]
fn undo_after_select_all_delete_restores_unicode_content() {
    let original = "Hello 🌍 world\nwith emoji 🎉\nthird line";
    let mut app = AppState::from_text(unique_temp_path("unicode_undo"), original);

    assert_eq!(app.text_string(), original);

    // Enter normal, select all, delete selection.
    let _ = dispatch_command(&mut app, Command::SwitchMode(ModeEvent::EnterNormal));
    let _ = dispatch_command(&mut app, Command::SwitchMode(ModeEvent::EnterVisual));
    let _ = dispatch_command(&mut app, Command::MoveToLastLine);
    let _ = dispatch_command(&mut app, Command::MoveToLineEnd);
    let _ = dispatch_command(&mut app, Command::DeleteSelection);
    let _ = dispatch_command(&mut app, Command::SwitchMode(ModeEvent::EnterNormal));
    assert!(
        app.text_string().len() < original.len(),
        "text should be shorter after delete"
    );

    let undo = dispatch_command(&mut app, Command::Undo);
    assert!(undo.success);
    assert!(undo.state_changed);
    assert_eq!(
        app.text_string(),
        original,
        "undo must restore exact unicode content including emoji"
    );
}

#[test]
fn visual_mode_star_search_sets_highlights_and_exits_to_normal() {
    let mut app_state = AppState::from_text(
        unique_temp_path("visual_star_search"),
        "alpha beta alpha gamma alpha",
    );
    let _ = dispatch_command(&mut app_state, Command::SwitchMode(ModeEvent::EnterNormal));

    // Move to first "alpha" and select it in visual mode
    let _ = dispatch_command(&mut app_state, Command::MoveToLineStart);
    let _ = dispatch_command(&mut app_state, Command::SwitchMode(ModeEvent::EnterVisual));
    // Move to end of word (not forward, which includes space)
    for _ in 0..4 {
        let _ = dispatch_command(&mut app_state, Command::MoveRight);
    }

    assert_eq!(app_state.current_mode(), EditorMode::Visual);
    let selection = app_state.visual_selection_text();
    assert_eq!(selection, Some("alpha".to_string()));

    // Press * to search for selected text
    let star = dispatch_command(&mut app_state, Command::SearchWordUnderCursor);
    assert!(star.success);

    // Should exit to Normal mode
    assert_eq!(app_state.current_mode(), EditorMode::Normal);

    // Search query should be set
    assert_eq!(app_state.last_search_query(), "alpha");

    // Search highlights should be present (3 occurrences of "alpha")
    assert_eq!(app_state.search_highlights().len(), 3);

    // n should work to go to next match
    let next = dispatch_command(&mut app_state, Command::SearchNext);
    assert!(next.success);
    assert!(next.state_changed);

    // Manually clearing search highlights should clear them
    let clear = dispatch_command(&mut app_state, Command::ClearSearchHighlights);
    assert!(clear.success);
    assert_eq!(app_state.search_highlights().len(), 0);
    assert_eq!(app_state.last_search_query(), "");

    // After clearing, n should not work
    let next_after_clear = dispatch_command(&mut app_state, Command::SearchNext);
    assert!(!next_after_clear.state_changed);
}

#[test]
fn visual_mode_star_search_persists_after_mode_exit() {
    let mut app_state = AppState::from_text(
        unique_temp_path("visual_star_persist"),
        "foo bar foo baz foo",
    );
    let _ = dispatch_command(&mut app_state, Command::SwitchMode(ModeEvent::EnterNormal));

    // Select "foo" in visual mode
    let _ = dispatch_command(&mut app_state, Command::MoveToLineStart);
    let _ = dispatch_command(&mut app_state, Command::SwitchMode(ModeEvent::EnterVisual));
    for _ in 0..2 {
        let _ = dispatch_command(&mut app_state, Command::MoveRight);
    }

    // Press * to search
    let star = dispatch_command(&mut app_state, Command::SearchWordUnderCursor);
    assert!(star.success);
    assert_eq!(app_state.current_mode(), EditorMode::Normal);

    // Search should persist
    assert_eq!(app_state.last_search_query(), "foo");
    assert_eq!(app_state.search_highlights().len(), 3);

    // n should work multiple times
    let n1 = dispatch_command(&mut app_state, Command::SearchNext);
    assert!(n1.state_changed);
    let n2 = dispatch_command(&mut app_state, Command::SearchNext);
    assert!(n2.state_changed);
    let n3 = dispatch_command(&mut app_state, Command::SearchNext);
    assert!(n3.state_changed);

    // Search should still be active
    assert_eq!(app_state.last_search_query(), "foo");
    assert_eq!(app_state.search_highlights().len(), 3);
}

#[test]
fn yank_flash_triggered_on_yank_commands() {
    let mut app_state = AppState::from_text(
        unique_temp_path("yank_flash"),
        "Hello World\nLine two\nLine three",
    );
    let mut clipboard = MockClipboard::default();

    // Test YankCurrentLine (yy equivalent)
    let report = dispatch_command_with_clipboard(
        &mut app_state,
        Command::YankCurrentLine,
        Some(&mut clipboard),
    );
    assert!(report.success);
    let flash_range = app_state.yank_flash_range();
    assert!(
        flash_range.is_some(),
        "YankCurrentLine should trigger yank flash range"
    );
    let (start, end) = flash_range.unwrap();
    assert_eq!(start, 0);
    // newline is included in current line range
    assert_eq!(end, 12);

    let mut word_end_state =
        AppState::from_text(unique_temp_path("yank_flash_word_end"), "hello world");
    let _ = dispatch_command(
        &mut word_end_state,
        Command::SwitchMode(ModeEvent::EnterNormal),
    );
    let _ = dispatch_command(&mut word_end_state, Command::MoveRight);
    let _ = dispatch_command(&mut word_end_state, Command::MoveRight);
    let report = dispatch_command_with_clipboard(
        &mut word_end_state,
        Command::YankToWordEnd,
        Some(&mut clipboard),
    );
    assert!(report.success);
    assert_eq!(word_end_state.yank_flash_range(), Some((2, 5)));

    let mut motion_state =
        AppState::from_text(unique_temp_path("yank_flash_motion"), "hello world");
    let _ = dispatch_command(
        &mut motion_state,
        Command::SwitchMode(ModeEvent::EnterNormal),
    );
    let report = dispatch_command_with_clipboard(
        &mut motion_state,
        Command::Operate {
            op: Operator::Yank,
            target: OperationTarget::Motion(Motion::WordForward),
        },
        Some(&mut clipboard),
    );
    assert!(report.success);
    assert!(
        motion_state.yank_flash_range().is_some(),
        "y{{motion}} should trigger yank flash range"
    );

    // Move to visual mode and select Hello
    let _ = dispatch_command(&mut app_state, Command::SwitchMode(ModeEvent::EnterVisual));
    for _ in 0..4 {
        let _ = dispatch_command(&mut app_state, Command::MoveRight);
    }
    // Now range is 0 to 5
    let report2 = dispatch_command_with_clipboard(
        &mut app_state,
        Command::YankSelection,
        Some(&mut clipboard),
    );
    assert!(report2.success);
    let flash_range2 = app_state.yank_flash_range();
    assert!(
        flash_range2.is_some(),
        "YankSelection should trigger yank flash range"
    );
    let (start2, end2) = flash_range2.unwrap();
    assert_eq!(start2, 0);
    assert_eq!(end2, 5);
}
