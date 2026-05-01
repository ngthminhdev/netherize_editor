use std::path::PathBuf;

pub(super) async fn run_git_blame_line(
    workspace_root: PathBuf,
    file_path: PathBuf,
    line_number: usize,
) -> Result<String, String> {
    use tokio::process::Command;

    let line_spec = format!("{line_number},{line_number}");
    let file_arg = file_path.to_string_lossy().to_string();
    let output = Command::new("git")
        .kill_on_drop(true)
        .args(["blame", "-L", &line_spec, "--porcelain", &file_arg])
        .current_dir(&workspace_root)
        .output()
        .await
        .map_err(|err| format!("git blame failed: {err}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        if stderr.is_empty() {
            return Err(format!("git blame exited with status {}", output.status));
        }
        return Err(format!(
            "git blame exited with status {}: {stderr}",
            output.status
        ));
    }

    parse_git_blame_summary(&String::from_utf8_lossy(&output.stdout))
}

pub(super) fn parse_git_blame_summary(stdout: &str) -> Result<String, String> {
    let mut author: Option<String> = None;
    let mut author_time: Option<u64> = None;

    for line in stdout.lines() {
        if author.is_none()
            && let Some(value) = line.strip_prefix("author ")
        {
            author = Some(value.trim().to_string());
            continue;
        }
        if author_time.is_none()
            && let Some(value) = line.strip_prefix("author-time ")
        {
            author_time = value.trim().parse::<u64>().ok();
        }
        if author.is_some() && author_time.is_some() {
            break;
        }
    }

    let author = author.unwrap_or_else(|| "Unknown".to_string());
    let relative = author_time
        .map(format_relative_unix_time)
        .unwrap_or_else(|| "unknown time".to_string());
    Ok(format!("{author}, {relative}"))
}

fn format_relative_unix_time(timestamp: u64) -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let delta = now.saturating_sub(timestamp);

    match delta {
        0..=59 => "just now".to_string(),
        60..=3_599 => format_relative_duration(delta / 60, "minute"),
        3_600..=86_399 => format_relative_duration(delta / 3_600, "hour"),
        86_400..=604_799 => format_relative_duration(delta / 86_400, "day"),
        604_800..=2_592_000 => format_relative_duration(delta / 604_800, "week"),
        2_592_001..=31_536_000 => format_relative_duration(delta / 2_592_000, "month"),
        _ => format_relative_duration(delta / 31_536_000, "year"),
    }
}

fn format_relative_duration(value: u64, unit: &str) -> String {
    if value <= 1 {
        format!("1 {unit} ago")
    } else {
        format!("{value} {unit}s ago")
    }
}

pub(super) async fn run_workspace_git_status(
    workspace_root: PathBuf,
) -> Result<Vec<(PathBuf, crate::async_runtime::message::GitFileStatus)>, String> {
    use tokio::process::Command;

    let output = Command::new("git")
        .kill_on_drop(true)
        .args(["status", "--porcelain", "--untracked-files=all"])
        .current_dir(&workspace_root)
        .output()
        .await
        .map_err(|err| format!("git status failed: {err}"))?;

    if !output.status.success() {
        return Ok(Vec::new());
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut statuses = Vec::new();
    for line in stdout.lines() {
        if line.len() < 4 {
            continue;
        }
        let status_code = &line[..2];
        let rel = line[3..].trim();
        let rel = rel.split(" -> ").last().unwrap_or(rel).trim();
        let Some(kind) = parse_git_file_status(status_code) else {
            continue;
        };
        let path = workspace_root.join(rel);
        let normalized = path.canonicalize().unwrap_or(path);
        statuses.push((normalized, kind));
    }
    Ok(statuses)
}

fn parse_git_file_status(
    status_code: &str,
) -> Option<crate::async_runtime::message::GitFileStatus> {
    if status_code.contains('A') || status_code == "??" {
        Some(crate::async_runtime::message::GitFileStatus::Added)
    } else if status_code.contains('M') {
        Some(crate::async_runtime::message::GitFileStatus::Modified)
    } else {
        None
    }
}

pub(super) async fn run_fetch_git_baseline(
    workspace_root: PathBuf,
    file_path: PathBuf,
) -> Result<Option<String>, String> {
    use tokio::process::Command;

    let relative_path = file_path
        .strip_prefix(&workspace_root)
        .unwrap_or(file_path.as_path())
        .to_string_lossy()
        .to_string();

    let head_output = Command::new("git")
        .kill_on_drop(true)
        .args(["rev-parse", "--verify", "HEAD"])
        .current_dir(&workspace_root)
        .output()
        .await
        .map_err(|err| format!("git rev-parse failed: {err}"))?;
    if !head_output.status.success() {
        return Ok(None);
    }

    let output = Command::new("git")
        .kill_on_drop(true)
        .args(["show", &format!("HEAD:{relative_path}")])
        .current_dir(&workspace_root)
        .output()
        .await
        .map_err(|err| format!("git show failed: {err}"))?;

    if output.status.success() {
        Ok(Some(String::from_utf8_lossy(&output.stdout).to_string()))
    } else {
        Ok(Some(String::new()))
    }
}
