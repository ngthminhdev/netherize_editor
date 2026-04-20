use std::sync::Arc;
use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
};

use winit::{
    application::ApplicationHandler,
    dpi::PhysicalSize,
    event::{Ime, WindowEvent},
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop},
    window::{Window, WindowId},
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
    config::{keymap_loader::KeymapLoader, theme_config::ThemeConfig},
    core::{
        command_dispatch::dispatch_command,
        commands::Command,
        mode::{EditorMode, ModeEvent},
    },
    lsp::client::path_to_lsp_uri,
    render::{
        region_pipeline::RegionDrawInstance,
        renderer::{RenderError, Renderer},
    },
    syntax::{highlight::HighlightSpan, syntax_engine::LanguageId},
    terminal::grid::TerminalGrid,
    text::text_system::StyledTextSpan,
    workbench::{
        focus_manager::{FocusManager, FocusTarget},
        layout_engine::{WorkbenchLayoutConfig, WorkbenchLayoutEngine},
        overlay_manager::OverlayManager,
        panel_state::WorkbenchPanelState,
        region_model::RegionId,
    },
    workspace::model::WorkspaceNodeType,
};

const WINDOW_TITLE: &str = "Netherize Editor";
const WINDOW_WIDTH: u32 = 1280;
const WINDOW_HEIGHT: u32 = 800;
const EXPLORER_PADDING: f32 = 8.0;

// ── AppShell ──────────────────────────────────────────────────────────────────

/// Unified application shell: the single `ApplicationHandler` for the main window.
///
/// Owns and wires together every subsystem:
///
/// ```text
///  Keyboard ──► InputHandler ──► InputMap ──► dispatch_command ──► AppState
///  Async results ◄── AppAsyncBridge ◄── AsyncScheduler ◄── Worker threads
///  Every frame: LayoutEngine → Renderer (GPU)
/// ```
pub struct AppShell {
    // ── Core editor state ─────────────────────────────────────────────────────
    app_state: AppState,

    // ── Input pipeline ────────────────────────────────────────────────────────
    input_handler: InputHandler,
    input_map: InputMap,

    // ── Async subsystems ──────────────────────────────────────────────────────
    scheduler: AsyncScheduler,
    /// Stored as `Option` so we can `take()` it during `pump_bridge()` without
    /// borrow-checker conflicts (we implement `AsyncResultRouter` on `Self`).
    bridge: Option<AppAsyncBridge>,

    // ── Session state owned by the shell ──────────────────────────────────────
    pty_session_id: Option<u64>,
    highlight_spans: Vec<HighlightSpan>,
    terminal_grid: TerminalGrid,
    explorer_cursor: usize,
    explorer_expanded: HashSet<PathBuf>,
    explorer_snapshot: ExplorerSnapshot,
    explorer_snapshot_dirty: bool,
    theme: ThemeConfig,

    // ── Workbench layout & focus ──────────────────────────────────────────────
    layout_engine: WorkbenchLayoutEngine,
    panel_state: WorkbenchPanelState,
    focus_manager: FocusManager,
    pub overlay_manager: OverlayManager,

    // ── GPU rendering ─────────────────────────────────────────────────────────
    window: Option<Arc<Window>>,
    renderer: Option<Renderer>,
    window_size: PhysicalSize<u32>,
    editor_needs_layout: bool,
    editor_caret_needs_layout: bool,
    sidebar_needs_layout: bool,
    terminal_needs_layout: bool,
    last_editor_bounds: Option<[f32; 4]>,
    last_sidebar_bounds: Option<[f32; 4]>,
    last_terminal_bounds: Option<[f32; 4]>,
}

impl AppShell {
    pub fn new() -> Result<Self, String> {
        let cwd = std::env::current_dir().unwrap_or_default();
        let save_path = cwd.join("untitled.rs");

        let (scheduler, rx) = AsyncScheduler::new()?;
        let bridge = AppAsyncBridge::new(rx);

        let starter = "// Netherize Editor — Grand Integration\n\
                        // Normal mode: hjkl / arrows to move, i = Insert, Esc = Normal\n\
                        // Mod+S = save  Mod+P = file picker  Ctrl+` = terminal\n\n\
                        fn main() {\n\
                        \tprintln!(\"Hello from Netherize!\");\n\
                        }\n";

        let mut app_state = AppState::from_text(save_path.clone(), starter);
        if let Err(e) = app_state.attach_workspace(cwd.clone()) {
            eprintln!("[AppShell] workspace attach skipped: {e}");
        }
        let mut explorer_expanded = HashSet::new();
        if let Some(root) = app_state.workspace_root_path() {
            explorer_expanded.insert(root.to_path_buf());
        }
        let theme = ThemeConfig::load_active();

        Ok(Self {
            app_state,
            input_handler: InputHandler::new(),
            input_map: InputMap::new(save_path),
            scheduler,
            bridge: Some(bridge),
            pty_session_id: None,
            highlight_spans: Vec::new(),
            terminal_grid: TerminalGrid::new(120, 40),
            explorer_cursor: 0,
            explorer_expanded,
            explorer_snapshot: ExplorerSnapshot::default(),
            explorer_snapshot_dirty: true,
            theme,
            layout_engine: WorkbenchLayoutEngine::new(WorkbenchLayoutConfig::default()),
            panel_state: WorkbenchPanelState::default(),
            focus_manager: FocusManager::default(),
            overlay_manager: OverlayManager::default(),
            window: None,
            renderer: None,
            window_size: PhysicalSize::new(WINDOW_WIDTH, WINDOW_HEIGHT),
            editor_needs_layout: true,
            editor_caret_needs_layout: false,
            sidebar_needs_layout: true,
            terminal_needs_layout: true,
            last_editor_bounds: None,
            last_sidebar_bounds: None,
            last_terminal_bounds: None,
        })
    }

    // ── Startup ───────────────────────────────────────────────────────────────

    /// Kick off background subsystems after the window is ready.
    fn startup_subsystems(&mut self) {
        let cwd = std::env::current_dir().unwrap_or_default();

        // 1. File-system watcher — keeps workspace tree fresh
        self.submit(RequestSpec {
            revision_id: 0,
            topic: RequestTopic::WorkspaceWatch,
            payload: WorkerRequestPayload::StartFileWatch {
                root_path: cwd.clone(),
            },
        });

        // 2. PTY shell — pre-warm so first ToggleTerminal is instant
        self.submit(RequestSpec {
            revision_id: 0,
            topic: RequestTopic::TerminalPty,
            payload: WorkerRequestPayload::SpawnPtyShell {
                shell: None,
                working_dir: Some(cwd.clone()),
            },
        });

        // 3. LSP server — auto-detect language server in PATH
        self.submit(RequestSpec {
            revision_id: 0,
            topic: RequestTopic::LspClient,
            payload: WorkerRequestPayload::StartLspServer {
                root_path: cwd,
                server_command: None,
            },
        });

        eprintln!(
            "[AppShell] subsystems started — profile={}",
            KeymapLoader::active_profile()
        );
    }

    // ── Helpers ───────────────────────────────────────────────────────────────

    fn submit(&self, spec: RequestSpec) {
        if let Err(e) = self.scheduler.submit(spec) {
            eprintln!("[AppShell] scheduler submit failed: {e}");
        }
    }

    /// Non-blocking bridge pump. Uses `take()` to satisfy the borrow checker
    /// while `self` also implements `AsyncResultRouter`.
    fn pump_bridge(&mut self) -> bool {
        let Some(mut bridge) = self.bridge.take() else {
            return false;
        };
        let stats = bridge.pump(self);
        self.bridge = Some(bridge);
        stats.accepted > 0 || stats.completed > 0
    }

    fn request_redraw(&self) {
        if let Some(w) = &self.window {
            w.request_redraw();
        }
    }

    fn build_context(&self) -> KeybindingContext {
        let mode = self.app_state.current_mode();
        let focus = match self.focus_manager.current() {
            FocusTarget::LeftSidebar => InputFocusContext::Explorer,
            FocusTarget::RightSidebar => InputFocusContext::Inspector,
            FocusTarget::BottomPanel => {
                if mode == EditorMode::TerminalFocus {
                    InputFocusContext::Terminal
                } else {
                    InputFocusContext::BottomPanel
                }
            }
            _ => InputFocusContext::Editor,
        };
        KeybindingContext {
            mode,
            focus,
            file_picker_open: self.app_state.is_file_picker_open(),
        }
    }

    fn release_focus_mode_to_editor(&mut self) -> bool {
        let mut changed = false;

        if self.app_state.current_mode() == EditorMode::PaletteFocus {
            changed |= self.app_state.close_file_picker();
        }

        if matches!(
            self.app_state.current_mode(),
            EditorMode::PaletteFocus | EditorMode::TerminalFocus
        ) && let Ok(result) = self.app_state.apply_mode_event(ModeEvent::ExitFocus)
        {
            changed |= result.changed;
        }

        changed
    }

    fn submit_parse_for_active_buffer(&self) {
        if let Some(file_path) = self.app_state.active_file().map(PathBuf::from) {
            self.submit(RequestSpec {
                revision_id: self.app_state.revision(),
                topic: RequestTopic::ActiveBufferLayout,
                payload: WorkerRequestPayload::ParseAndHighlight {
                    file_path: Some(file_path),
                    text_snapshot: self.app_state.text_string(),
                    language_id: LanguageId::Rust,
                },
            });
        }
    }

    fn submit_lsp_did_open_for_active_file(&self) {
        let Some(path) = self.app_state.active_file() else {
            return;
        };
        let version = self.app_state.revision().min(i32::MAX as u64) as i32;
        self.submit(RequestSpec {
            revision_id: self.app_state.revision(),
            topic: RequestTopic::LspClient,
            payload: WorkerRequestPayload::LspDidOpen {
                uri: path_to_lsp_uri(path),
                language_id: language_id_for_path(path),
                version,
                text: self.app_state.text_string(),
            },
        });
    }

    fn mark_explorer_dirty(&mut self) {
        self.explorer_snapshot_dirty = true;
        self.sidebar_needs_layout = true;
    }

    fn ensure_explorer_snapshot(&mut self) {
        if !self.explorer_snapshot_dirty {
            return;
        }

        let entries = collect_explorer_entries(&self.app_state, &self.explorer_expanded);
        self.explorer_cursor = if entries.is_empty() {
            0
        } else {
            self.explorer_cursor.min(entries.len() - 1)
        };
        self.explorer_snapshot = ExplorerSnapshot { entries };
        self.explorer_snapshot_dirty = false;
    }

    fn sync_explorer_expanded_with_workspace(&mut self) {
        let Some(root) = self.app_state.workspace_root_path().map(PathBuf::from) else {
            self.explorer_expanded.clear();
            self.explorer_cursor = 0;
            self.mark_explorer_dirty();
            return;
        };

        let mut valid_folders: HashSet<PathBuf> = HashSet::new();
        valid_folders.insert(root.clone());
        if let Some(nodes) = self.app_state.workspace_nodes() {
            for node in nodes {
                if node.file_type == WorkspaceNodeType::Folder {
                    valid_folders.insert(node.path.clone());
                }
            }
        }

        self.explorer_expanded
            .retain(|path| valid_folders.contains(path));
        self.explorer_expanded.insert(root);
        self.mark_explorer_dirty();
    }

    /// Apply a command. Returns whether a redraw is needed.
    fn handle_command(&mut self, command: Command) -> bool {
        match &command {
            // ── ToggleTerminal: extend dispatch_command with PTY + panel sync ──
            Command::ToggleTerminal => {
                let report = dispatch_command(&mut self.app_state, command);

                // Sync WorkbenchPanelState visibility with AppState
                let is_open = self.app_state.is_terminal_panel_open();
                if is_open != self.panel_state.bottom.visible {
                    self.panel_state.toggle_bottom();
                    self.terminal_needs_layout = true;
                }

                // Update focus target
                let focus_changed = if is_open {
                    let changed = self.focus_manager.set(FocusTarget::BottomPanel);
                    // Spawn PTY if not already running
                    if self.pty_session_id.is_none() {
                        let working_dir = self
                            .app_state
                            .active_file()
                            .and_then(|p| p.parent())
                            .map(PathBuf::from)
                            .or_else(|| std::env::current_dir().ok());
                        self.submit(RequestSpec {
                            revision_id: 0,
                            topic: RequestTopic::TerminalPty,
                            payload: WorkerRequestPayload::SpawnPtyShell {
                                shell: None,
                                working_dir,
                            },
                        });
                    }
                    changed
                } else {
                    self.focus_manager.set(FocusTarget::CenterEditor)
                };

                if focus_changed {
                    self.input_handler.clear_pending_prefix();
                }
                report.request_redraw
            }

            // ── OpenFileFinder: dispatch handles mode→PaletteFocus transition ──
            Command::OpenFileFinder => {
                let report = dispatch_command(&mut self.app_state, command);
                if report.success {
                    // Sync workbench focus so keyboard context sees OverlayLayer
                    if self.focus_manager.set(FocusTarget::OverlayLayer) {
                        self.input_handler.clear_pending_prefix();
                    }
                }
                report.request_redraw
            }

            // ── CloseFilePicker: dispatch handles mode←PaletteFocus transition ─
            Command::CloseFilePicker => {
                let report = dispatch_command(&mut self.app_state, command);
                if self.focus_manager.set(FocusTarget::CenterEditor) {
                    self.input_handler.clear_pending_prefix();
                }
                report.request_redraw
            }

            // ── ToggleExplorer ────────────────────────────────────────────────
            Command::ToggleExplorer => {
                let mut changed = self.panel_state.toggle_left();
                if changed {
                    self.sidebar_needs_layout = true;
                }
                let mut focus_changed = false;
                if self.panel_state.left.visible {
                    changed |= self.release_focus_mode_to_editor();
                    focus_changed = self.focus_manager.set(FocusTarget::LeftSidebar);
                    changed |= focus_changed;
                } else {
                    if self.focus_manager.current() == FocusTarget::LeftSidebar {
                        focus_changed = self.focus_manager.set(FocusTarget::CenterEditor);
                        changed |= focus_changed;
                    }
                }
                if focus_changed {
                    self.input_handler.clear_pending_prefix();
                }
                changed
            }

            // ── Workbench focus navigation ────────────────────────────────────
            Command::FocusEditor | Command::FocusBack => {
                let mut changed = self.release_focus_mode_to_editor();
                let focus_changed = self.focus_manager.set(FocusTarget::CenterEditor);
                changed |= focus_changed;
                if focus_changed {
                    self.input_handler.clear_pending_prefix();
                }
                changed
            }
            Command::FocusExplorer => {
                let mut changed = self.release_focus_mode_to_editor();
                if !self.panel_state.left.visible {
                    self.panel_state.left.visible = true;
                    changed = true;
                    self.sidebar_needs_layout = true;
                }
                let focus_changed = self.focus_manager.set(FocusTarget::LeftSidebar);
                changed |= focus_changed;
                if focus_changed {
                    self.input_handler.clear_pending_prefix();
                }
                changed
            }
            Command::FocusInspector => {
                let mut changed = self.release_focus_mode_to_editor();
                if !self.panel_state.right.visible {
                    self.panel_state.right.visible = true;
                    changed = true;
                }
                let focus_changed = self.focus_manager.set(FocusTarget::RightSidebar);
                changed |= focus_changed;
                if focus_changed {
                    self.input_handler.clear_pending_prefix();
                }
                changed
            }
            Command::FocusTerminal => {
                let mut changed = false;

                if self.app_state.current_mode() == EditorMode::PaletteFocus {
                    changed |= self.app_state.close_file_picker();
                    if let Ok(result) = self.app_state.apply_mode_event(ModeEvent::ExitFocus) {
                        changed |= result.changed;
                    }
                }

                if self.app_state.set_terminal_panel_open(true) {
                    changed = true;
                }
                if !self.panel_state.bottom.visible {
                    self.panel_state.bottom.visible = true;
                    changed = true;
                }
                self.terminal_needs_layout = true;

                if self.app_state.current_mode() != EditorMode::TerminalFocus
                    && let Ok(result) = self.app_state.apply_mode_event(ModeEvent::FocusTerminal)
                {
                    changed |= result.changed;
                }

                let focus_changed = self.focus_manager.set(FocusTarget::BottomPanel);
                changed |= focus_changed;
                if focus_changed {
                    self.input_handler.clear_pending_prefix();
                }

                if self.pty_session_id.is_none() {
                    let working_dir = self
                        .app_state
                        .active_file()
                        .and_then(|p| p.parent())
                        .map(PathBuf::from)
                        .or_else(|| std::env::current_dir().ok());
                    self.submit(RequestSpec {
                        revision_id: 0,
                        topic: RequestTopic::TerminalPty,
                        payload: WorkerRequestPayload::SpawnPtyShell {
                            shell: None,
                            working_dir,
                        },
                    });
                }

                changed
            }
            Command::TerminalWriteInput(input) => {
                self.forward_to_pty(input);
                false
            }
            Command::MoveFocusCycle => {
                let changed = self.focus_manager.cycle_next(&self.panel_state);
                if changed {
                    self.input_handler.clear_pending_prefix();
                }
                changed
            }

            // ── Explorer / panel navigation ────────────────────────────────────
            Command::ExplorerMoveUp => {
                self.ensure_explorer_snapshot();
                let entries_len = self.explorer_snapshot.entries.len();
                if entries_len == 0 {
                    self.explorer_cursor = 0;
                    return false;
                }
                self.explorer_cursor = self.explorer_cursor.min(entries_len.saturating_sub(1));
                if self.explorer_cursor == 0 {
                    false
                } else {
                    self.explorer_cursor -= 1;
                    self.sidebar_needs_layout = true;
                    true
                }
            }
            Command::ExplorerMoveDown => {
                self.ensure_explorer_snapshot();
                let entries_len = self.explorer_snapshot.entries.len();
                if entries_len == 0 {
                    self.explorer_cursor = 0;
                    return false;
                }
                self.explorer_cursor = self.explorer_cursor.min(entries_len.saturating_sub(1));
                if self.explorer_cursor + 1 >= entries_len {
                    false
                } else {
                    self.explorer_cursor += 1;
                    self.sidebar_needs_layout = true;
                    true
                }
            }
            Command::ExplorerCollapseOrParent => {
                self.ensure_explorer_snapshot();
                if self.explorer_snapshot.entries.is_empty() {
                    self.explorer_cursor = 0;
                    return false;
                }
                self.explorer_cursor = self
                    .explorer_cursor
                    .min(self.explorer_snapshot.entries.len().saturating_sub(1));
                let selected = self.explorer_snapshot.entries[self.explorer_cursor].clone();

                if selected.file_type == WorkspaceNodeType::Folder && selected.is_expanded {
                    let changed = self.explorer_expanded.remove(&selected.path);
                    if changed {
                        self.mark_explorer_dirty();
                    }
                    return changed;
                }

                let Some(parent_path) = selected.parent_path.as_ref() else {
                    return false;
                };
                let Some(parent_idx) = self
                    .explorer_snapshot
                    .entries
                    .iter()
                    .position(|entry| &entry.path == parent_path)
                else {
                    return false;
                };
                if parent_idx == self.explorer_cursor {
                    return false;
                }
                self.explorer_cursor = parent_idx;
                self.sidebar_needs_layout = true;
                true
            }
            Command::ExplorerExpandOrChild => {
                self.ensure_explorer_snapshot();
                if self.explorer_snapshot.entries.is_empty() {
                    self.explorer_cursor = 0;
                    return false;
                }
                self.explorer_cursor = self
                    .explorer_cursor
                    .min(self.explorer_snapshot.entries.len().saturating_sub(1));
                let selected = self.explorer_snapshot.entries[self.explorer_cursor].clone();
                if selected.file_type != WorkspaceNodeType::Folder {
                    return false;
                }

                if !selected.is_expanded {
                    let changed = self.explorer_expanded.insert(selected.path.clone());
                    if changed {
                        self.mark_explorer_dirty();
                    }
                    return changed;
                }

                let Some(first_child_idx) = self
                    .explorer_snapshot
                    .entries
                    .iter()
                    .position(|entry| entry.parent_path.as_ref() == Some(&selected.path))
                else {
                    return false;
                };

                if first_child_idx == self.explorer_cursor {
                    return false;
                }
                self.explorer_cursor = first_child_idx;
                self.sidebar_needs_layout = true;
                true
            }
            Command::ExplorerToggleOrOpen
            | Command::ExplorerExpandCollapse
            | Command::ExplorerOpenFile => {
                self.ensure_explorer_snapshot();
                if self.explorer_snapshot.entries.is_empty() {
                    self.explorer_cursor = 0;
                    return false;
                }
                self.explorer_cursor = self
                    .explorer_cursor
                    .min(self.explorer_snapshot.entries.len().saturating_sub(1));
                let selected = self.explorer_snapshot.entries[self.explorer_cursor].clone();
                if selected.file_type == WorkspaceNodeType::Folder {
                    if selected.is_expanded {
                        let changed = self.explorer_expanded.remove(&selected.path);
                        if changed {
                            self.mark_explorer_dirty();
                        }
                        return changed;
                    }
                    let changed = self.explorer_expanded.insert(selected.path.clone());
                    if changed {
                        self.mark_explorer_dirty();
                    }
                    return changed;
                }

                let report = dispatch_command(
                    &mut self.app_state,
                    Command::OpenFile(selected.path.clone()),
                );
                if !report.success {
                    return false;
                }
                self.submit_parse_for_active_buffer();
                self.submit_lsp_did_open_for_active_file();
                let mut changed = report.request_redraw || report.state_changed;
                let focus_changed = self.focus_manager.set(FocusTarget::CenterEditor);
                changed |= focus_changed;
                changed |= self.release_focus_mode_to_editor();
                self.editor_needs_layout = true;
                self.editor_caret_needs_layout = false;
                if focus_changed {
                    self.input_handler.clear_pending_prefix();
                }
                changed
            }
            Command::NextPanelTab => match self.focus_manager.current() {
                FocusTarget::BottomPanel => self.panel_state.switch_bottom_next_tab(),
                FocusTarget::LeftSidebar => self.panel_state.switch_left_next_tab(),
                FocusTarget::RightSidebar => self.panel_state.switch_right_next_tab(),
                _ => false,
            },
            Command::PrevPanelTab => match self.focus_manager.current() {
                FocusTarget::BottomPanel => self.panel_state.switch_bottom_prev_tab(),
                FocusTarget::LeftSidebar => self.panel_state.switch_left_prev_tab(),
                FocusTarget::RightSidebar => self.panel_state.switch_right_prev_tab(),
                _ => false,
            },

            // ── All other commands: plain dispatch ────────────────────────────
            _ => {
                let should_notify_did_open = matches!(
                    &command,
                    Command::OpenFile(_) | Command::FilePickerConfirmSelection
                );
                let should_refresh_caret_only = matches!(
                    &command,
                    Command::MoveLeft | Command::MoveRight | Command::MoveUp | Command::MoveDown
                );
                let should_reparse = matches!(
                    &command,
                    Command::InsertChar(_)
                        | Command::InsertText(_)
                        | Command::Newline
                        | Command::Backspace
                        | Command::OpenFile(_)
                        | Command::FilePickerConfirmSelection
                );
                let report = dispatch_command(&mut self.app_state, command);

                // When the buffer changes, kick off async syntax highlighting
                if report.state_changed {
                    if should_refresh_caret_only {
                        self.editor_caret_needs_layout = true;
                    } else {
                        self.editor_needs_layout = true;
                        self.editor_caret_needs_layout = false;
                    }
                }
                if report.state_changed && should_reparse {
                    self.submit_parse_for_active_buffer();
                }
                if report.success && should_notify_did_open {
                    self.submit_lsp_did_open_for_active_file();
                }

                report.request_redraw
            }
        }
    }

    /// Forward raw text to the active PTY session (terminal input).
    fn forward_to_pty(&self, text: &str) {
        if let Some(session_id) = self.pty_session_id {
            self.submit(RequestSpec {
                revision_id: 0,
                topic: RequestTopic::TerminalPty,
                payload: WorkerRequestPayload::WritePtyInput {
                    session_id,
                    input: text.to_string(),
                },
            });
        }
    }
}

// ── AsyncResultRouter ─────────────────────────────────────────────────────────

impl AsyncResultRouter for AppShell {
    fn current_revision_for(&self, topic: RequestTopic) -> u64 {
        match topic {
            RequestTopic::ActiveBufferLayout => self.app_state.revision(),
            _ => 0,
        }
    }

    fn on_worker_event(&mut self, _event: WorkerEvent) {
        // Bridge already logs lifecycle events; no additional action needed here.
    }

    fn on_worker_result(&mut self, result: WorkerResult) {
        match result.payload {
            // ── Syntax highlighting result ────────────────────────────────────
            WorkerResultPayload::ParseAndHighlight { spans, .. } => {
                self.highlight_spans = spans;
                self.editor_needs_layout = true;
                self.editor_caret_needs_layout = false;
            }

            // ── File-system changes ───────────────────────────────────────────
            WorkerResultPayload::FileSystemEvents { events, .. } => {
                if let Err(e) = self.app_state.apply_external_file_events(&events) {
                    eprintln!("[AppShell] fs-event apply failed: {e}");
                }
                self.sync_explorer_expanded_with_workspace();
                self.editor_needs_layout = true;
                self.editor_caret_needs_layout = false;
            }

            // ── PTY session spawned ───────────────────────────────────────────
            WorkerResultPayload::PtySpawned {
                session_id,
                shell,
                working_dir,
            } => {
                eprintln!(
                    "[AppShell] PTY ready: session={session_id} shell={shell} dir={}",
                    working_dir.display()
                );
                self.pty_session_id = Some(session_id);
                self.terminal_needs_layout = true;
            }

            // ── PTY output ────────────────────────────────────────────────────
            WorkerResultPayload::PtyOutput { session_id, chunk } => {
                if self.pty_session_id == Some(session_id) {
                    self.terminal_grid.feed_chunk(&chunk);
                    self.terminal_needs_layout = true;
                    self.request_redraw();
                }
            }

            // ── PTY session closed ────────────────────────────────────────────
            WorkerResultPayload::PtySessionClosed {
                session_id, reason, ..
            } => {
                if self.pty_session_id == Some(session_id) {
                    eprintln!("[AppShell] PTY {session_id} closed: {reason}");
                    self.pty_session_id = None;
                }
            }

            // ── LSP server ready ──────────────────────────────────────────────
            WorkerResultPayload::LspServerStarted {
                server_name,
                root_path,
                ..
            } => {
                eprintln!(
                    "[AppShell] LSP '{}' ready for {}",
                    server_name,
                    root_path.display()
                );
                self.submit_lsp_did_open_for_active_file();
            }

            // ── LSP diagnostics ───────────────────────────────────────────────
            WorkerResultPayload::LspDiagnostics {
                uri, diagnostics, ..
            } => {
                eprintln!(
                    "[AppShell] LSP diagnostics: {} issue(s) in {uri}",
                    diagnostics.len()
                );
                // TODO: route to Problems panel in workbench
            }

            // ── LSP log messages ──────────────────────────────────────────────
            WorkerResultPayload::LspLogMessage { level, message } => {
                eprintln!("[LSP/{level}] {message}");
            }

            _ => {}
        }
    }

    fn on_stale_result(&mut self, _stale: WorkerResult) {}
}

// ── ApplicationHandler ────────────────────────────────────────────────────────

impl ApplicationHandler for AppShell {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }

        let attrs = Window::default_attributes()
            .with_title(WINDOW_TITLE)
            .with_inner_size(PhysicalSize::new(WINDOW_WIDTH, WINDOW_HEIGHT));

        let window = match event_loop.create_window(attrs) {
            Ok(w) => Arc::new(w),
            Err(e) => {
                eprintln!("[AppShell] create_window failed: {e}");
                event_loop.exit();
                return;
            }
        };

        // wgpu init is async; block_on keeps us on the main thread until ready.
        let mut renderer = match pollster::block_on(Renderer::new(window.clone())) {
            Ok(r) => r,
            Err(e) => {
                eprintln!("[AppShell] renderer init failed: {e}");
                event_loop.exit();
                return;
            }
        };
        renderer.apply_theme(self.theme.clone());
        self.editor_needs_layout = true;
        self.editor_caret_needs_layout = false;
        self.sidebar_needs_layout = true;
        self.terminal_needs_layout = true;
        self.last_editor_bounds = None;
        self.last_sidebar_bounds = None;
        self.last_terminal_bounds = None;

        self.window_size = window.inner_size();
        self.window = Some(window);
        self.renderer = Some(renderer);

        self.startup_subsystems();
        self.request_redraw();
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        window_id: WindowId,
        event: WindowEvent,
    ) {
        if self.window.as_ref().is_some_and(|w| w.id() != window_id) {
            return;
        }

        match event {
            // ── Window lifecycle ──────────────────────────────────────────────
            WindowEvent::CloseRequested => event_loop.exit(),

            WindowEvent::Resized(new_size) => {
                self.window_size = new_size;
                if let Some(r) = self.renderer.as_mut() {
                    r.resize(new_size);
                }
                self.editor_needs_layout = true;
                self.editor_caret_needs_layout = false;
                self.sidebar_needs_layout = true;
                self.terminal_needs_layout = true;
                self.last_editor_bounds = None;
                self.last_sidebar_bounds = None;
                self.last_terminal_bounds = None;
                self.request_redraw();
            }

            WindowEvent::Focused(focused) => {
                self.input_handler.on_focus_changed(focused);
            }

            // ── Modifiers ─────────────────────────────────────────────────────
            WindowEvent::ModifiersChanged(mods) => {
                self.input_handler.update_modifiers(mods);
            }

            // ── IME commit (Vietnamese, Japanese, etc.) ───────────────────────
            WindowEvent::Ime(Ime::Commit(text)) => {
                let context = self.build_context();
                if let Some(translated) = self.input_handler.translate_ime_commit(&text, context)
                    && self.handle_command(translated.command)
                {
                    self.request_redraw();
                }
            }

            // ── Keyboard ──────────────────────────────────────────────────────
            WindowEvent::KeyboardInput {
                event: key_event, ..
            } => {
                let context = self.build_context();
                let outcome =
                    self.input_handler
                        .translate_key_event(&key_event, &self.input_map, context);

                match outcome {
                    Some(InputRouteOutcome::Dispatch(translated)) => {
                        if self.handle_command(translated.command) {
                            self.request_redraw();
                        }
                    }

                    Some(InputRouteOutcome::NoDispatch { .. }) | None => {}
                }
            }

            // ── Render ────────────────────────────────────────────────────────
            WindowEvent::RedrawRequested => {
                let got_new_data = self.pump_bridge();

                let layout = self
                    .layout_engine
                    .compute(self.window_size, &self.panel_state);

                let flat_regions: Vec<_> = layout.model.flatten();
                let sidebar_region = flat_regions.iter().find(|r| r.id == RegionId::LeftSidebar);
                let bottom_region = flat_regions.iter().find(|r| r.id == RegionId::BottomPanel);

                let sidebar_bounds = sidebar_region.and_then(|sidebar| {
                    sidebar.visible.then_some([
                        sidebar.bounds.x,
                        sidebar.bounds.y,
                        sidebar.bounds.width,
                        sidebar.bounds.height,
                    ])
                });

                let sidebar_active_line_idx = if sidebar_bounds.is_some() {
                    self.ensure_explorer_snapshot();
                    active_line_idx_for_cursor(
                        self.explorer_cursor,
                        self.explorer_snapshot.entries.len(),
                    )
                } else {
                    None
                };

                let mut region_instances: Vec<RegionDrawInstance> = flat_regions
                    .iter()
                    .copied()
                    .filter(|r| {
                        r.visible && r.id != RegionId::Root && r.id != RegionId::OverlayLayer
                    })
                    .map(|r| {
                        RegionDrawInstance::new(
                            [r.bounds.x, r.bounds.y, r.bounds.width, r.bounds.height],
                            region_color(r.id, &self.theme),
                        )
                    })
                    .collect();

                if let (Some(bounds), Some(active_line_idx)) =
                    (sidebar_bounds, sidebar_active_line_idx)
                {
                    let row_rect = [
                        bounds[0] + 2.0,
                        bounds[1]
                            + EXPLORER_PADDING
                            + active_line_idx as f32 * self.theme.ui.sidebar_line_height,
                        (bounds[2] - 4.0).max(0.0),
                        self.theme.ui.sidebar_line_height,
                    ];
                    region_instances.push(RegionDrawInstance::new(
                        row_rect,
                        self.theme.ui.selection_bg.as_f32(),
                    ));
                }

                // Update text + caret for the center editor region
                if let Some(center) = layout.model.find(RegionId::Center) {
                    let center_bounds = [center.x, center.y, center.width, center.height];
                    let bounds_changed = self.last_editor_bounds != Some(center_bounds);

                    if self.editor_needs_layout || bounds_changed {
                        let text = self.app_state.text_string();
                        let styled = syntax_spans_to_styled(&self.highlight_spans, &self.theme);
                        if let Some(r) = self.renderer.as_mut() {
                            r.update_editor_content(&text, &self.app_state, center_bounds, &styled);
                        }
                        self.last_editor_bounds = Some(center_bounds);
                        self.editor_needs_layout = false;
                        self.editor_caret_needs_layout = false;
                    } else if self.editor_caret_needs_layout {
                        if let Some(r) = self.renderer.as_mut() {
                            r.update_editor_caret(&self.app_state, center_bounds);
                        }
                        self.editor_caret_needs_layout = false;
                    }
                }

                if let Some(r) = self.renderer.as_mut() {
                    if let Some(bounds) = sidebar_bounds {
                        let bounds_changed = self.last_sidebar_bounds != Some(bounds);
                        if self.sidebar_needs_layout || bounds_changed {
                            let root_name = self
                                .app_state
                                .workspace_root_path()
                                .and_then(|r| r.file_name().and_then(|n| n.to_str()))
                                .unwrap_or("workspace");
                            let sidebar_text = build_explorer_text(
                                root_name,
                                &self.explorer_snapshot.entries,
                                self.explorer_cursor,
                            );
                            r.update_sidebar_content(&sidebar_text, bounds);
                            self.last_sidebar_bounds = Some(bounds);
                            self.sidebar_needs_layout = false;
                        }
                    } else if self.last_sidebar_bounds.is_some() {
                        r.clear_sidebar();
                        self.last_sidebar_bounds = None;
                    }
                }

                if let (Some(bottom), Some(r)) = (bottom_region, self.renderer.as_mut()) {
                    if bottom.visible {
                        let bottom_bounds = [
                            bottom.bounds.x,
                            bottom.bounds.y,
                            bottom.bounds.width,
                            bottom.bounds.height,
                        ];
                        let bounds_changed = self.last_terminal_bounds != Some(bottom_bounds);
                        if self.terminal_needs_layout || bounds_changed {
                            r.update_terminal_content(&self.terminal_grid, bottom_bounds);
                            self.last_terminal_bounds = Some(bottom_bounds);
                            self.terminal_needs_layout = false;
                        }
                    } else if self.last_terminal_bounds.is_some() {
                        r.clear_terminal();
                        self.last_terminal_bounds = None;
                    }
                }

                if let Some(r) = self.renderer.as_mut() {
                    match r.render(&region_instances) {
                        Ok(()) => {}
                        Err(RenderError::Outdated) | Err(RenderError::Lost) => {
                            r.reconfigure_surface();
                        }
                        Err(RenderError::Timeout) | Err(RenderError::Occluded) => {}
                        Err(RenderError::Validation) => {
                            eprintln!("[AppShell] render validation error");
                        }
                    }
                }

                if got_new_data {
                    self.request_redraw();
                }
            }

            _ => {}
        }
    }

    /// Called when the event queue drains — ideal place to pump the async bridge
    /// so results flow in even without user activity.
    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        if self.pump_bridge() {
            self.request_redraw();
        }
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn syntax_spans_to_styled(spans: &[HighlightSpan], theme: &ThemeConfig) -> Vec<StyledTextSpan> {
    spans
        .iter()
        .map(|span| {
            let color = match span.category {
                crate::syntax::highlight::HighlightCategory::Keyword => {
                    theme.syntax.keyword.as_u8()
                }
                crate::syntax::highlight::HighlightCategory::String => theme.syntax.string.as_u8(),
                crate::syntax::highlight::HighlightCategory::Comment => {
                    theme.syntax.comment.as_u8()
                }
                crate::syntax::highlight::HighlightCategory::Type => theme.syntax.r#type.as_u8(),
                crate::syntax::highlight::HighlightCategory::Function => {
                    theme.syntax.function.as_u8()
                }
                crate::syntax::highlight::HighlightCategory::Number => theme.syntax.number.as_u8(),
            };
            StyledTextSpan::new(span.range.start, span.range.end, color)
        })
        .collect()
}

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

fn collect_explorer_entries(
    app_state: &AppState,
    expanded: &HashSet<PathBuf>,
) -> Vec<ExplorerEntry> {
    let Some(nodes) = app_state.workspace_nodes() else {
        return Vec::new();
    };
    let root = match app_state.workspace_root_path() {
        Some(r) => r.to_path_buf(),
        None => return Vec::new(),
    };

    let mut node_types: HashMap<PathBuf, WorkspaceNodeType> = HashMap::new();
    let mut children_map: HashMap<PathBuf, Vec<PathBuf>> = HashMap::new();

    for node in nodes {
        node_types.insert(node.path.clone(), node.file_type);
    }

    for node in nodes.iter() {
        if node.path == root {
            continue;
        }
        let Some(parent) = node.path.parent() else {
            continue;
        };
        if !parent.starts_with(&root) {
            continue;
        }
        children_map
            .entry(parent.to_path_buf())
            .or_default()
            .push(node.path.clone());
    }

    for children in children_map.values_mut() {
        children.sort_by(|left, right| {
            let left_type = node_types
                .get(left)
                .copied()
                .unwrap_or(WorkspaceNodeType::File);
            let right_type = node_types
                .get(right)
                .copied()
                .unwrap_or(WorkspaceNodeType::File);
            let left_rank = if left_type == WorkspaceNodeType::Folder {
                0
            } else {
                1
            };
            let right_rank = if right_type == WorkspaceNodeType::Folder {
                0
            } else {
                1
            };
            left_rank.cmp(&right_rank).then_with(|| left.cmp(right))
        });
    }

    let mut entries = Vec::new();
    collect_visible_explorer_entries(&root, 0, &node_types, &children_map, expanded, &mut entries);
    entries
}

fn collect_visible_explorer_entries(
    parent: &Path,
    depth: usize,
    node_types: &HashMap<PathBuf, WorkspaceNodeType>,
    children_map: &HashMap<PathBuf, Vec<PathBuf>>,
    expanded: &HashSet<PathBuf>,
    out: &mut Vec<ExplorerEntry>,
) {
    let Some(children) = children_map.get(parent) else {
        return;
    };

    for child in children {
        let file_type = node_types
            .get(child)
            .copied()
            .unwrap_or(WorkspaceNodeType::File);
        let is_expanded = file_type == WorkspaceNodeType::Folder && expanded.contains(child);
        let name = child.file_name().and_then(|n| n.to_str()).unwrap_or("?");

        out.push(ExplorerEntry {
            path: child.clone(),
            parent_path: Some(parent.to_path_buf()),
            file_type,
            depth,
            is_expanded,
            name: name.to_string(),
        });

        if file_type == WorkspaceNodeType::Folder && is_expanded {
            collect_visible_explorer_entries(
                child,
                depth + 1,
                node_types,
                children_map,
                expanded,
                out,
            );
        }
    }
}

fn build_explorer_text(root_name: &str, entries: &[ExplorerEntry], selected_idx: usize) -> String {
    let mut lines: Vec<String> = vec![format!("[ {root_name} ]")];
    if entries.is_empty() {
        lines.push("  (no files)".to_string());
        return lines.join("\n");
    }

    let selected = selected_idx.min(entries.len().saturating_sub(1));
    for (idx, entry) in entries.iter().enumerate() {
        let prefix = if idx == selected { "› " } else { "  " };
        let indent = "  ".repeat(entry.depth);
        let line = match entry.file_type {
            WorkspaceNodeType::Folder => {
                let icon = if entry.is_expanded { "▼" } else { "▶" };
                format!("{prefix}{indent}{icon} {}", entry.name)
            }
            WorkspaceNodeType::File => format!("{prefix}{indent}  {}", entry.name),
        };
        lines.push(line);
    }
    lines.join("\n")
}

fn active_line_idx_for_cursor(cursor: usize, entries_len: usize) -> Option<usize> {
    if entries_len == 0 {
        None
    } else {
        Some(cursor.min(entries_len.saturating_sub(1)) + 1)
    }
}

fn region_color(id: RegionId, theme: &ThemeConfig) -> [f32; 4] {
    match id {
        RegionId::TopBar => theme.ui.panel_bg.as_f32(),
        RegionId::LeftSidebar => theme.ui.sidebar_bg.as_f32(),
        RegionId::Center => theme.editor.bg.as_f32(),
        RegionId::RightSidebar => theme.ui.sidebar_bg.as_f32(),
        RegionId::BottomPanel => theme.ui.panel_bg.as_f32(),
        RegionId::StatusBar => theme.ui.status_bar_bg.as_f32(),
        _ => theme.ui.border_color.as_f32(),
    }
}

fn language_id_for_path(path: &Path) -> String {
    match path
        .extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| ext.to_ascii_lowercase())
        .as_deref()
    {
        Some("rs") => "rust",
        Some("js") | Some("mjs") | Some("cjs") => "javascript",
        Some("ts") | Some("tsx") => "typescript",
        Some("jsx") => "javascriptreact",
        Some("py") => "python",
        Some("go") => "go",
        Some("json") => "json",
        Some("toml") => "toml",
        Some("md") => "markdown",
        _ => "plaintext",
    }
    .to_string()
}

// ── Entry point ───────────────────────────────────────────────────────────────

pub fn run() -> Result<(), winit::error::EventLoopError> {
    let event_loop = EventLoop::new()?;
    event_loop.set_control_flow(ControlFlow::Wait);

    let mut app = match AppShell::new() {
        Ok(a) => a,
        Err(e) => {
            eprintln!("[fatal] AppShell::new failed: {e}");
            return Ok(());
        }
    };

    event_loop.run_app(&mut app)
}
