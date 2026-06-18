//! Theme configuration entrypoint.
//!
//! Module split:
//! - `model`   — public theme token types used across the app
//! - `loader`  — TOML loading, validation, and profile resolution
//! - `raw`     — serde-only structs mirroring the TOML file shape
//! - `builtin` — built-in fallback theme when no profile can be loaded

mod builtin;
mod loader;
mod model;
mod raw;

pub use loader::ThemeProfileEntry;
pub use model::{
    EditorThemeTokens, FileIconThemeTokens, IconThemeTokens, LinearColor, SyntaxThemeTokens,
    ThemeColor, ThemeConfig, UiThemeTokens, derive_elevated_surface, linear_rgba_to_srgb_u8,
    srgb_rgba_to_linear_f32, srgb_to_linear,
};
