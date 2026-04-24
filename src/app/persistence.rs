use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::config::paths::{legacy_app_state_root, user_config_root};

const MAX_RECENT: usize = 10;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AppPersistentState {
    #[serde(default)]
    pub recent_projects: Vec<PathBuf>,
    #[serde(default)]
    pub theme_profile: Option<String>,
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
        self.recent_projects.retain(|p| p != &path);
        self.recent_projects.insert(0, path);
        self.recent_projects.truncate(MAX_RECENT);
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
