use std::path::{Path, PathBuf};
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LaunchJson {
    pub version: Option<String>,
    #[serde(default)]
    pub configurations: Vec<LaunchConfiguration>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LaunchConfiguration {
    pub name: String,
    #[serde(rename = "type")]
    pub config_type: String,
    pub request: String,
    #[serde(default)]
    pub program: Option<String>,
    #[serde(default)]
    pub args: Option<Vec<String>>,
    #[serde(default)]
    pub cwd: Option<String>,
    #[serde(default)]
    pub env: Option<std::collections::HashMap<String, String>>,
    #[serde(default)]
    pub tool_args: Option<Vec<String>>,
    #[serde(default)]
    pub device_id: Option<String>,
    #[serde(default)]
    pub flutter_mode: Option<String>,
    #[serde(default)]
    pub dart_path: Option<String>,
    #[serde(flatten)]
    pub extra: std::collections::HashMap<serde_json::Value, serde_json::Value>,
}

#[derive(Debug, Clone)]
pub struct ResolvedLaunchConfig {
    pub name: String,
    pub config_type: String,
    pub request: String,
    pub program: String,
    pub args: Vec<String>,
    pub cwd: Option<PathBuf>,
    pub env: std::collections::HashMap<String, String>,
    pub tool_args: Vec<String>,
    pub device_id: Option<String>,
}

impl LaunchConfiguration {
    pub fn resolve(&self, workspace_root: &Path) -> ResolvedLaunchConfig {
        let program = self.program.clone().unwrap_or_default();
        let cwd = self.cwd.as_ref().map(|c| {
            let p = PathBuf::from(c);
            if p.is_absolute() { p } else { workspace_root.join(p) }
        });
        ResolvedLaunchConfig {
            name: self.name.clone(),
            config_type: self.config_type.clone(),
            request: self.request.clone(),
            program,
            args: self.args.clone().unwrap_or_default(),
            cwd,
            env: self.env.clone().unwrap_or_default(),
            tool_args: self.tool_args.clone().unwrap_or_default(),
            device_id: self.device_id.clone(),
        }
    }
}

pub fn find_launch_json(workspace_root: &Path) -> Option<PathBuf> {
    let candidates = [
        workspace_root.join(".vscode").join("launch.json"),
        workspace_root.join(".zed").join("debug.json"),
        workspace_root.join(".cursor").join("launch.json"),
    ];
    candidates.into_iter().find(|p| p.exists())
}

pub fn load_launch_json(workspace_root: &Path) -> Option<LaunchJson> {
    let path = find_launch_json(workspace_root)?;
    let content = std::fs::read_to_string(&path).ok()?;
    // VS Code launch.json may contain comments (JSONC), strip them.
    let cleaned = strip_json_comments(&content);
    serde_json::from_str(&cleaned).ok()
}

fn strip_json_comments(json: &str) -> String {
    let mut result = String::with_capacity(json.len());
    let mut in_string = false;
    let mut escape_next = false;
    let chars: Vec<char> = json.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        let c = chars[i];

        if escape_next {
            result.push(c);
            escape_next = false;
            i += 1;
            continue;
        }

        if in_string {
            if c == '\\' {
                escape_next = true;
            } else if c == '"' {
                in_string = false;
            }
            result.push(c);
            i += 1;
            continue;
        }

        if c == '"' {
            in_string = true;
            result.push(c);
            i += 1;
            continue;
        }

        if c == '/' && i + 1 < chars.len() {
            if chars[i + 1] == '/' {
                // Line comment - skip to end of line
                while i < chars.len() && chars[i] != '\n' {
                    i += 1;
                }
                continue;
            } else if chars[i + 1] == '*' {
                // Block comment - skip to */
                i += 2;
                while i + 1 < chars.len() {
                    if chars[i] == '*' && chars[i + 1] == '/' {
                        i += 2;
                        break;
                    }
                    i += 1;
                }
                continue;
            }
        }

        result.push(c);
        i += 1;
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_flutter_launch_json() {
        let json = r#"{
            "version": "0.2.0",
            "configurations": [
                {
                    "name": "Flutter",
                    "type": "dart",
                    "request": "launch",
                    "program": "lib/main.dart",
                    "toolArgs": ["-d", "chrome"],
                    "deviceId": "chrome"
                }
            ]
        }"#;
        let config: LaunchJson = serde_json::from_str(json).unwrap();
        assert_eq!(config.configurations.len(), 1);
        assert_eq!(config.configurations[0].name, "Flutter");
        assert_eq!(config.configurations[0].config_type, "dart");
        assert_eq!(config.configurations[0].request, "launch");
        assert_eq!(
            config.configurations[0].program.as_deref(),
            Some("lib/main.dart")
        );
    }

    #[test]
    fn parse_rust_launch_json() {
        let json = r#"{
            "version": "0.2.0",
            "configurations": [
                {
                    "name": "Debug",
                    "type": "lldb",
                    "request": "launch",
                    "program": "${workspaceFolder}/target/debug/myapp"
                }
            ]
        }"#;
        let config: LaunchJson = serde_json::from_str(json).unwrap();
        assert_eq!(config.configurations[0].config_type, "lldb");
    }

    #[test]
    fn strip_single_line_comments() {
        let json = r#"{
            // comment
            "key": "value"
        }"#;
        let cleaned = strip_json_comments(json);
        let parsed: serde_json::Value = serde_json::from_str(&cleaned).unwrap();
        assert_eq!(parsed["key"], "value");
    }

    #[test]
    fn strip_block_comments() {
        let json = r#"{
            /* block
               comment */
            "key": "value"
        }"#;
        let cleaned = strip_json_comments(json);
        let parsed: serde_json::Value = serde_json::from_str(&cleaned).unwrap();
        assert_eq!(parsed["key"], "value");
    }

    #[test]
    fn strip_comments_preserves_strings() {
        let json = r#"{
            "url": "http://example.com//not-a-comment"
        }"#;
        let cleaned = strip_json_comments(json);
        let parsed: serde_json::Value = serde_json::from_str(&cleaned).unwrap();
        assert_eq!(parsed["url"], "http://example.com//not-a-comment");
    }

    #[test]
    fn resolve_config_with_relative_cwd() {
        let config = LaunchConfiguration {
            name: "Test".to_string(),
            config_type: "dart".to_string(),
            request: "launch".to_string(),
            program: Some("lib/main.dart".to_string()),
            args: None,
            cwd: Some("my_project".to_string()),
            env: None,
            tool_args: None,
            device_id: None,
            flutter_mode: None,
            dart_path: None,
            extra: std::collections::HashMap::new(),
        };
        let resolved = config.resolve(Path::new("/workspace"));
        assert_eq!(resolved.cwd, Some(PathBuf::from("/workspace/my_project")));
    }

    #[test]
    fn multiple_configurations() {
        let json = r#"{
            "version": "0.2.0",
            "configurations": [
                { "name": "App", "type": "dart", "request": "launch", "program": "lib/main.dart" },
                { "name": "Tests", "type": "dart", "request": "launch", "program": "test/" },
                { "name": "Profile", "type": "dart", "request": "launch", "program": "lib/main.dart", "flutterMode": "profile" }
            ]
        }"#;
        let config: LaunchJson = serde_json::from_str(json).unwrap();
        assert_eq!(config.configurations.len(), 3);
        assert_eq!(config.configurations[2].name, "Profile");
    }
}
