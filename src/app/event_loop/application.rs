use super::*;
use winit::{
    event::ElementState,
    keyboard::{Key, NamedKey},
};

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
                    let overlay_cleared = self.invalidate_editor_overlays();
                    if overlay_cleared {
                        self.request_redraw();
                    }
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
                if self.active_system_dep_guide.is_some() && key_event.state == ElementState::Pressed {
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
                    Some(InputRouteOutcome::NoDispatch { .. }) | None => {}
                }
            }
            WindowEvent::MouseWheel { .. } => {
                if self.invalidate_editor_overlays() {
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
        self.flush_pending_ai_inline_completion();
        self.flush_pending_lsp_did_change_after_debounce();
        if self.maybe_refresh_workspace_git_branch(false) {
            self.request_redraw();
        }
        if self.tick_smooth_scroll_animation() {
            self.request_redraw();
        }
        if self.tick_thinking_animation() {
            self.request_redraw();
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
        if self.app_state.workspace_is_inputting_filter() {
            self.sidebar_needs_layout = true;
            self.request_redraw();
        }

        let mut next_deadline = Some(self.next_git_branch_refresh_deadline());
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
    fn tick_smooth_scroll_animation(&mut self) -> bool {
        let now = Instant::now();
        let dt = now
            .saturating_duration_since(self.last_scroll_animation_tick)
            .as_secs_f32();
        self.last_scroll_animation_tick = now;

        let target = self.app_state.target_scroll_y;
        let current = self.app_state.current_scroll_y;
        let epsilon = self.ui_config.editor.smooth_scroll_snap_epsilon;
        let delta = target - current;
        if delta.abs() <= epsilon {
            if delta != 0.0 {
                self.app_state.current_scroll_y = target;
                self.editor_needs_layout = true;
            }
            return false;
        }

        if !self.ui_config.editor.smooth_scroll_enabled {
            self.app_state.current_scroll_y = target;
            self.editor_needs_layout = true;
            return false;
        }

        let rate = self.ui_config.editor.smooth_scroll_lerp_rate;
        let alpha = (1.0 - (-rate * dt.max(0.0)).exp()).clamp(0.0, 1.0);
        self.app_state.current_scroll_y = current + delta * alpha;
        if (target - self.app_state.current_scroll_y).abs() <= epsilon {
            self.app_state.current_scroll_y = target;
        }
        self.editor_needs_layout = true;
        (target - self.app_state.current_scroll_y).abs() > epsilon
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
        let ai_chat_input_bounds = flat_regions
            .iter()
            .find(|r| r.id == RegionId::AiChatInput && r.visible)
            .map(|r| [r.bounds.x, r.bounds.y, r.bounds.width, r.bounds.height]);

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
                    || region.id == RegionId::TopBar;

                if region.id == RegionId::RightSidebar {
                    if suppress_ring {
                        let mut quads = vec![
                            RegionDrawInstance::new(bounds, rs_panel_bg)
                                .with_radius(panel_radius),
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
                        let outline_color =
                            if is_focused { focused_outline } else { default_outline };
                        let mut quads =
                            focus_ring_instances(bounds, outline_color, 3.0, panel_radius, rs_panel_bg);
                        if let Some([ix, iy, iw, ih]) = ai_chat_input_bounds {
                            if iw > 0.0 && ih > 0.0 {
                                quads.push(
                                    RegionDrawInstance::new([ix, iy, iw, ih], rs_input_bg)
                                        .with_radius((panel_radius - 3.0).max(0.0)),
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
                    let outline_color =
                        if is_focused { focused_outline } else { default_outline };
                    focus_ring_instances(
                        bounds,
                        outline_color,
                        3.0,
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
                        renderer.clear_welcome_logo();
                        renderer.clear_buffer_terminal();
                        renderer.clear_editor_content();
                        renderer.update_fuzzy_picker_buffer_content(fuzzy_state, center_bounds);
                    } else if diagnostics_active {
                        renderer.clear_welcome_logo();
                        renderer.clear_buffer_terminal();
                        renderer.clear_editor_content();
                        if let Some(diagnostics) = self.app_state.active_diagnostics_buffer() {
                            renderer.update_diagnostics_buffer_content(diagnostics, center_bounds);
                        } else {
                            renderer.clear_editor_overlays();
                        }
                    } else if references_active {
                        renderer.clear_welcome_logo();
                        renderer.clear_buffer_terminal();
                        renderer.clear_editor_content();
                        if let Some(references) = self.app_state.active_references_buffer() {
                            renderer.update_references_buffer_content(references, center_bounds);
                        } else {
                            renderer.clear_editor_overlays();
                        }
                    } else if let Some(settings) = self.app_state.active_settings_buffer() {
                        renderer.clear_welcome_logo();
                        renderer.clear_buffer_terminal();
                        renderer.clear_editor_content();
                        renderer.update_settings_buffer_content(settings, center_bounds);
                    } else if let Some(help) = self.app_state.active_help_buffer() {
                        renderer.clear_welcome_logo();
                        renderer.clear_buffer_terminal();
                        renderer.clear_editor_content();
                        renderer.update_help_buffer_content(help, center_bounds);
                    } else if let Some(image) = self.app_state.active_image_buffer() {
                        renderer.update_image_content(image, center_bounds);
                    } else {
                        // Skip editor content rendering in Zen Mode when target is not CenterEditor
                        let zen_mode_active = self.panel_state.maximized_region.is_some()
                            && self.panel_state.maximized_region != Some(FocusTarget::CenterEditor);

                        if zen_mode_active {
                            renderer.clear_editor_content();
                            renderer.clear_editor_overlays();
                        } else {
                            renderer.clear_welcome_logo();
                            renderer.clear_buffer_terminal();
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
                self.caret_blink_dirty = false;
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
                        renderer.update_image_content(image, center_bounds);
                    }
                } else if !show_welcome
                    && (self.panel_state.maximized_region.is_none()
                        || self.panel_state.maximized_region
                            == Some(FocusTarget::CenterEditor))
                    && let Some(renderer) = self.renderer.as_mut()
                {
                    renderer.update_editor_caret(&self.app_state, center_bounds);
                    renderer.update_editor_overlays(&self.app_state, center_bounds);
                }
                self.editor_caret_needs_layout = false;
                // Cursor đã được re-projected → reset blink về visible.
                self.caret_blink_visible = true;
                self.caret_blink_dirty = false;
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
                if active_terminal_session.is_none() && !references_active && !show_welcome {
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
                    let root_name = self
                        .app_state
                        .workspace_root_path()
                        .and_then(|root| root.file_name().and_then(|name| name.to_str()))
                        .unwrap_or("workspace");
                    let header = format!("[ {root_name} ]");
                    self.sidebar_selection_quads = renderer.update_sidebar_content(
                        Some(&header),
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
        let ai_chat_active = self.panel_state.right.visible
            && self.panel_state.right.active_tab_id() == Some(PanelTabId::AiChat);
        if ai_chat_active {
            let history_bounds = flat_regions
                .iter()
                .find(|r| r.id == RegionId::AiChatHistory && r.visible)
                .map(|r| [r.bounds.x, r.bounds.y, r.bounds.width, r.bounds.height]);
            let input_bounds = flat_regions
                .iter()
                .find(|r| r.id == RegionId::AiChatInput && r.visible)
                .map(|r| [r.bounds.x, r.bounds.y, r.bounds.width, r.bounds.height]);

            if let (Some(hb), Some(ib)) = (history_bounds, input_bounds) {
                let chat = &self.panel_state.ai_chat;
                let file_suggestions = self.ai_chat_file_reference_suggestions(&chat.input_buffer);
                if let Some(renderer) = self.renderer.as_mut() {
                    let show_cursor = self.focus_manager.current() == FocusTarget::RightSidebar;
                    let inner_padding = self.layout_engine.config.inner_padding;
                    let cursor_quads = renderer.update_ai_chat_content(
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
                    );
                    region_instances.extend(cursor_quads);
                }
            }
        } else if let Some(renderer) = self.renderer.as_mut() {
            renderer.clear_ai_chat();
        }

        // ── Right-sidebar terminal ────────────────────────────────────────
        let right_terminal_active = self.panel_state.right.visible
            && self.panel_state.right.active_tab_id() == Some(PanelTabId::Terminal);
        if right_terminal_active {
            let right_bounds = flat_regions
                .iter()
                .find(|r| r.id == RegionId::RightSidebar && r.visible)
                .map(|r| [r.bounds.x, r.bounds.y, r.bounds.width, r.bounds.height]);
            if let Some(rb) = right_bounds {
                let bounds_changed = self.last_right_terminal_bounds != Some(rb);
                let grid_changed = self.sync_right_terminal_layout(rb);
                if (self.right_terminal_needs_layout || bounds_changed || grid_changed)
                    && let Some(renderer) = self.renderer.as_mut()
                {
                    renderer.update_terminal_content(
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
        }

        // ── Markdown Preview (right sidebar) ──────────────────────────────
        let md_preview_active = self.panel_state.right.visible
            && self.panel_state.right.active_tab_id() == Some(PanelTabId::MarkdownPreview);
        if md_preview_active && self.app_state.markdown_preview.visible {
            let preview_bounds = flat_regions
                .iter()
                .find(|r| r.id == RegionId::AiChatHistory && r.visible)
                .map(|r| [r.bounds.x, r.bounds.y, r.bounds.width, r.bounds.height])
                .or_else(|| {
                    // Fallback: when maximized, AiChatHistory isn't in flat_regions —
                    // use the RightSidebar region from the maximized layout instead.
                    self.panel_state.maximized_region.and_then(|_| {
                        flat_regions
                            .iter()
                            .find(|r| r.id == RegionId::RightSidebar && r.visible)
                            .map(|r| [r.bounds.x, r.bounds.y, r.bounds.width, r.bounds.height])
                    })
                });

            if let Some(bounds) = preview_bounds {
                let preview = &self.app_state.markdown_preview;
                let inner_padding = self.layout_engine.config.inner_padding;
                if let Some(renderer) = self.renderer.as_mut() {
                    renderer.update_markdown_preview_content(
                        bounds,
                        &preview.rendered_lines,
                        preview.scroll_y,
                        inner_padding,
                    );
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
                .map(|(idx, buffer)| TopbarTab {
                    label: buffer.label(),
                    kind: match &buffer.content {
                        BufferContent::Text(text) => TopbarTabKind::Text {
                            path: text.path.clone(),
                        },
                        BufferContent::Image(image) => TopbarTabKind::Image {
                            path: image.path.clone(),
                        },
                        BufferContent::Terminal(_) => TopbarTabKind::Terminal,
                        BufferContent::References(_) => TopbarTabKind::References,
                        BufferContent::Diagnostics(_) => TopbarTabKind::Diagnostics,
                        BufferContent::FuzzyPicker(_) => TopbarTabKind::FuzzyPicker,
                        BufferContent::SettingsTab(_) => TopbarTabKind::Settings,
                        BufferContent::Help(_) => TopbarTabKind::Help,
                    },
                    is_dirty: buffer.is_dirty(
                        self.app_state.active_buffer_index() == Some(idx),
                        self.app_state.is_dirty(),
                    ),
                })
                .collect::<Vec<_>>();
            if let Some(renderer) = self.renderer.as_mut() {
                let tab_quads = renderer.update_topbar_content(
                    &tabs,
                    self.app_state.active_buffer_index(),
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
                status_line,
                status_col,
                diagnostics_errors,
                diagnostics_warnings,
            ) = if show_welcome {
                ("Welcome", "", 0, 0, 0, 0)
            } else {
                let filetype = self.app_state.active_filetype_label();
                let git_branch = self.workspace_git_branch.as_deref().unwrap_or("-");
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
                    line,
                    col,
                    diagnostics_errors,
                    diagnostics_warnings,
                )
            };
            if let Some(renderer) = self.renderer.as_mut() {
                let pill_quads = renderer.update_statusbar_content(
                    mode,
                    &pending_keys,
                    git_branch,
                    filetype,
                    self.app_state.active_search_match_position(),
                    status_line,
                    status_col,
                    diagnostics_errors,
                    diagnostics_warnings,
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
                let bounds_changed = self.last_terminal_bounds != Some(bottom_bounds);
                let grid_changed = self.sync_terminal_layout(bottom_bounds);
                if (self.terminal_needs_layout || bounds_changed || grid_changed)
                    && let Some(renderer) = self.renderer.as_mut()
                {
                    renderer.update_terminal_content(
                        &self.terminal_grid,
                        bottom_bounds,
                        self.app_state.current_mode(),
                    );
                    self.last_terminal_bounds = Some(bottom_bounds);
                    self.terminal_needs_layout = false;
                }
            } else if self.last_terminal_bounds.is_some() {
                if let Some(renderer) = self.renderer.as_mut() {
                    renderer.clear_terminal();
                }
                self.last_terminal_bounds = None;
            }
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
            renderer.update_toast_popup(&toast.message, w, h);
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
    use super::{focus_ring_instances, focus_target_region_id};
    use crate::workbench::{focus_manager::FocusTarget, region_model::RegionId};

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
}
