//! Per-problem LeetCode metadata: solution-file header (so a file remembers
//! which problem it belongs to) plus an on-disk cache of the problem context and
//! its test cases, keyed by problem id.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::config::paths::user_config_root;

/// Parsed `netherize-leetcode` header from a solution file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LeetCodeHeader {
    pub id: String,
    pub slug: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CachedParam {
    pub name: String,
    pub type_name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CachedCase {
    pub input: String,
    pub expected: String,
}

/// Everything the test runner needs to restore a problem and to ask the AI for
/// new cases, persisted as JSON keyed by problem id.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LeetCodeProblemCache {
    pub id: String,
    pub slug: String,
    pub title: String,
    #[serde(default)]
    pub statement: String,
    #[serde(default)]
    pub function_name: String,
    #[serde(default)]
    pub parameters: Vec<CachedParam>,
    #[serde(default)]
    pub cases: Vec<CachedCase>,
}

/// Comment prefix for the header line in each language.
pub fn comment_prefix(language_key: &str) -> &'static str {
    match language_key {
        "python" | "ruby" => "#",
        _ => "//",
    }
}

/// Build the two-line metadata header prepended to a generated solution file.
pub fn build_header(language_key: &str, id: &str, slug: &str, title: &str) -> String {
    let prefix = comment_prefix(language_key);
    format!(
        "{prefix} netherize-leetcode id={id} slug={slug}\n{prefix} {title} — https://leetcode.com/problems/{slug}/\n\n"
    )
}

/// Extract the leetcode id/slug from a solution file's header, if present.
/// Only the first handful of lines are inspected so a `netherize-leetcode`
/// mention deeper in the file cannot be mistaken for the header.
pub fn parse_header(source: &str) -> Option<LeetCodeHeader> {
    let re = regex::Regex::new(r"netherize-leetcode\s+id=(\S+)\s+slug=(\S+)").ok()?;
    source.lines().take(10).find_map(|line| {
        re.captures(line).map(|caps| LeetCodeHeader {
            id: caps[1].to_string(),
            slug: caps[2].to_string(),
        })
    })
}

/// Default on-disk cache directory.
pub fn cache_dir() -> PathBuf {
    user_config_root().join("leetcode-cache")
}

fn sanitize_id(id: &str) -> String {
    id.chars()
        .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { '_' })
        .collect()
}

fn cache_path(dir: &Path, id: &str) -> PathBuf {
    dir.join(format!("{}.json", sanitize_id(id)))
}

/// Persist a problem cache into `dir` (keyed by id).
pub fn save_cache_in(dir: &Path, cache: &LeetCodeProblemCache) -> Result<(), String> {
    std::fs::create_dir_all(dir)
        .map_err(|err| format!("create leetcode cache dir failed: {err}"))?;
    let text = serde_json::to_string_pretty(cache)
        .map_err(|err| format!("serialize leetcode cache failed: {err}"))?;
    crate::app::persistence::atomic_write(&cache_path(dir, &cache.id), text)
        .map_err(|err| format!("write leetcode cache failed: {err}"))
}

/// Load a problem cache by id from `dir`, returning None if missing/invalid.
pub fn load_cache_in(dir: &Path, id: &str) -> Option<LeetCodeProblemCache> {
    let text = std::fs::read_to_string(cache_path(dir, id)).ok()?;
    serde_json::from_str(&text).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn comment_prefix_uses_hash_for_python_and_ruby() {
        assert_eq!(comment_prefix("python"), "#");
        assert_eq!(comment_prefix("ruby"), "#");
        assert_eq!(comment_prefix("javascript"), "//");
        assert_eq!(comment_prefix("rust"), "//");
    }

    #[test]
    fn build_header_round_trips_through_parse_header() {
        let header = build_header("javascript", "1", "two-sum", "Two Sum");
        assert!(header.starts_with("// netherize-leetcode id=1 slug=two-sum"));
        assert!(header.contains("Two Sum"));
        let parsed = parse_header(&header).expect("header should parse");
        assert_eq!(parsed.id, "1");
        assert_eq!(parsed.slug, "two-sum");
    }

    #[test]
    fn build_header_uses_hash_prefix_for_python() {
        let header = build_header("python", "15", "3sum", "3Sum");
        assert!(header.starts_with("# netherize-leetcode id=15 slug=3sum"));
    }

    #[test]
    fn parse_header_finds_marker_below_other_lines() {
        let source =
            "// some banner\n// netherize-leetcode id=42 slug=trapping-rain-water\ncode();\n";
        let parsed = parse_header(source).expect("header should parse");
        assert_eq!(parsed.id, "42");
        assert_eq!(parsed.slug, "trapping-rain-water");
    }

    #[test]
    fn parse_header_returns_none_when_absent() {
        assert!(parse_header("function solve() {}\n").is_none());
    }

    #[test]
    fn save_and_load_cache_round_trip() {
        let dir = std::env::temp_dir().join(format!("netherize_lc_cache_{}", std::process::id()));
        let cache = LeetCodeProblemCache {
            id: "1".into(),
            slug: "two-sum".into(),
            title: "Two Sum".into(),
            statement: "Given an array...".into(),
            function_name: "twoSum".into(),
            parameters: vec![CachedParam {
                name: "nums".into(),
                type_name: "integer[]".into(),
            }],
            cases: vec![CachedCase {
                input: r#"{"nums":[2,7,11,15],"target":9}"#.into(),
                expected: "[0,1]".into(),
            }],
        };
        save_cache_in(&dir, &cache).expect("save cache");
        let loaded = load_cache_in(&dir, "1").expect("load cache");
        assert_eq!(loaded, cache);
        assert!(load_cache_in(&dir, "999").is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
