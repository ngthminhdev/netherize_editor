use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::config::paths::user_config_root;
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

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Windowed => "windowed",
            Self::Maximized => "maximized",
            Self::Fullscreen => "fullscreen",
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
    pub scale_factor_override: Option<f32>,
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
    pub topbar_dirty_gap: f32,
}

#[derive(Debug, Clone)]
pub struct EditorUiConfig {
    pub relative_numbers: bool,
    pub font_family: Option<String>,
    pub font_size: f32,
    pub line_height: f32,
    pub smooth_scroll_enabled: bool,
    pub smooth_scroll_lerp_rate: f32,
    pub smooth_scroll_snap_epsilon: f32,
    /// Duration of the fixed-length ease-out scroll tween, in milliseconds.
    pub smooth_scroll_duration_ms: u32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AnimationConfig {
    pub enabled: bool,
    pub dock_duration_ms: u32,
    pub overlay_duration_ms: u32,
    pub curve: crate::workbench::motion::EaseCurve,
}

impl Default for AnimationConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            dock_duration_ms: 150,
            overlay_duration_ms: 110,
            curve: crate::workbench::motion::EaseCurve::EaseOutCubic,
        }
    }
}

impl AnimationConfig {
    pub fn dock_duration(&self) -> std::time::Duration {
        std::time::Duration::from_millis(self.dock_duration_ms as u64)
    }

    pub fn overlay_duration(&self) -> std::time::Duration {
        std::time::Duration::from_millis(self.overlay_duration_ms as u64)
    }
}

/// Neovide-style motion settings. `[motion]` is the home for editor smooth
/// scrolling; `enabled` is the master gate. The legacy `[editor].smooth_scroll_*`
/// keys are mapped in as fallbacks (see `from_raw`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MotionConfig {
    pub enabled: bool,
    pub duration_ms: u32,
    pub ease: crate::workbench::motion::EaseCurve,
    pub editor_smooth_scroll_enabled: bool,
    /// Master on/off + legacy fallback for the per-distance durations. `0` disables
    /// editor smooth scroll entirely (see `editor_smooth_scroll_active`).
    pub editor_smooth_scroll_animation_ms: u32,
    pub editor_smooth_scroll_far_lines: u32,
    /// Distance-scaled tween durations (ms). Short = j/k edge follow (≤3 lines),
    /// halfpage = Ctrl-D/U (≤24 lines), center = zz/gg/G recenter (further).
    pub editor_scroll_step_ms: u32,
    pub editor_scroll_halfpage_ms: u32,
    pub editor_scroll_center_ms: u32,
}

impl Default for MotionConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            duration_ms: 250,
            ease: crate::workbench::motion::EaseCurve::EaseOutCubic,
            editor_smooth_scroll_enabled: true,
            editor_smooth_scroll_animation_ms: 300,
            editor_smooth_scroll_far_lines: 1,
            editor_scroll_step_ms: 80,
            editor_scroll_halfpage_ms: 120,
            editor_scroll_center_ms: 130,
        }
    }
}

impl MotionConfig {
    /// Editor smooth scroll runs only when the master gate, the editor toggle,
    /// and a non-zero animation length all agree. Any one being off → snap.
    pub fn editor_smooth_scroll_active(&self) -> bool {
        self.enabled
            && self.editor_smooth_scroll_enabled
            && self.editor_smooth_scroll_animation_ms > 0
    }

    pub fn editor_scroll_duration(&self) -> std::time::Duration {
        std::time::Duration::from_millis(self.editor_smooth_scroll_animation_ms as u64)
    }

    /// Distance-scaled tween duration for the editor viewport, picking the
    /// step/halfpage/center bucket by the (post-clamp) visual line distance.
    pub fn scroll_duration_for(&self, animated_lines: f32) -> std::time::Duration {
        crate::workbench::motion::scroll_duration_for_distance(
            animated_lines,
            self.editor_scroll_step_ms,
            self.editor_scroll_halfpage_ms,
            self.editor_scroll_center_ms,
        )
    }
}

#[derive(Debug, Clone, Copy)]
pub struct IndentConfig {
    pub tab_width: u8,
    pub insert_spaces: bool,
}

impl Default for IndentConfig {
    fn default() -> Self {
        Self {
            tab_width: 4,
            insert_spaces: true,
        }
    }
}

#[derive(Debug, Clone)]
pub struct WelcomeUiConfig {
    pub version: String,
    pub card_max_width: f32,
    pub card_padding_x: f32,
    pub card_padding_y: f32,
    pub section_gap: f32,
    pub border_radius_px: f32,
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
    pub welcome: WelcomeUiConfig,
    pub indent: IndentConfig,
    pub animation: AnimationConfig,
    pub motion: MotionConfig,
    pub border_radius_px: f32,
    pub enable_outline: bool,
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
                config.with_user_overrides()
            }
            Err(err) => {
                eprintln!("[ui] {err}");
                eprintln!("[ui] falling back to built-in UI defaults");
                Self::builtin().with_user_overrides()
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
                max_content_scale: 1.0,
                scale_factor_override: None,
            },
            layout: WorkbenchLayoutConfig {
                outer_gap: 14.0,
                panel_gap: 14.0,
                inner_padding: 12.0,
                round_ui: true,
                top_bar_height: 34.0,
                status_bar_height: 22.0,
                center_min_width: 320.0,
                center_min_height: 180.0,
                sidebar_min_width: 180.0,
                bottom_min_height: 120.0,
                panel_border_width: 1.0,
            },
            docks: DockUiConfig {
                left: DockSectionConfig {
                    visible: false,
                    size_px: 280.0,
                },
                right: DockSectionConfig {
                    visible: false,
                    size_px: 650.0,
                },
                bottom: DockSectionConfig {
                    visible: false,
                    size_px: 420.0,
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
                topbar_dirty_gap: 6.0,
            },
            status_bar: StatusBarUiConfig {
                padding_x: 14.0,
                font_size: 11.0,
                line_height: 14.0,
            },
            editor: EditorUiConfig {
                relative_numbers: false,
                font_family: None,
                font_size: 14.0,
                line_height: 20.0,
                smooth_scroll_enabled: true,
                smooth_scroll_lerp_rate: 18.0,
                smooth_scroll_snap_epsilon: 0.01,
                smooth_scroll_duration_ms: 200,
            },
            welcome: WelcomeUiConfig {
                version: crate::APP_VERSION.to_string(),
                card_max_width: 60.0,
                card_padding_x: 42.0,
                card_padding_y: 34.0,
                section_gap: 16.0,
                border_radius_px: 18.0,
            },
            indent: IndentConfig::default(),
            animation: AnimationConfig::default(),
            motion: MotionConfig::default(),
            border_radius_px: 10.0,
            enable_outline: true,
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
                scale_factor_override: if let Some(val) = raw.window.scale_factor_override {
                    Some(parse_positive_f32("window", "scale_factor_override", val)?)
                } else {
                    None
                },
            },
            layout: WorkbenchLayoutConfig {
                outer_gap: parse_non_negative_f32(
                    "layout",
                    "outer_gap",
                    raw.layout.outer_gap.unwrap_or(fallback.layout.outer_gap),
                )?,
                panel_gap: parse_non_negative_f32(
                    "layout",
                    "panel_gap",
                    raw.layout.panel_gap.unwrap_or(fallback.layout.panel_gap),
                )?,
                inner_padding: parse_non_negative_f32(
                    "layout",
                    "inner_padding",
                    raw.layout
                        .inner_padding
                        .unwrap_or(fallback.layout.inner_padding),
                )?,
                round_ui: raw.layout.round_ui.unwrap_or(fallback.layout.round_ui),
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
                panel_border_width: parse_non_negative_f32(
                    "layout",
                    "panel_border_width",
                    raw.layout
                        .panel_border_width
                        .unwrap_or(fallback.layout.panel_border_width),
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
                topbar_dirty_gap: parse_positive_f32(
                    "spacing",
                    "topbar_dirty_gap",
                    raw.spacing
                        .topbar_dirty_gap
                        .unwrap_or(fallback.spacing.topbar_dirty_gap),
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
                font_family: raw.editor.font_family,
                font_size: parse_positive_f32(
                    "editor",
                    "font_size",
                    raw.editor.font_size.unwrap_or(fallback.editor.font_size),
                )?,
                line_height: parse_positive_f32(
                    "editor",
                    "line_height",
                    raw.editor
                        .line_height
                        .unwrap_or(fallback.editor.line_height),
                )?,
                smooth_scroll_enabled: raw
                    .editor
                    .smooth_scroll_enabled
                    .unwrap_or(fallback.editor.smooth_scroll_enabled),
                smooth_scroll_lerp_rate: parse_positive_f32(
                    "editor",
                    "smooth_scroll_lerp_rate",
                    raw.editor
                        .smooth_scroll_lerp_rate
                        .unwrap_or(fallback.editor.smooth_scroll_lerp_rate),
                )?,
                smooth_scroll_snap_epsilon: parse_positive_f32(
                    "editor",
                    "smooth_scroll_snap_epsilon",
                    raw.editor
                        .smooth_scroll_snap_epsilon
                        .unwrap_or(fallback.editor.smooth_scroll_snap_epsilon),
                )?,
                smooth_scroll_duration_ms: raw
                    .editor
                    .smooth_scroll_duration_ms
                    .unwrap_or(fallback.editor.smooth_scroll_duration_ms),
            },
            welcome: WelcomeUiConfig {
                version: raw
                    .welcome
                    .version
                    .unwrap_or_else(|| fallback.welcome.version.clone()),
                card_max_width: parse_positive_f32(
                    "welcome",
                    "card_max_width",
                    raw.welcome
                        .card_max_width
                        .unwrap_or(fallback.welcome.card_max_width),
                )?,
                card_padding_x: parse_positive_f32(
                    "welcome",
                    "card_padding_x",
                    raw.welcome
                        .card_padding_x
                        .unwrap_or(fallback.welcome.card_padding_x),
                )?,
                card_padding_y: parse_positive_f32(
                    "welcome",
                    "card_padding_y",
                    raw.welcome
                        .card_padding_y
                        .unwrap_or(fallback.welcome.card_padding_y),
                )?,
                section_gap: parse_positive_f32(
                    "welcome",
                    "section_gap",
                    raw.welcome
                        .section_gap
                        .unwrap_or(fallback.welcome.section_gap),
                )?,
                border_radius_px: raw
                    .welcome
                    .border_radius_px
                    .unwrap_or(fallback.welcome.border_radius_px)
                    .max(0.0),
            },
            indent: IndentConfig {
                tab_width: raw
                    .indent
                    .tab_width
                    .unwrap_or(fallback.indent.tab_width)
                    .max(1),
                insert_spaces: raw
                    .indent
                    .insert_spaces
                    .unwrap_or(fallback.indent.insert_spaces),
            },
            animation: {
                let fb = fallback.animation;
                AnimationConfig {
                    enabled: raw.animation.enabled.unwrap_or(fb.enabled),
                    dock_duration_ms: raw
                        .animation
                        .dock_duration_ms
                        .unwrap_or(fb.dock_duration_ms),
                    overlay_duration_ms: raw
                        .animation
                        .overlay_duration_ms
                        .unwrap_or(fb.overlay_duration_ms),
                    curve: raw
                        .animation
                        .curve
                        .map(|s| crate::workbench::motion::EaseCurve::from_str_or_default(&s))
                        .unwrap_or(fb.curve),
                }
            },
            motion: {
                let fb = MotionConfig::default();
                MotionConfig {
                    enabled: raw.motion.enabled.unwrap_or(fb.enabled),
                    duration_ms: raw.motion.duration_ms.unwrap_or(fb.duration_ms),
                    ease: raw
                        .motion
                        .ease
                        .map(|s| crate::workbench::motion::EaseCurve::from_str_or_default(&s))
                        .unwrap_or(fb.ease),
                    // Fall back to the legacy `[editor].smooth_scroll_*` keys when the
                    // new `[motion]` keys are absent, so existing configs keep working.
                    editor_smooth_scroll_enabled: raw
                        .motion
                        .editor_smooth_scroll_enabled
                        .or(raw.editor.smooth_scroll_enabled)
                        .unwrap_or(fb.editor_smooth_scroll_enabled),
                    editor_smooth_scroll_animation_ms: raw
                        .motion
                        .editor_smooth_scroll_animation_ms
                        .or(raw.editor.smooth_scroll_duration_ms)
                        .unwrap_or(fb.editor_smooth_scroll_animation_ms),
                    editor_smooth_scroll_far_lines: raw
                        .motion
                        .editor_smooth_scroll_far_lines
                        .unwrap_or(fb.editor_smooth_scroll_far_lines),
                    // Per-distance durations fall back to the single legacy
                    // `editor_smooth_scroll_animation_ms` when unset, so old configs
                    // (and the `[editor]` legacy key) still drive every bucket.
                    editor_scroll_step_ms: raw
                        .motion
                        .editor_scroll_step_ms
                        .or(raw.motion.editor_smooth_scroll_animation_ms)
                        .or(raw.editor.smooth_scroll_duration_ms)
                        .unwrap_or(fb.editor_scroll_step_ms),
                    editor_scroll_halfpage_ms: raw
                        .motion
                        .editor_scroll_halfpage_ms
                        .or(raw.motion.editor_smooth_scroll_animation_ms)
                        .or(raw.editor.smooth_scroll_duration_ms)
                        .unwrap_or(fb.editor_scroll_halfpage_ms),
                    editor_scroll_center_ms: raw
                        .motion
                        .editor_scroll_center_ms
                        .or(raw.motion.editor_smooth_scroll_animation_ms)
                        .or(raw.editor.smooth_scroll_duration_ms)
                        .unwrap_or(fb.editor_scroll_center_ms),
                }
            },
            border_radius_px: raw
                .border_radius_px
                .unwrap_or(if fallback.layout.round_ui {
                    fallback.border_radius_px
                } else {
                    0.0
                })
                .max(0.0),
            enable_outline: raw.enable_outline.unwrap_or(fallback.enable_outline),
        };
        config.validate()?;
        Ok(config)
    }

    fn with_user_overrides(mut self) -> Self {
        let path = Self::user_override_path();
        let Ok(content) = std::fs::read_to_string(&path) else {
            return self;
        };
        let Ok(raw) = toml::from_str::<RawUiFile>(&content) else {
            return self;
        };
        // Apply only explicitly-set editor font fields directly from raw (not through
        // from_raw, which fills missing fields with fallback values and would incorrectly
        // overwrite the profile defaults with builtin defaults).
        if let Some(fs) = raw.editor.font_size {
            self.editor.font_size = fs.clamp(8.0, 40.0);
        }
        if let Some(lh) = raw.editor.line_height {
            self.editor.line_height = lh.clamp(10.0, 64.0);
        }
        if raw.editor.font_family.is_some() {
            self.editor.font_family = raw.editor.font_family.clone();
        }
        if let Some(over) = raw.window.scale_factor_override {
            self.window.scale_factor_override = Some(over.max(0.1));
        }
        if let Some(auto) = raw.window.auto_scale {
            self.window.auto_scale = auto;
        }
        if let Some(min) = raw.window.min_content_scale {
            self.window.min_content_scale = min;
        }
        if let Some(max) = raw.window.max_content_scale {
            self.window.max_content_scale = max;
        }
        if let Some(w) = raw.window.width {
            self.window.width = w;
        }
        if let Some(h) = raw.window.height {
            self.window.height = h;
        }
        if let Some(ref title) = raw.window.title {
            self.window.title = title.clone();
        }
        if let Some(ref mode) = raw.window.startup_mode {
            if let Some(parsed) = WindowStartupMode::parse(mode) {
                self.window.startup_mode = parsed;
            }
        }
        if let Ok(override_config) = Self::from_raw(raw) {
            self.docks = override_config.docks;
            self.indent = override_config.indent;
            self.border_radius_px = override_config.border_radius_px;
            self.enable_outline = override_config.enable_outline;
        }
        self
    }

    /// Returns the editor fields that are **explicitly** set in the user override file.
    /// Used at startup to sync `base_theme` without affecting fresh installs that have
    /// no ui.toml (where the theme file is the correct source of truth).
    pub fn load_user_editor_overrides() -> (Option<f32>, Option<f32>, Option<String>) {
        let path = Self::user_override_path();
        let Ok(content) = std::fs::read_to_string(&path) else {
            return (None, None, None);
        };
        let Ok(raw) = toml::from_str::<RawUiFile>(&content) else {
            return (None, None, None);
        };
        (
            raw.editor.font_size.map(|v| v.clamp(8.0, 40.0)),
            raw.editor.line_height.map(|v| v.clamp(10.0, 64.0)),
            raw.editor.font_family,
        )
    }

    pub fn user_override_path() -> PathBuf {
        user_config_root().join("ui.toml")
    }

    pub fn save_user_override(&self) -> Result<(), String> {
        let path = Self::user_override_path();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|err| format!("create ui config dir failed: {err}"))?;
        }
        let raw = UserUiConfigFile::from(self);
        let text = toml::to_string_pretty(&raw)
            .map_err(|err| format!("serialize ui config failed: {err}"))?;
        crate::app::persistence::atomic_write(&path, text)
            .map_err(|err| format!("write ui config failed: {err}"))
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

fn parse_non_negative_f32(section: &str, token: &str, value: f32) -> Result<f32, String> {
    if value >= 0.0 {
        Ok(value)
    } else {
        Err(format!("{section}.{token}: expected >= 0, got {value}"))
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

    let user_path = user_config_root().join("config").join("ui").join(&filename);
    if user_path.exists() {
        return Some(user_path);
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
    #[serde(default)]
    welcome: RawWelcome,
    #[serde(default)]
    indent: RawIndent,
    #[serde(default)]
    animation: RawAnimation,
    #[serde(default)]
    motion: RawMotion,
    border_radius_px: Option<f32>,
    enable_outline: Option<bool>,
}

#[derive(Debug, Default, Deserialize)]
struct RawAnimation {
    enabled: Option<bool>,
    dock_duration_ms: Option<u32>,
    overlay_duration_ms: Option<u32>,
    curve: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
struct RawMotion {
    enabled: Option<bool>,
    duration_ms: Option<u32>,
    ease: Option<String>,
    editor_smooth_scroll_enabled: Option<bool>,
    editor_smooth_scroll_animation_ms: Option<u32>,
    editor_smooth_scroll_far_lines: Option<u32>,
    editor_scroll_step_ms: Option<u32>,
    editor_scroll_halfpage_ms: Option<u32>,
    editor_scroll_center_ms: Option<u32>,
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
    scale_factor_override: Option<f32>,
}

#[derive(Debug, Default, Deserialize)]
struct RawLayout {
    outer_gap: Option<f32>,
    panel_gap: Option<f32>,
    inner_padding: Option<f32>,
    round_ui: Option<bool>,
    top_bar_height: Option<f32>,
    status_bar_height: Option<f32>,
    center_min_width: Option<f32>,
    center_min_height: Option<f32>,
    sidebar_min_width: Option<f32>,
    bottom_min_height: Option<f32>,
    panel_border_width: Option<f32>,
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
    topbar_dirty_gap: Option<f32>,
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
    font_family: Option<String>,
    font_size: Option<f32>,
    line_height: Option<f32>,
    smooth_scroll_enabled: Option<bool>,
    smooth_scroll_lerp_rate: Option<f32>,
    smooth_scroll_snap_epsilon: Option<f32>,
    smooth_scroll_duration_ms: Option<u32>,
}

#[derive(Debug, Default, Deserialize)]
struct RawIndent {
    tab_width: Option<u8>,
    insert_spaces: Option<bool>,
}

#[derive(Debug, Default, Deserialize)]
struct RawWelcome {
    version: Option<String>,
    card_max_width: Option<f32>,
    card_padding_x: Option<f32>,
    card_padding_y: Option<f32>,
    section_gap: Option<f32>,
    border_radius_px: Option<f32>,
}

#[derive(Debug, Serialize)]
struct UserUiConfigFile {
    window: UserUiWindow,
    docks: UserUiDocks,
    editor: UserUiEditor,
    indent: UserUiIndent,
    border_radius_px: f32,
    enable_outline: bool,
}

#[derive(Debug, Serialize)]
struct UserUiWindow {
    width: u32,
    height: u32,
    title: String,
    startup_mode: String,
    auto_scale: bool,
    min_content_scale: f32,
    max_content_scale: f32,
    #[serde(skip_serializing_if = "Option::is_none")]
    scale_factor_override: Option<f32>,
}

#[derive(Debug, Serialize)]
struct UserUiIndent {
    tab_width: u8,
    insert_spaces: bool,
}

#[derive(Debug, Serialize)]
struct UserUiDocks {
    left_visible: bool,
    left_size_px: f32,
    right_visible: bool,
    right_size_px: f32,
    bottom_visible: bool,
    bottom_size_px: f32,
    overlay_visible: bool,
}

#[derive(Debug, Serialize)]
struct UserUiEditor {
    relative_numbers: bool,
    font_family: Option<String>,
    font_size: f32,
    line_height: f32,
}

impl From<&UiConfig> for UserUiConfigFile {
    fn from(value: &UiConfig) -> Self {
        Self {
            window: UserUiWindow {
                width: value.window.width,
                height: value.window.height,
                title: value.window.title.clone(),
                startup_mode: value.window.startup_mode.as_str().to_string(),
                auto_scale: value.window.auto_scale,
                min_content_scale: value.window.min_content_scale,
                max_content_scale: value.window.max_content_scale,
                scale_factor_override: value.window.scale_factor_override,
            },
            docks: UserUiDocks {
                left_visible: value.docks.left.visible,
                left_size_px: value.docks.left.size_px,
                right_visible: value.docks.right.visible,
                right_size_px: value.docks.right.size_px,
                bottom_visible: value.docks.bottom.visible,
                bottom_size_px: value.docks.bottom.size_px,
                overlay_visible: value.docks.overlay_visible,
            },
            editor: UserUiEditor {
                relative_numbers: value.editor.relative_numbers,
                font_family: value.editor.font_family.clone(),
                font_size: value.editor.font_size,
                line_height: value.editor.line_height,
            },
            indent: UserUiIndent {
                tab_width: value.indent.tab_width,
                insert_spaces: value.indent.insert_spaces,
            },
            border_radius_px: value.border_radius_px,
            enable_outline: value.enable_outline,
        }
    }
}

#[cfg(test)]
mod animation_config_tests {
    use super::*;
    use crate::workbench::motion::EaseCurve;

    #[test]
    fn animation_config_defaults() {
        let cfg = UiConfig::builtin();
        assert!(cfg.animation.enabled);
        assert_eq!(cfg.animation.dock_duration_ms, 150);
        assert_eq!(cfg.animation.overlay_duration_ms, 110);
        assert_eq!(cfg.animation.curve, EaseCurve::EaseOutCubic);
    }

    #[test]
    fn animation_config_parses_overrides() {
        let toml_src = r#"
            [animation]
            enabled = false
            dock_duration_ms = 200
            overlay_duration_ms = 90
            curve = "linear"
        "#;
        let raw: RawUiFile = toml::from_str(toml_src).unwrap();
        let cfg = UiConfig::from_raw(raw).unwrap();
        assert!(!cfg.animation.enabled);
        assert_eq!(cfg.animation.dock_duration_ms, 200);
        assert_eq!(cfg.animation.overlay_duration_ms, 90);
        assert_eq!(cfg.animation.curve, EaseCurve::Linear);
    }

    #[test]
    fn animation_config_missing_block_uses_fallback() {
        let raw: RawUiFile = toml::from_str("").unwrap();
        let cfg = UiConfig::from_raw(raw).unwrap();
        assert!(cfg.animation.enabled);
        assert_eq!(cfg.animation.curve, EaseCurve::EaseOutCubic);
    }

    #[test]
    fn motion_config_defaults() {
        let cfg = UiConfig::builtin();
        assert!(cfg.motion.enabled);
        assert_eq!(cfg.motion.editor_smooth_scroll_animation_ms, 300);
        assert_eq!(cfg.motion.editor_smooth_scroll_far_lines, 1);
        assert_eq!(cfg.motion.ease, EaseCurve::EaseOutCubic);
        assert!(cfg.motion.editor_smooth_scroll_active());
    }

    #[test]
    fn motion_disable_paths_each_turn_off_active() {
        let mut m = UiConfig::builtin().motion;
        m.enabled = false;
        assert!(!m.editor_smooth_scroll_active());
        let mut m = UiConfig::builtin().motion;
        m.editor_smooth_scroll_enabled = false;
        assert!(!m.editor_smooth_scroll_active());
        let mut m = UiConfig::builtin().motion;
        m.editor_smooth_scroll_animation_ms = 0;
        assert!(!m.editor_smooth_scroll_active());
    }

    #[test]
    fn motion_parses_overrides_and_bad_ease_falls_back() {
        let toml_src = r#"
            [motion]
            enabled = true
            ease = "garbage"
            editor_smooth_scroll_animation_ms = 120
            editor_smooth_scroll_far_lines = 4
        "#;
        let raw: RawUiFile = toml::from_str(toml_src).unwrap();
        let cfg = UiConfig::from_raw(raw).unwrap();
        assert_eq!(cfg.motion.ease, EaseCurve::EaseOutCubic);
        assert_eq!(cfg.motion.editor_smooth_scroll_animation_ms, 120);
        assert_eq!(cfg.motion.editor_smooth_scroll_far_lines, 4);
    }

    #[test]
    fn motion_distance_durations_default() {
        let m = UiConfig::builtin().motion;
        assert_eq!(m.editor_scroll_step_ms, 80);
        assert_eq!(m.editor_scroll_halfpage_ms, 120);
        assert_eq!(m.editor_scroll_center_ms, 130);
    }

    #[test]
    fn motion_distance_durations_parse_override() {
        let toml_src = r#"
            [motion]
            editor_scroll_step_ms = 60
            editor_scroll_halfpage_ms = 100
            editor_scroll_center_ms = 110
        "#;
        let raw: RawUiFile = toml::from_str(toml_src).unwrap();
        let cfg = UiConfig::from_raw(raw).unwrap();
        assert_eq!(cfg.motion.editor_scroll_step_ms, 60);
        assert_eq!(cfg.motion.editor_scroll_halfpage_ms, 100);
        assert_eq!(cfg.motion.editor_scroll_center_ms, 110);
    }

    #[test]
    fn motion_distance_durations_fall_back_to_legacy_animation_ms() {
        // A config that only sets the single legacy duration drives every bucket.
        let toml_src = r#"
            [motion]
            editor_smooth_scroll_animation_ms = 90
        "#;
        let raw: RawUiFile = toml::from_str(toml_src).unwrap();
        let cfg = UiConfig::from_raw(raw).unwrap();
        assert_eq!(cfg.motion.editor_scroll_step_ms, 90);
        assert_eq!(cfg.motion.editor_scroll_halfpage_ms, 90);
        assert_eq!(cfg.motion.editor_scroll_center_ms, 90);
    }

    #[test]
    fn motion_scroll_duration_for_buckets() {
        let m = UiConfig::builtin().motion;
        assert_eq!(
            m.scroll_duration_for(1.0),
            std::time::Duration::from_millis(80)
        );
        assert_eq!(
            m.scroll_duration_for(15.0),
            std::time::Duration::from_millis(120)
        );
        assert_eq!(
            m.scroll_duration_for(120.0),
            std::time::Duration::from_millis(130)
        );
    }

    #[test]
    fn motion_back_compat_maps_legacy_editor_keys() {
        let toml_src = r#"
            [editor]
            smooth_scroll_enabled = false
            smooth_scroll_duration_ms = 90
        "#;
        let raw: RawUiFile = toml::from_str(toml_src).unwrap();
        let cfg = UiConfig::from_raw(raw).unwrap();
        assert!(!cfg.motion.editor_smooth_scroll_enabled);
        assert_eq!(cfg.motion.editor_smooth_scroll_animation_ms, 90);
    }

    #[test]
    fn motion_missing_block_uses_defaults() {
        let raw: RawUiFile = toml::from_str("").unwrap();
        let cfg = UiConfig::from_raw(raw).unwrap();
        assert_eq!(cfg.motion.editor_smooth_scroll_animation_ms, 300);
        assert!(cfg.motion.editor_smooth_scroll_active());
    }
}
