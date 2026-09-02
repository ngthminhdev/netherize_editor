//! LeetCode statement HTML → styled lines for the Problem tab. Only the
//! handful of tags LeetCode uses matter: `p`/`div`/`ul`/`li`/`br` for
//! structure, `code`/`strong`/`pre` for emphasis, `sup`/`sub` for exponents.
//! Everything else is dropped, entities are decoded.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Style {
    Plain,
    Code,
    Bold,
    Pre,
}

/// One run of text in a single style.
pub type Run = (String, Style);
/// One visual line; wrapping happens later, per panel width.
pub type Line = Vec<Run>;

struct Builder {
    lines: Vec<Line>,
    current: Vec<(char, Style)>,
    pre: bool,
    code: usize,
    bold: usize,
    /// Pending whitespace outside `<pre>` (collapsed to one space), with
    /// the style that was active when it was seen.
    space_pending: Option<Style>,
}

impl Builder {
    fn style(&self) -> Style {
        if self.pre {
            Style::Pre
        } else if self.code > 0 {
            Style::Code
        } else if self.bold > 0 {
            Style::Bold
        } else {
            Style::Plain
        }
    }

    fn push_char(&mut self, ch: char) {
        if self.pre {
            if ch == '\n' {
                self.end_line();
            } else {
                self.current.push((ch, Style::Pre));
            }
            return;
        }
        if ch.is_whitespace() {
            if !self.current.is_empty() {
                self.space_pending = Some(self.style());
            }
            return;
        }
        if let Some(style) = self.space_pending.take() {
            self.current.push((' ', style));
        }
        self.current.push((ch, self.style()));
    }

    fn push_str(&mut self, text: &str) {
        for ch in text.chars() {
            self.push_char(ch);
        }
    }

    fn end_line(&mut self) {
        self.space_pending = None;
        while self.current.last().is_some_and(|(c, _)| *c == ' ') {
            self.current.pop();
        }
        let mut line: Line = Vec::new();
        for (ch, style) in self.current.drain(..) {
            match line.last_mut() {
                Some((text, last)) if *last == style => text.push(ch),
                _ => line.push((ch.to_string(), style)),
            }
        }
        self.lines.push(line);
    }

    fn blank_line(&mut self) {
        if !self.current.is_empty() {
            self.end_line();
        }
        if self.lines.last().is_some_and(|l| !l.is_empty()) {
            self.lines.push(Vec::new());
        }
    }
}

fn decode_entities(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(idx) = rest.find('&') {
        out.push_str(&rest[..idx]);
        let tail = &rest[idx..];
        let Some(end) = tail.find(';').filter(|e| *e <= 8) else {
            out.push('&');
            rest = &tail[1..];
            continue;
        };
        let entity = &tail[1..end];
        let decoded = match entity {
            "nbsp" => Some(' '),
            "lt" => Some('<'),
            "gt" => Some('>'),
            "amp" => Some('&'),
            "quot" => Some('"'),
            "apos" => Some('\''),
            _ => entity
                .strip_prefix('#')
                .and_then(|n| {
                    if let Some(hex) = n.strip_prefix('x').or_else(|| n.strip_prefix('X')) {
                        u32::from_str_radix(hex, 16).ok()
                    } else {
                        n.parse::<u32>().ok()
                    }
                })
                .and_then(char::from_u32),
        };
        match decoded {
            Some(ch) => out.push(ch),
            None => out.push_str(&tail[..=end]),
        }
        rest = &tail[end + 1..];
    }
    out.push_str(rest);
    out
}

/// Parse LeetCode statement HTML into styled lines. Consecutive blank lines
/// collapse to one; leading/trailing blank lines are dropped.
pub fn html_to_lines(html: &str) -> Vec<Line> {
    let mut b = Builder {
        lines: Vec::new(),
        current: Vec::new(),
        pre: false,
        code: 0,
        bold: 0,
        space_pending: None,
    };
    let mut rest = html;
    while !rest.is_empty() {
        let Some(open) = rest.find('<') else {
            b.push_str(&decode_entities(rest));
            break;
        };
        b.push_str(&decode_entities(&rest[..open]));
        let tail = &rest[open..];
        let Some(close) = tail.find('>') else {
            b.push_str(&decode_entities(tail));
            break;
        };
        let raw = &tail[1..close];
        rest = &tail[close + 1..];
        let closing = raw.starts_with('/');
        let name: String = raw
            .trim_start_matches('/')
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric())
            .collect::<String>()
            .to_ascii_lowercase();
        match (name.as_str(), closing) {
            ("p", false)
            | ("div", false)
            | ("ul", false)
            | ("ol", false)
            | ("table", false)
            | ("tr", false) => {
                if !b.current.is_empty() {
                    b.end_line();
                }
            }
            ("p", true) => b.blank_line(),
            ("div", true) | ("ul", true) | ("ol", true) | ("table", true) | ("tr", true) => {
                if !b.current.is_empty() {
                    b.end_line();
                }
            }
            ("br", _) => b.end_line(),
            ("li", false) => {
                if !b.current.is_empty() {
                    b.end_line();
                }
                b.current.push(('•', Style::Plain));
                b.current.push((' ', Style::Plain));
            }
            ("li", true) => b.end_line(),
            ("pre", false) => {
                if !b.current.is_empty() {
                    b.end_line();
                }
                b.pre = true;
            }
            ("pre", true) => {
                b.pre = false;
                b.blank_line();
            }
            ("code", false) => b.code += 1,
            ("code", true) => b.code = b.code.saturating_sub(1),
            ("strong", false) | ("b", false) => b.bold += 1,
            ("strong", true) | ("b", true) => b.bold = b.bold.saturating_sub(1),
            ("sup", false) => b.push_char('^'),
            ("sub", false) => b.push_char('_'),
            (h, false) if h.len() == 2 && h.starts_with('h') => {
                if !b.current.is_empty() {
                    b.end_line();
                }
                b.bold += 1;
            }
            (h, true) if h.len() == 2 && h.starts_with('h') => {
                b.bold = b.bold.saturating_sub(1);
                b.blank_line();
            }
            _ => {}
        }
    }
    if !b.current.is_empty() {
        b.end_line();
    }
    // Collapse blank runs, trim the ends.
    let mut out: Vec<Line> = Vec::new();
    for line in b.lines {
        if line.is_empty() && out.last().is_none_or(|l| l.is_empty()) {
            continue;
        }
        out.push(line);
    }
    while out.last().is_some_and(|l| l.is_empty()) {
        out.pop();
    }
    out
}

/// Greedy word wrap of one styled line; runs keep their style across breaks.
pub fn wrap_line(line: &Line, max_chars: usize) -> Vec<Line> {
    let max = max_chars.max(1);
    let chars: Vec<(char, Style)> = line
        .iter()
        .flat_map(|(text, style)| text.chars().map(move |c| (c, *style)))
        .collect();
    if chars.is_empty() {
        return vec![Vec::new()];
    }
    // Split into words (runs of non-space chars); spaces separate words.
    let mut words: Vec<Vec<(char, Style)>> = Vec::new();
    let mut word: Vec<(char, Style)> = Vec::new();
    for (c, s) in chars {
        if c == ' ' {
            if !word.is_empty() {
                words.push(std::mem::take(&mut word));
            }
        } else {
            word.push((c, s));
        }
    }
    if !word.is_empty() {
        words.push(word);
    }
    let mut lines: Vec<Vec<(char, Style)>> = Vec::new();
    let mut current: Vec<(char, Style)> = Vec::new();
    for mut word in words {
        while word.len() > max {
            if !current.is_empty() {
                lines.push(std::mem::take(&mut current));
            }
            lines.push(word.drain(..max).collect());
        }
        let needed = if current.is_empty() {
            word.len()
        } else {
            current.len() + 1 + word.len()
        };
        if needed > max && !current.is_empty() {
            lines.push(std::mem::take(&mut current));
        }
        if !current.is_empty() {
            current.push((' ', Style::Plain));
        }
        current.extend(word);
    }
    if !current.is_empty() {
        lines.push(current);
    }
    lines
        .into_iter()
        .map(|chars| {
            let mut runs: Line = Vec::new();
            for (ch, style) in chars {
                match runs.last_mut() {
                    Some((text, last)) if *last == style => text.push(ch),
                    _ => runs.push((ch.to_string(), style)),
                }
            }
            runs
        })
        .collect()
}

/// Plain text of a line (tests, current.md).
pub fn line_text(line: &Line) -> String {
    line.iter().map(|(t, _)| t.as_str()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "<p>Given an integer array <code>nums</code>, return <code>true</code> if any value appears <strong>at least twice</strong>.</p>\n\n<p>&nbsp;</p>\n<p><strong class=\"example\">Example 1:</strong></p>\n\n<div class=\"example-block\">\n<p><strong>Input:</strong> <span class=\"example-io\">nums = [1,2,3,1]</span></p>\n\n<p><strong>Output:</strong> <span class=\"example-io\">true</span></p>\n</div>\n\n<p><strong>Constraints:</strong></p>\n\n<ul>\n\t<li><code>1 &lt;= nums.length &lt;= 10<sup>5</sup></code></li>\n\t<li><code>-10<sup>9</sup> &lt;= nums[i] &lt;= 10<sup>9</sup></code></li>\n</ul>\n";

    #[test]
    fn statement_keeps_code_and_bold_runs_and_bullets() {
        let lines = html_to_lines(SAMPLE);
        let text: Vec<String> = lines.iter().map(line_text).collect();
        assert_eq!(
            text,
            vec![
                "Given an integer array nums, return true if any value appears at least twice.",
                "",
                "Example 1:",
                "",
                "Input: nums = [1,2,3,1]",
                "",
                "Output: true",
                "",
                "Constraints:",
                "",
                "• 1 <= nums.length <= 10^5",
                "• -10^9 <= nums[i] <= 10^9",
            ]
        );
        assert_eq!(
            lines[0][..3],
            [
                ("Given an integer array ".to_string(), Style::Plain),
                ("nums".to_string(), Style::Code),
                (", return ".to_string(), Style::Plain),
            ]
        );
        assert!(lines[0].contains(&("at least twice".to_string(), Style::Bold)));
        assert_eq!(lines[2], vec![("Example 1:".to_string(), Style::Bold)]);
        assert_eq!(lines[10][0], ("• ".to_string(), Style::Plain));
        assert_eq!(lines[10][1].1, Style::Code);
    }

    #[test]
    fn pre_blocks_keep_their_lines_and_entities_decode() {
        let lines = html_to_lines(
            "<p>Ex:</p><pre>Input: s = &quot;a b&quot;\nOutput: 2</pre><p>x &amp; y &#39;z&#39; &#x41;</p>",
        );
        let text: Vec<String> = lines.iter().map(line_text).collect();
        assert_eq!(
            text,
            vec![
                "Ex:",
                "",
                "Input: s = \"a b\"",
                "Output: 2",
                "",
                "x & y 'z' A"
            ]
        );
        assert!(lines[2].iter().all(|(_, s)| *s == Style::Pre));
        assert_eq!(
            html_to_lines("<p>&nbsp;</p><p>&nbsp;</p>"),
            Vec::<Line>::new()
        );
        assert_eq!(html_to_lines("a<br/>b<br>c").len(), 3);
    }

    #[test]
    fn wrap_keeps_styles_across_breaks() {
        let line: Line = vec![
            ("Given ".to_string(), Style::Plain),
            ("nums".to_string(), Style::Code),
            (" and target".to_string(), Style::Plain),
        ];
        let wrapped = wrap_line(&line, 11);
        let text: Vec<String> = wrapped.iter().map(line_text).collect();
        assert_eq!(text, vec!["Given nums", "and target"]);
        assert_eq!(wrapped[0][1], ("nums".to_string(), Style::Code));
        assert_eq!(wrap_line(&Vec::new(), 10), vec![Vec::<Run>::new()]);
        let long: Line = vec![("abcdefghij".to_string(), Style::Code)];
        assert_eq!(
            wrap_line(&long, 4)
                .iter()
                .map(line_text)
                .collect::<Vec<_>>(),
            vec!["abcd", "efgh", "ij"]
        );
    }
}
