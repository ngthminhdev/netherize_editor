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
    WrapSelectionWithStar,
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
    /// Visual mode paste: replace selection with clipboard content (nvim `p` in Visual).
    VisualPaste,
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

    // ── Debugger (DAP) & Flutter ───────────────────────────────────────────────
    DebugStart,
    DebugStop,
    DebugContinue,
    DebugStepOver,
    DebugStepInto,
    DebugStepOut,
    DebugToggleBreakpoint,
    DebugWatchAdd,
    DebugWatchRemove,
    DebugGotoFrame,
    FocusDap,
    DapToggleExpand,
    FlutterDevices,
    FlutterHotReload,
    FlutterHotRestart,

    // ── File & palette ─────────────────────────────────────────────────────────
    SaveFile,
    OpenFile(PathBuf),
    /// Launch a separate editor process with its own AppState/window.
    NewInstance,
    /// Open native OS folder picker and set the workspace root.
    OpenFolder,
    /// Open command palette showing recent projects list.
    OpenRecentProjects,
    OpenFilePicker,
    OpenVimCommand,
    OpenWorkspaceSymbols,
    OpenDocumentSymbols,
    OpenFileFinder,
    OpenInFileSearch,
    SearchInFiles,
    OpenThemeSelector,
    OpenFileHistory,
    OpenSettings,
    OpenHelp,
    OpenExtensionsManager,
    ExtensionsSelectNext,
    ExtensionsSelectPrev,
    ExtensionsStartFilter,
    ExtensionsCancelFilter,
    ExtensionsToggleExpanded,
    ExtensionsSwitchTabNext,
    ExtensionsSwitchTabPrev,
    ExtensionsInstallSelected,
    ExtensionsUninstallSelected,
    FilePickerAppendQuery(String),
    FilePickerBackspaceQuery,
    ToggleLiveGrepCaseSensitive,
    ToggleInFileSearchCaseSensitive,
    OverlaySelectNext,
    OverlaySelectPrev,
    SettingsSelectNext,
    SettingsSelectPrev,
    SettingsAdjustDecrease,
    SettingsAdjustIncrease,
    SettingsActivate,
    /// Decrease focused workbench window width in resize mode.
    ResizeDecreaseWidth,
    /// Increase focused workbench window width in resize mode.
    ResizeIncreaseWidth,
    /// Increase editor width from the left edge by shrinking the left dock.
    ResizeIncreaseLeftWidth,
    /// Decrease editor width from the left edge by growing the left dock.
    ResizeDecreaseLeftWidth,
    /// Increase editor width from the right edge by shrinking the right dock.
    ResizeIncreaseRightWidth,
    /// Decrease editor width from the right edge by growing the right dock.
    ResizeDecreaseRightWidth,
    /// Decrease focused workbench window height in resize mode.
    ResizeDecreaseHeight,
    /// Increase focused workbench window height in resize mode.
    ResizeIncreaseHeight,
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
    /// Open search within terminal scrollback (T-COPY mode).
    TerminalSearchOpen,
    /// Raw terminal bytes/string payload routed through command path.
    TerminalWriteInput(String),
    /// Paste system clipboard into the focused PTY session.
    TerminalPaste,
    /// Scroll terminal viewport up (towards scrollback history).
    TerminalScrollUp,
    /// Scroll terminal viewport down (towards live output).
    TerminalScrollDown,
    /// Create a new terminal tab in the bottom panel.
    TerminalTabNew,
    /// Close the current terminal tab in the bottom panel.
    TerminalTabClose,
    /// Switch to terminal tab N (0-based index).
    SwitchTerminalTab(usize),

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
    /// Toggle maximize focus for current region (Zen Mode).
    ToggleMaximizeFocus,

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
    ExplorerToggleGitChangesOnly,
    ExplorerMoveToTop,
    ExplorerMoveToBottom,
    ExplorerRenameFull,
    ExplorerRenameBase,
    ExplorerCopyFile,
    ExplorerPasteFile,
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
    BufferGoto(usize),

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
    /// <leader>rn: Mở prompt rename symbol theo LSP.
    LspRename,
    /// Format active document via LSP textDocument/formatting.
    LspFormatDocument,
    /// ctrl+space trong insert mode: gửi textDocument/completion.
    TriggerCompletion,
    /// <leader>ca: Gửi textDocument/codeAction, hiển thị quickfix/refactor menu.
    CodeAction,
    LspSelectPythonEnv,
    LspSelectDartEnv,
    ReloadWorkspace,
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
    /// Accept the next word/token from the AI inline ghost-text suggestion.
    AiAcceptInlineWord,

    // ── AI Chat ───────────────────────────────────────────────────────────────
    /// Toggle the AI chat panel open/closed.
    AiChatToggle,
    /// Send the current input text to the AI chat agent.
    AiChatSend,
    /// Stop the currently running AI chat generation.
    AiChatStop,
    /// Close the AI chat panel.
    AiChatClose,
    /// Unfocus AI chat input — return focus to editor without closing the dock.
    AiChatUnfocus,
    /// Focus into the AI chat panel — open dock if closed, switch to AI Chat tab, focus input.
    AiChatFocus,
    /// Add the current Visual selection as context for the next AI chat prompt.
    AiChatAddSelectionContext,
    /// Append a character to the AI chat input buffer.
    AiChatInputChar(char),
    /// Delete the last character from the AI chat input buffer.
    AiChatBackspace,
    /// Clear all text currently typed in the AI chat input buffer.
    AiChatClearInput,
    /// Complete the current slash command from the AI chat suggestion list.
    AiChatAcceptSuggestion,
    /// Cycle to the next suggestion in the AI chat suggestion popup.
    AiChatSuggestionNext,
    /// Cycle to the previous suggestion in the AI chat suggestion popup.
    AiChatSuggestionPrev,
    /// Append a text string to the AI chat input buffer (IME commit).
    AiChatInputText(String),
    /// Paste system clipboard text into the AI chat input buffer.
    AiChatPasteClipboard,
    /// Show the "opencode not found — install?" confirmation overlay.
    AiChatPromptInstall,
    /// Scroll AI chat message history up by half a page (see older messages).
    AiChatScrollHalfPageUp,
    /// Scroll AI chat message history down by half a page (see newer messages).
    AiChatScrollHalfPageDown,
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

    // ── Markdown Preview ──────────────────────────────────────────────────────
    /// Toggle markdown preview panel in the right sidebar.
    ToggleMarkdownPreview,
    /// Close focused sidebar(s): when RightSidebar has focus, close only the
    /// right dock; when LeftSidebar has focus, close only the left dock;
    /// otherwise close both if visible.
    CloseSidebars,
    /// Focus markdown preview in the right sidebar, opening it if needed.
    FocusMarkdownPreview,
    /// Scroll markdown preview up.
    MarkdownPreviewScrollUp,
    /// Scroll markdown preview down.
    MarkdownPreviewScrollDown,
    /// Scroll markdown preview up half page.
    MarkdownPreviewScrollHalfPageUp,
    /// Scroll markdown preview down half page.
    MarkdownPreviewScrollHalfPageDown,
    /// Scroll markdown preview to top (gg).
    MarkdownPreviewScrollTop,
    /// Scroll markdown preview to bottom (G).
    MarkdownPreviewScrollBottom,

    // ── Help / Cheat Sheet ────────────────────────────────────────────────────
    /// Scroll help / cheat sheet down.
    HelpScrollDown,
    /// Scroll help / cheat sheet up.
    HelpScrollUp,
    /// Scroll help / cheat sheet down half page.
    HelpScrollHalfPageDown,
    /// Scroll help / cheat sheet up half page.
    HelpScrollHalfPageUp,

    // ── Multiple Cursors (Module: MultiCursor) ────────────────────────────────
    /// Select word under cursor; on subsequent calls find the next identical
    /// word and place a virtual cursor on it.  Enters MultiCursor mode.
    MultiCursorAddNext,
    /// Skip the most recently added match and jump to the one after it.
    MultiCursorSkip,
    /// Move every cursor to the *start* of its selection then enter MultiInsert.
    MultiCursorInsertBefore,
    /// Move every cursor to the *end* of its selection then enter MultiInsert.
    MultiCursorAppendAfter,
    /// Delete every selection and enter MultiInsert (Vim `c` on multi-selection).
    MultiCursorChange,
    /// Delete every selection and stay in MultiCursor mode (Vim `d`).
    MultiCursorDelete,
    /// From Visual mode: select ALL occurrences of the visual selection and
    /// enter MultiCursor mode with a cursor on each match simultaneously.
    MultiCursorSelectAll,

    // ── Code folding ─────────────────────────────────────────────────────────
    /// Toggle fold/unfold the scope at the cursor line (Vim `za`).
    ToggleFold,
    /// Toggle fold all / unfold all scopes in the current file (Vim `zA`).
    ToggleFoldAll,
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
                | Self::FilePickerBackspaceQuery
                | Self::CompletionNext
                | Self::CompletionPrev
                | Self::ReferencesSelectNext
                | Self::ReferencesSelectPrev
                | Self::DiagnosticsSelectNext
                | Self::DiagnosticsSelectPrev
                | Self::ExtensionsSelectNext
                | Self::ExtensionsSelectPrev
                | Self::ExplorerMoveUp
                | Self::ExplorerMoveDown
                | Self::ExplorerCollapseOrParent
                | Self::ExplorerExpandOrChild
                | Self::TerminalScrollUp
                | Self::TerminalScrollDown
                | Self::MarkdownPreviewScrollUp
                | Self::MarkdownPreviewScrollDown
                | Self::MarkdownPreviewScrollHalfPageUp
                | Self::MarkdownPreviewScrollHalfPageDown
                | Self::MarkdownPreviewScrollTop
                | Self::MarkdownPreviewScrollBottom
                | Self::HelpScrollDown
                | Self::HelpScrollUp
                | Self::HelpScrollHalfPageDown
                | Self::HelpScrollHalfPageUp
        )
    }
}
