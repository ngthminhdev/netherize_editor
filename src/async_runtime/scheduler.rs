use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{
        Arc, Mutex, OnceLock,
        atomic::{AtomicU64, Ordering},
    },
    time::Duration,
};

use crate::lsp::client::LspClientProcess;
use crate::syntax::syntax_engine::{SyntaxEngine, LanguageId};
use crate::terminal::pty::PtyProcess;

mod ai;
mod ai_jobs;
mod dispatch;
mod emit;
mod file_watch;
mod fzf;
mod git;
mod lsp;
mod lsp_io;
mod lsp_parse;
mod pty;
mod runtime;
mod syntax_jobs;
#[cfg(test)]
mod tests;

pub use runtime::AsyncScheduler;

/// When a file is below both thresholds the async worker highlights the full
/// buffer (parse once, generate spans for everything).  Above the thresholds
/// it switches to viewport-only: only the visible + overscan window is parsed
/// and painted, avoiding O(file-size) per-frame work.
///
/// Matches INLINE_TREE_SITTER_* so the async path uses the same cutoff.
pub(super) const FULL_BUFFER_HIGHLIGHT_BYTE_THRESHOLD: usize = 32 * 1024;
pub(super) const FULL_BUFFER_HIGHLIGHT_LINE_THRESHOLD: usize = 300;
pub(super) const VIEWPORT_HIGHLIGHT_OVERSCAN_MULTIPLIER: usize = 3;
pub(super) const VIEWPORT_HIGHLIGHT_MIN_OVERSCAN_LINES: usize = 48;
pub(super) const FILE_WATCH_BATCH_WINDOW: Duration = Duration::from_millis(50);
pub(super) const LSP_HOVER_TIMEOUT_SECS: u64 = 10;
pub(super) const LSP_DEFINITION_TIMEOUT_SECS: u64 = 10;
pub(super) const LSP_REFERENCES_TIMEOUT_SECS: u64 = 15;
pub(super) const LSP_COMPLETION_TIMEOUT_SECS: u64 = 10;
pub(super) const LSP_COMPLETION_RESOLVE_TIMEOUT_SECS: u64 = 5;
pub(super) const LSP_FORMATTING_TIMEOUT_SECS: u64 = 15;
pub(super) const LSP_DOCUMENT_SYMBOLS_TIMEOUT_SECS: u64 = 10;
pub(super) const LSP_CODE_ACTION_TIMEOUT_SECS: u64 = 10;

/// Cache for syntax engines to enable incremental parsing and injection parser reuse.
///
/// - `main_parsers`: Per-file parser instances for the primary document language
/// - `injection_parsers`: Shared parser instances for embedded languages (e.g., bash in Dockerfile, code blocks in markdown)
///
/// The injection cache eliminates repeated parser initialization when highlighting files with many embedded code blocks.
/// For example, a markdown file with 50 code blocks will reuse the same Rust/JavaScript/Python parsers instead of
/// creating 50 new parser instances.
#[derive(Default)]
pub(super) struct SyntaxEngineCache {
    main_parsers: HashMap<PathBuf, SyntaxEngine>,
    pub(super) injection_parsers: HashMap<LanguageId, SyntaxEngine>,
}

impl SyntaxEngineCache {
    /// Get a main parser for a file, removing it from the cache.
    pub(super) fn take_main_parser(&mut self, file_key: &PathBuf) -> Option<SyntaxEngine> {
        self.main_parsers.remove(file_key)
    }

    /// Return a main parser to the cache after use.
    pub(super) fn return_main_parser(&mut self, file_key: PathBuf, engine: SyntaxEngine) {
        self.main_parsers.insert(file_key, engine);
    }
}

pub(super) type SyntaxEngineCacheHandle = Mutex<SyntaxEngineCache>;

pub(super) fn async_trace_enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| {
        std::env::var("NETHERIZE_ASYNC_TRACE")
            .map(|value| {
                matches!(
                    value.trim().to_ascii_lowercase().as_str(),
                    "1" | "true" | "yes" | "on"
                )
            })
            .unwrap_or(false)
    })
}

macro_rules! async_trace {
    ($($arg:tt)*) => {
        if crate::async_runtime::scheduler::async_trace_enabled() {
            println!($($arg)*);
        }
    };
}

pub(super) use async_trace;

#[derive(Default)]
pub(super) struct PtySessionRegistry {
    next_session_id: AtomicU64,
    sessions: Mutex<HashMap<u64, Arc<PtyProcess>>>,
}

#[derive(Default)]
pub(super) struct LspSessionRegistry {
    sessions: Mutex<HashMap<String, LspSessionHandle>>,
}

#[derive(Clone)]
pub(super) struct LspSessionHandle {
    process: Arc<LspClientProcess>,
    server_name: String,
    root_path: PathBuf,
    capabilities: crate::lsp::capabilities::ServerCapabilities,
}

impl LspSessionRegistry {
    pub(super) fn replace(
        &self,
        server_key: String,
        session: LspSessionHandle,
    ) -> Result<Option<LspSessionHandle>, String> {
        let mut guard = self
            .sessions
            .lock()
            .map_err(|_| "lsp session lock poisoned".to_string())?;
        Ok(guard.insert(server_key, session))
    }

    pub(super) fn get_by_binary(
        &self,
        binary: &str,
    ) -> Result<Option<Arc<LspClientProcess>>, String> {
        let guard = self
            .sessions
            .lock()
            .map_err(|_| "lsp session lock poisoned".to_string())?;
        Ok(guard
            .values()
            .find(|session| session.server_name == binary)
            .map(|session| session.process.clone()))
    }

    pub(super) fn get_handle(&self, binary: &str) -> Result<Option<LspSessionHandle>, String> {
        let guard = self
            .sessions
            .lock()
            .map_err(|_| "lsp session lock poisoned".to_string())?;
        Ok(guard
            .values()
            .find(|session| session.server_name == binary)
            .cloned())
    }

    pub(super) fn get_handle_by_uri(&self, uri: &str) -> Result<Option<LspSessionHandle>, String> {
        let guard = self
            .sessions
            .lock()
            .map_err(|_| "lsp session lock poisoned".to_string())?;
        Ok(guard
            .values()
            .find(|session| session.process.is_document_open(uri))
            .cloned())
    }

    pub(super) fn take_any(&self) -> Result<Option<LspSessionHandle>, String> {
        let mut guard = self
            .sessions
            .lock()
            .map_err(|_| "lsp session lock poisoned".to_string())?;
        let Some(key) = guard.keys().next().cloned() else {
            return Ok(None);
        };
        Ok(guard.remove(&key))
    }

    pub(super) fn clear_if_process(
        &self,
        process: &Arc<LspClientProcess>,
    ) -> Result<Option<LspSessionHandle>, String> {
        let mut guard = self
            .sessions
            .lock()
            .map_err(|_| "lsp session lock poisoned".to_string())?;
        if let Some(key) = guard
            .iter()
            .find_map(|(key, session)| Arc::ptr_eq(&session.process, process).then(|| key.clone()))
        {
            return Ok(guard.remove(&key));
        }
        Ok(None)
    }

    pub(super) fn drain_all(&self) -> Result<Vec<LspSessionHandle>, String> {
        let mut guard = self
            .sessions
            .lock()
            .map_err(|_| "lsp session lock poisoned".to_string())?;
        Ok(std::mem::take(&mut *guard).into_values().collect())
    }
}

impl PtySessionRegistry {
    pub(super) fn alloc_session_id(&self) -> u64 {
        self.next_session_id.fetch_add(1, Ordering::Relaxed) + 1
    }

    pub(super) fn insert(&self, session_id: u64, process: Arc<PtyProcess>) -> Result<(), String> {
        let mut sessions = self
            .sessions
            .lock()
            .map_err(|_| "pty sessions lock poisoned".to_string())?;
        sessions.insert(session_id, process);
        Ok(())
    }

    pub(super) fn get(&self, session_id: u64) -> Result<Option<Arc<PtyProcess>>, String> {
        let sessions = self
            .sessions
            .lock()
            .map_err(|_| "pty sessions lock poisoned".to_string())?;
        Ok(sessions.get(&session_id).cloned())
    }

    pub(super) fn remove(&self, session_id: u64) -> Result<Option<Arc<PtyProcess>>, String> {
        let mut sessions = self
            .sessions
            .lock()
            .map_err(|_| "pty sessions lock poisoned".to_string())?;
        Ok(sessions.remove(&session_id))
    }
}
