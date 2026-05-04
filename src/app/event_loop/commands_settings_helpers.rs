use super::*;

impl AppShell {
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
                crate::app::app_state::SettingItem::SidebarWidth { current } => {
                    let next = (current + delta * 20).clamp(160, 640);
                    self.update_active_settings_edit_draft(next.to_string())
                }
                crate::app::app_state::SettingItem::RightSidebarWidth { current } => {
                    let next = (current + delta * 20).clamp(180, 720);
                    self.update_active_settings_edit_draft(next.to_string())
                }
                crate::app::app_state::SettingItem::BottomPanelHeight { current } => {
                    let next = (current + delta * 20).clamp(120, 520);
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
                let next = (current + delta * 20).clamp(160, 640);
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
                let next = (current + delta * 20).clamp(180, 720);
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
                let next = (current + delta * 20).clamp(120, 520);
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
            _ => false,
        }
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
            crate::app::app_state::SettingItem::InlineSuggestion { .. } => false,
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
            | crate::app::app_state::SettingItem::SidebarWidth { .. }
            | crate::app::app_state::SettingItem::RightSidebarWidth { .. }
            | crate::app::app_state::SettingItem::BottomPanelHeight { .. } => {
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
            crate::app::app_state::SettingItem::UiRounding { enabled, radius_px } => {
                let next_radius = if !enabled || radius_px <= 0.0 {
                    8.0
                } else if radius_px < 12.0 {
                    16.0
                } else {
                    0.0
                };
                let next_enabled = next_radius > 0.0;
                if let Some(state) = self.app_state.active_settings_buffer_mut()
                    && let Some(crate::app::app_state::SettingItem::UiRounding {
                        enabled,
                        radius_px,
                    }) = state.selected_item_mut()
                {
                    *enabled = next_enabled;
                    *radius_px = next_radius;
                }
                self.ui_config.border_radius_px = next_radius;
                self.apply_scaled_runtime_config();
                let _ = self.ui_config.save_user_override();
                true
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
                    let value = value.clamp(160, 640);
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
                    let value = value.clamp(180, 720);
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
                    let value = value.clamp(120, 520);
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
