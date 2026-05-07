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
    "/clear", "/new", "/plan", "/build", "/model", "/models", "/mode", "/help",
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
    let suffix = if matches!(command, "/model" | "/mode") {
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

    /// Handle `AiChatToggle`, `AiChatSend`, `AiChatClose` commands.
    ///
    /// Returns `Some(changed)` when the command was consumed, `None` otherwise.
    pub(super) fn handle_ai_chat_command(&mut self, command: &Command) -> Option<bool> {
        match command {
            Command::AiChatToggle => {
                let is_now_visible = self.panel_state.toggle_right();

                // Switch the right sidebar to the AI Chat tab.
                self.panel_state.right.switch_to_tab(PanelTabId::AiChat);

                let focus_changed = if is_now_visible {
                    self.focus_manager.set(FocusTarget::RightSidebar)
                } else if self.focus_manager.current() == FocusTarget::RightSidebar {
                    self.focus_manager.set(FocusTarget::CenterEditor)
                } else {
                    false
                };

                if focus_changed {
                    self.input_handler.clear_pending_prefix();
                }

                self.sidebar_needs_layout = true;
                Some(true)
            }
            Command::AiChatPromptInstall => Some(self.begin_ai_chat_install_confirmation()),
            Command::AiChatAddSelectionContext => Some(self.add_visual_selection_to_ai_chat()),
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
                        "" | "help" => {
                            chat.messages.push(AiChatMessage {
                                role: AiRole::System,
                                text: [
                                    "Available commands:",
                                    "  /clear       clear chat history",
                                    "  /new         same as /clear",
                                    "  /plan        switch to opencode plan agent",
                                    "  /build       switch to opencode build agent",
                                    "  /mode        show current agent mode",
                                    "  /mode <name> set mode: build or plan",
                                    "  /model <id>  set model (e.g. anthropic/claude-opus-4-5)",
                                    "  /model default clear model override",
                                    "  /model       show current model",
                                    "  /models      show common model ids",
                                    "  /help        show this help",
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
                if self.focus_manager.current() == FocusTarget::RightSidebar {
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
            Command::AiChatAcceptSuggestion => {
                let idx = self.panel_state.ai_chat.selected_suggestion_index;
                let completed = self
                    .ai_chat_file_ref_completion_at(&self.panel_state.ai_chat.input_buffer, idx)
                    .or_else(|| {
                        ai_slash_command_completion_at(
                            &self.panel_state.ai_chat.input_buffer,
                            idx,
                        )
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
                    chat.selected_suggestion_index =
                        (chat.selected_suggestion_index + 1) % count;
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
}
