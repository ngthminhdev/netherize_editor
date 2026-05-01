use super::*;

impl AppShell {
    pub(super) fn handle_settings_command(&mut self, command: &Command) -> Option<bool> {
        match command {
            Command::OpenSettings => {
                let theme_profile = self
                    .persistent_state
                    .configured_theme_profile()
                    .unwrap_or(self.base_theme.name.as_str())
                    .to_string();
                let font_family = self
                    .base_theme
                    .editor
                    .font_family
                    .clone()
                    .unwrap_or_default();
                self.app_state.open_settings_buffer(
                    theme_profile,
                    font_family,
                    self.base_theme.editor.font_size,
                    self.base_theme.editor.line_height,
                    self.ui_config.indent.tab_width,
                    self.ui_config.indent.insert_spaces,
                    self.ui_config.docks.left.size_px.round() as i32,
                    self.ui_config.docks.right.size_px.round() as i32,
                    self.ui_config.docks.bottom.size_px.round() as i32,
                    self.ui_config.border_radius_px > 0.0,
                    self.ui_config.border_radius_px,
                );
                let _ = self.sync_focus_mode_for_active_buffer();
                self.editor_needs_layout = true;
                self.editor_caret_needs_layout = false;
                Some(true)
            }
            Command::SettingsSelectNext => {
                let changed = self.app_state.settings_select_next();
                if changed {
                    self.editor_needs_layout = true;
                    self.editor_caret_needs_layout = false;
                }
                Some(changed)
            }
            Command::SettingsSelectPrev => {
                let changed = self.app_state.settings_select_prev();
                if changed {
                    self.editor_needs_layout = true;
                    self.editor_caret_needs_layout = false;
                }
                Some(changed)
            }
            Command::SettingsAdjustDecrease => Some(self.adjust_selected_setting(-1)),
            Command::SettingsAdjustIncrease => Some(self.adjust_selected_setting(1)),
            Command::SettingsActivate => Some(self.activate_selected_setting()),
            Command::CloseFilePicker if self.app_state.active_buffer_is_settings() => {
                if self.app_state.settings_is_editing() {
                    let changed = self.app_state.settings_cancel_editing();
                    if changed {
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
                    Some(changed)
                } else {
                    Some(self.close_current_buffer_now())
                }
            }
            Command::FilePickerAppendQuery(_)
            | Command::FilePickerBackspaceQuery
            | Command::EditorPaste
            | Command::PasteSystemClipboard
                if self.app_state.active_buffer_is_settings()
                    && self.app_state.settings_is_editing() =>
            {
                let changed = match command {
                    Command::FilePickerAppendQuery(text) => {
                        self.app_state.settings_append_editing_text(text)
                    }
                    Command::FilePickerBackspaceQuery => {
                        self.app_state.settings_backspace_editing()
                    }
                    Command::EditorPaste | Command::PasteSystemClipboard => {
                        if let Ok(text) = self.clipboard.get_text() {
                            self.app_state.settings_append_editing_text(&text)
                        } else {
                            false
                        }
                    }
                    _ => false,
                };
                if changed {
                    self.editor_needs_layout = true;
                    self.editor_caret_needs_layout = false;
                }
                Some(changed)
            }
            _ => None,
        }
    }
}
