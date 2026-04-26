use std::sync::Arc;
use std::{
    collections::HashMap,
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
        command_palette::CommandPaletteMode,
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
    terminal::grid::TerminalGrid,
    text::text_system::StyledTextSpan,
    workbench::{
        focus_manager::{FocusManager, FocusTarget},
        layout_engine::WorkbenchLayoutEngine,
        overlay_manager::OverlayManager,
        panel_state::WorkbenchPanelState,
        region_model::RegionId,
    },
    workspace::model::WorkspaceNodeType,
};

mod application;
mod async_results;
mod commands;
mod helpers;
mod setup;
mod welcome;

use helpers::{
    build_preview_render_data, build_sidebar_rows, collect_explorer_entries, detect_git_branch,
    diagnostic_spans_to_styled, language_id_for_path, parse_hover_markdown_blocks, region_color,
    scale_theme, scale_ui_config, syntax_spans_to_styled,
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
    terminal_buffer_grids: HashMap<u64, TerminalGrid>,
    pending_lazygit_buffer_index: Option<usize>,
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
    /// Toast window-relative ngắn hạn cho các action nền.
    transient_toast: Option<TransientToast>,
    base_theme: ThemeConfig,
    theme: ThemeConfig,
    ui_config: UiConfig,
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
    last_frame_time: Instant,
    last_fps_metrics_update_at: Instant,
    accumulated_frame_time: Duration,
    accumulated_frame_count: u32,
    current_fps_metrics: String,
    last_parse_submit_at: Option<Instant>,
    /// Edit hint for the next incremental tree-sitter parse.
    /// Set to `Some` when exactly one edit occurred since the last reconcile.
    /// Set to `None` when multiple edits accumulated (debounced typing, undo/redo,
    /// paste) — the worker falls back to a full reparse in that case.
    last_syntax_edit_hint: Option<SyntaxEditHint>,
    active_highlight_request_revision: u64,
    fzf_search_revision: u64,
    pending_parse_after_debounce: bool,
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
    last_scroll_animation_tick: Instant,
}

const DEBUG_UI_ENABLED: bool = false;
const PARSE_DEBOUNCE_INTERVAL: Duration = Duration::from_millis(80);
const LSP_DIAGNOSTIC_DEBOUNCE_INTERVAL: Duration = Duration::from_millis(500);
const FPS_METRICS_UPDATE_INTERVAL: Duration = Duration::from_millis(500);

#[derive(Debug, Clone)]
struct ExplorerEntry {
    path: PathBuf,
    parent_path: Option<PathBuf>,
    file_type: WorkspaceNodeType,
    depth: usize,
    is_expanded: bool,
    name: String,
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

/// State cho popup hướng dẫn cài đặt Language Server.
#[derive(Debug, Clone, PartialEq, Eq)]
struct LspInstallGuide {
    /// Binary name (ví dụ "rust-analyzer").
    binary: String,
    /// Lệnh cài đặt sẽ được bơm thẳng vào terminal khi user bấm Enter.
    install_cmd: String,
}

#[derive(Debug, Clone)]
struct TransientToast {
    message: String,
    expires_at: Instant,
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
