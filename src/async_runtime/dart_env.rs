use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DartEnvKind {
    FvmLocal(String),  // Workspace-local FVM configuration
    FvmGlobal(String), // Version cache under ~/.fvm/versions/
    Global,            // Native system version from PATH
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DartEnv {
    pub kind: DartEnvKind,
    pub display_name: String,
    pub executable: PathBuf,
}

pub async fn scan_dart_environments(workspace_root: &Path) -> Vec<DartEnv> {
    let mut envs = Vec::new();

    // 1. Local FVM symlink check (.fvm/flutter_sdk)
    // Prioritize FVM inside the workspace repository folder
    let local_fvm_dart = workspace_root
        .join(".fvm")
        .join("flutter_sdk")
        .join("bin")
        .join("cache")
        .join("dart-sdk")
        .join("bin")
        .join("dart");
    if local_fvm_dart.try_exists().unwrap_or(false) {
        envs.push(DartEnv {
            kind: DartEnvKind::FvmLocal("Local FVM".to_string()),
            display_name: "[fvm] Workspace SDK (.fvm/flutter_sdk)".to_string(),
            executable: local_fvm_dart,
        });
    }

    // 2. Global FVM cached versions (~/.fvm/versions/* and ~/fvm/versions/*)
    if let Ok(home) = std::env::var("HOME") {
        let mut found_versions = Vec::new();
        for dir_name in &[".fvm", "fvm"] {
            let global_fvm_dir = PathBuf::from(&home).join(dir_name).join("versions");
            if let Ok(mut entries) = std::fs::read_dir(&global_fvm_dir) {
                while let Some(Ok(entry)) = entries.next() {
                    if entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                        let version_name = entry.file_name().to_string_lossy().to_string();
                        let dart_bin = entry
                            .path()
                            .join("bin")
                            .join("cache")
                            .join("dart-sdk")
                            .join("bin")
                            .join("dart");
                        if dart_bin.try_exists().unwrap_or(false) {
                            found_versions.push((version_name, dart_bin));
                        }
                    }
                }
            }
        }
        // Deduplicate and sort by version name for consistent UI display
        found_versions.sort_by(|a, b| b.0.cmp(&a.0));
        found_versions.dedup_by(|a, b| a.0 == b.0);
        for (version_name, dart_bin) in found_versions {
            envs.push(DartEnv {
                kind: DartEnvKind::FvmGlobal(version_name.clone()),
                display_name: format!("[fvm] {}", version_name),
                executable: dart_bin,
            });
        }
    }

    // 3. Global Dart from PATH
    let find_global_dart = tokio::time::timeout(
        std::time::Duration::from_secs(3),
        tokio::process::Command::new("sh")
            .arg("-ilc")
            .arg("command -v dart 2>/dev/null")
            .output(),
    )
    .await;
    if let Ok(Ok(output)) = find_global_dart {
        if output.status.success() {
            let path_str = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !path_str.is_empty() {
                let path = PathBuf::from(path_str);
                envs.push(DartEnv {
                    kind: DartEnvKind::Global,
                    display_name: format!("[global] {}", path.display()),
                    executable: path,
                });
            }
        }
    }

    envs
}
