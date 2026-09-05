use std::{fs, path::PathBuf};

use serde::{Deserialize, Serialize};

use crate::config::paths::user_config_root;

/// Which feature is asking for a provider. Each feature has its own `model`
/// (and reasoning default) on top of the shared `[provider]` endpoint.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AiFeature {
    InlineCompletion,
    LeetCode,
    CompletionRerank,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct AiConfig {
    /// Shared OpenAI-compatible endpoint used by every feature unless a legacy
    /// `[<feature>.provider]` block names its own `api_url`.
    pub provider: Option<AiEndpointConfig>,
    pub inline_completion: Option<InlineCompletionConfig>,
    pub leetcode: Option<LeetCodeConfig>,
    pub completion_rerank: Option<CompletionRerankConfig>,
}

/// `[provider]`: base URL + bearer key of one OpenAI-compatible API.
#[derive(Debug, Clone, Deserialize, Serialize, Default, PartialEq, Eq)]
pub struct AiEndpointConfig {
    pub api_url: String,
    pub api_key: Option<String>,
}

/// AI-assisted re-ranking of the LSP completion popup. The model only reorders
/// the server's own candidates by cursor context — it never invents or drops a
/// suggestion — so correctness stays with the LSP while ordering gets smarter.
/// Disabled by default; opt in via `[completion_rerank] enabled = true`.
#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct CompletionRerankConfig {
    pub enabled: Option<bool>,
    pub model: Option<String>,
    pub reasoning_effort: Option<String>,
    /// Legacy per-feature endpoint; overrides `[provider]` when its url is set.
    pub provider: Option<AiProviderConfig>,
    pub debounce_ms: Option<u64>,
    /// Cap on how many of the top candidates are sent to the model. Keeps the
    /// prompt small and the round-trip fast.
    pub max_candidates: Option<usize>,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct LeetCodeConfig {
    pub use_ai: Option<bool>,
    pub model: Option<String>,
    pub reasoning_effort: Option<String>,
    /// Legacy per-feature endpoint; overrides `[provider]` when its url is set.
    pub provider: Option<AiProviderConfig>,
    pub verify: Option<bool>,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct InlineCompletionConfig {
    pub enabled: Option<bool>,
    pub model: Option<String>,
    pub reasoning_effort: Option<String>,
    /// Legacy per-feature endpoint; overrides `[provider]` when its url is set.
    pub provider: Option<AiProviderConfig>,
    pub debounce_ms: Option<u64>,
    pub prefix_chars: Option<usize>,
    pub suffix_chars: Option<usize>,
    pub max_tokens: Option<u32>,
    pub trigger_chars: Option<Vec<String>>,
    pub idle_trigger_ms: Option<u64>,
    pub min_prefix_chars: Option<usize>,
    pub suppress_in_middle_of_word: Option<bool>,
    pub min_interval_ms: Option<u64>,
    /// How many neighbouring tabs (same extension) to send as reference
    /// context, and how many chars from the head of each. 0 disables.
    pub neighbor_files: Option<usize>,
    pub neighbor_chars: Option<usize>,
}

/// A fully resolved provider for one request: endpoint + model + options.
/// Also the on-disk shape of the legacy `[<feature>.provider]` blocks.
#[derive(Debug, Clone, Deserialize, Serialize, Default, PartialEq, Eq)]
pub struct AiProviderConfig {
    pub api_url: String,
    pub model: String,
    pub api_key: Option<String>,
    /// Only a custom request path (`"/v1/custom"`) is honoured; the request
    /// shape is always OpenAI `chat/completions`.
    pub endpoint_kind: Option<String>,
    /// `"none"` disables thinking (OpenRouter `reasoning.enabled=false`);
    /// `"low"|"medium"|"high"` sets the effort. Reasoning models burn the
    /// token budget on thinking and return empty content for inline
    /// completion, so inline/rerank default to `"none"`.
    pub reasoning_effort: Option<String>,
}

fn non_empty(value: Option<&String>) -> Option<String> {
    value
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .map(str::to_string)
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

    /// Endpoint + model for `feature`, or `None` when either is missing.
    pub fn resolve(&self, feature: AiFeature) -> Option<AiProviderConfig> {
        let (legacy, effort, default_effort) = match feature {
            AiFeature::InlineCompletion => {
                let cfg = self.inline_completion.as_ref();
                (
                    cfg.and_then(|c| c.provider.as_ref()),
                    cfg.and_then(|c| c.reasoning_effort.as_ref()),
                    Some("none"),
                )
            }
            AiFeature::LeetCode => {
                let cfg = self.leetcode.as_ref();
                (
                    cfg.and_then(|c| c.provider.as_ref()),
                    cfg.and_then(|c| c.reasoning_effort.as_ref()),
                    None,
                )
            }
            AiFeature::CompletionRerank => {
                let cfg = self.completion_rerank.as_ref();
                (
                    cfg.and_then(|c| c.provider.as_ref()),
                    cfg.and_then(|c| c.reasoning_effort.as_ref()),
                    Some("none"),
                )
            }
        };
        let legacy_endpoint = legacy.filter(|p| !p.api_url.trim().is_empty());
        let shared = self.provider.as_ref();
        let api_url = legacy_endpoint
            .map(|p| p.api_url.trim().to_string())
            .or_else(|| non_empty(shared.map(|s| &s.api_url)))?;
        let api_key = legacy_endpoint
            .and_then(|p| non_empty(p.api_key.as_ref()))
            .or_else(|| non_empty(shared.and_then(|s| s.api_key.as_ref())));
        let model = Some(self.feature_model(feature)).filter(|m| !m.is_empty())?;
        let reasoning_effort = non_empty(effort)
            .or_else(|| non_empty(legacy.and_then(|p| p.reasoning_effort.as_ref())))
            .or_else(|| default_effort.map(str::to_string));
        Some(AiProviderConfig {
            api_url,
            model,
            api_key,
            endpoint_kind: legacy.and_then(|p| non_empty(p.endpoint_kind.as_ref())),
            reasoning_effort,
        })
    }

    /// Shared endpoint url as shown in Settings (legacy blocks are not surfaced).
    pub fn provider_api_url(&self) -> String {
        self.provider
            .as_ref()
            .map(|p| p.api_url.clone())
            .unwrap_or_default()
    }

    pub fn provider_api_key(&self) -> String {
        self.provider
            .as_ref()
            .and_then(|p| p.api_key.clone())
            .unwrap_or_default()
    }

    /// Model configured for `feature` (its `model` key, else the legacy
    /// block's), independent of whether an endpoint is set — Settings shows
    /// it even before the endpoint is filled in.
    pub fn feature_model(&self, feature: AiFeature) -> String {
        let (model, legacy) = match feature {
            AiFeature::InlineCompletion => {
                let cfg = self.inline_completion.as_ref();
                (
                    cfg.and_then(|c| c.model.as_ref()),
                    cfg.and_then(|c| c.provider.as_ref()),
                )
            }
            AiFeature::LeetCode => {
                let cfg = self.leetcode.as_ref();
                (
                    cfg.and_then(|c| c.model.as_ref()),
                    cfg.and_then(|c| c.provider.as_ref()),
                )
            }
            AiFeature::CompletionRerank => {
                let cfg = self.completion_rerank.as_ref();
                (
                    cfg.and_then(|c| c.model.as_ref()),
                    cfg.and_then(|c| c.provider.as_ref()),
                )
            }
        };
        non_empty(model)
            .or_else(|| non_empty(legacy.map(|p| &p.model)))
            .unwrap_or_default()
    }

    pub fn inline_completion(&self) -> Option<&InlineCompletionConfig> {
        self.inline_completion
            .as_ref()
            .filter(|cfg| cfg.enabled.unwrap_or(false))
    }

    /// Provider for inline completion, only while the feature is enabled.
    pub fn inline_provider(&self) -> Option<AiProviderConfig> {
        self.inline_completion()?;
        self.resolve(AiFeature::InlineCompletion)
    }

    /// Active re-rank config, or `None` when the section is missing or disabled.
    pub fn completion_rerank(&self) -> Option<&CompletionRerankConfig> {
        self.completion_rerank
            .as_ref()
            .filter(|cfg| cfg.enabled.unwrap_or(false))
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

    pub fn leetcode_ai_provider(&self) -> Option<AiProviderConfig> {
        self.resolve(AiFeature::LeetCode)
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
        self.leetcode_mut().use_ai = Some(enabled);
        self.save_user_override()
    }

    pub fn set_inline_completion_enabled(&mut self, enabled: bool) -> Result<(), String> {
        self.inline_mut().enabled = Some(enabled);
        self.save_user_override()
    }

    fn leetcode_mut(&mut self) -> &mut LeetCodeConfig {
        self.leetcode.get_or_insert_with(LeetCodeConfig::default)
    }

    fn inline_mut(&mut self) -> &mut InlineCompletionConfig {
        self.inline_completion
            .get_or_insert_with(InlineCompletionConfig::default)
    }

    fn provider_mut(&mut self) -> &mut AiEndpointConfig {
        self.provider.get_or_insert_with(AiEndpointConfig::default)
    }

    pub fn set_provider_api_url(&mut self, value: String) -> Result<(), String> {
        self.provider_mut().api_url = value.trim().to_string();
        self.save_user_override()
    }

    pub fn set_provider_api_key(&mut self, value: String) -> Result<(), String> {
        let trimmed = value.trim();
        self.provider_mut().api_key = (!trimmed.is_empty()).then(|| trimmed.to_string());
        self.save_user_override()
    }

    pub fn set_inline_model(&mut self, value: String) -> Result<(), String> {
        let trimmed = value.trim();
        self.inline_mut().model = (!trimmed.is_empty()).then(|| trimmed.to_string());
        self.save_user_override()
    }

    pub fn set_leetcode_model(&mut self, value: String) -> Result<(), String> {
        let trimmed = value.trim();
        self.leetcode_mut().model = (!trimmed.is_empty()).then(|| trimmed.to_string());
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

    /// Write the whole config to the user's `ai.toml`. Never the repo's
    /// `./config/ai.toml` (that used to commit API keys) and never in tests.
    pub fn save_user_override(&self) -> Result<(), String> {
        if cfg!(test) {
            return Ok(());
        }
        let path = user_override_path();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .map_err(|err| format!("create ai config dir failed: {err}"))?;
        }
        let text = toml::to_string_pretty(self)
            .map_err(|err| format!("serialize ai config failed: {err}"))?;
        crate::app::persistence::atomic_write(&path, text)
            .map_err(|err| format!("write ai config failed: {err}"))
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

    pub fn neighbor_files(&self) -> usize {
        self.neighbor_files.unwrap_or(1).min(4)
    }

    pub fn neighbor_chars(&self) -> usize {
        self.neighbor_chars.unwrap_or(1200).min(8_000)
    }
}

impl CompletionRerankConfig {
    pub fn debounce_ms(&self) -> u64 {
        self.debounce_ms.unwrap_or(120)
    }

    pub fn max_candidates(&self) -> usize {
        self.max_candidates.unwrap_or(20).clamp(2, 50)
    }
}

fn user_paths() -> [PathBuf; 2] {
    let root = user_config_root();
    [root.join("config").join("ai.toml"), root.join("ai.toml")]
}

/// User config first, repo `./config/ai.toml` last (dev fallback only).
fn candidate_paths() -> Vec<PathBuf> {
    let mut paths = user_paths().to_vec();
    if let Ok(cwd) = std::env::current_dir() {
        paths.push(cwd.join("config").join("ai.toml"));
    }
    paths
}

fn user_override_path() -> PathBuf {
    let [primary, legacy] = user_paths();
    if legacy.is_file() && !primary.is_file() {
        legacy
    } else {
        primary
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn leetcode_ai_is_disabled_when_section_is_missing() {
        let config: AiConfig = toml::from_str("").expect("empty config should parse");
        assert!(!config.leetcode_ai_enabled());
        assert!(config.leetcode_ai_provider().is_none());
    }

    #[test]
    fn features_share_the_provider_endpoint_with_their_own_model() {
        let config: AiConfig = toml::from_str(
            r#"
[provider]
api_url = "https://openrouter.ai/api/v1/"
api_key = "sk-or-test"

[inline_completion]
enabled = true
model = "mistralai/codestral-2508"

[leetcode]
use_ai = true
model = "mistralai/mistral-large-2512"
reasoning_effort = "low"
"#,
        )
        .expect("config should parse");
        let inline = config.inline_provider().expect("inline provider");
        assert_eq!(inline.api_url, "https://openrouter.ai/api/v1/");
        assert_eq!(inline.api_key.as_deref(), Some("sk-or-test"));
        assert_eq!(inline.model, "mistralai/codestral-2508");
        // Inline defaults to no thinking so reasoning models stay usable.
        assert_eq!(inline.reasoning_effort.as_deref(), Some("none"));
        let leetcode = config.leetcode_ai_provider().expect("leetcode provider");
        assert_eq!(leetcode.api_key.as_deref(), Some("sk-or-test"));
        assert_eq!(leetcode.model, "mistralai/mistral-large-2512");
        assert_eq!(leetcode.reasoning_effort.as_deref(), Some("low"));
        // Rerank has no model configured → nothing to call.
        assert!(config.resolve(AiFeature::CompletionRerank).is_none());
    }

    #[test]
    fn legacy_per_feature_provider_block_still_overrides_the_shared_endpoint() {
        let config: AiConfig = toml::from_str(
            r#"
[provider]
api_url = "https://openrouter.ai/api/v1"
api_key = "sk-or-test"

[leetcode]
use_ai = true

[leetcode.provider]
api_url = "http://localhost:30000/v1"
model = "leetcode/dedicated-model"
reasoning_effort = "high"

[inline_completion]
enabled = true
model = "mistralai/codestral-2508"
"#,
        )
        .expect("config should parse");
        let leetcode = config.leetcode_ai_provider().expect("leetcode provider");
        assert_eq!(leetcode.api_url, "http://localhost:30000/v1");
        // Legacy block without a key borrows the shared key.
        assert_eq!(leetcode.api_key.as_deref(), Some("sk-or-test"));
        assert_eq!(leetcode.model, "leetcode/dedicated-model");
        assert_eq!(leetcode.reasoning_effort.as_deref(), Some("high"));
        assert_eq!(config.feature_model(AiFeature::LeetCode), "leetcode/dedicated-model");
        let inline = config.inline_provider().expect("inline provider");
        assert_eq!(inline.api_url, "https://openrouter.ai/api/v1");
    }

    #[test]
    fn inline_provider_is_none_when_disabled_or_without_model() {
        let config: AiConfig = toml::from_str(
            r#"
[provider]
api_url = "https://openrouter.ai/api/v1"

[inline_completion]
enabled = false
model = "mistralai/codestral-2508"
"#,
        )
        .expect("config should parse");
        assert!(config.inline_provider().is_none());
        assert!(config.resolve(AiFeature::InlineCompletion).is_some());

        let config: AiConfig = toml::from_str(
            r#"
[provider]
api_url = "https://openrouter.ai/api/v1"

[inline_completion]
enabled = true
"#,
        )
        .expect("config should parse");
        assert!(config.inline_provider().is_none());
    }

    #[test]
    fn setters_populate_missing_sections() {
        let mut config = AiConfig::default();
        config
            .set_provider_api_url(" https://openrouter.ai/api/v1 ".into())
            .unwrap();
        config.set_provider_api_key("sk-or-x".into()).unwrap();
        config.set_inline_model("a/b".into()).unwrap();
        config.set_leetcode_model("c/d".into()).unwrap();
        config.set_inline_completion_enabled(true).unwrap();
        assert_eq!(config.provider_api_url(), "https://openrouter.ai/api/v1");
        assert_eq!(config.provider_api_key(), "sk-or-x");
        assert_eq!(config.inline_provider().unwrap().model, "a/b");
        assert_eq!(config.leetcode_ai_provider().unwrap().model, "c/d");
        // Clearing the model removes it rather than storing "".
        config.set_leetcode_model("  ".into()).unwrap();
        assert!(config.leetcode_ai_provider().is_none());
    }

    #[test]
    fn feature_model_shows_even_without_an_endpoint() {
        let mut config = AiConfig::default();
        config.set_leetcode_model("m/l".into()).unwrap();
        assert!(config.leetcode_ai_provider().is_none());
        assert_eq!(config.feature_model(AiFeature::LeetCode), "m/l");
        assert_eq!(config.feature_model(AiFeature::InlineCompletion), "");
    }
}
