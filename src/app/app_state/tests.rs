use super::*;

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        fs,
        path::PathBuf,
        time::{Duration, SystemTime, UNIX_EPOCH},
    };

    use crate::app::command_palette::{CommandPaletteItem, CommandPaletteMode};
    use crate::async_runtime::message::{
        ExternalFileRead, FilePreviewLine, FileSystemChangeKind, FileSystemEvent,
    };
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
    fn make_diagnostic(line: u32, message: &str) -> crate::async_runtime::message::LspDiagnostic {
        use crate::async_runtime::message::{LspDiagnostic, LspPosition, LspRange};
        LspDiagnostic {
            range: LspRange {
                start: LspPosition { line, character: 0 },
                end: LspPosition { line, character: 1 },
            },
            severity: Some(1),
            code: None,
            source: None,
            message: message.to_string(),
            tags: Vec::new(),
        }
    }

    #[test]
    fn diagnostics_from_two_servers_merge_and_clear_independently() {
        let mut state = AppState::new(unique_temp_path("diag_merge_state"));
        let path = unique_temp_path("diag_merge_file");

        // pyright and ruff publish independently for the same file.
        assert!(state.set_file_diagnostics(
            path.clone(),
            "pyright-langserver".to_string(),
            vec![make_diagnostic(1, "type error")],
        ));
        assert!(state.set_file_diagnostics(
            path.clone(),
            "ruff".to_string(),
            vec![make_diagnostic(5, "unused import")],
        ));
        assert_eq!(
            state.diagnostics_for_path(&path).map(<[_]>::len),
            Some(2),
            "both servers' diagnostics should be visible (no clobber)"
        );

        // ruff clears its set; pyright's must remain.
        assert!(state.set_file_diagnostics(path.clone(), "ruff".to_string(), Vec::new()));
        let after = state.diagnostics_for_path(&path).unwrap_or(&[]);
        assert_eq!(after.len(), 1);
        assert_eq!(after[0].message, "type error");

        // Re-publishing the identical set from pyright is a no-op (no redraw).
        assert!(!state.set_file_diagnostics(
            path.clone(),
            "pyright-langserver".to_string(),
            vec![make_diagnostic(1, "type error")],
        ));

        // pyright clears too -> the file drops out entirely.
        assert!(state.set_file_diagnostics(
            path.clone(),
            "pyright-langserver".to_string(),
            Vec::new()
        ));
        assert!(state.diagnostics_for_path(&path).is_none());
    }

    fn read_external_files(paths: &[PathBuf]) -> Vec<ExternalFileRead> {
        paths
            .iter()
            .map(|path| ExternalFileRead {
                path: path.clone(),
                content: fs::read_to_string(path).ok(),
                modified_time: fs::metadata(path).and_then(|m| m.modified()).ok(),
            })
            .collect()
    }

    impl AppState {
        /// Test driver for the async external-change pipeline: runs phase 1
        /// (event triage), a synchronous stand-in for the worker (file reads +
        /// workspace rescan), then phase 2 (content apply) — and merges the
        /// reports so existing assertions keep working.
        fn apply_external_file_events_for_test(
            &mut self,
            events: &[FileSystemEvent],
        ) -> Result<ExternalChangeReport, String> {
            let mut report = self.apply_external_file_events(events)?;
            if report.workspace_rescan_needed
                && let Some((root, rules, options)) = self.workspace_rescan_request_params()
            {
                let scanner = crate::workspace::scanner::WorkspaceScanner::new(rules, options);
                if let Ok(nodes) = scanner.scan(&root) {
                    let _ = self.apply_workspace_rescan(&root, nodes)?;
                    report.workspace_reloaded = true;
                }
            }
            let files = read_external_files(&report.pending_reload_paths);
            let applied = self.apply_external_file_contents(&files);
            report.active_file_reloaded |= applied.active_file_reloaded;
            report.conflict_detected |= applied.conflict_detected;
            if report.conflict_path.is_none() {
                report.conflict_path = applied.conflict_path;
            }
            report.workspace_reloaded |= !applied.inactive_reloaded_paths.is_empty();
            report
                .inactive_reloaded_paths
                .extend(applied.inactive_reloaded_paths);
            report.notices.extend(applied.notices);
            Ok(report)
        }
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
    fn folded_ranges_hide_lines_after_marker_through_end_line() {
        let mut state = AppState::from_text(unique_temp_path("fold_map"), "a\nb\nc\nd\ne");
        state.folded_ranges = vec![(1, 3)];

        assert!(!state.is_line_folded(1));
        assert!(state.is_line_folded(2));
        assert!(state.is_line_folded(3));
        assert_eq!(state.folded_line_count_at_marker(1), Some(2));
        assert_eq!(state.compute_visible_line_map(), vec![0, 1, 4]);
        assert_eq!(state.visible_line_count(), 3);
        assert_eq!(state.logical_to_visible_line(1), Some(1));
        assert_eq!(state.logical_to_visible_line(2), None);
        assert_eq!(state.logical_to_visible_line(4), Some(2));
    }

    #[test]
    fn vertical_movement_skips_folded_hidden_lines() {
        let mut state = AppState::from_text(unique_temp_path("fold_move"), "a\nb\nc\nd\ne");
        state.folded_ranges = vec![(1, 3)];
        assert!(state.jump_to_line(1));

        state.move_down();
        assert_eq!(state.cursor_line_col(), (4, 0));

        state.move_up();
        assert_eq!(state.cursor_line_col(), (1, 0));
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
    fn match_bracket_jumps_between_nested_pairs_and_sets_ripple() {
        let mut state = AppState::from_text(
            unique_temp_path("match_bracket_nested"),
            "fn main() { call([1]); }",
        );

        assert!(state.jump_to_line_col(0, 10));
        assert!(state.match_bracket());

        assert_eq!(state.cursor_char_idx(), 23);
        assert_eq!(state.bracket_ripple_pos(), Some(23));
        assert_eq!(state.matched_bracket_pos(), Some(10));

        assert!(state.match_bracket());

        assert_eq!(state.cursor_char_idx(), 10);
        assert_eq!(state.bracket_ripple_pos(), Some(10));
        assert_eq!(state.matched_bracket_pos(), Some(23));
    }

    #[test]
    fn match_bracket_ignores_non_bracket_cursor_and_unmatched_bracket() {
        let mut state = AppState::from_text(unique_temp_path("match_bracket_none"), "abc {");

        assert!(!state.match_bracket());
        assert_eq!(state.cursor_char_idx(), 0);
        assert_eq!(state.bracket_ripple_pos(), None);
        assert_eq!(state.matched_bracket_pos(), None);

        assert!(state.jump_to_line_col(0, 4));
        assert!(!state.match_bracket());
        assert_eq!(state.cursor_char_idx(), 4);
        assert_eq!(state.bracket_ripple_pos(), None);
        assert_eq!(state.matched_bracket_pos(), None);
    }

    #[test]
    fn matched_bracket_highlight_tracks_only_cursor_on_brackets() {
        let mut state = AppState::from_text(unique_temp_path("match_bracket_highlight"), "([x])");

        assert!(state.refresh_matched_bracket());
        assert_eq!(state.matched_bracket_pos(), Some(4));

        assert!(state.jump_to_line_col(0, 1));
        assert_eq!(state.matched_bracket_pos(), Some(3));

        assert!(state.jump_to_line_col(0, 2));
        assert_eq!(state.matched_bracket_pos(), None);
    }

    #[test]
    fn help_buffer_uses_config_driven_keymap_content() {
        let bindings = vec![
            KeyBinding {
                key: "cmd+p".to_string(),
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
                    .any(|entry| entry.keys == vec!["cmd+p"] && entry.label == "Open file picker")
        }));
        assert!(
            help.lines
                .iter()
                .any(|line| line.contains("cmd+p") && line.contains("Open file picker"))
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
    fn help_buffer_includes_every_configured_mode() {
        let bindings = ["normal", "resize", "preview", "visual_block"]
            .into_iter()
            .map(|mode| KeyBinding {
                key: "j".to_string(),
                mode: Some(mode.to_string()),
                command: "editor.move_down".to_string(),
            })
            .collect::<Vec<_>>();

        let help = super::HelpState::from_bindings("test-profile", "test.toml", &bindings);
        let titles = help
            .sections
            .iter()
            .map(|section| section.title.as_str())
            .collect::<Vec<_>>();

        assert!(titles.contains(&"NORMAL"));
        assert!(titles.contains(&"RESIZE"));
        assert!(titles.contains(&"PREVIEW"));
        assert!(titles.contains(&"VISUAL BLOCK"));
    }

    #[test]
    fn opening_help_buffer_reuses_the_existing_tab() {
        let mut state = AppState::new(unique_temp_path("reuse_help_buffer"));

        let first_index = state.open_help_buffer();
        let buffer_count = state.buffers().len();
        let second_index = state.open_help_buffer();

        assert_eq!(second_index, first_index);
        assert_eq!(state.buffers().len(), buffer_count);
        assert_eq!(state.active_buffer_index(), Some(first_index));
    }

    #[test]
    fn help_scroll_is_clamped_to_the_rendered_content() {
        let mut state = AppState::new(unique_temp_path("bounded_help_scroll"));
        state.open_help_buffer();
        state.set_help_max_scroll(250.0);

        state.help_scroll_down(400.0);
        assert_eq!(
            state.active_help_buffer().map(|help| help.scroll_y),
            Some(250.0)
        );

        state.set_help_max_scroll(100.0);
        assert_eq!(
            state.active_help_buffer().map(|help| help.scroll_y),
            Some(100.0)
        );

        state.help_scroll_up(150.0);
        assert_eq!(
            state.active_help_buffer().map(|help| help.scroll_y),
            Some(0.0)
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
    fn backspace_between_empty_backticks_deletes_both_chars() {
        let mut state = AppState::from_text(unique_temp_path("smart_backspace_backticks"), "``");
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
    fn switching_back_to_text_buffer_restores_cursor_and_scroll_state() {
        let mut state = AppState::new(unique_temp_path("buffer_view_restore"));
        let root = unique_temp_dir("buffer_view_restore");
        fs::create_dir_all(&root).expect("create buffer view root");
        let file_a = root.join("a.rs");
        let file_b = root.join("b.rs");
        fs::write(&file_a, "zero\none\ntwo\nthree\nfour\n").expect("write a");
        fs::write(&file_b, "alpha\nbeta\ngamma\n").expect("write b");

        state.open_file(file_a.clone()).expect("open a");
        state.cursor_char_idx = state.text.line_to_char(3) + 2;
        state.target_col = 2;
        state.selection_anchor_char_idx = Some(state.text.line_to_char(2));
        state.visual_line_mode = true;
        state.target_scroll_y = 3.0;
        state.current_scroll_y = 2.5;
        state.scroll_column = 4;

        state.open_file(file_b).expect("open b");
        assert!(state.active_file().expect("active file").ends_with("b.rs"));

        assert!(state.buffer_prev().expect("switch back to a"));
        assert!(state.active_file().expect("active file").ends_with("a.rs"));
        assert_eq!(state.cursor_char_idx, state.text.line_to_char(3) + 2);
        assert_eq!(state.target_col, 2);
        assert_eq!(
            state.selection_anchor_char_idx,
            Some(state.text.line_to_char(2))
        );
        assert!(state.visual_line_mode);
        assert_eq!(state.target_scroll_y, 3.0);
        assert_eq!(state.current_scroll_y, 2.5);
        assert_eq!(state.scroll_column, 4);

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn switching_text_buffers_clears_stale_semantic_symbol_highlights() {
        let mut state = AppState::new(unique_temp_path("buffer_highlight_clear"));
        let root = unique_temp_dir("buffer_highlight_clear");
        fs::create_dir_all(&root).expect("create buffer highlight root");
        let file_a = root.join("a.rs");
        let file_b = root.join("b.rs");
        fs::write(&file_a, "let old_cursor = 1;\n").expect("write a");
        fs::write(&file_b, "let new_cursor = 2;\n").expect("write b");

        state.open_file(file_a.clone()).expect("open a");
        assert!(state.set_semantic_symbol_highlights(vec![(4, 14)]));
        assert_eq!(state.semantic_symbol_highlights(), &[(4, 14)]);

        state.open_file(file_b.clone()).expect("open b");
        assert!(state.active_file().expect("active file").ends_with("b.rs"));
        assert!(state.semantic_symbol_highlights().is_empty());

        assert!(state.set_semantic_symbol_highlights(vec![(4, 14)]));
        assert!(state.buffer_prev().expect("switch back to a"));
        assert!(state.active_file().expect("active file").ends_with("a.rs"));
        assert!(state.semantic_symbol_highlights().is_empty());

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn switching_buffers_keeps_unsaved_dirty_text_until_explicit_save() {
        let mut state = AppState::new(unique_temp_path("dirty_buffer_switch"));
        let root = unique_temp_dir("dirty_buffer_switch");
        fs::create_dir_all(&root).expect("create root");
        let file_a = root.join("a.rs");
        let file_b = root.join("b.rs");
        fs::write(&file_a, "alpha\n").expect("write a");
        fs::write(&file_b, "beta\n").expect("write b");

        state.open_file(file_a.clone()).expect("open a");
        state.cursor_char_idx = state.text.len_chars();
        assert!(state.insert_text_at_cursor("dirty"));
        let _ = state.commit_transaction();
        assert!(state.is_dirty());
        assert_eq!(state.text_string(), "alpha\ndirty");

        state.open_file(file_b.clone()).expect("open b");
        assert!(state.active_file().expect("active file").ends_with("b.rs"));
        assert!(!state.is_dirty());

        state.open_file(file_a.clone()).expect("back to a");
        assert!(state.active_file().expect("active file").ends_with("a.rs"));
        assert_eq!(state.text_string(), "alpha\ndirty");
        assert!(state.is_dirty());

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn closing_and_reopening_buffer_restores_undo_history_within_app_session() {
        let mut state = AppState::new(unique_temp_path("closed_buffer_undo"));
        let root = unique_temp_dir("closed_buffer_undo");
        fs::create_dir_all(&root).expect("create root");
        let file_path = root.join("history.rs");
        fs::write(&file_path, "seed").expect("write file");

        state.open_file(file_path.clone()).expect("open file");
        state.cursor_char_idx = state.text.len_chars();
        assert!(state.insert_text_at_cursor("_edit"));
        assert!(state.commit_transaction());
        assert_eq!(state.text_string(), "seed_edit");

        assert!(state.close_current_buffer().expect("close active buffer"));
        assert!(state.active_file().is_none());

        state.open_file(file_path.clone()).expect("reopen file");
        assert_eq!(state.text_string(), "seed_edit");
        assert!(state.is_dirty());
        assert!(state.undo());
        assert_eq!(state.text_string(), "seed");

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
            .apply_external_file_events_for_test(&[FileSystemEvent {
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
    fn external_rename_without_new_path_on_existing_file_reloads_like_modify() {
        // macOS FSEvents splits a rename into two events with one path each
        // (new_path = None). Atomic saves (write temp + rename over the real
        // file) arrive as Rename{new_path: None} on a path that still exists —
        // that must reload immediately, not wait for the 3s poll.
        let save_path = unique_temp_path("external_rename_modify");
        let mut state = AppState::new(save_path);
        let root = unique_temp_dir("external_rename_modify");
        fs::create_dir_all(root.join("src")).expect("create src");
        let active = root.join("src/main.rs");
        fs::write(&active, "fn main() {}\n").expect("write initial");

        state
            .attach_workspace(root.clone())
            .expect("attach workspace should succeed");
        state.open_file(active.clone()).expect("open active file");

        fs::write(&active, "fn main() { println!(\"atomic save\"); }\n")
            .expect("write replacement content");
        let report = state
            .apply_external_file_events_for_test(&[FileSystemEvent {
                kind: FileSystemChangeKind::Rename,
                path: active.clone(),
                new_path: None,
            }])
            .expect("apply external rename");

        assert!(report.active_file_reloaded);
        assert!(state.preview(64).contains("atomic save"));

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn external_modify_of_inactive_buffer_is_reported_for_lsp_sync() {
        let save_path = unique_temp_path("external_inactive_lsp");
        let mut state = AppState::new(save_path);
        let root = unique_temp_dir("external_inactive_lsp");
        fs::create_dir_all(root.join("src")).expect("create src");
        let first = root.join("src/lib.rs");
        let second = root.join("src/main.rs");
        fs::write(&first, "pub fn lib() {}\n").expect("write first");
        fs::write(&second, "fn main() {}\n").expect("write second");

        state
            .attach_workspace(root.clone())
            .expect("attach workspace should succeed");
        state.open_file(first.clone()).expect("open first file");
        state.open_file(second.clone()).expect("open second file");

        fs::write(&first, "pub fn lib() { /* edited externally */ }\n")
            .expect("rewrite inactive file");
        let report = state
            .apply_external_file_events_for_test(&[FileSystemEvent {
                kind: FileSystemChangeKind::Modify,
                path: first.clone(),
                new_path: None,
            }])
            .expect("apply external modify on inactive buffer");

        assert!(!report.active_file_reloaded);
        assert!(
            report
                .inactive_reloaded_paths
                .iter()
                .any(|path| path.ends_with("src/lib.rs")),
            "inactive reload must be reported so the shell can re-sync the LSP overlay"
        );
        assert!(
            state
                .buffer_text_for_path(&first)
                .is_some_and(|text| text.contains("edited externally"))
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
            .apply_external_file_events_for_test(&[FileSystemEvent {
                kind: FileSystemChangeKind::Modify,
                path: active.clone(),
                new_path: None,
            }])
            .expect("apply external clean");

        assert!(report.active_file_reloaded);
        assert!(state.preview(48).contains("reload"));

        state.insert_char('x');
        let dirty_report = state
            .apply_external_file_events_for_test(&[FileSystemEvent {
                kind: FileSystemChangeKind::Modify,
                path: active.clone(),
                new_path: None,
            }])
            .expect("apply external dirty");
        assert!(dirty_report.conflict_detected);
        assert!(
            dirty_report
                .conflict_path
                .as_ref()
                .is_some_and(|path| path.ends_with("src/main.rs"))
        );
        assert!(state.external_conflict_message().is_some());

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn test_inactive_buffer_reloads_when_clean_externally_modified() {
        let save_path = unique_temp_path("external_inactive_modify");
        let mut state = AppState::new(save_path);
        let root = unique_temp_dir("external_inactive_modify");
        fs::create_dir_all(root.join("src")).expect("create src");
        let inactive = fs::canonicalize(root.join("src"))
            .expect("canonicalize src")
            .join("inactive.rs");
        let active = fs::canonicalize(root.join("src"))
            .expect("canonicalize src")
            .join("active.rs");
        fs::write(&inactive, "fn inactive() {}\n").expect("write inactive");
        fs::write(&active, "fn active() {}\n").expect("write active");

        state
            .attach_workspace(root.clone())
            .expect("attach workspace");
        state.open_file(inactive.clone()).expect("open inactive");
        state.open_file(active.clone()).expect("open active");

        fs::write(&inactive, "fn inactive() { println!(\"reload\"); }\n")
            .expect("write modified inactive");
        let report = state
            .apply_external_file_events_for_test(&[FileSystemEvent {
                kind: FileSystemChangeKind::Modify,
                path: inactive.clone(),
                new_path: None,
            }])
            .expect("apply external inactive modify");

        assert!(report.workspace_reloaded);
        assert!(!report.active_file_reloaded);

        let inactive_entry = state
            .buffers()
            .iter()
            .find(|b| matches!(&b.content, BufferContent::Text(t) if t.path == inactive))
            .unwrap();
        if let BufferContent::Text(ref text_buf) = inactive_entry.content {
            assert_eq!(
                text_buf.in_memory_text.as_ref().unwrap().to_string(),
                "fn inactive() { println!(\"reload\"); }\n"
            );
            assert!(!text_buf.missing_on_disk);
            assert!(!text_buf.dirty);
        } else {
            panic!("expected text buffer");
        }

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn test_inactive_buffer_marked_missing_when_externally_deleted() {
        let save_path = unique_temp_path("external_inactive_delete");
        let mut state = AppState::new(save_path);
        let root = unique_temp_dir("external_inactive_delete");
        fs::create_dir_all(root.join("src")).expect("create src");
        let inactive = fs::canonicalize(root.join("src"))
            .expect("canonicalize src")
            .join("inactive.rs");
        let active = fs::canonicalize(root.join("src"))
            .expect("canonicalize src")
            .join("active.rs");
        fs::write(&inactive, "fn inactive() {}\n").expect("write inactive");
        fs::write(&active, "fn active() {}\n").expect("write active");

        state
            .attach_workspace(root.clone())
            .expect("attach workspace");
        state.open_file(inactive.clone()).expect("open inactive");
        state.open_file(active.clone()).expect("open active");

        fs::remove_file(&inactive).expect("remove inactive file");
        let report = state
            .apply_external_file_events_for_test(&[FileSystemEvent {
                kind: FileSystemChangeKind::Delete,
                path: inactive.clone(),
                new_path: None,
            }])
            .expect("apply external inactive delete");

        assert!(report.workspace_reloaded);

        let inactive_entry = state
            .buffers()
            .iter()
            .find(|b| matches!(&b.content, BufferContent::Text(t) if t.path == inactive))
            .unwrap();
        if let BufferContent::Text(ref text_buf) = inactive_entry.content {
            assert!(text_buf.missing_on_disk);
        } else {
            panic!("expected text buffer");
        }

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

        // Self-save THẬT: đĩa == bộ nhớ (chính nội dung vừa lưu). Modify echo do OS
        // bắn ra phải bị bỏ qua, không reload, không nhảy con trỏ.
        let report = state
            .apply_external_file_events_for_test(&[FileSystemEvent {
                kind: FileSystemChangeKind::Modify,
                path: file_path.clone(),
                new_path: None,
            }])
            .expect("apply modify event");

        assert!(!report.active_file_reloaded);
        assert_eq!(state.cursor_char_idx(), cursor_before);

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn external_edit_within_self_save_window_still_reloads() {
        // #3: edit NGOÀI thật xảy ra ngay sau khi save (trong cửa sổ debounce) KHÔNG
        // được nuốt — vì nội dung đĩa khác bộ nhớ thì đó là thay đổi thật, phải reload.
        let root = unique_temp_dir("external_edit_in_window");
        fs::create_dir_all(&root).expect("create temp dir");
        let file_path = root.join("main.rs");
        fs::write(&file_path, "one\ntwo\nthree\n").expect("write initial");

        let mut state = AppState::new(unique_temp_path("external_edit_in_window_fallback"));
        state.open_file(file_path.clone()).expect("open file");
        state.save_file().expect("save file");

        // Ghi nội dung khác ngay lập tức (vẫn trong SELF_SAVE_IGNORE_WINDOW).
        fs::write(&file_path, "external content arrived\n").expect("rewrite file quickly");

        let report = state
            .apply_external_file_events_for_test(&[FileSystemEvent {
                kind: FileSystemChangeKind::Modify,
                path: file_path.clone(),
                new_path: None,
            }])
            .expect("apply modify event");

        assert!(report.active_file_reloaded);
        assert!(state.preview(64).contains("external content"));

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

        let report = state
            .apply_external_file_events_for_test(&[FileSystemEvent {
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
            .apply_external_file_events_for_test(&[
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
            .apply_external_file_events_for_test(&[FileSystemEvent {
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
    fn toggle_collapse_expand_references_reports_collapse_and_expand_changes() {
        let mut state = AppState::new(unique_temp_path("references_collapse_toggle"));
        let first_path = PathBuf::from("/tmp/refs/a.rs");
        let second_path = PathBuf::from("/tmp/refs/b.rs");
        state
            .open_references_buffer(
                "References (2)",
                None,
                0,
                vec![
                    ReferencesBufferItem {
                        path: first_path,
                        relative_path: "src/a.rs".to_string(),
                        line: 10,
                        column: 4,
                        summary: "first reference".to_string(),
                    },
                    ReferencesBufferItem {
                        path: second_path,
                        relative_path: "src/b.rs".to_string(),
                        line: 20,
                        column: 7,
                        summary: "second reference".to_string(),
                    },
                ],
            )
            .expect("references buffer should open");

        assert!(state.toggle_collapse_expand_references());
        assert!(
            state
                .active_references_buffer()
                .expect("references buffer")
                .collapsed_paths
                .contains("src/a.rs")
        );

        assert!(state.toggle_collapse_expand_references());
        assert!(
            !state
                .active_references_buffer()
                .expect("references buffer")
                .collapsed_paths
                .contains("src/a.rs")
        );
    }

    #[test]
    fn toggle_collapse_expand_fuzzy_reports_collapse_and_expand_changes() {
        let mut state = AppState::new(unique_temp_path("fuzzy_collapse_toggle"));
        let mut fuzzy = FuzzyState::new(CommandPaletteMode::FilePicker);
        fuzzy.results = vec![
            CommandPaletteItem::file_match(
                "src/a.rs".to_string(),
                PathBuf::from("/tmp/fuzzy/src/a.rs"),
            ),
            CommandPaletteItem::file_match(
                "src/b.rs".to_string(),
                PathBuf::from("/tmp/fuzzy/src/b.rs"),
            ),
        ];
        state.buffers.push(BufferEntry {
            content: BufferContent::FuzzyPicker(fuzzy),
        });
        state.active_buffer_index = Some(0);

        assert!(state.toggle_collapse_expand_fuzzy());
        assert!(
            state
                .active_fuzzy_picker_buffer()
                .expect("fuzzy picker buffer")
                .collapsed_paths
                .contains("src")
        );

        assert!(state.toggle_collapse_expand_fuzzy());
        assert!(
            !state
                .active_fuzzy_picker_buffer()
                .expect("fuzzy picker buffer")
                .collapsed_paths
                .contains("src")
        );
    }

    #[test]
    fn toggle_collapse_expand_live_grep_uses_search_match_path_group() {
        let mut state = AppState::new(unique_temp_path("live_grep_collapse_toggle"));
        let path = PathBuf::from("/tmp/live-grep/cfl-manifest.txt");
        let other_path = PathBuf::from("/tmp/live-grep/package-lock.json");
        let mut fuzzy = FuzzyState::new(CommandPaletteMode::LiveGrep);
        fuzzy.results = vec![
            CommandPaletteItem::search_match(
                "src/games/arcade/result/model-builder/2998/type-arcade-2998.ts".to_string(),
                Some(
                    "src/games/arcade/result/model-builder/2998/type-arcade-2998.ts".to_string(),
                ),
                path.clone(),
                149,
                1,
            ),
            CommandPaletteItem::search_match(
                "src/games/arcade/result/types/2993/type-arcade-2993-result.ts".to_string(),
                Some("src/games/arcade/result/types/2993/type-arcade-2993-result.ts".to_string()),
                path.clone(),
                150,
                1,
            ),
            CommandPaletteItem::search_match(
                "\"resolved\": \"https://registry.npmjs.org/ms/-/ms-2.1.3.tgz\",".to_string(),
                Some("\"resolved\": \"https://registry.npmjs.org/ms/-/ms-2.1.3.tgz\",".to_string()),
                other_path.clone(),
                12,
                1,
            ),
        ];
        state.buffers.push(BufferEntry {
            content: BufferContent::FuzzyPicker(fuzzy),
        });
        state.active_buffer_index = Some(0);

        let group_key = path.display().to_string();
        assert!(state.toggle_collapse_expand_fuzzy());
        assert!(
            state
                .active_fuzzy_picker_buffer()
                .expect("fuzzy picker buffer")
                .collapsed_paths
                .contains(&group_key)
        );
        assert!(state.command_palette_select_next());
        assert_eq!(state.command_palette_selected_index(), 2);
        if let Some(BufferEntry {
            content: BufferContent::FuzzyPicker(fuzzy),
        }) = state.buffers.get_mut(0)
        {
            fuzzy.selected_index = 0;
        }

        assert!(state.toggle_collapse_expand_fuzzy());
        assert!(
            !state
                .active_fuzzy_picker_buffer()
                .expect("fuzzy picker buffer")
                .collapsed_paths
                .contains(&group_key)
        );
    }

    #[test]
    fn collapsing_fuzzy_group_moves_selection_to_group_header() {
        let mut state = AppState::new(unique_temp_path("fuzzy_collapse_selection"));
        let path = PathBuf::from("/tmp/live-grep/main.rs");
        let other_path = PathBuf::from("/tmp/live-grep/lib.rs");
        let mut fuzzy = FuzzyState::new(CommandPaletteMode::LiveGrep);
        fuzzy.results = vec![
            CommandPaletteItem::search_match(
                "first".to_string(),
                Some("first".to_string()),
                path.clone(),
                1,
                1,
            ),
            CommandPaletteItem::search_match(
                "second".to_string(),
                Some("second".to_string()),
                path.clone(),
                2,
                1,
            ),
            CommandPaletteItem::search_match(
                "other".to_string(),
                Some("other".to_string()),
                other_path,
                3,
                1,
            ),
        ];
        fuzzy.selected_index = 1; // second match inside the first file group
        state.buffers.push(BufferEntry {
            content: BufferContent::FuzzyPicker(fuzzy),
        });
        state.active_buffer_index = Some(0);

        assert!(state.toggle_collapse_expand_fuzzy());
        // Collapsing the group the selection lives in anchors the selection on the
        // group's first item so the highlight rides the (now header-only) group.
        assert_eq!(
            state
                .active_fuzzy_picker_buffer()
                .expect("fuzzy picker buffer")
                .selected_index,
            0
        );
    }

    #[test]
    fn collapsing_references_group_moves_selection_to_group_header() {
        let mut state = AppState::new(unique_temp_path("references_collapse_selection"));
        state
            .open_references_buffer(
                "References (3)",
                None,
                0,
                vec![
                    ReferencesBufferItem {
                        path: PathBuf::from("/tmp/refs/a.rs"),
                        relative_path: "src/a.rs".to_string(),
                        line: 10,
                        column: 4,
                        summary: "first".to_string(),
                    },
                    ReferencesBufferItem {
                        path: PathBuf::from("/tmp/refs/a.rs"),
                        relative_path: "src/a.rs".to_string(),
                        line: 22,
                        column: 4,
                        summary: "second".to_string(),
                    },
                    ReferencesBufferItem {
                        path: PathBuf::from("/tmp/refs/b.rs"),
                        relative_path: "src/b.rs".to_string(),
                        line: 5,
                        column: 7,
                        summary: "third".to_string(),
                    },
                ],
            )
            .expect("references buffer should open");

        if let Some(buffer) = state.active_references_buffer_mut() {
            buffer.selected_index = 1; // second reference inside src/a.rs
        }

        assert!(state.toggle_collapse_expand_references());
        assert_eq!(
            state
                .active_references_buffer()
                .expect("references buffer")
                .selected_index,
            0
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

    // ── HTML auto-close tag ────────────────────────────────────────────────────

    fn html_state_with(content: &str) -> AppState {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        let path = std::env::temp_dir().join(format!("netherize_autoclose_{nanos}.html"));
        std::fs::write(&path, content).expect("write temp html");
        let mut state = AppState::from_text(path.clone(), "");
        state.open_file(path).expect("open html");
        // Move cursor to end of loaded text
        state.cursor_char_idx = state.text.len_chars();
        state
    }

    #[test]
    fn html_auto_close_inserts_closing_tag_after_gt() {
        let mut state = html_state_with("<div");
        // Cursor is at end (after 'v'). Insert '>' then auto-close.
        state.insert_char('>');
        let closed = state.insert_html_auto_close_tag();
        assert!(closed, "should have inserted closing tag");
        assert_eq!(state.text_string(), "<div></div>");
        // Cursor should be between the two tags (position 5)
        assert_eq!(state.cursor_char_idx, 5);
    }

    #[test]
    fn html_auto_close_with_attributes() {
        let mut state = html_state_with(r#"<div class="foo""#);
        state.insert_char('>');
        let closed = state.insert_html_auto_close_tag();
        assert!(closed);
        assert!(state.text_string().ends_with("></div>"));
    }

    #[test]
    fn html_auto_close_skips_void_elements() {
        for void_tag in &["img", "br", "hr", "input", "meta", "link"] {
            let content = format!("<{void_tag}");
            let mut state = html_state_with(&content);
            state.insert_char('>');
            let closed = state.insert_html_auto_close_tag();
            assert!(!closed, "{void_tag} should not be auto-closed");
        }
    }

    #[test]
    fn html_auto_close_skips_self_closing_tag() {
        let mut state = html_state_with("<br /");
        state.insert_char('>');
        let closed = state.insert_html_auto_close_tag();
        assert!(!closed, "self-closing tag should be skipped");
    }

    #[test]
    fn html_auto_close_skips_closing_tag() {
        let mut state = html_state_with("<div></");
        // Simulate typing "</div>" — the `>` inside a closing tag should be ignored
        // Manually position cursor after the closing slash context
        state.insert_char('d');
        state.insert_char('i');
        state.insert_char('v');
        state.insert_char('>');
        let closed = state.insert_html_auto_close_tag();
        assert!(
            !closed,
            "closing tag sequence should not trigger auto-close"
        );
    }

    #[test]
    fn html_auto_close_does_not_apply_to_non_html_files() {
        // from_text has no active_file → extension is empty → returns false
        let mut state = AppState::from_text(PathBuf::from("test.rs"), "");
        for ch in "<div>".chars() {
            state.insert_char(ch);
        }
        state.cursor_char_idx = state.text.len_chars();
        let closed = state.insert_html_auto_close_tag();
        assert!(!closed, "Rust file should not trigger HTML auto-close");
    }

    // ── Multi-cursor tests ────────────────────────────────────────────────────

    fn setup_multi_cursor(text: &str) -> AppState {
        AppState::from_text(PathBuf::from("test.rs"), text)
    }

    /// After `c` (change), primary and virtual cursors must land at the start of
    /// their respective deleted selections, adjusted for all lower-index deletions.
    #[test]
    fn multi_cursor_change_places_cursors_at_deleted_selection_starts() {
        // "foo bar foo" — primary on first "foo" [0,3), virtual on second "foo" [8,11)
        let mut state = setup_multi_cursor("foo bar foo");

        // Put cursor on first "foo" and select it via ctrl-n.
        state.cursor_char_idx = 0;
        assert!(state.multi_cursor_add_next()); // seeds word "foo", primary sel [0,3)
        assert!(state.multi_cursor_add_next()); // adds second "foo", virtual sel [8,11)

        assert_eq!(state.current_mode(), EditorMode::MultiCursor);

        // Execute `c` (change): delete both selections, enter MultiInsert.
        assert!(state.multi_cursor_change());
        assert_eq!(state.current_mode(), EditorMode::MultiInsert);

        // Buffer is now " bar " (5 chars: space bar space, then the remaining space)
        // Actually "foo bar foo" → delete [8,11) first: "foo bar " → delete [0,3): " bar "
        // Primary cursor should be at 0 (start of where first "foo" was).
        assert_eq!(
            state.cursor_char_idx, 0,
            "primary cursor must be at position 0 after deleting first 'foo'"
        );

        // Virtual cursor: original sel_start = 8. After deleting [0,3), shift = 3.
        // Expected position = 8 - 3 = 5.
        assert_eq!(
            state.virtual_cursors().len(),
            1,
            "one virtual cursor must remain"
        );
        assert_eq!(
            state.virtual_cursors()[0].char_idx,
            5,
            "virtual cursor must be at position 5 (8 - 3) after deleting first 'foo'"
        );

        // Buffer content check: "foo bar foo" minus "foo" twice = " bar "
        assert_eq!(state.text_string(), " bar ");
    }

    /// Three occurrences — verifies each virtual cursor position is independently
    /// adjusted by the total chars deleted below it.
    #[test]
    fn multi_cursor_change_three_occurrences_cursor_positions() {
        // "ab|ab|ab" (| is a separator, word = "ab")
        // sel [0,2), [3,5), [6,8)
        let mut state = setup_multi_cursor("ab|ab|ab");

        state.cursor_char_idx = 0;
        assert!(state.multi_cursor_add_next()); // primary: "ab" at [0,2)
        assert!(state.multi_cursor_add_next()); // virtual: "ab" at [3,5)
        assert!(state.multi_cursor_add_next()); // virtual: "ab" at [6,8)

        assert!(state.multi_cursor_change());

        // Deleted [6,8) first, then [3,5), then [0,2). Buffer: "||"
        // Primary at 0: shift from ranges below 0 = 0 → pos 0
        // VC1 at orig 3: ranges below 3 → [0,2) len=2 → pos = 3 - 2 = 1
        // VC2 at orig 6: ranges below 6 → [0,2) len=2 + [3,5) len=2 = 4 → pos = 6 - 4 = 2
        assert_eq!(state.cursor_char_idx, 0);
        assert_eq!(state.virtual_cursors()[0].char_idx, 1);
        assert_eq!(state.virtual_cursors()[1].char_idx, 2);
        assert_eq!(state.text_string(), "||");
    }

    /// `d` (delete) shares the same deletion logic — cursors should also be correct.
    #[test]
    fn multi_cursor_delete_places_cursors_correctly() {
        let mut state = setup_multi_cursor("foo bar foo");

        state.cursor_char_idx = 0;
        assert!(state.multi_cursor_add_next());
        assert!(state.multi_cursor_add_next());
        assert!(state.multi_cursor_delete());

        assert_eq!(state.current_mode(), EditorMode::MultiCursor);
        assert_eq!(state.cursor_char_idx, 0);
        assert_eq!(state.virtual_cursors()[0].char_idx, 5);
        assert_eq!(state.text_string(), " bar ");
    }

    /// `I` must move cursors to sel_start, `A` to sel_end — unchanged behavior.
    #[test]
    fn multi_cursor_insert_before_and_append_after_unchanged() {
        // Test `I`
        let mut state = setup_multi_cursor("foo bar foo");
        state.cursor_char_idx = 0;
        assert!(state.multi_cursor_add_next());
        assert!(state.multi_cursor_add_next());

        let mut state_i = state.clone();
        assert!(state_i.multi_cursor_insert_before());
        assert_eq!(state_i.cursor_char_idx, 0, "I: primary at sel start");
        assert_eq!(
            state_i.virtual_cursors()[0].char_idx,
            8,
            "I: vc at its sel start"
        );
        assert_eq!(state_i.current_mode(), EditorMode::MultiInsert);

        // Test `A`
        let mut state_a = state.clone();
        assert!(state_a.multi_cursor_append_after());
        // primary: anchor=0, cursor=2 → end = max(0,2)+1 = 3
        assert_eq!(state_a.cursor_char_idx, 3, "A: primary at sel end");
        // vc: sel_end = 11 → char_idx = 11
        assert_eq!(
            state_a.virtual_cursors()[0].char_idx,
            11,
            "A: vc at its sel end"
        );
        assert_eq!(state_a.current_mode(), EditorMode::MultiInsert);
    }

    #[test]
    fn check_and_reload_external_changes_reloads_modified_files() {
        let temp_dir =
            std::env::temp_dir().join(format!("netherize_external_reload_{}", std::process::id()));
        fs::create_dir_all(&temp_dir).expect("create temp dir");
        let file_path = temp_dir.join("test.txt");
        fs::write(&file_path, "original content").expect("write test file");
        let file_path = file_path.canonicalize().expect("canonicalize file_path");

        let mut state = AppState::new(file_path.clone());
        state.open_file(file_path.clone()).expect("open file");

        let mut last_checked = HashMap::new();
        // First check: populates the check time registry
        let changed = state.collect_externally_modified_open_buffers(&mut last_checked);
        assert!(changed.is_empty());
        assert!(last_checked.contains_key(&file_path));

        // Sleep briefly to ensure time difference is detectable if filesystem timestamps are coarse
        std::thread::sleep(Duration::from_millis(100));

        // Modify file externally on disk
        fs::write(&file_path, "updated content").expect("write update");

        // Second check: should detect the modify; contents are applied via the
        // (worker-simulated) read + apply phase.
        let changed = state.collect_externally_modified_open_buffers(&mut last_checked);
        assert_eq!(changed, vec![file_path.clone()]);
        let applied = state.apply_external_file_contents(&read_external_files(&changed));
        assert!(applied.active_file_reloaded);
        assert_eq!(state.text_string(), "updated content");

        let _ = fs::remove_dir_all(temp_dir);
    }

    #[test]
    fn check_and_reload_external_changes_reloads_even_before_first_tick() {
        let temp_dir = std::env::temp_dir().join(format!(
            "netherize_external_reload_first_tick_{}",
            std::process::id()
        ));
        fs::create_dir_all(&temp_dir).expect("create temp dir");
        let file_path = temp_dir.join("test.txt");
        fs::write(&file_path, "original content").expect("write test file");
        let file_path = file_path.canonicalize().expect("canonicalize file_path");

        let mut state = AppState::new(file_path.clone());
        state.open_file(file_path.clone()).expect("open file");

        // Sleep briefly to ensure time difference is detectable
        std::thread::sleep(Duration::from_millis(100));

        // Modify file externally BEFORE any check/tick runs
        fs::write(&file_path, "modified before tick").expect("write update");

        let mut last_checked = HashMap::new();
        // First check: should detect the modify against last_known_modified_time
        let changed = state.collect_externally_modified_open_buffers(&mut last_checked);
        assert_eq!(changed, vec![file_path.clone()]);
        let applied = state.apply_external_file_contents(&read_external_files(&changed));
        assert!(applied.active_file_reloaded);
        assert_eq!(state.text_string(), "modified before tick");

        let _ = fs::remove_dir_all(temp_dir);
    }

    #[test]
    fn leetcode_cases_load_and_persist_via_file_header() {
        use crate::runner::leetcode_cache::{
            CachedCase, LeetCodeProblemCache, build_header, load_cache_in, save_cache_in,
        };
        let dir = unique_temp_path("leetcode_cache_glue");
        save_cache_in(
            &dir,
            &LeetCodeProblemCache {
                id: "1".into(),
                slug: "two-sum".into(),
                title: "Two Sum".into(),
                statement: String::new(),
                function_name: "twoSum".into(),
                parameters: Vec::new(),
                cases: vec![CachedCase {
                    input: r#"{"nums":[2,7],"target":9}"#.into(),
                    expected: "[0,1]".into(),
                }],
            },
        )
        .expect("seed cache");

        let header = build_header("javascript", "1", "two-sum", "Two Sum");
        let mut state = AppState::from_text(
            unique_temp_path("solution.js"),
            &format!("{header}function solve() {{}}\n"),
        );

        assert!(state.load_leetcode_cases_from(&dir));
        assert_eq!(state.test_runner.cases.len(), 1);
        assert_eq!(state.test_runner.cases[0].expected, "[0,1]");

        state.test_runner.add_case("{}", "null");
        state.persist_leetcode_cases_to(&dir);
        let reloaded = load_cache_in(&dir, "1").expect("reload cache");
        assert_eq!(reloaded.cases.len(), 2);

        let _ = fs::remove_dir_all(&dir);
    }

    // ---- Long-line auto-fold threshold (raised to 1000 chars) ----

    #[test]
    fn long_line_auto_fold_only_folds_above_1000_chars() {
        let short_line = "x".repeat(300);
        let long_line = "y".repeat(1001);
        let text = format!("fn a() {{}}\n{short_line}\n{long_line}\n");
        let mut st = AppState::from_text(PathBuf::from("fold_threshold.rs"), &text);
        st.auto_fold_pathological_long_lines();
        assert!(
            !st.is_auto_folded_long_line(1),
            "a 300-char line must NOT auto-fold"
        );
        assert!(
            st.is_auto_folded_long_line(2),
            "a 1001-char line must auto-fold"
        );
    }

    // ---- Fold-aware scroll conversion (smooth Ctrl-D/U across folds) ----

    #[test]
    fn scroll_visual_logical_roundtrip_is_identity_without_folds() {
        let text: String = (0..50).map(|i| format!("line {i}\n")).collect();
        let st = AppState::from_text(PathBuf::from("conv_identity.rs"), &text);
        for &x in &[0.0_f32, 3.0, 7.5, 12.25] {
            assert!((st.logical_scroll_to_visual(x) - x).abs() < 1e-4, "fwd {x}");
            assert!((st.visual_scroll_to_logical(x) - x).abs() < 1e-4, "inv {x}");
        }
    }

    #[test]
    fn scroll_visual_skips_hidden_folded_lines() {
        // Fold (5,8) hides logical lines 6,7,8; visible order is 0..=5, then 9,10,...
        let text: String = (0..30).map(|i| format!("line {i}\n")).collect();
        let mut st = AppState::from_text(PathBuf::from("conv_folds.rs"), &text);
        st.folded_ranges = vec![(5, 8)];

        // Marker line 5 stays at visual index 5; first line after the fold (9) is 6.
        assert_eq!(st.logical_scroll_to_visual(5.0), 5.0);
        assert_eq!(st.logical_scroll_to_visual(9.0), 6.0);
        assert_eq!(st.logical_scroll_to_visual(10.0), 7.0);
        // Inverse lands on visible logical lines, never inside the hidden block.
        assert_eq!(st.visual_scroll_to_logical(5.0), 5.0);
        assert_eq!(st.visual_scroll_to_logical(6.0), 9.0);
        // A fractional visual position interpolates within one on-screen line.
        assert!((st.visual_scroll_to_logical(5.5) - 5.5).abs() < 1e-4);
    }

    #[test]
    fn cursor_visual_line_without_folds_equals_logical() {
        let text: String = (0..50).map(|i| format!("line {i}\n")).collect();
        let mut st = AppState::from_text(PathBuf::from("cvl.rs"), &text);
        st.jump_to_line_and_column(17, 0);
        assert!((st.cursor_visual_line() - 17.0).abs() < 1e-3);
    }

    #[test]
    fn caret_scroll_lag_defaults_zero() {
        let st = AppState::from_text(PathBuf::from("lag.rs"), "a\nb\n");
        assert_eq!(st.caret_scroll_lag, 0.0);
    }

    #[test]
    fn cursor_visual_line_maps_hidden_to_fold_marker() {
        let text: String = (0..50).map(|i| format!("line {i}\n")).collect();
        let mut st = AppState::from_text(PathBuf::from("cvl_fold.rs"), &text);
        st.folded_ranges = vec![(5, 10)]; // hide logical 6..=10
        st.jump_to_line_and_column(8, 0); // cursor parked on a hidden line
        // Maps to marker line 5 → visual 5 (nothing hidden before it).
        assert!((st.cursor_visual_line() - 5.0).abs() < 1e-3);
    }

    #[test]
    fn auto_scroll_edge_follow_advances_exactly_one_visual_line() {
        let text: String = (0..200).map(|i| format!("line {i}\n")).collect();
        let mut st = AppState::from_text(PathBuf::from("edge.rs"), &text);
        let viewport = 40usize;
        // Cursor just inside the bottom margin → the viewport must NOT scroll.
        st.jump_to_line_and_column(46, 0);
        st.set_target_scroll_line(10);
        st.snap_current_scroll_to_target();
        st.auto_scroll_to_cursor(viewport);
        assert_eq!(st.scroll_line(), 10, "cursor inside the margin must not scroll");
        // Cursor one line past the bottom margin → advance by exactly one line.
        st.jump_to_line_and_column(47, 0);
        st.set_target_scroll_line(10);
        st.snap_current_scroll_to_target();
        st.auto_scroll_to_cursor(viewport);
        assert_eq!(st.scroll_line(), 11, "edge crossing must advance exactly one line");
    }

    #[test]
    fn auto_scroll_does_not_scroll_when_fold_above_keeps_cursor_visible() {
        // A fold ABOVE the cursor compresses the on-screen distance: logical line 50
        // sits at visual row ~10 (40 lines hidden), well inside a 40-row viewport.
        // The old logical-space math compared logical 50 against the visual viewport
        // height and scrolled prematurely — the fold-crossing jitter. Visual-space
        // follow must leave the scroll alone here.
        let text: String = (0..200).map(|i| format!("line {i}\n")).collect();
        let mut st = AppState::from_text(PathBuf::from("foldabove.rs"), &text);
        st.folded_ranges = vec![(5, 45)]; // hide logical 6..=45
        let viewport = 40usize;
        st.jump_to_line_and_column(50, 0);
        st.set_target_scroll_line(0);
        st.snap_current_scroll_to_target();
        st.auto_scroll_to_cursor(viewport);
        assert_eq!(
            st.scroll_line(),
            0,
            "a fold above the cursor must not trigger premature scrolling"
        );
    }

    #[test]
    fn auto_scroll_across_fold_keeps_visual_target_monotonic() {
        // Sweeping the cursor downward across a fold (carrying the scroll forward,
        // as repeated j does) must never move the on-screen scroll target backward.
        // Logical-vs-visual mixing in the old impl was the up/down fold jitter.
        let text: String = (0..200).map(|i| format!("line {i}\n")).collect();
        let mut st = AppState::from_text(PathBuf::from("foldsweep.rs"), &text);
        st.folded_ranges = vec![(20, 60)]; // hide logical lines 21..=60
        let viewport = 40usize;
        let mut carry = 0usize;
        let mut last_visual = -1.0_f32;
        for line in 0..120 {
            st.jump_to_line_and_column(line, 0);
            st.set_target_scroll_line(carry);
            st.auto_scroll_to_cursor(viewport);
            carry = st.scroll_line();
            let v = st.logical_scroll_to_visual(st.target_scroll_y);
            assert!(
                v + 1e-3 >= last_visual,
                "visual scroll target went backwards at line {line}: {v} < {last_visual}"
            );
            last_visual = v;
        }
    }

    #[test]
    fn scroll_tween_across_fold_is_visually_monotonic() {
        // Easing the scroll in visual space and round-tripping through logical must
        // never move the on-screen position backwards (that backward jump WAS the
        // Ctrl-D/U stutter across a fold).
        let text: String = (0..30).map(|i| format!("line {i}\n")).collect();
        let mut st = AppState::from_text(PathBuf::from("tween_fold.rs"), &text);
        st.folded_ranges = vec![(5, 8)];

        let mut prev_visual = -1.0_f32;
        for step in 0..=20 {
            let v = step as f32 * 0.5; // sweep visual scroll 0.0 .. 10.0
            let logical = st.visual_scroll_to_logical(v);
            let visual_back = st.logical_scroll_to_visual(logical);
            assert!(
                visual_back >= prev_visual - 1e-4,
                "visual y went backwards at step {step}: {visual_back} < {prev_visual}"
            );
            prev_visual = visual_back;
        }
    }

    #[test]
    fn insert_line_below_after_open_brace_indents_one_level() {
        // `o` on a block-opening line should descend one indent level.
        let mut state = AppState::from_text(unique_temp_path("o_brace"), "const X = {\n}");
        assert!(state.insert_line_below());
        assert_eq!(state.text_string(), "const X = {\n    \n}");
        assert_eq!(state.cursor_line_col(), (1, 4));
    }

    #[test]
    fn insert_line_below_after_open_paren_indents_one_level() {
        let mut state = AppState::from_text(unique_temp_path("o_paren"), "foo(");
        assert!(state.insert_line_below());
        assert_eq!(state.text_string(), "foo(\n    ");
        assert_eq!(state.cursor_line_col(), (1, 4));
    }

    #[test]
    fn insert_line_below_descends_from_indented_opener() {
        // `o` on an already-indented opener adds one more level on top.
        let mut state = AppState::from_text(
            unique_temp_path("o_nested"),
            "    if (x) {\n        body();\n    }",
        );
        assert!(state.insert_line_below());
        assert_eq!(
            state.text_string(),
            "    if (x) {\n        \n        body();\n    }"
        );
        assert_eq!(state.cursor_line_col(), (1, 8));
    }

    #[test]
    fn insert_line_below_copies_indent_without_extra_for_plain_line() {
        let mut state = AppState::from_text(unique_temp_path("o_plain"), "        body();");
        assert!(state.insert_line_below());
        assert_eq!(state.text_string(), "        body();\n        ");
        assert_eq!(state.cursor_line_col(), (1, 8));
    }

    #[test]
    fn insert_line_above_matches_current_line_indent() {
        // `O` should land at the current line's indent, not column 0.
        let mut state = AppState::from_text(unique_temp_path("o_above_indent"), "        logger();");
        assert!(state.insert_line_above());
        assert_eq!(state.text_string(), "        \n        logger();");
        assert_eq!(state.cursor_line_col(), (0, 8));
    }

    #[test]
    fn insert_line_above_uses_indent_of_line_pushed_down() {
        let mut state =
            AppState::from_text(unique_temp_path("o_above_body"), "} catch {\n    body();");
        state.move_down(); // cursor onto the indented body line
        assert!(state.insert_line_above());
        assert_eq!(state.text_string(), "} catch {\n    \n    body();");
        assert_eq!(state.cursor_line_col(), (1, 4));
    }
}
