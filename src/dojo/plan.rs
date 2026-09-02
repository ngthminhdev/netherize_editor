//! Dojo plan: pattern groups (pages) + system-design cases + timer budgets.
use std::path::Path;

use serde::Deserialize;

use super::problems::{Problem, Problems};

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Group {
    pub key: String,
    pub label: String,
    pub categories: Vec<String>,
    #[serde(default)]
    pub note: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct SdCase {
    pub key: String,
    pub label: String,
    #[serde(default)]
    pub topic: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Plan {
    #[serde(default = "default_notebook")]
    pub notebook: String,
    #[serde(default = "default_sd_dir")]
    pub sd_dir: String,
    #[serde(default = "default_dsa_minutes")]
    pub dsa_minutes: u32,
    #[serde(default = "default_phases")]
    pub dsa_phases: Vec<(String, u32)>,
    #[serde(default = "default_sd_minutes")]
    pub sd_minutes: u32,
    #[serde(default, rename = "group")]
    pub groups: Vec<Group>,
    #[serde(default, rename = "sd_case")]
    pub sd_cases: Vec<SdCase>,
}

fn default_notebook() -> String {
    "~/Work/docs/interview-notes.md".to_string()
}

fn default_sd_dir() -> String {
    "~/Work/docs/sd".to_string()
}

fn default_dsa_minutes() -> u32 {
    25
}

fn default_sd_minutes() -> u32 {
    45
}

fn default_phases() -> Vec<(String, u32)> {
    vec![
        ("THINK".to_string(), 3),
        ("CODE".to_string(), 15),
        ("TEST".to_string(), 5),
        ("REVIEW".to_string(), 2),
    ]
}

/// One browsable page of the Dojo list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Page {
    Group(usize),
    Sd,
}

pub const BUNDLED_PLAN: &str = include_str!("../../config/dojo/plan.toml");

impl Plan {
    pub fn parse(text: &str) -> Result<Self, String> {
        let plan: Self = toml::from_str(text).map_err(|err| format!("invalid plan: {err}"))?;
        if plan.groups.is_empty() {
            return Err("plan has no [[group]]".to_string());
        }
        Ok(plan)
    }

    pub fn bundled() -> Self {
        Self::parse(BUNDLED_PLAN).unwrap_or_else(|err| {
            eprintln!("[dojo] bundled plan is broken: {err}");
            Self {
                notebook: default_notebook(),
                sd_dir: default_sd_dir(),
                dsa_minutes: default_dsa_minutes(),
                dsa_phases: default_phases(),
                sd_minutes: default_sd_minutes(),
                groups: Vec::new(),
                sd_cases: Vec::new(),
            }
        })
    }

    pub fn load(user_override: &Path) -> Self {
        match std::fs::read_to_string(user_override) {
            Ok(text) => Self::parse(&text).unwrap_or_else(|err| {
                eprintln!("[dojo] {}: {err}", user_override.display());
                Self::bundled()
            }),
            Err(_) => Self::bundled(),
        }
    }

    /// Groups in file order, then the SD page when there are cases.
    pub fn pages(&self) -> Vec<Page> {
        let mut pages: Vec<Page> = (0..self.groups.len()).map(Page::Group).collect();
        if !self.sd_cases.is_empty() {
            pages.push(Page::Sd);
        }
        pages
    }

    /// First group listing `category`; unmatched categories fall into the last group.
    pub fn group_for_category(&self, category: &str) -> Option<usize> {
        self.groups
            .iter()
            .position(|g| g.categories.iter().any(|c| c == category))
            .or_else(|| self.groups.len().checked_sub(1))
    }

    pub fn group_problems<'a>(&self, idx: usize, problems: &'a Problems) -> Vec<&'a Problem> {
        problems
            .problems
            .iter()
            .filter(|p| self.group_for_category(&p.category) == Some(idx))
            .collect()
    }

    pub fn page_key(&self, page: Page) -> String {
        match page {
            Page::Sd => "sd".to_string(),
            Page::Group(i) => self
                .groups
                .get(i)
                .map(|g| g.key.clone())
                .unwrap_or_default(),
        }
    }

    pub fn page_by_key(&self, key: &str) -> Option<Page> {
        if key == "sd" {
            return (!self.sd_cases.is_empty()).then_some(Page::Sd);
        }
        self.groups
            .iter()
            .position(|g| g.key == key)
            .map(Page::Group)
    }

    pub fn page_label(&self, page: Page) -> String {
        match page {
            Page::Sd => "System Design".to_string(),
            Page::Group(i) => self
                .groups
                .get(i)
                .map(|g| g.label.clone())
                .unwrap_or_default(),
        }
    }

    pub fn dsa_budget_s(&self) -> u64 {
        u64::from(self.dsa_minutes) * 60
    }

    pub fn sd_budget_s(&self) -> u64 {
        u64::from(self.sd_minutes) * 60
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dojo::problems::Problems;

    #[test]
    fn bundled_plan_has_seven_groups_eight_cases_and_defaults() {
        let plan = Plan::bundled();
        assert_eq!(plan.groups.len(), 7);
        assert_eq!(plan.sd_cases.len(), 8);
        assert_eq!(plan.dsa_minutes, 25);
        assert_eq!(plan.dsa_phases.len(), 4);
        assert_eq!(plan.dsa_budget_s(), 25 * 60);
        assert_eq!(plan.sd_budget_s(), 45 * 60);
        assert_eq!(plan.pages().len(), 8);
        assert_eq!(plan.pages()[7], Page::Sd);
    }

    #[test]
    fn missing_fields_take_defaults() {
        let plan = Plan::parse("[[group]]\nkey = \"a\"\nlabel = \"A\"\ncategories = [\"stack\"]\n")
            .expect("parse");
        assert_eq!(plan.dsa_minutes, 25);
        assert_eq!(plan.sd_minutes, 45);
        assert_eq!(plan.dsa_phases[0], ("THINK".to_string(), 3));
        assert_eq!(plan.notebook, "~/Work/docs/interview-notes.md");
        assert_eq!(
            plan.pages(),
            vec![Page::Group(0)],
            "no sd_cases → no Sd page"
        );
    }

    #[test]
    fn every_bundled_problem_lands_in_exactly_one_group() {
        let plan = Plan::bundled();
        let problems = Problems::bundled();
        let mut total = 0;
        for idx in 0..plan.groups.len() {
            total += plan.group_problems(idx, &problems).len();
        }
        assert_eq!(total, 150);
        assert_eq!(plan.group_problems(0, &problems).len(), 14);
        assert_eq!(plan.group_for_category("dp_2d"), Some(6));
        assert_eq!(
            plan.group_for_category("made_up"),
            Some(6),
            "unknown → last group"
        );
    }

    #[test]
    fn page_keys_round_trip() {
        let plan = Plan::bundled();
        assert_eq!(plan.page_key(Page::Sd), "sd");
        assert_eq!(plan.page_key(Page::Group(1)), "sliding_stack");
        assert_eq!(plan.page_by_key("sliding_stack"), Some(Page::Group(1)));
        assert_eq!(plan.page_by_key("sd"), Some(Page::Sd));
        assert_eq!(plan.page_by_key("zzz"), None);
        assert_eq!(plan.page_label(Page::Group(1)), "Sliding Window, Stack");
        assert_eq!(plan.page_label(Page::Sd), "System Design");
    }
}
