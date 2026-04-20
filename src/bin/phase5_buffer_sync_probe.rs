use std::{
    path::PathBuf,
    sync::Arc,
    time::{Duration, Instant},
};

use cosmic_text::Metrics;
use netherize_editor::{
    app::{
        app_state::AppState,
        async_bridge::{AppAsyncBridge, AsyncResultRouter},
        editor_view_model::{EditorViewModel, ViewSyncStats, ViewportState},
        input::{InputHandler, InputRouteOutcome},
        input_map::{InputMap, KeybindingContext},
        panel_layout::{EditorPanelLayout, PanelLayoutConfig, PanelRect},
        revision_state::RevisionState,
    },
    async_runtime::{
        message::{
            FileSystemEvent, RequestSpec, RequestTopic, WorkerEvent, WorkerEventKind,
            WorkerRequestPayload, WorkerResult, WorkerResultPayload,
        },
        scheduler::AsyncScheduler,
    },
    core::mode::EditorMode,
    render::{
        caret::CaretPipeline, glyph_instance::GlyphInstance, surface::SurfaceState,
        text_pipeline::TextPipeline,
    },
    syntax::{highlight::HighlightSpan, syntax_engine::LanguageId},
    terminal::grid::TerminalGrid,
    text::{atlas::GlyphAtlas, raster::rasterize_glyph_alpha, text_system::TextSystem},
};
use winit::{
    application::ApplicationHandler,
    dpi::PhysicalSize,
    event::{ElementState, Ime, KeyEvent, WindowEvent},
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop},
    keyboard::{Key, KeyCode, NamedKey, PhysicalKey},
    window::{Window, WindowId},
};

const WINDOW_WIDTH: u32 = 1100;
const WINDOW_HEIGHT: u32 = 760;
const WINDOW_TITLE: &str = "Netherize Phase5 - Buffer Layout Sync Probe";

const INITIAL_TEXT: &str = "Hello Apple Silicon";
const DEFAULT_SAVE_PATH: &str = "phase5_scratch.txt";
const DEFAULT_OPEN_PATH: &str = "phase5_open_sample.txt";
const DEFAULT_OPEN_CONTENT: &str =
    "Phase5 open sample.\nUse Cmd/Ctrl+S to save and Cmd/Ctrl+O to reload this file.\n";

const HUD_ORIGIN_X: f32 = 16.0;
const HUD_ORIGIN_Y: f32 = 12.0;
const HUD_HEIGHT: f32 = 300.0;
const CONTENT_MARGIN_X: f32 = 40.0;
const CONTENT_TOP_Y: f32 = HUD_ORIGIN_Y + HUD_HEIGHT + 12.0;
const CONTENT_BOTTOM_MARGIN: f32 = 16.0;
const PANEL_SPLIT_RATIO: f32 = 0.70;
const PANEL_SPLIT_GAP: f32 = 8.0;

const BG_COLOR: wgpu::Color = wgpu::Color {
    r: 0.06,
    g: 0.07,
    b: 0.10,
    a: 1.0,
};
const TEXT_COLOR: [f32; 4] = [0.92, 0.94, 0.98, 1.0];
const CARET_COLOR: [f32; 4] = [0.98, 0.62, 0.20, 0.95];
const HUD_TEXT_COLOR: [f32; 4] = [0.70, 0.78, 0.92, 0.95];
const HUD_TEXT_COLOR_U8: [u8; 4] = [178, 199, 235, 242];
const TERMINAL_TEXT_COLOR: [f32; 4] = [0.82, 0.86, 0.94, 0.95];
const TERMINAL_TEXT_COLOR_U8: [u8; 4] = [209, 219, 240, 242];
const TERMINAL_FONT_SIZE: f32 = 16.0;
const TERMINAL_LINE_HEIGHT: f32 = 20.0;
const TERMINAL_GRID_COLS: usize = 120;
const TERMINAL_GRID_ROWS: usize = 30;

const DEBUG_CPU_BURN_MS: u64 = 1_800;
const DEBUG_SEARCH_DELAY_MS: u64 = 900;
const IDLE_PUMP_INTERVAL_MS: u64 = 33;
const LARGE_FILE_LINE_THRESHOLD: usize = 50_000;
const LARGE_FILE_BYTE_THRESHOLD: usize = 1_000_000;
const LARGE_FILE_RENDER_PREVIEW_CHARS: usize = 16_000;

#[derive(Debug, Clone)]
struct AcceptedSyntaxResult {
    revision_id: u64,
    file_path: Option<PathBuf>,
    language_id: LanguageId,
    spans: Vec<HighlightSpan>,
    line_count: usize,
    char_count: usize,
    byte_count: usize,
    parse_time_ms: u128,
    highlight_time_ms: u128,
}

#[derive(Debug, Clone)]
enum PendingPtyResult {
    Spawned {
        session_id: u64,
        shell: String,
        working_dir: PathBuf,
    },
    Output {
        session_id: u64,
        chunk: String,
    },
    InputWritten {
        session_id: u64,
        bytes: usize,
    },
    Closed {
        session_id: u64,
        exit_status: Option<i32>,
        reason: String,
    },
}

struct TerminalUiState {
    grid: TerminalGrid,
    session_id: Option<u64>,
    spawn_pending: bool,
    revision_id: u64,
    cols: usize,
    rows: usize,
    last_notice: String,
}

impl Default for TerminalUiState {
    fn default() -> Self {
        Self {
            grid: TerminalGrid::new(TERMINAL_GRID_COLS, TERMINAL_GRID_ROWS),
            session_id: None,
            spawn_pending: false,
            revision_id: 0,
            cols: TERMINAL_GRID_COLS,
            rows: TERMINAL_GRID_ROWS,
            last_notice: "terminal idle".to_string(),
        }
    }
}

impl TerminalUiState {
    fn ensure_grid_size(&mut self, cols: usize, rows: usize) -> bool {
        let cols = cols.max(1);
        let rows = rows.max(1);
        if cols == self.cols && rows == self.rows {
            return false;
        }
        self.cols = cols;
        self.rows = rows;
        self.grid = TerminalGrid::new(cols, rows);
        self.last_notice = format!("terminal grid resized to {}x{}", cols, rows);
        true
    }

    fn render_snapshot(&self, max_rows: usize) -> String {
        let used_rows = self.grid.used_rows().max(1);
        let start_row = used_rows.saturating_sub(max_rows.max(1));
        let mut output = String::new();
        for row in start_row..used_rows {
            for col in 0..self.cols {
                output.push(self.grid.cell_at(row, col).ch);
            }
            while output.ends_with(' ') {
                output.pop();
            }
            output.push('\n');
        }
        output
    }

    fn session_label(&self) -> String {
        match self.session_id {
            Some(session_id) => format!("session#{session_id}"),
            None if self.spawn_pending => "starting".to_string(),
            None => "none".to_string(),
        }
    }
}

#[derive(Debug)]
struct AsyncUiState {
    revisions: RevisionState,
    pending_jobs: usize,
    accepted_results: usize,
    stale_results: usize,
    failed_jobs: usize,
    last_message: String,
    pending_syntax_result: Option<AcceptedSyntaxResult>,
    pending_file_events: Vec<FileSystemEvent>,
    pending_pty_results: Vec<PendingPtyResult>,
}

impl Default for AsyncUiState {
    fn default() -> Self {
        Self {
            revisions: RevisionState::default(),
            pending_jobs: 0,
            accepted_results: 0,
            stale_results: 0,
            failed_jobs: 0,
            last_message: "idle".to_string(),
            pending_syntax_result: None,
            pending_file_events: Vec::new(),
            pending_pty_results: Vec::new(),
        }
    }
}

impl AsyncUiState {
    fn bump_revision(&mut self, topic: RequestTopic) -> u64 {
        self.revisions.bump(topic)
    }

    fn current_revision(&self, topic: RequestTopic) -> u64 {
        self.revisions.current(topic)
    }

    fn take_pending_syntax_result(&mut self) -> Option<AcceptedSyntaxResult> {
        self.pending_syntax_result.take()
    }

    fn take_pending_file_events(&mut self) -> Vec<FileSystemEvent> {
        std::mem::take(&mut self.pending_file_events)
    }

    fn take_pending_pty_results(&mut self) -> Vec<PendingPtyResult> {
        std::mem::take(&mut self.pending_pty_results)
    }

    fn status_line(&self) -> String {
        format!(
            "Async p:{} ok:{} stale:{} fail:{}",
            self.pending_jobs, self.accepted_results, self.stale_results, self.failed_jobs
        )
    }
}

impl AsyncResultRouter for AsyncUiState {
    fn current_revision_for(&self, topic: RequestTopic) -> u64 {
        self.revisions.current(topic)
    }

    fn on_worker_event(&mut self, event: WorkerEvent) {
        match event.kind {
            WorkerEventKind::Started => {
                self.pending_jobs += 1;
                self.last_message = format!(
                    "started request_id={} rev={} topic={:?}",
                    event.request_id, event.revision_id, event.topic
                );
            }
            WorkerEventKind::Completed => {
                self.pending_jobs = self.pending_jobs.saturating_sub(1);
                self.last_message = format!(
                    "completed request_id={} rev={} topic={:?}",
                    event.request_id, event.revision_id, event.topic
                );
            }
            WorkerEventKind::Failed { ref error } => {
                self.pending_jobs = self.pending_jobs.saturating_sub(1);
                self.failed_jobs += 1;
                self.last_message = format!(
                    "failed request_id={} rev={} topic={:?} kind={:?} error={}",
                    event.request_id, event.revision_id, event.topic, error.kind, error.message
                );
            }
            WorkerEventKind::Cancelled { ref reason } => {
                self.pending_jobs = self.pending_jobs.saturating_sub(1);
                self.last_message = format!(
                    "cancelled request_id={} rev={} topic={:?} reason={}",
                    event.request_id, event.revision_id, event.topic, reason
                );
            }
        }

        println!("[App Async] {}", self.last_message);
    }

    fn on_worker_result(&mut self, result: WorkerResult) {
        self.accepted_results += 1;

        self.last_message = match result.payload {
            WorkerResultPayload::ParseAndHighlight {
                file_path,
                language_id,
                spans,
                line_count,
                char_count,
                byte_count,
                parse_time_ms,
                highlight_time_ms,
            } => {
                self.pending_syntax_result = Some(AcceptedSyntaxResult {
                    revision_id: result.revision_id,
                    file_path: file_path.clone(),
                    language_id,
                    spans,
                    line_count,
                    char_count,
                    byte_count,
                    parse_time_ms,
                    highlight_time_ms,
                });
                let target = file_path
                    .as_ref()
                    .map(|path| path.display().to_string())
                    .unwrap_or_else(|| "<active-buffer>".to_string());
                format!(
                    "syntax accepted request_id={} rev={} language={} file={} lines={} chars={} bytes={} parse_ms={} highlight_ms={}",
                    result.request_id,
                    result.revision_id,
                    language_id.as_str(),
                    target,
                    line_count,
                    char_count,
                    byte_count,
                    parse_time_ms,
                    highlight_time_ms
                )
            }
            WorkerResultPayload::ParseSummary {
                file_path,
                line_count,
                char_count,
            } => format!(
                "parse accepted request_id={} rev={} file={} lines={} chars={}",
                result.request_id,
                result.revision_id,
                file_path.display(),
                line_count,
                char_count
            ),
            WorkerResultPayload::SearchMatches { query, matches } => format!(
                "search accepted request_id={} rev={} query={:?} matches={}",
                result.request_id,
                result.revision_id,
                query,
                matches.len()
            ),
            WorkerResultPayload::CpuBurnSummary {
                job_label,
                busy_millis,
                checksum,
            } => format!(
                "cpu accepted request_id={} rev={} label={} busy_ms={} checksum={:#x}",
                result.request_id, result.revision_id, job_label, busy_millis, checksum
            ),
            WorkerResultPayload::FileSystemEvents { root_path, events } => {
                let count = events.len();
                self.pending_file_events.extend(events);
                format!(
                    "file-watch accepted request_id={} rev={} root={} events={}",
                    result.request_id,
                    result.revision_id,
                    root_path.display(),
                    count
                )
            }
            WorkerResultPayload::PtySpawned {
                session_id,
                shell,
                working_dir,
            } => {
                self.pending_pty_results.push(PendingPtyResult::Spawned {
                    session_id,
                    shell: shell.clone(),
                    working_dir: working_dir.clone(),
                });
                format!(
                    "pty spawned request_id={} rev={} session_id={} shell={} cwd={}",
                    result.request_id,
                    result.revision_id,
                    session_id,
                    shell,
                    working_dir.display()
                )
            }
            WorkerResultPayload::PtyOutput { session_id, chunk } => {
                self.pending_pty_results.push(PendingPtyResult::Output {
                    session_id,
                    chunk: chunk.clone(),
                });
                format!(
                    "pty output request_id={} rev={} session_id={} bytes={}",
                    result.request_id,
                    result.revision_id,
                    session_id,
                    chunk.len()
                )
            }
            WorkerResultPayload::PtyInputWritten { session_id, bytes } => {
                self.pending_pty_results
                    .push(PendingPtyResult::InputWritten { session_id, bytes });
                format!(
                    "pty input written request_id={} rev={} session_id={} bytes={}",
                    result.request_id, result.revision_id, session_id, bytes
                )
            }
            WorkerResultPayload::PtySessionClosed {
                session_id,
                exit_status,
                reason,
            } => {
                self.pending_pty_results.push(PendingPtyResult::Closed {
                    session_id,
                    exit_status,
                    reason: reason.clone(),
                });
                format!(
                    "pty closed request_id={} rev={} session_id={} exit_status={:?} reason={}",
                    result.request_id, result.revision_id, session_id, exit_status, reason
                )
            }
            WorkerResultPayload::LspServerStarted {
                server_name,
                root_path,
                capabilities_summary,
            } => format!(
                "lsp started request_id={} rev={} server={} root={} capabilities={}",
                result.request_id,
                result.revision_id,
                server_name,
                root_path.display(),
                capabilities_summary
            ),
            WorkerResultPayload::LspServerStopped {
                exit_status,
                reason,
            } => format!(
                "lsp stopped request_id={} rev={} exit_status={:?} reason={}",
                result.request_id, result.revision_id, exit_status, reason
            ),
            WorkerResultPayload::LspAck {
                action,
                uri,
                version,
            } => format!(
                "lsp ack request_id={} rev={} action={} uri={:?} version={:?}",
                result.request_id, result.revision_id, action, uri, version
            ),
            WorkerResultPayload::LspDiagnostics {
                uri,
                version,
                diagnostics,
            } => format!(
                "lsp diagnostics request_id={} rev={} uri={} version={:?} count={}",
                result.request_id,
                result.revision_id,
                uri,
                version,
                diagnostics.len()
            ),
            WorkerResultPayload::LspLogMessage { level, message } => format!(
                "lsp log request_id={} rev={} level={} message={}",
                result.request_id, result.revision_id, level, message
            ),
        };

        println!("[App Async] {}", self.last_message);
    }

    fn on_stale_result(&mut self, stale: WorkerResult) {
        self.stale_results += 1;
        self.last_message = format!(
            "stale discarded request_id={} rev={} topic={:?}",
            stale.request_id, stale.revision_id, stale.topic
        );
        println!("[App Async] {}", self.last_message);
    }
}

struct Phase5Renderer {
    device: wgpu::Device,
    queue: wgpu::Queue,
    surface_state: SurfaceState,
    text_system: TextSystem,
    hud_text_system: TextSystem,
    terminal_text_system: TextSystem,
    atlas: GlyphAtlas,
    text_pipeline: TextPipeline,
    caret_pipeline: CaretPipeline,
    panel_config: PanelLayoutConfig,
    panel_layout: EditorPanelLayout,
    terminal_visible: bool,
}

impl Phase5Renderer {
    async fn new(window: Arc<Window>, terminal_visible: bool) -> Result<Self, String> {
        let window_size = window.inner_size();
        let panel_config = PanelLayoutConfig {
            outer_margin_x: CONTENT_MARGIN_X,
            content_top: CONTENT_TOP_Y,
            content_bottom_margin: CONTENT_BOTTOM_MARGIN,
            hud_origin_x: HUD_ORIGIN_X,
            hud_origin_y: HUD_ORIGIN_Y,
            hud_height: HUD_HEIGHT,
            split_ratio: PANEL_SPLIT_RATIO,
            split_gap: PANEL_SPLIT_GAP,
        };
        let panel_layout =
            EditorPanelLayout::from_window(window_size, terminal_visible, panel_config);

        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: wgpu::Backends::all(),
            ..wgpu::InstanceDescriptor::new_without_display_handle()
        });

        let surface = instance
            .create_surface(window)
            .map_err(|err| format!("create_surface failed: {err}"))?;

        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
            })
            .await
            .map_err(|err| format!("request_adapter failed: {err}"))?;

        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: Some("Netherize Phase5 Device"),
                required_features: wgpu::Features::empty(),
                required_limits: wgpu::Limits::default(),
                experimental_features: Default::default(),
                memory_hints: Default::default(),
                trace: Default::default(),
            })
            .await
            .map_err(|err| format!("request_device failed: {err}"))?;

        let surface_state = SurfaceState::new(surface, window_size, &adapter, &device)?;
        let mut text_system = TextSystem::new(
            Metrics::new(34.0, 44.0),
            Some(panel_layout.editor.width.max(1.0)),
            Some(panel_layout.editor.height.max(1.0)),
        );
        text_system.set_text(INITIAL_TEXT);
        let mut hud_text_system = TextSystem::new(
            Metrics::new(16.0, 22.0),
            Some(panel_layout.hud.width.max(1.0)),
            Some(panel_layout.hud.height.max(1.0)),
        );
        hud_text_system.set_text("");
        let mut terminal_text_system = TextSystem::new(
            Metrics::new(TERMINAL_FONT_SIZE, TERMINAL_LINE_HEIGHT),
            Some(panel_layout.terminal.width.max(1.0)),
            Some(panel_layout.terminal.height.max(1.0)),
        );
        terminal_text_system.set_text("");

        let atlas = GlyphAtlas::new(&device, 2048, 2048);
        let text_pipeline = TextPipeline::new(
            &device,
            surface_state.config.format,
            &atlas,
            surface_state.size.width,
            surface_state.size.height,
        );
        let caret_pipeline = CaretPipeline::new(
            &device,
            surface_state.config.format,
            surface_state.size.width,
            surface_state.size.height,
        );

        Ok(Self {
            device,
            queue,
            surface_state,
            text_system,
            hud_text_system,
            terminal_text_system,
            atlas,
            text_pipeline,
            caret_pipeline,
            panel_config,
            panel_layout,
            terminal_visible,
        })
    }

    fn panel_layout(&self) -> EditorPanelLayout {
        self.panel_layout
    }

    fn terminal_max_visible_rows(&self) -> usize {
        if !self.terminal_visible || self.panel_layout.terminal.height <= 0.0 {
            return 0;
        }
        ((self.panel_layout.terminal.height / TERMINAL_LINE_HEIGHT).floor() as usize).max(1)
    }

    fn reconfigure_panels(&mut self, terminal_visible: bool) {
        self.terminal_visible = terminal_visible;
        self.panel_layout = EditorPanelLayout::from_window(
            self.surface_state.size,
            self.terminal_visible,
            self.panel_config,
        );
        self.text_system.set_size(
            Some(self.panel_layout.editor.width.max(1.0)),
            Some(self.panel_layout.editor.height.max(1.0)),
        );
        self.hud_text_system.set_size(
            Some(self.panel_layout.hud.width.max(1.0)),
            Some(self.panel_layout.hud.height.max(1.0)),
        );
        self.terminal_text_system.set_size(
            Some(self.panel_layout.terminal.width.max(1.0)),
            Some(self.panel_layout.terminal.height.max(1.0)),
        );
    }

    fn resize(&mut self, new_size: PhysicalSize<u32>, terminal_visible: bool) {
        if new_size.width == 0 || new_size.height == 0 {
            return;
        }

        self.surface_state.resize(&self.device, new_size);
        self.text_pipeline
            .update_screen_size(&self.queue, new_size.width, new_size.height);
        self.caret_pipeline
            .update_screen_size(&self.queue, new_size.width, new_size.height);
        self.reconfigure_panels(terminal_visible);
    }

    fn sync_view_model(
        &mut self,
        app_state: &AppState,
        render_text: &str,
        hud_text: &str,
        terminal_text: Option<&str>,
        show_editor_caret: bool,
        view_model: &mut EditorViewModel,
    ) -> Result<ViewSyncStats, String> {
        let sync_stats = view_model.sync_projection_with_text(
            app_state,
            &mut self.text_system,
            &mut self.atlas,
            &self.queue,
            render_text,
        )?;
        // Gộp glyph của editor content + HUD debug/status vào cùng một instance buffer.
        // Cách này không đụng vào layout/caret logic của buffer chính.
        let mut instances = view_model.glyph_instances().to_vec();
        let hud_instances = self.build_hud_instances(hud_text)?;
        instances.extend(hud_instances);
        if let Some(terminal_text) = terminal_text {
            let terminal_instances = self.build_terminal_instances(terminal_text)?;
            instances.extend(terminal_instances);
        }

        self.text_pipeline
            .upload_instances(&self.device, &self.queue, &instances);
        let caret = if show_editor_caret {
            view_model.caret_rect()
        } else {
            None
        };
        self.caret_pipeline.upload_caret(&self.queue, caret);
        Ok(sync_stats)
    }

    fn build_hud_instances(&mut self, hud_text: &str) -> Result<Vec<GlyphInstance>, String> {
        Self::build_text_instances(
            &mut self.atlas,
            &self.queue,
            &mut self.hud_text_system,
            hud_text,
            HUD_TEXT_COLOR_U8,
            HUD_TEXT_COLOR,
            self.panel_layout.hud.origin(),
        )
    }

    fn build_terminal_instances(
        &mut self,
        terminal_text: &str,
    ) -> Result<Vec<GlyphInstance>, String> {
        if !self.terminal_visible || self.panel_layout.terminal.height <= 0.0 {
            return Ok(Vec::new());
        }
        Self::build_text_instances(
            &mut self.atlas,
            &self.queue,
            &mut self.terminal_text_system,
            terminal_text,
            TERMINAL_TEXT_COLOR_U8,
            TERMINAL_TEXT_COLOR,
            self.panel_layout.terminal.origin(),
        )
    }

    fn build_text_instances(
        atlas: &mut GlyphAtlas,
        queue: &wgpu::Queue,
        text_system: &mut TextSystem,
        text: &str,
        color_u8: [u8; 4],
        fallback_color: [f32; 4],
        origin: [f32; 2],
    ) -> Result<Vec<GlyphInstance>, String> {
        if text.is_empty() {
            return Ok(Vec::new());
        }

        text_system.set_text_with_color(text, color_u8);
        let visible_glyphs =
            text_system.collect_visible_glyphs(origin[0], origin[1], fallback_color);
        let mut instances = Vec::with_capacity(visible_glyphs.len());

        for glyph in visible_glyphs {
            let entry = if let Some(entry) = atlas.get(glyph.cache_key) {
                entry
            } else {
                let Some(rasterized) = rasterize_glyph_alpha(text_system, glyph.cache_key) else {
                    continue;
                };
                atlas.get_or_insert(queue, glyph.cache_key, &rasterized)?
            };

            if entry.region.width == 0 || entry.region.height == 0 {
                continue;
            }

            let (uv_min, uv_max) = atlas.uv_min_max(entry.region);
            let top_left_x = glyph.physical_x + entry.placement_left;
            let top_left_y = glyph.physical_y - entry.placement_top;

            instances.push(GlyphInstance::new(
                [top_left_x as f32, top_left_y as f32],
                [entry.region.width as f32, entry.region.height as f32],
                uv_min,
                uv_max,
                glyph.color,
            ));
        }

        Ok(instances)
    }

    fn render(&mut self) -> Result<(), String> {
        let frame = match self.surface_state.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(surface_texture) => surface_texture,
            wgpu::CurrentSurfaceTexture::Suboptimal(surface_texture) => surface_texture,
            wgpu::CurrentSurfaceTexture::Timeout => return Ok(()),
            wgpu::CurrentSurfaceTexture::Occluded => return Ok(()),
            wgpu::CurrentSurfaceTexture::Outdated => {
                self.surface_state.reconfigure(&self.device);
                return Ok(());
            }
            wgpu::CurrentSurfaceTexture::Lost => {
                self.surface_state.reconfigure(&self.device);
                return Ok(());
            }
            wgpu::CurrentSurfaceTexture::Validation => {
                return Err("surface validation error".to_string());
            }
        };

        let view = frame
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Netherize Phase5 Encoder"),
            });

        {
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Netherize Phase5 RenderPass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(BG_COLOR),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });

            self.text_pipeline.draw(&mut render_pass);
            self.caret_pipeline.draw(&mut render_pass);
        }

        self.queue.submit(std::iter::once(encoder.finish()));
        frame.present();
        Ok(())
    }
}

struct Phase5App {
    window: Option<Arc<Window>>,
    renderer: Option<Phase5Renderer>,
    scheduler: Option<AsyncScheduler>,
    async_bridge: Option<AppAsyncBridge>,
    async_state: AsyncUiState,
    input_handler: InputHandler,
    input_map: InputMap,
    app_state: AppState,
    view_model: EditorViewModel,
    terminal_state: TerminalUiState,
    syntax_reparse_enabled: bool,
    large_file_reason: Option<String>,
    frame_counter: u32,
    frame_started_at: Instant,
}

impl Phase5App {
    fn new() -> Self {
        let save_path = PathBuf::from(DEFAULT_SAVE_PATH);
        let open_path = PathBuf::from(DEFAULT_OPEN_PATH);
        if let Err(err) = AppState::ensure_probe_file(open_path.as_path(), DEFAULT_OPEN_CONTENT) {
            eprintln!("{err}");
        }

        let (scheduler, async_bridge) = match AsyncScheduler::new() {
            Ok((scheduler, receiver)) => (Some(scheduler), Some(AppAsyncBridge::new(receiver))),
            Err(err) => {
                eprintln!("async scheduler init failed: {err}");
                (None, None)
            }
        };

        Self {
            window: None,
            renderer: None,
            scheduler,
            async_bridge,
            async_state: AsyncUiState::default(),
            input_handler: InputHandler::new(),
            input_map: InputMap::new(open_path),
            app_state: AppState::from_text(save_path, INITIAL_TEXT),
            view_model: EditorViewModel::new(
                ViewportState::new(CONTENT_MARGIN_X, CONTENT_TOP_Y),
                TEXT_COLOR,
                CARET_COLOR,
            ),
            terminal_state: TerminalUiState::default(),
            syntax_reparse_enabled: true,
            large_file_reason: None,
            frame_counter: 0,
            frame_started_at: Instant::now(),
        }
    }

    fn refresh_window_title(&self) {
        let Some(window) = &self.window else {
            return;
        };

        let dirty_mark = if self.app_state.is_dirty() { "*" } else { "" };
        let (line, col) = self.app_state.cursor_line_col();
        let syntax_mode = if self.syntax_reparse_enabled {
            "syntax:on"
        } else {
            "syntax:large-file-fallback"
        };
        let terminal_mode = if self.app_state.is_terminal_panel_open() {
            format!("terminal:{}", self.terminal_state.session_label())
        } else {
            "terminal:closed".to_string()
        };
        window.set_title(&format!(
            "{WINDOW_TITLE}{dirty_mark} | Ln {}, Col {} | {} | {} | {}",
            line + 1,
            col + 1,
            syntax_mode,
            terminal_mode,
            self.async_state.status_line()
        ));
    }

    fn sync_and_schedule_redraw(&mut self, force_redraw: bool) {
        let render_text = self.build_render_text_snapshot();
        let hud_text = self.build_hud_text();
        let terminal_text = if self.app_state.is_terminal_panel_open() {
            Some(self.build_terminal_panel_text())
        } else {
            None
        };

        let Some(renderer) = self.renderer.as_mut() else {
            return;
        };

        renderer.reconfigure_panels(self.app_state.is_terminal_panel_open());
        let panel_layout = renderer.panel_layout();
        self.view_model
            .set_viewport_origin(panel_layout.editor.origin());
        if self.app_state.is_terminal_panel_open() {
            let (cols, rows) = terminal_grid_dimensions(panel_layout.terminal);
            if self.terminal_state.ensure_grid_size(cols, rows) {
                println!("[Terminal] {}", self.terminal_state.last_notice);
            }
        }

        match renderer.sync_view_model(
            &self.app_state,
            &render_text,
            &hud_text,
            terminal_text.as_deref(),
            self.app_state.current_mode() != EditorMode::TerminalFocus,
            &mut self.view_model,
        ) {
            Ok(stats) => {
                println!(
                    "Sync: {} layout_rebuilt={} glyphs={} highlights={} syntax_rev={:?} caret_visible={} redraw_required={}",
                    stats.dirty_flags.summary(),
                    stats.layout_rebuilt,
                    stats.glyph_count,
                    stats.highlight_span_count,
                    stats.syntax_revision,
                    stats.caret_visible,
                    stats.redraw_required
                );
            }
            Err(err) => {
                eprintln!("sync failed: {err}");
            }
        }

        self.refresh_window_title();

        if force_redraw || self.view_model.take_redraw_required() {
            if let Some(window) = &self.window {
                window.request_redraw();
            }
        }
    }

    fn submit_worker_request(
        &mut self,
        topic: RequestTopic,
        revision_id: u64,
        payload: WorkerRequestPayload,
    ) {
        let Some(scheduler) = self.scheduler.as_ref() else {
            eprintln!("[UI] async scheduler is unavailable");
            return;
        };

        match scheduler.submit(RequestSpec {
            revision_id,
            topic,
            payload,
        }) {
            Ok(request) => {
                println!(
                    "[UI] submitted request_id={} revision={} topic={:?}",
                    request.request_id, request.revision_id, request.topic
                );
            }
            Err(err) => eprintln!("[UI] submit request failed: {err}"),
        }
        self.refresh_window_title();
    }

    fn evaluate_large_file_policy(&mut self) {
        let line_count = self.app_state.len_lines();
        let byte_count = self.app_state.len_bytes();
        let is_large_file =
            line_count > LARGE_FILE_LINE_THRESHOLD || byte_count > LARGE_FILE_BYTE_THRESHOLD;

        if is_large_file {
            let reason = format!(
                "lines={} bytes={} (threshold lines>{}, bytes>{})",
                line_count, byte_count, LARGE_FILE_LINE_THRESHOLD, LARGE_FILE_BYTE_THRESHOLD
            );
            if self.syntax_reparse_enabled {
                println!(
                    "[Syntax] large-file fallback ON: {}. disable continuous async reparse.",
                    reason
                );
                self.view_model.clear_syntax_highlights();
                self.view_model.mark_layout_dirty();
            }
            self.syntax_reparse_enabled = false;
            self.large_file_reason = Some(reason);
            return;
        }

        if !self.syntax_reparse_enabled {
            println!(
                "[Syntax] large-file fallback OFF: lines={} bytes={} -> async reparse resumed.",
                line_count, byte_count
            );
        }
        self.syntax_reparse_enabled = true;
        self.large_file_reason = None;
    }

    fn build_render_text_snapshot(&self) -> String {
        if self.syntax_reparse_enabled {
            return self.app_state.text_string();
        }

        let reason = self
            .large_file_reason
            .as_deref()
            .unwrap_or("fallback reason unavailable");

        let mut text = String::new();
        text.push_str("[Large File Mode]\n");
        text.push_str("Continuous syntax reparse is disabled to keep input/render responsive.\n");
        text.push_str(&format!("Reason: {reason}\n\n"));
        text.push_str("Preview (prefix only):\n");
        text.push_str("----------------------------------------\n");
        text.push_str(&self.app_state.prefix_text(LARGE_FILE_RENDER_PREVIEW_CHARS));
        text.push_str("\n\n----------------------------------------\n");
        text.push_str("Preview is truncated. Full buffer is still editable and savable.\n");
        text
    }

    fn build_hud_text(&self) -> String {
        let (line, col) = self.app_state.cursor_line_col();
        let file_label = self
            .app_state
            .active_file()
            .map(|path| path.display().to_string())
            .unwrap_or_else(|| "<none>".to_string());
        let syntax_label = if self.syntax_reparse_enabled {
            "on"
        } else {
            "large-file-fallback"
        };
        let mut hud = format!(
            "mode={}  ln={} col={}  rev={}  dirty={}\nchars={} lines={} bytes={}  syntax={}  {}\nfile={}",
            self.app_state.current_mode().as_str(),
            line + 1,
            col + 1,
            self.app_state.revision(),
            self.app_state.is_dirty(),
            self.app_state.len_chars(),
            self.app_state.len_lines(),
            self.app_state.len_bytes(),
            syntax_label,
            self.async_state.status_line(),
            file_label,
        );

        if self.app_state.is_file_picker_open() {
            hud.push_str(&format!(
                "\n[picker] OPEN query={:?} selected={} results={} workspace_files={}",
                self.app_state.file_picker_query_text(),
                self.app_state.file_picker_selected_index(),
                self.app_state.file_picker_results().len(),
                self.app_state.workspace_file_count(),
            ));

            for (idx, entry) in self
                .app_state
                .file_picker_results()
                .iter()
                .take(8)
                .enumerate()
            {
                let marker = if idx == self.app_state.file_picker_selected_index() {
                    ">"
                } else {
                    " "
                };
                hud.push_str(&format!("\n{} {}", marker, entry.relative_path));
            }
        } else {
            hud.push_str(&format!(
                "\n[picker] closed (workspace_files={})  shortcut: Cmd/Ctrl+P or <Space> f",
                self.app_state.workspace_file_count()
            ));
        }

        if let Some(conflict) = self.app_state.external_conflict_message() {
            hud.push_str(&format!("\n[watch] conflict: {}", conflict));
        } else if let Some(notice) = self.app_state.last_external_notice() {
            hud.push_str(&format!("\n[watch] {}", notice));
        } else {
            hud.push_str("\n[watch] idle");
        }

        if self.app_state.is_terminal_panel_open() {
            hud.push_str(&format!(
                "\n[terminal] OPEN focus={} {} rows={} cols={} notice={}",
                if self.app_state.current_mode() == EditorMode::TerminalFocus {
                    "terminal"
                } else {
                    "editor"
                },
                self.terminal_state.session_label(),
                self.terminal_state.rows,
                self.terminal_state.cols,
                self.terminal_state.last_notice
            ));
        } else {
            hud.push_str("\n[terminal] closed  shortcut: <Space> t or Cmd/Ctrl+\\");
        }

        hud
    }

    fn build_terminal_panel_text(&self) -> String {
        let mut text = String::new();
        text.push_str(&format!(
            "[terminal panel] {} focus={}\n",
            self.terminal_state.session_label(),
            if self.app_state.current_mode() == EditorMode::TerminalFocus {
                "terminal"
            } else {
                "editor"
            }
        ));
        text.push_str("------------------------------------------------------------\n");

        let max_rows = self
            .renderer
            .as_ref()
            .map(|renderer| renderer.terminal_max_visible_rows().saturating_sub(2))
            .unwrap_or(10)
            .max(1);
        text.push_str(&self.terminal_state.render_snapshot(max_rows));
        text
    }

    fn submit_async_reparse(&mut self, trigger: &str) {
        if !self.syntax_reparse_enabled {
            let reason = self
                .large_file_reason
                .as_deref()
                .unwrap_or("fallback reason unavailable");
            println!(
                "[Syntax] skip async reparse trigger={} because large-file fallback is active ({})",
                trigger, reason
            );
            return;
        }

        let revision_id = self
            .async_state
            .bump_revision(RequestTopic::ActiveBufferLayout);
        let text_snapshot = self.app_state.text_string();
        let file_path = self.app_state.active_file().map(PathBuf::from);
        let line_count = self.app_state.len_lines();
        let byte_count = self.app_state.len_bytes();

        println!(
            "[Syntax] submit async reparse trigger={} revision={} lines={} bytes={}",
            trigger, revision_id, line_count, byte_count
        );

        self.submit_worker_request(
            RequestTopic::ActiveBufferLayout,
            revision_id,
            WorkerRequestPayload::ParseAndHighlight {
                file_path,
                text_snapshot,
                language_id: LanguageId::Rust,
            },
        );
    }

    fn on_buffer_text_changed(&mut self, trigger: &str) {
        self.evaluate_large_file_policy();
        self.submit_async_reparse(trigger);
    }

    fn submit_workspace_watch(&mut self) {
        let Some(root_path) = self.app_state.workspace_root_path().map(PathBuf::from) else {
            return;
        };

        let revision_id = self.async_state.bump_revision(RequestTopic::WorkspaceWatch);
        println!(
            "[Watch] submit workspace watch root={} revision={}",
            root_path.display(),
            revision_id
        );

        self.submit_worker_request(
            RequestTopic::WorkspaceWatch,
            revision_id,
            WorkerRequestPayload::StartFileWatch { root_path },
        );
    }

    fn ensure_terminal_session(&mut self) {
        if self.terminal_state.session_id.is_some() || self.terminal_state.spawn_pending {
            return;
        }

        if self.terminal_state.revision_id == 0 {
            let current = self.async_state.current_revision(RequestTopic::TerminalPty);
            self.terminal_state.revision_id = if current == 0 {
                self.async_state.bump_revision(RequestTopic::TerminalPty)
            } else {
                current
            };
        }

        self.terminal_state.spawn_pending = true;
        self.terminal_state.last_notice = "spawning PTY shell".to_string();
        self.submit_worker_request(
            RequestTopic::TerminalPty,
            self.terminal_state.revision_id,
            WorkerRequestPayload::SpawnPtyShell {
                shell: None,
                working_dir: self.app_state.workspace_root_path().map(PathBuf::from),
            },
        );
    }

    fn submit_terminal_input(&mut self, input: String) -> bool {
        if input.is_empty() {
            return false;
        }

        let Some(session_id) = self.terminal_state.session_id else {
            self.ensure_terminal_session();
            return false;
        };

        let revision_id = self.terminal_state.revision_id.max(1);
        self.submit_worker_request(
            RequestTopic::TerminalPty,
            revision_id,
            WorkerRequestPayload::WritePtyInput { session_id, input },
        );
        true
    }

    fn request_close_terminal_session(&mut self) {
        let Some(session_id) = self.terminal_state.session_id else {
            self.terminal_state.spawn_pending = false;
            return;
        };

        let revision_id = self.terminal_state.revision_id.max(1);
        self.submit_worker_request(
            RequestTopic::TerminalPty,
            revision_id,
            WorkerRequestPayload::ClosePtySession { session_id },
        );
        self.terminal_state.last_notice = "terminal close requested".to_string();
    }

    fn apply_pending_terminal_results(&mut self) -> bool {
        let results = self.async_state.take_pending_pty_results();
        if results.is_empty() {
            return false;
        }

        let mut changed = false;
        for result in results {
            match result {
                PendingPtyResult::Spawned {
                    session_id,
                    shell,
                    working_dir,
                } => {
                    self.terminal_state.session_id = Some(session_id);
                    self.terminal_state.spawn_pending = false;
                    self.terminal_state.last_notice =
                        format!("spawned {} in {}", shell, working_dir.display());
                    if !self.app_state.is_terminal_panel_open() {
                        self.request_close_terminal_session();
                    }
                    changed = true;
                }
                PendingPtyResult::Output { session_id, chunk } => {
                    if Some(session_id) != self.terminal_state.session_id {
                        continue;
                    }
                    if !self.app_state.is_terminal_panel_open() {
                        continue;
                    }
                    self.terminal_state.grid.feed_chunk(&chunk);
                    changed = true;
                }
                PendingPtyResult::InputWritten { session_id, bytes } => {
                    if Some(session_id) != self.terminal_state.session_id {
                        continue;
                    }
                    self.terminal_state.last_notice =
                        format!("stdin write accepted ({} bytes)", bytes);
                }
                PendingPtyResult::Closed {
                    session_id,
                    exit_status,
                    reason,
                } => {
                    if Some(session_id) == self.terminal_state.session_id {
                        self.terminal_state.session_id = None;
                        self.terminal_state.spawn_pending = false;
                        self.terminal_state.last_notice =
                            format!("session closed exit={:?} reason={}", exit_status, reason);
                        changed = true;
                    }
                }
            }
        }

        changed
    }

    fn apply_pending_syntax_result(&mut self) -> bool {
        let Some(result) = self.async_state.take_pending_syntax_result() else {
            return false;
        };

        if !self.syntax_reparse_enabled {
            println!(
                "[Syntax] drop accepted syntax result rev={} because fallback is active.",
                result.revision_id
            );
            return false;
        }

        let total_time_ms = result.parse_time_ms + result.highlight_time_ms;
        let target = result
            .file_path
            .as_ref()
            .map(|path| path.display().to_string())
            .unwrap_or_else(|| "<active-buffer>".to_string());

        self.view_model
            .apply_syntax_highlights(result.revision_id, &result.spans);
        println!(
            "[Syntax] apply highlights rev={} lang={} spans={} file={} lines={} chars={} bytes={} parse_ms={} highlight_ms={} total_ms={}",
            result.revision_id,
            result.language_id.as_str(),
            result.spans.len(),
            target,
            result.line_count,
            result.char_count,
            result.byte_count,
            result.parse_time_ms,
            result.highlight_time_ms,
            total_time_ms
        );
        true
    }

    fn apply_pending_file_watch_events(&mut self) -> bool {
        let events = self.async_state.take_pending_file_events();
        if events.is_empty() {
            return false;
        }

        match self.app_state.apply_external_file_events(&events) {
            Ok(report) => {
                for notice in &report.notices {
                    if report.conflict_detected {
                        eprintln!("[Watch][Conflict] {notice}");
                    } else {
                        println!("[Watch] {notice}");
                    }
                }

                if report.active_file_reloaded {
                    self.view_model.clear_syntax_highlights();
                    self.on_buffer_text_changed("external-watch-reload");
                }

                if report.workspace_reloaded
                    || report.active_file_reloaded
                    || report.conflict_detected
                {
                    self.view_model.mark_layout_dirty();
                    return true;
                }
                false
            }
            Err(err) => {
                eprintln!("[Watch] apply external events failed: {err}");
                false
            }
        }
    }

    fn submit_cpu_burn_job(&mut self) {
        let revision_id = self.async_state.bump_revision(RequestTopic::BackgroundDemo);
        self.submit_worker_request(
            RequestTopic::BackgroundDemo,
            revision_id,
            WorkerRequestPayload::MockCpuBurn {
                job_label: "phase6-hotkey-cpu".to_string(),
                busy_millis: DEBUG_CPU_BURN_MS,
            },
        );
    }

    fn submit_forced_fail_search(&mut self) {
        let revision_id = self.async_state.bump_revision(RequestTopic::ProjectSearch);
        self.submit_worker_request(
            RequestTopic::ProjectSearch,
            revision_id,
            WorkerRequestPayload::MockSearch {
                query: "fail".to_string(),
                simulated_delay_ms: DEBUG_SEARCH_DELAY_MS,
            },
        );
    }

    fn submit_forced_panic_job(&mut self) {
        let revision_id = self.async_state.bump_revision(RequestTopic::BackgroundDemo);
        self.submit_worker_request(
            RequestTopic::BackgroundDemo,
            revision_id,
            WorkerRequestPayload::MockPanic {
                reason: "phase5 forced panic via hotkey".to_string(),
            },
        );
    }

    fn handle_debug_hotkeys(&mut self, key_event: &KeyEvent) -> bool {
        if key_event.state != ElementState::Pressed || key_event.repeat {
            return false;
        }

        let PhysicalKey::Code(key_code) = key_event.physical_key else {
            return false;
        };

        match key_code {
            KeyCode::F5 => {
                println!("Debug hotkey: F5 -> force async syntax reparse");
                self.submit_async_reparse("manual-f5");
                true
            }
            KeyCode::F6 => {
                println!("Debug hotkey: F6 -> submit CPU burn job");
                self.submit_cpu_burn_job();
                true
            }
            KeyCode::F7 => {
                println!("Debug hotkey: F7 -> submit forced-fail search job");
                self.submit_forced_fail_search();
                true
            }
            KeyCode::F8 => {
                println!("Debug hotkey: F8 -> submit forced panic job");
                self.submit_forced_panic_job();
                true
            }
            _ => false,
        }
    }

    fn handle_terminal_keyboard_input(&mut self, key_event: &KeyEvent) -> bool {
        if self.app_state.current_mode() != EditorMode::TerminalFocus {
            return false;
        }
        if key_event.state != ElementState::Pressed {
            return false;
        }

        let modifiers = self.input_handler.current_modifiers();
        if modifiers.super_key() {
            // Giữ lại phím hệ thống (Cmd trên macOS) không đẩy vào shell.
            return false;
        }

        if let Some(input) = terminal_input_from_key_event(key_event, modifiers) {
            let preview = input.escape_default().to_string();
            if self.submit_terminal_input(input) {
                println!("[Terminal] input forwarded -> \"{preview}\"");
                return true;
            }
        }
        false
    }

    fn handle_terminal_ime_commit(&mut self, text: &str) -> bool {
        if self.app_state.current_mode() != EditorMode::TerminalFocus {
            return false;
        }
        if text.is_empty() || text.chars().any(char::is_control) {
            return false;
        }
        if self.submit_terminal_input(text.to_string()) {
            println!("[Terminal] IME commit forwarded -> {:?}", text);
            return true;
        }
        false
    }

    fn pump_async_bridge(&mut self) {
        let Some(bridge) = self.async_bridge.as_mut() else {
            return;
        };

        let stats = bridge.pump(&mut self.async_state);
        let applied_syntax = self.apply_pending_syntax_result();
        let applied_watch_events = self.apply_pending_file_watch_events();
        let applied_terminal_events = self.apply_pending_terminal_results();
        if applied_syntax || applied_watch_events || applied_terminal_events {
            self.sync_and_schedule_redraw(applied_terminal_events);
        }

        if stats.routed > 0 {
            println!(
                "[Bridge Summary] accepted={} stale={} failed={} started={} completed={} cancelled={}",
                stats.accepted,
                stats.stale,
                stats.failed,
                stats.started,
                stats.completed,
                stats.cancelled
            );
            self.refresh_window_title();
            if let Some(window) = &self.window {
                window.request_redraw();
            }
        }
    }
}

impl ApplicationHandler for Phase5App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }

        let window_attributes = Window::default_attributes()
            .with_title(WINDOW_TITLE)
            .with_inner_size(PhysicalSize::new(WINDOW_WIDTH, WINDOW_HEIGHT));

        let window = match event_loop.create_window(window_attributes) {
            Ok(window) => Arc::new(window),
            Err(err) => {
                eprintln!("create_window failed: {err}");
                event_loop.exit();
                return;
            }
        };

        let renderer = match pollster::block_on(Phase5Renderer::new(
            window.clone(),
            self.app_state.is_terminal_panel_open(),
        )) {
            Ok(renderer) => renderer,
            Err(err) => {
                eprintln!("phase5 renderer init failed: {err}");
                event_loop.exit();
                return;
            }
        };

        self.window = Some(window);
        self.renderer = Some(renderer);
        let mut workspace_attached = false;
        if let Ok(cwd) = std::env::current_dir() {
            if let Err(err) = self.app_state.attach_workspace(cwd.clone()) {
                eprintln!("workspace attach failed for {}: {err}", cwd.display());
            } else {
                workspace_attached = true;
                println!(
                    "Workspace attached: {} files={}",
                    cwd.display(),
                    self.app_state.workspace_file_count()
                );
            }
        }
        if workspace_attached {
            self.submit_workspace_watch();
        }
        self.evaluate_large_file_policy();
        self.sync_and_schedule_redraw(true);
        self.submit_async_reparse("startup");

        println!("Phase 5 ready: Buffer -> Layout -> Caret -> Render sync.");
        println!("Controls:");
        println!("  - Type text / Backspace / Arrow keys");
        println!("  - Cmd/Ctrl+S: Save file");
        println!(
            "  - Cmd/Ctrl+O: Open sample -> {}",
            self.input_map.open_file_path().display()
        );
        println!("  - Cmd/Ctrl+P or <Space> f: Open file picker overlay");
        println!("  - Picker keys: type query, Up/Down, Enter, Esc");
        println!("  - Terminal panel: <Space> t, `, or Cmd/Ctrl+\\");
        println!("  - Terminal focus: type to send PTY stdin, Esc to return editor focus");
        println!("  - File watcher: external create/delete/modify/rename events are tracked");
        println!("  - F5: force async syntax reparse");
        println!("  - F6: submit MockCpuBurn background job");
        println!("  - F7: submit forced-fail MockSearch");
        println!("  - F8: submit forced panic (panic-safe error path)");
        println!(
            "  - Large-file fallback thresholds: lines>{} OR bytes>{}",
            LARGE_FILE_LINE_THRESHOLD, LARGE_FILE_BYTE_THRESHOLD
        );
        println!("State: {}", self.app_state.debug_state_line());
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        window_id: WindowId,
        event: WindowEvent,
    ) {
        if self
            .window
            .as_ref()
            .is_some_and(|window| window.id() != window_id)
        {
            return;
        }

        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::ModifiersChanged(modifiers) => {
                self.input_handler.update_modifiers(modifiers);
            }
            WindowEvent::Focused(focused) => {
                self.input_handler.on_focus_changed(focused);
            }
            WindowEvent::Resized(new_size) => {
                if let Some(renderer) = self.renderer.as_mut() {
                    renderer.resize(new_size, self.app_state.is_terminal_panel_open());
                    self.view_model.mark_layout_dirty();
                    self.sync_and_schedule_redraw(true);
                }
            }
            WindowEvent::KeyboardInput {
                event: key_event, ..
            } => {
                if self.handle_debug_hotkeys(&key_event) {
                    return;
                }

                // Event loop giữ mỏng: chỉ translate input -> command, rồi forward.
                if let Some(route) = self.input_handler.translate_key_event(
                    &key_event,
                    &self.input_map,
                    KeybindingContext::for_mode_with_picker(
                        self.app_state.current_mode(),
                        self.app_state.is_file_picker_open(),
                    ),
                ) {
                    match route {
                        InputRouteOutcome::Dispatch(translated) => {
                            println!("Input: {}", translated.input_debug);
                            println!("Route: {}", translated.route_debug);
                            println!("Command: {:?}", translated.command);

                            let command = translated.command.clone();
                            let terminal_was_open = self.app_state.is_terminal_panel_open();
                            let report = self
                                .view_model
                                .apply_command(&mut self.app_state, translated.command);
                            println!("{}", report.message);
                            println!("State: {}", self.app_state.debug_state_line());
                            println!("Dirty: {}", self.view_model.dirty_flags().summary());
                            if report.success
                                && report.state_changed
                                && self.view_model.dirty_flags().text_dirty
                            {
                                self.on_buffer_text_changed("keyboard");
                            }

                            if report.success && report.state_changed {
                                if self.app_state.is_terminal_panel_open() {
                                    self.ensure_terminal_session();
                                } else if terminal_was_open {
                                    self.request_close_terminal_session();
                                }
                                if matches!(
                                    command,
                                    netherize_editor::core::commands::Command::ToggleTerminal
                                ) {
                                    self.view_model.mark_layout_dirty();
                                }
                            }

                            self.sync_and_schedule_redraw(false);
                        }
                        InputRouteOutcome::NoDispatch {
                            input_debug,
                            route_debug,
                        } => {
                            println!("Input: {}", input_debug);
                            println!("Route: {}", route_debug);
                        }
                    }
                } else if self.handle_terminal_keyboard_input(&key_event) {
                    self.sync_and_schedule_redraw(true);
                }
            }
            WindowEvent::Ime(Ime::Commit(text)) => {
                if self.handle_terminal_ime_commit(&text) {
                    self.sync_and_schedule_redraw(true);
                    return;
                }

                if let Some(translated) = self.input_handler.translate_ime_commit(
                    &text,
                    KeybindingContext::for_mode_with_picker(
                        self.app_state.current_mode(),
                        self.app_state.is_file_picker_open(),
                    ),
                ) {
                    println!("Input: {}", translated.input_debug);
                    println!("Route: {}", translated.route_debug);
                    println!("Command: {:?}", translated.command);

                    let report = self
                        .view_model
                        .apply_command(&mut self.app_state, translated.command);
                    println!("{}", report.message);
                    println!("State: {}", self.app_state.debug_state_line());
                    println!("Dirty: {}", self.view_model.dirty_flags().summary());
                    if report.success
                        && report.state_changed
                        && self.view_model.dirty_flags().text_dirty
                    {
                        self.on_buffer_text_changed("ime");
                    }

                    self.sync_and_schedule_redraw(false);
                }
            }
            WindowEvent::RedrawRequested => {
                if let Some(renderer) = self.renderer.as_mut() {
                    if let Err(err) = renderer.render() {
                        eprintln!("render failed: {err}");
                    }
                }

                self.frame_counter += 1;
                if self.frame_started_at.elapsed() >= Duration::from_secs(1) {
                    println!(
                        "[UI] frame heartbeat fps={} {}",
                        self.frame_counter,
                        self.async_state.status_line()
                    );
                    self.frame_counter = 0;
                    self.frame_started_at = Instant::now();
                }
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        // Pump bridge ở một nơi duy nhất để route result về app state.
        // Dùng WaitUntil để giảm CPU idle nhưng vẫn poll async bridge theo nhịp.
        self.pump_async_bridge();
        event_loop.set_control_flow(ControlFlow::WaitUntil(
            Instant::now() + Duration::from_millis(IDLE_PUMP_INTERVAL_MS),
        ));
    }
}

fn terminal_grid_dimensions(rect: PanelRect) -> (usize, usize) {
    if rect.width <= 0.0 || rect.height <= 0.0 {
        return (TERMINAL_GRID_COLS, TERMINAL_GRID_ROWS);
    }

    let cols = (rect.width / (TERMINAL_FONT_SIZE * 0.58)).floor() as usize;
    let rows = (rect.height / TERMINAL_LINE_HEIGHT).floor() as usize;
    (cols.max(20), rows.max(4))
}

fn terminal_input_from_key_event(
    key_event: &KeyEvent,
    modifiers: winit::keyboard::ModifiersState,
) -> Option<String> {
    if modifiers.control_key()
        && let PhysicalKey::Code(code) = key_event.physical_key
    {
        if let Some(ctrl) = control_char_for_keycode(code) {
            return Some(ctrl.to_string());
        }
    }

    if let Key::Character(text) = key_event.logical_key.as_ref()
        && !text.is_empty()
        && !text.chars().any(char::is_control)
    {
        return Some(text.to_string());
    }

    let named = match key_event.logical_key.as_ref() {
        Key::Named(named) => Some(named),
        _ => None,
    }?;

    match named {
        NamedKey::Space => Some(" ".to_string()),
        NamedKey::Enter => Some("\r".to_string()),
        NamedKey::Tab => Some("\t".to_string()),
        NamedKey::Backspace => Some("\u{7f}".to_string()),
        NamedKey::ArrowUp => Some("\u{1b}[A".to_string()),
        NamedKey::ArrowDown => Some("\u{1b}[B".to_string()),
        NamedKey::ArrowRight => Some("\u{1b}[C".to_string()),
        NamedKey::ArrowLeft => Some("\u{1b}[D".to_string()),
        NamedKey::Home => Some("\u{1b}[H".to_string()),
        NamedKey::End => Some("\u{1b}[F".to_string()),
        NamedKey::Delete => Some("\u{1b}[3~".to_string()),
        _ => None,
    }
}

fn control_char_for_keycode(key_code: KeyCode) -> Option<char> {
    let value: u8 = match key_code {
        KeyCode::KeyA => 0x01,
        KeyCode::KeyB => 0x02,
        KeyCode::KeyC => 0x03,
        KeyCode::KeyD => 0x04,
        KeyCode::KeyE => 0x05,
        KeyCode::KeyF => 0x06,
        KeyCode::KeyG => 0x07,
        KeyCode::KeyH => 0x08,
        KeyCode::KeyI => 0x09,
        KeyCode::KeyJ => 0x0A,
        KeyCode::KeyK => 0x0B,
        KeyCode::KeyL => 0x0C,
        KeyCode::KeyM => 0x0D,
        KeyCode::KeyN => 0x0E,
        KeyCode::KeyO => 0x0F,
        KeyCode::KeyP => 0x10,
        KeyCode::KeyQ => 0x11,
        KeyCode::KeyR => 0x12,
        KeyCode::KeyS => 0x13,
        KeyCode::KeyT => 0x14,
        KeyCode::KeyU => 0x15,
        KeyCode::KeyV => 0x16,
        KeyCode::KeyW => 0x17,
        KeyCode::KeyX => 0x18,
        KeyCode::KeyY => 0x19,
        KeyCode::KeyZ => 0x1A,
        _ => return None,
    };
    Some(value as char)
}

fn main() {
    let event_loop = EventLoop::new().expect("failed to create event loop");
    // Mặc định chờ event. `about_to_wait` sẽ set WaitUntil để có nhịp poll async nhẹ.
    event_loop.set_control_flow(ControlFlow::Wait);

    let mut app = Phase5App::new();
    if let Err(err) = event_loop.run_app(&mut app) {
        eprintln!("phase5_buffer_sync_probe exited with error: {err}");
    }
}
