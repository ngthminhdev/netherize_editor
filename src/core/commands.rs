use std::path::PathBuf;

use crate::core::mode::ModeEvent;

/// A keystroke forwarded into the palette's single-line Vim state machine.
/// Synthesized by the input layer — never parsed from a keymap.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PaletteVimKey {
    Char(char),
    Esc,
    Enter,
}

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
    /// Vim `%`: jump between matching brackets and trigger a short target flash.
    MatchBracket,
    /// Vim f/F/t/T motion: nhảy tới ký tự trên dòng hiện tại, đồng thời set
    /// search highlight cho ký tự đó để n/N nhảy tiếp giữa các match.
    MoveFindChar(FindMotionKind, char),
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
    /// Launch a separate editor process with its own AppState/window.
    NewInstance,
    /// Open native OS folder picker and set the workspace root.
    OpenFolder,
    /// Open command palette showing recent projects list.
    OpenRecentProjects,
    /// Remove the selected entry from the recent projects list.
    RemoveRecentProject,
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
    /// Move the command-palette cursor one character left.
    PaletteMoveCursorLeft,
    /// Move the command-palette cursor one character right.
    PaletteMoveCursorRight,
    /// Move the command-palette cursor to the start of the query.
    PaletteMoveCursorToStart,
    /// Move the command-palette cursor to the end of the query.
    PaletteMoveCursorToEnd,
    /// Delete the character after the command-palette cursor.
    PaletteDeleteCharForward,
    /// A keystroke routed into the palette's single-line Vim state machine.
    PaletteVimInput(PaletteVimKey),
    ToggleLiveGrepCaseSensitive,
    ToggleInFileSearchCaseSensitive,
    OverlaySelectNext,
    OverlaySelectPrev,
    SettingsSelectNext,
    SettingsSelectPrev,
    SettingsAdjustDecrease,
    SettingsAdjustIncrease,
    SettingsActivate,
    /// Resize-mode `h`: shrink width. Narrows the focused sidebar, or — when the
    /// editor is focused — shrinks the left dock so the editor grows leftward.
    ResizeDecreaseWidth,
    /// Resize-mode `l`: grow width. Widens the focused sidebar, or — when the
    /// editor is focused — shrinks the right dock so the editor grows rightward.
    ResizeIncreaseWidth,
    /// Resize-mode `k`: shrink height. Shortens the focused bottom panel, or
    /// shortens the editor (grows the bottom panel) when the editor is focused.
    ResizeDecreaseHeight,
    /// Resize-mode `j`: grow height. Heightens the focused bottom panel, or
    /// heightens the editor (shrinks the bottom panel) when the editor is focused.
    ResizeIncreaseHeight,
    /// Resize-mode `H`: grow the left dock (editor shrinks on its left edge).
    ResizeGrowLeftDock,
    /// Resize-mode `L`: grow the right dock (editor shrinks on its right edge).
    ResizeGrowRightDock,
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
    /// Scroll terminal viewport up half a page (Ctrl+U in the right chat).
    TerminalScrollHalfPageUp,
    /// Scroll terminal viewport down half a page (Ctrl+D in the right chat).
    TerminalScrollHalfPageDown,
    /// Create a new terminal tab in the bottom panel.
    TerminalTabNew,
    /// Close the current terminal tab in the bottom panel.
    TerminalTabClose,
    /// Switch to terminal tab N (0-based index).
    SwitchTerminalTab(usize),
    /// Switch the bottom dock's active outer tab to index N (0-based).
    SwitchBottomTab(usize),
    /// Switch the right dock's active outer tab to index N (0-based).
    SwitchRightTab(usize),
    /// Switch the left dock's active outer tab to index N (0-based).
    SwitchLeftTab(usize),
    /// Move outline selection highlight down (next) and jump main editor cursor.
    OutlineNext,
    /// Move outline selection highlight up (prev) and jump main editor cursor.
    OutlinePrev,
    /// Select the first symbol in the focused Outline panel.
    OutlineFirst,
    /// Select the last symbol in the focused Outline panel.
    OutlineLast,
    /// Confirm outline selection, centering cursor and returning focus to the editor.
    OutlineConfirm,
    /// Run the active source file against all authored test cases (LeetCode
    /// run-and-compare). Shell-handled: resolves the interpreter, marks cases
    /// running, and submits the async execution worker.
    RunTestCases,
    /// Move the AI-agent picker selection (in-panel, AI Chat tab).
    AiAgentPickerNext,
    AiAgentPickerPrev,
    /// Launch the selected AI agent in the right-dock terminal.
    AiAgentPickerLaunch,
    /// Open the "New LeetCode File" language picker. Selecting a language
    /// scaffolds a runnable stdin/stdout starter file and opens it.
    NewLeetCodeFile,
    /// Fetch an existing LeetCode problem, generate a runnable solution file,
    /// and populate the Test Runner with examples.
    FetchLeetCodeProblem,
    /// Focus the Test Runner panel (open bottom dock + switch to its tab).
    TestRunnerFocus,
    /// Return focus from the Test Runner panel to the editor.
    TestRunnerUnfocus,
    /// Append a new empty case and begin editing its Input field.
    TestRunnerAddCase,
    /// Delete the selected case.
    TestRunnerDeleteCase,
    /// Replace all cases with 5 AI-generated ones for the active problem.
    TestRunnerGenerateCases,
    /// Move case selection down (next).
    TestRunnerNextCase,
    /// Move case selection up (previous).
    TestRunnerPrevCase,
    /// Toggle the focused column between Input and Expected.
    TestRunnerToggleField,
    /// Start editing the focused field of the selected case in a scratch buffer.
    TestRunnerEditField,
    TestRunnerSelectCase(usize),
    TestRunnerOpenField {
        case_index: usize,
        expected: bool,
    },
    TestRunnerScroll(isize),

    // ── Workbench focus navigation (Module 12 Phase 2) ─────────────────────────
    /// Move keyboard focus to the center editor region.
    FocusEditor,
    /// Show and focus the left sidebar (file explorer).
    FocusExplorer,
    /// Show, switch to outline tab, and focus the left sidebar.
    FocusOutline,
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
    ExplorerHalfPageDown,
    ExplorerHalfPageUp,
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

    // ── NetherCanvas (Spatial Canvas, Phase A) ───────────────────────────────────
    /// Open the spatial canvas on the symbol under the cursor.
    CanvasOpen,
    /// Close the canvas and return to the editor.
    CanvasClose,
    CanvasFocusLeft,
    CanvasFocusRight,
    CanvasFocusUp,
    CanvasFocusDown,
    CanvasCycleNext,
    CanvasCyclePrev,
    /// Spawn def/caller/callee blocks for the focused block (LSP-sourced).
    CanvasSpawnRelations,
    /// Expand the callees (outgoing) of the focused block.
    CanvasExpandCallee,
    /// Expand the callers (incoming) of the focused block.
    CanvasExpandCaller,
    /// Pin/unpin the focused block.
    CanvasTogglePin,
    /// `gca`: auto-arrange the cards back into tidy columns (re-flows the layout
    /// and unfreezes the user-arranged lock).
    CanvasAutoArrange,
    /// Consume a key with no effect (e.g. the `g` prefix of `gd`/`gr`).
    CanvasNoop,
    /// `+`: read more context lines into each card (cards grow to fit).
    CanvasContextExpand,
    /// `-`: read fewer context lines (cards shrink to fit).
    CanvasContextShrink,
    /// `Shift`+`+`: widen the focused card (more horizontal code visible).
    CanvasWidthExpand,
    /// `Shift`+`-`: narrow the focused card.
    CanvasWidthShrink,
    /// Pan the camera (Shift+hjkl).
    CanvasPanLeft,
    CanvasPanRight,
    CanvasPanUp,
    CanvasPanDown,
    /// Move the focused card (hjkl); a no-op when the card is pinned.
    CanvasMoveLeft,
    CanvasMoveRight,
    CanvasMoveUp,
    CanvasMoveDown,
    /// Close the focused relation card (space then x).
    CanvasCloseFocused,
    /// Enter edit mode on the focused card (Phase B): promote it to a live
    /// mini-editor backed by the card's file as the active buffer.
    CanvasEnterEdit,
    /// `o`: open the focused card's file as a real buffer tab (cursor at the
    /// symbol) and stash the canvas so it can be restored with `gc`.
    CanvasOpenCardBuffer,
    /// Leave edit mode back to card navigation.
    CanvasExitEdit,
    /// Push the canvas to the background so the editor regains focus (1st Esc);
    /// a second Esc then closes the canvas.
    CanvasEnterBackground,

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
    /// Restart the language server for the active file (stop running session(s)
    /// then respawn against the current interpreter/SDK selection).
    LspRestart,
    /// gp: Open the Code Graph HUD for the symbol under the caret.
    CodeGraphOpenGraphHud,
    /// Code Graph HUD navigation (vim hjkl) while the overlay is open.
    CodeGraphNavLeft,
    CodeGraphNavRight,
    CodeGraphNavUp,
    CodeGraphNavDown,
    /// Enter: jump to the focused node's file:line and close the HUD.
    CodeGraphJump,
    /// Esc: close the HUD.
    CodeGraphClose,
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

    // ── AI Agent Terminal ────────────────────────────────────────────────────
    /// Toggle the AI agent terminal tab open/closed.
    AiChatToggle,
    /// Close the AI agent terminal tab.
    AiChatClose,
    /// Unfocus AI agent terminal — return focus to editor without closing the dock.
    AiChatUnfocus,
    /// Focus the AI agent terminal tab — open dock if closed and show picker if needed.
    AiChatFocus,
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
    /// Scroll markdown preview left (h).
    MarkdownPreviewScrollLeft,
    /// Scroll markdown preview right (l).
    MarkdownPreviewScrollRight,

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
    /// Toggle fold/unfold the scope at the cursor line (Vim `za`).
    ToggleFoldAll,
    /// Collapse/expand file groups in FuzzyPicker and References buffers.
    ToggleCollapseExpand,
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
                | Self::MatchBracket
                | Self::MoveFindChar(..)
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

    /// True for editor text/cursor/mode commands that should target an active
    /// NetherCanvas in-card edit session (via the scoped swap) instead of the
    /// main editor. Excludes `SaveFile` (handled specially — the card is not a
    /// buffer slot), search (search state isn't swapped), folds, multi-cursor,
    /// and all panel/app/canvas commands (which run on the real state).
    pub fn is_card_editing_command(&self) -> bool {
        matches!(
            self,
            // Insert / append / open-line
            Self::InsertChar(_)
                | Self::InsertText(_)
                | Self::Newline
                | Self::Backspace
                | Self::InsertTab
                | Self::InsertLineBelow
                | Self::InsertLineAbove
                | Self::InsertAtLineStart
                | Self::AppendAtLineEnd
                | Self::AppendAfterCursor
                | Self::SubstituteLine
                // Delete / change / replace / join
                | Self::DeleteChar
                | Self::DeleteSelection
                | Self::DeleteCurrentLine
                | Self::DeleteToLineEnd
                | Self::DeleteWordForward
                | Self::DeleteWordBackward
                | Self::ChangeSelection
                | Self::ChangeWordForward
                | Self::ChangeWordBackward
                | Self::ChangeToLineEnd
                | Self::ReplaceChar(_)
                | Self::JoinLines
                // Yank / paste
                | Self::YankSelection
                | Self::YankCurrentLine
                | Self::YankToWordEnd
                | Self::PasteAfter
                | Self::PasteBefore
                | Self::EditorPaste
                | Self::PasteSystemClipboard
                | Self::VisualPaste
                // Comment / wrap
                | Self::ToggleLineComment
                | Self::ToggleSelectionComment
                | Self::WrapSelectionWithStar
                // Undo / redo
                | Self::Undo
                | Self::Redo
                // Motions
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
                | Self::MatchBracket
                | Self::MoveFindChar(..)
                | Self::CenterCursorLine
                | Self::ScrollHalfPageUp
                | Self::ScrollHalfPageDown
                // Operators (d/c/y + motion), text objects (diw/ci(/ya"/vi{…) and
                // mode switches (i/a/v/Esc…) — full vim editing inside the card.
                | Self::Operate { .. }
                | Self::TextObjectAction { .. }
                | Self::SwitchMode(_)
                | Self::EnterVisualLine
        )
    }

    /// Whether this command is a pure cursor MOTION inside a card (no text edit).
    /// After such a command the in-card caret is clamped back into the card's
    /// scope so motions pan the scope view but never park the caret outside the
    /// enclosing function. Edits are excluded on purpose: `o`/`O`/paste may
    /// legitimately land the caret on a freshly-created line.
    pub fn is_card_motion_command(&self) -> bool {
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
                | Self::MatchBracket
                | Self::MoveFindChar(..)
                | Self::CenterCursorLine
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
                | Self::MatchBracket
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
                | Self::PaletteMoveCursorLeft
                | Self::PaletteMoveCursorRight
                | Self::PaletteMoveCursorToStart
                | Self::PaletteMoveCursorToEnd
                | Self::PaletteDeleteCharForward
                | Self::PaletteVimInput(PaletteVimKey::Char('j'))
                | Self::PaletteVimInput(PaletteVimKey::Char('k'))
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
                | Self::OutlineNext
                | Self::OutlinePrev
                | Self::TerminalScrollUp
                | Self::TerminalScrollDown
                | Self::TerminalScrollHalfPageUp
                | Self::TerminalScrollHalfPageDown
                | Self::MarkdownPreviewScrollUp
                | Self::MarkdownPreviewScrollDown
                | Self::MarkdownPreviewScrollHalfPageUp
                | Self::MarkdownPreviewScrollHalfPageDown
                | Self::MarkdownPreviewScrollTop
                | Self::MarkdownPreviewScrollBottom
                | Self::MarkdownPreviewScrollLeft
                | Self::MarkdownPreviewScrollRight
                | Self::HelpScrollDown
                | Self::HelpScrollUp
                | Self::HelpScrollHalfPageDown
                | Self::HelpScrollHalfPageUp
                | Self::ResizeDecreaseWidth
                | Self::ResizeIncreaseWidth
                | Self::ResizeDecreaseHeight
                | Self::ResizeIncreaseHeight
                | Self::ResizeGrowLeftDock
                | Self::ResizeGrowRightDock
                | Self::SettingsSelectNext
                | Self::SettingsSelectPrev
                | Self::CodeGraphNavLeft
                | Self::CodeGraphNavRight
                | Self::CodeGraphNavUp
                | Self::CodeGraphNavDown
                // NetherCanvas Navigate: hold hjkl to glide a card, Shift+hjkl to
                // pan, arrows to sweep focus, and +/- to ramp a card's context.
                | Self::CanvasMoveLeft
                | Self::CanvasMoveRight
                | Self::CanvasMoveUp
                | Self::CanvasMoveDown
                | Self::CanvasPanLeft
                | Self::CanvasPanRight
                | Self::CanvasPanUp
                | Self::CanvasPanDown
                | Self::CanvasFocusLeft
                | Self::CanvasFocusRight
                | Self::CanvasFocusUp
                | Self::CanvasFocusDown
                | Self::CanvasContextExpand
                | Self::CanvasContextShrink
                | Self::CanvasWidthExpand
                | Self::CanvasWidthShrink
        )
    }
}
