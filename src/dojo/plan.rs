//! Dojo plan: timer budgets + system-design cases. Categories come from the
//! problem list itself (`Problems::categories`), so there is no page/group
//! config any more.
use std::path::Path;

use serde::Deserialize;

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct SdCase {
    pub key: String,
    pub label: String,
    #[serde(default)]
    pub topic: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Plan {
    /// Notebook path override; `None` → `<workspace>/notes.md`.
    #[serde(default)]
    pub notebook: Option<String>,
    /// System-design outlines folder override; `None` → `<workspace>/sd`.
    #[serde(default)]
    pub sd_dir: Option<String>,
    #[serde(default = "default_dsa_minutes")]
    pub dsa_minutes: u32,
    #[serde(default = "default_phases")]
    pub dsa_phases: Vec<(String, u32)>,
    #[serde(default = "default_sd_minutes")]
    pub sd_minutes: u32,
    #[serde(default, rename = "sd_case")]
    pub sd_cases: Vec<SdCase>,
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

pub const BUNDLED_PLAN: &str = include_str!("../../config/dojo/plan.toml");

impl Plan {
    pub fn parse(text: &str) -> Result<Self, String> {
        toml::from_str(text).map_err(|err| format!("invalid plan: {err}"))
    }

    pub fn bundled() -> Self {
        Self::parse(BUNDLED_PLAN).unwrap_or_else(|err| {
            eprintln!("[dojo] bundled plan is broken: {err}");
            Self {
                notebook: None,
                sd_dir: None,
                dsa_minutes: default_dsa_minutes(),
                dsa_phases: default_phases(),
                sd_minutes: default_sd_minutes(),
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

    pub fn sd_case(&self, key: &str) -> Option<&SdCase> {
        self.sd_cases.iter().find(|c| c.key == key)
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

    #[test]
    fn bundled_plan_has_eight_cases_and_defaults() {
        let plan = Plan::bundled();
        assert_eq!(plan.sd_cases.len(), 8);
        assert_eq!(plan.dsa_minutes, 25);
        assert_eq!(plan.dsa_phases.len(), 4);
        assert_eq!(plan.dsa_budget_s(), 25 * 60);
        assert_eq!(plan.sd_budget_s(), 45 * 60);
        assert_eq!(plan.notebook, None, "notebook lives in the workspace");
        assert_eq!(
            plan.sd_case("feed").map(|c| c.label.as_str()),
            Some("Social feed")
        );
    }

    #[test]
    fn missing_fields_take_defaults() {
        let plan = Plan::parse("dsa_minutes = 30\n").expect("parse");
        assert_eq!(plan.dsa_minutes, 30);
        assert_eq!(plan.sd_minutes, 45);
        assert_eq!(plan.dsa_phases[0], ("THINK".to_string(), 3));
        assert!(plan.sd_cases.is_empty());
        assert_eq!(Plan::parse("").expect("empty").dsa_minutes, 25);
    }
}
