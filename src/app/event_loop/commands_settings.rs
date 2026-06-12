use super::*;
use crate::app::app_state::ExtensionCategory;

impl AppShell {
    pub(super) fn handle_settings_command(&mut self, command: &Command) -> Option<bool> {
        match command {
            Command::OpenSettings => {
                let theme_profile = self
                    .persistent_state
                    .configured_theme_profile()
                    .unwrap_or(self.base_theme.name.as_str())
                    .to_string();
                let font_family = self
                    .base_theme
                    .editor
                    .font_family
                    .clone()
                    .unwrap_or_default();
                self.app_state.open_settings_buffer(
                    theme_profile,
                    font_family,
                    self.base_theme.editor.font_size,
                    self.base_theme.editor.line_height,
                    self.ui_config.indent.tab_width,
                    self.ui_config.indent.insert_spaces,
                    self.ui_config.docks.left.size_px.round() as i32,
                    self.ui_config.docks.right.size_px.round() as i32,
                    self.ui_config.docks.bottom.size_px.round() as i32,
                    self.ui_config.border_radius_px > 0.0,
                    self.ui_config.border_radius_px,
                    self.ui_config.enable_outline,
                    self.ai_config.inline_completion_enabled(),
                );
                let _ = self.sync_focus_mode_for_active_buffer();
                self.editor_needs_layout = true;
                self.editor_caret_needs_layout = false;
                Some(true)
            }
            Command::OpenExtensionsManager => {
                let index = self.app_state.open_extensions_manager_buffer();
                if let Some(guide) = self.active_system_dep_guide.as_ref()
                    && let Some(missing) = guide.missing_tools.as_ref()
                    && let Some(state) = self.app_state.active_extensions_manager_buffer_mut()
                {
                    for item in &mut state.items {
                        if item.category == ExtensionCategory::CliTools {
                            item.installed = !missing.iter().any(|tool| tool == &item.binary);
                        }
                    }
                }
                let _ = index;
                self.submit(RequestSpec {
                    revision_id: 0,
                    topic: RequestTopic::SystemDepCheck,
                    payload: WorkerRequestPayload::CheckSystemDeps,
                });
                let _ = self.sync_focus_mode_for_active_buffer();
                self.editor_needs_layout = true;
                self.editor_caret_needs_layout = false;
                Some(true)
            }
            Command::ExtensionsSelectNext => {
                let changed = self.app_state.extensions_select_next();
                if changed {
                    self.editor_needs_layout = true;
                    self.editor_caret_needs_layout = false;
                }
                Some(changed)
            }
            Command::ExtensionsSelectPrev => {
                let changed = self.app_state.extensions_select_prev();
                if changed {
                    self.editor_needs_layout = true;
                    self.editor_caret_needs_layout = false;
                }
                Some(changed)
            }
            Command::ExtensionsToggleExpanded => {
                let changed = self.app_state.extensions_toggle_expanded_selected();
                if changed {
                    self.editor_needs_layout = true;
                    self.editor_caret_needs_layout = false;
                }
                Some(changed)
            }
            Command::ExtensionsSwitchTabNext => {
                let changed = self.app_state.extensions_switch_tab_next();
                if changed {
                    self.editor_needs_layout = true;
                    self.editor_caret_needs_layout = false;
                }
                Some(changed)
            }
            Command::ExtensionsSwitchTabPrev => {
                let changed = self.app_state.extensions_switch_tab_prev();
                if changed {
                    self.editor_needs_layout = true;
                    self.editor_caret_needs_layout = false;
                }
                Some(changed)
            }
            Command::ExtensionsStartFilter => {
                let changed = self.app_state.extensions_set_filter_focused(true);
                if let Ok(result) = self.app_state.apply_mode_event(ModeEvent::EnterInsert) {
                    self.editor_needs_layout = true;
                    self.editor_caret_needs_layout = false;
                    Some(changed || result.changed)
                } else {
                    Some(changed)
                }
            }
            Command::ExtensionsCancelFilter => {
                let mut cmd_changed = false;
                if let Some(state) = self.app_state.active_extensions_manager_buffer_mut() {
                    if state.command.is_some() {
                        state.command = None;
                        cmd_changed = true;
                    }
                }
                let changed = self.app_state.extensions_set_filter_focused(false);
                if let Ok(result) = self.app_state.apply_mode_event(ModeEvent::Escape) {
                    self.editor_needs_layout = true;
                    self.editor_caret_needs_layout = false;
                    Some(cmd_changed || changed || result.changed)
                } else {
                    Some(cmd_changed || changed)
                }
            }
            Command::ExtensionsInstallSelected => {
                let selected =
                    self.app_state
                        .active_extensions_manager_buffer()
                        .and_then(|state| {
                            state.selected_item().map(|item| {
                                let install = if state.platform == "macOS" {
                                    item.macos_install.clone()
                                } else {
                                    item.linux_install.clone()
                                };
                                (item.name.clone(), item.binary.clone(), install)
                            })
                        });

                let Some((name, binary, install_cmd)) = selected else {
                    return Some(false);
                };
                if install_cmd.trim().is_empty() {
                    self.show_transient_toast_kind(
                        format!("Install unavailable\nNo install command configured for {name}"),
                        ToastKind::Warning,
                    );
                    return Some(false);
                }

                if let Some(state) = self.app_state.active_extensions_manager_buffer_mut() {
                    state.start_command(binary.clone(), false);
                }
                self.submit(RequestSpec {
                    revision_id: 0,
                    topic: RequestTopic::SystemTask,
                    payload: WorkerRequestPayload::RunExtensionCommand {
                        binary: binary.clone(),
                        command: install_cmd.clone(),
                        uninstall: false,
                        working_dir: self
                            .app_state
                            .workspace_root_path()
                            .map(std::path::PathBuf::from),
                    },
                });

                let changed = true;
                self.pending_lsp_server = None;
                self.lsp_retry_at =
                    Some(std::time::Instant::now() + std::time::Duration::from_secs(15));
                self.show_transient_toast_kind(
                    format!("Installing {binary}\nRunning: {install_cmd}\nOpen Extensions Manager footer to watch live logs."),
                    ToastKind::Info,
                );
                self.editor_needs_layout = true;
                self.editor_caret_needs_layout = false;
                Some(changed || true)
            }
            Command::ExtensionsUninstallSelected => {
                let selected =
                    self.app_state
                        .active_extensions_manager_buffer()
                        .and_then(|state| {
                            state.selected_item().map(|item| {
                                let uninstall = if state.platform == "macOS" {
                                    item.macos_uninstall.clone()
                                } else {
                                    item.linux_uninstall.clone()
                                };
                                (item.name.clone(), item.binary.clone(), uninstall)
                            })
                        });

                let Some((name, binary, uninstall_cmd)) = selected else {
                    return Some(false);
                };
                if uninstall_cmd.trim().is_empty() {
                    self.show_transient_toast_kind(
                        format!(
                            "Uninstall unavailable\nNo uninstall command configured for {name}"
                        ),
                        ToastKind::Warning,
                    );
                    return Some(false);
                }

                if let Some(state) = self.app_state.active_extensions_manager_buffer_mut() {
                    state.start_command(binary.clone(), true);
                }
                self.submit(RequestSpec {
                    revision_id: 0,
                    topic: RequestTopic::SystemTask,
                    payload: WorkerRequestPayload::RunExtensionCommand {
                        binary: binary.clone(),
                        command: uninstall_cmd.clone(),
                        uninstall: true,
                        working_dir: self
                            .app_state
                            .workspace_root_path()
                            .map(std::path::PathBuf::from),
                    },
                });

                self.show_transient_toast_kind(
                    format!("Uninstalling {binary}\nRunning: {uninstall_cmd}\nOpen Extensions Manager footer to watch live logs."),
                    ToastKind::Info,
                );
                self.editor_needs_layout = true;
                self.editor_caret_needs_layout = false;
                Some(true)
            }
            Command::SettingsSelectNext => {
                let changed = self.app_state.settings_select_next();
                if changed {
                    self.editor_needs_layout = true;
                    self.editor_caret_needs_layout = false;
                }
                Some(changed)
            }
            Command::SettingsSelectPrev => {
                let changed = self.app_state.settings_select_prev();
                if changed {
                    self.editor_needs_layout = true;
                    self.editor_caret_needs_layout = false;
                }
                Some(changed)
            }
            Command::SettingsAdjustDecrease => Some(self.adjust_selected_setting(-1)),
            Command::SettingsAdjustIncrease => Some(self.adjust_selected_setting(1)),
            Command::SettingsActivate => Some(self.activate_selected_setting()),
            Command::ResizeDecreaseWidth => {
                Some(self.resize_focused_window(-Self::RESIZE_STEP_PX, 0.0))
            }
            Command::ResizeIncreaseWidth => {
                Some(self.resize_focused_window(Self::RESIZE_STEP_PX, 0.0))
            }
            Command::ResizeIncreaseLeftWidth => {
                Some(self.resize_editor_left_edge(Self::RESIZE_STEP_PX))
            }
            Command::ResizeDecreaseLeftWidth => {
                Some(self.resize_editor_left_edge(-Self::RESIZE_STEP_PX))
            }
            Command::ResizeIncreaseRightWidth => {
                Some(self.resize_editor_right_edge(Self::RESIZE_STEP_PX))
            }
            Command::ResizeDecreaseRightWidth => {
                Some(self.resize_editor_right_edge(-Self::RESIZE_STEP_PX))
            }
            Command::ResizeDecreaseHeight => {
                Some(self.resize_focused_window(0.0, -Self::RESIZE_STEP_PX))
            }
            Command::ResizeIncreaseHeight => {
                Some(self.resize_focused_window(0.0, Self::RESIZE_STEP_PX))
            }
            Command::CloseFilePicker if self.app_state.active_buffer_is_extensions_manager() => {
                if self.app_state.extensions_set_filter_focused(false) {
                    if let Ok(result) = self.app_state.apply_mode_event(ModeEvent::Escape) {
                        self.editor_needs_layout = true;
                        self.editor_caret_needs_layout = false;
                        return Some(result.changed || true);
                    }
                    self.editor_needs_layout = true;
                    self.editor_caret_needs_layout = false;
                    Some(true)
                } else {
                    Some(self.close_current_buffer_now())
                }
            }
            Command::CloseFilePicker if self.app_state.active_buffer_is_settings() => {
                if self.app_state.settings_is_editing() {
                    let changed = self.app_state.settings_cancel_editing();
                    if changed {
                        if self.app_state.current_mode() == crate::core::mode::EditorMode::Insert {
                            if let Ok(result) = self
                                .app_state
                                .apply_mode_event(crate::core::mode::ModeEvent::Escape)
                            {
                                let _ = result.changed;
                            }
                        }
                        self.editor_needs_layout = true;
                        self.editor_caret_needs_layout = false;
                    }
                    Some(changed)
                } else {
                    Some(self.close_current_buffer_now())
                }
            }
            Command::FilePickerAppendQuery(_) | Command::FilePickerBackspaceQuery
                if self.app_state.active_buffer_is_extensions_manager() =>
            {
                let changed = match command {
                    Command::FilePickerAppendQuery(text) => {
                        self.app_state.extensions_append_filter(text)
                    }
                    Command::FilePickerBackspaceQuery => {
                        self.app_state.extensions_backspace_filter()
                    }
                    _ => false,
                };
                if changed {
                    self.editor_needs_layout = true;
                    self.editor_caret_needs_layout = false;
                }
                Some(changed)
            }
            Command::FilePickerAppendQuery(_)
            | Command::FilePickerBackspaceQuery
            | Command::EditorPaste
            | Command::PasteSystemClipboard
                if self.app_state.active_buffer_is_settings()
                    && self.app_state.settings_is_editing() =>
            {
                let changed = match command {
                    Command::FilePickerAppendQuery(text) => {
                        self.app_state.settings_append_editing_text(text)
                    }
                    Command::FilePickerBackspaceQuery => {
                        self.app_state.settings_backspace_editing()
                    }
                    Command::EditorPaste | Command::PasteSystemClipboard => {
                        if let Ok(text) = self.clipboard.get_text() {
                            self.app_state.settings_append_editing_text(&text)
                        } else {
                            false
                        }
                    }
                    _ => false,
                };
                if changed {
                    self.editor_needs_layout = true;
                    self.editor_caret_needs_layout = false;
                }
                Some(changed)
            }
            _ => None,
        }
    }
}
