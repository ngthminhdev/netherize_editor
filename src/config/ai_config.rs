use std::{fs, path::PathBuf};

use serde::{Deserialize, Serialize};

use crate::config::paths::user_config_root;

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct AiConfig {
    pub inline_completion: Option<InlineCompletionConfig>,
    pub leetcode: Option<LeetCodeConfig>,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct LeetCodeConfig {
    pub use_ai: Option<bool>,
    /// Dedicated AI provider for the LeetCode adapter. When left empty the code
    /// temporarily falls back to `[inline_completion.provider]`; fill this block
    /// in to give LeetCode its own model/key without touching inline completion.
    pub provider: Option<AiProviderConfig>,
    pub verify: Option<bool>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
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

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AiProviderConfig {
    pub api_url: String,
    pub model: String,
    pub api_key: Option<String>,
    pub endpoint_kind: Option<String>,
    /// Sent as `reasoning_effort` in the request body when present. Reasoning
    /// models burn the token budget on thinking and return empty content for
    /// inline completion; "low" keeps them usable when no non-reasoning model
    /// is available.
    pub reasoning_effort: Option<String>,
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

    pub fn inline_completion_enabled(&self) -> bool {
        self.inline_completion
            .as_ref()
            .and_then(|cfg| cfg.enabled)
            .unwrap_or(false)
    }

    pub fn leetcode_ai_enabled(&self) -> bool {
        self.leetcode
            .as_ref()
            .and_then(|config| config.use_ai)
            .unwrap_or(false)
    }

    pub fn leetcode_ai_provider(&self) -> Option<&AiProviderConfig> {
        // Prefer a dedicated `[leetcode.provider]` block once the user has filled
        // in a usable url + model. Until then, temporarily borrow the
        // inline-completion provider so the LeetCode AI path works out of the box.
        let dedicated = self
            .leetcode
            .as_ref()
            .and_then(|config| config.provider.as_ref())
            .filter(|provider| {
                !provider.api_url.trim().is_empty() && !provider.model.trim().is_empty()
            });
        if dedicated.is_some() {
            return dedicated;
        }
        self.inline_completion
            .as_ref()
            .map(|config| &config.provider)
    }

    pub fn leetcode_verify_enabled(&self) -> bool {
        // Default ON: a second re-trace pass catches wrong expected outputs from
        // the first generation. With a capable model both passes are reliable and
        // the extra round-trip stays well within the generation time budget.
        self.leetcode
            .as_ref()
            .and_then(|lc| lc.verify)
            .unwrap_or(true)
    }

    pub fn set_leetcode_ai_enabled(&mut self, enabled: bool) -> Result<(), String> {
        self.leetcode
            .get_or_insert_with(LeetCodeConfig::default)
            .use_ai = Some(enabled);
        self.save_user_override()
    }

    pub fn set_inline_completion_enabled(&mut self, enabled: bool) -> Result<(), String> {
        let Some(inline_completion) = self.inline_completion.as_mut() else {
            return Err("inline completion config is missing".to_string());
        };
        inline_completion.enabled = Some(enabled);
        self.save_user_override()
    }

    /// Mutable access to the inline-completion config, creating a disabled
    /// default if the section is missing so settings edits never silently fail.
    fn leetcode_mut(&mut self) -> &mut LeetCodeConfig {
        self.leetcode.get_or_insert_with(LeetCodeConfig::default)
    }

    fn leetcode_provider_mut(&mut self) -> &mut AiProviderConfig {
        self.leetcode_mut()
            .provider
            .get_or_insert_with(|| AiProviderConfig {
                api_url: String::new(),
                model: String::new(),
                api_key: None,
                endpoint_kind: None,
                reasoning_effort: None,
            })
    }

    pub fn set_leetcode_api_url(&mut self, value: String) -> Result<(), String> {
        self.leetcode_provider_mut().api_url = value.trim().to_string();
        self.save_user_override()
    }

    pub fn set_leetcode_model(&mut self, value: String) -> Result<(), String> {
        self.leetcode_provider_mut().model = value.trim().to_string();
        self.save_user_override()
    }

    pub fn set_leetcode_api_key(&mut self, value: String) -> Result<(), String> {
        let trimmed = value.trim();
        self.leetcode_provider_mut().api_key = (!trimmed.is_empty()).then(|| trimmed.to_string());
        self.save_user_override()
    }

    pub fn set_leetcode_endpoint_kind(&mut self, value: String) -> Result<(), String> {
        let trimmed = value.trim();
        self.leetcode_provider_mut().endpoint_kind =
            (!trimmed.is_empty()).then(|| trimmed.to_string());
        self.save_user_override()
    }

    pub fn set_leetcode_reasoning_effort(&mut self, value: String) -> Result<(), String> {
        let trimmed = value.trim();
        self.leetcode_provider_mut().reasoning_effort =
            (!trimmed.is_empty()).then(|| trimmed.to_string());
        self.save_user_override()
    }

    fn inline_mut(&mut self) -> &mut InlineCompletionConfig {
        self.inline_completion
            .get_or_insert_with(InlineCompletionConfig::with_defaults)
    }

    pub fn set_inline_api_url(&mut self, value: String) -> Result<(), String> {
        self.inline_mut().provider.api_url = value.trim().to_string();
        self.save_user_override()
    }

    pub fn set_inline_model(&mut self, value: String) -> Result<(), String> {
        self.inline_mut().provider.model = value.trim().to_string();
        self.save_user_override()
    }

    pub fn set_inline_api_key(&mut self, value: String) -> Result<(), String> {
        let trimmed = value.trim();
        self.inline_mut().provider.api_key = (!trimmed.is_empty()).then(|| trimmed.to_string());
        self.save_user_override()
    }

    pub fn set_inline_endpoint_kind(&mut self, value: String) -> Result<(), String> {
        let trimmed = value.trim();
        self.inline_mut().provider.endpoint_kind =
            (!trimmed.is_empty()).then(|| trimmed.to_string());
        self.save_user_override()
    }

    pub fn set_inline_max_tokens(&mut self, value: u32) -> Result<(), String> {
        self.inline_mut().max_tokens = Some(value);
        self.save_user_override()
    }

    pub fn set_inline_prefix_chars(&mut self, value: usize) -> Result<(), String> {
        self.inline_mut().prefix_chars = Some(value);
        self.save_user_override()
    }

    pub fn set_inline_suffix_chars(&mut self, value: usize) -> Result<(), String> {
        self.inline_mut().suffix_chars = Some(value);
        self.save_user_override()
    }

    pub fn set_inline_debounce_ms(&mut self, value: u64) -> Result<(), String> {
        self.inline_mut().debounce_ms = Some(value);
        self.save_user_override()
    }

    pub fn save_user_override(&self) -> Result<(), String> {
        let path = candidate_paths()
            .into_iter()
            .find(|path| path.is_file())
            .unwrap_or_else(|| user_config_root().join("ai.toml"));
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .map_err(|err| format!("create ai config dir failed: {err}"))?;
        }
        let text = toml::to_string_pretty(self)
            .map_err(|err| format!("serialize ai config failed: {err}"))?;
        fs::write(&path, text).map_err(|err| format!("write ai config failed: {err}"))
    }
}

impl InlineCompletionConfig {
    /// A disabled config with empty provider fields, used as the seed when the
    /// user edits AI settings while no `[inline_completion]` section exists yet.
    fn with_defaults() -> Self {
        Self {
            enabled: Some(false),
            provider: AiProviderConfig {
                api_url: String::new(),
                model: String::new(),
                api_key: None,
                endpoint_kind: None,
                reasoning_effort: None,
            },
            debounce_ms: None,
            prefix_chars: None,
            suffix_chars: None,
            max_tokens: None,
            trigger_chars: None,
            idle_trigger_ms: None,
            min_prefix_chars: None,
            suppress_in_middle_of_word: None,
            min_interval_ms: None,
        }
    }

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
            .map(|items| {
                items
                    .iter()
                    .filter_map(|item| item.chars().next())
                    .collect()
            })
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
    paths.push(user_config_root().join("config").join("ai.toml"));
    paths.push(user_config_root().join("ai.toml"));
    paths
}

#[cfg(test)]
mod leetcode_tests {
    use super::*;

    #[test]
    fn leetcode_ai_is_disabled_when_section_is_missing() {
        let config: AiConfig = toml::from_str("").expect("empty config should parse");
        assert!(!config.leetcode_ai_enabled());
    }

    #[test]
    fn leetcode_uses_inline_completion_provider_temporarily() {
        let config: AiConfig = toml::from_str(
            r#"
[leetcode]
use_ai = true

[inline_completion]
enabled = true

[inline_completion.provider]
api_url = "http://localhost:20128/v1"
model = "mistral/devstral-2512"
"#,
        )
        .expect("leetcode config should parse");
        assert!(config.leetcode_ai_enabled());
        assert_eq!(
            config
                .leetcode_ai_provider()
                .map(|provider| provider.model.as_str()),
            Some("mistral/devstral-2512")
        );
    }

    #[test]
    fn dedicated_leetcode_provider_overrides_inline_completion() {
        let config: AiConfig = toml::from_str(
            r#"
[leetcode]
use_ai = true

[leetcode.provider]
api_url = "http://localhost:30000/v1"
model = "leetcode/dedicated-model"

[inline_completion]
enabled = true

[inline_completion.provider]
api_url = "http://localhost:20128/v1"
model = "mistral/devstral-2512"
"#,
        )
        .expect("leetcode config should parse");
        assert_eq!(
            config
                .leetcode_ai_provider()
                .map(|provider| provider.model.as_str()),
            Some("leetcode/dedicated-model")
        );
    }

    #[test]
    fn empty_leetcode_provider_falls_back_to_inline_completion() {
        let config: AiConfig = toml::from_str(
            r#"
[leetcode]
use_ai = true

[leetcode.provider]
api_url = ""
model = ""

[inline_completion]
enabled = true

[inline_completion.provider]
api_url = "http://localhost:20128/v1"
model = "mistral/devstral-2512"
"#,
        )
        .expect("leetcode config should parse");
        assert_eq!(
            config
                .leetcode_ai_provider()
                .map(|provider| provider.model.as_str()),
            Some("mistral/devstral-2512")
        );
    }
}
