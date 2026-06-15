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
                let ai_cfg = self.ai_config.inline_completion.as_ref();
                let leetcode_cfg = self.ai_config.leetcode.as_ref();
                let leetcode_provider = leetcode_cfg.and_then(|c| c.provider.as_ref());
                let ai = crate::app::app_state::AiInlineSettings {
                    api_url: ai_cfg
                        .map(|cfg| cfg.provider.api_url.clone())
                        .unwrap_or_default(),
                    model: ai_cfg
                        .map(|cfg| cfg.provider.model.clone())
                        .unwrap_or_default(),
                    api_key: ai_cfg
                        .and_then(|cfg| cfg.provider.api_key.clone())
                        .unwrap_or_default(),
                    endpoint_kind: ai_cfg
                        .and_then(|cfg| cfg.provider.endpoint_kind.clone())
                        .unwrap_or_default(),
                    max_tokens: ai_cfg.map(|cfg| cfg.max_tokens()).unwrap_or(96),
                    prefix_chars: ai_cfg.map(|cfg| cfg.prefix_chars()).unwrap_or(1200),
                    suffix_chars: ai_cfg.map(|cfg| cfg.suffix_chars()).unwrap_or(400),
                    debounce_ms: ai_cfg.map(|cfg| cfg.debounce_ms()).unwrap_or(80),
                    leetcode_ai_enabled: self.ai_config.leetcode_ai_enabled(),
                    leetcode_api_url: leetcode_provider
                        .map(|p| p.api_url.clone())
                        .unwrap_or_default(),
                    leetcode_model: leetcode_provider
                        .map(|p| p.model.clone())
                        .unwrap_or_default(),
                    leetcode_api_key: leetcode_provider
                        .and_then(|p| p.api_key.clone())
                        .unwrap_or_default(),
                    leetcode_endpoint_kind: leetcode_provider
                        .and_then(|p| p.endpoint_kind.clone())
                        .unwrap_or_default(),
                    leetcode_reasoning_effort: leetcode_provider
                        .and_then(|p| p.reasoning_effort.clone())
                        .unwrap_or_default(),
                };
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
                    ai,
                    self.ui_config.window.scale_factor_override,
                    self.base_theme.ui.bg_opacity,
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
                    // Only dismiss the install/uninstall popup once the process has
                    // finished. Clearing it mid-run dropped the live logs and made the
                    // running install invisible ("popup does nothing" bug).
                    match state.command.as_ref() {
                        Some(cmd) if cmd.running => {}
                        Some(_) => {
                            state.command = None;
                            cmd_changed = true;
                        }
                        None => {}
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
                if let Some(running) = self
                    .app_state
                    .active_extensions_manager_buffer()
                    .and_then(|state| state.command.as_ref())
                    .filter(|cmd| cmd.running)
                {
                    self.show_transient_toast_kind(
                        format!(
                            "Another operation is in progress ({})\nWait for it to finish before starting a new one.",
                            running.binary
                        ),
                        ToastKind::Warning,
                    );
                    return Some(false);
                }
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

                // Pre-check: the package manager this install command relies on
                // must itself be installed, otherwise the command dies with a
                // cryptic "command not found" in the logs.
                if let Some((manager, hint)) = missing_install_prerequisite(&install_cmd) {
                    self.show_transient_toast_kind(
                        format!("Cannot install {name}\n`{manager}` is required but was not found on this system. {hint}"),
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
                if let Some(running) = self
                    .app_state
                    .active_extensions_manager_buffer()
                    .and_then(|state| state.command.as_ref())
                    .filter(|cmd| cmd.running)
                {
                    self.show_transient_toast_kind(
                        format!(
                            "Another operation is in progress ({})\nWait for it to finish before starting a new one.",
                            running.binary
                        ),
                        ToastKind::Warning,
                    );
                    return Some(false);
                }
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
            Command::ResizeDecreaseWidth => Some(self.resize_focused_width(-Self::RESIZE_STEP_PX)),
            Command::ResizeIncreaseWidth => Some(self.resize_focused_width(Self::RESIZE_STEP_PX)),
            Command::ResizeDecreaseHeight => {
                Some(self.resize_focused_height(-Self::RESIZE_STEP_PX))
            }
            Command::ResizeIncreaseHeight => Some(self.resize_focused_height(Self::RESIZE_STEP_PX)),
            Command::ResizeGrowLeftDock => Some(self.resize_left_dock(Self::RESIZE_STEP_PX)),
            Command::ResizeGrowRightDock => Some(self.resize_right_dock(Self::RESIZE_STEP_PX)),
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

/// If `install_cmd` relies on a package manager that is missing from the
/// (version-manager-augmented) PATH, return that manager plus an install hint.
/// Unknown leading tokens are not gated — only well-known managers are checked.
fn missing_install_prerequisite(install_cmd: &str) -> Option<(&'static str, &'static str)> {
    let first_token = install_cmd.split_whitespace().next()?;
    let (manager, hint) = match first_token {
        "brew" => (
            "brew",
            "Install Homebrew first: https://brew.sh, then retry.",
        ),
        "npm" | "npx" => (
            "npm",
            "Install Node.js (includes npm) first: https://nodejs.org or `brew install node`.",
        ),
        "go" => (
            "go",
            "Install Go first: https://go.dev/dl or `brew install go`.",
        ),
        "cargo" | "rustup" => ("cargo", "Install Rust first: https://rustup.rs."),
        "pip" | "pip3" | "pipx" => (
            "pip3",
            "Install Python 3 first: https://www.python.org or `brew install python`.",
        ),
        "gem" => ("gem", "Install Ruby first: `brew install ruby`."),
        "dotnet" => (
            "dotnet",
            "Install the .NET SDK first: https://dotnet.microsoft.com/download.",
        ),
        _ => return None,
    };

    let resolved_path = crate::async_runtime::scheduler::resolve_system_path();
    let found = std::env::split_paths(&resolved_path).any(|dir| dir.join(manager).is_file());
    if found { None } else { Some((manager, hint)) }
}
