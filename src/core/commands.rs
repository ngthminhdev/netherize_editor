use std::path::PathBuf;

use crate::core::mode::ModeEvent;

/// Command là giao diện trung gian giữa input layer và editor core.
/// Event loop chỉ chuyển phím -> command, không sửa state trực tiếp.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    // ── Text editing ────────────────────────────────────────────────────────────
    InsertChar(char),
    InsertText(String),
    Newline,
    Backspace,
    InsertLineBelow,
    InsertLineAbove,
    InsertAtLineStart,
    AppendAtLineEnd,
    AppendAfterCursor,
    SubstituteLine,
    DeleteChar,
    DeleteSelection,
    DeleteCurrentLine,
    DeleteWordForward,
    DeleteWordBackward,
    ChangeSelection,
    ChangeWordForward,
    ChangeWordBackward,
    Undo,
    Redo,
    ReplaceChar(char),
    MoveLeft,
    MoveRight,
    MoveUp,
    MoveDown,
    MoveWordForward,
    MoveWordBackward,
    MoveWordEnd,
    MoveToLineStart,
    MoveToLineEnd,
    MoveToFirstNonWhitespace,
    MoveToFirstLine,
    MoveToLastLine,
    ScrollHalfPageUp,
    ScrollHalfPageDown,
    CenterCursorLine,

    // ── File & palette ─────────────────────────────────────────────────────────
    SaveFile,
    OpenFile(PathBuf),
    OpenFilePicker,
    OpenVimCommand,
    OpenWorkspaceSymbols,
    OpenFileFinder,
    SearchInFiles,
    FilePickerAppendQuery(String),
    FilePickerBackspaceQuery,
    FilePickerSelectNext,
    FilePickerSelectPrev,
    FilePickerConfirmSelection,
    CloseFilePicker,
    OpenCommandPalette,

    // ── Terminal ───────────────────────────────────────────────────────────────
    ToggleTerminal,
    ToggleExplorer,
    /// Raw terminal bytes/string payload routed through command path.
    TerminalWriteInput(String),
    /// Scroll terminal viewport up (towards scrollback history).
    TerminalScrollUp,
    /// Scroll terminal viewport down (towards live output).
    TerminalScrollDown,

    // ── Workbench focus navigation (Module 12 Phase 2) ─────────────────────────
    /// Move keyboard focus to the center editor region.
    FocusEditor,
    /// Show and focus the left sidebar (file explorer).
    FocusExplorer,
    /// Show and focus the terminal in the bottom panel.
    FocusTerminal,
    /// Show and focus the right sidebar (inspector).
    FocusInspector,
    /// Move focus left in the workbench graph (nvim-style directional jump).
    FocusLeft,
    /// Move focus right in the workbench graph (nvim-style directional jump).
    FocusRight,
    /// Move focus up in the workbench graph (nvim-style directional jump).
    FocusUp,
    /// Move focus down in the workbench graph (nvim-style directional jump).
    FocusDown,
    /// Cycle focus through visible panels: Editor → Explorer → Bottom → Editor.
    MoveFocusCycle,
    /// Return focus to the editor from any other surface (universal escape).
    FocusBack,

    // ── Explorer surface commands ───────────────────────────────────────────────
    ExplorerMoveUp,
    ExplorerMoveDown,
    ExplorerCollapseOrParent,
    ExplorerExpandOrChild,
    ExplorerToggleOrOpen,
    // Legacy aliases (kept for backward compatibility with old keymaps/tests).
    ExplorerExpandCollapse,
    ExplorerOpenFile,

    // ── Panel tab commands ─────────────────────────────────────────────────────
    NextPanelTab,
    PrevPanelTab,

    // ── Buffer commands ────────────────────────────────────────────────────────
    BufferNew,
    BufferNext,
    BufferPrev,
    BufferCloseCurrent,

    // ── Mode transitions ────────────────────────────────────────────────────────
    /// Request a mode change; actual transition decided by dispatcher / app state.
    SwitchMode(ModeEvent),
    /// Enter Visual Line mode (Vim `V`): selects whole lines.
    EnterVisualLine,
}
