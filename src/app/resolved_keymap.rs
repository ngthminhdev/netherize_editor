use std::collections::HashMap;

use winit::keyboard::{KeyCode, NamedKey};

use crate::{
    app::input::NormalizedInput,
    config::keymap_config::KeyBinding,
    core::{command_ids, commands::Command, mode::EditorMode},
};

// ── KeySpec ───────────────────────────────────────────────────────────────────

/// Typed representation of a key specification parsed from a TOML string.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum KeySpec {
    /// A logical named key: Escape, Enter, Backspace, ArrowUp, etc.
    Named(NamedKey),
    /// A physical key code (no modifier): h, j, k, l, backtick, etc.
    Physical(KeyCode),
    /// Command/Ctrl modifier + physical key: "mod+s"
    ModPlus(KeyCode),
    /// Leader key followed by a physical key: "<leader>f"
    LeaderThen(KeyCode),
}

impl KeySpec {
    /// Returns true if this spec matches the given input event.
    /// Note: `LeaderThen` is checked separately in `lookup_leader`.
    pub fn matches(&self, input: &NormalizedInput) -> bool {
        match self {
            Self::Named(named) => !input.has_command_modifier() && input.named_key == Some(*named),
            Self::Physical(code) => {
                !input.has_command_modifier() && input.physical_key == Some(*code)
            }
            Self::ModPlus(code) => {
                input.has_command_modifier() && input.physical_key == Some(*code)
            }
            Self::LeaderThen(_) => false, // matched via lookup_leader, not here
        }
    }
}

/// Parse a key string from a TOML binding entry into a `KeySpec`.
/// Returns `None` if the string is unrecognised.
pub fn parse_key_spec(s: &str) -> Option<KeySpec> {
    // <leader>x
    if let Some(rest) = s.strip_prefix("<leader>") {
        return char_key_to_code(rest).map(KeySpec::LeaderThen);
    }
    // mod+x
    if let Some(rest) = s.strip_prefix("mod+") {
        return char_key_to_code(rest).map(KeySpec::ModPlus);
    }
    // Named logical keys
    let named = match s {
        "Escape" => Some(NamedKey::Escape),
        "Backspace" => Some(NamedKey::Backspace),
        "Enter" => Some(NamedKey::Enter),
        "Space" => Some(NamedKey::Space),
        "ArrowUp" => Some(NamedKey::ArrowUp),
        "ArrowDown" => Some(NamedKey::ArrowDown),
        "ArrowLeft" => Some(NamedKey::ArrowLeft),
        "ArrowRight" => Some(NamedKey::ArrowRight),
        "Tab" => Some(NamedKey::Tab),
        _ => None,
    };
    if let Some(n) = named {
        return Some(KeySpec::Named(n));
    }
    // Special physical key names
    match s {
        "backtick" => return Some(KeySpec::Physical(KeyCode::Backquote)),
        "backslash" => return Some(KeySpec::Physical(KeyCode::Backslash)),
        _ => {}
    }
    // Single letter or digit
    char_key_to_code(s).map(KeySpec::Physical)
}

/// Map a single-char string (a-z, 0-9) or special name to a `KeyCode`.
fn char_key_to_code(s: &str) -> Option<KeyCode> {
    match s.to_ascii_lowercase().as_str() {
        "a" => Some(KeyCode::KeyA),
        "b" => Some(KeyCode::KeyB),
        "c" => Some(KeyCode::KeyC),
        "d" => Some(KeyCode::KeyD),
        "e" => Some(KeyCode::KeyE),
        "f" => Some(KeyCode::KeyF),
        "g" => Some(KeyCode::KeyG),
        "h" => Some(KeyCode::KeyH),
        "i" => Some(KeyCode::KeyI),
        "j" => Some(KeyCode::KeyJ),
        "k" => Some(KeyCode::KeyK),
        "l" => Some(KeyCode::KeyL),
        "m" => Some(KeyCode::KeyM),
        "n" => Some(KeyCode::KeyN),
        "o" => Some(KeyCode::KeyO),
        "p" => Some(KeyCode::KeyP),
        "q" => Some(KeyCode::KeyQ),
        "r" => Some(KeyCode::KeyR),
        "s" => Some(KeyCode::KeyS),
        "t" => Some(KeyCode::KeyT),
        "u" => Some(KeyCode::KeyU),
        "v" => Some(KeyCode::KeyV),
        "w" => Some(KeyCode::KeyW),
        "x" => Some(KeyCode::KeyX),
        "y" => Some(KeyCode::KeyY),
        "z" => Some(KeyCode::KeyZ),
        "0" => Some(KeyCode::Digit0),
        "1" => Some(KeyCode::Digit1),
        "2" => Some(KeyCode::Digit2),
        "3" => Some(KeyCode::Digit3),
        "4" => Some(KeyCode::Digit4),
        "5" => Some(KeyCode::Digit5),
        "6" => Some(KeyCode::Digit6),
        "7" => Some(KeyCode::Digit7),
        "8" => Some(KeyCode::Digit8),
        "9" => Some(KeyCode::Digit9),
        "backslash" => Some(KeyCode::Backslash),
        _ => None,
    }
}

pub fn editor_mode_str(mode: EditorMode) -> &'static str {
    match mode {
        EditorMode::Insert => "insert",
        EditorMode::Normal => "normal",
        EditorMode::Visual => "visual",
        EditorMode::PaletteFocus => "palette",
        EditorMode::TerminalFocus => "terminal",
    }
}

// ── ResolvedKeymap ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct BindingKey {
    /// None = global (applies to all modes)
    mode: Option<String>,
    spec: KeySpec,
}

/// The fully resolved keymap after all layers have been applied.
///
/// Maps (mode, KeySpec) → command_id string.
/// Lookup priority: mode-specific binding wins over global binding.
#[derive(Debug, Clone)]
pub struct ResolvedKeymap {
    bindings: HashMap<BindingKey, String>,
}

impl ResolvedKeymap {
    fn new() -> Self {
        Self {
            bindings: HashMap::new(),
        }
    }

    fn insert(&mut self, mode: Option<&str>, spec: KeySpec, cmd_id: &str) {
        self.bindings.insert(
            BindingKey {
                mode: mode.map(str::to_string),
                spec,
            },
            cmd_id.to_string(),
        );
    }

    /// Merge `other` on top of `self` (override semantics).
    pub fn apply_overrides(&mut self, other: Self) {
        self.bindings.extend(other.bindings);
    }

    /// Look up the command ID for a non-leader input in the given mode.
    /// Mode-specific bindings take priority over global bindings.
    pub fn lookup(&self, input: &NormalizedInput, mode_str: &str) -> Option<&str> {
        // Build candidate KeySpec list for this input
        let specs = input_to_specs(input);
        for spec in specs {
            // Mode-specific first
            let mk = BindingKey {
                mode: Some(mode_str.to_string()),
                spec: spec.clone(),
            };
            if let Some(id) = self.bindings.get(&mk) {
                return Some(id);
            }
            // Global fallback
            let gk = BindingKey { mode: None, spec };
            if let Some(id) = self.bindings.get(&gk) {
                return Some(id);
            }
        }
        None
    }

    /// Look up only mode-specific bindings (do not fallback to global bindings).
    pub fn lookup_mode_only(&self, input: &NormalizedInput, mode_str: &str) -> Option<&str> {
        let specs = input_to_specs(input);
        for spec in specs {
            let mk = BindingKey {
                mode: Some(mode_str.to_string()),
                spec,
            };
            if let Some(id) = self.bindings.get(&mk) {
                return Some(id);
            }
        }
        None
    }

    /// Look up the command ID for a leader-prefixed input.
    /// Leader bindings are stored with `LeaderThen(code)` and mode = None.
    pub fn lookup_leader(&self, input: &NormalizedInput) -> Option<&str> {
        let code = input.physical_key?;
        let key = BindingKey {
            mode: None,
            spec: KeySpec::LeaderThen(code),
        };
        self.bindings.get(&key).map(String::as_str)
    }

    /// Build a `ResolvedKeymap` from a list of `KeyBinding` entries loaded
    /// from TOML.  Invalid key strings are logged and skipped.
    pub fn from_bindings(bindings: &[KeyBinding]) -> Self {
        let mut km = Self::new();
        for b in bindings {
            match parse_key_spec(&b.key) {
                Some(spec) => km.insert(b.mode.as_deref(), spec, &b.command),
                None => eprintln!(
                    "[keymap] cannot parse key '{}' for command '{}' — skipped",
                    b.key, b.command
                ),
            }
        }
        km
    }
}

fn input_to_specs(input: &NormalizedInput) -> Vec<KeySpec> {
    let mut specs = Vec::with_capacity(2);
    if input.has_command_modifier() {
        if let Some(code) = input.physical_key {
            specs.push(KeySpec::ModPlus(code));
        }
    } else {
        if let Some(named) = input.named_key {
            specs.push(KeySpec::Named(named));
        }
        if let Some(code) = input.physical_key {
            specs.push(KeySpec::Physical(code));
        }
    }
    specs
}

// ── Built-in Rust defaults ────────────────────────────────────────────────────

/// Comprehensive built-in defaults mirroring the previously hardcoded behavior.
/// These are ALWAYS applied as the base layer — TOML profiles override on top.
/// The app is always fully usable even if all TOML files are missing or broken.
pub fn builtin_defaults() -> ResolvedKeymap {
    use command_ids::*;

    // Aliases to avoid glob-import ambiguity between KeyCode and NamedKey
    let nk = |n: NamedKey| KeySpec::Named(n);
    let ph = |c: KeyCode| KeySpec::Physical(c);
    let mp = |c: KeyCode| KeySpec::ModPlus(c);
    let ld = |c: KeyCode| KeySpec::LeaderThen(c);

    let mut km = ResolvedKeymap::new();

    // ── Global (mod+key) shortcuts — apply across all modes ──────────────────
    km.insert(None, mp(KeyCode::KeyS), SAVE_FILE);
    km.insert(None, mp(KeyCode::KeyO), OPEN_FILE);
    km.insert(None, mp(KeyCode::KeyP), OPEN_FILE_FINDER);
    km.insert(None, mp(KeyCode::Backslash), TOGGLE_TERMINAL);
    km.insert(None, mp(KeyCode::KeyE), TOGGLE_EXPLORER);

    // ── Insert mode ───────────────────────────────────────────────────────────
    km.insert(Some("insert"), nk(NamedKey::Escape), ENTER_NORMAL);
    km.insert(Some("insert"), nk(NamedKey::Backspace), BACKSPACE);
    km.insert(Some("insert"), nk(NamedKey::Enter), NEWLINE);
    km.insert(Some("insert"), nk(NamedKey::ArrowLeft), MOVE_LEFT);
    km.insert(Some("insert"), nk(NamedKey::ArrowRight), MOVE_RIGHT);
    km.insert(Some("insert"), nk(NamedKey::ArrowUp), MOVE_UP);
    km.insert(Some("insert"), nk(NamedKey::ArrowDown), MOVE_DOWN);

    // ── Normal mode ───────────────────────────────────────────────────────────
    km.insert(Some("normal"), nk(NamedKey::Escape), ENTER_NORMAL);
    km.insert(Some("normal"), nk(NamedKey::ArrowLeft), MOVE_LEFT);
    km.insert(Some("normal"), nk(NamedKey::ArrowRight), MOVE_RIGHT);
    km.insert(Some("normal"), nk(NamedKey::ArrowUp), MOVE_UP);
    km.insert(Some("normal"), nk(NamedKey::ArrowDown), MOVE_DOWN);
    km.insert(Some("normal"), ph(KeyCode::KeyH), MOVE_LEFT);
    km.insert(Some("normal"), ph(KeyCode::KeyJ), MOVE_DOWN);
    km.insert(Some("normal"), ph(KeyCode::KeyK), MOVE_UP);
    km.insert(Some("normal"), ph(KeyCode::KeyL), MOVE_RIGHT);
    km.insert(Some("normal"), ph(KeyCode::KeyI), ENTER_INSERT);
    km.insert(Some("normal"), ph(KeyCode::KeyV), ENTER_VISUAL);
    km.insert(Some("normal"), ph(KeyCode::Backquote), TOGGLE_TERMINAL);

    // ── Visual mode ───────────────────────────────────────────────────────────
    km.insert(Some("visual"), nk(NamedKey::Escape), ENTER_NORMAL);
    km.insert(Some("visual"), nk(NamedKey::ArrowLeft), MOVE_LEFT);
    km.insert(Some("visual"), nk(NamedKey::ArrowRight), MOVE_RIGHT);
    km.insert(Some("visual"), nk(NamedKey::ArrowUp), MOVE_UP);
    km.insert(Some("visual"), nk(NamedKey::ArrowDown), MOVE_DOWN);
    km.insert(Some("visual"), ph(KeyCode::KeyH), MOVE_LEFT);
    km.insert(Some("visual"), ph(KeyCode::KeyJ), MOVE_DOWN);
    km.insert(Some("visual"), ph(KeyCode::KeyK), MOVE_UP);
    km.insert(Some("visual"), ph(KeyCode::KeyL), MOVE_RIGHT);

    // ── Palette focus (file picker) ───────────────────────────────────────────
    km.insert(Some("palette"), nk(NamedKey::Enter), FILE_PICKER_CONFIRM);
    km.insert(
        Some("palette"),
        nk(NamedKey::ArrowUp),
        FILE_PICKER_SELECT_PREV,
    );
    km.insert(
        Some("palette"),
        nk(NamedKey::ArrowDown),
        FILE_PICKER_SELECT_NEXT,
    );
    km.insert(
        Some("palette"),
        nk(NamedKey::Backspace),
        FILE_PICKER_BACKSPACE,
    );
    km.insert(Some("palette"), nk(NamedKey::Space), FILE_PICKER_BACKSPACE);

    // ── Terminal focus mode bindings (mode-only lookup in InputMap) ──────────
    km.insert(Some("terminal"), nk(NamedKey::Escape), FOCUS_EDITOR);
    km.insert(Some("terminal"), ph(KeyCode::Backquote), TOGGLE_TERMINAL);
    km.insert(Some("terminal"), mp(KeyCode::Backslash), TOGGLE_TERMINAL);

    // ── Explorer focus mode bindings (mode-only lookup in InputMap) ──────────
    km.insert(Some("explorer"), nk(NamedKey::Escape), FOCUS_EDITOR);
    km.insert(
        Some("explorer"),
        nk(NamedKey::ArrowDown),
        EXPLORER_MOVE_DOWN,
    );
    km.insert(Some("explorer"), ph(KeyCode::KeyJ), EXPLORER_MOVE_DOWN);
    km.insert(Some("explorer"), nk(NamedKey::ArrowUp), EXPLORER_MOVE_UP);
    km.insert(Some("explorer"), ph(KeyCode::KeyK), EXPLORER_MOVE_UP);
    km.insert(
        Some("explorer"),
        nk(NamedKey::ArrowLeft),
        EXPLORER_COLLAPSE_OR_PARENT,
    );
    km.insert(
        Some("explorer"),
        ph(KeyCode::KeyH),
        EXPLORER_COLLAPSE_OR_PARENT,
    );
    km.insert(
        Some("explorer"),
        nk(NamedKey::ArrowRight),
        EXPLORER_EXPAND_OR_CHILD,
    );
    km.insert(
        Some("explorer"),
        ph(KeyCode::KeyL),
        EXPLORER_EXPAND_OR_CHILD,
    );
    km.insert(
        Some("explorer"),
        nk(NamedKey::Enter),
        EXPLORER_TOGGLE_OR_OPEN,
    );

    // ── Global Ctrl+W → focus back to editor ─────────────────────────────────
    km.insert(None, mp(KeyCode::KeyW), FOCUS_BACK);

    // ── Leader prefix bindings (stored as LeaderThen, mode = global) ─────────
    km.insert(None, ld(KeyCode::KeyF), OPEN_FILE_FINDER);
    km.insert(None, ld(KeyCode::KeyP), OPEN_COMMAND_PALETTE);
    km.insert(None, ld(KeyCode::KeyT), TOGGLE_TERMINAL);
    km.insert(None, ld(KeyCode::KeyE), FOCUS_EXPLORER);
    km.insert(None, ld(KeyCode::KeyI), FOCUS_INSPECTOR);
    km.insert(None, ld(KeyCode::KeyB), FOCUS_TERMINAL);

    km
}

// ── Convenience constructor ───────────────────────────────────────────────────

/// Build the final `ResolvedKeymap` for a startup:
///   built-in defaults  →  TOML profile bindings  →  (user overrides already merged in `bindings`)
pub fn build(toml_bindings: &[KeyBinding]) -> ResolvedKeymap {
    let mut km = builtin_defaults();
    km.apply_overrides(ResolvedKeymap::from_bindings(toml_bindings));
    km
}

// ── Lookup helper: convert command ID → Command ───────────────────────────────

/// Resolve a command ID from the keymap to a concrete `Command`, providing
/// the open-file path needed for `editor.open_file`.
pub fn resolve_command(
    keymap: &ResolvedKeymap,
    input: &NormalizedInput,
    mode_str: &str,
    open_file_path: &std::path::Path,
) -> Option<Command> {
    let id = keymap.lookup(input, mode_str)?;
    command_ids::parse(id, Some(open_file_path))
}

pub fn resolve_command_mode_only(
    keymap: &ResolvedKeymap,
    input: &NormalizedInput,
    mode_str: &str,
    open_file_path: &std::path::Path,
) -> Option<Command> {
    let id = keymap.lookup_mode_only(input, mode_str)?;
    command_ids::parse(id, Some(open_file_path))
}

pub fn resolve_leader_command(
    keymap: &ResolvedKeymap,
    input: &NormalizedInput,
    open_file_path: &std::path::Path,
) -> Option<Command> {
    let id = keymap.lookup_leader(input)?;
    command_ids::parse(id, Some(open_file_path))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_named_keys() {
        assert_eq!(
            parse_key_spec("Escape"),
            Some(KeySpec::Named(NamedKey::Escape))
        );
        assert_eq!(
            parse_key_spec("Enter"),
            Some(KeySpec::Named(NamedKey::Enter))
        );
        assert_eq!(
            parse_key_spec("ArrowUp"),
            Some(KeySpec::Named(NamedKey::ArrowUp))
        );
    }

    #[test]
    fn parse_physical_keys() {
        assert_eq!(parse_key_spec("h"), Some(KeySpec::Physical(KeyCode::KeyH)));
        assert_eq!(parse_key_spec("j"), Some(KeySpec::Physical(KeyCode::KeyJ)));
        assert_eq!(
            parse_key_spec("backtick"),
            Some(KeySpec::Physical(KeyCode::Backquote))
        );
    }

    #[test]
    fn parse_modifier_key() {
        assert_eq!(
            parse_key_spec("mod+s"),
            Some(KeySpec::ModPlus(KeyCode::KeyS))
        );
    }

    #[test]
    fn parse_leader_key() {
        assert_eq!(
            parse_key_spec("<leader>f"),
            Some(KeySpec::LeaderThen(KeyCode::KeyF))
        );
    }

    #[test]
    fn parse_unknown_key_returns_none() {
        assert!(parse_key_spec("XF86AudioPlay").is_none());
    }

    #[test]
    fn builtin_defaults_has_hjkl_in_normal() {
        use winit::keyboard::ModifiersState;
        let km = builtin_defaults();
        let input = NormalizedInput {
            physical_key: Some(KeyCode::KeyJ),
            named_key: None,
            text: Some("j".into()),
            modifiers: ModifiersState::empty(),
        };
        let id = km.lookup(&input, "normal");
        assert_eq!(id, Some(command_ids::MOVE_DOWN));
    }

    #[test]
    fn builtin_defaults_has_no_hjkl_in_insert() {
        use winit::keyboard::ModifiersState;
        let km = builtin_defaults();
        let input = NormalizedInput {
            physical_key: Some(KeyCode::KeyJ),
            named_key: None,
            text: Some("j".into()),
            modifiers: ModifiersState::empty(),
        };
        // In insert mode j should NOT be bound (falls through to InsertChar)
        assert_eq!(km.lookup(&input, "insert"), None);
    }
}
