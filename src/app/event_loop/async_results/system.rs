use super::super::*;
use crate::async_runtime::message::WorkerResultPayload;

pub(super) fn handle_system_result(app: &mut AppShell, payload: WorkerResultPayload) {
    match payload {
        WorkerResultPayload::SystemDepCheckResult { missing } => {
            let missing_names: Vec<String> = missing.iter().map(|s| s.to_string()).collect();
            if let Some(state) = app.app_state.active_extensions_manager_buffer_mut() {
                for item in &mut state.items {
                    if item.category == crate::app::app_state::ExtensionCategory::CliTools {
                        item.installed = !missing_names.iter().any(|tool| tool == &item.binary);
                    }
                }
            }
            if missing.is_empty() {
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
                format!("brew install {}", missing.join(" "))
            } else {
                format!("sudo apt-get install -y {}", missing.join(" "))
            };
            let tool_statuses = missing_names
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
                missing_tools: Some(missing_names),
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
            // Logic handled in command palette / config if needed, but here we just store if there was a field.
            // AppShell doesn't seem to store the full list of envs, just the selected one.
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
    if let Some(state) = app.app_state.active_extensions_manager_buffer_mut() {
        state.start_command(binary, uninstall);
    }
    app.editor_needs_layout = true;
    app.request_redraw();
}

pub(super) fn handle_extension_command_log(app: &mut AppShell, binary: String, line: String) {
    if let Some(state) = app.app_state.active_extensions_manager_buffer_mut() {
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
    if let Some(state) = app.app_state.active_extensions_manager_buffer_mut() {
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
