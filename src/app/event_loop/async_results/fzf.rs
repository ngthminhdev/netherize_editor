use super::super::*;
use crate::app::command_palette::CommandPaletteMode;
use crate::async_runtime::message::{FzfSearchMode, WorkerResultPayload};

pub(super) fn handle_fzf_result(app: &mut AppShell, payload: WorkerResultPayload) {
    if let WorkerResultPayload::FzfResults {
        query,
        mode,
        case_sensitive,
        items,
    } = payload
    {
        if mode == FzfSearchMode::LiveGrep
            && case_sensitive != app.app_state.live_grep_case_sensitive()
        {
            return;
        }
        let palette_mode = match mode {
            FzfSearchMode::FindFile => CommandPaletteMode::FilePicker,
            FzfSearchMode::LiveGrep => CommandPaletteMode::LiveGrep,
        };
        let mut raw_items = items;
        if mode == FzfSearchMode::LiveGrep {
            raw_items.sort_by(|a, b| {
                a.path
                    .cmp(&b.path)
                    .then_with(|| a.line.unwrap_or(0).cmp(&b.line.unwrap_or(0)))
                    .then_with(|| a.column.unwrap_or(0).cmp(&b.column.unwrap_or(0)))
            });
        }
        let palette_items = raw_items
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
