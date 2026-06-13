use super::*;
use crate::core::commands::Command;
use crate::workbench::focus_manager::FocusTarget;
use crate::workbench::panel_state::PanelTabId;
use std::path::{Path, PathBuf};
use std::sync::Arc;

fn resolve_flutter_path(dart_path: &Path) -> PathBuf {
    let dart_str = dart_path.to_string_lossy();
    if dart_str.contains("bin/cache/dart-sdk/bin/dart") {
        let flutter_str = dart_str.replace("bin/cache/dart-sdk/bin/dart", "bin/flutter");
        let path = PathBuf::from(flutter_str);
        if path.exists() {
            return path;
        }
    }
    if let Some(parent) = dart_path.parent() {
        let flutter_same_dir = parent.join("flutter");
        if flutter_same_dir.exists() {
            return flutter_same_dir;
        }
        if let Some(grandparent) = parent.parent() {
            let flutter_grand = grandparent.join("flutter");
            if flutter_grand.exists() {
                return flutter_grand;
            }
            let flutter_grand_bin = grandparent.join("bin").join("flutter");
            if flutter_grand_bin.exists() {
                return flutter_grand_bin;
            }
        }
    }
    PathBuf::from("flutter")
}

impl AppShell {
    pub fn handle_dap_command(&mut self, command: &Command) -> Option<bool> {
        match command {
            Command::FocusDap => {
                let mut changed = self.release_focus_mode_to_editor();
                if !self.panel_state.left.visible {
                    self.panel_state.left.visible = true;
                    changed = true;
                    self.sidebar_needs_layout = true;
                }
                if self.panel_state.left.active_tab_id() != Some(PanelTabId::Inspector) {
                    self.panel_state.left.switch_to_tab(PanelTabId::Inspector);
                    changed = true;
                    self.sidebar_needs_layout = true;
                }
                changed |= self.dismiss_initial_launch_welcome_if_active();
                let focus_changed = self.focus_manager.set(FocusTarget::LeftSidebar);
                changed |= focus_changed;
                if focus_changed {
                    self.input_handler.clear_pending_prefix();
                }
                Some(changed)
            }
            Command::DapToggleExpand => {
                if self.focus_manager.current() == FocusTarget::LeftSidebar
                    && self.panel_state.left.active_tab_id() == Some(PanelTabId::Inspector)
                {
                    let changed = self.dap_panel_state.toggle_selected_expand();
                    if changed {
                        self.sidebar_needs_layout = true;
                    }
                    Some(changed)
                } else {
                    None
                }
            }
            Command::DebugStart => {
                eprintln!(
                    "[DAP LOG] [F5 Commands DAP] handle_dap_command: entered Command::DebugStart"
                );
                let mut should_clear = false;
                if let Some(session) = &self.dap_session {
                    let (paused, terminated) = {
                        let state = session.state.try_lock();
                        state
                            .map(|s| (s.paused, s.terminated))
                            .unwrap_or((false, false))
                    };
                    eprintln!(
                        "[DAP LOG] [F5 Commands DAP] DebugStart with existing session, paused={}, terminated={}",
                        paused, terminated
                    );
                    if terminated {
                        should_clear = true;
                    } else {
                        if paused {
                            let session_clone = session.clone();
                            let window_clone = self.window.clone();
                            let _guard = self.scheduler.enter();
                            tokio::spawn(async move {
                                let _ = session_clone.resume().await;
                                if let Some(w) = window_clone {
                                    w.request_redraw();
                                }
                            });
                            return Some(true);
                        }
                        return Some(false);
                    }
                }
                if should_clear {
                    self.dap_session = None;
                }

                // Launch new session — try launch.json first, then fallback to Flutter defaults
                let workspace_root = self
                    .app_state
                    .workspace_root_path()
                    .map(PathBuf::from)
                    .or_else(|| {
                        self.app_state
                            .active_file()
                            .and_then(|file| file.parent().map(PathBuf::from))
                    })
                    .unwrap_or_default();
                eprintln!(
                    "[DAP LOG] [F5 Commands DAP] workspace_root: {:?}",
                    workspace_root
                );

                if workspace_root.as_os_str().is_empty() {
                    eprintln!(
                        "[DAP LOG] [F5 Commands DAP] No workspace_root found, returning false"
                    );
                    self.show_transient_toast_kind(
                        "No active workspace or file parent folder",
                        ToastKind::Error,
                    );
                    return Some(false);
                }

                self.show_transient_toast("Starting debugger...".to_string());

                // Try to load launch.json (.vscode/launch.json or .zed/debug.json)
                let launch_config = crate::dap::launch_config::load_launch_json(&workspace_root)
                    .and_then(|cfg| {
                        eprintln!(
                            "[DebugStart] Found launch.json with {} configurations",
                            cfg.configurations.len()
                        );
                        cfg.configurations.into_iter().next()
                    });

                if launch_config.is_none() {
                    eprintln!("[DebugStart] No launch.json found, using Flutter defaults");
                }

                let (program, adapter_cmd, adapter_args, adapter_id) = if let Some(ref config) =
                    launch_config
                {
                    let resolved = config.resolve(&workspace_root);
                    let program = if resolved.program.is_empty() {
                        workspace_root
                            .join("lib")
                            .join("main.dart")
                            .to_string_lossy()
                            .to_string()
                    } else {
                        resolved.program.clone()
                    };
                    match config.config_type.as_str() {
                        "dart" | "flutter" => {
                            // Check if FVM is available - prefer fvm flutter over direct flutter
                            let fvm_path = Self::resolve_fvm_path();
                            let has_local_fvm =
                                workspace_root.join(".fvm").join("flutter_sdk").exists();

                            let (adapter_cmd, adapter_args) = if has_local_fvm {
                                // Use FVM wrapper for local FVM projects
                                if let Some(ref fvm) = fvm_path {
                                    eprintln!("[DebugStart] Using FVM wrapper: {:?}", fvm);
                                    (
                                        fvm.to_string_lossy().to_string(),
                                        vec!["flutter".to_string(), "debug_adapter".to_string()],
                                    )
                                } else {
                                    // FVM not found in PATH, fallback to direct flutter
                                    let dart_bin = self.selected_dart_env.clone().or_else(|| {
                                        let local_fvm = workspace_root
                                            .join(".fvm")
                                            .join("flutter_sdk")
                                            .join("bin")
                                            .join("cache")
                                            .join("dart-sdk")
                                            .join("bin")
                                            .join("dart");
                                        if local_fvm.try_exists().unwrap_or(false) {
                                            Some(local_fvm)
                                        } else {
                                            None
                                        }
                                    });
                                    let flutter_path = dart_bin
                                        .as_ref()
                                        .map(|d| resolve_flutter_path(d))
                                        .unwrap_or_else(|| PathBuf::from("flutter"));
                                    (
                                        flutter_path.to_string_lossy().to_string(),
                                        vec!["debug_adapter".to_string()],
                                    )
                                }
                            } else {
                                // No local FVM, use direct flutter
                                let dart_bin = self.selected_dart_env.clone().or_else(|| {
                                    let local_fvm = workspace_root
                                        .join(".fvm")
                                        .join("flutter_sdk")
                                        .join("bin")
                                        .join("cache")
                                        .join("dart-sdk")
                                        .join("bin")
                                        .join("dart");
                                    if local_fvm.try_exists().unwrap_or(false) {
                                        Some(local_fvm)
                                    } else {
                                        None
                                    }
                                });
                                let flutter_path = dart_bin
                                    .as_ref()
                                    .map(|d| resolve_flutter_path(d))
                                    .unwrap_or_else(|| PathBuf::from("flutter"));
                                (
                                    flutter_path.to_string_lossy().to_string(),
                                    vec!["debug_adapter".to_string()],
                                )
                            };

                            (program, adapter_cmd, adapter_args, "flutter".to_string())
                        }
                        "lldb" | "codelldb" => {
                            (program, "codelldb".to_string(), vec![], "lldb".to_string())
                        }
                        "python" | "debugpy" => (
                            program,
                            "python".to_string(),
                            vec![
                                "-m".to_string(),
                                "debugpy".to_string(),
                                "--adapter".to_string(),
                            ],
                            "debugpy".to_string(),
                        ),
                        _ => {
                            // Generic adapter — use config type as adapter ID
                            (
                                program,
                                config.config_type.clone(),
                                vec![],
                                config.config_type.clone(),
                            )
                        }
                    }
                } else {
                    // No launch.json — fallback to Flutter defaults with FVM support
                    let fvm_path = Self::resolve_fvm_path();
                    let has_local_fvm = workspace_root.join(".fvm").join("flutter_sdk").exists();

                    let (adapter_cmd, adapter_args) = if has_local_fvm {
                        // Use FVM wrapper for local FVM projects
                        if let Some(ref fvm) = fvm_path {
                            eprintln!("[DebugStart] Using FVM wrapper (no launch.json): {:?}", fvm);
                            (
                                fvm.to_string_lossy().to_string(),
                                vec!["flutter".to_string(), "debug_adapter".to_string()],
                            )
                        } else {
                            // FVM not found in PATH, fallback to direct flutter
                            let dart_bin = self.selected_dart_env.clone().or_else(|| {
                                let local_fvm = workspace_root
                                    .join(".fvm")
                                    .join("flutter_sdk")
                                    .join("bin")
                                    .join("cache")
                                    .join("dart-sdk")
                                    .join("bin")
                                    .join("dart");
                                if local_fvm.try_exists().unwrap_or(false) {
                                    Some(local_fvm)
                                } else {
                                    None
                                }
                            });
                            let flutter_path = dart_bin
                                .as_ref()
                                .map(|d| resolve_flutter_path(d))
                                .unwrap_or_else(|| PathBuf::from("flutter"));
                            (
                                flutter_path.to_string_lossy().to_string(),
                                vec!["debug_adapter".to_string()],
                            )
                        }
                    } else {
                        // No local FVM, use direct flutter
                        let dart_bin = self.selected_dart_env.clone().or_else(|| {
                            let local_fvm = workspace_root
                                .join(".fvm")
                                .join("flutter_sdk")
                                .join("bin")
                                .join("cache")
                                .join("dart-sdk")
                                .join("bin")
                                .join("dart");
                            if local_fvm.try_exists().unwrap_or(false) {
                                Some(local_fvm)
                            } else {
                                None
                            }
                        });
                        let flutter_path = dart_bin
                            .as_ref()
                            .map(|d| resolve_flutter_path(d))
                            .unwrap_or_else(|| PathBuf::from("flutter"));
                        (
                            flutter_path.to_string_lossy().to_string(),
                            vec!["debug_adapter".to_string()],
                        )
                    };

                    let program = self
                        .app_state
                        .active_file()
                        .and_then(|id| {
                            if id.extension().map(|ext| ext == "dart").unwrap_or(false) {
                                Some(id.to_string_lossy().to_string())
                            } else {
                                None
                            }
                        })
                        .unwrap_or_else(|| {
                            let main_dart = workspace_root.join("lib").join("main.dart");
                            if main_dart.exists() {
                                main_dart.to_string_lossy().to_string()
                            } else {
                                "lib/main.dart".to_string()
                            }
                        });
                    (program, adapter_cmd, adapter_args, "flutter".to_string())
                };

                eprintln!(
                    "[DebugStart] adapter_cmd={}, adapter_args={:?}, adapter_id={}, program={}",
                    adapter_cmd, adapter_args, adapter_id, program
                );

                // Build FVM environment variables if using FVM
                let fvm_env = if adapter_args.len() >= 2
                    && adapter_args[0] == "flutter"
                    && adapter_args[1] == "debug_adapter"
                {
                    // Using FVM wrapper - set FLUTTER_ROOT to the local SDK
                    let flutter_sdk = workspace_root.join(".fvm").join("flutter_sdk");
                    if flutter_sdk.exists() {
                        let mut env = std::collections::HashMap::new();
                        env.insert(
                            "FLUTTER_ROOT".to_string(),
                            flutter_sdk.to_string_lossy().to_string(),
                        );
                        // Also add flutter/bin to PATH
                        let flutter_bin = flutter_sdk.join("bin");
                        if let Ok(path) = std::env::var("PATH") {
                            let new_path = format!("{}:{}", flutter_bin.display(), path);
                            env.insert("PATH".to_string(), new_path);
                        }
                        eprintln!(
                            "[DebugStart] Setting FVM env: FLUTTER_ROOT={}",
                            flutter_sdk.display()
                        );
                        Some(env)
                    } else {
                        None
                    }
                } else {
                    None
                };

                let (event_tx, mut event_rx) = tokio::sync::mpsc::unbounded_channel();
                let _guard = self.scheduler.enter();
                match crate::dap::client::DapClient::launch_with_env(
                    &adapter_cmd,
                    &adapter_args,
                    Some(workspace_root.clone()),
                    event_tx,
                    fvm_env,
                ) {
                    Ok(client) => {
                        let session = Arc::new(crate::dap::DapSession::new(client));
                        self.dap_session = Some(session.clone());

                        // Open Debug Console tab in bottom dock and focus it
                        self.panel_state.bottom.visible = true;
                        self.panel_state
                            .bottom
                            .switch_to_tab(PanelTabId::DebugConsole);

                        // Open Left Sidebar and switch to DAP (Inspector)
                        if !self.panel_state.left.visible {
                            self.panel_state.left.visible = true;
                        }
                        if self.panel_state.left.active_tab_id() != Some(PanelTabId::Inspector) {
                            self.panel_state.left.switch_to_tab(PanelTabId::Inspector);
                        }

                        self.sidebar_needs_layout = true;

                        // Build launch arguments from config or defaults
                        let launch_args = if let Some(ref config) = launch_config {
                            let resolved = config.resolve(&workspace_root);
                            let program_to_use = if resolved.program.is_empty() {
                                program.clone()
                            } else {
                                resolved.program.clone()
                            };
                            let mut args = serde_json::json!({
                                "request": "launch",
                                "program": program_to_use,
                                "noDebug": false,
                            });
                            if !resolved.args.is_empty() {
                                args["args"] = serde_json::json!(resolved.args);
                            }
                            if !resolved.tool_args.is_empty() {
                                args["toolArgs"] = serde_json::json!(resolved.tool_args);
                            }
                            if let Some(device_id) = &resolved.device_id {
                                args["deviceId"] = serde_json::json!(device_id);
                            }
                            if let Some(cwd) = &resolved.cwd {
                                args["cwd"] = serde_json::json!(cwd.to_string_lossy());
                            }
                            if !resolved.env.is_empty() {
                                args["env"] = serde_json::json!(resolved.env);
                            }
                            // Merge custom/extra fields from the launch configuration to be fully compatible with VS Code
                            if let Some(obj) = args.as_object_mut() {
                                for (k, v) in &config.extra {
                                    if let Some(k_str) = k.as_str() {
                                        obj.insert(k_str.to_string(), v.clone());
                                    }
                                }
                            }
                            eprintln!("[DebugStart] Using launch.json config: {:?}", config.name);
                            args
                        } else {
                            let device_id = self
                                .app_state
                                .active_dart_device_id()
                                .unwrap_or("chrome")
                                .to_string();
                            eprintln!(
                                "[DebugStart] Using Flutter defaults: program={}, device={}",
                                program, device_id
                            );
                            serde_json::json!({
                                "request": "launch",
                                "program": program,
                                "toolArgs": ["-d", device_id],
                                "noDebug": false,
                            })
                        };
                        eprintln!("[DebugStart] launch_args: {}", launch_args);

                        let adapter_id_clone = adapter_id.clone();
                        let session_clone = session.clone();
                        let window_clone = self.window.clone();
                        tokio::spawn(async move {
                            if let Err(e) = session_clone.initialize(&adapter_id_clone).await {
                                eprintln!("DAP Init error: {}", e);
                                return;
                            }
                            if let Err(e) = session_clone
                                .client
                                .send_request("launch", Some(launch_args))
                                .await
                            {
                                eprintln!("DAP Launch error: {}", e);
                                return;
                            }
                            let _ = session_clone
                                .client
                                .send_request("configurationDone", None)
                                .await;
                            {
                                let mut state = session_clone.state.lock().await;
                                state.paused = false;
                                state
                                    .console_messages
                                    .push("Debug session started.".to_string());
                            }
                            if let Some(w) = &window_clone {
                                w.request_redraw();
                            }
                        });

                        // Sync existing breakpoints to session
                        let grouped_bps = self.breakpoints.clone();
                        let session_bps_clone = session.clone();
                        let _guard = self.scheduler.enter();
                        tokio::spawn(async move {
                            for (path, lines) in grouped_bps {
                                let _ = session_bps_clone.set_breakpoints(&path, &lines).await;
                            }
                        });

                        let session_event_clone = session.clone();
                        let window_event_clone = self.window.clone();
                        tokio::spawn(async move {
                            while let Some(event) = event_rx.recv().await {
                                match event.event.as_str() {
                                    "stopped" => {
                                        let thread_id = event
                                            .body
                                            .as_ref()
                                            .and_then(|b| b.get("threadId"))
                                            .and_then(|t| t.as_i64())
                                            .unwrap_or(1);
                                        let _ = session_event_clone
                                            .update_suspended_state(thread_id)
                                            .await;
                                        let _ =
                                            session_event_clone.evaluate_watch_expressions().await;
                                    }
                                    "continued" => {
                                        let mut state_guard =
                                            session_event_clone.state.lock().await;
                                        state_guard.paused = false;
                                        state_guard.execution_location = None;
                                    }
                                    "output" => {
                                        if let Some(body) = event.body {
                                            if let Some(output) =
                                                body.get("output").and_then(|v| v.as_str())
                                            {
                                                let mut state_guard =
                                                    session_event_clone.state.lock().await;
                                                state_guard
                                                    .console_messages
                                                    .push(output.to_string());
                                            }
                                        }
                                    }
                                    "terminated" => {
                                        let mut state_guard =
                                            session_event_clone.state.lock().await;
                                        state_guard.terminated = true;
                                        state_guard.paused = true;
                                        state_guard.execution_location = None;
                                        break;
                                    }
                                    _ => {}
                                }
                                if let Some(w) = &window_event_clone {
                                    w.request_redraw();
                                }
                            }
                        });
                        Some(true)
                    }
                    Err(err) => {
                        eprintln!("Failed to launch debug adapter: {}", err);
                        self.show_transient_toast_kind(
                            format!("Failed to launch debug adapter: {}", err),
                            ToastKind::Error,
                        );
                        Some(false)
                    }
                }
            }
            Command::DebugStop => {
                if let Some(session) = &self.dap_session {
                    let session_clone = session.clone();
                    let window_clone = self.window.clone();
                    let _guard = self.scheduler.enter();
                    tokio::spawn(async move {
                        let _ = session_clone.stop().await;
                        if let Some(w) = window_clone {
                            w.request_redraw();
                        }
                    });
                    self.dap_session = None;
                    Some(true)
                } else {
                    None
                }
            }
            Command::DebugContinue => {
                if let Some(session) = &self.dap_session {
                    let session_clone = session.clone();
                    let window_clone = self.window.clone();
                    let _guard = self.scheduler.enter();
                    tokio::spawn(async move {
                        let _ = session_clone.resume().await;
                        if let Some(w) = window_clone {
                            w.request_redraw();
                        }
                    });
                    Some(true)
                } else {
                    None
                }
            }
            Command::DebugStepOver => {
                if let Some(session) = &self.dap_session {
                    let session_clone = session.clone();
                    let window_clone = self.window.clone();
                    let _guard = self.scheduler.enter();
                    tokio::spawn(async move {
                        let _ = session_clone.step_over().await;
                        if let Some(w) = window_clone {
                            w.request_redraw();
                        }
                    });
                    Some(true)
                } else {
                    None
                }
            }
            Command::DebugStepInto => {
                if let Some(session) = &self.dap_session {
                    let session_clone = session.clone();
                    let window_clone = self.window.clone();
                    let _guard = self.scheduler.enter();
                    tokio::spawn(async move {
                        let _ = session_clone.step_into().await;
                        if let Some(w) = window_clone {
                            w.request_redraw();
                        }
                    });
                    Some(true)
                } else {
                    None
                }
            }
            Command::DebugStepOut => {
                if let Some(session) = &self.dap_session {
                    let session_clone = session.clone();
                    let window_clone = self.window.clone();
                    let _guard = self.scheduler.enter();
                    tokio::spawn(async move {
                        let _ = session_clone.step_out().await;
                        if let Some(w) = window_clone {
                            w.request_redraw();
                        }
                    });
                    Some(true)
                } else {
                    None
                }
            }
            Command::DebugToggleBreakpoint => {
                if let Some(active_file) = self.app_state.active_file() {
                    let (line, _) = self.app_state.cursor_line_col();
                    let path = active_file
                        .canonicalize()
                        .unwrap_or_else(|_| active_file.to_path_buf());
                    // Toggle in our local breakpoints map
                    let is_empty = {
                        let lines = self.breakpoints.entry(path.clone()).or_default();
                        if let Some(pos) = lines.iter().position(|&l| l == line) {
                            lines.remove(pos);
                        } else {
                            lines.push(line);
                        }
                        lines.is_empty()
                    };
                    if is_empty {
                        self.breakpoints.remove(&path);
                    }
                    let lines_clone = self.breakpoints.get(&path).cloned().unwrap_or_default();
                    self.save_breakpoints();

                    if let Some(session) = &self.dap_session {
                        if let Ok(mut state_guard) = session.state.try_lock() {
                            state_guard.toggle_breakpoint_at_line(&path, line);
                        }
                        let session_clone = session.clone();
                        let _guard = self.scheduler.enter();
                        tokio::spawn(async move {
                            let _ = session_clone.set_breakpoints(&path, &lines_clone).await;
                        });
                    }
                    self.sidebar_needs_layout = true;
                    self.editor_needs_layout = true;
                    self.editor_caret_needs_layout = true;
                    if let Some(w) = &self.window {
                        w.request_redraw();
                    }
                    Some(true)
                } else {
                    None
                }
            }
            Command::FlutterHotReload => {
                if let Some(session) = &self.dap_session {
                    let session_clone = session.clone();
                    let _guard = self.scheduler.enter();
                    tokio::spawn(async move {
                        let _ = session_clone.hot_reload().await;
                    });
                    Some(true)
                } else {
                    None
                }
            }
            Command::FlutterHotRestart => {
                if let Some(session) = &self.dap_session {
                    let session_clone = session.clone();
                    let _guard = self.scheduler.enter();
                    tokio::spawn(async move {
                        let _ = session_clone.hot_restart().await;
                    });
                    Some(true)
                } else {
                    None
                }
            }
            Command::DebugWatchAdd => {
                if self.dap_session.is_some() {
                    let _ = self.app_state.open_command_palette_mode(
                        crate::app::command_palette::CommandPaletteMode::DebugWatchInput,
                    );
                    Some(true)
                } else {
                    None
                }
            }
            Command::DebugWatchRemove => {
                if let Some(session) = &self.dap_session {
                    let rows = self.dap_panel_state.visible_rows();
                    if let Some(row) = rows.get(self.dap_panel_state.selected_row) {
                        if row.section_index == 1 {
                            // Watch section
                            if let Some(path) = &row.node_path {
                                if let Some(node_index) = path.first() {
                                    if let Ok(mut state) = session.state.try_lock() {
                                        if state.remove_watch_expression(*node_index) {
                                            drop(state);
                                            if let Ok(state_guard) = session.state.try_lock() {
                                                self.dap_panel_state
                                                    .sync_from_debug_state(&state_guard);
                                            }
                                            self.sidebar_needs_layout = true;
                                            return Some(true);
                                        }
                                    }
                                }
                            }
                        }
                    }
                    Some(false)
                } else {
                    None
                }
            }
            Command::DebugGotoFrame => {
                if let Some(session) = &self.dap_session {
                    let rows = self.dap_panel_state.visible_rows();
                    if let Some(row) = rows.get(self.dap_panel_state.selected_row) {
                        if row.section_index == 2 {
                            // Call Stack section
                            if let Some(path) = &row.node_path {
                                if let Some(node_index) = path.first() {
                                    if let Ok(state) = session.state.try_lock() {
                                        if let Some(frame) = state.call_stack.get(*node_index) {
                                            let file_path = frame.path.clone();
                                            let line = frame.location.line;
                                            drop(state);
                                            if !file_path.as_os_str().is_empty() {
                                                let _ = self.app_state.open_file(file_path);
                                                self.app_state.jump_to_line(line);
                                                return Some(true);
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                    Some(false)
                } else {
                    None
                }
            }
            Command::FlutterDevices => {
                self.app_state.open_flutter_device_selector();
                let flutter_path = self.current_flutter_path();
                self.submit(crate::async_runtime::message::RequestSpec {
                    revision_id: 0,
                    topic: crate::async_runtime::message::RequestTopic::SystemTask,
                    payload:
                        crate::async_runtime::message::WorkerRequestPayload::ScanFlutterDevices {
                            flutter_path: Some(flutter_path),
                        },
                });
                Some(true)
            }
            _ => None,
        }
    }

    pub(super) fn confirm_flutter_device_selection(&mut self) -> bool {
        use crate::app::command_palette::CommandPaletteAction;
        if let Some(CommandPaletteAction::SelectFlutterDevice {
            device_id,
            is_emulator,
            is_active,
        }) = self.app_state.command_palette_selected_action()
        {
            self.app_state
                .set_active_dart_device_id(Some(device_id.clone()));
            if is_emulator && !is_active {
                let flutter_path = self.current_flutter_path();
                self.show_transient_toast(format!("Launching emulator {}...", device_id));
                self.submit(crate::async_runtime::message::RequestSpec {
                    revision_id: 0,
                    topic: crate::async_runtime::message::RequestTopic::SystemTask,
                    payload:
                        crate::async_runtime::message::WorkerRequestPayload::LaunchFlutterEmulator {
                            flutter_path: Some(flutter_path),
                            emulator_id: device_id.clone(),
                        },
                });
            }
            true
        } else {
            false
        }
    }

    fn resolve_fvm_path() -> Option<PathBuf> {
        if let Ok(home) = std::env::var("HOME") {
            for relative in &["fvm/bin/fvm", ".fvm/bin/fvm", ".pub-cache/bin/fvm"] {
                let p = PathBuf::from(&home).join(relative);
                if p.exists() {
                    return Some(p);
                }
            }
        }
        for p in &[
            "/opt/homebrew/bin/fvm",
            "/usr/local/bin/fvm",
            "/usr/bin/fvm",
        ] {
            let p_path = PathBuf::from(p);
            if p_path.exists() {
                return Some(p_path);
            }
        }
        None
    }

    pub fn current_flutter_path(&self) -> PathBuf {
        let workspace_root = self
            .app_state
            .workspace_root_path()
            .map(PathBuf::from)
            .unwrap_or_default();
        let dart_bin = self.selected_dart_env.clone().or_else(|| {
            let local_fvm = workspace_root
                .join(".fvm")
                .join("flutter_sdk")
                .join("bin")
                .join("cache")
                .join("dart-sdk")
                .join("bin")
                .join("dart");
            if local_fvm.try_exists().unwrap_or(false) {
                Some(local_fvm)
            } else {
                None
            }
        });

        if let Some(d) = dart_bin.as_ref() {
            resolve_flutter_path(d)
        } else if let Some(fvm_path) = Self::resolve_fvm_path() {
            fvm_path
        } else {
            PathBuf::from("flutter")
        }
    }

    pub fn load_breakpoints(&mut self) {
        let workspace_root = self
            .app_state
            .workspace_root_path()
            .map(PathBuf::from)
            .or_else(|| {
                self.app_state
                    .active_file()
                    .and_then(|file| file.parent().map(PathBuf::from))
            });
        if let Some(root) = workspace_root {
            let bp_path = root.join(".vscode").join("breakpoints.json");
            if bp_path.exists() {
                if let Ok(content) = std::fs::read_to_string(&bp_path) {
                    if let Ok(bps) = serde_json::from_str::<
                        std::collections::HashMap<PathBuf, Vec<usize>>,
                    >(&content)
                    {
                        self.breakpoints = bps
                            .into_iter()
                            .map(|(p, l)| (p.canonicalize().unwrap_or(p), l))
                            .collect();
                        eprintln!(
                            "[DAP LOG] [Breakpoint Persist] Loaded breakpoints: {:?}",
                            self.breakpoints
                        );
                    }
                }
            }
        }
    }

    pub fn save_breakpoints(&self) {
        let workspace_root = self
            .app_state
            .workspace_root_path()
            .map(PathBuf::from)
            .or_else(|| {
                self.app_state
                    .active_file()
                    .and_then(|file| file.parent().map(PathBuf::from))
            });
        if let Some(root) = workspace_root {
            let vscode_dir = root.join(".vscode");
            if !vscode_dir.exists() {
                let _ = std::fs::create_dir_all(&vscode_dir);
            }
            let bp_path = vscode_dir.join("breakpoints.json");
            if let Ok(content) = serde_json::to_string_pretty(&self.breakpoints) {
                if let Err(e) = std::fs::write(&bp_path, content) {
                    eprintln!("Failed to write breakpoints.json: {}", e);
                }
            }
        }
    }
}
