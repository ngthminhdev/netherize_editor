//! Error-notebook formatting (append-only markdown the user reads on Sundays).
use super::state::Outcome;

pub const NOTEBOOK_HEADER: &str = "# Sổ tay lỗi\n\n";

pub fn mm_ss(secs: u64) -> String {
    format!("{:02}:{:02}", secs / 60, secs % 60)
}

/// LeetCode statement HTML → plain text. Block tags become newlines, inline
/// tags vanish, common entities decode, 3+ blank lines collapse to one.
pub fn html_to_text(html: &str) -> String {
    let block = regex::Regex::new(r"(?i)<br\s*/?>|</p>|</li>|</pre>|</div>|</h[1-6]>|</tr>");
    let tags = regex::Regex::new(r"(?s)<[^>]+>");
    let many_newlines = regex::Regex::new(r"\n{3,}");
    let (Ok(block), Ok(tags), Ok(many_newlines)) = (block, tags, many_newlines) else {
        return html.to_string();
    };
    let text = block.replace_all(html, "\n");
    let text = tags.replace_all(&text, "");
    let text = text
        .replace("&quot;", "\"")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&nbsp;", " ")
        .replace("&#39;", "'")
        .replace("&amp;", "&");
    let text = many_newlines.replace_all(&text, "\n\n");
    text.lines()
        .map(str::trim_end)
        .collect::<Vec<_>>()
        .join("\n")
        .trim()
        .to_string()
}

#[allow(clippy::too_many_arguments)]
pub fn format_block(
    date: &str,
    id: u32,
    title: &str,
    outcome: Outcome,
    elapsed_s: u64,
    redo: bool,
    approach: &str,
    answers: &[(&str, &str)],
) -> String {
    let mut out = format!(
        "## {date} · #{id} {title} · {} {}",
        outcome.label(),
        mm_ss(elapsed_s)
    );
    if redo {
        out.push_str(" · #redo");
    }
    out.push('\n');
    if !approach.trim().is_empty() {
        out.push_str(&format!("- Hướng: {}\n", approach.trim()));
    }
    for (label, answer) in answers {
        if !answer.trim().is_empty() {
            out.push_str(&format!("- {label}: {}\n", answer.trim()));
        }
    }
    out.push('\n');
    out
}

pub fn format_sd_block(date: &str, label: &str, elapsed_s: u64, note: &str) -> String {
    let mut out = format!("## {date} · SD · {label} · {}\n", mm_ss(elapsed_s));
    if !note.trim().is_empty() {
        out.push_str(&format!("- Ghi chú: {}\n", note.trim()));
    }
    out.push('\n');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_tags_and_decodes_entities() {
        let html = "<p>Given <code>nums</code> &amp; <strong>target</strong>.</p><pre>Input: nums = [2,7]\nOutput: [0,1]</pre><ul><li>1 &lt;= n</li></ul>";
        let text = html_to_text(html);
        assert_eq!(
            text,
            "Given nums & target.\nInput: nums = [2,7]\nOutput: [0,1]\n1 <= n"
        );
        assert_eq!(html_to_text("a<br/>b<br>c"), "a\nb\nc");
        assert_eq!(html_to_text("<p>x</p>\n\n\n\n<p>y</p>"), "x\n\ny");
    }

    #[test]
    fn formats_time_and_blocks() {
        assert_eq!(mm_ss(0), "00:00");
        assert_eq!(mm_ss(754), "12:34");
        let block = format_block(
            "2026-09-02",
            3,
            "Longest Substring",
            Outcome::Timeout,
            1500,
            true,
            "sliding window + set, O(n)",
            &[
                ("Bí", "quên co cửa sổ"),
                ("Pattern", "window co giãn"),
                ("Dấu hiệu", ""),
            ],
        );
        assert_eq!(
            block,
            "## 2026-09-02 · #3 Longest Substring · timeout 25:00 · #redo\n- Hướng: sliding window + set, O(n)\n- Bí: quên co cửa sổ\n- Pattern: window co giãn\n\n"
        );
        let pass = format_block(
            "2026-09-02",
            1,
            "Two Sum",
            Outcome::Pass,
            760,
            false,
            "",
            &[("Ghi chú", "")],
        );
        assert_eq!(pass, "## 2026-09-02 · #1 Two Sum · pass 12:40\n\n");
        assert_eq!(
            format_sd_block("2026-09-02", "Rút gọn URL", 2700, "ok"),
            "## 2026-09-02 · SD · Rút gọn URL · 45:00\n- Ghi chú: ok\n\n"
        );
    }
}
