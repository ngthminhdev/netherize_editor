use super::*;
use crate::workbench::panel_state::{AiChatMessage, AiRole, PanelTabId};

impl AppShell {
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
            Command::AiChatSend => {
                let chat = &mut self.panel_state.ai_chat;
                if !chat.input_buffer.trim().is_empty() && !chat.is_generating {
                    let prompt = chat.input_buffer.clone();
                    chat.messages.push(AiChatMessage {
                        role: AiRole::User,
                        text: prompt.clone(),
                    });
                    chat.input_buffer.clear();
                    chat.is_generating = true;

                    // Gather buffer context and cursor position
                    let buffer_context = self.app_state.text_string();
                    let cursor_position = self.app_state.cursor_line_col();

                    // Build history from existing messages
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

                    self.submit(RequestSpec {
                        revision_id: 0,
                        topic: RequestTopic::AiChat,
                        payload: WorkerRequestPayload::AiChatRequest {
                            prompt,
                            buffer_context,
                            cursor_position,
                            history,
                        },
                    });

                    Some(true)
                } else {
                    Some(false)
                }
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
                Some(true)
            }
            Command::AiChatBackspace => {
                let chat = &mut self.panel_state.ai_chat;
                if chat.input_buffer.pop().is_some() {
                    Some(true)
                } else {
                    Some(false)
                }
            }
            Command::AiChatInputText(text) => {
                self.panel_state.ai_chat.input_buffer.push_str(text);
                Some(true)
            }
            _ => None,
        }
    }
}
