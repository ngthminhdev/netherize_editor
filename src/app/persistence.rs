use std::path::PathBuf;

use serde::{Deserialize, Serialize};

const MAX_RECENT: usize = 10;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AppPersistentState {
    #[serde(default)]
    pub recent_projects: Vec<PathBuf>,
}

impl AppPersistentState {
    fn state_dir() -> PathBuf {
        #[cfg(target_os = "windows")]
        let home = std::env::var_os("USERPROFILE")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("."));
        #[cfg(not(target_os = "windows"))]
        let home = std::env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("."));
        home.join(".netherize_editor")
    }

    fn state_path() -> PathBuf {
        Self::state_dir().join("state.toml")
    }

    pub fn load() -> Self {
        let path = Self::state_path();
        let text = match std::fs::read_to_string(&path) {
            Ok(t) => t,
            Err(_) => return Self::default(),
        };
        toml::from_str::<Self>(&text).unwrap_or_default()
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
}
