use super::super::*;
use crate::async_runtime::message::WorkerResultPayload;

pub(super) fn handle_shell_result(_app: &mut AppShell, payload: WorkerResultPayload) {
    if let WorkerResultPayload::DetachedShellCommandSpawned { command, pid } = payload {
        eprintln!(
            "[AppShell] background shell command started pid={:?}: {}",
            pid, command
        );
    }
}
