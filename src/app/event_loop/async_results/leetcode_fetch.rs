use super::super::*;
use crate::async_runtime::message::WorkerResultPayload;

pub(super) fn handle_leetcode_generate_result(app: &mut AppShell, payload: WorkerResultPayload) {
    match payload {
        WorkerResultPayload::LeetCodeTestsGenerated {
            id: _,
            cases,
            verified,
        } => {
            app.app_state.test_runner.is_generating = false;
            app.app_state.test_runner.cases = cases
                .into_iter()
                .map(|case| crate::runner::TestCase::new_ai(case.input, case.expected))
                .collect();
            app.app_state.test_runner.selected =
                (!app.app_state.test_runner.cases.is_empty()).then_some(0);
            app.app_state.test_runner.focused_field = crate::runner::TestField::Input;
            app.app_state.test_runner.is_running = false;
            app.app_state.test_runner.launch_error = None;
            let suffix = if verified {
                " (verified)"
            } else {
                " — review Expected, then F5"
            };
            app.show_transient_toast_kind(
                format!(
                    "Generate LeetCode Tests\n{} AI test cases{suffix}.",
                    app.app_state.test_runner.cases.len()
                ),
                ToastKind::Success,
            );
            app.request_redraw();
        }
        WorkerResultPayload::LeetCodeTestsGenerateFailed { message } => {
            app.app_state.test_runner.is_generating = false;
            app.show_transient_toast_kind(
                format!("Generate LeetCode Tests\n{message}"),
                ToastKind::Error,
            );
            app.request_redraw();
        }
        _ => {}
    }
}

pub(super) fn handle_leetcode_fetch_result(app: &mut AppShell, payload: WorkerResultPayload) {
    let WorkerResultPayload::LeetCodeProblemFetched {
        title,
        title_slug,
        language_key,
        file_path,
        cases,
    } = payload
    else {
        if let WorkerResultPayload::LeetCodeProblemFetchFailed { message } = payload {
            app.show_transient_toast_kind(
                format!("Fetch LeetCode Problem\n{message}"),
                ToastKind::Error,
            );
            app.request_redraw();
        }
        return;
    };

    let report = dispatch_command(&mut app.app_state, Command::OpenFile(file_path.clone()));
    if !report.success {
        app.show_transient_toast_kind(
            format!(
                "Fetch LeetCode Problem\nCould not open {}",
                file_path.display()
            ),
            ToastKind::Error,
        );
        app.request_redraw();
        return;
    }

    let total_cases = cases.len();
    let missing_expected = cases
        .iter()
        .filter(|case| case.expected.trim().is_empty())
        .count();
    app.app_state.test_runner.cases = cases
        .into_iter()
        .map(|case| crate::runner::TestCase::new(case.input, case.expected))
        .collect();
    app.app_state.test_runner.selected = (!app.app_state.test_runner.cases.is_empty()).then_some(0);
    app.app_state.test_runner.focused_field = crate::runner::TestField::Input;
    app.app_state.test_runner.is_running = false;
    app.app_state.test_runner.launch_error = None;

    app.clear_highlight_layers();
    app.submit_workspace_rescan();
    app.submit_active_buffer_git_baseline_refresh();
    app.submit_parse_for_active_buffer(true);
    app.submit_lsp_did_open_for_active_file();
    app.explorer_reveal_file(&file_path);
    app.submit_lsp_check_for_path(file_path.clone());
    let _ = app.release_focus_mode_to_editor();
    app.panel_state.right.visible = true;
    app.panel_state
        .right
        .switch_to_tab(crate::workbench::panel_state::PanelTabId::TestRunner);
    app.focus_manager.set(FocusTarget::RightSidebar);
    app.editor_needs_layout = true;
    app.editor_caret_needs_layout = false;
    let hint = if missing_expected > 0 {
        format!(
            "\n⚠ {missing_expected}/{total_cases} case chưa parse được Output — điền Expected bằng tay nhé."
        )
    } else {
        String::new()
    };
    app.show_transient_toast_kind(
        format!(
            "Fetch LeetCode Problem\n{} ({}) · {} · {} cases — press F5 to run.{}",
            title,
            title_slug,
            language_key,
            app.app_state.test_runner.cases.len(),
            hint
        ),
        ToastKind::Success,
    );
    app.request_redraw();
}
