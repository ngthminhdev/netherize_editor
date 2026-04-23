use std::{
    fs,
    time::{SystemTime, UNIX_EPOCH},
};

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
        commands::Command,
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

    let enter_insert = dispatch_command(&mut app_state, Command::SwitchMode(ModeEvent::EnterInsert));
    assert!(enter_insert.success);

    let pair = dispatch_command(&mut app_state, Command::InsertChar('{'));
    assert!(pair.success);
    assert_eq!(app_state.text_string(), "{}");

    let exit_after_pair = dispatch_command(&mut app_state, Command::SwitchMode(ModeEvent::EnterNormal));
    assert!(exit_after_pair.success);

    let undo_pair = dispatch_command(&mut app_state, Command::Undo);
    assert!(undo_pair.success);
    assert!(undo_pair.state_changed);
    assert_eq!(app_state.text_string(), "");

    let reenter_insert = dispatch_command(&mut app_state, Command::SwitchMode(ModeEvent::EnterInsert));
    assert!(reenter_insert.success);
    let _ = dispatch_command(&mut app_state, Command::InsertChar('{'));
    let smart_enter = dispatch_command(&mut app_state, Command::Newline);
    assert!(smart_enter.success);
    assert_eq!(app_state.text_string(), "{\n    \n}");

    let exit_after_newline = dispatch_command(&mut app_state, Command::SwitchMode(ModeEvent::EnterNormal));
    assert!(exit_after_newline.success);

    let undo_newline = dispatch_command(&mut app_state, Command::Undo);
    assert!(undo_newline.success);
    assert!(undo_newline.state_changed);
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
fn toggle_selection_comment_exits_visual_mode_and_undo_restores_buffer() {
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
        "// let first = 1;\n// let second = 2;\n"
    );

    let undo = dispatch_command(&mut app_state, Command::Undo);
    assert!(undo.success);
    assert_eq!(app_state.text_string(), "let first = 1;\nlet second = 2;\n");
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
