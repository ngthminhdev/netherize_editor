use super::*;
#[cfg(target_os = "macos")]
use winit::platform::macos::WindowAttributesExtMacOS;
use winit::{
    event::{ElementState, MouseButton, MouseScrollDelta},
    keyboard::{Key, KeyCode, NamedKey, PhysicalKey},
};

const FOCUS_RING_THICKNESS: f32 = 2.0;
const TERMINAL_SAFE_INSET_X: f32 = 2.0;
/// Delay before the which-key overlay appears for a pending chord — long
/// enough that fast chords never flash it, short enough to help a stuck user.
const WHICHKEY_DELAY: Duration = Duration::from_millis(300);

#[cfg(target_os = "macos")]
fn apply_platform_window_chrome(
    attrs: winit::window::WindowAttributes,
) -> winit::window::WindowAttributes {
    attrs
        .with_titlebar_transparent(true)
        .with_title_hidden(true)
        .with_fullsize_content_view(true)
}

#[cfg(not(target_os = "macos"))]
fn apply_platform_window_chrome(
    attrs: winit::window::WindowAttributes,
) -> winit::window::WindowAttributes {
    attrs
}

fn statusbar_source_path_label(
    active_file: Option<&Path>,
    workspace_root: Option<&Path>,
) -> String {
    let Some(path) = active_file else {
        return String::new();
    };

    if let Some(root) = workspace_root
        && let Ok(relative) = path.strip_prefix(root)
    {
        let relative = relative.display().to_string();
        if !relative.is_empty() {
            return relative;
        }
    }

    path.file_name()
        .and_then(|name| name.to_str())
        .map(str::to_owned)
        .unwrap_or_else(|| path.display().to_string())
}

fn visible_region_bounds(
    flat_regions: &[crate::workbench::region_model::FlatRegion],
    id: RegionId,
) -> Option<[f32; 4]> {
    flat_regions
        .iter()
        .find(|region| region.id == id && region.visible)
        .map(|region| {
            [
                region.bounds.x,
                region.bounds.y,
                region.bounds.width,
                region.bounds.height,
            ]
        })
}

fn point_in_bounds(position: (f32, f32), bounds: [f32; 4]) -> bool {
    let (x, y) = position;
    x >= bounds[0] && x <= bounds[0] + bounds[2] && y >= bounds[1] && y <= bounds[1] + bounds[3]
}

fn terminal_cell_at_position(
    position: (f32, f32),
    bounds: [f32; 4],
    cols: usize,
    rows: usize,
    panel_padding: f32,
    font_size: f32,
    line_height: f32,
) -> Option<(usize, usize)> {
    if cols == 0 || rows == 0 || !point_in_bounds(position, bounds) {
        return None;
    }

    let origin_x = bounds[0] + panel_padding + TERMINAL_SAFE_INSET_X;
    let origin_y = bounds[1] + panel_padding;
    let cell_width = (font_size * 0.6).max(1.0);
    let cell_height = line_height.max(1.0);
    let x = position.0 - origin_x;
    let y = position.1 - origin_y;
    if x < 0.0 || y < 0.0 {
        return None;
    }

    let col = (x / cell_width).floor() as usize;
    let row = (y / cell_height).floor() as usize;
    if col >= cols || row >= rows {
        return None;
    }
    // Xterm mouse coordinates are 1-based column/row values.
    Some((col + 1, row + 1))
}

fn sgr_mouse_sequence(button_code: u8, col: usize, row: usize, pressed: bool) -> String {
    let suffix = if pressed { 'M' } else { 'm' };
    format!("\x1b[<{button_code};{col};{row}{suffix}")
}

/// SGR wheel events are press-only: button 64 scrolls up, 65 scrolls down.
fn sgr_wheel_sequence(scroll_up: bool, col: usize, row: usize) -> String {
    let code = if scroll_up { 64 } else { 65 };
    sgr_mouse_sequence(code, col, row, true)
}

fn mouse_button_code(button: MouseButton) -> Option<u8> {
    match button {
        MouseButton::Left => Some(0),
        MouseButton::Middle => Some(1),
        MouseButton::Right => Some(2),
        _ => None,
    }
}

fn symbol_kind_label(kind: &str) -> String {
    match kind {
        "Class" => "class".to_string(),
        "Method" => "method".to_string(),
        "Function" => "fn".to_string(),
        "Constant" => "const".to_string(),
        "Variable" => "var".to_string(),
        "Property" => "prop".to_string(),
        "Field" => "field".to_string(),
        "Interface" => "interface".to_string(),
        "Struct" => "struct".to_string(),
        "Enum" => "enum".to_string(),
        "EnumMember" => "member".to_string(),
        "Namespace" => "namespace".to_string(),
        "Module" => "module".to_string(),
        "Constructor" => "constructor".to_string(),
        other => other.to_ascii_lowercase(),
    }
}

fn symbol_kind_color(kind: &str, theme: &ThemeConfig) -> [f32; 4] {
    match kind {
        "Class" | "Interface" | "Struct" | "Enum" | "TypeParameter" => theme.ui.magenta.as_f32(),
        "Method" | "Function" | "Constructor" => theme.ui.cyan.as_f32(),
        "Constant" | "Variable" | "Property" | "Field" | "EnumMember" => theme.ui.info.as_f32(),
        "Namespace" | "Module" | "Package" => theme.ui.amber.as_f32(),
        _ => theme.ui.fg.as_f32(),
    }
}

fn breadcrumb_segment_text(kind: &str, name: &str) -> String {
    const BREADCRUMB_ICON_PREFIX: &str = "[sym]";
    let label = symbol_kind_label(kind);
    if kind == "Constructor" && name.eq_ignore_ascii_case("constructor") {
        format!("{BREADCRUMB_ICON_PREFIX} {label}")
    } else {
        format!("{BREADCRUMB_ICON_PREFIX} {label} {name}")
    }
}

fn cursor_in_lsp_range(
    range: &crate::async_runtime::message::LspRange,
    line: usize,
    col: usize,
) -> bool {
    let start_line = range.start.line as usize;
    let end_line = range.end.line as usize;
    let start_col = range.start.character as usize;
    let end_col = range.end.character as usize;
    (line > start_line || (line == start_line && col >= start_col))
        && (line < end_line || (line == end_line && col <= end_col))
}

fn symbol_range_score(range: &crate::async_runtime::message::LspRange) -> (usize, usize) {
    let start_line = range.start.line as usize;
    let end_line = range.end.line as usize;
    let start_col = range.start.character as usize;
    let end_col = range.end.character as usize;
    (
        end_line.saturating_sub(start_line),
        end_col.saturating_sub(start_col),
    )
}

fn build_editor_breadcrumb_segments(
    symbols: &[crate::async_runtime::message::LspDocumentSymbol],
    cursor_line: usize,
    cursor_col: usize,
    theme: &ThemeConfig,
) -> Vec<crate::render::renderer::EditorBreadcrumbSegment> {
    let current = symbols
        .iter()
        .filter(|symbol| cursor_in_lsp_range(&symbol.range, cursor_line, cursor_col))
        .min_by(|left, right| {
            left.ancestors
                .len()
                .cmp(&right.ancestors.len())
                .reverse()
                .then_with(|| {
                    symbol_range_score(&left.range).cmp(&symbol_range_score(&right.range))
                })
        });

    let Some(current) = current else {
        return Vec::new();
    };

    let mut segments = Vec::new();
    for ancestor in &current.ancestors {
        segments.push(crate::render::renderer::EditorBreadcrumbSegment {
            text: breadcrumb_segment_text(&ancestor.kind, &ancestor.name),
            color: symbol_kind_color(&ancestor.kind, theme),
        });
    }
    segments.push(crate::render::renderer::EditorBreadcrumbSegment {
        text: breadcrumb_segment_text(&current.kind, &current.name),
        color: symbol_kind_color(&current.kind, theme),
    });
    segments
}

impl AppShell {
    fn current_right_sidebar_bounds(&self) -> Option<[f32; 4]> {
        let layout = self
            .layout_engine
            .compute(self.window_size, &self.panel_state);
        let flat_regions: Vec<_> = layout.model.flatten();
        visible_region_bounds(&flat_regions, RegionId::RightSidebar)
    }

    fn right_terminal_active(&self) -> bool {
        self.panel_state.right.visible
            && self.panel_state.right.active_tab_id() == Some(PanelTabId::Terminal)
    }

    fn right_terminal_mouse_cell_at(&self, position: (f32, f32)) -> Option<(usize, usize)> {
        if !self.right_terminal_active() {
            return None;
        }
        let bounds = self.current_right_sidebar_bounds()?;
        let scaled_ui = scale_ui_config(&self.ui_config, self.runtime_scale);
        let panel_padding = scaled_ui
            .layout
            .inner_padding
            .max(scaled_ui.spacing.panel_padding);
        terminal_cell_at_position(
            position,
            bounds,
            self.right_terminal_grid.cols,
            self.right_terminal_grid.rows,
            panel_padding,
            self.theme.ui.panel_font_size,
            self.theme.ui.panel_line_height,
        )
    }

    fn right_terminal_mouse_targeted(&self) -> bool {
        if !self.right_terminal_active() {
            return false;
        }
        let Some(bounds) = self.current_right_sidebar_bounds() else {
            return false;
        };
        self.last_cursor_position
            .is_some_and(|position| point_in_bounds(position, bounds))
            || self.focus_manager.current() == FocusTarget::RightSidebar
    }

    fn focus_right_terminal_for_mouse(&mut self) {
        let _ = self.handle_command(Command::AiChatFocus);
    }

    fn handle_right_terminal_mouse_wheel(&mut self, delta: MouseScrollDelta) -> bool {
        if !self.right_terminal_mouse_targeted() {
            return false;
        }

        let line_height = self.theme.ui.panel_line_height.max(1.0) as f64;
        let Some((scroll_up, steps)) = (match delta {
            MouseScrollDelta::LineDelta(_, y) if y.abs() > f32::EPSILON => {
                Some((y > 0.0, y.abs().ceil() as usize))
            }
            MouseScrollDelta::PixelDelta(position) if position.y.abs() > f64::EPSILON => Some((
                position.y > 0.0,
                (position.y.abs() / line_height).ceil() as usize,
            )),
            _ => None,
        }) else {
            return false;
        };

        self.focus_right_terminal_for_mouse();

        // opencode keeps its chat history inside its own TUI viewport, so the
        // wheel must be forwarded as SGR mouse events for opencode to scroll
        // itself (its mouse capture is on by default). Scrolling our grid
        // scrollback would show nothing — full-screen repaints never reach it.
        let (col, row) = self
            .last_cursor_position
            .and_then(|position| self.right_terminal_mouse_cell_at(position))
            .unwrap_or((
                (self.right_terminal_grid.cols / 2).max(1),
                (self.right_terminal_grid.rows / 2).max(1),
            ));
        let mut changed = false;
        for _ in 0..steps.max(1).min(24) {
            changed |= self.handle_command(Command::TerminalWriteInput(sgr_wheel_sequence(
                scroll_up, col, row,
            )));
        }
        changed
    }

    fn handle_right_terminal_mouse_input(
        &mut self,
        button: MouseButton,
        state: ElementState,
    ) -> bool {
        let Some(base_code) = mouse_button_code(button) else {
            return false;
        };
        let Some(position) = self.last_cursor_position else {
            return false;
        };
        if !self.right_terminal_active() {
            return false;
        }
        let Some(bounds) = self.current_right_sidebar_bounds() else {
            return false;
        };
        if !point_in_bounds(position, bounds) {
            return false;
        }

        let cell = self.right_terminal_mouse_cell_at(position);
        self.focus_right_terminal_for_mouse();

        if let Some((col, row)) = cell {
            let modifiers = self.input_handler.current_modifiers();
            let mut code = base_code;
            if modifiers.shift_key() {
                code += 4;
            }
            if modifiers.alt_key() {
                code += 8;
            }
            if modifiers.control_key() {
                code += 16;
            }
            let sequence = sgr_mouse_sequence(code, col, row, state == ElementState::Pressed);
            let _ = self.handle_command(Command::TerminalWriteInput(sequence));
        }
        true
    }
}

impl ApplicationHandler<AppEvent> for AppShell {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }

        let mut attrs =
            Window::default_attributes().with_title(self.ui_config.window.title.clone());
        attrs = match self.ui_config.window.startup_mode {
            WindowStartupMode::Windowed => attrs.with_inner_size(LogicalSize::new(
                f64::from(self.ui_config.window.width),
                f64::from(self.ui_config.window.height),
            )),
            WindowStartupMode::Maximized | WindowStartupMode::Fullscreen => {
                attrs.with_maximized(true)
            }
        };
        attrs = apply_platform_window_chrome(attrs);

        let window = match event_loop.create_window(attrs) {
            Ok(window) => Arc::new(window),
            Err(err) => {
                eprintln!("[AppShell] create_window failed: {err}");
                event_loop.exit();
                return;
            }
        };

        match self.ui_config.window.startup_mode {
            WindowStartupMode::Windowed => {}
            WindowStartupMode::Maximized => window.set_maximized(true),
            WindowStartupMode::Fullscreen => {
                window.set_fullscreen(Some(Fullscreen::Borderless(None)));
            }
        }

        let renderer = match pollster::block_on(Renderer::new(window.clone())) {
            Ok(renderer) => renderer,
            Err(err) => {
                eprintln!("[AppShell] renderer init failed: {err}");
                event_loop.exit();
                return;
            }
        };

        self.editor_needs_layout = true;
        self.editor_caret_needs_layout = false;
        self.sidebar_needs_layout = true;
        self.terminal_needs_layout = true;
        self.buffer_terminal_needs_layout = true;
        self.last_editor_bounds = None;
        self.last_show_welcome = None;
        self.last_sidebar_bounds = None;
        self.last_terminal_bounds = None;
        self.last_buffer_terminal_bounds = None;

        self.window_size = window.inner_size();
        self.window = Some(window);
        self.renderer = Some(renderer);
        let scale_factor = self
            .window
            .as_ref()
            .map(|window| window.scale_factor())
            .unwrap_or(1.0);
        self.update_runtime_scaling_for_window(scale_factor);

        self.startup_subsystems();
        self.update_window_title();
        self.show_first_run_tour_if_needed();
        self.request_redraw();
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
                self.window_size = new_size;
                if let Some(renderer) = self.renderer.as_mut() {
                    renderer.resize(new_size);
                }
                self.editor_needs_layout = true;
                self.editor_caret_needs_layout = false;
                self.sidebar_needs_layout = true;
                self.terminal_needs_layout = true;
                self.buffer_terminal_needs_layout = true;
                self.last_editor_bounds = None;
                self.last_show_welcome = None;
                self.last_sidebar_bounds = None;
                self.last_terminal_bounds = None;
                self.last_buffer_terminal_bounds = None;
                let scale_factor = self
                    .window
                    .as_ref()
                    .map(|window| window.scale_factor())
                    .unwrap_or(1.0);
                self.update_runtime_scaling_for_window(scale_factor);
                self.request_redraw();
            }
            WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                // Refresh the cached physical size before recomputing: when the
                // window crosses monitors the old physical size belongs to the
                // previous scale factor and inflates content_scale (UI zoom bug).
                if let Some(window) = self.window.as_ref() {
                    self.window_size = window.inner_size();
                }
                self.update_runtime_scaling_for_window(scale_factor);
                self.request_redraw();
            }
            WindowEvent::Focused(focused) => {
                self.input_handler.on_focus_changed(focused);
            }
            WindowEvent::ModifiersChanged(mods) => {
                self.input_handler.update_modifiers(mods);
            }
            WindowEvent::Ime(Ime::Commit(text)) => {
                if self.pending_confirmation.is_some() {
                    return;
                }
                if self.handle_explorer_filter_ime_commit(&text) {
                    self.request_redraw();
                    return;
                }
                if self.should_swallow_palette_ime_commit() {
                    return;
                }
                let overlay_cleared = self.invalidate_editor_overlays();
                let context = self.build_context();
                if let Some(translated) = self.input_handler.translate_ime_commit(&text, context)
                    && self.handle_command_with_count(translated.command, translated.repeat_count)
                {
                    self.request_redraw();
                } else if overlay_cleared {
                    self.request_redraw();
                }
            }
            WindowEvent::KeyboardInput {
                event: key_event, ..
            } => {
                if key_event.state == ElementState::Pressed && !key_event.repeat {
                    self.note_post_open_keyboard_press();
                    let has_hover_overlay = self.app_state.has_scrollable_floating_overlay();
                    let is_modifier_key = matches!(
                        key_event.logical_key,
                        Key::Named(
                            NamedKey::Control
                                | NamedKey::Shift
                                | NamedKey::Alt
                                | NamedKey::Super
                                | NamedKey::Meta
                        )
                    );
                    let keep_hover_for_scroll = has_hover_overlay
                        && matches!(
                            key_event.physical_key,
                            PhysicalKey::Code(KeyCode::KeyD | KeyCode::KeyU)
                        )
                        && self.input_handler.current_modifiers().control_key()
                        && !self.input_handler.current_modifiers().super_key();
                    let keep_hover_for_modifier = has_hover_overlay && is_modifier_key;
                    if !keep_hover_for_scroll && !keep_hover_for_modifier {
                        let overlay_cleared = self.invalidate_editor_overlays();
                        if overlay_cleared {
                            self.request_redraw();
                        }
                    }
                }
                if key_event.state == ElementState::Pressed
                    && !key_event.repeat
                    && matches!(key_event.physical_key, PhysicalKey::Code(KeyCode::KeyN))
                    && self.input_handler.current_modifiers().super_key()
                    && self.input_handler.current_modifiers().shift_key()
                    && self.handle_command(Command::NewInstance)
                {
                    self.request_redraw();
                    return;
                }

                if let Some(changed) = self.handle_pending_confirmation_key_event(&key_event) {
                    if changed {
                        self.request_redraw();
                    }
                    return;
                }

                // LSP Install Guide popup — intercept input khi popup active.
                if self.active_lsp_guide.is_some() && key_event.state == ElementState::Pressed {
                    let named = match &key_event.logical_key {
                        Key::Named(n) => Some(*n),
                        _ => None,
                    };

                    match named {
                        Some(NamedKey::Escape) => {
                            self.dismiss_lsp_guide();
                            self.request_redraw();
                        }
                        Some(NamedKey::Enter) => {
                            if self.accept_lsp_install_guide() {
                                self.request_redraw();
                            }
                        }
                        _ => {}
                    }
                    // Swallow tất cả input khi popup active.
                    return;
                }

                // System Dependency Check popup — intercept input khi popup active.
                if self.active_system_dep_guide.is_some()
                    && key_event.state == ElementState::Pressed
                {
                    let named = match &key_event.logical_key {
                        Key::Named(n) => Some(*n),
                        _ => None,
                    };
                    match named {
                        Some(NamedKey::Escape) => {
                            self.dismiss_system_dep_guide();
                            self.request_redraw();
                        }
                        Some(NamedKey::Enter) => {
                            if self.accept_system_dep_guide() {
                                self.request_redraw();
                            }
                        }
                        _ => {}
                    }
                    return;
                }
                if let Some(changed) = self.handle_explorer_filter_key_event(&key_event) {
                    if changed {
                        self.request_redraw();
                    }
                    return;
                }
                let context = self.build_context();
                let outcome =
                    self.input_handler
                        .translate_key_event(&key_event, &self.input_map, context);
                match outcome {
                    Some(InputRouteOutcome::Dispatch(translated)) => {
                        if self
                            .handle_command_with_count(translated.command, translated.repeat_count)
                        {
                            self.request_redraw();
                        }
                    }
                    Some(InputRouteOutcome::NoDispatch { .. }) => {
                        // Pending state may have changed (chord started, advanced,
                        // or cancelled by Esc/timeout): refresh so the statusbar
                        // pending keys and the which-key overlay track it.
                        self.request_redraw();
                    }
                    None => {}
                }
            }
            WindowEvent::CursorMoved { position, .. } => {
                self.last_cursor_position = Some((position.x as f32, position.y as f32));
            }
            WindowEvent::MouseWheel { delta, .. } => {
                if self.handle_right_terminal_mouse_wheel(delta) {
                    self.request_redraw();
                } else if self.invalidate_editor_overlays() {
                    self.request_redraw();
                }
            }
            WindowEvent::MouseInput { state, button, .. } => {
                if self.handle_right_terminal_mouse_input(button, state) {
                    self.request_redraw();
                }
            }
            WindowEvent::RedrawRequested => self.redraw(),
            _ => {}
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        self.flush_pending_parse_after_debounce();
        self.flush_pending_git_diff_after_debounce();
        self.enforce_ai_inline_anchor();
        self.flush_pending_ai_inline_completion();
        self.flush_pending_lsp_did_change_after_debounce();
        self.flush_pending_lsp_completion_after_debounce();
        self.flush_pending_completion_resolve_after_debounce();
        if self.flush_lsp_retry_if_due() {
            self.request_redraw();
        }
        if self.maybe_refresh_workspace_git_branch(false) {
            self.request_redraw();
        }
        if self.maybe_refresh_workspace_git_status() {
            self.request_redraw();
        }
        if self.tick_smooth_scroll_animation() {
            self.request_redraw();
        }
        if self.tick_thinking_animation() {
            self.request_redraw();
        }
        if self.tick_lsp_loading_animation() {
            self.request_redraw();
        }
        let now = Instant::now();
        // #6: notify watcher giờ đã wake loop & realtime nên poll chỉ còn là safety-net
        // (file ngoài tầm watcher, watcher chết, mtime cùng giây…). 3s đủ, đỡ wake CPU.
        if now.duration_since(self.last_external_file_check) >= Duration::from_secs(3) {
            self.last_external_file_check = now;
            // #4: poll chỉ stat mtime; nội dung được worker đọc và áp về qua
            // ExternalFilesRead -> apply_external_file_contents (kèm LSP sync).
            let changed_paths = self
                .app_state
                .collect_externally_modified_open_buffers(&mut self.last_external_file_check_times);
            if !changed_paths.is_empty() {
                self.submit(RequestSpec {
                    revision_id: 0,
                    topic: RequestTopic::WorkspaceWatch,
                    payload: WorkerRequestPayload::ReadExternalFiles {
                        paths: changed_paths,
                    },
                });
            }
        }
        if self.tick_caret_blink() {
            self.request_redraw();
        }
        if self.pump_bridge() {
            self.request_redraw();
        }
        if self.clear_expired_transient_toast() {
            self.request_redraw();
        }
        if self.tick_whichkey_delay() {
            self.request_redraw();
        }
        if self.app_state.workspace_is_inputting_filter() {
            self.sidebar_needs_layout = true;
            self.request_redraw();
        }

        let mut next_deadline = Some(self.next_git_branch_refresh_deadline());
        let external_check_deadline = self.last_external_file_check + Duration::from_secs(3);
        next_deadline = Some(match next_deadline {
            Some(existing) => existing.min(external_check_deadline),
            None => external_check_deadline,
        });
        if let Some(git_status_deadline) = self.next_workspace_git_status_refresh_deadline() {
            next_deadline = Some(
                next_deadline
                    .unwrap_or(git_status_deadline)
                    .min(git_status_deadline),
            );
        }
        if let Some(lsp_deadline) = self.next_lsp_did_change_flush_deadline() {
            next_deadline = Some(match next_deadline {
                Some(existing) => existing.min(lsp_deadline),
                None => lsp_deadline,
            });
        }
        if let Some(ai_deadline) = self.next_ai_inline_flush_deadline() {
            next_deadline = Some(match next_deadline {
                Some(existing) => existing.min(ai_deadline),
                None => ai_deadline,
            });
        }
        if let Some(git_diff_deadline) = self.next_git_diff_recalc_deadline() {
            next_deadline = Some(match next_deadline {
                Some(existing) => existing.min(git_diff_deadline),
                None => git_diff_deadline,
            });
        }
        if let Some(lsp_retry_deadline) = self.next_lsp_retry_deadline() {
            next_deadline = Some(match next_deadline {
                Some(existing) => existing.min(lsp_retry_deadline),
                None => lsp_retry_deadline,
            });
        }
        if !self.whichkey_redraw_fired
            && let Some((_, started_at)) = self.input_handler.pending_chord_sequence()
        {
            let whichkey_deadline = started_at + WHICHKEY_DELAY;
            next_deadline = Some(match next_deadline {
                Some(existing) => existing.min(whichkey_deadline),
                None => whichkey_deadline,
            });
        }

        if let Some(deadline) = next_deadline {
            event_loop.set_control_flow(ControlFlow::WaitUntil(deadline));
        } else {
            event_loop.set_control_flow(ControlFlow::Wait);
        }
    }

    fn user_event(&mut self, _event_loop: &ActiveEventLoop, event: AppEvent) {
        match event {
            AppEvent::TerminalOutputReady => {
                if self.pump_bridge() {
                    self.terminal_needs_layout = true;
                    self.buffer_terminal_needs_layout = true;
                }
                self.caret_blink_visible = true;
                self.caret_blink_dirty = true;
                self.last_caret_blink_tick = Instant::now();
                self.request_redraw();
            }
            AppEvent::AiInlineReady => {
                if self.pump_bridge() {
                    self.request_redraw();
                }
            }
            AppEvent::WorkerMessageReady => {
                if self.pump_bridge() {
                    self.request_redraw();
                }
            }
        }
    }
}

impl AppShell {
    /// True exactly once when a pending chord crosses `WHICHKEY_DELAY`, so the
    /// frame that first shows the which-key overlay gets scheduled.
    fn tick_whichkey_delay(&mut self) -> bool {
        match self.input_handler.pending_chord_sequence() {
            Some((_, started_at)) => {
                if !self.whichkey_redraw_fired && started_at.elapsed() >= WHICHKEY_DELAY {
                    self.whichkey_redraw_fired = true;
                    true
                } else {
                    false
                }
            }
            None => {
                self.whichkey_redraw_fired = false;
                false
            }
        }
    }

    fn tick_smooth_scroll_animation(&mut self) -> bool {
        // Global kill-switch: disable smooth scroll and always snap to target.
        self.last_scroll_animation_tick = Instant::now();
        let target = self.app_state.target_scroll_y;
        let current = self.app_state.current_scroll_y;
        if (target - current).abs() > f32::EPSILON {
            self.app_state.current_scroll_y = target;
            self.editor_needs_layout = true;
            true
        } else {
            false
        }
    }

    fn tick_lsp_loading_animation(&mut self) -> bool {
        if self.pending_lsp_server.is_none() {
            return false;
        }
        let now = Instant::now();
        if now.duration_since(self.last_lsp_loading_animation_tick)
            >= LSP_LOADING_ANIMATION_INTERVAL
        {
            self.last_lsp_loading_animation_tick = now;
            self.lsp_loading_frame = self.lsp_loading_frame.wrapping_add(1);
            true
        } else {
            false
        }
    }

    fn tick_thinking_animation(&mut self) -> bool {
        if !self.panel_state.ai_chat.is_generating {
            return false;
        }
        let now = Instant::now();
        if now.duration_since(self.last_thinking_animation_tick) >= THINKING_ANIMATION_INTERVAL {
            self.last_thinking_animation_tick = now;
            true
        } else {
            false
        }
    }

    /// Tối ưu 3: Caret Blink — tick timer nhấp nháy, chỉ set caret_blink_dirty.
    /// KHÔNG set editor_needs_layout hay editor_caret_needs_layout.
    /// Nhờ đó toàn bộ text pipeline không bị trigger reshape chỉ vì con trỏ nháy.
    fn tick_caret_blink(&mut self) -> bool {
        let now = Instant::now();
        if now.duration_since(self.last_caret_blink_tick) >= Duration::from_millis(1000) {
            self.last_caret_blink_tick = now;
            self.caret_blink_visible = !self.caret_blink_visible;
            self.caret_blink_dirty = true;
            return true;
        }
        false
    }

    fn handle_explorer_filter_ime_commit(&mut self, text: &str) -> bool {
        if self.focus_manager.current() != FocusTarget::LeftSidebar
            || !self.app_state.workspace_is_inputting_filter()
        {
            return false;
        }
        if self.app_state.workspace_append_filter_text(text) {
            self.mark_explorer_dirty();
            return true;
        }
        false
    }

    fn handle_explorer_filter_key_event(
        &mut self,
        key_event: &winit::event::KeyEvent,
    ) -> Option<bool> {
        if self.focus_manager.current() != FocusTarget::LeftSidebar
            || !self.app_state.workspace_is_inputting_filter()
        {
            return None;
        }

        if key_event.state != ElementState::Pressed {
            return Some(false);
        }

        let changed = match key_event.logical_key.as_ref() {
            Key::Named(NamedKey::Escape) | Key::Named(NamedKey::Enter) => {
                let changed = self.app_state.workspace_stop_filter_input();
                if changed {
                    self.input_handler.clear_pending_prefix();
                    self.mark_explorer_dirty();
                }
                changed
            }
            Key::Named(NamedKey::Backspace) => {
                let changed = self.app_state.workspace_backspace_filter();
                if changed {
                    self.mark_explorer_dirty();
                }
                changed
            }
            Key::Character(text) => {
                let changed = self.app_state.workspace_append_filter_text(text);
                if changed {
                    self.mark_explorer_dirty();
                }
                changed
            }
            _ => false,
        };

        Some(changed)
    }

    fn redraw(&mut self) {
        let got_new_data = self.pump_bridge();
        self.update_frame_metrics_snapshot(Instant::now());
        let layout = self
            .layout_engine
            .compute(self.window_size, &self.panel_state);

        let flat_regions: Vec<_> = layout.model.flatten();
        let sidebar_region = flat_regions
            .iter()
            .find(|region| region.id == RegionId::LeftSidebar);
        let bottom_region = flat_regions
            .iter()
            .find(|region| region.id == RegionId::BottomPanel);
        let center_bounds = layout
            .model
            .find(RegionId::Center)
            .map(|center| [center.x, center.y, center.width, center.height]);
        // Show the centered empty-state only when no tab is open and no overlay
        // is actively using the center of the screen.
        let show_welcome = self.should_show_welcome();
        let show_welcome_changed = self.last_show_welcome != Some(show_welcome);
        let workspace_attached = self.app_state.workspace_root_path().is_some();

        // Keep the explorer hidden on a true "empty" start, but once a workspace
        // has been attached explicitly we still allow the tree to stay visible
        // even if there are no open tabs yet.
        let sidebar_bounds = sidebar_region.and_then(|sidebar| {
            (sidebar.visible && (!show_welcome || workspace_attached)).then_some([
                sidebar.bounds.x,
                sidebar.bounds.y,
                sidebar.bounds.width,
                sidebar.bounds.height,
            ])
        });

        if sidebar_bounds.is_some() {
            self.ensure_explorer_snapshot();
        }

        let panel_radius = if self.layout_engine.config.round_ui {
            self.ui_config.border_radius_px
        } else {
            0.0
        };
        let mut default_outline = self.theme.ui.accent.as_f32();
        default_outline[3] = default_outline[3].max(0.95);
        let focus_target = if show_welcome && !workspace_attached {
            FocusTarget::CenterEditor
        } else {
            self.focus_manager.current()
        };
        let center_has_error_diagnostics = self
            .app_state
            .active_file()
            .and_then(|path| self.app_state.diagnostics_for_path(path))
            .is_some_and(|items| items.iter().any(|item| item.severity == Some(1)));
        let focus_region = focus_target_region_id(focus_target);
        let mut focused_outline =
            if focus_target == FocusTarget::CenterEditor && center_has_error_diagnostics {
                self.theme.ui.error.as_f32()
            } else {
                self.theme.ui.cyan.as_f32()
            };
        focused_outline[3] = focused_outline[3].max(0.95);

        // RightSidebar (AI Chat) background: flat fill + input-box accent.
        let rs_panel_bg = self.theme.ui.panel_bg.as_f32();
        let rs_input_bg = self.theme.editor.bg.as_f32();
        let right_panel_tab = self.panel_state.right.active_tab_id();
        let right_panel_uses_ai_chat_input = right_panel_tab == Some(PanelTabId::AiChat);
        let ai_chat_input_bounds = if right_panel_uses_ai_chat_input {
            flat_regions
                .iter()
                .find(|r| r.id == RegionId::AiChatInput && r.visible)
                .map(|r| [r.bounds.x, r.bounds.y, r.bounds.width, r.bounds.height])
        } else {
            None
        };

        let mut region_instances: Vec<RegionDrawInstance> = flat_regions
            .iter()
            .copied()
            .filter(|region| {
                region.visible
                    && region.id != RegionId::Root
                    && region.id != RegionId::OverlayLayer
                    && region.id != RegionId::StatusBar
                    && region.id != RegionId::AiChatHistory
                    && region.id != RegionId::AiChatInput
                    && !(show_welcome && !workspace_attached && region.id == RegionId::LeftSidebar)
            })
            .flat_map(|region| {
                let bounds = [
                    region.bounds.x,
                    region.bounds.y,
                    region.bounds.width,
                    region.bounds.height,
                ];
                let is_focused = Some(region.id) == focus_region;
                // When enable_outline is off, hide borders on unfocused panels only —
                // the focused panel keeps its ring so the user always knows where focus is.
                let suppress_ring = (!self.ui_config.enable_outline && !is_focused)
                    || region.id == RegionId::TopBar
                    || (show_welcome && region.id == RegionId::Center);

                if region.id == RegionId::RightSidebar {
                    if suppress_ring {
                        let mut quads = vec![
                            RegionDrawInstance::new(bounds, rs_panel_bg).with_radius(panel_radius),
                        ];
                        if let Some([ix, iy, iw, ih]) = ai_chat_input_bounds {
                            if iw > 0.0 && ih > 0.0 {
                                quads.push(
                                    RegionDrawInstance::new([ix, iy, iw, ih], rs_input_bg)
                                        .with_radius(panel_radius),
                                );
                            }
                        }
                        quads
                    } else {
                        let outline_color = if is_focused {
                            focused_outline
                        } else {
                            default_outline
                        };
                        let mut quads = focus_ring_instances(
                            bounds,
                            outline_color,
                            FOCUS_RING_THICKNESS,
                            panel_radius,
                            rs_panel_bg,
                        );
                        if let Some([ix, iy, iw, ih]) = ai_chat_input_bounds {
                            if iw > 0.0 && ih > 0.0 {
                                quads.push(
                                    RegionDrawInstance::new([ix, iy, iw, ih], rs_input_bg)
                                        .with_radius(
                                            (panel_radius - FOCUS_RING_THICKNESS).max(0.0),
                                        ),
                                );
                            }
                        }
                        quads
                    }
                } else if suppress_ring {
                    vec![
                        RegionDrawInstance::new(bounds, region_color(region.id, &self.theme))
                            .with_radius(panel_radius),
                    ]
                } else {
                    let outline_color = if is_focused {
                        focused_outline
                    } else {
                        default_outline
                    };
                    focus_ring_instances(
                        bounds,
                        outline_color,
                        FOCUS_RING_THICKNESS,
                        panel_radius,
                        region_color(region.id, &self.theme),
                    )
                }
            })
            .collect();

        if let Some(center_bounds) = center_bounds {
            let bounds_changed = self.last_editor_bounds != Some(center_bounds);
            let mut refresh_highlights_for_viewport = false;
            let active_terminal_session = self.app_state.active_terminal_session_id();
            let references_active = self.app_state.active_buffer_is_references();
            let diagnostics_active = self.app_state.active_buffer_is_diagnostics();
            let markdown_preview_active = self.app_state.active_buffer_is_markdown_preview();

            // ── Invariant guard ───────────────────────────────────────────────
            // Nếu terminal buffer đang chiếm center, đảm bảo editor GPU buffer
            // luôn được xóa trước khi render — bất kể nhánh nào xử lý frame này.
            // Điều này ngăn editor stale content "leak" qua khi chỉ có caret/terminal dirty.
            if active_terminal_session.is_some() {
                if let Some(renderer) = self.renderer.as_mut() {
                    renderer.clear_editor_content();
                }
                // Push solid terminal_bg background quad MỖI FRAME khi terminal active.
                // Không phụ thuộc vào dirty flag — đảm bảo background không bao giờ
                // biến mất giữa các frame (khi chỉ có gutter/caret dirty mà không push bg).
                region_instances.push(
                    RegionDrawInstance::new(center_bounds, self.theme.ui.terminal_bg.as_f32())
                        .with_radius(self.ui_config.border_radius_px),
                );
            }

            if self.editor_needs_layout || bounds_changed || show_welcome_changed || show_welcome {
                if let Some(renderer) = self.renderer.as_mut() {
                    if show_welcome {
                        renderer.set_editor_breadcrumb_segments(Vec::new());
                        let (text, styled) = welcome_screen_content(&self.theme);
                        renderer.clear_editor_content();
                        renderer.clear_buffer_terminal();
                        renderer.update_welcome_screen_content(
                            &text,
                            &styled,
                            center_bounds,
                            &self.persistent_state.recent_projects,
                            self.app_state.command_palette_selected_index(),
                        );
                    } else if let Some(session_id) = active_terminal_session {
                        renderer.set_editor_breadcrumb_segments(Vec::new());
                        renderer.clear_welcome_logo();
                        renderer.clear_editor_content();
                        if let Some(grid) = self.terminal_buffer_grids.get(&session_id) {
                            if let Some(bg_quad) = renderer.update_buffer_terminal_content(
                                grid,
                                center_bounds,
                                self.app_state.current_mode(),
                            ) {
                                region_instances.push(bg_quad);
                            }
                        } else {
                            renderer.clear_buffer_terminal();
                        }
                    } else if let Some(fuzzy_state) = self.app_state.active_fuzzy_picker_buffer() {
                        renderer.set_editor_breadcrumb_segments(Vec::new());
                        renderer.clear_welcome_logo();
                        renderer.clear_buffer_terminal();
                        renderer.clear_editor_content();
                        renderer.update_fuzzy_picker_buffer_content(fuzzy_state, center_bounds);
                    } else if diagnostics_active {
                        renderer.set_editor_breadcrumb_segments(Vec::new());
                        renderer.clear_welcome_logo();
                        renderer.clear_buffer_terminal();
                        renderer.clear_editor_content();
                        if let Some(diagnostics) = self.app_state.active_diagnostics_buffer() {
                            renderer.update_diagnostics_buffer_content(diagnostics, center_bounds);
                        } else {
                            renderer.clear_editor_overlays();
                        }
                    } else if markdown_preview_active {
                        renderer.set_editor_breadcrumb_segments(Vec::new());
                        renderer.clear_welcome_logo();
                        renderer.clear_buffer_terminal();
                        renderer.clear_editor_content();
                        if let Some(preview) = self.app_state.active_markdown_preview_buffer() {
                            let scroll_y = preview.scroll_y;
                            let rendered_lines = preview.rendered_lines.clone();
                            let inner_padding = self.layout_engine.config.inner_padding;
                            let max_scroll = renderer.update_markdown_preview_content(
                                center_bounds,
                                &rendered_lines,
                                scroll_y,
                                inner_padding,
                            );
                            self.app_state.markdown_preview.max_scroll = max_scroll;
                            if self.app_state.markdown_preview.scroll_y > max_scroll {
                                self.app_state.markdown_preview.scroll_y = max_scroll;
                            }
                            let preview_cloned = self.app_state.markdown_preview.clone();
                            self.app_state.sync_markdown_preview_buffer(preview_cloned);
                        } else {
                            renderer.clear_editor_overlays();
                        }
                    } else if references_active {
                        renderer.set_editor_breadcrumb_segments(Vec::new());
                        renderer.clear_welcome_logo();
                        renderer.clear_buffer_terminal();
                        renderer.clear_editor_content();
                        if let Some(references) = self.app_state.active_references_buffer() {
                            renderer.update_references_buffer_content(references, center_bounds);
                        } else {
                            renderer.clear_editor_overlays();
                        }
                    } else if let Some(settings) = self.app_state.active_settings_buffer() {
                        renderer.set_editor_breadcrumb_segments(Vec::new());
                        renderer.clear_welcome_logo();
                        renderer.clear_buffer_terminal();
                        renderer.clear_editor_content();
                        renderer.update_settings_buffer_content(settings, center_bounds);
                    } else if let Some(help) = self.app_state.active_help_buffer() {
                        renderer.set_editor_breadcrumb_segments(Vec::new());
                        renderer.clear_welcome_logo();
                        renderer.clear_buffer_terminal();
                        renderer.clear_editor_content();
                        let max_scroll_y = renderer.update_help_buffer_content(help, center_bounds);
                        self.app_state.set_help_max_scroll(max_scroll_y);
                    } else if let Some(extensions) =
                        self.app_state.active_extensions_manager_buffer()
                    {
                        renderer.set_editor_breadcrumb_segments(Vec::new());
                        renderer.clear_welcome_logo();
                        renderer.clear_buffer_terminal();
                        renderer.clear_editor_content();
                        renderer.update_extensions_manager_content(extensions, center_bounds);
                    } else if let Some(image) = self.app_state.active_image_buffer() {
                        renderer.set_editor_breadcrumb_segments(Vec::new());
                        renderer.update_image_content(image, center_bounds);
                    } else {
                        // Skip editor content rendering in Zen Mode when target is not CenterEditor
                        let zen_mode_active = self.panel_state.maximized_region.is_some()
                            && self.panel_state.maximized_region != Some(FocusTarget::CenterEditor);

                        if zen_mode_active {
                            renderer.set_editor_breadcrumb_segments(Vec::new());
                            renderer.clear_editor_content();
                            renderer.clear_editor_overlays();
                        } else {
                            renderer.clear_welcome_logo();
                            renderer.clear_buffer_terminal();
                            let (cursor_line, cursor_col) = self.app_state.cursor_line_col();
                            renderer.set_editor_breadcrumb_segments(
                                build_editor_breadcrumb_segments(
                                    &self.cached_document_symbols,
                                    cursor_line,
                                    cursor_col,
                                    &self.theme,
                                ),
                            );
                            let effective_highlights =
                                crate::syntax::highlight::overlay_highlight_layers(
                                    &self.highlight_spans,
                                    &self.semantic_highlight_spans,
                                );
                            let text = self.app_state.text_string();
                            let mut styled_spans =
                                syntax_spans_to_styled(&effective_highlights, &text, &self.theme);
                            styled_spans
                                .extend(diagnostic_spans_to_styled(&self.app_state, &self.theme));
                            renderer.update_editor_content(
                                &text,
                                &self.app_state,
                                center_bounds,
                                &styled_spans,
                            );
                            renderer.update_editor_overlays(&self.app_state, center_bounds);
                        }
                    }
                }
                self.last_editor_bounds = Some(center_bounds);
                self.last_buffer_terminal_bounds = Some(center_bounds);
                self.editor_needs_layout = false;
                self.editor_caret_needs_layout = false;
                self.buffer_terminal_needs_layout = false;
                // Cursor đã được render đúng vị trí → reset blink về visible.
                self.caret_blink_visible = true;
                self.caret_blink_dirty = true;
                self.last_caret_blink_tick = Instant::now();
                refresh_highlights_for_viewport =
                    bounds_changed && !show_welcome && active_terminal_session.is_none();
            } else if self.editor_caret_needs_layout {
                if let Some(session_id) = active_terminal_session {
                    // Terminal buffer đang active: xóa editor stale content và
                    // force re-render terminal qua main branch ở frame tiếp theo.
                    if let Some(renderer) = self.renderer.as_mut() {
                        renderer.clear_editor_content();
                    }
                    // Render ngay lập tức cho terminal thay vì đợi frame sau.
                    let grid_changed = self.sync_terminal_buffer_layout(session_id, center_bounds);
                    if let Some(grid) = self.terminal_buffer_grids.get(&session_id)
                        && let Some(renderer) = self.renderer.as_mut()
                    {
                        if self.buffer_terminal_needs_layout || bounds_changed || grid_changed {
                            if let Some(bg_quad) = renderer.update_buffer_terminal_content(
                                grid,
                                center_bounds,
                                self.app_state.current_mode(),
                            ) {
                                region_instances.push(bg_quad);
                            }
                            self.last_buffer_terminal_bounds = Some(center_bounds);
                            self.buffer_terminal_needs_layout = false;
                        }
                    }
                } else if markdown_preview_active {
                    if let Some(renderer) = self.renderer.as_mut() {
                        renderer.clear_editor_content();
                        if let Some(preview) = self.app_state.active_markdown_preview_buffer() {
                            let scroll_y = preview.scroll_y;
                            let rendered_lines = preview.rendered_lines.clone();
                            let inner_padding = self.layout_engine.config.inner_padding;
                            let max_scroll = renderer.update_markdown_preview_content(
                                center_bounds,
                                &rendered_lines,
                                scroll_y,
                                inner_padding,
                            );
                            self.app_state.markdown_preview.max_scroll = max_scroll;
                            if self.app_state.markdown_preview.scroll_y > max_scroll {
                                self.app_state.markdown_preview.scroll_y = max_scroll;
                            }
                            let preview_cloned = self.app_state.markdown_preview.clone();
                            self.app_state.sync_markdown_preview_buffer(preview_cloned);
                        } else {
                            renderer.clear_editor_overlays();
                        }
                    }
                } else if references_active {
                    if let Some(renderer) = self.renderer.as_mut() {
                        renderer.clear_editor_content();
                        if let Some(references) = self.app_state.active_references_buffer() {
                            renderer.update_references_buffer_content(references, center_bounds);
                        } else {
                            renderer.clear_editor_overlays();
                        }
                    }
                } else if diagnostics_active {
                    if let Some(renderer) = self.renderer.as_mut() {
                        renderer.clear_editor_content();
                        if let Some(diagnostics) = self.app_state.active_diagnostics_buffer() {
                            renderer.update_diagnostics_buffer_content(diagnostics, center_bounds);
                        } else {
                            renderer.clear_editor_overlays();
                        }
                    }
                } else if let Some(settings) = self.app_state.active_settings_buffer() {
                    if let Some(renderer) = self.renderer.as_mut() {
                        renderer.clear_editor_content();
                        renderer.update_settings_buffer_content(settings, center_bounds);
                    }
                } else if let Some(image) = self.app_state.active_image_buffer() {
                    if let Some(renderer) = self.renderer.as_mut() {
                        renderer.set_editor_breadcrumb_segments(Vec::new());
                        renderer.update_image_content(image, center_bounds);
                    }
                } else if !show_welcome
                    && (self.panel_state.maximized_region.is_none()
                        || self.panel_state.maximized_region == Some(FocusTarget::CenterEditor))
                    && let Some(renderer) = self.renderer.as_mut()
                {
                    let (cursor_line, cursor_col) = self.app_state.cursor_line_col();
                    let breadcrumb_viewport_changed =
                        renderer.set_editor_breadcrumb_segments(build_editor_breadcrumb_segments(
                            &self.cached_document_symbols,
                            cursor_line,
                            cursor_col,
                            &self.theme,
                        ));
                    if breadcrumb_viewport_changed {
                        let effective_highlights =
                            crate::syntax::highlight::overlay_highlight_layers(
                                &self.highlight_spans,
                                &self.semantic_highlight_spans,
                            );
                        let text = self.app_state.text_string();
                        let mut styled_spans =
                            syntax_spans_to_styled(&effective_highlights, &text, &self.theme);
                        styled_spans
                            .extend(diagnostic_spans_to_styled(&self.app_state, &self.theme));
                        renderer.update_editor_content(
                            &text,
                            &self.app_state,
                            center_bounds,
                            &styled_spans,
                        );
                    } else {
                        renderer.update_editor_caret(&self.app_state, center_bounds);
                    }
                    renderer.update_editor_overlays(&self.app_state, center_bounds);
                }
                self.editor_caret_needs_layout = false;
                // Cursor đã được re-projected → reset blink về visible.
                self.caret_blink_visible = true;
                self.caret_blink_dirty = true;
                self.last_caret_blink_tick = Instant::now();
            } else if let Some(session_id) = active_terminal_session {
                let grid_changed = self.sync_terminal_buffer_layout(session_id, center_bounds);
                if (self.buffer_terminal_needs_layout || bounds_changed || grid_changed)
                    && let Some(grid) = self.terminal_buffer_grids.get(&session_id)
                    && let Some(renderer) = self.renderer.as_mut()
                {
                    if let Some(bg_quad) = renderer.update_buffer_terminal_content(
                        grid,
                        center_bounds,
                        self.app_state.current_mode(),
                    ) {
                        region_instances.push(bg_quad);
                    }
                    self.last_buffer_terminal_bounds = Some(center_bounds);
                    self.buffer_terminal_needs_layout = false;
                }
            }

            if !show_welcome
                && active_terminal_session.is_none()
                && !references_active
                && !markdown_preview_active
                && let Some(renderer) = self.renderer.as_ref()
            {
                if self.app_state.current_mode() != EditorMode::Visual
                    && let Some(quad) =
                        renderer.current_line_highlight_quad(&self.app_state, center_bounds)
                {
                    region_instances.push(quad);
                }
                region_instances
                    .extend(renderer.indent_guide_quads(&self.app_state, center_bounds));
                region_instances
                    .extend(renderer.search_highlight_quads(&self.app_state, center_bounds));
                if self.app_state.current_mode() == EditorMode::Visual {
                    region_instances
                        .extend(renderer.visual_selection_quads(&self.app_state, center_bounds));
                }
                if matches!(
                    self.app_state.current_mode(),
                    EditorMode::MultiCursor | EditorMode::MultiInsert
                ) {
                    region_instances.extend(
                        renderer.multi_cursor_selection_quads(&self.app_state, center_bounds),
                    );
                }
            }

            if let Some(renderer) = self.renderer.as_mut() {
                if active_terminal_session.is_none()
                    && !references_active
                    && !markdown_preview_active
                    && !show_welcome
                {
                    if let Some(leap_state) = &self.leap_state {
                        renderer.update_editor_leap_labels(
                            &leap_state.targets,
                            &leap_state.typed_prefix,
                            &self.app_state,
                            center_bounds,
                        );
                    } else {
                        renderer.clear_leap_labels();
                    }
                } else {
                    renderer.clear_leap_labels();
                }
            }

            if let Some(renderer) = self.renderer.as_mut() {
                let show_diagnostic_hover = !show_welcome
                    && active_terminal_session.is_none()
                    && !references_active
                    && !diagnostics_active
                    && !markdown_preview_active
                    && !self.app_state.has_completion()
                    && self.app_state.active_fuzzy_picker_buffer().is_none()
                    && self.app_state.active_settings_buffer().is_none();
                if show_diagnostic_hover {
                    renderer.update_diagnostic_hover_popup(&self.app_state, center_bounds);
                } else {
                    renderer.clear_diagnostic_hover_popup();
                }
            }

            if refresh_highlights_for_viewport {
                self.submit_parse_for_active_buffer(true);
            }
        } else if let Some(renderer) = self.renderer.as_mut() {
            renderer.clear_welcome_logo();
            renderer.clear_buffer_terminal();
            renderer.clear_leap_labels();
            renderer.clear_diagnostic_hover_popup();
        }
        self.last_show_welcome = Some(show_welcome);

        let sidebar_filter_state = self.sidebar_filter_state();
        let sidebar_scroll_offset_rows = if let Some(bounds) = sidebar_bounds {
            if self.sync_explorer_scroll_to_selected(bounds) {
                self.sidebar_needs_layout = true;
            }
            self.app_state
                .workspace_scroll_offset_rows(self.theme.ui.sidebar_line_height.max(1.0))
        } else {
            0
        };
        let sidebar_rows = build_sidebar_rows(
            &self.explorer_snapshot.entries,
            self.explorer_cursor,
            &self.theme,
            self.app_state.workspace_has_active_filter(),
            sidebar_scroll_offset_rows,
        );
        if let Some(renderer) = self.renderer.as_mut() {
            if let Some(bounds) = sidebar_bounds {
                let sidebar_focused = self.focus_manager.current() == FocusTarget::LeftSidebar;
                let bounds_changed = self.last_sidebar_bounds != Some(bounds);
                let focus_changed = self.last_sidebar_focused != Some(sidebar_focused);
                if self.sidebar_needs_layout || bounds_changed || focus_changed {
                    self.sidebar_selection_quads = renderer.update_sidebar_content(
                        None,
                        &sidebar_rows,
                        bounds,
                        sidebar_focused,
                        sidebar_filter_state.as_ref(),
                    );
                    self.last_sidebar_bounds = Some(bounds);
                    self.last_sidebar_focused = Some(sidebar_focused);
                    self.sidebar_needs_layout = false;
                }
                region_instances.extend(self.sidebar_selection_quads.iter().copied());
            } else if self.last_sidebar_bounds.is_some() {
                renderer.clear_sidebar();
                self.last_sidebar_bounds = None;
                self.last_sidebar_focused = None;
                self.sidebar_selection_quads.clear();
            }
        }

        // ── AI Chat text (right sidebar) ──────────────────────────────────
        let right_sidebar_bounds = visible_region_bounds(&flat_regions, RegionId::RightSidebar);
        let ai_chat_active = self.panel_state.right.visible
            && right_sidebar_bounds.is_some()
            && self.panel_state.right.active_tab_id() == Some(PanelTabId::AiChat);
        let markdown_preview_uses_ai_chat_pipeline =
            self.app_state.active_buffer_is_markdown_preview()
                || (self.panel_state.right.visible
                    && right_sidebar_bounds.is_some()
                    && self.panel_state.right.active_tab_id() == Some(PanelTabId::MarkdownPreview)
                    && self.app_state.markdown_preview.visible);
        if ai_chat_active {
            let history_bounds = visible_region_bounds(&flat_regions, RegionId::AiChatHistory);
            let input_bounds = visible_region_bounds(&flat_regions, RegionId::AiChatInput);

            if let (Some(hb), Some(ib)) = (history_bounds, input_bounds) {
                let chat = &self.panel_state.ai_chat;
                let file_suggestions = self.ai_chat_file_reference_suggestions(&chat.input_buffer);
                if let Some(renderer) = self.renderer.as_mut() {
                    let show_cursor = self.focus_manager.current() == FocusTarget::RightSidebar;
                    let inner_padding = self.layout_engine.config.inner_padding;
                    let scroll_y = chat.scroll_y;
                    let (cursor_quads, max_scroll_y) = renderer.update_ai_chat_content(
                        hb,
                        ib,
                        &chat.messages,
                        &chat.input_buffer,
                        &file_suggestions,
                        chat.selected_suggestion_index,
                        show_cursor,
                        inner_padding,
                        chat.is_opencode_missing,
                        chat.model.as_deref(),
                        chat.agent.label(),
                        chat.is_generating,
                        scroll_y,
                    );
                    self.panel_state.ai_chat.max_scroll_y = max_scroll_y;
                    region_instances.extend(cursor_quads);
                }
            }
        } else if !markdown_preview_uses_ai_chat_pipeline
            && let Some(renderer) = self.renderer.as_mut()
        {
            renderer.clear_ai_chat();
        }

        // ── Right-sidebar terminal ────────────────────────────────────────
        let right_terminal_active = self.panel_state.right.visible
            && right_sidebar_bounds.is_some()
            && self.panel_state.right.active_tab_id() == Some(PanelTabId::Terminal);
        if right_terminal_active {
            if let Some(rb) = right_sidebar_bounds {
                let bounds_changed = self.last_right_terminal_bounds != Some(rb);
                let grid_changed = self.sync_right_terminal_layout(rb);
                if (self.right_terminal_needs_layout || bounds_changed || grid_changed)
                    && let Some(renderer) = self.renderer.as_mut()
                {
                    renderer.update_right_terminal_content(
                        &self.right_terminal_grid,
                        rb,
                        self.app_state.current_mode(),
                    );
                    self.last_right_terminal_bounds = Some(rb);
                    self.right_terminal_needs_layout = false;
                }
            }
        } else if self.last_right_terminal_bounds.is_some() {
            self.last_right_terminal_bounds = None;
            if let Some(renderer) = self.renderer.as_mut() {
                renderer.clear_right_terminal();
            }
        }

        // ── Markdown Preview (right sidebar) ──────────────────────────────
        let md_preview_active = self.panel_state.right.visible
            && right_sidebar_bounds.is_some()
            && self.panel_state.right.active_tab_id() == Some(PanelTabId::MarkdownPreview);
        if md_preview_active && self.app_state.markdown_preview.visible {
            if let Some(bounds) = right_sidebar_bounds {
                let preview = &self.app_state.markdown_preview;
                let inner_padding = self.layout_engine.config.inner_padding;
                let scroll_y = preview.scroll_y;
                let rendered_lines = preview.rendered_lines.clone();
                if let Some(renderer) = self.renderer.as_mut() {
                    let max_scroll = renderer.update_markdown_preview_content(
                        bounds,
                        &rendered_lines,
                        scroll_y,
                        inner_padding,
                    );
                    self.app_state.markdown_preview.max_scroll = max_scroll;
                    if self.app_state.markdown_preview.scroll_y > max_scroll {
                        self.app_state.markdown_preview.scroll_y = max_scroll;
                    }
                    let preview_cloned = self.app_state.markdown_preview.clone();
                    self.app_state.sync_markdown_preview_buffer(preview_cloned);
                }
            }
        } else if md_preview_active {
            if let Some(renderer) = self.renderer.as_mut() {
                renderer.clear_ai_chat();
            }
        }

        if let Some(renderer) = self.renderer.as_ref() {
            region_instances.extend(renderer.editor_chrome_instances().iter().copied());
        }

        if let Some(top) = layout.model.find(RegionId::TopBar) {
            let top_bounds = [top.x, top.y, top.width, top.height];
            let tabs = self
                .app_state
                .buffers()
                .iter()
                .enumerate()
                .map(|(idx, buffer)| {
                    let (file_path, git_color, missing_on_disk) = match &buffer.content {
                        BufferContent::Text(text) => {
                            let status = self.app_state.workspace_git_status(&text.path);
                            let color = match status {
                                Some(WorkspaceGitStatus::Modified) => {
                                    Some(self.theme.git.modified_sidebar.as_f32())
                                }
                                Some(WorkspaceGitStatus::Added) => {
                                    Some(self.theme.git.added_sidebar.as_f32())
                                }
                                Some(WorkspaceGitStatus::Dirty) => {
                                    Some(self.theme.git.modified_sidebar.as_f32())
                                }
                                None => None,
                            };
                            (text.path.clone(), color, text.missing_on_disk)
                        }
                        _ => (PathBuf::new(), None, false),
                    };
                    TopbarTab {
                        label: buffer.label(),
                        kind: match &buffer.content {
                            BufferContent::Text(_text) => TopbarTabKind::Text { path: file_path },
                            BufferContent::Image(image) => TopbarTabKind::Image {
                                path: image.path.clone(),
                            },
                            BufferContent::Terminal(_) => TopbarTabKind::Terminal,
                            BufferContent::References(_) => TopbarTabKind::References,
                            BufferContent::Diagnostics(_) => TopbarTabKind::Diagnostics,
                            BufferContent::MarkdownPreview(_) => TopbarTabKind::MarkdownPreview,
                            BufferContent::FuzzyPicker(_) => TopbarTabKind::FuzzyPicker,
                            BufferContent::SettingsTab(_) => TopbarTabKind::Settings,
                            BufferContent::Help(_) => TopbarTabKind::Help,
                            BufferContent::ExtensionsManager(_) => TopbarTabKind::ExtensionsManager,
                        },
                        is_dirty: buffer.is_dirty(
                            self.app_state.active_buffer_index() == Some(idx),
                            self.app_state.is_dirty(),
                        ),
                        git_color,
                        missing_on_disk,
                    }
                })
                .collect::<Vec<_>>();
            if let Some(renderer) = self.renderer.as_mut() {
                let project_name = if show_welcome {
                    ""
                } else {
                    self.app_state
                        .workspace_root_path()
                        .and_then(|root| root.file_name().and_then(|name| name.to_str()))
                        .unwrap_or("")
                };
                let center_x = visible_region_bounds(&flat_regions, RegionId::Center)
                    .map(|b| b[0])
                    .unwrap_or(0.0);
                let tab_quads = renderer.update_topbar_content(
                    &tabs,
                    self.app_state.active_buffer_index(),
                    project_name,
                    center_x,
                    top_bounds,
                );
                region_instances.extend(tab_quads);
            }
        }

        if let Some(status) = layout.model.find(RegionId::StatusBar) {
            let status_bounds = [0.0, status.y, self.window_size.width as f32, status.height];
            let mode = self.app_state.current_mode();
            let (line, col) = self.app_state.cursor_line_col();
            let pending_keys = self.input_handler.get_pending_keys();
            let (
                filetype,
                git_branch,
                is_dirty,
                active_file_name,
                status_line,
                status_col,
                diagnostics_errors,
                diagnostics_warnings,
            ) = if show_welcome {
                ("Welcome", "", false, String::new(), 0, 0, 0, 0)
            } else {
                let filetype = self.app_state.active_filetype_label();
                let git_branch = self.workspace_git_branch.as_deref().unwrap_or("-");
                let is_dirty = self.app_state.is_dirty();
                let active_file_name = statusbar_source_path_label(
                    self.app_state.active_file(),
                    self.app_state.workspace_root_path(),
                );
                let (diagnostics_errors, diagnostics_warnings) = self
                    .app_state
                    .active_file()
                    .and_then(|path| self.app_state.diagnostics_for_path(path))
                    .map(|items| {
                        items
                            .iter()
                            .fold((0usize, 0usize), |(e, w), item| match item.severity {
                                Some(1) => (e + 1, w),
                                Some(2) => (e, w + 1),
                                _ => (e, w),
                            })
                    })
                    .unwrap_or((0, 0));
                (
                    filetype,
                    git_branch,
                    is_dirty,
                    active_file_name,
                    line,
                    col,
                    diagnostics_errors,
                    diagnostics_warnings,
                )
            };
            if let Some(renderer) = self.renderer.as_mut() {
                let lsp_progress_label = self
                    .app_state
                    .lsp_progress()
                    .map(|entry| entry.status_label())
                    .or_else(|| {
                        let active_path = self.app_state.active_file()?;
                        let profile = crate::lsp::registry::language_profile_for_path(active_path)?;
                        self.app_state
                            .workspace_symbol_cache()
                            .is_indexing(profile.key)
                            .then(|| "Indexing symbols…".to_string())
                    });
                let lsp_indicator = {
                    use crate::render::renderer::LspStatusIndicator;
                    let active_profile = (!show_welcome)
                        .then(|| {
                            self.app_state
                                .active_file()
                                .and_then(crate::lsp::registry::language_profile_for_path)
                        })
                        .flatten();
                    match active_profile {
                        None => LspStatusIndicator::NotApplicable,
                        Some(profile) => {
                            if let Some(pending) = &self.pending_lsp_server {
                                LspStatusIndicator::Starting(pending.server_name.clone())
                            } else if let Some(guide) = &self.active_lsp_guide {
                                LspStatusIndicator::Missing(guide.binary.clone())
                            } else if let Some(active) = self
                                .active_lsp_server
                                .as_ref()
                                .filter(|active| active.server_name == profile.lsp_binary)
                            {
                                LspStatusIndicator::Running(active.server_name.clone())
                            } else {
                                LspStatusIndicator::Inactive
                            }
                        }
                    }
                };
                let ai_status = if show_welcome {
                    None
                } else {
                    self.ai_config.inline_completion().map(|_| {
                        let cooling_down = self
                            .ai_inline_cooldown_until
                            .is_some_and(|until| std::time::Instant::now() < until);
                        if cooling_down {
                            crate::render::renderer::AiInlineStatus::Error
                        } else if self.ai_inline_inflight {
                            crate::render::renderer::AiInlineStatus::Loading
                        } else {
                            crate::render::renderer::AiInlineStatus::Ready
                        }
                    })
                };
                let pill_quads = renderer.update_statusbar_content(
                    mode,
                    &pending_keys,
                    git_branch,
                    is_dirty,
                    &active_file_name,
                    filetype,
                    self.app_state.active_search_match_position(),
                    status_line,
                    status_col,
                    diagnostics_errors,
                    diagnostics_warnings,
                    self.pending_lsp_server.is_some(),
                    self.lsp_loading_frame,
                    lsp_progress_label.as_deref(),
                    &lsp_indicator,
                    ai_status,
                    self.runtime_versions.venv_name.as_deref(),
                    self.runtime_versions.python_version.as_deref(),
                    self.runtime_versions.node_version.as_deref(),
                    self.runtime_versions.go_version.as_deref(),
                    status_bounds,
                );
                region_instances.extend(pill_quads);
            }
        }

        let welcome_recent_projects_active = show_welcome
            && self.app_state.command_palette_mode() == Some(CommandPaletteMode::RecentProjects);

        if self.app_state.is_command_palette_visible() && !welcome_recent_projects_active {
            let overlay_bounds = [
                0.0,
                0.0,
                self.window_size.width as f32,
                self.window_size.height as f32,
            ];
            if let Some(model) = self
                .app_state
                .command_palette_render_model(&self.theme, overlay_bounds)
                && let Some(renderer) = self.renderer.as_mut()
            {
                renderer.update_palette_content(&model);
            }
        } else if let Some(renderer) = self.renderer.as_mut() {
            renderer.clear_palette();
        }

        if let Some(bottom) = bottom_region {
            // Ẩn bottom panel terminal khi center đang hiển thị terminal buffer
            // (lazygit, v.v.) để tránh render terminal hai nơi đồng thời.
            let center_has_terminal = self.app_state.active_terminal_session_id().is_some();
            // In Zen Mode, only render terminal when BottomPanel is the target.
            let zen_allows_terminal = self.panel_state.maximized_region.is_none()
                || self.panel_state.maximized_region == Some(FocusTarget::BottomPanel);
            if bottom.visible && !show_welcome && !center_has_terminal && zen_allows_terminal {
                let bottom_bounds = [
                    bottom.bounds.x,
                    bottom.bounds.y,
                    bottom.bounds.width,
                    bottom.bounds.height,
                ];

                let terminal_content_bounds = self
                    .renderer
                    .as_ref()
                    .map(|renderer| {
                        renderer.terminal_tab_bar_content_bounds(
                            bottom_bounds,
                            self.terminal_tabs.len(),
                        )
                    })
                    .unwrap_or(bottom_bounds);

                let bounds_changed = self.last_terminal_bounds != Some(bottom_bounds);
                let grid_changed = self.sync_terminal_layout(terminal_content_bounds);
                if (self.terminal_needs_layout || bounds_changed || grid_changed)
                    && let Some(renderer) = self.renderer.as_mut()
                {
                    renderer.update_terminal_content(
                        &self.terminal_tabs[self.active_terminal_tab].grid,
                        terminal_content_bounds,
                        self.app_state.current_mode(),
                    );
                    self.last_terminal_bounds = Some(bottom_bounds);
                    self.terminal_needs_layout = false;
                }

                // Render tab bar after terminal content layout so tab labels are
                // appended after body glyphs in the shared terminal text pipeline.
                let tab_bar_quads = if let Some(renderer) = self.renderer.as_mut() {
                    let labels: Vec<&str> = self
                        .terminal_tabs
                        .iter()
                        .map(|t| t.label.as_str())
                        .collect();
                    let running: Vec<bool> = self
                        .terminal_tabs
                        .iter()
                        .map(|t| t.status.is_running())
                        .collect();
                    renderer
                        .update_terminal_tab_bar(
                            &labels,
                            &running,
                            self.active_terminal_tab,
                            bottom_bounds,
                        )
                        .0
                } else {
                    Vec::new()
                };
                region_instances.extend(tab_bar_quads);
            } else if self.last_terminal_bounds.is_some() {
                if let Some(renderer) = self.renderer.as_mut() {
                    renderer.clear_terminal();
                }
                self.last_terminal_bounds = None;
            }
        }

        // ── Which-key overlay: pending chord continuations ───────────────────
        let whichkey_model = self
            .input_handler
            .pending_chord_sequence()
            .filter(|(_, started_at)| started_at.elapsed() >= WHICHKEY_DELAY)
            .map(|(sequence, _)| {
                (
                    self.input_handler.get_pending_keys(),
                    self.input_map
                        .whichkey_entries(sequence, self.build_context()),
                )
            })
            .filter(|(_, entries)| !entries.is_empty());
        if let Some((prefix_label, entries)) = whichkey_model {
            let statusbar_h = layout
                .model
                .find(RegionId::StatusBar)
                .map(|region| region.height)
                .unwrap_or(0.0);
            if let Some(renderer) = self.renderer.as_mut() {
                let w = self.window_size.width as f32;
                let h = self.window_size.height as f32;
                renderer.update_whichkey_popup(&prefix_label, &entries, w, h, statusbar_h);
            }
        } else if let Some(renderer) = self.renderer.as_mut() {
            renderer.clear_whichkey_popup();
        }

        // ── LSP Install Guide Popup (always on top) ─────────────────────────
        // Render sau tất cả overlay khác để popup nổi lên trên cùng.
        if let Some(guide) = self.active_lsp_guide.clone()
            && let Some(renderer) = self.renderer.as_mut()
        {
            let w = self.window_size.width as f32;
            let h = self.window_size.height as f32;
            renderer.update_lsp_guide_popup(&guide.binary, &guide.install_cmd, w, h);
        } else if let Some(renderer) = self.renderer.as_mut() {
            renderer.clear_lsp_guide_popup();
        }

        // ── System Dependency Check Popup ────────────────────────────────────
        if let Some(ref guide) = self.active_system_dep_guide
            && let Some(renderer) = self.renderer.as_mut()
        {
            let w = self.window_size.width as f32;
            let h = self.window_size.height as f32;
            renderer.update_system_dep_popup(guide, w, h);
        } else if let Some(renderer) = self.renderer.as_mut() {
            renderer.clear_system_dep_popup();
        }

        if let Some(toast) = self.transient_toast.clone()
            && let Some(renderer) = self.renderer.as_mut()
        {
            let w = self.window_size.width as f32;
            let h = self.window_size.height as f32;
            renderer.update_toast_popup(
                &toast.message,
                toast.kind,
                toast.progress_fraction(Instant::now()),
                w,
                h,
            );
        } else if let Some(renderer) = self.renderer.as_mut() {
            renderer.clear_toast_popup();
        }

        // ── Tối ưu 3: Caret Blink ────────────────────────────────────────────
        // Nếu chỉ blink dirty (không có layout rebuild nào), flip caret visibility
        // mà không trigger bất kỳ text pipeline hay glyph rebuild nào.
        // Khi editor_needs_layout hoặc editor_caret_needs_layout đã chạy ở trên,
        // caret đã được upload đúng vị trí → reset blink về visible.
        if self.caret_blink_dirty {
            if let Some(renderer) = self.renderer.as_mut() {
                renderer.update_caret_visibility(self.caret_blink_visible);
            }
            self.caret_blink_dirty = false;
        }

        if let Some(renderer) = self.renderer.as_mut() {
            match renderer.render(&region_instances) {
                Ok(()) => {
                    self.last_frame_time = Instant::now();
                }
                Err(RenderError::Outdated) | Err(RenderError::Lost) => {
                    renderer.reconfigure_surface();
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

    pub(super) fn handle_pending_confirmation_key_event(
        &mut self,
        key_event: &winit::event::KeyEvent,
    ) -> Option<bool> {
        if self.pending_confirmation.is_none() {
            return None;
        }
        if key_event.state != ElementState::Pressed || key_event.repeat {
            return Some(false);
        }

        let Some(decision) = (match key_event.logical_key.as_ref() {
            Key::Named(NamedKey::Escape) => Some(false),
            Key::Character(text) if text.eq_ignore_ascii_case("y") => Some(true),
            Key::Character(text) if text.eq_ignore_ascii_case("n") => Some(false),
            _ => None,
        }) else {
            return Some(false);
        };

        Some(self.respond_to_pending_confirmation(decision))
    }
}

fn focus_target_region_id(target: FocusTarget) -> Option<RegionId> {
    match target {
        FocusTarget::CenterEditor => Some(RegionId::Center),
        FocusTarget::LeftSidebar => Some(RegionId::LeftSidebar),
        FocusTarget::RightSidebar => Some(RegionId::RightSidebar),
        FocusTarget::BottomPanel => Some(RegionId::BottomPanel),
        FocusTarget::TopBar => Some(RegionId::TopBar),
        FocusTarget::StatusBar => Some(RegionId::StatusBar),
        FocusTarget::OverlayLayer => None,
    }
}

fn focus_ring_instances(
    bounds: [f32; 4],
    mut color: [f32; 4],
    thickness: f32,
    border_radius: f32,
    inner_fill: [f32; 4],
) -> Vec<RegionDrawInstance> {
    let [x, y, w, h] = bounds;
    if w <= 0.0 || h <= 0.0 {
        return Vec::new();
    }

    let t = thickness.max(1.0).min(w * 0.5).min(h * 0.5);
    color[3] = color[3].max(0.9);

    let inner_x = x + t;
    let inner_y = y + t;
    let inner_w = (w - t * 2.0).max(0.0);
    let inner_h = (h - t * 2.0).max(0.0);

    let mut instances =
        vec![RegionDrawInstance::new([x, y, w, h], color).with_radius(border_radius)];
    if inner_w > 0.0 && inner_h > 0.0 {
        instances.push(
            RegionDrawInstance::new([inner_x, inner_y, inner_w, inner_h], inner_fill)
                .with_radius((border_radius - t).max(0.0)),
        );
    }
    instances
}

#[cfg(test)]
mod tests {
    use super::{
        breadcrumb_segment_text, build_editor_breadcrumb_segments, focus_ring_instances,
        focus_target_region_id, mouse_button_code, point_in_bounds, sgr_mouse_sequence,
        sgr_wheel_sequence,
        statusbar_source_path_label, terminal_cell_at_position, visible_region_bounds,
    };
    use crate::async_runtime::message::{
        LspDocumentSymbol, LspDocumentSymbolSegment, LspPosition, LspRange,
    };
    use crate::config::theme_config::ThemeConfig;
    use crate::workbench::{
        focus_manager::FocusTarget,
        region_model::{FlatRegion, RegionBounds, RegionId},
    };
    use std::path::Path;
    use winit::event::MouseButton;

    #[test]
    fn focus_target_region_id_maps_center_editor() {
        assert_eq!(
            focus_target_region_id(FocusTarget::CenterEditor),
            Some(RegionId::Center)
        );
        assert_eq!(focus_target_region_id(FocusTarget::OverlayLayer), None);
    }

    #[test]
    fn focus_ring_keeps_outline_and_panel_fill() {
        let instances = focus_ring_instances(
            [12.0, 24.0, 320.0, 180.0],
            [0.7, 0.3, 1.0, 1.0],
            3.0,
            10.0,
            [0.08, 0.08, 0.1, 1.0],
        );

        assert_eq!(
            instances.len(),
            2,
            "panel regions should still render both outline and fill"
        );
    }

    #[test]
    fn statusbar_source_path_label_prefers_workspace_relative_path() {
        let root = Path::new("/tmp/demo");
        let file = Path::new("/tmp/demo/src/app/main.rs");

        assert_eq!(
            statusbar_source_path_label(Some(file), Some(root)),
            "src/app/main.rs"
        );
    }

    #[test]
    fn statusbar_source_path_label_falls_back_to_filename_outside_workspace() {
        let root = Path::new("/tmp/demo");
        let file = Path::new("/tmp/other/main.rs");

        assert_eq!(
            statusbar_source_path_label(Some(file), Some(root)),
            "main.rs"
        );
    }

    #[test]
    fn visible_region_bounds_ignores_hidden_regions() {
        let regions = vec![
            FlatRegion {
                id: RegionId::RightSidebar,
                bounds: RegionBounds::new(10.0, 20.0, 300.0, 400.0),
                visible: false,
            },
            FlatRegion {
                id: RegionId::Center,
                bounds: RegionBounds::new(0.0, 0.0, 800.0, 600.0),
                visible: true,
            },
        ];

        assert_eq!(
            visible_region_bounds(&regions, RegionId::RightSidebar),
            None
        );
        assert_eq!(
            visible_region_bounds(&regions, RegionId::Center),
            Some([0.0, 0.0, 800.0, 600.0])
        );
    }

    #[test]
    fn terminal_cell_at_position_returns_one_based_cell_inside_content() {
        let cell = terminal_cell_at_position(
            (114.0, 73.0),
            [100.0, 50.0, 300.0, 200.0],
            20,
            10,
            10.0,
            14.0,
            20.0,
        );

        assert_eq!(cell, Some((1, 1)));
        assert_eq!(
            terminal_cell_at_position(
                (90.0, 73.0),
                [100.0, 50.0, 300.0, 200.0],
                20,
                10,
                10.0,
                14.0,
                20.0,
            ),
            None
        );
    }

    #[test]
    fn sgr_mouse_sequence_uses_press_and_release_suffixes() {
        assert_eq!(mouse_button_code(MouseButton::Left), Some(0));
        assert!(point_in_bounds((12.0, 14.0), [10.0, 10.0, 20.0, 20.0]));
        assert_eq!(sgr_mouse_sequence(0, 3, 4, true), "\x1b[<0;3;4M");
        assert_eq!(sgr_mouse_sequence(0, 3, 4, false), "\x1b[<0;3;4m");
    }

    #[test]
    fn sgr_wheel_sequence_is_press_only_with_wheel_button_codes() {
        assert_eq!(sgr_wheel_sequence(true, 5, 7), "\x1b[<64;5;7M");
        assert_eq!(sgr_wheel_sequence(false, 5, 7), "\x1b[<65;5;7M");
    }

    #[test]
    fn build_editor_breadcrumb_segments_uses_deepest_matching_symbol() {
        let theme = ThemeConfig::builtin_dark();
        let symbols = vec![
            LspDocumentSymbol {
                name: "SubscribeLogSync".to_string(),
                kind: "Class".to_string(),
                range: LspRange {
                    start: LspPosition {
                        line: 10,
                        character: 0,
                    },
                    end: LspPosition {
                        line: 80,
                        character: 1,
                    },
                },
                ancestors: Vec::new(),
            },
            LspDocumentSymbol {
                name: "stop".to_string(),
                kind: "Method".to_string(),
                range: LspRange {
                    start: LspPosition {
                        line: 24,
                        character: 2,
                    },
                    end: LspPosition {
                        line: 40,
                        character: 1,
                    },
                },
                ancestors: vec![LspDocumentSymbolSegment {
                    name: "SubscribeLogSync".to_string(),
                    kind: "Class".to_string(),
                }],
            },
            LspDocumentSymbol {
                name: "intervalId".to_string(),
                kind: "Constant".to_string(),
                range: LspRange {
                    start: LspPosition {
                        line: 25,
                        character: 4,
                    },
                    end: LspPosition {
                        line: 25,
                        character: 22,
                    },
                },
                ancestors: vec![
                    LspDocumentSymbolSegment {
                        name: "SubscribeLogSync".to_string(),
                        kind: "Class".to_string(),
                    },
                    LspDocumentSymbolSegment {
                        name: "stop".to_string(),
                        kind: "Method".to_string(),
                    },
                ],
            },
        ];

        let breadcrumb = build_editor_breadcrumb_segments(&symbols, 25, 10, &theme);
        let labels: Vec<&str> = breadcrumb
            .iter()
            .map(|segment| segment.text.as_str())
            .collect();
        assert_eq!(
            labels,
            vec![
                "[sym] class SubscribeLogSync",
                "[sym] method stop",
                "[sym] const intervalId"
            ]
        );
    }

    #[test]
    fn build_editor_breadcrumb_segments_renders_constructor_chain() {
        let theme = ThemeConfig::builtin_dark();
        let symbols = vec![
            LspDocumentSymbol {
                name: "KafkaProducer".to_string(),
                kind: "Class".to_string(),
                range: LspRange {
                    start: LspPosition {
                        line: 6,
                        character: 0,
                    },
                    end: LspPosition {
                        line: 30,
                        character: 1,
                    },
                },
                ancestors: Vec::new(),
            },
            LspDocumentSymbol {
                name: "constructor".to_string(),
                kind: "Constructor".to_string(),
                range: LspRange {
                    start: LspPosition {
                        line: 12,
                        character: 2,
                    },
                    end: LspPosition {
                        line: 22,
                        character: 3,
                    },
                },
                ancestors: vec![LspDocumentSymbolSegment {
                    name: "KafkaProducer".to_string(),
                    kind: "Class".to_string(),
                }],
            },
            LspDocumentSymbol {
                name: "kafkaClient".to_string(),
                kind: "Constant".to_string(),
                range: LspRange {
                    start: LspPosition {
                        line: 14,
                        character: 8,
                    },
                    end: LspPosition {
                        line: 14,
                        character: 30,
                    },
                },
                ancestors: vec![
                    LspDocumentSymbolSegment {
                        name: "KafkaProducer".to_string(),
                        kind: "Class".to_string(),
                    },
                    LspDocumentSymbolSegment {
                        name: "constructor".to_string(),
                        kind: "Constructor".to_string(),
                    },
                ],
            },
        ];

        let breadcrumb = build_editor_breadcrumb_segments(&symbols, 14, 12, &theme);
        let labels: Vec<&str> = breadcrumb
            .iter()
            .map(|segment| segment.text.as_str())
            .collect();
        assert_eq!(
            labels,
            vec![
                "[sym] class KafkaProducer",
                "[sym] constructor",
                "[sym] const kafkaClient"
            ]
        );
    }

    #[test]
    fn breadcrumb_segment_text_dedupes_constructor_label() {
        assert_eq!(
            breadcrumb_segment_text("Constructor", "constructor"),
            "[sym] constructor"
        );
    }
}
