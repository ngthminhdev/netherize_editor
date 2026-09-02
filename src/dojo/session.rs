//! Timed practice session (phases, budget). Pure: every fn takes `now`.
use serde::{Deserialize, Serialize};

use super::state::ActiveSession;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SessionKind {
    #[default]
    Dsa,
    Sd,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Phase {
    pub name: String,
    pub index: usize,
    pub remaining_s: u64,
}

pub fn total_secs(phases: &[(String, u32)]) -> u64 {
    phases.iter().map(|(_, m)| u64::from(*m) * 60).sum()
}

/// Phase containing `elapsed_s`, or `None` once the budget is spent.
pub fn phase_at(phases: &[(String, u32)], elapsed_s: u64) -> Option<Phase> {
    let mut start = 0u64;
    for (index, (name, minutes)) in phases.iter().enumerate() {
        let end = start + u64::from(*minutes) * 60;
        if elapsed_s < end {
            return Some(Phase {
                name: name.clone(),
                index,
                remaining_s: end - elapsed_s,
            });
        }
        start = end;
    }
    None
}

pub fn single_phase(name: &str, minutes: u32) -> Vec<(String, u32)> {
    vec![(name.to_string(), minutes)]
}

impl ActiveSession {
    pub fn elapsed_s(&self, now_unix: u64) -> u64 {
        now_unix.saturating_sub(self.started_unix)
    }

    pub fn remaining_s(&self, now_unix: u64) -> u64 {
        self.budget_s.saturating_sub(self.elapsed_s(now_unix))
    }

    pub fn is_expired(&self, now_unix: u64) -> bool {
        self.elapsed_s(now_unix) >= self.budget_s
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn phases() -> Vec<(String, u32)> {
        vec![
            ("THINK".into(), 3),
            ("CODE".into(), 15),
            ("TEST".into(), 5),
            ("REVIEW".into(), 2),
        ]
    }

    #[test]
    fn phase_boundaries() {
        let p = phases();
        assert_eq!(total_secs(&p), 1500);
        let at = |e| phase_at(&p, e).map(|ph| (ph.index, ph.name.clone(), ph.remaining_s));
        assert_eq!(at(0), Some((0, "THINK".into(), 180)));
        assert_eq!(at(179), Some((0, "THINK".into(), 1)));
        assert_eq!(at(180), Some((1, "CODE".into(), 900)));
        assert_eq!(at(1499), Some((3, "REVIEW".into(), 1)));
        assert_eq!(at(1500), None);
        assert_eq!(phase_at(&[], 0), None);
    }

    #[test]
    fn session_clock() {
        let s = ActiveSession {
            kind: SessionKind::Sd,
            slug: "wallet_v1".into(),
            title: String::new(),
            started_unix: 1000,
            budget_s: 60,
            file: "/tmp/x.md".into(),
        };
        assert_eq!(s.elapsed_s(999), 0, "clock skew clamps to 0");
        assert_eq!(s.elapsed_s(1030), 30);
        assert_eq!(s.remaining_s(1030), 30);
        assert!(!s.is_expired(1059));
        assert!(s.is_expired(1060));
        assert_eq!(single_phase("SD", 45), vec![("SD".to_string(), 45)]);
    }
}
