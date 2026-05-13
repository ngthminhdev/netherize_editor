use super::super::*;
use crate::async_runtime::message::WorkerResultPayload;
use std::path::PathBuf;

pub(super) fn handle_preview_result(app: &mut AppShell, payload: WorkerResultPayload) {
    if let WorkerResultPayload::FilePreviewLoaded {
        file_path,
        target_line,
        lines,
    } = payload
    {
        let changed = if app.app_state.active_buffer_is_fuzzy_picker()
            && active_fuzzy_preview_target(&app.app_state) == Some((file_path.clone(), target_line))
        {
            let (preview_text, preview_spans) =
                build_preview_render_data(&lines, &file_path, &app.theme);
            app.app_state
                .set_fuzzy_picker_preview(lines, preview_text, preview_spans)
        } else if app.app_state.active_buffer_is_references()
            && active_references_preview_target(&app.app_state)
                == Some((file_path.clone(), target_line))
        {
            let (preview_text, preview_spans) =
                build_preview_render_data(&lines, &file_path, &app.theme);
            app.app_state
                .set_active_references_preview(lines, preview_text, preview_spans)
        } else if app.app_state.active_buffer_is_diagnostics()
            && active_diagnostics_preview_target(&app.app_state)
                == Some((file_path.clone(), target_line))
        {
            let (preview_text, preview_spans) =
                build_preview_render_data(&lines, &file_path, &app.theme);
            app.app_state
                .set_active_diagnostics_preview(lines, preview_text, preview_spans)
        } else {
            false
        };

        if changed {
            app.editor_needs_layout = true;
            app.editor_caret_needs_layout = false;
            app.request_redraw();
        }
    }
}

pub(super) fn active_fuzzy_preview_target(
    app_state: &AppState,
) -> Option<(PathBuf, Option<usize>)> {
    match app_state.command_palette_selected_action()? {
        crate::app::command_palette::CommandPaletteAction::OpenFile(path) => Some((path, None)),
        crate::app::command_palette::CommandPaletteAction::OpenSearchMatch {
            path, line, ..
        } => Some((path, Some(line as usize))),
        _ => None,
    }
}

pub(super) fn active_references_preview_target(
    app_state: &AppState,
) -> Option<(PathBuf, Option<usize>)> {
    let item = app_state.selected_reference_item()?;
    Some((item.path.clone(), Some(item.line + 1)))
}

pub(super) fn active_diagnostics_preview_target(
    app_state: &AppState,
) -> Option<(PathBuf, Option<usize>)> {
    let item = app_state.selected_diagnostic_item()?;
    Some((item.file_path.clone(), Some(item.line + 1)))
}

pub(super) fn read_file_preview(
    path: &std::path::Path,
    center_line: usize,
    context: usize,
) -> Vec<String> {
    use std::io::{BufRead, BufReader};
    let file = match std::fs::File::open(path) {
        Ok(f) => f,
        Err(_) => return Vec::new(),
    };
    let start = center_line.saturating_sub(context);
    let end = center_line + context + 1;
    BufReader::new(file)
        .lines()
        .enumerate()
        .filter_map(|(i, line)| {
            if i >= start && i < end {
                let text = line.unwrap_or_default();
                let marker = if i == center_line { "▶" } else { " " };
                Some(format!("{marker} {:>4}  {}", i + 1, text))
            } else {
                None
            }
        })
        .collect()
}
