use std::path::PathBuf;

use crate::workspace::model::{WorkspaceModel, WorkspaceNodeType};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspaceMatch {
    pub absolute_path: PathBuf,
    pub relative_path: String,
    pub score: i64,
}

pub fn find_file_matches(model: &WorkspaceModel, query: &str, limit: usize) -> Vec<WorkspaceMatch> {
    let limit = limit.max(1);
    let query = query.trim().to_lowercase();

    let mut matches = Vec::new();
    for node in &model.nodes {
        if node.file_type != WorkspaceNodeType::File {
            continue;
        }

        let relative_path = node
            .path
            .strip_prefix(&model.root_path)
            .unwrap_or(node.path.as_path())
            .to_string_lossy()
            .replace('\\', "/");

        let relative_lower = relative_path.to_lowercase();
        let score = if query.is_empty() {
            // Query rỗng: trả danh sách file theo thứ tự path.
            Some(0)
        } else {
            score_candidate(&query, &relative_lower)
        };

        if let Some(score) = score {
            matches.push(WorkspaceMatch {
                absolute_path: node.path.clone(),
                relative_path,
                score,
            });
        }
    }

    matches.sort_by(|left, right| {
        right
            .score
            .cmp(&left.score)
            .then_with(|| left.relative_path.cmp(&right.relative_path))
    });
    matches.truncate(limit);
    matches
}

fn score_candidate(query: &str, candidate: &str) -> Option<i64> {
    if let Some(idx) = candidate.find(query) {
        // Bonus lớn cho match substring trực tiếp.
        let distance_penalty = idx as i64 * 8;
        let length_penalty = candidate.len().saturating_sub(query.len()) as i64;
        return Some(20_000 - distance_penalty - length_penalty);
    }

    // Fuzzy subsequence cơ bản: query chars phải xuất hiện theo thứ tự.
    let mut score = 0_i64;
    let mut previous_idx = None;
    let mut search_start = 0_usize;

    for q in query.chars() {
        let found = candidate[search_start..]
            .char_indices()
            .find(|(_, c)| *c == q)
            .map(|(offset, _)| search_start + offset)?;

        score += 120;

        if let Some(prev) = previous_idx {
            if found == prev + 1 {
                score += 45;
            } else {
                let gap = found.saturating_sub(prev + 1) as i64;
                score -= gap.min(40);
            }
        } else {
            score += 64 - (found.min(64) as i64);
        }

        previous_idx = Some(found);
        search_start = found + q.len_utf8();
    }

    Some(score)
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        time::{SystemTime, UNIX_EPOCH},
    };

    use crate::workspace::{
        fuzzy::find_file_matches,
        model::{WorkspaceIgnoreRules, WorkspaceModel},
    };

    fn unique_temp_dir(prefix: &str) -> std::path::PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock drift")
            .as_nanos();
        std::env::temp_dir().join(format!("netherize_fuzzy_{prefix}_{nanos}"))
    }

    #[test]
    fn fuzzy_matches_rank_substring_hits_higher() {
        let root = unique_temp_dir("rank");
        fs::create_dir_all(root.join("src/bin")).expect("create bin");
        fs::create_dir_all(root.join("docs")).expect("create docs");
        fs::write(root.join("src/main.rs"), "fn main() {}\n").expect("write main");
        fs::write(
            root.join("src/bin/phase8_workspace_probe.rs"),
            "fn probe() {}\n",
        )
        .expect("write probe");
        fs::write(root.join("docs/readme.md"), "hello\n").expect("write readme");

        let model = WorkspaceModel::load_with_rules(
            root.clone(),
            WorkspaceIgnoreRules::new(Vec::<String>::new()),
        )
        .expect("load workspace");
        let matches = find_file_matches(&model, "phase8", 10);

        assert!(!matches.is_empty());
        assert!(
            matches[0]
                .relative_path
                .contains("phase8_workspace_probe.rs")
        );

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn empty_query_returns_first_files_only() {
        let root = unique_temp_dir("empty");
        fs::create_dir_all(root.join("src")).expect("create src");
        fs::write(root.join("src/a.rs"), "a\n").expect("write a");
        fs::write(root.join("src/b.rs"), "b\n").expect("write b");

        let model = WorkspaceModel::load_with_rules(
            root.clone(),
            WorkspaceIgnoreRules::new(Vec::<String>::new()),
        )
        .expect("load workspace");
        let matches = find_file_matches(&model, "", 20);

        assert!(matches.len() >= 2);
        assert!(
            matches
                .iter()
                .any(|m| m.relative_path.ends_with("src/a.rs"))
        );
        assert!(
            matches
                .iter()
                .any(|m| m.relative_path.ends_with("src/b.rs"))
        );

        let _ = fs::remove_dir_all(root);
    }
}
