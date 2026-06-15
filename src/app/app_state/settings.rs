/// Snapshot of `[inline_completion]` config values used to seed the AI section
/// of the settings tab. Built from `AiConfig` when the tab opens.
#[derive(Debug, Clone, PartialEq)]
pub struct AiInlineSettings {
    pub api_url: String,
    pub model: String,
    pub api_key: String,
    pub endpoint_kind: String,
    pub max_tokens: u32,
    pub prefix_chars: usize,
    pub suffix_chars: usize,
    pub debounce_ms: u64,
    pub leetcode_ai_enabled: bool,
    pub leetcode_api_url: String,
    pub leetcode_model: String,
    pub leetcode_api_key: String,
    pub leetcode_endpoint_kind: String,
    pub leetcode_reasoning_effort: String,
}

#[derive(Debug, Clone, PartialEq)]
pub enum SettingItem {
    ThemeSelector {
        current: String,
    },
    FontFamily {
        current: String,
    },
    FontSize {
        current: f32,
    },
    LineHeight {
        current: f32,
    },
    IndentTabWidth {
        current: u8,
    },
    IndentInsertSpaces {
        enabled: bool,
    },
    InlineSuggestion {
        enabled: bool,
    },
    LeetCodeAi {
        enabled: bool,
    },
    LeetCodeAiApiUrl {
        current: String,
    },
    LeetCodeAiModel {
        current: String,
    },
    LeetCodeAiApiKey {
        current: String,
    },
    LeetCodeAiEndpointKind {
        current: String,
    },
    LeetCodeAiReasoningEffort {
        current: String,
    },
    AiApiUrl {
        current: String,
    },
    AiModel {
        current: String,
    },
    AiApiKey {
        current: String,
    },
    AiEndpointKind {
        current: String,
    },
    AiMaxTokens {
        current: u32,
    },
    AiPrefixChars {
        current: usize,
    },
    AiSuffixChars {
        current: usize,
    },
    AiDebounceMs {
        current: u64,
    },
    SidebarWidth {
        current: i32,
    },
    RightSidebarWidth {
        current: i32,
    },
    BottomPanelHeight {
        current: i32,
    },
    UiRounding {
        enabled: bool,
        radius_px: f32,
    },
    EnableOutline {
        enabled: bool,
    },
    /// window.scale_factor_override — None = follow the display ("Auto").
    UiScale {
        current: Option<f32>,
    },
    BgOpacity {
        current: u8,
    },
}

impl SettingItem {
    pub fn title(&self) -> &'static str {
        match self {
            Self::ThemeSelector { .. } => "Theme",
            Self::FontFamily { .. } => "Font Family",
            Self::FontSize { .. } => "Font Size",
            Self::LineHeight { .. } => "Line Height",
            Self::IndentTabWidth { .. } => "Tab Width",
            Self::IndentInsertSpaces { .. } => "Indent Style",
            Self::InlineSuggestion { .. } => "Inline Completion",
            Self::LeetCodeAi { .. } => "LeetCode AI",
            Self::LeetCodeAiApiUrl { .. } => "LeetCode AI Endpoint",
            Self::LeetCodeAiModel { .. } => "LeetCode AI Model",
            Self::LeetCodeAiApiKey { .. } => "LeetCode AI API Key",
            Self::LeetCodeAiEndpointKind { .. } => "LeetCode AI Endpoint Kind",
            Self::LeetCodeAiReasoningEffort { .. } => "LeetCode AI Reasoning Effort",
            Self::AiApiUrl { .. } => "AI Endpoint",
            Self::AiModel { .. } => "AI Model",
            Self::AiApiKey { .. } => "AI API Key",
            Self::AiEndpointKind { .. } => "AI Endpoint Kind",
            Self::AiMaxTokens { .. } => "AI Max Tokens",
            Self::AiPrefixChars { .. } => "AI Prefix Context",
            Self::AiSuffixChars { .. } => "AI Suffix Context",
            Self::AiDebounceMs { .. } => "AI Debounce",
            Self::SidebarWidth { .. } => "Left Dock Width",
            Self::RightSidebarWidth { .. } => "Right Dock Width",
            Self::BottomPanelHeight { .. } => "Bottom Dock Height",
            Self::UiRounding { .. } => "UI Rounding",
            Self::EnableOutline { .. } => "Panel Outlines",
            Self::UiScale { .. } => "UI Scale",
            Self::BgOpacity { .. } => "Panel Background Opacity",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum SettingsEditingKind {
    FontFamily,
    FontSize,
    LineHeight,
    IndentTabWidth,
    UiRounding,
    SidebarWidth,
    RightSidebarWidth,
    BottomPanelHeight,
    UiScale,
    AiApiUrl,
    AiModel,
    AiApiKey,
    AiEndpointKind,
    AiMaxTokens,
    AiPrefixChars,
    AiSuffixChars,
    AiDebounceMs,
    LeetCodeAiApiUrl,
    LeetCodeAiModel,
    LeetCodeAiApiKey,
    LeetCodeAiEndpointKind,
    LeetCodeAiReasoningEffort,
    BgOpacity,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SettingsEditingState {
    pub kind: SettingsEditingKind,
    pub draft: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SettingsState {
    pub selected_index: usize,
    pub items: Vec<SettingItem>,
    pub editing: Option<SettingsEditingState>,
}

impl SettingsState {
    pub fn new(
        theme_profile: impl Into<String>,
        font_family: impl Into<String>,
        font_size: f32,
        line_height: f32,
        tab_width: u8,
        insert_spaces: bool,
        left_width: i32,
        right_width: i32,
        bottom_height: i32,
        ui_rounding_enabled: bool,
        border_radius_px: f32,
        enable_outline: bool,
        inline_suggestion_enabled: bool,
        ai: AiInlineSettings,
        ui_scale_override: Option<f32>,
        bg_opacity: u8,
    ) -> Self {
        Self {
            selected_index: 0,
            items: vec![
                // APPEARANCE
                SettingItem::ThemeSelector {
                    current: theme_profile.into(),
                },
                SettingItem::UiRounding {
                    enabled: ui_rounding_enabled,
                    radius_px: border_radius_px.max(0.0),
                },
                SettingItem::EnableOutline {
                    enabled: enable_outline,
                },
                SettingItem::UiScale {
                    current: ui_scale_override,
                },
                SettingItem::BgOpacity {
                    current: bg_opacity,
                },
                // TYPOGRAPHY
                SettingItem::FontFamily {
                    current: font_family.into(),
                },
                SettingItem::FontSize {
                    current: font_size.max(1.0),
                },
                SettingItem::LineHeight {
                    current: line_height.max(1.0),
                },
                // EDITOR
                SettingItem::IndentTabWidth {
                    current: tab_width.max(1),
                },
                SettingItem::IndentInsertSpaces {
                    enabled: insert_spaces,
                },
                // AI
                SettingItem::InlineSuggestion {
                    enabled: inline_suggestion_enabled,
                },
                SettingItem::LeetCodeAi {
                    enabled: ai.leetcode_ai_enabled,
                },
                SettingItem::LeetCodeAiApiUrl {
                    current: ai.leetcode_api_url,
                },
                SettingItem::LeetCodeAiModel {
                    current: ai.leetcode_model,
                },
                SettingItem::LeetCodeAiApiKey {
                    current: ai.leetcode_api_key,
                },
                SettingItem::LeetCodeAiEndpointKind {
                    current: ai.leetcode_endpoint_kind,
                },
                SettingItem::LeetCodeAiReasoningEffort {
                    current: ai.leetcode_reasoning_effort,
                },
                SettingItem::AiApiUrl {
                    current: ai.api_url,
                },
                SettingItem::AiModel { current: ai.model },
                SettingItem::AiApiKey {
                    current: ai.api_key,
                },
                SettingItem::AiEndpointKind {
                    current: ai.endpoint_kind,
                },
                SettingItem::AiMaxTokens {
                    current: ai.max_tokens,
                },
                SettingItem::AiPrefixChars {
                    current: ai.prefix_chars,
                },
                SettingItem::AiSuffixChars {
                    current: ai.suffix_chars,
                },
                SettingItem::AiDebounceMs {
                    current: ai.debounce_ms,
                },
                // LAYOUT
                SettingItem::SidebarWidth {
                    current: left_width.max(0),
                },
                SettingItem::RightSidebarWidth {
                    current: right_width.max(0),
                },
                SettingItem::BottomPanelHeight {
                    current: bottom_height.max(0),
                },
            ],
            editing: None,
        }
    }

    pub fn select_next(&mut self) -> bool {
        if self.selected_index + 1 < self.items.len() {
            self.selected_index += 1;
            true
        } else {
            false
        }
    }

    pub fn select_prev(&mut self) -> bool {
        if self.selected_index > 0 {
            self.selected_index -= 1;
            true
        } else {
            false
        }
    }

    pub fn selected_item(&self) -> Option<&SettingItem> {
        self.items.get(self.selected_index)
    }

    pub fn selected_item_mut(&mut self) -> Option<&mut SettingItem> {
        self.items.get_mut(self.selected_index)
    }

    pub fn begin_editing(&mut self) -> bool {
        let Some(item) = self.selected_item() else {
            return false;
        };
        let (kind, draft) = match item {
            SettingItem::FontFamily { current } => {
                (SettingsEditingKind::FontFamily, current.clone())
            }
            SettingItem::FontSize { current } => {
                (SettingsEditingKind::FontSize, format!("{current:.1}"))
            }
            SettingItem::LineHeight { current } => {
                (SettingsEditingKind::LineHeight, format!("{current:.1}"))
            }
            SettingItem::SidebarWidth { current } => {
                (SettingsEditingKind::SidebarWidth, current.to_string())
            }
            SettingItem::RightSidebarWidth { current } => {
                (SettingsEditingKind::RightSidebarWidth, current.to_string())
            }
            SettingItem::BottomPanelHeight { current } => {
                (SettingsEditingKind::BottomPanelHeight, current.to_string())
            }
            SettingItem::IndentTabWidth { current } => {
                (SettingsEditingKind::IndentTabWidth, current.to_string())
            }
            SettingItem::UiRounding { enabled, radius_px } => (
                SettingsEditingKind::UiRounding,
                if *enabled && *radius_px > 0.0 {
                    format!("{radius_px:.0}")
                } else {
                    "0".to_string()
                },
            ),
            SettingItem::UiScale { current } => (
                SettingsEditingKind::UiScale,
                current
                    .map(|v| format!("{v:.2}"))
                    .unwrap_or_else(|| "auto".to_string()),
            ),
            SettingItem::AiApiUrl { current } => (SettingsEditingKind::AiApiUrl, current.clone()),
            SettingItem::AiModel { current } => (SettingsEditingKind::AiModel, current.clone()),
            SettingItem::AiApiKey { current } => (SettingsEditingKind::AiApiKey, current.clone()),
            SettingItem::AiEndpointKind { current } => {
                (SettingsEditingKind::AiEndpointKind, current.clone())
            }
            SettingItem::AiMaxTokens { current } => {
                (SettingsEditingKind::AiMaxTokens, current.to_string())
            }
            SettingItem::AiPrefixChars { current } => {
                (SettingsEditingKind::AiPrefixChars, current.to_string())
            }
            SettingItem::AiSuffixChars { current } => {
                (SettingsEditingKind::AiSuffixChars, current.to_string())
            }
            SettingItem::AiDebounceMs { current } => {
                (SettingsEditingKind::AiDebounceMs, current.to_string())
            }
            SettingItem::LeetCodeAiApiUrl { current } => {
                (SettingsEditingKind::LeetCodeAiApiUrl, current.clone())
            }
            SettingItem::LeetCodeAiModel { current } => {
                (SettingsEditingKind::LeetCodeAiModel, current.clone())
            }
            SettingItem::LeetCodeAiApiKey { current } => {
                (SettingsEditingKind::LeetCodeAiApiKey, current.clone())
            }
            SettingItem::LeetCodeAiEndpointKind { current } => {
                (SettingsEditingKind::LeetCodeAiEndpointKind, current.clone())
            }
            SettingItem::LeetCodeAiReasoningEffort { current } => {
                (SettingsEditingKind::LeetCodeAiReasoningEffort, current.clone())
            }
            SettingItem::BgOpacity { current } => {
                (SettingsEditingKind::BgOpacity, current.to_string())
            }
            SettingItem::ThemeSelector { .. }
            | SettingItem::EnableOutline { .. }
            | SettingItem::IndentInsertSpaces { .. }
            | SettingItem::InlineSuggestion { .. }
            | SettingItem::LeetCodeAi { .. } => return false,
        };
        self.editing = Some(SettingsEditingState { kind, draft });
        true
    }

    pub fn cancel_editing(&mut self) -> bool {
        self.editing.take().is_some()
    }

    pub fn append_editing_text(&mut self, text: &str) -> bool {
        let Some(editing) = &mut self.editing else {
            return false;
        };
        editing.draft.push_str(text);
        true
    }

    pub fn backspace_editing(&mut self) -> bool {
        let Some(editing) = &mut self.editing else {
            return false;
        };
        editing.draft.pop().is_some()
    }
}
