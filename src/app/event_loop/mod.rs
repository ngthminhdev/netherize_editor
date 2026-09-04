use std::sync::Arc;
use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

use tokio_util::sync::CancellationToken;
use winit::{
    application::ApplicationHandler,
    dpi::{LogicalSize, PhysicalPosition, PhysicalSize},
    event::{Ime, WindowEvent},
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop, EventLoopProxy},
    window::{Fullscreen, Window, WindowId},
};

use crate::{
    app::{
        app_state::{AppState, BufferContent, EditorOverlay, OverlayColorToken},
        async_bridge::{AppAsyncBridge, AsyncResultRouter},
        clipboard::SystemClipboard,
        command_palette::{CommandPaletteAction, CommandPaletteMode},
        input::{InputHandler, InputRouteOutcome, LeapState},
        input_map::{InputFocusContext, InputMap, KeybindingContext},
        persistence::{
            AppPersistentState, WindowGeometry, install_panic_recovery_hook,
            replace_panic_recovery_snapshot,
        },
    },
    async_runtime::{
        message::{
            FzfSearchMode, RequestSpec, RequestTopic, SyntaxEditHint, WorkerRequestPayload,
            WorkerResult,
        },
        scheduler::AsyncScheduler,
    },
    config::{
        ai_config::AiConfig,
        keymap_loader::KeymapLoader,
        theme_config::ThemeConfig,
        ui_config::{UiConfig, WindowStartupMode},
    },
    core::{
        command_dispatch::{dispatch_command, dispatch_command_with_clipboard},
        commands::Command,
        mode::{EditorMode, ModeEvent},
    },
    lsp::client::path_to_lsp_uri,
    render::{
        region_pipeline::RegionDrawInstance,
        renderer::{
            RenderError, Renderer, SidebarFilterState, SidebarRow, TopbarTab, TopbarTabKind,
        },
    },
    syntax::{highlight::HighlightSpan, syntax_engine::SyntaxEngine},
    terminal::grid::{HighlightColors, TerminalGrid},
    text::text_system::StyledTextSpan,
    workbench::{
        focus_manager::{FocusManager, FocusTarget},
        layout_engine::WorkbenchLayoutEngine,
        overlay_manager::OverlayManager,
        panel_state::{PanelTabId, WorkbenchPanelState},
        region_model::RegionId,
    },
    workspace::model::{WorkspaceGitStatus, WorkspaceNodeType},
};

mod application;
mod async_results;
mod commands;
mod helpers;
pub mod perf_probe;
mod setup;
mod welcome;
mod workspace_session;

use helpers::{
    build_preview_render_data, build_sidebar_rows, collect_explorer_entries,
    convert_worker_hover_blocks, detect_git_branch, diagnostic_spans_to_styled,
    format_window_title, language_id_for_path, list_git_worktrees, parse_hover_markdown_blocks,
    region_surface_color, scale_theme, scale_ui_config, syntax_spans_to_styled,
};
use welcome::welcome_screen_content;

/// Unified application shell: the single `ApplicationHandler` for the main window.
///
/// Owns and wires together every subsystem:
///
/// ```text
///  Keyboard -> InputHandler -> InputMap -> dispatch_command -> AppState
///  Async results <- AppAsyncBridge <- AsyncScheduler <- Worker threads
///  Every frame: LayoutEngine -> Renderer (GPU)
/// ```
pub struct AppShell {
    app_state: AppState,
    persistent_state: AppPersistentState,
    /// Parked workspace sessions (the active one lives in the fields below).
    background_sessions: Vec<workspace_session::WorkspaceSession>,
    /// File-system events restored with a session, replayed on activation.
    pending_fs_events_to_drain: Vec<crate::async_runtime::message::FileSystemEvent>,
    input_handler: InputHandler,
    input_map: InputMap,
    scheduler: AsyncScheduler,
    bridge: Option<AppAsyncBridge>,
    right_pty_session_id: Option<u64>,
    right_terminal_grid: TerminalGrid,
    right_terminal_needs_layout: bool,
    last_right_terminal_bounds: Option<[f32; 4]>,
    last_cursor_position: Option<(f32, f32)>,
    /// Previous press in the GPU-drawn titlebar, used to recognize the native
    /// double-click-to-zoom gesture without treating content clicks as chrome.
    last_titlebar_click: Option<(Instant, (f32, f32))>,
    /// A titlebar press that has not yet crossed the native-drag threshold.
    titlebar_drag_origin: Option<(f32, f32)>,
    /// Fractional mouse-wheel carry for the bottom-dock terminal scrollback.
    /// Wheels (esp. trackpads) emit many sub-line pixel deltas; we scale them
    /// down for gentler scrolling and carry the remainder so slow scrolls still
    /// register. Reset to 0 when the scroll direction flips.
    bottom_terminal_wheel_accum: f64,
    /// Fractional mouse-wheel carry for the right-dock terminal (forwarded as
    /// SGR wheel events). Same gentling/carry scheme as the bottom dock.
    right_terminal_wheel_accum: f64,
    /// Active pointer drag (panel resize / card move / card resize), if any.
    active_drag: Option<crate::workbench::pointer_drag::ActiveDrag>,
    /// What draggable zone the cursor is currently hovering (for cursor shape +
    /// highlight). Recomputed on every `CursorMoved` while not dragging.
    hover_target: Option<crate::workbench::pointer_drag::HoverTarget>,
    /// Last OS cursor icon pushed to the window, so we only call `set_cursor`
    /// when the shape actually changes (not on every `CursorMoved`).
    last_cursor_icon: Option<winit::window::CursorIcon>,
    /// Topbar tab index currently under the pointer (identity of the hover
    /// wash + Pointer cursor). Tracked here so redraws only happen when the
    /// hovered tab actually changes.
    last_topbar_hover: Option<usize>,
    /// Left/right dock tab index under the pointer — same identity role as
    /// `last_topbar_hover`, one slot per dock strip.
    last_left_dock_hover: Option<usize>,
    last_right_dock_hover: Option<usize>,
    /// Bottom-dock terminal tab index under the pointer (`term_count` when on
    /// the "+" button). Drives the live strip's hover wash + Pointer cursor.
    last_bottom_dock_hover: Option<usize>,
    /// Last left-dock hover index actually baked into the rendered strip. The
    /// left-dock panel only re-renders on layout/focus/tab changes, so this
    /// comparison tells it when a hover change still needs a repaint.
    rendered_left_dock_hover: Option<usize>,
    pending_right_pty_spawn: bool,
    /// Label of the AI agent currently launched in the right-dock terminal
    /// (e.g. "opencode"), for display. `None` when no agent is running.
    right_agent_label: Option<String>,
    /// Selection index in the in-panel AI-agent picker (AI Chat tab, no agent).
    ai_agent_picker_selected: usize,
    /// Last text-editing command recorded for `.` (RepeatLastChange) replay.
    last_edit_command: Option<Command>,
    /// Char index where an editor text drag started (mouse selection).
    editor_text_drag_anchor: Option<usize>,
    /// Previous editor-text press for the double-click word-select gesture.
    last_editor_text_click: Option<(std::time::Instant, (f32, f32))>,
    /// When `Some`, this command string will be written into the right PTY
    /// immediately after `PtySpawned` is received for the right terminal.
    right_pty_startup_command: Option<String>,
    terminal_buffer_grids: HashMap<u64, TerminalGrid>,
    pending_lazygit_buffer_index: Option<usize>,
    pending_lazydocker_buffer_index: Option<usize>,
    highlight_spans: Vec<HighlightSpan>,
    semantic_highlight_spans: Vec<HighlightSpan>,
    cached_document_symbols_path: Option<PathBuf>,
    cached_document_symbols: Vec<crate::async_runtime::message::LspDocumentSymbol>,
    /// File the Outline panel last requested symbols for, so it fetches once per
    /// file instead of every frame the Outline tab is shown.
    outline_fetch_path: Option<PathBuf>,
    /// Selected index in the Outline list when navigating via keyboard.
    outline_selected: Option<usize>,
    /// Interview-prep Dojo: plan, problem list, state, panel cursor, session.
    dojo: commands::commands_dojo::DojoRuntime,
    syntax_engine: Option<SyntaxEngine>,
    syntax_engine_file: Option<PathBuf>,
    /// Bottom-panel terminal tabs. Always non-empty when the panel is open.
    terminal_tabs: Vec<TerminalTab>,
    active_terminal_tab: usize,
    /// Spawn request id → target bottom-panel tab index.
    ///
    /// PTY spawn completes asynchronously, so binding `PtySpawned` to the
    /// currently active tab is racy if the user creates/switches tabs before
    /// the worker replies.
    pending_terminal_tab_spawns: HashMap<u64, usize>,
    /// Spawn requests that belonged to terminal tabs reset during a workspace
    /// switch. If the worker reports them later, close the newborn PTY instead
    /// of binding it into the new workspace's terminal tab ring.
    ignored_terminal_tab_spawns: HashSet<u64>,
    explorer_cursor: usize,
    explorer_snapshot: ExplorerSnapshot,
    explorer_snapshot_dirty: bool,
    /// Path of the file copied via ExplorerCopyFile command.
    explorer_clipboard_path: Option<PathBuf>,
    pending_paste_source_path: Option<PathBuf>,
    pending_paste_target_dir: Option<PathBuf>,
    pending_confirmation: Option<PendingConfirmation>,
    /// Set after a clean close request or an explicit save/discard response.
    /// `about_to_wait` performs the actual event-loop exit after state is flushed.
    exit_requested: bool,
    /// Last time the panic-recovery snapshot was refreshed (throttled — see
    /// about_to_wait; cloning whole buffers every tick ballooned RSS).
    last_recovery_snapshot_at: Instant,
    /// Env-gated benchmark probe (NETH_PERF_PROBE=1) — inert otherwise.
    perf_probe: perf_probe::PerfProbe,
    window_geometry_dirty: bool,
    last_window_geometry_change: Option<Instant>,
    workspace_git_branch: Option<String>,
    active_lsp_server: Option<ActiveLspServer>,
    pending_lsp_server: Option<ActiveLspServer>,
    lsp_completion_trigger_chars: Vec<char>,
    /// Popup hướng dẫn cài LSP — `Some` khi binary chưa cài, `None` khi đã dismiss.
    active_lsp_guide: Option<LspInstallGuide>,
    /// Các binary LSP mà user đã dismiss guide — không show lại trong session này.
    dismissed_lsp_binaries: HashSet<String>,
    /// Popup for missing system CLI tools — `Some` when tools are missing.
    active_system_dep_guide: Option<SystemDepGuide>,
    /// `true` after user dismissed the system dep popup this session.
    dismissed_system_deps: bool,
    /// Toast window-relative ngắn hạn cho các action nền.
    transient_toast: Option<TransientToast>,
    /// True once the which-key redraw for the current pending chord was
    /// scheduled; resets when the chord resolves or is abandoned.
    whichkey_redraw_fired: bool,
    theme_picker_original_theme: Option<ThemeConfig>,
    theme_picker_preview_profile: Option<String>,
    base_theme: ThemeConfig,
    theme: ThemeConfig,
    ui_config: UiConfig,
    ai_config: AiConfig,
    git_config: crate::config::git_config::GitConfig,
    runtime_scale: f32,
    layout_engine: WorkbenchLayoutEngine,
    panel_state: WorkbenchPanelState,
    focus_manager: FocusManager,
    pub overlay_manager: OverlayManager,
    clipboard: SystemClipboard,
    window: Option<Arc<Window>>,
    /// Last title pushed to the OS window, so per-frame refreshes skip the
    /// syscall when nothing changed.
    last_window_title: Option<String>,
    renderer: Option<Renderer>,
    window_size: PhysicalSize<u32>,
    editor_needs_layout: bool,
    editor_caret_needs_layout: bool,
    sidebar_needs_layout: bool,
    terminal_needs_layout: bool,
    buffer_terminal_needs_layout: bool,
    /// Active workbench layout slide (dock toggle / zen). `None` when settled.
    panel_transition: Option<crate::workbench::motion::LayoutTransition>,
    /// The authoritative (non-animated) layout currently on screen, used as the
    /// `from` snapshot when a new transition starts.
    last_committed_layout: Option<crate::workbench::layout_engine::WorkbenchLayout>,
    /// Active command-palette enter/leave motion (fade + pop). `None` when settled.
    palette_motion: Option<crate::workbench::motion::OverlayMotion>,
    /// Whether the command palette was rendered last frame, to detect the open edge.
    palette_was_visible: bool,
    /// Track whether the InFileSearch palette was opened from terminal context
    /// (via `TerminalSearchOpen`), so focus returns to the terminal when the
    /// palette closes instead of the center editor.
    terminal_search_palette_active: bool,
    last_frame_time: Instant,
    last_fps_metrics_update_at: Instant,
    accumulated_frame_time: Duration,
    accumulated_frame_count: u32,
    current_fps_metrics: String,
    last_parse_submit_at: Option<Instant>,
    last_git_diff_recalc_at: Option<Instant>,
    /// Edit hint for the next incremental tree-sitter parse.
    /// Set to `Some` when exactly one edit occurred since the last reconcile.
    /// Set to `None` when multiple edits accumulated (debounced typing, undo/redo,
    /// paste) — the worker falls back to a full reparse in that case.
    last_syntax_edit_hint: Option<SyntaxEditHint>,
    active_highlight_request_revision: u64,
    semantic_highlight_request_revision: u64,
    references_request_revision: u64,
    document_symbols_request_revision: u64,
    lsp_rename_request_revision: u64,
    /// In-flight `completionItem/resolve` request id; used to correlate failure events
    /// back to the pending docs panel so we can flip "Loading…" to "No docs" when the
    /// server rejects or times out.
    completion_resolve_request_id: Option<u64>,
    /// Latest in-flight `textDocument/completion` request. Clearing this on Esc
    /// makes late LSP responses no-ops instead of reopening a stale popup.
    active_lsp_completion_request_id: Option<u64>,
    /// Request id of the last in-flight hover — used to clear the loading overlay
    /// when the request fails or returns empty (so the overlay doesn't get stuck).
    hover_loading_request_id: Option<u64>,
    /// Request id of the latest hover request. Stale responses (from old cursor positions)
    /// are dropped to prevent overlays appearing after the user has moved/edited.
    latest_hover_request_id: Option<u64>,
    /// Request id of the last in-flight `gd`/`gD` definition request. When a
    /// new request is dispatched, the previous one's result must be dropped on
    /// arrival to avoid a flicker (or a wrong jump) if the server replies out
    /// of order.
    latest_definition_request_id: Option<u64>,
    /// In-flight LSP request ids whose results should populate the NetherCanvas
    /// (definition → Definition block, references → Caller blocks) instead of the
    /// normal peek/references-buffer flow.
    canvas_def_request_id: Option<u64>,
    canvas_refs_request_id: Option<u64>,
    /// Set when F8 opened the canvas before the LSP server was ready: the
    /// source-function fetch is deferred and fired by the `LspServerReady`
    /// handler so the canvas never spins on "Loading…" forever.
    canvas_def_deferred: bool,
    /// The card a pending canvas def/refs request was spawned FROM (in-card
    /// `gd`/`gr`), so the resulting cards record their parent + the connector is
    /// drawn from that card. `None` when spawned from the focal symbol.
    canvas_def_parent: Option<crate::canvas::BlockId>,
    canvas_refs_parent: Option<crate::canvas::BlockId>,
    /// The card file we registered with the LSP (`didOpen`) for the active in-card
    /// edit session, so `gd`/`gr`/completion resolve against it. `None` when no
    /// card doc is ours to manage (the file is the active buffer / already open).
    /// Cleared on `didClose`. (In-card LSP Phase 1: document lifecycle.)
    canvas_card_lsp_open: Option<PathBuf>,
    /// Monotonic LSP document version for the card doc above (separate from the
    /// main buffer revision); bumped on each `didChange`.
    canvas_card_lsp_version: i32,
    /// In-flight in-card `K` hover request id; its result fills the card overlay
    /// (Phase 2 in-card LSP) instead of the main editor's FloatingBox.
    canvas_hover_request_id: Option<u64>,
    /// In-flight in-card completion request id; its result fills the card
    /// completion menu (Phase 3 in-card LSP), not the main editor's.
    canvas_completion_request_id: Option<u64>,
    /// Card-scoped completion state (the source of truth for navigate/accept while
    /// editing a card); mirrored to `CanvasState.card_overlay` for rendering. Kept
    /// separate from `app_state.completion` so the main editor is untouched.
    canvas_completion: Option<crate::app::app_state::CompletionState>,
    /// Request id of the latest `textDocument/rename`; stale responses are dropped.
    latest_rename_request_id: Option<u64>,
    fzf_search_revision: u64,
    pending_parse_after_debounce: bool,
    pending_git_diff_after_debounce: bool,
    /// Set when the user changes the selected completion item; the timer is
    /// drained from `about_to_wait` after `COMPLETION_RESOLVE_DEBOUNCE_INTERVAL`,
    /// at which point the actual LSP resolve is dispatched.
    pending_completion_resolve_after_debounce: bool,
    last_completion_resolve_select_at: Option<Instant>,
    /// Revision (matching `CompletionState.current_revision`) that the pending
    /// debounced resolve was queued for; used to skip dispatch if the user
    /// has already moved on, but mainly to tag the eventual request payload.
    pending_completion_resolve_revision: u64,
    /// Set when Enter was pressed on an unresolved LSP completion item. The
    /// resolve response should merge import edits first, then accept exactly
    /// this item once.
    pending_completion_accept_after_resolve: Option<(String, u64)>,
    pending_lsp_completion_after_debounce: bool,
    last_lsp_completion_type_at: Option<Instant>,
    ai_inline_revision: u64,
    pending_ai_inline_request: Option<PendingAiInlineRequest>,
    ai_inline_cancel_token: Option<CancellationToken>,
    /// Cancels an in-flight AI completion-rerank request when a newer completion
    /// popup supersedes it.
    ai_rerank_cancel_token: Option<CancellationToken>,
    last_ai_inline_submit_at: Option<Instant>,
    /// Set when the current typing command consumed the head of the visible
    /// ghost text (prefix match) — the post-edit hook must keep the retained
    /// suggestion instead of queueing a new request.
    ai_inline_suggestion_retained: bool,
    /// True between submitting an inline request and its result/failure;
    /// drives the status-bar AI indicator.
    ai_inline_inflight: bool,
    /// Consecutive non-cancelled inline failures; at the toast threshold the
    /// feature cools down instead of failing silently forever.
    ai_inline_failure_streak: u32,
    ai_inline_cooldown_until: Option<Instant>,
    /// (buffer, caret) the inline pipeline — pending debounce, in-flight
    /// request, or visible ghost text — is valid for. When the caret leaves
    /// this position other than by consuming the suggestion, the pipeline is
    /// cancelled and the ghost cleared so a late result can't appear at (and
    /// follow) the new caret position.
    ai_inline_anchor: Option<(Option<PathBuf>, usize)>,
    pending_lsp_document_sync: Option<PendingLspDocumentSync>,
    last_editor_bounds: Option<[f32; 4]>,
    last_show_welcome: Option<bool>,
    last_sidebar_bounds: Option<[f32; 4]>,
    last_sidebar_focused: Option<bool>,
    last_left_active_tab: Option<crate::workbench::panel_state::PanelTabId>,
    last_terminal_bounds: Option<[f32; 4]>,
    last_buffer_terminal_bounds: Option<[f32; 4]>,
    sidebar_selection_quads: Vec<RegionDrawInstance>,
    suppress_next_palette_ime_commit: bool,
    /// Leap/EasyMotion state hiện tại cho active editor viewport.
    /// `typed_prefix` giữ các phím user đã gõ, `targets` giữ labels + char_idx.
    leap_state: Option<LeapState>,
    git_overlay_revision: u64,
    git_status_revision: u64,
    git_baseline_revision: u64,
    last_scroll_animation_tick: Instant,
    /// Smooth-scroll tween bookkeeping. `scroll_anim_started_at` is `Some` while a
    /// scroll animation is running; `scroll_anim_start` is the position it began
    /// from (after the far-jump clamp); `scroll_anim_last_target` is the target the
    /// tick last reacted to, so a command changing `target_scroll_y` retargets a
    /// fresh tween. Kept on `AppShell` (not `AppState`) so buffer snapshots and the
    /// canvas edit-session swap are untouched.
    scroll_anim_started_at: Option<Instant>,
    scroll_anim_start: f32,
    scroll_anim_last_target: f32,
    /// Distance-scaled duration chosen when the current tween (re)started.
    scroll_anim_duration: Duration,
    /// Caret coupling: `caret_visual_current` is the caret's displayed visual line
    /// (maintained every frame, like `current_scroll_y`); `caret_anim_start` is
    /// where the caret tween began (after the far clamp). The renderer reads the
    /// resulting `app_state.caret_scroll_lag = caret_visual_current - cursor_visual`.
    caret_anim_start: f32,
    caret_visual_current: f32,
    /// Whole-buffer text cached across scroll-only frames so a smooth-scroll tween
    /// doesn't re-clone the entire rope every frame. Keyed on `(revision, active
    /// file)`; the viewport-scoped styled spans are still rebuilt each frame, so
    /// async highlight refreshes after a scroll are never stale.
    cached_editor_text: String,
    cached_editor_text_key: Option<(u64, usize, Option<PathBuf>)>,
    /// One-shot: set by an explicit scroll command (zz/gg/G/Ctrl-D/Ctrl-U) so the
    /// next `advance_scroll_anim` retarget animates even when the delta is below
    /// the snap threshold. Read via `std::mem::take`, so it cannot leak into a
    /// later `j`/`k` cursor-follow retarget.
    scroll_anim_force: bool,
    last_git_branch_refresh_at: Instant,
    last_workspace_git_status_refresh_at: Instant,
    last_lsp_loading_animation_tick: Instant,
    lsp_loading_frame: u8,
    caret_blink_visible: bool,
    caret_blink_dirty: bool,
    last_caret_blink_tick: Instant,
    last_external_file_check: Instant,
    last_external_file_check_times:
        std::collections::HashMap<std::path::PathBuf, std::time::SystemTime>,
    /// #5: thư mục cha của các file mở NGOÀI workspace root đã được gắn watcher
    /// (non-recursive). Dùng để dedup, tránh spawn watcher trùng cho cùng một dir.
    externally_watched_dirs: std::collections::HashSet<std::path::PathBuf>,
    pre_markdown_preview_right_width: Option<f32>,
    /// Code actions từ lần request gần nhất, dùng để apply khi user chọn trong picker.
    pending_code_actions: Vec<crate::async_runtime::message::LspCodeAction>,
    /// Python interpreter path selected by the user from the command palette.
    selected_python_env: Option<std::path::PathBuf>,
    selected_dart_env: Option<std::path::PathBuf>,
    /// Cached runtime version strings for the statusbar right zone.
    runtime_versions: RuntimeVersionInfo,
    /// Scheduled instant to auto-retry LSP server start after user accepted an install guide.
    lsp_retry_at: Option<Instant>,
    /// Set when the user requests an LSP restart: the running session(s) are
    /// shut down first, and the fresh server is spawned only once the
    /// `LspServerStopped` result lands (LSP requests run concurrently, so
    /// respawning in the same turn would race the shutdown's drain).
    pending_lsp_restart: bool,
}

const DEBUG_UI_ENABLED: bool = false;
const PARSE_DEBOUNCE_INTERVAL: Duration = Duration::from_millis(20);
const GIT_DIFF_DEBOUNCE_INTERVAL: Duration = Duration::from_millis(80);
/// User must dwell on a completion item for at least this long before we fire
/// `completionItem/resolve`. Prevents spamming the LSP server while the user
/// scrolls quickly through items with arrow keys.
const COMPLETION_RESOLVE_DEBOUNCE_INTERVAL: Duration = Duration::from_millis(100);
const LSP_COMPLETION_DEBOUNCE_INTERVAL: Duration = Duration::from_millis(200);
const LSP_DIAGNOSTIC_DEBOUNCE_INTERVAL: Duration = Duration::from_millis(500);
const FPS_METRICS_UPDATE_INTERVAL: Duration = Duration::from_millis(500);
const GIT_BRANCH_REFRESH_INTERVAL: Duration = Duration::from_millis(750);
const GIT_STATUS_REFRESH_INTERVAL: Duration = Duration::from_millis(750);
const LSP_LOADING_ANIMATION_INTERVAL: Duration = Duration::from_millis(100);

#[derive(Debug, Clone)]
struct ExplorerEntry {
    path: PathBuf,
    parent_path: Option<PathBuf>,
    file_type: WorkspaceNodeType,
    depth: usize,
    is_expanded: bool,
    name: String,
    git_status: Option<WorkspaceGitStatus>,
    is_hidden: bool,
    is_ignored: bool,
}

#[derive(Debug, Clone, Default)]
struct ExplorerSnapshot {
    entries: Vec<ExplorerEntry>,
}

#[derive(Debug, Clone)]
enum PendingConfirmationAction {
    Delete {
        path: PathBuf,
        file_type: WorkspaceNodeType,
    },
    CloseDirtyBuffer {
        path: Option<PathBuf>,
    },
    QuitDirtyBuffers {
        count: usize,
    },
    ExternalOverwrite {
        path: PathBuf,
    },
    /// Session close requested while it has unsaved edits: y = save all
    /// first, n = discard and close, Esc = keep it open.
    WorkspaceClose {
        root: PathBuf,
        dirty_count: usize,
    },
}

#[derive(Debug, Clone)]
struct PendingConfirmation {
    action: PendingConfirmationAction,
    return_focus: FocusTarget,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ActiveLspServer {
    server_name: String,
    root_path: PathBuf,
}

/// Cached runtime version strings shown in the statusbar right zone.
#[derive(Debug, Clone, Default)]
pub(super) struct RuntimeVersionInfo {
    pub(super) python_version: Option<String>,
    pub(super) node_version: Option<String>,
    pub(super) go_version: Option<String>,
    /// Display name of the selected Python venv (e.g. "venv", ".venv/py3.11").
    pub(super) venv_name: Option<String>,
}

/// State cho popup hướng dẫn cài đặt Language Server.
#[derive(Debug, Clone, PartialEq, Eq)]
struct LspInstallGuide {
    /// Binary name (ví dụ "rust-analyzer").
    binary: String,
    /// Lệnh cài đặt sẽ được bơm thẳng vào terminal khi user bấm Enter.
    install_cmd: String,
}

/// State for the system dependency checker popup.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SystemDepGuide {
    /// Current phase of the popup.
    pub state: SystemDepState,
    /// Human-readable tool names that are missing, e.g. ["fzf", "lazygit"].
    pub missing_tools: Option<Vec<String>>,
    /// Full install command suitable for the detected package manager.
    pub install_command: Option<String>,
    /// Per-tool install progress tracked during the Installing phase.
    pub tool_statuses: Vec<(String, crate::async_runtime::message::InstallStatus)>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SystemDepState {
    /// Tool(s) detected missing — user can install or skip.
    Detected,
    /// Installation worker is running.
    Installing,
    /// Installation finished — user needs to source rc and restart.
    Complete,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToastKind {
    Info,
    Warning,
    Error,
    Success,
}

#[derive(Debug, Clone)]
struct TransientToast {
    message: String,
    kind: ToastKind,
    created_at: Instant,
    expires_at: Instant,
}

impl TransientToast {
    fn progress_fraction(&self, now: Instant) -> f32 {
        let total = self.expires_at.saturating_duration_since(self.created_at);
        if total.is_zero() {
            return 0.0;
        }
        let remaining = self.expires_at.saturating_duration_since(now);
        (remaining.as_secs_f32() / total.as_secs_f32()).clamp(0.0, 1.0)
    }
}

#[derive(Debug, Clone)]
struct PendingAiInlineRequest {
    revision: u64,
    queued_at: Instant,
}

#[derive(Debug, Clone)]
struct PendingLspDocumentSync {
    path: PathBuf,
    revision: u64,
    queued_at: Instant,
}

#[derive(Debug, Clone)]
pub enum AppEvent {
    TerminalOutputReady,
    AiInlineReady,
    WorkerMessageReady,
    /// A second launch forwarded its CLI open request to this instance.
    RemoteOpen(Vec<PathBuf>),
}

/// Trạng thái của một terminal tab.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum TerminalTabStatus {
    /// PTY session đang chạy.
    Running,
    /// PTY session đã kết thúc với exit code.
    Exited(i32),
}

/// Một tab terminal trong bottom panel.
#[derive(Clone)]
pub(super) struct TerminalTab {
    pub grid: TerminalGrid,
    pub session_id: Option<u64>,
    pub label: String,
    pub status: TerminalTabStatus,
    pub shell_label: String,
    pub pending_input: String,
}

impl std::fmt::Debug for TerminalTab {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TerminalTab")
            .field("session_id", &self.session_id)
            .field("label", &self.label)
            .field("status", &self.status)
            .field("shell_label", &self.shell_label)
            .finish_non_exhaustive()
    }
}

impl TerminalTab {
    fn new(grid: TerminalGrid, label: String) -> Self {
        Self {
            grid,
            session_id: None,
            label: label.clone(),
            status: TerminalTabStatus::Running,
            shell_label: label,
            pending_input: String::new(),
        }
    }
}

pub fn run() -> Result<(), winit::error::EventLoopError> {
    install_panic_recovery_hook();

    // Launch-from-terminal convenience: re-exec ourselves detached so the
    // shell prompt returns right away instead of the GUI process holding the
    // terminal until quit. Skipped for dock/Finder launches (no tty) and for
    // the detached child itself (env guard).
    #[cfg(unix)]
    if crate::app::single_instance::reexec_detached_from_terminal() {
        return Ok(());
    }

    // Single-instance routing: hand our CLI paths to an already-running
    // instance (one dock icon, workspace switches in place) unless the user
    // explicitly asked for a separate process.
    // A running instance of a DIFFERENT build (typically: `cargo run` after
    // a rebuild while the old window is still open) is never forwarded to —
    // the user would keep looking at the old code. Start alongside it.
    let mut stale_instance_running = false;
    #[cfg(unix)]
    let build_stamp = crate::app::single_instance::build_stamp();
    #[cfg(unix)]
    if !std::env::args().any(|arg| arg == "--new-instance") {
        use crate::app::single_instance::Forward;
        let paths = crate::app::single_instance::cli_open_paths();
        let sock = crate::app::single_instance::default_socket_path();
        match crate::app::single_instance::try_forward_at(&sock, &paths, build_stamp) {
            Forward::Acked => {
                println!("[netherize] open request forwarded to the running instance");
                return Ok(());
            }
            Forward::Stale => {
                eprintln!(
                    "[netherize] a running instance is a different (older) build — starting this build alongside it; quit the old window (Cmd+Q) when done"
                );
                stale_instance_running = true;
            }
            Forward::NoInstance => {}
        }
    }

    let event_loop = EventLoop::<AppEvent>::with_user_event().build()?;
    event_loop.set_control_flow(ControlFlow::Wait);
    let event_proxy = event_loop.create_proxy();

    #[cfg(unix)]
    {
        let sock = crate::app::single_instance::default_socket_path();
        if let Some(listener) = crate::app::single_instance::bind_at(&sock) {
            let remote_proxy = event_loop.create_proxy();
            crate::app::single_instance::spawn_listener(listener, build_stamp, move |paths| {
                let _ = remote_proxy.send_event(AppEvent::RemoteOpen(paths));
            });
        }
    }

    let mut app = match AppShell::new(event_proxy) {
        Ok(mut app) => {
            if stale_instance_running {
                app.show_transient_toast_kind(
                    "Older Netherize build still running\nThis window is the new build. Quit the old one (Cmd+Q) when you are done there.",
                    crate::app::event_loop::ToastKind::Warning,
                );
            }
            app
        }
        Err(err) => {
            eprintln!("[fatal] AppShell::new failed: {err}");
            return Ok(());
        }
    };

    event_loop.run_app(&mut app)
}

impl AppShell {
    fn should_show_welcome(&self) -> bool {
        self.app_state.is_initial_launch_welcome()
            && self.app_state.buffers().is_empty()
            && (!self.app_state.is_command_palette_visible()
                || self.app_state.command_palette_mode()
                    == Some(CommandPaletteMode::RecentProjects))
    }

    fn arm_palette_ime_commit_suppression(&mut self) {
        self.suppress_next_palette_ime_commit = true;
    }

    fn clear_palette_ime_commit_suppression(&mut self) {
        self.suppress_next_palette_ime_commit = false;
    }

    fn note_post_open_keyboard_press(&mut self) {
        if self.suppress_next_palette_ime_commit {
            self.clear_palette_ime_commit_suppression();
        }
    }

    fn should_swallow_palette_ime_commit(&mut self) -> bool {
        let should_swallow = self.suppress_next_palette_ime_commit
            && self.app_state.is_command_palette_visible()
            && self.app_state.current_mode() == EditorMode::PaletteFocus
            && self.app_state.command_palette_query_text().is_empty();
        if should_swallow {
            self.clear_palette_ime_commit_suppression();
        }
        should_swallow
    }

    fn invalidate_editor_overlays(&mut self) -> bool {
        self.git_overlay_revision = self.git_overlay_revision.saturating_add(1);
        let changed = self.app_state.clear_current_overlays();
        if changed {
            self.editor_needs_layout = true;
            self.editor_caret_needs_layout = false;
        }
        changed
    }

    fn active_terminal_grid_mut(&mut self) -> Option<&mut TerminalGrid> {
        let session_id = self.app_state.active_terminal_session_id()?;
        self.terminal_buffer_grids.get_mut(&session_id)
    }

    fn focused_terminal_grid_mut(&mut self) -> Option<&mut TerminalGrid> {
        if self.app_state.active_buffer_is_terminal()
            && self.focus_manager.current() == FocusTarget::CenterEditor
        {
            return self.active_terminal_grid_mut();
        }
        if self.focus_manager.current() == FocusTarget::BottomPanel {
            return self.active_terminal_tab_mut().map(|tab| &mut tab.grid);
        }
        if self.focus_manager.current() == FocusTarget::RightSidebar
            && (self.panel_state.right.active_tab_id() == Some(PanelTabId::Terminal)
                || (self.panel_state.right.active_tab_id() == Some(PanelTabId::AiChat)
                    && self.right_pty_session_id.is_some()))
        {
            return Some(&mut self.right_terminal_grid);
        }
        None
    }

    fn focused_terminal_session_id(&self) -> Option<u64> {
        if self.app_state.active_buffer_is_terminal()
            && self.focus_manager.current() == FocusTarget::CenterEditor
        {
            return self.app_state.active_terminal_session_id();
        }
        if self.focus_manager.current() == FocusTarget::BottomPanel {
            return self.active_terminal_tab().and_then(|tab| tab.session_id);
        }
        if self.focus_manager.current() == FocusTarget::RightSidebar
            && matches!(
                self.panel_state.right.active_tab_id(),
                Some(PanelTabId::Terminal) | Some(PanelTabId::AiChat)
            )
        {
            // `right_pty_session_id` is None when no agent is running, so this
            // naturally yields None on the AiChat tab's picker state.
            return self.right_pty_session_id;
        }
        None
    }

    fn active_terminal_tab(&self) -> Option<&TerminalTab> {
        self.terminal_tabs.get(self.active_terminal_tab)
    }

    fn active_terminal_tab_mut(&mut self) -> Option<&mut TerminalTab> {
        let idx = self.active_terminal_tab;
        self.terminal_tabs.get_mut(idx)
    }

    fn sync_focus_mode_for_active_buffer(&mut self) -> bool {
        let mut changed = false;
        if self.app_state.active_buffer_is_terminal() {
            if self.app_state.current_mode() == EditorMode::PaletteFocus {
                changed |= self.app_state.close_command_palette();
                if let Ok(result) = self.app_state.apply_mode_event(ModeEvent::ExitFocus) {
                    changed |= result.changed;
                }
            }
            if !matches!(
                self.app_state.current_mode(),
                EditorMode::TerminalFocus | EditorMode::TerminalNormal
            ) && let Ok(result) = self.app_state.apply_mode_event(ModeEvent::FocusTerminal)
            {
                changed |= result.changed;
            }
            let focus_changed = self.focus_manager.set(FocusTarget::CenterEditor);
            changed |= focus_changed;
            if focus_changed {
                self.input_handler.clear_pending_prefix();
            }
        } else if self.app_state.active_buffer_is_settings()
            || self.app_state.active_buffer_is_extensions_manager()
        {
            if matches!(
                self.app_state.current_mode(),
                EditorMode::TerminalFocus | EditorMode::TerminalNormal | EditorMode::PaletteFocus
            ) {
                changed |= self.app_state.close_command_palette();
                if let Ok(result) = self.app_state.apply_mode_event(ModeEvent::ExitFocus) {
                    changed |= result.changed;
                }
            }
            let focus_changed = self.focus_manager.set(FocusTarget::CenterEditor);
            changed |= focus_changed;
            if focus_changed {
                self.input_handler.clear_pending_prefix();
            }
        } else {
            if matches!(
                self.app_state.current_mode(),
                EditorMode::TerminalFocus | EditorMode::TerminalNormal
            ) && let Ok(result) = self.app_state.apply_mode_event(ModeEvent::ExitFocus)
            {
                changed |= result.changed;
            }
            let focus_changed = self.focus_manager.set(FocusTarget::CenterEditor);
            changed |= focus_changed;
            if focus_changed {
                self.input_handler.clear_pending_prefix();
            }
        }
        changed
    }
}
