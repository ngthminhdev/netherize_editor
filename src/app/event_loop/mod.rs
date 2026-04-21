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
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop},
    window::{Fullscreen, Window, WindowId},
};

use crate::{
    app::{
        app_state::AppState,
        async_bridge::{AppAsyncBridge, AsyncResultRouter},
        clipboard::SystemClipboard,
        command_palette::CommandPaletteMode,
        input::{InputHandler, InputRouteOutcome},
        input_map::{InputFocusContext, InputMap, KeybindingContext},
        persistence::AppPersistentState,
    },
    async_runtime::{
        message::{
            FzfSearchMode, RequestSpec, RequestTopic, WorkerEvent, WorkerRequestPayload,
            WorkerResult, WorkerResultPayload,
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
    lsp::client::{detect_lsp_server_for_path, detect_lsp_server_for_workspace, path_to_lsp_uri},
    render::{
        region_pipeline::RegionDrawInstance,
        renderer::{RenderError, Renderer, SidebarRow},
    },
    syntax::{highlight::HighlightSpan, syntax_engine::LanguageId},
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
    build_sidebar_rows, collect_explorer_entries, detect_git_branch, language_id_for_path,
    region_color, scale_theme, scale_ui_config, syntax_spans_to_styled,
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
    highlight_spans: Vec<HighlightSpan>,
    terminal_grid: TerminalGrid,
    explorer_cursor: usize,
    explorer_snapshot: ExplorerSnapshot,
    explorer_snapshot_dirty: bool,
    pending_confirmation: Option<PendingConfirmation>,
    workspace_git_branch: Option<String>,
    active_lsp_server: Option<ActiveLspServer>,
    pending_lsp_server: Option<ActiveLspServer>,
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
    last_frame_time: Instant,
    last_fps_metrics_update_at: Instant,
    accumulated_frame_time: Duration,
    accumulated_frame_count: u32,
    current_fps_metrics: String,
    last_parse_submit_at: Option<Instant>,
    active_highlight_request_revision: u64,
    fzf_search_revision: u64,
    pending_parse_after_debounce: bool,
    last_editor_bounds: Option<[f32; 4]>,
    last_show_welcome: Option<bool>,
    last_sidebar_bounds: Option<[f32; 4]>,
    last_sidebar_focused: Option<bool>,
    last_terminal_bounds: Option<[f32; 4]>,
    sidebar_selection_quads: Vec<RegionDrawInstance>,
    suppress_next_palette_ime_commit: bool,
    /// Leap/EasyMotion labels hiện tại: (label_char, char_idx_in_text).
    /// `Some(labels)` khi đang ở PendingLeapLabel state, `None` khi không active.
    leap_labels: Option<Vec<(char, usize)>>,
}

const DEBUG_UI_ENABLED: bool = false;
const PARSE_DEBOUNCE_INTERVAL: Duration = Duration::from_millis(80);
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
}

#[derive(Debug, Clone)]
struct PendingConfirmation {
    action: PendingConfirmationAction,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ActiveLspServer {
    server_name: String,
    root_path: PathBuf,
}

pub fn run() -> Result<(), winit::error::EventLoopError> {
    let event_loop = EventLoop::new()?;
    event_loop.set_control_flow(ControlFlow::Wait);

    let mut app = match AppShell::new() {
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
        self.app_state.open_buffers().is_empty() && !self.app_state.is_command_palette_visible()
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
}
