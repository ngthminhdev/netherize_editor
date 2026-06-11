use std::{
    path::PathBuf,
    sync::{Arc, mpsc as std_mpsc},
};

use winit::event_loop::EventLoopProxy;

use crate::{
    app::event_loop::AppEvent,
    async_runtime::message::{
        WorkerEvent, WorkerEventKind, WorkerFailure, WorkerFailureKind, WorkerMessage,
        WorkerRequest, WorkerRequestPayload, WorkerResult, WorkerResultPayload,
    },
    lsp::{
        client::{
            build_did_change_notification, build_did_close_notification,
            build_did_open_notification, spawn_lsp_server,
        },
        registry::language_profile_for_path,
        registry::{language_profile_for_binary, language_profile_for_language_id},
    },
};

use super::{
    LspSessionHandle, LspSessionRegistry, async_trace,
    emit::{emit_message, emit_message_and_wake, failure_from_join_error},
    lsp_io::{spawn_lsp_stderr_logger, spawn_lsp_stdout_reader},
    lsp_parse::{
        handle_lsp_code_action, handle_lsp_completion, handle_lsp_completion_resolve,
        handle_lsp_completion_virtual_hover, handle_lsp_definition, handle_lsp_document_highlight,
        handle_lsp_document_symbols, handle_lsp_formatting, handle_lsp_hover,
        handle_lsp_references, handle_lsp_rename, handle_workspace_symbol,
    },
};

pub(super) async fn run_lsp_request(
    request: WorkerRequest,
    lsp_sessions: Arc<LspSessionRegistry>,
    worker_tx: std_mpsc::Sender<WorkerMessage>,
    event_proxy: EventLoopProxy<AppEvent>,
) {
    emit_message(
        &worker_tx,
        WorkerMessage::Event(WorkerEvent {
            request_id: request.request_id,
            revision_id: request.revision_id,
            topic: request.topic,
            kind: WorkerEventKind::Started,
        }),
    );
    async_trace!(
        "[Worker] started lsp request_id={} revision={}",
        request.request_id,
        request.revision_id
    );

    let lsp_request = request.clone();
    let lsp_sessions_for_task = lsp_sessions.clone();
    let worker_tx_for_task = worker_tx.clone();
    let worker_handle = tokio::task::spawn_blocking(move || {
        execute_lsp_request(&lsp_request, &lsp_sessions_for_task, &worker_tx_for_task)
    });

    match worker_handle.await {
        Ok(Ok(payload)) => {
            emit_message_and_wake(
                &worker_tx,
                &event_proxy,
                WorkerMessage::Result(WorkerResult {
                    request_id: request.request_id,
                    revision_id: request.revision_id,
                    topic: request.topic,
                    payload,
                }),
            );
            emit_message_and_wake(
                &worker_tx,
                &event_proxy,
                WorkerMessage::Event(WorkerEvent {
                    request_id: request.request_id,
                    revision_id: request.revision_id,
                    topic: request.topic,
                    kind: WorkerEventKind::Completed,
                }),
            );
            async_trace!(
                "[Worker] completed lsp request_id={} revision={}",
                request.request_id,
                request.revision_id
            );
        }
        Ok(Err(message)) => {
            if let Some(tool_name) = message.strip_prefix("LSPMISSING:") {
                let language_id = language_profile_for_binary(tool_name)
                    .map(|p| p.language_id.to_string())
                    .unwrap_or_default();
                emit_message_and_wake(
                    &worker_tx,
                    &event_proxy,
                    WorkerMessage::LspMissingDependency {
                        language_id,
                        tool_name: tool_name.to_string(),
                    },
                );
                async_trace!(
                    "[Worker] lsp binary missing request_id={} revision={}",
                    request.request_id,
                    request.revision_id
                );
            } else {
                emit_message_and_wake(
                    &worker_tx,
                    &event_proxy,
                    WorkerMessage::Event(WorkerEvent {
                        request_id: request.request_id,
                        revision_id: request.revision_id,
                        topic: request.topic,
                        kind: WorkerEventKind::Failed {
                            error: WorkerFailure {
                                kind: WorkerFailureKind::Execution,
                                message,
                            },
                        },
                    }),
                );
                async_trace!(
                    "[Worker] failed lsp request_id={} revision={}",
                    request.request_id,
                    request.revision_id
                );
            }
        }
        Err(join_error) => {
            emit_message_and_wake(
                &worker_tx,
                &event_proxy,
                WorkerMessage::Event(WorkerEvent {
                    request_id: request.request_id,
                    revision_id: request.revision_id,
                    topic: request.topic,
                    kind: WorkerEventKind::Failed {
                        error: failure_from_join_error(join_error),
                    },
                }),
            );
            async_trace!(
                "[Worker] failed (panic/cancelled) lsp request_id={} revision={}",
                request.request_id,
                request.revision_id
            );
        }
    }
}

fn execute_lsp_request(
    request: &WorkerRequest,
    lsp_sessions: &Arc<LspSessionRegistry>,
    worker_tx: &std_mpsc::Sender<WorkerMessage>,
) -> Result<WorkerResultPayload, String> {
    match &request.payload {
        WorkerRequestPayload::StartLspServer {
            root_path,
            server_command,
            custom_bin_path,
        } => {
            let handle = tokio::runtime::Handle::try_current()
                .map_err(|_| "tokio runtime unavailable while starting lsp server".to_string())?;
            let spawned = handle.block_on(spawn_lsp_server(
                server_command.as_deref(),
                root_path,
                custom_bin_path.as_deref(),
                request.request_id,
                request.revision_id,
            ))?;
            let session = spawned.process.clone();
            let server_key = format!("{}@{}", spawned.server_name, spawned.root_path.display());
            let previous = lsp_sessions.replace(
                server_key,
                LspSessionHandle {
                    process: session.clone(),
                    server_name: spawned.server_name.clone(),
                    root_path: spawned.root_path.clone(),
                    capabilities: spawned.capabilities.clone(),
                },
            )?;

            spawn_lsp_stdout_reader(
                session,
                spawned.reader,
                request.topic,
                spawned.server_name.clone(),
                lsp_sessions.clone(),
                worker_tx.clone(),
            )?;
            if let Some(stderr) = spawned.stderr {
                spawn_lsp_stderr_logger(stderr, spawned.server_name.clone())?;
            }
            if let Some(previous) = previous {
                eprintln!(
                    "[Worker] restarting LSP '{}' for {} -> '{}' for {}",
                    previous.server_name,
                    previous.root_path.display(),
                    spawned.server_name,
                    spawned.root_path.display()
                );
                previous
                    .process
                    .update_request_meta(request.request_id, request.revision_id);
                let _ = previous.process.shutdown_and_exit();
            }

            Ok(WorkerResultPayload::LspServerStarted {
                server_name: spawned.server_name,
                root_path: spawned.root_path,
                completion_trigger_chars: spawned.capabilities.completion_trigger_chars,
            })
        }
        WorkerRequestPayload::LspDidOpen {
            uri,
            language_id,
            version,
            text,
        } => {
            let Some(server_key) = language_profile_for_language_id(language_id)
                .map(|profile| profile.lsp_binary)
                .or(Some(language_id.as_str()))
            else {
                return Err("lsp didOpen rejected: language profile not found".to_string());
            };
            let Some(session) = lsp_sessions.get_by_binary(server_key)? else {
                return Err("lsp didOpen rejected: server is not running".to_string());
            };
            session.update_request_meta(request.request_id, request.revision_id);
            if session.is_document_open(uri) {
                session.send_notification(
                    "textDocument/didChange",
                    build_did_change_notification(uri, *version, text),
                )?;
                Ok(WorkerResultPayload::LspAck {
                    action: "didChange".to_string(),
                    uri: Some(uri.clone()),
                    version: Some(*version),
                })
            } else {
                session.send_notification(
                    "textDocument/didOpen",
                    build_did_open_notification(uri, language_id, *version, text),
                )?;
                session.mark_document_open(uri);
                Ok(WorkerResultPayload::LspAck {
                    action: "didOpen".to_string(),
                    uri: Some(uri.clone()),
                    version: Some(*version),
                })
            }
        }
        WorkerRequestPayload::LspDidChange { uri, version, text } => {
            let Some(active_session) = lsp_sessions
                .sessions
                .lock()
                .map_err(|_| "lsp session lock poisoned".to_string())?
                .values()
                .find(|session| session.process.is_document_open(uri))
                .map(|session| session.process.clone())
            else {
                return Err("lsp didChange rejected: server is not running".to_string());
            };
            active_session.update_request_meta(request.request_id, request.revision_id);
            active_session.send_notification(
                "textDocument/didChange",
                build_did_change_notification(uri, *version, text),
            )?;

            Ok(WorkerResultPayload::LspAck {
                action: "didChange".to_string(),
                uri: Some(uri.clone()),
                version: Some(*version),
            })
        }
        WorkerRequestPayload::LspDidClose { uri } => {
            let Some(active_session) = lsp_sessions
                .sessions
                .lock()
                .map_err(|_| "lsp session lock poisoned".to_string())?
                .values()
                .find(|session| session.process.is_document_open(uri))
                .map(|session| session.process.clone())
            else {
                return Err("lsp didClose rejected: server is not running".to_string());
            };
            active_session.update_request_meta(request.request_id, request.revision_id);
            active_session
                .send_notification("textDocument/didClose", build_did_close_notification(uri))?;
            active_session.mark_document_closed(uri);

            Ok(WorkerResultPayload::LspAck {
                action: "didClose".to_string(),
                uri: Some(uri.clone()),
                version: None,
            })
        }
        WorkerRequestPayload::StopLspServer => {
            let Some(session) = lsp_sessions.take_any()? else {
                return Err("stop lsp rejected: no active server".to_string());
            };
            session
                .process
                .update_request_meta(request.request_id, request.revision_id);
            let exit_status = session.process.shutdown_and_exit()?;
            Ok(WorkerResultPayload::LspServerStopped {
                exit_status,
                reason: "shutdown requested by app".to_string(),
            })
        }
        WorkerRequestPayload::ShutdownAllLspServers => {
            let sessions = lsp_sessions.drain_all()?;
            let mut last_exit_status = None;
            for session in sessions {
                session
                    .process
                    .update_request_meta(request.request_id, request.revision_id);
                last_exit_status = session.process.shutdown_and_exit()?;
            }
            Ok(WorkerResultPayload::LspServerStopped {
                exit_status: last_exit_status,
                reason: "all lsp servers shutdown for workspace switch".to_string(),
            })
        }
        WorkerRequestPayload::LspHoverRequest {
            language_id,
            uri,
            line,
            character,
            for_completion,
            completion_revision,
        } => {
            let handle = lsp_sessions.get_handle_by_uri(uri)?.or_else(|| {
                language_profile_for_language_id(language_id)
                    .map(|profile| profile.lsp_binary)
                    .and_then(|key| lsp_sessions.get_handle(key).ok().flatten())
            });
            let Some(handle) = handle else {
                return Err("hover rejected: LSP server not running".to_string());
            };
            if !handle.capabilities.hover {
                return Err(format!(
                    "hover rejected: {} does not advertise hoverProvider",
                    handle.server_name
                ));
            }
            handle
                .process
                .update_request_meta(request.request_id, request.revision_id);
            handle_lsp_hover(
                &handle.process,
                uri,
                *line,
                *character,
                0,
                0,
                *for_completion,
                *completion_revision,
            )
        }
        WorkerRequestPayload::LspCompletionVirtualHoverRequest {
            language_id,
            uri,
            original_text,
            text,
            hover_line,
            hover_character,
            completion_revision,
        } => {
            let handle = lsp_sessions.get_handle_by_uri(uri)?.or_else(|| {
                language_profile_for_language_id(language_id)
                    .map(|profile| profile.lsp_binary)
                    .and_then(|key| lsp_sessions.get_handle(key).ok().flatten())
            });
            let Some(handle) = handle else {
                return Err("completion virtual hover rejected: LSP server not running".to_string());
            };
            if !handle.capabilities.hover {
                return Err(format!(
                    "completion virtual hover rejected: {} does not advertise hoverProvider",
                    handle.server_name
                ));
            }
            handle
                .process
                .update_request_meta(request.request_id, request.revision_id);
            handle_lsp_completion_virtual_hover(
                &handle.process,
                uri,
                original_text,
                text,
                *hover_line,
                *hover_character,
                *completion_revision,
            )
        }
        WorkerRequestPayload::LspDefinitionRequest {
            uri,
            line,
            character,
            jump,
        } => {
            let Some(handle) = lsp_sessions.get_handle_by_uri(uri)? else {
                return Err("definition rejected: LSP server not running".to_string());
            };
            if !handle.capabilities.definition {
                return Err(format!(
                    "definition rejected: {} does not advertise definitionProvider",
                    handle.server_name
                ));
            }
            handle
                .process
                .update_request_meta(request.request_id, request.revision_id);
            handle_lsp_definition(&handle.process, uri, *line, *character, *jump)
        }
        WorkerRequestPayload::LspReferencesRequest {
            uri,
            line,
            character,
        } => {
            let Some(handle) = lsp_sessions.get_handle_by_uri(uri)? else {
                return Err("references rejected: LSP server not running".to_string());
            };
            if !handle.capabilities.references {
                return Err(format!(
                    "references rejected: {} does not advertise referencesProvider",
                    handle.server_name
                ));
            }
            handle
                .process
                .update_request_meta(request.request_id, request.revision_id);
            handle_lsp_references(&handle.process, uri, *line, *character)
                .map(|locations| WorkerResultPayload::LspReferencesResult { locations })
        }
        WorkerRequestPayload::LspRenameRequest {
            uri,
            line,
            character,
            new_name,
        } => {
            let Some(handle) = lsp_sessions.get_handle_by_uri(uri)? else {
                return Err("rename rejected: LSP server not running".to_string());
            };
            if !handle.capabilities.rename {
                return Err(format!(
                    "rename rejected: {} does not advertise renameProvider",
                    handle.server_name
                ));
            }
            handle
                .process
                .update_request_meta(request.request_id, request.revision_id);
            handle_lsp_rename(&handle.process, uri, *line, *character, new_name)
        }
        WorkerRequestPayload::LspDocumentHighlightRequest {
            language_id,
            uri,
            line,
            character,
        } => {
            let handle = lsp_sessions.get_handle_by_uri(uri)?.or_else(|| {
                language_profile_for_language_id(language_id)
                    .map(|profile| profile.lsp_binary)
                    .and_then(|key| lsp_sessions.get_handle(key).ok().flatten())
            });
            let Some(handle) = handle else {
                return Err("document highlight rejected: LSP server not running".to_string());
            };
            if !handle.capabilities.document_highlight {
                return Err(format!(
                    "document highlight rejected: {} does not advertise documentHighlightProvider",
                    handle.server_name
                ));
            }
            handle
                .process
                .update_request_meta(request.request_id, request.revision_id);
            handle_lsp_document_highlight(&handle.process, uri, *line, *character)
        }
        WorkerRequestPayload::LspDocumentSymbolsRequest { language_id, uri } => {
            let handle = lsp_handle_for_uri_or_profile(lsp_sessions, uri, Some(language_id))?;
            let Some(handle) = handle else {
                return Err("document symbols rejected: LSP server not running".to_string());
            };
            handle
                .process
                .update_request_meta(request.request_id, request.revision_id);
            handle_lsp_document_symbols(&handle.process, uri)
        }
        WorkerRequestPayload::LspFormattingRequest {
            language_id,
            uri,
            tab_size,
            insert_spaces,
        } => {
            let handle = lsp_sessions.get_handle_by_uri(uri)?.or_else(|| {
                language_profile_for_language_id(language_id)
                    .map(|profile| profile.lsp_binary)
                    .and_then(|key| lsp_sessions.get_handle(key).ok().flatten())
            });
            let Some(handle) = handle else {
                return Err("formatting rejected: LSP server not running".to_string());
            };
            if !handle.capabilities.document_formatting {
                return Err(format!(
                    "formatting rejected: {} does not advertise documentFormattingProvider",
                    handle.server_name
                ));
            }
            handle
                .process
                .update_request_meta(request.request_id, request.revision_id);
            handle_lsp_formatting(&handle.process, uri, *tab_size, *insert_spaces)
        }
        WorkerRequestPayload::LspCompletionRequest {
            language_id,
            uri,
            line,
            character,
            cursor_line,
            cursor_col,
            prefix_start_col,
            prefix,
        } => {
            let handle = lsp_sessions.get_handle_by_uri(uri)?.or_else(|| {
                language_profile_for_language_id(language_id)
                    .map(|profile| profile.lsp_binary)
                    .and_then(|key| lsp_sessions.get_handle(key).ok().flatten())
            });
            let Some(handle) = handle else {
                return Err("completion rejected: LSP server not running".to_string());
            };
            if !handle.capabilities.completion {
                return Err(format!(
                    "completion rejected: {} does not advertise completionProvider",
                    handle.server_name
                ));
            }
            handle
                .process
                .update_request_meta(request.request_id, request.revision_id);
            handle_lsp_completion(
                &handle.process,
                uri,
                *line,
                *character,
                *cursor_line,
                *cursor_col,
                *prefix_start_col,
                prefix,
            )
        }
        WorkerRequestPayload::LspCompletionResolveRequest {
            language_id,
            uri,
            item_json,
            item_label,
            completion_revision,
        } => {
            let handle = lsp_sessions.get_handle_by_uri(uri)?.or_else(|| {
                language_profile_for_language_id(language_id)
                    .map(|profile| profile.lsp_binary)
                    .and_then(|key| lsp_sessions.get_handle(key).ok().flatten())
            });
            let Some(handle) = handle else {
                return Err("completion resolve rejected: LSP server not running".to_string());
            };
            if !handle.capabilities.completion_resolve {
                return Err(format!(
                    "completion resolve rejected: {} does not advertise resolveProvider",
                    handle.server_name
                ));
            }
            handle
                .process
                .update_request_meta(request.request_id, request.revision_id);
            handle_lsp_completion_resolve(
                &handle.process,
                item_label,
                item_json,
                *completion_revision,
            )
        }
        WorkerRequestPayload::LspCodeActionRequest {
            uri,
            line,
            character,
            diagnostics,
        } => {
            let Some(handle) = lsp_handle_for_uri_or_profile(lsp_sessions, uri, None)? else {
                return Err("codeAction rejected: LSP server not running".to_string());
            };
            handle
                .process
                .update_request_meta(request.request_id, request.revision_id);
            handle_lsp_code_action(&handle.process, uri, *line, *character, diagnostics)
        }
        WorkerRequestPayload::WorkspaceSymbolRequest { language_id, query } => {
            let handle = language_profile_for_language_id(language_id)
                .map(|profile| profile.lsp_binary)
                .and_then(|key| lsp_sessions.get_handle(key).ok().flatten());
            let Some(handle) = handle else {
                return Err("workspace/symbol rejected: LSP server not running".to_string());
            };
            handle
                .process
                .update_request_meta(request.request_id, request.revision_id);
            let symbols = handle_workspace_symbol(&handle.process, query)?;
            Ok(WorkerResultPayload::WorkspaceSymbols {
                language_id: language_id.clone(),
                symbols,
            })
        }
        _ => Err("execute_lsp_request received non-lsp payload".to_string()),
    }
}

fn lsp_handle_for_uri_or_profile(
    lsp_sessions: &Arc<LspSessionRegistry>,
    uri: &str,
    language_id: Option<&str>,
) -> Result<Option<LspSessionHandle>, String> {
    if let Some(handle) = lsp_sessions.get_handle_by_uri(uri)? {
        return Ok(Some(handle));
    }

    if let Some(handle) = language_id
        .and_then(language_profile_for_language_id)
        .map(|profile| profile.lsp_binary)
        .and_then(|key| lsp_sessions.get_handle(key).ok().flatten())
    {
        return Ok(Some(handle));
    }

    let Some(path) = path_from_lsp_uri(uri) else {
        return Ok(None);
    };
    let Some(profile) = language_profile_for_path(&path) else {
        return Ok(None);
    };
    lsp_sessions.get_handle(profile.lsp_binary)
}

fn path_from_lsp_uri(uri: &str) -> Option<PathBuf> {
    url::Url::parse(uri).ok()?.to_file_path().ok()
}
