use serde::Deserialize;

/// Root structure of a `.toml` keymap profile file.
#[derive(Debug, Clone, Deserialize)]
pub struct KeymapFile {
    pub profile: ProfileMeta,
    #[serde(default)]
    pub bindings: Vec<KeyBinding>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ProfileMeta {
    pub name: String,
    pub description: Option<String>,
}

/// One key → command mapping entry from a TOML file.
#[derive(Debug, Clone, Deserialize)]
pub struct KeyBinding {
    /// Key spec string. Examples:
    /// - Named keys:  "Escape", "Enter", "Backspace", "Space"
    ///   "ArrowUp", "ArrowDown", "ArrowLeft", "ArrowRight"
    /// - Physical keys: "h", "j", "k", "l", "i", "v"
    ///   Special names: "backtick", "backslash"
    /// - Modifier combo: "mod+s" (Cmd on macOS, Ctrl elsewhere)
    ///   Aliases: "Ctrl+e", "Cmd+Shift+P"
    /// - Chord sequence: "d d", "<leader>f f", "<leader>f w"
    pub key: String,

    /// Mode scope. Absent or null = global (applies to all modes).
    /// Valid values: "insert", "normal", "visual", "palette", "terminal",
    /// "terminal_normal"
    pub mode: Option<String>,

    /// Stable command ID, e.g. "editor.move_up" or "app.toggle_terminal".
    /// See `core::command_ids` for the full list.
    pub command: String,
}
