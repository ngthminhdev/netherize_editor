use super::super::*;
use crate::async_runtime::message::WorkerResultPayload;

pub(super) fn handle_system_result(app: &mut AppShell, payload: WorkerResultPayload) {
    match payload {
        WorkerResultPayload::SystemDepCheckResult { missing } => {
            let missing_names: Vec<String> = missing.iter().map(|s| s.to_string()).collect();
            // Apply to the extensions buffer even when it is not the active one —
            // otherwise results arriving after a buffer switch left stale states.
            if let Some(state) = app.app_state.any_extensions_manager_buffer_mut() {
                for item in &mut state.items {
                    item.installed = !missing_names.iter().any(|tool| tool == &item.binary);
                }
                state.deps_checked = true;
            }

            let cli_tools = ["fzf", "lazygit", "lazydocker", "rg", "fd", "bat", "delta"];
            let critical_missing: Vec<String> = missing_names
                .iter()
                .filter(|tool| cli_tools.contains(&tool.as_str()))
                .cloned()
                .collect();

            if critical_missing.is_empty() {
                app.active_system_dep_guide = None;
                app.editor_needs_layout = true;
                app.request_redraw();
                return;
            }
            if app.dismissed_system_deps {
                app.editor_needs_layout = true;
                app.request_redraw();
                return;
            }
            let install_cmd = if cfg!(target_os = "macos") {
                format!("brew install {}", critical_missing.join(" "))
            } else {
                format!("sudo apt-get install -y {}", critical_missing.join(" "))
            };
            let tool_statuses = critical_missing
                .iter()
                .map(|t| {
                    (
                        t.clone(),
                        crate::async_runtime::message::InstallStatus::Pending,
                    )
                })
                .collect();
            app.active_system_dep_guide = Some(SystemDepGuide {
                state: SystemDepState::Detected,
                missing_tools: Some(critical_missing),
                install_command: Some(install_cmd),
                tool_statuses,
            });
            app.request_redraw();
        }
        WorkerResultPayload::RuntimeVersionsDetected {
            python_version,
            node_version,
            go_version,
        } => {
            app.runtime_versions.python_version = python_version;
            app.runtime_versions.node_version = node_version;
            app.runtime_versions.go_version = go_version;
            app.request_redraw();
        }
        WorkerResultPayload::PythonEnvironmentsDiscovered(envs) => {
            eprintln!("[AppShell] python environments discovered: {}", envs.len());
            let items: Vec<crate::app::command_palette::CommandPaletteItem> = envs
                .into_iter()
                .map(|env| crate::app::command_palette::CommandPaletteItem {
                    label: env.display_name,
                    secondary_label: Some(env.executable.display().to_string()),
                    action: crate::app::command_palette::CommandPaletteAction::SelectPythonEnv(
                        env.executable,
                    ),
                    tone: crate::app::command_palette::CommandPaletteItemTone::Default,
                    preview_colors: Vec::new(),
                })
                .collect();
            app.app_state.open_python_env_selector_with_items(items);
            app.request_redraw();
        }
        WorkerResultPayload::DartEnvironmentsDiscovered(envs) => {
            eprintln!("[AppShell] dart environments discovered: {}", envs.len());
            let items: Vec<crate::app::command_palette::CommandPaletteItem> = envs
                .into_iter()
                .map(|env| crate::app::command_palette::CommandPaletteItem {
                    label: env.display_name,
                    secondary_label: Some(env.executable.display().to_string()),
                    action: crate::app::command_palette::CommandPaletteAction::SelectDartEnv(
                        env.executable,
                    ),
                    tone: crate::app::command_palette::CommandPaletteItemTone::Default,
                    preview_colors: Vec::new(),
                })
                .collect();
            app.app_state.open_dart_env_selector_with_items(items);
            app.request_redraw();
        }
        WorkerResultPayload::FlutterDevicesDiscovered(devices) => {
            eprintln!("[AppShell] flutter devices discovered: {}", devices.len());
            let items: Vec<crate::app::command_palette::CommandPaletteItem> = devices
                .into_iter()
                .map(|dev| {
                    let secondary = if dev.is_active {
                        Some(format!("active ({})", dev.platform))
                    } else {
                        Some(format!("emulator offline ({})", dev.platform))
                    };
                    crate::app::command_palette::CommandPaletteItem {
                        label: dev.name,
                        secondary_label: secondary,
                        action:
                            crate::app::command_palette::CommandPaletteAction::SelectFlutterDevice {
                                device_id: dev.id,
                                is_emulator: dev.emulator,
                                is_active: dev.is_active,
                            },
                        tone: crate::app::command_palette::CommandPaletteItemTone::Default,
                        preview_colors: Vec::new(),
                    }
                })
                .collect();
            app.app_state.open_flutter_device_selector_with_items(items);
            app.request_redraw();
        }
        WorkerResultPayload::FlutterEmulatorLaunched => {
            app.show_transient_toast("Flutter emulator launched successfully.".to_string());
            let flutter_path = app.current_flutter_path();
            app.submit(crate::async_runtime::message::RequestSpec {
                revision_id: 0,
                topic: crate::async_runtime::message::RequestTopic::SystemTask,
                payload: crate::async_runtime::message::WorkerRequestPayload::ScanFlutterDevices {
                    flutter_path: Some(flutter_path),
                },
            });
        }
        _ => {}
    }
}

pub(super) fn handle_system_dep_tool_progress(
    app: &mut AppShell,
    tool: String,
    status: crate::async_runtime::message::InstallStatus,
) {
    let Some(guide) = app.active_system_dep_guide.as_mut() else {
        return;
    };
    if let Some(entry) = guide.tool_statuses.iter_mut().find(|(t, _)| *t == tool) {
        entry.1 = status;
    }
    app.editor_needs_layout = true;
    app.request_redraw();
}

pub(super) fn handle_system_dep_install_done(app: &mut AppShell) {
    if let Some(guide) = app.active_system_dep_guide.as_mut() {
        guide.state = SystemDepState::Complete;
    }
    app.editor_needs_layout = true;
    app.request_redraw();
}

pub(super) fn handle_extension_command_started(
    app: &mut AppShell,
    binary: String,
    uninstall: bool,
) {
    if let Some(state) = app.app_state.any_extensions_manager_buffer_mut() {
        state.start_command(binary, uninstall);
    }
    app.editor_needs_layout = true;
    app.request_redraw();
}

pub(super) fn handle_extension_command_log(app: &mut AppShell, binary: String, line: String) {
    if let Some(state) = app.app_state.any_extensions_manager_buffer_mut() {
        let _ = state.push_command_log(&binary, line);
    }
    app.editor_needs_layout = true;
    app.request_redraw();
}

pub(super) fn handle_extension_command_finished(
    app: &mut AppShell,
    binary: String,
    uninstall: bool,
    success: bool,
    exit_code: Option<i32>,
) {
    if let Some(state) = app.app_state.any_extensions_manager_buffer_mut() {
        let _ = state.finish_command(&binary, uninstall, success, exit_code);
        if success {
            for item in &mut state.items {
                if item.binary == binary {
                    item.installed = !uninstall;
                }
            }
        }
    }
    app.submit(RequestSpec {
        revision_id: 0,
        topic: RequestTopic::SystemDepCheck,
        payload: WorkerRequestPayload::CheckSystemDeps,
    });
    app.pending_lsp_server = None;
    app.lsp_retry_at = Some(std::time::Instant::now() + std::time::Duration::from_secs(3));
    let action = if uninstall { "Uninstall" } else { "Install" };
    if success {
        app.show_transient_toast_kind(
            format!("{action} complete\n{binary} finished successfully"),
            ToastKind::Success,
        );
    } else {
        app.show_transient_toast_kind(
            format!("{action} failed\n{binary} exited with {:?}", exit_code),
            ToastKind::Error,
        );
    }
    app.editor_needs_layout = true;
    app.request_redraw();
}
