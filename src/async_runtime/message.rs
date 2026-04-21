use std::{ops::Range, path::PathBuf};

use crate::syntax::{highlight::HighlightSpan, syntax_engine::LanguageId};

/// Topic giúp app biết result thuộc subsystem nào để so revision.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RequestTopic {
    ActiveBufferLayout,
    ProjectSearch,
    BackgroundDemo,
    WorkspaceWatch,
    TerminalPty,
    LspClient,
    FzfSearch,
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

#[derive(Debug, Clone)]
pub enum WorkerRequestPayload {
    ParseAndHighlight {
        file_path: Option<PathBuf>,
        text_snapshot: String,
        language_id: LanguageId,
        buffer_revision: u64,
        viewport_line_start: usize,
        viewport_line_count: usize,
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
    WritePtyInput {
        session_id: u64,
        input: String,
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
    StopLspServer,
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
        chunk: String,
    },
    PtyInputWritten {
        session_id: u64,
        bytes: usize,
    },
    PtySessionClosed {
        session_id: u64,
        exit_status: Option<i32>,
        reason: String,
    },
    LspServerStarted {
        server_name: String,
        root_path: PathBuf,
        capabilities_summary: String,
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
    FzfResults {
        query: String,
        mode: FzfSearchMode,
        items: Vec<FzfResultItem>,
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
}

/// RequestSpec giúp caller tạo request mà không cần tự cấp request_id.
#[derive(Debug, Clone)]
pub struct RequestSpec {
    pub revision_id: u64,
    pub topic: RequestTopic,
    pub payload: WorkerRequestPayload,
}
