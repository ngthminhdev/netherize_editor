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
    YankSelection,
    ChangeSelection,
    ChangeWordForward,
    ChangeWordBackward,
    PasteAfter,
    PasteBefore,
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
    SearchNext,
    SearchPrev,
    SearchWordUnderCursor,
    ClearSearchHighlights,

    // ── File & palette ─────────────────────────────────────────────────────────
    SaveFile,
    OpenFile(PathBuf),
    /// Open native OS folder picker and set the workspace root.
    OpenFolder,
    /// Open command palette showing recent projects list.
    OpenRecentProjects,
    OpenFilePicker,
    OpenVimCommand,
    OpenWorkspaceSymbols,
    OpenFileFinder,
    OpenInFileSearch,
    SearchInFiles,
    FilePickerAppendQuery(String),
    FilePickerBackspaceQuery,
    OverlaySelectNext,
    OverlaySelectPrev,
    // Legacy aliases (kept for backward compatibility with old keymaps/tests).
    FilePickerSelectNext,
    FilePickerSelectPrev,
    FilePickerConfirmSelection,
    CloseFilePicker,
    OpenCommandPalette,

    // ── Terminal ───────────────────────────────────────────────────────────────
    ToggleTerminal,
    ToggleLeftDock,
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
    ExplorerExpandNode,
    ExplorerCollapseAllUnderNode,
    ExplorerExpandOrChild,
    ExplorerExpandAllUnderNode,
    ExplorerToggleOrOpen,
    ExplorerDeleteNode,
    ExplorerCreateFile,
    ExplorerCreateFolder,
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

    // ── Text Objects (Vim vi(/va{/ci{/dib …) ─────────────────────────────────
    /// Chọn text object bằng cách set Visual selection lên vùng bracket.
    /// `open_char`/`close_char`: cặp bracket ('(',')', '{','}', '[',']').
    /// `inner`: true = nội dung bên trong; false = bao gồm cả bracket (around).
    SelectTextObject {
        open_char: char,
        close_char: char,
        inner: bool,
    },
    /// Xóa text object (d + i/a + bracket).
    DeleteTextObject {
        open_char: char,
        close_char: char,
        inner: bool,
    },
    /// Xóa text object rồi vào Insert mode (c + i/a + bracket).
    ChangeTextObject {
        open_char: char,
        close_char: char,
        inner: bool,
    },
    /// Yank (copy) text object vào clipboard (y + i/a + bracket).
    YankTextObject {
        open_char: char,
        close_char: char,
        inner: bool,
    },

    // ── Leap / EasyMotion navigation (Module 07 Phase 3) ──────────────────────
    /// Bắt đầu Leap session — InputHandler chuyển sang PendingLeapChar state.
    LeapStart,
    /// Nhận target char từ user, kích hoạt tìm kiếm và sinh labels.
    LeapActivate(char),
    /// User đã chọn một label — nhảy cursor đến vị trí tương ứng.
    LeapJump(char),
    /// Hủy Leap session (Escape hoặc không tìm thấy kết quả).
    LeapCancel,
}

impl Command {
    pub fn supports_numeric_count(&self) -> bool {
        matches!(
            self,
            Self::DeleteChar
                | Self::DeleteCurrentLine
                | Self::DeleteWordForward
                | Self::DeleteWordBackward
                | Self::ChangeWordForward
                | Self::ChangeWordBackward
                | Self::MoveLeft
                | Self::MoveRight
                | Self::MoveUp
                | Self::MoveDown
                | Self::MoveWordForward
                | Self::MoveWordBackward
                | Self::MoveWordEnd
                | Self::MoveToLineStart
                | Self::MoveToLineEnd
                | Self::MoveToFirstNonWhitespace
                | Self::MoveToFirstLine
                | Self::MoveToLastLine
                | Self::ScrollHalfPageUp
                | Self::ScrollHalfPageDown
                | Self::CenterCursorLine
                | Self::SearchNext
                | Self::SearchPrev
                | Self::InsertLineBelow
                | Self::InsertLineAbove
                | Self::PasteAfter
                | Self::PasteBefore
                | Self::Undo
                | Self::Redo
        )
    }

    pub fn groups_repeated_edits_into_single_transaction(&self) -> bool {
        matches!(
            self,
            Self::DeleteChar
                | Self::DeleteCurrentLine
                | Self::DeleteWordForward
                | Self::DeleteWordBackward
                | Self::PasteAfter
                | Self::PasteBefore
        )
    }
}
