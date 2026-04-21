use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::workbench::layout_engine::WorkbenchLayoutConfig;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CursorShape {
    Beam,
    Block,
    Underline,
}

impl CursorShape {
    fn parse(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "beam" | "bar" | "line" | "zed" => Some(Self::Beam),
            "block" | "box" | "nvim" => Some(Self::Block),
            "underline" | "under" => Some(Self::Underline),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindowStartupMode {
    Windowed,
    Maximized,
    Fullscreen,
}

impl WindowStartupMode {
    fn parse(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "windowed" | "window" | "normal" => Some(Self::Windowed),
            "maximized" | "maximize" | "max" => Some(Self::Maximized),
            "fullscreen" | "full" | "borderless" => Some(Self::Fullscreen),
            _ => None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct WindowUiConfig {
    pub width: u32,
    pub height: u32,
    pub title: String,
    pub startup_mode: WindowStartupMode,
    pub auto_scale: bool,
    pub min_content_scale: f32,
    pub max_content_scale: f32,
}

#[derive(Debug, Clone, Copy)]
pub struct DockSectionConfig {
    pub visible: bool,
    pub size_px: f32,
}

#[derive(Debug, Clone, Copy)]
pub struct DockUiConfig {
    pub left: DockSectionConfig,
    pub right: DockSectionConfig,
    pub bottom: DockSectionConfig,
    pub overlay_visible: bool,
}

#[derive(Debug, Clone, Copy)]
pub struct CursorUiConfig {
    pub shape: CursorShape,
    pub beam_width: f32,
    pub block_width: f32,
    pub underline_height: f32,
}

#[derive(Debug, Clone, Copy)]
pub struct StatusBarUiConfig {
    pub padding_x: f32,
    pub font_size: f32,
    pub line_height: f32,
}

#[derive(Debug, Clone, Copy)]
pub struct SpacingUiConfig {
    pub editor_padding: f32,
    pub panel_padding: f32,
    pub explorer_padding: f32,
}

#[derive(Debug, Clone, Copy)]
pub struct EditorUiConfig {
    pub relative_numbers: bool,
}

#[derive(Debug, Clone)]
pub struct UiConfig {
    pub window: WindowUiConfig,
    pub layout: WorkbenchLayoutConfig,
    pub docks: DockUiConfig,
    pub cursor: CursorUiConfig,
    pub spacing: SpacingUiConfig,
    pub status_bar: StatusBarUiConfig,
    pub editor: EditorUiConfig,
}

impl UiConfig {
    pub fn active_profile() -> String {
        std::env::var("NETHERIZE_UI").unwrap_or_else(|_| "default".into())
    }

    pub fn load_active() -> Self {
        let profile = Self::active_profile();
        match Self::load(&profile) {
            Ok(config) => {
                eprintln!("[ui] loaded profile '{profile}'");
                config
            }
            Err(err) => {
                eprintln!("[ui] {err}");
                eprintln!("[ui] falling back to built-in UI defaults");
                Self::builtin()
            }
        }
    }

    pub fn load(profile: &str) -> Result<Self, String> {
        let path = find_profile_path(profile)
            .ok_or_else(|| format!("ui profile '{profile}' not found under config/ui"))?;
        Self::load_from_path(&path)
    }

    pub fn load_from_path(path: &Path) -> Result<Self, String> {
        let content = std::fs::read_to_string(path)
            .map_err(|err| format!("cannot read ui file {}: {err}", path.display()))?;

        let raw: RawUiFile = toml::from_str(&content)
            .map_err(|err| format!("parse error in ui file {}: {err}", path.display()))?;
        Self::from_raw(raw).map_err(|err| format!("invalid ui file {}: {err}", path.display()))
    }

    pub fn builtin() -> Self {
        Self {
            window: WindowUiConfig {
                width: 1280,
                height: 800,
                title: "Netherize Editor".to_string(),
                startup_mode: WindowStartupMode::Maximized,
                auto_scale: true,
                min_content_scale: 1.0,
                max_content_scale: 2.0,
            },
            layout: WorkbenchLayoutConfig {
                region_gap: 2.0,
                top_bar_height: 38.0,
                status_bar_height: 36.0,
                center_min_width: 320.0,
                center_min_height: 180.0,
                sidebar_min_width: 180.0,
                bottom_min_height: 120.0,
            },
            docks: DockUiConfig {
                left: DockSectionConfig {
                    visible: false,
                    size_px: 280.0,
                },
                right: DockSectionConfig {
                    visible: false,
                    size_px: 320.0,
                },
                bottom: DockSectionConfig {
                    visible: false,
                    size_px: 230.0,
                },
                overlay_visible: false,
            },
            cursor: CursorUiConfig {
                shape: CursorShape::Block,
                beam_width: 1.8,
                block_width: 10.0,
                underline_height: 2.0,
            },
            spacing: SpacingUiConfig {
                editor_padding: 14.0,
                panel_padding: 10.0,
                explorer_padding: 10.0,
            },
            status_bar: StatusBarUiConfig {
                padding_x: 14.0,
                font_size: 15.0,
                line_height: 24.0,
            },
            editor: EditorUiConfig {
                relative_numbers: false,
            },
        }
    }

    fn from_raw(raw: RawUiFile) -> Result<Self, String> {
        let fallback = Self::builtin();
        let config = Self {
            window: WindowUiConfig {
                width: parse_positive_u32(
                    "window",
                    "width",
                    raw.window.width.unwrap_or(fallback.window.width),
                )?,
                height: parse_positive_u32(
                    "window",
                    "height",
                    raw.window.height.unwrap_or(fallback.window.height),
                )?,
                title: raw
                    .window
                    .title
                    .unwrap_or_else(|| fallback.window.title.clone()),
                startup_mode: raw
                    .window
                    .startup_mode
                    .as_deref()
                    .and_then(WindowStartupMode::parse)
                    .unwrap_or(fallback.window.startup_mode),
                auto_scale: raw.window.auto_scale.unwrap_or(fallback.window.auto_scale),
                min_content_scale: parse_positive_f32(
                    "window",
                    "min_content_scale",
                    raw.window
                        .min_content_scale
                        .unwrap_or(fallback.window.min_content_scale),
                )?,
                max_content_scale: parse_positive_f32(
                    "window",
                    "max_content_scale",
                    raw.window
                        .max_content_scale
                        .unwrap_or(fallback.window.max_content_scale),
                )?,
            },
            layout: WorkbenchLayoutConfig {
                region_gap: parse_positive_f32(
                    "layout",
                    "region_gap",
                    raw.layout.region_gap.unwrap_or(fallback.layout.region_gap),
                )?,
                top_bar_height: parse_positive_f32(
                    "layout",
                    "top_bar_height",
                    raw.layout
                        .top_bar_height
                        .unwrap_or(fallback.layout.top_bar_height),
                )?,
                status_bar_height: parse_positive_f32(
                    "layout",
                    "status_bar_height",
                    raw.layout
                        .status_bar_height
                        .unwrap_or(fallback.layout.status_bar_height),
                )?,
                center_min_width: parse_positive_f32(
                    "layout",
                    "center_min_width",
                    raw.layout
                        .center_min_width
                        .unwrap_or(fallback.layout.center_min_width),
                )?,
                center_min_height: parse_positive_f32(
                    "layout",
                    "center_min_height",
                    raw.layout
                        .center_min_height
                        .unwrap_or(fallback.layout.center_min_height),
                )?,
                sidebar_min_width: parse_positive_f32(
                    "layout",
                    "sidebar_min_width",
                    raw.layout
                        .sidebar_min_width
                        .unwrap_or(fallback.layout.sidebar_min_width),
                )?,
                bottom_min_height: parse_positive_f32(
                    "layout",
                    "bottom_min_height",
                    raw.layout
                        .bottom_min_height
                        .unwrap_or(fallback.layout.bottom_min_height),
                )?,
            },
            docks: DockUiConfig {
                left: DockSectionConfig {
                    visible: raw
                        .docks
                        .left_visible
                        .unwrap_or(fallback.docks.left.visible),
                    size_px: parse_positive_f32(
                        "docks",
                        "left_size_px",
                        raw.docks
                            .left_size_px
                            .unwrap_or(fallback.docks.left.size_px),
                    )?,
                },
                right: DockSectionConfig {
                    visible: raw
                        .docks
                        .right_visible
                        .unwrap_or(fallback.docks.right.visible),
                    size_px: parse_positive_f32(
                        "docks",
                        "right_size_px",
                        raw.docks
                            .right_size_px
                            .unwrap_or(fallback.docks.right.size_px),
                    )?,
                },
                bottom: DockSectionConfig {
                    visible: raw
                        .docks
                        .bottom_visible
                        .unwrap_or(fallback.docks.bottom.visible),
                    size_px: parse_positive_f32(
                        "docks",
                        "bottom_size_px",
                        raw.docks
                            .bottom_size_px
                            .unwrap_or(fallback.docks.bottom.size_px),
                    )?,
                },
                overlay_visible: raw
                    .docks
                    .overlay_visible
                    .unwrap_or(fallback.docks.overlay_visible),
            },
            cursor: CursorUiConfig {
                shape: raw
                    .cursor
                    .shape
                    .as_deref()
                    .and_then(CursorShape::parse)
                    .unwrap_or(fallback.cursor.shape),
                beam_width: parse_positive_f32(
                    "cursor",
                    "beam_width",
                    raw.cursor.beam_width.unwrap_or(fallback.cursor.beam_width),
                )?,
                block_width: parse_positive_f32(
                    "cursor",
                    "block_width",
                    raw.cursor
                        .block_width
                        .unwrap_or(fallback.cursor.block_width),
                )?,
                underline_height: parse_positive_f32(
                    "cursor",
                    "underline_height",
                    raw.cursor
                        .underline_height
                        .unwrap_or(fallback.cursor.underline_height),
                )?,
            },
            spacing: SpacingUiConfig {
                editor_padding: parse_positive_f32(
                    "spacing",
                    "editor_padding",
                    raw.spacing
                        .editor_padding
                        .unwrap_or(fallback.spacing.editor_padding),
                )?,
                panel_padding: parse_positive_f32(
                    "spacing",
                    "panel_padding",
                    raw.spacing
                        .panel_padding
                        .unwrap_or(fallback.spacing.panel_padding),
                )?,
                explorer_padding: parse_positive_f32(
                    "spacing",
                    "explorer_padding",
                    raw.spacing
                        .explorer_padding
                        .unwrap_or(fallback.spacing.explorer_padding),
                )?,
            },
            status_bar: StatusBarUiConfig {
                padding_x: parse_positive_f32(
                    "status_bar",
                    "padding_x",
                    raw.status_bar
                        .padding_x
                        .unwrap_or(fallback.status_bar.padding_x),
                )?,
                font_size: parse_positive_f32(
                    "status_bar",
                    "font_size",
                    raw.status_bar
                        .font_size
                        .unwrap_or(fallback.status_bar.font_size),
                )?,
                line_height: parse_positive_f32(
                    "status_bar",
                    "line_height",
                    raw.status_bar
                        .line_height
                        .unwrap_or(fallback.status_bar.line_height),
                )?,
            },
            editor: EditorUiConfig {
                relative_numbers: raw.editor.relative_numbers.unwrap_or(false),
            },
        };
        config.validate()?;
        Ok(config)
    }
}

impl UiConfig {
    pub fn validate(&self) -> Result<(), String> {
        if self.window.max_content_scale < self.window.min_content_scale {
            return Err(format!(
                "window.max_content_scale ({}) must be >= window.min_content_scale ({})",
                self.window.max_content_scale, self.window.min_content_scale
            ));
        }
        Ok(())
    }
}

fn parse_positive_f32(section: &str, token: &str, value: f32) -> Result<f32, String> {
    if value > 0.0 {
        Ok(value)
    } else {
        Err(format!("{section}.{token}: expected > 0, got {value}"))
    }
}

fn parse_positive_u32(section: &str, token: &str, value: u32) -> Result<u32, String> {
    if value > 0 {
        Ok(value)
    } else {
        Err(format!("{section}.{token}: expected > 0, got {value}"))
    }
}

fn find_profile_path(name: &str) -> Option<PathBuf> {
    let filename = format!("{name}.toml");

    if let Ok(cwd) = std::env::current_dir() {
        let path = cwd.join("config").join("ui").join(&filename);
        if path.exists() {
            return Some(path);
        }
    }

    if let Ok(exe) = std::env::current_exe()
        && let Some(parent) = exe.parent()
    {
        let path = parent.join("config").join("ui").join(&filename);
        if path.exists() {
            return Some(path);
        }
    }

    None
}

#[derive(Debug, Deserialize)]
struct RawUiFile {
    #[serde(default)]
    window: RawWindow,
    #[serde(default)]
    layout: RawLayout,
    #[serde(default)]
    docks: RawDocks,
    #[serde(default)]
    cursor: RawCursor,
    #[serde(default)]
    spacing: RawSpacing,
    #[serde(default)]
    status_bar: RawStatusBar,
    #[serde(default)]
    editor: RawEditorSection,
}

#[derive(Debug, Default, Deserialize)]
struct RawWindow {
    width: Option<u32>,
    height: Option<u32>,
    title: Option<String>,
    startup_mode: Option<String>,
    auto_scale: Option<bool>,
    min_content_scale: Option<f32>,
    max_content_scale: Option<f32>,
}

#[derive(Debug, Default, Deserialize)]
struct RawLayout {
    region_gap: Option<f32>,
    top_bar_height: Option<f32>,
    status_bar_height: Option<f32>,
    center_min_width: Option<f32>,
    center_min_height: Option<f32>,
    sidebar_min_width: Option<f32>,
    bottom_min_height: Option<f32>,
}

#[derive(Debug, Default, Deserialize)]
struct RawDocks {
    left_visible: Option<bool>,
    left_size_px: Option<f32>,
    right_visible: Option<bool>,
    right_size_px: Option<f32>,
    bottom_visible: Option<bool>,
    bottom_size_px: Option<f32>,
    overlay_visible: Option<bool>,
}

#[derive(Debug, Default, Deserialize)]
struct RawCursor {
    shape: Option<String>,
    beam_width: Option<f32>,
    block_width: Option<f32>,
    underline_height: Option<f32>,
}

#[derive(Debug, Default, Deserialize)]
struct RawSpacing {
    editor_padding: Option<f32>,
    panel_padding: Option<f32>,
    explorer_padding: Option<f32>,
}

#[derive(Debug, Default, Deserialize)]
struct RawStatusBar {
    padding_x: Option<f32>,
    font_size: Option<f32>,
    line_height: Option<f32>,
}

#[derive(Debug, Default, Deserialize)]
struct RawEditorSection {
    relative_numbers: Option<bool>,
}
