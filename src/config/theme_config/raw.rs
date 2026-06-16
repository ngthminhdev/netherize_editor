use std::collections::HashMap;

use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub(in crate::config::theme_config) struct RawThemeFile {
    #[serde(default)]
    pub(in crate::config::theme_config) theme: RawThemeMeta,
    pub(in crate::config::theme_config) editor: RawEditor,
    pub(in crate::config::theme_config) ui: RawUi,
    #[serde(default)]
    pub(in crate::config::theme_config) git: RawGit,
    pub(in crate::config::theme_config) syntax: RawSyntax,
    #[serde(default)]
    pub(in crate::config::theme_config) icons: RawIcons,
    #[serde(default)]
    pub(in crate::config::theme_config) file_icons: RawFileIcons,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub(in crate::config::theme_config) struct RawGit {
    pub(in crate::config::theme_config) modified_sidebar: Option<String>,
    pub(in crate::config::theme_config) added_sidebar: Option<String>,
    pub(in crate::config::theme_config) modified_gutter: Option<String>,
    pub(in crate::config::theme_config) added_gutter: Option<String>,
    pub(in crate::config::theme_config) deleted_gutter: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub(in crate::config::theme_config) struct RawThemeMeta {
    #[serde(default)]
    pub(in crate::config::theme_config) name: String,
    pub(in crate::config::theme_config) description: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub(in crate::config::theme_config) struct RawEditor {
    pub(in crate::config::theme_config) bg: String,
    pub(in crate::config::theme_config) fg: String,
    pub(in crate::config::theme_config) cursor: String,
    pub(in crate::config::theme_config) selection: String,
    pub(in crate::config::theme_config) yank_flash: Option<String>,
    pub(in crate::config::theme_config) gutter: String,
    pub(in crate::config::theme_config) gutter_active: Option<String>,
    pub(in crate::config::theme_config) indent_guide: Option<String>,
    pub(in crate::config::theme_config) rainbow_brackets: Option<Vec<String>>,
    pub(in crate::config::theme_config) font_size: Option<f32>,
    pub(in crate::config::theme_config) line_height: Option<f32>,
    pub(in crate::config::theme_config) font_family: Option<String>,
    /// NerdFont riêng cho sidebar icons; nếu không có thì fallback về font_family.
    pub(in crate::config::theme_config) nerd_font_family: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub(in crate::config::theme_config) struct RawUi {
    pub(in crate::config::theme_config) bg: Option<String>,
    pub(in crate::config::theme_config) sidebar_bg: String,
    pub(in crate::config::theme_config) panel_bg: String,
    pub(in crate::config::theme_config) terminal_bg: Option<String>,
    pub(in crate::config::theme_config) overlay_bg: Option<String>,
    pub(in crate::config::theme_config) status_bar_bg: String,
    pub(in crate::config::theme_config) border_color: String,
    pub(in crate::config::theme_config) selection_bg: Option<String>,
    pub(in crate::config::theme_config) dirty_indicator: Option<String>,
    pub(in crate::config::theme_config) fg: Option<String>,
    pub(in crate::config::theme_config) fg_dim: Option<String>,
    pub(in crate::config::theme_config) fg_ghost: Option<String>,
    pub(in crate::config::theme_config) accent: Option<String>,
    pub(in crate::config::theme_config) cyan: Option<String>,
    pub(in crate::config::theme_config) magenta: Option<String>,
    pub(in crate::config::theme_config) amber: Option<String>,
    pub(in crate::config::theme_config) success: Option<String>,
    pub(in crate::config::theme_config) warning: Option<String>,
    pub(in crate::config::theme_config) info: Option<String>,
    pub(in crate::config::theme_config) error: Option<String>,
    pub(in crate::config::theme_config) mode_normal: Option<String>,
    pub(in crate::config::theme_config) mode_insert: Option<String>,
    pub(in crate::config::theme_config) mode_visual: Option<String>,
    pub(in crate::config::theme_config) sidebar_font_size: Option<f32>,
    pub(in crate::config::theme_config) sidebar_line_height: Option<f32>,
    pub(in crate::config::theme_config) panel_font_size: Option<f32>,
    pub(in crate::config::theme_config) panel_line_height: Option<f32>,
    pub(in crate::config::theme_config) sidebar_width: Option<f32>,
    pub(in crate::config::theme_config) right_sidebar_width: Option<f32>,
    pub(in crate::config::theme_config) bottom_panel_height: Option<f32>,
    pub(in crate::config::theme_config) top_bar_height: Option<f32>,
    pub(in crate::config::theme_config) status_bar_height: Option<f32>,
}

#[derive(Debug, Clone, Deserialize)]
pub(in crate::config::theme_config) struct RawSyntax {
    pub(in crate::config::theme_config) keyword: String,
    pub(in crate::config::theme_config) keyword_control: Option<String>,
    pub(in crate::config::theme_config) keyword_storage: Option<String>,
    pub(in crate::config::theme_config) string: String,
    pub(in crate::config::theme_config) string_escape: Option<String>,
    pub(in crate::config::theme_config) function: String,
    pub(in crate::config::theme_config) function_builtin: Option<String>,
    pub(in crate::config::theme_config) comment: String,
    pub(in crate::config::theme_config) comment_doc: Option<String>,
    pub(in crate::config::theme_config) r#type: String,
    pub(in crate::config::theme_config) type_builtin: Option<String>,
    pub(in crate::config::theme_config) number: String,
    pub(in crate::config::theme_config) boolean: Option<String>,
    pub(in crate::config::theme_config) identifier: Option<String>,
    pub(in crate::config::theme_config) variable: Option<String>,
    pub(in crate::config::theme_config) variable_builtin: Option<String>,
    pub(in crate::config::theme_config) parameter: Option<String>,
    pub(in crate::config::theme_config) field: Option<String>,
    pub(in crate::config::theme_config) property: Option<String>,
    pub(in crate::config::theme_config) constant: Option<String>,
    pub(in crate::config::theme_config) operator: Option<String>,
    pub(in crate::config::theme_config) punctuation: Option<String>,
    pub(in crate::config::theme_config) escape: Option<String>,
    pub(in crate::config::theme_config) r#macro: Option<String>,
    pub(in crate::config::theme_config) lifetime: Option<String>,
    pub(in crate::config::theme_config) constructor: Option<String>,
    pub(in crate::config::theme_config) attribute: Option<String>,
    pub(in crate::config::theme_config) namespace: Option<String>,
    pub(in crate::config::theme_config) tag: Option<String>,
    pub(in crate::config::theme_config) markup_strong: Option<String>,
    pub(in crate::config::theme_config) markup_italic: Option<String>,
    pub(in crate::config::theme_config) markup_inline_code: Option<String>,
    pub(in crate::config::theme_config) markup_link: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub(in crate::config::theme_config) struct RawIcons {
    pub(in crate::config::theme_config) explorer_file_marker: Option<String>,
    pub(in crate::config::theme_config) explorer_folder_collapsed_marker: Option<String>,
    pub(in crate::config::theme_config) explorer_folder_expanded_marker: Option<String>,
    pub(in crate::config::theme_config) file_picker_dot: Option<String>,
    pub(in crate::config::theme_config) folder_closed: Option<RawFileIconTheme>,
    pub(in crate::config::theme_config) folder_open: Option<RawFileIconTheme>,
    pub(in crate::config::theme_config) default_file: Option<RawFileIconTheme>,
    pub(in crate::config::theme_config) rust: Option<RawFileIconTheme>,
    pub(in crate::config::theme_config) javascript: Option<RawFileIconTheme>,
    pub(in crate::config::theme_config) typescript: Option<RawFileIconTheme>,
    pub(in crate::config::theme_config) tsx: Option<RawFileIconTheme>,
    pub(in crate::config::theme_config) jsx: Option<RawFileIconTheme>,
    pub(in crate::config::theme_config) java: Option<RawFileIconTheme>,
    pub(in crate::config::theme_config) kotlin: Option<RawFileIconTheme>,
    pub(in crate::config::theme_config) c: Option<RawFileIconTheme>,
    pub(in crate::config::theme_config) cpp: Option<RawFileIconTheme>,
    pub(in crate::config::theme_config) csharp: Option<RawFileIconTheme>,
    pub(in crate::config::theme_config) dart: Option<RawFileIconTheme>,
    pub(in crate::config::theme_config) swift: Option<RawFileIconTheme>,
    pub(in crate::config::theme_config) php: Option<RawFileIconTheme>,
    pub(in crate::config::theme_config) ruby: Option<RawFileIconTheme>,
    pub(in crate::config::theme_config) lua: Option<RawFileIconTheme>,
    pub(in crate::config::theme_config) zig: Option<RawFileIconTheme>,
    pub(in crate::config::theme_config) scala: Option<RawFileIconTheme>,
    pub(in crate::config::theme_config) docker: Option<RawFileIconTheme>,
    pub(in crate::config::theme_config) sql: Option<RawFileIconTheme>,
    pub(in crate::config::theme_config) xml: Option<RawFileIconTheme>,
    pub(in crate::config::theme_config) gradle: Option<RawFileIconTheme>,
    pub(in crate::config::theme_config) vue: Option<RawFileIconTheme>,
    pub(in crate::config::theme_config) svelte: Option<RawFileIconTheme>,
    pub(in crate::config::theme_config) astro: Option<RawFileIconTheme>,
    pub(in crate::config::theme_config) elm: Option<RawFileIconTheme>,
    pub(in crate::config::theme_config) haskell: Option<RawFileIconTheme>,
    pub(in crate::config::theme_config) ocaml: Option<RawFileIconTheme>,
    pub(in crate::config::theme_config) r: Option<RawFileIconTheme>,
    pub(in crate::config::theme_config) perl: Option<RawFileIconTheme>,
    pub(in crate::config::theme_config) clojure: Option<RawFileIconTheme>,
    pub(in crate::config::theme_config) fsharp: Option<RawFileIconTheme>,
    pub(in crate::config::theme_config) nim: Option<RawFileIconTheme>,
    pub(in crate::config::theme_config) solidity: Option<RawFileIconTheme>,
    pub(in crate::config::theme_config) graphql: Option<RawFileIconTheme>,
    pub(in crate::config::theme_config) toml: Option<RawFileIconTheme>,
    pub(in crate::config::theme_config) yaml: Option<RawFileIconTheme>,
    pub(in crate::config::theme_config) makefile: Option<RawFileIconTheme>,
    pub(in crate::config::theme_config) cmake: Option<RawFileIconTheme>,
    pub(in crate::config::theme_config) nginx: Option<RawFileIconTheme>,
    pub(in crate::config::theme_config) terraform: Option<RawFileIconTheme>,
    pub(in crate::config::theme_config) ansible: Option<RawFileIconTheme>,
    pub(in crate::config::theme_config) python: Option<RawFileIconTheme>,
    pub(in crate::config::theme_config) go: Option<RawFileIconTheme>,
    pub(in crate::config::theme_config) config: Option<RawFileIconTheme>,
    pub(in crate::config::theme_config) json: Option<RawFileIconTheme>,
    pub(in crate::config::theme_config) markdown: Option<RawFileIconTheme>,
    pub(in crate::config::theme_config) html: Option<RawFileIconTheme>,
    pub(in crate::config::theme_config) css: Option<RawFileIconTheme>,
    pub(in crate::config::theme_config) sass: Option<RawFileIconTheme>,
    pub(in crate::config::theme_config) shell: Option<RawFileIconTheme>,
    pub(in crate::config::theme_config) git: Option<RawFileIconTheme>,
    pub(in crate::config::theme_config) lock: Option<RawFileIconTheme>,
    pub(in crate::config::theme_config) image: Option<RawFileIconTheme>,
    pub(in crate::config::theme_config) proto: Option<RawFileIconTheme>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub(in crate::config::theme_config) struct RawFileIconTheme {
    pub(in crate::config::theme_config) glyph: Option<String>,
    pub(in crate::config::theme_config) color: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub(in crate::config::theme_config) struct RawFileIcons {
    pub(in crate::config::theme_config) default_file: Option<String>,
    pub(in crate::config::theme_config) default_folder: Option<String>,
    pub(in crate::config::theme_config) extensions: Option<HashMap<String, String>>,
    pub(in crate::config::theme_config) exact: Option<HashMap<String, String>>,
}
