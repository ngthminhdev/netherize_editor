use std::{fs, path::PathBuf};

use serde::Deserialize;

use crate::config::paths::user_config_root;

#[derive(Debug, Clone, Deserialize, Default)]
pub struct AiConfig {
    pub inline_completion: Option<InlineCompletionConfig>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct InlineCompletionConfig {
    pub enabled: Option<bool>,
    pub provider: AiProviderConfig,
    pub debounce_ms: Option<u64>,
    pub prefix_chars: Option<usize>,
    pub suffix_chars: Option<usize>,
    pub max_tokens: Option<u32>,
    pub trigger_chars: Option<Vec<String>>,
    pub idle_trigger_ms: Option<u64>,
    pub min_prefix_chars: Option<usize>,
    pub suppress_in_middle_of_word: Option<bool>,
    pub min_interval_ms: Option<u64>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AiProviderConfig {
    pub api_url: String,
    pub model: String,
    pub api_key: Option<String>,
    pub endpoint_kind: Option<String>,
}

impl AiConfig {
    pub fn load() -> Self {
        for path in candidate_paths() {
            let Ok(raw) = fs::read_to_string(&path) else {
                continue;
            };
            match toml::from_str::<AiConfig>(&raw) {
                Ok(config) => return config,
                Err(err) => eprintln!("[ai] parse error in {}: {err}", path.display()),
            }
        }
        Self::default()
    }

    pub fn inline_completion(&self) -> Option<&InlineCompletionConfig> {
        self.inline_completion
            .as_ref()
            .filter(|cfg| cfg.enabled.unwrap_or(true))
    }
}

impl InlineCompletionConfig {
    pub fn debounce_ms(&self) -> u64 {
        self.debounce_ms.unwrap_or(80)
    }

    pub fn prefix_chars(&self) -> usize {
        self.prefix_chars.unwrap_or(1200)
    }

    pub fn suffix_chars(&self) -> usize {
        self.suffix_chars.unwrap_or(400)
    }

    pub fn max_tokens(&self) -> u32 {
        self.max_tokens.unwrap_or(96)
    }

    pub fn trigger_chars(&self) -> Vec<char> {
        self.trigger_chars
            .as_ref()
            .map(|items| items.iter().filter_map(|item| item.chars().next()).collect())
            .unwrap_or_else(|| {
                vec![
                    ' ', '\n', '\t', '.', ':', ';', ',', '(', ')', '{', '}', '[', ']',
                ]
            })
    }

    pub fn idle_trigger_ms(&self) -> u64 {
        self.idle_trigger_ms.unwrap_or(500)
    }

    pub fn min_prefix_chars(&self) -> usize {
        self.min_prefix_chars.unwrap_or(2)
    }

    pub fn suppress_in_middle_of_word(&self) -> bool {
        self.suppress_in_middle_of_word.unwrap_or(true)
    }

    pub fn min_interval_ms(&self) -> u64 {
        self.min_interval_ms.unwrap_or(250)
    }
}

fn candidate_paths() -> Vec<PathBuf> {
    let mut paths = Vec::new();
    if let Ok(cwd) = std::env::current_dir() {
        paths.push(cwd.join("config").join("ai.toml"));
    }
    paths.push(user_config_root().join("ai.toml"));
    paths
}
