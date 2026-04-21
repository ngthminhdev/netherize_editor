use super::*;
use winit::{
    event::ElementState,
    keyboard::{Key, NamedKey},
};

impl ApplicationHandler for AppShell {
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
        self.last_editor_bounds = None;
        self.last_show_welcome = None;
        self.last_sidebar_bounds = None;
        self.last_terminal_bounds = None;

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
                self.last_editor_bounds = None;
                self.last_show_welcome = None;
                self.last_sidebar_bounds = None;
                self.last_terminal_bounds = None;
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
                if self.should_swallow_palette_ime_commit() {
                    return;
                }
                let context = self.build_context();
                if let Some(translated) = self.input_handler.translate_ime_commit(&text, context)
                    && self.handle_command_with_count(translated.command, translated.repeat_count)
                {
                    self.request_redraw();
                }
            }
            WindowEvent::KeyboardInput {
                event: key_event, ..
            } => {
                if key_event.state == ElementState::Pressed && !key_event.repeat {
                    self.note_post_open_keyboard_press();
                }
                if let Some(changed) = self.handle_pending_confirmation_key_event(&key_event) {
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
            WindowEvent::RedrawRequested => self.redraw(),
            _ => {}
        }
    }

    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        self.flush_pending_parse_after_debounce();
        if self.pump_bridge() {
            self.request_redraw();
        }
    }
}

impl AppShell {
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

        let mut region_instances: Vec<RegionDrawInstance> = flat_regions
            .iter()
            .copied()
            .filter(|region| {
                region.visible
                    && region.id != RegionId::Root
                    && region.id != RegionId::OverlayLayer
                    && region.id != RegionId::StatusBar
                    && !(show_welcome && !workspace_attached && region.id == RegionId::LeftSidebar)
            })
            .map(|region| {
                RegionDrawInstance::new(
                    [
                        region.bounds.x,
                        region.bounds.y,
                        region.bounds.width,
                        region.bounds.height,
                    ],
                    region_color(region.id, &self.theme),
                )
            })
            .collect();
        let focus_target = if show_welcome && !workspace_attached {
            FocusTarget::CenterEditor
        } else {
            self.focus_manager.current()
        };
        if let Some(bounds) = focus_region_bounds(&layout.model, focus_target) {
            region_instances.extend(focus_ring_instances(
                bounds,
                self.theme.ui.accent.as_f32(),
                2.0,
            ));
        }

        if let Some(center_bounds) = center_bounds {
            let bounds_changed = self.last_editor_bounds != Some(center_bounds);
            let mut refresh_highlights_for_viewport = false;
            if self.editor_needs_layout || bounds_changed || show_welcome_changed {
                if let Some(renderer) = self.renderer.as_mut() {
                    if show_welcome {
                        let (text, styled) = welcome_screen_content(&self.theme);
                        renderer.clear_editor_content();
                        renderer.update_welcome_screen_content(&text, &styled, center_bounds);
                    } else {
                        renderer.clear_welcome_logo();
                        renderer.update_editor_content(
                            &self.app_state.text_string(),
                            &self.app_state,
                            center_bounds,
                            &syntax_spans_to_styled(&self.highlight_spans, &self.theme),
                        );
                    }
                }
                self.last_editor_bounds = Some(center_bounds);
                self.editor_needs_layout = false;
                self.editor_caret_needs_layout = false;
                refresh_highlights_for_viewport = bounds_changed && !show_welcome;
            } else if self.editor_caret_needs_layout {
                if !show_welcome && let Some(renderer) = self.renderer.as_mut() {
                    renderer.update_editor_caret(&self.app_state, center_bounds);
                }
                self.editor_caret_needs_layout = false;
            }

            if !show_welcome && let Some(renderer) = self.renderer.as_ref() {
                if self.app_state.current_mode() != EditorMode::Visual
                    && let Some(quad) =
                        renderer.current_line_highlight_quad(&self.app_state, center_bounds)
                {
                    region_instances.push(quad);
                }
                region_instances
                    .extend(renderer.search_highlight_quads(&self.app_state, center_bounds));
                if self.app_state.current_mode() == EditorMode::Visual {
                    region_instances
                        .extend(renderer.visual_selection_quads(&self.app_state, center_bounds));
                }
            }

            // Leap label overlay — luôn cập nhật sau khi editor đã shape buffer.
            // Nếu không có labels (đã cancel hoặc jump xong) thì clear.
            if let Some(labels) = &self.leap_labels {
                if !show_welcome {
                    if let Some(renderer) = self.renderer.as_mut() {
                        let labels_clone = labels.clone();
                        renderer.update_leap_labels(&labels_clone, &self.app_state, center_bounds);
                    }
                }
            } else if let Some(renderer) = self.renderer.as_mut() {
                renderer.clear_leap_labels();
            }

            if refresh_highlights_for_viewport {
                self.submit_parse_for_active_buffer(true);
            }
        } else if let Some(renderer) = self.renderer.as_mut() {
            renderer.clear_welcome_logo();
        }
        self.last_show_welcome = Some(show_welcome);

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
                    let rows = build_sidebar_rows(
                        &self.explorer_snapshot.entries,
                        self.explorer_cursor,
                        &self.theme,
                    );
                    self.sidebar_selection_quads = renderer.update_sidebar_content(
                        Some(&header),
                        &rows,
                        bounds,
                        sidebar_focused,
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

        if let Some(top) = layout.model.find(RegionId::TopBar) {
            let top_bounds = [top.x, top.y, top.width, top.height];
            let tabs = self
                .app_state
                .open_buffers()
                .iter()
                .map(|path| crate::render::renderer::TopbarTab {
                    label: path
                        .file_name()
                        .and_then(|name| name.to_str())
                        .map(str::to_string)
                        .unwrap_or_else(|| path.display().to_string()),
                    path: path.clone(),
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
            let filetype = self.app_state.active_filetype_label();
            let git_branch = self.workspace_git_branch.as_deref().unwrap_or("-");
            if let Some(renderer) = self.renderer.as_mut() {
                let pill_quads = renderer.update_statusbar_content(
                    mode,
                    &pending_keys,
                    git_branch,
                    filetype,
                    line,
                    col,
                    status_bounds,
                );
                region_instances.extend(pill_quads);
            }
        }

        if self.app_state.is_command_palette_visible() {
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

        if let (Some(bottom), Some(renderer)) = (bottom_region, self.renderer.as_mut()) {
            if bottom.visible && !show_welcome {
                let bottom_bounds = [
                    bottom.bounds.x,
                    bottom.bounds.y,
                    bottom.bounds.width,
                    bottom.bounds.height,
                ];
                let bounds_changed = self.last_terminal_bounds != Some(bottom_bounds);
                if self.terminal_needs_layout || bounds_changed {
                    renderer.update_terminal_content(
                        &self.terminal_grid,
                        bottom_bounds,
                        self.app_state.current_mode() == EditorMode::TerminalFocus,
                    );
                    self.last_terminal_bounds = Some(bottom_bounds);
                    self.terminal_needs_layout = false;
                }
            } else if self.last_terminal_bounds.is_some() {
                renderer.clear_terminal();
                self.last_terminal_bounds = None;
            }
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

        let decision = match key_event.logical_key.as_ref() {
            Key::Named(NamedKey::Escape) => Some(false),
            Key::Character(text) if text.eq_ignore_ascii_case("y") => Some(true),
            Key::Character(text) if text.eq_ignore_ascii_case("n") => Some(false),
            _ => Some(false),
        };

        decision.map(|confirmed| self.respond_to_pending_confirmation(confirmed))
    }
}

fn focus_region_bounds(
    model: &crate::workbench::region_model::RegionModel,
    target: FocusTarget,
) -> Option<[f32; 4]> {
    let region_id = match target {
        FocusTarget::CenterEditor => RegionId::Center,
        FocusTarget::LeftSidebar => RegionId::LeftSidebar,
        FocusTarget::RightSidebar => RegionId::RightSidebar,
        FocusTarget::BottomPanel => RegionId::BottomPanel,
        FocusTarget::TopBar => RegionId::TopBar,
        FocusTarget::StatusBar => RegionId::StatusBar,
        FocusTarget::OverlayLayer => return None,
    };
    let region = model.find(region_id)?;
    if region.width <= 0.0 || region.height <= 0.0 {
        return None;
    }
    Some([region.x, region.y, region.width, region.height])
}

fn focus_ring_instances(
    bounds: [f32; 4],
    mut color: [f32; 4],
    thickness: f32,
) -> Vec<RegionDrawInstance> {
    let [x, y, w, h] = bounds;
    if w <= 0.0 || h <= 0.0 {
        return Vec::new();
    }

    let t = thickness.max(1.0).min(w * 0.5).min(h * 0.5);
    color[3] = color[3].max(0.9);

    vec![
        RegionDrawInstance::new([x, y, w, t], color),
        RegionDrawInstance::new([x, y + h - t, w, t], color),
        RegionDrawInstance::new([x, y, t, h], color),
        RegionDrawInstance::new([x + w - t, y, t, h], color),
    ]
}
