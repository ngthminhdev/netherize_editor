use std::sync::Arc;
use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

use winit::{
    application::ApplicationHandler,
    dpi::{LogicalSize, PhysicalSize},
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
        persistence::AppPersistentState,
    },
    async_runtime::{
        message::{
            FzfSearchMode, RequestSpec, RequestTopic, SyntaxEditHint, WorkerEvent,
            WorkerRequestPayload, WorkerResult, WorkerResultPayload,
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
mod setup;
mod welcome;

use helpers::{
    build_preview_render_data, build_sidebar_rows, collect_explorer_entries,
    convert_worker_hover_blocks, detect_git_branch, diagnostic_spans_to_styled,
    language_id_for_path, parse_hover_markdown_blocks, region_color, scale_theme, scale_ui_config,
    syntax_spans_to_styled,
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
    input_handler: InputHandler,
    input_map: InputMap,
    scheduler: AsyncScheduler,
    bridge: Option<AppAsyncBridge>,
    pty_session_id: Option<u64>,
    right_pty_session_id: Option<u64>,
    right_terminal_grid: TerminalGrid,
    right_terminal_needs_layout: bool,
    last_right_terminal_bounds: Option<[f32; 4]>,
    pending_right_pty_spawn: bool,
    terminal_buffer_grids: HashMap<u64, TerminalGrid>,
    pending_lazygit_buffer_index: Option<usize>,
    pending_lazydocker_buffer_index: Option<usize>,
    highlight_spans: Vec<HighlightSpan>,
    semantic_highlight_spans: Vec<HighlightSpan>,
    syntax_engine: Option<SyntaxEngine>,
    syntax_engine_file: Option<PathBuf>,
    terminal_grid: TerminalGrid,
    explorer_cursor: usize,
    explorer_snapshot: ExplorerSnapshot,
    explorer_snapshot_dirty: bool,
    pending_confirmation: Option<PendingConfirmation>,
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
    base_theme: ThemeConfig,
    theme: ThemeConfig,
    ui_config: UiConfig,
    ai_config: AiConfig,
    runtime_scale: f32,
    layout_engine: WorkbenchLayoutEngine,
    panel_state: WorkbenchPanelState,
    focus_manager: FocusManager,
    pub overlay_manager: OverlayManager,
    clipboard: SystemClipboard,
    window: Option<Arc<Window>>,
    renderer: Option<Renderer>,
    window_size: PhysicalSize<u32>,
    editor_needs_layout: bool,
    editor_caret_needs_layout: bool,
    sidebar_needs_layout: bool,
    terminal_needs_layout: bool,
    buffer_terminal_needs_layout: bool,
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
    references_request_revision: u64,
    document_symbols_request_revision: u64,
    /// In-flight `completionItem/resolve` request id; used to correlate failure events
    /// back to the pending docs panel so we can flip "Loading…" to "No docs" when the
    /// server rejects or times out.
    completion_resolve_request_id: Option<u64>,
    /// Request id of the last in-flight hover — used to clear the loading overlay
    /// when the request fails or returns empty (so the overlay doesn't get stuck).
    hover_loading_request_id: Option<u64>,
    /// Request id of the last in-flight `gd`/`gD` definition request. When a
    /// new request is dispatched, the previous one's result must be dropped on
    /// arrival to avoid a flicker (or a wrong jump) if the server replies out
    /// of order.
    latest_definition_request_id: Option<u64>,
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
    ai_inline_revision: u64,
    pending_ai_inline_request: Option<PendingAiInlineRequest>,
    pending_lsp_document_sync: Option<PendingLspDocumentSync>,
    last_editor_bounds: Option<[f32; 4]>,
    last_show_welcome: Option<bool>,
    last_sidebar_bounds: Option<[f32; 4]>,
    last_sidebar_focused: Option<bool>,
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
    last_git_branch_refresh_at: Instant,
    last_thinking_animation_tick: Instant,
    last_lsp_loading_animation_tick: Instant,
    lsp_loading_frame: u8,
    caret_blink_visible: bool,
    caret_blink_dirty: bool,
    pre_markdown_preview_right_width: Option<f32>,
    /// Code actions từ lần request gần nhất, dùng để apply khi user chọn trong picker.
    pending_code_actions: Vec<crate::async_runtime::message::LspCodeAction>,
    /// Python interpreter path selected by the user from the command palette.
    selected_python_env: Option<std::path::PathBuf>,
    /// Cached runtime version strings for the statusbar right zone.
    runtime_versions: RuntimeVersionInfo,
    /// Scheduled instant to auto-retry LSP server start after user accepted an install guide.
    lsp_retry_at: Option<Instant>,
}

const DEBUG_UI_ENABLED: bool = false;
const PARSE_DEBOUNCE_INTERVAL: Duration = Duration::from_millis(20);
const GIT_DIFF_DEBOUNCE_INTERVAL: Duration = Duration::from_millis(80);
/// User must dwell on a completion item for at least this long before we fire
/// `completionItem/resolve`. Prevents spamming the LSP server while the user
/// scrolls quickly through items with arrow keys.
const COMPLETION_RESOLVE_DEBOUNCE_INTERVAL: Duration = Duration::from_millis(100);
const LSP_DIAGNOSTIC_DEBOUNCE_INTERVAL: Duration = Duration::from_millis(500);
const FPS_METRICS_UPDATE_INTERVAL: Duration = Duration::from_millis(500);
const GIT_BRANCH_REFRESH_INTERVAL: Duration = Duration::from_millis(750);
const THINKING_ANIMATION_INTERVAL: Duration = Duration::from_millis(400);
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
    /// User confirmed or cancelled the opencode auto-install prompt.
    AiChatInstall,
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

#[derive(Debug, Clone)]
struct TransientToast {
    message: String,
    expires_at: Instant,
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
}

pub fn run() -> Result<(), winit::error::EventLoopError> {
    let event_loop = EventLoop::<AppEvent>::with_user_event().build()?;
    event_loop.set_control_flow(ControlFlow::Wait);
    let event_proxy = event_loop.create_proxy();

    let mut app = match AppShell::new(event_proxy) {
        Ok(app) => app,
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
            return Some(&mut self.terminal_grid);
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
            return self.pty_session_id;
        }
        if self.focus_manager.current() == FocusTarget::RightSidebar
            && self.panel_state.right.active_tab_id() == Some(PanelTabId::Terminal)
        {
            return self.right_pty_session_id;
        }
        None
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
        } else if self.app_state.active_buffer_is_settings() {
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
