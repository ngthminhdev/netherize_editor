//! The renderable graph model built from the three codegraph CLI payloads.
//!
//! Risk model (blast radius is an *estimate* — codegraph under-reports
//! trait/dynamic-dispatch and macro calls):
//! - Center (focal) → `Focal`.
//! - Every callee → `Safe` (the focal depends on them, not vice versa).
//! - Caller → `High` if it appears in `impact.affected` (matched by
//!   name + file + line), else `Medium`.

use crate::codegraph::cli_json::{CalleesJson, CallersJson, CgSymbol, ImpactJson};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NodeRole {
    Center,
    Caller,
    Callee,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RiskLevel {
    Focal,
    Safe,
    Medium,
    High,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GraphNode {
    pub name: String,
    pub kind: String,
    pub file_path: String,
    pub line: u32,
    pub role: NodeRole,
    pub risk: RiskLevel,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodeGraphModel {
    pub focal: GraphNode,
    pub callers: Vec<GraphNode>,
    pub callees: Vec<GraphNode>,
}

impl CodeGraphModel {
    pub fn is_empty(&self) -> bool {
        self.callers.is_empty() && self.callees.is_empty()
    }
}

fn ident(s: &CgSymbol) -> (String, String, u32) {
    (s.name.clone(), s.file_path.clone(), s.start_line)
}

/// Build the renderable model from the three CLI json payloads.
/// `focal_name`/`focal_file`/`focal_line` describe the symbol under the caret.
pub fn build_model(
    focal_name: &str,
    focal_file: &str,
    focal_line: u32,
    callers: &CallersJson,
    callees: &CalleesJson,
    impact: &ImpactJson,
) -> CodeGraphModel {
    use std::collections::HashSet;

    let affected: HashSet<(String, String, u32)> = impact.affected.iter().map(ident).collect();

    let mut seen: HashSet<(String, String, u32)> = HashSet::new();
    let dedup = |src: &[CgSymbol], seen: &mut HashSet<(String, String, u32)>| -> Vec<CgSymbol> {
        let mut out = Vec::new();
        for s in src {
            if seen.insert(ident(s)) {
                out.push(s.clone());
            }
        }
        out
    };

    let focal = GraphNode {
        name: focal_name.to_string(),
        kind: "focal".to_string(),
        file_path: focal_file.to_string(),
        line: focal_line,
        role: NodeRole::Center,
        risk: RiskLevel::Focal,
    };
    // Focal must never duplicate into the side columns.
    seen.insert((focal_name.to_string(), focal_file.to_string(), focal_line));

    let callers = dedup(&callers.callers, &mut seen)
        .into_iter()
        .map(|s| {
            // Blast-radius heuristic for a caller (it depends on the focal symbol):
            //  - in the impact set AND in a different file  -> High  (confirmed
            //    dependent whose breakage crosses a module boundary),
            //  - confirmed dependent in the focal's own file -> Medium (contained),
            //  - a caller the `impact` traversal did not surface -> Medium.
            let in_impact = affected.contains(&ident(&s));
            let cross_file = s.file_path != focal_file;
            let risk = match (in_impact, cross_file) {
                (true, true) => RiskLevel::High,
                _ => RiskLevel::Medium,
            };
            GraphNode {
                name: s.name,
                kind: s.kind,
                file_path: s.file_path,
                line: s.start_line,
                role: NodeRole::Caller,
                risk,
            }
        })
        .collect();

    let callees = dedup(&callees.callees, &mut seen)
        .into_iter()
        .map(|s| GraphNode {
            name: s.name,
            kind: s.kind,
            file_path: s.file_path,
            line: s.start_line,
            role: NodeRole::Callee,
            risk: RiskLevel::Safe,
        })
        .collect();

    CodeGraphModel {
        focal,
        callers,
        callees,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegraph::cli_json::{parse_callees, parse_callers, parse_impact};

    fn fixtures() -> (CallersJson, CalleesJson, ImpactJson) {
        let callers = parse_callers(
            r#"{"symbol":"validate","callers":[
            {"name":"login","kind":"function","filePath":"src/auth.rs","startLine":58},
            {"name":"check","kind":"method","filePath":"src/sess.rs","startLine":23}]}"#,
        )
        .unwrap();
        let callees = parse_callees(
            r#"{"symbol":"validate","callees":[
            {"name":"find","kind":"function","filePath":"src/db.rs","startLine":3},
            {"name":"find","kind":"function","filePath":"src/db.rs","startLine":3}]}"#,
        )
        .unwrap();
        let impact = parse_impact(
            r#"{"symbol":"validate","affected":[
            {"name":"login","kind":"function","filePath":"src/auth.rs","startLine":58}]}"#,
        )
        .unwrap();
        (callers, callees, impact)
    }

    #[test]
    fn caller_in_impact_is_high_else_medium() {
        let (cr, ce, im) = fixtures();
        let m = build_model("validate", "src/user.rs", 142, &cr, &ce, &im);
        assert_eq!(m.focal.risk, RiskLevel::Focal);
        assert_eq!(m.callers[0].name, "login");
        assert_eq!(m.callers[0].risk, RiskLevel::High); // in impact
        assert_eq!(m.callers[1].risk, RiskLevel::Medium); // not in impact
    }

    #[test]
    fn callees_are_safe_and_deduped() {
        let (cr, ce, im) = fixtures();
        let m = build_model("validate", "src/user.rs", 142, &cr, &ce, &im);
        assert_eq!(m.callees.len(), 1); // duplicate "find" collapsed
        assert_eq!(m.callees[0].risk, RiskLevel::Safe);
    }
}
