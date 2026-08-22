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
    /// Shift + physical key: "H", "L", etc.
    ShiftPlus(KeyCode),
    /// Command/Super modifier + physical key: "cmd+s" (macOS ⌘)
    CmdPlus(KeyCode),
    /// Command/Super + Shift + physical key: "cmd+shift+p"
    CmdShiftPlus(KeyCode),
    /// Ctrl modifier + physical key: "ctrl+o"
    CtrlPlus(KeyCode),
    /// Ctrl + Shift + physical key: "ctrl+shift+o"
    CtrlShiftPlus(KeyCode),
    /// Printable character key (layout-aware), e.g. ":".
    Char(char),
    /// Leader token (Space) used as the first step of a chord sequence.
    Leader,
}

impl KeySpec {
    /// Returns true if this spec matches the given input event.
    pub fn matches(&self, input: &NormalizedInput) -> bool {
        match self {
            Self::Named(named) => !input.has_command_modifier() && input.named_key == Some(*named),
            Self::Physical(code) => {
                !input.has_command_modifier()
                    && !input.modifiers.shift_key()
                    && input.physical_key == Some(*code)
            }
            Self::ShiftPlus(code) => {
                !input.has_command_modifier()
                    && input.modifiers.shift_key()
                    && input.physical_key == Some(*code)
            }
            Self::CmdPlus(code) => {
                input.modifiers.super_key()
                    && !input.modifiers.shift_key()
                    && input.physical_key == Some(*code)
            }
            Self::CmdShiftPlus(code) => {
                input.modifiers.super_key()
                    && input.modifiers.shift_key()
                    && input.physical_key == Some(*code)
            }
            Self::CtrlPlus(code) => {
                input.modifiers.control_key()
                    && !input.modifiers.shift_key()
                    && input.physical_key == Some(*code)
            }
            Self::CtrlShiftPlus(code) => {
                input.modifiers.control_key()
                    && input.modifiers.shift_key()
                    && input.physical_key == Some(*code)
            }
            Self::Char(ch) => {
                !input.has_command_modifier()
                    && input
                        .text
                        .as_ref()
                        .is_some_and(|text| text.chars().count() == 1 && text.starts_with(*ch))
            }
            Self::Leader => is_leader_input(input),
        }
    }

    pub fn display_token(&self) -> String {
        match self {
            Self::Named(named) => named_key_display(*named),
            Self::Physical(code) => physical_key_display(*code),
            Self::ShiftPlus(code) => physical_key_display(*code).to_ascii_uppercase(),
            Self::CmdPlus(code) => format!("cmd+{}", physical_key_display(*code)),
            Self::CmdShiftPlus(code) => format!("cmd+shift+{}", physical_key_display(*code)),
            Self::CtrlPlus(code) => format!("ctrl+{}", physical_key_display(*code)),
            Self::CtrlShiftPlus(code) => format!("ctrl+shift+{}", physical_key_display(*code)),
            Self::Char(ch) => ch.to_string(),
            Self::Leader => "<Space>".to_string(),
        }
    }
}

fn named_key_display(named: NamedKey) -> String {
    match named {
        NamedKey::Space => "<Space>".to_string(),
        NamedKey::Escape => "<Esc>".to_string(),
        NamedKey::Enter => "<Enter>".to_string(),
        NamedKey::Backspace => "<Backspace>".to_string(),
        NamedKey::Tab => "<Tab>".to_string(),
        NamedKey::ArrowUp => "<Up>".to_string(),
        NamedKey::ArrowDown => "<Down>".to_string(),
        NamedKey::ArrowLeft => "<Left>".to_string(),
        NamedKey::ArrowRight => "<Right>".to_string(),
        NamedKey::F1 => "<F1>".to_string(),
        NamedKey::F5 => "<F5>".to_string(),
        NamedKey::F10 => "<F10>".to_string(),
        NamedKey::F12 => "<F12>".to_string(),
        _ => format!("<{named:?}>"),
    }
}

fn physical_key_display(code: KeyCode) -> String {
    match code {
        KeyCode::KeyA => "a".to_string(),
        KeyCode::KeyB => "b".to_string(),
        KeyCode::KeyC => "c".to_string(),
        KeyCode::KeyD => "d".to_string(),
        KeyCode::KeyE => "e".to_string(),
        KeyCode::KeyF => "f".to_string(),
        KeyCode::KeyG => "g".to_string(),
        KeyCode::KeyH => "h".to_string(),
        KeyCode::KeyI => "i".to_string(),
        KeyCode::KeyJ => "j".to_string(),
        KeyCode::KeyK => "k".to_string(),
        KeyCode::KeyL => "l".to_string(),
        KeyCode::KeyM => "m".to_string(),
        KeyCode::KeyN => "n".to_string(),
        KeyCode::KeyO => "o".to_string(),
        KeyCode::KeyP => "p".to_string(),
        KeyCode::KeyQ => "q".to_string(),
        KeyCode::KeyR => "r".to_string(),
        KeyCode::KeyS => "s".to_string(),
        KeyCode::KeyT => "t".to_string(),
        KeyCode::KeyU => "u".to_string(),
        KeyCode::KeyV => "v".to_string(),
        KeyCode::KeyW => "w".to_string(),
        KeyCode::KeyX => "x".to_string(),
        KeyCode::KeyY => "y".to_string(),
        KeyCode::KeyZ => "z".to_string(),
        KeyCode::Digit0 => "0".to_string(),
        KeyCode::Digit1 => "1".to_string(),
        KeyCode::Digit2 => "2".to_string(),
        KeyCode::Digit3 => "3".to_string(),
        KeyCode::Digit4 => "4".to_string(),
        KeyCode::Digit5 => "5".to_string(),
        KeyCode::Digit6 => "6".to_string(),
        KeyCode::Digit7 => "7".to_string(),
        KeyCode::Digit8 => "8".to_string(),
        KeyCode::Digit9 => "9".to_string(),
        KeyCode::Comma => ",".to_string(),
        KeyCode::Semicolon => ";".to_string(),
        KeyCode::Backslash => "\\".to_string(),
        KeyCode::Backquote => "`".to_string(),
        KeyCode::Space => "<Space>".to_string(),
        _ => format!("<{code:?}>"),
    }
}

/// Parse a key string from a TOML binding entry into a chord sequence.
/// Examples:
/// - "d d" -> [Physical(KeyD), Physical(KeyD)]
/// - "<leader>f f" -> [Leader, Physical(KeyF), Physical(KeyF)]
pub fn parse_key_sequence(s: &str) -> Option<Vec<KeySpec>> {
    let mut steps = Vec::new();
    for token in s.split_whitespace() {
        let parsed = parse_key_token(token)?;
        steps.extend(parsed);
    }
    (!steps.is_empty()).then_some(steps)
}

/// Backward-compatible helper used by older tests/callers.
/// Returns `Some` only for single-step key specs.
pub fn parse_key_spec(s: &str) -> Option<KeySpec> {
    let sequence = parse_key_sequence(s)?;
    (sequence.len() == 1).then(|| sequence[0].clone())
}

fn parse_key_token(token: &str) -> Option<Vec<KeySpec>> {
    // Check for <leader> prefix first (case-insensitive)
    let lower = token.to_ascii_lowercase();
    if lower == "<leader>" {
        return Some(vec![KeySpec::Leader]);
    }
    if let Some(rest) = lower.strip_prefix("<leader>") {
        if rest.is_empty() {
            return Some(vec![KeySpec::Leader]);
        }
        return parse_non_leader_key(rest).map(|spec| vec![KeySpec::Leader, spec]);
    }

    // For modifier keys, normalize to lowercase
    // For single characters, preserve case
    let normalized = if token.contains('+') || token.len() > 1 {
        token.to_ascii_lowercase()
    } else {
        token.to_string()
    };

    parse_non_leader_key(&normalized).map(|spec| vec![spec])
}

fn parse_non_leader_key(token: &str) -> Option<KeySpec> {
    if let Some(rest) = token.strip_prefix("cmd+shift+") {
        return char_key_to_code(rest).map(KeySpec::CmdShiftPlus);
    }
    if let Some(rest) = token.strip_prefix("cmd+") {
        return char_key_to_code(rest).map(KeySpec::CmdPlus);
    }
    if let Some(rest) = token.strip_prefix("ctrl+shift+") {
        return char_key_to_code(rest).map(KeySpec::CtrlShiftPlus);
    }
    if let Some(rest) = token.strip_prefix("ctrl+") {
        return char_key_to_code(rest).map(KeySpec::CtrlPlus);
    }

    let named = match token.to_ascii_lowercase().as_str() {
        "esc" | "escape" => Some(NamedKey::Escape),
        "backspace" => Some(NamedKey::Backspace),
        "delete" => Some(NamedKey::Delete),
        "enter" => Some(NamedKey::Enter),
        "home" => Some(NamedKey::Home),
        "end" => Some(NamedKey::End),
        "space" => Some(NamedKey::Space),
        "arrowup" => Some(NamedKey::ArrowUp),
        "arrowdown" => Some(NamedKey::ArrowDown),
        "arrowleft" => Some(NamedKey::ArrowLeft),
        "arrowright" => Some(NamedKey::ArrowRight),
        "tab" => Some(NamedKey::Tab),
        "f1" => Some(NamedKey::F1),
        "f2" => Some(NamedKey::F2),
        "f3" => Some(NamedKey::F3),
        "f4" => Some(NamedKey::F4),
        "f5" => Some(NamedKey::F5),
        "f6" => Some(NamedKey::F6),
        "f7" => Some(NamedKey::F7),
        "f8" => Some(NamedKey::F8),
        "f9" => Some(NamedKey::F9),
        "f10" => Some(NamedKey::F10),
        "f11" => Some(NamedKey::F11),
        "f12" => Some(NamedKey::F12),
        _ => None,
    };
    if let Some(n) = named {
        return Some(KeySpec::Named(n));
    }

    // Single-character token: lowercase letter/digit prefers physical key;
    // uppercase/punctuation stays as layout-aware Char to preserve case.
    if token.chars().count() == 1
        && let Some(ch) = token.chars().next()
        && !ch.is_control()
    {
        if ch.is_ascii_lowercase() || ch.is_ascii_digit() {
            return char_key_to_code(token).map(KeySpec::Physical);
        }
        if ch.is_ascii_uppercase() {
            return char_key_to_code(token).map(KeySpec::ShiftPlus);
        }
        return Some(KeySpec::Char(ch));
    }

    char_key_to_code(token).map(KeySpec::Physical)
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
        "," | "comma" => Some(KeyCode::Comma),
        "semicolon" => Some(KeyCode::Semicolon),
        "backslash" => Some(KeyCode::Backslash),
        "`" | "backtick" | "grave" => Some(KeyCode::Backquote),
        "space" => Some(KeyCode::Space),
        _ => None,
    }
}

pub fn is_leader_input(input: &NormalizedInput) -> bool {
    !input.has_command_modifier()
        && (input.named_key == Some(NamedKey::Space)
            || input.physical_key == Some(KeyCode::Space)
            || input.text.as_deref() == Some(" "))
}

pub fn editor_mode_str(mode: EditorMode) -> &'static str {
    match mode {
        EditorMode::Insert => "insert",
        EditorMode::Normal => "normal",
        EditorMode::Visual => "visual",
        EditorMode::VisualBlock => "visual_block",
        EditorMode::PaletteFocus => "palette",
        EditorMode::TerminalFocus => "terminal",
        EditorMode::TerminalNormal => "terminal_normal",
        EditorMode::MultiCursor => "multicursor",
        EditorMode::MultiInsert => "multiinsert",
        EditorMode::Resize => "resize",
    }
}

// ── ResolvedKeymap ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct BindingKey {
    /// None = global (applies to all modes)
    mode: Option<String>,
    spec: KeySpec,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct SequenceBindingKey {
    /// None = global (applies to all modes)
    mode: Option<String>,
    steps: Vec<KeySpec>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SequenceLookup<'a> {
    None,
    Prefix,
    Exact(&'a str),
}

/// One possible continuation of a pending chord prefix (which-key overlay).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SequenceContinuation {
    /// Display token of the next key (e.g. "f", "<Space>").
    pub key: String,
    /// Command id when prefix+key is an exact binding; `None` for a group
    /// where deeper chords continue.
    pub command_id: Option<String>,
}

/// The fully resolved keymap after all layers have been applied.
///
/// Maps (mode, KeySpec) → command_id string.
/// Lookup priority: mode-specific binding wins over global binding.
#[derive(Debug, Clone)]
pub struct ResolvedKeymap {
    bindings: HashMap<BindingKey, String>,
    sequences: HashMap<SequenceBindingKey, String>,
}

impl ResolvedKeymap {
    fn new() -> Self {
        Self {
            bindings: HashMap::new(),
            sequences: HashMap::new(),
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

    fn insert_sequence(&mut self, mode: Option<&str>, steps: Vec<KeySpec>, cmd_id: &str) {
        self.sequences.insert(
            SequenceBindingKey {
                mode: mode.map(str::to_string),
                steps,
            },
            cmd_id.to_string(),
        );
    }

    /// Merge `other` on top of `self` (override semantics).
    pub fn apply_overrides(&mut self, other: Self) {
        self.bindings.extend(other.bindings);
        self.sequences.extend(other.sequences);
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

    /// Look up global (mode=None) bindings only.
    pub fn lookup_global(&self, input: &NormalizedInput) -> Option<&str> {
        let specs = input_to_specs(input);
        for spec in specs {
            let gk = BindingKey { mode: None, spec };
            if let Some(id) = self.bindings.get(&gk) {
                return Some(id);
            }
        }
        None
    }

    /// Match a chord sequence in the current mode.
    /// Priority: mode-specific exact > global exact > mode-specific prefix > global prefix.
    pub fn lookup_sequence(&self, steps: &[KeySpec], mode_str: &str) -> SequenceLookup<'_> {
        let mut mode_exact: Option<&str> = None;
        let mut global_exact: Option<&str> = None;
        let mut mode_prefix = false;
        let mut global_prefix = false;

        for (key, id) in &self.sequences {
            let scope = key.mode.as_deref();
            let in_mode_scope = scope == Some(mode_str);
            let in_global_scope = scope.is_none();
            if !in_mode_scope && !in_global_scope {
                continue;
            }
            if !key.steps.starts_with(steps) {
                continue;
            }

            if key.steps.len() == steps.len() {
                if in_mode_scope {
                    mode_exact = Some(id.as_str());
                } else if in_global_scope {
                    global_exact = Some(id.as_str());
                }
            } else if in_mode_scope {
                mode_prefix = true;
            } else if in_global_scope {
                global_prefix = true;
            }
        }

        if let Some(id) = mode_exact.or(global_exact) {
            return SequenceLookup::Exact(id);
        }
        if mode_prefix || global_prefix {
            return SequenceLookup::Prefix;
        }
        SequenceLookup::None
    }

    /// Enumerate every distinct next key continuing `prefix` in the given mode,
    /// for the which-key overlay. `command_id` is `Some` when prefix+key is an
    /// exact binding (mode-specific wins over global, mirroring
    /// `lookup_sequence`), `None` when only deeper chords follow (+group).
    pub fn sequence_continuations(
        &self,
        prefix: &[KeySpec],
        mode_str: &str,
    ) -> Vec<SequenceContinuation> {
        #[derive(Default)]
        struct Acc {
            mode_exact: Option<String>,
            global_exact: Option<String>,
        }
        let mut by_key: HashMap<String, Acc> = HashMap::new();
        for (key, id) in &self.sequences {
            let scope = key.mode.as_deref();
            let in_mode_scope = scope == Some(mode_str);
            if !in_mode_scope && scope.is_some() {
                continue;
            }
            if key.steps.len() <= prefix.len() || !key.steps.starts_with(prefix) {
                continue;
            }
            let token = key.steps[prefix.len()].display_token();
            let acc = by_key.entry(token).or_default();
            if key.steps.len() == prefix.len() + 1 {
                if in_mode_scope {
                    acc.mode_exact = Some(id.clone());
                } else {
                    acc.global_exact = Some(id.clone());
                }
            }
        }
        let mut out: Vec<SequenceContinuation> = by_key
            .into_iter()
            .map(|(key, acc)| SequenceContinuation {
                key,
                command_id: acc.mode_exact.or(acc.global_exact),
            })
            .collect();
        out.sort_by(|a, b| {
            a.key
                .to_ascii_lowercase()
                .cmp(&b.key.to_ascii_lowercase())
                .then_with(|| a.key.cmp(&b.key))
        });
        out
    }

    /// Build a `ResolvedKeymap` from a list of `KeyBinding` entries loaded
    /// from TOML.  Invalid key strings are logged and skipped.
    pub fn from_bindings(bindings: &[KeyBinding]) -> Self {
        let mut km = Self::new();
        for b in bindings {
            match parse_key_sequence(&b.key) {
                Some(steps) if steps.len() == 1 => {
                    km.insert(b.mode.as_deref(), steps[0].clone(), &b.command)
                }
                Some(steps) => km.insert_sequence(b.mode.as_deref(), steps, &b.command),
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
    let mut specs = Vec::with_capacity(4);

    // Check for Cmd/Super modifier
    if input.modifiers.super_key() {
        if let Some(code) = input.physical_key {
            if input.modifiers.shift_key() {
                specs.push(KeySpec::CmdShiftPlus(code));
            } else {
                specs.push(KeySpec::CmdPlus(code));
            }
        }
    }
    // Check for Ctrl modifier (independent of Cmd)
    else if input.modifiers.control_key() {
        if let Some(code) = input.physical_key {
            if input.modifiers.shift_key() {
                specs.push(KeySpec::CtrlShiftPlus(code));
            } else {
                specs.push(KeySpec::CtrlPlus(code));
            }
        }
    }
    // No command modifiers
    else {
        if let Some(named) = input.named_key {
            specs.push(KeySpec::Named(named));
        }
        if input.modifiers.shift_key()
            && let Some(code) = input.physical_key
        {
            specs.push(KeySpec::ShiftPlus(code));
        }
        // Char needs to be checked before Physical so shifted keys like `I` / `O`
        // can override lowercase physical bindings (`i` / `o`) in normal mode.
        if let Some(text) = &input.text
            && text.chars().count() == 1
            && let Some(ch) = text.chars().next()
            && !ch.is_control()
        {
            specs.push(KeySpec::Char(ch));
        }
        if let Some(code) = input.physical_key {
            specs.push(KeySpec::Physical(code));
        }
    }
    specs
}

pub fn sequence_step_candidates(input: &NormalizedInput, allow_leader: bool) -> Vec<KeySpec> {
    let mut specs = Vec::new();
    if allow_leader && is_leader_input(input) {
        specs.push(KeySpec::Leader);
    }
    specs.extend(input_to_specs(input));
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
    let cmd = |c: KeyCode| KeySpec::CmdPlus(c);
    let ch = |c: char| KeySpec::Char(c);
    let seq = |steps: &[KeySpec]| steps.to_vec();

    let mut km = ResolvedKeymap::new();

    // ── Global (cmd+key) shortcuts — apply across all modes ──────────────────
    km.insert(None, cmd(KeyCode::KeyS), SAVE_FILE);
    km.insert(None, cmd(KeyCode::KeyO), OPEN_FILE);
    km.insert(None, cmd(KeyCode::KeyP), OPEN_FILE_PICKER);
    km.insert(
        None,
        KeySpec::CmdShiftPlus(KeyCode::KeyP),
        OPEN_COMMAND_PALETTE,
    );
    km.insert(None, cmd(KeyCode::KeyB), TOGGLE_LEFT_DOCK);
    km.insert(None, KeySpec::CtrlPlus(KeyCode::KeyF), FOCUS_EXPLORER);
    km.insert(None, KeySpec::CtrlPlus(KeyCode::KeyP), FOCUS_OUTLINE);
    km.insert(None, cmd(KeyCode::KeyR), FOCUS_INSPECTOR);
    km.insert(None, nk(NamedKey::F12), FOCUS_TERMINAL);
    km.insert(None, nk(NamedKey::F10), CANVAS_OPEN);
    km.insert(None, cmd(KeyCode::Backslash), TOGGLE_BOTTOM_DOCK);
    km.insert(None, cmd(KeyCode::Digit1), RIGHT_DOCK_SWITCH_1);
    km.insert(None, cmd(KeyCode::Digit2), RIGHT_DOCK_SWITCH_2);
    km.insert(None, cmd(KeyCode::Digit3), RIGHT_DOCK_SWITCH_3);
    km.insert(None, cmd(KeyCode::Digit4), RIGHT_DOCK_SWITCH_4);
    km.insert(None, cmd(KeyCode::Digit5), RIGHT_DOCK_SWITCH_5);
    km.insert(None, cmd(KeyCode::Digit6), RIGHT_DOCK_SWITCH_6);
    km.insert(None, cmd(KeyCode::Digit7), RIGHT_DOCK_SWITCH_7);
    km.insert(None, cmd(KeyCode::Digit8), RIGHT_DOCK_SWITCH_8);
    km.insert(None, cmd(KeyCode::Digit9), RIGHT_DOCK_SWITCH_9);

    // ── Insert mode ───────────────────────────────────────────────────────────
    km.insert(Some("insert"), nk(NamedKey::Escape), ENTER_NORMAL);
    km.insert(Some("insert"), nk(NamedKey::Backspace), BACKSPACE);
    km.insert(Some("insert"), nk(NamedKey::Enter), NEWLINE);
    // Tab inserts indentation; AI ghost text is accepted only via Ctrl+J
    // (and Ctrl+L for word-by-word). Keeping Tab off the accept path avoids the
    // overlap/ambiguity with the LSP completion menu.
    km.insert(Some("insert"), nk(NamedKey::Tab), INSERT_TAB);
    km.insert(Some("insert"), nk(NamedKey::ArrowLeft), MOVE_LEFT);
    km.insert(Some("insert"), nk(NamedKey::ArrowRight), MOVE_RIGHT);
    km.insert(Some("insert"), nk(NamedKey::ArrowUp), MOVE_UP);
    km.insert(Some("insert"), nk(NamedKey::ArrowDown), MOVE_DOWN);
    km.insert(Some("insert"), cmd(KeyCode::KeyV), EDITOR_PASTE);
    km.insert(
        Some("insert"),
        KeySpec::CtrlPlus(KeyCode::KeyJ),
        AI_ACCEPT_INLINE,
    );
    km.insert(
        Some("insert"),
        KeySpec::CtrlPlus(KeyCode::KeyL),
        AI_ACCEPT_INLINE_WORD,
    );

    // ── Normal mode ───────────────────────────────────────────────────────────
    km.insert(
        Some("normal"),
        nk(NamedKey::Escape),
        CLEAR_SEARCH_HIGHLIGHTS,
    );
    km.insert(Some("normal"), nk(NamedKey::ArrowLeft), MOVE_LEFT);
    km.insert(Some("normal"), nk(NamedKey::ArrowRight), MOVE_RIGHT);
    km.insert(Some("normal"), nk(NamedKey::ArrowUp), MOVE_UP);
    km.insert(Some("normal"), nk(NamedKey::ArrowDown), MOVE_DOWN);
    km.insert(Some("normal"), ph(KeyCode::KeyH), MOVE_LEFT);
    km.insert(Some("normal"), ph(KeyCode::KeyJ), MOVE_DOWN);
    km.insert(Some("normal"), ph(KeyCode::KeyK), MOVE_UP);
    km.insert(Some("normal"), ph(KeyCode::KeyL), MOVE_RIGHT);
    km.insert(Some("normal"), ph(KeyCode::KeyW), MOVE_WORD_FORWARD);
    km.insert(Some("normal"), ph(KeyCode::KeyB), MOVE_WORD_BACKWARD);
    km.insert(Some("normal"), ph(KeyCode::KeyE), MOVE_WORD_END);
    km.insert(Some("normal"), ph(KeyCode::KeyT), TOGGLE_COLLAPSE_EXPAND);
    km.insert(Some("normal"), ph(KeyCode::Digit0), MOVE_TO_LINE_START);
    km.insert(Some("normal"), ch('$'), MOVE_TO_LINE_END);
    km.insert(Some("normal"), ch('^'), MOVE_TO_FIRST_NON_WHITESPACE);
    km.insert(Some("normal"), ch('G'), MOVE_TO_LAST_LINE);
    km.insert(Some("normal"), ph(KeyCode::KeyI), ENTER_INSERT);
    km.insert(Some("normal"), ph(KeyCode::KeyV), ENTER_VISUAL);
    km.insert(Some("normal"), ch('V'), ENTER_VISUAL_LINE);
    km.insert(Some("normal"), ph(KeyCode::KeyO), INSERT_LINE_BELOW);
    km.insert(Some("normal"), ch('O'), INSERT_LINE_ABOVE);
    km.insert(Some("normal"), ch('I'), INSERT_AT_LINE_START);
    km.insert(Some("normal"), ch('A'), APPEND_AT_LINE_END);
    km.insert(Some("normal"), ch('C'), CHANGE_TO_LINE_END);
    km.insert(Some("normal"), ch('D'), DELETE_TO_LINE_END);
    km.insert(Some("normal"), ph(KeyCode::KeyA), APPEND_AFTER_CURSOR);
    km.insert(Some("normal"), ch('S'), SUBSTITUTE_LINE);
    km.insert(Some("normal"), ch('J'), JOIN_LINES);
    km.insert(Some("normal"), ph(KeyCode::KeyX), DELETE_CHAR);
    km.insert(Some("normal"), ph(KeyCode::KeyP), PASTE_AFTER);
    km.insert(Some("normal"), ch('P'), PASTE_BEFORE);
    km.insert(
        Some("normal"),
        KeySpec::CtrlPlus(KeyCode::KeyV),
        ENTER_VISUAL_BLOCK,
    );
    km.insert(Some("normal"), ph(KeyCode::KeyU), UNDO);
    km.insert(
        Some("normal"),
        KeySpec::CtrlPlus(KeyCode::KeyU),
        SCROLL_HALF_PAGE_UP,
    );
    km.insert(
        Some("normal"),
        KeySpec::CtrlPlus(KeyCode::KeyD),
        SCROLL_HALF_PAGE_DOWN,
    );
    km.insert(Some("normal"), ch('{'), MOVE_PARAGRAPH_UP);
    km.insert(Some("normal"), ch('}'), MOVE_PARAGRAPH_DOWN);
    km.insert(Some("normal"), ch('%'), MATCH_BRACKET);
    km.insert(
        Some("normal"),
        KeySpec::CtrlPlus(KeyCode::KeyH),
        BUFFER_PREV,
    );
    km.insert(
        Some("normal"),
        KeySpec::CtrlPlus(KeyCode::KeyL),
        BUFFER_NEXT,
    );
    km.insert(Some("normal"), KeySpec::CtrlPlus(KeyCode::KeyR), REDO);
    km.insert(Some("normal"), ch('n'), SEARCH_NEXT);
    km.insert(Some("normal"), ch('N'), SEARCH_PREV);
    km.insert(Some("normal"), ch('*'), SEARCH_WORD_UNDER_CURSOR);
    km.insert(Some("normal"), ch('/'), OPEN_IN_FILE_SEARCH);
    km.insert(Some("normal"), KeySpec::Char(':'), OPEN_VIM_COMMAND);

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
    km.insert(Some("visual"), ph(KeyCode::KeyW), MOVE_WORD_FORWARD);
    km.insert(Some("visual"), ph(KeyCode::KeyB), MOVE_WORD_BACKWARD);
    km.insert(Some("visual"), ph(KeyCode::KeyE), MOVE_WORD_END);
    km.insert(Some("visual"), ph(KeyCode::KeyT), TOGGLE_COLLAPSE_EXPAND);
    km.insert(Some("visual"), ph(KeyCode::Digit0), MOVE_TO_LINE_START);
    km.insert(Some("visual"), ch('$'), MOVE_TO_LINE_END);
    km.insert(Some("visual"), ch('^'), MOVE_TO_FIRST_NON_WHITESPACE);
    km.insert(Some("visual"), ch('G'), MOVE_TO_LAST_LINE);
    km.insert(Some("visual"), ch('{'), MOVE_PARAGRAPH_UP);
    km.insert(Some("visual"), ch('}'), MOVE_PARAGRAPH_DOWN);
    km.insert(Some("visual"), ch('%'), MATCH_BRACKET);
    km.insert(Some("visual"), ch('*'), SEARCH_WORD_UNDER_CURSOR);
    km.insert(Some("visual"), ph(KeyCode::KeyD), DELETE_SELECTION);
    km.insert(Some("visual"), ph(KeyCode::KeyC), CHANGE_SELECTION);
    km.insert(Some("visual"), ph(KeyCode::KeyX), DELETE_SELECTION);
    km.insert(Some("visual"), ph(KeyCode::KeyY), YANK_SELECTION);
    km.insert(Some("visual"), cmd(KeyCode::KeyV), EDITOR_PASTE);

    // ── Visual Block mode ────────────────────────────────────────────────────
    km.insert(Some("visual_block"), nk(NamedKey::Escape), ENTER_NORMAL);
    km.insert(Some("visual_block"), ph(KeyCode::KeyH), MOVE_LEFT);
    km.insert(Some("visual_block"), ph(KeyCode::KeyJ), MOVE_DOWN);
    km.insert(Some("visual_block"), ph(KeyCode::KeyK), MOVE_UP);
    km.insert(Some("visual_block"), ph(KeyCode::KeyL), MOVE_RIGHT);
    km.insert(Some("visual_block"), ch('I'), INSERT_AT_LINE_START);
    km.insert(Some("visual_block"), ch('A'), APPEND_AT_LINE_END);
    km.insert(Some("visual_block"), ph(KeyCode::KeyC), CHANGE_SELECTION);
    km.insert(Some("visual_block"), ph(KeyCode::KeyD), DELETE_SELECTION);

    // ── Palette focus (overlay / command palette / file picker) ─────────────
    km.insert(Some("palette"), nk(NamedKey::Enter), FILE_PICKER_CONFIRM);
    km.insert(Some("palette"), nk(NamedKey::ArrowUp), OVERLAY_SELECT_PREV);
    km.insert(
        Some("palette"),
        nk(NamedKey::ArrowDown),
        OVERLAY_SELECT_NEXT,
    );
    km.insert(
        Some("palette"),
        KeySpec::CtrlPlus(KeyCode::KeyP),
        OVERLAY_SELECT_PREV,
    );
    km.insert(
        Some("palette"),
        KeySpec::CtrlPlus(KeyCode::KeyN),
        OVERLAY_SELECT_NEXT,
    );
    km.insert(
        Some("palette"),
        nk(NamedKey::Backspace),
        FILE_PICKER_BACKSPACE,
    );
    km.insert(Some("palette"), cmd(KeyCode::KeyV), EDITOR_PASTE);

    // ── Palette cursor movement ───────────────────────────────────────────────
    km.insert(
        Some("palette"),
        nk(NamedKey::ArrowLeft),
        PALETTE_MOVE_CURSOR_LEFT,
    );
    km.insert(
        Some("palette"),
        nk(NamedKey::ArrowRight),
        PALETTE_MOVE_CURSOR_RIGHT,
    );
    km.insert(
        Some("palette"),
        nk(NamedKey::Home),
        PALETTE_MOVE_CURSOR_TO_START,
    );
    km.insert(
        Some("palette"),
        nk(NamedKey::End),
        PALETTE_MOVE_CURSOR_TO_END,
    );
    km.insert(
        Some("palette"),
        KeySpec::CtrlPlus(KeyCode::KeyA),
        PALETTE_MOVE_CURSOR_TO_START,
    );
    km.insert(
        Some("palette"),
        KeySpec::CtrlPlus(KeyCode::KeyE),
        PALETTE_MOVE_CURSOR_TO_END,
    );
    km.insert(
        Some("palette"),
        nk(NamedKey::Delete),
        PALETTE_DELETE_CHAR_FORWARD,
    );

    // ── Terminal focus mode bindings (mode-only lookup in InputMap) ──────────
    km.insert(Some("terminal"), nk(NamedKey::Escape), FOCUS_BACK);
    km.insert(
        Some("terminal"),
        KeySpec::CtrlPlus(KeyCode::KeyQ),
        TERMINAL_ENTER_NORMAL_MODE,
    );
    km.insert(Some("terminal"), nk(NamedKey::F12), FOCUS_TERMINAL);
    km.insert(Some("terminal"), cmd(KeyCode::KeyR), FOCUS_INSPECTOR);
    km.insert(Some("terminal"), cmd(KeyCode::KeyV), TERMINAL_PASTE);
    km.insert(Some("terminal"), cmd(KeyCode::KeyT), TERMINAL_TAB_NEW);
    km.insert(Some("terminal"), cmd(KeyCode::KeyW), TERMINAL_TAB_CLOSE);
    km.insert(
        Some("terminal"),
        cmd(KeyCode::Digit1),
        TERMINAL_TAB_SWITCH_1,
    );
    km.insert(
        Some("terminal"),
        cmd(KeyCode::Digit2),
        TERMINAL_TAB_SWITCH_2,
    );
    km.insert(
        Some("terminal"),
        cmd(KeyCode::Digit3),
        TERMINAL_TAB_SWITCH_3,
    );
    km.insert(
        Some("terminal"),
        cmd(KeyCode::Digit4),
        TERMINAL_TAB_SWITCH_4,
    );
    km.insert(
        Some("terminal"),
        cmd(KeyCode::Digit5),
        TERMINAL_TAB_SWITCH_5,
    );
    km.insert(
        Some("terminal"),
        cmd(KeyCode::Digit6),
        TERMINAL_TAB_SWITCH_6,
    );
    km.insert(
        Some("terminal"),
        cmd(KeyCode::Digit7),
        TERMINAL_TAB_SWITCH_7,
    );
    km.insert(
        Some("terminal"),
        cmd(KeyCode::Digit8),
        TERMINAL_TAB_SWITCH_8,
    );
    km.insert(
        Some("terminal"),
        cmd(KeyCode::Digit9),
        TERMINAL_TAB_SWITCH_9,
    );

    // ── Terminal normal mode bindings (copy mode / virtual cursor) ──────────
    km.insert(
        Some("terminal_normal"),
        nk(NamedKey::Escape),
        ENTER_TERMINAL_FOCUS,
    );
    km.insert(Some("terminal_normal"), nk(NamedKey::F12), FOCUS_TERMINAL);
    km.insert(Some("terminal_normal"), cmd(KeyCode::KeyR), FOCUS_INSPECTOR);
    km.insert(Some("terminal_normal"), ph(KeyCode::KeyH), MOVE_LEFT);
    km.insert(Some("terminal_normal"), ph(KeyCode::KeyJ), MOVE_DOWN);
    km.insert(Some("terminal_normal"), ph(KeyCode::KeyK), MOVE_UP);
    km.insert(Some("terminal_normal"), ph(KeyCode::KeyL), MOVE_RIGHT);
    km.insert(
        Some("terminal_normal"),
        ph(KeyCode::KeyW),
        MOVE_WORD_FORWARD,
    );
    km.insert(
        Some("terminal_normal"),
        ph(KeyCode::KeyB),
        MOVE_WORD_BACKWARD,
    );
    km.insert(Some("terminal_normal"), ph(KeyCode::KeyE), MOVE_WORD_END);
    km.insert(
        Some("terminal_normal"),
        ph(KeyCode::KeyT),
        TOGGLE_COLLAPSE_EXPAND,
    );
    km.insert(
        Some("terminal_normal"),
        ph(KeyCode::Digit0),
        MOVE_TO_LINE_START,
    );
    km.insert(Some("terminal_normal"), ch('$'), MOVE_TO_LINE_END);
    km.insert(
        Some("terminal_normal"),
        ch('^'),
        MOVE_TO_FIRST_NON_WHITESPACE,
    );
    km.insert(Some("terminal_normal"), ch('G'), MOVE_TO_LAST_LINE);
    km.insert(
        Some("terminal_normal"),
        KeySpec::CtrlPlus(KeyCode::KeyU),
        SCROLL_HALF_PAGE_UP,
    );
    km.insert(
        Some("terminal_normal"),
        KeySpec::CtrlPlus(KeyCode::KeyD),
        SCROLL_HALF_PAGE_DOWN,
    );
    km.insert(Some("terminal_normal"), ph(KeyCode::KeyV), ENTER_VISUAL);
    km.insert(Some("terminal_normal"), ch('V'), ENTER_VISUAL_LINE);
    km.insert(Some("terminal_normal"), ph(KeyCode::KeyY), YANK_SELECTION);
    km.insert(Some("terminal_normal"), cmd(KeyCode::KeyV), TERMINAL_PASTE);
    // ── Vim-style shell line editing (Warp-style) ────────────────────────────
    // Khi viewport ở live prompt (scroll_offset == 0), các op này dịch sang
    // readline sequences và shell tự edit input line tại vị trí cursor.
    // Khi đang scroll xem scrollback, h/l/w/b/e/0/$ giữ hành vi virtual cursor.
    km.insert(
        Some("terminal_normal"),
        ph(KeyCode::KeyX),
        TERMINAL_LINE_DELETE_CHAR,
    );
    km.insert(
        Some("terminal_normal"),
        ch('X'),
        TERMINAL_LINE_BACKSPACE_CHAR,
    );
    km.insert(Some("terminal_normal"), ch('D'), TERMINAL_LINE_KILL_TO_END);
    km.insert_sequence(
        Some("terminal_normal"),
        seq(&[ph(KeyCode::KeyD), ph(KeyCode::KeyD)]),
        TERMINAL_LINE_KILL_LINE,
    );
    km.insert_sequence(
        Some("terminal_normal"),
        seq(&[ph(KeyCode::KeyD), ph(KeyCode::KeyW)]),
        TERMINAL_LINE_KILL_WORD_FORWARD,
    );
    km.insert_sequence(
        Some("terminal_normal"),
        seq(&[ph(KeyCode::KeyD), ph(KeyCode::KeyB)]),
        TERMINAL_LINE_KILL_WORD_BACKWARD,
    );
    km.insert(Some("terminal_normal"), ph(KeyCode::KeyP), TERMINAL_LINE_YANK);
    km.insert(Some("terminal_normal"), ph(KeyCode::KeyU), TERMINAL_LINE_UNDO);
    km.insert(
        Some("terminal_normal"),
        ph(KeyCode::KeyI),
        TERMINAL_LINE_INSERT_AT_CURSOR,
    );
    km.insert(
        Some("terminal_normal"),
        ph(KeyCode::KeyA),
        TERMINAL_LINE_APPEND_AFTER_CURSOR,
    );
    km.insert(
        Some("terminal_normal"),
        ch('I'),
        TERMINAL_LINE_INSERT_LINE_START,
    );
    km.insert(
        Some("terminal_normal"),
        ch('A'),
        TERMINAL_LINE_APPEND_LINE_END,
    );
    km.insert(
        Some("terminal_normal"),
        ph(KeyCode::KeyS),
        TERMINAL_LINE_SUBSTITUTE_CHAR,
    );
    km.insert_sequence(
        Some("terminal_normal"),
        seq(&[ph(KeyCode::KeyC), ph(KeyCode::KeyC)]),
        TERMINAL_LINE_CHANGE_LINE,
    );
    km.insert_sequence(
        Some("terminal_normal"),
        seq(&[ph(KeyCode::KeyC), ph(KeyCode::KeyW)]),
        TERMINAL_LINE_CHANGE_WORD,
    );
    km.insert(Some("terminal_normal"), ch('C'), TERMINAL_LINE_CHANGE_TO_END);
    km.insert(
        Some("terminal_normal"),
        cmd(KeyCode::KeyT),
        TERMINAL_TAB_NEW,
    );
    km.insert(
        Some("terminal_normal"),
        cmd(KeyCode::KeyW),
        TERMINAL_TAB_CLOSE,
    );
    km.insert(
        Some("terminal_normal"),
        cmd(KeyCode::Digit1),
        TERMINAL_TAB_SWITCH_1,
    );
    km.insert(
        Some("terminal_normal"),
        cmd(KeyCode::Digit2),
        TERMINAL_TAB_SWITCH_2,
    );
    km.insert(
        Some("terminal_normal"),
        cmd(KeyCode::Digit3),
        TERMINAL_TAB_SWITCH_3,
    );
    km.insert(
        Some("terminal_normal"),
        cmd(KeyCode::Digit4),
        TERMINAL_TAB_SWITCH_4,
    );
    km.insert(
        Some("terminal_normal"),
        cmd(KeyCode::Digit5),
        TERMINAL_TAB_SWITCH_5,
    );
    km.insert(
        Some("terminal_normal"),
        cmd(KeyCode::Digit6),
        TERMINAL_TAB_SWITCH_6,
    );
    km.insert(
        Some("terminal_normal"),
        cmd(KeyCode::Digit7),
        TERMINAL_TAB_SWITCH_7,
    );
    km.insert(
        Some("terminal_normal"),
        cmd(KeyCode::Digit8),
        TERMINAL_TAB_SWITCH_8,
    );
    km.insert(
        Some("terminal_normal"),
        cmd(KeyCode::Digit9),
        TERMINAL_TAB_SWITCH_9,
    );
    km.insert(Some("terminal_normal"), ch('/'), TERMINAL_SEARCH_OPEN);
    km.insert(Some("terminal_normal"), ch('n'), SEARCH_NEXT);
    km.insert(Some("terminal_normal"), ch('N'), SEARCH_PREV);
    km.insert(Some("terminal_normal"), ch('*'), SEARCH_WORD_UNDER_CURSOR);

    // ── Resize mode bindings ──────────────────────────────────────────────────
    // Lowercase h/j/k/l resize whichever panel/region is focused.
    km.insert(Some("resize"), ph(KeyCode::KeyH), RESIZE_DECREASE_WIDTH);
    km.insert(Some("resize"), ph(KeyCode::KeyL), RESIZE_INCREASE_WIDTH);
    km.insert(Some("resize"), ph(KeyCode::KeyJ), RESIZE_INCREASE_HEIGHT);
    km.insert(Some("resize"), ph(KeyCode::KeyK), RESIZE_DECREASE_HEIGHT);
    // Uppercase H/L grow the left/right dock (the editor edge they border).
    km.insert(
        Some("resize"),
        KeySpec::ShiftPlus(KeyCode::KeyH),
        RESIZE_GROW_LEFT_DOCK,
    );
    km.insert(
        Some("resize"),
        KeySpec::ShiftPlus(KeyCode::KeyL),
        RESIZE_GROW_RIGHT_DOCK,
    );
    km.insert(Some("resize"), nk(NamedKey::Escape), ENTER_NORMAL);

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
        KeySpec::CtrlPlus(KeyCode::KeyD),
        EXPLORER_HALF_PAGE_DOWN,
    );
    km.insert(
        Some("explorer"),
        KeySpec::CtrlPlus(KeyCode::KeyU),
        EXPLORER_HALF_PAGE_UP,
    );
    km.insert(
        Some("explorer"),
        nk(NamedKey::ArrowLeft),
        EXPLORER_COLLAPSE_NODE,
    );
    km.insert(Some("explorer"), ph(KeyCode::KeyH), EXPLORER_COLLAPSE_NODE);
    km.insert(Some("explorer"), ph(KeyCode::KeyW), EXPLORER_COLLAPSE_NODE);
    km.insert(Some("explorer"), ch('W'), EXPLORER_COLLAPSE_ALL_UNDER_NODE);
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
    km.insert(Some("explorer"), ph(KeyCode::KeyO), EXPLORER_TOGGLE_OR_OPEN);
    km.insert(Some("explorer"), ph(KeyCode::KeyD), EXPLORER_DELETE_NODE);
    km.insert(Some("explorer"), ph(KeyCode::KeyE), EXPLORER_EXPAND_NODE);
    km.insert(Some("explorer"), ch('E'), EXPLORER_EXPAND_ALL_UNDER_NODE);
    km.insert(Some("explorer"), ph(KeyCode::KeyA), EXPLORER_CREATE_FILE);
    km.insert(Some("explorer"), ch('A'), EXPLORER_CREATE_FOLDER);
    km.insert(Some("explorer"), ph(KeyCode::KeyF), EXPLORER_START_FILTER);
    km.insert(Some("explorer"), ch('F'), EXPLORER_CLEAR_FILTER);
    km.insert(Some("explorer"), ch('H'), EXPLORER_TOGGLE_HIDDEN);
    km.insert(Some("explorer"), ch('I'), EXPLORER_TOGGLE_IGNORED);
    km.insert(Some("explorer"), ch('G'), EXPLORER_MOVE_TO_BOTTOM);
    km.insert(Some("explorer"), ph(KeyCode::KeyR), EXPLORER_RENAME_FULL);
    km.insert(Some("explorer"), ch('R'), EXPLORER_RENAME_BASE);
    km.insert(
        Some("explorer"),
        KeySpec::CtrlPlus(KeyCode::KeyR),
        RELOAD_WORKSPACE,
    );
    km.insert(
        Some("explorer"),
        ph(KeyCode::KeyS),
        EXPLORER_TOGGLE_GIT_CHANGES_ONLY,
    );

    // ── Chord bindings (multi-step sequences) ─────────────────────────────────
    km.insert_sequence(
        Some("normal"),
        seq(&[ph(KeyCode::KeyG), ph(KeyCode::KeyC), ph(KeyCode::KeyC)]),
        TOGGLE_LINE_COMMENT,
    );
    km.insert_sequence(
        Some("normal"),
        seq(&[ph(KeyCode::KeyG), ph(KeyCode::KeyG)]),
        MOVE_TO_FIRST_LINE,
    );
    km.insert_sequence(
        Some("explorer"),
        seq(&[ph(KeyCode::KeyG), ph(KeyCode::KeyG)]),
        EXPLORER_MOVE_TO_TOP,
    );
    km.insert_sequence(
        Some("preview"),
        seq(&[ph(KeyCode::KeyG), ph(KeyCode::KeyG)]),
        MARKDOWN_PREVIEW_SCROLL_TOP,
    );
    km.insert_sequence(
        Some("preview"),
        seq(&[KeySpec::Leader, ph(KeyCode::KeyX)]),
        BUFFER_CLOSE_CURRENT,
    );
    km.insert(Some("preview"), ch('G'), MARKDOWN_PREVIEW_SCROLL_BOTTOM);
    // ── Global Zen Mode toggle ─────────────────────────────────────────────
    km.insert_sequence(
        None,
        seq(&[KeySpec::Leader, ph(KeyCode::KeyZ), ph(KeyCode::KeyM)]),
        TOGGLE_MAXIMIZE_FOCUS,
    );
    km.insert_sequence(
        Some("normal"),
        seq(&[ph(KeyCode::KeyZ), ph(KeyCode::KeyZ)]),
        CENTER_CURSOR_LINE,
    );
    km.insert_sequence(
        Some("visual"),
        seq(&[ph(KeyCode::KeyG), ph(KeyCode::KeyG)]),
        MOVE_TO_FIRST_LINE,
    );
    km.insert_sequence(
        Some("visual"),
        seq(&[ph(KeyCode::KeyG), ph(KeyCode::KeyC)]),
        TOGGLE_SELECTION_COMMENT,
    );
    km.insert_sequence(
        Some("visual"),
        seq(&[ph(KeyCode::KeyZ), ph(KeyCode::KeyZ)]),
        CENTER_CURSOR_LINE,
    );
    // Leader bindings (Space = leader) are represented as explicit sequences.
    // Note: <leader>p removed — command palette is opened via mod+p only.
    km.insert_sequence(
        Some("normal"),
        seq(&[KeySpec::Leader, ph(KeyCode::KeyR), ph(KeyCode::KeyN)]),
        LSP_RENAME,
    );
    km.insert_sequence(
        Some("normal"),
        seq(&[KeySpec::Leader, ph(KeyCode::KeyR), ph(KeyCode::KeyR)]),
        ENTER_RESIZE,
    );
    // Resize mode is reachable from either sidebar, the bottom panel, and the
    // terminal too, so docks can be resized while they hold focus.
    km.insert_sequence(
        Some("explorer"),
        seq(&[KeySpec::Leader, ph(KeyCode::KeyR), ph(KeyCode::KeyR)]),
        ENTER_RESIZE,
    );
    km.insert_sequence(
        Some("inspector"),
        seq(&[KeySpec::Leader, ph(KeyCode::KeyR), ph(KeyCode::KeyR)]),
        ENTER_RESIZE,
    );
    km.insert_sequence(
        Some("bottom_panel"),
        seq(&[KeySpec::Leader, ph(KeyCode::KeyR), ph(KeyCode::KeyR)]),
        ENTER_RESIZE,
    );
    km.insert_sequence(
        Some("terminal_normal"),
        seq(&[KeySpec::Leader, ph(KeyCode::KeyR), ph(KeyCode::KeyR)]),
        ENTER_RESIZE,
    );
    km.insert_sequence(
        None,
        seq(&[KeySpec::Leader, ph(KeyCode::KeyI)]),
        FOCUS_INSPECTOR,
    );
    km.insert_sequence(
        None,
        seq(&[KeySpec::Leader, ph(KeyCode::KeyF), ph(KeyCode::KeyF)]),
        OPEN_FILE_PICKER,
    );
    km.insert_sequence(
        None,
        seq(&[KeySpec::Leader, ph(KeyCode::KeyF), ph(KeyCode::KeyW)]),
        SEARCH_IN_FILES,
    );
    km.insert_sequence(
        None,
        seq(&[KeySpec::Leader, ph(KeyCode::KeyF), ph(KeyCode::KeyM)]),
        LSP_FORMAT_DOCUMENT,
    );
    km.insert_sequence(
        Some("normal"),
        seq(&[KeySpec::Leader, ph(KeyCode::KeyC), ph(KeyCode::KeyA)]),
        LSP_CODE_ACTION,
    );
    km.insert_sequence(
        Some("normal"),
        seq(&[KeySpec::Leader, ph(KeyCode::KeyT), ph(KeyCode::KeyH)]),
        OPEN_THEME_SELECTOR,
    );
    km.insert_sequence(
        Some("normal"),
        seq(&[KeySpec::Leader, ph(KeyCode::KeyM), ph(KeyCode::KeyF)]),
        FOCUS_MARKDOWN_PREVIEW,
    );
    km.insert_sequence(
        Some("normal"),
        seq(&[KeySpec::Leader, ph(KeyCode::KeyM), ph(KeyCode::KeyN)]),
        TOGGLE_MINIMAP,
    );
    km.insert_sequence(
        Some("normal"),
        seq(&[KeySpec::Leader, ph(KeyCode::KeyG), ph(KeyCode::KeyF)]),
        GIT_OPEN_LAZYGIT,
    );
    km.insert_sequence(
        Some("normal"),
        seq(&[KeySpec::Leader, ph(KeyCode::KeyD), ph(KeyCode::KeyF)]),
        DOCKER_OPEN_LAZYDOCKER,
    );
    km.insert_sequence(
        Some("normal"),
        seq(&[KeySpec::Leader, ph(KeyCode::KeyG), ph(KeyCode::KeyL)]),
        GIT_BLAME_LINE,
    );
    km.insert_sequence(
        Some("normal"),
        seq(&[KeySpec::Leader, ph(KeyCode::KeyX)]),
        BUFFER_CLOSE_CURRENT,
    );

    // Leap / EasyMotion navigation (Space + s → LeapStart)
    km.insert_sequence(
        Some("normal"),
        seq(&[KeySpec::Leader, ph(KeyCode::KeyS)]),
        LEAP_START,
    );
    km.insert_sequence(
        Some("visual"),
        seq(&[KeySpec::Leader, ph(KeyCode::KeyS)]),
        LEAP_START,
    );

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

pub fn resolve_global_command(
    keymap: &ResolvedKeymap,
    input: &NormalizedInput,
    open_file_path: &std::path::Path,
) -> Option<Command> {
    let id = keymap.lookup_global(input)?;
    command_ids::parse(id, Some(open_file_path))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::keymap_config::KeymapFile;

    #[test]
    fn default_keymap_has_no_duplicate_bindings_in_the_same_scope() {
        let keymap: KeymapFile = toml::from_str(include_str!("../../config/keymaps/default.toml"))
            .expect("default keymap should parse");
        let mut seen = HashMap::new();

        for binding in keymap.bindings {
            let sequence = parse_key_sequence(&binding.key)
                .unwrap_or_else(|| panic!("default keymap has invalid key: {}", binding.key));
            let scope = binding.mode.map(|mode| mode.to_ascii_lowercase());
            let identity = (scope.clone(), sequence);

            if let Some(previous) = seen.insert(identity, binding.command.clone()) {
                panic!(
                    "default keymap duplicates key {:?} in scope {:?}: {previous} and {}",
                    binding.key, scope, binding.command
                );
            }
        }
    }

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
        assert_eq!(
            parse_key_spec("esc"),
            Some(KeySpec::Named(NamedKey::Escape))
        );
        assert_eq!(parse_key_spec("Home"), Some(KeySpec::Named(NamedKey::Home)));
        assert_eq!(parse_key_spec("End"), Some(KeySpec::Named(NamedKey::End)));
        assert_eq!(
            parse_key_spec("Delete"),
            Some(KeySpec::Named(NamedKey::Delete))
        );
    }

    #[test]
    fn parse_physical_keys() {
        assert_eq!(parse_key_spec("h"), Some(KeySpec::Physical(KeyCode::KeyH)));
        assert_eq!(parse_key_spec("j"), Some(KeySpec::Physical(KeyCode::KeyJ)));
        assert_eq!(
            parse_key_spec("backslash"),
            Some(KeySpec::Physical(KeyCode::Backslash))
        );
        assert_eq!(
            parse_key_spec("backtick"),
            Some(KeySpec::Physical(KeyCode::Backquote))
        );
    }

    #[test]
    fn parse_modifier_key() {
        assert_eq!(
            parse_key_spec("cmd+s"),
            Some(KeySpec::CmdPlus(KeyCode::KeyS))
        );
        assert_eq!(
            parse_key_spec("cmd+shift+p"),
            Some(KeySpec::CmdShiftPlus(KeyCode::KeyP))
        );
        assert_eq!(
            parse_key_spec("ctrl+space"),
            Some(KeySpec::CtrlPlus(KeyCode::Space))
        );
        assert_eq!(
            parse_key_spec("ctrl+shift+o"),
            Some(KeySpec::CtrlShiftPlus(KeyCode::KeyO))
        );
    }

    #[test]
    fn parse_leader_key() {
        assert_eq!(
            parse_key_sequence("<leader>f"),
            Some(vec![KeySpec::Leader, KeySpec::Physical(KeyCode::KeyF)])
        );
    }

    #[test]
    fn parse_unknown_key_returns_none() {
        assert!(parse_key_spec("XF86AudioPlay").is_none());
    }

    #[test]
    fn sequence_continuations_lists_exacts_groups_and_mode_priority() {
        let mut km = ResolvedKeymap::new();
        // <leader>f -> file picker (global)
        km.insert_sequence(
            None,
            vec![KeySpec::Leader, KeySpec::Physical(KeyCode::KeyF)],
            "app.open_file_picker",
        );
        // <leader>g d -> deeper chord => "g" is a group after <leader>
        km.insert_sequence(
            None,
            vec![
                KeySpec::Leader,
                KeySpec::Physical(KeyCode::KeyG),
                KeySpec::Physical(KeyCode::KeyD),
            ],
            "lsp.goto_definition",
        );
        // mode-specific override of <leader>f in visual mode
        km.insert_sequence(
            Some("visual"),
            vec![KeySpec::Leader, KeySpec::Physical(KeyCode::KeyF)],
            "editor.format_selection",
        );

        let prefix = vec![KeySpec::Leader];
        let normal = km.sequence_continuations(&prefix, "normal");
        let f = normal.iter().find(|c| c.key == "f").expect("f present");
        assert_eq!(f.command_id.as_deref(), Some("app.open_file_picker"));
        let g = normal.iter().find(|c| c.key == "g").expect("g present");
        assert_eq!(g.command_id, None, "deeper chord shows as group");

        let visual = km.sequence_continuations(&prefix, "visual");
        let f = visual.iter().find(|c| c.key == "f").expect("f present");
        assert_eq!(
            f.command_id.as_deref(),
            Some("editor.format_selection"),
            "mode-specific exact wins over global"
        );

        // Non-matching prefix yields nothing.
        let none = km.sequence_continuations(&[KeySpec::Physical(KeyCode::KeyZ)], "normal");
        assert!(none.is_empty());
    }

    #[test]
    fn parse_character_key() {
        assert_eq!(parse_key_spec(":"), Some(KeySpec::Char(':')));
        // Uppercase letters are parsed as ShiftPlus, not Char
        assert_eq!(parse_key_spec("O"), Some(KeySpec::ShiftPlus(KeyCode::KeyO)));
    }

    #[test]
    fn parse_chord_sequence_key() {
        assert_eq!(
            parse_key_sequence("d d"),
            Some(vec![
                KeySpec::Physical(KeyCode::KeyD),
                KeySpec::Physical(KeyCode::KeyD)
            ])
        );
        assert_eq!(
            parse_key_sequence("<leader>f w"),
            Some(vec![
                KeySpec::Leader,
                KeySpec::Physical(KeyCode::KeyF),
                KeySpec::Physical(KeyCode::KeyW)
            ])
        );
    }

    #[test]
    fn parse_ctrl_as_ctrl_plus() {
        assert_eq!(
            parse_key_spec("Ctrl+e"),
            Some(KeySpec::CtrlPlus(KeyCode::KeyE))
        );
        assert_eq!(
            parse_key_spec("ctrl+o"),
            Some(KeySpec::CtrlPlus(KeyCode::KeyO))
        );
        // ctrl+shift+ maps to CtrlShiftPlus
        assert_eq!(
            parse_key_spec("Ctrl+Shift+p"),
            Some(KeySpec::CtrlShiftPlus(KeyCode::KeyP))
        );
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

    #[test]
    fn builtin_defaults_has_visual_delete_and_change_bindings() {
        use winit::keyboard::ModifiersState;
        let km = builtin_defaults();

        let visual_d = NormalizedInput {
            physical_key: Some(KeyCode::KeyD),
            named_key: None,
            text: Some("d".into()),
            modifiers: ModifiersState::empty(),
        };
        let visual_c = NormalizedInput {
            physical_key: Some(KeyCode::KeyC),
            named_key: None,
            text: Some("c".into()),
            modifiers: ModifiersState::empty(),
        };
        let visual_x = NormalizedInput {
            physical_key: Some(KeyCode::KeyX),
            named_key: None,
            text: Some("x".into()),
            modifiers: ModifiersState::empty(),
        };
        let visual_y = NormalizedInput {
            physical_key: Some(KeyCode::KeyY),
            named_key: None,
            text: Some("y".into()),
            modifiers: ModifiersState::empty(),
        };

        assert_eq!(
            km.lookup(&visual_d, "visual"),
            Some(command_ids::DELETE_SELECTION)
        );
        assert_eq!(
            km.lookup(&visual_c, "visual"),
            Some(command_ids::CHANGE_SELECTION)
        );
        assert_eq!(
            km.lookup(&visual_x, "visual"),
            Some(command_ids::DELETE_SELECTION)
        );
        assert_eq!(
            km.lookup(&visual_y, "visual"),
            Some(command_ids::YANK_SELECTION)
        );
    }

    #[test]
    fn builtin_defaults_include_normal_paste_bindings() {
        use winit::keyboard::ModifiersState;
        let km = builtin_defaults();

        let normal_p = NormalizedInput {
            physical_key: Some(KeyCode::KeyP),
            named_key: None,
            text: Some("p".into()),
            modifiers: ModifiersState::empty(),
        };
        let normal_shift_p = NormalizedInput {
            physical_key: Some(KeyCode::KeyP),
            named_key: None,
            text: Some("P".into()),
            modifiers: ModifiersState::SHIFT,
        };

        assert_eq!(
            km.lookup(&normal_p, "normal"),
            Some(command_ids::PASTE_AFTER)
        );
        assert_eq!(
            km.lookup(&normal_shift_p, "normal"),
            Some(command_ids::PASTE_BEFORE)
        );
    }

    #[test]
    fn builtin_defaults_map_cmd_v_to_editor_paste() {
        use winit::keyboard::ModifiersState;
        let km = builtin_defaults();
        let cmd_v = NormalizedInput {
            physical_key: Some(KeyCode::KeyV),
            named_key: None,
            text: Some("v".into()),
            modifiers: ModifiersState::SUPER,
        };

        assert_eq!(km.lookup(&cmd_v, "insert"), Some(command_ids::EDITOR_PASTE));
        // In normal mode, cmd+v is not bound (ctrl+v is visual block)
        assert_eq!(km.lookup(&cmd_v, "normal"), None);
        assert_eq!(km.lookup(&cmd_v, "visual"), Some(command_ids::EDITOR_PASTE));
        assert_eq!(
            km.lookup_mode_only(&cmd_v, "palette"),
            Some(command_ids::EDITOR_PASTE)
        );
        assert_eq!(
            km.lookup_mode_only(&cmd_v, "terminal"),
            Some(command_ids::TERMINAL_PASTE)
        );
        assert_eq!(
            km.lookup_mode_only(&cmd_v, "terminal_normal"),
            Some(command_ids::TERMINAL_PASTE)
        );
    }

    #[test]
    fn builtin_defaults_include_expected_static_chords() {
        use winit::keyboard::ModifiersState;
        let km = builtin_defaults();

        let g_upper = NormalizedInput {
            physical_key: Some(KeyCode::KeyG),
            named_key: None,
            text: Some("G".to_string()),
            modifiers: ModifiersState::SHIFT,
        };
        assert_eq!(
            km.lookup_mode_only(&g_upper, "normal"),
            Some(command_ids::MOVE_TO_LAST_LINE)
        );

        let ctrl_d = NormalizedInput {
            physical_key: Some(KeyCode::KeyD),
            named_key: None,
            text: Some("d".to_string()),
            modifiers: ModifiersState::CONTROL,
        };
        assert_eq!(
            km.lookup_mode_only(&ctrl_d, "normal"),
            Some(command_ids::SCROLL_HALF_PAGE_DOWN)
        );

        let leader_steps = vec![
            KeySpec::Leader,
            KeySpec::Physical(KeyCode::KeyF),
            KeySpec::Physical(KeyCode::KeyF),
        ];
        let search = km.lookup_sequence(&leader_steps, "normal");
        assert_eq!(search, SequenceLookup::Exact(command_ids::OPEN_FILE_PICKER));

        let gg_steps = vec![
            KeySpec::Physical(KeyCode::KeyG),
            KeySpec::Physical(KeyCode::KeyG),
        ];
        let gg = km.lookup_sequence(&gg_steps, "normal");
        assert_eq!(gg, SequenceLookup::Exact(command_ids::MOVE_TO_FIRST_LINE));

        let zz_steps = vec![
            KeySpec::Physical(KeyCode::KeyZ),
            KeySpec::Physical(KeyCode::KeyZ),
        ];
        let zz = km.lookup_sequence(&zz_steps, "normal");
        assert_eq!(zz, SequenceLookup::Exact(command_ids::CENTER_CURSOR_LINE));

        let candidates = sequence_step_candidates(
            &NormalizedInput {
                physical_key: Some(KeyCode::Space),
                named_key: Some(NamedKey::Space),
                text: None,
                modifiers: ModifiersState::empty(),
            },
            true,
        );
        assert!(candidates.contains(&KeySpec::Leader));
    }

    #[test]
    fn builtin_defaults_include_comment_chords() {
        let km = builtin_defaults();

        let gcc_steps = vec![
            KeySpec::Physical(KeyCode::KeyG),
            KeySpec::Physical(KeyCode::KeyC),
            KeySpec::Physical(KeyCode::KeyC),
        ];
        assert_eq!(
            km.lookup_sequence(&gcc_steps, "normal"),
            SequenceLookup::Exact(command_ids::TOGGLE_LINE_COMMENT)
        );

        let gc_steps = vec![
            KeySpec::Physical(KeyCode::KeyG),
            KeySpec::Physical(KeyCode::KeyC),
        ];
        assert_eq!(
            km.lookup_sequence(&gc_steps, "visual"),
            SequenceLookup::Exact(command_ids::TOGGLE_SELECTION_COMMENT)
        );
        assert_eq!(
            km.lookup_sequence(&gc_steps, "normal"),
            SequenceLookup::Prefix
        );
    }

    #[test]
    fn builtin_defaults_map_cmd_b_to_toggle_left_dock() {
        use winit::keyboard::ModifiersState;
        let km = builtin_defaults();
        let input = NormalizedInput {
            physical_key: Some(KeyCode::KeyB),
            named_key: None,
            text: Some("b".into()),
            modifiers: ModifiersState::SUPER,
        };

        assert_eq!(
            km.lookup(&input, "normal"),
            Some(command_ids::TOGGLE_LEFT_DOCK)
        );
    }

    #[test]
    fn builtin_defaults_map_cmd_backslash_to_toggle_bottom_dock() {
        use winit::keyboard::ModifiersState;
        let km = builtin_defaults();
        let input = NormalizedInput {
            physical_key: Some(KeyCode::Backslash),
            named_key: None,
            text: Some("\\".into()),
            modifiers: ModifiersState::SUPER,
        };

        assert_eq!(
            km.lookup(&input, "normal"),
            Some(command_ids::TOGGLE_BOTTOM_DOCK)
        );
    }

    #[test]
    fn builtin_defaults_do_not_map_mod_e_globally() {
        use winit::keyboard::ModifiersState;
        let km = builtin_defaults();
        let input = NormalizedInput {
            physical_key: Some(KeyCode::KeyE),
            named_key: None,
            text: Some("e".into()),
            modifiers: ModifiersState::CONTROL,
        };

        assert_eq!(km.lookup(&input, "normal"), None);
    }

    #[test]
    fn builtin_defaults_include_explorer_nerdtree_shortcuts() {
        use winit::keyboard::ModifiersState;
        let km = builtin_defaults();

        let create_file = NormalizedInput {
            physical_key: Some(KeyCode::KeyA),
            named_key: None,
            text: Some("a".into()),
            modifiers: ModifiersState::empty(),
        };
        let create_folder = NormalizedInput {
            physical_key: Some(KeyCode::KeyA),
            named_key: None,
            text: Some("A".into()),
            modifiers: ModifiersState::SHIFT,
        };
        let open_entry = NormalizedInput {
            physical_key: Some(KeyCode::KeyO),
            named_key: None,
            text: Some("o".into()),
            modifiers: ModifiersState::empty(),
        };
        let expand_entry = NormalizedInput {
            physical_key: Some(KeyCode::KeyE),
            named_key: None,
            text: Some("e".into()),
            modifiers: ModifiersState::empty(),
        };
        let expand_subtree_entry = NormalizedInput {
            physical_key: Some(KeyCode::KeyE),
            named_key: None,
            text: Some("E".into()),
            modifiers: ModifiersState::SHIFT,
        };

        assert_eq!(
            km.lookup(&create_file, "explorer"),
            Some(command_ids::EXPLORER_CREATE_FILE)
        );
        assert_eq!(
            km.lookup(&create_folder, "explorer"),
            Some(command_ids::EXPLORER_CREATE_FOLDER)
        );
        assert_eq!(
            km.lookup(&open_entry, "explorer"),
            Some(command_ids::EXPLORER_TOGGLE_OR_OPEN)
        );
        assert_eq!(
            km.lookup(&expand_entry, "explorer"),
            Some(command_ids::EXPLORER_EXPAND_NODE)
        );
        assert_eq!(
            km.lookup(&expand_subtree_entry, "explorer"),
            Some(command_ids::EXPLORER_EXPAND_ALL_UNDER_NODE)
        );
    }
}
