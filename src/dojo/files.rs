//! Paths + text templates for the Dojo's files: the per-problem folder inside
//! the user's LeetCode workspace, the notebook, SD outlines, and the side
//! files the AI interviewer reads.
use std::path::{Path, PathBuf};

use super::{plan::Plan, session::SessionKind};

pub const INTERVIEWER_PROMPT: &str = include_str!("interviewer_prompt.md");

pub fn expand_tilde(path: &str) -> PathBuf {
    if let Some(rest) = path.strip_prefix("~/") {
        let home = std::env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("."));
        return home.join(rest);
    }
    PathBuf::from(path)
}

pub fn dojo_dir() -> PathBuf {
    crate::config::paths::user_config_root().join("dojo")
}

pub fn current_md_path() -> PathBuf {
    dojo_dir().join("current.md")
}

pub fn interviewer_md_path() -> PathBuf {
    dojo_dir().join("interviewer.md")
}

/// `<workspace>/0001-two-sum` — one folder per problem, so the Explorer of the
/// workspace lists exactly the problems that were attempted.
pub fn problem_dir(workspace: &Path, id: u32, slug: &str) -> PathBuf {
    workspace.join(format!("{id:04}-{slug}"))
}

pub fn notebook_path(plan: &Plan, workspace: &Path) -> PathBuf {
    plan.notebook
        .as_deref()
        .map(expand_tilde)
        .unwrap_or_else(|| workspace.join("notes.md"))
}

pub fn sd_dir(plan: &Plan, workspace: &Path) -> PathBuf {
    plan.sd_dir
        .as_deref()
        .map(expand_tilde)
        .unwrap_or_else(|| workspace.join("sd"))
}

pub fn current_md(
    kind: SessionKind,
    id: u32,
    title: &str,
    statement: &str,
    language: &str,
    phases: &[(String, u32)],
) -> String {
    let kind_label = match kind {
        SessionKind::Dsa => "dsa",
        SessionKind::Sd => "sd",
    };
    let mut out = format!("# Dojo session\nkind: {kind_label}\n");
    if id > 0 {
        out.push_str(&format!("problem: #{id} {title}\n"));
    } else {
        out.push_str(&format!("case: {title}\n"));
    }
    if !language.is_empty() {
        out.push_str(&format!("language: {language}\n"));
    }
    if !phases.is_empty() {
        let budget: Vec<String> = phases.iter().map(|(n, m)| format!("{n} {m}'")).collect();
        out.push_str(&format!("phases: {}\n", budget.join(" → ")));
    }
    if !statement.trim().is_empty() {
        out.push_str("\n## Statement\n");
        out.push_str(statement.trim());
        out.push('\n');
    }
    out
}

pub fn sd_template(label: &str, date: &str) -> String {
    format!(
        "# {label} — {date}\n\n## 1. Clarify requirements (5')\n\n## 2. Estimate scale (5')\n\n## 3. API + data model (5')\n\n## 4. High-level design (10')\n\n## 5. Deep dive 1–2 parts (15')\n\n## 6. Bottlenecks + trade-offs (5')\n\n> Mandatory question: what happens when this request dies halfway?\n"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tilde_expands_to_home() {
        let home = std::env::var("HOME").expect("HOME");
        assert_eq!(
            expand_tilde("~/Work/x.md"),
            PathBuf::from(format!("{home}/Work/x.md"))
        );
        assert_eq!(expand_tilde("/abs/x"), PathBuf::from("/abs/x"));
        assert!(dojo_dir().ends_with("dojo"));
        assert!(current_md_path().ends_with("dojo/current.md"));
    }

    #[test]
    fn workspace_paths() {
        let ws = Path::new("/tmp/lc");
        assert_eq!(
            problem_dir(ws, 1, "two-sum"),
            PathBuf::from("/tmp/lc/0001-two-sum")
        );
        assert_eq!(
            problem_dir(ws, 1143, "longest-common-subsequence"),
            PathBuf::from("/tmp/lc/1143-longest-common-subsequence")
        );
        let plan = Plan::bundled();
        assert_eq!(notebook_path(&plan, ws), PathBuf::from("/tmp/lc/notes.md"));
        assert_eq!(sd_dir(&plan, ws), PathBuf::from("/tmp/lc/sd"));
        let custom = Plan {
            notebook: Some("/abs/n.md".to_string()),
            sd_dir: Some("~/sd".to_string()),
            ..Plan::bundled()
        };
        assert_eq!(notebook_path(&custom, ws), PathBuf::from("/abs/n.md"));
        assert!(sd_dir(&custom, ws).ends_with("sd"));
        assert!(!sd_dir(&custom, ws).starts_with("/tmp/lc"));
    }

    #[test]
    fn current_md_and_templates_carry_the_essentials() {
        let phases = vec![("THINK".to_string(), 3), ("CODE".to_string(), 15)];
        let md = current_md(
            SessionKind::Dsa,
            1,
            "Two Sum",
            "Given nums…",
            "javascript",
            &phases,
        );
        assert!(md.starts_with("# Dojo session\n"));
        assert!(md.contains("kind: dsa"));
        assert!(md.contains("#1 Two Sum"));
        assert!(md.contains("THINK 3'"));
        assert!(md.contains("## Statement\nGiven nums…"));
        let sd = current_md(SessionKind::Sd, 0, "URL shortener", "", "", &[]);
        assert!(sd.contains("kind: sd"));
        assert!(sd.contains("case: URL shortener"));
        assert!(INTERVIEWER_PROMPT.contains("current.md"));
        let t = sd_template("URL shortener", "2026-09-02");
        assert!(t.starts_with("# URL shortener — 2026-09-02\n"));
        assert!(t.contains("## 6. Bottlenecks + trade-offs (5')"));
    }
}
