use std::path::PathBuf;

use crate::core::mode::ModeEvent;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Operator {
    Visual,
    Delete,
    Change,
    Yank,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextObjectModifier {
    Inner,
    Around,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextObjectKind {
    Word,
    Quote(char),
    Bracket(char, char),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FindMotionKind {
    ForwardTo,
    ForwardTill,
    BackwardTo,
    BackwardTill,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Motion {
    WordForward,
    WordBackward,
    WordEnd,
    LineStart,
    LineEnd,
    FirstNonWhitespace,
    FirstLine,
    LastLine,
    FindChar(FindMotionKind, char),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OperationTarget {
    Motion(Motion),
    TextObject {
        modifier: TextObjectModifier,
        kind: TextObjectKind,
    },
    CurrentLine,
}

/// Command là giao diện trung gian giữa input layer và editor core.
/// Event loop chỉ chuyển phím -> command, không sửa state trực tiếp.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    // ── Text editing ────────────────────────────────────────────────────────────
    InsertChar(char),
    InsertText(String),
    Newline,
    Backspace,
    InsertTab,
    InsertLineBelow,
    InsertLineAbove,
    InsertAtLineStart,
    AppendAtLineEnd,
    AppendAfterCursor,
    SubstituteLine,
    DeleteChar,
    DeleteSelection,
    DeleteCurrentLine,
    DeleteToLineEnd,
    ToggleLineComment,
    ToggleSelectionComment,
    DeleteWordForward,
    DeleteWordBackward,
    YankSelection,
    YankCurrentLine,
    YankToWordEnd,
    ChangeSelection,
    ChangeWordForward,
    ChangeWordBackward,
    ChangeToLineEnd,
    JoinLines,
    PasteAfter,
    PasteBefore,
    EditorPaste,
    /// Legacy alias for older keymaps; routed with `EditorPaste`.
    PasteSystemClipboard,
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
    MoveParagraphUp,
    MoveParagraphDown,
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
    OpenThemeSelector,
    OpenSettings,
    OpenHelp,
    FilePickerAppendQuery(String),
    FilePickerBackspaceQuery,
    OverlaySelectNext,
    OverlaySelectPrev,
    SettingsSelectNext,
    SettingsSelectPrev,
    SettingsAdjustDecrease,
    SettingsAdjustIncrease,
    SettingsActivate,
    GitOpenLazygit,
    GitOpenLazydocker,
    GitBlameLine,
    // Legacy aliases (kept for backward compatibility with old keymaps/tests).
    FilePickerSelectNext,
    FilePickerSelectPrev,
    FilePickerConfirmSelection,
    CloseFilePicker,
    OpenCommandPalette,

    // ── Terminal ───────────────────────────────────────────────────────────────
    ToggleTerminal,
    ToggleBottomDock,
    ToggleLeftDock,
    /// Raw terminal bytes/string payload routed through command path.
    TerminalWriteInput(String),
    /// Paste system clipboard into the focused PTY session.
    TerminalPaste,
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
    ExplorerStartFilter,
    ExplorerClearFilter,
    ExplorerToggleHidden,
    ExplorerToggleIgnored,
    ExplorerMoveToTop,
    ExplorerMoveToBottom,
    ExplorerRenameFull,
    ExplorerRenameBase,
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
    TextObjectAction {
        op: Operator,
        modifier: TextObjectModifier,
        kind: TextObjectKind,
    },
    Operate {
        op: Operator,
        target: OperationTarget,
    },

    // ── Leap / EasyMotion navigation (Module 07 Phase 3) ──────────────────────
    /// Bắt đầu Leap session — InputHandler chuyển sang PendingLeapChar state.
    LeapStart,
    /// Nhận target char từ user, kích hoạt tìm kiếm và sinh labels.
    LeapActivate(char),
    /// Nhận thêm 1 ký tự label/prefix — command layer sẽ lọc live và nhảy khi đủ duy nhất.
    LeapJump(char),
    /// Hủy Leap session (Escape hoặc không tìm thấy kết quả).
    LeapCancel,

    // ── LSP Interactive Features (Module 10) ──────────────────────────────────────
    /// Shift+K: Gửi textDocument/hover, hiển thị FloatingBox tài liệu hàm/biến.
    LspHover,
    /// gd: Gửi textDocument/definition, nhảy đến file và dòng định nghĩa.
    LspGoToDefinition,
    /// gD: Gửi textDocument/definition, hiển thị code peek FloatingBox.
    LspPreviewDefinition,
    /// gr: Gửi textDocument/references, mở danh sách tham chiếu.
    LspReferences,
    /// Format active document via LSP textDocument/formatting.
    LspFormatDocument,
    /// ctrl+space trong insert mode: gửi textDocument/completion.
    TriggerCompletion,
    /// Completion popup: chọn item kế tiếp.
    CompletionNext,
    /// Completion popup: chọn item trước đó.
    CompletionPrev,
    /// Completion popup: chèn item đang chọn.
    CompletionAccept,
    /// Completion popup: đóng popup.
    CompletionClose,
    /// Accept AI inline ghost-text suggestion into the real buffer.
    AiAcceptInline,
    /// References view: chọn item kế tiếp.
    ReferencesSelectNext,
    /// References view: chọn item trước đó.
    ReferencesSelectPrev,
    /// References view: mở item đang chọn như `gd`.
    ReferencesOpenSelection,
    /// Open workspace diagnostics picker.
    DiagnosticsOpenPicker,
    /// Diagnostics view: chọn item kế tiếp.
    DiagnosticsSelectNext,
    /// Diagnostics view: chọn item trước đó.
    DiagnosticsSelectPrev,
    /// Diagnostics view: mở item đang chọn.
    DiagnosticsOpenSelection,
    /// ctrl+o: Nhảy lùi về vị trí trước trong jump list.
    JumpBack,
    /// ctrl+i: Nhảy tiến về vị trí sau trong jump list.
    JumpForward,
}

impl Command {
    pub fn supports_numeric_count(&self) -> bool {
        matches!(
            self,
            Self::DeleteChar
                | Self::DeleteCurrentLine
                | Self::DeleteToLineEnd
                | Self::DeleteWordForward
                | Self::DeleteWordBackward
                | Self::ChangeWordForward
                | Self::ChangeWordBackward
                | Self::ChangeToLineEnd
                | Self::JoinLines
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
                | Self::MoveParagraphUp
                | Self::MoveParagraphDown
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
                | Self::Operate { .. }
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
                | Self::Operate { .. }
        )
    }

    pub fn supports_press_and_hold_repeat(&self) -> bool {
        matches!(
            self,
            Self::MoveLeft
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
                | Self::MoveParagraphUp
                | Self::MoveParagraphDown
                | Self::Backspace
                | Self::DeleteChar
                | Self::DeleteWordForward
                | Self::DeleteWordBackward
                | Self::InsertChar(_)
                | Self::InsertTab
                | Self::Newline
                | Self::ScrollHalfPageUp
                | Self::ScrollHalfPageDown
                | Self::SearchNext
                | Self::SearchPrev
                | Self::Undo
                | Self::Redo
                | Self::OverlaySelectNext
                | Self::OverlaySelectPrev
                | Self::CompletionNext
                | Self::CompletionPrev
                | Self::ReferencesSelectNext
                | Self::ReferencesSelectPrev
                | Self::DiagnosticsSelectNext
                | Self::DiagnosticsSelectPrev
                | Self::ExplorerMoveUp
                | Self::ExplorerMoveDown
                | Self::ExplorerCollapseOrParent
                | Self::ExplorerExpandOrChild
                | Self::TerminalScrollUp
                | Self::TerminalScrollDown
        )
    }
}
