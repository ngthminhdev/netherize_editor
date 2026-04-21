use std::path::PathBuf;

use crate::core::{commands::Command, mode::ModeEvent};

// ── Editor movement & editing ────────────────────────────────────────────────
pub const MOVE_LEFT: &str = "editor.move_left";
pub const MOVE_RIGHT: &str = "editor.move_right";
pub const MOVE_UP: &str = "editor.move_up";
pub const MOVE_DOWN: &str = "editor.move_down";
pub const MOVE_WORD_FORWARD: &str = "editor.move_word_forward";
pub const MOVE_WORD_BACKWARD: &str = "editor.move_word_backward";
pub const MOVE_WORD_END: &str = "editor.move_word_end";
pub const MOVE_TO_LINE_START: &str = "editor.move_to_line_start";
pub const MOVE_TO_LINE_END: &str = "editor.move_to_line_end";
pub const MOVE_TO_FIRST_NON_WHITESPACE: &str = "editor.move_to_first_non_whitespace";
pub const MOVE_TO_FIRST_LINE: &str = "editor.move_to_first_line";
pub const MOVE_TO_LAST_LINE: &str = "editor.move_to_last_line";
pub const SCROLL_HALF_PAGE_UP: &str = "editor.scroll_half_page_up";
pub const SCROLL_HALF_PAGE_DOWN: &str = "editor.scroll_half_page_down";
pub const CENTER_CURSOR_LINE: &str = "editor.center_cursor_line";
pub const BACKSPACE: &str = "editor.backspace";
pub const NEWLINE: &str = "editor.newline";
pub const INSERT_LINE_BELOW: &str = "editor.insert_line_below";
pub const INSERT_LINE_ABOVE: &str = "editor.insert_line_above";
pub const INSERT_AT_LINE_START: &str = "editor.insert_at_line_start";
pub const APPEND_AT_LINE_END: &str = "editor.append_at_line_end";
pub const APPEND_AFTER_CURSOR: &str = "editor.append_after_cursor";
pub const SUBSTITUTE_LINE: &str = "editor.substitute_line";
pub const DELETE_CHAR: &str = "editor.delete_char";
pub const DELETE_SELECTION: &str = "editor.delete_selection";
pub const DELETE_CURRENT_LINE: &str = "editor.delete_current_line";
pub const DELETE_WORD_FORWARD: &str = "editor.delete_word_forward";
pub const DELETE_WORD_BACKWARD: &str = "editor.delete_word_backward";
pub const CHANGE_SELECTION: &str = "editor.change_selection";
pub const CHANGE_WORD_FORWARD: &str = "editor.change_word_forward";
pub const CHANGE_WORD_BACKWARD: &str = "editor.change_word_backward";
pub const UNDO: &str = "editor.undo";
pub const REDO: &str = "editor.redo";
pub const SAVE_FILE: &str = "editor.save_file";
pub const OPEN_FILE: &str = "editor.open_file";

// ── Mode transitions ──────────────────────────────────────────────────────────
pub const ENTER_NORMAL: &str = "mode.enter_normal";
pub const ENTER_INSERT: &str = "mode.enter_insert";
pub const ENTER_VISUAL: &str = "mode.enter_visual";
pub const ENTER_VISUAL_LINE: &str = "mode.enter_visual_line";
pub const EXIT_FOCUS: &str = "mode.exit_focus";

// ── App-level UI ──────────────────────────────────────────────────────────────
pub const TOGGLE_TERMINAL: &str = "app.toggle_terminal";
pub const TOGGLE_EXPLORER: &str = "app.toggle_explorer";
pub const OPEN_FILE_PICKER: &str = "app.open_file_picker";
pub const OPEN_FILE_FINDER: &str = "app.open_file_finder";
pub const OPEN_COMMAND_PALETTE: &str = "app.open_command_palette";
pub const OPEN_VIM_COMMAND: &str = "app.open_vim_command";
pub const OPEN_WORKSPACE_SYMBOLS: &str = "app.open_workspace_symbols";
pub const SEARCH_IN_FILES: &str = "app.search_in_files";

// ── Workbench focus navigation ────────────────────────────────────────────────
pub const FOCUS_EDITOR: &str = "app.focus_editor";
pub const FOCUS_EXPLORER: &str = "app.focus_explorer";
pub const FOCUS_TERMINAL: &str = "app.focus_terminal";
pub const FOCUS_INSPECTOR: &str = "app.focus_inspector";
pub const FOCUS_LEFT: &str = "app.focus_left";
pub const FOCUS_RIGHT: &str = "app.focus_right";
pub const FOCUS_UP: &str = "app.focus_up";
pub const FOCUS_DOWN: &str = "app.focus_down";
pub const MOVE_FOCUS_CYCLE: &str = "app.move_focus_cycle";
pub const FOCUS_BACK: &str = "app.focus_back";

// ── Panel tabs ────────────────────────────────────────────────────────────────
pub const NEXT_PANEL_TAB: &str = "app.next_panel_tab";
pub const PREV_PANEL_TAB: &str = "app.prev_panel_tab";

// ── Buffer management ────────────────────────────────────────────────────────
pub const BUFFER_NEW: &str = "buffer.new";
pub const BUFFER_NEXT: &str = "buffer.next";
pub const BUFFER_PREV: &str = "buffer.prev";
pub const BUFFER_CLOSE_CURRENT: &str = "buffer.close_current";

// ── Explorer surface ──────────────────────────────────────────────────────────
pub const EXPLORER_MOVE_UP: &str = "explorer.move_up";
pub const EXPLORER_MOVE_DOWN: &str = "explorer.move_down";
pub const EXPLORER_COLLAPSE_OR_PARENT: &str = "explorer.collapse_or_parent";
pub const EXPLORER_EXPAND_OR_CHILD: &str = "explorer.expand_or_child";
pub const EXPLORER_TOGGLE_OR_OPEN: &str = "explorer.toggle_or_open";
// Legacy command IDs.
pub const EXPLORER_EXPAND_COLLAPSE: &str = "explorer.expand_collapse";
pub const EXPLORER_OPEN_FILE: &str = "explorer.open_file";

// ── File picker ───────────────────────────────────────────────────────────────
pub const FILE_PICKER_CONFIRM: &str = "file_picker.confirm";
pub const FILE_PICKER_CLOSE: &str = "file_picker.close";
pub const FILE_PICKER_SELECT_NEXT: &str = "file_picker.select_next";
pub const FILE_PICKER_SELECT_PREV: &str = "file_picker.select_prev";
pub const FILE_PICKER_BACKSPACE: &str = "file_picker.backspace";

pub const ALL_IDS: &[&str] = &[
    MOVE_LEFT,
    MOVE_RIGHT,
    MOVE_UP,
    MOVE_DOWN,
    MOVE_WORD_FORWARD,
    MOVE_WORD_BACKWARD,
    MOVE_WORD_END,
    MOVE_TO_LINE_START,
    MOVE_TO_LINE_END,
    MOVE_TO_FIRST_NON_WHITESPACE,
    MOVE_TO_FIRST_LINE,
    MOVE_TO_LAST_LINE,
    SCROLL_HALF_PAGE_UP,
    SCROLL_HALF_PAGE_DOWN,
    CENTER_CURSOR_LINE,
    BACKSPACE,
    NEWLINE,
    INSERT_LINE_BELOW,
    INSERT_LINE_ABOVE,
    INSERT_AT_LINE_START,
    APPEND_AT_LINE_END,
    APPEND_AFTER_CURSOR,
    SUBSTITUTE_LINE,
    DELETE_CHAR,
    DELETE_SELECTION,
    DELETE_CURRENT_LINE,
    DELETE_WORD_FORWARD,
    DELETE_WORD_BACKWARD,
    CHANGE_SELECTION,
    CHANGE_WORD_FORWARD,
    CHANGE_WORD_BACKWARD,
    UNDO,
    REDO,
    SAVE_FILE,
    OPEN_FILE,
    ENTER_NORMAL,
    ENTER_INSERT,
    ENTER_VISUAL,
    ENTER_VISUAL_LINE,
    EXIT_FOCUS,
    TOGGLE_TERMINAL,
    TOGGLE_EXPLORER,
    OPEN_FILE_PICKER,
    OPEN_FILE_FINDER,
    OPEN_COMMAND_PALETTE,
    OPEN_VIM_COMMAND,
    OPEN_WORKSPACE_SYMBOLS,
    SEARCH_IN_FILES,
    FOCUS_EDITOR,
    FOCUS_EXPLORER,
    FOCUS_TERMINAL,
    FOCUS_INSPECTOR,
    FOCUS_LEFT,
    FOCUS_RIGHT,
    FOCUS_UP,
    FOCUS_DOWN,
    MOVE_FOCUS_CYCLE,
    FOCUS_BACK,
    NEXT_PANEL_TAB,
    PREV_PANEL_TAB,
    BUFFER_NEW,
    BUFFER_NEXT,
    BUFFER_PREV,
    BUFFER_CLOSE_CURRENT,
    EXPLORER_MOVE_UP,
    EXPLORER_MOVE_DOWN,
    EXPLORER_COLLAPSE_OR_PARENT,
    EXPLORER_EXPAND_OR_CHILD,
    EXPLORER_TOGGLE_OR_OPEN,
    EXPLORER_EXPAND_COLLAPSE,
    EXPLORER_OPEN_FILE,
    FILE_PICKER_CONFIRM,
    FILE_PICKER_CLOSE,
    FILE_PICKER_SELECT_NEXT,
    FILE_PICKER_SELECT_PREV,
    FILE_PICKER_BACKSPACE,
];

pub fn is_valid(id: &str) -> bool {
    ALL_IDS.contains(&id)
}

pub fn parse(id: &str, open_file_path: Option<&std::path::Path>) -> Option<Command> {
    match id {
        MOVE_LEFT => Some(Command::MoveLeft),
        MOVE_RIGHT => Some(Command::MoveRight),
        MOVE_UP => Some(Command::MoveUp),
        MOVE_DOWN => Some(Command::MoveDown),
        MOVE_WORD_FORWARD => Some(Command::MoveWordForward),
        MOVE_WORD_BACKWARD => Some(Command::MoveWordBackward),
        MOVE_WORD_END => Some(Command::MoveWordEnd),
        MOVE_TO_LINE_START => Some(Command::MoveToLineStart),
        MOVE_TO_LINE_END => Some(Command::MoveToLineEnd),
        MOVE_TO_FIRST_NON_WHITESPACE => Some(Command::MoveToFirstNonWhitespace),
        MOVE_TO_FIRST_LINE => Some(Command::MoveToFirstLine),
        MOVE_TO_LAST_LINE => Some(Command::MoveToLastLine),
        SCROLL_HALF_PAGE_UP => Some(Command::ScrollHalfPageUp),
        SCROLL_HALF_PAGE_DOWN => Some(Command::ScrollHalfPageDown),
        CENTER_CURSOR_LINE => Some(Command::CenterCursorLine),
        BACKSPACE => Some(Command::Backspace),
        NEWLINE => Some(Command::Newline),
        INSERT_LINE_BELOW => Some(Command::InsertLineBelow),
        INSERT_LINE_ABOVE => Some(Command::InsertLineAbove),
        INSERT_AT_LINE_START => Some(Command::InsertAtLineStart),
        APPEND_AT_LINE_END => Some(Command::AppendAtLineEnd),
        APPEND_AFTER_CURSOR => Some(Command::AppendAfterCursor),
        SUBSTITUTE_LINE => Some(Command::SubstituteLine),
        DELETE_CHAR => Some(Command::DeleteChar),
        DELETE_SELECTION => Some(Command::DeleteSelection),
        DELETE_CURRENT_LINE => Some(Command::DeleteCurrentLine),
        DELETE_WORD_FORWARD => Some(Command::DeleteWordForward),
        DELETE_WORD_BACKWARD => Some(Command::DeleteWordBackward),
        CHANGE_SELECTION => Some(Command::ChangeSelection),
        CHANGE_WORD_FORWARD => Some(Command::ChangeWordForward),
        CHANGE_WORD_BACKWARD => Some(Command::ChangeWordBackward),
        UNDO => Some(Command::Undo),
        REDO => Some(Command::Redo),
        SAVE_FILE => Some(Command::SaveFile),
        OPEN_FILE => Some(Command::OpenFile(
            open_file_path.map(PathBuf::from).unwrap_or_default(),
        )),
        ENTER_NORMAL => Some(Command::SwitchMode(ModeEvent::EnterNormal)),
        ENTER_INSERT => Some(Command::SwitchMode(ModeEvent::EnterInsert)),
        ENTER_VISUAL => Some(Command::SwitchMode(ModeEvent::EnterVisual)),
        ENTER_VISUAL_LINE => Some(Command::EnterVisualLine),
        EXIT_FOCUS => Some(Command::SwitchMode(ModeEvent::ExitFocus)),
        TOGGLE_TERMINAL => Some(Command::ToggleTerminal),
        TOGGLE_EXPLORER => Some(Command::ToggleExplorer),
        OPEN_FILE_PICKER => Some(Command::OpenFilePicker),
        OPEN_FILE_FINDER => Some(Command::OpenFileFinder),
        OPEN_COMMAND_PALETTE => Some(Command::OpenCommandPalette),
        OPEN_VIM_COMMAND => Some(Command::OpenVimCommand),
        OPEN_WORKSPACE_SYMBOLS => Some(Command::OpenWorkspaceSymbols),
        SEARCH_IN_FILES => Some(Command::SearchInFiles),
        FOCUS_EDITOR => Some(Command::FocusEditor),
        FOCUS_EXPLORER => Some(Command::FocusExplorer),
        FOCUS_TERMINAL => Some(Command::FocusTerminal),
        FOCUS_INSPECTOR => Some(Command::FocusInspector),
        FOCUS_LEFT => Some(Command::FocusLeft),
        FOCUS_RIGHT => Some(Command::FocusRight),
        FOCUS_UP => Some(Command::FocusUp),
        FOCUS_DOWN => Some(Command::FocusDown),
        MOVE_FOCUS_CYCLE => Some(Command::MoveFocusCycle),
        FOCUS_BACK => Some(Command::FocusBack),
        NEXT_PANEL_TAB => Some(Command::NextPanelTab),
        PREV_PANEL_TAB => Some(Command::PrevPanelTab),
        BUFFER_NEW => Some(Command::BufferNew),
        BUFFER_NEXT => Some(Command::BufferNext),
        BUFFER_PREV => Some(Command::BufferPrev),
        BUFFER_CLOSE_CURRENT => Some(Command::BufferCloseCurrent),
        EXPLORER_MOVE_UP => Some(Command::ExplorerMoveUp),
        EXPLORER_MOVE_DOWN => Some(Command::ExplorerMoveDown),
        EXPLORER_COLLAPSE_OR_PARENT => Some(Command::ExplorerCollapseOrParent),
        EXPLORER_EXPAND_OR_CHILD => Some(Command::ExplorerExpandOrChild),
        EXPLORER_TOGGLE_OR_OPEN => Some(Command::ExplorerToggleOrOpen),
        EXPLORER_EXPAND_COLLAPSE => Some(Command::ExplorerExpandCollapse),
        EXPLORER_OPEN_FILE => Some(Command::ExplorerOpenFile),
        FILE_PICKER_CONFIRM => Some(Command::FilePickerConfirmSelection),
        FILE_PICKER_CLOSE => Some(Command::CloseFilePicker),
        FILE_PICKER_SELECT_NEXT => Some(Command::FilePickerSelectNext),
        FILE_PICKER_SELECT_PREV => Some(Command::FilePickerSelectPrev),
        FILE_PICKER_BACKSPACE => Some(Command::FilePickerBackspaceQuery),
        _ => None,
    }
}
