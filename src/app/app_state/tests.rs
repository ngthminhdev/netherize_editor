use super::*;

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        fs,
        path::PathBuf,
        time::{Instant, SystemTime, UNIX_EPOCH},
    };

    use crate::app::command_palette::{CommandPaletteItem, CommandPaletteMode};
    use crate::async_runtime::message::{FilePreviewLine, FileSystemChangeKind, FileSystemEvent};
    use crate::config::keymap_config::KeyBinding;
    use crate::core::commands::{TextObjectKind, TextObjectModifier};
    use crate::core::mode::{EditorMode, ModeEvent};
    use crate::syntax::highlight::HighlightEdit;

    fn unique_temp_path(suffix: &str) -> std::path::PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock drift")
            .as_nanos();
        std::env::temp_dir().join(format!("netherize_phase4_{suffix}_{nanos}.txt"))
    }
    fn unique_temp_dir(suffix: &str) -> std::path::PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock drift")
            .as_nanos();
        std::env::temp_dir().join(format!("netherize_phase4_dir_{suffix}_{nanos}"))
    }

    #[test]
    fn insert_move_and_backspace_flow() {
        let mut state = AppState::new(unique_temp_path("scratch"));
        state.insert_char('a');
        state.insert_char('b');
        state.insert_char('c');
        assert_eq!(state.len_chars(), 3);

        state.move_left();
        state.backspace();

        assert_eq!(state.len_chars(), 2);
        assert_eq!(state.preview(16), "ac");
        assert!(state.is_dirty());
    }

    #[test]
    fn text_edits_record_highlight_byte_deltas() {
        let mut state = AppState::from_text(unique_temp_path("scratch"), "");

        state.insert_char('a');
        assert_eq!(
            state.take_highlight_edits(),
            vec![HighlightEdit::insert(0, 1)]
        );

        assert!(state.backspace());
        assert_eq!(
            state.take_highlight_edits(),
            vec![HighlightEdit::delete(0, 1)]
        );
    }

    #[test]
    fn backspace_between_empty_auto_pair_deletes_both_chars() {
        let mut state = AppState::from_text(unique_temp_path("smart_backspace_pair"), "()");
        state.move_right();

        assert!(state.backspace());

        assert_eq!(state.text_string(), "");
        assert_eq!(state.cursor_char_idx(), 0);
        assert_eq!(
            state.take_highlight_edits(),
            vec![HighlightEdit::delete(0, 2)]
        );
    }

    #[test]
    fn help_buffer_uses_config_driven_keymap_content() {
        let bindings = vec![
            KeyBinding {
                key: "mod+p".to_string(),
                mode: Some("normal".to_string()),
                command: "app.open_file_picker".to_string(),
            },
            KeyBinding {
                key: "<leader>/".to_string(),
                mode: Some("normal".to_string()),
                command: "app.open_command_palette".to_string(),
            },
            KeyBinding {
                key: "j".to_string(),
                mode: Some("normal".to_string()),
                command: "editor.move_down".to_string(),
            },
        ];

        let help = super::HelpState::from_bindings("test-profile", "test.toml", &bindings);
        assert_eq!(help.profile_name, "test-profile");
        assert_eq!(help.source_label, "test.toml");
        assert!(help.sections.iter().any(|section| {
            section.title == "NORMAL"
                && section
                    .entries
                    .iter()
                    .any(|entry| entry.keys == vec!["mod+p"] && entry.label == "Open file picker")
        }));
        assert!(
            help.lines
                .iter()
                .any(|line| line.contains("mod+p") && line.contains("Open file picker"))
        );
        assert!(
            help.lines
                .iter()
                .any(|line| line.contains("<Space> /") && line.contains("Open command palette"))
        );
        assert!(
            help.lines
                .iter()
                .any(|line| line.contains("j") && line.contains("Move down"))
        );
    }

    #[test]
    fn backspace_between_empty_quotes_deletes_both_chars() {
        let mut state = AppState::from_text(unique_temp_path("smart_backspace_quotes"), "\"\"");
        state.move_right();

        assert!(state.backspace());

        assert_eq!(state.text_string(), "");
        assert_eq!(state.cursor_char_idx(), 0);
        assert_eq!(
            state.take_highlight_edits(),
            vec![HighlightEdit::delete(0, 2)]
        );
    }

    #[test]
    fn save_then_open_roundtrip() {
        let save_path = unique_temp_path("save");
        let open_path = unique_temp_path("open");

        let mut state = AppState::from_text(save_path.clone(), "hello");
        let saved = state.save_file().expect("save should succeed");
        let canonical_save_path = save_path
            .canonicalize()
            .expect("canonical save path should exist");
        assert_eq!(saved, canonical_save_path);
        assert!(!state.is_dirty());

        std::fs::write(&open_path, "world").expect("write open file");
        state
            .open_file(open_path.clone())
            .expect("open should succeed");

        assert_eq!(state.preview(16), "world");
        assert!(!state.is_dirty());
        let canonical_open_path = open_path
            .canonicalize()
            .expect("canonical open path should exist");
        assert_eq!(
            state.active_file().expect("has file"),
            canonical_open_path.as_path()
        );

        let _ = std::fs::remove_file(save_path);
        let _ = std::fs::remove_file(open_path);
    }

    #[test]
    fn buffer_cycle_and_close_follow_open_document_ring() {
        let mut state = AppState::new(unique_temp_path("buffer_ring"));
        let root = unique_temp_dir("buffer_ring");
        fs::create_dir_all(&root).expect("create buffer ring root");
        let file_a = root.join("a.rs");
        let file_b = root.join("b.rs");
        let file_c = root.join("c.rs");
        fs::write(&file_a, "alpha\n").expect("write a");
        fs::write(&file_b, "beta\n").expect("write b");
        fs::write(&file_c, "gamma\n").expect("write c");

        state.open_file(file_a.clone()).expect("open a");
        state.open_file(file_b.clone()).expect("open b");
        state.open_file(file_c.clone()).expect("open c");
        assert!(state.active_file().expect("active file").ends_with("c.rs"));

        assert!(state.buffer_prev().expect("buffer prev"));
        assert!(state.active_file().expect("active file").ends_with("b.rs"));
        assert!(state.buffer_next().expect("buffer next"));
        assert!(state.active_file().expect("active file").ends_with("c.rs"));
        assert!(state.buffer_next().expect("buffer next wrap"));
        assert!(state.active_file().expect("active file").ends_with("a.rs"));
        assert!(state.buffer_prev().expect("buffer prev wrap"));
        assert!(state.active_file().expect("active file").ends_with("c.rs"));

        assert!(state.close_current_buffer().expect("close current"));
        assert!(state.active_file().expect("active file").ends_with("b.rs"));
        assert!(state.close_current_buffer().expect("close current"));
        assert!(state.active_file().expect("active file").ends_with("a.rs"));
        assert!(state.close_current_buffer().expect("close current"));
        assert!(state.active_file().is_none());
        assert!(state.text_string().is_empty());

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn terminal_buffer_entries_are_tracked_in_tab_ring() {
        let mut state = AppState::new(unique_temp_path("terminal_buffer"));
        let root = unique_temp_dir("terminal_buffer");
        fs::create_dir_all(&root).expect("create terminal buffer root");
        let file_a = root.join("a.rs");
        fs::write(&file_a, "alpha\n").expect("write a");

        state.open_file(file_a.clone()).expect("open a");
        let terminal_idx = state.open_terminal_buffer("[Lazygit]", Some(root.clone()));

        assert_eq!(state.buffers().len(), 2);
        assert_eq!(state.active_buffer_index(), Some(terminal_idx));
        assert!(state.active_buffer_is_terminal());
        assert_eq!(state.active_filetype_label(), "Terminal");
        assert_eq!(state.buffers()[terminal_idx].label(), "[Lazygit]");

        assert!(state.buffer_prev().expect("switch back to text"));
        assert!(state.active_file().expect("active file").ends_with("a.rs"));
        assert!(!state.active_buffer_is_terminal());

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn move_right_reaches_end_of_last_line_without_newline() {
        let mut state = AppState::from_text(unique_temp_path("cursor"), "abc");

        // Column có thể đi tới 3 (sau ký tự 'c').
        state.move_right();
        state.move_right();
        state.move_right();
        assert_eq!(state.cursor_line_col(), (0, 3));

        // Đi tiếp là no-op.
        state.move_right();
        assert_eq!(state.cursor_line_col(), (0, 3));
    }

    #[test]
    fn move_right_crosses_to_next_line_at_end_of_line() {
        let mut state = AppState::from_text(unique_temp_path("cursor"), "abc\ndef");

        state.move_right(); // col 1
        state.move_right(); // col 2
        state.move_right(); // col 3 (end of line 0)
        assert_eq!(state.cursor_line_col(), (0, 3));

        // ArrowRight ở cuối line 0 -> đầu line 1
        state.move_right();
        assert_eq!(state.cursor_line_col(), (1, 0));
    }

    #[test]
    fn delete_word_forward_eats_word_plus_trailing_space() {
        let mut state = AppState::from_text(unique_temp_path("dw"), "foo   bar baz");
        // cursor at col 0, on 'f'
        assert!(state.delete_word_forward());
        assert_eq!(state.text_string(), "bar baz");
        assert_eq!(state.cursor_line_col(), (0, 0));
        assert!(state.is_dirty());
    }

    #[test]
    fn delete_word_forward_stops_at_newline_and_preserves_next_line() {
        let mut state = AppState::from_text(unique_temp_path("dw_nl"), "foo\nbar");
        // cursor on 'f'; dw should eat "foo" but NOT the '\n'.
        assert!(state.delete_word_forward());
        assert_eq!(state.text_string(), "\nbar");
        assert_eq!(state.cursor_line_col(), (0, 0));
    }

    #[test]
    fn delete_word_forward_on_newline_joins_lines() {
        let mut state = AppState::from_text(unique_temp_path("dw_join"), "ab\ncd");
        // Two move_right steps leave the cursor ON the '\n' (char idx 2). A third
        // move_right would cross into line 1, per move_right's end-of-line jump.
        state.move_right();
        state.move_right();
        assert_eq!(state.cursor_line_col(), (0, 2));
        let cursor_before = state.cursor_char_idx();

        assert!(state.delete_word_forward());
        assert_eq!(state.text_string(), "abcd");
        assert_eq!(state.cursor_char_idx(), cursor_before);
    }

    #[test]
    fn delete_word_forward_on_punct_run_eats_only_punct() {
        let mut state = AppState::from_text(unique_temp_path("dw_punct"), "!!!foo");
        assert!(state.delete_word_forward());
        // Punct class is "!!!", then no trailing space, so cursor lands on 'f'.
        assert_eq!(state.text_string(), "foo");
    }

    #[test]
    fn delete_word_forward_at_eof_is_noop() {
        let mut state = AppState::from_text(unique_temp_path("dw_eof"), "abc");
        state.move_right();
        state.move_right();
        state.move_right(); // col 3 == len_chars
        assert!(!state.delete_word_forward());
        assert_eq!(state.text_string(), "abc");
    }

    #[test]
    fn delete_word_backward_erases_previous_word_span() {
        let mut state = AppState::from_text(unique_temp_path("db"), "foo   bar");
        for _ in 0..6 {
            state.move_right();
        }
        assert_eq!(state.cursor_line_col(), (0, 6));

        assert!(state.delete_word_backward());
        assert_eq!(state.text_string(), "bar");
        assert_eq!(state.cursor_line_col(), (0, 0));
    }

    #[test]
    fn append_after_cursor_moves_one_step_or_stays_at_line_end() {
        let mut state = AppState::from_text(unique_temp_path("a"), "abc");
        assert!(state.append_after_cursor());
        assert_eq!(state.cursor_line_col(), (0, 1));

        state.move_to_line_end();
        assert!(!state.append_after_cursor());
        assert_eq!(state.cursor_line_col(), (0, 3));
    }

    #[test]
    fn change_word_forward_deletes_span_and_sets_dirty() {
        let mut state = AppState::from_text(unique_temp_path("cw"), "foo   bar");
        assert!(state.change_word_forward());
        assert_eq!(state.text_string(), "bar");
        assert_eq!(state.cursor_line_col(), (0, 0));
        assert!(state.is_dirty());
    }

    #[test]
    fn change_word_backward_deletes_previous_span_and_sets_dirty() {
        let mut state = AppState::from_text(unique_temp_path("cb"), "foo   bar");
        for _ in 0..6 {
            state.move_right();
        }
        assert_eq!(state.cursor_line_col(), (0, 6));

        assert!(state.change_word_backward());
        assert_eq!(state.text_string(), "bar");
        assert_eq!(state.cursor_line_col(), (0, 0));
        assert!(state.is_dirty());
    }

    #[test]
    fn replace_char_at_cursor_replaces_without_mode_change() {
        let mut state = AppState::from_text(unique_temp_path("r"), "abc");
        let mode_before = state.current_mode();

        assert!(state.replace_char_at_cursor('X'));
        assert_eq!(state.text_string(), "Xbc");
        assert_eq!(state.current_mode(), mode_before);
        assert!(state.is_dirty());
    }

    #[test]
    fn visual_selection_anchor_focus_and_delete_work() {
        let mut state = AppState::from_text(unique_temp_path("visual"), "abcdef");
        state.move_right(); // anchor at char index 1 ('b')
        state
            .apply_mode_event(ModeEvent::EnterVisual)
            .expect("normal -> visual");
        assert!(state.begin_visual_selection());

        state.move_right();
        state.move_right(); // focus at index 3 ('d') -> selected "bcd"

        let selection = state.visual_selection_range().expect("selection exists");
        assert_eq!(selection.start_char, 1);
        assert_eq!(selection.end_char, 4);
        assert!(state.delete_visual_selection());
        assert_eq!(state.text_string(), "aef");
    }

    #[test]
    fn move_word_forward_jumps_to_next_word_start() {
        let mut state = AppState::from_text(unique_temp_path("w"), "foo   bar");
        assert!(state.move_word_forward());
        assert_eq!(state.cursor_line_col(), (0, 6));
    }

    #[test]
    fn move_word_backward_jumps_to_previous_word_start() {
        let mut state = AppState::from_text(unique_temp_path("b"), "foo   bar");
        for _ in 0..8 {
            state.move_right();
        }
        assert_eq!(state.cursor_line_col(), (0, 8));

        assert!(state.move_word_backward());
        assert_eq!(state.cursor_line_col(), (0, 6));
        assert!(state.move_word_backward());
        assert_eq!(state.cursor_line_col(), (0, 0));
    }

    #[test]
    fn move_word_end_stops_at_last_char_of_word() {
        let mut state = AppState::from_text(unique_temp_path("e"), "foo   bar");
        assert!(state.move_word_end());
        assert_eq!(state.cursor_line_col(), (0, 2));

        assert!(state.move_word_forward());
        assert!(state.move_word_end());
        assert_eq!(state.cursor_line_col(), (0, 8));
    }

    #[test]
    fn line_start_and_first_non_whitespace_motions_work() {
        let mut state = AppState::from_text(unique_temp_path("line_motion"), "   abc");
        for _ in 0..5 {
            state.move_right();
        }
        assert_eq!(state.cursor_line_col(), (0, 5));

        assert!(state.move_to_line_start());
        assert_eq!(state.cursor_line_col(), (0, 0));

        assert!(state.move_to_first_non_whitespace());
        assert_eq!(state.cursor_line_col(), (0, 3));
    }

    #[test]
    fn mode_state_is_centralized_and_defaults_to_normal() {
        let state = AppState::new(unique_temp_path("mode"));
        assert_eq!(state.current_mode(), EditorMode::Normal);
    }

    #[test]
    fn mode_transition_normal_to_insert_via_app_state() {
        let mut state = AppState::new(unique_temp_path("mode"));
        assert_eq!(state.current_mode(), EditorMode::Normal);

        let result = state
            .apply_mode_event(ModeEvent::EnterInsert)
            .expect("normal -> insert should be valid");

        assert_eq!(result.from, EditorMode::Normal);
        assert_eq!(result.to, EditorMode::Insert);
        assert_eq!(state.current_mode(), EditorMode::Insert);
    }

    #[test]
    fn invalid_mode_transition_is_rejected_from_app_state() {
        let mut state = AppState::new(unique_temp_path("mode"));
        state
            .apply_mode_event(ModeEvent::EnterInsert)
            .expect("normal -> insert should be valid");
        let error = state
            .apply_mode_event(ModeEvent::EnterVisual)
            .expect_err("insert -> visual should be invalid");
        assert_eq!(
            error,
            crate::core::mode::ModeTransitionError::InvalidTransition {
                from: EditorMode::Insert,
                event: ModeEvent::EnterVisual
            }
        );
        assert_eq!(state.current_mode(), EditorMode::Insert);
    }

    #[test]
    fn focus_mode_returns_to_previous_mode_via_app_state() {
        let mut state = AppState::new(unique_temp_path("mode"));
        state
            .apply_mode_event(ModeEvent::OpenPalette)
            .expect("normal -> palette focus");
        assert_eq!(state.current_mode(), EditorMode::PaletteFocus);

        state
            .apply_mode_event(ModeEvent::ExitFocus)
            .expect("palette -> previous");
        assert_eq!(state.current_mode(), EditorMode::Normal);
    }

    #[test]
    fn workspace_and_file_picker_state_are_tracked() {
        let mut state = AppState::new(unique_temp_path("workspace"));
        let root = unique_temp_dir("workspace");
        fs::create_dir_all(root.join("src")).expect("create src");
        fs::write(root.join("src/picker.rs"), "pub fn picker() {}\n").expect("write source");

        state
            .attach_workspace(root.clone())
            .expect("attach workspace should succeed");
        assert!(state.workspace_file_count() >= 1);

        let count = state.open_file_picker().expect("open picker");
        assert_eq!(count, 0);
        assert!(state.is_file_picker_open());

        let changed = state
            .file_picker_append_query("picker")
            .expect("append query");
        assert!(changed);
        assert_eq!(state.file_picker_query_text(), "picker");
        assert!(state.set_command_palette_results(
            CommandPaletteMode::FilePicker,
            "picker",
            vec![CommandPaletteItem::file_match(
                "src/picker.rs".to_string(),
                root.join("src/picker.rs"),
            )],
        ));
        assert_eq!(state.file_picker_results().len(), 1);
        assert!(
            state
                .file_picker_results()
                .iter()
                .any(|entry| entry.relative_path.ends_with("src/picker.rs"))
        );

        let _ = state.close_file_picker();
        assert!(!state.is_file_picker_open());

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn file_picker_results_refresh_while_overlay_is_open_after_external_create() {
        let mut state = AppState::new(unique_temp_path("workspace_picker_refresh"));
        let root = unique_temp_dir("workspace_picker_refresh");
        fs::create_dir_all(root.join("src")).expect("create src");
        fs::write(root.join("src/old.rs"), "pub fn old() {}\n").expect("write old");

        state
            .attach_workspace(root.clone())
            .expect("attach workspace should succeed");
        state.open_file_picker().expect("open picker");
        state
            .file_picker_append_query("old")
            .expect("append old query");
        assert!(state.set_command_palette_results(
            CommandPaletteMode::FilePicker,
            "old",
            vec![CommandPaletteItem::file_match(
                "src/old.rs".to_string(),
                root.join("src/old.rs"),
            )],
        ));

        assert!(
            state
                .file_picker_results()
                .iter()
                .all(|entry| !entry.relative_path.ends_with("src/new_file.rs"))
        );

        let created_path = root.join("src/new_file.rs");
        fs::write(&created_path, "pub fn new_file() {}\n").expect("write new file");
        let report = state
            .apply_external_file_events(&[FileSystemEvent {
                kind: FileSystemChangeKind::Create,
                path: created_path,
                new_path: None,
            }])
            .expect("apply external create");

        assert!(report.workspace_reloaded);
        assert!(state.is_file_picker_open());
        assert!(state.set_command_palette_results(
            CommandPaletteMode::FilePicker,
            "old",
            vec![
                CommandPaletteItem::file_match("src/old.rs".to_string(), root.join("src/old.rs")),
                CommandPaletteItem::file_match(
                    "src/new_file.rs".to_string(),
                    root.join("src/new_file.rs"),
                ),
            ],
        ));
        assert!(
            state
                .file_picker_results()
                .iter()
                .any(|entry| entry.relative_path.ends_with("src/new_file.rs"))
        );

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn external_modify_reloads_when_clean_and_warns_when_dirty() {
        let save_path = unique_temp_path("external");
        let mut state = AppState::new(save_path);
        let root = unique_temp_dir("external");
        fs::create_dir_all(root.join("src")).expect("create src");
        let active = root.join("src/main.rs");
        fs::write(&active, "fn main() {}\n").expect("write initial");

        state
            .attach_workspace(root.clone())
            .expect("attach workspace should succeed");
        state.open_file(active.clone()).expect("open active file");

        fs::write(&active, "fn main() { println!(\"reload\"); }\n").expect("write modified");
        let report = state
            .apply_external_file_events(&[FileSystemEvent {
                kind: FileSystemChangeKind::Modify,
                path: active.clone(),
                new_path: None,
            }])
            .expect("apply external clean");

        assert!(report.active_file_reloaded);
        assert!(state.preview(48).contains("reload"));

        state.insert_char('x');
        let dirty_report = state
            .apply_external_file_events(&[FileSystemEvent {
                kind: FileSystemChangeKind::Modify,
                path: active.clone(),
                new_path: None,
            }])
            .expect("apply external dirty");
        assert!(dirty_report.conflict_detected);
        assert!(state.external_conflict_message().is_some());

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn save_file_preserves_cursor_and_selection_state() {
        let root = unique_temp_dir("save_preserve_cursor");
        fs::create_dir_all(&root).expect("create temp dir");
        let file_path = root.join("sample.rs");
        fs::write(&file_path, "alpha\nbeta\ngamma\n").expect("write initial");

        let mut state = AppState::new(unique_temp_path("save_preserve_cursor_fallback"));
        state.open_file(file_path.clone()).expect("open file");
        state.move_down();
        assert!(state.move_to_line_end());
        state
            .apply_mode_event(ModeEvent::EnterVisual)
            .expect("enter visual");
        assert!(state.begin_visual_selection());
        state.move_left();

        let cursor_before = state.cursor_char_idx();
        let selection_before = state.selection_anchor_char_idx;

        let saved_path = state.save_file().expect("save file");

        assert_eq!(saved_path, file_path.canonicalize().expect("canonicalize"));
        assert_eq!(state.cursor_char_idx(), cursor_before);
        assert_eq!(state.selection_anchor_char_idx, selection_before);

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn self_save_modify_event_is_ignored_without_reloading_cursor() {
        let root = unique_temp_dir("self_save_ignore");
        fs::create_dir_all(&root).expect("create temp dir");
        let file_path = root.join("main.rs");
        fs::write(&file_path, "one\ntwo\nthree\n").expect("write initial");

        let mut state = AppState::new(unique_temp_path("self_save_ignore_fallback"));
        state.open_file(file_path.clone()).expect("open file");
        state.move_down();
        assert!(state.move_to_line_end());
        let cursor_before = state.cursor_char_idx();

        state.save_file().expect("save file");

        fs::write(
            &file_path,
            "changed externally but should be ignored in debounce window\n",
        )
        .expect("rewrite file quickly");

        let report = state
            .apply_external_file_events(&[FileSystemEvent {
                kind: FileSystemChangeKind::Modify,
                path: file_path.clone(),
                new_path: None,
            }])
            .expect("apply modify event");

        assert!(!report.active_file_reloaded);
        assert_eq!(state.cursor_char_idx(), cursor_before);
        assert!(!state.preview(128).contains("changed externally"));

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn external_reload_clamps_cursor_and_selection_to_new_buffer_length() {
        let root = unique_temp_dir("external_reload_clamp");
        fs::create_dir_all(&root).expect("create temp dir");
        let file_path = root.join("main.rs");
        fs::write(&file_path, "alpha\nbeta\ngamma\ndelta").expect("write initial");

        let mut state = AppState::new(unique_temp_path("external_reload_clamp_fallback"));
        state.open_file(file_path.clone()).expect("open file");
        assert!(state.move_to_last_line());
        assert!(state.move_to_line_end());
        state
            .apply_mode_event(ModeEvent::EnterVisual)
            .expect("enter visual");
        assert!(state.begin_visual_selection());
        state.move_up();

        fs::write(&file_path, "x\n").expect("write shorter file");
        state.last_saved_at = Some(Instant::now() - AppState::SELF_SAVE_IGNORE_WINDOW);

        let report = state
            .apply_external_file_events(&[FileSystemEvent {
                kind: FileSystemChangeKind::Modify,
                path: file_path.clone(),
                new_path: None,
            }])
            .expect("apply external modify");

        assert!(report.active_file_reloaded);
        assert!(state.preview(16).starts_with('x'));
        assert_eq!(state.cursor_char_idx(), state.len_chars());
        assert!(
            state
                .selection_anchor_char_idx
                .is_none_or(|anchor| anchor <= state.len_chars())
        );

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn external_reload_error_does_not_abort_workspace_updates() {
        let save_path = unique_temp_path("external_reload_error");
        let mut state = AppState::new(save_path);
        let root = unique_temp_dir("external_reload_error");
        fs::create_dir_all(root.join("src")).expect("create src");
        let active = root.join("src/main.rs");
        fs::write(&active, "fn main() {}\n").expect("write initial");

        state
            .attach_workspace(root.clone())
            .expect("attach workspace should succeed");
        state.open_file(active.clone()).expect("open active file");
        state.open_file_picker().expect("open picker");
        state
            .file_picker_append_query("created")
            .expect("append created query");

        // Mô phỏng file active bị xóa từ bên ngoài trước khi có event modify/reload.
        fs::remove_file(&active).expect("remove active file");

        let created_path = root.join("src/created_after_delete.rs");
        fs::write(&created_path, "pub fn created() {}\n").expect("write created file");
        let report = state
            .apply_external_file_events(&[
                FileSystemEvent {
                    kind: FileSystemChangeKind::Modify,
                    path: active.clone(),
                    new_path: None,
                },
                FileSystemEvent {
                    kind: FileSystemChangeKind::Create,
                    path: created_path.clone(),
                    new_path: None,
                },
            ])
            .expect("apply external events should not fail");

        assert!(report.workspace_reloaded);
        assert!(state.set_command_palette_results(
            CommandPaletteMode::FilePicker,
            "created",
            vec![CommandPaletteItem::file_match(
                "src/created_after_delete.rs".to_string(),
                created_path.clone(),
            )],
        ));
        assert!(
            state
                .file_picker_results()
                .iter()
                .any(|entry| entry.relative_path.ends_with("src/created_after_delete.rs"))
        );

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn modify_event_on_missing_path_triggers_workspace_rescan_for_rename_like_flow() {
        let mut state = AppState::new(unique_temp_path("rename_like_modify"));
        let root = unique_temp_dir("rename_like_modify");
        fs::create_dir_all(root.join("src")).expect("create src");
        let old_path = root.join("src/old_name.rs");
        let new_path = root.join("src/new_name.rs");
        fs::write(&old_path, "pub fn old_name() {}\n").expect("write old");

        state
            .attach_workspace(root.clone())
            .expect("attach workspace should succeed");
        state.open_file_picker().expect("open picker");
        state
            .file_picker_append_query("old_name")
            .expect("append old_name query");
        assert!(state.set_command_palette_results(
            CommandPaletteMode::FilePicker,
            "old_name",
            vec![CommandPaletteItem::file_match(
                "src/old_name.rs".to_string(),
                old_path.clone(),
            )],
        ));
        assert!(
            state
                .file_picker_results()
                .iter()
                .any(|entry| entry.relative_path.ends_with("src/old_name.rs"))
        );

        fs::rename(&old_path, &new_path).expect("rename file");
        let report = state
            .apply_external_file_events(&[FileSystemEvent {
                kind: FileSystemChangeKind::Modify,
                path: old_path.clone(),
                new_path: None,
            }])
            .expect("apply external rename-like modify");

        assert!(report.workspace_reloaded);
        assert!(state.set_command_palette_results(
            CommandPaletteMode::FilePicker,
            "old_name",
            vec![CommandPaletteItem::file_match(
                "src/new_name.rs".to_string(),
                new_path.clone(),
            )],
        ));
        assert!(
            state
                .file_picker_results()
                .iter()
                .any(|entry| entry.relative_path.ends_with("src/new_name.rs"))
        );
        assert!(
            state
                .file_picker_results()
                .iter()
                .all(|entry| !entry.relative_path.ends_with("src/old_name.rs"))
        );

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn in_file_search_collects_matches_and_wraps_navigation() {
        let mut state = AppState::from_text(unique_temp_path("search"), "alpha beta alpha");

        assert!(state.set_in_file_search_query("alpha"));
        assert_eq!(state.search_highlights().len(), 2);
        assert_eq!(state.last_search_query(), "alpha");
        assert_eq!(state.active_search_match_position(), Some((1, 2)));

        assert!(state.search_next());
        assert_eq!(state.cursor_char_idx(), 11);
        assert_eq!(state.active_search_match_position(), Some((2, 2)));

        assert!(state.search_prev());
        assert_eq!(state.cursor_char_idx(), 0);
        assert_eq!(state.active_search_match_position(), Some((1, 2)));
    }

    #[test]
    fn search_word_under_cursor_uses_whole_word_matches() {
        let mut state = AppState::from_text(unique_temp_path("star"), "foo foobar foo");

        assert!(state.search_word_under_cursor());
        assert_eq!(state.last_search_query(), "foo");
        assert_eq!(state.search_highlights().len(), 2);
        assert_eq!(state.cursor_char_idx(), 11);
        assert_eq!(state.active_search_match_position(), Some((2, 2)));
    }

    #[test]
    fn clear_search_highlights_resets_query_and_matches() {
        let mut state = AppState::from_text(unique_temp_path("clear_search"), "alpha beta alpha");

        assert!(state.set_in_file_search_query("alpha"));
        assert_eq!(state.search_highlights().len(), 2);
        assert_eq!(state.active_search_match_position(), Some((1, 2)));

        assert!(state.clear_search_highlights());
        assert!(state.last_search_query().is_empty());
        assert!(state.search_highlights().is_empty());
        assert_eq!(state.active_search_match_position(), None);
        assert!(!state.clear_search_highlights());
    }

    #[test]
    fn active_search_match_position_tracks_next_match_from_intermediate_cursor() {
        let mut state = AppState::from_text(
            unique_temp_path("search_position"),
            "alpha beta alpha gamma alpha",
        );

        assert!(state.set_in_file_search_query("alpha"));
        assert!(state.move_cursor_to_char_idx(8));

        assert_eq!(state.active_search_match_position(), Some((2, 3)));

        assert!(state.search_next());
        assert_eq!(state.active_search_match_position(), Some((2, 3)));

        assert!(state.search_next());
        assert_eq!(state.active_search_match_position(), Some((3, 3)));
    }

    // ── find_text_object_bounds tests ────────────────────────────────────────

    #[test]
    fn text_object_select_enters_visual_mode() {
        let mut s = AppState::from_text(std::path::PathBuf::from("t.txt"), "foo(bar)");
        s.cursor_char_idx = 4; // trên 'b'
        // Bắt đầu từ Normal mode
        assert_eq!(s.current_mode(), EditorMode::Normal);
        let ok = s.select_text_object(TextObjectModifier::Inner, TextObjectKind::Bracket('(', ')'));
        assert!(ok);
        assert_eq!(s.current_mode(), EditorMode::Visual);
        // anchor nên là idx 4 ('b'), focus là idx 6 ('r')
        assert_eq!(s.selection_anchor_char_idx, Some(4));
        assert_eq!(s.cursor_char_idx, 6);
    }

    #[test]
    fn text_object_delete_removes_inner() {
        let mut s = AppState::from_text(std::path::PathBuf::from("t.txt"), "foo(bar)end");
        s.cursor_char_idx = 5; // 'a' inside parens
        let ok = s.delete_text_object(TextObjectModifier::Inner, TextObjectKind::Bracket('(', ')'));
        assert!(ok);
        // "foo()end" phải còn lại
        assert_eq!(s.text_string(), "foo()end");
        assert_eq!(s.cursor_char_idx, 4); // cursor dừng ở chỗ xóa
    }

    #[test]
    fn open_file_reveals_active_path_in_workspace_tree() {
        let mut state = AppState::new(unique_temp_path("workspace_reveal_on_open"));
        let root = unique_temp_dir("workspace_reveal_on_open");
        let nested_dir = root.join("src/ui");
        let active = nested_dir.join("tabs.rs");
        fs::create_dir_all(&nested_dir).expect("create nested dirs");
        fs::write(&active, "pub fn tabs() {}\n").expect("write active file");
        let canonical_root = root.canonicalize().expect("canonical root");
        let canonical_nested_dir = canonical_root.join("src/ui");
        let canonical_active = canonical_nested_dir.join("tabs.rs");

        state
            .attach_workspace(root.clone())
            .expect("attach workspace should succeed");
        state.open_file(active.clone()).expect("open active file");

        assert_eq!(
            state.workspace_selected_path(),
            Some(canonical_active.as_path())
        );
        assert!(state.workspace_is_expanded(&canonical_root.join("src")));
        assert!(state.workspace_is_expanded(&canonical_nested_dir));

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn references_buffer_tracks_selection_and_origin() {
        let mut state = AppState::new(unique_temp_path("references_buffer_state"));
        let first_path = PathBuf::from("/tmp/refs/a.rs");
        let second_path = PathBuf::from("/tmp/refs/b.rs");
        let origin_path = PathBuf::from("/tmp/origin.rs");
        let items = vec![
            ReferencesBufferItem {
                path: first_path.clone(),
                relative_path: "src/a.rs".to_string(),
                line: 10,
                column: 4,
                summary: "first reference".to_string(),
            },
            ReferencesBufferItem {
                path: second_path.clone(),
                relative_path: "src/b.rs".to_string(),
                line: 20,
                column: 7,
                summary: "second reference".to_string(),
            },
        ];

        let opened_index = state
            .open_references_buffer("References (2)", Some(origin_path.clone()), 6, items)
            .expect("references buffer should open");

        assert_eq!(state.active_buffer_index(), Some(opened_index));
        assert!(state.active_buffer_is_references());
        assert_eq!(state.active_filetype_label(), "References");
        assert_eq!(
            state
                .selected_reference_item()
                .map(|item| item.path.as_path()),
            Some(first_path.as_path())
        );
        assert_eq!(
            state.active_references_origin(),
            Some((origin_path.clone(), 6))
        );

        assert!(state.references_select_next());
        assert_eq!(
            state
                .selected_reference_item()
                .map(|item| item.path.as_path()),
            Some(second_path.as_path())
        );

        assert!(state.references_select_next());
        assert_eq!(
            state
                .selected_reference_item()
                .map(|item| item.path.as_path()),
            Some(first_path.as_path())
        );

        assert!(state.references_select_prev());
        assert_eq!(
            state
                .selected_reference_item()
                .map(|item| item.path.as_path()),
            Some(second_path.as_path())
        );

        assert_eq!(
            state
                .save_file()
                .expect_err("references buffer cannot be saved"),
            "cannot save references buffer"
        );
    }

    #[test]
    fn pending_references_buffer_accepts_async_results_and_preview() {
        let mut state = AppState::new(unique_temp_path("pending_references_buffer"));
        let origin_path = PathBuf::from("/tmp/origin.rs");
        let item_path = PathBuf::from("/tmp/refs/a.rs");
        let request_id = 77;

        state.open_pending_references_buffer(
            "References",
            Some(origin_path.clone()),
            8,
            request_id,
        );
        assert!(state.active_buffer_is_references());
        let loading = state
            .active_references_buffer()
            .expect("references buffer should be active");
        assert!(loading.loading);
        assert_eq!(loading.pending_request_id, Some(request_id));
        assert!(loading.items.is_empty());

        assert!(state.finish_pending_references_buffer(
            request_id,
            "References (2)",
            vec![
                ReferencesBufferItem {
                    path: item_path.clone(),
                    relative_path: "src/a.rs".to_string(),
                    line: 10,
                    column: 4,
                    summary: "Ln 11, Col 5".to_string(),
                },
                ReferencesBufferItem {
                    path: PathBuf::from("/tmp/refs/b.rs"),
                    relative_path: "src/b.rs".to_string(),
                    line: 20,
                    column: 2,
                    summary: "Ln 21, Col 3".to_string(),
                },
            ],
        ));

        assert_eq!(
            state
                .selected_reference_item()
                .map(|item| item.path.as_path()),
            Some(item_path.as_path())
        );
        let loaded = state
            .active_references_buffer()
            .expect("references buffer should stay active");
        assert!(!loaded.loading);
        assert_eq!(loaded.pending_request_id, None);
        assert!(loaded.preview_lines.is_empty());

        assert!(state.set_active_references_preview(
            vec![FilePreviewLine {
                line_number: 11,
                text: "call()".to_string(),
                is_target: true,
            }],
            String::new(),
            Vec::new(),
        ));
        assert_eq!(
            state
                .active_references_buffer()
                .expect("references buffer")
                .preview_lines
                .len(),
            1
        );

        assert!(state.references_select_next());
        assert!(
            state
                .active_references_buffer()
                .expect("references buffer")
                .preview_lines
                .is_empty()
        );
    }

    #[test]
    fn failing_pending_references_buffer_surfaces_status() {
        let mut state = AppState::new(unique_temp_path("pending_references_failure"));

        state.open_pending_references_buffer("References", None, 0, 91);

        assert!(state.fail_pending_references_buffer(91, "No references found"));

        let references = state
            .active_references_buffer()
            .expect("references buffer should stay active");
        assert!(!references.loading);
        assert_eq!(references.title, "References (0)");
        assert_eq!(
            references.status_message.as_deref(),
            Some("No references found")
        );
        assert!(references.items.is_empty());
    }

    #[test]
    fn completion_prefix_info_stops_at_member_access_boundary() {
        let state = AppState::from_text(unique_temp_path("completion_prefix"), "MessageManager.ge");
        let info = state.completion_prefix_info_at(0, "MessageManager.ge".chars().count());

        assert_eq!(info.start_col, "MessageManager.".chars().count());
        assert_eq!(info.prefix, "ge");
    }

    #[test]
    fn replace_completion_prefix_at_cursor_deletes_prefix_then_inserts_item() {
        let mut state =
            AppState::from_text(unique_temp_path("completion_replace"), "MessageManager.ge");
        assert!(state.jump_to_line_and_column(0, "MessageManager.ge".chars().count()));

        assert!(state.replace_completion_prefix_at_cursor(2, "getInstance()"));
        assert_eq!(state.text_string(), "MessageManager.getInstance()");
        assert_eq!(
            state.cursor_char_idx(),
            "MessageManager.getInstance()".chars().count()
        );
        assert!(state.undo());
        assert_eq!(state.text_string(), "MessageManager.ge");
    }

    #[test]
    fn committed_history_transaction_stores_single_delta() {
        let mut state = AppState::from_text(unique_temp_path("history_delta"), "hello world");
        assert!(state.jump_to_line_and_column(0, "hello ".chars().count()));

        assert!(state.apply_delete("hello ".chars().count(), "world".chars().count()));
        assert!(state.insert_text_at_cursor("rust"));
        assert!(state.commit_transaction());

        assert_eq!(state.history.undo_stack.len(), 1);
        let edit = &state.history.undo_stack[0].edit;
        assert_eq!(edit.start_char_idx, "hello ".chars().count());
        assert_eq!(edit.deleted_text, "world");
        assert_eq!(edit.inserted_text, "rust");
        assert!(state.undo());
        assert_eq!(state.text_string(), "hello world");
        assert!(state.redo());
        assert_eq!(state.text_string(), "hello rust");
    }

    #[test]
    fn file_history_picker_uses_delta_label_and_preview() {
        let file_path = unique_temp_path("history_picker_delta");
        let mut state = AppState::new(file_path.clone());
        state.active_file = Some(file_path);

        assert!(state.insert_text_at_cursor("alpha"));
        assert!(state.commit_transaction());

        let items = state.file_history_picker_items();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].label, "[+ alpha]");
        assert_eq!(
            items[0].tone,
            crate::app::command_palette::CommandPaletteItemTone::Added
        );

        assert!(state.begin_file_history_preview_session());
        assert!(state.preview_file_history_index(0));
        let (_lines, preview_text) = state
            .build_file_history_diff_preview()
            .expect("delta preview");
        assert!(preview_text.contains("+++ inserted"));
        assert!(preview_text.contains("+ alpha"));
    }

    #[test]
    fn file_history_preview_replays_on_temp_rope_without_rewriting_history() {
        let file_path = unique_temp_path("history_temp_preview");
        let mut state = AppState::new(file_path.clone());
        state.active_file = Some(file_path);

        assert!(state.insert_text_at_cursor("old"));
        assert!(state.commit_transaction());
        assert!(state.insert_text_at_cursor(" new"));
        assert!(state.commit_transaction());
        assert_eq!(state.text_string(), "old new");
        assert_eq!(state.history.undo_stack.len(), 2);

        assert!(state.begin_file_history_preview_session());
        assert!(state.preview_file_history_index(0));

        assert_eq!(state.text_string(), "old");
        assert_eq!(state.history.undo_stack.len(), 2);
        assert!(state.history.redo_stack.is_empty());

        assert!(state.cancel_file_history_preview());
        assert_eq!(state.text_string(), "old new");
        assert_eq!(state.history.undo_stack.len(), 2);
        assert!(state.history.redo_stack.is_empty());
    }
}
