use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
};

use super::{
    model::{
        EditorThemeTokens, FileIconThemeTokens, IconThemeTokens, SyntaxThemeTokens, ThemeColor,
        ThemeConfig, UiThemeTokens,
    },
    raw::{RawEditor, RawFileIconTheme, RawFileIcons, RawIcons, RawSyntax, RawThemeFile, RawUi},
};
use crate::config::paths::{legacy_app_state_root, user_config_root};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ThemeProfileEntry {
    pub profile: String,
    pub path: PathBuf,
}

impl ThemeConfig {
    pub fn default_profile() -> &'static str {
        "default-dark"
    }

    pub fn resolved_profile(persisted_profile: Option<&str>) -> String {
        std::env::var("NETHERIZE_THEME")
            .ok()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
            .or_else(|| {
                persisted_profile
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .map(str::to_string)
            })
            .unwrap_or_else(|| Self::default_profile().to_string())
    }

    pub fn active_profile() -> String {
        Self::resolved_profile(None)
    }

    pub fn list_available_themes() -> Vec<String> {
        Self::list_available_theme_entries()
            .into_iter()
            .map(|entry| entry.profile)
            .collect()
    }

    pub fn list_available_theme_entries() -> Vec<ThemeProfileEntry> {
        let mut seen = HashSet::new();
        let mut themes = Vec::new();

        for entry in theme_search_dirs()
            .into_iter()
            .flat_map(|dir| list_available_theme_entries_in_dir(&dir))
        {
            let dedupe_key = entry.profile.to_ascii_lowercase();
            if seen.insert(dedupe_key) {
                themes.push(entry);
            }
        }

        themes.sort_by_cached_key(|entry| entry.profile.to_ascii_lowercase());
        themes
    }

    pub fn load_active() -> Self {
        Self::load_preferred(None)
    }

    pub fn load_preferred(persisted_profile: Option<&str>) -> Self {
        let profile = Self::resolved_profile(persisted_profile);
        match Self::load(&profile) {
            Ok(theme) => {
                eprintln!("[theme] loaded profile '{profile}'");
                theme
            }
            Err(err) => {
                eprintln!("[theme] {err}");
                eprintln!("[theme] falling back to built-in dark theme");
                Self::builtin_dark()
            }
        }
    }

    pub fn load(profile: &str) -> Result<Self, String> {
        let path = find_profile_path(profile)
            .ok_or_else(|| format!("theme profile '{profile}' not found in theme search paths"))?;
        Self::load_from_path(&path)
    }

    pub fn load_from_path(path: &Path) -> Result<Self, String> {
        let content = std::fs::read_to_string(path)
            .map_err(|err| format!("cannot read theme file {}: {err}", path.display()))?;

        let raw: RawThemeFile = toml::from_str(&content)
            .map_err(|err| format!("parse error in theme file {}: {err}", path.display()))?;

        let mut theme = Self::from_raw(raw)
            .map_err(|err| format!("invalid theme file {}: {err}", path.display()))?;

        if theme.name.is_empty() {
            theme.name = path
                .file_stem()
                .and_then(|name| name.to_str())
                .unwrap_or("theme")
                .to_string();
        }

        Ok(theme)
    }

    fn from_raw(raw: RawThemeFile) -> Result<Self, String> {
        let RawThemeFile {
            theme,
            editor: raw_editor,
            ui: raw_ui,
            syntax: raw_syntax,
            icons: raw_icons,
            file_icons: raw_file_icons,
        } = raw;

        let editor = parse_editor(&raw_editor)?;
        let ui = parse_ui(&raw_ui, &raw_editor)?;
        let syntax = parse_syntax(&raw_syntax)?;
        let icons = parse_icons(&raw_icons, &ui)?;

        Ok(Self {
            name: theme.name,
            description: theme.description,
            editor,
            ui,
            syntax,
            icons,
            exact_icons: parse_exact_file_icons(&raw_file_icons),
            extension_icons: parse_extension_file_icons(&raw_file_icons),
            default_file_icon: raw_file_icons
                .default_file
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .unwrap_or("📄")
                .to_string(),
            default_folder_icon: raw_file_icons
                .default_folder
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .unwrap_or("📁")
                .to_string(),
        })
    }
}

fn parse_extension_file_icons(raw: &RawFileIcons) -> HashMap<String, String> {
    raw.extensions
        .as_ref()
        .map(|map| {
            map.iter()
                .filter_map(|(ext, icon)| {
                    let ext = ext.trim().trim_start_matches('.').to_ascii_lowercase();
                    let icon = icon.trim();
                    if ext.is_empty() || icon.is_empty() {
                        None
                    } else {
                        Some((ext, icon.to_string()))
                    }
                })
                .collect()
        })
        .unwrap_or_default()
}

fn parse_exact_file_icons(raw: &RawFileIcons) -> HashMap<String, String> {
    raw.exact
        .as_ref()
        .map(|map| {
            map.iter()
                .filter_map(|(name, icon)| {
                    let name = name.trim();
                    let icon = icon.trim();
                    if name.is_empty() || icon.is_empty() {
                        None
                    } else {
                        Some((name.to_string(), icon.to_string()))
                    }
                })
                .collect()
        })
        .unwrap_or_default()
}

fn parse_editor(raw: &RawEditor) -> Result<EditorThemeTokens, String> {
    let rainbow_brackets = if let Some(colors) = raw.rainbow_brackets.as_ref() {
        let parsed = colors
            .iter()
            .enumerate()
            .map(|(idx, value)| parse_color("editor", &format!("rainbow_brackets[{idx}]"), value))
            .collect::<Result<Vec<_>, _>>()?;
        if parsed.is_empty() {
            vec![
                ThemeColor::from_rgba_u8(47, 211, 246, 255),
                ThemeColor::from_rgba_u8(231, 122, 233, 255),
                ThemeColor::from_rgba_u8(245, 182, 58, 255),
                ThemeColor::from_rgba_u8(155, 229, 100, 255),
                ThemeColor::from_rgba_u8(255, 123, 114, 255),
                ThemeColor::from_rgba_u8(183, 138, 255, 255),
            ]
        } else {
            parsed
        }
    } else {
        vec![
            ThemeColor::from_rgba_u8(47, 211, 246, 255),
            ThemeColor::from_rgba_u8(231, 122, 233, 255),
            ThemeColor::from_rgba_u8(245, 182, 58, 255),
            ThemeColor::from_rgba_u8(155, 229, 100, 255),
            ThemeColor::from_rgba_u8(255, 123, 114, 255),
            ThemeColor::from_rgba_u8(183, 138, 255, 255),
        ]
    };

    Ok(EditorThemeTokens {
        bg: parse_color("editor", "bg", &raw.bg)?,
        fg: parse_color("editor", "fg", &raw.fg)?,
        cursor: parse_color("editor", "cursor", &raw.cursor)?,
        selection: parse_color("editor", "selection", &raw.selection)?,
        gutter: parse_color("editor", "gutter", &raw.gutter)?,
        gutter_active: parse_color(
            "editor",
            "gutter_active",
            raw.gutter_active.as_deref().unwrap_or(raw.fg.as_str()),
        )?,
        indent_guide: parse_color(
            "editor",
            "indent_guide",
            raw.indent_guide.as_deref().unwrap_or("#8f98aa38"),
        )?,
        rainbow_brackets,
        font_size: parse_positive_size("editor", "font_size", raw.font_size.unwrap_or(17.0))?,
        line_height: parse_positive_size("editor", "line_height", raw.line_height.unwrap_or(26.0))?,
        font_family: raw
            .font_family
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string),
        nerd_font_family: raw
            .nerd_font_family
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string),
    })
}

fn parse_ui(raw: &RawUi, raw_editor: &RawEditor) -> Result<UiThemeTokens, String> {
    Ok(UiThemeTokens {
        bg: parse_color("ui", "bg", raw.bg.as_deref().unwrap_or("#07080d"))?,
        sidebar_bg: parse_color("ui", "sidebar_bg", &raw.sidebar_bg)?,
        panel_bg: parse_color("ui", "panel_bg", &raw.panel_bg)?,
        terminal_bg: parse_color(
            "ui",
            "terminal_bg",
            raw.terminal_bg.as_deref().unwrap_or(raw.panel_bg.as_str()),
        )?,
        overlay_bg: parse_color(
            "ui",
            "overlay_bg",
            raw.overlay_bg.as_deref().unwrap_or("#05070cD9"),
        )?,
        status_bar_bg: parse_color("ui", "status_bar_bg", &raw.status_bar_bg)?,
        border_color: parse_color("ui", "border_color", &raw.border_color)?,
        selection_bg: parse_color(
            "ui",
            "selection_bg",
            raw.selection_bg.as_deref().unwrap_or("#094771"),
        )?,
        dirty_indicator: parse_color(
            "ui",
            "dirty_indicator",
            raw.dirty_indicator
                .as_deref()
                .unwrap_or(raw.accent.as_deref().unwrap_or("#9BE564")),
        )?,
        fg: parse_color(
            "ui",
            "fg",
            raw.fg.as_deref().unwrap_or(raw_editor.fg.as_str()),
        )?,
        fg_dim: parse_color("ui", "fg_dim", raw.fg_dim.as_deref().unwrap_or("#b7bfcc"))?,
        fg_ghost: parse_color(
            "ui",
            "fg_ghost",
            raw.fg_ghost.as_deref().unwrap_or("#6d7483"),
        )?,
        accent: parse_color("ui", "accent", raw.accent.as_deref().unwrap_or("#9BE564"))?,
        cyan: parse_color("ui", "cyan", raw.cyan.as_deref().unwrap_or("#2FD3F6"))?,
        magenta: parse_color("ui", "magenta", raw.magenta.as_deref().unwrap_or("#E77AE9"))?,
        amber: parse_color("ui", "amber", raw.amber.as_deref().unwrap_or("#F5B63A"))?,
        success: parse_color("ui", "success", raw.success.as_deref().unwrap_or("#67D67C"))?,
        warning: parse_color("ui", "warning", raw.warning.as_deref().unwrap_or("#F2B84B"))?,
        info: parse_color("ui", "info", raw.info.as_deref().unwrap_or("#49C6F8"))?,
        error: parse_color("ui", "error", raw.error.as_deref().unwrap_or("#FF7B72"))?,
        mode_normal: parse_color(
            "ui",
            "mode_normal",
            raw.mode_normal.as_deref().unwrap_or("#2FD3F6"),
        )?,
        mode_insert: parse_color(
            "ui",
            "mode_insert",
            raw.mode_insert.as_deref().unwrap_or("#9BE564"),
        )?,
        mode_visual: parse_color(
            "ui",
            "mode_visual",
            raw.mode_visual.as_deref().unwrap_or("#E77AE9"),
        )?,
        sidebar_font_size: parse_positive_size(
            "ui",
            "sidebar_font_size",
            raw.sidebar_font_size.unwrap_or(14.0),
        )?,
        sidebar_line_height: parse_positive_size(
            "ui",
            "sidebar_line_height",
            raw.sidebar_line_height.unwrap_or(21.0),
        )?,
        panel_font_size: parse_positive_size(
            "ui",
            "panel_font_size",
            raw.panel_font_size.unwrap_or(14.0),
        )?,
        panel_line_height: parse_positive_size(
            "ui",
            "panel_line_height",
            raw.panel_line_height.unwrap_or(21.0),
        )?,
        sidebar_width: parse_positive_size(
            "ui",
            "sidebar_width",
            raw.sidebar_width.unwrap_or(260.0),
        )?,
        right_sidebar_width: parse_positive_size(
            "ui",
            "right_sidebar_width",
            raw.right_sidebar_width.unwrap_or(280.0),
        )?,
        bottom_panel_height: parse_positive_size(
            "ui",
            "bottom_panel_height",
            raw.bottom_panel_height.unwrap_or(220.0),
        )?,
        top_bar_height: parse_positive_size(
            "ui",
            "top_bar_height",
            raw.top_bar_height.unwrap_or(32.0),
        )?,
        status_bar_height: parse_positive_size(
            "ui",
            "status_bar_height",
            raw.status_bar_height.unwrap_or(24.0),
        )?,
    })
}

fn parse_syntax(raw: &RawSyntax) -> Result<SyntaxThemeTokens, String> {
    Ok(SyntaxThemeTokens {
        keyword: parse_color("syntax", "keyword", &raw.keyword)?,
        string: parse_color("syntax", "string", &raw.string)?,
        function: parse_color("syntax", "function", &raw.function)?,
        comment: parse_color("syntax", "comment", &raw.comment)?,
        r#type: parse_color("syntax", "type", &raw.r#type)?,
        number: parse_color("syntax", "number", &raw.number)?,
        boolean: parse_color(
            "syntax",
            "boolean",
            raw.boolean
                .as_deref()
                .or(raw.constant.as_deref())
                .unwrap_or(raw.number.as_str()),
        )?,
        identifier: parse_color(
            "syntax",
            "identifier",
            raw.identifier.as_deref().unwrap_or("#d0d7e4"),
        )?,
        variable: parse_color(
            "syntax",
            "variable",
            raw.variable
                .as_deref()
                .or(raw.identifier.as_deref())
                .unwrap_or("#d0d7e4"),
        )?,
        parameter: parse_color(
            "syntax",
            "parameter",
            raw.parameter
                .as_deref()
                .or(raw.identifier.as_deref())
                .unwrap_or(raw.function.as_str()),
        )?,
        field: parse_color(
            "syntax",
            "field",
            raw.field
                .as_deref()
                .or(raw.property.as_deref())
                .or(raw.identifier.as_deref())
                .unwrap_or(raw.function.as_str()),
        )?,
        property: parse_color(
            "syntax",
            "property",
            raw.property
                .as_deref()
                .or(raw.field.as_deref())
                .or(raw.identifier.as_deref())
                .unwrap_or(raw.function.as_str()),
        )?,
        constant: parse_color(
            "syntax",
            "constant",
            raw.constant.as_deref().unwrap_or(raw.number.as_str()),
        )?,
        operator: parse_color(
            "syntax",
            "operator",
            raw.operator
                .as_deref()
                .or(raw.punctuation.as_deref())
                .unwrap_or("#8f98aa"),
        )?,
        punctuation: parse_color(
            "syntax",
            "punctuation",
            raw.punctuation.as_deref().unwrap_or("#8f98aa"),
        )?,
        escape: parse_color(
            "syntax",
            "escape",
            raw.escape
                .as_deref()
                .or(raw.constant.as_deref())
                .unwrap_or(raw.number.as_str()),
        )?,
        r#macro: parse_color(
            "syntax",
            "macro",
            raw.r#macro.as_deref().unwrap_or(raw.keyword.as_str()),
        )?,
        lifetime: parse_color(
            "syntax",
            "lifetime",
            raw.lifetime.as_deref().unwrap_or(raw.number.as_str()),
        )?,
        constructor: parse_color(
            "syntax",
            "constructor",
            raw.constructor.as_deref().unwrap_or(raw.r#type.as_str()),
        )?,
        attribute: parse_color(
            "syntax",
            "attribute",
            raw.attribute.as_deref().unwrap_or(raw.keyword.as_str()),
        )?,
        namespace: parse_color(
            "syntax",
            "namespace",
            raw.namespace.as_deref().unwrap_or(raw.r#type.as_str()),
        )?,
        tag: parse_color(
            "syntax",
            "tag",
            raw.tag.as_deref().unwrap_or(raw.r#type.as_str()),
        )?,
    })
}

fn parse_icons(raw: &RawIcons, ui: &UiThemeTokens) -> Result<IconThemeTokens, String> {
    Ok(IconThemeTokens {
        explorer_file_marker: parse_string_token(
            "icons",
            "explorer_file_marker",
            raw.explorer_file_marker.as_deref(),
            "·",
        )?,
        explorer_folder_collapsed_marker: parse_string_token(
            "icons",
            "explorer_folder_collapsed_marker",
            raw.explorer_folder_collapsed_marker.as_deref(),
            "▶",
        )?,
        explorer_folder_expanded_marker: parse_string_token(
            "icons",
            "explorer_folder_expanded_marker",
            raw.explorer_folder_expanded_marker.as_deref(),
            "▼",
        )?,
        file_picker_dot: parse_string_token(
            "icons",
            "file_picker_dot",
            raw.file_picker_dot.as_deref(),
            "●",
        )?,
        folder_closed: parse_file_icon(
            "icons.folder_closed",
            raw.folder_closed.as_ref(),
            "\u{F07B}",
            ui.amber,
        )?,
        folder_open: parse_file_icon(
            "icons.folder_open",
            raw.folder_open.as_ref(),
            "\u{F07C}",
            ui.amber,
        )?,
        default_file: parse_file_icon(
            "icons.default_file",
            raw.default_file.as_ref(),
            "\u{F15B}",
            ui.fg_ghost,
        )?,
        rust: parse_file_icon("icons.rust", raw.rust.as_ref(), "\u{E7A8}", ui.warning)?,
        javascript: parse_file_icon(
            "icons.javascript",
            raw.javascript.as_ref(),
            "\u{E781}",
            ui.amber,
        )?,
        typescript: parse_file_icon(
            "icons.typescript",
            raw.typescript.as_ref(),
            "\u{E628}",
            ui.info,
        )?,
        tsx: parse_file_icon("icons.tsx", raw.tsx.as_ref(), "\u{E7BA}", ui.info)?,
        jsx: parse_file_icon("icons.jsx", raw.jsx.as_ref(), "\u{E7BA}", ui.amber)?,
        python: parse_file_icon("icons.python", raw.python.as_ref(), "\u{E73C}", ui.info)?,
        go: parse_file_icon("icons.go", raw.go.as_ref(), "\u{E724}", ui.cyan)?,
        config: parse_file_icon("icons.config", raw.config.as_ref(), "\u{E615}", ui.fg_ghost)?,
        json: parse_file_icon("icons.json", raw.json.as_ref(), "\u{E60B}", ui.amber)?,
        markdown: parse_file_icon("icons.markdown", raw.markdown.as_ref(), "\u{F48A}", ui.info)?,
        html: parse_file_icon("icons.html", raw.html.as_ref(), "\u{E736}", ui.error)?,
        css: parse_file_icon("icons.css", raw.css.as_ref(), "\u{E749}", ui.info)?,
        sass: parse_file_icon("icons.sass", raw.sass.as_ref(), "\u{E603}", ui.magenta)?,
        shell: parse_file_icon("icons.shell", raw.shell.as_ref(), "\u{F489}", ui.success)?,
        git: parse_file_icon("icons.git", raw.git.as_ref(), "\u{F1D3}", ui.warning)?,
        lock: parse_file_icon("icons.lock", raw.lock.as_ref(), "\u{F13E}", ui.fg_ghost)?,
        image: parse_file_icon("icons.image", raw.image.as_ref(), "\u{F1C5}", ui.success)?,
    })
}

fn parse_file_icon(
    section: &str,
    raw: Option<&RawFileIconTheme>,
    default_glyph: &str,
    default_color: ThemeColor,
) -> Result<FileIconThemeTokens, String> {
    Ok(FileIconThemeTokens {
        glyph: parse_string_token(
            section,
            "glyph",
            raw.and_then(|icon| icon.glyph.as_deref()),
            default_glyph,
        )?,
        color: if let Some(color) = raw.and_then(|icon| icon.color.as_deref()) {
            parse_color(section, "color", color)?
        } else {
            default_color
        },
    })
}

fn parse_color(section: &str, token: &str, value: &str) -> Result<ThemeColor, String> {
    ThemeColor::from_hex(value).map_err(|err| format!("{section}.{token}: {err}"))
}

fn parse_positive_size(section: &str, token: &str, value: f32) -> Result<f32, String> {
    if value > 0.0 {
        Ok(value)
    } else {
        Err(format!("{section}.{token}: expected > 0, got {value}"))
    }
}

fn parse_string_token(
    section: &str,
    token: &str,
    value: Option<&str>,
    default: &str,
) -> Result<String, String> {
    let parsed = value.unwrap_or(default).trim();
    if parsed.is_empty() {
        Err(format!("{section}.{token}: expected non-empty string"))
    } else {
        Ok(parsed.to_string())
    }
}

fn find_profile_path(name: &str) -> Option<PathBuf> {
    let filename = format!("{name}.toml");
    theme_search_dirs()
        .into_iter()
        .map(|dir| dir.join(&filename))
        .find(|path| path.exists())
}

fn theme_search_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();

    if let Ok(cwd) = std::env::current_dir() {
        let dir = cwd.join("config").join("themes");
        if dir.is_dir() {
            dirs.push(dir);
        }
    }

    let user_dir = user_config_root().join("themes");
    if user_dir.is_dir() && !dirs.contains(&user_dir) {
        dirs.push(user_dir);
    }

    let legacy_dir = legacy_app_state_root().join("themes");
    if legacy_dir.is_dir() && !dirs.contains(&legacy_dir) {
        dirs.push(legacy_dir);
    }

    if let Ok(exe) = std::env::current_exe()
        && let Some(parent) = exe.parent()
    {
        let dir = parent.join("config").join("themes");
        if dir.is_dir() && !dirs.contains(&dir) {
            dirs.push(dir);
        }
    }

    dirs
}

fn list_available_theme_entries_in_dir(dir: &Path) -> Vec<ThemeProfileEntry> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };

    entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.extension().and_then(|ext| ext.to_str()) == Some("toml"))
        .filter_map(|path| {
            let profile = path.file_stem().and_then(|stem| stem.to_str())?;
            Some(ThemeProfileEntry {
                profile: profile.to_string(),
                path,
            })
        })
        .collect()
}
