use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use serde::{Deserialize, Serialize};

use crate::config::paths::{legacy_app_state_root, user_config_root};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AppPersistentState {
    #[serde(default)]
    pub recent_projects: Vec<PathBuf>,
    #[serde(default)]
    pub recent_project_meta: HashMap<PathBuf, RecentProjectMeta>,
    #[serde(default)]
    pub theme_profile: Option<String>,
    /// True after the one-time first-run key-hint toast was shown.
    #[serde(default)]
    pub first_run_tour_shown: bool,
    /// Language keys for the "New LeetCode File" picker, most-recently-used
    /// first. Lets the picker surface the language you reached for last.
    #[serde(default)]
    pub recent_leetcode_languages: Vec<String>,
    /// Agent ids for the AI Chat agent picker, most-recently-used first.
    #[serde(default)]
    pub recent_ai_agents: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RecentProjectMeta {
    #[serde(default)]
    pub icon_source: Option<String>,
    #[serde(default)]
    pub last_opened_unix_secs: Option<u64>,
}

impl AppPersistentState {
    fn state_dir() -> PathBuf {
        user_config_root()
    }

    fn legacy_state_dir() -> PathBuf {
        legacy_app_state_root()
    }

    fn state_path() -> PathBuf {
        Self::state_dir().join("state.toml")
    }

    fn legacy_state_path() -> PathBuf {
        Self::legacy_state_dir().join("state.toml")
    }

    fn load_from_path(path: &Path) -> Option<Self> {
        let text = std::fs::read_to_string(path).ok()?;
        toml::from_str::<Self>(&text).ok()
    }

    pub fn load() -> Self {
        let path = Self::state_path();
        if let Some(state) = Self::load_from_path(&path) {
            return state;
        }

        let legacy_path = Self::legacy_state_path();
        if let Some(state) = Self::load_from_path(&legacy_path) {
            state.save();
            return state;
        }

        Self::default()
    }

    pub fn save(&self) {
        let path = Self::state_path();
        if let Some(parent) = path.parent() {
            if let Err(err) = std::fs::create_dir_all(parent) {
                eprintln!("[AppPersistentState] create dir failed: {err}");
                return;
            }
        }
        match toml::to_string_pretty(self) {
            Ok(text) => {
                if let Err(err) = std::fs::write(&path, text) {
                    eprintln!("[AppPersistentState] write failed: {err}");
                }
            }
            Err(err) => {
                eprintln!("[AppPersistentState] serialize failed: {err}");
            }
        }
    }

    pub fn push_recent(&mut self, path: PathBuf) {
        self.push_recent_with_icon(path, None);
    }

    pub fn push_recent_with_icon(&mut self, path: PathBuf, icon_source: Option<String>) {
        self.recent_projects.retain(|p| p != &path);
        self.recent_projects.insert(0, path.clone());
        let keep: std::collections::HashSet<_> = self.recent_projects.iter().cloned().collect();
        self.recent_project_meta
            .retain(|path, _| keep.contains(path));
        let last_opened_unix_secs = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .ok()
            .map(|duration| duration.as_secs());
        self.recent_project_meta.insert(
            path,
            RecentProjectMeta {
                icon_source,
                last_opened_unix_secs,
            },
        );
    }

    /// Record `language_key` as the most-recently-used LeetCode language,
    /// moving it to the front of the MRU list (deduped, capped).
    pub fn push_recent_leetcode_language(&mut self, language_key: &str) {
        const MAX_RECENT_LANGUAGES: usize = 12;
        self.recent_leetcode_languages
            .retain(|key| key != language_key);
        self.recent_leetcode_languages
            .insert(0, language_key.to_string());
        self.recent_leetcode_languages
            .truncate(MAX_RECENT_LANGUAGES);
    }

    /// Record `agent_id` as the most-recently-used AI agent (front, dedup, cap).
    pub fn push_recent_ai_agent(&mut self, agent_id: &str) {
        const MAX_RECENT_AGENTS: usize = 12;
        self.recent_ai_agents.retain(|id| id != agent_id);
        self.recent_ai_agents.insert(0, agent_id.to_string());
        self.recent_ai_agents.truncate(MAX_RECENT_AGENTS);
    }

    pub fn infer_project_icon_source(path: &Path) -> String {
        const MARKERS: &[&str] = &[
            "Cargo.toml",
            "package.json",
            "go.mod",
            "pom.xml",
            "build.gradle",
            "build.gradle.kts",
            "settings.gradle",
            "settings.gradle.kts",
            "gradlew",
            "flake.nix",
            "default.nix",
            "deno.json",
            "tsconfig.json",
            "pyproject.toml",
            "requirements.txt",
            "build.zig",
            "CMakeLists.txt",
            "Makefile",
            "README.md",
        ];
        for marker in MARKERS {
            let candidate = path.join(marker);
            if candidate.exists() {
                return candidate.display().to_string();
            }
        }
        path.display().to_string()
    }

    pub fn most_recent_existing(&self) -> Option<PathBuf> {
        self.recent_projects
            .iter()
            .find(|p| p.exists() && p.is_dir())
            .cloned()
    }

    pub fn configured_theme_profile(&self) -> Option<&str> {
        self.theme_profile
            .as_deref()
            .map(str::trim)
            .filter(|profile| !profile.is_empty())
    }

    pub fn set_theme_profile(&mut self, theme_profile: Option<String>) {
        self.theme_profile = theme_profile
            .map(|profile| profile.trim().to_string())
            .filter(|profile| !profile.is_empty());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn leetcode_language_mru_moves_to_front_and_dedups() {
        let mut state = AppPersistentState::default();
        state.push_recent_leetcode_language("python");
        state.push_recent_leetcode_language("go");
        state.push_recent_leetcode_language("python"); // re-use floats to front
        assert_eq!(state.recent_leetcode_languages, vec!["python", "go"]);
    }

    #[test]
    fn leetcode_language_mru_is_capped() {
        let mut state = AppPersistentState::default();
        for i in 0..20 {
            state.push_recent_leetcode_language(&format!("lang{i}"));
        }
        assert_eq!(state.recent_leetcode_languages.len(), 12);
        // Most recent stays at the front.
        assert_eq!(state.recent_leetcode_languages[0], "lang19");
    }
}
