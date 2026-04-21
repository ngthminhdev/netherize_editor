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
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop},
    window::{Fullscreen, Window, WindowId},
};

use crate::{
    app::{
        app_state::AppState,
        async_bridge::{AppAsyncBridge, AsyncResultRouter},
        input::{InputHandler, InputRouteOutcome},
        input_map::{InputFocusContext, InputMap, KeybindingContext},
    },
    async_runtime::{
        message::{
            RequestSpec, RequestTopic, WorkerEvent, WorkerRequestPayload, WorkerResult,
            WorkerResultPayload,
        },
        scheduler::AsyncScheduler,
    },
    config::{
        keymap_loader::KeymapLoader,
        theme_config::ThemeConfig,
        ui_config::{UiConfig, WindowStartupMode},
    },
    core::{
        command_dispatch::dispatch_command,
        commands::Command,
        mode::{EditorMode, ModeEvent},
    },
    lsp::client::path_to_lsp_uri,
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
    input_handler: InputHandler,
    input_map: InputMap,
    scheduler: AsyncScheduler,
    bridge: Option<AppAsyncBridge>,
    pty_session_id: Option<u64>,
    highlight_spans: Vec<HighlightSpan>,
    terminal_grid: TerminalGrid,
    explorer_cursor: usize,
    explorer_expanded: HashSet<PathBuf>,
    explorer_snapshot: ExplorerSnapshot,
    explorer_snapshot_dirty: bool,
    workspace_git_branch: Option<String>,
    base_theme: ThemeConfig,
    theme: ThemeConfig,
    ui_config: UiConfig,
    runtime_scale: f32,
    layout_engine: WorkbenchLayoutEngine,
    panel_state: WorkbenchPanelState,
    focus_manager: FocusManager,
    pub overlay_manager: OverlayManager,
    window: Option<Arc<Window>>,
    renderer: Option<Renderer>,
    window_size: PhysicalSize<u32>,
    editor_needs_layout: bool,
    editor_caret_needs_layout: bool,
    sidebar_needs_layout: bool,
    terminal_needs_layout: bool,
    last_parse_submit_at: Option<Instant>,
    active_highlight_request_revision: u64,
    pending_parse_after_debounce: bool,
    last_editor_bounds: Option<[f32; 4]>,
    last_sidebar_bounds: Option<[f32; 4]>,
    last_sidebar_focused: Option<bool>,
    last_terminal_bounds: Option<[f32; 4]>,
    sidebar_selection_quads: Vec<RegionDrawInstance>,
}

const DEBUG_UI_ENABLED: bool = false;
const PARSE_DEBOUNCE_INTERVAL: Duration = Duration::from_millis(80);

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
