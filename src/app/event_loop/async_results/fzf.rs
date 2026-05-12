use super::super::*;
use crate::app::command_palette::CommandPaletteMode;
use crate::async_runtime::message::{FzfSearchMode, WorkerResultPayload};

pub(super) fn handle_fzf_result(app: &mut AppShell, payload: WorkerResultPayload) {
    if let WorkerResultPayload::FzfResults { query, mode, items } = payload {
        let palette_mode = match mode {
            FzfSearchMode::FindFile => CommandPaletteMode::FilePicker,
            FzfSearchMode::LiveGrep => CommandPaletteMode::LiveGrep,
        };
        let palette_items = items
            .into_iter()
            .map(|item| {
                if let (Some(line), Some(column)) = (item.line, item.column) {
                    crate::app::command_palette::CommandPaletteItem::search_match(
                        item.label,
                        item.preview,
                        item.path,
                        line,
                        column,
                    )
                } else {
                    crate::app::command_palette::CommandPaletteItem::file_match(
                        item.label, item.path,
                    )
                }
            })
            .collect();
        if app
            .app_state
            .set_command_palette_results(palette_mode, &query, palette_items)
        {
            app.editor_needs_layout = true;
            app.editor_caret_needs_layout = false;
            app.submit_fuzzy_picker_preview_load();
            app.request_redraw();
        }
    }
}
