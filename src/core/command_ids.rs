use std::path::PathBuf;

use crate::core::{commands::Command, mode::ModeEvent};

// ── Editor movement & editing ────────────────────────────────────────────────
pub const MOVE_LEFT: &str = "editor.move_left";
pub const MOVE_RIGHT: &str = "editor.move_right";
pub const MOVE_UP: &str = "editor.move_up";
pub const MOVE_DOWN: &str = "editor.move_down";
pub const BACKSPACE: &str = "editor.backspace";
pub const NEWLINE: &str = "editor.newline";
pub const SAVE_FILE: &str = "editor.save_file";
pub const OPEN_FILE: &str = "editor.open_file";

// ── Mode transitions ──────────────────────────────────────────────────────────
pub const ENTER_NORMAL: &str = "mode.enter_normal";
pub const ENTER_INSERT: &str = "mode.enter_insert";
pub const ENTER_VISUAL: &str = "mode.enter_visual";
pub const EXIT_FOCUS: &str = "mode.exit_focus";

// ── App-level UI ──────────────────────────────────────────────────────────────
pub const TOGGLE_TERMINAL: &str = "app.toggle_terminal";
pub const TOGGLE_EXPLORER: &str = "app.toggle_explorer";
pub const OPEN_FILE_FINDER: &str = "app.open_file_finder";
pub const OPEN_COMMAND_PALETTE: &str = "app.open_command_palette";

// ── Workbench focus navigation ────────────────────────────────────────────────
pub const FOCUS_EDITOR: &str = "app.focus_editor";
pub const FOCUS_EXPLORER: &str = "app.focus_explorer";
pub const FOCUS_TERMINAL: &str = "app.focus_terminal";
pub const FOCUS_INSPECTOR: &str = "app.focus_inspector";
pub const MOVE_FOCUS_CYCLE: &str = "app.move_focus_cycle";
pub const FOCUS_BACK: &str = "app.focus_back";

// ── Panel tabs ────────────────────────────────────────────────────────────────
pub const NEXT_PANEL_TAB: &str = "app.next_panel_tab";
pub const PREV_PANEL_TAB: &str = "app.prev_panel_tab";

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
    BACKSPACE,
    NEWLINE,
    SAVE_FILE,
    OPEN_FILE,
    ENTER_NORMAL,
    ENTER_INSERT,
    ENTER_VISUAL,
    EXIT_FOCUS,
    TOGGLE_TERMINAL,
    TOGGLE_EXPLORER,
    OPEN_FILE_FINDER,
    OPEN_COMMAND_PALETTE,
    FOCUS_EDITOR,
    FOCUS_EXPLORER,
    FOCUS_TERMINAL,
    FOCUS_INSPECTOR,
    MOVE_FOCUS_CYCLE,
    FOCUS_BACK,
    NEXT_PANEL_TAB,
    PREV_PANEL_TAB,
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
        BACKSPACE => Some(Command::Backspace),
        NEWLINE => Some(Command::Newline),
        SAVE_FILE => Some(Command::SaveFile),
        OPEN_FILE => Some(Command::OpenFile(
            open_file_path.map(PathBuf::from).unwrap_or_default(),
        )),
        ENTER_NORMAL => Some(Command::SwitchMode(ModeEvent::EnterNormal)),
        ENTER_INSERT => Some(Command::SwitchMode(ModeEvent::EnterInsert)),
        ENTER_VISUAL => Some(Command::SwitchMode(ModeEvent::EnterVisual)),
        EXIT_FOCUS => Some(Command::SwitchMode(ModeEvent::ExitFocus)),
        TOGGLE_TERMINAL => Some(Command::ToggleTerminal),
        TOGGLE_EXPLORER => Some(Command::ToggleExplorer),
        OPEN_FILE_FINDER => Some(Command::OpenFileFinder),
        OPEN_COMMAND_PALETTE => Some(Command::OpenCommandPalette),
        FOCUS_EDITOR => Some(Command::FocusEditor),
        FOCUS_EXPLORER => Some(Command::FocusExplorer),
        FOCUS_TERMINAL => Some(Command::FocusTerminal),
        FOCUS_INSPECTOR => Some(Command::FocusInspector),
        MOVE_FOCUS_CYCLE => Some(Command::MoveFocusCycle),
        FOCUS_BACK => Some(Command::FocusBack),
        NEXT_PANEL_TAB => Some(Command::NextPanelTab),
        PREV_PANEL_TAB => Some(Command::PrevPanelTab),
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
