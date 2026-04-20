use std::sync::Arc;

use netherize_editor::{
    render::{
        region_pipeline::{RegionDrawInstance, RegionPipeline},
        surface::SurfaceState,
    },
    workbench::{
        command_router::{RoutedWorkbenchCommand, WorkbenchCommand, WorkbenchCommandRouter},
        debug_state::DebugSharedState,
        focus_manager::{FocusManager, FocusTarget},
        inspector_panel::{InspectorPanelState, InspectorSectionKind},
        layout_engine::{
            SplitHandleId, WorkbenchLayout, WorkbenchLayoutConfig, WorkbenchLayoutEngine,
        },
        overlay_manager::OverlayManager,
        panel_state::{PanelTabId, WorkbenchPanelState},
        region_model::{RegionBounds, RegionId, RegionModel},
        text_coordinate_map::TextCoordinateMapper,
    },
};
use winit::{
    application::ApplicationHandler,
    dpi::{PhysicalPosition, PhysicalSize},
    event::{ElementState, MouseButton, WindowEvent},
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop},
    keyboard::ModifiersState,
    window::{Window, WindowId},
};

const WINDOW_WIDTH: u32 = 1280;
const WINDOW_HEIGHT: u32 = 800;
const WINDOW_TITLE: &str = "Netherize Phase11 - Debug Overlay + Inspector Probe";

const CLEAR_COLOR: wgpu::Color = wgpu::Color {
    r: 0.06,
    g: 0.07,
    b: 0.10,
    a: 1.0,
};

const EDITOR_SAMPLE_SNIPPET: &str = r#"fn process_event(user: &User, max: i32) -> Result<()> {
    let mut counter = 40;
    let active = user.is_active();
    if active {
        counter += 2;
    }
    if counter > max {
        return Err(Error::Overflow);
    }
    Ok(())
}"#;

fn build_editor_sample_text() -> String {
    let mut out = String::new();
    for idx in 0..24 {
        out.push_str(&format!("// --- sample block {idx:02} ---\n"));
        out.push_str(EDITOR_SAMPLE_SNIPPET);
        out.push('\n');
    }
    out
}

#[derive(Debug, Clone, Copy)]
struct DragState {
    handle_id: SplitHandleId,
    last_cursor: [f32; 2],
}

struct WorkbenchRenderer {
    device: wgpu::Device,
    queue: wgpu::Queue,
    surface_state: SurfaceState,
    region_pipeline: RegionPipeline,
    layout_engine: WorkbenchLayoutEngine,
    panel_state: WorkbenchPanelState,
    focus_manager: FocusManager,
    command_router: WorkbenchCommandRouter,
    layout: WorkbenchLayout,
    drag_state: Option<DragState>,
    last_cursor: [f32; 2],
    modifiers: ModifiersState,
    debug_state: DebugSharedState,
    inspector_panel: InspectorPanelState,
    overlay_manager: OverlayManager,
    text_mapper: TextCoordinateMapper,
    editor_sample_text: String,
    editor_scroll_lines: usize,
    focus_before_command_palette: Option<FocusTarget>,
}

impl WorkbenchRenderer {
    async fn new(window: Arc<Window>) -> Result<Self, String> {
        let window_size = window.inner_size();
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
                label: Some("Netherize Phase11.3 Device"),
                required_features: wgpu::Features::empty(),
                required_limits: wgpu::Limits::default(),
                experimental_features: Default::default(),
                memory_hints: Default::default(),
                trace: Default::default(),
            })
            .await
            .map_err(|err| format!("request_device failed: {err}"))?;

        let surface_state = SurfaceState::new(surface, window_size, &adapter, &device)?;
        let region_pipeline = RegionPipeline::new(
            &device,
            surface_state.config.format,
            surface_state.size.width,
            surface_state.size.height,
        );

        let layout_engine = WorkbenchLayoutEngine::new(WorkbenchLayoutConfig::default());
        let panel_state = WorkbenchPanelState::default();
        let focus_manager = FocusManager::default();
        let layout = layout_engine.compute(surface_state.size, &panel_state);
        let debug_state = DebugSharedState::default();
        let mut inspector_panel = InspectorPanelState::default();
        inspector_panel.sync_from_debug_state(&debug_state);
        let editor_sample_text = build_editor_sample_text();

        let editor_bounds = layout
            .model
            .find(RegionId::Center)
            .unwrap_or(RegionBounds::new(0.0, 0.0, 1.0, 1.0));
        let mut text_mapper = TextCoordinateMapper::from_text(
            &editor_sample_text,
            editor_bounds,
            20.0,
            14.0,
            20.0,
            9.0,
        );
        text_mapper.set_vertical_scroll_lines(0);

        let mut renderer = Self {
            device,
            queue,
            surface_state,
            region_pipeline,
            layout_engine,
            panel_state,
            focus_manager,
            command_router: WorkbenchCommandRouter,
            layout,
            drag_state: None,
            last_cursor: [0.0, 0.0],
            modifiers: ModifiersState::empty(),
            debug_state,
            inspector_panel,
            overlay_manager: OverlayManager::default(),
            text_mapper,
            editor_sample_text,
            editor_scroll_lines: 0,
            focus_before_command_palette: None,
        };
        renderer.rebuild_layout_instances();
        Ok(renderer)
    }

    fn resize(&mut self, new_size: PhysicalSize<u32>) {
        if new_size.width == 0 || new_size.height == 0 {
            return;
        }
        self.surface_state.resize(&self.device, new_size);
        self.region_pipeline
            .update_screen_size(&self.queue, new_size.width, new_size.height);
        self.rebuild_layout_instances();
    }

    fn reconfigure_surface(&self) {
        self.surface_state.reconfigure(&self.device);
    }

    fn rebuild_layout_instances(&mut self) {
        self.layout = self
            .layout_engine
            .compute(self.surface_state.size, &self.panel_state);
        self.focus_manager.ensure_valid(&self.panel_state);
        self.inspector_panel
            .sync_from_debug_state(&self.debug_state);

        let window_bounds = self
            .layout
            .model
            .find(RegionId::Root)
            .unwrap_or(RegionBounds::new(
                0.0,
                0.0,
                self.surface_state.size.width as f32,
                self.surface_state.size.height as f32,
            ));
        let editor_bounds = self
            .layout
            .model
            .find(RegionId::Center)
            .unwrap_or(RegionBounds::new(0.0, 0.0, 1.0, 1.0));
        self.text_mapper = TextCoordinateMapper::from_text(
            &self.editor_sample_text,
            editor_bounds,
            20.0,
            14.0,
            20.0,
            9.0,
        );
        self.text_mapper
            .set_vertical_scroll_lines(self.editor_scroll_lines);
        self.editor_scroll_lines = self.text_mapper.vertical_scroll_lines();

        let mut draw_instances = Vec::new();
        for region in self.layout.model.flatten() {
            if region.id == RegionId::Root || !region.visible {
                continue;
            }
            let bounds = region.bounds;
            let mut color = region_color(region.id, &self.panel_state, &self.debug_state);
            if focus_matches_region(self.focus_manager.current(), region.id) {
                color = brighten(color);
            }
            draw_instances.push(RegionDrawInstance::new(
                [bounds.x, bounds.y, bounds.width, bounds.height],
                color,
            ));
        }

        // Shared debug state -> gutter markers trong editor region.
        for breakpoint in &self.debug_state.breakpoints {
            if let Some(marker_rect) = self
                .text_mapper
                .gutter_marker_rect(breakpoint.location.line)
            {
                draw_instances.push(RegionDrawInstance::new(
                    [
                        marker_rect.x,
                        marker_rect.y,
                        marker_rect.width,
                        marker_rect.height,
                    ],
                    if breakpoint.enabled {
                        [0.86, 0.24, 0.26, 0.96]
                    } else {
                        [0.46, 0.18, 0.18, 0.80]
                    },
                ));
            }
        }

        // Inspector section surfaces trong right sidebar.
        if self.panel_state.right.visible
            && self.panel_state.right.active_tab_id() == Some(PanelTabId::Inspector)
            && let Some(right_bounds) = self.layout.model.find(RegionId::RightSidebar)
        {
            let selected_section = self
                .inspector_panel
                .visible_rows()
                .get(self.inspector_panel.selected_row)
                .and_then(|row| self.inspector_panel.sections.get(row.section_index))
                .map(|section| section.kind);

            for surface in self
                .inspector_panel
                .section_surfaces(right_bounds, selected_section)
            {
                let color = inspector_section_color(surface.kind, surface.selected);
                draw_instances.push(RegionDrawInstance::new(
                    [
                        surface.bounds.x + 8.0,
                        surface.bounds.y + 6.0,
                        (surface.bounds.width - 16.0).max(1.0),
                        (surface.bounds.height - 12.0).max(1.0),
                    ],
                    color,
                ));
            }
        }

        for handle in &self.layout.handles {
            draw_instances.push(RegionDrawInstance::new(
                [
                    handle.bounds.x,
                    handle.bounds.y,
                    handle.bounds.width,
                    handle.bounds.height,
                ],
                [0.90, 0.92, 0.98, 0.22],
            ));
        }

        let overlays = self.overlay_manager.build_overlays(
            window_bounds,
            editor_bounds,
            &self.text_mapper,
            &self.debug_state,
        );
        for overlay in &overlays {
            draw_instances.push(RegionDrawInstance::new(
                [
                    overlay.rect.x,
                    overlay.rect.y,
                    overlay.rect.width,
                    overlay.rect.height,
                ],
                overlay.color,
            ));
        }

        self.region_pipeline
            .upload_instances(&self.device, &self.queue, &draw_instances);
        log_layout(
            &self.layout.model,
            &self.panel_state,
            self.focus_manager.current(),
            &self.inspector_panel,
            overlays.len(),
            self.editor_scroll_lines,
            self.text_mapper.max_scroll_lines(),
            self.overlay_manager.command_palette_query(),
        );
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
                label: Some("Netherize Phase11.3 Encoder"),
            });

        {
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Netherize Phase11.3 RenderPass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(CLEAR_COLOR),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });

            self.region_pipeline.draw(&mut render_pass);
        }

        self.queue.submit(std::iter::once(encoder.finish()));
        frame.present();
        Ok(())
    }

    fn update_modifiers(&mut self, modifiers: ModifiersState) {
        self.modifiers = modifiers;
    }

    fn handle_key_input(&mut self, event: &winit::event::KeyEvent) -> bool {
        let Some(route) = self.command_router.resolve_key_event(
            event,
            self.modifiers,
            self.focus_manager.current(),
            self.overlay_manager.is_command_palette_open(),
        ) else {
            return false;
        };

        println!(
            "[Router] focus={} -> {}",
            self.focus_manager.current().label(),
            route.reason
        );

        let changed = self.apply_command(route);
        if changed {
            self.rebuild_layout_instances();
        }
        changed
    }

    fn apply_command(&mut self, routed: RoutedWorkbenchCommand) -> bool {
        match routed.command {
            WorkbenchCommand::ToggleLeftSidebar => self.panel_state.toggle_left(),
            WorkbenchCommand::ToggleRightSidebar => self.panel_state.toggle_right(),
            WorkbenchCommand::ToggleBottomPanel => self.panel_state.toggle_bottom(),
            WorkbenchCommand::ToggleOverlay => self.panel_state.toggle_overlay(),
            WorkbenchCommand::ToggleCommandPaletteOverlay => self.toggle_command_palette(),
            WorkbenchCommand::CommandPaletteBackspace => {
                if self.overlay_manager.pop_command_palette_char() {
                    println!(
                        "[OverlayInput] palette_query={:?}",
                        self.overlay_manager.command_palette_query()
                    );
                    return true;
                }
                false
            }
            WorkbenchCommand::CommandPaletteConfirm => self.confirm_command_palette_query(),
            WorkbenchCommand::FocusNext => self.focus_manager.cycle_next(&self.panel_state),
            WorkbenchCommand::FocusPrev => self.focus_manager.cycle_prev(&self.panel_state),
            WorkbenchCommand::FocusCenterEditor => {
                self.focus_manager.set(FocusTarget::CenterEditor)
            }
            WorkbenchCommand::FocusBottomPanel => self.focus_manager.set(FocusTarget::BottomPanel),
            WorkbenchCommand::FocusLeftSidebar => self.focus_manager.set(FocusTarget::LeftSidebar),
            WorkbenchCommand::FocusRightSidebar => {
                self.focus_manager.set(FocusTarget::RightSidebar)
            }
            WorkbenchCommand::NextTabInFocusedPanel => self.switch_tab_in_focused_panel(true),
            WorkbenchCommand::PrevTabInFocusedPanel => self.switch_tab_in_focused_panel(false),
            WorkbenchCommand::ExplorerMoveDown
            | WorkbenchCommand::ExplorerMoveUp
            | WorkbenchCommand::ExplorerCollapseOrParent
            | WorkbenchCommand::ExplorerExpandOrChild
            | WorkbenchCommand::ExplorerToggleOrOpen => false,
            WorkbenchCommand::InspectorSelectNext => self.inspector_panel.move_selection_next(),
            WorkbenchCommand::InspectorSelectPrev => self.inspector_panel.move_selection_prev(),
            WorkbenchCommand::InspectorToggleExpand => {
                if self.focus_manager.current() != FocusTarget::RightSidebar {
                    return false;
                }
                self.inspector_panel.toggle_selected_expand()
            }
            WorkbenchCommand::TogglePausedState => {
                self.debug_state.toggle_paused();
                self.overlay_manager
                    .set_toast(Some(if self.debug_state.paused {
                        "debugger paused".to_string()
                    } else {
                        "debugger resumed".to_string()
                    }));
                true
            }
            WorkbenchCommand::MoveExecutionUp => self.debug_state.move_execution_up(),
            WorkbenchCommand::MoveExecutionDown => self
                .debug_state
                .move_execution_down(self.text_mapper.line_count().max(1)),
            WorkbenchCommand::ToggleBreakpointAtExecutionLine => {
                self.debug_state.toggle_breakpoint_on_execution_line()
            }
            WorkbenchCommand::RouteTextToFocused(text) => {
                if self.focus_manager.current() == FocusTarget::OverlayLayer
                    && self.overlay_manager.is_command_palette_open()
                    && self.overlay_manager.push_command_palette_text(&text)
                {
                    println!(
                        "[OverlayInput] palette_query={:?}",
                        self.overlay_manager.command_palette_query()
                    );
                    return true;
                }
                println!(
                    "[FocusRoute] {} <= {:?}",
                    self.focus_manager.current().label(),
                    text
                );
                false
            }
        }
    }

    fn switch_tab_in_focused_panel(&mut self, forward: bool) -> bool {
        match self.focus_manager.current() {
            FocusTarget::BottomPanel => {
                if forward {
                    self.panel_state.switch_bottom_next_tab()
                } else {
                    self.panel_state.switch_bottom_prev_tab()
                }
            }
            FocusTarget::LeftSidebar => {
                if forward {
                    self.panel_state.switch_left_next_tab()
                } else {
                    self.panel_state.switch_left_prev_tab()
                }
            }
            FocusTarget::RightSidebar => {
                if forward {
                    self.panel_state.switch_right_next_tab()
                } else {
                    self.panel_state.switch_right_prev_tab()
                }
            }
            _ => false,
        }
    }

    fn toggle_command_palette(&mut self) -> bool {
        let was_open = self.overlay_manager.is_command_palette_open();
        self.overlay_manager.toggle_command_palette();
        if was_open {
            self.panel_state.overlay_visible = false;
            let restore = self
                .focus_before_command_palette
                .take()
                .unwrap_or(FocusTarget::CenterEditor);
            self.focus_manager.set(restore);
            self.overlay_manager
                .set_toast(Some("command palette closed".to_string()));
            println!(
                "[Overlay] command palette closed -> focus={}",
                restore.label()
            );
        } else {
            self.focus_before_command_palette = Some(self.focus_manager.current());
            self.panel_state.overlay_visible = true;
            self.focus_manager.set(FocusTarget::OverlayLayer);
            self.overlay_manager
                .set_toast(Some("command palette opened".to_string()));
            println!(
                "[Overlay] command palette opened -> focus=overlay_layer return_to={}",
                self.focus_before_command_palette
                    .unwrap_or(FocusTarget::CenterEditor)
                    .label()
            );
        }
        true
    }

    fn confirm_command_palette_query(&mut self) -> bool {
        let query = self
            .overlay_manager
            .command_palette_query()
            .trim()
            .to_string();
        self.overlay_manager.set_toast(Some(if query.is_empty() {
            "palette confirm: <empty query>".to_string()
        } else {
            format!("palette confirm: {query}")
        }));
        self.toggle_command_palette()
    }

    fn toggle_breakpoint_from_gutter_click(&mut self, x: f32, y: f32) -> bool {
        if !self.text_mapper.hit_test_gutter(x, y) {
            return false;
        }
        let Some(line) = self.text_mapper.line_at_screen_y(y) else {
            return false;
        };
        if self.debug_state.toggle_breakpoint_at_line(line) {
            println!("[Debug] gutter toggle breakpoint line={}", line + 1);
            self.overlay_manager
                .set_toast(Some(format!("breakpoint toggled at line {}", line + 1)));
            self.focus_manager.set(FocusTarget::CenterEditor);
            return true;
        }
        false
    }

    fn scroll_editor_by_lines(&mut self, delta_lines: i32) -> bool {
        if delta_lines == 0 {
            return false;
        }
        let max = self.text_mapper.max_scroll_lines() as i32;
        let current = self.editor_scroll_lines as i32;
        let next = (current + delta_lines).clamp(0, max);
        if next == current {
            return false;
        }
        self.editor_scroll_lines = next as usize;
        println!(
            "[EditorScroll] lines={} / {}",
            self.editor_scroll_lines,
            self.text_mapper.max_scroll_lines()
        );
        true
    }

    fn on_mouse_wheel(&mut self, delta: winit::event::MouseScrollDelta) -> bool {
        let center_bounds = self.layout.model.find(RegionId::Center);
        let cursor_inside_editor = center_bounds.is_some_and(|bounds| {
            self.last_cursor[0] >= bounds.x
                && self.last_cursor[0] <= bounds.x + bounds.width
                && self.last_cursor[1] >= bounds.y
                && self.last_cursor[1] <= bounds.y + bounds.height
        });

        if self.focus_manager.current() != FocusTarget::CenterEditor && !cursor_inside_editor {
            return false;
        }

        let delta_lines = match delta {
            winit::event::MouseScrollDelta::LineDelta(_, y) => -(y.round() as i32),
            winit::event::MouseScrollDelta::PixelDelta(pos) => {
                if pos.y.abs() < 1.0 {
                    0
                } else {
                    -(pos.y / 24.0).round() as i32
                }
            }
        };

        self.scroll_editor_by_lines(delta_lines)
    }

    fn on_cursor_moved(&mut self, position: PhysicalPosition<f64>) -> bool {
        let cursor = [position.x as f32, position.y as f32];
        self.last_cursor = cursor;

        let Some(drag) = self.drag_state else {
            return false;
        };

        let delta_x = cursor[0] - drag.last_cursor[0];
        let delta_y = cursor[1] - drag.last_cursor[1];
        let changed = self.layout_engine.apply_handle_drag(
            self.surface_state.size,
            &mut self.panel_state,
            drag.handle_id,
            delta_x,
            delta_y,
        );
        self.drag_state = Some(DragState {
            handle_id: drag.handle_id,
            last_cursor: cursor,
        });
        if changed {
            self.rebuild_layout_instances();
        }
        changed
    }

    fn on_mouse_left_pressed(&mut self) -> bool {
        let cursor = self.last_cursor;
        if let Some(handle) = self
            .layout
            .handles
            .iter()
            .copied()
            .find(|handle| handle.contains(cursor[0], cursor[1]))
        {
            self.drag_state = Some(DragState {
                handle_id: handle.id,
                last_cursor: cursor,
            });
            println!("[Split] drag start handle={}", handle.id.label());
            return false;
        }

        if self.toggle_breakpoint_from_gutter_click(cursor[0], cursor[1]) {
            return true;
        }

        let changed = self.focus_manager.set_from_click(
            cursor[0],
            cursor[1],
            &self.layout.model,
            &self.panel_state,
        );
        if changed {
            self.rebuild_layout_instances();
        }
        changed
    }

    fn on_mouse_left_released(&mut self) -> bool {
        if let Some(drag) = self.drag_state.take() {
            println!("[Split] drag end handle={}", drag.handle_id.label());
        }
        false
    }
}

fn region_color(
    id: RegionId,
    panels: &WorkbenchPanelState,
    debug_state: &DebugSharedState,
) -> [f32; 4] {
    match id {
        RegionId::TopBar => [0.17, 0.20, 0.28, 0.95],
        RegionId::LeftSidebar => {
            match panels.left.active_tab_id().unwrap_or(PanelTabId::Explorer) {
                PanelTabId::Explorer => [0.12, 0.18, 0.26, 0.92],
                PanelTabId::Search => [0.14, 0.22, 0.28, 0.92],
                _ => [0.13, 0.18, 0.26, 0.92],
            }
        }
        RegionId::Center => [0.19, 0.24, 0.35, 0.84],
        RegionId::RightSidebar => match panels
            .right
            .active_tab_id()
            .unwrap_or(PanelTabId::Inspector)
        {
            PanelTabId::Inspector => [0.16, 0.20, 0.30, 0.92],
            PanelTabId::Outline => [0.18, 0.22, 0.32, 0.92],
            _ => [0.16, 0.20, 0.30, 0.92],
        },
        RegionId::BottomPanel => match panels
            .bottom
            .active_tab_id()
            .unwrap_or(PanelTabId::Terminal)
        {
            PanelTabId::Terminal => [0.11, 0.16, 0.25, 0.94],
            PanelTabId::DebugConsole => [0.15, 0.13, 0.26, 0.94],
            PanelTabId::Problems => [0.25, 0.13, 0.15, 0.94],
            _ => [0.13, 0.16, 0.24, 0.94],
        },
        RegionId::StatusBar => {
            if debug_state.paused {
                [0.38, 0.24, 0.16, 0.97]
            } else {
                [0.18, 0.30, 0.20, 0.97]
            }
        }
        RegionId::OverlayLayer => [0.95, 0.58, 0.25, 0.10],
        RegionId::Root => [0.0, 0.0, 0.0, 0.0],
    }
}

fn inspector_section_color(kind: InspectorSectionKind, selected: bool) -> [f32; 4] {
    let base = match kind {
        InspectorSectionKind::Variables => [0.14, 0.19, 0.27, 0.88],
        InspectorSectionKind::Watch => [0.17, 0.18, 0.28, 0.88],
        InspectorSectionKind::CallStack => [0.18, 0.21, 0.29, 0.88],
        InspectorSectionKind::Breakpoints => [0.24, 0.16, 0.18, 0.88],
    };
    if selected { brighten(base) } else { base }
}

fn brighten(mut color: [f32; 4]) -> [f32; 4] {
    color[0] = (color[0] + 0.12).min(1.0);
    color[1] = (color[1] + 0.12).min(1.0);
    color[2] = (color[2] + 0.12).min(1.0);
    color[3] = (color[3] + 0.04).min(1.0);
    color
}

fn focus_matches_region(focus: FocusTarget, region_id: RegionId) -> bool {
    matches!(
        (focus, region_id),
        (FocusTarget::CenterEditor, RegionId::Center)
            | (FocusTarget::LeftSidebar, RegionId::LeftSidebar)
            | (FocusTarget::RightSidebar, RegionId::RightSidebar)
            | (FocusTarget::BottomPanel, RegionId::BottomPanel)
            | (FocusTarget::TopBar, RegionId::TopBar)
            | (FocusTarget::StatusBar, RegionId::StatusBar)
            | (FocusTarget::OverlayLayer, RegionId::OverlayLayer)
    )
}

fn log_layout(
    model: &RegionModel,
    panels: &WorkbenchPanelState,
    focus: FocusTarget,
    inspector: &InspectorPanelState,
    overlay_count: usize,
    editor_scroll_lines: usize,
    editor_scroll_max: usize,
    palette_query: &str,
) {
    println!(
        "[WorkbenchState] focus={} left(tab={},visible={},w={:.1}) right(tab={},visible={},w={:.1}) bottom(tab={},visible={},h={:.1}) overlay={} overlays={} editor_scroll={}/{} palette_query={:?} inspector_selected={:?}",
        focus.label(),
        panels.left.active_tab_label(),
        panels.left.visible,
        panels.left.size_px,
        panels.right.active_tab_label(),
        panels.right.visible,
        panels.right.size_px,
        panels.bottom.active_tab_label(),
        panels.bottom.visible,
        panels.bottom.size_px,
        panels.overlay_visible,
        overlay_count,
        editor_scroll_lines,
        editor_scroll_max,
        palette_query,
        inspector.selected_row_label()
    );

    for region in model.flatten() {
        if region.id == RegionId::Root || !region.visible {
            continue;
        }
        println!(
            "[Workbench] {} x={:.1} y={:.1} w={:.1} h={:.1}",
            region.id.label(),
            region.bounds.x,
            region.bounds.y,
            region.bounds.width,
            region.bounds.height
        );
    }
}

struct WorkbenchApp {
    window: Option<Arc<Window>>,
    renderer: Option<WorkbenchRenderer>,
}

impl WorkbenchApp {
    fn new() -> Self {
        Self {
            window: None,
            renderer: None,
        }
    }
}

impl ApplicationHandler for WorkbenchApp {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }

        let attrs = Window::default_attributes()
            .with_title(WINDOW_TITLE)
            .with_inner_size(PhysicalSize::new(WINDOW_WIDTH, WINDOW_HEIGHT));
        let window = match event_loop.create_window(attrs) {
            Ok(window) => Arc::new(window),
            Err(err) => {
                eprintln!("create_window failed: {err}");
                event_loop.exit();
                return;
            }
        };

        let renderer = match pollster::block_on(WorkbenchRenderer::new(window.clone())) {
            Ok(renderer) => renderer,
            Err(err) => {
                eprintln!("renderer init failed: {err}");
                event_loop.exit();
                return;
            }
        };

        println!("Phase11.3 controls:");
        println!("  Cmd/Ctrl+1|2|3|4 -> toggle Left/Right/Bottom/Overlay");
        println!("  Cmd/Ctrl+P -> toggle command palette overlay (window-relative)");
        println!("  F6 or Cmd/Ctrl+Tab -> cycle focus, Esc -> focus editor");
        println!("  F7 or [ ] -> switch tab in focused panel");
        println!("  Right Sidebar focus: j/k or Up/Down select, Enter/Left/Right toggle expand");
        println!(
            "  F5 pause/resume debug, F9 toggle breakpoint, Up/Down in editor moves execution line"
        );
        println!("  Scroll wheel in editor -> scroll sample code (overlays follow)");
        println!("  Click gutter in editor -> toggle breakpoint and sync inspector list");
        println!("  Command palette focus: type/backspace/enter, Esc closes and restores focus");
        println!("  Drag split handles with mouse to resize panels");

        self.window = Some(window);
        self.renderer = Some(renderer);
        if let Some(window) = &self.window {
            window.request_redraw();
        }
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
            WindowEvent::Resized(new_size) => {
                if let Some(renderer) = self.renderer.as_mut() {
                    renderer.resize(new_size);
                }
                if let Some(window) = &self.window {
                    window.request_redraw();
                }
            }
            WindowEvent::RedrawRequested => {
                if let Some(renderer) = self.renderer.as_mut() {
                    if let Err(err) = renderer.render() {
                        eprintln!("render error: {err}");
                        renderer.reconfigure_surface();
                    }
                }
            }
            WindowEvent::ModifiersChanged(modifiers) => {
                if let Some(renderer) = self.renderer.as_mut() {
                    renderer.update_modifiers(modifiers.state());
                }
            }
            WindowEvent::KeyboardInput { event, .. } => {
                if let Some(renderer) = self.renderer.as_mut()
                    && renderer.handle_key_input(&event)
                    && let Some(window) = &self.window
                {
                    window.request_redraw();
                }
            }
            WindowEvent::CursorMoved { position, .. } => {
                if let Some(renderer) = self.renderer.as_mut()
                    && renderer.on_cursor_moved(position)
                    && let Some(window) = &self.window
                {
                    window.request_redraw();
                }
            }
            WindowEvent::MouseWheel { delta, .. } => {
                if let Some(renderer) = self.renderer.as_mut()
                    && renderer.on_mouse_wheel(delta)
                    && let Some(window) = &self.window
                {
                    window.request_redraw();
                }
            }
            WindowEvent::MouseInput {
                state,
                button: MouseButton::Left,
                ..
            } => {
                let changed = if let Some(renderer) = self.renderer.as_mut() {
                    match state {
                        ElementState::Pressed => renderer.on_mouse_left_pressed(),
                        ElementState::Released => renderer.on_mouse_left_released(),
                    }
                } else {
                    false
                };

                if changed && let Some(window) = &self.window {
                    window.request_redraw();
                }
            }
            _ => {}
        }
    }
}

fn main() -> Result<(), winit::error::EventLoopError> {
    let event_loop = EventLoop::new()?;
    event_loop.set_control_flow(ControlFlow::Wait);
    let mut app = WorkbenchApp::new();
    event_loop.run_app(&mut app)
}
