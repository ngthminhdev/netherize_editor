//! Serde structs matching the `codegraph` CLI `--json` output.
//!
//! Captured from codegraph v1.0.1:
//! - `callers <sym> --json` → `{ "symbol": "...", "callers": [ {name,kind,filePath,startLine} ] }`
//! - `callees <sym> --json` → `{ "symbol": "...", "callees": [ {...} ] }`
//! - `impact  <sym> --json --depth 2` → `{ "symbol","depth","nodeCount","edgeCount","affected":[{...}] }`

use serde::Deserialize;

/// One symbol entry as emitted by codegraph's `--json` array elements.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct CgSymbol {
    pub name: String,
    pub kind: String,
    #[serde(rename = "filePath")]
    pub file_path: String,
    #[serde(rename = "startLine")]
    pub start_line: u32,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CallersJson {
    pub symbol: String,
    #[serde(default)]
    pub callers: Vec<CgSymbol>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CalleesJson {
    pub symbol: String,
    #[serde(default)]
    pub callees: Vec<CgSymbol>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ImpactJson {
    pub symbol: String,
    #[serde(default)]
    pub affected: Vec<CgSymbol>,
}

pub fn parse_callers(json: &str) -> Result<CallersJson, String> {
    serde_json::from_str(json).map_err(|e| format!("callers json: {e}"))
}

pub fn parse_callees(json: &str) -> Result<CalleesJson, String> {
    serde_json::from_str(json).map_err(|e| format!("callees json: {e}"))
}

pub fn parse_impact(json: &str) -> Result<ImpactJson, String> {
    serde_json::from_str(json).map_err(|e| format!("impact json: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_callers_with_camelcase_fields() {
        let json = r#"{"symbol":"validate","callers":[
            {"name":"login","kind":"function","filePath":"src/auth.rs","startLine":58}]}"#;
        let parsed = parse_callers(json).unwrap();
        assert_eq!(parsed.symbol, "validate");
        assert_eq!(parsed.callers.len(), 1);
        assert_eq!(parsed.callers[0].name, "login");
        assert_eq!(parsed.callers[0].file_path, "src/auth.rs");
        assert_eq!(parsed.callers[0].start_line, 58);
    }

    #[test]
    fn parses_impact_affected_array() {
        let json = r#"{"symbol":"validate","depth":2,"nodeCount":2,"edgeCount":1,
            "affected":[{"name":"login","kind":"function","filePath":"src/auth.rs","startLine":58}]}"#;
        let parsed = parse_impact(json).unwrap();
        assert_eq!(parsed.affected.len(), 1);
        assert_eq!(parsed.affected[0].name, "login");
    }

    #[test]
    fn missing_array_defaults_empty() {
        let parsed = parse_callees(r#"{"symbol":"x"}"#).unwrap();
        assert!(parsed.callees.is_empty());
    }
}
