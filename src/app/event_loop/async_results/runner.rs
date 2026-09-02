use super::super::*;
use crate::async_runtime::message::WorkerResultPayload;

/// Fold the worker's per-case outcomes back into `test_runner` state and judge
/// pass/fail. Outcomes are index-aligned with the cases that were submitted; a
/// mismatched length (cases edited mid-run) is tolerated by zipping.
pub(super) fn handle_test_cases_completed(app: &mut AppShell, payload: WorkerResultPayload) {
    let WorkerResultPayload::TestCasesCompleted {
        command_preview,
        outcomes,
    } = payload
    else {
        return;
    };

    let runner = &mut app.app_state.test_runner;
    runner.is_running = false;
    runner.launch_error = None;
    runner.last_command_preview = Some(command_preview);

    for (case, outcome) in runner.cases.iter_mut().zip(outcomes.into_iter()) {
        case.apply_outcome(outcome);
    }

    let total = runner.len();
    let passed = runner.passed_count();
    let kind = if passed == total {
        ToastKind::Success
    } else {
        ToastKind::Error
    };
    app.show_transient_toast_kind(format!("Test Runner\n{passed}/{total} passed"), kind);
    app.dojo_on_run_completed(total > 0 && passed == total);
    app.editor_needs_layout = true;
    app.request_redraw();
}
