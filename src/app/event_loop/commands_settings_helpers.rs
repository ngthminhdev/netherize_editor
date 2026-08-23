use super::*;

/// Which workbench dock a resize-mode key adjusts.
#[derive(Clone, Copy)]
enum ResizeDock {
    Left,
    Right,
    Bottom,
}

impl AppShell {
    pub(super) const RESIZE_STEP_PX: f32 = 20.0;

    /// Resize-mode `h`/`l`: adjust the width of whatever region is focused.
    ///
    /// `delta > 0` means "grow toward the right" (`l`), `delta < 0` means
    /// "grow toward the left" (`h`). A focused sidebar simply changes its own
    /// width; with the editor focused the matching dock is shrunk so the editor
    /// expands on that edge instead.
    pub(super) fn resize_focused_width(&mut self, delta: f32) -> bool {
        match self.focus_manager.current() {
            FocusTarget::LeftSidebar => self.adjust_dock(ResizeDock::Left, delta),
            FocusTarget::RightSidebar => self.adjust_dock(ResizeDock::Right, delta),
            FocusTarget::CenterEditor => {
                if delta < 0.0 {
                    // `h`: shrink the left dock so the editor grows leftward.
                    self.adjust_dock(ResizeDock::Left, delta)
                } else {
                    // `l`: shrink the right dock so the editor grows rightward.
                    self.adjust_dock(ResizeDock::Right, -delta)
                }
            }
            _ => false,
        }
    }

    /// Resize-mode `j`/`k`: adjust the height of whatever region is focused.
    ///
    /// `delta > 0` is "taller" (`j`), `delta < 0` is "shorter" (`k`). The bottom
    /// panel changes its own height; with the editor focused the bottom panel is
    /// shrunk so the editor grows downward.
    pub(super) fn resize_focused_height(&mut self, delta: f32) -> bool {
        match self.focus_manager.current() {
            FocusTarget::BottomPanel => self.adjust_dock(ResizeDock::Bottom, delta),
            FocusTarget::CenterEditor => self.adjust_dock(ResizeDock::Bottom, -delta),
            _ => false,
        }
    }

    /// Resize-mode `H`: grow the left dock (the editor shrinks on its left edge).
    pub(super) fn resize_left_dock(&mut self, delta: f32) -> bool {
        self.adjust_dock(ResizeDock::Left, delta)
    }

    /// Resize-mode `L`: grow the right dock (the editor shrinks on its right edge).
    pub(super) fn resize_right_dock(&mut self, delta: f32) -> bool {
        self.adjust_dock(ResizeDock::Right, delta)
    }

    /// Apply a clamped size delta to one dock and flag the affected layout.
    /// Returns `true` if the dock was visible and its size actually changed.
    fn adjust_dock(&mut self, dock: ResizeDock, delta: f32) -> bool {
        let (size, min, max, visible) = match dock {
            ResizeDock::Left => (
                &mut self.panel_state.left.size_px,
                160.0,
                1280.0,
                self.panel_state.left.visible,
            ),
            ResizeDock::Right => (
                &mut self.panel_state.right.size_px,
                180.0,
                1440.0,
                self.panel_state.right.visible,
            ),
            ResizeDock::Bottom => (
                &mut self.panel_state.bottom.size_px,
                120.0,
                1040.0,
                self.panel_state.bottom.visible,
            ),
        };

        if !visible {
            return false;
        }

        let next = (*size + delta).clamp(min, max);
        if (next - *size).abs() <= f32::EPSILON {
            return false;
        }
        *size = next;

        match dock {
            ResizeDock::Left => {
                // Config stores unscaled units; panel_state holds physical px.
                self.ui_config.docks.left.size_px = next / self.runtime_scale.max(0.5);
                self.sidebar_needs_layout = true;
            }
            ResizeDock::Right => {
                self.ui_config.docks.right.size_px = next / self.runtime_scale.max(0.5);
                self.sidebar_needs_layout = true;
            }
            ResizeDock::Bottom => {
                self.ui_config.docks.bottom.size_px = next / self.runtime_scale.max(0.5);
                self.terminal_needs_layout = true;
            }
        }

        self.editor_needs_layout = true;
        self.editor_caret_needs_layout = true;
        let _ = self.ui_config.save_user_override();
        true
    }

    pub(super) fn finalize_settings_change(&mut self) -> bool {
        self.apply_scaled_runtime_config();
        let _ = self.ui_config.save_user_override();
        self.editor_needs_layout = true;
        self.editor_caret_needs_layout = false;
        true
    }

    pub(super) fn update_active_settings_edit_draft(&mut self, text: String) -> bool {
        let Some(state) = self.app_state.active_settings_buffer_mut() else {
            return false;
        };
        let Some(editing) = &mut state.editing else {
            return false;
        };
        editing.draft = text;
        self.editor_needs_layout = true;
        self.editor_caret_needs_layout = false;
        true
    }

    pub(super) fn adjust_selected_setting(&mut self, delta: i32) -> bool {
        let Some(selected) = self
            .app_state
            .active_settings_buffer()
            .and_then(|state| state.selected_item())
            .cloned()
        else {
            return false;
        };

        if self.app_state.settings_is_editing() {
            return match selected {
                crate::app::app_state::SettingItem::FontSize { current } => {
                    let next = (current + delta as f32 * 0.5).clamp(8.0, 40.0);
                    self.update_active_settings_edit_draft(format!("{next:.1}"))
                }
                crate::app::app_state::SettingItem::LineHeight { current } => {
                    let next = (current + delta as f32 * 0.5).clamp(10.0, 64.0);
                    self.update_active_settings_edit_draft(format!("{next:.1}"))
                }
                crate::app::app_state::SettingItem::IndentTabWidth { current } => {
                    let next = (current as i32 + delta).clamp(1, 8) as u8;
                    self.update_active_settings_edit_draft(next.to_string())
                }
                crate::app::app_state::SettingItem::UiRounding { enabled, radius_px } => {
                    let current = if enabled && radius_px > 0.0 {
                        radius_px.round() as i32
                    } else {
                        0
                    };
                    let next = (current + delta * 8).clamp(0, 24);
                    self.update_active_settings_edit_draft(next.to_string())
                }
                crate::app::app_state::SettingItem::SidebarWidth { current } => {
                    let next = (current + delta * 20).clamp(160, 1280);
                    self.update_active_settings_edit_draft(next.to_string())
                }
                crate::app::app_state::SettingItem::RightSidebarWidth { current } => {
                    let next = (current + delta * 20).clamp(180, 1440);
                    self.update_active_settings_edit_draft(next.to_string())
                }
                crate::app::app_state::SettingItem::BottomPanelHeight { current } => {
                    let next = (current + delta * 20).clamp(120, 1040);
                    self.update_active_settings_edit_draft(next.to_string())
                }
                crate::app::app_state::SettingItem::UiScale { current } => {
                    let base = current.unwrap_or(self.runtime_scale);
                    let next = (base + delta as f32 * 0.05).clamp(0.5, 3.0);
                    self.update_active_settings_edit_draft(format!("{next:.2}"))
                }
                crate::app::app_state::SettingItem::AiMaxTokens { current } => {
                    let next = (current as i64 + delta as i64 * 16).clamp(1, 4096);
                    self.update_active_settings_edit_draft(next.to_string())
                }
                crate::app::app_state::SettingItem::AiPrefixChars { current } => {
                    let next = (current as i64 + delta as i64 * 200).clamp(0, 32_000);
                    self.update_active_settings_edit_draft(next.to_string())
                }
                crate::app::app_state::SettingItem::AiSuffixChars { current } => {
                    let next = (current as i64 + delta as i64 * 100).clamp(0, 16_000);
                    self.update_active_settings_edit_draft(next.to_string())
                }
                crate::app::app_state::SettingItem::AiDebounceMs { current } => {
                    let next = (current as i64 + delta as i64 * 10).clamp(0, 5_000);
                    self.update_active_settings_edit_draft(next.to_string())
                }
                _ => false,
            };
        }

        match selected {
            crate::app::app_state::SettingItem::FontSize { current } => {
                let next = (current + delta as f32 * 0.5).clamp(8.0, 40.0);
                self.base_theme.editor.font_size = next;
                self.ui_config.editor.font_size = next;
                if let Some(state) = self.app_state.active_settings_buffer_mut()
                    && let Some(crate::app::app_state::SettingItem::FontSize { current }) =
                        state.selected_item_mut()
                {
                    *current = next;
                }
                self.finalize_settings_change()
            }
            crate::app::app_state::SettingItem::LineHeight { current } => {
                let next = (current + delta as f32 * 0.5).clamp(10.0, 64.0);
                self.base_theme.editor.line_height = next;
                self.ui_config.editor.line_height = next;
                if let Some(state) = self.app_state.active_settings_buffer_mut()
                    && let Some(crate::app::app_state::SettingItem::LineHeight { current }) =
                        state.selected_item_mut()
                {
                    *current = next;
                }
                self.finalize_settings_change()
            }
            crate::app::app_state::SettingItem::SidebarWidth { current } => {
                let next = (current + delta * 20).clamp(160, 1280);
                self.ui_config.docks.left.size_px = next as f32;
                if let Some(state) = self.app_state.active_settings_buffer_mut()
                    && let Some(crate::app::app_state::SettingItem::SidebarWidth { current }) =
                        state.selected_item_mut()
                {
                    *current = next;
                }
                self.finalize_settings_change()
            }
            crate::app::app_state::SettingItem::RightSidebarWidth { current } => {
                let next = (current + delta * 20).clamp(180, 1440);
                self.ui_config.docks.right.size_px = next as f32;
                if let Some(state) = self.app_state.active_settings_buffer_mut()
                    && let Some(crate::app::app_state::SettingItem::RightSidebarWidth { current }) =
                        state.selected_item_mut()
                {
                    *current = next;
                }
                self.finalize_settings_change()
            }
            crate::app::app_state::SettingItem::BottomPanelHeight { current } => {
                let next = (current + delta * 20).clamp(120, 1040);
                self.ui_config.docks.bottom.size_px = next as f32;
                if let Some(state) = self.app_state.active_settings_buffer_mut()
                    && let Some(crate::app::app_state::SettingItem::BottomPanelHeight { current }) =
                        state.selected_item_mut()
                {
                    *current = next;
                }
                self.finalize_settings_change()
            }
            crate::app::app_state::SettingItem::IndentTabWidth { current } => {
                let next = (current as i32 + delta).clamp(1, 8) as u8;
                self.ui_config.indent.tab_width = next;
                self.app_state.set_indent_config(self.ui_config.indent);
                if let Some(state) = self.app_state.active_settings_buffer_mut()
                    && let Some(crate::app::app_state::SettingItem::IndentTabWidth { current }) =
                        state.selected_item_mut()
                {
                    *current = next;
                }
                self.finalize_settings_change()
            }
            crate::app::app_state::SettingItem::UiRounding { enabled, radius_px } => {
                let current = if enabled && radius_px > 0.0 {
                    radius_px.round() as i32
                } else {
                    0
                };
                let next = (current + delta * 8).clamp(0, 24) as f32;
                let next_enabled = next > 0.0;
                if let Some(state) = self.app_state.active_settings_buffer_mut()
                    && let Some(crate::app::app_state::SettingItem::UiRounding {
                        enabled,
                        radius_px,
                    }) = state.selected_item_mut()
                {
                    *enabled = next_enabled;
                    *radius_px = next;
                }
                self.ui_config.border_radius_px = next;
                self.finalize_settings_change()
            }
            crate::app::app_state::SettingItem::UiScale { current } => {
                let base = current.unwrap_or(self.runtime_scale);
                let next = (base + delta as f32 * 0.05).clamp(0.5, 3.0);
                self.ui_config.window.scale_factor_override = Some(next);
                if let Some(state) = self.app_state.active_settings_buffer_mut()
                    && let Some(crate::app::app_state::SettingItem::UiScale { current }) =
                        state.selected_item_mut()
                {
                    *current = Some(next);
                }
                self.refresh_runtime_scale_from_window();
                self.finalize_settings_change()
            }
            crate::app::app_state::SettingItem::AiMaxTokens { current } => {
                let next = (current as i64 + delta as i64 * 16).clamp(1, 4096) as u32;
                self.apply_ai_number_adjust(
                    |ai| ai.set_inline_max_tokens(next),
                    |item| {
                        if let crate::app::app_state::SettingItem::AiMaxTokens { current } = item {
                            *current = next;
                        }
                    },
                    "max tokens",
                )
            }
            crate::app::app_state::SettingItem::AiPrefixChars { current } => {
                let next = (current as i64 + delta as i64 * 200).clamp(0, 32_000) as usize;
                self.apply_ai_number_adjust(
                    |ai| ai.set_inline_prefix_chars(next),
                    |item| {
                        if let crate::app::app_state::SettingItem::AiPrefixChars { current } = item
                        {
                            *current = next;
                        }
                    },
                    "prefix context",
                )
            }
            crate::app::app_state::SettingItem::AiSuffixChars { current } => {
                let next = (current as i64 + delta as i64 * 100).clamp(0, 16_000) as usize;
                self.apply_ai_number_adjust(
                    |ai| ai.set_inline_suffix_chars(next),
                    |item| {
                        if let crate::app::app_state::SettingItem::AiSuffixChars { current } = item
                        {
                            *current = next;
                        }
                    },
                    "suffix context",
                )
            }
            crate::app::app_state::SettingItem::AiDebounceMs { current } => {
                let next = (current as i64 + delta as i64 * 10).clamp(0, 5_000) as u64;
                self.apply_ai_number_adjust(
                    |ai| ai.set_inline_debounce_ms(next),
                    |item| {
                        if let crate::app::app_state::SettingItem::AiDebounceMs { current } = item {
                            *current = next;
                        }
                    },
                    "debounce",
                )
            }
            _ => false,
        }
    }

    /// Apply an h/l adjustment to a numeric AI field: persist via the config
    /// setter (active immediately, no reload), mirror onto the live item, and
    /// flag a relayout. Shares the persistence path with `commit_ai_number_edit`.
    fn apply_ai_number_adjust(
        &mut self,
        set: impl FnOnce(&mut crate::config::ai_config::AiConfig) -> Result<(), String>,
        update_item: impl FnOnce(&mut crate::app::app_state::SettingItem),
        label: &str,
    ) -> bool {
        let changed = self.commit_ai_number_edit(set, update_item, label);
        if changed {
            self.editor_needs_layout = true;
            self.editor_caret_needs_layout = false;
        }
        changed
    }

    /// Recompute runtime scaling with the live window scale factor — needed when
    /// `scale_factor_override` changes, since `apply_scaled_runtime_config` alone
    /// reuses the previously computed `runtime_scale`.
    fn refresh_runtime_scale_from_window(&mut self) {
        let scale_factor = self
            .window
            .as_ref()
            .map(|window| window.scale_factor())
            .unwrap_or(1.0);
        self.update_runtime_scaling_for_window(scale_factor);
    }

    pub(super) fn activate_selected_setting(&mut self) -> bool {
        if self.app_state.settings_is_editing() {
            return self.commit_settings_editing();
        }

        let Some(selected) = self
            .app_state
            .active_settings_buffer()
            .and_then(|state| state.selected_item())
            .cloned()
        else {
            return false;
        };

        match selected {
            crate::app::app_state::SettingItem::ThemeSelector { .. } => {
                self.handle_command(Command::OpenThemeSelector)
            }
            crate::app::app_state::SettingItem::IndentInsertSpaces { enabled } => {
                let next = !enabled;
                self.ui_config.indent.insert_spaces = next;
                self.app_state.set_indent_config(self.ui_config.indent);
                if let Some(state) = self.app_state.active_settings_buffer_mut()
                    && let Some(crate::app::app_state::SettingItem::IndentInsertSpaces { enabled }) =
                        state.selected_item_mut()
                {
                    *enabled = next;
                }
                let _ = self.ui_config.save_user_override();
                self.editor_needs_layout = true;
                self.editor_caret_needs_layout = false;
                true
            }
            crate::app::app_state::SettingItem::InlineSuggestion { enabled } => {
                let next = !enabled;
                if let Err(err) = self.ai_config.set_inline_completion_enabled(next) {
                    eprintln!("[settings] failed to save AI config: {err}");
                    return false;
                }
                if !next {
                    self.cancel_ai_inline_completion();
                    let _ = self.app_state.clear_inline_suggestion();
                }
                if let Some(state) = self.app_state.active_settings_buffer_mut()
                    && let Some(crate::app::app_state::SettingItem::InlineSuggestion { enabled }) =
                        state.selected_item_mut()
                {
                    *enabled = next;
                }
                self.editor_needs_layout = true;
                self.editor_caret_needs_layout = false;
                true
            }
            crate::app::app_state::SettingItem::LeetCodeAi { enabled } => {
                let next = !enabled;
                if let Err(err) = self.ai_config.set_leetcode_ai_enabled(next) {
                    eprintln!("[settings] failed to save LeetCode AI config: {err}");
                    return false;
                }
                if let Some(state) = self.app_state.active_settings_buffer_mut()
                    && let Some(crate::app::app_state::SettingItem::LeetCodeAi { enabled }) =
                        state.selected_item_mut()
                {
                    *enabled = next;
                }
                self.editor_needs_layout = true;
                self.editor_caret_needs_layout = false;
                true
            }
            crate::app::app_state::SettingItem::GitUiTarget { git_min } => {
                let next = !git_min;
                self.git_config.ui = if next {
                    crate::config::git_config::GitUi::GitMin
                } else {
                    crate::config::git_config::GitUi::Lazygit
                };
                if let Err(err) = self.git_config.save_ui_target() {
                    eprintln!("[settings] failed to save git ui target: {err}");
                    return false;
                }
                if let Some(state) = self.app_state.active_settings_buffer_mut()
                    && let Some(crate::app::app_state::SettingItem::GitUiTarget { git_min }) =
                        state.selected_item_mut()
                {
                    *git_min = next;
                }
                self.editor_needs_layout = true;
                self.editor_caret_needs_layout = false;
                true
            }
            crate::app::app_state::SettingItem::EnableOutline { enabled } => {
                let next = !enabled;
                self.ui_config.enable_outline = next;
                if let Some(state) = self.app_state.active_settings_buffer_mut()
                    && let Some(crate::app::app_state::SettingItem::EnableOutline { enabled }) =
                        state.selected_item_mut()
                {
                    *enabled = next;
                }
                let _ = self.ui_config.save_user_override();
                self.editor_needs_layout = true;
                self.editor_caret_needs_layout = false;
                true
            }
            crate::app::app_state::SettingItem::FontFamily { .. }
            | crate::app::app_state::SettingItem::FontSize { .. }
            | crate::app::app_state::SettingItem::LineHeight { .. }
            | crate::app::app_state::SettingItem::IndentTabWidth { .. }
            | crate::app::app_state::SettingItem::UiRounding { .. }
            | crate::app::app_state::SettingItem::SidebarWidth { .. }
            | crate::app::app_state::SettingItem::RightSidebarWidth { .. }
            | crate::app::app_state::SettingItem::BottomPanelHeight { .. }
            | crate::app::app_state::SettingItem::AiApiUrl { .. }
            | crate::app::app_state::SettingItem::AiModel { .. }
            | crate::app::app_state::SettingItem::AiApiKey { .. }
            | crate::app::app_state::SettingItem::AiEndpointKind { .. }
            | crate::app::app_state::SettingItem::AiMaxTokens { .. }
            | crate::app::app_state::SettingItem::AiPrefixChars { .. }
            | crate::app::app_state::SettingItem::AiSuffixChars { .. }
            | crate::app::app_state::SettingItem::AiDebounceMs { .. }
            | crate::app::app_state::SettingItem::LeetCodeAiApiUrl { .. }
            | crate::app::app_state::SettingItem::LeetCodeAiModel { .. }
            | crate::app::app_state::SettingItem::LeetCodeAiApiKey { .. }
            | crate::app::app_state::SettingItem::LeetCodeAiEndpointKind { .. }
            | crate::app::app_state::SettingItem::LeetCodeAiReasoningEffort { .. }
            | crate::app::app_state::SettingItem::UiScale { .. } => {
                let changed = self.app_state.settings_begin_editing();
                if changed {
                    if let Ok(result) = self
                        .app_state
                        .apply_mode_event(crate::core::mode::ModeEvent::EnterInsert)
                    {
                        let _ = result.changed;
                    }
                    self.editor_needs_layout = true;
                    self.editor_caret_needs_layout = false;
                }
                changed
            }
        }
    }

    /// Persist an AI inline-completion text field and mirror it onto the live
    /// settings item. The config setter mutates `self.ai_config` in place and
    /// saves to disk, so the change takes effect on the next keystroke with no
    /// reload. Returns whether the value was committed.
    fn commit_ai_text_edit(
        &mut self,
        value: String,
        set: impl FnOnce(&mut crate::config::ai_config::AiConfig, String) -> Result<(), String>,
        update_item: impl FnOnce(&mut crate::app::app_state::SettingItem, String),
        label: &str,
    ) -> bool {
        match set(&mut self.ai_config, value.clone()) {
            Ok(()) => {
                if let Some(state) = self.app_state.active_settings_buffer_mut()
                    && let Some(item) = state.selected_item_mut()
                {
                    update_item(item, value);
                }
                true
            }
            Err(err) => {
                eprintln!("[settings] failed to save AI {label}: {err}");
                false
            }
        }
    }

    /// Numeric counterpart to [`Self::commit_ai_text_edit`]; the clamped value is
    /// captured by the closures so callers keep parse/clamp logic at the call site.
    fn commit_ai_number_edit(
        &mut self,
        set: impl FnOnce(&mut crate::config::ai_config::AiConfig) -> Result<(), String>,
        update_item: impl FnOnce(&mut crate::app::app_state::SettingItem),
        label: &str,
    ) -> bool {
        match set(&mut self.ai_config) {
            Ok(()) => {
                if let Some(state) = self.app_state.active_settings_buffer_mut()
                    && let Some(item) = state.selected_item_mut()
                {
                    update_item(item);
                }
                true
            }
            Err(err) => {
                eprintln!("[settings] failed to save AI {label}: {err}");
                false
            }
        }
    }

    pub(super) fn commit_settings_editing(&mut self) -> bool {
        let Some((kind, draft)) = self
            .app_state
            .active_settings_buffer()
            .and_then(|state| state.editing.as_ref())
            .map(|editing| (editing.kind.clone(), editing.draft.clone()))
        else {
            return false;
        };

        let trimmed = draft.trim();
        let mut changed = false;

        match kind {
            crate::app::app_state::SettingsEditingKind::FontFamily => {
                self.base_theme.editor.font_family =
                    (!trimmed.is_empty()).then(|| trimmed.to_string());
                self.ui_config.editor.font_family =
                    (!trimmed.is_empty()).then(|| trimmed.to_string());
                if let Some(state) = self.app_state.active_settings_buffer_mut()
                    && let Some(crate::app::app_state::SettingItem::FontFamily { current }) =
                        state.selected_item_mut()
                {
                    *current = trimmed.to_string();
                }
                changed = true;
            }
            crate::app::app_state::SettingsEditingKind::FontSize => {
                if let Ok(value) = trimmed.parse::<f32>() {
                    let value = value.clamp(8.0, 40.0);
                    self.base_theme.editor.font_size = value;
                    self.ui_config.editor.font_size = value;
                    if let Some(state) = self.app_state.active_settings_buffer_mut()
                        && let Some(crate::app::app_state::SettingItem::FontSize { current }) =
                            state.selected_item_mut()
                    {
                        *current = value;
                    }
                    changed = true;
                }
            }
            crate::app::app_state::SettingsEditingKind::LineHeight => {
                if let Ok(value) = trimmed.parse::<f32>() {
                    let value = value.clamp(10.0, 64.0);
                    self.base_theme.editor.line_height = value;
                    self.ui_config.editor.line_height = value;
                    if let Some(state) = self.app_state.active_settings_buffer_mut()
                        && let Some(crate::app::app_state::SettingItem::LineHeight { current }) =
                            state.selected_item_mut()
                    {
                        *current = value;
                    }
                    changed = true;
                }
            }
            crate::app::app_state::SettingsEditingKind::SidebarWidth => {
                if let Ok(value) = trimmed.parse::<i32>() {
                    let value = value.clamp(160, 1280);
                    self.ui_config.docks.left.size_px = value as f32;
                    if let Some(state) = self.app_state.active_settings_buffer_mut()
                        && let Some(crate::app::app_state::SettingItem::SidebarWidth { current }) =
                            state.selected_item_mut()
                    {
                        *current = value;
                    }
                    changed = true;
                }
            }
            crate::app::app_state::SettingsEditingKind::RightSidebarWidth => {
                if let Ok(value) = trimmed.parse::<i32>() {
                    let value = value.clamp(180, 1440);
                    self.ui_config.docks.right.size_px = value as f32;
                    if let Some(state) = self.app_state.active_settings_buffer_mut()
                        && let Some(crate::app::app_state::SettingItem::RightSidebarWidth {
                            current,
                        }) = state.selected_item_mut()
                    {
                        *current = value;
                    }
                    changed = true;
                }
            }
            crate::app::app_state::SettingsEditingKind::BottomPanelHeight => {
                if let Ok(value) = trimmed.parse::<i32>() {
                    let value = value.clamp(120, 1040);
                    self.ui_config.docks.bottom.size_px = value as f32;
                    if let Some(state) = self.app_state.active_settings_buffer_mut()
                        && let Some(crate::app::app_state::SettingItem::BottomPanelHeight {
                            current,
                        }) = state.selected_item_mut()
                    {
                        *current = value;
                    }
                    changed = true;
                }
            }
            crate::app::app_state::SettingsEditingKind::IndentTabWidth => {
                if let Ok(value) = trimmed.parse::<u8>() {
                    let value = value.clamp(1, 8);
                    self.ui_config.indent.tab_width = value;
                    self.app_state.set_indent_config(self.ui_config.indent);
                    if let Some(state) = self.app_state.active_settings_buffer_mut()
                        && let Some(crate::app::app_state::SettingItem::IndentTabWidth { current }) =
                            state.selected_item_mut()
                    {
                        *current = value;
                    }
                    changed = true;
                }
            }
            crate::app::app_state::SettingsEditingKind::UiRounding => {
                if let Ok(value) = trimmed.parse::<f32>() {
                    let value = value.clamp(0.0, 24.0).round();
                    self.ui_config.border_radius_px = value;
                    if let Some(state) = self.app_state.active_settings_buffer_mut()
                        && let Some(crate::app::app_state::SettingItem::UiRounding {
                            enabled,
                            radius_px,
                        }) = state.selected_item_mut()
                    {
                        *enabled = value > 0.0;
                        *radius_px = value;
                    }
                    changed = true;
                }
            }
            crate::app::app_state::SettingsEditingKind::UiScale => {
                let next = if trimmed.is_empty() || trimmed.eq_ignore_ascii_case("auto") {
                    Some(None)
                } else {
                    trimmed
                        .parse::<f32>()
                        .ok()
                        .map(|value| Some(value.clamp(0.5, 3.0)))
                };
                if let Some(next) = next {
                    self.ui_config.window.scale_factor_override = next;
                    if let Some(state) = self.app_state.active_settings_buffer_mut()
                        && let Some(crate::app::app_state::SettingItem::UiScale { current }) =
                            state.selected_item_mut()
                    {
                        *current = next;
                    }
                    self.refresh_runtime_scale_from_window();
                    changed = true;
                }
            }
            crate::app::app_state::SettingsEditingKind::AiApiUrl => {
                changed = self.commit_ai_text_edit(
                    trimmed.to_string(),
                    |ai, value| ai.set_inline_api_url(value),
                    |item, value| {
                        if let crate::app::app_state::SettingItem::AiApiUrl { current } = item {
                            *current = value;
                        }
                    },
                    "endpoint",
                );
            }
            crate::app::app_state::SettingsEditingKind::AiModel => {
                changed = self.commit_ai_text_edit(
                    trimmed.to_string(),
                    |ai, value| ai.set_inline_model(value),
                    |item, value| {
                        if let crate::app::app_state::SettingItem::AiModel { current } = item {
                            *current = value;
                        }
                    },
                    "model",
                );
            }
            crate::app::app_state::SettingsEditingKind::AiApiKey => {
                changed = self.commit_ai_text_edit(
                    trimmed.to_string(),
                    |ai, value| ai.set_inline_api_key(value),
                    |item, value| {
                        if let crate::app::app_state::SettingItem::AiApiKey { current } = item {
                            *current = value;
                        }
                    },
                    "API key",
                );
            }
            crate::app::app_state::SettingsEditingKind::AiEndpointKind => {
                changed = self.commit_ai_text_edit(
                    trimmed.to_string(),
                    |ai, value| ai.set_inline_endpoint_kind(value),
                    |item, value| {
                        if let crate::app::app_state::SettingItem::AiEndpointKind { current } = item
                        {
                            *current = value;
                        }
                    },
                    "endpoint kind",
                );
            }
            crate::app::app_state::SettingsEditingKind::AiMaxTokens => {
                if let Ok(value) = trimmed.parse::<u32>() {
                    let value = value.clamp(1, 4096);
                    changed = self.commit_ai_number_edit(
                        |ai| ai.set_inline_max_tokens(value),
                        |item| {
                            if let crate::app::app_state::SettingItem::AiMaxTokens { current } =
                                item
                            {
                                *current = value;
                            }
                        },
                        "max tokens",
                    );
                }
            }
            crate::app::app_state::SettingsEditingKind::AiPrefixChars => {
                if let Ok(value) = trimmed.parse::<usize>() {
                    let value = value.min(32_000);
                    changed = self.commit_ai_number_edit(
                        |ai| ai.set_inline_prefix_chars(value),
                        |item| {
                            if let crate::app::app_state::SettingItem::AiPrefixChars { current } =
                                item
                            {
                                *current = value;
                            }
                        },
                        "prefix context",
                    );
                }
            }
            crate::app::app_state::SettingsEditingKind::AiSuffixChars => {
                if let Ok(value) = trimmed.parse::<usize>() {
                    let value = value.min(16_000);
                    changed = self.commit_ai_number_edit(
                        |ai| ai.set_inline_suffix_chars(value),
                        |item| {
                            if let crate::app::app_state::SettingItem::AiSuffixChars { current } =
                                item
                            {
                                *current = value;
                            }
                        },
                        "suffix context",
                    );
                }
            }
            crate::app::app_state::SettingsEditingKind::AiDebounceMs => {
                if let Ok(value) = trimmed.parse::<u64>() {
                    let value = value.clamp(0, 5_000);
                    changed = self.commit_ai_number_edit(
                        |ai| ai.set_inline_debounce_ms(value),
                        |item| {
                            if let crate::app::app_state::SettingItem::AiDebounceMs { current } =
                                item
                            {
                                *current = value;
                            }
                        },
                        "debounce",
                    );
                }
            }
            crate::app::app_state::SettingsEditingKind::LeetCodeAiApiUrl => {
                changed = self.commit_ai_text_edit(
                    trimmed.to_string(),
                    |ai, value| ai.set_leetcode_api_url(value),
                    |item, value| {
                        if let crate::app::app_state::SettingItem::LeetCodeAiApiUrl { current } =
                            item
                        {
                            *current = value;
                        }
                    },
                    "LeetCode API URL",
                );
            }
            crate::app::app_state::SettingsEditingKind::LeetCodeAiModel => {
                changed = self.commit_ai_text_edit(
                    trimmed.to_string(),
                    |ai, value| ai.set_leetcode_model(value),
                    |item, value| {
                        if let crate::app::app_state::SettingItem::LeetCodeAiModel { current } =
                            item
                        {
                            *current = value;
                        }
                    },
                    "LeetCode Model",
                );
            }
            crate::app::app_state::SettingsEditingKind::LeetCodeAiApiKey => {
                changed = self.commit_ai_text_edit(
                    trimmed.to_string(),
                    |ai, value| ai.set_leetcode_api_key(value),
                    |item, value| {
                        if let crate::app::app_state::SettingItem::LeetCodeAiApiKey { current } =
                            item
                        {
                            *current = value;
                        }
                    },
                    "LeetCode API Key",
                );
            }
            crate::app::app_state::SettingsEditingKind::LeetCodeAiEndpointKind => {
                changed = self.commit_ai_text_edit(
                    trimmed.to_string(),
                    |ai, value| ai.set_leetcode_endpoint_kind(value),
                    |item, value| {
                        if let crate::app::app_state::SettingItem::LeetCodeAiEndpointKind {
                            current,
                        } = item
                        {
                            *current = value;
                        }
                    },
                    "LeetCode Endpoint Kind",
                );
            }
            crate::app::app_state::SettingsEditingKind::LeetCodeAiReasoningEffort => {
                changed = self.commit_ai_text_edit(
                    trimmed.to_string(),
                    |ai, value| ai.set_leetcode_reasoning_effort(value),
                    |item, value| {
                        if let crate::app::app_state::SettingItem::LeetCodeAiReasoningEffort {
                            current,
                        } = item
                        {
                            *current = value;
                        }
                    },
                    "LeetCode Reasoning Effort",
                );
            }
        }

        if changed {
            self.apply_scaled_runtime_config();
            let _ = self.ui_config.save_user_override();
        }
        let cancelled = self.app_state.settings_cancel_editing();
        if changed || cancelled {
            if self.app_state.current_mode() == crate::core::mode::EditorMode::Insert {
                if let Ok(result) = self
                    .app_state
                    .apply_mode_event(crate::core::mode::ModeEvent::Escape)
                {
                    let _ = result.changed;
                }
            }
            self.editor_needs_layout = true;
            self.editor_caret_needs_layout = false;
        }
        changed || cancelled
    }
}
