use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PythonEnvKind {
    Venv(String),
    Pyenv(String),
    Global,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PythonEnv {
    pub kind: PythonEnvKind,
    pub display_name: String,
    pub executable: PathBuf,
}

pub async fn scan_python_environments(workspace_root: &Path) -> Vec<PythonEnv> {
    let mut envs = Vec::new();

    for venv_name in &["venv", ".venv"] {
        let python_path = workspace_root.join(venv_name).join("bin").join("python");
        if python_path.try_exists().unwrap_or(false) {
            envs.push(PythonEnv {
                kind: PythonEnvKind::Venv(venv_name.to_string()),
                display_name: format!("[venv] {}/bin/python", venv_name),
                executable: python_path,
            });
        }
    }

    if let Ok(env) = std::env::var("HOME") {
        let timeout_result = tokio::time::timeout(
            std::time::Duration::from_secs(5),
            tokio::process::Command::new("sh")
                .arg("-ilc")
                .arg("pyenv versions --bare 2>/dev/null")
                .output(),
        )
        .await
        .ok()
        .and_then(|cmd_result| cmd_result.ok())
        .and_then(|output| {
            if output.status.success() {
                Some(output)
            } else {
                None
            }
        });
        if let Some(output) = timeout_result {
            let stdout = String::from_utf8_lossy(&output.stdout);
            for line in stdout.lines() {
                let version = line.trim();
                if version.is_empty() {
                    continue;
                }
                let pyenv_python = PathBuf::from(&env)
                    .join(".pyenv")
                    .join("versions")
                    .join(version)
                    .join("bin")
                    .join("python");
                if pyenv_python.try_exists().unwrap_or(false) {
                    envs.push(PythonEnv {
                        kind: PythonEnvKind::Pyenv(version.to_string()),
                        display_name: format!("[pyenv] {}", version),
                        executable: pyenv_python,
                    });
                }
            }
        }
    }

    let timeout_result = tokio::time::timeout(
        std::time::Duration::from_secs(3),
        tokio::process::Command::new("sh")
            .arg("-ilc")
            .arg("command -v python3 2>/dev/null || command -v python 2>/dev/null")
            .output(),
    )
    .await
    .ok()
    .and_then(|cmd_result| cmd_result.ok())
    .and_then(|output| {
        if output.status.success() {
            Some(output)
        } else {
            None
        }
    });
    if let Some(output) = timeout_result {
        let stdout = String::from_utf8_lossy(&output.stdout);
        let path = stdout.lines().next().map(|s| s.trim()).unwrap_or("");
        if !path.is_empty() && Path::new(path).try_exists().unwrap_or(false) {
            envs.push(PythonEnv {
                kind: PythonEnvKind::Global,
                display_name: format!("[global] {}", path),
                executable: PathBuf::from(path),
            });
        }
    }

    envs
}
