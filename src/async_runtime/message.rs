use std::{ops::Range, path::PathBuf};

use serde::{Deserialize, Serialize};

use crate::syntax::{highlight::HighlightSpan, syntax_engine::LanguageId};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GitFileStatus {
    Modified,
    Added,
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
    LspClient,
    LspCheck,
    /// Các LSP interactive requests: hover, definition, references.
    LspRequest,
    FzfSearch,
    FilePreview,
    AiInlineCompletion,
    LocalHistory,
    AiChat,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersistedHistoryEnvelope {
    pub version: u32,
    pub file_path: PathBuf,
    pub history: crate::core::transaction::EditHistory,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileSystemChangeKind {
    Create,
    Delete,
    Modify,
    Rename,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileSystemEvent {
    pub kind: FileSystemChangeKind,
    pub path: PathBuf,
    pub new_path: Option<PathBuf>,
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
    },
    FzfSearch {
        query: String,
        mode: FzfSearchMode,
        workspace_root: PathBuf,
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
    LoadLocalHistory {
        file_path: PathBuf,
    },
    SaveLocalHistory {
        file_path: PathBuf,
        history: PersistedHistoryEnvelope,
        max_bytes: usize,
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
    /// textDocument/documentSymbol request for the active file.
    LspDocumentSymbolsRequest {
        language_id: String,
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
    AiInlineCompletionRequest {
        api_url: String,
        api_key: Option<String>,
        model: String,
        endpoint_kind: Option<String>,
        prefix: String,
        suffix: String,
        language_id: Option<String>,
        file_path: Option<PathBuf>,
        max_tokens: u32,
    },
    AiChatRequest {
        prompt: String,
        buffer_context: String,
        cursor_position: (usize, usize),
        history: Vec<(String, String)>,
    },
    StopLspServer,
    ShutdownAllLspServers,
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
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LspCompletionItem {
    pub label: String,
    pub detail: Option<String>,
    pub insert_text: Option<String>,
    pub text_edit_text: Option<String>,
    pub kind: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LspDocumentSymbol {
    pub name: String,
    pub kind: String,
    pub range: LspRange,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LspTextEdit {
    pub range: LspRange,
    pub new_text: String,
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
        line_count: usize,
        char_count: usize,
        byte_count: usize,
        parse_time_ms: u128,
        highlight_time_ms: u128,
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
    },
    LspLogMessage {
        level: String,
        message: String,
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
    /// textDocument/documentSymbol response.
    LspDocumentSymbolsResult {
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
    FzfResults {
        query: String,
        mode: FzfSearchMode,
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
    LocalHistoryLoaded {
        file_path: PathBuf,
        history: Option<PersistedHistoryEnvelope>,
    },
    LocalHistorySaved {
        file_path: PathBuf,
        bytes_written: usize,
        trimmed_transactions: usize,
    },
    AiInlineCompletionResult {
        suggestion: String,
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
    AiMessageChunk { text: String },
    AiStreamComplete,
    AiStreamError { error: String },
}

/// RequestSpec giúp caller tạo request mà không cần tự cấp request_id.
#[derive(Debug, Clone)]
pub struct RequestSpec {
    pub revision_id: u64,
    pub topic: RequestTopic,
    pub payload: WorkerRequestPayload,
}
