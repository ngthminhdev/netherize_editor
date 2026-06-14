use std::collections::HashSet;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PythonEnvKind {
    Venv(String),
    Pyenv(String),
    Conda(String),
    Global,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PythonEnv {
    pub kind: PythonEnvKind,
    pub display_name: String,
    pub executable: PathBuf,
}

/// Interpreter path inside a venv/conda-style prefix directory (`<prefix>/bin/python`).
fn prefix_python(prefix: &Path) -> PathBuf {
    prefix.join("bin").join("python")
}

fn python_exists(path: &Path) -> bool {
    path.try_exists().unwrap_or(false)
}

fn home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME").map(PathBuf::from)
}

/// Discover Python interpreters available to the editor.
///
/// Scans, in priority order: workspace-local virtualenvs, pyenv versions
/// (honoring a project `.python-version`), conda/mamba environments, named
/// virtualenv pools (poetry / pipenv / virtualenvwrapper), and finally the
/// system interpreters resolvable through `PATH`. No interactive shell is
/// spawned — every probe is a direct filesystem lookup — so the scan is fast
/// and independent of the user's shell rc files.
pub async fn scan_python_environments(workspace_root: &Path) -> Vec<PythonEnv> {
    let mut envs: Vec<PythonEnv> = Vec::new();

    collect_workspace_venvs(workspace_root, &mut envs);
    collect_pyenv_versions(workspace_root, &mut envs);
    collect_conda_envs(&mut envs);
    collect_named_virtualenvs(&mut envs);
    collect_global_pythons(&mut envs);

    dedup_by_executable(envs)
}

/// Virtualenvs that live directly inside the project (`venv/`, `.venv/`, `env/`).
fn collect_workspace_venvs(workspace_root: &Path, envs: &mut Vec<PythonEnv>) {
    for venv_name in &["venv", ".venv", "env"] {
        let python_path = prefix_python(&workspace_root.join(venv_name));
        if python_exists(&python_path) {
            envs.push(PythonEnv {
                kind: PythonEnvKind::Venv((*venv_name).to_string()),
                display_name: format!("[venv] {}/bin/python", venv_name),
                executable: python_path,
            });
        }
    }
}

fn pyenv_root() -> Option<PathBuf> {
    if let Some(root) = std::env::var_os("PYENV_ROOT") {
        return Some(PathBuf::from(root));
    }
    home_dir().map(|home| home.join(".pyenv"))
}

/// pyenv-managed interpreters, read straight from `<root>/versions/`. A project
/// `.python-version` pins a preferred version, which is surfaced first.
fn collect_pyenv_versions(workspace_root: &Path, envs: &mut Vec<PythonEnv>) {
    let Some(root) = pyenv_root() else {
        return;
    };
    let Ok(entries) = std::fs::read_dir(root.join("versions")) else {
        return;
    };

    let pinned = std::fs::read_to_string(workspace_root.join(".python-version"))
        .ok()
        .and_then(|contents| contents.lines().next().map(|line| line.trim().to_string()))
        .filter(|line| !line.is_empty());

    let mut found: Vec<(String, PathBuf)> = Vec::new();
    for entry in entries.flatten() {
        let python_path = prefix_python(&entry.path());
        if python_exists(&python_path) {
            let version = entry.file_name().to_string_lossy().to_string();
            found.push((version, python_path));
        }
    }
    // Deterministic ordering, then bubble the pinned version (stable sort) to the top.
    found.sort_by(|a, b| a.0.cmp(&b.0));
    if let Some(pin) = &pinned {
        found.sort_by_key(|(version, _)| usize::from(version != pin));
    }

    for (version, python_path) in found {
        let tag = if pinned.as_deref() == Some(version.as_str()) {
            " (.python-version)"
        } else {
            ""
        };
        envs.push(PythonEnv {
            kind: PythonEnvKind::Pyenv(version.clone()),
            display_name: format!("[pyenv] {version}{tag}"),
            executable: python_path,
        });
    }
}

/// conda / miniconda / miniforge / mamba environments, including each install's
/// `base` env, its `envs/*`, and the currently-activated `$CONDA_PREFIX`.
fn collect_conda_envs(envs: &mut Vec<PythonEnv>) {
    let Some(home) = home_dir() else {
        return;
    };

    for root_name in &["miniconda3", "anaconda3", "miniforge3", "mambaforge"] {
        let root = home.join(root_name);

        let base_python = prefix_python(&root);
        if python_exists(&base_python) {
            envs.push(PythonEnv {
                kind: PythonEnvKind::Conda("base".to_string()),
                display_name: format!("[conda] base ({root_name})"),
                executable: base_python,
            });
        }

        if let Ok(entries) = std::fs::read_dir(root.join("envs")) {
            for entry in entries.flatten() {
                let python_path = prefix_python(&entry.path());
                if python_exists(&python_path) {
                    let name = entry.file_name().to_string_lossy().to_string();
                    envs.push(PythonEnv {
                        kind: PythonEnvKind::Conda(name.clone()),
                        display_name: format!("[conda] {name}"),
                        executable: python_path,
                    });
                }
            }
        }
    }

    if let Some(prefix) = std::env::var_os("CONDA_PREFIX") {
        let prefix = PathBuf::from(prefix);
        let python_path = prefix_python(&prefix);
        if python_exists(&python_path) {
            let name = prefix
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| "active".to_string());
            envs.push(PythonEnv {
                kind: PythonEnvKind::Conda(name.clone()),
                display_name: format!("[conda] {name} (active)"),
                executable: python_path,
            });
        }
    }
}

/// Virtualenv pools managed by poetry / pipenv / virtualenvwrapper, each of
/// which keeps one interpreter per subdirectory.
fn collect_named_virtualenvs(envs: &mut Vec<PythonEnv>) {
    let Some(home) = home_dir() else {
        return;
    };

    let mut pools: Vec<(&str, PathBuf)> = vec![
        ("virtualenvwrapper", home.join(".virtualenvs")),
        (
            "pipenv",
            home.join(".local").join("share").join("virtualenvs"),
        ),
        (
            "poetry",
            home.join(".cache").join("pypoetry").join("virtualenvs"),
        ),
        (
            "poetry",
            home.join("Library")
                .join("Caches")
                .join("pypoetry")
                .join("virtualenvs"),
        ),
    ];
    if let Some(workon) = std::env::var_os("WORKON_HOME") {
        pools.push(("virtualenvwrapper", PathBuf::from(workon)));
    }

    for (label, dir) in pools {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let python_path = prefix_python(&entry.path());
            if python_exists(&python_path) {
                let name = entry.file_name().to_string_lossy().to_string();
                envs.push(PythonEnv {
                    kind: PythonEnvKind::Venv(name.clone()),
                    display_name: format!("[{label}] {name}"),
                    executable: python_path,
                });
            }
        }
    }
}

/// System interpreters: the first `python3` and `python` resolvable on `PATH`.
fn collect_global_pythons(envs: &mut Vec<PythonEnv>) {
    let Some(path_var) = std::env::var_os("PATH") else {
        return;
    };
    let dirs: Vec<PathBuf> = std::env::split_paths(&path_var).collect();
    for exe in &["python3", "python"] {
        if let Some(candidate) = dirs
            .iter()
            .map(|dir| dir.join(exe))
            .find(|p| python_exists(p))
        {
            envs.push(PythonEnv {
                kind: PythonEnvKind::Global,
                display_name: format!("[global] {}", candidate.display()),
                executable: candidate,
            });
        }
    }
}

/// Drop entries that point at the same interpreter path. Earlier (more specific)
/// sources win, so a workspace venv is preferred over the system interpreter it
/// may resolve to.
fn dedup_by_executable(envs: Vec<PythonEnv>) -> Vec<PythonEnv> {
    let mut seen: HashSet<PathBuf> = HashSet::new();
    let mut out = Vec::with_capacity(envs.len());
    for env in envs {
        if seen.insert(env.executable.clone()) {
            out.push(env);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn block_on<F: std::future::Future>(fut: F) -> F::Output {
        tokio::runtime::Builder::new_current_thread()
            .build()
            .expect("build test runtime")
            .block_on(fut)
    }

    #[test]
    fn detects_workspace_venv_and_dedups() {
        let tmp = std::env::temp_dir().join(format!("nz_pyenv_scan_{}", std::process::id()));
        let venv_bin = tmp.join(".venv").join("bin");
        std::fs::create_dir_all(&venv_bin).expect("create venv bin");
        std::fs::write(venv_bin.join("python"), b"").expect("write fake python");

        let envs = block_on(scan_python_environments(&tmp));

        let venv_python = venv_bin.join("python");
        let matches: Vec<_> = envs
            .iter()
            .filter(|e| e.executable == venv_python)
            .collect();
        assert_eq!(matches.len(), 1, "venv interpreter listed exactly once");
        assert!(matches!(&matches[0].kind, PythonEnvKind::Venv(name) if name == ".venv"));

        let _ = std::fs::remove_dir_all(&tmp);
    }
}
