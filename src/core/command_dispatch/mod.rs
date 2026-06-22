use crate::{
    app::{app_state::AppState, clipboard::ClipboardProvider},
    core::commands::Command,
    terminal::grid::TerminalGrid,
};

mod common;
mod editing;
mod navigation;
mod palette;
mod session;

#[cfg(test)]
mod tests;

use common::DispatchCtx;
pub use common::DispatchReport;

/// Dispatcher là điểm duy nhất được phép apply `Command` vào `AppState`.
/// Event loop chỉ forward input, còn mutation logic được chia nhỏ theo domain
/// để flow `input -> command -> app state` vẫn dễ lần theo.
pub fn dispatch_command(app_state: &mut AppState, command: Command) -> DispatchReport {
    dispatch_command_count_with_terminal(app_state, command, 1, None)
}

pub fn dispatch_command_count(
    app_state: &mut AppState,
    command: Command,
    count: usize,
) -> DispatchReport {
    dispatch_command_with_clipboard_count_with_terminal(app_state, command, count, None, None)
}

pub fn dispatch_command_with_clipboard(
    app_state: &mut AppState,
    command: Command,
    clipboard: Option<&mut dyn ClipboardProvider>,
) -> DispatchReport {
    dispatch_command_with_clipboard_count_with_terminal(app_state, command, 1, clipboard, None)
}

pub fn dispatch_command_with_clipboard_count(
    app_state: &mut AppState,
    command: Command,
    count: usize,
    clipboard: Option<&mut dyn ClipboardProvider>,
) -> DispatchReport {
    dispatch_command_with_clipboard_count_with_terminal(app_state, command, count, clipboard, None)
}

pub fn dispatch_command_with_terminal(
    app_state: &mut AppState,
    command: Command,
    terminal: Option<&mut TerminalGrid>,
) -> DispatchReport {
    dispatch_command_count_with_terminal(app_state, command, 1, terminal)
}

pub fn dispatch_command_count_with_terminal(
    app_state: &mut AppState,
    command: Command,
    count: usize,
    terminal: Option<&mut TerminalGrid>,
) -> DispatchReport {
    dispatch_command_with_clipboard_count_with_terminal(app_state, command, count, None, terminal)
}

pub fn dispatch_command_with_clipboard_and_terminal(
    app_state: &mut AppState,
    command: Command,
    clipboard: Option<&mut dyn ClipboardProvider>,
    terminal: Option<&mut TerminalGrid>,
) -> DispatchReport {
    dispatch_command_with_clipboard_count_with_terminal(app_state, command, 1, clipboard, terminal)
}

pub fn dispatch_command_with_clipboard_count_with_terminal(
    app_state: &mut AppState,
    command: Command,
    count: usize,
    mut clipboard: Option<&mut dyn ClipboardProvider>,
    mut terminal: Option<&mut TerminalGrid>,
) -> DispatchReport {
    let repeat_count = if command.supports_numeric_count() {
        count.max(1)
    } else {
        1
    };

    if repeat_count == 1 {
        return dispatch_command_with_clipboard_once(
            app_state,
            command,
            &mut clipboard,
            &mut terminal,
            true,
        );
    }

    let group_transaction = command.groups_repeated_edits_into_single_transaction();
    let mut executed = 0usize;
    let mut any_redraw = false;
    let mut any_state_changed = false;
    let mut all_success = true;
    let mut last_message = String::new();

    for _ in 0..repeat_count {
        let report = dispatch_command_with_clipboard_once(
            app_state,
            command.clone(),
            &mut clipboard,
            &mut terminal,
            !group_transaction,
        );
        executed += 1;
        any_redraw |= report.request_redraw;
        any_state_changed |= report.state_changed;
        all_success &= report.success;
        last_message = report.message;

        if !report.success || !report.state_changed {
            break;
        }
    }

    if group_transaction && any_state_changed {
        let _ = app_state.commit_transaction();
    }

    let summary_message = if executed == repeat_count {
        format!("Dispatch: repeated {}x -> {}", repeat_count, last_message)
    } else {
        format!(
            "Dispatch: repeated {} of {}x -> {}",
            executed, repeat_count, last_message
        )
    };

    DispatchReport {
        message: summary_message,
        request_redraw: any_redraw,
        success: all_success,
        state_changed: any_state_changed,
    }
}

fn dispatch_command_with_clipboard_once(
    app_state: &mut AppState,
    command: Command,
    clipboard: &mut Option<&mut dyn ClipboardProvider>,
    terminal: &mut Option<&mut TerminalGrid>,
    auto_commit_text_transactions: bool,
) -> DispatchReport {
    let mut ctx = DispatchCtx::new(
        app_state,
        clipboard,
        terminal,
        auto_commit_text_transactions,
    );

    let is_text_mutation = matches!(
        &command,
        Command::InsertChar(_)
            | Command::InsertText(_)
            | Command::Newline
            | Command::InsertTab
            | Command::AiAcceptInline
            | Command::AiAcceptInlineWord
            | Command::Backspace
            | Command::InsertLineBelow
            | Command::InsertLineAbove
            | Command::InsertAtLineStart
            | Command::AppendAtLineEnd
            | Command::AppendAfterCursor
            | Command::SubstituteLine
            | Command::DeleteChar
            | Command::DeleteSelection
            | Command::DeleteCurrentLine
            | Command::DeleteToLineEnd
            | Command::ToggleLineComment
            | Command::ToggleSelectionComment
            | Command::WrapSelectionWithStar
            | Command::DeleteWordForward
            | Command::DeleteWordBackward
            | Command::ChangeSelection
            | Command::ChangeWordForward
            | Command::ChangeWordBackward
            | Command::ChangeToLineEnd
            | Command::JoinLines
            | Command::PasteAfter
            | Command::PasteBefore
            | Command::EditorPaste
            | Command::PasteSystemClipboard
            | Command::VisualPaste
            | Command::ReplaceChar(_)
            | Command::TextObjectAction { .. }
            | Command::Operate { .. }
            | Command::MultiCursorInsertBefore
            | Command::MultiCursorAppendAfter
            | Command::MultiCursorChange
            | Command::MultiCursorDelete
    );
    if is_text_mutation && ctx.app_state.active_text_buffer_missing_on_disk() {
        return DispatchReport::success(
            "Dispatch: active file is missing on disk; edit blocked until file exists again",
            false,
        );
    }

    match &command {
        Command::InsertChar(_)
        | Command::InsertText(_)
        | Command::Newline
        | Command::InsertTab
        | Command::AiAcceptInline
        | Command::AiAcceptInlineWord
        | Command::Backspace
        | Command::InsertLineBelow
        | Command::InsertLineAbove
        | Command::InsertAtLineStart
        | Command::AppendAtLineEnd
        | Command::AppendAfterCursor
        | Command::SubstituteLine
        | Command::DeleteChar
        | Command::DeleteSelection
        | Command::DeleteCurrentLine
        | Command::DeleteToLineEnd
        | Command::ToggleLineComment
        | Command::ToggleSelectionComment
        | Command::WrapSelectionWithStar
        | Command::DeleteWordForward
        | Command::DeleteWordBackward
        | Command::YankSelection
        | Command::YankCurrentLine
        | Command::YankToWordEnd
        | Command::ChangeSelection
        | Command::ChangeWordForward
        | Command::ChangeWordBackward
        | Command::ChangeToLineEnd
        | Command::JoinLines
        | Command::PasteAfter
        | Command::PasteBefore
        | Command::EditorPaste
        | Command::PasteSystemClipboard
        | Command::VisualPaste
        | Command::Undo
        | Command::Redo
        | Command::ReplaceChar(_)
        | Command::TextObjectAction { .. }
        | Command::Operate { .. }
        | Command::MultiCursorAddNext
        | Command::MultiCursorSkip
        | Command::MultiCursorInsertBefore
        | Command::MultiCursorAppendAfter
        | Command::MultiCursorChange
        | Command::MultiCursorDelete
        | Command::MultiCursorSelectAll => editing::dispatch(&mut ctx, command),
        Command::MoveLeft
        | Command::MoveRight
        | Command::MoveUp
        | Command::MoveDown
        | Command::MoveWordForward
        | Command::MoveWordBackward
        | Command::MoveWordEnd
        | Command::MoveToLineStart
        | Command::MoveToLineEnd
        | Command::MoveToFirstNonWhitespace
        | Command::MoveToFirstLine
        | Command::MoveToLastLine
        | Command::MoveParagraphUp
        | Command::MoveParagraphDown
        | Command::MatchBracket
        | Command::MoveFindChar(..)
        | Command::ScrollHalfPageUp
        | Command::ScrollHalfPageDown
        | Command::CenterCursorLine
        | Command::SearchNext
        | Command::SearchPrev
        | Command::SearchWordUnderCursor
        | Command::ClearSearchHighlights
        | Command::LeapStart
        | Command::LeapActivate(_)
        | Command::LeapJump(_)
        | Command::LeapCancel => navigation::dispatch(&mut ctx, command),
        Command::OpenFilePicker
        | Command::OpenFileFinder
        | Command::OpenVimCommand
        | Command::OpenInFileSearch
        | Command::OpenWorkspaceSymbols
        | Command::OpenDocumentSymbols
        | Command::FetchLeetCodeProblem
        | Command::LspRename
        | Command::SearchInFiles
        | Command::OpenThemeSelector
        | Command::OpenFileHistory
        | Command::OpenSettings
        | Command::OpenHelp
        | Command::OpenExtensionsManager
        | Command::ExtensionsSelectNext
        | Command::ExtensionsSelectPrev
        | Command::ExtensionsStartFilter
        | Command::ExtensionsCancelFilter
        | Command::ExtensionsToggleExpanded
        | Command::ExtensionsSwitchTabNext
        | Command::ExtensionsSwitchTabPrev
        | Command::ExtensionsInstallSelected
        | Command::ExtensionsUninstallSelected
        | Command::GitOpenLazygit
        | Command::GitOpenLazydocker
        | Command::GitBlameLine
        | Command::FilePickerAppendQuery(_)
        | Command::FilePickerBackspaceQuery
        | Command::PaletteMoveCursorLeft
        | Command::PaletteMoveCursorRight
        | Command::PaletteMoveCursorToStart
        | Command::PaletteMoveCursorToEnd
        | Command::PaletteDeleteCharForward
        | Command::PaletteVimInput(_)
        | Command::ToggleLiveGrepCaseSensitive
        | Command::ToggleInFileSearchCaseSensitive
        | Command::OverlaySelectNext
        | Command::OverlaySelectPrev
        | Command::SettingsSelectNext
        | Command::SettingsSelectPrev
        | Command::SettingsAdjustDecrease
        | Command::SettingsAdjustIncrease
        | Command::SettingsActivate
        | Command::FilePickerSelectNext
        | Command::FilePickerSelectPrev
        | Command::FilePickerConfirmSelection
        | Command::CloseFilePicker
        | Command::OpenCommandPalette
        | Command::TerminalSearchOpen => palette::dispatch(&mut ctx, command),
        Command::ResizeDecreaseWidth
        | Command::ResizeIncreaseWidth
        | Command::ResizeDecreaseHeight
        | Command::ResizeIncreaseHeight
        | Command::ResizeGrowLeftDock
        | Command::ResizeGrowRightDock => DispatchReport::success(
            format!("Dispatch: resize command {command:?} handled by event loop"),
            false,
        ),
        Command::SaveFile
        | Command::OpenFile(_)
        | Command::NewInstance
        | Command::OpenFolder
        | Command::OpenRecentProjects
        | Command::RemoveRecentProject
        | Command::ToggleTerminal
        | Command::ToggleBottomDock
        | Command::ToggleLeftDock
        | Command::TerminalWriteInput(_)
        | Command::TerminalPaste
        | Command::TerminalScrollUp
        | Command::TerminalScrollDown
        | Command::TerminalScrollHalfPageUp
        | Command::TerminalScrollHalfPageDown
        | Command::TerminalTabNew
        | Command::TerminalTabClose
        | Command::CloseSidebars
        | Command::SwitchTerminalTab(_)
        | Command::SwitchBottomTab(_)
        | Command::SwitchRightTab(_)
        | Command::SwitchLeftTab(_)
        | Command::OutlineNext
        | Command::OutlinePrev
        | Command::OutlineFirst
        | Command::OutlineLast
        | Command::OutlineConfirm
        | Command::RunTestCases
        | Command::AiAgentPickerNext
        | Command::AiAgentPickerPrev
        | Command::AiAgentPickerLaunch
        | Command::TestRunnerFocus
        | Command::TestRunnerUnfocus
        | Command::TestRunnerAddCase
        | Command::TestRunnerDeleteCase
        | Command::TestRunnerGenerateCases
        | Command::TestRunnerNextCase
        | Command::TestRunnerPrevCase
        | Command::TestRunnerToggleField
        | Command::TestRunnerEditField
        | Command::TestRunnerSelectCase(_)
        | Command::TestRunnerOpenField { .. }
        | Command::TestRunnerScroll(_)
        | Command::FocusEditor
        | Command::FocusExplorer
        | Command::FocusOutline
        | Command::FocusTerminal
        | Command::FocusInspector
        | Command::FocusLeft
        | Command::FocusRight
        | Command::FocusUp
        | Command::FocusDown
        | Command::MoveFocusCycle
        | Command::FocusBack
        | Command::ToggleMaximizeFocus
        | Command::ExplorerMoveUp
        | Command::ExplorerMoveDown
        | Command::ExplorerCollapseOrParent
        | Command::ExplorerExpandNode
        | Command::ExplorerCollapseAllUnderNode
        | Command::ExplorerExpandOrChild
        | Command::ExplorerExpandAllUnderNode
        | Command::ExplorerToggleOrOpen
        | Command::ExplorerDeleteNode
        | Command::ExplorerCreateFile
        | Command::ExplorerCreateFolder
        | Command::ExplorerStartFilter
        | Command::ExplorerClearFilter
        | Command::ExplorerToggleHidden
        | Command::ExplorerToggleIgnored
        | Command::ExplorerToggleGitChangesOnly
        | Command::ExplorerMoveToTop
        | Command::ExplorerMoveToBottom
        | Command::ExplorerHalfPageDown
        | Command::ExplorerHalfPageUp
        | Command::ExplorerRenameFull
        | Command::ExplorerRenameBase
        | Command::ExplorerCopyFile
        | Command::ExplorerPasteFile
        | Command::ExplorerExpandCollapse
        | Command::ExplorerOpenFile
        | Command::NextPanelTab
        | Command::PrevPanelTab
        | Command::BufferNew
        | Command::BufferNext
        | Command::BufferPrev
        | Command::BufferCloseCurrent
        | Command::BufferGoto(_)
        | Command::SwitchMode(_)
        | Command::EnterVisualLine
        | Command::LspHover
        | Command::LspGoToDefinition
        | Command::LspPreviewDefinition
        | Command::LspReferences
        | Command::CodeGraphOpenGraphHud
        | Command::CodeGraphNavLeft
        | Command::CodeGraphNavRight
        | Command::CodeGraphNavUp
        | Command::CodeGraphNavDown
        | Command::CodeGraphJump
        | Command::CodeGraphClose
        | Command::CanvasOpen
        | Command::CanvasClose
        | Command::CanvasFocusLeft
        | Command::CanvasFocusRight
        | Command::CanvasFocusUp
        | Command::CanvasFocusDown
        | Command::CanvasCycleNext
        | Command::CanvasCyclePrev
        | Command::CanvasSpawnRelations
        | Command::CanvasExpandCallee
        | Command::CanvasExpandCaller
        | Command::CanvasTogglePin
        | Command::CanvasAutoArrange
        | Command::CanvasNoop
        | Command::CanvasContextExpand
        | Command::CanvasContextShrink
        | Command::CanvasWidthExpand
        | Command::CanvasWidthShrink
        | Command::CanvasPanLeft
        | Command::CanvasPanRight
        | Command::CanvasPanUp
        | Command::CanvasPanDown
        | Command::CanvasMoveLeft
        | Command::CanvasMoveRight
        | Command::CanvasMoveUp
        | Command::CanvasMoveDown
        | Command::CanvasCloseFocused
        | Command::CanvasEnterEdit
        | Command::CanvasOpenCardBuffer
        | Command::CanvasExitEdit
        | Command::CanvasEnterBackground
        | Command::LspFormatDocument
        | Command::TriggerCompletion
        | Command::CodeAction
        | Command::CompletionNext
        | Command::CompletionPrev
        | Command::CompletionAccept
        | Command::CompletionClose
        | Command::ReferencesSelectNext
        | Command::ReferencesSelectPrev
        | Command::ReferencesOpenSelection
        | Command::DiagnosticsOpenPicker
        | Command::DiagnosticsSelectNext
        | Command::DiagnosticsSelectPrev
        | Command::DiagnosticsOpenSelection
        | Command::JumpBack
        | Command::JumpForward
        | Command::AiChatToggle
        | Command::AiChatClose
        | Command::AiChatUnfocus
        | Command::AiChatFocus
        | Command::ToggleMarkdownPreview
        | Command::FocusMarkdownPreview
        | Command::MarkdownPreviewScrollUp
        | Command::MarkdownPreviewScrollDown
        | Command::MarkdownPreviewScrollHalfPageUp
        | Command::MarkdownPreviewScrollHalfPageDown
        | Command::MarkdownPreviewScrollTop
        | Command::MarkdownPreviewScrollBottom
        | Command::MarkdownPreviewScrollLeft
        | Command::MarkdownPreviewScrollRight => session::dispatch(&mut ctx, command),
        Command::HelpScrollDown
        | Command::HelpScrollUp
        | Command::HelpScrollHalfPageDown
        | Command::HelpScrollHalfPageUp => session::dispatch(&mut ctx, command),
        Command::ToggleFold | Command::ToggleFoldAll | Command::ToggleCollapseExpand => session::dispatch(&mut ctx, command),
        Command::NewLeetCodeFile => {
            // Open the picker; the AppShell repopulates it with MRU-sorted
            // language items right after this dispatch returns.
            let changed = ctx.app_state.open_leetcode_language_selector();
            DispatchReport::success_with_flags(
                "Dispatch: leetcode language selector opened".to_string(),
                changed,
                changed,
            )
        }
        Command::ReloadWorkspace => DispatchReport::success(
            "Dispatch: reload workspace routed to event loop".to_string(),
            false,
        ),
        Command::LspSelectPythonEnv => {
            let changed = ctx.app_state.open_python_env_selector();
            DispatchReport::success_with_flags(
                "Dispatch: python env selector opened".to_string(),
                changed,
                changed,
            )
        }
        Command::LspSelectDartEnv => {
            let changed = ctx.app_state.open_dart_env_selector();
            DispatchReport::success_with_flags(
                "Dispatch: dart env selector opened".to_string(),
                changed,
                changed,
            )
        }
    }
}
