//! Paths + text templates for the Dojo's side files (current.md for the AI
//! interviewer, the interviewer prompt, the SD outline template).
use std::path::PathBuf;

use super::session::SessionKind;

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

pub fn current_md(
    kind: SessionKind,
    id: u32,
    title: &str,
    statement: &str,
    language: &str,
    phases: &[(String, u32)],
    approach: Option<&str>,
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
    if let Some(a) = approach.filter(|a| !a.trim().is_empty()) {
        out.push_str(&format!("Approach: {}\n", a.trim()));
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
        "# {label} — {date}\n\n## 1. Làm rõ yêu cầu (5')\n\n## 2. Ước lượng quy mô (5')\n\n## 3. API + mô hình dữ liệu (5')\n\n## 4. Kiến trúc mức cao (10')\n\n## 5. Đào sâu 1–2 điểm (15')\n\n## 6. Nút cổ chai + đánh đổi (5')\n\n> Câu hỏi bắt buộc: request này chết giữa chừng thì sao?\n"
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
    fn current_md_and_templates_carry_the_essentials() {
        let phases = vec![("THINK".to_string(), 3), ("CODE".to_string(), 15)];
        let md = current_md(
            SessionKind::Dsa,
            1,
            "Two Sum",
            "Given nums…",
            "javascript",
            &phases,
            Some("hash map"),
        );
        assert!(md.starts_with("# Dojo session\n"));
        assert!(md.contains("kind: dsa"));
        assert!(md.contains("#1 Two Sum"));
        assert!(md.contains("THINK 3'"));
        assert!(md.contains("Approach: hash map"));
        let sd = current_md(SessionKind::Sd, 0, "Rút gọn URL", "", "", &[], None);
        assert!(sd.contains("kind: sd"));
        assert!(!sd.contains("Approach:"));
        assert!(INTERVIEWER_PROMPT.contains("current.md"));
        let t = sd_template("Rút gọn URL", "2026-09-02");
        assert!(t.starts_with("# Rút gọn URL — 2026-09-02\n"));
        assert!(t.contains("## 6. Nút cổ chai + đánh đổi (5')"));
    }
}
