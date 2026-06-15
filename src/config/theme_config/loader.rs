use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
};

use super::{
    model::{
        EditorThemeTokens, FileIconThemeTokens, GitThemeTokens, IconThemeTokens, SyntaxThemeTokens,
        ThemeColor, ThemeConfig, UiThemeTokens,
    },
    raw::{
        RawEditor, RawFileIconTheme, RawFileIcons, RawGit, RawIcons, RawSyntax, RawThemeFile, RawUi,
    },
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
            git: raw_git,
            syntax: raw_syntax,
            icons: raw_icons,
            file_icons: raw_file_icons,
        } = raw;

        let default_theme_source = load_default_theme_icon_source();
        let merged_editor = merge_raw_editor(
            default_theme_source.as_ref().map(|src| &src.editor),
            &raw_editor,
        );
        let merged_ui = merge_raw_ui(default_theme_source.as_ref().map(|src| &src.ui), &raw_ui);

        let editor = parse_editor(&merged_editor)?;
        let ui = parse_ui(&merged_ui, &merged_editor)?;
        let git = parse_git(&raw_git, &ui)?;
        let syntax = parse_syntax(&raw_syntax)?;
        let merged_icons = merge_raw_icons(
            default_theme_source.as_ref().map(|src| &src.icons),
            &raw_icons,
        );
        let merged_file_icons = merge_raw_file_icons(
            default_theme_source.as_ref().map(|src| &src.file_icons),
            &raw_file_icons,
        );
        let icons = parse_icons(&merged_icons, &ui)?;

        Ok(Self {
            name: theme.name,
            description: theme.description,
            editor,
            ui,
            git,
            syntax,
            icons,
            exact_icons: parse_exact_file_icons(&merged_file_icons),
            extension_icons: parse_extension_file_icons(&merged_file_icons),
            default_file_icon: merged_file_icons
                .default_file
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .unwrap_or("built_in:file")
                .to_string(),
            default_folder_icon: merged_file_icons
                .default_folder
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .unwrap_or("built_in:folder")
                .to_string(),
        })
    }
}

fn load_default_theme_icon_source() -> Option<RawThemeFile> {
    let path = find_profile_path(ThemeConfig::default_profile())?;
    let content = std::fs::read_to_string(path).ok()?;
    toml::from_str(&content).ok()
}

fn merge_raw_editor(defaults: Option<&RawEditor>, current: &RawEditor) -> RawEditor {
    let Some(defaults) = defaults else {
        return current.clone();
    };

    RawEditor {
        bg: current.bg.clone(),
        fg: current.fg.clone(),
        cursor: current.cursor.clone(),
        selection: current.selection.clone(),
        gutter: current.gutter.clone(),
        gutter_active: current
            .gutter_active
            .clone()
            .or_else(|| defaults.gutter_active.clone()),
        indent_guide: current
            .indent_guide
            .clone()
            .or_else(|| defaults.indent_guide.clone()),
        rainbow_brackets: current
            .rainbow_brackets
            .clone()
            .or_else(|| defaults.rainbow_brackets.clone()),
        font_size: current.font_size.or(defaults.font_size),
        line_height: current.line_height.or(defaults.line_height),
        font_family: current
            .font_family
            .clone()
            .or_else(|| defaults.font_family.clone()),
        nerd_font_family: current
            .nerd_font_family
            .clone()
            .or_else(|| defaults.nerd_font_family.clone()),
    }
}

fn merge_raw_ui(defaults: Option<&RawUi>, current: &RawUi) -> RawUi {
    let Some(defaults) = defaults else {
        return current.clone();
    };

    RawUi {
        bg: current.bg.clone().or_else(|| defaults.bg.clone()),
        sidebar_bg: current.sidebar_bg.clone(),
        panel_bg: current.panel_bg.clone(),
        terminal_bg: current
            .terminal_bg
            .clone()
            .or_else(|| defaults.terminal_bg.clone()),
        overlay_bg: current
            .overlay_bg
            .clone()
            .or_else(|| defaults.overlay_bg.clone()),
        status_bar_bg: current.status_bar_bg.clone(),
        border_color: current.border_color.clone(),
        selection_bg: current
            .selection_bg
            .clone()
            .or_else(|| defaults.selection_bg.clone()),
        dirty_indicator: current
            .dirty_indicator
            .clone()
            .or_else(|| defaults.dirty_indicator.clone()),
        fg: current.fg.clone().or_else(|| defaults.fg.clone()),
        fg_dim: current.fg_dim.clone().or_else(|| defaults.fg_dim.clone()),
        fg_ghost: current
            .fg_ghost
            .clone()
            .or_else(|| defaults.fg_ghost.clone()),
        accent: current.accent.clone().or_else(|| defaults.accent.clone()),
        cyan: current.cyan.clone().or_else(|| defaults.cyan.clone()),
        magenta: current.magenta.clone().or_else(|| defaults.magenta.clone()),
        amber: current.amber.clone().or_else(|| defaults.amber.clone()),
        success: current.success.clone().or_else(|| defaults.success.clone()),
        warning: current.warning.clone().or_else(|| defaults.warning.clone()),
        info: current.info.clone().or_else(|| defaults.info.clone()),
        error: current.error.clone().or_else(|| defaults.error.clone()),
        mode_normal: current
            .mode_normal
            .clone()
            .or_else(|| defaults.mode_normal.clone()),
        mode_insert: current
            .mode_insert
            .clone()
            .or_else(|| defaults.mode_insert.clone()),
        mode_visual: current
            .mode_visual
            .clone()
            .or_else(|| defaults.mode_visual.clone()),
        sidebar_font_size: current.sidebar_font_size.or(defaults.sidebar_font_size),
        sidebar_line_height: current.sidebar_line_height.or(defaults.sidebar_line_height),
        panel_font_size: current.panel_font_size.or(defaults.panel_font_size),
        panel_line_height: current.panel_line_height.or(defaults.panel_line_height),
        sidebar_width: current.sidebar_width.or(defaults.sidebar_width),
        right_sidebar_width: current.right_sidebar_width.or(defaults.right_sidebar_width),
        bottom_panel_height: current.bottom_panel_height.or(defaults.bottom_panel_height),
        top_bar_height: current.top_bar_height.or(defaults.top_bar_height),
        status_bar_height: current.status_bar_height.or(defaults.status_bar_height),
        bg_opacity: current.bg_opacity.or(defaults.bg_opacity),
    }
}

fn merge_raw_icons(defaults: Option<&RawIcons>, current: &RawIcons) -> RawIcons {
    let Some(defaults) = defaults else {
        return current.clone();
    };

    RawIcons {
        explorer_file_marker: current
            .explorer_file_marker
            .clone()
            .or_else(|| defaults.explorer_file_marker.clone()),
        explorer_folder_collapsed_marker: current
            .explorer_folder_collapsed_marker
            .clone()
            .or_else(|| defaults.explorer_folder_collapsed_marker.clone()),
        explorer_folder_expanded_marker: current
            .explorer_folder_expanded_marker
            .clone()
            .or_else(|| defaults.explorer_folder_expanded_marker.clone()),
        file_picker_dot: current
            .file_picker_dot
            .clone()
            .or_else(|| defaults.file_picker_dot.clone()),
        folder_closed: current
            .folder_closed
            .clone()
            .or_else(|| defaults.folder_closed.clone()),
        folder_open: current
            .folder_open
            .clone()
            .or_else(|| defaults.folder_open.clone()),
        default_file: current
            .default_file
            .clone()
            .or_else(|| defaults.default_file.clone()),
        rust: current.rust.clone().or_else(|| defaults.rust.clone()),
        javascript: current
            .javascript
            .clone()
            .or_else(|| defaults.javascript.clone()),
        typescript: current
            .typescript
            .clone()
            .or_else(|| defaults.typescript.clone()),
        tsx: current.tsx.clone().or_else(|| defaults.tsx.clone()),
        jsx: current.jsx.clone().or_else(|| defaults.jsx.clone()),
        java: current.java.clone().or_else(|| defaults.java.clone()),
        kotlin: current.kotlin.clone().or_else(|| defaults.kotlin.clone()),
        c: current.c.clone().or_else(|| defaults.c.clone()),
        cpp: current.cpp.clone().or_else(|| defaults.cpp.clone()),
        csharp: current.csharp.clone().or_else(|| defaults.csharp.clone()),
        dart: current.dart.clone().or_else(|| defaults.dart.clone()),
        swift: current.swift.clone().or_else(|| defaults.swift.clone()),
        php: current.php.clone().or_else(|| defaults.php.clone()),
        ruby: current.ruby.clone().or_else(|| defaults.ruby.clone()),
        lua: current.lua.clone().or_else(|| defaults.lua.clone()),
        zig: current.zig.clone().or_else(|| defaults.zig.clone()),
        scala: current.scala.clone().or_else(|| defaults.scala.clone()),
        docker: current.docker.clone().or_else(|| defaults.docker.clone()),
        sql: current.sql.clone().or_else(|| defaults.sql.clone()),
        xml: current.xml.clone().or_else(|| defaults.xml.clone()),
        gradle: current.gradle.clone().or_else(|| defaults.gradle.clone()),
        vue: current.vue.clone().or_else(|| defaults.vue.clone()),
        svelte: current.svelte.clone().or_else(|| defaults.svelte.clone()),
        astro: current.astro.clone().or_else(|| defaults.astro.clone()),
        elm: current.elm.clone().or_else(|| defaults.elm.clone()),
        haskell: current.haskell.clone().or_else(|| defaults.haskell.clone()),
        ocaml: current.ocaml.clone().or_else(|| defaults.ocaml.clone()),
        r: current.r.clone().or_else(|| defaults.r.clone()),
        perl: current.perl.clone().or_else(|| defaults.perl.clone()),
        clojure: current.clojure.clone().or_else(|| defaults.clojure.clone()),
        fsharp: current.fsharp.clone().or_else(|| defaults.fsharp.clone()),
        nim: current.nim.clone().or_else(|| defaults.nim.clone()),
        solidity: current
            .solidity
            .clone()
            .or_else(|| defaults.solidity.clone()),
        graphql: current.graphql.clone().or_else(|| defaults.graphql.clone()),
        toml: current.toml.clone().or_else(|| defaults.toml.clone()),
        yaml: current.yaml.clone().or_else(|| defaults.yaml.clone()),
        makefile: current
            .makefile
            .clone()
            .or_else(|| defaults.makefile.clone()),
        cmake: current.cmake.clone().or_else(|| defaults.cmake.clone()),
        nginx: current.nginx.clone().or_else(|| defaults.nginx.clone()),
        terraform: current
            .terraform
            .clone()
            .or_else(|| defaults.terraform.clone()),
        ansible: current.ansible.clone().or_else(|| defaults.ansible.clone()),
        python: current.python.clone().or_else(|| defaults.python.clone()),
        go: current.go.clone().or_else(|| defaults.go.clone()),
        config: current.config.clone().or_else(|| defaults.config.clone()),
        json: current.json.clone().or_else(|| defaults.json.clone()),
        markdown: current
            .markdown
            .clone()
            .or_else(|| defaults.markdown.clone()),
        html: current.html.clone().or_else(|| defaults.html.clone()),
        css: current.css.clone().or_else(|| defaults.css.clone()),
        sass: current.sass.clone().or_else(|| defaults.sass.clone()),
        shell: current.shell.clone().or_else(|| defaults.shell.clone()),
        git: current.git.clone().or_else(|| defaults.git.clone()),
        lock: current.lock.clone().or_else(|| defaults.lock.clone()),
        image: current.image.clone().or_else(|| defaults.image.clone()),
        proto: current.proto.clone().or_else(|| defaults.proto.clone()),
    }
}

fn merge_raw_file_icons(defaults: Option<&RawFileIcons>, current: &RawFileIcons) -> RawFileIcons {
    let Some(defaults) = defaults else {
        return current.clone();
    };

    let mut extensions = defaults.extensions.clone().unwrap_or_default();
    if let Some(current_extensions) = &current.extensions {
        for (key, value) in current_extensions {
            extensions.insert(key.clone(), value.clone());
        }
    }

    let mut exact = defaults.exact.clone().unwrap_or_default();
    if let Some(current_exact) = &current.exact {
        for (key, value) in current_exact {
            exact.insert(key.clone(), value.clone());
        }
    }

    RawFileIcons {
        default_file: current
            .default_file
            .clone()
            .or_else(|| defaults.default_file.clone()),
        default_folder: current
            .default_folder
            .clone()
            .or_else(|| defaults.default_folder.clone()),
        extensions: Some(extensions),
        exact: Some(exact),
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
        bg_opacity: raw.bg_opacity.unwrap_or(100).clamp(0, 100),
    })
}

fn parse_syntax(raw: &RawSyntax) -> Result<SyntaxThemeTokens, String> {
    Ok(SyntaxThemeTokens {
        keyword: parse_color("syntax", "keyword", &raw.keyword)?,
        keyword_control: parse_color(
            "syntax",
            "keyword_control",
            raw.keyword_control
                .as_deref()
                .unwrap_or(raw.keyword.as_str()),
        )?,
        keyword_storage: parse_color(
            "syntax",
            "keyword_storage",
            raw.keyword_storage
                .as_deref()
                .unwrap_or(raw.keyword.as_str()),
        )?,
        string: parse_color("syntax", "string", &raw.string)?,
        string_escape: parse_color(
            "syntax",
            "string_escape",
            raw.string_escape
                .as_deref()
                .or(raw.escape.as_deref())
                .or(raw.constant.as_deref())
                .unwrap_or(raw.number.as_str()),
        )?,
        function: parse_color("syntax", "function", &raw.function)?,
        function_builtin: parse_color(
            "syntax",
            "function_builtin",
            raw.function_builtin
                .as_deref()
                .unwrap_or(raw.function.as_str()),
        )?,
        comment: parse_color("syntax", "comment", &raw.comment)?,
        comment_doc: parse_color(
            "syntax",
            "comment_doc",
            raw.comment_doc.as_deref().unwrap_or(raw.comment.as_str()),
        )?,
        r#type: parse_color("syntax", "type", &raw.r#type)?,
        type_builtin: parse_color(
            "syntax",
            "type_builtin",
            raw.type_builtin.as_deref().unwrap_or(raw.r#type.as_str()),
        )?,
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
        variable_builtin: parse_color(
            "syntax",
            "variable_builtin",
            raw.variable_builtin
                .as_deref()
                .or(raw.variable.as_deref())
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
        markup_strong: parse_color(
            "syntax",
            "markup_strong",
            raw.markup_strong.as_deref().unwrap_or(raw.keyword.as_str()),
        )?,
        markup_italic: parse_color(
            "syntax",
            "markup_italic",
            raw.markup_italic.as_deref().unwrap_or(raw.comment.as_str()),
        )?,
        markup_inline_code: parse_color(
            "syntax",
            "markup_inline_code",
            raw.markup_inline_code
                .as_deref()
                .unwrap_or(raw.string.as_str()),
        )?,
        markup_link: parse_color(
            "syntax",
            "markup_link",
            raw.markup_link.as_deref().unwrap_or(raw.function.as_str()),
        )?,
    })
}

fn parse_git(raw: &RawGit, ui: &UiThemeTokens) -> Result<GitThemeTokens, String> {
    Ok(GitThemeTokens {
        modified_sidebar: parse_color(
            "git",
            "modified_sidebar",
            raw.modified_sidebar.as_deref().unwrap_or("#E2C08D"),
        )?,
        added_sidebar: parse_color(
            "git",
            "added_sidebar",
            raw.added_sidebar.as_deref().unwrap_or("#7FD68C"),
        )?,
        modified_gutter: parse_color(
            "git",
            "modified_gutter",
            raw.modified_gutter
                .as_deref()
                .or(raw.modified_sidebar.as_deref())
                .unwrap_or_else(|| {
                    let _ = ui.warning;
                    "#E2C08D"
                }),
        )?,
        added_gutter: parse_color(
            "git",
            "added_gutter",
            raw.added_gutter
                .as_deref()
                .or(raw.added_sidebar.as_deref())
                .unwrap_or("#50D890"),
        )?,
        deleted_gutter: parse_color(
            "git",
            "deleted_gutter",
            raw.deleted_gutter.as_deref().unwrap_or("#F14C4C"),
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
            "built_in:folder",
            ui.amber,
        )?,
        folder_open: parse_file_icon(
            "icons.folder_open",
            raw.folder_open.as_ref(),
            "built_in:folder_open",
            ui.amber,
        )?,
        default_file: parse_file_icon(
            "icons.default_file",
            raw.default_file.as_ref(),
            "built_in:file",
            ui.fg_ghost,
        )?,
        rust: parse_file_icon("icons.rust", raw.rust.as_ref(), "built_in:rust", ui.warning)?,
        javascript: parse_file_icon(
            "icons.javascript",
            raw.javascript.as_ref(),
            "built_in:node",
            ui.amber,
        )?,
        typescript: parse_file_icon(
            "icons.typescript",
            raw.typescript.as_ref(),
            "built_in:typescript",
            ui.info,
        )?,
        tsx: parse_file_icon("icons.tsx", raw.tsx.as_ref(), "built_in:tsx", ui.info)?,
        jsx: parse_file_icon("icons.jsx", raw.jsx.as_ref(), "built_in:reactjs", ui.amber)?,
        java: parse_file_icon("icons.java", raw.java.as_ref(), "built_in:java", ui.error)?,
        kotlin: parse_file_icon(
            "icons.kotlin",
            raw.kotlin.as_ref(),
            "built_in:kotlin",
            ui.magenta,
        )?,
        c: parse_file_icon("icons.c", raw.c.as_ref(), "built_in:c", ui.info)?,
        cpp: parse_file_icon("icons.cpp", raw.cpp.as_ref(), "built_in:cpp", ui.info)?,
        csharp: parse_file_icon(
            "icons.csharp",
            raw.csharp.as_ref(),
            "built_in:csharp",
            ui.success,
        )?,
        dart: parse_file_icon("icons.dart", raw.dart.as_ref(), "built_in:dart", ui.cyan)?,
        swift: parse_file_icon(
            "icons.swift",
            raw.swift.as_ref(),
            "built_in:swift",
            ui.warning,
        )?,
        php: parse_file_icon("icons.php", raw.php.as_ref(), "built_in:php", ui.magenta)?,
        ruby: parse_file_icon("icons.ruby", raw.ruby.as_ref(), "built_in:ruby", ui.error)?,
        lua: parse_file_icon("icons.lua", raw.lua.as_ref(), "built_in:lua", ui.info)?,
        zig: parse_file_icon("icons.zig", raw.zig.as_ref(), "built_in:zig", ui.warning)?,
        scala: parse_file_icon(
            "icons.scala",
            raw.scala.as_ref(),
            "built_in:scala",
            ui.error,
        )?,
        docker: parse_file_icon(
            "icons.docker",
            raw.docker.as_ref(),
            "built_in:docker",
            ui.info,
        )?,
        sql: parse_file_icon("icons.sql", raw.sql.as_ref(), "built_in:sql", ui.amber)?,
        xml: parse_file_icon("icons.xml", raw.xml.as_ref(), "built_in:xml", ui.amber)?,
        gradle: parse_file_icon(
            "icons.gradle",
            raw.gradle.as_ref(),
            "built_in:gradle",
            ui.success,
        )?,
        vue: parse_file_icon("icons.vue", raw.vue.as_ref(), "built_in:vue", ui.success)?,
        svelte: parse_file_icon(
            "icons.svelte",
            raw.svelte.as_ref(),
            "built_in:svelte",
            ui.error,
        )?,
        astro: parse_file_icon(
            "icons.astro",
            raw.astro.as_ref(),
            "built_in:astro",
            ui.warning,
        )?,
        elm: parse_file_icon("icons.elm", raw.elm.as_ref(), "built_in:elm", ui.info)?,
        haskell: parse_file_icon(
            "icons.haskell",
            raw.haskell.as_ref(),
            "built_in:haskell",
            ui.magenta,
        )?,
        ocaml: parse_file_icon(
            "icons.ocaml",
            raw.ocaml.as_ref(),
            "built_in:ocaml",
            ui.amber,
        )?,
        r: parse_file_icon("icons.r", raw.r.as_ref(), "built_in:r", ui.info)?,
        perl: parse_file_icon("icons.perl", raw.perl.as_ref(), "built_in:perl", ui.info)?,
        clojure: parse_file_icon(
            "icons.clojure",
            raw.clojure.as_ref(),
            "built_in:clojure",
            ui.success,
        )?,
        fsharp: parse_file_icon(
            "icons.fsharp",
            raw.fsharp.as_ref(),
            "built_in:fsharp",
            ui.info,
        )?,
        nim: parse_file_icon("icons.nim", raw.nim.as_ref(), "built_in:nim", ui.amber)?,
        solidity: parse_file_icon(
            "icons.solidity",
            raw.solidity.as_ref(),
            "built_in:sol",
            ui.fg_dim,
        )?,
        graphql: parse_file_icon(
            "icons.graphql",
            raw.graphql.as_ref(),
            "built_in:graphql",
            ui.magenta,
        )?,
        toml: parse_file_icon(
            "icons.toml",
            raw.toml.as_ref(),
            "built_in:toml",
            ui.fg_ghost,
        )?,
        yaml: parse_file_icon("icons.yaml", raw.yaml.as_ref(), "built_in:yaml", ui.amber)?,
        makefile: parse_file_icon(
            "icons.makefile",
            raw.makefile.as_ref(),
            "built_in:makefile",
            ui.warning,
        )?,
        cmake: parse_file_icon("icons.cmake", raw.cmake.as_ref(), "built_in:cmake", ui.info)?,
        nginx: parse_file_icon(
            "icons.nginx",
            raw.nginx.as_ref(),
            "built_in:nginx",
            ui.success,
        )?,
        terraform: parse_file_icon(
            "icons.terraform",
            raw.terraform.as_ref(),
            "built_in:terraform",
            ui.magenta,
        )?,
        ansible: parse_file_icon(
            "icons.ansible",
            raw.ansible.as_ref(),
            "built_in:ansible",
            ui.error,
        )?,
        python: parse_file_icon(
            "icons.python",
            raw.python.as_ref(),
            "built_in:python",
            ui.info,
        )?,
        go: parse_file_icon("icons.go", raw.go.as_ref(), "built_in:go", ui.cyan)?,
        config: parse_file_icon(
            "icons.config",
            raw.config.as_ref(),
            "built_in:conf",
            ui.fg_ghost,
        )?,
        json: parse_file_icon("icons.json", raw.json.as_ref(), "built_in:json", ui.amber)?,
        markdown: parse_file_icon(
            "icons.markdown",
            raw.markdown.as_ref(),
            "built_in:markdown",
            ui.info,
        )?,
        html: parse_file_icon("icons.html", raw.html.as_ref(), "built_in:html", ui.error)?,
        css: parse_file_icon("icons.css", raw.css.as_ref(), "built_in:css", ui.info)?,
        sass: parse_file_icon("icons.sass", raw.sass.as_ref(), "built_in:sass", ui.magenta)?,
        shell: parse_file_icon(
            "icons.shell",
            raw.shell.as_ref(),
            "built_in:shell",
            ui.success,
        )?,
        git: parse_file_icon("icons.git", raw.git.as_ref(), "built_in:git", ui.warning)?,
        lock: parse_file_icon(
            "icons.lock",
            raw.lock.as_ref(),
            "built_in:lock",
            ui.fg_ghost,
        )?,
        image: parse_file_icon(
            "icons.image",
            raw.image.as_ref(),
            "built_in:image",
            ui.success,
        )?,
        proto: parse_file_icon("icons.proto", raw.proto.as_ref(), "built_in:proto", ui.info)?,
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
