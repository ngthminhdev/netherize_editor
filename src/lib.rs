pub mod app;
pub mod async_runtime;
pub mod config;
pub mod core;
pub mod editor_core;
pub mod lsp;
pub mod render;
pub mod syntax;
pub mod terminal;
pub mod text;
pub mod workbench;
pub mod workspace;

/// Phiên bản hiển thị trong UI (Welcome screen, status bar…).
/// Đổi ở đây là xong — đồng bộ toàn bộ code.
/// Lưu ý: config/ui/default.toml cũng có `[welcome] version` cần cập nhật cùng.
pub const APP_VERSION: &str = "v1.0.2-alpha";
