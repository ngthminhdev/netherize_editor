use super::*;
use crate::app::clipboard::ClipboardProvider;

#[derive(Default)]
struct MockClipboard {
    text: String,
}

impl ClipboardProvider for MockClipboard {
    fn get_text(&mut self) -> Result<String, String> {
        Ok(self.text.clone())
    }

    fn set_text(&mut self, text: &str) -> Result<(), String> {
        self.text = text.to_string();
        Ok(())
    }
}

fn select_ui_rounding(shell: &mut AppShell) {
    shell.handle_command(Command::OpenSettings);
    let settings = shell
        .app_state
        .active_settings_buffer_mut()
        .expect("settings buffer");
    settings.selected_index = 1;
}

#[test]
fn palette_paste_uses_clipboard_provider() {
    let mut app_state = AppState::from_text(PathBuf::from("palette-paste.txt"), "alpha beta");
    let mut clipboard = MockClipboard {
        text: "foo\nbar".to_string(),
    };

    let open = dispatch_command(&mut app_state, Command::OpenInFileSearch);
    assert!(open.success);
    assert_eq!(app_state.current_mode(), EditorMode::PaletteFocus);
    assert!(app_state.is_command_palette_visible());

    let report =
        dispatch_palette_overlay_command(&mut app_state, &mut clipboard, Command::EditorPaste);

    assert!(report.success);
    assert!(report.state_changed);
    assert_eq!(app_state.command_palette_query_text(), "foo bar");
}

#[test]
fn terminal_paste_normalizes_newlines_to_carriage_returns() {
    assert_eq!(
        normalize_terminal_paste_text("echo one\necho two\r\npwd\r"),
        "echo one\recho two\rpwd\r"
    );
}

#[test]
fn move_to_first_line_uses_viewport_layout_path() {
    let mut shell = AppShell::new_for_tests().expect("create app shell");
    let text = (0..80)
        .map(|idx| format!("line {idx}"))
        .collect::<Vec<_>>()
        .join("\n");
    shell.app_state = AppState::from_text(PathBuf::from("gg-layout.txt"), &text);
    let _ = shell.app_state.apply_mode_event(ModeEvent::EnterNormal);
    assert!(shell.app_state.move_to_last_line());
    shell.app_state.set_target_scroll_line(24);
    shell.editor_needs_layout = false;
    shell.editor_caret_needs_layout = true;

    let changed = shell.handle_command(Command::MoveToFirstLine);

    assert!(changed);
    assert_eq!(shell.app_state.cursor_line_col(), (0, 0));
    assert_eq!(shell.app_state.scroll_line(), 0);
    assert!(shell.editor_needs_layout);
    assert!(!shell.editor_caret_needs_layout);
}

#[test]
fn move_to_last_line_uses_viewport_layout_path() {
    let mut shell = AppShell::new_for_tests().expect("create app shell");
    let text = (0..120)
        .map(|idx| format!("line {idx}"))
        .collect::<Vec<_>>()
        .join("\n");
    shell.app_state = AppState::from_text(PathBuf::from("g-layout.txt"), &text);
    let _ = shell.app_state.apply_mode_event(ModeEvent::EnterNormal);
    shell.app_state.move_to_first_line();
    shell.app_state.set_target_scroll_line(0);
    shell.editor_needs_layout = false;
    shell.editor_caret_needs_layout = true;

    let changed = shell.handle_command(Command::MoveToLastLine);

    assert!(changed);
    let (cursor_line, _) = shell.app_state.cursor_line_col();
    assert_eq!(cursor_line, shell.app_state.total_lines().saturating_sub(1));
    assert!(shell.app_state.scroll_line() > 0);
    assert!(shell.editor_needs_layout);
    assert!(!shell.editor_caret_needs_layout);
}

#[test]
fn center_cursor_line_uses_viewport_layout_path() {
    let mut shell = AppShell::new_for_tests().expect("create app shell");
    let text = (0..80)
        .map(|idx| format!("line {idx}"))
        .collect::<Vec<_>>()
        .join("\n");
    shell.app_state = AppState::from_text(PathBuf::from("zz-layout.txt"), &text);
    let _ = shell.app_state.apply_mode_event(ModeEvent::EnterNormal);
    for _ in 0..30 {
        shell.app_state.move_down();
    }
    shell.app_state.set_target_scroll_line(0);
    shell.editor_needs_layout = false;
    shell.editor_caret_needs_layout = true;
    let viewport_lines = shell.editor_viewport_lines();

    let changed = shell.handle_command(Command::CenterCursorLine);

    assert!(changed);
    let (cursor_line, _) = shell.app_state.cursor_line_col();
    assert_eq!(
        shell.app_state.scroll_line(),
        cursor_line.saturating_sub(viewport_lines / 2)
    );
    assert!(shell.editor_needs_layout);
    assert!(!shell.editor_caret_needs_layout);
}

#[test]
fn settings_activate_begins_numeric_edit_for_ui_rounding() {
    let mut shell = AppShell::new_for_tests().expect("create app shell");
    shell.ui_config.border_radius_px = 16.0;
    select_ui_rounding(&mut shell);

    let changed = shell.handle_command(Command::SettingsActivate);

    assert!(changed);
    assert_eq!(shell.app_state.current_mode(), EditorMode::Insert);
    let settings = shell
        .app_state
        .active_settings_buffer()
        .expect("settings buffer");
    let editing = settings.editing.as_ref().expect("editing state");
    assert_eq!(
        editing.kind,
        crate::app::app_state::SettingsEditingKind::UiRounding
    );
    assert_eq!(editing.draft, "16");
}

#[test]
fn settings_adjust_increase_allows_ui_rounding_to_reach_24() {
    let mut shell = AppShell::new_for_tests().expect("create app shell");
    shell.ui_config.border_radius_px = 16.0;
    select_ui_rounding(&mut shell);

    let changed = shell.handle_command(Command::SettingsAdjustIncrease);

    assert!(changed);
    assert_eq!(shell.ui_config.border_radius_px, 24.0);
    let settings = shell
        .app_state
        .active_settings_buffer()
        .expect("settings buffer");
    assert_eq!(
        settings.selected_item(),
        Some(&crate::app::app_state::SettingItem::UiRounding {
            enabled: true,
            radius_px: 24.0,
        })
    );
}

#[test]
fn settings_commit_ui_rounding_edit_clamps_to_24() {
    let mut shell = AppShell::new_for_tests().expect("create app shell");
    shell.ui_config.border_radius_px = 16.0;
    select_ui_rounding(&mut shell);
    assert!(shell.handle_command(Command::SettingsActivate));
    {
        let settings = shell
            .app_state
            .active_settings_buffer_mut()
            .expect("settings buffer");
        let editing = settings.editing.as_mut().expect("editing state");
        editing.draft = "32".to_string();
    }

    let changed = shell.handle_command(Command::SettingsActivate);

    assert!(changed);
    assert_eq!(shell.ui_config.border_radius_px, 24.0);
    let settings = shell
        .app_state
        .active_settings_buffer()
        .expect("settings buffer");
    assert!(settings.editing.is_none());
    assert_eq!(
        settings.selected_item(),
        Some(&crate::app::app_state::SettingItem::UiRounding {
            enabled: true,
            radius_px: 24.0,
        })
    );
}

#[test]
fn scroll_half_page_down_uses_viewport_layout_path() {
    let mut shell = AppShell::new_for_tests().expect("create app shell");
    let text = (0..100)
        .map(|idx| format!("line {idx}"))
        .collect::<Vec<_>>()
        .join("\n");
    shell.app_state = AppState::from_text(PathBuf::from("ctrl-d-layout.txt"), &text);
    let _ = shell.app_state.apply_mode_event(ModeEvent::EnterNormal);
    shell.app_state.set_target_scroll_line(0);
    shell.editor_needs_layout = false;
    shell.editor_caret_needs_layout = true;
    let half = (shell.editor_viewport_lines() / 2).max(1);

    let changed = shell.handle_command(Command::ScrollHalfPageDown);

    assert!(changed);
    let (cursor_line, _) = shell.app_state.cursor_line_col();
    assert_eq!(cursor_line, half);
    assert_eq!(shell.app_state.scroll_line(), half);
    assert!(shell.editor_needs_layout);
    assert!(!shell.editor_caret_needs_layout);
}

#[test]
fn explorer_rename_base_selection_keeps_extension() {
    assert_eq!(AppShell::explorer_rename_base_selection("main.rs"), (0, 4));
    assert_eq!(
        AppShell::explorer_rename_base_selection("archive.tar.gz"),
        (0, 11)
    );
    assert_eq!(AppShell::explorer_rename_base_selection("README"), (0, 6));
    assert_eq!(
        AppShell::explorer_rename_base_selection(".gitignore"),
        (0, 10)
    );
}

#[test]
fn toggle_terminal_command_closes_bottom_panel_after_second_press() {
    let mut shell = AppShell::new_for_tests().expect("create app shell");
    assert!(!shell.panel_state.bottom.visible);

    assert!(shell.handle_command(Command::ToggleTerminal));
    assert!(shell.panel_state.bottom.visible);
    assert_eq!(shell.focus_manager.current(), FocusTarget::BottomPanel);

    assert!(shell.handle_command(Command::ToggleTerminal));
    assert!(!shell.panel_state.bottom.visible);
    assert_eq!(shell.focus_manager.current(), FocusTarget::CenterEditor);
}

#[test]
fn focus_terminal_focuses_open_panel_from_editor_without_closing_it() {
    let mut shell = AppShell::new_for_tests().expect("create app shell");
    assert!(shell.handle_command(Command::ToggleBottomDock));
    assert!(shell.panel_state.bottom.visible);
    assert_eq!(shell.focus_manager.current(), FocusTarget::CenterEditor);

    assert!(shell.handle_command(Command::FocusTerminal));
    assert!(shell.panel_state.bottom.visible);
    assert_eq!(shell.focus_manager.current(), FocusTarget::BottomPanel);
    assert_eq!(shell.app_state.current_mode(), EditorMode::TerminalFocus);
}

#[test]
fn focus_terminal_closes_panel_when_terminal_already_has_focus() {
    let mut shell = AppShell::new_for_tests().expect("create app shell");
    assert!(shell.handle_command(Command::FocusTerminal));
    assert!(shell.panel_state.bottom.visible);
    assert_eq!(shell.focus_manager.current(), FocusTarget::BottomPanel);
    assert_eq!(shell.app_state.current_mode(), EditorMode::TerminalFocus);

    assert!(shell.handle_command(Command::FocusTerminal));
    assert!(!shell.panel_state.bottom.visible);
    assert_eq!(shell.focus_manager.current(), FocusTarget::CenterEditor);
    assert_eq!(shell.app_state.current_mode(), EditorMode::Normal);
}

#[test]
fn focus_terminal_closes_panel_from_terminal_normal() {
    let mut shell = AppShell::new_for_tests().expect("create app shell");
    assert!(shell.handle_command(Command::FocusTerminal));
    assert!(shell.handle_command(Command::SwitchMode(ModeEvent::EnterTerminalNormal)));
    assert_eq!(shell.app_state.current_mode(), EditorMode::TerminalNormal);

    assert!(shell.handle_command(Command::FocusTerminal));
    assert!(!shell.panel_state.bottom.visible);
    assert_eq!(shell.focus_manager.current(), FocusTarget::CenterEditor);
    assert_eq!(shell.app_state.current_mode(), EditorMode::Normal);
}

#[test]
fn toggle_bottom_dock_keeps_editor_focus_when_opening() {
    let mut shell = AppShell::new_for_tests().expect("create app shell");
    assert_eq!(shell.focus_manager.current(), FocusTarget::CenterEditor);
    assert!(!shell.panel_state.bottom.visible);

    assert!(shell.handle_command(Command::ToggleBottomDock));
    assert!(shell.panel_state.bottom.visible);
    assert_eq!(shell.focus_manager.current(), FocusTarget::CenterEditor);

    assert!(shell.handle_command(Command::ToggleBottomDock));
    assert!(!shell.panel_state.bottom.visible);
    assert_eq!(shell.focus_manager.current(), FocusTarget::CenterEditor);
}

#[test]
fn ai_chat_toggle_closing_right_dock_returns_focus_to_editor() {
    let mut shell = AppShell::new_for_tests().expect("create app shell");
    assert_eq!(shell.focus_manager.current(), FocusTarget::CenterEditor);
    assert!(!shell.panel_state.right.visible);

    assert!(shell.handle_command(Command::AiChatToggle));
    assert!(shell.panel_state.right.visible);
    assert_eq!(
        shell.panel_state.right.active_tab_id(),
        Some(PanelTabId::AiChat)
    );
    assert_eq!(shell.focus_manager.current(), FocusTarget::RightSidebar);

    assert!(shell.handle_command(Command::AiChatToggle));
    assert!(!shell.panel_state.right.visible);
    assert_eq!(
        shell.panel_state.right.active_tab_id(),
        Some(PanelTabId::AiChat)
    );
    assert_eq!(shell.focus_manager.current(), FocusTarget::CenterEditor);
}

#[test]
fn explorer_filter_commands_update_workspace_state() {
    let mut shell = AppShell::new_for_tests().expect("create app shell");
    let root = std::env::temp_dir().join(format!(
        "netherize_explorer_filter_cmd_{}",
        std::process::id()
    ));
    std::fs::create_dir_all(root.join("src")).expect("create dirs");
    std::fs::write(root.join("src/main.rs"), "fn main() {}\n").expect("write file");

    shell
        .app_state
        .attach_workspace(root.clone())
        .expect("attach workspace");
    shell.focus_manager.set(FocusTarget::LeftSidebar);

    assert!(shell.handle_command(Command::ExplorerStartFilter));
    assert!(shell.app_state.workspace_is_inputting_filter());
    assert!(shell.app_state.workspace_append_filter_text("main"));
    assert!(shell.handle_command(Command::ExplorerClearFilter));
    assert!(!shell.app_state.workspace_is_inputting_filter());
    assert!(!shell.app_state.workspace_has_active_filter());

    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn leap_uses_editor_targets_even_when_explorer_is_focused() {
    let mut shell = AppShell::new_for_tests().expect("create app shell");
    shell.app_state = AppState::from_text(PathBuf::from("editor-leap.txt"), "beta\nomega");
    shell.focus_manager.set(FocusTarget::LeftSidebar);
    shell.last_editor_bounds = Some([0.0, 0.0, 640.0, 240.0]);

    assert!(shell.handle_command(Command::LeapActivate('b')));

    let leap_state = shell.leap_state.as_ref().expect("editor leap state");
    assert_eq!(leap_state.typed_prefix, "");
    assert_eq!(leap_state.targets.len(), 1);
    assert_eq!(leap_state.targets[0].label, "a");
    assert_eq!(leap_state.targets[0].char_idx, 0);
}

#[test]
fn leap_generates_multi_char_labels_after_twenty_six_matches() {
    let mut shell = AppShell::new_for_tests().expect("create app shell");
    let text = (0..27).map(|_| "a").collect::<Vec<_>>().join(" ");
    shell.app_state = AppState::from_text(PathBuf::from("editor-leap.txt"), &text);
    shell.last_editor_bounds = Some([0.0, 0.0, 960.0, 240.0]);

    assert!(shell.handle_command(Command::LeapActivate('a')));

    let leap_state = shell.leap_state.as_ref().expect("editor leap state");
    assert_eq!(leap_state.targets.len(), 27);
    assert_eq!(leap_state.targets[0].label, "a");
    assert_eq!(leap_state.targets[12].label, "m");
    assert_eq!(leap_state.targets[13].label, "na");
    assert_eq!(leap_state.targets[25].label, "nm");
    assert_eq!(leap_state.targets[26].label, "nn");
}

#[test]
fn leap_fast_jump_label_resolves_immediately() {
    let mut shell = AppShell::new_for_tests().expect("create app shell");
    let text = (0..40).map(|_| "a").collect::<Vec<_>>().join(" ");
    shell.app_state = AppState::from_text(PathBuf::from("editor-leap.txt"), &text);
    shell.last_editor_bounds = Some([0.0, 0.0, 960.0, 240.0]);

    assert!(shell.handle_command(Command::LeapActivate('a')));
    assert!(shell.handle_command(Command::LeapJump('b')));

    assert!(shell.leap_state.is_none());
    assert_eq!(shell.app_state.cursor_line_col(), (0, 2));
}

#[test]
fn leap_prefix_label_filters_and_waits_for_second_key() {
    let mut shell = AppShell::new_for_tests().expect("create app shell");
    let text = (0..40).map(|_| "a").collect::<Vec<_>>().join(" ");
    shell.app_state = AppState::from_text(PathBuf::from("editor-leap.txt"), &text);
    shell.last_editor_bounds = Some([0.0, 0.0, 960.0, 240.0]);

    assert!(shell.handle_command(Command::LeapActivate('a')));
    assert!(shell.handle_command(Command::LeapJump('n')));

    let leap_state = shell.leap_state.as_ref().expect("filtered leap state");
    assert_eq!(leap_state.typed_prefix, "n");
    assert_eq!(leap_state.targets.len(), 26);
    assert!(
        leap_state
            .targets
            .iter()
            .all(|target| target.label.starts_with("n"))
    );

    assert!(shell.handle_command(Command::LeapJump('b')));
    assert!(shell.leap_state.is_none());
    assert_eq!(shell.app_state.cursor_line_col(), (0, 28));
}

#[test]
fn delete_confirmation_removes_selected_file_after_y() {
    let mut shell = AppShell::new_for_tests().expect("create app shell");
    let root =
        std::env::temp_dir().join(format!("netherize_delete_confirm_{}", std::process::id()));
    std::fs::create_dir_all(&root).expect("create root");
    let file_path = root.join("delete-me.txt");
    std::fs::write(&file_path, "bye\n").expect("write file");

    shell
        .app_state
        .attach_workspace(root.clone())
        .expect("attach workspace");
    let _ = shell.app_state.workspace_select_path(&file_path);
    shell.mark_explorer_dirty();
    shell.ensure_explorer_snapshot();

    assert!(shell.handle_command(Command::ExplorerDeleteNode));
    assert_eq!(
        shell.app_state.command_palette_mode(),
        Some(crate::app::command_palette::CommandPaletteMode::ExplorerDeleteConfirm)
    );
    assert_eq!(
        shell.pending_confirmation_prompt().as_deref(),
        Some("Delete delete-me.txt? (y/n)")
    );
    assert_eq!(
        shell.app_state.command_palette_query_text(),
        "Delete delete-me.txt? (y/n)"
    );
    assert!(shell.respond_to_pending_confirmation(true));
    assert!(!file_path.exists());
    assert!(shell.pending_confirmation.is_none());
    assert!(!shell.app_state.is_command_palette_visible());

    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn delete_confirmation_cancels_on_escape() {
    let mut shell = AppShell::new_for_tests().expect("create app shell");
    shell.pending_confirmation = Some(PendingConfirmation {
        action: PendingConfirmationAction::Delete {
            path: PathBuf::from("demo.txt"),
            file_type: WorkspaceNodeType::File,
        },
        return_focus: FocusTarget::LeftSidebar,
    });

    assert!(shell.respond_to_pending_confirmation(false));
    assert!(shell.pending_confirmation.is_none());
    assert!(!shell.app_state.is_command_palette_visible());
}

#[test]
fn dirty_buffer_close_opens_save_confirmation_prompt() {
    let mut shell = AppShell::new_for_tests().expect("create app shell");
    let file_name = format!("netherize_dirty_close_prompt_{}.txt", std::process::id());
    let file_path = std::env::temp_dir().join(&file_name);
    let expected_prompt = format!("Save changes to {file_name} before closing? (y/n)");
    std::fs::write(&file_path, "hello\n").expect("write file");
    shell
        .app_state
        .open_file(file_path.clone())
        .expect("open file");
    shell.app_state.insert_char('!');

    assert!(shell.handle_command(Command::BufferCloseCurrent));
    assert_eq!(
        shell.app_state.command_palette_mode(),
        Some(crate::app::command_palette::CommandPaletteMode::BufferCloseConfirm)
    );
    assert_eq!(
        shell.pending_confirmation_prompt().as_deref(),
        Some(expected_prompt.as_str())
    );

    let _ = std::fs::remove_file(file_path);
}

#[test]
fn dirty_buffer_close_confirmation_yes_saves_then_closes() {
    let mut shell = AppShell::new_for_tests().expect("create app shell");
    let file_path = std::env::temp_dir().join(format!(
        "netherize_dirty_close_yes_{}.txt",
        std::process::id()
    ));
    std::fs::write(&file_path, "hello\n").expect("write file");
    shell
        .app_state
        .open_file(file_path.clone())
        .expect("open file");
    shell.app_state.insert_char('!');

    assert!(shell.handle_command(Command::BufferCloseCurrent));
    assert!(shell.respond_to_pending_confirmation(true));
    assert_eq!(
        std::fs::read_to_string(&file_path).expect("read file"),
        "!hello\n"
    );
    assert!(shell.app_state.active_file().is_none());
    assert!(!shell.app_state.is_command_palette_visible());

    let _ = std::fs::remove_file(file_path);
}

#[test]
fn dirty_buffer_close_confirmation_no_discards_changes_and_closes() {
    let mut shell = AppShell::new_for_tests().expect("create app shell");
    let file_path = std::env::temp_dir().join(format!(
        "netherize_dirty_close_no_{}.txt",
        std::process::id()
    ));
    std::fs::write(&file_path, "hello\n").expect("write file");
    shell
        .app_state
        .open_file(file_path.clone())
        .expect("open file");
    shell.app_state.insert_char('!');

    assert!(shell.handle_command(Command::BufferCloseCurrent));
    assert!(shell.respond_to_pending_confirmation(false));
    assert_eq!(
        std::fs::read_to_string(&file_path).expect("read file"),
        "hello\n"
    );
    assert!(shell.app_state.active_file().is_none());
    assert!(!shell.app_state.is_command_palette_visible());

    let _ = std::fs::remove_file(file_path);
}

#[test]
fn external_overwrite_confirmation_yes_saves_local_buffer_to_disk() {
    let mut shell = AppShell::new_for_tests().expect("create app shell");
    let file_path = std::env::temp_dir().join(format!(
        "netherize_external_overwrite_yes_{}.txt",
        std::process::id()
    ));
    std::fs::write(&file_path, "hello\n").expect("write file");
    shell
        .app_state
        .open_file(file_path.clone())
        .expect("open file");
    shell.app_state.insert_char('!');

    assert!(shell.begin_external_overwrite_confirmation(file_path.clone()));
    assert_eq!(
        shell.app_state.command_palette_mode(),
        Some(crate::app::command_palette::CommandPaletteMode::ExplorerDeleteConfirm)
    );
    assert!(shell.respond_to_pending_confirmation(true));
    assert_eq!(
        std::fs::read_to_string(&file_path).expect("read file"),
        "!hello\n"
    );
    assert!(!shell.app_state.is_dirty());
    assert!(!shell.app_state.is_command_palette_visible());

    let _ = std::fs::remove_file(file_path);
}

#[test]
fn external_overwrite_confirmation_no_reloads_from_disk_and_discards_local_dirty() {
    let mut shell = AppShell::new_for_tests().expect("create app shell");
    let file_path = std::env::temp_dir().join(format!(
        "netherize_external_overwrite_no_{}.txt",
        std::process::id()
    ));
    std::fs::write(&file_path, "hello\n").expect("write file");
    shell
        .app_state
        .open_file(file_path.clone())
        .expect("open file");
    shell.app_state.insert_char('!');
    std::fs::write(&file_path, "external\n").expect("write external");

    assert!(shell.begin_external_overwrite_confirmation(file_path.clone()));
    assert!(shell.respond_to_pending_confirmation(false));
    assert_eq!(shell.app_state.text_string(), "external\n");
    assert!(!shell.app_state.is_dirty());
    assert!(!shell.app_state.is_command_palette_visible());

    let _ = std::fs::remove_file(file_path);
}

#[test]
fn opening_palette_arms_one_shot_ime_suppression() {
    let mut shell = AppShell::new_for_tests().expect("create app shell");

    assert!(shell.handle_command(Command::OpenCommandPalette));
    assert!(shell.app_state.is_command_palette_visible());
    assert!(shell.suppress_next_palette_ime_commit);
    assert!(shell.should_swallow_palette_ime_commit());
    assert!(!shell.suppress_next_palette_ime_commit);
}

#[test]
fn first_real_keypress_after_palette_open_clears_ime_suppression() {
    let mut shell = AppShell::new_for_tests().expect("create app shell");

    assert!(shell.handle_command(Command::OpenCommandPalette));
    assert!(shell.suppress_next_palette_ime_commit);

    shell.note_post_open_keyboard_press();

    assert!(!shell.suppress_next_palette_ime_commit);
    assert!(!shell.should_swallow_palette_ime_commit());
}

#[test]
fn visual_selection_adds_code_context_to_ai_chat() {
    let mut shell = AppShell::new_for_tests().expect("create app shell");
    shell.app_state = AppState::from_text(
        PathBuf::from("src/main.rs"),
        "let secret = 1;\nlet visible = true;\n",
    );
    let _ = dispatch_command(
        &mut shell.app_state,
        Command::SwitchMode(ModeEvent::EnterVisual),
    );
    shell.app_state.move_to_line_end();

    let changed = shell.handle_command(Command::AiChatAddSelectionContext);

    assert!(changed);
    assert!(shell.panel_state.right.visible);
    assert_eq!(
        shell.panel_state.right.active_tab_id(),
        Some(PanelTabId::AiChat)
    );
    assert_eq!(shell.focus_manager.current(), FocusTarget::RightSidebar);
    assert_eq!(shell.app_state.current_mode(), EditorMode::Normal);
    assert_eq!(shell.panel_state.ai_chat.attached_code_contexts.len(), 1);
    assert!(
        shell.panel_state.ai_chat.attached_code_contexts[0]
            .text
            .contains("let secret = 1")
    );
    assert!(
        shell
            .panel_state
            .ai_chat
            .input_buffer
            .starts_with("Hỏi về đoạn code đã chọn")
    );
}

#[test]
fn ai_chat_at_file_suggestions_use_workspace_files() {
    let mut shell = AppShell::new_for_tests().expect("create app shell");
    let root = std::env::temp_dir().join(format!(
        "netherize_ai_chat_at_suggestions_{}",
        std::process::id()
    ));
    std::fs::create_dir_all(root.join("src")).expect("create dirs");
    std::fs::write(root.join("src/main.rs"), "fn main() {}\n").expect("write file");

    shell
        .app_state
        .attach_workspace(root.clone())
        .expect("attach workspace");

    let suggestions = shell.ai_chat_file_reference_suggestions("read @mai");

    assert!(suggestions.iter().any(|(path, _)| path == "src/main.rs"));

    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn open_file_finder_keeps_center_focus_for_fuzzy_buffer() {
    let mut shell = AppShell::new_for_tests().expect("create app shell");

    assert!(shell.handle_command(Command::OpenFileFinder));

    assert_eq!(shell.focus_manager.current(), FocusTarget::CenterEditor);
    assert!(shell.app_state.active_buffer_is_fuzzy_picker());
    assert_eq!(shell.app_state.current_mode(), EditorMode::Insert);
    assert!(!shell.app_state.is_command_palette_visible());
}

#[test]
fn search_in_files_keeps_center_focus_for_fuzzy_buffer() {
    let mut shell = AppShell::new_for_tests().expect("create app shell");

    assert!(shell.handle_command(Command::SearchInFiles));

    assert_eq!(shell.focus_manager.current(), FocusTarget::CenterEditor);
    assert!(shell.app_state.active_buffer_is_fuzzy_picker());
    assert_eq!(shell.app_state.current_mode(), EditorMode::Insert);
    assert_eq!(
        shell.app_state.command_palette_mode(),
        Some(CommandPaletteMode::LiveGrep)
    );
    assert!(!shell.app_state.is_command_palette_visible());
}

#[test]
fn welcome_recent_projects_can_navigate_without_opening_palette() {
    let mut shell = AppShell::new_for_tests().expect("create app shell");
    let root = std::env::temp_dir().join(format!(
        "netherize_welcome_recent_nav_{}",
        std::process::id()
    ));
    let project_a = root.join("project_a");
    let project_b = root.join("project_b");
    std::fs::create_dir_all(&project_a).expect("create project a");
    std::fs::create_dir_all(&project_b).expect("create project b");
    shell.persistent_state.recent_projects = vec![project_a.clone(), project_b.clone()];

    assert!(shell.app_state.buffers().is_empty());
    assert!(!shell.app_state.is_command_palette_visible());

    assert!(shell.handle_command(Command::OverlaySelectNext));

    assert!(!shell.app_state.is_command_palette_visible());
    assert_eq!(shell.app_state.command_palette_selected_index(), 1);

    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn welcome_recent_projects_navigation_is_limited_to_visible_rows() {
    let mut shell = AppShell::new_for_tests().expect("create app shell");
    let root = std::env::temp_dir().join(format!(
        "netherize_welcome_recent_limit_{}",
        std::process::id()
    ));
    shell.persistent_state.recent_projects = (0..6)
        .map(|idx| root.join(format!("project_{idx}")))
        .collect();

    for _ in 0..5 {
        assert!(shell.handle_command(Command::OverlaySelectNext));
    }

    assert!(!shell.app_state.is_command_palette_visible());
    assert_eq!(shell.app_state.command_palette_selected_index(), 0);
}

#[test]
fn colon_help_vim_command_opens_help_buffer() {
    let mut shell = AppShell::new_for_tests().expect("create app shell");

    assert!(shell.handle_command(Command::OpenVimCommand));
    assert!(shell.handle_command(Command::FilePickerAppendQuery(":help".to_string())));
    assert!(shell.handle_command(Command::FilePickerConfirmSelection));

    let help = shell
        .app_state
        .active_help_buffer()
        .expect(":help should open the cheat sheet help buffer");
    assert_eq!(help.title, "[Cheat Sheet]");
    assert!(
        help.lines
            .iter()
            .any(|line| line == "Netherize Cheat Sheet")
    );
    assert!(
        help.lines
            .iter()
            .any(|line| { line.contains("mod+p") && line.contains("Open command palette") })
    );
    assert_eq!(
        shell.app_state.buffers().last().unwrap().label(),
        "[Cheat Sheet]"
    );
    assert!(!shell.app_state.is_command_palette_visible());
}

#[test]
fn file_picker_confirm_scrolls_explorer_to_opened_file() {
    let mut shell = AppShell::new_for_tests().expect("create app shell");
    let root = std::env::temp_dir().join(format!("netherize_picker_scroll_{}", std::process::id()));
    std::fs::create_dir_all(&root).expect("create workspace");
    for idx in 0..40 {
        std::fs::write(root.join(format!("file_{idx:02}.rs")), "fn main() {}\n")
            .expect("write file");
    }
    let target = root.join("file_35.rs");
    let canonical_target = target.canonicalize().expect("canonical target");

    shell
        .app_state
        .attach_workspace(root.clone())
        .expect("attach workspace");
    shell.last_sidebar_bounds = Some([0.0, 0.0, 240.0, 90.0]);
    shell.sidebar_needs_layout = false;

    assert!(shell.handle_command(Command::OpenFileFinder));
    assert!(shell.handle_command(Command::FilePickerAppendQuery("file_35".to_string())));
    assert!(shell.app_state.set_command_palette_results(
        CommandPaletteMode::FilePicker,
        "file_35",
        vec![crate::app::command_palette::CommandPaletteItem::file_match(
            "file_35.rs".to_string(),
            target.clone(),
        )],
    ));
    assert!(shell.handle_command(Command::FilePickerConfirmSelection));

    assert_eq!(
        shell.app_state.workspace_selected_path(),
        Some(canonical_target.as_path())
    );
    assert!(
        shell
            .app_state
            .workspace_scroll_offset_rows(shell.theme.ui.sidebar_line_height)
            > 0
    );
    assert!(shell.sidebar_needs_layout);

    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn file_picker_confirm_submits_git_baseline_refresh_for_opened_file() {
    let mut shell = AppShell::new_for_tests().expect("create app shell");
    let root = std::env::temp_dir().join(format!(
        "netherize_picker_git_baseline_{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&root).expect("create workspace");
    let target = root.join("changed.rs");
    std::fs::write(&target, "fn changed() {}\n").expect("write file");

    shell
        .app_state
        .attach_workspace(root.clone())
        .expect("attach workspace");
    let revision_before = shell.git_baseline_revision;

    assert!(shell.handle_command(Command::OpenFileFinder));
    assert!(shell.handle_command(Command::FilePickerAppendQuery("changed".to_string())));
    assert!(shell.app_state.set_command_palette_results(
        CommandPaletteMode::FilePicker,
        "changed",
        vec![crate::app::command_palette::CommandPaletteItem::file_match(
            "changed.rs".to_string(),
            target,
        )],
    ));
    assert!(shell.handle_command(Command::FilePickerConfirmSelection));

    assert!(shell.git_baseline_revision > revision_before);

    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn focus_markdown_preview_opens_preview_tab_and_focuses_sidebar() {
    let mut shell = AppShell::new_for_tests().expect("create app shell");
    shell.panel_state.right.visible = false;
    shell.app_state.markdown_preview.visible = false;

    let markdown_path = std::env::temp_dir().join(format!(
        "netherize_markdown_preview_{}.md",
        std::process::id()
    ));
    std::fs::write(&markdown_path, "# Preview title\n\nBody text\n").expect("write markdown");
    shell
        .app_state
        .open_file(markdown_path.clone())
        .expect("open markdown file");

    assert!(shell.handle_command(Command::FocusMarkdownPreview));

    assert!(shell.app_state.markdown_preview.visible);
    assert_eq!(shell.focus_manager.current(), FocusTarget::CenterEditor);
    let preview = shell
        .app_state
        .active_markdown_preview_buffer()
        .expect("markdown preview buffer active");
    assert!(!preview.rendered_lines.is_empty());
    assert_eq!(preview.rendered_lines[0].text, "Preview title");

    let _ = std::fs::remove_file(markdown_path);
}

#[test]
fn move_to_last_line_scrolls_active_markdown_preview_buffer_to_bottom() {
    let mut shell = AppShell::new_for_tests().expect("create app shell");
    let markdown_path = std::env::temp_dir().join(format!(
        "netherize_markdown_preview_scroll_{}.md",
        std::process::id()
    ));
    let markdown = (0..80)
        .map(|idx| format!("line {idx}"))
        .collect::<Vec<_>>()
        .join("\n");
    std::fs::write(&markdown_path, markdown).expect("write markdown");
    shell
        .app_state
        .open_file(markdown_path.clone())
        .expect("open markdown file");
    assert!(shell.handle_command(Command::FocusMarkdownPreview));
    shell.app_state.markdown_preview.scroll_y = 0.0;
    shell.app_state.markdown_preview.rendered_lines = (0..80)
        .map(|idx| crate::app::app_state::MarkdownPreviewLine {
            text: format!("line {idx}"),
            spans: Vec::new(),
            block_type: crate::app::app_state::MarkdownBlockType::Paragraph,
            code_language: None,
        })
        .collect();
    let _ = shell
        .app_state
        .sync_markdown_preview_buffer(shell.app_state.markdown_preview.clone());
    shell.app_state.markdown_preview = crate::app::app_state::MarkdownPreviewState::default();

    assert!(shell.handle_command(Command::MoveToLastLine));

    assert!(shell.app_state.markdown_preview.scroll_y > 0.0);
    let preview = shell
        .app_state
        .active_markdown_preview_buffer()
        .expect("markdown preview buffer active");
    assert_eq!(preview.scroll_y, shell.app_state.markdown_preview.scroll_y);

    let _ = std::fs::remove_file(markdown_path);
}

#[test]
fn open_theme_selector_opens_overlay_with_theme_profiles() {
    let mut shell = AppShell::new_for_tests().expect("create app shell");

    assert!(shell.handle_command(Command::OpenThemeSelector));

    assert_eq!(shell.focus_manager.current(), FocusTarget::OverlayLayer);
    assert_eq!(
        shell.app_state.command_palette_mode(),
        Some(CommandPaletteMode::ThemeSelector)
    );
    assert!(
        shell
            .app_state
            .command_palette_result_labels()
            .contains(&"default-dark".to_string())
    );
}

#[test]
fn confirming_theme_selector_reloads_theme_and_closes_overlay() {
    let mut shell = AppShell::new_for_tests().expect("create app shell");
    shell.editor_needs_layout = false;
    shell.sidebar_needs_layout = false;
    shell.terminal_needs_layout = false;

    assert!(shell.handle_command(Command::OpenThemeSelector));
    shell
        .app_state
        .set_command_palette_query("default-dark")
        .expect("set theme query");
    let expected_theme_name = ThemeConfig::load("default-dark")
        .expect("default-dark theme should load")
        .name;
    assert!(shell.handle_command(Command::FilePickerConfirmSelection));

    assert!(!shell.app_state.is_command_palette_visible());
    assert_eq!(shell.focus_manager.current(), FocusTarget::CenterEditor);
    assert_eq!(shell.base_theme.name, expected_theme_name);
    assert_eq!(shell.theme.name, expected_theme_name);
    assert_eq!(
        shell.persistent_state.configured_theme_profile(),
        Some("default-dark")
    );
    assert!(shell.editor_needs_layout);
    assert!(shell.sidebar_needs_layout);
    assert!(shell.terminal_needs_layout);
}

#[test]
fn lsp_references_open_loading_buffer_immediately() {
    let mut shell = AppShell::new_for_tests().expect("create app shell");
    let root = std::env::temp_dir().join(format!(
        "netherize_references_loading_{}",
        std::process::id()
    ));
    std::fs::create_dir_all(root.join("src")).expect("create workspace");
    let file_path = root.join("src/main.rs");
    std::fs::write(&file_path, "fn demo() {\n    demo();\n}\n").expect("write file");
    shell
        .app_state
        .attach_workspace(root.clone())
        .expect("attach workspace");
    shell
        .app_state
        .open_file(file_path.clone())
        .expect("open file");
    shell.active_lsp_server = Some(ActiveLspServer {
        server_name: "rust-analyzer".to_string(),
        root_path: root.clone(),
    });

    assert!(shell.handle_command(Command::LspReferences));

    let references = shell
        .app_state
        .active_references_buffer()
        .expect("references buffer should open immediately");
    assert!(references.loading);
    assert!(references.items.is_empty());
    assert_eq!(
        references.status_message.as_deref(),
        Some("Loading references...")
    );
    assert!(references.pending_request_id.is_some());
    assert_eq!(shell.focus_manager.current(), FocusTarget::CenterEditor);
    assert!(shell.editor_needs_layout);

    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn lsp_rename_opens_palette_prompt() {
    let mut shell = AppShell::new_for_tests().expect("create app shell");
    let root = std::env::temp_dir().join(format!("netherize_rename_prompt_{}", std::process::id()));
    std::fs::create_dir_all(root.join("src")).expect("create workspace");
    let file_path = root.join("src/main.rs");
    std::fs::write(&file_path, "fn demo() { let value = 1; }\n").expect("write file");
    shell
        .app_state
        .attach_workspace(root.clone())
        .expect("attach workspace");
    shell
        .app_state
        .open_file(file_path.clone())
        .expect("open file");
    shell.active_lsp_server = Some(ActiveLspServer {
        server_name: "rust-analyzer".to_string(),
        root_path: root.clone(),
    });

    assert!(shell.handle_command(Command::LspRename));
    assert_eq!(
        shell.app_state.command_palette_mode(),
        Some(CommandPaletteMode::LspRename)
    );
    assert_eq!(shell.focus_manager.current(), FocusTarget::OverlayLayer);

    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn opening_fuzzy_buffer_marks_editor_layout_dirty() {
    let mut shell = AppShell::new_for_tests().expect("create app shell");
    shell.editor_needs_layout = false;
    shell.editor_caret_needs_layout = true;

    assert!(shell.handle_command(Command::SearchInFiles));

    assert!(shell.editor_needs_layout);
    assert!(!shell.editor_caret_needs_layout);
}

#[test]
fn fuzzy_picker_query_updates_mark_editor_layout_dirty() {
    let mut shell = AppShell::new_for_tests().expect("create app shell");
    assert!(shell.handle_command(Command::SearchInFiles));
    shell.editor_needs_layout = false;
    shell.editor_caret_needs_layout = true;

    assert!(shell.handle_command(Command::FilePickerAppendQuery("foo".to_string())));

    assert!(shell.editor_needs_layout);
    assert!(!shell.editor_caret_needs_layout);
}

#[test]
fn fuzzy_picker_selection_clears_stale_preview_lines() {
    let mut shell = AppShell::new_for_tests().expect("create app shell");
    shell
        .app_state
        .open_fuzzy_picker_buffer(CommandPaletteMode::FilePicker);
    assert!(shell.app_state.set_command_palette_results(
        CommandPaletteMode::FilePicker,
        "",
        vec![
            crate::app::command_palette::CommandPaletteItem::file_match(
                "a.rs".to_string(),
                PathBuf::from("a.rs"),
            ),
            crate::app::command_palette::CommandPaletteItem::file_match(
                "b.rs".to_string(),
                PathBuf::from("b.rs"),
            ),
        ],
    ));
    assert!(shell.app_state.set_fuzzy_picker_preview(
        vec![crate::async_runtime::message::FilePreviewLine {
            line_number: 1,
            text: "hello".to_string(),
            is_target: false,
        }],
        String::new(),
        Vec::new(),
    ));

    assert!(shell.handle_command(Command::OverlaySelectNext));

    assert!(
        shell
            .app_state
            .active_fuzzy_picker_buffer()
            .expect("fuzzy buffer")
            .preview_lines
            .is_empty()
    );
}

#[test]
fn open_file_history_opens_center_fuzzy_buffer_tab() {
    let mut shell = AppShell::new_for_tests().expect("create app shell");
    let file_path = std::env::temp_dir().join("netherize_file_history_buffer_test.txt");
    std::fs::write(&file_path, "hello\n").expect("write file");
    shell
        .app_state
        .open_file(file_path.clone())
        .expect("open file");

    let _ = dispatch_command(
        &mut shell.app_state,
        Command::SwitchMode(crate::core::mode::ModeEvent::EnterInsert),
    );
    let _ = dispatch_command(
        &mut shell.app_state,
        Command::InsertText("world".to_string()),
    );
    let _ = dispatch_command(&mut shell.app_state, Command::SaveFile);

    assert!(shell.handle_command(Command::OpenFileHistory));
    assert_eq!(shell.focus_manager.current(), FocusTarget::CenterEditor);
    let fuzzy = shell
        .app_state
        .active_fuzzy_picker_buffer()
        .expect("file history should open as fuzzy buffer");
    assert_eq!(fuzzy.mode, CommandPaletteMode::FileHistory);
    assert!(
        !fuzzy.results.is_empty(),
        "history list should not be empty"
    );
    assert!(
        !fuzzy.preview_lines.is_empty(),
        "history diff preview should be populated"
    );

    let _ = std::fs::remove_file(file_path);
}

#[test]
fn fuzzy_picker_open_search_match_confirm_closes_results_buffer() {
    let mut shell = AppShell::new_for_tests().expect("create app shell");
    let root = std::env::temp_dir().join(format!(
        "netherize_fuzzy_confirm_close_{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&root).expect("create workspace");
    let target = root.join("match.rs");
    std::fs::write(&target, "alpha\nbeta\ngamma\n").expect("write target");
    let canonical_target = target.canonicalize().expect("canonical target");

    shell
        .app_state
        .attach_workspace(root.clone())
        .expect("attach workspace");
    shell
        .app_state
        .open_fuzzy_picker_buffer(CommandPaletteMode::LiveGrep);
    assert!(shell.handle_command(Command::FilePickerAppendQuery("beta".to_string())));
    assert!(shell.app_state.set_command_palette_results(
        CommandPaletteMode::LiveGrep,
        "beta",
        vec![
            crate::app::command_palette::CommandPaletteItem::search_match(
                "match.rs:2".to_string(),
                Some("beta".to_string()),
                target.clone(),
                2,
                1,
            )
        ],
    ));

    assert!(shell.handle_command(Command::FilePickerConfirmSelection));

    assert!(!shell.app_state.active_buffer_is_fuzzy_picker());
    assert_eq!(shell.app_state.current_mode(), EditorMode::Normal);
    assert_eq!(
        shell.app_state.active_file(),
        Some(canonical_target.as_path())
    );
    assert_eq!(shell.app_state.cursor_line_col(), (1, 0));

    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn references_selection_clears_stale_preview_lines() {
    let mut shell = AppShell::new_for_tests().expect("create app shell");
    shell
        .app_state
        .open_references_buffer(
            "References (2)",
            Some(PathBuf::from("origin.rs")),
            4,
            vec![
                crate::app::app_state::ReferencesBufferItem {
                    path: PathBuf::from("a.rs"),
                    relative_path: "a.rs".to_string(),
                    line: 10,
                    column: 2,
                    summary: "Ln 11, Col 3".to_string(),
                },
                crate::app::app_state::ReferencesBufferItem {
                    path: PathBuf::from("b.rs"),
                    relative_path: "b.rs".to_string(),
                    line: 20,
                    column: 5,
                    summary: "Ln 21, Col 6".to_string(),
                },
            ],
        )
        .expect("open references buffer");
    assert!(shell.app_state.set_active_references_preview(
        vec![crate::async_runtime::message::FilePreviewLine {
            line_number: 11,
            text: "hello".to_string(),
            is_target: true,
        }],
        String::new(),
        Vec::new(),
    ));

    assert!(shell.handle_command(Command::ReferencesSelectNext));

    assert!(
        shell
            .app_state
            .active_references_buffer()
            .expect("references buffer")
            .preview_lines
            .is_empty()
    );
}

#[test]
fn references_open_selection_closes_results_buffer() {
    let mut shell = AppShell::new_for_tests().expect("create app shell");
    let root = std::env::temp_dir().join(format!(
        "netherize_refs_confirm_close_{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&root).expect("create workspace");
    let origin = root.join("origin.rs");
    let target = root.join("target.rs");
    std::fs::write(&origin, "origin\n").expect("write origin");
    std::fs::write(&target, "one\ntwo\nthree\n").expect("write target");
    let canonical_target = target.canonicalize().expect("canonical target");

    shell
        .app_state
        .open_references_buffer(
            "References (1)",
            Some(origin.clone()),
            0,
            vec![crate::app::app_state::ReferencesBufferItem {
                path: target.clone(),
                relative_path: "target.rs".to_string(),
                line: 1,
                column: 0,
                summary: "Ln 2, Col 1".to_string(),
            }],
        )
        .expect("open references buffer");

    assert!(shell.handle_command(Command::ReferencesOpenSelection));

    assert!(!shell.app_state.active_buffer_is_references());
    assert_eq!(
        shell.app_state.active_file(),
        Some(canonical_target.as_path())
    );
    assert_eq!(shell.app_state.cursor_line_col(), (1, 0));

    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn startup_keeps_a_workspace_attached_for_global_search() {
    let shell = AppShell::new_for_tests().expect("create app shell");

    assert!(shell.app_state.workspace_root_path().is_some());
}

fn test_completion_item(
    label: &str,
    insert_text: &str,
) -> crate::async_runtime::message::LspCompletionItem {
    crate::async_runtime::message::LspCompletionItem {
        label: label.to_string(),
        detail: None,
        insert_text: Some(insert_text.to_string()),
        text_edit: None,
        text_edit_text: None,
        additional_text_edits: Vec::new(),
        kind: Some(3),
        documentation: None,
        data: None,
        source_path: None,
        import_path: None,
        export_kind: None,
        raw_json: None,
    }
}

fn lsp_insert_edit(
    line: u32,
    character: u32,
    new_text: &str,
) -> crate::async_runtime::message::LspTextEdit {
    crate::async_runtime::message::LspTextEdit {
        range: crate::async_runtime::message::LspRange {
            start: crate::async_runtime::message::LspPosition { line, character },
            end: crate::async_runtime::message::LspPosition { line, character },
        },
        new_text: new_text.to_string(),
    }
}

fn completion_temp_root(suffix: &str) -> PathBuf {
    std::env::temp_dir().join(format!(
        "netherize_completion_{suffix}_{}",
        std::process::id()
    ))
}

fn write_completion_file(path: &std::path::Path, text: &str) -> PathBuf {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).expect("create completion test dir");
    }
    std::fs::write(path, text).expect("write completion test file");
    path.canonicalize().expect("canonical completion test file")
}

fn open_completion_file(shell: &mut AppShell, path: &std::path::Path, text: &str) -> PathBuf {
    let canonical = write_completion_file(path, text);
    shell
        .app_state
        .open_file(canonical.clone())
        .expect("open completion test file");
    canonical
}

fn cached_ts_export(name: &str, source_path: &std::path::Path) -> crate::lsp::CachedSymbol {
    let source_path = source_path.to_path_buf();
    crate::lsp::CachedSymbol {
        name: name.to_string(),
        kind: "Function".to_string(),
        container_name: None,
        file_path: source_path.clone(),
        line: 0,
        character: 16,
        source_path: Some(source_path.clone()),
        import_path: Some(source_path.with_extension("").display().to_string()),
        export_kind: Some("named".to_string()),
    }
}

#[test]
fn completion_accept_replaces_typed_prefix_instead_of_inserting_after_it() {
    let mut shell = AppShell::new_for_tests().expect("create app shell");
    let root = completion_temp_root("replace_prefix");
    let _path = open_completion_file(
        &mut shell,
        &root.join("completion_accept.ts"),
        "MessageManager.ge",
    );
    let cache = crate::lsp::WorkspaceSymbolCache::new();
    let completion = crate::app::app_state::CompletionState::from_lsp_items(
        vec![crate::async_runtime::message::LspCompletionItem {
            label: "getInstance".to_string(),
            detail: Some("() -> MessageManager".to_string()),
            insert_text: Some("getInstance()".to_string()),
            text_edit: None,
            text_edit_text: None,
            additional_text_edits: Vec::new(),
            kind: Some(2),
            documentation: None,
            data: None,
            source_path: None,
            import_path: None,
            export_kind: None,
            raw_json: None,
        }],
        0,
        "MessageManager.ge".chars().count(),
        "MessageManager.".chars().count(),
        "ge".to_string(),
        &cache,
        None,
    );
    assert!(
        shell
            .app_state
            .jump_to_line_and_column(0, "MessageManager.ge".chars().count())
    );
    assert!(shell.app_state.set_completion(completion));

    assert!(shell.handle_command(Command::CompletionAccept));
    assert_eq!(
        shell.app_state.text_string(),
        "MessageManager.getInstance()"
    );
    assert!(shell.app_state.completion().is_none());
}

#[test]
fn completion_accept_deduplicates_trigger_char_in_insert_text() {
    // Scenario: user typed "message." and LSP returns insertText = ".getInstance()"
    // (trigger char included). Without dedup the result would be "message..getInstance()".
    let mut shell = AppShell::new_for_tests().expect("create app shell");
    let root = completion_temp_root("dedup_trigger");
    let _path = open_completion_file(&mut shell, &root.join("dedup_trigger.ts"), "message.");
    shell.lsp_completion_trigger_chars = vec!['.'];
    let cursor_col = "message.".chars().count();
    let cache = crate::lsp::WorkspaceSymbolCache::new();
    let completion = crate::app::app_state::CompletionState::from_lsp_items(
        vec![crate::async_runtime::message::LspCompletionItem {
            label: "getInstance".to_string(),
            detail: None,
            insert_text: Some(".getInstance()".to_string()),
            text_edit: None,
            text_edit_text: None,
            additional_text_edits: Vec::new(),
            kind: Some(2),
            documentation: None,
            data: None,
            source_path: None,
            import_path: None,
            export_kind: None,
            raw_json: None,
        }],
        0,
        cursor_col,
        cursor_col,
        String::new(),
        &cache,
        None,
    );
    assert!(shell.app_state.jump_to_line_and_column(0, cursor_col));
    assert!(shell.app_state.set_completion(completion));

    assert!(shell.handle_command(Command::CompletionAccept));
    assert_eq!(shell.app_state.text_string(), "message.getInstance()");
    assert!(shell.app_state.completion().is_none());
}

#[test]
fn completion_accept_applies_lsp_additional_import_edits_in_one_undo() {
    let mut shell = AppShell::new_for_tests().expect("create app shell");
    let root = completion_temp_root("lsp_import_edit");
    let _path = open_completion_file(&mut shell, &root.join("src/app.ts"), "con");
    let mut item = test_completion_item("connect", "connect");
    item.additional_text_edits = vec![lsp_insert_edit(
        0,
        0,
        "import { connect } from './api';\n",
    )];
    let cache = crate::lsp::WorkspaceSymbolCache::new();
    let completion = crate::app::app_state::CompletionState::from_lsp_items(
        vec![item],
        0,
        "con".chars().count(),
        0,
        "con".to_string(),
        &cache,
        None,
    );
    assert!(shell.app_state.jump_to_line_and_column(0, "con".chars().count()));
    assert!(shell.app_state.set_completion(completion));

    assert!(shell.handle_command(Command::CompletionAccept));
    assert_eq!(
        shell.app_state.text_string(),
        "import { connect } from './api';\nconnect"
    );

    assert!(shell.handle_command(Command::Undo));
    assert_eq!(shell.app_state.text_string(), "con");
}

#[test]
fn workspace_completion_fallback_merges_existing_named_import() {
    let mut shell = AppShell::new_for_tests().expect("create app shell");
    let root = completion_temp_root("merge_import");
    let app_path = root.join("src/app.ts");
    let source_path = write_completion_file(&root.join("src/utils.ts"), "export function connect() {}\n");
    let _path = open_completion_file(
        &mut shell,
        &app_path,
        "import { existing } from './utils';\n\ncon",
    );
    let cache = crate::lsp::WorkspaceSymbolCache::new();
    cache.insert_symbols("typescript", vec![cached_ts_export("connect", &source_path)]);
    let completion = crate::app::app_state::CompletionState::from_lsp_items(
        Vec::new(),
        2,
        3,
        0,
        "con".to_string(),
        &cache,
        Some("typescript"),
    );
    assert!(!completion.filtered_items.is_empty());
    assert!(shell.app_state.jump_to_line_and_column(2, 3));
    assert!(shell.app_state.set_completion(completion));

    assert!(shell.handle_command(Command::CompletionAccept));
    assert_eq!(
        shell.app_state.text_string(),
        "import { existing, connect } from './utils';\n\nconnect"
    );
}

#[test]
fn workspace_completion_fallback_inserts_new_named_import() {
    let mut shell = AppShell::new_for_tests().expect("create app shell");
    let text = "const value = con";
    let root = completion_temp_root("new_import");
    let source_path = write_completion_file(&root.join("src/api.ts"), "export function connect() {}\n");
    let _path = open_completion_file(&mut shell, &root.join("src/app.ts"), text);
    let cache = crate::lsp::WorkspaceSymbolCache::new();
    cache.insert_symbols("typescript", vec![cached_ts_export("connect", &source_path)]);
    let cursor_col = text.chars().count();
    let prefix_start = "const value = ".chars().count();
    let completion = crate::app::app_state::CompletionState::from_lsp_items(
        Vec::new(),
        0,
        cursor_col,
        prefix_start,
        "con".to_string(),
        &cache,
        Some("typescript"),
    );
    assert!(!completion.filtered_items.is_empty());
    assert!(shell.app_state.jump_to_line_and_column(0, cursor_col));
    assert!(shell.app_state.set_completion(completion));

    assert!(shell.handle_command(Command::CompletionAccept));
    assert_eq!(
        shell.app_state.text_string(),
        "import { connect } from './api';\nconst value = connect"
    );
}

#[test]
fn workspace_completion_fallback_same_file_symbol_has_no_import() {
    let mut shell = AppShell::new_for_tests().expect("create app shell");
    let root = completion_temp_root("same_file");
    let active_path = open_completion_file(&mut shell, &root.join("src/app.ts"), "con");
    let cache = crate::lsp::WorkspaceSymbolCache::new();
    cache.insert_symbols("typescript", vec![cached_ts_export("connect", &active_path)]);
    let completion = crate::app::app_state::CompletionState::from_lsp_items(
        Vec::new(),
        0,
        3,
        0,
        "con".to_string(),
        &cache,
        Some("typescript"),
    );
    assert!(!completion.filtered_items.is_empty());
    assert!(shell.app_state.jump_to_line_and_column(0, 3));
    assert!(shell.app_state.set_completion(completion));

    assert!(shell.handle_command(Command::CompletionAccept));
    assert_eq!(shell.app_state.text_string(), "connect");
}

#[test]
fn welcome_hides_while_command_palette_is_visible() {
    let mut shell = AppShell::new_for_tests().expect("create app shell");

    assert!(shell.should_show_welcome());
    assert!(shell.handle_command(Command::OpenCommandPalette));
    assert!(!shell.should_show_welcome());
}

#[test]
fn welcome_stays_visible_for_regular_editor_commands() {
    let mut shell = AppShell::new_for_tests().expect("create app shell");

    assert!(shell.should_show_welcome());
    let _ = shell.handle_command(Command::MoveDown);
    assert!(shell.should_show_welcome());
}

#[test]
fn welcome_hides_when_opening_explorer_or_terminal_surfaces() {
    let mut explorer_shell = AppShell::new_for_tests().expect("create app shell");
    assert!(explorer_shell.should_show_welcome());
    assert!(explorer_shell.handle_command(Command::FocusExplorer));
    assert!(!explorer_shell.should_show_welcome());

    let mut terminal_shell = AppShell::new_for_tests().expect("create app shell");
    assert!(terminal_shell.should_show_welcome());
    assert!(terminal_shell.handle_command(Command::FocusTerminal));
    assert!(!terminal_shell.should_show_welcome());

    let mut bottom_dock_shell = AppShell::new_for_tests().expect("create app shell");
    assert!(bottom_dock_shell.should_show_welcome());
    assert!(bottom_dock_shell.handle_command(Command::ToggleBottomDock));
    assert!(!bottom_dock_shell.should_show_welcome());
}

#[test]
fn welcome_hides_when_opening_palette_surfaces() {
    let mut vim_shell = AppShell::new_for_tests().expect("create app shell");
    assert!(vim_shell.should_show_welcome());
    assert!(vim_shell.handle_command(Command::OpenVimCommand));
    assert!(!vim_shell.app_state.is_initial_launch_welcome());

    let root = std::env::temp_dir().join(format!(
        "netherize_welcome_file_picker_{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&root).expect("create workspace");

    let mut picker_shell = AppShell::new_for_tests().expect("create app shell");
    picker_shell
        .app_state
        .attach_workspace(root.clone())
        .expect("attach workspace");
    let _ = picker_shell.app_state.set_initial_launch_welcome(true);

    assert!(picker_shell.should_show_welcome());
    assert!(picker_shell.handle_command(Command::OpenFilePicker));
    assert!(!picker_shell.app_state.is_initial_launch_welcome());

    let _ = std::fs::remove_dir_all(root);
}

// ── Resize mode tests ─────────────────────────────────────────────────────────
// Clamp bounds: left [160, 1280], right [180, 1440], bottom [120, 1040]
// Step: 20px

#[test]
fn resize_left_sidebar_increases_width_on_increase_command() {
    let mut shell = AppShell::new_for_tests().expect("create app shell");
    shell.panel_state.left.visible = true;
    shell.panel_state.left.size_px = 280.0;
    shell.focus_manager.set(FocusTarget::LeftSidebar);

    let changed = shell.handle_command(Command::ResizeIncreaseWidth);

    assert!(changed);
    assert_eq!(shell.panel_state.left.size_px, 300.0);
    assert_eq!(shell.ui_config.docks.left.size_px, 300.0);
}

#[test]
fn resize_left_sidebar_decreases_width_on_decrease_command() {
    let mut shell = AppShell::new_for_tests().expect("create app shell");
    shell.panel_state.left.visible = true;
    shell.panel_state.left.size_px = 280.0;
    shell.focus_manager.set(FocusTarget::LeftSidebar);

    let changed = shell.handle_command(Command::ResizeDecreaseWidth);

    assert!(changed);
    assert_eq!(shell.panel_state.left.size_px, 260.0);
    assert_eq!(shell.ui_config.docks.left.size_px, 260.0);
}

#[test]
fn resize_left_sidebar_clamps_at_max_1280() {
    let mut shell = AppShell::new_for_tests().expect("create app shell");
    shell.panel_state.left.visible = true;
    shell.panel_state.left.size_px = 1270.0;
    shell.focus_manager.set(FocusTarget::LeftSidebar);

    assert!(shell.handle_command(Command::ResizeIncreaseWidth));
    assert_eq!(shell.panel_state.left.size_px, 1280.0);

    // Already at max — no change
    let changed = shell.handle_command(Command::ResizeIncreaseWidth);
    assert!(!changed);
    assert_eq!(shell.panel_state.left.size_px, 1280.0);
}

#[test]
fn resize_left_sidebar_clamps_at_min_160() {
    let mut shell = AppShell::new_for_tests().expect("create app shell");
    shell.panel_state.left.visible = true;
    shell.panel_state.left.size_px = 170.0;
    shell.focus_manager.set(FocusTarget::LeftSidebar);

    assert!(shell.handle_command(Command::ResizeDecreaseWidth));
    assert_eq!(shell.panel_state.left.size_px, 160.0);

    let changed = shell.handle_command(Command::ResizeDecreaseWidth);
    assert!(!changed);
    assert_eq!(shell.panel_state.left.size_px, 160.0);
}

#[test]
fn resize_right_sidebar_clamps_at_max_1440() {
    let mut shell = AppShell::new_for_tests().expect("create app shell");
    shell.panel_state.right.visible = true;
    shell.panel_state.right.size_px = 1430.0;
    shell.focus_manager.set(FocusTarget::RightSidebar);

    assert!(shell.handle_command(Command::ResizeIncreaseWidth));
    assert_eq!(shell.panel_state.right.size_px, 1440.0);

    let changed = shell.handle_command(Command::ResizeIncreaseWidth);
    assert!(!changed);
}

#[test]
fn resize_right_sidebar_clamps_at_min_180() {
    let mut shell = AppShell::new_for_tests().expect("create app shell");
    shell.panel_state.right.visible = true;
    shell.panel_state.right.size_px = 190.0;
    shell.focus_manager.set(FocusTarget::RightSidebar);

    assert!(shell.handle_command(Command::ResizeDecreaseWidth));
    assert_eq!(shell.panel_state.right.size_px, 180.0);

    let changed = shell.handle_command(Command::ResizeDecreaseWidth);
    assert!(!changed);
}

#[test]
fn resize_bottom_panel_clamps_at_max_1040() {
    let mut shell = AppShell::new_for_tests().expect("create app shell");
    shell.panel_state.bottom.visible = true;
    shell.panel_state.bottom.size_px = 1030.0;
    shell.focus_manager.set(FocusTarget::BottomPanel);

    assert!(shell.handle_command(Command::ResizeIncreaseHeight));
    assert_eq!(shell.panel_state.bottom.size_px, 1040.0);

    let changed = shell.handle_command(Command::ResizeIncreaseHeight);
    assert!(!changed);
}

#[test]
fn resize_bottom_panel_clamps_at_min_120() {
    let mut shell = AppShell::new_for_tests().expect("create app shell");
    shell.panel_state.bottom.visible = true;
    shell.panel_state.bottom.size_px = 130.0;
    shell.focus_manager.set(FocusTarget::BottomPanel);

    assert!(shell.handle_command(Command::ResizeDecreaseHeight));
    assert_eq!(shell.panel_state.bottom.size_px, 120.0);

    let changed = shell.handle_command(Command::ResizeDecreaseHeight);
    assert!(!changed);
}

#[test]
fn resize_returns_false_when_panel_not_visible() {
    let mut shell = AppShell::new_for_tests().expect("create app shell");
    shell.panel_state.left.visible = false;
    shell.panel_state.left.size_px = 280.0;
    shell.focus_manager.set(FocusTarget::LeftSidebar);

    let changed = shell.handle_command(Command::ResizeIncreaseWidth);

    assert!(!changed);
    assert_eq!(shell.panel_state.left.size_px, 280.0);
}

#[test]
fn resize_center_editor_increase_width_shrinks_left_sidebar_only() {
    let mut shell = AppShell::new_for_tests().expect("create app shell");
    shell.panel_state.left.visible = true;
    shell.panel_state.left.size_px = 280.0;
    shell.panel_state.right.visible = true;
    shell.panel_state.right.size_px = 320.0;
    shell.focus_manager.set(FocusTarget::CenterEditor);

    let changed = shell.handle_command(Command::ResizeIncreaseLeftWidth);

    assert!(changed);
    assert_eq!(shell.panel_state.left.size_px, 260.0);
    assert_eq!(shell.panel_state.right.size_px, 320.0);
}

#[test]
fn resize_center_editor_decrease_left_width_grows_left_sidebar_only() {
    let mut shell = AppShell::new_for_tests().expect("create app shell");
    shell.panel_state.left.visible = true;
    shell.panel_state.left.size_px = 280.0;
    shell.panel_state.right.visible = true;
    shell.panel_state.right.size_px = 320.0;
    shell.focus_manager.set(FocusTarget::CenterEditor);

    let changed = shell.handle_command(Command::ResizeDecreaseLeftWidth);

    assert!(changed);
    assert_eq!(shell.panel_state.left.size_px, 300.0);
    assert_eq!(shell.panel_state.right.size_px, 320.0);
}

#[test]
fn resize_center_editor_increase_right_width_shrinks_right_sidebar_only() {
    let mut shell = AppShell::new_for_tests().expect("create app shell");
    shell.panel_state.left.visible = true;
    shell.panel_state.left.size_px = 280.0;
    shell.panel_state.right.visible = true;
    shell.panel_state.right.size_px = 320.0;
    shell.focus_manager.set(FocusTarget::CenterEditor);

    let changed = shell.handle_command(Command::ResizeIncreaseRightWidth);

    assert!(changed);
    assert_eq!(shell.panel_state.left.size_px, 280.0);
    assert_eq!(shell.panel_state.right.size_px, 300.0);
}

#[test]
fn resize_center_editor_decrease_width_grows_right_sidebar_only() {
    let mut shell = AppShell::new_for_tests().expect("create app shell");
    shell.panel_state.left.visible = true;
    shell.panel_state.left.size_px = 280.0;
    shell.panel_state.right.visible = true;
    shell.panel_state.right.size_px = 320.0;
    shell.focus_manager.set(FocusTarget::CenterEditor);

    let changed = shell.handle_command(Command::ResizeDecreaseRightWidth);

    assert!(changed);
    assert_eq!(shell.panel_state.left.size_px, 280.0);
    assert_eq!(shell.panel_state.right.size_px, 340.0);
}

#[test]
fn resize_panel_and_ui_config_stay_in_sync() {
    let mut shell = AppShell::new_for_tests().expect("create app shell");
    shell.panel_state.bottom.visible = true;
    shell.panel_state.bottom.size_px = 230.0;
    shell.focus_manager.set(FocusTarget::BottomPanel);

    shell.handle_command(Command::ResizeIncreaseHeight);

    assert_eq!(
        shell.panel_state.bottom.size_px,
        shell.ui_config.docks.bottom.size_px
    );
}
