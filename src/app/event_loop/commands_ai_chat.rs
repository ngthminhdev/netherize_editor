use super::*;
use crate::{
    workbench::panel_state::{AiAgentMode, AiChatCodeContext, AiChatMessage, AiRole, PanelTabId},
    workspace::model::WorkspaceNodeType,
};

const AI_MODEL_PRESETS: &[&str] = &[
    "mimo-v2.5-pro",
    "anthropic/claude-sonnet-4-5",
    "anthropic/claude-opus-4-5",
];

const AI_SLASH_COMMANDS: &[&str] = &[
    "/clear", "/new", "/review", "/explain", "/fix", "/test", "/commit", "/diff", "/context",
    "/plan", "/build", "/mode", "/agent", "/model", "/models", "/status", "/compact", "/tokens",
    "/help",
];

/// Like [`ai_slash_command_completion`] but selects the Nth matching command.
fn ai_slash_command_completion_at(input: &str, index: usize) -> Option<String> {
    let rest = input.trim_start().strip_prefix('/')?;
    let query = rest
        .split_whitespace()
        .next()
        .unwrap_or(rest)
        .to_ascii_lowercase();
    let command = AI_SLASH_COMMANDS
        .iter()
        .copied()
        .filter(|command| command.trim_start_matches('/').starts_with(&query))
        .nth(index)?;
    let suffix = if matches!(
        command,
        "/model" | "/mode" | "/agent" | "/review" | "/explain" | "/fix" | "/test" | "/diff"
    ) {
        " "
    } else {
        ""
    };
    Some(format!("{command}{suffix}"))
}

/// Count how many slash commands match the current input (for suggestion navigation).
fn slash_command_suggestion_count(input: &str) -> usize {
    let Some(rest) = input.trim_start().strip_prefix('/') else {
        return 0;
    };
    let query = rest
        .split_whitespace()
        .next()
        .unwrap_or(rest)
        .to_ascii_lowercase();
    AI_SLASH_COMMANDS
        .iter()
        .filter(|command| command.trim_start_matches('/').starts_with(&query))
        .count()
}

fn clean_ai_file_ref_token(token: &str) -> Option<String> {
    let without_marker = token.trim().strip_prefix('@').unwrap_or(token.trim());
    let trimmed = without_marker
        .trim_matches(|ch| matches!(ch, '"' | '\'' | '`' | '<' | '>'))
        .trim_end_matches(|ch| matches!(ch, ',' | ';' | ':' | ')' | ']' | '}'));
    (!trimmed.is_empty()).then(|| trimmed.to_string())
}

fn current_ai_at_token(input: &str) -> Option<(usize, &str)> {
    let trimmed = input.trim_end();
    if trimmed.is_empty() {
        return None;
    }
    let start = trimmed
        .char_indices()
        .rev()
        .find(|(_, ch)| ch.is_whitespace())
        .map(|(idx, ch)| idx + ch.len_utf8())
        .unwrap_or(0);
    let token = &trimmed[start..];
    let query = token.strip_prefix('@')?;
    Some((start, query))
}

fn build_prompt_with_code_contexts(prompt: String, contexts: &[AiChatCodeContext]) -> String {
    if contexts.is_empty() {
        return prompt;
    }

    let mut sections = Vec::new();
    for context in contexts {
        let language = context.language_id.as_deref().unwrap_or("");
        sections.push(format!(
            "--- BEGIN SELECTED CODE: {} ---\n```{}\n{}\n```\n--- END SELECTED CODE ---",
            context.title, language, context.text
        ));
    }

    format!(
        "The user attached these selected code snippets as extra context:\n\n{}\n\nUser request:\n{}",
        sections.join("\n\n"),
        prompt
    )
}

fn ai_models_help(current: Option<&str>) -> String {
    let current = current.unwrap_or("default (opencode config)");
    let mut lines = vec![
        format!("Current model: {current}"),
        "Use /model <id> to switch, or /model default to clear the override.".to_string(),
        "Common model ids:".to_string(),
    ];
    lines.extend(AI_MODEL_PRESETS.iter().map(|model| format!("  {model}")));
    lines.join("\n")
}

fn ai_agent_help(current: AiAgentMode) -> String {
    [
        format!("Current mode: {}", current.label()),
        "Use /plan for read-only planning, or /build for normal implementation.".to_string(),
        "Plan maps to opencode's built-in plan agent.".to_string(),
    ]
    .join("\n")
}

/// Check if `opencode` is resolvable — checks $PATH then the default install
/// location (~/.opencode/bin/opencode) without spawning a process.
#[allow(dead_code)]
fn opencode_available() -> bool {
    if let Some(path_var) = std::env::var_os("PATH") {
        for dir in std::env::split_paths(&path_var) {
            if dir.join("opencode").is_file() {
                return true;
            }
        }
    }
    // Default install path from the official installer.
    if let Some(home) = std::env::var_os("HOME") {
        let candidate = std::path::PathBuf::from(home)
            .join(".opencode")
            .join("bin")
            .join("opencode");
        if candidate.is_file() {
            return true;
        }
    }
    false
}

impl AppShell {
    fn resolve_ai_chat_file_ref(&self, raw_path: &str) -> Option<PathBuf> {
        let trimmed = raw_path.trim();
        if trimmed.is_empty() || trimmed.contains('\0') {
            return None;
        }

        if let Some(rest) = trimmed.strip_prefix("~/")
            && let Some(home) = std::env::var_os("HOME")
        {
            return Some(PathBuf::from(home).join(rest));
        }

        let candidate = PathBuf::from(trimmed);
        if candidate.is_absolute() {
            return Some(candidate);
        }

        if let Some(root) = self.app_state.workspace_root_path() {
            return Some(root.join(candidate));
        }

        if let Some(parent) = self.app_state.active_file().and_then(|path| path.parent()) {
            return Some(parent.join(candidate));
        }

        Some(candidate)
    }

    fn ai_chat_file_refs_from_prompt(&self, prompt: &str) -> Vec<PathBuf> {
        let mut refs = Vec::new();
        for token in prompt.split_whitespace() {
            if !token.trim_start().starts_with('@') {
                continue;
            }
            let Some(raw_path) = clean_ai_file_ref_token(token) else {
                continue;
            };
            let Some(path) = self.resolve_ai_chat_file_ref(&raw_path) else {
                continue;
            };
            if !refs.contains(&path) {
                refs.push(path);
            }
        }
        refs
    }

    pub(in crate::app::event_loop) fn ai_chat_file_reference_suggestions(
        &self,
        input_buffer: &str,
    ) -> Vec<(String, String)> {
        let Some((_, query)) = current_ai_at_token(input_buffer) else {
            return Vec::new();
        };
        let query = query.to_ascii_lowercase();
        let Some(root) = self.app_state.workspace_root_path() else {
            return self
                .app_state
                .active_file()
                .and_then(|path| {
                    let label = path.file_name()?.to_str()?.to_string();
                    (query.is_empty() || label.to_ascii_lowercase().contains(&query))
                        .then_some((label, "active file".to_string()))
                })
                .into_iter()
                .collect();
        };
        let Some(nodes) = self.app_state.workspace_nodes() else {
            return Vec::new();
        };

        let mut matches: Vec<(String, String)> = nodes
            .iter()
            .filter(|node| node.file_type == WorkspaceNodeType::File)
            .filter_map(|node| {
                let rel = node.path.strip_prefix(root).unwrap_or(&node.path);
                let label = rel.to_string_lossy().replace('\\', "/");
                let label_lower = label.to_ascii_lowercase();
                if !query.is_empty() && !label_lower.contains(&query) {
                    return None;
                }
                let detail = node
                    .path
                    .parent()
                    .and_then(|parent| parent.strip_prefix(root).ok())
                    .map(|parent| {
                        let text = parent.to_string_lossy().replace('\\', "/");
                        if text.is_empty() {
                            "workspace root".to_string()
                        } else {
                            text
                        }
                    })
                    .unwrap_or_else(|| "workspace file".to_string());
                Some((label, detail))
            })
            .collect();

        matches.sort_by(|(a, _), (b, _)| {
            let a_lower = a.to_ascii_lowercase();
            let b_lower = b.to_ascii_lowercase();
            let a_starts = a_lower.starts_with(&query);
            let b_starts = b_lower.starts_with(&query);
            b_starts
                .cmp(&a_starts)
                .then_with(|| a.len().cmp(&b.len()))
                .then_with(|| a.cmp(b))
        });
        matches.truncate(5);
        matches
    }

    /// Select the Nth file-reference suggestion and return the completed input text.
    fn ai_chat_file_ref_completion_at(&self, input_buffer: &str, index: usize) -> Option<String> {
        let (start, _) = current_ai_at_token(input_buffer)?;
        let (path, _) = self
            .ai_chat_file_reference_suggestions(input_buffer)
            .into_iter()
            .nth(index)?;
        Some(format!("{}@{} ", &input_buffer[..start], path))
    }

    fn submit_ai_chat_prompt(
        &mut self,
        prompt: String,
        display_text: String,
        file_refs: Vec<PathBuf>,
    ) -> bool {
        let (history, model, agent, prompt) = {
            let chat = &mut self.panel_state.ai_chat;
            let contexts: Vec<AiChatCodeContext> = chat.attached_code_contexts.drain(..).collect();
            let prompt = build_prompt_with_code_contexts(prompt, &contexts);
            chat.messages.push(AiChatMessage {
                role: AiRole::User,
                text: display_text,
            });
            chat.input_buffer.clear();
            chat.is_generating = true;
            chat.scroll_y = f32::MAX;

            let history: Vec<(String, String)> = chat
                .messages
                .iter()
                .map(|msg| {
                    let role = match msg.role {
                        AiRole::User => "user",
                        AiRole::Assistant => "assistant",
                        AiRole::System => "system",
                    };
                    (role.to_string(), msg.text.clone())
                })
                .collect();
            let model = chat.model.clone();
            let agent = Some(chat.agent.opencode_agent().to_string());
            (history, model, agent, prompt)
        };

        let buffer_context = self.app_state.text_string();
        let cursor_position = self.app_state.cursor_line_col();
        let active_buffer_path = self.app_state.active_file().map(|p| p.to_path_buf());
        let workspace_root = self.app_state.workspace_root_path().map(PathBuf::from);

        self.submit(RequestSpec {
            revision_id: 0,
            topic: RequestTopic::AiChat,
            payload: WorkerRequestPayload::AiChatRequest {
                prompt,
                buffer_context,
                cursor_position,
                history,
                active_buffer_path,
                workspace_root,
                file_refs,
                model,
                agent,
            },
        });

        true
    }

    fn add_visual_selection_to_ai_chat(&mut self) -> bool {
        let Some(selection) = self.app_state.visual_selection_range() else {
            return false;
        };
        let Some(selection_text) = self.app_state.visual_selection_text() else {
            return false;
        };
        if selection_text.trim().is_empty() {
            return false;
        }

        let active_path = self.app_state.active_file().map(PathBuf::from);
        let title = active_path
            .as_ref()
            .map(|path| {
                let display_path = self
                    .app_state
                    .workspace_root_path()
                    .and_then(|root| path.strip_prefix(root).ok())
                    .unwrap_or(path.as_path())
                    .display();
                format!(
                    "{}:{}-{}",
                    display_path,
                    selection.start_line + 1,
                    selection.end_line + 1
                )
            })
            .unwrap_or_else(|| {
                format!(
                    "selection:{}-{}",
                    selection.start_line + 1,
                    selection.end_line + 1
                )
            });
        let language_id = active_path.as_ref().map(|path| language_id_for_path(path));
        let lang_tag = language_id
            .as_ref()
            .map(|id| id.as_str())
            .unwrap_or("")
            .to_string();
        let context = AiChatCodeContext {
            title: title.clone(),
            language_id,
            text: selection_text,
        };

        let chat = &mut self.panel_state.ai_chat;
        chat.attached_code_contexts.push(context);
        if chat.input_buffer.trim().is_empty() {
            chat.input_buffer = "Hỏi về đoạn code đã chọn: ".to_string();
        }
        chat.messages.push(AiChatMessage {
            role: AiRole::System,
            text: format!(
                "Attached: {title}\n```{lang_tag}\n{}\n```",
                chat.attached_code_contexts.last().unwrap().text,
            ),
        });

        if !self.panel_state.right.visible {
            self.panel_state.right.visible = true;
            self.sidebar_needs_layout = true;
        }
        self.panel_state.right.switch_to_tab(PanelTabId::AiChat);
        let focus_changed = self.focus_manager.set(FocusTarget::RightSidebar);
        if focus_changed {
            self.input_handler.clear_pending_prefix();
        }
        if self.app_state.current_mode() == EditorMode::Visual {
            if let Ok(result) = self.app_state.apply_mode_event(ModeEvent::Escape) {
                self.sidebar_needs_layout |= result.changed;
            }
            let _ = self.app_state.clear_visual_selection();
        }
        true
    }

    fn stop_ai_chat_generation(&mut self) -> bool {
        if !self.panel_state.ai_chat.is_generating {
            return false;
        }

        self.submit(RequestSpec {
            revision_id: 0,
            topic: RequestTopic::AiChat,
            payload: WorkerRequestPayload::AiChatCancel,
        });
        true
    }

    /// Handle `AiChatToggle`, `AiChatSend`, `AiChatStop`, `AiChatClose` commands.
    ///
    /// Returns `Some(changed)` when the command was consumed, `None` otherwise.
    pub(super) fn handle_ai_chat_command(&mut self, command: &Command) -> Option<bool> {
        match command {
            Command::AiChatToggle => {
                self.panel_state.toggle_right();
                let is_now_visible = self.panel_state.right.visible;

                // AI Chat tab = a terminal running a chosen CLI agent.
                self.panel_state.right.switch_to_tab(PanelTabId::AiChat);

                if is_now_visible {
                    let focus_changed = self.focus_manager.set(FocusTarget::RightSidebar);
                    if focus_changed {
                        self.input_handler.clear_pending_prefix();
                    }
                    if self.right_pty_session_id.is_some() || self.pending_right_pty_spawn {
                        // Agent already running — drop into it.
                        if let Ok(result) =
                            self.app_state.apply_mode_event(ModeEvent::FocusTerminal)
                        {
                            let _ = result;
                        }
                    } else {
                        // No agent yet — let the user pick which CLI to launch.
                        self.open_ai_agent_chooser();
                    }
                } else {
                    // Closing: exit any lingering terminal focus mode first.
                    if matches!(
                        self.app_state.current_mode(),
                        EditorMode::TerminalFocus | EditorMode::TerminalNormal
                    ) {
                        let _ = self.app_state.apply_mode_event(ModeEvent::ExitFocus);
                    }
                    if self.focus_manager.set(FocusTarget::CenterEditor) {
                        self.input_handler.clear_pending_prefix();
                    }
                }

                self.sidebar_needs_layout = true;
                Some(true)
            }
            Command::AiChatPromptInstall => Some(self.begin_ai_chat_install_confirmation()),
            Command::AiChatAddSelectionContext => Some(self.add_visual_selection_to_ai_chat()),
            Command::AiChatStop => Some(self.stop_ai_chat_generation()),
            Command::AiChatSend => {
                let raw = self.panel_state.ai_chat.input_buffer.trim().to_string();
                if raw.is_empty() || self.panel_state.ai_chat.is_generating {
                    return Some(false);
                }

                // ── Slash commands (handled client-side, no worker) ────────
                if let Some(slash) = raw.strip_prefix('/') {
                    let mut parts = slash.splitn(2, char::is_whitespace);
                    let cmd_name = parts.next().unwrap_or("").to_ascii_lowercase();
                    let arg = parts.next().map(str::trim);

                    let chat = &mut self.panel_state.ai_chat;
                    chat.input_buffer.clear();

                    match cmd_name.as_str() {
                        "clear" | "new" => {
                            chat.messages.clear();
                            chat.attached_code_contexts.clear();
                            chat.is_generating = false;
                        }
                        "review" => {
                            let focus = arg.filter(|s| !s.is_empty());
                            let prompt = match focus {
                                Some(f) => format!("Please review this code with a focus on: {f}. Look for bugs, logic errors, style issues, and suggest improvements."),
                                None => "Please review this code thoroughly. Look for bugs, logic errors, performance issues, and suggest concrete improvements.".to_string(),
                            };
                            let display = format!(
                                "/review{}",
                                arg.map(|a| format!(" {a}")).unwrap_or_default()
                            );
                            return Some(self.submit_ai_chat_prompt(prompt, display, Vec::new()));
                        }
                        "explain" => {
                            let topic = arg.filter(|s| !s.is_empty());
                            let prompt = match topic {
                                Some(t) => format!("Please explain this clearly: {t}"),
                                None => "Please explain this code clearly — what it does, how it works, and any non-obvious design decisions.".to_string(),
                            };
                            let display = format!(
                                "/explain{}",
                                arg.map(|a| format!(" {a}")).unwrap_or_default()
                            );
                            return Some(self.submit_ai_chat_prompt(prompt, display, Vec::new()));
                        }
                        "fix" => {
                            let issue = arg.filter(|s| !s.is_empty());
                            let prompt = match issue {
                                Some(i) => {
                                    format!("Please fix the following issue in this code: {i}")
                                }
                                None => "Please identify and fix any bugs or issues in this code."
                                    .to_string(),
                            };
                            let display =
                                format!("/fix{}", arg.map(|a| format!(" {a}")).unwrap_or_default());
                            return Some(self.submit_ai_chat_prompt(prompt, display, Vec::new()));
                        }
                        "test" => {
                            let desc = arg.filter(|s| !s.is_empty());
                            let prompt = match desc {
                                Some(d) => format!("Generate unit tests for this code. Focus on: {d}"),
                                None => "Generate comprehensive unit tests for this code. Cover happy paths, edge cases, and error conditions.".to_string(),
                            };
                            let display = format!(
                                "/test{}",
                                arg.map(|a| format!(" {a}")).unwrap_or_default()
                            );
                            return Some(self.submit_ai_chat_prompt(prompt, display, Vec::new()));
                        }
                        "commit" => {
                            return Some(self.submit_ai_chat_prompt(
                                "Write a concise, well-structured git commit message for the current staged changes. Follow the conventional commits format if appropriate. Output only the commit message.".to_string(),
                                "/commit".to_string(),
                                Vec::new(),
                            ));
                        }
                        "diff" => {
                            return Some(self.submit_ai_chat_prompt(
                                "Summarize the current git diff or pending changes. Describe what was changed, why it matters, and flag anything that looks risky.".to_string(),
                                "/diff".to_string(),
                                Vec::new(),
                            ));
                        }
                        "context" => {
                            let ctx_count = chat.attached_code_contexts.len();
                            let detail = if ctx_count == 0 {
                                "No file contexts attached. Use @path in your message to attach a file.".to_string()
                            } else {
                                let names: Vec<_> = chat
                                    .attached_code_contexts
                                    .iter()
                                    .map(|c| c.title.as_str())
                                    .collect();
                                format!("{ctx_count} context(s) attached:\n{}", names.join("\n  "))
                            };
                            chat.messages.push(AiChatMessage {
                                role: AiRole::System,
                                text: detail,
                            });
                        }
                        "plan" => {
                            chat.agent = AiAgentMode::Plan;
                            chat.messages.push(AiChatMessage {
                                role: AiRole::System,
                                text: "Plan mode enabled. Netherize will ask opencode to use the plan agent.".to_string(),
                            });
                        }
                        "build" => {
                            chat.agent = AiAgentMode::Build;
                            chat.messages.push(AiChatMessage {
                                role: AiRole::System,
                                text: "Build mode enabled. Netherize will ask opencode to use the build agent.".to_string(),
                            });
                        }
                        "mode" | "agent" => {
                            if let Some(mode) = arg
                                .filter(|s| !s.is_empty())
                                .and_then(AiAgentMode::from_input)
                            {
                                chat.agent = mode;
                                chat.messages.push(AiChatMessage {
                                    role: AiRole::System,
                                    text: format!("Mode set to: {}", mode.label()),
                                });
                            } else {
                                chat.messages.push(AiChatMessage {
                                    role: AiRole::System,
                                    text: ai_agent_help(chat.agent),
                                });
                            }
                        }
                        "model" => {
                            if let Some(m) = arg.filter(|s| !s.is_empty()) {
                                let normalized = m.to_ascii_lowercase();
                                let msg = if matches!(
                                    normalized.as_str(),
                                    "default" | "reset" | "clear" | "none"
                                ) {
                                    let previous = chat.model.take();
                                    match previous {
                                        Some(old) => {
                                            format!("Model override cleared. Previous: {old}")
                                        }
                                        None => {
                                            "Using default model from opencode config.".to_string()
                                        }
                                    }
                                } else {
                                    let prev = chat.model.replace(m.to_string());
                                    if let Some(old) = prev {
                                        format!("Model changed: {old} -> {m}")
                                    } else {
                                        format!("Model set to: {m}")
                                    }
                                };
                                chat.messages.push(AiChatMessage {
                                    role: AiRole::System,
                                    text: msg,
                                });
                            } else {
                                let current = chat
                                    .model
                                    .clone()
                                    .unwrap_or_else(|| "default (from opencode config)".into());
                                chat.messages.push(AiChatMessage {
                                    role: AiRole::System,
                                    text: format!("Current model: {current}"),
                                });
                            }
                        }
                        "models" => {
                            chat.messages.push(AiChatMessage {
                                role: AiRole::System,
                                text: ai_models_help(chat.model.as_deref()),
                            });
                        }
                        "status" => {
                            let model =
                                chat.model.as_deref().unwrap_or("default (opencode config)");
                            chat.messages.push(AiChatMessage {
                                role: AiRole::System,
                                text: format!(
                                    "AI chat status:\n- agent: {}\n- model: {model}\n- messages: {}\n- attached contexts: {}\n- generating: {}",
                                    chat.agent.label(),
                                    chat.messages.len(),
                                    chat.attached_code_contexts.len(),
                                    chat.is_generating,
                                ),
                            });
                        }
                        "compact" => {
                            chat.messages.push(AiChatMessage {
                                role: AiRole::System,
                                text: "Compact is not wired to opencode yet. Tip: use /new to start a fresh context or ask Netherize to summarize this chat.".to_string(),
                            });
                        }
                        "tokens" => {
                            chat.messages.push(AiChatMessage {
                                role: AiRole::System,
                                text: "Token usage is reported by opencode in stream status lines when available. Netherize currently filters noisy status lines from history.".to_string(),
                            });
                        }
                        "" | "help" => {
                            chat.messages.push(AiChatMessage {
                                role: AiRole::System,
                                text: [
                                    "Available commands:",
                                    "  /review [focus]   review code for bugs / improvements",
                                    "  /explain [topic]  explain the current code or topic",
                                    "  /fix [issue]      find and fix bugs",
                                    "  /test [focus]     generate unit tests",
                                    "  /commit           write a git commit message",
                                    "  /diff             summarise current git diff",
                                    "  /context          show attached file contexts",
                                    "  /clear            clear chat history",
                                    "  /new              same as /clear",
                                    "  /plan             switch to opencode plan agent",
                                    "  /build            switch to opencode build agent",
                                    "  /mode             show current agent mode",
                                    "  /mode <name>      set mode: build or plan",
                                    "  /agent <name>     alias for /mode <name>",
                                    "  /model <id>       set model (e.g. anthropic/claude-opus-4-5)",
                                    "  /model default    clear model override",
                                    "  /model            show current model",
                                    "  /models           show common model ids",
                                    "  /status           show current chat settings",
                                    "  /compact          summarize/compact context hint",
                                    "  /tokens           show token usage hint",
                                    "  /help             show this help",
                                ]
                                .join("\n"),
                            });
                        }
                        _ => {
                            chat.messages.push(AiChatMessage {
                                role: AiRole::System,
                                text: format!("Unknown command: /{cmd_name}  (try /help)"),
                            });
                        }
                    }
                    // Always jump to the latest message after a client-side command.
                    self.panel_state.ai_chat.scroll_y = f32::MAX;
                    return Some(true);
                }

                // ── Normal message → send to opencode worker ───────────────
                let file_refs = self.ai_chat_file_refs_from_prompt(&raw);
                Some(self.submit_ai_chat_prompt(raw.clone(), raw, file_refs))
            }
            Command::AiChatClose => {
                let mut changed = false;
                if self.panel_state.right.visible {
                    self.panel_state.right.visible = false;
                    self.sidebar_needs_layout = true;
                    changed = true;
                }
                if self.focus_manager.current() == FocusTarget::RightSidebar {
                    let focus_changed = self.focus_manager.set(FocusTarget::CenterEditor);
                    if focus_changed {
                        self.input_handler.clear_pending_prefix();
                    }
                    changed |= focus_changed;
                }
                Some(changed)
            }
            Command::AiChatUnfocus => {
                // In Zen Mode the editor is hidden, so unfocusing back to it would
                // strand the user on a blank surface. Keep focus on the maximized
                // chat panel instead — Esc should not leave the zen window.
                if self.panel_state.maximized_region.is_some() {
                    Some(false)
                } else if self.focus_manager.current() == FocusTarget::RightSidebar {
                    let focus_changed = self.focus_manager.set(FocusTarget::CenterEditor);
                    if focus_changed {
                        self.input_handler.clear_pending_prefix();
                    }
                    Some(focus_changed)
                } else {
                    Some(false)
                }
            }
            Command::AiChatFocus => {
                if !self.panel_state.right.visible {
                    self.panel_state.right.visible = true;
                    self.sidebar_needs_layout = true;
                }
                self.panel_state.right.switch_to_tab(PanelTabId::AiChat);
                let focus_changed = self.focus_manager.set(FocusTarget::RightSidebar);
                if focus_changed {
                    self.input_handler.clear_pending_prefix();
                }
                if self.right_pty_session_id.is_some() || self.pending_right_pty_spawn {
                    if let Ok(result) = self.app_state.apply_mode_event(ModeEvent::FocusTerminal) {
                        let _ = result;
                    }
                } else {
                    self.open_ai_agent_chooser();
                }
                Some(true)
            }
            Command::AiChatInputChar(ch) => {
                self.panel_state.ai_chat.input_buffer.push(*ch);
                self.panel_state.ai_chat.selected_suggestion_index = 0;
                Some(true)
            }
            Command::AiChatBackspace => {
                let chat = &mut self.panel_state.ai_chat;
                if chat.input_buffer.pop().is_some() {
                    chat.selected_suggestion_index = 0;
                    Some(true)
                } else {
                    Some(false)
                }
            }
            Command::AiChatClearInput => {
                if self.panel_state.ai_chat.is_generating {
                    return Some(self.stop_ai_chat_generation());
                }
                let chat = &mut self.panel_state.ai_chat;
                if chat.input_buffer.is_empty() {
                    Some(false)
                } else {
                    chat.input_buffer.clear();
                    chat.selected_suggestion_index = 0;
                    Some(true)
                }
            }
            Command::AiChatAcceptSuggestion => {
                let idx = self.panel_state.ai_chat.selected_suggestion_index;
                let completed = self
                    .ai_chat_file_ref_completion_at(&self.panel_state.ai_chat.input_buffer, idx)
                    .or_else(|| {
                        ai_slash_command_completion_at(&self.panel_state.ai_chat.input_buffer, idx)
                    });
                let Some(completed) = completed else {
                    return Some(false);
                };
                let chat = &mut self.panel_state.ai_chat;
                if chat.input_buffer == completed {
                    Some(false)
                } else {
                    chat.input_buffer = completed;
                    chat.selected_suggestion_index = 0;
                    Some(true)
                }
            }
            Command::AiChatSuggestionNext => {
                let count = self
                    .ai_chat_file_reference_suggestions(&self.panel_state.ai_chat.input_buffer)
                    .len();
                // Fallback to slash-command count when there are no @-file suggestions.
                let count = if count > 0 {
                    count
                } else {
                    slash_command_suggestion_count(&self.panel_state.ai_chat.input_buffer)
                };
                let chat = &mut self.panel_state.ai_chat;
                if count > 0 {
                    chat.selected_suggestion_index = (chat.selected_suggestion_index + 1) % count;
                }
                Some(true)
            }
            Command::AiChatSuggestionPrev => {
                let count = self
                    .ai_chat_file_reference_suggestions(&self.panel_state.ai_chat.input_buffer)
                    .len();
                let count = if count > 0 {
                    count
                } else {
                    slash_command_suggestion_count(&self.panel_state.ai_chat.input_buffer)
                };
                let chat = &mut self.panel_state.ai_chat;
                if count > 0 {
                    chat.selected_suggestion_index = chat
                        .selected_suggestion_index
                        .checked_sub(1)
                        .unwrap_or(count - 1);
                }
                Some(true)
            }
            Command::AiChatInputText(text) => {
                self.panel_state.ai_chat.input_buffer.push_str(text);
                self.panel_state.ai_chat.selected_suggestion_index = 0;
                Some(true)
            }
            Command::AiChatPasteClipboard => {
                let Ok(text) = self.clipboard.get_text() else {
                    return Some(false);
                };
                if text.is_empty() {
                    return Some(false);
                }
                self.panel_state.ai_chat.input_buffer.push_str(&text);
                self.panel_state.ai_chat.selected_suggestion_index = 0;
                Some(true)
            }
            Command::AiChatScrollHalfPageUp => {
                let chat = &mut self.panel_state.ai_chat;
                let current = chat.scroll_y.min(chat.max_scroll_y);
                chat.scroll_y = (current - 200.0).max(0.0);
                Some(true)
            }
            Command::AiChatScrollHalfPageDown => {
                let chat = &mut self.panel_state.ai_chat;
                let current = chat.scroll_y.min(chat.max_scroll_y);
                chat.scroll_y = (current + 200.0).min(chat.max_scroll_y);
                Some(true)
            }
            _ => None,
        }
    }

    /// Show the in-panel AI-agent picker: focus the AI Chat tab so its inline
    /// list (navigated with j/k, launched with Enter) renders and receives input.
    pub(in crate::app::event_loop) fn open_ai_agent_chooser(&mut self) {
        if !self.panel_state.right.visible {
            self.panel_state.right.visible = true;
            self.sidebar_needs_layout = true;
        }
        self.panel_state.right.switch_to_tab(PanelTabId::AiChat);
        let count = self.ai_agent_picker_agents().len();
        if self.ai_agent_picker_selected >= count {
            self.ai_agent_picker_selected = 0;
        }
        if self.focus_manager.set(FocusTarget::RightSidebar) {
            self.input_handler.clear_pending_prefix();
        }
    }

    /// Agents for the picker, most-recently-used first.
    pub(in crate::app::event_loop) fn ai_agent_picker_agents(
        &self,
    ) -> Vec<&'static crate::app::ai_agents::AiAgent> {
        let recent = &self.persistent_state.recent_ai_agents;
        let mut agents: Vec<&crate::app::ai_agents::AiAgent> =
            crate::app::ai_agents::default_ai_agents().iter().collect();
        agents.sort_by_key(|a| {
            recent
                .iter()
                .position(|id| id == a.id)
                .unwrap_or(usize::MAX)
        });
        agents
    }

    pub(in crate::app::event_loop) fn ai_agent_picker_move(&mut self, forward: bool) -> bool {
        let count = self.ai_agent_picker_agents().len();
        if count == 0 {
            return false;
        }
        let cur = self.ai_agent_picker_selected.min(count - 1);
        self.ai_agent_picker_selected = if forward {
            (cur + 1) % count
        } else {
            (cur + count - 1) % count
        };
        true
    }

    /// Launch the selected agent in the right-dock terminal.
    pub(in crate::app::event_loop) fn ai_agent_picker_launch(&mut self) -> bool {
        let agents = self.ai_agent_picker_agents();
        let Some(agent) = agents.get(self.ai_agent_picker_selected).copied() else {
            return false;
        };

        self.persistent_state.push_recent_ai_agent(agent.id);
        self.persistent_state.save();

        if !self.panel_state.right.visible {
            self.panel_state.right.visible = true;
            self.sidebar_needs_layout = true;
        }
        self.panel_state.right.switch_to_tab(PanelTabId::AiChat);
        self.spawn_right_agent_terminal(agent.command, agent.label);
        self.focus_manager.set(FocusTarget::RightSidebar);
        self.input_handler.clear_pending_prefix();
        if let Ok(result) = self.app_state.apply_mode_event(ModeEvent::FocusTerminal) {
            let _ = result;
        }
        self.right_terminal_needs_layout = true;
        self.show_transient_toast(format!("AI Chat\nLaunching {} …", agent.label));
        true
    }

    /// Spawn `command` in the right-dock PTY via a login shell (so PATH resolves
    /// the agent and any "command not found" shows in the terminal). Replaces any
    /// running agent.
    pub(super) fn spawn_right_agent_terminal(&mut self, command: &str, label: &str) {
        let working_dir = self.default_terminal_working_dir();
        // Reset the visible grid so the previous agent's output doesn't linger;
        // the next layout sync resizes it to the panel.
        self.right_terminal_grid = TerminalGrid::new(120, 40);
        self.right_pty_session_id = None;
        self.pending_right_pty_spawn = true;
        self.right_agent_label = Some(label.to_string());
        // Run the agent in the shell once the PTY is ready. `exec` replaces the
        // login shell with the agent process, so the shell resolves PATH (login
        // env) AND, when the agent exits or is killed (Ctrl-C), the PTY closes
        // cleanly instead of dropping back to a live shell prompt. The PTY close
        // resets `right_pty_session_id` to None, which re-shows the agent picker
        // and clears the grid (see handle_terminal_result / PtySessionClosed).
        self.right_pty_startup_command = Some(format!("exec {command}\r"));
        self.submit(RequestSpec {
            revision_id: 0,
            topic: RequestTopic::TerminalPty,
            payload: WorkerRequestPayload::SpawnPtyShell {
                shell: None,
                working_dir,
            },
        });
    }

    /// Ensure the right-dock terminal is running `opencode`.
    ///
    /// * If the right PTY is already alive, does nothing (opencode is already running).
    /// * If no right PTY exists yet, spawns `opencode` **directly** as a PTY process
    ///   (not via a shell) so that when opencode exits the PTY closes cleanly with no
    ///   leftover shell prompt showing in the right dock.
    pub(super) fn ensure_right_opencode_terminal(&mut self) {
        if self.right_pty_session_id.is_some() || self.pending_right_pty_spawn {
            // Already running — nothing to do.
            return;
        }

        // Resolve the opencode binary (same logic as opencode_available but returns path).
        let binary: Option<std::path::PathBuf> = (|| {
            if let Some(path_var) = std::env::var_os("PATH") {
                for dir in std::env::split_paths(&path_var) {
                    let candidate = dir.join("opencode");
                    if candidate.is_file() {
                        return Some(candidate);
                    }
                }
            }
            if let Some(home) = std::env::var_os("HOME") {
                let candidate = std::path::PathBuf::from(home)
                    .join(".opencode")
                    .join("bin")
                    .join("opencode");
                if candidate.is_file() {
                    return Some(candidate);
                }
            }
            None
        })();

        let working_dir = self.default_terminal_working_dir();
        self.pending_right_pty_spawn = true;
        // No startup command needed — opencode IS the process.
        self.right_pty_startup_command = None;

        if let Some(bin) = binary {
            // Spawn opencode directly as the PTY process.
            self.submit(RequestSpec {
                revision_id: 0,
                topic: RequestTopic::TerminalPty,
                payload: WorkerRequestPayload::SpawnPtyCommand {
                    program: bin.to_string_lossy().into_owned(),
                    args: Vec::new(),
                    working_dir,
                },
            });
        } else {
            // opencode not found — fall back to spawning a shell that tells the user.
            self.right_pty_startup_command = Some(
                "echo 'opencode not found. Run: curl -fsSL https://opencode.ai/install | sh'\\r"
                    .to_string(),
            );
            self.submit(RequestSpec {
                revision_id: 0,
                topic: RequestTopic::TerminalPty,
                payload: WorkerRequestPayload::SpawnPtyShell {
                    shell: None,
                    working_dir,
                },
            });
        }
    }
}
