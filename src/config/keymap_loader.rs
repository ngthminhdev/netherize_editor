use std::path::{Path, PathBuf};

use crate::{
    config::{
        keymap_config::{KeyBinding, KeymapFile},
        paths::user_config_root,
    },
    core::command_ids,
};

/// Load and layer keymap profiles, returning a flat list of validated bindings.
///
/// Layer order (later layers win):
///   1. Built-in Rust defaults  (always present, cannot be broken by config)
///   2. Selected profile TOML   (config/keymaps/{profile}.toml)
///   3. User overrides          (~/.config/netherize/keymaps/user.toml)
///
/// If any TOML layer has parse errors, a warning is logged and that layer is
/// silently skipped — the app never crashes.
pub struct KeymapLoader;

impl KeymapLoader {
    /// Resolve active profile name: checks `NETHERIZE_PROFILE` env var first,
    /// then falls back to `"default"`.
    pub fn active_profile() -> String {
        std::env::var("NETHERIZE_PROFILE").unwrap_or_else(|_| "default".into())
    }

    /// Load with the profile name resolved via `active_profile()`.
    pub fn load_active(user_overrides: Option<&Path>) -> Vec<KeyBinding> {
        let profile = Self::active_profile();
        Self::load(&profile, user_overrides)
    }

    /// Load a specific profile by name.
    pub fn load(profile: &str, user_overrides: Option<&Path>) -> Vec<KeyBinding> {
        // Layer 1: built-in Rust defaults (always valid, hard-coded).
        // These are defined in app::resolved_keymap::builtin_defaults()
        // and are applied before this list, so we return an empty base here —
        // the caller (ResolvedKeymap) merges them on top of the Rust defaults.
        let mut bindings: Vec<KeyBinding> = Vec::new();

        // Layer 2: selected profile TOML
        let path = find_profile_path(profile).or_else(|| {
            if profile != "default" {
                eprintln!("[keymap] profile '{profile}' not found — trying 'default'");
                find_profile_path("default")
            } else {
                None
            }
        });

        if let Some(p) = path {
            if let Some(mut layer) = load_validated_file(&p) {
                bindings.append(&mut layer);
            }
        }

        // Layer 3: user overrides
        let user_path = user_overrides
            .map(PathBuf::from)
            .or_else(default_user_overrides_path);

        if let Some(p) = user_path {
            if p.exists() {
                if let Some(mut layer) = load_validated_file(&p) {
                    bindings.append(&mut layer);
                }
            }
        }

        bindings
    }
}

// ── Internal helpers ──────────────────────────────────────────────────────────

fn find_profile_path(name: &str) -> Option<PathBuf> {
    let filename = format!("{name}.toml");

    // Search relative to current working directory first (dev workflow).
    if let Ok(cwd) = std::env::current_dir() {
        let p = cwd.join("config").join("keymaps").join(&filename);
        if p.exists() {
            return Some(p);
        }
    }

    // Then relative to the binary (installed / release builds).
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let p = dir.join("config").join("keymaps").join(&filename);
            if p.exists() {
                return Some(p);
            }
        }
    }

    None
}

fn default_user_overrides_path() -> Option<PathBuf> {
    Some(user_config_root().join("keymaps").join("user.toml"))
}

/// Read and parse a TOML keymap file.
/// On any error: log a warning and return None (safe fallback).
fn load_validated_file(path: &Path) -> Option<Vec<KeyBinding>> {
    let content = match std::fs::read_to_string(path) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("[keymap] cannot read {path:?}: {e}");
            return None;
        }
    };

    let file: KeymapFile = match toml::from_str(&content) {
        Ok(f) => f,
        Err(e) => {
            eprintln!("[keymap] parse error in {path:?}: {e}");
            eprintln!("[keymap] skipping this layer — falling back to previous layer");
            return None;
        }
    };

    eprintln!(
        "[keymap] loaded profile '{}' from {path:?}",
        file.profile.name
    );

    // Validate command IDs — skip invalid entries, never crash.
    let validated: Vec<KeyBinding> = file
        .bindings
        .into_iter()
        .filter(|b| {
            if command_ids::is_valid(&b.command) {
                true
            } else {
                eprintln!(
                    "[keymap] unknown command '{}' (key='{}') — skipped",
                    b.command, b.key
                );
                false
            }
        })
        .collect();

    Some(validated)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_keymap_has_no_unknown_commands() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("config/keymaps/default.toml");
        let content = std::fs::read_to_string(&path).expect("read default keymap");
        let file: KeymapFile = toml::from_str(&content).expect("parse default keymap");
        let unknown: Vec<String> = file
            .bindings
            .iter()
            .filter(|b| !command_ids::is_valid(&b.command))
            .map(|b| format!("{} (key='{}')", b.command, b.key))
            .collect();
        assert!(
            unknown.is_empty(),
            "default.toml references unknown command ids (their keys are silently dead): {unknown:?}"
        );
    }

    #[test]
    fn invalid_toml_returns_none() {
        let dir = std::env::temp_dir();
        let path = dir.join("netherize_bad_keymap_test.toml");
        std::fs::write(&path, "this is [not valid toml !!@@").unwrap();
        let result = load_validated_file(&path);
        assert!(result.is_none(), "bad TOML must not crash — returns None");
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn invalid_command_id_is_skipped() {
        let dir = std::env::temp_dir();
        let path = dir.join("netherize_bad_cmd_test.toml");
        std::fs::write(
            &path,
            r#"
[profile]
name = "test"

[[bindings]]
key = "h"
mode = "normal"
command = "not.a.real.command"

[[bindings]]
key = "j"
mode = "normal"
command = "editor.move_down"
"#,
        )
        .unwrap();

        let bindings = load_validated_file(&path).expect("valid TOML should parse");
        assert_eq!(bindings.len(), 1, "only valid command survives");
        assert_eq!(bindings[0].command, "editor.move_down");
        let _ = std::fs::remove_file(&path);
    }
}
