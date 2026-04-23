use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ThemeColor {
    rgba_u8: [u8; 4],
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LinearColor {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

impl LinearColor {
    pub const fn new(r: f32, g: f32, b: f32, a: f32) -> Self {
        Self { r, g, b, a }
    }

    pub const fn as_array(self) -> [f32; 4] {
        [self.r, self.g, self.b, self.a]
    }

    pub fn to_wgpu(self) -> wgpu::Color {
        wgpu::Color {
            r: f64::from(self.r),
            g: f64::from(self.g),
            b: f64::from(self.b),
            a: f64::from(self.a),
        }
    }
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

    pub fn as_srgb_f32(self) -> [f32; 4] {
        [
            f32::from(self.rgba_u8[0]) / 255.0,
            f32::from(self.rgba_u8[1]) / 255.0,
            f32::from(self.rgba_u8[2]) / 255.0,
            f32::from(self.rgba_u8[3]) / 255.0,
        ]
    }

    pub fn as_linear(self) -> LinearColor {
        let [r, g, b, a] = self.as_srgb_f32();
        LinearColor::new(srgb_to_linear(r), srgb_to_linear(g), srgb_to_linear(b), a)
    }

    pub fn as_f32(self) -> [f32; 4] {
        self.as_linear().as_array()
    }
}

pub fn srgb_to_linear(channel: f32) -> f32 {
    let value = channel.clamp(0.0, 1.0);
    if value <= 0.04045 {
        value / 12.92
    } else {
        ((value + 0.055) / 1.055).powf(2.4)
    }
}

pub fn linear_to_srgb(channel: f32) -> f32 {
    let value = channel.clamp(0.0, 1.0);
    if value <= 0.003_130_8 {
        value * 12.92
    } else {
        1.055 * value.powf(1.0 / 2.4) - 0.055
    }
}

pub fn srgb_rgba_to_linear_f32(rgba: [u8; 4]) -> [f32; 4] {
    [
        srgb_to_linear(f32::from(rgba[0]) / 255.0),
        srgb_to_linear(f32::from(rgba[1]) / 255.0),
        srgb_to_linear(f32::from(rgba[2]) / 255.0),
        f32::from(rgba[3]) / 255.0,
    ]
}

pub fn linear_rgba_to_srgb_u8(rgba: [f32; 4]) -> [u8; 4] {
    [
        f32_channel_to_u8(linear_to_srgb(rgba[0])),
        f32_channel_to_u8(linear_to_srgb(rgba[1])),
        f32_channel_to_u8(linear_to_srgb(rgba[2])),
        f32_channel_to_u8(rgba[3]),
    ]
}

fn parse_hex_component(component: &str, original: &str) -> Result<u8, String> {
    u8::from_str_radix(component, 16)
        .map_err(|_| format!("invalid hex color '{original}' (bad component '{component}')"))
}

fn f32_channel_to_u8(channel: f32) -> u8 {
    let clamped = channel.clamp(0.0, 1.0);
    (clamped * 255.0).round() as u8
}

#[derive(Debug, Clone)]
pub struct ThemeConfig {
    pub name: String,
    pub description: Option<String>,
    pub editor: EditorThemeTokens,
    pub ui: UiThemeTokens,
    pub syntax: SyntaxThemeTokens,
    pub icons: IconThemeTokens,
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
    pub font_family: Option<String>,
    pub nerd_font_family: Option<String>,
}

#[derive(Debug, Clone)]
pub struct UiThemeTokens {
    pub bg: ThemeColor,
    pub sidebar_bg: ThemeColor,
    pub panel_bg: ThemeColor,
    pub terminal_bg: ThemeColor,
    pub overlay_bg: ThemeColor,
    pub status_bar_bg: ThemeColor,
    pub border_color: ThemeColor,
    pub selection_bg: ThemeColor,
    pub fg: ThemeColor,
    pub fg_dim: ThemeColor,
    pub fg_ghost: ThemeColor,
    pub accent: ThemeColor,
    pub cyan: ThemeColor,
    pub magenta: ThemeColor,
    pub amber: ThemeColor,
    pub success: ThemeColor,
    pub warning: ThemeColor,
    pub info: ThemeColor,
    pub error: ThemeColor,
    pub mode_normal: ThemeColor,
    pub mode_insert: ThemeColor,
    pub mode_visual: ThemeColor,
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
    pub identifier: ThemeColor,
    pub parameter: ThemeColor,
    pub field: ThemeColor,
    pub property: ThemeColor,
    pub constant: ThemeColor,
    pub operator: ThemeColor,
    pub punctuation: ThemeColor,
    pub r#macro: ThemeColor,
    pub lifetime: ThemeColor,
}

#[derive(Debug, Clone)]
pub struct FileIconThemeTokens {
    pub glyph: String,
    pub color: ThemeColor,
}

#[derive(Debug, Clone)]
pub struct IconThemeTokens {
    pub explorer_file_marker: String,
    pub explorer_folder_collapsed_marker: String,
    pub explorer_folder_expanded_marker: String,
    pub file_picker_dot: String,
    pub folder_closed: FileIconThemeTokens,
    pub folder_open: FileIconThemeTokens,
    pub default_file: FileIconThemeTokens,
    pub rust: FileIconThemeTokens,
    pub javascript: FileIconThemeTokens,
    pub typescript: FileIconThemeTokens,
    pub tsx: FileIconThemeTokens,
    pub jsx: FileIconThemeTokens,
    pub python: FileIconThemeTokens,
    pub go: FileIconThemeTokens,
    pub config: FileIconThemeTokens,
    pub json: FileIconThemeTokens,
    pub markdown: FileIconThemeTokens,
    pub html: FileIconThemeTokens,
    pub css: FileIconThemeTokens,
    pub sass: FileIconThemeTokens,
    pub shell: FileIconThemeTokens,
    pub git: FileIconThemeTokens,
    pub lock: FileIconThemeTokens,
    pub image: FileIconThemeTokens,
}

impl ThemeConfig {
    pub fn sidebar_arrow(&self, is_dir: bool, is_expanded: bool) -> &str {
        if is_dir {
            if is_expanded {
                self.icons.explorer_folder_expanded_marker.as_str()
            } else {
                self.icons.explorer_folder_collapsed_marker.as_str()
            }
        } else {
            self.icons.explorer_file_marker.as_str()
        }
    }

    pub fn file_icon_for_path(
        &self,
        path: &Path,
        is_dir: bool,
        is_expanded: bool,
    ) -> &FileIconThemeTokens {
        if is_dir {
            if is_expanded {
                &self.icons.folder_open
            } else {
                &self.icons.folder_closed
            }
        } else {
            let extension = path.extension().and_then(|ext| ext.to_str()).unwrap_or("");
            self.file_icon_for_extension(extension)
        }
    }

    pub fn file_icon_for_extension(&self, ext: &str) -> &FileIconThemeTokens {
        match ext.to_ascii_lowercase().as_str() {
            "rs" => &self.icons.rust,
            "js" | "mjs" | "cjs" => &self.icons.javascript,
            "ts" => &self.icons.typescript,
            "tsx" => &self.icons.tsx,
            "jsx" => &self.icons.jsx,
            "py" | "pyw" => &self.icons.python,
            "go" => &self.icons.go,
            "toml" | "yaml" | "yml" | "ini" | "env" => &self.icons.config,
            "json" | "jsonc" => &self.icons.json,
            "md" | "mdx" | "markdown" => &self.icons.markdown,
            "html" | "htm" => &self.icons.html,
            "css" => &self.icons.css,
            "scss" | "sass" => &self.icons.sass,
            "sh" | "bash" | "zsh" | "fish" => &self.icons.shell,
            "gitignore" | "gitmodules" | "gitattributes" => &self.icons.git,
            "lock" => &self.icons.lock,
            "png" | "jpg" | "jpeg" | "gif" | "svg" | "webp" | "ico" => &self.icons.image,
            _ => &self.icons.default_file,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{ThemeColor, linear_rgba_to_srgb_u8, srgb_to_linear};

    #[test]
    fn parse_hex_color_supports_rgb_and_rgba() {
        let rgb = ThemeColor::from_hex("#AABBCC").expect("rgb hex should parse");
        assert_eq!(rgb.as_u8(), [0xAA, 0xBB, 0xCC, 0xFF]);

        let rgba = ThemeColor::from_hex("A1B2C3DD").expect("rgba hex should parse");
        assert_eq!(rgba.as_u8(), [0xA1, 0xB2, 0xC3, 0xDD]);
    }

    #[test]
    fn srgb_mid_gray_decodes_to_expected_linear_value() {
        let decoded = srgb_to_linear(0.5);
        assert!((decoded - 0.214_041_14).abs() < 1e-6, "{decoded}");
    }

    #[test]
    fn linear_round_trip_preserves_srgb_bytes() {
        let color = ThemeColor::from_hex("#8080C0CC").expect("theme color should parse");
        assert_eq!(linear_rgba_to_srgb_u8(color.as_f32()), color.as_u8());
    }
}
