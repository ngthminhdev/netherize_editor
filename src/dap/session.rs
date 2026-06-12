use serde_json::json;
use std::path::Path;
use std::sync::{Arc, Mutex};

use crate::dap::client::DapClient;
use crate::dap::types::{
    Breakpoint as DapBreakpoint, Scope as DapScope, SourceBreakpoint, StackFrame as DapStackFrame,
    Variable as DapVariable,
};
use crate::workbench::debug_state::{
    Breakpoint as EditorBreakpoint, DebugSharedState, DebugVariable, SourceLocation,
    StackFrame as EditorStackFrame,
};

pub struct DapSession {
    pub client: Arc<DapClient>,
    pub state: Arc<tokio::sync::Mutex<DebugSharedState>>,
    pub active_thread_id: Arc<Mutex<Option<i64>>>,
}

impl DapSession {
    pub fn new(client: Arc<DapClient>) -> Self {
        Self {
            client,
            state: Arc::new(tokio::sync::Mutex::new(DebugSharedState::default())),
            active_thread_id: Arc::new(Mutex::new(None)),
        }
    }

    pub async fn initialize(&self, adapter_id: &str) -> Result<(), String> {
        let args = json!({
            "clientID": "netherize",
            "clientName": "Netherize Editor",
            "adapterID": adapter_id,
            "linesStartAt1": true,
            "columnsStartAt1": true,
            "pathFormat": "path"
        });

        self.client.send_request("initialize", Some(args)).await?;
        Ok(())
    }

    pub async fn launch_flutter(&self, program: &str, device_id: &str) -> Result<(), String> {
        let args = json!({
            "request": "launch",
            "program": program,
            "toolArgs": ["-d", device_id],
            "noDebug": false
        });

        self.client.send_request("launch", Some(args)).await?;
        // Standard configurationDone to let the adapter know initialization is finished
        self.client.send_request("configurationDone", None).await?;

        let mut state_guard = self.state.lock().await;
        state_guard.paused = false;
        state_guard
            .console_messages
            .push("Flutter application launching...".to_string());

        Ok(())
    }

    pub async fn set_breakpoints(&self, path: &Path, lines: &[usize]) -> Result<(), String> {
        let source_path = path.to_string_lossy().to_string();
        let breakpoints: Vec<SourceBreakpoint> = lines
            .iter()
            .map(|&l| SourceBreakpoint {
                line: (l + 1) as i64, // Convert 0-indexed to 1-indexed
                column: None,
            })
            .collect();

        let args = json!({
            "source": {
                "name": path.file_name().map(|f| f.to_string_lossy().to_string()),
                "path": source_path,
            },
            "breakpoints": breakpoints
        });

        let resp = self
            .client
            .send_request("setBreakpoints", Some(args))
            .await?;
        if resp.success {
            if let Some(body) = resp.body {
                if let Ok(dap_bps) =
                    serde_json::from_value::<Vec<DapBreakpoint>>(body["breakpoints"].clone())
                {
                    let mut state_guard = self.state.lock().await;
                    state_guard.breakpoints.clear();
                    for (i, dap_bp) in dap_bps.into_iter().enumerate() {
                        if dap_bp.verified {
                            if let Some(line) = dap_bp.line {
                                state_guard.breakpoints.push(EditorBreakpoint {
                                    id: dap_bp.id.unwrap_or(i as i64) as u64,
                                    location: SourceLocation {
                                        line: (line - 1) as usize, // 1-indexed to 0-indexed
                                        column: 0,
                                    },
                                    enabled: true,
                                    path: path
                                        .canonicalize()
                                        .unwrap_or_else(|_| path.to_path_buf()),
                                });
                            }
                        }
                    }
                }
            }
        }
        Ok(())
    }

    pub async fn resume(&self) -> Result<(), String> {
        let thread_id = { *self.active_thread_id.lock().unwrap() };
        if let Some(tid) = thread_id {
            let args = json!({ "threadId": tid });
            self.client.send_request("continue", Some(args)).await?;
            let mut state_guard = self.state.lock().await;
            state_guard.paused = false;
            state_guard.execution_location = None;
        }
        Ok(())
    }

    pub async fn step_over(&self) -> Result<(), String> {
        let thread_id = { *self.active_thread_id.lock().unwrap() };
        if let Some(tid) = thread_id {
            let args = json!({ "threadId": tid });
            self.client.send_request("next", Some(args)).await?;
        }
        Ok(())
    }

    pub async fn step_into(&self) -> Result<(), String> {
        let thread_id = { *self.active_thread_id.lock().unwrap() };
        if let Some(tid) = thread_id {
            let args = json!({ "threadId": tid });
            self.client.send_request("stepIn", Some(args)).await?;
        }
        Ok(())
    }

    pub async fn step_out(&self) -> Result<(), String> {
        let thread_id = { *self.active_thread_id.lock().unwrap() };
        if let Some(tid) = thread_id {
            let args = json!({ "threadId": tid });
            self.client.send_request("stepOut", Some(args)).await?;
        }
        Ok(())
    }

    pub async fn stop(&self) -> Result<(), String> {
        self.client.send_request("disconnect", None).await?;
        let mut state_guard = self.state.lock().await;
        state_guard.paused = true;
        state_guard.execution_location = None;
        state_guard
            .console_messages
            .push("Debugger session stopped.".to_string());
        Ok(())
    }

    pub async fn hot_reload(&self) -> Result<(), String> {
        let args = json!({
            "request": "hotReload"
        });
        self.client
            .send_request("customRequest", Some(args))
            .await?;
        let mut state_guard = self.state.lock().await;
        state_guard
            .console_messages
            .push("⚡ Hot Reload triggered...".to_string());
        Ok(())
    }

    pub async fn hot_restart(&self) -> Result<(), String> {
        let args = json!({
            "request": "hotRestart"
        });
        self.client
            .send_request("customRequest", Some(args))
            .await?;
        let mut state_guard = self.state.lock().await;
        state_guard
            .console_messages
            .push("↻ Hot Restart triggered...".to_string());
        Ok(())
    }

    pub async fn evaluate_watch_expressions(&self) -> Result<(), String> {
        let expressions: Vec<String> = {
            let state_guard = self.state.lock().await;
            state_guard
                .watch
                .iter()
                .map(|w| w.expression.clone())
                .collect()
        };

        let mut results = Vec::new();
        for expr in expressions {
            let args = json!({
                "expression": expr,
                "context": "watch"
            });
            if let Ok(resp) = self.client.send_request("evaluate", Some(args)).await {
                if resp.success {
                    if let Some(body) = resp.body {
                        let value = body
                            .get("result")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string();
                        results.push((expr, value));
                    }
                } else {
                    results.push((expr, "<error>".to_string()));
                }
            }
        }

        let mut state_guard = self.state.lock().await;
        state_guard.update_watch_values(results);
        Ok(())
    }

    // ── Update suspended state when paused ─────────────────────────────────────

    pub async fn update_suspended_state(&self, thread_id: i64) -> Result<(), String> {
        {
            let mut tid_guard = self.active_thread_id.lock().unwrap();
            *tid_guard = Some(thread_id);
        }

        // 1. Fetch Call Stack
        let args = json!({ "threadId": thread_id });
        let resp = self.client.send_request("stackTrace", Some(args)).await?;
        let mut call_stack = Vec::new();
        let mut current_loc = None;
        let mut top_frame_id = None;

        if resp.success
            && let Some(body) = resp.body
        {
            if let Ok(frames) =
                serde_json::from_value::<Vec<DapStackFrame>>(body["stackFrames"].clone())
            {
                for (idx, frame) in frames.into_iter().enumerate() {
                    let line = (frame.line - 1).max(0) as usize; // 1-indexed to 0-indexed
                    let col = (frame.column - 1).max(0) as usize;

                    let file_label = frame
                        .source
                        .as_ref()
                        .and_then(|s| s.path.as_ref())
                        .and_then(|p| Path::new(p).file_name())
                        .map(|n| n.to_string_lossy().to_string())
                        .unwrap_or_else(|| "unknown".to_string());

                    let path = frame
                        .source
                        .as_ref()
                        .and_then(|s| s.path.as_ref())
                        .map(Path::new)
                        .map(|p| p.to_path_buf())
                        .unwrap_or_default();

                    if idx == 0 {
                        top_frame_id = Some(frame.id);
                        current_loc = Some(SourceLocation { line, column: col });
                    }

                    call_stack.push(EditorStackFrame {
                        function: format!("{} ({})", frame.name, file_label),
                        location: SourceLocation { line, column: col },
                        path,
                    });
                }
            }
        }

        // 2. Fetch Variables for the Top Frame
        let mut variables = Vec::new();
        if let Some(frame_id) = top_frame_id {
            let args = json!({ "frameId": frame_id });
            let resp = self.client.send_request("scopes", Some(args)).await?;
            if resp.success
                && let Some(body) = resp.body
            {
                if let Ok(scopes) = serde_json::from_value::<Vec<DapScope>>(body["scopes"].clone())
                {
                    // Iterate scopes (usually Local, Global)
                    for scope in scopes {
                        if scope.variables_reference > 0 {
                            let var_args =
                                json!({ "variablesReference": scope.variables_reference });
                            let var_resp = self
                                .client
                                .send_request("variables", Some(var_args))
                                .await?;
                            if var_resp.success
                                && let Some(var_body) = var_resp.body
                            {
                                if let Ok(dap_vars) = serde_json::from_value::<Vec<DapVariable>>(
                                    var_body["variables"].clone(),
                                ) {
                                    for d_var in dap_vars {
                                        variables.push(self.fetch_variable_tree(d_var).await);
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        // 3. Update active state
        {
            let mut state_guard = self.state.lock().await;
            state_guard.paused = true;
            state_guard.execution_location = current_loc;
            state_guard.call_stack = call_stack;
            state_guard.variables = variables;
        }

        Ok(())
    }

    // Lazily evaluate/fetch nested object properties (children)
    async fn fetch_variable_tree(&self, var: DapVariable) -> DebugVariable {
        let mut children = Vec::new();
        if var.variables_reference > 0 {
            let args = json!({ "variablesReference": var.variables_reference });
            if let Ok(resp) = self.client.send_request("variables", Some(args)).await {
                if resp.success
                    && let Some(body) = resp.body
                {
                    if let Ok(dap_vars) =
                        serde_json::from_value::<Vec<DapVariable>>(body["variables"].clone())
                    {
                        for d_var in dap_vars {
                            // Don't recursively fetch indefinitely (fetch 1 level down)
                            children.push(DebugVariable {
                                name: d_var.name,
                                value: d_var.value,
                                children: Vec::new(),
                            });
                        }
                    }
                }
            }
        }

        DebugVariable {
            name: var.name,
            value: var.value,
            children,
        }
    }
}
