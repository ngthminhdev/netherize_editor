//! AI agent CLIs that can be launched inside the right-dock terminal.
//!
//! The right dock's "AI Chat" tab is a real PTY: picking an agent spawns a
//! login shell and runs the agent's command in it (so PATH resolution and any
//! "command not found" errors surface in the terminal, exactly like running it
//! by hand). The list is intentionally small and easy to extend.

/// A launchable AI CLI agent.
#[derive(Debug, Clone, Copy)]
pub struct AiAgent {
    /// Stable identifier used for the MRU cache and the palette action.
    pub id: &'static str,
    /// Display label in the agent picker.
    pub label: &'static str,
    /// Shell command run in the right-dock PTY (resolved via the login shell's
    /// PATH). May include arguments, e.g. `claude --resume`.
    pub command: &'static str,
}

/// All launchable agents, in default display order. The MRU cache reorders this.
pub fn default_ai_agents() -> &'static [AiAgent] {
    &[
        AiAgent {
            id: "opencode",
            label: "opencode",
            command: "opencode",
        },
        AiAgent {
            id: "claude",
            label: "Claude Code",
            command: "claude",
        },
        AiAgent {
            id: "codex",
            label: "Codex",
            command: "codex",
        },
        AiAgent {
            id: "antigravity",
            label: "Antigravity",
            command: "agy",
        },
        AiAgent {
            id: "gemini",
            label: "Gemini",
            command: "gemini",
        },
        AiAgent {
            id: "mimo",
            label: "MiMo",
            command: "mimo",
        },
        AiAgent {
            id: "claudemimo",
            label: "Claude Mimo",
            command: "mimocode",
        },
        AiAgent {
            id: "claudekimi",
            label: "Claude Kimi",
            command: "kimicode",
        },
        // Dojo mock interviewer: Claude Code with the prompt the Dojo ships to
        // ~/.config/netherize/dojo/interviewer.md (user-editable). `$(cat …)`
        // expands in the login shell that hosts the agent.
        AiAgent {
            id: "interviewer",
            label: "Interviewer (claude)",
            command: "claude --append-system-prompt \"$(cat ~/.config/netherize/dojo/interviewer.md)\"",
        },
    ]
}

/// Look up an agent by its stable id.
pub fn ai_agent(id: &str) -> Option<&'static AiAgent> {
    default_ai_agents().iter().find(|a| a.id == id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_are_unique_and_resolvable() {
        let agents = default_ai_agents();
        for agent in agents {
            assert!(ai_agent(agent.id).is_some());
            assert!(!agent.command.is_empty());
        }
        let mut ids: Vec<&str> = agents.iter().map(|a| a.id).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), agents.len(), "duplicate agent id");
    }

    #[test]
    fn unknown_agent_is_none() {
        assert!(ai_agent("nope").is_none());
    }

    #[test]
    fn interviewer_agent_reads_the_dojo_prompt_file() {
        let a = ai_agent("interviewer").expect("interviewer");
        assert!(a.command.starts_with("claude --append-system-prompt"));
        assert!(a.command.contains("dojo/interviewer.md"));
    }
}
