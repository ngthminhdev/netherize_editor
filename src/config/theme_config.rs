use std::path::{Path, PathBuf};

use serde::Deserialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ThemeColor {
    rgba_u8: [u8; 4],
}

impl ThemeColor {
    pub const fn from_rgba_u8(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self {
            rgba_u8: [r, g, b, a],
        }
    }

    pub fn from_hex(hex: &str) -> Result<Self, String> {
        let value = hex.trim();
        let digits = value.strip_prefix('#').unwrap_or(value);

        match digits.len() {
            6 => {
                let r = parse_hex_component(&digits[0..2], value)?;
                let g = parse_hex_component(&digits[2..4], value)?;
                let b = parse_hex_component(&digits[4..6], value)?;
                Ok(Self::from_rgba_u8(r, g, b, 255))
            }
            8 => {
                let r = parse_hex_component(&digits[0..2], value)?;
                let g = parse_hex_component(&digits[2..4], value)?;
                let b = parse_hex_component(&digits[4..6], value)?;
                let a = parse_hex_component(&digits[6..8], value)?;
                Ok(Self::from_rgba_u8(r, g, b, a))
            }
            _ => Err(format!(
                "invalid hex color '{value}' (expected #RRGGBB or #RRGGBBAA)"
            )),
        }
    }

    pub const fn as_u8(self) -> [u8; 4] {
        self.rgba_u8
    }

    pub fn as_f32(self) -> [f32; 4] {
        [
            f32::from(self.rgba_u8[0]) / 255.0,
            f32::from(self.rgba_u8[1]) / 255.0,
            f32::from(self.rgba_u8[2]) / 255.0,
            f32::from(self.rgba_u8[3]) / 255.0,
        ]
    }
}

fn parse_hex_component(component: &str, original: &str) -> Result<u8, String> {
    u8::from_str_radix(component, 16)
        .map_err(|_| format!("invalid hex color '{original}' (bad component '{component}')"))
}

#[derive(Debug, Clone)]
pub struct ThemeConfig {
    pub name: String,
    pub editor: EditorThemeTokens,
    pub ui: UiThemeTokens,
    pub syntax: SyntaxThemeTokens,
}

#[derive(Debug, Clone)]
pub struct EditorThemeTokens {
    pub bg: ThemeColor,
    pub fg: ThemeColor,
    pub cursor: ThemeColor,
    pub selection: ThemeColor,
    pub gutter: ThemeColor,
    pub gutter_active: ThemeColor,
    pub font_size: f32,
    pub line_height: f32,
    /// Optional font family name (e.g. "GoogleSansCode"). Empty/missing → default.
    pub font_family: Option<String>,
}

#[derive(Debug, Clone)]
pub struct UiThemeTokens {
    pub sidebar_bg: ThemeColor,
    pub panel_bg: ThemeColor,
    pub status_bar_bg: ThemeColor,
    pub border_color: ThemeColor,
    pub selection_bg: ThemeColor,
    pub fg_dim: ThemeColor,
    pub fg_faint: ThemeColor,
    pub fg_ghost: ThemeColor,
    pub accent: ThemeColor,
    pub cyan: ThemeColor,
    pub magenta: ThemeColor,
    pub amber: ThemeColor,
    pub success: ThemeColor,
    pub error: ThemeColor,
    pub sidebar_font_size: f32,
    pub sidebar_line_height: f32,
    pub panel_font_size: f32,
    pub panel_line_height: f32,
    /// Default width of the left explorer sidebar, px.
    pub sidebar_width: f32,
    /// Default width of the right inspector sidebar, px.
    pub right_sidebar_width: f32,
    /// Default height of the bottom terminal panel, px.
    pub bottom_panel_height: f32,
    /// Height of the top tab/title bar, px.
    pub top_bar_height: f32,
    /// Height of the status bar, px.
    pub status_bar_height: f32,
}

#[derive(Debug, Clone)]
pub struct SyntaxThemeTokens {
    pub keyword: ThemeColor,
    pub string: ThemeColor,
    pub function: ThemeColor,
    pub comment: ThemeColor,
    pub r#type: ThemeColor,
    pub number: ThemeColor,
    pub constant: ThemeColor,
    pub operator: ThemeColor,
    pub variable: ThemeColor,
}

impl ThemeConfig {
    pub fn active_profile() -> String {
        std::env::var("NETHERIZE_THEME").unwrap_or_else(|_| "default-dark".into())
    }

    pub fn load_active() -> Self {
        let profile = Self::active_profile();
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
            .ok_or_else(|| format!("theme profile '{profile}' not found under config/themes"))?;
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

    pub fn builtin_dark() -> Self {
        Self {
            name: "builtin-dark".to_string(),
            editor: EditorThemeTokens {
                bg: ThemeColor::from_rgba_u8(30, 30, 30, 255),
                fg: ThemeColor::from_rgba_u8(212, 212, 212, 255),
                cursor: ThemeColor::from_rgba_u8(174, 175, 173, 255),
                selection: ThemeColor::from_rgba_u8(38, 79, 120, 255),
                gutter: ThemeColor::from_rgba_u8(42, 45, 46, 255),
                gutter_active: ThemeColor::from_rgba_u8(212, 212, 212, 255),
                font_size: 17.0,
                line_height: 26.0,
                font_family: None,
            },
            ui: UiThemeTokens {
                sidebar_bg: ThemeColor::from_rgba_u8(37, 37, 38, 255),
                panel_bg: ThemeColor::from_rgba_u8(30, 30, 30, 255),
                status_bar_bg: ThemeColor::from_rgba_u8(0, 122, 204, 255),
                border_color: ThemeColor::from_rgba_u8(60, 60, 60, 255),
                selection_bg: ThemeColor::from_rgba_u8(9, 71, 113, 255),
                fg_dim: ThemeColor::from_rgba_u8(183, 191, 204, 255),
                fg_faint: ThemeColor::from_rgba_u8(143, 152, 170, 255),
                fg_ghost: ThemeColor::from_rgba_u8(109, 116, 131, 255),
                accent: ThemeColor::from_rgba_u8(155, 229, 100, 255),
                cyan: ThemeColor::from_rgba_u8(47, 211, 246, 255),
                magenta: ThemeColor::from_rgba_u8(231, 122, 233, 255),
                amber: ThemeColor::from_rgba_u8(245, 182, 58, 255),
                success: ThemeColor::from_rgba_u8(103, 214, 124, 255),
                error: ThemeColor::from_rgba_u8(255, 123, 114, 255),
                sidebar_font_size: 14.0,
                sidebar_line_height: 21.0,
                panel_font_size: 14.0,
                panel_line_height: 21.0,
                sidebar_width: 260.0,
                right_sidebar_width: 280.0,
                bottom_panel_height: 220.0,
                top_bar_height: 32.0,
                status_bar_height: 24.0,
            },
            syntax: SyntaxThemeTokens {
                keyword: ThemeColor::from_rgba_u8(86, 156, 214, 255),
                string: ThemeColor::from_rgba_u8(206, 145, 120, 255),
                function: ThemeColor::from_rgba_u8(220, 220, 170, 255),
                comment: ThemeColor::from_rgba_u8(106, 153, 85, 255),
                r#type: ThemeColor::from_rgba_u8(78, 201, 176, 255),
                number: ThemeColor::from_rgba_u8(181, 206, 168, 255),
                constant: ThemeColor::from_rgba_u8(79, 193, 255, 255),
                operator: ThemeColor::from_rgba_u8(212, 212, 212, 255),
                variable: ThemeColor::from_rgba_u8(156, 220, 254, 255),
            },
        }
    }

    fn from_raw(raw: RawThemeFile) -> Result<Self, String> {
        Ok(Self {
            name: raw.theme.name,
            editor: EditorThemeTokens {
                bg: parse_color("editor", "bg", &raw.editor.bg)?,
                fg: parse_color("editor", "fg", &raw.editor.fg)?,
                cursor: parse_color("editor", "cursor", &raw.editor.cursor)?,
                selection: parse_color("editor", "selection", &raw.editor.selection)?,
                gutter: parse_color("editor", "gutter", &raw.editor.gutter)?,
                gutter_active: parse_color(
                    "editor",
                    "gutter_active",
                    raw.editor
                        .gutter_active
                        .as_deref()
                        .unwrap_or(raw.editor.fg.as_str()),
                )?,
                font_size: parse_positive_size(
                    "editor",
                    "font_size",
                    raw.editor.font_size.unwrap_or(17.0),
                )?,
                line_height: parse_positive_size(
                    "editor",
                    "line_height",
                    raw.editor.line_height.unwrap_or(26.0),
                )?,
                font_family: raw
                    .editor
                    .font_family
                    .as_deref()
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(str::to_string),
            },
            ui: UiThemeTokens {
                sidebar_bg: parse_color("ui", "sidebar_bg", &raw.ui.sidebar_bg)?,
                panel_bg: parse_color("ui", "panel_bg", &raw.ui.panel_bg)?,
                status_bar_bg: parse_color("ui", "status_bar_bg", &raw.ui.status_bar_bg)?,
                border_color: parse_color("ui", "border_color", &raw.ui.border_color)?,
                selection_bg: parse_color(
                    "ui",
                    "selection_bg",
                    raw.ui.selection_bg.as_deref().unwrap_or("#094771"),
                )?,
                fg_dim: parse_color(
                    "ui",
                    "fg_dim",
                    raw.ui.fg_dim.as_deref().unwrap_or("#b7bfcc"),
                )?,
                fg_faint: parse_color(
                    "ui",
                    "fg_faint",
                    raw.ui.fg_faint.as_deref().unwrap_or("#8f98aa"),
                )?,
                fg_ghost: parse_color(
                    "ui",
                    "fg_ghost",
                    raw.ui.fg_ghost.as_deref().unwrap_or("#6d7483"),
                )?,
                accent: parse_color(
                    "ui",
                    "accent",
                    raw.ui.accent.as_deref().unwrap_or("#9BE564"),
                )?,
                cyan: parse_color("ui", "cyan", raw.ui.cyan.as_deref().unwrap_or("#2FD3F6"))?,
                magenta: parse_color(
                    "ui",
                    "magenta",
                    raw.ui.magenta.as_deref().unwrap_or("#E77AE9"),
                )?,
                amber: parse_color("ui", "amber", raw.ui.amber.as_deref().unwrap_or("#F5B63A"))?,
                success: parse_color(
                    "ui",
                    "success",
                    raw.ui.success.as_deref().unwrap_or("#67D67C"),
                )?,
                error: parse_color("ui", "error", raw.ui.error.as_deref().unwrap_or("#FF7B72"))?,
                sidebar_font_size: parse_positive_size(
                    "ui",
                    "sidebar_font_size",
                    raw.ui.sidebar_font_size.unwrap_or(14.0),
                )?,
                sidebar_line_height: parse_positive_size(
                    "ui",
                    "sidebar_line_height",
                    raw.ui.sidebar_line_height.unwrap_or(21.0),
                )?,
                panel_font_size: parse_positive_size(
                    "ui",
                    "panel_font_size",
                    raw.ui.panel_font_size.unwrap_or(14.0),
                )?,
                panel_line_height: parse_positive_size(
                    "ui",
                    "panel_line_height",
                    raw.ui.panel_line_height.unwrap_or(21.0),
                )?,
                sidebar_width: parse_positive_size(
                    "ui",
                    "sidebar_width",
                    raw.ui.sidebar_width.unwrap_or(260.0),
                )?,
                right_sidebar_width: parse_positive_size(
                    "ui",
                    "right_sidebar_width",
                    raw.ui.right_sidebar_width.unwrap_or(280.0),
                )?,
                bottom_panel_height: parse_positive_size(
                    "ui",
                    "bottom_panel_height",
                    raw.ui.bottom_panel_height.unwrap_or(220.0),
                )?,
                top_bar_height: parse_positive_size(
                    "ui",
                    "top_bar_height",
                    raw.ui.top_bar_height.unwrap_or(32.0),
                )?,
                status_bar_height: parse_positive_size(
                    "ui",
                    "status_bar_height",
                    raw.ui.status_bar_height.unwrap_or(24.0),
                )?,
            },
            syntax: SyntaxThemeTokens {
                keyword: parse_color("syntax", "keyword", &raw.syntax.keyword)?,
                string: parse_color("syntax", "string", &raw.syntax.string)?,
                function: parse_color("syntax", "function", &raw.syntax.function)?,
                comment: parse_color("syntax", "comment", &raw.syntax.comment)?,
                r#type: parse_color("syntax", "type", &raw.syntax.r#type)?,
                number: parse_color("syntax", "number", &raw.syntax.number)?,
                constant: parse_color("syntax", "constant", &raw.syntax.constant)?,
                operator: parse_color("syntax", "operator", &raw.syntax.operator)?,
                variable: parse_color("syntax", "variable", &raw.syntax.variable)?,
            },
        })
    }
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

fn find_profile_path(name: &str) -> Option<PathBuf> {
    let filename = format!("{name}.toml");

    if let Ok(cwd) = std::env::current_dir() {
        let path = cwd.join("config").join("themes").join(&filename);
        if path.exists() {
            return Some(path);
        }
    }

    if let Ok(exe) = std::env::current_exe()
        && let Some(parent) = exe.parent()
    {
        let path = parent.join("config").join("themes").join(&filename);
        if path.exists() {
            return Some(path);
        }
    }

    None
}

#[derive(Debug, Deserialize)]
struct RawThemeFile {
    #[serde(default)]
    theme: RawThemeMeta,
    editor: RawEditor,
    ui: RawUi,
    syntax: RawSyntax,
}

#[derive(Debug, Default, Deserialize)]
struct RawThemeMeta {
    #[serde(default)]
    name: String,
}

#[derive(Debug, Deserialize)]
struct RawEditor {
    bg: String,
    fg: String,
    cursor: String,
    selection: String,
    gutter: String,
    gutter_active: Option<String>,
    font_size: Option<f32>,
    line_height: Option<f32>,
    font_family: Option<String>,
}

#[derive(Debug, Deserialize)]
struct RawUi {
    sidebar_bg: String,
    panel_bg: String,
    status_bar_bg: String,
    border_color: String,
    selection_bg: Option<String>,
    fg_dim: Option<String>,
    fg_faint: Option<String>,
    fg_ghost: Option<String>,
    accent: Option<String>,
    cyan: Option<String>,
    magenta: Option<String>,
    amber: Option<String>,
    success: Option<String>,
    error: Option<String>,
    sidebar_font_size: Option<f32>,
    sidebar_line_height: Option<f32>,
    panel_font_size: Option<f32>,
    panel_line_height: Option<f32>,
    sidebar_width: Option<f32>,
    right_sidebar_width: Option<f32>,
    bottom_panel_height: Option<f32>,
    top_bar_height: Option<f32>,
    status_bar_height: Option<f32>,
}

#[derive(Debug, Deserialize)]
struct RawSyntax {
    keyword: String,
    string: String,
    function: String,
    comment: String,
    r#type: String,
    number: String,
    constant: String,
    operator: String,
    variable: String,
}

#[cfg(test)]
mod tests {
    use super::{ThemeColor, ThemeConfig};

    #[test]
    fn parse_hex_color_supports_rgb_and_rgba() {
        let rgb = ThemeColor::from_hex("#AABBCC").expect("rgb hex should parse");
        assert_eq!(rgb.as_u8(), [0xAA, 0xBB, 0xCC, 0xFF]);

        let rgba = ThemeColor::from_hex("A1B2C3DD").expect("rgba hex should parse");
        assert_eq!(rgba.as_u8(), [0xA1, 0xB2, 0xC3, 0xDD]);
    }

    #[test]
    fn invalid_theme_file_returns_error() {
        let path = std::env::temp_dir().join("netherize_invalid_theme.toml");
        std::fs::write(
            &path,
            r##"
[editor]
bg = "#111111"
fg = "#eeeeee"
cursor = "#ffffff"
selection = "#333333"
gutter = "#222222"

[ui]
sidebar_bg = "#111111"
panel_bg = "#111111"
status_bar_bg = "#111111"
border_color = "#111111"

[syntax]
keyword = "not-a-hex"
string = "#00ff00"
function = "#00ff00"
comment = "#00ff00"
type = "#00ff00"
number = "#00ff00"
constant = "#00ff00"
operator = "#00ff00"
variable = "#00ff00"
"##,
        )
        .expect("write invalid theme");

        let result = ThemeConfig::load_from_path(&path);
        assert!(result.is_err());

        let _ = std::fs::remove_file(path);
    }
}
