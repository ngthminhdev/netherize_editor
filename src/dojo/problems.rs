//! NeetCode 150 problem list (`config/dojo/neetcode150.toml`, user override in
//! `~/.config/netherize/dojo/neetcode150.toml`).
use std::path::Path;

use serde::Deserialize;

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Problem {
    pub id: u32,
    pub slug: String,
    pub title: String,
    pub category: String,
    #[serde(default)]
    pub difficulty: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
pub struct Problems {
    #[serde(default, rename = "problem")]
    pub problems: Vec<Problem>,
}

pub const BUNDLED_PROBLEMS: &str = include_str!("../../config/dojo/neetcode150.toml");

impl Problems {
    pub fn parse(text: &str) -> Result<Self, String> {
        let parsed: Self =
            toml::from_str(text).map_err(|err| format!("invalid problem list: {err}"))?;
        if parsed.problems.is_empty() {
            return Err("problem list is empty".to_string());
        }
        Ok(parsed)
    }

    pub fn bundled() -> Self {
        Self::parse(BUNDLED_PROBLEMS).unwrap_or_else(|err| {
            eprintln!("[dojo] bundled problem list is broken: {err}");
            Self::default()
        })
    }

    /// A user override wins when it exists and parses; otherwise the bundled list.
    pub fn load(user_override: &Path) -> Self {
        match std::fs::read_to_string(user_override) {
            Ok(text) => Self::parse(&text).unwrap_or_else(|err| {
                eprintln!("[dojo] {}: {err}", user_override.display());
                Self::bundled()
            }),
            Err(_) => Self::bundled(),
        }
    }

    pub fn by_slug(&self, slug: &str) -> Option<&Problem> {
        self.problems.iter().find(|p| p.slug == slug)
    }

    pub fn len(&self) -> usize {
        self.problems.len()
    }

    pub fn is_empty(&self) -> bool {
        self.problems.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_a_problem_table() {
        let p = Problems::parse(
            "[[problem]]\nid = 1\nslug = \"two-sum\"\ntitle = \"Two Sum\"\ncategory = \"arrays_hashing\"\ndifficulty = \"easy\"\n",
        )
        .expect("parse");
        assert_eq!(p.len(), 1);
        assert_eq!(p.by_slug("two-sum").map(|x| x.id), Some(1));
        assert!(p.by_slug("nope").is_none());
    }

    #[test]
    fn empty_list_is_an_error() {
        assert!(Problems::parse("").is_err());
    }

    #[test]
    fn bundled_list_is_150_unique_and_categorised() {
        let p = Problems::bundled();
        assert_eq!(p.len(), 150);
        let mut slugs: Vec<&str> = p.problems.iter().map(|x| x.slug.as_str()).collect();
        slugs.sort_unstable();
        slugs.dedup();
        assert_eq!(slugs.len(), 150, "duplicate slug");
        assert!(p.problems.iter().all(|x| x.id > 0 && !x.title.is_empty()));
        let known = [
            "arrays_hashing",
            "two_pointers",
            "sliding_window",
            "stack",
            "binary_search",
            "linked_list",
            "trees",
            "tries",
            "heap",
            "backtracking",
            "graphs",
            "advanced_graphs",
            "dp_1d",
            "dp_2d",
            "greedy",
            "intervals",
            "math_geometry",
            "bit_manipulation",
        ];
        for x in &p.problems {
            assert!(
                known.contains(&x.category.as_str()),
                "unknown category {}",
                x.category
            );
        }
        let count = |c: &str| p.problems.iter().filter(|x| x.category == c).count();
        assert_eq!(count("arrays_hashing"), 9);
        assert_eq!(count("two_pointers"), 5);
        assert_eq!(count("trees"), 15);
        assert_eq!(count("tries"), 3);
    }

    #[test]
    fn load_prefers_a_valid_override_and_falls_back_otherwise() {
        let dir = std::env::temp_dir().join(format!("dojo_problems_{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("dir");
        let path = dir.join("neetcode150.toml");
        assert_eq!(Problems::load(&path).len(), 150, "missing file → bundled");
        std::fs::write(
            &path,
            "[[problem]]\nid = 7\nslug = \"x\"\ntitle = \"X\"\ncategory = \"stack\"\n",
        )
        .expect("write");
        assert_eq!(Problems::load(&path).len(), 1, "override wins");
        std::fs::write(&path, "not toml [[").expect("write");
        assert_eq!(
            Problems::load(&path).len(),
            150,
            "broken override → bundled"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
