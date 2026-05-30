use std::path::PathBuf;

use notify::{Event as NotifyEvent, EventKind as NotifyEventKind, event::ModifyKind};

use crate::async_runtime::{
    message::{FileSystemChangeKind, FileSystemEvent},
    scheduler::{
        file_watch::{extend_unique_file_events, normalize_notify_event},
        fzf::{build_file_preview_lines, build_fzf_find_file_script, build_fzf_live_grep_script},
        git::parse_git_blame_summary,
        runtime::build_worker_runtime,
        session_name_matches_binary,
    },
};

#[test]
fn normalize_create_event_maps_to_internal_create() {
    let raw = NotifyEvent {
        kind: NotifyEventKind::Create(notify::event::CreateKind::File),
        paths: vec![PathBuf::from("/tmp/a.rs")],
        attrs: Default::default(),
    };
    let mapped = normalize_notify_event(raw);

    assert_eq!(mapped.len(), 1);
    assert_eq!(mapped[0].kind, FileSystemChangeKind::Create);
    assert_eq!(mapped[0].path, PathBuf::from("/tmp/a.rs"));
}

#[test]
fn normalize_rename_event_maps_old_and_new_paths() {
    let raw = NotifyEvent {
        kind: NotifyEventKind::Modify(ModifyKind::Name(notify::event::RenameMode::Both)),
        paths: vec![PathBuf::from("/tmp/old.rs"), PathBuf::from("/tmp/new.rs")],
        attrs: Default::default(),
    };
    let mapped = normalize_notify_event(raw);

    assert_eq!(mapped.len(), 1);
    assert_eq!(mapped[0].kind, FileSystemChangeKind::Rename);
    assert_eq!(mapped[0].path, PathBuf::from("/tmp/old.rs"));
    assert_eq!(mapped[0].new_path, Some(PathBuf::from("/tmp/new.rs")));
}

#[test]
fn normalize_single_path_rename_still_maps_to_rename() {
    let raw = NotifyEvent {
        kind: NotifyEventKind::Modify(ModifyKind::Name(notify::event::RenameMode::From)),
        paths: vec![PathBuf::from("/tmp/old.rs")],
        attrs: Default::default(),
    };
    let mapped = normalize_notify_event(raw);

    assert_eq!(mapped.len(), 1);
    assert_eq!(mapped[0].kind, FileSystemChangeKind::Rename);
    assert_eq!(mapped[0].path, PathBuf::from("/tmp/old.rs"));
    assert_eq!(mapped[0].new_path, None);
}

#[test]
fn extend_unique_file_events_deduplicates_burst_entries() {
    let mut target = Vec::new();
    let event = FileSystemEvent {
        kind: FileSystemChangeKind::Modify,
        path: PathBuf::from("/tmp/demo.rs"),
        new_path: None,
    };

    extend_unique_file_events(&mut target, [event.clone(), event.clone()]);

    assert_eq!(target, vec![event]);
}

#[test]
fn worker_runtime_enables_io_for_tokio_process() {
    let runtime = build_worker_runtime().expect("worker runtime");
    let output = runtime
        .block_on(async {
            tokio::process::Command::new("sh")
                .args(["-lc", "printf ok"])
                .output()
                .await
        })
        .expect("spawn process");

    assert!(output.status.success());
    assert_eq!(String::from_utf8_lossy(&output.stdout), "ok");
}

#[test]
fn lsp_session_lookup_matches_absolute_binary_path() {
    assert!(session_name_matches_binary(
        "/Users/dev/project/.fvm/flutter_sdk/bin/cache/dart-sdk/bin/dart",
        "dart"
    ));
    assert!(session_name_matches_binary(
        "dart",
        "/Users/dev/project/.fvm/flutter_sdk/bin/cache/dart-sdk/bin/dart"
    ));
    assert!(!session_name_matches_binary(
        "/Users/dev/project/.fvm/flutter_sdk/bin/cache/dart-sdk/bin/dart",
        "rust-analyzer"
    ));
}

#[test]
fn file_preview_lines_center_around_target_line() {
    let path = std::env::temp_dir().join(format!(
        "netherize_preview_target_{}.txt",
        std::process::id()
    ));
    let text = (1..=8)
        .map(|line| format!("line {line}"))
        .collect::<Vec<_>>()
        .join("\n");
    std::fs::write(&path, text).expect("write preview file");

    let lines = build_file_preview_lines(&path, 5, Some(6));

    assert_eq!(
        lines
            .iter()
            .map(|line| line.line_number)
            .collect::<Vec<_>>(),
        vec![4, 5, 6, 7, 8]
    );
    assert_eq!(
        lines
            .iter()
            .find(|line| line.is_target)
            .map(|line| line.line_number),
        Some(6)
    );

    let _ = std::fs::remove_file(path);
}

#[test]
fn file_preview_lines_without_target_use_file_start() {
    let path = std::env::temp_dir().join(format!(
        "netherize_preview_plain_{}.txt",
        std::process::id()
    ));
    std::fs::write(&path, "a\nb\nc\nd\n").expect("write preview file");

    let lines = build_file_preview_lines(&path, 2, None);

    assert_eq!(
        lines
            .iter()
            .map(|line| line.line_number)
            .collect::<Vec<_>>(),
        vec![1, 2]
    );
    assert!(!lines.iter().any(|line| line.is_target));

    let _ = std::fs::remove_file(path);
}

#[test]
fn fzf_find_file_script_uses_ripgrep_files_and_ignore_globs() {
    let script = build_fzf_find_file_script();

    assert!(script.contains("rg --files --hidden"));
    assert!(script.contains("--glob '!**/.git/**'"));
    assert!(script.contains("--glob '!**/target/**'"));
    assert!(script.contains("fzf -f \"$1\""));
    assert!(!script.contains("find ."));
}

#[test]
fn fzf_live_grep_script_uses_ripgrep_and_ignore_globs() {
    let script = build_fzf_live_grep_script(false);

    assert!(script.contains("rg --line-number --column --hidden --fixed-strings"));
    assert!(script.contains("--glob '!**/.git/**'"));
    assert!(script.contains("--glob '!**/target/**'"));
    assert!(script.contains("--ignore-case"));
    assert!(script.contains("-- \"$1\" ."));
    assert!(script.contains("fzf -i -f \"$1\""));
}

#[test]
fn parse_git_blame_summary_extracts_author_and_relative_time() {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let stdout = format!(
        "deadbeef 10 10 1\nauthor Jane Dev\nauthor-time {}\nsummary hello\n",
        now.saturating_sub(7_200)
    );

    let summary = parse_git_blame_summary(&stdout).expect("parse blame summary");

    assert!(summary.starts_with("Jane Dev, "));
    assert!(summary.ends_with("ago"));
}
