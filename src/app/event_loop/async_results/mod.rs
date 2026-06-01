use super::*;
use crate::async_runtime::message::{RequestTopic, WorkerEvent, WorkerResult, WorkerResultPayload};

mod ai;
mod failure;
mod filesystem;
mod fzf;
mod git;
mod lsp;
mod preview;
mod shell;
mod syntax;
mod system;
mod terminal;

pub(crate) use lsp::apply_lsp_text_edits;

use lsp::{friendly_references_status, stale_references_status};

impl AsyncResultRouter for AppShell {
    fn current_revision_for(&self, topic: RequestTopic) -> u64 {
        match topic {
            RequestTopic::ActiveBufferLayout => self.active_highlight_request_revision,
            RequestTopic::FzfSearch => self.fzf_search_revision,
            RequestTopic::Git => self.git_overlay_revision,
            RequestTopic::GitStatus => self.git_status_revision,
            RequestTopic::GitBaseline => self.git_baseline_revision,
            RequestTopic::AiInlineCompletion => self.ai_inline_revision,
            RequestTopic::SystemDepCheck | RequestTopic::SystemDepInstall => 0,
            RequestTopic::FileOperation => 0,
            _ => 0,
        }
    }

    fn on_worker_event(&mut self, event: WorkerEvent) {
        failure::handle_worker_failure(self, event);
    }

    fn on_worker_result(&mut self, result: WorkerResult) {
        let request_id = result.request_id;
        let revision_id = result.revision_id;
        match result.payload {
            WorkerResultPayload::ParseAndHighlight { .. }
            | WorkerResultPayload::ParseSummary { .. } => {
                syntax::handle_syntax_result(self, result.payload);
            }
            WorkerResultPayload::FileSystemEvents { .. } => {
                filesystem::handle_filesystem_result(self, result.payload);
            }
            WorkerResultPayload::FzfResults { .. } => {
                fzf::handle_fzf_result(self, result.payload);
            }
            WorkerResultPayload::FilePreviewLoaded { .. } => {
                preview::handle_preview_result(self, result.payload);
            }
            WorkerResultPayload::PtySpawned { .. }
            | WorkerResultPayload::PtyOutput { .. }
            | WorkerResultPayload::PtyInputWritten { .. }
            | WorkerResultPayload::PtyResized { .. }
            | WorkerResultPayload::PtySessionClosed { .. } => {
                terminal::handle_terminal_result(self, result.payload, request_id);
            }
            WorkerResultPayload::DetachedShellCommandSpawned { .. } => {
                shell::handle_shell_result(self, result.payload);
            }
            WorkerResultPayload::WorkspaceGitStatus { .. }
            | WorkerResultPayload::BufferGitBaseline { .. }
            | WorkerResultPayload::GitBlameLine { .. } => {
                git::handle_git_result(self, result.payload);
            }
            WorkerResultPayload::LspServerStarted { .. }
            | WorkerResultPayload::LspServerStopped { .. }
            | WorkerResultPayload::LspDiagnostics { .. }
            | WorkerResultPayload::LspLogMessage { .. }
            | WorkerResultPayload::LspProgress { .. }
            | WorkerResultPayload::LspCheckResult { .. }
            | WorkerResultPayload::LspHoverResult { .. }
            | WorkerResultPayload::LspDefinitionResult { .. }
            | WorkerResultPayload::LspDocumentHighlightResult { .. }
            | WorkerResultPayload::LspReferencesResult { .. }
            | WorkerResultPayload::LspRenameResult { .. }
            | WorkerResultPayload::LspDocumentSymbolsResult { .. }
            | WorkerResultPayload::LspFormattingResult { .. }
            | WorkerResultPayload::LspCodeActionResult { .. }
            | WorkerResultPayload::LspCompletionResult { .. }
            | WorkerResultPayload::LspCompletionResolveResult { .. }
            | WorkerResultPayload::WorkspaceSymbols { .. } => {
                lsp::handle_lsp_result(self, result.payload, request_id, revision_id);
            }
            WorkerResultPayload::AiInlineCompletionChunk { .. }
            | WorkerResultPayload::AiInlineCompletionResult { .. } => {
                ai::handle_ai_result(self, result.payload);
            }
            WorkerResultPayload::SystemDepCheckResult { .. }
            | WorkerResultPayload::RuntimeVersionsDetected { .. }
            | WorkerResultPayload::PythonEnvironmentsDiscovered(_)
            | WorkerResultPayload::DartEnvironmentsDiscovered(_)
            | WorkerResultPayload::FlutterDevicesDiscovered(_)
            | WorkerResultPayload::FlutterEmulatorLaunched => {
                system::handle_system_result(self, result.payload);
            }
            WorkerResultPayload::FileCopyResult { .. } => {
                filesystem::handle_file_copy_result(self, result.payload);
            }
            _ => {}
        }
    }

    fn on_stale_result(&mut self, stale: WorkerResult) {
        lsp::handle_stale_result(self, stale);
    }

    fn on_ai_message_chunk(&mut self, text: String) {
        ai::handle_ai_message_chunk(self, text);
    }

    fn on_ai_stream_complete(&mut self) {
        ai::handle_ai_stream_complete(self);
    }

    fn on_ai_stream_cancelled(&mut self) {
        ai::handle_ai_stream_cancelled(self);
    }

    fn on_ai_stream_error(&mut self, error: String) {
        ai::handle_ai_stream_error(self, error);
    }

    fn on_ai_install_success(&mut self) {
        ai::handle_ai_install_success(self);
    }

    fn on_system_dep_tool_progress(
        &mut self,
        tool: String,
        status: crate::async_runtime::message::InstallStatus,
    ) {
        system::handle_system_dep_tool_progress(self, tool, status);
    }

    fn on_system_dep_install_done(&mut self) {
        system::handle_system_dep_install_done(self);
    }

    fn on_extension_command_started(&mut self, binary: String, uninstall: bool) {
        system::handle_extension_command_started(self, binary, uninstall);
    }

    fn on_extension_command_log(&mut self, binary: String, line: String) {
        system::handle_extension_command_log(self, binary, line);
    }

    fn on_extension_command_finished(
        &mut self,
        binary: String,
        uninstall: bool,
        success: bool,
        exit_code: Option<i32>,
    ) {
        system::handle_extension_command_finished(self, binary, uninstall, success, exit_code);
    }

    fn on_lsp_missing_dependency(&mut self, _language_id: String, tool_name: String) {
        lsp::handle_lsp_missing_dependency(self, tool_name);
    }
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        path::PathBuf,
        time::{SystemTime, UNIX_EPOCH},
    };

    use crate::{
        app::{app_state::ReferencesBufferItem, async_bridge::AsyncResultRouter},
        async_runtime::message::{
            FilePreviewLine, FileSystemChangeKind, FileSystemEvent, LspDocumentHighlight,
            LspLocation, LspPosition, LspRange, LspTextEdit, RequestTopic, WorkerEvent,
            WorkerEventKind, WorkerFailure, WorkerFailureKind, WorkerResult, WorkerResultPayload,
        },
        core::commands::Command,
        lsp::{CachedSymbol, client::path_to_lsp_uri},
        syntax::{
            highlight::{HighlightCategory, HighlightSpan},
            syntax_engine::LanguageId,
        },
    };

    use super::AppShell;

    fn unique_temp_path(suffix: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock drift")
            .as_nanos();
        std::env::temp_dir().join(format!("netherize_async_results_{suffix}_{nanos}"))
    }

    fn write_temp_file(suffix: &str, contents: &str) -> PathBuf {
        let path = unique_temp_path(suffix);
        fs::write(&path, contents).expect("write temp file");
        path.canonicalize().expect("canonical temp file")
    }

    fn parse_highlight_result(buffer_id: PathBuf, buffer_revision: u64) -> WorkerResult {
        WorkerResult {
            request_id: 1,
            revision_id: 1,
            topic: RequestTopic::ActiveBufferLayout,
            payload: WorkerResultPayload::ParseAndHighlight {
                buffer_id: buffer_id.clone(),
                file_path: Some(buffer_id),
                language_id: LanguageId::Rust,
                buffer_revision,
                spans: vec![HighlightSpan {
                    range: 0..2,
                    category: HighlightCategory::Keyword,
                }],
                covered_byte_range: None,
                foldable_ranges: Vec::new(),
                line_count: 1,
                char_count: 10,
                byte_count: 10,
                parse_time_ms: 1,
                highlight_time_ms: 1,
            },
        }
    }

    fn filesystem_result(root_path: PathBuf, events: Vec<FileSystemEvent>) -> WorkerResult {
        WorkerResult {
            request_id: 100,
            revision_id: 0,
            topic: RequestTopic::WorkspaceWatch,
            payload: WorkerResultPayload::FileSystemEvents { root_path, events },
        }
    }

    #[test]
    fn workspace_symbols_result_populates_cache_via_normal_router() {
        let mut shell = AppShell::new_for_tests().expect("create app shell");

        AsyncResultRouter::on_worker_result(
            &mut shell,
            WorkerResult {
                request_id: 200,
                revision_id: 0,
                topic: RequestTopic::SystemTask,
                payload: WorkerResultPayload::WorkspaceSymbols {
                    language_id: "typescript".to_string(),
                    symbols: vec![CachedSymbol {
                        name: "connect".to_string(),
                        kind: "Function".to_string(),
                        container_name: None,
                        file_path: PathBuf::from("src/api.ts"),
                        line: 0,
                        character: 16,
                        source_path: Some(PathBuf::from("src/api.ts")),
                        import_path: Some("src/api".to_string()),
                        export_kind: Some("named".to_string()),
                        callable: Some(true),
                        has_parameters: Some(false),
                    }],
                },
            },
        );

        assert_eq!(
            shell
                .app_state
                .workspace_symbol_cache()
                .symbol_count("typescript"),
            1
        );
        assert_eq!(
            shell
                .app_state
                .workspace_symbol_cache()
                .query_symbols("con", Some("typescript"))
                .first()
                .map(|symbol| symbol.name.as_str()),
            Some("connect")
        );
    }

    #[test]
    fn parse_highlight_result_for_inactive_buffer_is_rejected() {
        let mut shell = AppShell::new_for_tests().expect("create app shell");
        let old_file = write_temp_file("old_highlight.rs", "fn old() {}\n");
        let active_file = write_temp_file("active_highlight.rs", "fn active() {}\n");

        shell
            .app_state
            .open_file(old_file.clone())
            .expect("open old file");
        shell
            .app_state
            .open_file(active_file)
            .expect("open active file");
        let active_revision = shell.app_state.revision();
        shell.editor_needs_layout = false;
        shell.editor_caret_needs_layout = true;

        AsyncResultRouter::on_worker_result(
            &mut shell,
            parse_highlight_result(old_file, active_revision),
        );

        assert!(shell.highlight_spans.is_empty());
        assert!(!shell.editor_needs_layout);
    }

    #[test]
    fn parse_highlight_result_for_stale_revision_is_rejected() {
        let mut shell = AppShell::new_for_tests().expect("create app shell");
        let file_path = write_temp_file("stale_highlight.rs", "fn stale() {}\n");
        shell
            .app_state
            .open_file(file_path.clone())
            .expect("open file");
        let stale_revision = shell.app_state.revision().saturating_sub(1);
        shell.editor_needs_layout = false;
        shell.editor_caret_needs_layout = true;

        AsyncResultRouter::on_worker_result(
            &mut shell,
            parse_highlight_result(file_path, stale_revision),
        );

        assert!(shell.highlight_spans.is_empty());
        assert!(!shell.editor_needs_layout);
    }

    #[test]
    fn parse_highlight_result_for_active_buffer_revision_is_applied() {
        let mut shell = AppShell::new_for_tests().expect("create app shell");
        let file_path = write_temp_file("fresh_highlight.rs", "fn fresh() {}\n");
        shell
            .app_state
            .open_file(file_path.clone())
            .expect("open file");
        let active_revision = shell.app_state.revision();
        shell.editor_needs_layout = false;
        shell.editor_caret_needs_layout = true;

        AsyncResultRouter::on_worker_result(
            &mut shell,
            parse_highlight_result(file_path, active_revision),
        );

        assert_eq!(shell.highlight_spans.len(), 1);
        assert!(shell.editor_needs_layout);
        assert!(!shell.editor_caret_needs_layout);
    }

    #[test]
    fn dirty_external_modify_opens_overwrite_confirmation_overlay() {
        let mut shell = AppShell::new_for_tests().expect("create app shell");
        let file_path = write_temp_file("external_overwrite_prompt.rs", "fn main() {}\n");
        let root_path = file_path.parent().expect("parent path").to_path_buf();
        shell
            .app_state
            .open_file(file_path.clone())
            .expect("open file");
        shell.app_state.insert_char('!');
        fs::write(&file_path, "fn main() { println!(\"external\"); }\n").expect("external write");

        AsyncResultRouter::on_worker_result(
            &mut shell,
            filesystem_result(
                root_path,
                vec![FileSystemEvent {
                    kind: FileSystemChangeKind::Modify,
                    path: file_path.clone(),
                    new_path: None,
                }],
            ),
        );

        assert_eq!(
            shell.app_state.command_palette_mode(),
            Some(crate::app::command_palette::CommandPaletteMode::ExplorerDeleteConfirm)
        );
        let prompt = shell.app_state.command_palette_query_text();
        assert!(prompt.contains("Overwrite with current buffer? (y/n)"));

        let _ = fs::remove_file(file_path);
    }

    #[test]
    fn document_highlight_result_maps_ranges_into_active_buffer_bytes() {
        let mut shell = AppShell::new_for_tests().expect("create app shell");
        let file_path = write_temp_file(
            "document_highlight.rs",
            "let count = 1;\ncount += 1;\nother();\n",
        );
        shell
            .app_state
            .open_file(file_path.clone())
            .expect("open file");
        shell.semantic_highlight_request_revision = 2;
        shell.editor_needs_layout = false;
        shell.editor_caret_needs_layout = false;

        AsyncResultRouter::on_worker_result(
            &mut shell,
            WorkerResult {
                request_id: 88,
                revision_id: 2,
                topic: RequestTopic::LspRequest,
                payload: WorkerResultPayload::LspDocumentHighlightResult {
                    uri: path_to_lsp_uri(&file_path),
                    highlights: vec![
                        LspDocumentHighlight {
                            range: LspRange {
                                start: LspPosition {
                                    line: 0,
                                    character: 4,
                                },
                                end: LspPosition {
                                    line: 0,
                                    character: 9,
                                },
                            },
                            kind: Some(1),
                        },
                        LspDocumentHighlight {
                            range: LspRange {
                                start: LspPosition {
                                    line: 1,
                                    character: 0,
                                },
                                end: LspPosition {
                                    line: 1,
                                    character: 5,
                                },
                            },
                            kind: Some(2),
                        },
                    ],
                },
            },
        );

        assert_eq!(
            shell.app_state.semantic_symbol_highlights(),
            &[(4, 9), (15, 20)]
        );
        assert!(shell.editor_caret_needs_layout);
    }

    #[test]
    fn stale_document_highlight_result_is_ignored() {
        let mut shell = AppShell::new_for_tests().expect("create app shell");
        let file_path = write_temp_file("stale_document_highlight.rs", "let count = 1;\n");
        shell
            .app_state
            .open_file(file_path.clone())
            .expect("open file");
        let _ = shell.app_state.set_semantic_symbol_highlights(vec![(4, 9)]);
        shell.semantic_highlight_request_revision = 3;
        shell.editor_caret_needs_layout = false;

        AsyncResultRouter::on_worker_result(
            &mut shell,
            WorkerResult {
                request_id: 89,
                revision_id: 2,
                topic: RequestTopic::LspRequest,
                payload: WorkerResultPayload::LspDocumentHighlightResult {
                    uri: path_to_lsp_uri(&file_path),
                    highlights: vec![],
                },
            },
        );

        assert_eq!(shell.app_state.semantic_symbol_highlights(), &[(4, 9)]);
        assert!(!shell.editor_caret_needs_layout);
    }

    #[test]
    fn empty_document_highlight_result_falls_back_to_local_identifier_matches() {
        let mut shell = AppShell::new_for_tests().expect("create app shell");
        let file_path = write_temp_file(
            "fallback_document_highlight.rs",
            "let goals = 1;\ngoals += 1;\nlet go = goals;\n",
        );
        shell
            .app_state
            .open_file(file_path.clone())
            .expect("open file");
        assert!(shell.app_state.jump_to_line_and_column(1, 1));
        shell.semantic_highlight_request_revision = 4;
        shell.editor_caret_needs_layout = false;

        AsyncResultRouter::on_worker_result(
            &mut shell,
            WorkerResult {
                request_id: 90,
                revision_id: 4,
                topic: RequestTopic::LspRequest,
                payload: WorkerResultPayload::LspDocumentHighlightResult {
                    uri: path_to_lsp_uri(&file_path),
                    highlights: vec![],
                },
            },
        );

        assert_eq!(
            shell.app_state.semantic_symbol_highlights(),
            &[(4, 9), (15, 20), (36, 41)]
        );
        assert!(shell.editor_caret_needs_layout);
    }

    #[test]
    fn empty_document_highlight_result_does_not_match_identifier_substrings() {
        let mut shell = AppShell::new_for_tests().expect("create app shell");
        let file_path = write_temp_file(
            "fallback_document_highlight_boundaries.rs",
            "goal goals goaler\ngoal\n",
        );
        shell
            .app_state
            .open_file(file_path.clone())
            .expect("open file");
        assert!(shell.app_state.jump_to_line_and_column(0, 1));
        shell.semantic_highlight_request_revision = 5;
        shell.editor_caret_needs_layout = false;

        AsyncResultRouter::on_worker_result(
            &mut shell,
            WorkerResult {
                request_id: 91,
                revision_id: 5,
                topic: RequestTopic::LspRequest,
                payload: WorkerResultPayload::LspDocumentHighlightResult {
                    uri: path_to_lsp_uri(&file_path),
                    highlights: vec![],
                },
            },
        );

        assert_eq!(
            shell.app_state.semantic_symbol_highlights(),
            &[(0, 4), (18, 22)]
        );
        assert!(shell.editor_caret_needs_layout);
    }

    #[test]
    fn non_empty_document_highlight_result_keeps_lsp_ranges() {
        let mut shell = AppShell::new_for_tests().expect("create app shell");
        let file_path = write_temp_file(
            "lsp_document_highlight_preferred.rs",
            "let goals = 1;\ngoals += 1;\n",
        );
        shell
            .app_state
            .open_file(file_path.clone())
            .expect("open file");
        assert!(shell.app_state.jump_to_line_and_column(0, 5));
        shell.semantic_highlight_request_revision = 6;
        shell.editor_caret_needs_layout = false;

        AsyncResultRouter::on_worker_result(
            &mut shell,
            WorkerResult {
                request_id: 92,
                revision_id: 6,
                topic: RequestTopic::LspRequest,
                payload: WorkerResultPayload::LspDocumentHighlightResult {
                    uri: path_to_lsp_uri(&file_path),
                    highlights: vec![LspDocumentHighlight {
                        range: LspRange {
                            start: LspPosition {
                                line: 0,
                                character: 4,
                            },
                            end: LspPosition {
                                line: 0,
                                character: 9,
                            },
                        },
                        kind: Some(1),
                    }],
                },
            },
        );

        assert_eq!(shell.app_state.semantic_symbol_highlights(), &[(4, 9)]);
        assert!(shell.editor_caret_needs_layout);
    }

    #[test]
    fn references_result_clears_loading_and_populates_items() {
        let mut shell = AppShell::new_for_tests().expect("create app shell");
        let file_path = unique_temp_path("references_result.rs");
        shell.app_state.open_pending_references_buffer(
            "References",
            Some(file_path.clone()),
            0,
            41,
        );
        shell.references_request_revision = 1;
        shell.editor_needs_layout = false;
        shell.editor_caret_needs_layout = true;

        AsyncResultRouter::on_worker_result(
            &mut shell,
            WorkerResult {
                request_id: 41,
                revision_id: 1,
                topic: RequestTopic::LspRequest,
                payload: WorkerResultPayload::LspReferencesResult {
                    locations: vec![LspLocation {
                        uri: path_to_lsp_uri(&file_path),
                        line: 7,
                        character: 3,
                    }],
                },
            },
        );

        let references = shell
            .app_state
            .active_references_buffer()
            .expect("references buffer");
        assert!(!references.loading);
        assert_eq!(references.pending_request_id, None);
        assert_eq!(references.items.len(), 1);
        assert_eq!(references.items[0].line, 7);
        assert!(shell.editor_needs_layout);
        assert!(!shell.editor_caret_needs_layout);
    }

    #[test]
    fn stale_references_result_stops_loading_and_keeps_items_empty() {
        let mut shell = AppShell::new_for_tests().expect("create app shell");
        let file_path = unique_temp_path("stale_references_result.rs");
        shell.app_state.open_pending_references_buffer(
            "References",
            Some(file_path.clone()),
            0,
            52,
        );
        shell.references_request_revision = 2;
        shell.editor_needs_layout = false;
        shell.editor_caret_needs_layout = true;

        AsyncResultRouter::on_worker_result(
            &mut shell,
            WorkerResult {
                request_id: 52,
                revision_id: 1,
                topic: RequestTopic::LspRequest,
                payload: WorkerResultPayload::LspReferencesResult {
                    locations: vec![LspLocation {
                        uri: path_to_lsp_uri(&file_path),
                        line: 4,
                        character: 1,
                    }],
                },
            },
        );

        let references = shell
            .app_state
            .active_references_buffer()
            .expect("references buffer");
        assert!(!references.loading);
        assert_eq!(references.pending_request_id, None);
        assert!(references.items.is_empty());
        assert_eq!(
            references.status_message.as_deref(),
            Some("References request superseded by newer request")
        );
        assert!(shell.editor_needs_layout);
        assert!(!shell.editor_caret_needs_layout);
    }

    #[test]
    fn rename_result_applies_as_single_undo_transaction() {
        let mut shell = AppShell::new_for_tests().expect("create app shell");
        let file_path = write_temp_file(
            "rename_result.rs",
            "fn main() {\n    let value = 1;\n    value;\n}\n",
        );
        shell
            .app_state
            .open_file(file_path.clone())
            .expect("open file");
        shell.lsp_rename_request_revision = 1;
        shell.latest_rename_request_id = Some(91);

        AsyncResultRouter::on_worker_result(
            &mut shell,
            WorkerResult {
                request_id: 91,
                revision_id: 1,
                topic: RequestTopic::LspRequest,
                payload: WorkerResultPayload::LspRenameResult {
                    uri: path_to_lsp_uri(&file_path),
                    edits: vec![
                        LspTextEdit {
                            range: LspRange {
                                start: LspPosition {
                                    line: 1,
                                    character: 8,
                                },
                                end: LspPosition {
                                    line: 1,
                                    character: 13,
                                },
                            },
                            new_text: "total".to_string(),
                        },
                        LspTextEdit {
                            range: LspRange {
                                start: LspPosition {
                                    line: 2,
                                    character: 4,
                                },
                                end: LspPosition {
                                    line: 2,
                                    character: 9,
                                },
                            },
                            new_text: "total".to_string(),
                        },
                    ],
                    other_file_edit_count: 0,
                },
            },
        );

        assert_eq!(
            shell.app_state.text_string(),
            "fn main() {\n    let total = 1;\n    total;\n}\n"
        );

        assert!(shell.handle_command(Command::Undo));
        assert_eq!(
            shell.app_state.text_string(),
            "fn main() {\n    let value = 1;\n    value;\n}\n"
        );

        let _ = fs::remove_file(file_path);
    }

    #[test]
    fn formatting_result_applies_as_single_undo_redo_transaction() {
        let mut shell = AppShell::new_for_tests().expect("create app shell");
        let original = "fn main(){\nprintln!(\"hi\");\n}\n";
        let formatted = "fn main() {\n    println!(\"hi\");\n}\n";
        let file_path = write_temp_file("format_result.rs", original);
        shell
            .app_state
            .open_file(file_path.clone())
            .expect("open file");

        AsyncResultRouter::on_worker_result(
            &mut shell,
            WorkerResult {
                request_id: 92,
                revision_id: 1,
                topic: RequestTopic::LspRequest,
                payload: WorkerResultPayload::LspFormattingResult {
                    uri: path_to_lsp_uri(&file_path),
                    edits: vec![LspTextEdit {
                        range: LspRange {
                            start: LspPosition {
                                line: 0,
                                character: 0,
                            },
                            end: LspPosition {
                                line: 3,
                                character: 0,
                            },
                        },
                        new_text: formatted.to_string(),
                    }],
                },
            },
        );

        assert_eq!(shell.app_state.text_string(), formatted);
        assert!(shell.handle_command(Command::Undo));
        assert_eq!(shell.app_state.text_string(), original);
        assert!(shell.handle_command(Command::Redo));
        assert_eq!(shell.app_state.text_string(), formatted);

        let _ = fs::remove_file(file_path);
    }

    #[test]
    fn failed_references_event_clears_loading_buffer() {
        let mut shell = AppShell::new_for_tests().expect("create app shell");
        shell
            .app_state
            .open_pending_references_buffer("References", None, 0, 77);
        shell.references_request_revision = 3;
        shell.editor_needs_layout = false;
        shell.editor_caret_needs_layout = true;

        AsyncResultRouter::on_worker_event(
            &mut shell,
            WorkerEvent {
                request_id: 77,
                revision_id: 3,
                topic: RequestTopic::LspRequest,
                kind: WorkerEventKind::Failed {
                    error: WorkerFailure {
                        kind: WorkerFailureKind::Execution,
                        message: "references: timed out waiting for response".to_string(),
                    },
                },
            },
        );

        let references = shell
            .app_state
            .active_references_buffer()
            .expect("references buffer");
        assert!(!references.loading);
        assert_eq!(
            references.status_message.as_deref(),
            Some("References request timed out")
        );
        assert!(shell.editor_needs_layout);
        assert!(!shell.editor_caret_needs_layout);
    }

    #[test]
    fn references_preview_result_marks_editor_layout_dirty() {
        let mut shell = AppShell::new_for_tests().expect("create app shell");
        let file_path = unique_temp_path("references_preview.rs");
        shell
            .app_state
            .open_references_buffer(
                "References (1)",
                None,
                0,
                vec![ReferencesBufferItem {
                    path: file_path.clone(),
                    relative_path: "references_preview.rs".to_string(),
                    line: 4,
                    column: 0,
                    summary: "Ln 5, Col 1".to_string(),
                }],
            )
            .expect("open references buffer");
        shell.editor_needs_layout = false;
        shell.editor_caret_needs_layout = true;

        AsyncResultRouter::on_worker_result(
            &mut shell,
            WorkerResult {
                request_id: 1,
                revision_id: 0,
                topic: RequestTopic::FilePreview,
                payload: WorkerResultPayload::FilePreviewLoaded {
                    file_path,
                    target_line: Some(5),
                    lines: vec![FilePreviewLine {
                        line_number: 5,
                        text: "demo()".to_string(),
                        is_target: true,
                    }],
                },
            },
        );

        let references = shell
            .app_state
            .active_references_buffer()
            .expect("references buffer");
        assert_eq!(references.preview_lines.len(), 1);
        assert!(shell.editor_needs_layout);
        assert!(!shell.editor_caret_needs_layout);
    }
}
