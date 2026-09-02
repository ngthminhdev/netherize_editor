//! Row/header model for the Dojo panel. Pure so ordering rules are testable
//! without the renderer.
use chrono::NaiveDate;

use super::{
    notebook::mm_ss,
    plan::{Page, Plan},
    problems::Problems,
    session::SessionKind,
    state::{DojoState, Status, parse_date},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RowGlyph {
    RedoDue,
    RedoLater,
    Todo,
    Done,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DojoRow {
    pub slug: String,
    pub id: u32,
    pub title: String,
    pub glyph: RowGlyph,
    pub trailing: String,
    pub kind: SessionKind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DojoHeader {
    pub page_label: String,
    pub page_index: usize,
    pub page_count: usize,
    pub page_done: usize,
    pub page_total: usize,
    pub overall_done: usize,
    pub overall_total: usize,
    pub streak: u32,
    pub redo_due: usize,
    pub note: String,
}

/// What the panel shows while a session runs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DojoSessionView {
    pub title: String,
    pub phase: String,
    pub phase_index: usize,
    pub remaining: String,
    pub remaining_s: u64,
    /// Paragraphs; the renderer wraps them to its own width.
    pub statement_lines: Vec<String>,
    pub approach: Option<String>,
    pub kind: SessionKind,
    pub expired: bool,
}

/// Everything the renderer needs for one frame of the Dojo tab.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DojoPanelModel {
    pub header: DojoHeader,
    pub rows: Vec<DojoRow>,
    pub selected: usize,
    pub scroll: usize,
    pub redo_only: bool,
    pub session: Option<DojoSessionView>,
    pub focused: bool,
}

fn dsa_row(
    problems: &Problems,
    state: &DojoState,
    slug: &str,
    today: NaiveDate,
) -> Option<DojoRow> {
    let p = problems.by_slug(slug)?;
    let progress = state.progress_of(slug);
    let (glyph, trailing) = match progress.status {
        Status::Done => (
            RowGlyph::Done,
            state
                .best_pass_secs(slug)
                .map(|s| format!("pass {}", mm_ss(s)))
                .unwrap_or_default(),
        ),
        Status::Redo if state.is_due(slug, today) => {
            (RowGlyph::RedoDue, "redo hôm nay".to_string())
        }
        Status::Redo => (
            RowGlyph::RedoLater,
            progress
                .redo_at
                .as_deref()
                .and_then(parse_date)
                .map(|d| format!("redo {}", d.format("%d/%m")))
                .unwrap_or_default(),
        ),
        Status::Todo => (RowGlyph::Todo, String::new()),
    };
    Some(DojoRow {
        slug: p.slug.clone(),
        id: p.id,
        title: p.title.clone(),
        glyph,
        trailing,
        kind: SessionKind::Dsa,
    })
}

fn due_rows(problems: &Problems, state: &DojoState, today: NaiveDate) -> Vec<DojoRow> {
    // Problems-file order, not BTreeMap order.
    problems
        .problems
        .iter()
        .filter(|p| state.is_due(&p.slug, today))
        .filter_map(|p| dsa_row(problems, state, &p.slug, today))
        .collect()
}

pub fn list_rows(
    plan: &Plan,
    problems: &Problems,
    state: &DojoState,
    page: Page,
    redo_only: bool,
    today: NaiveDate,
) -> Vec<DojoRow> {
    let mut rows = due_rows(problems, state, today);
    if redo_only {
        return rows;
    }
    match page {
        Page::Sd => {
            for case in &plan.sd_cases {
                let done = state
                    .attempts
                    .iter()
                    .any(|a| a.kind == SessionKind::Sd && a.slug == case.key);
                rows.push(DojoRow {
                    slug: case.key.clone(),
                    id: 0,
                    title: case.label.clone(),
                    glyph: if done { RowGlyph::Done } else { RowGlyph::Todo },
                    trailing: case.topic.clone(),
                    kind: SessionKind::Sd,
                });
            }
        }
        Page::Group(idx) => {
            let mut todo = Vec::new();
            let mut later = Vec::new();
            let mut done = Vec::new();
            for p in plan.group_problems(idx, problems) {
                if state.is_due(&p.slug, today) {
                    continue; // already in the due block
                }
                if let Some(row) = dsa_row(problems, state, &p.slug, today) {
                    match row.glyph {
                        RowGlyph::Todo => todo.push(row),
                        RowGlyph::RedoLater => later.push(row),
                        _ => done.push(row),
                    }
                }
            }
            rows.extend(todo);
            rows.extend(later);
            rows.extend(done);
        }
    }
    rows
}

pub fn header(
    plan: &Plan,
    problems: &Problems,
    state: &DojoState,
    page: Page,
    today: NaiveDate,
) -> DojoHeader {
    let pages = plan.pages();
    let page_index = pages
        .iter()
        .position(|p| *p == page)
        .map(|i| i + 1)
        .unwrap_or(0);
    let all: Vec<&str> = problems.problems.iter().map(|p| p.slug.as_str()).collect();
    let (page_done, page_total, note) = match page {
        Page::Sd => {
            let done = plan
                .sd_cases
                .iter()
                .filter(|c| {
                    state
                        .attempts
                        .iter()
                        .any(|a| a.kind == SessionKind::Sd && a.slug == c.key)
                })
                .count();
            (done, plan.sd_cases.len(), String::new())
        }
        Page::Group(idx) => {
            let slugs: Vec<&str> = plan
                .group_problems(idx, problems)
                .iter()
                .map(|p| p.slug.as_str())
                .collect();
            (
                state.done_count(&slugs),
                slugs.len(),
                plan.groups
                    .get(idx)
                    .map(|g| g.note.clone())
                    .unwrap_or_default(),
            )
        }
    };
    DojoHeader {
        page_label: plan.page_label(page),
        page_index,
        page_count: pages.len(),
        page_done,
        page_total,
        overall_done: state.done_count(&all),
        overall_total: all.len(),
        streak: state.streak(today),
        redo_due: due_rows(problems, state, today).len(),
        note,
    }
}

/// First due redo, else first todo on `page`, else first todo anywhere.
pub fn suggested_next(
    plan: &Plan,
    problems: &Problems,
    state: &DojoState,
    page: Page,
    today: NaiveDate,
) -> Option<DojoRow> {
    if let Some(row) = due_rows(problems, state, today).into_iter().next() {
        return Some(row);
    }
    let on_page = list_rows(plan, problems, state, page, false, today)
        .into_iter()
        .find(|r| r.glyph == RowGlyph::Todo);
    if on_page.is_some() {
        return on_page;
    }
    problems
        .problems
        .iter()
        .find(|p| state.status_of(&p.slug) == Status::Todo)
        .and_then(|p| dsa_row(problems, state, &p.slug, today))
}

/// `last_group` if it still exists, else the first group with a todo, else the first page.
pub fn initial_page(plan: &Plan, problems: &Problems, state: &DojoState) -> Page {
    if let Some(page) = state
        .last_group
        .as_deref()
        .and_then(|k| plan.page_by_key(k))
    {
        return page;
    }
    (0..plan.groups.len())
        .map(Page::Group)
        .find(|page| match page {
            Page::Group(idx) => plan
                .group_problems(*idx, problems)
                .iter()
                .any(|p| state.status_of(&p.slug) == Status::Todo),
            Page::Sd => false,
        })
        .unwrap_or(Page::Group(0))
}

/// Welcome-card text: (title, subtitle). Suggests; never assigns.
pub fn welcome_card(
    plan: &Plan,
    problems: &Problems,
    state: &DojoState,
    page: Page,
    today: NaiveDate,
) -> (String, String) {
    let h = header(plan, problems, state, page, today);
    let title = match suggested_next(plan, problems, state, page, today) {
        Some(row) if h.redo_due > 0 => {
            format!("↻ {} redo tới hạn · #{} {}", h.redo_due, row.id, row.title)
        }
        Some(row) => format!("○ #{} {}", row.id, row.title),
        None => "Hết bài — tự thêm vào neetcode150.toml".to_string(),
    };
    let sub = format!(
        "{} {}/{} · {}/{} · streak {}",
        h.page_label, h.page_done, h.page_total, h.overall_done, h.overall_total, h.streak
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
            approach: String::new(),
        }
    }

    #[test]
    fn rows_put_due_redos_first_then_todo_then_later_then_done() {
        let (plan, problems, mut state, today) = fixture();
        // "valid-anagram" (group 0) fails on 2026-09-01 → due since 09-04.
        state.record_attempt(
            attempt("valid-anagram", Outcome::Timeout, 1500),
            parse_date("2026-09-01").expect("d"),
        );
        // "min-stack" (group 1) fails today → redo later (09-13).
        state.record_attempt(attempt("min-stack", Outcome::Giveup, 100), today);
        // "two-sum" passes.
        state.record_attempt(attempt("two-sum", Outcome::Pass, 760), today);
        let rows = list_rows(&plan, &problems, &state, Page::Group(1), false, today);
        assert_eq!(
            rows[0].slug, "valid-anagram",
            "due redo from ANOTHER group leads"
        );
        assert!(matches!(rows[0].glyph, RowGlyph::RedoDue));
        assert_eq!(rows[0].trailing, "redo hôm nay");
        let later = rows
            .iter()
            .position(|r| r.slug == "min-stack")
            .expect("min-stack");
        let last_todo = rows
            .iter()
            .rposition(|r| matches!(r.glyph, RowGlyph::Todo))
            .expect("todo");
        assert!(later > last_todo, "redo-later sorts after todo");
        assert_eq!(rows[later].trailing, "redo 13/09");
        assert!(
            !rows.iter().any(|r| r.slug == "two-sum"),
            "other group's done row not on this page"
        );
        let rows0 = list_rows(&plan, &problems, &state, Page::Group(0), false, today);
        let done = rows0.last().expect("rows");
        assert_eq!(
            (done.slug.as_str(), done.trailing.as_str()),
            ("two-sum", "pass 12:40")
        );
        let only = list_rows(&plan, &problems, &state, Page::Group(0), true, today);
        assert_eq!(only.len(), 1);
    }

    #[test]
    fn sd_page_rows_and_header() {
        let (plan, problems, mut state, today) = fixture();
        state.attempts.push(Attempt {
            slug: "url_shortener".into(),
            kind: SessionKind::Sd,
            started_unix: 1,
            ended_unix: 2701,
            outcome: Outcome::Pass,
            elapsed_s: 2700,
            approach: String::new(),
        });
        let rows = list_rows(&plan, &problems, &state, Page::Sd, false, today);
        assert_eq!(rows.len(), 8);
        assert!(rows.iter().all(|r| r.kind == SessionKind::Sd && r.id == 0));
        let done = rows
            .iter()
            .find(|r| r.slug == "url_shortener")
            .expect("row");
        assert!(matches!(done.glyph, RowGlyph::Done));
        let h = header(&plan, &problems, &state, Page::Sd, today);
        assert_eq!(
            (
                h.page_label.as_str(),
                h.page_index,
                h.page_count,
                h.page_total
            ),
            ("System Design", 8, 8, 8)
        );
        assert_eq!(h.overall_total, 150);
    }

    #[test]
    fn header_and_suggestion() {
        let (plan, problems, mut state, today) = fixture();
        let h = header(&plan, &problems, &state, Page::Group(0), today);
        assert_eq!(
            (
                h.page_index,
                h.page_count,
                h.page_done,
                h.page_total,
                h.overall_done,
                h.overall_total,
                h.streak,
                h.redo_due
            ),
            (1, 8, 0, 14, 0, 150, 0, 0)
        );
        assert_eq!(
            suggested_next(&plan, &problems, &state, Page::Group(0), today).map(|r| r.slug),
            Some("contains-duplicate".into())
        );
        assert_eq!(initial_page(&plan, &problems, &state), Page::Group(0));
        state.last_group = Some("graph".into());
        assert_eq!(initial_page(&plan, &problems, &state), Page::Group(5));
        state.record_attempt(
            attempt("min-stack", Outcome::Timeout, 1500),
            parse_date("2026-09-01").expect("d"),
        );
        assert_eq!(
            suggested_next(&plan, &problems, &state, Page::Group(0), today).map(|r| r.slug),
            Some("min-stack".into()),
            "due redo wins"
        );
        assert_eq!(
            header(&plan, &problems, &state, Page::Group(0), today).redo_due,
            1
        );
    }

    #[test]
    fn welcome_card_text() {
        let (plan, problems, mut state, today) = fixture();
        let (title, sub) = welcome_card(&plan, &problems, &state, Page::Group(0), today);
        assert_eq!(title, "○ #217 Contains Duplicate");
        assert_eq!(sub, "Array/Hash Map, Two Pointers 0/14 · 0/150 · streak 0");
        state.record_attempt(
            attempt("min-stack", Outcome::Timeout, 1500),
            parse_date("2026-09-01").expect("d"),
        );
        let (title, _) = welcome_card(&plan, &problems, &state, Page::Group(0), today);
        assert_eq!(title, "↻ 1 redo tới hạn · #155 Min Stack");
    }

    #[test]
    fn wraps_words_and_hard_breaks_long_tokens() {
        assert_eq!(wrap_text("aaa bbb ccc", 7), vec!["aaa bbb", "ccc"]);
        assert_eq!(wrap_text("x\n\ny", 10), vec!["x", "", "y"]);
        assert_eq!(wrap_text("abcdefghij", 4), vec!["abcd", "efgh", "ij"]);
        assert_eq!(wrap_text("", 4), Vec::<String>::new());
    }
}
