use std::path::PathBuf;
use std::time::SystemTime;

use crate::render::renderer::{
    WelcomeAccent, WelcomeLang, WelcomeQuickAction, WelcomeRecentProject, WelcomeScreenKind,
    WelcomeScreenModel, WelcomeSection, WelcomeShortcut,
};

pub(super) fn welcome_screen_model(version: &str, recent: &[PathBuf]) -> WelcomeScreenModel {
    WelcomeScreenModel {
        kind: WelcomeScreenKind::Welcome,
        title: "N E T H E R I Z E".to_string(),
        subtitle: "Zero-latency keyboard-first editor".to_string(),
        meta: "Author: ngthminhdev".to_string(),
        meta_version: format!("v{version}"),
        legend: vec![],
        sections: vec![],
        footer_left: "START".to_string(),
        footer_right: "WORKFLOW".to_string(),
        quick_actions: vec![
            WelcomeQuickAction::new("Open Project / File", "Cmd + O"),
            WelcomeQuickAction::new("Recent Projects", "j / k"),
            WelcomeQuickAction::new("Find File", "Space f f"),
            WelcomeQuickAction::new("Command Palette", "Space /"),
        ],
        recent_projects: build_recent_projects(recent),
        sidebar_shortcuts: vec![
            WelcomeShortcut::new(&["Cmd", "O"], "Open Project / File").important(),
            WelcomeShortcut::new(&["j", "k"], "Recent Projects").important(),
            WelcomeShortcut::new(&["Space", "f", "f"], "Find File").important(),
            WelcomeShortcut::new(&["Space", "g", "g"], "Open Git Panel").important(),
            WelcomeShortcut::new(&["F12"], "Toggle Terminal"),
            WelcomeShortcut::new(&["Space", "/"], "Command Palette"),
        ],
        shortcuts_hint: "press ? or :help for cheat sheet".to_string(),
    }
}

pub(super) fn cheat_sheet_screen_model(version: &str) -> WelcomeScreenModel {
    WelcomeScreenModel {
        kind: WelcomeScreenKind::CheatSheet,
        title: "Netherize Cheat Sheet".to_string(),
        subtitle: "Keymap reference - nvim-ultimate profile".to_string(),
        meta: format!("config v{version} | by ngthminhdev"),
        meta_version: String::new(),
        legend: vec![
            WelcomeShortcut::new(&["spc"], "leader"),
            WelcomeShortcut::new(&["mod"], "Cmd on macOS"),
            WelcomeShortcut::new(&["Ctrl"], "control"),
            WelcomeShortcut::new(&["K"], "press keys in sequence"),
        ],
        sections: vec![
            WelcomeSection::new(
                "Global",
                "any mode",
                WelcomeAccent::Global,
                vec![
                    WelcomeShortcut::new(&["Cmd", "O"], "Open folder / project").important(),
                    WelcomeShortcut::new(&["Cmd", "P"], "Command palette").important(),
                    WelcomeShortcut::new(&["Cmd", "S"], "Save file").important(),
                    WelcomeShortcut::new(&["F12"], "Focus terminal").important(),
                    WelcomeShortcut::new(&["F10"], "Compile and run"),
                    WelcomeShortcut::new(&["Ctrl", "=", "-", "0"], "Scale up / down / reset"),
                ],
            ),
            WelcomeSection::new(
                "Insert",
                "mode = insert",
                WelcomeAccent::Insert,
                vec![
                    WelcomeShortcut::new(&["Esc"], "Normal mode").important(),
                    WelcomeShortcut::new(&["Ctrl", "Space"], "Trigger completion").important(),
                    WelcomeShortcut::new(&["Cmd", "Space"], "Trigger completion alt"),
                    WelcomeShortcut::new(&["Cmd", "V"], "Paste"),
                    WelcomeShortcut::new(&["Ctrl", "J"], "Move line down"),
                    WelcomeShortcut::new(&["Ctrl", "K"], "Move line up"),
                ],
            ),
            WelcomeSection::new(
                "Palette",
                "mode = palette",
                WelcomeAccent::Palette,
                vec![
                    WelcomeShortcut::new(&["Ctrl", "P"], "Select previous"),
                    WelcomeShortcut::new(&["Ctrl", "N"], "Select next"),
                    WelcomeShortcut::new(&["Up"], "Select previous"),
                    WelcomeShortcut::new(&["Down"], "Select next"),
                    WelcomeShortcut::new(&["Enter"], "Confirm selection").important(),
                    WelcomeShortcut::new(&["Esc"], "Close palette"),
                ],
            ),
            WelcomeSection::new(
                "Normal motion",
                "mode = normal",
                WelcomeAccent::Normal,
                vec![
                    WelcomeShortcut::new(&["H", "J", "K", "L"], "Move cursor").important(),
                    WelcomeShortcut::new(&["W", "B", "E"], "Word motions"),
                    WelcomeShortcut::new(&["0", "$", "^"], "Line motions"),
                    WelcomeShortcut::new(&["G", "G"], "First line"),
                    WelcomeShortcut::new(&["G"], "Last line"),
                    WelcomeShortcut::new(&["Ctrl", "U", "/", "D"], "Half page up / down"),
                    WelcomeShortcut::new(&["Z", "Z"], "Center cursor line"),
                ],
            ),
            WelcomeSection::new(
                "Normal edit",
                "mode = normal",
                WelcomeAccent::Normal,
                vec![
                    WelcomeShortcut::new(&["I"], "Insert mode").important(),
                    WelcomeShortcut::new(&["A"], "Append at line end"),
                    WelcomeShortcut::new(&["O"], "New line below"),
                    WelcomeShortcut::new(&["D", "D"], "Delete line").important(),
                    WelcomeShortcut::new(&["D", "W"], "Delete word forward"),
                    WelcomeShortcut::new(&["U"], "Undo").important(),
                ],
            ),
            WelcomeSection::new(
                "Normal search",
                "mode = normal",
                WelcomeAccent::Normal,
                vec![
                    WelcomeShortcut::new(&["/"], "Search in file").important(),
                    WelcomeShortcut::new(&["N"], "Next match"),
                    WelcomeShortcut::new(&["Shift", "N"], "Previous match"),
                    WelcomeShortcut::new(&["*"], "Search word under cursor"),
                    WelcomeShortcut::new(&["Ctrl", "H", "/", "L"], "Prev / next buffer"),
                    WelcomeShortcut::new(&["Cmd", "H", "/", "L"], "Prev / next panel tab"),
                ],
            ),
            WelcomeSection::new(
                "Visual",
                "mode = visual",
                WelcomeAccent::Visual,
                vec![
                    WelcomeShortcut::new(&["H", "J", "K", "L"], "Move selection"),
                    WelcomeShortcut::new(&["W", "B", "E"], "Word motions"),
                    WelcomeShortcut::new(&["D"], "Delete selection").important(),
                    WelcomeShortcut::new(&["C"], "Change selection").important(),
                    WelcomeShortcut::new(&["Y"], "Yank selection").important(),
                    WelcomeShortcut::new(&["Esc"], "Normal mode"),
                ],
            ),
            WelcomeSection::new(
                "Leader bindings",
                "leader = Space",
                WelcomeAccent::Leader,
                vec![
                    WelcomeShortcut::new(&["spc", "C", "H"], "Open cheat sheet").important(),
                    WelcomeShortcut::new(&["spc", "X"], "Close current buffer").important(),
                    WelcomeShortcut::new(&["spc", "P", "J"], "Recent projects").important(),
                    WelcomeShortcut::new(&["spc", "F", "F"], "File picker").important(),
                    WelcomeShortcut::new(&["spc", "F", "W"], "Find word / grep").important(),
                    WelcomeShortcut::new(&["spc", "G", "F"], "Open lazygit"),
                ],
            ),
            WelcomeSection::new(
                "LSP",
                "normal / insert",
                WelcomeAccent::Lsp,
                vec![
                    WelcomeShortcut::new(&["K"], "Hover docs").important(),
                    WelcomeShortcut::new(&["G", "D"], "Go to definition").important(),
                    WelcomeShortcut::new(&["G", "Shift", "D"], "Preview definition"),
                    WelcomeShortcut::new(&["G", "R"], "References"),
                    WelcomeShortcut::new(&["Ctrl", "Space"], "Trigger completion"),
                ],
            ),
            WelcomeSection::new(
                "Explorer",
                "mode = explorer",
                WelcomeAccent::Explorer,
                vec![
                    WelcomeShortcut::new(&["J", "K"], "Down / up").important(),
                    WelcomeShortcut::new(&["H", "L"], "Collapse / expand").important(),
                    WelcomeShortcut::new(&["O"], "Toggle / open file").important(),
                    WelcomeShortcut::new(&["E"], "Expand node"),
                    WelcomeShortcut::new(&["A"], "Create file"),
                    WelcomeShortcut::new(&["F"], "Filter tree"),
                ],
            ),
            WelcomeSection::new(
                "Terminal",
                "terminal / normal",
                WelcomeAccent::Terminal,
                vec![
                    WelcomeShortcut::new(&["Esc"], "Focus editor").important(),
                    WelcomeShortcut::new(&["Ctrl", "Q"], "Enter terminal normal").important(),
                    WelcomeShortcut::new(&["Cmd", "V"], "Paste into PTY"),
                    WelcomeShortcut::new(&["H", "J", "K", "L"], "Move output cursor"),
                    WelcomeShortcut::new(&["V"], "Select output"),
                    WelcomeShortcut::new(&["Y"], "Yank selection"),
                ],
            ),
        ],
        footer_left: "profile: nvim-ultimate | config: ~/.config/netherize/config.toml".to_string(),
        footer_right: "q or Space x closes this tab".to_string(),
        quick_actions: vec![],
        recent_projects: vec![],
        sidebar_shortcuts: vec![],
        shortcuts_hint: String::new(),
    }
}

fn build_recent_projects(paths: &[PathBuf]) -> Vec<WelcomeRecentProject> {
    paths
        .iter()
        .take(5)
        .enumerate()
        .filter_map(|(i, path)| {
            let name = path.file_name()?.to_str()?.to_string();
            let display = shellify_path(path);
            let lang = detect_lang(path);
            let ago = format_ago(path);
            let proj = WelcomeRecentProject::new(&name, &display, lang, &ago);
            Some(if i == 0 { proj.active() } else { proj })
        })
        .collect()
}

fn detect_lang(path: &std::path::Path) -> WelcomeLang {
    if path.join("Cargo.toml").exists() {
        WelcomeLang::Rust
    } else if path.join("package.json").exists() || path.join("tsconfig.json").exists() {
        WelcomeLang::TypeScript
    } else if path.join("pyproject.toml").exists() || path.join("setup.py").exists() {
        WelcomeLang::Other("PY".to_string())
    } else {
        WelcomeLang::Other("DIR".to_string())
    }
}

fn shellify_path(path: &std::path::Path) -> String {
    let home = std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(std::path::PathBuf::from);
    if let Some(home) = home {
        if let Ok(rel) = path.strip_prefix(&home) {
            return format!("~/{}", rel.display());
        }
    }
    path.display().to_string()
}

fn format_ago(path: &std::path::Path) -> String {
    let modified = std::fs::metadata(path).and_then(|m| m.modified()).ok();
    let Some(modified) = modified else {
        return "recently".to_string();
    };
    let elapsed = SystemTime::now()
        .duration_since(modified)
        .unwrap_or_default();
    let secs = elapsed.as_secs();
    if secs < 120 {
        "just now".to_string()
    } else if secs < 3600 {
        format!("{}m ago", secs / 60)
    } else if secs < 86_400 {
        format!("{}h ago", secs / 3600)
    } else if secs < 86_400 * 2 {
        "yesterday".to_string()
    } else if secs < 86_400 * 7 {
        format!("{}d ago", secs / 86_400)
    } else {
        format!("{}w ago", secs / (86_400 * 7))
    }
}

#[cfg(test)]
mod tests {
    use super::{cheat_sheet_screen_model, welcome_screen_model};
    use crate::render::renderer::WelcomeScreenKind;

    #[test]
    fn welcome_model_is_compact_empty_state() {
        let model = welcome_screen_model("1.0.0", &[]);

        assert_eq!(model.kind, WelcomeScreenKind::Welcome);
        assert_eq!(model.title, "N E T H E R I Z E");
        assert!(model.meta_version.contains("1.0.0"));
        assert_eq!(model.quick_actions.len(), 4);
        assert!(
            model
                .quick_actions
                .iter()
                .any(|a| a.label == "Open Project / File")
        );
        assert!(model.quick_actions.iter().any(|a| a.label == "Find File"));
        assert!(
            model
                .sidebar_shortcuts
                .iter()
                .any(|s| s.label == "Open Project / File")
        );
        assert!(
            model
                .sidebar_shortcuts
                .iter()
                .any(|s| s.label == "Find File")
        );
        assert!(model.recent_projects.is_empty());
    }

    #[test]
    fn cheat_sheet_model_surfaces_core_startup_actions() {
        let model = cheat_sheet_screen_model("1.0.0");
        let labels = model
            .sections
            .iter()
            .flat_map(|section| section.shortcuts.iter())
            .map(|shortcut| shortcut.label.as_str())
            .collect::<Vec<_>>();

        assert_eq!(model.kind, WelcomeScreenKind::CheatSheet);
        assert!(model.sections.len() >= 10);
        assert!(model.meta.contains("1.0.0"));
        assert!(labels.contains(&"Open folder / project"));
        assert!(labels.contains(&"Command palette"));
        assert!(labels.contains(&"Focus terminal"));
        assert!(labels.contains(&"File picker"));
        assert!(labels.contains(&"Enter terminal normal"));
        assert!(labels.contains(&"Close current buffer"));
    }
}
