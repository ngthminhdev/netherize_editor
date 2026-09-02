//! Pure view models: the category tree for the left dock, the problem panel
//! for the right dock, the welcome card. No renderer, no I/O.
use chrono::NaiveDate;

use crate::runner::leetcode_cache::LeetCodeProblemCache;

use super::{
    notebook::mm_ss,
    plan::Plan,
    problems::{Problem, Problems, category_label},
    session::SessionKind,
    state::{DojoState, Status, parse_date},
    statement::{Line, html_to_lines},
};

/// Tree key of the System Design group (not a LeetCode category).
pub const SD_GROUP_KEY: &str = "sd";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RowGlyph {
    RedoDue,
    RedoLater,
    Todo,
    Done,
}

impl RowGlyph {
    pub fn symbol(self) -> &'static str {
        match self {
            Self::RedoDue => "↻",
            Self::RedoLater => "·",
            Self::Todo => "○",
            Self::Done => "●",
        }
    }
}

/// One visible row of the left-dock tree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TreeRow {
    Group {
        key: String,
        label: String,
        done: usize,
        total: usize,
        expanded: bool,
    },
    Problem {
        slug: String,
        id: u32,
        title: String,
        difficulty: String,
        glyph: RowGlyph,
        trailing: String,
    },
    SdGroup {
        done: usize,
        total: usize,
        expanded: bool,
    },
    SdCase {
        key: String,
        label: String,
        done: bool,
    },
}

impl TreeRow {
    /// Group key, problem slug, or SD case key — what selection remembers.
    pub fn key(&self) -> &str {
        match self {
            Self::Group { key, .. } | Self::SdCase { key, .. } => key,
            Self::Problem { slug, .. } => slug,
            Self::SdGroup { .. } => SD_GROUP_KEY,
        }
    }

    pub fn is_group(&self) -> bool {
        matches!(self, Self::Group { .. } | Self::SdGroup { .. })
    }

    pub fn group_key(&self) -> Option<&str> {
        match self {
            Self::Group { key, .. } => Some(key),
            Self::SdGroup { .. } => Some(SD_GROUP_KEY),
            _ => None,
        }
    }
}

/// Easy < Medium < Hard; unknown strings sort with Medium.
pub fn difficulty_rank(difficulty: &str) -> u8 {
    match difficulty {
        "easy" => 0,
        "hard" => 2,
        _ => 1,
    }
}

pub fn difficulty_letter(difficulty: &str) -> char {
    match difficulty {
        "easy" => 'E',
        "hard" => 'H',
        _ => 'M',
    }
}

pub fn difficulty_label(difficulty: &str) -> &'static str {
    match difficulty {
        "easy" => "Easy",
        "hard" => "Hard",
        _ => "Medium",
    }
}

/// (glyph, trailing text) for a problem's progress.
pub fn status_of(state: &DojoState, slug: &str, today: NaiveDate) -> (RowGlyph, String) {
    let progress = state.progress_of(slug);
    match progress.status {
        Status::Done => (
            RowGlyph::Done,
            state.best_pass_secs(slug).map(mm_ss).unwrap_or_default(),
        ),
        Status::Redo if state.is_due(slug, today) => (RowGlyph::RedoDue, "redo".to_string()),
        Status::Redo => (
            RowGlyph::RedoLater,
            progress
                .redo_at
                .as_deref()
                .and_then(parse_date)
                .map(|d| d.format("%d/%m").to_string())
                .unwrap_or_default(),
        ),
        Status::Todo => (RowGlyph::Todo, String::new()),
    }
}

fn problem_row(p: &Problem, state: &DojoState, today: NaiveDate) -> TreeRow {
    let (glyph, trailing) = status_of(state, &p.slug, today);
    TreeRow::Problem {
        slug: p.slug.clone(),
        id: p.id,
        title: p.title.clone(),
        difficulty: p.difficulty.clone(),
        glyph,
        trailing,
    }
}

/// Due redos in problem-file order.
pub fn due_rows(problems: &Problems, state: &DojoState, today: NaiveDate) -> Vec<TreeRow> {
    problems
        .problems
        .iter()
        .filter(|p| state.is_due(&p.slug, today))
        .map(|p| problem_row(p, state, today))
        .collect()
}

/// The flattened tree: one group header per category (file order) with its
/// problems when expanded — Easy first, then Medium, then Hard, NeetCode
/// order inside each band — then the System Design group. `redo_only`
/// shows only due problems, forces groups open and hides empty ones.
pub fn tree_rows(
    problems: &Problems,
    plan: &Plan,
    state: &DojoState,
    redo_only: bool,
    today: NaiveDate,
) -> Vec<TreeRow> {
    let mut rows = Vec::new();
    for key in problems.categories() {
        let mut in_cat = problems.in_category(&key);
        in_cat.sort_by_key(|p| difficulty_rank(&p.difficulty));
        let slugs: Vec<&str> = in_cat.iter().map(|p| p.slug.as_str()).collect();
        let children: Vec<TreeRow> = in_cat
            .iter()
            .filter(|p| !redo_only || state.is_due(&p.slug, today))
            .map(|p| problem_row(p, state, today))
            .collect();
        if redo_only && children.is_empty() {
            continue;
        }
        let expanded = redo_only || !state.collapsed.contains(&key);
        rows.push(TreeRow::Group {
            label: category_label(&key),
            key,
            done: state.done_count(&slugs),
            total: slugs.len(),
            expanded,
        });
        if expanded {
            rows.extend(children);
        }
    }
    if !redo_only && !plan.sd_cases.is_empty() {
        let is_done = |key: &str| {
            state
                .attempts
                .iter()
                .any(|a| a.kind == SessionKind::Sd && a.slug == key)
        };
        let expanded = !state.collapsed.iter().any(|c| c == SD_GROUP_KEY);
        rows.push(TreeRow::SdGroup {
            done: plan.sd_cases.iter().filter(|c| is_done(&c.key)).count(),
            total: plan.sd_cases.len(),
            expanded,
        });
        if expanded {
            for case in &plan.sd_cases {
                rows.push(TreeRow::SdCase {
                    key: case.key.clone(),
                    label: case.label.clone(),
                    done: is_done(&case.key),
                });
            }
        }
    }
    rows
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DojoHeader {
    pub overall_done: usize,
    pub overall_total: usize,
    pub streak: u32,
    pub redo_due: usize,
}

pub fn header(problems: &Problems, state: &DojoState, today: NaiveDate) -> DojoHeader {
    let all: Vec<&str> = problems.problems.iter().map(|p| p.slug.as_str()).collect();
    DojoHeader {
        overall_done: state.done_count(&all),
        overall_total: all.len(),
        streak: state.streak(today),
        redo_due: due_rows(problems, state, today).len(),
    }
}

/// Right-dock view of one problem.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProblemView {
    pub id: u32,
    pub slug: String,
    pub title: String,
    pub difficulty: String,
    pub category: String,
    pub language: String,
    pub glyph: RowGlyph,
    pub status_line: String,
    /// Styled statement lines (blank = paragraph gap); the renderer wraps them.
    pub statement: Vec<Line>,
    pub hints: Vec<String>,
    pub loading: bool,
    pub error: Option<String>,
}

pub fn problem_view(
    p: &Problem,
    state: &DojoState,
    cache: Option<&LeetCodeProblemCache>,
    language: &str,
    loading: bool,
    error: Option<String>,
    today: NaiveDate,
) -> ProblemView {
    let (glyph, trailing) = status_of(state, &p.slug, today);
    let attempts = state.attempts.iter().filter(|a| a.slug == p.slug).count();
    let status_line = match glyph {
        RowGlyph::Done => format!(
            "solved · best {trailing} · {attempts} attempt{}",
            if attempts == 1 { "" } else { "s" }
        ),
        RowGlyph::RedoDue => format!("redo due · {attempts} attempts"),
        RowGlyph::RedoLater => format!("redo on {trailing} · {attempts} attempts"),
        RowGlyph::Todo => "not attempted".to_string(),
    };
    let statement = cache
        .map(|c| html_to_lines(&c.statement))
        .unwrap_or_default();
    ProblemView {
        id: p.id,
        slug: p.slug.clone(),
        title: p.title.clone(),
        difficulty: p.difficulty.clone(),
        category: category_label(&p.category),
        language: language.to_string(),
        glyph,
        status_line,
        statement,
        hints: cache.map(|c| c.hints.clone()).unwrap_or_default(),
        loading,
        error,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SdView {
    pub key: String,
    pub label: String,
    pub topic: String,
    pub done: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PanelContent {
    Problem(ProblemView),
    Sd(SdView),
    /// Nothing selected (or a group header): one hint line.
    Empty(String),
}

/// Clock line while a session runs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DojoSessionView {
    pub title: String,
    pub phase: String,
    pub phase_index: usize,
    pub remaining: String,
    pub remaining_s: u64,
    pub kind: SessionKind,
    pub expired: bool,
}

/// One clickable keycap chip in the Problem tab footer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FooterAction {
    /// Stable id the app maps back to a `Command` (`start`, `hints`…).
    pub id: &'static str,
    pub key: &'static str,
    pub label: &'static str,
}

/// Footer chips for the current panel content / session state.
pub fn footer_actions(content: &PanelContent, session_running: bool) -> Vec<FooterAction> {
    let a = |id, key, label| FooterAction { id, key, label };
    match content {
        PanelContent::Empty(_) => vec![
            a("start", "↵", "start"),
            a("language", "c", "language"),
            a("folder", "w", "folder"),
            a("notebook", "n", "notebook"),
            a("editor", "esc", "editor"),
        ],
        PanelContent::Sd(_) if session_running => vec![
            a("giveup", "x", "finish"),
            a("interviewer", "i", "interviewer"),
            a("editor", "esc", "editor"),
        ],
        PanelContent::Sd(_) => vec![
            a("start", "↵", "start"),
            a("interviewer", "i", "interviewer"),
            a("editor", "esc", "editor"),
        ],
        PanelContent::Problem(_) if session_running => vec![
            a("start", "↵", "back to code"),
            a("giveup", "x", "give up"),
            a("hints", "?", "hints"),
            a("interviewer", "i", "interviewer"),
            a("editor", "esc", "editor"),
        ],
        PanelContent::Problem(_) => vec![
            a("start", "↵", "start"),
            a("hints", "?", "hints"),
            a("language", "c", "language"),
            a("notebook", "n", "notebook"),
            a("interviewer", "i", "interviewer"),
            a("editor", "esc", "editor"),
        ],
    }
}

/// Everything the renderer needs for one frame of the Problem tab.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProblemPanelModel {
    pub header: DojoHeader,
    pub content: PanelContent,
    pub session: Option<DojoSessionView>,
    pub show_hints: bool,
    pub scroll: usize,
    pub focused: bool,
    pub actions: Vec<FooterAction>,
    /// Chip under the pointer (drawn raised).
    pub hovered_action: Option<&'static str>,
    /// Chip whose action just fired (drawn pressed for a moment).
    pub flashed_action: Option<&'static str>,
}

/// First due redo, else the first problem never attempted.
pub fn suggested_next(problems: &Problems, state: &DojoState, today: NaiveDate) -> Option<TreeRow> {
    if let Some(row) = due_rows(problems, state, today).into_iter().next() {
        return Some(row);
    }
    problems
        .problems
        .iter()
        .find(|p| state.status_of(&p.slug) == Status::Todo)
        .map(|p| problem_row(p, state, today))
}

/// Welcome-card text: (title, subtitle). Suggests; never assigns.
pub fn welcome_card(problems: &Problems, state: &DojoState, today: NaiveDate) -> (String, String) {
    let h = header(problems, state, today);
    let title = match suggested_next(problems, state, today) {
        Some(TreeRow::Problem { id, title, .. }) if h.redo_due > 0 => {
            format!("↻ {} redo due · #{id} {title}", h.redo_due)
        }
        Some(TreeRow::Problem { id, title, .. }) => format!("○ #{id} {title}"),
        _ => "All 150 solved — add more to neetcode150.toml".to_string(),
    };
    let sub = format!(
        "{}/{} solved · streak {}",
        h.overall_done, h.overall_total, h.streak
    );
    (title, sub)
}

/// Greedy word wrap; blank lines preserved; over-long tokens hard-broken.
pub fn wrap_text(text: &str, max_chars: usize) -> Vec<String> {
    let max = max_chars.max(1);
    let mut out = Vec::new();
    if text.trim().is_empty() {
        return out;
    }
    for line in text.lines() {
        if line.trim().is_empty() {
            out.push(String::new());
            continue;
        }
        let mut current = String::new();
        for word in line.split_whitespace() {
            let mut word: Vec<char> = word.chars().collect();
            while word.len() > max {
                if !current.is_empty() {
                    out.push(std::mem::take(&mut current));
                }
                out.push(word.drain(..max).collect());
            }
            let word: String = word.into_iter().collect();
            let needed = if current.is_empty() {
                word.chars().count()
            } else {
                current.chars().count() + 1 + word.chars().count()
            };
            if needed > max && !current.is_empty() {
                out.push(std::mem::take(&mut current));
            }
            if !current.is_empty() {
                current.push(' ');
            }
            current.push_str(&word);
        }
        if !current.is_empty() {
            out.push(current);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dojo::{
        plan::Plan,
        problems::Problems,
        state::{Attempt, DojoState, Outcome, parse_date},
    };

    fn fixture() -> (Plan, Problems, DojoState, NaiveDate) {
        (
            Plan::bundled(),
            Problems::bundled(),
            DojoState::default(),
            parse_date("2026-09-10").expect("d"),
        )
    }

    fn attempt(slug: &str, outcome: Outcome, elapsed: u64) -> Attempt {
        Attempt {
            slug: slug.into(),
            kind: SessionKind::Dsa,
            started_unix: 1_788_000_000,
            ended_unix: 1_788_000_000 + elapsed,
            outcome,
            elapsed_s: elapsed,
        }
    }

    #[test]
    fn tree_lists_categories_in_file_order_with_problems_under_open_groups() {
        let (plan, problems, mut state, today) = fixture();
        state.record_attempt(attempt("two-sum", Outcome::Pass, 760), today);
        state.collapsed = vec!["two_pointers".into()];
        let rows = tree_rows(&problems, &plan, &state, false, today);
        assert!(matches!(
            &rows[0],
            TreeRow::Group { key, label, done: 1, total: 9, expanded: true }
                if key == "arrays_hashing" && label == "Arrays & Hashing"
        ));
        assert!(
            matches!(&rows[1], TreeRow::Problem { id: 217, .. }),
            "Easy band first, NeetCode order inside it"
        );
        let ranks: Vec<u8> = rows[1..10]
            .iter()
            .filter_map(|r| match r {
                TreeRow::Problem { difficulty, .. } => Some(difficulty_rank(difficulty)),
                _ => None,
            })
            .collect();
        assert!(
            ranks.windows(2).all(|w| w[0] <= w[1]),
            "sorted by difficulty: {ranks:?}"
        );
        let two_sum = rows
            .iter()
            .find(|r| matches!(r, TreeRow::Problem { slug, .. } if slug == "two-sum"))
            .expect("two-sum");
        assert!(matches!(
            two_sum,
            TreeRow::Problem { glyph: RowGlyph::Done, trailing, difficulty, .. }
                if trailing == "12:40" && difficulty == "easy"
        ));
        let tp = rows
            .iter()
            .position(|r| matches!(r, TreeRow::Group { key, .. } if key == "two_pointers"))
            .expect("two_pointers header");
        assert!(matches!(
            &rows[tp],
            TreeRow::Group {
                expanded: false,
                ..
            }
        ));
        assert!(
            rows[tp + 1].is_group(),
            "collapsed group hides its problems"
        );
        assert!(matches!(rows.last(), Some(TreeRow::SdCase { .. })));
        let sd = rows
            .iter()
            .position(|r| matches!(r, TreeRow::SdGroup { total: 8, .. }))
            .expect("sd group");
        assert_eq!(rows.len() - sd, 9, "8 SD cases follow their header");
        let groups = rows.iter().filter(|r| r.is_group()).count();
        assert_eq!(groups, 19, "18 categories + System Design");
        let problems_shown = rows
            .iter()
            .filter(|r| matches!(r, TreeRow::Problem { .. }))
            .count();
        assert_eq!(problems_shown, 150 - 5, "two_pointers (5) folded");
    }

    #[test]
    fn redo_only_shows_due_problems_and_forces_groups_open() {
        let (plan, problems, mut state, today) = fixture();
        state.record_attempt(
            attempt("valid-anagram", Outcome::Timeout, 1500),
            parse_date("2026-09-01").expect("d"),
        );
        state.record_attempt(attempt("min-stack", Outcome::Giveup, 100), today);
        state.collapsed = vec!["arrays_hashing".into()];
        let rows = tree_rows(&problems, &plan, &state, true, today);
        assert_eq!(rows.len(), 2);
        assert!(
            matches!(&rows[0], TreeRow::Group { key, expanded: true, .. } if key == "arrays_hashing")
        );
        assert!(matches!(
            &rows[1],
            TreeRow::Problem { slug, glyph: RowGlyph::RedoDue, trailing, .. }
                if slug == "valid-anagram" && trailing == "redo"
        ));
        let all = tree_rows(&problems, &plan, &state, false, today);
        let later = all
            .iter()
            .find(|r| matches!(r, TreeRow::Problem { slug, .. } if slug == "min-stack"))
            .expect("min-stack");
        assert!(matches!(
            later,
            TreeRow::Problem { glyph: RowGlyph::RedoLater, trailing, .. } if trailing == "13/09"
        ));
        assert_eq!(header(&problems, &state, today).redo_due, 1);
    }

    #[test]
    fn problem_view_status_lines_and_cache_fields() {
        let (_, problems, mut state, today) = fixture();
        let p = problems.by_slug("two-sum").expect("p");
        let v = problem_view(p, &state, None, "javascript", true, None, today);
        assert_eq!(
            (v.id, v.category.as_str(), v.status_line.as_str(), v.loading),
            (1, "Arrays & Hashing", "not attempted", true)
        );
        assert!(v.statement.is_empty());
        state.record_attempt(attempt("two-sum", Outcome::Fail, 900), today);
        state.record_attempt(attempt("two-sum", Outcome::Pass, 760), today);
        let cache = LeetCodeProblemCache {
            id: "1".into(),
            slug: "two-sum".into(),
            title: "Two Sum".into(),
            statement: "<p>Given <code>nums</code>.</p><p>Return indices.</p>".into(),
            function_name: "twoSum".into(),
            parameters: vec![],
            cases: vec![crate::runner::leetcode_cache::CachedCase {
                input: "{\"nums\":[2,7]}".into(),
                expected: "[0,1]".into(),
            }],
            hints: vec!["Use a hash map.".into()],
        };
        let v = problem_view(p, &state, Some(&cache), "javascript", false, None, today);
        assert_eq!(v.status_line, "redo on 24/09 · 2 attempts");
        let text: Vec<String> = v
            .statement
            .iter()
            .map(crate::dojo::statement::line_text)
            .collect();
        assert_eq!(text, vec!["Given nums.", "", "Return indices."]);
        assert_eq!(v.hints, vec!["Use a hash map.".to_string()]);
        assert_eq!(
            footer_actions(&PanelContent::Problem(v.clone()), false)
                .iter()
                .map(|a| a.id)
                .collect::<Vec<_>>(),
            vec![
                "start",
                "hints",
                "language",
                "notebook",
                "interviewer",
                "editor"
            ]
        );
        assert_eq!(
            footer_actions(&PanelContent::Problem(v), true)[0].label,
            "back to code"
        );
        state.record_attempt(attempt("two-sum", Outcome::Pass, 500), today);
        let v = problem_view(p, &state, Some(&cache), "javascript", false, None, today);
        assert_eq!(v.status_line, "solved · best 08:20 · 3 attempts");
        assert_eq!(difficulty_letter("easy"), 'E');
        assert_eq!(difficulty_label("hard"), "Hard");
    }

    #[test]
    fn header_suggestion_and_welcome_card() {
        let (_, problems, mut state, today) = fixture();
        let h = header(&problems, &state, today);
        assert_eq!(
            (h.overall_done, h.overall_total, h.streak, h.redo_due),
            (0, 150, 0, 0)
        );
        assert!(matches!(
            suggested_next(&problems, &state, today),
            Some(TreeRow::Problem { slug, .. }) if slug == "contains-duplicate"
        ));
        let (title, sub) = welcome_card(&problems, &state, today);
        assert_eq!(title, "○ #217 Contains Duplicate");
        assert_eq!(sub, "0/150 solved · streak 0");
        state.record_attempt(
            attempt("min-stack", Outcome::Timeout, 1500),
            parse_date("2026-09-01").expect("d"),
        );
        assert!(matches!(
            suggested_next(&problems, &state, today),
            Some(TreeRow::Problem { slug, .. }) if slug == "min-stack"
        ));
        let (title, _) = welcome_card(&problems, &state, today);
        assert_eq!(title, "↻ 1 redo due · #155 Min Stack");
    }

    #[test]
    fn wraps_words_and_hard_breaks_long_tokens() {
        assert_eq!(wrap_text("aaa bbb ccc", 7), vec!["aaa bbb", "ccc"]);
        assert_eq!(wrap_text("x\n\ny", 10), vec!["x", "", "y"]);
        assert_eq!(wrap_text("abcdefghij", 4), vec!["abcd", "efgh", "ij"]);
        assert_eq!(wrap_text("", 4), Vec::<String>::new());
    }
}
