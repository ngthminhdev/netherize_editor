//! Persistent Dojo state (`~/.config/netherize/dojo.toml`): attempts, per-problem
//! status + spaced-redo dates, the active session. Derived values (streak,
//! progress, due list) are computed, never stored.
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use chrono::{Days, NaiveDate, TimeZone};
use serde::{Deserialize, Serialize};

use super::session::SessionKind;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Outcome {
    Pass,
    Fail,
    Timeout,
    Giveup,
}

impl Outcome {
    pub fn label(self) -> &'static str {
        match self {
            Self::Pass => "pass",
            Self::Fail => "fail",
            Self::Timeout => "timeout",
            Self::Giveup => "giveup",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Status {
    #[default]
    Todo,
    Redo,
    Done,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Attempt {
    pub slug: String,
    #[serde(default)]
    pub kind: SessionKind,
    pub started_unix: u64,
    pub ended_unix: u64,
    pub outcome: Outcome,
    pub elapsed_s: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct ProblemProgress {
    #[serde(default)]
    pub status: Status,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub redo_at: Option<String>,
    #[serde(default)]
    pub passes: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActiveSession {
    pub kind: SessionKind,
    pub slug: String,
    #[serde(default)]
    pub title: String,
    pub started_unix: u64,
    pub budget_s: u64,
    pub file: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct DojoState {
    /// Folder holding one sub-folder per attempted problem (user-chosen via
    /// the folder dialog; machine-local, hence state not plan).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workspace: Option<String>,
    /// LeetCode template key (`javascript`, `python`…); `None` until picked.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
    /// Collapsed category keys in the left tree.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub collapsed: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_session: Option<ActiveSession>,
    #[serde(default, rename = "attempt")]
    pub attempts: Vec<Attempt>,
    #[serde(default)]
    pub problem: BTreeMap<String, ProblemProgress>,
}

// ── dates ─────────────────────────────────────────────────────────────────────

pub fn today_local() -> NaiveDate {
    chrono::Local::now().date_naive()
}

pub fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

pub fn now_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| u64::try_from(d.as_millis()).unwrap_or(0))
        .unwrap_or(0)
}

pub fn date_str(date: NaiveDate) -> String {
    date.format("%Y-%m-%d").to_string()
}

pub fn parse_date(text: &str) -> Option<NaiveDate> {
    NaiveDate::parse_from_str(text.trim(), "%Y-%m-%d").ok()
}

pub fn unix_to_local_date(secs: u64) -> Option<NaiveDate> {
    chrono::Local
        .timestamp_opt(i64::try_from(secs).ok()?, 0)
        .single()
        .map(|dt| dt.date_naive())
}

fn plus_days(date: NaiveDate, days: u64) -> NaiveDate {
    date.checked_add_days(Days::new(days)).unwrap_or(date)
}

// ── spaced repetition (spec §8) ───────────────────────────────────────────────

pub fn apply_outcome(p: &mut ProblemProgress, outcome: Outcome, today: NaiveDate) {
    match (p.status, outcome) {
        (Status::Done, _) => {}
        (Status::Todo, Outcome::Pass) => {
            p.status = Status::Done;
            p.passes = 1;
            p.redo_at = None;
        }
        (Status::Redo, Outcome::Pass) => {
            p.passes += 1;
            if p.passes >= 2 {
                p.status = Status::Done;
                p.redo_at = None;
            } else {
                p.redo_at = Some(date_str(plus_days(today, 14)));
            }
        }
        (Status::Todo | Status::Redo, _) => {
            p.status = Status::Redo;
            p.redo_at = Some(date_str(plus_days(today, 3)));
        }
    }
}

/// Consecutive days with a session, counted back from today (if today has one)
/// or yesterday. `dates` may be unsorted and contain duplicates.
pub fn streak_from_dates(dates: &[NaiveDate], today: NaiveDate) -> u32 {
    let mut days: Vec<NaiveDate> = dates.to_vec();
    days.sort_unstable();
    days.dedup();
    let mut cursor = if days.last() == Some(&today) {
        today
    } else {
        match today.checked_sub_days(Days::new(1)) {
            Some(y) => y,
            None => return 0,
        }
    };
    let mut streak = 0;
    while days.binary_search(&cursor).is_ok() {
        streak += 1;
        match cursor.checked_sub_days(Days::new(1)) {
            Some(prev) => cursor = prev,
            None => break,
        }
    }
    streak
}

impl DojoState {
    pub fn state_path() -> PathBuf {
        crate::config::paths::user_config_root().join("dojo.toml")
    }

    pub fn load(path: &Path) -> Self {
        match std::fs::read_to_string(path) {
            Ok(text) => toml::from_str(&text).unwrap_or_else(|err| {
                eprintln!(
                    "[dojo] {} unreadable, starting fresh: {err}",
                    path.display()
                );
                Self::default()
            }),
            Err(_) => Self::default(),
        }
    }

    pub fn save(&self, path: &Path) -> Result<(), String> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let text = toml::to_string_pretty(self).map_err(|e| e.to_string())?;
        crate::app::persistence::atomic_write(path, text).map_err(|e| e.to_string())
    }

    pub fn status_of(&self, slug: &str) -> Status {
        self.problem.get(slug).map(|p| p.status).unwrap_or_default()
    }

    pub fn progress_of(&self, slug: &str) -> ProblemProgress {
        self.problem.get(slug).cloned().unwrap_or_default()
    }

    pub fn record_attempt(&mut self, attempt: Attempt, today: NaiveDate) {
        let entry = self.problem.entry(attempt.slug.clone()).or_default();
        apply_outcome(entry, attempt.outcome, today);
        self.attempts.push(attempt);
    }

    pub fn is_due(&self, slug: &str, today: NaiveDate) -> bool {
        self.problem.get(slug).is_some_and(|p| {
            p.status == Status::Redo
                && p.redo_at
                    .as_deref()
                    .and_then(parse_date)
                    .is_some_and(|d| d <= today)
        })
    }

    /// Slugs due for redo, in BTreeMap (alphabetical) order; callers re-sort.
    pub fn redo_due_slugs(&self, today: NaiveDate) -> Vec<String> {
        self.problem
            .keys()
            .filter(|slug| self.is_due(slug, today))
            .cloned()
            .collect()
    }

    pub fn done_count(&self, slugs: &[&str]) -> usize {
        slugs
            .iter()
            .filter(|s| self.status_of(s) == Status::Done)
            .count()
    }

    pub fn streak(&self, today: NaiveDate) -> u32 {
        let dates: Vec<NaiveDate> = self
            .attempts
            .iter()
            .filter_map(|a| unix_to_local_date(a.ended_unix))
            .collect();
        streak_from_dates(&dates, today)
    }

    pub fn best_pass_secs(&self, slug: &str) -> Option<u64> {
        self.attempts
            .iter()
            .filter(|a| a.slug == slug && a.outcome == Outcome::Pass)
            .map(|a| a.elapsed_s)
            .min()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(s: &str) -> NaiveDate {
        parse_date(s).expect("date")
    }

    fn attempt(slug: &str, outcome: Outcome, ended: u64) -> Attempt {
        Attempt {
            slug: slug.to_string(),
            kind: SessionKind::Dsa,
            started_unix: ended - 600,
            ended_unix: ended,
            outcome,
            elapsed_s: 600,
        }
    }

    #[test]
    fn srs_table() {
        let today = d("2026-09-02");
        let mut p = ProblemProgress::default();
        apply_outcome(&mut p, Outcome::Pass, today);
        assert_eq!(
            (p.status, p.passes, p.redo_at.as_deref()),
            (Status::Done, 1, None)
        );

        let mut p = ProblemProgress::default();
        apply_outcome(&mut p, Outcome::Timeout, today);
        assert_eq!(
            (p.status, p.redo_at.as_deref()),
            (Status::Redo, Some("2026-09-05"))
        );
        apply_outcome(&mut p, Outcome::Pass, d("2026-09-05"));
        assert_eq!(
            (p.status, p.passes, p.redo_at.as_deref()),
            (Status::Redo, 1, Some("2026-09-19"))
        );
        apply_outcome(&mut p, Outcome::Giveup, d("2026-09-19"));
        assert_eq!(
            (p.status, p.redo_at.as_deref()),
            (Status::Redo, Some("2026-09-22"))
        );
        apply_outcome(&mut p, Outcome::Pass, d("2026-09-22"));
        assert_eq!(
            (p.status, p.passes, p.redo_at.as_deref()),
            (Status::Done, 2, None)
        );
        apply_outcome(&mut p, Outcome::Pass, d("2026-10-06"));
        assert_eq!(p.passes, 2, "done stays done, passes frozen");

        let mut p = ProblemProgress {
            status: Status::Done,
            redo_at: None,
            passes: 1,
        };
        apply_outcome(&mut p, Outcome::Fail, today);
        assert_eq!(p.status, Status::Done, "done stays done");
    }

    #[test]
    fn record_attempt_updates_progress_and_due_list() {
        let mut s = DojoState::default();
        let today = d("2026-09-02");
        s.record_attempt(attempt("two-sum", Outcome::Timeout, 1_788_400_000), today);
        assert_eq!(s.status_of("two-sum"), Status::Redo);
        assert!(!s.is_due("two-sum", today));
        assert!(s.is_due("two-sum", d("2026-09-05")));
        assert_eq!(
            s.redo_due_slugs(d("2026-09-06")),
            vec!["two-sum".to_string()]
        );
        assert_eq!(s.done_count(&["two-sum", "x"]), 0);
        s.record_attempt(attempt("x", Outcome::Pass, 1_788_400_100), today);
        assert_eq!(s.done_count(&["two-sum", "x"]), 1);
        assert_eq!(s.best_pass_secs("x"), Some(600));
        assert_eq!(s.best_pass_secs("two-sum"), None);
    }

    #[test]
    fn streak_counts_back_from_today_or_yesterday() {
        let today = d("2026-09-10");
        let dates = [d("2026-09-07"), d("2026-09-08"), d("2026-09-09")];
        assert_eq!(
            streak_from_dates(&dates, today),
            3,
            "no session yet today → from yesterday"
        );
        let dates = [d("2026-09-08"), d("2026-09-09"), d("2026-09-10")];
        assert_eq!(streak_from_dates(&dates, today), 3);
        let dates = [d("2026-09-01"), d("2026-09-09"), d("2026-09-09")];
        assert_eq!(
            streak_from_dates(&dates, today),
            1,
            "gap breaks; duplicates ignored"
        );
        assert_eq!(streak_from_dates(&[d("2026-09-01")], today), 0);
        assert_eq!(streak_from_dates(&[], today), 0);
    }

    #[test]
    fn save_and_load_round_trip() {
        let dir = std::env::temp_dir().join(format!("dojo_state_{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("dir");
        let path = dir.join("dojo.toml");
        assert_eq!(
            DojoState::load(&path),
            DojoState::default(),
            "missing → default"
        );
        let mut s = DojoState {
            workspace: Some("/tmp/leetcode".to_string()),
            language: Some("javascript".to_string()),
            collapsed: vec!["trees".to_string()],
            ..Default::default()
        };
        s.active_session = Some(ActiveSession {
            kind: SessionKind::Dsa,
            slug: "two-sum".to_string(),
            title: "Two Sum".to_string(),
            started_unix: 10,
            budget_s: 1500,
            file: std::path::PathBuf::from("/tmp/solution.js"),
        });
        s.record_attempt(
            attempt("two-sum", Outcome::Fail, 1_788_400_000),
            d("2026-09-02"),
        );
        s.save(&path).expect("save");
        assert_eq!(DojoState::load(&path), s);
        std::fs::write(&path, "garbage [[").expect("write");
        assert_eq!(
            DojoState::load(&path),
            DojoState::default(),
            "broken → default"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
