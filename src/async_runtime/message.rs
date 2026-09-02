use std::{ops::Range, path::PathBuf};

use tokio_util::sync::CancellationToken;

use crate::syntax::{highlight::HighlightSpan, syntax_engine::LanguageId};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GitFileStatus {
    Modified,
    Added,
}

/// Trạng thái cài đặt của từng tool hệ thống.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InstallStatus {
    Pending,
    Installing,
    Success,
    Failed,
}

/// Wire-format mirror of `WorkDoneProgress` lifecycle. Decoupled from
/// `app::app_state::LspProgressKind` so the message layer doesn't depend on
/// the app crate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LspProgressKindWire {
    Begin,
    Report,
    End,
}

/// Topic giúp app biết result thuộc subsystem nào để so revision.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RequestTopic {
    ActiveBufferLayout,
    ProjectSearch,
    BackgroundDemo,
    WorkspaceWatch,
    TerminalPty,
    Git,
    GitStatus,
    GitBaseline,
    LspClient,
    LspCheck,
    /// Các LSP interactive requests: hover, definition, references.
    LspRequest,
    FzfSearch,
    FilePreview,
    AiInlineCompletion,
    SystemDepCheck,
    /// Per-tool streaming installation.
    SystemDepInstall,
    /// General-purpose system tasks (Python env scan, etc).
    SystemTask,
    /// File system operations (copy, move, etc).
    FileOperation,
    /// Run-and-compare test execution (LeetCode/interview runner).
    TestRunner,
    LeetCode,
    /// Code Graph HUD query (codegraph callers/callees/impact).
    CodeGraph,
}

/// Which search mode the fzf worker is running.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FzfSearchMode {
    /// `find . -type f | fzf --filter query` — Find File picker
    FindFile,
    /// `rg --line-number --column "" . | fzf --filter query` — Live Grep
    LiveGrep,
}

/// A single result row returned by the fzf worker.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FzfResultItem {
    /// Display label shown in the palette row.
    pub label: String,
    /// Optional secondary preview line shown beneath the primary label.
    pub preview: Option<String>,
    /// Absolute path to the file (used as the OpenFile action).
    pub path: PathBuf,
    /// 1-based line number (Live Grep only).
    pub line: Option<u32>,
    /// 1-based column number (Live Grep only).
    pub column: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FilePreviewLine {
    pub line_number: usize,
    pub text: String,
    pub is_target: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FileSystemChangeKind {
    Create,
    Delete,
    Modify,
    Rename,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct FileSystemEvent {
    pub kind: FileSystemChangeKind,
    pub path: PathBuf,
    pub new_path: Option<PathBuf>,
}

/// One externally changed file fetched by the worker for the UI to apply.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExternalFileRead {
    pub path: PathBuf,
    pub content: Option<String>,
    pub modified_time: Option<std::time::SystemTime>,
}

/// Request từ UI/main thread sang worker.
/// revision_id là bắt buộc để detect stale result.
#[derive(Debug, Clone)]
pub struct WorkerRequest {
    pub request_id: u64,
    pub revision_id: u64,
    pub topic: RequestTopic,
    pub payload: WorkerRequestPayload,
}

/// Byte-level description of a single edit applied to the buffer.
/// Passed through to the worker so tree-sitter can do an incremental reparse
/// instead of a full reparse.  Row/column positions are computed in the worker
/// from the new text snapshot, which avoids the need to ship the old text.
#[derive(Debug, Clone, Copy)]
pub struct SyntaxEditHint {
    pub start_byte: usize,
    pub old_end_byte: usize,
    pub new_end_byte: usize,
}

#[derive(Debug, Clone)]
pub enum WorkerRequestPayload {
    ParseAndHighlight {
        /// Stable identity of the text buffer this request was created for.
        /// For file-backed buffers this is the canonical path.
        buffer_id: PathBuf,
        file_path: Option<PathBuf>,
        text_snapshot: String,
        language_id: LanguageId,
        buffer_revision: u64,
        viewport_line_start: usize,
        viewport_line_count: usize,
        line_starts: Vec<usize>,
        /// Single-edit hint for incremental tree-sitter reparse.
        /// `None` when multiple edits accumulated (debounced typing, paste, undo/redo)
        /// — the worker falls back to a full reparse in that case.
        edit_hint: Option<SyntaxEditHint>,
    },
    MockParseBuffer {
        file_path: PathBuf,
        text_snapshot: String,
        simulated_delay_ms: u64,
    },
    MockSearch {
        query: String,
        simulated_delay_ms: u64,
    },
    MockCpuBurn {
        job_label: String,
        busy_millis: u64,
    },
    MockPanic {
        reason: String,
    },
    StartFileWatch {
        root_path: PathBuf,
        /// `true` cho workspace root (quét cả cây), `false` cho file mở ngoài root
        /// (chỉ watch thư mục cha — tránh dựng recursive watcher nặng trên /tmp, /etc…).
        recursive: bool,
    },
    SpawnPtyShell {
        shell: Option<String>,
        working_dir: Option<PathBuf>,
    },
    SpawnPtyCommand {
        program: String,
        args: Vec<String>,
        working_dir: Option<PathBuf>,
    },
    SpawnDetachedShellCommand {
        command: String,
        working_dir: Option<PathBuf>,
    },
    WritePtyInput {
        session_id: u64,
        input: String,
    },
    ResizePtySession {
        session_id: u64,
        cols: u16,
        rows: u16,
    },
    ClosePtySession {
        session_id: u64,
    },
    StartLspServer {
        root_path: PathBuf,
        server_command: Option<String>,
        custom_bin_path: Option<PathBuf>,
        /// Python interpreter chosen by the user, pushed to the Python server so
        /// analysis (completion, diagnostics) runs against the selected env.
        interpreter_path: Option<PathBuf>,
        /// Launch arguments. `None` falls back to the primary profile's args;
        /// companion servers (e.g. `ruff server`) pass their own.
        launch_args: Option<Vec<String>>,
    },
    FzfSearch {
        query: String,
        mode: FzfSearchMode,
        workspace_root: PathBuf,
        case_sensitive: bool,
    },
    GitBlameLine {
        workspace_root: PathBuf,
        file_path: PathBuf,
        line_number: usize,
    },
    RefreshWorkspaceGitStatus {
        workspace_root: PathBuf,
    },
    FetchGitBaseline {
        workspace_root: PathBuf,
        file_path: PathBuf,
    },
    LoadFilePreview {
        file_path: PathBuf,
        max_lines: usize,
        target_line: Option<usize>,
    },
    LspDidOpen {
        uri: String,
        language_id: String,
        version: i32,
        text: String,
    },
    LspDidChange {
        uri: String,
        version: i32,
        text: String,
    },
    LspDidClose {
        uri: String,
    },
    CheckLspForPath {
        /// File path được mở — dùng để look up extension và registry.
        path: PathBuf,
    },
    /// textDocument/hover request.
    LspHoverRequest {
        language_id: String,
        uri: String,
        line: u32,
        character: u32,
        /// When true, result goes into CompletionState.hover_doc instead of overlay.
        for_completion: bool,
        /// `Some(revision)` only when `for_completion=true`: snapshot of
        /// `CompletionState.current_revision` to echo back so a stale
        /// fallback-hover doesn't overwrite docs for a newer item.
        completion_revision: Option<u64>,
    },
    /// textDocument/definition request — jump hoặc peek.
    LspDefinitionRequest {
        uri: String,
        line: u32,
        character: u32,
        /// `true` = nhảy thẳng (gd), `false` = hiển peek (gD).
        jump: bool,
    },
    /// textDocument/references request.
    LspReferencesRequest {
        uri: String,
        line: u32,
        character: u32,
    },
    /// textDocument/rename request.
    LspRenameRequest {
        uri: String,
        line: u32,
        character: u32,
        new_name: String,
    },
    /// textDocument/documentHighlight request for the active file/cursor.
    LspDocumentHighlightRequest {
        language_id: String,
        uri: String,
        line: u32,
        character: u32,
    },
    /// textDocument/documentSymbol request for the active file.
    LspDocumentSymbolsRequest {
        language_id: String,
        uri: String,
    },
    /// Request documentSymbol for a NON-active file (a spawned canvas card's
    /// target), correlated to the card by `card_id` so the result can refine that
    /// one card's snapshot to the enclosing-definition scope. Best-effort: an
    /// empty/error result keeps the card's ±N window snapshot (no regression).
    CanvasCardScopeRequest {
        card_id: u64,
        uri: String,
    },
    /// textDocument/formatting request.
    LspFormattingRequest {
        language_id: String,
        uri: String,
        tab_size: u32,
        insert_spaces: bool,
    },
    /// textDocument/completion request.
    LspCompletionRequest {
        language_id: String,
        uri: String,
        line: u32,
        character: u32,
        cursor_line: usize,
        cursor_col: usize,
        prefix_start_col: usize,
        prefix: String,
    },
    /// textDocument/codeAction request.
    LspCodeActionRequest {
        uri: String,
        line: u32,
        character: u32,
        diagnostics: Vec<LspDiagnostic>,
    },
    /// completionItem/resolve request — fetches additional `documentation`/`detail`
    /// for a previously returned completion item.
    LspCompletionResolveRequest {
        language_id: String,
        uri: String,
        /// Original JSON of the completion item (round-tripped to the server).
        item_json: String,
        /// Label used to verify the response still matches the requested item.
        item_label: String,
        /// Snapshot of `CompletionState.current_revision` when this request
        /// was issued. Echoed back in `LspCompletionResolveResult` so the
        /// main thread can drop the result if the user has since moved on
        /// to a different item (race-condition guard).
        completion_revision: u64,
    },
    /// Fallback docs for completion items that don't provide documentation via
    /// `completionItem/resolve`: open a short-lived shadow document with the
    /// selected completion text inserted, then run `textDocument/hover` there.
    LspCompletionVirtualHoverRequest {
        language_id: String,
        uri: String,
        original_text: String,
        text: String,
        hover_line: u32,
        hover_character: u32,
        completion_revision: u64,
    },
    AiInlineCompletionRequest {
        api_url: String,
        api_key: Option<String>,
        model: String,
        endpoint_kind: Option<String>,
        reasoning_effort: Option<String>,
        prefix: String,
        suffix: String,
        language_id: Option<String>,
        file_path: Option<PathBuf>,
        max_tokens: u32,
        cancel_token: CancellationToken,
    },
    /// AI re-rank of an LSP completion popup. The worker asks the model to order
    /// `candidates` (the server's own labels) by relevance at the cursor and
    /// returns the preferred ordering. The model never adds or removes labels.
    AiCompletionRerankRequest {
        api_url: String,
        api_key: Option<String>,
        model: String,
        endpoint_kind: Option<String>,
        reasoning_effort: Option<String>,
        prefix: String,
        suffix: String,
        language_id: Option<String>,
        candidates: Vec<String>,
        /// Echoed back so the main thread can drop a result that arrived after
        /// the user changed the typed prefix / closed the popup.
        prefix_token: String,
        completion_revision: u64,
        cancel_token: CancellationToken,
    },
    /// Check for missing system CLI tools (fzf, lazygit, lazydocker, rg, etc.).
    CheckSystemDeps,
    /// Install system CLI tools one-by-one, streaming per-tool progress messages.
    InstallSystemDeps {
        tools: Vec<String>,
    },
    /// Run an extension install/uninstall command and stream stdout/stderr lines.
    RunExtensionCommand {
        binary: String,
        command: String,
        uninstall: bool,
        working_dir: Option<PathBuf>,
    },
    StopLspServer,
    ShutdownAllLspServers,
    ScanPythonEnvironments {
        workspace_root: PathBuf,
    },
    ScanDartEnvironments {
        workspace_root: PathBuf,
    },
    /// Detect Python / Node / Go runtime versions for statusbar display.
    /// `python_binary` is the interpreter path from the selected venv (or system).
    DetectRuntimeVersions {
        python_binary: Option<PathBuf>,
        workspace_root: PathBuf,
    },
    /// Copy a file from source to target path (async to avoid blocking UI).
    CopyFile {
        source_path: PathBuf,
        target_path: PathBuf,
    },
    /// Small text-file writes off the UI thread (Dojo notebook, current.md…).
    /// Ops run sequentially in the order given.
    WriteTextFiles {
        ops: Vec<TextFileOp>,
    },
    /// Read contents of externally changed files off the UI thread.
    /// Phase 2 of the external-change pipeline: the main thread only decides
    /// WHICH paths need reloading; the worker does the actual disk reads.
    ReadExternalFiles {
        paths: Vec<PathBuf>,
    },
    /// Re-scan the workspace tree off the UI thread (external create/delete/
    /// rename). The worker walks the tree and returns fresh nodes to swap in.
    RescanWorkspace {
        root_path: PathBuf,
        ignore_rules: crate::workspace::model::WorkspaceIgnoreRules,
        options: crate::workspace::scanner::WorkspaceScanOptions,
    },
    /// workspace/symbol request — index all symbols in the workspace.
    WorkspaceSymbolRequest {
        language_id: String,
        query: String,
        /// Project root of the target server, so the worker routes to the
        /// session at this root instead of an arbitrary same-binary one
        /// (critical when multiple gopls/tsserver sessions run side by side).
        root_path: Option<PathBuf>,
    },
    /// Local TS/JS export indexing request — scans project files outside the UI
    /// thread and returns importable workspace symbols.
    WorkspaceExportIndexRequest {
        language_id: String,
        workspace_root: PathBuf,
    },
    /// Run the active source file against a batch of test-case inputs. The same
    /// `program`+`args` is invoked once per input (stdin), and stdout/stderr is
    /// captured per case. Comparison with expected output happens on the main
    /// thread (kept out of the worker so the judge stays single-sourced).
    RunTestCases {
        /// Optional one-time compile step (compiled languages like Rust). Run
        /// once before any case; on failure every case becomes an Error.
        compile: Option<crate::runner::CompileStep>,
        program: String,
        /// Full argument vector (leading args + source file path), or just the
        /// compiled binary path for compiled languages.
        args: Vec<String>,
        working_dir: Option<PathBuf>,
        /// stdin text for each case, index-aligned with the authored cases.
        inputs: Vec<String>,
        /// Per-case wall-clock kill deadline in milliseconds.
        timeout_ms: u64,
        /// Human-readable command preview echoed back for display.
        command_preview: String,
    },
    FetchLeetCodeProblem {
        input: String,
        language_key: String,
        destination_dir: PathBuf,
        use_ai: bool,
        provider: Option<crate::config::ai_config::AiProviderConfig>,
    },
    /// Dojo preview: refresh the per-problem cache (statement, examples,
    /// hints) for `slug` without writing a solution file.
    FetchLeetCodeStatement {
        slug: String,
    },
    GenerateLeetCodeTests {
        cache: crate::runner::leetcode_cache::LeetCodeProblemCache,
        language_key: String,
        provider: crate::config::ai_config::AiProviderConfig,
        verify: bool,
    },
    /// Run codegraph callers+callees+impact for the symbol under the caret,
    /// building the Code Graph HUD model off the UI thread.
    CodeGraphQuery {
        symbol: String,
        focal_file: String,
        focal_line: u32,
        workspace_root: PathBuf,
    },
}

/// Loại location từ LSP — dùng cho definition và references.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LspLocation {
    /// URI dạng `file:///absolute/path`.
    pub uri: String,
    pub line: u32,
    pub character: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LspPosition {
    pub line: u32,
    pub character: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LspRange {
    pub start: LspPosition,
    pub end: LspPosition,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LspDiagnostic {
    pub range: LspRange,
    pub severity: Option<u32>,
    pub code: Option<String>,
    pub source: Option<String>,
    pub message: String,
    /// LSP DiagnosticTag values. `1` = Unnecessary/unused, `2` = Deprecated.
    pub tags: Vec<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LspCompletionItem {
    pub label: String,
    pub detail: Option<String>,
    pub insert_text: Option<String>,
    pub text_edit: Option<LspTextEdit>,
    pub text_edit_text: Option<String>,
    pub additional_text_edits: Vec<LspTextEdit>,
    pub kind: Option<u32>,
    /// LSP `sortText` — the server's preferred ordering key. Items without it
    /// compare by `label` (LSP spec fallback).
    pub sort_text: Option<String>,
    /// Whether accepting this item should produce a callable expression. `None`
    /// means the server/cache did not say either way.
    pub callable: Option<bool>,
    /// For callable completions, whether the call signature has parameters.
    /// `None` means unknown.
    pub has_parameters: Option<bool>,
    pub documentation: Option<String>,
    /// Raw `data` JSON from LSP. Some servers require it to resolve auto-import
    /// edits later via `completionItem/resolve`.
    pub data: Option<String>,
    /// Source path for locally indexed workspace exports.
    pub source_path: Option<PathBuf>,
    /// Display/debug import path for locally indexed workspace exports.
    pub import_path: Option<String>,
    /// Export kind for locally indexed workspace exports. `named` is the only
    /// custom fallback import kind accepted in v1.
    pub export_kind: Option<String>,
    /// Original JSON of the item — needed to round-trip through `completionItem/resolve`.
    /// `None` for items synthesized in tests.
    pub raw_json: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LspDocumentSymbolSegment {
    pub name: String,
    pub kind: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LspDocumentSymbol {
    pub name: String,
    pub kind: String,
    pub range: LspRange,
    pub ancestors: Vec<LspDocumentSymbolSegment>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LspDocumentHighlight {
    pub range: LspRange,
    pub kind: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LspTextEdit {
    pub range: LspRange,
    pub new_text: String,
}

/// Pre-parsed markdown block produced on the worker for hover overlays.
/// `Code` carries Tree-sitter highlight spans (theme-agnostic) so the main
/// thread only does the cheap colour-resolution step on layout.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HoverDocBlock {
    Prose(String),
    Code {
        text: String,
        /// Highlight spans produced by `highlight_snippet` on the worker.
        /// Empty when the language is unknown or the snippet is too long
        /// for inline tree-sitter (mirrors the threshold check used elsewhere).
        spans: Vec<HighlightSpan>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LspCodeAction {
    pub title: String,
    pub edits: Vec<LspTextEdit>,
    /// Raw action JSON for codeAction/resolve when edits are empty but command is present.
    pub raw_action: Option<String>,
}

/// Result worker trả về main thread.
#[derive(Debug, Clone)]
pub struct WorkerResult {
    pub request_id: u64,
    pub revision_id: u64,
    pub topic: RequestTopic,
    pub payload: WorkerResultPayload,
}

#[derive(Debug, Clone)]
pub enum WorkerResultPayload {
    ParseAndHighlight {
        /// Stable identity copied from the request so the main thread can
        /// reconcile async results against the currently active buffer.
        buffer_id: PathBuf,
        file_path: Option<PathBuf>,
        language_id: LanguageId,
        buffer_revision: u64,
        spans: Vec<HighlightSpan>,
        covered_byte_range: Option<Range<usize>>,
        foldable_ranges: Vec<(usize, usize)>,
        line_count: usize,
        char_count: usize,
        byte_count: usize,
        parse_time_ms: u128,
        highlight_time_ms: u128,
    },
    LeetCodeProblemFetched {
        title: String,
        title_slug: String,
        language_key: String,
        file_path: PathBuf,
        cases: Vec<crate::runner::leetcode_api::LeetCodeTestCase>,
    },
    LeetCodeProblemFetchFailed {
        message: String,
    },
    /// `FetchLeetCodeStatement` landed; the cache for `slug` is fresh.
    LeetCodeStatementFetched {
        slug: String,
    },
    LeetCodeStatementFetchFailed {
        slug: String,
        message: String,
    },
    LeetCodeTestsGenerated {
        id: String,
        cases: Vec<crate::runner::leetcode_api::LeetCodeTestCase>,
        verified: bool,
    },
    LeetCodeTestsGenerateFailed {
        message: String,
    },
    ParseSummary {
        file_path: PathBuf,
        line_count: usize,
        char_count: usize,
    },
    SearchMatches {
        query: String,
        matches: Vec<String>,
    },
    CpuBurnSummary {
        job_label: String,
        busy_millis: u64,
        checksum: u64,
    },
    FileSystemEvents {
        root_path: PathBuf,
        events: Vec<FileSystemEvent>,
    },
    PtySpawned {
        session_id: u64,
        shell: String,
        working_dir: PathBuf,
    },
    PtyOutput {
        session_id: u64,
        chunk: Vec<u8>,
    },
    PtyInputWritten {
        session_id: u64,
        bytes: usize,
    },
    DetachedShellCommandSpawned {
        command: String,
        pid: Option<u32>,
    },
    PtyResized {
        session_id: u64,
        cols: u16,
        rows: u16,
    },
    PtySessionClosed {
        session_id: u64,
        exit_status: Option<i32>,
        reason: String,
    },
    LspServerStarted {
        server_name: String,
        root_path: PathBuf,
        /// Extracted từ ServerCapabilities.completionProvider.triggerCharacters.
        /// Đã parse sẵn — app không cần deserialize raw JSON nữa.
        completion_trigger_chars: Vec<char>,
    },
    LspServerStopped {
        exit_status: Option<i32>,
        reason: String,
    },
    LspAck {
        action: String,
        uri: Option<String>,
        version: Option<i32>,
    },
    LspDiagnostics {
        uri: String,
        version: Option<u64>,
        diagnostics: Vec<LspDiagnostic>,
        /// Which server published these. Diagnostics are stored per-server so
        /// pyright and ruff (running together) don't overwrite each other.
        server_name: String,
    },
    LspLogMessage {
        level: String,
        message: String,
    },
    /// Decoded `$/progress` notification (`WorkDoneProgress`). The main
    /// thread renders this on the status bar so the user knows the LSP server
    /// is busy (e.g. rust-analyzer indexing) and avoids spamming requests
    /// that would just queue up.
    LspProgress {
        server_name: String,
        token: String,
        kind: LspProgressKindWire,
        title: Option<String>,
        message: Option<String>,
        percentage: Option<u32>,
    },
    LspCheckResult {
        /// File path gốc được check.
        path: PathBuf,
        /// Tên binary (ví dụ "rust-analyzer").
        binary: String,
        /// Tên ngôn ngữ hiển thị (ví dụ "Rust").
        language_label: String,
        /// Lệnh cài đặt để gợi ý user.
        install_cmd: String,
        /// `true` nếu binary đã có trong $PATH.
        is_installed: bool,
    },
    /// textDocument/hover response.
    LspHoverResult {
        /// Nội dung markdown hoặc plain text từ LSP.
        content: String,
        /// Vị trí cursor tại thời điểm gửi request (dùng để định vị popup).
        cursor_line: usize,
        cursor_col: usize,
        /// Mirrors the request flag — routes result to completion doc panel.
        for_completion: bool,
        /// Echoed from request — only meaningful when `for_completion=true`.
        /// Used by main thread to drop stale fallback-hover results.
        completion_revision: Option<u64>,
        /// Pre-parsed markdown blocks for the **non-completion** hover overlay
        /// (Tree-sitter syntax highlighting computed on the worker so the main
        /// thread doesn't block on heavy parses). `None` for completion path.
        parsed_blocks: Option<Vec<HoverDocBlock>>,
    },
    /// textDocument/definition response.
    LspDefinitionResult {
        locations: Vec<LspLocation>,
        /// Giữ nguyên intent (jump vs. peek) để routing đúng.
        jump: bool,
    },
    /// textDocument/references response.
    LspReferencesResult {
        locations: Vec<LspLocation>,
    },
    /// textDocument/rename response.
    LspRenameResult {
        uri: String,
        edits: Vec<LspTextEdit>,
        other_file_edit_count: usize,
    },
    /// textDocument/documentHighlight response.
    LspDocumentHighlightResult {
        uri: String,
        highlights: Vec<LspDocumentHighlight>,
    },
    /// textDocument/documentSymbol response.
    LspDocumentSymbolsResult {
        uri: String,
        symbols: Vec<LspDocumentSymbol>,
    },
    /// documentSymbol response for a spawned canvas card (correlated by `card_id`).
    /// Empty `symbols` → no enclosing definition → the card keeps its ±N window.
    CanvasCardScopeResult {
        card_id: u64,
        uri: String,
        symbols: Vec<LspDocumentSymbol>,
    },
    /// textDocument/formatting response.
    LspFormattingResult {
        uri: String,
        edits: Vec<LspTextEdit>,
    },
    /// textDocument/completion response.
    LspCompletionResult {
        items: Vec<LspCompletionItem>,
        cursor_line: usize,
        cursor_col: usize,
        prefix_start_col: usize,
        prefix: String,
    },
    /// textDocument/codeAction response.
    LspCodeActionResult {
        actions: Vec<LspCodeAction>,
    },
    /// completionItem/resolve response — augmented detail/documentation for the item
    /// that the client requested resolution for.
    LspCompletionResolveResult {
        item_label: String,
        detail: Option<String>,
        documentation: Option<String>,
        resolved_item: Option<LspCompletionItem>,
        /// Echoed from the request so the main thread can compare against
        /// `CompletionState.current_revision` and drop stale results.
        completion_revision: u64,
    },
    FzfResults {
        query: String,
        mode: FzfSearchMode,
        case_sensitive: bool,
        items: Vec<FzfResultItem>,
    },
    GitBlameLine {
        file_path: PathBuf,
        line_number: usize,
        summary: String,
    },
    WorkspaceGitStatus {
        workspace_root: PathBuf,
        statuses: Vec<(PathBuf, GitFileStatus)>,
    },
    BufferGitBaseline {
        file_path: PathBuf,
        baseline: Option<String>,
    },
    FilePreviewLoaded {
        file_path: PathBuf,
        target_line: Option<usize>,
        lines: Vec<FilePreviewLine>,
    },
    AiInlineCompletionChunk {
        chunk: String,
    },
    AiInlineCompletionResult {
        suggestion: String,
    },
    /// AI re-rank result: the candidate labels in the model's preferred order.
    /// `prefix_token` / `completion_revision` are echoed from the request so the
    /// main thread can drop a result the user has already moved past.
    AiCompletionRerankResult {
        ranked: Vec<String>,
        prefix_token: String,
        completion_revision: u64,
    },
    /// Result of system dependency check: list of tools not found by `which`.
    SystemDepCheckResult {
        missing: Vec<String>,
    },
    /// Kết quả scan Python environments.
    PythonEnvironmentsDiscovered(Vec<crate::async_runtime::python_env::PythonEnv>),
    /// Kết quả scan Dart environments.
    DartEnvironmentsDiscovered(Vec<crate::async_runtime::dart_env::DartEnv>),
    /// Runtime version strings for statusbar display.
    RuntimeVersionsDetected {
        python_version: Option<String>,
        node_version: Option<String>,
        go_version: Option<String>,
    },
    /// Result of async file copy operation.
    FileCopyResult {
        source_path: PathBuf,
        target_path: PathBuf,
        success: bool,
        error_message: Option<String>,
    },
    /// Outcome of `WriteTextFiles`: one (path, error) per failed op.
    TextFilesWritten {
        failures: Vec<(PathBuf, String)>,
    },
    /// Contents of externally changed files, read off the UI thread.
    /// `content: None` means the read failed (deleted/permission/binary).
    ExternalFilesRead {
        files: Vec<ExternalFileRead>,
    },
    /// Fresh workspace tree from an async rescan.
    WorkspaceRescanned {
        root_path: PathBuf,
        nodes: Vec<crate::workspace::model::WorkspaceNode>,
    },
    /// workspace/symbol response — all symbols in the workspace.
    WorkspaceSymbols {
        language_id: String,
        symbols: Vec<crate::lsp::CachedSymbol>,
    },
    /// All test cases finished running. `outcomes` is index-aligned with the
    /// submitted `inputs`; the main thread folds each into its `TestCase` and
    /// judges pass/fail.
    TestCasesCompleted {
        command_preview: String,
        outcomes: Vec<crate::runner::TestCaseOutcome>,
    },
    /// Code Graph HUD model built from codegraph callers/callees/impact.
    CodeGraphReady {
        model: crate::codegraph::model::CodeGraphModel,
    },
    /// Code Graph HUD query failed. `not_installed` distinguishes a missing
    /// `codegraph` binary from a query/parse error.
    CodeGraphFailed {
        not_installed: bool,
        message: String,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkerFailureKind {
    Execution,
    Panic,
    JoinCancelled,
}

#[derive(Debug, Clone)]
pub struct WorkerFailure {
    pub kind: WorkerFailureKind,
    pub message: String,
}

/// Event lifecycle để debug/tracing request.
#[derive(Debug, Clone)]
pub struct WorkerEvent {
    pub request_id: u64,
    pub revision_id: u64,
    pub topic: RequestTopic,
    pub kind: WorkerEventKind,
}

#[derive(Debug, Clone)]
pub enum WorkerEventKind {
    Started,
    Completed,
    Failed { error: WorkerFailure },
    Cancelled { reason: String },
}

/// Message bridge dùng một channel duy nhất từ worker -> app layer.
#[derive(Debug, Clone)]
pub enum WorkerMessage {
    Event(WorkerEvent),
    Result(WorkerResult),
    /// Per-tool progress update during system dependency installation.
    SystemDepToolProgress {
        tool: String,
        status: InstallStatus,
    },
    /// All system dep tools have been processed — installation loop is done.
    SystemDepInstallDone,
    ExtensionCommandStarted {
        binary: String,
        uninstall: bool,
    },
    ExtensionCommandLog {
        binary: String,
        line: String,
    },
    ExtensionCommandFinished {
        binary: String,
        uninstall: bool,
        success: bool,
        exit_code: Option<i32>,
    },
    /// LSP binary was not found on PATH when attempting to spawn the server.
    LspMissingDependency {
        language_id: String,
        tool_name: String,
    },
}

/// RequestSpec giúp caller tạo request mà không cần tự cấp request_id.
#[derive(Debug, Clone)]
pub struct RequestSpec {
    pub revision_id: u64,
    pub topic: RequestTopic,
    pub payload: WorkerRequestPayload,
}

/// One write in a `WorkerRequestPayload::WriteTextFiles` batch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TextFileOp {
    /// Create or overwrite.
    Write { path: PathBuf, contents: String },
    /// Create with `header` if missing, then append `contents`.
    Append {
        path: PathBuf,
        header: String,
        contents: String,
    },
    /// Write only when the file does not exist.
    WriteIfMissing { path: PathBuf, contents: String },
    /// Delete; a missing file is not an error.
    Remove { path: PathBuf },
}
