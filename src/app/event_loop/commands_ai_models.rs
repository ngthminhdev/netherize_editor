//! Settings → "Inline Model" / "LeetCode Model": pick a model from the shared
//! AI endpoint's `/models` catalog instead of typing an id by hand.

use super::*;
use crate::{
    app::{
        app_state::{AiModelTarget, SettingItem},
        command_palette::{CommandPaletteAction, CommandPaletteItem, CommandPaletteMode},
    },
    async_runtime::message::AiModelInfo,
};

impl AppShell {
    /// Open the picker (spinner) and ask the worker for the catalog. Without an
    /// endpoint the row falls back to the plain text edit so it is never a dead
    /// end.
    pub(super) fn open_ai_model_picker(&mut self, target: AiModelTarget) -> bool {
        let shared_url = self.ai_config.provider_api_url();
        let endpoint = if shared_url.trim().is_empty() {
            self.ai_config
                .resolve(target.feature())
                .map(|provider| (provider.api_url, provider.api_key))
        } else {
            let key = self.ai_config.provider_api_key();
            Some((shared_url, (!key.trim().is_empty()).then_some(key)))
        };
        let Some((api_url, api_key)) = endpoint else {
            self.show_transient_toast_kind(
                "AI Endpoint is not set\nFill AI Endpoint + API Key first, or type a model id."
                    .to_string(),
                ToastKind::Error,
            );
            return self.begin_settings_text_edit();
        };
        let current_mode = self.app_state.current_mode();
        if current_mode != EditorMode::PaletteFocus
            && !self.app_state.can_apply_mode_event(ModeEvent::OpenPalette)
        {
            return self.begin_settings_text_edit();
        }
        self.app_state.open_ai_model_picker_palette(Vec::new(), true);
        if current_mode != EditorMode::PaletteFocus
            && let Err(err) = self.app_state.apply_mode_event(ModeEvent::OpenPalette)
        {
            let _ = self.app_state.close_command_palette();
            eprintln!("[AppShell] ai model picker mode change failed: {err:?}");
            return false;
        }
        self.pending_ai_model_target = Some(target);
        self.arm_palette_ime_commit_suppression();
        if self.focus_manager.set(FocusTarget::OverlayLayer) {
            self.input_handler.clear_pending_prefix();
        }
        let _ = self.submit(RequestSpec {
            revision_id: 0,
            topic: RequestTopic::AiModels,
            payload: WorkerRequestPayload::AiListModels { api_url, api_key },
        });
        true
    }

    /// The text-edit path of a settings row (same as Enter on any text row).
    fn begin_settings_text_edit(&mut self) -> bool {
        let changed = self.app_state.settings_begin_editing();
        if changed {
            let _ = self.app_state.apply_mode_event(ModeEvent::EnterInsert);
            self.editor_needs_layout = true;
            self.editor_caret_needs_layout = false;
        }
        changed
    }

    /// Catalog arrived: fill the open picker, current model first so a bare
    /// Enter keeps it.
    pub(in crate::app::event_loop) fn on_ai_models_listed(&mut self, models: Vec<AiModelInfo>) {
        let Some(target) = self.pending_ai_model_target else {
            return;
        };
        let current = self.ai_config.feature_model(target.feature());
        let mut items: Vec<CommandPaletteItem> = models
            .iter()
            .map(|model| CommandPaletteItem::ai_model(model, model.id == current))
            .collect();
        if let Some(pos) = items.iter().position(|item| item.label == current) {
            let head = items.remove(pos);
            items.insert(0, head);
        }
        if self.app_state.replace_ai_model_picker_items(items) {
            self.request_redraw();
        }
    }

    /// `/models` failed (no key, offline, wrong url): close the picker, say
    /// why, and drop into the text edit so the id can still be typed.
    pub(in crate::app::event_loop) fn on_ai_model_list_failed(&mut self, message: String) {
        if self.app_state.command_palette_mode() != Some(CommandPaletteMode::AiModelPicker) {
            return;
        }
        self.pending_ai_model_target = None;
        self.close_ai_model_picker();
        self.show_transient_toast_kind(
            format!("Could not list models\n{message}"),
            ToastKind::Error,
        );
        let _ = self.begin_settings_text_edit();
        self.request_redraw();
    }

    fn close_ai_model_picker(&mut self) {
        let _ = self.app_state.close_command_palette();
        if self.app_state.current_mode() == EditorMode::PaletteFocus {
            let _ = self.app_state.apply_mode_event(ModeEvent::ExitFocus);
        }
        self.focus_manager.set(FocusTarget::CenterEditor);
        self.input_handler.clear_pending_prefix();
    }

    /// Enter in the picker: persist the model for the pending target and
    /// mirror it onto the settings row.
    pub(super) fn confirm_ai_model_selection(&mut self) -> bool {
        let Some(CommandPaletteAction::SelectAiModel(model)) =
            self.app_state.command_palette_selected_action()
        else {
            // Still loading or nothing matched: keep the picker open.
            return false;
        };
        let Some(target) = self.pending_ai_model_target.take() else {
            self.close_ai_model_picker();
            return true;
        };
        self.close_ai_model_picker();
        let saved = match target {
            AiModelTarget::Inline => self.ai_config.set_inline_model(model.clone()),
            AiModelTarget::LeetCode => self.ai_config.set_leetcode_model(model.clone()),
        };
        if let Err(err) = saved {
            self.show_transient_toast_kind(
                format!("Could not save model\n{err}"),
                ToastKind::Error,
            );
            return true;
        }
        if let Some(state) = self.app_state.active_settings_buffer_mut() {
            for item in &mut state.items {
                match (target, item) {
                    (AiModelTarget::Inline, SettingItem::AiModel { current })
                    | (AiModelTarget::LeetCode, SettingItem::LeetCodeAiModel { current }) => {
                        *current = model.clone();
                    }
                    _ => {}
                }
            }
        }
        self.editor_needs_layout = true;
        self.editor_caret_needs_layout = false;
        self.show_transient_toast_kind(
            format!("{}\n{model}", target.label()),
            ToastKind::Success,
        );
        true
    }
}
