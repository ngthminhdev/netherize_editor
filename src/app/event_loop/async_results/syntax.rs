use super::super::*;
use crate::async_runtime::message::WorkerResultPayload;
use std::path::PathBuf;

pub(super) fn handle_syntax_result(app: &mut AppShell, payload: WorkerResultPayload) {
    match payload {
        WorkerResultPayload::ParseAndHighlight {
            buffer_id,
            file_path,
            spans,
            buffer_revision,
            covered_byte_range,
            foldable_ranges,
            ..
        } => {
            let active_buffer_id = app.app_state.active_file().map(PathBuf::from);
            if active_buffer_id.as_ref() != Some(&buffer_id) {
                return;
            }
            if buffer_revision != app.app_state.revision() {
                return;
            }
            if file_path != active_buffer_id {
                return;
            }

            let covered_byte_range = covered_byte_range.map(|window| {
                crate::syntax::highlight::expand_merge_window(&app.highlight_spans, &spans, window)
            });

            crate::syntax::highlight::merge_highlight_spans(
                &mut app.highlight_spans,
                spans,
                covered_byte_range,
            );
            app.app_state.set_foldable_ranges_cache(foldable_ranges);
            app.app_state.auto_fold_pathological_long_lines();
            app.editor_needs_layout = true;
            app.editor_caret_needs_layout = false;
            app.request_redraw();
        }
        _ => {}
    }
}
