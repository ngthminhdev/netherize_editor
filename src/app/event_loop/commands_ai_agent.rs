use super::*;
use crate::workbench::panel_state::PanelTabId;

impl AppShell {
    /// Handle the right-dock AI agent tab. The tab is terminal-backed; when no
    /// agent is running it shows the inline picker, otherwise input goes to PTY.
    pub(super) fn handle_ai_agent_command(&mut self, command: &Command) -> Option<bool> {
        match command {
            Command::AiChatToggle => {
                self.panel_state.toggle_right();
                let is_now_visible = self.panel_state.right.visible;
                self.panel_state.right.switch_to_tab(PanelTabId::AiChat);

                if is_now_visible {
                    let focus_changed = self.focus_manager.set(FocusTarget::RightSidebar);
                    if focus_changed {
                        self.input_handler.clear_pending_prefix();
                    }
                    if self.right_pty_session_id.is_some() || self.pending_right_pty_spawn {
                        if let Ok(result) = self.app_state.apply_mode_event(ModeEvent::FocusTerminal)
                        {
                            let _ = result;
                        }
                    } else {
                        self.open_ai_agent_chooser();
                    }
                } else {
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
            _ => None,
        }
    }

    /// Show the in-panel AI-agent picker: focus the AI Chat tab so its inline
    /// list renders and receives j/k/Enter through right-sidebar routing.
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

    pub(in crate::app::event_loop) fn ai_agent_picker_agents(
        &self,
    ) -> Vec<&'static crate::app::ai_agents::AiAgent> {
        let recent = &self.persistent_state.recent_ai_agents;
        let mut agents: Vec<&crate::app::ai_agents::AiAgent> =
            crate::app::ai_agents::default_ai_agents().iter().collect();
        agents.sort_by_key(|agent| {
            recent
                .iter()
                .position(|id| id == agent.id)
                .unwrap_or(usize::MAX)
        });
        agents
    }

    pub(in crate::app::event_loop) fn ai_agent_picker_move(&mut self, forward: bool) -> bool {
        let count = self.ai_agent_picker_agents().len();
        if count == 0 {
            return false;
        }
        let current = self.ai_agent_picker_selected.min(count - 1);
        self.ai_agent_picker_selected = if forward {
            (current + 1) % count
        } else {
            (current + count - 1) % count
        };
        true
    }

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
        self.show_transient_toast(format!("AI Agent\nLaunching {} ...", agent.label));
        true
    }

    /// Spawn `command` through the right-dock login shell so agent binaries are
    /// resolved from the user's normal PATH and failures show inside the terminal.
    pub(super) fn spawn_right_agent_terminal(&mut self, command: &str, label: &str) {
        let working_dir = self.default_terminal_working_dir();
        self.right_terminal_grid = TerminalGrid::new(120, 40);
        self.right_pty_session_id = None;
        self.pending_right_pty_spawn = true;
        self.right_agent_label = Some(label.to_string());
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
}
