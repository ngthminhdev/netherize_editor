use super::*;
use crate::app::clipboard::ClipboardProvider;
use crate::app::input::{InputRouteOutcome, NormalizedInput};
use winit::keyboard::{KeyCode, ModifiersState};

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
fn canvas_open_toggles_off_when_canvas_has_navigation_focus() {
    let mut shell = AppShell::new_for_tests().expect("create app shell");
    let path = std::env::temp_dir().join(format!(
        "netherize_canvas_toggle_{}.rs",
        std::process::id()
    ));
    std::fs::write(&path, "fn main() {}\n").expect("write canvas fixture");
    shell.app_state = AppState::new(path.clone());
    shell.app_state.open_file(path.clone()).expect("open canvas fixture");
    let _ = shell.app_state.apply_mode_event(ModeEvent::EnterNormal);
    assert!(shell.app_state.open_canvas(480.0, 320.0, 20.0));
    assert_eq!(
        shell.app_state.canvas_interaction(),
        Some(crate::canvas::CanvasInteraction::Navigate)
    );
    assert_eq!(shell.build_context().focus, InputFocusContext::Canvas);

    assert!(shell.handle_command(Command::CanvasOpen));

    assert!(!shell.app_state.is_canvas_active());
    let _ = std::fs::remove_file(path);
}

/// Regression (bug-020): once you've navigated to a different source file,
/// pressing F8 must open a FRESH canvas on the file in front of you — NOT
/// teleport the editor back to the old focal file to restore the stale canvas.
#[test]
fn f8_on_a_different_file_opens_fresh_canvas_here_not_old_focal() {
    let mut shell = AppShell::new_for_tests().expect("create app shell");
    let nanos = std::process::id();
    let dir = std::env::temp_dir().join(format!("netherize_canvas_switch_{nanos}"));
    std::fs::create_dir_all(&dir).unwrap();
    let a = dir.join("a.rs");
    let b = dir.join("b.rs");
    std::fs::write(&a, "fn alpha() {}\n").unwrap();
    std::fs::write(&b, "fn beta() {}\n").unwrap();

    shell.app_state = AppState::new(a.clone());
    shell.app_state.open_file(a.clone()).expect("open a");
    let _ = shell.app_state.apply_mode_event(ModeEvent::EnterNormal);
    assert!(shell.app_state.open_canvas(480.0, 320.0, 20.0));
    let a_active = shell.app_state.active_file().map(|p| p.to_path_buf());
    assert_eq!(shell.app_state.canvas_focal_file(), a_active);

    // Navigate to file B, then F8.
    shell.app_state.open_file(b.clone()).expect("open b");
    let b_active = shell.app_state.active_file().map(|p| p.to_path_buf());
    assert_ne!(a_active, b_active);
    assert!(shell.handle_command(Command::CanvasOpen));

    // Editor stays on B (no teleport back to A) and the canvas is now bound to B.
    assert_eq!(
        shell.app_state.active_file().map(|p| p.to_path_buf()),
        b_active,
        "F8 must not switch the editor back to the old focal file"
    );
    assert_eq!(
        shell.app_state.canvas_focal_file(),
        b_active,
        "a fresh canvas for the current file, not the stale one for A"
    );

    let _ = std::fs::remove_dir_all(dir);
}

/// Regression (bug-019): F8 before the LSP server is ready must NOT leave the
/// canvas spinning on "Loading source function…" forever. The source-function
/// fetch is deferred (no phantom request id that never clears) and flagged so
/// the `LspServerReady` handler can fire it automatically — no F8 spamming.
#[test]
fn canvas_definition_defers_until_lsp_ready_instead_of_loading_forever() {
    let mut shell = AppShell::new_for_tests().expect("create app shell");
    let path = std::env::temp_dir().join(format!(
        "netherize_canvas_defer_{}.rs",
        std::process::id()
    ));
    std::fs::write(&path, "fn main() {}\n").expect("write canvas fixture");
    shell.app_state = AppState::new(path.clone());
    shell.app_state.open_file(path.clone()).expect("open canvas fixture");
    let _ = shell.app_state.apply_mode_event(ModeEvent::EnterNormal);
    assert!(shell.app_state.open_canvas(480.0, 320.0, 20.0));

    // Server still starting → defer, and crucially leave no stuck request id.
    shell.active_lsp_server = None;
    shell.pending_lsp_server = Some(ActiveLspServer {
        server_name: "rust-analyzer".to_string(),
        root_path: std::env::temp_dir(),
    });
    assert!(!shell.canvas_submit_definition());
    assert!(
        shell.canvas_def_deferred,
        "fetch must be deferred while the LSP is starting"
    );
    assert!(
        shell.canvas_def_request_id.is_none(),
        "no phantom request id that would spin Loading forever"
    );

    // No LSP at all (nothing starting) → don't defer; fall through to the
    // 'Nothing to show' hint instead of a permanent spinner.
    shell.pending_lsp_server = None;
    shell.canvas_def_deferred = false;
    assert!(!shell.canvas_submit_definition());
    assert!(!shell.canvas_def_deferred);

    let _ = std::fs::remove_file(path);
}

/// Regression: the in-card edit session is KEPT stashed after leaving edit mode
/// (to resume unsaved edits), so once you back the canvas to the Background — or
/// open a card as a real buffer with `o` — the main editor must regain full vim
/// control. Before the `EditCard`-gate fix, the bare session check hijacked
/// hjkl/d/c/b/w into the (hidden) card and the editor looked frozen.
#[test]
fn background_canvas_with_stashed_session_lets_main_editor_move() {
    let mut shell = AppShell::new_for_tests().expect("create app shell");
    let nanos = std::process::id();
    let dir = std::env::temp_dir().join(format!("netherize_canvas_bg_{nanos}"));
    std::fs::create_dir_all(&dir).unwrap();
    let foo = dir.join("foo.rs");
    let bar = dir.join("bar.rs");
    std::fs::write(&foo, "fn main() {\n    helper();\n}\n").unwrap();
    std::fs::write(&bar, "fn helper() {\n    let x = 1;\n}\n").unwrap();

    shell.app_state = AppState::new(foo.clone());
    shell.app_state.open_file(foo.clone()).expect("open foo");
    let _ = shell.app_state.apply_mode_event(ModeEvent::EnterNormal);
    assert!(shell.app_state.open_canvas(480.0, 320.0, 20.0));
    let bar_canon = bar.canonicalize().unwrap();
    shell.app_state.canvas_add_relations(vec![(
        crate::canvas::BlockRelation::Definition,
        crate::canvas::BlockOrigin {
            path: bar_canon,
            start_byte: 0,
            end_byte: 1,
            symbol_name: "helper".into(),
            lsp_line: 0,
            lsp_character: 0,
        },
        {
            let mut s = crate::canvas::BlockSnapshot::default();
            s.text = "fn helper() {".into();
            s
        },
    )]);

    // Enter then leave edit → Navigate, but the session stays stashed.
    assert!(shell.app_state.canvas_begin_edit());
    assert!(shell.app_state.canvas_end_edit());
    // Back the canvas to the editor (S3). Session is STILL present.
    assert!(shell.app_state.canvas_enter_background());
    assert!(
        shell.app_state.canvas_edit_session_block().is_some(),
        "session is intentionally kept stashed for resume"
    );

    let (line_before, _) = shell.app_state.cursor_line_col();
    assert_eq!(line_before, 0);
    // `j` in the Background must move the MAIN editor, not the hidden card.
    assert!(shell.handle_command(Command::MoveDown));
    let (line_after, _) = shell.app_state.cursor_line_col();
    assert_eq!(line_after, 1, "main editor cursor must advance — not the card");

    let _ = std::fs::remove_dir_all(dir);
}

/// Helper: an open canvas (focal = foo.rs) with one relation card, sitting in the
/// Background (so the user is "coding in the main editor" with cards floating).
fn shell_with_background_canvas_card() -> (AppShell, std::path::PathBuf, crate::canvas::BlockId) {
    let mut shell = AppShell::new_for_tests().expect("create app shell");
    let nanos = std::process::id();
    let dir = std::env::temp_dir().join(format!("netherize_canvas_click_{nanos}"));
    std::fs::create_dir_all(&dir).unwrap();
    let foo = dir.join("foo.rs");
    let bar = dir.join("bar.rs");
    std::fs::write(&foo, "fn main() {\n    helper();\n}\n").unwrap();
    std::fs::write(&bar, "fn helper() {\n    let x = 1;\n}\n").unwrap();
    shell.app_state = AppState::new(foo.clone());
    shell.app_state.open_file(foo.clone()).expect("open foo");
    let _ = shell.app_state.apply_mode_event(ModeEvent::EnterNormal);
    assert!(shell.app_state.open_canvas(480.0, 320.0, 20.0));
    shell.app_state.canvas_add_relations(vec![(
        crate::canvas::BlockRelation::Definition,
        crate::canvas::BlockOrigin {
            path: bar.canonicalize().unwrap(),
            start_byte: 0,
            end_byte: 1,
            symbol_name: "helper".into(),
            lsp_line: 0,
            lsp_character: 0,
        },
        {
            let mut s = crate::canvas::BlockSnapshot::default();
            s.text = "fn helper() {".into();
            s
        },
    )]);
    let card_id = shell
        .app_state
        .canvas()
        .unwrap()
        .blocks
        .iter()
        .find(|b| b.relation != crate::canvas::BlockRelation::Focal)
        .map(|b| b.id)
        .unwrap();
    assert!(shell.app_state.canvas_enter_background());
    (shell, dir, card_id)
}

/// Mouse click on a floating canvas card while coding in the main editor focuses
/// that card AND enters edit (= F8 → Enter), so typing goes into the card.
#[test]
fn clicking_a_canvas_card_focuses_it_and_enters_edit() {
    let (mut shell, dir, card_id) = shell_with_background_canvas_card();
    assert_eq!(
        shell.app_state.canvas_interaction(),
        Some(crate::canvas::CanvasInteraction::Background)
    );

    assert!(shell.focus_canvas_card_for_click(card_id));

    match shell.app_state.canvas_interaction() {
        Some(crate::canvas::CanvasInteraction::EditCard { block, .. }) => {
            assert_eq!(block, card_id, "the clicked card is the edit target")
        }
        other => panic!("expected EditCard, got {other:?}"),
    }
    let _ = std::fs::remove_dir_all(dir);
}

/// Mouse click on the main editor while editing a card hands keyboard focus back
/// to the editor (leaves the card edit, canvas drops to Background, focus = center).
#[test]
fn clicking_main_editor_while_editing_card_returns_focus_to_editor() {
    let (mut shell, dir, card_id) = shell_with_background_canvas_card();
    shell.app_state.canvas_focus_block(card_id);
    assert!(shell.app_state.canvas_begin_edit());
    assert!(matches!(
        shell.app_state.canvas_interaction(),
        Some(crate::canvas::CanvasInteraction::EditCard { .. })
    ));
    // Pretend focus had drifted elsewhere; clicking main must pull it back.
    shell.focus_manager.set(FocusTarget::LeftSidebar);

    assert!(shell.focus_main_editor_from_canvas());

    assert_eq!(
        shell.app_state.canvas_interaction(),
        Some(crate::canvas::CanvasInteraction::Background),
        "card edit left, canvas floats in the background"
    );
    assert_eq!(shell.focus_manager.current(), FocusTarget::CenterEditor);
    let _ = std::fs::remove_dir_all(dir);
}

/// Regression (bug-022): while editing card 1, clicking card 2 must switch the
/// edit target straight to card 2 — NOT focus the main editor first (which forced
/// a second click). The fix keeps cards mouse-interactive in the EditCard state.
#[test]
fn clicking_card_2_while_editing_card_1_switches_edit_directly() {
    let mut shell = AppShell::new_for_tests().expect("create app shell");
    let nanos = std::process::id();
    let dir = std::env::temp_dir().join(format!("netherize_canvas_switch2_{nanos}"));
    std::fs::create_dir_all(&dir).unwrap();
    let foo = dir.join("foo.rs");
    let bar = dir.join("bar.rs");
    let baz = dir.join("baz.rs");
    std::fs::write(&foo, "fn main() {\n    a();\n    b();\n}\n").unwrap();
    std::fs::write(&bar, "fn a() {}\n").unwrap();
    std::fs::write(&baz, "fn b() {}\n").unwrap();
    shell.app_state = AppState::new(foo.clone());
    shell.app_state.open_file(foo.clone()).expect("open foo");
    let _ = shell.app_state.apply_mode_event(ModeEvent::EnterNormal);
    assert!(shell.app_state.open_canvas(480.0, 320.0, 20.0));
    for (p, sym) in [(&bar, "a"), (&baz, "b")] {
        shell.app_state.canvas_add_relations(vec![(
            crate::canvas::BlockRelation::Caller,
            crate::canvas::BlockOrigin {
                path: p.canonicalize().unwrap(),
                start_byte: 0,
                end_byte: 1,
                symbol_name: sym.into(),
                lsp_line: 0,
                lsp_character: 0,
            },
            {
                let mut s = crate::canvas::BlockSnapshot::default();
                s.text = format!("fn {sym}() {{}}");
                s
            },
        )]);
    }
    let cards: Vec<crate::canvas::BlockId> = shell
        .app_state
        .canvas()
        .unwrap()
        .blocks
        .iter()
        .filter(|b| b.relation != crate::canvas::BlockRelation::Focal)
        .map(|b| b.id)
        .collect();
    let (card1, card2) = (cards[0], cards[1]);

    // Editing card 1.
    shell.app_state.canvas_focus_block(card1);
    assert!(shell.app_state.canvas_begin_edit());
    assert!(matches!(
        shell.app_state.canvas_interaction(),
        Some(crate::canvas::CanvasInteraction::EditCard { block, .. }) if block == card1
    ));
    // Cards must still be mouse-interactive while editing (so card 2 is hit-tested).
    assert!(shell.app_state.canvas_cards_interactive());

    // "Click" card 2 → edit target jumps straight to card 2 (no editor detour).
    assert!(shell.focus_canvas_card_for_click(card2));
    match shell.app_state.canvas_interaction() {
        Some(crate::canvas::CanvasInteraction::EditCard { block, .. }) => {
            assert_eq!(block, card2, "edit switched directly to card 2")
        }
        other => panic!("expected EditCard(card2), got {other:?}"),
    }
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn insert_edit_clears_stale_semantic_highlight_spans() {
    let mut shell = AppShell::new_for_tests().expect("create app shell");
    let text = "bootstrap.NewApp";
    shell.app_state = AppState::from_text(PathBuf::from("semantic-highlight-edit.ts"), text);
    assert!(
        shell
            .app_state
            .jump_to_line_and_column(0, text.chars().count())
    );
    shell.semantic_highlight_spans = vec![crate::syntax::highlight::HighlightSpan {
        range: 0.."bootstrap".len(),
        category: crate::syntax::highlight::HighlightCategory::Variable,
    }];
    assert!(
        shell
            .app_state
            .set_semantic_symbol_highlights(vec![(0, "bootstrap".len())])
    );
    shell.editor_needs_layout = false;
    shell.editor_caret_needs_layout = false;

    assert!(shell.handle_command(Command::Backspace));

    assert!(shell.semantic_highlight_spans.is_empty());
    assert!(shell.app_state.semantic_symbol_highlights().is_empty());
    assert!(shell.editor_needs_layout);
}

#[test]
fn operator_delete_clears_stale_semantic_symbol_highlights() {
    let mut shell = AppShell::new_for_tests().expect("create app shell");
    let text = "bootstrap.NewApp\nsecond line";
    shell.app_state = AppState::from_text(PathBuf::from("semantic-highlight-dd.ts"), text);
    let _ = shell.app_state.apply_mode_event(ModeEvent::EnterNormal);
    assert!(
        shell
            .app_state
            .set_semantic_symbol_highlights(vec![(0, "bootstrap".len())])
    );
    let revision_before = shell.semantic_highlight_request_revision;

    assert!(shell.handle_command(Command::Operate {
        op: crate::core::commands::Operator::Delete,
        target: crate::core::commands::OperationTarget::CurrentLine,
    }));

    assert!(shell.app_state.semantic_symbol_highlights().is_empty());
    assert!(shell.semantic_highlight_request_revision > revision_before);
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
fn settings_exposes_ai_inline_config_items() {
    let mut shell = AppShell::new_for_tests().expect("create app shell");
    shell.handle_command(Command::OpenSettings);
    let settings = shell
        .app_state
        .active_settings_buffer()
        .expect("settings buffer");

    // The AI section must surface the editable endpoint/model/tuning fields, not
    // just the on/off toggle — otherwise they would only be reachable by hand-
    // editing config/ai.toml.
    let has =
        |pred: fn(&crate::app::app_state::SettingItem) -> bool| settings.items.iter().any(pred);
    assert!(has(|i| matches!(
        i,
        crate::app::app_state::SettingItem::AiApiUrl { .. }
    )));
    assert!(has(|i| matches!(
        i,
        crate::app::app_state::SettingItem::AiModel { .. }
    )));
    assert!(has(|i| matches!(
        i,
        crate::app::app_state::SettingItem::AiApiKey { .. }
    )));
    assert!(has(|i| matches!(
        i,
        crate::app::app_state::SettingItem::AiEndpointKind { .. }
    )));
    assert!(has(|i| matches!(
        i,
        crate::app::app_state::SettingItem::AiMaxTokens { .. }
    )));
    assert!(has(|i| matches!(
        i,
        crate::app::app_state::SettingItem::AiPrefixChars { .. }
    )));
    assert!(has(|i| matches!(
        i,
        crate::app::app_state::SettingItem::AiSuffixChars { .. }
    )));
    assert!(has(|i| matches!(
        i,
        crate::app::app_state::SettingItem::AiDebounceMs { .. }
    )));
    assert!(has(|i| matches!(
        i,
        crate::app::app_state::SettingItem::LeetCodeAi { .. }
    )));
    assert!(has(|i| matches!(
        i,
        crate::app::app_state::SettingItem::LeetCodeAiApiUrl { .. }
    )));
    assert!(has(|i| matches!(
        i,
        crate::app::app_state::SettingItem::LeetCodeAiModel { .. }
    )));
    assert!(has(|i| matches!(
        i,
        crate::app::app_state::SettingItem::LeetCodeAiApiKey { .. }
    )));
    assert!(has(|i| matches!(
        i,
        crate::app::app_state::SettingItem::LeetCodeAiEndpointKind { .. }
    )));
    assert!(has(|i| matches!(
        i,
        crate::app::app_state::SettingItem::LeetCodeAiReasoningEffort { .. }
    )));
}

#[test]
fn fetch_leetcode_command_opens_problem_input_palette() {
    let mut shell = AppShell::new_for_tests().expect("create app shell");
    assert!(shell.handle_command(Command::FetchLeetCodeProblem));
    assert_eq!(
        shell.app_state.command_palette_mode(),
        Some(crate::app::command_palette::CommandPaletteMode::LeetCodeProblemInput)
    );
    assert_eq!(shell.app_state.current_mode(), EditorMode::PaletteFocus);
}

#[test]
fn fetch_leetcode_from_command_list_keeps_problem_input_open() {
    let mut shell = AppShell::new_for_tests().expect("create app shell");
    // Open the command palette in its default command-list mode.
    assert!(shell.handle_command(Command::OpenCommandPalette));
    // Simulate the user selecting the "Fetch LeetCode Problem" entry.
    assert!(shell.app_state.set_command_palette_results(
        CommandPaletteMode::CommandPalette,
        "",
        vec![crate::app::command_palette::CommandPaletteItem::command(
            "runner.fetch_leetcode_problem",
            "Fetch LeetCode Problem",
        )],
    ));
    assert!(shell.handle_command(Command::FilePickerConfirmSelection));
    // The confirm flow must keep the problem-input prompt open with palette
    // focus instead of tearing it down and returning focus to the editor.
    assert_eq!(
        shell.app_state.command_palette_mode(),
        Some(CommandPaletteMode::LeetCodeProblemInput)
    );
    assert_eq!(shell.app_state.current_mode(), EditorMode::PaletteFocus);
}

#[test]
fn fetched_leetcode_result_opens_file_and_populates_test_runner() {
    let mut shell = AppShell::new_for_tests().expect("create app shell");
    let path = std::env::temp_dir().join(format!(
        "netherize_fetched_leetcode_{}.js",
        std::process::id()
    ));
    std::fs::write(&path, "console.log(1);\n").expect("write fetched solution");

    shell.on_worker_result(crate::async_runtime::message::WorkerResult {
        request_id: 1,
        revision_id: 0,
        topic: crate::async_runtime::message::RequestTopic::LeetCode,
        payload: crate::async_runtime::message::WorkerResultPayload::LeetCodeProblemFetched {
            title: "Two Sum".into(),
            title_slug: "two-sum".into(),
            language_key: "javascript".into(),
            file_path: path.clone(),
            cases: vec![crate::runner::leetcode_api::LeetCodeTestCase {
                input: r#"{"nums":[2,7,11,15],"target":9}"#.into(),
                expected: "[0,1]".into(),
            }],
        },
    });

    let canonical = path.canonicalize().expect("canonical fetched solution");
    assert_eq!(shell.app_state.active_file(), Some(canonical.as_path()));
    assert_eq!(shell.app_state.test_runner.cases.len(), 1);
    assert_eq!(shell.app_state.test_runner.cases[0].expected, "[0,1]");
    assert_eq!(
        shell.panel_state.right.active_tab_id(),
        Some(crate::workbench::panel_state::PanelTabId::TestRunner)
    );
    let _ = std::fs::remove_file(path);
}

#[test]
fn settings_activate_begins_text_edit_for_ai_model() {
    let mut shell = AppShell::new_for_tests().expect("create app shell");
    shell.handle_command(Command::OpenSettings);
    let model_value = {
        let settings = shell
            .app_state
            .active_settings_buffer_mut()
            .expect("settings buffer");
        let (idx, value) = settings
            .items
            .iter()
            .enumerate()
            .find_map(|(idx, item)| match item {
                crate::app::app_state::SettingItem::AiModel { current } => {
                    Some((idx, current.clone()))
                }
                _ => None,
            })
            .expect("ai model setting present");
        settings.selected_index = idx;
        value
    };

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
        crate::app::app_state::SettingsEditingKind::AiModel
    );
    // Editing seeds the draft from the live config value so the user edits the
    // real model id rather than a blank field.
    assert_eq!(editing.draft, model_value);
}

#[test]
fn settings_activate_begins_text_edit_for_leetcode_ai_model() {
    let mut shell = AppShell::new_for_tests().expect("create app shell");
    shell.handle_command(Command::OpenSettings);
    let model_value = {
        let settings = shell
            .app_state
            .active_settings_buffer_mut()
            .expect("settings buffer");
        let (idx, value) = settings
            .items
            .iter()
            .enumerate()
            .find_map(|(idx, item)| match item {
                crate::app::app_state::SettingItem::LeetCodeAiModel { current } => {
                    Some((idx, current.clone()))
                }
                _ => None,
            })
            .expect("leetcode ai model setting present");
        settings.selected_index = idx;
        value
    };

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
        crate::app::app_state::SettingsEditingKind::LeetCodeAiModel
    );
    assert_eq!(editing.draft, model_value);
}

#[test]
fn settings_text_edit_works_after_opening_from_right_terminal_focus() {
    let mut shell = AppShell::new_for_tests().expect("create app shell");
    shell.panel_state.right.visible = true;
    shell.panel_state.right.switch_to_tab(PanelTabId::Terminal);
    shell.focus_manager.set(FocusTarget::RightSidebar);
    let _ = shell.app_state.apply_mode_event(ModeEvent::FocusTerminal);

    assert!(shell.handle_command(Command::OpenSettings));
    assert_eq!(shell.focus_manager.current(), FocusTarget::CenterEditor);
    assert_eq!(shell.app_state.current_mode(), EditorMode::Normal);

    let settings = shell
        .app_state
        .active_settings_buffer_mut()
        .expect("settings buffer");
    settings.selected_index = settings
        .items
        .iter()
        .position(|item| {
            matches!(
                item,
                crate::app::app_state::SettingItem::RightSidebarWidth { .. }
            )
        })
        .expect("right sidebar width setting");

    assert!(shell.handle_command(Command::SettingsActivate));
    assert_eq!(shell.app_state.current_mode(), EditorMode::Insert);
    let context = shell.build_context();
    assert_eq!(context.focus, InputFocusContext::SettingsTab);

    let routed = shell.input_handler.route_normalized_input(
        NormalizedInput {
            physical_key: Some(KeyCode::Digit7),
            named_key: None,
            text: Some("7".to_string()),
            modifiers: ModifiersState::empty(),
        },
        &shell.input_map,
        context,
        std::time::Instant::now(),
    );
    match routed {
        Some(InputRouteOutcome::Dispatch(translated)) => {
            assert_eq!(
                translated.command,
                Command::FilePickerAppendQuery("7".to_string())
            );
            assert!(shell.handle_command(translated.command));
        }
        other => panic!("expected settings text append route, got {:?}", other),
    }

    let settings = shell
        .app_state
        .active_settings_buffer()
        .expect("settings buffer");
    let editing = settings.editing.as_ref().expect("editing state");
    assert!(editing.draft.ends_with('7'));
}

#[test]
fn right_terminal_focus_cmd_comma_routes_to_settings() {
    let mut shell = AppShell::new_for_tests().expect("create app shell");
    shell.app_state = AppState::from_text(PathBuf::from("main.rs"), "fn main() {}\n");
    shell.panel_state.right.visible = true;
    // Opencode terminal lives on the Terminal tab, which is no longer in the
    // default right-dock tab set — add it for this terminal-focus scenario.
    shell.panel_state.right.tabs.push(PanelTabId::Terminal);
    shell.panel_state.right.switch_to_tab(PanelTabId::Terminal);
    shell.focus_manager.set(FocusTarget::RightSidebar);
    shell
        .app_state
        .apply_mode_event(ModeEvent::FocusTerminal)
        .expect("focus terminal mode");
    let context = shell.build_context();
    assert_eq!(context.focus, InputFocusContext::Terminal);
    assert_eq!(context.mode, EditorMode::TerminalFocus);
    assert!(context.right_sidebar_terminal);

    let routed = shell.input_handler.route_normalized_input(
        NormalizedInput {
            physical_key: Some(KeyCode::Comma),
            named_key: None,
            text: Some(",".to_string()),
            modifiers: ModifiersState::SUPER,
        },
        &shell.input_map,
        context,
        std::time::Instant::now(),
    );

    match routed {
        Some(InputRouteOutcome::Dispatch(translated)) => {
            assert_eq!(translated.command, Command::OpenSettings);
            assert!(shell.handle_command(translated.command));
        }
        other => panic!(
            "expected settings route from right terminal, got {:?}",
            other
        ),
    }

    assert_eq!(shell.focus_manager.current(), FocusTarget::CenterEditor);
    assert_eq!(shell.app_state.current_mode(), EditorMode::Normal);
    assert!(shell.app_state.active_settings_buffer().is_some());
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
fn clicking_focused_bottom_terminal_restores_terminal_mode_after_drift() {
    // Regression for bug-008 recurrence: the terminal already HAS focus
    // (BottomPanel) but the editor mode has drifted back to Normal (e.g. after an
    // overlay/palette closed). Clicking the terminal body must restore
    // TerminalFocus so the user can type. Previously the mode-set was gated behind
    // a focus *change*, so a click that didn't move focus left the mode stuck at
    // Normal and keystrokes never reached the PTY.
    let mut shell = AppShell::new_for_tests().expect("create app shell");
    // Open the bottom dock with a terminal tab, but stay in the editor (Normal).
    assert!(shell.handle_command(Command::ToggleBottomDock));
    assert!(shell.panel_state.bottom.visible);
    if shell.terminal_tabs.is_empty() {
        shell.handle_command(Command::TerminalTabNew);
    }
    assert!(!shell.terminal_tabs.is_empty());

    // Focus lands on the bottom terminal WITHOUT entering terminal mode — the stuck
    // state the user hits (e.g. Tab-cycling focus onto the panel): BottomPanel +
    // Normal. The state machine has no TerminalFocus←EnterNormal edge, so this is
    // how the drift is reached, not by EnterNormal from TerminalFocus.
    shell.focus_manager.set(FocusTarget::BottomPanel);
    assert_eq!(shell.focus_manager.current(), FocusTarget::BottomPanel);
    assert_eq!(shell.app_state.current_mode(), EditorMode::Normal);

    // Click inside the bottom terminal body (focus does NOT change).
    let bounds = shell
        .current_bottom_panel_bounds()
        .expect("bottom panel bounds");
    shell.last_cursor_position =
        Some((bounds[0] + bounds[2] * 0.5, bounds[1] + bounds[3] * 0.75));
    shell.handle_click_focus();

    assert_eq!(
        shell.focus_manager.current(),
        FocusTarget::BottomPanel,
        "focus stays on the terminal"
    );
    assert_eq!(
        shell.app_state.current_mode(),
        EditorMode::TerminalFocus,
        "clicking the focused terminal restores TerminalFocus even without a focus change"
    );
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

    // Pretend an agent is already running so the toggle opens straight to the
    // terminal instead of the agent chooser.
    shell.right_pty_session_id = Some(42);

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
fn ai_chat_focus_opens_inline_agent_picker_when_none_running() {
    let mut shell = AppShell::new_for_tests().expect("create app shell");
    shell.panel_state.right.visible = true;
    shell.focus_manager.set(FocusTarget::RightSidebar);
    assert_eq!(shell.app_state.current_mode(), EditorMode::Normal);

    assert!(shell.handle_command(Command::AiChatFocus));

    // No agent running → AI Chat tab focused with its in-panel picker (no palette).
    assert_eq!(
        shell.panel_state.right.active_tab_id(),
        Some(PanelTabId::AiChat)
    );
    assert!(!shell.app_state.is_command_palette_visible());
    assert_eq!(shell.focus_manager.current(), FocusTarget::RightSidebar);

    // j/k move the selection; Launch spawns the selected agent.
    assert_eq!(shell.ai_agent_picker_selected, 0);
    assert!(shell.handle_command(Command::AiAgentPickerNext));
    assert_eq!(shell.ai_agent_picker_selected, 1);
    assert!(shell.handle_command(Command::AiAgentPickerPrev));
    assert_eq!(shell.ai_agent_picker_selected, 0);
}

#[test]
fn ai_chat_focus_enters_terminal_when_agent_running() {
    let mut shell = AppShell::new_for_tests().expect("create app shell");
    shell.panel_state.right.visible = true;
    shell.right_pty_session_id = Some(7);

    assert!(shell.handle_command(Command::AiChatFocus));

    assert_eq!(shell.focus_manager.current(), FocusTarget::RightSidebar);
    assert_eq!(
        shell.panel_state.right.active_tab_id(),
        Some(PanelTabId::AiChat)
    );
    assert!(!shell.app_state.is_command_palette_visible());
}

#[test]
fn right_terminal_output_preserves_manual_scrollback_view() {
    let mut shell = AppShell::new_for_tests().expect("create app shell");
    shell.panel_state.right.visible = true;
    shell.panel_state.right.switch_to_tab(PanelTabId::Terminal);
    shell.focus_manager.set(FocusTarget::RightSidebar);
    let _ = shell.app_state.apply_mode_event(ModeEvent::FocusTerminal);
    shell.right_pty_session_id = Some(7);
    shell.right_terminal_grid = TerminalGrid::new(5, 2);
    shell
        .right_terminal_grid
        .feed_chunk("11111\r\n22222\r\n33333\r\n44444\r\n");
    shell.right_terminal_grid.view_scroll_up(1);
    assert!(shell.right_terminal_grid.scroll_offset > 0);

    shell.on_worker_result(crate::async_runtime::message::WorkerResult {
        request_id: 1,
        revision_id: 0,
        topic: crate::async_runtime::message::RequestTopic::TerminalPty,
        payload: crate::async_runtime::message::WorkerResultPayload::PtyOutput {
            session_id: 7,
            chunk: b"55555\r\n".to_vec(),
        },
    });

    assert!(
        shell.right_terminal_grid.scroll_offset > 0,
        "right terminal output should not jump to bottom while user is viewing scrollback"
    );
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
fn saving_a_ts_js_file_reindexes_its_exports() {
    // Staleness fix: the whole-workspace export index only runs at LSP start, so
    // edits made afterwards were invisible until restart. Saving a TS/JS file must
    // refresh that file's exports in the workspace symbol cache.
    let mut shell = AppShell::new_for_tests().expect("create app shell");
    let root = completion_temp_root("reindex_on_save");
    std::fs::create_dir_all(&root).expect("create workspace root");
    shell
        .app_state
        .attach_workspace(root.clone())
        .expect("attach workspace");
    let file = root.join("lib/widget.js");
    let _canonical = open_completion_file(&mut shell, &file, "export const Widget = 1;\n");

    // Opening alone does not index the file's exports.
    assert!(
        shell
            .app_state
            .workspace_symbol_cache()
            .query_symbols("Widget", Some("javascript"))
            .is_empty(),
        "precondition: export not indexed before save"
    );

    // Dirty the buffer, then save. (SaveFile reports no document-state change, so
    // its return is false; the re-index runs inside save_file itself.)
    shell.app_state.insert_char(' ');
    shell.handle_command(Command::SaveFile);

    let hits = shell
        .app_state
        .workspace_symbol_cache()
        .query_symbols("Widget", Some("javascript"));
    assert!(
        hits.iter().any(|symbol| symbol.name == "Widget"),
        "saving a TS/JS file should re-index its exports"
    );
    let _ = std::fs::remove_dir_all(root);
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
            .any(|line| { line.contains("cmd+p") && line.contains("Open command palette") })
    );
    assert_eq!(
        shell.app_state.buffers().last().unwrap().label(),
        "[Cheat Sheet]"
    );
    assert!(!shell.app_state.is_command_palette_visible());
}

#[test]
fn help_scroll_command_marks_the_cheatsheet_layout_dirty() {
    let mut shell = AppShell::new_for_tests().expect("create app shell");
    assert!(shell.handle_command(Command::OpenHelp));
    shell.app_state.set_help_max_scroll(500.0);
    shell.editor_needs_layout = false;

    assert!(shell.handle_command(Command::HelpScrollDown));

    assert_eq!(
        shell
            .app_state
            .active_help_buffer()
            .map(|help| help.scroll_y),
        Some(100.0)
    );
    assert!(shell.editor_needs_layout);
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
}

#[test]
fn fuzzy_picker_vim_normal_mode_edits_query_instead_of_appending() {
    // NOTE: Vim in fuzzy pickers is currently DISABLED at the input layer
    // (build_context leaves palette_vim_mode = None for fuzzy buffers, see
    // bug-181) because it hijacked Esc-closes-picker and needs interactive
    // verification. This test exercises only the engine plumbing — if a
    // PaletteVimInput reaches the handler, it correctly edits the FuzzyState
    // query — so the wiring stays correct for a future re-land. It does NOT
    // assert the live feature is reachable from real keystrokes.
    use crate::core::commands::PaletteVimKey;
    let mut shell = AppShell::new_for_tests().expect("create app shell");
    let root = std::env::temp_dir().join(format!("netherize_fuzzy_vim_{}", std::process::id()));
    std::fs::create_dir_all(&root).expect("create workspace");
    shell
        .app_state
        .attach_workspace(root.clone())
        .expect("attach workspace");

    assert!(shell.handle_command(Command::OpenFileFinder));
    assert!(shell.app_state.active_buffer_is_fuzzy_picker());
    assert!(shell.handle_command(Command::FilePickerAppendQuery("foo bar".to_string())));
    assert_eq!(shell.app_state.command_palette_query_text(), "foo bar");

    // Esc enters Normal sub-mode on the fuzzy buffer (not closing the picker).
    assert!(shell.handle_command(Command::PaletteVimInput(PaletteVimKey::Esc)));
    assert_eq!(
        shell.app_state.active_fuzzy_picker_vim_mode(),
        Some(crate::app::command_palette::PaletteVimMode::Normal)
    );

    // `0` then `dw` deletes "foo " — proving keys edit the query, not append.
    assert!(shell.handle_command(Command::PaletteVimInput(PaletteVimKey::Char('0'))));
    assert!(shell.handle_command(Command::PaletteVimInput(PaletteVimKey::Char('d'))));
    assert!(shell.handle_command(Command::PaletteVimInput(PaletteVimKey::Char('w'))));
    assert_eq!(shell.app_state.command_palette_query_text(), "bar");

    // `i` returns to Insert; typing appends again.
    assert!(shell.handle_command(Command::PaletteVimInput(PaletteVimKey::Char('i'))));
    assert_eq!(
        shell.app_state.active_fuzzy_picker_vim_mode(),
        Some(crate::app::command_palette::PaletteVimMode::Insert)
    );

    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn close_buffer_submits_git_baseline_refresh_for_next_active_file() {
    let mut shell = AppShell::new_for_tests().expect("create app shell");
    let root = std::env::temp_dir().join(format!(
        "netherize_close_git_baseline_{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&root).expect("create workspace");
    let a = root.join("a.rs");
    let b = root.join("b.rs");
    std::fs::write(&a, "fn a() {}\n").expect("write a");
    std::fs::write(&b, "fn b() {}\n").expect("write b");

    shell
        .app_state
        .attach_workspace(root.clone())
        .expect("attach workspace");

    // Open first file, then second file
    assert!(shell.handle_command(Command::OpenFile(a)));
    assert!(shell.handle_command(Command::OpenFile(b)));

    let revision_before = shell.git_baseline_revision;

    // Close current file (b.rs), which will activate a.rs
    assert!(shell.close_current_buffer_now());

    // Verify git baseline refresh was requested for the newly active file (a.rs)
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
            ..Default::default()
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
        detail: Some("()".to_string()),
        insert_text: Some(insert_text.to_string()),
        text_edit: None,
        text_edit_text: None,
        additional_text_edits: Vec::new(),
        kind: Some(3),
        callable: Some(true),
        has_parameters: Some(false),
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

fn lsp_replace_edit(
    line: u32,
    start_character: u32,
    end_character: u32,
    new_text: &str,
) -> crate::async_runtime::message::LspTextEdit {
    crate::async_runtime::message::LspTextEdit {
        range: crate::async_runtime::message::LspRange {
            start: crate::async_runtime::message::LspPosition {
                line,
                character: start_character,
            },
            end: crate::async_runtime::message::LspPosition {
                line,
                character: end_character,
            },
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
        callable: Some(true),
        has_parameters: Some(false),
    }
}

fn cached_ts_default_export(name: &str, source_path: &std::path::Path) -> crate::lsp::CachedSymbol {
    let source_path = source_path.to_path_buf();
    crate::lsp::CachedSymbol {
        name: name.to_string(),
        kind: "Function".to_string(),
        container_name: None,
        file_path: source_path.clone(),
        line: 0,
        character: 24,
        source_path: Some(source_path.clone()),
        import_path: Some(source_path.with_extension("").display().to_string()),
        export_kind: Some("default".to_string()),
        callable: Some(true),
        has_parameters: Some(false),
    }
}

fn cached_go_symbol(name: &str, source_path: &std::path::Path) -> crate::lsp::CachedSymbol {
    crate::lsp::CachedSymbol {
        name: name.to_string(),
        kind: "Function".to_string(),
        container_name: None,
        file_path: source_path.to_path_buf(),
        line: 0,
        character: 5,
        source_path: None,
        import_path: None,
        export_kind: None,
        callable: Some(true),
        has_parameters: Some(false),
    }
}

fn cached_package_default_export(
    name: &str,
    source_path: &std::path::Path,
    import_path: &str,
) -> crate::lsp::CachedSymbol {
    let source_path = source_path.to_path_buf();
    crate::lsp::CachedSymbol {
        name: name.to_string(),
        kind: "Variable".to_string(),
        container_name: None,
        file_path: source_path.clone(),
        line: 0,
        character: 0,
        source_path: Some(source_path),
        import_path: Some(import_path.to_string()),
        export_kind: Some("default".to_string()),
        callable: Some(false),
        has_parameters: None,
    }
}

#[test]
fn completion_close_exits_insert_and_cancels_pending_lsp_completion() {
    let mut shell = AppShell::new_for_tests().expect("create app shell");
    let root = completion_temp_root("close_cancels_lsp");
    let _path = open_completion_file(&mut shell, &root.join("src/app.ts"), "axios.p");
    let cache = crate::lsp::WorkspaceSymbolCache::new();
    let completion = crate::app::app_state::CompletionState::from_lsp_items(
        vec![test_completion_item("post", "post")],
        0,
        "axios.p".chars().count(),
        "axios.".chars().count(),
        "p".to_string(),
        &cache,
        Some("typescript"),
    );
    shell
        .app_state
        .apply_mode_event(ModeEvent::EnterInsert)
        .expect("enter insert");
    assert!(shell.app_state.set_completion(completion));
    shell.active_lsp_completion_request_id = Some(77);
    shell.app_state.set_completion_loading(true);

    assert!(shell.handle_command(Command::CompletionClose));

    assert_eq!(shell.app_state.current_mode(), EditorMode::Normal);
    assert!(shell.app_state.completion().is_none());
    assert_eq!(shell.active_lsp_completion_request_id, None);
    assert!(!shell.app_state.is_completion_loading());
}

#[test]
fn stale_lsp_completion_result_after_escape_is_ignored() {
    let mut shell = AppShell::new_for_tests().expect("create app shell");
    let root = completion_temp_root("stale_after_escape");
    let _path = open_completion_file(&mut shell, &root.join("src/app.ts"), "axios.p");
    shell
        .app_state
        .jump_to_line_and_column(0, "axios.p".chars().count());
    shell
        .app_state
        .apply_mode_event(ModeEvent::EnterInsert)
        .expect("enter insert");
    shell.active_lsp_completion_request_id = Some(91);
    shell.app_state.set_completion_loading(true);

    assert!(shell.handle_command(Command::SwitchMode(ModeEvent::Escape)));
    assert_eq!(shell.app_state.current_mode(), EditorMode::Normal);
    assert_eq!(shell.active_lsp_completion_request_id, None);

    shell.on_worker_result(crate::async_runtime::message::WorkerResult {
        request_id: 91,
        revision_id: 0,
        topic: crate::async_runtime::message::RequestTopic::LspRequest,
        payload: crate::async_runtime::message::WorkerResultPayload::LspCompletionResult {
            items: vec![test_completion_item("post", "post")],
            cursor_line: 0,
            cursor_col: "axios.p".chars().count(),
            prefix_start_col: "axios.".chars().count(),
            prefix: "p".to_string(),
        },
    });

    assert!(shell.app_state.completion().is_none());
    assert!(!shell.app_state.is_completion_loading());
}

#[test]
fn member_access_completion_debounces_after_one_typed_character() {
    let mut shell = AppShell::new_for_tests().expect("create app shell");
    let root = completion_temp_root("member_access_one_char");
    let _path = open_completion_file(&mut shell, &root.join("src/app.ts"), "axios.p");
    shell
        .app_state
        .jump_to_line_and_column(0, "axios.p".chars().count());
    shell
        .app_state
        .apply_mode_event(ModeEvent::EnterInsert)
        .expect("enter insert");
    shell.active_lsp_server = Some(ActiveLspServer {
        server_name: "typescript-language-server".to_string(),
        root_path: root,
    });
    shell.lsp_completion_trigger_chars = vec!['.'];

    shell.queue_lsp_completion_after_debounce_if_needed();

    assert!(shell.pending_lsp_completion_after_debounce);
}

#[test]
fn go_completion_debounces_after_two_typed_characters() {
    let mut shell = AppShell::new_for_tests().expect("create app shell");
    let root = completion_temp_root("go_prefix_completion");
    let _path = open_completion_file(&mut shell, &root.join("main.go"), "Co");
    let _ = shell.app_state.jump_to_line_and_column(0, 2);
    shell
        .app_state
        .apply_mode_event(ModeEvent::EnterInsert)
        .expect("enter insert");
    shell.active_lsp_server = Some(ActiveLspServer {
        server_name: "gopls".to_string(),
        root_path: root,
    });
    shell.lsp_completion_trigger_chars = vec!['.'];

    shell.queue_lsp_completion_after_debounce_if_needed();

    assert!(shell.pending_lsp_completion_after_debounce);
}

#[test]
fn go_member_access_completion_debounces_after_one_typed_character() {
    let mut shell = AppShell::new_for_tests().expect("create app shell");
    let root = completion_temp_root("go_member_completion");
    let _path = open_completion_file(&mut shell, &root.join("main.go"), "client.G");
    let _ = shell
        .app_state
        .jump_to_line_and_column(0, "client.G".chars().count());
    shell
        .app_state
        .apply_mode_event(ModeEvent::EnterInsert)
        .expect("enter insert");
    shell.active_lsp_server = Some(ActiveLspServer {
        server_name: "gopls".to_string(),
        root_path: root,
    });
    shell.lsp_completion_trigger_chars = vec!['.'];

    shell.queue_lsp_completion_after_debounce_if_needed();

    assert!(shell.pending_lsp_completion_after_debounce);
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
            callable: Some(true),
            has_parameters: Some(false),
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
fn completion_accept_adds_call_parens_and_places_cursor_inside_for_params() {
    let mut shell = AppShell::new_for_tests().expect("create app shell");
    let root = completion_temp_root("call_parens_with_params");
    let _path = open_completion_file(&mut shell, &root.join("call_parens.ts"), "wai");
    let mut item = test_completion_item("wait", "wait");
    item.detail = Some("function wait(ms: number): Promise<void>".to_string());
    item.has_parameters = Some(true);
    let cache = crate::lsp::WorkspaceSymbolCache::new();
    let completion = crate::app::app_state::CompletionState::from_lsp_items(
        vec![item],
        0,
        "wai".chars().count(),
        0,
        "wai".to_string(),
        &cache,
        None,
    );
    assert!(
        shell
            .app_state
            .jump_to_line_and_column(0, "wai".chars().count())
    );
    assert!(shell.app_state.set_completion(completion));

    assert!(shell.handle_command(Command::CompletionAccept));
    assert_eq!(shell.app_state.text_string(), "wait()");
    assert_eq!(
        shell.app_state.cursor_line_col(),
        (0, "wait(".chars().count())
    );
}

#[test]
fn completion_accept_adds_call_parens_and_keeps_cursor_after_no_param_call() {
    let mut shell = AppShell::new_for_tests().expect("create app shell");
    let root = completion_temp_root("call_parens_no_params");
    let _path = open_completion_file(&mut shell, &root.join("call_parens.ts"), "ini");
    let mut item = test_completion_item("init", "init");
    item.detail = Some("function init(): void".to_string());
    item.has_parameters = Some(false);
    let cache = crate::lsp::WorkspaceSymbolCache::new();
    let completion = crate::app::app_state::CompletionState::from_lsp_items(
        vec![item],
        0,
        "ini".chars().count(),
        0,
        "ini".to_string(),
        &cache,
        None,
    );
    assert!(
        shell
            .app_state
            .jump_to_line_and_column(0, "ini".chars().count())
    );
    assert!(shell.app_state.set_completion(completion));

    assert!(shell.handle_command(Command::CompletionAccept));
    assert_eq!(shell.app_state.text_string(), "init()");
    assert_eq!(
        shell.app_state.cursor_line_col(),
        (0, "init()".chars().count())
    );
}

#[test]
fn completion_accept_reuses_existing_call_parens_after_prefix() {
    let mut shell = AppShell::new_for_tests().expect("create app shell");
    let root = completion_temp_root("existing_call_parens");
    let _path = open_completion_file(&mut shell, &root.join("call_parens.ts"), "wai(500)");
    let mut item = test_completion_item("wait", "wait");
    item.detail = Some("function wait(ms: number): Promise<void>".to_string());
    item.has_parameters = Some(true);
    let cache = crate::lsp::WorkspaceSymbolCache::new();
    let completion = crate::app::app_state::CompletionState::from_lsp_items(
        vec![item],
        0,
        "wai".chars().count(),
        0,
        "wai".to_string(),
        &cache,
        None,
    );
    assert!(
        shell
            .app_state
            .jump_to_line_and_column(0, "wai".chars().count())
    );
    assert!(shell.app_state.set_completion(completion));

    assert!(shell.handle_command(Command::CompletionAccept));
    assert_eq!(shell.app_state.text_string(), "wait(500)");
    assert_eq!(
        shell.app_state.cursor_line_col(),
        (0, "wait".chars().count())
    );
}

#[test]
fn completion_accept_preserves_viewport_scroll_line() {
    let mut shell = AppShell::new_for_tests().expect("create app shell");
    let root = completion_temp_root("preserve_completion_scroll");
    let lines = (0..80)
        .map(|idx| {
            if idx == 40 {
                "wai".to_string()
            } else {
                format!("line {idx}")
            }
        })
        .collect::<Vec<_>>();
    let _path = open_completion_file(
        &mut shell,
        &root.join("completion-scroll.ts"),
        &lines.join("\n"),
    );
    assert!(
        shell
            .app_state
            .jump_to_line_and_column(40, "wai".chars().count())
    );
    shell.app_state.set_target_scroll_line(35);
    let mut item = test_completion_item("wait", "wait");
    item.detail = Some("function wait(): void".to_string());
    item.has_parameters = Some(false);
    let cache = crate::lsp::WorkspaceSymbolCache::new();
    let completion = crate::app::app_state::CompletionState::from_lsp_items(
        vec![item],
        40,
        "wai".chars().count(),
        0,
        "wai".to_string(),
        &cache,
        None,
    );
    assert!(shell.app_state.set_completion(completion));

    assert!(shell.handle_command(Command::CompletionAccept));

    assert_eq!(shell.app_state.scroll_line(), 35);
    assert_eq!(
        shell.app_state.text_string().lines().nth(40),
        Some("wait()")
    );
}

#[test]
fn completion_accept_strips_lsp_empty_call_parens_when_source_already_has_call() {
    let mut shell = AppShell::new_for_tests().expect("create app shell");
    let root = completion_temp_root("existing_call_parens_lsp");
    let _path = open_completion_file(&mut shell, &root.join("call_parens.ts"), "wai(500)");
    let mut item = test_completion_item("wait", "wait()");
    item.detail = Some("function wait(ms: number): Promise<void>".to_string());
    item.has_parameters = Some(true);
    let cache = crate::lsp::WorkspaceSymbolCache::new();
    let completion = crate::app::app_state::CompletionState::from_lsp_items(
        vec![item],
        0,
        "wai".chars().count(),
        0,
        "wai".to_string(),
        &cache,
        None,
    );
    assert!(
        shell
            .app_state
            .jump_to_line_and_column(0, "wai".chars().count())
    );
    assert!(shell.app_state.set_completion(completion));

    assert!(shell.handle_command(Command::CompletionAccept));
    assert_eq!(shell.app_state.text_string(), "wait(500)");
    assert_eq!(
        shell.app_state.cursor_line_col(),
        (0, "wait".chars().count())
    );
}

#[test]
fn completion_accept_keeps_cursor_after_go_no_param_call_signature() {
    let mut shell = AppShell::new_for_tests().expect("create app shell");
    let root = completion_temp_root("go_no_param_call");
    let _path = open_completion_file(&mut shell, &root.join("main.go"), "bootstrap.New");
    let mut item = test_completion_item("NewApp", "NewApp()");
    item.detail = Some("func() *bootstrap.App".to_string());
    item.has_parameters = None;
    let cache = crate::lsp::WorkspaceSymbolCache::new();
    let completion = crate::app::app_state::CompletionState::from_lsp_items(
        vec![item],
        0,
        "bootstrap.New".chars().count(),
        "bootstrap.".chars().count(),
        "New".to_string(),
        &cache,
        None,
    );
    assert!(
        shell
            .app_state
            .jump_to_line_and_column(0, "bootstrap.New".chars().count())
    );
    assert!(shell.app_state.set_completion(completion));

    assert!(shell.handle_command(Command::CompletionAccept));
    assert_eq!(shell.app_state.text_string(), "bootstrap.NewApp()");
    assert_eq!(
        shell.app_state.cursor_line_col(),
        (0, "bootstrap.NewApp()".chars().count())
    );
}

#[test]
fn completion_accept_places_cursor_inside_go_with_param_call_signature() {
    let mut shell = AppShell::new_for_tests().expect("create app shell");
    let root = completion_temp_root("go_with_param_call");
    let _path = open_completion_file(&mut shell, &root.join("main.go"), "bootstrap.New");
    let mut item = test_completion_item("NewApp", "NewApp()");
    item.detail = Some("func(x int, y string) *bootstrap.App".to_string());
    item.has_parameters = None;
    let cache = crate::lsp::WorkspaceSymbolCache::new();
    let completion = crate::app::app_state::CompletionState::from_lsp_items(
        vec![item],
        0,
        "bootstrap.New".chars().count(),
        "bootstrap.".chars().count(),
        "New".to_string(),
        &cache,
        None,
    );
    assert!(
        shell
            .app_state
            .jump_to_line_and_column(0, "bootstrap.New".chars().count())
    );
    assert!(shell.app_state.set_completion(completion));

    assert!(shell.handle_command(Command::CompletionAccept));
    assert_eq!(shell.app_state.text_string(), "bootstrap.NewApp()");
    assert_eq!(
        shell.app_state.cursor_line_col(),
        (0, "bootstrap.NewApp(".chars().count())
    );
}

#[test]
fn completion_accept_places_cursor_inside_rust_with_param_call_signature() {
    let mut shell = AppShell::new_for_tests().expect("create app shell");
    let root = completion_temp_root("rust_with_param_call");
    let _path = open_completion_file(&mut shell, &root.join("main.rs"), "foo.ba");
    let mut item = test_completion_item("bar", "bar()");
    item.detail = Some("pub fn bar(x: i32) -> bool".to_string());
    item.has_parameters = None;
    let cache = crate::lsp::WorkspaceSymbolCache::new();
    let completion = crate::app::app_state::CompletionState::from_lsp_items(
        vec![item],
        0,
        "foo.ba".chars().count(),
        "foo.".chars().count(),
        "ba".to_string(),
        &cache,
        None,
    );
    assert!(
        shell
            .app_state
            .jump_to_line_and_column(0, "foo.ba".chars().count())
    );
    assert!(shell.app_state.set_completion(completion));

    assert!(shell.handle_command(Command::CompletionAccept));
    assert_eq!(shell.app_state.text_string(), "foo.bar()");
    assert_eq!(
        shell.app_state.cursor_line_col(),
        (0, "foo.bar(".chars().count())
    );
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
            callable: Some(true),
            has_parameters: Some(false),
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
fn completion_accept_strips_existing_member_receiver_from_full_insert_text() {
    let mut shell = AppShell::new_for_tests().expect("create app shell");
    let root = completion_temp_root("strip_member_receiver");
    let text = "bootstrap.N";
    let _path = open_completion_file(&mut shell, &root.join("member_receiver.ts"), text);
    shell.lsp_completion_trigger_chars = vec!['.'];
    let cursor_col = text.chars().count();
    let prefix_start = "bootstrap.".chars().count();
    let cache = crate::lsp::WorkspaceSymbolCache::new();
    let completion = crate::app::app_state::CompletionState::from_lsp_items(
        vec![crate::async_runtime::message::LspCompletionItem {
            label: "bootstrap.NewAppN".to_string(),
            detail: None,
            insert_text: Some("bootstrap.NewAppN".to_string()),
            text_edit: None,
            text_edit_text: None,
            additional_text_edits: Vec::new(),
            kind: Some(5),
            callable: Some(false),
            has_parameters: None,
            documentation: None,
            data: None,
            source_path: None,
            import_path: None,
            export_kind: None,
            raw_json: None,
        }],
        0,
        cursor_col,
        prefix_start,
        "N".to_string(),
        &cache,
        None,
    );
    assert!(shell.app_state.jump_to_line_and_column(0, cursor_col));
    assert!(shell.app_state.set_completion(completion));

    assert!(shell.handle_command(Command::CompletionAccept));
    assert_eq!(shell.app_state.text_string(), "bootstrap.NewAppN");
}

#[test]
fn completion_accept_replaces_member_prefix_when_lsp_edit_is_zero_width() {
    let mut shell = AppShell::new_for_tests().expect("create app shell");
    let root = completion_temp_root("zero_width_member_edit");
    let text = "bootstrap.N";
    let _path = open_completion_file(&mut shell, &root.join("zero_width_member.ts"), text);
    shell.lsp_completion_trigger_chars = vec!['.'];
    let cursor_col = text.chars().count();
    let prefix_start = "bootstrap.".chars().count();
    let mut item = test_completion_item("NewAppN", "NewAppN");
    item.text_edit = Some(lsp_insert_edit(0, cursor_col as u32, "NewAppN"));
    item.text_edit_text = Some("NewAppN".to_string());
    item.kind = Some(5);
    item.callable = Some(false);
    item.has_parameters = None;
    let cache = crate::lsp::WorkspaceSymbolCache::new();
    let completion = crate::app::app_state::CompletionState::from_lsp_items(
        vec![item],
        0,
        cursor_col,
        prefix_start,
        "N".to_string(),
        &cache,
        None,
    );
    assert!(shell.app_state.jump_to_line_and_column(0, cursor_col));
    assert!(shell.app_state.set_completion(completion));

    assert!(shell.handle_command(Command::CompletionAccept));
    assert_eq!(shell.app_state.text_string(), "bootstrap.NewAppN");
}

#[test]
fn completion_accept_keeps_lsp_edit_that_replaces_full_member_expression() {
    let mut shell = AppShell::new_for_tests().expect("create app shell");
    let root = completion_temp_root("full_member_lsp_edit");
    let text = "bootstrap.N";
    let _path = open_completion_file(&mut shell, &root.join("full_member.ts"), text);
    shell.lsp_completion_trigger_chars = vec!['.'];
    let cursor_col = text.chars().count();
    let prefix_start = "bootstrap.".chars().count();
    let mut item = test_completion_item("bootstrap.NewAppN", "bootstrap.NewAppN");
    item.text_edit = Some(lsp_replace_edit(
        0,
        0,
        cursor_col as u32,
        "bootstrap.NewAppN",
    ));
    item.text_edit_text = Some("bootstrap.NewAppN".to_string());
    item.kind = Some(5);
    item.callable = Some(false);
    item.has_parameters = None;
    let cache = crate::lsp::WorkspaceSymbolCache::new();
    let completion = crate::app::app_state::CompletionState::from_lsp_items(
        vec![item],
        0,
        cursor_col,
        prefix_start,
        "N".to_string(),
        &cache,
        None,
    );
    assert!(shell.app_state.jump_to_line_and_column(0, cursor_col));
    assert!(shell.app_state.set_completion(completion));

    assert!(shell.handle_command(Command::CompletionAccept));
    assert_eq!(shell.app_state.text_string(), "bootstrap.NewAppN");
}

#[test]
fn completion_accept_prefers_lsp_text_edit_text_over_insert_text() {
    let mut shell = AppShell::new_for_tests().expect("create app shell");
    let root = completion_temp_root("prefer_text_edit_new_text");
    let text = "bootstrap.N";
    let _path = open_completion_file(&mut shell, &root.join("text_edit_new_text.ts"), text);
    shell.lsp_completion_trigger_chars = vec!['.'];
    let cursor_col = text.chars().count();
    let prefix_start = "bootstrap.".chars().count();
    let mut item = test_completion_item("bootstrap.NewAppN", "bootstrap.NewAppN");
    item.text_edit = Some(lsp_replace_edit(
        0,
        prefix_start as u32,
        cursor_col as u32,
        "NewAppN",
    ));
    item.text_edit_text = Some("NewAppN".to_string());
    item.kind = Some(5);
    item.callable = Some(false);
    item.has_parameters = None;
    let cache = crate::lsp::WorkspaceSymbolCache::new();
    let completion = crate::app::app_state::CompletionState::from_lsp_items(
        vec![item],
        0,
        cursor_col,
        prefix_start,
        "N".to_string(),
        &cache,
        None,
    );
    assert!(shell.app_state.jump_to_line_and_column(0, cursor_col));
    assert!(shell.app_state.set_completion(completion));

    assert!(shell.handle_command(Command::CompletionAccept));
    assert_eq!(shell.app_state.text_string(), "bootstrap.NewAppN");
}

#[test]
fn completion_accept_applies_lsp_additional_import_edits_in_one_undo() {
    let mut shell = AppShell::new_for_tests().expect("create app shell");
    let root = completion_temp_root("lsp_import_edit");
    let _path = open_completion_file(&mut shell, &root.join("src/app.ts"), "con");
    let mut item = test_completion_item("connect", "connect");
    item.additional_text_edits = vec![lsp_insert_edit(0, 0, "import { connect } from './api';\n")];
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
    assert!(
        shell
            .app_state
            .jump_to_line_and_column(0, "con".chars().count())
    );
    assert!(shell.app_state.set_completion(completion));

    assert!(shell.handle_command(Command::CompletionAccept));
    assert_eq!(
        shell.app_state.text_string(),
        "import { connect } from './api';\nconnect()"
    );

    assert!(shell.handle_command(Command::Undo));
    assert_eq!(shell.app_state.text_string(), "con");
}

#[test]
fn completion_accept_waits_for_lsp_resolve_before_inserting_unresolved_item() {
    let mut shell = AppShell::new_for_tests().expect("create app shell");
    let root = completion_temp_root("accept_waits_for_resolve");
    let _path = open_completion_file(&mut shell, &root.join("src/app.ts"), "con");
    shell.active_lsp_server = Some(ActiveLspServer {
        server_name: "typescript-language-server".to_string(),
        root_path: root.clone(),
    });
    let mut item = test_completion_item("connect", "connect");
    item.raw_json = Some(r#"{"label":"connect","data":{"source":"./api"}}"#.to_string());
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
    assert!(
        shell
            .app_state
            .jump_to_line_and_column(0, "con".chars().count())
    );
    assert!(shell.app_state.set_completion(completion));

    assert!(shell.handle_command(Command::CompletionAccept));
    assert_eq!(shell.app_state.text_string(), "con");
    assert_eq!(
        shell.pending_completion_accept_after_resolve,
        Some(("connect".to_string(), 0))
    );
    assert!(shell.app_state.completion().is_some());

    let request_id = shell
        .completion_resolve_request_id
        .expect("resolve request id");
    let mut resolved = test_completion_item("connect", "connect");
    resolved.additional_text_edits =
        vec![lsp_insert_edit(0, 0, "import { connect } from './api';\n")];
    shell.on_worker_result(crate::async_runtime::message::WorkerResult {
        request_id,
        revision_id: 0,
        topic: crate::async_runtime::message::RequestTopic::LspRequest,
        payload: crate::async_runtime::message::WorkerResultPayload::LspCompletionResolveResult {
            item_label: "connect".to_string(),
            detail: None,
            documentation: None,
            resolved_item: Some(resolved),
            completion_revision: 0,
        },
    });

    assert_eq!(
        shell.app_state.text_string(),
        "import { connect } from './api';\nconnect()"
    );
    assert!(shell.pending_completion_accept_after_resolve.is_none());
    assert!(shell.app_state.completion().is_none());
}

#[test]
fn workspace_completion_fallback_merges_existing_named_import() {
    let mut shell = AppShell::new_for_tests().expect("create app shell");
    let root = completion_temp_root("merge_import");
    let app_path = root.join("src/app.ts");
    let source_path =
        write_completion_file(&root.join("src/utils.ts"), "export function connect() {}\n");
    let _path = open_completion_file(
        &mut shell,
        &app_path,
        "import { existing } from './utils';\n\ncon",
    );
    let cache = crate::lsp::WorkspaceSymbolCache::new();
    cache.insert_symbols(
        "typescript",
        vec![cached_ts_export("connect", &source_path)],
    );
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
        "import { existing, connect } from './utils';\n\nconnect()"
    );
}

#[test]
fn workspace_completion_fallback_inserts_new_named_import() {
    let mut shell = AppShell::new_for_tests().expect("create app shell");
    let text = "const value = con";
    let root = completion_temp_root("new_import");
    let source_path =
        write_completion_file(&root.join("src/api.ts"), "export function connect() {}\n");
    let _path = open_completion_file(&mut shell, &root.join("src/app.ts"), text);
    let cache = crate::lsp::WorkspaceSymbolCache::new();
    cache.insert_symbols(
        "typescript",
        vec![cached_ts_export("connect", &source_path)],
    );
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
    assert_eq!(
        completion.filtered_items[0].item.detail.as_deref(),
        Some("api.ts:1")
    );
    assert!(shell.app_state.jump_to_line_and_column(0, cursor_col));
    assert!(shell.app_state.set_completion(completion));

    assert!(shell.handle_command(Command::CompletionAccept));
    assert_eq!(
        shell.app_state.text_string(),
        "import { connect } from './api';\nconst value = connect()"
    );
}

#[test]
fn workspace_completion_fallback_inserts_default_import() {
    let mut shell = AppShell::new_for_tests().expect("create app shell");
    let text = "const value = wai";
    let root = completion_temp_root("default_import");
    let source_path = write_completion_file(
        &root.join("src/wait.ts"),
        "export default function wait() {}\n",
    );
    let _path = open_completion_file(&mut shell, &root.join("src/app.ts"), text);
    let cache = crate::lsp::WorkspaceSymbolCache::new();
    cache.insert_symbols(
        "typescript",
        vec![cached_ts_default_export("wait", &source_path)],
    );
    let cursor_col = text.chars().count();
    let prefix_start = "const value = ".chars().count();
    let completion = crate::app::app_state::CompletionState::from_lsp_items(
        Vec::new(),
        0,
        cursor_col,
        prefix_start,
        "wai".to_string(),
        &cache,
        Some("typescript"),
    );
    assert!(!completion.filtered_items.is_empty());
    assert!(shell.app_state.jump_to_line_and_column(0, cursor_col));
    assert!(shell.app_state.set_completion(completion));

    assert!(shell.handle_command(Command::CompletionAccept));
    assert_eq!(
        shell.app_state.text_string(),
        "import wait from './wait';\nconst value = wait()"
    );
}

#[test]
fn workspace_completion_symbols_merge_even_when_lsp_has_many_results() {
    let root = completion_temp_root("merge_cache_with_lsp");
    let source_path =
        write_completion_file(&root.join("src/api.ts"), "export function connect() {}\n");
    let cache = crate::lsp::WorkspaceSymbolCache::new();
    cache.insert_symbols(
        "typescript",
        vec![cached_ts_export("connect", &source_path)],
    );

    let lsp_items = vec![
        test_completion_item(
            "ContentVisibilityAutoStateChangeEvent",
            "ContentVisibilityAutoStateChangeEvent",
        ),
        test_completion_item("CanvasGradient", "CanvasGradient"),
        test_completion_item("CSSPageDescriptors", "CSSPageDescriptors"),
        test_completion_item("CookieChangeEvent", "CookieChangeEvent"),
        test_completion_item("CustomEvent", "CustomEvent"),
    ];
    let completion = crate::app::app_state::CompletionState::from_lsp_items(
        lsp_items,
        0,
        3,
        0,
        "con".to_string(),
        &cache,
        Some("typescript"),
    );

    let imported = completion
        .filtered_items
        .iter()
        .find(|entry| entry.item.label == "connect")
        .expect("workspace importable symbol should be merged");
    assert_eq!(
        imported.source,
        crate::app::app_state::CompletionItemSource::WorkspaceSymbol
    );
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn go_workspace_completion_symbols_merge_with_lsp_results() {
    let root = completion_temp_root("go_merge_cache_with_lsp");
    let source_path =
        write_completion_file(&root.join("api.go"), "package main\n\nfunc Connect() {}\n");
    let cache = crate::lsp::WorkspaceSymbolCache::new();
    cache.insert_symbols("go", vec![cached_go_symbol("Connect", &source_path)]);

    let completion = crate::app::app_state::CompletionState::from_lsp_items(
        vec![test_completion_item("Context", "Context")],
        0,
        3,
        0,
        "Con".to_string(),
        &cache,
        Some("go"),
    );

    let imported = completion
        .filtered_items
        .iter()
        .find(|entry| entry.item.label == "Connect")
        .expect("go workspace symbol should be merged");
    assert_eq!(
        imported.source,
        crate::app::app_state::CompletionItemSource::WorkspaceSymbol
    );
    assert_eq!(imported.item.export_kind, None);
    assert_eq!(imported.item.import_path, None);
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn non_go_bare_workspace_symbol_is_not_offered_standalone() {
    // Java/Rust/Python can't reference a bare name without an import, and the
    // editor synthesizes imports only for TS/JS. So a workspace symbol with no
    // export metadata in these languages must NOT be injected standalone — their
    // LSP provides the proper auto-import form. (Go is exempt: goimports /
    // same-package resolves bare inserts.)
    let root = completion_temp_root("rust_bare_not_standalone");
    let source_path = write_completion_file(&root.join("src/lib.rs"), "pub fn connect() {}\n");
    let cache = crate::lsp::WorkspaceSymbolCache::new();
    cache.insert_symbols(
        "rust",
        vec![crate::lsp::CachedSymbol {
            name: "connect".to_string(),
            kind: "Function".to_string(),
            container_name: None,
            file_path: source_path.clone(),
            line: 0,
            character: 7,
            source_path: None,
            import_path: None,
            export_kind: None,
            callable: Some(true),
            has_parameters: Some(false),
        }],
    );
    let completion = crate::app::app_state::CompletionState::from_lsp_items(
        vec![test_completion_item("connectLocal", "connectLocal")],
        0,
        3,
        0,
        "con".to_string(),
        &cache,
        Some("rust"),
    );
    assert!(
        completion
            .filtered_items
            .iter()
            .all(|entry| entry.item.label != "connect"),
        "a bare Rust workspace symbol must not be offered as a standalone completion"
    );
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn ts_non_exported_workspace_symbol_is_not_offered_standalone() {
    // A TS workspace symbol without export metadata can't be auto-imported, so
    // inserting it bare would create an undefined reference. It must be filtered
    // out of standalone completions (in-scope forms come from tsserver).
    let root = completion_temp_root("ts_non_export_filter");
    let source_path = write_completion_file(&root.join("util.ts"), "function helperLocal() {}\n");
    let cache = crate::lsp::WorkspaceSymbolCache::new();
    cache.insert_symbols(
        "typescript",
        vec![crate::lsp::CachedSymbol {
            name: "helperLocal".to_string(),
            kind: "Function".to_string(),
            container_name: None,
            file_path: source_path.clone(),
            line: 0,
            character: 9,
            source_path: None,
            import_path: None,
            export_kind: None,
            callable: Some(true),
            has_parameters: Some(false),
        }],
    );

    let completion = crate::app::app_state::CompletionState::from_lsp_items(
        vec![test_completion_item("helperOther", "helperOther")],
        0,
        6,
        0,
        "helper".to_string(),
        &cache,
        Some("typescript"),
    );
    assert!(
        completion
            .filtered_items
            .iter()
            .all(|entry| entry.item.label != "helperLocal"),
        "non-exported TS symbol must not be offered as a standalone completion"
    );
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn workspace_completion_import_metadata_enriches_duplicate_lsp_item() {
    let mut shell = AppShell::new_for_tests().expect("create app shell");
    let text = "const value = con";
    let root = completion_temp_root("duplicate_lsp_import_metadata");
    let source_path =
        write_completion_file(&root.join("src/api.ts"), "export function connect() {}\n");
    let _path = open_completion_file(&mut shell, &root.join("src/app.ts"), text);
    let cache = crate::lsp::WorkspaceSymbolCache::new();
    cache.insert_symbols(
        "typescript",
        vec![cached_ts_export("connect", &source_path)],
    );
    let cursor_col = text.chars().count();
    let prefix_start = "const value = ".chars().count();
    let completion = crate::app::app_state::CompletionState::from_lsp_items(
        vec![test_completion_item("connect", "connect")],
        0,
        cursor_col,
        prefix_start,
        "con".to_string(),
        &cache,
        Some("typescript"),
    );
    let item = &completion.filtered_items[0].item;
    assert_eq!(item.source_path.as_deref(), Some(source_path.as_path()));
    assert_eq!(item.export_kind.as_deref(), Some("named"));
    assert!(shell.app_state.jump_to_line_and_column(0, cursor_col));
    assert!(shell.app_state.set_completion(completion));

    assert!(shell.handle_command(Command::CompletionAccept));
    assert_eq!(
        shell.app_state.text_string(),
        "import { connect } from './api';\nconst value = connect()"
    );
}

fn rerank_item(label: &str, score: i64) -> crate::app::app_state::CompletionDisplayItem {
    crate::app::app_state::CompletionDisplayItem {
        item: test_completion_item(label, label),
        match_ranges: Vec::new(),
        score,
        source: crate::app::app_state::CompletionItemSource::Lsp,
    }
}

#[test]
fn ai_rerank_floats_named_labels_to_front_in_ranked_order() {
    let items = vec![
        rerank_item("alpha", 100),
        rerank_item("beta", 90),
        rerank_item("gamma", 80),
    ];
    let ranked = vec!["gamma".to_string(), "alpha".to_string()];
    let out = crate::app::app_state::rerank_completion_items(items, &ranked);
    let labels: Vec<_> = out.iter().map(|entry| entry.item.label.clone()).collect();
    // gamma + alpha float up in the AI's order; beta keeps its place after them.
    assert_eq!(labels, vec!["gamma", "alpha", "beta"]);
}

#[test]
fn ai_rerank_ignores_unknown_labels_and_never_changes_membership() {
    let items = vec![rerank_item("alpha", 100), rerank_item("beta", 90)];
    let ranked = vec!["zeta".to_string(), "beta".to_string()];
    let out = crate::app::app_state::rerank_completion_items(items, &ranked);
    let labels: Vec<_> = out.iter().map(|entry| entry.item.label.clone()).collect();
    assert_eq!(labels, vec!["beta", "alpha"]);
    assert_eq!(out.len(), 2, "rerank must never drop or add items");
}

#[test]
fn completion_state_apply_ai_rerank_floats_item_and_preselects_it() {
    let cache = crate::lsp::WorkspaceSymbolCache::new();
    let mut completion = crate::app::app_state::CompletionState::from_lsp_items(
        vec![
            test_completion_item("append", "append"),
            test_completion_item("apply", "apply"),
        ],
        0,
        2,
        0,
        "ap".to_string(),
        &cache,
        None,
    );
    // Default score order is shortest-first: "apply" before "append".
    assert_eq!(completion.filtered_items[0].item.label, "apply");

    let changed = completion.apply_ai_rerank(&["append".to_string()]);
    assert!(changed, "reordering should report a change");
    assert_eq!(completion.filtered_items[0].item.label, "append");
    assert_eq!(
        completion.selected_index, 0,
        "the AI's top pick must become the pre-selected item"
    );
    assert_eq!(completion.filtered_items.len(), 2);
}

#[test]
fn ai_rerank_with_empty_order_is_identity() {
    let items = vec![rerank_item("alpha", 100), rerank_item("beta", 90)];
    let out = crate::app::app_state::rerank_completion_items(items.clone(), &[]);
    assert_eq!(out, items);
}

#[test]
fn standalone_workspace_symbol_injections_are_capped() {
    // Typing a short prefix in a large workspace can match dozens of exported
    // symbols. Injecting all of them buries the LSP's context-aware suggestions
    // under workspace-wide noise, which is exactly the "gợi ý không đúng context"
    // complaint. The standalone injections must be capped to the best few.
    let root = completion_temp_root("workspace_injection_cap");
    let cache = crate::lsp::WorkspaceSymbolCache::new();
    let symbols: Vec<_> = (0..30)
        .map(|i| {
            let path = write_completion_file(
                &root.join(format!("src/m{i}.ts")),
                &format!("export function connect{i}() {{}}\n"),
            );
            cached_ts_export(&format!("connect{i}"), &path)
        })
        .collect();
    cache.insert_symbols("typescript", symbols);

    let completion = crate::app::app_state::CompletionState::from_lsp_items(
        Vec::new(),
        0,
        4,
        0,
        "conn".to_string(),
        &cache,
        Some("typescript"),
    );

    let injected = completion
        .filtered_items
        .iter()
        .filter(|entry| {
            entry.source == crate::app::app_state::CompletionItemSource::WorkspaceSymbol
        })
        .count();
    assert!(
        injected <= 12,
        "standalone workspace-symbol completions must be capped to avoid flooding the popup; got {injected}"
    );
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn resolvable_lsp_item_is_not_enriched_by_cache_so_real_auto_import_wins() {
    // A real tsserver auto-import candidate carries `raw_json`/`data` so it can
    // be resolved into real `additionalTextEdits`. When the workspace cache holds
    // a same-named export, it must NOT overwrite the LSP item's import metadata:
    // doing so flips `export_kind`/`source_path` to `Some`, which suppresses
    // `should_resolve_lsp_completion_before_accept` and forces a *guessed* import
    // instead of the server's correct one.
    let root = completion_temp_root("resolvable_not_enriched");
    let source_path =
        write_completion_file(&root.join("src/api.ts"), "export function connect() {}\n");
    let cache = crate::lsp::WorkspaceSymbolCache::new();
    cache.insert_symbols(
        "typescript",
        vec![cached_ts_export("connect", &source_path)],
    );

    let mut item = test_completion_item("connect", "connect");
    item.raw_json = Some(r#"{"label":"connect","data":{"source":"./api"}}"#.to_string());

    let completion = crate::app::app_state::CompletionState::from_lsp_items(
        vec![item],
        0,
        3,
        0,
        "con".to_string(),
        &cache,
        Some("typescript"),
    );

    let entry = completion
        .filtered_items
        .iter()
        .find(|entry| entry.item.label == "connect")
        .expect("connect candidate present");
    assert_eq!(
        entry.item.export_kind, None,
        "resolvable LSP item must keep export_kind None so resolve fires"
    );
    assert_eq!(
        entry.item.source_path, None,
        "resolvable LSP item must keep source_path None so resolve fires"
    );
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn typing_filename_suggests_require_for_whole_module_default() {
    // The user's exact case: `lib/logger.js` does `module.exports = createLogger()`
    // (RHS has no name). Typing `logg` in another file must suggest `logger` and,
    // on accept, add `const logger = require('../lib/logger')`.
    let mut shell = AppShell::new_for_tests().expect("create app shell");
    // The real-world shape: a non-importable local `const logger` plus a chained
    // `let seft = module.exports = exports = { … }` whole-module export.
    let logger_src = "const logger = winston.createLogger({});\nlet seft = module.exports = exports = {\n  logError: () => {},\n};\n";
    let text = "const x = logg";
    let root = completion_temp_root("filename_default_require");
    let logger_path = write_completion_file(&root.join("lib/logger.js"), logger_src);
    let _path = open_completion_file(&mut shell, &root.join("src/app.js"), text);

    let cache = crate::lsp::WorkspaceSymbolCache::new();
    cache.insert_symbols(
        "javascript",
        crate::lsp::extract_ts_js_exports_from_text(&logger_path, &root, logger_src),
    );

    let cursor_col = text.chars().count();
    let prefix_start = "const x = ".chars().count();
    let completion = crate::app::app_state::CompletionState::from_lsp_items(
        Vec::new(),
        0,
        cursor_col,
        prefix_start,
        "logg".to_string(),
        &cache,
        Some("javascript"),
    );
    let logger = completion
        .filtered_items
        .iter()
        .find(|entry| entry.item.label == "logger")
        .expect("whole-module default should be suggested when typing its file name");
    assert_eq!(logger.item.export_kind.as_deref(), Some("default"));

    assert!(shell.app_state.jump_to_line_and_column(0, cursor_col));
    assert!(shell.app_state.set_completion(completion));
    assert!(shell.handle_command(Command::CompletionAccept));
    assert_eq!(
        shell.app_state.text_string(),
        "const logger = require('../lib/logger');\nconst x = logger"
    );
}

#[test]
fn workspace_completion_fallback_inserts_commonjs_named_require() {
    let mut shell = AppShell::new_for_tests().expect("create app shell");
    let text = "const value = con";
    let root = completion_temp_root("commonjs_named_require");
    let source_path = write_completion_file(
        &root.join("src/api.cjs"),
        "exports.connect = function() {}\n",
    );
    let _path = open_completion_file(&mut shell, &root.join("src/app.cjs"), text);
    let cache = crate::lsp::WorkspaceSymbolCache::new();
    cache.insert_symbols(
        "javascript",
        vec![cached_ts_export("connect", &source_path)],
    );
    let cursor_col = text.chars().count();
    let prefix_start = "const value = ".chars().count();
    let completion = crate::app::app_state::CompletionState::from_lsp_items(
        Vec::new(),
        0,
        cursor_col,
        prefix_start,
        "con".to_string(),
        &cache,
        Some("javascript"),
    );
    assert!(!completion.filtered_items.is_empty());
    assert!(shell.app_state.jump_to_line_and_column(0, cursor_col));
    assert!(shell.app_state.set_completion(completion));

    assert!(shell.handle_command(Command::CompletionAccept));
    assert_eq!(
        shell.app_state.text_string(),
        "const { connect } = require('./api');\nconst value = connect()"
    );
}

#[test]
fn workspace_completion_fallback_inserts_package_default_import() {
    let mut shell = AppShell::new_for_tests().expect("create app shell");
    let text = "const client = axi";
    let root = completion_temp_root("package_default_import");
    let package_type_path = write_completion_file(
        &root.join("node_modules/axios/index.d.ts"),
        "declare const axios: AxiosStatic;\nexport default axios;\n",
    );
    let _path = open_completion_file(&mut shell, &root.join("src/app.ts"), text);
    let cache = crate::lsp::WorkspaceSymbolCache::new();
    cache.insert_symbols(
        "typescript",
        vec![cached_package_default_export(
            "axios",
            &package_type_path,
            "axios",
        )],
    );
    let cursor_col = text.chars().count();
    let prefix_start = "const client = ".chars().count();
    let completion = crate::app::app_state::CompletionState::from_lsp_items(
        Vec::new(),
        0,
        cursor_col,
        prefix_start,
        "axi".to_string(),
        &cache,
        Some("typescript"),
    );
    assert!(!completion.filtered_items.is_empty());
    assert!(shell.app_state.jump_to_line_and_column(0, cursor_col));
    assert!(shell.app_state.set_completion(completion));

    assert!(shell.handle_command(Command::CompletionAccept));
    assert_eq!(
        shell.app_state.text_string(),
        "import axios from 'axios';\nconst client = axios"
    );
}

#[test]
fn workspace_completion_fallback_inserts_package_default_require_for_commonjs() {
    let mut shell = AppShell::new_for_tests().expect("create app shell");
    let text = "const client = axi";
    let root = completion_temp_root("package_default_require");
    let package_type_path = write_completion_file(
        &root.join("node_modules/axios/index.d.ts"),
        "declare const axios: AxiosStatic;\nexport = axios;\n",
    );
    let _path = open_completion_file(&mut shell, &root.join("src/app.cjs"), text);
    let cache = crate::lsp::WorkspaceSymbolCache::new();
    cache.insert_symbols(
        "javascript",
        vec![cached_package_default_export(
            "axios",
            &package_type_path,
            "axios",
        )],
    );
    let cursor_col = text.chars().count();
    let prefix_start = "const client = ".chars().count();
    let completion = crate::app::app_state::CompletionState::from_lsp_items(
        Vec::new(),
        0,
        cursor_col,
        prefix_start,
        "axi".to_string(),
        &cache,
        Some("javascript"),
    );
    assert!(!completion.filtered_items.is_empty());
    assert!(shell.app_state.jump_to_line_and_column(0, cursor_col));
    assert!(shell.app_state.set_completion(completion));

    assert!(shell.handle_command(Command::CompletionAccept));
    assert_eq!(
        shell.app_state.text_string(),
        "const axios = require('axios');\nconst client = axios"
    );
}

#[test]
fn workspace_completion_fallback_same_file_symbol_has_no_import() {
    let mut shell = AppShell::new_for_tests().expect("create app shell");
    let root = completion_temp_root("same_file");
    let active_path = open_completion_file(&mut shell, &root.join("src/app.ts"), "con");
    let cache = crate::lsp::WorkspaceSymbolCache::new();
    cache.insert_symbols(
        "typescript",
        vec![cached_ts_export("connect", &active_path)],
    );
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
    assert_eq!(shell.app_state.text_string(), "connect()");
}

#[test]
fn go_workspace_completion_accept_does_not_synthesize_imports() {
    let mut shell = AppShell::new_for_tests().expect("create app shell");
    let root = completion_temp_root("go_no_synthetic_import");
    let source_path =
        write_completion_file(&root.join("api.go"), "package main\n\nfunc Connect() {}\n");
    let text = "package main\n\nfunc main() {\n\tCon\n}\n";
    let _active_path = open_completion_file(&mut shell, &root.join("main.go"), text);
    let cache = crate::lsp::WorkspaceSymbolCache::new();
    cache.insert_symbols("go", vec![cached_go_symbol("Connect", &source_path)]);
    let completion = crate::app::app_state::CompletionState::from_lsp_items(
        Vec::new(),
        3,
        "\tCon".chars().count(),
        "\t".chars().count(),
        "Con".to_string(),
        &cache,
        Some("go"),
    );
    assert!(!completion.filtered_items.is_empty());
    assert!(
        shell
            .app_state
            .jump_to_line_and_column(3, "\tCon".chars().count())
    );
    assert!(shell.app_state.set_completion(completion));

    assert!(shell.handle_command(Command::CompletionAccept));
    assert_eq!(
        shell.app_state.text_string(),
        "package main\n\nfunc main() {\n\tConnect()\n}\n"
    );
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
fn resize_editor_h_shrinks_left_dock() {
    let mut shell = AppShell::new_for_tests().expect("create app shell");
    shell.panel_state.left.visible = true;
    shell.panel_state.left.size_px = 280.0;
    shell.panel_state.right.visible = true;
    shell.panel_state.right.size_px = 320.0;
    shell.focus_manager.set(FocusTarget::CenterEditor);

    // `h` with the editor focused shrinks the left dock (editor grows leftward).
    let changed = shell.handle_command(Command::ResizeDecreaseWidth);

    assert!(changed);
    assert_eq!(shell.panel_state.left.size_px, 260.0);
    assert_eq!(shell.panel_state.right.size_px, 320.0);
}

#[test]
fn resize_editor_l_shrinks_right_dock() {
    let mut shell = AppShell::new_for_tests().expect("create app shell");
    shell.panel_state.left.visible = true;
    shell.panel_state.left.size_px = 280.0;
    shell.panel_state.right.visible = true;
    shell.panel_state.right.size_px = 320.0;
    shell.focus_manager.set(FocusTarget::CenterEditor);

    // `l` with the editor focused shrinks the right dock (editor grows rightward).
    let changed = shell.handle_command(Command::ResizeIncreaseWidth);

    assert!(changed);
    assert_eq!(shell.panel_state.left.size_px, 280.0);
    assert_eq!(shell.panel_state.right.size_px, 300.0);
}

#[test]
fn resize_grow_left_dock_command_grows_left() {
    let mut shell = AppShell::new_for_tests().expect("create app shell");
    shell.panel_state.left.visible = true;
    shell.panel_state.left.size_px = 280.0;
    shell.focus_manager.set(FocusTarget::CenterEditor);

    // `H` grows the left dock (editor shrinks on its left edge).
    let changed = shell.handle_command(Command::ResizeGrowLeftDock);

    assert!(changed);
    assert_eq!(shell.panel_state.left.size_px, 300.0);
}

#[test]
fn resize_grow_right_dock_command_grows_right() {
    let mut shell = AppShell::new_for_tests().expect("create app shell");
    shell.panel_state.right.visible = true;
    shell.panel_state.right.size_px = 320.0;
    shell.focus_manager.set(FocusTarget::CenterEditor);

    // `L` grows the right dock (editor shrinks on its right edge).
    let changed = shell.handle_command(Command::ResizeGrowRightDock);

    assert!(changed);
    assert_eq!(shell.panel_state.right.size_px, 340.0);
}

#[test]
fn resize_editor_j_makes_editor_taller() {
    let mut shell = AppShell::new_for_tests().expect("create app shell");
    shell.panel_state.bottom.visible = true;
    shell.panel_state.bottom.size_px = 230.0;
    shell.focus_manager.set(FocusTarget::CenterEditor);

    // `j` with the editor focused shrinks the bottom panel (editor grows down).
    let changed = shell.handle_command(Command::ResizeIncreaseHeight);

    assert!(changed);
    assert_eq!(shell.panel_state.bottom.size_px, 210.0);
}

#[test]
fn resize_editor_k_makes_editor_shorter() {
    let mut shell = AppShell::new_for_tests().expect("create app shell");
    shell.panel_state.bottom.visible = true;
    shell.panel_state.bottom.size_px = 230.0;
    shell.focus_manager.set(FocusTarget::CenterEditor);

    let changed = shell.handle_command(Command::ResizeDecreaseHeight);

    assert!(changed);
    assert_eq!(shell.panel_state.bottom.size_px, 250.0);
}

#[test]
fn resize_editor_h_is_noop_when_left_dock_hidden() {
    let mut shell = AppShell::new_for_tests().expect("create app shell");
    shell.panel_state.left.visible = false;
    shell.panel_state.left.size_px = 280.0;
    shell.focus_manager.set(FocusTarget::CenterEditor);

    let changed = shell.handle_command(Command::ResizeDecreaseWidth);

    assert!(!changed);
    assert_eq!(shell.panel_state.left.size_px, 280.0);
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

#[test]
fn manual_trigger_completion_dismisses_ghost_text_and_invalidates_inflight_ai() {
    let mut shell = AppShell::new_for_tests().expect("create app shell");
    let root = std::env::temp_dir().join(format!(
        "netherize_manual_completion_{}",
        std::process::id()
    ));
    std::fs::create_dir_all(root.join("src")).expect("create workspace");
    let file_path = root.join("src/main.rs");
    std::fs::write(&file_path, "fn demo() {\n    de\n}\n").expect("write file");
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
    shell
        .app_state
        .apply_mode_event(ModeEvent::EnterInsert)
        .expect("enter insert");
    shell.app_state.jump_to_line_and_column(1, 6);

    // Ghost text visible + an AI request in flight at the current caret.
    shell
        .app_state
        .set_inline_suggestion(Some("mo()".to_string()));
    shell.reanchor_ai_inline();
    shell.ai_inline_inflight = true;
    shell.ai_inline_cancel_token = Some(CancellationToken::new());
    let revision_before = shell.ai_inline_revision;

    assert!(shell.handle_command(Command::TriggerCompletion));

    // Ctrl+Space must dismiss the ghost, cancel AI, and submit an LSP request.
    assert!(shell.app_state.inline_suggestion().is_none());
    assert!(shell.ai_inline_anchor.is_none());
    assert!(!shell.ai_inline_inflight);
    assert!(shell.ai_inline_revision > revision_before);
    assert!(shell.app_state.is_completion_loading());
    let request_id = shell
        .active_lsp_completion_request_id
        .expect("manual completion request should be active");

    shell.on_worker_result(crate::async_runtime::message::WorkerResult {
        request_id,
        revision_id: 0,
        topic: crate::async_runtime::message::RequestTopic::LspRequest,
        payload: crate::async_runtime::message::WorkerResultPayload::LspCompletionResult {
            items: vec![test_completion_item("demo", "demo")],
            cursor_line: 1,
            cursor_col: 6,
            prefix_start_col: 4,
            prefix: "de".to_string(),
        },
    });

    assert!(shell.app_state.completion().is_some());

    // A late result from the cancelled AI request must not replace the manual
    // completion popup or restore ghost text.
    shell.on_worker_result(crate::async_runtime::message::WorkerResult {
        request_id: request_id + 1,
        revision_id: revision_before,
        topic: crate::async_runtime::message::RequestTopic::AiInlineCompletion,
        payload: crate::async_runtime::message::WorkerResultPayload::AiInlineCompletionResult {
            suggestion: "mo()".to_string(),
        },
    });

    assert!(shell.app_state.inline_suggestion().is_none());
    assert!(shell.app_state.completion().is_some());
}

#[test]
fn ai_inline_result_yields_to_open_completion_menu() {
    // LSP completion wins: a VALID (current-revision, current-anchor) AI inline
    // result that arrives while the completion menu is open must be dropped — it
    // must neither show ghost text nor close the menu the user is picking from.
    let mut shell = AppShell::new_for_tests().expect("create app shell");
    let root = completion_temp_root("ai_yields_to_completion");
    let _path = open_completion_file(&mut shell, &root.join("src/app.ts"), "axios.p");
    let cache = crate::lsp::WorkspaceSymbolCache::new();
    let completion = crate::app::app_state::CompletionState::from_lsp_items(
        vec![test_completion_item("post", "post")],
        0,
        "axios.p".chars().count(),
        "axios.".chars().count(),
        "p".to_string(),
        &cache,
        Some("typescript"),
    );
    shell
        .app_state
        .apply_mode_event(ModeEvent::EnterInsert)
        .expect("enter insert");
    assert!(shell.app_state.set_completion(completion));
    assert!(shell.app_state.has_completion());
    // Anchor the AI pipeline at the current caret so the anchor guard would PASS —
    // proving the result is dropped by the completion-open guard, not the anchor one.
    shell.reanchor_ai_inline();
    let revision = shell.ai_inline_revision;

    shell.on_worker_result(crate::async_runtime::message::WorkerResult {
        request_id: 999,
        revision_id: revision,
        topic: crate::async_runtime::message::RequestTopic::AiInlineCompletion,
        payload: crate::async_runtime::message::WorkerResultPayload::AiInlineCompletionResult {
            suggestion: "ost()".to_string(),
        },
    });

    assert!(
        shell.app_state.inline_suggestion().is_none(),
        "AI ghost text must not show over the completion menu"
    );
    assert!(
        shell.app_state.has_completion(),
        "the LSP completion menu must stay open (AI inline yields to it)"
    );
}

#[test]
fn test_outline_navigation_commands() {
    use crate::async_runtime::message::{LspDocumentSymbol, LspPosition, LspRange};

    let mut shell = AppShell::new_for_tests().expect("create app shell");
    let file_path = std::env::temp_dir().join("test_outline.rs");
    std::fs::write(&file_path, "fn foo() {}\n\nfn bar() {}\n").expect("write file");
    shell
        .app_state
        .open_file(file_path.clone())
        .expect("open file");

    shell.cached_document_symbols = vec![
        LspDocumentSymbol {
            name: "foo".to_string(),
            kind: "Function".to_string(),
            range: LspRange {
                start: LspPosition {
                    line: 0,
                    character: 3,
                },
                end: LspPosition {
                    line: 0,
                    character: 6,
                },
            },
            ancestors: vec![],
        },
        LspDocumentSymbol {
            name: "bar".to_string(),
            kind: "Function".to_string(),
            range: LspRange {
                start: LspPosition {
                    line: 2,
                    character: 3,
                },
                end: LspPosition {
                    line: 2,
                    character: 6,
                },
            },
            ancestors: vec![],
        },
    ];

    // Set focus and switch active tab so outline commands are not bypassed/reset
    shell.focus_manager.set(FocusTarget::LeftSidebar);
    shell
        .panel_state
        .left
        .switch_to_tab(crate::workbench::panel_state::PanelTabId::Outline);

    // Set cursor to a line without any symbols (line 1)
    shell.app_state.jump_to_line_and_column(1, 0);

    // Initial state
    assert_eq!(shell.outline_selected, None);

    // 1. Move to next (first symbol)
    shell.sidebar_needs_layout = false;
    assert!(shell.handle_command(Command::OutlineNext));
    assert_eq!(shell.outline_selected, Some(0));
    assert_eq!(shell.app_state.cursor_line_col(), (0, 3));
    // The outline highlight lives in the sidebar, so navigating must flag the
    // sidebar for re-layout or the highlight appears frozen.
    assert!(
        shell.sidebar_needs_layout,
        "OutlineNext must request a sidebar re-render so the highlight moves"
    );

    // 2. Move to next (second symbol)
    assert!(shell.handle_command(Command::OutlineNext));
    assert_eq!(shell.outline_selected, Some(1));
    assert_eq!(shell.app_state.cursor_line_col(), (2, 3));

    // 3. Move to next at boundary (should clamp to last symbol)
    assert!(shell.handle_command(Command::OutlineNext));
    assert_eq!(shell.outline_selected, Some(1));
    assert_eq!(shell.app_state.cursor_line_col(), (2, 3));

    // 4. Move to previous (first symbol)
    shell.sidebar_needs_layout = false;
    assert!(shell.handle_command(Command::OutlinePrev));
    assert_eq!(shell.outline_selected, Some(0));
    assert_eq!(shell.app_state.cursor_line_col(), (0, 3));
    assert!(
        shell.sidebar_needs_layout,
        "OutlinePrev must request a sidebar re-render so the highlight moves"
    );

    // 5. Move to previous at boundary (should clamp/stay at first symbol)
    assert!(shell.handle_command(Command::OutlinePrev));
    assert_eq!(shell.outline_selected, Some(0));
    assert_eq!(shell.app_state.cursor_line_col(), (0, 3));

    // 6. Confirm selection (should clear outline_selected and focus editor)
    shell.focus_manager.set(FocusTarget::LeftSidebar);
    assert!(shell.handle_command(Command::OutlineConfirm));
    assert_eq!(shell.outline_selected, None);
    assert_eq!(shell.focus_manager.current(), FocusTarget::CenterEditor);
    assert_eq!(shell.app_state.cursor_line_col(), (0, 3));

    // First/last shortcuts select the corresponding symbol directly.
    shell.focus_manager.set(FocusTarget::LeftSidebar);
    assert!(shell.handle_command(Command::OutlineLast));
    assert_eq!(shell.outline_selected, Some(1));
    assert_eq!(shell.app_state.cursor_line_col(), (2, 3));

    assert!(shell.handle_command(Command::OutlineFirst));
    assert_eq!(shell.outline_selected, Some(0));
    assert_eq!(shell.app_state.cursor_line_col(), (0, 3));

    let _ = std::fs::remove_file(file_path);
}

#[test]
fn outline_navigation_preserves_selection_in_right_dock() {
    use crate::async_runtime::message::{LspDocumentSymbol, LspPosition, LspRange};

    let mut shell = AppShell::new_for_tests().expect("create app shell");
    shell.cached_document_symbols = vec![LspDocumentSymbol {
        name: "foo".to_string(),
        kind: "Function".to_string(),
        range: LspRange {
            start: LspPosition {
                line: 0,
                character: 0,
            },
            end: LspPosition {
                line: 0,
                character: 3,
            },
        },
        ancestors: vec![],
    }];
    shell.panel_state.right.tabs = vec![
        crate::workbench::panel_state::PanelTabId::AiChat,
        crate::workbench::panel_state::PanelTabId::TestRunner,
        crate::workbench::panel_state::PanelTabId::Outline,
    ];
    shell.focus_manager.set(FocusTarget::RightSidebar);
    shell
        .panel_state
        .right
        .switch_to_tab(crate::workbench::panel_state::PanelTabId::Outline);

    assert!(shell.handle_command(Command::OutlineNext));
    assert_eq!(shell.outline_selected, Some(0));
}

#[test]
fn outline_symbol_refresh_tracks_active_buffer_and_retries_after_lsp_startup() {
    use crate::async_runtime::message::{
        LspDocumentSymbol, LspPosition, LspRange, RequestTopic, WorkerResult, WorkerResultPayload,
    };

    let mut shell = AppShell::new_for_tests().expect("create app shell");
    let root =
        std::env::temp_dir().join(format!("netherize_outline_refresh_{}", std::process::id()));
    std::fs::create_dir_all(root.join("src")).expect("create workspace");
    let first_path = root.join("src/first.rs");
    let second_path = root.join("src/second.rs");
    std::fs::write(&first_path, "fn first() {}\n").expect("write first file");
    std::fs::write(&second_path, "fn second() {}\n").expect("write second file");
    shell
        .app_state
        .attach_workspace(root.clone())
        .expect("attach workspace");
    shell
        .app_state
        .open_file(first_path.clone())
        .expect("open first file");
    shell.cached_document_symbols_path = Some(first_path.clone());
    shell.cached_document_symbols = vec![LspDocumentSymbol {
        name: "first".to_string(),
        kind: "Function".to_string(),
        range: LspRange {
            start: LspPosition {
                line: 0,
                character: 3,
            },
            end: LspPosition {
                line: 0,
                character: 8,
            },
        },
        ancestors: vec![],
    }];
    shell.outline_fetch_path = Some(first_path);
    shell
        .app_state
        .open_file(second_path.clone())
        .expect("open second file");
    let active_second_path = shell
        .app_state
        .active_file()
        .map(PathBuf::from)
        .expect("active second file");

    // The old file's symbols must disappear immediately, but an unavailable
    // LSP must not mark the new file as fetched or suppress the later retry.
    shell.ensure_outline_symbols();
    assert!(shell.cached_document_symbols.is_empty());
    assert_eq!(shell.cached_document_symbols_path, None);
    assert_eq!(shell.outline_fetch_path, None);

    shell.panel_state.left.visible = true;
    shell
        .panel_state
        .left
        .switch_to_tab(crate::workbench::panel_state::PanelTabId::Outline);
    shell.on_worker_result(WorkerResult {
        request_id: 1,
        revision_id: 0,
        topic: RequestTopic::LspClient,
        payload: WorkerResultPayload::LspServerStarted {
            server_name: "rust-analyzer".to_string(),
            root_path: root.clone(),
            completion_trigger_chars: Vec::new(),
        },
    });
    assert_eq!(
        shell.outline_fetch_path.as_deref(),
        Some(active_second_path.as_path())
    );

    shell.sidebar_needs_layout = false;
    shell.on_worker_result(WorkerResult {
        request_id: 2,
        revision_id: shell.document_symbols_request_revision,
        topic: RequestTopic::LspRequest,
        payload: WorkerResultPayload::LspDocumentSymbolsResult {
            uri: crate::lsp::client::path_to_lsp_uri(&second_path),
            symbols: vec![LspDocumentSymbol {
                name: "second".to_string(),
                kind: "Function".to_string(),
                range: LspRange {
                    start: LspPosition {
                        line: 0,
                        character: 3,
                    },
                    end: LspPosition {
                        line: 0,
                        character: 9,
                    },
                },
                ancestors: vec![],
            }],
        },
    });
    assert_eq!(
        shell.cached_document_symbols_path.as_deref(),
        Some(active_second_path.as_path())
    );
    assert_eq!(shell.cached_document_symbols[0].name, "second");
    assert!(
        shell.sidebar_needs_layout,
        "document-symbol results must invalidate the visible Outline layout"
    );

    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn generated_leetcode_tests_populate_runner_with_ai_flag() {
    let mut shell = AppShell::new_for_tests().expect("create app shell");
    // Simulate receiving AI-generated test cases via the worker result.
    shell.app_state.test_runner.is_generating = true;
    shell.on_worker_result(crate::async_runtime::message::WorkerResult {
        request_id: 42,
        revision_id: 0,
        topic: crate::async_runtime::message::RequestTopic::LeetCode,
        payload: crate::async_runtime::message::WorkerResultPayload::LeetCodeTestsGenerated {
            id: "1".into(),
            cases: vec![
                crate::runner::leetcode_api::LeetCodeTestCase {
                    input: r#"{"nums":[2,7,11,15],"target":9}"#.into(),
                    expected: "[0,1]".into(),
                },
                crate::runner::leetcode_api::LeetCodeTestCase {
                    input: r#"{"nums":[3,3],"target":6}"#.into(),
                    expected: "[0,1]".into(),
                },
            ],
            verified: false,
        },
    });

    // is_generating should be cleared.
    assert!(!shell.app_state.test_runner.is_generating);
    // Two cases should now be in the runner.
    assert_eq!(shell.app_state.test_runner.cases.len(), 2);
    // First case should be selected.
    assert_eq!(shell.app_state.test_runner.selected, Some(0));
    // All cases should be flagged as AI-generated.
    assert!(shell.app_state.test_runner.cases[0].ai_generated);
    assert!(shell.app_state.test_runner.cases[1].ai_generated);
    // Verify case data.
    assert_eq!(
        shell.app_state.test_runner.cases[0].input,
        r#"{"nums":[2,7,11,15],"target":9}"#
    );
    assert_eq!(shell.app_state.test_runner.cases[0].expected, "[0,1]");
}

#[test]
fn generated_leetcode_tests_failure_clears_generating_flag() {
    let mut shell = AppShell::new_for_tests().expect("create app shell");
    shell.app_state.test_runner.is_generating = true;
    // Add a pre-existing case to ensure it survives the failure.
    shell.app_state.test_runner.cases = vec![crate::runner::TestCase::new("{}", "null")];

    shell.on_worker_result(crate::async_runtime::message::WorkerResult {
        request_id: 43,
        revision_id: 0,
        topic: crate::async_runtime::message::RequestTopic::LeetCode,
        payload: crate::async_runtime::message::WorkerResultPayload::LeetCodeTestsGenerateFailed {
            message: "AI model exhausted token budget on reasoning".into(),
        },
    });

    // is_generating should be cleared even on failure.
    assert!(!shell.app_state.test_runner.is_generating);
    // Pre-existing cases should NOT be replaced on failure.
    assert_eq!(shell.app_state.test_runner.cases.len(), 1);
}

#[test]
fn test_terminal_focus_exits_on_right_sidebar_tab_switch() {
    use crate::core::mode::{EditorMode, ModeEvent};
    use crate::workbench::focus_manager::FocusTarget;

    let mut shell = AppShell::new_for_tests().expect("create app shell");

    // 1. Focus RightSidebar and switch to active AI Chat tab
    shell.focus_manager.set(FocusTarget::RightSidebar);
    shell.panel_state.right.switch_to_index(0); // AI Chat is at index 0

    // 2. Set mode to TerminalFocus
    let _ = shell.app_state.apply_mode_event(ModeEvent::FocusTerminal);
    assert_eq!(shell.app_state.current_mode(), EditorMode::TerminalFocus);

    // 3. Switch Right Tab to TestRunner (index 1) via command
    assert!(shell.handle_command(Command::SwitchRightTab(1)));

    // 4. Redraw triggers the validation/reset
    shell.redraw();

    // 5. Mode must have transitioned back to Normal (or non-terminal mode) since TestRunner doesn't support terminal
    assert_eq!(shell.app_state.current_mode(), EditorMode::Normal);
}

// ---- Neovide-style pixel-smooth editor viewport scrolling ----

/// An explicit scroll command (force flag set) animates the editor buffer rather
/// than snapping: after one tick the tween is live and `current` sits between the
/// start and the target.
#[test]
fn forced_editor_scroll_animates_not_snaps() {
    let mut shell = AppShell::new_for_tests().expect("create app shell");
    let text: String = (0..500).map(|i| format!("line {i}\n")).collect();
    shell.app_state = AppState::from_text(PathBuf::from("scroll_anim_test.rs"), &text);

    let now = std::time::Instant::now();
    shell.advance_scroll_anim(now); // settle baseline (target == 0)

    // Simulate Ctrl-D / zz moving the target, with the explicit-command force flag.
    shell.app_state.target_scroll_y = 30.0;
    shell.scroll_anim_force = true;

    let moved = shell.advance_scroll_anim(now + std::time::Duration::from_millis(1));
    assert!(moved, "scroll should advance");
    assert!(
        shell.scroll_anim_started_at.is_some(),
        "explicit command should animate, not snap"
    );
    assert!(
        shell.app_state.current_scroll_y < 30.0,
        "mid-tween, got {}",
        shell.app_state.current_scroll_y
    );
}

/// A single-line cursor follow at the viewport edge (no force) glides smoothly
/// instead of jumping — one line is a real move, above the 0.5-line snap floor.
#[test]
fn unforced_single_line_follow_animates() {
    let mut shell = AppShell::new_for_tests().expect("create app shell");
    let text: String = (0..500).map(|i| format!("line {i}\n")).collect();
    shell.app_state = AppState::from_text(PathBuf::from("scroll_snap_test.rs"), &text);

    let now = std::time::Instant::now();
    shell.advance_scroll_anim(now); // baseline

    shell.app_state.target_scroll_y = 1.0; // one line at the edge, no force
    shell.advance_scroll_anim(now + std::time::Duration::from_millis(1)); // retarget (t=0)
    assert!(
        shell.scroll_anim_started_at.is_some(),
        "single-line edge scroll should glide, not jump"
    );
    // Sample mid-tween: the viewport is partway between the old and new line.
    let moved = shell.advance_scroll_anim(now + std::time::Duration::from_millis(30));
    assert!(moved);
    assert!(
        shell.app_state.current_scroll_y > 0.0 && shell.app_state.current_scroll_y < 1.0,
        "mid-tween toward the line, got {}",
        shell.app_state.current_scroll_y
    );
}

/// Driving the editor smooth-scroll tween must not mutate any NetherCanvas state.
#[test]
fn editor_smooth_scroll_leaves_canvas_untouched() {
    let (mut shell, dir, _card) = shell_with_background_canvas_card();
    let canvas_before = format!("{:?}", shell.app_state.canvas());

    shell.app_state.target_scroll_y = 25.0;
    shell.scroll_anim_force = true;
    let now = std::time::Instant::now();
    shell.advance_scroll_anim(now);
    shell.advance_scroll_anim(now + std::time::Duration::from_millis(5));

    let canvas_after = format!("{:?}", shell.app_state.canvas());
    assert_eq!(
        canvas_before, canvas_after,
        "editor smooth scroll must not touch canvas camera/cards"
    );
    let _ = std::fs::remove_dir_all(dir);
}

/// Ctrl-D moves cursor AND viewport, so the caret must ride the SAME tween: a
/// non-zero `caret_scroll_lag` mid-tween proves the caret is gliding with the
/// scroll instead of teleporting to its line and waiting for the viewport.
#[test]
fn halfpage_down_couples_caret_lag_to_scroll() {
    let mut shell = AppShell::new_for_tests().expect("create app shell");
    let text: String = (0..400).map(|i| format!("line {i}\n")).collect();
    shell.app_state = AppState::from_text(PathBuf::from("couple.rs"), &text);

    let now = std::time::Instant::now();
    shell.advance_scroll_anim(now); // settle baseline

    assert!(shell.handle_command(Command::ScrollHalfPageDown));
    shell.advance_scroll_anim(now + std::time::Duration::from_millis(1)); // retarget (t≈0)
    assert!(
        shell.scroll_anim_started_at.is_some(),
        "half-page should animate, not snap"
    );
    shell.advance_scroll_anim(now + std::time::Duration::from_millis(30)); // mid-tween
    assert!(
        shell.app_state.caret_scroll_lag.abs() > 1e-4,
        "caret must lag (couple) mid-tween, got {}",
        shell.app_state.caret_scroll_lag
    );
    assert!(
        shell.app_state.current_scroll_y > 0.0
            && shell.app_state.current_scroll_y <= shell.app_state.target_scroll_y + 1e-3,
        "scroll mid-tween between start and target: {} (target {})",
        shell.app_state.current_scroll_y,
        shell.app_state.target_scroll_y
    );
}

/// A `j` that does NOT move the viewport keeps the caret instant — no lag, no tween.
#[test]
fn move_down_without_scroll_keeps_caret_unlagged() {
    let mut shell = AppShell::new_for_tests().expect("create app shell");
    let text: String = (0..400).map(|i| format!("line {i}\n")).collect();
    shell.app_state = AppState::from_text(PathBuf::from("nolag.rs"), &text);

    let now = std::time::Instant::now();
    shell.advance_scroll_anim(now);
    shell.handle_command(Command::MoveDown); // cursor 0->1, cursor stays on-screen
    shell.advance_scroll_anim(now + std::time::Duration::from_millis(1));
    assert_eq!(
        shell.app_state.caret_scroll_lag, 0.0,
        "no viewport move → caret is instant (unlagged)"
    );
    assert!(shell.scroll_anim_started_at.is_none());
}

/// Once the tween completes the caret sits flush on its line again (lag == 0).
#[test]
fn caret_lag_settles_to_zero_when_tween_completes() {
    let mut shell = AppShell::new_for_tests().expect("create app shell");
    let text: String = (0..400).map(|i| format!("line {i}\n")).collect();
    shell.app_state = AppState::from_text(PathBuf::from("settle.rs"), &text);

    let now = std::time::Instant::now();
    shell.advance_scroll_anim(now);
    shell.handle_command(Command::ScrollHalfPageDown);
    shell.advance_scroll_anim(now + std::time::Duration::from_millis(1));
    // Well past the longest bucket (≤130ms).
    shell.advance_scroll_anim(now + std::time::Duration::from_millis(500));
    assert!(
        shell.scroll_anim_started_at.is_none(),
        "tween should have completed"
    );
    assert_eq!(
        shell.app_state.caret_scroll_lag, 0.0,
        "caret settles flush to its line"
    );
}
