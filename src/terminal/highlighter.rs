//! Terminal output highlighter — pre-compiled regex patterns for syntax coloring.
//!
//! Provides thread-safe lazy-initialized `Regex` patterns for common terminal
//! output tokens: log-level prefixes, strings, numbers, booleans, null, and
//! time formats.

use once_cell::sync::Lazy;
use regex::Regex;

// ─── Log Level Patterns ──────────────────────────────────────────────────────

/// Matches Debug log prefix: `D/` at the start of a line.
pub static RE_LOG_DEBUG: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?m)^D/").expect("invalid debug regex"));

/// Matches Info log prefix: `I/` at the start of a line.
pub static RE_LOG_INFO: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?m)^I/").expect("invalid info regex"));

/// Matches Warn log prefix: `W/` at the start of a line.
pub static RE_LOG_WARN: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?m)^W/").expect("invalid warn regex"));

/// Matches Error log prefix: `E/` at the start of a line.
pub static RE_LOG_ERROR: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?m)^E/").expect("invalid error regex"));

// ─── Data Type Patterns ──────────────────────────────────────────────────────

/// Matches a quoted string literal (double or single quotes, supports escaped quotes).
pub static RE_STRING: Lazy<Regex> =
    Lazy::new(|| Regex::new(r#""(?:[^"\\]|\\.)*"|'(?:[^'\\]|\\.)*'"#).expect("invalid string regex"));

/// Matches an integer or floating-point number (including negative).
pub static RE_NUMBER: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"-?\b\d+(?:\.\d+)?(?:[eE][+-]?\d+)?\b").expect("invalid number regex"));

/// Matches boolean literals `true` or `false`.
pub static RE_BOOL: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"\b(?:true|false)\b").expect("invalid bool regex"));

/// Matches the `null` literal.
pub static RE_NULL: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"\bnull\b").expect("invalid null regex"));

/// Matches a time format `HH:MM:SS` (24-hour).
pub static RE_TIME: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"\b\d{2}:\d{2}:\d{2}\b").expect("invalid time regex"));

/// Matches common programming language keywords as whole words.
pub static RE_KEYWORD: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        r"\b(?:if|else|for|while|do|switch|case|break|continue|return|function|fn|def|\
          class|struct|enum|interface|var|let|const|import|export|module|package|\
          async|await|try|catch|finally|throw|new|this|super|extends|implements|\
          public|private|protected|static|final|abstract|void|int|float|double|\
          char|boolean|string|type|namespace|using|from|as|in|of|is|not|and|or|\
          nil|None|Some|Ok|Err|include|require|print|log|console|error|warn|info|debug)\b",
    )
    .expect("invalid keyword regex")
});