use super::super::*;
use crate::async_runtime::message::WorkerResultPayload;

/// Fire-and-forget text writes (notebook, current.md, SD outline). Success is
/// silent; each failure gets its own toast so nothing is lost quietly.
pub(super) fn handle_text_files_written(app: &mut AppShell, payload: WorkerResultPayload) {
    let WorkerResultPayload::TextFilesWritten { failures } = payload else {
        return;
    };
    for (path, err) in failures {
        app.show_transient_toast_kind(
            format!("Dojo\nKhông ghi được {}: {err}", path.display()),
            ToastKind::Error,
        );
    }
    app.request_redraw();
}
