pub mod capabilities;
pub mod client;
pub mod registry;
pub mod symbol_cache;

pub use symbol_cache::{
    CachedSymbol, WorkspaceSymbolCache, extract_ts_js_exports_from_text,
    index_ts_js_workspace_exports,
};
