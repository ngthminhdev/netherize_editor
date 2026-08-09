use super::*;

impl AppShell {
    pub(super) fn begin_theme_picker_preview_session(&mut self) {
        if self.theme_picker_original_theme.is_none() {
            self.theme_picker_original_theme = Some(self.base_theme.clone());
        }
        self.theme_picker_preview_profile = None;
    }

    pub(super) fn preview_selected_theme_from_picker(&mut self) -> bool {
        let Some(crate::app::command_palette::CommandPaletteAction::SelectTheme(theme_profile)) =
            self.app_state.command_palette_selected_action()
        else {
            return false;
        };
        if self.theme_picker_preview_profile.as_deref() == Some(theme_profile.as_str()) {
            return false;
        }
        if self.theme_picker_original_theme.is_none() {
            self.theme_picker_original_theme = Some(self.base_theme.clone());
        }
        let loaded_theme = match ThemeConfig::load(&theme_profile) {
            Ok(theme) => theme,
            Err(err) => {
                eprintln!(
                    "[AppShell] theme preview failed for profile '{}': {err}",
                    theme_profile
                );
                return false;
            }
        };
        self.base_theme = loaded_theme;
        self.theme_picker_preview_profile = Some(theme_profile);
        self.apply_scaled_runtime_config();
        self.leap_state = None;
        if let Some(renderer) = self.renderer.as_mut() {
            renderer.clear_leap_labels();
        }
        true
    }

    pub(super) fn restore_theme_picker_preview(&mut self) -> bool {
        let Some(original_theme) = self.theme_picker_original_theme.take() else {
            self.theme_picker_preview_profile = None;
            return false;
        };
        self.theme_picker_preview_profile = None;
        self.base_theme = original_theme;
        self.apply_scaled_runtime_config();
        self.leap_state = None;
        if let Some(renderer) = self.renderer.as_mut() {
            renderer.clear_leap_labels();
        }
        true
    }

    pub(super) fn pending_confirmation_prompt(&self) -> Option<String> {
        match &self.pending_confirmation.as_ref()?.action {
            PendingConfirmationAction::Delete { path, .. } => {
                let label = path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .map(str::to_string)
                    .unwrap_or_else(|| path.to_string_lossy().into_owned());
                Some(format!("Delete {label}? (y/n)"))
            }
            PendingConfirmationAction::CloseDirtyBuffer { path } => {
                let label = path
                    .as_ref()
                    .and_then(|path| path.file_name())
                    .and_then(|name| name.to_str())
                    .map(str::to_string)
                    .unwrap_or_else(|| "current buffer".to_string());
                Some(format!("Save changes to {label} before closing? (y/n)"))
            }
            PendingConfirmationAction::QuitDirtyBuffers { count } => {
                let noun = if *count == 1 { "file" } else { "files" };
                Some(format!(
                    "Save changes to {count} {noun} before quitting? (y/n)"
                ))
            }
            PendingConfirmationAction::ExternalOverwrite { path } => {
                let label = path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .map(str::to_string)
                    .unwrap_or_else(|| path.to_string_lossy().into_owned());
                Some(format!(
                    "{label} changed externally while dirty. Overwrite with current buffer? (y/n)"
                ))
            }
        }
    }

    pub(super) fn begin_explorer_delete_confirmation(&mut self) -> bool {
        self.ensure_explorer_snapshot();
        if self.explorer_snapshot.entries.is_empty() {
            self.explorer_cursor = 0;
            return false;
        }
        self.explorer_cursor = self
            .explorer_cursor
            .min(self.explorer_snapshot.entries.len().saturating_sub(1));
        let selected = self.explorer_snapshot.entries[self.explorer_cursor].clone();
        self.pending_confirmation = Some(PendingConfirmation {
            action: PendingConfirmationAction::Delete {
                path: selected.path,
                file_type: selected.file_type,
            },
            return_focus: FocusTarget::LeftSidebar,
        });
        let prompt = self.pending_confirmation_prompt().unwrap_or_default();
        if !self.open_prompt_overlay(
            crate::app::command_palette::CommandPaletteMode::ExplorerDeleteConfirm,
        ) {
            self.pending_confirmation = None;
            return false;
        }
        if let Err(err) = self.app_state.set_command_palette_query(&prompt) {
            eprintln!("[AppShell] delete confirmation prompt failed: {err}");
            self.pending_confirmation = None;
            let _ = self.app_state.close_command_palette();
            let _ = self.app_state.apply_mode_event(ModeEvent::ExitFocus);
            let _ = self.focus_manager.set(FocusTarget::LeftSidebar);
            return false;
        }
        true
    }

    pub(super) fn begin_dirty_buffer_close_confirmation(&mut self) -> bool {
        self.pending_confirmation = Some(PendingConfirmation {
            action: PendingConfirmationAction::CloseDirtyBuffer {
                path: self.app_state.active_file().map(PathBuf::from),
            },
            return_focus: FocusTarget::CenterEditor,
        });
        let prompt = self.pending_confirmation_prompt().unwrap_or_default();
        if !self.open_prompt_overlay(CommandPaletteMode::BufferCloseConfirm) {
            self.pending_confirmation = None;
            return false;
        }
        if let Err(err) = self.app_state.set_command_palette_query(&prompt) {
            eprintln!("[AppShell] dirty close confirmation prompt failed: {err}");
            self.pending_confirmation = None;
            let _ = self.app_state.close_command_palette();
            let _ = self.app_state.apply_mode_event(ModeEvent::ExitFocus);
            let _ = self.focus_manager.set(FocusTarget::CenterEditor);
            return false;
        }
        true
    }

    pub(in crate::app::event_loop) fn begin_dirty_window_quit_confirmation(
        &mut self,
        count: usize,
    ) -> bool {
        if count == 0 {
            self.exit_requested = true;
            return true;
        }
        self.pending_confirmation = Some(PendingConfirmation {
            action: PendingConfirmationAction::QuitDirtyBuffers { count },
            return_focus: self.focus_manager.current(),
        });
        let prompt = self.pending_confirmation_prompt().unwrap_or_default();
        if !self.open_prompt_overlay(CommandPaletteMode::BufferCloseConfirm) {
            self.pending_confirmation = None;
            return false;
        }
        if let Err(err) = self.app_state.set_command_palette_query(&prompt) {
            eprintln!("[AppShell] dirty quit confirmation prompt failed: {err}");
            self.pending_confirmation = None;
            let _ = self.app_state.close_command_palette();
            let _ = self.app_state.apply_mode_event(ModeEvent::ExitFocus);
            return false;
        }
        true
    }

    fn save_all_dirty_buffers_for_quit(&mut self) -> Result<usize, String> {
        let mut saved = 0usize;
        if self.app_state.canvas_edit_session_is_dirty()
            && self
                .app_state
                .canvas_save_edit_session()
                .map_err(|err| format!("canvas edit: {err}"))?
        {
            saved = saved.saturating_add(1);
        }
        if self.app_state.active_buffer_index().is_none() && self.app_state.is_dirty() {
            let _ = self.handle_command(Command::SaveFile);
            if self.app_state.is_dirty() {
                return Err("untitled buffer remained dirty after save".to_string());
            }
            saved = saved.saturating_add(1);
        }
        let buffer_count = self.app_state.buffers().len();
        for index in 0..buffer_count {
            if self.app_state.is_dirty() {
                let _ = self.handle_command(Command::SaveFile);
                if self.app_state.is_dirty() {
                    return Err(format!(
                        "buffer {} remained dirty after save",
                        index.saturating_add(1)
                    ));
                }
                saved = saved.saturating_add(1);
            }
            if buffer_count > 1 {
                let _ = self.handle_command(Command::BufferNext);
            }
        }
        Ok(saved)
    }

    pub(in crate::app::event_loop) fn begin_external_overwrite_confirmation(
        &mut self,
        path: PathBuf,
    ) -> bool {
        self.pending_confirmation = Some(PendingConfirmation {
            action: PendingConfirmationAction::ExternalOverwrite { path },
            return_focus: FocusTarget::CenterEditor,
        });
        let prompt = self.pending_confirmation_prompt().unwrap_or_default();
        // Reuse the same confirmation overlay style as Explorer delete.
        if !self.open_prompt_overlay(CommandPaletteMode::ExplorerDeleteConfirm) {
            self.pending_confirmation = None;
            return false;
        }
        if let Err(err) = self.app_state.set_command_palette_query(&prompt) {
            eprintln!("[AppShell] external overwrite prompt failed: {err}");
            self.pending_confirmation = None;
            let _ = self.app_state.close_command_palette();
            let _ = self.app_state.apply_mode_event(ModeEvent::ExitFocus);
            let _ = self.focus_manager.set(FocusTarget::CenterEditor);
            return false;
        }
        true
    }

    pub(super) fn close_pending_confirmation_overlay(&mut self, focus_target: FocusTarget) -> bool {
        let mut changed = self.app_state.close_command_palette();
        if self.app_state.current_mode() == EditorMode::PaletteFocus
            && let Ok(result) = self.app_state.apply_mode_event(ModeEvent::ExitFocus)
        {
            changed |= result.changed;
        }
        let focus_changed = self.focus_manager.set(focus_target);
        changed |= focus_changed;
        if focus_changed {
            self.input_handler.clear_pending_prefix();
        }
        changed
    }

    pub(super) fn open_prompt_overlay(&mut self, mode: CommandPaletteMode) -> bool {
        let current_mode = self.app_state.current_mode();
        if current_mode != EditorMode::PaletteFocus
            && !self.app_state.can_apply_mode_event(ModeEvent::OpenPalette)
        {
            return false;
        }

        if let Err(err) = self.app_state.open_command_palette_mode(mode) {
            eprintln!("[AppShell] prompt overlay open failed: {err}");
            return false;
        }

        if current_mode != EditorMode::PaletteFocus
            && let Err(err) = self.app_state.apply_mode_event(ModeEvent::OpenPalette)
        {
            let _ = self.app_state.close_command_palette();
            eprintln!("[AppShell] explorer prompt mode change failed: {err:?}");
            return false;
        }

        let focus_changed = self.focus_manager.set(FocusTarget::OverlayLayer);
        if focus_changed {
            self.input_handler.clear_pending_prefix();
        }
        true
    }

    pub(super) fn confirm_explorer_prompt(&mut self) -> bool {
        let Some(mode) = self.app_state.command_palette_mode() else {
            return false;
        };
        let target_path = match mode {
            CommandPaletteMode::ExplorerCreateFile | CommandPaletteMode::ExplorerCreateFolder => {
                let Some(target_path) = self.resolve_explorer_creation_target() else {
                    return false;
                };

                let create_result = match mode {
                    CommandPaletteMode::ExplorerCreateFile => {
                        if let Some(parent) = target_path.parent()
                            && let Err(err) = std::fs::create_dir_all(parent)
                        {
                            eprintln!(
                                "[AppShell] explorer create parent directories failed for {}: {err}",
                                target_path.display()
                            );
                            return false;
                        }
                        std::fs::OpenOptions::new()
                            .write(true)
                            .create_new(true)
                            .open(&target_path)
                            .map(|_| ())
                    }
                    CommandPaletteMode::ExplorerCreateFolder => {
                        std::fs::create_dir_all(&target_path)
                    }
                    _ => unreachable!(),
                };

                if let Err(err) = create_result {
                    eprintln!(
                        "[AppShell] explorer create failed for {}: {err}",
                        target_path.display()
                    );
                    return false;
                }
                target_path
            }
            CommandPaletteMode::ExplorerRenameFull | CommandPaletteMode::ExplorerRenameBase => {
                let Some(old_path) = self
                    .app_state
                    .pending_explorer_rename_path()
                    .map(PathBuf::from)
                else {
                    return false;
                };
                let Some(parent) = old_path.parent().map(PathBuf::from) else {
                    return false;
                };
                let new_name = self.app_state.command_palette_query_text().trim();
                if new_name.is_empty()
                    || new_name.contains(std::path::MAIN_SEPARATOR)
                    || new_name.contains('/')
                    || new_name.contains('\\')
                {
                    return false;
                }
                let new_path = parent.join(new_name);
                if new_path == old_path || new_path.exists() {
                    return false;
                }
                if let Err(err) = std::fs::rename(&old_path, &new_path) {
                    eprintln!(
                        "[AppShell] explorer rename failed from {} to {}: {err}",
                        old_path.display(),
                        new_path.display()
                    );
                    return false;
                }
                let _ = self.app_state.set_pending_explorer_rename_path(None);
                new_path
            }
            CommandPaletteMode::ExplorerPasteFile => {
                let Some(source_path) = self.pending_paste_source_path.take() else {
                    return false;
                };
                let Some(target_dir) = self.pending_paste_target_dir.take() else {
                    return false;
                };

                let new_name = self.app_state.command_palette_query_text().trim();
                if new_name.is_empty() {
                    self.show_transient_toast("Paste name cannot be empty".to_string());
                    // Restore state so the user can retry.
                    self.pending_paste_source_path = Some(source_path);
                    self.pending_paste_target_dir = Some(target_dir);
                    return false;
                }
                if new_name.contains(std::path::MAIN_SEPARATOR)
                    || new_name.contains('/')
                    || new_name.contains('\\')
                {
                    self.show_transient_toast("Invalid file name".to_string());
                    self.pending_paste_source_path = Some(source_path);
                    self.pending_paste_target_dir = Some(target_dir);
                    return false;
                }

                let target_path = target_dir.join(new_name);
                if target_path.exists() {
                    self.show_transient_toast("File already exists".to_string());
                    self.pending_paste_source_path = Some(source_path);
                    self.pending_paste_target_dir = Some(target_dir);
                    return false;
                }

                self.submit(RequestSpec {
                    revision_id: 0,
                    topic: RequestTopic::FileOperation,
                    payload: WorkerRequestPayload::CopyFile {
                        source_path,
                        target_path: target_path.clone(),
                    },
                });
                target_path
            }
            _ => return false,
        };

        if let Err(err) = self.app_state.rescan_workspace() {
            eprintln!(
                "[AppShell] workspace rescan failed after explorer prompt confirm for {}: {err}",
                target_path.display()
            );
        }
        self.explorer_reveal_file(&target_path);

        let mut changed = self.app_state.close_command_palette();
        if let Ok(result) = self.app_state.apply_mode_event(ModeEvent::ExitFocus) {
            changed |= result.changed;
        }
        let focus_changed = self.focus_manager.set(FocusTarget::LeftSidebar);
        changed |= focus_changed;
        if focus_changed {
            self.input_handler.clear_pending_prefix();
        }
        self.sidebar_needs_layout = true;
        changed
    }

    pub(super) fn resolve_explorer_creation_target(&mut self) -> Option<PathBuf> {
        let raw_name = self
            .app_state
            .command_palette_query_text()
            .trim()
            .to_string();
        if raw_name.is_empty() {
            return None;
        }

        self.ensure_explorer_snapshot();
        let root = self.app_state.workspace_root_path()?.to_path_buf();
        let base_dir = self
            .explorer_snapshot
            .entries
            .get(self.explorer_cursor)
            .map(|entry| {
                if entry.file_type == WorkspaceNodeType::Folder {
                    entry.path.clone()
                } else {
                    entry
                        .path
                        .parent()
                        .map(PathBuf::from)
                        .unwrap_or_else(|| root.clone())
                }
            })
            .unwrap_or_else(|| root.clone());

        let target_path = if Path::new(&raw_name).is_absolute() {
            PathBuf::from(&raw_name)
        } else {
            base_dir.join(&raw_name)
        };

        if !target_path.starts_with(&root) {
            eprintln!(
                "[AppShell] explorer create target must stay inside workspace root: {}",
                target_path.display()
            );
            return None;
        }

        Some(target_path)
    }

    pub(super) fn open_folder_with_dialog(&mut self) -> bool {
        let Some(folder) = rfd::FileDialog::new().pick_folder() else {
            return false;
        };

        self.switch_workspace_to(folder)
    }

    pub(super) fn open_recent_projects_palette(&mut self) -> bool {
        let recent = self.persistent_state.recent_projects.clone();
        if recent.is_empty() {
            self.show_transient_toast(
                "No recent projects. Use Ctrl+O to open a folder.".to_string(),
            );
            return false;
        }

        let current_mode = self.app_state.current_mode();
        if current_mode != EditorMode::PaletteFocus
            && !self.app_state.can_apply_mode_event(ModeEvent::OpenPalette)
        {
            return false;
        }

        if let Err(err) = self.app_state.open_recent_projects_palette_with_meta(
            &recent,
            &self.persistent_state.recent_project_meta,
        ) {
            eprintln!("[AppShell] open recent projects palette failed: {err}");
            return false;
        }

        if current_mode != EditorMode::PaletteFocus
            && let Err(err) = self.app_state.apply_mode_event(ModeEvent::OpenPalette)
        {
            let _ = self.app_state.close_command_palette();
            eprintln!("[AppShell] recent projects mode change failed: {err:?}");
            return false;
        }

        if self.focus_manager.set(FocusTarget::OverlayLayer) {
            self.input_handler.clear_pending_prefix();
        }
        true
    }

    pub(super) fn confirm_recent_project_selection(&mut self) -> bool {
        let Some(crate::app::command_palette::CommandPaletteAction::OpenFile(path)) =
            self.app_state.command_palette_selected_action()
        else {
            return false;
        };

        let mut changed = self.app_state.close_command_palette();
        if let Ok(result) = self.app_state.apply_mode_event(ModeEvent::ExitFocus) {
            changed |= result.changed;
        }
        let focus_changed = self.focus_manager.set(FocusTarget::CenterEditor);
        changed |= focus_changed;
        if focus_changed {
            self.input_handler.clear_pending_prefix();
        }

        changed | self.switch_workspace_to(path)
    }

    pub(super) fn remove_recent_project_selection(&mut self) -> bool {
        let Some(crate::app::command_palette::CommandPaletteAction::OpenFile(path)) =
            self.app_state.command_palette_selected_action()
        else {
            return false;
        };

        let label = path
            .file_name()
            .and_then(|name| name.to_str())
            .map(str::to_string)
            .unwrap_or_else(|| path.to_string_lossy().into_owned());

        if !self.persistent_state.remove_recent(&path) {
            return false;
        }

        self.persistent_state.save();
        let mut changed = self
            .app_state
            .open_recent_projects_palette_with_meta(
                &self.persistent_state.recent_projects,
                &self.persistent_state.recent_project_meta,
            )
            .is_ok();
        changed |= self
            .app_state
            .sync_welcome_recent_projects(&self.persistent_state.recent_projects);
        self.show_transient_toast(format!("Removed recent project: {label}"));
        changed || self.transient_toast.is_some()
    }

    pub(super) fn confirm_theme_selection(&mut self) -> bool {
        let Some(crate::app::command_palette::CommandPaletteAction::SelectTheme(theme_profile)) =
            self.app_state.command_palette_selected_action()
        else {
            return false;
        };

        let loaded_theme =
            if self.theme_picker_preview_profile.as_deref() == Some(theme_profile.as_str()) {
                self.base_theme.clone()
            } else {
                match ThemeConfig::load(&theme_profile) {
                    Ok(theme) => theme,
                    Err(err) => {
                        eprintln!(
                            "[AppShell] theme load failed for profile '{}': {err}",
                            theme_profile
                        );
                        self.show_transient_toast(format!("Failed to load theme: {theme_profile}"));
                        return true;
                    }
                }
            };

        self.theme_picker_original_theme = None;
        self.theme_picker_preview_profile = None;
        self.base_theme = loaded_theme;
        self.apply_scaled_runtime_config();
        self.leap_state = None;
        if let Some(renderer) = self.renderer.as_mut() {
            renderer.clear_leap_labels();
        }
        self.persistent_state
            .set_theme_profile(Some(theme_profile.clone()));
        self.persistent_state.save();

        self.clear_palette_ime_commit_suppression();
        let mut changed = self.app_state.close_command_palette();
        if let Ok(result) = self.app_state.apply_mode_event(ModeEvent::ExitFocus) {
            changed |= result.changed;
        }
        let focus_changed = self.focus_manager.set(FocusTarget::CenterEditor);
        changed |= focus_changed;
        if focus_changed {
            self.input_handler.clear_pending_prefix();
        }

        self.show_transient_toast(format!("Theme loaded: {theme_profile}"));
        changed || self.transient_toast.is_some()
    }

    /// Push the current file-history preview (full file at the selected step) into
    /// the picker's preview pane, syntax-highlighted from the source file's
    /// extension — same in-process highlighter the file/reference previews use.
    pub(super) fn refresh_file_history_preview(&mut self) {
        let Some((lines, preview_text)) = self.app_state.build_file_history_diff_preview() else {
            return;
        };
        let (text, spans) = match self.app_state.file_history_source_path() {
            Some(path) => super::helpers::build_preview_render_data(&lines, &path, &self.theme),
            None => (preview_text, Vec::new()),
        };
        let _ = self.app_state.set_fuzzy_picker_preview(lines, text, spans);
    }

    /// Confirm an entry in the center-pane file-history picker: close the picker,
    /// return to the source file, and time-travel it back to the chosen step
    /// (non-destructively — later steps stay redoable).
    pub(super) fn confirm_file_history_selection(&mut self) -> bool {
        // Read target step + source file from the still-active picker before closing.
        let target = match self.app_state.command_palette_selected_action() {
            Some(crate::app::command_palette::CommandPaletteAction::SelectFileHistoryEntry(
                index,
            )) => Some(index),
            _ => None,
        };
        let source_path = self.app_state.file_history_source_path();

        // Drop the preview session, then close the center-pane picker buffer.
        let _ = self.app_state.cancel_file_history_preview();
        let _ = self.close_current_buffer_now();

        // The picker closes to an arbitrary adjacent buffer, so return to the
        // source file explicitly, then replay undo down to the selected step.
        if let Some(path) = source_path {
            let _ = self.app_state.activate_text_buffer_for_path(&path);
        }
        if let Some(index) = target {
            let _ = self.app_state.restore_to_history_index(index);
        }

        self.editor_needs_layout = true;
        self.editor_caret_needs_layout = false;
        if self.focus_manager.set(FocusTarget::CenterEditor) {
            self.input_handler.clear_pending_prefix();
        }
        self.submit_parse_for_active_buffer(true);
        true
    }

    pub(in crate::app::event_loop) fn respond_to_pending_confirmation(
        &mut self,
        confirmed: bool,
    ) -> bool {
        let Some(pending) = self.pending_confirmation.take() else {
            return false;
        };
        let mut changed = self.close_pending_confirmation_overlay(pending.return_focus);

        match pending.action {
            PendingConfirmationAction::Delete { path, file_type } => {
                if !confirmed {
                    return changed;
                }
                let fallback_selection = self.app_state.workspace_root_path().and_then(|root| {
                    path.parent().and_then(|parent| {
                        (parent.starts_with(root) && parent != root).then(|| parent.to_path_buf())
                    })
                });

                let delete_result = match file_type {
                    WorkspaceNodeType::File => std::fs::remove_file(&path),
                    WorkspaceNodeType::Folder => std::fs::remove_dir_all(&path),
                };
                if let Err(err) = delete_result {
                    eprintln!(
                        "[AppShell] explorer delete failed for {}: {err}",
                        path.display()
                    );
                    return changed;
                }

                if let Err(err) = self.app_state.rescan_workspace() {
                    eprintln!(
                        "[AppShell] workspace rescan failed after explorer delete for {}: {err}",
                        path.display()
                    );
                }
                if matches!(file_type, WorkspaceNodeType::File) {
                    match self.app_state.close_buffer_for_path(&path) {
                        Ok(closed) => {
                            if closed {
                                self.editor_needs_layout = true;
                                self.editor_caret_needs_layout = false;
                                self.clear_highlight_layers();
                            }
                        }
                        Err(err) => {
                            eprintln!(
                                "[AppShell] close buffer after explorer delete failed for {}: {err}",
                                path.display()
                            );
                        }
                    }
                }
                self.submit_workspace_git_status_refresh();
                if let Some(parent_path) = fallback_selection {
                    let _ = self.app_state.workspace_select_path(&parent_path);
                }
                self.mark_explorer_dirty();
                true
            }
            PendingConfirmationAction::CloseDirtyBuffer { .. } => {
                if confirmed {
                    let saved = self.handle_command(Command::SaveFile);
                    changed |= saved;
                    if self.app_state.is_dirty() {
                        return changed;
                    }
                }
                changed | self.close_current_buffer_now()
            }
            PendingConfirmationAction::QuitDirtyBuffers { .. } => {
                if confirmed {
                    match self.save_all_dirty_buffers_for_quit() {
                        Ok(saved_count) => {
                            if saved_count > 0 {
                                self.submit_workspace_git_status_refresh();
                                self.submit_active_buffer_git_baseline_refresh();
                            }
                        }
                        Err(err) => {
                            self.show_transient_toast_kind(
                                format!("Quit cancelled\nCould not save: {err}"),
                                ToastKind::Error,
                            );
                            return true;
                        }
                    }
                }
                self.exit_requested = true;
                true
            }
            PendingConfirmationAction::ExternalOverwrite { path } => {
                let active_matches = self.app_state.active_file().is_some_and(|active| {
                    if active == path.as_path() {
                        return true;
                    }
                    match (active.canonicalize().ok(), path.canonicalize().ok()) {
                        (Some(active_canon), Some(target_canon)) => active_canon == target_canon,
                        _ => false,
                    }
                });
                if !active_matches {
                    return changed;
                }
                if confirmed {
                    changed |= self.handle_command(Command::SaveFile);
                } else {
                    match self
                        .app_state
                        .reload_active_file_from_disk_discarding_local()
                    {
                        Ok(_) => {
                            self.invalidate_highlights_and_parse_active_buffer();
                            self.force_flush_lsp_did_change_for_active_file();
                            changed = true;
                        }
                        Err(err) => {
                            eprintln!(
                                "[AppShell] reload-after-external-conflict failed for {}: {err}",
                                path.display()
                            );
                        }
                    }
                }
                changed
            }
        }
    }

    pub(in crate::app::event_loop) fn cancel_pending_confirmation(&mut self) -> bool {
        let Some(pending) = self.pending_confirmation.take() else {
            return false;
        };
        self.close_pending_confirmation_overlay(pending.return_focus)
    }
}
